import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

describe('ACP chat session tauri-api bindings', () => {
  it('startAgentSession invokes start_agent_session with camelCase args', async () => {
    vi.mocked(invoke).mockResolvedValue('session-1');
    const { startAgentSession } = await import('@/lib/tauri-api');
    const result = await startAgentSession('agent-1', '/collections/my-collection');
    expect(invoke).toHaveBeenCalledWith('start_agent_session', {
      agentConfigId: 'agent-1',
      cwd: '/collections/my-collection',
    });
    expect(result).toBe('session-1');
  });

  it('sendAgentPrompt invokes send_agent_prompt with camelCase args', async () => {
    vi.mocked(invoke).mockResolvedValue('end_turn');
    const { sendAgentPrompt } = await import('@/lib/tauri-api');
    const result = await sendAgentPrompt('session-1', 'hello');
    expect(invoke).toHaveBeenCalledWith('send_agent_prompt', {
      sessionId: 'session-1',
      prompt: 'hello',
    });
    expect(result).toBe('end_turn');
  });

  it('endAgentSession invokes end_agent_session with the session id', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { endAgentSession } = await import('@/lib/tauri-api');
    await endAgentSession('session-1');
    expect(invoke).toHaveBeenCalledWith('end_agent_session', { sessionId: 'session-1' });
  });

  it('onAgentSessionStarted subscribes to agent-session-started and unwraps the payload', async () => {
    const payload = { type: 'acpSessionStarted', session_id: 'session-1' };
    vi.mocked(listen).mockImplementation(((
      _event: string,
      cb: (e: { payload: unknown }) => void,
    ) => {
      cb({ payload });
      return Promise.resolve(() => undefined);
    }) as typeof listen);
    const { onAgentSessionStarted } = await import('@/lib/tauri-api');
    const handler = vi.fn();
    await onAgentSessionStarted(handler);
    expect(listen).toHaveBeenCalledWith('agent-session-started', expect.any(Function));
    expect(handler).toHaveBeenCalledWith(payload);
  });

  it('onAgentSessionChunk subscribes to agent-session-chunk and unwraps the payload', async () => {
    const payload = { type: 'acpSessionChunk', session_id: 'session-1', text: 'hello' };
    vi.mocked(listen).mockImplementation(((
      _event: string,
      cb: (e: { payload: unknown }) => void,
    ) => {
      cb({ payload });
      return Promise.resolve(() => undefined);
    }) as typeof listen);
    const { onAgentSessionChunk } = await import('@/lib/tauri-api');
    const handler = vi.fn();
    await onAgentSessionChunk(handler);
    expect(listen).toHaveBeenCalledWith('agent-session-chunk', expect.any(Function));
    expect(handler).toHaveBeenCalledWith(payload);
  });

  it('onAgentSessionFinished subscribes to agent-session-finished and unwraps the payload', async () => {
    const payload = {
      type: 'acpSessionFinished',
      session_id: 'session-1',
      stop_reason: 'end_turn',
    };
    vi.mocked(listen).mockImplementation(((
      _event: string,
      cb: (e: { payload: unknown }) => void,
    ) => {
      cb({ payload });
      return Promise.resolve(() => undefined);
    }) as typeof listen);
    const { onAgentSessionFinished } = await import('@/lib/tauri-api');
    const handler = vi.fn();
    await onAgentSessionFinished(handler);
    expect(listen).toHaveBeenCalledWith('agent-session-finished', expect.any(Function));
    expect(handler).toHaveBeenCalledWith(payload);
  });

  it('onAgentSessionFailed subscribes to agent-session-failed and unwraps the payload', async () => {
    const payload = { type: 'acpSessionFailed', session_id: 'session-1', error: 'boom' };
    vi.mocked(listen).mockImplementation(((
      _event: string,
      cb: (e: { payload: unknown }) => void,
    ) => {
      cb({ payload });
      return Promise.resolve(() => undefined);
    }) as typeof listen);
    const { onAgentSessionFailed } = await import('@/lib/tauri-api');
    const handler = vi.fn();
    await onAgentSessionFailed(handler);
    expect(listen).toHaveBeenCalledWith('agent-session-failed', expect.any(Function));
    expect(handler).toHaveBeenCalledWith(payload);
  });
});
