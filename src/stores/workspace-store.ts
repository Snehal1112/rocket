import { create } from 'zustand'

interface Workspace {
  id: string
  name: string
}

interface WorkspaceState {
  workspaces: Workspace[]
  activeWorkspaceId: string
  setActiveWorkspace: (id: string) => void
}

export const useWorkspaceStore = create<WorkspaceState>((set) => ({
  workspaces: [{ id: 'default', name: 'Default Workspace' }],
  activeWorkspaceId: 'default',
  setActiveWorkspace: (id) => set({ activeWorkspaceId: id }),
}))
