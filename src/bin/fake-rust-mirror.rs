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
            "protocol2" => println!(r#"{{"tool":"0.9.0","protocol":2}}"#),
            _ => println!(r#"{{"tool":"0.0.0-fake","library":"none (fake)","protocol":1}}"#),
        }
        return;
    }
    let command = args.first().cloned().unwrap_or_default();

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
        r#"{{"type":"hello","protocol":1,"tool":"0.2.0-fake","command":"{command}","heartbeat_ms":500}}"#
    ));
    eprintln!(
        "fake-rust-mirror: simulating `{command}` (scenario {scenario}); no files are written"
    );

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
                        r#"{{"type":"bytes","path":"{path}","bytes":{},"total":{size}}}"#,
                        size * c / chunks
                    ));
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
                    r#"{{"type":"artifact","path":"{path}","action":"verified"}}"#
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
