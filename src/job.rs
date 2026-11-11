//! Runs one rust-mirror command as a child process and tracks what it reports.
//!
//! All blocking I/O happens on reader threads; the UI only calls [`Job::poll`],
//! which never blocks. The GUI therefore stays responsive, and can tell the
//! user how long ago the backend last said anything, even if it hangs.

use crate::protocol::{self, Event, Outcome};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

/// Most recent lines kept for the activity view.
const LOG_LIMIT: usize = 2000;

/// What to run. `env` is passed to the child only and never displayed.
pub struct Spec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub env: Vec<(String, String)>,
    pub label: String,
}

impl Spec {
    /// Equivalent rust-mirror CLI command, for display. A leading `--worker`
    /// is shown as `rust-mirror`. Environment values are never included.
    pub fn display(&self) -> String {
        let mut args = self.args.iter().peekable();
        let mut out = if args
            .peek()
            .is_some_and(|a| *a == crate::protocol::WORKER_ARG)
        {
            args.next();
            "rust-mirror".to_string()
        } else {
            quote(&self.program.to_string_lossy())
        };
        for a in args {
            out.push(' ');
            out.push_str(&quote(&a.to_string_lossy()));
        }
        out
    }
}

fn quote(s: &str) -> String {
    if !s.is_empty() && !s.contains([' ', '"', '\'']) {
        s.to_owned()
    } else {
        format!("\"{}\"", s.replace('"', "\\\""))
    }
}

enum Msg {
    Event(Event),
    Stderr(String),
    StdoutClosed(Option<String>),
}

/// One line in the activity log.
#[derive(Clone)]
pub struct LogLine {
    pub at: Duration,
    pub kind: LogKind,
    pub text: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Stage,
    Artifact,
    Info,
    Warn,
    Error,
    Stderr,
}

#[derive(Clone, Default)]
pub struct FileProgress {
    pub action: String,
    pub bytes: u64,
    pub total: Option<u64>,
    pub done: bool,
}

/// Overall state as shown to the user.
#[derive(Debug, Clone, PartialEq)]
pub enum Health {
    /// Running; last event this long ago.
    Running(Duration),
    /// Running, but no event (not even a heartbeat) for this long.
    Stalled(Duration),
    /// Stop requested this long ago; waiting for the backend to finish.
    Stopping(Duration),
    Succeeded,
    /// The backend reported a typed failure.
    Failed {
        kind: String,
        message: String,
    },
    /// The process ended without a `result` event.
    ExitedWithoutResult(String),
    Killed,
}

/// Progress model built only from events. Kept separate from the process so it
/// can be tested without one.
#[derive(Default)]
pub struct Progress {
    pub hello: Option<protocol::Hello>,
    pub stage: Option<protocol::Stage>,
    pub files: BTreeMap<String, FileProgress>,
    pub bytes_total: u64,
    pub result: Option<Outcome>,
    pub protocol_warnings: usize,
    pub events: u64,
    /// Time without visible progress, as last reported by a heartbeat.
    pub idle: Option<Duration>,
}

impl Progress {
    /// Apply one event; returns a log line when the event is worth showing.
    pub fn apply(&mut self, event: &Event) -> Option<(LogKind, String)> {
        self.events += 1;
        match event {
            Event::Hello(h) => {
                self.hello = Some(h.clone());
                Some((
                    LogKind::Info,
                    format!(
                        "backend {} started `{}` (protocol {})",
                        h.tool, h.command, h.protocol
                    ),
                ))
            }
            Event::Stage(s) => {
                self.stage = Some(s.clone());
                let n = match (s.index, s.total) {
                    (Some(i), Some(t)) => format!(" {i}/{t}"),
                    _ => String::new(),
                };
                let d = s
                    .detail
                    .as_deref()
                    .map(|d| format!(": {d}"))
                    .unwrap_or_default();
                Some((LogKind::Stage, format!("{}{n}{d}", s.name)))
            }
            Event::Artifact(a) => {
                let f = self.files.entry(a.path.clone()).or_default();
                f.action = a.action.clone();
                f.total = a.expected_bytes.or(f.total);
                f.done = matches!(a.action.as_str(), "reuse" | "verified" | "complete");
                let at = a
                    .offset
                    .filter(|o| *o > 0)
                    .map(|o| format!(" @ {}", human_bytes(o)))
                    .unwrap_or_default();
                Some((LogKind::Artifact, format!("{} {}{at}", a.action, a.path)))
            }
            Event::Bytes(b) => {
                let f = self.files.entry(b.path.clone()).or_default();
                if b.bytes > f.bytes {
                    self.bytes_total += b.bytes - f.bytes;
                }
                f.bytes = b.bytes;
                f.total = b.total.or(f.total);
                None
            }
            Event::Heartbeat(h) => {
                self.idle = h.idle_ms.map(Duration::from_millis);
                None
            }
            Event::Log(l) => {
                let kind = match l.level.as_str() {
                    "warn" | "warning" => LogKind::Warn,
                    "error" => LogKind::Error,
                    _ => LogKind::Info,
                };
                Some((kind, l.message.clone()))
            }
            Event::Result(o) => {
                self.result = Some(o.clone());
                Some(match &o.error {
                    None if o.ok => (LogKind::Info, "finished successfully".into()),
                    Some(e) if e.kind == "cancelled" => {
                        (LogKind::Warn, format!("stopped: {}", e.message))
                    }
                    Some(e) => (
                        LogKind::Error,
                        format!("failed [{}]: {}", e.kind, e.message),
                    ),
                    None => (LogKind::Error, "failed without an error description".into()),
                })
            }
            Event::Unknown { kind, .. } => {
                Some((LogKind::Info, format!("(unrecognised event \"{kind}\")")))
            }
            Event::Malformed { line, reason } => {
                self.protocol_warnings += 1;
                Some((LogKind::Warn, format!("protocol: {reason}: {line}")))
            }
        }
    }

    pub fn heartbeat_interval(&self) -> Duration {
        Duration::from_millis(
            self.hello
                .as_ref()
                .and_then(|h| h.heartbeat_ms)
                .unwrap_or(protocol::DEFAULT_HEARTBEAT_MS)
                .clamp(250, 60_000),
        )
    }

    /// Silence longer than this means the backend is stalled.
    pub fn stall_after(&self) -> Duration {
        (self.heartbeat_interval() * 3).max(Duration::from_secs(5))
    }
}

/// A running or finished rust-mirror process.
pub struct Job {
    pub label: String,
    pub command_line: String,
    pub started: Instant,
    pub last_event: Instant,
    pub progress: Progress,
    pub log: Vec<LogLine>,
    pub stop_requested: Option<Instant>,
    pub killed: bool,
    pub exit: Option<ExitStatus>,
    pub exited_at: Option<Instant>,
    stdout_closed: bool,
    child: Child,
    stdin: Option<ChildStdin>,
    rx: Receiver<Msg>,
}

impl Job {
    /// Start the process. `wake` is called from reader threads whenever new
    /// output arrives (the UI passes a repaint request).
    pub fn start(spec: Spec, wake: impl Fn() + Send + Clone + 'static) -> std::io::Result<Job> {
        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        no_console_window(&mut cmd);
        let mut child = cmd.spawn()?;
        let (tx, rx) = mpsc::channel();
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        spawn_stdout_reader(stdout, tx.clone(), wake.clone());
        spawn_stderr_reader(stderr, tx, wake);
        let now = Instant::now();
        Ok(Job {
            label: spec.label.clone(),
            command_line: spec.display(),
            started: now,
            last_event: now,
            progress: Progress::default(),
            log: Vec::new(),
            stop_requested: None,
            killed: false,
            exit: None,
            exited_at: None,
            stdout_closed: false,
            stdin: child.stdin.take(),
            child,
            rx,
        })
    }

    /// Drain pending output and check for process exit. Never blocks.
    pub fn poll(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Event(e) => {
                    self.last_event = Instant::now();
                    if let Some((kind, text)) = self.progress.apply(&e) {
                        self.push(kind, text);
                    }
                }
                Msg::Stderr(line) => {
                    // Bridge for rust-mirror 0.1.1, which reports sync stages only
                    // as text: show them as stage detail. Display only.
                    if let (Some(detail), Some(stage)) =
                        (line.strip_prefix("sync: "), self.progress.stage.as_mut())
                    {
                        stage.detail = Some(detail.to_string());
                    }
                    self.push(LogKind::Stderr, line)
                }
                Msg::StdoutClosed(err) => {
                    self.stdout_closed = true;
                    if let Some(err) = err {
                        self.progress.protocol_warnings += 1;
                        self.push(LogKind::Warn, format!("protocol: {err}"));
                    }
                }
            }
        }
        if self.exit.is_none() {
            if let Ok(Some(status)) = self.child.try_wait() {
                self.exit = Some(status);
                self.exited_at = Some(Instant::now());
                self.stdin = None;
                let text = format!("process exited ({})", describe_exit(&status));
                self.push(LogKind::Info, text);
            }
        }
    }

    fn push(&mut self, kind: LogKind, text: String) {
        if self.log.len() >= LOG_LIMIT {
            self.log.drain(..LOG_LIMIT / 4);
        }
        self.log.push(LogLine {
            at: self.started.elapsed(),
            kind,
            text,
        });
    }

    /// The process has exited and all of its output has been read.
    pub fn finished(&self) -> bool {
        self.exit.is_some() && self.stdout_closed
    }

    pub fn health(&self) -> Health {
        if self.killed && self.finished() {
            return Health::Killed;
        }
        // Wait until all output is read, so a final `result` is not missed.
        if self.finished() {
            return match &self.progress.result {
                Some(o) if o.ok && o.error.is_none() => Health::Succeeded,
                Some(o) => Health::Failed {
                    kind: o
                        .error
                        .as_ref()
                        .map_or("unknown".into(), |e| e.kind.clone()),
                    message: o
                        .error
                        .as_ref()
                        .map_or("no message".into(), |e| e.message.clone()),
                },
                None => Health::ExitedWithoutResult(describe_exit(&self.exit.unwrap())),
            };
        }
        if let Some(at) = self.stop_requested {
            return Health::Stopping(at.elapsed());
        }
        let quiet = self.last_event.elapsed();
        if quiet >= self.progress.stall_after() {
            Health::Stalled(quiet)
        } else {
            Health::Running(quiet)
        }
    }

    /// Ask the backend to stop at its next safe point: send the cancel line and
    /// close stdin. Progress already on disk is kept for Resume.
    pub fn request_stop(&mut self) {
        if self.exit.is_some() || self.stop_requested.is_some() {
            return;
        }
        if let Some(mut stdin) = self.stdin.take() {
            let _ = stdin.write_all(protocol::CANCEL_LINE.as_bytes());
            let _ = stdin.flush();
        }
        self.stop_requested = Some(Instant::now());
        self.push(
            LogKind::Warn,
            "stop requested; waiting for the backend to stop safely".into(),
        );
    }

    /// Terminate the process immediately.
    pub fn kill(&mut self) {
        if self.exit.is_none() {
            let _ = self.child.kill();
            self.killed = true;
            self.push(LogKind::Warn, "process terminated by user".into());
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        if self.exit.is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn describe_exit(status: &ExitStatus) -> String {
    match status.code() {
        Some(c) => format!("exit code {c}"),
        None => "terminated by signal".into(),
    }
}

fn spawn_stdout_reader(
    stdout: impl Read + Send + 'static,
    tx: Sender<Msg>,
    wake: impl Fn() + Send + 'static,
) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = Vec::new();
        let err = loop {
            line.clear();
            match (&mut reader)
                .take(protocol::MAX_LINE as u64 + 1)
                .read_until(b'\n', &mut line)
            {
                Ok(0) => break None,
                Ok(_) if line.len() > protocol::MAX_LINE => {
                    break Some("event line exceeds 1 MiB; stopped reading events".to_string())
                }
                Ok(_) => {
                    let text = String::from_utf8_lossy(&line);
                    if text.trim().is_empty() {
                        continue;
                    }
                    if tx.send(Msg::Event(Event::parse(&text))).is_err() {
                        return;
                    }
                    wake();
                }
                Err(e) => break Some(format!("reading events failed: {e}")),
            }
        };
        let _ = tx.send(Msg::StdoutClosed(err));
        wake();
    });
}

fn spawn_stderr_reader(
    stderr: impl Read + Send + 'static,
    tx: Sender<Msg>,
    wake: impl Fn() + Send + 'static,
) {
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines() {
            let Ok(line) = line else { break };
            let line: String = line.chars().take(2000).collect();
            if tx.send(Msg::Stderr(line)).is_err() {
                return;
            }
            wake();
        }
    });
}

#[cfg(windows)]
fn no_console_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn no_console_window(_: &mut Command) {}

/// Run `program --version --json` with a timeout and check compatibility.
/// Blocks; call from a background thread.
pub fn discover(
    program: &std::path::Path,
    timeout: Duration,
) -> Result<protocol::BackendVersion, String> {
    if program.as_os_str().is_empty() {
        return Err("Could not find this program's own executable path.".into());
    }
    let mut cmd = Command::new(program);
    cmd.args(protocol::VERSION_ARGS)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    no_console_window(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| {
        format!(
            "Could not start the operation worker {}: {e}",
            program.display()
        )
    })?;
    let mut stdout = child.stdout.take().expect("piped");
    let mut stderr = child.stderr.take().expect("piped");
    let out = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = (&mut stdout).take(64 * 1024).read_to_string(&mut s);
        s
    });
    let err = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = (&mut stderr).take(64 * 1024).read_to_string(&mut s);
        s
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Ok(Some(s)) = child.try_wait() {
            break s;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "{} did not answer within {} s.",
                program.display(),
                timeout.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    if !status.success() {
        let hint = stderr.lines().next().unwrap_or("").trim();
        return Err(format!(
            "The operation worker failed to start ({}{}{}).",
            describe_exit(&status),
            if hint.is_empty() { "" } else { ": " },
            hint
        ));
    }
    protocol::check_version(&stdout)
}

pub fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

pub fn human_duration(d: Duration) -> String {
    let s = d.as_secs();
    if d < Duration::from_secs(10) && d.subsec_millis() != 0 {
        format!("{:.1} s", d.as_secs_f32())
    } else if s < 60 {
        format!("{s} s")
    } else if s < 3600 {
        format!("{}:{:02} min", s / 60, s % 60)
    } else {
        format!("{}:{:02}:{:02} h", s / 3600, (s / 60) % 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_events_accumulate_without_double_counting() {
        let mut p = Progress::default();
        for line in [
            r#"{"type":"artifact","path":"a","action":"download","expected_bytes":100}"#,
            r#"{"type":"bytes","path":"a","bytes":40}"#,
            r#"{"type":"bytes","path":"a","bytes":100}"#,
            r#"{"type":"bytes","path":"b","bytes":10}"#,
        ] {
            p.apply(&Event::parse(line));
        }
        assert_eq!(p.bytes_total, 110);
        assert_eq!(p.files["a"].total, Some(100));
    }

    #[test]
    fn stall_threshold_follows_heartbeat() {
        let mut p = Progress::default();
        assert_eq!(p.stall_after(), Duration::from_secs(6));
        p.apply(&Event::parse(
            r#"{"type":"hello","protocol":1,"heartbeat_ms":500}"#,
        ));
        assert_eq!(p.stall_after(), Duration::from_secs(5));
        p.apply(&Event::parse(
            r#"{"type":"hello","protocol":1,"heartbeat_ms":10000}"#,
        ));
        assert_eq!(p.stall_after(), Duration::from_secs(30));
    }

    #[test]
    fn display_never_contains_env() {
        let spec = Spec {
            program: "rust-mirror".into(),
            args: vec!["publish".into(), "--source".into(), "C:\\my dir".into()],
            env: vec![("NEXUS_PASSWORD".into(), "secret".into())],
            label: "x".into(),
        };
        let shown = spec.display();
        assert!(!shown.contains("secret"));
        assert!(shown.contains("\"C:\\my dir\""));
        let w = crate::ops::sync(
            "C:/app/gui.exe",
            "m.toml",
            "o",
            "",
            crate::ops::SyncMode::New,
        );
        assert_eq!(w.display(), "rust-mirror sync --config m.toml --output o");
    }
}
