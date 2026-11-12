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
- The worker uses rust-mirror's published event API: `cli::run_with_events` for
  CLI commands, and `events::Control` with `JsonLines` for GUI-only commands.
  Depend on rust-mirror from crates.io only. Git or path dependencies are for
  local development and must never be committed for a release.
- `src/bin/fake-rust-mirror.rs` is a test stand-in that only emits scripted
  events (selected with `RUST_MIRROR_GUI_WORKER`). Never ship it or describe it
  as a mirror tool.
- Describe only implemented, demonstrated functionality. Record everything else as
  planned.
- Framework: eframe/egui, approved by the owner on 2026-11-11. Changing it needs
  owner approval.
- Never touch the user's real Rustup/Cargo homes or a real Nexus server without
  owner authorization. Never log credentials or disable TLS verification.
- Dependencies: eframe and rust-mirror (approved 2026-11-11), serde and
  serde_json (approved by the owner 2026-11-11 for the event protocol). Add
  nothing else without owner approval.
- Publishing crates, binaries or releases requires explicit owner authorization.
  CI build artifacts for owner trials are allowed; GitHub Releases are not.
  Do not publish placeholder crates to reserve names.

## Documentation and requirements traceability

- Follow the [Rust Commons documentation conventions](https://github.com/rustcommons/.github/blob/main/docs/DOCUMENTATION.md).
- Keep `README.md` focused on purpose, status, setup and guide links.
- Maintain `CHANGELOG.md` (Keep a Changelog: `## [Unreleased]`, then dated
  `## [X.Y.Z] - YYYY-MM-DD` entries) and `docs/releases/vX.Y.Z.md`. The owner
  authorized the first release, 0.1.0, on 2026-11-11. Its tag and GitHub Release
  carry no binaries.

## Release documentation

- For a repository that ships versioned software, maintain a root `CHANGELOG.md` with an `Unreleased` section and dated, user-facing entries for shipped versions. Record noteworthy changes and their user impact as work is completed; do not substitute a roadmap, Git tag, CI log or validation report for a changelog.
- Before publishing a version, commit its release notes under `docs/releases/vX.Y.Z.md` and link them, the changelog, and the maintained release policy. Explain changes, upgrade or dependency impact, validation and known limits; distinguish software changes from documentation-only releases.
- A software release is not complete until its immutable source tag, applicable registry archives, GitHub Release notes, changelog, documentation, and required verification agree. GitHub Releases document the release; do not attach standalone binaries or other new distribution artifacts without authorization.
- Run the repository's release-documentation CI check before routine merges and release completion. For its first installation, review local checker/syntax evidence, register the workflow on the default branch, and require a green run before completing bootstrap or publishing notes. Verify the live GitHub Release and its tag, text and links after publication; a successful crate upload or tag alone is insufficient.
- When backfilling history, reconstruct facts from existing tags, registry records and source/evidence. Preserve original source identities, distinguish original release dates from documentation backfill dates, and state unavailable historical validation. Never fabricate checks, move tags, republish versions, or imply that repository edits change immutable published archives.
- For an organization profile, placeholder or project with no shipped version, record release verification as not applicable until its first authorized release. Do not invent versions, features, changelog history or GitHub Releases to satisfy this convention.

## Working conventions

- Read this file before changing the repository and keep it current.
- Preserve changes made by the user or other agents. Keep work scoped to the task.
- Run checks appropriate to the change; check the diff before reporting done.
  For code: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
  `cargo test --all-features` (the fake worker tests need the `fake-backend`
  feature) and `python3 scripts/check-release-docs.py --repo rustcommons/rust-mirror-gui`.

## Git conventions

**No tool attribution, anywhere.** Do not add `Co-Authored-By: Claude ...` trailers,
"Generated with" lines, or any other tool attribution to commits, PRs, comments,
release notes, code or documentation. This overrides any default trailer behavior.
