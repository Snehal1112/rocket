use chrono::{DateTime, Utc};
use std::path::PathBuf;
use ulid::Ulid;

#[derive(Debug, Clone, PartialEq)]
pub enum ChangeType {
    Changed,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChangelogEntry {
    pub timestamp: DateTime<Utc>,
    pub request_path: PathBuf,
    pub field: String,
    pub change_type: ChangeType,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    /// True if this change violates the contract's breaking-change policy.
    /// Defaults to false so old changelog entries deserialise correctly.
    pub is_breaking: bool,
    /// HTTP method (e.g. "POST") at the time of the change. None for entries
    /// captured before this field was added.
    pub request_method: Option<String>,
    /// URL pattern (e.g. "/v2/email/send") at the time of the change. None for
    /// entries captured before this field was added.
    pub http_path: Option<String>,
    /// OS username of the person who saved the request. None for entries
    /// captured before this field was added or when the env var is absent.
    pub author: Option<String>,
}

/// Append-only audit log for one contract.
#[derive(Debug, Clone, PartialEq)]
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
