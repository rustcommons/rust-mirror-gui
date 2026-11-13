//! Hidden worker mode: `rust-mirror-gui --worker COMMAND …`.
//!
//! The GUI re-runs its own executable in this mode for every operation, so a
//! hung or crashed operation can always be stopped or killed without taking the
//! window down. The worker contains no mirroring logic and writes rust-mirror's
//! event protocol 1 (rust-mirror `docs/EVENTS.md`) to stdout:
//!
//! - rust-mirror CLI commands (`--version`, `sync`, `rustup`, `publish`,
//!   `generations`, …) run through `rust_mirror::cli::run_with_events`, the same
//!   code path as `rust-mirror --events jsonl --cancel-stdin …`;
//! - GUI-only commands (`plan-all`, `retention-preview`, `retention-apply`) call
//!   library APIs inside an `events::Control` session serialized by the
//!   library's `JsonLines`, with the same hello/result framing.
use rust_mirror::events::{CancelToken, Control, EventKind, JsonLines, PROTOCOL_VERSION};
use serde_json::{json, Value};
use std::ffi::OsString;
use std::io::{self, BufRead, Read};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

/// rust-mirror library version compiled into this executable (checked by a test
/// against Cargo.lock).
pub const LIBRARY_VERSION: &str = "0.5.0";

/// Entry point; `args` are the arguments after `--worker`.
pub fn main(args: Vec<OsString>) -> ExitCode {
    let command = args
        .first()
        .and_then(|a| a.to_str())
        .unwrap_or("")
        .to_string();
    let code = match command.as_str() {
        "plan-all"
        | "retention-preview"
        | "retention-apply"
        | "network-env"
        | "inspect-ca"
        | "crates-write"
        | "crates-client-config"
        | "crates-retention-preview"
        | "crates-retention-apply" => gui_command(&command, &args[1..]),
        // Discovery is local and cheap; it takes no control input.
        "--version" | "--help" => {
            rust_mirror::cli::run_with_events(args, io::stdout(), CancelToken::new())
        }
        _ => {
            let mut cli = vec![OsString::from("--cancel-stdin")];
            cli.extend(args);
            rust_mirror::cli::run_with_events(cli, io::stdout(), CancelToken::new())
        }
    };
    ExitCode::from(code.clamp(0, 255) as u8)
}

/// Same stdin control rule as `--cancel-stdin`: a `{"type":"cancel"}` line or
/// EOF requests cancellation.
fn listen_for_cancel(cancel: CancelToken) {
    std::thread::spawn(move || {
        let stdin = io::stdin();
        let mut line = String::new();
        loop {
            line.clear();
            match stdin.lock().read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
                    if compact == r#"{"type":"cancel"}"# {
                        break;
                    }
                }
            }
        }
        cancel.cancel();
    });
}

fn gui_command(command: &str, args: &[OsString]) -> i32 {
    // As with rust-mirror's `--proxy-password-stdin`: consume exactly one
    // bounded password line before any cancel-line processing. Never echoed.
    let password = if args.iter().any(|a| a == "--proxy-password-stdin") {
        let mut line = String::new();
        let _ = io::stdin().lock().take(4098).read_line(&mut line);
        Some(line.trim_end_matches(['\n', '\r']).to_string())
    } else {
        None
    };
    let cancel = CancelToken::new();
    listen_for_cancel(cancel.clone());
    let control = Control::new(Arc::new(JsonLines::new(io::stdout())), cancel);
    let hello = EventKind::Hello {
        protocol: PROTOCOL_VERSION,
        tool_version: LIBRARY_VERSION.into(),
        command: command.into(),
    };
    if control.emit(hello).is_err() {
        return 1;
    }
    if matches!(
        command,
        "network-env"
            | "inspect-ca"
            | "crates-write"
            | "crates-client-config"
            | "crates-retention-preview"
            | "crates-retention-apply"
    ) {
        let name = match command {
            "network-env" => "read environment",
            "inspect-ca" => "read certificate file",
            "crates-write" => "write catalog",
            "crates-retention-preview" => "read generations",
            "crates-retention-apply" => "re-check and delete",
            _ => "read crate generation",
        };
        let _ = control.emit(EventKind::Stage {
            name: name.into(),
            index: 1,
            total: Some(1),
        });
    }
    let result = Options::parse(args).and_then(|o| match command {
        "plan-all" => plan_all(&control, &o, password),
        "network-env" => Ok(serde_json::to_value(
            rust_mirror::network::detect_environment()?,
        )?),
        "crates-write" => crates_write(&o),
        "crates-retention-preview" => crates_retention_preview(&control, &o),
        "crates-retention-apply" => crates_retention_apply(&control, &o),
        "crates-client-config" => Ok(json!({
            "config": rust_mirror::crates::mirror::client_config_for_generation(
                &o.path("--generation")?
            )?
        })),
        "inspect-ca" => Ok(json!({
            "certificates": rust_mirror::network::inspect_ca_file(&o.path("--file")?)?
        })),
        "retention-preview" => retention_preview(&control, &o),
        _ => retention_apply(&control, &o),
    });
    let (event, code) = match result {
        Ok(report) => (success(report), 0),
        Err(error) => {
            let error = error
                .downcast_ref::<rust_mirror::Error>()
                .cloned()
                .unwrap_or_else(|| {
                    rust_mirror::Error::new(
                        rust_mirror::error_kind(error.as_ref()),
                        error.to_string(),
                    )
                });
            let code = if error.kind == rust_mirror::ErrorKind::Cancelled {
                130
            } else {
                1
            };
            (
                EventKind::Result {
                    ok: false,
                    report: None,
                    error: Some(error),
                },
                code,
            )
        }
    };
    if control.emit(event).is_err() {
        1
    } else {
        code
    }
}

/// Build a successful `result` from JSON data. The protocol's report type has
/// no null, so absent optional fields are omitted, as in the CLI's reports.
fn success(report: Value) -> EventKind {
    fn strip(v: Value) -> Value {
        match v {
            Value::Object(m) => Value::Object(
                m.into_iter()
                    .filter(|(_, v)| !v.is_null())
                    .map(|(k, v)| (k, strip(v)))
                    .collect(),
            ),
            Value::Array(a) => {
                Value::Array(a.into_iter().filter(|v| !v.is_null()).map(strip).collect())
            }
            other => other,
        }
    }
    serde_json::from_value(json!({"type": "result", "ok": true, "report": strip(report)}))
        .unwrap_or_else(|e| EventKind::Result {
            ok: false,
            report: None,
            error: Some(rust_mirror::Error::new(
                rust_mirror::ErrorKind::Internal,
                format!("report encoding failed: {e}"),
            )),
        })
}

type Res<T> = rust_mirror::Result<T>;

#[derive(Default)]
struct Options {
    values: Vec<(String, OsString)>,
    flags: Vec<String>,
}

impl Options {
    fn parse(args: &[OsString]) -> Res<Self> {
        let mut o = Options::default();
        let mut args = args.iter();
        while let Some(key) = args.next() {
            let key = key
                .to_str()
                .ok_or("option names must be UTF-8")?
                .to_string();
            if [
                "--allow-delete",
                "--replace",
                "--estimate-sizes",
                "--proxy-from-env",
                "--proxy-password-stdin",
            ]
            .contains(&key.as_str())
            {
                o.flags.push(key);
                continue;
            }
            let value = args
                .next()
                .ok_or(format!("missing value for {key}"))?
                .clone();
            o.values.push((key, value));
        }
        Ok(o)
    }
    fn get(&self, key: &str) -> Option<&OsString> {
        self.values.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    fn path(&self, key: &str) -> Res<PathBuf> {
        Ok(self
            .get(key)
            .map(PathBuf::from)
            .ok_or(format!("{key} is required"))?)
    }
    fn text(&self, key: &str) -> Res<String> {
        Ok(self
            .get(key)
            .ok_or(format!("{key} is required"))?
            .to_str()
            .map(str::to_owned)
            .ok_or(format!("{key} must be UTF-8"))?)
    }
}

fn invalid(message: String) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(rust_mirror::Error::new(
        rust_mirror::ErrorKind::Invalid,
        message,
    ))
}

/// Build the library's network options from the same flags rust-mirror's CLI
/// accepts (docs/NETWORK.md). The password arrives only via stdin.
fn network_options(
    o: &Options,
    password: Option<String>,
) -> Res<rust_mirror::network::NetworkOptions> {
    use rust_mirror::network::{NetworkOptions, ProxyCredentials, ProxyMode};
    let mut n = NetworkOptions::default();
    let from_env = o.flags.iter().any(|f| f == "--proxy-from-env");
    if o.get("--proxy").is_some() && from_env {
        return Err(invalid(
            "manual and environment proxy modes are mutually exclusive".into(),
        ));
    }
    if o.get("--proxy").is_some() {
        n.proxy = ProxyMode::Manual(o.text("--proxy")?);
    } else if from_env {
        n.proxy = ProxyMode::Environment;
    }
    let username = match o.get("--proxy-username-env") {
        Some(name) => Some(
            std::env::var(name)
                .map_err(|_| invalid("proxy username variable is not set".into()))?,
        ),
        None => None,
    };
    match (username, password) {
        (Some(username), Some(password)) => {
            n.proxy_credentials = Some(ProxyCredentials { username, password })
        }
        (None, None) => {}
        _ => {
            return Err(invalid(
                "enter both the proxy username and password, or neither".into(),
            ))
        }
    }
    if let Some(list) = o.get("--no-proxy") {
        let list = list.to_str().ok_or("--no-proxy must be UTF-8")?;
        n.no_proxy = list
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
    }
    n.extra_ca_file = o.get("--extra-ca-file").map(PathBuf::from);
    let secs = |key: &str| -> Res<Option<u64>> {
        o.get(key)
            .map(|v| {
                v.to_str()
                    .and_then(|v| v.parse().ok())
                    .ok_or_else(|| invalid(format!("{key} must be whole seconds")))
            })
            .transpose()
    };
    if let Some(s) = secs("--connect-timeout")? {
        n.connect_timeout = std::time::Duration::from_secs(s);
    }
    if let Some(s) = secs("--read-timeout")? {
        n.read_timeout = std::time::Duration::from_secs(s);
    }
    Ok(n)
}

/// Read-only crate retention preview. The catalog (JSON, with the window's
/// `crate-retention` policy) supplies the policy and exact crate pins.
fn crates_retention_preview(control: &Control, o: &Options) -> Res<Value> {
    let file = o.path("--catalog-json")?;
    let raw = std::fs::read(&file)?;
    let _ = std::fs::remove_file(&file);
    let catalog: rust_mirror::crates::catalog::Catalog =
        serde_json::from_slice(&raw).map_err(|e| invalid(format!("invalid catalog: {e}")))?;
    let plan = rust_mirror::crates::retention::preview_directory_with_control(
        &catalog,
        &o.path("--directory")?,
        &o.path("--next-previous")?,
        &o.text("--as-of")?,
        control,
    )?;
    Ok(json!({ "plan": plan }))
}

/// Apply exactly the reviewed crate prune plan; the library re-previews and
/// refuses a stale plan (`resume_mismatch`) before deleting anything.
fn crates_retention_apply(control: &Control, o: &Options) -> Res<Value> {
    if !o.flags.iter().any(|f| f == "--allow-delete") {
        return Err(invalid("deletion needs --allow-delete".into()));
    }
    let plan_file = o.path("--plan-file")?;
    let raw = std::fs::read(&plan_file)?;
    let _ = std::fs::remove_file(&plan_file);
    let plan: rust_mirror::crates::retention::PrunePlan =
        serde_json::from_slice(&raw).map_err(|e| invalid(format!("invalid plan: {e}")))?;
    let report = rust_mirror::crates::retention::apply_with_control(&plan, true, control)?;
    if let Some(failure) = &report.failure {
        return Err(Box::new(rust_mirror::Error::new(
            rust_mirror::ErrorKind::Io,
            format!(
                "deletion stopped at {}: {} (earlier deletions remain; inspect before retrying)",
                failure.path.display(),
                failure.message
            ),
        )));
    }
    Ok(serde_json::to_value(&report)?)
}

/// Validate an edited crate catalog (JSON from the window) and write it as
/// canonical TOML through the library. Comments and key order are not kept.
fn crates_write(o: &Options) -> Res<Value> {
    let raw = std::fs::read(o.path("--input")?)?;
    let catalog: rust_mirror::crates::catalog::Catalog =
        serde_json::from_slice(&raw).map_err(|e| invalid(format!("invalid catalog: {e}")))?;
    let output = o.path("--output")?;
    catalog.write(&output, o.flags.iter().any(|f| f == "--replace"))?;
    Ok(json!({ "path": output.display().to_string() }))
}

/// Resolve every selection's metadata (no payload downloads, no files written).
fn plan_all(control: &Control, o: &Options, password: Option<String>) -> Res<Value> {
    let config = rust_mirror::selection::Config::read(&o.path("--config")?)?;
    let source = rust_mirror::HttpsSource::with_network(&network_options(o, password)?)?;
    let estimate = o.flags.iter().any(|f| f == "--estimate-sizes");
    control.run("plan preview", || {
        let total = config.selections.len() as u64;
        let mut plans = Vec::new();
        for (i, selection) in config.selections.iter().enumerate() {
            control.check()?;
            control.emit(EventKind::Stage {
                name: format!("resolve {}", selection.name),
                index: i as u64 + 1,
                total: Some(total),
            })?;
            let resolved = rust_mirror::selection::resolve(&config, &selection.name, &source)?;
            let mut plan = serde_json::to_value(&resolved.plan)?;
            if estimate {
                // Opt-in HEAD estimates; never verification.
                let sizes = rust_mirror::selection::measure_sizes(&resolved, &source, control)?;
                plan["sizes"] = serde_json::to_value(&sizes)?;
            }
            plans.push(plan);
        }
        Ok(json!({ "plans": plans }))
    })
}

/// The library's read-only retention preview for one workspace directory.
fn retention_preview(control: &Control, o: &Options) -> Res<Value> {
    let policy: rust_mirror::selection::retention::Policy =
        serde_json::from_str(&o.text("--policy")?)
            .map_err(|e| invalid(format!("invalid policy: {e}")))?;
    policy.validate()?;
    let plan = rust_mirror::selection::retention::preview_directory_with_control(
        &policy,
        &o.path("--directory")?,
        &o.path("--next-previous")?,
        &o.text("--as-of")?,
        control,
    )?;
    Ok(json!({ "plan": plan }))
}

/// Apply exactly the plan the user reviewed; the library reconstructs it and
/// refuses if anything changed since the preview.
fn retention_apply(control: &Control, o: &Options) -> Res<Value> {
    if !o.flags.iter().any(|f| f == "--allow-delete") {
        return Err(invalid("deletion needs --allow-delete".into()));
    }
    let plan_file = o.path("--plan-file")?;
    let raw = std::fs::read(&plan_file)?;
    let _ = std::fs::remove_file(&plan_file);
    let plan: rust_mirror::selection::retention::PrunePlan =
        serde_json::from_slice(&raw).map_err(|e| invalid(format!("invalid plan: {e}")))?;
    let report = rust_mirror::selection::retention::apply_with_control(&plan, true, control)?;
    if let Some(failure) = &report.failure {
        return Err(Box::new(rust_mirror::Error::new(
            rust_mirror::ErrorKind::Io,
            format!(
                "deletion stopped at {}: {} (earlier deletions remain; inspect before retrying)",
                failure.path.display(),
                failure.message
            ),
        )));
    }
    Ok(serde_json::to_value(&report)?)
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
