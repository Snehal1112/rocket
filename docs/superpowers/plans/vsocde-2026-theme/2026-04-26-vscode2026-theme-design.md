# Design Spec: VSCode 2026 Theme Migration

**Date:** 2026-04-26
**Status:** Approved
**Scope:** `src/globals.css` CSS variable update + CM6 variable-token dark mode fix + `apps/theme-playground` standalone showcase app

---

## Problem

RocketAPI's current theme is based on **VSCode Modern** (Light Modern / Dark Modern). VSCode shipped **2026 Light** and **2026 Dark** themes in early 2026 with a refined palette — cooler neutrals, a richer blue accent, and a more controlled dark mode that is dimmer and easier on the eyes for long coding sessions. The Sage internal adoption pitch benefits from looking current.

Additionally, the CM6 `rocketTheme` in `SingleLineEditor` uses **hardcoded `rgb()` values** for variable-token highlight colors. These are light-mode values that look washed-out in dark mode. The fix is to replace them with HSL values derived from the new 2026 token palette that actually adapt.

There is also no isolated environment for rapidly iterating on component appearance. Every UI change currently requires running the full Tauri app. A plain Vite React playground (`apps/theme-playground`) solves this — it starts in under a second, runs in any browser, and lets Sage colleagues preview the design without installing Rust or Tauri.

---

## Goals

1. **Update `src/globals.css`** — replace all HSL values with VSCode 2026 equivalents. Zero component edits required — all `bg-card`, `bg-background`, `text-foreground`, etc. classes continue to work unchanged.
2. **Fix CM6 variable-token dark mode** — replace hardcoded `rgb()` highlight values in `rocketTheme` with `hsl(var(--*))` or 2026-derived HSL values that work in both modes.
3. **Create `apps/theme-playground`** — a standalone Vite + React + TypeScript app that showcases every shadcn/ui component used in RocketAPI, themed with the 2026 palette, with a working light/dark toggle.

---

## Non-Goals

- No Monaco theme update in this spec (Monaco is a separate `useMonacoTheme.ts` concern — out of scope here).
- No new CSS custom properties — only updating existing values.
- No Tailwind config changes.
- No component structure changes.
- The playground does NOT use Tauri — it is a browser-only app.

---

## Architecture

### 1 — `src/globals.css` update

The file already exists at `frontend/src/globals.css`. It is structured as:

```
@import fonts
@import tailwind
@layer base {
  :root {  /* light tokens */  }
  .dark {  /* dark tokens */  }
}
```

Every token is already present. The change is purely **value replacement** — same property names, new HSL numbers derived from the 2026 JSON files.

**Key token changes from Modern → 2026:**

| Token | Light Modern | **2026 Light** | Dark Modern | **2026 Dark** |
|---|---|---|---|---|
| `--background` | `0 0% 97.3%` | **`240 20% 99%`** | `0 0% 9.4%` | **`210 5% 7.8%`** |
| `--card` | `0 0% 100%` | **`0 0% 100%`** | `0 0% 12%` | **`210 4% 10%`** |
| `--foreground` | `0 0% 23%` | **`0 0% 12.5%`** | `0 0% 80%` | **`0 0% 74.9%`** |
| `--muted-foreground` | `0 0% 53%` | **`0 0% 37.6%`** | `0 0% 53%` | **`0 0% 54.9%`** |
| `--primary` | `209 100% 36.1%` | **`208 100% 40%`** | `206 100% 41.6%` | **`201 61% 39.6%`** |
| `--secondary` | `0 0% 90%` | **`0 0% 91.8%`** | `0 0% 17%` | **`210 2% 15.1%`** |
| `--border` | `0 0% 90%` | **`0 0% 84.7%`** | `0 0% 17%` | **`210 2.4% 20.2%`** |
| `--input` | `0 0% 80.8%` | **`0 0% 84.7%`** | `0 0% 80.8%` | **`210 2.4% 20.2%`** |
| `--ring` | `209 100% 36.1%` | **`208 100% 40%`** | `206 100% 41.6%` | **`201 53% 47.5%`** |
| `--destructive` | `0 84.2% 60.2%` | **`0 89% 35%` (light)** | unchanged | **`13 87% 70%` (dark)** |
| `--muted` | `0 0% 95%` | **`240 18% 95%`** | `0 0% 17%` | **`210 2% 15.1%`** |
| `--accent` | `0 0% 91%` | **`208 100% 40% / 10%`** | `0 0% 17%` | **`201 54% 47.5% / 15%`** |
| `--popover` | `0 0% 100%` | **`240 20% 99%`** | `0 0% 12%` | **`210 3% 12.9%`** |

The `--background` / `--card` layer distinction established by plan `2026-04-23-theme-layer-hierarchy` is **preserved** — background is the shell, card is the elevated content surface. The 2026 colors give the shell a faint cool tint (`#FAFAFD`) rather than pure grey.

### 2 — CM6 variable-token dark mode fix

The current `rocketTheme` in `src/components/editor/cm6-theme.ts` (or wherever it lives — agent must `grep -rn "rocketTheme" src/`) uses hardcoded `rgb()` values for `.cm-var-*` token colors. These are light-mode values:

```ts
'.cm-var-environment': {
  background: 'rgba(234, 179, 8, 0.15)',
  color: 'rgb(180, 83, 9)', // amber-700 — too dark in dark mode
},
```

The fix is a `rocketThemeDark` extension (or a CSS-variable-based approach) that overrides `.cm-var-*` colors in dark mode. The new colors derive from the 2026 semantic palette:

| Variable scope | Light text | Dark text | Notes |
|---|---|---|---|
| `environment` | `#b69500` (2026 warning) | `#e5ba7d` (2026 dark warning) | amber tone |
| `collection` | `#606060` (2026 muted) | `#8c8c8c` (2026 dark muted) | neutral |
| `global` | `#0069cc` (2026 primary) | `#3994bc` (2026 dark ring) | blue |
| `folder` | `#587c0c` (2026 success) | `#73c991` (2026 dark success) | green |
| `request`/`runtime` | `#587c0c` | `#73c991` | same as folder |
| `process` | `#606060` | `#8c8c8c` | muted |
| `unresolved` | `#ad0707` (2026 destructive) | `#f48771` (2026 dark destructive) | red |
| `pathparam` | `#652d90` (2026 chart.purple) | `#ad80d7` (approx) | violet |
| `querykey` | `#652d90` | `#ad80d7` | purple |

Implementation: extend `rocketTheme` with a `.dark &` selector or add a separate `rocketThemeDark` `EditorView.theme()` export and conditionally apply it alongside the base theme in `SingleLineEditor.tsx`.

### 3 — `apps/theme-playground`

A minimal standalone Vite + React 18 + TypeScript app. No Tauri. No Rust. Opens in a browser tab in under 1 second.

**Directory layout:**
```
apps/
└── theme-playground/
    ├── package.json          # name: @rocket/theme-playground, no Tauri deps
    ├── vite.config.ts        # path alias: @/ui → ../../frontend/src/components/ui
    ├── tsconfig.json
    ├── index.html
    └── src/
        ├── main.tsx          # mounts App, imports globals.css via relative path
        ├── App.tsx           # sidebar nav + section router
        ├── globals.css       # @import ../../frontend/src/globals.css (symlink or re-import)
        ├── ThemeToggle.tsx   # light/dark toggle using document.documentElement.classList
        └── sections/
            ├── ButtonSection.tsx
            ├── InputSection.tsx
            ├── CheckboxSection.tsx
            ├── BadgeSection.tsx
            ├── TabsSection.tsx
            ├── TableSection.tsx
            ├── CardSection.tsx
            ├── AlertSection.tsx
            ├── DropdownSection.tsx
            ├── CollapsibleSection.tsx
            ├── ProgressSection.tsx
            ├── RequestBarSection.tsx    # RocketAPI pattern
            ├── KeyValueSection.tsx      # RocketAPI pattern
            ├── GitStatusSection.tsx     # RocketAPI pattern
            └── EnvSwitcherSection.tsx   # RocketAPI pattern
```

**Sharing shadcn components:** The playground uses a Vite path alias pointing at `frontend/src/components/ui/`. No copying, no separate `components.json`. The playground inherits the same exact Button, Input, Checkbox, etc. components that the main app uses. When a component is updated in the main app, the playground reflects it immediately.

**Sharing the theme:** The playground imports `frontend/src/globals.css` via a relative path in its own `globals.css`. This means one edit to the theme file is reflected in both the Tauri app and the playground simultaneously.

**Dev command:** `cd apps/theme-playground && yarn dev` → opens at `http://localhost:5174` (Vite auto-selects next port after 5173).

---

## Color Source Mapping (full reference)

| CSS var | shadcn role | Light source (2026-light.json) | Dark source (2026-dark.json) |
|---|---|---|---|
| `--background` | App shell | `sideBar.background #FAFAFD` | `sideBar.background #191A1B` |
| `--card` | Content panel | `editor.background #FFFFFF` | `editor.background #121314` |
| `--popover` | Dropdown/tooltip bg | `editorWidget.background #FAFAFD` | `editorWidget.background #202122` |
| `--foreground` | Primary text | `foreground #202020` | `foreground #bfbfbf` |
| `--muted-foreground` | Secondary text | `descriptionForeground #606060` | `descriptionForeground #8C8C8C` |
| `--primary` | CTA button, accent | `button.background #0069CC` | `button.background #297AA0` |
| `--ring` | Focus outline | `focusBorder #0069CC` | `focusBorder #3994BC` |
| `--secondary` | Ghost button bg | `button.secondaryBackground #EAEAEA` | `list.inactiveSelectionBackground #2C2D2E` |
| `--muted` | Subtle surface | `editorStickyScrollHover #F0F0F3` | `textBlockQuote.background #242526` |
| `--accent` | Hover/active bg | `list.activeSelectionBackground #0069CC1A` | `list.activeSelectionBackground #3994BC26` |
| `--border` | All borders | `dropdown.border #D8D8D8` | `dropdown.border #333536` |
| `--input` | Input border | `input.border #D8D8D866` | `input.border #333536` |
| `--destructive` | Error / danger | `errorForeground #ad0707` | `errorForeground #f48771` |

---

## Acceptance Criteria

1. `yarn build` passes with no TypeScript or CSS errors after `globals.css` changes.
2. In the running Tauri app, the sidebar is `#FAFAFD` (light) / `#191A1B` (dark) and the editor panel is `#FFFFFF` (light) / `#121314` (dark).
3. CM6 variable tokens (yellow, blue, green, red) are readable in both light and dark mode — no washed-out colors in dark mode.
4. `cd apps/theme-playground && yarn dev` starts successfully and opens a component showcase in the browser.
5. The playground light/dark toggle changes all component colors in sync.
6. Toggling the Tauri app theme and the playground theme produce visually identical component appearances.
