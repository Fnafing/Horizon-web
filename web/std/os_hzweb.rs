//! Horizon web: the hooks JavaScript needs into std's virtual filesystem and threads.

use crate::path::Path;

/// Puts a file into the virtual filesystem (parents created), e.g. from the resource pack.
/// `record`: report it as changed (for saves restored from IndexedDB: false).
pub fn put(path: &Path, data: Vec<u8>, record: bool) {
    crate::sys::fs::hzweb_hooks::vfs_put(path, data, record)
}

/// Creates a directory and its parents without reporting a change.
pub fn mkdirs(path: &Path) {
    crate::sys::fs::hzweb_hooks::vfs_mkdirs(path)
}

/// The next changed path since the last call: `Some(bytes)` for a file (a directory ends
/// in '/' with empty bytes), `None` for a removed path.
pub fn take_change() -> Option<(String, Option<Vec<u8>>)> {
    crate::sys::fs::hzweb_hooks::vfs_take_change()
}

/// How many files and directories exist.
pub fn file_count() -> usize {
    crate::sys::fs::hzweb_hooks::vfs_file_count()
}

/// Entry point of a spawned thread's worker.
///
/// # Safety
/// `arg` must be the value JS received from `hz_spawn`, used once.
pub unsafe fn thread_start(arg: u32) {
    unsafe { crate::sys::thread::hzweb_thread_start(arg) }
}

/// Whether this is the browser's main thread.
pub fn is_main_thread() -> bool {
    crate::sys::thread::hzweb_is_main()
}

/// Sets what `std::env::args()` returns (the program name first).
pub fn set_args(args: Vec<String>) {
    crate::sys::args::hzweb_set_args(args)
}
