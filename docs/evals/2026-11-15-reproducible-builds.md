# Reproducible release builds (2026-11-15)

`reproducible-build.yml` builds one commit twice with the release build
(`build-binaries-core.yml`), on separate fresh GitHub-hosted runners, and
compares the SHA-256 of every executable and archive. These are the first
two runs, on PR #23 (the security baseline). The source was version 0.5.0's.
Nothing was published.

## Run 1: Windows differed

[Run 43752801827](https://github.com/rustcommons/rust-mirror-gui/actions/runs/43752801827),
head `d636b0d`:

| Archive | Executable | Archive file |
| --- | --- | --- |
| aarch64-apple-darwin | identical | identical |
| x86_64-apple-darwin | identical | identical |
| x86_64-unknown-linux-gnu (AlmaLinux 8) | identical | identical |
| x86_64-pc-windows-msvc | **different** | **different** |

The two Windows executables (27,135,488 bytes each) differed in 24 bytes, all
of them build stamps the MSVC linker writes by default:
- the PE header's `TimeDateStamp` (offset 0x108);
- three debug-directory timestamps;
- the 16-byte PDB GUID.

## Fix

The Windows release build now links with `/Brepro`
(`RUSTFLAGS='-C target-feature=+crt-static -C link-arg=/Brepro'`, release
builds only). With it, the linker writes hashes of the content instead of
the time, so identical input gives identical output.

## Run 2: all identical

[Run 43756146177](https://github.com/rustcommons/rust-mirror-gui/actions/runs/43756146177),
head `f70e5fe`:

| Archive | Executable | Archive file | Executable SHA-256 |
| --- | --- | --- | --- |
| aarch64-apple-darwin | identical | identical | `bfd27a135a56d8049d6628839033f5bf53ab46ef88174a165092393fedafe8ea` |
| x86_64-apple-darwin | identical | identical | `100f993b868033bcea4f2d842bf4841714f5a3677b57c24edc2afb37600d62c8` |
| x86_64-pc-windows-msvc | identical | identical | `436b06fdd245958213444c669d2a2aa09a6b7f08ed980e4a1acb531fddba103c` |
| x86_64-unknown-linux-gnu | identical | identical | `8acd040d1a2e7e3f305d56848258fb424236dc69dcba80170dbb07cded2d58dc` |

rust-mirror measured the same: its Windows builds became identical with the
same flag (rust-mirror PR #51, run 43759286498).

## Limits

- **Same pinned environment:** both builds used the same runner images,
  the same container digest (Linux) and Rust 1.95.0. The check doesn't show
  that a different toolchain or machine would give the same bytes.
- **SBOM not compared:** CycloneDX gives every SBOM a new serial number and
  timestamp, so the SBOMs differ by design. The release attests the one it
  publishes.
- **Ongoing:** the check runs weekly and on changes to the build, and
  records its result each time.
