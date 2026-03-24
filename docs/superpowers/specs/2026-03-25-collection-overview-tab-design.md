# Collection Overview Tab — Design Spec

**Date:** 2026-03-25
**Status:** Approved
**Scope:** Sub-project 1 of Collection Overview (name, description, auth, headers, request list, method breakdown)

## Problem

There is no way to view or edit collection-level details (description, auth, headers) in a dedicated page. Clicking a collection only expands/collapses the tree. Users have to open a modal dialog for settings, and there is no description field or overview of the collection contents.

## Solution

Open a collection overview as a **tab in the editor area** (like request tabs). Shows collection metadata, a method breakdown chart, and tabbed sections for auth, headers, and a request list.

## How It Opens

- **Double-click** collection name in sidebar → opens overview tab
- **Single-click** still expands/collapses the tree (unchanged)
- **Context menu** → "Overview" option added to collection right-click menu

## Tab Identity

- Tab `id` = collection uid (from `.uid` file)
- Tab `tabType` = `'collection'` (new type)
- Tab `title` = collection name
- `source` = `{ collection: collectionName }` (no path — it's the collection root)

## Layout

```
┌──────────────────────────────────────────────────────┐
│                                                      │
│  📁 My API Collection                                │
│     12 requests · 3 folders · Modified 2h ago        │
│                                                      │
│  ┌──────────────────────────────────────────────┐    │
│  │ A REST API for managing user accounts,       │    │
│  │ authentication, and billing.                 │    │
│  └──────────────────────────────────────────────┘    │
│                                                      │
│  ┌──────────────────────────────────────────────┐    │
│  │  Method Breakdown                            │    │
│  │                                              │    │
│  │  GET     ████████████████████       5  (42%) │    │
│  │  POST    ████████████               3  (25%) │    │
│  │  PUT     ████████                   2  (17%) │    │
│  │  PATCH   ████                       1   (8%) │    │
│  │  DELETE  ████                       1   (8%) │    │
│  │                                              │    │
│  └──────────────────────────────────────────────┘    │
│                                                      │
│  [Auth]  [Headers]  [Requests]                       │
│  ─────────────────────────────────────────────       │
│                                                      │
│  (active tab content here)                           │
│                                                      │
└──────────────────────────────────────────────────────┘
```

### Sections

**Header area (always visible, top)**
- Folder icon + collection name (double-click to rename inline)
- Stats line: `{N} requests · {M} folders · Modified {relative time}`
- Stats are computed from the collection tree data

**Description (always visible, below header)**
- Plain text textarea (not markdown for MVP)
- Editable inline — changes save when the tab loses focus or user clicks Save
- Stored as `description` field in `collection.json`
- If empty, show placeholder: "Add a description..."

**Method Breakdown card (always visible)**
- One card with vertical list of methods
- Each row: method name (colored), horizontal progress bar (colored), count, percentage
- Only methods with count > 0 are shown
- Bar width proportional to count / total requests
- Colors match existing method colors (GET=emerald, POST=amber, PUT=blue, PATCH=violet, DELETE=red)
- Computed from the collection tree — no new API call

**Tabs (below breakdown)**
- **Auth** — reuses existing `AuthEditor` component. Shows inherited auth config. Changes save to `collection.json`.
- **Headers** — reuses existing `HeadersEditor` component. Shows default headers. Changes save to `collection.json`.
- **Requests** — table of all requests in the collection. Columns: Method (badge), Name, URL. Grouped by folder with folder name headers. Click a row to open the request in a new tab.

### Save behavior

- **Description**: saved on blur or tab switch (same as request auto-save pattern)
- **Auth + Headers**: Save button at the bottom of each tab section. Calls `saveCollectionSettings`.
- No auto-save on every keystroke.

## Backend Changes

### Add `description` to CollectionSettings

In `crates/rocket-collection/src/settings.rs`:
```rust
pub struct CollectionSettings {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub auth: Option<Auth>,
    #[serde(default)]
    pub headers: Vec<Header>,
}
```

### Add collection metadata to the `get` response

The `Collection` struct needs to carry settings data so the overview tab can display description, auth, and headers without a separate API call. Extend the Tauri `get_collection` command to also return settings:

Option A: Include `settings: CollectionSettings` in the `Collection` struct.
Option B: Add a separate `get_collection_settings` command.

**Choose A** — include settings in Collection so one `getCollection` call gives everything.

Add to `Collection` struct:
```rust
pub struct Collection {
    pub uid: String,  // if available from .uid file
    pub name: String,
    pub root: Folder,
    pub settings: CollectionSettings,
}
```

Update `FsCollectionRepo::get()` to also read `collection.json` and include settings.

### Add last modified timestamp

Read the directory's modification time (`fs::metadata(path)?.modified()`) in `list()` and include it in `CollectionSummary` as `modified_at: Option<String>` (ISO 8601 string).

## Frontend Changes

### New component: `CollectionOverviewTab`

Create `src/components/collections/CollectionOverviewTab.tsx`.

Props: `collectionName: string`, `collectionUid: string`

On mount, calls `getCollection(collectionName)` to load the full tree + settings.

### Tab type restructuring

The current `Tab` type has `request: RequestState` which is required for all tabs. Collection tabs don't have request data, so the type needs restructuring as a discriminated union:

```typescript
export type Tab = RequestTab | CollectionTab;

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
```

All components that access `tab.request` need a guard: `if (tab.tabType !== 'collection')` or check `'request' in tab`.

### EditorGroup routing

In `EditorGroup.tsx`, when the active tab has `tabType === 'collection'`, render `CollectionOverviewTab` instead of `RequestPanel`:

```tsx
{activeTab.tabType === 'collection' ? (
  <CollectionOverviewTab collectionName={activeTab.collectionName} />
) : (
  <RequestPanel tab={activeTab} groupId={node.groupId} />
)}
```

### Sidebar wiring

In `CollectionNode`:
- **Double-click** on the collection name button → open overview tab
- **Context menu** → add "Overview" item that opens the overview tab

Opening the tab:
```tsx
const tab: CollectionTab = {
  id: summary.uid,
  title: summary.name,
  tabType: 'collection',
  collectionName: summary.name,
  isDirty: false,
  source: { collection: summary.name, path: '' },
};
usePaneStore.getState().openTab(tab);
```

### Save behavior clarification

- **Description**: saved on blur (not debounced, immediate single write when focus leaves the textarea)
- **Auth + Headers**: explicit Save button at the bottom of each tab section
- **No auto-save on keystroke** for any field in the overview tab

### Stats computation

- Request count: total leaf requests recursively (including inside nested folders)
- Folder count: total directories at all levels
- Modified time: from `fs::metadata(path)?.modified()`. If unavailable, omit from stats line. Frontend formats as relative time (e.g., "2h ago", "3 days ago")
- Singular/plural: "1 request" vs "2 requests", "1 folder" vs "2 folders"

### Request list details

- Root-level requests listed first, then folders as group headers with their requests indented
- Nested folders shown with hierarchy (indentation)
- Sort order: alphabetical within each level
- Click a request row → opens it as a request tab (same as sidebar click)

### Method breakdown component

Create `src/components/collections/MethodBreakdown.tsx`.

Takes `items: CollectionItem[]`, recursively counts methods, renders the bar chart.

### Request list component

Create `src/components/collections/RequestList.tsx`.

Takes `items: CollectionItem[]`, renders a grouped table. Click row → `openTab` with request data.

## Files

### Rust
- Modify: `crates/rocket-collection/src/settings.rs` (add description field)
- Modify: `crates/rocket-collection/src/collection.rs` (add settings to Collection)
- Modify: `crates/rocket-collection/src/summary.rs` (add modified_at)
- Modify: `crates/rocket-infra/src/fs_collection_repo.rs` (load settings in get(), modified time in list())

### Frontend
- Create: `src/components/collections/CollectionOverviewTab.tsx`
- Create: `src/components/collections/MethodBreakdown.tsx`
- Create: `src/components/collections/RequestList.tsx`
- Modify: `src/types/pane-types.ts` (add 'collection' to tabType)
- Modify: `src/lib/tauri-api.ts` (update Collection and CollectionSummary types)
- Modify: `src/components/panes/EditorGroup.tsx` (route collection tabs)
- Modify: `src/components/layout/CollectionsSidebar.tsx` (double-click + context menu)
