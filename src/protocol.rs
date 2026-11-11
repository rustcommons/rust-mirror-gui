//! Client side of the rust-mirror JSON-lines event protocol.
//!
//! PROVISIONAL: rust-mirror 0.2.0 defines the protocol in its `docs/EVENTS.md`.
//! Until that spec is on rust-mirror's main branch, the shapes here follow the
//! GUI's request to the rust-mirror maintainers and are exercised only against
//! the fake backend. Parsing is deliberately tolerant: unknown event types and
//! unknown fields are kept and shown, never fatal.

use serde::Deserialize;
use serde_json::Value;

/// Protocol major version this GUI understands.
pub const PROTOCOL_MAJOR: u64 = 1;
/// Oldest rust-mirror release expected to speak the protocol.
pub const MIN_TOOL_VERSION: (u64, u64, u64) = (0, 2, 0);
/// Option that switches a rust-mirror command to JSON lines on stdout.
pub const EVENTS_ARGS: &[&str] = &["--events", "jsonl"];
/// Arguments that ask rust-mirror for its version and protocol, offline.
pub const VERSION_ARGS: &[&str] = &["--version", "--json"];
/// Line written to the backend's stdin to ask for a graceful stop.
pub const CANCEL_LINE: &str = "{\"type\":\"cancel\"}\n";
/// Heartbeat interval assumed when `hello` does not state one.
pub const DEFAULT_HEARTBEAT_MS: u64 = 2000;
/// Longest stdout line accepted from the backend.
pub const MAX_LINE: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Hello {
    pub protocol: u64,
    #[serde(default)]
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
    /// Cumulative bytes for this path in this run.
    pub bytes: u64,
    #[serde(default)]
    pub total: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Heartbeat {
    #[serde(default)]
    pub stage: Option<String>,
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

/// Version information reported by `rust-mirror --version --json`.
#[derive(Debug, Clone, PartialEq)]
pub struct BackendVersion {
    pub tool: String,
    pub protocol: u64,
}

/// Parse the discovery output and check compatibility. The error text is shown
/// to the user as is.
pub fn check_version(stdout: &str) -> Result<BackendVersion, String> {
    let first = stdout.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let value: Value = serde_json::from_str(first).map_err(|_| {
        let shown: String = first.chars().take(80).collect();
        format!(
            "This rust-mirror does not report an event protocol (it printed \"{shown}\"). \
             rust-mirror 0.2.0 or newer is required."
        )
    })?;
    let tool = value
        .get("tool")
        .or_else(|| value.get("version"))
        .and_then(Value::as_str)
        .ok_or("Version output has no tool version.")?
        .to_owned();
    let protocol = value
        .get("protocol")
        .and_then(Value::as_u64)
        .ok_or("Version output has no protocol version.")?;
    let parsed =
        parse_semver(&tool).ok_or_else(|| format!("Unreadable tool version \"{tool}\"."))?;
    if parsed < MIN_TOOL_VERSION {
        return Err(format!(
            "rust-mirror {tool} is too old. Version {}.{}.{} or newer is required.",
            MIN_TOOL_VERSION.0, MIN_TOOL_VERSION.1, MIN_TOOL_VERSION.2
        ));
    }
    if protocol != PROTOCOL_MAJOR {
        return Err(format!(
            "rust-mirror {tool} speaks event protocol {protocol}; this GUI supports protocol {PROTOCOL_MAJOR}. \
             Use a matching rust-mirror or GUI version."
        ));
    }
    Ok(BackendVersion { tool, protocol })
}

fn parse_semver(text: &str) -> Option<(u64, u64, u64)> {
    let core = text.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    Some((parts.next()??, parts.next()??, parts.next()??))
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
        assert!(check_version(r#"{"tool":"0.2.0","protocol":1}"#).is_ok());
        assert!(check_version("rust-mirror 0.1.1\n")
            .unwrap_err()
            .contains("0.2.0"));
        assert!(check_version(r#"{"tool":"0.1.9","protocol":1}"#)
            .unwrap_err()
            .contains("too old"));
        assert!(check_version(r#"{"tool":"0.3.0","protocol":2}"#)
            .unwrap_err()
            .contains("protocol 2"));
    }
}
