# Design System Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 4 design system gaps: remove inline styles for CSP compliance, add Monaco theme tokens, add custom font size tokens, and replace all arbitrary font size values.

**Architecture:** Add CSS custom properties and Tailwind theme extensions in `index.css`, then update component files to use them. Inline `style={}` props are replaced with CSS custom property patterns. Monaco hex colors move to CSS variables. Arbitrary `text-[10px]` / `text-[11px]` are replaced with semantic theme tokens `text-2xs` / `text-label`.

**Tech Stack:** React, TypeScript, Tailwind CSS v4 (`yarn tsc --noEmit` for verification, `yarn build` for final check)

**Spec:** `docs/superpowers/specs/2026-03-26-design-system-cleanup-design.md`

---

## File Map

| File | Role |
|---|---|
| `src/index.css` | Add Monaco theme variables, custom font size tokens |
| `src/components/ui/tree.tsx` | Replace inline `style` with CSS custom property |
| `src/App.tsx` | Replace inline `style` with CSS custom property |
| `src/components/request/RequestPanel.tsx` | Replace inline `style` with CSS custom property |
| `src/components/collections/MethodBreakdown.tsx` | Replace inline `style` with CSS custom property |
| `src/components/editor/useMonacoTheme.ts` | Read Monaco colors from CSS variables |
| Multiple component files | Replace `text-[10px]` with `text-2xs`, `text-[11px]` with `text-label` |

---

### Task 1: Remove inline styles from 4 components

**Files:**
- Modify: `src/components/ui/tree.tsx`
- Modify: `src/App.tsx`
- Modify: `src/components/request/RequestPanel.tsx`
- Modify: `src/components/collections/MethodBreakdown.tsx`

- [ ] **Step 1: Fix tree.tsx — replace inline paddingLeft**

Find (~line 114-121):
```tsx
          className={cn(
            "flex items-center gap-1 px-1 py-1 text-sm cursor-pointer",
            "hover:bg-accent/50",
            "data-[selected]:bg-accent/30",
            "data-[active]:border-l-2 data-[active]:border-primary data-[active]:bg-accent/60 data-[active]:text-accent-foreground",
          )}
          style={{ paddingLeft: `${(depth + 1) * 8}px` }}
```

Replace with:
```tsx
          className={cn(
            "flex items-center gap-1 px-1 py-1 text-sm cursor-pointer pl-[var(--tree-indent)]",
            "hover:bg-accent/50",
            "data-[selected]:bg-accent/30",
            "data-[active]:border-l-2 data-[active]:border-primary data-[active]:bg-accent/60 data-[active]:text-accent-foreground",
          )}
          style={{ '--tree-indent': `${(depth + 1) * 8}px` } as React.CSSProperties}
```

- [ ] **Step 2: Fix App.tsx — replace inline sidebar width**

Find (~line 24):
```tsx
            <div style={{ width: `${sidebarWidth}px` }} className="shrink-0">
```

Replace with:
```tsx
            <div style={{ '--sidebar-w': `${sidebarWidth}px` } as React.CSSProperties} className="w-[var(--sidebar-w)] shrink-0">
```

- [ ] **Step 3: Fix RequestPanel.tsx — replace inline height**

Find (~lines 140-142):
```tsx
      <div
        className="flex flex-col overflow-hidden bg-card/80"
        style={{ height: `${requestHeight}%`, minHeight: '20%', maxHeight: '80%' }}
      >
```

Replace with:
```tsx
      <div
        className="flex flex-col overflow-hidden bg-card/80 h-[var(--req-h)] min-h-[20%] max-h-[80%]"
        style={{ '--req-h': `${requestHeight}%` } as React.CSSProperties}
      >
```

- [ ] **Step 4: Fix MethodBreakdown.tsx — replace inline bar width**

Find (~lines 59-61):
```tsx
                  <div
                    className={cn('h-full rounded-full', color.bg)}
                    style={{ width: `${pct}%` }}
                  />
```

Replace with:
```tsx
                  <div
                    className={cn('h-full rounded-full w-[var(--bar-w)]', color.bg)}
                    style={{ '--bar-w': `${pct}%` } as React.CSSProperties}
                  />
```

- [ ] **Step 5: Verify no inline styles remain**

```bash
cd /home/numericlabs/data/Rust/Rocket && grep -rn "style={{" src/components src/App.tsx | grep -v "as React.CSSProperties"
```

Expected: no output (all remaining `style={}` use the CSS custom property pattern).

- [ ] **Step 6: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -20
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/ui/tree.tsx src/App.tsx src/components/request/RequestPanel.tsx src/components/collections/MethodBreakdown.tsx
git commit -m "fix: replace inline styles with CSS custom properties for CSP compliance"
```

---

### Task 2: Add Monaco theme CSS variables

**Files:**
- Modify: `src/index.css`
- Modify: `src/components/editor/useMonacoTheme.ts`

- [ ] **Step 1: Add Monaco CSS variables to index.css**

Find the `:root` block in `src/index.css`. Add these variables at the end, before the closing `}` (~after line 92):

```css
  /* Monaco editor syntax theme. */
  --monaco-bg: #f5f8fc;
  --monaco-fg: #1a1f36;
  --monaco-line-highlight: #eef2f9;
  --monaco-line-number: #9ca3af;
  --monaco-string: 16a34a;
  --monaco-number: d97706;
  --monaco-keyword: 7c3aed;
  --monaco-comment: 9ca3af;
  --monaco-type: 2563eb;
```

Find the `.dark` block. Add these variables at the end, before the closing `}` (~after line 129):

```css
  /* Monaco editor syntax theme. */
  --monaco-bg: #1f1f1f;
  --monaco-fg: #dedede;
  --monaco-line-highlight: #242424;
  --monaco-line-number: #666666;
  --monaco-string: 4ade80;
  --monaco-number: fbbf24;
  --monaco-keyword: a78bfa;
  --monaco-comment: 6b7280;
  --monaco-type: 60a5fa;
```

- [ ] **Step 2: Update useMonacoTheme.ts to read CSS variables**

Replace the entire contents of `src/components/editor/useMonacoTheme.ts` with:

```tsx
import { useEffect, useState } from 'react';
import type { Monaco } from '@monaco-editor/react';

// Reads a CSS variable from the document root.
function cssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

export function useMonacoTheme() {
  const [isDark, setIsDark] = useState(() =>
    document.documentElement.classList.contains('dark'),
  );

  useEffect(() => {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = (e: MediaQueryListEvent) => setIsDark(e.matches);
    mq.addEventListener('change', handler);

    // Also observe the document class for manual dark mode toggle.
    const observer = new MutationObserver(() => {
      setIsDark(document.documentElement.classList.contains('dark'));
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class'],
    });

    return () => {
      mq.removeEventListener('change', handler);
      observer.disconnect();
    };
  }, []);

  const themeName = isDark ? 'rocket-dark' : 'rocket-light';

  function defineThemes(monaco: Monaco) {
    const bg = cssVar('--monaco-bg');
    const fg = cssVar('--monaco-fg');
    const lineHighlight = cssVar('--monaco-line-highlight');
    const lineNumber = cssVar('--monaco-line-number');
    const str = cssVar('--monaco-string');
    const num = cssVar('--monaco-number');
    const kw = cssVar('--monaco-keyword');
    const comment = cssVar('--monaco-comment');
    const type = cssVar('--monaco-type');

    monaco.editor.defineTheme('rocket-light', {
      base: 'vs',
      inherit: true,
      rules: [
        { token: 'string', foreground: str },
        { token: 'number', foreground: num },
        { token: 'keyword', foreground: kw },
        { token: 'comment', foreground: comment },
        { token: 'type', foreground: type },
      ],
      colors: {
        'editor.background': bg,
        'editor.foreground': fg,
        'editor.lineHighlightBackground': lineHighlight,
        'editorLineNumber.foreground': lineNumber,
      },
    });

    monaco.editor.defineTheme('rocket-dark', {
      base: 'vs-dark',
      inherit: true,
      rules: [
        { token: 'string', foreground: str },
        { token: 'number', foreground: num },
        { token: 'keyword', foreground: kw },
        { token: 'comment', foreground: comment },
        { token: 'type', foreground: type },
      ],
      colors: {
        'editor.background': bg,
        'editor.foreground': fg,
        'editor.lineHighlightBackground': lineHighlight,
        'editorLineNumber.foreground': lineNumber,
      },
    });
  }

  return { themeName, defineThemes, isDark };
}
```

- [ ] **Step 3: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -20
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/index.css src/components/editor/useMonacoTheme.ts
git commit -m "feat: add Monaco theme CSS variables — no more hardcoded hex colors"
```

---

### Task 3: Add custom font size tokens and replace arbitrary sizes

**Files:**
- Modify: `src/index.css`
- Modify (bulk): All files using `text-[10px]` or `text-[11px]`

- [ ] **Step 1: Add font size tokens to index.css**

Find the `@theme inline` block in `src/index.css` (~line 10). Add after the `--font-mono` line (~line 12):

```css
  --font-size-2xs: 0.625rem;      /* 10px — micro text for badges, counts, kbd. */
  --line-height-2xs: 1rem;
  --font-size-label: 0.6875rem;   /* 11px — form labels, subtitles. */
  --line-height-label: 1rem;
```

This registers `text-2xs` and `text-label` as valid Tailwind utilities.

- [ ] **Step 2: Replace all `text-[10px]` with `text-2xs`**

Run this replacement across all TSX files that use `text-[10px]`:

Files to update (13 occurrences):
- `src/components/response/ResponseBodyViewer.tsx`
- `src/components/panes/EditorGroup.tsx`
- `src/components/panes/TabItem.tsx` (2 occurrences)
- `src/components/collections/CollectionOverviewTab.tsx`
- `src/components/collections/RequestNode.tsx`
- `src/components/collections/RequestList.tsx`
- `src/components/layout/CollectionsSidebar.tsx`
- `src/components/request/RequestPanel.tsx` (3 occurrences)
- `src/components/request/AuthEditor.tsx` (2 occurrences)

In every file, find `text-[10px]` and replace with `text-2xs`.

- [ ] **Step 3: Replace all `text-[11px]` with `text-label`**

Files to update (23 occurrences):
- `src/components/layout/Header.tsx`
- `src/components/request/AuthEditor.tsx` (21 occurrences)

In every file, find `text-[11px]` and replace with `text-label`.

- [ ] **Step 4: Verify no arbitrary font sizes remain**

```bash
cd /home/numericlabs/data/Rust/Rocket && grep -rn "text-\[1[01]px\]" src/
```

Expected: no output.

- [ ] **Step 5: Verify types and build**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit && yarn build 2>&1 | tail -10
```

Expected: both succeed.

- [ ] **Step 6: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/index.css src/components/
git commit -m "feat: add text-2xs and text-label font tokens; replace all arbitrary font sizes"
```

---

## Done

Design system cleanup complete:
- Zero inline `style={}` with direct CSS properties — all use CSS custom property pattern for CSP compliance
- Monaco editor colors read from CSS variables, following the main theme
- `text-[10px]` replaced with semantic `text-2xs` token (13 occurrences)
- `text-[11px]` replaced with semantic `text-label` token (23 occurrences)
- `yarn tsc --noEmit` and `yarn build` pass clean
