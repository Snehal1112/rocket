use std::path::Path;

use crate::error::ImportResult;

/// Extract a Bruno ZIP to a temp directory.
///
/// Bruno ZIPs always contain exactly one top-level directory (e.g. `my-workspace/`).
/// Returns `(TempDir, inner_path)`. The caller must keep `TempDir` alive for the
/// duration of the import — dropping it deletes the extracted files.
pub(crate) fn extract_to_temp(zip_path: &Path) -> ImportResult<(tempfile::TempDir, std::path::PathBuf)> {
    use crate::error::ImportError;
    use std::fs;

    let file = fs::File::open(zip_path)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| ImportError::ZipExtractionFailed(e.to_string()))?;

    let temp_dir = tempfile::TempDir::new()?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| ImportError::ZipExtractionFailed(e.to_string()))?;

        let out_path = match entry.enclosed_name() {
            Some(p) => temp_dir.path().join(p),
            None => continue,
        };

        if entry.is_dir() {
            fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out_file = fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out_file)?;
        }
    }

    // Some ZIPs have no wrapper folder — workspace.yml sits directly at the archive root.
    let root = temp_dir.path().to_path_buf();
    if root.join("workspace.yml").exists()
        || root.join("opencollection.yml").exists()
        || root.join("bruno.json").exists()
    {
        return Ok((temp_dir, root));
    }

    // Standard case: Bruno ZIPs extract to a single top-level folder.
    let inner = fs::read_dir(temp_dir.path())?
        .filter_map(|e| e.ok())
        .find(|e| e.path().is_dir())
        .map(|e| e.path())
        .ok_or(ImportError::EmptyZip)?;

    Ok((temp_dir, inner))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;

    use super::*;
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

        let (_dir, inner) = extract_to_temp(&zip_path).unwrap();
        assert!(inner.is_dir());
        assert_eq!(inner.file_name().unwrap(), "my-workspace");
        assert!(inner.join("workspace.yml").exists());
    }

    #[test]
    fn flat_root_zip_returns_temp_root_when_marker_at_root() {
        // Reproduces the "new workspace.zip" case: no wrapper folder, workspace.yml at root.
        let tmp = make_test_zip(&[
            ("workspace.yml", "opencollection: 1.0.0\n"),
            ("collections/my-col/opencollection.yml", "opencollection: 1.0.0\n"),
        ]);
        let zip_path = tmp.path().join("test.zip");

        let (_dir, inner) = extract_to_temp(&zip_path).unwrap();
        assert!(inner.join("workspace.yml").exists(), "should return root");
    }

    #[test]
    fn returns_empty_zip_error_when_no_top_level_dir() {
        let tmp = make_test_zip(&[("readme.txt", "hello")]);
        let zip_path = tmp.path().join("test.zip");

        let result = extract_to_temp(&zip_path);
        assert!(
            matches!(result, Err(crate::error::ImportError::EmptyZip)),
            "expected EmptyZip, got: {:?}",
            result
        );
    }
}
