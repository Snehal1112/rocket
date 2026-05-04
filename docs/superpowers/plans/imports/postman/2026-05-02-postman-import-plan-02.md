# Postman Import — Plan 02: Converter + ImportService

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `converter/postman.rs` (pure conversion functions: Postman AST → domain types) and extend `ImportService` with `import_postman_collection` and `import_postman_environment`.

**Architecture:** Converter is pure functions — no I/O. `ImportService` uses `self.collection_repo` (already a field) for all disk writes via `save_request`, `create_folder`, and `save_settings` — **no raw YAML, no `std::fs` writes from importer code**. Environment import uses `self.env_factory.make(col_name).save(env)`. This is identical to the Bruno importer pattern.

**Tech Stack:** Rust, serde, serde_yaml

**Spec:** `docs/superpowers/specs/2026-05-02-postman-import-design.md`

**Prerequisite:** Plan 01 complete.

**Authoritative type/API reference (read these — never guess):**

| File | What's there |
|---|---|
| `crates/rocket-shared/src/types.rs` | `Auth` (variants `None`, `Basic`, `Bearer`, `ApiKey { key, value, placement }`, …), `Header` (`key`, `value`, `enabled`, `description`), `QueryParam` (`key`, `value`, `enabled`, `description`), `PathParam` (`name`, `value`, `description`), `Body` (struct: `mode: BodyMode`, `content: Option<String>`, `form_data: Option<Vec<FormDataEntry>>`, `file_path: Option<String>`), `BodyMode` (enum: `None`, `Json`, `Xml`, `Text`, `Sparql`, `FormUrlEncoded`, `FormData`, `Binary`), `FormDataEntry` (`key`, `value`, `entry_type: FormDataType`, `enabled`, `content_type`, `description`), `FormDataType` (`Text`, `File`), `HttpMethod` (impls `FromStr` returning `DomainResult<Self>` — does **not** impl `Default`) |
| `crates/rocket-collection/src/request.rs` | `Request::new(name, method, url)` constructor + builder methods (`with_header`, `with_body`, `with_auth`). Public fields include `headers: Vec<Header>`, `query_params: Vec<QueryParam>`, `path_params: Vec<PathParam>`, `body: Option<Body>`, `auth: Auth` (not `Option<Auth>` — `Auth::None` is the empty case), `description: Option<Description>`. There is **no** `params` field. |
| `crates/rocket-collection/src/settings.rs` | `CollectionVariable { key, value, initial_value, enabled, secret }`. `CollectionSettings { docs, auth, headers, variables }`. |
| `crates/rocket-collection/src/repository.rs` | Trait methods used: `create(name) -> DomainResult<Collection>`, `save_request(collection, path, request) -> DomainResult<String>`, `create_folder(collection, path) -> DomainResult<()>`, `save_settings(name, settings) -> DomainResult<()>`. |
| `crates/rocket-environment/src/variable.rs` | `Variable::new(key, value)` returns enabled variable. Public `enabled: bool` field — set after construction. |
| `crates/rocket-environment/src/environment.rs` | `Environment::new(name)`, `env.set_variable(var)`. |
| `crates/rocket-import/src/importer.rs` | `ImportService` already holds `self.collection_repo`, `self.env_factory`, `self.workspace_path`. Has `self.resolve_collection_name(&str) -> ImportResult<String>` for auto-rename on conflict. No `sanitize_filename` helper exists yet. |
| `crates/rocket-import/src/report.rs` | `ImportReport { total_files, imported, skipped, created_workspace, created_collections, detected_type }`. `SkippedItem { path, reason: SkipReason }`. `SkipReason::{UnsupportedRequestType(String), UnsupportedAuthType(String), ParseError(String)}`. |
| `crates/rocket-import/src/converter/request.rs` | The Bruno converter — reference pattern. |

---

## File Map

| File | Action |
|---|---|
| `crates/rocket-import/src/converter/mod.rs` | Modify — add `pub(crate) mod postman` |
| `crates/rocket-import/src/converter/postman.rs` | Create |
| `crates/rocket-import/src/importer.rs` | Modify — add two public methods + helpers |
| `crates/rocket-import/tests/postman_integration_test.rs` | Create |

---

## Task 1: Converter — auth, headers, params, body, variables

**Files:**
- Modify: `crates/rocket-import/src/converter/mod.rs`
- Create: `crates/rocket-import/src/converter/postman.rs`

- [ ] **Step 1: Add `postman` module to `converter/mod.rs`**

```rust
pub(crate) mod postman;
```

- [ ] **Step 2: Create `converter/postman.rs` with full implementation**

```rust
use crate::postman::ast::*;
use crate::report::{SkipReason, SkippedItem};
use rocket_collection::settings::CollectionVariable;
use rocket_shared::types::{
    Auth, Body, BodyMode, FormDataEntry, FormDataType, Header, PathParam, QueryParam,
};

/// Convert a `PostmanAuth` to a domain `Auth`.
/// Returns `None` only for `oauth2` (unsupported — caller records skip).
/// `noauth` and unknown types map to `Some(Auth::None)`.
pub(crate) fn convert_auth(auth: &PostmanAuth) -> Option<Auth> {
    match auth.auth_type.as_str() {
        "bearer" => Some(Auth::Bearer {
            token: find_kv(&auth.bearer, "token"),
        }),
        "basic" => Some(Auth::Basic {
            username: find_kv(&auth.basic, "username"),
            password: find_kv(&auth.basic, "password"),
        }),
        "apikey" => {
            let placement = match find_kv(&auth.apikey, "in").as_str() {
                "query" => "query".to_string(),
                _ => "header".to_string(),
            };
            Some(Auth::ApiKey {
                key: find_kv(&auth.apikey, "key"),
                value: find_kv(&auth.apikey, "value"),
                placement,
            })
        }
        "noauth" => Some(Auth::None),
        "oauth2" => None,
        _ => Some(Auth::None),
    }
}

fn find_kv(list: &[PostmanKeyValue], key: &str) -> String {
    list.iter()
        .find(|kv| kv.key == key)
        .map(|kv| kv.as_str_value())
        .unwrap_or_default()
}

pub(crate) fn convert_headers(headers: &[PostmanHeader]) -> Vec<Header> {
    headers
        .iter()
        .map(|h| Header {
            key: h.key.clone(),
            value: h.value.clone(),
            enabled: !h.disabled,
            description: None,
        })
        .collect()
}

pub(crate) fn convert_collection_variables(vars: &[PostmanVariable]) -> Vec<CollectionVariable> {
    vars.iter()
        .map(|v| CollectionVariable {
            key: v.key.clone(),
            value: v.value.clone(),
            initial_value: v.value.clone(),
            enabled: !v.disabled,
            secret: false,
        })
        .collect()
}

/// Convert Postman query params to domain `QueryParam`. Skips entries
/// whose `key` is missing (Postman allows null keys in malformed exports).
pub(crate) fn convert_query_params(params: &[PostmanQueryParam]) -> Vec<QueryParam> {
    params
        .iter()
        .filter_map(|p| {
            p.key.as_ref().map(|k| QueryParam {
                key: k.clone(),
                value: p.value.clone().unwrap_or_default(),
                enabled: !p.disabled,
                description: None,
            })
        })
        .collect()
}

pub(crate) fn convert_path_variables(vars: &[PostmanPathVariable]) -> Vec<PathParam> {
    vars.iter()
        .map(|v| PathParam {
            name: v.key.clone(),
            value: v.value.clone().unwrap_or_default(),
            description: None,
        })
        .collect()
}

/// Convert a Postman body to a domain `Body`. Returns `None` only for
/// `mode = "file"` (unsupported binary file body). Caller records skip via
/// `body_skip_items`.
pub(crate) fn convert_body(body: &PostmanBody) -> Option<Body> {
    match body.mode.as_str() {
        "raw" => {
            let language = body
                .options
                .as_ref()
                .and_then(|o| o.raw.as_ref())
                .and_then(|r| r.language.as_deref())
                .unwrap_or("text");
            let mode = match language {
                "json" => BodyMode::Json,
                "xml" => BodyMode::Xml,
                _ => BodyMode::Text,
            };
            Some(Body {
                mode,
                content: body.raw.clone(),
                form_data: None,
                file_path: None,
            })
        }
        "urlencoded" => {
            // Encode active entries as `k=v&k2=v2` — disabled entries are
            // skipped because BodyMode::FormUrlEncoded stores a flat string.
            let encoded = body
                .urlencoded
                .iter()
                .filter(|p| !p.disabled && p.param_type != "file")
                .map(|p| format!("{}={}", p.key, p.value.clone().unwrap_or_default()))
                .collect::<Vec<_>>()
                .join("&");
            Some(Body {
                mode: BodyMode::FormUrlEncoded,
                content: Some(encoded),
                form_data: None,
                file_path: None,
            })
        }
        "formdata" => {
            // Multipart: import text entries; file entries are skipped (see body_skip_items).
            let entries = body
                .formdata
                .iter()
                .filter(|p| p.param_type != "file")
                .map(|p| FormDataEntry {
                    key: p.key.clone(),
                    value: p.value.clone().unwrap_or_default(),
                    entry_type: FormDataType::Text,
                    enabled: !p.disabled,
                    content_type: None,
                    description: None,
                })
                .collect();
            Some(Body {
                mode: BodyMode::FormData,
                content: None,
                form_data: Some(entries),
                file_path: None,
            })
        }
        "file" => None,
        _ => None,
    }
}

/// Returns skip items for body modes/entries that cannot be imported.
/// - `mode == "file"` → one `UnsupportedRequestType("file-body")` skip
/// - `formdata` entries with `param_type == "file"` → one skip per entry
pub(crate) fn body_skip_items(body: &PostmanBody, request_name: &str) -> Vec<SkippedItem> {
    let mut out = Vec::new();
    if body.mode == "file" {
        out.push(SkippedItem {
            path: request_name.to_string(),
            reason: SkipReason::UnsupportedRequestType("file-body".into()),
        });
    }
    if body.mode == "formdata" {
        for p in &body.formdata {
            if p.param_type == "file" {
                out.push(SkippedItem {
                    path: format!("{} / {}", request_name, p.key),
                    reason: SkipReason::UnsupportedRequestType("formdata-file-entry".into()),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkv(key: &str, value: serde_json::Value) -> PostmanKeyValue {
        PostmanKeyValue {
            key: key.into(),
            value,
        }
    }

    fn bearer() -> PostmanAuth {
        PostmanAuth {
            auth_type: "bearer".into(),
            bearer: vec![pkv("token", serde_json::json!("{{myToken}}"))],
            basic: vec![],
            apikey: vec![],
            oauth2: vec![],
        }
    }

    fn basic_auth() -> PostmanAuth {
        PostmanAuth {
            auth_type: "basic".into(),
            bearer: vec![],
            basic: vec![
                pkv("username", serde_json::json!("admin")),
                pkv("password", serde_json::json!("{{pass}}")),
            ],
            apikey: vec![],
            oauth2: vec![],
        }
    }

    fn apikey() -> PostmanAuth {
        PostmanAuth {
            auth_type: "apikey".into(),
            bearer: vec![],
            basic: vec![],
            apikey: vec![
                pkv("key", serde_json::json!("X-API-Key")),
                pkv("value", serde_json::json!("{{apiKey}}")),
                pkv("in", serde_json::json!("header")),
            ],
            oauth2: vec![],
        }
    }

    fn noauth() -> PostmanAuth {
        PostmanAuth {
            auth_type: "noauth".into(),
            bearer: vec![],
            basic: vec![],
            apikey: vec![],
            oauth2: vec![],
        }
    }

    fn oauth2() -> PostmanAuth {
        PostmanAuth {
            auth_type: "oauth2".into(),
            bearer: vec![],
            basic: vec![],
            apikey: vec![],
            oauth2: vec![],
        }
    }

    #[test]
    fn converts_bearer_auth() {
        match convert_auth(&bearer()).unwrap() {
            Auth::Bearer { token } => assert_eq!(token, "{{myToken}}"),
            other => panic!("expected bearer, got {:?}", other),
        }
    }

    #[test]
    fn converts_basic_auth() {
        match convert_auth(&basic_auth()).unwrap() {
            Auth::Basic { username, password } => {
                assert_eq!(username, "admin");
                assert_eq!(password, "{{pass}}");
            }
            other => panic!("expected basic, got {:?}", other),
        }
    }

    #[test]
    fn converts_apikey_auth_header_placement() {
        match convert_auth(&apikey()).unwrap() {
            Auth::ApiKey { key, value, placement } => {
                assert_eq!(key, "X-API-Key");
                assert_eq!(value, "{{apiKey}}");
                assert_eq!(placement, "header");
            }
            other => panic!("expected apikey, got {:?}", other),
        }
    }

    #[test]
    fn noauth_becomes_auth_none() {
        assert!(matches!(convert_auth(&noauth()), Some(Auth::None)));
    }

    #[test]
    fn oauth2_returns_none() {
        assert!(convert_auth(&oauth2()).is_none());
    }

    #[test]
    fn converts_headers_with_disabled_flag() {
        let headers = vec![
            PostmanHeader {
                key: "Content-Type".into(),
                value: "application/json".into(),
                disabled: false,
            },
            PostmanHeader {
                key: "X-Old".into(),
                value: "foo".into(),
                disabled: true,
            },
        ];
        let result = convert_headers(&headers);
        assert_eq!(result.len(), 2);
        assert!(result[0].enabled);
        assert!(!result[1].enabled);
    }

    #[test]
    fn query_param_skips_missing_key() {
        let params = vec![
            PostmanQueryParam {
                key: Some("page".into()),
                value: Some("1".into()),
                disabled: false,
            },
            PostmanQueryParam {
                key: None,
                value: None,
                disabled: false,
            },
        ];
        let result = convert_query_params(&params);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key, "page");
        assert_eq!(result[0].value, "1");
        assert!(result[0].enabled);
    }

    #[test]
    fn converts_path_variables() {
        let vars = vec![PostmanPathVariable {
            key: "id".into(),
            value: Some("123".into()),
        }];
        let result = convert_path_variables(&vars);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "id");
        assert_eq!(result[0].value, "123");
    }

    #[test]
    fn converts_raw_json_body() {
        let body = PostmanBody {
            mode: "raw".into(),
            raw: Some(r#"{"name":"Alice"}"#.into()),
            options: Some(PostmanBodyOptions {
                raw: Some(PostmanRawBodyOptions {
                    language: Some("json".into()),
                }),
            }),
            urlencoded: vec![],
            formdata: vec![],
        };
        let domain = convert_body(&body).unwrap();
        assert_eq!(domain.mode, BodyMode::Json);
        assert_eq!(domain.content.as_deref(), Some(r#"{"name":"Alice"}"#));
    }

    #[test]
    fn converts_urlencoded_body() {
        let body = PostmanBody {
            mode: "urlencoded".into(),
            raw: None,
            options: None,
            urlencoded: vec![
                PostmanFormParam {
                    key: "grant_type".into(),
                    value: Some("password".into()),
                    param_type: String::new(),
                    disabled: false,
                },
                PostmanFormParam {
                    key: "x".into(),
                    value: Some("y".into()),
                    param_type: String::new(),
                    disabled: true,
                },
            ],
            formdata: vec![],
        };
        let domain = convert_body(&body).unwrap();
        assert_eq!(domain.mode, BodyMode::FormUrlEncoded);
        assert_eq!(domain.content.as_deref(), Some("grant_type=password"));
    }

    #[test]
    fn converts_formdata_body_skipping_file_entries() {
        let body = PostmanBody {
            mode: "formdata".into(),
            raw: None,
            options: None,
            urlencoded: vec![],
            formdata: vec![
                PostmanFormParam {
                    key: "title".into(),
                    value: Some("My File".into()),
                    param_type: "text".into(),
                    disabled: false,
                },
                PostmanFormParam {
                    key: "file".into(),
                    value: None,
                    param_type: "file".into(),
                    disabled: false,
                },
            ],
        };
        let domain = convert_body(&body).unwrap();
        assert_eq!(domain.mode, BodyMode::FormData);
        let entries = domain.form_data.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "title");

        let skips = body_skip_items(&body, "Upload");
        assert_eq!(skips.len(), 1);
        assert!(matches!(skips[0].reason, SkipReason::UnsupportedRequestType(_)));
    }

    #[test]
    fn file_body_returns_none_and_skip_item() {
        let body = PostmanBody {
            mode: "file".into(),
            raw: None,
            options: None,
            urlencoded: vec![],
            formdata: vec![],
        };
        assert!(convert_body(&body).is_none());
        let skips = body_skip_items(&body, "Upload");
        assert_eq!(skips.len(), 1);
        assert!(matches!(skips[0].reason, SkipReason::UnsupportedRequestType(_)));
    }

    #[test]
    fn converts_collection_variables() {
        let vars = vec![PostmanVariable {
            key: "baseUrl".into(),
            value: "http://localhost:3000".into(),
            disabled: false,
        }];
        let out = convert_collection_variables(&vars);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, "baseUrl");
        assert_eq!(out[0].value, "http://localhost:3000");
        assert_eq!(out[0].initial_value, "http://localhost:3000");
        assert!(out[0].enabled);
        assert!(!out[0].secret);
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p rocket-import converter::postman::tests
```

Expected: all 13 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-import/src/converter/
git commit -m "feat(import): postman converter — auth, headers, params, body, variables"
```

---

## Task 2: Converter — request item conversion

**Files:**
- Modify: `crates/rocket-import/src/converter/postman.rs`

- [ ] **Step 1: Append `convert_request_item` + tests to `converter/postman.rs`**

Add the function above the `#[cfg(test)]` block:

```rust
use rocket_collection::Request;
use rocket_shared::types::HttpMethod;
use std::str::FromStr;

/// Convert a `PostmanRequestItem` into a domain `Request` plus any skip items
/// (unsupported auth or body modes). Always returns a `Request` — the auth/body
/// fields are simply omitted when unsupported.
pub(crate) fn convert_request_item(
    item: &PostmanRequestItem,
) -> (Request, Vec<SkippedItem>) {
    let mut skipped = Vec::new();

    // Method — fall back to GET on parse failure (Postman is permissive about case).
    let method = HttpMethod::from_str(&item.request.method).unwrap_or(HttpMethod::Get);

    let mut req = Request::new(item.name.clone(), method, item.request.url.raw().to_string());

    req.headers = convert_headers(&item.request.header);
    req.query_params = convert_query_params(item.request.url.query_params());
    req.path_params = convert_path_variables(item.request.url.path_variables());

    // Auth: oauth2 records a skip; everything else converts (or becomes Auth::None).
    if let Some(a) = item.request.auth.as_ref() {
        if a.auth_type == "oauth2" {
            skipped.push(SkippedItem {
                path: item.name.clone(),
                reason: SkipReason::UnsupportedAuthType("oauth2".into()),
            });
        } else if let Some(domain_auth) = convert_auth(a) {
            req.auth = domain_auth;
        }
    }

    // Body: file body returns None and records a skip; multipart records skips
    // for any file entries it encounters.
    if let Some(b) = item.request.body.as_ref() {
        skipped.extend(body_skip_items(b, &item.name));
        req.body = convert_body(b);
    }

    if let Some(d) = item.request.description.as_ref() {
        req.description = Some(rocket_shared::description::Description::text(d.as_str()));
    }

    (req, skipped)
}
```

Append to the existing `#[cfg(test)]` block:

```rust
    #[test]
    fn converts_get_request() {
        let item = PostmanRequestItem {
            name: "Get Users".into(),
            request: PostmanRequest {
                method: "GET".into(),
                url: PostmanUrl::Object(PostmanUrlObject {
                    raw: "{{baseUrl}}/users".into(),
                    query: vec![PostmanQueryParam {
                        key: Some("page".into()),
                        value: Some("1".into()),
                        disabled: false,
                    }],
                    variable: vec![],
                }),
                header: vec![PostmanHeader {
                    key: "Accept".into(),
                    value: "application/json".into(),
                    disabled: false,
                }],
                auth: None,
                body: None,
                description: None,
            },
        };
        let (req, skipped) = convert_request_item(&item);
        assert_eq!(req.name, "Get Users");
        assert_eq!(req.method, HttpMethod::Get);
        assert_eq!(req.url, "{{baseUrl}}/users");
        assert_eq!(req.query_params.len(), 1);
        assert_eq!(req.query_params[0].key, "page");
        assert_eq!(req.headers.len(), 1);
        assert!(skipped.is_empty());
    }

    #[test]
    fn converts_post_with_json_body() {
        let item = PostmanRequestItem {
            name: "Create User".into(),
            request: PostmanRequest {
                method: "POST".into(),
                url: PostmanUrl::String("{{baseUrl}}/users".into()),
                header: vec![],
                auth: None,
                body: Some(PostmanBody {
                    mode: "raw".into(),
                    raw: Some(r#"{"name":"Alice"}"#.into()),
                    options: Some(PostmanBodyOptions {
                        raw: Some(PostmanRawBodyOptions {
                            language: Some("json".into()),
                        }),
                    }),
                    urlencoded: vec![],
                    formdata: vec![],
                }),
                description: None,
            },
        };
        let (req, skipped) = convert_request_item(&item);
        assert_eq!(req.method, HttpMethod::Post);
        let body = req.body.unwrap();
        assert_eq!(body.mode, BodyMode::Json);
        assert!(skipped.is_empty());
    }

    #[test]
    fn oauth2_auth_records_skip_and_leaves_auth_none() {
        let item = PostmanRequestItem {
            name: "OAuth Request".into(),
            request: PostmanRequest {
                method: "GET".into(),
                url: PostmanUrl::String("https://api.example.com".into()),
                header: vec![],
                auth: Some(PostmanAuth {
                    auth_type: "oauth2".into(),
                    bearer: vec![],
                    basic: vec![],
                    apikey: vec![],
                    oauth2: vec![],
                }),
                body: None,
                description: None,
            },
        };
        let (req, skipped) = convert_request_item(&item);
        assert!(matches!(req.auth, Auth::None));
        assert_eq!(skipped.len(), 1);
        assert!(matches!(skipped[0].reason, SkipReason::UnsupportedAuthType(_)));
    }

    #[test]
    fn file_body_records_skip_and_leaves_body_none() {
        let item = PostmanRequestItem {
            name: "Upload".into(),
            request: PostmanRequest {
                method: "POST".into(),
                url: PostmanUrl::String("https://api.example.com/upload".into()),
                header: vec![],
                auth: None,
                body: Some(PostmanBody {
                    mode: "file".into(),
                    raw: None,
                    options: None,
                    urlencoded: vec![],
                    formdata: vec![],
                }),
                description: None,
            },
        };
        let (req, skipped) = convert_request_item(&item);
        assert!(req.body.is_none());
        assert_eq!(skipped.len(), 1);
    }

    #[test]
    fn path_variables_become_path_params() {
        let item = PostmanRequestItem {
            name: "Get By ID".into(),
            request: PostmanRequest {
                method: "GET".into(),
                url: PostmanUrl::Object(PostmanUrlObject {
                    raw: "{{baseUrl}}/users/:id".into(),
                    query: vec![],
                    variable: vec![PostmanPathVariable {
                        key: "id".into(),
                        value: Some("123".into()),
                    }],
                }),
                header: vec![],
                auth: None,
                body: None,
                description: None,
            },
        };
        let (req, _) = convert_request_item(&item);
        assert_eq!(req.path_params.len(), 1);
        assert_eq!(req.path_params[0].name, "id");
    }
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p rocket-import converter::postman::tests
```

Expected: all 18 tests pass (13 from Task 1 + 5 new).

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-import/src/converter/postman.rs
git commit -m "feat(import): postman request item converter"
```

---

## Task 3: ImportService methods + integration test

**Files:**
- Create: `crates/rocket-import/tests/postman_integration_test.rs`
- Modify: `crates/rocket-import/src/importer.rs`

- [ ] **Step 1: Add the integration test file**

```rust
// crates/rocket-import/tests/postman_integration_test.rs
use rocket_environment::EnvironmentRepository;
use rocket_import::{EnvironmentRepositoryFactory, ImportService};
use rocket_infra::{FsCollectionRepo, FsEnvironmentRepo};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

struct FsEnvFactory(PathBuf);
impl EnvironmentRepositoryFactory for FsEnvFactory {
    fn make(&self, collection_name: &str) -> Box<dyn EnvironmentRepository> {
        Box::new(FsEnvironmentRepo::new(
            self.0
                .join("collections")
                .join(collection_name)
                .join("environments"),
        ))
    }
}

fn make_service(workspace_path: &Path) -> ImportService {
    let path = workspace_path.to_path_buf();
    ImportService::new(
        path.clone(),
        Box::new(FsCollectionRepo::new(path.join("collections"))),
        Box::new(FsEnvFactory(path)),
    )
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/postman")
        .join(name)
}

#[test]
fn imports_minimal_collection() {
    let ws = TempDir::new().unwrap();
    let report = make_service(ws.path())
        .import_postman_collection(&fixture("minimal-collection.json"), "default")
        .expect("should import");
    assert_eq!(report.imported, 2);
    assert!(report
        .created_collections
        .iter()
        .any(|n| n.starts_with("Minimal")));
}

#[test]
fn imports_full_collection_with_folders() {
    let ws = TempDir::new().unwrap();
    let report = make_service(ws.path())
        .import_postman_collection(&fixture("full-collection.json"), "default")
        .expect("should import");
    // Users folder (3) + Auth (1) + Upload (1, body partly skipped).
    assert!(report.imported >= 4, "got {}", report.imported);
    // formdata file entry should be skipped.
    assert!(report
        .skipped
        .iter()
        .any(|s| s.path.contains("Upload") || s.path.contains("file")));
}

#[test]
fn imports_v2_0_collection() {
    let ws = TempDir::new().unwrap();
    let report = make_service(ws.path())
        .import_postman_collection(&fixture("v2.0-collection.json"), "default")
        .expect("should import v2.0");
    assert_eq!(report.imported, 1);
}

#[test]
fn auto_renames_on_conflict() {
    let ws = TempDir::new().unwrap();
    let svc = make_service(ws.path());
    svc.import_postman_collection(&fixture("minimal-collection.json"), "default")
        .unwrap();
    let r2 = svc
        .import_postman_collection(&fixture("minimal-collection.json"), "default")
        .unwrap();
    assert!(
        r2.created_collections.iter().any(|n| n.contains("-1")),
        "expected -1 suffix, got {:?}",
        r2.created_collections
    );
}

#[test]
fn rejects_non_postman_json() {
    let ws = TempDir::new().unwrap();
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("other.json");
    std::fs::write(&path, r#"{"foo": "bar"}"#).unwrap();
    let result = make_service(ws.path()).import_postman_collection(&path, "default");
    assert!(result.is_err());
}

#[test]
fn imports_embedded_environments_from_collection() {
    let ws = TempDir::new().unwrap();
    let report = make_service(ws.path())
        .import_postman_collection(&fixture("full-collection.json"), "default")
        .expect("should import");

    let col_name = report.created_collections[0].clone();
    let env_dir = ws
        .path()
        .join("collections")
        .join(&col_name)
        .join("environments");

    assert!(
        env_dir.join("Local.yml").exists(),
        "Local.yml not found in {:?}",
        env_dir
    );
    assert!(
        env_dir.join("Staging.yml").exists(),
        "Staging.yml not found in {:?}",
        env_dir
    );
}

#[test]
fn imports_environment_into_existing_collection() {
    let ws = TempDir::new().unwrap();
    let svc = make_service(ws.path());
    svc.import_postman_collection(&fixture("minimal-collection.json"), "default")
        .unwrap();

    let col_name = std::fs::read_dir(ws.path().join("collections"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .file_name()
        .to_string_lossy()
        .to_string();

    let report = svc
        .import_postman_environment(&fixture("environment.json"), &col_name, "default")
        .expect("should import env");

    assert_eq!(report.imported, 3);
    assert!(ws
        .path()
        .join("collections")
        .join(&col_name)
        .join("environments/Local.yml")
        .exists());
}
```

- [ ] **Step 2: Run integration test to confirm it fails (compile error — methods don't exist yet)**

```bash
cargo test -p rocket-import --test postman_integration_test
```

Expected: compile error.

- [ ] **Step 3: Append the two methods + helper to `importer.rs`**

Add at the end of `impl ImportService` (above the closing `}`):

```rust
    /// Import a Postman Collection JSON (v2.0 or v2.1) into the workspace.
    pub fn import_postman_collection(
        &self,
        json_path: &Path,
        _workspace_id: &str,
    ) -> ImportResult<ImportReport> {
        use crate::converter::postman as pc;
        use crate::postman::parse_postman_json;

        let mut report = ImportReport::default();
        report.detected_type = "collection".to_string();

        let collection = parse_postman_json(json_path)?;
        let col_name = self.resolve_collection_name(&collection.info.name)?;

        self.collection_repo
            .create(&col_name)
            .map_err(ImportError::DomainError)?;
        report.created_collections.push(col_name.clone());

        // Collection-level variables + auth → settings.
        let variables = pc::convert_collection_variables(&collection.variable);
        let auth = collection.auth.as_ref().and_then(|a| {
            if a.auth_type == "oauth2" {
                report.skipped.push(SkippedItem {
                    path: col_name.clone(),
                    reason: SkipReason::UnsupportedAuthType("oauth2".into()),
                });
                None
            } else {
                pc::convert_auth(a)
            }
        });
        if !variables.is_empty() || auth.is_some() {
            use rocket_collection::settings::CollectionSettings;
            let settings = CollectionSettings {
                auth,
                variables,
                ..Default::default()
            };
            self.collection_repo
                .save_settings(&col_name, &settings)
                .map_err(ImportError::DomainError)?;
        }

        // Embedded environments (the primary way Postman exports them).
        for postman_env in &collection.environment {
            let mut env = rocket_environment::Environment::new(&postman_env.name);
            for v in &postman_env.values {
                let mut var = rocket_environment::Variable::new(&v.key, &v.value);
                var.enabled = v.enabled;
                env.set_variable(var);
            }
            self.env_factory
                .make(&col_name)
                .save(&env)
                .map_err(ImportError::DomainError)?;
        }

        // Walk and write all requests + folders via the repo.
        self.write_postman_items(&collection.item, &col_name, "", &mut report)?;

        Ok(report)
    }

    /// Recursively write Postman items via `self.collection_repo`.
    fn write_postman_items(
        &self,
        items: &[crate::postman::ast::PostmanItem],
        col_name: &str,
        path_prefix: &str,
        report: &mut ImportReport,
    ) -> ImportResult<()> {
        use crate::converter::postman as pc;
        use crate::postman::ast::PostmanItem;

        for item in items {
            match item {
                PostmanItem::Request(req_item) => {
                    report.total_files += 1;
                    let (req, mut skipped) = pc::convert_request_item(req_item);
                    report.skipped.append(&mut skipped);

                    let slug = sanitize_postman_filename(&req_item.name);
                    let request_path = if path_prefix.is_empty() {
                        slug
                    } else {
                        format!("{}/{}", path_prefix, slug)
                    };

                    self.collection_repo
                        .save_request(col_name, &request_path, &req)
                        .map_err(ImportError::DomainError)?;
                    report.imported += 1;
                }
                PostmanItem::Folder(folder) => {
                    let folder_slug = sanitize_postman_filename(&folder.name);
                    let folder_path = if path_prefix.is_empty() {
                        folder_slug
                    } else {
                        format!("{}/{}", path_prefix, folder_slug)
                    };

                    self.collection_repo
                        .create_folder(col_name, &folder_path)
                        .map_err(ImportError::DomainError)?;

                    self.write_postman_items(&folder.item, col_name, &folder_path, report)?;
                }
            }
        }
        Ok(())
    }

    /// Import a Postman environment JSON file into an existing collection.
    pub fn import_postman_environment(
        &self,
        json_path: &Path,
        collection_name: &str,
        _workspace_id: &str,
    ) -> ImportResult<ImportReport> {
        use crate::postman::parse_postman_environment;
        use rocket_environment::{Environment, Variable};

        let mut report = ImportReport::default();
        report.detected_type = "environment".to_string();

        let postman_env = parse_postman_environment(json_path)?;

        let mut env = Environment::new(&postman_env.name);
        for v in &postman_env.values {
            let mut var = Variable::new(&v.key, &v.value);
            var.enabled = v.enabled;
            env.set_variable(var);
        }

        self.env_factory
            .make(collection_name)
            .save(&env)
            .map_err(ImportError::DomainError)?;

        report.imported = postman_env.values.len();
        Ok(report)
    }
```

Add this free function near the bottom of `importer.rs` (outside the `impl` block):

```rust
/// Sanitize a Postman item name for use as a folder/file path component.
/// Keeps alphanumerics, `-`, and `_`; replaces everything else with `-`.
fn sanitize_postman_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}
```

- [ ] **Step 4: Run integration tests**

```bash
cargo test -p rocket-import --test postman_integration_test
```

Expected: all 7 tests pass.

- [ ] **Step 5: Run the full crate test suite — no regressions**

```bash
cargo test -p rocket-import
```

Expected: all Bruno + Postman tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-import/src/importer.rs \
        crates/rocket-import/tests/postman_integration_test.rs
git commit -m "feat(import): import_postman_collection + import_postman_environment on ImportService"
```
