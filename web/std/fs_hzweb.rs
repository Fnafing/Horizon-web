//! Horizon web: std::fs over an in-memory filesystem that lives in wasm memory, so every
//! thread (Web Worker on the shared memory) sees the same files. JavaScript fills it at
//! start (`std::os::hzweb::put`) and drains changed paths into IndexedDB
//! (`std::os::hzweb::take_change`). Paths are absolute, '/'-separated; relative paths are
//! taken from the virtual current directory `/`.

use crate::collections::{BTreeMap, BTreeSet};
use crate::ffi::OsString;
use crate::fmt;
use crate::fs::TryLockError;
use crate::hash::{Hash, Hasher};
use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut, SeekFrom};
use crate::path::{Component, Path, PathBuf};
use crate::sync::{Arc, Mutex, RwLock};
pub use crate::sys::fs::common::{Dir, ExtraHomeDirs, ExtraMediaDirs};
use crate::sys::time::SystemTime;


type Data = Arc<RwLock<Vec<u8>>>;

#[derive(Clone)]
enum Node {
    Dir { mtime: SystemTime },
    File { data: Data, mtime: SystemTime },
}

struct Vfs {
    nodes: BTreeMap<String, Node>,
    /// Paths changed since JS last drained them (written, created or removed).
    changed: BTreeSet<String>,
}

static VFS: Mutex<Vfs> = Mutex::new(Vfs { nodes: BTreeMap::new(), changed: BTreeSet::new() });

fn now() -> SystemTime {
    SystemTime::now()
}

/// `/a/./b/../c` and `a/c` -> `/a/c`. The root is `/`.
fn norm(p: &Path) -> String {
    let mut parts: crate::vec::Vec<String> = crate::vec::Vec::new();
    for c in p.components() {
        match c {
            Component::RootDir | Component::Prefix(_) | Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
        }
    }
    let mut out = String::from("/");
    out.push_str(&parts.join("/"));
    out
}

fn parent(path: &str) -> Option<&str> {
    if path == "/" {
        return None;
    }
    match path.rfind('/') {
        Some(0) => Some("/"),
        Some(i) => Some(&path[..i]),
        None => None,
    }
}

fn not_found() -> io::Error {
    io::const_error!(io::ErrorKind::NotFound, "No such file or directory")
}
fn exists_err() -> io::Error {
    io::const_error!(io::ErrorKind::AlreadyExists, "File exists")
}
fn is_dir_err() -> io::Error {
    io::const_error!(io::ErrorKind::IsADirectory, "Is a directory")
}
fn not_dir_err() -> io::Error {
    io::const_error!(io::ErrorKind::NotADirectory, "Not a directory")
}

impl Vfs {
    fn mkdirs(&mut self, path: &str) {
        let mut cur = Some(path);
        let mut todo = crate::vec::Vec::new();
        while let Some(p) = cur {
            if self.nodes.contains_key(p) {
                break;
            }
            todo.push(p.to_string());
            cur = parent(p);
        }
        for p in todo.into_iter().rev() {
            self.nodes.insert(p.clone(), Node::Dir { mtime: now() });
            self.changed.insert(p);
        }
    }
    fn parent_is_dir(&self, path: &str) -> io::Result<()> {
        match parent(path) {
            None => Ok(()),
            Some("/") => Ok(()),
            Some(p) => match self.nodes.get(p) {
                Some(Node::Dir { .. }) => Ok(()),
                Some(Node::File { .. }) => Err(not_dir_err()),
                None => Err(not_found()),
            },
        }
    }
    fn children(&self, dir: &str) -> crate::vec::Vec<String> {
        let prefix = if dir == "/" { String::from("/") } else { format!("{dir}/") };
        self.nodes
            .range(prefix.clone()..)
            .take_while(|(k, _)| k.starts_with(&prefix))
            .filter(|(k, _)| !k[prefix.len()..].contains('/') && k.len() > prefix.len())
            .map(|(k, _)| k.clone())
            .collect()
    }
}

// ---- hooks for std::os::hzweb ----

/// Adds or replaces a file (parents are created); not reported back as a change.
pub fn vfs_put(path: &Path, data: Vec<u8>, record: bool) {
    let p = norm(path);
    let mut v = VFS.lock().unwrap();
    if let Some(par) = parent(&p) {
        let par = par.to_string();
        let before = v.changed.clone();
        v.mkdirs(&par);
        if !record {
            v.changed = before;
        }
    }
    v.nodes.insert(p.clone(), Node::File { data: Arc::new(RwLock::new(data)), mtime: now() });
    if record {
        v.changed.insert(p);
    }
}

pub fn vfs_mkdirs(path: &Path) {
    let p = norm(path);
    let mut v = VFS.lock().unwrap();
    let before = v.changed.clone();
    v.mkdirs(&p);
    v.changed = before;
}

/// One changed path: `(path, Some(bytes))` for a file, `(path, None)` for a removed path,
/// and directories as `(path/, None)`... directories are reported with a trailing '/'
/// and empty contents. `None` when nothing changed.
pub fn vfs_take_change() -> Option<(String, Option<Vec<u8>>)> {
    let mut v = VFS.lock().unwrap();
    let p = v.changed.pop_first()?;
    Some(match v.nodes.get(&p) {
        Some(Node::File { data, .. }) => (p, Some(data.read().unwrap().clone())),
        Some(Node::Dir { .. }) => (format!("{p}/"), Some(Vec::new())),
        None => (p, None),
    })
}

pub fn vfs_file_count() -> usize {
    VFS.lock().unwrap().nodes.len()
}

// ---- std types ----

pub struct File {
    path: String,
    data: Data,
    pos: Mutex<u64>,
    append: bool,
    write: bool,
    read: bool,
}

#[derive(Clone)]
pub struct FileAttr {
    size: u64,
    kind: FileType,
    mtime: SystemTime,
}

pub struct ReadDir {
    root: PathBuf,
    entries: crate::vec::IntoIter<String>,
}

pub struct DirEntry {
    path: String,
}

#[derive(Clone, Debug)]
pub struct OpenOptions {
    read: bool,
    write: bool,
    append: bool,
    truncate: bool,
    create: bool,
    create_new: bool,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct FileTimes {
    modified: Option<SystemTime>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FilePermissions {
    readonly: bool,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct FileType {
    dir: bool,
}

#[derive(Debug)]
pub struct DirBuilder {}

impl FileAttr {
    pub fn size(&self) -> u64 {
        self.size
    }
    pub fn perm(&self) -> FilePermissions {
        FilePermissions { readonly: false }
    }
    pub fn file_type(&self) -> FileType {
        self.kind
    }
    pub fn modified(&self) -> io::Result<SystemTime> {
        Ok(self.mtime)
    }
    pub fn accessed(&self) -> io::Result<SystemTime> {
        Ok(self.mtime)
    }
    pub fn created(&self) -> io::Result<SystemTime> {
        Ok(self.mtime)
    }
}

impl FilePermissions {
    pub fn readonly(&self) -> bool {
        self.readonly
    }
    pub fn set_readonly(&mut self, readonly: bool) {
        self.readonly = readonly
    }
}

impl FileTimes {
    pub fn set_accessed(&mut self, _t: SystemTime) {}
    pub fn set_modified(&mut self, t: SystemTime) {
        self.modified = Some(t)
    }
}

impl FileType {
    pub fn is_dir(&self) -> bool {
        self.dir
    }
    pub fn is_file(&self) -> bool {
        !self.dir
    }
    pub fn is_symlink(&self) -> bool {
        false
    }
}

impl ReadDir {
    pub fn dir(&self) -> io::Result<Dir> {
        Err(io::Error::UNSUPPORTED_PLATFORM)
    }
}
impl fmt::Debug for ReadDir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadDir").field("root", &self.root).finish()
    }
}
impl Iterator for ReadDir {
    type Item = io::Result<DirEntry>;
    fn next(&mut self) -> Option<io::Result<DirEntry>> {
        self.entries.next().map(|path| Ok(DirEntry { path }))
    }
}

impl DirEntry {
    pub fn path(&self) -> PathBuf {
        PathBuf::from(&self.path)
    }
    pub fn file_name(&self) -> OsString {
        OsString::from(self.path.rsplit('/').next().unwrap_or(""))
    }
    pub fn metadata(&self) -> io::Result<FileAttr> {
        stat(Path::new(&self.path))
    }
    pub fn file_type(&self) -> io::Result<FileType> {
        Ok(self.metadata()?.kind)
    }
}

impl OpenOptions {
    pub fn new() -> OpenOptions {
        OpenOptions { read: false, write: false, append: false, truncate: false, create: false, create_new: false }
    }
    pub fn read(&mut self, v: bool) {
        self.read = v
    }
    pub fn write(&mut self, v: bool) {
        self.write = v
    }
    pub fn append(&mut self, v: bool) {
        self.append = v
    }
    pub fn truncate(&mut self, v: bool) {
        self.truncate = v
    }
    pub fn create(&mut self, v: bool) {
        self.create = v
    }
    pub fn create_new(&mut self, v: bool) {
        self.create_new = v
    }
}

impl File {
    pub fn open(path: &Path, opts: &OpenOptions) -> io::Result<File> {
        let p = norm(path);
        let writes = opts.write || opts.append;
        let mut v = VFS.lock().unwrap();
        let data = match v.nodes.get(&p) {
            Some(Node::Dir { .. }) => {
                if writes {
                    return Err(is_dir_err());
                }
                // Reading a directory as a file: std allows opening it on unix.
                Arc::new(RwLock::new(Vec::new()))
            }
            Some(Node::File { data, .. }) => {
                let data = data.clone();
                if opts.create_new {
                    return Err(exists_err());
                }
                if opts.truncate && writes {
                    data.write().unwrap().clear();
                    v.changed.insert(p.clone());
                }
                data
            }
            None => {
                if !(opts.create || opts.create_new) || !writes {
                    return Err(not_found());
                }
                v.parent_is_dir(&p)?;
                let data: Data = Arc::new(RwLock::new(Vec::new()));
                v.nodes.insert(p.clone(), Node::File { data: data.clone(), mtime: now() });
                v.changed.insert(p.clone());
                data
            }
        };
        Ok(File {
            path: p,
            data,
            pos: Mutex::new(0),
            append: opts.append,
            write: writes,
            read: opts.read || !writes,
        })
    }

    fn touched(&self) {
        let mut v = VFS.lock().unwrap();
        if let Some(Node::File { mtime, .. }) = v.nodes.get_mut(&self.path) {
            *mtime = now();
        }
        v.changed.insert(self.path.clone());
    }

    pub fn file_attr(&self) -> io::Result<FileAttr> {
        let size = self.data.read().unwrap().len() as u64;
        let mtime = match VFS.lock().unwrap().nodes.get(&self.path) {
            Some(Node::File { mtime, .. }) | Some(Node::Dir { mtime }) => *mtime,
            None => now(),
        };
        Ok(FileAttr { size, kind: FileType { dir: false }, mtime })
    }
    pub fn fsync(&self) -> io::Result<()> {
        Ok(())
    }
    pub fn datasync(&self) -> io::Result<()> {
        Ok(())
    }
    pub fn lock(&self) -> io::Result<()> {
        Ok(())
    }
    pub fn lock_shared(&self) -> io::Result<()> {
        Ok(())
    }
    pub fn try_lock(&self) -> Result<(), TryLockError> {
        Ok(())
    }
    pub fn try_lock_shared(&self) -> Result<(), TryLockError> {
        Ok(())
    }
    pub fn unlock(&self) -> io::Result<()> {
        Ok(())
    }
    pub fn truncate(&self, size: u64) -> io::Result<()> {
        if !self.write {
            return Err(io::const_error!(io::ErrorKind::PermissionDenied, "not opened for writing"));
        }
        self.data.write().unwrap().resize(size as usize, 0);
        self.touched();
        Ok(())
    }
    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.read {
            return Err(io::const_error!(io::ErrorKind::PermissionDenied, "not opened for reading"));
        }
        let data = self.data.read().unwrap();
        let mut pos = self.pos.lock().unwrap();
        let start = (*pos as usize).min(data.len());
        let n = buf.len().min(data.len() - start);
        buf[..n].copy_from_slice(&data[start..start + n]);
        *pos += n as u64;
        Ok(n)
    }
    pub fn read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        crate::io::default_read_vectored(|b| self.read(b), bufs)
    }
    pub fn is_read_vectored(&self) -> bool {
        false
    }
    pub fn read_buf(&self, cursor: BorrowedCursor<'_, u8>) -> io::Result<()> {
        crate::io::default_read_buf(|b| self.read(b), cursor)
    }
    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        if !self.write {
            return Err(io::const_error!(io::ErrorKind::PermissionDenied, "not opened for writing"));
        }
        {
            let mut data = self.data.write().unwrap();
            let mut pos = self.pos.lock().unwrap();
            if self.append {
                *pos = data.len() as u64;
            }
            let start = *pos as usize;
            if data.len() < start {
                data.resize(start, 0);
            }
            let end = start + buf.len();
            if data.len() < end {
                data.resize(end, 0);
            }
            data[start..end].copy_from_slice(buf);
            *pos = end as u64;
        }
        self.touched();
        Ok(buf.len())
    }
    pub fn write_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        crate::io::default_write_vectored(|b| self.write(b), bufs)
    }
    pub fn is_write_vectored(&self) -> bool {
        false
    }
    pub fn flush(&self) -> io::Result<()> {
        Ok(())
    }
    pub fn seek(&self, to: SeekFrom) -> io::Result<u64> {
        let len = self.data.read().unwrap().len() as i128;
        let mut pos = self.pos.lock().unwrap();
        let new = match to {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::End(d) => len + d as i128,
            SeekFrom::Current(d) => *pos as i128 + d as i128,
        };
        if new < 0 {
            return Err(io::const_error!(io::ErrorKind::InvalidInput, "seek before start"));
        }
        *pos = new as u64;
        Ok(*pos)
    }
    pub fn size(&self) -> Option<io::Result<u64>> {
        Some(Ok(self.data.read().unwrap().len() as u64))
    }
    pub fn tell(&self) -> io::Result<u64> {
        Ok(*self.pos.lock().unwrap())
    }
    pub fn duplicate(&self) -> io::Result<File> {
        Ok(File {
            path: self.path.clone(),
            data: self.data.clone(),
            pos: Mutex::new(*self.pos.lock().unwrap()),
            append: self.append,
            write: self.write,
            read: self.read,
        })
    }
    pub fn set_permissions(&self, _perm: FilePermissions) -> io::Result<()> {
        Ok(())
    }
    pub fn set_times(&self, times: FileTimes) -> io::Result<()> {
        set_times(Path::new(&self.path), times)
    }
}

impl fmt::Debug for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("File").field("path", &self.path).finish()
    }
}

impl DirBuilder {
    pub fn new() -> DirBuilder {
        DirBuilder {}
    }
    pub fn mkdir(&self, p: &Path) -> io::Result<()> {
        let p = norm(p);
        let mut v = VFS.lock().unwrap();
        if v.nodes.contains_key(&p) || p == "/" {
            return Err(exists_err());
        }
        v.parent_is_dir(&p)?;
        v.nodes.insert(p.clone(), Node::Dir { mtime: now() });
        v.changed.insert(p);
        Ok(())
    }
}

pub fn readdir(p: &Path) -> io::Result<ReadDir> {
    let n = norm(p);
    let v = VFS.lock().unwrap();
    if n != "/" {
        match v.nodes.get(&n) {
            Some(Node::Dir { .. }) => {}
            Some(Node::File { .. }) => return Err(not_dir_err()),
            None => return Err(not_found()),
        }
    }
    Ok(ReadDir { root: PathBuf::from(&n), entries: v.children(&n).into_iter() })
}

pub fn unlink(p: &Path) -> io::Result<()> {
    let p = norm(p);
    let mut v = VFS.lock().unwrap();
    match v.nodes.get(&p) {
        Some(Node::File { .. }) => {
            v.nodes.remove(&p);
            v.changed.insert(p);
            Ok(())
        }
        Some(Node::Dir { .. }) => Err(is_dir_err()),
        None => Err(not_found()),
    }
}

pub fn rename(old: &Path, new: &Path) -> io::Result<()> {
    let (o, n) = (norm(old), norm(new));
    if o == n {
        return Ok(());
    }
    let mut v = VFS.lock().unwrap();
    let node = v.nodes.get(&o).cloned().ok_or_else(not_found)?;
    v.parent_is_dir(&n)?;
    match node {
        Node::File { .. } => {
            if let Some(Node::Dir { .. }) = v.nodes.get(&n) {
                return Err(is_dir_err());
            }
            v.nodes.remove(&o);
            v.nodes.insert(n.clone(), node);
        }
        Node::Dir { .. } => {
            if let Some(Node::Dir { .. }) = v.nodes.get(&n) {
                if !v.children(&n).is_empty() {
                    return Err(io::const_error!(io::ErrorKind::DirectoryNotEmpty, "Directory not empty"));
                }
            }
            let prefix = format!("{o}/");
            let moved: crate::vec::Vec<String> = v
                .nodes
                .range(prefix.clone()..)
                .take_while(|(k, _)| k.starts_with(&prefix))
                .map(|(k, _)| k.clone())
                .collect();
            v.nodes.remove(&o);
            v.nodes.insert(n.clone(), node);
            v.changed.insert(o.clone());
            for k in moved {
                let node = v.nodes.remove(&k).unwrap();
                let nk = format!("{n}{}", &k[o.len()..]);
                v.changed.insert(k);
                v.changed.insert(nk.clone());
                v.nodes.insert(nk, node);
            }
        }
    }
    v.changed.insert(o);
    v.changed.insert(n);
    Ok(())
}

pub fn rename_noreplace(old: &Path, new: &Path) -> io::Result<()> {
    if exists(new)? {
        return Err(exists_err());
    }
    rename(old, new)
}

pub fn set_perm(_p: &Path, _perm: FilePermissions) -> io::Result<()> {
    Ok(())
}
pub fn set_perm_nofollow(p: &Path, perm: FilePermissions) -> io::Result<()> {
    set_perm(p, perm)
}

pub fn set_times(p: &Path, times: FileTimes) -> io::Result<()> {
    let p = norm(p);
    let mut v = VFS.lock().unwrap();
    match v.nodes.get_mut(&p) {
        Some(Node::File { mtime, .. }) | Some(Node::Dir { mtime }) => {
            if let Some(t) = times.modified {
                *mtime = t;
            }
            Ok(())
        }
        None => Err(not_found()),
    }
}
pub fn set_times_nofollow(p: &Path, times: FileTimes) -> io::Result<()> {
    set_times(p, times)
}

pub fn rmdir(p: &Path) -> io::Result<()> {
    let p = norm(p);
    let mut v = VFS.lock().unwrap();
    match v.nodes.get(&p) {
        Some(Node::Dir { .. }) => {
            if !v.children(&p).is_empty() {
                return Err(io::const_error!(io::ErrorKind::DirectoryNotEmpty, "Directory not empty"));
            }
            v.nodes.remove(&p);
            v.changed.insert(p);
            Ok(())
        }
        Some(Node::File { .. }) => Err(not_dir_err()),
        None => Err(not_found()),
    }
}

pub fn remove_dir_all(path: &Path) -> io::Result<()> {
    let p = norm(path);
    let mut v = VFS.lock().unwrap();
    match v.nodes.get(&p) {
        Some(Node::Dir { .. }) => {}
        Some(Node::File { .. }) => return Err(not_dir_err()),
        None => return Err(not_found()),
    }
    let prefix = format!("{p}/");
    let gone: crate::vec::Vec<String> = v
        .nodes
        .range(prefix.clone()..)
        .take_while(|(k, _)| k.starts_with(&prefix))
        .map(|(k, _)| k.clone())
        .collect();
    for k in gone {
        v.nodes.remove(&k);
        v.changed.insert(k);
    }
    v.nodes.remove(&p);
    v.changed.insert(p);
    Ok(())
}

pub fn exists(path: &Path) -> io::Result<bool> {
    let p = norm(path);
    Ok(p == "/" || VFS.lock().unwrap().nodes.contains_key(&p))
}

pub fn readlink(_p: &Path) -> io::Result<PathBuf> {
    Err(io::const_error!(io::ErrorKind::InvalidInput, "not a symlink"))
}
pub fn symlink(_original: &Path, _link: &Path) -> io::Result<()> {
    Err(io::Error::UNSUPPORTED_PLATFORM)
}
pub fn link(src: &Path, dst: &Path) -> io::Result<()> {
    copy(src, dst).map(|_| ())
}

pub fn stat(p: &Path) -> io::Result<FileAttr> {
    let p = norm(p);
    if p == "/" {
        return Ok(FileAttr { size: 0, kind: FileType { dir: true }, mtime: crate::sys::time::UNIX_EPOCH });
    }
    let v = VFS.lock().unwrap();
    match v.nodes.get(&p) {
        Some(Node::Dir { mtime }) => Ok(FileAttr { size: 0, kind: FileType { dir: true }, mtime: *mtime }),
        Some(Node::File { data, mtime }) => {
            Ok(FileAttr { size: data.read().unwrap().len() as u64, kind: FileType { dir: false }, mtime: *mtime })
        }
        None => Err(not_found()),
    }
}
pub fn lstat(p: &Path) -> io::Result<FileAttr> {
    stat(p)
}

pub fn canonicalize(p: &Path) -> io::Result<PathBuf> {
    let n = norm(p);
    if exists(Path::new(&n))? { Ok(PathBuf::from(n)) } else { Err(not_found()) }
}

pub fn copy(from: &Path, to: &Path) -> io::Result<u64> {
    let (f, t) = (norm(from), norm(to));
    let mut v = VFS.lock().unwrap();
    let bytes = match v.nodes.get(&f) {
        Some(Node::File { data, .. }) => data.read().unwrap().clone(),
        Some(Node::Dir { .. }) => return Err(is_dir_err()),
        None => return Err(not_found()),
    };
    v.parent_is_dir(&t)?;
    let len = bytes.len() as u64;
    v.nodes.insert(t.clone(), Node::File { data: Arc::new(RwLock::new(bytes)), mtime: now() });
    v.changed.insert(t);
    Ok(len)
}

impl Hash for FileAttr {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.size.hash(h);
    }
}
