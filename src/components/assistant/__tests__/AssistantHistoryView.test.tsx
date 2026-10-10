import { act, render, screen, waitFor, within } from '@testing-library/react';
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

const toastError = vi.hoisted(() => vi.fn());

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
vi.mock('sonner', () => ({ toast: { error: (m: string) => toastError(m) } }));
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
    expect(screen.getByText(/Claude · 4 items/)).toBeInTheDocument();
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

  it('shows 1 item in the singular', async () => {
    vi.mocked(api.listConversations).mockResolvedValue([{ ...summary, messageCount: 1 }]);
    render(<AssistantHistoryView />);
    expect(await screen.findByText(/Claude · 1 item ·/)).toBeInTheDocument();
  });

  it('keeps the newest list when an older request resolves last', async () => {
    let resolveOld: (value: ConversationSummary[]) => void = () => undefined;
    vi.mocked(api.listConversations)
      .mockReturnValueOnce(new Promise((resolve) => { resolveOld = resolve; }))
      .mockResolvedValueOnce([{ ...summary, id: 'new', title: 'Newer list' }]);
    render(<AssistantHistoryView />);
    await waitFor(() => expect(mocks.handlers.saved).toBeDefined());
    act(() => mocks.handlers.saved({ type: 'assistantConversationSaved', conversation_id: 'x' }));
    expect(await screen.findByText('Newer list')).toBeInTheDocument();
    await act(async () => resolveOld([{ ...summary, id: 'old', title: 'Older list' }]));
    expect(screen.getByText('Newer list')).toBeInTheDocument();
    expect(screen.queryByText('Older list')).toBeNull();
  });

  it('hides the previous workspace list while the new one loads', async () => {
    render(<AssistantHistoryView />);
    await screen.findByText('Why does login return 401?');
    vi.mocked(api.listConversations).mockReturnValue(new Promise(() => undefined));
    act(() => useWorkspaceStore.setState({ activeWorkspaceId: 'ws-2' }));
    expect(screen.queryByText('Why does login return 401?')).toBeNull();
    expect(screen.queryByRole('button', { name: /^Delete "/ })).toBeNull();
  });

  it('discards a conversation that loads after a workspace switch', async () => {
    let resolveRecord: (value: ConversationRecord) => void = () => undefined;
    vi.mocked(api.getConversation).mockReturnValue(
      new Promise((resolve) => { resolveRecord = resolve; }),
    );
    render(<AssistantHistoryView />);
    await userEvent.click(await screen.findByText('Why does login return 401?'));
    act(() => useWorkspaceStore.setState({ activeWorkspaceId: 'ws-2' }));
    await act(async () => resolveRecord(record));
    expect(screen.queryByText('The token expired.')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Back to the history' })).toBeNull();
  });

  it('does not reopen a conversation deleted while it loads', async () => {
    let resolveRecord: (value: ConversationRecord) => void = () => undefined;
    vi.mocked(api.getConversation).mockReturnValue(
      new Promise((resolve) => { resolveRecord = resolve; }),
    );
    render(<AssistantHistoryView />);
    await userEvent.click(await screen.findByText('Why does login return 401?'));
    act(() => mocks.handlers.deleted({ type: 'assistantConversationDeleted', conversation_id: 'c1' }));
    await act(async () => resolveRecord(record));
    expect(screen.queryByText('The token expired.')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Back to the history' })).toBeNull();
  });

  it('shows a generic error with a retry', async () => {
    vi.mocked(api.listConversations).mockRejectedValueOnce(new Error('secret path /x/y'));
    render(<AssistantHistoryView />);
    expect(await screen.findByText('Could not load the conversations.')).toBeInTheDocument();
    expect(screen.queryByText(/secret path/)).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(await screen.findByText('Why does login return 401?')).toBeInTheDocument();
  });

  it('clears stale items when a reload fails', async () => {
    render(<AssistantHistoryView />);
    await screen.findByText('Why does login return 401?');
    vi.mocked(api.listConversations).mockRejectedValueOnce(new Error('boom'));
    act(() => mocks.handlers.saved({ type: 'assistantConversationSaved', conversation_id: 'c2' }));
    expect(await screen.findByText('Could not load the conversations.')).toBeInTheDocument();
    expect(screen.queryByText('Why does login return 401?')).toBeNull();
  });

  it('does not delete twice while a delete is in flight', async () => {
    let finish: () => void = () => undefined;
    vi.mocked(api.deleteConversation).mockReturnValue(new Promise((resolve) => { finish = () => resolve(undefined); }));
    render(<AssistantHistoryView />);
    const button = await screen.findByRole('button', { name: 'Delete "Why does login return 401?"' });
    await userEvent.click(button);
    expect(button).toBeDisabled();
    await userEvent.click(button);
    expect(api.deleteConversation).toHaveBeenCalledTimes(1);
    await act(async () => finish());
  });

  it('uses a generic toast when a delete fails', async () => {
    vi.mocked(api.deleteConversation).mockRejectedValue(new Error('disk /secret'));
    render(<AssistantHistoryView />);
    await userEvent.click(
      await screen.findByRole('button', { name: 'Delete "Why does login return 401?"' }),
    );
    await waitFor(() => expect(toastError).toHaveBeenCalledWith('Could not delete the conversation.'));
  });

  it.each([
    ['deleted', { type: 'assistantConversationDeleted', conversation_id: 'c1' }],
    ['cleared', { type: 'assistantConversationsCleared' }],
  ])('closes the open conversation on the %s event', async (name, payload) => {
    render(<AssistantHistoryView />);
    await userEvent.click(await screen.findByText('Why does login return 401?'));
    await screen.findByRole('button', { name: 'Back to the history' });
    act(() => mocks.handlers[name](payload));
    expect(await screen.findByText(HISTORY_PRIVACY_NOTICE)).toBeInTheDocument();
  });

  it('disposes listeners that register after the view unmounts', async () => {
    const dispose = vi.fn(() => undefined);
    let registered: (fn: () => undefined) => void = () => undefined;
    mocks.api.onAssistantConversationSaved.mockImplementationOnce((handler) => {
      mocks.handlers.saved = handler;
      return new Promise((resolve) => { registered = resolve; });
    });
    const { unmount } = render(<AssistantHistoryView />);
    unmount();
    await act(async () => registered(dispose));
    expect(dispose).toHaveBeenCalledTimes(1);
  });

  it('has only the back button as an action in the read-only view', async () => {
    render(<AssistantHistoryView />);
    await userEvent.click(await screen.findByText('Why does login return 401?'));
    await screen.findByText('The token expired.');
    const buttons = screen.getAllByRole('button');
    expect(buttons.map((b) => b.getAttribute('aria-label'))).toEqual(['Back to the history']);
    expect(within(document.body).queryByRole('textbox')).toBeNull();
  });

  it('names the conversation a continued one came from', async () => {
    vi.mocked(api.listConversations).mockResolvedValue([
      { ...summary, id: 'c2', title: 'Follow-up', continuedFrom: 'c1' },
      { ...summary, id: 'c3', title: 'Older follow-up', continuedFrom: 'gone' },
      summary,
    ]);
    render(<AssistantHistoryView />);
    expect(
      await screen.findByText('Continued from "Why does login return 401?"'),
    ).toBeInTheDocument();
    expect(screen.getByText('Continued from an earlier conversation')).toBeInTheDocument();
  });
});
