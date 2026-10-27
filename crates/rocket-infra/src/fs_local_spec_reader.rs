//! Reads an OpenAPI spec file from this computer.

use rocket_http::{LocalReadFailure, LocalSpecReader};
use std::io::Read;
use std::path::Path;

/// Reads one regular file. A symlink is followed, but only when it ends at a regular file, so
/// a link to a directory or a device such as `/dev/zero` is refused. Failures carry no path.
#[derive(Debug, Default, Clone, Copy)]
pub struct FsLocalSpecReader;

fn failure_of(error: &std::io::Error) -> LocalReadFailure {
    match error.kind() {
        std::io::ErrorKind::NotFound => LocalReadFailure::NotFound,
        _ => LocalReadFailure::Io,
    }
}

impl LocalSpecReader for FsLocalSpecReader {
    fn read(&self, path: &Path, max_bytes: usize) -> Result<Vec<u8>, LocalReadFailure> {
        let meta = std::fs::metadata(path).map_err(|e| failure_of(&e))?;
        if !meta.is_file() {
            return Err(LocalReadFailure::NotAFile);
        }
        let cap = u64::try_from(max_bytes).unwrap_or(u64::MAX);
        if meta.len() > cap {
            return Err(LocalReadFailure::TooLarge);
        }
        let file = std::fs::File::open(path).map_err(|e| failure_of(&e))?;
        // The file can grow after the size check, so the read itself is capped too.
        let mut bytes = Vec::new();
        file.take(cap.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|e| failure_of(&e))?;
        if bytes.len() > max_bytes {
            return Err(LocalReadFailure::TooLarge);
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn fs_reader_reads_a_regular_file() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("api.yaml");
        std::fs::write(&path, b"openapi: 3.0.0").expect("write");
        assert_eq!(
            FsLocalSpecReader.read(&path, 1024).expect("read"),
            b"openapi: 3.0.0"
        );
    }

    #[test]
    fn unreadable_files_give_fixed_failures() {
        let dir = tempdir().expect("tempdir");
        assert_eq!(
            FsLocalSpecReader.read(&dir.path().join("missing.yaml"), 1024),
            Err(LocalReadFailure::NotFound)
        );
        assert_eq!(
            FsLocalSpecReader.read(dir.path(), 1024),
            Err(LocalReadFailure::NotAFile)
        );
        let big = dir.path().join("big.yaml");
        std::fs::write(&big, vec![b'a'; 2048]).expect("write");
        assert_eq!(
            FsLocalSpecReader.read(&big, 1024),
            Err(LocalReadFailure::TooLarge)
        );
        // The failure text names no path.
        let text = LocalReadFailure::NotFound.to_string();
        assert!(!text.contains('/'), "{text}");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_to_a_directory_or_device_is_not_a_file() {
        let dir = tempdir().expect("tempdir");
        let link = dir.path().join("dir-link");
        std::os::unix::fs::symlink(dir.path(), &link).expect("symlink");
        assert_eq!(
            FsLocalSpecReader.read(&link, 1024),
            Err(LocalReadFailure::NotAFile)
        );
        let dev = dir.path().join("dev-link");
        std::os::unix::fs::symlink("/dev/zero", &dev).expect("symlink");
        assert_eq!(
            FsLocalSpecReader.read(&dev, 1024),
            Err(LocalReadFailure::NotAFile)
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_to_a_regular_file_is_followed() {
        let dir = tempdir().expect("tempdir");
        let real = dir.path().join("real.yaml");
        std::fs::write(&real, b"x: 1").expect("write");
        let link = dir.path().join("link.yaml");
        std::os::unix::fs::symlink(&real, &link).expect("symlink");
        assert_eq!(FsLocalSpecReader.read(&link, 1024).expect("read"), b"x: 1");
    }
}
