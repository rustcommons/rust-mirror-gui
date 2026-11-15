# Remote Desktop qualification (2026-11-14 to 2026-11-15)

Owner-approved task: steps 1 and 2 of the
[no-GPU and Remote Desktop test plan](../plans/NO-GPU-TEST-PLAN.md).
- Step 1 (environment W1, with real RDP): Windows Server 2022 and 2025 on
  disposable GitHub runners, no GPU.
- Step 2 (environment L3): Linux, with xrdp.

The question: does rust-mirror-gui open, draw, stay usable and complete a
sync inside a real Remote Desktop session with no GPU? Does it survive
disconnects, resolution and scaling changes, and moving between console and
RDP? Is its CPU use reasonable?

**Result:** yes, after two fixes found by this test (in 0.4.2):
- the window redrew continuously while an operation ran, using several
  cores with software rendering;
- large crate previews failed in the window.

**CPU, before and after** (GUI process only, % of one core, 4-core
machines, crate download with the preview on screen):

| Platform | 0.4.1 | 0.4.2 | Plan threshold (25%) |
| --- | --- | --- | --- |
| Linux, xrdp, Mesa llvmpipe/lavapipe | 317% | **14.4%** | pass |
| Windows Server 2025, RDP, WARP | 165.5% | **6.2%** (4.5% on the larger catalog) | pass |
| Windows Server 2022, RDP, WARP | 326.9% | **86.2%** (88.6% on the larger catalog) | **limit** (see below) |

Windows numbers are from the same run, same session and same 181-version
catalog for both builds (run 43676198817). The Linux numbers are from the
same session and catalog. Idle (no operation running) is 0–2.5% everywhere.

## Windows (step 1)

**Evidence run:**
[43676198817](https://github.com/rustcommons/rust-mirror-gui/actions/runs/43676198817)
(commit `2cd22ce`, the final app code; the harness was later changed only to
record the CPU limit below). Workflow:
[`remote-desktop.yml`](../../.github/workflows/remote-desktop.yml).

The scripts are in [`qualification/rdp/`](../../qualification/rdp). On a
disposable GitHub runner with no GPU, the workflow:
- enables Remote Desktop and creates a temporary local user, with a random
  password that is masked and never written down;
- installs the exact FreeRDP client package `freerdp 3.33.1-1` from MSYS2,
  checked by SHA-256 `B854F530…66A56F3A` (pacman also checks its signature);
- connects to `127.0.0.2`, pinning the RDP server certificate by SHA-256,
  so nothing is ignored.

The app runs as that user inside the RDP session, on the Microsoft Remote
Display Adapter. It is driven by UI Automation (egui exposes AccessKit) and
captured with PrintWindow, both from a scheduled task scoped to the
session. Disconnects use `tsdiscon`; console takeover uses `tscon` run as
SYSTEM. `cleanup.ps1` logs off the sessions and removes the user, its
profile, the tasks and the files.

The data was crates.io sync of 291 crate versions (tokio, reqwest, clap,
regex, serde, anyhow, aws-sdk-s3, aws-config), downloaded one at a time.

| Scenario | Server 2022 | Server 2025 |
| --- | --- | --- |
| Launch under 10 s, full draw | **Pass:** window 0.5 s, drawn 2.3 s | **Pass:** window 0.6 s, drawn 0.9 s |
| Renderer and adapter | **Pass:** Direct3D 12 on WARP (`d3d10warp.dll`, `d3d12.dll`, `D3D12Core.dll`); adapters Microsoft Hyper-V Video and Microsoft Remote Display Adapter | **Pass:** same |
| Idle CPU and memory (30 s) | **Pass:** 2.5%; working set 74 MiB, private 289 MiB | **Pass:** 0%; 81 MiB, 292 MiB |
| Live status during a download | **Pass:** transferred 2.5 → 5.7 → 9.2 → 13.1 MiB over 10 s | **Pass:** 4.2 → 8.2 → 12.1 → 14.7 MiB over 9 s |
| Stop | **Pass:** "Stopped: operation cancelled" at 91 files | **Pass:** at 114 files |
| Resume, disconnect mid-sync, reconnect | **Pass:** session "Disc" with GUI and worker running; after reconnecting it drew; "Verified crate mirror … 291 files" | **Pass:** same |
| Reconnect at 1600×900 | **Pass:** drew on the 1600×900 screen | **Pass** |
| Reconnect at 150% | **Pass:** DPI 144, drew | **Pass:** DPI 144, drew ([screenshot](../screenshots/rdp-windows-2025-150pct.png)) |
| Minimize the RDP client mid-sync | **Pass:** sync finished, drew after restore (GUI 89.6% while minimized) | **Pass:** GUI 4.3% while minimized |
| CPU during a download | **Limit:** 88.6% (0.4.1: 326.9%) | **Pass:** 4.5% (0.4.1: 165.5%) |
| Start on the console, take over through RDP | **Pass:** on the console (Hyper-V Video, 1024×768) it drew; after RDP takeover it drew | **Pass** ([on the console](../screenshots/rdp-windows-2025-on-console.png)) |

Screenshots: [downloading, Server 2025](../screenshots/rdp-windows-2025-downloading.png),
[after reconnecting, Server 2022](../screenshots/rdp-windows-2022-after-reconnect.png).
All captures are in the run's artifacts.

### The Server 2022 CPU limit

The frame log shows exactly 1 redraw a second during the 8-second sample.
Building each frame takes about 10 ms, yet the window uses 88% of a core.
So each frame costs about 0.9 s of CPU in Server 2022's software renderer
(WARP), against about 45 ms on Server 2025.

Presenting without vsync (`RUST_MIRROR_GUI_PRESENT_MODE=auto_no_vsync`)
made no difference: 92.3%, against 86.2% with the default.

The app cannot make that renderer faster. 0.4.2 cuts its use from 327% to
86–89%, which is about 22% of the 4-core machine. The remaining options, for
the owner to decide:
- redraw less often on very slow renderers (adaptive);
- accept it as a known limit of Server 2022 without a GPU.

The workflow records it as a limit and fails if CPU goes over 150%, which
would mean the 0.4.1 behaviour is back.

### Harness problems found on the way (not app bugs)

- The first CPU fix was measured only on Linux. On Windows it was invisible
  until the frame log showed a resizing egui grid requesting extra redraws.
- Large previews failed in the window: a real app bug, fixed (64 MiB).
- The harness was adjusted along the way:
  - it waits for the download stage before sampling, because the sync first
    resolves again for 10–20 s;
  - it samples transferred bytes, because file counts pause on large crates;
  - it runs `tscon` as SYSTEM;
  - it runs the 0.4.1 comparison on a catalog that 0.4.1 can preview.

Earlier runs: 43653491629, 43655102977, 43667562821, 43670573136,
43673582573.

## Linux with xrdp (step 2)

Hand-run in the cloud sandbox: Debian 12 aarch64, 4 cores, no GPU.
- xrdp 0.9.21.1, xorgxrdp 0.9.19 and the FreeRDP 2.11.7 client, all from
  Debian's signed repositories.
- Mesa 22.3.6.

xrdp listened only on 127.0.0.1. A temporary local user had a random
password. The FreeRDP client connected locally, pinning xrdp's certificate
by SHA-256 (`/cert:fingerprint:sha256:…`). The app ran as that user on the
session's X server (`Xorg :10` driven by xorgxrdp), and captures were taken
inside the session with `xwd`. The catalog was 181 crate versions (tokio,
reqwest, clap, regex, serde, anyhow), downloaded one at a time from real
crates.io. The user, sessions and files were removed afterwards.

| Scenario | Result |
| --- | --- |
| Launch and full draw | **Pass.** Window after 0.5 s, fully drawn (927 colors); fitted to the 1280×800 RDP desktop (1264×704). |
| Renderer | **Pass.** Software: Mesa lavapipe Vulkan (`libvulkan_lvp.so`) and llvmpipe GL (`swrast_dri.so`, LLVM 15) loaded; no GPU. |
| Preview, then download with live status | **Pass.** Status updated live: 54 files, then 65 one second later ([screenshot](../screenshots/rdp-linux-xrdp-sync-live.png)). |
| Stop, then Resume | **Pass.** "Stopped: operation cancelled" at 93 of 181, verified files kept ([screenshot](../screenshots/rdp-linux-xrdp-stopped.png)). Resume finished the same approved preview. |
| Disconnect mid-sync, reconnect | **Pass** (repeated with the final 0.4.2 build). With the client ended, the session's X server and the app (GUI and worker) kept running. After reconnecting, the window drew and showed the finished mirror: 181 files ([screenshot](../screenshots/rdp-linux-xrdp-finished-after-reconnect.png)). |
| Reconnect at another resolution | **Pass.** 1280×800 changed to 1440×900 (FreeRDP limits the requested 1600×900 to its own 1440×900 display); the window drew ([screenshot](../screenshots/rdp-linux-xrdp-after-reconnect.png)). |
| Minimize the RDP client mid-sync | **Pass.** The sync finished (181 files) and the window drew after restoring. The window kept using about 24% while minimized: the server session keeps running, so the app cannot tell. |
| Idle CPU and memory | **Pass.** 0% CPU over 30 s with a preview on screen; about 136 MiB resident. |
| CPU during a download | **Fail with 0.4.1, pass with 0.4.2.** Same 181-version download, preview table on screen, GUI process only: **317%** of one core with 0.4.1; 26–28% with redraws every 0.5 s; **14.4%** with the final 0.4.2 (1 redraw a second; the frame log shows 1 frame a second). The download also finished sooner (46 s with 0.4.1, about 20 s with 0.4.2), because the GUI had been starving the worker. |
| 150% scaling | Not tested on Linux; covered on Windows. |

## What was wrong, and the fixes (0.4.2)

- **Continuous redraws while an operation ran.**
  - **Measured with `RUST_MIRROR_GUI_FRAME_LOG`**, which records redraws
    per second and egui's repaint causes. During a download the window
    redrew 15–30 times a second, at roughly 70–130 ms of CPU per frame
    with software rendering. Two sources:
    - every line of worker output asked for a redraw;
    - the job details grid changed size with its text, and egui grids ask
      for another redraw whenever their size changes, so with new output
      on every frame this fed itself.
  - **Fix:**
    - the worker no longer wakes the window;
    - the window redraws on a 0.5 s timer while an operation runs (twice
      a second);
    - the details grid keeps a stable width, and the current-files list
      uses fixed columns;
    - the crate preview table and the activity log lay out only the rows
      in view.

    Result on Linux: 2 redraws a second, 26–28% CPU instead of 317%.
- **Large crate previews failed in the window.** The worker sends a
  preview's result as one line of about 4 KB per crate version: 1.2 MB for
  291 versions, and 1.7 MB for eframe's 426. The window refused lines over
  1 MiB ("event line exceeds 1 MiB"). The limit is now 64 MiB. A test with
  a 3 MiB line fails without the fix.
- **rust-mirror 0.5.1** (reported during this test). Previews of crates
  that crates.io lists more than once with different build metadata (such
  as `ndk-sys 0.4.0`, needed by eframe) failed with "duplicate index
  version". 0.5.1 accepts them when none is selected and reports a warning,
  which the crate preview now shows
  ([screenshot](../screenshots/crates-22-index-collision-warnings.png)).

## Limits

- Loopback RDP on the same machine, not a remote client over a network.
  The client is FreeRDP, not Microsoft's `mstsc`.
- Linux used xrdp with xorgxrdp only, with no other RDP servers or VNC.
- macOS was not tested.
