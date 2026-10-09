import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as api from '@/lib/tauri-api';
import type { AgentSessionStarted } from '@/lib/tauri-api';
import { useAssistantStore } from '@/stores/assistant-store';
import { createDeferred } from '@/test/deferred';
import {
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

  it('starts a session and activates it with the reported options', async () => {
    vi.mocked(api.startWorkspaceAssistant).mockResolvedValue({
      sessionId: 's1',
      configOptions: [
        { id: 'model', name: 'Model', category: 'model', currentValue: 'opus', choices: [] },
      ],
    });
    await startAssistant('agent-1');
    expect(api.startWorkspaceAssistant).toHaveBeenCalledWith('agent-1', 'edit');
    expect(store().session).toMatchObject({ status: 'active', sessionId: 's1', mode: 'edit' });
    expect(store().session?.configOptions[0].currentValue).toBe('opus');
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
    expect(api.sendAgentPrompt).toHaveBeenCalledWith('s1', 'hi');
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
    expect(store().session?.status).toBe('error');
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
