# Theme Palette Update — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update all CSS custom property values in `src/globals.css` to match the screenshot-derived dark palette and its mathematically-derived light counterpart, plus three Option B light-theme corrections.

**Architecture:** Single file edit — only the `:root` and `.dark` CSS custom property blocks change. No component markup, no Tailwind config, no chart colors, no scrollbar CSS, no Linux window chrome. The nested card elevation rules and all Tailwind utilities consume these variables automatically.

**Tech Stack:** CSS custom properties (HSL), Tailwind CSS v4, shadcn/ui.

---

## File Map

| File | Change |
|---|---|
| `src/globals.css` | Update `:root` (light) and `.dark` custom property values |

---

## Task 1: Update dark theme token values

**Files:**
- Modify: `src/globals.css` — `.dark` block

- [ ] **Step 1: Read the current `.dark` block to confirm line numbers**

Open `src/globals.css` and locate the `.dark {` block (currently around line 57–97). Confirm the exact lines before editing.

- [ ] **Step 2: Replace the entire `.dark` block with the new values**

Replace the `.dark { ... }` block with:

```css
  .dark {
    /* screenshot: sidebar #1C1C1C */
    --background: 0 0% 11%;
    /* screenshot: primary text #D4D4D4 */
    --foreground: 0 0% 83%;
    /* screenshot: editor surface #252526 */
    --card: 0 0% 14.5%;
    --card-foreground: 0 0% 83%;
    /* VSCode Dark Modern: dropdown.listBackground */
    --popover: 0 0% 14.5%;
    --popover-foreground: 0 0% 83%;
    /* VSCode Dark Modern: button.background #0078D4 — unchanged */
    --primary: 206 100% 41.6%;
    --primary-foreground: 0 0% 100%;
    /* screenshot: tab bar #2D2D2D */
    --secondary: 0 0% 17.6%;
    --secondary-foreground: 0 0% 83%;
    /* screenshot: input background #2D2D2D */
    --muted: 0 0% 17.6%;
    /* screenshot: muted text #9D9D9D */
    --muted-foreground: 0 0% 61.6%;
    /* screenshot: hover #2A2A2A — separated from border */
    --accent: 0 0% 16.5%;
    --accent-foreground: 0 0% 83%;
    --destructive: 0 71% 61.6%;
    --destructive-foreground: 0 0% 98%;
    --warning: 45 100% 40%;
    --warning-foreground: 0 0% 98%;
    /* screenshot: card borders #3C3C3C — major visibility improvement */
    --border: 0 0% 23.5%;
    /* screenshot: input border #3C3C3C */
    --input: 0 0% 23.5%;
    /* VSCode Dark Modern: focusBorder #0078D4 — unchanged */
    --ring: 206 100% 41.6%;
    --chart-1: 206 100% 41.6%;
    --chart-2: 160 63% 39%;
    --chart-3: 30 94% 55%;
    --chart-4: 282 60% 62%;
    --chart-5: 340 78% 61%;
  }
```

- [ ] **Step 3: Verify `--radius` is NOT in the `.dark` block**

`--radius` is defined in `:root` only and applies to both themes. Do not add it to `.dark`. Confirm the `.dark` block contains no `--radius` line.

- [ ] **Step 4: Visually diff the changed lines**

Run:
```bash
git diff src/globals.css
```

Confirm only `.dark` block variables changed. Confirm no other sections were touched (scrollbar, Linux chrome, nested card rules, prose-doc).

- [ ] **Step 5: Commit**

```bash
git add src/globals.css
git commit -m "fix(theme): update dark theme tokens to match screenshot palette"
```

---

## Task 2: Update light theme token values and radius

**Files:**
- Modify: `src/globals.css` — `:root` block

- [ ] **Step 1: Read the current `:root` block to confirm line numbers**

Open `src/globals.css` and locate the `:root {` block (currently around lines 14–56). Confirm exact lines before editing.

- [ ] **Step 2: Replace the entire `:root` block with the new values**

Replace the `:root { ... }` block with:

```css
  :root {
    /* ratio-derived from dark bg #1C1C1C → light equivalent */
    --background: 0 0% 96.9%;
    /* dark fg #D4D4D4 → flip: #2E2E2E */
    --foreground: 0 0% 18%;
    /* light top surface — unchanged */
    --card: 0 0% 100%;
    --card-foreground: 0 0% 18%;
    /* VSCode Light Modern: dropdown.listBackground #FFFFFF */
    --popover: 0 0% 100%;
    --popover-foreground: 0 0% 18%;
    /* VSCode Light Modern: button.background #005FB8 — unchanged */
    --primary: 209 100% 36.1%;
    --primary-foreground: 0 0% 100%;
    /* dark secondary #2D2D2D → flip: #E8E8E8 */
    --secondary: 0 0% 91%;
    --secondary-foreground: 0 0% 18%;
    /* dark muted #2D2D2D → flip: #EEEEEE */
    --muted: 0 0% 93.5%;
    /* Option B fix: was 53% (too washed-out) → 44% (#707070) */
    --muted-foreground: 0 0% 44%;
    /* dark accent #2A2A2A → flip, separated from border */
    --accent: 0 0% 92%;
    --accent-foreground: 0 0% 18%;
    --destructive: 0 71% 50%;
    --destructive-foreground: 0 0% 98%;
    --warning: 45 90% 42%;
    --warning-foreground: 0 0% 98%;
    /* dark border #3C3C3C → flip: #D9D9D9 — more visible than current #E5E5E5 */
    --border: 0 0% 85%;
    /* dark input #3C3C3C → flip: #CCCCCC */
    --input: 0 0% 80%;
    /* VSCode Light Modern: focusBorder #005FB8 — unchanged */
    --ring: 209 100% 36.1%;
    --chart-1: 209 100% 36.1%;
    --chart-2: 171 65% 35%;
    --chart-3: 28 91% 52%;
    --chart-4: 283 52% 49%;
    --chart-5: 343 74% 52%;
    /* Option B fix: was 0.7rem (too round for VS Code feel) */
    --radius: 0.3rem;
    --font-mono: "JetBrains Mono", ui-monospace, monospace;
  }
```

- [ ] **Step 3: Visually diff the changed lines**

Run:
```bash
git diff src/globals.css
```

Confirm only `:root` block variables changed. The values that must be different from old:
- `--background`: was `0 0% 97.3%` → now `0 0% 96.9%`
- `--foreground`: was `0 0% 23%` → now `0 0% 18%`
- `--muted-foreground`: was `0 0% 53%` → now `0 0% 44%` ← most important fix
- `--accent`: was `0 0% 91%` → now `0 0% 92%`
- `--border`: was `0 0% 90%` → now `0 0% 85%` ← more visible
- `--input`: was `0 0% 80.8%` → now `0 0% 80%`
- `--radius`: was `0.7rem` → now `0.3rem` ← less rounded

- [ ] **Step 4: Run TypeScript check to confirm no regressions**

```bash
yarn tsc --noEmit
```

Expected: no errors (CSS-only change, TS is unaffected — this just confirms the build toolchain is intact).

- [ ] **Step 5: Run lint check**

```bash
yarn check
```

Expected: same pre-existing errors as before this change, none new.

- [ ] **Step 6: Commit**

```bash
git add src/globals.css
git commit -m "fix(theme): update light theme tokens — ratio-derived palette + Option B fixes"
```
