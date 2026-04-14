use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ulid::Ulid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    pub id: Ulid,
    pub title: String,
    pub provider: String,
    pub consumer: String,
    pub project: String,
    pub version: String,
    pub effective_date: NaiveDate,
    pub expiry_date: Option<NaiveDate>,
    /// Relative paths to attachment files stored inside the collection.
    /// Stored under `.rocket/contracts/attachments/<id>/`.
    /// `default` handles old YAML files that pre-date this field.
    #[serde(default)]
    pub document_paths: Vec<PathBuf>,
    pub enforcement_mode: ContractEnforcementMode,
    pub scope: ContractScope,
}

impl Contract {
    pub fn status(&self) -> ContractStatus {
        let today = Utc::now().date_naive();
        match self.expiry_date {
            None => ContractStatus::Active,
            Some(exp) if exp < today => ContractStatus::Expired,
            Some(exp) if (exp - today).num_days() <= 30 => ContractStatus::ExpiringIn30Days,
            _ => ContractStatus::Active,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ContractStatus {
    Active,
    ExpiringIn30Days,
    Expired,
}

/// Model B extension seam.
/// Only `Informational` is reachable from UI in this sprint.
/// `Warn` and `Block` are defined now so Model B requires zero domain changes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContractEnforcementMode {
    #[default]
    Informational,
    Warn,
    Block,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContractScope {
    Collection,
    Folder { rel_path: PathBuf },
    Request { rel_path: PathBuf },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_scope_folder_serializes_rel_path_as_snake_case() {
        let scope = ContractScope::Folder { rel_path: PathBuf::from("auth/login.yml") };
        let yaml = serde_yaml::to_string(&scope).unwrap();
        // rel_path must stay snake_case — the frontend wire type uses rel_path.
        assert!(yaml.contains("rel_path:"), "expected rel_path in:\n{yaml}");
        assert!(!yaml.contains("relPath:"), "camelCase relPath must not appear in:\n{yaml}");
    }

    #[test]
    fn contract_scope_request_serializes_rel_path_as_snake_case() {
        let scope = ContractScope::Request { rel_path: PathBuf::from("users/get.yml") };
        let yaml = serde_yaml::to_string(&scope).unwrap();
        assert!(yaml.contains("rel_path:"), "expected rel_path in:\n{yaml}");
        assert!(!yaml.contains("relPath:"), "camelCase relPath must not appear in:\n{yaml}");
    }

    #[test]
    fn contract_scope_folder_roundtrips() {
        let scope = ContractScope::Folder { rel_path: PathBuf::from("auth/login.yml") };
        let yaml = serde_yaml::to_string(&scope).unwrap();
        let back: ContractScope = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(scope, back);
    }

    #[test]
    fn contract_scope_collection_roundtrips() {
        let scope = ContractScope::Collection;
        let yaml = serde_yaml::to_string(&scope).unwrap();
        let back: ContractScope = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(scope, back);
    }
}
