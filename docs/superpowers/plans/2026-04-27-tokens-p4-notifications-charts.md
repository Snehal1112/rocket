# VSCode 2026 Tokens — P4: Notifications & Charts

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add notification icon color tokens and remaining chart color tokens to Rocket's design system, and wire the CodeMirror theme's hardcoded notification-like colors to use them.

**Architecture:** Add CSS vars to `globals.css`, register in `tailwind.config.js`, update `theme.ts` (CodeMirror) which has hardcoded `#b69500` notification warning colors, and extend `colors.ts` chart palette with the remaining VSCode 2026 chart tokens.

**Tech Stack:** Tailwind CSS v4, CSS custom properties.

---

### Task 1: Add notification and chart CSS vars

**Files:**
- Modify: `src/globals.css`
- Modify: `tailwind.config.js`

- [ ] **Step 1: Add CSS vars to `:root`**

After the diff vars in `:root`, add:

```css
    /* VSCode 2026 Light: notification icon tokens */
    --notification-warning-fg: 49 100% 36%;   /* notificationsWarningIcon.foreground #B69500 */
    --notification-error-fg:   3 93% 35%;     /* notificationsErrorIcon.foreground   #ad0707 */
    --notification-info-fg:    210 100% 40%;  /* notificationsInfoIcon.foreground    #0069CC */
    /* VSCode 2026 Light: remaining chart tokens */
    --chart-orange: 35 90% 46%;   /* charts.orange   #d18616 */
    --chart-green:  113 44% 37%;  /* charts.green    #388A34 */
    --chart-purple: 281 48% 37%;  /* charts.purple   #652D90 */
```

- [ ] **Step 2: Add CSS vars to `.dark`**

After the diff vars in `.dark`, add:

```css
    /* VSCode 2026 Dark: notification icon tokens */
    --notification-warning-fg: 45 100% 40%;   /* notificationsWarningIcon.foreground #CCA700 */
    --notification-error-fg:   9 85% 70%;     /* notificationsErrorIcon.foreground   #f48771 */
    --notification-info-fg:    199 47% 54%;   /* notificationsInfoIcon.foreground    #3a94bc */
    /* VSCode 2026 Dark: remaining chart tokens */
    --chart-orange: 34 67% 46%;   /* charts.orange   #CD861A */
    --chart-green:  120 33% 67%;  /* charts.green    #86CF86 */
    --chart-purple: 274 53% 67%;  /* charts.purple   #AD80D7 */
```

- [ ] **Step 3: Register in `tailwind.config.js`**

After the `diff` group, add:

```js
  			notification: {
  				'warning-fg': 'hsl(var(--notification-warning-fg))',
  				'error-fg':   'hsl(var(--notification-error-fg))',
  				'info-fg':    'hsl(var(--notification-info-fg))',
  			},
  			'chart-orange':  'hsl(var(--chart-orange))',
  			'chart-green':   'hsl(var(--chart-green))',
  			'chart-purple':  'hsl(var(--chart-purple))',
```

- [ ] **Step 4: Verify**

```bash
yarn tsc --noEmit
```

Expected: no output.

- [ ] **Step 5: Commit**

```bash
git add src/globals.css tailwind.config.js
git commit -m "feat(theme): add notification and chart CSS vars from VSCode 2026"
```

---

### Task 2: Update CodeMirror theme notification-warning color

**Files:**
- Modify: `src/components/editor/extensions/theme.ts`

The CodeMirror theme already has a hardcoded `#b69500` for variable badge warning color. This was partially addressed in P3, but let's verify and confirm it uses `hsl(var(--warning))` which maps to `--notification-warning-fg` (same value `#B69500` light).

- [ ] **Step 1: Verify P3 wired the warning color correctly**

```bash
grep -n "b69500\|warning\|B69500" src/components/editor/extensions/theme.ts
```

Expected: no `#b69500` or `#B69500` hardcoded values — they should all reference `hsl(var(--warning))` after P3. If any remain, replace them:

```ts
    background: 'hsl(var(--warning) / 0.15)',
    color: 'hsl(var(--warning))',
```

- [ ] **Step 2: Verify**

```bash
yarn tsc --noEmit && yarn check 2>&1 | grep theme.ts
```

Expected: no output.

- [ ] **Step 3: Commit (only if changes were needed)**

```bash
git add src/components/editor/extensions/theme.ts
git commit -m "fix(theme): ensure CodeMirror warning color uses CSS var"
```

---

### Task 3: Extend chart color palette in colors.ts

**Files:**
- Modify: `src/lib/colors.ts`

The existing `METHOD_CHART_COLOR` uses HTTP-method-specific Tailwind colors. The VSCode 2026 chart tokens (`charts.orange`, `charts.green`, `charts.purple`) are generic palette entries useful for future chart components. Add them as named exports.

- [ ] **Step 1: Add chart palette exports to `colors.ts`**

At the end of `src/lib/colors.ts`, add:

```ts
// ── VSCode 2026 Chart Palette ───────────────────────────────────────

/** Named chart colors from VSCode 2026 token set — use for non-method chart elements. */
export const CHART_COLORS = {
  blue:   'text-chart-1',
  green:  'text-chart-green',
  orange: 'text-chart-orange',
  purple: 'text-chart-purple',
  red:    'text-chart-5',
  yellow: 'text-chart-3',
} as const;
```

- [ ] **Step 2: Verify**

```bash
yarn tsc --noEmit && yarn check 2>&1 | grep colors.ts
```

Expected: no output.

- [ ] **Step 3: Commit**

```bash
git add src/lib/colors.ts
git commit -m "feat(theme): add VSCode 2026 chart palette to colors.ts"
```
