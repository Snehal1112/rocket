# Design: Color System Overhaul

**Date:** 2026-03-29
**Status:** Approved

## Problem

1. HTTP method colors are defined independently in 5+ files with conflicting values (GET is emerald in the tree but green-600 in the panel; POST is amber in one place, blue in another).
2. Only 2 of 12 files with hardcoded colors have `dark:` overrides. The rest use single-mode colors that look wrong in the opposite theme.
3. No centralized color constants file exists — every component defines its own inline color maps.
4. The base theme text colors could have better contrast, especially `--muted-foreground`.

## Scope

### Part A: Centralized color constants

Create `src/lib/colors.ts` as the single source of truth for all semantic color mappings:
- HTTP method colors (text, background, border)
- HTTP status code colors
- Git status colors

Each mapping provides both light and dark variants in a single string using Tailwind's `dark:` prefix.

### Part B: Unified HTTP method palette (jewel tones)

Standardize all method colors to the jewel tone palette:

| Method | Text | Background | Border |
|---|---|---|---|
| GET | `text-emerald-500 dark:text-emerald-400` | `bg-emerald-500/10 dark:bg-emerald-500/20` | `border-emerald-500/30` |
| POST | `text-amber-500 dark:text-amber-400` | `bg-amber-500/10 dark:bg-amber-500/20` | `border-amber-500/30` |
| PUT | `text-blue-500 dark:text-blue-400` | `bg-blue-500/10 dark:bg-blue-500/20` | `border-blue-500/30` |
| PATCH | `text-violet-500 dark:text-violet-400` | `bg-violet-500/10 dark:bg-violet-500/20` | `border-violet-500/30` |
| DELETE | `text-red-500 dark:text-red-400` | `bg-red-500/10 dark:bg-red-500/20` | `border-red-500/30` |
| OPTIONS | `text-cyan-500 dark:text-cyan-400` | `bg-cyan-500/10 dark:bg-cyan-500/20` | `border-cyan-500/30` |
| HEAD | `text-pink-500 dark:text-pink-400` | `bg-pink-500/10 dark:bg-pink-500/20` | `border-pink-500/30` |

The `dark:` variant bumps from 500 to 400 for better readability on dark backgrounds. Background opacity increases from 10% to 20% in dark mode for visibility.

### Part C: Status code colors with dark mode

| Range | Text | Background | Border |
|---|---|---|---|
| 2xx | `text-emerald-600 dark:text-emerald-400` | `bg-emerald-100 dark:bg-emerald-900/30` | `border-emerald-200 dark:border-emerald-800` |
| 3xx | `text-blue-600 dark:text-blue-400` | `bg-blue-100 dark:bg-blue-900/30` | `border-blue-200 dark:border-blue-800` |
| 4xx | `text-amber-600 dark:text-amber-400` | `bg-amber-100 dark:bg-amber-900/30` | `border-amber-200 dark:border-amber-800` |
| 5xx / 0 | `text-red-600 dark:text-red-400` | `bg-red-100 dark:bg-red-900/30` | `border-red-200 dark:border-red-800` |

### Part D: Git status colors with dark mode

Already consistent in GitStatusBadge.tsx — extract into colors.ts as-is and import from there.

### Part E: Improved base theme text contrast

Adjust these CSS variables in `src/index.css`:

**Light theme:**
- `--foreground`: `hsl(222 47% 11%)` stays (already dark enough)
- `--muted-foreground`: `hsl(215 20% 35%)` (was `hsl(215 17% 40%)` — slightly darker for better readability)
- `--card-foreground`: stays same as `--foreground`

**Dark theme:**
- `--foreground`: `hsl(0 0% 90%)` (was `hsl(0 0% 87%)` — slightly brighter for better contrast)
- `--muted-foreground`: `hsl(0 0% 65%)` (was `hsl(0 0% 60%)` — brighter for readability)

### Part F: Sandbox/environment indicator colors

SandboxPopover's green-500/amber-500 colors and EnvironmentSwitcher's green-500 dot are semantic UI indicators. Add `dark:` variants:
- Safe mode: `text-green-500 dark:text-green-400`, borders/backgrounds follow same pattern
- Developer mode: `text-amber-500 dark:text-amber-400`
- Active env dot: `bg-green-500 dark:bg-green-400`

## Files to modify

**Create:**
- `src/lib/colors.ts` — centralized color constants

**Modify (import from colors.ts, remove inline constants):**
- `src/components/collections/RequestNode.tsx`
- `src/components/collections/RequestList.tsx`
- `src/components/collections/MethodBreakdown.tsx`
- `src/components/panes/TabItem.tsx`
- `src/components/request/RequestPanel.tsx`
- `src/components/history/HistoryPanel.tsx`
- `src/components/layout/ConsolePanel.tsx`
- `src/components/response/ResponseBodyViewer.tsx`
- `src/components/git/GitStatusBadge.tsx`
- `src/components/layout/SandboxPopover.tsx`
- `src/components/layout/EnvironmentSwitcher.tsx`

**Modify (theme values):**
- `src/index.css` — adjust muted-foreground and foreground values

## What Does NOT Change

- The overall HSL-based theme architecture stays.
- Primary, secondary, accent, destructive tokens stay.
- Monaco editor syntax colors stay (they already have proper light/dark variants).
- Sidebar colors stay.
- Font families and sizes stay.

## Testing

- `yarn tsc --noEmit` — no errors.
- `yarn test` — all tests pass.
- Visual: toggle between light/dark mode and verify all method badges, status badges, git badges, and sandbox indicators look correct in both themes.
