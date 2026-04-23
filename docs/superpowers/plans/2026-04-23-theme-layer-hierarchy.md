# Theme Layer Hierarchy Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Swap `--background` and `--card` HSL values in both light and dark themes so the sidebar/chrome sits at the base layer and the content panel is visually elevated above it.

**Architecture:** Single CSS variable swap in `src/globals.css` — no component edits, no new tokens, no Tailwind changes. The existing component classes (`bg-card`, `bg-background`) already map to the right surfaces; only the underlying colour values are wrong.

**Tech Stack:** CSS custom properties, Tailwind CSS (reads variables via `hsl(var(--*))`)

---

### Task 1: Swap `--background` and `--card` in both themes

**Files:**
- Modify: `src/globals.css:16-20` (light `:root`) and `src/globals.css:59-63` (dark `.dark`)

- [ ] **Step 1: Confirm current values before editing**

Run:
```bash
grep -n "\-\-background\|\-\-card" src/globals.css | head -10
```

Expected output:
```
16:    --background: 0 0% 100%;
20:    --card: 0 0% 97.3%;
21:    --card-foreground: 0 0% 23%;
59:    --background: 0 0% 12%;
63:    --card: 0 0% 9.4%;
64:    --card-foreground: 0 0% 80%;
```

- [ ] **Step 2: Apply the light theme swap in `src/globals.css`**

In the `:root` block, change lines 15–21 from:
```css
    /* VSCode Light Modern: editor.background #FFFFFF */
    --background: 0 0% 100%;
    /* VSCode Light Modern: foreground #3B3B3B */
    --foreground: 0 0% 23%;
    /* VSCode Light Modern: sideBar.background #F8F8F8 */
    --card: 0 0% 97.3%;
    --card-foreground: 0 0% 23%;
```
to:
```css
    /* VSCode Light Modern: sideBar.background #F8F8F8 — shell/sidebar base layer */
    --background: 0 0% 97.3%;
    /* VSCode Light Modern: foreground #3B3B3B */
    --foreground: 0 0% 23%;
    /* VSCode Light Modern: editor.background #FFFFFF — elevated content surface */
    --card: 0 0% 100%;
    --card-foreground: 0 0% 23%;
```

- [ ] **Step 3: Apply the dark theme swap in `src/globals.css`**

In the `.dark` block, change lines 58–64 from:
```css
    /* VSCode Dark Modern: editor.background #1F1F1F */
    --background: 0 0% 12%;
    /* VSCode Dark Modern: foreground #CCCCCC */
    --foreground: 0 0% 80%;
    /* VSCode Dark Modern: sideBar.background #181818 */
    --card: 0 0% 9.4%;
    --card-foreground: 0 0% 80%;
```
to:
```css
    /* VSCode Dark Modern: sideBar.background #181818 — shell/sidebar base layer */
    --background: 0 0% 9.4%;
    /* VSCode Dark Modern: foreground #CCCCCC */
    --foreground: 0 0% 80%;
    /* VSCode Dark Modern: editor.background #1F1F1F — elevated content surface */
    --card: 0 0% 12%;
    --card-foreground: 0 0% 80%;
```

- [ ] **Step 4: Verify the values are correct after editing**

Run:
```bash
grep -n "\-\-background\|\-\-card" src/globals.css | head -10
```

Expected output:
```
16:    --background: 0 0% 97.3%;
20:    --card: 0 0% 100%;
21:    --card-foreground: 0 0% 23%;
59:    --background: 0 0% 9.4%;
63:    --card: 0 0% 12%;
64:    --card-foreground: 0 0% 80%;
```

- [ ] **Step 5: Run TypeScript check to confirm nothing broke**

Run:
```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 6: Run lint check**

Run:
```bash
yarn check
```

Expected: no errors (CSS changes don't affect Biome lint).

- [ ] **Step 7: Start the dev server and visually verify both themes**

Run:
```bash
yarn dev
```

Open `http://localhost:1420` in a browser. Toggle between dark and light mode using the status bar sun/moon button.

**Dark mode — check:**
- Sidebar background is darker than the main content panel
- Title bar and status bar match the sidebar tone (dark base)
- Active tab appears slightly lighter than the tab bar

**Light mode — check:**
- Sidebar is a visible light grey (#F8F8F8), content panel is white (#FFFFFF)
- Title bar and status bar match the sidebar grey tone
- Active tab is white, lifting off the grey tab bar

- [ ] **Step 8: Commit**

```bash
git add src/globals.css
git commit -m "fix(theme): swap --background and --card roles for correct layer depth

Dark: --background #181818 (shell/sidebar), --card #1F1F1F (content)
Light: --background #F8F8F8 (shell/sidebar), --card #FFFFFF (content)

All VSCode Modern hex values unchanged — only role assignments corrected."
```
