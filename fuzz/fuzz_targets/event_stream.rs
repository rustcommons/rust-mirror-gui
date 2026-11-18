//! A worker's whole stdout: the line reader with its size limit
//! (`job::read_events`, here with a small limit so the overlong-line path is
//! reached), every event folded into the job's state (`Progress::apply`), and
//! the state's derived values. Must never panic or loop.
#![no_main]
use libfuzzer_sys::fuzz_target;
use rust_mirror_gui::job::{read_events, Progress};

fuzz_target!(|data: &[u8]| {
    let mut progress = Progress::default();
    let _ = read_events(data, 4096, |event| {
        let _ = progress.apply(&event);
        true
    });
    let _ = progress.stall_after();
    let _ = progress.heartbeat_interval();
});
