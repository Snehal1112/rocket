# Scope Dropdown in New Contract Modal — Design Spec

**Date:** 2026-05-10  
**Status:** Approved  
**Scope:** Replace free-text scope path input with smart dropdowns in `NewContractModal`, and add backend unit tests that verify folder/request scope coverage in `on_request_saved`.

---

## Background

The New Contract modal's Scope field shows a radio group (Collection / Folder / Request). When Folder or Request is selected, a free-text `Input` appears for the user to type a relative path. This is error-prone — users must know the exact path. The legacy `ContractTab` already solved this with a `<Select>` dropdown populated from the live collection tree. This spec ports that solution to `NewContractModal` and adds backend tests that were missing for non-collection scopes.

---

## Section 1 — Shared Utility: `collectPaths`

**Create:** `src/lib/contracts/collectPaths.ts`

Extract the `collectPaths` function from `ContractTab.tsx` (currently duplicated there) into a shared module so both `ContractTab` and `NewContractModal` import from one place.

```typescript
import type { CollectionItem } from '@/lib/tauri-api';

export function collectPaths(
  items: CollectionItem[],
  prefix: string,
  folders: string[],
  requests: string[],
): void {
  for (const item of items) {
    if (item.type === 'folder') {
      const seg = item.dirName ?? item.name;
      const path = prefix ? `${prefix}/${seg}` : seg;
      folders.push(path);
      collectPaths(item.items, path, folders, requests);
    } else if (item.type === 'request' || item.type === 'summary') {
      const seg = item.fileName ?? item.name;
      const path = prefix ? `${prefix}/${seg}` : seg;
      requests.push(path);
    }
  }
}
```

Update `ContractTab.tsx` to import from this module instead of defining it inline.

---

## Section 2 — Frontend: `NewContractModal.tsx`

### New state

```typescript
const [folders, setFolders] = useState<string[]>([]);
const [requests, setRequests] = useState<string[]>([]);
```

### Data loading

Add a `useEffect` that fetches the collection tree when the modal opens. Mirrors the existing pattern in `ContractTab`:

```typescript
useEffect(() => {
  if (!open) return;
  getCollection(collectionName)
    .then((col) => {
      const f: string[] = [];
      const r: string[] = [];
      collectPaths(col.root.items, '', f, r);
      setFolders(f);
      setRequests(r);
    })
    .catch(() => {
      // Leave lists empty — dropdowns will show "No folders/requests found".
    });
}, [collectionName, open]);
```

Dep array `[collectionName, open]` ensures the lists refresh if the modal is opened for a different collection.

### Scope UI change

Replace the single `<Input id='nc-scopePath' ...>` block with separate `<Select>` dropdowns. Pattern is ported directly from `ContractForm.tsx` (lines 181–232):

- **Folder radio row**: when `form.scopeType === 'folder'`, show a `<Select>` listing all folder paths. `onValueChange` sets `form.scopePath`.
- **Request radio row**: when `form.scopeType === 'request'`, show a `<Select>` listing all request file paths. `onValueChange` sets `form.scopePath`.
- Both selects show a disabled "No folders/requests found" option when the list is empty.
- When the user switches scope type (`setScopeType`), `scopePath` resets to `''` (existing behaviour is unchanged).

No changes to:
- `FormState` shape — `scopePath: string` remains as-is
- `validate()` — path-required validation unchanged
- `ContractScope` construction in `submit()` — unchanged
- `adaptIpcContract` — unchanged

### Imports to add

```typescript
import { getCollection } from '@/lib/tauri-api';
import { collectPaths } from '@/lib/contracts/collectPaths';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
```

---

## Section 3 — Backend Tests: `on_request_saved` scope coverage

Add 4 tests to the `#[cfg(test)]` block in `crates/rocket-app/src/contract_service.rs`. All use the existing `MockContractRepo`, `MockCollectionRepo`, `make_snap`, and `make_contract` helpers.

### Test pattern (same for all 4)

1. Attach a contract with the target scope and a baseline snapshot via `attach_contract`
2. Construct a modified snapshot (e.g., method flipped `GET → POST`)
3. Call `svc.on_request_saved(root(), modified_snap)`
4. Load changelog, assert entry count

### Test table

| Test name | Scope | Modified request path | Assert |
|---|---|---|---|
| `folder_scope_logs_changes_inside_folder` | `Folder { rel_path: "auth" }` | `auth/login.yml` | `log.entries.len() == 1` |
| `folder_scope_ignores_requests_outside_folder` | `Folder { rel_path: "auth" }` | `payments/pay.yml` | `log.entries.is_empty()` |
| `request_scope_logs_changes_for_covered_request` | `Request { rel_path: "auth/login.yml" }` | `auth/login.yml` | `log.entries.len() == 1` |
| `request_scope_ignores_other_requests` | `Request { rel_path: "auth/login.yml" }` | `auth/register.yml` | `log.entries.is_empty()` |

For tests 1 and 3, the baseline snapshot must include the request being modified so a diff can be computed. Use `make_snap("auth/login.yml")` as the initial snapshot and pass it to `attach_contract`'s `initial_snapshots` arg.

For tests 2 and 4, no baseline snapshot is needed for the out-of-scope request (it won't be found in the snapshot, and the `covers()` guard short-circuits before any diff).

---

## Files Changed

| File | Change |
|---|---|
| `src/lib/contracts/collectPaths.ts` | **Create** — shared tree-walk utility |
| `src/components/contract/ContractTab.tsx` | **Modify** — import `collectPaths` instead of defining it inline |
| `src/components/contracts/NewContractModal.tsx` | **Modify** — add state, useEffect, replace Input with Select |
| `crates/rocket-app/src/contract_service.rs` | **Modify** — add 4 `on_request_saved` tests |

---

## What This Does NOT Change

- The `ContractScope` Rust type, persistence format, or IPC DTO — scope paths are still plain strings
- `NewContractModal` submit/validation logic
- The `ContractForm` component used by the legacy `ContractTab` — it already has the correct Select UI and is unchanged
