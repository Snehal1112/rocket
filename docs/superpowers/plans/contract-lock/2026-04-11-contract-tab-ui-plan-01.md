# Contract Tab UI — Plan 01: Type System + Display Components

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `ContractTab` to the pane type system, add `openContractTab` to the pane store, and build all self-contained display components that have no dependencies on each other.

**Architecture:** `ContractTab` joins the discriminated `Tab` union exactly like `GitTab`. Four purely presentational components are created: `ContractCard` (used in both list and live preview), `ContractEmptyState`, `ChangelogSummaryBar`, and `ChangelogTable`. None of these components call IPC directly — they receive data as props.

**Tech Stack:** React 18, TypeScript, Zustand, shadcn/ui (`Badge`, `Button`, `Separator`, `Table`), Lucide icons

**Spec:** `docs/superpowers/specs/2026-04-11-contract-tab-ui-design.md`

**Depends on:** Contract Lock Plans 01–04 merged. `useContractStore`, `Contract`, `ContractChangelog`, `ChangelogEntry` already exist in `src/stores/contract-store.ts` and `src/lib/tauri-api.ts`.

---

## File Map

| File | Action |
|---|---|
| `src/types/pane-types.ts` | Modify — add `ContractTab` interface + `isContractTab` guard |
| `src/stores/pane-store.ts` | Modify — add `openContractTab()` action |
| `src/components/contract/ContractCard.tsx` | Create — reusable contract card |
| `src/components/contract/ContractEmptyState.tsx` | Create — empty list state |
| `src/components/contract/ChangelogSummaryBar.tsx` | Create — 4 metric stat cards |
| `src/components/contract/ChangelogTable.tsx` | Create — sortable changelog rows |

---

## Task 1: Pane type + store action

**Files:**
- Modify: `src/types/pane-types.ts`
- Modify: `src/stores/pane-store.ts`

- [ ] **Step 1: Add `ContractTab` interface to `pane-types.ts`**

Open `src/types/pane-types.ts`. Locate the `GitTab` interface. Add the following block immediately after it:

```typescript
export interface ContractTab extends BaseTab {
  tabType: 'contract';
  collectionName: string;
  collectionRoot: string; // absolute path — required for all IPC calls
  initialScope?: import('@/lib/tauri-api').ContractScope;
}

export function isContractTab(tab: Tab): tab is ContractTab {
  return tab.tabType === 'contract';
}
```

Then update the `Tab` union type to include `ContractTab`. Find the current `Tab` type declaration and replace it with:

```typescript
export type Tab =
  | RequestTab
  | CollectionTab
  | WorkspaceTab
  | DiffTab
  | ConflictTab
  | GitTab
  | ContractTab;
```

- [ ] **Step 2: Add `openContractTab` to `pane-store.ts`**

Open `src/stores/pane-store.ts`. Find the store interface definition (the `interface` or `type` block that lists all store actions). Add this line to the interface:

```typescript
openContractTab: (collectionName: string, collectionRoot: string) => void;
```

Find the `create<...>(...)` implementation block. Add the following action implementation alongside the other `open*Tab` actions:

```typescript
openContractTab: (collectionName, collectionRoot) => {
  const id = `contract:${collectionName}`;
  const tab: ContractTab = {
    id,
    title: `Contracts — ${collectionName}`,
    tabType: 'contract',
    collectionName,
    collectionRoot,
    isDirty: false,
  };
  get().openTab(tab);
},
```

Make sure `ContractTab` is imported at the top of the file from `@/types/pane-types`.

- [ ] **Step 3: Verify TypeScript**

```bash
cd frontend && yarn tsc --noEmit
```

Expected: no errors. If you see "ContractTab not found", check the import at the top of `pane-store.ts`.

- [ ] **Step 4: Commit**

```bash
git add src/types/pane-types.ts src/stores/pane-store.ts
git commit -m "feat(contract-tab): add ContractTab pane type and openContractTab store action"
```

---

## Task 2: ContractCard + ContractEmptyState

**Files:**
- Create: `src/components/contract/ContractCard.tsx`
- Create: `src/components/contract/ContractEmptyState.tsx`

- [ ] **Step 1: Create `src/components/contract/ContractCard.tsx`**

This component is used in two contexts: the list view (where `preview=false`, action buttons are shown) and the live preview column (where `preview=true`, action buttons are hidden). The `Contract` type comes from `@/lib/tauri-api`.

```tsx
import { Trash2 } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Separator } from '@/components/ui/separator'
import { Contract } from '@/lib/tauri-api'
import { useContractStore } from '@/stores/contract-store'

interface ContractCardProps {
  contract: Contract
  collectionRoot: string
  preview?: boolean
  onViewChangelog?: () => void
  onEdit?: () => void
  onDelete?: () => void
}

export function ContractCard({
  contract,
  collectionRoot,
  preview = false,
  onViewChangelog,
  onEdit,
  onDelete,
}: ContractCardProps) {
  const contractStatus = useContractStore((s) => s.contractStatus)
  const changelogs = useContractStore((s) => s.changelogs)
  const status = contractStatus(contract)

  const statusVariant =
    status === 'expired' ? 'destructive' :
    status === 'expiring' ? 'warning' : 'default'

  const statusLabel =
    status === 'expired' ? 'Expired' :
    status === 'expiring' ? 'Expiring soon' : 'Active'

  const scopeLabel =
    contract.scope.type === 'collection'
      ? 'Entire collection'
      : contract.scope.type === 'folder'
      ? `Folder: ${(contract.scope as any).rel_path}`
      : `Request: ${(contract.scope as any).rel_path}`

  const changeCount = changelogs[contract.id]?.entries.length ?? 0

  return (
    <div
      className={[
        'rounded-lg border bg-card p-4 space-y-3 transition-colors',
        preview ? '' : 'hover:border-primary/40',
      ].join(' ')}
    >
      {/* Header row: title + status chip */}
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="text-sm font-medium text-foreground truncate">{contract.title}</p>
          <p className="text-xs text-muted-foreground mt-0.5">
            {contract.project}
            {contract.version ? ` · ${contract.version}` : ''}
          </p>
        </div>
        <Badge variant={statusVariant} className="shrink-0 text-xs">
          {statusLabel}
        </Badge>
      </div>

      {/* Parties: pill badges with coloured dots */}
      <div className="flex items-center gap-2 flex-wrap">
        <span className="inline-flex items-center gap-1.5 bg-secondary rounded-full px-2.5 py-1 text-xs">
          <span className="w-2 h-2 rounded-full bg-violet-500 shrink-0" />
          {contract.provider}
        </span>
        <span className="text-muted-foreground text-xs">→</span>
        <span className="inline-flex items-center gap-1.5 bg-secondary rounded-full px-2.5 py-1 text-xs">
          <span className="w-2 h-2 rounded-full bg-emerald-500 shrink-0" />
          {contract.consumer}
        </span>
      </div>

      {/* Date range */}
      <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
        <span>Effective {contract.effectiveDate}</span>
        {contract.expiryDate && <span>Expires {contract.expiryDate}</span>}
        {!contract.expiryDate && <span>No expiry</span>}
      </div>

      {/* Scope badge */}
      <div>
        <span className="inline-block text-xs bg-secondary text-muted-foreground rounded-full px-2.5 py-0.5">
          {scopeLabel}
        </span>
      </div>

      {/* Footer action row — hidden in preview mode */}
      {!preview && (
        <>
          <Separator />
          <div className="flex items-center justify-between">
            <span className="text-xs text-muted-foreground">
              {changeCount === 0
                ? 'No changes recorded'
                : `${changeCount} change${changeCount === 1 ? '' : 's'} logged`}
            </span>
            <div className="flex items-center gap-1">
              {onViewChangelog && (
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-7 text-xs"
                  onClick={onViewChangelog}
                >
                  View changelog
                </Button>
              )}
              {onEdit && (
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-7 text-xs"
                  onClick={onEdit}
                >
                  Edit
                </Button>
              )}
              {onDelete && (
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-7 w-7 p-0 text-muted-foreground hover:text-destructive"
                  onClick={onDelete}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              )}
            </div>
          </div>
        </>
      )}
    </div>
  )
}
```

- [ ] **Step 2: Create `src/components/contract/ContractEmptyState.tsx`**

```tsx
import { Lock, Plus } from 'lucide-react'
import { Button } from '@/components/ui/button'

interface ContractEmptyStateProps {
  onNew: () => void
}

export function ContractEmptyState({ onNew }: ContractEmptyStateProps) {
  return (
    <div className="flex flex-col items-center justify-center py-24 gap-4 text-center">
      <div className="w-12 h-12 rounded-xl bg-primary/8 flex items-center justify-center">
        <Lock className="h-6 w-6 text-primary/50" />
      </div>
      <div className="space-y-1.5 max-w-xs">
        <p className="text-sm font-medium text-foreground">No contracts yet</p>
        <p className="text-xs text-muted-foreground">
          Attach a contract to lock this collection's API signature and automatically
          track any changes made after signing.
        </p>
      </div>
      <Button size="sm" onClick={onNew}>
        <Plus className="h-3.5 w-3.5 mr-1.5" />
        New contract
      </Button>
    </div>
  )
}
```

- [ ] **Step 3: Verify TypeScript**

```bash
cd frontend && yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/contract/ContractCard.tsx
git add src/components/contract/ContractEmptyState.tsx
git commit -m "feat(contract-tab): ContractCard and ContractEmptyState components"
```

---

## Task 3: ChangelogSummaryBar + ChangelogTable

**Files:**
- Create: `src/components/contract/ChangelogSummaryBar.tsx`
- Create: `src/components/contract/ChangelogTable.tsx`

- [ ] **Step 1: Create `src/components/contract/ChangelogSummaryBar.tsx`**

Four stat cards: total changes, removed, added, changed. Uses Tailwind semantic colour classes that work in both light and dark mode.

```tsx
import { ContractChangelog } from '@/lib/tauri-api'

interface ChangelogSummaryBarProps {
  changelog: ContractChangelog
}

export function ChangelogSummaryBar({ changelog }: ChangelogSummaryBarProps) {
  const total   = changelog.entries.length
  const removed = changelog.entries.filter((e) => e.changeType === 'removed').length
  const added   = changelog.entries.filter((e) => e.changeType === 'added').length
  const changed = changelog.entries.filter((e) => e.changeType === 'changed').length

  const metrics = [
    {
      label: 'Total changes',
      value: total,
      valueClass: 'text-foreground',
    },
    {
      label: 'Removed',
      value: removed,
      valueClass: 'text-destructive',
    },
    {
      label: 'Added',
      value: added,
      valueClass: 'text-green-600 dark:text-green-400',
    },
    {
      label: 'Changed',
      value: changed,
      valueClass: 'text-blue-600 dark:text-blue-400',
    },
  ]

  return (
    <div className="grid grid-cols-4 gap-3 mb-6">
      {metrics.map((m) => (
        <div key={m.label} className="bg-secondary rounded-lg p-3">
          <p className="text-xs text-muted-foreground mb-1">{m.label}</p>
          <p className={`text-2xl font-medium tabular-nums ${m.valueClass}`}>
            {m.value}
          </p>
        </div>
      ))}
    </div>
  )
}
```

- [ ] **Step 2: Create `src/components/contract/ChangelogTable.tsx`**

Renders the full audit log. Uses shadcn `Table` components. Mono font for field names and values. Badge variants map to change types.

```tsx
import { Badge } from '@/components/ui/badge'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { ChangelogEntry } from '@/lib/tauri-api'

interface ChangelogTableProps {
  entries: ChangelogEntry[]
}

export function ChangelogTable({ entries }: ChangelogTableProps) {
  if (entries.length === 0) {
    return (
      <div className="flex items-center justify-center py-16 border rounded-lg">
        <p className="text-sm text-muted-foreground">
          No changes recorded since contract was signed.
        </p>
      </div>
    )
  }

  const badgeVariant = (changeType: ChangelogEntry['changeType']) => {
    if (changeType === 'removed') return 'destructive'
    if (changeType === 'added')   return 'default'
    return 'secondary'
  }

  return (
    <div className="border rounded-lg overflow-hidden">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead className="text-xs w-28">Date</TableHead>
            <TableHead className="text-xs">Field</TableHead>
            <TableHead className="text-xs w-24">Type</TableHead>
            <TableHead className="text-xs">Before</TableHead>
            <TableHead className="text-xs">After</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {entries.map((entry, i) => (
            <TableRow key={i}>
              {/* Date */}
              <TableCell className="text-xs text-muted-foreground whitespace-nowrap">
                {new Date(entry.timestamp).toLocaleDateString(undefined, {
                  month: 'short',
                  day:   'numeric',
                  year:  '2-digit',
                })}
              </TableCell>

              {/* Field name — mono */}
              <TableCell>
                <code className="text-xs font-mono bg-muted px-1.5 py-0.5 rounded">
                  {entry.field}
                </code>
              </TableCell>

              {/* Change type badge */}
              <TableCell>
                <Badge variant={badgeVariant(entry.changeType)} className="text-xs capitalize">
                  {entry.changeType}
                </Badge>
              </TableCell>

              {/* Old value */}
              <TableCell className="text-xs text-muted-foreground">
                {entry.oldValue
                  ? <code className="font-mono bg-muted px-1 rounded">{entry.oldValue}</code>
                  : <span>—</span>}
              </TableCell>

              {/* New value */}
              <TableCell className="text-xs">
                {entry.newValue
                  ? <code className="font-mono bg-muted px-1 rounded">{entry.newValue}</code>
                  : <span>—</span>}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  )
}
```

- [ ] **Step 3: Verify TypeScript**

```bash
cd frontend && yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/contract/ChangelogSummaryBar.tsx
git add src/components/contract/ChangelogTable.tsx
git commit -m "feat(contract-tab): ChangelogSummaryBar and ChangelogTable"
```

---

## Plan 01 complete

Verify the full plan before handing off to Plan 02:

```bash
cd frontend && yarn tsc --noEmit
```

Expected: zero type errors across all six modified/created files. Plan 02 (`ContractForm`, `ContractLivePreview`, `ContractTabTopBar`, `ContractTab`) depends on every file in this plan being present and compiling cleanly.
