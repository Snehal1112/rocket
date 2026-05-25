# Lora Font for Documentation Preview — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Apply the Lora serif font to all markdown preview surfaces (`MarkdownRenderer`) so documentation reads like a polished editorial surface, while keeping the rest of the app on IBM Plex Sans.

**Architecture:** All markdown preview typography is governed by the `.prose-doc` CSS class in `src/globals.css`. Adding Lora via `@fontsource/lora`, declaring a `--font-serif` CSS variable, and applying it to `.prose-doc` is the complete change — no component files need to be touched.

**Tech Stack:** `@fontsource/lora` npm package, CSS custom properties, Tailwind v4 (`globals.css`)

---

### Task 1: Install `@fontsource/lora` and wire up CSS

**Files:**
- Modify: `package.json` (via yarn add)
- Modify: `src/globals.css` — lines 1–4 (imports), line 62 (CSS variable), line 628 (`.prose-doc` root rule)

- [ ] **Step 1: Install the package**

Run from the repo root:

```bash
yarn add @fontsource/lora
```

Expected output: something like `success Saved 1 new dependency`.

Verify the files exist:

```bash
ls node_modules/@fontsource/lora/400.css node_modules/@fontsource/lora/400-italic.css node_modules/@fontsource/lora/600.css
```

Expected: all three paths print without error.

- [ ] **Step 2: Import Lora in `globals.css`**

Open `src/globals.css`. The top of the file currently reads:

```css
@import "@fontsource-variable/ibm-plex-sans";
@import "@fontsource/ibm-plex-mono/400.css";
@import "@fontsource/ibm-plex-mono/500.css";
@import "@fontsource/ibm-plex-mono/600.css";
@import "tailwindcss";
```

Add the Lora imports immediately after the IBM Plex Mono imports, before `@import "tailwindcss"`:

```css
@import "@fontsource-variable/ibm-plex-sans";
@import "@fontsource/ibm-plex-mono/400.css";
@import "@fontsource/ibm-plex-mono/500.css";
@import "@fontsource/ibm-plex-mono/600.css";
@import "@fontsource/lora/400.css";
@import "@fontsource/lora/400-italic.css";
@import "@fontsource/lora/600.css";
@import "tailwindcss";
```

- [ ] **Step 3: Add `--font-serif` CSS variable**

Still in `src/globals.css`, find line 62 which currently reads:

```css
    --font-mono: "IBM Plex Mono", ui-monospace, monospace;
```

Add `--font-serif` on the line immediately after it:

```css
    --font-mono: "IBM Plex Mono", ui-monospace, monospace;
    --font-serif: "Lora", Georgia, serif;
```

- [ ] **Step 4: Apply Lora to `.prose-doc`**

Find the `.prose-doc` root rule (around line 628 after edits). It currently reads:

```css
.prose-doc {
  font-size: 0.9375rem;
  line-height: 1.7;
  color: hsl(var(--foreground));
}
```

Add `font-family: var(--font-serif);` as the first property:

```css
.prose-doc {
  font-family: var(--font-serif);
  font-size: 0.9375rem;
  line-height: 1.7;
  color: hsl(var(--foreground));
}
```

- [ ] **Step 5: Verify TypeScript and lint pass**

```bash
yarn tsc --noEmit && yarn check
```

Expected: no errors. (This is a CSS-only change — TS errors are not expected, but lint covers import ordering.)

- [ ] **Step 6: Commit**

```bash
git add package.json yarn.lock src/globals.css
git commit -m "feat: apply Lora serif font to markdown documentation preview"
```
