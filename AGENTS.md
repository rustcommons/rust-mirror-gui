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
- Network settings (0.2.0+) are per machine, in the app's config dir, never in
  the shared mirror configuration. The proxy password is never saved to disk,
  never put in argv, env or logs; the worker receives it as the first stdin
  line (`--proxy-password-stdin`). Never auto-trust a certificate: the user
  picks the PEM and confirms after seeing its details.
- Crate pages (0.3.0+) edit only the library's catalog JSON. The worker writes
  TOML with `Catalog::write`; never hand-write Cargo dependency syntax.
  Approval is always for the whole saved preview (`--approve-plan` with that
  plan ID), never per crate, and never without showing the saved preview first.
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
  The owner authorized 0.5.1 on 2026-11-16 (the security baseline on main,
  SBOM and SLSA L3 release builds, rust-mirror 0.5.4), published through
  Trusted Publishing.
  CI build artifacts for owner trials are allowed.
- Prebuilt binaries on GitHub Releases: on 2026-11-15 the owner authorized
  attaching them from now on, replacing the earlier "notes only" rule. They
  are produced only by `.github/workflows/release-binaries.yml`, which:
  - builds from exactly the tagged commit with `--locked`;
  - produces unsigned archives (no paid code signing), with `SHA256SUMS` and
    GitHub artifact attestations;
  - uploads only to the existing release for that tag;
  - never replaces or deletes a published asset.

  See [docs/RELEASING.md](docs/RELEASING.md).
  Do not publish placeholder crates to reserve names.
- On 2026-11-15 the owner authorized publishing 0.4.3 (the wgpu limits fix)
  to crates.io as the first release with prebuilt downloads, and decided not
  to backfill downloads for 0.4.2.
- On 2026-11-15 the owner approved 0.5.0 and authorized its publication once
  green: built-in fonts (DejaVu Sans, a bundled asset, owner-approved; no new
  crate dependency), documented supported platforms, and CI-only glibc
  experiments. The Linux download stays the glibc 2.28 build.

## Security baseline (owner-approved 2026-11-15)

- **`main` changes only through pull requests.** The org ruleset
  "protect-main" requires a PR and the `lf-only` check, and forbids
  force-push and deletion. Release tags `v*` can never be moved or deleted
  ("protect-release-tags").
- **Actions are pinned to full commit SHAs**, with the version as a trailing
  comment (`uses: owner/action@<sha> # vX.Y.Z`). Dependabot
  (`.github/dependabot.yml`) proposes version updates monthly (at most 2
  open PRs per ecosystem; security updates come at once). Never add an
  unpinned `uses:`.
- **Runner budget** (the org shares 20 concurrent jobs): every PR workflow
  has a per-PR concurrency group that cancels the PR's older runs.
  Dependabot PRs run only the core checks: fmt, clippy, tests, cargo-deny,
  zizmor, `lf-only`, release docs and CodeQL. Expensive jobs (release dry
  runs, reproducibility, qualifications, glibc experiments, download
  checks) carry `if: github.actor != 'dependabot[bot]'`. Keep both when
  adding a workflow.
- **Runner images are explicit:** `ubuntu-24.04`, `ubuntu-22.04` (the
  AlmaLinux 8 container host), `windows-2025`, `windows-2022`, `macos-15`
  and `macos-15-intel`. Never use a moving label such as `ubuntu-latest`.
  Moving to Ubuntu 26.04 (or any newer image) is a deliberate change, made
  in its own PR with a green release dry run.
- **Actions run on Node 24:** checkout v7, upload-artifact v7,
  download-artifact v8 (a digest mismatch fails the download), setup-python
  v7.
- **Rust toolchain action:** `dtolnay/rust-toolchain` is pinned to the head
  commit of its `1.95.0` branch, which fixes the toolchain itself. Don't
  pass a `toolchain:` input; the action ignores it ("Unexpected input"). Each
  job prints `rustc --version`. To change Rust, re-pin to the new
  version's branch head.
- **Least privilege:** every workflow's top-level `permissions` is read-only;
  a job that needs more asks for it itself (`contents: write` only to upload
  release assets; `id-token`/`attestations: write` only where it signs or
  publishes).
- **Gates on every PR** (fix findings; any suppression is inline with a
  reason):
  - `zizmor.yml` lints the workflows;
  - the `cargo-deny` job in `ci.yml` applies `deny.toml` (RustSec
    advisories, allowed licenses, bans, crates.io-only sources).
  A new dependency's license must already be on the allowlist, or the owner
  must approve adding it.
- **Release builds** run only in the reusable `build-binaries-core.yml`
  (read-only; also used by dry runs). For a release, `build-binaries.yml`
  runs it and then signs provenance and the SBOM (SLSA Build L3). `reproducible-build.yml`
  compares two independent builds weekly.
- **Signed releases:** from 0.5.2 on, every release also carries
  `<name>-<version>.intoto.jsonl` (name shared with rust-mirror), the
  Sigstore bundles from `build-binaries.yml`'s attest job. It isn't in
  `SHA256SUMS`, and is checked against GitHub's stored attestations before
  and after upload. Never replace it or build it by hand.
- **OpenSSF Scorecard** (`scorecard.yml`) runs on `main` and weekly.
- **Trusted Publishing** is the only way to publish to crates.io (since
  2026-11-16; no API token exists any more). A `v*` tag push runs
  `trusted-publishing.yml`, which waits for the owner's approval of the
  `crates-io` environment. Never ask anyone to publish from a machine; see
  `docs/RELEASING.md`.
- **Never skip CI** (owner rule, 2026-11-17): no `[skip ci]`, `[ci skip]` or
  similar in commit or merge messages. They skip CodeQL and CI, which lowers
  Scorecard's SAST and CI-Tests.
- **Every install in a workflow or script is pinned:**
  - actions by commit SHA;
  - container and Docker images by digest;
  - `cargo install` with `--locked` and an exact `--version` (or a path
    checked out at a pinned commit);
  - pip only with `--require-hashes -r <hashed requirements file>`.

  Never pipe a download into a shell or interpreter (`curl … | sh`,
  `| python3`). Save it to a file first. Run a downloaded file only after
  its SHA-256 matched a pinned or attested value.
- **Fuzzing:** the parsers of untrusted input have cargo-fuzz targets in
  `fuzz/` (a separate, unpublished workspace; `libfuzzer-sys` and
  `arbitrary` there only). ClusterFuzzLite fuzzes PRs that change the code
  for 300 s (`cflite_pr.yml`) and runs a weekly 1800 s batch
  (`cflite_batch.yml`). A new parser of outside input gets a target and
  seeds. A crash is a real bug: fix it with a regression test, or report
  it. See `docs/FUZZING.md`.
- **Security reports:** the org-wide `SECURITY.md` (in the `.github`
  repository) applies; this repository has none of its own.

## Documentation and requirements traceability

- Follow the [Rust Commons documentation conventions](https://github.com/rustcommons/.github/blob/main/docs/DOCUMENTATION.md).
- Keep `README.md` focused on purpose, status, setup and guide links.
- Maintain `CHANGELOG.md` (Keep a Changelog: `## [Unreleased]`, then dated
  `## [X.Y.Z] - YYYY-MM-DD` entries) and `docs/releases/vX.Y.Z.md`. The owner
  authorized the first release, 0.1.0, on 2026-11-11. Its tag and GitHub Release
  carry no binaries.
- On 2026-11-13 the owner authorized publication of 0.4.0 once it is ready, that
  is, after rust-mirror 0.5.0 is on crates.io, and said to merge PRs once their
  checks pass. The flow is unchanged: the maintainer reports each green PR, the
  coordinator reviews, merges and publishes, and the maintainer does the
  post-publication steps.

## Release documentation

- For a repository that ships versioned software, maintain a root `CHANGELOG.md` with an `Unreleased` section and dated, user-facing entries for shipped versions. Record noteworthy changes and their user impact as work is completed; do not substitute a roadmap, Git tag, CI log or validation report for a changelog.
- Before publishing a version, commit its release notes under `docs/releases/vX.Y.Z.md` and link them, the changelog, and the maintained release policy. Explain changes, upgrade or dependency impact, validation and known limits; distinguish software changes from documentation-only releases.
- A software release is not complete until its immutable source tag, applicable registry archives, GitHub Release notes, changelog, documentation, and required verification agree. GitHub Releases document the release. Prebuilt binaries are authorized (2026-11-15) only through the release-binaries workflow and its reusable build-binaries.yml (see docs/RELEASING.md); do not attach other new distribution artifacts without authorization.
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
