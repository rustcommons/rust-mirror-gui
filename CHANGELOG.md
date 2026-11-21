# Changelog

## [Unreleased]

- **Signed provenance on each release** (from the next release on): the
  release also gets `rust-mirror-gui-X.Y.Z.intoto.jsonl`, the Sigstore
  bundle of its build-provenance attestation (JSON Lines; the same design
  as rust-mirror).
  - Scorecard's Signed-Releases check recognizes it.
  - Users can verify offline with `gh attestation verify --bundle`.
  - The release and download checks confirm its subjects are exactly the
    release files and that it matches GitHub's stored attestation.

  It is not in `SHA256SUMS`. Existing releases are unchanged.

- **Fuzzing:**
  - four cargo-fuzz targets in `fuzz/`: worker event lines, the whole event
    stream with its line-size limit, the settings file, and company CA
    files;
  - seed corpora from the test fixtures;
  - ClusterFuzzLite fuzzes PRs that change the code for 300 s, and runs a
    weekly 1800 s batch; crashes go to code scanning.

  The event-line reader moved into `job::read_events` and settings
  decoding into `settings::from_bytes`, with new unit tests; behaviour is
  unchanged. See [docs/FUZZING.md](docs/FUZZING.md).
- **Pinned dependencies:** no download is piped into an interpreter any
  more (`trusted-publishing.yml`). The download check runs an archive only
  after its SHA-256 matched the attested `SHA256SUMS`. Dependabot also
  updates the ClusterFuzzLite image digest.
- **Rules** (AGENTS.md): never `[skip ci]`; every install pinned.

- Documentation: add the 0.5.1 [publication record](docs/evals/2026-11-16-v0.5.1-publication.md),
  and the SLSA 3 badge, after the live verification of the release's
  attestations.
- Testing: `verify-release-download.yml` checks v0.5.1 by default.

## [0.5.1] - 2026-11-16

The first release whose downloads come with an SBOM, SLSA Build Level 3
provenance and reproducible builds, published to crates.io by Trusted
Publishing. The built-in engine is rust-mirror 0.5.4. See the
[versioned notes](https://github.com/rustcommons/rust-mirror-gui/blob/main/docs/releases/v0.5.1.md).

- **Built on rust-mirror 0.5.4** (the same engine code as 0.5.3, with its
  patched dependency lineage). The lock now has rustls 0.23.46,
  rustls-webpki 0.103.17 (was 0.103.16) and rustix 1.1.6; cargo-deny
  reports no advisories.
- **Published by Trusted Publishing:** `trusted-publishing.yml` (with the
  owner's approval of the `crates-io` environment) is now the only way to
  publish; no crates.io API token exists.
- **Security baseline** (owner-approved 2026-11-15):
  - Dependabot proposes monthly version updates for Cargo and GitHub
    Actions (minor and patch grouped; 7-day cooldown; at most 2 open PRs per
    ecosystem); security updates come at once. Its PRs run only the core
    checks, and every PR workflow cancels a PR's superseded runs. Every
    action is pinned to a full commit SHA.
  - CI now runs zizmor (workflow security lint) and cargo-deny (RustSec
    advisories, a license allowlist, crates.io-only sources; see
    `deny.toml`). Four unmaintained crates that come only through eframe,
    with no safe upgrade, are listed there with reasons.
  - OpenSSF Scorecard runs on `main` and weekly.
- **Release builds** (from the next release on):
  - The build moved into the reusable, read-only `build-binaries-core.yml`.
    For releases, `build-binaries.yml` runs it and signs the attestations
    (SLSA Build L3).
  - Each release gets a CycloneDX SBOM (`rust-mirror-gui-X.Y.Z.cdx.json`),
    listed in `SHA256SUMS` and attested for every archive.
  - `reproducible-build.yml` builds twice and compares the results. With
    `/Brepro` on the Windows link, all four targets' executables and
    archives were bit-identical
    ([record](docs/evals/2026-11-15-reproducible-builds.md)).
  - The release and download checks verify that attestations were signed by
    the release commit's `build-binaries.yml` on GitHub-hosted runners, and
    that the attested SBOM is exactly the published one.
- **Trusted Publishing:** `trusted-publishing.yml` is ready for crates.io
  Trusted Publishing; the current publishing process is unchanged until
  the owner enables it.
- Documentation: add the 0.5.0 [publication record](docs/evals/2026-11-15-v0.5.0-publication.md).
- Testing: `verify-release-download.yml` checks v0.5.0 by default and, from
  0.5.0 on, runs the Linux archive with no system fonts.

## [0.5.0] - 2026-11-15

Built-in fonts, and documented supported platforms. See the
[versioned notes](https://github.com/rustcommons/rust-mirror-gui/blob/main/docs/releases/v0.5.0.md).

- **Supported platforms are documented:** what is tested, what is expected
  to work, and what isn't supported (with the rust-mirror command line as
  the alternative). They are in the README, the tutorial, the archives'
  `README.txt` and the About page.
- **Measured: how old a Linux the GUI runs on** (CI only, nothing
  published). Below glibc 2.28, old X and graphics libraries stop the
  window even when a glibc 2.17 build loads. See the
  [record](docs/evals/2026-11-15-glibc-experiments.md). The Linux download
  stays the glibc 2.28 build.
- **Fonts are built in.** The app bundles DejaVu Sans 2.37 (free license, see
  `NOTICE`) after egui's own fonts and no longer looks up system fonts. On
  minimal Linux systems without fontconfig or fonts, symbols such as ✔, ✖
  and ⚠ showed as empty boxes; they now always draw. fontconfig and DejaVu
  are no longer in the documented Linux runtime packages. Prebuilt archives
  include `LICENSE-DejaVu.txt`. The executable is about 0.8 MB larger.
- Documentation: add the 0.4.3 [publication record](docs/evals/2026-11-15-v0.4.3-publication.md).
- Testing: `verify-release-download.yml` checks a published release's
  downloads as a user gets them (checksums, attestations, and running the
  Windows and Linux archives); read-only.

## [0.4.3] - 2026-11-15

The window opens on Linux machines without a GPU that have an older Mesa,
and this is the first release with prebuilt downloads. See the
[versioned notes](https://github.com/rustcommons/rust-mirror-gui/blob/main/docs/releases/v0.4.3.md).

- **Fixed: the window did not open on some Linux machines without a GPU,**
  for example Ubuntu 20.04, with "WGPU error: Limit 'max_color_attachments'
  value 8 is better than allowed 2". The app now asks the graphics driver
  for no more than it offers, but never less than egui needs; if the driver
  offers less than that, it says which limits are short.
- **Prebuilt binaries** on GitHub Releases (owner-authorized 2026-11-15),
  for the first time:
  - Windows x86-64 (`.zip`, C runtime linked statically, so no Visual C++
    Redistributable is needed), Linux x86-64 (`.tar.gz`, glibc 2.28 or
    newer), and macOS Apple silicon and Intel (`.tar.gz`);
  - built from the tagged commit by `release-binaries.yml`, with
    `SHA256SUMS` and GitHub build-provenance attestations;
  - not code-signed (the README explains the SmartScreen and Gatekeeper
    prompts).

  The README now lists "Download a prebuilt binary" first, and
  [docs/RELEASING.md](docs/RELEASING.md) describes the release procedure.
  Earlier versions have no downloads.
- Documentation: add the 0.3.0 [publication record](docs/evals/2026-11-13-v0.3.0-publication.md).
- Documentation: add the 0.4.0 [publication record](docs/evals/2026-11-13-v0.4.0-publication.md), the
  Windows renderer finding in the [0.4.0 validation record](docs/evals/2026-11-13-v0.4.0-validation.md),
  and a tutorial troubleshooting entry for a blank window on Windows.
- Documentation: add the 0.4.1 [publication record](docs/evals/2026-11-14-v0.4.1-publication.md).
- Documentation: add the 0.4.2 [publication record](docs/evals/2026-11-15-v0.4.2-publication.md),
  and a roadmap item for slower redraws when frames are slow.
- Testing: a repeatable Windows qualification workflow runs the crate
  workflow end to end through the app's worker (paste, Cargo.lock import,
  preview, approval, hard kill and resume, retention preview, Client setup,
  publish to an owned native Windows Nexus OSS), then has a fresh Cargo home
  build offline from the published mirror, and captures the window on a
  GPU-less Windows Server. See the [record](docs/evals/2026-11-14-windows-qualification.md).

## [0.4.2] - 2026-11-15

Lower CPU use while an operation runs, and large crate previews work. The
built-in engine is rust-mirror 0.5.1. See the
[versioned notes](https://github.com/rustcommons/rust-mirror-gui/blob/main/docs/releases/v0.4.2.md).

- **Fixed: high CPU use while an operation runs.** The window redrew for every
  line of worker output, and a details table that changed size on every
  redraw asked for another redraw, so the window redrew continuously during
  a download. With software rendering (Remote Desktop, machines without a
  GPU) that used several cores and slowed the download itself. The window
  now redraws once a second while an operation runs, and at once for input,
  the end of an operation, errors, retry waits and the stalled warning. The
  crate preview table and the activity log lay out only the rows in view.
  Window CPU during a crate download inside Remote Desktop, % of one core:
  Linux with xrdp 317% → 14.4%; Windows on Intel Xeon 190% → 8.6%; Windows
  on AMD EPYC 326% → 66%, where Windows' software renderer is about ten times
  slower per frame (a known limit).
- **Fixed: large crate previews failed in the window.** The worker's result
  for a preview of about 300 crate versions is over 1 MiB, which the window
  refused ("event line exceeds 1 MiB"). The limit is now 64 MiB.
- **rust-mirror 0.5.1:** some crates list a version more than once on
  crates.io, differing only in build metadata (for example `ndk-sys 0.4.0`,
  needed by eframe). rust-mirror 0.5.0 refused such previews ("duplicate
  index version"); 0.5.1 accepts them when none of those versions is
  selected and shows a warning, which the crate preview now displays. A
  selected version with a conflicting checksum is still refused, naming the
  crate and versions.
- The crate preview table has fixed columns; long flags and reasons end in
  "…" with the full text on hover. It lists up to 10,000 rows (was 400).
  Long lines in the activity log scroll sideways instead of wrapping.
- Diagnostics: `RUST_MIRROR_GUI_FRAME_LOG=<file>` writes, once a second, how
  often the window redrew and why; `RUST_MIRROR_GUI_PRESENT_MODE` picks the
  swap-chain present mode.
- Testing: a Remote Desktop qualification workflow runs the app inside a real
  RDP session on Windows Server 2022 and 2025; xrdp on Linux was tested by
  hand. See the [record](docs/evals/2026-11-15-remote-desktop.md).

## [0.4.1] - 2026-11-14

Narrow windows and display scaling. The built-in engine is still
rust-mirror 0.5.0. See the
[versioned notes](https://github.com/rustcommons/rust-mirror-gui/blob/main/docs/releases/v0.4.1.md).

- **Fixed: parts of pages could not be reached on narrow windows.** On a
  window narrower than about 1280 points, the Selections page's right
  column was cut off (for example **Refresh** and the "Other hosts" hint),
  with no way to scroll to it. Every page now stays fully usable down to the
  minimum window size (900×600 points), also at 125% and 150% display
  scaling:
  - the page area scrolls both ways, so nothing is out of reach;
  - hint lines, "Same as" lines and long values wrap instead of running off
    the page;
  - Selections puts its two columns under each other on narrow windows;
  - tables keep their columns and wrap their last column; long paths,
    reasons and file names were moved to the last column;
  - the top bar's **Show** button comes before the operation status, and a
    long status is shortened with "…" (the full text is in a tooltip and on
    the operation's page).
- **The first window fits the screen.** It no longer opens larger than the
  monitor (for example 1280×860 on a 1024×768 screen); it shrinks to fit,
  leaving room for the taskbar, but never below the minimum size.
- Testing: automated layout checks render every page at 900×600 points, at
  100%, 125% and 150% scaling, empty and with real reports recorded from the
  worker. Windows CI captures the first window and the window at its minimum
  size.

## [0.4.0] - 2026-11-13

Cargo.lock import and crate retention. The built-in engine is now
rust-mirror 0.5.0. See the
[versioned notes](https://github.com/rustcommons/rust-mirror-gui/blob/main/docs/releases/v0.4.0.md).

- **Windows blank window fixed:** render with Direct3D 12 on Windows unless
  `WGPU_BACKEND` is set. Some Vulkan drivers (seen on an AMD Radeon RX 580)
  showed only a blank window in 0.3.0.
- **Cargo.lock import** (Crate catalog page):
  - import one or more lock files as named sources, by path or by dropping
    them on the window; names are suggested from the project folder;
  - an import summary before anything is written: pinned versions added,
    removed and unchanged; skipped git and path entries with line numbers;
    refused registries and other problems with line, column and a fix; a
    warning when the project also needs git or path sources;
  - re-importing a source shows exactly what changed;
  - lock sources are listed as groups in the catalog, each with a remove
    preview (mirrored files are never deleted);
  - preview rows show which lock sources need each version and flag yanked
    versions kept for a lock file; the filter matches source names.
- **Crate retention page:** list the crate mirrors in a workspace folder;
  set the policy (next sync's previous, keep newest, previous to keep,
  pinned mirrors, as-of date) and save it in the catalog; preview the exact
  folders and bytes to remove with every keep reason, including the catalog
  crates and Cargo.lock sources that protect a mirror; delete only after
  typing `delete N`. rust-mirror previews again before deleting and refuses
  a stale preview, deleting nothing.
- Deletion jobs no longer show the re-check byte count as if it were
  deletion progress.

## [0.3.0] - 2026-11-13

Crate mirroring. The built-in engine is now rust-mirror 0.4.0. See the
[versioned notes](https://github.com/rustcommons/rust-mirror-gui/blob/main/docs/releases/v0.3.0.md).

- **Crate catalog page:**
  - load and save through the library (canonical TOML: comments and order are
    not kept, with a warning before saving);
  - paste a `[dependencies]` block, with per-line refusals (git, path,
    workspace, other registry, whole manifest) and plain-English fixes;
  - per-crate rules: version range, features, default features, newest N,
    pins, and aliases for two ranges of one crate;
  - platforms, size limits, publish address, contact and parallel downloads.
- **Crate preview & sync page:**
  - the frozen preview: one row per crate version with size or "unknown",
    yanked, lock-only and kept-from-previous flags, and the reason it is
    included;
  - a change summary against the previous mirror, the index-read time with a
    stale warning, and the library's budget flags;
  - **Why?** paths back to the catalog;
  - one approval for the whole preview;
  - download with files-verified progress, Stop and Resume, and typed retry
    reasons;
  - **Client setup** with the `.cargo/config.toml` snippet.
- **Generations list** labels crate mirrors with crate and version counts.
  Crate retention stays in the CLI.
- **Publish** accepts crate mirrors; a mismatched base URL is refused before
  any write.
- Uses the existing Network settings for crate downloads.
- **Documentation:**
  - a tutorial section "Mirroring crates", the README and these notes;
  - the 0.2.0 [publication record](docs/evals/2026-11-12-v0.2.0-publication.md);
  - a roadmap item for using system network settings by default.

## [0.2.0] - 2026-11-12

Works on a corporate network. Requires rust-mirror 0.3.0 (built in). See the
[versioned notes](https://github.com/rustcommons/rust-mirror-gui/blob/main/docs/releases/v0.2.0.md).

- **Network settings page** (per computer, never in the mirror configuration):
  - modes: direct, proxy from environment (the detected value is shown first),
    or manual proxy URL with an optional Basic login;
  - a NO_PROXY list;
  - an extra company CA file, added to the built-in roots, with its subject,
    issuer, expiry and SHA-256 fingerprint;
  - connect and read timeouts.
  - The proxy password is kept in memory only and handed to the worker over
    stdin. It never appears in argv, environment, logs or settings.
- **Test connection:** shows each step (DNS, connect, proxy login, TLS,
  download) as passed, failed or skipped, with a plain-English fix. When HTTPS
  inspection is detected, it shows "Certificate issued by <issuer>" from the
  presented chain and offers an explicit add-a-company-certificate flow: the
  user picks the PEM; nothing is trusted automatically.
- **Plan preview:** optional download-size estimates (HEAD requests), with a
  size column and an "about X of Y budget" bar.
- **Sync:** optional size estimates, with percent, speed and time left.
- **Selections:** a toolchain picker from rust-mirror's release list (stable
  versions with dates, recent beta and nightly builds). It loads from a local
  cache when offline, has a Refresh button, and still accepts free text.
- **Fix:** the first-run release-list load no longer reports a failure when
  there is no cache yet.
- **Documentation:**
  - the tutorial gains a Corporate network section with screenshots from the
    real app;
  - the README is updated;
  - the 0.1.0 publication record and tutorial were added after 0.1.0.

## [0.1.0] - 2026-11-12

First release: a desktop app for building offline Rust toolchain mirrors with
the rust-mirror library compiled in. See the
[versioned notes](https://github.com/rustcommons/rust-mirror-gui/blob/main/docs/releases/v0.1.0.md)
for validation and known limits.

- One executable. Every operation runs in a hidden `--worker` child process of
  the same program, which calls the rust-mirror library and reports over a
  JSON-lines event protocol. The window never blocks. It shows the live stage,
  files, bytes and time since the last event, and warns when the worker stops
  sending heartbeats.
- Uses rust-mirror 0.2.0 event protocol 1: CLI commands run through
  `rust_mirror::cli::run_with_events` with `--cancel-stdin`. The window parses
  typed events (stage, artifact, bytes, hashing, heartbeat with `progress_seq`)
  and typed error kinds.
- Stop asks the worker to stop safely; "End process now" is offered when it is
  stopping or stalled. Sync explains what is kept for Resume. Publish explains
  that partial remote files must not be activated.
- Pages:
  - Selections: writes a new configuration file.
  - Plan preview: metadata only.
  - Sync: new run, resume or start over.
  - Rustup finish.
  - Generations and retention: discover, preview a policy with the library's
    reasons, and delete only after a typed confirmation.
  - Publish to Nexus Raw or a static HTTPS host, with a no-write destination
    check, an optional CA bundle, and Basic credentials that are passed only to
    the worker.
- Each page shows the equivalent `rust-mirror` command.
