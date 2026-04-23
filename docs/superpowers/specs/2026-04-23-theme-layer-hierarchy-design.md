# Theme Layer Hierarchy Fix — Design Spec

**Date:** 2026-04-23  
**Status:** Approved  

---

## Problem

The current dark and light themes assign `--background` and `--card` to the wrong surfaces. As a result, the sidebar is *darker* than the main content area in both modes — an inverted depth that makes the UI feel flat and counterintuitive.

| Mode | Current (wrong) | Effect |
|------|----------------|--------|
| Dark | `--background: #1F1F1F` on content, `--card: #181818` on sidebar | Sidebar darker than content |
| Light | `--background: #FFFFFF` on content, `--card: #F8F8F8` on sidebar | Sidebar almost indistinguishable from content |

Modern developer tools (Linear, Raycast, Figma) follow a consistent layering rule: the **shell/chrome/sidebar sits at the base layer**, and the **main content panel is elevated** (lighter in light mode, slightly lighter in dark mode). This creates natural depth without needing shadows.

---

## Decision

**Swap the role assignments of `--background` and `--card` in both themes.**

All hex values remain 100% VSCode Modern palette — no new colors are introduced. Only what each token *means* in the layout changes.

---

## Token Changes

### Dark Theme (`.dark`)

| Token | Current value | Proposed value | VSCode source |
|-------|--------------|----------------|---------------|
| `--background` | `0 0% 12%` (#1F1F1F) | `0 0% 9.4%` (#181818) | `sideBar.background` |
| `--card` | `0 0% 9.4%` (#181818) | `0 0% 12%` (#1F1F1F) | `editor.background` |
| `--card-foreground` | `0 0% 80%` | `0 0% 80%` | unchanged |

All other dark tokens (`--foreground`, `--primary`, `--secondary`, `--muted`, `--muted-foreground`, `--accent`, `--border`, `--input`, `--ring`, `--popover`, `--destructive`, charts) are **unchanged**.

### Light Theme (`:root`)

| Token | Current value | Proposed value | VSCode source |
|-------|--------------|----------------|---------------|
| `--background` | `0 0% 100%` (#FFFFFF) | `0 0% 97.3%` (#F8F8F8) | `sideBar.background` |
| `--card` | `0 0% 97.3%` (#F8F8F8) | `0 0% 100%` (#FFFFFF) | `editor.background` |
| `--card-foreground` | `0 0% 23%` | `0 0% 23%` | unchanged |

All other light tokens are **unchanged**.

---

## Layout Role Mapping (after fix)

After the swap, `--background` (#181818 dark / #F8F8F8 light) is the base shell value and `--card` (#1F1F1F dark / #FFFFFF light) is the elevated content surface. Component color class usage maps as follows — **no component edits required**:

| Component | Class used | Role after fix |
|-----------|-----------|----------------|
| `App` root div | `bg-background` | base shell wrapper |
| `TitleBar` | `bg-background` | base chrome |
| `CollectionsSidebar` | `bg-card` | base sidebar — same tone as shell |
| `StatusBar` | `bg-card` | base chrome |
| `TabBar` | `bg-card` | base tab bar |
| Active tab | `bg-background/95` | elevated — lighter than tab bar ✓ |
| `PaneRenderer` content | inherits `bg-background` | base; individual pane panels use `bg-card` for elevated feel |
| Popovers / Dropdowns | `--popover` (#1F1F1F dark) | top layer, unchanged |

---

## Affected File

**One file changes:** `src/globals.css`

Specifically, lines 16–20 (light `:root`) and lines 59–63 (dark `.dark`) — the `--background` and `--card` HSL values are swapped between each other. Comment labels are updated to match the corrected mapping.

No Tailwind config changes. No component changes. No new CSS classes.

---

## Non-Goals

- No accent color changes (VSCode blue `#005FB8` / `#0078D4` kept as-is)
- No foreground/text color changes
- No border, muted, or input color changes
- No component refactoring
- No new design tokens

---

## Acceptance Criteria

1. In dark mode, the sidebar and title/status bar are visually darker than the main content panel.
2. In light mode, the sidebar and title/status bar are visibly grey while the content panel is white.
3. Active tabs appear to "lift" off the tab bar (slightly lighter than tab bar background).
4. All existing component color classes (`bg-card`, `bg-background`, `bg-muted`, etc.) continue to work without any component edits.
5. All VSCode Modern token hex values remain unchanged.
