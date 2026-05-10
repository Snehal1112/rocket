//! Persistence records for `KeyValueEntry`, `RequestSignatureSnapshot`,
//! and `ContractSnapshot`. Owns `serde(default)` for legacy v0.6.x fields
//! (`headers`, `query_params`, `auth_detail`, etc.) so old on-disk snapshots
//! deserialise without error.

use chrono::{DateTime, Utc};
use rocket_collection::contract::snapshot::{
    ContractSnapshot, KeyValueEntry, RequestSignatureSnapshot,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ulid::Ulid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyValueEntryRecord {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RequestSignatureSnapshotRecord {
    pub request_path: PathBuf,
    pub method: String,
    pub url_pattern: String,
    #[serde(default)]
    pub headers: Vec<KeyValueEntryRecord>,
    #[serde(default)]
    pub query_params: Vec<KeyValueEntryRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_content: Option<String>,
    #[serde(default)]
    pub form_fields: Vec<KeyValueEntryRecord>,
    pub auth_type: String,
    #[serde(default)]
    pub auth_detail: String,
    pub captured_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub query_param_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub header_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body_field_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractSnapshotRecord {
    pub contract_id: Ulid,
    pub entries: Vec<RequestSignatureSnapshotRecord>,
}

impl From<&KeyValueEntry> for KeyValueEntryRecord {
    fn from(k: &KeyValueEntry) -> Self {
        Self { key: k.key.clone(), value: k.value.clone() }
    }
}
impl From<KeyValueEntryRecord> for KeyValueEntry {
    fn from(r: KeyValueEntryRecord) -> Self {
        Self { key: r.key, value: r.value }
    }
}

impl From<&RequestSignatureSnapshot> for RequestSignatureSnapshotRecord {
    fn from(s: &RequestSignatureSnapshot) -> Self {
        Self {
            request_path: s.request_path.clone(),
            method: s.method.clone(),
            url_pattern: s.url_pattern.clone(),
            headers: s.headers.iter().map(Into::into).collect(),
            query_params: s.query_params.iter().map(Into::into).collect(),
            body_content: s.body_content.clone(),
            form_fields: s.form_fields.iter().map(Into::into).collect(),
            auth_type: s.auth_type.clone(),
            auth_detail: s.auth_detail.clone(),
            captured_at: s.captured_at,
            query_param_keys: s.query_param_keys.clone(),
            header_keys: s.header_keys.clone(),
            body_field_keys: s.body_field_keys.clone(),
        }
    }
}
impl From<RequestSignatureSnapshotRecord> for RequestSignatureSnapshot {
    fn from(r: RequestSignatureSnapshotRecord) -> Self {
        Self {
            request_path: r.request_path,
            method: r.method,
            url_pattern: r.url_pattern,
            headers: r.headers.into_iter().map(Into::into).collect(),
            query_params: r.query_params.into_iter().map(Into::into).collect(),
            body_content: r.body_content,
            form_fields: r.form_fields.into_iter().map(Into::into).collect(),
            auth_type: r.auth_type,
            auth_detail: r.auth_detail,
            captured_at: r.captured_at,
            query_param_keys: r.query_param_keys,
            header_keys: r.header_keys,
            body_field_keys: r.body_field_keys,
        }
    }
}

impl From<&ContractSnapshot> for ContractSnapshotRecord {
    fn from(c: &ContractSnapshot) -> Self {
        Self {
            contract_id: c.contract_id,
            entries: c.entries.iter().map(Into::into).collect(),
        }
    }
}

impl From<ContractSnapshotRecord> for ContractSnapshot {
    fn from(r: ContractSnapshotRecord) -> Self {
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
    fn old_format_snapshot_deserialises_via_record() {
        let yaml = r#"contractId: 01KQRNJTCG9AA2FR6AV9N1H3QA
entries:
- requestPath: get-users.yml
  method: GET
  urlPattern: https://api.example.com/users
  queryParamKeys: []
  headerKeys:
  - Authorization
  bodyFieldKeys: []
  authType: none
  capturedAt: 2026-05-04T04:57:42.033432603Z
"#;
        let r: ContractSnapshotRecord = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(r.entries.len(), 1);
        let e = &r.entries[0];
        assert!(e.headers.is_empty());
        assert!(e.query_params.is_empty());
        assert!(e.auth_detail.is_empty());
        assert_eq!(e.header_keys, vec!["Authorization".to_string()]);
    }

    #[test]
    fn snapshot_record_roundtrip_camel_case_yaml() {
        let r = RequestSignatureSnapshotRecord {
            request_path: PathBuf::from("a.yml"),
            method: "GET".into(),
            url_pattern: "/x".into(),
            headers: vec![KeyValueEntryRecord { key: "K".into(), value: "V".into() }],
            query_params: vec![],
            body_content: None,
            form_fields: vec![],
            auth_type: "none".into(),
            auth_detail: String::new(),
            captured_at: "2026-05-08T00:00:00Z".parse().unwrap(),
            query_param_keys: vec![],
            header_keys: vec![],
            body_field_keys: vec![],
        };
        let yaml = serde_yaml::to_string(&r).unwrap();
        assert!(yaml.contains("requestPath:"));
        assert!(yaml.contains("urlPattern:"));
        assert!(yaml.contains("authType:"));
        let back: RequestSignatureSnapshotRecord = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(r, back);
    }
}
