import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { AgentChatSession } from '@/types/pane-types';
import { AgentChatPanel } from '../AgentChatPanel';

// Radix Select (used for the agent picker) calls pointer-capture and
// scrollIntoView APIs jsdom doesn't implement — polyfill them so
// userEvent can open the dropdown and pick an option.
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
}
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => {
    // No-op for test polyfill.
  };
}

const mockActions = vi.hoisted(() => ({
  beginAgentSession: vi.fn(),
  activateAgentSession: vi.fn(),
  appendAgentChatMessage: vi.fn(),
  failAgentChatMessage: vi.fn(),
  markAgentSessionEnded: vi.fn(),
  clearAgentSession: vi.fn(),
}));

vi.mock('@/stores/pane-store', () => ({
  usePaneStore: (selector: (s: typeof mockActions) => unknown) => selector(mockActions),
}));

vi.mock('@/lib/collection-path', () => ({
  useCollectionPath: () => '/ws/root/collections/my-collection',
}));

vi.mock('@/lib/queries/agent-config-queries', () => ({
  useAgentConfigs: () => ({
    data: [
      { id: 'agent-1', label: 'Claude', command: 'claude-acp', args: [] },
      { id: 'agent-2', label: 'Gemini', command: 'gemini-acp', args: [] },
    ],
  }),
}));

vi.mock('@/lib/tauri-api', () => ({
  startAgentSession: vi.fn(),
  sendAgentPrompt: vi.fn(),
  endAgentSession: vi.fn(),
}));

import * as tauriApi from '@/lib/tauri-api';

function activeSession(overrides: Partial<AgentChatSession> = {}): AgentChatSession {
  return {
    agentConfigId: 'agent-1',
    sessionId: 'session-1',
    status: 'active',
    messages: [],
    ...overrides,
  };
}

describe('AgentChatPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockActions.activateAgentSession.mockReturnValue(true);
  });

  it('shows the agent picker and a disabled Start button with no session', () => {
    render(<AgentChatPanel tabId='tab-1' collectionName='my-collection' onInsertCode={vi.fn()} />);
    expect(screen.getByText('Select an agent…')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Start' })).toBeDisabled();
  });

  it('starting a session calls startAgentSession with the resolved cwd, then begins and activates it', async () => {
    vi.mocked(tauriApi.startAgentSession).mockResolvedValue('session-1');
    render(<AgentChatPanel tabId='tab-1' collectionName='my-collection' onInsertCode={vi.fn()} />);

    await userEvent.click(screen.getByRole('combobox'));
    await userEvent.click(await screen.findByText('Claude'));
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));

    expect(mockActions.beginAgentSession).toHaveBeenCalledWith('tab-1', 'agent-1');
    await waitFor(() =>
      expect(tauriApi.startAgentSession).toHaveBeenCalledWith(
        'agent-1',
        '/ws/root/collections/my-collection',
        'my-collection',
      ),
    );
    await waitFor(() =>
      expect(mockActions.activateAgentSession).toHaveBeenCalledWith('tab-1', 'session-1'),
    );
  });

  it('ends the new session when the store refuses to activate it (tab was removed)', async () => {
    vi.mocked(tauriApi.startAgentSession).mockResolvedValue('session-orphan');
    vi.mocked(tauriApi.endAgentSession).mockResolvedValue(undefined);
    mockActions.activateAgentSession.mockReturnValue(false);
    render(<AgentChatPanel tabId='tab-1' collectionName='my-collection' onInsertCode={vi.fn()} />);

    await userEvent.click(screen.getByRole('combobox'));
    await userEvent.click(await screen.findByText('Claude'));
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));

    await waitFor(() => expect(tauriApi.endAgentSession).toHaveBeenCalledWith('session-orphan'));
    expect(mockActions.activateAgentSession).toHaveBeenCalledWith('tab-1', 'session-orphan');
    expect(mockActions.clearAgentSession).not.toHaveBeenCalled();
  });

  it('does not end the new session when the store activates it', async () => {
    vi.mocked(tauriApi.startAgentSession).mockResolvedValue('session-1');
    mockActions.activateAgentSession.mockReturnValue(true);
    render(<AgentChatPanel tabId='tab-1' collectionName='my-collection' onInsertCode={vi.fn()} />);

    await userEvent.click(screen.getByRole('combobox'));
    await userEvent.click(await screen.findByText('Claude'));
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));

    await waitFor(() =>
      expect(mockActions.activateAgentSession).toHaveBeenCalledWith('tab-1', 'session-1'),
    );
    expect(tauriApi.endAgentSession).not.toHaveBeenCalled();
  });

  it('a start failure shows an inline error and clears the session', async () => {
    vi.mocked(tauriApi.startAgentSession).mockRejectedValue(new Error('no credential'));
    render(<AgentChatPanel tabId='tab-1' collectionName='my-collection' onInsertCode={vi.fn()} />);

    await userEvent.click(screen.getByRole('combobox'));
    await userEvent.click(await screen.findByText('Claude'));
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));

    await waitFor(() => expect(mockActions.clearAgentSession).toHaveBeenCalledWith('tab-1'));
    expect(screen.getByText(/no credential/)).toBeInTheDocument();
  });

  it('sending a message appends a user message and a streaming placeholder, then calls sendAgentPrompt', async () => {
    vi.mocked(tauriApi.sendAgentPrompt).mockResolvedValue('end_turn');
    render(
      <AgentChatPanel
        tabId='tab-1'
        collectionName='my-collection'
        agentSession={activeSession()}
        onInsertCode={vi.fn()}
      />,
    );

    await userEvent.type(screen.getByPlaceholderText('Ask the agent…'), 'hello');
    await userEvent.click(screen.getByRole('button', { name: /send/i }));

    expect(mockActions.appendAgentChatMessage).toHaveBeenNthCalledWith(1, 'tab-1', {
      id: expect.any(String),
      role: 'user',
      text: 'hello',
    });
    expect(mockActions.appendAgentChatMessage).toHaveBeenNthCalledWith(2, 'tab-1', {
      id: expect.any(String),
      role: 'agent',
      text: '',
      streaming: true,
    });
    await waitFor(() =>
      expect(tauriApi.sendAgentPrompt).toHaveBeenCalledWith('session-1', 'hello'),
    );
  });

  it('does not send a second prompt while a message is still streaming', async () => {
    // sendAgentPrompt deliberately never resolves during this test, so the
    // first send stays "in flight" while we attempt a second one.
    vi.mocked(tauriApi.sendAgentPrompt).mockReturnValue(new Promise(() => undefined));

    const { rerender } = render(
      <AgentChatPanel
        tabId='tab-1'
        collectionName='my-collection'
        agentSession={activeSession()}
        onInsertCode={vi.fn()}
      />,
    );

    // First send: no streaming message yet, so the guard lets it through
    // and sendAgentPrompt is actually invoked.
    await userEvent.type(screen.getByPlaceholderText('Ask the agent…'), 'hello');
    await userEvent.click(screen.getByRole('button', { name: /send/i }));
    expect(tauriApi.sendAgentPrompt).toHaveBeenCalledTimes(1);

    // Simulate the store update a real send would have produced by now: an
    // agent message with streaming: true is present on the session prop.
    rerender(
      <AgentChatPanel
        tabId='tab-1'
        collectionName='my-collection'
        agentSession={activeSession({
          messages: [{ id: 'm1', role: 'agent', text: '', streaming: true }],
        })}
        onInsertCode={vi.fn()}
      />,
    );

    const sendButton = screen.getByRole('button', { name: /send/i });
    expect(sendButton).toBeDisabled();
    await userEvent.click(sendButton);

    // The second attempt must not have reached sendAgentPrompt at all.
    expect(tauriApi.sendAgentPrompt).toHaveBeenCalledTimes(1);
  });

  it('clicking Insert on a rendered code block calls onInsertCode with the code', async () => {
    const onInsertCode = vi.fn();
    render(
      <AgentChatPanel
        tabId='tab-1'
        collectionName='my-collection'
        agentSession={activeSession({
          messages: [
            { id: 'm1', role: 'agent', text: '```js\nconst x = 1;\n```', streaming: false },
          ],
        })}
        onInsertCode={onInsertCode}
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Insert' }));
    expect(onInsertCode).toHaveBeenCalledWith('const x = 1;');
  });

  it('End session calls endAgentSession then marks the session ended', async () => {
    vi.mocked(tauriApi.endAgentSession).mockResolvedValue(undefined);
    render(
      <AgentChatPanel
        tabId='tab-1'
        collectionName='my-collection'
        agentSession={activeSession()}
        onInsertCode={vi.fn()}
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'End session' }));
    await waitFor(() => expect(tauriApi.endAgentSession).toHaveBeenCalledWith('session-1'));
    expect(mockActions.markAgentSessionEnded).toHaveBeenCalledWith('tab-1');
  });
});
