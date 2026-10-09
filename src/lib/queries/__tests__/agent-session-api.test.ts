import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

describe('ACP chat session tauri-api bindings', () => {
  it('startAgentSession invokes start_agent_session and returns the session info', async () => {
    const started = { sessionId: 'session-1', configOptions: [] };
    vi.mocked(invoke).mockResolvedValue(started);
    const { startAgentSession } = await import('@/lib/tauri-api');
    const result = await startAgentSession(
      'agent-1',
      '/collections/my-collection',
      'my-collection',
    );
    expect(invoke).toHaveBeenCalledWith('start_agent_session', {
      agentConfigId: 'agent-1',
      cwd: '/collections/my-collection',
      collection: 'my-collection',
    });
    expect(result).toEqual(started);
  });

  it('sendAgentPrompt sends a null resource list when none is given', async () => {
    vi.mocked(invoke).mockResolvedValue('end_turn');
    const { sendAgentPrompt } = await import('@/lib/tauri-api');
    const result = await sendAgentPrompt('session-1', 'hello');
    expect(invoke).toHaveBeenCalledWith('send_agent_prompt', {
      sessionId: 'session-1',
      prompt: 'hello',
      resources: null,
    });
    expect(result).toBe('end_turn');
  });

  it('sendAgentPrompt passes resources through', async () => {
    vi.mocked(invoke).mockResolvedValue('end_turn');
    const { sendAgentPrompt } = await import('@/lib/tauri-api');
    const resources = [{ uri: 'rocket://request/a', mimeType: 'text/plain', text: 'GET /a' }];
    await sendAgentPrompt('session-1', 'explain', resources);
    expect(invoke).toHaveBeenCalledWith('send_agent_prompt', {
      sessionId: 'session-1',
      prompt: 'explain',
      resources,
    });
  });

  it('cancelAgentPrompt invokes cancel_agent_prompt with the session id', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { cancelAgentPrompt } = await import('@/lib/tauri-api');
    await cancelAgentPrompt('session-1');
    expect(invoke).toHaveBeenCalledWith('cancel_agent_prompt', { sessionId: 'session-1' });
  });

  it('setAgentConfigOption invokes set_agent_config_option and returns the options', async () => {
    const options = [
      { id: 'model', name: 'Model', category: 'model', currentValue: 'opus', choices: [] },
    ];
    vi.mocked(invoke).mockResolvedValue(options);
    const { setAgentConfigOption } = await import('@/lib/tauri-api');
    const result = await setAgentConfigOption('session-1', 'model', 'opus');
    expect(invoke).toHaveBeenCalledWith('set_agent_config_option', {
      sessionId: 'session-1',
      configId: 'model',
      value: 'opus',
    });
    expect(result).toEqual(options);
  });

  it('endAgentSession invokes end_agent_session with the session id', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { endAgentSession } = await import('@/lib/tauri-api');
    await endAgentSession('session-1');
    expect(invoke).toHaveBeenCalledWith('end_agent_session', { sessionId: 'session-1' });
  });

  it('endStaleAssistantSessions invokes end_stale_assistant_sessions and returns the count', async () => {
    vi.mocked(invoke).mockResolvedValue(2);
    const { endStaleAssistantSessions } = await import('@/lib/tauri-api');
    const ended = await endStaleAssistantSessions();
    expect(invoke).toHaveBeenCalledWith('end_stale_assistant_sessions');
    expect(ended).toBe(2);
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

  it.each([
    [
      'onAgentToolActivity',
      'agent-session-tool-activity',
      {
        type: 'acpToolActivity',
        session_id: 'session-1',
        call_id: 'call-1',
        title: 'Read GET /orders',
        status: 'in_progress',
      },
    ],
    [
      'onAgentConfigOptions',
      'agent-session-config-options',
      { type: 'acpConfigOptionsChanged', session_id: 'session-1', options: [] },
    ],
    [
      'onAgentUsage',
      'agent-session-usage',
      { type: 'acpUsage', session_id: 'session-1', used: 10, size: 100, cost_usd: null },
    ],
  ] as const)('%s subscribes to %s and unwraps the payload', async (name, channel, payload) => {
    vi.mocked(listen).mockImplementation(((
      _event: string,
      cb: (e: { payload: unknown }) => void,
    ) => {
      cb({ payload });
      return Promise.resolve(() => undefined);
    }) as typeof listen);
    const api = await import('@/lib/tauri-api');
    const handler = vi.fn();
    await api[name](handler);
    expect(listen).toHaveBeenCalledWith(channel, expect.any(Function));
    expect(handler).toHaveBeenCalledWith(payload);
  });

  it('configOptionsFromEvent converts snake_case options to the camelCase shape', async () => {
    const { configOptionsFromEvent } = await import('@/lib/tauri-api');
    expect(
      configOptionsFromEvent([
        {
          id: 'effort',
          name: 'Effort',
          category: 'thought_level',
          current_value: 'high',
          choices: [{ value: 'high', name: 'High', description: null }],
        },
      ]),
    ).toEqual([
      {
        id: 'effort',
        name: 'Effort',
        category: 'thought_level',
        currentValue: 'high',
        choices: [{ value: 'high', name: 'High', description: null }],
      },
    ]);
  });
});
