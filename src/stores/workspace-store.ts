import { listen } from '@tauri-apps/api/event';
import { create } from 'zustand';
import {
  closeWorkspace as apiClose,
  createWorkspace as apiCreate,
  deleteWorkspace as apiDelete,
  openWorkspaceFromDisk as apiOpenFromDisk,
  pinWorkspace as apiPin,
  renameWorkspace as apiRename,
  setMultiWorkspaceMode as apiSetMultiMode,
  switchWorkspace as apiSwitch,
  unpinWorkspace as apiUnpin,
  updateWorkspaceDescription as apiUpdateDescription,
  getActiveWorkspace,
  getMultiWorkspaceMode,
  listWorkspaces,
  type Workspace,
} from '@/lib/tauri-api';
import { useEnvStore } from '@/stores/env-store';
import { usePaneStore } from '@/stores/pane-store';
import type { PaneNode } from '@/types/pane-types';

interface WorkspaceState {
  workspaces: Workspace[];
  activeWorkspaceId: string;
  initialized: boolean;
  multiWorkspaceMode: boolean;
  loadWorkspaces: () => Promise<void>;
  createWorkspace: (name: string, path: string) => Promise<void>;
  switchWorkspace: (id: string) => Promise<void>;
  renameWorkspace: (id: string, newName: string) => Promise<void>;
  closeWorkspace: (id: string) => Promise<void>;
  deleteWorkspace: (id: string) => Promise<void>;
  pinWorkspace: (id: string) => Promise<void>;
  unpinWorkspace: (id: string) => Promise<void>;
  updateDescription: (id: string, description: string | null) => Promise<void>;
  openWorkspaceFromDisk: (path: string) => Promise<Workspace>;
  setMultiWorkspaceMode: (enabled: boolean) => Promise<void>;
}

// Module-level guard so concurrent loadWorkspaces() calls await one promise.
let initPromise: Promise<void> | null = null;

export const useWorkspaceStore = create<WorkspaceState>((set, get) => ({
  workspaces: [],
  activeWorkspaceId: '',
  initialized: false,
  multiWorkspaceMode: false,

  loadWorkspaces: async () => {
    if (get().initialized) return;
    if (initPromise) return initPromise;
    initPromise = (async () => {
      const [workspaces, active, mode] = await Promise.all([
        listWorkspaces(),
        getActiveWorkspace(),
        getMultiWorkspaceMode(),
      ]);
      set({
        workspaces,
        activeWorkspaceId: active.id,
        multiWorkspaceMode: mode,
        initialized: true,
      });
      subscribeToEvents();
    })();
    return initPromise;
  },

  createWorkspace: async (name, path) => {
    await apiCreate(name, path);
  },
  switchWorkspace: async (id) => {
    await apiSwitch(id);
  },
  renameWorkspace: async (id, newName) => {
    await apiRename(id, newName);
  },
  closeWorkspace: async (id) => {
    await apiClose(id);
  },
  deleteWorkspace: async (id) => {
    await apiDelete(id);
  },
  pinWorkspace: async (id) => {
    await apiPin(id);
  },
  unpinWorkspace: async (id) => {
    await apiUnpin(id);
  },
  updateDescription: async (id, description) => {
    await apiUpdateDescription(id, description);
  },
  openWorkspaceFromDisk: async (path) => {
    const ws = await apiOpenFromDisk(path);
    return ws;
  },
  setMultiWorkspaceMode: async (enabled) => {
    await apiSetMultiMode(enabled);
    set({ multiWorkspaceMode: enabled });
  },
}));

function subscribeToEvents() {
  listen<Workspace>('workspace-created', ({ payload }) => {
    useWorkspaceStore.setState((s) => ({
      workspaces: [...s.workspaces, payload],
    }));
  });

  listen<Workspace>('workspace-switched', ({ payload }) => {
    // Clear all tabs from the previous workspace before activating the new one.
    usePaneStore.getState().closeAll();
    useWorkspaceStore.setState({ activeWorkspaceId: payload.id });
    usePaneStore.getState().openWorkspaceTabs(payload.id);
    // Clear env state — the new workspace's collection will reload envs via switchCollection.
    useEnvStore.setState({ environments: [], activeEnvId: null, activeCollection: null });
  });

  listen<{ id: string; oldName: string; newName: string }>('workspace-renamed', ({ payload }) => {
    useWorkspaceStore.setState((s) => ({
      workspaces: s.workspaces.map((w) =>
        w.id === payload.id ? { ...w, name: payload.newName } : w,
      ),
    }));
  });

  listen<{ id: string }>('workspace-closed', ({ payload }) => {
    useWorkspaceStore.setState((s) => ({
      workspaces: s.workspaces.filter((w) => w.id !== payload.id),
    }));
    if (usePaneStore.getState().isWorkspaceMode()) {
      const store = useWorkspaceStore.getState();
      const activeWs = store.workspaces.find((w) => w.id === store.activeWorkspaceId);
      if (activeWs) {
        usePaneStore.getState().openWorkspaceTabs(activeWs.id);
      }
    }
  });

  listen<{ id: string }>('workspace-deleted', ({ payload }) => {
    useWorkspaceStore.setState((s) => {
      const deleted = s.workspaces.find((w) => w.id === payload.id);
      if (deleted) closeTabsForWorkspacePath(deleted.path);
      return { workspaces: s.workspaces.filter((w) => w.id !== payload.id) };
    });
    if (usePaneStore.getState().isWorkspaceMode()) {
      const store = useWorkspaceStore.getState();
      const activeWs = store.workspaces.find((w) => w.id === store.activeWorkspaceId);
      if (activeWs) {
        usePaneStore.getState().openWorkspaceTabs(activeWs.id);
      }
    }
  });

  listen<{ id: string }>('workspace-pinned', ({ payload }) => {
    useWorkspaceStore.setState((s) => ({
      workspaces: s.workspaces.map((w) => (w.id === payload.id ? { ...w, pinned: true } : w)),
    }));
  });

  listen<{ id: string }>('workspace-unpinned', ({ payload }) => {
    useWorkspaceStore.setState((s) => ({
      workspaces: s.workspaces.map((w) => (w.id === payload.id ? { ...w, pinned: false } : w)),
    }));
  });

  listen<{ id: string; description: string | null }>(
    'workspace-description-updated',
    ({ payload }) => {
      useWorkspaceStore.setState((s) => ({
        workspaces: s.workspaces.map((w) =>
          w.id === payload.id ? { ...w, description: payload.description } : w,
        ),
      }));
    },
  );
}

function closeTabsForWorkspacePath(workspacePath: string) {
  const store = usePaneStore.getState();
  const closeInNode = (node: PaneNode): void => {
    if (node.type === 'leaf') {
      for (const tab of node.tabs) {
        if (tab.source?.collection.startsWith(workspacePath)) {
          store.closeTab(tab.id, node.groupId);
        }
      }
    } else {
      closeInNode(node.children[0]);
      closeInNode(node.children[1]);
    }
  };
  closeInNode(store.root);
}
