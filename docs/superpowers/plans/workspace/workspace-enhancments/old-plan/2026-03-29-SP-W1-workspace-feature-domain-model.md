# SP-W1: Domain Model + Per-Workspace Config — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend `rocket-workspace` domain crate with `description`, `pinned` fields, `WorkspaceConfig` struct, `CollectionReference` type, `WorkspaceConfigRepository` trait, and new domain events.

**Architecture:** Pure domain changes in `rocket-workspace` and `rocket-shared`. No I/O, no frontend. All new types use `serde` with `rename_all = "camelCase"`.

**Tech Stack:** Rust, serde, serde_yaml, uuid

**Spec:** `docs/superpowers/specs/2026-03-29-workspace-feature-design.md`

---

## Chunk 1: Workspace entity — add `description` field

### Task 1: Add `description: Option<String>` to `Workspace` struct

**Files:**
- Modify: `crates/rocket-workspace/src/workspace.rs`

- [ ] **Step 1: Add the field to the struct**

In `crates/rocket-workspace/src/workspace.rs`, find the `Workspace` struct. Add a new field after `path`:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub description: Option<String>,
```

The full struct should now be:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
```

- [ ] **Step 2: Update `Workspace::new()` to initialize `description`**

Find the `Workspace::new()` method. Add `description: None,` to the struct literal:

```rust
pub fn new(name: &str, path: PathBuf) -> Self {
    Self {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.to_string(),
        path,
        description: None,
    }
}
```

- [ ] **Step 3: Update `WorkspaceRegistry::new_with_default()` to include `description`**

Find the `new_with_default` method. Add `description: None,` to the default workspace literal:

```rust
let default = Workspace {
    id: "default".to_string(),
    name: "Default Workspace".to_string(),
    path: default_path,
    description: None,
};
```

- [ ] **Step 4: Add tests for description field**

Add to the existing `mod tests` block:

```rust
#[test]
fn new_workspace_has_no_description() {
    let ws = Workspace::new("Test", PathBuf::from("/tmp/test"));
    assert_eq!(ws.description, None);
}

#[test]
fn workspace_description_serde_roundtrip() {
    let ws = Workspace {
        id: "test-id".to_string(),
        name: "Test".to_string(),
        path: PathBuf::from("/tmp/test"),
        description: Some("My description".to_string()),
    };
    let yaml = serde_yaml::to_string(&ws).unwrap();
    let back: Workspace = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(back.description, Some("My description".to_string()));
}

#[test]
fn workspace_deserializes_without_description_backward_compat() {
    let yaml = "id: old-ws\nname: Old\npath: /tmp/old\n";
    let ws: Workspace = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(ws.description, None);
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p rocket-workspace`
Expected: ALL tests pass (including existing tests + 3 new ones).

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-workspace/src/workspace.rs
git commit -m "feat(workspace): add description field to Workspace entity"
```

---

## Chunk 2: Workspace entity — add `pinned` field

### Task 2: Add `pinned: bool` to `Workspace` struct

**Files:**
- Modify: `crates/rocket-workspace/src/workspace.rs`

- [ ] **Step 1: Add the field to the struct**

In the `Workspace` struct, add after `description`:

```rust
#[serde(default)]
pub pinned: bool,
```

- [ ] **Step 2: Update `Workspace::new()` to initialize `pinned: false`**

Add `pinned: false,` to the struct literal in `new()`.

- [ ] **Step 3: Update `WorkspaceRegistry::new_with_default()` — default workspace is pinned**

Change the default workspace literal to include `pinned: true,` (the default workspace should be pinned by default).

- [ ] **Step 4: Add tests**

```rust
#[test]
fn new_workspace_is_not_pinned() {
    let ws = Workspace::new("Test", PathBuf::from("/tmp/test"));
    assert!(!ws.pinned);
}

#[test]
fn default_workspace_is_pinned() {
    let reg = WorkspaceRegistry::new_with_default(PathBuf::from("/tmp/default"));
    assert!(reg.workspaces[0].pinned);
}

#[test]
fn workspace_pinned_serde_roundtrip() {
    let mut ws = Workspace::new("Test", PathBuf::from("/tmp/test"));
    ws.pinned = true;
    let yaml = serde_yaml::to_string(&ws).unwrap();
    let back: Workspace = serde_yaml::from_str(&yaml).unwrap();
    assert!(back.pinned);
}

#[test]
fn workspace_deserializes_without_pinned_defaults_false() {
    let yaml = "id: old-ws\nname: Old\npath: /tmp/old\n";
    let ws: Workspace = serde_yaml::from_str(yaml).unwrap();
    assert!(!ws.pinned);
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p rocket-workspace`
Expected: ALL tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-workspace/src/workspace.rs
git commit -m "feat(workspace): add pinned field to Workspace entity"
```

---

## Chunk 3: WorkspaceRegistry — add `multi_workspace_mode`

### Task 3: Add `multi_workspace_mode: bool` to `WorkspaceRegistry`

**Files:**
- Modify: `crates/rocket-workspace/src/workspace.rs`

- [ ] **Step 1: Add the field**

In `WorkspaceRegistry`, add:

```rust
#[serde(default)]
pub multi_workspace_mode: bool,
```

- [ ] **Step 2: Update `new_with_default()` to initialize it**

Add `multi_workspace_mode: false,` to the `Self { ... }` block.

- [ ] **Step 3: Add tests**

```rust
#[test]
fn new_registry_has_multi_workspace_mode_false() {
    let reg = WorkspaceRegistry::new_with_default(PathBuf::from("/tmp/default"));
    assert!(!reg.multi_workspace_mode);
}

#[test]
fn registry_multi_workspace_mode_serde_roundtrip() {
    let mut reg = WorkspaceRegistry::new_with_default(PathBuf::from("/tmp/default"));
    reg.multi_workspace_mode = true;
    let yaml = serde_yaml::to_string(&reg).unwrap();
    let back: WorkspaceRegistry = serde_yaml::from_str(&yaml).unwrap();
    assert!(back.multi_workspace_mode);
}

#[test]
fn registry_deserializes_without_multi_workspace_mode_backward_compat() {
    let yaml = "activeWorkspaceId: default\nworkspaces:\n  - id: default\n    name: Default\n    path: /tmp/d\n";
    let reg: WorkspaceRegistry = serde_yaml::from_str(yaml).unwrap();
    assert!(!reg.multi_workspace_mode);
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p rocket-workspace`
Expected: ALL tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-workspace/src/workspace.rs
git commit -m "feat(workspace): add multi_workspace_mode to WorkspaceRegistry"
```

---

## Chunk 4: WorkspaceConfig struct

### Task 4: Create `CollectionRefType` enum and `CollectionReference` struct

**Files:**
- Create: `crates/rocket-workspace/src/config.rs`

- [ ] **Step 1: Create the file with the enum and struct**

Create `crates/rocket-workspace/src/config.rs` with this exact content:

```rust
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Whether a collection is embedded (inside workspace dir) or external.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CollectionRefType {
    Embedded,
    External,
}

/// A reference to a collection within a workspace.
/// Embedded collections live inside `workspace/collections/`.
/// External collections are referenced by absolute path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CollectionReference {
    pub name: String,
    #[serde(rename = "type")]
    pub ref_type: CollectionRefType,
    /// Only present for external collections.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_ref_serializes_correctly() {
        let r = CollectionReference {
            name: "Users API".to_string(),
            ref_type: CollectionRefType::Embedded,
            path: None,
        };
        let yaml = serde_yaml::to_string(&r).unwrap();
        assert!(yaml.contains("type: embedded"));
        assert!(!yaml.contains("path:"));
    }

    #[test]
    fn external_ref_serializes_with_path() {
        let r = CollectionReference {
            name: "Shared Auth".to_string(),
            ref_type: CollectionRefType::External,
            path: Some(PathBuf::from("/home/user/shared-auth")),
        };
        let yaml = serde_yaml::to_string(&r).unwrap();
        assert!(yaml.contains("type: external"));
        assert!(yaml.contains("/home/user/shared-auth"));
    }

    #[test]
    fn collection_ref_serde_roundtrip() {
        let r = CollectionReference {
            name: "Test".to_string(),
            ref_type: CollectionRefType::External,
            path: Some(PathBuf::from("/tmp/ext")),
        };
        let yaml = serde_yaml::to_string(&r).unwrap();
        let back: CollectionReference = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(r, back);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p rocket-workspace -- config`
Expected: All 3 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-workspace/src/config.rs
git commit -m "feat(workspace): add CollectionRefType and CollectionReference types"
```

---

### Task 5: Add `WorkspaceEnvironmentsConfig` struct to `config.rs`

**Files:**
- Modify: `crates/rocket-workspace/src/config.rs`

- [ ] **Step 1: Add the struct after `CollectionReference`**

Append to `crates/rocket-workspace/src/config.rs`, before the `#[cfg(test)]` block:

```rust
/// Configuration for workspace-level environments.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEnvironmentsConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_environment: Option<String>,
}
```

- [ ] **Step 2: Add test**

Add to the `mod tests` block:

```rust
#[test]
fn environments_config_defaults_to_none() {
    let cfg = WorkspaceEnvironmentsConfig::default();
    assert_eq!(cfg.active_environment, None);
}

#[test]
fn environments_config_serde_roundtrip() {
    let cfg = WorkspaceEnvironmentsConfig {
        active_environment: Some("staging".to_string()),
    };
    let yaml = serde_yaml::to_string(&cfg).unwrap();
    let back: WorkspaceEnvironmentsConfig = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(cfg, back);
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p rocket-workspace -- config`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-workspace/src/config.rs
git commit -m "feat(workspace): add WorkspaceEnvironmentsConfig struct"
```

---

### Task 6: Add `WorkspaceConfig` struct with helper methods

**Files:**
- Modify: `crates/rocket-workspace/src/config.rs`

- [ ] **Step 1: Add the struct after `WorkspaceEnvironmentsConfig`**

Append before the `#[cfg(test)]` block:

```rust
/// Represents the per-workspace `workspace.yml` that lives inside
/// each workspace directory. This file makes the workspace portable
/// and Git-friendly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceConfig {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collections: Vec<CollectionReference>,
    #[serde(default)]
    pub environments: WorkspaceEnvironmentsConfig,
}

impl WorkspaceConfig {
    /// Create a new workspace config with just a name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            collections: Vec::new(),
            environments: WorkspaceEnvironmentsConfig::default(),
        }
    }

    /// Add an embedded collection reference.
    pub fn add_embedded_collection(&mut self, name: impl Into<String>) {
        self.collections.push(CollectionReference {
            name: name.into(),
            ref_type: CollectionRefType::Embedded,
            path: None,
        });
    }

    /// Add an external collection reference.
    pub fn add_external_collection(&mut self, name: impl Into<String>, path: PathBuf) {
        self.collections.push(CollectionReference {
            name: name.into(),
            ref_type: CollectionRefType::External,
            path: Some(path),
        });
    }

    /// Remove a collection reference by name.
    pub fn remove_collection(&mut self, name: &str) {
        self.collections.retain(|c| c.name != name);
    }

    /// Check if a collection name already exists in this config.
    pub fn has_collection(&self, name: &str) -> bool {
        self.collections.iter().any(|c| c.name == name)
    }
}
```

- [ ] **Step 2: Add tests**

Add to `mod tests`:

```rust
#[test]
fn workspace_config_new_is_empty() {
    let cfg = WorkspaceConfig::new("Test");
    assert_eq!(cfg.name, "Test");
    assert!(cfg.collections.is_empty());
    assert_eq!(cfg.description, None);
    assert_eq!(cfg.environments.active_environment, None);
}

#[test]
fn workspace_config_add_embedded_collection() {
    let mut cfg = WorkspaceConfig::new("Test");
    cfg.add_embedded_collection("Users API");
    assert_eq!(cfg.collections.len(), 1);
    assert_eq!(cfg.collections[0].name, "Users API");
    assert_eq!(cfg.collections[0].ref_type, CollectionRefType::Embedded);
    assert_eq!(cfg.collections[0].path, None);
}

#[test]
fn workspace_config_add_external_collection() {
    let mut cfg = WorkspaceConfig::new("Test");
    cfg.add_external_collection("Shared Auth", PathBuf::from("/home/user/shared"));
    assert_eq!(cfg.collections.len(), 1);
    assert_eq!(cfg.collections[0].ref_type, CollectionRefType::External);
    assert_eq!(cfg.collections[0].path, Some(PathBuf::from("/home/user/shared")));
}

#[test]
fn workspace_config_remove_collection() {
    let mut cfg = WorkspaceConfig::new("Test");
    cfg.add_embedded_collection("A");
    cfg.add_embedded_collection("B");
    cfg.remove_collection("A");
    assert_eq!(cfg.collections.len(), 1);
    assert_eq!(cfg.collections[0].name, "B");
}

#[test]
fn workspace_config_has_collection() {
    let mut cfg = WorkspaceConfig::new("Test");
    cfg.add_embedded_collection("Users API");
    assert!(cfg.has_collection("Users API"));
    assert!(!cfg.has_collection("Other"));
}

#[test]
fn workspace_config_full_serde_roundtrip() {
    let mut cfg = WorkspaceConfig::new("My Project");
    cfg.description = Some("Backend APIs".to_string());
    cfg.add_embedded_collection("Users API");
    cfg.add_external_collection("Shared Auth", PathBuf::from("/tmp/shared"));
    cfg.environments.active_environment = Some("staging".to_string());

    let yaml = serde_yaml::to_string(&cfg).unwrap();
    let back: WorkspaceConfig = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(cfg, back);
}

#[test]
fn workspace_config_deserialize_minimal_yaml() {
    let yaml = "name: Minimal\n";
    let cfg: WorkspaceConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(cfg.name, "Minimal");
    assert!(cfg.collections.is_empty());
    assert_eq!(cfg.description, None);
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p rocket-workspace -- config`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-workspace/src/config.rs
git commit -m "feat(workspace): add WorkspaceConfig struct with helper methods"
```

---

## Chunk 5: Repository trait and module registration

### Task 7: Create `WorkspaceConfigRepository` trait

**Files:**
- Create: `crates/rocket-workspace/src/config_repository.rs`

- [ ] **Step 1: Create the file**

Create `crates/rocket-workspace/src/config_repository.rs` with this exact content:

```rust
use std::path::Path;
use rocket_shared::error::DomainResult;
use crate::config::WorkspaceConfig;

/// Repository trait for reading/writing per-workspace `workspace.yml` config.
/// The `workspace_path` parameter is the root directory of the workspace.
pub trait WorkspaceConfigRepository: Send + Sync {
    /// Load the workspace config from `workspace_path/workspace.yml`.
    /// Returns a default config (derived from directory name) if the file does not exist.
    fn load(&self, workspace_path: &Path) -> DomainResult<WorkspaceConfig>;

    /// Save the workspace config to `workspace_path/workspace.yml`.
    fn save(&self, workspace_path: &Path, config: &WorkspaceConfig) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn _assert_object_safe(_: Box<dyn WorkspaceConfigRepository>) {}
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p rocket-workspace -- config_repository`
Expected: 1 test passes.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-workspace/src/config_repository.rs
git commit -m "feat(workspace): add WorkspaceConfigRepository trait"
```

---

### Task 8: Register new modules in `lib.rs` and add re-exports

**Files:**
- Modify: `crates/rocket-workspace/src/lib.rs`

- [ ] **Step 1: Replace the entire `lib.rs` content**

Replace `crates/rocket-workspace/src/lib.rs` with:

```rust
pub mod config;
pub mod config_repository;
pub mod repository;
pub mod workspace;

pub use config::{CollectionRefType, CollectionReference, WorkspaceConfig, WorkspaceEnvironmentsConfig};
pub use config_repository::WorkspaceConfigRepository;
pub use repository::WorkspaceRepository;
pub use workspace::{Workspace, WorkspaceRegistry};
```

- [ ] **Step 2: Run full crate tests**

Run: `cargo test -p rocket-workspace`
Expected: ALL tests pass across all modules.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-workspace/src/lib.rs
git commit -m "feat(workspace): register config and config_repository modules in lib.rs"
```

---

## Chunk 6: Domain events

### Task 9: Add `WorkspacePinned` and `WorkspaceUnpinned` events

**Files:**
- Modify: `crates/rocket-shared/src/events.rs`

- [ ] **Step 1: Add event variants**

In `crates/rocket-shared/src/events.rs`, find the `DomainEvent` enum. Add these variants after `WorkspaceDeleted`:

```rust
WorkspacePinned   { id: String },
WorkspaceUnpinned { id: String },
```

- [ ] **Step 2: Add tests**

Add to the existing `mod tests` block:

```rust
#[test]
fn workspace_pinned_serializes() {
    let event = DomainEvent::WorkspacePinned { id: "ws-123".into() };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("ws-123"));
}

#[test]
fn workspace_unpinned_serializes() {
    let event = DomainEvent::WorkspaceUnpinned { id: "ws-123".into() };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("ws-123"));
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p rocket-shared`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-shared/src/events.rs
git commit -m "feat(events): add WorkspacePinned and WorkspaceUnpinned events"
```

---

### Task 10: Add `WorkspaceDescriptionUpdated` event

**Files:**
- Modify: `crates/rocket-shared/src/events.rs`

- [ ] **Step 1: Add event variant**

Add after `WorkspaceUnpinned`:

```rust
WorkspaceDescriptionUpdated { id: String, description: Option<String> },
```

- [ ] **Step 2: Add test**

```rust
#[test]
fn workspace_description_updated_serializes() {
    let event = DomainEvent::WorkspaceDescriptionUpdated {
        id: "ws-123".into(),
        description: Some("New desc".into()),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("ws-123"));
    assert!(json.contains("New desc"));
}
```

- [ ] **Step 3: Run full workspace build to catch any breakage**

Run: `cargo test --workspace`
Expected: ALL tests pass across all crates. This verifies that adding new enum variants doesn't break exhaustive match patterns elsewhere.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-shared/src/events.rs
git commit -m "feat(events): add WorkspaceDescriptionUpdated event"
```
