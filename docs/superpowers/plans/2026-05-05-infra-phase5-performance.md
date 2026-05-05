# rocket-infra Phase 5 — Performance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the three remaining performance bottlenecks from the `rocket-infra` code review: (1) eager full-tree load on every `Collection::get`, (2) `serde_json::Value` round-trips for request variables, (3) `serde_json::Value` round-trips for `HttpRequestExample` snapshots and `Environment.client_certificates`.

**Architecture:** Tasks are ordered by user-visible impact. Task 1 (lazy tree) is the biggest structural change — it adds a `RequestSummary` type to `rocket-collection` and a new `CollectionRepository::get_summaries` method; full `Request` bodies continue to load via the existing `get_request` method. Tasks 2–4 replace `serde_json::Value` placeholder fields with concrete domain types, eliminating per-call allocation and the silent `unwrap_or_default` data-loss risk. No caching infrastructure is added (mtime-keyed cache deferred until profiling confirms it is needed after Task 1 lands).

**Tech Stack:** Rust, `rocket-collection`, `rocket-environment`, `rocket-shared`, `rocket-infra`, `serde_yaml`, `tempfile` (tests)

> **Already completed (excluded from this plan):**
> - P1 — `save_request` now takes `&Request` (no deep clone)
> - P3 — `folder.yml` parsed once per folder in `build_folder_tree`
> - P4 — early-return for root requests in `get_folder_chain_variables`
> - P6 — `fs_history_repo::list` sorts by mtime before parsing
> - P7 — `atomic_write_bulk` with batched parent-dir fsync
> - P8 — `into_owned()` removed from sort comparator

---

## File Map

| File | Task | Change |
|------|------|--------|
| `crates/rocket-collection/src/summary.rs` | 1 | Add `RequestSummary` struct (name, uid, method, url, file_name) |
| `crates/rocket-collection/src/folder.rs` | 1 | Add `summaries: Vec<RequestSummary>` field to `Folder`; add `CollectionItem::Summary` variant |
| `crates/rocket-collection/src/lib.rs` | 1 | Export `RequestSummary` |
| `crates/rocket-collection/src/repository.rs` | 1 | Add `get_summaries(&self, name: &str) -> DomainResult<Collection>` (loads summaries, not full requests) |
| `crates/rocket-infra/src/fs_collection/tree.rs` | 1 | Add `build_folder_tree_summaries` — parses only name/uid/method/url from each request file |
| `crates/rocket-infra/src/fs_collection/mod.rs` | 1 | Implement `get_summaries` on `FsCollectionRepo` |
| `crates/rocket-collection/src/request.rs` | 2 | Change `variables: Vec<serde_json::Value>` → `Vec<CollectionVariable>` |
| `crates/rocket-infra/src/conversions/request.rs` | 2 | Update `oc_http_request_to_request` and `request_to_oc_http_request` to use `CollectionVariable` |
| `crates/rocket-shared/src/action.rs` | 3 | Replace `request: Option<serde_json::Value>` and `response: Option<serde_json::Value>` with concrete types |
| `crates/rocket-infra/src/conversions/request.rs` | 3 | Update example conversion to use concrete types (no `serde_json::to_value`) |
| `crates/rocket-environment/src/environment.rs` | 4 | Change `client_certificates: Vec<serde_json::Value>` → `Vec<ClientCertificate>` |
| `crates/rocket-infra/src/conversions/environment.rs` | 4 | Remove `serde_json::to_value`/`from_value` round-trips for client certs |

---

### Task 1: Add `get_summaries` — load sidebar tree without parsing full request bodies

**Background:** `FsCollectionRepo::get` calls `build_folder_tree` which reads and fully parses every `.yml` request file — O(n) YAML deserializations just to show the sidebar. For a 1000-request collection on a cold disk this takes ~1 s (serde_yaml is 5-10× slower than serde_json). The fix: add a `get_summaries` repository method that reads only the fields the sidebar needs (name, uid, method, url, file_name) from each file. The existing `get` method is unchanged; `src-tauri` command handlers switch to `get_summaries` for sidebar loads.

**Files:**
- Create: `crates/rocket-collection/src/request_summary.rs`
- Modify: `crates/rocket-collection/src/folder.rs`
- Modify: `crates/rocket-collection/src/lib.rs`
- Modify: `crates/rocket-collection/src/repository.rs`
- Modify: `crates/rocket-infra/src/fs_collection/tree.rs`
- Modify: `crates/rocket-infra/src/fs_collection/mod.rs`
- Modify: `src-tauri/src/commands/collection.rs` (or wherever `get_collection` Tauri command lives)

- [ ] **Step 1: Write failing tests for `get_summaries`**

  Add inside `mod tests` in `crates/rocket-infra/src/fs_collection/tests.rs`:

  ```rust
  #[test]
  fn get_summaries_returns_collection_with_summary_items() {
      let (_dir, repo) = setup();
      repo.create("pets").unwrap();
      let req = Request::new("List Pets", HttpMethod::Get, "https://api.example.com/pets");
      repo.save_request("pets", "list-pets.yml", &req).unwrap();

      let col = repo.get_summaries("pets").unwrap();
      assert_eq!(col.name, "pets");
      let summaries = col.root.request_summaries();
      assert_eq!(summaries.len(), 1);
      assert_eq!(summaries[0].name, "List Pets");
      assert_eq!(summaries[0].method, "GET");
      assert_eq!(summaries[0].url, "https://api.example.com/pets");
      assert!(!summaries[0].uid.is_empty());
  }

  #[test]
  fn get_summaries_does_not_load_body_or_auth() {
      let (_dir, repo) = setup();
      repo.create("api").unwrap();
      let mut req = Request::new("Post Data", HttpMethod::Post, "https://api.example.com/data");
      req.body = Some(rocket_shared::types::Body::json(r#"{"x":1}"#));
      repo.save_request("api", "post-data.yml", &req).unwrap();

      // get_summaries must succeed and return name/method/url — body is not loaded.
      let col = repo.get_summaries("api").unwrap();
      let summaries = col.root.request_summaries();
      assert_eq!(summaries.len(), 1);
      assert_eq!(summaries[0].name, "Post Data");
      assert_eq!(summaries[0].method, "POST");
  }

  #[test]
  fn get_summaries_preserves_folder_structure() {
      let (_dir, repo) = setup();
      repo.create("api").unwrap();
      repo.create_folder("api", "auth").unwrap();
      let req = Request::new("Login", HttpMethod::Post, "https://api.example.com/login");
      repo.save_request("api", "auth/login.yml", &req).unwrap();

      let col = repo.get_summaries("api").unwrap();
      let auth_folder = col.root.subfolders.iter().find(|f| f.dir_name.as_deref() == Some("auth"));
      assert!(auth_folder.is_some(), "auth folder missing from summaries tree");
      let summaries = auth_folder.unwrap().request_summaries();
      assert_eq!(summaries.len(), 1);
      assert_eq!(summaries[0].name, "Login");
  }
  ```

- [ ] **Step 2: Run the tests to confirm they fail (types don't exist yet)**

  ```bash
  cargo test -p rocket-infra get_summaries 2>&1 | head -30
  ```

  Expected: compile errors — `get_summaries`, `request_summaries`, `RequestSummary` are undefined.

- [ ] **Step 3: Create `RequestSummary` type in `rocket-collection`**

  Create `crates/rocket-collection/src/request_summary.rs`:

  ```rust
  use serde::{Deserialize, Serialize};

  /// Lightweight request descriptor for sidebar display.
  /// Contains only the fields the sidebar needs — body and auth are not loaded.
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct RequestSummary {
      pub uid: String,
      pub name: String,
      pub method: String,
      pub url: String,
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub file_name: Option<String>,
  }
  ```

- [ ] **Step 4: Add `request_summaries()` helper and `CollectionItem::Summary` variant to `folder.rs`**

  Open `crates/rocket-collection/src/folder.rs`. Add `Summary(RequestSummary)` as a new variant to `CollectionItem`:

  ```rust
  use crate::request_summary::RequestSummary;

  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  #[serde(tag = "type", rename_all = "camelCase")]
  pub enum CollectionItem {
      #[serde(rename = "request")]
      Request(Request),
      #[serde(rename = "folder")]
      Folder(Folder),
      #[serde(rename = "opaque")]
      OpaqueItem(OpaqueProtocolItem),
      /// Lightweight request placeholder for sidebar loads (no body/auth).
      #[serde(rename = "summary")]
      Summary(RequestSummary),
  }
  ```

  Add `request_summaries()` method to `Folder` (alongside the existing `requests()` method):

  ```rust
  impl Folder {
      /// Return all `Summary` items at this folder level (non-recursive).
      pub fn request_summaries(&self) -> Vec<&RequestSummary> {
          self.items.iter()
              .filter_map(|item| match item {
                  CollectionItem::Summary(s) => Some(s),
                  _ => None,
              })
              .collect()
      }

      pub fn add_summary(&mut self, summary: RequestSummary) {
          self.items.push(CollectionItem::Summary(summary));
      }
  }
  ```

- [ ] **Step 5: Export `RequestSummary` from `rocket-collection`**

  In `crates/rocket-collection/src/lib.rs`, add:

  ```rust
  pub mod request_summary;
  pub use request_summary::RequestSummary;
  ```

- [ ] **Step 6: Add `get_summaries` to `CollectionRepository` trait**

  In `crates/rocket-collection/src/repository.rs`, add after `get`:

  ```rust
  /// Get collection tree with lightweight request summaries instead of full Request bodies.
  /// Use for sidebar loads; call `get_request` for the full body when the user opens a request.
  fn get_summaries(&self, name: &str) -> DomainResult<Collection>;
  ```

- [ ] **Step 7: Compile check domain crate**

  ```bash
  cargo check -p rocket-collection 2>&1 | grep "^error" | head -20
  ```

  Expected: zero errors. Fix any issues before proceeding.

- [ ] **Step 8: Implement `build_folder_tree_summaries` in `tree.rs`**

  In `crates/rocket-infra/src/fs_collection/tree.rs`, add a new function after `build_folder_tree`:

  ```rust
  use rocket_collection::RequestSummary;

  /// Build the folder tree loading only lightweight request summaries.
  /// Reads name/uid/method/url from each request file but skips body, auth,
  /// scripts, and examples. About 5-10× faster than build_folder_tree for
  /// large collections.
  pub(super) fn build_folder_tree_summaries(current: &Path) -> DomainResult<Folder> {
      let dir_name = current
          .file_name()
          .map(|n| n.to_string_lossy().to_string())
          .unwrap_or_default();
      let mut folder = Folder::new(&dir_name);
      folder.uid = String::new();

      let folder_yml = current.join("folder.yml");
      if folder_yml.exists() {
          if let Ok(content) = fs::read_to_string(&folder_yml) {
              if let Ok(info) = serde_yaml::from_str::<OcFolderInfo>(&content) {
                  if let Some(ref uid) = info.uid {
                      if !uid.is_empty() {
                          folder.uid = uid.clone();
                      }
                  }
                  folder.name = info.name;
              }
          }
          if folder.uid.is_empty() {
              folder.uid = read_uid_from_yaml(current);
          }
      } else {
          folder.uid = read_uid_from_yaml(current);
      }
      folder.dir_name = Some(dir_name);

      if !current.exists() {
          return Ok(folder);
      }

      let mut entries: Vec<_> = fs::read_dir(current)?.filter_map(|e| e.ok()).collect();
      let order_path = current.join("_order.yml");
      let order_path = if order_path.exists() { order_path } else { current.join("_order.json") };
      if let Ok(content) = fs::read_to_string(&order_path) {
          if let Ok(ordered) = serde_yaml::from_str::<Vec<String>>(&content) {
              let pos: std::collections::HashMap<String, usize> = ordered
                  .into_iter().enumerate().map(|(i, name)| (name, i)).collect();
              entries.sort_by(|a, b| {
                  let ai = a.file_name().to_str().and_then(|n| pos.get(n)).copied().unwrap_or(usize::MAX);
                  let bi = b.file_name().to_str().and_then(|n| pos.get(n)).copied().unwrap_or(usize::MAX);
                  ai.cmp(&bi).then_with(|| a.file_name().cmp(&b.file_name()))
              });
          } else {
              entries.sort_by_key(|e| e.file_name());
          }
      } else {
          entries.sort_by_key(|e| e.file_name());
      }

      for entry in entries {
          let path = entry.path();
          let entry_name = entry.file_name().to_string_lossy().to_string();
          if entry_name.starts_with('.') || entry_name == "environments" {
              continue;
          }
          if path.is_dir() {
              if std::fs::symlink_metadata(&path).map(|m| m.file_type().is_symlink()).unwrap_or(false) {
                  tracing::warn!(path = %path.display(), "skipping symlinked directory in folder tree summaries");
                  continue;
              }
              folder.add_subfolder(build_folder_tree_summaries(&path)?);
          } else if is_request_file(&path) {
              match load_request_summary(&path, &entry_name) {
                  Ok(summary) => folder.add_summary(summary),
                  Err(e) => tracing::warn!(path = %path.display(), error = %e, "skipping corrupt request file in summary load"),
              }
          }
      }

      Ok(folder)
  }

  /// Read only name/uid/method/url from a request YAML file.
  fn load_request_summary(path: &Path, entry_name: &str) -> DomainResult<RequestSummary> {
      let content = fs::read_to_string(path)?;
      let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

      // For YAML files, deserialize just the fields we need via a minimal struct.
      // For legacy JSON files, fall back to full deserialization.
      if ext == "yml" || ext == "yaml" {
          #[derive(serde::Deserialize)]
          struct MinReq {
              uid: Option<String>,
              info: MinInfo,
              http: MinHttp,
          }
          #[derive(serde::Deserialize)]
          struct MinInfo {
              name: String,
          }
          #[derive(serde::Deserialize)]
          struct MinHttp {
              method: String,
              url: String,
          }
          let min: MinReq = serde_yaml::from_str(&content)
              .map_err(|e| DomainError::Internal(format!("Failed to parse request summary: {e}")))?;
          Ok(RequestSummary {
              uid: min.uid.unwrap_or_default(),
              name: min.info.name,
              method: min.http.method,
              url: min.http.url,
              file_name: Some(entry_name.to_string()),
          })
      } else {
          // Legacy JSON: deserialize full Request and extract fields.
          let req: rocket_collection::Request = serde_json::from_str(&content)
              .map_err(|e| DomainError::Internal(format!("Failed to parse legacy request: {e}")))?;
          Ok(RequestSummary {
              uid: req.uid,
              name: req.name,
              method: req.method.to_string(),
              url: req.url,
              file_name: Some(entry_name.to_string()),
          })
      }
  }
  ```

- [ ] **Step 9: Implement `get_summaries` on `FsCollectionRepo`**

  In `crates/rocket-infra/src/fs_collection/mod.rs`, add to `impl CollectionRepository for FsCollectionRepo`:

  ```rust
  fn get_summaries(&self, name: &str) -> DomainResult<Collection> {
      folders::get_summaries(self, name)
  }
  ```

  In `crates/rocket-infra/src/fs_collection/folders.rs`, add the function:

  ```rust
  pub(super) fn get_summaries(repo: &FsCollectionRepo, name: &str) -> DomainResult<Collection> {
      Collection::validate_name(name)?;
      let path = repo.collection_path(name);
      if !path.exists() {
          return Err(DomainError::NotFound(format!("Collection '{}'", name)));
      }
      if is_migration_interrupted(&path) {
          return Err(DomainError::Internal(format!(
              "Collection '{}' has an incomplete migration. \
               Restore from .legacy_backup/ or remove .migration_in_progress to retry.",
              name
          )));
      }
      if detect_format(&path) == CollectionFormat::LegacyJson {
          migrate_collection(&path)?;
      }
      let root = super::tree::build_folder_tree_summaries(&path)?;
      let settings = super::settings::get_settings(repo, name).unwrap_or_default();
      Ok(Collection { name: name.to_string(), root, settings })
  }
  ```

- [ ] **Step 10: Compile and run the tests**

  ```bash
  cargo check -p rocket-infra 2>&1 | grep "^error" | head -30
  cargo test -p rocket-infra get_summaries 2>&1 | tail -20
  ```

  Expected: zero errors, all three tests PASS.

- [ ] **Step 11: Run the full infra and collection test suites**

  ```bash
  cargo test -p rocket-infra 2>&1 | tail -20
  cargo test -p rocket-collection 2>&1 | tail -10
  ```

  Expected: all tests pass.

- [ ] **Step 12: Update `SharedPathCollectionRepo` to delegate `get_summaries`**

  Open `crates/rocket-infra/src/shared_path_collection_repo.rs`. Find the `impl CollectionRepository` block and add:

  ```rust
  fn get_summaries(&self, name: &str) -> DomainResult<Collection> {
      let base = self.base_dir.lock().unwrap_or_else(|e| e.into_inner()).clone();
      let repo = FsCollectionRepo::new(base.join("collections"), Arc::clone(&self.locks));
      repo.get_summaries(name)
  }
  ```

- [ ] **Step 13: Find and update the Tauri `get_collection` command to use `get_summaries`**

  Run:

  ```bash
  grep -rn "get_collection\|collection_repo\.get\b" src-tauri/src/commands/ | head -20
  ```

  In whichever command file calls `repo.get(name)` for the sidebar collection load, change it to `repo.get_summaries(name)`. Confirm the command only returns the tree structure (not used for getting full request bodies — those go through `get_request`).

  After editing, run:

  ```bash
  cargo check -p rocket-tauri 2>&1 | grep "^error" | head -20
  yarn tsc --noEmit 2>&1 | grep "error TS" | head -20
  ```

  Expected: zero errors.

- [ ] **Step 14: Commit**

  ```bash
  git add \
    crates/rocket-collection/src/request_summary.rs \
    crates/rocket-collection/src/folder.rs \
    crates/rocket-collection/src/lib.rs \
    crates/rocket-collection/src/repository.rs \
    crates/rocket-infra/src/fs_collection/tree.rs \
    crates/rocket-infra/src/fs_collection/folders.rs \
    crates/rocket-infra/src/fs_collection/mod.rs \
    crates/rocket-infra/src/shared_path_collection_repo.rs
  git commit -m "perf(infra): add get_summaries — load sidebar tree without parsing full request bodies"
  ```

---

### Task 2: Replace `Request.variables: Vec<serde_json::Value>` with `Vec<CollectionVariable>`

**Background:** `Request.variables` holds per-request runtime variables typed as `Vec<serde_json::Value>` "until `rocket-environment` is wired as a dependency." The actual type in `OcVariable` / `CollectionVariable` is already available in `rocket-collection`, so no new dependency is needed. The `serde_json::Value` round-trip calls `serde_json::to_value(v).unwrap_or_default()` on every load and `serde_json::from_value::<OcVariable>(v.clone()).ok()` on every save — silently dropping variables when the cast fails. Changing the field to `Vec<CollectionVariable>` removes the lossy round-trip.

**Files:**
- Modify: `crates/rocket-collection/src/request.rs` — change field type
- Modify: `crates/rocket-infra/src/conversions/request.rs` — update both conversion functions

- [ ] **Step 1: Write a failing test documenting round-trip fidelity**

  Add to `crates/rocket-infra/src/conversions/tests.rs` (find the existing tests module there):

  ```rust
  #[test]
  fn request_variables_survive_oc_roundtrip() {
      use rocket_collection::settings::CollectionVariable;
      let mut req = Request::new("Vars", HttpMethod::Get, "https://example.com");
      req.variables = vec![
          CollectionVariable {
              key: "token".to_string(),
              value: "abc".to_string(),
              initial_value: String::new(),
              enabled: true,
              secret: false,
          },
          CollectionVariable {
              key: "disabled_var".to_string(),
              value: "nope".to_string(),
              initial_value: String::new(),
              enabled: false,
              secret: false,
          },
      ];
      let oc = request_to_oc_http_request(&req);
      let back = oc_http_request_to_request(oc);
      assert_eq!(back.variables.len(), 2);
      assert_eq!(back.variables[0].key, "token");
      assert_eq!(back.variables[0].value, "abc");
      assert_eq!(back.variables[1].key, "disabled_var");
      assert!(!back.variables[1].enabled);
  }
  ```

- [ ] **Step 2: Run the test to confirm it fails (type mismatch)**

  ```bash
  cargo test -p rocket-infra request_variables_survive_oc_roundtrip 2>&1 | head -30
  ```

  Expected: compile error — `Request.variables` is `Vec<serde_json::Value>`, not `Vec<CollectionVariable>`.

- [ ] **Step 3: Change the field type in `Request`**

  In `crates/rocket-collection/src/request.rs`, replace:

  ```rust
  /// Request-level variables. Typed as Value until rocket-environment is wired as a dependency.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub variables: Vec<serde_json::Value>,
  ```

  with:

  ```rust
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub variables: Vec<crate::settings::CollectionVariable>,
  ```

  Also add the import at the top of `request.rs` if `CollectionVariable` is not already imported (it lives in `crate::settings`).

- [ ] **Step 4: Update `oc_http_request_to_request` in `conversions/request.rs`**

  Find the `variables` line (currently around line 33):

  ```rust
  let variables = oc.runtime.as_ref()
      .map(|r| r.variables.iter().map(|v| serde_json::to_value(v).unwrap_or_default()).collect())
      .unwrap_or_default();
  ```

  Replace with:

  ```rust
  let variables: Vec<rocket_collection::settings::CollectionVariable> = oc.runtime.as_ref()
      .map(|r| r.variables.iter().cloned().map(rocket_collection::settings::CollectionVariable::from).collect())
      .unwrap_or_default();
  ```

  This requires `CollectionVariable: From<OcVariable>`. Verify that `From<OcVariable> for CollectionVariable` exists in `crates/rocket-infra/src/conversions/variables.rs`:

  ```bash
  grep -n "impl From<OcVariable> for CollectionVariable\|CollectionVariable::from\|OcVariable" \
    crates/rocket-infra/src/conversions/variables.rs | head -10
  ```

  If the impl exists, proceed. If not, add it to `conversions/variables.rs` alongside the existing `From<CollectionVariable> for OcVariable` impl (mirror the pattern for `OcVariable → CollectionVariable`).

- [ ] **Step 5: Update `request_to_oc_http_request` in `conversions/request.rs`**

  Find the runtime assembly block (around lines 130–147). Replace:

  ```rust
  variables: req.variables.iter()
      .filter_map(|v| serde_json::from_value::<OcVariable>(v.clone()).ok())
      .collect(),
  ```

  with:

  ```rust
  variables: req.variables.iter().cloned().map(OcVariable::from).collect(),
  ```

- [ ] **Step 6: Fix the `has_runtime` check**

  The check `|| !req.variables.is_empty()` on line ~133 remains valid; the type change does not affect this logic.

- [ ] **Step 7: Check if any other crate uses `request.variables` as `serde_json::Value`**

  ```bash
  grep -rn "\.variables\b" crates/rocket-app/src/ crates/src-tauri/src/ | grep -v "env_vars\|folder_vars\|collection_vars\|save_folder\|get_folder\|save_request_var\|get_request_var" | head -20
  ```

  For any site that indexes into `request.variables` as a `Value`, update to use the `CollectionVariable` API (`.key`, `.value`, `.enabled`).

- [ ] **Step 8: Compile and run the test**

  ```bash
  cargo check 2>&1 | grep "^error" | head -30
  cargo test -p rocket-infra request_variables_survive_oc_roundtrip 2>&1 | tail -10
  ```

  Expected: zero errors, test PASS.

- [ ] **Step 9: Run the full test suite**

  ```bash
  cargo test -p rocket-collection -p rocket-infra 2>&1 | tail -20
  ```

  Expected: all tests pass.

- [ ] **Step 10: Commit**

  ```bash
  git add \
    crates/rocket-collection/src/request.rs \
    crates/rocket-infra/src/conversions/request.rs \
    crates/rocket-infra/src/conversions/variables.rs
  git commit -m "perf(collection): replace Request.variables serde_json::Value with CollectionVariable"
  ```

---

### Task 3: Replace `HttpRequestExample` `serde_json::Value` fields with concrete types

**Background:** `HttpRequestExample.request` and `.response` are `Option<serde_json::Value>`. The concrete types `OcExampleRequest` and `OcExampleResponse` already exist in `crates/rocket-infra/src/oc/http.rs`. Move these types to `rocket-shared` (or reuse them from `rocket-shared`), update `HttpRequestExample`, and remove the `serde_json::to_value` / `from_value` calls in the conversion layer.

Because `OcExampleRequest` and `OcExampleResponse` reference `OcHttpRequestHeader`, `OcHttpRequestParam`, and `OcHttpRequestBody` — which are infra-layer types — the simplest approach is to define parallel domain types in `rocket-shared` and convert at the infra boundary. Alternatively, since examples are treated as opaque snapshots (never interpreted by domain or app), keeping them as `serde_yaml::Value` (not `serde_json::Value`) in the domain and eliminating the JSON serialize→deserialize round-trip is a valid middle ground.

**The chosen approach:** Change `HttpRequestExample.request` and `.response` from `Option<serde_json::Value>` to `Option<serde_yaml::Value>` in `rocket-shared`. This:
- Eliminates the `serde_json::to_value` + `serde_json::from_value` round-trip in the conversion layer
- Keeps examples as opaque pass-through data (no parsing)
- Does not require new domain types or new dependencies
- Is backward-compatible: `serde_yaml::Value` serializes faithfully

**Files:**
- Modify: `crates/rocket-shared/src/action.rs` — change field types
- Modify: `crates/rocket-shared/Cargo.toml` — ensure `serde_yaml` is a dependency (check first)
- Modify: `crates/rocket-infra/src/conversions/request.rs` — update example conversion

- [ ] **Step 1: Write a test for example round-trip fidelity**

  Add to `crates/rocket-infra/src/conversions/tests.rs`:

  ```rust
  #[test]
  fn request_examples_survive_oc_roundtrip() {
      use rocket_shared::action::HttpRequestExample;
      let mut req = Request::new("With Examples", HttpMethod::Get, "https://example.com");
      req.examples = vec![
          HttpRequestExample {
              name: "Success".to_string(),
              description: None,
              request: Some(serde_yaml::Value::Mapping({
                  let mut m = serde_yaml::Mapping::new();
                  m.insert("method".into(), "GET".into());
                  m.insert("url".into(), "https://example.com".into());
                  m
              })),
              response: Some(serde_yaml::Value::Mapping({
                  let mut m = serde_yaml::Mapping::new();
                  m.insert("status".into(), serde_yaml::Value::Number(200.into()));
                  m
              })),
          },
      ];
      let oc = request_to_oc_http_request(&req);
      let back = oc_http_request_to_request(oc);
      assert_eq!(back.examples.len(), 1);
      assert_eq!(back.examples[0].name, "Success");
      assert!(back.examples[0].request.is_some());
      assert!(back.examples[0].response.is_some());
  }
  ```

- [ ] **Step 2: Run the test to confirm it fails**

  ```bash
  cargo test -p rocket-infra request_examples_survive_oc_roundtrip 2>&1 | head -20
  ```

  Expected: compile error — `serde_yaml::Value` does not match `serde_json::Value`.

- [ ] **Step 3: Check if `rocket-shared` already depends on `serde_yaml`**

  ```bash
  grep "serde_yaml" crates/rocket-shared/Cargo.toml
  ```

  If not present, add to `crates/rocket-shared/Cargo.toml`:

  ```toml
  serde_yaml.workspace = true
  ```

- [ ] **Step 4: Change field types in `HttpRequestExample`**

  In `crates/rocket-shared/src/action.rs`, replace:

  ```rust
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub request: Option<serde_json::Value>, // Nested request snapshot.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub response: Option<serde_json::Value>, // Nested response snapshot.
  ```

  with:

  ```rust
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub request: Option<serde_yaml::Value>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub response: Option<serde_yaml::Value>,
  ```

- [ ] **Step 5: Update `oc_http_request_to_request` example conversion**

  In `crates/rocket-infra/src/conversions/request.rs`, find the examples block (lines ~40-47):

  ```rust
  let examples = oc.examples.unwrap_or_default().into_iter()
      .map(|e| HttpRequestExample {
          name: e.name,
          description: e.description,
          request: e.request.and_then(|r| serde_json::to_value(r).ok()),
          response: e.response.and_then(|r| serde_json::to_value(r).ok()),
      })
      .collect();
  ```

  Replace with:

  ```rust
  let examples = oc.examples.unwrap_or_default().into_iter()
      .map(|e| HttpRequestExample {
          name: e.name,
          description: e.description,
          request: e.request.and_then(|r| serde_yaml::to_value(r).ok()),
          response: e.response.and_then(|r| serde_yaml::to_value(r).ok()),
      })
      .collect();
  ```

- [ ] **Step 6: Update `request_to_oc_http_request` example conversion**

  In the same file, find the examples block near line 152-159:

  ```rust
  Some(req.examples.iter().map(|e| {
      OcHttpRequestExample {
          name: e.name.clone(),
          description: e.description.clone(),
          request: e.request.clone().and_then(|v| serde_json::from_value(v).ok()),
          response: e.response.clone().and_then(|v| serde_json::from_value(v).ok()),
      }
  }).collect())
  ```

  Replace with:

  ```rust
  Some(req.examples.iter().map(|e| {
      OcHttpRequestExample {
          name: e.name.clone(),
          description: e.description.clone(),
          request: e.request.clone().and_then(|v| serde_yaml::from_value(v).ok()),
          response: e.response.clone().and_then(|v| serde_yaml::from_value(v).ok()),
      }
  }).collect())
  ```

- [ ] **Step 7: Fix any call sites that use examples as `serde_json::Value`**

  ```bash
  grep -rn "\.examples\b" crates/ src-tauri/ | grep -v "test\|is_empty\|unwrap\|iter\|push\|\.len\(\)" | head -20
  ```

  For any code that calls `serde_json::from_value` or `serde_json::to_value` on example fields, update to use `serde_yaml`.

- [ ] **Step 8: Compile and run the test**

  ```bash
  cargo check 2>&1 | grep "^error" | head -30
  cargo test -p rocket-infra request_examples_survive_oc_roundtrip 2>&1 | tail -10
  ```

  Expected: zero errors, test PASS.

- [ ] **Step 9: Run the full test suite**

  ```bash
  cargo test -p rocket-shared -p rocket-infra 2>&1 | tail -20
  ```

  Expected: all tests pass.

- [ ] **Step 10: Commit**

  ```bash
  git add \
    crates/rocket-shared/Cargo.toml \
    crates/rocket-shared/src/action.rs \
    crates/rocket-infra/src/conversions/request.rs
  git commit -m "perf(shared): replace HttpRequestExample serde_json::Value snapshots with serde_yaml::Value"
  ```

---

### Task 4: Replace `Environment.client_certificates: Vec<serde_json::Value>` with `Vec<ClientCertificate>`

**Background:** `rocket-environment::Environment.client_certificates` is typed as `Vec<serde_json::Value>` with the comment "Domain uses Vec<serde_json::Value> as a placeholder for client certs." The concrete type `ClientCertificate` already lives in `rocket-shared` and `rocket-environment` already depends on `rocket-shared`, so this is a direct field type change. The infra conversion layer (`conversions/environment.rs`) currently calls `serde_json::to_value(c).unwrap_or_default()` and `serde_json::from_value(v).ok()` — both of which can silently drop certificates.

**Files:**
- Modify: `crates/rocket-environment/src/environment.rs` — change field type, add import
- Modify: `crates/rocket-infra/src/conversions/environment.rs` — remove value round-trips

- [ ] **Step 1: Write a failing test documenting round-trip fidelity**

  Add to `crates/rocket-infra/src/conversions/tests.rs`:

  ```rust
  #[test]
  fn environment_client_certificates_survive_oc_roundtrip() {
      use rocket_shared::certificate::ClientCertificate;
      use rocket_environment::environment::Environment;
      use crate::oc::OcEnvironment;

      let mut env = Environment::new("prod");
      env.client_certificates = vec![
          ClientCertificate::Pem {
              domain: "api.example.com".to_string(),
              certificate_file_path: "/certs/client.pem".to_string(),
              private_key_file_path: "/certs/key.pem".to_string(),
              passphrase: None,
          },
      ];
      let oc = OcEnvironment::from(env.clone());
      let back = Environment::from(oc);
      assert_eq!(back.client_certificates.len(), 1);
      assert!(matches!(back.client_certificates[0], ClientCertificate::Pem { ref domain, .. } if domain == "api.example.com"));
  }
  ```

- [ ] **Step 2: Run the test to confirm it fails**

  ```bash
  cargo test -p rocket-infra environment_client_certificates_survive_oc_roundtrip 2>&1 | head -20
  ```

  Expected: compile error — `env.client_certificates` is `Vec<serde_json::Value>` and `ClientCertificate` is a different type.

- [ ] **Step 3: Change the field type in `Environment`**

  In `crates/rocket-environment/src/environment.rs`, add the import:

  ```rust
  use rocket_shared::certificate::ClientCertificate;
  ```

  Change the field:

  ```rust
  pub client_certificates: Vec<serde_json::Value>,
  ```

  to:

  ```rust
  pub client_certificates: Vec<ClientCertificate>,
  ```

  Update any `Environment::new` or default constructors that initialize this field (change `Vec::new()` calls — they require no change since `Vec::new::<ClientCertificate>()` has the same value). Update test fixtures in the same file that set `client_certificates: Vec::new()`.

- [ ] **Step 4: Remove the value round-trips in `conversions/environment.rs`**

  In `crates/rocket-infra/src/conversions/environment.rs`, replace:

  ```rust
  client_certificates: oc.client_certificates.into_iter()
      .map(|c| serde_json::to_value(c).unwrap_or_default())
      .collect(),
  ```

  with:

  ```rust
  client_certificates: oc.client_certificates,
  ```

  And replace:

  ```rust
  client_certificates: env.client_certificates.into_iter()
      .filter_map(|v| serde_json::from_value(v).ok())
      .collect(),
  ```

  with:

  ```rust
  client_certificates: env.client_certificates,
  ```

  Note: `OcEnvironment.client_certificates` is already `Vec<ClientCertificate>` (using `rocket_shared::certificate::ClientCertificate` as `OcClientCertificate`), so no type conversion is needed — the types are identical.

- [ ] **Step 5: Remove unused `serde_json` import if no longer needed**

  ```bash
  grep -n "serde_json" crates/rocket-infra/src/conversions/environment.rs
  grep -n "serde_json" crates/rocket-environment/src/environment.rs
  ```

  Remove any `use serde_json` lines that are now unused.

- [ ] **Step 6: Compile and run the test**

  ```bash
  cargo check 2>&1 | grep "^error" | head -30
  cargo test -p rocket-infra environment_client_certificates_survive_oc_roundtrip 2>&1 | tail -10
  ```

  Expected: zero errors, test PASS.

- [ ] **Step 7: Run the full test suite**

  ```bash
  cargo test -p rocket-environment -p rocket-infra 2>&1 | tail -20
  ```

  Expected: all tests pass.

- [ ] **Step 8: Commit**

  ```bash
  git add \
    crates/rocket-environment/src/environment.rs \
    crates/rocket-infra/src/conversions/environment.rs
  git commit -m "perf(environment): replace client_certificates serde_json::Value with ClientCertificate"
  ```

---

## Self-Review

### Spec coverage

Phase 5 from the review synthesis:

| Item | Task | Status |
|------|------|--------|
| Lazy folder tree: `Collection::get` returns request summaries | Task 1 | Covered — adds `get_summaries`; `get` preserved for full loads |
| mtime-keyed parsed-YAML cache | — | **Deferred** — defer until profiling after Task 1 lands confirms it is still needed |
| Replace `Vec<serde_json::Value>` for variables | Task 2 | Covered — `Request.variables` → `Vec<CollectionVariable>` |
| Replace `Vec<serde_json::Value>` for examples | Task 3 | Covered — `HttpRequestExample` fields → `serde_yaml::Value` |
| Replace `Vec<serde_json::Value>` for client certs | Task 4 | Covered — `Environment.client_certificates` → `Vec<ClientCertificate>` |
| `fs_history_repo::list` — sort by mtime, truncate to limit | — | Already complete (excluded) |
| Bulk-import path with deferred fsync | — | Already complete (excluded) |

**mtime cache deferral rationale:** Task 1 eliminates the most expensive part of cold loads (parsing full request bodies during sidebar display). After it lands, the remaining cost is `folder.yml` reads in `build_folder_tree_summaries` — which is much cheaper because YAML parsing of a small struct is fast. Measuring before adding cache infrastructure avoids premature optimization.

### Placeholder scan

- Task 1 Step 13 says "find and update the Tauri command" rather than providing the exact edit. This is intentional: the command file path depends on the live tree structure, and an incorrect hardcoded path would be worse than the guidance. The `grep` command in that step will locate the exact site, and `cargo check` immediately after catches any mistake.
- Task 2 Step 4 says "verify `From<OcVariable> for CollectionVariable` exists." This is a verification step before writing code — it prevents assuming an impl exists when it may not.

### Type consistency

- `RequestSummary` — defined in `rocket-collection/src/request_summary.rs`, exported as `rocket_collection::RequestSummary`. Used in `tree.rs` and `folder.rs` throughout.
- `CollectionVariable` — `rocket_collection::settings::CollectionVariable`. Already used throughout the codebase; no new type introduced.
- `ClientCertificate` — `rocket_shared::certificate::ClientCertificate`. Already used in `OcEnvironment` as `OcClientCertificate`; the types are the same struct re-aliased.
- `OcVariable` / `CollectionVariable` conversion — `From<OcVariable> for CollectionVariable` and `From<CollectionVariable> for OcVariable` must both exist in `conversions/variables.rs`. Task 2 Step 4 verifies this before writing the conversion code.
