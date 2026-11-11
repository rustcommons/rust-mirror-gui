# rust-mirror-gui

## Project scope

The owner chose `rust-mirror-gui` on 2026-11-09 as the graphical companion to the
`rust-mirror` CLI/library, and started the project on 2026-11-11. It gives users a
friendly interface for the existing rust-mirror workflow: selections, plan preview,
fetch/sync with progress and resume, multi-version assembly, retention preview
(once rust-mirror ships it) and Nexus/static-host publication.

- One executable (owner decisions 2026-11-11). The rust-mirror library crate is
  compiled in, and no separate rust-mirror install is used. Every operation runs in
  a child process of the same executable (`rust-mirror-gui --worker COMMAND …`).
  The worker only calls rust-mirror library APIs and reports to the window over
  the JSON-lines event protocol (rust-mirror `docs/EVENTS.md`, 0.2.0+). Never
  reimplement mirroring logic: planning, hashing, assembly, publication, retention
  and trust checks stay in rust-mirror. If the worker needs more, propose a
  library API upstream in rustcommons/rust-mirror.
- The window never runs operations in-process. Keep the UI thread free of blocking
  I/O. Always show the live stage and time since the last event. Flag a stalled
  worker when heartbeats stop. Stop asks the worker to stop safely, and only then
  offers ending the process. Tell the user what is kept for Resume.
- Do not invent protocol fields that conflict with rust-mirror's spec. Coordinate
  protocol needs with the rust-mirror maintainers.
- Until rust-mirror 0.2.0 is on crates.io, the worker bridges 0.1.1 through its
  public `Source` trait (`src/worker.rs`). Switch to the crates.io 0.2.0 event API
  before merging the prototype PR. Git or path dependencies are for development only.
- `src/bin/fake-rust-mirror.rs` is a test stand-in that only emits scripted
  events (selected with `RUST_MIRROR_GUI_WORKER`). Never ship it or describe it
  as a mirror tool.
- Describe only implemented, demonstrated functionality. Record everything else as
  planned.
- Framework: eframe/egui, approved by the owner on 2026-11-11. Changing it needs
  owner approval.
- Never touch the user's real Rustup/Cargo homes or a real Nexus server without
  owner authorization. Never log credentials or disable TLS verification.
- Dependencies: eframe and rust-mirror (approved 2026-11-11), plus serde and
  serde_json for the event protocol (flagged for owner confirmation with the
  prototype). Add nothing else without owner approval.
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
