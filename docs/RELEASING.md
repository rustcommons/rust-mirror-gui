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

The crate is released by the Rust Commons
[standard release pipeline](https://github.com/rustcommons/.github/blob/main/docs/TRUSTED-PUBLISHING.md)
(owner-approved 2026-11-21):
[`trusted-publishing.yml`](../.github/workflows/trusted-publishing.yml),
workflow "Trusted publishing", with six jobs: `gate`, `build`, `attest`,
`publish`, `github-release` and `verify`. The binaries are still built by
[`release-binaries.yml`](../.github/workflows/release-binaries.yml) on the same
tag (see [Prebuilt binaries](#prebuilt-binaries)).

1. **Release PR (maintainer).** It contains:
   - the version in `Cargo.toml`;
   - the `Unreleased` CHANGELOG entries moved under a dated
     `## [X.Y.Z] - YYYY-MM-DD` heading;
   - `docs/releases/vX.Y.Z.md` in final form (the GitHub Release body is
     exactly this file, and its first heading is the title);
   - evidence under `docs/evals/`.

   CI must be green at the exact head: the release-documentation check, and
   the release dry run (all builds, the old-Linux launches without system
   fonts, the Windows capture). Before a release PR merges, also run the
   reproducible-build check on it.
2. **Merge** (coordinator, owner-authorized): merge normally, and wait for CI
   on the merge commit to pass.
3. **Tag (maintainer):** push an annotated tag `vX.Y.Z` on the merge. **Do
   not create the GitHub Release by hand**: the pipeline creates it.
4. **The tag push runs `trusted-publishing.yml`:**
   - **Release gate:** refuses the run unless
     - the tag is annotated and names the `Cargo.toml` version;
     - the commit is on `main` with a successful CI run;
     - `CHANGELOG.md` has the dated entry and `docs/releases/vX.Y.Z.md`
       exists;
     - the release-documentation check passes;
     - the workspace is exactly the `rust-mirror-gui` crate.
   - **Build:**
     - the crate SBOM `rust-mirror-gui-X.Y.Z.crates.cdx.json`
       (cargo-cyclonedx 0.5.10, from the clean checkout);
     - `cargo package --locked`, which also builds the packaged source;
     - a package audit: regular files only, every file equal to the commit,
       a clean VCS record, and no `fuzz/`, CI, `docs/` or `qualification/`
       content;
     - a clean `cargo install --locked` of the packaged crate, whose
       built-in worker must answer protocol 1;
     - `SHA256SUMS-crates`.
   - **Checksums and attestations:** SLSA provenance for the `.crate` and
     the crate SBOM, saved as `rust-mirror-gui-X.Y.Z.crates.intoto.jsonl`.
   - **Publish to crates.io:** waits for the owner's approval of the
     `crates-io` environment. It then:
     - skips a version crates.io already has, but only if that version is
       exactly the attested crate;
     - otherwise publishes with a short-lived token, and only bytes
       identical (`cmp`) to the attested `.crate`.
   - **GitHub release:** creates `vX.Y.Z` with `--verify-tag` from the
     committed notes and attaches the `.crate`, the crate SBOM,
     `SHA256SUMS-crates` and the crate bundle. A re-run uploads only missing
     files.
   - **Verify publication:**
     - the live body equals the notes;
     - the crates.io API, the index and the archive agree with the attested
       `.crate`;
     - the archive's VCS record is the tag's commit and every file equals the
       tag;
     - `gh attestation verify --bundle` passes for the crate and its SBOM;
     - docs.rs has built it (still pending after 20 minutes is reported as
       pending, not as a pass);
     - a clean `cargo install --locked` from crates.io works.

   Pull requests that change the pipeline, the release scripts, the
   CHANGELOG or the release notes run `gate` and `build` as a dry run with
   publish=false.
5. **Binaries:** the same tag push runs `release-binaries.yml`. It waits up
   to 30 minutes for the GitHub Release, which `github-release` now creates
   after the owner approves publishing. It then attaches the archives, the
   binaries' SBOM, `SHA256SUMS` and `rust-mirror-gui-X.Y.Z.intoto.jsonl`.

   Its uploader accepts a release that already holds exactly the four crate
   files, and refuses any other existing asset.

   **If the approval comes more than 30 minutes after the tag** (as for
   0.5.2), the tag run times out in `prepare`. It uploads nothing and signs
   nothing. Then dispatch it on the tag ref:

   ```sh
   gh workflow run release-binaries.yml --ref vX.Y.Z -f tag=vX.Y.Z -f dry_run=false
   ```

   Never dispatch from another ref, because the signer and source digests
   must be the release commit. Phase 2 removes this wait (see the
   [roadmap](ROADMAP.md#standard-release-pipeline)).
6. **Post-publication (maintainer)**, once both workflows succeeded:
   - verify the binary assets, `SHA256SUMS` and every attestation (signer
     `build-binaries.yml`, source and signer digest = the release commit,
     SBOM bindings, the bundle);
   - run
     [`verify-release-download.yml`](../.github/workflows/verify-release-download.yml)
     (Actions → Verify release downloads, `tag`). It downloads the live
     binary assets, checks `SHA256SUMS` and the attestations, and runs the
     Windows and Linux archives. It is read-only;
   - open a publication-record PR citing both runs.

Releases up to 0.5.2 were published by the earlier `check`/`publish`/`verify`
workflow, and their GitHub Releases were created by hand. They keep their
existing assets; none is re-tagged or re-released.

## Prebuilt binaries

Authorized by the owner on 2026-11-15 for every release from then on; 0.4.3
is the first. Earlier releases stay notes-only (the owner decided not to
backfill 0.4.2, whose Linux binary did not start without a GPU on Ubuntu
20.04). The
asset names and archive layout are shared with rust-mirror.

| Asset | Built on |
| --- | --- |
| `rust-mirror-gui-X.Y.Z-x86_64-pc-windows-msvc.zip` | `windows-2022` |
| `rust-mirror-gui-X.Y.Z-x86_64-unknown-linux-gnu.tar.gz` | an `almalinux:8` container (glibc 2.28, pinned by digest) on `ubuntu-22.04`; the run log records the highest `GLIBC_` symbol version. Not musl: the window loads X11, Wayland and GL libraries at run time |
| `rust-mirror-gui-X.Y.Z-aarch64-apple-darwin.tar.gz` | `macos-15` (macOS 11 or newer) |
| `rust-mirror-gui-X.Y.Z-x86_64-apple-darwin.tar.gz` | `macos-15-intel` (macOS 11 or newer) |
| `rust-mirror-gui-X.Y.Z.cdx.json` | the release's CycloneDX SBOM (from 0.5.1 on; name shared with rust-mirror): `cargo cyclonedx --target all --spec-version 1.6` (cargo-cyclonedx 0.5.10, installed with `--locked`), so it lists every target's dependencies |
| `SHA256SUMS` | `sha256sum` format: lowercase hex, two spaces, sorted by file name; the archives and, from 0.5.1 on, the SBOM |
| `rust-mirror-gui-X.Y.Z.intoto.jsonl` | from 0.5.2 on (name shared with rust-mirror): the release's signed provenance, made by `build-binaries.yml`. JSON Lines, one Sigstore bundle per line: the SLSA build-provenance attestation, whose subjects are exactly the archives, the SBOM and `SHA256SUMS`. The SBOM attestation stays in GitHub's store. Scorecard's Signed-Releases check recognizes it, and users verify offline with it. **Not in `SHA256SUMS`:** it is signed itself, it is made after `SHA256SUMS` is signed, and `SHA256SUMS` is one of its subjects |
| `rust-mirror-gui-X.Y.Z.crate`, `rust-mirror-gui-X.Y.Z.crates.cdx.json`, `SHA256SUMS-crates`, `rust-mirror-gui-X.Y.Z.crates.intoto.jsonl` | after 0.5.2: the published crate, its SBOM, their checksums and their provenance bundle, attached by `trusted-publishing.yml`'s `github-release` job before the binaries (owner authorization 2026-11-21). The binary workflows leave them alone, and `verify-release-download.yml` requires exactly these four |

- **Archive layout:** each archive holds one folder,
  `rust-mirror-gui-X.Y.Z-<target>/`, with the executable, `LICENSE`,
  `LICENSE-DejaVu.txt` (the built-in font's license, from 0.5.0 on) and
  `README.txt`. `packaging/package.py` makes the archives with fixed
  timestamps and permissions.
- **How they are built:**
  - Rust 1.95.0 (the MSRV and CI toolchain);
  - `cargo build --release --locked`;
  - on Windows, with `RUSTFLAGS='-C target-feature=+crt-static -C
    link-arg=/Brepro'` in the release job only (CI and development builds
    are unchanged). `/Brepro` makes the linker write content hashes instead
    of timestamps, so two builds can be identical. The C runtime
    is linked statically, so users need no Visual C++ Redistributable. The
    job lists the executable's DLL imports and fails if any `vcruntime`,
    `msvcp`, `ucrtbase` or `api-ms-win-crt-*` DLL remains;
  - from exactly the tagged commit; the workflow refuses if the tag, the
    commit and `Cargo.toml` disagree.
- **Smoke tests before upload:**
  - `rust-mirror-gui --worker --version` must report protocol 1 and its
    rust-mirror version;
  - on Windows, the window is also launched and captured with PrintWindow,
    and must not be blank;
  - on Linux, the packaged archive is unpacked in `almalinux:8` and
    `ubuntu:20.04` containers that have only the runtime packages README.txt
    lists. From 0.5.0 on, every system font directory and the fontconfig
    settings are deleted first, to prove the built-in fonts suffice. It
    must start its worker, map the "rust-mirror GUI" window on Xvfb and draw
    it (`qualification/linux-launch.sh`). Publishing waits for this job.
- **Attestations (SLSA Build L3 from 0.5.1 on):**
  - every file (archives, SBOM, `SHA256SUMS`) gets a build-provenance
    attestation (`actions/attest-build-provenance` v4.2.2);
  - every archive gets an SBOM attestation (`actions/attest-sbom` v4.1.0);
  - both are signed inside the reusable `build-binaries.yml`, isolated from
    the calling workflow, as GitHub describes for SLSA v1 Build Level 3. It
    runs the build in the read-only `build-binaries-core.yml` first; the
    same core is what dry runs use;
  - they are verified before and after the upload.

  Users verify with:

  ```sh
  gh attestation verify FILE --repo rustcommons/rust-mirror-gui \
    --signer-workflow rustcommons/rust-mirror-gui/.github/workflows/build-binaries.yml
  ```

  The release workflow and `verify-release-download.yml` check more: the
  signer and source digests must both be the release commit
  (`--signer-digest`, `--source-digest`), and the attestation must come
  from a GitHub-hosted runner (`--deny-self-hosted-runners`). So the same
  workflow path at another commit does not pass. For the SBOM attestation
  they also check that the attested SBOM is exactly the published file.
- **Provenance on the release itself (from 0.5.2 on):** the attest job in
  `build-binaries.yml` writes the provenance attestation's Sigstore bundle
  into
  `rust-mirror-gui-X.Y.Z.intoto.jsonl` (`scripts/provenance_bundle.py
  assemble`). The publish job, the only one that can write contents,
  checks three things before uploading it with the other assets:
  - its subjects are exactly the release files (`check`);
  - every file verifies offline against it (`gh attestation verify
    --bundle`);
  - its bundle is exactly GitHub's stored provenance attestation
    (`match`).

  After the upload it compares the live copy. `verify-release-download.yml`
  repeats these checks. For releases without the file, it builds one from
  the stored attestations, so every run exercises this path; dry runs can't
  sign anything.

  0.4.3 and 0.5.0 were signed by `release-binaries.yml` itself (use that
  file name as the signer) and have no SBOM.
- **Reproducibility:** `reproducible-build.yml` builds the same commit twice
  on fresh runners with `build-binaries-core.yml` and compares every executable
  and archive (weekly, and on PRs that change the build). Its results are
  recorded as they are; see
  [the reproducibility record](evals/2026-11-15-reproducible-builds.md).
- **What a backfill's provenance names:** it records the workflow run's
  revision (main), not the tagged commit. Each archive's `README.txt`
  therefore states the tag and commit it was built from ("Built from"), and
  that file is inside the attested archive.
- **Not code-signed:** users see Windows SmartScreen and macOS Gatekeeper
  prompts. The README's install section says what to do.

### Guards

- **Release must match:** the workflow uploads only to an existing,
  published release whose body equals the tagged `docs/releases/vX.Y.Z.md`
  (`scripts/check-release-docs.py --published-tag`).
- **Never replace:** it refuses if any asset with the same name is already
  on the release. It never uses `--clobber` and never deletes an asset. A
  published asset is never replaced; if a binary is wrong, ship a new
  version.
- **Least privilege:**
  - only the publish job can write contents (upload assets);
  - only the attest job in `build-binaries.yml` requests an OIDC token and
    writes attestations, and that workflow runs only when publishing;
  - dry runs and the reproducibility check call the read-only
    `build-binaries-core.yml`, which has no signing job;
  - all other jobs are read-only.
- **Verified after upload:** the publish job downloads the live assets,
  checks them against `SHA256SUMS`, and verifies every attestation.

### Dry runs and backfills

- **Pull requests** that change the app (`src/`, `Cargo.toml`,
  `Cargo.lock`), the workflow, `packaging/` or the launch scripts dry-run
  their own head as the next release would build it:
  - the version from `Cargo.toml`, without the tag checks (the tag doesn't
    exist yet);
  - build, smoke-test, package, launch-test on old Linux distributions and
    checksum, on all four targets;
  - check the committed release documentation;
  - upload nothing. The archives' `README.txt` says "pull request (dry run,
    not a release)".

  So a release PR proves its own binaries before the merge.
- **Manual dispatch** (Actions → Release binaries → Run workflow):
  - `tag`: an existing release tag;
  - `dry_run`: true by default, which builds and verifies without uploading.

  To backfill a release, run it first as a dry run, then with `dry_run`
  off.
