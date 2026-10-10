import { describe, expect, it } from 'vitest';
import type { AssistantMessage } from '@/stores/assistant-store';
import { makeProposal } from '@/test/assistant-fixtures';
import { buildConversationInput, toStoredMessages } from '../conversation-snapshot';

const messages: AssistantMessage[] = [
  { kind: 'user', id: 'u1', text: 'Why 401?' },
  { kind: 'agent', id: 'a1', text: '', streaming: false },
  { kind: 'tool', id: 't1', callId: 'c1', title: 'get_request: login', status: 'completed' },
  {
    kind: 'agent',
    id: 'a2',
    text: 'The token expired.',
    streaming: false,
    error: 'stopped early',
  },
  { kind: 'notice', id: 'n1', text: 'Session ended.' },
];

describe('toStoredMessages', () => {
  it('maps every kind, drops empty and streaming segments and places proposals at their anchor', () => {
    const proposal = makeProposal({ id: 'p1', summary: 'Refresh the token', status: 'rejected' });
    const withStreaming: AssistantMessage[] = [
      ...messages,
      { kind: 'agent', id: 'a3', text: 'partial', streaming: true },
    ];
    expect(toStoredMessages(withStreaming, [proposal], { p1: 't1' })).toEqual([
      { kind: 'user', text: 'Why 401?' },
      { kind: 'tool', title: 'get_request: login', status: 'completed' },
      { kind: 'proposal', summary: 'Refresh the token', status: 'rejected' },
      { kind: 'agent', text: 'The token expired.' },
      { kind: 'notice', text: 'stopped early' },
      { kind: 'notice', text: 'Session ended.' },
    ]);
  });
});

describe('toStoredMessages pending proposals', () => {
  it('leaves out a proposal that is still pending', () => {
    const pending = makeProposal({ id: 'p1', summary: 'Waiting', status: 'pending' });
    const done = makeProposal({ id: 'p2', summary: 'Done', status: 'accepted' });
    const list: AssistantMessage[] = [{ kind: 'user', id: 'u1', text: 'Hi' }];
    expect(toStoredMessages(list, [pending, done], { p1: 'u1', p2: 'u1' })).toEqual([
      { kind: 'user', text: 'Hi' },
      { kind: 'proposal', summary: 'Done', status: 'accepted' },
    ]);
  });
});

describe('buildConversationInput', () => {
  const base = {
    conversationId: 'c1',
    agentConfigId: 'agent-1',
    mode: 'ask' as const,
    workspaceId: 'ws-1',
    collections: ['orders'],
    proposals: [],
    proposalAnchors: {},
  };

  it('returns null until the agent replied with text', () => {
    expect(
      buildConversationInput({ ...base, messages: [{ kind: 'user', id: 'u', text: 'hi' }] }),
    ).toBeNull();
  });

  it('builds the save payload, with continuedFrom only when set', () => {
    const chat: AssistantMessage[] = [
      { kind: 'user', id: 'u', text: 'hi' },
      { kind: 'agent', id: 'a', text: 'hello', streaming: false },
    ];
    expect(buildConversationInput({ ...base, messages: chat })).toEqual({
      id: 'c1',
      agentConfigId: 'agent-1',
      mode: 'ask',
      workspaceId: 'ws-1',
      collections: ['orders'],
      messages: [
        { kind: 'user', text: 'hi' },
        { kind: 'agent', text: 'hello' },
      ],
    });
    expect(
      buildConversationInput({ ...base, messages: chat, continuedFrom: 'c0' })?.continuedFrom,
    ).toBe('c0');
  });
});
