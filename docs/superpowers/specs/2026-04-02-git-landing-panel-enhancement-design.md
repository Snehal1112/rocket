# Git Landing Panel Enhancement — Status Card Design

**Date:** 2026-04-02  
**File:** `src/components/git/GitLandingPanel.tsx`  
**Status:** Approved

---

## Problem

The current `GitLandingPanel` is visually loose: a large faded `GitBranch` icon, centered descriptive text, a button row, an inline timestamp, a plain text ahead/behind line, and a status badge. Nothing groups the related pieces together. The result feels more like a placeholder than a useful at-a-glance dashboard.

---

## Goal

Redesign using existing shadcn components to produce a compact, data-dense, developer-focused layout (Bruno-inspired: monospace accents, minimal decoration, tight information hierarchy). All existing logic and dialogs stay untouched.

---

## Approved Design — Approach A: Status Card

### Overall Structure

Centered flex column. The primary UI is a `<Card>` at `w-full max-w-[320px]`. Below it, the existing status badge sits outside the card as a standalone row.

```
┌──────────────────────────────────────┐
│  ⑇ main              [↑2] [↓0]       │  CardHeader
├──────────────────────────────────────┤
│  [Fetch]  [Pull ↓0]  [Push ↑2]       │  CardContent, row 1
│  🕐 Last fetched: 12:34:01 PM        │  CardContent, row 2
└──────────────────────────────────────┘
[  ✓ Your branch is up to date  ]        Status badge (below card, unchanged)
```

### CardHeader

- Small `<GitBranch className="h-3.5 w-3.5 text-muted-foreground" />` icon
- Branch name from `status?.branch ?? 'no branch'` in `font-mono text-sm font-medium`
- Ahead/behind `<Badge variant="outline">` pushed to the right (`ml-auto flex gap-1.5`)
  - Ahead badge: `↑{ahead}` — `text-emerald-500` when 0, `text-amber-500` when > 0
  - Behind badge: `↓{behind}` — `text-emerald-500` when 0, `text-amber-500` when > 0
- `CardHeader` padding: `px-4 py-3`

### CardContent

Row 1 — action buttons (unchanged logic, layout unchanged):
```tsx
<div className="flex gap-2 mb-3">
  <Button ...>Fetch</Button>
  <Button ...>Pull ↓N</Button>
  <Button ...>Push ↑N</Button>
</div>
```

Row 2 — last fetched timestamp (unchanged logic):
```tsx
<p className="text-xs text-muted-foreground flex items-center gap-1.5">
  <Clock className="h-3.5 w-3.5" />
  Last fetched: <span className="font-medium text-foreground">{lastFetched ?? 'Never'}</span>
</p>
```

`CardContent` padding: `px-4 pb-4 pt-0`

### Status Badge (below card)

The existing status badge `<div className="flex items-center gap-1.5 text-xs border rounded-md px-3 py-1.5">` is kept as-is, moved to `mt-3` below the card.

---

## Removals

| Removed element | Reason |
|---|---|
| `<GitBranch className="h-12 w-12 text-muted-foreground/30" />` | Decorative filler; replaced by small icon in card header |
| `<p>Perform git actions or open files from sidebar to view</p>` | Redundant instruction text |
| `<p>↑ {ahead} Ahead | ↓ {behind} Behind</p>` | Replaced by ahead/behind Badge components in card header |

---

## New shadcn Imports

```tsx
import { Card, CardContent, CardHeader } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
```

All existing imports (`Button`, `AlertDialog` variants, lucide icons, `useGitStore`) are kept.

---

## Not Changed

- All handler functions (`handleFetch`, `handlePull`, `handlePush`, `handleStashAndPull`, `handlePullAnyway`, `handleFetchAndPush`, `handlePushAnyway`)
- Both `<AlertDialog>` components and their content
- `ahead`, `behind`, `isUpToDate` derived values
- State variables (`pushing`, `pulling`, `fetching`, `lastFetched`, `showStashDialog`, `showFetchFirstDialog`)
- Button icons and loading spinners

---

## Implementation Scope

Single file change: `src/components/git/GitLandingPanel.tsx`

No backend changes. No new components. No store changes.
