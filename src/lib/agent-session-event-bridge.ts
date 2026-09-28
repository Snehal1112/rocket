import { useEffect } from 'react';
import { collectAllTabs } from '@/lib/pane-utils';
import { onAgentSessionChunk, onAgentSessionFailed, onAgentSessionFinished } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { isRequestTab } from '@/types/pane-types';

// Finds the request tab that owns sessionId and its currently streaming
// message. Searches the live pane tree first, then every collection snapshot,
// so a reply still lands while its tab is parked after a collection switch.
export function findStreamingTarget(
  sessionId: string,
): { tabId: string; messageId: string } | null {
  const { root, collectionTabState } = usePaneStore.getState();
  const tabs = [
    ...collectAllTabs(root),
    ...Object.values(collectionTabState).flatMap((entry) => entry.tabs),
  ];
  for (const tab of tabs) {
    if (!isRequestTab(tab) || tab.agentSession?.sessionId !== sessionId) continue;
    const message = tab.agentSession.messages.find((m) => m.streaming);
    if (message) return { tabId: tab.id, messageId: message.id };
  }
  return null;
}

// Subscribes once, for the app's lifetime, to the agent session streaming
// events and routes each one into the pane store by session id. This lives
// outside AgentChatPanel so events are never dropped while the panel for the
// owning tab is unmounted or showing a different tab.
export function useAgentSessionEventBridge(): void {
  useEffect(() => {
    const unsubs = Promise.all([
      onAgentSessionChunk((e) => {
        const target = findStreamingTarget(e.session_id);
        if (target) {
          usePaneStore.getState().appendAgentChatChunk(target.tabId, target.messageId, e.text);
        }
      }),
      onAgentSessionFinished((e) => {
        const target = findStreamingTarget(e.session_id);
        if (target) {
          usePaneStore.getState().completeAgentChatMessage(target.tabId, target.messageId);
        }
      }),
      onAgentSessionFailed((e) => {
        const target = findStreamingTarget(e.session_id);
        if (target) {
          usePaneStore.getState().failAgentChatMessage(target.tabId, target.messageId, e.error);
        }
      }),
    ]);

    return () => {
      unsubs.then((fns) => {
        for (const fn of fns) fn();
      });
    };
  }, []);
}
