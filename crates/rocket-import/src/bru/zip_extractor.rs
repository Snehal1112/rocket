use std::path::Path;

use crate::error::ImportResult;
use crate::files::{EntryKind, ImportFiles, WorkDir};
use crate::limits::{MAX_ZIP_ENTRIES, MAX_ZIP_TOTAL_BYTES};

/// Extract a Bruno ZIP to a temp directory.
///
/// Bruno ZIPs always contain exactly one top-level directory (e.g. `my-workspace/`).
/// Returns `(WorkDir, inner_path)`. The caller must keep the `WorkDir` alive for the
/// duration of the import — dropping it deletes the extracted files.
pub(crate) fn extract_to_temp(
    files: &dyn ImportFiles,
    zip_path: &Path,
) -> ImportResult<(Box<dyn WorkDir>, std::path::PathBuf)> {
    extract_with_limits(files, zip_path, MAX_ZIP_ENTRIES, MAX_ZIP_TOTAL_BYTES)
}

/// Extracts like `extract_to_temp`, with the entry and byte limits given.
fn extract_with_limits(
    files: &dyn ImportFiles,
    zip_path: &Path,
    max_entries: usize,
    max_bytes: u64,
) -> ImportResult<(Box<dyn WorkDir>, std::path::PathBuf)> {
    use crate::error::ImportError;
    use std::io::Read;

    let file = files.open_read(zip_path)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| ImportError::ZipExtractionFailed(e.to_string()))?;

    // Count the entries first, so nothing is written for an archive that is refused.
    if archive.len() > max_entries {
        return Err(ImportError::ZipExtractionFailed(format!(
            "the zip has more than {max_entries} entries"
        )));
    }

    let temp_dir = files.temp_dir()?;
    let mut written_total: u64 = 0;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| ImportError::ZipExtractionFailed(e.to_string()))?;

        let out_path = match entry.enclosed_name() {
            Some(p) => temp_dir.path().join(p),
            None => continue,
        };

        if entry.is_dir() {
            files.create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                files.create_dir_all(parent)?;
            }
            // The declared size is not trusted, so the stream itself is bounded.
            let remaining = max_bytes.saturating_sub(written_total).saturating_add(1);
            let mut bounded = (&mut entry).take(remaining);
            written_total =
                written_total.saturating_add(files.write_stream(&out_path, &mut bounded)?);
            if written_total > max_bytes {
                return Err(ImportError::ZipExtractionFailed(format!(
                    "the zip expands to more than {max_bytes} bytes"
                )));
            }
        }
    }

    // Some ZIPs have no wrapper folder — workspace.yml sits directly at the archive root.
    let root = temp_dir.path().to_path_buf();
    let has_marker = ["workspace.yml", "opencollection.yml", "bruno.json"]
        .iter()
        .any(|name| files.kind(&root.join(name)).is_ok());
    if has_marker {
        return Ok((temp_dir, root));
    }

    // Standard case: Bruno ZIPs extract to a single top-level folder.
    let inner = files
        .list_dir(temp_dir.path())?
        .into_iter()
        .find(|e| e.kind == EntryKind::Dir)
        .map(|e| e.path)
        .ok_or(ImportError::EmptyZip)?;

    Ok((temp_dir, inner))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;

    use super::*;
    use crate::importer::test_files::StdFiles;
    use std::fs;

    /// Build a minimal ZIP in memory with the given entries and return the TempDir holding it.
    fn make_test_zip(content: &[(&str, &str)]) -> TempDir {
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("test.zip");
        let file = fs::File::create(&zip_path).unwrap();
        let mut w = zip::ZipWriter::new(file);
        let opts = SimpleFileOptions::default();
        for (name, data) in content {
            if name.ends_with('/') {
                w.add_directory(*name, opts).unwrap();
            } else {
                w.start_file(*name, opts).unwrap();
                w.write_all(data.as_bytes()).unwrap();
            }
        }
        w.finish().unwrap();
        tmp
    }

    #[test]
    fn extracts_zip_and_returns_inner_dir() {
        let tmp = make_test_zip(&[
            ("my-workspace/", ""),
            ("my-workspace/workspace.yml", "opencollection: 1.0.0\n"),
        ]);
        let zip_path = tmp.path().join("test.zip");

        let (_dir, inner) = extract_to_temp(&StdFiles, &zip_path).unwrap();
        assert!(inner.is_dir());
        assert_eq!(inner.file_name().unwrap(), "my-workspace");
        assert!(inner.join("workspace.yml").exists());
    }

    #[test]
    fn flat_root_zip_returns_temp_root_when_marker_at_root() {
        // Reproduces the "new workspace.zip" case: no wrapper folder, workspace.yml at root.
        let tmp = make_test_zip(&[
            ("workspace.yml", "opencollection: 1.0.0\n"),
            (
                "collections/my-col/opencollection.yml",
                "opencollection: 1.0.0\n",
            ),
        ]);
        let zip_path = tmp.path().join("test.zip");

        let (_dir, inner) = extract_to_temp(&StdFiles, &zip_path).unwrap();
        assert!(inner.join("workspace.yml").exists(), "should return root");
    }

    #[test]
    fn returns_empty_zip_error_when_no_top_level_dir() {
        let tmp = make_test_zip(&[("readme.txt", "hello")]);
        let zip_path = tmp.path().join("test.zip");

        let result = extract_to_temp(&StdFiles, &zip_path);
        assert!(
            matches!(result, Err(crate::error::ImportError::EmptyZip)),
            "expected EmptyZip, got: {:?}",
            result
        );
    }

    use crate::files::{DirEntryInfo, ReadSeek};
    use std::io::Cursor;
    use std::path::PathBuf;
    use std::sync::Mutex;

    #[derive(Debug)]
    struct FakeDir(PathBuf);

    impl WorkDir for FakeDir {
        fn path(&self) -> &Path {
            &self.0
        }
    }

    /// A port that serves a zip from memory and records the writes.
    struct ZipFiles {
        zip: Vec<u8>,
        writes: Mutex<Vec<String>>,
    }

    impl ZipFiles {
        fn new(zip: Vec<u8>) -> Self {
            Self {
                zip,
                writes: Mutex::new(Vec::new()),
            }
        }
        fn write_count(&self) -> usize {
            self.writes.lock().map(|w| w.len()).unwrap_or(0)
        }
    }

    impl ImportFiles for ZipFiles {
        fn list_dir(&self, dir: &Path) -> std::io::Result<Vec<DirEntryInfo>> {
            Ok(vec![DirEntryInfo {
                path: dir.join("inner"),
                kind: EntryKind::Dir,
            }])
        }
        fn kind(&self, _path: &Path) -> std::io::Result<EntryKind> {
            Err(std::io::Error::other("missing"))
        }
        fn create_dir_all(&self, _dir: &Path) -> std::io::Result<()> {
            Ok(())
        }
        fn copy(&self, _src: &Path, _dest: &Path) -> std::io::Result<()> {
            Err(std::io::Error::other("unsupported"))
        }
        fn read_to_string(&self, _path: &Path) -> std::io::Result<String> {
            Err(std::io::Error::other("unsupported"))
        }
        fn write(&self, _path: &Path, _text: &str) -> std::io::Result<()> {
            Err(std::io::Error::other("unsupported"))
        }
        fn regular_file_len(&self, _path: &Path) -> std::io::Result<Option<u64>> {
            Ok(None)
        }
        fn read_up_to(&self, _path: &Path, _max: u64) -> std::io::Result<Vec<u8>> {
            Err(std::io::Error::other("unsupported"))
        }
        fn canonicalize(&self, _path: &Path) -> std::io::Result<PathBuf> {
            Err(std::io::Error::other("unsupported"))
        }
        fn open_read(&self, _path: &Path) -> std::io::Result<Box<dyn ReadSeek>> {
            Ok(Box::new(Cursor::new(self.zip.clone())))
        }
        fn temp_dir(&self) -> std::io::Result<Box<dyn WorkDir>> {
            Ok(Box::new(FakeDir(PathBuf::from("/fake-work"))))
        }
        fn write_stream(&self, path: &Path, from: &mut dyn std::io::Read) -> std::io::Result<u64> {
            let n = std::io::copy(from, &mut std::io::sink())?;
            if let Ok(mut w) = self.writes.lock() {
                w.push(path.to_string_lossy().into_owned());
            }
            Ok(n)
        }
    }

    fn zip_bytes(entries: &[(String, usize)]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = SimpleFileOptions::default();
        for (name, size) in entries {
            w.start_file(name.as_str(), opts).unwrap();
            w.write_all(&vec![b'a'; *size]).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    fn zip_error(result: ImportResult<(Box<dyn WorkDir>, PathBuf)>) -> String {
        match result {
            Err(crate::error::ImportError::ZipExtractionFailed(m)) => m,
            other => panic!("expected ZipExtractionFailed, got {:?}", other.map(|r| r.1)),
        }
    }

    #[test]
    fn a_zip_with_too_many_entries_is_refused_before_writing() {
        let entries: Vec<(String, usize)> = (0..20_001).map(|i| (format!("d/f{i}"), 0)).collect();
        let files = ZipFiles::new(zip_bytes(&entries));

        let result = extract_to_temp(&files, Path::new("a.zip"));

        assert_eq!(zip_error(result), "the zip has more than 20000 entries");
        assert_eq!(files.write_count(), 0);
    }

    #[test]
    fn a_zip_that_expands_past_the_limit_is_refused() {
        let files = ZipFiles::new(zip_bytes(&[("d/big.txt".to_string(), 2048)]));

        let result = extract_with_limits(&files, Path::new("a.zip"), 10, 1024);

        assert_eq!(zip_error(result), "the zip expands to more than 1024 bytes");
    }

    #[test]
    fn a_normal_zip_still_extracts() {
        let files = ZipFiles::new(zip_bytes(&[
            ("d/a.txt".to_string(), 10),
            ("d/b.txt".to_string(), 20),
        ]));

        let result = extract_with_limits(&files, Path::new("a.zip"), 10, 1024);

        assert!(result.is_ok());
        assert_eq!(files.write_count(), 2);
    }
}
