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
   waits for the release from step 3, then attaches the binaries described
   below. Check its run, and record it in the publication record.

## Prebuilt binaries

Authorized by the owner on 2026-11-15 for every release from then on.
Earlier releases stay notes-only, except 0.4.2, which was backfilled. The
asset names and archive layout are shared with rust-mirror.

| Asset | Built on |
| --- | --- |
| `rust-mirror-gui-X.Y.Z-x86_64-pc-windows-msvc.zip` | `windows-2022` |
| `rust-mirror-gui-X.Y.Z-x86_64-unknown-linux-gnu.tar.gz` | `ubuntu-22.04`, the oldest available runner, for the widest glibc support; the run log records the minimum glibc |
| `rust-mirror-gui-X.Y.Z-aarch64-apple-darwin.tar.gz` | `macos-15` (macOS 11 or newer) |
| `rust-mirror-gui-X.Y.Z-x86_64-apple-darwin.tar.gz` | `macos-15-intel` (macOS 11 or newer) |
| `SHA256SUMS` | `sha256sum` format: lowercase hex, two spaces, sorted by file name |

- **Archive layout:** each archive holds one folder,
  `rust-mirror-gui-X.Y.Z-<target>/`, with the executable, `LICENSE` and
  `README.txt`. `packaging/package.py` makes the archives with fixed
  timestamps and permissions.
- **How they are built:**
  - Rust 1.95.0 (the MSRV and CI toolchain);
  - `cargo build --release --locked`;
  - from exactly the tagged commit; the workflow refuses if the tag, the
    commit and `Cargo.toml` disagree.
- **Smoke tests before upload:**
  - `rust-mirror-gui --worker --version` must report protocol 1 and its
    rust-mirror version;
  - on Windows, the window is also launched and captured with PrintWindow,
    and must not be blank.
- **Attestations:** every archive and `SHA256SUMS` get a GitHub artifact
  attestation (Sigstore build provenance), using
  `actions/attest-build-provenance` v4.2.2 pinned by commit. They are
  verified before and after the upload. Users verify with
  `gh attestation verify <file> --repo rustcommons/rust-mirror-gui`.
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
- **Least privilege:** only the final publish job can write contents,
  request an OIDC token and write attestations. All other jobs are
  read-only.
- **Verified after upload:** the publish job downloads the live assets,
  checks them against `SHA256SUMS`, and verifies every attestation.

### Dry runs and backfills

- **Pull requests** that change the workflow or `packaging/` dry-run a
  backfill of the latest release:
  - its tag and its source, with all of the tag checks;
  - build, smoke-test, package and checksum on all four targets;
  - run the release guard read-only;
  - upload nothing.

  Manual dispatch only works once the workflow is on `main`.
- **Manual dispatch** (Actions → Release binaries → Run workflow):
  - `tag`: an existing release tag;
  - `dry_run`: true by default, which builds and verifies without uploading.

  To backfill a release, run it first as a dry run, then with `dry_run`
  off.
