//! Builds worker command lines and explains outcomes. No mirroring logic: every
//! operation runs in a `--worker` child process that calls the rust-mirror library.
use crate::job::Spec;
use crate::protocol::WORKER_ARG;
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Sync,
    Rustup,
    Publish,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncMode {
    New,
    Resume,
    Force,
}

/// The executable that runs operations: this program itself. Developers and
/// tests may point `RUST_MIRROR_GUI_WORKER` at a stand-in such as the fake backend.
pub fn worker_program() -> String {
    if let Some(p) = std::env::var_os("RUST_MIRROR_GUI_WORKER") {
        return p.to_string_lossy().into_owned();
    }
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn finish(program: &str, label: &str, mut args: Vec<OsString>, env: Vec<(String, String)>) -> Spec {
    args.insert(0, WORKER_ARG.into());
    Spec {
        program: PathBuf::from(program.trim()),
        args,
        env,
        label: label.into(),
    }
}

pub fn sync(program: &str, config: &str, output: &str, previous: &str, mode: SyncMode) -> Spec {
    let mut a: Vec<OsString> = vec![
        "sync".into(),
        "--config".into(),
        config.trim().into(),
        "--output".into(),
        output.trim().into(),
    ];
    if !previous.trim().is_empty() {
        a.extend(["--previous".into(), previous.trim().into()]);
    }
    match mode {
        SyncMode::New => {}
        SyncMode::Resume => a.push("--resume".into()),
        SyncMode::Force => a.push("--force".into()),
    }
    finish(program, "Sync", a, vec![])
}

pub fn rustup(program: &str, snapshot: &str, version: &str, output: &str, promote: &str) -> Spec {
    let mut a: Vec<OsString> = vec![
        "rustup".into(),
        "--snapshot".into(),
        snapshot.trim().into(),
        "--version".into(),
        version.trim().into(),
        "--output".into(),
        output.trim().into(),
    ];
    for channel in crate::config::split_list(promote) {
        a.extend(["--promote".into(), channel.into()]);
    }
    finish(program, "Rustup finish", a, vec![])
}

/// Credentials go to the child's environment only (the variables the
/// rust-mirror CLI documents), never into arguments, logs or settings.
pub fn publish(
    program: &str,
    source: &str,
    url: &str,
    ca: &str,
    user: &str,
    password: &str,
) -> Spec {
    let mut a: Vec<OsString> = vec![
        "publish".into(),
        "--source".into(),
        source.trim().into(),
        "--base-url".into(),
        url.trim().into(),
    ];
    if !ca.trim().is_empty() {
        a.extend(["--ca-file".into(), ca.trim().into()]);
    }
    let env = if user.is_empty() && password.is_empty() {
        vec![]
    } else {
        vec![
            ("NEXUS_USERNAME".into(), user.into()),
            ("NEXUS_PASSWORD".into(), password.into()),
        ]
    };
    finish(program, "Publish", a, env)
}

/// What to do next after a stop or failure, in plain words.
pub fn advice(op: Op, kind: &str) -> &'static str {
    match (op, kind) {
        (Op::Publish, "cancelled") | (Op::Publish, "killed") | (Op::Publish, "no_result") => {
            "Publishing was interrupted. The server may hold some files. Do not activate this repository; publish into a new, empty one."
        }
        (Op::Publish, "remote_exists") => "The destination already has files. Publish only into a new, empty repository or prefix.",
        (Op::Publish, "auth") => "The server refused the credentials. Check the user name, password and its upload rights.",
        (Op::Publish, "tls") => "The server certificate was not trusted. Choose the company CA bundle (PEM) and try again.",
        (Op::Publish, _) => "Nothing is activated by publishing. If any file was uploaded, use a new repository for the next attempt.",
        (Op::Rustup, "cancelled") | (Op::Rustup, "killed") | (Op::Rustup, "no_result") => {
            "The Rustup step does not resume. Delete the incomplete output folder, then run it again."
        }
        (_, "cancelled") | (_, "killed") | (_, "no_result") => {
            "Downloaded and verified files are kept. To continue, choose “Resume” with the same configuration, output and previous mirror, then start again."
        }
        (_, "output_exists") => "The output folder already exists. Choose “Resume” to continue it, or pick a new folder.",
        (_, "resume_mismatch") => "The configuration or previous mirror differs from the interrupted run. Restore them, or choose “Start over”.",
        (_, "locked") => "Another rust-mirror process is using this output folder. Wait for it to finish.",
        (_, "network") => "A network error stopped the run. Check the connection, then choose “Resume” and start again.",
        (_, "integrity") => "A file did not match its expected checksum and was discarded. Choose “Resume” to download it again.",
        (_, "budget") => "The download budget was exceeded. Raise the budget in the configuration or select fewer items.",
        (_, "invalid") => "rust-mirror rejected the input. The message above says which value to change.",
        _ => "See the message above and the activity log for details.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &Spec) -> Vec<String> {
        s.args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn sync_args() {
        let s = sync("rust-mirror", "m.toml", "run-2", "", SyncMode::Resume);
        assert_eq!(
            args(&s),
            ["--worker", "sync", "--config", "m.toml", "--output", "run-2", "--resume"]
        );
        let s = sync(
            "rust-mirror",
            "m.toml",
            "run-2",
            "run-1/mirror",
            SyncMode::Force,
        );
        assert!(
            args(&s).contains(&"--previous".to_string())
                && args(&s).contains(&"--force".to_string())
        );
    }

    #[test]
    fn publish_keeps_credentials_out_of_arguments() {
        let s = publish("rust-mirror", "m", "https://x/", "", "u", "p@ss");
        assert!(!args(&s).iter().any(|a| a.contains("p@ss")));
        assert_eq!(s.env.len(), 2);
        assert!(publish("rust-mirror", "m", "https://x/", "", "", "")
            .env
            .is_empty());
    }

    #[test]
    fn rustup_promotes_each_channel() {
        let s = rustup(
            "rust-mirror",
            "run/mirror",
            "1.28.2",
            "complete",
            "stable-2025-09-18, beta-2025-09-18",
        );
        assert_eq!(args(&s).iter().filter(|a| *a == "--promote").count(), 2);
    }
}
