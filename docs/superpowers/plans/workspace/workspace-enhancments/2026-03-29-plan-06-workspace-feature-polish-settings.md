# SP-W6: Polish + Settings — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pin sorting in switcher, tab state persistence, first-launch default workspace, edge case handling.

**Architecture:** Frontend sorting in `WorkspaceSwitcher`, new `ui-state` persistence layer, Tauri setup changes for first launch, store event listener updates for edge cases.

**Tech Stack:** Rust (fs, dirs), TypeScript (React, Zustand, Tauri)

**Spec:** `docs/superpowers/specs/2026-03-29-workspace-feature-design.md`

**Depends on:** SP-W4 and SP-W5 complete

---

## Chunk 1: Pin sorting in workspace switcher

### Task 1: Sort workspaces by pinned status in switcher dropdown

**Files:**
- Modify: `src/components/title-bar/WorkspaceSwitcher.tsx`

- [ ] **Step 1:** Find where the `workspaces` array is iterated to render workspace rows (look for `workspaces.map`). Before the map, split into two sorted arrays:

```typescript
const pinned = workspaces.filter((w) => w.pinned).sort((a, b) => a.name.localeCompare(b.name))
const unpinned = workspaces.filter((w) => !w.pinned).sort((a, b) => a.name.localeCompare(b.name))
```

- [ ] **Step 2:** Replace the single `workspaces.map(...)` with two maps separated by a `DropdownMenuSeparator`:

```tsx
{pinned.map((ws) => (
  /* existing workspace row JSX — unchanged */
))}
{pinned.length > 0 && unpinned.length > 0 && <DropdownMenuSeparator />}
{unpinned.map((ws) => (
  /* existing workspace row JSX — unchanged */
))}
```

- [ ] **Step 3:** Commit: `git commit -m "feat(switcher): sort workspaces by pinned status"`

---

### Task 2: Add pin toggle button to workspace rows in switcher

**Files:**
- Modify: `src/components/title-bar/WorkspaceSwitcher.tsx`

- [ ] **Step 1:** Import `Pin` from `lucide-react` and `pinWorkspace, unpinWorkspace` from store.

- [ ] **Step 2:** In each workspace row, add a pin toggle button (shown on hover via `group-hover`):

```tsx
<Button
  variant="ghost"
  size="icon"
  className="h-5 w-5 shrink-0 opacity-0 group-hover:opacity-100"
  onClick={(e) => {
    e.stopPropagation()
    if (ws.pinned) {
      useWorkspaceStore.getState().unpinWorkspace(ws.id)
    } else {
      useWorkspaceStore.getState().pinWorkspace(ws.id)
    }
  }}
>
  <Pin className={cn("h-3 w-3", ws.pinned && "fill-current")} />
</Button>
```

- [ ] **Step 3:** Commit: `git commit -m "feat(switcher): add pin/unpin toggle to workspace rows"`

---

### Task 3: Add "Open workspace" option to switcher dropdown

**Files:**
- Modify: `src/components/title-bar/WorkspaceSwitcher.tsx`

- [ ] **Step 1:** Import `FolderOpen` from `lucide-react` and `openFolderPicker` from `@/lib/tauri-api`.

- [ ] **Step 2:** After the "New workspace" `DropdownMenuItem`, add:

```tsx
<DropdownMenuItem onSelect={async () => {
  const path = await openFolderPicker()
  if (path) {
    try {
      await useWorkspaceStore.getState().openWorkspaceFromDisk(path)
    } catch (err) {
      console.error('Failed to open workspace:', err)
      // TODO: show error toast
    }
  }
}}>
  <FolderOpen className="h-3.5 w-3.5 mr-2" />
  Open workspace
</DropdownMenuItem>
```

- [ ] **Step 3:** Commit: `git commit -m "feat(switcher): add open workspace from disk action"`

---

## Chunk 2: Tab state persistence — backend

### Task 4: Add Tauri commands for UI state persistence

**Files:**
- Modify: `src-tauri/src/lib.rs`

The Rust backend handles YAML serialization/deserialization. The frontend sends/receives a typed `UiState` struct — not raw strings.

- [ ] **Step 1:** Add a `UiState` struct in `src-tauri/src/lib.rs` (or in a shared location):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UiState {
    active_mode: String, // "workspace" or "collection"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    workspace_tabs: Option<UiStateWorkspaceTabs>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UiStateWorkspaceTabs {
    workspace_id: String,
}
```

- [ ] **Step 2:** Add two command functions:

```rust
#[tauri::command]
fn load_ui_state(app_handle: tauri::AppHandle) -> Result<Option<UiState>, String> {
    let config_dir = app_handle.path().app_config_dir()
        .map_err(|e| e.to_string())?;
    let path = config_dir.join("ui-state.yml");
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let state: UiState = serde_yaml::from_str(&content).map_err(|e| e.to_string())?;
    Ok(Some(state))
}

#[tauri::command]
fn save_ui_state(app_handle: tauri::AppHandle, state: UiState) -> Result<(), String> {
    let config_dir = app_handle.path().app_config_dir()
        .map_err(|e| e.to_string())?;
    fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;
    let content = serde_yaml::to_string(&state).map_err(|e| e.to_string())?;
    fs::write(config_dir.join("ui-state.yml"), content).map_err(|e| e.to_string())
}
```

Note: The subagent should check which Tauri path API is available. In Tauri v2, it might be `app_handle.path().app_config_dir()` or `app_handle.path_resolver().app_config_dir()`. The subagent should look at existing path resolution code in `src-tauri/src/lib.rs` and follow the same pattern.

- [ ] **Step 3:** Register in `generate_handler!`: add `load_ui_state, save_ui_state,`.

- [ ] **Step 4:** Add frontend API types and bindings in `src/lib/tauri-api.ts`:

```typescript
export interface UiStateWorkspaceTabs {
  workspaceId: string;
}

export interface UiState {
  activeMode: 'workspace' | 'collection';
  workspaceTabs?: UiStateWorkspaceTabs;
}

export const loadUiState = () =>
  invoke<UiState | null>('load_ui_state');

export const saveUiState = (state: UiState) =>
  invoke<void>('save_ui_state', { state });
```

- [ ] **Step 5:** Commit: `git commit -m "feat(tauri): add load/save UI state commands with YAML persistence"`

---

## Chunk 3: Tab state persistence — frontend

### Task 5: Create `ui-state` persistence module

**Files:**
- Create: `src/lib/ui-state.ts`

- [ ] **Step 1:** Create the file:

```typescript
import { loadUiState, saveUiState, type UiState } from '@/lib/tauri-api'
import { usePaneStore } from '@/stores/pane-store'
import { isWorkspaceTab } from '@/types/pane-types'
import type { PaneNode } from '@/types/pane-types'

let saveTimeout: ReturnType<typeof setTimeout> | null = null

export async function restoreUiState(): Promise<UiState | null> {
  try {
    return await loadUiState()
  } catch {
    return null
  }
}

export function scheduleSaveUiState() {
  if (saveTimeout) clearTimeout(saveTimeout)
  saveTimeout = setTimeout(() => {
    const state = usePaneStore.getState()
    const isWsMode = state.isWorkspaceMode()

    const uiState: UiState = {
      activeMode: isWsMode ? 'workspace' : 'collection',
    }

    if (isWsMode) {
      const findWsId = (node: PaneNode): string | null => {
        if (node.type === 'leaf') {
          const wsTab = node.tabs.find((t) => isWorkspaceTab(t))
          if (wsTab && 'workspaceId' in wsTab) return (wsTab as any).workspaceId
          return null
        }
        return findWsId(node.children[0]) || findWsId(node.children[1])
      }
      const wsId = findWsId(state.root)
      if (wsId) uiState.workspaceTabs = { workspaceId: wsId }
    }

    saveUiState(uiState).catch(console.error)
  }, 500)
}
```

- [ ] **Step 2:** Commit: `git commit -m "feat(frontend): create UI state persistence module"`

---

### Task 6: Restore tab state on app launch

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1:** Import the persistence module:

```typescript
import { restoreUiState, scheduleSaveUiState } from '@/lib/ui-state'
```

- [ ] **Step 2:** Find the existing `useEffect` that calls `loadWorkspaces()`. After the `loadWorkspaces()` call completes, add UI state restoration:

```typescript
useEffect(() => {
  const init = async () => {
    await loadWorkspaces()
    const uiState = await restoreUiState()
    if (uiState?.activeMode === 'workspace' && uiState.workspaceTabs) {
      const { workspaceId } = uiState.workspaceTabs
      const ws = useWorkspaceStore.getState().workspaces.find((w) => w.id === workspaceId)
      if (ws) {
        usePaneStore.getState().openWorkspaceTabs(ws.id, ws.id === 'default')
      }
    }
  }
  void init()
}, [loadWorkspaces])
```

- [ ] **Step 3:** Subscribe to pane store changes for auto-save. Add another `useEffect`:

```typescript
useEffect(() => {
  const unsub = usePaneStore.subscribe(scheduleSaveUiState)
  return unsub
}, [])
```

- [ ] **Step 4:** Commit: `git commit -m "feat: restore tab state on app launch and auto-save changes"`

---

## Chunk 4: First-launch default workspace

### Task 7: Ensure default workspace has proper directory structure on first launch

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1:** Find the Tauri setup function where workspace service is created. After creating the workspace service, add a check to ensure the default workspace has `workspace.yml`, `collections/`, and `environments/` dirs:

```rust
// Ensure default workspace has proper structure on first launch.
if let Ok(active) = workspace_service.get_active() {
    let ws_yml = active.path.join("workspace.yml");
    if !ws_yml.exists() {
        let config = WorkspaceConfig::new(&active.name);
        let _ = workspace_config_repo.save(&active.path, &config);
        let _ = std::fs::create_dir_all(active.path.join("collections"));
        let _ = std::fs::create_dir_all(active.path.join("environments"));
    }
}
```

Note: `workspace_config_repo` must still be in scope. The subagent should place this code AFTER the `WorkspaceService` and `FsWorkspaceConfigRepo` are created but BEFORE they are moved into `Arc` or Tauri state. Import `WorkspaceConfig` from `rocket_workspace`.

- [ ] **Step 2:** Verify the default workspace path uses `~/Documents/RocketAPI/`. Check `FsWorkspaceRepo::new()` in `crates/rocket-infra/src/fs_workspace_repo.rs` — if the `default_workspace_path` uses a different directory, update it to use `dirs::document_dir().join("RocketAPI")`.

- [ ] **Step 3:** Commit: `git commit -m "feat: ensure default workspace structure on first launch"`

---

## Chunk 5: Edge cases

### Task 8: Handle workspace deleted/closed while its tabs are open

**Files:**
- Modify: `src/stores/workspace-store.ts`

- [ ] **Step 1:** Find the `workspace-deleted` event listener in `subscribeToEvents`. After removing the workspace from state, add tab cleanup:

```typescript
listen<{ id: string }>('workspace-deleted', ({ payload }) => {
  useWorkspaceStore.setState((s) => ({
    workspaces: s.workspaces.filter((w) => w.id !== payload.id),
    activeWorkspaceId: s.activeWorkspaceId === payload.id
      ? s.workspaces.find((w) => w.id !== payload.id)?.id ?? ''
      : s.activeWorkspaceId,
  }))
  // If deleted workspace's tabs are showing, switch to active workspace tabs.
  if (usePaneStore.getState().isWorkspaceMode()) {
    const store = useWorkspaceStore.getState()
    const activeWs = store.workspaces.find((w) => w.id === store.activeWorkspaceId)
    if (activeWs) {
      usePaneStore.getState().openWorkspaceTabs(activeWs.id, activeWs.id === 'default')
    }
  }
})
```

- [ ] **Step 2:** Apply the same pattern to the `workspace-closed` event listener.

- [ ] **Step 3:** Commit: `git commit -m "feat: handle workspace delete/close while tabs are open"`

---

### Task 9: Open workspace default tabs on initial app launch (no prior state)

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1:** In the UI state restoration logic (Task 6), add a fallback for when no prior state exists:

After the `restoreUiState()` call, if `uiState` is `null` (first ever launch), open workspace tabs for the active workspace:

```typescript
if (!uiState) {
  // First launch — open workspace default tabs.
  const store = useWorkspaceStore.getState()
  const activeWs = store.workspaces.find((w) => w.id === store.activeWorkspaceId)
  if (activeWs) {
    usePaneStore.getState().openWorkspaceTabs(activeWs.id, activeWs.id === 'default')
  }
}
```

- [ ] **Step 2:** Commit: `git commit -m "feat: open workspace tabs on first-ever app launch"`
