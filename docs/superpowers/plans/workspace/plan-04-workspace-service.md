# Plan 04 — WorkspaceService

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `WorkspaceService` in `rocket-app` with full CRUD operations, validation, and domain event publishing.

**Architecture:** `WorkspaceService` wraps `WorkspaceRepository` and an `EventPublisher`. It also holds a shared `Arc<Mutex<PathBuf>>` — the active workspace path — which it updates on `switch` and `delete/close` (when the active workspace is affected). All existing services in `AppState` will read from this shared path.

**Tech Stack:** Rust, std::sync (Arc, Mutex), std::fs

**Spec:** `docs/superpowers/specs/2026-03-28-workspace-feature-design.md`

**Previous plan:** `plan-03-domain-events.md`
**Next plan:** `plan-05-tauri-commands.md`

---

### Task 1: Add rocket-workspace dependency to rocket-app

**Files:**
- Modify: `crates/rocket-app/Cargo.toml`

- [ ] **Step 1: Add dependency**

```toml
rocket-workspace = { path = "../rocket-workspace" }
```

- [ ] **Step 2: Verify compile**

```bash
cargo build -p rocket-app
```

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-app/Cargo.toml
git commit -m "chore(app): add rocket-workspace dependency to rocket-app"
```

---

### Task 2: Implement WorkspaceService

**Files:**
- Create: `crates/rocket-app/src/workspace_service.rs`
- Modify: `crates/rocket-app/src/lib.rs`

- [ ] **Step 1: Create `crates/rocket-app/src/workspace_service.rs`**

```rust
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};
use rocket_workspace::{Workspace, WorkspaceRepository};

pub struct WorkspaceService {
    repo: Box<dyn WorkspaceRepository>,
    publisher: Box<dyn EventPublisher>,
    active_path: Arc<Mutex<PathBuf>>,
}

impl WorkspaceService {
    pub fn new(
        repo: Box<dyn WorkspaceRepository>,
        publisher: Box<dyn EventPublisher>,
        active_path: Arc<Mutex<PathBuf>>,
    ) -> Self {
        Self { repo, publisher, active_path }
    }

    pub fn list(&self) -> DomainResult<Vec<Workspace>> {
        Ok(self.repo.load()?.workspaces)
    }

    pub fn get_active(&self) -> DomainResult<Workspace> {
        let registry = self.repo.load()?;
        registry
            .active()
            .cloned()
            .ok_or_else(|| DomainError::NotFound("active workspace".into()))
    }

    pub fn create(&self, name: &str, path: PathBuf) -> DomainResult<Workspace> {
        Workspace::validate_name(name)?;
        if !path.exists() {
            fs::create_dir_all(&path).map_err(|e| {
                DomainError::Io(format!("Failed to create workspace directory: {e}"))
            })?;
        }
        let mut registry = self.repo.load()?;
        if registry.name_exists(name, None) {
            return Err(DomainError::AlreadyExists(name.into()));
        }
        let workspace = Workspace::new(name, path.clone());
        registry.workspaces.push(workspace.clone());
        self.repo.save(&registry)?;
        self.publisher.publish(DomainEvent::WorkspaceCreated {
            id: workspace.id.clone(),
            name: workspace.name.clone(),
            path: path.to_string_lossy().to_string(),
        });
        Ok(workspace)
    }

    pub fn switch(&self, id: &str) -> DomainResult<Workspace> {
        let mut registry = self.repo.load()?;
        let workspace = registry
            .find_by_id(id)
            .cloned()
            .ok_or_else(|| DomainError::NotFound(id.into()))?;
        registry.active_workspace_id = id.to_string();
        self.repo.save(&registry)?;
        *self.active_path.lock().unwrap() = workspace.path.clone();
        self.publisher.publish(DomainEvent::WorkspaceSwitched {
            id: workspace.id.clone(),
            name: workspace.name.clone(),
            path: workspace.path.to_string_lossy().to_string(),
        });
        Ok(workspace)
    }

    pub fn rename(&self, id: &str, new_name: &str) -> DomainResult<()> {
        Workspace::validate_name(new_name)?;
        let mut registry = self.repo.load()?;
        if registry.name_exists(new_name, Some(id)) {
            return Err(DomainError::AlreadyExists(new_name.into()));
        }
        let workspace = registry
            .find_by_id_mut(id)
            .ok_or_else(|| DomainError::NotFound(id.into()))?;
        let old_name = workspace.name.clone();
        workspace.name = new_name.to_string();
        self.repo.save(&registry)?;
        self.publisher.publish(DomainEvent::WorkspaceRenamed {
            id: id.to_string(),
            old_name,
            new_name: new_name.to_string(),
        });
        Ok(())
    }

    pub fn close(&self, id: &str) -> DomainResult<()> {
        let mut registry = self.repo.load()?;
        if registry.workspaces.len() <= 1 {
            return Err(DomainError::InvalidInput(
                "Cannot close the last workspace".into(),
            ));
        }
        registry.workspaces.retain(|w| w.id != id);
        if registry.active_workspace_id == id {
            registry.active_workspace_id = registry.workspaces[0].id.clone();
            *self.active_path.lock().unwrap() = registry.workspaces[0].path.clone();
        }
        self.repo.save(&registry)?;
        self.publisher.publish(DomainEvent::WorkspaceClosed { id: id.to_string() });
        Ok(())
    }

    pub fn delete(&self, id: &str) -> DomainResult<()> {
        if id == "default" {
            return Err(DomainError::InvalidInput(
                "Cannot delete the default workspace".into(),
            ));
        }
        let mut registry = self.repo.load()?;
        if registry.workspaces.len() <= 1 {
            return Err(DomainError::InvalidInput(
                "Cannot delete the last workspace".into(),
            ));
        }
        let workspace = registry
            .find_by_id(id)
            .cloned()
            .ok_or_else(|| DomainError::NotFound(id.into()))?;
        if workspace.path.exists() {
            fs::remove_dir_all(&workspace.path).map_err(|e| {
                DomainError::Io(format!("Failed to delete workspace directory: {e}"))
            })?;
        }
        registry.workspaces.retain(|w| w.id != id);
        if registry.active_workspace_id == id {
            registry.active_workspace_id = registry.workspaces[0].id.clone();
            *self.active_path.lock().unwrap() = registry.workspaces[0].path.clone();
        }
        self.repo.save(&registry)?;
        self.publisher.publish(DomainEvent::WorkspaceDeleted { id: id.to_string() });
        Ok(())
    }
}
```

- [ ] **Step 2: Export from `crates/rocket-app/src/lib.rs`**

Add:
```rust
pub mod workspace_service;
pub use workspace_service::WorkspaceService;
```

- [ ] **Step 3: Verify compile**

```bash
cargo build -p rocket-app
```

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-app/src/workspace_service.rs crates/rocket-app/src/lib.rs
git commit -m "feat(workspace): implement WorkspaceService with CRUD and event publishing"
```

---

### Task 3: Unit tests for WorkspaceService

**Files:**
- Modify: `crates/rocket-app/src/workspace_service.rs` (append tests module)

- [ ] **Step 1: Add `tempfile` to dev-dependencies in `crates/rocket-app/Cargo.toml`**

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Append tests module to `workspace_service.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::error::DomainResult;
    use rocket_shared::events::NullEventPublisher;
    use rocket_workspace::{WorkspaceRegistry};
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    struct MockWorkspaceRepo {
        registry: Mutex<WorkspaceRegistry>,
    }

    impl MockWorkspaceRepo {
        fn new(default_path: PathBuf) -> Self {
            Self {
                registry: Mutex::new(WorkspaceRegistry::new_with_default(default_path)),
            }
        }
    }

    impl WorkspaceRepository for MockWorkspaceRepo {
        fn load(&self) -> DomainResult<WorkspaceRegistry> {
            Ok(self.registry.lock().unwrap().clone())
        }
        fn save(&self, registry: &WorkspaceRegistry) -> DomainResult<()> {
            *self.registry.lock().unwrap() = registry.clone();
            Ok(())
        }
    }

    fn make_service(tmp: &TempDir) -> WorkspaceService {
        let default_path = tmp.path().join("default");
        std::fs::create_dir_all(&default_path).unwrap();
        let repo = Box::new(MockWorkspaceRepo::new(default_path.clone()));
        let active_path = Arc::new(Mutex::new(default_path));
        WorkspaceService::new(repo, Box::new(NullEventPublisher), active_path)
    }

    #[test]
    fn list_returns_default() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        assert_eq!(svc.list().unwrap().len(), 1);
    }

    #[test]
    fn create_adds_workspace() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        let ws = svc.create("New WS", tmp.path().join("new-ws")).unwrap();
        assert_eq!(ws.name, "New WS");
        assert_eq!(svc.list().unwrap().len(), 2);
    }

    #[test]
    fn create_rejects_duplicate_name() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        svc.create("My WS", tmp.path().join("ws1")).unwrap();
        assert!(svc.create("My WS", tmp.path().join("ws2")).is_err());
    }

    #[test]
    fn create_rejects_empty_name() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        assert!(svc.create("", tmp.path().join("ws")).is_err());
    }

    #[test]
    fn switch_updates_active_path() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        let ws = svc.create("Other", tmp.path().join("other")).unwrap();
        svc.switch(&ws.id).unwrap();
        let active = svc.get_active().unwrap();
        assert_eq!(active.id, ws.id);
    }

    #[test]
    fn rename_updates_name() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        let ws = svc.create("Old", tmp.path().join("ws")).unwrap();
        svc.rename(&ws.id, "New").unwrap();
        assert!(svc.list().unwrap().iter().any(|w| w.name == "New"));
    }

    #[test]
    fn rename_rejects_duplicate() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        let ws = svc.create("Alpha", tmp.path().join("ws")).unwrap();
        assert!(svc.rename(&ws.id, "Default Workspace").is_err());
    }

    #[test]
    fn cannot_close_last_workspace() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        assert!(svc.close("default").is_err());
    }

    #[test]
    fn close_switches_active_if_needed() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        let ws = svc.create("Other", tmp.path().join("other")).unwrap();
        svc.switch(&ws.id).unwrap();
        svc.close(&ws.id).unwrap();
        assert_eq!(svc.get_active().unwrap().id, "default");
    }

    #[test]
    fn cannot_delete_default() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        svc.create("Other", tmp.path().join("other")).unwrap();
        assert!(svc.delete("default").is_err());
    }

    #[test]
    fn delete_removes_directory() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        let path = tmp.path().join("to-delete");
        std::fs::create_dir_all(&path).unwrap();
        let ws = svc.create("ToDelete", path.clone()).unwrap();
        svc.delete(&ws.id).unwrap();
        assert!(!path.exists());
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p rocket-app workspace_service
```

Expected: all 10 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-app/src/workspace_service.rs crates/rocket-app/Cargo.toml
git commit -m "test(workspace): add WorkspaceService unit tests"
```
