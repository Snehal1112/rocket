# Bruno Import — Plan 03: Bruno `.yml` Adapter

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement serde structs that deserialise Bruno's `.yml` request/environment format, plus a `yml_adapter` that normalises them into the shared `BruDocument` AST — so the converter (plan-04) sees one uniform type regardless of source format.

**Architecture:** `yml_adapter.rs` defines `BruYmlRequest` and `BruYmlEnv` serde structs matching Bruno's `.yml` schema, and `From` impls that produce `BruDocument`. The lexer/parser from plan-02 is NOT used for `.yml` files — serde_yaml handles deserialisation directly.

**Tech Stack:** Rust, serde, serde_yaml

**Prerequisite:** Plans 01 and 02 complete.

**Spec:** `docs/superpowers/specs/2026-04-04-bruno-import-design.md`

---

## Task 1: Bruno `.yml` serde structs

**Files:**
- Modify: `crates/rocket-import/src/bru/yml_adapter.rs`

Bruno's `.yml` request format looks like:

```yaml
meta:
  name: Get Users
  type: http
  seq: 1
http:
  method: GET
  url: "{{baseUrl}}/users"
  auth:
    mode: bearer
    bearer:
      token: "{{authToken}}"
  headers:
    - name: Content-Type
      value: application/json
      disabled: false
  body:
    mode: json
    json: |
      {"page": 1}
  script:
    req: "bru.setVar('ts', Date.now());"
    res: ""
```

Bruno's `.yml` environment format looks like:

```yaml
name: local
variables:
  - name: baseUrl
    value: http://localhost:3000
    enabled: true
  - name: DB_PASSWORD
    value: ""
    secret: true
    enabled: true
```

- [ ] **Step 1: Write the serde structs**

```rust
use serde::{Deserialize, Serialize};
use crate::bru::ast::*;
use crate::error::ImportResult;

// ─── Request structs ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BruYmlRequest {
    pub meta: Option<BruYmlMeta>,
    pub http: Option<BruYmlHttp>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlMeta {
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub request_type: Option<String>,
    pub seq: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlHttp {
    pub method: Option<String>,
    pub url: Option<String>,
    pub headers: Option<Vec<BruYmlHeader>>,
    pub body: Option<BruYmlBody>,
    pub auth: Option<BruYmlAuth>,
    pub script: Option<BruYmlScript>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlHeader {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlBody {
    pub mode: Option<String>,
    pub json: Option<String>,
    pub text: Option<String>,
    pub xml: Option<String>,
    #[serde(rename = "formUrlEncoded")]
    pub form_url_encoded: Option<Vec<BruYmlFormField>>,
    pub multipart: Option<Vec<BruYmlFormField>>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlFormField {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlAuth {
    pub mode: Option<String>,
    pub bearer: Option<BruYmlBearerAuth>,
    pub basic: Option<BruYmlBasicAuth>,
    pub awsv4: Option<BruYmlAwsV4Auth>,
    pub apikey: Option<BruYmlApiKeyAuth>,
    pub digest: Option<BruYmlBasicAuth>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlBearerAuth {
    pub token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlBasicAuth {
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlAwsV4Auth {
    #[serde(rename = "accessKeyId")]
    pub access_key_id: Option<String>,
    #[serde(rename = "secretAccessKey")]
    pub secret_access_key: Option<String>,
    #[serde(rename = "sessionToken")]
    pub session_token: Option<String>,
    pub service: Option<String>,
    pub region: Option<String>,
    #[serde(rename = "profileName")]
    pub profile_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlApiKeyAuth {
    pub key: Option<String>,
    pub value: Option<String>,
    pub placement: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlScript {
    pub req: Option<String>,
    pub res: Option<String>,
}

// ─── Environment structs ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BruYmlEnv {
    pub name: Option<String>,
    pub variables: Option<Vec<BruYmlEnvVar>>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlEnvVar {
    pub name: String,
    pub value: Option<String>,
    #[serde(default)]
    pub secret: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool { true }
```

- [ ] **Step 2: Verify compile**

```bash
cargo check -p rocket-import
```
Expected: compiles cleanly.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-import/src/bru/yml_adapter.rs
git commit -m "feat(import): BruYmlRequest and BruYmlEnv serde structs for Bruno .yml format"
```

---

## Task 2: Adapter — `BruYmlRequest` → `BruDocument`

**Files:**
- Modify: `crates/rocket-import/src/bru/yml_adapter.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapts_yml_request_to_bru_document() {
        let yml = r#"
meta:
  name: Get Users
  type: http
  seq: 1
http:
  method: GET
  url: "{{baseUrl}}/users"
  headers:
    - name: Content-Type
      value: application/json
      disabled: false
  body:
    mode: json
    json: '{"page": 1}'
  auth:
    mode: bearer
    bearer:
      token: "{{authToken}}"
  script:
    req: "bru.setVar('ts', Date.now());"
    res: ""
"#;
        let doc = bru_document_from_yml_str(yml).unwrap();
        assert_eq!(doc.meta.as_ref().unwrap().name, "Get Users");
        assert_eq!(doc.method, Some(BruMethod::Get));
        assert_eq!(doc.url.as_deref(), Some("{{baseUrl}}/users"));
        assert_eq!(doc.headers.len(), 1);
        assert!(matches!(doc.body, Some(BruBody::Json(_))));
        assert!(matches!(doc.auth, Some(BruAuth::Bearer { .. })));
        assert!(doc.pre_request_script.is_some());
    }

    #[test]
    fn unknown_auth_mode_lands_in_unknown_blocks() {
        let yml = r#"
meta:
  name: Test
  type: http
http:
  method: GET
  url: https://example.com
  auth:
    mode: oauth2
"#;
        let doc = bru_document_from_yml_str(yml).unwrap();
        assert_eq!(doc.unknown_blocks.len(), 1);
        assert_eq!(doc.unknown_blocks[0].name, "auth");
        assert_eq!(doc.unknown_blocks[0].subtype.as_deref(), Some("oauth2"));
    }

    #[test]
    fn adapts_yml_env_to_bru_document() {
        let yml = r#"
name: local
variables:
  - name: baseUrl
    value: http://localhost:3000
    enabled: true
  - name: DB_PASSWORD
    value: ""
    secret: true
    enabled: true
"#;
        let doc = bru_document_from_yml_env_str(yml).unwrap();
        assert_eq!(doc.vars.len(), 1);
        assert_eq!(doc.vars[0].key, "baseUrl");
        assert_eq!(doc.secret_vars.len(), 1);
        assert_eq!(doc.secret_vars[0], "DB_PASSWORD");
    }

    #[test]
    fn graphql_request_type_lands_in_unknown_blocks() {
        let yml = r#"
meta:
  name: GQL Query
  type: graphql
http:
  method: POST
  url: https://api.example.com/graphql
"#;
        let doc = bru_document_from_yml_str(yml).unwrap();
        assert_eq!(doc.unknown_blocks.len(), 1);
        assert_eq!(doc.unknown_blocks[0].name, "unsupported_type");
    }
}
```

Run: `cargo test -p rocket-import bru::yml_adapter`
Expected: FAIL (adapter functions not yet implemented)

- [ ] **Step 2: Implement adapter functions**

Add to `yml_adapter.rs`:

```rust
/// Parse a Bruno .yml request file string into a BruDocument.
pub fn bru_document_from_yml_str(input: &str) -> ImportResult<BruDocument> {
    let yml: BruYmlRequest = serde_yaml::from_str(input)
        .map_err(|e| ImportError::ParseError {
            path: std::path::PathBuf::new(),
            message: e.to_string(),
        })?;
    Ok(adapt_request(yml))
}

/// Parse a Bruno .yml environment file string into a BruDocument.
pub fn bru_document_from_yml_env_str(input: &str) -> ImportResult<BruDocument> {
    let yml: BruYmlEnv = serde_yaml::from_str(input)
        .map_err(|e| ImportError::ParseError {
            path: std::path::PathBuf::new(),
            message: e.to_string(),
        })?;
    Ok(adapt_env(yml))
}

fn adapt_request(yml: BruYmlRequest) -> BruDocument {
    let mut doc = BruDocument::default();

    // Meta
    if let Some(m) = yml.meta {
        let request_type = m.request_type.clone().unwrap_or_default();
        // Non-http types go to unknown_blocks immediately.
        if !matches!(request_type.as_str(), "http" | "") {
            doc.unknown_blocks.push(BruRawBlock {
                name: "unsupported_type".into(),
                subtype: Some(request_type.clone()),
                content: String::new(),
            });
        }
        doc.meta = Some(BruMeta {
            name: m.name.unwrap_or_default(),
            request_type,
            seq: m.seq,
        });
    }

    if let Some(http) = yml.http {
        // Method
        doc.method = http.method.as_deref().and_then(|m| BruMethod::from_block_name(&m.to_lowercase()));
        doc.url = http.url;

        // Headers
        if let Some(headers) = http.headers {
            doc.headers = headers.into_iter().map(|h| BruKeyValue {
                key: h.name,
                value: h.value,
                disabled: h.disabled,
            }).collect();
        }

        // Body
        if let Some(body) = http.body {
            doc.body = adapt_body(body, &mut doc.unknown_blocks);
        }

        // Auth
        if let Some(auth) = http.auth {
            doc.auth = adapt_auth(auth, &mut doc.unknown_blocks);
        }

        // Scripts
        if let Some(script) = http.script {
            if let Some(req) = script.req {
                if !req.is_empty() { doc.pre_request_script = Some(req); }
            }
            if let Some(res) = script.res {
                if !res.is_empty() { doc.post_response_script = Some(res); }
            }
        }
    }

    doc
}

fn adapt_body(body: BruYmlBody, unknown: &mut Vec<BruRawBlock>) -> Option<BruBody> {
    match body.mode.as_deref() {
        Some("json")            => Some(BruBody::Json(body.json.unwrap_or_default())),
        Some("text")            => Some(BruBody::Text(body.text.unwrap_or_default())),
        Some("xml")             => Some(BruBody::Xml(body.xml.unwrap_or_default())),
        Some("formUrlEncoded")  => Some(BruBody::FormUrlEncoded(
            body.form_url_encoded.unwrap_or_default().into_iter()
                .map(|f| BruKeyValue { key: f.name, value: f.value, disabled: f.disabled })
                .collect()
        )),
        Some("multipart")       => Some(BruBody::Multipart(
            body.multipart.unwrap_or_default().into_iter()
                .map(|f| BruKeyValue { key: f.name, value: f.value, disabled: f.disabled })
                .collect()
        )),
        Some(other) => {
            unknown.push(BruRawBlock {
                name: "body".into(),
                subtype: Some(other.to_string()),
                content: String::new(),
            });
            None
        }
        None => None,
    }
}

fn adapt_auth(auth: BruYmlAuth, unknown: &mut Vec<BruRawBlock>) -> Option<BruAuth> {
    match auth.mode.as_deref() {
        Some("bearer") => {
            let b = auth.bearer.unwrap_or_default_bearer();
            Some(BruAuth::Bearer { token: b.token.unwrap_or_default() })
        }
        Some("basic") => {
            let b = auth.basic.unwrap_or_default_basic();
            Some(BruAuth::Basic {
                username: b.username.unwrap_or_default(),
                password: b.password.unwrap_or_default(),
            })
        }
        Some("awsv4") => {
            let a = auth.awsv4.unwrap_or(BruYmlAwsV4Auth {
                access_key_id: None, secret_access_key: None,
                session_token: None, service: None, region: None, profile_name: None,
            });
            Some(BruAuth::AwsV4 {
                access_key_id: a.access_key_id.unwrap_or_default(),
                secret_access_key: a.secret_access_key.unwrap_or_default(),
                session_token: a.session_token,
                service: a.service,
                region: a.region,
                profile_name: a.profile_name,
            })
        }
        Some("apikey") => {
            let a = auth.apikey.unwrap_or(BruYmlApiKeyAuth {
                key: None, value: None, placement: None,
            });
            Some(BruAuth::ApiKey {
                key: a.key.unwrap_or_default(),
                value: a.value.unwrap_or_default(),
                placement: a.placement.unwrap_or_default(),
            })
        }
        Some("digest") => {
            let d = auth.digest.unwrap_or_default_basic();
            Some(BruAuth::Digest {
                username: d.username.unwrap_or_default(),
                password: d.password.unwrap_or_default(),
            })
        }
        Some(other) => {
            unknown.push(BruRawBlock {
                name: "auth".into(),
                subtype: Some(other.to_string()),
                content: String::new(),
            });
            None
        }
        None => None,
    }
}

// Helper traits to avoid repeated Option::unwrap_or boilerplate.
trait DefaultBearer { fn unwrap_or_default_bearer(self) -> BruYmlBearerAuth; }
impl DefaultBearer for Option<BruYmlBearerAuth> {
    fn unwrap_or_default_bearer(self) -> BruYmlBearerAuth {
        self.unwrap_or(BruYmlBearerAuth { token: None })
    }
}

trait DefaultBasic { fn unwrap_or_default_basic(self) -> BruYmlBasicAuth; }
impl DefaultBasic for Option<BruYmlBasicAuth> {
    fn unwrap_or_default_basic(self) -> BruYmlBasicAuth {
        self.unwrap_or(BruYmlBasicAuth { username: None, password: None })
    }
}

fn adapt_env(yml: BruYmlEnv) -> BruDocument {
    let mut doc = BruDocument::default();
    if let Some(vars) = yml.variables {
        for v in vars {
            if v.secret {
                doc.secret_vars.push(v.name);
            } else {
                doc.vars.push(BruKeyValue {
                    key: v.name,
                    value: v.value.unwrap_or_default(),
                    disabled: !v.enabled,
                });
            }
        }
    }
    doc
}
```

- [ ] **Step 3: Run all tests**

```bash
cargo test -p rocket-import bru::yml_adapter
```
Expected: ALL PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-import/src/bru/yml_adapter.rs
git commit -m "feat(import): Bruno .yml adapter — normalises BruYmlRequest/Env to BruDocument"
```

---

## Task 3: File-level dispatch helper

**Files:**
- Modify: `crates/rocket-import/src/bru/mod.rs`

This helper is used by `ImportService` in plan-04 to parse any Bruno file without caring about its format.

- [ ] **Step 1: Add `parse_file` to `bru/mod.rs`**

```rust
pub(crate) mod ast;
pub(crate) mod lexer;
pub(crate) mod parser;
pub(crate) mod yml_adapter;

use std::path::Path;
use crate::error::ImportResult;
use ast::BruDocument;

/// Detect format from file extension and parse into a BruDocument.
/// `.bru` → lexer/parser; `.yml`/`.yaml` → yml_adapter.
pub(crate) fn parse_file(path: &Path) -> ImportResult<BruDocument> {
    let content = std::fs::read_to_string(path)
        .map_err(crate::error::ImportError::IoError)?;

    match path.extension().and_then(|e| e.to_str()) {
        Some("bru") => parser::parse(&content),
        Some("yml") | Some("yaml") => yml_adapter::bru_document_from_yml_str(&content),
        _ => Err(crate::error::ImportError::ParseError {
            path: path.to_path_buf(),
            message: "unsupported file extension (expected .bru or .yml)".into(),
        }),
    }
}

/// Parse a Bruno environment file (`.yml` or `.bru`) into a BruDocument.
pub(crate) fn parse_env_file(path: &Path) -> ImportResult<BruDocument> {
    let content = std::fs::read_to_string(path)
        .map_err(crate::error::ImportError::IoError)?;

    match path.extension().and_then(|e| e.to_str()) {
        Some("yml") | Some("yaml") => yml_adapter::bru_document_from_yml_env_str(&content),
        Some("bru") => parser::parse(&content),
        _ => Err(crate::error::ImportError::ParseError {
            path: path.to_path_buf(),
            message: "unsupported env file extension".into(),
        }),
    }
}
```

- [ ] **Step 2: Run all import tests**

```bash
cargo test -p rocket-import
```
Expected: ALL PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-import/src/bru/mod.rs
git commit -m "feat(import): bru::parse_file dispatch helper — routes .bru/.yml to correct parser"
```
