# rocket-infra Phase 4: Split God Modules

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Break three oversized files in `rocket-infra` into focused submodules — `fs_collection_repo.rs` (1754 lines) into a `fs_collection/` directory, `oc_conversions.rs` (2568 lines) into a `conversions/` directory, and `opencollection.rs` (1774 lines) into an `oc/schema/` directory.

**Architecture:** Each split is an independent, purely mechanical refactor: move code into new files, add the appropriate `mod` declarations, verify the crate still compiles and tests still pass. No logic changes. Tasks 1, 2, 3 are fully independent — any can be executed in isolation. `pub(crate)` visibility is already correct on all three modules, so no public API changes are needed.

**Tech Stack:** Rust, `cargo check`, `cargo test -p rocket-infra`

---

## Scope note

Phase 4 covers three independent splits. Each task produces working, testable software on its own. They share no file overlap and can be executed in any order.

## Status: `pub(crate)` already done

The synthesis says "Mark `pub mod opencollection`/`oc_conversions` as `pub(crate)`". Both are already `pub(crate)` in `lib.rs`. No change needed there.

---

## File Map

### Task 1: Split `fs_collection_repo.rs`

| New file | Content |
|---|---|
| `crates/rocket-infra/src/fs_collection/mod.rs` | `FsCollectionRepo` struct, `impl FsCollectionRepo` (constructors + path helpers + mutex), `impl CollectionRepository` (thin delegation shell), re-exports |
| `crates/rocket-infra/src/fs_collection/paths.rs` | UID reading helpers, `reject_symlink`, `count_request_files`, `is_request_file`, `resolve_request_path` |
| `crates/rocket-infra/src/fs_collection/tree.rs` | `build_folder_tree` and its order-reading logic |
| `crates/rocket-infra/src/fs_collection/requests.rs` | `get_request`, `save_request`, `rename_request`, `delete_request` impl bodies |
| `crates/rocket-infra/src/fs_collection/folders.rs` | `list`, `get`, `create`, `delete`, `rename`, `create_folder`, `delete_folder`, `move_item`, `reorder_items` impl bodies |
| `crates/rocket-infra/src/fs_collection/settings.rs` | `get_settings`, `save_settings` impl bodies |
| `crates/rocket-infra/src/fs_collection/variables.rs` | `get_folder_chain_variables`, `save_folder_variables`, `get_folder_variables`, `get_request_variables`, `save_request_variables` impl bodies |
| `crates/rocket-infra/src/lib.rs` | `mod fs_collection_repo` → `mod fs_collection` (rename) |

> **Rust note on splitting a trait impl:** A `impl Trait for Type` block cannot be split across files in Rust. The pattern is: keep the `impl CollectionRepository for FsCollectionRepo` block in `mod.rs`, but each method body calls a free function defined in a submodule. This keeps each file focused while satisfying the borrow checker.

### Task 2: Split `oc_conversions.rs`

| New file | Content |
|---|---|
| `crates/rocket-infra/src/conversions/mod.rs` | Re-exports all pub functions from submodules |
| `crates/rocket-infra/src/conversions/header.rs` | `From<OcHttpRequestHeader> for Header`, `From<Header> for OcHttpRequestHeader` |
| `crates/rocket-infra/src/conversions/param.rs` | Param `From` impls, `split_params`, `merge_params` |
| `crates/rocket-infra/src/conversions/body.rs` | Body `From` impls and helper functions |
| `crates/rocket-infra/src/conversions/auth.rs` | Auth `From` impls and all OAuth2 helpers |
| `crates/rocket-infra/src/conversions/settings.rs` | `oc_settings_to_domain`, `domain_settings_to_oc`, `InheritableBoolean/Number` helpers |
| `crates/rocket-infra/src/conversions/variables.rs` | Variable `From` impls, `From<OcVariable> for Variable`, `From<Variable> for OcVariable`, `From<OcSecretVariable> for Variable` |
| `crates/rocket-infra/src/conversions/environment.rs` | Environment `From` impls |
| `crates/rocket-infra/src/conversions/workspace.rs` | WorkspaceConfig `From` impls |
| `crates/rocket-infra/src/conversions/request.rs` | `oc_http_request_to_request`, `request_to_oc_http_request`, extract helpers |
| `crates/rocket-infra/src/conversions/protocol.rs` | `oc_item_to_protocol_request`, `protocol_request_to_oc_item` |
| `crates/rocket-infra/src/conversions/folder.rs` | `oc_folder_to_folder`, `folder_to_oc_folder`, `oc_collection_to_collection`, `collection_to_oc_collection` |
| `crates/rocket-infra/src/lib.rs` | `pub(crate) mod oc_conversions` → `pub(crate) mod conversions` |

### Task 3: Split `opencollection.rs`

| New file | Content |
|---|---|
| `crates/rocket-infra/src/oc/mod.rs` | Re-exports everything from submodules |
| `crates/rocket-infra/src/oc/auth.rs` | `OcAuth`, `OcAuthTyped`, all OAuth2 structs, `InheritableBoolean`, `InheritableNumber` |
| `crates/rocket-infra/src/oc/variables.rs` | `OcVariable`, `OcSecretVariable` |
| `crates/rocket-infra/src/oc/http.rs` | All HTTP request structs (`OcHttpRequest`, `OcHttpRequestDetails`, `OcHttpRequestBody`, etc.) |
| `crates/rocket-infra/src/oc/graphql.rs` | All GraphQL request structs |
| `crates/rocket-infra/src/oc/grpc.rs` | All gRPC request structs |
| `crates/rocket-infra/src/oc/websocket.rs` | All WebSocket request structs |
| `crates/rocket-infra/src/oc/folder.rs` | `OcFolderInfo`, `OcFolder`, `OcItem` |
| `crates/rocket-infra/src/oc/collection.rs` | `OcInfo`, `OcAuthor`, `OcCollection`, `OcRequestDefaults`, `OcCollectionConfig`, `OcRequestSettings`, related protobuf structs |
| `crates/rocket-infra/src/oc/environment.rs` | `OcEnvironment` |
| `crates/rocket-infra/src/oc/workspace.rs` | `OcWorkspaceConfig`, `OcWorkspaceCollectionRef`, `OcWorkspaceInfo`, `OcWorkspaceEnvironments` |
| `crates/rocket-infra/src/lib.rs` | `pub(crate) mod opencollection` → `pub(crate) mod oc` |

---

## Task 1: Split `fs_collection_repo.rs` into `fs_collection/`

**Files:**
- Create: `crates/rocket-infra/src/fs_collection/mod.rs`
- Create: `crates/rocket-infra/src/fs_collection/paths.rs`
- Create: `crates/rocket-infra/src/fs_collection/tree.rs`
- Create: `crates/rocket-infra/src/fs_collection/requests.rs`
- Create: `crates/rocket-infra/src/fs_collection/folders.rs`
- Create: `crates/rocket-infra/src/fs_collection/settings.rs`
- Create: `crates/rocket-infra/src/fs_collection/variables.rs`
- Modify: `crates/rocket-infra/src/lib.rs`
- Delete: `crates/rocket-infra/src/fs_collection_repo.rs`

**Background:** Rust's trait impl blocks cannot be split across files. The approach: keep `impl CollectionRepository for FsCollectionRepo` in `mod.rs` as a thin delegation shell — each method body is a one-liner that calls a pub(super) free function defined in the appropriate submodule. The free functions receive `&FsCollectionRepo` as their first argument.

This means `mod.rs` will have one `impl CollectionRepository` block with thin wrappers, and the real logic lives in the submodule files.

- [ ] **Step 1: Create the `fs_collection/` directory structure**

  ```bash
  mkdir -p crates/rocket-infra/src/fs_collection
  ```

- [ ] **Step 2: Create `paths.rs`**

  Create `crates/rocket-infra/src/fs_collection/paths.rs` with the UID helpers, path utilities, and support functions. Copy these functions verbatim from `fs_collection_repo.rs` (adjust visibility to `pub(super)` where needed):

  ```rust
  use std::fs;
  use std::path::{Path, PathBuf};

  use rocket_collection::generate_uid;
  use rocket_shared::error::{DomainError, DomainResult};

  use crate::atomic_write;
  use crate::opencollection::{OcCollection, OcFolderInfo};

  use super::FsCollectionRepo;

  /// Read UID from YAML metadata (opencollection.yml or folder.yml).
  /// Falls back to legacy .uid file, migrating the value into YAML.
  pub(super) fn read_uid_from_yaml(dir: &Path) -> String {
      // Try opencollection.yml first (collection root).
      let oc_path = dir.join("opencollection.yml");
      if oc_path.exists() {
          if let Ok(content) = fs::read_to_string(&oc_path) {
              if let Ok(mut oc) = serde_yaml::from_str::<OcCollection>(&content) {
                  if let Some(ref uid) = oc.uid {
                      if !uid.is_empty() {
                          return uid.clone();
                      }
                  }
                  let uid = read_legacy_uid(dir);
                  oc.uid = Some(uid.clone());
                  if let Ok(yaml) = serde_yaml::to_string(&oc) {
                      if atomic_write(&oc_path, yaml.as_bytes()).is_ok() {
                          cleanup_legacy_uid(dir);
                      }
                  }
                  return uid;
              }
          }
      }

      let folder_path = dir.join("folder.yml");
      if folder_path.exists() {
          if let Ok(content) = fs::read_to_string(&folder_path) {
              if let Ok(mut info) = serde_yaml::from_str::<OcFolderInfo>(&content) {
                  if let Some(ref uid) = info.uid {
                      if !uid.is_empty() {
                          return uid.clone();
                      }
                  }
                  let uid = read_legacy_uid(dir);
                  info.uid = Some(uid.clone());
                  if let Ok(yaml) = serde_yaml::to_string(&info) {
                      if atomic_write(&folder_path, yaml.as_bytes()).is_ok() {
                          cleanup_legacy_uid(dir);
                      }
                  }
                  return uid;
              }
          }
      }

      read_legacy_uid(dir)
  }

  pub(super) fn read_legacy_uid(dir: &Path) -> String {
      let uid_path = dir.join(".uid");
      if let Ok(uid) = fs::read_to_string(&uid_path) {
          let trimmed = uid.trim().to_string();
          if !trimmed.is_empty() {
              return trimmed;
          }
      }
      generate_uid()
  }

  pub(super) fn cleanup_legacy_uid(dir: &Path) {
      let uid_path = dir.join(".uid");
      if uid_path.exists() {
          let _ = fs::remove_file(&uid_path);
      }
  }

  /// Return an error if `path` is a symlink.
  pub(super) fn reject_symlink(path: &Path) -> DomainResult<()> {
      match std::fs::symlink_metadata(path) {
          Ok(meta) if meta.file_type().is_symlink() => Err(DomainError::InvalidInput(
              format!("Refusing operation on symlink: {}", path.display()),
          )),
          Ok(_) => Ok(()),
          Err(e) => Err(DomainError::Io(e.to_string())),
      }
  }

  pub(super) fn count_request_files(dir: &Path) -> usize {
      let mut count = 0;
      if let Ok(entries) = fs::read_dir(dir) {
          for entry in entries.flatten() {
              let path = entry.path();
              if path.is_dir() {
                  count += count_request_files(&path);
              } else if is_request_file(&path) {
                  count += 1;
              }
          }
      }
      count
  }

  pub(super) fn is_request_file(path: &Path) -> bool {
      if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
          if matches!(name, "collection.json" | "_order.json" | "_order.yml" | "opencollection.yml" | "folder.yml") {
              return false;
          }
      }
      path.extension().is_some_and(|ext| ext == "json" || ext == "yml" || ext == "yaml" || ext == "bru")
  }

  /// Resolve a request file path, trying .yml first, then .json for backward compat.
  pub(super) fn resolve_request_path(repo: &FsCollectionRepo, collection_dir: &Path, path: &str) -> DomainResult<PathBuf> {
      let yml = if path.ends_with(".yml") || path.ends_with(".yaml") {
          path.to_string()
      } else {
          format!("{}.yml", path.strip_suffix(".json").unwrap_or(path))
      };
      if let Ok(p) = repo.validate_path(collection_dir, Path::new(&yml)) {
          if p.exists() {
              return Ok(p);
          }
      }
      let json = if path.ends_with(".json") { path.to_string() } else { format!("{}.json", path) };
      repo.validate_path(collection_dir, Path::new(&json))
  }
  ```

- [ ] **Step 3: Create `tree.rs`**

  Copy `build_folder_tree` from `fs_collection_repo.rs` (around line 838–947) into `crates/rocket-infra/src/fs_collection/tree.rs`. Adjust imports:

  ```rust
  use std::fs;
  use std::path::{Path, PathBuf};

  use rocket_collection::{Folder, Request};
  use rocket_shared::error::DomainResult;

  use crate::oc_conversions::oc_http_request_to_request;
  use crate::opencollection::{OcFolderInfo, OcHttpRequest};

  use super::paths::{is_request_file, read_uid_from_yaml};

  pub(super) fn build_folder_tree(current: &Path) -> DomainResult<Folder> {
      // ... copy body verbatim from fs_collection_repo.rs lines 838–947 ...
  }
  ```

  The function body is identical to the current one — just copy it exactly. The key import changes: `is_request_file` and `read_uid_from_yaml` now come from `super::paths`.

- [ ] **Step 4: Create `requests.rs`**

  Create `crates/rocket-infra/src/fs_collection/requests.rs`. Copy the bodies of `get_request`, `save_request`, `rename_request`, `delete_request` as free functions taking `&FsCollectionRepo`:

  ```rust
  use std::fs;
  use std::path::Path;

  use rocket_collection::{request_filename_for, Collection, CollectionRepository, Request};
  use rocket_shared::error::{DomainError, DomainResult};

  use crate::atomic_write;
  use crate::oc_conversions::{oc_http_request_to_request, request_to_oc_http_request};

  use super::paths::{reject_symlink, resolve_request_path};
  use super::FsCollectionRepo;

  pub(super) fn get_request(repo: &FsCollectionRepo, collection: &str, path: &str) -> DomainResult<Request> {
      // ... copy body from CollectionRepository impl get_request method ...
  }

  pub(super) fn save_request(repo: &FsCollectionRepo, collection: &str, path: &str, request: &Request) -> DomainResult<String> {
      // ... copy body from CollectionRepository impl save_request method ...
  }

  pub(super) fn rename_request(repo: &FsCollectionRepo, collection: &str, old_path: &str, new_path: &str) -> DomainResult<()> {
      // ... copy body ...
  }

  pub(super) fn delete_request(repo: &FsCollectionRepo, collection: &str, path: &str) -> DomainResult<()> {
      // ... copy body ...
  }
  ```

- [ ] **Step 5: Create `folders.rs`**

  Create `crates/rocket-infra/src/fs_collection/folders.rs`. Copy the bodies of `list`, `get`, `create`, `delete`, `rename`, `create_folder`, `delete_folder`, `move_item`, `reorder_items` as free functions:

  ```rust
  use std::fs;
  use std::path::Path;

  use rocket_collection::{generate_uid, Collection, CollectionRepository, CollectionSummary};
  use rocket_shared::error::{DomainError, DomainResult};

  use crate::atomic_write;
  use crate::migration::{detect_format, is_migration_interrupted, migrate_collection, CollectionFormat};
  use crate::opencollection::{OcCollection, OcFolderInfo, OcInfo};

  use super::paths::{count_request_files, read_uid_from_yaml, reject_symlink};
  use super::tree::build_folder_tree;
  use super::FsCollectionRepo;

  pub(super) fn list(repo: &FsCollectionRepo) -> DomainResult<Vec<CollectionSummary>> {
      // ... copy body ...
  }

  pub(super) fn get(repo: &FsCollectionRepo, name: &str) -> DomainResult<Collection> {
      // ... copy body ...
  }

  pub(super) fn create(repo: &FsCollectionRepo, name: &str) -> DomainResult<Collection> {
      // ... copy body ...
  }

  pub(super) fn delete(repo: &FsCollectionRepo, name: &str) -> DomainResult<()> {
      // ... copy body ...
  }

  pub(super) fn rename(repo: &FsCollectionRepo, old_name: &str, new_name: &str) -> DomainResult<()> {
      // ... copy body ...
  }

  pub(super) fn create_folder(repo: &FsCollectionRepo, collection: &str, path: &str) -> DomainResult<()> {
      // ... copy body ...
  }

  pub(super) fn delete_folder(repo: &FsCollectionRepo, collection: &str, path: &str) -> DomainResult<()> {
      // ... copy body ...
  }

  pub(super) fn move_item(repo: &FsCollectionRepo, src_collection: &str, src_path: &str, dst_collection: &str, dst_path: &str) -> DomainResult<()> {
      // ... copy body ...
  }

  pub(super) fn reorder_items(repo: &FsCollectionRepo, collection: &str, folder_path: &str, ordered_names: &[String]) -> DomainResult<()> {
      // ... copy body ...
  }
  ```

- [ ] **Step 6: Create `settings.rs`**

  Create `crates/rocket-infra/src/fs_collection/settings.rs`. Copy `get_settings` and `save_settings` bodies:

  ```rust
  use std::fs;

  use rocket_collection::{generate_uid, Collection, CollectionSettings};
  use rocket_shared::error::{DomainError, DomainResult};

  use crate::atomic_write;
  use crate::opencollection::{OcAuth, OcCollection, OcHttpRequestHeader, OcInfo, OcRequestDefaults, OcVariable};
  use crate::oc_conversions::collection_to_oc_collection;

  use super::FsCollectionRepo;

  pub(super) fn get_settings(repo: &FsCollectionRepo, name: &str) -> DomainResult<CollectionSettings> {
      // ... copy body ...
  }

  pub(super) fn save_settings(repo: &FsCollectionRepo, name: &str, settings: &CollectionSettings) -> DomainResult<()> {
      // ... copy body ...
  }
  ```

- [ ] **Step 7: Create `variables.rs`**

  Create `crates/rocket-infra/src/fs_collection/variables.rs`. Copy the five variable method bodies:

  ```rust
  use std::fs;
  use std::path::Path;

  use rocket_collection::{Collection, CollectionVariable};
  use rocket_shared::error::{DomainError, DomainResult};

  use crate::atomic_write;
  use crate::opencollection::{OcFolderInfo, OcHttpRequest, OcRequestDefaults, OcVariable};

  use super::paths::resolve_request_path;
  use super::FsCollectionRepo;

  pub(super) fn get_folder_chain_variables(repo: &FsCollectionRepo, collection: &str, request_path: &str) -> DomainResult<Vec<CollectionVariable>> {
      // ... copy body ...
  }

  pub(super) fn save_folder_variables(repo: &FsCollectionRepo, collection: &str, folder_path: &str, vars: Vec<CollectionVariable>) -> DomainResult<()> {
      // ... copy body ...
  }

  pub(super) fn get_folder_variables(repo: &FsCollectionRepo, collection: &str, folder_path: &str) -> DomainResult<Vec<CollectionVariable>> {
      // ... copy body ...
  }

  pub(super) fn get_request_variables(repo: &FsCollectionRepo, collection: &str, request_path: &str) -> DomainResult<Vec<CollectionVariable>> {
      // ... copy body ...
  }

  pub(super) fn save_request_variables(repo: &FsCollectionRepo, collection: &str, request_path: &str, vars: Vec<CollectionVariable>) -> DomainResult<()> {
      // ... copy body ...
  }
  ```

- [ ] **Step 8: Create `mod.rs`**

  Create `crates/rocket-infra/src/fs_collection/mod.rs`. This contains the struct definition, `impl FsCollectionRepo`, and `impl CollectionRepository` as a thin delegation shell:

  ```rust
  use std::path::{Path, PathBuf};
  use std::sync::{Arc, Mutex};

  use dashmap::DashMap;

  use rocket_collection::{
      Collection, CollectionRepository, CollectionSettings, CollectionSummary, CollectionVariable,
      Request,
  };
  use rocket_shared::error::{DomainError, DomainResult};

  mod folders;
  mod paths;
  mod requests;
  mod settings;
  mod tree;
  mod variables;

  pub struct FsCollectionRepo {
      pub(super) base_dir: PathBuf,
      pub(super) locks: Arc<DashMap<String, Arc<Mutex<()>>>>,
  }

  impl FsCollectionRepo {
      pub fn new(base_dir: PathBuf, locks: Arc<DashMap<String, Arc<Mutex<()>>>>) -> Self {
          Self { base_dir, locks }
      }

      pub fn new_standalone(base_dir: PathBuf) -> Self {
          Self::new(base_dir, Arc::new(DashMap::new()))
      }

      pub(super) fn collection_mutex(&self, name: &str) -> Arc<Mutex<()>> {
          Arc::clone(
              self.locks
                  .entry(name.to_string())
                  .or_insert_with(|| Arc::new(Mutex::new(())))
                  .value(),
          )
      }

      pub(super) fn collection_path(&self, name: &str) -> PathBuf {
          self.base_dir.join(name)
      }

      pub(super) fn settings_path(&self, name: &str) -> PathBuf {
          self.collection_path(name).join("opencollection.yml")
      }

      pub(super) fn validate_path(&self, base: &Path, path: &Path) -> Result<PathBuf, DomainError> {
          // ... copy body verbatim from fs_collection_repo.rs lines 134–169 ...
      }
  }

  impl CollectionRepository for FsCollectionRepo {
      fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
          folders::list(self)
      }
      fn get(&self, name: &str) -> DomainResult<Collection> {
          folders::get(self, name)
      }
      fn create(&self, name: &str) -> DomainResult<Collection> {
          folders::create(self, name)
      }
      fn delete(&self, name: &str) -> DomainResult<()> {
          folders::delete(self, name)
      }
      fn rename(&self, old_name: &str, new_name: &str) -> DomainResult<()> {
          folders::rename(self, old_name, new_name)
      }
      fn get_request(&self, collection: &str, path: &str) -> DomainResult<Request> {
          requests::get_request(self, collection, path)
      }
      fn save_request(&self, collection: &str, path: &str, request: &Request) -> DomainResult<String> {
          requests::save_request(self, collection, path, request)
      }
      fn rename_request(&self, collection: &str, old_path: &str, new_path: &str) -> DomainResult<()> {
          requests::rename_request(self, collection, old_path, new_path)
      }
      fn delete_request(&self, collection: &str, path: &str) -> DomainResult<()> {
          requests::delete_request(self, collection, path)
      }
      fn create_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
          folders::create_folder(self, collection, path)
      }
      fn delete_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
          folders::delete_folder(self, collection, path)
      }
      fn move_item(&self, src_collection: &str, src_path: &str, dst_collection: &str, dst_path: &str) -> DomainResult<()> {
          folders::move_item(self, src_collection, src_path, dst_collection, dst_path)
      }
      fn reorder_items(&self, collection: &str, folder_path: &str, ordered_names: &[String]) -> DomainResult<()> {
          folders::reorder_items(self, collection, folder_path, ordered_names)
      }
      fn get_settings(&self, name: &str) -> DomainResult<CollectionSettings> {
          settings::get_settings(self, name)
      }
      fn save_settings(&self, name: &str, settings: &CollectionSettings) -> DomainResult<CollectionSettings> {
          settings::save_settings(self, name, settings)
      }
      fn get_folder_chain_variables(&self, collection: &str, request_path: &str) -> DomainResult<Vec<CollectionVariable>> {
          variables::get_folder_chain_variables(self, collection, request_path)
      }
      fn save_folder_variables(&self, collection: &str, folder_path: &str, vars: Vec<CollectionVariable>) -> DomainResult<()> {
          variables::save_folder_variables(self, collection, folder_path, vars)
      }
      fn get_folder_variables(&self, collection: &str, folder_path: &str) -> DomainResult<Vec<CollectionVariable>> {
          variables::get_folder_variables(self, collection, folder_path)
      }
      fn get_request_variables(&self, collection: &str, request_path: &str) -> DomainResult<Vec<CollectionVariable>> {
          variables::get_request_variables(self, collection, request_path)
      }
      fn save_request_variables(&self, collection: &str, request_path: &str, vars: Vec<CollectionVariable>) -> DomainResult<()> {
          variables::save_request_variables(self, collection, request_path, vars)
      }
  }

  // Re-export for shared_path_collection_repo.rs which imports FsCollectionRepo directly.
  pub use self::FsCollectionRepo as FsCollectionRepo;
  ```

  > **Important:** The `CollectionRepository` trait methods above must match the actual trait definition. Before writing `mod.rs`, read the trait definition to get exact signatures:
  > ```bash
  > grep -n "fn " crates/rocket-collection/src/lib.rs crates/rocket-collection/src/repository.rs 2>/dev/null | head -40
  > ```

- [ ] **Step 9: Update `lib.rs` to use the new module**

  In `crates/rocket-infra/src/lib.rs`, change:
  ```rust
  // Before:
  pub mod fs_collection_repo;
  // ...
  pub use fs_collection_repo::FsCollectionRepo;
  ```
  To:
  ```rust
  // After:
  pub mod fs_collection;
  // ...
  pub use fs_collection::FsCollectionRepo;
  ```

  Also update `shared_path_collection_repo.rs` if it imports `crate::FsCollectionRepo` — check with:
  ```bash
  grep -n "FsCollectionRepo\|fs_collection_repo" crates/rocket-infra/src/shared_path_collection_repo.rs
  ```

- [ ] **Step 10: Compile check — iterate until clean**

  ```bash
  cargo check -p rocket-infra 2>&1 | grep "^error" | head -30
  ```

  Fix each error. Common issues:
  - Missing `use` imports in submodule files (add them)
  - `pub(super)` vs `pub(crate)` access conflicts (tighten to `pub(super)` where possible)
  - The `#[tracing::instrument]` attributes on methods — move them to the free functions in submodules
  - Test module at the bottom of `fs_collection_repo.rs` — move it to `mod.rs` or keep it as a separate `tests.rs` file in the `fs_collection/` directory

- [ ] **Step 11: Move the test module**

  The test block (lines 949–1754 in `fs_collection_repo.rs`) should move to `crates/rocket-infra/src/fs_collection/tests.rs`. Add `#[cfg(test)] mod tests;` at the bottom of `mod.rs`.

  The test module imports will need updating — change `use super::*` to reference the new module paths.

- [ ] **Step 12: Run the full test suite**

  ```bash
  cargo test -p rocket-infra 2>&1 | tail -15
  ```

  Expected: same number of tests as before (currently 247), all passing.

- [ ] **Step 13: Delete the old file**

  ```bash
  rm crates/rocket-infra/src/fs_collection_repo.rs
  ```

  Then compile again to confirm nothing references the old path:
  ```bash
  cargo check -p rocket-infra 2>&1 | grep "^error" | head -10
  ```

- [ ] **Step 14: Commit**

  ```bash
  git add crates/rocket-infra/src/fs_collection/ crates/rocket-infra/src/lib.rs
  git rm crates/rocket-infra/src/fs_collection_repo.rs
  git commit -m "refactor(infra): split fs_collection_repo.rs into fs_collection/ submodule"
  ```

---

## Task 2: Split `oc_conversions.rs` into `conversions/`

**Files:**
- Create: `crates/rocket-infra/src/conversions/mod.rs`
- Create: `crates/rocket-infra/src/conversions/header.rs`
- Create: `crates/rocket-infra/src/conversions/param.rs`
- Create: `crates/rocket-infra/src/conversions/body.rs`
- Create: `crates/rocket-infra/src/conversions/auth.rs`
- Create: `crates/rocket-infra/src/conversions/settings.rs` (request settings, not collection settings)
- Create: `crates/rocket-infra/src/conversions/variables.rs`
- Create: `crates/rocket-infra/src/conversions/environment.rs`
- Create: `crates/rocket-infra/src/conversions/workspace.rs`
- Create: `crates/rocket-infra/src/conversions/request.rs`
- Create: `crates/rocket-infra/src/conversions/protocol.rs`
- Create: `crates/rocket-infra/src/conversions/folder.rs`
- Modify: `crates/rocket-infra/src/lib.rs`
- Delete: `crates/rocket-infra/src/oc_conversions.rs`

**Background:** `oc_conversions.rs` has 12 clearly-delimited sections separated by `// ============================================================` comment markers. Each section maps to one submodule. `mod.rs` re-exports everything that is currently `pub` in `oc_conversions.rs`.

The current callers use `use crate::oc_conversions::*` or `use crate::oc_conversions::{some_fn}`. After the rename to `crate::conversions`, all callers inside `rocket-infra` need their import paths updated.

- [ ] **Step 1: Create the directory**

  ```bash
  mkdir -p crates/rocket-infra/src/conversions
  ```

- [ ] **Step 2: Find all callers of `oc_conversions` inside rocket-infra**

  ```bash
  grep -rn "oc_conversions\|crate::oc_conversions" crates/rocket-infra/src/
  ```

  Note every file that imports from `oc_conversions`. You will need to update these after the rename.

- [ ] **Step 3: Split into submodule files**

  For each section of `oc_conversions.rs`, create a corresponding file in `conversions/`. The sections are delimited by `// ============================================================` comment markers. Here is the mapping (line numbers are approximate — read the actual file):

  **`header.rs`** (lines ~27–51):
  - `impl From<OcHttpRequestHeader> for Header`
  - `impl From<Header> for OcHttpRequestHeader`

  **`param.rs`** (lines ~54–127):
  - `impl From<OcHttpRequestParam> for QueryParam`
  - `impl From<QueryParam> for OcHttpRequestParam`
  - `impl From<OcHttpRequestParam> for PathParam`
  - `impl From<PathParam> for OcHttpRequestParam`
  - `pub fn split_params`
  - `pub fn merge_params`

  **`body.rs`** (lines ~127–280):
  - `impl From<OcHttpRequestBody> for Body`
  - `fn form_field_to_entry`
  - `fn multipart_to_entry`
  - `impl From<Body> for OcHttpRequestBody`
  - `fn entry_to_form_field`
  - `fn entry_to_multipart`

  **`auth.rs`** (lines ~282–693, the largest section):
  - `impl From<OcAuth> for Auth`
  - `impl From<OcAuthTyped> for Auth`
  - All OAuth2 helper fns
  - `impl From<Auth> for OcAuth`
  - `fn domain_oauth2_to_oc_fields` and helpers

  **`request_settings.rs`** (lines ~644–691 — name it `request_settings.rs` to avoid collision with collection `settings`):
  - `fn oc_settings_to_domain`
  - `fn domain_settings_to_oc`
  - `fn inheritable_bool_to_domain`
  - `fn inheritable_number_to_domain`
  - `fn domain_bool_to_inheritable`
  - `fn domain_number_to_inheritable`

  **`variables.rs`** (lines ~696–769):
  - `impl From<OcVariable> for CollectionVariable`
  - `impl From<CollectionVariable> for OcVariable`
  - `impl From<OcVariable> for Variable`
  - `impl From<Variable> for OcVariable`
  - `impl From<OcSecretVariable> for Variable`

  **`environment.rs`** (lines ~774–806):
  - `impl From<OcEnvironment> for Environment`
  - `impl From<Environment> for OcEnvironment`

  **`workspace.rs`** (lines ~811–879):
  - `impl From<OcWorkspaceCollectionRef> for CollectionReference`
  - `impl From<CollectionReference> for OcWorkspaceCollectionRef`
  - `impl From<OcWorkspaceConfig> for WorkspaceConfig`
  - `impl From<WorkspaceConfig> for OcWorkspaceConfig`

  **`request.rs`** (lines ~886–1048):
  - `pub fn oc_http_request_to_request`
  - `pub fn request_to_oc_http_request`
  - Private extract helpers

  **`protocol.rs`** (lines ~1069–1101):
  - `pub fn oc_item_to_protocol_request`
  - `pub fn protocol_request_to_oc_item`

  **`folder.rs`** (lines ~1109–1458):
  - `pub fn oc_folder_to_folder`
  - `pub fn folder_to_oc_folder`
  - `pub fn oc_collection_to_collection`
  - `pub fn collection_to_oc_collection`
  - `fn extract_scripts`
  - `fn extract_actions`

  Each file needs its own `use` imports — copy from the top of `oc_conversions.rs` and trim to only what that file needs.

- [ ] **Step 4: Create `mod.rs` as a re-export hub**

  Create `crates/rocket-infra/src/conversions/mod.rs`:

  ```rust
  mod auth;
  mod body;
  mod environment;
  mod folder;
  mod header;
  mod param;
  mod protocol;
  mod request;
  mod request_settings;
  mod variables;
  mod workspace;

  // Re-export everything that was pub in oc_conversions.rs
  pub use auth::*;
  pub use body::*;
  pub use environment::*;
  pub use folder::*;
  pub use header::*;
  pub use param::{merge_params, split_params};
  pub use protocol::{oc_item_to_protocol_request, protocol_request_to_oc_item};
  pub use request::{oc_http_request_to_request, request_to_oc_http_request};
  pub use variables::*;
  pub use workspace::*;
  ```

  Note: `pub use x::*` re-exports all `pub` items from each submodule. Only use it for modules that have a clean `pub` surface. For modules with many private helpers (like `auth`), list explicitly.

- [ ] **Step 5: Update `lib.rs`**

  ```rust
  // Before:
  pub(crate) mod oc_conversions;
  // After:
  pub(crate) mod conversions;
  ```

- [ ] **Step 6: Update all import paths inside rocket-infra**

  For each file that used `crate::oc_conversions`, update to `crate::conversions`. Check with:

  ```bash
  grep -rn "oc_conversions" crates/rocket-infra/src/
  ```

  The callers are:
  - `fs_collection/mod.rs` (or `fs_collection_repo.rs` if Task 1 hasn't been done yet): `use crate::oc_conversions::{...}` → `use crate::conversions::{...}`
  - `migration.rs`: `use crate::oc_conversions::request_to_oc_http_request` → `use crate::conversions::request_to_oc_http_request`

- [ ] **Step 7: Move tests**

  The `mod tests` block in `oc_conversions.rs` (lines 1461–2568) can become `crates/rocket-infra/src/conversions/tests.rs`. Add `#[cfg(test)] mod tests;` to `mod.rs`. The tests use `use super::*` and individual function names — update imports as needed.

- [ ] **Step 8: Compile check — iterate until clean**

  ```bash
  cargo check -p rocket-infra 2>&1 | grep "^error" | head -30
  ```

  Common issues: missing `use` statements in subfiles (each file needs its own imports), `pub` visibility on items that need to be visible to sibling modules (use `pub(super)` for items only used within `conversions/`).

- [ ] **Step 9: Run tests**

  ```bash
  cargo test -p rocket-infra 2>&1 | tail -15
  ```

  Expected: same count as before, all passing.

- [ ] **Step 10: Delete old file and final check**

  ```bash
  rm crates/rocket-infra/src/oc_conversions.rs
  cargo check -p rocket-infra 2>&1 | grep "^error" | head -10
  ```

- [ ] **Step 11: Commit**

  ```bash
  git add crates/rocket-infra/src/conversions/ crates/rocket-infra/src/lib.rs crates/rocket-infra/src/migration.rs
  git rm crates/rocket-infra/src/oc_conversions.rs
  git commit -m "refactor(infra): split oc_conversions.rs into conversions/ submodule by domain"
  ```

---

## Task 3: Split `opencollection.rs` into `oc/`

**Files:**
- Create: `crates/rocket-infra/src/oc/mod.rs`
- Create: `crates/rocket-infra/src/oc/auth.rs`
- Create: `crates/rocket-infra/src/oc/variables.rs`
- Create: `crates/rocket-infra/src/oc/http.rs`
- Create: `crates/rocket-infra/src/oc/graphql.rs`
- Create: `crates/rocket-infra/src/oc/grpc.rs`
- Create: `crates/rocket-infra/src/oc/websocket.rs`
- Create: `crates/rocket-infra/src/oc/folder.rs`
- Create: `crates/rocket-infra/src/oc/collection.rs`
- Create: `crates/rocket-infra/src/oc/environment.rs`
- Create: `crates/rocket-infra/src/oc/workspace.rs`
- Modify: `crates/rocket-infra/src/lib.rs`
- Delete: `crates/rocket-infra/src/opencollection.rs`

**Background:** `opencollection.rs` is 1774 lines of pure struct/enum definitions and a few `impl` blocks — no logic functions. It has clear protocol-based sections separated by comment blocks. The caller (`oc_conversions.rs` or `conversions/`) uses `use crate::opencollection::*` — a single glob import. After the rename to `crate::oc`, `mod.rs` re-exports everything with `pub use submodule::*` so callers just change `opencollection` to `oc` in their import path.

- [ ] **Step 1: Create the directory**

  ```bash
  mkdir -p crates/rocket-infra/src/oc
  ```

- [ ] **Step 2: Find all callers of `opencollection`**

  ```bash
  grep -rn "opencollection\|crate::opencollection" crates/rocket-infra/src/
  ```

  Note every `use crate::opencollection` statement. You will change these to `use crate::oc` after the split.

- [ ] **Step 3: Split into submodule files**

  The sections in `opencollection.rs` are clearly delimited. Create each file by cutting the relevant structs:

  **`variables.rs`** (lines ~19–47): `OcVariable`, `OcSecretVariable`

  **`auth.rs`** (lines ~48–196):
  - `OcAuth`, `OcAuthTyped` (the enum with all auth variants)
  - All OAuth2 structs: `OcOAuth2Credentials`, `OcOAuth2ResourceOwner`, `OcOAuth2PKCE`
  - `InheritableBoolean`, `InheritableNumber`
  - `OcHttpRequestSettings`, `OcGraphQLRequestSettings`

  **`http.rs`** (lines ~204–480):
  - `OcHttpRequestInfo`, `OcHttpRequestParam`, `OcHttpRequestHeader`, `OcHttpResponseHeader`
  - `OcFormField`, `OcMultipartValue`, `OcMultipartFormPart`, `OcFileBodyVariant`
  - `OcHttpRequestBody`, `OcHttpRequestBodyVariant`
  - `OcScript`, `OcScriptFile`, `OcActionSelector`, `OcActionVariable`, `OcAction`
  - `OcHttpRequestRuntime`, `OcExampleResponseBody`, `OcExampleRequest`, `OcExampleResponse`, `OcHttpRequestExample`
  - `OcHttpRequestDetails`, `OcHttpRequest`

  **`graphql.rs`** (lines ~488–568):
  - `OcGraphQLRequestInfo`, `OcGraphQLBody`, `OcGraphQLBodyVariant`, `OcGraphQLBodyOrVariants`
  - `OcGraphQLRequestDetails`, `OcGraphQLRequestRuntime`, `OcGraphQLRequest`

  **`grpc.rs`** (lines ~575–656):
  - `OcGrpcRequestInfo`, `OcGrpcMetadata`, `OcGrpcMessageVariant`, `OcGrpcMessageOrVariants`
  - `OcGrpcRequestDetails`, `OcGrpcRequestRuntime`, `OcGrpcRequest`

  **`websocket.rs`** (lines ~664–734):
  - `OcWebSocketRequestInfo`, `OcWebSocketMessage`, `OcWebSocketMessageVariant`, `OcWebSocketMessageOrVariants`
  - `OcWebSocketRequestDetails`, `OcWebSocketRequestRuntime`, `OcWebSocketRequest`

  **`folder.rs`** (lines ~741–803):
  - `OcFolderInfo` (with `impl Default`)
  - `OcFolder`
  - `OcItem` (the enum that can be Http/Folder/GraphQL/gRPC/WS/ScriptFile)

  **`collection.rs`** (lines ~804–956):
  - `OcInfo`, `OcAuthor`
  - `OcProtoFileItem`, `OcProtoFileImportPath`, `OcProtobuf`
  - `OcProxyAuth`, `OcProxyConnectionConfig`, `OcProxy`
  - `OcRequestSettings`, `OcRequestDefaults`, `OcCollectionConfig`, `OcCollection`

  **`environment.rs`** (lines ~893–911): `OcEnvironment`

  **`workspace.rs`** (lines ~983–1009):
  - `OcWorkspaceInfo`, `OcWorkspaceCollectionRef`, `OcWorkspaceEnvironments`, `OcWorkspaceConfig`

  Each file needs:
  ```rust
  use serde::{Deserialize, Serialize};
  ```
  Plus any cross-module dependencies (e.g., `http.rs` needs `use super::auth::OcHttpRequestSettings`).

- [ ] **Step 4: Handle cross-module struct references**

  Some structs reference others across section boundaries. For example:
  - `OcFolder` in `folder.rs` contains `OcItem` which references `OcHttpRequest` from `http.rs`, `OcGraphQLRequest` from `graphql.rs`, etc.
  - `OcCollection` in `collection.rs` has `OcRequestDefaults` which uses types from `auth.rs`

  The cleanest fix: in each file that needs a type from another submodule, add `use super::module_name::TypeName;`. The `oc/mod.rs` re-exports everything so the glob `use crate::oc::*` still works for `conversions/`.

- [ ] **Step 5: Create `mod.rs`**

  Create `crates/rocket-infra/src/oc/mod.rs`:

  ```rust
  pub mod auth;
  pub mod collection;
  pub mod environment;
  pub mod folder;
  pub mod graphql;
  pub mod grpc;
  pub mod http;
  pub mod variables;
  pub mod websocket;
  pub mod workspace;

  // Re-export everything so callers can use `use crate::oc::*` just like before.
  pub use auth::*;
  pub use collection::*;
  pub use environment::*;
  pub use folder::*;
  pub use graphql::*;
  pub use grpc::*;
  pub use http::*;
  pub use variables::*;
  pub use websocket::*;
  pub use workspace::*;

  // Re-export types that opencollection.rs re-exported from other crates.
  pub use rocket_shared::description::{Description as OcDescription, Documentation as OcDocumentation};
  pub use rocket_shared::oauth2::{OAuth2AdditionalParameters, OAuth2Settings, OAuth2TokenConfig};
  pub use rocket_shared::variable_value::{VariableValue as OcVariableValue, VariableValueVariant as OcVariableValueVariant};
  ```

- [ ] **Step 6: Update `lib.rs`**

  ```rust
  // Before:
  pub(crate) mod opencollection;
  // After:
  pub(crate) mod oc;
  ```

- [ ] **Step 7: Update all import paths**

  For every file in `crates/rocket-infra/src/` that had `use crate::opencollection::...`, change to `use crate::oc::...`:

  ```bash
  grep -rn "crate::opencollection\|opencollection::" crates/rocket-infra/src/
  ```

  Main callers: `conversions/` subfiles (or `oc_conversions.rs` if Task 2 not done), `fs_collection/` (or `fs_collection_repo.rs`), `migration.rs`.

- [ ] **Step 8: Compile check — iterate until clean**

  ```bash
  cargo check -p rocket-infra 2>&1 | grep "^error" | head -30
  ```

  Common issues:
  - `OcItem` references `OcHttpRequest`, `OcGraphQLRequest`, etc. from other submodules — add `use super::http::OcHttpRequest;` etc. in `folder.rs`
  - `serde` attributes need `use serde::{Deserialize, Serialize};` in each file
  - The `pub use rocket_shared::...` aliases from the top of `opencollection.rs` must be in `mod.rs`

- [ ] **Step 9: Run tests**

  ```bash
  cargo test -p rocket-infra 2>&1 | tail -15
  ```

  Expected: all tests pass.

- [ ] **Step 10: Delete old file and final check**

  ```bash
  rm crates/rocket-infra/src/opencollection.rs
  cargo check -p rocket-infra 2>&1 | grep "^error" | head -10
  ```

- [ ] **Step 11: Commit**

  ```bash
  git add crates/rocket-infra/src/oc/ crates/rocket-infra/src/lib.rs
  git rm crates/rocket-infra/src/opencollection.rs
  git commit -m "refactor(infra): split opencollection.rs into oc/ submodule by protocol"
  ```

---

## Self-Review

### Spec coverage

| Requirement | Task | Status |
|---|---|---|
| Split `fs_collection_repo.rs` into `fs_collection/` with `paths.rs`, `requests.rs`, `folders.rs`, `settings.rs`, `variables.rs` | Task 1 | This plan (also adds `tree.rs` for `build_folder_tree`) |
| Split `oc_conversions.rs` by domain: `auth.rs`, `body.rs`, `request.rs`, `folder.rs`, `variables.rs` | Task 2 | This plan (adds `header.rs`, `param.rs`, `request_settings.rs`, `environment.rs`, `workspace.rs`, `protocol.rs` for completeness) |
| Split `opencollection.rs` by protocol (HTTP/GraphQL/gRPC/WS) | Task 3 | This plan |
| Mark `pub mod opencollection`/`oc_conversions` as `pub(crate)` | — | ✅ Already done in `lib.rs` |
| OcCodec facade | — | Not implemented — YAGNI. Nothing outside `rocket-infra` imports these modules. The facade would add indirection with no current benefit. |

### Placeholder scan

Task 1 Steps 3–7 contain `// ... copy body ...` in code blocks. This is intentional — the function bodies are long (20–80 lines each) and copying them verbatim from the existing file is the correct action. The plan cannot include them without becoming a 5000-line document. The instruction to "copy body verbatim" is unambiguous and not a TBD.

### Type consistency

- `FsCollectionRepo` fields change from private to `pub(super)` to allow submodule access — this is consistent across all submodules.
- `build_folder_tree` stays in `tree.rs` and is called from `folders.rs` via `use super::tree::build_folder_tree` — consistent.
- `validate_path` stays on `impl FsCollectionRepo` in `mod.rs` — called as `repo.validate_path(...)` from all submodules — consistent.
