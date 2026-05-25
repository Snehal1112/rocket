# Font Swap — Plus Jakarta Sans + JetBrains Mono Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Permanently replace IBM Plex Sans (body/UI font) and IBM Plex Mono (code font) with Plus Jakarta Sans and JetBrains Mono, removing the old packages from `package.json`.

**Architecture:** All changes live in `src/globals.css` (font imports, CSS variables, `body` rule) and `package.json` (add Plus Jakarta Sans, remove IBM Plex packages). No component files are touched — Tailwind's `font-mono`/`font-sans` utility classes and CSS variable references in components automatically pick up the new fonts.

**Tech Stack:** `@fontsource-variable/plus-jakarta-sans`, `@fontsource/jetbrains-mono` (already installed), CSS custom properties, Tailwind v4

---

### Task 1: Install Plus Jakarta Sans and swap font imports + variables

**Files:**
- Modify: `package.json` (via yarn commands)
- Modify: `src/globals.css` — lines 1–4 (imports), line 65 (`--font-mono`), line 66 (add `--font-sans`), line 393 (`body` font-family)

- [ ] **Step 1: Install Plus Jakarta Sans**

Run from the repo root:

```bash
yarn add @fontsource-variable/plus-jakarta-sans
```

Expected: output includes `+ @fontsource-variable/plus-jakarta-sans@...` and no errors.

Verify the index file exists:

```bash
ls node_modules/@fontsource-variable/plus-jakarta-sans/index.css
```

Expected: path prints without error.

- [ ] **Step 2: Verify JetBrains Mono weight files exist**

```bash
ls node_modules/@fontsource/jetbrains-mono/400.css \
   node_modules/@fontsource/jetbrains-mono/500.css \
   node_modules/@fontsource/jetbrains-mono/600.css
```

Expected: all three paths print without error.

- [ ] **Step 3: Swap the font `@import` lines in `src/globals.css`**

Open `src/globals.css`. Lines 1–4 currently read:

```css
@import "@fontsource-variable/ibm-plex-sans";
@import "@fontsource/ibm-plex-mono/400.css";
@import "@fontsource/ibm-plex-mono/500.css";
@import "@fontsource/ibm-plex-mono/600.css";
```

Replace them with:

```css
@import "@fontsource-variable/plus-jakarta-sans";
@import "@fontsource/jetbrains-mono/400.css";
@import "@fontsource/jetbrains-mono/500.css";
@import "@fontsource/jetbrains-mono/600.css";
```

Lines 5–10 (Lora, tailwindcss, tw-animate-css, @config) must remain unchanged.

- [ ] **Step 4: Add `--font-sans` variable and update `--font-mono` in `:root`**

In `src/globals.css`, find line 65 which currently reads:

```css
    --font-mono: "IBM Plex Mono", ui-monospace, monospace;
```

Replace it with two lines — `--font-sans` first, then updated `--font-mono`:

```css
    --font-sans: "Plus Jakarta Sans Variable", "Plus Jakarta Sans", ui-sans-serif, sans-serif;
    --font-mono: "JetBrains Mono", ui-monospace, monospace;
```

The `--font-serif` line immediately after must remain unchanged.

- [ ] **Step 5: Fix the hardcoded `body` font-family**

In `src/globals.css`, find the `body` rule (around line 393 after prior edits) which currently reads:

```css
  body {
    background-color: hsl(var(--background));
    color: hsl(var(--foreground));
    font-family: "IBM Plex Sans Variable", "IBM Plex Sans", "Segoe UI", sans-serif;
  }
```

Change it to:

```css
  body {
    background-color: hsl(var(--background));
    color: hsl(var(--foreground));
    font-family: var(--font-sans);
  }
```

- [ ] **Step 6: Remove IBM Plex packages**

```bash
yarn remove @fontsource-variable/ibm-plex-sans @fontsource/ibm-plex-mono
```

Expected: output confirms both packages removed. No errors.

- [ ] **Step 7: Verify TypeScript and lint pass**

```bash
yarn tsc --noEmit && yarn check
```

Expected: TypeScript — no errors. Biome — no new errors introduced by this change. (Pre-existing Biome errors in other files are unrelated and acceptable.)

- [ ] **Step 8: Commit**

```bash
git add package.json yarn.lock src/globals.css
git commit -m "feat: replace IBM Plex Sans/Mono with Plus Jakarta Sans and JetBrains Mono"
```
