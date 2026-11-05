//! Size limits for what an import reads, so one huge file cannot exhaust memory.

use crate::error::ImportResult;
use crate::files::ImportFiles;
use std::io;
use std::path::Path;

/// The most bytes the importer reads from one text file.
pub(crate) const MAX_IMPORT_FILE_BYTES: u64 = 32 * 1024 * 1024;
/// The most entries a zip archive may hold.
pub(crate) const MAX_ZIP_ENTRIES: usize = 20_000;
/// The most bytes a zip archive may expand to.
pub(crate) const MAX_ZIP_TOTAL_BYTES: u64 = 512 * 1024 * 1024;

fn invalid(kind: io::ErrorKind, message: String) -> crate::error::ImportError {
    crate::error::ImportError::IoError(io::Error::new(kind, message))
}

/// Reads a text file of at most `max` bytes. A file that is not regular, is too large or is not
/// UTF-8 is refused with an error. The length is checked before any byte is read.
pub(crate) fn read_text_capped(
    files: &dyn ImportFiles,
    path: &Path,
    max: u64,
) -> ImportResult<String> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let Some(len) = files.regular_file_len(path)? else {
        return Err(invalid(
            io::ErrorKind::InvalidInput,
            format!("{name} is not a regular file"),
        ));
    };
    let too_large = || {
        invalid(
            io::ErrorKind::InvalidInput,
            format!("{name} is larger than {max} bytes"),
        )
    };
    if len > max {
        return Err(too_large());
    }
    // Read one byte more than the cap, so a file that grew after the length check is caught.
    let bytes = files.read_up_to(path, max.saturating_add(1))?;
    if bytes.len() as u64 > max {
        return Err(too_large());
    }
    String::from_utf8(bytes).map_err(|_| {
        invalid(
            io::ErrorKind::InvalidData,
            format!("{name} is not valid UTF-8 text"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::{DirEntryInfo, EntryKind, ReadSeek, WorkDir};
    use std::path::PathBuf;

    /// A port that serves one file of a chosen length. `read_up_to` panics when told to.
    struct SizedFile {
        len: Option<u64>,
        bytes: Vec<u8>,
        forbid_read: bool,
    }

    impl ImportFiles for SizedFile {
        fn list_dir(&self, _dir: &Path) -> io::Result<Vec<DirEntryInfo>> {
            Err(io::Error::other("unsupported"))
        }
        fn kind(&self, _path: &Path) -> io::Result<EntryKind> {
            Err(io::Error::other("unsupported"))
        }
        fn create_dir_all(&self, _dir: &Path) -> io::Result<()> {
            Err(io::Error::other("unsupported"))
        }
        fn copy(&self, _src: &Path, _dest: &Path) -> io::Result<()> {
            Err(io::Error::other("unsupported"))
        }
        fn read_to_string(&self, _path: &Path) -> io::Result<String> {
            panic!("read_to_string must not be used");
        }
        fn write(&self, _path: &Path, _text: &str) -> io::Result<()> {
            Err(io::Error::other("unsupported"))
        }
        fn regular_file_len(&self, _path: &Path) -> io::Result<Option<u64>> {
            Ok(self.len)
        }
        fn read_up_to(&self, _path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
            assert!(!self.forbid_read, "read_up_to must not be called");
            let n = usize::try_from(max_bytes).unwrap_or(usize::MAX);
            Ok(self.bytes.iter().copied().take(n).collect())
        }
        fn canonicalize(&self, _path: &Path) -> io::Result<PathBuf> {
            Err(io::Error::other("unsupported"))
        }
        fn open_read(&self, _path: &Path) -> io::Result<Box<dyn ReadSeek>> {
            Err(io::Error::other("unsupported"))
        }
        fn temp_dir(&self) -> io::Result<Box<dyn WorkDir>> {
            Err(io::Error::other("unsupported"))
        }
        fn write_stream(&self, _path: &Path, _from: &mut dyn io::Read) -> io::Result<u64> {
            Err(io::Error::other("unsupported"))
        }
    }

    fn invalid_input(result: ImportResult<String>) -> String {
        match result {
            Err(crate::error::ImportError::IoError(e)) => {
                assert_eq!(e.kind(), io::ErrorKind::InvalidInput);
                e.to_string()
            }
            other => panic!("expected an InvalidInput io error, got {other:?}"),
        }
    }

    #[test]
    fn reads_a_file_at_the_cap() {
        let files = SizedFile {
            len: Some(5),
            bytes: b"hello".to_vec(),
            forbid_read: false,
        };

        let text = read_text_capped(&files, Path::new("a.bru"), 5).expect("read");

        assert_eq!(text, "hello");
    }

    #[test]
    fn refuses_one_byte_over_the_cap() {
        let files = SizedFile {
            len: Some(6),
            bytes: Vec::new(),
            forbid_read: true,
        };

        let msg = invalid_input(read_text_capped(&files, Path::new("a.bru"), 5));

        assert_eq!(msg, "a.bru is larger than 5 bytes");
    }

    #[test]
    fn refuses_a_file_that_grew_past_the_cap_while_reading() {
        let files = SizedFile {
            len: Some(5),
            bytes: b"123456789".to_vec(),
            forbid_read: false,
        };

        let msg = invalid_input(read_text_capped(&files, Path::new("a.bru"), 5));

        assert_eq!(msg, "a.bru is larger than 5 bytes");
    }

    #[test]
    fn refuses_a_non_regular_file() {
        let files = SizedFile {
            len: None,
            bytes: Vec::new(),
            forbid_read: true,
        };

        let msg = invalid_input(read_text_capped(&files, Path::new("dir/pipe.bru"), 5));

        assert_eq!(msg, "pipe.bru is not a regular file");
    }

    #[test]
    fn a_bad_encoding_is_invalid_data() {
        let files = SizedFile {
            len: Some(2),
            bytes: vec![0xff, 0xfe],
            forbid_read: false,
        };

        match read_text_capped(&files, Path::new("a.bru"), 5) {
            Err(crate::error::ImportError::IoError(e)) => {
                assert_eq!(e.kind(), io::ErrorKind::InvalidData);
            }
            other => panic!("expected InvalidData, got {other:?}"),
        }
    }

    #[test]
    fn parse_file_refuses_an_oversized_bru() {
        let files = SizedFile {
            len: Some(MAX_IMPORT_FILE_BYTES + 1),
            bytes: Vec::new(),
            forbid_read: true,
        };

        let result = crate::bru::parse_file(&files, Path::new("big.bru"));

        assert!(matches!(
            result,
            Err(crate::error::ImportError::IoError(ref e)) if e.kind() == io::ErrorKind::InvalidInput
        ));
    }
}
