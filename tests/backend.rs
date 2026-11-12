//! Runs the real process client against the fake backend binary.
#[cfg(feature = "fake-backend")]
use rust_mirror_gui::job::Spec;
use rust_mirror_gui::job::{discover, Health, Job};
use rust_mirror_gui::ops::{self, SyncMode};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[cfg(feature = "fake-backend")]
fn fake() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_fake-rust-mirror"))
}

#[cfg(feature = "fake-backend")]
fn spec(scenario: &str, step_ms: u32) -> Spec {
    let mut s = ops::sync(
        &fake().to_string_lossy(),
        "mirror.toml",
        "run-1",
        "",
        SyncMode::New,
    );
    s.env = vec![
        ("FAKE_SCENARIO".into(), scenario.into()),
        ("FAKE_STEP_MS".into(), step_ms.to_string()),
    ];
    s
}

fn wait_for(job: &mut Job, timeout: Duration, mut done: impl FnMut(&Job) -> bool) {
    let deadline = Instant::now() + timeout;
    loop {
        job.poll();
        if done(job) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out; health {:?}",
            job.health()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(feature = "fake-backend")]
fn discover_with(scenario: &str) -> Result<rust_mirror_gui::protocol::BackendVersion, String> {
    // discover() passes no extra environment, so set it process-wide. Other
    // tests set FAKE_SCENARIO on their own child, which overrides this.
    std::env::set_var("FAKE_SCENARIO", scenario);
    let r = discover(&fake(), Duration::from_secs(10));
    std::env::remove_var("FAKE_SCENARIO");
    r
}

#[cfg(feature = "fake-backend")]
#[test]
fn discovery_accepts_and_refuses() {
    // One test so the process-wide FAKE_SCENARIO variable is not raced.
    assert_eq!(discover_with("ok").unwrap().protocol, 1);
    assert!(discover_with("legacy").unwrap_err().contains("unreadable"));
    assert!(discover_with("protocol2")
        .unwrap_err()
        .contains("protocol 2"));
    assert!(discover(
        &PathBuf::from("definitely-not-a-real-program-xyz"),
        Duration::from_secs(5)
    )
    .unwrap_err()
    .contains("Could not start"));
}

#[cfg(feature = "fake-backend")]
#[test]
fn successful_run_reports_progress() {
    let mut job = Job::start(spec("ok", 5), || {}).unwrap();
    wait_for(&mut job, Duration::from_secs(30), Job::finished);
    assert_eq!(job.health(), Health::Succeeded);
    assert!(job.progress.bytes_total > 100_000_000);
    assert_eq!(job.progress.files.len(), 4);
    assert!(job.progress.files.values().all(|f| f.done));
    assert_eq!(job.progress.hello.as_ref().unwrap().command, "sync");
}

#[cfg(feature = "fake-backend")]
#[test]
fn typed_failure_is_reported() {
    let mut job = Job::start(spec("fail", 5), || {}).unwrap();
    wait_for(&mut job, Duration::from_secs(30), Job::finished);
    match job.health() {
        Health::Failed { kind, .. } => assert_eq!(kind, "network"),
        other => panic!("{other:?}"),
    }
}

#[cfg(feature = "fake-backend")]
#[test]
fn crash_without_result_is_detected() {
    let mut job = Job::start(spec("crash", 5), || {}).unwrap();
    wait_for(&mut job, Duration::from_secs(30), Job::finished);
    assert!(
        matches!(job.health(), Health::ExitedWithoutResult(_)),
        "{:?}",
        job.health()
    );
}

#[cfg(feature = "fake-backend")]
#[test]
fn stop_is_graceful() {
    let mut job = Job::start(spec("ok", 200), || {}).unwrap();
    wait_for(&mut job, Duration::from_secs(30), |j| {
        j.progress.bytes_total > 0
    });
    job.request_stop();
    assert!(matches!(job.health(), Health::Stopping(_)));
    wait_for(&mut job, Duration::from_secs(30), Job::finished);
    match job.health() {
        Health::Failed { kind, .. } => assert_eq!(kind, "cancelled"),
        other => panic!("{other:?}"),
    }
}

#[cfg(feature = "fake-backend")]
#[test]
fn stalled_backend_is_flagged_and_can_be_killed() {
    let mut job = Job::start(spec("stall", 5), || {}).unwrap();
    wait_for(&mut job, Duration::from_secs(30), |j| {
        matches!(j.health(), Health::Stalled(_))
    });
    job.request_stop(); // the stalled fake ignores this
    std::thread::sleep(Duration::from_millis(300));
    job.poll();
    assert!(matches!(job.health(), Health::Stopping(_)));
    job.kill();
    wait_for(&mut job, Duration::from_secs(30), Job::finished);
    assert_eq!(job.health(), Health::Killed);
}

#[cfg(feature = "fake-backend")]
#[test]
fn heartbeats_keep_a_slow_stage_alive() {
    let mut job = Job::start(spec("quiet-stage", 5), || {}).unwrap();
    wait_for(&mut job, Duration::from_secs(30), |j| {
        j.progress
            .stage
            .as_ref()
            .is_some_and(|s| s.index == Some(4))
    });
    std::thread::sleep(Duration::from_secs(6));
    job.poll();
    assert!(
        matches!(job.health(), Health::Running(_)),
        "{:?}",
        job.health()
    );
    job.request_stop();
    wait_for(&mut job, Duration::from_secs(30), Job::finished);
}

#[cfg(feature = "fake-backend")]
#[test]
fn garbage_lines_are_counted_not_fatal() {
    let mut job = Job::start(spec("garbage", 5), || {}).unwrap();
    wait_for(&mut job, Duration::from_secs(30), Job::finished);
    assert_eq!(job.health(), Health::Succeeded);
    assert_eq!(job.progress.protocol_warnings, 2);
}

fn gui_exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rust-mirror-gui"))
}

#[test]
fn built_in_worker_reports_versions() {
    let v = discover(&gui_exe(), Duration::from_secs(20)).unwrap();
    assert_eq!(v.protocol, 1);
    assert_eq!(
        v.library.as_deref(),
        Some(rust_mirror_gui::worker::LIBRARY_VERSION)
    );
}

#[test]
fn built_in_worker_reports_library_errors_as_results() {
    let dir = std::env::temp_dir().join(format!("rmg-test-{}", std::process::id()));
    let missing = dir.join("missing.toml");
    let mut spec = ops::sync(
        &gui_exe().to_string_lossy(),
        &missing.to_string_lossy(),
        &dir.join("out").to_string_lossy(),
        "",
        SyncMode::New,
    );
    spec.env.clear();
    let mut job = Job::start(spec, || {}).unwrap();
    wait_for(&mut job, Duration::from_secs(30), Job::finished);
    assert_eq!(job.progress.hello.as_ref().unwrap().command, "sync");
    match job.health() {
        Health::Failed { kind, message } => {
            assert_eq!(kind, "invalid");
            assert!(message.contains("regular file"), "{message}");
        }
        other => panic!("{other:?}"),
    }
    assert!(!dir.join("out").exists());
}

#[cfg(feature = "fake-backend")]
fn net_spec(
    net: &str,
    settings: &rust_mirror_gui::settings::NetworkSettings,
    password: &str,
) -> Spec {
    let mut s = ops::check_network(&fake().to_string_lossy(), settings, password);
    s.env.push(("FAKE_NET".into(), net.into()));
    s.env.push(("FAKE_STEP_MS".into(), "5".into()));
    s
}

#[cfg(feature = "fake-backend")]
#[test]
fn connection_test_failure_carries_steps_and_presented_certificate() {
    let settings = rust_mirror_gui::settings::NetworkSettings::default();
    let mut job = Job::start(net_spec("tls", &settings, ""), || {}).unwrap();
    wait_for(&mut job, Duration::from_secs(30), Job::finished);
    let outcome = job.progress.result.clone().unwrap();
    let err = outcome.error.unwrap();
    assert_eq!(err.kind, "tls");
    let steps = err.report.unwrap()["steps"].as_array().unwrap().clone();
    let tls = steps.iter().find(|s| s["name"] == "tls").unwrap();
    assert_eq!(tls["status"], "failed");
    assert!(tls["certificates"][0]["issuer"]
        .as_str()
        .unwrap()
        .contains("Example Corp"));
    assert_eq!(
        steps.iter().find(|s| s["name"] == "download").unwrap()["status"],
        "skipped"
    );
}

#[cfg(feature = "fake-backend")]
#[test]
fn proxy_password_reaches_worker_over_stdin_and_cancel_still_works() {
    use rust_mirror_gui::settings::{NetworkSettings, ProxyMode};
    let settings = NetworkSettings {
        mode: ProxyMode::Manual,
        proxy_url: "http://proxy:3128".into(),
        ..Default::default()
    };
    let spec = net_spec("proxy_auth", &settings, "pw-123");
    assert!(!spec
        .args
        .iter()
        .any(|a| a.to_string_lossy().contains("pw-123")));
    let mut job = Job::start(spec, || {}).unwrap();
    wait_for(&mut job, Duration::from_secs(30), Job::finished);
    let report = job
        .progress
        .result
        .clone()
        .unwrap()
        .error
        .unwrap()
        .report
        .unwrap();
    let auth = report["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "proxy_auth")
        .unwrap()
        .clone();
    assert!(auth["message"]
        .as_str()
        .unwrap()
        .contains("password supplied: true"));
    assert!(!job.log.iter().any(|l| l.text.contains("pw-123")));
}
