import { useEffect } from 'react';
import {
  endAssistantSession,
  sweepStaleAssistantSessions,
  WORKSPACE_SWITCH_NOTICE,
} from '@/lib/assistant/assistant-session';
import {
  configOptionsFromEvent,
  listAgentProposals,
  onAgentConfigOptions,
  onAgentProposalCreated,
  onAgentProposalResolved,
  onAgentSessionChunk,
  onAgentSessionFailed,
  onAgentSessionFinished,
  onAgentToolActivity,
  onAgentUsage,
} from '@/lib/tauri-api';
import { useAssistantStore } from '@/stores/assistant-store';
import { useWorkspaceStore } from '@/stores/workspace-store';

// The created event names one proposal. The list carries the full DTOs.
async function refreshProposals(sessionId: string): Promise<void> {
  if (useAssistantStore.getState().session?.sessionId !== sessionId) return;
  try {
    const proposals = await listAgentProposals(sessionId);
    // upsertProposal drops them when the session ended in the meantime.
    for (const proposal of proposals) useAssistantStore.getState().upsertProposal(proposal);
  } catch (err) {
    console.error('[assistant] failed to load proposals', err);
  }
}

/**
 * Subscribes once, for the app's lifetime, to every assistant event and
 * routes it into assistant-store by session id. It also ends the session when
 * the active workspace changes, and sweeps sessions left by an earlier load.
 */
export function useAssistantEventBridge(): void {
  useEffect(() => {
    void sweepStaleAssistantSessions();
    const store = () => useAssistantStore.getState();

    const unsubs = Promise.all([
      onAgentSessionChunk((e) => store().appendChunk(e.session_id, e.text)),
      onAgentSessionFinished((e) => store().completeMessage(e.session_id)),
      onAgentSessionFailed((e) => store().failMessage(e.session_id, e.error)),
      onAgentToolActivity((e) =>
        store().upsertToolActivity(e.session_id, {
          callId: e.call_id,
          title: e.title,
          status: e.status,
        }),
      ),
      // Event options are snake_case. Plan 01's helper converts them.
      onAgentConfigOptions((e) =>
        store().setConfigOptions(e.session_id, configOptionsFromEvent(e.options)),
      ),
      onAgentUsage((e) =>
        store().setUsage(e.session_id, {
          used: e.used,
          size: e.size,
          costUsd: e.cost_usd ?? undefined,
        }),
      ),
      onAgentProposalCreated((e) => {
        void refreshProposals(e.session_id);
      }),
      onAgentProposalResolved((e) =>
        store().resolveProposal(e.session_id, e.proposal_id, e.status),
      ),
    ]);

    // The first id is set during startup. Only a change from one workspace
    // to another ends the session.
    const unsubWorkspace = useWorkspaceStore.subscribe((state, prev) => {
      if (!prev.activeWorkspaceId || state.activeWorkspaceId === prev.activeWorkspaceId) return;
      store().setFocus(undefined);
      void endAssistantSession(WORKSPACE_SWITCH_NOTICE);
    });

    return () => {
      unsubWorkspace();
      void unsubs.then((fns) => {
        for (const fn of fns) fn();
      });
    };
  }, []);
}
