# Elevation system design

Applies Atlassian's four elevation types (sunken, default, raised, overlay) to Rocket's light and
dark themes on top of the VS Code style token system in `src/globals.css`. Reference:
https://atlassian.design/foundations/elevation

## Problem

- In the dark theme the Collections sidebar and the Settings nav were the same colour (`#191A1B`).
  Settings now sits on the editor colour (`#121314`), but the edge between the sidebar and the
  content is still flat: `.dark .sidebar-elevation` sets `box-shadow: none` (`src/globals.css:539`).
- Overlays are inconsistent. Menus use `bg-dropdown-bg/50` plus blur and a custom 2px/8px shadow.
  Dialogs use `bg-card` plus `shadow-lg`. The sheet uses `bg-background` plus `shadow-lg`. The
  editor hover tip uses `bg-card` plus `shadow-md`. `--popover` is used by no overlay.
- The console output well uses `bg-background/60` over a `--panel-bg` surface of the same colour,
  so in dark mode it shows no well at all (`src/components/layout/ConsolePanel.tsx:134`).

## Rules (from Atlassian, adapted)

1. Four levels. Each pairs a surface token with a shadow token. Never mix tokens across levels.
2. Colours are VS Code 2026 only, never Atlassian's. Height is shown by shadow and layering, not by inventing lighter surfaces. In dark the raised level keeps the sidebar colour. Shadows are black at a higher
   alpha than in light.
3. Sunken and default carry no shadow. Use borders and spacing to separate areas on them.
4. Raised is for the sidebar and cards. Overlay is for UI that floats: menus, popovers, dialogs,
   sheets, toasts and the editor hover tip. Overlays may stack.
5. Hover and press change the surface (`list-hover-bg`, `list-active-bg`). No elevation transitions.
6. Scrolled content gets a border, not a shadow. The Atlassian "overflow" shadow is out of scope.

## Tokens

Raw HSL triples for surfaces, full `box-shadow` values for shadows. Defined in `:root` and `.dark`.

| Token | Dark | Light | Used by |
|---|---|---|---|
| `--elevation-sunken-bg` | `210 5% 7.5%` (`#121314`) | `0 0% 100%` | Editor, Settings page, console output well |
| `--elevation-surface-bg` | `210 5% 10%` (`#181A1B`) | `220 21.4% 94.5%` | Title bar, status bar, console panel, shell |
| `--elevation-raised-bg` | `210 5% 10%` (`#191A1B`, the VS Code sidebar colour) | `240 50% 99%` | Collections sidebar, cards |
| `--elevation-overlay-bg` | `210 3% 13%` (`#202122`, editorWidget) | `0 0% 100%` | Menus, popovers, dialogs, sheet, toast, hover tip |
| `--elevation-shadow-raised` | `2px 0 6px rgba(0,0,0,0.45), 6px 0 16px rgba(0,0,0,0.3)` | `2px 0 4px rgba(0,0,0,0.08), 4px 0 12px rgba(0,0,0,0.05)` | Sidebar edge |
| `--elevation-shadow-overlay` | `0 2px 6px rgba(0,0,0,0.4), 0 8px 24px rgba(0,0,0,0.55)` | `0 2px 6px rgba(9,30,66,0.12), 0 8px 24px rgba(9,30,66,0.18)` | Floating UI |

Light keeps the VS Code ordering (the editor is white, the brightest layer) and shows height by
shadow only. The lightness-ordering rule is tested for dark.

Classes, unlayered like the existing `.sidebar-elevation`: `.elevation-sunken`, `.elevation-surface`,
`.elevation-raised`, `.elevation-overlay`. Each sets the surface, and raised and overlay also set the
shadow. Tailwind aliases: `bg-elevation-sunken|surface|raised|overlay`. Do not combine an
`.elevation-*` class with a `bg-*` utility on one element.

## Contrast

Measured with the WCAG formula. Body text (`--foreground`, 82%) is 9.9:1 or better on every dark
level. Secondary text (`--muted-foreground`, 48%) is 4.35 on sunken, 4.11 on surface, 3.83 on raised
and 3.54 on overlay, all under 4.5. The global token is not changed, because that shifts the text
hierarchy of the whole app. Instead `.dark .elevation-raised` and `.dark .elevation-overlay` set
`--muted-foreground: 0 0% 56%` (4.66:1 on overlay, higher on raised). This follows the existing
`.collection-overview-workbench` precedent. The 48% baseline on sunken and surface is logged as a
follow-up in the roadmap, not fixed here.

## Findings that shape the plan

- Nothing clips a sidebar shadow. The sidebar wrapper (`src/App.tsx:202-205`) has no overflow
  class, and the shell is plain flex boxes. There is no resizable panel around the sidebar.
- The Settings overlay is `absolute inset-0 z-20` in `<main>` (`src/components/layout/MainArea.tsx:18`),
  and `<main>` is not a stacking context. It stacks above the sidebar wrapper (`z-[5]`) and paints
  over the shadow. The fix is a z-order change: Settings drops to `z-[1]`, still above the editor's
  `isolate` wrapper.
- The resize handle (`z-10`) paints its 1px line over the first pixel of the shadow. Accepted.
- Cards already behave as raised: `.dark [data-slot="card"]` uses `--popover` (13%). They move to the
  raised token (12.5%). Nested cards keep `--accent` (15%).
- Radix content is portaled to `body`, so sidebar z-order does not affect menus or dialogs.

## Exemptions and non-goals

- Tooltips keep their inverse surface (`bg-primary`). Atlassian also treats them as inverse.
- Dialog and sheet scrims (`bg-black/80` versus `bg-background/20` plus blur) stay as they are.
- Inputs: `Input` is transparent, `Textarea` uses `bg-input-bg`, the select trigger uses
  `dark:bg-input/30`. Standardising them is a separate change, not part of elevation.
- Light theme surfaces do not change. Only the overlay shadow gets a cool tint.
- No `color-mix()` and no new `backdrop-filter`: both caused WebKitGTK render problems on this app.
