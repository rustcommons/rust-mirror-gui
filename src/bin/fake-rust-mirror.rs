//! Test stand-in for the rust-mirror CLI. NOT a mirror tool.
//!
//! It speaks the (provisional) event protocol from scripted scenarios, writes no
//! files and uses no network. Tests and GUI demos select behaviour with:
//!
//! - `FAKE_SCENARIO`: `ok` (default), `fail`, `stall`, `quiet-stage`,
//!   `ignore-cancel`, `garbage`, `crash`, `legacy`, `protocol2`
//! - `FAKE_STEP_MS`: delay between steps in ms (default 300)
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn emit(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

fn main() {
    // Accept the worker calling convention: `--worker COMMAND …`.
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| a != "--worker")
        .collect();
    let scenario = std::env::var("FAKE_SCENARIO").unwrap_or_else(|_| "ok".into());
    let step = Duration::from_millis(
        std::env::var("FAKE_STEP_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300),
    );
    if args.first().map(String::as_str) == Some("--version") {
        match scenario.as_str() {
            "legacy" => println!("not json"),
            "protocol2" => {
                println!(
                    r#"{{"type":"hello","seq":0,"elapsed_ms":0,"protocol":2,"tool_version":"9.0.0","command":"--version"}}"#
                );
                println!(
                    r#"{{"type":"result","seq":1,"elapsed_ms":0,"ok":true,"report":{{"tool_version":"9.0.0","protocol":2}},"error":null}}"#
                );
            }
            _ => {
                println!(
                    r#"{{"type":"hello","seq":0,"elapsed_ms":0,"protocol":1,"tool_version":"0.0.0-fake","command":"--version"}}"#
                );
                println!(
                    r#"{{"type":"result","seq":1,"elapsed_ms":0,"ok":true,"report":{{"tool_version":"0.0.0-fake","protocol":1}},"error":null}}"#
                );
            }
        }
        return;
    }
    let command = args.first().cloned().unwrap_or_default();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    // `--proxy-password-stdin`: consume one line first, as rust-mirror does.
    // Only its presence is reported; the value is never printed.
    let mut password_received = false;
    if has("--proxy-password-stdin") {
        let mut line = String::new();
        password_received =
            std::io::stdin().lock().read_line(&mut line).is_ok() && !line.trim().is_empty();
    }

    let cancel = Arc::new(AtomicBool::new(false));
    if scenario != "ignore-cancel" {
        let flag = cancel.clone();
        std::thread::spawn(move || {
            let stdin = std::io::stdin();
            let mut line = String::new();
            loop {
                line.clear();
                match stdin.lock().read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) if line.contains("cancel") => break,
                    Ok(_) => {}
                }
            }
            flag.store(true, Ordering::SeqCst);
        });
    }
    let alive = Arc::new(AtomicBool::new(true));
    let stage_name = Arc::new(std::sync::Mutex::new(String::from("start")));
    {
        let alive = alive.clone();
        let stage_name = stage_name.clone();
        std::thread::spawn(move || {
            while alive.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(500));
                if alive.load(Ordering::SeqCst) {
                    let s = stage_name.lock().unwrap().clone();
                    emit(&format!(r#"{{"type":"heartbeat","stage":"{s}"}}"#));
                }
            }
        });
    }
    emit(&format!(
        r#"{{"type":"hello","protocol":1,"tool_version":"0.0.0-fake","command":"{command}","heartbeat_ms":500}}"#
    ));
    eprintln!(
        "fake-rust-mirror: simulating `{command}` (scenario {scenario}); no files are written"
    );
    if let Some((ok, body)) = network_command(&command, &args, password_received, step) {
        alive.store(false, Ordering::SeqCst);
        if ok {
            emit(&format!(
                r#"{{"type":"result","ok":true,"report":{body},"error":null}}"#
            ));
            std::process::exit(0);
        }
        emit(&format!(
            r#"{{"type":"result","ok":false,"report":null,"error":{body}}}"#
        ));
        std::process::exit(1);
    }
    let estimate = has("--estimate-sizes");
    let total_expected: u64 = 87_000_000 + 9_800_000 + 21_000_000 + 13_500_000;
    let mut completed: u64 = 0;

    let stages: &[&str] = match command.as_str() {
        "publish" => &[
            "validate local generation",
            "check remote paths",
            "upload",
            "verify remote bytes",
        ],
        "rustup" => &["read snapshot", "fetch installers", "write generation"],
        _ => &["verify previous", "resolve", "fetch", "verify and assemble"],
    };
    let files = [
        (
            "dist/2025-09-18/rustc-1.90.0-x86_64-pc-windows-msvc.tar.xz",
            87_000_000u64,
        ),
        (
            "dist/2025-09-18/cargo-1.90.0-x86_64-pc-windows-msvc.tar.xz",
            9_800_000,
        ),
        (
            "dist/2025-09-18/rust-std-1.90.0-x86_64-pc-windows-msvc.tar.xz",
            21_000_000,
        ),
        (
            "rustup/archive/1.28.2/x86_64-pc-windows-msvc/rustup-init.exe",
            13_500_000,
        ),
    ];
    let started = Instant::now();
    let finish = |ok: bool, kind: &str, message: &str| {
        alive.store(false, Ordering::SeqCst);
        if ok {
            emit(&format!(
                r#"{{"type":"result","ok":true,"report":{{"status":"created","files":{},"elapsed_ms":{}}}}}"#,
                files.len(),
                started.elapsed().as_millis()
            ));
            std::process::exit(0);
        }
        emit(&format!(
            r#"{{"type":"result","ok":false,"error":{{"kind":"{kind}","message":"{message}"}}}}"#
        ));
        std::process::exit(1);
    };
    let check_cancel = || {
        if cancel.load(Ordering::SeqCst) {
            finish(
                false,
                "cancelled",
                "stopped at a safe point; progress is kept for --resume",
            );
        }
    };

    for (i, name) in stages.iter().enumerate() {
        *stage_name.lock().unwrap() = name.to_string();
        emit(&format!(
            r#"{{"type":"stage","name":"{name}","index":{},"total":{}}}"#,
            i + 1,
            stages.len()
        ));
        if scenario == "garbage" && i == 0 {
            emit("Downloading something (plain text, not an event)");
            emit(r#"{"type":"bytes"}"#);
            emit(r#"{"type":"from_the_future","x":1}"#);
        }
        if i == 2 || (command == "rustup" && i == 1) {
            for (path, size) in files {
                let action = if path.contains("cargo") {
                    "reuse"
                } else {
                    "download"
                };
                emit(&format!(
                    r#"{{"type":"artifact","path":"{path}","action":"{action}","offset":0,"expected_bytes":{size}}}"#
                ));
                let chunks = if action == "reuse" { 1 } else { 10 };
                for c in 1..=chunks {
                    check_cancel();
                    std::thread::sleep(step);
                    emit(&format!(
                        r#"{{"type":"bytes","path":"{path}","count":{},"total":{size}}}"#,
                        size * c / chunks
                    ));
                    completed += size / chunks;
                    if estimate {
                        let rate = 25_000_000.0f64;
                        let eta = (total_expected.saturating_sub(completed)) as f64 / rate;
                        emit(&format!(
                            r#"{{"type":"transfer_summary","completed_bytes":{completed},"transferred_bytes":{completed},"expected_bytes":{total_expected},"bytes_per_second":{rate},"eta_seconds":{eta:.1},"estimated":true}}"#
                        ));
                    }
                    if scenario == "fail" && path.contains("rust-std") && c == 5 {
                        finish(
                            false,
                            "network",
                            "connection reset while downloading rust-std (simulated)",
                        );
                    }
                    if scenario == "crash" && path.contains("rust-std") && c == 5 {
                        eprintln!("fake-rust-mirror: simulated crash");
                        std::process::exit(3);
                    }
                    if scenario == "stall" && path.contains("rust-std") && c == 5 {
                        // Stop all output, heartbeats included, as if the process hung.
                        alive.store(false, Ordering::SeqCst);
                        loop {
                            std::thread::sleep(Duration::from_secs(3600));
                        }
                    }
                }
                emit(&format!(
                    r#"{{"type":"hashing","path":"{path}","count":{size},"total":{size}}}"#
                ));
            }
        }
        if scenario == "quiet-stage" && i == 3 {
            // Heartbeats continue, but the stage makes no visible progress.
            loop {
                check_cancel();
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        check_cancel();
        std::thread::sleep(step * 3);
    }
    finish(true, "", "");
}

/// Scripted rust-mirror 0.3.0 network operations (`FAKE_NET`: ok, tls, proxy_auth, dns).
/// Returns (ok, report-or-error JSON). Shapes follow rust-mirror docs/NETWORK.md.
fn network_command(
    command: &str,
    args: &[String],
    password: bool,
    step: Duration,
) -> Option<(bool, String)> {
    let net = std::env::var("FAKE_NET").unwrap_or_else(|_| "ok".into());
    let proxy = args
        .iter()
        .any(|a| a == "--proxy" || a == "--proxy-from-env");
    match command {
        "network-env" => Some((true, r#"{"proxy_url":"http://proxy.corp.example:3128","no_proxy":["localhost",".corp.example"],"credentials_present":false}"#.into())),
        "inspect-ca" => Some((true, r#"{"certificates":[{"subject":"CN=Example Corp Inspection Root CA, O=Example Corp","issuer":"CN=Example Corp Inspection Root CA, O=Example Corp","not_after":"2031-06-30T23:59:59Z","sha256":"3f1c9a0b5e7d2c48a9e0b1f6c3d2e4a5b6c7d8e9f0a1b2c3d4e5f60718293a4b"}]}"#.into())),
        "releases" => {
            emit(r#"{"type":"stage","name":"releases","index":1,"total":1}"#);
            std::thread::sleep(step);
            let offline = args.iter().any(|a| a == "--offline");
            Some((true, format!(
                r#"{{"releases":[{{"channel":"stable","version":"1.91.1","date":"2025-11-10"}},{{"channel":"stable","version":"1.91.0","date":"2025-10-30"}},{{"channel":"stable","version":"1.90.0","date":"2025-09-18"}},{{"channel":"stable","version":"1.89.0","date":"2025-08-07"}},{{"channel":"stable","version":"1.88.0","date":"2025-06-26"}},{{"channel":"beta","date":"2025-11-08"}},{{"channel":"beta","date":"2025-11-01"}},{{"channel":"nightly","date":"2025-11-10"}},{{"channel":"nightly","date":"2025-11-09"}}],"source_url":"https://static.rust-lang.org/manifests.txt","cached":{offline},"fetched_at":"2025-11-10T08:00:00Z","age_seconds":{}}}"#,
                if offline { 7200 } else { 0 }
            )))
        }
        "check-network" => {
            let names = ["dns", "connect", "proxy_auth", "tls", "download"];
            let fail_at = match net.as_str() {
                "dns" => Some(0),
                "proxy_auth" if proxy => Some(2),
                "tls" => Some(3),
                _ => None,
            };
            let mut steps = Vec::new();
            for (i, name) in names.iter().enumerate() {
                emit(&format!(r#"{{"type":"stage","name":"check {name}","index":{},"total":5}}"#, i + 1));
                std::thread::sleep(step);
                let (status, kind, msg, certs) = match fail_at {
                    Some(f) if i == f => match *name {
                        "dns" => ("failed", "\"network\"", "could not resolve static.rust-lang.org".to_string(), "[]"),
                        "proxy_auth" => ("failed", "\"auth\"", format!("the proxy answered 407 Proxy Authentication Required (password supplied: {password})"), "[]"),
                        _ => ("failed", "\"tls\"", "the certificate presented for static.rust-lang.org is not trusted (unknown issuer)".to_string(),
                              r#"[{"subject":"CN=static.rust-lang.org","issuer":"CN=Example Corp Inspection Root CA, O=Example Corp","not_after":"2026-12-01T00:00:00Z","sha256":"9a8b7c6d5e4f30211203f4e5d6c7b8a9901a2b3c4d5e6f708192a3b4c5d6e7f8"}]"#),
                    },
                    Some(f) if i > f => ("skipped", "null", "not run because an earlier step failed".to_string(), "[]"),
                    _ if *name == "proxy_auth" && !proxy => ("skipped", "null", "direct connection: no proxy".to_string(), "[]"),
                    _ => ("passed", "null", match *name {
                        "dns" => "static.rust-lang.org resolved".to_string(),
                        "connect" => if proxy { "connected through the proxy".to_string() } else { "connected directly".to_string() },
                        "proxy_auth" => "the proxy accepted the connection".to_string(),
                        "tls" => "certificate trusted".to_string(),
                        _ => "downloaded dist/channel-rust-stable.toml.sha256".to_string(),
                    }, "[]"),
                };
                steps.push(format!(r#"{{"name":"{name}","status":"{status}","error_kind":{kind},"message":"{msg}","certificates":{certs}}}"#));
            }
            let report = format!(r#"{{"ok":{},"steps":[{}]}}"#, fail_at.is_none(), steps.join(","));
            if fail_at.is_none() {
                Some((true, report))
            } else {
                let kind = match net.as_str() { "dns" => "network", "proxy_auth" => "auth", _ => "tls" };
                Some((false, format!(r#"{{"kind":"{kind}","message":"network check failed","report":{report}}}"#)))
            }
        }
        "plan-all" => {
            emit(r#"{"type":"stage","name":"resolve windows-dev","index":1,"total":1}"#);
            std::thread::sleep(step);
            let sizes = if args.iter().any(|a| a == "--estimate-sizes") {
                r#","sizes":{"artifacts":[{"path":"dist/2025-09-18/rustc-1.90.0-x86_64-pc-windows-msvc.tar.xz","expected_bytes":87000000},{"path":"dist/2025-09-18/cargo-1.90.0-x86_64-pc-windows-msvc.tar.xz","expected_bytes":9800000},{"path":"rustup/archive/1.28.2/x86_64-pc-windows-msvc/rustup-init.exe","expected_bytes":null}],"known_bytes":96800000,"unknown_artifacts":1,"complete":false,"exceeds_budget":false,"estimated":true}"#
            } else { "" };
            Some((true, format!(
                r#"{{"plans":[{{"selection":{{"name":"windows-dev","toolchain":"1.90.0"}},"pinned_toolchain":"1.90.0","resolved_release":"1.90.0","date":"2025-09-18","max_download_bytes":10737418240,"artifacts":[{{"path":"dist/2025-09-18/rustc-1.90.0-x86_64-pc-windows-msvc.tar.xz","sha256":"a18b3912905f","package":"rustc","target":"x86_64-pc-windows-msvc","kind":"component"}},{{"path":"dist/2025-09-18/cargo-1.90.0-x86_64-pc-windows-msvc.tar.xz","sha256":"a238d6a87cb0","package":"cargo","target":"x86_64-pc-windows-msvc","kind":"component"}},{{"path":"rustup/archive/1.28.2/x86_64-pc-windows-msvc/rustup-init.exe","sha256":"88d8258dcf6a","package":"rustup-init","target":"x86_64-pc-windows-msvc","kind":"bootstrap"}}]{sizes}}}]}}"#
            )))
        }
        _ => None,
    }
}
