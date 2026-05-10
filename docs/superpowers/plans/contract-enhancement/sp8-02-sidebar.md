# SP8-02 — Sidebar Lock Pin + Request Contract Dot

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **⚠️ Worktree** — all commands run inside `.worktrees/contract-enhancement` on branch `feat/contract-lock-enhancement`.

**Goal:** Add the lock pin to collection rows with the `lockPinLabel` helper from spec §8.1; add the contract status dot to request rows, propagating `collectionId` as a prop if it isn't already available.

**Spec:** `Implementation_Plan_v2.md §8.1, §8.2`

**Depends on:** SP8-01 merged.

---

## Task 1: Lock pin on collection row

**Files:**
- Modify: collection row component (find first)

- [ ] **Step 1: Find the collection row component**

```bash
find frontend/src -name "CollectionItem*" -o -name "CollectionRow*" \
  -o -name "SidebarCollection*" 2>/dev/null | grep -v node_modules | grep -v ".test."
```

Read the top 50 lines to understand: prop names for the collection object, existing JSX structure, how the row is laid out.

```bash
head -50 <found-file>
```

Note the prop name that holds collection data (e.g. `collection`, `item`, `node`) and the field names for id/name.

- [ ] **Step 2: Add imports**

```tsx
import { useMemo } from 'react'
import { Lock, TriangleAlert, CircleAlert } from 'lucide-react'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { useContractsStore } from '@/store/contracts/contractsSlice'
import { usePaneStore } from '@/stores/pane-store'
```

- [ ] **Step 3: Add lock pin logic inside the component**

Adjust `collection.id` and `collection.name` to match the actual prop names from Step 1:

```tsx
const contractIds  = useContractsStore(s => s.byCollection[collection.id] ?? [])
const contractsById = useContractsStore(s => s.byId)
const openContractTab = usePaneStore(s => s.openContractTab)

const contractMeta = useMemo(() => {
  const contracts = contractIds.map(id => contractsById[id]).filter(Boolean)
  return {
    count:       contracts.length,
    driftCount:  contracts.filter(c => c.status === 'drift').length,
    breachCount: contracts.filter(c => c.status === 'breach').length,
  }
}, [contractIds, contractsById])
```

- [ ] **Step 4: Add `lockPinLabel` helper (pure function, above the component)**

Per spec §8.1 tooltip text:

```tsx
function lockPinLabel(meta: { count: number; driftCount: number; breachCount: number }): string {
  const parts: string[] = [`${meta.count} contract${meta.count !== 1 ? 's' : ''}`]
  if (meta.breachCount > 0) parts.push(`${meta.breachCount} breaching`)
  else if (meta.driftCount > 0) parts.push(`${meta.driftCount} drifting`)
  else parts.push('in compliance')
  return parts.join(' · ')
}
```

- [ ] **Step 5: Add lock pin JSX**

After the collection name span (inside the same flex row, using `ml-auto`):

```tsx
{contractMeta.count > 0 && (
  <Tooltip>
    <TooltipTrigger asChild>
      <button
        type="button"
        className="ml-auto flex items-center gap-[3px] text-[10px] font-semibold text-primary shrink-0 hover:opacity-80 transition-opacity"
        onClick={e => {
          e.stopPropagation()
          openContractTab(collection.id, collection.name)
        }}
        aria-label={lockPinLabel(contractMeta)}
      >
        <Lock className="w-[10px] h-[10px]" aria-hidden="true" />
        {contractMeta.count > 1 && (
          <span>{contractMeta.count}</span>
        )}
        {contractMeta.driftCount > 0 && (
          <TriangleAlert
            className="w-[10px] h-[10px] text-[hsl(var(--warning))]"
            aria-hidden="true"
          />
        )}
        {contractMeta.breachCount > 0 && (
          <CircleAlert
            className="w-[10px] h-[10px] text-[hsl(var(--destructive))]"
            aria-hidden="true"
          />
        )}
      </button>
    </TooltipTrigger>
    <TooltipContent side="right">
      {lockPinLabel(contractMeta)}
    </TooltipContent>
  </Tooltip>
)}
```

- [ ] **Step 6: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | grep "CollectionItem\|CollectionRow\|SidebarCollection" | head -5
```

- [ ] **Step 7: Commit**

```bash
git add <collection-row-file>
git commit -m "feat(contracts): sidebar lock pin with lockPinLabel — opens ContractsTab, drift/breach icons"
```

---

## Task 2: Contract status dot on request row

**Files:**
- Modify: `frontend/src/components/sidebar/RequestItem.tsx`

- [ ] **Step 1: Read `RequestItem.tsx` props interface**

```bash
head -60 frontend/src/components/sidebar/RequestItem.tsx
```

Note: whether `collectionId` is already a prop, what the request object field is called (`request`, `item`, `node`), and how `request.id` is accessed.

- [ ] **Step 2: Check all call sites of `RequestItem`**

```bash
grep -rn "RequestItem\|<RequestItem" frontend/src --include="*.tsx" | grep -v ".test." | head -15
```

If `collectionId` is not currently a prop, note every call site that needs to be updated.

- [ ] **Step 3: Add `collectionId` prop if missing**

If `collectionId` is not already available inside `RequestItem`, add it:

```tsx
// In the Props interface:
interface RequestItemProps {
  // ... existing props ...
  collectionId: string   // add this
}
```

Then update all call sites from Step 2 to pass `collectionId`. Each call site has access to the collection either from context, a parent prop, or the store. Pass the collection's ID accordingly.

- [ ] **Step 4: Add imports**

```tsx
import { useMemo } from 'react'
import { cn } from '@/lib/utils'
import { useContractsStore } from '@/store/contracts/contractsSlice'
```

- [ ] **Step 5: Add dot derivation logic**

Inside `RequestItem`, after the existing hooks:

```tsx
const contractIds   = useContractsStore(s => s.byCollection[collectionId] ?? [])
const contractsById = useContractsStore(s => s.byId)

/**
 * Highest-severity contract status covering this request.
 * Priority: breach > drift > compliant > undefined (no contract covers it).
 */
const contractDotStatus = useMemo(() => {
  let highestCompliant = false
  for (const id of contractIds) {
    const contract = contractsById[id]
    if (!contract) continue

    const covers =
      contract.scope.type === 'collection' ||
      (contract.scope.type === 'requests' &&
        contract.scope.requestIds.includes(request.id))

    if (!covers) continue

    if (contract.status === 'breach')  return 'breach'  as const
    if (contract.status === 'drift')   return 'drift'   as const
    if (contract.status === 'active' ||
        contract.status === 'expiring_in_30_days') {
      highestCompliant = true
    }
  }
  return highestCompliant ? 'compliant' as const : undefined
}, [contractIds, contractsById, request.id])
```

- [ ] **Step 6: Add dot JSX after the request name/method label**

```tsx
{contractDotStatus && (
  <span
    className={cn(
      'ml-auto w-[7px] h-[7px] rounded-full shrink-0 inline-block',
      contractDotStatus === 'breach'    && 'bg-[hsl(var(--destructive))] animate-pulse',
      contractDotStatus === 'drift'     && 'bg-[hsl(var(--warning))]',
      contractDotStatus === 'compliant' && 'bg-[hsl(var(--success)/0.8)]',
    )}
    aria-label={`Contract status: ${contractDotStatus}`}
  />
)}
```

- [ ] **Step 7: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | grep "RequestItem" | head -10
```

Fix any call sites that are missing the new `collectionId` prop.

- [ ] **Step 8: Commit**

```bash
git add frontend/src/components/sidebar/RequestItem.tsx
git add -p  # stage any call-site changes
git commit -m "feat(contracts): request row contract dot — compliant/drift/breach, collectionId prop propagated"
```
