use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Write `content` to `path` atomically using a temp-file-then-rename strategy.
///
/// Steps:
/// 1. Ensure the parent directory exists.
/// 2. Write content to `<path>.tmp.<nanos>`, then call `sync_data`.
/// 3. Rename the `.tmp.<nanos>` file over `path` (atomic on POSIX, best-effort on Windows).
///
/// If the write or sync fails, the `.tmp.<nanos>` file is removed before returning the error.
pub fn atomic_write(path: &Path, content: &[u8]) -> std::io::Result<()> {
    // Ensure the parent directory exists.
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    // Generate a unique suffix from current time nanos to avoid concurrent write collisions.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let tmp_path = path.with_extension(
        path.extension()
            .map(|e| format!("{}.tmp.{:08x}", e.to_string_lossy(), nanos))
            .unwrap_or_else(|| format!("tmp.{:08x}", nanos)),
    );

    let write_result = (|| {
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(content)?;
        file.sync_data()
    })();

    if let Err(e) = write_result {
        // Best-effort cleanup of the temp file.
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }

    // Rename is atomic on POSIX; best-effort on Windows.
    if let Err(e) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn atomic_write_creates_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("output.yml");
        atomic_write(&path, b"hello: world\n").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "hello: world\n");
    }

    #[test]
    fn atomic_write_is_atomic_on_existing_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("data.yml");

        // Write initial content A.
        atomic_write(&path, b"version: A\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "version: A\n");

        // Overwrite with content B.
        atomic_write(&path, b"version: B\n").unwrap();

        // File must contain B, not a truncated or empty state.
        let final_content = fs::read_to_string(&path).unwrap();
        assert_eq!(final_content, "version: B\n");
    }

    #[test]
    fn atomic_write_tmp_file_removed_after_success() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("output.yml");
        atomic_write(&path, b"content: ok\n").unwrap();

        // After successful write, no .tmp.* files should remain in the directory.
        let entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .collect();

        for entry in &entries {
            let name = entry.to_string_lossy();
            assert!(
                !name.contains(".tmp"),
                "Temp file {} was not cleaned up",
                name
            );
        }

        // Verify the actual file exists and has the right content.
        assert_eq!(fs::read_to_string(&path).unwrap(), "content: ok\n");
    }

    #[test]
    fn atomic_write_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let nested_path = dir.path().join("subdir").join("nested").join("file.yml");

        // Parent directories should not exist yet.
        assert!(!nested_path.parent().unwrap().exists());

        // atomic_write should create them.
        atomic_write(&nested_path, b"nested: content\n").unwrap();

        // Verify the file was created with correct content.
        assert!(nested_path.exists());
        assert_eq!(
            fs::read_to_string(&nested_path).unwrap(),
            "nested: content\n"
        );
    }
}
