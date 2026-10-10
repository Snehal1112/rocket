import { toast } from 'sonner';
import { buildConversationInput } from '@/lib/assistant/conversation-snapshot';
import { saveConversation } from '@/lib/tauri-api';
import {
  type AssistantState,
  selectTurnRunning,
  useAssistantStore,
} from '@/stores/assistant-store';
import { useWorkspaceStore } from '@/stores/workspace-store';

/** Wait after a turn ends, so a burst of late updates makes one save. */
export const CONVERSATION_SAVE_DEBOUNCE_MS = 500;

/** One quiet notice per conversation, whatever made the save fail. */
export const SAVE_FAILED_NOTICE =
  'Could not save this conversation. The chat still works. This can happen when Rocket cannot read the workspace files or the keychain it needs to hide secrets, or when the disk is full.';

type SaveSource = Pick<
  AssistantState,
  'session' | 'conversationId' | 'replay' | 'messages' | 'proposals' | 'proposalAnchors'
>;

/** What the saver keeps per session: where it started and what it focused on. */
interface TrackedSession {
  sessionId: string;
  workspaceId: string;
  collections: Set<string>;
}

/**
 * Saves the assistant conversation 500 ms after a turn ends, and at once
 * when the session ends or the panel closes. Saves run one after another,
 * so an older snapshot never lands after a newer one. A failed save shows
 * one quiet notice per conversation and never touches the chat. The backend
 * picks the workspace from the session id. Returns a stop function.
 */
export function startConversationSaver(): () => void {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let queue: Promise<void> = Promise.resolve();
  const warned = new Set<string>();
  let tracked: TrackedSession | undefined;

  const cancelTimer = () => {
    if (timer !== undefined) {
      clearTimeout(timer);
      timer = undefined;
    }
  };

  const save = (source: SaveSource) => {
    const { session, conversationId } = source;
    if (!session?.sessionId || !conversationId || tracked?.sessionId !== session.sessionId) return;
    const input = buildConversationInput({
      conversationId,
      continuedFrom: source.replay?.fromId,
      agentConfigId: session.agentConfigId,
      mode: session.mode,
      workspaceId: tracked.workspaceId,
      collections: [...tracked.collections],
      messages: source.messages,
      proposals: source.proposals,
      proposalAnchors: source.proposalAnchors,
    });
    if (!input) return;
    const sessionId = session.sessionId;
    queue = queue.then(async () => {
      try {
        await saveConversation(sessionId, input);
      } catch {
        // The error is not logged, because it could carry message text.
        if (!warned.has(conversationId)) {
          warned.add(conversationId);
          toast.warning(SAVE_FAILED_NOTICE);
        }
      }
    });
  };

  const unsubscribe = useAssistantStore.subscribe((state, prev) => {
    const session = state.session;
    if (session?.status === 'active' && tracked?.sessionId !== session.sessionId) {
      tracked = {
        sessionId: session.sessionId,
        workspaceId: useWorkspaceStore.getState().activeWorkspaceId,
        collections: new Set(),
      };
    }
    if (tracked && session?.sessionId === tracked.sessionId && state.focus?.collection) {
      tracked.collections.add(state.focus.collection);
    }

    const wasActive = prev.session?.status === 'active';
    const sameSession = session?.sessionId === prev.session?.sessionId;
    if (wasActive && !(session?.status === 'active' && sameSession)) {
      // The session ended. endSession and a fatal failure drop the proposals
      // in this same update, so they come from the previous state.
      cancelTimer();
      save(
        sameSession
          ? { ...state, proposals: prev.proposals, proposalAnchors: prev.proposalAnchors }
          : prev,
      );
      return;
    }
    if (prev.panelOpen && !state.panelOpen && session?.status === 'active') {
      cancelTimer();
      save(state);
      return;
    }
    if (session?.status === 'active' && selectTurnRunning(prev) && !selectTurnRunning(state)) {
      cancelTimer();
      timer = setTimeout(() => {
        timer = undefined;
        save(useAssistantStore.getState());
      }, CONVERSATION_SAVE_DEBOUNCE_MS);
    }
  });

  return () => {
    cancelTimer();
    unsubscribe();
  };
}
