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

1. **Release PR (maintainer):**
   - the version in `Cargo.toml`;
   - a dated `CHANGELOG.md` entry;
   - `docs/releases/vX.Y.Z.md`;
   - evidence under `docs/evals/`.

   CI must be green at the exact head: the release-documentation check, and
   the release dry run (all builds, the old-Linux launches without system
   fonts, the Windows capture). Before a release PR merges, also run the
   reproducible-build check on it.
2. **Merge** (coordinator, owner-authorized): merge normally.
3. **Tag and release notes (maintainer), together:**
   - create the annotated tag `vX.Y.Z` on the merge and push it;
   - right away, create the GitHub Release whose body is exactly
     `docs/releases/vX.Y.Z.md`.

   The tag push starts two workflows:
   - [`trusted-publishing.yml`](../.github/workflows/trusted-publishing.yml)
     waits for the owner to approve the `crates-io` environment, runs
     `cargo publish --locked`, then checks that the published crate is the
     tag's source (checksums, `.cargo_vcs_info.json`, every file);
   - [`release-binaries.yml`](../.github/workflows/release-binaries.yml)
     waits up to 30 minutes for the GitHub Release, has the reusable
     [`build-binaries.yml`](../.github/workflows/build-binaries.yml) build
     them (with
     [`build-binaries-core.yml`](../.github/workflows/build-binaries-core.yml))
     and sign them, then attaches the binaries described below.

   Create the release promptly: only the tag-push run produces attestations
   whose source and signer are the release commit. A later manual run is
   signed at another commit and fails the strict verification.
4. **Post-publication (maintainer)**, once both runs succeeded:
   - verify the registry archive against the merge;
   - verify docs.rs and a fresh `cargo install --locked`;
   - verify the live assets, `SHA256SUMS` and every attestation (signer
     `build-binaries.yml`, source and signer digest = the release commit,
     SBOM bindings);
   - run the release-documentation checks;
   - open a publication-record PR.
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
| `rust-mirror-gui-X.Y.Z.intoto.jsonl` | from 0.5.2 on (name shared with rust-mirror): the release's signed provenance, made by `build-binaries.yml`. JSON Lines, one Sigstore bundle per line: the SLSA build-provenance attestation, whose subjects are exactly the archives, the SBOM and `SHA256SUMS`. The SBOM attestation stays in GitHub's store. Scorecard's Signed-Releases check recognizes it, and users verify offline with it. **Not in `SHA256SUMS`:** it is signed itself, it is made after `SHA256SUMS` is signed, and `SHA256SUMS` is one of its subjects |

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
