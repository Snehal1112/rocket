use chrono::{DateTime, Utc};
use rocket_shared::types::{Auth, Body, BodyMode, HttpMethod};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ulid::Ulid;

use crate::request::Request;

/// A key+value pair used for headers, query params, and form fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyValueEntry {
    pub key: String,
    pub value: String,
}

/// Shape of one request at the moment a contract is signed.
/// camelCase is intentional: serves as both YAML persistence and Tauri IPC wire type.
/// Rebuilt on every save and diffed against this baseline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RequestSignatureSnapshot {
    pub request_path: PathBuf,
    pub method: String,
    pub url_pattern: String,
    /// Full key+value pairs for enabled headers.
    #[serde(default)]
    pub headers: Vec<KeyValueEntry>,
    /// Full key+value pairs for enabled query params.
    #[serde(default)]
    pub query_params: Vec<KeyValueEntry>,
    /// Raw body string for text-like body modes (Json, Xml, Text, Sparql, Binary).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_content: Option<String>,
    /// Key+value pairs for enabled form fields (FormData/FormUrlEncoded modes).
    #[serde(default)]
    pub form_fields: Vec<KeyValueEntry>,
    pub auth_type: String,
    /// Auth credential summary — does not include the auth type itself.
    #[serde(default)]
    pub auth_detail: String,
    pub captured_at: DateTime<Utc>,
    // Legacy key-list fields from v0.6.x snapshots. Kept with serde(default) so
    // old on-disk files deserialise without error. No longer written by from_request.
    // The first save after upgrade will emit "added" entries for all current fields
    // relative to these empty lists — delete old -snapshot.yml files to reset.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub query_param_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub header_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body_field_keys: Vec<String>,
}

impl RequestSignatureSnapshot {
    /// Build a signature snapshot from a request and its collection-relative path.
    ///
    /// The `Request` struct itself has no authoritative path, so the caller
    /// supplies it explicitly.
    pub fn from_request(path: impl AsRef<Path>, request: &Request) -> Self {
        Self {
            request_path: path.as_ref().to_path_buf(),
            method: http_method_name(&request.method),
            url_pattern: request.url.clone(),
            headers: request
                .headers
                .iter()
                .filter(|h| h.enabled)
                .map(|h| KeyValueEntry { key: h.key.clone(), value: h.value.clone() })
                .collect(),
            query_params: request
                .query_params
                .iter()
                .filter(|q| q.enabled)
                .map(|q| KeyValueEntry { key: q.key.clone(), value: q.value.clone() })
                .collect(),
            body_content: extract_body_content(&request.body),
            form_fields: extract_form_fields(&request.body),
            auth_type: auth_type_name(&request.auth),
            auth_detail: auth_detail(&request.auth),
            captured_at: Utc::now(),
            // Legacy fields are empty; old files still deserialize via #[serde(default)].
            query_param_keys: vec![],
            header_keys: vec![],
            body_field_keys: vec![],
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

/// Returns a credential summary string for the given auth variant.
///
/// Only captures distinguishing credential data — the auth type itself is tracked separately.
fn auth_detail(auth: &Auth) -> String {
    match auth {
        Auth::None | Auth::Inherit => String::new(),
        Auth::OAuth2(flow) => {
            use rocket_shared::oauth2::OAuth2Flow;
            match flow {
                OAuth2Flow::ClientCredentials { credentials, .. }
                | OAuth2Flow::ResourceOwnerPassword { credentials, .. }
                | OAuth2Flow::AuthorizationCode { credentials, .. } => {
                    credentials.client_id.clone()
                }
                OAuth2Flow::Implicit { client_id, .. } => client_id.clone(),
            }
        }
        Auth::Basic { username, .. }
        | Auth::Wsse { username, .. }
        | Auth::Digest { username, .. }
        | Auth::Ntlm { username, .. } => username.clone(),
        Auth::Bearer { token } => {
            if token.len() > 8 {
                format!("{}…", token.chars().take(8).collect::<String>())
            } else {
                token.clone()
            }
        }
        Auth::ApiKey { key, value, placement } => {
            format!("{}={} ({})", key, value, placement)
        }
        Auth::AwsSigV4 { access_key, region, service, .. } => {
            format!("{}@{}/{}", access_key, region, service)
        }
    }
}

/// Extracts the raw body string for text-like body modes.
fn extract_body_content(body: &Option<Body>) -> Option<String> {
    let Some(body) = body else {
        return None;
    };
    match body.mode {
        BodyMode::Json | BodyMode::Xml | BodyMode::Text | BodyMode::Sparql => body.content.clone(),
        BodyMode::Binary => body.file_path.clone(),
        BodyMode::FormUrlEncoded | BodyMode::FormData | BodyMode::None => None,
    }
}

/// Extracts enabled form field key+value pairs for form body modes.
fn extract_form_fields(body: &Option<Body>) -> Vec<KeyValueEntry> {
    let Some(body) = body else {
        return vec![];
    };
    match body.mode {
        BodyMode::FormUrlEncoded | BodyMode::FormData => body
            .form_data
            .as_ref()
            .map(|entries| {
                entries
                    .iter()
                    .filter(|e| e.enabled)
                    .map(|e| KeyValueEntry { key: e.key.clone(), value: e.value.clone() })
                    .collect()
            })
            .unwrap_or_default(),
        _ => vec![],
    }
}

/// All snapshots for one contract (one entry per covered request).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    use rocket_shared::types::{Body, BodyMode, FormDataEntry, FormDataType, Header, HttpMethod, QueryParam};

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
        // Legacy fields are empty; use the new key+value fields instead.
        assert_eq!(snap.headers[0].key, "X-Trace-Id");
        assert_eq!(snap.query_params[0].key, "page");
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

        // body_content now stores the raw JSON string.
        assert_eq!(snap.body_content, Some(r#"{"name":"Ada","email":"a@b.com"}"#.to_string()));
    }

    #[test]
    fn from_request_captures_header_key_and_value() {
        let req = Request::new("Get", HttpMethod::Get, "/users")
            .with_header("Authorization", "Bearer abc123");
        let snap = RequestSignatureSnapshot::from_request("get.yml", &req);
        assert_eq!(snap.headers.len(), 1);
        assert_eq!(snap.headers[0].key, "Authorization");
        assert_eq!(snap.headers[0].value, "Bearer abc123");
    }

    #[test]
    fn from_request_captures_query_param_key_and_value() {
        let mut req = Request::new("Get", HttpMethod::Get, "/search");
        req.query_params.push(QueryParam { key: "q".into(), value: "hello".into(), enabled: true, description: None });
        let snap = RequestSignatureSnapshot::from_request("search.yml", &req);
        assert_eq!(snap.query_params.len(), 1);
        assert_eq!(snap.query_params[0].key, "q");
        assert_eq!(snap.query_params[0].value, "hello");
    }

    #[test]
    fn from_request_captures_raw_body_content() {
        let req = Request::new("Post", HttpMethod::Post, "/users").with_body(Body {
            mode: BodyMode::Json,
            content: Some(r#"{"name":"Ada"}"#.into()),
            form_data: None,
            file_path: None,
        });
        let snap = RequestSignatureSnapshot::from_request("post.yml", &req);
        assert_eq!(snap.body_content, Some(r#"{"name":"Ada"}"#.to_string()));
        assert!(snap.form_fields.is_empty());
    }

    #[test]
    fn from_request_captures_form_fields_key_and_value() {
        let req = Request::new("Post", HttpMethod::Post, "/form").with_body(Body {
            mode: BodyMode::FormData,
            content: None,
            form_data: Some(vec![FormDataEntry {
                key: "name".into(),
                value: "Ada".into(),
                entry_type: FormDataType::Text,
                enabled: true,
                content_type: None,
                description: None,
            }]),
            file_path: None,
        });
        let snap = RequestSignatureSnapshot::from_request("form.yml", &req);
        assert_eq!(snap.form_fields.len(), 1);
        assert_eq!(snap.form_fields[0].key, "name");
        assert_eq!(snap.form_fields[0].value, "Ada");
    }

    #[test]
    fn from_request_captures_auth_detail_bearer() {
        use rocket_shared::types::Auth;
        let req = Request::new("Get", HttpMethod::Get, "/secure")
            .with_auth(Auth::Bearer { token: "supersecrettoken".into() });
        let snap = RequestSignatureSnapshot::from_request("secure.yml", &req);
        assert_eq!(snap.auth_detail, "supersec…");
    }

    #[test]
    fn from_request_skips_disabled_form_fields() {
        use rocket_shared::types::{FormDataEntry, FormDataType};
        let req = Request::new("Post", HttpMethod::Post, "/form").with_body(Body {
            mode: BodyMode::FormData,
            content: None,
            form_data: Some(vec![
                FormDataEntry { key: "enabled".into(), value: "yes".into(), entry_type: FormDataType::Text, enabled: true, content_type: None, description: None },
                FormDataEntry { key: "disabled".into(), value: "no".into(), entry_type: FormDataType::Text, enabled: false, content_type: None, description: None },
            ]),
            file_path: None,
        });
        let snap = RequestSignatureSnapshot::from_request("form.yml", &req);
        assert_eq!(snap.form_fields.len(), 1);
        assert_eq!(snap.form_fields[0].key, "enabled");
    }

    #[test]
    fn from_request_auth_detail_short_bearer() {
        use rocket_shared::types::Auth;
        let req = Request::new("Get", HttpMethod::Get, "/x")
            .with_auth(Auth::Bearer { token: "short".into() });
        let snap = RequestSignatureSnapshot::from_request("x.yml", &req);
        assert_eq!(snap.auth_detail, "short");
    }

    #[test]
    fn from_request_auth_detail_oauth2_has_client_id() {
        use rocket_shared::oauth2::{OAuth2ClientCredentials, OAuth2Flow};
        use rocket_shared::types::Auth;
        let flow = OAuth2Flow::ClientCredentials {
            access_token_url: "https://auth.example.com/token".into(),
            refresh_token_url: None,
            credentials: OAuth2ClientCredentials {
                client_id: "my-client".into(),
                client_secret: "secret".into(),
                placement: None,
            },
            scope: None,
            additional_parameters: None,
            token_config: None,
            settings: None,
        };
        let req = Request::new("Get", HttpMethod::Get, "/secure")
            .with_auth(Auth::OAuth2(flow));
        let snap = RequestSignatureSnapshot::from_request("secure.yml", &req);
        assert_eq!(snap.auth_detail, "my-client");
    }

    #[test]
    fn old_format_snapshot_deserialises_without_error() {
        // Regression test: snapshots written before the full-changelog expansion
        // used key-list fields only (headerKeys, queryParamKeys, bodyFieldKeys).
        // The new fields (headers, queryParams, authDetail) must default to empty
        // rather than causing a serde error that silently breaks the audit hook.
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
        let snapshot: ContractSnapshot = serde_yaml::from_str(yaml)
            .expect("old-format snapshot must deserialise without error");
        assert_eq!(snapshot.entries.len(), 1);
        let entry = &snapshot.entries[0];
        // New fields must default to empty — not an error.
        assert!(entry.headers.is_empty());
        assert!(entry.query_params.is_empty());
        assert!(entry.auth_detail.is_empty());
        // Legacy fields are still there.
        assert_eq!(entry.header_keys, vec!["Authorization".to_string()]);
    }

    #[test]
    fn from_request_skips_disabled_headers_and_params() {
        let mut req = Request::new("Get", HttpMethod::Get, "/x");
        req.headers.push(Header { key: "X-Enabled".into(), value: "yes".into(), enabled: true, description: None });
        req.headers.push(Header { key: "X-Disabled".into(), value: "no".into(), enabled: false, description: None });
        req.query_params.push(QueryParam { key: "active".into(), value: "1".into(), enabled: true, description: None });
        req.query_params.push(QueryParam { key: "inactive".into(), value: "0".into(), enabled: false, description: None });
        let snap = RequestSignatureSnapshot::from_request("x.yml", &req);
        assert_eq!(snap.headers.len(), 1);
        assert_eq!(snap.headers[0].key, "X-Enabled");
        assert_eq!(snap.query_params.len(), 1);
        assert_eq!(snap.query_params[0].key, "active");
    }
}
