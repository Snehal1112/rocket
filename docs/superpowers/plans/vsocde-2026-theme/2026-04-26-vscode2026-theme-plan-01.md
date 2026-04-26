# VSCode 2026 Theme — Plan 01: Update `globals.css` + Fix CM6 Dark Mode

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the existing VSCode Modern HSL values in `src/globals.css` with VSCode 2026 values, and fix the CM6 variable-token highlight colors so they are readable in dark mode.

**Architecture:** Pure value replacement in two files. No component edits. No new tokens. No Tailwind config changes. The `--background` / `--card` layer hierarchy (shell vs content panel) established by `2026-04-23-theme-layer-hierarchy` is preserved.

**Tech Stack:** CSS custom properties, Tailwind CSS, CodeMirror 6 `EditorView.theme()`

**Spec:** `docs/superpowers/specs/2026-04-26-vscode2026-theme-design.md`
**Branch:** `feat/vscode-2026-theme` (created in Chunk 0 below — Plans 02 and 03 continue on this same branch)

---

## Chunk 0: Worktree setup

> **Skill invoked:** `superpowers:using-git-worktrees`

- [ ] **Step 1: Check for existing worktree directory**

```bash
ls -d .worktrees 2>/dev/null && echo "found .worktrees" || \
ls -d worktrees  2>/dev/null && echo "found worktrees"  || \
echo "none found"
```

If a directory is found, use it. If neither exists, check `CLAUDE.md` for a preference:

```bash
grep -i "worktree" CLAUDE.md 2>/dev/null || echo "no preference in CLAUDE.md"
```

If no directory and no preference, use `.worktrees/` (project-local, hidden).

- [ ] **Step 2: Verify the worktree directory is git-ignored**

```bash
git check-ignore -q .worktrees && echo "ignored OK" || echo "NOT ignored — fix needed"
```

If **not ignored**, add it and commit before proceeding:

```bash
echo ".worktrees/" >> .gitignore
git add .gitignore
git commit -m "chore: add .worktrees to .gitignore"
```

- [ ] **Step 3: Create the worktree on a new branch**

```bash
git worktree add .worktrees/vscode-2026-theme -b feat/vscode-2026-theme
cd .worktrees/vscode-2026-theme
```

- [ ] **Step 4: Install frontend dependencies in the worktree**

```bash
cd frontend
yarn install
cd ..
```

- [ ] **Step 5: Verify a clean baseline — TypeScript must pass**

```bash
cd frontend
yarn tsc --noEmit
```

Expected: no errors. If errors exist, **stop and report them** — do not proceed until the baseline is clean.

```bash
cd ..
```

- [ ] **Step 6: Confirm worktree is ready**

```bash
git branch --show-current
# Expected output: feat/vscode-2026-theme
pwd
# Expected: ends in .worktrees/vscode-2026-theme
```

All subsequent tasks in Plans 01, 02, and 03 run inside this worktree directory.

---

## Task 1: Update `:root` (light) tokens in `src/globals.css`

**Files:**
- Modify: `frontend/src/globals.css`

- [ ] **Step 1: Confirm current light token values before editing**

```bash
cd frontend
grep -n "\-\-background\|\-\-card\|\-\-foreground\|\-\-primary\|\-\-border\|\-\-ring\|\-\-destructive\|\-\-muted\|\-\-accent\|\-\-popover\|\-\-secondary\|\-\-input" src/globals.css | head -30
```

Note the line numbers. You will replace specific lines in the `:root` block.

- [ ] **Step 2: Replace the entire `:root` block contents**

Find the `:root { ... }` block inside `@layer base`. Replace **only the CSS variable values**, preserving the comment style. The new `:root` block must be:

```css
  :root {
    /* VSCode 2026 Light: sideBar.background #FAFAFD — shell/sidebar base */
    --background: 240 20% 99%;
    /* VSCode 2026 Light: foreground #202020 */
    --foreground: 0 0% 12.5%;
    /* VSCode 2026 Light: editor.background #FFFFFF — elevated content panel */
    --card: 0 0% 100%;
    --card-foreground: 0 0% 12.5%;
    /* VSCode 2026 Light: editorWidget.background #FAFAFD */
    --popover: 240 20% 99%;
    --popover-foreground: 0 0% 12.5%;
    /* VSCode 2026 Light: button.background #0069CC */
    --primary: 208 100% 40%;
    --primary-foreground: 0 0% 100%;
    /* VSCode 2026 Light: button.secondaryBackground #EAEAEA */
    --secondary: 0 0% 91.8%;
    --secondary-foreground: 0 0% 12.5%;
    /* VSCode 2026 Light: editorStickyScrollHover #F0F0F3 */
    --muted: 240 18% 95%;
    /* VSCode 2026 Light: descriptionForeground #606060 */
    --muted-foreground: 0 0% 37.6%;
    /* VSCode 2026 Light: list.activeSelectionBackground #0069CC1A */
    --accent: 208 100% 40% / 10%;
    --accent-foreground: 0 0% 12.5%;
    /* VSCode 2026 Light: errorForeground #ad0707 */
    --destructive: 0 89% 35%;
    --destructive-foreground: 0 0% 100%;
    /* VSCode 2026 Light: dropdown.border #D8D8D8 */
    --border: 0 0% 84.7%;
    /* VSCode 2026 Light: input.border #D8D8D866 */
    --input: 0 0% 84.7%;
    /* VSCode 2026 Light: focusBorder #0069CC */
    --ring: 208 100% 40%;
    /* VSCode 2026 Light: notificationsWarningIcon #B69500 */
    --warning: 48 100% 35.7%;
    --warning-foreground: 0 0% 100%;
    /* Chart colors from 2026 Light charts.* tokens */
    --chart-1: 208 100% 40%;
    --chart-2: 85 77% 27%;
    --chart-3: 48 100% 35.7%;
    --chart-4: 280 60% 44%;
    --chart-5: 0 89% 35%;
    --radius: 0.7rem;
    --font-mono: "JetBrains Mono", ui-monospace, monospace;
  }
```

- [ ] **Step 3: Verify the light block was applied correctly**

```bash
grep -n "\-\-primary\|\-\-background\|\-\-card\b\|\-\-foreground\b\|\-\-border\b\|\-\-ring\b" src/globals.css | head -20
```

Expected output must include:
```
--background: 240 20% 99%;
--card: 0 0% 100%;
--primary: 208 100% 40%;
--border: 0 0% 84.7%;
--ring: 208 100% 40%;
```

- [ ] **Step 4: Commit**

```bash
git add src/globals.css
git commit -m "feat(theme): update light theme to VSCode 2026 Light palette

Maps sideBar.background (#FAFAFD) → --background (shell)
Maps editor.background (#FFFFFF) → --card (content)
Maps button.background (#0069CC) → --primary
Maps focusBorder (#0069CC) → --ring
Maps errorForeground (#ad0707) → --destructive"
```

---

## Task 2: Update `.dark` tokens in `src/globals.css`

**Files:**
- Modify: `frontend/src/globals.css`

- [ ] **Step 1: Confirm current dark token block location**

```bash
grep -n "\.dark" src/globals.css
```

Note the line number of `.dark {`. You will replace its contents.

- [ ] **Step 2: Replace the entire `.dark` block contents**

Find the `.dark { ... }` block inside `@layer base`. Replace **only the CSS variable values**. The new `.dark` block must be:

```css
  .dark {
    /* VSCode 2026 Dark: sideBar.background #191A1B — shell/sidebar base */
    --background: 210 4% 10%;
    /* VSCode 2026 Dark: foreground #bfbfbf */
    --foreground: 0 0% 74.9%;
    /* VSCode 2026 Dark: editor.background #121314 — elevated content panel */
    --card: 210 5% 7.8%;
    --card-foreground: 0 0% 74.9%;
    /* VSCode 2026 Dark: editorWidget.background #202122 */
    --popover: 210 3% 12.9%;
    --popover-foreground: 0 0% 74.9%;
    /* VSCode 2026 Dark: button.background #297AA0 */
    --primary: 201 61% 39.6%;
    --primary-foreground: 0 0% 100%;
    /* VSCode 2026 Dark: list.inactiveSelectionBackground #2C2D2E */
    --secondary: 210 2% 17.3%;
    --secondary-foreground: 0 0% 74.9%;
    /* VSCode 2026 Dark: textBlockQuote.background #242526 */
    --muted: 210 2% 15.1%;
    /* VSCode 2026 Dark: descriptionForeground #8C8C8C */
    --muted-foreground: 0 0% 54.9%;
    /* VSCode 2026 Dark: list.activeSelectionBackground #3994BC26 */
    --accent: 201 54% 47.5% / 15%;
    --accent-foreground: 0 0% 74.9%;
    /* VSCode 2026 Dark: errorForeground #f48771 */
    --destructive: 13 87% 70%;
    --destructive-foreground: 0 0% 100%;
    /* VSCode 2026 Dark: dropdown.border #333536 */
    --border: 210 2.4% 20.2%;
    /* VSCode 2026 Dark: input.border #333536 */
    --input: 210 2.4% 20.2%;
    /* VSCode 2026 Dark: focusBorder #3994BC */
    --ring: 201 53% 47.5%;
    /* VSCode 2026 Dark: notificationsWarningIcon #CCA700 */
    --warning: 48 100% 40%;
    --warning-foreground: 0 0% 10%;
    /* Chart colors from 2026 Dark charts.* tokens */
    --chart-1: 201 61% 39.6%;
    --chart-2: 143 44% 62%;
    --chart-3: 37 67% 69%;
    --chart-4: 278 55% 66%;
    --chart-5: 13 87% 70%;
  }
```

- [ ] **Step 3: Verify the dark block was applied correctly**

```bash
grep -n "\-\-primary\|\-\-background\|\-\-card\b\|\-\-foreground\b\|\-\-border\b\|\-\-ring\b" src/globals.css | tail -20
```

Expected output must include:
```
--background: 210 4% 10%;
--card: 210 5% 7.8%;
--primary: 201 61% 39.6%;
--border: 210 2.4% 20.2%;
--ring: 201 53% 47.5%;
```

- [ ] **Step 4: Run TypeScript and build checks**

```bash
yarn tsc --noEmit
```

Expected: no errors.

```bash
yarn build
```

Expected: build succeeds with no errors.

- [ ] **Step 5: Commit**

```bash
git add src/globals.css
git commit -m "feat(theme): update dark theme to VSCode 2026 Dark palette

Maps sideBar.background (#191A1B) → --background (shell)
Maps editor.background (#121314) → --card (content)
Maps button.background (#297AA0) → --primary
Maps focusBorder (#3994BC) → --ring
Maps errorForeground (#f48771) → --destructive"
```

---

## Task 3: Fix CM6 variable-token dark mode colors

**Files:**
- Modify: `src/components/editor/cm6-theme.ts` (find exact path with grep below)

- [ ] **Step 1: Locate the CM6 theme file**

```bash
grep -rn "rocketTheme\|cm-var-environment\|EditorView.theme" src/ --include="*.ts" --include="*.tsx" -l
```

Note all matching files. Open the `.ts` file containing `EditorView.theme` — that is the primary theme definition.

- [ ] **Step 2: Audit current dark mode variable-token colors**

```bash
grep -n "cm-var\|cm-pathparam\|cm-querykey" src/components/editor/cm6-theme.ts
```

Note every `color:` value — these are the hardcoded `rgb()` values that are too dark in dark mode.

- [ ] **Step 3: Replace variable-token colors in `rocketTheme` with 2026 Light values**

In the base `rocketTheme` `EditorView.theme({...})` call, replace all `.cm-var-*`, `.cm-pathparam*`, `.cm-querykey*` `color:` and `background:` values:

```ts
'.cm-var-environment': {
  background: 'color-mix(in srgb, #b69500 15%, transparent)',
  color: '#b69500',  // VSCode 2026 Light: notificationsWarningIcon
},
'.cm-var-collection': {
  background: 'color-mix(in srgb, #606060 12%, transparent)',
  color: '#606060',  // VSCode 2026 Light: descriptionForeground
},
'.cm-var-global': {
  background: 'color-mix(in srgb, #0069cc 12%, transparent)',
  color: '#0069cc',  // VSCode 2026 Light: primary
},
'.cm-var-folder': {
  background: 'color-mix(in srgb, #587c0c 12%, transparent)',
  color: '#587c0c',  // VSCode 2026 Light: gitDecoration.added
},
'.cm-var-request, .cm-var-runtime': {
  background: 'color-mix(in srgb, #587c0c 12%, transparent)',
  color: '#587c0c',
},
'.cm-var-process': {
  background: 'color-mix(in srgb, #606060 12%, transparent)',
  color: '#606060',
},
'.cm-var-unresolved': {
  background: 'hsl(var(--destructive) / 0.15)',
  color: 'hsl(var(--destructive))',
},
'.cm-pathparam': {
  borderRadius: '3px',
  padding: '1px 3px',
  background: 'color-mix(in srgb, #652d90 12%, transparent)',
  color: '#652d90',  // VSCode 2026 Light: charts.purple
},
'.cm-pathparam-unresolved': {
  borderRadius: '3px',
  padding: '1px 3px',
  background: 'hsl(var(--destructive) / 0.15)',
  color: 'hsl(var(--destructive))',
},
'.cm-querykey': {
  borderRadius: '3px',
  padding: '1px 3px',
  background: 'color-mix(in srgb, #652d90 10%, transparent)',
  color: '#652d90',
},
```

- [ ] **Step 4: Add or update `rocketThemeDark`**

Find the `rocketThemeDark` export (check what the file exports). If it exists, update its `.cm-var-*` overrides. If it does not exist, add it after the base `rocketTheme`:

```ts
/**
 * Dark mode overrides for variable-token colors.
 * Uses VSCode 2026 Dark semantic colors.
 */
export const rocketThemeDark = EditorView.theme(
  {
    '.cm-var-environment': {
      background: 'color-mix(in srgb, #e5ba7d 15%, transparent)',
      color: '#e5ba7d',   // VSCode 2026 Dark: list.warningForeground
    },
    '.cm-var-collection': {
      background: 'color-mix(in srgb, #8c8c8c 12%, transparent)',
      color: '#8c8c8c',   // VSCode 2026 Dark: descriptionForeground
    },
    '.cm-var-global': {
      background: 'color-mix(in srgb, #3994bc 15%, transparent)',
      color: '#3994bc',   // VSCode 2026 Dark: focusBorder / ring
    },
    '.cm-var-folder': {
      background: 'color-mix(in srgb, #73c991 12%, transparent)',
      color: '#73c991',   // VSCode 2026 Dark: gitDecoration.added
    },
    '.cm-var-request, .cm-var-runtime': {
      background: 'color-mix(in srgb, #73c991 12%, transparent)',
      color: '#73c991',
    },
    '.cm-var-process': {
      background: 'color-mix(in srgb, #8c8c8c 12%, transparent)',
      color: '#8c8c8c',
    },
    '.cm-pathparam': {
      background: 'color-mix(in srgb, #ad80d7 12%, transparent)',
      color: '#ad80d7',   // approx dark purple — VSCode 2026 Dark charts.purple
    },
    '.cm-querykey': {
      background: 'color-mix(in srgb, #ad80d7 10%, transparent)',
      color: '#ad80d7',
    },
  },
  { dark: true },
);
```

- [ ] **Step 5: Verify `rocketThemeDark` is wired in `SingleLineEditor`**

```bash
grep -rn "rocketThemeDark\|darkTheme" src/ --include="*.ts" --include="*.tsx"
```

If `rocketThemeDark` is already conditionally applied alongside `rocketTheme` in `SingleLineEditor.tsx`, no further wiring is needed.

If it is **not** wired, find where `rocketTheme` is added to the CodeMirror `extensions` array and apply the conditional:

```ts
import { rocketTheme, rocketThemeDark } from '@/components/editor/cm6-theme';
// inside the useMemo for extensions:
isDark ? rocketThemeDark : rocketTheme,
```

Where `isDark` reads from `document.documentElement.classList.contains('dark')` or the Zustand theme store — use whichever the file already uses for Monaco.

- [ ] **Step 6: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add src/components/editor/cm6-theme.ts src/components/editor/SingleLineEditor.tsx
git commit -m "fix(cm6): replace hardcoded rgb() variable-token colors with 2026 palette

Light: amber #b69500, green #587c0c, blue #0069cc, purple #652d90
Dark:  amber #e5ba7d, green #73c991, teal #3994bc, purple #ad80d7
Unresolved tokens use hsl(var(--destructive)) in both modes.

Fixes washed-out variable highlight colors in dark mode."
```

---

## Smoke Test

After completing all three tasks:

- [ ] Start the dev server: `cd frontend && yarn dev`
- [ ] Light mode: sidebar `#FAFAFD`, editor panel `#FFFFFF`, primary buttons `#0069CC` blue
- [ ] Toggle to dark: sidebar `#191A1B`, editor `#121314`, buttons steel blue `#297AA0`
- [ ] Open a request with `{{variable}}` tokens — amber/green/blue highlights readable in both modes
- [ ] `{{missingVar}}` shows red in both modes
