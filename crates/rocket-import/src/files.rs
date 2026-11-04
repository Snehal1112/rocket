//! The file port of the importer. Disk access for imported files goes through this trait.
//! `rocket-infra` implements it with `std::fs`.

use std::io;
use std::path::{Path, PathBuf};

/// What a path is, without following a symlink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
    Other,
}

/// One entry of a directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntryInfo {
    pub path: PathBuf,
    pub kind: EntryKind,
}

pub trait ImportFiles: Send + Sync {
    /// Lists the entries of `dir`. A symlink to a directory is reported as `Symlink`.
    fn list_dir(&self, dir: &Path) -> io::Result<Vec<DirEntryInfo>>;
    /// Tells what `path` is. It does not follow a symlink.
    fn kind(&self, path: &Path) -> io::Result<EntryKind>;
    fn create_dir_all(&self, dir: &Path) -> io::Result<()>;
    fn copy(&self, src: &Path, dest: &Path) -> io::Result<()>;
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
    fn write(&self, path: &Path, text: &str) -> io::Result<()>;
}
