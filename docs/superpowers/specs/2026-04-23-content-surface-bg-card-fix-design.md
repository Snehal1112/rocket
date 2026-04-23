# Content Surface bg-card Fix — Design Spec

**Date:** 2026-04-23  
**Status:** Approved  

---

## Problem

After the `--background`/`--card` role swap, several components still use `bg-background` for content-level surfaces. Since `--background` is now the dark base layer (`#181818` dark / `#F8F8F8` light), these surfaces appear too dark/grey instead of elevated.

Two specific symptoms reported:
- Active tab is the same shade as the tab bar (no elevation)
- In light mode, active tab is lighter than expected vs inactive tabs (actually the reverse — active tab is now grey #F8F8F8 instead of white #FFFFFF)

---

## Rule

**Shell/chrome surfaces** (TitleBar, StatusBar, sidebar, tab bar background) → `bg-background` (base layer)  
**Content surfaces** (active tab, editor panels, dialogs, modals, response/contract/audit panels) → `bg-card` (elevated surface)

---

## Changes Required

All changes are class replacements only — `bg-background` → `bg-card` on content surfaces. No token changes, no layout changes.

### Active tab (2 places)

| File | Line | Change |
|------|------|--------|
| `src/components/panes/TabItem.tsx` | 60 | `bg-background/95` → `bg-card` |
| `src/components/panes/TabBar.tsx` | 104 | `bg-background/95` → `bg-card` |

### Response viewer toolbars

| File | Lines | Change |
|------|-------|--------|
| `src/components/response/ResponseBodyViewer.tsx` | 234, 284 | `bg-background` → `bg-card` |

### Contract tab panels

| File | Lines | Change |
|------|-------|--------|
| `src/components/contract/ContractTab.tsx` | 215, 247, 274, 298 | `bg-background` → `bg-card` |
| `src/components/contract/ContractTabTopBar.tsx` | 22 | `bg-background` → `bg-card` |

### Audit log panel

| File | Lines | Change |
|------|-------|--------|
| `src/components/audit/AuditLogTab.tsx` | 43, 107 | `bg-background` → `bg-card` |

### Dialogs / modals (shadcn/ui primitives)

| File | Change |
|------|--------|
| `src/components/ui/dialog.tsx` | `bg-background` → `bg-card` in DialogContent |
| `src/components/ui/alert-dialog.tsx` | `bg-background` → `bg-card` in AlertDialogContent |

---

## Surfaces NOT changed

- `src/components/title-bar/TitleBar.tsx` — `bg-background` is correct (shell chrome)
- `src/components/layout/StatusBar.tsx` — already uses `bg-card` (correct after layer swap — base chrome)
- `src/components/ui/input.tsx`, `select.tsx`, `textarea.tsx` — use `bg-background dark:bg-input/30` which is correct for form fields
- `src/components/ui/tabs.tsx` (shadcn tab trigger) — `data-[state=active]:bg-background` is for the pill-style tabs inside panels, not the editor tab bar — leave unchanged
- `src/components/layout/ConsolePanel.tsx` — `bg-background/60` is a subtle tint overlay, intentional

---

## Acceptance Criteria

1. Active tab is visually elevated above the tab bar in both dark and light modes.
2. In light mode, active tab is white (#FFFFFF), inactive tabs show on grey (#F8F8F8) tab bar.
3. In dark mode, active tab is #1F1F1F, tab bar is #181818.
4. Response body viewer toolbars match the content panel elevation.
5. Dialogs/alerts appear on a white (#FFFFFF light) or #1F1F1F (dark) surface.
6. No regressions in form input styling (inputs stay `bg-background dark:bg-input/30`).
