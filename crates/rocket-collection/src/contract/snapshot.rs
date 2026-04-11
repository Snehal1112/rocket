use chrono::{DateTime, Utc};
use rocket_shared::types::{Auth, Body, BodyMode, HttpMethod};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ulid::Ulid;

use crate::request::Request;

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

impl RequestSignatureSnapshot {
    /// Build a signature snapshot from a request and its collection-relative path.
    ///
    /// The `Request` struct itself has no authoritative path, so the caller
    /// supplies it explicitly. Keys are taken as-is (no sorting) — ordering is
    /// irrelevant for diffing because `diff_signature` uses set-style checks.
    pub fn from_request(path: impl AsRef<Path>, request: &Request) -> Self {
        Self {
            request_path: path.as_ref().to_path_buf(),
            method: http_method_name(&request.method),
            url_pattern: request.url.clone(),
            query_param_keys: request.query_params.iter().map(|q| q.key.clone()).collect(),
            header_keys: request.headers.iter().map(|h| h.key.clone()).collect(),
            body_field_keys: extract_body_keys(&request.body),
            auth_type: auth_type_name(&request.auth),
            captured_at: Utc::now(),
        }
    }
}

fn http_method_name(m: &HttpMethod) -> String {
    match m {
        HttpMethod::Get => "GET",
        HttpMethod::Post => "POST",
        HttpMethod::Put => "PUT",
        HttpMethod::Patch => "PATCH",
        HttpMethod::Delete => "DELETE",
        HttpMethod::Options => "OPTIONS",
        HttpMethod::Head => "HEAD",
    }
    .to_string()
}

fn auth_type_name(auth: &Auth) -> String {
    match auth {
        Auth::None => "none",
        Auth::Basic { .. } => "basic",
        Auth::Bearer { .. } => "bearer",
        Auth::ApiKey { .. } => "api-key",
        Auth::OAuth2(_) => "oauth2",
        Auth::AwsSigV4 { .. } => "aws-sig-v4",
        Auth::Inherit => "inherit",
        Auth::Wsse { .. } => "wsse",
        Auth::Digest { .. } => "digest",
        Auth::Ntlm { .. } => "ntlm",
    }
    .to_string()
}

fn extract_body_keys(body: &Option<Body>) -> Vec<String> {
    let Some(body) = body else {
        return vec![];
    };
    match body.mode {
        BodyMode::FormUrlEncoded | BodyMode::FormData => body
            .form_data
            .as_ref()
            .map(|entries| entries.iter().map(|e| e.key.clone()).collect())
            .unwrap_or_default(),
        BodyMode::Json => body
            .content
            .as_deref()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .and_then(|v| v.as_object().cloned())
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default(),
        BodyMode::None
        | BodyMode::Xml
        | BodyMode::Text
        | BodyMode::Sparql
        | BodyMode::Binary => vec![],
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::types::{Body, BodyMode, HttpMethod, QueryParam};

    #[test]
    fn from_request_captures_method_url_and_keys() {
        let mut req = Request::new("Get Users", HttpMethod::Get, "https://api.example.com/users");
        req = req.with_header("X-Trace-Id", "123");
        req.query_params.push(QueryParam {
            key: "page".into(),
            value: "1".into(),
            enabled: true,
            description: None,
        });

        let snap = RequestSignatureSnapshot::from_request("users/get-users.yml", &req);

        assert_eq!(snap.request_path, PathBuf::from("users/get-users.yml"));
        assert_eq!(snap.method, "GET");
        assert_eq!(snap.url_pattern, "https://api.example.com/users");
        assert_eq!(snap.header_keys, vec!["X-Trace-Id".to_string()]);
        assert_eq!(snap.query_param_keys, vec!["page".to_string()]);
        assert_eq!(snap.auth_type, "none");
        assert!(snap.body_field_keys.is_empty());
    }

    #[test]
    fn from_request_extracts_json_body_keys() {
        let req = Request::new("Create", HttpMethod::Post, "/users").with_body(Body {
            mode: BodyMode::Json,
            content: Some(r#"{"name":"Ada","email":"a@b.com"}"#.into()),
            form_data: None,
            file_path: None,
        });

        let snap = RequestSignatureSnapshot::from_request("create.yml", &req);

        let mut keys = snap.body_field_keys.clone();
        keys.sort();
        assert_eq!(keys, vec!["email".to_string(), "name".to_string()]);
    }
}
