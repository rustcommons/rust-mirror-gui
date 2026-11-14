# Roadmap

Planned and deferred work for rust-mirror-gui. Nothing here is implemented or
promised for a specific release. Shipped behavior is described in the
[README](../README.md) and [changelog](../CHANGELOG.md).

## Deferred: qualification

- **No-GPU and Remote Desktop qualification** (deferred by the owner on
  2026-11-12). Check that the app opens, draws correctly, stays responsive and
  completes a sync on machines without a GPU, especially Windows Server over
  Remote Desktop. Includes a clear failure message where nothing can draw and
  tested workarounds. Waiting on a test environment decision.
  Plan: [plans/NO-GPU-TEST-PLAN.md](plans/NO-GPU-TEST-PLAN.md). First partial
  data point (2026-11-14): on a GitHub-hosted Windows Server 2025 runner with no
  GPU, 0.4.0 opens and draws with Direct3D 12 on WARP; see the
  [Windows qualification](evals/2026-11-14-windows-qualification.md).
- **Screen readers.** Run NVDA, Narrator, VoiceOver and Orca against a form, a
  checkbox list and a progress display. Only the Linux accessibility tree has been
  inspected so far.
- **Offline start on Windows and macOS.** Confirmed on Linux only.

## Planned

- **Use system network settings by default** (raised by the owner on
  2026-11-12; not yet scheduled). Today (0.2.0) the app connects directly
  unless you choose a proxy and add a company certificate yourself. The goal
  is for it to just work on a managed company machine:
  - **Trust the operating system's certificate store**, so company
    certificates installed by IT are trusted without a PEM file. Likely via
    `rustls-platform-verifier` in rust-mirror; this is a new dependency and
    **needs owner approval**.
  - **Use the operating system's proxy settings**, including automatic
    configuration (PAC/WPAD). On Windows this means WinHTTP's proxy
    resolution.
  - **Later step:** NTLM/Kerberos proxy sign-in (Windows integrated
    authentication). 0.2.0 supports only Basic.
  - **Manual settings stay** as an override for machines where the system
    settings are wrong or missing.

  Most of this belongs in rust-mirror's network layer, with the GUI exposing
  the choice. Not designed or approved yet.
- Team feedback from the 0.1.0 demo (to be collected).

## Possible future additions

Ideas recorded for later. Not planned, designed or approved.

- **Scheduled mirror updates** (considered and set aside by the owner on
  2026-11-12: rust-mirror-gui is a simple desktop app and does not need a
  scheduler). The idea: a single update run (sync, Rustup finish, retention,
  optional publish) that the operating system's scheduler could start, such as
  Windows Task Scheduler, a systemd timer or cron. If ever wanted, it belongs in
  the rust-mirror command line rather than in the GUI.
