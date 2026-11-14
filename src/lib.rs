//! rust-mirror-gui: a desktop front end that drives the rust-mirror CLI as a
//! child process through its JSON-lines event protocol.
pub mod app;
pub mod config;
pub mod job;
pub mod ops;
pub mod protocol;
pub mod settings;
pub mod worker;

/// The window size the app asks for at start (points); the app shrinks it on
/// first frame if the monitor is smaller.
pub const DEFAULT_INNER_SIZE: [f32; 2] = [1280.0, 860.0];
/// Every page stays fully usable down to this window size (points).
pub const MIN_INNER_SIZE: [f32; 2] = [900.0, 600.0];
