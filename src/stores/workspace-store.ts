import { create } from 'zustand';

interface WorkspaceState {
  activeWorkspaceId: string;
  multiWorkspaceMode: boolean;
  setActiveWorkspaceId: (id: string) => void;
  setMultiWorkspaceMode: (enabled: boolean) => void;
}

export const useWorkspaceStore = create<WorkspaceState>()((set) => ({
  activeWorkspaceId: '',
  multiWorkspaceMode: false,
  setActiveWorkspaceId: (id) => set({ activeWorkspaceId: id }),
  setMultiWorkspaceMode: (enabled) => set({ multiWorkspaceMode: enabled }),
}));
