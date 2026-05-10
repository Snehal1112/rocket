# SP8-04 — Frontend Export: tauri-api wrapper + Save Dialog + Context Menu Wire

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **⚠️ Worktree** — all commands run inside `.worktrees/contract-enhancement` on branch `feat/contract-lock-enhancement`.

**Goal:** Add `exportContractOpenapi` to `tauri-api.ts`; wire the "Export as OpenAPI" context menu action to fetch YAML from Rust then prompt the user to save via Tauri's save dialog.

**Architecture (Option B):** Rust generates YAML, returns it as a string. Frontend calls `@tauri-apps/plugin-dialog`'s `save` to get a path, then `@tauri-apps/plugin-fs`'s `writeTextFile` to write it. Falls back to browser `<a download>` if Tauri fs/dialog plugins are unavailable.

**Spec:** `Implementation_Plan_v2.md §7.10 "Export as OpenAPI"`

**Depends on:** SP8-03 merged.

---

## Task 1: `tauri-api.ts` wrapper + `saveOpenApiFile` utility

**Files:**
- Modify: `frontend/src/lib/tauri-api.ts`
- Create: `frontend/src/lib/contracts/exportOpenApi.ts`

- [ ] **Step 1: Verify Tauri plugin availability**

```bash
# Check if dialog/fs plugins are registered
grep -rn "tauri-plugin-dialog\|tauri-plugin-fs\|dialog::\|fs::" \
  src-tauri/Cargo.toml src-tauri/src/lib.rs | head -10

# Check frontend imports
grep -rn "@tauri-apps/plugin-dialog\|@tauri-apps/plugin-fs" \
  frontend/src --include="*.ts" --include="*.tsx" | grep -v node_modules | head -5
```

If the plugins are present → use path A (Tauri save dialog).
If absent → use path B (browser download link).

- [ ] **Step 2: Add wrapper to `tauri-api.ts`**

```typescript
// In tauri-api.ts — add with other contract functions:
export async function exportContractOpenapi(
  collectionRoot: string,
  contractId: string,
): Promise<string> {
  return invoke<string>('export_contract_openapi', { collectionRoot, contractId })
}
```

- [ ] **Step 3: Create `exportOpenApi.ts`**

```typescript
// frontend/src/lib/contracts/exportOpenApi.ts
import { exportContractOpenapi } from '@/lib/tauri-api'

/**
 * Fetches the OpenAPI YAML from Rust, then saves it to disk.
 *
 * Path A (Tauri plugins available): uses save dialog + writeTextFile
 * Path B (fallback): triggers browser <a download>
 */
export async function saveContractAsOpenApi(
  collectionRoot: string,
  contractId: string,
  contractName: string,
): Promise<void> {
  const yaml = await exportContractOpenapi(collectionRoot, contractId)
  const filename = `${contractName.replace(/\s+/g, '-').toLowerCase()}-openapi.yaml`

  // Path A: Tauri save dialog (preferred in desktop app)
  try {
    // Dynamic import so the bundle doesn't break if plugins aren't installed
    const { save } = await import('@tauri-apps/plugin-dialog')
    const { writeTextFile } = await import('@tauri-apps/plugin-fs')

    const savePath = await save({
      defaultPath: filename,
      filters: [{ name: 'YAML', extensions: ['yaml', 'yml'] }],
    })

    if (savePath) {
      await writeTextFile(savePath, yaml)
    }
    return
  } catch {
    // Plugins not available — fall through to browser download
  }

  // Path B: browser download link
  const blob = new Blob([yaml], { type: 'text/yaml' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  document.body.appendChild(a)
  a.click()
  document.body.removeChild(a)
  URL.revokeObjectURL(url)
}
```

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/tauri-api.ts
git add frontend/src/lib/contracts/exportOpenApi.ts
git commit -m "feat(contracts): exportContractOpenapi tauri-api wrapper + saveContractAsOpenApi util"
```

---

## Task 2: Wire "Export as OpenAPI" into `ContractContextMenu` + `ContractsTab`

**Files:**
- Modify: `frontend/src/components/contracts/ContractContextMenu.tsx`
- Modify: `frontend/src/components/contracts/ContractsTab.tsx`

- [ ] **Step 1: Update `ContractContextMenu` to call the export utility**

In `ContractContextMenu.tsx`, the "Export as OpenAPI" menu item currently calls `onAction('export', contract.id)`. Replace with a direct call:

Add import:

```tsx
import { saveContractAsOpenApi } from '@/lib/contracts/exportOpenApi'
```

Replace the existing "Export as OpenAPI" `ContextMenuItem`:

```tsx
<ContextMenuItem
  onSelect={async () => {
    try {
      // collectionRoot must be passed as a prop to ContractContextMenu
      await saveContractAsOpenApi(collectionRoot, contract.id, contract.name)
    } catch (err) {
      console.error('[ContractContextMenu] export failed:', err)
    }
  }}
>
  <FileDown className="h-3.5 w-3.5 mr-2" aria-hidden="true" />
  Export as OpenAPI
</ContextMenuItem>
```

`collectionRoot` must be added to `ContractContextMenuProps`:

```tsx
interface ContractContextMenuProps {
  contract: Contract
  collectionRoot: string   // ADD THIS
  onAction: (action: ContractAction, id: string) => void
  children: React.ReactNode
}
```

- [ ] **Step 2: Pass `collectionRoot` from `ContractCard`**

In `ContractCard.tsx`, `ContractContextMenu` is used in two places (wrapping the article and wrapping the MoreHorizontal button). Both need `collectionRoot`:

Add `collectionRoot` to `ContractCardProps`:

```tsx
interface ContractCardProps {
  contract: Contract
  collectionRoot: string   // ADD THIS
  collectionName?: string
  onAction: (action: ContractAction, contractId: string) => void
  onOpen?: (contractId: string) => void
  focused?: boolean
  className?: string
}
```

Pass it to both `ContractContextMenu` usages:

```tsx
<ContractContextMenu contract={contract} collectionRoot={collectionRoot} onAction={onAction}>
```

- [ ] **Step 3: Update `ContractsTab.tsx` to pass `collectionRoot` (which equals `collectionId` in Option B)**

In `ContractsTab.tsx`, `ContractCard` is rendered in three groups. Add `collectionRoot` prop to all three:

```tsx
// In all three map calls (attention, active, inactive):
<ContractCard
  key={c.id}
  contract={c}
  collectionRoot={collectionId}   // ADD THIS — collectionId IS the root path in Option B
  collectionName={collectionName}
  onAction={handleAction}
  focused={focusedIdx === i}
/>
```

- [ ] **Step 4: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | head -15
```

Fix all prop-missing errors.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/contracts/ContractContextMenu.tsx
git add frontend/src/components/contracts/ContractCard.tsx
git add frontend/src/components/contracts/ContractsTab.tsx
git commit -m "feat(contracts): Export as OpenAPI — save dialog via Tauri fs/dialog plugins, browser fallback"
```

---

# SP9-01 — Hotkeys, Enter Focus, Table Toast, effectiveAt Warning, Focus Trap

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **⚠️ Worktree** — all commands run inside `.worktrees/contract-enhancement` on branch `feat/contract-lock-enhancement`.

**Goal:** (1) Global `⌘L` hotkey opens ContractsTab. (2) `j/k` actually moves DOM focus to the card so `Enter` works. (3) Table-view toggle fires "coming soon" toast. (4) `effectiveAt` past-date warns (not blocks). (5) Focus trap in `NewContractModal` is verified.

**Spec:** `Implementation_Plan_v2.md §7.7, §7.9, §12, §13 PR 10`

**Depends on:** SP8-04 merged.

---

## Task 1: Global `⌘L` + `j/k` DOM focus + `Enter` on card

**Files:**
- Modify: existing global hotkeys file (find below)
- Modify: `frontend/src/components/contracts/ContractsTab.tsx`

- [ ] **Step 1: Find global hotkeys registration**

```bash
grep -rn "meta+k\|mod+k\|useHotkeys\|globalHotkeys\|KeyboardShortcuts" \
  frontend/src --include="*.tsx" --include="*.ts" \
  | grep -v node_modules | grep -v ".test." | head -10
```

Read the found file's top 30 lines. Note import path and call pattern.

- [ ] **Step 2: Find the active collection store selector**

```bash
grep -rn "activeCollectionId\|activeCollection\b\|selectedCollection" \
  frontend/src/stores --include="*.ts" | head -10
```

Note the store and selector names.

- [ ] **Step 3: Add `⌘L` to global hotkeys**

In the global hotkeys registration file, add:

```typescript
import { usePaneStore } from '@/stores/pane-store'
// Also import whichever store has active collection (found in Step 2)

// Inside the hook/component body:
const openContractTab      = usePaneStore(s => s.openContractTab)
const activeCollectionId   = /* use the selector from Step 2 */
const activeCollectionName = /* use the selector from Step 2 */

// Register ⌘L / Ctrl+L
useHotkeys(
  'meta+l, ctrl+l',
  () => {
    if (activeCollectionId) {
      openContractTab(activeCollectionId, activeCollectionName ?? activeCollectionId)
    }
  },
  { preventDefault: true },
)
```

- [ ] **Step 4: Update `ContractsTab` so `j/k` moves DOM focus**

`j/k` currently updates `focusedIdx` state but never calls `.focus()` on the card element. Fix this by adding a `refs` array and a `useEffect` that focuses the card when index changes.

In `ContractsTab.tsx`:

```tsx
import { useState, useEffect, useRef, useCallback } from 'react'

// Add inside the component:
const cardRefs = useRef<(HTMLElement | null)[]>([])

// Effect: when focusedIdx changes, focus the card element
useEffect(() => {
  if (focusedIdx >= 0 && cardRefs.current[focusedIdx]) {
    cardRefs.current[focusedIdx]!.focus()
  }
}, [focusedIdx])
```

Pass a `ref` callback to each `ContractCard`:

```tsx
// In all three map calls (attention, active, inactive):
<ContractCard
  key={c.id}
  contract={c}
  collectionRoot={collectionId}
  collectionName={collectionName}
  onAction={handleAction}
  focused={focusedIdx === globalIdx}
  ref={(el) => { cardRefs.current[globalIdx] = el }}  // ADD THIS
/>
```

Where `globalIdx` is the index across all three groups (same as the `focusedIdx === attention.length + i` pattern already used).

- [ ] **Step 5: Add `ref` forwarding to `ContractCard`**

In `ContractCard.tsx`, wrap with `React.forwardRef`:

```tsx
import { forwardRef } from 'react'

export const ContractCard = forwardRef<HTMLElement, ContractCardProps>(
  function ContractCard({ contract, collectionRoot, collectionName, onAction, onOpen, focused, className }, ref) {
    // ... existing code unchanged ...

    return (
      <ContractContextMenu contract={contract} collectionRoot={collectionRoot} onAction={onAction}>
        <article
          ref={ref as React.Ref<HTMLElement>}  // ADD THIS
          role="article"
          // ... all other props unchanged ...
        >
          {/* ... unchanged ... */}
        </article>
      </ContractContextMenu>
    )
  }
)
```

Now `Enter` works because: `j/k` → `focusedIdx` updates → `useEffect` calls `cardRefs.current[idx].focus()` → article has DOM focus → user presses `Enter` → `onKeyDown` fires `onOpen`.

- [ ] **Step 6: Uncomment hotkeys in `ContractsTab`**

The hotkeys block was left as comments in SP7-02. Now that `useHotkeys` import is known (from Step 1), uncomment and fill in the import. Replace the commented block with:

```tsx
// Add import at top of ContractsTab.tsx:
// import { useHotkeys } from '<actual-path>'  ← use path found in Task 1 Step 1

useHotkeys('j', () => setFocusedIdx(i => Math.min(i + 1, allCards.length - 1)), { scopes: ['contracts'] })
useHotkeys('k', () => setFocusedIdx(i => Math.max(0, i - 1)),                   { scopes: ['contracts'] })
useHotkeys('n', () => setModalOpen(true),                                         { scopes: ['contracts'] })
useHotkeys('e', () => { const c = allCards[focusedIdx]; if (c) handleAction('edit', c.id) },  { scopes: ['contracts'] })
useHotkeys('p', () => {
  const c = allCards[focusedIdx]
  if (!c) return
  handleAction(c.status === 'paused' ? 'resume' : 'pause', c.id)
}, { scopes: ['contracts'] })
useHotkeys(['delete', 'backspace'], () => {
  const c = allCards[focusedIdx]
  if (c) handleAction('delete', c.id)
}, { scopes: ['contracts'] })
```

- [ ] **Step 7: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | head -15
```

- [ ] **Step 8: Commit**

```bash
git add <global-hotkeys-file>
git add frontend/src/components/contracts/ContractsTab.tsx
git add frontend/src/components/contracts/ContractCard.tsx
git commit -m "feat(contracts): ⌘L global hotkey; j/k DOM focus via forwardRef; Enter opens focused card"
```

---

## Task 2: Table-view "coming soon" toast + `effectiveAt` past-date warning

**Files:**
- Modify: `frontend/src/components/contracts/ContractsFilterBar.tsx`
- Modify: `frontend/src/components/contracts/NewContractModal.tsx`

- [ ] **Step 1: Discover toast import (if not already known)**

If you already identified the toast pattern in SP7-01 Task 1, use it. Otherwise:

```bash
grep -rn "toast\|sonner\|useToast" frontend/src --include="*.tsx" --include="*.ts" \
  | grep -v node_modules | grep -v ".test." | head -5
```

- [ ] **Step 2: Add "coming soon" toast to `ContractsFilterBar`**

The view toggle in `ContractsFilterBar.tsx` currently calls `onSetView`. Per spec §7.7, clicking the table icon should show a "coming soon" toast instead of switching views.

Find the view toggle button in `ContractsFilterBar.tsx`:

```tsx
<Button
  variant="ghost" size="icon" className="h-8 w-8"
  onClick={() => onSetView(filterState.view === 'cards' ? 'table' : 'cards')}
  aria-label={...}
>
```

Replace its `onClick` with:

```tsx
onClick={() => {
  if (filterState.view === 'cards') {
    // Table view not yet implemented
    // REPLACE with actual toast call pattern from Step 1:
    // toast('Table view coming soon')
    // toast.info('Table view coming soon')
    // toast({ title: 'Table view coming soon' })
    console.info('[toast] Table view coming soon')
  } else {
    onSetView('cards')
  }
}}
```

After completing the `console.info` stub, replace it with the real toast call.

- [ ] **Step 3: Add `effectiveAt` past-date warning to `NewContractModal`**

Per spec §7.9: `effectiveAt: valid date, not in past (warn, not block)`.

In `NewContractModal.tsx`, find the `validate` function. It currently checks presence but not past-date. Add a warning (not an error — does not prevent submit):

Add to `FormState`:

```typescript
interface FormState {
  // ... existing fields ...
}

// Add a separate warnings state alongside errors:
const [warnings, setWarnings] = useState<Partial<Record<keyof FormState, string>>>({})
```

Add a `checkWarnings` function called on every render or on `effectiveAt` change:

```typescript
function checkWarnings(f: FormState): Partial<Record<keyof FormState, string>> {
  const w: Partial<Record<keyof FormState, string>> = {}
  if (f.effectiveAt) {
    const d = new Date(f.effectiveAt + 'T00:00:00')
    if (d < new Date()) {
      w.effectiveAt = 'This date is in the past — the contract will take effect immediately'
    }
  }
  return w
}
```

Update `setField` for `effectiveAt` to also recompute warnings:

```typescript
// Override the generic setField for effectiveAt to trigger warning:
function setEffectiveAt(e: React.ChangeEvent<HTMLInputElement>) {
  const value = e.target.value
  setForm(prev => ({ ...prev, effectiveAt: value }))
  setWarnings(checkWarnings({ ...form, effectiveAt: value }))
}
```

In the JSX, render the warning below the `effectiveAt` field (only if no error):

```tsx
{!errors.effectiveAt && warnings.effectiveAt && (
  <p className="text-xs text-[hsl(var(--warning))] flex items-center gap-1">
    <span aria-hidden="true">⚠</span>
    {warnings.effectiveAt}
  </p>
)}
```

- [ ] **Step 4: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | grep "ContractsFilterBar\|NewContractModal" | head -5
```

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/contracts/ContractsFilterBar.tsx
git add frontend/src/components/contracts/NewContractModal.tsx
git commit -m "feat(contracts): table-view 'coming soon' toast; effectiveAt past-date warning in modal"
```

---

## Task 3: Focus trap verification + ARIA final check

**Files:**
- Verify: `frontend/src/components/contracts/NewContractModal.tsx`
- Verify + fix: `frontend/src/components/contracts/ContractCard.tsx`
- Verify + fix: `frontend/src/components/contracts/ContractsFilterBar.tsx`

- [ ] **Step 1: Verify focus trap in `NewContractModal`**

shadcn `Dialog` traps focus by default via `@radix-ui/react-dialog`. No custom code needed. Verify it's not been disabled:

```bash
grep -n "FocusTrap\|trapFocus\|initialFocus\|returnFocus\|modal=" \
  frontend/src/components/contracts/NewContractModal.tsx
```

If `modal={false}` is set on `<DialogContent>`, remove it — that disables the trap.

- [ ] **Step 2: ARIA scan — icon-only buttons missing aria-label**

```bash
grep -rn 'size="icon"' frontend/src/components/contracts/ --include="*.tsx" \
  | grep -v 'aria-label' | grep -v '.test.'
```

For every result: add `aria-label="<descriptive action>"`. Common missing ones:
- View toggle in `ContractsFilterBar`: should have `aria-label={filterState.view === 'cards' ? 'Switch to table view' : 'Switch to card view'}`
- Any icon-only button in `ContractCard`

- [ ] **Step 3: Verify colour is never the only signal**

The spec says: "Drift/breach background color tints are never the only signal — icon + text always present."

- Drift card: has amber left border ✓, `StatusSubline` text "Drift detected — 2 breaking" ✓, `AlertTriangle` icon in meta row ✓
- Breach card: has red left border + bg tint, `ContractStatusChip` text "Breaching" ✓, `AlertTriangle` icon ✓

Run a quick visual check — open the hi-fi design file and compare. No code change needed if both text and icon are present.

- [ ] **Step 4: Verify `prefers-reduced-motion` for `pulseRed`**

```bash
grep -n "prefers-reduced-motion\|animate-pulse-red" frontend/src/globals.css
```

Expected output: both lines present (added in SP4-01). If either is missing, add:

```css
/* In globals.css, after @keyframes pulseRed: */
@media (prefers-reduced-motion: reduce) {
  .animate-pulse-red {
    animation: none;
    box-shadow: 0 0 0 2px hsl(var(--destructive) / 0.5);
  }
}
```

- [ ] **Step 5: Commit any fixes**

```bash
git add -p
git commit -m "fix(contracts): ARIA audit — icon labels, focus trap verified, reduced-motion fallback"
```
