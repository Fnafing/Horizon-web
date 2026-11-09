//! Horizon web: threads are Web Workers on the shared wasm memory. `hz_spawn` asks JS for a
//! worker, which instantiates the module on this memory and calls
//! `std::os::hzweb::thread_start(ptr)` (exported by the game as `hzweb_thread_start`).
use crate::ffi::CStr;
use crate::io;
use crate::num::NonZero;
use crate::sync::Arc;
use crate::sync::atomic::{Atomic, AtomicU32, Ordering};
use crate::thread::ThreadInit;
use crate::time::Duration;

#[link(wasm_import_module = "hzweb")]
unsafe extern "C" {
    /// Starts a worker that will call `thread_start(arg)`. 0 on success.
    fn hz_spawn(arg: u32, stack: u32) -> i32;
    fn hz_cores() -> u32;
    /// 1 on the browser's main thread, where blocking waits are forbidden.
    fn hz_is_main() -> u32;
}

pub fn is_main() -> bool {
    unsafe { hz_is_main() != 0 }
}

pub const DEFAULT_MIN_STACK_SIZE: usize = 1024 * 1024;

struct Start {
    init: Box<ThreadInit>,
    done: Arc<AtomicU32>,
}

pub struct Thread {
    done: Arc<AtomicU32>,
}

impl Thread {
    pub unsafe fn new(stack: usize, init: Box<ThreadInit>) -> io::Result<Thread> {
        let done = Arc::new(AtomicU32::new(0));
        let start = Box::into_raw(Box::new(Start { init, done: done.clone() }));
        let ok = unsafe { hz_spawn(start as u32, stack.max(DEFAULT_MIN_STACK_SIZE) as u32) };
        if ok != 0 {
            drop(unsafe { Box::from_raw(start) });
            return Err(io::const_error!(io::ErrorKind::Other, "could not start a Web Worker"));
        }
        Ok(Thread { done })
    }

    pub fn join(self) {
        while self.done.load(Ordering::Acquire) == 0 {
            crate::sys::sync::futex::futex_wait(&self.done, 0, None);
        }
    }
}

/// Runs a spawned thread's body on the worker that JS started for it.
pub unsafe fn thread_start(arg: u32) {
    let start = unsafe { Box::from_raw(arg as *mut Start) };
    let Start { init, done } = *start;
    let rust_start = init.init();
    rust_start();
    done.store(1, Ordering::Release);
    crate::sys::sync::futex::futex_wake_all(&done);
}

pub fn available_parallelism() -> io::Result<NonZero<usize>> {
    NonZero::new(unsafe { hz_cores() } as usize).ok_or(io::Error::UNKNOWN_THREAD_COUNT)
}

pub fn current_os_id() -> Option<u64> {
    None
}

pub fn yield_now() {}

pub fn set_name(_name: &CStr) {}

pub fn sleep(dur: Duration) {
    if is_main() {
        let end = crate::time::Instant::now() + dur;
        while crate::time::Instant::now() < end {
            crate::hint::spin_loop();
        }
        return;
    }
    let mut nanos = dur.as_nanos();
    while nanos > 0 {
        let amt = crate::cmp::min(i64::MAX as u128, nanos);
        let x: Atomic<u32> = AtomicU32::new(0);
        unsafe {
            core::arch::wasm32::memory_atomic_wait32(x.as_ptr() as *mut i32, 0, amt as i64);
        }
        nanos -= amt;
    }
}
