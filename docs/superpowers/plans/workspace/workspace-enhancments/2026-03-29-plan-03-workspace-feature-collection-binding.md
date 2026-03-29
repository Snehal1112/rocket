# SP-W3: Collection Binding — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend collection loading to resolve embedded + external collections from workspace config, implement `link_external_collection`, update sidebar to show badges.

**Architecture:** Service method in `rocket-app`, Tauri command, frontend API + sidebar badge.

**Tech Stack:** Rust (serde_yaml, fs), TypeScript (React, Tauri)

**Spec:** `docs/superpowers/specs/2026-03-29-workspace-feature-design.md`

**Depends on:** SP-W2 complete

---

## Chunk 1: CollectionSummary ref_type

### Task 1: Add `ref_type` field to `CollectionSummary`

**Files:**
- Modify: `crates/rocket-collection/src/summary.rs`

- [ ] **Step 1: Examine the current `CollectionSummary` struct**

Open `crates/rocket-collection/src/summary.rs` and read the existing struct definition. Note all existing fields.

- [ ] **Step 2: Add `ref_type` field with default**

Add this field to `CollectionSummary`:

```rust
/// "embedded" (default) or "external". Set by the workspace layer.
#[serde(default = "default_ref_type")]
pub ref_type: String,
```

Add the helper function outside the struct (at module level):

```rust
fn default_ref_type() -> String {
    "embedded".to_string()
}
```

- [ ] **Step 3: Update any constructor or `new()` method**

If `CollectionSummary` has a `new()` method or is constructed directly elsewhere in the crate, add `ref_type: "embedded".to_string()` to the construction. Search the crate for `CollectionSummary {` to find all construction sites.

- [ ] **Step 4: Add backward compatibility test**

Note: The subagent must check which serializer `CollectionSummary` uses in existing code — it may be `serde_json` (for Tauri command responses) or `serde_yaml` (for file persistence). Use the same serializer in the test. If `CollectionSummary` uses `serde_json` for Tauri serialization (which is the default for Tauri commands), use this test:

```rust
#[test]
fn summary_defaults_ref_type_to_embedded() {
    let json = r#"{"name":"test","requestCount":0}"#;
    let s: CollectionSummary = serde_json::from_str(json).unwrap();
    assert_eq!(s.ref_type, "embedded");
}
```

If the crate uses `serde_yaml` for tests, use YAML instead. The subagent should check existing tests in `summary.rs` to match the convention.

- [ ] **Step 5: Run tests**

Run: `cargo test -p rocket-collection`

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-collection/src/summary.rs
git commit -m "feat(collection): add ref_type field to CollectionSummary"
```

---

## Chunk 2: Link external collection

### Task 2: Add `link_external_collection` to `WorkspaceService`

**Files:**
- Modify: `crates/rocket-app/src/workspace_service.rs`

**Prerequisite check:** Verify that `serde_yaml.workspace = true` is in `crates/rocket-app/Cargo.toml` (it should have been added in SP-W2 Task 3). If not, add it before proceeding.

- [ ] **Step 1: Add the method**

```rust
/// Link an external collection directory to a workspace.
/// The directory must contain `opencollection.yml`.
pub fn link_external_collection(&self, workspace_id: &str, collection_path: PathBuf) -> DomainResult<()> {
    let oc_path = collection_path.join("opencollection.yml");
    if !oc_path.exists() {
        return Err(DomainError::NotFound(
            "opencollection.yml not found in the selected directory".into(),
        ));
    }

    let oc_content = fs::read_to_string(&oc_path).map_err(|e| {
        DomainError::Io(format!("Failed to read opencollection.yml: {e}"))
    })?;
    let oc_value: serde_yaml::Value = serde_yaml::from_str(&oc_content).map_err(|e| {
        DomainError::InvalidInput(format!("Failed to parse opencollection.yml: {e}"))
    })?;
    let collection_name = oc_value.get("name")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| {
            collection_path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Untitled")
        })
        .to_string();

    let registry = self.repo.load()?;
    let workspace = registry
        .find_by_id(workspace_id)
        .ok_or_else(|| DomainError::NotFound(workspace_id.into()))?;

    let mut config = self.config_repo.load(&workspace.path)?;

    if config.has_collection(&collection_name) {
        return Err(DomainError::AlreadyExists(collection_name));
    }

    config.add_external_collection(&collection_name, collection_path);
    self.config_repo.save(&workspace.path, &config)?;
    Ok(())
}
```

- [ ] **Step 2: Add tests**

```rust
#[test]
fn link_external_collection_success() {
    let tmp = TempDir::new().unwrap();
    let svc = make_service(&tmp);
    let ext = tmp.path().join("ext-col");
    std::fs::create_dir_all(&ext).unwrap();
    std::fs::write(ext.join("opencollection.yml"), "name: External API\nitems: []\n").unwrap();
    svc.link_external_collection("default", ext).unwrap();
    let cfg = svc.get_workspace_config("default").unwrap();
    assert_eq!(cfg.collections.len(), 1);
    assert_eq!(cfg.collections[0].name, "External API");
}

#[test]
fn link_external_without_opencollection_yml_fails() {
    let tmp = TempDir::new().unwrap();
    let svc = make_service(&tmp);
    let ext = tmp.path().join("no-oc");
    std::fs::create_dir_all(&ext).unwrap();
    assert!(svc.link_external_collection("default", ext).is_err());
}
```

- [ ] **Step 3: Run tests and commit**

Run: `cargo test -p rocket-app -- workspace`

```bash
git add crates/rocket-app/src/workspace_service.rs
git commit -m "feat(app): add link_external_collection to WorkspaceService"
```

---

## Chunk 3: Tauri command for linking

### Task 3: Wire `link_external_collection` Tauri command

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add command function**

```rust
#[tauri::command]
fn link_external_collection(
    workspace_service: tauri::State<'_, Arc<WorkspaceService>>,
    workspace_id: String,
    collection_path: String,
) -> Result<(), String> {
    workspace_service
        .link_external_collection(&workspace_id, PathBuf::from(collection_path))
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 2: Register in `generate_handler!`**

- [ ] **Step 3: Verify compilation and commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(tauri): wire link_external_collection command"
```

---

## Chunk 4: Frontend bindings and sidebar badge

### Task 4: Add `linkExternalCollection` API binding

**Files:**
- Modify: `src/lib/tauri-api.ts`

- [ ] **Step 1: Add the function**

```typescript
export const linkExternalCollection = (workspaceId: string, collectionPath: string) =>
  invoke<void>('link_external_collection', { workspaceId, collectionPath });
```

- [ ] **Step 2: Commit**

```bash
git add src/lib/tauri-api.ts
git commit -m "feat(frontend): add linkExternalCollection API binding"
```

---

### Task 5: Show embedded/external badge in sidebar `CollectionNode`

**Files:**
- Modify: `src/components/collections/CollectionNode.tsx`

The subagent must open `src/components/collections/CollectionNode.tsx` and find where the collection name is rendered. After the name text, add a small badge for external collections.

- [ ] **Step 1: Find the collection name rendering location**

Open the file and look for where `summary.name` or the collection name is displayed. It will be inside a `TreeItemContent` or similar element.

- [ ] **Step 2: Add the badge**

After the collection name `<span>`, add:

```tsx
{summary.refType === 'external' && (
  <span className="ml-auto shrink-0 text-2xs text-muted-foreground bg-muted px-1.5 py-0.5 rounded">
    ext
  </span>
)}
```

Note: `refType` comes from the `CollectionSummary` which has `ref_type` in Rust — serde's `camelCase` rename makes it `refType` in the JSON/TypeScript side. The subagent should verify this by checking the `CollectionSummary` type definition in `tauri-api.ts`.

- [ ] **Step 3: Commit**

```bash
git add src/components/collections/CollectionNode.tsx
git commit -m "feat(frontend): show external badge on linked collections in sidebar"
```
