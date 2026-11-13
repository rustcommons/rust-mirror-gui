# Changelog

## [Unreleased]

- Documentation: add the 0.3.0 [publication record](docs/evals/2026-11-13-v0.3.0-publication.md).
- Documentation: add the 0.4.0 [publication record](docs/evals/2026-11-13-v0.4.0-publication.md), the
  Windows renderer finding in the [0.4.0 validation record](docs/evals/2026-11-13-v0.4.0-validation.md),
  and a tutorial troubleshooting entry for a blank window on Windows.

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
