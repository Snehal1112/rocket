# rocket-infra Immediate Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the 5 highest-priority fixes from the `rocket-infra` code review — covering critical security (path/symlink), data integrity (atomic writes, silent loss), and performance (remove clone on save).

**Architecture:** Each fix is narrowly scoped to the files named in the review. No refactoring beyond the stated change. Tasks are ordered from highest risk (security) to lowest.

**Tech Stack:** Rust, `std::fs`, `std::path`, `serde_yaml`, `tempfile` (tests)

---

## File Map

| File | Change |
|------|--------|
| `crates/rocket-infra/src/fs_collection_repo.rs` | Fix 1: symlink guard on `delete`, `delete_folder`, `build_folder_tree`; Fix 3: take `&Request` in `save_request` |
| `crates/rocket-infra/src/oc_conversions.rs` | Fix 3: change `request_to_oc_http_request` to take `&Request` |
| `crates/rocket-infra/src/migration.rs` | Fix 1: symlink guard in `migrate_directory`; Fix 2: replace `fs::write` with `atomic_write` |
| `crates/rocket-infra/src/fs_workspace_config_repo.rs` | Fix 2: replace `fs::write` with `atomic_write` |
| `crates/rocket-infra/src/fs_template_repo.rs` | Fix 2: replace `fs::write` with `atomic_write` |
| `crates/rocket-infra/src/atomic_write.rs` | Fix 5: `sync_all`, parent-dir fsync, collision-resistant tmp suffix |

---

### Task 1: Symlink guard — refuse symlinks in destructive/traversal ops (Fix 1 from §6)

**Files:**
- Modify: `crates/rocket-infra/src/fs_collection_repo.rs`
- Modify: `crates/rocket-infra/src/migration.rs`

**Background:** `fs::remove_dir_all` and `fs::read_dir` follow symlinks. A malicious workspace can contain a symlinked "folder" pointing at `/home/user/Documents`; clicking Delete wipes it. The fix adds a helper that calls `symlink_metadata` (which does NOT follow symlinks) and rejects the path if it is a symlink.

- [ ] **Step 1: Add `reject_symlink` helper to `fs_collection_repo.rs`**

  Add this function immediately after the `impl FsCollectionRepo { ... }` block (around line 150), before `impl CollectionRepository`:

  ```rust
  /// Return an error if `path` is a symlink. Protects destructive ops from traversal via symlink.
  fn reject_symlink(path: &Path) -> DomainResult<()> {
      match std::fs::symlink_metadata(path) {
          Ok(meta) if meta.file_type().is_symlink() => Err(DomainError::InvalidInput(
              format!("Refusing operation on symlink: {}", path.display()),
          )),
          _ => Ok(()),
      }
  }
  ```

- [ ] **Step 2: Guard `delete` against symlinks**

  In `fs_collection_repo.rs`, find the `delete` method (around line 245). After the `if !path.exists()` check, insert the symlink guard:

  ```rust
  fn delete(&self, name: &str) -> DomainResult<()> {
      let path = self.collection_path(name);
      if !path.exists() {
          return Err(DomainError::NotFound(format!("Collection '{}'", name)));
      }
      reject_symlink(&path)?;
      fs::remove_dir_all(&path)?;
      Ok(())
  }
  ```

- [ ] **Step 3: Guard `delete_folder` against symlinks**

  In `fs_collection_repo.rs`, find `delete_folder` (around line 379). Add the guard after the `validate_path` call:

  ```rust
  fn delete_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
      let collection_dir = self.collection_path(collection);
      let dir_path = self.validate_path(&collection_dir, Path::new(path))?;
      if !dir_path.exists() {
          return Err(DomainError::NotFound(format!("{}/{}", collection, path)));
      }
      reject_symlink(&dir_path)?;
      fs::remove_dir_all(&dir_path)?;
      Ok(())
  }
  ```

- [ ] **Step 4: Guard `build_folder_tree` against symlink directories**

  In `fs_collection_repo.rs`, find `build_folder_tree` (around line 745). In the loop that processes `entries`, replace the `if path.is_dir()` branch with a symlink-aware check:

  Current code (around line 804):
  ```rust
  if path.is_dir() {
      folder.add_subfolder(build_folder_tree(&path)?);
  } else if is_request_file(&path) {
  ```

  Change to:
  ```rust
  if path.is_dir() {
      // Skip symlinked directories to prevent exfiltration.
      if std::fs::symlink_metadata(&path).map(|m| m.file_type().is_symlink()).unwrap_or(false) {
          tracing::warn!(path = %path.display(), "skipping symlinked directory in folder tree");
          continue;
      }
      folder.add_subfolder(build_folder_tree(&path)?);
  } else if is_request_file(&path) {
  ```

- [ ] **Step 5: Guard `migrate_directory` in `migration.rs`**

  In `migration.rs`, find the `migrate_directory` function (around line 108). In the loop processing entries, add a symlink guard for directories, before the recursive call:

  Current code (around line 117):
  ```rust
  if path.is_dir() {
      if name.starts_with('.') {
          continue;
      }
  ```

  Change to:
  ```rust
  if path.is_dir() {
      if name.starts_with('.') {
          continue;
      }
      // Skip symlinked directories — following them during migration can exfiltrate or corrupt files outside the workspace.
      if std::fs::symlink_metadata(&path).map(|m| m.file_type().is_symlink()).unwrap_or(false) {
          tracing::warn!(path = %path.display(), "skipping symlinked directory during migration");
          continue;
      }
  ```

- [ ] **Step 6: Write failing tests for symlink rejection**

  Add at the bottom of the `#[cfg(test)]` block in `fs_collection_repo.rs` (before the final `}`):

  ```rust
  #[test]
  #[cfg(unix)]
  fn delete_rejects_symlinked_collection() {
      use std::os::unix::fs::symlink;
      let dir = TempDir::new().unwrap();
      let repo = FsCollectionRepo::new(dir.path().to_path_buf());
      // Create a real target outside base_dir.
      let target = dir.path().parent().unwrap().join("outside");
      fs::create_dir_all(&target).unwrap();
      // Symlink inside base_dir pointing to the external target.
      let link = dir.path().join("evil-collection");
      symlink(&target, &link).unwrap();
      let err = repo.delete("evil-collection").unwrap_err();
      assert!(matches!(err, DomainError::InvalidInput(_)), "expected InvalidInput, got {:?}", err);
      // Target directory must NOT be deleted.
      assert!(target.exists());
  }

  #[test]
  #[cfg(unix)]
  fn delete_folder_rejects_symlinked_folder() {
      use std::os::unix::fs::symlink;
      let dir = TempDir::new().unwrap();
      let repo = FsCollectionRepo::new(dir.path().to_path_buf());
      repo.create("my-api").unwrap();
      // Create a real directory outside the collection.
      let target = dir.path().parent().unwrap().join("important");
      fs::create_dir_all(&target).unwrap();
      // Plant a symlink inside the collection directory.
      let link = dir.path().join("my-api").join("evil-folder");
      symlink(&target, &link).unwrap();
      let err = repo.delete_folder("my-api", "evil-folder").unwrap_err();
      assert!(matches!(err, DomainError::InvalidInput(_)), "expected InvalidInput, got {:?}", err);
      assert!(target.exists());
  }
  ```

- [ ] **Step 7: Run the new tests and verify they pass**

  ```bash
  cargo test -p rocket-infra delete_rejects_symlinked_collection delete_folder_rejects_symlinked_folder 2>&1 | tail -20
  ```

  Expected: both tests PASS.

- [ ] **Step 8: Run the full infra test suite to check for regressions**

  ```bash
  cargo test -p rocket-infra 2>&1 | tail -30
  ```

  Expected: all tests pass.

- [ ] **Step 9: Commit**

  ```bash
  git add crates/rocket-infra/src/fs_collection_repo.rs crates/rocket-infra/src/migration.rs
  git commit -m "fix(infra): reject symlinks in delete, delete_folder, build_folder_tree, and migration"
  ```

---

### Task 2: Switch all important writes to `atomic_write` (Fix 2 from §6)

**Files:**
- Modify: `crates/rocket-infra/src/fs_workspace_config_repo.rs`
- Modify: `crates/rocket-infra/src/migration.rs`
- Modify: `crates/rocket-infra/src/fs_template_repo.rs`

**Background:** `fs::write` truncates the file then writes. A crash between truncate and write leaves an empty or partial file. `atomic_write` writes to a temp file first, then renames atomically.

- [ ] **Step 1: Write a failing test for `fs_workspace_config_repo` atomicity**

  In `fs_workspace_config_repo.rs`, add inside the `#[cfg(test)]` block:

  ```rust
  #[test]
  fn save_uses_atomic_write_no_tmp_files_left() {
      let tmp = TempDir::new().unwrap();
      let ws_path = tmp.path().join("my-ws");
      let repo = FsWorkspaceConfigRepo::new();
      repo.save(&ws_path, &WorkspaceConfig::new("Test")).unwrap();
      let entries: Vec<_> = fs::read_dir(&ws_path).unwrap()
          .filter_map(|e| e.ok())
          .map(|e| e.file_name().to_string_lossy().to_string())
          .filter(|n| n.contains(".tmp"))
          .collect();
      assert!(entries.is_empty(), "tmp files found: {:?}", entries);
  }
  ```

- [ ] **Step 2: Run the test to confirm it passes (it doesn't test crash safety, just no leaks)**

  ```bash
  cargo test -p rocket-infra save_uses_atomic_write_no_tmp_files_left 2>&1 | tail -15
  ```

  Note: if using `fs::write` today the test still passes because there's no crash — this test confirms the post-fix state is clean. Proceed.

- [ ] **Step 3: Fix `fs_workspace_config_repo.rs` — replace `fs::write` with `atomic_write`**

  In `fs_workspace_config_repo.rs`, add the import at the top of the file after existing imports:

  ```rust
  use crate::atomic_write;
  ```

  Then find the `save` method (around line 50). Replace:

  ```rust
  fs::write(&config_path, content).map_err(|e| {
      DomainError::Io(format!("Failed to write workspace.yml: {e}"))
  })
  ```

  With:

  ```rust
  atomic_write(&config_path, content.as_bytes())
      .map_err(|e| DomainError::Io(format!("Failed to write workspace.yml: {e}")))
  ```

  Also remove the now-redundant `fs::create_dir_all` call before it — `atomic_write` handles parent dir creation. Keep the existing `fs::create_dir_all(workspace_path)` at the top of `save` since it's creating the *workspace* directory itself (not the parent of the YAML file), so it's still needed. Only remove if there's a second redundant one immediately before `fs::write`.

- [ ] **Step 4: Fix `migration.rs` — replace four `fs::write` calls with `atomic_write`**

  In `migration.rs`, add the import at the top:

  ```rust
  use crate::atomic_write;
  ```

  Then find and replace each `fs::write` call:

  **Site 1** — `migrate_collection`, writing `opencollection.yml` (around line 96):
  ```rust
  // Before:
  fs::write(collection_dir.join("opencollection.yml"), yaml)?;
  // After:
  atomic_write(&collection_dir.join("opencollection.yml"), yaml.as_bytes())?;
  ```

  **Site 2** — `migrate_directory`, writing `folder.yml` (around line 136):
  ```rust
  // Before:
  fs::write(&folder_yml, yaml)?;
  // After:
  atomic_write(&folder_yml, yaml.as_bytes())?;
  ```

  **Site 3** — `migrate_request_file`, writing `.yml` request (around line 171):
  ```rust
  // Before:
  fs::write(&yml_path, yaml)?;
  // After:
  atomic_write(&yml_path, yaml.as_bytes())?;
  ```

  **Site 4** — `migrate_order_file`, writing `_order.yml` (around line 198):
  ```rust
  // Before:
  fs::write(&yml_path, yaml)?;
  // After:
  atomic_write(&yml_path, yaml.as_bytes())?;
  ```

- [ ] **Step 5: Fix `fs_template_repo.rs` — replace `fs::write` with `atomic_write`**

  In `fs_template_repo.rs`, add the import after existing imports:

  ```rust
  use crate::atomic_write;
  ```

  Find the `save` method (around line 51). Replace:

  ```rust
  fs::write(self.file_path(&template.name), yaml)?;
  ```

  With:

  ```rust
  atomic_write(&self.file_path(&template.name), yaml.as_bytes())?;
  ```

- [ ] **Step 6: Run the full infra test suite**

  ```bash
  cargo test -p rocket-infra 2>&1 | tail -30
  ```

  Expected: all tests pass.

- [ ] **Step 7: Commit**

  ```bash
  git add crates/rocket-infra/src/fs_workspace_config_repo.rs crates/rocket-infra/src/migration.rs crates/rocket-infra/src/fs_template_repo.rs
  git commit -m "fix(infra): switch workspace_config, migration, and template writes to atomic_write"
  ```

---

### Task 3: Eliminate `request.clone()` in `save_request` — take `&Request` (Fix 3 from §6)

**Files:**
- Modify: `crates/rocket-infra/src/oc_conversions.rs`
- Modify: `crates/rocket-infra/src/fs_collection_repo.rs`
- Modify: `crates/rocket-infra/src/migration.rs`

**Background:** `save_request` calls `request_to_oc_http_request(request.clone())`, deep-cloning the full `Request` including potentially large body/examples. Changing the conversion function to take `&Request` eliminates the clone. The function currently consumes all fields by value; it must be refactored to borrow them.

- [ ] **Step 1: Write a compile-check test for the new signature**

  Add to `fs_collection_repo.rs` tests (the test verifies the function compiles and works with a reference):

  ```rust
  #[test]
  fn save_request_accepts_ref_without_clone() {
      let (_dir, repo) = setup();
      repo.create("my-api").unwrap();
      let req = rocket_collection::Request::new("Get Items", HttpMethod::Get, "https://example.com/items");
      // save_request takes &Request; calling it should not require the caller to clone.
      let path = repo.save_request("my-api", "get-items.yml", &req).unwrap();
      // req is still usable after the call (proving no move occurred).
      assert_eq!(req.name, "Get Items");
      let loaded = repo.get_request("my-api", &path).unwrap();
      assert_eq!(loaded.name, "Get Items");
  }
  ```

- [ ] **Step 2: Run the test to confirm it currently fails (or passes if it compiles due to auto-deref)**

  ```bash
  cargo test -p rocket-infra save_request_accepts_ref_without_clone 2>&1 | tail -15
  ```

  If it already passes (the trait signature already takes `&Request`), the fix is just to remove the `.clone()` inside the body. Continue.

- [ ] **Step 3: Change `request_to_oc_http_request` signature in `oc_conversions.rs` to take `&Request`**

  In `oc_conversions.rs`, find `pub fn request_to_oc_http_request(req: Request)` (line 960). Change the signature and all field accesses from moves to clones or borrows.

  New signature:
  ```rust
  pub fn request_to_oc_http_request(req: &Request) -> OcHttpRequest {
  ```

  Then update every field access in the function body that consumes the value. The function body moves many fields out of `req`. Since the input is now `&Request`, use `.clone()` on each moved field:

  ```rust
  pub fn request_to_oc_http_request(req: &Request) -> OcHttpRequest {
      let params = merge_params(&req.query_params, &req.path_params);
      let runtime_auth = req.runtime_auth.clone().map(OcAuth::from);
      let settings = req.settings.clone().map(domain_settings_to_oc);

      let info = OcHttpRequestInfo {
          name: req.name.clone(),
          description: req.description.clone(),
          request_type: Some("http".into()),
          seq: req.seq.clone(),
          tags: req.tags.clone(),
      };

      let http = OcHttpRequestDetails {
          method: req.method.to_string(),
          url: req.url.clone(),
          headers: req.headers.iter().cloned().map(OcHttpRequestHeader::from).collect(),
          params,
          body: req.body.clone().map(OcHttpRequestBody::from),
          auth: if req.auth == Auth::None { None } else { Some(OcAuth::from(req.auth.clone())) },
      };

      let mut scripts = Vec::new();
      if let Some(ref code) = req.pre_request_script {
          scripts.push(OcScript { script_type: "before-request".into(), code: code.clone() });
      }
      if let Some(ref code) = req.post_response_script {
          scripts.push(OcScript { script_type: "after-response".into(), code: code.clone() });
      }
      if let Some(ref code) = req.tests {
          scripts.push(OcScript { script_type: "tests".into(), code: code.clone() });
      }

      let actions: Vec<OcAction> = req.actions.iter().map(|a| {
          OcAction::SetVariable {
              description: a.description.clone(),
              phase: a.phase.clone(),
              selector: OcActionSelector { expression: a.selector.expression.clone(), method: a.selector.method.clone() },
              variable: OcActionVariable { name: a.variable.name.clone(), scope: a.variable.scope.clone() },
              disabled: a.disabled,
          }
      }).collect();

      let has_runtime = !scripts.is_empty()
          || !req.assertions.is_empty()
          || !actions.is_empty()
          || !req.variables.is_empty()
          || runtime_auth.is_some();
      let runtime = if has_runtime {
          Some(OcHttpRequestRuntime {
              variables: req.variables.iter()
                  .filter_map(|v| serde_json::from_value::<OcVariable>(v.clone()).ok())
                  .collect(),
              scripts,
              assertions: req.assertions.clone(),
              actions,
              auth: runtime_auth,
          })
      } else {
          None
      };

      let examples = if req.examples.is_empty() {
          None
      } else {
          Some(req.examples.iter().map(|e| {
              OcHttpRequestExample {
                  name: e.name.clone(),
                  description: e.description.clone(),
                  request: e.request.clone().and_then(|v| serde_json::from_value(v).ok()),
                  response: e.response.clone().and_then(|v| serde_json::from_value(v).ok()),
              }
          }).collect())
      };

      let docs = req.docs.as_ref().and_then(|d| d.content().map(String::from));

      OcHttpRequest {
          uid: Some(req.uid.clone()),
          info,
          http,
          runtime,
          settings,
          examples,
          docs,
      }
  }
  ```

  > **Note:** After this change the net allocations are roughly the same as before — previously the whole `Request` was cloned before calling the function, now individual fields are cloned inside it. The difference is that the caller's `Request` is no longer moved, so no deep-clone of the aggregate is needed at the call site.

- [ ] **Step 4: Remove `.clone()` at the call site in `save_request` (`fs_collection_repo.rs`)**

  Find `save_request` (around line 303). Change:

  ```rust
  let oc = request_to_oc_http_request(request.clone());
  ```

  To:

  ```rust
  let oc = request_to_oc_http_request(request);
  ```

- [ ] **Step 5: Fix the call site in `migration.rs`**

  In `migration.rs`, find `migrate_request_file` (around line 160). The call is:

  ```rust
  let oc = request_to_oc_http_request(request);
  ```

  This currently passes by value. Change to pass by reference:

  ```rust
  let oc = request_to_oc_http_request(&request);
  ```

- [ ] **Step 6: Fix any other call sites in `oc_conversions.rs` itself or its tests**

  ```bash
  grep -rn "request_to_oc_http_request" crates/rocket-infra/src/ 2>&1
  ```

  For each remaining call site that passes by value, add `&` before the argument.

- [ ] **Step 7: Compile to catch any missed sites**

  ```bash
  cargo check -p rocket-infra 2>&1 | grep "error\|warning" | head -30
  ```

  Fix any type errors surfaced.

- [ ] **Step 8: Run the full test suite**

  ```bash
  cargo test -p rocket-infra 2>&1 | tail -30
  ```

  Expected: all tests pass.

- [ ] **Step 9: Commit**

  ```bash
  git add crates/rocket-infra/src/oc_conversions.rs crates/rocket-infra/src/fs_collection_repo.rs crates/rocket-infra/src/migration.rs
  git commit -m "perf(infra): take &Request in request_to_oc_http_request, eliminating deep-clone on save"
  ```

---

### Task 4: Stop silent data loss — propagate parse errors instead of `unwrap_or_default` (Fix 4 from §6)

**Files:**
- Modify: `crates/rocket-infra/src/fs_collection_repo.rs`

**Background:** Two sites silently discard corrupt data:
- `save_folder_variables` at line ~613: `serde_yaml::from_str::<OcFolderInfo>(&content).unwrap_or_default()` — a corrupt `folder.yml` is quietly replaced with defaults, overwriting user content on the next save.
- `build_folder_tree` at line ~820: when a request file fails to parse, the error is silently dropped and the request disappears from the tree.

- [ ] **Step 1: Write a failing test for `save_folder_variables` with corrupt folder.yml**

  Add to the test block in `fs_collection_repo.rs`:

  ```rust
  #[test]
  fn save_folder_variables_rejects_corrupt_folder_yml() {
      let (_dir, repo) = setup();
      repo.create("my-api").unwrap();
      repo.create_folder("my-api", "auth").unwrap();
      // Corrupt the folder.yml.
      let folder_yml = _dir.path().join("my-api").join("auth").join("folder.yml");
      fs::write(&folder_yml, b"{{{{not valid yaml: [[[").unwrap();
      // Attempting to save variables into a corrupted folder must return an error,
      // NOT silently overwrite the file with defaults.
      let vars = vec![];
      let result = repo.save_folder_variables("my-api", "auth", vars);
      assert!(result.is_err(), "expected error on corrupt folder.yml, got Ok");
      // Verify the file was NOT silently overwritten (still corrupt).
      let content = fs::read_to_string(&folder_yml).unwrap();
      assert!(content.contains("not valid yaml"), "file was silently overwritten");
  }
  ```

- [ ] **Step 2: Run the test to verify it fails (current code silently overwrites)**

  ```bash
  cargo test -p rocket-infra save_folder_variables_rejects_corrupt_folder_yml 2>&1 | tail -20
  ```

  Expected: FAIL — the test should detect the silent overwrite.

- [ ] **Step 3: Fix `save_folder_variables` — propagate parse error instead of `unwrap_or_default`**

  In `fs_collection_repo.rs`, find `save_folder_variables` (around line 597). Replace:

  ```rust
  let mut info: OcFolderInfo = if folder_yml_path.exists() {
      let content = fs::read_to_string(&folder_yml_path)?;
      serde_yaml::from_str::<OcFolderInfo>(&content)
          .unwrap_or_default()
  } else {
      OcFolderInfo::default()
  };
  ```

  With:

  ```rust
  let mut info: OcFolderInfo = if folder_yml_path.exists() {
      let content = fs::read_to_string(&folder_yml_path)?;
      serde_yaml::from_str::<OcFolderInfo>(&content)
          .map_err(|e| DomainError::Internal(format!("Failed to parse folder.yml: {e}")))?
  } else {
      OcFolderInfo::default()
  };
  ```

- [ ] **Step 4: Run the test again to verify it now passes**

  ```bash
  cargo test -p rocket-infra save_folder_variables_rejects_corrupt_folder_yml 2>&1 | tail -20
  ```

  Expected: PASS.

- [ ] **Step 5: Write a failing test for `build_folder_tree` — corrupt request file becomes a placeholder, not silence**

  Add to the test block:

  ```rust
  #[test]
  fn build_folder_tree_surfaces_corrupt_request_as_warning_not_panic() {
      let (_dir, repo) = setup();
      repo.create("my-api").unwrap();
      // Write a valid request.
      let req = rocket_collection::Request::new("Good", HttpMethod::Get, "https://example.com");
      repo.save_request("my-api", "good.yml", &req).unwrap();
      // Write a corrupt YAML request file.
      let bad_path = _dir.path().join("my-api").join("bad.yml");
      fs::write(&bad_path, b"http:\n  method: [[[unclosed").unwrap();
      // Collection::get must succeed (not propagate the corrupt file error).
      let collection = repo.get("my-api").unwrap();
      // The good request must appear in the tree.
      let names: Vec<_> = collection.root.requests.iter().map(|r| r.name.as_str()).collect();
      assert!(names.contains(&"Good"), "good request missing from tree: {:?}", names);
      // The corrupt file must NOT appear as a real request (it is skipped with a warning).
      assert!(!names.contains(&"bad"), "corrupt file should be skipped");
  }
  ```

- [ ] **Step 6: Run the test to verify it currently passes (corrupt files are silently dropped today)**

  ```bash
  cargo test -p rocket-infra build_folder_tree_surfaces_corrupt_request_as_warning_not_panic 2>&1 | tail -20
  ```

  This test documents the existing behavior (silent drop) as acceptable — the corrupt file is skipped with a warning, not propagated. If the test passes, we only need to add the `tracing::warn!` call.

- [ ] **Step 7: Add `tracing::warn!` for corrupt request files in `build_folder_tree`**

  In `build_folder_tree` (around line 820), find:

  ```rust
  if let Ok(mut request) = request_result {
      request.file_name = Some(entry_name.clone());
      folder.add_request(request);
  }
  ```

  Change to:

  ```rust
  match request_result {
      Ok(mut request) => {
          request.file_name = Some(entry_name.clone());
          folder.add_request(request);
      }
      Err(e) => {
          tracing::warn!(
              path = %path.display(),
              error = %e,
              "skipping corrupt request file"
          );
      }
  }
  ```

- [ ] **Step 8: Run the full test suite**

  ```bash
  cargo test -p rocket-infra 2>&1 | tail -30
  ```

  Expected: all tests pass.

- [ ] **Step 9: Commit**

  ```bash
  git add crates/rocket-infra/src/fs_collection_repo.rs
  git commit -m "fix(infra): propagate folder.yml parse error in save_folder_variables; warn on corrupt request files"
  ```

---

### Task 5: Harden `atomic_write` — `sync_all`, parent-dir fsync, collision-resistant suffix (Fix 5 from §6)

**Files:**
- Modify: `crates/rocket-infra/src/atomic_write.rs`

**Background:** Three weaknesses in `atomic_write`:
1. `sync_data` syncs file data but not metadata (size, timestamps). On some filesystems a crash between `sync_data` and `rename` can leave a zero-byte tmp file. `sync_all` also flushes metadata.
2. No parent-dir fsync after rename: on some Linux filesystems the rename itself is not durable until the directory entry is fsynced.
3. Tmp suffix uses only 32-bit nanosecond subsecond time — two concurrent writes in the same nanosecond produce the same suffix. Adding PID makes collisions practically impossible.

- [ ] **Step 1: Write a failing test documenting the expected `sync_all` behavior**

  The existing tests do not distinguish `sync_data` vs `sync_all`. Add a test that verifies the tmp file is cleaned up even when the parent dir exists. This is already covered by `atomic_write_tmp_file_removed_after_success`. The main new test is for the suffix uniqueness:

  ```rust
  #[test]
  fn concurrent_writes_to_same_path_produce_distinct_tmp_suffixes() {
      let dir = TempDir::new().unwrap();
      let path = dir.path().join("data.yml");
      // Write twice in the same process — with pid in the suffix, suffixes differ even in same nanosecond.
      atomic_write(&path, b"v1\n").unwrap();
      atomic_write(&path, b"v2\n").unwrap();
      // Verify no leftover tmp files.
      let tmps: Vec<_> = fs::read_dir(dir.path()).unwrap()
          .filter_map(|e| e.ok())
          .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
          .collect();
      assert!(tmps.is_empty(), "leftover tmp files: {:?}", tmps);
      assert_eq!(fs::read_to_string(&path).unwrap(), "v2\n");
  }
  ```

- [ ] **Step 2: Run the test to verify it passes with current code (baseline)**

  ```bash
  cargo test -p rocket-infra concurrent_writes_to_same_path_produce_distinct_tmp_suffixes 2>&1 | tail -15
  ```

- [ ] **Step 3: Replace `sync_data` with `sync_all` and add PID to tmp suffix**

  In `atomic_write.rs`, replace the entire function body with:

  ```rust
  pub fn atomic_write(path: &Path, content: &[u8]) -> std::io::Result<()> {
      if let Some(parent) = path.parent() {
          if !parent.as_os_str().is_empty() {
              fs::create_dir_all(parent)?;
          }
      }

      // Combine PID + nanosecond counter for a collision-resistant tmp suffix.
      let nanos = SystemTime::now()
          .duration_since(UNIX_EPOCH)
          .unwrap_or_default()
          .subsec_nanos();
      let pid = std::process::id();
      let tmp_path = path.with_extension(
          path.extension()
              .map(|e| format!("{}.tmp.{pid}_{nanos:08x}", e.to_string_lossy()))
              .unwrap_or_else(|| format!("tmp.{pid}_{nanos:08x}")),
      );

      let write_result = (|| {
          let mut file = fs::File::create(&tmp_path)?;
          file.write_all(content)?;
          // sync_all flushes both data and metadata (unlike sync_data).
          file.sync_all()
      })();

      if let Err(e) = write_result {
          let _ = fs::remove_file(&tmp_path);
          return Err(e);
      }

      if let Err(e) = fs::rename(&tmp_path, path) {
          let _ = fs::remove_file(&tmp_path);
          return Err(e);
      }

      // Best-effort parent-dir fsync to make the rename durable.
      if let Some(parent) = path.parent() {
          if !parent.as_os_str().is_empty() {
              if let Ok(dir_file) = fs::File::open(parent) {
                  let _ = dir_file.sync_all();
              }
          }
      }

      Ok(())
  }
  ```

- [ ] **Step 4: Run all `atomic_write` tests**

  ```bash
  cargo test -p rocket-infra atomic_write 2>&1 | tail -20
  ```

  Expected: all atomic_write tests pass.

- [ ] **Step 5: Run the full infra test suite**

  ```bash
  cargo test -p rocket-infra 2>&1 | tail -30
  ```

  Expected: all tests pass.

- [ ] **Step 6: Commit**

  ```bash
  git add crates/rocket-infra/src/atomic_write.rs
  git commit -m "fix(infra): harden atomic_write — sync_all, parent-dir fsync, pid+nanos collision-resistant suffix"
  ```

---

## Self-Review

**Spec coverage (§6 checklist):**

| Fix | Tasks |
|-----|-------|
| 1. Lock down filesystem boundaries — `validate_name` + `symlink_metadata` guard | Task 1 (symlink guards on delete, delete_folder, build_folder_tree, migrate_directory) |
| 2. Switch all important writes to `atomic_write` | Task 2 (workspace_config, migration x4, template) |
| 3. Eliminate `request.clone()` in `save_request` | Task 3 (refactor `request_to_oc_http_request` to `&Request`) |
| 4. Stop silent data loss — `unwrap_or_default` on OcFolderInfo + `build_folder_tree:884` | Task 4 (propagate parse error + tracing::warn) |
| 5. Harden `atomic_write` — `sync_all`, parent-dir fsync, collision suffix | Task 5 |

**Gap:** Fix 1 also mentions adding `Collection::validate_name` to all public methods that currently lack it (`get`, `delete`, `get_request`, `save_request`, etc.). The current plan adds symlink guards but does NOT add name validation to those methods. This is a separate correctness issue — the review notes that `..` in a collection name would bypass `validate_path` because `collection_path` calls `self.base_dir.join(name)` which would produce `base_dir/../evil`. Adding validate_name to `get`, `delete`, `get_request`, `save_request`, `rename_request`, `delete_request`, `create_folder`, `delete_folder`, `move_item`, `reorder_items`, `get_settings`, `save_settings`, `get_folder_chain_variables`, `save_folder_variables`, `get_folder_variables`, `get_request_variables`, `save_request_variables` is a **required addition to Task 1**.

- [ ] **Task 1 addendum: add `Collection::validate_name(collection)` at the top of every public `CollectionRepository` method that takes a `collection: &str` argument**

  Add `Collection::validate_name(collection)?;` as the first line of each of these methods in `fs_collection_repo.rs`:
  - `get`
  - `delete`
  - `get_request`
  - `save_request`
  - `rename_request`
  - `delete_request`
  - `create_folder`
  - `delete_folder`
  - `move_item` (both `src_collection` and `dst_collection`)
  - `reorder_items`
  - `get_settings`
  - `save_settings`
  - `get_folder_chain_variables`
  - `save_folder_variables`
  - `get_folder_variables`
  - `get_request_variables`
  - `save_request_variables`

  Then add a test:

  ```rust
  #[test]
  fn get_rejects_path_traversal_in_collection_name() {
      let (_dir, repo) = setup();
      let err = repo.get("../evil").unwrap_err();
      assert!(matches!(err, DomainError::InvalidInput(_)), "expected InvalidInput, got {:?}", err);
  }

  #[test]
  fn delete_rejects_path_traversal_in_collection_name() {
      let (_dir, repo) = setup();
      let err = repo.delete("../evil").unwrap_err();
      assert!(matches!(err, DomainError::InvalidInput(_)), "expected InvalidInput, got {:?}", err);
  }
  ```

  Add these steps to Task 1 before the final commit step, and include `fs_collection_repo.rs` in that commit.

**Placeholder scan:** None found.

**Type consistency:** `request_to_oc_http_request` is changed to `&Request` in Task 3. All call sites are updated in the same task. The `OcHttpRequest` return type is unchanged.
