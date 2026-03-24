import { useEffect } from 'react';
import { usePaneStore } from '@/stores/pane-store';
import { findActiveLeaf } from '@/lib/pane-utils';
import { sendRequest } from '@/lib/execute-request';

// Registers global keyboard shortcuts for tab management across all pane groups.
export function useKeyboardShortcuts() {
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
        if (tab) {
          sendRequest(tab.id, tab.request);
        }
        return;
      }

      // Cmd/Ctrl+S — save draft to collection.
      if (e.key === 's') {
        e.preventDefault();
        const tab = activeLeaf.tabs.find((t) => t.id === activeLeaf.activeTabId);
        if (tab && tab.tabType === 'draft') {
          window.dispatchEvent(new CustomEvent('rocket:save-draft', { detail: { tabId: tab.id } }));
        }
        return;
      }

      // Cmd/Ctrl+N — open a new draft tab in the active group.
      if (e.key === 'n') {
        e.preventDefault();
        store.newDraftTab(activeGroupId);
        return;
      }

      // Cmd/Ctrl+W — close the active tab in the active group.
      if (e.key === 'w') {
        e.preventDefault();
        store.closeTab(activeLeaf.activeTabId, activeGroupId);
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
