# rust-mirror-gui

A desktop interface for [rust-mirror](https://github.com/rustcommons/rust-mirror):
choose Rust toolchains, build offline mirrors and publish them to an internal
server, without memorizing CLI commands.

**Status: prototype, not released.** One executable with the rust-mirror library
compiled in. No separate rust-mirror install is needed. Every operation runs in a
hidden worker copy of the same program and reports to the window over the
JSON-lines event protocol, so a hung operation can always be stopped or ended.
The prototype embeds rust-mirror 0.1.1 through a temporary bridge until
rust-mirror 0.2.0 (event API) is published.

Implemented in the prototype:

- Selections editor that writes the configuration file.
- Sync (new, resume or start over) and the Rustup finishing step.
- Publish form: credentials are passed only to the worker process.
- Live stage, files, bytes and activity log; a "last event" clock and stall warning.
- Stop, then end the worker if needed.

Planned: plan preview and generations/retention pages.

- Try it on Windows: [TRYING.md](TRYING.md)
- Design, decisions and architecture: [docs/DESIGN.md](docs/DESIGN.md)

## Build from source

Requires Rust 1.95 or newer.

```sh
cargo build --release      # target/release/rust-mirror-gui
cargo test                 # process client vs. fake backend, plus the real worker
```

On Linux the app needs a working Vulkan or OpenGL driver. On a machine without a
GPU, Mesa's software drivers work (Debian: `mesa-vulkan-drivers libegl1`).

## License

Apache-2.0. See [LICENSE](LICENSE).
