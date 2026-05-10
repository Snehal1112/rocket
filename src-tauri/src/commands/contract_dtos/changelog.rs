//! IPC DTOs for `ChangelogEntry` and `ContractChangelog`.

use chrono::{DateTime, Utc};
use rocket_collection::contract::changelog::{ChangeType, ChangelogEntry, ContractChangelog};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ulid::Ulid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeTypeDto {
    Changed,
    Added,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChangelogEntryDto {
    pub timestamp: DateTime<Utc>,
    pub request_path: PathBuf,
    pub field: String,
    pub change_type: ChangeTypeDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_value: Option<String>,
    #[serde(default)]
    pub is_breaking: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractChangelogDto {
    pub contract_id: Ulid,
    pub entries: Vec<ChangelogEntryDto>,
}

impl From<&ChangeType> for ChangeTypeDto {
    fn from(c: &ChangeType) -> Self {
        match c {
            ChangeType::Changed => Self::Changed,
            ChangeType::Added => Self::Added,
            ChangeType::Removed => Self::Removed,
        }
    }
}
impl From<ChangeTypeDto> for ChangeType {
    fn from(d: ChangeTypeDto) -> Self {
        match d {
            ChangeTypeDto::Changed => Self::Changed,
            ChangeTypeDto::Added => Self::Added,
            ChangeTypeDto::Removed => Self::Removed,
        }
    }
}

impl From<&ChangelogEntry> for ChangelogEntryDto {
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
impl From<ChangelogEntryDto> for ChangelogEntry {
    fn from(d: ChangelogEntryDto) -> Self {
        Self {
            timestamp: d.timestamp,
            request_path: d.request_path,
            field: d.field,
            change_type: d.change_type.into(),
            old_value: d.old_value,
            new_value: d.new_value,
            is_breaking: d.is_breaking,
        }
    }
}

impl From<&ContractChangelog> for ContractChangelogDto {
    fn from(c: &ContractChangelog) -> Self {
        Self {
            contract_id: c.contract_id,
            entries: c.entries.iter().map(Into::into).collect(),
        }
    }
}
impl From<ContractChangelogDto> for ContractChangelog {
    fn from(d: ContractChangelogDto) -> Self {
        Self {
            contract_id: d.contract_id,
            entries: d.entries.into_iter().map(Into::into).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changelog_entry_dto_json_camel_case() {
        let d = ChangelogEntryDto {
            timestamp: "2026-05-07T10:00:00Z".parse().unwrap(),
            request_path: PathBuf::from("a.yml"),
            field: "method".into(),
            change_type: ChangeTypeDto::Changed,
            old_value: Some("GET".into()),
            new_value: Some("POST".into()),
            is_breaking: false,
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"requestPath\":"));
        assert!(json.contains("\"isBreaking\":"));
        let back: ChangelogEntryDto = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn changelog_dto_domain_roundtrip() {
        let domain = ContractChangelog::new(Ulid::new());
        let dto: ContractChangelogDto = (&domain).into();
        let back: ContractChangelog = dto.into();
        assert_eq!(domain.contract_id, back.contract_id);
    }
}
