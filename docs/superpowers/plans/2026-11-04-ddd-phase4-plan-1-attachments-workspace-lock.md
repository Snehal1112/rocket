# DDD Phase 4, Plan 1: Attachments, Workspace Files, Lock Key

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the direct `std::fs` calls from `contract_service.rs`, `workspace_service.rs` and `collection_lock.rs` in `rocket-app` (roadmap items D-40, D-41, D-42).

**Architecture:** Each fs call moves behind a trait owned by a domain crate (`AttachmentStore` in `rocket-collection`, new methods on `WorkspaceRepository` and `CollectionRepository`). `rocket-infra` implements them with the same fs calls as today. Behavior stays the same; the services become testable with a double.

**Tech Stack:** Rust, `rocket-collection`, `rocket-workspace`, `rocket-app`, `rocket-infra`, `src-tauri` wiring.

**Spec:** `docs/roadmaps/ddd-roadmap.md` (Phase 4, D-40 to D-42). Plan 2 (D-43 to D-45) follows.

## Global Constraints

- Every task starts with: read `docs/superpowers/specs/opencollection-spec-reference.md` (the tasks touch collection crates and `rocket-infra`).
- Cargo commands always carry `-j4`. No `cargo test --workspace`. Test one crate at a time: `cargo test -j4 -p <crate> <name>`.
- No panicking unwrap calls in production paths; map errors to `DomainError` or `ContractError`. Serde attributes are not touched.
- Code comments are short full sentences ending in a period.
- Do not commit inside a task. The user reviews the whole plan, then the commit is split by group with the `dev-workflow-skills:1-git-commit` skill.
- Do not add a new `std::fs`, `tokio::fs` or `reqwest` use to `rocket-app/src` outside `#[cfg(test)]`.

## Review Focus

- A source file that does not exist: `AttachmentStore::file_size` returns an error, nothing is stored, no panic (Task 1).
- An upper-case extension such as `.PDF` is still accepted; a file with no extension is refused (Task 1).
- `update_contract` with an empty `kept_paths` removes every stored attachment; with no attachments directory it succeeds (Task 1).
- `delete_workspace` when the directory is already gone still removes the registry entry (Task 2).
- A collection root that does not exist gets a lock keyed by the path as given, with no panic (Task 3).
- Two spellings of the same root (a symlink alias) share one lock (Task 3).

---

### Task 1: Attachment store port (D-40)

**Files:**
- Create: `crates/rocket-collection/src/contract/attachments.rs`
- Modify: `crates/rocket-collection/src/contract/mod.rs` (declare and re-export the module)
- Create: `crates/rocket-infra/src/fs_attachment_store.rs`
- Modify: `crates/rocket-infra/src/lib.rs` (`pub use fs_attachment_store::FsAttachmentStore;`)
- Modify: `crates/rocket-app/src/contract_service.rs` (lines 54-93, 262-271 area, 423-435, and the tests)
- Modify: `src-tauri/src/lib.rs:849` (pass `Arc::new(FsAttachmentStore)`)

**Interfaces:**
- Produces, in `rocket_collection::contract::attachments`:
  ```rust
  pub trait AttachmentStore: Send + Sync {
      /// Size in bytes of a user-chosen source file.
      fn file_size(&self, src: &Path) -> ContractResult<u64>;
      /// Copies `src` into the contract's attachments directory (created on demand)
      /// and returns the stored path relative to `collection_root`.
      fn store(&self, collection_root: &Path, contract_id: Ulid, src: &Path) -> ContractResult<PathBuf>;
      /// Removes every stored file whose relative path is not in `kept`.
      /// A missing directory is not an error; a single failed removal is skipped.
      fn retain_only(&self, collection_root: &Path, contract_id: Ulid, kept: &[PathBuf]) -> ContractResult<()>;
  }
  ```
- Changes: `ContractService::new(repo, collection_repo, attachments: Arc<dyn AttachmentStore>)` and `ContractService::new_with_audit(repo, collection_repo, attachments, audit)`.

- [ ] **Step 1: Read the spec reference.**
- [ ] **Step 2: Write failing tests in `contract_service.rs`.** Add an in-memory `FakeAttachmentStore` (records `store` calls and the `retain_only` argument; `file_size` reads from a map). Tests: `attach_rejects_unsupported_extension_without_storing`, `attach_rejects_file_over_2_mb`, `attach_accepts_uppercase_extension`, `attach_missing_source_errors_and_stores_nothing`, `update_with_empty_kept_paths_retains_nothing`. Each asserts on the fake's recorded calls, not the disk.
- [ ] **Step 3: Run them to see them fail** (they do not compile yet): `cargo test -j4 -p rocket-app contract_service`.
- [ ] **Step 4: Add the trait** in `attachments.rs` and re-export it. Add an object-safety test (`fn _assert(_: Box<dyn AttachmentStore>) {}`) like `repository.rs` does.
- [ ] **Step 5: Implement `FsAttachmentStore`** in `fs_attachment_store.rs`. Reuse `FsContractRepo::attachments_dir(collection_root, id)` for the directory (do not copy the path logic). `store` creates the directory, copies, and returns the path relative to `collection_root`; `retain_only` mirrors the current loop in `update_contract`, including skipping a failed removal. Add tempdir tests: store copies and returns the relative path, `retain_only` removes the unlisted file, missing directory is `Ok`, missing source errors.
- [ ] **Step 6: Switch `contract_service.rs`.** Keep the extension and size validation in the service. Replace the `std::fs` calls with the port. Delete the private `attachments_dir` helper from the service if nothing else uses it. Remove the early directory creation: the directory is now created by `store`, so an invalid first file leaves no empty directory behind. Add the new constructor argument and update the 5 test constructors.
- [ ] **Step 7: Wire `src-tauri/src/lib.rs:849`.**
- [ ] **Step 8: Verify.** Expected: all pass, and `grep -n "std::fs" crates/rocket-app/src/contract_service.rs` shows only `#[cfg(test)]` hits.
  `cargo test -j4 -p rocket-collection attachments`, `cargo test -j4 -p rocket-infra fs_attachment_store`, `cargo test -j4 -p rocket-app contract_service`, `cargo check -j4`.

### Task 2: Workspace file checks and delete (D-41)

**Files:**
- Modify: `crates/rocket-workspace/src/repository.rs`
- Modify: `crates/rocket-infra/src/fs_workspace_repo.rs`
- Modify: `crates/rocket-app/src/workspace_service.rs` (lines 1, 267, 329, 378-383, and the `MockWorkspaceRepo` in the tests)

**Interfaces:**
- Adds to `WorkspaceRepository` (no default bodies):
  ```rust
  /// True when `<path>/workspace.yml` exists.
  fn workspace_config_exists(&self, path: &Path) -> bool;
  /// True when `<path>/opencollection.yml` exists.
  fn collection_manifest_exists(&self, path: &Path) -> bool;
  /// Deletes the workspace directory and everything in it. A missing directory is Ok.
  /// A failure maps to `DomainError::Io("Failed to delete workspace directory: <cause>")`.
  fn remove_workspace_dir(&self, path: &Path) -> DomainResult<()>;
  ```

- [ ] **Step 1: Read the spec reference.**
- [ ] **Step 2: Write failing tests in `workspace_service.rs`.** Extend `MockWorkspaceRepo` to record `remove_workspace_dir` calls. Tests: `open_workspace_without_config_file_is_not_found`, `link_external_collection_without_manifest_is_not_found`, `delete_workspace_calls_the_repo_to_remove_the_directory`, `delete_workspace_with_missing_directory_still_updates_registry`.
- [ ] **Step 3: Run to see them fail:** `cargo test -j4 -p rocket-app workspace_service`.
- [ ] **Step 4: Add the three methods** to the trait.
- [ ] **Step 5: Implement them in `FsWorkspaceRepo`** with `Path::exists` and `std::fs::remove_dir_all` (return `Ok` when the directory does not exist). Add tempdir tests next to the existing ones.
- [ ] **Step 6: Switch `workspace_service.rs`** to the three methods. Remove `use std::fs;` if it is now unused. The mock's own `std::fs` calls stay (test code).
- [ ] **Step 7: Verify.** `cargo test -j4 -p rocket-infra fs_workspace_repo`, `cargo test -j4 -p rocket-app workspace_service`, `cargo check -j4`. Expected: pass, and no non-test `std::fs` in `workspace_service.rs`.

### Task 3: Lock key through the repository (D-42) and docs

**Files:**
- Modify: `crates/rocket-collection/src/repository.rs` (`CollectionRepository`)
- Modify: `crates/rocket-infra/src/fs_collection/mod.rs` and `crates/rocket-infra/src/shared_path_collection_repo.rs`
- Modify: `crates/rocket-app/src/collection_lock.rs:83-85`
- Modify callers: `collection_service.rs:76`, `openapi_sync_changes.rs:160`, `openapi_connect.rs:206,290`, `openapi_sync_delete.rs:65`, `openapi_sync_lock_tests.rs:67`
- Modify docs: `crates/rocket-app/CLAUDE.md` (drop D-40 to D-42 from the exceptions paragraph), `crates/rocket-infra/CLAUDE.md` (add `FsAttachmentStore` to the types table), `docs/roadmaps/ddd-roadmap.md` (D-40 to D-42 status `done (2026-11-04)`)

**Interfaces:**
- Adds to `CollectionRepository`:
  ```rust
  /// The path used as the identity of a collection lock. The default returns `path`
  /// as given; filesystem repositories resolve symlinks and fall back to `path` when that fails.
  fn lock_key(&self, path: &Path) -> PathBuf { path.to_path_buf() }
  ```
- Changes: `pub(crate) fn collection_lock(repo: &dyn CollectionRepository, root: &Path) -> Arc<CollectionLock>`.

- [ ] **Step 1: Read the spec reference.**
- [ ] **Step 2: Write failing tests.** In `collection_lock.rs` tests: `two_spellings_of_one_root_share_a_lock` (a small repo double whose `lock_key` maps an alias path to the real one; assert `Arc::ptr_eq`) and `unresolvable_root_keys_by_the_given_path` (default `lock_key`, assert two calls on the same path return the same `Arc`). In `rocket-infra`: `lock_key_resolves_a_symlink` (`#[cfg(unix)]`, tempdir plus `std::os::unix::fs::symlink`) and `lock_key_of_missing_path_is_the_path`.
- [ ] **Step 3: Run to see them fail:** `cargo test -j4 -p rocket-app collection_lock`.
- [ ] **Step 4: Add `lock_key`** to the trait with the default body above.
- [ ] **Step 5: Override it** in `FsCollectionRepo` and `SharedPathCollectionRepo` with `std::fs::canonicalize(path)`, falling back to `path.to_path_buf()` on error.
- [ ] **Step 6: Change `collection_lock`** to take the repo and use `repo.lock_key(root)`. Update `with_collection_lock` and the callers listed above. Where a `pinned` repo is in scope (the `openapi_*` files), pass that same `repo` variable.
- [ ] **Step 7: Update the three docs** listed under Files.
- [ ] **Step 8: Verify.** `cargo test -j4 -p rocket-infra`, `cargo test -j4 -p rocket-app collection_lock`, `cargo test -j4 -p rocket-app openapi_sync`, `cargo check -j4`, `cargo clippy -j4 -p rocket-app -p rocket-infra -p rocket-collection -p rocket-workspace`. Then `grep -n "std::fs\|tokio::fs" crates/rocket-app/src/{contract_service,workspace_service,collection_lock}.rs` shows test-only hits.
- [ ] **Step 9: Stop.** Report to the user for review. Do not commit.
