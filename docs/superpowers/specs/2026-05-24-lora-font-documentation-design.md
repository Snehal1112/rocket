---
title: Lora Font for Documentation Preview
date: 2026-05-24
status: approved
---

# Lora Font for Documentation Preview

## Goal

Apply the Lora serif font to all markdown preview surfaces in the app — request Documentation tab and collection/workspace overview Documentation card. The rest of the UI remains IBM Plex Sans.

## Scope

- `MarkdownRenderer` is the single shared component for all markdown preview rendering.
- It applies a `div.prose-doc` wrapper; all typography is governed by `.prose-doc` rules in `globals.css`.
- No component logic changes are needed.

## Implementation

### 1. Install `@fontsource/lora`

```
yarn add @fontsource/lora
```

Import weights 400 (regular) and 600 (semi-bold). These cover body text and headings respectively.

### 2. Import in `globals.css`

Add alongside the existing IBM Plex imports at the top of `globals.css`:

```css
@import "@fontsource/lora/400.css";
@import "@fontsource/lora/400-italic.css";
@import "@fontsource/lora/600.css";
```

### 3. Add `--font-serif` CSS variable

In `:root` inside `globals.css`, alongside `--font-mono`:

```css
--font-serif: "Lora", Georgia, serif;
```

### 4. Apply to `.prose-doc`

Add `font-family: var(--font-serif);` to the root `.prose-doc` rule. This cascades to all body text, headings, lists, and blockquotes. Code blocks already override with `var(--font-mono)` and remain unaffected.

## What does NOT change

- Edit-mode textarea: stays `font-mono`.
- Code blocks and inline code: stay `var(--font-mono)`.
- All app chrome (sidebar, panels, buttons, tabs): stays IBM Plex Sans.
- No component files are modified.

## Affected files

| File | Change |
|---|---|
| `package.json` | Add `@fontsource/lora` dependency |
| `src/globals.css` | Import Lora, add `--font-serif` variable, apply to `.prose-doc` |
