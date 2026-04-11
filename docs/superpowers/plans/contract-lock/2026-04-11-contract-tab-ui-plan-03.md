# Contract Tab UI — Plan 03: Wiring + Cleanup

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire `ContractTab` into the editor routing system, add the sidebar context menu entry point, update `ContractBadge` to open the tab instead of a sheet, and delete the two now-redundant files (`AttachContractDialog.tsx` and `ContractPanel.tsx`).

**Architecture:** Three surgical modifications to existing files, each touching a well-isolated location. `EditorGroup` gains one routing branch. `CollectionsSidebar` gains one context menu item. `ContractBadge` replaces sheet-open logic with `openContractTab`. No new files are created in this plan.

**Tech Stack:** React 18, TypeScript, shadcn/ui (`ContextMenuItem`), Lucide icons

**Spec:** `docs/superpowers/specs/2026-04-11-contract-tab-ui-design.md`

**Depends on:** Plan 01 and Plan 02 complete and merged. The following must exist and compile cleanly before starting:
- `src/types/pane-types.ts` — `ContractTab` + `isContractTab`
- `src/stores/pane-store.ts` — `openContractTab`
- `src/components/contract/ContractTab.tsx` — default export `ContractTab`

---

## File Map

| File | Action |
|---|---|
| `src/components/panes/EditorGroup.tsx` | Modify — add `isContractTab` routing branch |
| `src/components/layout/CollectionsSidebar.tsx` | Modify — add "Manage contracts" context menu item |
| `src/components/contract/ContractBadge.tsx` | Modify — replace sheet-open with `openContractTab` |
| `src/components/contract/AttachContractDialog.tsx` | Delete — replaced by tab |
| `src/components/contract/ContractPanel.tsx` | Delete — replaced by tab |

---

## Task 1: Wire ContractTab into EditorGroup

**Files:**
- Modify: `src/components/panes/EditorGroup.tsx`

- [ ] **Step 1: Add imports to `EditorGroup.tsx`**

Open `src/components/panes/EditorGroup.tsx`. At the top of the file, alongside the other tab component imports (`GitTab`, `DiffViewer`, `RequestPanel`, etc.), add:

```typescript
import { ContractTab } from '@/components/contract/ContractTab'
import { isContractTab } from '@/types/pane-types'
```

- [ ] **Step 2: Add `isContractTab` routing branch**

Inside `EditorGroup`, find the tab content rendering block — the chain of ternary conditions that routes `activeTab` to the correct component. It will look something like:

```tsx
{activeTab ? (
  isRequestTab(activeTab) ? (
    <RequestPanel ... />
  ) : isGitTab(activeTab) ? (
    <GitTab ... />
  ) : isWorkspaceTab(activeTab) ? (
    ...
  ) : null
) : (
  <EmptyState />
)}
```

Add the `isContractTab` branch **before the final `null` fallback** and **after the `isGitTab` branch**:

```tsx
) : isContractTab(activeTab) ? (
  <ContractTab tab={activeTab} />
```

The full chain after the edit should read:

```tsx
{activeTab ? (
  isRequestTab(activeTab) ? (
    <RequestPanel tab={activeTab} groupId={node.groupId} />
  ) : isDiffTab(activeTab) ? (
    <DiffViewer diffState={activeTab.diffState} />
  ) : isConflictTab(activeTab) ? (
    <ConflictResolver conflictState={activeTab.conflictState} />
  ) : isGitTab(activeTab) ? (
    <GitTab tab={activeTab} />
  ) : isContractTab(activeTab) ? (
    <ContractTab tab={activeTab} />
  ) : isWorkspaceTab(activeTab) ? (
    <WorkspaceTab tab={activeTab} />
  ) : null
) : (
  <EmptyState />
)}
```

Note: your actual chain may differ slightly in ordering. Insert `isContractTab` before whichever branch is currently the last one before `null`. Do not change the order of existing branches.

- [ ] **Step 3: Verify TypeScript**

```bash
cd frontend && yarn tsc --noEmit
```

Expected: no errors. If you see "ContractTab is not assignable to type Tab", check that `ContractTab` was added to the `Tab` union in Plan 01.

- [ ] **Step 4: Commit**

```bash
git add src/components/panes/EditorGroup.tsx
git commit -m "feat(contract-tab): wire ContractTab into EditorGroup routing"
```

---

## Task 2: Sidebar context menu + ContractBadge update

**Files:**
- Modify: `src/components/layout/CollectionsSidebar.tsx`
- Modify: `src/components/contract/ContractBadge.tsx`

- [ ] **Step 1: Add "Manage contracts" to sidebar context menu**

Open `src/components/layout/CollectionsSidebar.tsx`.

Add these imports at the top of the file if not already present:

```typescript
import { Lock } from 'lucide-react'
import { usePaneStore } from '@/stores/pane-store'
```

Inside the component body, get the store action:

```typescript
const openContractTab = usePaneStore((s) => s.openContractTab)
```

Find the **collection row context menu** — the `<ContextMenu>` / `<ContextMenuContent>` block that wraps each collection row. Add a new `<ContextMenuItem>` entry. Place it logically near the Git item (after "Open Git" or before "Delete"):

```tsx
<ContextMenuItem
  onSelect={() => openContractTab(collection.name, collection.path)}
>
  <Lock className="h-3.5 w-3.5 mr-2 text-muted-foreground" />
  Manage contracts
</ContextMenuItem>
```

Note: Replace `collection.name` and `collection.path` with whatever variable names are used in that component to refer to the current collection's name and absolute filesystem path.

If the sidebar also has context menus on **folder rows** and **request rows**, add the same item to those context menus as well, passing the relevant collection name and root path.

- [ ] **Step 2: Update `ContractBadge.tsx` to open tab instead of sheet**

Open `src/components/contract/ContractBadge.tsx`. The current implementation opens a `ContractPanel` sheet on click. Replace it entirely with the version below, which opens the contract tab instead.

```tsx
import { Lock } from 'lucide-react'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { Contract } from '@/lib/tauri-api'
import { useContractStore } from '@/stores/contract-store'
import { usePaneStore } from '@/stores/pane-store'

interface ContractBadgeProps {
  contracts: Contract[]
  collectionName: string
  collectionRoot: string
}

export function ContractBadge({ contracts, collectionName, collectionRoot }: ContractBadgeProps) {
  const contractStatus   = useContractStore((s) => s.contractStatus)
  const openContractTab  = usePaneStore((s) => s.openContractTab)

  if (contracts.length === 0) return null

  const firstContract = contracts[0]
  const status = contractStatus(firstContract)

  const iconClass =
    status === 'expired'  ? 'text-destructive' :
    status === 'expiring' ? 'text-amber-500'   :
    'text-muted-foreground'

  const handleClick = (e: React.MouseEvent) => {
    e.stopPropagation()
    openContractTab(collectionName, collectionRoot)
  }

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          onClick={handleClick}
          className={[
            'inline-flex items-center justify-center h-4 w-4 rounded-sm',
            'hover:bg-accent transition-colors',
            iconClass,
          ].join(' ')}
          aria-label="Manage contracts"
        >
          <Lock className="h-3 w-3" />
        </button>
      </TooltipTrigger>
      <TooltipContent side="right">
        <p className="text-xs font-medium">{firstContract.title}</p>
        <p className="text-xs text-muted-foreground">
          {firstContract.provider} → {firstContract.consumer}
        </p>
      </TooltipContent>
    </Tooltip>
  )
}
```

Note: `ContractBadge` now requires a `collectionName` and `collectionRoot` prop in addition to `contracts`. Update every call site in `CollectionsSidebar.tsx` that renders `<ContractBadge>` to pass these two new props.

- [ ] **Step 3: Verify TypeScript**

```bash
cd frontend && yarn tsc --noEmit
```

Expected: no errors. If you see prop-mismatch errors on `ContractBadge`, check that all call sites in `CollectionsSidebar.tsx` were updated with the new props.

- [ ] **Step 4: Commit**

```bash
git add src/components/layout/CollectionsSidebar.tsx
git add src/components/contract/ContractBadge.tsx
git commit -m "feat(contract-tab): sidebar context menu entry + ContractBadge opens tab"
```

---

## Task 3: Delete old dialog and panel, smoke test

**Files:**
- Delete: `src/components/contract/AttachContractDialog.tsx`
- Delete: `src/components/contract/ContractPanel.tsx`

- [ ] **Step 1: Search for any remaining imports of the deleted files**

Before deleting, confirm nothing else imports them:

```bash
grep -r "AttachContractDialog" src/ --include="*.ts" --include="*.tsx"
grep -r "ContractPanel" src/ --include="*.ts" --include="*.tsx"
```

Expected: only the files themselves appear. If other files import them, update those imports first — they should now use the contract tab flow instead.

- [ ] **Step 2: Delete the files**

```bash
rm src/components/contract/AttachContractDialog.tsx
rm src/components/contract/ContractPanel.tsx
```

- [ ] **Step 3: Full TypeScript check**

```bash
cd frontend && yarn tsc --noEmit
```

Expected: zero errors. If you see errors referencing the deleted files, a consumer was missed in Step 1.

- [ ] **Step 4: Smoke test in the running app**

Start the app:

```bash
cargo tauri dev
```

Run through each scenario:

1. Right-click a collection in the sidebar → confirm "Manage contracts" appears in the context menu.
2. Click "Manage contracts" → confirm a tab opens titled "Contracts — {collection name}".
3. Click "New contract" inside the tab → confirm the two-column form appears (no dialog).
4. Type in the title field → confirm the live preview card updates in real time.
5. Fill all required fields → click "Create contract" → confirm you return to the list view and the new card appears.
6. Click "View changelog" on a card → confirm the changelog view opens inside the same tab.
7. Click "Back" → confirm return to the list view.
8. Click the lock badge on a collection in the sidebar → confirm it opens / focuses the same contract tab (stable id — no duplicate tab).
9. Switch to dark mode → confirm all text, cards, and form fields remain readable.
10. Confirm no "Attach contract" dialog or "Contract Panel" sheet appears anywhere.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(contract-tab): delete AttachContractDialog and ContractPanel — fully replaced by ContractTab"
```

---

## Plan 03 complete — full feature done

All three plans together deliver:

- `ContractTab` as a first-class pane tab (same pattern as `GitTab`)
- Three views inside the tab: list, create/edit, changelog
- Live preview while creating — no dialog required
- Context menu entry on every collection row
- Lock badge on sidebar items opens the tab directly
- Full light + dark theme compatibility via shadcn/ui CSS variables
- Old dialog and sheet completely removed
