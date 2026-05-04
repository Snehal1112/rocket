use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Write `content` to `path` atomically using a temp-file-then-rename strategy.
///
/// Steps:
/// 1. Ensure the parent directory exists.
/// 2. Write content to `<path>.tmp.<pid>_<nanos>`, then call `sync_all`.
/// 3. Rename the tmp file over `path` (atomic on POSIX, best-effort on Windows).
/// 4. Best-effort fsync the parent directory to make the rename durable.
///
/// If the write or sync fails, the tmp file is removed before returning the error.
pub fn atomic_write(path: &Path, content: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    // PID + nanoseconds gives a collision-resistant tmp suffix.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let pid = std::process::id();
    let tmp_path = path.with_extension(
        path.extension()
            .map(|e| format!("{}.tmp.{pid}_{nanos:08x}", e.to_string_lossy()))
            .unwrap_or_else(|| format!("tmp.{pid}_{nanos:08x}")),
    );

    let write_result = (|| {
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(content)?;
        // sync_all flushes both data and metadata (unlike sync_data).
        file.sync_all()
    })();

    if let Err(e) = write_result {
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }

    if let Err(e) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }

    // Best-effort parent-dir fsync to make the rename durable on Linux ext4/xfs.
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            if let Ok(dir_file) = fs::File::open(parent) {
                let _ = dir_file.sync_all();
            }
        }
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
    fn no_leftover_tmp_files_after_sequential_writes() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("data.yml");
        atomic_write(&path, b"v1\n").unwrap();
        atomic_write(&path, b"v2\n").unwrap();
        let tmps: Vec<_> = fs::read_dir(dir.path()).unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(tmps.is_empty(), "leftover tmp files: {:?}", tmps);
        assert_eq!(fs::read_to_string(&path).unwrap(), "v2\n");
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
