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

WINDOWS_SUMMARY

## Windows (step 1)

WINDOWS_SECTION

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
| Disconnect mid-sync, reconnect | **Pass.** With the client ended, the session's X server and the app (GUI and worker) kept running. After reconnecting, the window drew, and the sync finished: 181 files ([screenshot](../screenshots/rdp-linux-xrdp-finished-after-reconnect.png)). |
| Reconnect at another resolution | **Pass.** 1280×800 changed to 1440×900 (FreeRDP limits the requested 1600×900 to its own 1440×900 display); the window drew ([screenshot](../screenshots/rdp-linux-xrdp-after-reconnect.png)). |
| Minimize the RDP client mid-sync | **Pass.** The sync finished (181 files) and the window drew after restoring. The window kept using about 24% while minimized: the server session keeps running, so the app cannot tell. |
| Idle CPU and memory | **Pass.** 0% CPU over 30 s with a preview on screen; about 136 MiB resident. |
| CPU during a download | **Fail with 0.4.1, pass with 0.4.2.** Same download, preview on screen: GUI 317% of one core with 0.4.1, 26–28% with 0.4.2. The download also finished sooner (46 s, then 26 s), because the GUI had been starving the worker. |
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
