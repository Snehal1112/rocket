use crate::scripting::ops::ScriptOpError;
use deno_core::op2;
use std::fs;

fn io_err(e: std::io::Error, path: &str) -> ScriptOpError {
    ScriptOpError(format!("{path}: {e}"))
}

fn read_file_impl(path: &str, encoding: &str) -> Result<String, ScriptOpError> {
    let bytes = fs::read(path).map_err(|e| io_err(e, path))?;
    if encoding == "base64" {
        use base64::Engine;
        Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
    } else {
        String::from_utf8(bytes).map_err(|e| ScriptOpError(format!("{path}: not valid utf8: {e}")))
    }
}

/// Backs `fs.readFile(path, encoding)`. `encoding` is a required string ("utf8" or "base64")
/// since the op has no way to express an omitted argument; the JS wrapper must always supply it.
#[op2]
#[string]
pub fn op_fs_read_file(
    #[string] path: String,
    #[string] encoding: String,
) -> Result<String, ScriptOpError> {
    read_file_impl(&path, &encoding)
}

fn write_file_impl(path: &str, content: &str, encoding: &str) -> Result<(), ScriptOpError> {
    let bytes = if encoding == "base64" {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(content)
            .map_err(|e| ScriptOpError(format!("{path}: invalid base64: {e}")))?
    } else {
        content.as_bytes().to_vec()
    };
    fs::write(path, bytes).map_err(|e| io_err(e, path))
}

/// Backs `fs.writeFile(path, content, encoding)`. `encoding` is a required string ("utf8" or
/// "base64") that the JS wrapper must always supply explicitly, since the op has no way to
/// express an omitted argument.
#[op2(fast)]
pub fn op_fs_write_file(
    #[string] path: String,
    #[string] content: String,
    #[string] encoding: String,
) -> Result<(), ScriptOpError> {
    write_file_impl(&path, &content, &encoding)
}

fn read_dir_impl(path: &str) -> Result<String, ScriptOpError> {
    let entries = fs::read_dir(path).map_err(|e| io_err(e, path))?;
    let mut items = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| io_err(e, path))?;
        let file_type = entry.file_type().map_err(|e| io_err(e, path))?;
        items.push(serde_json::json!({
            "name": entry.file_name().to_string_lossy().to_string(),
            "isDirectory": file_type.is_dir(),
            "isFile": file_type.is_file(),
        }));
    }
    Ok(serde_json::to_string(&items).unwrap_or_else(|_| "[]".into()))
}

/// Backs `fs.readDir(path)`. Returns a JSON array of `{ name, isDirectory, isFile }` entries as a
/// string, which the JS wrapper must `JSON.parse`, matching the pattern other ops in this file
/// use for structured returns.
#[op2]
#[string]
pub fn op_fs_read_dir(#[string] path: String) -> Result<String, ScriptOpError> {
    read_dir_impl(&path)
}

fn exists_impl(path: &str) -> bool {
    std::path::Path::new(path).exists()
}

/// Backs `fs.exists(path)`. Returns true when the path exists (following symlinks), false
/// otherwise.
#[op2(fast)]
pub fn op_fs_exists(#[string] path: String) -> bool {
    exists_impl(&path)
}

fn mkdir_impl(path: &str, recursive: bool) -> Result<(), ScriptOpError> {
    let result = if recursive {
        fs::create_dir_all(path)
    } else {
        fs::create_dir(path)
    };
    result.map_err(|e| io_err(e, path))
}

/// Backs `fs.mkdir(path, recursive)`. `recursive` is a required bool that the JS wrapper must
/// default to `false` when the caller omits it, since the op has no way to express an omitted
/// argument.
#[op2(fast)]
pub fn op_fs_mkdir(#[string] path: String, recursive: bool) -> Result<(), ScriptOpError> {
    mkdir_impl(&path, recursive)
}

fn remove_impl(path: &str, recursive: bool) -> Result<(), ScriptOpError> {
    // Use symlink_metadata (lstat semantics) so a symlink is treated as itself rather than
    // followed to its target, matching Node's rmSync behavior.
    let meta = fs::symlink_metadata(path).map_err(|e| io_err(e, path))?;
    let result = if meta.is_dir() {
        if recursive {
            fs::remove_dir_all(path)
        } else {
            fs::remove_dir(path)
        }
    } else {
        fs::remove_file(path)
    };
    result.map_err(|e| io_err(e, path))
}

/// Backs `fs.remove(path, recursive)`. `recursive` is a required bool that the JS wrapper must
/// default to `false` when the caller omits it. Uses symlink-aware (`lstat`-style) semantics like
/// Node's `rmSync`, so a symlink is unlinked itself rather than followed to its target.
#[op2(fast)]
pub fn op_fs_remove(#[string] path: String, recursive: bool) -> Result<(), ScriptOpError> {
    remove_impl(&path, recursive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn write_then_read_utf8_roundtrips() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("hello.txt").to_string_lossy().to_string();
        write_file_impl(&path, "hello world", "utf8").expect("write");
        let content = read_file_impl(&path, "utf8").expect("read");
        assert_eq!(content, "hello world");
    }

    #[test]
    fn write_then_read_base64_roundtrips() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("bin.dat").to_string_lossy().to_string();
        // "hi" base64-encoded is "aGk=".
        write_file_impl(&path, "aGk=", "base64").expect("write");
        let content = read_file_impl(&path, "base64").expect("read");
        assert_eq!(content, "aGk=");
    }

    #[test]
    fn read_file_missing_path_returns_error() {
        let err = read_file_impl("/nonexistent/path/does/not/exist.txt", "utf8");
        assert!(err.is_err());
    }

    #[test]
    fn read_dir_lists_files_and_subdirectories() {
        let dir = TempDir::new().expect("tempdir");
        fs::write(dir.path().join("a.txt"), "a").expect("write a");
        fs::create_dir(dir.path().join("sub")).expect("mkdir sub");
        let json = read_dir_impl(&dir.path().to_string_lossy()).expect("read_dir");
        let entries: serde_json::Value = serde_json::from_str(&json).expect("parse");
        let names: Vec<&str> = entries
            .as_array()
            .expect("array")
            .iter()
            .map(|e| e["name"].as_str().expect("name"))
            .collect();
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&"sub"));
        let sub_entry = entries
            .as_array()
            .expect("array")
            .iter()
            .find(|e| e["name"] == "sub")
            .expect("sub entry");
        assert_eq!(sub_entry["isDirectory"], true);
        let file_entry = entries
            .as_array()
            .expect("array")
            .iter()
            .find(|e| e["name"] == "a.txt")
            .expect("a.txt entry");
        assert_eq!(file_entry["isFile"], true);
    }

    #[test]
    fn exists_true_for_present_path_false_for_absent() {
        let dir = TempDir::new().expect("tempdir");
        let present = dir.path().join("here.txt");
        fs::write(&present, "x").expect("write");
        assert!(exists_impl(&present.to_string_lossy()));
        assert!(!exists_impl(&dir.path().join("nope.txt").to_string_lossy()));
    }

    #[test]
    fn mkdir_non_recursive_fails_when_parent_missing() {
        let dir = TempDir::new().expect("tempdir");
        let nested = dir.path().join("a").join("b");
        assert!(mkdir_impl(&nested.to_string_lossy(), false).is_err());
    }

    #[test]
    fn mkdir_recursive_creates_missing_parents() {
        let dir = TempDir::new().expect("tempdir");
        let nested = dir.path().join("a").join("b");
        mkdir_impl(&nested.to_string_lossy(), true).expect("mkdir recursive");
        assert!(nested.exists());
    }

    #[test]
    fn remove_deletes_a_file() {
        let dir = TempDir::new().expect("tempdir");
        let file = dir.path().join("gone.txt");
        fs::write(&file, "x").expect("write");
        remove_impl(&file.to_string_lossy(), false).expect("remove");
        assert!(!file.exists());
    }

    #[test]
    fn remove_non_recursive_fails_on_non_empty_directory() {
        let dir = TempDir::new().expect("tempdir");
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).expect("mkdir");
        fs::write(sub.join("inner.txt"), "x").expect("write inner");
        assert!(remove_impl(&sub.to_string_lossy(), false).is_err());
        assert!(
            sub.exists(),
            "non-empty dir must survive a non-recursive remove attempt"
        );
    }

    #[test]
    fn remove_recursive_deletes_non_empty_directory() {
        let dir = TempDir::new().expect("tempdir");
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).expect("mkdir");
        fs::write(sub.join("inner.txt"), "x").expect("write inner");
        remove_impl(&sub.to_string_lossy(), true).expect("remove recursive");
        assert!(!sub.exists());
    }

    #[test]
    #[cfg(unix)]
    fn remove_deletes_a_symlink_without_following_it() {
        let dir = TempDir::new().expect("tempdir");
        let target = dir.path().join("target.txt");
        fs::write(&target, "x").expect("write target");
        let link = dir.path().join("link.txt");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        remove_impl(&link.to_string_lossy(), false).expect("remove symlink");

        assert!(!link.exists(), "symlink itself must be gone");
        assert!(
            target.exists(),
            "remove must not follow the symlink and delete its target"
        );
    }
}
