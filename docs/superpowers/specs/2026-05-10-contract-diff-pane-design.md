# Contract Diff Pane — Design Spec

**Date:** 2026-05-10  
**Status:** Approved  
**Scope:** Open contract changelogs as a dedicated tab ("Contract Diff Pane") instead of the Edit Contract dialog when the user clicks "Review diff →" or "View changelog" on a contract card.

---

## Background

When a contract enters **Drift** or **Breach** status the "Review diff →" primary action currently opens the Edit Contract dialog, which is wrong. The user wants a read-focused changelog view that shows what changed, lets them judge severity, and provides a single CTA to accept all changes (resign). This view should open as a tab in the editor area, consistent with how Diff and Git views behave.

---

## Section 1 — Tab Type

Add `ContractDiffTab` to `src/types/pane-types.ts`.

```typescript
export interface ContractDiffTab extends BaseTab {
  tabType: 'contract_diff';
  collectionId: string;   // absolute path — passed as collectionRoot to IPC
  contractId: string;
}

export function isContractDiffTab(tab: Tab): tab is ContractDiffTab {
  return tab.tabType === 'contract_diff';
}
```

Add `ContractDiffTab` to the `Tab` union:

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

Tab `id` is deterministic: `contract_diff:${contractId}` — reopening the same contract reuses the existing tab.

---

## Section 2 — ContractDiffPane Component

**Create:** `src/components/contracts/ContractDiffPane.tsx`

The component receives `collectionId` and `contractId` from the tab props. On mount it calls `getContractChangelog(collectionId, contractId)` and `getContracts(collectionId)` (to find the contract name and status).

### Layout

```
┌─────────────────────────────────────────────────────────────┐
│ Summary bar                                                  │
│  <contract name>  · <status chip>  ·  N changes  ·  B breaking  │
├─────────────────────────────────────────────────────────────┤
│ Changelog table                                              │
│  Date  │ Request path  │ Field  │ Kind  │ Old → New  │ Breaking  │
│  ...                                                         │
├─────────────────────────────────────────────────────────────┤
│ Footer CTA (only when status is drift or breach)             │
│  [Accept all changes — resign contract]                      │
└─────────────────────────────────────────────────────────────┘
```

### Summary bar

- Contract name (from contracts store, fallback to contractId)
- Status chip (reuse the same chip used in ContractCard)
- Total change count
- Breaking change count highlighted in destructive color when > 0

### Changelog table

Columns:

| Column | Source |
|---|---|
| Date | `entry.timestamp` formatted as `MMM D, HH:mm` |
| Request | `entry.requestPath` (monospace, truncated) |
| Field | `entry.field` |
| Kind | `<ChangeChip>` reusing the existing component (changeType ADD/MOD/REM) |
| Old → New | `entry.oldValue` and `entry.newValue` side-by-side, truncated at 40 chars |
| Breaking | Red badge "Breaking" when `entry.isBreaking === true`, empty otherwise |

Empty state when `entries.length === 0`: "No changes recorded yet."

Loading state: skeleton rows (3 placeholder rows with animate-pulse).

### Footer CTA

Show only when the contract status is `'drift'` or `'breach'`.

```
[Accept all changes]
```

Clicking calls `resignContract(collectionId, contractId)`, then closes the tab on success.

### Error handling

If `getContractChangelog` rejects, show an inline error message with a Retry button. Do not crash.

---

## Section 3 — EditorGroup wiring

In `src/components/panes/EditorGroup.tsx`, add `ContractDiffPane` to the render switch.

Import:
```typescript
import { ContractDiffPane } from '@/components/contracts/ContractDiffPane';
import { isContractDiffTab } from '@/types/pane-types';
```

Add case after the `isContractTab` branch:
```tsx
) : isContractDiffTab(activeTab) ? (
  <ContractDiffPane
    collectionId={activeTab.collectionId}
    contractId={activeTab.contractId}
  />
)
```

---

## Section 4 — ContractsTab wiring

In `src/components/contracts/ContractsTab.tsx`, change `case 'review_diff'` and the `case 'open'` that currently call `setEditingId`.

Import `usePaneStore`:
```typescript
const openTab = usePaneStore((s) => s.openTab);
```

Replace the two cases:
```typescript
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

The `open` / `view_changelog` action (from active card) should also open this tab so users can browse the changelog at any time, not just during drift.

---

## IPC used

| Function | File |
|---|---|
| `getContractChangelog(collectionRoot, contractId)` | Already exists in `tauri-api.ts` |
| `resignContract(collectionRoot, contractId)` | Already exists in `tauri-api.ts` (via contracts store) |
| `getContracts(collectionRoot)` | Already loaded by `useContracts` hook — read from store |

No new IPC commands are needed.

---

## Files Changed

| File | Change |
|---|---|
| `src/types/pane-types.ts` | Add `ContractDiffTab`, `isContractDiffTab`, update `Tab` union |
| `src/components/contracts/ContractDiffPane.tsx` | **Create** — the pane component |
| `src/components/panes/EditorGroup.tsx` | Add `isContractDiffTab` branch |
| `src/components/contracts/ContractsTab.tsx` | Wire `review_diff`/`view_changelog`/`open` to `openTab` |

---

## What This Does NOT Change

- No new Tauri commands — all IPC already exists
- `ContractCard` action names are unchanged
- The Edit Contract modal (`EditContractModal`) is unchanged — still opens for the `edit` action
- `ContractScope`, changelog format, or any persistence layer
