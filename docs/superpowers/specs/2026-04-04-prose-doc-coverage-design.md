# Design: Complete `.prose-doc` Markdown Coverage

**Date:** 2026-04-04  
**Status:** Approved  
**File changed:** `src/globals.css` only

## Problem

The `.prose-doc` CSS class used by `WorkspaceOverviewTab` to render workspace documentation markdown is missing rules for four CommonMark/GFM element groups. Tailwind v4's preflight stylesheet resets many browser defaults (e.g. `del { text-decoration: inherit }`), so missing rules don't silently fall back to browser defaults — they render unstyled or broken.

The existing 18 rules cover: h1–h3, p, ul/ol/li, inline code, fenced code (pre + pre code), strong, em, hr, a, blockquote, and GFM tables.

## Gaps Identified

| Element | Markdown syntax | Problem |
|---|---|---|
| h4, h5, h6 | `#### … ######` | No size/weight rule — inherits body font at 0.75rem, indistinct from h3 |
| `<del>` | `~~strikethrough~~` (GFM) | Tailwind v4 preflight resets `text-decoration` to `inherit`; renders as plain text |
| Task list checkboxes | `- [x] / - [ ]` (GFM) | `ul:has(input[type="checkbox"])` still shows disc markers; checkbox renders at full browser size (~16px) |
| `<img>` | `![alt](url)` | No `max-width` — wide images overflow the narrow workspace documentation panel |

## Approach

**Option A chosen:** Add the missing CSS rules to `.prose-doc` in `src/globals.css`. No TypeScript changes, no dependency changes, `MarkdownEditor` (which uses Tailwind Typography `.prose`) is untouched.

## CSS Rules to Add

### h4 / h5 / h6

```css
.prose-doc h4 { font-size: 0.6875rem; font-weight: 600; margin: 0.5rem 0 0.25rem; }
.prose-doc h5 { font-size: 0.6875rem; font-weight: 600; margin: 0.5rem 0 0.25rem; color: hsl(var(--muted-foreground)); }
.prose-doc h6 { font-size: 0.6875rem; font-weight: 500; margin: 0.5rem 0 0.25rem; color: hsl(var(--muted-foreground)); }
```

h4/h5/h6 all sit at the same `xs` size (0.6875rem) as the surrounding body text. h4 stays at foreground colour; h5/h6 drop to `--muted-foreground` to create a visible hierarchy below h3 without introducing new font sizes.

### Strikethrough (`<del>`)

```css
.prose-doc del { text-decoration: line-through; }
```

Restores the browser default that Tailwind v4 preflight resets.

### Task list checkboxes

```css
.prose-doc ul:has(li > input[type="checkbox"]) { list-style: none; padding-left: 0.25rem; }
.prose-doc li:has(> input[type="checkbox"]) { display: flex; align-items: baseline; gap: 0.375rem; }
.prose-doc li > input[type="checkbox"] { width: 0.75rem; height: 0.75rem; flex-shrink: 0; }
```

`:has()` detects checkbox lists and suppresses the disc marker. The checkbox is pinned to `0.75rem` to match the `xs` text scale. `align-items: baseline` aligns the checkbox with the first line of multi-line items.

**Compatibility:** CSS `:has()` requires Chromium ≥ 105. Tauri bundles a modern WebView on all platforms and meets this requirement.

### Images

```css
.prose-doc img { max-width: 100%; height: auto; border-radius: 4px; margin-bottom: 0.5rem; }
```

Prevents wide images from overflowing the workspace documentation panel. `border-radius: 4px` matches the existing `pre` block radius.

## Scope Constraints

- Only `src/globals.css` is modified
- No changes to `MarkdownEditor.tsx`, `WorkspaceOverviewTab.tsx`, or any Rust code
- No new npm dependencies
- No Tailwind config changes
- The two-system situation (`prose-doc` vs Tailwind Typography `.prose`) is left as-is (Option A)

## Testing

Manual verification in the running app:
1. Open workspace documentation panel, add markdown using each new element type
2. Confirm h4/h5/h6 render smaller than h3 with the expected weight/colour hierarchy
3. Confirm `~~strikethrough~~` shows a line through the text  
4. Confirm `- [x]` and `- [ ]` task lists show checkboxes without disc markers
5. Confirm a wide `![img](url)` does not overflow the panel

No automated tests required — this is pure CSS with no logic.
