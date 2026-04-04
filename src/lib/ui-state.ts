import { loadUiState, saveUiState, type UiState } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { PaneNode } from '@/types/pane-types';
import { isWorkspaceTab } from '@/types/pane-types';

let saveTimeout: ReturnType<typeof setTimeout> | null = null;

export async function restoreUiState(): Promise<UiState | null> {
  try {
    return await loadUiState();
  } catch {
    return null;
  }
}

export function scheduleSaveUiState() {
  if (saveTimeout) clearTimeout(saveTimeout);
  saveTimeout = setTimeout(() => {
    const state = usePaneStore.getState();
    const isWsMode = state.isWorkspaceMode();

    const uiState: UiState = {
      activeMode: isWsMode ? 'workspace' : 'collection',
    };

    if (isWsMode) {
      const findWsId = (node: PaneNode): string | null => {
        if (node.type === 'leaf') {
          const wsTab = node.tabs.find((t) => isWorkspaceTab(t));
          if (wsTab && 'workspaceId' in wsTab)
            return (wsTab as { workspaceId: string }).workspaceId;
          return null;
        }
        return findWsId(node.children[0]) || findWsId(node.children[1]);
      };
      const wsId = findWsId(state.root);
      if (wsId) uiState.workspaceTabs = { workspaceId: wsId };
    }

    saveUiState(uiState).catch(console.error);
  }, 500);
}
