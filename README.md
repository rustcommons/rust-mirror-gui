# rust-mirror-gui

A desktop interface for [rust-mirror](https://github.com/rustcommons/rust-mirror):
choose Rust toolchains, build offline mirrors and publish them to an internal
server, without memorizing CLI commands.

**Status: prototype, not released.** The egui app runs the `rust-mirror` program
as a child process and follows its JSON-lines event protocol. That protocol
arrives in rust-mirror 0.2.0, which is not yet verified against this app. Until
then the app is tested only against a scripted fake backend. Use the
[rust-mirror CLI](https://crates.io/crates/rust-mirror) for real work today.

Implemented in the prototype:

- Backend discovery: refuses incompatible rust-mirror versions and protocols.
- Selections editor: writes the configuration file.
- Sync with resume or start-over, and the Rustup finishing step.
- Publish form: credentials are passed only to the child process.
- Live stage, files, bytes and activity log; a "last event" clock and stall warning.
- Stop, then end the process if needed.

Planned: plan preview and generations/retention pages.

- Try it on Windows: [TRYING.md](TRYING.md)
- Design, decisions and architecture: [docs/DESIGN.md](docs/DESIGN.md)

## Build from source

Requires Rust 1.95 or newer.

```sh
cargo build --release      # target/release/rust-mirror-gui
cargo test                 # drives the process client against the fake backend
```

On Linux the app needs a working Vulkan or OpenGL driver. On a machine without a
GPU, Mesa's software drivers work (Debian: `mesa-vulkan-drivers libegl1`).

## License

Apache-2.0. See [LICENSE](LICENSE).
