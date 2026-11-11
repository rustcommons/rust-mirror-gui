# Changelog

## Unreleased

- Repository created. No released versions.
- Added a design proposal (`docs/DESIGN.md`) with screen mockups under
  `docs/design/`.
- Added an egui prototype: one executable with the rust-mirror library compiled
  in. Every operation runs in a `--worker` child process of the same executable
  that reports over the JSON-lines event protocol. The protocol is provisional
  until rust-mirror 0.2.0, and rust-mirror 0.1.1 is bridged through its `Source`
  trait. The prototype includes a selections editor, sync/Rustup/publish forms,
  live progress, stall detection, and stop/end-process. Tests use a scripted
  fake backend and the real worker.
- Added CI (fmt, clippy, tests on Linux and Windows) and a Windows x86-64 trial
  build artifact.
