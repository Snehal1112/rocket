import { loadUiState, saveUiState, type UiState, type UiStateCollectionTab } from '@/lib/tauri-api';
import { useLayoutStore } from '@/stores/layout-store';
import { usePaneStore } from '@/stores/pane-store';
import type { CollectionTab, PaneNode } from '@/types/pane-types';
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
    const layoutState = useLayoutStore.getState();

    const uiState: UiState = {
      activeMode: isWsMode ? 'workspace' : 'collection',
      layoutDirection: layoutState.requestLayout,
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
    } else if (state.activeCollection) {
      // Persist which collection was active and its open collection-type tabs
      // so they can be restored after a reload.
      uiState.activeCollection = state.activeCollection;

      const collectCollectionTabs = (node: PaneNode): UiStateCollectionTab[] => {
        if (node.type === 'leaf') {
          return node.tabs
            .filter((t): t is CollectionTab => t.tabType === 'collection')
            .map((t) => ({
              id: t.id,
              title: t.title,
              collectionName: t.collectionName,
              activeSection: t.activeSection,
            }));
        }
        return [
          ...collectCollectionTabs(node.children[0]),
          ...collectCollectionTabs(node.children[1]),
        ];
      };
      uiState.collectionTabs = collectCollectionTabs(state.root);
    }

    saveUiState(uiState).catch(console.error);
  }, 500);
}
