# Trying rust-mirror-gui on Windows x86-64

This guide is for trying release 0.1.0 on Windows. Windows runtime behavior was
not verified before release, so please report what you see.

The rust-mirror engine is compiled into this one program, so there is nothing else
to install. Each operation runs in a separate hidden copy of the program (a
"worker"). If an operation hangs or crashes, the window stays usable and can end
it.

The built-in engine is rust-mirror 0.4.0. You can also install the app with
`cargo install rust-mirror-gui --locked` (Rust 1.95+) instead of using the CI build.

## 1. Download and start

1. On GitHub, open **rustcommons/rust-mirror-gui → Actions → CI** and pick the
   latest successful run on `main`.
2. Under **Artifacts**, download `rust-mirror-gui-windows-x86_64` and unzip it.
3. Run `rust-mirror-gui.exe`. The file is not signed, so Windows SmartScreen may
   warn. Choose **More info → Run anyway**.

Nothing is installed and no Rust toolchain is needed. The app remembers form
values (never passwords) in `%APPDATA%\rust-mirror-gui\settings.json`.

The top bar should show **"✔ rust-mirror 0.4.0 built in · event protocol 1"**.
If it shows a red message instead, open **About the engine**. Security software
may be blocking the program from starting its own worker copy.

## 2. A safe, small demo

This downloads about 330 MB from `static.rust-lang.org` into a new folder. It
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
4. **Try stopping:** during the download, press **Stop**. When it says "Stopped",
   choose **Resume an interrupted run** and press **Start sync** again. Verified
   files are reused.
5. **Optional, finish the generation:**
   - Synced mirror `C:\rm-demo\run-001\mirror`
   - Rustup version `1.28.2`
   - Output folder `C:\rm-demo\complete-001`

   Then press **Start Rustup finish**.
6. **Skip Publish** unless you have a disposable test repository. Publishing
   writes to the server.
7. **Clean up:** delete `C:\rm-demo`.

Each page shows the equivalent `rust-mirror` command under **Same as**, for
scripting later with the command-line tool.

## What feedback helps most

- Does the window open and respond over **Remote Desktop**, or on a VM without a GPU?
- Can **Narrator** or **NVDA** read and operate the fields, checkboxes and buttons?
- Did the window ever freeze, or ever show something other than what was happening?
- Which wording was unclear?
