#!/usr/bin/env python3
"""Patch the nightly toolchain's rust-src so wasm32-unknown-unknown std gets Horizon web's
fs (in-memory VFS), time (performance.now/Date.now) and threads (Web Workers).
Idempotent. Usage: python3 web/std/apply.py [SYSROOT]   (default: rustc +nightly --print sysroot)"""
import os, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sysroot = sys.argv[1] if len(sys.argv) > 1 else subprocess.check_output(
    ["rustc", "+nightly", "--print", "sysroot"], text=True).strip()
STD = os.path.join(sysroot, "lib/rustlib/src/rust/library/std/src")
WEB = 'all(target_family = "wasm", target_os = "unknown")'
MARK = "// hzweb-patched"


def rd(p):
    return open(os.path.join(STD, p)).read()


def wr(p, s):
    open(os.path.join(STD, p), "w").write(s)


def copy(src, dst):
    wr(dst, open(os.path.join(HERE, src)).read())


def patch(path, old, new):
    s = rd(path)
    if MARK in s:
        return
    assert s.count(old) == 1, f"{path}: anchor not found once: {old!r}"
    wr(path, MARK + "\n" + s.replace(old, new))


copy("fs_hzweb.rs", "sys/fs/hzweb.rs")
copy("time_hzweb.rs", "sys/time/hzweb.rs")
copy("thread_hzweb.rs", "sys/thread/hzweb.rs")
copy("os_hzweb.rs", "os/hzweb.rs")
copy("args_hzweb.rs", "sys/args/hzweb.rs")
copy("env_hzweb.rs", "sys/env/hzweb.rs")
copy("paths_hzweb.rs", "sys/paths/hzweb.rs")
copy("stdio_hzweb.rs", "sys/stdio/hzweb.rs")

# fs: our module before the catch-all, plus the hooks std::os::hzweb calls.
patch("sys/fs/mod.rs", "    _ => {\n        mod unsupported;\n        use unsupported as imp;\n    }\n}",
      f"    {WEB} => {{\n        mod hzweb;\n        use hzweb as imp;\n        pub(crate) mod hzweb_hooks {{\n"
      "            pub use super::hzweb::{vfs_file_count, vfs_mkdirs, vfs_put, vfs_take_change};\n        }\n    }\n"
      "    _ => {\n        mod unsupported;\n        use unsupported as imp;\n    }\n}")

# time: same, before the catch-all.
patch("sys/time/mod.rs", "    _ => {\n        mod unsupported;\n        use unsupported as imp;\n    }",
      f"    {WEB} => {{\n        mod hzweb;\n        use hzweb as imp;\n    }}\n"
      "    _ => {\n        mod unsupported;\n        use unsupported as imp;\n    }")

# thread: our arm ahead of the generic wasm+atomics one.
patch("sys/thread/mod.rs", '    all(target_family = "wasm", target_feature = "atomics") => {',
      f'    all({WEB[4:-1]}, target_feature = "atomics") => {{\n'
      "        mod hzweb;\n"
      "        pub use hzweb::{DEFAULT_MIN_STACK_SIZE, Thread, available_parallelism, current_os_id, set_name, sleep, yield_now};\n"
      "        pub(crate) use hzweb::{is_main as hzweb_is_main, thread_start as hzweb_thread_start};\n"
      "    }\n"
      '    all(target_family = "wasm", target_feature = "atomics") => {')

# futex: the browser's main thread may not block; spin there instead.
patch("sys/sync/futex/wasm.rs",
      "pub fn futex_wait(futex: &Atomic<u32>, expected: u32, timeout: Option<Duration>) -> bool {\n",
      "pub fn futex_wait(futex: &Atomic<u32>, expected: u32, timeout: Option<Duration>) -> bool {\n"
      '    #[cfg(target_os = "unknown")]\n'
      "    if crate::sys::thread::hzweb_is_main() {\n"
      "        use crate::sync::atomic::Ordering;\n"
      "        let end = timeout.map(|t| crate::time::Instant::now() + t);\n"
      "        while futex.load(Ordering::Acquire) == expected {\n"
      "            if end.is_some_and(|e| crate::time::Instant::now() >= e) {\n"
      "                return false;\n"
      "            }\n"
      "            crate::hint::spin_loop();\n"
      "        }\n"
      "        return true;\n"
      "    }\n")

# args, env, paths.
patch("sys/args/mod.rs", "    _ => {\n        mod unsupported;\n        pub use unsupported::*;",
      f"    {WEB} => {{\n        mod hzweb;\n        pub use hzweb::{{Args, args}};\n        pub(crate) use hzweb::set as hzweb_set_args;\n    }}\n"
      "    _ => {\n        mod unsupported;\n        pub use unsupported::*;")
patch("sys/env/mod.rs", "    _ => {\n        mod unsupported;\n        pub use unsupported::*;",
      f"    {WEB} => {{\n        mod hzweb;\n        pub use hzweb::*;\n    }}\n"
      "    _ => {\n        mod unsupported;\n        pub use unsupported::*;")
patch("sys/paths/mod.rs", "    _ => {\n        mod unsupported;\n        use unsupported as imp;",
      f"    {WEB} => {{\n        mod hzweb;\n        use hzweb as imp;\n    }}\n"
      "    _ => {\n        mod unsupported;\n        use unsupported as imp;")

patch("sys/stdio/mod.rs", "    _ => {\n        mod unsupported;\n        pub use unsupported::*;",
      f"    {WEB} => {{\n        mod hzweb;\n        pub use hzweb::*;\n    }}\n"
      "    _ => {\n        mod unsupported;\n        pub use unsupported::*;")
# std::process::id(): one process.
patch("sys/process/unsupported.rs", 'pub fn getpid() -> u32 {\n    panic!("no pids on this platform")\n}',
      'pub fn getpid() -> u32 {\n    1\n}')

# threads wait on the futex directly.
patch("sys/sync/mod.rs", "mod futex;\n", "pub(crate) mod futex;\n")

# std::os::hzweb
patch("os/mod.rs", "pub mod raw;",
      "pub mod raw;\n\n"
      f"#[cfg({WEB})]\n"
      '#[unstable(feature = "hzweb", issue = "none")]\n'
      "pub mod hzweb;")

print("std patched for hzweb:", STD)
