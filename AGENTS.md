# rust-mirror-gui

## Project scope

The owner chose `rust-mirror-gui` on 2026-11-09 as the graphical companion to the
`rust-mirror` CLI/library, and started the project on 2026-11-11. It gives users a
friendly interface for the existing rust-mirror workflow: selections, plan preview,
fetch/sync with progress and resume, multi-version assembly, retention preview
(once rust-mirror ships it) and Nexus/static-host publication.

- The GUI is a front end. It talks to the rust-mirror **process** through the
  documented JSON-lines event protocol (rust-mirror `docs/EVENTS.md`, 0.2.0+). It
  does not link the rust-mirror library for operations. Never reimplement
  mirroring logic here: planning, hashing, assembly, publication, retention and
  trust checks stay in rust-mirror. If the GUI needs more, propose it upstream in
  rustcommons/rust-mirror (owner decision 2026-11-11).
- Keep the UI thread free of blocking I/O. Always show the live stage and time
  since the last event; flag a stalled backend when heartbeats stop; Stop asks
  rust-mirror to stop safely and only then offers ending the process.
- Do not invent protocol fields that conflict with rust-mirror's spec. Coordinate
  protocol needs with the rust-mirror maintainers.
- `src/bin/fake-rust-mirror.rs` is a test stand-in that only emits scripted
  events. Never put it in the GUI package or describe it as a mirror tool. CI
  uploads it only as a separately named demo artifact for UI-only trials.
- Describe only implemented, demonstrated functionality. Record everything else as
  planned.
- Framework: eframe/egui, approved by the owner on 2026-11-11. Changing it needs
  owner approval.
- Never touch the user's real Rustup/Cargo homes or a real Nexus server without
  owner authorization. Never log credentials or disable TLS verification.
- Dependencies: eframe (approved 2026-11-11), plus serde and serde_json for the
  event protocol (flagged for owner confirmation with the prototype). Add nothing
  else without owner approval.
- Publishing crates, binaries or releases requires explicit owner authorization.
  CI build artifacts for owner trials are allowed; GitHub Releases are not.
  Do not publish placeholder crates to reserve names.

## Documentation and requirements traceability

- Follow the [Rust Commons documentation conventions](https://github.com/rustcommons/.github/blob/main/docs/DOCUMENTATION.md).
- Keep `README.md` focused on purpose, status, setup and guide links.
- Maintain `CHANGELOG.md` with an `Unreleased` section. This repository has no
  shipped version; release verification is not applicable until the first
  authorized release.

## Working conventions

- Read this file before changing the repository and keep it current.
- Preserve changes made by the user or other agents. Keep work scoped to the task.
- Run checks appropriate to the change; check the diff before reporting done.
  For code: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and
  `cargo test` (the tests drive the real process client against the fake backend).

## Git conventions

**No tool attribution, anywhere.** Do not add `Co-Authored-By: Claude ...` trailers,
"Generated with" lines, or any other tool attribution to commits, PRs, comments,
release notes, code or documentation. This overrides any default trailer behavior.
