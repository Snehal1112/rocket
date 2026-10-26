//! Helpers for the `<collection>/.rocket` folder.

use std::path::Path;

use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;

const GITIGNORE_FILE: &str = ".gitignore";
/// Files in the folder that are local state and must not be committed.
const IGNORED_NAMES: [&str; 2] = ["openapi-baseline.json", "openapi-pending.spec"];

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
