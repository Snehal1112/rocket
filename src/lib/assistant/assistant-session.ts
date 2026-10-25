import { loadRememberedEffort, loadRememberedModel } from '@/lib/assistant/model-memory';
import { buildReplayPreface, UNLIMITED_REPLAY_LIMITS } from '@/lib/assistant/replay-preface';
import { storedToChatMessages } from '@/lib/assistant/stored-messages';
import {
  type ConversationRecord,
  cancelAgentPrompt,
  endAgentSession,
  endStaleAssistantSessions,
  type PromptResourceDto,
  sendAgentPrompt,
  startWorkspaceAssistant,
} from '@/lib/tauri-api';
import {
  type AssistantMessage,
  type AssistantMode,
  type ReplaySeed,
  selectTurnRunning,
  useAssistantStore,
} from '@/stores/assistant-store';
import { useWorkspaceStore } from '@/stores/workspace-store';

/** Mode for a new session. The composer's mode picker changes it afterwards. */
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
  replay?: ReplaySeed,
  seedMessages?: AssistantMessage[],
): Promise<void> {
  // A session that failed fatally may still have a live agent and MCP server
  // behind it. Ending it before the new start keeps either from leaking.
  const previous = useAssistantStore.getState().session;
  if (previous?.status === 'error' && previous.sessionId) {
    endInBackground(previous.sessionId, 'the failed session');
  }
  const token = useAssistantStore
    .getState()
    .beginSession(agentConfigId, mode, replay, seedMessages);
  try {
    // A sweep still in flight would end the session started below.
    await sweepStaleAssistantSessions();
    const started = await startWorkspaceAssistant(
      agentConfigId,
      mode,
      loadRememberedModel(agentConfigId),
      loadRememberedEffort(agentConfigId),
    );
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

export async function sendAssistantMessage(
  text: string,
  resources?: PromptResourceDto[],
): Promise<void> {
  const trimmed = text.trim();
  const store = useAssistantStore.getState();
  const session = store.session;
  if (!trimmed || session?.status !== 'active') return;
  // A continued session sends its preface once, with the first message that goes out.
  const replay = store.replay && !store.replay.sent ? store.replay : undefined;
  // The store refuses a second turn while one runs, so a double send stops here.
  // The chat, and so the saved conversation, keeps only what the user typed.
  if (!store.appendUserMessage(trimmed)) return;
  const prompt = replay ? `${replay.preface}\n\n${trimmed}` : trimmed;
  try {
    await sendAgentPrompt(session.sessionId, prompt, resources);
    if (replay) useAssistantStore.getState().markReplaySent(session.sessionId);
  } catch (err) {
    // A rejected prompt fails the turn only. A dead session arrives as its own event.
    useAssistantStore.getState().failMessage(session.sessionId, String(err), false);
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

export const CONTINUE_WORKSPACE_MISMATCH =
  'This conversation belongs to another workspace. Switch back to that workspace to continue it.';

/** True when the stored conversation belongs to the active workspace. */
export function canContinueHere(record: ConversationRecord): boolean {
  return record.workspaceId === useWorkspaceStore.getState().activeWorkspaceId;
}

/** A failed Continue. `started` tells whether the new session was begun. */
export class ContinueError extends Error {
  readonly started: boolean;
  constructor(message: string, started: boolean) {
    super(message);
    this.name = 'ContinueError';
    this.started = started;
  }
}

/**
 * Starts a new session that carries a stored conversation as context. It
 * always starts in Ask mode, so a stored Agent mode is never re-applied. A
 * live session ends first, which saves it. A conversation of another
 * workspace is refused, so its transcript never reaches this workspace.
 * A throw is a `ContinueError` that says whether the start began.
 */
export async function continueConversation(
  record: ConversationRecord,
  agentConfigId: string,
): Promise<void> {
  let started = false;
  try {
    if (!canContinueHere(record)) {
      throw new Error(CONTINUE_WORKSPACE_MISMATCH);
    }
    const current = useAssistantStore.getState().session;
    if (current?.status === 'active' || current?.status === 'starting') {
      await endAssistantSession();
    }
    // The workspace may have changed while the live session ended.
    if (!canContinueHere(record)) {
      throw new Error(CONTINUE_WORKSPACE_MISMATCH);
    }
    // The conversation carries on in place: its messages stay in the chat, it is saved
    // over itself, and the agent gets all of it, with nothing cut.
    const seed = {
      fromId: record.id,
      preface: buildReplayPreface(record, UNLIMITED_REPLAY_LIMITS),
      resume: { continuedFrom: record.continuedFrom },
    };
    started = true;
    await startAssistant(agentConfigId, 'ask', seed, storedToChatMessages(record.messages));
  } catch (err) {
    throw new ContinueError(err instanceof Error ? err.message : String(err), started);
  }
}
