//! Filesystem implementation of the importer file port.

use std::io::{self, Read};
use std::path::{Path, PathBuf};

use rocket_import::{DirEntryInfo, EntryKind, ImportFiles, ReadSeek, WorkDir};

pub struct FsImportFiles;

fn kind_of(file_type: std::fs::FileType) -> EntryKind {
    if file_type.is_symlink() {
        EntryKind::Symlink
    } else if file_type.is_dir() {
        EntryKind::Dir
    } else if file_type.is_file() {
        EntryKind::File
    } else {
        EntryKind::Other
    }
}

impl ImportFiles for FsImportFiles {
    fn list_dir(&self, dir: &Path) -> io::Result<Vec<DirEntryInfo>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            out.push(DirEntryInfo {
                path: entry.path(),
                kind: kind_of(entry.file_type()?),
            });
        }
        Ok(out)
    }

    fn kind(&self, path: &Path) -> io::Result<EntryKind> {
        Ok(kind_of(std::fs::symlink_metadata(path)?.file_type()))
    }

    fn create_dir_all(&self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)
    }

    fn copy(&self, src: &Path, dest: &Path) -> io::Result<()> {
        std::fs::copy(src, dest).map(|_| ())
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn write(&self, path: &Path, text: &str) -> io::Result<()> {
        std::fs::write(path, text)
    }

    fn regular_file_len(&self, path: &Path) -> io::Result<Option<u64>> {
        let meta = std::fs::metadata(path)?;
        Ok(meta.is_file().then_some(meta.len()))
    }

    fn read_up_to(&self, path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
        let mut buf = Vec::new();
        std::fs::File::open(path)?
            .take(max_bytes)
            .read_to_end(&mut buf)?;
        Ok(buf)
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }

    fn open_read(&self, path: &Path) -> io::Result<Box<dyn ReadSeek>> {
        Ok(Box::new(std::fs::File::open(path)?))
    }

    fn temp_dir(&self) -> io::Result<Box<dyn WorkDir>> {
        Ok(Box::new(TempWorkDir(tempfile::TempDir::new()?)))
    }

    fn write_stream(&self, path: &Path, from: &mut dyn Read) -> io::Result<u64> {
        let mut out = std::fs::File::create(path)?;
        io::copy(from, &mut out)
    }
}

/// A scratch directory that is deleted when dropped.
#[derive(Debug)]
struct TempWorkDir(tempfile::TempDir);

impl WorkDir for TempWorkDir {
    fn path(&self) -> &Path {
        self.0.path()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn list_dir_reports_files_dirs_and_symlinks() {
        let tmp = TempDir::new().expect("tmp");
        std::fs::write(tmp.path().join("a.yml"), "x").expect("write");
        std::fs::create_dir(tmp.path().join("sub")).expect("mkdir");
        #[cfg(unix)]
        std::os::unix::fs::symlink(tmp.path().join("a.yml"), tmp.path().join("link")).expect("ln");

        let mut entries = FsImportFiles.list_dir(tmp.path()).expect("list");
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        let kinds: Vec<_> = entries.iter().map(|e| e.kind).collect();

        #[cfg(unix)]
        assert_eq!(
            kinds,
            vec![EntryKind::File, EntryKind::Symlink, EntryKind::Dir]
        );
        #[cfg(not(unix))]
        assert_eq!(kinds, vec![EntryKind::File, EntryKind::Dir]);
    }

    #[cfg(unix)]
    #[test]
    fn kind_does_not_follow_a_symlink() {
        let tmp = TempDir::new().expect("tmp");
        std::fs::write(tmp.path().join("a.txt"), "x").expect("write");
        std::os::unix::fs::symlink(tmp.path().join("a.txt"), tmp.path().join("link")).expect("ln");

        assert_eq!(
            FsImportFiles.kind(&tmp.path().join("link")).expect("kind"),
            EntryKind::Symlink
        );
        assert_eq!(
            FsImportFiles.kind(&tmp.path().join("a.txt")).expect("kind"),
            EntryKind::File
        );
    }

    #[test]
    fn copy_and_write_round_trip_through_read_to_string() {
        let tmp = TempDir::new().expect("tmp");
        let a = tmp.path().join("a.txt");
        let b = tmp.path().join("b.txt");

        FsImportFiles.write(&a, "hello").expect("write");
        FsImportFiles.copy(&a, &b).expect("copy");

        assert_eq!(FsImportFiles.read_to_string(&b).expect("read"), "hello");
    }

    #[test]
    fn create_dir_all_makes_nested_dirs() {
        let tmp = TempDir::new().expect("tmp");
        let nested = tmp.path().join("a").join("b").join("c");

        FsImportFiles.create_dir_all(&nested).expect("mkdir");

        assert!(nested.is_dir());
    }

    #[test]
    fn regular_file_len_is_none_for_a_directory() {
        let tmp = TempDir::new().expect("tmp");
        std::fs::write(tmp.path().join("a.txt"), "12345").expect("write");

        assert_eq!(
            FsImportFiles
                .regular_file_len(&tmp.path().join("a.txt"))
                .expect("len"),
            Some(5)
        );
        assert_eq!(
            FsImportFiles.regular_file_len(tmp.path()).expect("len"),
            None
        );
    }

    #[test]
    fn read_up_to_stops_at_the_limit() {
        let tmp = TempDir::new().expect("tmp");
        let file = tmp.path().join("a.txt");
        std::fs::write(&file, "0123456789").expect("write");

        assert_eq!(FsImportFiles.read_up_to(&file, 4).expect("read"), b"0123");
    }

    #[test]
    fn canonicalize_of_a_missing_path_errors() {
        let tmp = TempDir::new().expect("tmp");

        assert!(FsImportFiles
            .canonicalize(&tmp.path().join("nope"))
            .is_err());
    }

    #[test]
    fn write_stream_writes_every_byte() {
        let tmp = TempDir::new().expect("tmp");
        let file = tmp.path().join("out.bin");

        let n = FsImportFiles
            .write_stream(&file, &mut std::io::Cursor::new(b"abc".to_vec()))
            .expect("write");

        assert_eq!(
            (n, std::fs::read(&file).expect("read")),
            (3, b"abc".to_vec())
        );
    }

    #[test]
    fn a_temp_dir_is_deleted_when_dropped() {
        let dir = FsImportFiles.temp_dir().expect("temp");
        let path = dir.path().to_path_buf();
        assert!(path.is_dir());

        drop(dir);

        assert!(!path.exists());
    }

    #[test]
    fn open_read_of_a_missing_file_errors() {
        let tmp = TempDir::new().expect("tmp");

        assert!(FsImportFiles.open_read(&tmp.path().join("nope")).is_err());
    }

    #[test]
    fn list_dir_of_a_missing_dir_errors() {
        let tmp = TempDir::new().expect("tmp");

        assert!(FsImportFiles.list_dir(&tmp.path().join("nope")).is_err());
    }
}
