# Horizon Web (WIP)

<img width="1320" height="685" alt="4mrmxgaz" src="https://github.com/user-attachments/assets/9c784b9e-5c06-4158-b538-58eda954ff59" />

Minecraft Java 26.3 in the browser, built on [Horizon](https://github.com/horizon-rust/horizon) (MIT).
Unofficial. No Minecraft code or assets here: players make their own `.hzpack` with mcpack from their own copy.

- `horizon-patches/` - the changes on top of horizon-rust/horizon (base commit 595421d). Apply: `git am horizon-patches/*.patch`
- `web/` - patched std (in-memory fs, clock, Web Worker threads), JS runtime, launcher page, build script
- `crates/horizon-web/` - wasm entry crate
- `mcpack/` - Windows tool: makes the .hzpack from a 26.3 folder + client jar, and serves the web build (Launch Web)

Build: nightly Rust + rust-src, `wasm-bindgen-cli 0.2.129`, then `web/build.sh release` -> `web/dist`.

Known issue: crashes when more than 64 threads are needed ("no idle thread among 64").
No sound, controllers, multiplayer or shader packs yet. WebGL2 only.
