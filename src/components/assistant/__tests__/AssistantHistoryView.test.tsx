import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as api from '@/lib/tauri-api';
import type { ConversationRecord, ConversationSummary } from '@/lib/tauri-api';
import { useWorkspaceStore } from '@/stores/workspace-store';
import {
  AssistantHistoryView,
  HISTORY_EMPTY_TEXT,
  HISTORY_PRIVACY_NOTICE,
} from '../AssistantHistoryView';

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
      listConversations: vi.fn(),
      getConversation: vi.fn(),
      deleteConversation: vi.fn(),
      clearConversations: vi.fn(),
      onAssistantConversationSaved: listener('saved'),
      onAssistantConversationDeleted: listener('deleted'),
      onAssistantConversationsCleared: listener('cleared'),
    },
  };
});

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  ...mocks.api,
}));
vi.mock('@/lib/queries/agent-config-queries', () => ({
  useAgentConfigs: () => ({ data: [{ id: 'agent-1', label: 'Claude' }] }),
}));
vi.mock('@/components/collections/MarkdownRenderer', () => ({
  MarkdownRenderer: ({ children }: { children: string }) => <div>{children}</div>,
}));

const summary: ConversationSummary = {
  id: 'c1',
  title: 'Why does login return 401?',
  agentConfigId: 'agent-1',
  mode: 'ask',
  createdAt: '2026-10-11T09:12:00Z',
  updatedAt: '2026-10-11T09:20:41Z',
  messageCount: 4,
};

const record: ConversationRecord = {
  id: 'c1',
  title: 'Why does login return 401?',
  agentConfigId: 'agent-1',
  mode: 'ask',
  workspaceId: 'ws-1',
  createdAt: '2026-10-11T09:12:00Z',
  updatedAt: '2026-10-11T09:20:41Z',
  collections: [],
  messages: [
    { kind: 'user', text: 'Why does login return 401?' },
    { kind: 'agent', text: 'The token expired.' },
    { kind: 'tool', title: 'get_request: login', status: 'completed' },
    { kind: 'proposal', summary: 'Refresh the token', status: 'pending' },
  ],
};

describe('AssistantHistoryView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useWorkspaceStore.setState({ activeWorkspaceId: 'ws-1' });
    vi.mocked(api.listConversations).mockResolvedValue([summary]);
    vi.mocked(api.getConversation).mockResolvedValue(record);
    vi.mocked(api.deleteConversation).mockResolvedValue(undefined);
    vi.mocked(api.clearConversations).mockResolvedValue(undefined);
  });

  it('lists the conversations with their agent and message count, under the privacy notice', async () => {
    render(<AssistantHistoryView />);
    expect(await screen.findByText('Why does login return 401?')).toBeInTheDocument();
    expect(screen.getByText(/Claude · 4 messages/)).toBeInTheDocument();
    expect(screen.getByText(HISTORY_PRIVACY_NOTICE)).toBeInTheDocument();
  });

  it('says so when the workspace has no conversations', async () => {
    vi.mocked(api.listConversations).mockResolvedValue([]);
    render(<AssistantHistoryView />);
    expect(await screen.findByText(HISTORY_EMPTY_TEXT)).toBeInTheDocument();
  });

  it('opens a conversation read-only, with proposals as plain lines', async () => {
    render(<AssistantHistoryView />);
    await userEvent.click(await screen.findByText('Why does login return 401?'));
    expect(await screen.findByText('The token expired.')).toBeInTheDocument();
    expect(screen.getByText('get_request: login')).toBeInTheDocument();
    expect(screen.getByText('Proposal: Refresh the token (not decided)')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /accept|reject/i })).toBeNull();
    expect(api.getConversation).toHaveBeenCalledWith('c1');
    await userEvent.click(screen.getByRole('button', { name: 'Back to the history' }));
    expect(await screen.findByText(HISTORY_PRIVACY_NOTICE)).toBeInTheDocument();
  });

  it('deletes one conversation and reloads the list', async () => {
    render(<AssistantHistoryView />);
    await userEvent.click(
      await screen.findByRole('button', { name: 'Delete "Why does login return 401?"' }),
    );
    expect(api.deleteConversation).toHaveBeenCalledWith('c1');
    await waitFor(() => expect(api.listConversations).toHaveBeenCalledTimes(2));
  });

  it('clears all only after the confirm step', async () => {
    render(<AssistantHistoryView />);
    await screen.findByText('Why does login return 401?');
    await userEvent.click(screen.getByRole('button', { name: 'Clear all' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
    expect(api.clearConversations).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole('button', { name: 'Clear all' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Delete all' }));
    await waitFor(() => expect(api.clearConversations).toHaveBeenCalledTimes(1));
  });

  it('reloads when a conversation is saved elsewhere', async () => {
    render(<AssistantHistoryView />);
    await waitFor(() => expect(mocks.handlers.saved).toBeDefined());
    await screen.findByText('Why does login return 401?');
    act(() => mocks.handlers.saved({ type: 'assistantConversationSaved', conversation_id: 'c2' }));
    await waitFor(() => expect(api.listConversations).toHaveBeenCalledTimes(2));
  });

  it('a workspace switch closes the open conversation and reloads', async () => {
    render(<AssistantHistoryView />);
    await userEvent.click(await screen.findByText('Why does login return 401?'));
    expect(await screen.findByRole('button', { name: 'Back to the history' })).toBeInTheDocument();
    act(() => useWorkspaceStore.setState({ activeWorkspaceId: 'ws-2' }));
    expect(await screen.findByText(HISTORY_PRIVACY_NOTICE)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Back to the history' })).toBeNull();
    expect(api.listConversations).toHaveBeenCalledTimes(2);
  });
});
