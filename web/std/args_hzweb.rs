//! Horizon web: command-line arguments set by JS (`std::os::hzweb::set_args`).
use crate::ffi::OsString;
use crate::fmt;
use crate::sync::Mutex;

static ARGS: Mutex<Vec<OsString>> = Mutex::new(Vec::new());

pub fn set(args: Vec<String>) {
    *ARGS.lock().unwrap() = args.into_iter().map(OsString::from).collect();
}

pub struct Args {
    iter: crate::vec::IntoIter<OsString>,
}

pub fn args() -> Args {
    Args { iter: ARGS.lock().unwrap().clone().into_iter() }
}

impl fmt::Debug for Args {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter.as_slice()).finish()
    }
}
impl Iterator for Args {
    type Item = OsString;
    fn next(&mut self) -> Option<OsString> {
        self.iter.next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}
impl DoubleEndedIterator for Args {
    fn next_back(&mut self) -> Option<OsString> {
        self.iter.next_back()
    }
}
impl ExactSizeIterator for Args {
    fn len(&self) -> usize {
        self.iter.len()
    }
}
