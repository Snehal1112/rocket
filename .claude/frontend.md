# Frontend Architecture

## State Management — Zustand Stores (`src/stores/`)

| Store | Purpose |
|---|---|
| `pane-store` | Tab groups, active tab, split layout, collection tab state snapshot |
| `workspace-store` | Workspace list, active workspace, subscriptions to workspace events |
| `env-store` | Environments, active environment |
| `git-store` | Git status, staging, commit, push/pull state |
| `console-store` | Request log for the console panel |
| `sandbox-store` | Sandbox mode (`safe` / `developer`), persisted to `localStorage` |

## Tauri IPC

All backend calls go through `src/lib/tauri-api.ts`. Never call Tauri commands directly from components.

## UI Component Layout

- `src/components/ui/` — shadcn/ui primitives
- Feature components organized by domain: `collections/`, `environments/`, `git/`, `workspace/`, `request/`, `response/`
- `src/components/panes/` — tab system and split-pane layout (driven by `pane-store`)
- `src/components/editor/` — Monaco wrapper and theme sync

## Tab System (`src/types/pane-types.ts`)

Tab types: `RequestTab | CollectionTab | WorkspaceTab | DiffTab | ConflictTab | GitTab`

- Guard functions: `isRequestTab()`, `isDiffTab()`, `isConflictTab()`, `isWorkspaceTab()`, `isGitTab()`
- `CollectionTab` sections: `'overview' | 'auth' | 'variables'`
- `WorkspaceTab` sections: `'overview' | 'environments' | 'git'` — never has a close button, opens automatically on workspace switch
- The `isDirty` flag triggers `scheduleAutoSave()` before tab close or switch
- Mode switching: opening a workspace tab closes all collection tabs (and vice versa)

## Keyboard Shortcuts (`src/hooks/useKeyboardShortcuts.ts`)

| Shortcut | Action |
|---|---|
| Cmd/Ctrl+Enter | Send active request |
| Cmd/Ctrl+S | Save draft (`rocket:save-draft` event) |
| Cmd/Ctrl+W | Close active tab |
| Cmd/Ctrl+Tab | Next tab (wraps) |
| Cmd/Ctrl+Shift+Tab | Previous tab (wraps) |
| Cmd/Ctrl+1–9 | Jump to tab by 1-based index |

## Sandbox Mode

`ResponseBodyViewer` renders HTML responses in an iframe. `useSandboxStore` (persisted to `localStorage`) holds `mode: 'safe' | 'developer'`:
- **Safe** (default): restricts JS execution in the iframe.
- **Developer**: allows full script execution.

Toggled via `SandboxPopover` in the toolbar.

## UI State Persistence

On app launch, `load_ui_state()` restores the previously active workspace and tab mode. `scheduleSaveUiState()` debounces writes (500 ms) via `save_ui_state()`. Implemented in `src-tauri/src/commands/ui_state.rs`.
