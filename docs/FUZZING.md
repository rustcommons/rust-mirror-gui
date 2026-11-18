# Fuzzing

rust-mirror-gui fuzzes the code that reads input it doesn't control:
- the worker's event stream;
- the settings file;
- the company CA file a user picks.

The targets are in [`fuzz/`](../fuzz), a cargo-fuzz (libFuzzer) crate with
its own workspace and lockfile. It is never published, and the shipped
crate doesn't depend on it: `libfuzzer-sys` and `arbitrary` are
dependencies of the fuzz crate only (owner-approved for fuzzing,
2026-11-17). `fuzz/Cargo.lock` starts from the main `Cargo.lock`, so the
fuzzers test the same dependency versions that ship.

## Targets

| Target | What it feeds | What must hold |
| --- | --- | --- |
| `event_line` | One event line into `protocol::Event::parse` | Never panics: any bytes, nesting or field types become an event, `Unknown` or `Malformed` |
| `event_stream` | A whole worker stdout into `job::read_events` (the line reader, here with a 4 KiB limit so the overlong-line path is reached), with every event folded into `job::Progress::apply` | Never panics or loops; an overlong line stops reading |
| `settings` | The settings file into `settings::from_bytes` | Anything gives settings (a damaged file means defaults), and saved settings read back unchanged |
| `ca_file` | A CA file into `rust_mirror::network::inspect_ca_file`, which the worker's `inspect-ca` uses | Errors are fine; never panics |

**Seed corpora:** `fuzz/seeds/<target>/`, taken from the test fixtures:
- the worker output in `tests/fixtures/layout/*.jsonl`, whole and line by
  line;
- the event lines in `src/protocol.rs`'s tests;
- `tests/fixtures/test-ca.pem`, whole, truncated and doubled.

## In CI (ClusterFuzzLite)

- **Pull requests:** [`cflite_pr.yml`](../.github/workflows/cflite_pr.yml)
  runs on PRs that change `src/`, `fuzz/`, `Cargo.toml`, `Cargo.lock` or
  `.clusterfuzzlite/`. It builds the targets with AddressSanitizer and
  fuzzes the ones the change affects for 300 seconds.
  - Dependabot PRs skip it.
  - A new push cancels the PR's older run.
- **Weekly:** [`cflite_batch.yml`](../.github/workflows/cflite_batch.yml)
  fuzzes every target for 1800 seconds. It runs on Wednesdays, or by hand,
  and grows the corpus that PR fuzzing starts from.
- **Crashes** fail the run, and are uploaded to code scanning as SARIF.
- **Build image:** `.clusterfuzzlite/` holds the Dockerfile, `build.sh` and
  `project.yaml`. The OSS-Fuzz Rust base image is pinned by digest, which
  pins its nightly toolchain too; Dependabot proposes digest updates.
  Sanitizer builds need nightly, so the project's stable 1.95.0 isn't used
  here.

**A crash is a real bug.** Fix it, add a regression test (a unit test, or
the input in `fuzz/seeds/<target>/`), and say so in the changelog. If the
fix needs a decision, report it instead.

## Locally

You need a nightly Rust toolchain and cargo-fuzz, installed reproducibly:

```sh
rustup toolchain install nightly --profile minimal
cargo install cargo-fuzz --version 0.13.2 --locked
cargo +nightly fuzz list
```

Run a target. New inputs go to `fuzz/corpus/<target>` (git-ignored); the
seeds are only read:

```sh
mkdir -p fuzz/corpus/event_stream
cargo +nightly fuzz run event_stream fuzz/corpus/event_stream fuzz/seeds/event_stream \
  -- -max_total_time=300 -max_len=65536
```

`cargo +nightly fuzz build` builds all targets without running them. To
reproduce a crash, pass its file:
`cargo +nightly fuzz run <target> fuzz/artifacts/<target>/crash-…`.
