//! Client side of the rust-mirror JSON-lines event protocol.
//!
//! Implements protocol 1 as specified in rust-mirror's `docs/EVENTS.md`
//! (rust-mirror 0.2.0). Parsing is deliberately tolerant, as the spec requires:
//! unknown fields and unknown nonterminal event types are ignored (and shown
//! in the activity log), never fatal.

use serde::Deserialize;
use serde_json::Value;

/// Protocol major version this GUI understands.
pub const PROTOCOL_MAJOR: u64 = 1;
/// First argument that runs this executable as an operation worker.
pub const WORKER_ARG: &str = "--worker";
/// Arguments that ask the worker for its versions and protocol, offline.
pub const VERSION_ARGS: &[&str] = &[WORKER_ARG, "--version"];
/// Line written to the backend's stdin to ask for a graceful stop.
pub const CANCEL_LINE: &str = "{\"type\":\"cancel\"}\n";
/// Heartbeat interval assumed when `hello` does not state one (spec: about 1 s).
pub const DEFAULT_HEARTBEAT_MS: u64 = 1000;
/// Longest stdout line accepted from the backend.
pub const MAX_LINE: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Hello {
    pub protocol: u64,
    /// Backend (rust-mirror) package version.
    #[serde(default, alias = "tool_version")]
    pub tool: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub heartbeat_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Stage {
    pub name: String,
    #[serde(default)]
    pub index: Option<u64>,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Artifact {
    pub path: String,
    pub action: String,
    #[serde(default)]
    pub offset: Option<u64>,
    #[serde(default)]
    pub expected_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Bytes {
    pub path: String,
    /// Cumulative bytes of the current operation on this path, including any
    /// resume offset; resets with a new `artifact` event.
    pub count: u64,
    #[serde(default)]
    pub total: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Hashing {
    pub path: String,
    pub count: u64,
    #[serde(default)]
    pub total: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Heartbeat {
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    /// Sequence of the last non-heartbeat event; unchanged means no new progress.
    #[serde(default)]
    pub progress_seq: Option<u64>,
    #[serde(default)]
    pub cancel_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Log {
    #[serde(default)]
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ErrorInfo {
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Outcome {
    pub ok: bool,
    #[serde(default)]
    pub report: Option<Value>,
    #[serde(default)]
    pub error: Option<ErrorInfo>,
}

/// One line from the backend.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Hello(Hello),
    Stage(Stage),
    Artifact(Artifact),
    Bytes(Bytes),
    Hashing(Hashing),
    Heartbeat(Heartbeat),
    Log(Log),
    Result(Outcome),
    /// Valid JSON object with a type this GUI does not know (kept for display).
    Unknown {
        kind: String,
        raw: String,
    },
    /// A line that is not a JSON object with a string `type`, or has bad fields.
    Malformed {
        line: String,
        reason: String,
    },
}

impl Event {
    pub fn parse(line: &str) -> Event {
        let line = line.trim_end_matches(['\r', '\n']);
        let malformed = |reason: String| Event::Malformed {
            line: line.chars().take(500).collect(),
            reason,
        };
        let value: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => return malformed(format!("not JSON: {e}")),
        };
        let Some(kind) = value.get("type").and_then(Value::as_str).map(str::to_owned) else {
            return malformed("missing string field \"type\"".into());
        };
        fn typed<T: for<'de> Deserialize<'de>>(v: Value) -> Result<T, String> {
            serde_json::from_value(v).map_err(|e| e.to_string())
        }
        let parsed = match kind.as_str() {
            "hello" => typed(value).map(Event::Hello),
            "stage" => typed(value).map(Event::Stage),
            "artifact" => typed(value).map(Event::Artifact),
            "bytes" => typed(value).map(Event::Bytes),
            "hashing" => typed(value).map(Event::Hashing),
            "heartbeat" => typed(value).map(Event::Heartbeat),
            "log" => typed(value).map(Event::Log),
            "result" => typed(value).map(Event::Result),
            _ => {
                return Event::Unknown {
                    kind,
                    raw: line.chars().take(500).collect(),
                }
            }
        };
        parsed.unwrap_or_else(|e| malformed(format!("bad \"{kind}\" event: {e}")))
    }
}

/// Version information reported by the worker (`--worker --version`).
#[derive(Debug, Clone, PartialEq)]
pub struct BackendVersion {
    pub tool: String,
    /// rust-mirror library version compiled into the worker, if reported.
    pub library: Option<String>,
    pub protocol: u64,
}

/// Parse the worker's version answer and check the protocol. Because the worker
/// is this same executable, a failure here means a broken build or a blocked
/// child process, not a version mismatch. The text is shown to the user.
pub fn check_version(stdout: &str) -> Result<BackendVersion, String> {
    // `--worker --version` answers with a protocol-1 session: `hello`, then a
    // `result` whose report repeats `tool_version` and `protocol`.
    let mut hello_protocol = None;
    for line in stdout.lines().filter(|l| !l.trim().is_empty()) {
        match Event::parse(line) {
            Event::Hello(h) => hello_protocol = Some(h.protocol),
            Event::Result(o) => {
                let report = o.report.filter(|_| o.ok).ok_or_else(|| {
                    o.error
                        .map_or("The worker could not report its version.".into(), |e| {
                            format!("The worker could not report its version: {}", e.message)
                        })
                })?;
                let protocol = report
                    .get("protocol")
                    .and_then(Value::as_u64)
                    .or(hello_protocol)
                    .ok_or("The worker's answer has no protocol version.")?;
                if protocol != PROTOCOL_MAJOR {
                    return Err(format!(
                        "The worker speaks event protocol {protocol}; this window supports protocol {PROTOCOL_MAJOR}."
                    ));
                }
                let library = report
                    .get("tool_version")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                return Ok(BackendVersion {
                    tool: env!("CARGO_PKG_VERSION").into(),
                    library,
                    protocol,
                });
            }
            Event::Malformed { line, .. } => {
                let shown: String = line.chars().take(80).collect();
                return Err(format!(
                    "The operation worker gave an unreadable answer (\"{shown}\")."
                ));
            }
            _ => {}
        }
    }
    Err("The operation worker gave no version answer.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_events() {
        assert_eq!(
            Event::parse(r#"{"type":"stage","name":"fetch","index":2,"total":4}"#),
            Event::Stage(Stage {
                name: "fetch".into(),
                index: Some(2),
                total: Some(4),
                detail: None
            })
        );
        let e = Event::parse(
            r#"{"type":"result","ok":false,"error":{"kind":"cancelled","message":"stopped"}}"#,
        );
        match e {
            Event::Result(o) => assert_eq!(o.error.unwrap().kind, "cancelled"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn tolerates_unknown_types_and_fields() {
        assert!(matches!(
            Event::parse(r#"{"type":"future_thing","x":1}"#),
            Event::Unknown { .. }
        ));
        assert!(matches!(
            Event::parse(r#"{"type":"heartbeat","stage":"fetch","extra":true}"#),
            Event::Heartbeat(_)
        ));
    }

    #[test]
    fn flags_malformed_lines() {
        for line in [
            "Downloading foo",
            "{}",
            r#"{"type":5}"#,
            r#"{"type":"bytes"}"#,
        ] {
            assert!(
                matches!(Event::parse(line), Event::Malformed { .. }),
                "{line}"
            );
        }
    }

    #[test]
    fn version_check() {
        let ok = concat!(
            r#"{"type":"hello","seq":0,"elapsed_ms":0,"protocol":1,"tool_version":"0.2.0","command":"--version"}"#,
            "\n",
            r#"{"type":"result","seq":1,"elapsed_ms":1,"ok":true,"report":{"tool_version":"0.2.0","protocol":1},"error":null}"#
        );
        let v = check_version(ok).unwrap();
        assert_eq!(v.library.as_deref(), Some("0.2.0"));
        assert!(check_version("rust-mirror 0.1.1\n")
            .unwrap_err()
            .contains("unreadable"));
        let p2 = r#"{"type":"result","ok":true,"report":{"tool_version":"9.0.0","protocol":2}}"#;
        assert!(check_version(p2).unwrap_err().contains("protocol 2"));
        assert!(check_version("").unwrap_err().contains("no version"));
    }
}
