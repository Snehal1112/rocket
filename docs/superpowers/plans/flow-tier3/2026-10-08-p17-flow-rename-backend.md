# Flow Rename Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `rename_flow` IPC command that renames a stored flow on disk without ever leaving two flow files, including case-only renames.

**Architecture:** `FlowRepository` (domain trait in `rocket-flow`) gains a `rename` method with a default body, so the three test fakes need no change. `FsFlowRepo` overrides it with a file-level implementation (write the new file atomically, then remove the old one, and roll back if removal fails). `SharedPathFlowRepo` delegates. `FlowService::rename` trims and validates, and a thin Tauri command and a TS wrapper `renameFlow` expose it. The frontend (plan P18) consumes `rename_flow` and `renameFlow`.

**Tech Stack:** Rust (`rocket-flow`, `rocket-infra`, `rocket-app`, `src-tauri`), `tempfile` fixtures, TypeScript wrapper and a Vitest contract test.

**Spec:** Roadmap item F-46 in `.claude/flow-roadmap.md`. Design notes: `docs/superpowers/plans/flow-tier3/01-design-notes.md` section P17.

## Global Constraints

- Before starting any task, read `docs/superpowers/specs/opencollection-spec-reference.md` (this plan touches `rocket-infra` persistence).
- Rust: no unwrap calls in production paths (tests use `expect`). Follow `.claude/rules/rust-ddd-boundaries.md` and `.claude/rules/tauri-ipc-boundaries.md`.
- Domain crate `rocket-flow` stays free of I/O. File logic lives only in `rocket-infra`. The Tauri command stays thin.
- No serde changes. `Flow` is a persistence struct and must not gain `rename_all = "camelCase"`. The command takes plain string arguments, so there is no new DTO.
- Cargo commands always use `-j4` and `-p <crate>`. Never `--workspace` or `--all`.
- Code comments are short full sentences that end with a punctuation mark.
- Commits use conventional commit format and are created through the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only. Never `git add -A`, `--all` or `.`.
- Not in scope: any UI (plan P18), rejecting `::` in flow names (frontend only, see P18), renaming flows during a run (frontend guard in P18), moving flows between collections.

## Review Focus

Failure modes the obvious tests would miss, most likely first:

1. A case-only or punctuation-only rename (`Login Flow` to `login flow`, same slug and same file) must rewrite the file in place. The naive save-then-delete path raises Conflict, or deletes the file it just wrote. Test pinned in Task 1.
2. A rename onto a different flow whose name shares the target slug (`Old` to `my-flow` while `My Flow` exists) must return Conflict and leave both files untouched. Test pinned in Task 1.
3. If removing the old file fails, the new file must be removed and an error returned, so there are never two flows. Test pinned in Task 1.
4. `SharedPathFlowRepo` must override `rename`. If it falls back to the trait default, a case-only rename fails and a crash between save and delete leaves two flows. Test pinned in Task 1 (case-only rename through the shared-path repo).
5. A wrong `old_name` that only shares the slug (`Login flow` while the file holds `Login Flow`) must return NotFound without touching the file, and a target name with an empty slug must leave the old file intact. Tests pinned in Task 1. The service must reject an unchanged or blank name before the repo is called. Tests pinned in Task 2.

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/rocket-flow/src/flow.rs` (modify) | `FlowRepository::rename` with a default body, plus tests on the in-memory fake. |
| `crates/rocket-infra/src/fs_flow_repo.rs` (modify) | `rename_with` (testable core taking a remover) and the `rename` override, plus tests. |
| `crates/rocket-infra/src/shared_path_flow_repo.rs` (modify) | Delegating `rename` override and a test. |
| `crates/rocket-app/src/flow_service.rs` (modify) | `FlowService::rename` and tests. |
| `src-tauri/src/commands/flow.rs` (modify) | `rename_flow` command. |
| `src-tauri/src/lib.rs` (modify) | Register `rename_flow` next to `delete_flow` (near line 681). |
| `src/lib/tauri-api.ts` (modify) | `renameFlow(collection, oldName, newName)` after `deleteFlow` (near line 2178). |
| `src/lib/queries/__tests__/flow-api.test.ts` (modify) | Contract test for the `rename_flow` arguments. |

Existing code to know: `FsFlowRepo::file_path` rejects an empty slug with `InvalidInput`; `get` and `delete` check `flow.name == name` so a flow sharing the slug is "not found"; `save` raises `Conflict` when the file holds a different name; `atomic_write` (in `rocket-infra/src/atomic_write.rs`) writes a temp file and renames it into place.

---

### Task 1: Repository `rename` with rollback

**Model:** implement with opus or review in the main loop (file atomicity invariant).

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-flow/src/flow.rs` (import at line 2, trait at lines 59-65, tests module)
- Modify: `crates/rocket-infra/src/fs_flow_repo.rs` (inherent impl near line 44, trait impl near line 73, tests module end)
- Modify: `crates/rocket-infra/src/shared_path_flow_repo.rs` (trait impl at line 35, tests module)

**Interfaces:**
- Produces: `FlowRepository::rename(&self, collection: &str, old_name: &str, new_name: &str) -> DomainResult<()>`. Errors: `NotFound` (no flow named `old_name`), `Conflict` (another flow already occupies the target), `InvalidInput` (target slug empty), `Io`.
- Produces (private): `FsFlowRepo::rename_with(&self, collection, old_name, new_name, remove: &dyn Fn(&Path) -> std::io::Result<()>) -> DomainResult<()>`.

- [ ] **Step 1: Write the failing trait-default tests**

In `crates/rocket-flow/src/flow.rs`, append these tests at the end of the `mod tests` block (after `flow_with_callback_host_roundtrips`):

```rust
    #[test]
    fn default_rename_moves_the_flow_to_the_new_name() {
        let repo = FakeRepo::new();
        let flow = sample_flow();
        repo.save("my-collection", &flow).expect("save");
        repo.rename("my-collection", &flow.name, "Renamed")
            .expect("rename");
        assert!(repo.get("my-collection", &flow.name).is_err());
        let renamed = repo.get("my-collection", "Renamed").expect("get renamed");
        assert_eq!(renamed.name, "Renamed");
        assert_eq!(renamed.nodes, flow.nodes);
        assert_eq!(repo.list("my-collection").expect("list").len(), 1);
    }

    #[test]
    fn default_rename_onto_an_existing_flow_is_a_conflict() {
        let repo = FakeRepo::new();
        let flow = sample_flow();
        repo.save("my-collection", &flow).expect("save first");
        let mut other = sample_flow();
        other.name = "Other".to_string();
        repo.save("my-collection", &other).expect("save other");
        let err = repo
            .rename("my-collection", &flow.name, "Other")
            .expect_err("target exists");
        assert!(matches!(
            err,
            rocket_shared::error::DomainError::Conflict(_)
        ));
        assert_eq!(repo.list("my-collection").expect("list").len(), 2);
    }

    #[test]
    fn default_rename_of_a_missing_flow_is_not_found() {
        let repo = FakeRepo::new();
        let err = repo
            .rename("my-collection", "nope", "Renamed")
            .expect_err("missing");
        assert!(matches!(
            err,
            rocket_shared::error::DomainError::NotFound(_)
        ));
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-flow default_rename`
Expected: FAIL to compile, `no method named rename found`.

- [ ] **Step 3: Add the trait method with a default body**

In `crates/rocket-flow/src/flow.rs`, change the import on line 2:

```rust
use rocket_shared::error::{DomainError, DomainResult};
```

Replace the trait (lines 59-65) with:

```rust
pub trait FlowRepository: Send + Sync {
    /// Returns the names of all flows in `collection`.
    fn list(&self, collection: &str) -> DomainResult<Vec<String>>;
    fn get(&self, collection: &str, name: &str) -> DomainResult<Flow>;
    fn save(&self, collection: &str, flow: &Flow) -> DomainResult<()>;
    fn delete(&self, collection: &str, name: &str) -> DomainResult<()>;

    /// Renames a flow. The default body (get, check the target, save under the
    /// new name, delete the old one) suits name-keyed stores. File-backed
    /// stores whose keys are derived from the name must override it, because
    /// two names can map to one key.
    fn rename(&self, collection: &str, old_name: &str, new_name: &str) -> DomainResult<()> {
        let mut flow = self.get(collection, old_name)?;
        if self.get(collection, new_name).is_ok() {
            return Err(DomainError::Conflict(format!(
                "Flow '{new_name}' already exists in collection '{collection}'"
            )));
        }
        flow.name = new_name.to_string();
        self.save(collection, &flow)?;
        self.delete(collection, old_name)
    }
}
```

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -j4 -p rocket-flow default_rename`
Expected: PASS (3 tests).

- [ ] **Step 5: Write the failing filesystem tests**

In `crates/rocket-infra/src/fs_flow_repo.rs`, append at the end of `mod tests` (after `wait_for_callback_node_and_callback_host_roundtrip_on_disk`):

```rust
    fn flow_files(dir: &TempDir) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir.path().join("acme").join("flows"))
            .expect("read flows dir")
            .map(|e| {
                e.expect("dir entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn rename_moves_the_flow_to_a_new_file_and_keeps_its_content() {
        let (dir, repo) = setup();
        let original = sample("Login Flow");
        repo.save("acme", &original).expect("save");

        repo.rename("acme", "Login Flow", "Sign In").expect("rename");

        assert_eq!(flow_files(&dir), vec!["sign-in.yml".to_string()]);
        assert!(matches!(
            repo.get("acme", "Login Flow"),
            Err(DomainError::NotFound(_))
        ));
        let loaded = repo.get("acme", "Sign In").expect("get renamed");
        assert_eq!(loaded.name, "Sign In");
        assert_eq!(loaded.nodes, original.nodes);
        assert_eq!(repo.list("acme").expect("list"), vec!["Sign In".to_string()]);
    }

    #[test]
    fn rename_that_only_changes_case_rewrites_the_same_file() {
        let (dir, repo) = setup();
        repo.save("acme", &sample("Login Flow")).expect("save");

        repo.rename("acme", "Login Flow", "login flow")
            .expect("case-only rename");

        assert_eq!(flow_files(&dir), vec!["login-flow.yml".to_string()]);
        assert_eq!(
            repo.list("acme").expect("list"),
            vec!["login flow".to_string()]
        );
        assert!(repo.get("acme", "Login Flow").is_err());
        assert!(repo.get("acme", "login flow").is_ok());
    }

    #[test]
    fn rename_that_only_changes_punctuation_rewrites_the_same_file() {
        let (dir, repo) = setup();
        repo.save("acme", &sample("My Flow")).expect("save");

        repo.rename("acme", "My Flow", "My Flow!").expect("rename");

        assert_eq!(flow_files(&dir), vec!["my-flow.yml".to_string()]);
        assert_eq!(
            repo.list("acme").expect("list"),
            vec!["My Flow!".to_string()]
        );
    }

    #[test]
    fn rename_onto_an_existing_flow_is_a_conflict_and_changes_nothing() {
        let (dir, repo) = setup();
        repo.save("acme", &sample("Login Flow")).expect("save a");
        repo.save("acme", &sample("Other Flow")).expect("save b");

        let err = repo
            .rename("acme", "Login Flow", "Other Flow")
            .expect_err("target exists");
        assert!(matches!(err, DomainError::Conflict(_)));
        assert_eq!(
            flow_files(&dir),
            vec!["login-flow.yml".to_string(), "other-flow.yml".to_string()]
        );
        assert!(repo.get("acme", "Login Flow").is_ok());
        assert!(repo.get("acme", "Other Flow").is_ok());
    }

    #[test]
    fn rename_onto_a_different_name_with_the_same_slug_is_a_conflict() {
        let (dir, repo) = setup();
        repo.save("acme", &sample("My Flow")).expect("save a");
        repo.save("acme", &sample("Old")).expect("save b");

        // "my-flow" slugifies to the file that holds "My Flow".
        let err = repo
            .rename("acme", "Old", "my-flow")
            .expect_err("slug is taken");
        assert!(matches!(err, DomainError::Conflict(_)));
        assert_eq!(
            flow_files(&dir),
            vec!["my-flow.yml".to_string(), "old.yml".to_string()]
        );
        assert_eq!(
            repo.get("acme", "My Flow").expect("get").name,
            "My Flow".to_string()
        );
    }

    #[test]
    fn rename_with_a_wrong_old_name_is_not_found_and_leaves_the_file_alone() {
        let (dir, repo) = setup();
        repo.save("acme", &sample("Login Flow")).expect("save");

        // Same slug, different name, so it is a different flow.
        let err = repo
            .rename("acme", "Login flow", "Sign In")
            .expect_err("wrong old name");
        assert!(matches!(err, DomainError::NotFound(_)));
        let err = repo
            .rename("acme", "No Such Flow", "Sign In")
            .expect_err("missing flow");
        assert!(matches!(err, DomainError::NotFound(_)));
        assert_eq!(flow_files(&dir), vec!["login-flow.yml".to_string()]);
        assert_eq!(
            repo.get("acme", "Login Flow").expect("get").name,
            "Login Flow".to_string()
        );
    }

    #[test]
    fn rename_to_a_name_with_an_empty_slug_is_rejected_and_keeps_the_old_file() {
        let (dir, repo) = setup();
        repo.save("acme", &sample("Login Flow")).expect("save");

        let err = repo
            .rename("acme", "Login Flow", "!!!")
            .expect_err("empty slug");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        assert_eq!(flow_files(&dir), vec!["login-flow.yml".to_string()]);
        assert!(repo.get("acme", "Login Flow").is_ok());
    }

    #[test]
    fn failed_removal_of_the_old_file_rolls_back_so_there_is_never_a_second_flow() {
        let (dir, repo) = setup();
        repo.save("acme", &sample("Login Flow")).expect("save");

        let err = repo
            .rename_with("acme", "Login Flow", "Sign In", &|_: &Path| {
                Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "locked",
                ))
            })
            .expect_err("removal fails");
        assert!(matches!(err, DomainError::Io(_)));

        // Only the original file remains, still under the old name, and no temp file is left.
        assert_eq!(flow_files(&dir), vec!["login-flow.yml".to_string()]);
        assert_eq!(
            repo.list("acme").expect("list"),
            vec!["Login Flow".to_string()]
        );
        assert!(matches!(
            repo.get("acme", "Sign In"),
            Err(DomainError::NotFound(_))
        ));
    }
```

- [ ] **Step 6: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-infra fs_flow_repo::tests::rename`
Expected: FAIL to compile (`rename_with` not found). The default trait method exists after Step 3, but `rename_with` does not.

- [ ] **Step 7: Implement `rename_with` and the override**

In `crates/rocket-infra/src/fs_flow_repo.rs`, add this method to the inherent `impl FsFlowRepo` block, directly after `read_flow` (before the closing brace at line 44):

```rust

    /// The rename algorithm. `remove` deletes the old file and is a parameter
    /// only so a test can make it fail. Order matters: the new file is fully
    /// written before the old one goes, so a crash leaves at least one copy,
    /// and a failed removal deletes the new copy again so there is never a
    /// duplicate.
    fn rename_with(
        &self,
        collection: &str,
        old_name: &str,
        new_name: &str,
        remove: &dyn Fn(&Path) -> std::io::Result<()>,
    ) -> DomainResult<()> {
        let old_path = self.file_path(collection, old_name)?;
        // Rejects a target whose slug is empty before anything is touched.
        let new_path = self.file_path(collection, new_name)?;
        if !old_path.exists() {
            return Err(not_found(collection, old_name));
        }
        let mut flow = Self::read_flow(&old_path)?;
        // A different name that shares the slug is a different flow.
        if flow.name != old_name {
            return Err(not_found(collection, old_name));
        }
        flow.name = new_name.to_string();
        let yaml = serde_yaml::to_string(&flow)
            .map_err(|e| DomainError::Internal(format!("Failed to serialize flow: {e}")))?;

        // Case or punctuation only: both names use one file, so rewrite it in place.
        if old_path == new_path {
            return atomic_write(&new_path, yaml.as_bytes())
                .map_err(|e| DomainError::Io(format!("Failed to write flow file: {e}")));
        }
        if new_path.exists() {
            return Err(DomainError::Conflict(format!(
                "Flow name '{new_name}' collides with an existing flow in collection '{collection}'"
            )));
        }
        atomic_write(&new_path, yaml.as_bytes())
            .map_err(|e| DomainError::Io(format!("Failed to write flow file: {e}")))?;
        if let Err(e) = remove(&old_path) {
            // Best effort: if this also fails there is nothing more to do, and the
            // original error is the one worth reporting.
            let _ = fs::remove_file(&new_path);
            return Err(DomainError::Io(format!(
                "Failed to remove the old flow file: {e}"
            )));
        }
        Ok(())
    }
```

In the same file, add the override inside `impl FlowRepository for FsFlowRepo`, after `delete`:

```rust

    fn rename(&self, collection: &str, old_name: &str, new_name: &str) -> DomainResult<()> {
        self.rename_with(collection, old_name, new_name, &|path: &Path| {
            fs::remove_file(path)
        })
    }
```

- [ ] **Step 8: Run the filesystem tests to verify they pass**

Run: `cargo test -j4 -p rocket-infra fs_flow_repo::tests::rename && cargo test -j4 -p rocket-infra failed_removal_of_the_old_file`
Expected: PASS (8 tests). If `rename_that_only_changes_case_rewrites_the_same_file` fails with Conflict, the in-place branch is not reached: check that `old_path == new_path` compares the slugged paths.

- [ ] **Step 9: Write the failing shared-path test**

In `crates/rocket-infra/src/shared_path_flow_repo.rs`, append at the end of `mod tests`:

```rust

    #[test]
    fn rename_goes_through_the_fs_override_and_follows_the_workspace() {
        let dir = TempDir::new().expect("temp dir");
        std::fs::create_dir_all(dir.path().join("collections").join("acme"))
            .expect("create collection");
        let shared_path = Arc::new(Mutex::new(dir.path().to_path_buf()));
        let repo = SharedPathFlowRepo::new(shared_path);

        repo.save("acme", &sample("Login Flow")).expect("save");
        // A case-only rename fails on the trait default, so this proves the override is used.
        repo.rename("acme", "Login Flow", "login flow")
            .expect("case-only rename");
        repo.rename("acme", "login flow", "Sign In").expect("rename");

        assert_eq!(repo.list("acme").expect("list"), vec!["Sign In".to_string()]);
        let files: Vec<_> = std::fs::read_dir(dir.path().join("collections/acme/flows"))
            .expect("read flows dir")
            .collect();
        assert_eq!(files.len(), 1, "a rename must never leave two flow files");
    }
```

- [ ] **Step 10: Run it to verify it fails**

Run: `cargo test -j4 -p rocket-infra rename_goes_through_the_fs_override`
Expected: FAIL. The delegate does not exist yet, so the trait default runs, and `save` on the same slug returns Conflict at the case-only step.

- [ ] **Step 11: Add the delegating override**

In `crates/rocket-infra/src/shared_path_flow_repo.rs`, add inside `impl FlowRepository for SharedPathFlowRepo`, after `delete`:

```rust

    fn rename(&self, collection: &str, old_name: &str, new_name: &str) -> DomainResult<()> {
        self.repo().rename(collection, old_name, new_name)
    }
```

- [ ] **Step 12: Run the crate tests and check**

Run: `cargo test -j4 -p rocket-infra flow_repo && cargo check -j4 -p rocket-infra`
Expected: PASS. The `flow_repo` filter runs the `fs_flow_repo` and `shared_path_flow_repo` modules.

- [ ] **Step 13: Commit**

Run: `cargo check -j4 -p rocket-flow`. Expected: clean.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`crates/rocket-flow/src/flow.rs crates/rocket-infra/src/fs_flow_repo.rs crates/rocket-infra/src/shared_path_flow_repo.rs`
Suggested subject: `feat(flow): rename flows on disk without leaving a duplicate file`.

---

### Task 2: `FlowService::rename`, the `rename_flow` command and the TS wrapper

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/flow_service.rs` (impl at lines 8-31, tests module)
- Modify: `src-tauri/src/commands/flow.rs` (after `delete_flow`, near line 441)
- Modify: `src-tauri/src/lib.rs` (registration list, near line 681)
- Modify: `src/lib/tauri-api.ts` (after `deleteFlow`, near line 2178)
- Modify: `src/lib/queries/__tests__/flow-api.test.ts` (after the `deleteFlow` test)

**Interfaces:**
- Consumes: `FlowRepository::rename` from Task 1.
- Produces: `FlowService::rename(&self, collection: &str, old_name: &str, new_name: &str) -> DomainResult<()>`. It trims `new_name`, rejects a blank or unchanged name with `InvalidInput`, and passes the trimmed name to the repo.
- Produces: Tauri command `rename_flow(collection: String, old_name: String, new_name: String)`. The frontend calls it with the keys `collection`, `oldName`, `newName` (Tauri maps camelCase keys to the snake_case Rust arguments).
- Produces: `renameFlow(collection: string, oldName: string, newName: string): Promise<void>` exported from `src/lib/tauri-api.ts`. Plan P18 imports this exact name.

- [ ] **Step 1: Write the failing service tests**

In `crates/rocket-app/src/flow_service.rs`, append at the end of `mod tests` (the existing `FakeFlowRepo` uses the trait default `rename`):

```rust

    fn named_flow(name: &str) -> Flow {
        Flow {
            name: name.to_string(),
            ..sample_flow()
        }
    }

    #[test]
    fn rename_moves_the_flow_and_trims_the_new_name() {
        let svc = FlowService::new(Box::new(FakeFlowRepo::seeded("demo", sample_flow())));
        svc.rename("demo", "Login Then Fetch", "  Sign In  ")
            .expect("rename");
        assert_eq!(svc.list("demo").expect("list"), vec!["Sign In".to_string()]);
        assert_eq!(svc.get("demo", "Sign In").expect("get").name, "Sign In");
    }

    #[test]
    fn rename_rejects_a_blank_name_without_touching_the_repo() {
        let svc = FlowService::new(Box::new(FakeFlowRepo::seeded("demo", sample_flow())));
        let err = svc
            .rename("demo", "Login Then Fetch", "   ")
            .expect_err("blank name");
        assert!(matches!(
            err,
            rocket_shared::error::DomainError::InvalidInput(_)
        ));
        assert!(svc.get("demo", "Login Then Fetch").is_ok());
    }

    #[test]
    fn rename_rejects_an_unchanged_name_even_with_surrounding_spaces() {
        let svc = FlowService::new(Box::new(FakeFlowRepo::seeded("demo", sample_flow())));
        let err = svc
            .rename("demo", "Login Then Fetch", " Login Then Fetch ")
            .expect_err("unchanged name");
        assert!(matches!(
            err,
            rocket_shared::error::DomainError::InvalidInput(_)
        ));
        assert!(svc.get("demo", "Login Then Fetch").is_ok());
    }

    #[test]
    fn rename_onto_an_existing_flow_is_a_conflict_and_keeps_both() {
        let repo = FakeFlowRepo::seeded("demo", sample_flow());
        repo.save("demo", &named_flow("Other")).expect("seed other");
        let svc = FlowService::new(Box::new(repo));
        let err = svc
            .rename("demo", "Login Then Fetch", "Other")
            .expect_err("target exists");
        assert!(matches!(
            err,
            rocket_shared::error::DomainError::Conflict(_)
        ));
        assert_eq!(svc.list("demo").expect("list").len(), 2);
    }

    #[test]
    fn rename_of_a_missing_flow_is_not_found() {
        let svc = FlowService::new(Box::new(FakeFlowRepo::new()));
        let err = svc
            .rename("demo", "missing", "Anything")
            .expect_err("missing flow");
        assert!(matches!(
            err,
            rocket_shared::error::DomainError::NotFound(_)
        ));
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-app flow_service::tests::rename`
Expected: FAIL to compile, `no method named rename found for struct FlowService`.

- [ ] **Step 3: Implement the service method**

In `crates/rocket-app/src/flow_service.rs`, add after `delete` (inside `impl FlowService`):

```rust

    /// Renames a flow. The new name is trimmed. A blank or unchanged name is
    /// rejected here, so the repository only sees real renames.
    pub fn rename(&self, collection: &str, old_name: &str, new_name: &str) -> DomainResult<()> {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(DomainError::InvalidInput(
                "Flow name must not be empty".to_string(),
            ));
        }
        if new_name == old_name {
            return Err(DomainError::InvalidInput(
                "The new flow name is the same as the current one".to_string(),
            ));
        }
        self.flow_repo.rename(collection, old_name, new_name)
    }
```

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -j4 -p rocket-app flow_service::tests::rename`
Expected: PASS (5 tests).

- [ ] **Step 5: Add the command and register it**

In `src-tauri/src/commands/flow.rs`, add after `delete_flow`:

```rust

#[tauri::command]
pub fn rename_flow(
    collection: String,
    old_name: String,
    new_name: String,
    svc: State<'_, FlowService>,
) -> Result<(), DomainError> {
    svc.rename(&collection, &old_name, &new_name)
}
```

In `src-tauri/src/lib.rs`, add the line directly after `commands::flow::delete_flow,`:

```rust
            commands::flow::rename_flow,
```

- [ ] **Step 6: Write the failing TS contract test**

In `src/lib/queries/__tests__/flow-api.test.ts`, add after the `deleteFlow` test (before the next `it(`):

```ts
  it('renameFlow invokes rename_flow with collection, oldName and newName', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { renameFlow } = await import('@/lib/tauri-api');
    await renameFlow('my-collection', 'My Flow', 'Sign In');
    expect(invoke).toHaveBeenCalledWith('rename_flow', {
      collection: 'my-collection',
      oldName: 'My Flow',
      newName: 'Sign In',
    });
  });
```

- [ ] **Step 7: Run it to verify it fails**

Run: `yarn test src/lib/queries/__tests__/flow-api.test.ts`
Expected: FAIL, `renameFlow is not a function`.

- [ ] **Step 8: Add the wrapper**

In `src/lib/tauri-api.ts`, add directly after `deleteFlow` (line ~2178):

```ts

export const renameFlow = (collection: string, oldName: string, newName: string) =>
  invoke<void>('rename_flow', { collection, oldName, newName });
```

- [ ] **Step 9: Run it to verify it passes**

Run: `yarn test src/lib/queries/__tests__/flow-api.test.ts`
Expected: PASS.

- [ ] **Step 10: Gates and commit**

Run: `cargo check -j4 -p rocket-app && cargo check -j4 -p rocket && yarn tsc --noEmit && yarn check`
Expected: all pass. (`-p rocket` is the `src-tauri` package; it is the slowest check, run it once.)

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`crates/rocket-app/src/flow_service.rs src-tauri/src/commands/flow.rs src-tauri/src/lib.rs src/lib/tauri-api.ts src/lib/queries/__tests__/flow-api.test.ts`
Suggested subject: `feat(flow): add rename_flow command and renameFlow wrapper`.

---

### Task 3: Final checks

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:** none (verification only). Fix any failure in the file that owns it and re-run; do not change behavior here.

- [ ] **Step 1: Run the focused Rust tests**

Run each, one after the other:
`cargo test -j4 -p rocket-flow default_rename`
`cargo test -j4 -p rocket-infra flow_repo`
`cargo test -j4 -p rocket-app flow_service`
Expected: all PASS. Do not run `cargo test --workspace`.

- [ ] **Step 2: Check the layers wire together**

Run: `cargo check -j4 -p rocket-flow && cargo check -j4 -p rocket-infra && cargo check -j4 -p rocket-app && cargo check -j4 -p rocket`
Expected: no errors and no new warnings. A "never used" warning on `rename_with` means the override was not added.

- [ ] **Step 3: Run the frontend gates**

Run: `yarn tsc --noEmit && yarn check && yarn test src/lib/queries/__tests__/flow-api.test.ts`
Expected: all pass.

- [ ] **Step 4: Confirm the invariants by inspection**

Confirm with `git diff --stat` and a read of `fs_flow_repo.rs`:
- `Flow` and every persistence struct are unchanged (no `rename_all` added).
- `rename_with` has no unwrap or expect calls.
- The only `let _ =` is the rollback `remove_file` call, with its comment.
- `SharedPathFlowRepo` overrides `rename`.

No commit in this task. If Steps 1 to 4 needed a fix, commit it with the `dev-workflow-skills:1-git-commit` skill, staging only the files changed.

---

## Self-Review

- **Spec coverage:** F-46 backend: repo `rename` with rollback and case-only handling (Task 1), service validation and command and wrapper (Task 2), checks (Task 3). Delete already exists (`delete_flow`, `deleteFlow`), no work.
- **Placeholders:** none. Every code step shows code.
- **Type consistency:** the trait signature `rename(&self, &str, &str, &str) -> DomainResult<()>` is identical in the trait, `FsFlowRepo`, `SharedPathFlowRepo` and `FlowService`. The command arguments `collection, old_name, new_name` match the TS keys `collection, oldName, newName`. The TS export is `renameFlow`, which plan P18 imports.
- **Review Focus coverage:** item 1 is `rename_that_only_changes_case_rewrites_the_same_file` and the punctuation twin; item 2 is `rename_onto_a_different_name_with_the_same_slug_is_a_conflict`; item 3 is `failed_removal_of_the_old_file_rolls_back...`; item 4 is `rename_goes_through_the_fs_override_and_follows_the_workspace`; item 5 is the wrong-old-name and empty-slug tests plus the service blank and unchanged tests.

Known follow-ups outside this plan: the existence check and the write in `rename_with` are not one atomic step (a second writer between them could win); this app has one writer per workspace, so it is accepted. Names containing `::` are not rejected here; plan P18 rejects them in the UI until roadmap F-17 lands.
