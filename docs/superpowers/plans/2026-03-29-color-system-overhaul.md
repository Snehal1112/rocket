# Color System Overhaul Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a centralized color constants file, unify HTTP method colors to jewel tones with dark mode variants, add dark mode to status/git colors, and improve base theme text contrast.

**Architecture:** Create `src/lib/colors.ts` with all color mappings, then update each component to import from it instead of defining inline constants. Finally adjust CSS variables in `src/index.css` for better text contrast. Each task is one commit.

**Tech Stack:** TypeScript, React, Tailwind CSS v4

**Spec:** `docs/superpowers/specs/2026-03-29-color-system-overhaul-design.md`

---

## Task 1: Create centralized color constants file

**Files:**
- Create: `src/lib/colors.ts`

- [ ] **Step 1: Create `src/lib/colors.ts`**

```typescript
import type { HttpMethod } from '@/types/shared-types';

// ── HTTP Method Colors (jewel tones) ────────────────────────────────

/** Text-only color for method labels in compact contexts (tree nodes, tabs). */
export const METHOD_TEXT_COLOR: Record<string, string> = {
  GET:     'text-emerald-500 dark:text-emerald-400',
  POST:    'text-amber-500 dark:text-amber-400',
  PUT:     'text-blue-500 dark:text-blue-400',
  PATCH:   'text-violet-500 dark:text-violet-400',
  DELETE:  'text-red-500 dark:text-red-400',
  OPTIONS: 'text-cyan-500 dark:text-cyan-400',
  HEAD:    'text-pink-500 dark:text-pink-400',
};

/** Full badge color (text + bg + border) for method badges with backgrounds. */
export const METHOD_BADGE_COLOR: Record<string, string> = {
  GET:     'text-emerald-500 dark:text-emerald-400 border-emerald-500/30 bg-emerald-500/10 dark:bg-emerald-500/20',
  POST:    'text-amber-500 dark:text-amber-400 border-amber-500/30 bg-amber-500/10 dark:bg-amber-500/20',
  PUT:     'text-blue-500 dark:text-blue-400 border-blue-500/30 bg-blue-500/10 dark:bg-blue-500/20',
  PATCH:   'text-violet-500 dark:text-violet-400 border-violet-500/30 bg-violet-500/10 dark:bg-violet-500/20',
  DELETE:  'text-red-500 dark:text-red-400 border-red-500/30 bg-red-500/10 dark:bg-red-500/20',
  OPTIONS: 'text-cyan-500 dark:text-cyan-400 border-cyan-500/30 bg-cyan-500/10 dark:bg-cyan-500/20',
  HEAD:    'text-pink-500 dark:text-pink-400 border-pink-500/30 bg-pink-500/10 dark:bg-pink-500/20',
};

/** Solid bg color for chart bars and progress indicators. */
export const METHOD_CHART_COLOR: Record<string, { text: string; bg: string }> = {
  GET:     { text: 'text-emerald-500 dark:text-emerald-400', bg: 'bg-emerald-500' },
  POST:    { text: 'text-amber-500 dark:text-amber-400',   bg: 'bg-amber-500' },
  PUT:     { text: 'text-blue-500 dark:text-blue-400',     bg: 'bg-blue-500' },
  PATCH:   { text: 'text-violet-500 dark:text-violet-400', bg: 'bg-violet-500' },
  DELETE:  { text: 'text-red-500 dark:text-red-400',       bg: 'bg-red-500' },
  OPTIONS: { text: 'text-cyan-500 dark:text-cyan-400',     bg: 'bg-cyan-500' },
  HEAD:    { text: 'text-pink-500 dark:text-pink-400',     bg: 'bg-pink-500' },
};

// ── HTTP Status Code Colors ─────────────────────────────────────────

/** Text-only color for status codes in compact contexts. */
export function statusTextColor(status: number): string {
  if (status >= 500 || status === 0) return 'text-red-600 dark:text-red-400';
  if (status >= 400) return 'text-amber-600 dark:text-amber-400';
  if (status >= 300) return 'text-blue-600 dark:text-blue-400';
  if (status >= 200) return 'text-emerald-600 dark:text-emerald-400';
  return 'text-muted-foreground';
}

/** Full badge color for status code badges (text + bg + border). */
export function statusBadgeColor(status: number): string {
  if (status >= 500) return 'bg-red-100 text-red-700 border-red-200 dark:bg-red-900/30 dark:text-red-400 dark:border-red-800';
  if (status >= 400) return 'bg-amber-100 text-amber-700 border-amber-200 dark:bg-amber-900/30 dark:text-amber-400 dark:border-amber-800';
  if (status >= 300) return 'bg-blue-100 text-blue-700 border-blue-200 dark:bg-blue-900/30 dark:text-blue-400 dark:border-blue-800';
  if (status >= 200) return 'bg-emerald-100 text-emerald-700 border-emerald-200 dark:bg-emerald-900/30 dark:text-emerald-400 dark:border-emerald-800';
  return 'bg-red-100 text-red-700 border-red-200 dark:bg-red-900/30 dark:text-red-400 dark:border-red-800';
}

/** Color for response time indicators. */
export function timeColor(ms: number): string {
  if (ms <= 200) return 'text-emerald-600 dark:text-emerald-400';
  if (ms <= 1000) return 'text-amber-600 dark:text-amber-400';
  return 'text-red-600 dark:text-red-400';
}

// ── Git Status Colors ───────────────────────────────────────────────

export type GitStatusKind = 'modified' | 'added' | 'deleted' | 'renamed' | 'untracked' | 'conflicted' | 'unchanged';

export const GIT_STATUS_CONFIG: Record<GitStatusKind, { label: string; className: string }> = {
  modified:   { label: 'M', className: 'bg-amber-500/20 text-amber-400 border-amber-500/30' },
  added:      { label: 'A', className: 'bg-green-500/20 text-green-400 border-green-500/30' },
  deleted:    { label: 'D', className: 'bg-red-500/20 text-red-400 border-red-500/30' },
  renamed:    { label: 'R', className: 'bg-blue-500/20 text-blue-400 border-blue-500/30' },
  untracked:  { label: '?', className: 'bg-gray-500/20 text-gray-400 border-gray-500/30' },
  conflicted: { label: 'C', className: 'bg-red-700/20 text-red-300 border-red-700/30' },
  unchanged:  { label: '', className: '' },
};
```

- [ ] **Step 2: Verify it compiles**

Run: `npx tsc --noEmit`
Expected: No errors. (If `HttpMethod` or `GitStatusKind` imports fail, check the actual type location and adjust — read `src/types/shared-types.ts` or grep for the type.)

- [ ] **Step 3: Commit**

```bash
git add src/lib/colors.ts
git commit -m "feat: add centralized color constants for methods, statuses, and git"
```

---

## Task 2: Migrate sidebar and tree components to centralized colors

**Files:**
- Modify: `src/components/collections/RequestNode.tsx`
- Modify: `src/components/collections/RequestList.tsx`
- Modify: `src/components/collections/MethodBreakdown.tsx`

- [ ] **Step 1: Update RequestNode.tsx**

Remove the inline `METHOD_COLOR` constant (lines 35–43):
```typescript
// DELETE this entire block:
const METHOD_COLOR: Record<string, string> = {
  GET:     'text-emerald-500',
  POST:    'text-amber-500',
  PUT:     'text-blue-500',
  PATCH:   'text-violet-500',
  DELETE:  'text-red-500',
  OPTIONS: 'text-cyan-500',
  HEAD:    'text-pink-500',
};
```

Add import at the top:
```typescript
import { METHOD_TEXT_COLOR } from '@/lib/colors';
```

Replace all usages of `METHOD_COLOR[` with `METHOD_TEXT_COLOR[` in the file.

- [ ] **Step 2: Update RequestList.tsx**

Remove the inline `METHOD_COLORS` constant (lines 17–25).

Add import:
```typescript
import { METHOD_BADGE_COLOR } from '@/lib/colors';
```

Replace all usages of `METHOD_COLORS[` with `METHOD_BADGE_COLOR[` in the file.

- [ ] **Step 3: Update MethodBreakdown.tsx**

Remove the inline `METHOD_COLORS` constant (lines 10–18).

Add import:
```typescript
import { METHOD_CHART_COLOR } from '@/lib/colors';
```

Replace all usages of `METHOD_COLORS[` with `METHOD_CHART_COLOR[` in the file. Note the shape is the same (`{ text: string; bg: string }`), so `.text` and `.bg` accesses stay unchanged.

- [ ] **Step 4: Verify**

Run: `npx tsc --noEmit`
Expected: No errors.

- [ ] **Step 5: Commit**

```bash
git add src/components/collections/RequestNode.tsx src/components/collections/RequestList.tsx src/components/collections/MethodBreakdown.tsx
git commit -m "refactor: migrate sidebar/tree components to centralized color constants"
```

---

## Task 3: Migrate tab bar and request panel to centralized colors

**Files:**
- Modify: `src/components/panes/TabItem.tsx`
- Modify: `src/components/request/RequestPanel.tsx`

- [ ] **Step 1: Update TabItem.tsx**

Remove the inline `METHOD_TEXT_COLORS` constant (lines 6–14):
```typescript
// DELETE this entire block:
const METHOD_TEXT_COLORS: Record<HttpMethod, string> = {
  GET: 'text-green-600',
  POST: 'text-blue-600',
  PUT: 'text-orange-600',
  PATCH: 'text-yellow-600',
  DELETE: 'text-red-600',
  OPTIONS: 'text-gray-500',
  HEAD: 'text-gray-500',
};
```

Add import:
```typescript
import { METHOD_TEXT_COLOR } from '@/lib/colors';
```

Replace all usages of `METHOD_TEXT_COLORS[` with `METHOD_TEXT_COLOR[` in the file. Note the key type may change from `HttpMethod` to `string` — `METHOD_TEXT_COLOR` uses `Record<string, string>`, so bracket access works the same way. If TypeScript complains about the key type, cast with `tab.request.method as string` or use `METHOD_TEXT_COLOR[tab.request.method] ?? ''`.

- [ ] **Step 2: Update RequestPanel.tsx**

Remove the inline `METHOD_COLORS` constant (lines 49–57):
```typescript
// DELETE this entire block:
const METHOD_COLORS: Record<HttpMethod, string> = {
  GET: 'text-green-600 dark:text-green-400',
  POST: 'text-blue-600 dark:text-blue-400',
  PUT: 'text-orange-600 dark:text-orange-400',
  PATCH: 'text-yellow-600 dark:text-yellow-400',
  DELETE: 'text-red-600 dark:text-red-400',
  OPTIONS: 'text-gray-500',
  HEAD: 'text-gray-400',
};
```

Add import:
```typescript
import { METHOD_TEXT_COLOR } from '@/lib/colors';
```

Replace all usages of `METHOD_COLORS[` with `METHOD_TEXT_COLOR[` in the file.

- [ ] **Step 3: Verify**

Run: `npx tsc --noEmit`
Expected: No errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/panes/TabItem.tsx src/components/request/RequestPanel.tsx
git commit -m "refactor: migrate tab bar and request panel to centralized color constants"
```

---

## Task 4: Migrate history, console, and response panels to centralized colors

**Files:**
- Modify: `src/components/history/HistoryPanel.tsx`
- Modify: `src/components/layout/ConsolePanel.tsx`
- Modify: `src/components/response/ResponseBodyViewer.tsx`

- [ ] **Step 1: Update HistoryPanel.tsx**

Remove the inline `statusColor` function (lines 26–32) and `methodColor` function (lines 35–44).

Add imports:
```typescript
import { METHOD_TEXT_COLOR, statusTextColor } from '@/lib/colors';
```

Replace usages:
- `statusColor(...)` → `statusTextColor(...)`
- `methodColor(method)` → `METHOD_TEXT_COLOR[method.toUpperCase()] ?? 'text-muted-foreground'`

- [ ] **Step 2: Update ConsolePanel.tsx**

Remove the inline `statusColor` function (lines 17–23).

Add import:
```typescript
import { statusTextColor } from '@/lib/colors';
```

Replace usages of `statusColor(...)` → `statusTextColor(...)`.

- [ ] **Step 3: Update ResponseBodyViewer.tsx**

Remove the inline `statusClasses` function (lines 76–82) and `timeClasses` function (lines 85–89).

Add imports:
```typescript
import { statusBadgeColor, timeColor } from '@/lib/colors';
```

Replace usages:
- `statusClasses(...)` → `statusBadgeColor(...)`
- `timeClasses(...)` → `timeColor(...)`

- [ ] **Step 4: Verify**

Run: `npx tsc --noEmit`
Expected: No errors.

- [ ] **Step 5: Commit**

```bash
git add src/components/history/HistoryPanel.tsx src/components/layout/ConsolePanel.tsx src/components/response/ResponseBodyViewer.tsx
git commit -m "refactor: migrate history, console, and response to centralized color functions"
```

---

## Task 5: Migrate GitStatusBadge to centralized colors

**Files:**
- Modify: `src/components/git/GitStatusBadge.tsx`

- [ ] **Step 1: Update GitStatusBadge.tsx**

Remove the inline `statusConfig` constant (lines 4–12).

Remove the local `GitStatusKind` type import if it exists, and import from colors instead.

Add import:
```typescript
import { GIT_STATUS_CONFIG } from '@/lib/colors';
```

Replace all usages of `statusConfig[` with `GIT_STATUS_CONFIG[`.

If the component imports `GitStatusKind` from elsewhere (e.g., `@/types/...`), keep that import — the `GIT_STATUS_CONFIG` just uses the same type. If `GitStatusKind` was defined locally in this file, import it from `@/lib/colors` instead.

- [ ] **Step 2: Verify**

Run: `npx tsc --noEmit`
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/git/GitStatusBadge.tsx
git commit -m "refactor: migrate GitStatusBadge to centralized color constants"
```

---

## Task 6: Add dark mode variants to SandboxPopover and EnvironmentSwitcher

**Files:**
- Modify: `src/components/layout/SandboxPopover.tsx`
- Modify: `src/components/layout/EnvironmentSwitcher.tsx`

- [ ] **Step 1: Update SandboxPopover.tsx**

Read the file. Add `dark:` variants to all hardcoded color classes. Apply these replacements:

- `text-green-500` → `text-green-500 dark:text-green-400`
- `text-amber-500` → `text-amber-500 dark:text-amber-400`
- `border-green-500` → `border-green-500 dark:border-green-400`
- `border-amber-500` → `border-amber-500 dark:border-amber-400`
- `bg-green-500/5` → `bg-green-500/5 dark:bg-green-500/10`
- `bg-amber-500/5` → `bg-amber-500/5 dark:bg-amber-500/10`
- `bg-green-500` (solid radio dot) → `bg-green-500 dark:bg-green-400`
- `bg-amber-500` (solid radio dot) → `bg-amber-500 dark:bg-amber-400`
- `text-green-600` → `text-green-600 dark:text-green-400`
- `text-amber-600` → `text-amber-600 dark:text-amber-400`
- `bg-green-500/10` (badge bg) → `bg-green-500/10 dark:bg-green-500/20`
- `bg-amber-500/10` (badge bg) → `bg-amber-500/10 dark:bg-amber-500/20`

Do NOT change the `hover:border-green-500/50` and `hover:border-amber-500/50` — hover states are fine as-is.

- [ ] **Step 2: Update EnvironmentSwitcher.tsx**

Find the active environment dot color (line ~41):
```typescript
activeEnvId ? 'bg-green-500' : 'bg-muted-foreground/50'
```

Change to:
```typescript
activeEnvId ? 'bg-green-500 dark:bg-green-400' : 'bg-muted-foreground/50'
```

- [ ] **Step 3: Verify**

Run: `npx tsc --noEmit`
Expected: No errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/layout/SandboxPopover.tsx src/components/layout/EnvironmentSwitcher.tsx
git commit -m "style: add dark mode variants to SandboxPopover and EnvironmentSwitcher"
```

---

## Task 7: Improve base theme text contrast in index.css

**Files:**
- Modify: `src/index.css`

- [ ] **Step 1: Update light theme muted-foreground**

In `:root` (around line 74), change:
```css
--muted-foreground: hsl(215 17% 40%);
```
to:
```css
--muted-foreground: hsl(215 20% 35%);
```

- [ ] **Step 2: Update dark theme foreground and muted-foreground**

In `.dark` (around lines 113, 123), change:
```css
--foreground: hsl(0 0% 87%);
```
to:
```css
--foreground: hsl(0 0% 90%);
```

And change:
```css
--muted-foreground: hsl(0 0% 60%);
```
to:
```css
--muted-foreground: hsl(0 0% 65%);
```

- [ ] **Step 3: Verify**

Run: `npx tsc --noEmit`
Expected: No errors.

Run: `npx vitest run`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/index.css
git commit -m "style: improve text contrast for light and dark theme foreground colors"
```
