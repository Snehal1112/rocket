# Plan 5 — UI Editors for Folder + Request Variables

> **For agentic workers:** Use `superpowers:subagent-driven-development`.
> Read `docs/superpowers/specs/variables-design.md` before starting.

**Depends on:** Plan 4  
**Spec:** `docs/superpowers/specs/variables-design.md`

Final plan — includes smoke test checklist for all 7 variable scopes.

**Goal:** Add `FolderVariablesPopover` (opened from sidebar folder menu) and `RequestVariablesPanel` (new Variables tab in request editor). End-to-end smoke test.

---

## File Map

| File | Change |
|---|---|
| `src/components/collections/FolderVariablesPopover.tsx` | New |
| `src/components/layout/CollectionsSidebar.tsx` | Add "Variables" to folder menu |
| `src/components/request/RequestVariablesPanel.tsx` | New |
| `src/components/request/RequestPanel.tsx` | Add Variables tab |

---

## Chunk 1: Folder variables editor

### Task 1: FolderVariablesPopover

- [ ] **Step 1: Create component**

```tsx
// Props
interface FolderVariablesPopoverProps {
  open: boolean
  onClose: () => void
  collection: string
  folderPath: string    // e.g. "auth" or "auth/oauth" — immediate parent only
  folderName: string
}
```

On open: call `getFolderChainVariables(collection, `${folderPath}/placeholder`)` to show inherited vars (read-only), and load this folder's own vars separately for editing.

Show two sections:
1. **This folder's variables** — editable table (key / value / enabled / delete)
2. **Inherited from parent folders** — read-only, greyed out, shows which folder each comes from

Save button calls `saveFolderVariables(collection, folderPath, vars)`.

All interactive elements must use shadcn/ui (`Dialog`, `Input`, `Switch`, `Button`).

- [ ] **Step 2: Wire into CollectionsSidebar**

Add to folder `DropdownMenuContent`:
```tsx
<DropdownMenuItem onSelect={() => { setFolderVarsTarget({collection, path, name}); setFolderVarsOpen(true); }}>
  <Variable className="h-3.5 w-3.5 mr-2" />
  Variables
</DropdownMenuItem>
```

- [ ] **Step 3: Commit**

```bash
git add src/components/collections/FolderVariablesPopover.tsx src/components/layout/CollectionsSidebar.tsx
git commit -m "feat: FolderVariablesPopover + sidebar wiring"
```

---

## Chunk 2: Request variables editor

### Task 2: RequestVariablesPanel + Variables tab

- [ ] **Step 1: Create RequestVariablesPanel**

```tsx
interface RequestVariablesPanelProps {
  collection: string
  requestPath: string
}
```

On mount: call `getRequestVariables(collection, requestPath)`.

Show:
- Description: "Request variables are available to this request only. They have higher priority than folder, environment, and collection variables."
- Variable table: key / value / enabled toggle / delete
- "Add variable" button
- Auto-save on blur OR explicit Save button (match collection variables tab UX)

- [ ] **Step 2: Add Variables tab to RequestPanel**

```tsx
<TabsTrigger value="variables">
  Variables
  {requestVarCount > 0 && (
    <span className="ml-1.5 text-[10px] bg-muted px-1 rounded">{requestVarCount}</span>
  )}
</TabsTrigger>

<TabsContent value="variables">
  {tab.source?.collection && tab.source?.path ? (
    <RequestVariablesPanel
      collection={tab.source.collection}
      requestPath={tab.source.path}
    />
  ) : (
    <p className="p-4 text-sm text-muted-foreground">
      Save this request to a collection before adding request variables.
    </p>
  )}
</TabsContent>
```

- [ ] **Step 3: Final smoke test**

```bash
cargo tauri dev
```

- [ ] Create env in collection A → verify file at `collections/A/environments/`
- [ ] Switch collection → env list changes
- [ ] Add folder variable at `auth/` → use `{{folderVar}}` in request inside `auth/oauth/` → verify it resolves
- [ ] Request at `auth/oauth/refresh.yml` with parent folder vars → chain merged correctly
- [ ] Set `initialValue` on collection var, leave `value` empty → resolves to `initialValue`
- [ ] Form-urlencoded body with `{{TOKEN}}` in a field value → field resolved, not whole body string
- [ ] Header `{{HEADER_NAME}}: {{HEADER_VALUE}}` → both key and value resolved
- [ ] URL overlay shows correct badge colour per scope; secret vars show `●●●●`
- [ ] Global env selection persists across collection switches

- [ ] **Step 4: Commit**

```bash
git add src/components/request/
git commit -m "feat: RequestVariablesPanel + Variables tab in request editor"
git commit -m "chore: variables system complete — all 7 scopes wired"
```
