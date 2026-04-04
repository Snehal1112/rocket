# Spec: Per-Request Docs Tab (Phase 1)

**Date**: 2026-04-04  
**Status**: Approved  
**Scope**: Phase 1 — request-level documentation only  
**Deferred**: Folder tab (Phase 2)

---

## Goal

Add a "Docs" tab to the request editor that lets users write Markdown documentation for individual requests. The content is stored in the request's YAML file (`docs` field) and round-trips through the existing `Documentation` domain type.

---

## User Experience

### Tab Placement

A new **Docs** tab is added to the request editor tab bar (BrunoTabBar), to the right of Variables:

```
Params | Headers | Body | Auth | Variables | Docs
```

When the Docs tab is active:
- The **right slot** of the tab bar shows an Edit / Preview toggle (same `<Tabs>` pattern as `WorkspaceOverviewTab`).
- The request area below the tab bar renders `<RequestDocsPanel>`.

### Edit Mode

- Full-width `<textarea>` with monospace font, transparent background.
- Placeholder: `"Add docs for this request...\n\nSupports **Markdown**"`
- Saves on blur and on explicit "Save" button click.
- Footer bar: `"Markdown supported · saves on blur"` + **Save** button (h-6, text-[10px]).

### Preview Mode

- Scrollable area with `ReactMarkdown` + `remarkGfm` inside `.prose-doc text-xs leading-relaxed`.
- Empty state (no content): centered icon + message + **+ Add Documentation** button that switches to Edit mode.

### Docs Indicator

When a request has non-empty docs, the Docs tab label shows a small filled circle (same `bg-primary` dot pattern used by Body/Auth tabs).

### Unsaved request (no `tab.source`)

The Docs tab is still shown. Save is disabled with a subtle note: *"Save this request to a collection before adding docs."*

---

## Data Model

### Backend — no changes needed

`Request.docs: Option<Documentation>` already exists in `crates/rocket-collection/src/request.rs`.  
`OcHttpRequest.docs: Option<String>` already round-trips through `crates/rocket-infra/src/oc_conversions.rs`.

### Frontend — `RequestState` (`src/types/pane-types.ts`)

Add one field:

```ts
export interface RequestState {
  // ... existing fields ...
  docs: string | null;  // NEW
}
```

### Frontend — `Request` interface (`src/lib/tauri-api.ts`)

Add one optional field:

```ts
export interface Request {
  // ... existing fields ...
  docs?: string | null;  // NEW — round-trips with backend
}
```

---

## New Tauri Command

### `update_request_docs`

A dedicated command that loads the request from disk, patches only the `docs` field, and writes it back. This avoids sending the full `RequestState` over IPC just to update one field.

**Signature (Rust, `src-tauri/src/commands/collections.rs`):**

```rust
#[tauri::command]
pub fn update_request_docs(
    collection: String,
    path: String,
    docs: Option<String>,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.update_request_docs(&collection, &path, docs)
}
```

**Service method (`crates/rocket-app/src/collection_service.rs`):**

```rust
pub fn update_request_docs(&self, collection: &str, path: &str, docs: Option<String>) -> DomainResult<()> {
    let mut request = self.repo.get_request(collection, path)?;
    request.docs = docs.map(Documentation::text);
    self.repo.save_request(collection, path, &request)?;
    Ok(())
}
```

Import required: `use rocket_shared::description::Documentation;`

**Frontend wrapper (`src/lib/tauri-api.ts`):**

```ts
export const updateRequestDocs = (
  collection: string,
  path: string,
  docs: string | null,
): Promise<void> =>
  invoke<void>('update_request_docs', { collection, path, docs });
```

---

## Frontend Changes

### `src/lib/pane-utils.ts`

1. `mapApiRequestToState`: add `docs: req.docs ?? null` to the returned object.
2. `createDefaultRequest`: add `docs: null` to the returned object.

### `src/components/request/RequestDocsPanel.tsx` — new file

Props:
```ts
interface RequestDocsPanelProps {
  docs: string | null;
  mode: 'edit' | 'preview';
  hasSource: boolean;       // tab.source is defined
  onSave: (docs: string | null) => void;
}
```

Behaviour:
- Manages local text state initialized from `docs` prop; re-syncs when `docs` prop changes (e.g. tab switches).
- Edit mode: `<textarea>` + save-on-blur + Save button footer. Disabled (read-only hint) when `!hasSource`.
- Preview mode: `ReactMarkdown` inside `.prose-doc text-xs leading-relaxed`, or empty-state with **+ Add Documentation** CTA.

### `src/components/request/RequestPanel.tsx`

1. Extend `SectionTab` type: `'params' | 'headers' | 'body' | 'auth' | 'variables' | 'docs'`
2. Import `updateRequestDocs` from `@/lib/tauri-api`.
3. Import new `RequestDocsPanel`.
4. Add Docs entry to `tabDefs` array (at the end, with dot indicator when `request.docs` is non-null).
5. In `tabRightContent` memo: when `activeSection === 'docs'`, return Edit/Preview `<Tabs>` toggle; also add local `docMode` state `'edit' | 'preview'`.
6. In the section render block: add `{activeSection === 'docs' && <RequestDocsPanel ... />}`.
7. `handleSaveDocs` async function: calls `updateRequestDocs(tab.source.collection, tab.source.path, newDocs)`, then `updateRequest(tab.id, { docs: newDocs })`.

---

## Save Flow

```
User edits textarea
  → onBlur / Save button click
    → handleSaveDocs(content):
        const trimmed = content.trim() || null
        if (!tab.source) return   // unsaved request: skip
        await updateRequestDocs(tab.source.collection, tab.source.path, trimmed)
        updateRequest(tab.id, { docs: trimmed })   // update pane store
```

---

## Out of Scope (Phase 2)

- Folder tab with Headers / Vars / Auth / Docs sub-tabs.
- Collection Readme tab: already implemented, no change.

---

## Files Changed

| File | Change |
|---|---|
| `src/types/pane-types.ts` | Add `docs: string \| null` to `RequestState` |
| `src/lib/pane-utils.ts` | Map `docs` in `mapApiRequestToState`; add `docs: null` to `createDefaultRequest` |
| `src/lib/tauri-api.ts` | Add `docs?` to `Request` interface; add `updateRequestDocs` function |
| `crates/rocket-app/src/collection_service.rs` | Add `update_request_docs` service method |
| `src-tauri/src/commands/collections.rs` | Add `update_request_docs` Tauri command |
| `src-tauri/src/lib.rs` | Register new command in `invoke_handler` |
| `src/components/request/RequestDocsPanel.tsx` | New component |
| `src/components/request/RequestPanel.tsx` | Add Docs tab, docMode state, tabRightContent, panel render |
