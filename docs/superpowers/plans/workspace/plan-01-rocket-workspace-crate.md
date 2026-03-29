# Plan 01 — rocket-workspace crate

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the `rocket-workspace` crate with domain structs, repository trait, and validation logic.

**Architecture:** New crate sits alongside `rocket-collection`, `rocket-environment` etc. Owns `Workspace`, `WorkspaceRegistry` structs and the `WorkspaceRepository` trait. No infrastructure concerns here — pure domain.

**Tech Stack:** Rust, serde, serde_yaml, uuid

**Spec:** `docs/superpowers/specs/2026-03-28-workspace-feature-design.md`

**Next plan:** `plan-02-fs-workspace-repo.md`

---

### Task 1: Scaffold the crate

**Files:**
- Create: `crates/rocket-workspace/Cargo.toml`
- Create: `crates/rocket-workspace/src/lib.rs`
- Modify: `Cargo.toml` (root workspace members)

- [ ] **Step 1: Create `crates/rocket-workspace/Cargo.toml`**

```toml
[package]
name = "rocket-workspace"
version = "0.1.0"
edition = "2021"

[dependencies]
rocket-shared = { path = "../rocket-shared" }
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
uuid = { version = "1", features = ["v4"] }
```

- [ ] **Step 2: Create `crates/rocket-workspace/src/lib.rs`** (empty stubs to compile)

```rust
pub mod repository;
pub mod workspace;

pub use repository::WorkspaceRepository;
pub use workspace::{Workspace, WorkspaceRegistry};
```

- [ ] **Step 3: Add to root `Cargo.toml` workspace members**

Find the `[workspace]` members array and add:
```toml
"crates/rocket-workspace",
```

- [ ] **Step 4: Verify it compiles**

```bash
cargo build -p rocket-workspace
```

Expected: compiles (modules will be empty stubs for now).

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-workspace Cargo.toml
git commit -m "feat(workspace): scaffold rocket-workspace crate"
```

---

### Task 2: Implement workspace.rs

**Files:**
- Create: `crates/rocket-workspace/src/workspace.rs`

- [ ] **Step 1: Create `crates/rocket-workspace/src/workspace.rs`**

```rust
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use rocket_shared::error::{DomainError, DomainResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRegistry {
    pub workspaces: Vec<Workspace>,
    pub active_workspace_id: String,
}

impl Workspace {
    pub fn new(name: &str, path: PathBuf) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            path,
        }
    }

    pub fn validate_name(name: &str) -> DomainResult<()> {
        if name.trim().is_empty() {
            return Err(DomainError::InvalidInput(
                "Workspace name cannot be empty".into(),
            ));
        }
        Ok(())
    }
}

impl WorkspaceRegistry {
    pub fn new_with_default(default_path: PathBuf) -> Self {
        let default = Workspace {
            id: "default".to_string(),
            name: "Default Workspace".to_string(),
            path: default_path,
        };
        Self {
            active_workspace_id: default.id.clone(),
            workspaces: vec![default],
        }
    }

    pub fn find_by_id(&self, id: &str) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.id == id)
    }

    pub fn find_by_id_mut(&mut self, id: &str) -> Option<&mut Workspace> {
        self.workspaces.iter_mut().find(|w| w.id == id)
    }

    pub fn active(&self) -> Option<&Workspace> {
        self.find_by_id(&self.active_workspace_id)
    }

    pub fn name_exists(&self, name: &str, exclude_id: Option<&str>) -> bool {
        self.workspaces.iter().any(|w| {
            w.name.to_lowercase() == name.to_lowercase()
                && exclude_id.map_or(true, |id| w.id != id)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn validate_name_rejects_empty() {
        assert!(Workspace::validate_name("").is_err());
        assert!(Workspace::validate_name("   ").is_err());
    }

    #[test]
    fn validate_name_accepts_valid() {
        assert!(Workspace::validate_name("My Workspace").is_ok());
    }

    #[test]
    fn new_with_default_sets_id() {
        let reg = WorkspaceRegistry::new_with_default(PathBuf::from("/tmp/default"));
        assert_eq!(reg.workspaces.len(), 1);
        assert_eq!(reg.workspaces[0].id, "default");
        assert_eq!(reg.active_workspace_id, "default");
    }

    #[test]
    fn name_exists_case_insensitive() {
        let reg = WorkspaceRegistry::new_with_default(PathBuf::from("/tmp/default"));
        assert!(reg.name_exists("default workspace", None));
        assert!(!reg.name_exists("default workspace", Some("default")));
    }

    #[test]
    fn find_by_id_returns_none_for_missing() {
        let reg = WorkspaceRegistry::new_with_default(PathBuf::from("/tmp/default"));
        assert!(reg.find_by_id("nonexistent").is_none());
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p rocket-workspace
```

Expected: 5 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-workspace/src/workspace.rs
git commit -m "feat(workspace): add Workspace and WorkspaceRegistry domain structs with tests"
```

---

### Task 3: Implement repository.rs

**Files:**
- Create: `crates/rocket-workspace/src/repository.rs`

- [ ] **Step 1: Create `crates/rocket-workspace/src/repository.rs`**

```rust
use rocket_shared::error::DomainResult;
use crate::workspace::WorkspaceRegistry;

pub trait WorkspaceRepository: Send + Sync {
    fn load(&self) -> DomainResult<WorkspaceRegistry>;
    fn save(&self, registry: &WorkspaceRegistry) -> DomainResult<()>;
}
```

- [ ] **Step 2: Verify full crate compiles and tests pass**

```bash
cargo test -p rocket-workspace
```

Expected: all tests still pass, no compile errors.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-workspace/src/repository.rs
git commit -m "feat(workspace): add WorkspaceRepository trait"
```
