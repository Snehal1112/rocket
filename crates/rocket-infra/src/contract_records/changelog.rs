//! Persistence records for `ChangelogEntry` and `ContractChangelog`.

use chrono::{DateTime, Utc};
use rocket_collection::contract::changelog::{ChangeType, ChangelogEntry, ContractChangelog};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ulid::Ulid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeTypeRecord {
    Changed,
    Added,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChangelogEntryRecord {
    pub timestamp: DateTime<Utc>,
    pub request_path: PathBuf,
    pub field: String,
    pub change_type: ChangeTypeRecord,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_value: Option<String>,
    #[serde(default)]
    pub is_breaking: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractChangelogRecord {
    pub contract_id: Ulid,
    pub entries: Vec<ChangelogEntryRecord>,
}

impl From<&ChangeType> for ChangeTypeRecord {
    fn from(c: &ChangeType) -> Self {
        match c {
            ChangeType::Changed => Self::Changed,
            ChangeType::Added => Self::Added,
            ChangeType::Removed => Self::Removed,
        }
    }
}

impl From<ChangeTypeRecord> for ChangeType {
    fn from(r: ChangeTypeRecord) -> Self {
        match r {
            ChangeTypeRecord::Changed => Self::Changed,
            ChangeTypeRecord::Added => Self::Added,
            ChangeTypeRecord::Removed => Self::Removed,
        }
    }
}

impl From<&ChangelogEntry> for ChangelogEntryRecord {
    fn from(e: &ChangelogEntry) -> Self {
        Self {
            timestamp: e.timestamp,
            request_path: e.request_path.clone(),
            field: e.field.clone(),
            change_type: (&e.change_type).into(),
            old_value: e.old_value.clone(),
            new_value: e.new_value.clone(),
            is_breaking: e.is_breaking,
        }
    }
}

impl From<ChangelogEntryRecord> for ChangelogEntry {
    fn from(r: ChangelogEntryRecord) -> Self {
        Self {
            timestamp: r.timestamp,
            request_path: r.request_path,
            field: r.field,
            change_type: r.change_type.into(),
            old_value: r.old_value,
            new_value: r.new_value,
            is_breaking: r.is_breaking,
        }
    }
}

impl From<&ContractChangelog> for ContractChangelogRecord {
    fn from(c: &ContractChangelog) -> Self {
        Self {
            contract_id: c.contract_id,
            entries: c.entries.iter().map(Into::into).collect(),
        }
    }
}

impl From<ContractChangelogRecord> for ContractChangelog {
    fn from(r: ContractChangelogRecord) -> Self {
        Self {
            contract_id: r.contract_id,
            entries: r.entries.into_iter().map(Into::into).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_changelog_entry_without_is_breaking_defaults_false_via_record() {
        let yaml = r#"
timestamp: "2026-05-07T10:00:00Z"
requestPath: requests/payments.yml
field: method
changeType: changed
oldValue: GET
newValue: POST
"#;
        let r: ChangelogEntryRecord = serde_yaml::from_str(yaml).unwrap();
        assert!(!r.is_breaking);
    }

    #[test]
    fn changelog_entry_record_yaml_roundtrip_camel_case() {
        let r = ChangelogEntryRecord {
            timestamp: "2026-05-07T10:00:00Z".parse().unwrap(),
            request_path: PathBuf::from("a.yml"),
            field: "method".into(),
            change_type: ChangeTypeRecord::Changed,
            old_value: Some("GET".into()),
            new_value: Some("POST".into()),
            is_breaking: true,
        };
        let yaml = serde_yaml::to_string(&r).unwrap();
        assert!(yaml.contains("requestPath:"));
        assert!(yaml.contains("isBreaking:"));
        let back: ChangelogEntryRecord = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn domain_changelog_record_roundtrip() {
        let domain = ContractChangelog::new(Ulid::new());
        let r: ContractChangelogRecord = (&domain).into();
        let back: ContractChangelog = r.into();
        // ContractChangelog has no PartialEq; compare field-by-field.
        assert_eq!(domain.contract_id, back.contract_id);
        assert_eq!(domain.entries.len(), back.entries.len());
    }
}
