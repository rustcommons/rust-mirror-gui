//! Hidden worker mode: `rust-mirror-gui --worker COMMAND …`.
//!
//! The GUI re-runs its own executable in this mode for every operation, so a
//! hung or crashed operation can always be stopped or killed without taking the
//! window down. The worker only calls rust-mirror library APIs and reports on
//! stdout in the JSON-lines event protocol. It contains no mirroring logic.
//!
//! BRIDGE FOR rust-mirror 0.1.1: the library has no event or cancel API yet.
//! Until 0.2.0 provides one (docs/EVENTS.md), the worker:
//! - wraps the public `Source` trait to report artifact and byte events and to
//!   stop at the next transfer write when cancelled;
//! - reports error kind `cancelled` when it stopped the run, otherwise
//!   `unclassified` with the library's message;
//! - leaves the library's own progress text on stderr, which the GUI shows.
use crate::protocol::PROTOCOL_MAJOR;
use rust_mirror::{HttpsSource, Source, Transfer};
use serde_json::{json, Value};
use std::ffi::OsString;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// rust-mirror library version compiled into this executable (checked by a test
/// against Cargo.lock).
pub const LIBRARY_VERSION: &str = "0.1.1";
const HEARTBEAT: Duration = Duration::from_millis(1000);
const ORIGIN: &str = "https://static.rust-lang.org/";

/// Shared state between the operation, heartbeat and stdin threads.
struct Shared {
    out: Mutex<io::Stdout>,
    cancel: AtomicBool,
    started: Instant,
    /// Milliseconds since start of the last transfer write.
    last_activity_ms: AtomicU64,
    stage: Mutex<String>,
    /// Set once the final `result` line is written; nothing follows it.
    closed: AtomicBool,
}

impl Shared {
    fn emit(&self, value: Value) {
        let mut out = self.out.lock().unwrap_or_else(|e| e.into_inner());
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        let _ = writeln!(out, "{value}");
        let _ = out.flush();
    }
    /// Write the `result` line; later events (e.g. a racing heartbeat) are dropped.
    fn emit_final(&self, value: Value) {
        let mut out = self.out.lock().unwrap_or_else(|e| e.into_inner());
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        let _ = writeln!(out, "{value}");
        let _ = out.flush();
    }
    fn touch(&self) {
        self.last_activity_ms
            .store(self.started.elapsed().as_millis() as u64, Ordering::Relaxed);
    }
    fn stage(&self, name: &str, index: u64, total: u64) {
        *self.stage.lock().unwrap_or_else(|e| e.into_inner()) = name.into();
        self.emit(json!({"type":"stage","name":name,"index":index,"total":total}));
    }
}

/// Entry point; `args` are the arguments after `--worker`.
pub fn main(args: Vec<OsString>) -> ExitCode {
    let mut args = args.into_iter();
    let command = args
        .next()
        .and_then(|a| a.into_string().ok())
        .unwrap_or_default();
    if command == "--version" {
        println!(
            "{}",
            json!({"tool": env!("CARGO_PKG_VERSION"), "library": LIBRARY_VERSION, "protocol": PROTOCOL_MAJOR})
        );
        return ExitCode::SUCCESS;
    }
    let shared = Arc::new(Shared {
        out: Mutex::new(io::stdout()),
        cancel: AtomicBool::new(false),
        started: Instant::now(),
        last_activity_ms: AtomicU64::new(0),
        stage: Mutex::new("start".into()),
        closed: AtomicBool::new(false),
    });
    shared.emit(json!({
        "type":"hello","protocol":PROTOCOL_MAJOR,"tool":env!("CARGO_PKG_VERSION"),
        "command":command,"heartbeat_ms":HEARTBEAT.as_millis() as u64,
        "library":LIBRARY_VERSION
    }));
    spawn_cancel_listener(shared.clone());
    spawn_heartbeat(shared.clone());

    let opts = match Options::parse(args) {
        Ok(o) => o,
        Err(e) => return finish(&shared, Err(e)),
    };
    let result = match command.as_str() {
        "sync" => sync(&shared, &opts),
        "rustup" => rustup(&shared, &opts),
        "publish" => publish(&shared, &opts),
        other => Err(format!("unknown worker command: {other}")),
    };
    finish(&shared, result)
}

fn finish(shared: &Shared, result: Result<Value, String>) -> ExitCode {
    match result {
        Ok(report) => {
            shared.emit_final(json!({"type":"result","ok":true,"report":report}));
            ExitCode::SUCCESS
        }
        Err(message) => {
            let kind = if shared.cancel.load(Ordering::SeqCst) {
                "cancelled"
            } else {
                "unclassified"
            };
            shared.emit_final(
                json!({"type":"result","ok":false,"error":{"kind":kind,"message":message}}),
            );
            ExitCode::FAILURE
        }
    }
}

fn spawn_cancel_listener(shared: Arc<Shared>) {
    std::thread::spawn(move || {
        let stdin = io::stdin();
        let mut line = String::new();
        loop {
            line.clear();
            match stdin.lock().read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) if line.contains("\"cancel\"") => break,
                Ok(_) => {}
            }
        }
        // Closing stdin also means stop: the parent went away or asked to stop.
        shared.cancel.store(true, Ordering::SeqCst);
    });
}

fn spawn_heartbeat(shared: Arc<Shared>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(HEARTBEAT);
        if shared.closed.load(Ordering::SeqCst) {
            break;
        }
        let now = shared.started.elapsed().as_millis() as u64;
        let idle = now.saturating_sub(shared.last_activity_ms.load(Ordering::Relaxed));
        let stage = shared
            .stage
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        shared.emit(json!({"type":"heartbeat","stage":stage,"idle_ms":idle}));
    });
}

#[derive(Default)]
struct Options {
    values: Vec<(String, OsString)>,
    flags: Vec<String>,
}

impl Options {
    fn parse(args: impl Iterator<Item = OsString>) -> Result<Self, String> {
        let mut o = Options::default();
        let mut args = args.peekable();
        while let Some(key) = args.next() {
            let key = key
                .into_string()
                .map_err(|_| "option names must be UTF-8")?;
            if ["--resume", "--force"].contains(&key.as_str()) {
                o.flags.push(key);
                continue;
            }
            let value = args.next().ok_or(format!("missing value for {key}"))?;
            o.values.push((key, value));
        }
        Ok(o)
    }
    fn get(&self, key: &str) -> Option<&OsString> {
        self.values.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    fn path(&self, key: &str) -> Result<PathBuf, String> {
        self.get(key)
            .map(PathBuf::from)
            .ok_or(format!("{key} is required"))
    }
    fn text(&self, key: &str) -> Result<String, String> {
        self.get(key)
            .ok_or(format!("{key} is required"))?
            .to_str()
            .map(str::to_owned)
            .ok_or(format!("{key} must be UTF-8"))
    }
    fn all(&self, key: &str) -> Result<Vec<String>, String> {
        self.values
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| {
                v.to_str()
                    .map(str::to_owned)
                    .ok_or(format!("{key} must be UTF-8"))
            })
            .collect()
    }
    fn flag(&self, key: &str) -> bool {
        self.flags.iter().any(|f| f == key)
    }
}

fn sync(shared: &Arc<Shared>, o: &Options) -> Result<Value, String> {
    let resume = o.flag("--resume");
    let force = o.flag("--force");
    if resume && force {
        return Err("--resume and --force are mutually exclusive".into());
    }
    shared.stage("sync", 1, 1);
    let config =
        rust_mirror::selection::Config::read(&o.path("--config")?).map_err(|e| e.to_string())?;
    let output = o.path("--output")?;
    let previous = o.get("--previous").map(PathBuf::from);
    let source = Observed::new(HttpsSource::default(), shared.clone());
    let result = if force {
        rust_mirror::selection::sync::run_forced(&config, previous.as_deref(), &output, &source)
    } else {
        rust_mirror::selection::sync::run(&config, previous.as_deref(), &output, resume, &source)
    }
    .map_err(|e| e.to_string())?;
    let mut report = serde_json::to_value(&result.report).map_err(|e| e.to_string())?;
    report["mirror"] = json!(result.mirror.display().to_string());
    Ok(report)
}

fn rustup(shared: &Arc<Shared>, o: &Options) -> Result<Value, String> {
    shared.stage("rustup finish", 1, 1);
    let budget = match o.get("--max-bytes") {
        Some(v) => v
            .to_str()
            .and_then(|s| s.parse().ok())
            .ok_or("invalid --max-bytes")?,
        None => 10 * 1024 * 1024 * 1024,
    };
    let source = Observed::new(HttpsSource::default(), shared.clone());
    let mirror = rust_mirror::selection::rustup::prepare(
        &o.path("--snapshot")?,
        &o.text("--version")?,
        &o.all("--promote")?,
        &o.path("--output")?,
        budget,
        &source,
    )
    .map_err(|e| e.to_string())?;
    Ok(json!({"mirror": mirror.display().to_string()}))
}

fn publish(shared: &Arc<Shared>, o: &Options) -> Result<Value, String> {
    shared.stage("publish", 1, 1);
    shared.emit(json!({"type":"log","level":"warn","message":
        "rust-mirror 0.1.1 cannot stop a publication safely; Stop only takes effect when it ends"}));
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let credentials = match (env("NEXUS_USERNAME"), env("NEXUS_PASSWORD")) {
        (None, None) => None,
        (Some(u), Some(p)) => Some((u, p)),
        _ => return Err("enter both username and password, or neither".into()),
    };
    let report = rust_mirror::publish::publish(
        &o.path("--source")?,
        &rust_mirror::publish::Options {
            base_url: o.text("--base-url")?,
            ca_file: o.get("--ca-file").map(PathBuf::from),
            credentials,
            allow_loopback_http: false,
        },
    )
    .map_err(|e| e.to_string())?;
    serde_json::to_value(&report).map_err(|e| e.to_string())
}

/// Wraps the library's HTTPS source to report transfers and honour Stop. It
/// passes every request through unchanged; verification stays in the library.
struct Observed<S> {
    inner: S,
    shared: Arc<Shared>,
}

impl<S: Source> Observed<S> {
    fn new(inner: S, shared: Arc<Shared>) -> Self {
        Self { inner, shared }
    }
    fn observe(
        &self,
        url: &str,
        out: &mut dyn Write,
        offset: u64,
        run: impl FnOnce(&mut dyn Write) -> rust_mirror::Result<Transfer>,
    ) -> rust_mirror::Result<Transfer> {
        if self.shared.cancel.load(Ordering::SeqCst) {
            return Err("stopped by user".into());
        }
        let path = url.strip_prefix(ORIGIN).unwrap_or(url).to_string();
        let action = if offset > 0 { "resume" } else { "download" };
        self.shared
            .emit(json!({"type":"artifact","path":path,"action":action,"offset":offset}));
        self.shared.touch();
        let mut w = Counting {
            out,
            shared: &self.shared,
            path: &path,
            bytes: offset,
            last: Instant::now(),
        };
        let result = run(&mut w);
        w.report();
        if result.is_ok() {
            self.shared
                .emit(json!({"type":"artifact","path":path,"action":"complete"}));
        }
        result
    }
}

impl<S: Source> Source for Observed<S> {
    fn fetch(&self, url: &str, output: &mut dyn Write, limit: u64) -> rust_mirror::Result<()> {
        self.observe(url, output, 0, |w| {
            self.inner.fetch(url, w, limit).map(|_| Transfer::Complete)
        })
        .map(|_| ())
    }
    fn fetch_from(
        &self,
        url: &str,
        output: &mut dyn Write,
        offset: u64,
        limit: u64,
    ) -> rust_mirror::Result<Transfer> {
        self.observe(url, output, offset, |w| {
            self.inner.fetch_from(url, w, offset, limit)
        })
    }
}

struct Counting<'a> {
    out: &'a mut dyn Write,
    shared: &'a Shared,
    path: &'a str,
    bytes: u64,
    last: Instant,
}

impl Counting<'_> {
    fn report(&self) {
        self.shared
            .emit(json!({"type":"bytes","path":self.path,"bytes":self.bytes}));
    }
}

impl Write for Counting<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.shared.cancel.load(Ordering::SeqCst) {
            return Err(io::Error::other("stopped by user"));
        }
        let n = self.out.write(buf)?;
        self.bytes += n as u64;
        self.shared.touch();
        if self.last.elapsed() >= Duration::from_millis(250) {
            self.last = Instant::now();
            self.report();
        }
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn library_version_matches_lockfile() {
        let lock = include_str!("../Cargo.lock");
        let needle = format!(
            "name = \"rust-mirror\"\nversion = \"{}\"",
            super::LIBRARY_VERSION
        );
        assert!(lock.contains(&needle), "update LIBRARY_VERSION");
    }
}
