# SP8-01 — Pane Type + EditorGroup + openContractTab + Tab Persistence + Collection Context Menu

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **⚠️ Worktree** — all commands run inside `.worktrees/contract-enhancement` on branch `feat/contract-lock-enhancement`.

**Goal:** Ensure `ContractTab` pane type has required fields, route it to `ContractsTab` in `EditorGroup`, add `openContractTab` to the pane store with persistence, and add "View contracts" to the collection row context menu.

**Architecture:** `openContractTab` is idempotent — if the tab is already open it activates it rather than duplicating. Tab persistence uses whatever mechanism the existing pane store already uses for other tab types (discover first, copy pattern).

**Spec:** `Implementation_Plan_v2.md §7.1, PR 6`

**Depends on:** SP7-02 merged.

---

## Task 1: `pane-types.ts` + `EditorGroup.tsx`

**Files:**
- Modify: `frontend/src/types/pane-types.ts`
- Modify: `frontend/src/components/panes/EditorGroup.tsx`

- [ ] **Step 1: Read current `pane-types.ts`**

```bash
cat frontend/src/types/pane-types.ts
```

Note: the existing union type name (e.g. `AnyTab`, `Tab`, `PaneTab`), and how existing tab types are structured. Also note whether a `tabType` discriminant is already used.

- [ ] **Step 2: Verify or add `ContractTab`**

The interface must have exactly these fields. If it already exists with different names, rename consistently throughout this task:

```typescript
export interface ContractTab {
  id: string
  tabType: 'contract'
  title: string
  /** ID used as key in useContractsStore byCollection map */
  collectionId: string
  collectionName: string
  isDirty: boolean
}
```

If `tabType` is named something else in the project (e.g. `type`, `kind`), use that. Keep it consistent.

- [ ] **Step 3: Add type guard**

```typescript
export function isContractTab(tab: AnyTab): tab is ContractTab {
  return (tab as ContractTab).tabType === 'contract'
}
```

Replace `AnyTab` with the actual union type name from Step 1.

- [ ] **Step 4: Add `ContractTab` to the union type**

Find the union type (e.g. `type AnyTab = RequestTab | CollectionTab | ...`) and add `| ContractTab`.

- [ ] **Step 5: Route in `EditorGroup.tsx`**

Find the content-rendering switch/if-chain. Add:

```tsx
import { ContractsTab } from '@/components/contracts/ContractsTab'
import { isContractTab } from '@/types/pane-types'

// In the render chain:
} else if (isContractTab(activeTab)) {
  content = (
    <ContractsTab
      collectionId={activeTab.collectionId}
      collectionName={activeTab.collectionName}
    />
  )
```

- [ ] **Step 6: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | grep -E "pane-types|EditorGroup" | head -10
```

- [ ] **Step 7: Commit**

```bash
git add frontend/src/types/pane-types.ts frontend/src/components/panes/EditorGroup.tsx
git commit -m "feat(contracts): ContractTab pane type + EditorGroup routing"
```

---

## Task 2: `openContractTab` in `pane-store.ts` with tab persistence

**Files:**
- Modify: `frontend/src/stores/pane-store.ts`

- [ ] **Step 1: Read existing open-tab patterns and persistence**

```bash
# See how other tabs are opened
grep -n "openTab\|openRequest\|openCollection\|persist\|_persist" \
  frontend/src/stores/pane-store.ts | head -30
```

Note: (a) how `openTab` is called with a tab object, (b) whether the store uses `persist` middleware, (c) what tab fields are serialised on reload.

- [ ] **Step 2: Find `findTab` / `activateTab` equivalents**

The implementation below assumes `openTab`, `findTab`, `activateTab` methods. Find their actual names:

```bash
grep -n "activateTab\|focusTab\|selectTab\|setActive\|findTab\|getTab" \
  frontend/src/stores/pane-store.ts | head -10
```

Note the actual method names and substitute below.

- [ ] **Step 3: Add `openContractTab` action**

Add to the store interface and implementation (adjust method names from Step 2):

```typescript
// In the store interface:
openContractTab: (collectionId: string, collectionName: string) => void

// In the store implementation:
openContractTab: (collectionId, collectionName) => {
  const tabId = `contracts:${collectionId}`

  // Activate if already open (idempotent)
  const existing = get().tabs?.find(t => t.id === tabId)
    ?? get().findTab?.(tabId)  // use whichever method exists
  if (existing) {
    get().activateTab(tabId)   // use whichever activation method exists
    return
  }

  const tab: import('@/types/pane-types').ContractTab = {
    id: tabId,
    tabType: 'contract',
    title: `Contracts — ${collectionName}`,
    collectionId,
    collectionName,
    isDirty: false,
  }
  get().openTab(tab)
},
```

- [ ] **Step 4: Verify persistence**

If the pane store uses `persist` middleware, `ContractTab` will be serialised automatically — nothing extra needed. Verify:

```bash
grep -n "persist\|storage\|serialize" frontend/src/stores/pane-store.ts | head -10
```

If the store already persists `tabs` via `persist({ name: 'pane-store', ... })`, the `ContractTab` will survive reloads as long as `isContractTab` is used to re-hydrate. If tabs are NOT persisted, document this in a TODO comment in the store — do not invent a new persistence mechanism.

- [ ] **Step 5: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | grep "pane-store" | head -5
```

- [ ] **Step 6: Commit**

```bash
git add frontend/src/stores/pane-store.ts
git commit -m "feat(contracts): openContractTab — idempotent, persisted via existing pane store"
```

---

## Task 3: "View contracts" in collection row context menu

**Files:**
- Modify: the collection context menu component (find below)

- [ ] **Step 1: Find the collection context menu**

```bash
# Find collection-level context menu
grep -rn "ContextMenu\|contextMenu\|context-menu\|rightClick\|onContextMenu" \
  frontend/src/components/sidebar/ --include="*.tsx" -l | head -5

# Also check for collection-specific menus
grep -rn "Rename.*collection\|Delete.*collection\|View.*collection\|collection.*menu" \
  frontend/src/components --include="*.tsx" -l | head -5
```

Read the identified file to understand its structure.

- [ ] **Step 2: Add "View contracts" menu item**

Find the menu items list. Add between the existing items (after "Open" or similar, before destructive actions):

```tsx
import { Lock } from 'lucide-react'
import { usePaneStore } from '@/stores/pane-store'

// Inside the collection context menu component:
const openContractTab = usePaneStore(s => s.openContractTab)

// In the menu JSX — adjust based on whether this uses shadcn ContextMenu or DropdownMenu:
<ContextMenuItem onSelect={() => openContractTab(collection.id, collection.name)}>
  <Lock className="h-3.5 w-3.5 mr-2" aria-hidden="true" />
  View contracts
</ContextMenuItem>
```

Position it logically (e.g., after "Open" if present, or before "Rename"). Add a separator if needed to visually group it.

- [ ] **Step 3: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 4: Commit**

```bash
git add -p  # stage only the collection menu file
git commit -m "feat(contracts): 'View contracts' item in collection context menu"
```
