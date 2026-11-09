//! Horizon web: a process environment kept in wasm memory (shared by all threads).
use crate::collections::BTreeMap;
use crate::ffi::{OsStr, OsString};
use crate::sync::Mutex;
use crate::{fmt, io};

static ENV: Mutex<BTreeMap<OsString, OsString>> = Mutex::new(BTreeMap::new());

pub struct Env {
    iter: crate::collections::btree_map::IntoIter<OsString, OsString>,
}
impl fmt::Debug for Env {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Env")
    }
}
impl Iterator for Env {
    type Item = (OsString, OsString);
    fn next(&mut self) -> Option<(OsString, OsString)> {
        self.iter.next()
    }
}
pub fn env() -> Env {
    Env { iter: ENV.lock().unwrap().clone().into_iter() }
}
pub fn getenv(k: &OsStr) -> Option<OsString> {
    ENV.lock().unwrap().get(k).cloned()
}
pub unsafe fn setenv(k: &OsStr, v: &OsStr) -> io::Result<()> {
    ENV.lock().unwrap().insert(k.to_owned(), v.to_owned());
    Ok(())
}
pub unsafe fn unsetenv(k: &OsStr) -> io::Result<()> {
    ENV.lock().unwrap().remove(k);
    Ok(())
}
