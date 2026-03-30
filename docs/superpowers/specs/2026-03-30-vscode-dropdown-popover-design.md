# VS Code Modern Style: Dropdown, Popover, Context Menu

**Date:** 2026-03-30
**Scope:** `src/components/ui/dropdown-menu.tsx`, `src/components/ui/popover.tsx`, `src/components/ui/context-menu.tsx`

## Goal

Bring the three menu/overlay primitives in line with VS Code's modern menu aesthetic: compact items, sharp full-width hover highlights, frosted-glass container with subtle border and directional shadow. No changes to the broader theme system, Zustand stores, Tauri commands, or any consumer components.

## Container Changes

Applies to: `DropdownMenuContent`, `DropdownMenuSubContent`, `ContextMenuContent`, `ContextMenuSubContent`, `PopoverContent`.

| Property | Before | After |
|---|---|---|
| Border radius | `rounded-md` | `rounded-sm` |
| Background | `bg-popover` | `bg-popover/95 backdrop-blur-sm` |
| Shadow | `shadow-md` | `shadow-[0_2px_8px_rgba(0,0,0,0.16)] dark:shadow-[0_2px_8px_rgba(0,0,0,0.4)]` |
| Border | `border` | `border border-border/60` |
| Animations | existing | unchanged |
| Popover default padding | `p-4` | `p-0` |

The `bg-popover/95` opacity is required for `backdrop-blur-sm` to be visible. At 95% opacity the blur effect is subtle and readable in both light and dark themes.

## Item Changes

Applies to: `DropdownMenuItem`, `DropdownMenuSubTrigger`, `DropdownMenuCheckboxItem`, `DropdownMenuRadioItem`, `ContextMenuItem`, `ContextMenuSubTrigger`, `ContextMenuCheckboxItem`, `ContextMenuRadioItem`.

| Property | Before | After |
|---|---|---|
| Border radius | `rounded-sm` | `rounded-none` |
| Padding | `px-2 py-1.5` | `px-3 py-1` |
| Hover/focus background | `focus:bg-accent` | unchanged |
| Separator | `bg-muted` | `bg-border/60` |

The `rounded-none` + full-width layout produces VS Code's flat selection highlight. The compact `py-1` brings item height to approximately 22px, matching VS Code's menu row height.

## What Does Not Change

- Animations (fade-in/out, zoom, slide) — kept as-is.
- Accent/hover color tokens — existing tokens already map to VS Code selection colors.
- Label, shortcut, and separator font styles.
- All consumer components (`SandboxPopover`, `CollectionDropdown`, `RequestNode`, etc.) — no changes needed since they override padding/width independently.
- `context-menu.tsx` separator already uses `bg-border` — updated to `bg-border/60` for consistency.

## Files Changed

1. `src/components/ui/dropdown-menu.tsx`
2. `src/components/ui/popover.tsx`
3. `src/components/ui/context-menu.tsx`
