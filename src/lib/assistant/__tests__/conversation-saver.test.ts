import { toast } from 'sonner';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { type ConversationSummary, saveConversation } from '@/lib/tauri-api';
import { useAssistantStore } from '@/stores/assistant-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import { makeProposal } from '@/test/assistant-fixtures';
import { createDeferred } from '@/test/deferred';
import {
  CONVERSATION_SAVE_DEBOUNCE_MS,
  SAVE_FAILED_NOTICE,
  startConversationSaver,
} from '../conversation-saver';

vi.mock('sonner', () => ({ toast: { warning: vi.fn() } }));
vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  saveConversation: vi.fn(),
}));

const store = () => useAssistantStore.getState();
const save = vi.mocked(saveConversation);
const summary: ConversationSummary = {
  id: 'c1',
  title: 'Why 401?',
  agentConfigId: 'agent-1',
  mode: 'edit',
  createdAt: '2026-10-11T09:00:00Z',
  updatedAt: '2026-10-11T09:00:00Z',
  messageCount: 2,
};

function activate(sessionId = 's1'): void {
  const token = store().beginSession('agent-1', 'edit');
  store().activateSession(token, sessionId, []);
}

function turn(question: string, reply: string, sessionId = 's1'): void {
  store().appendUserMessage(question);
  store().appendChunk(sessionId, reply);
  store().completeMessage(sessionId);
}

describe('startConversationSaver', () => {
  let stop: () => void;

  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
    save.mockResolvedValue(summary);
    store().reset();
    useAssistantStore.setState({ panelOpen: true, focus: undefined });
    useWorkspaceStore.setState({ activeWorkspaceId: 'ws-1' });
    stop = startConversationSaver();
  });

  afterEach(() => {
    stop();
    vi.useRealTimers();
  });

  it('saves 500 ms after a turn finishes', async () => {
    activate();
    turn('Why 401?', 'Hello');
    await vi.advanceTimersByTimeAsync(CONVERSATION_SAVE_DEBOUNCE_MS - 1);
    expect(save).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(save).toHaveBeenCalledTimes(1);
    const [sessionId, input] = save.mock.calls[0];
    expect(sessionId).toBe('s1');
    expect(input).toMatchObject({
      id: store().conversationId,
      agentConfigId: 'agent-1',
      mode: 'edit',
      workspaceId: 'ws-1',
      messages: [
        { kind: 'user', text: 'Why 401?' },
        { kind: 'agent', text: 'Hello' },
      ],
    });
  });

  it('never saves a reply that is still streaming or a chat without a reply', async () => {
    activate();
    store().appendUserMessage('Why 401?');
    store().appendChunk('s1', 'Partial');
    store().closePanel();
    await vi.advanceTimersByTimeAsync(CONVERSATION_SAVE_DEBOUNCE_MS);
    expect(save).not.toHaveBeenCalled();
  });

  it('saves at once when the panel closes, without the streaming segment', async () => {
    activate();
    turn('Why 401?', 'First');
    await vi.advanceTimersByTimeAsync(CONVERSATION_SAVE_DEBOUNCE_MS);
    store().appendUserMessage('Again');
    store().appendChunk('s1', 'Partial');
    store().closePanel();
    await vi.advanceTimersByTimeAsync(0);
    expect(save).toHaveBeenCalledTimes(2);
    expect(save.mock.calls[1][1].messages).toEqual([
      { kind: 'user', text: 'Why 401?' },
      { kind: 'agent', text: 'First' },
      { kind: 'user', text: 'Again' },
    ]);
  });

  it('saves the proposals that ending the session drops', async () => {
    activate();
    turn('Why 401?', 'Hello');
    store().upsertProposal(
      makeProposal({ id: 'p1', sessionId: 's1', summary: 'Add a status test', status: 'accepted' }),
    );
    store().endSession('Session ended.');
    await vi.advanceTimersByTimeAsync(CONVERSATION_SAVE_DEBOUNCE_MS);
    expect(save).toHaveBeenCalledTimes(1);
    expect(save.mock.calls[0][1].messages).toEqual([
      { kind: 'user', text: 'Why 401?' },
      { kind: 'agent', text: 'Hello' },
      { kind: 'proposal', summary: 'Add a status test', status: 'accepted' },
      { kind: 'notice', text: 'Session ended.' },
    ]);
  });

  it('saves one after another, so an older snapshot never lands last', async () => {
    const first = createDeferred<ConversationSummary>();
    save.mockImplementationOnce(() => first.promise);
    activate();
    turn('Why 401?', 'Hello');
    await vi.advanceTimersByTimeAsync(CONVERSATION_SAVE_DEBOUNCE_MS);
    expect(save).toHaveBeenCalledTimes(1);
    store().endSession('Session ended.');
    await vi.advanceTimersByTimeAsync(0);
    expect(save).toHaveBeenCalledTimes(1);
    first.resolve(summary);
    await vi.advanceTimersByTimeAsync(0);
    expect(save).toHaveBeenCalledTimes(2);
    expect(save.mock.calls[1][1].messages[save.mock.calls[1][1].messages.length - 1]).toEqual({
      kind: 'notice',
      text: 'Session ended.',
    });
  });

  it('shows one quiet notice per conversation when saves fail', async () => {
    save.mockRejectedValue(new Error('disk full'));
    activate();
    turn('Why 401?', 'One');
    await vi.advanceTimersByTimeAsync(CONVERSATION_SAVE_DEBOUNCE_MS);
    turn('More', 'Two');
    await vi.advanceTimersByTimeAsync(CONVERSATION_SAVE_DEBOUNCE_MS);
    expect(save).toHaveBeenCalledTimes(2);
    expect(toast.warning).toHaveBeenCalledTimes(1);
    expect(toast.warning).toHaveBeenCalledWith(SAVE_FAILED_NOTICE);
    expect(store().session?.status).toBe('active');
  });

  it('says in plain words that saving is paused when secrets cannot be read', () => {
    expect(SAVE_FAILED_NOTICE).toBe(
      'Could not save this conversation: Rocket could not read the workspace secrets it needs to hide them, for example when the keychain is locked or unavailable. The chat still works.',
    );
  });

  it('never logs the message text or the error details', async () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    save.mockRejectedValue(new Error('secret-token-123'));
    activate();
    turn('my password is hunter2', 'Hello');
    await vi.advanceTimersByTimeAsync(CONVERSATION_SAVE_DEBOUNCE_MS);
    expect(JSON.stringify(spy.mock.calls)).not.toMatch(/hunter2|secret-token-123/);
    spy.mockRestore();
  });

  it('records the workspace the session started in and the focused collections', async () => {
    store().setFocus({ collection: 'orders', path: 'get.yml' });
    activate();
    useWorkspaceStore.setState({ activeWorkspaceId: 'ws-2' });
    turn('Why 401?', 'Hello');
    await vi.advanceTimersByTimeAsync(CONVERSATION_SAVE_DEBOUNCE_MS);
    expect(save.mock.calls[0][1]).toMatchObject({ workspaceId: 'ws-1', collections: ['orders'] });
  });
});
