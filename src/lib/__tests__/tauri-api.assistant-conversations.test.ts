import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  type ConversationInput,
  clearConversations,
  deleteConversation,
  getConversation,
  listConversations,
  onAssistantConversationDeleted,
  onAssistantConversationSaved,
  onAssistantConversationsCleared,
  saveConversation,
} from '../tauri-api';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

const input: ConversationInput = {
  id: '0b6f3f4e-0000-4000-8000-000000000001',
  agentConfigId: 'agent-1',
  mode: 'ask',
  workspaceId: 'ws-1',
  collections: ['orders'],
  messages: [
    { kind: 'user', text: 'Why 401?' },
    { kind: 'agent', text: 'The token expired.' },
  ],
};

describe('assistant conversation commands', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(listen).mockReset();
  });

  it('calls the five commands with camelCase arguments', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await listConversations();
    expect(invoke).toHaveBeenLastCalledWith('list_conversations');
    await getConversation('c1');
    expect(invoke).toHaveBeenLastCalledWith('get_conversation', { id: 'c1' });
    await saveConversation('s1', input);
    expect(invoke).toHaveBeenLastCalledWith('save_conversation', {
      sessionId: 's1',
      conversation: input,
    });
    await deleteConversation('c1');
    expect(invoke).toHaveBeenLastCalledWith('delete_conversation', { id: 'c1' });
    await clearConversations();
    expect(invoke).toHaveBeenLastCalledWith('clear_conversations');
  });

  it('listens on the three channels and passes the payload through', async () => {
    vi.mocked(listen).mockResolvedValue(vi.fn());
    const handler = vi.fn();
    await onAssistantConversationSaved(handler);
    await onAssistantConversationDeleted(handler);
    await onAssistantConversationsCleared(handler);
    expect(vi.mocked(listen).mock.calls.map((call) => call[0])).toEqual([
      'assistant-conversation-saved',
      'assistant-conversation-deleted',
      'assistant-conversations-cleared',
    ]);
    const deliver = vi.mocked(listen).mock.calls[0][1];
    deliver({ payload: { type: 'assistantConversationSaved', conversation_id: 'c1' } } as never);
    expect(handler).toHaveBeenCalledWith({
      type: 'assistantConversationSaved',
      conversation_id: 'c1',
    });
  });
});
