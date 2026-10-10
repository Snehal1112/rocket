import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { canContinueHere, continueConversation } from '@/lib/assistant/assistant-session';
import type { ConversationRecord } from '@/lib/tauri-api';
import { useAssistantStore } from '@/stores/assistant-store';
import {
  AssistantConversationView,
  CONTINUE_HINT,
  MISSING_AGENT_TEXT,
} from '../AssistantConversationView';

// Radix Select calls pointer-capture APIs that jsdom lacks.
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
}
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => undefined;
}

vi.mock('@/lib/assistant/assistant-session', () => ({
  CONTINUE_WORKSPACE_MISMATCH: 'Another workspace.',
  canContinueHere: vi.fn(() => true),
  continueConversation: vi.fn(),
}));
vi.mock('sonner', () => ({ toast: { error: vi.fn() } }));
vi.mock('@/lib/queries/agent-config-queries', () => ({
  useAgentConfigs: () => ({ data: [{ id: 'agent-1', label: 'Claude' }] }),
}));
vi.mock('@/components/collections/MarkdownRenderer', () => ({
  MarkdownRenderer: ({ children }: { children: string }) => <div>{children}</div>,
}));

const record: ConversationRecord = {
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

describe('AssistantConversationView Continue', () => {
  beforeEach(() => vi.clearAllMocks());

  it('continues with the stored agent and leaves the history', async () => {
    const onContinued = vi.fn();
    render(
      <AssistantConversationView
        record={record}
        agentLabel='Claude'
        onBack={vi.fn()}
        onContinued={onContinued}
      />,
    );
    expect(screen.getByText(CONTINUE_HINT)).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Continue' }));
    expect(onContinued).toHaveBeenCalledTimes(1);
    expect(continueConversation).toHaveBeenCalledWith(record, 'agent-1');
  });

  it('stays in the history and says why when Continue is refused', async () => {
    vi.mocked(canContinueHere).mockReturnValueOnce(false);
    const onContinued = vi.fn();
    render(
      <AssistantConversationView
        record={record}
        agentLabel='Claude'
        onBack={vi.fn()}
        onContinued={onContinued}
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Continue' }));
    expect(onContinued).not.toHaveBeenCalled();
    expect(continueConversation).not.toHaveBeenCalled();
  });

  it('shows a generic toast and returns to the history when Continue throws before the start', async () => {
    vi.mocked(continueConversation).mockRejectedValueOnce(new Error('secret text'));
    useAssistantStore.getState().reset();
    const onContinueFailed = vi.fn();
    render(
      <AssistantConversationView
        record={record}
        agentLabel='Claude'
        onBack={vi.fn()}
        onContinued={vi.fn()}
        onContinueFailed={onContinueFailed}
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Continue' }));
    await waitFor(() => expect(onContinueFailed).toHaveBeenCalledTimes(1));
    expect(toast.error).toHaveBeenCalledWith('Could not continue this conversation.');
  });

  it('returns to the history on a pre-start throw even when a stale replay id matches', async () => {
    // An earlier Continue of this record left its id in the store after the session ended.
    const store = useAssistantStore.getState();
    const token = store.beginSession('agent-1', 'ask', { fromId: record.id, preface: 'old' });
    store.activateSession(token, 's1', []);
    store.endSession();
    vi.mocked(continueConversation).mockRejectedValueOnce(
      Object.assign(new Error('workspace changed'), { started: false }),
    );
    const onContinueFailed = vi.fn();
    render(
      <AssistantConversationView
        record={record}
        agentLabel='Claude'
        onBack={vi.fn()}
        onContinued={vi.fn()}
        onContinueFailed={onContinueFailed}
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Continue' }));
    await waitFor(() => expect(onContinueFailed).toHaveBeenCalledTimes(1));
  });

  it('keeps the chat open when the start began and then failed', async () => {
    vi.mocked(continueConversation).mockRejectedValueOnce(
      Object.assign(new Error('late'), { started: true }),
    );
    const onContinueFailed = vi.fn();
    render(
      <AssistantConversationView
        record={record}
        agentLabel='Claude'
        onBack={vi.fn()}
        onContinued={vi.fn()}
        onContinueFailed={onContinueFailed}
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Continue' }));
    await waitFor(() => expect(toast.error).toHaveBeenCalled());
    expect(onContinueFailed).not.toHaveBeenCalled();
  });

  it('asks for an agent when the stored one is gone', () => {
    render(
      <AssistantConversationView
        record={{ ...record, agentConfigId: 'deleted-agent' }}
        agentLabel='deleted-agent'
        onBack={vi.fn()}
        onContinued={vi.fn()}
      />,
    );
    expect(screen.getByText(MISSING_AGENT_TEXT)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Continue' })).toBeDisabled();
  });

  it('has no Continue without onContinued', () => {
    render(<AssistantConversationView record={record} agentLabel='Claude' onBack={vi.fn()} />);
    expect(screen.queryByRole('button', { name: 'Continue' })).toBeNull();
  });
});
