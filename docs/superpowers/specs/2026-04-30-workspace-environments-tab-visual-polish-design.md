# WorkspaceEnvironmentsTab — Visual Polish Design

**Date:** 2026-04-30
**Status:** Approved

## Problem

`WorkspaceEnvironmentsTab` has accumulated visual noise through layered borders, inconsistent surface colors, cramped spacing, and a custom button-as-checkbox that diverges from both the shadcn `Checkbox` primitive and the pattern already used in `EnvironmentDialog`.

Specific issues:

- The left panel has its own outer border (`border border-border`) plus a right-side divider, creating a double-border effect where the two panels meet.
- The right panel is wrapped in a `Card` with `rounded-none` and `.border-0` (a no-op class), adding a card surface that conflicts with the tab background.
- Column headers have a bottom border; the footer has a top border; the left panel button bar has its own top border — four horizontal separator lines in a single view.
- Variable rows use `h-7` (28px) with `space-y-1` (4px gap) which feels cramped.
- The enabled toggle is a `Button` styled to look like a checkbox, not the `Checkbox` primitive — inconsistent with `EnvironmentDialog`.

## Goals

1. Reduce visual noise: fewer borders, cleaner surface separation.
2. Improve spacing rhythm: slightly more breathing room in the variable list.
3. Fix the checkbox inconsistency: use the shadcn `Checkbox` primitive.
4. Keep Key and Value fields equal width using a CSS grid layout.
5. No behavior changes — purely visual.

## Out of Scope

- Functional changes (save flow, rename, delete, env switching).
- Changes to `EnvironmentDialog` or `InlineEnvName`.
- Dark/light theme token changes.

## Design

### Left panel

- Remove the outer `border border-border` wrapper. Keep only the right-side divider: `border-r border-border`.
- Keep `bg-card/50` for the subtle tonal separation.
- Button bar at the bottom: remove the `border-t border-border/60`. Ghost icon buttons are sufficient without the separator line.

### Right panel

- Remove the `Card` / `CardContent` wrapper entirely. The panel is a plain `div`.
- Remove `border-t border-border` from the panel's outer div (the left panel's right-side border already separates the two).
- Column header row: remove the `border-b border-border/40`. Spacing alone provides the separation.
- Footer row: keep one `border-t border-border/40` separator — a single line to anchor the save action.

### Variable rows — grid layout

Replace the `flex` layout on variable rows (and column headers) with a CSS grid:

```
grid-template-columns: 20px 1fr 1fr 52px
gap: 6px (gap-1.5)
```

- `20px` — checkbox column (fixed).
- `1fr 1fr` — Key and Value fields receive identical computed width.
- `52px` — action buttons column (2 × 24px buttons + 4px gap).

Column headers use the same grid template so labels sit directly above their fields.

Row height: `h-8` (32px) instead of `h-7` (28px). Input height stays `h-7` — the extra row height comes from `items-center` within the taller grid row, giving more vertical breathing room between rows.

### Checkbox fix

Replace the custom `Button`-as-checkbox with the shadcn `Checkbox` primitive:

```tsx
<Checkbox
  checked={variable.enabled}
  onCheckedChange={(checked) => updateVar(idx, { enabled: !!checked })}
  aria-label={variable.enabled ? 'Disable variable' : 'Enable variable'}
  className='shrink-0'
/>
```

This matches the pattern already used in `EnvironmentDialog`.

## Files Changed

| File | Change |
|---|---|
| `src/components/workspace/WorkspaceEnvironmentsTab.tsx` | All visual changes described above |

No other files need to change.

## Theme Compatibility

All changes use existing CSS variable tokens (`border-border`, `bg-card`, `text-muted-foreground`, etc.) from `globals.css`, which maps VSCode 2026 Light and Dark spec values. Both themes work correctly with no additional token changes.
