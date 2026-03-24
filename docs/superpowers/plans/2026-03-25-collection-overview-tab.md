# Collection Overview Tab Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Open a collection overview as a tab in the editor area showing name, description, method breakdown, auth, headers, and request list.

**Architecture:** Restructure the `Tab` type as a discriminated union (RequestTab | CollectionTab). Add description to CollectionSettings and settings to the Collection struct on the Rust side. Create three new frontend components: CollectionOverviewTab, MethodBreakdown, RequestList. Wire into sidebar via double-click and context menu.

**Tech Stack:** Rust (serde, fs), React, TypeScript, Zustand, shadcn/ui, Tailwind

**Spec:** `docs/superpowers/specs/2026-03-25-collection-overview-tab-design.md`

---

### File Structure

```
Rust:
  crates/rocket-collection/src/settings.rs       # add description field
  crates/rocket-collection/src/collection.rs      # add settings to Collection
  crates/rocket-collection/src/summary.rs         # add modified_at
  crates/rocket-infra/src/fs_collection_repo.rs   # load settings in get(), modified time in list()

Frontend:
  src/types/pane-types.ts                         # restructure Tab as discriminated union
  src/lib/tauri-api.ts                            # update Collection + CollectionSummary types
  src/components/collections/CollectionOverviewTab.tsx  # main overview component
  src/components/collections/MethodBreakdown.tsx   # method bar chart
  src/components/collections/RequestList.tsx        # grouped request table
  src/components/panes/EditorGroup.tsx             # route collection tabs
  src/components/panes/TabItem.tsx                 # guard tab.request access
  src/components/panes/TabBar.tsx                  # guard tab.request/source access
  src/components/request/SaveRequestButton.tsx     # guard tab type
  src/components/layout/CollectionsSidebar.tsx     # double-click + context menu
  src/stores/pane-store.ts                         # handle new tab type
```

---

### Task 1: Rust backend — add description to settings and settings to Collection

**Files:**
- Modify: `crates/rocket-collection/src/settings.rs`
- Modify: `crates/rocket-collection/src/collection.rs`
- Modify: `crates/rocket-collection/src/summary.rs`
- Modify: `crates/rocket-infra/src/fs_collection_repo.rs`

- [ ] **Step 1: Add description to CollectionSettings**

In `crates/rocket-collection/src/settings.rs`, add:
```rust
#[serde(default)]
pub description: Option<String>,
```
as the first field of `CollectionSettings`.

- [ ] **Step 2: Add settings field to Collection struct**

In `crates/rocket-collection/src/collection.rs`, add:
```rust
#[serde(default)]
pub settings: CollectionSettings,
```

Import `CollectionSettings` from the settings module. Update `Collection::new()` to include `settings: CollectionSettings::default()`.

- [ ] **Step 3: Add modified_at to CollectionSummary**

In `crates/rocket-collection/src/summary.rs`, add:
```rust
pub modified_at: Option<String>,
```

Update `CollectionSummary::new()` to accept `modified_at: Option<String>` as the last parameter.

- [ ] **Step 4: Update FsCollectionRepo::get() to load settings**

In `crates/rocket-infra/src/fs_collection_repo.rs`, update the `get()` method:
```rust
fn get(&self, name: &str) -> DomainResult<Collection> {
    let path = self.collection_path(name);
    if !path.exists() {
        return Err(DomainError::NotFound(format!("Collection '{}'", name)));
    }
    let root = build_folder_tree(&path)?;
    let settings = self.get_settings(name).unwrap_or_default();
    Ok(Collection { name: name.to_string(), root, settings })
}
```

- [ ] **Step 5: Update list() to include modified_at**

In `list()`, after getting the directory entry, read the modification time:
```rust
let modified_at = fs::metadata(&path)
    .and_then(|m| m.modified())
    .ok()
    .map(|t| {
        let datetime: chrono::DateTime<chrono::Utc> = t.into();
        datetime.to_rfc3339()
    });
```

Note: `chrono` may not be a dependency. If not, use a simpler format or just pass the unix timestamp as a string. Check if chrono is available — if not, use:
```rust
let modified_at = fs::metadata(&path)
    .and_then(|m| m.modified())
    .ok()
    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
    .map(|d| d.as_secs().to_string());
```

Pass `modified_at` to `CollectionSummary::new()`.

- [ ] **Step 6: Fix all compilation errors**

Run `cargo check --workspace`. Fix missing fields in struct constructors, test mocks, etc.

- [ ] **Step 7: Run tests**

Run: `cargo test --workspace`
Expected: All pass.

- [ ] **Step 8: Commit**

```bash
git add crates/
git commit -m "feat: add description to settings, settings to Collection, modified_at to summary"
```

---

### Task 2: Restructure Tab type as discriminated union

**Files:**
- Modify: `src/types/pane-types.ts`

- [ ] **Step 1: Restructure the Tab type**

Replace the current `Tab` interface with a discriminated union:

```typescript
interface BaseTab {
  id: string;
  title: string;
  isDirty: boolean;
  source?: { collection: string; path: string };
}

export interface RequestTab extends BaseTab {
  tabType: 'request' | 'draft' | 'history';
  request: RequestState;
  response: ResponseState | null;
}

export interface CollectionTab extends BaseTab {
  tabType: 'collection';
  collectionName: string;
}

export type Tab = RequestTab | CollectionTab;
```

Export `RequestTab` and `CollectionTab` separately so components can narrow the type.

- [ ] **Step 2: Add type guard helper**

```typescript
export function isRequestTab(tab: Tab): tab is RequestTab {
  return tab.tabType !== 'collection';
}
```

- [ ] **Step 3: Commit (will have TS errors — fixed in Task 3)**

```bash
git add src/types/pane-types.ts
git commit -m "refactor: restructure Tab as discriminated union (RequestTab | CollectionTab)"
```

---

### Task 3: Fix all components that access tab.request

**Files:**
- Modify: `src/components/panes/TabItem.tsx`
- Modify: `src/components/panes/TabBar.tsx`
- Modify: `src/components/request/SaveRequestButton.tsx`
- Modify: `src/components/request/RequestPanel.tsx`
- Modify: `src/stores/pane-store.ts`
- Modify: `src/lib/auto-save.ts`
- Modify: `src/components/panes/EditorGroup.tsx`

- [ ] **Step 1: Fix TabItem.tsx**

`TabItem` accesses `tab.request.url` and `tab.request.method`. Guard with type check:

```typescript
import { isRequestTab } from '@/types/pane-types';

// Where tab.request.method is accessed:
const method = isRequestTab(tab) ? tab.request.method : undefined;
const url = isRequestTab(tab) ? tab.request.url : undefined;
```

For collection tabs, show a folder icon and collection name instead of method badge.

- [ ] **Step 2: Fix TabBar.tsx**

The save context menu item accesses `tab.isDirty` and `tab.source` — these are on `BaseTab` so they're fine. But the `rocket:save-draft` event dispatch should only fire for request tabs. Add a guard.

- [ ] **Step 3: Fix SaveRequestButton.tsx**

`buildRequestPayload` accesses `tab.request`. The component should only render for request tabs. Add a guard in the parent (`RequestPanel`) or in the component itself:

```typescript
export function SaveRequestButton({ tab, groupId }: { tab: RequestTab; groupId: string }) {
```

Change prop type from `Tab` to `RequestTab`.

- [ ] **Step 4: Fix RequestPanel.tsx**

Change prop type from `Tab` to `RequestTab`:
```typescript
interface RequestPanelProps {
  tab: RequestTab;
  groupId: string;
}
```

- [ ] **Step 5: Fix EditorGroup.tsx**

Route based on tab type:
```tsx
import { isRequestTab } from '@/types/pane-types';
import { CollectionOverviewTab } from '@/components/collections/CollectionOverviewTab';

// In render:
{activeTab && isRequestTab(activeTab) ? (
  <RequestPanel tab={activeTab} groupId={node.groupId} />
) : activeTab?.tabType === 'collection' ? (
  <CollectionOverviewTab collectionName={(activeTab as CollectionTab).collectionName} />
) : (
  <div>No open tabs</div>
)}
```

Note: `CollectionOverviewTab` doesn't exist yet — create a placeholder for now:
```tsx
// src/components/collections/CollectionOverviewTab.tsx
export function CollectionOverviewTab({ collectionName }: { collectionName: string }) {
  return <div className="p-4">Collection: {collectionName} (overview coming)</div>;
}
```

- [ ] **Step 6: Fix pane-store.ts**

The `updateRequest` action accesses `tab.request`. Add a guard:
```typescript
const newRoot = updateTabInTree(root, tabId, (tab) => {
  if (tab.tabType === 'collection') return tab;
  return { ...tab, request: { ...tab.request, ...patch }, isDirty: true };
});
```

Similar guards in `setResponse`, `markDirty`, `markClean`, `updateTabTitle`, auto-save trigger.

- [ ] **Step 7: Fix auto-save.ts if needed**

Check if any function signature needs `RequestTab` instead of `Tab`.

- [ ] **Step 8: Verify TypeScript**

Run: `npx tsc --noEmit`
Expected: Clean.

- [ ] **Step 9: Commit**

```bash
git add src/
git commit -m "refactor: fix all components for Tab discriminated union"
```

---

### Task 4: Update frontend types for Collection and CollectionSummary

**Files:**
- Modify: `src/lib/tauri-api.ts`

- [ ] **Step 1: Update Collection type**

```typescript
export interface Collection {
  name: string;
  root: Folder;
  settings: CollectionSettings;
}

export interface CollectionSettings {
  description?: string;
  auth?: Auth;
  headers: Header[];
}
```

- [ ] **Step 2: Update CollectionSummary type**

Add `modifiedAt`:
```typescript
export interface CollectionSummary {
  uid: string;
  name: string;
  path: string;
  requestCount: number;
  modifiedAt?: string;
}
```

- [ ] **Step 3: Commit**

```bash
git add src/lib/tauri-api.ts
git commit -m "feat: update Collection and CollectionSummary types with settings and modifiedAt"
```

---

### Task 5: Create MethodBreakdown and RequestList components

**Files:**
- Create: `src/components/collections/MethodBreakdown.tsx`
- Create: `src/components/collections/RequestList.tsx`

- [ ] **Step 1: Create MethodBreakdown**

`src/components/collections/MethodBreakdown.tsx`:

Takes `items: CollectionItem[]`. Recursively counts methods. Renders a card with vertical bar chart.

Each row: method name (colored text), horizontal bar (colored bg, width proportional to count/total), count, percentage. Only methods with count > 0.

Use shadcn Card component. Method colors match sidebar (GET=emerald, POST=amber, PUT=blue, PATCH=violet, DELETE=red).

- [ ] **Step 2: Create RequestList**

`src/components/collections/RequestList.tsx`:

Takes `items: CollectionItem[]` and `collectionName: string`. Renders a grouped table.

- Root-level requests first
- Folder headers with indented requests
- Columns: method badge, name, URL
- Click row → opens request as a tab (uses `mapApiRequestToState` and `openTab`)
- Filter input at the top

Use shadcn Table, Input, Badge components.

- [ ] **Step 3: Verify TypeScript**

Run: `npx tsc --noEmit`

- [ ] **Step 4: Commit**

```bash
git add src/components/collections/MethodBreakdown.tsx src/components/collections/RequestList.tsx
git commit -m "feat: MethodBreakdown bar chart and RequestList table components"
```

---

### Task 6: Build the full CollectionOverviewTab

**Files:**
- Modify: `src/components/collections/CollectionOverviewTab.tsx`

- [ ] **Step 1: Replace placeholder with full implementation**

The component:
1. On mount, calls `getCollection(collectionName)` to load tree + settings
2. Shows loading state while fetching

Layout:
- **Header**: Folder icon + collection name (double-click to rename) + stats line
- **Description**: Textarea, saves on blur via `saveCollectionSettings`
- **Method Breakdown**: `<MethodBreakdown items={collection.root.items} />`
- **Tabs**: Auth | Headers | Requests using shadcn Tabs
  - Auth: `<AuthEditor>` (reuse existing)
  - Headers: `<HeadersEditor>` (reuse existing)
  - Requests: `<RequestList items={collection.root.items} collectionName={collectionName} />`
- **Save button** at bottom of Auth/Headers tabs

Stats line computation:
```typescript
function computeStats(items: CollectionItem[]): { requests: number; folders: number } {
  let requests = 0, folders = 0;
  for (const item of items) {
    if (item.type === 'request') requests++;
    else { folders++; requests += computeStats(item.items).requests; folders += computeStats(item.items).folders; }
  }
  return { requests, folders };
}
```

Relative time formatting:
```typescript
function relativeTime(timestamp: string): string {
  const seconds = Math.floor((Date.now() - Number(timestamp) * 1000) / 1000);
  if (seconds < 60) return 'just now';
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return `${Math.floor(seconds / 86400)}d ago`;
}
```

- [ ] **Step 2: Verify TypeScript**

Run: `npx tsc --noEmit`

- [ ] **Step 3: Commit**

```bash
git add src/components/collections/CollectionOverviewTab.tsx
git commit -m "feat: full CollectionOverviewTab with description, breakdown, auth, headers, requests"
```

---

### Task 7: Wire sidebar — double-click + context menu

**Files:**
- Modify: `src/components/layout/CollectionsSidebar.tsx`

- [ ] **Step 1: Add double-click handler to CollectionNode**

On the collection name button, add `onDoubleClick` that opens the overview tab:

```tsx
onDoubleClick={(e) => {
  e.stopPropagation();
  const tab: CollectionTab = {
    id: summary.uid,
    title: summary.name,
    tabType: 'collection',
    collectionName: summary.name,
    isDirty: false,
    source: { collection: summary.name, path: '' },
  };
  usePaneStore.getState().openTab(tab);
}}
```

Import `CollectionTab` from `@/types/pane-types`.

Single click still expands/collapses (unchanged).

- [ ] **Step 2: Add "Overview" to context menu**

In `CollectionNode`'s `ContextMenuContent`, add at the top:

```tsx
<ContextMenuItem onClick={() => {
  const tab: CollectionTab = {
    id: summary.uid,
    title: summary.name,
    tabType: 'collection',
    collectionName: summary.name,
    isDirty: false,
    source: { collection: summary.name, path: '' },
  };
  usePaneStore.getState().openTab(tab);
}}>
  Overview
</ContextMenuItem>
<ContextMenuSeparator />
```

- [ ] **Step 3: Verify TypeScript and tests**

Run: `npx tsc --noEmit && npx vitest run`
Expected: Clean, all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/components/layout/CollectionsSidebar.tsx
git commit -m "feat: open collection overview via double-click or context menu"
```

---

### Task 8: End-to-end verification

- [ ] **Step 1: Restart yarn tauri dev** (Rust changes)
- [ ] **Step 2: Test overview tab opens** — double-click collection, verify tab opens
- [ ] **Step 3: Test description** — edit, blur, reopen — persists
- [ ] **Step 4: Test method breakdown** — shows correct counts with colored bars
- [ ] **Step 5: Test auth/headers tabs** — edit, save, reopen — persists
- [ ] **Step 6: Test request list** — click row opens request tab
- [ ] **Step 7: Test stats line** — correct request/folder counts
- [ ] **Step 8: Commit any fixes**
