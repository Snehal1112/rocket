# SP9-01 — Keyboard Shortcuts + Table Toast + effectiveAt Warning + ARIA

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **⚠️ Worktree** — all commands run inside `.worktrees/contract-enhancement` on branch `feat/contract-lock-enhancement`.

**Goal:**
1. Global `⌘L` shortcut opens ContractsTab for active collection.
2. `j/k/e/p/n/del` shortcuts scoped to the contracts tab with correct scope activation.
3. `j/k` moves real DOM focus; `Enter` then opens the card (via `forwardRef` + stable ID-keyed ref map).
4. Table-view toggle fires "coming soon" toast — single-edit, no console.info stub.
5. `effectiveAt` past-date warning wired correctly; warning state cleared on modal close.
6. `Escape` — confirm shadcn handles it; add hotkeys entry for explicit documentation.
7. ARIA final audit with machine-verifiable checks.

**Spec:** `Implementation_Plan_v2.md §7.7, §7.9, §12, §13`

**Depends on:** SP8-04 merged.

---

## Task 1: Discover hotkeys + toast API, then global `⌘L`

**Files:**
- Modify: existing global hotkeys registration file

- [ ] **Step 1: Find how `useHotkeys` is called in the project**

```bash
grep -rn "useHotkeys\|HotkeysProvider\|hotkeys-hook" \
  frontend/src --include="*.tsx" --include="*.ts" \
  | grep -v node_modules | grep -v ".test." | head -10
```

Read one call site fully. Note:
- exact import path (e.g. `'react-hotkeys-hook'`)
- whether `{ scopes }` is used — if yes, find `HotkeysProvider`
- whether `enabled` boolean option is used instead of scopes

```bash
# Find HotkeysProvider if scopes are used
grep -rn "HotkeysProvider\|initiallyActiveScopes" \
  frontend/src --include="*.tsx" | grep -v node_modules | head -5
```

- [ ] **Step 2: Find the active collection store selector**

```bash
grep -rn "activeCollectionId\|activeCollection[^s]\|selectedCollection" \
  frontend/src/stores --include="*.ts" | head -10
```

Read the line — note the store name and selector key.

- [ ] **Step 3: Find the global hotkeys registration file**

```bash
grep -rn "meta+k\|mod+k\|Meta+K\|globalShortcuts\|AppHotkeys\|KeyboardShortcuts" \
  frontend/src --include="*.tsx" --include="*.ts" \
  | grep -v node_modules | grep -v ".test." | head -10
```

Read the file. Note the existing hotkey entries pattern.

- [ ] **Step 4: Add `⌘L` / `Ctrl+L` global shortcut**

In the global hotkeys file, add (using the exact import and selector names found above):

```tsx
// Imports — use exact paths from Step 1 and Step 2:
import { usePaneStore } from '@/stores/pane-store'
import { useCollectionStore } from '@/stores/<actual-store>'  // from Step 2

// Inside the hook/component body:
const openContractTab      = usePaneStore(s => s.openContractTab)
const activeCollectionId   = useCollectionStore(s => s.<actualSelector>)
const activeCollectionName = useCollectionStore(s => s.<actualNameSelector> ?? s.<actualSelector>)

useHotkeys(
  'meta+l, ctrl+l',
  (e) => {
    e.preventDefault()
    if (activeCollectionId) {
      openContractTab(activeCollectionId, activeCollectionName)
    }
  },
  // If the project uses scopes, add the global scope:
  // { scopes: ['global'] }
  // If the project uses `enabled` option instead:
  // { enabled: true }
)
```

- [ ] **Step 5: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | grep "$(basename <global-hotkeys-file> .tsx)" | head -5
```

- [ ] **Step 6: Commit**

```bash
git add <global-hotkeys-file>
git commit -m "feat(contracts): global ⌘L shortcut — opens ContractsTab for active collection"
```

---

## Task 2: Scoped `j/k/e/p/n/del` + stable DOM focus via ID-keyed ref map + `forwardRef` on `ContractCard`

**Files:**
- Modify: `frontend/src/components/contracts/ContractCard.tsx`
- Modify: `frontend/src/components/contracts/ContractsTab.tsx`

- [ ] **Step 1: Add `forwardRef` to `ContractCard`**

Open `ContractCard.tsx`. Currently exports:

```tsx
export function ContractCard(...) { ... }
```

Replace with `forwardRef`. The full updated export (only the outer wrapper changes — all internal logic stays identical):

```tsx
import { forwardRef } from 'react'

export const ContractCard = forwardRef<HTMLElement, ContractCardProps>(
  function ContractCard(
    { contract, collectionRoot, collectionName, onAction, onOpen, focused, className },
    ref,
  ) {
    // ... all existing code unchanged ...

    return (
      <ContractContextMenu contract={contract} collectionRoot={collectionRoot} onAction={onAction}>
        <article
          ref={ref as React.Ref<HTMLElement>}   {/* ADD ref here */}
          role="article"
          aria-labelledby={`cc-name-${contract.id}`}
          {/* ... all other existing props unchanged ... */}
        >
          {/* ... all existing children unchanged ... */}
        </article>
      </ContractContextMenu>
    )
  },
)
```

- [ ] **Step 2: Add ID-keyed ref map + scope activation to `ContractsTab`**

Open `ContractsTab.tsx`. Find the import block and add `useRef`.

Replace the existing `focusedIdx` state + any cardRefs code with this stable pattern:

```tsx
const [focusedIdx, setFocusedIdx] = useState(-1)

// Stable ID-keyed ref map — never goes stale when list is filtered/reordered
const cardRefMap = useRef<Map<string, HTMLElement>>(new Map())

// Clear refs for cards no longer in the list when allCards changes
useEffect(() => {
  const currentIds = new Set(allCards.map(c => c.id))
  for (const id of cardRefMap.current.keys()) {
    if (!currentIds.has(id)) cardRefMap.current.delete(id)
  }
}, [allCards])

// When focusedIdx changes, move DOM focus to that card
useEffect(() => {
  const card = allCards[focusedIdx]
  if (card) cardRefMap.current.get(card.id)?.focus()
}, [focusedIdx, allCards])
```

Pass ref to every `ContractCard` render. Compute a single flat index per card:

```tsx
// Helper to get the ref callback for a given contract
function cardRef(id: string) {
  return (el: HTMLElement | null) => {
    if (el) cardRefMap.current.set(id, el)
    else cardRefMap.current.delete(id)
  }
}

// In the attention group render:
{attention.map((c, i) => (
  <ContractCard
    key={c.id}
    ref={cardRef(c.id)}
    contract={c}
    collectionRoot={collectionId}
    collectionName={collectionName}
    onAction={handleAction}
    focused={focusedIdx === i}
  />
))}

// In the active group render:
{active.map((c, i) => (
  <ContractCard
    key={c.id}
    ref={cardRef(c.id)}
    contract={c}
    collectionRoot={collectionId}
    collectionName={collectionName}
    onAction={handleAction}
    focused={focusedIdx === attention.length + i}
  />
))}

// In the inactive group render:
{inactive.map((c, i) => (
  <ContractCard
    key={c.id}
    ref={cardRef(c.id)}
    contract={c}
    collectionRoot={collectionId}
    collectionName={collectionName}
    onAction={handleAction}
    focused={focusedIdx === attention.length + active.length + i}
  />
))}
```

- [ ] **Step 3: Add scoped hotkeys to `ContractsTab`**

First understand the scope system from Task 1 Step 1. Then add the hotkeys block.

**If the project uses `{ scopes }` option with `HotkeysProvider`:**

```tsx
// Add at the top of ContractsTab.tsx:
import { useHotkeys } from '<actual-path>'  // from Task 1 Step 1

// The contracts tab must activate its scope when mounted and deactivate when unmounted.
// Find how other components do this:
//   grep -rn "enableScope\|activateScope\|useHotkeysContext" frontend/src --include="*.tsx" | head -5
// Then add the same pattern here.

// Inside the component body:
useHotkeys('j', () => setFocusedIdx(i => Math.min(i + 1, allCards.length - 1)), { scopes: ['contracts'] })
useHotkeys('k', () => setFocusedIdx(i => Math.max(0, i - 1)),                   { scopes: ['contracts'] })
useHotkeys('n', () => setModalOpen(true),                                         { scopes: ['contracts'] })
useHotkeys('e', () => { const c = allCards[focusedIdx]; if (c) handleAction('edit', c.id) },       { scopes: ['contracts'] })
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

**If the project uses `enabled` option instead of scopes:**

```tsx
// Track whether this tab is the active pane (find the active pane selector):
const isActivePane = usePaneStore(s => s.activeTabId === `contracts:${collectionId}`)

useHotkeys('j', () => setFocusedIdx(i => Math.min(i + 1, allCards.length - 1)), { enabled: isActivePane })
useHotkeys('k', () => setFocusedIdx(i => Math.max(0, i - 1)),                   { enabled: isActivePane })
useHotkeys('n', () => setModalOpen(true),                                         { enabled: isActivePane })
useHotkeys('e', () => { const c = allCards[focusedIdx]; if (c) handleAction('edit', c.id) },       { enabled: isActivePane })
useHotkeys('p', () => {
  const c = allCards[focusedIdx]
  if (!c) return
  handleAction(c.status === 'paused' ? 'resume' : 'pause', c.id)
}, { enabled: isActivePane })
useHotkeys(['delete', 'backspace'], () => {
  const c = allCards[focusedIdx]
  if (c) handleAction('delete', c.id)
}, { enabled: isActivePane })
```

**Document `Escape`:** shadcn `Dialog` and `ContextMenu` both handle `Escape` natively via Radix UI — no code needed. Add a code comment above the hotkeys block:

```tsx
// Escape: handled natively by shadcn Dialog (closes modal) and ContextMenu (dismisses).
// No useHotkeys entry needed.
```

- [ ] **Step 4: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | grep -E "ContractCard|ContractsTab" | head -10
```

Fix any ref-type errors. Common fix: `forwardRef<HTMLElement, ...>` → if TS complains about the `article` element, use `React.ElementRef<'article'>` which is `HTMLElement`.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/contracts/ContractCard.tsx
git add frontend/src/components/contracts/ContractsTab.tsx
git commit -m "feat(contracts): j/k/e/p/n/del scoped hotkeys; ID-keyed ref map for stable DOM focus; Enter opens card"
```

---

## Task 3: Table toast + effectiveAt warning (complete, single-edit) + ARIA audit

**Files:**
- Modify: `frontend/src/components/contracts/ContractsFilterBar.tsx`
- Modify: `frontend/src/components/contracts/NewContractModal.tsx`
- Verify: multiple contracts components

- [ ] **Step 1: Find toast API**

```bash
grep -rn "from 'sonner'\|from '@/components/ui/use-toast'\|from '@/components/ui/toast'" \
  frontend/src --include="*.tsx" | grep -v node_modules | grep "import" | head -5
```

Read one usage:

```bash
grep -B1 -A2 "toast\." $(grep -rl "toast\." frontend/src/components --include="*.tsx" | grep -v node_modules | head -1) | head -10
```

Note the exact call syntax. Use it in Steps 2 and 3.

- [ ] **Step 2: Table-view "coming soon" toast in `ContractsFilterBar.tsx`**

Add the toast import at the top of `ContractsFilterBar.tsx`.

Find the view toggle button. It currently has:

```tsx
onClick={() => onSetView(filterState.view === 'cards' ? 'table' : 'cards')}
```

Replace with the final version (no `console.info` stub — use the real toast call directly):

```tsx
onClick={() => {
  if (filterState.view === 'cards') {
    // Per spec §7.7: table view is out of scope this milestone
    // REPLACE the call below with the actual toast pattern found in Step 1.
    // Sonner:        toast.info('Table view coming soon')
    // shadcn toast:  toast({ title: 'Table view coming soon', variant: 'default' })
    /* USE ACTUAL CALL HERE — do not leave as a comment */
  } else {
    onSetView('cards')
  }
}}
```

After writing this code, immediately replace the comment with the real call. Do not commit with a `console.info` or comment placeholder.

- [ ] **Step 3: `effectiveAt` past-date warning in `NewContractModal.tsx`**

This adds a `warnings` state alongside the existing `errors` state. Every related change is in one place.

**3a. Add `warnings` state declaration** — after the existing `const [errors, setErrors]` line:

```tsx
const [warnings, setWarnings] = useState<Partial<Record<keyof FormState, string>>>({})
```

**3b. Add `checkWarnings` function** — add as a module-level pure function above the component:

```tsx
function checkWarnings(f: FormState): Partial<Record<keyof FormState, string>> {
  const w: Partial<Record<keyof FormState, string>> = {}
  if (f.effectiveAt) {
    const d = new Date(f.effectiveAt + 'T00:00:00')
    if (!isNaN(d.getTime()) && d < new Date()) {
      w.effectiveAt =
        'This date is in the past — the contract will take effect immediately'
    }
  }
  return w
}
```

**3c. Override `setField` for `effectiveAt`** — in the component body, after the generic `setField` function, add a specific override:

```tsx
function setEffectiveAt(e: React.ChangeEvent<HTMLInputElement>) {
  const value = e.target.value
  setForm(prev => ({ ...prev, effectiveAt: value }))
  setWarnings(checkWarnings({ ...form, effectiveAt: value }))
}
```

**3d. Wire `setEffectiveAt` into the `<Input>`** — find the effectiveAt input:

```tsx
<Input type="date" {...inputProps('effectiveAt')} />
```

Replace with:

```tsx
<Input
  id="nc-effectiveAt"
  type="date"
  value={form.effectiveAt}
  onChange={setEffectiveAt}           {/* uses specific handler, not generic setField */}
  aria-invalid={!!errors.effectiveAt}
  aria-describedby={
    errors.effectiveAt ? 'nc-effectiveAt-err' :
    warnings.effectiveAt ? 'nc-effectiveAt-warn' : undefined
  }
/>
{!errors.effectiveAt && warnings.effectiveAt && (
  <p
    id="nc-effectiveAt-warn"
    role="status"
    className="text-xs text-[hsl(var(--warning))] flex items-center gap-1"
  >
    <span aria-hidden="true">⚠</span>
    {warnings.effectiveAt}
  </p>
)}
```

**3e. Reset warnings in `resetAndClose`** — find the `resetAndClose` function:

```tsx
function resetAndClose() {
  setForm(INITIAL_STATE)
  setErrors({})
  setWarnings({})   // ADD THIS LINE
  setSaving(false)
  onOpenChange(false)
}
```

- [ ] **Step 4: ARIA machine-verifiable audit**

Run each check and fix anything that fails:

```bash
# Check 1: icon-only buttons missing aria-label in contracts components
grep -rn 'size="icon"' frontend/src/components/contracts/ --include="*.tsx" \
  | grep -v 'aria-label' | grep -v '.test.'
# Expected output: empty. If not, add aria-label to each result.

# Check 2: ContractCard has aria-labelledby
grep -n "aria-labelledby" frontend/src/components/contracts/ContractCard.tsx
# Expected: at least one match.

# Check 3: ContractStatusChip has role="status"
grep -n 'role="status"' frontend/src/components/contracts/ContractStatusChip.tsx
# Expected: at least one match.

# Check 4: pulseRed reduced-motion override
grep -n "prefers-reduced-motion" frontend/src/globals.css
# Expected: at least one match.

# Check 5: ContractCard article has tabIndex
grep -n "tabIndex" frontend/src/components/contracts/ContractCard.tsx
# Expected: tabIndex={0}

# Check 6: ContractCard article has onKeyDown Enter
grep -n "onKeyDown\|Enter" frontend/src/components/contracts/ContractCard.tsx
# Expected: line with both Enter and onOpen/onKeyDown
```

For each check that returns empty/missing — make the fix before committing.

- [ ] **Step 5: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/contracts/ContractsFilterBar.tsx
git add frontend/src/components/contracts/NewContractModal.tsx
git add frontend/src/globals.css  # if reduced-motion was added
git commit -m "fix(contracts): table-view toast; effectiveAt warning (wired+reset); ARIA audit"
```
