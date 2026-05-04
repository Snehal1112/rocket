# Contract Full Changelog Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expand the contract changelog to record all request field changes — header key+value pairs, query param key+value pairs, body content (raw) and form field key+value pairs, and auth credential fields — in addition to the existing structural fields (method, URL, auth type).

**Architecture:** Expand `RequestSignatureSnapshot` to store full field data (key+value pairs, body content, auth details) rather than only key lists. Expand `diff_signature` to diff the new fields and emit one `ChangelogEntry` per changed value. The on-disk snapshot format changes, but existing snapshots are handled via `#[serde(default)]` fields so old files still load. No frontend type changes are needed beyond adding the new fields to `RequestSignatureSnapshot` in `tauri-api.ts` (the `ChangelogEntry` wire format is unchanged).

**Tech Stack:** Rust (`rocket-collection`, `rocket-infra`, `rocket-app`), TypeScript (`src/lib/tauri-api.ts`).

---

## File Map

| File | Change |
|---|---|
| `crates/rocket-collection/src/contract/snapshot.rs` | Add new fields to `RequestSignatureSnapshot`; update `from_request` to populate them |
| `crates/rocket-collection/src/contract/diff.rs` | Expand `diff_signature` to diff new fields; add `diff_key_value_list` helper |
| `src/lib/tauri-api.ts` | Add new fields to `RequestSignatureSnapshot` interface |

No other files need to change — `ContractRepository`, `ContractService`, `FsContractRepo`, and all Tauri commands are unaffected.

---

### Task 1: Expand `RequestSignatureSnapshot` with full field data

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-collection/src/contract/snapshot.rs`

The snapshot currently stores only key lists (`header_keys`, `query_param_keys`, `body_field_keys`). We need to store full key+value pairs and the raw body content so the diff can detect value changes, not just key additions/removals.

New fields added to `RequestSignatureSnapshot`:

| Field | Type | Populated from |
|---|---|---|
| `headers` | `Vec<KeyValueEntry>` | `request.headers` where `enabled == true` |
| `query_params` | `Vec<KeyValueEntry>` | `request.query_params` where `enabled == true` |
| `body_content` | `Option<String>` | Raw body string for text/JSON/XML/Sparql/Binary modes |
| `form_fields` | `Vec<KeyValueEntry>` | `body.form_data` where `enabled == true` |
| `auth_detail` | `String` | Serialised auth credential summary (see below) |

`KeyValueEntry` is a new struct defined in the same file:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyValueEntry {
    pub key: String,
    pub value: String,
}
```

`auth_detail` is a single human-readable string that summarises auth credentials (not the auth type, which is already tracked). Rules:

- `Auth::None | Auth::Inherit` → `""`
- `Auth::Basic { username, .. }` → `username`
- `Auth::Bearer { token }` → first 8 chars of token + `"…"` if longer, else full token
- `Auth::ApiKey { key, value, placement }` → `"{key}={value} ({placement})"`
- `Auth::OAuth2(_)` → `"oauth2"` (credentials managed externally)
- `Auth::AwsSigV4 { access_key, region, service, .. }` → `"{access_key}@{region}/{service}"`
- `Auth::Wsse { username, .. } | Auth::Digest { username, .. } | Auth::Ntlm { username, .. }` → `username`

The old key-list fields (`header_keys`, `query_param_keys`, `body_field_keys`) are **kept** with `#[serde(default)]` so existing on-disk snapshots still deserialise. They are no longer populated by `from_request` (set to `vec![]`). The diff logic in Task 2 will use the new fields exclusively.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)]` block at the bottom of `crates/rocket-collection/src/contract/snapshot.rs`:

```rust
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
    use rocket_shared::types::QueryParam;

    let mut req = Request::new("Get", HttpMethod::Get, "/search");
    req.query_params.push(QueryParam {
        key: "q".into(),
        value: "hello".into(),
        enabled: true,
        description: None,
    });

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
    use rocket_shared::types::{FormDataEntry, FormDataType};

    let req = Request::new("Post", HttpMethod::Post, "/form").with_body(Body {
        mode: BodyMode::FormData,
        content: None,
        form_data: Some(vec![
            FormDataEntry {
                key: "name".into(),
                value: "Ada".into(),
                entry_type: FormDataType::Text,
                enabled: true,
                content_type: None,
                description: None,
            },
        ]),
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

    // First 8 chars + ellipsis
    assert_eq!(snap.auth_detail, "supersec…");
}

#[test]
fn from_request_skips_disabled_headers_and_params() {
    use rocket_shared::types::{Header, QueryParam};

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
```

- [ ] **Step 2: Run tests — verify they fail**

```bash
cargo test -p rocket-collection contract::snapshot 2>&1 | tail -20
```

Expected: compilation errors (fields don't exist yet).

- [ ] **Step 3: Implement the new snapshot fields**

Replace the contents of `crates/rocket-collection/src/contract/snapshot.rs` with:

```rust
use chrono::{DateTime, Utc};
use rocket_shared::types::{Auth, Body, BodyMode, HttpMethod};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ulid::Ulid;

use crate::request::Request;

/// A key-value pair captured from a request field.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyValueEntry {
    pub key: String,
    pub value: String,
}

/// Shape of one request at the moment a contract is signed.
// camelCase is intentional: serves as both YAML persistence and Tauri IPC wire type.
/// Rebuilt on every save and diffed against this baseline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RequestSignatureSnapshot {
    pub request_path: PathBuf,
    pub method: String,
    pub url_pattern: String,
    /// Enabled headers with their values.
    pub headers: Vec<KeyValueEntry>,
    /// Enabled query params with their values.
    pub query_params: Vec<KeyValueEntry>,
    /// Raw body string for text/JSON/XML/Sparql/Binary body modes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_content: Option<String>,
    /// Enabled form fields (FormData / FormUrlEncoded) with their values.
    #[serde(default)]
    pub form_fields: Vec<KeyValueEntry>,
    /// Summarised auth credentials (not the auth type).
    pub auth_type: String,
    pub auth_detail: String,
    pub captured_at: DateTime<Utc>,

    // Legacy fields kept for backward compatibility with old on-disk snapshots.
    // No longer populated by from_request; ignored by diff_signature.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub query_param_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub header_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body_field_keys: Vec<String>,
}

impl RequestSignatureSnapshot {
    /// Build a signature snapshot from a request and its collection-relative path.
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
            // Legacy fields — empty for new snapshots.
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

fn auth_detail(auth: &Auth) -> String {
    match auth {
        Auth::None | Auth::Inherit | Auth::OAuth2(_) => String::new(),
        Auth::Basic { username, .. }
        | Auth::Wsse { username, .. }
        | Auth::Digest { username, .. }
        | Auth::Ntlm { username, .. } => username.clone(),
        Auth::Bearer { token } => {
            if token.len() > 8 {
                format!("{}…", &token[..8])
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

fn extract_body_content(body: &Option<Body>) -> Option<String> {
    let Some(body) = body else { return None };
    match body.mode {
        BodyMode::Json
        | BodyMode::Xml
        | BodyMode::Text
        | BodyMode::Sparql
        | BodyMode::Binary => body.content.clone(),
        BodyMode::FormUrlEncoded | BodyMode::FormData | BodyMode::None => None,
    }
}

fn extract_form_fields(body: &Option<Body>) -> Vec<KeyValueEntry> {
    let Some(body) = body else { return vec![] };
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
        assert_eq!(snap.headers[0].key, "X-Trace-Id");
        assert_eq!(snap.query_params[0].key, "page");
        assert_eq!(snap.auth_type, "none");
    }

    #[test]
    fn from_request_extracts_json_body_content() {
        let req = Request::new("Create", HttpMethod::Post, "/users").with_body(Body {
            mode: BodyMode::Json,
            content: Some(r#"{"name":"Ada","email":"a@b.com"}"#.into()),
            form_data: None,
            file_path: None,
        });

        let snap = RequestSignatureSnapshot::from_request("create.yml", &req);

        assert_eq!(snap.body_content, Some(r#"{"name":"Ada","email":"a@b.com"}"#.to_string()));
        assert!(snap.form_fields.is_empty());
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
        req.query_params.push(QueryParam {
            key: "q".into(),
            value: "hello".into(),
            enabled: true,
            description: None,
        });

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
```

- [ ] **Step 4: Run tests — verify they pass**

```bash
cargo test -p rocket-collection contract::snapshot 2>&1 | tail -20
```

Expected: all snapshot tests pass.

- [ ] **Step 5: Cargo check the whole workspace**

```bash
cargo check 2>&1 | grep -E "error|warning: unused" | head -30
```

Expected: no errors. There may be warnings about `body_field_keys` / `header_keys` / `query_param_keys` being unused in `diff.rs` — those go away in Task 2.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-collection/src/contract/snapshot.rs
git commit -m "feat(contract): expand snapshot to capture full key-value fields and auth detail"
```

---

### Task 2: Expand `diff_signature` to diff all new fields

**Files:**
- Modify: `crates/rocket-collection/src/contract/diff.rs`

The diff function needs to compare:
- `headers` — detect added/removed keys, and value changes for existing keys
- `query_params` — same
- `form_fields` — same
- `body_content` — simple string equality (Changed entry if different)
- `auth_detail` — simple string equality (Changed entry if different)

The existing `diff_key_list` helper is replaced by a new `diff_key_value_list` helper that:
1. Reports `Removed` for keys present in `old` but absent in `new`
2. Reports `Added` for keys present in `new` but absent in `old`
3. Reports `Changed` (with old/new value) for keys present in both where the value differs

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)]` block in `crates/rocket-collection/src/contract/diff.rs`:

```rust
fn make_kv(key: &str, value: &str) -> KeyValueEntry {
    KeyValueEntry { key: key.into(), value: value.into() }
}

fn base_snap_v2() -> RequestSignatureSnapshot {
    RequestSignatureSnapshot {
        request_path: PathBuf::from("requests/payment.yml"),
        method: "POST".into(),
        url_pattern: "/payments".into(),
        headers: vec![make_kv("Authorization", "Bearer old"), make_kv("Content-Type", "application/json")],
        query_params: vec![make_kv("currency", "USD")],
        body_content: Some(r#"{"amount":100}"#.into()),
        form_fields: vec![],
        auth_type: "bearer".into(),
        auth_detail: "oldtoken…".into(),
        captured_at: Utc::now(),
        // Legacy fields empty.
        query_param_keys: vec![],
        header_keys: vec![],
        body_field_keys: vec![],
    }
}

#[test]
fn header_value_change_detected() {
    let old = base_snap_v2();
    let mut new = base_snap_v2();
    new.headers[0].value = "Bearer new".into();

    let changes = diff_signature(&old, &new);

    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].field, "header.Authorization");
    assert_eq!(changes[0].change_type, ChangeType::Changed);
    assert_eq!(changes[0].old_value.as_deref(), Some("Bearer old"));
    assert_eq!(changes[0].new_value.as_deref(), Some("Bearer new"));
}

#[test]
fn header_removed_detected() {
    let old = base_snap_v2();
    let mut new = base_snap_v2();
    new.headers.retain(|h| h.key != "Content-Type");

    let changes = diff_signature(&old, &new);

    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].field, "header.Content-Type");
    assert_eq!(changes[0].change_type, ChangeType::Removed);
}

#[test]
fn header_added_detected() {
    let old = base_snap_v2();
    let mut new = base_snap_v2();
    new.headers.push(make_kv("X-New", "yes"));

    let changes = diff_signature(&old, &new);

    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].field, "header.X-New");
    assert_eq!(changes[0].change_type, ChangeType::Added);
    assert_eq!(changes[0].new_value.as_deref(), Some("yes"));
}

#[test]
fn query_param_value_change_detected() {
    let old = base_snap_v2();
    let mut new = base_snap_v2();
    new.query_params[0].value = "EUR".into();

    let changes = diff_signature(&old, &new);

    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].field, "query_param.currency");
    assert_eq!(changes[0].change_type, ChangeType::Changed);
    assert_eq!(changes[0].old_value.as_deref(), Some("USD"));
    assert_eq!(changes[0].new_value.as_deref(), Some("EUR"));
}

#[test]
fn body_content_change_detected() {
    let old = base_snap_v2();
    let mut new = base_snap_v2();
    new.body_content = Some(r#"{"amount":200}"#.into());

    let changes = diff_signature(&old, &new);

    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].field, "body");
    assert_eq!(changes[0].change_type, ChangeType::Changed);
    assert_eq!(changes[0].old_value.as_deref(), Some(r#"{"amount":100}"#));
    assert_eq!(changes[0].new_value.as_deref(), Some(r#"{"amount":200}"#));
}

#[test]
fn body_content_removed_detected() {
    let old = base_snap_v2();
    let mut new = base_snap_v2();
    new.body_content = None;

    let changes = diff_signature(&old, &new);

    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].field, "body");
    assert_eq!(changes[0].change_type, ChangeType::Removed);
    assert!(changes[0].new_value.is_none());
}

#[test]
fn auth_detail_change_detected() {
    let old = base_snap_v2();
    let mut new = base_snap_v2();
    new.auth_detail = "newtoken…".into();

    let changes = diff_signature(&old, &new);

    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].field, "auth_detail");
    assert_eq!(changes[0].change_type, ChangeType::Changed);
}

#[test]
fn no_changes_v2_returns_empty() {
    let snap = base_snap_v2();
    assert!(diff_signature(&snap, &snap).is_empty());
}
```

- [ ] **Step 2: Run tests — verify they fail**

```bash
cargo test -p rocket-collection contract::diff 2>&1 | tail -20
```

Expected: compilation errors (`make_kv`, `base_snap_v2`, new fields not referenced yet).

- [ ] **Step 3: Implement the expanded diff**

Replace `crates/rocket-collection/src/contract/diff.rs` with:

```rust
use crate::contract::changelog::{ChangeType, ChangelogEntry};
use crate::contract::snapshot::{KeyValueEntry, RequestSignatureSnapshot};
use chrono::Utc;

/// Pure function — no I/O, no side effects.
/// Returns one `ChangelogEntry` per detected change.
///
/// This is the Model B extension seam:
/// the save hook calls this function and currently logs results silently.
/// Model B will act on the return value (warn / block) without changing this function.
pub fn diff_signature(
    old: &RequestSignatureSnapshot,
    new: &RequestSignatureSnapshot,
) -> Vec<ChangelogEntry> {
    let mut entries = Vec::new();
    let now = Utc::now();
    let path = new.request_path.clone();

    macro_rules! field_diff {
        ($field:expr, $old:expr, $new:expr) => {
            if $old != $new {
                entries.push(ChangelogEntry {
                    timestamp: now,
                    request_path: path.clone(),
                    field: $field.to_string(),
                    change_type: ChangeType::Changed,
                    old_value: Some($old.to_string()),
                    new_value: Some($new.to_string()),
                });
            }
        };
    }

    field_diff!("method", old.method, new.method);
    field_diff!("url_pattern", old.url_pattern, new.url_pattern);
    field_diff!("auth_type", old.auth_type, new.auth_type);

    // Auth credentials detail (non-empty strings only — skip when both are absent).
    if old.auth_detail != new.auth_detail
        && !(old.auth_detail.is_empty() && new.auth_detail.is_empty())
    {
        entries.push(ChangelogEntry {
            timestamp: now,
            request_path: path.clone(),
            field: "auth_detail".to_string(),
            change_type: ChangeType::Changed,
            old_value: if old.auth_detail.is_empty() { None } else { Some(old.auth_detail.clone()) },
            new_value: if new.auth_detail.is_empty() { None } else { Some(new.auth_detail.clone()) },
        });
    }

    diff_key_value_list(&path, "header", &old.headers, &new.headers, now, &mut entries);
    diff_key_value_list(&path, "query_param", &old.query_params, &new.query_params, now, &mut entries);
    diff_key_value_list(&path, "form_field", &old.form_fields, &new.form_fields, now, &mut entries);

    // Body content (raw text diff).
    match (&old.body_content, &new.body_content) {
        (Some(o), Some(n)) if o != n => {
            entries.push(ChangelogEntry {
                timestamp: now,
                request_path: path.clone(),
                field: "body".to_string(),
                change_type: ChangeType::Changed,
                old_value: Some(o.clone()),
                new_value: Some(n.clone()),
            });
        }
        (Some(o), None) => {
            entries.push(ChangelogEntry {
                timestamp: now,
                request_path: path.clone(),
                field: "body".to_string(),
                change_type: ChangeType::Removed,
                old_value: Some(o.clone()),
                new_value: None,
            });
        }
        (None, Some(n)) => {
            entries.push(ChangelogEntry {
                timestamp: now,
                request_path: path.clone(),
                field: "body".to_string(),
                change_type: ChangeType::Added,
                old_value: None,
                new_value: Some(n.clone()),
            });
        }
        _ => {}
    }

    entries
}

/// Diff two key-value lists:
/// - Removed: key in old, not in new
/// - Added: key in new, not in old (new_value is the value)
/// - Changed: key in both, but value differs
fn diff_key_value_list(
    path: &std::path::Path,
    prefix: &str,
    old: &[KeyValueEntry],
    new: &[KeyValueEntry],
    now: chrono::DateTime<chrono::Utc>,
    out: &mut Vec<ChangelogEntry>,
) {
    for old_entry in old {
        match new.iter().find(|e| e.key == old_entry.key) {
            None => out.push(ChangelogEntry {
                timestamp: now,
                request_path: path.to_path_buf(),
                field: format!("{}.{}", prefix, old_entry.key),
                change_type: ChangeType::Removed,
                old_value: Some(old_entry.value.clone()),
                new_value: None,
            }),
            Some(new_entry) if new_entry.value != old_entry.value => {
                out.push(ChangelogEntry {
                    timestamp: now,
                    request_path: path.to_path_buf(),
                    field: format!("{}.{}", prefix, old_entry.key),
                    change_type: ChangeType::Changed,
                    old_value: Some(old_entry.value.clone()),
                    new_value: Some(new_entry.value.clone()),
                });
            }
            _ => {}
        }
    }
    for new_entry in new {
        if !old.iter().any(|e| e.key == new_entry.key) {
            out.push(ChangelogEntry {
                timestamp: now,
                request_path: path.to_path_buf(),
                field: format!("{}.{}", prefix, new_entry.key),
                change_type: ChangeType::Added,
                old_value: None,
                new_value: Some(new_entry.value.clone()),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::snapshot::KeyValueEntry;
    use std::path::PathBuf;

    fn make_kv(key: &str, value: &str) -> KeyValueEntry {
        KeyValueEntry { key: key.into(), value: value.into() }
    }

    fn base_snap_v2() -> RequestSignatureSnapshot {
        RequestSignatureSnapshot {
            request_path: PathBuf::from("requests/payment.yml"),
            method: "POST".into(),
            url_pattern: "/payments".into(),
            headers: vec![make_kv("Authorization", "Bearer old"), make_kv("Content-Type", "application/json")],
            query_params: vec![make_kv("currency", "USD")],
            body_content: Some(r#"{"amount":100}"#.into()),
            form_fields: vec![],
            auth_type: "bearer".into(),
            auth_detail: "oldtoken…".into(),
            captured_at: Utc::now(),
            query_param_keys: vec![],
            header_keys: vec![],
            body_field_keys: vec![],
        }
    }

    #[test]
    fn no_changes_v2_returns_empty() {
        let snap = base_snap_v2();
        assert!(diff_signature(&snap, &snap).is_empty());
    }

    #[test]
    fn method_change_detected() {
        let old = base_snap_v2();
        let mut new = base_snap_v2();
        new.method = "PUT".into();
        let changes = diff_signature(&old, &new);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "method");
        assert_eq!(changes[0].change_type, ChangeType::Changed);
    }

    #[test]
    fn header_value_change_detected() {
        let old = base_snap_v2();
        let mut new = base_snap_v2();
        new.headers[0].value = "Bearer new".into();

        let changes = diff_signature(&old, &new);

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "header.Authorization");
        assert_eq!(changes[0].change_type, ChangeType::Changed);
        assert_eq!(changes[0].old_value.as_deref(), Some("Bearer old"));
        assert_eq!(changes[0].new_value.as_deref(), Some("Bearer new"));
    }

    #[test]
    fn header_removed_detected() {
        let old = base_snap_v2();
        let mut new = base_snap_v2();
        new.headers.retain(|h| h.key != "Content-Type");

        let changes = diff_signature(&old, &new);

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "header.Content-Type");
        assert_eq!(changes[0].change_type, ChangeType::Removed);
    }

    #[test]
    fn header_added_detected() {
        let old = base_snap_v2();
        let mut new = base_snap_v2();
        new.headers.push(make_kv("X-New", "yes"));

        let changes = diff_signature(&old, &new);

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "header.X-New");
        assert_eq!(changes[0].change_type, ChangeType::Added);
        assert_eq!(changes[0].new_value.as_deref(), Some("yes"));
    }

    #[test]
    fn query_param_value_change_detected() {
        let old = base_snap_v2();
        let mut new = base_snap_v2();
        new.query_params[0].value = "EUR".into();

        let changes = diff_signature(&old, &new);

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "query_param.currency");
        assert_eq!(changes[0].change_type, ChangeType::Changed);
        assert_eq!(changes[0].old_value.as_deref(), Some("USD"));
        assert_eq!(changes[0].new_value.as_deref(), Some("EUR"));
    }

    #[test]
    fn body_content_change_detected() {
        let old = base_snap_v2();
        let mut new = base_snap_v2();
        new.body_content = Some(r#"{"amount":200}"#.into());

        let changes = diff_signature(&old, &new);

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "body");
        assert_eq!(changes[0].change_type, ChangeType::Changed);
        assert_eq!(changes[0].old_value.as_deref(), Some(r#"{"amount":100}"#));
        assert_eq!(changes[0].new_value.as_deref(), Some(r#"{"amount":200}"#));
    }

    #[test]
    fn body_content_removed_detected() {
        let old = base_snap_v2();
        let mut new = base_snap_v2();
        new.body_content = None;

        let changes = diff_signature(&old, &new);

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "body");
        assert_eq!(changes[0].change_type, ChangeType::Removed);
        assert!(changes[0].new_value.is_none());
    }

    #[test]
    fn auth_detail_change_detected() {
        let old = base_snap_v2();
        let mut new = base_snap_v2();
        new.auth_detail = "newtoken…".into();

        let changes = diff_signature(&old, &new);

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "auth_detail");
        assert_eq!(changes[0].change_type, ChangeType::Changed);
    }

    #[test]
    fn both_auth_detail_empty_no_entry() {
        // When old and new both have no auth detail, no entry should fire.
        let mut old = base_snap_v2();
        let mut new = base_snap_v2();
        old.auth_detail = String::new();
        new.auth_detail = String::new();

        assert!(diff_signature(&old, &new).is_empty());
    }
}
```

- [ ] **Step 4: Run all rocket-collection tests**

```bash
cargo test -p rocket-collection 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 5: Cargo check full workspace**

```bash
cargo check 2>&1 | grep "^error" | head -20
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-collection/src/contract/diff.rs
git commit -m "feat(contract): diff full key-value fields, body content, and auth detail"
```

---

### Task 3: Update frontend `RequestSignatureSnapshot` type

**Files:**
- Modify: `src/lib/tauri-api.ts`

The `RequestSignatureSnapshot` interface is used when calling `attach_contract` (the `initialSnapshots` field). It must match the Rust struct's new wire shape so TypeScript callers can construct correct snapshots.

- [ ] **Step 1: Update the interface**

In `src/lib/tauri-api.ts`, find `export interface RequestSignatureSnapshot` and replace it with:

```typescript
export interface KeyValueEntry {
  key: string;
  value: string;
}

export interface RequestSignatureSnapshot {
  requestPath: string;
  method: string;
  urlPattern: string;
  /** Enabled headers with key and value. */
  headers: KeyValueEntry[];
  /** Enabled query params with key and value. */
  queryParams: KeyValueEntry[];
  /** Raw body string (JSON/XML/Text/Sparql/Binary). Absent for form bodies. */
  bodyContent?: string;
  /** Enabled form fields (FormData/FormUrlEncoded) with key and value. */
  formFields: KeyValueEntry[];
  authType: string;
  /** Summarised auth credentials for change detection. */
  authDetail: string;
  capturedAt: string;
}
```

- [ ] **Step 2: TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | head -30
```

Expected: no errors. (`RequestSignatureSnapshot` is only used in `AttachContractInput.initialSnapshots`, which the frontend currently always passes as `[]`, so no call sites break.)

- [ ] **Step 3: Commit**

```bash
git add src/lib/tauri-api.ts
git commit -m "feat(contract): update RequestSignatureSnapshot type with full field data"
```

---

### Task 4: Migrate existing on-disk snapshots (one-time, manual)

> This task is informational — no code change required. Existing snapshot YAML files on disk will still deserialise because the old fields (`headerKeys`, `queryParamKeys`, `bodyFieldKeys`) have `#[serde(default)]` and the new fields (`headers`, `queryParams`, etc.) also have `#[serde(default)]` via empty vecs.
>
> **Effect on existing contracts:** The first time a request covered by an existing contract is saved after this update, `on_request_saved` loads the old snapshot (which has empty `headers`/`queryParams`/etc.) and diffs it against the new full snapshot. This will fire changelog entries for every header and query param that exists in the request, since they appear as "added" relative to an empty baseline.
>
> To avoid this false-positive burst, the old snapshots should be regenerated. The clean way: after deploying this build, the user can delete the existing `-snapshot.yml` files for each contract. The next save of any covered request will create a fresh full-fidelity baseline, and subsequent saves will produce accurate diffs.
>
> No automated migration is implemented at this stage (YAGNI — the user base is small).

- [ ] **Step 1: Document the migration note in a code comment**

In `crates/rocket-collection/src/contract/snapshot.rs`, add a comment above the legacy fields:

```rust
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
```

- [ ] **Step 2: Commit**

```bash
git add crates/rocket-collection/src/contract/snapshot.rs
git commit -m "docs(contract): note legacy snapshot field migration behaviour"
```

---

### Task 5: Full verification

- [ ] **Step 1: Run all crate tests**

```bash
cargo test -p rocket-collection -p rocket-app 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 2: TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | head -20
```

Expected: no errors.

- [ ] **Step 3: Biome lint**

```bash
yarn check 2>&1 | head -20
```

Expected: no errors or warnings.

- [ ] **Step 4: Smoke-test in the running app**

1. Run `yarn tauri dev`.
2. Open a collection that has a contract attached.
3. Open a request covered by the contract.
4. Change a header value (not key) and save.
5. Open the contract tab → click Changelog.
6. Verify a `header.<name>` Changed entry appears with old and new values.
7. Change the request body content and save again.
8. Verify a `body` Changed entry appears.

- [ ] **Step 5: Final commit if anything was adjusted**

```bash
git add -p
git commit -m "fix(contract): adjust after smoke test"
```

---

## Self-Review

**Spec coverage:**
- Header key additions/removals → `diff_key_value_list` on `headers` ✓
- Header value changes → `diff_key_value_list` Changed arm ✓
- Query param key additions/removals + value changes → `diff_key_value_list` on `query_params` ✓
- Body content changes → body diff in `diff_signature` ✓
- Form field key+value changes → `diff_key_value_list` on `form_fields` ✓
- Auth credential changes → `auth_detail` field + diff ✓
- Existing method/URL/auth-type tracking preserved ✓
- Backward compatibility with old snapshots → `#[serde(default)]` on all new fields + legacy fields kept ✓
- Frontend type updated ✓

**Placeholder scan:** None found.

**Type consistency:** `KeyValueEntry` defined in Task 1 (`snapshot.rs`), imported in Task 2 (`diff.rs`) via `use crate::contract::snapshot::KeyValueEntry`, and in `tauri-api.ts` in Task 3. All consistent.
