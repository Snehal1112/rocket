# Spec: Git Right Panel — Back to Overview Navigation

**Date:** 2026-04-01
**Status:** Draft
**Depends on:** SP4 Git Polish (Plans 7–8 complete — two-panel GitPanel layout)

---

## Problem

In the `GitPanel` two-panel layout, the right panel switches between four views: **landing** (overview with Fetch/Pull/Push), **diff** (file changes), **commits** (log), and **stashes**. Once the user navigates away from the landing state — by clicking a file, or clicking "Commits" / "Stashes" in the Links section — there is **no visible way to return to the landing/overview panel**.

Bruno solves a similar problem with a "← Back to Overview" link (see screenshot), but their architecture is different — they replace the entire page with a file diff view. RocketAPI's two-panel approach is better (file list stays visible), but still lacks a back affordance.

### What users expect

- Click a changed file → diff appears in right panel ✓
- Click "Commits" link → commit log appears ✓
- **Want to get back to the overview (Fetch/Pull/Push, ahead/behind status) → no affordance ✗**

The only current workaround is clicking a different file (which just changes the diff) or knowing to click Links items. There is no way to reach the landing panel again without reloading the tab.

---

## Solution

Add a **contextual breadcrumb header** at the top of the right panel whenever the view is not `landing`. This header contains:

1. **"← Overview" ghost button** — resets `rightPanel` state to `{ kind: 'landing' }`
2. **Vertical separator** — visual divider
3. **Context label** — shows what is currently displayed:
   - For `diff`: the file path (e.g., `collections/tyyy/opencollection.yml`)
   - For `commits`: "Commit History"
   - For `stashes`: "Stashes"

### Visual design

```
┌─────────────────────────────────────────────────────┐
│  ← Overview  │  collections/tyyy/opencollection.yml │
├─────────────────────────────────────────────────────┤
│                                                     │
│              (diff / commits / stashes)             │
│                                                     │
└─────────────────────────────────────────────────────┘
```

The header is a single `<div>` row with:
- `Button` (shadcn, variant `ghost`, size `sm`) containing `ArrowLeft` Lucide icon + "Overview" text
- `Separator` (shadcn, orientation `vertical`)
- `<span>` with the truncated context label

The header uses `border-b border-border/70` to visually separate from content below.

### Interaction details

- Clicking "← Overview" sets `rightPanel` to `{ kind: 'landing' }`, showing the `GitLandingPanel`
- The header does **not** appear when `rightPanel.kind === 'landing'` (no back navigation needed on the default view)
- Keyboard: the button is a standard focusable shadcn `Button`, so Tab + Enter works naturally

### What this does NOT change

- Left panel layout — unchanged
- `GitLandingPanel` component — unchanged
- `DiffViewForFile`, `GitCommitLog`, `GitStashSection` — unchanged
- No new components created — this is purely a layout change within `GitPanel.tsx`

---

## Changes

### Single file: `src/components/git/GitPanel.tsx`

1. **Add import** for `ArrowLeft` from `lucide-react`
2. **Add import** for `Separator` from `@/components/ui/separator`
3. **Wrap the right panel** `<div>` to use `flex flex-col` layout
4. **Add conditional breadcrumb header** when `rightPanel.kind !== 'landing'`
5. **Wrap existing right panel content** in a `flex-1 overflow-hidden` div

The change is approximately 15–20 lines of JSX added to the right panel section of `GitPanel.tsx`.

---

## Constraints

- All UI uses shadcn/ui primitives (`Button`, `Separator`)
- Icon from Lucide React (`ArrowLeft`)
- No new components — inline in `GitPanel.tsx`
- No state changes beyond setting `rightPanel` (already exists)
