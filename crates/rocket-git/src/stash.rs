use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashEntry {
    pub index: usize,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub branch: String,
    /// Number of files touched by the stash (mirrors `git stash show --stat`).
    pub files_changed: usize,
    /// Lines added across all changed files.
    pub insertions: usize,
    /// Lines removed across all changed files.
    pub deletions: usize,
    /// Paths of every file touched, relative to the repo root.
    pub changed_files: Vec<String>,
}
