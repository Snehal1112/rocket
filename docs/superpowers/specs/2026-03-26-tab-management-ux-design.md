# Area B: Tab Management UX — Reference Spec

**Date:** 2026-03-26
**Status:** Reference — not yet scheduled for implementation
**Goal:** Improve tab bar usability for power users with many open tabs.

## Improvements

### 1. Tab tooltips for truncated titles
Show full request name + collection path on hover when title is truncated (max-w-[190px]).

### 2. Scroll-to-active-tab
When a tab is activated (via tree click or keyboard shortcut), auto-scroll the tab bar to ensure the active tab is visible. Use `element.scrollIntoView({ behavior: 'smooth', block: 'nearest' })`.

### 3. Tab overflow indicator
Add subtle left/right fade gradients when tabs overflow the container, indicating scrollable content.

### 4. Move-to-group context menu
Add "Move to..." submenu to tab context menu, listing available pane groups. Uses the existing `moveTab` store action (already implemented, just not wired to UI).

### 5. Tab reordering (stretch)
Drag-and-drop tab reordering within a group. Low priority — requires either a DnD library or custom pointer event handling.

## Files

| File | Changes |
|---|---|
| `src/components/panes/TabItem.tsx` | Add title tooltip |
| `src/components/panes/TabBar.tsx` | Scroll-to-active, overflow indicators, move-to-group menu |
| `src/stores/pane-store.ts` | No changes needed (moveTab already exists) |
