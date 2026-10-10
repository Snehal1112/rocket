import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as api from '@/lib/tauri-api';
import type { AgentSessionStarted, ConversationRecord } from '@/lib/tauri-api';
import { useAssistantStore } from '@/stores/assistant-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import { createDeferred } from '@/test/deferred';
import { REPLAY_PREFACE_HEADER } from '../replay-preface';
import {
  CONTINUE_WORKSPACE_MISMATCH,
  ContinueError,
  continueConversation,
  endAssistantSession,
  resetStaleSweepForTests,
  sendAssistantMessage,
  startAssistant,
  stopAssistantTurn,
} from '../assistant-session';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  startWorkspaceAssistant: vi.fn(),
  sendAgentPrompt: vi.fn(),
  cancelAgentPrompt: vi.fn(),
  endAgentSession: vi.fn(),
  endStaleAssistantSessions: vi.fn(),
}));

const store = () => useAssistantStore.getState();

function activate(sessionId = 's1'): void {
  const token = store().beginSession('agent-1', 'edit');
  store().activateSession(token, sessionId, []);
}

describe('assistant session flows', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetStaleSweepForTests();
    store().reset();
    vi.mocked(api.endStaleAssistantSessions).mockResolvedValue(0);
    vi.mocked(api.endAgentSession).mockResolvedValue(undefined);
    vi.mocked(api.cancelAgentPrompt).mockResolvedValue(undefined);
  });

  const stored: ConversationRecord = {
    id: '0b6f3f4e-0000-4000-8000-000000000001',
    title: 'Why 401?',
    agentConfigId: 'agent-1',
    mode: 'agent',
    workspaceId: 'ws-1',
    createdAt: '2026-10-11T09:12:00Z',
    updatedAt: '2026-10-11T09:20:41Z',
    collections: [],
    messages: [
      { kind: 'user', text: 'Why 401?' },
      { kind: 'agent', text: 'The token expired.' },
    ],
  };

  it('continues in Ask mode and sends the preface with the first message only', async () => {
    useWorkspaceStore.setState({ activeWorkspaceId: 'ws-1' });
    vi.mocked(api.startWorkspaceAssistant).mockResolvedValue({ sessionId: 's2', configOptions: [] });
    vi.mocked(api.sendAgentPrompt).mockResolvedValue('end_turn');
    activate('s1');
    await continueConversation(stored, 'agent-1');
    expect(api.endAgentSession).toHaveBeenCalledWith('s1');
    expect(api.startWorkspaceAssistant).toHaveBeenCalledWith('agent-1', 'ask', undefined);
    expect(store().session).toMatchObject({ sessionId: 's2', status: 'active', mode: 'ask' });
    expect(store().replay).toMatchObject({ fromId: stored.id, sent: false });

    await sendAssistantMessage('  And now?  ');
    const first = vi.mocked(api.sendAgentPrompt).mock.calls[0][1];
    expect(first.startsWith(REPLAY_PREFACE_HEADER)).toBe(true);
    expect(first).toContain('Assistant: The token expired.');
    expect(first.endsWith('\n\nAnd now?')).toBe(true);
    expect(store().messages[0]).toMatchObject({ kind: 'user', text: 'And now?' });
    expect(store().replay?.sent).toBe(true);

    store().completeMessage('s2');
    await sendAssistantMessage('Next');
    expect(vi.mocked(api.sendAgentPrompt).mock.calls[1][1]).toBe('Next');
  });

  it('keeps the preface when the first send fails', async () => {
    useWorkspaceStore.setState({ activeWorkspaceId: 'ws-1' });
    vi.mocked(api.startWorkspaceAssistant).mockResolvedValue({ sessionId: 's2', configOptions: [] });
    await continueConversation(stored, 'agent-1');
    vi.mocked(api.sendAgentPrompt).mockRejectedValueOnce('agent busy');
    await sendAssistantMessage('First try');
    expect(store().replay?.sent).toBe(false);
    vi.mocked(api.sendAgentPrompt).mockResolvedValueOnce('end_turn');
    await sendAssistantMessage('Second try');
    expect(
      vi.mocked(api.sendAgentPrompt).mock.calls[1][1].startsWith(REPLAY_PREFACE_HEADER),
    ).toBe(true);
  });

  it('refuses when the workspace changes while the live session ends', async () => {
    useWorkspaceStore.setState({ activeWorkspaceId: 'ws-1' });
    activate('s1');
    vi.mocked(api.endAgentSession).mockImplementationOnce(async () => {
      useWorkspaceStore.setState({ activeWorkspaceId: 'ws-2' });
    });
    await expect(continueConversation(stored, 'agent-1')).rejects.toThrow(
      CONTINUE_WORKSPACE_MISMATCH,
    );
    expect(api.startWorkspaceAssistant).not.toHaveBeenCalled();
  });

  it('reports started false for a pre-start throw after an earlier Continue of the same record', async () => {
    useWorkspaceStore.setState({ activeWorkspaceId: 'ws-1' });
    vi.mocked(api.startWorkspaceAssistant).mockResolvedValue({ sessionId: 's2', configOptions: [] });
    await continueConversation(stored, 'agent-1');
    await endAssistantSession();
    expect(store().replay?.fromId).toBe(stored.id);
    useWorkspaceStore.setState({ activeWorkspaceId: 'ws-2' });
    const err = await continueConversation(stored, 'agent-1').catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ContinueError);
    expect((err as ContinueError).started).toBe(false);
    expect((err as ContinueError).message).toBe(CONTINUE_WORKSPACE_MISMATCH);
  });

  it('refuses to continue a conversation of another workspace', async () => {
    useWorkspaceStore.setState({ activeWorkspaceId: 'ws-2' });
    await expect(continueConversation(stored, 'agent-1')).rejects.toThrow(
      CONTINUE_WORKSPACE_MISMATCH,
    );
    expect(api.startWorkspaceAssistant).not.toHaveBeenCalled();
    expect(store().session).toBeUndefined();
  });

  it('starts a session and activates it with the reported options', async () => {
    vi.mocked(api.startWorkspaceAssistant).mockResolvedValue({
      sessionId: 's1',
      configOptions: [
        { id: 'model', name: 'Model', category: 'model', currentValue: 'opus', choices: [] },
      ],
    });
    await startAssistant('agent-1');
    expect(api.startWorkspaceAssistant).toHaveBeenCalledWith('agent-1', 'edit', undefined);
    expect(store().session).toMatchObject({ status: 'active', sessionId: 's1', mode: 'edit' });
    expect(store().session?.configOptions[0].currentValue).toBe('opus');
  });

  it('keeps a focus set just before the start and clears it when the session ends', async () => {
    vi.mocked(api.startWorkspaceAssistant).mockResolvedValue({ sessionId: 's1', configOptions: [] });
    store().setFocus({ collection: 'orders', path: 'get.yml' });
    await startAssistant('agent-1');
    expect(store().focus).toEqual({ collection: 'orders', path: 'get.yml' });
    await endAssistantSession();
    expect(store().focus).toBeUndefined();
  });

  it('ends the backend session of a failed session before starting a new one', async () => {
    activate('s-old');
    store().appendUserMessage('hi');
    store().failMessage('s-old', 'agent crashed');
    vi.mocked(api.startWorkspaceAssistant).mockResolvedValue({ sessionId: 's2', configOptions: [] });
    await startAssistant('agent-1');
    expect(api.endAgentSession).toHaveBeenCalledWith('s-old');
    expect(store().session).toMatchObject({ status: 'active', sessionId: 's2' });
  });

  it('does not end anything when the previous start never got a session id', async () => {
    const token = store().beginSession('agent-1', 'edit');
    store().failStart(token, 'no credential');
    vi.mocked(api.startWorkspaceAssistant).mockResolvedValue({ sessionId: 's2', configOptions: [] });
    await startAssistant('agent-1');
    expect(api.endAgentSession).not.toHaveBeenCalled();
  });

  it('waits for the stale-session sweep before starting', async () => {
    const sweep = createDeferred<number>();
    vi.mocked(api.endStaleAssistantSessions).mockReturnValue(sweep.promise);
    vi.mocked(api.startWorkspaceAssistant).mockResolvedValue({ sessionId: 's1', configOptions: [] });
    const started = startAssistant('agent-1');
    await Promise.resolve();
    await Promise.resolve();
    expect(api.startWorkspaceAssistant).not.toHaveBeenCalled();
    sweep.resolve(2);
    await started;
    expect(api.startWorkspaceAssistant).toHaveBeenCalledTimes(1);
    expect(store().session?.status).toBe('active');
  });

  it('still starts when the sweep fails', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    vi.mocked(api.endStaleAssistantSessions).mockRejectedValue(new Error('no backend'));
    vi.mocked(api.startWorkspaceAssistant).mockResolvedValue({ sessionId: 's1', configOptions: [] });
    await startAssistant('agent-1');
    expect(store().session?.status).toBe('active');
  });

  it('ends the backend session when the start was abandoned', async () => {
    const start = createDeferred<AgentSessionStarted>();
    vi.mocked(api.startWorkspaceAssistant).mockReturnValue(start.promise);
    const started = startAssistant('agent-1');
    await endAssistantSession();
    expect(api.endAgentSession).not.toHaveBeenCalled();
    start.resolve({ sessionId: 'orphan', configOptions: [] });
    await started;
    expect(api.endAgentSession).toHaveBeenCalledWith('orphan');
    expect(store().session?.status).toBe('ended');
  });

  it('shows the error when the start fails', async () => {
    vi.mocked(api.startWorkspaceAssistant).mockRejectedValue('agent not found');
    await startAssistant('agent-1');
    expect(store().session).toMatchObject({ status: 'error', error: 'agent not found' });
  });

  it('sends one prompt per turn', async () => {
    activate();
    vi.mocked(api.sendAgentPrompt).mockReturnValue(new Promise<string>(() => undefined));
    void sendAssistantMessage('  hi  ');
    void sendAssistantMessage('again');
    await Promise.resolve();
    expect(api.sendAgentPrompt).toHaveBeenCalledTimes(1);
    expect(api.sendAgentPrompt).toHaveBeenCalledWith('s1', 'hi', undefined);
  });

  it('ignores an empty message', async () => {
    activate();
    await sendAssistantMessage('   ');
    expect(api.sendAgentPrompt).not.toHaveBeenCalled();
    expect(store().messages).toEqual([]);
  });

  it('fails the reply when the prompt call throws', async () => {
    activate();
    vi.mocked(api.sendAgentPrompt).mockRejectedValue('agent exited');
    await sendAssistantMessage('hi');
    const { messages } = store();
    expect(messages[messages.length - 1]).toMatchObject({
      kind: 'agent',
      streaming: false,
      error: 'agent exited',
    });
    expect(store().session?.status).toBe('active');
  });

  it('accepts a new message after a rejected prompt', async () => {
    activate();
    vi.mocked(api.sendAgentPrompt).mockRejectedValueOnce('agent busy');
    await sendAssistantMessage('one');
    vi.mocked(api.sendAgentPrompt).mockResolvedValueOnce('end_turn');
    await sendAssistantMessage('two');
    expect(api.sendAgentPrompt).toHaveBeenCalledTimes(2);
  });

  it('stops a running turn', async () => {
    activate();
    store().appendUserMessage('hi');
    await stopAssistantTurn();
    expect(api.cancelAgentPrompt).toHaveBeenCalledWith('s1');
  });

  it('does not stop when no turn runs', async () => {
    activate();
    await stopAssistantTurn();
    expect(api.cancelAgentPrompt).not.toHaveBeenCalled();
  });

  it('ends an active session', async () => {
    activate();
    await endAssistantSession('Bye.');
    expect(api.endAgentSession).toHaveBeenCalledWith('s1');
    expect(store().session?.status).toBe('ended');
  });
});
