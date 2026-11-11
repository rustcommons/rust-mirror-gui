# Trying the rust-mirror GUI prototype (Windows x86-64)

This is an **early prototype** for the owner's trial. It is not a release. It runs
the `rust-mirror` command-line program and shows what that program reports. It
does no mirroring itself.

> The GUI needs **rust-mirror 0.2.0 or newer**, the first version with the event
> protocol. If 0.2.0 is not published yet, you can still try the UI with the demo
> backend: see [UI-only trial](#ui-only-trial-without-rust-mirror-020).

## 1. Install rust-mirror 0.2.0

You need Rust 1.90 or newer and the Microsoft C++ build tools.

```powershell
cargo install rust-mirror --version 0.2.0 --locked
rust-mirror --version --json   # should mention the tool version and "protocol"
```

This installs the program to `%USERPROFILE%\.cargo\bin\rust-mirror.exe`. It does
not change any installed Rust toolchain.

## 2. Download and start the GUI

1. On GitHub, open **rustcommons/rust-mirror-gui → Actions → CI**. Pick the latest
   successful run on `main`.
2. Under **Artifacts**, download `rust-mirror-gui-windows-x86_64` and unzip it.
3. Run `rust-mirror-gui.exe`. The file is not signed, so Windows SmartScreen may
   warn. Choose **More info → Run anyway**.

Nothing is installed. The app remembers form values (never passwords) in
`%APPDATA%\rust-mirror-gui\settings.json`.

## 3. Connect it to rust-mirror

Open **rust-mirror program**. The default `rust-mirror` is looked up on PATH. If
the check fails, enter the full path, for example
`C:\Users\<you>\.cargo\bin\rust-mirror.exe`, and press **Check**. A green message
means the version and protocol are compatible. Otherwise the app says why, for
example "0.1.1 is too old".

## 4. A safe, small demo

This downloads a few hundred MB from `static.rust-lang.org` into a new folder. It
never touches your Rustup or Cargo installation or any Nexus server.

1. Create an empty folder, for example `C:\rm-demo`.
2. **Selections:** keep a single selection.
   - Toolchain `1.90.0`
   - Host **Windows x86-64 (MSVC)** only
   - Profile **minimal**
   - Save as `C:\rm-demo\mirror.toml`, then press **Save configuration**.
3. **Sync & finish:**
   - Configuration file `C:\rm-demo\mirror.toml`
   - Output folder `C:\rm-demo\run-001`. It must not exist yet.
   - Choose **New run**, then **Start sync**.

   Watch the stage, the files, the "last event" clock and the activity log.
4. **Try stopping:** during the download, press **Stop**. When it has stopped,
   choose **Resume an interrupted run** and start again. Files that were already
   verified are reused.
5. **Optional, finish the generation:**
   - Synced mirror `C:\rm-demo\run-001\mirror`
   - Rustup version `1.28.2`
   - Output folder `C:\rm-demo\complete-001`

   Then press **Start Rustup finish**.
6. **Skip Publish** unless you have a disposable test repository. Publishing
   writes to the server.
7. **Clean up:** delete `C:\rm-demo`.

## UI-only trial without rust-mirror 0.2.0

Download the artifact `demo-backend-windows-x86_64-not-a-mirror-tool` from the
same CI run. It contains `demo-backend-not-a-mirror-tool.exe`, a stand-in that
only plays back scripted events. It downloads nothing and writes no files. Enter
its path on the **rust-mirror program** page. Then any configuration path and a
non-existent output folder will play a fake sync.

To see how the GUI handles a backend that stops responding, start the GUI from
PowerShell like this:

```powershell
$env:FAKE_SCENARIO = 'stall'; .\rust-mirror-gui.exe
```

Other scenarios are `fail`, `crash` and `quiet-stage`.

## What feedback helps most

- Does the window open and respond over **Remote Desktop**, or on a VM without a GPU?
- Can **Narrator** or **NVDA** read and operate the fields, checkboxes and buttons?
- Did the app ever freeze, or ever show something other than what rust-mirror was doing?
- Which wording was unclear?
