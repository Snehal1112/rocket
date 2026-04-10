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
    pub document_path: Option<PathBuf>,
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
