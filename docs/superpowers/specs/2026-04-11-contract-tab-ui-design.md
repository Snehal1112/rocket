# Contract Lock Tab — UI Design Spec

> **Type:** Spec (reference only — never executed directly)
> **Date:** 2026-04-11
> **Status:** Approved
> **Depends on:** Contract Lock domain spec (2026-04-07)

---

## Overview

Contract creation and reading opens as a dedicated tab in the main editor area — the same pattern as the Git tab and Workspace Overview tab. The tab is triggered by right-clicking a collection, folder, or request in the sidebar and selecting "Manage contracts".

The tab has two views inside it: a **list view** (all contracts for the collection) and a **create/edit view** (two-column form + live preview). Switching between them never opens a dialog — everything lives within the tab.

---

## Entry Points

| Trigger | Result |
|---|---|
| Right-click collection → "Manage contracts" | Opens `ContractTab` for that collection, stable id `contract:{collectionName}` |
| Click lock badge on collection / folder / request | Opens `ContractTab` scrolled to / highlighting that contract |
| Right-click folder → "Manage contracts" | Same tab, list pre-filtered to folder scope |

The tab id is stable and deduplicated — clicking the same collection again focuses the existing tab.

---

## Tab Type Addition

Add `ContractTab` to `src/types/pane-types.ts`:

```typescript
export interface ContractTab extends BaseTab {
  tabType: 'contract';
  collectionName: string;
  collectionRoot: string;      // absolute path — needed for all IPC calls
  initialScope?: ContractScope; // pre-fill scope when opened from badge click
}

export function isContractTab(tab: Tab): tab is ContractTab {
  return tab.tabType === 'contract';
}
```

Update `Tab` union:
```typescript
export type Tab = RequestTab | CollectionTab | WorkspaceTab | DiffTab | ConflictTab | GitTab | ContractTab;
```

---

## Layout — List View (default)

```
┌─ Tab bar ────────────────────────────────────────────────────┐
│  [Request tabs...]   [🔒 Contracts — Payments API  ×]        │
├──────────────────────────────────────────────────────────────┤
│  ┌─ Top bar ──────────────────────────────────────────────┐  │
│  │  🔒 Contracts                    [ + New contract ]    │  │
│  │  Payments API                                          │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌─ Contract card ────────────────────────────────────────┐  │
│  │  Payments API v2.3                    [ Active ]       │  │
│  │  Checkout Revamp  ·  Billing Team → Platform Team      │  │
│  │  Effective 2026-01-15  ·  Expires 2026-12-31           │  │
│  │  ── Scope: Entire collection ──────────────────────── │  │
│  │  3 changes logged        [ View changelog ]  [ Edit ]  │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌─ Contract card ────────────────────────────────────────┐  │
│  │  Auth Service v1.0                 [ Expiring soon ]   │  │
│  │  App Relaunch  ·  Identity Team → Mobile Team          │  │
│  │  Effective 2025-09-01  ·  Expires 2026-06-30           │  │
│  │  ── Scope: /auth folder ──────────────────────────── │  │
│  │  0 changes logged        [ View changelog ]  [ Edit ]  │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
│  [ Empty state if no contracts: "No contracts yet..." ]      │
└──────────────────────────────────────────────────────────────┘
```

### Contract card anatomy

- Title + status chip (right-aligned): `Active` = green, `Expiring soon` = amber, `Expired` = red/destructive
- Sub-line: `{project} · {provider} → {consumer}` with coloured dots
- Date line: `Effective {date} · Expires {date}` (or "No expiry")
- Scope badge: pill showing `Collection` / `Folder: /auth` / `Request: create-payment.yml`
- Footer: change count + `View changelog` ghost button + `Edit` ghost button
- Hover: card lifts with `border-border` → `border-primary/40` transition

---

## Layout — Create / Edit View

Triggered by "+ New contract" button or "Edit" on an existing card. Replaces the list view inside the same tab (no new tab, no dialog).

```
┌─ Top bar ───────────────────────────────────────────────────┐
│  ← Back to contracts       New contract / Edit contract     │
│                                     ●  ●  ○  (step dots)   │
├──────────────────────────┬──────────────────────────────────┤
│  FORM (left, 380px)      │  LIVE PREVIEW (right, flex-1)   │
│                          │                                  │
│  Title                   │  [Contract card renders here     │
│  [________________]      │   as you type — same design      │
│                          │   as list view card]             │
│  Provider team           │                                  │
│  [________________]      │  ─────────────────────────────  │
│                          │                                  │
│  Consumer team           │  💡 Once created, RocketAPI      │
│  [________________]      │  will snapshot all covered       │
│                          │  endpoint signatures. Every      │
│  Project                 │  save is diffed automatically.   │
│  [________________]      │                                  │
│                          │                                  │
│  Version                 │                                  │
│  [________________]      │                                  │
│                          │                                  │
│  Effective date          │                                  │
│  [________________]      │                                  │
│                          │                                  │
│  Expiry date (optional)  │                                  │
│  [________________]      │                                  │
│                          │                                  │
│  Scope                   │                                  │
│  ○ Entire collection     │                                  │
│  ○ Folder  [select ▾]   │                                  │
│  ○ Request [select ▾]   │                                  │
│                          │                                  │
│  Attach document         │                                  │
│  [📎 Browse…]            │                                  │
├──────────────────────────┴──────────────────────────────────┤
│                              [ Cancel ]  [ Create contract ] │
└─────────────────────────────────────────────────────────────┘
```

---

## Layout — Changelog View

Opened by "View changelog" on a contract card. Replaces the list view, same pattern as create/edit.

```
┌─ Top bar ───────────────────────────────────────────────────┐
│  ← Back to contracts                                        │
│  Payments API v2.3  ·  Billing Team → Platform Team        │
├─────────────────────────────────────────────────────────────┤
│  SUMMARY BAR                                                │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐  │
│  │ 3 changes│  │ 2 removed│  │  1 added │  │ 0 changed│  │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘  │
├─────────────────────────────────────────────────────────────┤
│  DATE        FIELD                  TYPE      BEFORE  AFTER │
│  ──────────────────────────────────────────────────────── │
│  Apr 7, '26  query_param.amount    [removed]  amount   —   │
│  Apr 5, '26  method                [changed]  GET      POST │
│  Apr 1, '26  header.x-api-key      [added]    —        key  │
│                                                             │
│  (empty state: "No changes recorded since contract signed") │
└─────────────────────────────────────────────────────────────┘
```

---

## Theme Rules

All colours use shadcn/ui CSS variables — no hardcoded hex in component code.

| Element | Token |
|---|---|
| Card background | `bg-card` |
| Card border (default) | `border-border` |
| Card border (hover) | `border-primary/40` |
| Page background | `bg-background` |
| Form input | shadcn `<Input>` — inherits theme |
| Status chip Active | `bg-green-500/10 text-green-700 dark:text-green-400` |
| Status chip Expiring | `bg-amber-500/10 text-amber-700 dark:text-amber-400` |
| Status chip Expired | `bg-destructive/10 text-destructive` |
| Provider dot | `bg-violet-500` (both modes — decorative, not semantic) |
| Consumer dot | `bg-emerald-500` (both modes — decorative, not semantic) |
| Change badge removed | `bg-destructive/10 text-destructive` |
| Change badge added | `bg-green-500/10 text-green-700 dark:text-green-400` |
| Change badge changed | `bg-blue-500/10 text-blue-700 dark:text-blue-400` |
| Hint box border-left | `border-l-2 border-primary/30` + `bg-primary/5` |
| Section label | `text-xs font-medium uppercase tracking-wide text-muted-foreground` |
| Mono values (field names) | `font-mono text-xs bg-muted px-1 rounded` |

All interactive elements: shadcn/ui primitives only (`Button`, `Input`, `Label`, `RadioGroup`, `Select`, `Separator`, `ScrollArea`, `Badge`).

---

## Component Tree

```
ContractTab
├── ContractTabTopBar          (title + action button / back button + step dots)
├── [view === 'list']
│   ├── ContractEmptyState     (shown when contracts.length === 0)
│   └── ContractCard[]         (one per contract)
│       ├── status chip
│       ├── parties pills
│       ├── scope badge
│       └── footer actions (View changelog · Edit · Delete)
├── [view === 'create' | 'edit']
│   ├── ContractForm           (left column — all shadcn/ui inputs)
│   └── ContractLivePreview    (right column — renders ContractCard with form values)
└── [view === 'changelog']
    ├── ChangelogSummaryBar    (4 metric cards)
    └── ChangelogTable         (sortable, shadcn Table)
```

---

## State Management

`ContractTab` is a self-contained component. It holds its own local UI state (which view is active, form values, optimistic loading). It reads from and writes to `useContractStore` (already defined in Plan 04) for all IPC operations.

```typescript
type ContractTabView =
  | { type: 'list' }
  | { type: 'create' }
  | { type: 'edit'; contractId: string }
  | { type: 'changelog'; contractId: string }

// Local state inside ContractTab:
const [view, setView] = useState<ContractTabView>({ type: 'list' })
```

---

## File Map

| File | Action | Purpose |
|---|---|---|
| `src/types/pane-types.ts` | Modify | Add `ContractTab` type + `isContractTab` guard |
| `src/stores/pane-store.ts` | Modify | Add `openContractTab()` action |
| `src/components/contract/ContractTab.tsx` | Create | Root tab component, view router |
| `src/components/contract/ContractTabTopBar.tsx` | Create | Title bar with back/action buttons |
| `src/components/contract/ContractCard.tsx` | Create | Reusable contract card (list + preview) |
| `src/components/contract/ContractForm.tsx` | Create | Left-column create/edit form |
| `src/components/contract/ContractLivePreview.tsx` | Create | Right-column live preview |
| `src/components/contract/ChangelogSummaryBar.tsx` | Create | 4 metric cards |
| `src/components/contract/ChangelogTable.tsx` | Create | Sortable table with badge types |
| `src/components/contract/ContractEmptyState.tsx` | Create | Empty list state |
| `src/components/panes/EditorGroup.tsx` | Modify | Add `isContractTab` routing branch |
| `src/components/layout/CollectionsSidebar.tsx` | Modify | Add "Manage contracts" to right-click context menu |
