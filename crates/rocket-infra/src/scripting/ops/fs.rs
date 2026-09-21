use deno_core::op2;
use std::fs;
use crate::scripting::ops::ScriptOpError;

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

#[op2]
#[string]
pub fn op_fs_read_file(#[string] path: String, #[string] encoding: String) -> Result<String, ScriptOpError> {
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

#[op2(fast)]
pub fn op_fs_write_file(#[string] path: String, #[string] content: String, #[string] encoding: String) -> Result<(), ScriptOpError> {
    write_file_impl(&path, &content, &encoding)
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
}
