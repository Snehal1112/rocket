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
    useAssistantStore.getState().setMode('agent');
    expect(useAssistantStore.getState().session?.mode).toBe('agent');
  });

  it('does nothing without a session', () => {
    useAssistantStore.getState().setMode('edit');
    expect(useAssistantStore.getState().session).toBeUndefined();
  });
});
