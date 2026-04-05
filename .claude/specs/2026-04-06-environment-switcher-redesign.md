# Environment Switcher Redesign

**Date:** 2026-04-06
**Scope:** Single component rewrite — `src/components/layout/EnvironmentSwitcher.tsx`

## Problem

The current EnvironmentSwitcher dropdown has two flat sections ("Global" and "Environment") that both list collection-scoped environments. This is confusing because:

1. The "Global" section doesn't show workspace-level environments — it re-uses the collection list.
2. There's no visual distinction between the two scopes.
3. No empty state guidance when no environments exist.

## Design

Rebuild EnvironmentSwitcher as a Popover (not DropdownMenu) with two tabs: **Collection** and **Global**.

### Collection Tab

- Lists `useEnvStore.environments` (collection-scoped environments).
- "No Environment" option at top to deselect.
- Check mark next to active environment (`activeEnvId`).
- Selecting calls `setActiveEnv(name)` / `setActiveEnv(null)`.
- "Configure" link at bottom opens `EnvironmentDialog`.

### Global Tab

- Lists `useEnvStore.globalEnvironments` (workspace-level environments).
- "No Global Environment" option at top to deselect.
- Check mark next to active global environment (`globalEnvName`).
- Selecting calls `setGlobalEnv(name)` / `setGlobalEnv(null)`.
- "Configure" link at bottom opens WorkspaceEnvironmentsTab via `usePaneStore.openTab()`.
- Empty state: "Ready to get started?" with Create button (inline name input) and Configure link.

### Trigger Button

- Shows active collection environment name, or "No Environment" in muted text.
- Teal dot indicator when a global environment is active (with tooltip).
- ChevronDown icon.

### Data Loading

- `globalEnvironments` must be loaded. The component calls `loadGlobalEnvironments()` on mount if the list is empty, so the Global tab always has data.

## Files Changed

- `src/components/layout/EnvironmentSwitcher.tsx` — full rewrite
- No backend changes
- No store changes
- No other component changes

## Not In Scope

- Reworking EnvironmentDialog or WorkspaceEnvironmentsTab
- Changing variable resolution logic
- Adding import functionality to the Global tab
