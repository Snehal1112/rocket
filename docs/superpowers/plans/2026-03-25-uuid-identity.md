# UUID Identity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add stable UUID v4 identifiers to collections, folders, and requests so tab matching survives renames and moves.

**Architecture:** UUIDs are stored alongside each entity — inside `.json` files for requests, in `.uid` hidden files for folders and collections. `build_folder_tree` and `list` handle migration (generate + persist UIDs for existing data). Frontend uses UIDs as tab IDs instead of filename-based paths.

**Tech Stack:** Rust (uuid crate), React, TypeScript, Zustand

**Spec:** `docs/superpowers/specs/2026-03-25-uuid-identity-design.md`

---

### File Structure

```
Rust:
  crates/rocket-collection/Cargo.toml           # add uuid dep
  crates/rocket-collection/src/request.rs        # add uid field
  crates/rocket-collection/src/folder.rs         # add uid field
  crates/rocket-collection/src/summary.rs        # add uid field
  crates/rocket-infra/Cargo.toml                 # add uuid dep
  crates/rocket-infra/src/fs_collection_repo.rs  # read/write .uid files, migration
  src-tauri/src/commands/collections.rs          # save_request returns Request

Frontend:
  src/lib/tauri-api.ts                           # add uid to types
  src/lib/pane-utils.ts                          # remove findTabBySource
  src/stores/pane-store.ts                       # tab id = uid
  src/components/layout/CollectionsSidebar.tsx    # use uid for tab matching
  src/components/collections/SaveToCollectionDialog.tsx  # use uid
  src/components/request/SaveRequestButton.tsx    # use uid
```

---

### Task 1: Add uuid dependency and uid field to Rust structs

**Files:**
- Modify: `crates/rocket-collection/Cargo.toml`
- Modify: `crates/rocket-collection/src/request.rs`
- Modify: `crates/rocket-collection/src/folder.rs`
- Modify: `crates/rocket-collection/src/summary.rs`

- [ ] **Step 1: Add uuid dependency to rocket-collection**

In `crates/rocket-collection/Cargo.toml`, add under `[dependencies]`:
```toml
uuid = { version = "1", features = ["v4"] }
```

- [ ] **Step 2: Add uid to Request**

In `crates/rocket-collection/src/request.rs`, add a `generate_uid` function and `uid` field:

```rust
fn generate_uid() -> String {
    uuid::Uuid::new_v4().to_string()
}
```

Add `uid` as the first field of `Request`:
```rust
#[serde(default = "generate_uid")]
pub uid: String,
```

Update `Request::new()` to set `uid: generate_uid()`.

- [ ] **Step 3: Add uid to Folder**

In `crates/rocket-collection/src/folder.rs`, add same `generate_uid` function. Add `uid` field:
```rust
#[serde(default = "generate_uid")]
pub uid: String,
```

Update `Folder::new()` to set `uid: generate_uid()`.

- [ ] **Step 4: Add uid to CollectionSummary**

In `crates/rocket-collection/src/summary.rs`, add `uid: String` field. Update `CollectionSummary::new()` to accept and store uid.

- [ ] **Step 5: Fix all compilation errors**

Run `cargo check --workspace` and fix any struct initialization that's now missing the `uid` field (tests, mock repos, etc.).

- [ ] **Step 6: Run tests**

Run: `cargo test --workspace`
Expected: All pass.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-collection/
git commit -m "feat: add uid field to Request, Folder, and CollectionSummary"
```

---

### Task 2: Read/write .uid files for folders and collections

**Files:**
- Modify: `crates/rocket-infra/Cargo.toml`
- Modify: `crates/rocket-infra/src/fs_collection_repo.rs`

- [ ] **Step 1: Add uuid dependency to rocket-infra**

In `crates/rocket-infra/Cargo.toml`, add:
```toml
uuid = { version = "1", features = ["v4"] }
```

- [ ] **Step 2: Add helper to read or create .uid file**

In `fs_collection_repo.rs`, add a helper function:

```rust
/// Reads the .uid file from a directory. If missing, generates a UUID and writes it.
fn read_or_create_uid(dir: &Path) -> String {
    let uid_path = dir.join(".uid");
    if let Ok(uid) = fs::read_to_string(&uid_path) {
        let trimmed = uid.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }
    let uid = uuid::Uuid::new_v4().to_string();
    let _ = fs::write(&uid_path, &uid);
    uid
}
```

- [ ] **Step 3: Update list() to read collection UIDs**

In the `list()` method, after getting the directory name and count, read the uid:
```rust
let uid = read_or_create_uid(&path);
result.push(CollectionSummary::new(uid, &name, path.to_string_lossy().to_string(), count));
```

- [ ] **Step 4: Update build_folder_tree to read folder UIDs**

At the start of `build_folder_tree`, after creating the `Folder`:
```rust
let mut folder = Folder::new(name);
folder.uid = read_or_create_uid(current);
```

- [ ] **Step 5: Update create_folder to write .uid file**

After `fs::create_dir_all`, write a `.uid` file:
```rust
fn create_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
    let collection_dir = self.collection_path(collection);
    let dir_path = self.validate_path(&collection_dir, Path::new(path))?;
    fs::create_dir_all(&dir_path)?;
    read_or_create_uid(&dir_path); // generates and writes .uid
    Ok(())
}
```

- [ ] **Step 6: Migrate request UIDs in build_folder_tree**

After deserializing a request, check if its uid was auto-generated (by serde default). Since we can't easily detect this, instead check if the file on disk has a `uid` field. The simplest approach: always re-write the file if the parsed `uid` differs from what's in the file. Or more practically: on first read, if the JSON doesn't contain `"uid"`, write it back with the generated uid.

```rust
if let Ok(mut request) = serde_json::from_str::<rocket_collection::Request>(&content) {
    request.file_name = Some(entry_name.clone());
    // Migrate: if the file doesn't have a uid field, write it back.
    if !content.contains("\"uid\"") {
        let json = serde_json::to_string_pretty(&request).unwrap_or_default();
        let _ = fs::write(&path, json);
    }
    folder.add_request(request);
}
```

- [ ] **Step 7: Run tests**

Run: `cargo test --workspace`
Expected: All pass.

- [ ] **Step 8: Commit**

```bash
git add crates/rocket-infra/
git commit -m "feat: read/write .uid files for folders and collections, migrate request UIDs"
```

---

### Task 3: save_request returns Request with uid

**Files:**
- Modify: `crates/rocket-app/src/collection_service.rs`
- Modify: `src-tauri/src/commands/collections.rs`

- [ ] **Step 1: Update CollectionService::save_request to return Request**

```rust
pub fn save_request(&self, collection: &str, path: &str, request: &Request) -> DomainResult<Request> {
    self.repo.save_request(collection, path, request)?;
    // Read back to get the persisted version (with uid).
    self.repo.get_request(collection, path)
}
```

- [ ] **Step 2: Update Tauri command to return Request**

In `src-tauri/src/commands/collections.rs`:
```rust
#[tauri::command]
pub fn save_request(
    collection: String,
    path: String,
    request: Request,
    svc: State<'_, CollectionService>,
) -> Result<Request, DomainError> {
    svc.save_request(&collection, &path, &request)
}
```

- [ ] **Step 3: Fix compilation errors**

Run `cargo check --workspace` and fix any issues.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-app/ src-tauri/
git commit -m "feat: save_request returns Request with uid"
```

---

### Task 4: Frontend types and tab ID migration

**Files:**
- Modify: `src/lib/tauri-api.ts`
- Modify: `src/lib/pane-utils.ts`
- Modify: `src/stores/pane-store.ts`

- [ ] **Step 1: Add uid to frontend types**

In `src/lib/tauri-api.ts`:

Add `uid` to `Request`:
```typescript
export interface Request {
  uid: string;
  name: string;
  // ... rest unchanged
}
```

Add `uid` to `Folder`:
```typescript
export interface Folder {
  uid: string;
  name: string;
  items: CollectionItem[];
}
```

Add `uid` to `CollectionSummary`:
```typescript
export interface CollectionSummary {
  uid: string;
  name: string;
  path: string;
  requestCount: number;
}
```

Update `saveRequest` return type from `void` to `Request`:
```typescript
export const saveRequest = (
  collection: string,
  path: string,
  request: Request,
) => invoke<Request>("save_request", { collection, path, request });
```

- [ ] **Step 2: Remove findTabBySource from pane-utils.ts**

Delete the `findTabBySource` function. It's no longer needed — uid matching replaces it.

- [ ] **Step 3: Update openTab in pane-store.ts**

Remove the `findTabBySource` fallback in `openTab`. Tab matching is now purely by `tab.id` (which will be the uid):

```typescript
openTab(tab, groupId) {
    const { root, activeGroupId } = get();
    const existing = findTabInTree(root, tab.id);
    if (existing) {
      const newRoot = updateLeaf(root, existing.leaf.groupId, (leaf) => ({
        ...leaf,
        activeTabId: existing.tab.id,
      }));
      set({ root: newRoot, activeGroupId: existing.leaf.groupId });
      return;
    }
    // ... rest unchanged
},
```

Remove the `findTabBySource` import.

- [ ] **Step 4: Verify TypeScript**

Run: `npx tsc --noEmit`
Expected: Clean (may have errors from sidebar — fixed in Task 5).

- [ ] **Step 5: Commit**

```bash
git add src/lib/tauri-api.ts src/lib/pane-utils.ts src/stores/pane-store.ts
git commit -m "feat: frontend types with uid, tab matching by uid"
```

---

### Task 5: Sidebar and dialogs use uid for tab matching

**Files:**
- Modify: `src/components/layout/CollectionsSidebar.tsx`
- Modify: `src/components/collections/SaveToCollectionDialog.tsx`
- Modify: `src/components/request/SaveRequestButton.tsx`

- [ ] **Step 1: Update RequestNode to use uid as tabId**

In `CollectionsSidebar.tsx`, `RequestNode` currently uses `const tabId = "${collectionName}/${path}"`. Change to use the request's uid. The `RequestNode` needs a new `uid` prop:

Add `uid: string` to `RequestNode` props. Set `const tabId = uid`.

Where `RequestNode` is rendered (in `FolderNode` and `CollectionNode`), pass `uid={item.uid}`.

- [ ] **Step 2: Update handleNewRequest to use uid from save response**

`saveRequest` now returns a `Request` with a uid. Use it:

```typescript
const saved = await saveRequest(collection, path, { ... });
const tab: Tab = {
  id: saved.uid,
  title: name,
  // ...
  source: { collection, path: saved.fileName ?? `${path}.json` },
};
```

- [ ] **Step 3: Update handleClick in RequestNode**

When clicking a request in the sidebar, use uid as the tab id:

```typescript
function handleClick() {
    const tab: Tab = {
      id: uid,  // was tabId = collection/path
      title: name,
      tabType: 'request',
      // ...
      source: { collection: collectionName, path },
    };
    usePaneStore.getState().openTab(tab);
}
```

- [ ] **Step 4: Update isActiveRequest to use uid**

The highlight function checks if any active tab matches. Now it should match by uid:
```typescript
function isActiveRequest(node: PaneNode, uid: string): boolean {
  if (node.type === 'leaf') return node.activeTabId === uid;
  return isActiveRequest(node.children[0], uid) || isActiveRequest(node.children[1], uid);
}
```

- [ ] **Step 5: Update SaveToCollectionDialog**

After save, use the returned request's uid as the tab id:
```typescript
const saved = await saveReq(selectedCollection, requestName.trim(), { ... });
// In the tab update:
tabs[idx] = {
  ...tabs[idx],
  id: saved.uid,
  // ...
};
```

- [ ] **Step 6: Verify TypeScript and tests**

Run: `npx tsc --noEmit && npx vitest run`
Expected: Clean, 70+ tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/components/ src/lib/ src/stores/
git commit -m "feat: sidebar and dialogs use uid for tab matching"
```

---

### Task 6: Verify end-to-end

- [ ] **Step 1: Restart yarn tauri dev**

Kill old process, restart to pick up Rust changes.

- [ ] **Step 2: Test migration**

Open app with existing collections. Check:
- `.uid` files created in each collection and folder directory
- Request JSON files now contain `"uid"` field
- Collections list in sidebar shows correctly

- [ ] **Step 3: Test tab matching after rename**

1. Create collection, add request
2. Open request in tab
3. Double-click tab to rename
4. Click renamed request in sidebar → should focus existing tab (not open duplicate)

- [ ] **Step 4: Test new request flow**

1. Click + to create new request
2. Tab opens with uid-based ID
3. Save to collection
4. Click in sidebar → focuses same tab

- [ ] **Step 5: Commit any fixes**

```bash
git add -A && git commit -m "fix: end-to-end UUID identity verification"
```
