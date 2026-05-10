# Scope Dropdown in New Contract Modal — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the free-text scope path input in `NewContractModal` with live dropdowns populated from the collection tree, and add Rust unit tests verifying folder/request scope coverage in `on_request_saved`.

**Architecture:** Extract the tree-walk utility already present in `ContractTab` into a shared module, then import it in both `ContractTab` and `NewContractModal`. `NewContractModal` gains two new state arrays (`folders`, `requests`) loaded via `getCollection` on modal open, and replaces the single `<Input>` with inline `<Select>` dropdowns inside the existing `<RadioGroup>`.

**Tech Stack:** React/TypeScript, shadcn/ui Select, Tauri `getCollection` IPC, Rust `#[test]` in `rocket-app`

---

## File Map

| File | Change |
|---|---|
| `src/lib/contracts/collectPaths.ts` | **Create** — shared recursive tree-walk utility |
| `src/components/contract/ContractTab.tsx` | **Modify** — replace inline function with import |
| `src/components/contracts/NewContractModal.tsx` | **Modify** — add state, useEffect, replace Input with Selects |
| `crates/rocket-app/src/contract_service.rs` | **Modify** — add 4 `on_request_saved` scope coverage tests |

---

## Task 1: Create `collectPaths` shared utility

**Files:**
- Create: `src/lib/contracts/collectPaths.ts`

- [ ] **Step 1: Create the file**

```typescript
// src/lib/contracts/collectPaths.ts
import type { CollectionItem } from '@/lib/tauri-api';

/**
 * Recursively walks a collection item tree and appends relative
 * folder paths and request file paths to the supplied arrays.
 * Call with `collectPaths(collection.root.items, '', folders, requests)`.
 */
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
    } else {
      const seg = item.fileName ?? item.name;
      const path = prefix ? `${prefix}/${seg}` : seg;
      requests.push(path);
    }
  }
}
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/lib/contracts/collectPaths.ts
git commit -m "refactor(contracts): extract collectPaths tree-walk to shared util"
```

---

## Task 2: Update `ContractTab` to use shared utility

**Files:**
- Modify: `src/components/contract/ContractTab.tsx`

- [ ] **Step 1: Add the import at the top of `ContractTab.tsx`**

Find the existing imports block (around line 11) and add:

```typescript
import { collectPaths } from '@/lib/contracts/collectPaths';
```

- [ ] **Step 2: Delete the inline `collectPaths` function**

Remove these lines (approximately lines 32–50):

```typescript
// Walk the collection item tree and collect relative folder/request paths.
function collectPaths(
  items: CollectionItem[],
  prefix: string,
  folders: string[],
  requests: string[],
) {
  for (const item of items) {
    if (item.type === 'folder') {
      const seg = item.dirName ?? item.name;
      const path = prefix ? `${prefix}/${seg}` : seg;
      folders.push(path);
      collectPaths(item.items, path, folders, requests);
    } else {
      const seg = item.fileName ?? item.name;
      const path = prefix ? `${prefix}/${seg}` : seg;
      requests.push(path);
    }
  }
}
```

Also remove the `CollectionItem` import from `@/lib/tauri-api` if it was only used by the deleted function. Check by searching for other `CollectionItem` usages in the file before removing.

- [ ] **Step 3: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/contract/ContractTab.tsx
git commit -m "refactor(contracts): ContractTab imports collectPaths from shared util"
```

---

## Task 3: Add scope dropdowns to `NewContractModal`

**Files:**
- Modify: `src/components/contracts/NewContractModal.tsx`

- [ ] **Step 1: Add new imports**

The current imports block starts at line 1. Add these imports (after the existing ones):

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

- [ ] **Step 2: Add `folders` and `requests` state**

Inside the `NewContractModal` component function, after the existing `useState` calls (around line 125), add:

```typescript
const [folders, setFolders] = useState<string[]>([]);
const [requests, setRequests] = useState<string[]>([]);
```

- [ ] **Step 3: Add `useEffect` to load collection tree**

After the existing `useEffect` (around line 138), add:

```typescript
// Load folder/request lists for the scope dropdowns.
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

- [ ] **Step 4: Replace the scope section JSX**

Find the scope section (around lines 358–390):

```tsx
          {/* ── Scope ────────────────────────── */}
          <div className='space-y-1.5'>
            <Label className='text-sm'>Scope</Label>
            <RadioGroup
              value={form.scopeType}
              onValueChange={(v) =>
                setForm((p) => ({ ...p, scopeType: v as FormState['scopeType'] }))
              }
              className='flex gap-4'
            >
              {(['collection', 'folder', 'request'] as const).map((s) => (
                <div key={s} className='flex items-center gap-1.5'>
                  <RadioGroupItem value={s} id={`scope-${s}`} />
                  <Label
                    htmlFor={`scope-${s}`}
                    className='text-sm font-normal cursor-pointer capitalize'
                  >
                    {s}
                  </Label>
                </div>
              ))}
            </RadioGroup>
            {form.scopeType !== 'collection' && (
              <Input
                id='nc-scopePath'
                value={form.scopePath}
                onChange={setField('scopePath')}
                className='mt-1.5 font-mono text-sm'
                placeholder={form.scopeType === 'folder' ? 'auth/' : 'requests/payments.yml'}
              />
            )}
          </div>
```

Replace it with:

```tsx
          {/* ── Scope ────────────────────────── */}
          <div className='space-y-1.5'>
            <Label className='text-sm'>Scope</Label>
            <RadioGroup
              value={form.scopeType}
              onValueChange={(v) =>
                setForm((p) => ({
                  ...p,
                  scopeType: v as FormState['scopeType'],
                  scopePath: '',
                }))
              }
              className='space-y-2'
            >
              {/* Collection */}
              <div className='flex items-center gap-2'>
                <RadioGroupItem value='collection' id='scope-collection' />
                <Label
                  htmlFor='scope-collection'
                  className='text-sm font-normal cursor-pointer'
                >
                  Collection
                </Label>
              </div>

              {/* Folder */}
              <div className='flex items-center gap-2 flex-wrap'>
                <RadioGroupItem value='folder' id='scope-folder' />
                <Label
                  htmlFor='scope-folder'
                  className='text-sm font-normal cursor-pointer'
                >
                  Folder
                </Label>
                {form.scopeType === 'folder' && (
                  <Select
                    value={form.scopePath}
                    onValueChange={(v) => setForm((p) => ({ ...p, scopePath: v }))}
                  >
                    <SelectTrigger className='h-8 text-sm w-48'>
                      <SelectValue placeholder='Select folder…' />
                    </SelectTrigger>
                    <SelectContent>
                      {folders.length === 0 && (
                        <SelectItem
                          value='__none__'
                          disabled
                          className='text-sm text-muted-foreground'
                        >
                          No folders found
                        </SelectItem>
                      )}
                      {folders.map((f) => (
                        <SelectItem key={f} value={f} className='text-sm font-mono'>
                          {f}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                )}
              </div>

              {/* Request */}
              <div className='flex items-center gap-2 flex-wrap'>
                <RadioGroupItem value='request' id='scope-request' />
                <Label
                  htmlFor='scope-request'
                  className='text-sm font-normal cursor-pointer'
                >
                  Request
                </Label>
                {form.scopeType === 'request' && (
                  <Select
                    value={form.scopePath}
                    onValueChange={(v) => setForm((p) => ({ ...p, scopePath: v }))}
                  >
                    <SelectTrigger className='h-8 text-sm w-52'>
                      <SelectValue placeholder='Select request…' />
                    </SelectTrigger>
                    <SelectContent>
                      {requests.length === 0 && (
                        <SelectItem
                          value='__none__'
                          disabled
                          className='text-sm text-muted-foreground'
                        >
                          No requests found
                        </SelectItem>
                      )}
                      {requests.map((r) => (
                        <SelectItem key={r} value={r} className='text-sm font-mono'>
                          {r}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                )}
              </div>
            </RadioGroup>
          </div>
```

- [ ] **Step 5: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 6: Run biome format**

```bash
yarn format
```

- [ ] **Step 7: Commit**

```bash
git add src/components/contracts/NewContractModal.tsx
git commit -m "feat(contracts): scope folder/request dropdowns populated from collection tree"
```

---

## Task 4: Rust tests — `on_request_saved` scope coverage

**Files:**
- Modify: `crates/rocket-app/src/contract_service.rs`

- [ ] **Step 1: Add 4 tests to the `#[cfg(test)]` block**

Find the closing `}` of the test module (the very last `}` at the end of the file, around line 1440). Add the following 4 tests immediately before that closing brace:

```rust
    #[test]
    fn folder_scope_logs_changes_inside_folder() {
        // Attach a contract scoped to the "auth" folder with a baseline snapshot
        // for auth/login.yml, then save a modified version of that request.
        // Expect: one changelog entry (method change).
        let svc = make_service();
        let snap = make_snap("auth/login.yml");
        let mut contract = make_contract();
        contract.scope = ContractScope::Folder { rel_path: PathBuf::from("auth") };
        let contract = svc
            .attach_contract(root(), contract, vec![snap.clone()], vec![])
            .unwrap();

        let mut changed = snap;
        changed.method = "POST".into();
        svc.on_request_saved(root(), changed).unwrap();

        let log = svc.get_changelog(root(), contract.id).unwrap();
        assert_eq!(
            log.entries.len(),
            1,
            "folder-scoped contract must log changes to requests inside the folder"
        );
        assert_eq!(log.entries[0].field, "method");
    }

    #[test]
    fn folder_scope_ignores_requests_outside_folder() {
        // Attach a contract scoped to the "auth" folder, then save a request
        // from a DIFFERENT folder. Expect: no changelog entries.
        let svc = make_service();
        let snap = make_snap("auth/login.yml");
        let mut contract = make_contract();
        contract.scope = ContractScope::Folder { rel_path: PathBuf::from("auth") };
        let contract = svc
            .attach_contract(root(), contract, vec![snap], vec![])
            .unwrap();

        // "payments/pay.yml" is outside the "auth" folder.
        let outside = make_snap_with_method("payments/pay.yml", "POST");
        svc.on_request_saved(root(), outside).unwrap();

        let log = svc.get_changelog(root(), contract.id).unwrap();
        assert!(
            log.entries.is_empty(),
            "folder-scoped contract must not log changes to requests outside the folder"
        );
    }

    #[test]
    fn request_scope_logs_changes_for_covered_request() {
        // Attach a contract scoped to the single request "auth/login.yml", then
        // save a modified version of that exact request.
        // Expect: one changelog entry (method change).
        let svc = make_service();
        let snap = make_snap("auth/login.yml");
        let mut contract = make_contract();
        contract.scope = ContractScope::Request { rel_path: PathBuf::from("auth/login.yml") };
        let contract = svc
            .attach_contract(root(), contract, vec![snap.clone()], vec![])
            .unwrap();

        let mut changed = snap;
        changed.method = "POST".into();
        svc.on_request_saved(root(), changed).unwrap();

        let log = svc.get_changelog(root(), contract.id).unwrap();
        assert_eq!(
            log.entries.len(),
            1,
            "request-scoped contract must log changes to the covered request"
        );
        assert_eq!(log.entries[0].field, "method");
    }

    #[test]
    fn request_scope_ignores_other_requests() {
        // Attach a contract scoped to "auth/login.yml", then save a DIFFERENT
        // request. Expect: no changelog entries.
        let svc = make_service();
        let snap = make_snap("auth/login.yml");
        let mut contract = make_contract();
        contract.scope = ContractScope::Request { rel_path: PathBuf::from("auth/login.yml") };
        let contract = svc
            .attach_contract(root(), contract, vec![snap], vec![])
            .unwrap();

        let other = make_snap_with_method("auth/register.yml", "POST");
        svc.on_request_saved(root(), other).unwrap();

        let log = svc.get_changelog(root(), contract.id).unwrap();
        assert!(
            log.entries.is_empty(),
            "request-scoped contract must not log changes to other requests"
        );
    }
```

- [ ] **Step 2: Run the 4 new tests**

```bash
cargo test -p rocket-app folder_scope request_scope 2>&1 | tail -15
```

Expected output:
```
running 4 tests
test contract_service::tests::folder_scope_ignores_requests_outside_folder ... ok
test contract_service::tests::folder_scope_logs_changes_inside_folder ... ok
test contract_service::tests::request_scope_ignores_other_requests ... ok
test contract_service::tests::request_scope_logs_changes_for_covered_request ... ok

test result: ok. 4 passed; 0 failed
```

- [ ] **Step 3: Run full rocket-app test suite to catch regressions**

```bash
cargo test -p rocket-app 2>&1 | tail -5
```

Expected: all tests pass (currently 110 pass).

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-app/src/contract_service.rs
git commit -m "test(contracts): add on_request_saved tests for folder and request scope coverage"
```

---

## Self-Review

- [x] **Spec coverage:** Section 1 (collectPaths utility) → Task 1. ContractTab update → Task 2. NewContractModal dropdowns → Task 3. Backend tests → Task 4.
- [x] **Placeholder scan:** All code is complete. No TBDs.
- [x] **Type consistency:** `collectPaths` signature is identical in Task 1 and used the same way in Task 2 (ContractTab) and Task 3 (NewContractModal). `ContractScope::Folder`/`::Request` constructors match what's defined in `rocket-collection`. `make_snap_with_method(path, method)` matches the existing helper at line 1119.
- [x] **scopePath reset:** `onValueChange` in Task 3 now resets `scopePath: ''` when switching scope type — prevents stale path from a previous selection leaking through.
