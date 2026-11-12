# Roadmap

Planned and deferred work for rust-mirror-gui. Nothing here is implemented or
promised for a specific release. Shipped behavior is described in the
[README](README.md) and [changelog](CHANGELOG.md).

## Deferred: qualification

- **No-GPU and Remote Desktop qualification** (deferred by the owner on
  2026-11-12). Check that the app opens, draws correctly, stays responsive and
  completes a sync on machines without a GPU, especially Windows Server over
  Remote Desktop. Includes a clear failure message where nothing can draw and
  tested workarounds. Waiting on a test environment decision.
  Plan: [docs/plans/NO-GPU-TEST-PLAN.md](docs/plans/NO-GPU-TEST-PLAN.md).
- **Screen readers.** Run NVDA, Narrator, VoiceOver and Orca against a form, a
  checkbox list and a progress display. Only the Linux accessibility tree has been
  inspected so far.
- **Offline start on Windows and macOS.** Confirmed on Linux only.

## Planned

- Team feedback from the 0.1.0 demo (to be collected).
