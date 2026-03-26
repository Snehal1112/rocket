# Design System Cleanup — Spec

**Date:** 2026-03-26
**Branch:** feat/ux-workflows
**Goal:** Fix 4 concrete design system gaps: remove inline styles for CSP compliance, consolidate spacing values, add Monaco theme tokens, and clean up arbitrary font sizes.

## Current State (Audit Summary)

The app already uses Tailwind CSS v4.2.2 with a well-structured HSL-based token system (32 light + 32 dark variables), zero hardcoded colors in TSX components, full dark mode support, and a single CSS file. Score: 8.5/10.

## Fix 1 — Remove inline `style={}` for CSP compliance

4 inline style usages exist. Convert each to a CSS custom property approach.

### `src/components/ui/tree.tsx:120`

Current: `style={{ paddingLeft: \`${(depth + 1) * 8}px\` }}`

Fix: Set a CSS custom property and reference it in Tailwind.

```tsx
// Set the custom property on the element.
style={{ '--tree-indent': `${(depth + 1) * 8}px` } as React.CSSProperties}
// Use it in className.
className={cn("... pl-[var(--tree-indent)]")}
```

Remove the `style` prop entirely and use Tailwind arbitrary value `pl-[var(--tree-indent)]` with the CSS variable set inline. This is CSP-safe because CSS custom properties in `style` attributes are allowed by most CSP policies (they don't apply styling directly — the class does).

### `src/App.tsx:24`

Current: `style={{ width: \`${sidebarWidth}px\` }}`

Fix: Same pattern — set `--sidebar-w` as CSS custom property, use `w-[var(--sidebar-w)]` in className.

### `src/components/request/RequestPanel.tsx:142`

Current: `style={{ height: \`${requestHeight}%\`, minHeight: '20%', maxHeight: '80%' }}`

Fix: Set `--req-h` as CSS custom property, use `h-[var(--req-h)] min-h-[20%] max-h-[80%]` in className.

### `src/components/collections/MethodBreakdown.tsx:61`

Current: `style={{ width: \`${pct}%\` }}`

Fix: Set `--bar-w` as CSS custom property, use `w-[var(--bar-w)]` in className.

---

## Fix 2 — Consolidate spacing values

65 unique spacing values is too many. Establish guidelines and normalize.

### Icon size scale (3 sizes only)

| Purpose | Size | Classes |
|---|---|---|
| Small (tree icons, inline indicators) | 12px | `h-3 w-3` |
| Default (menu icons, action buttons) | 14px | `h-3.5 w-3.5` |
| Medium (standalone icons, headers) | 16px | `h-4 w-4` |

Audit all icon sizes across TSX files. Replace `h-5 w-5`, `h-6 w-6`, `h-7 w-7`, `h-8 w-8` icons with the nearest standard size unless they serve a distinct purpose (e.g., empty state illustrations).

### Content padding scale

| Context | Horizontal | Vertical |
|---|---|---|
| Tight (tree rows, tabs) | `px-1` or `px-2` | `py-0.5` or `py-1` |
| Standard (cards, panels) | `px-4` | `py-3` or `py-4` |
| Spacious (dialogs, hero sections) | `px-6` | `py-6` |

### Gap scale

| Size | Use case |
|---|---|
| `gap-1` | Tight items (tree row elements) |
| `gap-2` | Default (form fields, button groups) |
| `gap-4` | Sections within a panel |
| `gap-6` | Major layout sections |

Audit and normalize `gap-*` values. Remove `gap-1.5`, `gap-3`, `gap-5` unless clearly needed.

---

## Fix 3 — Monaco theme tokens

Move 12 hardcoded hex colors from `src/components/editor/useMonacoTheme.ts` into CSS custom properties in `src/index.css`.

### New CSS variables (add to `:root` and `.dark`)

```css
:root {
  --monaco-bg: 245 248 252;          /* #f5f8fc */
  --monaco-fg: 31 41 55;             /* gray-800 */
  --monaco-line-number: 156 163 175; /* gray-400 */
  --monaco-string: 22 163 74;        /* green-600 */
  --monaco-number: 217 119 6;        /* amber-600 */
  --monaco-keyword: 124 58 237;      /* violet-600 */
  --monaco-comment: 156 163 175;     /* gray-400 */
  --monaco-type: 37 99 235;          /* blue-600 */
}

.dark {
  --monaco-bg: 31 31 31;             /* #1f1f1f */
  --monaco-fg: 212 212 212;          /* gray-300 */
  --monaco-line-number: 102 102 102; /* #666 */
  --monaco-string: 74 222 128;       /* green-400 */
  --monaco-number: 251 191 36;       /* amber-400 */
  --monaco-keyword: 167 139 250;     /* violet-400 */
  --monaco-comment: 107 114 128;     /* gray-500 */
  --monaco-type: 96 165 250;         /* blue-400 */
}
```

Then in `useMonacoTheme.ts`, read these values from the computed style and convert to hex for Monaco's API.

---

## Fix 4 — Arbitrary font sizes

36 instances of `text-[Npx]`. Normalize:

| Current | Replace with |
|---|---|
| `text-[10px]` | `text-[0.625rem]` (keep as arbitrary but use rem for accessibility) |
| `text-[11px]` | `text-xs` (12px — close enough, standardize up) |
| `text-[13px]` | `text-sm` (14px — standardize up) |

Where exact pixel sizes are required for design precision (e.g., method labels at 10px), keep the arbitrary value but use rem units.

---

## Files Changed

| File | Changes |
|---|---|
| `src/components/ui/tree.tsx` | Replace inline style with CSS custom property |
| `src/App.tsx` | Replace inline style with CSS custom property |
| `src/components/request/RequestPanel.tsx` | Replace inline style with CSS custom property |
| `src/components/collections/MethodBreakdown.tsx` | Replace inline style with CSS custom property |
| `src/index.css` | Add Monaco theme CSS variables |
| `src/components/editor/useMonacoTheme.ts` | Read Monaco colors from CSS variables |
| Multiple component files | Normalize icon sizes, spacing, and font sizes |

## Out of Scope

- Color palette changes (already well-tokenized)
- Dark/light mode toggle (already working)
- Component API changes
- New UI components
- Visual redesign or new aesthetic direction
