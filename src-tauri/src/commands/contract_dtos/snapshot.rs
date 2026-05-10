//! IPC DTOs for `KeyValueEntry` and `RequestSignatureSnapshot`.

use chrono::{DateTime, Utc};
use rocket_collection::contract::snapshot::{KeyValueEntry, RequestSignatureSnapshot};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyValueEntryDto {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RequestSignatureSnapshotDto {
    pub request_path: PathBuf,
    pub method: String,
    pub url_pattern: String,
    #[serde(default)]
    pub headers: Vec<KeyValueEntryDto>,
    #[serde(default)]
    pub query_params: Vec<KeyValueEntryDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_content: Option<String>,
    #[serde(default)]
    pub form_fields: Vec<KeyValueEntryDto>,
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

impl From<&KeyValueEntry> for KeyValueEntryDto {
    fn from(k: &KeyValueEntry) -> Self {
        Self { key: k.key.clone(), value: k.value.clone() }
    }
}
impl From<KeyValueEntryDto> for KeyValueEntry {
    fn from(d: KeyValueEntryDto) -> Self {
        Self { key: d.key, value: d.value }
    }
}

impl From<&RequestSignatureSnapshot> for RequestSignatureSnapshotDto {
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
impl From<RequestSignatureSnapshotDto> for RequestSignatureSnapshot {
    fn from(d: RequestSignatureSnapshotDto) -> Self {
        Self {
            request_path: d.request_path,
            method: d.method,
            url_pattern: d.url_pattern,
            headers: d.headers.into_iter().map(Into::into).collect(),
            query_params: d.query_params.into_iter().map(Into::into).collect(),
            body_content: d.body_content,
            form_fields: d.form_fields.into_iter().map(Into::into).collect(),
            auth_type: d.auth_type,
            auth_detail: d.auth_detail,
            captured_at: d.captured_at,
            query_param_keys: d.query_param_keys,
            header_keys: d.header_keys,
            body_field_keys: d.body_field_keys,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_dto_json_uses_camel_case_field_names() {
        let d = RequestSignatureSnapshotDto {
            request_path: PathBuf::from("a.yml"),
            method: "GET".into(),
            url_pattern: "/x".into(),
            headers: vec![],
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
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"requestPath\":"));
        assert!(json.contains("\"urlPattern\":"));
        assert!(json.contains("\"authType\":"));
        let back: RequestSignatureSnapshotDto = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
