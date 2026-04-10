use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ulid::Ulid;

/// Shape of one request at the moment a contract is signed.
/// Rebuilt on every save and diffed against this baseline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RequestSignatureSnapshot {
    pub request_path: PathBuf,
    pub method: String,
    pub url_pattern: String,
    pub query_param_keys: Vec<String>,
    pub header_keys: Vec<String>,
    pub body_field_keys: Vec<String>,
    pub auth_type: String,
    pub captured_at: DateTime<Utc>,
}

/// All snapshots for one contract (one entry per covered request).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractSnapshot {
    pub contract_id: Ulid,
    pub entries: Vec<RequestSignatureSnapshot>,
}

impl ContractSnapshot {
    pub fn new(contract_id: Ulid) -> Self {
        Self { contract_id, entries: Vec::new() }
    }

    pub fn get(&self, request_path: &std::path::Path) -> Option<&RequestSignatureSnapshot> {
        self.entries.iter().find(|e| e.request_path == request_path)
    }

    pub fn upsert(&mut self, snap: RequestSignatureSnapshot) {
        if let Some(existing) = self.entries.iter_mut().find(|e| e.request_path == snap.request_path) {
            *existing = snap;
        } else {
            self.entries.push(snap);
        }
    }
}
