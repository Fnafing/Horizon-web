//! Horizon in the browser. Built with the patched std of `web/std` (an in-memory filesystem,
//! the browser's clock, threads as Web Workers): `web/build.sh`. JS (`web/runtime`) fills
//! the filesystem from the player's .hzpack, sets the arguments and calls [`run`].
#![feature(hzweb)]

use wasm_bindgen::prelude::*;

/// Where the prepared resources (the .hzpack's `res/`) are mounted.
pub const RESOURCES: &str = "/horizon/resources";
/// The game's home: options, saves, logs (persisted to IndexedDB).
pub const HOME: &str = "/home/player/horizon";

/// Entry of a thread's worker (`web/runtime/hzweb_worker.js`).
#[wasm_bindgen]
pub fn hzweb_thread_start(arg: u32) {
    // SAFETY: called once per spawn with the value std handed to `hz_spawn`.
    unsafe { std::os::hzweb::thread_start(arg) }
}

/// Puts one file into the virtual filesystem. `record`: report it back as changed.
#[wasm_bindgen]
pub fn hz_put(path: &str, data: Vec<u8>, record: bool) {
    std::os::hzweb::put(std::path::Path::new(path), data, record);
}

/// Creates a directory (and parents) without reporting it as changed.
#[wasm_bindgen]
pub fn hz_mkdirs(path: &str) {
    std::os::hzweb::mkdirs(std::path::Path::new(path));
}

/// Number of files and directories in the virtual filesystem.
#[wasm_bindgen]
pub fn hz_file_count() -> usize {
    std::os::hzweb::file_count()
}

/// The next changed path: `[path, Uint8Array]` for a file (directories end in '/'),
/// `[path, null]` for a removed one, `undefined` when nothing changed.
#[wasm_bindgen]
pub fn hz_take_change() -> JsValue {
    match std::os::hzweb::take_change() {
        None => JsValue::UNDEFINED,
        Some((path, data)) => {
            let out = js_sys::Array::new();
            out.push(&JsValue::from_str(&path));
            out.push(&match data {
                Some(bytes) => js_sys::Uint8Array::from(bytes.as_slice()).into(),
                None => JsValue::NULL,
            });
            out.into()
        }
    }
}

/// Sets an environment variable for the game.
#[wasm_bindgen]
pub fn hz_set_env(key: &str, value: &str) {
    // SAFETY: called before the game starts its threads.
    unsafe { std::env::set_var(key, value) };
}

/// Starts the game with these arguments (the program name is added). Never returns
/// normally: winit's web event loop takes over through a JS exception.
#[wasm_bindgen]
pub fn hz_run(args: Vec<String>) {
    console_error_panic_hook::set_once();
    let mut all = vec!["horizon".to_string()];
    all.extend(args);
    std::os::hzweb::set_args(all);
    horizon_client::entry::main();
}
