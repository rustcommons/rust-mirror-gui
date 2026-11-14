# Windows qualification of the crate workflow (2026-11-14)

Owner-approved task, 2026-11-13. This is a repeatable GitHub Actions job,
[`windows-qualification.yml`](../../.github/workflows/windows-qualification.yml)
(manual dispatch, and pull requests that change it). It drives the crate
workflow end to end on Windows through the GUI's own worker,
`rust-mirror-gui --worker`, built in release mode. It publishes to an
**owned, native Windows Nexus OSS** and builds a real project offline from it.
It then launches the window itself and captures it.

**Evidence run:**
[43564858059](https://github.com/rustcommons/rust-mirror-gui/actions/runs/43564858059)
(head `1bde7cf`, which is main `12cfa02` (rust-mirror-gui 0.4.0 with rust-mirror
0.5.0) plus this harness), success, 10.5 minutes. The run's artifact
`windows-qualification-evidence` holds:
- `summary.json` (cases, Nexus identity) and `cleanup.json`;
- `requests.json` (method, path and status for every request through the TLS
  front; no headers);
- `revocation-requests.json`;
- `work/evidence.json` (every worker step with its exact arguments, outcome
  and timing), the catalog and the Client setup snippet;
- the client fetch, build and run output;
- the window capture.

## Environment

- GitHub-hosted `windows-latest`: Windows Server 2025 Datacenter (build 26100).
  Display adapter "Microsoft Hyper-V Video"; **no GPU**. Primary screen
  1024×768.
- Rust 1.95.0 (`x86_64-pc-windows-msvc`), Python 3.12.
- **Nexus:** official Nexus OSS 3.76.1-01 win64 bundle, SHA-256
  `8522ed37…6cdb8991`, checked against Sonatype's published checksum. It ran
  with its bundled JRE (Zulu 17.0.10) on 127.0.0.1, and the server reported
  `Nexus/3.76.1-01 (OSS)`. No EULA was accepted and no service was
  registered. The owned instance, its data and its TLS keys were deleted
  afterwards (`cleanup.json`: removed, keys destroyed).
- Real network access to crates.io.

## What runs, and whose code it is

- **rust-mirror's Windows Nexus harness, reused unchanged.** The workflow
  checks out rustcommons/rust-mirror `scripts/` at `5850dcf` (the PR #39
  merge, reviewed source `012ea84`). From it:
  - `nexus()` downloads, verifies, starts and deletes the owned Nexus
    instance;
  - `certificates()` makes a disposable CA, a signed CRL served over local
    HTTP, and a 127.0.0.1 server certificate whose CRL distribution point
    names that CRL;
  - `tls_proxy()` puts HTTPS with that certificate in front of Nexus.
- **[`qualification/run_gui_nexus_windows.py`](../../qualification/run_gui_nexus_windows.py)**
  uses those pieces.
  - It sets up the same least-privilege layout as rust-mirror's proof:
    - a raw hosted repository with write policy ALLOW_ONCE and strict content
      types;
    - a publisher role limited to that repository;
    - anonymous read-only access;
    - a random replacement admin password.
  - It then runs the steps below. Credentials are random per run and passed
    only in child environments. The test asserts the publisher and admin
    passwords are absent from `evidence.json`.
- **[`tests/windows_qualification.rs`](../../tests/windows_qualification.rs)**
  is an ignored integration test. It builds every command with the window's
  own `ops::*` builders and runs it with the window's `Job` process client.
  That is the same code path as pressing the buttons; only the clicks are
  skipped. The worker is the release `rust-mirror-gui.exe`.

## Results (run 43564858059)

| Step (window equivalent) | Result |
| --- | --- |
| Paste from Cargo.toml: `anyhow`, `itoa` and a path dependency | 2 usable; the path dependency refused (kind `path`, line 4) |
| Save catalog (the window's "New catalog" shape plus the pasted crates; publish address = the Nexus repository; one download at a time) | Written by rust-mirror as TOML |
| Cargo.lock import of [`qualification/demo/Cargo.lock`](../../qualification/demo/Cargo.lock) (17 packages, format 4, SHA-256 `1e1b2e72…6114f7f`): preview, then write | Accepted: 16 pins added, the project itself skipped as `local` (line 41). Written. |
| Preview with download sizes | `needs_approval`: 16 crates, 16 versions, 34 files, all 16 versions attributed to the `demo` lock source; no budget exceeded |
| Approve this preview and download, then **hard kill** (the window's "End process now": `TerminateProcess`) | Killed after 3 of 16 files (249,148 bytes) |
| Resume download (same approved plan) | Completed: 16/16 verified, 249,148 bytes reused, 2,091,600 downloaded |
| Second mirror with the first as previous: preview, approve | Completed, all reused |
| Discover generations; crate retention preview (keep newest 1, previous 0, next previous run-002) | Would remove run-001. run-002 is kept, with keep source Cargo.lock "demo". Nothing deleted. |
| Client setup | `replace-with = "org-mirror"`, `registry = "sparse+https://127.0.0.1:57226/repository/rust-mirror-gui-win-e1190f3e551d/crate-generation/cargo/index/"` |
| Publish: check destination (no writes) | 34 files, 2,507,979 bytes, all absent |
| Publish to Nexus as the scoped publisher | 34 files uploaded; every remote byte read back and verified; inventory schema 5 |
| Check destination again | Refused with `remote_exists` before any write |

**Nexus's own write policy.**
- Re-sending a published `.crate` (`aho-corasick-1.1.5.crate`) with its own
  bytes, as the publisher, got **400**. The original stayed byte-identical.
  Re-sending the identical bytes rules out content-type validation as the
  reason for the refusal.
- An anonymous PUT got **401**.

Requests to the repository through the TLS front: 68 `HEAD` 404 (two absence
checks), 34 `PUT` 201, 68 `GET` 200 (verification, the original-kept check and
the Cargo client), 1 `HEAD` 200 (the refused second check), 1 `PUT` 400 and
1 `PUT` 401.

### Cargo client on Windows

The client was a fresh `CARGO_HOME` containing only the Client setup snippet,
with the test CA trusted through `CARGO_HTTP_CAINFO` and SSL verification on.
- `cargo fetch --locked` downloaded all 16 crates from `org-mirror`
  (anonymous read).
- `cargo build --locked --offline` built the fixture project, and it printed
  `{"word":"mirror","count":2} 2 4`.
- `Cargo.lock` was byte-identical afterwards.

**Revocation checking stayed on.** Windows' TLS stack, which Cargo uses on
Windows, fetched the CRL (`/fixture.crl`, once) through the certificate's
distribution point. rust-mirror's publisher (rustls with the given CA) does
not check revocation.

As rust-mirror found, Cargo's CAINFO file trust is enough on Windows. Windows
Rustup needs the CA in the Windows certificate store, but Rustup is not part
of this crate qualification.

### The window on a GPU-less Windows Server

[`qualification/capture-window.ps1`](../../qualification/capture-window.ps1)
started the release `rust-mirror-gui.exe` with no `WGPU_BACKEND` (the 0.4.0
Windows default, Direct3D 12). It waited for the window titled
"rust-mirror GUI", waited 15 s, and captured the window with `PrintWindow`
(`PW_RENDERFULLCONTENT`).

- Graphics modules loaded by the app: `d3d10warp.dll`, `d3d12.dll`,
  `D3D12Core.dll`, `dxgi.dll`, `opengl32.dll`. **Direct3D 12 ran on WARP**,
  Windows' software rasterizer.
- The window drew its full first screen: 372 distinct colors, with the most
  common color covering 82.6%. Every run since the capture was fixed gave the same figures. The top
  bar reads "rust-mirror 0.5.0 built in · event protocol 1", and the sidebar
  shows the CRATES and MANAGE sections.
- The window is 1040×807, larger than the 1024×768 screen, so the right edge
  of the page is cut off. The app asks for 1280×860 with a 900×600 minimum.
- The light theme follows the runner's system setting.

![rust-mirror-gui 0.4.0 on Windows Server 2025, no GPU, Direct3D 12 on WARP](../screenshots/windows-server-2025-dx12-no-gpu.png)

**This is a partial no-GPU data point** for the
[no-GPU and Remote Desktop test plan](../plans/NO-GPU-TEST-PLAN.md),
environment W1 (Server 2025 only). It covers plan steps 1–2 (launch, adapter)
and one screenshot from step 3. It does not cover the rest of that plan:
- input, every screen, or the window doing a sync;
- resource use, resizing or scaling;
- Remote Desktop;
- Server 2022.

The crate workflow above ran through the worker, not by clicking the window.

## Earlier runs on this branch

- **Local HTTPS stand-in (superseded).** Before rust-mirror's Windows Nexus
  harness landed, the same worker workflow ran against a local HTTPS
  stand-in for Nexus raw. Run
  [43563103813](https://github.com/rustcommons/rust-mirror-gui/actions/runs/43563103813)
  passed, with the same certificates and CRL and the same Cargo client
  result. The stand-in was then removed in favor of real Nexus.
- **Harness problems, not app bugs:**
  - [43560079332](https://github.com/rustcommons/rust-mirror-gui/actions/runs/43560079332):
    - From PowerShell, `bash` is WSL's `bash.exe`, so the first certificate
      script never ran.
    - `MainWindowHandle` first pointed at a 16×16 untitled helper window. The
      capture now waits for the titled window.
  - [43560831374](https://github.com/rustcommons/rust-mirror-gui/actions/runs/43560831374):
    Git Bash rewrote `-subj "/CN=…"` into a path.
  - [43561396336](https://github.com/rustcommons/rust-mirror-gui/actions/runs/43561396336):
    - "publication transport failed" at the first HEAD. The stand-in had been
      started in an earlier step, and the runner ends a step's processes
      when the step finishes; its log shows no HEAD request reached it.
      Everything that needs the server now runs in one step.
    - That early harness also turned Cargo's revocation check off. It was
      replaced by rust-mirror's CRL-serving `certificates()`, as the
      coordinator advised, and revocation has stayed on since.

## Limits

- One owned, fresh Nexus OSS 3.76.1 on the same machine, behind a local TLS
  front with a disposable CA. Not covered:
  - Nexus Pro;
  - other Nexus versions;
  - a remote server across a corporate network or proxy;
  - an enterprise CA chain.
- One small real project (16 crates, about 2.4 MiB). The hard kill landed
  after 3 files with one download at a time. Larger catalogs and kills during
  a single large file were not covered.
- The window was launched and captured but not driven. Nothing ran over
  Remote Desktop, and nothing ran on Server 2022 or Windows 10/11 desktop
  editions. The owner's Windows 11 + RX 580 launch check is in the
  [0.4.0 validation record](2026-11-13-v0.4.0-validation.md).
- No release was needed: no app bug was found.
