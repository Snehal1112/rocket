use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ulid::Ulid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    Changed,
    Added,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangelogEntry {
    pub timestamp: DateTime<Utc>,
    pub request_path: PathBuf,
    pub field: String,
    pub change_type: ChangeType,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

/// Append-only audit log for one contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractChangelog {
    pub contract_id: Ulid,
    pub entries: Vec<ChangelogEntry>,
}

impl ContractChangelog {
    pub fn new(contract_id: Ulid) -> Self {
        Self { contract_id, entries: Vec::new() }
    }

    /// Appends entries — never removes. Returns count added.
    pub fn append(&mut self, new_entries: Vec<ChangelogEntry>) -> usize {
        let count = new_entries.len();
        self.entries.extend(new_entries);
        count
    }
}
