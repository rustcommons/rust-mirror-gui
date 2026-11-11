# Changelog

## Unreleased

- Repository created. No released versions.
- Added a design proposal (`docs/DESIGN.md`) with screen mockups under
  `docs/design/`.
- Added an egui prototype that drives the rust-mirror program as a child process
  through its JSON-lines event protocol. The protocol is provisional until
  rust-mirror 0.2.0. The prototype includes backend version/protocol checks,
  a selections editor, sync/Rustup/publish forms, live progress, stall
  detection, and stop/end-process. Tests use a scripted fake backend.
- Added CI (fmt, clippy, tests on Linux and Windows) and a Windows x86-64 trial
  build artifact.
