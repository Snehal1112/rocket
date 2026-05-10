# Contract Diff Pane Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Open a dedicated "Contract Diff Pane" tab when the user clicks "Review diff →" or "Open contract" on a contract card, showing the full changelog and an "Accept all changes" CTA.

**Architecture:** Add `ContractDiffTab` to the tab union, create a `ContractDiffPane` component that fetches changelog via existing `getContractChangelog` IPC, wire it into `EditorGroup`'s render switch, and update `ContractsTab` to open the tab instead of the edit modal for `review_diff` / `open` / `view_changelog` actions.

**Tech Stack:** React/TypeScript, shadcn/ui (Card, Badge, Button, Skeleton), lucide-react, date-fns, Zustand (`usePaneStore`, `useContractsStore`), Tauri IPC (`getContractChangelog`, `publishContract`)

---

## File Map

| File | Change |
|---|---|
| `src/types/pane-types.ts` | Add `ContractDiffTab` interface and `isContractDiffTab` guard, add to `Tab` union |
| `src/components/contracts/ContractDiffPane.tsx` | **Create** — the pane component |
| `src/components/panes/EditorGroup.tsx` | Import `ContractDiffPane` + `isContractDiffTab`, add render branch |
| `src/components/contracts/ContractsTab.tsx` | Wire `review_diff`, `open`, `view_changelog` to `openTab` |

---

## Task 1: Add `ContractDiffTab` to the tab type system

**Files:**
- Modify: `src/types/pane-types.ts`

- [ ] **Step 1: Add `ContractDiffTab` interface and type guard**

Open `src/types/pane-types.ts`. After the `ContractTab` block (around line 88) add:

```typescript
export interface ContractDiffTab extends BaseTab {
  tabType: 'contract_diff';
  collectionId: string; // absolute path — passed as collectionRoot to IPC
  contractId: string;
}

export function isContractDiffTab(tab: Tab): tab is ContractDiffTab {
  return tab.tabType === 'contract_diff';
}
```

- [ ] **Step 2: Add `ContractDiffTab` to the `Tab` union**

Find the `Tab` type union (around line 98):

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

Replace it with:

```typescript
export type Tab =
  | RequestTab
  | CollectionTab
  | WorkspaceTab
  | DiffTab
  | ConflictTab
  | GitTab
  | ContractTab
  | ContractDiffTab;
```

- [ ] **Step 3: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/types/pane-types.ts
git commit -m "feat(contracts): add ContractDiffTab type and isContractDiffTab guard"
```

---

## Task 2: Create `ContractDiffPane` component

**Files:**
- Create: `src/components/contracts/ContractDiffPane.tsx`

**Context:**
- `getContractChangelog(collectionRoot, contractId)` returns `ContractChangelog { contractId: string; entries: ChangelogEntry[] }` — already in `src/lib/tauri-api.ts`
- `ChangelogEntry` has: `timestamp: string`, `requestPath: string`, `field: string`, `changeType: 'changed' | 'added' | 'removed'`, `oldValue: string | null`, `newValue: string | null`, `isBreaking: boolean`
- `ChangeChip` takes `kind: ChangeKind` where `ChangeKind = 'add' | 'remove' | 'modify'` — map `'added'→'add'`, `'removed'→'remove'`, `'changed'→'modify'`
- `publishContract(collectionRoot, contractId)` is the IPC for "accept all changes / resign" (same as `accept_drift` action uses)
- `useContractsStore((s) => s.byId)` gives access to contract objects by id; contract has `name: string`, `status: ContractStatus`
- `publishContract` is imported from `src/lib/tauri-api.ts`; the store's `publishContract` action also works — use `useContractsStore((s) => s.publishContract)`
- `usePaneStore((s) => s.closeTab)` closes the current tab after accepting

- [ ] **Step 1: Create the file**

```typescript
import { format } from 'date-fns';
import { AlertCircle, RefreshCw } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { ChangeChip } from '@/components/contracts/ChangeChip';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Skeleton } from '@/components/ui/skeleton';
import { getContractChangelog } from '@/lib/tauri-api';
import type { ChangeKind } from '@/types/contracts';
import { useContractsStore } from '@/stores/contracts/contractsSlice';
import { usePaneStore } from '@/stores/pane-store';

interface ContractDiffPaneProps {
  collectionId: string;
  contractId: string;
}

function changeTypeToKind(changeType: 'changed' | 'added' | 'removed'): ChangeKind {
  if (changeType === 'added') return 'add';
  if (changeType === 'removed') return 'remove';
  return 'modify';
}

function truncate(val: string | null, max = 40): string {
  if (!val) return '—';
  return val.length > max ? `${val.slice(0, max)}…` : val;
}

export function ContractDiffPane({ collectionId, contractId }: ContractDiffPaneProps) {
  const byId = useContractsStore((s) => s.byId);
  const publishContract = useContractsStore((s) => s.publishContract);
  const closeTab = usePaneStore((s) => s.closeTab);
  const activeGroupId = usePaneStore((s) => s.activeGroupId);

  const contract = byId[contractId];
  const [entries, setEntries] = useState<
    Awaited<ReturnType<typeof getContractChangelog>>['entries']
  >([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [accepting, setAccepting] = useState(false);

  const loadChangelog = useCallback(() => {
    setLoading(true);
    setError(null);
    getContractChangelog(collectionId, contractId)
      .then((log) => setEntries(log.entries))
      .catch((err) => setError(String(err)))
      .finally(() => setLoading(false));
  }, [collectionId, contractId]);

  useEffect(() => {
    loadChangelog();
  }, [loadChangelog]);

  const handleAccept = useCallback(async () => {
    setAccepting(true);
    try {
      await publishContract(collectionId, contractId);
      closeTab(`contract_diff:${contractId}`, activeGroupId);
    } catch (err) {
      console.error('[ContractDiffPane] Accept failed:', err);
    } finally {
      setAccepting(false);
    }
  }, [publishContract, collectionId, contractId, closeTab, activeGroupId]);

  const breakingCount = entries.filter((e) => e.isBreaking).length;
  const canAccept = contract?.status === 'drift' || contract?.status === 'breach';

  return (
    <div className='flex flex-col h-full'>
      {/* Summary bar */}
      <div className='flex items-center gap-3 px-6 py-3 border-b bg-card shrink-0'>
        <span className='font-semibold text-sm truncate'>{contract?.name ?? contractId}</span>
        {contract?.status && (
          <Badge variant='outline' className='capitalize text-xs'>
            {contract.status.replace(/_/g, ' ')}
          </Badge>
        )}
        <span className='text-xs text-muted-foreground ml-auto'>
          {loading ? '…' : `${entries.length} change${entries.length !== 1 ? 's' : ''}`}
          {breakingCount > 0 && (
            <span className='ml-2 text-destructive font-medium'>
              · {breakingCount} breaking
            </span>
          )}
        </span>
      </div>

      {/* Content */}
      <ScrollArea className='flex-1 min-h-0'>
        <div className='px-6 py-4'>
          {loading ? (
            <div className='space-y-2'>
              {[1, 2, 3].map((i) => (
                <Skeleton key={i} className='h-10 w-full rounded' />
              ))}
            </div>
          ) : error ? (
            <Card className='border-destructive/30'>
              <CardContent className='flex items-center gap-3 py-4'>
                <AlertCircle className='h-4 w-4 text-destructive shrink-0' />
                <span className='text-sm text-destructive'>{error}</span>
                <Button size='sm' variant='outline' className='ml-auto' onClick={loadChangelog}>
                  <RefreshCw className='h-3.5 w-3.5 mr-1' />
                  Retry
                </Button>
              </CardContent>
            </Card>
          ) : entries.length === 0 ? (
            <p className='text-sm text-muted-foreground py-8 text-center'>
              No changes recorded yet.
            </p>
          ) : (
            <div className='rounded-md border overflow-hidden'>
              <table className='w-full text-sm'>
                <thead className='bg-muted/50'>
                  <tr>
                    <th className='text-left font-medium text-xs text-muted-foreground px-3 py-2 w-32'>Date</th>
                    <th className='text-left font-medium text-xs text-muted-foreground px-3 py-2'>Request</th>
                    <th className='text-left font-medium text-xs text-muted-foreground px-3 py-2 w-24'>Field</th>
                    <th className='text-left font-medium text-xs text-muted-foreground px-3 py-2 w-16'>Kind</th>
                    <th className='text-left font-medium text-xs text-muted-foreground px-3 py-2'>Old → New</th>
                    <th className='text-left font-medium text-xs text-muted-foreground px-3 py-2 w-24'></th>
                  </tr>
                </thead>
                <tbody>
                  {entries.map((entry, idx) => (
                    <tr
                      key={`${entry.requestPath}-${entry.field}-${idx}`}
                      className='border-t hover:bg-muted/20 transition-colors'
                    >
                      <td className='px-3 py-2 text-xs text-muted-foreground whitespace-nowrap'>
                        {format(new Date(entry.timestamp), 'MMM d, HH:mm')}
                      </td>
                      <td className='px-3 py-2 font-mono text-xs truncate max-w-[180px]'>
                        {entry.requestPath}
                      </td>
                      <td className='px-3 py-2 text-xs'>{entry.field}</td>
                      <td className='px-3 py-2'>
                        <ChangeChip kind={changeTypeToKind(entry.changeType)} />
                      </td>
                      <td className='px-3 py-2 text-xs text-muted-foreground font-mono'>
                        <span className='line-through mr-1'>{truncate(entry.oldValue)}</span>
                        <span className='text-foreground'>{truncate(entry.newValue)}</span>
                      </td>
                      <td className='px-3 py-2'>
                        {entry.isBreaking && (
                          <Badge variant='destructive' className='text-[10px] px-1.5 py-0'>
                            Breaking
                          </Badge>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </ScrollArea>

      {/* Footer CTA */}
      {canAccept && (
        <div className='flex items-center justify-end gap-3 px-6 py-3 border-t bg-card shrink-0'>
          <p className='text-xs text-muted-foreground'>
            Accepting re-signs the contract at the current API shape.
          </p>
          <Button size='sm' disabled={accepting} onClick={() => void handleAccept()}>
            {accepting ? 'Accepting…' : 'Accept all changes'}
          </Button>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Run Biome format**

```bash
yarn format
```

- [ ] **Step 4: Commit**

```bash
git add src/components/contracts/ContractDiffPane.tsx
git commit -m "feat(contracts): create ContractDiffPane component"
```

---

## Task 3: Wire `ContractDiffPane` into `EditorGroup`

**Files:**
- Modify: `src/components/panes/EditorGroup.tsx`

- [ ] **Step 1: Add the import for `ContractDiffPane` and `isContractDiffTab`**

In `src/components/panes/EditorGroup.tsx`, find the existing imports block. Add these two imports alongside the other contracts/pane-types imports:

```typescript
import { ContractDiffPane } from '@/components/contracts/ContractDiffPane';
import { isContractDiffTab } from '@/types/pane-types';
```

The existing import from `@/types/pane-types` (around line 36) looks like:

```typescript
import {
  isConflictTab,
  isContractTab,
  isDiffTab,
  isGitTab,
  isRequestTab,
  isWorkspaceTab,
} from '@/types/pane-types';
```

Replace it with:

```typescript
import {
  isConflictTab,
  isContractDiffTab,
  isContractTab,
  isDiffTab,
  isGitTab,
  isRequestTab,
  isWorkspaceTab,
} from '@/types/pane-types';
```

- [ ] **Step 2: Add the render branch for `ContractDiffPane`**

Find the `isContractTab` branch in the render switch (around line 187):

```tsx
          ) : isContractTab(activeTab) ? (
            <ContractsTab
              collectionId={activeTab.collectionRoot}
              collectionName={activeTab.collectionName}
            />
          ) : isWorkspaceTab(activeTab) ? (
```

Add the `isContractDiffTab` branch immediately after `isContractTab`:

```tsx
          ) : isContractTab(activeTab) ? (
            <ContractsTab
              collectionId={activeTab.collectionRoot}
              collectionName={activeTab.collectionName}
            />
          ) : isContractDiffTab(activeTab) ? (
            <ContractDiffPane
              collectionId={activeTab.collectionId}
              contractId={activeTab.contractId}
            />
          ) : isWorkspaceTab(activeTab) ? (
```

- [ ] **Step 3: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/panes/EditorGroup.tsx
git commit -m "feat(contracts): render ContractDiffPane in EditorGroup"
```

---

## Task 4: Wire `review_diff`, `open`, and `view_changelog` in `ContractsTab`

**Files:**
- Modify: `src/components/contracts/ContractsTab.tsx`

- [ ] **Step 1: Import `usePaneStore`**

In `src/components/contracts/ContractsTab.tsx`, the imports already pull from stores. Add `usePaneStore`:

```typescript
import { usePaneStore } from '@/stores/pane-store';
```

- [ ] **Step 2: Subscribe to `openTab` from the pane store**

Inside the `ContractsTab` component function, after the existing `useContractsStore` selectors (around line 47), add:

```typescript
const openTab = usePaneStore((s) => s.openTab);
const byId = useContractsStore((s) => s.byId);
```

Note: `byId` may already be accessible via the existing `editingContract` selector — check if the component already has `byId` in scope. If it does, skip adding `byId` again.

- [ ] **Step 3: Replace the `review_diff` / `open` cases**

Find the current handling (around lines 125–130):

```typescript
          case 'open':
          case 'review_diff':
            // Open the contract editor — the best available detail view until a dedicated panel is built.
            setEditingId(contractId);
            setModalOpen(true);
            break;
```

Replace with:

```typescript
          case 'open':
          case 'review_diff':
          case 'view_changelog':
            openTab({
              id: `contract_diff:${contractId}`,
              title: `Diff — ${byId[contractId]?.name ?? contractId}`,
              tabType: 'contract_diff',
              collectionId,
              contractId,
              isDirty: false,
            });
            break;
```

- [ ] **Step 4: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 5: Run Biome format**

```bash
yarn format
```

- [ ] **Step 6: Commit**

```bash
git add src/components/contracts/ContractsTab.tsx
git commit -m "feat(contracts): open ContractDiffPane tab for review_diff/open/view_changelog actions"
```

---

## Self-Review

- [x] **Spec coverage:**
  - Section 1 (tab type) → Task 1
  - Section 2 (ContractDiffPane: summary bar, table, CTA, loading, error) → Task 2
  - Section 3 (EditorGroup wiring) → Task 3
  - Section 4 (ContractsTab wiring) → Task 4

- [x] **Placeholder scan:** All code is complete. No TBDs or vague steps.

- [x] **Type consistency:**
  - `ContractDiffTab.collectionId` matches `ContractDiffPane` prop `collectionId` and the `openTab` payload in Task 4.
  - `ContractDiffTab.contractId` matches `ContractDiffPane` prop `contractId` and the `openTab` payload.
  - `tab.id` uses `contract_diff:${contractId}` consistently in Task 4 (openTab call) and Task 2 (closeTab call after accept).
  - `changeTypeToKind` maps all three `changeType` variants from the IPC (`'added'`, `'removed'`, `'changed'`) to valid `ChangeKind` values (`'add'`, `'remove'`, `'modify'`).
  - `publishContract` from the contracts store takes `(collectionId, contractId)` — matches Task 2 usage.
  - `isContractDiffTab` is exported from `pane-types.ts` in Task 1 and imported in Task 3.
