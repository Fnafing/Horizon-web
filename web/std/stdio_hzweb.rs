//! Horizon web: stdout/stderr go to the browser console (`hz_write`, line-buffered in JS).
use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut};

#[link(wasm_import_module = "hzweb")]
unsafe extern "C" {
    fn hz_write(fd: u32, ptr: *const u8, len: usize);
}

pub struct Stdin;
pub struct Stdout;
pub struct Stderr;

impl Stdin {
    pub const fn new() -> Stdin {
        Stdin
    }
}
impl io::Read for Stdin {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Ok(0)
    }
    fn read_buf(&mut self, _cursor: BorrowedCursor<'_, u8>) -> io::Result<()> {
        Ok(())
    }
    fn read_vectored(&mut self, _bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        Ok(0)
    }
}

fn write(fd: u32, buf: &[u8]) -> io::Result<usize> {
    if !buf.is_empty() {
        unsafe { hz_write(fd, buf.as_ptr(), buf.len()) };
    }
    Ok(buf.len())
}

impl Stdout {
    pub const fn new() -> Stdout {
        Stdout
    }
}
impl io::Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        write(1, buf)
    }
    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        io::default_write_vectored(|b| write(1, b), bufs)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Stderr {
    pub const fn new() -> Stderr {
        Stderr
    }
}
impl io::Write for Stderr {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        write(2, buf)
    }
    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        io::default_write_vectored(|b| write(2, b), bufs)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub const STDIN_BUF_SIZE: usize = 0;

pub fn is_ebadf(_err: &io::Error) -> bool {
    false
}

pub fn panic_output() -> Option<impl io::Write> {
    Some(Stderr::new())
}
