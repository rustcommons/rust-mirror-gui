# Roadmap

Planned and deferred work for rust-mirror-gui. Nothing here is implemented or
promised for a specific release. Shipped behavior is described in the
[README](../README.md) and [changelog](../CHANGELOG.md).

## Standard release pipeline

rust-mirror-gui adopted the Rust Commons
[standard release pipeline](https://github.com/rustcommons/.github/blob/main/docs/TRUSTED-PUBLISHING.md)
on 2026-11-21 (PR #32, following rust-mirror#75). See the
[release steps](RELEASING.md#steps).

**Phase 2 is done (2026-11-21, following rust-mirror#76).** `build` now
calls the reusable `build-binaries.yml`, which:
- builds the four binaries with every qualification gate (the AlmaLinux 8
  build, the old-Linux launches without fonts, the Windows capture and the
  static C runtime);
- packages, audits and installs the crate;
- writes one SBOM and one `SHA256SUMS`;
- signs, on the tag only, in its nested "Checksums and attestations" job.

`github-release` creates the release as a draft with all eight assets, then
publishes it.

`release-binaries.yml` is retired, so the late-approval timeout of 0.5.2 is
gone: nothing waits for a hand-made release. The verifiers keep 0.4.3 to
0.5.2 in their original layouts.

**First complete tag run passed with 0.5.3 (2026-11-22).**
[Trusted publishing run 45037116262](https://github.com/rustcommons/rust-mirror-gui/actions/runs/45037116262)
succeeded on attempt 1: signing before owner approval, crates.io publication,
automatic draft creation and publication with eight assets, and Windows/Linux
public verification. The tag followed successful CI on main. See the
[publication record](evals/2026-11-22-v0.5.3-publication.md) for each job,
timings, signer evidence, registry equality and the macOS capacity notice.

Deferred, not lost:

- **Release-PR audit overlap.** Every dry run now repeats the package audit,
  the packaged install and all qualification gates. The manual release-PR
  checks remain for now. 0.5.3 proves the full pipeline; deciding which
  manual checks can cite the dry run is a follow-up.
- **Window checks after publication.** `verify` runs the downloaded worker.
  The window-drawing checks on the published downloads stay in
  `verify-release-download.yml`, run by hand after publication. Folding them
  into `verify` would add the old-distribution containers to every tag run.
- **Already-published versions.** 0.5.2 and earlier keep their hand-created
  releases and assets. None is re-tagged or re-released.

## Deferred: qualification

- **No-GPU and Remote Desktop qualification** (deferred by the owner on
  2026-11-12; Remote Desktop steps 1 and 2 done on 2026-11-15, see the
  [Remote Desktop record](evals/2026-11-15-remote-desktop.md): Windows
  Server 2022 and 2025 over loopback RDP and Linux with xrdp pass, with
  CPU use on some machines as an open limit). Check that the app opens, draws correctly, stays responsive and
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

- **Group crate metadata warnings — done (2026-11-22; unreleased).**
  The preview prepares one summary at report receipt, counting distinct
  versions/crates and taking the union of typed field names. Details are
  grouped by crate, closed by default, and capped at 200 heading/version
  rows with the remaining version count. Single warnings remain a plain
  line. Owner-approved implementation only; no version bump or release.

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
- **Slower redraws when frames are slow** (small follow-up, decided by the
  owner on 2026-11-15; not yet scheduled). Without a GPU, Windows draws in
  software, and on some processors one frame costs 0.6–0.9 s of CPU (seen on
  AMD EPYC 7763 runners), so even at one redraw a second the window uses
  60–90% of a core while an operation runs. The app would time its frames
  and redraw less often while a job runs when frames are that slow, while
  input still redraws at once. 0.4.2 ships this as a documented limit; see
  the [Remote Desktop record](evals/2026-11-15-remote-desktop.md).
- Team feedback from the 0.1.0 demo (to be collected).

## Possible future additions

Ideas recorded for later. Not planned, designed or approved.

- **Scheduled mirror updates** (considered and set aside by the owner on
  2026-11-12: rust-mirror-gui is a simple desktop app and does not need a
  scheduler). The idea: a single update run (sync, Rustup finish, retention,
  optional publish) that the operating system's scheduler could start, such as
  Windows Task Scheduler, a systemd timer or cron. If ever wanted, it belongs in
  the rust-mirror command line rather than in the GUI.
