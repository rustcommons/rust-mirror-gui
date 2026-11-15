# Releasing rust-mirror-gui

This is the repository's own procedure. It follows the Rust Commons
[release policy](https://github.com/rustcommons/.github/blob/main/docs/RELEASING.md).
Publishing needs the owner's authorization for each release; the coordinator
publishes from the owner's machine.

## Steps

1. **Release PR:**
   - the version in `Cargo.toml`;
   - a dated `CHANGELOG.md` entry;
   - `docs/releases/vX.Y.Z.md`;
   - evidence under `docs/evals/`.

   CI must be green (the release-documentation check included).
2. **Merge and publish** (coordinator, owner-authorized): merge normally, then
   run `cargo publish --locked` from a clean worktree at the merge.
3. **Post-publication** (maintainer):
   - verify the registry archive against the merge;
   - verify docs.rs and a fresh `cargo install --locked`;
   - create the annotated tag `vX.Y.Z` on the merge;
   - create the GitHub Release, whose body is exactly `docs/releases/vX.Y.Z.md`;
   - run the release-documentation checks;
   - open a publication-record PR.
4. **Binaries:** pushing the tag starts
   [`release-binaries.yml`](../.github/workflows/release-binaries.yml). It
   waits for the release from step 3, has the reusable
   [`build-binaries.yml`](../.github/workflows/build-binaries.yml) build
   them (with
   [`build-binaries-core.yml`](../.github/workflows/build-binaries-core.yml))
   and sign them, then attaches the binaries described below. The same tag push
   also starts [`trusted-publishing.yml`](../.github/workflows/trusted-publishing.yml).
   While the owner publishes by hand (step 2), it only checks that the
   published crate is the tag's source. Check its run, and record it in the publication record.
5. **Downloads, as a user gets them:** run
   [`verify-release-download.yml`](../.github/workflows/verify-release-download.yml)
   (Actions → Verify release downloads, `tag`). It downloads the live assets,
   checks `SHA256SUMS` and the attestations, and runs the Windows and Linux
   archives. It is read-only. Record its run too.

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
