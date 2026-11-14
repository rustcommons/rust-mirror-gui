//! End-to-end crate workflow qualification against real crates.io, driven
//! through the window's own command builders (`ops`) and process client
//! (`Job`), which start `rust-mirror-gui --worker` exactly as the window does.
//!
//! Ignored by default (network, minutes). The Windows qualification workflow
//! runs it in release mode:
//!
//! ```text
//! cargo test --release --locked --test windows_qualification -- --ignored --nocapture
//! ```
//!
//! Environment:
//! - `QUAL_DIR`: an empty work folder; evidence and outputs go here;
//! - `QUAL_LOCK`: the Cargo.lock to import;
//! - `QUAL_PUBLISH_URL`: the publish base URL (also the catalog's publish
//!   address), ending in `/`;
//! - `QUAL_CA_FILE`: optional extra CA (PEM) for the publish destination;
//! - `QUAL_PUBLISH_USER` / `QUAL_PUBLISH_PASSWORD`: publish credentials. They
//!   go to the worker's environment only, as in the window, and are never
//!   written to the evidence.
//!
//! Writes `QUAL_DIR/evidence.json` (one entry per step) and
//! `QUAL_DIR/cargo-config.toml` (the Client setup snippet).
use rust_mirror_gui::job::{Health, Job, Spec};
use rust_mirror_gui::ops;
use rust_mirror_gui::settings::NetworkSettings;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set"))
}

fn worker() -> String {
    env!("CARGO_BIN_EXE_rust-mirror-gui").to_string()
}

/// Upstream-facing commands get the window's network flags (direct here).
fn net(spec: Spec) -> Spec {
    ops::with_network(spec, &NetworkSettings::default(), "")
}

struct Evidence {
    dir: PathBuf,
    steps: Vec<Value>,
}

impl Evidence {
    fn save(&self) {
        let text = serde_json::to_string_pretty(&json!({ "steps": self.steps })).unwrap();
        std::fs::write(self.dir.join("evidence.json"), text).unwrap();
    }

    /// Run one step to completion (or until `stop_when` asks for a hard kill)
    /// and record it. Returns the finished job.
    fn run(
        &mut self,
        step: &str,
        spec: Spec,
        timeout: Duration,
        mut kill_when: impl FnMut(&Job) -> bool,
    ) -> Job {
        let args: Vec<String> = spec
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let started = Instant::now();
        let mut job = Job::start(spec, || {}).expect("start worker");
        let deadline = started + timeout;
        let mut killed_at = None;
        while !job.finished() {
            job.poll();
            if killed_at.is_none() && kill_when(&job) {
                killed_at = Some(job.progress.crate_summary.clone());
                job.kill();
            }
            assert!(Instant::now() < deadline, "{step}: timed out");
            std::thread::sleep(Duration::from_millis(20));
        }
        let outcome = job.progress.result.clone();
        let entry = json!({
            "step": step,
            "args": args,
            "health": format!("{:?}", job.health()),
            "seconds": started.elapsed().as_secs_f64(),
            "events": job.progress.events,
            "killed_at": killed_at.map(|c| format!("{c:?}")),
            "ok": outcome.as_ref().map(|o| o.ok),
            "error_kind": outcome.as_ref().and_then(|o| o.error.as_ref()).map(|e| e.kind.clone()),
        });
        println!("{}", serde_json::to_string(&entry).unwrap());
        self.steps.push(entry);
        self.save();
        job
    }

    fn note(&mut self, step: &str, details: Value) {
        let entry = json!({ "step": step, "details": details });
        println!("{}", serde_json::to_string(&entry).unwrap());
        self.steps.push(entry);
        self.save();
    }
}

fn report(job: &Job) -> Value {
    let outcome = job.progress.result.clone().expect("a result event");
    assert!(
        outcome.ok,
        "step failed: {:?}; last log: {:?}",
        outcome.error,
        job.log
            .iter()
            .rev()
            .take(5)
            .map(|l| l.text.clone())
            .collect::<Vec<_>>()
    );
    outcome.report.unwrap_or(Value::Null)
}

fn write_json(path: &Path, v: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(v).unwrap()).unwrap();
}

#[test]
#[ignore = "real crates.io and publish target; run by the Windows qualification workflow"]
fn crate_workflow_end_to_end() {
    let dir = PathBuf::from(env("QUAL_DIR"));
    std::fs::create_dir_all(&dir).unwrap();
    let lock = env("QUAL_LOCK");
    let url = env("QUAL_PUBLISH_URL");
    assert!(url.ends_with('/'), "QUAL_PUBLISH_URL must end with /");
    let ca = std::env::var("QUAL_CA_FILE").unwrap_or_default();
    let user = env("QUAL_PUBLISH_USER");
    let password = env("QUAL_PUBLISH_PASSWORD");
    let exe = worker();
    let mut ev = Evidence {
        dir: dir.clone(),
        steps: Vec::new(),
    };
    let short = Duration::from_secs(120);
    let long = Duration::from_secs(1800);
    let never = |_: &Job| false;
    let s = |p: &Path| p.to_string_lossy().into_owned();

    // 1. Catalog: paste a [dependencies] block, keep the usable entries.
    let paste = dir.join("paste.toml");
    std::fs::write(
        &paste,
        "[dependencies]\nanyhow = \"1\"\nitoa = \"1\"\nlocal = { path = \"../local\" }\n",
    )
    .unwrap();
    let job = ev.run("paste", ops::crates_paste(&exe, &paste), short, never);
    let r = report(&job);
    assert_eq!(r["accepted"], json!(["anyhow", "itoa"]));
    assert_eq!(r["rejected"][0]["kind"], "path");
    assert_eq!(r["rejected"][0]["line"], 4);
    // The window's "New catalog" shape plus the pasted crates.
    let mut catalog = json!({
        "crates": r["catalog"]["crates"].clone(),
        "crate-rules": {},
        "crate-policy": {
            "features": "declared",
            "resolver": "2",
            "hosts": ["x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu"],
            "targets": ["x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu"],
            "max-versions": 3000,
            "max-files": 10000,
            "max-metadata-bytes": 268435456u64,
            "max-file-bytes": 536870912u64,
            "max-final-bytes": 10737418240u64,
            "max-transfer-bytes": 10737418240u64
        },
        "crate-output": { "base-url": url },
        // One download at a time, so the hard kill lands mid-download.
        "crate-network": {
            "contact": "https://github.com/rustcommons/rust-mirror-gui (CI qualification)",
            "concurrency": 1, "attempts": 4, "max-wait-seconds": 60
        }
    });
    let cat_json = dir.join("catalog.json");
    write_json(&cat_json, &catalog);
    let cat = s(&dir.join("crates.toml"));
    let job = ev.run(
        "save catalog",
        ops::crates_save(&exe, &cat_json, &cat, false),
        short,
        never,
    );
    report(&job);

    // 2. One real Cargo.lock: preview, then write.
    let inputs = vec![("demo".to_string(), lock.clone())];
    let job = ev.run(
        "import lock (preview)",
        ops::crates_import_lock(&exe, &cat, &inputs, false),
        short,
        never,
    );
    let r = report(&job);
    assert_eq!(r["accepted"], true);
    assert_eq!(r["written"], false);
    let added = r["sources"][0]["added"].as_array().map_or(0, Vec::len);
    assert!(added > 0);
    let job = ev.run(
        "import lock (write)",
        ops::crates_import_lock(&exe, &cat, &inputs, true),
        short,
        never,
    );
    let r = report(&job);
    assert_eq!(r["written"], true);
    ev.note(
        "import summary",
        json!({ "pins_added": added, "skipped": r["sources"][0]["skipped"], "lock_sha256": r["sources"][0]["sha256"] }),
    );

    // 3. Preview with sizes into run-001.
    let ws = dir.join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let run1 = s(&ws.join("run-001"));
    let job = ev.run(
        "preview with sizes",
        net(ops::crates_sync(&exe, &cat, &run1, "", true, None, false)),
        long,
        never,
    );
    let r = report(&job);
    assert_eq!(r["status"], "needs_approval");
    let plan_id = r["plan_id"].as_str().unwrap().to_string();
    let counts = r["preview"]["counts"].clone();
    let total_files = counts["files"].as_u64().unwrap();
    let locked: Vec<String> = r["preview"]["versions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| {
            v["sources"]
                .as_array()
                .is_some_and(|s| s.iter().any(|x| x["kind"] == "lock" && x["name"] == "demo"))
        })
        .map(|v| {
            format!(
                "{} {}",
                v["name"].as_str().unwrap(),
                v["version"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(locked.len(), added, "every lock pin is in the preview");
    ev.note(
        "preview",
        json!({ "plan_id": plan_id, "counts": counts, "budgets": r["preview"]["budgets"], "versions_from_lock": locked.len() }),
    );

    // 4. Approve the saved preview; hard-kill the worker mid-download.
    let job = ev.run(
        "approve and download (hard kill)",
        net(ops::crates_sync(
            &exe,
            &cat,
            &run1,
            "",
            false,
            Some(&plan_id),
            false,
        )),
        long,
        |j: &Job| {
            j.progress
                .crate_summary
                .as_ref()
                .is_some_and(|c| c.files_completed >= 3)
        },
    );
    assert_eq!(job.health(), Health::Killed);
    let killed_after = job
        .progress
        .crate_summary
        .as_ref()
        .map_or(0, |c| c.files_completed);
    assert!(killed_after < total_files, "the kill landed before the end");

    // 5. Resume the same approved preview.
    let job = ev.run(
        "resume download",
        net(ops::crates_sync(
            &exe,
            &cat,
            &run1,
            "",
            false,
            Some(&plan_id),
            true,
        )),
        long,
        never,
    );
    let r = report(&job);
    assert_eq!(r["status"], "completed");
    let mirror1 = r["mirror"].as_str().unwrap().to_string();
    ev.note(
        "resumed",
        json!({ "files_completed": r["report"]["files_completed"], "transferred_bytes": r["report"]["transferred_bytes"], "reused_bytes": r["report"]["reused_bytes"], "mirror": mirror1 }),
    );
    assert!(
        r["report"]["reused_bytes"].as_u64().unwrap_or(0) > 0,
        "resume reused verified files"
    );

    // 6. A second mirror with the first as previous (everything reused).
    let run2 = s(&ws.join("run-002"));
    let job = ev.run(
        "preview run-002",
        net(ops::crates_sync(
            &exe, &cat, &run2, &mirror1, false, None, false,
        )),
        long,
        never,
    );
    let r = report(&job);
    let plan2 = r["plan_id"].as_str().unwrap().to_string();
    let job = ev.run(
        "download run-002",
        net(ops::crates_sync(
            &exe,
            &cat,
            &run2,
            &mirror1,
            false,
            Some(&plan2),
            false,
        )),
        long,
        never,
    );
    let r = report(&job);
    assert_eq!(r["status"], "completed");
    let mirror2 = r["mirror"].as_str().unwrap().to_string();

    // 7. Crate retention preview (read-only), as the window builds it.
    let job = ev.run(
        "crate generations",
        ops::generations(&exe, &s(&ws)),
        short,
        never,
    );
    report(&job);
    let loaded = ev.run("load catalog", ops::crates_load(&exe, &cat), short, never);
    catalog = report(&loaded);
    catalog["crate-retention"] =
        json!({ "keep-newest": 1, "previous-generations": 0, "pinned-generations": [] });
    let ret_json = dir.join("retention-catalog.json");
    write_json(&ret_json, &catalog);
    let job = ev.run(
        "crate retention preview",
        ops::crates_retention_preview(&exe, &ret_json, &s(&ws), &mirror2, "2026-11-14"),
        short,
        never,
    );
    let plan = report(&job)["plan"].clone();
    let kept_for_lock = plan["keep_sources"].as_array().is_some_and(|k| {
        k.iter().any(|g| {
            g["sources"]
                .as_array()
                .is_some_and(|s| s.iter().any(|x| x["kind"] == "lock" && x["name"] == "demo"))
        })
    });
    assert!(kept_for_lock, "a mirror is kept for the demo lock source");
    ev.note(
        "retention preview",
        json!({ "deletion": plan["deletion"], "keep_sources": plan["keep_sources"] }),
    );
    assert!(
        Path::new(&mirror1).exists() && Path::new(&mirror2).exists(),
        "preview deletes nothing"
    );

    // 8. Client setup snippet for the published mirror.
    let job = ev.run(
        "client setup",
        ops::crates_client_config(&exe, &mirror2),
        short,
        never,
    );
    let config = report(&job)["config"].as_str().unwrap().to_string();
    assert!(config.contains(&format!("sparse+{url}")), "{config}");
    std::fs::write(dir.join("cargo-config.toml"), &config).unwrap();

    // 9. Publish: check destination (no writes), then publish.
    let job = ev.run(
        "publish preflight",
        ops::publish_preflight(&exe, &mirror2, &url, &ca, &user, &password),
        long,
        never,
    );
    let r = report(&job);
    ev.note(
        "preflight",
        json!({ "files": r["files"], "local_bytes": r["local_bytes"] }),
    );
    let job = ev.run(
        "publish",
        ops::publish(&exe, &mirror2, &url, &ca, &user, &password),
        long,
        never,
    );
    let r = report(&job);
    ev.note("published", r.clone());
    // Publishing again into the same place must refuse before writing.
    let job = ev.run(
        "publish preflight again (must refuse)",
        ops::publish_preflight(&exe, &mirror2, &url, &ca, &user, &password),
        long,
        never,
    );
    assert!(
        matches!(job.health(), Health::Failed { ref kind, .. } if kind == "remote_exists"),
        "{:?}",
        job.health()
    );
    let text = std::fs::read_to_string(dir.join("evidence.json")).unwrap();
    assert!(
        !text.contains(&password),
        "the publish password is not in the evidence"
    );
}
