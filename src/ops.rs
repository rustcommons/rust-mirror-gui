//! Builds worker command lines and explains outcomes. No mirroring logic: every
//! operation runs in a `--worker` child process that calls the rust-mirror library.
use crate::job::Spec;
use crate::protocol::WORKER_ARG;
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Plan,
    CrateLoad,
    CratePaste,
    CrateSave,
    CratePreview,
    CrateSync,
    CrateExplain,
    CrateClientConfig,
    CrateDiscover,
    CrateRetentionPreview,
    CrateRetentionApply,
    LockPreview,
    LockWrite,
    CheckNetwork,
    Releases,
    NetworkEnv,
    InspectCa,
    Discover,
    RetentionPreview,
    RetentionApply,
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

/// Environment variable carrying the (non-secret) proxy username to the worker,
/// referenced by name through `--proxy-username-env`.
pub const PROXY_USER_ENV: &str = "RUST_MIRROR_GUI_PROXY_USER";

/// Add per-machine network settings to an upstream-facing command, using
/// rust-mirror 0.3.0's CLI flags (docs/NETWORK.md). The password, if any, goes
/// to the worker as the first stdin line (`--proxy-password-stdin`); it never
/// appears in argv, environment, logs or settings.
pub fn with_network(
    mut spec: Spec,
    net: &crate::settings::NetworkSettings,
    password: &str,
) -> Spec {
    use crate::settings::ProxyMode;
    let mut a: Vec<OsString> = Vec::new();
    match net.mode {
        ProxyMode::Direct => {}
        ProxyMode::Environment => a.push("--proxy-from-env".into()),
        ProxyMode::Manual => {
            a.extend(["--proxy".into(), net.proxy_url.trim().into()]);
            if !net.proxy_username.trim().is_empty() {
                spec.env
                    .push((PROXY_USER_ENV.into(), net.proxy_username.trim().into()));
                a.extend(["--proxy-username-env".into(), PROXY_USER_ENV.into()]);
            }
            if !password.is_empty() {
                a.push("--proxy-password-stdin".into());
                spec.stdin_secret = Some(password.to_string());
            }
        }
    }
    let no_proxy = crate::config::split_list(&net.no_proxy).join(",");
    if !no_proxy.is_empty() {
        a.extend(["--no-proxy".into(), no_proxy.into()]);
    }
    if !net.extra_ca_file.trim().is_empty() {
        a.extend(["--extra-ca-file".into(), net.extra_ca_file.trim().into()]);
    }
    if net.connect_timeout_seconds != 180 {
        a.extend([
            "--connect-timeout".into(),
            net.connect_timeout_seconds.to_string().into(),
        ]);
    }
    if net.read_timeout_seconds != 180 {
        a.extend([
            "--read-timeout".into(),
            net.read_timeout_seconds.to_string().into(),
        ]);
    }
    if let Some(cli) = spec.cli.as_mut().filter(|_| !a.is_empty()) {
        let flags: Vec<String> = a.iter().map(|f| f.to_string_lossy().into_owned()).collect();
        let flags = flags.join(" ");
        *cli = match cli.split_once(" (once per selection)") {
            Some((head, tail)) => format!("{head} {flags} (once per selection){tail}"),
            None => format!("{cli} {flags}"),
        };
    }
    spec.args.extend(a);
    spec
}

/// Load a crate catalog as JSON (`crates catalog`). Local; no network.
pub fn crates_load(program: &str, config: &str) -> Spec {
    let a: Vec<OsString> = vec![
        "crates".into(),
        "catalog".into(),
        "--config".into(),
        config.trim().into(),
    ];
    finish(program, "Load crate catalog", a, vec![])
}

/// Check a pasted `[dependencies]` block (`crates paste`). Local; no network.
pub fn crates_paste(program: &str, input: &std::path::Path) -> Spec {
    let a: Vec<OsString> = vec![
        "crates".into(),
        "paste".into(),
        "--input".into(),
        input.into(),
    ];
    let mut spec = finish(program, "Check pasted dependencies", a, vec![]);
    spec.cli = Some("rust-mirror crates paste --input FILE".into());
    spec
}

/// Preview (or, with `write`, commit) importing named Cargo.lock files into a
/// catalog (`crates import-lock`, rust-mirror 0.5.0). Local; no network.
pub fn crates_import_lock(
    program: &str,
    config: &str,
    inputs: &[(String, String)],
    write: bool,
) -> Spec {
    let mut a: Vec<OsString> = vec![
        "crates".into(),
        "import-lock".into(),
        "--config".into(),
        config.trim().into(),
    ];
    let mut cli = format!("rust-mirror crates import-lock --config {}", config.trim());
    for (name, path) in inputs {
        a.push("--lock".into());
        a.push(format!("{name}={}", path.trim()).into());
        cli.push_str(&format!(" --lock {name}={}", path.trim()));
    }
    if write {
        a.push("--write".into());
        cli.push_str(" --write");
    }
    let label = if write {
        "Import Cargo.lock into the catalog"
    } else {
        "Preview Cargo.lock import"
    };
    let mut spec = finish(program, label, a, vec![]);
    spec.cli = Some(cli);
    spec
}

/// Preview (or commit) removing named lock sources (`crates remove-lock`).
pub fn crates_remove_lock(program: &str, config: &str, names: &[String], write: bool) -> Spec {
    let mut a: Vec<OsString> = vec![
        "crates".into(),
        "remove-lock".into(),
        "--config".into(),
        config.trim().into(),
    ];
    let mut cli = format!("rust-mirror crates remove-lock --config {}", config.trim());
    for n in names {
        a.push("--source".into());
        a.push(n.into());
        cli.push_str(&format!(" --source {n}"));
    }
    if write {
        a.push("--write".into());
        cli.push_str(" --write");
    }
    let label = if write {
        "Remove lock source from the catalog"
    } else {
        "Preview lock source removal"
    };
    let mut spec = finish(program, label, a, vec![]);
    spec.cli = Some(cli);
    spec
}

/// Write an edited catalog (JSON) as canonical TOML through the library.
pub fn crates_save(program: &str, json: &std::path::Path, output: &str, replace: bool) -> Spec {
    let mut a: Vec<OsString> = vec![
        "crates-write".into(),
        "--input".into(),
        json.into(),
        "--output".into(),
        output.trim().into(),
    ];
    if replace {
        a.push("--replace".into());
    }
    let mut spec = finish(program, "Save crate catalog", a, vec![]);
    spec.cli = Some(format!(
        "(saves {}; edit the TOML file to do this by hand)",
        output.trim()
    ));
    spec
}

/// `crates sync`: without `approve` it writes a frozen preview into the new
/// output folder and returns `needs_approval`; with it, it downloads exactly
/// that saved preview.
pub fn crates_sync(
    program: &str,
    config: &str,
    output: &str,
    previous: &str,
    estimate: bool,
    approve: Option<&str>,
    resume: bool,
) -> Spec {
    let mut a: Vec<OsString> = vec![
        "crates".into(),
        "sync".into(),
        "--config".into(),
        config.trim().into(),
        "--output".into(),
        output.trim().into(),
    ];
    if !previous.trim().is_empty() {
        a.extend(["--previous".into(), previous.trim().into()]);
    }
    if let Some(id) = approve {
        a.extend(["--approve-plan".into(), id.into()]);
    }
    if resume {
        a.push("--resume".into());
    }
    if estimate && approve.is_none() {
        a.push("--estimate-sizes".into());
    }
    let label = if approve.is_some() {
        "Crate sync"
    } else {
        "Crate preview"
    };
    finish(program, label, a, vec![])
}

/// Why a crate version is included (`crates explain`). Local; no network.
pub fn crates_explain(program: &str, plan: &std::path::Path, name: &str, version: &str) -> Spec {
    let a: Vec<OsString> = vec![
        "crates".into(),
        "explain".into(),
        "--plan".into(),
        plan.into(),
        "--crate".into(),
        name.into(),
        "--version".into(),
        version.into(),
        "--limit".into(),
        "20".into(),
    ];
    finish(program, "Explain", a, vec![])
}

/// Crate retention preview (read-only). `catalog_json` holds the catalog with
/// the window's `crate-retention` policy.
pub fn crates_retention_preview(
    program: &str,
    catalog_json: &std::path::Path,
    directory: &str,
    next_previous: &str,
    as_of: &str,
) -> Spec {
    let a: Vec<OsString> = vec![
        "crates-retention-preview".into(),
        "--catalog-json".into(),
        catalog_json.into(),
        "--directory".into(),
        directory.trim().into(),
        "--next-previous".into(),
        next_previous.trim().into(),
        "--as-of".into(),
        as_of.trim().into(),
    ];
    let mut spec = finish(program, "Crate retention preview", a, vec![]);
    spec.cli = Some(format!(
        "rust-mirror crates prune --config CATALOG --directory {} --next-previous {} --as-of {} --dry-run",
        directory.trim(),
        next_previous.trim(),
        as_of.trim()
    ));
    spec
}

/// Apply the reviewed crate prune plan.
pub fn crates_retention_apply(program: &str, plan_file: &std::path::Path) -> Spec {
    let a: Vec<OsString> = vec![
        "crates-retention-apply".into(),
        "--plan-file".into(),
        plan_file.into(),
        "--allow-delete".into(),
    ];
    let mut spec = finish(program, "Remove crate generations", a, vec![]);
    spec.cli = Some("rust-mirror crates prune … --apply (applies the reviewed preview)".into());
    spec
}

/// Cargo client configuration for a completed crate generation (metadata only).
pub fn crates_client_config(program: &str, generation: &str) -> Spec {
    let a: Vec<OsString> = vec![
        "crates-client-config".into(),
        "--generation".into(),
        generation.trim().into(),
    ];
    let mut spec = finish(program, "Client setup", a, vec![]);
    spec.cli = Some("(see docs/CRATES.md: Cargo client setup)".into());
    spec
}

/// `check-network`: ordered dns/connect/proxy_auth/tls/download diagnostics.
pub fn check_network(
    program: &str,
    net: &crate::settings::NetworkSettings,
    password: &str,
) -> Spec {
    with_network(
        finish(
            program,
            "Connection test",
            vec!["check-network".into()],
            vec![],
        ),
        net,
        password,
    )
}

/// Release list from the per-machine cache; `refresh` fetches upstream first
/// (falling back to the cache), otherwise it never touches the network.
pub fn releases(
    program: &str,
    cache: &std::path::Path,
    refresh: bool,
    net: &crate::settings::NetworkSettings,
    password: &str,
) -> Spec {
    let mut a: Vec<OsString> = vec!["releases".into(), "--cache".into(), cache.into()];
    if refresh {
        let spec = finish(program, "Refresh release list", a, vec![]);
        return with_network(spec, net, password);
    }
    a.push("--offline".into());
    finish(program, "Load release list", a, vec![])
}

/// Sanitized environment proxy detection (no credential values).
pub fn network_env(program: &str) -> Spec {
    let mut spec = finish(
        program,
        "Detect environment proxy",
        vec!["network-env".into()],
        vec![],
    );
    spec.cli = Some("(shown by rust-mirror check-network --proxy-from-env)".into());
    spec
}

/// Certificate details of a PEM file. Diagnostic only; never changes trust.
pub fn inspect_ca(program: &str, file: &str) -> Spec {
    let mut spec = finish(
        program,
        "Read certificate",
        vec!["inspect-ca".into(), "--file".into(), file.trim().into()],
        vec![],
    );
    spec.cli = Some(format!("(certificate details of {})", file.trim()));
    spec
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
        cli: None,
        stdin_secret: None,
    }
}

pub fn generations(program: &str, directory: &str) -> Spec {
    let a: Vec<OsString> = vec![
        "generations".into(),
        "--directory".into(),
        directory.trim().into(),
    ];
    finish(program, "Discover generations", a, vec![])
}

pub fn retention_preview(
    program: &str,
    directory: &str,
    next_previous: &str,
    as_of: &str,
    policy_json: &str,
) -> Spec {
    let a: Vec<OsString> = vec![
        "retention-preview".into(),
        "--directory".into(),
        directory.trim().into(),
        "--next-previous".into(),
        next_previous.trim().into(),
        "--as-of".into(),
        as_of.trim().into(),
        "--policy".into(),
        policy_json.into(),
    ];
    let mut spec = finish(program, "Retention preview", a, vec![]);
    spec.cli = Some(format!(
        "rust-mirror prune --config FILE-WITH-[retention] --as-of {} --directory {} --next-previous {} --dry-run",
        as_of.trim(),
        directory.trim(),
        next_previous.trim()
    ));
    spec
}

pub fn retention_apply(program: &str, plan_file: &std::path::Path) -> Spec {
    let a: Vec<OsString> = vec![
        "retention-apply".into(),
        "--plan-file".into(),
        plan_file.into(),
        "--allow-delete".into(),
    ];
    let mut spec = finish(program, "Remove generations", a, vec![]);
    spec.cli = Some("rust-mirror prune … --apply (applies the reviewed preview)".into());
    spec
}

pub fn plan(program: &str, config: &str, estimate_sizes: bool) -> Spec {
    let mut a: Vec<OsString> = vec!["plan-all".into(), "--config".into(), config.trim().into()];
    if estimate_sizes {
        a.push("--estimate-sizes".into());
    }
    let mut spec = finish(program, "Plan preview", a, vec![]);
    spec.cli = Some(format!(
        "rust-mirror plan --config {} --selection NAME --output NEW_PLAN_DIRECTORY{} (once per selection)",
        config.trim(),
        if estimate_sizes { " --estimate-sizes" } else { "" }
    ));
    spec
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
/// Same as [`publish`] but only checks local content, TLS, credentials and
/// remote absence (`publish --preflight`); nothing is written.
pub fn publish_preflight(
    program: &str,
    source: &str,
    url: &str,
    ca: &str,
    user: &str,
    password: &str,
) -> Spec {
    let mut spec = publish(program, source, url, ca, user, password);
    spec.args.insert(2, "--preflight".into());
    spec.label = "Publish check".into();
    spec
}

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
        (Op::Discover, _) | (Op::RetentionPreview, _) => "Nothing was deleted. Check the message, then try again.",
        (Op::RetentionApply, _) => {
            "Deletion stopped. Files removed before the stop stay removed and there is no rollback. Discover again and inspect the folder before retrying."
        }
        (Op::CheckNetwork, _) => "See each step below for what failed and how to fix it.",
        (Op::Releases, _) => "The release list could not be loaded. You can still type a toolchain such as 1.90.0.",
        (Op::NetworkEnv, _) | (Op::InspectCa, _) => "Nothing was changed.",
        (Op::CratePreview, "budget") => "The preview is larger than the catalog's limits. Raise the limits under Policy or select fewer crates.",
        (Op::CratePreview, _) | (Op::CrateLoad, _) | (Op::CratePaste, _) | (Op::CrateSave, _) | (Op::CrateExplain, _) | (Op::CrateClientConfig, _) => {
            "Nothing was downloaded. Check the message above."
        }
        (Op::CrateRetentionPreview, "invalid") => "Nothing was deleted. If the message names a Cargo.lock source, no finished crate mirror holds all of its versions yet: sync a crate mirror first, or remove that source from the catalog.",
        (Op::CrateDiscover, _) | (Op::CrateRetentionPreview, _) => "Nothing was deleted. Check the message above.",
        (Op::LockPreview, _) => "Nothing was changed. The summary below lists each problem with its line; fix or leave out those files, then preview again.",
        (Op::LockWrite, _) => "The catalog file was not changed: rust-mirror writes it all at once or not at all. Preview again.",
        (Op::CrateRetentionApply, "resume_mismatch") => {
            "Nothing was deleted: the folders changed since the preview. Preview again, check it, then delete."
        }
        (Op::CrateRetentionApply, _) => {
            "Deletion stopped. rust-mirror re-checks the folder before deleting, so a refusal usually means nothing was removed; if it stopped partway, removed folders stay removed. Press Discover to see what is left, fix the folder, then preview again."
        }
        (Op::CrateSync, "invalid") => "The approval did not match the saved preview. Preview again into a new folder.",
        (Op::CrateSync, "resume_mismatch") => "The catalog or previous mirror changed since the preview. Preview again into a new folder.",
        (Op::CrateSync, "cancelled") | (Op::CrateSync, "killed") | (Op::CrateSync, "no_result") | (Op::CrateSync, "network") => {
            "Verified downloads are kept. Press \u{201c}Resume download\u{201d} to continue the same approved preview."
        }
        (Op::Plan, _) => "A preview only reads metadata and writes nothing. Check the message, then preview again.",
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

    #[test]
    fn network_settings_become_flags_and_password_goes_to_stdin_only() {
        use crate::settings::{NetworkSettings, ProxyMode};
        let net = NetworkSettings {
            mode: ProxyMode::Manual,
            proxy_url: "http://proxy.corp.example:3128".into(),
            proxy_username: "alice".into(),
            no_proxy: "localhost, .corp.example".into(),
            extra_ca_file: "C:/certs/root.pem".into(),
            connect_timeout_seconds: 30,
            read_timeout_seconds: 180,
        };
        let s = check_network("gui", &net, "s3cret!");
        let a = args(&s);
        assert!(!a.iter().any(|x| x.contains("s3cret")));
        assert!(!s.env.iter().any(|(_, v)| v.contains("s3cret")));
        assert!(!s.display().contains("s3cret"));
        assert_eq!(s.stdin_secret.as_deref(), Some("s3cret!"));
        for want in [
            "--proxy",
            "--proxy-password-stdin",
            "--proxy-username-env",
            "--no-proxy",
            "--extra-ca-file",
            "--connect-timeout",
        ] {
            assert!(a.iter().any(|x| x == want), "{want}");
        }
        assert!(!a.iter().any(|x| x == "--read-timeout"));
        assert!(a.contains(&"localhost,.corp.example".to_string()));
        assert_eq!(
            s.env,
            vec![(PROXY_USER_ENV.to_string(), "alice".to_string())]
        );
        let direct = check_network("gui", &NetworkSettings::default(), "ignored");
        assert_eq!(args(&direct), ["--worker", "check-network"]);
        assert!(direct.stdin_secret.is_none());
        let env = check_network(
            "gui",
            &NetworkSettings {
                mode: ProxyMode::Environment,
                ..Default::default()
            },
            "x",
        );
        assert!(args(&env).contains(&"--proxy-from-env".to_string()) && env.stdin_secret.is_none());
    }

    #[test]
    fn offline_release_list_has_no_network_flags() {
        use crate::settings::{NetworkSettings, ProxyMode};
        let net = NetworkSettings {
            mode: ProxyMode::Manual,
            proxy_url: "http://p:1".into(),
            ..Default::default()
        };
        let offline = releases("gui", std::path::Path::new("/c"), false, &net, "pw");
        assert!(args(&offline).contains(&"--offline".to_string()));
        assert!(!args(&offline).contains(&"--proxy".to_string()) && offline.stdin_secret.is_none());
        let refresh = releases("gui", std::path::Path::new("/c"), true, &net, "pw");
        assert!(args(&refresh).contains(&"--proxy".to_string()) && refresh.stdin_secret.is_some());
    }

    #[test]
    fn crate_sync_preview_then_approval() {
        let preview = crates_sync("gui", "c.toml", "run", "", true, None, false);
        let a = args(&preview);
        assert!(
            a.contains(&"--estimate-sizes".to_string())
                && !a.contains(&"--approve-plan".to_string())
        );
        assert_eq!(&a[1..3], ["crates", "sync"]);
        let approve = crates_sync(
            "gui",
            "c.toml",
            "run",
            "prev/mirror",
            true,
            Some("abc"),
            true,
        );
        let a = args(&approve);
        assert!(a.windows(2).any(|w| w == ["--approve-plan", "abc"]));
        assert!(a.contains(&"--resume".to_string()) && a.contains(&"--previous".to_string()));
        // Approval replays the frozen plan; size estimation is a preview option.
        assert!(!a.contains(&"--estimate-sizes".to_string()));
    }

    #[test]
    fn lock_import_and_removal_command_lines() {
        let inputs = vec![
            ("svc-a".to_string(), "C:\\src\\a\\Cargo.lock".to_string()),
            ("svc-b".to_string(), "/src/b/Cargo.lock".to_string()),
        ];
        let spec = crates_import_lock("gui", "crates.toml", &inputs, false);
        let a: Vec<String> = spec
            .args
            .iter()
            .map(|x| x.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            &a[1..],
            [
                "crates",
                "import-lock",
                "--config",
                "crates.toml",
                "--lock",
                "svc-a=C:\\src\\a\\Cargo.lock",
                "--lock",
                "svc-b=/src/b/Cargo.lock"
            ]
        );
        let spec = crates_import_lock("gui", "crates.toml", &inputs, true);
        assert_eq!(spec.args.last().unwrap(), "--write");
        let spec = crates_remove_lock("gui", "crates.toml", &["svc-a".to_string()], false);
        let a: Vec<String> = spec
            .args
            .iter()
            .map(|x| x.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            &a[1..],
            [
                "crates",
                "remove-lock",
                "--config",
                "crates.toml",
                "--source",
                "svc-a"
            ]
        );
    }
}
