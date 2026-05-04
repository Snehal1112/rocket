# Postman Import — Plan 01: AST + Parser + Fixtures

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the `postman` module to `rocket-import` — serde AST structs, a JSON file parser, an environment file parser, and four fixture JSON files used by all later tests.

**Architecture:** New `crates/rocket-import/src/postman/` sub-module (4 files). Extends `ImportError` with two new variants. Adds `serde_json` dependency. Zero changes to any existing module except `error.rs` and `lib.rs`.

**Tech Stack:** Rust, serde, serde_json, thiserror

**Spec:** `docs/superpowers/specs/2026-05-02-postman-import-design.md`

**Prerequisite:** `rocket-import` crate exists (Bruno import complete).

---

## File Map

| File | Action |
|---|---|
| `crates/rocket-import/Cargo.toml` | Modify |
| `crates/rocket-import/src/error.rs` | Modify |
| `crates/rocket-import/src/lib.rs` | Modify |
| `crates/rocket-import/src/postman/mod.rs` | Create |
| `crates/rocket-import/src/postman/ast.rs` | Create |
| `crates/rocket-import/src/postman/parser.rs` | Create |
| `crates/rocket-import/src/postman/env_parser.rs` | Create |
| `crates/rocket-import/tests/fixtures/postman/minimal-collection.json` | Create |
| `crates/rocket-import/tests/fixtures/postman/full-collection.json` | Create |
| `crates/rocket-import/tests/fixtures/postman/v2.0-collection.json` | Create |
| `crates/rocket-import/tests/fixtures/postman/environment.json` | Create |

---

## Task 1: Cargo dep + error variants + AST structs

**Files:**
- Modify: `crates/rocket-import/Cargo.toml`
- Modify: `crates/rocket-import/src/error.rs`
- Modify: `crates/rocket-import/src/lib.rs`
- Create: `crates/rocket-import/src/postman/mod.rs`
- Create: `crates/rocket-import/src/postman/ast.rs`

- [ ] **Step 1: Add `serde_json` to Cargo.toml**

In `crates/rocket-import/Cargo.toml`, add to `[dependencies]`:

```toml
serde_json = "1"
```

- [ ] **Step 2: Extend `error.rs` with two new variants**

Open `crates/rocket-import/src/error.rs`. Append two variants to the existing `ImportError` enum:

```rust
#[error("not a Postman collection (missing schema.getpostman.com in info.schema): {0}")]
NotAPostmanCollection(std::path::PathBuf),

#[error("JSON parse error in {path}: {message}")]
JsonParseError { path: std::path::PathBuf, message: String },
```

- [ ] **Step 3: Create `crates/rocket-import/src/postman/mod.rs`**

```rust
pub(crate) mod ast;
pub(crate) mod env_parser;
pub(crate) mod parser;

pub(crate) use ast::PostmanCollection;
pub(crate) use env_parser::{parse_postman_environment, PostmanEnvironment};
pub(crate) use parser::parse_postman_json;
```

- [ ] **Step 4: Create `crates/rocket-import/src/postman/ast.rs`**

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanCollection {
    pub info: PostmanInfo,
    #[serde(default)]
    pub item: Vec<PostmanItem>,
    #[serde(default)]
    pub variable: Vec<PostmanVariable>,
    pub auth: Option<PostmanAuth>,
    /// Environments embedded directly in the collection export.
    /// Most real Postman exports include environments here.
    #[serde(default)]
    pub environment: Vec<PostmanEmbeddedEnvironment>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanInfo {
    pub name: String,
    pub schema: String,
}

/// Untagged: Request is tried first because it has a required `request`
/// field. Folder items lack that field and fall through to `Folder`.
/// (Reversing this order would misclassify every request as a folder,
/// since `PostmanFolder` only requires `name` — all other fields default.)
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum PostmanItem {
    Request(PostmanRequestItem),
    Folder(PostmanFolder),
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanFolder {
    pub name: String,
    #[serde(default)]
    pub item: Vec<PostmanItem>,
    pub auth: Option<PostmanAuth>,
    #[serde(default)]
    pub variable: Vec<PostmanVariable>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanRequestItem {
    pub name: String,
    pub request: PostmanRequest,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanRequest {
    pub method: String,
    pub url: PostmanUrl,
    #[serde(default)]
    pub header: Vec<PostmanHeader>,
    pub auth: Option<PostmanAuth>,
    pub body: Option<PostmanBody>,
    pub description: Option<PostmanDescription>,
}

/// Untagged: Object (v2.1) tried first, then plain String (v2.0).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum PostmanUrl {
    Object(PostmanUrlObject),
    String(String),
}

impl PostmanUrl {
    pub(crate) fn raw(&self) -> &str {
        match self {
            PostmanUrl::Object(o) => &o.raw,
            PostmanUrl::String(s) => s.as_str(),
        }
    }

    pub(crate) fn query_params(&self) -> &[PostmanQueryParam] {
        match self {
            PostmanUrl::Object(o) => &o.query,
            PostmanUrl::String(_) => &[],
        }
    }

    pub(crate) fn path_variables(&self) -> &[PostmanPathVariable] {
        match self {
            PostmanUrl::Object(o) => &o.variable,
            PostmanUrl::String(_) => &[],
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanUrlObject {
    pub raw: String,
    #[serde(default)]
    pub query: Vec<PostmanQueryParam>,
    #[serde(default)]
    pub variable: Vec<PostmanPathVariable>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanHeader {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanQueryParam {
    pub key: Option<String>,
    pub value: Option<String>,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanPathVariable {
    pub key: String,
    pub value: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanVariable {
    pub key: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanAuth {
    #[serde(rename = "type")]
    pub auth_type: String,
    #[serde(default)]
    pub bearer: Vec<PostmanKeyValue>,
    #[serde(default)]
    pub basic: Vec<PostmanKeyValue>,
    #[serde(default)]
    pub apikey: Vec<PostmanKeyValue>,
    #[serde(default)]
    pub oauth2: Vec<PostmanKeyValue>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanKeyValue {
    pub key: String,
    pub value: serde_json::Value,
}

impl PostmanKeyValue {
    /// Extract value as String regardless of JSON type.
    pub(crate) fn as_str_value(&self) -> String {
        match &self.value {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanBody {
    pub mode: String,
    pub raw: Option<String>,
    pub options: Option<PostmanBodyOptions>,
    #[serde(default)]
    pub urlencoded: Vec<PostmanFormParam>,
    #[serde(default)]
    pub formdata: Vec<PostmanFormParam>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanBodyOptions {
    pub raw: Option<PostmanRawBodyOptions>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanRawBodyOptions {
    pub language: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanFormParam {
    pub key: String,
    pub value: Option<String>,
    #[serde(rename = "type", default)]
    pub param_type: String,
    #[serde(default)]
    pub disabled: bool,
}

/// An environment embedded directly inside a collection JSON export.
#[derive(Debug, Deserialize)]
pub(crate) struct PostmanEmbeddedEnvironment {
    pub name: String,
    #[serde(default)]
    pub values: Vec<PostmanEnvVar>,
}

// PostmanEnvVar is also defined in env_parser.rs — duplicate here so ast.rs
// remains self-contained. Both share the same shape.
#[derive(Debug, Deserialize)]
pub(crate) struct PostmanEnvVar {
    pub key: String,
    #[serde(default)]
    pub value: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool { true }

/// Description is either a plain string or an object with `content` + `type`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum PostmanDescription {
    String(String),
    Object { content: String, #[serde(rename = "type")] content_type: String },
}

impl PostmanDescription {
    pub(crate) fn as_str(&self) -> &str {
        match self {
            PostmanDescription::String(s) => s.as_str(),
            PostmanDescription::Object { content, .. } => content.as_str(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_collection() {
        let json = r#"{
            "info": { "name": "My API", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
            "item": [
                {
                    "name": "Get Users",
                    "request": {
                        "method": "GET",
                        "url": { "raw": "{{baseUrl}}/users", "query": [], "variable": [] },
                        "header": []
                    }
                }
            ]
        }"#;
        let col: PostmanCollection = serde_json::from_str(json).unwrap();
        assert_eq!(col.info.name, "My API");
        assert_eq!(col.item.len(), 1);
    }

    #[test]
    fn parses_v2_0_url_as_plain_string() {
        let json = r#"{
            "info": { "name": "Legacy", "schema": "https://schema.getpostman.com/json/collection/v2.0.0/collection.json" },
            "item": [{
                "name": "Ping",
                "request": { "method": "GET", "url": "https://example.com/ping", "header": [] }
            }]
        }"#;
        let col: PostmanCollection = serde_json::from_str(json).unwrap();
        match &col.item[0] {
            PostmanItem::Request(r) => assert_eq!(r.request.url.raw(), "https://example.com/ping"),
            _ => panic!("expected request item"),
        }
    }

    #[test]
    fn parses_folder_with_nested_request() {
        let json = r#"{
            "info": { "name": "API", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
            "item": [{
                "name": "Auth",
                "item": [{
                    "name": "Login",
                    "request": { "method": "POST", "url": { "raw": "{{baseUrl}}/login" }, "header": [] }
                }]
            }]
        }"#;
        let col: PostmanCollection = serde_json::from_str(json).unwrap();
        match &col.item[0] {
            PostmanItem::Folder(f) => { assert_eq!(f.name, "Auth"); assert_eq!(f.item.len(), 1); }
            _ => panic!("expected folder"),
        }
    }

    #[test]
    fn parses_bearer_auth() {
        let json = r#"{
            "info": { "name": "A", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
            "item": [],
            "auth": { "type": "bearer", "bearer": [{ "key": "token", "value": "{{myToken}}", "type": "string" }] }
        }"#;
        let col: PostmanCollection = serde_json::from_str(json).unwrap();
        let auth = col.auth.unwrap();
        assert_eq!(auth.auth_type, "bearer");
        assert_eq!(auth.bearer[0].as_str_value(), "{{myToken}}");
    }

    #[test]
    fn parses_collection_variables() {
        let json = r#"{
            "info": { "name": "A", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
            "item": [],
            "variable": [{ "key": "baseUrl", "value": "http://localhost:3000" }]
        }"#;
        let col: PostmanCollection = serde_json::from_str(json).unwrap();
        assert_eq!(col.variable[0].key, "baseUrl");
        assert_eq!(col.variable[0].value, "http://localhost:3000");
    }

    #[test]
    fn parses_embedded_environments() {
        let json = r#"{
            "info": { "name": "A", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
            "item": [],
            "environment": [
                {
                    "name": "Local",
                    "values": [
                        { "key": "baseUrl", "value": "http://localhost:3000", "enabled": true },
                        { "key": "apiKey", "value": "dev-key", "enabled": false }
                    ]
                },
                {
                    "name": "Staging",
                    "values": [
                        { "key": "baseUrl", "value": "https://staging.example.com", "enabled": true }
                    ]
                }
            ]
        }"#;
        let col: PostmanCollection = serde_json::from_str(json).unwrap();
        assert_eq!(col.environment.len(), 2);
        assert_eq!(col.environment[0].name, "Local");
        assert_eq!(col.environment[0].values.len(), 2);
        assert!(col.environment[0].values[0].enabled);
        assert!(!col.environment[0].values[1].enabled);
        assert_eq!(col.environment[1].name, "Staging");
    }

    #[test]
    fn collection_without_environment_field_defaults_to_empty() {
        let json = r#"{
            "info": { "name": "A", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
            "item": []
        }"#;
        let col: PostmanCollection = serde_json::from_str(json).unwrap();
        assert!(col.environment.is_empty());
    }
}
```

- [ ] **Step 5: Expose the `postman` module in `lib.rs`**

In `crates/rocket-import/src/lib.rs`, add:

```rust
pub(crate) mod postman;
```

- [ ] **Step 6: Check the crate compiles**

```bash
cargo check -p rocket-import
```

Expected: compiles cleanly. Dead-code warnings on the new module are expected and harmless.

- [ ] **Step 7: Run AST unit tests**

```bash
cargo test -p rocket-import postman::ast::tests
```

Expected: 7 tests pass (5 original + 2 new embedded env tests).

- [ ] **Step 8: Commit**

```bash
git add crates/rocket-import/
git commit -m "feat(import): postman AST structs + error variants + serde_json dep"
```

---

## Task 2: JSON parsers

**Files:**
- Create: `crates/rocket-import/src/postman/parser.rs`
- Create: `crates/rocket-import/src/postman/env_parser.rs`

- [ ] **Step 1: Create `crates/rocket-import/src/postman/parser.rs`**

```rust
use std::path::Path;
use crate::error::{ImportError, ImportResult};
use crate::postman::ast::PostmanCollection;

/// Read a Postman Collection JSON file and return the parsed AST.
/// Returns `ImportError::NotAPostmanCollection` if `info.schema` does not
/// contain `schema.getpostman.com`.
pub(crate) fn parse_postman_json(path: &Path) -> ImportResult<PostmanCollection> {
    let content = std::fs::read_to_string(path)
        .map_err(ImportError::IoError)?;

    let col: PostmanCollection = serde_json::from_str(&content)
        .map_err(|e| ImportError::JsonParseError {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;

    if !col.info.schema.contains("schema.getpostman.com") {
        return Err(ImportError::NotAPostmanCollection(path.to_path_buf()));
    }

    Ok(col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/postman")
            .join(name)
    }

    #[test]
    fn parses_minimal_collection_from_disk() {
        let col = parse_postman_json(&fixture("minimal-collection.json")).unwrap();
        assert_eq!(col.info.name, "Minimal API");
        assert_eq!(col.item.len(), 2);
    }

    #[test]
    fn parses_full_collection_from_disk() {
        let col = parse_postman_json(&fixture("full-collection.json")).unwrap();
        assert_eq!(col.info.name, "Full API");
        assert!(!col.item.is_empty());
        assert!(!col.variable.is_empty());
        assert!(col.auth.is_some());
        // Embedded environments must be parsed
        assert_eq!(col.environment.len(), 2);
        assert_eq!(col.environment[0].name, "Local");
        assert_eq!(col.environment[1].name, "Staging");
    }

    #[test]
    fn parses_v2_0_collection_from_disk() {
        let col = parse_postman_json(&fixture("v2.0-collection.json")).unwrap();
        assert_eq!(col.info.name, "Legacy API");
    }

    #[test]
    fn rejects_non_postman_json() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("other.json");
        std::fs::write(&path, r#"{"info": {"name": "X", "schema": "https://example.com"}}"#).unwrap();
        assert!(matches!(parse_postman_json(&path), Err(ImportError::NotAPostmanCollection(_))));
    }

    #[test]
    fn rejects_invalid_json() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "not json").unwrap();
        assert!(matches!(parse_postman_json(&path), Err(ImportError::JsonParseError { .. })));
    }
}
```

- [ ] **Step 2: Create `crates/rocket-import/src/postman/env_parser.rs`**

```rust
use serde::Deserialize;
use std::path::Path;
use crate::error::{ImportError, ImportResult};

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanEnvironment {
    pub name: String,
    #[serde(default)]
    pub values: Vec<PostmanEnvVar>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanEnvVar {
    pub key: String,
    #[serde(default)]
    pub value: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool { true }

pub(crate) fn parse_postman_environment(path: &Path) -> ImportResult<PostmanEnvironment> {
    let content = std::fs::read_to_string(path)
        .map_err(ImportError::IoError)?;

    serde_json::from_str(&content)
        .map_err(|e| ImportError::JsonParseError {
            path: path.to_path_buf(),
            message: e.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/postman")
            .join(name)
    }

    #[test]
    fn parses_environment_json() {
        let env = parse_postman_environment(&fixture("environment.json")).unwrap();
        assert_eq!(env.name, "Local");
        assert_eq!(env.values.len(), 3);
        assert_eq!(env.values[0].key, "baseUrl");
        assert_eq!(env.values[0].value, "http://localhost:3000");
        assert!(env.values[0].enabled);
        assert!(!env.values[2].enabled); // adminPass is disabled
    }

    #[test]
    fn rejects_invalid_env_json() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "{bad json}").unwrap();
        assert!(matches!(parse_postman_environment(&path), Err(ImportError::JsonParseError { .. })));
    }
}
```

- [ ] **Step 3: Run both parser test suites**

```bash
cargo test -p rocket-import postman::parser::tests
```

Expected: **FAIL** — fixture files don't exist yet. Proceed to Task 3, then re-run.

```bash
cargo test -p rocket-import postman::env_parser::tests
```

Expected: same — will pass after Task 3 creates the fixtures.

- [ ] **Step 4: Commit the parsers (fixtures come next)**

```bash
git add crates/rocket-import/src/postman/
git commit -m "feat(import): postman JSON parser + environment parser"
```

---

## Task 3: Fixture files

**Files:**
- Create: `crates/rocket-import/tests/fixtures/postman/minimal-collection.json`
- Create: `crates/rocket-import/tests/fixtures/postman/full-collection.json`
- Create: `crates/rocket-import/tests/fixtures/postman/v2.0-collection.json`
- Create: `crates/rocket-import/tests/fixtures/postman/environment.json`

- [ ] **Step 1: Create `minimal-collection.json`**

```json
{
  "info": {
    "_postman_id": "abc-001",
    "name": "Minimal API",
    "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
  },
  "item": [
    {
      "name": "Get Users",
      "request": {
        "method": "GET",
        "url": {
          "raw": "{{baseUrl}}/users",
          "query": [],
          "variable": []
        },
        "header": [
          { "key": "Content-Type", "value": "application/json", "disabled": false }
        ]
      }
    },
    {
      "name": "Health Check",
      "request": {
        "method": "GET",
        "url": { "raw": "{{baseUrl}}/health", "query": [] },
        "header": []
      }
    }
  ]
}
```

- [ ] **Step 2: Create `full-collection.json`**

```json
{
  "info": {
    "_postman_id": "def-002",
    "name": "Full API",
    "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
  },
  "auth": {
    "type": "bearer",
    "bearer": [{ "key": "token", "value": "{{authToken}}", "type": "string" }]
  },
  "variable": [
    { "key": "baseUrl", "value": "http://localhost:3000", "type": "string" },
    { "key": "authToken", "value": "", "type": "string" }
  ],
  "environment": [
    {
      "name": "Local",
      "values": [
        { "key": "baseUrl", "value": "http://localhost:3000", "enabled": true },
        { "key": "authToken", "value": "dev-token", "enabled": true }
      ]
    },
    {
      "name": "Staging",
      "values": [
        { "key": "baseUrl", "value": "https://staging.example.com", "enabled": true },
        { "key": "authToken", "value": "", "enabled": true }
      ]
    }
  ],
  "item": [
    {
      "name": "Users",
      "item": [
        {
          "name": "List Users",
          "request": {
            "method": "GET",
            "url": {
              "raw": "{{baseUrl}}/users?page=1&limit=10",
              "query": [
                { "key": "page", "value": "1", "disabled": false },
                { "key": "limit", "value": "10", "disabled": false }
              ],
              "variable": []
            },
            "header": [{ "key": "Accept", "value": "application/json" }]
          }
        },
        {
          "name": "Create User",
          "request": {
            "method": "POST",
            "url": { "raw": "{{baseUrl}}/users" },
            "header": [{ "key": "Content-Type", "value": "application/json" }],
            "body": {
              "mode": "raw",
              "raw": "{\"name\": \"Alice\", \"email\": \"alice@example.com\"}",
              "options": { "raw": { "language": "json" } }
            }
          }
        },
        {
          "name": "Get User by ID",
          "request": {
            "method": "GET",
            "url": {
              "raw": "{{baseUrl}}/users/:id",
              "variable": [{ "key": "id", "value": "123" }]
            },
            "header": []
          }
        }
      ]
    },
    {
      "name": "Auth",
      "item": [
        {
          "name": "Login",
          "request": {
            "method": "POST",
            "url": { "raw": "{{baseUrl}}/auth/login" },
            "header": [],
            "auth": {
              "type": "basic",
              "basic": [
                { "key": "username", "value": "admin", "type": "string" },
                { "key": "password", "value": "{{adminPass}}", "type": "string" }
              ]
            },
            "body": {
              "mode": "urlencoded",
              "urlencoded": [
                { "key": "grant_type", "value": "password", "disabled": false }
              ]
            }
          }
        }
      ]
    },
    {
      "name": "Upload File",
      "request": {
        "method": "POST",
        "url": { "raw": "{{baseUrl}}/upload" },
        "header": [],
        "body": {
          "mode": "formdata",
          "formdata": [
            { "key": "title", "value": "My File", "type": "text", "disabled": false },
            { "key": "file", "value": "", "type": "file", "disabled": false }
          ]
        }
      }
    }
  ]
}
```

- [ ] **Step 3: Create `v2.0-collection.json`**

```json
{
  "info": {
    "_postman_id": "ghi-003",
    "name": "Legacy API",
    "schema": "https://schema.getpostman.com/json/collection/v2.0.0/collection.json"
  },
  "item": [
    {
      "name": "Ping",
      "request": {
        "method": "GET",
        "url": "https://api.example.com/ping",
        "header": []
      }
    }
  ]
}
```

- [ ] **Step 4: Create `environment.json`**

```json
{
  "name": "Local",
  "values": [
    { "key": "baseUrl", "value": "http://localhost:3000", "enabled": true },
    { "key": "authToken", "value": "test-token-123", "enabled": true },
    { "key": "adminPass", "value": "secret", "enabled": false }
  ]
}
```

- [ ] **Step 5: Run all parser tests (should now pass)**

```bash
cargo test -p rocket-import postman::
```

Expected: all 12 tests across `ast`, `parser`, `env_parser` pass.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-import/tests/fixtures/postman/
git commit -m "test(import): postman fixture JSON files"
```
