import {
  cancelAgentPrompt,
  endAgentSession,
  endStaleAssistantSessions,
  sendAgentPrompt,
  startWorkspaceAssistant,
} from '@/lib/tauri-api';
import { type AssistantMode, selectTurnRunning, useAssistantStore } from '@/stores/assistant-store';

/** Mode for a new session until the composer (Plan 06) adds the picker. */
export const DEFAULT_ASSISTANT_MODE: AssistantMode = 'edit';

export const WORKSPACE_SWITCH_NOTICE =
  'The workspace changed, so the assistant session ended and its pending proposals were discarded.';

let staleSweep: Promise<void> | null = null;

/**
 * Ends backend assistant sessions left over from an earlier webview load.
 * Runs once per load, before any new session starts.
 */
export function sweepStaleAssistantSessions(): Promise<void> {
  if (!staleSweep) {
    staleSweep = Promise.resolve()
      .then(async () => {
        await endStaleAssistantSessions();
      })
      .catch((err) => {
        console.error('[assistant] stale session sweep failed', err);
      });
  }
  return staleSweep;
}

/** Lets a test run the once-per-load sweep again. */
export function resetStaleSweepForTests(): void {
  staleSweep = null;
}

function endInBackground(sessionId: string, what: string): void {
  Promise.resolve(endAgentSession(sessionId)).catch((err) => {
    console.error(`[assistant] failed to end ${what}`, err);
  });
}

export async function startAssistant(
  agentConfigId: string,
  mode: AssistantMode = DEFAULT_ASSISTANT_MODE,
): Promise<void> {
  const token = useAssistantStore.getState().beginSession(agentConfigId, mode);
  try {
    // A sweep still in flight would end the session started below.
    await sweepStaleAssistantSessions();
    const started = await startWorkspaceAssistant(agentConfigId, mode);
    const applied = useAssistantStore
      .getState()
      .activateSession(token, started.sessionId, started.configOptions);
    // The start was abandoned (End session, a workspace switch or a newer
    // start), so nothing else would ever end this agent process.
    if (!applied) endInBackground(started.sessionId, 'an abandoned session');
  } catch (err) {
    useAssistantStore.getState().failStart(token, String(err));
  }
}

export async function sendAssistantMessage(text: string): Promise<void> {
  const trimmed = text.trim();
  const store = useAssistantStore.getState();
  const session = store.session;
  if (!trimmed || session?.status !== 'active') return;
  // The store refuses a second turn while one runs, so a double send stops here.
  if (!store.appendUserMessage(trimmed)) return;
  try {
    await sendAgentPrompt(session.sessionId, trimmed);
  } catch (err) {
    useAssistantStore.getState().failMessage(session.sessionId, String(err));
  }
}

export async function stopAssistantTurn(): Promise<void> {
  const state = useAssistantStore.getState();
  const session = state.session;
  if (session?.status !== 'active' || !selectTurnRunning(state)) return;
  try {
    // The turn then finishes with a 'cancelled' stop reason, through the bridge.
    await cancelAgentPrompt(session.sessionId);
  } catch (err) {
    console.error('[assistant] failed to stop the turn', err);
  }
}

export async function endAssistantSession(notice?: string): Promise<void> {
  const session = useAssistantStore.getState().session;
  if (!session || session.status === 'ended') return;
  useAssistantStore.getState().endSession(notice);
  // A session still starting has no id yet. startAssistant ends it when the start resolves.
  if (!session.sessionId) return;
  try {
    await endAgentSession(session.sessionId);
  } catch (err) {
    console.error('[assistant] failed to end the session', err);
  }
}
