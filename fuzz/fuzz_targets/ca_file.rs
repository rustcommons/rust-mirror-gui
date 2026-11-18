//! A company CA file the user picks (PEM certificates), read by the worker's
//! `inspect-ca` through rust-mirror's `network::inspect_ca_file`. Errors are
//! fine; a panic is a bug.
#![no_main]
use libfuzzer_sys::fuzz_target;
use std::io::Write;

fuzz_target!(|data: &[u8]| {
    let path = std::env::temp_dir().join(format!("rmg-fuzz-ca-{}.pem", std::process::id()));
    let mut f = std::fs::File::create(&path).expect("temp file");
    f.write_all(data).expect("write temp file");
    drop(f);
    let _ = rust_mirror::network::inspect_ca_file(&path);
});
