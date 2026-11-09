//! Horizon web: the virtual filesystem's paths. The working directory is changeable.
use crate::ffi::{OsStr, OsString};
use crate::path::{self, Path, PathBuf};
use crate::sync::Mutex;
use crate::{fmt, io};

static CWD: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn getcwd() -> io::Result<PathBuf> {
    Ok(CWD.lock().unwrap().clone().unwrap_or_else(|| PathBuf::from("/")))
}
pub fn chdir(p: &path::Path) -> io::Result<()> {
    let to = if p.is_absolute() { p.to_path_buf() } else { getcwd()?.join(p) };
    if !crate::sys::fs::exists(&to)? {
        return Err(io::const_error!(io::ErrorKind::NotFound, "No such file or directory"));
    }
    *CWD.lock().unwrap() = Some(to);
    Ok(())
}

pub struct SplitPaths<'a> {
    iter: crate::str::Split<'a, char>,
}
pub struct SplitPathsRef<'a> {
    iter: crate::str::Split<'a, char>,
}
pub fn split_paths(unparsed: &OsStr) -> SplitPaths<'_> {
    SplitPaths { iter: unparsed.to_str().unwrap_or("").split(':') }
}
pub fn split_paths_ref(unparsed: &OsStr) -> Option<SplitPathsRef<'_>> {
    Some(SplitPathsRef { iter: unparsed.to_str()?.split(':') })
}
impl<'a> Iterator for SplitPaths<'a> {
    type Item = PathBuf;
    fn next(&mut self) -> Option<PathBuf> {
        self.iter.next().map(PathBuf::from)
    }
}
impl<'a> Iterator for SplitPathsRef<'a> {
    type Item = &'a Path;
    fn next(&mut self) -> Option<&'a Path> {
        self.iter.next().map(Path::new)
    }
}

#[derive(Debug)]
pub struct JoinPathsError;
pub fn join_paths<I, T>(paths: I) -> Result<OsString, JoinPathsError>
where
    I: Iterator<Item = T>,
    T: AsRef<OsStr>,
{
    let mut out = OsString::new();
    for (i, p) in paths.enumerate() {
        let p = p.as_ref();
        if p.to_str().is_some_and(|s| s.contains(':')) {
            return Err(JoinPathsError);
        }
        if i > 0 {
            out.push(":");
        }
        out.push(p);
    }
    Ok(out)
}
impl fmt::Display for JoinPathsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        "path segment contains separator `:`".fmt(f)
    }
}
impl crate::error::Error for JoinPathsError {}

pub fn current_exe() -> io::Result<PathBuf> {
    Ok(PathBuf::from("/horizon/horizon"))
}
pub fn temp_dir() -> PathBuf {
    let _ = crate::sys::fs::DirBuilder::new().mkdir(Path::new("/tmp"));
    PathBuf::from("/tmp")
}
pub fn home_dir() -> Option<PathBuf> {
    Some(PathBuf::from("/home/player"))
}
