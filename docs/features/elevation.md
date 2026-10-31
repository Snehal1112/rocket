# Elevation

Rocket borrows Atlassian's elevation style (four levels, a shadow where a surface floats) and none of
its colours. Every colour is a VS Code 2026 colour. Reference for the style only:
https://atlassian.design/foundations/elevation. Tokens and classes are pinned by
`src/lib/__tests__/elevation-tokens.test.ts`, and the border rule by
`src/lib/__tests__/border-consistency.test.ts`.

## The four levels

Surfaces are raw HSL triples, shadows are full `box-shadow` values. All are in `:root` and `.dark` of
`src/globals.css`.

| Token | Dark | Light | Meaning |
|---|---|---|---|
| `--elevation-sunken-bg` | `210 5% 7.5%` (`#121314`) | `0 0% 100%` | editor.background |
| `--elevation-surface-bg` | `210 5% 10%` (`#191A1B`) | `220 21.4% 94.5%` | the shell |
| `--elevation-raised-bg` | `210 5% 10%` (`#191A1B`) | `240 50% 99%` | sideBar.background |
| `--elevation-overlay-bg` | `210 3% 13%` (`#202122`) | `0 0% 100%` | editorWidget.background |
| `--elevation-shadow-raised` | `2px 0 6px rgba(0,0,0,0.45), 6px 0 16px rgba(0,0,0,0.3)` | `2px 0 4px rgba(0,0,0,0.08), 4px 0 12px rgba(0,0,0,0.05)` | sidebar edge |
| `--elevation-shadow-overlay` | `0 2px 6px rgba(0,0,0,0.4), 0 8px 24px rgba(0,0,0,0.55)` | `0 2px 6px rgba(9,30,66,0.12), 0 8px 24px rgba(9,30,66,0.18)` | floating UI |

Classes: `.elevation-sunken`, `.elevation-surface`, `.elevation-raised`, `.elevation-overlay`. Tailwind
colours: `bg-elevation-sunken|surface|raised|overlay`. Do not put a `bg-*` utility on the same element
as an `.elevation-*` class.

## Rules

1. Colours come from VS Code 2026 only. Height is shown by a shadow and layering, never by inventing a
   lighter fill. In dark, raised keeps the sidebar colour.
2. Sunken and default have no shadow. Separate areas with the 1px border.
3. Do not change the look of menus, popovers, dialogs, the sheet, the toast or the hover tip. They
   keep their semi-transparent surfaces. The overlay token exists but nothing uses it yet.
4. Hover and press change the surface (`list-hover-bg`, `list-active-bg`). No elevation transitions.

## Where each level is used

- Raised: the Collections sidebar wrapper in `src/App.tsx` (`elevation-raised`, and `--sidebar-bg`
  aliases the raised token).
- Raised, right edge: the AI Assistant panel (`elevation-raised-end`, `--elevation-shadow-raised-end`,
  the same shadow mirrored toward the editor, `relative z-[5]`).
- Sunken: the Settings page, its sticky Save bar and the console output well (white in light).
- Default: dark `--background`, `--statusbar-bg`, `--panel-bg` and `--titlebar-bg` alias the default
  surface. The light title bar already shares the sidebar value, so light is not aliased.
- Dark cards: flat panels on `--background`, one small step above the editor, with one 1px
  `--border` and no ring. Nested cards use `--accent` with the same border colour.
- Raw `bg-card` on plain divs resolves to the sunken value in dark. That is intended.

## Borders

Solid borders use the plain `border-border` token, one 1px weight everywhere. Do not fade it with
`border-border/40` or similar. Dashed borders and hover states may differ.

## Stacking

The Settings overlay in `MainArea` is `z-[1]`, below the sidebar wrapper (`z-[5]`), so the raised
shadow shows over Settings. The editor wrapper stays `isolate` and `inert`.

## Contrast

Body text is 9.9:1 or better on every dark level. The global dark `--muted-foreground` (48%) is
4.35 on the editor and 4.11 on the shell, sidebar and cards, so it stays as it is to keep the look.
It is lifted to 56% only where it fails worst: nested cards and solid `bg-accent` rows (3.54 at 48%,
4.66 at 56%), and `.elevation-overlay` (3.77 at 48%, 4.97 at 56%). The collection overview lifts it
to 60%. The light shell (`#EEF0F4`) is 4.33 and is not lifted. Pinned in `elevation-tokens.test.ts`.

## WebKitGTK

Do not use `color-mix()` or add `backdrop-filter` in elevation rules. The test rejects them.

## Add a new surface

Pick the level, use its `.elevation-*` class or `bg-elevation-*`, and take the colour from the token,
never a new hex. If you need to show height, use the level's shadow.

## Manual checks

Both themes, in the running app:

1. Settings open: the sidebar shows its shadow over Settings. In dark the Settings page is darker
   than the sidebar; in light it is white.
2. Sidebar colour matches VS Code 2026 (`#191A1B` dark, `#FAFAFD` light).
3. Console panel with output: the output well is visibly darker than the panel in dark mode.
4. Cards: one thin border, flat fill, nested cards one step lighter.
5. Menus, popovers, dialogs and toasts look as before.
