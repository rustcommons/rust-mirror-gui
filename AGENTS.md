# rust-mirror-gui

## Project scope

The owner chose `rust-mirror-gui` on 2026-11-09 as the graphical companion to the
`rust-mirror` CLI/library, and started the project on 2026-11-11. It gives users a
friendly interface for the existing rust-mirror workflow: selections, plan preview,
fetch/sync with progress and resume, multi-version assembly, retention preview
(once rust-mirror ships it) and Nexus/static-host publication.

- The GUI is a front end. All mirroring logic lives in the `rust-mirror` library
  crate. Do not reimplement planning, hashing, publication or trust checks here. If
  the GUI needs an API that rust-mirror lacks, propose it upstream in
  rustcommons/rust-mirror instead of forking the logic.
- Describe only implemented, demonstrated functionality. Record everything else as
  planned.
- The GUI framework is a major architecture decision. Get explicit owner approval,
  with rendered mockups, before committing to one.
- Never touch the user's real Rustup/Cargo homes or a real Nexus server without
  owner authorization. Never log credentials or disable TLS verification.
- Do not add dependencies beyond the approved framework without owner approval.
- Publishing crates, binaries or releases requires explicit owner authorization.
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

## Git conventions

**No tool attribution, anywhere.** Do not add `Co-Authored-By: Claude ...` trailers,
"Generated with" lines, or any other tool attribution to commits, PRs, comments,
release notes, code or documentation. This overrides any default trailer behavior.
