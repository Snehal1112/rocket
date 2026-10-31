# Elevation Plan Index

Spec: `docs/superpowers/specs/2026-10-31-elevation-design.md`.

Plans run one at a time, in order. Each plan has at most three tasks and ends with working,
checked software. One plan per elevation type, after a foundation plan.

| # | Plan | Elevation type | Delivers |
|---|---|---|---|
| 01 | `2026-10-31-elevation-plan-01-tokens.md` | All four (definitions) | Tokens, `.elevation-*` classes, Tailwind aliases, the CSS contract test with contrast checks. No visible change yet. |
| 02 | `2026-10-31-elevation-plan-02-raised-sidebar.md` | Raised | Lighter sidebar with a visible shadow in dark mode; Settings no longer paints over it. |
| 03 | `2026-10-31-elevation-plan-03-overlay.md` | Overlay | Menus, popovers, dialogs, sheet, toast and the editor hover tip share one surface and shadow. |
| 04 | `2026-10-31-elevation-plan-04-sunken-default-cards-docs.md` | Sunken, default, raised cards | Sunken surfaces and the console well, one default surface source, cards on the raised token, feature doc, manual checks. |

## Cross-plan facts

- Frontend and CSS only. No Rust, no IPC command, no `src-tauri/app-commands.txt` change.
- Commands (Bun, per the user's global rule): `bun run tsc --noEmit`, `bun run check`,
  `bun run test <path>`. If `bun run check` reports formatting or import order, run `bun run lint`
  (auto-fix), then `bun run check` again.
- Commits go through the `dev-workflow-skills:1-git-commit` skill with conventional subjects. Stage
  explicit paths only, never `-A` or `.`; other sessions share this checkout.
- shadcn/ui primitives and lucide-react icons only. Zustand: narrow selectors.
- No task touches `.yml`, collections, environments, auth or variable resolution, so the
  OpenCollection spec-reference step does not apply.
- Comments are short full sentences ending with a period.
- Do not use `color-mix()` or add `backdrop-filter` anywhere (WebKitGTK).

### Names shared across plans

```css
/* src/globals.css, defined in :root and .dark (plan 01) */
--elevation-sunken-bg; --elevation-surface-bg; --elevation-raised-bg; --elevation-overlay-bg; /* HSL triples */
--elevation-shadow-raised; --elevation-shadow-overlay;                                         /* box-shadow values */
.elevation-sunken; .elevation-surface; .elevation-raised; .elevation-overlay;                  /* unlayered classes */
```

```ts
// tailwind.config.js theme.extend.colors.elevation (plan 01)
{ sunken: string; surface: string; raised: string; overlay: string } // bg-elevation-sunken etc.

// src/lib/__tests__/elevation-tokens.test.ts (plan 01; plans 02 and 04 add cases)
function cssBlock(css: string, selector: string): string;   // body of the first rule with exactly that selector
function hslTriple(block: string, name: string): [h: number, s: number, l: number];
function contrast(a: [number, number, number], b: [number, number, number]): number; // WCAG, HSL in
```

### Review Focus (whole series)

1. Light theme surfaces are unchanged (only the overlay shadow tint differs). Pinned in plan 01.
2. Secondary text keeps 4.5:1 on raised and overlay in dark. Pinned in plan 01.
3. No `color-mix` or `backdrop-filter` in the new CSS. Pinned in plan 01.
4. A menu opened inside a dialog opened from Settings stays readable (overlay on overlay). Manual
   check in plan 04.
5. `hsl(var(--background) / 0.7)` (translucent headers, `src/globals.css:474`) still renders after
   `--background` becomes an alias. Manual check in plan 04.
