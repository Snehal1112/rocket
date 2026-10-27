//! Helpers for the `<collection>/.rocket` folder.

use std::path::Path;

use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;

const GITIGNORE_FILE: &str = ".gitignore";
/// Files in the folder that are local state and must not be committed.
const IGNORED_NAMES: [&str; 7] = [
    "openapi-baseline.json",
    "openapi-reset-backups.json",
    "openapi-applied.spec",
    "openapi-pending.spec",
    "openapi-local.json",
    "apply-journal.json",
    "staging/",
];

/// Makes sure `<collection>/.rocket/.gitignore` lists the local state files. A missing file is
/// created. An existing one only gets the missing lines appended, and the user's lines stay.
pub fn ensure_gitignore(collection_root: &Path) -> DomainResult<()> {
    let path = collection_root.join(".rocket").join(GITIGNORE_FILE);
    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            return Err(DomainError::Internal(format!(
                "{GITIGNORE_FILE} could not be read: {e}"
            )))
        }
    };
    let missing: Vec<&str> = IGNORED_NAMES
        .iter()
        .copied()
        .filter(|name| !existing.lines().any(|line| line.trim() == *name))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let mut text = existing;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    for name in missing {
        text.push_str(name);
        text.push('\n');
    }
    atomic_write(&path, text.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn the_gitignore_lists_the_applied_copy() {
        let dir = tempdir().expect("tempdir");
        ensure_gitignore(dir.path()).expect("ensure");
        let text = std::fs::read_to_string(dir.path().join(".rocket/.gitignore")).expect("read");
        assert!(text.lines().any(|l| l == "openapi-applied.spec"), "{text}");
    }

    #[test]
    fn the_gitignore_lists_the_backups_file() {
        let dir = tempdir().expect("tempdir");
        ensure_gitignore(dir.path()).expect("ensure");
        let text = std::fs::read_to_string(dir.path().join(".rocket/.gitignore")).expect("read");
        assert!(
            text.lines().any(|l| l == "openapi-reset-backups.json"),
            "{text}"
        );
    }
}
