# Releasing rust-mirror-gui

This is the repository's own procedure. It follows the Rust Commons
[release policy](https://github.com/rustcommons/.github/blob/main/docs/RELEASING.md).
Publishing needs the owner's authorization for each release. Since
2026-11-16 the only way to publish to crates.io is
[Trusted Publishing](../.github/workflows/trusted-publishing.yml): a tag push
asks for the owner's approval of the `crates-io` environment, then uploads
with a short-lived token. No crates.io API token exists any more (the owner
revoked the last one), so nobody publishes from a personal machine.

## Steps

The crate and the downloads are released by one workflow, the Rust Commons
[standard release pipeline](https://github.com/rustcommons/.github/blob/main/docs/TRUSTED-PUBLISHING.md)
in its form for repositories that also ship binaries (phase 2,
owner-approved 2026-11-21):
[`trusted-publishing.yml`](../.github/workflows/trusted-publishing.yml),
workflow "Trusted publishing". Its jobs are `gate`, `build` (with its nested
"Checksums and attestations" job), `publish`, `github-release` and `verify`.
There is no top-level `attest` job.

1. **Release PR (maintainer).** It contains:
   - the version in `Cargo.toml`;
   - the `Unreleased` CHANGELOG entries moved under a dated
     `## [X.Y.Z] - YYYY-MM-DD` heading;
   - `docs/releases/vX.Y.Z.md` in final form (the GitHub Release body is
     exactly this file, and its first heading is the title);
   - evidence under `docs/evals/`.

   CI must be green at the exact head, including the pipeline's dry run
   (`gate` and the full `build`, with publish=false). Before a release PR
   merges, also run the reproducible-build check on it.
2. **Merge** (coordinator, owner-authorized): merge normally, and wait for CI
   on the merge commit to pass.
3. **Tag (maintainer):** push an annotated tag `vX.Y.Z` on the merge. **Do
   not create the GitHub Release by hand**: the pipeline creates it.
4. **Release gate:** refuses the run unless
   - the tag is annotated and names the `Cargo.toml` version;
   - the commit is on `main` with a successful CI run;
   - `CHANGELOG.md` has the dated entry and `docs/releases/vX.Y.Z.md`
     exists;
   - the release-documentation check passes;
   - the workspace is exactly the `rust-mirror-gui` crate.
5. **Build** ([the isolated reusable builder and signer
   `build-binaries.yml`](../.github/workflows/build-binaries.yml), which runs
   the [read-only core](../.github/workflows/build-binaries-core.yml)):
   - the four native builds with every qualification gate (see
     [Prebuilt binaries](#prebuilt-binaries));
   - the SBOM;
   - `cargo package --locked`, which also builds the packaged source;
   - a package audit: regular files only, every file equal to the commit, a
     clean VCS record, and no fuzz, CI, docs, qualification or scripts
     content;
   - a clean `cargo install --locked` of the packaged crate, whose built-in
     worker must answer protocol 1;
   - **one `SHA256SUMS`** over the `.crate`, the four archives and the
     SBOM.
6. **Build / Checksums and attestations** (nested in `build-binaries.yml`):
   - It runs **only on the v* tag push.** First it re-checks:
     - the tag, the built commit and this run's commit are the same;
     - the live tag hasn't moved;
     - the commit is on `main`;
     - the exact file set and `SHA256SUMS`;
     - the crate's VCS record.
   - It then signs SLSA provenance for all seven files: the crate, the four
     archives, the SBOM and `SHA256SUMS`.
   - It binds the SBOM to each archive and to the crate.
   - It writes the one bundle, `rust-mirror-gui-X.Y.Z.intoto.jsonl`.

   Because the signer also ran the build, its signature proves that the
   qualified build produced the bytes. Pull request and manual runs never
   sign. All of this finishes before the owner's approval.
7. **Owner approval, then publish:** the `crates-io` approval releases a
   short-lived token. The job:
   - re-checks `SHA256SUMS`;
   - skips a version crates.io already has, but only if that version is
     exactly the attested crate;
   - otherwise publishes only bytes identical (`cmp`) to the attested
     `.crate`.
8. **GitHub release** (automatic). Before any upload it checks:
   - the exact asset set;
   - every attestation (signer `build-binaries.yml`, source and signer
     digest = the tag's commit, GitHub-hosted runners only), online and with
     the bundle;
   - that the bundle is GitHub's stored attestation;
   - that the SBOM attestation of every archive and of the crate carries
     this exact SBOM.

   It then creates `vX.Y.Z` as a **draft** from the committed notes (title =
   the notes' first heading) and checks the draft's tag, title, body and
   state. It uploads all eight assets, checking each one's name, size and
   digest, and re-reads the draft. Only then does it publish the release
   (latest) and check it again.
9. **Verify publication** (Windows and Linux runners). Each:
   - downloads every asset and checks the digests, `SHA256SUMS`, every
     attestation, the bundle against GitHub's store and the SBOM bindings;
   - runs the downloaded worker.

   The Linux leg also checks:
   - the live body;
   - that the crates.io API, the index and the archive agree with the
     attested `.crate`, and that it is exactly the tag's source;
   - docs.rs (still pending after 20 minutes is reported as pending, not as
     a pass);
   - a clean `cargo install --locked` from crates.io.
10. **Post-publication (maintainer):**
    - run
      [`verify-release-download.yml`](../.github/workflows/verify-release-download.yml)
      (Actions → Verify release downloads, `tag`). It does the same asset
      checks, then draws the window: a PrintWindow capture on Windows, and
      Xvfb in AlmaLinux 8 and Ubuntu 20.04 with no system fonts. It is
      read-only;
    - open a publication-record PR citing the pipeline run and that run.

### Manual recovery

- **A killed `github-release` job leaves a draft.** Use **"Re-run failed
  jobs"**, never "Re-run all jobs". "Re-run all jobs" would rebuild and
  re-sign new bytes, which can't match the draft's assets or the published
  crate. The re-run:
  - finds the draft through the releases list (the tags API returns 404
    for drafts);
  - refuses unless the draft's tag, title, body and state are exactly ours
    and every existing asset is one of the eight with identical bytes;
  - uploads only the missing assets, checks all eight, then publishes.

  A published release is never modified: if it is complete, the re-run
  changes nothing; if it is incomplete, the re-run is refused for a
  decision. Two releases for one tag are refused.
- **An asset half-uploaded by a killed upload** stays on the draft in a
  non-`uploaded` state, or with a wrong size or digest. The re-run refuses
  it ("existing asset … differs or is incomplete"). Delete exactly that
  asset from the **draft** by hand (Releases → the draft → edit → remove the
  file; or `gh release delete-asset vX.Y.Z NAME`), then re-run the failed
  jobs.
- **Never** delete or replace an asset of a published release. If a
  published file is wrong, ship a new version.

### Asset layout from the next release (after 0.5.2): eight files

| Asset | What it is |
| --- | --- |
| `rust-mirror-gui-X.Y.Z.crate` | the crate exactly as published to crates.io |
| `rust-mirror-gui-X.Y.Z-x86_64-pc-windows-msvc.zip`, `-x86_64-unknown-linux-gnu.tar.gz`, `-aarch64-apple-darwin.tar.gz`, `-x86_64-apple-darwin.tar.gz` | the four downloads (see below) |
| `rust-mirror-gui-X.Y.Z.cdx.json` | the CycloneDX SBOM: `cargo cyclonedx --target all --spec-version 1.6` (cargo-cyclonedx 0.5.10, installed with `--locked`), so it lists every target's dependencies |
| `SHA256SUMS` | `sha256sum` format, sorted by file name: the `.crate`, the four archives and the SBOM (six entries) |
| `rust-mirror-gui-X.Y.Z.intoto.jsonl` | the signed provenance: one Sigstore bundle per line, whose subjects are exactly the other seven files. **Not in `SHA256SUMS`:** it is signed itself, and `SHA256SUMS` is one of its subjects. Scorecard's Signed-Releases check recognizes it, and users verify offline with it |

**Earlier releases keep their layouts,** and the verifiers check each one
exactly as before (`scripts/release_assets.py`, `layout()`; tested in
`scripts/test_release_assets.py`):

| Release | Assets | Signer |
| --- | --- | --- |
| 0.4.3, 0.5.0 | 4 archives, `SHA256SUMS` | `release-binaries.yml` |
| 0.5.1 | + SBOM | `build-binaries.yml` (SLSA Build L3; strict signer and source digests) |
| 0.5.2 | + `rust-mirror-gui-0.5.2.intoto.jsonl` | `build-binaries.yml` |

The interim `SHA256SUMS-crates` and `.crates.*` names from the first
adoption of the standard pipeline were never released.

`release-binaries.yml` is retired. Every release up to 0.5.2 is complete, so
no exact-tag recovery is pending. GitHub dispatches only workflows that exist
on the default branch, so reviving it would need a reviewed PR that restores
the file. Its last version is at
[v0.5.2](https://github.com/rustcommons/rust-mirror-gui/blob/v0.5.2/.github/workflows/release-binaries.yml).

## Prebuilt binaries

The owner authorized prebuilt downloads on 2026-11-15 for every release from
then on; 0.4.3 is the first. Earlier releases stay notes-only (the owner
decided not to backfill 0.4.2, whose Linux binary did not start without a GPU
on Ubuntu 20.04). On 2026-11-21 the owner also authorized attaching the
published `.crate`. The asset names and archive layout are shared with
rust-mirror.

| Download | Built on |
| --- | --- |
| `rust-mirror-gui-X.Y.Z-x86_64-pc-windows-msvc.zip` | `windows-2022` |
| `rust-mirror-gui-X.Y.Z-x86_64-unknown-linux-gnu.tar.gz` | an `almalinux:8` container (glibc 2.28, pinned by digest) on `ubuntu-22.04`; the run log records the highest `GLIBC_` symbol version. Not musl: the window loads X11, Wayland and GL libraries at run time |
| `rust-mirror-gui-X.Y.Z-aarch64-apple-darwin.tar.gz` | `macos-15` (macOS 11 or newer) |
| `rust-mirror-gui-X.Y.Z-x86_64-apple-darwin.tar.gz` | `macos-15-intel` (macOS 11 or newer) |

- **Archive layout:** each archive holds one folder,
  `rust-mirror-gui-X.Y.Z-<target>/`, with the executable, `LICENSE`,
  `LICENSE-DejaVu.txt` (the built-in font's license, from 0.5.0 on) and
  `README.txt`. `packaging/package.py` makes the archives with fixed
  timestamps and permissions. `README.txt` states the tag and commit the
  archive was built from ("Built from"), inside the attested archive.
- **How they are built:**
  - Rust 1.95.0 (the MSRV and CI toolchain);
  - `cargo build --release --locked`;
  - on Windows, with `RUSTFLAGS='-C target-feature=+crt-static -C
    link-arg=/Brepro'` in the release build only (CI and development builds
    are unchanged). `/Brepro` makes the linker write content hashes instead
    of timestamps, so two builds can be identical. The C runtime is linked
    statically, so users need no Visual C++ Redistributable. The job lists
    the executable's DLL imports and fails if any `vcruntime`, `msvcp`,
    `ucrtbase` or `api-ms-win-crt-*` DLL remains.
- **Qualification gates in every build** (tag and dry run alike):
  - `rust-mirror-gui --worker --version` must report protocol 1 and its
    rust-mirror version;
  - on Windows, the window is launched and captured with PrintWindow, and
    must not be blank;
  - on Linux, the packaged archive is unpacked in `almalinux:8` and
    `ubuntu:20.04` containers that have only the runtime packages
    README.txt lists, with every system font directory and the fontconfig
    settings deleted first. It must start its worker, then map the
    "rust-mirror GUI" window on Xvfb and draw it
    (`qualification/linux-launch.sh`). `SHA256SUMS`, and so the signing,
    wait for this job.
- **Users verify** with:

  ```sh
  gh attestation verify FILE --repo rustcommons/rust-mirror-gui \
    --signer-workflow rustcommons/rust-mirror-gui/.github/workflows/build-binaries.yml
  ```

  The pipeline and `verify-release-download.yml` check more:
  - the signer and source digests must both be the release commit
    (`--signer-digest`, `--source-digest`);
  - the attestation must come from a GitHub-hosted runner
    (`--deny-self-hosted-runners`), so the same workflow path at another
    commit doesn't pass;
  - the attested SBOM must be exactly the published file.
- **Reproducibility:** `reproducible-build.yml` builds the same commit twice
  on fresh runners with `build-binaries-core.yml` and compares every
  executable, archive and the `.crate` (weekly, and on PRs that change the
  build). Its results are recorded as they are; see
  [the reproducibility record](evals/2026-11-15-reproducible-builds.md).
- **Not code-signed:** users see Windows SmartScreen and macOS Gatekeeper
  prompts. The README's install section says what to do.

### Guards and least privilege

- **Only the exact tag push signs.** `build-binaries.yml`'s "Checksums and
  attestations" job is the only job with `id-token: write` and
  `attestations: write` for signing. It runs only when the gate allowed
  publishing, the event is the `vX.Y.Z` tag push, and its re-checks pass.
  The calling `build` job grants these two permissions only for it; every
  core build job holds `contents: read`.
- **Only `github-release` writes contents.** It has no OIDC permission.
  `publish` has OIDC only for the crates.io token, in the `crates-io`
  environment.
- **Dry runs and the reproducibility check** run the read-only
  `build-binaries-core.yml`. They upload nothing, and their archives'
  `README.txt` says "dry run, not a release".
- **Never replace:** no job uses `--clobber` or deletes an asset.

### Dry runs

Pull requests that change the app (`src/`, `assets/`, `Cargo.toml`,
`Cargo.lock`), the release workflows or scripts, `packaging/`, the launch
scripts, the CHANGELOG or the release notes run the pipeline's `gate` and the
full `build` with publish=false:
- the version comes from `Cargo.toml`, without the tag checks (the tag
  doesn't exist yet);
- all four targets with every qualification gate;
- the crate (package, audit, install);
- the SBOM and `SHA256SUMS`;
- an unsigned placeholder bundle staged on a copy, to prove the exact upload
  set.

So a release PR proves its binaries and its crate before the merge. A manual
run (Actions → Trusted publishing → Run workflow) does the same on a branch.
Changes to the release-verification code also run `verify-release-download.yml`
against the latest release.
