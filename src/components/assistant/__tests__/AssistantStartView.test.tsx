import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { startAssistant } from '@/lib/assistant/assistant-session';
import { useAssistantStore } from '@/stores/assistant-store';
import { AssistantStartView } from '../AssistantStartView';

// Radix Select calls pointer-capture APIs that jsdom lacks.
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
}
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => undefined;
}

vi.mock('@/lib/assistant/assistant-session', () => ({ startAssistant: vi.fn() }));
vi.mock('@/lib/queries/agent-config-queries', () => ({
  useAgentConfigs: () => ({ data: [{ id: 'agent-1', label: 'Claude' }] }),
}));

function renderView() {
  const qc = new QueryClient();
  return render(
    <QueryClientProvider client={qc}>
      <AssistantStartView />
    </QueryClientProvider>,
  );
}

describe('AssistantStartView retry', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useAssistantStore.getState().reset();
  });

  it('retries a failed Continue start in Ask mode with the same replay seed', async () => {
    const seed = { fromId: 'c0', preface: 'earlier text' };
    const token = useAssistantStore.getState().beginSession('agent-1', 'ask', seed);
    useAssistantStore.getState().failStart(token, 'boom');
    renderView();
    await userEvent.click(screen.getByRole('button', { name: /Start/ }));
    expect(startAssistant).toHaveBeenCalledWith('agent-1', 'ask', seed);
  });

  it('retries a normal failed start without a seed', async () => {
    const token = useAssistantStore.getState().beginSession('agent-1', 'edit');
    useAssistantStore.getState().failStart(token, 'boom');
    renderView();
    await userEvent.click(screen.getByRole('button', { name: /Start/ }));
    expect(startAssistant).toHaveBeenCalledWith('agent-1', 'edit', undefined);
  });

  it('restarts in the default mode after a mid-session fatal failure', async () => {
    const store = useAssistantStore.getState();
    const token = store.beginSession('agent-1', 'ask');
    store.activateSession(token, 's1', []);
    store.setMode('s1', 'agent');
    store.failMessage('s1', 'dead', true);
    renderView();
    await userEvent.click(screen.getByRole('button', { name: /Start/ }));
    expect(startAssistant).toHaveBeenCalledWith('agent-1', undefined, undefined);
  });

  it('drops the replay seed after a continued session dies mid-chat', async () => {
    const store = useAssistantStore.getState();
    const token = store.beginSession('agent-1', 'ask', { fromId: 'c0', preface: 'earlier' });
    store.activateSession(token, 's1', []);
    store.markReplaySent('s1');
    store.failMessage('s1', 'dead', true);
    renderView();
    await userEvent.click(screen.getByRole('button', { name: /Start/ }));
    expect(startAssistant).toHaveBeenCalledWith('agent-1', undefined, undefined);
  });

  it('starts fresh with defaults when there was no failure', async () => {
    renderView();
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));
    expect(startAssistant).toHaveBeenCalledWith('agent-1', undefined, undefined);
  });
});
