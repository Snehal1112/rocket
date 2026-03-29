# Plan 06 — Frontend tauri-api and workspace-store

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend `tauri-api.ts` with workspace bindings and replace the `workspace-store.ts` stub with a real Zustand store that loads from Tauri and reacts to domain events.

**Architecture:** `tauri-api.ts` gets typed wrappers for all 8 workspace commands. `workspace-store.ts` replaces the current stub (which has hardcoded `[{ id: 'default', name: 'Default Workspace' }]`) with a store that calls Tauri on load, subscribes to all 5 workspace domain events, and closes orphaned pane tabs on `WorkspaceDeleted`.

**Tech Stack:** TypeScript, Zustand, `@tauri-apps/api/core` (invoke), `@tauri-apps/api/event` (listen)

**Spec:** `docs/superpowers/specs/2026-03-28-workspace-feature-design.md`

**Previous plan:** `plan-05-tauri-commands.md`
**Next plan:** `plan-07-create-workspace-dialog.md`

---

### Task 1: Add workspace types and commands to tauri-api.ts

**Files:**
- Modify: `src/lib/tauri-api.ts`

- [ ] **Step 1: Add `Workspace` type and 8 command wrappers to `src/lib/tauri-api.ts`**

Add after the existing type definitions:

```ts
export interface Workspace {
  id: string
  name: string
  path: string
}
```

Add after the existing command exports:

```ts
// ============================================================
// Workspace commands
// ============================================================

export const listWorkspaces = () =>
  invoke<Workspace[]>('list_workspaces')

export const getActiveWorkspace = () =>
  invoke<Workspace>('get_active_workspace')

export const createWorkspace = (name: string, path: string) =>
  invoke<Workspace>('create_workspace', { name, path })

export const switchWorkspace = (id: string) =>
  invoke<Workspace>('switch_workspace', { id })

export const renameWorkspace = (id: string, newName: string) =>
  invoke<void>('rename_workspace', { id, newName })

export const closeWorkspace = (id: string) =>
  invoke<void>('close_workspace', { id })

export const deleteWorkspace = (id: string) =>
  invoke<void>('delete_workspace', { id })

export const openFolderPicker = () =>
  invoke<string | null>('open_folder_picker')
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Expected: no type errors.

- [ ] **Step 3: Commit**

```bash
git add src/lib/tauri-api.ts
git commit -m "feat(workspace): add Workspace type and command wrappers to tauri-api"
```

---

### Task 2: Replace workspace-store.ts stub

**Files:**
- Modify: `src/stores/workspace-store.ts`

- [ ] **Step 1: Replace entire contents of `src/stores/workspace-store.ts`**

```ts
import { create } from 'zustand'
import { listen } from '@tauri-apps/api/event'
import {
  listWorkspaces,
  getActiveWorkspace,
  createWorkspace as apiCreate,
  switchWorkspace as apiSwitch,
  renameWorkspace as apiRename,
  closeWorkspace as apiClose,
  deleteWorkspace as apiDelete,
  type Workspace,
} from '@/lib/tauri-api'
import { usePaneStore } from '@/stores/pane-store'
import type { PaneNode } from '@/types/pane-types'

interface WorkspaceState {
  workspaces: Workspace[]
  activeWorkspaceId: string
  initialized: boolean
  loadWorkspaces: () => Promise<void>
  createWorkspace: (name: string, path: string) => Promise<void>
  switchWorkspace: (id: string) => Promise<void>
  renameWorkspace: (id: string, newName: string) => Promise<void>
  closeWorkspace: (id: string) => Promise<void>
  deleteWorkspace: (id: string) => Promise<void>
}

export const useWorkspaceStore = create<WorkspaceState>((set, get) => ({
  workspaces: [],
  activeWorkspaceId: '',
  initialized: false,

  loadWorkspaces: async () => {
    if (get().initialized) return
    const [workspaces, active] = await Promise.all([
      listWorkspaces(),
      getActiveWorkspace(),
    ])
    set({ workspaces, activeWorkspaceId: active.id, initialized: true })
    subscribeToEvents()
  },

  createWorkspace: async (name, path) => { await apiCreate(name, path) },
  switchWorkspace: async (id) => { await apiSwitch(id) },
  renameWorkspace: async (id, newName) => { await apiRename(id, newName) },
  closeWorkspace: async (id) => { await apiClose(id) },
  deleteWorkspace: async (id) => { await apiDelete(id) },
}))

function subscribeToEvents() {
  listen<Workspace>('workspace-created', ({ payload }) => {
    useWorkspaceStore.setState((s) => ({
      workspaces: [...s.workspaces, payload],
    }))
  })

  listen<Workspace>('workspace-switched', ({ payload }) => {
    useWorkspaceStore.setState({ activeWorkspaceId: payload.id })
  })

  listen<{ id: string; oldName: string; newName: string }>(
    'workspace-renamed',
    ({ payload }) => {
      useWorkspaceStore.setState((s) => ({
        workspaces: s.workspaces.map((w) =>
          w.id === payload.id ? { ...w, name: payload.newName } : w,
        ),
      }))
    },
  )

  listen<{ id: string }>('workspace-closed', ({ payload }) => {
    useWorkspaceStore.setState((s) => ({
      workspaces: s.workspaces.filter((w) => w.id !== payload.id),
    }))
  })

  listen<{ id: string }>('workspace-deleted', ({ payload }) => {
    useWorkspaceStore.setState((s) => {
      const deleted = s.workspaces.find((w) => w.id === payload.id)
      if (deleted) closeTabsForWorkspacePath(deleted.path)
      return { workspaces: s.workspaces.filter((w) => w.id !== payload.id) }
    })
  })
}

function closeTabsForWorkspacePath(workspacePath: string) {
  const store = usePaneStore.getState()
  const closeInNode = (node: PaneNode): void => {
    if (node.type === 'leaf') {
      for (const tab of node.tabs) {
        if (tab.source?.collection.startsWith(workspacePath)) {
          store.closeTab(tab.id, node.groupId)
        }
      }
    } else {
      closeInNode(node.children[0])
      closeInNode(node.children[1])
    }
  }
  closeInNode(store.root)
}
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Expected: no type errors. If `PaneNode` or `tab.source` types don't match, adjust to the actual shape in `src/types/pane-types.ts`.

- [ ] **Step 3: Commit**

```bash
git add src/stores/workspace-store.ts
git commit -m "feat(workspace): replace workspace-store stub with real Tauri-backed Zustand store"
```

---

### Task 3: Call loadWorkspaces on app startup

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Add `loadWorkspaces` call to `App.tsx`**

In `App.tsx`, import and call `loadWorkspaces` in a `useEffect`:

```tsx
import { useWorkspaceStore } from '@/stores/workspace-store'

// Inside the App() function, alongside the existing useEffect/useKeyboardShortcuts:
const loadWorkspaces = useWorkspaceStore((s) => s.loadWorkspaces)
useEffect(() => { void loadWorkspaces() }, [loadWorkspaces])
```

- [ ] **Step 2: Run the app and verify store loads**

```bash
yarn tauri dev
```

Open browser devtools. In the React DevTools or console, check:
```js
// In console:
window.__zustand_workspace = require('@/stores/workspace-store').useWorkspaceStore.getState()
// Or just observe that the WorkspaceSwitcher title bar area shows "Default Workspace"
```

Expected: `workspaces` array has at least 1 entry, `initialized` is `true`.

- [ ] **Step 3: Commit**

```bash
git add src/App.tsx
git commit -m "feat(workspace): call loadWorkspaces on app startup"
```
