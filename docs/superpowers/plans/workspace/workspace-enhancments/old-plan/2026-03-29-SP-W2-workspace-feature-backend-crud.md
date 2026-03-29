# SP-W2: Backend CRUD Extensions — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `FsWorkspaceConfigRepo`, extend `WorkspaceService` with pin/unpin/description/open methods, create workspace directory structure on create, wire Tauri commands, update frontend bindings.

**Architecture:** Infrastructure in `rocket-infra`, service methods in `rocket-app`, Tauri commands in `src-tauri`, frontend API in `src/lib/tauri-api.ts` and `src/stores/workspace-store.ts`.

**Tech Stack:** Rust (serde_yaml, fs, dirs), TypeScript (Tauri invoke, Zustand)

**Spec:** `docs/superpowers/specs/2026-03-29-workspace-feature-design.md`

**Depends on:** SP-W1 complete

---

## Chunk 1: FsWorkspaceConfigRepo

### Task 1: Create `FsWorkspaceConfigRepo` — load method

**Files:**
- Create: `crates/rocket-infra/src/fs_workspace_config_repo.rs`

- [ ] **Step 1: Create the file with the struct and `load` implementation**

Create `crates/rocket-infra/src/fs_workspace_config_repo.rs`:

```rust
use std::fs;
use std::path::Path;

use rocket_shared::error::{DomainError, DomainResult};
use rocket_workspace::{WorkspaceConfig, WorkspaceConfigRepository};

/// Filesystem implementation of `WorkspaceConfigRepository`.
/// Reads and writes `workspace.yml` inside each workspace directory.
pub struct FsWorkspaceConfigRepo;

impl FsWorkspaceConfigRepo {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FsWorkspaceConfigRepo {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceConfigRepository for FsWorkspaceConfigRepo {
    fn load(&self, workspace_path: &Path) -> DomainResult<WorkspaceConfig> {
        let config_path = workspace_path.join("workspace.yml");
        if !config_path.exists() {
            let name = workspace_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".into());
            return Ok(WorkspaceConfig::new(name));
        }

        let content = fs::read_to_string(&config_path).map_err(|e| {
            DomainError::Io(format!("Failed to read workspace.yml: {e}"))
        })?;

        serde_yaml::from_str(&content).map_err(|e| {
            DomainError::InvalidInput(format!("Failed to parse workspace.yml: {e}"))
        })
    }

    fn save(&self, workspace_path: &Path, config: &WorkspaceConfig) -> DomainResult<()> {
        fs::create_dir_all(workspace_path).map_err(|e| {
            DomainError::Io(format!("Failed to create workspace directory: {e}"))
        })?;

        let config_path = workspace_path.join("workspace.yml");
        let content = serde_yaml::to_string(config).map_err(|e| {
            DomainError::InvalidInput(format!("Failed to serialize workspace.yml: {e}"))
        })?;

        fs::write(&config_path, content).map_err(|e| {
            DomainError::Io(format!("Failed to write workspace.yml: {e}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_returns_default_when_no_file_exists() {
        let tmp = TempDir::new().unwrap();
        let ws_path = tmp.path().join("my-project");
        fs::create_dir_all(&ws_path).unwrap();
        let repo = FsWorkspaceConfigRepo::new();
        let config = repo.load(&ws_path).unwrap();
        assert_eq!(config.name, "my-project");
        assert!(config.collections.is_empty());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let ws_path = tmp.path().join("test-ws");
        let repo = FsWorkspaceConfigRepo::new();
        let mut config = WorkspaceConfig::new("Test Workspace");
        config.description = Some("A test".to_string());
        config.add_embedded_collection("Users API");
        repo.save(&ws_path, &config).unwrap();
        assert!(ws_path.join("workspace.yml").exists());
        let loaded = repo.load(&ws_path).unwrap();
        assert_eq!(loaded.name, "Test Workspace");
        assert_eq!(loaded.description, Some("A test".to_string()));
        assert_eq!(loaded.collections.len(), 1);
    }

    #[test]
    fn save_creates_directory_if_missing() {
        let tmp = TempDir::new().unwrap();
        let ws_path = tmp.path().join("new-ws");
        let repo = FsWorkspaceConfigRepo::new();
        repo.save(&ws_path, &WorkspaceConfig::new("New")).unwrap();
        assert!(ws_path.join("workspace.yml").exists());
    }

    #[test]
    fn load_invalid_yaml_returns_error() {
        let tmp = TempDir::new().unwrap();
        let ws_path = tmp.path().join("bad-ws");
        fs::create_dir_all(&ws_path).unwrap();
        fs::write(ws_path.join("workspace.yml"), "{{{{invalid").unwrap();
        let repo = FsWorkspaceConfigRepo::new();
        assert!(repo.load(&ws_path).is_err());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p rocket-infra -- fs_workspace_config_repo`
Expected: All 4 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-infra/src/fs_workspace_config_repo.rs
git commit -m "feat(infra): add FsWorkspaceConfigRepo for per-workspace workspace.yml"
```

---

### Task 2: Register `FsWorkspaceConfigRepo` in `rocket-infra/src/lib.rs`

**Files:**
- Modify: `crates/rocket-infra/src/lib.rs`

- [ ] **Step 1: Add the module declaration and re-export**

Add to `crates/rocket-infra/src/lib.rs`:

Module declaration (add alongside existing `pub mod` lines):
```rust
pub mod fs_workspace_config_repo;
```

Re-export (add alongside existing `pub use` lines):
```rust
pub use fs_workspace_config_repo::FsWorkspaceConfigRepo;
```

- [ ] **Step 2: Run full crate tests**

Run: `cargo test -p rocket-infra`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-infra/src/lib.rs
git commit -m "feat(infra): register FsWorkspaceConfigRepo module"
```

---

## Chunk 2: WorkspaceService — add config repo dependency

### Task 3: Add `WorkspaceConfigRepository` dependency to `WorkspaceService`

**Files:**
- Modify: `crates/rocket-app/src/workspace_service.rs`

This is a **breaking change** to `WorkspaceService::new()`. All callers (tests + `src-tauri`) must be updated.

- [ ] **Step 1: Add the import**

At the top of `crates/rocket-app/src/workspace_service.rs`, add:

```rust
use rocket_workspace::{Workspace, WorkspaceRepository, WorkspaceConfig, WorkspaceConfigRepository};
```

(Replace the existing `use rocket_workspace::{Workspace, WorkspaceRepository};` line.)

- [ ] **Step 2: Add the field to the struct**

Update the `WorkspaceService` struct:

```rust
pub struct WorkspaceService {
    repo: Box<dyn WorkspaceRepository>,
    config_repo: Box<dyn WorkspaceConfigRepository>,
    publisher: Box<dyn EventPublisher>,
    active_path: Arc<Mutex<PathBuf>>,
}
```

- [ ] **Step 3: Update the constructor**

```rust
impl WorkspaceService {
    pub fn new(
        repo: Box<dyn WorkspaceRepository>,
        config_repo: Box<dyn WorkspaceConfigRepository>,
        publisher: Box<dyn EventPublisher>,
        active_path: Arc<Mutex<PathBuf>>,
    ) -> Self {
        Self { repo, config_repo, publisher, active_path }
    }
```

- [ ] **Step 4: Add `MockWorkspaceConfigRepo` to the test module and update `make_service`**

In the `#[cfg(test)] mod tests` block, add:

```rust
use rocket_workspace::{WorkspaceConfig, WorkspaceConfigRepository};
use std::path::Path;

struct MockWorkspaceConfigRepo;

impl WorkspaceConfigRepository for MockWorkspaceConfigRepo {
    fn load(&self, workspace_path: &Path) -> DomainResult<WorkspaceConfig> {
        let config_path = workspace_path.join("workspace.yml");
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)
                .map_err(|e| DomainError::Io(e.to_string()))?;
            serde_yaml::from_str(&content)
                .map_err(|e| DomainError::InvalidInput(e.to_string()))
        } else {
            let name = workspace_path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Test".into());
            Ok(WorkspaceConfig::new(name))
        }
    }
    fn save(&self, workspace_path: &Path, config: &WorkspaceConfig) -> DomainResult<()> {
        std::fs::create_dir_all(workspace_path)
            .map_err(|e| DomainError::Io(e.to_string()))?;
        let content = serde_yaml::to_string(config)
            .map_err(|e| DomainError::InvalidInput(e.to_string()))?;
        std::fs::write(workspace_path.join("workspace.yml"), content)
            .map_err(|e| DomainError::Io(e.to_string()))
    }
}
```

Update `make_service`:

```rust
fn make_service(tmp: &TempDir) -> WorkspaceService {
    let default_path = tmp.path().join("default");
    std::fs::create_dir_all(&default_path).unwrap();
    let repo = Box::new(MockWorkspaceRepo::new(default_path.clone()));
    let config_repo = Box::new(MockWorkspaceConfigRepo);
    let active_path = Arc::new(Mutex::new(default_path));
    WorkspaceService::new(repo, config_repo, Box::new(NullEventPublisher), active_path)
}
```

Add `serde_yaml` import to test module:

```rust
use serde_yaml;
```

Add `serde_yaml` to `crates/rocket-app/Cargo.toml` if not already present:

```toml
serde_yaml.workspace = true
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p rocket-app -- workspace`
Expected: All existing tests pass (the only change is constructor signature).

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/workspace_service.rs crates/rocket-app/Cargo.toml
git commit -m "refactor(app): add WorkspaceConfigRepository dependency to WorkspaceService"
```

---

## Chunk 3: WorkspaceService — pin/unpin/description methods

### Task 4: Add `pin` and `unpin` methods

**Files:**
- Modify: `crates/rocket-app/src/workspace_service.rs`

- [ ] **Step 1: Add the methods**

Add to `WorkspaceService` impl block (after existing methods):

```rust
pub fn pin(&self, id: &str) -> DomainResult<()> {
    let mut registry = self.repo.load()?;
    let workspace = registry
        .find_by_id_mut(id)
        .ok_or_else(|| DomainError::NotFound(id.into()))?;
    workspace.pinned = true;
    self.repo.save(&registry)?;
    self.publisher.publish(DomainEvent::WorkspacePinned { id: id.to_string() });
    Ok(())
}

pub fn unpin(&self, id: &str) -> DomainResult<()> {
    let mut registry = self.repo.load()?;
    let workspace = registry
        .find_by_id_mut(id)
        .ok_or_else(|| DomainError::NotFound(id.into()))?;
    workspace.pinned = false;
    self.repo.save(&registry)?;
    self.publisher.publish(DomainEvent::WorkspaceUnpinned { id: id.to_string() });
    Ok(())
}
```

- [ ] **Step 2: Add tests**

```rust
#[test]
fn pin_workspace() {
    let tmp = TempDir::new().unwrap();
    let svc = make_service(&tmp);
    let ws = svc.create("Pinnable", tmp.path().join("pin-ws")).unwrap();
    svc.pin(&ws.id).unwrap();
    let list = svc.list().unwrap();
    let pinned = list.iter().find(|w| w.id == ws.id).unwrap();
    assert!(pinned.pinned);
}

#[test]
fn unpin_workspace() {
    let tmp = TempDir::new().unwrap();
    let svc = make_service(&tmp);
    svc.unpin("default").unwrap();
    let list = svc.list().unwrap();
    let def = list.iter().find(|w| w.id == "default").unwrap();
    assert!(!def.pinned);
}

#[test]
fn pin_nonexistent_fails() {
    let tmp = TempDir::new().unwrap();
    let svc = make_service(&tmp);
    assert!(svc.pin("nonexistent").is_err());
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p rocket-app -- workspace`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-app/src/workspace_service.rs
git commit -m "feat(app): add pin/unpin methods to WorkspaceService"
```

---

### Task 5: Add `update_description` method

**Files:**
- Modify: `crates/rocket-app/src/workspace_service.rs`

- [ ] **Step 1: Add the method**

```rust
pub fn update_description(&self, id: &str, description: Option<&str>) -> DomainResult<()> {
    let mut registry = self.repo.load()?;
    let workspace = registry
        .find_by_id_mut(id)
        .ok_or_else(|| DomainError::NotFound(id.into()))?;
    workspace.description = description.map(|s| s.to_string());
    self.repo.save(&registry)?;
    self.publisher.publish(DomainEvent::WorkspaceDescriptionUpdated {
        id: id.to_string(),
        description: description.map(|s| s.to_string()),
    });
    Ok(())
}
```

- [ ] **Step 2: Add tests**

```rust
#[test]
fn update_description_sets_value() {
    let tmp = TempDir::new().unwrap();
    let svc = make_service(&tmp);
    svc.update_description("default", Some("My desc")).unwrap();
    let ws = svc.get_active().unwrap();
    assert_eq!(ws.description, Some("My desc".to_string()));
}

#[test]
fn update_description_to_none_clears_it() {
    let tmp = TempDir::new().unwrap();
    let svc = make_service(&tmp);
    svc.update_description("default", Some("Initial")).unwrap();
    svc.update_description("default", None).unwrap();
    let ws = svc.get_active().unwrap();
    assert_eq!(ws.description, None);
}

#[test]
fn update_description_nonexistent_fails() {
    let tmp = TempDir::new().unwrap();
    let svc = make_service(&tmp);
    assert!(svc.update_description("nope", Some("x")).is_err());
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p rocket-app -- workspace`

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-app/src/workspace_service.rs
git commit -m "feat(app): add update_description to WorkspaceService"
```

---

## Chunk 4: Create workspace with directory structure

### Task 6: Update `create` to write `workspace.yml` + subdirs

**Files:**
- Modify: `crates/rocket-app/src/workspace_service.rs`

- [ ] **Step 1: Update the `create` method**

Replace the existing `create` method body. After creating the directory, also create `collections/` and `environments/` subdirs and write `workspace.yml`:

Add these lines after `fs::create_dir_all(&path)`:

```rust
// Create subdirectories.
fs::create_dir_all(path.join("collections")).map_err(|e| {
    DomainError::Io(format!("Failed to create collections dir: {e}"))
})?;
fs::create_dir_all(path.join("environments")).map_err(|e| {
    DomainError::Io(format!("Failed to create environments dir: {e}"))
})?;

// Write workspace.yml inside the workspace directory.
let config = WorkspaceConfig::new(name);
self.config_repo.save(&path, &config)?;
```

These lines go BEFORE the `let mut registry = self.repo.load()?;` line.

- [ ] **Step 2: Add test**

```rust
#[test]
fn create_writes_workspace_yml_and_subdirs() {
    let tmp = TempDir::new().unwrap();
    let svc = make_service(&tmp);
    let ws_path = tmp.path().join("structured-ws");
    svc.create("Structured", ws_path.clone()).unwrap();
    assert!(ws_path.join("workspace.yml").exists());
    assert!(ws_path.join("collections").is_dir());
    assert!(ws_path.join("environments").is_dir());
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p rocket-app -- workspace`

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-app/src/workspace_service.rs
git commit -m "feat(app): create workspace.yml and subdirs on workspace create"
```

---

## Chunk 5: Open workspace + get config

### Task 7: Add `get_workspace_config` method

**Files:**
- Modify: `crates/rocket-app/src/workspace_service.rs`

- [ ] **Step 1: Add the method**

```rust
pub fn get_workspace_config(&self, workspace_id: &str) -> DomainResult<WorkspaceConfig> {
    let registry = self.repo.load()?;
    let workspace = registry
        .find_by_id(workspace_id)
        .ok_or_else(|| DomainError::NotFound(workspace_id.into()))?;
    self.config_repo.load(&workspace.path)
}
```

- [ ] **Step 2: Add test**

```rust
#[test]
fn get_workspace_config_returns_config() {
    let tmp = TempDir::new().unwrap();
    let svc = make_service(&tmp);
    let ws = svc.create("Configurable", tmp.path().join("cfg-ws")).unwrap();
    let config = svc.get_workspace_config(&ws.id).unwrap();
    assert_eq!(config.name, "Configurable");
}
```

- [ ] **Step 3: Run tests and commit**

Run: `cargo test -p rocket-app -- workspace`

```bash
git add crates/rocket-app/src/workspace_service.rs
git commit -m "feat(app): add get_workspace_config method"
```

---

### Task 8: Add `open_workspace` method

**Files:**
- Modify: `crates/rocket-app/src/workspace_service.rs`

- [ ] **Step 1: Add the method**

```rust
/// Open an existing workspace from disk. The directory must contain `workspace.yml`.
pub fn open_workspace(&self, path: PathBuf) -> DomainResult<Workspace> {
    if !path.join("workspace.yml").exists() {
        return Err(DomainError::NotFound(
            "workspace.yml not found in the selected directory".into(),
        ));
    }

    let config = self.config_repo.load(&path)?;
    let mut registry = self.repo.load()?;

    if registry.workspaces.iter().any(|w| w.path == path) {
        return Err(DomainError::AlreadyExists("This workspace is already open".into()));
    }

    if registry.name_exists(&config.name, None) {
        return Err(DomainError::AlreadyExists(config.name.clone()));
    }

    let mut workspace = Workspace::new(&config.name, path.clone());
    workspace.description = config.description;

    registry.workspaces.push(workspace.clone());
    self.repo.save(&registry)?;
    self.publisher.publish(DomainEvent::WorkspaceCreated {
        id: workspace.id.clone(),
        name: workspace.name.clone(),
        path: path.to_string_lossy().to_string(),
    });
    Ok(workspace)
}
```

- [ ] **Step 2: Add tests**

```rust
#[test]
fn open_existing_workspace() {
    let tmp = TempDir::new().unwrap();
    let svc = make_service(&tmp);
    let ws_path = tmp.path().join("ext-ws");
    std::fs::create_dir_all(&ws_path).unwrap();
    let cfg = WorkspaceConfig::new("External");
    let yaml = serde_yaml::to_string(&cfg).unwrap();
    std::fs::write(ws_path.join("workspace.yml"), yaml).unwrap();

    let ws = svc.open_workspace(ws_path).unwrap();
    assert_eq!(ws.name, "External");
    assert_eq!(svc.list().unwrap().len(), 2);
}

#[test]
fn open_workspace_without_yml_fails() {
    let tmp = TempDir::new().unwrap();
    let svc = make_service(&tmp);
    let ws_path = tmp.path().join("no-cfg");
    std::fs::create_dir_all(&ws_path).unwrap();
    assert!(svc.open_workspace(ws_path).is_err());
}

#[test]
fn open_workspace_already_registered_fails() {
    let tmp = TempDir::new().unwrap();
    let svc = make_service(&tmp);
    let ws_path = tmp.path().join("dup");
    std::fs::create_dir_all(&ws_path).unwrap();
    let cfg = WorkspaceConfig::new("Dup");
    let yaml = serde_yaml::to_string(&cfg).unwrap();
    std::fs::write(ws_path.join("workspace.yml"), yaml).unwrap();
    svc.open_workspace(ws_path.clone()).unwrap();
    assert!(svc.open_workspace(ws_path).is_err());
}
```

- [ ] **Step 3: Run tests and commit**

Run: `cargo test -p rocket-app -- workspace`

```bash
git add crates/rocket-app/src/workspace_service.rs
git commit -m "feat(app): add open_workspace to register existing workspace from disk"
```

---

## Chunk 6: Multi-workspace mode methods

### Task 9: Add `get_multi_workspace_mode` and `set_multi_workspace_mode`

**Files:**
- Modify: `crates/rocket-app/src/workspace_service.rs`

- [ ] **Step 1: Add methods**

```rust
pub fn get_multi_workspace_mode(&self) -> DomainResult<bool> {
    Ok(self.repo.load()?.multi_workspace_mode)
}

pub fn set_multi_workspace_mode(&self, enabled: bool) -> DomainResult<()> {
    let mut registry = self.repo.load()?;
    registry.multi_workspace_mode = enabled;
    self.repo.save(&registry)?;
    Ok(())
}
```

- [ ] **Step 2: Add test**

```rust
#[test]
fn get_and_set_multi_workspace_mode() {
    let tmp = TempDir::new().unwrap();
    let svc = make_service(&tmp);
    assert!(!svc.get_multi_workspace_mode().unwrap());
    svc.set_multi_workspace_mode(true).unwrap();
    assert!(svc.get_multi_workspace_mode().unwrap());
}
```

- [ ] **Step 3: Run tests and commit**

```bash
git add crates/rocket-app/src/workspace_service.rs
git commit -m "feat(app): add multi_workspace_mode getter/setter"
```

---

## Chunk 7: Tauri commands

### Task 10: Wire `pin_workspace` and `unpin_workspace` Tauri commands

**Files:**
- Modify: `src-tauri/src/lib.rs`

The subagent must find the existing workspace commands (search for `fn list_workspaces`, `fn create_workspace`) and add new commands following the exact same pattern. Also find the `generate_handler![...]` macro invocation and add the new command names.

- [ ] **Step 1: Add command functions**

Add near the existing workspace commands:

```rust
#[tauri::command]
fn pin_workspace(
    workspace_service: tauri::State<'_, Arc<WorkspaceService>>,
    id: String,
) -> Result<(), String> {
    workspace_service.pin(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn unpin_workspace(
    workspace_service: tauri::State<'_, Arc<WorkspaceService>>,
    id: String,
) -> Result<(), String> {
    workspace_service.unpin(&id).map_err(|e| e.to_string())
}
```

- [ ] **Step 2: Register in `generate_handler!`**

Add `pin_workspace, unpin_workspace,` to the handler list.

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p rocket-tauri` (or the tauri crate name — the subagent should find the correct package name from `src-tauri/Cargo.toml`)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(tauri): wire pin_workspace and unpin_workspace commands"
```

---

### Task 11: Wire `update_workspace_description` and `open_workspace` Tauri commands

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add command functions**

```rust
#[tauri::command]
fn update_workspace_description(
    workspace_service: tauri::State<'_, Arc<WorkspaceService>>,
    id: String,
    description: Option<String>,
) -> Result<(), String> {
    workspace_service
        .update_description(&id, description.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn open_workspace(
    workspace_service: tauri::State<'_, Arc<WorkspaceService>>,
    path: String,
) -> Result<Workspace, String> {
    workspace_service
        .open_workspace(PathBuf::from(path))
        .map_err(|e| e.to_string())
}
```

The subagent must ensure `Workspace` and `PathBuf` are imported. Check existing imports at the top of `src-tauri/src/lib.rs`.

- [ ] **Step 2: Register in `generate_handler!`**

Add `update_workspace_description, open_workspace,` to the handler list.

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p rocket-tauri` (or correct package name)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(tauri): wire update_workspace_description and open_workspace commands"
```

---

### Task 12: Wire `get_workspace_config`, `get_multi_workspace_mode`, `set_multi_workspace_mode` Tauri commands

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add command functions**

```rust
#[tauri::command]
fn get_workspace_config(
    workspace_service: tauri::State<'_, Arc<WorkspaceService>>,
    workspace_id: String,
) -> Result<WorkspaceConfig, String> {
    workspace_service
        .get_workspace_config(&workspace_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_multi_workspace_mode(
    workspace_service: tauri::State<'_, Arc<WorkspaceService>>,
) -> Result<bool, String> {
    workspace_service.get_multi_workspace_mode().map_err(|e| e.to_string())
}

#[tauri::command]
fn set_multi_workspace_mode(
    workspace_service: tauri::State<'_, Arc<WorkspaceService>>,
    enabled: bool,
) -> Result<(), String> {
    workspace_service.set_multi_workspace_mode(enabled).map_err(|e| e.to_string())
}
```

Add import: `use rocket_workspace::WorkspaceConfig;`

- [ ] **Step 2: Register in `generate_handler!`**

Add `get_workspace_config, get_multi_workspace_mode, set_multi_workspace_mode,` to the handler list.

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p rocket-tauri` (or correct package name)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(tauri): wire workspace config and multi-workspace mode commands"
```

---

### Task 13: Update `WorkspaceService` construction in Tauri setup

**Files:**
- Modify: `src-tauri/src/lib.rs`

The subagent must find where `WorkspaceService::new(...)` is called in the Tauri setup/run function. The constructor now requires a `config_repo` parameter.

- [ ] **Step 1: Add `FsWorkspaceConfigRepo` to imports**

Add: `use rocket_infra::FsWorkspaceConfigRepo;`

- [ ] **Step 2: Create config repo and pass to WorkspaceService**

Find the `WorkspaceService::new(...)` call. Add `Box::new(FsWorkspaceConfigRepo::new())` as the second argument:

```rust
let workspace_config_repo = Box::new(FsWorkspaceConfigRepo::new());
// Update the existing call — add workspace_config_repo as second arg:
let workspace_service = Arc::new(WorkspaceService::new(
    workspace_repo,
    workspace_config_repo,
    workspace_event_publisher,
    active_workspace_path.clone(),
));
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p rocket-tauri`

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "refactor(tauri): pass FsWorkspaceConfigRepo to WorkspaceService constructor"
```

---

## Chunk 8: Frontend API bindings

### Task 14: Update `Workspace` type and add new API functions in `tauri-api.ts`

**Files:**
- Modify: `src/lib/tauri-api.ts`

- [ ] **Step 1: Update the `Workspace` interface**

Find the existing `Workspace` interface and add the new fields:

```typescript
export interface Workspace {
  id: string;
  name: string;
  path: string;
  description?: string | null;
  pinned: boolean;
}
```

- [ ] **Step 2: Add new workspace types**

Add after the `Workspace` interface:

```typescript
export interface CollectionReference {
  name: string;
  type: 'embedded' | 'external';
  path?: string;
}

export interface WorkspaceEnvironmentsConfig {
  activeEnvironment?: string | null;
}

export interface WorkspaceConfig {
  name: string;
  description?: string | null;
  collections: CollectionReference[];
  environments: WorkspaceEnvironmentsConfig;
}
```

- [ ] **Step 3: Add new API functions**

Add after the existing workspace commands section:

```typescript
export const pinWorkspace = (id: string) =>
  invoke<void>('pin_workspace', { id });

export const unpinWorkspace = (id: string) =>
  invoke<void>('unpin_workspace', { id });

export const updateWorkspaceDescription = (id: string, description: string | null) =>
  invoke<void>('update_workspace_description', { id, description });

export const openWorkspaceFromDisk = (path: string) =>
  invoke<Workspace>('open_workspace', { path });

export const getWorkspaceConfig = (workspaceId: string) =>
  invoke<WorkspaceConfig>('get_workspace_config', { workspaceId });

export const getMultiWorkspaceMode = () =>
  invoke<boolean>('get_multi_workspace_mode');

export const setMultiWorkspaceMode = (enabled: boolean) =>
  invoke<void>('set_multi_workspace_mode', { enabled });
```

- [ ] **Step 4: Commit**

```bash
git add src/lib/tauri-api.ts
git commit -m "feat(frontend): add workspace API types and bindings for new commands"
```

---

### Task 15: Update workspace store with new actions and event listeners

**Files:**
- Modify: `src/stores/workspace-store.ts`

- [ ] **Step 1: Add new imports**

Update the import from `@/lib/tauri-api`:

```typescript
import {
  listWorkspaces,
  getActiveWorkspace,
  createWorkspace as apiCreate,
  switchWorkspace as apiSwitch,
  renameWorkspace as apiRename,
  closeWorkspace as apiClose,
  deleteWorkspace as apiDelete,
  pinWorkspace as apiPin,
  unpinWorkspace as apiUnpin,
  updateWorkspaceDescription as apiUpdateDescription,
  openWorkspaceFromDisk as apiOpenFromDisk,
  getMultiWorkspaceMode,
  setMultiWorkspaceMode as apiSetMultiMode,
  type Workspace,
} from '@/lib/tauri-api'
```

- [ ] **Step 2: Add new fields and actions to the interface**

Add to `WorkspaceState` interface:

```typescript
multiWorkspaceMode: boolean
pinWorkspace: (id: string) => Promise<void>
unpinWorkspace: (id: string) => Promise<void>
updateDescription: (id: string, description: string | null) => Promise<void>
openWorkspaceFromDisk: (path: string) => Promise<void>
setMultiWorkspaceMode: (enabled: boolean) => Promise<void>
```

- [ ] **Step 3: Initialize new state and implement actions**

In the `create` callback, add:

State: `multiWorkspaceMode: false,`

In `loadWorkspaces`, after loading workspaces and active workspace, also load the mode:

```typescript
const mode = await getMultiWorkspaceMode()
set({ workspaces, activeWorkspaceId: active.id, multiWorkspaceMode: mode, initialized: true })
```

Implementations:

```typescript
pinWorkspace: async (id) => { await apiPin(id) },
unpinWorkspace: async (id) => { await apiUnpin(id) },
updateDescription: async (id, description) => { await apiUpdateDescription(id, description) },
openWorkspaceFromDisk: async (path) => { await apiOpenFromDisk(path) },
setMultiWorkspaceMode: async (enabled) => {
  await apiSetMultiMode(enabled)
  set({ multiWorkspaceMode: enabled })
},
```

- [ ] **Step 4: Add event listeners for new events**

In the `subscribeToEvents` function, add:

```typescript
listen<{ id: string }>('workspace-pinned', ({ payload }) => {
  useWorkspaceStore.setState((s) => ({
    workspaces: s.workspaces.map((w) =>
      w.id === payload.id ? { ...w, pinned: true } : w
    ),
  }))
})

listen<{ id: string }>('workspace-unpinned', ({ payload }) => {
  useWorkspaceStore.setState((s) => ({
    workspaces: s.workspaces.map((w) =>
      w.id === payload.id ? { ...w, pinned: false } : w
    ),
  }))
})

listen<{ id: string; description: string | null }>(
  'workspace-description-updated',
  ({ payload }) => {
    useWorkspaceStore.setState((s) => ({
      workspaces: s.workspaces.map((w) =>
        w.id === payload.id ? { ...w, description: payload.description } : w
      ),
    }))
  }
)
```

- [ ] **Step 5: Commit**

```bash
git add src/stores/workspace-store.ts
git commit -m "feat(frontend): add pin, unpin, description, open, multi-mode to workspace store"
```
