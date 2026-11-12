# No-GPU and Remote Desktop test plan

Status: **planned, deferred** (2026-11-12). Not started. See the [roadmap](../../ROADMAP.md).

## Question

Does rust-mirror-gui open, draw correctly, stay responsive and complete a sync
on machines without a GPU, especially Windows Server over Remote Desktop? Where
it cannot, is the error clear and is there a workaround?

## Known before testing

- The app renders with wgpu through eframe 0.37. wgpu tries Vulkan, DirectX 12,
  Metal and OpenGL. It does not force a software (CPU) adapter on its own.
- Windows includes a CPU DirectX 12 renderer (WARP). Untested with this app.
- The backend can be forced without rebuilding: `WGPU_BACKEND=dx12|vulkan|opengl`.
- Linux without a GPU works with Mesa lavapipe (software Vulkan). With no driver
  at all, the window fails with a wgpu surface error
  ([0.1.0 validation](../evals/2026-11-11-v0.1.0-validation.md)).

## Environments

| ID | Environment | Method | Needs |
|---|---|---|---|
| W1 | Windows Server 2022 and 2025, no GPU | GitHub-hosted Windows runners | Automated, free |
| W2 | Windows Server 2025 over real RDP, no GPU | Hyper-V VM (evaluation edition) on the owner's PC, or a team test server, or a cloud VM | Owner decision on where |
| W3 | Windows Server 2019 over RDP | As W2 | Only if admins use 2019 or older |
| L1 | Linux, no GPU, X11; lavapipe and llvmpipe separately | Cloud sandbox | Automated |
| L2 | Linux, no GPU, Wayland | Cloud sandbox | |
| L3 | Linux over xrdp | Ubuntu 24.04 | |
| L4 | Linux with no graphics drivers | Cloud sandbox | Failure message only |
| M1 | macOS VMs without a dedicated GPU, Apple Silicon and Intel | GitHub-hosted macOS runners | Launch and screenshot |

## Steps in each environment

1. Launch; the window opens within 10 s.
2. Record the adapter used, without code changes. Windows: whether `d3d10warp.dll`
   or a hardware driver is loaded, plus the display adapter name. Linux: Mesa and
   Vulkan loader debug output.
3. Window-only screenshots of every screen, compared with references.
4. A small real sync (one pinned toolchain, one host, about 325 MB, new output
   folder): the last-event counter updates every second, Stop works, Resume
   completes, Rustup finish works.
5. Resource use on 2 vCPUs: idle under 3% of one core, under 25% during a sync,
   memory under 300 MB, input response under 200 ms.
6. Resize, maximize, minimize and restore at 100%, 150% and 200% scaling.
7. Remote Desktop (W2, W3, L3): launch in session; disconnect and reconnect during
   a sync; change resolution on reconnect; minimize the client during a sync;
   multi-monitor; lock and unlock; console start then RDP reconnect.
8. No usable renderer (L4, or W2 with DirectX 12 disabled): the app exits with a
   clear message, not a panic or a silent exit.
9. On any failure, retry with `WGPU_BACKEND=dx12`, then `WGPU_BACKEND=opengl`.

## Outcomes

- **Supported:** W2 passes steps 1 to 8 with defaults.
- **Supported with workaround:** passes only with `WGPU_BACKEND`; document it and
  plan an automatic fallback.
- **Not supported:** record as a known limit and decide a fix with the owner.

Possible fixes (not part of this test): retry with wgpu's software fallback
adapter when no GPU adapter is found (no new crates); a clear error dialog; an
OpenGL renderer through eframe's `glow` feature (adds `egui_glow`, needs owner
approval).

## Deliverables

- `docs/evals/<date>-no-gpu.md`: matrix, pass/fail, adapter, timings, resource
  use, screenshots and reproduction steps.
- Updated known limits in the README.
- A CI job covering W1, L1 and M1 on every release.

Never use the owner's Rustup or Cargo homes or a real Nexus server during this test.
