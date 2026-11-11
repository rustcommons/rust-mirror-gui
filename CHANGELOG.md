# Changelog

## [Unreleased]

No changes yet.

## [0.1.0] - 2026-11-11

First release: a desktop app for building offline Rust toolchain mirrors with
the rust-mirror library compiled in. See the
[versioned notes](https://github.com/rustcommons/rust-mirror-gui/blob/main/docs/releases/v0.1.0.md)
for validation and known limits.

- One executable. Every operation runs in a hidden `--worker` child process of
  the same program, which calls the rust-mirror library and reports over a
  JSON-lines event protocol. The window never blocks. It shows the live stage,
  files, bytes and time since the last event, and warns when the worker stops
  sending heartbeats.
- Stop asks the worker to stop safely; "End process now" is offered when it is
  stopping or stalled. Sync explains what is kept for Resume. Publish explains
  that partial remote files must not be activated.
- Pages:
  - Selections: writes a new configuration file.
  - Plan preview: metadata only.
  - Sync: new run, resume or start over.
  - Rustup finish.
  - Publish to Nexus Raw or a static HTTPS host, with an optional CA bundle and
    Basic credentials that are passed only to the worker.
- Each page shows the equivalent `rust-mirror` command.
