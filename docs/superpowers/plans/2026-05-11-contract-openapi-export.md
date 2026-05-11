# Contract OpenAPI Export Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the minimal `export_as_openapi_yaml` stub with a complete OpenAPI 3.0.3 YAML export covering all contract metadata, per-endpoint parameters/body/auth, and `components.securitySchemes`.

**Architecture:** All work lives in a single private module appended to `crates/rocket-app/src/contract_service.rs`. Five focused helper functions handle URL extraction, tag derivation, content-type inference, auth scheme mapping, and description building. The main function wires them together and serialises via `serde_yaml`. No new dependencies — `serde_yaml`, `serde_json`, and `chrono` are already in `rocket-app`.

**Tech Stack:** Rust, `serde` + `serde_yaml` 0.9, `serde_json`, `chrono`, `std::collections::BTreeMap`

---

## File Modified

- `crates/rocket-app/src/contract_service.rs` — append a private `openapi` module containing all structs and helpers; replace the body of `export_as_openapi_yaml`.

---

## Before You Start

Read the spec:
```
docs/superpowers/specs/2026-05-11-contract-openapi-export-design.md
```

Verify the crate compiles cleanly before touching anything:
```bash
cargo check -p rocket-app
```
Expected: no errors.

---

## Task 1: Define private OpenAPI serde structs

**Files:**
- Modify: `crates/rocket-app/src/contract_service.rs` (append at bottom)

These structs are the data model for the YAML document. They have no runtime behaviour — no tests for this task. The compiler will catch any mistakes in later tasks.

- [ ] **Step 1: Append the private module to the file**

Add the following block at the very bottom of `crates/rocket-app/src/contract_service.rs`, after the existing `#[cfg(test)]` block:

```rust
// ─── OpenAPI export types ─────────────────────────────────────────────────

mod openapi {
    use serde::Serialize;
    use std::collections::BTreeMap;

    #[derive(Serialize)]
    pub struct OpenApiDoc {
        pub openapi: &'static str,
        pub info: InfoObject,
        pub servers: Vec<ServerObject>,
        pub paths: BTreeMap<String, BTreeMap<String, OperationObject>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub components: Option<ComponentsObject>,
    }

    #[derive(Serialize)]
    pub struct ServerObject {
        pub url: String,
    }

    #[derive(Serialize)]
    pub struct InfoObject {
        pub title: String,
        pub version: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub contact: Option<ContactObject>,
        #[serde(rename = "x-contract-id")]
        pub x_contract_id: String,
        #[serde(rename = "x-contract-status")]
        pub x_contract_status: String,
        #[serde(rename = "x-contract-enforcement-mode")]
        pub x_contract_enforcement_mode: String,
        #[serde(rename = "x-contract-provider")]
        pub x_contract_provider: PartyValue,
        #[serde(rename = "x-contract-consumers")]
        pub x_contract_consumers: Vec<PartyValue>,
        #[serde(rename = "x-contract-effective-date")]
        pub x_contract_effective_date: String,
        #[serde(rename = "x-contract-expiry-date", skip_serializing_if = "Option::is_none")]
        pub x_contract_expiry_date: Option<String>,
        #[serde(rename = "x-contract-policy")]
        pub x_contract_policy: PolicyValue,
        #[serde(rename = "x-contract-scope")]
        pub x_contract_scope: String,
        #[serde(rename = "x-contract-drift-count")]
        pub x_contract_drift_count: u32,
        #[serde(rename = "x-contract-breach-count")]
        pub x_contract_breach_count: u32,
        #[serde(rename = "x-contract-endpoint-count")]
        pub x_contract_endpoint_count: u32,
        #[serde(rename = "x-contract-document-paths", skip_serializing_if = "Vec::is_empty")]
        pub x_contract_document_paths: Vec<String>,
        #[serde(rename = "x-contract-created-by", skip_serializing_if = "Option::is_none")]
        pub x_contract_created_by: Option<String>,
        #[serde(rename = "x-contract-created-at", skip_serializing_if = "Option::is_none")]
        pub x_contract_created_at: Option<String>,
        #[serde(rename = "x-contract-updated-at", skip_serializing_if = "Option::is_none")]
        pub x_contract_updated_at: Option<String>,
    }

    #[derive(Serialize)]
    pub struct ContactObject {
        pub name: String,
    }

    #[derive(Serialize)]
    pub struct PartyValue {
        pub id: String,
        pub name: String,
        pub kind: String,
    }

    #[derive(Serialize)]
    pub struct PolicyValue {
        #[serde(rename = "breakingChangePolicy")]
        pub breaking_change_policy: String,
        #[serde(rename = "noticeDays")]
        pub notice_days: u32,
        #[serde(rename = "uptimeSla", skip_serializing_if = "Option::is_none")]
        pub uptime_sla: Option<f32>,
    }

    #[derive(Serialize)]
    pub struct ParameterObject {
        pub name: String,
        #[serde(rename = "in")]
        pub location: &'static str,
        pub schema: SchemaObject,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub example: Option<String>,
    }

    #[derive(Serialize)]
    pub struct SchemaObject {
        #[serde(rename = "type")]
        pub schema_type: &'static str,
    }

    #[derive(Serialize)]
    pub struct RequestBodyObject {
        pub required: bool,
        pub content: BTreeMap<String, MediaTypeObject>,
    }

    #[derive(Serialize)]
    pub struct MediaTypeObject {
        pub schema: SchemaObject,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub example: Option<serde_yaml::Value>,
    }

    #[derive(Serialize)]
    pub struct ResponseObject {
        pub description: &'static str,
    }

    #[derive(Serialize)]
    pub struct OperationObject {
        #[serde(rename = "operationId")]
        pub operation_id: String,
        pub summary: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        pub tags: Vec<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        pub parameters: Vec<ParameterObject>,
        #[serde(rename = "requestBody", skip_serializing_if = "Option::is_none")]
        pub request_body: Option<RequestBodyObject>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub security: Option<Vec<BTreeMap<String, Vec<String>>>>,
        pub responses: BTreeMap<String, ResponseObject>,
        #[serde(rename = "x-source-path", skip_serializing_if = "Option::is_none")]
        pub x_source_path: Option<String>,
        #[serde(rename = "x-captured-at", skip_serializing_if = "Option::is_none")]
        pub x_captured_at: Option<String>,
        #[serde(rename = "x-auth-detail", skip_serializing_if = "Option::is_none")]
        pub x_auth_detail: Option<String>,
    }

    #[derive(Serialize)]
    pub struct ComponentsObject {
        #[serde(rename = "securitySchemes")]
        pub security_schemes: BTreeMap<String, SecuritySchemeObject>,
    }

    #[derive(Serialize)]
    pub struct SecuritySchemeObject {
        #[serde(rename = "type")]
        pub scheme_type: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub scheme: Option<&'static str>,
        #[serde(rename = "in", skip_serializing_if = "Option::is_none")]
        pub location: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub flows: Option<serde_yaml::Value>,
    }
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check -p rocket-app
```
Expected: no errors. (The existing `export_as_openapi_yaml` is unchanged so far.)

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-app/src/contract_service.rs
git commit -m "feat: add private OpenAPI serde structs for contract export"
```

---

## Task 2: Helper — `extract_server_and_path`

**Files:**
- Modify: `crates/rocket-app/src/contract_service.rs` — add function inside `openapi` mod, add tests inside `#[cfg(test)]` block

Splits a raw `url_pattern` string into an optional server base URL and a path string suitable for the OpenAPI `paths` map.

- [ ] **Step 1: Write the failing tests**

Inside the existing `#[cfg(test)] mod tests { ... }` block in `contract_service.rs`, add:

```rust
// ── extract_server_and_path ──────────────────────────────────────────────
#[test]
fn extract_full_https_url() {
    let (server, path) = super::openapi::extract_server_and_path(
        "https://api.example.com/users",
    );
    assert_eq!(server, Some("https://api.example.com".to_string()));
    assert_eq!(path, "/users");
}

#[test]
fn extract_full_http_url_with_port_and_nested_path() {
    let (server, path) = super::openapi::extract_server_and_path(
        "http://localhost:3000/api/v1/users",
    );
    assert_eq!(server, Some("http://localhost:3000".to_string()));
    assert_eq!(path, "/api/v1/users");
}

#[test]
fn extract_path_only_url() {
    let (server, path) = super::openapi::extract_server_and_path("/users");
    assert_eq!(server, None);
    assert_eq!(path, "/users");
}

#[test]
fn extract_template_variable_url() {
    let (server, path) = super::openapi::extract_server_and_path("{{baseUrl}}/users");
    assert_eq!(server, Some("{{baseUrl}}".to_string()));
    assert_eq!(path, "/users");
}

#[test]
fn extract_url_strips_query_string_from_path() {
    let (server, path) = super::openapi::extract_server_and_path(
        "https://api.example.com/users?page=1&limit=20",
    );
    assert_eq!(server, Some("https://api.example.com".to_string()));
    assert_eq!(path, "/users");
}
```

- [ ] **Step 2: Run to confirm they fail**

```bash
cargo test -p rocket-app extract_server 2>&1 | tail -15
```
Expected: compile error — `extract_server_and_path` not found.

- [ ] **Step 3: Implement the function**

Inside the `mod openapi { ... }` block, add:

```rust
/// Splits a raw url_pattern into (optional_server_base, path_string).
///
/// "https://api.example.com/users"  → (Some("https://api.example.com"), "/users")
/// "http://localhost:3000/api/v1"   → (Some("http://localhost:3000"), "/api/v1")
/// "/users"                          → (None, "/users")
/// "{{baseUrl}}/users"              → (Some("{{baseUrl}}"), "/users")
pub fn extract_server_and_path(url_pattern: &str) -> (Option<String>, String) {
    // Strip query string first.
    let url = url_pattern.split('?').next().unwrap_or(url_pattern);

    // Handle http:// and https:// URLs.
    for scheme in &["https://", "http://"] {
        if let Some(after_scheme) = url.strip_prefix(scheme) {
            return if let Some(slash_pos) = after_scheme.find('/') {
                let server = format!("{}{}", scheme, &after_scheme[..slash_pos]);
                let path = after_scheme[slash_pos..].to_string();
                (Some(server), path)
            } else {
                // URL with no path component (e.g. "https://api.example.com")
                (Some(url.to_string()), "/".to_string())
            };
        }
    }

    // Handle template variable prefix like "{{baseUrl}}/users".
    if let Some(brace_end) = url.find("}/") {
        let server = url[..brace_end + 1].to_string();
        let path = url[brace_end + 1..].to_string();
        return (Some(server), path);
    }

    // Path-only URL — ensure leading slash.
    let path = if url.starts_with('/') {
        url.to_string()
    } else {
        format!("/{}", url)
    };
    (None, path)
}
```

- [ ] **Step 4: Run to confirm they pass**

```bash
cargo test -p rocket-app extract_server 2>&1 | tail -15
```
Expected: 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/contract_service.rs
git commit -m "feat: add extract_server_and_path helper for OpenAPI export"
```

---

## Task 3: Helper — `tag_from_request_path`

**Files:**
- Modify: `crates/rocket-app/src/contract_service.rs`

Derives an OpenAPI tag string from the first directory segment of the collection-relative request file path.

- [ ] **Step 1: Write failing tests**

Inside `#[cfg(test)] mod tests`, add:

```rust
// ── tag_from_request_path ────────────────────────────────────────────────
#[test]
fn tag_from_nested_path_returns_first_segment() {
    use std::path::Path;
    assert_eq!(
        super::openapi::tag_from_request_path(Path::new("users/get-users.yml")),
        Some("users".to_string()),
    );
}

#[test]
fn tag_from_deep_nested_path_returns_first_segment() {
    use std::path::Path;
    assert_eq!(
        super::openapi::tag_from_request_path(Path::new("auth/v2/login.yml")),
        Some("auth".to_string()),
    );
}

#[test]
fn tag_from_root_level_file_returns_none() {
    use std::path::Path;
    assert_eq!(
        super::openapi::tag_from_request_path(Path::new("root-request.yml")),
        None,
    );
}
```

- [ ] **Step 2: Run to confirm they fail**

```bash
cargo test -p rocket-app tag_from_request_path 2>&1 | tail -10
```
Expected: compile error — function not found.

- [ ] **Step 3: Implement**

Inside `mod openapi`, add:

```rust
/// Returns the first directory segment of a collection-relative request path
/// as an OpenAPI tag, or None for root-level files.
///
/// "users/get-users.yml" → Some("users")
/// "auth/v2/login.yml"   → Some("auth")
/// "root-request.yml"    → None
pub fn tag_from_request_path(request_path: &std::path::Path) -> Option<String> {
    use std::path::Component;
    let mut components = request_path.components();
    let first = components.next()?;
    // If there's no second component, the file is at the root (no folder tag).
    let _has_more = components.next()?;
    if let Component::Normal(s) = first {
        s.to_str().map(|s| s.to_string())
    } else {
        None
    }
}
```

- [ ] **Step 4: Run to confirm they pass**

```bash
cargo test -p rocket-app tag_from_request_path 2>&1 | tail -10
```
Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/contract_service.rs
git commit -m "feat: add tag_from_request_path helper for OpenAPI export"
```

---

## Task 4: Helper — `infer_content_type_and_example`

**Files:**
- Modify: `crates/rocket-app/src/contract_service.rs`

Given a raw body string, returns the appropriate OpenAPI content-type key and a `serde_yaml::Value` to use as the example.

- [ ] **Step 1: Write failing tests**

Inside `#[cfg(test)] mod tests`, add:

```rust
// ── infer_content_type_and_example ──────────────────────────────────────
#[test]
fn infer_json_object_body() {
    let (ct, example) = super::openapi::infer_content_type_and_example(
        r#"{"name":"Ada","email":"a@b.com"}"#,
    );
    assert_eq!(ct, "application/json");
    // Example must be a YAML mapping (not a raw string).
    assert!(example.is_mapping(), "expected mapping, got {:?}", example);
}

#[test]
fn infer_json_array_body() {
    let (ct, example) = super::openapi::infer_content_type_and_example(r#"[1,2,3]"#);
    assert_eq!(ct, "application/json");
    assert!(example.is_sequence());
}

#[test]
fn infer_plain_text_body() {
    let (ct, example) = super::openapi::infer_content_type_and_example("hello world");
    assert_eq!(ct, "text/plain");
    assert_eq!(example, serde_yaml::Value::String("hello world".into()));
}

#[test]
fn infer_xml_body_falls_back_to_text_plain() {
    let (ct, _) = super::openapi::infer_content_type_and_example("<root><id>1</id></root>");
    // XML detection is out of scope — falls back to text/plain.
    assert_eq!(ct, "text/plain");
}
```

- [ ] **Step 2: Run to confirm they fail**

```bash
cargo test -p rocket-app infer_content_type 2>&1 | tail -10
```
Expected: compile error — function not found.

- [ ] **Step 3: Implement**

Inside `mod openapi`, add:

```rust
/// Infers an OpenAPI content-type string and example value from a raw body string.
///
/// JSON objects/arrays → ("application/json", parsed Value)
/// Anything else       → ("text/plain", String Value)
pub fn infer_content_type_and_example(body: &str) -> (&'static str, serde_yaml::Value) {
    let trimmed = body.trim();
    let looks_like_json = (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'));

    if looks_like_json {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Ok(yaml_val) = serde_yaml::to_value(val) {
                return ("application/json", yaml_val);
            }
        }
    }
    ("text/plain", serde_yaml::Value::String(body.to_string()))
}
```

- [ ] **Step 4: Run to confirm they pass**

```bash
cargo test -p rocket-app infer_content_type 2>&1 | tail -10
```
Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/contract_service.rs
git commit -m "feat: add infer_content_type_and_example helper for OpenAPI export"
```

---

## Task 5: Helper — `auth_to_scheme`

**Files:**
- Modify: `crates/rocket-app/src/contract_service.rs`

Maps an `auth_type` string (as stored in `RequestSignatureSnapshot`) to an OpenAPI scheme name and `SecuritySchemeObject`.

- [ ] **Step 1: Write failing tests**

Inside `#[cfg(test)] mod tests`, add:

```rust
// ── auth_to_scheme ───────────────────────────────────────────────────────
#[test]
fn auth_bearer_maps_to_http_bearer() {
    let result = super::openapi::auth_to_scheme("bearer", "supersec…");
    let (name, scheme) = result.expect("bearer must produce a scheme");
    assert_eq!(name, "BearerAuth");
    assert_eq!(scheme.scheme_type, "http");
    assert_eq!(scheme.scheme, Some("bearer"));
}

#[test]
fn auth_basic_maps_to_http_basic() {
    let (name, scheme) = super::openapi::auth_to_scheme("basic", "alice")
        .expect("basic must produce a scheme");
    assert_eq!(name, "BasicAuth");
    assert_eq!(scheme.scheme_type, "http");
    assert_eq!(scheme.scheme, Some("basic"));
}

#[test]
fn auth_api_key_header_extracts_placement_and_name() {
    // auth_detail format for api-key: "X-Api-Key=abc… (header)"
    let (name, scheme) = super::openapi::auth_to_scheme("api-key", "X-Api-Key=abc… (header)")
        .expect("api-key must produce a scheme");
    assert_eq!(name, "ApiKeyAuth");
    assert_eq!(scheme.scheme_type, "apiKey");
    assert_eq!(scheme.location.as_deref(), Some("header"));
    assert_eq!(scheme.name.as_deref(), Some("X-Api-Key"));
}

#[test]
fn auth_api_key_query_extracts_placement() {
    let (_, scheme) = super::openapi::auth_to_scheme("api-key", "token=abc… (query)")
        .expect("api-key must produce a scheme");
    assert_eq!(scheme.location.as_deref(), Some("query"));
}

#[test]
fn auth_oauth2_maps_to_oauth2_with_empty_flows() {
    let (name, scheme) = super::openapi::auth_to_scheme("oauth2", "my-client")
        .expect("oauth2 must produce a scheme");
    assert_eq!(name, "OAuth2Auth");
    assert_eq!(scheme.scheme_type, "oauth2");
    assert!(scheme.flows.is_some());
}

#[test]
fn auth_none_returns_none() {
    assert!(super::openapi::auth_to_scheme("none", "").is_none());
}

#[test]
fn auth_inherit_returns_none() {
    assert!(super::openapi::auth_to_scheme("inherit", "").is_none());
}
```

- [ ] **Step 2: Run to confirm they fail**

```bash
cargo test -p rocket-app auth_to_scheme 2>&1 | tail -10
```
Expected: compile error — function not found.

- [ ] **Step 3: Implement**

Inside `mod openapi`, add:

```rust
/// Maps a snapshot auth_type string to an OpenAPI security scheme name and object.
/// Returns None for "none" and "inherit" (no security requirement emitted).
pub fn auth_to_scheme(
    auth_type: &str,
    auth_detail: &str,
) -> Option<(&'static str, SecuritySchemeObject)> {
    match auth_type {
        "bearer" => Some(("BearerAuth", SecuritySchemeObject {
            scheme_type: "http",
            scheme: Some("bearer"),
            location: None,
            name: None,
            flows: None,
        })),
        "basic" => Some(("BasicAuth", SecuritySchemeObject {
            scheme_type: "http",
            scheme: Some("basic"),
            location: None,
            name: None,
            flows: None,
        })),
        "api-key" => {
            // auth_detail format: "KEY_NAME=value… (header|query)"
            let placement = if auth_detail.contains("(query)") { "query" } else { "header" };
            let key_name = auth_detail
                .split('=')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("X-Api-Key")
                .to_string();
            Some(("ApiKeyAuth", SecuritySchemeObject {
                scheme_type: "apiKey",
                scheme: None,
                location: Some(placement.to_string()),
                name: Some(key_name),
                flows: None,
            }))
        }
        "oauth2" => Some(("OAuth2Auth", SecuritySchemeObject {
            scheme_type: "oauth2",
            scheme: None,
            location: None,
            name: None,
            flows: Some(serde_yaml::Value::Mapping(serde_yaml::Mapping::new())),
        })),
        "aws-sig-v4" => Some(("AwsSigV4Auth", SecuritySchemeObject {
            scheme_type: "http",
            scheme: Some("aws-sig-v4"),
            location: None,
            name: None,
            flows: None,
        })),
        "wsse" => Some(("WsseAuth", SecuritySchemeObject {
            scheme_type: "http",
            scheme: Some("wsse"),
            location: None,
            name: None,
            flows: None,
        })),
        "digest" => Some(("DigestAuth", SecuritySchemeObject {
            scheme_type: "http",
            scheme: Some("digest"),
            location: None,
            name: None,
            flows: None,
        })),
        "ntlm" => Some(("NtlmAuth", SecuritySchemeObject {
            scheme_type: "http",
            scheme: Some("ntlm"),
            location: None,
            name: None,
            flows: None,
        })),
        _ => None,
    }
}
```

- [ ] **Step 4: Run to confirm they pass**

```bash
cargo test -p rocket-app auth_to_scheme 2>&1 | tail -10
```
Expected: 7 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/contract_service.rs
git commit -m "feat: add auth_to_scheme helper for OpenAPI export"
```

---

## Task 6: Implement `export_as_openapi_yaml` and integration tests

**Files:**
- Modify: `crates/rocket-app/src/contract_service.rs`

This is the main assembly step. Write all integration tests first, then replace the existing function body.

- [ ] **Step 1: Write the 7 failing integration tests**

Inside `#[cfg(test)] mod tests`, add these tests. They all use the existing `make_contract()` / `make_snap()` helpers already defined earlier in the test module.

```rust
// ── export_as_openapi_yaml ───────────────────────────────────────────────

/// Parse YAML output from export_as_openapi_yaml and return as Value for assertions.
fn export_yaml(svc: &ContractService, contract_id: Ulid) -> serde_yaml::Value {
    let yaml = svc
        .export_as_openapi_yaml(root(), contract_id)
        .expect("export must succeed");
    serde_yaml::from_str(&yaml).expect("export must produce valid YAML")
}

// 1. Full export: contract metadata and endpoint appear in output.
#[test]
fn export_includes_contract_metadata() {
    let svc = make_service();
    let mut contract = make_contract();
    contract.status = ContractStatus::Active;
    let contract = svc.attach_contract(root(), contract, vec![], vec![]).unwrap();

    let doc = export_yaml(&svc, contract.id);

    // OpenAPI version
    assert_eq!(doc["openapi"].as_str(), Some("3.0.3"));
    // info fields
    assert_eq!(doc["info"]["title"].as_str(), Some("Test API"));
    assert_eq!(doc["info"]["version"].as_str(), Some("v1.0"));
    // x-contract-id
    assert_eq!(
        doc["info"]["x-contract-id"].as_str(),
        Some(contract.id.to_string().as_str()),
    );
    // x-contract-status
    assert_eq!(doc["info"]["x-contract-status"].as_str(), Some("active"));
    // provider
    assert_eq!(doc["info"]["x-contract-provider"]["name"].as_str(), Some("Team A"));
    // consumer
    assert_eq!(doc["info"]["x-contract-consumers"][0]["name"].as_str(), Some("Team B"));
    // policy
    assert_eq!(doc["info"]["x-contract-policy"]["noticeDays"].as_u64(), Some(30));
    // description present and non-empty
    assert!(doc["info"]["description"].as_str().map(|s| !s.is_empty()).unwrap_or(false));
    // contact
    assert_eq!(doc["info"]["contact"]["name"].as_str(), Some("Team A"));
}

// 2. URL extraction: full URL split into server + path.
#[test]
fn export_splits_full_url_into_server_and_path() {
    let svc = make_service();
    let contract = svc.attach_contract(root(), make_contract(), vec![], vec![]).unwrap();

    // Manually upsert a snapshot with a full URL.
    let mut snap = make_snap("get-users.yml");
    snap.url_pattern = "https://api.example.com/users".into();
    let mut snapshot = svc.repo.load_snapshot(root(), contract.id).unwrap();
    snapshot.upsert(snap);
    svc.repo.save_snapshot(root(), &snapshot).unwrap();

    let doc = export_yaml(&svc, contract.id);

    assert_eq!(doc["servers"][0]["url"].as_str(), Some("https://api.example.com"));
    assert!(doc["paths"]["/users"].is_mapping(), "path /users must exist");
}

// 3. Auth scheme: bearer produces BearerAuth in securitySchemes.
#[test]
fn export_bearer_auth_produces_security_scheme() {
    use rocket_collection::contract::snapshot::RequestSignatureSnapshot;

    let svc = make_service();
    let contract = svc.attach_contract(root(), make_contract(), vec![], vec![]).unwrap();

    let mut snap = make_snap("secure.yml");
    snap.url_pattern = "/secure".into();
    snap.auth_type = "bearer".into();
    snap.auth_detail = "supersec…".into();
    let mut snapshot = svc.repo.load_snapshot(root(), contract.id).unwrap();
    snapshot.upsert(snap);
    svc.repo.save_snapshot(root(), &snapshot).unwrap();

    let doc = export_yaml(&svc, contract.id);

    assert_eq!(
        doc["components"]["securitySchemes"]["BearerAuth"]["type"].as_str(),
        Some("http"),
    );
    assert_eq!(
        doc["components"]["securitySchemes"]["BearerAuth"]["scheme"].as_str(),
        Some("bearer"),
    );
    // Operation must reference the scheme.
    let security = &doc["paths"]["/secure"]["get"]["security"];
    assert!(security.is_sequence(), "security must be a sequence");
}

// 4. Body content-type: JSON body → application/json with parsed example.
#[test]
fn export_json_body_produces_request_body_with_content_type() {
    let svc = make_service();
    let contract = svc.attach_contract(root(), make_contract(), vec![], vec![]).unwrap();

    let mut snap = make_snap("create.yml");
    snap.url_pattern = "/users".into();
    snap.method = "POST".into();
    snap.body_content = Some(r#"{"name":"Ada","role":"admin"}"#.into());
    let mut snapshot = svc.repo.load_snapshot(root(), contract.id).unwrap();
    snapshot.upsert(snap);
    svc.repo.save_snapshot(root(), &snapshot).unwrap();

    let doc = export_yaml(&svc, contract.id);

    let content = &doc["paths"]["/users"]["post"]["requestBody"]["content"];
    assert!(content["application/json"].is_mapping());
    // 422 added because requestBody is present.
    assert!(doc["paths"]["/users"]["post"]["responses"]["422"].is_mapping());
}

// 5. Path grouping: GET and POST on same URL share one path item.
#[test]
fn export_groups_same_path_methods_into_one_path_item() {
    let svc = make_service();
    let contract = svc.attach_contract(root(), make_contract(), vec![], vec![]).unwrap();

    let mut get_snap = make_snap("get-users.yml");
    get_snap.url_pattern = "/users".into();
    get_snap.method = "GET".into();

    let mut post_snap = make_snap("post-users.yml");
    post_snap.url_pattern = "/users".into();
    post_snap.method = "POST".into();

    let mut snapshot = svc.repo.load_snapshot(root(), contract.id).unwrap();
    snapshot.upsert(get_snap);
    snapshot.upsert(post_snap);
    svc.repo.save_snapshot(root(), &snapshot).unwrap();

    let doc = export_yaml(&svc, contract.id);

    let path_item = &doc["paths"]["/users"];
    assert!(path_item["get"].is_mapping(), "GET must be present");
    assert!(path_item["post"].is_mapping(), "POST must be present");
}

// 6. Fallback: no snapshot → placeholder path emitted.
#[test]
fn export_with_no_snapshot_emits_placeholder_path() {
    let svc = make_service();
    // Attach a Draft contract (no snapshot taken).
    let mut contract = make_contract();
    contract.status = ContractStatus::Draft;
    let contract = svc.attach_contract(root(), contract, vec![], vec![]).unwrap();

    // Delete the snapshot so load_snapshot returns empty.
    // The simplest way: replace the saved snapshot with an empty one.
    let empty = rocket_collection::contract::snapshot::ContractSnapshot::new(contract.id);
    svc.repo.save_snapshot(root(), &empty).unwrap();

    // Also delete it by loading from repo with Ok(empty) — already handled
    // by MockContractRepo::load_snapshot returning empty when not found.

    let doc = export_yaml(&svc, contract.id);

    assert!(doc["paths"]["/example"].is_mapping(), "/example placeholder must be present");
}

// 7. Tag derivation: request in a subfolder gets a tag.
#[test]
fn export_derives_tag_from_request_path_folder() {
    let svc = make_service();
    let contract = svc.attach_contract(root(), make_contract(), vec![], vec![]).unwrap();

    let mut snap = make_snap("auth/login.yml");
    snap.url_pattern = "/login".into();
    let mut snapshot = svc.repo.load_snapshot(root(), contract.id).unwrap();
    snapshot.upsert(snap);
    svc.repo.save_snapshot(root(), &snapshot).unwrap();

    let doc = export_yaml(&svc, contract.id);

    let tags = &doc["paths"]["/login"]["get"]["tags"];
    assert!(tags.is_sequence());
    assert_eq!(tags[0].as_str(), Some("auth"));
}
```

- [ ] **Step 2: Run to confirm they fail**

```bash
cargo test -p rocket-app export_as_openapi_yaml 2>&1 | tail -20
```
Expected: all 7 tests fail — the current implementation returns only title/version/stub paths.

- [ ] **Step 3: Add private helper functions to `mod openapi`**

Inside `mod openapi`, add these four helpers (needed by the main function):

```rust
use rocket_collection::contract::types::{
    BreakingChangePolicy, Contract, ContractEnforcementMode, ContractParty, ContractPolicy,
    ContractScope, ContractStatus, PartyKind,
};

pub fn contract_status_str(status: &ContractStatus) -> &'static str {
    match status {
        ContractStatus::Draft => "draft",
        ContractStatus::Active => "active",
        ContractStatus::Drift => "drift",
        ContractStatus::Breach => "breach",
        ContractStatus::InReview => "in_review",
        ContractStatus::Paused => "paused",
        ContractStatus::ExpiringIn30Days => "expiring_in_30_days",
        ContractStatus::Expired => "expired",
        ContractStatus::Archived => "archived",
    }
}

pub fn enforcement_mode_str(mode: &ContractEnforcementMode) -> &'static str {
    match mode {
        ContractEnforcementMode::Informational => "informational",
        ContractEnforcementMode::Warn => "warn",
        ContractEnforcementMode::Block => "block",
    }
}

pub fn scope_str(scope: &ContractScope) -> String {
    match scope {
        ContractScope::Collection => "collection".into(),
        ContractScope::Folder { rel_path } => format!("folder:{}", rel_path.display()),
        ContractScope::Request { rel_path } => format!("request:{}", rel_path.display()),
    }
}

pub fn party_kind_str(kind: &PartyKind) -> &'static str {
    match kind {
        PartyKind::Team => "team",
        PartyKind::Company => "company",
        PartyKind::Service => "service",
    }
}

pub fn breaking_policy_str(policy: &BreakingChangePolicy) -> &'static str {
    match policy {
        BreakingChangePolicy::Strict => "strict",
        BreakingChangePolicy::Lenient => "lenient",
        BreakingChangePolicy::AdditiveOk => "additive_ok",
    }
}

pub fn build_description(contract: &Contract) -> String {
    let consumers_str: String = if contract.consumers.is_empty() {
        "—".into()
    } else {
        contract.consumers
            .iter()
            .map(|c| format!("{} ({})", c.name, party_kind_str(&c.kind)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let expiry = contract
        .expiry_date
        .map(|d| d.to_string())
        .unwrap_or_else(|| "—".into());
    let sla = contract
        .policy
        .uptime_sla
        .map(|s| format!("{:.1}%", s))
        .unwrap_or_else(|| "none".into());
    let attachments = if contract.document_paths.is_empty() {
        String::new()
    } else {
        let list = contract
            .document_paths
            .iter()
            .filter_map(|p| p.to_str())
            .map(|s| format!("- {}", s))
            .collect::<Vec<_>>()
            .join("\n");
        format!("\n\n## Attachments\n{}", list)
    };
    let created_suffix = match (&contract.created_by, &contract.created_at) {
        (Some(by), Some(at)) => format!(" — created by {} on {}", by, at.to_rfc3339()),
        (Some(by), None) => format!(" — created by {}", by),
        (None, Some(at)) => format!(" — created on {}", at.to_rfc3339()),
        _ => String::new(),
    };

    format!(
        "# {title} v{version}\n\n\
         **Provider:** {provider} ({provider_kind})\n\
         **Consumers:** {consumers}\n\
         **Status:** {status}  |  **Enforcement:** {enforcement}\n\
         **Effective:** {effective}  |  **Expires:** {expiry}\n\
         **Scope:** {scope}\n\
         **Drift:** {drift} changes  |  **Breach:** {breach} breaking\n\n\
         ## Policy\n\
         - Breaking change policy: {policy}\n\
         - Notice period: {notice} days\n\
         - Uptime SLA: {sla}\
         {attachments}\n\n\
         *Exported from Rocket API{created_suffix}*",
        title = contract.title,
        version = contract.version,
        provider = contract.provider.name,
        provider_kind = party_kind_str(&contract.provider.kind),
        consumers = consumers_str,
        status = contract_status_str(&contract.status),
        enforcement = enforcement_mode_str(&contract.enforcement_mode),
        effective = contract.effective_date,
        expiry = expiry,
        scope = scope_str(&contract.scope),
        drift = contract.drift_count,
        breach = contract.breach_count,
        policy = breaking_policy_str(&contract.policy.breaking_change_policy),
        notice = contract.policy.notice_days,
        sla = sla,
        attachments = attachments,
        created_suffix = created_suffix,
    )
}

impl From<&ContractParty> for PartyValue {
    fn from(p: &ContractParty) -> Self {
        PartyValue {
            id: p.id.clone(),
            name: p.name.clone(),
            kind: party_kind_str(&p.kind).to_string(),
        }
    }
}

impl From<&ContractPolicy> for PolicyValue {
    fn from(p: &ContractPolicy) -> Self {
        PolicyValue {
            breaking_change_policy: breaking_policy_str(&p.breaking_change_policy).to_string(),
            notice_days: p.notice_days,
            uptime_sla: p.uptime_sla,
        }
    }
}
```

- [ ] **Step 4: Replace `export_as_openapi_yaml` body**

Find the existing function in `ContractService`:

```rust
pub fn export_as_openapi_yaml(
    &self,
    collection_root: &std::path::Path,
    id: ulid::Ulid,
) -> ContractResult<String> {
```

Replace everything from the opening `{` to the closing `}` of that function with:

```rust
{
    use openapi::*;
    use std::collections::BTreeMap;

    let contract = self.repo.load_contract(collection_root, id)?;
    let snapshot = self.repo.load_snapshot(collection_root, id).ok();

    // ── info ──────────────────────────────────────────────────────────────
    let info = InfoObject {
        title: contract.title.clone(),
        version: contract.version.clone(),
        description: Some(build_description(&contract)),
        contact: Some(ContactObject { name: contract.provider.name.clone() }),
        x_contract_id: contract.id.to_string(),
        x_contract_status: contract_status_str(&contract.status).to_string(),
        x_contract_enforcement_mode: enforcement_mode_str(&contract.enforcement_mode).to_string(),
        x_contract_provider: PartyValue::from(&contract.provider),
        x_contract_consumers: contract.consumers.iter().map(PartyValue::from).collect(),
        x_contract_effective_date: contract.effective_date.to_string(),
        x_contract_expiry_date: contract.expiry_date.map(|d| d.to_string()),
        x_contract_policy: PolicyValue::from(&contract.policy),
        x_contract_scope: scope_str(&contract.scope),
        x_contract_drift_count: contract.drift_count,
        x_contract_breach_count: contract.breach_count,
        x_contract_endpoint_count: contract.endpoint_count,
        x_contract_document_paths: contract
            .document_paths
            .iter()
            .filter_map(|p| p.to_str().map(|s| s.to_string()))
            .collect(),
        x_contract_created_by: contract.created_by.clone(),
        x_contract_created_at: contract.created_at.map(|dt| dt.to_rfc3339()),
        x_contract_updated_at: contract.updated_at.map(|dt| dt.to_rfc3339()),
    };

    // ── paths + servers + security schemes ────────────────────────────────
    let mut server_urls: Vec<String> = Vec::new();
    let mut paths: BTreeMap<String, BTreeMap<String, OperationObject>> = BTreeMap::new();
    let mut scheme_map: BTreeMap<String, SecuritySchemeObject> = BTreeMap::new();

    let has_snapshot = snapshot
        .as_ref()
        .map(|s| !s.entries.is_empty())
        .unwrap_or(false);

    if let Some(snap) = snapshot.filter(|s| !s.entries.is_empty()) {
        for entry in &snap.entries {
            let (server_opt, path_key) = extract_server_and_path(&entry.url_pattern);
            if let Some(srv) = server_opt {
                if !server_urls.contains(&srv) {
                    server_urls.push(srv);
                }
            }

            let method = entry.method.to_lowercase();
            let operation_id = format!("{} {}", entry.method, path_key);

            // Parameters (query then header)
            let mut parameters: Vec<ParameterObject> = entry
                .query_params
                .iter()
                .map(|qp| ParameterObject {
                    name: qp.key.clone(),
                    location: "query",
                    schema: SchemaObject { schema_type: "string" },
                    example: Some(qp.value.clone()),
                })
                .chain(entry.headers.iter().map(|h| ParameterObject {
                    name: h.key.clone(),
                    location: "header",
                    schema: SchemaObject { schema_type: "string" },
                    example: Some(h.value.clone()),
                }))
                .collect();

            // Request body
            let request_body: Option<RequestBodyObject> =
                if !entry.form_fields.is_empty() {
                    // Form body: example is a mapping of key→value.
                    let mut example_map = serde_yaml::Mapping::new();
                    for f in &entry.form_fields {
                        example_map.insert(
                            serde_yaml::Value::String(f.key.clone()),
                            serde_yaml::Value::String(f.value.clone()),
                        );
                    }
                    let mut content = BTreeMap::new();
                    content.insert(
                        "application/x-www-form-urlencoded".into(),
                        MediaTypeObject {
                            schema: SchemaObject { schema_type: "object" },
                            example: Some(serde_yaml::Value::Mapping(example_map)),
                        },
                    );
                    Some(RequestBodyObject { required: true, content })
                } else if let Some(body) = &entry.body_content {
                    let (content_type, example_val) = infer_content_type_and_example(body);
                    let mut content = BTreeMap::new();
                    content.insert(
                        content_type.to_string(),
                        MediaTypeObject {
                            schema: SchemaObject { schema_type: "object" },
                            example: Some(example_val),
                        },
                    );
                    Some(RequestBodyObject { required: true, content })
                } else {
                    None
                };

            // Security
            let scheme_info = auth_to_scheme(&entry.auth_type, &entry.auth_detail);
            let security = scheme_info.as_ref().map(|(name, _)| {
                let mut m = BTreeMap::new();
                m.insert((*name).to_string(), vec![]);
                vec![m]
            });
            if let Some((name, scheme)) = scheme_info {
                scheme_map.entry(name.to_string()).or_insert(scheme);
            }

            // Responses
            let mut responses: BTreeMap<String, ResponseObject> = BTreeMap::new();
            responses.insert("200".into(), ResponseObject { description: "OK" });
            if !matches!(entry.auth_type.as_str(), "none" | "inherit" | "") {
                responses.insert("401".into(), ResponseObject { description: "Unauthorized" });
            }
            if request_body.is_some() {
                responses.insert(
                    "422".into(),
                    ResponseObject { description: "Unprocessable Entity" },
                );
            }

            let tag = tag_from_request_path(&entry.request_path);

            let op = OperationObject {
                operation_id,
                summary: format!("{} {}", entry.method, path_key),
                tags: tag.into_iter().collect(),
                parameters,
                request_body,
                security,
                responses,
                x_source_path: entry.request_path.to_str().map(|s| s.to_string()),
                x_captured_at: Some(entry.captured_at.to_rfc3339()),
                x_auth_detail: if entry.auth_detail.is_empty() {
                    None
                } else {
                    Some(entry.auth_detail.clone())
                },
            };

            paths.entry(path_key).or_default().insert(method, op);
        }
    }

    // Fallback when no snapshot entries exist.
    if !has_snapshot {
        let mut responses = BTreeMap::new();
        responses.insert("200".into(), ResponseObject { description: "OK" });
        let mut ops = BTreeMap::new();
        ops.insert(
            "get".into(),
            OperationObject {
                operation_id: "example".into(),
                summary: "No snapshot available — contract has not been signed".into(),
                tags: vec![],
                parameters: vec![],
                request_body: None,
                security: None,
                responses,
                x_source_path: None,
                x_captured_at: None,
                x_auth_detail: None,
            },
        );
        paths.insert("/example".into(), ops);
    }

    let servers: Vec<ServerObject> =
        server_urls.into_iter().map(|url| ServerObject { url }).collect();
    let components = if scheme_map.is_empty() {
        None
    } else {
        Some(ComponentsObject { security_schemes: scheme_map })
    };

    let doc = OpenApiDoc { openapi: "3.0.3", info, servers, paths, components };
    serde_yaml::to_string(&doc).map_err(|e| ContractError::Internal(e.to_string()))
}
```

- [ ] **Step 5: Run all 7 integration tests**

```bash
cargo test -p rocket-app export 2>&1 | tail -20
```
Expected: all 7 tests pass.

- [ ] **Step 6: Run the full test suite to confirm no regressions**

```bash
cargo test -p rocket-app 2>&1 | tail -20
```
Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-app/src/contract_service.rs
git commit -m "feat: full OpenAPI 3.0.3 export with metadata, parameters, body, auth, and security schemes"
```

---

## Task 7: Final verification

- [ ] **Step 1: Run cargo check on all crates**

```bash
cargo check 2>&1 | tail -10
```
Expected: no errors.

- [ ] **Step 2: Run the full rocket-app test suite one more time**

```bash
cargo test -p rocket-app 2>&1 | tail -10
```
Expected: all tests pass, no regressions.

- [ ] **Step 3: Smoke-test from the frontend (optional but recommended)**

```bash
yarn tauri dev
```
Open the Contracts tab, right-click any contract → "Export as OpenAPI". Verify the saved YAML contains the `x-contract-*` fields, `servers`, `parameters`, and `components.securitySchemes` sections.

---

## Self-Review

**Spec coverage:**
- ✅ Full `info` block — title, version, description, contact, all 14 `x-contract-*` fields
- ✅ `servers` derived from URL extraction
- ✅ `paths` grouped by path key, methods as keys
- ✅ `parameters` — query and header with examples
- ✅ `requestBody` — JSON (`application/json`) and form (`application/x-www-form-urlencoded`)
- ✅ `security` requirement per operation when auth is non-none
- ✅ Additional responses: 401 on auth, 422 on body
- ✅ `x-source-path`, `x-captured-at`, `x-auth-detail` per operation
- ✅ `tags` from request path folder segment
- ✅ `components.securitySchemes` — all 8 auth types
- ✅ ApiKeyAuth placement parsing from `auth_detail`
- ✅ Fallback placeholder when no snapshot exists
- ✅ 7 integration tests (one per spec scenario)
- ✅ 4 URL extraction tests covering full URL, port+path, path-only, template variable, query string stripping
- ✅ 3 tag derivation tests
- ✅ 4 content-type inference tests
- ✅ 7 auth scheme mapping tests

**Placeholder scan:** No TBDs, no "similar to Task N" references. Every step shows complete code.

**Type consistency:**
- `OperationObject.x_source_path: Option<String>` — used consistently in Step 3 (helper add) and Step 4 (main function)
- `SecuritySchemeObject.scheme: Option<&'static str>` — matches struct definition in Task 1 and usage in Task 5
- `PartyValue::from(&ContractParty)` — impl added in Task 6 Step 3, used in Task 6 Step 4
- `extract_server_and_path`, `tag_from_request_path`, `infer_content_type_and_example`, `auth_to_scheme` — all inside `mod openapi`, accessed as `openapi::*` in the main function via `use openapi::*`
