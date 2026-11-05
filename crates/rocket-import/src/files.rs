//! The file port of the importer. Disk access for imported files goes through this trait.
//! `rocket-infra` implements it with `std::fs`.

use std::io::{self, Read, Seek};
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

/// A readable and seekable stream, as the zip reader needs.
pub trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}

/// A scratch directory. Dropping it deletes the directory and everything in it.
pub trait WorkDir: Send + std::fmt::Debug {
    fn path(&self) -> &Path;
}

pub trait ImportFiles: Send + Sync {
    /// Lists the entries of `dir`. A symlink to a directory is reported as `Symlink`.
    fn list_dir(&self, dir: &Path) -> io::Result<Vec<DirEntryInfo>>;
    /// Tells what `path` is. It does not follow a symlink.
    fn kind(&self, path: &Path) -> io::Result<EntryKind>;
    fn create_dir_all(&self, dir: &Path) -> io::Result<()>;
    fn copy(&self, src: &Path, dest: &Path) -> io::Result<()>;
    /// Reads the whole file with no size cap. Imported files go through `read_text_capped`.
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
    fn write(&self, path: &Path, text: &str) -> io::Result<()>;
    /// The length of the file at `path`, following symlinks. `None` when it is not a regular
    /// file (a directory, a device or a pipe).
    fn regular_file_len(&self, path: &Path) -> io::Result<Option<u64>>;
    /// Reads at most `max_bytes` bytes from the start of the file.
    fn read_up_to(&self, path: &Path, max_bytes: u64) -> io::Result<Vec<u8>>;
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
    fn open_read(&self, path: &Path) -> io::Result<Box<dyn ReadSeek>>;
    /// Creates an empty scratch directory.
    fn temp_dir(&self) -> io::Result<Box<dyn WorkDir>>;
    /// Writes the whole stream to `path`, replacing the file. Returns the bytes written.
    fn write_stream(&self, path: &Path, from: &mut dyn Read) -> io::Result<u64>;
}
