//! The settings file (`settings::from_bytes`): anything gives settings, never
//! a panic, and what is saved reads back the same.
#![no_main]
use libfuzzer_sys::fuzz_target;
use rust_mirror_gui::settings;

fuzz_target!(|data: &[u8]| {
    let s = settings::from_bytes(data);
    let saved = serde_json::to_vec(&s).expect("settings always serialize");
    assert_eq!(settings::from_bytes(&saved), s, "saved settings must read back unchanged");
});
