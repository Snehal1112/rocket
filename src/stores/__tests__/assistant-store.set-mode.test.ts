import { beforeEach, describe, expect, it } from 'vitest';
import { useAssistantStore } from '@/stores/assistant-store';

beforeEach(() => {
  useAssistantStore.setState({ session: undefined });
});

describe('assistant-store setMode', () => {
  it('changes the mode of the current session', () => {
    useAssistantStore.setState({
      session: {
        sessionId: 's1',
        agentConfigId: 'a1',
        status: 'active',
        configOptions: [],
        mode: 'ask',
      },
    });
    useAssistantStore.getState().setMode('s1', 'agent');
    expect(useAssistantStore.getState().session?.mode).toBe('agent');
  });

  it('ignores a session that is no longer current', () => {
    useAssistantStore.setState({
      session: {
        sessionId: 's2',
        agentConfigId: 'a1',
        status: 'active',
        configOptions: [],
        mode: 'ask',
      },
    });
    useAssistantStore.getState().setMode('s1', 'agent');
    expect(useAssistantStore.getState().session?.mode).toBe('ask');
  });

  it('does nothing without a session', () => {
    useAssistantStore.getState().setMode('s1', 'edit');
    expect(useAssistantStore.getState().session).toBeUndefined();
  });
});
