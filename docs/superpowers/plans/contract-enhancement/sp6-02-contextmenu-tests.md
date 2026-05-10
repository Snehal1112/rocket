# SP6-02 — ContractCard Tests + ContractContextMenu

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **⚠️ Worktree** — all commands run inside `.worktrees/contract-enhancement` on branch `feat/contract-lock-enhancement`.

**Goal:** Write `ContractCard.test.tsx` covering all 7 status states, then build `ContractContextMenu.tsx` and replace the `MoreHorizontal` stub in `ContractCard.tsx` with the real context-menu trigger.

**Architecture:** `ContractContextMenu` wraps any child as its `ContextMenuTrigger`. The `MoreHorizontal` button in the card footer is also used as the trigger for click-open (not just right-click), so both interactions work.

**Tech Stack:** React 18, TypeScript, shadcn/ui (`ContextMenu`, `AlertDialog`), Vitest + Testing Library

**Spec:** `Implementation_Plan_v2.md §7.2, §7.10`

**Depends on:** SP6-01 merged.

---

## Task 1: `ContractCard.test.tsx` — 8 test cases

**Files:**
- Create: `frontend/src/components/contracts/ContractCard.test.tsx`

- [ ] **Step 1: Write the tests**

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { ContractCard } from './ContractCard'
import type { Contract } from '@/types/contracts'

// Tooltip needs a provider — use a simple wrapper
import { TooltipProvider } from '@/components/ui/tooltip'

function wrap(ui: React.ReactElement) {
  return render(<TooltipProvider>{ui}</TooltipProvider>)
}

function makeContract(
  status: Contract['status'],
  overrides: Partial<Contract> = {},
): Contract {
  return {
    id: 'c-test',
    collectionId: 'col1',
    name: 'Payments API',
    version: '1.2.0',
    status,
    provider: { id: 'billing', name: 'Billing Team', kind: 'team' },
    consumers: [{ id: 'platform', name: 'Platform', kind: 'team' }],
    scope: { type: 'collection' },
    policy: { breakingChangePolicy: 'lenient', noticeDays: 30, uptimeSla: null },
    effectiveAt: '2026-01-15',
    expiresAt: null,
    signedSnapshot: null,
    driftCount: 0,
    breachCount: 0,
    endpointCount: 5,
    changelog: [],
    createdBy: 'user1',
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    ...overrides,
  }
}

describe('ContractCard', () => {
  it('renders contract name', () => {
    wrap(<ContractCard contract={makeContract('active')} onAction={vi.fn()} />)
    expect(screen.getByText('Payments API')).toBeInTheDocument()
  })

  it('renders version tag', () => {
    wrap(<ContractCard contract={makeContract('active')} onAction={vi.fn()} />)
    expect(screen.getByText('1.2.0')).toBeInTheDocument()
  })

  it('renders all 7 status variants without throwing', () => {
    const statuses: Contract['status'][] = [
      'active', 'drift', 'breach', 'in_review', 'draft', 'paused', 'expired',
    ]
    for (const status of statuses) {
      const { unmount } = wrap(
        <ContractCard contract={makeContract(status)} onAction={vi.fn()} />,
      )
      unmount()
    }
  })

  it('shows "Drift detected" StatusSubline for drift status', () => {
    const c = makeContract('drift', { driftCount: 3, breachCount: 1 })
    wrap(<ContractCard contract={c} onAction={vi.fn()} />)
    expect(screen.getByText(/Drift detected/)).toBeInTheDocument()
  })

  it('shows "Monitoring paused" for paused status', () => {
    wrap(<ContractCard contract={makeContract('paused')} onAction={vi.fn()} />)
    expect(screen.getByText('Monitoring paused')).toBeInTheDocument()
  })

  it('calls onOpen when article is clicked', () => {
    const onOpen = vi.fn()
    wrap(
      <ContractCard contract={makeContract('active')} onAction={vi.fn()} onOpen={onOpen} />,
    )
    fireEvent.click(screen.getByRole('article'))
    expect(onOpen).toHaveBeenCalledWith('c-test')
  })

  it('has aria-labelledby pointing to contract name span', () => {
    wrap(<ContractCard contract={makeContract('active')} onAction={vi.fn()} />)
    const article = screen.getByRole('article')
    expect(article).toHaveAttribute('aria-labelledby', 'cc-name-c-test')
    expect(document.getElementById('cc-name-c-test')?.textContent).toBe('Payments API')
  })

  it('shows +N more tooltip when >1 consumer', () => {
    const c = makeContract('active', {
      consumers: [
        { id: 'c1', name: 'Platform', kind: 'team' },
        { id: 'c2', name: 'Mobile', kind: 'team' },
        { id: 'c3', name: 'Web', kind: 'team' },
      ],
    })
    wrap(<ContractCard contract={c} onAction={vi.fn()} />)
    expect(screen.getByText('+2 more')).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: Run tests**

```bash
cd frontend && yarn vitest run src/components/contracts/ContractCard.test.tsx 2>&1 | tail -8
```

Expected: `8 tests passed`.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/contracts/ContractCard.test.tsx
git commit -m "test(contracts): ContractCard — 8 tests covering all status states, a11y, interactions"
```

---

## Task 2: `ContractContextMenu.tsx`

**Files:**
- Create: `frontend/src/components/contracts/ContractContextMenu.tsx`

- [ ] **Step 1: Create the component**

Per spec §7.10 — 10 items. The component wraps `children` as the `ContextMenuTrigger` so it can be used both as a card wrapper (right-click) and by forwarding the `MoreHorizontal` button click.

```tsx
import { useState } from 'react'
import {
  ExternalLink, Pencil, Copy, FileDown, Link,
  PauseCircle, PlayCircle, RefreshCw, Trash2,
  Send, CheckCircle, XCircle,
} from 'lucide-react'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import type { Contract } from '@/types/contracts'
import type { ContractAction } from './ContractCard'

interface ContractContextMenuProps {
  contract: Contract
  onAction: (action: ContractAction, id: string) => void
  children: React.ReactNode
}

export function ContractContextMenu({
  contract,
  onAction,
  children,
}: ContractContextMenuProps) {
  const [deleteOpen, setDeleteOpen] = useState(false)
  const s = contract.status

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
        <ContextMenuContent className="w-52">
          {/* 1 — Open contract (detail view, future milestone) */}
          <ContextMenuItem onSelect={() => onAction('open', contract.id)}>
            <ExternalLink className="h-3.5 w-3.5 mr-2" aria-hidden="true" />
            Open contract
          </ContextMenuItem>
          {/* 2 — Edit */}
          <ContextMenuItem onSelect={() => onAction('edit', contract.id)}>
            <Pencil className="h-3.5 w-3.5 mr-2" aria-hidden="true" />
            Edit
          </ContextMenuItem>

          <ContextMenuSeparator />

          {/* 3 — Duplicate */}
          <ContextMenuItem onSelect={() => onAction('duplicate', contract.id)}>
            <Copy className="h-3.5 w-3.5 mr-2" aria-hidden="true" />
            Duplicate
          </ContextMenuItem>
          {/* 4 — Export as OpenAPI */}
          <ContextMenuItem onSelect={() => onAction('export', contract.id)}>
            <FileDown className="h-3.5 w-3.5 mr-2" aria-hidden="true" />
            Export as OpenAPI
          </ContextMenuItem>
          {/* 5 — Copy contract link */}
          <ContextMenuItem
            onSelect={() =>
              navigator.clipboard.writeText(
                `rocketapi://contract/${contract.id}`,
              )
            }
          >
            <Link className="h-3.5 w-3.5 mr-2" aria-hidden="true" />
            Copy contract link
          </ContextMenuItem>

          <ContextMenuSeparator />

          {/* 6a — Approve / Reject (in_review only) */}
          {s === 'in_review' && (
            <>
              <ContextMenuItem onSelect={() => onAction('approve', contract.id)}>
                <CheckCircle className="h-3.5 w-3.5 mr-2" aria-hidden="true" />
                Approve
              </ContextMenuItem>
              <ContextMenuItem onSelect={() => onAction('reject', contract.id)}>
                <XCircle className="h-3.5 w-3.5 mr-2" aria-hidden="true" />
                Reject
              </ContextMenuItem>
              <ContextMenuSeparator />
            </>
          )}

          {/* 6b — Send for review */}
          {['active', 'drift', 'breach', 'expiring_in_30_days'].includes(s) && (
            <ContextMenuItem onSelect={() => onAction('send_for_review', contract.id)}>
              <Send className="h-3.5 w-3.5 mr-2" aria-hidden="true" />
              Send for review
            </ContextMenuItem>
          )}

          {/* 7 — Pause monitoring */}
          {['active', 'drift', 'breach', 'expiring_in_30_days'].includes(s) && (
            <ContextMenuItem onSelect={() => onAction('pause', contract.id)}>
              <PauseCircle className="h-3.5 w-3.5 mr-2" aria-hidden="true" />
              Pause monitoring
            </ContextMenuItem>
          )}

          {/* 8 — Resume */}
          {s === 'paused' && (
            <ContextMenuItem onSelect={() => onAction('resume', contract.id)}>
              <PlayCircle className="h-3.5 w-3.5 mr-2" aria-hidden="true" />
              Resume
            </ContextMenuItem>
          )}

          {/* 9 — Renew */}
          {s === 'expired' && (
            <ContextMenuItem onSelect={() => onAction('renew', contract.id)}>
              <RefreshCw className="h-3.5 w-3.5 mr-2" aria-hidden="true" />
              Renew
            </ContextMenuItem>
          )}

          <ContextMenuSeparator />

          {/* 10 — Delete */}
          <ContextMenuItem
            onSelect={() => setDeleteOpen(true)}
            className="text-destructive focus:text-destructive"
          >
            <Trash2 className="h-3.5 w-3.5 mr-2" aria-hidden="true" />
            Delete
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      {/* Delete confirmation dialog */}
      <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete contract?</AlertDialogTitle>
            <AlertDialogDescription>
              <strong>{contract.name}</strong> and all its changelog entries will be
              permanently deleted. This action cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                setDeleteOpen(false)
                onAction('delete', contract.id)
              }}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              Delete contract
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/components/contracts/ContractContextMenu.tsx
git commit -m "feat(contracts): ContractContextMenu — 10 items, status-aware, delete AlertDialog"
```

---

## Task 3: Wire `ContractContextMenu` into `ContractCard`

**Files:**
- Modify: `frontend/src/components/contracts/ContractCard.tsx`

Replace the `MoreHorizontal` stub button and add context menu wrapping. This is a surgical edit — only the return statement changes.

- [ ] **Step 1: Add import**

Add to the imports at the top of `ContractCard.tsx`:

```tsx
import { ContractContextMenu } from './ContractContextMenu'
```

- [ ] **Step 2: Replace the `MoreHorizontal` button with a context menu trigger**

Find this block in `ContractCard.tsx` (in the footer actions div):

```tsx
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
```

Replace it with:

```tsx
            <ContractContextMenu contract={contract} onAction={onAction}>
              <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                onClick={stopPropagation}
                aria-label="More actions"
              >
                <MoreHorizontal className="h-3.5 w-3.5" aria-hidden="true" />
              </Button>
            </ContractContextMenu>
```

- [ ] **Step 3: Wrap the entire `<article>` for right-click**

Find the `return (` in `ContractCard`. Wrap the `<article>` element with `ContractContextMenu`:

```tsx
  return (
    <ContractContextMenu contract={contract} onAction={onAction}>
      <article
        role="article"
        {/* ... all existing props unchanged ... */}
      >
        {/* ... all existing children unchanged ... */}
      </article>
    </ContractContextMenu>
  )
```

The complete updated return statement:

```tsx
  return (
    <ContractContextMenu contract={contract} onAction={onAction}>
      <article
        role="article"
        aria-labelledby={`cc-name-${contract.id}`}
        data-status={contract.status}
        tabIndex={0}
        onClick={() => onOpen?.(contract.id)}
        onKeyDown={e => { if (e.key === 'Enter') onOpen?.(contract.id) }}
        className={cn(
          'group relative bg-card border border-border rounded-[var(--radius)]',
          'p-[18px_20px] grid grid-cols-1 md:grid-cols-[1fr_220px] gap-6 mb-[10px]',
          'cursor-pointer transition-[border-color,box-shadow] duration-[120ms]',
          'hover:border-[hsl(var(--border)/1.4)] hover:shadow-[0_1px_3px_rgba(0,0,0,0.06)]',
          'focus-visible:outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50',
          contract.status === 'drift' &&
            'border-l-[3px] border-l-[hsl(var(--warning))] pl-[17px]',
          contract.status === 'breach' &&
            'border-l-[3px] border-l-[hsl(var(--destructive))] pl-[17px] bg-[color-mix(in_oklab,hsl(var(--destructive-soft))_25%,hsl(var(--card)))]',
          contract.status === 'paused' &&
            'bg-[color-mix(in_oklab,hsl(var(--muted))_50%,hsl(var(--card)))]',
          contract.status === 'expired' && 'opacity-75',
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
              value={contract.expiresAt ? formatDate(contract.expiresAt) : '—'}
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

          {/* Scope tags */}
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
              <ContractContextMenu contract={contract} onAction={onAction}>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7"
                  onClick={stopPropagation}
                  aria-label="More actions"
                >
                  <MoreHorizontal className="h-3.5 w-3.5" aria-hidden="true" />
                </Button>
              </ContractContextMenu>
            </div>
          </div>
        </div>

        {/* ─── Right column ────────────────────────────────── */}
        <div className="hidden md:block">
          <MiniChangelog
            entries={contract.changelog}
            status={contract.status}
            onViewAll={() => onAction('view_changelog', contract.id)}
          />
        </div>
      </article>
    </ContractContextMenu>
  )
```

- [ ] **Step 4: Re-run tests to confirm nothing broke**

```bash
cd frontend && yarn vitest run src/components/contracts/ContractCard.test.tsx 2>&1 | tail -5
```

Expected: `8 tests passed`.

- [ ] **Step 5: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/contracts/ContractCard.tsx
git commit -m "feat(contracts): wire ContractContextMenu into ContractCard — right-click + MoreHorizontal button"
```
