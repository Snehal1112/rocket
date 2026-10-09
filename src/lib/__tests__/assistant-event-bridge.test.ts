import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  resetStaleSweepForTests,
  WORKSPACE_SWITCH_NOTICE,
} from '@/lib/assistant/assistant-session';
import { loadPromptHistory, savePromptHistory } from '@/lib/assistant/prompt-history';
import type { AgentProposal } from '@/lib/tauri-api';
import { useAssistantStore } from '@/stores/assistant-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import { makeProposal } from '@/test/assistant-fixtures';
import { createDeferred } from '@/test/deferred';
import { useAssistantEventBridge } from '../assistant-event-bridge';

const mocks = vi.hoisted(() => {
  const handlers: Record<string, (payload: unknown) => void> = {};
  const listener = (name: string) =>
    vi.fn((handler: (payload: unknown) => void) => {
      handlers[name] = handler;
      return Promise.resolve(() => undefined);
    });
  return {
    handlers,
    api: {
      onAgentSessionChunk: listener('chunk'),
      onAgentSessionFinished: listener('finished'),
      onAgentSessionFailed: listener('failed'),
      onAgentToolActivity: listener('toolActivity'),
      onAgentConfigOptions: listener('configOptions'),
      onAgentUsage: listener('usage'),
      onAgentProposalCreated: listener('proposalCreated'),
      onAgentProposalResolved: listener('proposalResolved'),
      listAgentProposals: vi.fn(),
      endAgentSession: vi.fn(),
      endStaleAssistantSessions: vi.fn(),
    },
  };
});

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  ...mocks.api,
}));

const store = () => useAssistantStore.getState();

function activate(sessionId = 's1'): void {
  const token = store().beginSession('agent-1', 'edit');
  store().activateSession(token, sessionId, []);
}

async function mountBridge(): Promise<void> {
  renderHook(() => useAssistantEventBridge());
  await waitFor(() => expect(mocks.handlers.proposalResolved).toBeDefined());
}

function emit(name: string, payload: unknown): void {
  const handler = mocks.handlers[name];
  if (!handler) throw new Error(`no ${name} listener`);
  handler(payload);
}

function lastMessage() {
  const { messages } = store();
  return messages[messages.length - 1];
}

describe('useAssistantEventBridge', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    for (const key of Object.keys(mocks.handlers)) delete mocks.handlers[key];
    resetStaleSweepForTests();
    store().reset();
    useAssistantStore.setState({ panelOpen: false, focus: undefined });
    useWorkspaceStore.setState({ activeWorkspaceId: '' });
    mocks.api.endStaleAssistantSessions.mockResolvedValue(0);
    mocks.api.endAgentSession.mockResolvedValue(undefined);
  });

  it('streams chunks and finishes the turn of the active session', async () => {
    activate();
    store().appendUserMessage('hi');
    await mountBridge();
    emit('chunk', { session_id: 's1', text: 'Hello' });
    emit('finished', { session_id: 's1', stop_reason: 'end_turn' });
    expect(lastMessage()).toMatchObject({ kind: 'agent', text: 'Hello', streaming: false });
  });

  it('does not apply events after the bridge is unmounted', async () => {
    activate();
    store().appendUserMessage('hi');
    const view = renderHook(() => useAssistantEventBridge());
    await waitFor(() => expect(mocks.handlers.chunk).toBeDefined());
    view.unmount();
    emit('chunk', { session_id: 's1', text: 'late' });
    expect(lastMessage()).toMatchObject({ kind: 'agent', text: '' });
  });

  it('ignores events for another session', async () => {
    activate();
    store().appendUserMessage('hi');
    await mountBridge();
    emit('chunk', { session_id: 'other', text: 'Hello' });
    emit('failed', { session_id: 'other', error: 'boom' });
    expect(lastMessage()).toMatchObject({ kind: 'agent', text: '', streaming: true });
    expect(store().session?.status).toBe('active');
  });

  it('fails the turn on a failed event', async () => {
    activate();
    store().appendUserMessage('hi');
    await mountBridge();
    emit('failed', { session_id: 's1', error: 'idle timeout' });
    expect(store().session).toMatchObject({ status: 'error', error: 'idle timeout' });
  });

  it('adds and updates a tool activity line', async () => {
    activate();
    store().appendUserMessage('hi');
    await mountBridge();
    emit('toolActivity', {
      session_id: 's1',
      call_id: 'c1',
      title: 'Running Login',
      status: 'in_progress',
    });
    emit('toolActivity', { session_id: 's1', call_id: 'c1', title: '', status: 'completed' });
    const tools = store().messages.filter((m) => m.kind === 'tool');
    expect(tools).toEqual([
      expect.objectContaining({ callId: 'c1', title: 'Running Login', status: 'completed' }),
    ]);
  });

  it('converts the snake_case config options of the event', async () => {
    activate();
    await mountBridge();
    emit('configOptions', {
      session_id: 's1',
      options: [
        {
          id: 'model',
          name: 'Model',
          category: 'model',
          current_value: 'opus',
          choices: [{ value: 'opus', name: 'Opus', description: null }],
        },
        { id: 'effort', name: 'Effort', category: 'thought_level', current_value: 'high', choices: [] },
      ],
    });
    expect(store().session?.configOptions.map((o) => o.currentValue)).toEqual(['opus', 'high']);
  });

  it('stores usage', async () => {
    activate();
    await mountBridge();
    emit('usage', { session_id: 's1', used: 1200, size: 200000, cost_usd: null });
    expect(store().usage).toEqual({ used: 1200, size: 200000, costUsd: undefined });
  });

  it('loads the session proposals when one is created', async () => {
    activate();
    mocks.api.listAgentProposals.mockResolvedValue([makeProposal()]);
    await mountBridge();
    emit('proposalCreated', { session_id: 's1', proposal_id: 'p1', summary: 'Add a status test' });
    await waitFor(() => expect(store().proposals).toHaveLength(1));
    expect(mocks.api.listAgentProposals).toHaveBeenCalledWith('s1');
  });

  it('drops proposals that arrive after the session ended', async () => {
    activate();
    const pending = createDeferred<AgentProposal[]>();
    mocks.api.listAgentProposals.mockReturnValue(pending.promise);
    await mountBridge();
    emit('proposalCreated', { session_id: 's1', proposal_id: 'p1', summary: 'Add a status test' });
    store().endSession();
    pending.resolve([makeProposal()]);
    await pending.promise;
    await Promise.resolve();
    expect(store().proposals).toEqual([]);
  });

  it('applies a resolved status', async () => {
    activate();
    store().upsertProposal(makeProposal());
    await mountBridge();
    emit('proposalResolved', { session_id: 's1', proposal_id: 'p1', status: 'stale' });
    expect(store().proposals[0].status).toBe('stale');
  });

  it('ends the session and clears the focus when the workspace changes', async () => {
    useWorkspaceStore.setState({ activeWorkspaceId: 'ws-1' });
    activate();
    store().setFocus({ collection: 'orders', path: 'get.yml' });
    await mountBridge();
    useWorkspaceStore.getState().setActiveWorkspaceId('ws-2');
    await waitFor(() => expect(mocks.api.endAgentSession).toHaveBeenCalledWith('s1'));
    expect(store().session?.status).toBe('ended');
    expect(store().focus).toBeUndefined();
    expect(lastMessage()).toMatchObject({ kind: 'notice', text: WORKSPACE_SWITCH_NOTICE });
  });

  it('forgets the prompt history of the workspace it leaves', async () => {
    useWorkspaceStore.setState({ activeWorkspaceId: 'ws-1' });
    savePromptHistory('ws-1', ['old prompt']);
    await mountBridge();
    useWorkspaceStore.getState().setActiveWorkspaceId('ws-2');
    expect(loadPromptHistory('ws-1')).toEqual([]);
  });

  it('keeps the session when the first workspace id is set at startup', async () => {
    activate();
    await mountBridge();
    useWorkspaceStore.getState().setActiveWorkspaceId('ws-1');
    expect(store().session?.status).toBe('active');
    expect(mocks.api.endAgentSession).not.toHaveBeenCalled();
  });

  it('sweeps stale backend sessions once per webview load', async () => {
    const first = renderHook(() => useAssistantEventBridge());
    first.unmount();
    renderHook(() => useAssistantEventBridge());
    await waitFor(() => expect(mocks.api.endStaleAssistantSessions).toHaveBeenCalledTimes(1));
  });
});
