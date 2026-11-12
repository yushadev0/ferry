# Ferry Download Center

A lightweight, open-source download manager written in Rust, inspired by Internet Download Manager.

> **Status:** early development. Nothing is usable yet.

## Planned features

- Segmented downloads: files are split into parts that download in parallel, with dynamic re-splitting
- Pause and resume, including after restarting the app
- Download queues, a scheduler and speed limits
- A browser extension (Chrome, Edge, Firefox) that hands downloads over to Ferry
- A small desktop app built with [Tauri](https://tauri.app). Windows first, then Linux and macOS

## Repository layout

| Path | What it is |
|---|---|
| `crates/ferry-core` | Download engine (library, no UI) |
| `crates/ferry-host` | Native messaging host for the browser extension |
| `app/` | Desktop app (Tauri 2 + Svelte) |
| `extension/` | Browser extension (Manifest V3) |

## Building

```sh
cargo build --workspace
cargo test --workspace
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
