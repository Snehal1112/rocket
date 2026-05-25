---
title: Font Swap — Plus Jakarta Sans + JetBrains Mono
date: 2026-05-25
status: approved
---

# Font Swap — Plus Jakarta Sans + JetBrains Mono

## Goal

Permanently replace IBM Plex Sans (body/UI) and IBM Plex Mono (code/mono) with Plus Jakarta Sans and JetBrains Mono respectively. The Lora serif used in markdown documentation preview is untouched.

## Scope

All changes are in `src/globals.css` and `package.json`. No component files are modified. Tailwind utility classes (`font-mono`, `font-sans`) automatically inherit the new fonts via the CSS variables they resolve through.

## Implementation

### 1. Install `@fontsource-variable/plus-jakarta-sans`

```bash
yarn add @fontsource-variable/plus-jakarta-sans
```

`@fontsource/jetbrains-mono` is already installed — only specific weight files need to be imported.

### 2. Swap `@import` lines in `src/globals.css`

Replace:
```css
@import "@fontsource-variable/ibm-plex-sans";
@import "@fontsource/ibm-plex-mono/400.css";
@import "@fontsource/ibm-plex-mono/500.css";
@import "@fontsource/ibm-plex-mono/600.css";
```

With:
```css
@import "@fontsource-variable/plus-jakarta-sans";
@import "@fontsource/jetbrains-mono/400.css";
@import "@fontsource/jetbrains-mono/500.css";
@import "@fontsource/jetbrains-mono/600.css";
```

### 3. Add `--font-sans` CSS variable and update `--font-mono`

In `:root`, replace the existing `--font-mono` line and add `--font-sans`:

```css
--font-sans: "Plus Jakarta Sans Variable", "Plus Jakarta Sans", ui-sans-serif, sans-serif;
--font-mono: "JetBrains Mono", ui-monospace, monospace;
```

### 4. Fix hardcoded `body` font-family

The `body` rule currently hardcodes IBM Plex Sans directly. Change it to use the new variable:

```css
body {
  background-color: hsl(var(--background));
  color: hsl(var(--foreground));
  font-family: var(--font-sans);
}
```

### 5. Remove IBM Plex packages

```bash
yarn remove @fontsource-variable/ibm-plex-sans @fontsource/ibm-plex-mono
```

## What does NOT change

- `@fontsource/lora` — markdown documentation preview stays Lora.
- All component `.tsx`/`.ts` files — no changes.
- All Tailwind `font-mono` / `font-sans` utility class usages in components — they resolve through the CSS variables automatically.
- `MarkdownRenderer` `codeTagProps` — already uses `var(--font-mono)`, picks up JetBrains Mono automatically.

## Affected files

| File | Change |
|---|---|
| `package.json` | Add `@fontsource-variable/plus-jakarta-sans`; remove `@fontsource-variable/ibm-plex-sans` and `@fontsource/ibm-plex-mono` |
| `yarn.lock` | Updated by yarn |
| `src/globals.css` | Swap imports, add `--font-sans` variable, update `--font-mono`, fix `body` font-family |
