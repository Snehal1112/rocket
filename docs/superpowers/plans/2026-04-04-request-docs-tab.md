# Plan: Per-Request Docs Tab (Phase 1)

**Spec**: `docs/superpowers/specs/2026-04-04-request-docs-tab-design.md`  
**Date**: 2026-04-04

---

## Steps

### 1. Commit spec
Commit `docs/superpowers/specs/2026-04-04-request-docs-tab-design.md`.

---

### 2. Add `docs` to frontend types and API

**`src/types/pane-types.ts`**
- Add `docs: string | null;` to `RequestState` interface.

**`src/lib/tauri-api.ts`**
- Add `docs?: string | null;` to `Request` interface.
- Add `updateRequestDocs` export:
  ```ts
  export const updateRequestDocs = (
    collection: string,
    path: string,
    docs: string | null,
  ): Promise<void> =>
    invoke<void>('update_request_docs', { collection, path, docs });
  ```

**`src/lib/pane-utils.ts`**
- In `mapApiRequestToState`: add `docs: req.docs ?? null` to the returned `RequestState`.
- In `createDefaultRequest`: add `docs: null`.

---

### 3. Add Rust service method

**`crates/rocket-app/src/collection_service.rs`**
- Add import: `use rocket_shared::description::Documentation;`
- Add method after `rename_request`:
  ```rust
  pub fn update_request_docs(&self, collection: &str, path: &str, docs: Option<String>) -> DomainResult<()> {
      let mut request = self.repo.get_request(collection, path)?;
      request.docs = docs.map(Documentation::text);
      self.repo.save_request(collection, path, &request)?;
      Ok(())
  }
  ```

---

### 4. Add Tauri command

**`src-tauri/src/commands/collections.rs`**
- Add after `save_request_variables`:
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

**`src-tauri/src/lib.rs`**
- Register `commands::collections::update_request_docs` in `invoke_handler`.

---

### 5. Create `RequestDocsPanel` component

**`src/components/request/RequestDocsPanel.tsx`** — new file
- Props: `docs`, `mode`, `hasSource`, `onSave`.
- Local `text` state synced from `docs` prop.
- Edit mode: textarea + blur-save + footer with Save button. Shows read-only note when `!hasSource`.
- Preview mode: ReactMarkdown + empty state.

---

### 6. Wire into `RequestPanel`

**`src/components/request/RequestPanel.tsx`**
- Add `'docs'` to `SectionTab`.
- Add `docMode` state `'edit' | 'preview'` (default `'preview'`).
- Add Docs entry to `tabDefs` (dot indicator when `request.docs` is non-null).
- In `tabRightContent` memo: return `<Tabs>` Edit/Preview toggle when `activeSection === 'docs'`.
- Add `handleSaveDocs` callback (calls `updateRequestDocs`, then `updateRequest`).
- Add `{activeSection === 'docs' && <RequestDocsPanel .../>}` to section render block.

---

### 7. Verify

```bash
yarn tsc --noEmit
yarn lint
cargo check
```

---

### 8. Commit

Single commit: `feat: add per-request Docs tab (Phase 1)`
