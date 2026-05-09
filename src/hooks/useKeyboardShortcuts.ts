import { useEffect, useRef } from 'react';
import { sendRequest } from '@/lib/execute-request';
import { findActiveLeaf } from '@/lib/pane-utils';
import { workspaceKeys } from '@/lib/queries/workspace-queries';
import type { Workspace } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import { isRequestTab } from '@/types/pane-types';
import { useQueryClient } from '@tanstack/react-query';

// Registers global keyboard shortcuts for tab management across all pane groups.
export function useKeyboardShortcuts() {
  const qc = useQueryClient();
  const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const activeWorkspaceIdRef = useRef(activeWorkspaceId);
  useEffect(() => {
    activeWorkspaceIdRef.current = activeWorkspaceId;
  }, [activeWorkspaceId]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (!mod) return;

      const store = usePaneStore.getState();
      const { root, activeGroupId } = store;
      const activeLeaf = findActiveLeaf(root, activeGroupId);

      // Cmd/Ctrl+Enter — send the active tab's request.
      if (e.key === 'Enter') {
        e.preventDefault();
        const tab = activeLeaf.tabs.find((t) => t.id === activeLeaf.activeTabId);
        if (tab && isRequestTab(tab)) {
          sendRequest(tab.id, tab.request);
        }
        return;
      }

      // Cmd/Ctrl+S — open save-to-collection for ephemeral tabs, else save draft.
      if (e.key === 's') {
        e.preventDefault();
        const tab = activeLeaf.tabs.find((t) => t.id === activeLeaf.activeTabId);
        if (!tab) return;
        if (isRequestTab(tab) && !tab.source) {
          window.dispatchEvent(
            new CustomEvent('rocket:save-to-collection', { detail: { tabId: tab.id } }),
          );
        } else {
          window.dispatchEvent(new CustomEvent('rocket:save-draft', { detail: { tabId: tab.id } }));
        }
        return;
      }

      // Cmd/Ctrl+W — close the active tab in the active group.
      if (e.key === 'w') {
        e.preventDefault();
        store.closeTab(activeLeaf.activeTabId, activeGroupId);
        return;
      }

      // Cmd/Ctrl+L — open contracts tab for the active collection.
      if (e.key === 'l' || e.key === 'L') {
        e.preventDefault();
        const paneStore = usePaneStore.getState();
        const activeCollection = paneStore.activeCollection;
        if (!activeCollection) return;
        const workspaces =
          qc.getQueryData<Workspace[]>(workspaceKeys.all) ?? [];
        const activeWorkspace = workspaces.find(
          (w) => w.id === activeWorkspaceIdRef.current,
        );
        if (!activeWorkspace) return;
        const collectionRoot = `${activeWorkspace.path}/collections/${activeCollection}`;
        paneStore.openContractTab(activeCollection, collectionRoot);
        return;
      }

      // Cmd/Ctrl+Shift+G — open the git panel for the active collection.
      if (e.key === 'G' && e.shiftKey) {
        e.preventDefault();
        import('@/components/layout/GitToolbarButton').then(({ openGitPanel }) => {
          void openGitPanel();
        });
        return;
      }

      // Cmd/Ctrl+Tab — cycle to the next tab (wrapping) in the active group.
      if (e.key === 'Tab' && !e.shiftKey) {
        e.preventDefault();
        const { tabs, activeTabId, groupId } = activeLeaf;
        if (tabs.length < 2) return;
        const idx = tabs.findIndex((t) => t.id === activeTabId);
        const next = tabs[(idx + 1) % tabs.length];
        store.setActiveTab(next.id, groupId);
        return;
      }

      // Cmd/Ctrl+Shift+Tab — cycle to the previous tab (wrapping).
      if (e.key === 'Tab' && e.shiftKey) {
        e.preventDefault();
        const { tabs, activeTabId, groupId } = activeLeaf;
        if (tabs.length < 2) return;
        const idx = tabs.findIndex((t) => t.id === activeTabId);
        const prev = tabs[(idx - 1 + tabs.length) % tabs.length];
        store.setActiveTab(prev.id, groupId);
        return;
      }

      // Cmd/Ctrl+1 through Cmd/Ctrl+9 — jump to tab by 1-based index.
      const digit = parseInt(e.key, 10);
      if (digit >= 1 && digit <= 9) {
        e.preventDefault();
        const { tabs, groupId } = activeLeaf;
        const target = tabs[digit - 1];
        if (target) {
          store.setActiveTab(target.id, groupId);
        }
      }
    };

    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, []);
}
