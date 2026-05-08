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

// camelCase is intentional: serves as both YAML persistence and Tauri IPC wire type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangelogEntry {
    pub timestamp: DateTime<Utc>,
    pub request_path: PathBuf,
    pub field: String,
    pub change_type: ChangeType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_value: Option<String>,
    /// True if this change violates the contract's breaking-change policy.
    /// Defaults to false so old changelog entries deserialise correctly.
    #[serde(default)]
    pub is_breaking: bool,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changelog_entry_is_breaking_defaults_false() {
        let yaml = r#"
timestamp: "2026-05-07T10:00:00Z"
requestPath: requests/payments.yml
field: method
changeType: changed
oldValue: GET
newValue: POST
"#;
        let entry: ChangelogEntry = serde_yaml::from_str(yaml).unwrap();
        assert!(!entry.is_breaking, "is_breaking must default to false for old entries");
    }
}
