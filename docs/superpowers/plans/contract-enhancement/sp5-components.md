# SP5-01 — Leaf Components: StatusChip, PartyPill, ScopeTag, ChangeChip, MiniChangelog, Skeleton

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **⚠️ Worktree** — all commands run inside `.worktrees/contract-enhancement` on branch `feat/contract-lock-enhancement`.

**Goal:** Build all leaf components. No business logic. Each renders in isolation with no Zustand dependency.

**Architecture:** Pure presentational components. `ChangeKind` values are `'add' | 'remove' | 'modify'` (from spec). Status chip label format: `"⚠ Drift · {n}"` for drift status.

**Tech Stack:** React 18, TypeScript, shadcn/ui, Lucide React, Vitest

**Spec:** `Implementation_Plan_v2.md §7.3, §7.4, §7.5`

**Depends on:** SP4-03 merged.

---

## Task 1: `ContractStatusChip` + `ChangeChip`

**Files:**
- Create: `frontend/src/components/contracts/ContractStatusChip.tsx`
- Create: `frontend/src/components/contracts/ChangeChip.tsx`

- [ ] **Step 1: Create `ContractStatusChip.tsx`**

```tsx
import { cn } from '@/lib/utils'
import { statusChipLabel } from '@/lib/contracts/statusMachine'
import type { ContractStatus } from '@/types/contracts'

interface ContractStatusChipProps {
  status: ContractStatus
  /** driftCount or breachCount — shown in label for drift/breach */
  count?: number
  className?: string
}

const chipVariants: Record<ContractStatus, string> = {
  active:      'bg-[hsl(var(--success)/0.14)] text-[hsl(var(--success))] border-[hsl(var(--success)/0.30)]',
  drift:       'bg-[hsl(var(--warning)/0.14)] text-[hsl(var(--warning))] border-[hsl(var(--warning)/0.30)]',
  breach:      'bg-[hsl(var(--destructive)/0.16)] text-[hsl(var(--destructive))] border-[hsl(var(--destructive)/0.34)] animate-pulse-red',
  in_review:   'bg-[hsl(var(--primary)/0.14)] text-primary border-[hsl(var(--primary)/0.30)]',
  draft:       'bg-muted text-muted-foreground border-border',
  paused:      'bg-muted text-foreground border-border border-2',
  expired:     'bg-[hsl(var(--muted-foreground)/0.18)] text-muted-foreground border-border border-2',
}

const dotVariants: Record<ContractStatus, string | null> = {
  active:    'bg-[hsl(var(--success))] animate-pulse',
  drift:     null,
  breach:    null,
  in_review: null,
  draft:     null,
  paused:    null,
  expired:   null,
}

export function ContractStatusChip({ status, count, className }: ContractStatusChipProps) {
  const label = statusChipLabel(status, count)
  const dot = dotVariants[status]

  return (
    <div
      role="status"
      className={cn(
        'inline-flex items-center gap-1.5 px-2 py-0.5 rounded-md border text-xs font-semibold transition-colors shrink-0',
        chipVariants[status],
        className,
      )}
    >
      {dot && <span className={cn('w-1.5 h-1.5 rounded-full shrink-0', dot)} aria-hidden="true" />}
      <span>{label}</span>
      <span className="sr-only">Status: {label}</span>
    </div>
  )
}
```

- [ ] **Step 2: Create `ChangeChip.tsx`**

```tsx
import { cn } from '@/lib/utils'
import type { ChangeKind } from '@/types/contracts'

interface ChangeChipProps {
  kind: ChangeKind
  className?: string
}

const styles: Record<ChangeKind, string> = {
  add:    'bg-[hsl(var(--success)/0.12)] text-[hsl(var(--success))] border-[hsl(var(--success)/0.25)]',
  remove: 'bg-[hsl(var(--destructive)/0.12)] text-[hsl(var(--destructive))] border-[hsl(var(--destructive)/0.25)]',
  modify: 'bg-[hsl(var(--warning)/0.12)] text-[hsl(var(--warning))] border-[hsl(var(--warning)/0.25)]',
}

const labels: Record<ChangeKind, string> = {
  add: '+add', remove: '−rem', modify: '~mod',
}

export function ChangeChip({ kind, className }: ChangeChipProps) {
  return (
    <span
      className={cn(
        'inline-flex items-center px-1.5 py-0.5 rounded-[3px] border text-[10px] font-mono font-semibold shrink-0',
        styles[kind],
        className,
      )}
    >
      {labels[kind]}
    </span>
  )
}
```

- [ ] **Step 3: Write component tests**

Create `frontend/src/components/contracts/ContractStatusChip.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { ContractStatusChip } from './ContractStatusChip'

describe('ContractStatusChip', () => {
  it('renders Active with dot', () => {
    render(<ContractStatusChip status="active" />)
    expect(screen.getByRole('status')).toBeInTheDocument()
    expect(screen.getByText('Active')).toBeInTheDocument()
  })

  it('renders Drift with count', () => {
    render(<ContractStatusChip status="drift" count={3} />)
    expect(screen.getByText('⚠ Drift · 3')).toBeInTheDocument()
  })

  it('renders Breaching', () => {
    render(<ContractStatusChip status="breach" />)
    expect(screen.getByText('Breaching')).toBeInTheDocument()
  })

  it('has sr-only text for each status', () => {
    const statuses = ['active', 'drift', 'breach', 'in_review', 'draft', 'paused', 'expired'] as const
    for (const status of statuses) {
      const { unmount } = render(<ContractStatusChip status={status} />)
      const srOnly = document.querySelector('.sr-only')
      expect(srOnly?.textContent).toMatch(/Status:/)
      unmount()
    }
  })
})
```

Run:
```bash
cd frontend && yarn vitest run src/components/contracts/ContractStatusChip.test.tsx 2>&1 | tail -5
```

Expected: `4 tests passed`.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/contracts/ContractStatusChip.tsx
git add frontend/src/components/contracts/ContractStatusChip.test.tsx
git add frontend/src/components/contracts/ChangeChip.tsx
git commit -m "feat(contracts): ContractStatusChip (7 variants + a11y) + ChangeChip + tests"
```

---

## Task 2: `PartyAvatar`, `PartyPill`, `ScopeTag`

**Files:**
- Create: `frontend/src/components/contracts/PartyAvatar.tsx`
- Create: `frontend/src/components/contracts/PartyPill.tsx`
- Create: `frontend/src/components/contracts/ScopeTag.tsx`

- [ ] **Step 1: Create `PartyAvatar.tsx`**

```tsx
import { avatarColorForName, initialsForName } from '@/lib/contracts/avatarColor'
import type { Party } from '@/types/contracts'

interface PartyAvatarProps {
  party: Party
  size?: number
}

export function PartyAvatar({ party, size = 20 }: PartyAvatarProps) {
  const bg = avatarColorForName(party.name, party.avatarColor)
  const initials = initialsForName(party.name)
  return (
    <span
      aria-hidden="true"
      style={{ width: size, height: size, backgroundColor: bg, fontSize: size * 0.4 }}
      className="rounded-full inline-flex items-center justify-center text-white font-semibold shrink-0 select-none"
    >
      {initials}
    </span>
  )
}
```

- [ ] **Step 2: Create `PartyPill.tsx`**

```tsx
import { cn } from '@/lib/utils'
import { PartyAvatar } from './PartyAvatar'
import type { Party, PartyRole } from '@/types/contracts'

interface PartyPillProps {
  party: Party
  role?: PartyRole
  className?: string
}

const roleLabels: Record<PartyRole, string> = { provider: 'Provider', consumer: 'Consumer' }

export function PartyPill({ party, role, className }: PartyPillProps) {
  return (
    <div
      className={cn(
        'inline-flex items-center gap-[7px] pl-[4px] pr-[10px] py-1',
        'border border-border rounded-full bg-card text-xs text-foreground',
        className,
      )}
    >
      <PartyAvatar party={party} size={20} />
      <span className="truncate max-w-[120px]">{party.name}</span>
      {role && (
        <span className="text-[10px] text-muted-foreground font-medium shrink-0">
          · {roleLabels[role]}
        </span>
      )}
    </div>
  )
}
```

- [ ] **Step 3: Create `ScopeTag.tsx`**

Per spec §7.5 — handles scope, endpoints, policy, SLA tags.

```tsx
import { cn } from '@/lib/utils'
import type { ContractScope } from '@/types/contracts'

type ScopeTagProps =
  | { scope: ContractScope; type?: never; count?: never; label?: never; className?: string }
  | { scope?: never; type: 'endpoints'; count: number; label?: never; className?: string }
  | { scope?: never; type: 'policy'; count?: never; label: string; className?: string }
  | { scope?: never; type: 'sla'; count?: never; label: string | null; className?: string }

export function ScopeTag(props: ScopeTagProps & { className?: string }) {
  const { className } = props
  let prefix: string
  let value: string

  if (props.scope !== undefined) {
    const s = props.scope
    if (s.type === 'collection') { prefix = 'Scope'; value = 'Entire collection' }
    else if (s.type === 'folder') { prefix = 'Folder'; value = s.path }
    else { prefix = 'Requests'; value = `${s.requestIds.length} selected` }
  } else if (props.type === 'endpoints') {
    prefix = 'Endpoints'; value = String(props.count)
  } else if (props.type === 'policy') {
    prefix = 'Policy'; value = props.label
  } else if (props.type === 'sla') {
    prefix = 'SLA'; value = props.label ? `${props.label}%` : '—'
  } else {
    return null
  }

  return (
    <span
      className={cn(
        'inline-flex items-center gap-[5px] px-2 py-0.5',
        'font-mono text-[11px] text-foreground',
        'bg-muted border border-border rounded-[4px]',
        className,
      )}
    >
      <span className="font-sans text-muted-foreground font-medium not-italic text-[10px]">{prefix}</span>
      {value}
    </span>
  )
}
```

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/contracts/PartyAvatar.tsx
git add frontend/src/components/contracts/PartyPill.tsx
git add frontend/src/components/contracts/ScopeTag.tsx
git commit -m "feat(contracts): PartyAvatar, PartyPill, ScopeTag leaf components"
```

---

## Task 3: `MiniChangelog` + `ContractCardSkeleton` + group/summary/empty/filter-bar components

**Files:**
- Create: `frontend/src/components/contracts/MiniChangelog.tsx`
- Create: `frontend/src/components/contracts/ContractCardSkeleton.tsx`
- Create: `frontend/src/components/contracts/ContractsGroupHeader.tsx`
- Create: `frontend/src/components/contracts/ContractsEmptyState.tsx`

- [ ] **Step 1: Create `MiniChangelog.tsx`**

Uses `entry.at` (ISO datetime) and `entry.summary` per spec §7.2 right column.

```tsx
import { formatDistanceToNow } from 'date-fns'
import { ChangeChip } from './ChangeChip'
import type { ChangelogEntry, ContractStatus } from '@/types/contracts'

interface MiniChangelogProps {
  entries: ChangelogEntry[]
  status: ContractStatus
  onViewAll?: () => void
}

function railLabel(status: ContractStatus): string {
  if (status === 'paused') return 'Paused state'
  if (status === 'draft') return 'Proposed shape'
  return 'Recent changes'
}

function timeAgo(iso: string): string {
  try {
    const dist = formatDistanceToNow(new Date(iso), { addSuffix: false })
    return dist
      .replace('about ', '').replace(' minutes', 'm').replace(' minute', 'm')
      .replace(' hours', 'h').replace(' hour', 'h')
      .replace(' days', 'd').replace(' day', 'd')
  } catch { return '—' }
}

export function MiniChangelog({ entries, status, onViewAll }: MiniChangelogProps) {
  const visible = entries.slice(0, 4)

  return (
    <div className="bg-card border border-border rounded-[calc(var(--radius)-2px)] p-3 flex flex-col gap-1 h-full">
      <div className="flex justify-between items-center mb-1">
        <span className="text-[10px] font-semibold text-muted-foreground uppercase tracking-wider">
          {railLabel(status)}
        </span>
        {entries.length > 0 && onViewAll && (
          <button onClick={onViewAll} className="text-[11px] text-primary hover:underline cursor-pointer">
            View all →
          </button>
        )}
      </div>

      {visible.map(entry => (
        <div key={entry.id} className="flex items-center gap-2 py-0.5">
          <span className="text-[11px] text-muted-foreground/70 w-11 shrink-0 tabular-nums">
            {timeAgo(entry.at)}
          </span>
          <ChangeChip kind={entry.kind} />
          <span className="text-[11px] text-muted-foreground truncate flex-1">
            <code className="font-mono text-[10px] bg-background px-1 rounded text-foreground">
              {entry.summary}
            </code>
          </span>
        </div>
      ))}

      {entries.length === 0 && (
        <p className="text-[11px] text-muted-foreground/60 italic py-1">No changes recorded</p>
      )}
    </div>
  )
}
```

- [ ] **Step 2: Create remaining tab-level components**

Create `frontend/src/components/contracts/ContractCardSkeleton.tsx`:

```tsx
import { Skeleton } from '@/components/ui/skeleton'

export function ContractCardSkeleton() {
  return (
    <div className="border border-border rounded-[var(--radius)] p-[18px_20px] grid grid-cols-[1fr_220px] gap-6 mb-[10px] animate-pulse">
      <div className="space-y-3">
        <div className="flex items-center gap-2">
          <Skeleton className="h-3.5 w-3.5 rounded" />
          <Skeleton className="h-4 w-48" />
          <Skeleton className="h-4 w-14 rounded" />
          <Skeleton className="ml-auto h-5 w-16 rounded-md" />
        </div>
        <div className="flex gap-2">
          <Skeleton className="h-7 w-32 rounded-full" />
          <Skeleton className="h-4 w-4 rounded" />
          <Skeleton className="h-7 w-32 rounded-full" />
        </div>
        <div className="flex gap-5">
          <Skeleton className="h-3 w-24" />
          <Skeleton className="h-3 w-20" />
        </div>
        <div className="flex gap-1.5">
          <Skeleton className="h-5 w-32 rounded-[4px]" />
          <Skeleton className="h-5 w-20 rounded-[4px]" />
          <Skeleton className="h-5 w-24 rounded-[4px]" />
        </div>
      </div>
      <Skeleton className="h-full rounded-[calc(var(--radius)-2px)]" />
    </div>
  )
}
```

Create `frontend/src/components/contracts/ContractsGroupHeader.tsx`:

```tsx
interface ContractsGroupHeaderProps {
  label: string
  count: number
}

export function ContractsGroupHeader({ label, count }: ContractsGroupHeaderProps) {
  return (
    <div className="flex items-center gap-2 px-1 py-2 mb-1 mt-4 first:mt-0">
      <span className="text-xs font-semibold text-muted-foreground uppercase tracking-[0.06em]">
        {label}
      </span>
      <span className="text-xs text-muted-foreground/60 tabular-nums">{count}</span>
      <div className="flex-1 h-[1px] bg-border/50 ml-1" />
    </div>
  )
}
```

Create `frontend/src/components/contracts/ContractsEmptyState.tsx`:

```tsx
import { Lock, Upload } from 'lucide-react'
import { Button } from '@/components/ui/button'

interface ContractsEmptyStateProps {
  onStartFromCurrent: () => void
}

export function ContractsEmptyState({ onStartFromCurrent }: ContractsEmptyStateProps) {
  return (
    <div className="flex flex-col items-center justify-center flex-1 py-16 px-6 text-center">
      <div
        className="w-[120px] h-[120px] rounded-full border-2 border-dashed border-primary/30 bg-[hsl(var(--primary)/0.06)] flex items-center justify-center mb-6"
        aria-hidden="true"
      >
        <Lock className="h-10 w-10 text-primary" />
      </div>
      <h2 className="text-[22px] font-semibold text-foreground mb-2">Lock the shape of this API</h2>
      <p className="text-sm text-muted-foreground max-w-[420px] mb-6 leading-relaxed">
        Pin endpoint signatures so your consumer team builds against a known shape.
        Rocket tracks every change after — you'll see breaking diffs before they ship.
      </p>
      <div className="flex flex-col items-center gap-3">
        <Button size="default" onClick={onStartFromCurrent}>
          <Lock className="h-4 w-4 mr-2" aria-hidden="true" />
          Start from current state
        </Button>
        <Button variant="outline" size="default" disabled>
          <Upload className="h-4 w-4 mr-2" aria-hidden="true" />
          Import OpenAPI…
        </Button>
        <p className="text-xs text-muted-foreground/60 mt-1">
          or snapshot only a folder / single request
        </p>
      </div>
    </div>
  )
}
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/contracts/MiniChangelog.tsx
git add frontend/src/components/contracts/ContractCardSkeleton.tsx
git add frontend/src/components/contracts/ContractsGroupHeader.tsx
git add frontend/src/components/contracts/ContractsEmptyState.tsx
git commit -m "feat(contracts): MiniChangelog (entry.at + entry.summary), Skeleton, GroupHeader, EmptyState"
```

---

# SP5-02 — ContractsSummaryRow + ContractsFilterBar + useContractsFilter

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **⚠️ Worktree** — all commands run inside `.worktrees/contract-enhancement` on branch `feat/contract-lock-enhancement`.

**Goal:** Build `ContractsSummaryRow` (with `+{add} · −{rem} · ~{mod}` breakdown trend), `ContractsFilterBar`, and `useContractsFilter` hook with component tests.

**Spec:** `Implementation_Plan_v2.md §7.6, §7.7`

**Depends on:** SP5-01 merged.

---

## Task 1: `ContractsSummaryRow` with breakdown trend

**Files:**
- Create: `frontend/src/components/contracts/ContractsSummaryRow.tsx`

- [ ] **Step 1: Create the component**

```tsx
import { cn } from '@/lib/utils'
import type { ContractCounts } from '@/types/contracts'

interface StatCardProps {
  label: string
  value: number
  trend: string
  warning?: boolean
  danger?: boolean
}

function StatCard({ label, value, trend, warning, danger }: StatCardProps) {
  return (
    <div className={cn(
      'flex-1 bg-card border border-border rounded-[calc(var(--radius)-2px)] p-3 flex flex-col gap-0.5',
      warning && 'border-[hsl(var(--warning)/0.4)]',
      danger  && 'border-[hsl(var(--destructive)/0.4)]',
    )}>
      <span className="text-[11px] font-medium text-muted-foreground uppercase tracking-[0.04em]">
        {label}
      </span>
      <span className={cn(
        'text-[22px] font-semibold tabular-nums tracking-tight',
        warning && 'text-[hsl(var(--warning))]',
        danger  && 'text-[hsl(var(--destructive))]',
      )}>
        {value}
      </span>
      <span className="text-[11px] text-muted-foreground">{trend}</span>
    </div>
  )
}

export function ContractsSummaryRow({ counts }: { counts: ContractCounts }) {
  const healthPct = counts.total > 0 ? Math.round((counts.active / counts.total) * 100) : 0

  // Breakdown for "Changes · 30d" trend: +{add} · −{rem} · ~{mod}
  const changeTrend = counts.totalChanges > 0
    ? `+${counts.changesAdded} · −${counts.changesRemoved} · ~${counts.changesModified}`
    : 'No changes'

  return (
    <div className="flex gap-2 px-6 pt-[14px] pb-3 flex-shrink-0">
      <StatCard label="Total"           value={counts.total}        trend={`${counts.draft} draft`} />
      <StatCard label="Active & healthy" value={counts.active}       trend={`${healthPct}% of contracts`} />
      <StatCard label="Drifting"         value={counts.drift}        trend="Needs review"       warning={counts.drift > 0} />
      <StatCard label="Breaching"        value={counts.breach}       trend="Consumer at risk"   danger={counts.breach > 0} />
      <StatCard label="Changes · 30d"    value={counts.totalChanges} trend={changeTrend} />
    </div>
  )
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/components/contracts/ContractsSummaryRow.tsx
git commit -m "feat(contracts): ContractsSummaryRow — +add/−rem/~mod breakdown trend line"
```

---

## Task 2: `useContractsFilter.ts` + tests

**Files:**
- Create: `frontend/src/hooks/useContractsFilter.ts`
- Create: `frontend/src/hooks/useContractsFilter.test.ts`

- [ ] **Step 1: Write failing tests**

```typescript
import { describe, it, expect } from 'vitest'
import { applyFilter } from './useContractsFilter'
import type { Contract, ContractsFilterState } from '@/types/contracts'

function c(id: string, status: Contract['status'], name = `Contract ${id}`): Contract {
  return {
    id, collectionId: 'col1', name, version: '1.0.0', status,
    provider: { id: 'p', name: 'Billing Team', kind: 'team' },
    consumers: [{ id: 'c', name: 'Platform', kind: 'team' }],
    scope: { type: 'collection' },
    policy: { breakingChangePolicy: 'lenient', noticeDays: 30, uptimeSla: null },
    effectiveAt: '2026-01-01', expiresAt: null,
    signedSnapshot: null, driftCount: 0, breachCount: 0, endpointCount: 1,
    changelog: [], createdBy: 'u1',
    createdAt: '2026-01-01T00:00:00Z', updatedAt: '2026-01-01T00:00:00Z',
  }
}

const contracts = [c('r1', 'active', 'Payments API'), c('r2', 'drift'), c('r3', 'breach'), c('r4', 'draft', 'Orders API')]

const base: ContractsFilterState = { search: '', statuses: ['all'], sort: 'updated', sortDir: 'desc', view: 'cards' }

describe('applyFilter', () => {
  it('all filter returns all contracts', () => {
    expect(applyFilter(contracts, base)).toHaveLength(4)
  })
  it('status filter returns only matching status', () => {
    const result = applyFilter(contracts, { ...base, statuses: ['drift'] })
    expect(result).toHaveLength(1)
    expect(result[0].id).toBe('r2')
  })
  it('multiple status chips are OR combined', () => {
    const result = applyFilter(contracts, { ...base, statuses: ['drift', 'breach'] })
    expect(result).toHaveLength(2)
  })
  it('search filters by name', () => {
    const result = applyFilter(contracts, { ...base, search: 'Payments' })
    expect(result).toHaveLength(1)
    expect(result[0].id).toBe('r1')
  })
  it('search filters by provider name', () => {
    const result = applyFilter(contracts, { ...base, search: 'Billing' })
    expect(result).toHaveLength(4) // all share same provider
  })
  it('empty search returns all', () => {
    expect(applyFilter(contracts, { ...base, search: '' })).toHaveLength(4)
  })
})
```

- [ ] **Step 2: Implement `useContractsFilter.ts`**

```typescript
import { useState, useMemo, useCallback } from 'react'
import type { Contract, ContractFilterStatus, ContractSortKey, ContractViewMode, ContractsFilterState } from '@/types/contracts'

const DEFAULT: ContractsFilterState = {
  search: '', statuses: ['all'], sort: 'updated', sortDir: 'desc', view: 'cards',
}

/** Pure function — exported for unit testing */
export function applyFilter(contracts: Contract[], state: ContractsFilterState): Contract[] {
  let result = contracts

  if (state.search.trim()) {
    const q = state.search.toLowerCase()
    result = result.filter(c =>
      c.name.toLowerCase().includes(q) ||
      c.provider.name.toLowerCase().includes(q) ||
      c.consumers.some(p => p.name.toLowerCase().includes(q)) ||
      c.version.toLowerCase().includes(q) ||
      (c.scope.type === 'folder' && c.scope.path.toLowerCase().includes(q))
    )
  }

  if (!state.statuses.includes('all')) {
    result = result.filter(c => state.statuses.includes(c.status as ContractFilterStatus))
  }

  return applySort(result, state.sort, state.sortDir)
}

function applySort(contracts: Contract[], sort: ContractSortKey, dir: 'asc' | 'desc'): Contract[] {
  return [...contracts].sort((a, b) => {
    let cmp = 0
    switch (sort) {
      case 'name':      cmp = a.name.localeCompare(b.name); break
      case 'effective': cmp = a.effectiveAt.localeCompare(b.effectiveAt); break
      case 'drift':     cmp = b.driftCount - a.driftCount; break
      default:          cmp = b.updatedAt.localeCompare(a.updatedAt); break
    }
    return dir === 'asc' ? -cmp : cmp
  })
}

export function useContractsFilter(contracts: Contract[]) {
  const [state, setState] = useState<ContractsFilterState>(DEFAULT)

  const filtered = useMemo(() => applyFilter(contracts, state), [contracts, state])

  const setSearch = useCallback((s: string) => setState(prev => ({ ...prev, search: s })), [])

  const toggleStatus = useCallback((status: ContractFilterStatus) => {
    setState(prev => {
      if (status === 'all') return { ...prev, statuses: ['all'] }
      const without = prev.statuses.filter(s => s !== 'all' && s !== status)
      const next = prev.statuses.includes(status) ? without : [...without, status]
      return { ...prev, statuses: next.length === 0 ? ['all'] : next }
    })
  }, [])

  const setSort = useCallback((sort: ContractSortKey) => setState(p => ({ ...p, sort })), [])
  const setView = useCallback((view: ContractViewMode) => setState(p => ({ ...p, view })), [])

  return { filtered, filterState: state, setSearch, toggleStatus, setSort, setView }
}
```

- [ ] **Step 3: Run tests**

```bash
cd frontend && yarn vitest run src/hooks/useContractsFilter.test.ts 2>&1 | tail -5
```

Expected: `6 tests passed`.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/hooks/useContractsFilter.ts
git add frontend/src/hooks/useContractsFilter.test.ts
git commit -m "feat(contracts): useContractsFilter — fuzzy search + status chips + sort + 6 tests"
```

---

## Task 3: `ContractsFilterBar.tsx`

**Files:**
- Create: `frontend/src/components/contracts/ContractsFilterBar.tsx`

- [ ] **Step 1: Create the component**

Per spec §7.7 — pill chips, debounced search, sort dropdown, view toggle.

```tsx
import { useEffect, useState } from 'react'
import { Search, LayoutGrid, List, ChevronDown } from 'lucide-react'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { cn } from '@/lib/utils'
import { statusLabel } from '@/lib/contracts/statusMachine'
import type { ContractFilterStatus, ContractSortKey, ContractViewMode, ContractsFilterState, ContractCounts } from '@/types/contracts'

const STATUS_CHIPS: ContractFilterStatus[] = ['all', 'active', 'drift', 'breach', 'draft', 'paused', 'expired']

const SORT_OPTIONS: Array<{ key: ContractSortKey; label: string }> = [
  { key: 'updated',   label: 'Last updated' },
  { key: 'name',      label: 'Name A→Z' },
  { key: 'effective', label: 'Effective date' },
  { key: 'drift',     label: 'Drift count' },
]

function getChipCount(status: ContractFilterStatus, counts: ContractCounts): number {
  if (status === 'all') return counts.total
  const map: Partial<Record<ContractFilterStatus, number>> = {
    active: counts.active, drift: counts.drift, breach: counts.breach,
    draft: counts.draft, paused: counts.paused, expired: counts.expired,
  }
  return map[status] ?? 0
}

interface ContractsFilterBarProps {
  filterState: ContractsFilterState
  counts: ContractCounts
  onSearch: (q: string) => void
  onToggleStatus: (s: ContractFilterStatus) => void
  onSetSort: (s: ContractSortKey) => void
  onSetView: (v: ContractViewMode) => void
}

export function ContractsFilterBar({
  filterState, counts, onSearch, onToggleStatus, onSetSort, onSetView,
}: ContractsFilterBarProps) {
  // Debounce search 200ms
  const [localSearch, setLocalSearch] = useState(filterState.search)
  useEffect(() => {
    const t = setTimeout(() => onSearch(localSearch), 200)
    return () => clearTimeout(t)
  }, [localSearch, onSearch])

  const sortLabel = SORT_OPTIONS.find(o => o.key === filterState.sort)?.label ?? 'Sort'

  return (
    <div className="flex items-center gap-2 px-6 py-3 border-b border-border flex-shrink-0 flex-wrap gap-y-2">
      {/* Search */}
      <div className="relative max-w-[340px] flex-1 min-w-[180px]">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground pointer-events-none" aria-hidden="true" />
        <Input
          placeholder="Search contracts…"
          value={localSearch}
          onChange={e => setLocalSearch(e.target.value)}
          className="pl-9 h-8 text-sm"
          aria-label="Search contracts"
        />
      </div>

      {/* Status chips */}
      <div className="flex items-center gap-1 flex-wrap">
        {STATUS_CHIPS.map(status => {
          const n = getChipCount(status, counts)
          if (status !== 'all' && n === 0) return null
          const isActive = filterState.statuses.includes(status)
          return (
            <button
              key={status}
              onClick={() => onToggleStatus(status)}
              className={cn(
                'inline-flex items-center gap-1 px-2.5 py-1 rounded-full text-[12px] font-medium border transition-colors',
                isActive
                  ? 'bg-[hsl(var(--primary)/0.12)] text-primary border-[hsl(var(--primary)/0.4)]'
                  : 'text-muted-foreground border-border hover:text-foreground hover:border-border/80',
              )}
            >
              {status === 'all' ? 'All' : statusLabel(status as any)}
              <span className={cn(
                'inline-flex items-center justify-center rounded-full px-1 min-w-[16px] text-[10px] tabular-nums',
                isActive ? 'bg-[hsl(var(--primary)/0.18)]' : 'bg-muted',
              )}>
                {n}
              </span>
            </button>
          )
        })}
      </div>

      <div className="ml-auto flex items-center gap-1">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="sm" className="h-8 text-xs gap-1.5 text-muted-foreground">
              Sort: {sortLabel} <ChevronDown className="h-3 w-3" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {SORT_OPTIONS.map(o => (
              <DropdownMenuItem
                key={o.key}
                onClick={() => onSetSort(o.key)}
                className={cn('text-sm', filterState.sort === o.key && 'font-medium text-primary')}
              >
                {o.label}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>

        <Button
          variant="ghost" size="icon" className="h-8 w-8"
          onClick={() => onSetView(filterState.view === 'cards' ? 'table' : 'cards')}
          aria-label={filterState.view === 'cards' ? 'Switch to table view' : 'Switch to card view'}
        >
          {filterState.view === 'cards'
            ? <List className="h-4 w-4" aria-hidden="true" />
            : <LayoutGrid className="h-4 w-4" aria-hidden="true" />
          }
        </Button>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/contracts/ContractsFilterBar.tsx
git commit -m "feat(contracts): ContractsFilterBar — debounced search, status chips, sort dropdown"
```
