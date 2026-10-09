import { useEffect } from 'react';
import {
  endAssistantSession,
  sweepStaleAssistantSessions,
  WORKSPACE_SWITCH_NOTICE,
} from '@/lib/assistant/assistant-session';
import { clearPromptHistory } from '@/lib/assistant/prompt-history';
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
    let disposed = false;
    // Handlers ignore events once the effect is torn down, so an overlapping
    // mount (StrictMode, HMR) never applies an event twice.
    const guard =
      <T>(handler: (event: T) => void) =>
      (event: T) => {
        if (!disposed) handler(event);
      };

    const registrations = [
      onAgentSessionChunk(guard((e) => store().appendChunk(e.session_id, e.text))),
      onAgentSessionFinished(guard((e) => store().completeMessage(e.session_id))),
      onAgentSessionFailed(guard((e) => store().failMessage(e.session_id, e.error))),
      onAgentToolActivity(
        guard((e) =>
          store().upsertToolActivity(e.session_id, {
            callId: e.call_id,
            title: e.title,
            status: e.status,
          }),
        ),
      ),
      // Event options are snake_case. Plan 01's helper converts them.
      onAgentConfigOptions(
        guard((e) => store().setConfigOptions(e.session_id, configOptionsFromEvent(e.options))),
      ),
      onAgentUsage(
        guard((e) =>
          store().setUsage(e.session_id, {
            used: e.used,
            size: e.size,
            costUsd: e.cost_usd ?? undefined,
          }),
        ),
      ),
      onAgentProposalCreated(
        guard((e) => {
          void refreshProposals(e.session_id);
        }),
      ),
      onAgentProposalResolved(
        guard((e) => store().resolveProposal(e.session_id, e.proposal_id, e.status)),
      ),
    ];

    // A listener that resolves after disposal is unlistened at once.
    const settled = Promise.allSettled(registrations).then((results) => {
      const fns: Array<() => void> = [];
      for (const result of results) {
        if (result.status === 'fulfilled') fns.push(result.value);
        else console.error('[assistant] failed to register a listener', result.reason);
      }
      if (disposed) for (const fn of fns) fn();
      return fns;
    });

    // The first id is set during startup. Only a change from one workspace
    // to another ends the session.
    const unsubWorkspace = useWorkspaceStore.subscribe((state, prev) => {
      if (!prev.activeWorkspaceId || state.activeWorkspaceId === prev.activeWorkspaceId) return;
      store().setFocus(undefined);
      // Prompts typed in the old workspace must not outlive the switch.
      clearPromptHistory(prev.activeWorkspaceId);
      void endAssistantSession(WORKSPACE_SWITCH_NOTICE);
    });

    return () => {
      disposed = true;
      unsubWorkspace();
      void settled.then((fns) => {
        for (const fn of fns) fn();
      });
    };
  }, []);
}
