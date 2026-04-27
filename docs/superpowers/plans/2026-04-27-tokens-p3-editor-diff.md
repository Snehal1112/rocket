# VSCode 2026 Tokens — P3: Editor & Diff

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace hardcoded hex colors in Monaco editor config, CodeMirror theme, and VisualDiffView with CSS var–backed tokens from VSCode 2026.

**Architecture:** Add editor/diff CSS vars to `globals.css` and `tailwind.config.js`. Then update `monaco-config.ts` to reference CSS vars via `getComputedStyle`, update `theme.ts` (CodeMirror) to use CSS vars, and update `VisualDiffView.tsx` to use token-based Tailwind classes. Monaco reads theme colors at registration time, so we read CSS vars once when the theme is registered.

**Tech Stack:** Monaco Editor, CodeMirror 6, Tailwind CSS v4, CSS custom properties.

---

### Task 1: Add editor and diff CSS vars

**Files:**
- Modify: `src/globals.css`
- Modify: `tailwind.config.js`

- [ ] **Step 1: Add CSS vars to `:root`**

After the titlebar vars in `:root`, add:

```css
    /* VSCode 2026 Light: editor surface tokens */
    --editor-bg:                  0 0% 100%;     /* editor.background               #FFFFFF */
    --editor-line-highlight:      0 0% 91.8%;    /* editor.lineHighlightBackground   #EAEAEA */
    --editor-line-number-fg:      0 0% 37.6%;    /* editorLineNumber.foreground      #606060 */
    --editor-line-number-active:  0 0% 12.5%;    /* editorLineNumber.activeForeground #202020 */
    --editor-widget-bg:           240 50% 99%;   /* editorWidget.background          #FAFAFD */
    --editor-widget-border:       210 7% 90%;    /* editorWidget.border              #E4E5E6 */
    /* VSCode 2026 Light: gutter decoration tokens */
    --editor-gutter-added:        88 74% 26%;    /* editorGutter.addedBackground     #587c0c */
    --editor-gutter-deleted:      3 93% 35%;     /* editorGutter.deletedBackground   #ad0707 */
    /* VSCode 2026 Light: diff editor tokens */
    --diff-inserted-text:   rgba(88, 124, 12, 0.149);   /* diffEditor.insertedTextBackground #587c0c26 */
    --diff-removed-text:    rgba(173, 7, 7, 0.149);     /* diffEditor.removedTextBackground  #ad070726 */
    --diff-inserted-line:   rgba(88, 124, 12, 0.149);   /* diffEditor.insertedLineBackground (same as text light) */
    --diff-removed-line:    rgba(173, 7, 7, 0.149);     /* diffEditor.removedLineBackground  (same as text light) */
```

- [ ] **Step 2: Add CSS vars to `.dark`**

After the titlebar vars in `.dark`, add:

```css
    /* VSCode 2026 Dark: editor surface tokens */
    --editor-bg:                  210 5% 7.5%;   /* editor.background               #121314 */
    --editor-line-highlight:      210 3% 14.5%;  /* editor.lineHighlightBackground   #242526 */
    --editor-line-number-fg:      210 2% 53%;    /* editorLineNumber.foreground      #858889 */
    --editor-line-number-active:  210 1% 74%;    /* editorLineNumber.activeForeground #BBBEBF */
    --editor-widget-bg:           210 3% 13%;    /* editorWidget.background          #202122 */
    --editor-widget-border:       210 2% 17%;    /* editorWidget.border              #2A2B2C */
    /* VSCode 2026 Dark: gutter decoration tokens */
    --editor-gutter-added:        137 47% 69%;   /* editorGutter.addedBackground     #72C892 */
    --editor-gutter-deleted:      9 85% 70%;     /* editorGutter.deletedBackground   #F28772 */
    /* VSCode 2026 Dark: diff editor tokens */
    --diff-inserted-text:   rgba(87, 171, 90, 0.302);   /* diffEditor.insertedTextBackground #57ab5a4d */
    --diff-removed-text:    rgba(244, 112, 103, 0.302); /* diffEditor.removedTextBackground  #f470674d */
    --diff-inserted-line:   rgba(52, 125, 57, 0.149);   /* diffEditor.insertedLineBackground #347d3926 */
    --diff-removed-line:    rgba(201, 60, 55, 0.149);   /* diffEditor.removedLineBackground  #c93c3726 */
```

- [ ] **Step 3: Register in `tailwind.config.js`**

After the `titlebar` group, add:

```js
  			editor: {
  				bg:                  'hsl(var(--editor-bg))',
  				'line-highlight':    'hsl(var(--editor-line-highlight))',
  				'line-number-fg':    'hsl(var(--editor-line-number-fg))',
  				'widget-bg':         'hsl(var(--editor-widget-bg))',
  				'widget-border':     'hsl(var(--editor-widget-border))',
  				'gutter-added':      'hsl(var(--editor-gutter-added))',
  				'gutter-deleted':    'hsl(var(--editor-gutter-deleted))',
  			},
  			diff: {
  				'inserted-text': 'var(--diff-inserted-text)',
  				'removed-text':  'var(--diff-removed-text)',
  				'inserted-line': 'var(--diff-inserted-line)',
  				'removed-line':  'var(--diff-removed-line)',
  			},
```

- [ ] **Step 4: Verify**

```bash
yarn tsc --noEmit
```

Expected: no output.

- [ ] **Step 5: Commit**

```bash
git add src/globals.css tailwind.config.js
git commit -m "feat(theme): add editor/diff CSS vars from VSCode 2026"
```

---

### Task 2: Update Monaco config to use CSS vars

**Files:**
- Modify: `src/components/editor/monaco-config.ts`

Monaco reads theme colors at registration time (not reactively), so we resolve CSS vars once using `getComputedStyle(document.documentElement)` when building the theme objects. The file already has correct hardcoded values — we add a helper to read from CSS vars at runtime so the theme respects whatever `globals.css` defines.

- [ ] **Step 1: Add a CSS var reader helper at the top of the file**

After the imports and `BASE_EDITOR_OPTIONS`, before `ROCKET_LIGHT_THEME`, add:

```ts
/** Read a CSS custom property as a hex-compatible string for Monaco. */
function cssVar(name: string): string {
  if (typeof document === 'undefined') return '#000000';
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

/** Convert an HSL CSS var (e.g. "210 5% 7.5%") to a hex color for Monaco. */
function cssVarHex(name: string): string {
  const val = cssVar(name);
  if (!val) return '#000000';
  // Already a raw rgba/hex value (scrollbar vars etc.)
  if (val.startsWith('#') || val.startsWith('rgb')) return val;
  // HSL triplet: "H S% L%" → build hsl() and convert
  const [h, s, l] = val.split(' ').map((v) => Number.parseFloat(v));
  // Convert HSL to hex via canvas trick
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 1;
  const ctx = canvas.getContext('2d');
  if (!ctx) return '#000000';
  ctx.fillStyle = `hsl(${h},${s}%,${l}%)`;
  ctx.fillRect(0, 0, 1, 1);
  const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data;
  return `#${r.toString(16).padStart(2, '0')}${g.toString(16).padStart(2, '0')}${b.toString(16).padStart(2, '0')}`;
}
```

- [ ] **Step 2: Update `ROCKET_LIGHT_THEME` colors block to use `cssVarHex`**

Find the `colors:` object inside `ROCKET_LIGHT_THEME` and replace all surface/editor colors with CSS var calls. Replace the entire `colors:` block:

```ts
  colors: {
    'editor.background': cssVarHex('--editor-bg'),
    'editor.foreground': '#202020',
    'editorGutter.background': cssVarHex('--editor-bg'),
    'editorWidget.background': cssVarHex('--editor-widget-bg'),
    'editorWidget.border': cssVarHex('--editor-widget-border'),
    'editorWidget.foreground': '#202020',
    'editorSuggestWidget.background': cssVarHex('--editor-widget-bg'),
    'editorSuggestWidget.border': cssVarHex('--editor-widget-border'),
    'editorSuggestWidget.foreground': '#202020',
    'editorSuggestWidget.highlightForeground': '#0069CC',
    'editorSuggestWidget.selectedBackground': '#0069CC26',
    'editorHoverWidget.background': cssVarHex('--editor-widget-bg'),
    'editorHoverWidget.border': cssVarHex('--editor-widget-border'),
    'editorCursor.foreground': '#202020',
    'editor.selectionBackground': '#0069CC40',
    'editor.inactiveSelectionBackground': '#0069CC1A',
    'editor.selectionHighlightBackground': '#0069CC15',
    'editor.wordHighlightBackground': '#0069CC26',
    'editor.wordHighlightStrongBackground': '#0069CC26',
    'editor.lineHighlightBackground': cssVarHex('--editor-line-highlight') + '40',
    'editor.findMatchBackground': '#0069CC40',
    'editor.findMatchHighlightBackground': '#0069CC1A',
    'editor.findRangeHighlightBackground': cssVarHex('--editor-line-highlight'),
    'editor.rangeHighlightBackground': cssVarHex('--editor-line-highlight'),
    'editor.hoverHighlightBackground': cssVarHex('--editor-line-highlight'),
    'editorLineNumber.foreground': cssVarHex('--editor-line-number-fg'),
    'editorLineNumber.activeForeground': cssVarHex('--editor-line-number-active'),
    'editorIndentGuide.background1': '#F7F7F7',
    'editorIndentGuide.activeBackground1': '#EEEEEE',
    'editorBracketMatch.background': '#0069CC40',
    'editorBracketMatch.border': '#F0F1F2',
    'scrollbar.shadow': '#00000000',
    'scrollbarSlider.background': '#99999926',
    'scrollbarSlider.hoverBackground': '#99999940',
    'scrollbarSlider.activeBackground': '#99999955',
    'minimapSlider.background': '#99999926',
    'minimapSlider.hoverBackground': '#99999940',
    'minimapSlider.activeBackground': '#99999955',
    'diffEditor.insertedTextBackground': '#587c0c26',
    'diffEditor.removedTextBackground': '#ad070726',
    'peekView.border': '#0069CC',
    'peekViewEditor.background': cssVarHex('--editor-widget-bg'),
    'peekViewEditor.matchHighlightBackground': '#0069CC33',
    'peekViewResult.background': cssVarHex('--editor-widget-bg'),
    'peekViewResult.fileForeground': '#202020',
    'peekViewResult.lineForeground': '#606060',
    'peekViewResult.matchHighlightBackground': '#0069CC33',
    'peekViewResult.selectionBackground': '#0069CC26',
    'peekViewResult.selectionForeground': '#202020',
    'peekViewTitle.background': cssVarHex('--editor-widget-bg'),
    'peekViewTitleDescription.foreground': '#606060',
    'peekViewTitleLabel.foreground': '#202020',
    'editorGutter.addedBackground': cssVarHex('--editor-gutter-added'),
    'editorGutter.deletedBackground': cssVarHex('--editor-gutter-deleted'),
  },
```

- [ ] **Step 3: Update `ROCKET_DARK_THEME` colors block similarly**

Find the `colors:` object inside `ROCKET_DARK_THEME`. Replace with:

```ts
  colors: {
    'editor.background': cssVarHex('--editor-bg'),
    'editor.foreground': '#BBBEBF',
    'editorGutter.background': cssVarHex('--editor-bg'),
    'editorStickyScroll.background': cssVarHex('--editor-bg'),
    'editorStickyScrollHover.background': cssVarHex('--editor-widget-bg'),
    'editorWidget.background': cssVarHex('--editor-widget-bg'),
    'editorWidget.border': cssVarHex('--editor-widget-border'),
    'editorWidget.foreground': '#bfbfbf',
    'editorSuggestWidget.background': cssVarHex('--editor-widget-bg'),
    'editorSuggestWidget.border': cssVarHex('--editor-widget-border'),
    'editorSuggestWidget.foreground': '#bfbfbf',
    'editorSuggestWidget.highlightForeground': '#bfbfbf',
    'editorSuggestWidget.selectedBackground': '#3994BC26',
    'editorHoverWidget.background': cssVarHex('--editor-widget-bg'),
    'editorHoverWidget.border': cssVarHex('--editor-widget-border'),
    'editorCursor.foreground': '#BBBEBF',
    'editor.selectionBackground': '#276782dd',
    'editor.inactiveSelectionBackground': '#27678260',
    'editor.selectionHighlightBackground': '#27678260',
    'editor.wordHighlightBackground': '#27678250',
    'editor.wordHighlightStrongBackground': '#27678280',
    'editor.lineHighlightBackground': cssVarHex('--editor-line-highlight'),
    'editor.findMatchBackground': '#27678290',
    'editor.findMatchHighlightBackground': '#27678280',
    'editor.findRangeHighlightBackground': cssVarHex('--editor-line-highlight'),
    'editor.rangeHighlightBackground': cssVarHex('--editor-line-highlight'),
    'editor.hoverHighlightBackground': cssVarHex('--editor-line-highlight'),
    'editorLineNumber.foreground': cssVarHex('--editor-line-number-fg'),
    'editorLineNumber.activeForeground': cssVarHex('--editor-line-number-active'),
    'editorIndentGuide.background1': '#8384854D',
    'editorIndentGuide.activeBackground1': '#838485',
    'editorBracketMatch.background': '#3994BC55',
    'editorBracketMatch.border': '#2A2B2C',
    'scrollbar.shadow': '#191B1D4D',
    'scrollbarSlider.background': '#83848533',
    'scrollbarSlider.hoverBackground': '#83848566',
    'scrollbarSlider.activeBackground': '#83848599',
    'minimapSlider.background': '#83848533',
    'minimapSlider.hoverBackground': '#83848566',
    'minimapSlider.activeBackground': '#83848599',
    'diffEditor.insertedTextBackground': '#57ab5a4d',
    'diffEditor.removedTextBackground': '#f470674d',
    'diffEditor.insertedLineBackground': '#347d3926',
    'diffEditor.removedLineBackground': '#c93c3726',
    'peekView.border': '#2A2B2C',
    'peekViewEditor.background': cssVarHex('--editor-bg'),
    'peekViewEditor.matchHighlightBackground': '#3994BC33',
    'peekViewResult.background': cssVarHex('--editor-bg'),
    'peekViewResult.fileForeground': '#bfbfbf',
    'peekViewResult.lineForeground': '#8C8C8C',
    'peekViewResult.matchHighlightBackground': '#3994BC33',
    'peekViewResult.selectionBackground': '#3994BC26',
    'peekViewResult.selectionForeground': '#bfbfbf',
    'peekViewTitle.background': cssVarHex('--editor-widget-bg'),
    'peekViewTitleDescription.foreground': '#8C8C8C',
    'peekViewTitleLabel.foreground': '#bfbfbf',
    'editorGutter.addedBackground': cssVarHex('--editor-gutter-added'),
    'editorGutter.deletedBackground': cssVarHex('--editor-gutter-deleted'),
  },
```

- [ ] **Step 4: Verify**

```bash
yarn tsc --noEmit
```

Expected: no output.

- [ ] **Step 5: Commit**

```bash
git add src/components/editor/monaco-config.ts
git commit -m "feat(theme): wire Monaco editor theme to VSCode 2026 CSS vars"
```

---

### Task 3: Update CodeMirror theme to use CSS vars

**Files:**
- Modify: `src/components/editor/extensions/theme.ts`

- [ ] **Step 1: Replace hardcoded variable badge colors with CSS vars**

In `src/components/editor/extensions/theme.ts`, find the hardcoded color values and replace with `hsl(var(--...))` references. Find:

```ts
    background: 'color-mix(in srgb, #b69500 15%, transparent)',
    color: '#b69500', // VSCode 2026 Light: notificationsWarningIcon
```

Replace with:

```ts
    background: 'hsl(var(--warning) / 0.15)',
    color: 'hsl(var(--warning))',
```

Find:

```ts
    background: 'color-mix(in srgb, #606060 12%, transparent)',
    color: '#606060', // VSCode 2026 Light: descriptionForeground
```

Replace with:

```ts
    background: 'hsl(var(--muted-foreground) / 0.12)',
    color: 'hsl(var(--muted-foreground))',
```

Find:

```ts
    background: 'color-mix(in srgb, #0069cc 12%, transparent)',
    color: '#0069cc', // VSCode 2026 Light: button.background / focusBorder
```

Replace with:

```ts
    background: 'hsl(var(--primary) / 0.12)',
    color: 'hsl(var(--primary))',
```

Find:

```ts
    background: 'color-mix(in srgb, #587c0c 12%, transparent)',
    color: '#587c0c', // VSCode 2026 Light: gitDecoration.addedResourceForeground
```

Replace with (both occurrences):

```ts
    background: 'hsl(var(--git-added) / 0.12)',
    color: 'hsl(var(--git-added))',
```

Find:

```ts
    background: 'color-mix(in srgb, #0069cc 10%, transparent)',
    color: '#0069cc', // VSCode 2026 Light: primary
```

Replace with:

```ts
    background: 'hsl(var(--primary) / 0.10)',
    color: 'hsl(var(--primary))',
```

Find:

```ts
    background: 'color-mix(in srgb, #652d90 12%, transparent)',
    color: '#652d90', // VSCode 2026 Light: charts.purple
```

Replace with (both occurrences):

```ts
    background: 'hsl(var(--chart-4) / 0.12)',
    color: 'hsl(var(--chart-4))',
```

- [ ] **Step 2: Verify**

```bash
yarn tsc --noEmit && yarn check 2>&1 | grep theme.ts
```

Expected: no output.

- [ ] **Step 3: Commit**

```bash
git add src/components/editor/extensions/theme.ts
git commit -m "feat(theme): wire CodeMirror variable badge colors to CSS vars"
```

---

### Task 4: Update VisualDiffView to use token-based diff classes

**Files:**
- Modify: `src/components/git/VisualDiffView.tsx`

- [ ] **Step 1: Replace hardcoded Tailwind diff colors**

In `src/components/git/VisualDiffView.tsx`, find:

```ts
  added:    'bg-green-50 dark:bg-green-950/20',
  removed:  'bg-red-50 dark:bg-red-950/20',
  modified: 'bg-amber-50 dark:bg-amber-950/20',
```

Replace with:

```ts
  added:    'bg-diff-inserted-line',
  removed:  'bg-diff-removed-line',
  modified: 'bg-diff-inserted-line',
```

Also find the inline `className` usages of the same patterns on `<tr>` elements:

```tsx
        <tr className='bg-red-50 dark:bg-red-950/20'>
```

```tsx
        <tr className='bg-green-50 dark:bg-green-950/20'>
```

Replace with:

```tsx
        <tr className='bg-diff-removed-line'>
```

```tsx
        <tr className='bg-diff-inserted-line'>
```

- [ ] **Step 2: Verify**

```bash
yarn tsc --noEmit && yarn check 2>&1 | grep VisualDiffView
```

Expected: no output.

- [ ] **Step 3: Commit**

```bash
git add src/components/git/VisualDiffView.tsx
git commit -m "feat(theme): wire VisualDiffView to VSCode 2026 diff tokens"
```
