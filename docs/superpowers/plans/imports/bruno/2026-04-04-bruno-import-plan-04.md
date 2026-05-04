# Bruno Import — Plan 04: Converters + ImportService

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the three converter modules (`BruDocument` → domain types) and the full `ImportService` orchestration that walks a Bruno directory tree and writes an RocketAPI collection via existing repos.

**Architecture:** Converters produce domain types from `rocket-shared` / `rocket-collection` / `rocket-environment`. `ImportService` delegates ALL disk writes to `FsCollectionRepo` and `FsEnvironmentRepo` — zero raw YAML writes. Name conflicts resolved by appending `-1`, `-2`, etc.

**Tech Stack:** Rust, rocket-collection, rocket-environment, rocket-workspace domain types

**Prerequisite:** Plans 01, 02, and 03 complete.

**Spec:** `docs/superpowers/specs/2026-04-04-bruno-import-design.md`

---

## Task 1: Request converter

**Files:**
- Modify: `crates/rocket-import/src/converter/request.rs`

Read `crates/rocket-infra/src/opencollection.rs` and `crates/rocket-collection/src/` to understand the domain `Request` type and its fields before writing this task.

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::bru::ast::*;

    fn doc_with_method(method: BruMethod, url: &str) -> BruDocument {
        BruDocument {
            meta: Some(BruMeta {
                name: "Test".into(),
                request_type: "http".into(),
                seq: Some(1),
            }),
            method: Some(method),
            url: Some(url.into()),
            ..BruDocument::default()
        }
    }

    #[test]
    fn converts_get_request_name_method_url() {
        let doc = doc_with_method(BruMethod::Get, "https://api.example.com/users");
        let (req, skipped) = convert(&doc);
        assert!(skipped.is_empty());
        let req = req.unwrap();
        assert_eq!(req.name, "Test");
        // Verify HTTP method and URL are set — exact field names depend on
        // rocket-collection's Request type. Adjust assertions to match.
        assert!(format!("{:?}", req).contains("Get") || format!("{:?}", req).contains("GET"));
        assert!(format!("{:?}", req).contains("api.example.com"));
    }

    #[test]
    fn converts_bearer_auth() {
        let mut doc = doc_with_method(BruMethod::Get, "https://example.com");
        doc.auth = Some(BruAuth::Bearer { token: "{{token}}".into() });
        let (req, skipped) = convert(&doc);
        assert!(skipped.is_empty());
        let req = req.unwrap();
        assert!(format!("{:?}", req).contains("Bearer") || format!("{:?}", req).contains("bearer"));
    }

    #[test]
    fn unsupported_request_type_produces_skip_reason() {
        let doc = BruDocument {
            meta: Some(BruMeta {
                name: "GQL".into(),
                request_type: "graphql".into(),
                seq: None,
            }),
            unknown_blocks: vec![BruRawBlock {
                name: "unsupported_type".into(),
                subtype: Some("graphql".into()),
                content: String::new(),
            }],
            ..BruDocument::default()
        };
        let (req, skipped) = convert(&doc);
        assert!(req.is_none());
        assert_eq!(skipped.len(), 1);
        assert!(matches!(skipped[0], SkipReason::UnsupportedRequestType(_)));
    }

    #[test]
    fn oauth2_auth_produces_skip_reason() {
        let mut doc = doc_with_method(BruMethod::Get, "https://example.com");
        doc.unknown_blocks.push(BruRawBlock {
            name: "auth".into(),
            subtype: Some("oauth2".into()),
            content: String::new(),
        });
        let (req, skipped) = convert(&doc);
        // Request itself is still produced, just auth is skipped.
        assert!(req.is_some());
        assert_eq!(skipped.len(), 1);
        assert!(matches!(skipped[0], SkipReason::UnsupportedAuthType(_)));
    }

    #[test]
    fn converts_json_body() {
        let mut doc = doc_with_method(BruMethod::Post, "https://example.com");
        doc.body = Some(BruBody::Json("{\"key\":\"val\"}".into()));
        let (req, skipped) = convert(&doc);
        assert!(skipped.is_empty());
        assert!(req.is_some());
    }

    #[test]
    fn disabled_headers_are_preserved() {
        let mut doc = doc_with_method(BruMethod::Get, "https://example.com");
        doc.headers = vec![
            BruKeyValue { key: "Accept".into(), value: "application/json".into(), disabled: false },
            BruKeyValue { key: "~X-Debug".into(), value: "true".into(), disabled: true },
        ];
        let (req, _) = convert(&doc);
        assert!(req.is_some());
    }
}
```

Run: `cargo test -p rocket-import converter::request`
Expected: FAIL (convert not implemented)

- [ ] **Step 2: Implement `converter/request.rs`**

Inspect `crates/rocket-collection/src/` for the domain `Request` struct and its builder/constructor. Map each `BruDocument` field to the matching domain field. The exact field names depend on what `rocket-collection` exposes — read its source before writing this converter.

Key mapping rules:
- `meta.name` → `Request::name`
- `meta.request_type == "graphql"|"grpc"|"websocket"` → return `(None, vec![SkipReason::UnsupportedRequestType(type)])`
- `method` + `url` → HTTP method and URL fields
- `headers` → headers list; `disabled: true` headers are included but marked disabled
- `body` → body mode + content fields
- `auth: BruAuth::Bearer` → auth bearer fields; `unknown_blocks` with `name == "auth"` and `subtype == "oauth2"` → `SkipReason::UnsupportedAuthType("oauth2")`
- `pre_request_script` / `post_response_script` → script fields

```rust
use crate::bru::ast::*;
use crate::report::SkipReason;

/// Convert a BruDocument to a domain Request.
/// Returns (Option<Request>, Vec<SkipReason>) — Request is None if the type is unsupported.
/// Non-fatal skips (e.g. unsupported auth) still return Some(Request) with auth omitted.
pub fn convert(doc: &BruDocument) -> (Option<rocket_collection::Request>, Vec<SkipReason>) {
    let mut skipped: Vec<SkipReason> = Vec::new();

    // Check for unsupported request types first.
    for block in &doc.unknown_blocks {
        if block.name == "unsupported_type" {
            let t = block.subtype.clone().unwrap_or_default();
            skipped.push(SkipReason::UnsupportedRequestType(t));
        }
        if block.name == "auth" {
            let t = block.subtype.clone().unwrap_or_default();
            skipped.push(SkipReason::UnsupportedAuthType(t));
        }
    }

    // If unsupported type, bail out entirely.
    if skipped.iter().any(|s| matches!(s, SkipReason::UnsupportedRequestType(_))) {
        return (None, skipped);
    }

    let name = doc.meta.as_ref()
        .map(|m| m.name.clone())
        .unwrap_or_else(|| "Untitled".into());

    // TODO: construct the domain Request using rocket-collection's builder/constructor.
    // Replace this placeholder with the actual construction once you've read
    // crates/rocket-collection/src/ for the Request type API.
    //
    // Fields to populate:
    //   name, method, url, headers (with enabled flag), body (mode + content),
    //   auth (from doc.auth), pre_request_script, post_response_script
    //
    // Example skeleton (adjust to actual API):
    //
    // let mut request = rocket_collection::Request::new(name);
    // if let Some(method) = &doc.method { request.set_method(...); }
    // if let Some(url) = &doc.url { request.set_url(url); }
    // for h in &doc.headers { request.add_header(h.key.clone(), h.value.clone(), !h.disabled); }
    // ... etc.

    let _ = name; // remove when used
    todo!("implement after reading rocket-collection Request type")
}
```

**Important:** The `todo!()` must be replaced with the real implementation after reading `crates/rocket-collection/src/`. The tests above will guide correctness.

- [ ] **Step 3: Run tests**

```bash
cargo test -p rocket-import converter::request
```
Expected: ALL PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-import/src/converter/request.rs
git commit -m "feat(import): request converter — BruDocument → rocket_collection::Request"
```

---

## Task 2: Environment + collection converters

**Files:**
- Modify: `crates/rocket-import/src/converter/environment.rs`
- Modify: `crates/rocket-import/src/converter/collection.rs`

Read `crates/rocket-environment/src/` for the domain `Environment` and `Variable` types before writing this task.

- [ ] **Step 1: Write failing tests for environment converter**

```rust
// crates/rocket-import/src/converter/environment.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::bru::ast::*;

    #[test]
    fn converts_plain_vars() {
        let doc = BruDocument {
            vars: vec![
                BruKeyValue { key: "baseUrl".into(), value: "http://localhost:3000".into(), disabled: false },
                BruKeyValue { key: "apiKey".into(), value: "abc123".into(), disabled: true },
            ],
            ..BruDocument::default()
        };
        let env = convert("local", &doc);
        assert_eq!(env.name, "local");
        // baseUrl: enabled, apiKey: disabled
        assert_eq!(env.variables.len(), 2);
    }

    #[test]
    fn secret_var_names_become_secret_variables() {
        let doc = BruDocument {
            secret_vars: vec!["DB_PASSWORD".into(), "API_SECRET".into()],
            ..BruDocument::default()
        };
        let env = convert("prod", &doc);
        // Secret vars have empty values and secret: true
        assert_eq!(env.variables.iter().filter(|v| v.secret).count(), 2);
    }
}
```

- [ ] **Step 2: Implement `converter/environment.rs`**

Map `BruDocument::vars` → `Environment::variables` (plain), `BruDocument::secret_vars` → `Environment::variables` (secret: true, value: empty).

```rust
use crate::bru::ast::BruDocument;

/// Convert a parsed BruDocument (env file) to a domain Environment.
pub fn convert(name: &str, doc: &BruDocument) -> rocket_environment::Environment {
    // TODO: construct using rocket-environment's Environment type.
    // Replace with actual API after reading crates/rocket-environment/src/.
    //
    // Mapping:
    //   doc.vars (disabled=false) → enabled variables with value
    //   doc.vars (disabled=true)  → disabled variables with value
    //   doc.secret_vars           → secret variables (empty value, secret: true)
    todo!("implement after reading rocket-environment Environment type")
}
```

- [ ] **Step 3: Implement `converter/collection.rs`**

Collection-level vars come from Bruno's `bruno.json` (already parsed into `BruDocument::vars` by the caller in `ImportService`). This converter produces the variable list for `opencollection.yml`.

```rust
use crate::bru::ast::BruKeyValue;

/// Convert Bruno collection-level variables into domain Variable list.
pub fn convert_variables(vars: &[BruKeyValue]) -> Vec<rocket_collection::Variable> {
    // TODO: map each BruKeyValue → rocket_collection::Variable
    // disabled → !enabled
    todo!("implement after reading rocket-collection Variable type")
}
```

- [ ] **Step 4: Run all converter tests**

```bash
cargo test -p rocket-import converter::
```
Expected: ALL PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-import/src/converter/
git commit -m "feat(import): environment and collection converters"
```

---

## Task 3: Full ImportService implementation

**Files:**
- Modify: `crates/rocket-import/src/importer.rs`
- Create: `crates/rocket-import/tests/fixtures/` (fixture files)
- Create: `crates/rocket-import/tests/integration_test.rs`

- [ ] **Step 1: Create fixture Bruno collection**

Create the following fixture directory structure under `crates/rocket-import/tests/fixtures/my-api/`:

```
my-api/
  bruno.json          ← { "name": "My API", "version": "1", "type": "collection" }
  get-users.bru       ← GET request (see below)
  create-user.yml     ← POST request in Bruno .yml format (see below)
  environments/
    local.yml         ← Bruno env .yml with baseUrl variable
  auth/
    bruno.json        ← { "name": "auth", "type": "folder" }
    login.bru         ← POST request
```

`get-users.bru`:
```
meta {
  name: Get Users
  type: http
  seq: 1
}

get {
  url: {{baseUrl}}/users
}

headers {
  Content-Type: application/json
}
```

`create-user.yml` (Bruno format):
```yaml
meta:
  name: Create User
  type: http
  seq: 2
http:
  method: POST
  url: "{{baseUrl}}/users"
  body:
    mode: json
    json: '{"name": "Alice"}'
```

`environments/local.yml`:
```yaml
name: local
variables:
  - name: baseUrl
    value: http://localhost:3000
    enabled: true
```

`auth/login.bru`:
```
meta {
  name: Login
  type: http
  seq: 1
}

post {
  url: {{baseUrl}}/auth/login
}

body:json {
  {"username": "admin", "password": "{{adminPass}}"}
}

auth:basic {
  username: admin
  password: {{adminPass}}
}
```

- [ ] **Step 2: Write integration test**

```rust
// crates/rocket-import/tests/integration_test.rs
use rocket_import::{ImportService, ImportReport};
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn imports_fixture_collection_successfully() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/my-api");

    // Create a temp workspace directory to import into.
    let workspace_dir = TempDir::new().unwrap();

    let service = ImportService::new_with_workspace_path(workspace_dir.path());
    let report = service
        .import_collection(&fixture, "default")
        .expect("import should succeed");

    assert!(report.imported >= 3, "expected at least 3 requests imported");
    assert!(report.created_collections.contains(&"my-api".to_string()));

    // Verify the collection directory was created.
    assert!(workspace_dir.path().join("collections/my-api/opencollection.yml").exists());
    assert!(workspace_dir.path().join("collections/my-api/get-users.yml").exists());
    assert!(workspace_dir.path().join("collections/my-api/create-user.yml").exists());
    assert!(workspace_dir.path().join("collections/my-api/auth/login.yml").exists());
    assert!(workspace_dir.path().join("collections/my-api/environments/local.yml").exists());
}

#[test]
fn import_report_counts_correctly() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/my-api");
    let workspace_dir = TempDir::new().unwrap();

    let service = ImportService::new_with_workspace_path(workspace_dir.path());
    let report = service.import_collection(&fixture, "default").unwrap();

    assert_eq!(report.total_files, 3); // get-users.bru, create-user.yml, auth/login.bru
    assert_eq!(report.imported, 3);
    assert!(report.skipped.is_empty());
}

#[test]
fn auto_renames_on_collection_name_conflict() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/my-api");
    let workspace_dir = TempDir::new().unwrap();

    let service = ImportService::new_with_workspace_path(workspace_dir.path());
    // Import twice — second should be renamed.
    service.import_collection(&fixture, "default").unwrap();
    let report2 = service.import_collection(&fixture, "default").unwrap();

    assert!(report2.created_collections.iter().any(|n| n == "my-api-1"));
    assert!(workspace_dir.path().join("collections/my-api-1").exists());
}
```

- [ ] **Step 3: Implement `ImportService`**

```rust
use std::path::{Path, PathBuf};
use crate::bru;
use crate::converter::{collection as col_converter, environment as env_converter, request as req_converter};
use crate::error::{ImportError, ImportResult};
use crate::report::{ImportReport, SkipReason, SkippedItem};

pub struct ImportService {
    workspace_path: PathBuf,
}

impl ImportService {
    pub fn new() -> Self {
        // Default: use the active workspace path from the app config.
        // For tests, use new_with_workspace_path.
        Self {
            workspace_path: default_workspace_path(),
        }
    }

    pub fn new_with_workspace_path(path: &Path) -> Self {
        Self { workspace_path: path.to_path_buf() }
    }

    pub fn import_collection(
        &self,
        path: &Path,
        _workspace_id: &str,
    ) -> ImportResult<ImportReport> {
        // Validate Bruno collection.
        if !path.join("bruno.json").exists() {
            return Err(ImportError::NotABrunoDirectory(path.to_path_buf()));
        }

        let mut report = ImportReport::default();

        let col_name = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "imported".into());

        let resolved_name = self.resolve_collection_name(&col_name);
        let col_path = self.workspace_path.join("collections").join(&resolved_name);

        // Create collection directory + opencollection.yml via FsCollectionRepo.
        let repo = self.make_collection_repo();
        repo.create(&resolved_name)
            .map_err(ImportError::DomainError)?;

        report.created_collections.push(resolved_name.clone());

        // Walk request files.
        self.walk_requests(path, path, &resolved_name, &repo, &mut report)?;

        // Walk environments.
        let env_dir = path.join("environments");
        if env_dir.is_dir() {
            self.import_environments(&env_dir, &resolved_name, &mut report)?;
        }

        Ok(report)
    }

    fn walk_requests(
        &self,
        root: &Path,
        dir: &Path,
        collection_name: &str,
        repo: &impl rocket_collection::CollectionRepo,
        report: &mut ImportReport,
    ) -> ImportResult<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let p = entry.path();

            if p.is_dir() {
                // Skip environments directory.
                if p.file_name().map_or(false, |n| n == "environments") {
                    continue;
                }
                // Subfolder — create folder, recurse.
                let folder_rel = p.strip_prefix(root).unwrap_or(&p);
                let folder_path = folder_rel.to_string_lossy().to_string();
                let _ = repo.create_folder(collection_name, &folder_path);
                self.walk_requests(root, &p, collection_name, repo, report)?;
                continue;
            }

            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !matches!(ext, "bru" | "yml" | "yaml") {
                continue;
            }
            // Skip bruno.json-equivalent and _order files.
            if p.file_name().map_or(false, |n| n == "bruno.json" || n == "_order.yml") {
                continue;
            }

            report.total_files += 1;

            let rel_path = p.strip_prefix(root).unwrap_or(&p);
            let rel_str = rel_path.to_string_lossy().to_string();

            match bru::parse_file(&p) {
                Err(e) => {
                    report.skipped.push(SkippedItem {
                        path: rel_str,
                        reason: SkipReason::ParseError(e.to_string()),
                    });
                }
                Ok(doc) => {
                    let (req_opt, skipped_reasons) = req_converter::convert(&doc);

                    for reason in skipped_reasons {
                        report.skipped.push(SkippedItem {
                            path: rel_str.clone(),
                            reason,
                        });
                    }

                    if let Some(req) = req_opt {
                        // Derive output path: same relative path but .yml extension.
                        let out_path = rel_path.with_extension("yml").to_string_lossy().to_string();
                        let _ = repo.create_request(collection_name, &out_path, req);
                        report.imported += 1;
                    }
                }
            }
        }
        Ok(())
    }

    fn import_environments(
        &self,
        env_dir: &Path,
        collection_name: &str,
        report: &mut ImportReport,
    ) -> ImportResult<()> {
        let env_repo = self.make_env_repo(collection_name);
        for entry in std::fs::read_dir(env_dir)? {
            let entry = entry?;
            let p = entry.path();
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !matches!(ext, "bru" | "yml" | "yaml") { continue; }

            let env_name = p.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "env".into());

            match bru::parse_env_file(&p) {
                Err(e) => {
                    report.skipped.push(SkippedItem {
                        path: p.to_string_lossy().to_string(),
                        reason: SkipReason::ParseError(e.to_string()),
                    });
                }
                Ok(doc) => {
                    let env = env_converter::convert(&env_name, &doc);
                    let _ = env_repo.create(env);
                }
            }
        }
        Ok(())
    }

    pub fn import_workspace(
        &self,
        path: &Path,
        create_new_workspace: bool,
        target_workspace_id: Option<&str>,
    ) -> ImportResult<ImportReport> {
        if !path.join("bruno.json").exists() {
            return Err(ImportError::NotABrunoDirectory(path.to_path_buf()));
        }

        let mut combined = ImportReport::default();

        if create_new_workspace {
            let ws_name = path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "imported-workspace".into());
            // TODO: call WorkspaceService::create(&ws_name) and set self.workspace_path accordingly.
            combined.created_workspace = Some(ws_name);
        }

        // Each subdirectory with a bruno.json is a Bruno collection.
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_dir() && p.join("bruno.json").exists() {
                let id = target_workspace_id.unwrap_or("default");
                match self.import_collection(&p, id) {
                    Ok(r) => {
                        combined.total_files += r.total_files;
                        combined.imported += r.imported;
                        combined.skipped.extend(r.skipped);
                        combined.created_collections.extend(r.created_collections);
                    }
                    Err(e) => {
                        combined.skipped.push(SkippedItem {
                            path: p.to_string_lossy().to_string(),
                            reason: SkipReason::ParseError(e.to_string()),
                        });
                    }
                }
            }
        }

        Ok(combined)
    }

    fn resolve_collection_name(&self, name: &str) -> String {
        let col_dir = self.workspace_path.join("collections");
        if !col_dir.join(name).exists() {
            return name.to_string();
        }
        let mut i = 1;
        loop {
            let candidate = format!("{name}-{i}");
            if !col_dir.join(&candidate).exists() {
                return candidate;
            }
            i += 1;
        }
    }

    fn make_collection_repo(&self) -> rocket_infra::FsCollectionRepo {
        rocket_infra::FsCollectionRepo::new(self.workspace_path.join("collections"))
    }

    fn make_env_repo(&self, collection_name: &str) -> rocket_infra::FsEnvironmentRepo {
        rocket_infra::FsEnvironmentRepo::new(
            self.workspace_path.join("collections").join(collection_name).join("environments")
        )
    }
}

fn default_workspace_path() -> PathBuf {
    // Reads the active workspace path from app state.
    // Replace with real workspace path resolution once WorkspaceService is available.
    PathBuf::from(std::env::var("ROCKET_WORKSPACE_PATH").unwrap_or_else(|_| ".".into()))
}
```

**Note:** Add `rocket-infra` to `crates/rocket-import/Cargo.toml` dependencies:
```toml
rocket-infra = { path = "../rocket-infra" }
```

- [ ] **Step 4: Run integration tests**

```bash
cargo test -p rocket-import
```
Expected: ALL PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-import/src/importer.rs crates/rocket-import/tests/ crates/rocket-import/Cargo.toml
git commit -m "feat(import): full ImportService — walks Bruno directory tree, writes via existing repos"
```
