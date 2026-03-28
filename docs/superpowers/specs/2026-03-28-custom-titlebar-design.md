# Custom Title Bar — Design Spec
Date: 2026-03-28

## Overview

Replace Rocket's native OS title bar with a custom title bar inspired by Bruno. The bar shows the app logo and name on the left, a workspace switcher centered, and platform-appropriate window controls. The implementation targets macOS, Windows, and Linux.

## Layout

### macOS
```
[ ● ● ● ] | 🚀 Rocket   [My Workspace ▾]   <drag region>
```
- Native traffic lights (close/min/max) on the far left via `titleBarStyle: "Overlay"`
- Divider separating traffic lights from app identity
- Logo + "Rocket" text anchored left
- Workspace switcher centered between two equal flex spacers
- Right spacer acts as drag region

### Windows / Linux
```
🚀 Rocket   [My Workspace ▾]   [ — ] [ ▢ ] [ ✕ ]
```
- Logo + "Rocket" text anchored left (no native controls)
- Workspace switcher centered between two equal flex spacers
- Custom win32-style buttons flush to the right edge
- Full bar (minus buttons) is the drag region

## Components

### `TitleBar`
- Top-level component rendered outside the main router, always visible
- Detects OS at runtime via `@tauri-apps/plugin-os`
- Conditionally renders `MacTitleBar` or `WinTitleBar` layout variant
- Applies `data-tauri-drag-region` to all non-interactive regions
- Uses shadcn/ui primitives throughout (no raw divs for interactive elements)

### `WorkspaceSwitcher`
- Uses shadcn `DropdownMenu` (trigger + content)
- Reads workspace list and active workspace from global store
- Dispatches workspace change action on selection
- Bottom of dropdown: "Add workspace" item separated by a `DropdownMenuSeparator`
- Active workspace highlighted with info background
- Closes on outside click or `Escape` (handled by shadcn primitives)

### `WindowControls` (Windows/Linux only)
- Three shadcn `Button` components: minimize, maximize/restore, close
- Calls `@tauri-apps/api/window`: `appWindow.minimize()`, `appWindow.toggleMaximize()`, `appWindow.close()`
- Close button gets destructive hover style (red background)
- Not rendered on macOS

## Tauri Configuration

### macOS
```json
{
  "titleBarStyle": "Overlay",
  "hiddenTitle": true
}
```
Native traffic lights are preserved. Custom HTML fills the rest of the bar area.

### Windows / Linux
```json
{
  "decorations": false
}
```
Native title bar removed entirely. Custom window controls handle all window management.

Platform detection happens at runtime — a single `tauri.conf.json` can use `decorations: false` globally with the macOS overlay handled via `titleBarStyle` override, or use Tauri's platform-specific config files (`tauri.macos.conf.json`, `tauri.windows.conf.json`).

## Data Flow

```
Global Store (Zustand / existing)
  └── workspaces: Workspace[]
  └── activeWorkspaceId: string

WorkspaceSwitcher
  reads  → workspaces, activeWorkspaceId
  writes → setActiveWorkspace(id)
```

The title bar owns no data — it is purely a presentational + interaction layer over the existing store.

## Styling

- Height: 44px
- Background: `var(--color-background-secondary)` or match existing sidebar header
- All interactive elements: shadcn/ui primitives
- Workspace button: shadcn `Button` variant ghost or outline with caret icon
- Drag region: `data-tauri-drag-region` attribute, `cursor: default`
- No box-shadows or gradients — flat surface consistent with the rest of the UI

## Out of Scope

- Environment selector (not requested)
- Sidebar toggle button in title bar (lives in sidebar, not title bar)
- Title bar theming / color customization
