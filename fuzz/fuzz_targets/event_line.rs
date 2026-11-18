//! One worker event line (JSON) through `Event::parse`: any bytes, any
//! nesting, any field types. Must never panic.
#![no_main]
use libfuzzer_sys::fuzz_target;
use rust_mirror_gui::protocol::Event;

fuzz_target!(|data: &[u8]| {
    let line = String::from_utf8_lossy(data);
    let event = Event::parse(&line);
    let _ = format!("{event:?}");
});
