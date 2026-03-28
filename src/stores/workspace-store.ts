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
