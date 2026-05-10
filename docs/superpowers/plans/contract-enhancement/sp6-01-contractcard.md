# SP6-01 — ContractCard Sub-components + ContractCard

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **⚠️ Worktree** — all commands run inside `.worktrees/contract-enhancement` on branch `feat/contract-lock-enhancement`.

**Goal:** Build four internal sub-components (`VersionTag`, `StatusSubline`, `MetaItem`, `PrimaryAction`) then assemble `ContractCard` using the exact two-column grid from `Implementation_Plan_v2.md §7.2`.

**Architecture:** Sub-components live in `internal/` (not exported from the barrel). `ContractAction` union is defined in `ContractCard.tsx` and re-exported — `'resign'` is included as a valid action for drift/breach re-sign flow.

**Tech Stack:** React 18, TypeScript, shadcn/ui, Lucide React, date-fns

**Spec:** `Implementation_Plan_v2.md §7.2`

**Depends on:** SP5-02 merged.

---

## Task 1: `VersionTag.tsx` + `StatusSubline.tsx`

**Files:**
- Create: `frontend/src/components/contracts/internal/VersionTag.tsx`
- Create: `frontend/src/components/contracts/internal/StatusSubline.tsx`

- [ ] **Step 1: Create `VersionTag.tsx`**

```tsx
/** Mono bordered version badge shown inline with contract title. */
export function VersionTag({ version }: { version: string }) {
  return (
    <span className="font-mono text-xs border border-border rounded px-1.5 py-0.5 text-muted-foreground font-normal shrink-0">
      {version}
    </span>
  )
}
```

- [ ] **Step 2: Create `StatusSubline.tsx`**

```tsx
import type { Contract } from '@/types/contracts'

interface StatusSublineProps {
  contract: Pick<Contract, 'status' | 'driftCount' | 'breachCount'>
}

/**
 * Short explanatory text shown below the contract title.
 * Returns null for active/expiring statuses (no subline needed).
 */
export function StatusSubline({ contract }: StatusSublineProps) {
  const { status, driftCount, breachCount } = contract

  if (status === 'drift') {
    const breakingText = breachCount > 0 ? ` — ${breachCount} breaking` : ''
    return (
      <span className="text-[hsl(var(--warning))]">
        Drift detected{breakingText}
      </span>
    )
  }
  if (status === 'breach') {
    return (
      <span className="text-[hsl(var(--destructive))]">
        {breachCount} breaking change{breachCount !== 1 ? 's' : ''}
      </span>
    )
  }
  if (status === 'paused')    return <span className="text-muted-foreground">Monitoring paused</span>
  if (status === 'draft')     return <span className="text-muted-foreground">Not yet published</span>
  if (status === 'expired')   return <span className="text-muted-foreground">Contract expired</span>
  if (status === 'in_review') return <span className="text-primary">Awaiting consumer review</span>
  return null
}
```

- [ ] **Step 3: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | grep "internal/" | head -5
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/contracts/internal/VersionTag.tsx
git add frontend/src/components/contracts/internal/StatusSubline.tsx
git commit -m "feat(contracts): VersionTag + StatusSubline internal sub-components"
```

---

## Task 2: `MetaItem.tsx` + `PrimaryAction.tsx`

**Files:**
- Create: `frontend/src/components/contracts/internal/MetaItem.tsx`
- Create: `frontend/src/components/contracts/internal/PrimaryAction.tsx`

- [ ] **Step 1: Create `MetaItem.tsx`**

```tsx
import { cn } from '@/lib/utils'
import type { ReactNode } from 'react'

interface MetaItemProps {
  icon: ReactNode
  label?: string
  value: string
  danger?: boolean
  warning?: boolean
}

/**
 * Icon + optional label + value row used in the card meta section.
 * Colors default to muted-foreground; danger/warning override the value color.
 */
export function MetaItem({ icon, label, value, danger, warning }: MetaItemProps) {
  return (
    <div className={cn(
      'flex items-center gap-1 text-xs text-muted-foreground',
      danger  && 'text-[hsl(var(--destructive))]',
      warning && 'text-[hsl(var(--warning))]',
    )}>
      <span aria-hidden="true" className="shrink-0">{icon}</span>
      {label && <span className="shrink-0">{label}</span>}
      <span className={cn(
        'font-medium',
        !danger && !warning && 'text-foreground',
      )}>
        {value}
      </span>
    </div>
  )
}
```

- [ ] **Step 2: Create `PrimaryAction.tsx`**

Note: `'resign'` is included in `ContractAction` (defined in `ContractCard.tsx` in Task 3). This file imports that type.

```tsx
import { Button } from '@/components/ui/button'
import type { Contract } from '@/types/contracts'
import type { ContractAction } from '../ContractCard'

interface PrimaryActionProps {
  contract: Pick<Contract, 'id' | 'status'>
  onAction: (action: ContractAction, id: string) => void
}

/**
 * Context-sensitive primary CTA shown in the card footer on hover.
 * Returns null for active/in_review/expiring (no single primary action needed).
 */
export function PrimaryAction({ contract, onAction }: PrimaryActionProps) {
  const stop = (e: React.MouseEvent) => e.stopPropagation()

  switch (contract.status) {
    case 'drift':
    case 'breach':
      return (
        <Button variant="outline" size="sm" className="h-7 text-xs"
          onClick={e => { stop(e); onAction('resign', contract.id) }}>
          Re-sign
        </Button>
      )
    case 'draft':
      return (
        <Button variant="outline" size="sm" className="h-7 text-xs"
          onClick={e => { stop(e); onAction('publish', contract.id) }}>
          Publish
        </Button>
      )
    case 'paused':
      return (
        <Button variant="outline" size="sm" className="h-7 text-xs"
          onClick={e => { stop(e); onAction('resume', contract.id) }}>
          Resume
        </Button>
      )
    case 'expired':
      return (
        <Button variant="outline" size="sm" className="h-7 text-xs"
          onClick={e => { stop(e); onAction('renew', contract.id) }}>
          Renew
        </Button>
      )
    default:
      return null
  }
}
```

- [ ] **Step 3: Compile check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | grep "internal/" | head -5
```

`ContractCard.tsx` doesn't exist yet so `ContractAction` import will fail — that's expected. Full check is in Task 3.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/contracts/internal/MetaItem.tsx
git add frontend/src/components/contracts/internal/PrimaryAction.tsx
git commit -m "feat(contracts): MetaItem + PrimaryAction internal sub-components"
```

---

## Task 3: `ContractCard.tsx` — full implementation

**Files:**
- Create: `frontend/src/components/contracts/ContractCard.tsx`

- [ ] **Step 1: Create the component**

This is the complete, final file — no placeholders, every import explicit.

```tsx
import { Lock, Calendar, Clock, AlertTriangle, ArrowRight, MoreHorizontal } from 'lucide-react'
import { formatDistanceToNow, parseISO } from 'date-fns'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { ContractStatusChip } from './ContractStatusChip'
import { PartyPill } from './PartyPill'
import { ScopeTag } from './ScopeTag'
import { MiniChangelog } from './MiniChangelog'
import { VersionTag } from './internal/VersionTag'
import { StatusSubline } from './internal/StatusSubline'
import { MetaItem } from './internal/MetaItem'
import { PrimaryAction } from './internal/PrimaryAction'
import type { Contract } from '@/types/contracts'

// All actions a parent can receive from this card.
// 'resign' = re-sign a drifted/breached contract.
export type ContractAction =
  | 'open'
  | 'edit'
  | 'resign'
  | 'publish'
  | 'pause'
  | 'resume'
  | 'renew'
  | 'send_for_review'
  | 'approve'
  | 'reject'
  | 'duplicate'
  | 'export'
  | 'delete'
  | 'view_changelog'

interface ContractCardProps {
  contract: Contract
  collectionName?: string
  onAction: (action: ContractAction, contractId: string) => void
  onOpen?: (contractId: string) => void
  /** Passed by ContractsTab for j/k keyboard navigation highlight */
  focused?: boolean
  className?: string
}

// ─── Pure helpers ─────────────────────────────────────────

function formatDate(iso: string): string {
  try {
    return new Date(iso + 'T00:00:00').toLocaleDateString('en-US', {
      month: 'short', day: 'numeric', year: 'numeric',
    })
  } catch { return iso }
}

function isExpiringSoon(expiresAt: string | null): boolean {
  if (!expiresAt) return false
  const ms = new Date(expiresAt + 'T00:00:00').getTime() - Date.now()
  return ms > 0 && ms < 30 * 24 * 60 * 60 * 1000
}

function lastChangeLabel(updatedAt: string): string {
  try {
    return `Updated ${formatDistanceToNow(parseISO(updatedAt), { addSuffix: true })}`
  } catch { return 'Updated recently' }
}

function policyLabel(policy: Contract['policy']): string {
  return (
    { strict: 'Strict', lenient: 'Lenient', additive_ok: 'Additive OK' }[
      policy.breakingChangePolicy
    ] ?? policy.breakingChangePolicy
  )
}

// ─── Component ────────────────────────────────────────────

export function ContractCard({
  contract,
  collectionName,
  onAction,
  onOpen,
  focused,
  className,
}: ContractCardProps) {
  const statusCount =
    contract.breachCount > 0 ? contract.breachCount : contract.driftCount

  function stopPropagation(e: React.MouseEvent) {
    e.stopPropagation()
  }

  return (
    <article
      role="article"
      aria-labelledby={`cc-name-${contract.id}`}
      data-status={contract.status}
      tabIndex={0}
      onClick={() => onOpen?.(contract.id)}
      onKeyDown={e => { if (e.key === 'Enter') onOpen?.(contract.id) }}
      className={cn(
        // Base
        'group relative bg-card border border-border rounded-[var(--radius)]',
        'p-[18px_20px] grid grid-cols-1 md:grid-cols-[1fr_220px] gap-6 mb-[10px]',
        'cursor-pointer transition-[border-color,box-shadow] duration-[120ms]',
        'hover:border-[hsl(var(--border)/1.4)] hover:shadow-[0_1px_3px_rgba(0,0,0,0.06)]',
        'focus-visible:outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50',
        // Status modifiers (spec §7.2)
        contract.status === 'drift' &&
          'border-l-[3px] border-l-[hsl(var(--warning))] pl-[17px]',
        contract.status === 'breach' &&
          'border-l-[3px] border-l-[hsl(var(--destructive))] pl-[17px] bg-[color-mix(in_oklab,hsl(var(--destructive-soft))_25%,hsl(var(--card)))]',
        contract.status === 'paused' &&
          'bg-[color-mix(in_oklab,hsl(var(--muted))_50%,hsl(var(--card)))]',
        contract.status === 'expired' && 'opacity-75',
        // Keyboard focus highlight from parent
        focused && 'ring-2 ring-ring ring-offset-2 ring-offset-background',
        className,
      )}
    >
      {/* ─── Left column ─────────────────────────────────── */}
      <div className="min-w-0">

        {/* Title row */}
        <div className="flex items-start justify-between gap-3 mb-2.5">
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2 text-[15px] font-semibold tracking-tight">
              <Lock className="w-3.5 h-3.5 text-muted-foreground shrink-0" aria-hidden="true" />
              <span id={`cc-name-${contract.id}`} className="truncate">
                {contract.name}
              </span>
              <VersionTag version={contract.version} />
            </div>
            <div className="flex gap-1.5 items-center flex-wrap mt-1 text-xs text-muted-foreground">
              {collectionName && (
                <>
                  <span>{collectionName}</span>
                  <span className="w-1 h-1 rounded-full bg-muted-foreground/40" aria-hidden="true" />
                </>
              )}
              <span>{formatDate(contract.effectiveAt)}</span>
              <StatusSubline contract={contract} />
            </div>
          </div>
          <ContractStatusChip status={contract.status} count={statusCount} />
        </div>

        {/* Parties */}
        <div className="flex items-center gap-2 flex-wrap mb-3">
          <PartyPill party={contract.provider} role="provider" />
          <ArrowRight className="w-4 h-3.5 text-muted-foreground shrink-0" aria-hidden="true" />
          <PartyPill party={contract.consumers[0]} role="consumer" />
          {contract.consumers.length > 1 && (
            <Tooltip>
              <TooltipTrigger asChild>
                <span className="text-xs text-muted-foreground cursor-default select-none">
                  +{contract.consumers.length - 1} more
                </span>
              </TooltipTrigger>
              <TooltipContent side="top">
                {contract.consumers.slice(1).map(c => c.name).join(', ')}
              </TooltipContent>
            </Tooltip>
          )}
        </div>

        {/* Meta row */}
        <div className="flex gap-5 flex-wrap mb-3">
          <MetaItem
            icon={<Calendar className="h-3 w-3" />}
            label="Effective"
            value={formatDate(contract.effectiveAt)}
          />
          <MetaItem
            icon={<Clock className="h-3 w-3" />}
            label={contract.expiresAt ? 'Expires' : 'No expiry'}
            value={
              contract.expiresAt
                ? formatDate(contract.expiresAt)
                : '—'
            }
            warning={isExpiringSoon(contract.expiresAt)}
          />
          {contract.driftCount > 0 && (
            <MetaItem
              icon={<AlertTriangle className="h-3 w-3" />}
              value={`${contract.driftCount} change${contract.driftCount !== 1 ? 's' : ''}`}
              danger={contract.breachCount > 0}
              warning={contract.breachCount === 0}
            />
          )}
        </div>

        {/* Scope + meta tags */}
        <div className="flex gap-1.5 flex-wrap mb-3">
          <ScopeTag scope={contract.scope} />
          <ScopeTag type="endpoints" count={contract.endpointCount} />
          <ScopeTag type="policy" label={policyLabel(contract.policy)} />
          {contract.policy.uptimeSla !== null && (
            <ScopeTag type="sla" label={String(contract.policy.uptimeSla)} />
          )}
        </div>

        {/* Footer */}
        <div className="mt-1.5 pt-3 border-t border-dashed border-border flex justify-between items-center">
          <span className="text-[11px] text-muted-foreground">
            {lastChangeLabel(contract.updatedAt)}
          </span>
          {/* Action buttons — hidden until hover/focus */}
          <div className="flex gap-1 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity">
            <PrimaryAction contract={contract} onAction={onAction} />
            <Button
              variant="ghost"
              size="sm"
              className="h-7 text-xs"
              onClick={e => { stopPropagation(e); onAction('edit', contract.id) }}
              aria-label={`Edit ${contract.name}`}
            >
              Edit
            </Button>
            {/* MoreHorizontal is replaced by ContractContextMenu in SP6-02 */}
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              onClick={stopPropagation}
              aria-label="More actions"
              data-more-trigger
            >
              <MoreHorizontal className="h-3.5 w-3.5" aria-hidden="true" />
            </Button>
          </div>
        </div>
      </div>

      {/* ─── Right column — MiniChangelog ────────────────── */}
      <div className="hidden md:block">
        <MiniChangelog
          entries={contract.changelog}
          status={contract.status}
          onViewAll={() => onAction('view_changelog', contract.id)}
        />
      </div>
    </article>
  )
}
```

- [ ] **Step 2: Full TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | head -15
```

Expected: no errors (all imports now resolve since sub-components exist).

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/contracts/ContractCard.tsx
git commit -m "feat(contracts): ContractCard — two-column layout, 7 status modifiers, sub-components wired"
```
