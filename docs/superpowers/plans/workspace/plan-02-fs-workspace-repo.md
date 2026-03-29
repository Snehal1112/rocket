# Plan 02 — FsWorkspaceRepo

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `FsWorkspaceRepo` in `rocket-infra` — reads/writes `workspaces.yml` via `serde_yaml`. Bootstraps the default workspace on first launch.

**Architecture:** `FsWorkspaceRepo` implements `WorkspaceRepository` from `rocket-workspace`. On `load`, if `workspaces.yml` does not exist it creates a default workspace dir and saves a fresh registry. File lives at `{app_data_dir}/workspaces.yml`.

**Tech Stack:** Rust, serde_yaml (already a dep in rocket-infra), std::fs

**Spec:** `docs/superpowers/specs/2026-03-28-workspace-feature-design.md`

**Previous plan:** `plan-01-rocket-workspace-crate.md`
**Next plan:** `plan-03-domain-events.md`

---

### Task 1: Add rocket-workspace dependency to rocket-infra

**Files:**
- Modify: `crates/rocket-infra/Cargo.toml`

- [ ] **Step 1: Add dependency**

In `[dependencies]`:
```toml
rocket-workspace = { path = "../rocket-workspace" }
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo build -p rocket-infra
```

Expected: compiles with no errors.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-infra/Cargo.toml
git commit -m "chore(infra): add rocket-workspace dependency"
```

---

### Task 2: Implement FsWorkspaceRepo

**Files:**
- Create: `crates/rocket-infra/src/fs_workspace_repo.rs`
- Modify: `crates/rocket-infra/src/lib.rs`

- [ ] **Step 1: Create `crates/rocket-infra/src/fs_workspace_repo.rs`**

```rust
use std::fs;
use std::path::PathBuf;

use rocket_shared::error::{DomainError, DomainResult};
use rocket_workspace::{WorkspaceRegistry, WorkspaceRepository};

pub struct FsWorkspaceRepo {
    registry_path: PathBuf,
    default_workspace_path: PathBuf,
}

impl FsWorkspaceRepo {
    pub fn new(app_data_dir: PathBuf) -> Self {
        Self {
            registry_path: app_data_dir.join("workspaces.yml"),
            default_workspace_path: app_data_dir.join("Default Workspace"),
        }
    }
}

impl WorkspaceRepository for FsWorkspaceRepo {
    fn load(&self) -> DomainResult<WorkspaceRegistry> {
        if !self.registry_path.exists() {
            fs::create_dir_all(&self.default_workspace_path).map_err(|e| {
                DomainError::Io(format!("Failed to create default workspace dir: {e}"))
            })?;
            let registry =
                WorkspaceRegistry::new_with_default(self.default_workspace_path.clone());
            self.save(&registry)?;
            return Ok(registry);
        }

        let content = fs::read_to_string(&self.registry_path).map_err(|e| {
            DomainError::Io(format!("Failed to read workspaces.yml: {e}"))
        })?;

        serde_yaml::from_str(&content).map_err(|e| {
            DomainError::InvalidInput(format!("Failed to parse workspaces.yml: {e}"))
        })
    }

    fn save(&self, registry: &WorkspaceRegistry) -> DomainResult<()> {
        if let Some(parent) = self.registry_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                DomainError::Io(format!("Failed to create app data dir: {e}"))
            })?;
        }

        let content = serde_yaml::to_string(registry).map_err(|e| {
            DomainError::InvalidInput(format!("Failed to serialize workspaces.yml: {e}"))
        })?;

        fs::write(&self.registry_path, content).map_err(|e| {
            DomainError::Io(format!("Failed to write workspaces.yml: {e}"))
        })
    }
}
```

- [ ] **Step 2: Export from `crates/rocket-infra/src/lib.rs`**

Add:
```rust
pub mod fs_workspace_repo;
pub use fs_workspace_repo::FsWorkspaceRepo;
```

- [ ] **Step 3: Verify it compiles**

```bash
cargo build -p rocket-infra
```

Expected: compiles with no errors.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-infra/src/fs_workspace_repo.rs crates/rocket-infra/src/lib.rs
git commit -m "feat(workspace): add FsWorkspaceRepo — reads/writes workspaces.yml"
```

---

### Task 3: Integration test for FsWorkspaceRepo

**Files:**
- Modify: `crates/rocket-infra/src/fs_workspace_repo.rs` (add tests module)

- [ ] **Step 1: Add tests to `fs_workspace_repo.rs`**

Append to the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_repo(tmp: &TempDir) -> FsWorkspaceRepo {
        FsWorkspaceRepo::new(tmp.path().to_path_buf())
    }

    #[test]
    fn first_load_creates_default_workspace() {
        let tmp = TempDir::new().unwrap();
        let repo = make_repo(&tmp);
        let registry = repo.load().unwrap();
        assert_eq!(registry.workspaces.len(), 1);
        assert_eq!(registry.workspaces[0].id, "default");
        assert_eq!(registry.workspaces[0].name, "Default Workspace");
        assert!(tmp.path().join("workspaces.yml").exists());
        assert!(tmp.path().join("Default Workspace").exists());
    }

    #[test]
    fn save_and_reload_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let repo = make_repo(&tmp);
        let mut registry = repo.load().unwrap();
        registry.workspaces.push(rocket_workspace::Workspace::new(
            "My API",
            tmp.path().join("my-api"),
        ));
        repo.save(&registry).unwrap();

        let reloaded = repo.load().unwrap();
        assert_eq!(reloaded.workspaces.len(), 2);
        assert_eq!(reloaded.workspaces[1].name, "My API");
    }

    #[test]
    fn subsequent_loads_read_existing_file() {
        let tmp = TempDir::new().unwrap();
        let repo = make_repo(&tmp);
        repo.load().unwrap(); // creates the file
        // Second load must read the file, not recreate it.
        let registry = repo.load().unwrap();
        assert_eq!(registry.workspaces.len(), 1);
    }
}
```

- [ ] **Step 2: Add `tempfile` to dev-dependencies in `crates/rocket-infra/Cargo.toml`** (if not already present)

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p rocket-infra fs_workspace_repo
```

Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-infra/src/fs_workspace_repo.rs crates/rocket-infra/Cargo.toml
git commit -m "test(workspace): add FsWorkspaceRepo integration tests"
```
