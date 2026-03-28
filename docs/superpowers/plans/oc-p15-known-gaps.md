# OC-P15: Known Gaps Resolution

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve the 7 known architectural gaps from the P01-P14 implementation.

**Architecture:** Extend domain types to carry fields that were previously discarded during conversion. Migrate collection settings from JSON to YAML. Preserve non-HTTP protocol data in the folder tree.

**Tech Stack:** Rust, serde, serde_yaml

**Prerequisite:** OC-P14 complete.

---

## Task 1: Add `BodyMode::Sparql` variant

**Problem:** SPARQL bodies map to `BodyMode::Text`, losing the type distinction on roundtrip. When a domain `Body` with mode `Text` is written back to OC YAML, it emits `type: text` instead of `type: sparql`.

**Files to modify:**
- `crates/rocket-shared/src/types.rs`
- `crates/rocket-infra/src/oc_conversions.rs`

### Steps

- [ ] **1.1** In `crates/rocket-shared/src/types.rs`, add a `Sparql` variant to the `BodyMode` enum:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BodyMode {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "json")]
    Json,
    #[serde(rename = "xml")]
    Xml,
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "sparql")]
    Sparql,
    #[serde(rename = "formdata")]
    FormData,
    #[serde(rename = "binary")]
    Binary,
}
```

- [ ] **1.2** In `crates/rocket-infra/src/oc_conversions.rs`, update the `From<OcHttpRequestBody> for Body` impl. Change the `Sparql` arm (line ~147) from mapping to `BodyMode::Text` to `BodyMode::Sparql`:

```rust
OcHttpRequestBody::Sparql { data } => Body {
    mode: BodyMode::Sparql,
    content: Some(data),
    form_data: None,
    file_path: None,
},
```

- [ ] **1.3** In `crates/rocket-infra/src/oc_conversions.rs`, update the `From<Body> for OcHttpRequestBody` impl. Add a `BodyMode::Sparql` arm after the `BodyMode::Xml` arm:

```rust
BodyMode::Sparql => OcHttpRequestBody::Sparql {
    data: b.content.unwrap_or_default(),
},
```

- [ ] **1.4** Add tests in `crates/rocket-shared/src/types.rs`:

```rust
#[test]
fn body_mode_sparql_serialization() {
    let body = Body {
        mode: BodyMode::Sparql,
        content: Some("SELECT ?s WHERE { ?s ?p ?o }".into()),
        form_data: None,
        file_path: None,
    };
    let json = serde_json::to_string(&body).unwrap();
    assert!(json.contains("\"mode\":\"sparql\""));
    let back: Body = serde_json::from_str(&json).unwrap();
    assert_eq!(body, back);
}
```

- [ ] **1.5** Add a roundtrip test in `crates/rocket-infra/src/oc_conversions.rs`:

```rust
#[test]
fn body_sparql_roundtrip() {
    let oc = OcHttpRequestBody::Sparql { data: "SELECT ?s WHERE { ?s ?p ?o }".into() };
    let body: Body = oc.into();
    assert_eq!(body.mode, BodyMode::Sparql);
    assert_eq!(body.content.as_deref(), Some("SELECT ?s WHERE { ?s ?p ?o }"));
    let back: OcHttpRequestBody = body.into();
    assert!(matches!(back, OcHttpRequestBody::Sparql { data } if data == "SELECT ?s WHERE { ?s ?p ?o }"));
}
```

- [ ] **1.6** Run `cargo test -p rocket-shared -p rocket-infra` and verify all tests pass.

**Commit:** `feat(oc): add BodyMode::Sparql to preserve SPARQL body type on roundtrip`

---

## Task 2: Extract URL params to OC params array

**Problem:** `request_to_oc_http_request()` always sets `params: Vec::new()`. The domain `Request` struct has no fields for query/path params (they live only in the URL string). On roundtrip, OC params with descriptions and disabled state are lost.

**Files to modify:**
- `crates/rocket-collection/src/request.rs`
- `crates/rocket-infra/src/oc_conversions.rs`

### Steps

- [ ] **2.1** In `crates/rocket-collection/src/request.rs`, add two new fields to `Request` after the `headers` field. Import `QueryParam` and `PathParam`:

```rust
use rocket_shared::types::{Auth, Body, Header, HttpMethod, QueryParam, PathParam};
```

Add fields to the struct:

```rust
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub query_params: Vec<QueryParam>,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub path_params: Vec<PathParam>,
```

- [ ] **2.2** Update `Request::new()` to initialise both fields to `Vec::new()`.

- [ ] **2.3** Add builder methods:

```rust
/// Builder method: add a query parameter.
pub fn with_query_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
    self.query_params.push(QueryParam {
        key: key.into(),
        value: value.into(),
        enabled: true,
        description: None,
    });
    self
}

/// Builder method: add a path parameter.
pub fn with_path_param(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
    self.path_params.push(PathParam {
        name: name.into(),
        value: value.into(),
        description: None,
    });
    self
}
```

- [ ] **2.4** In `crates/rocket-infra/src/oc_conversions.rs`, update `oc_http_request_to_request()`. After the `headers` line (~739), add param extraction using the existing `split_params` function:

```rust
let (query_params, path_params) = split_params(oc.http.params);
```

Then include them in the `Request` struct literal:

```rust
Request {
    // ... existing fields ...
    query_params,
    path_params,
    // ... rest of fields ...
}
```

- [ ] **2.5** In `crates/rocket-infra/src/oc_conversions.rs`, update `request_to_oc_http_request()`. Replace the `params: Vec::new()` line (~803) with:

```rust
params: merge_params(&req.query_params, &req.path_params),
```

Note: `req.query_params` and `req.path_params` must be read before `req` is consumed. Extract them before building `OcHttpRequestDetails`:

```rust
let oc_params = merge_params(&req.query_params, &req.path_params);
```

Then use `params: oc_params` in the struct literal.

- [ ] **2.6** Add a roundtrip test in `crates/rocket-infra/src/oc_conversions.rs`:

```rust
#[test]
fn params_survive_roundtrip() {
    let yaml = r#"
info:
  name: Parameterised
  type: http
http:
  method: GET
  url: "https://api.example.com/users/:id"
  params:
    - name: page
      value: "1"
      type: query
      description: Page number
    - name: id
      value: "42"
      type: path
    - name: limit
      value: "10"
      type: query
      disabled: true
"#;
    let oc: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
    let req = oc_http_request_to_request(oc);
    assert_eq!(req.query_params.len(), 2);
    assert_eq!(req.path_params.len(), 1);
    assert_eq!(req.query_params[0].key, "page");
    assert!(req.query_params[0].description.is_some());
    assert!(!req.query_params[1].enabled);
    assert_eq!(req.path_params[0].name, "id");

    let back = request_to_oc_http_request(req);
    assert_eq!(back.http.params.len(), 3);
    assert_eq!(back.http.params[0].param_type, Some("query".into()));
    assert_eq!(back.http.params[2].param_type, Some("path".into()));
}
```

- [ ] **2.7** Run `cargo test -p rocket-collection -p rocket-infra` and verify all tests pass (including the backward-compat test that deserialises old JSON without the new fields).

**Commit:** `feat(oc): carry query_params and path_params on Request for lossless OC roundtrip`

---

## Task 3: Preserve runtime auth on roundtrip

**Problem:** `request_to_oc_http_request()` always sets `runtime.auth` to `None`. If an OC YAML file has `runtime.auth`, that value is parsed but discarded when converting back.

**Files to modify:**
- `crates/rocket-collection/src/request.rs`
- `crates/rocket-infra/src/oc_conversions.rs`

### Steps

- [ ] **3.1** In `crates/rocket-collection/src/request.rs`, add a new field to `Request` after the `variables` field:

```rust
/// Auth override applied at runtime (e.g. runtime.auth in OC YAML).
#[serde(default, skip_serializing_if = "Option::is_none")]
pub runtime_auth: Option<Auth>,
```

- [ ] **3.2** Update `Request::new()` to initialise `runtime_auth: None`.

- [ ] **3.3** In `crates/rocket-infra/src/oc_conversions.rs`, update `oc_http_request_to_request()`. After the `variables` extraction (~750), add:

```rust
let runtime_auth = oc.runtime.as_ref()
    .and_then(|r| r.auth.clone())
    .map(Auth::from);
```

Include `runtime_auth` in the `Request` struct literal.

- [ ] **3.4** In `crates/rocket-infra/src/oc_conversions.rs`, update `request_to_oc_http_request()`. Change the `has_runtime` check to also consider `runtime_auth`:

```rust
let has_runtime = !scripts.is_empty()
    || !req.assertions.is_empty()
    || !actions.is_empty()
    || !req.variables.is_empty()
    || req.runtime_auth.is_some();
```

Replace the `auth: None` line inside `OcHttpRequestRuntime` with:

```rust
auth: req.runtime_auth.map(OcAuth::from),
```

Note: `req.runtime_auth` must be read before `req` is moved. Extract it early alongside other fields.

- [ ] **3.5** Add a roundtrip test:

```rust
#[test]
fn runtime_auth_survives_roundtrip() {
    let yaml = r#"
info:
  name: Runtime Auth
  type: http
http:
  method: GET
  url: "https://api.example.com"
runtime:
  auth:
    type: bearer
    token: runtime-token
"#;
    let oc: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
    let req = oc_http_request_to_request(oc);
    assert!(req.runtime_auth.is_some());
    match req.runtime_auth.as_ref().unwrap() {
        Auth::Bearer { token } => assert_eq!(token, "runtime-token"),
        _ => panic!("expected Bearer"),
    }

    let back = request_to_oc_http_request(req);
    let rt = back.runtime.unwrap();
    assert!(rt.auth.is_some());
}
```

- [ ] **3.6** Run `cargo test -p rocket-collection -p rocket-infra` and verify all tests pass.

**Commit:** `feat(oc): preserve runtime.auth on Request for lossless OC roundtrip`

---

## Task 4: Preserve request-level settings on roundtrip

**Problem:** `request_to_oc_http_request()` always sets `settings: None`. OC YAML `settings` (encodeUrl, timeout, followRedirects, maxRedirects) are lost.

**Files to modify:**
- `crates/rocket-shared/src/types.rs` (new `RequestSettings` type)
- `crates/rocket-collection/src/request.rs`
- `crates/rocket-infra/src/oc_conversions.rs`

### Steps

- [ ] **4.1** In `crates/rocket-shared/src/types.rs`, add a new `RequestSettings` struct after the `Auth` enum (before the `#[cfg(test)]` block). This mirrors `OcHttpRequestSettings` but uses domain-friendly types:

```rust
// ============================================================
// RequestSettings
// ============================================================

/// Request-level execution settings.
/// Values are optional; `None` means "inherit from collection/folder".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encode_url: Option<RequestSettingValue<bool>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<RequestSettingValue<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_redirects: Option<RequestSettingValue<bool>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_redirects: Option<RequestSettingValue<f64>>,
}

/// A setting value that can be a concrete value or "inherit".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestSettingValue<T> {
    Value(T),
    Inherit(String),
}
```

- [ ] **4.2** In `crates/rocket-collection/src/request.rs`, add `RequestSettings` to the import and add a new field to `Request` after `runtime_auth` (or after `variables` if Task 3 is not yet applied):

```rust
use rocket_shared::types::{Auth, Body, Header, HttpMethod, QueryParam, PathParam, RequestSettings};
```

```rust
/// Request-level execution settings (timeout, encode URL, etc.).
#[serde(default, skip_serializing_if = "Option::is_none")]
pub settings: Option<RequestSettings>,
```

- [ ] **4.3** Update `Request::new()` to initialise `settings: None`.

- [ ] **4.4** In `crates/rocket-infra/src/oc_conversions.rs`, add conversion functions between `OcHttpRequestSettings` and `RequestSettings`. Place them after the body conversion section:

```rust
// ============================================================
// Settings conversions
// ============================================================

use rocket_shared::types::RequestSettings;
use rocket_shared::types::RequestSettingValue;

fn oc_settings_to_domain(oc: OcHttpRequestSettings) -> RequestSettings {
    RequestSettings {
        encode_url: oc.encode_url.map(inheritable_bool_to_domain),
        timeout: oc.timeout.map(inheritable_number_to_domain),
        follow_redirects: oc.follow_redirects.map(inheritable_bool_to_domain),
        max_redirects: oc.max_redirects.map(inheritable_number_to_domain),
    }
}

fn domain_settings_to_oc(s: RequestSettings) -> OcHttpRequestSettings {
    OcHttpRequestSettings {
        encode_url: s.encode_url.map(domain_bool_to_inheritable),
        timeout: s.timeout.map(domain_number_to_inheritable),
        follow_redirects: s.follow_redirects.map(domain_bool_to_inheritable),
        max_redirects: s.max_redirects.map(domain_number_to_inheritable),
    }
}

fn inheritable_bool_to_domain(ib: InheritableBoolean) -> RequestSettingValue<bool> {
    match ib {
        InheritableBoolean::Value(v) => RequestSettingValue::Value(v),
        InheritableBoolean::Inherit(s) => RequestSettingValue::Inherit(s),
    }
}

fn inheritable_number_to_domain(in_: InheritableNumber) -> RequestSettingValue<f64> {
    match in_ {
        InheritableNumber::Value(v) => RequestSettingValue::Value(v),
        InheritableNumber::Inherit(s) => RequestSettingValue::Inherit(s),
    }
}

fn domain_bool_to_inheritable(v: RequestSettingValue<bool>) -> InheritableBoolean {
    match v {
        RequestSettingValue::Value(b) => InheritableBoolean::Value(b),
        RequestSettingValue::Inherit(s) => InheritableBoolean::Inherit(s),
    }
}

fn domain_number_to_inheritable(v: RequestSettingValue<f64>) -> InheritableNumber {
    match v {
        RequestSettingValue::Value(n) => InheritableNumber::Value(n),
        RequestSettingValue::Inherit(s) => InheritableNumber::Inherit(s),
    }
}
```

- [ ] **4.5** Update `oc_http_request_to_request()`. After existing field extraction, add:

```rust
let settings = oc.settings.map(oc_settings_to_domain);
```

Include `settings` in the `Request` struct literal.

- [ ] **4.6** Update `request_to_oc_http_request()`. Replace `settings: None` with:

```rust
settings: req.settings.map(domain_settings_to_oc),
```

Note: extract `req.settings` before `req` is consumed.

- [ ] **4.7** Add a roundtrip test:

```rust
#[test]
fn settings_survive_roundtrip() {
    let yaml = r#"
info:
  name: With Settings
  type: http
http:
  method: GET
  url: "https://api.example.com"
settings:
  encodeUrl: true
  timeout: 30000
  followRedirects: inherit
  maxRedirects: 5
"#;
    let oc: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
    let req = oc_http_request_to_request(oc);
    let s = req.settings.as_ref().unwrap();
    assert!(matches!(s.encode_url, Some(RequestSettingValue::Value(true))));
    assert!(matches!(s.timeout, Some(RequestSettingValue::Value(t)) if (t - 30000.0).abs() < f64::EPSILON));
    assert!(matches!(s.follow_redirects, Some(RequestSettingValue::Inherit(_))));

    let back = request_to_oc_http_request(req);
    let os = back.settings.unwrap();
    assert_eq!(os.encode_url, Some(InheritableBoolean::Value(true)));
    assert_eq!(os.timeout, Some(InheritableNumber::Value(30000.0)));
    assert_eq!(os.follow_redirects, Some(InheritableBoolean::Inherit("inherit".into())));
    assert_eq!(os.max_redirects, Some(InheritableNumber::Value(5.0)));
}
```

- [ ] **4.8** Run `cargo test -p rocket-shared -p rocket-collection -p rocket-infra` and verify all tests pass.

**Commit:** `feat(oc): preserve request-level settings (timeout, encodeUrl, etc.) on roundtrip`

---

## Task 5: Migrate collection settings from JSON to YAML

**Problem:** `get_settings()` / `save_settings()` in `FsCollectionRepo` read and write `collection.json` using `serde_json`. The rest of the system uses `opencollection.yml`. Settings should be read from the `request` section of `opencollection.yml`.

**Files to modify:**
- `crates/rocket-infra/src/fs_collection_repo.rs`

### Steps

- [ ] **5.1** In `crates/rocket-infra/src/fs_collection_repo.rs`, change `settings_path()` to return the path to `opencollection.yml` instead of `collection.json`:

```rust
fn settings_path(&self, name: &str) -> PathBuf {
    self.collection_path(name).join("opencollection.yml")
}
```

- [ ] **5.2** Rewrite `get_settings()` to read from `opencollection.yml` and convert via the existing `oc_collection_to_collection` logic. Import `OcCollection` and the conversion helpers:

```rust
fn get_settings(&self, name: &str) -> DomainResult<CollectionSettings> {
    let path = self.settings_path(name);
    if !path.exists() {
        return Ok(CollectionSettings::default());
    }
    let content = fs::read_to_string(&path)?;
    let oc: OcCollection = serde_yaml::from_str(&content)
        .map_err(|e| DomainError::Internal(format!("Failed to parse opencollection.yml: {e}")))?;

    // Convert request defaults to settings.
    use crate::oc_conversions::{split_params, merge_params};
    use rocket_environment::variable::Variable;
    if let Some(defaults) = oc.request {
        Ok(CollectionSettings {
            description: oc.docs,
            auth: defaults.auth.map(rocket_shared::types::Auth::from),
            headers: defaults.headers
                .unwrap_or_default()
                .into_iter()
                .map(rocket_shared::types::Header::from)
                .collect(),
            variables: defaults.variables
                .unwrap_or_default()
                .into_iter()
                .map(|v| {
                    let var: Variable = v.into();
                    CollectionVariable {
                        key: var.key,
                        value: var.value.clone(),
                        initial_value: var.value,
                        enabled: var.enabled,
                        secret: var.secret,
                    }
                })
                .collect(),
        })
    } else {
        Ok(CollectionSettings {
            description: oc.docs,
            ..CollectionSettings::default()
        })
    }
}
```

- [ ] **5.3** Rewrite `save_settings()` to read the existing `opencollection.yml`, update only the `request` and `docs` sections, then write back:

```rust
fn save_settings(&self, name: &str, settings: &CollectionSettings) -> DomainResult<()> {
    let path = self.settings_path(name);

    // Read existing YAML to preserve other fields.
    let mut oc: OcCollection = if path.exists() {
        let content = fs::read_to_string(&path)?;
        serde_yaml::from_str(&content)
            .map_err(|e| DomainError::Internal(format!("Failed to parse opencollection.yml: {e}")))?
    } else {
        OcCollection {
            opencollection: Some("0.1".into()),
            uid: Some(uuid::Uuid::new_v4().to_string()),
            info: Some(OcInfo { name: name.into(), summary: None, version: None, authors: None }),
            config: None,
            items: None,
            request: None,
            docs: None,
            bundled: None,
            extensions: None,
        }
    };

    // Update request defaults.
    use crate::opencollection::{OcRequestDefaults, OcHttpRequestHeader, OcVariable};
    use rocket_shared::variable_value::VariableValue;

    let has_defaults = !settings.headers.is_empty()
        || settings.auth.is_some()
        || !settings.variables.is_empty();
    oc.request = if has_defaults {
        Some(OcRequestDefaults {
            headers: if settings.headers.is_empty() {
                None
            } else {
                Some(settings.headers.iter().cloned().map(OcHttpRequestHeader::from).collect())
            },
            metadata: None,
            auth: settings.auth.clone().map(crate::opencollection::OcAuth::from),
            variables: if settings.variables.is_empty() {
                None
            } else {
                Some(settings.variables.iter().map(|cv| {
                    OcVariable {
                        name: cv.key.clone(),
                        value: Some(VariableValue::simple(&cv.value)),
                        description: None,
                        disabled: if cv.enabled { None } else { Some(true) },
                    }
                }).collect())
            },
            scripts: None,
            settings: None,
        })
    } else {
        None
    };
    oc.docs = settings.description.clone();

    let yaml = serde_yaml::to_string(&oc)
        .map_err(|e| DomainError::Internal(format!("Failed to serialize opencollection.yml: {e}")))?;
    fs::write(&path, yaml)?;

    // Clean up legacy collection.json if it exists.
    let legacy = self.collection_path(name).join("collection.json");
    if legacy.exists() {
        let _ = fs::remove_file(&legacy);
    }

    Ok(())
}
```

- [ ] **5.4** Update `is_request_file()` to NOT count `collection.json` (already excluded, but verify it is still in the exclusion list).

- [ ] **5.5** Update existing tests in `fs_collection_repo.rs`. The `settings_roundtrip` test should still pass since it uses the public `get_settings` / `save_settings` API. Verify the settings are now inside `opencollection.yml`:

```rust
#[test]
fn settings_stored_in_opencollection_yml() {
    use rocket_shared::types::{Auth, Header};

    let (dir, repo) = setup();
    repo.create("my-api").unwrap();

    let settings = CollectionSettings {
        description: Some("My API docs".into()),
        auth: Some(Auth::Bearer { token: "tok".into() }),
        headers: vec![Header::new("X-Tenant", "acme")],
        variables: vec![],
    };
    repo.save_settings("my-api", &settings).unwrap();

    // Settings should be in opencollection.yml, not collection.json.
    assert!(!dir.path().join("my-api/collection.json").exists());
    let content = fs::read_to_string(dir.path().join("my-api/opencollection.yml")).unwrap();
    assert!(content.contains("X-Tenant"));
    assert!(content.contains("bearer"));

    let loaded = repo.get_settings("my-api").unwrap();
    assert_eq!(loaded.auth, settings.auth);
    assert_eq!(loaded.headers.len(), 1);
    assert_eq!(loaded.description, Some("My API docs".into()));
}
```

- [ ] **5.6** Add a backward-compat migration test: if `collection.json` exists but `opencollection.yml` has no `request` section, `get_settings()` returns defaults (the old JSON is not automatically read). Document that migration is handled by `save_settings()` which removes the JSON file.

- [ ] **5.7** Run `cargo test -p rocket-infra` and verify all tests pass.

**Commit:** `feat(oc): migrate collection settings from collection.json to opencollection.yml`

---

## Task 6: Preserve non-HTTP protocol items in folder tree

**Problem:** `oc_folder_to_folder()` and `oc_collection_to_collection()` skip non-HTTP items (GraphQL, gRPC, WebSocket) with `_ => None`. The `CollectionItem` enum only has `Request` and `Folder` variants.

**Files to modify:**
- `crates/rocket-collection/src/folder.rs`
- `crates/rocket-infra/src/oc_conversions.rs`

### Steps

- [ ] **6.1** In `crates/rocket-collection/src/folder.rs`, add a new variant to `CollectionItem` to hold opaque non-HTTP protocol data:

```rust
/// Raw YAML data for a non-HTTP protocol item (GraphQL, gRPC, WebSocket).
/// Stored losslessly so it survives folder-tree roundtrips.
OpaqueItem(OpaqueProtocolItem),
```

Add the supporting struct:

```rust
/// An opaque protocol item stored as raw YAML for lossless roundtrip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpaqueProtocolItem {
    /// The protocol type: "graphql", "grpc", "websocket".
    pub protocol: String,
    /// The display name (from info.name).
    pub name: String,
    /// The raw YAML value, preserved for lossless roundtrip.
    pub raw: serde_yaml::Value,
}
```

- [ ] **6.2** Update any helper methods on `Folder` that count requests (like `request_count()`) to skip `OpaqueItem` variants (or count them separately if desired).

- [ ] **6.3** In `crates/rocket-infra/src/oc_conversions.rs`, update `oc_folder_to_folder()`. Replace `_ => None` with conversion to `OpaqueItem`:

```rust
OcItem::GraphQL(ref gql) => {
    serde_yaml::to_value(&item).ok().map(|raw| {
        CollectionItem::OpaqueItem(OpaqueProtocolItem {
            protocol: "graphql".into(),
            name: gql.info.name.clone(),
            raw,
        })
    })
}
OcItem::Grpc(ref grpc) => {
    serde_yaml::to_value(&item).ok().map(|raw| {
        CollectionItem::OpaqueItem(OpaqueProtocolItem {
            protocol: "grpc".into(),
            name: grpc.info.name.clone(),
            raw,
        })
    })
}
OcItem::WebSocket(ref ws) => {
    serde_yaml::to_value(&item).ok().map(|raw| {
        CollectionItem::OpaqueItem(OpaqueProtocolItem {
            protocol: "websocket".into(),
            name: ws.info.name.clone(),
            raw,
        })
    })
}
OcItem::ScriptFile(_) => None,
```

Apply the same change to `oc_collection_to_collection()`.

- [ ] **6.4** Update `folder_to_oc_folder()` and `collection_to_oc_collection()` to convert `OpaqueItem` back:

```rust
CollectionItem::OpaqueItem(opaque) => {
    serde_yaml::from_value::<OcItem>(opaque.raw).unwrap_or_else(|_| {
        // Fallback: should not happen since we stored valid YAML.
        OcItem::Folder(OcFolder {
            info: OcFolderInfo {
                name: opaque.name,
                uid: None,
                description: None,
                folder_type: Some("folder".into()),
                seq: None,
                tags: Vec::new(),
            },
            items: None,
            request: None,
            docs: None,
        })
    })
}
```

- [ ] **6.5** Add a roundtrip test:

```rust
#[test]
fn non_http_items_preserved_in_folder_roundtrip() {
    let yaml = r#"
info:
  name: Mixed
  type: folder
items:
  - info:
      name: Get Users
      type: http
    http:
      method: GET
      url: "https://api.example.com/users"
  - info:
      name: GQL Query
      type: graphql
    graphql:
      url: "https://api.example.com/graphql"
      body:
        query: "query { users { id } }"
"#;
    let oc: OcFolder = serde_yaml::from_str(yaml).unwrap();
    let folder = oc_folder_to_folder(oc);
    assert_eq!(folder.items.len(), 2);
    assert!(matches!(&folder.items[0], CollectionItem::Request(_)));
    assert!(matches!(&folder.items[1], CollectionItem::OpaqueItem(o) if o.protocol == "graphql"));

    let back = folder_to_oc_folder(folder);
    let items = back.items.unwrap();
    assert_eq!(items.len(), 2);
    assert!(matches!(&items[0], OcItem::Http(_)));
    assert!(matches!(&items[1], OcItem::GraphQL(_)));
}
```

- [ ] **6.6** Run `cargo test -p rocket-collection -p rocket-infra` and verify all tests pass.

**Commit:** `feat(oc): preserve non-HTTP protocol items (GraphQL, gRPC, WebSocket) in folder tree`

---

## Task 7: Use typed domain structs for OAuth2 sub-fields instead of `serde_json::Value`

**Problem:** In `OcAuthTyped::OAuth2`, the `additional_parameters`, `token_config`, and `settings` fields are `Option<serde_json::Value>`. The conversion code uses `serde_json::from_value` / `serde_json::to_value` as a bridge. The domain types `OAuth2AdditionalParameters`, `OAuth2TokenConfig`, and `OAuth2Settings` already exist and should be used directly.

**Files to modify:**
- `crates/rocket-infra/src/opencollection.rs`
- `crates/rocket-infra/src/oc_conversions.rs`

### Steps

- [ ] **7.1** In `crates/rocket-infra/src/opencollection.rs`, change the `OcAuthTyped::OAuth2` variant's three fields from `serde_json::Value` to the domain types. First, add the imports:

```rust
use rocket_shared::oauth2::{OAuth2AdditionalParameters, OAuth2TokenConfig, OAuth2Settings};
```

Then change the fields:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
additional_parameters: Option<OAuth2AdditionalParameters>,
#[serde(default, skip_serializing_if = "Option::is_none")]
token_config: Option<OAuth2TokenConfig>,
#[serde(default, skip_serializing_if = "Option::is_none")]
settings: Option<OAuth2Settings>,
```

- [ ] **7.2** In `crates/rocket-infra/src/oc_conversions.rs`, simplify the `From<OcAuthTyped> for Auth` impl. Remove the `serde_json::from_value` bridge for these three fields. Replace:

```rust
let add_params = additional_parameters
    .and_then(|v| serde_json::from_value(v).ok());
let tok_cfg = token_config
    .and_then(|v| serde_json::from_value(v).ok());
let setts = settings
    .and_then(|v| serde_json::from_value(v).ok());
```

With direct assignment:

```rust
let add_params = additional_parameters;
let tok_cfg = token_config;
let setts = settings;
```

- [ ] **7.3** In `crates/rocket-infra/src/oc_conversions.rs`, simplify `domain_oauth2_to_oc_fields()`. The return type's last three elements change from `Option<serde_json::Value>` to the typed options. Update the function signature:

```rust
fn domain_oauth2_to_oc_fields(
    flow: OAuth2Flow,
) -> (
    String,                                // flow name
    Option<String>,                        // access_token_url
    Option<String>,                        // refresh_token_url
    Option<String>,                        // authorization_url
    Option<String>,                        // callback_url
    Option<OcOAuth2Credentials>,           // credentials
    Option<OcOAuth2ResourceOwner>,         // resource_owner
    Option<String>,                        // scope
    Option<String>,                        // state
    Option<OcOAuth2PKCE>,                  // pkce
    Option<OAuth2AdditionalParameters>,    // additional_parameters
    Option<OAuth2TokenConfig>,             // token_config
    Option<OAuth2Settings>,                // settings
)
```

In each match arm, replace `additional_parameters.and_then(|v| serde_json::to_value(v).ok())` with just `additional_parameters`. Same for `token_config` and `settings`.

- [ ] **7.4** Add an import for the OAuth2 types at the top of `oc_conversions.rs` if not already present:

```rust
use rocket_shared::oauth2::{OAuth2AdditionalParameters, OAuth2TokenConfig, OAuth2Settings};
```

- [ ] **7.5** Add a roundtrip test that exercises the typed fields:

```rust
#[test]
fn oauth2_typed_subfields_roundtrip() {
    let yaml = r#"
info:
  name: OAuth2 Test
  type: http
http:
  method: GET
  url: "https://api.example.com"
  auth:
    type: oauth2
    flow: client_credentials
    accessTokenUrl: "https://auth.example.com/token"
    credentials:
      clientId: my-id
      clientSecret: my-secret
    additionalParameters:
      accessTokenRequest:
        - name: audience
          value: "https://api.example.com"
    tokenConfig:
      id: my-token
      placement:
        header: Authorization
    settings:
      autoFetchToken: true
      autoRefreshToken: false
"#;
    let oc: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
    let req = oc_http_request_to_request(oc);
    match &req.auth {
        Auth::OAuth2(OAuth2Flow::ClientCredentials {
            additional_parameters,
            token_config,
            settings,
            ..
        }) => {
            let ap = additional_parameters.as_ref().unwrap();
            assert_eq!(ap.access_token_request.as_ref().unwrap().len(), 1);
            assert_eq!(ap.access_token_request.as_ref().unwrap()[0].name, "audience");

            let tc = token_config.as_ref().unwrap();
            assert_eq!(tc.id, Some("my-token".into()));

            let s = settings.as_ref().unwrap();
            assert_eq!(s.auto_fetch_token, Some(true));
            assert_eq!(s.auto_refresh_token, Some(false));
        }
        _ => panic!("expected OAuth2 ClientCredentials"),
    }

    let back = request_to_oc_http_request(req);
    let auth = back.http.auth.unwrap();
    match auth {
        OcAuth::Typed(OcAuthTyped::OAuth2 {
            additional_parameters,
            token_config,
            settings,
            ..
        }) => {
            assert!(additional_parameters.is_some());
            assert!(token_config.is_some());
            assert!(settings.is_some());
        }
        _ => panic!("expected OcAuthTyped::OAuth2"),
    }
}
```

- [ ] **7.6** Run `cargo test -p rocket-infra -p rocket-shared` and verify all tests pass. Pay attention to existing OAuth2 YAML tests in `opencollection.rs` -- they must still deserialize correctly.

**Commit:** `refactor(oc): use typed OAuth2 domain structs instead of serde_json::Value bridge`

---

## Execution Order

Tasks are independent and can be executed in parallel, with two exceptions:
- **Task 2, 3, 4** all add fields to `Request` -- if done in parallel, merge the struct changes.
- **Task 5** depends on no specific task but should be tested after Tasks 2-4 to confirm settings integration.

Recommended serial order for a single worker:
1. Task 1 (smallest, self-contained)
2. Task 7 (self-contained refactor)
3. Task 2 (adds params to Request)
4. Task 3 (adds runtime_auth to Request)
5. Task 4 (adds settings to Request)
6. Task 6 (adds OpaqueItem to CollectionItem)
7. Task 5 (migrates settings storage)

After all tasks: run `cargo test --workspace` to confirm no regressions.

---

## Verification Checklist

- [ ] `cargo test --workspace` passes with zero failures.
- [ ] `cargo clippy --workspace` reports no new warnings.
- [ ] SPARQL body type preserved on OC YAML roundtrip.
- [ ] OC params (query + path) preserved on roundtrip.
- [ ] Runtime auth preserved on roundtrip.
- [ ] Request settings (timeout, encodeUrl, etc.) preserved on roundtrip.
- [ ] Collection settings stored in `opencollection.yml`, not `collection.json`.
- [ ] GraphQL / gRPC / WebSocket items survive folder-tree roundtrip.
- [ ] OAuth2 additional_parameters, token_config, settings use typed structs (no serde_json::Value).
- [ ] Backward compatibility: old JSON without new fields still deserializes.
