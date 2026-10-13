import { beforeEach, describe, expect, it } from 'vitest';
import { makeProposal } from '@/test/assistant-fixtures';
import {
  type AssistantMessage,
  MAX_REASONING_CHARS,
  selectTurnRunning,
  useAssistantStore,
} from '../assistant-store';

const store = () => useAssistantStore.getState();

function activate(sessionId = 's1'): void {
  const token = store().beginSession('agent-1', 'edit');
  store().activateSession(token, sessionId, []);
}

function lastMessage(): AssistantMessage | undefined {
  const { messages } = store();
  return messages[messages.length - 1];
}

describe('assistant-store', () => {
  beforeEach(() => {
    store().reset();
    useAssistantStore.setState({ panelOpen: false, focus: undefined });
  });

  it('gives the conversation an id at the first user message and a new one per session', () => {
    activate();
    expect(store().conversationId).toBeUndefined();
    store().appendUserMessage('hi');
    const first = store().conversationId;
    expect(first).toMatch(/^[0-9a-f-]{36}$/);
    store().completeMessage('s1');
    store().appendUserMessage('again');
    expect(store().conversationId).toBe(first);
    store().endSession();
    activate('s2');
    expect(store().conversationId).toBeUndefined();
  });

  it('collects the reasoning of the current turn and clears it at the next user message', () => {
    activate();
    store().appendUserMessage('why does login fail?');
    store().appendReasoning('s1', 'Checking the auth header. ');
    store().appendReasoning('s1', 'The token variable is empty.');
    expect(store().reasoning).toBe('Checking the auth header. The token variable is empty.');
    store().completeMessage('s1');
    expect(store().reasoning).not.toBe('');
    store().appendUserMessage('and now?');
    expect(store().reasoning).toBe('');
  });

  it('ignores reasoning for another session and empty chunks', () => {
    activate();
    store().appendUserMessage('hi');
    store().appendReasoning('other', 'secret thoughts');
    store().appendReasoning('s1', '');
    expect(store().reasoning).toBe('');
  });

  it('keeps only the newest part of a very long reasoning stream', () => {
    activate();
    store().appendUserMessage('think hard');
    store().appendReasoning('s1', 'a'.repeat(MAX_REASONING_CHARS));
    store().appendReasoning('s1', 'END');
    const text = store().reasoning;
    expect(text.length).toBeLessThanOrEqual(MAX_REASONING_CHARS + 1);
    expect(text.startsWith('…')).toBe(true);
    expect(text.endsWith('END')).toBe(true);
  });

  it('clears the reasoning on reset and when a new session starts', () => {
    activate();
    store().appendUserMessage('hi');
    store().appendReasoning('s1', 'thinking');
    store().reset();
    expect(store().reasoning).toBe('');
    activate('s2');
    expect(store().reasoning).toBe('');
  });

  it('records the time of the last activity of the active session only', () => {
    activate();
    store().appendUserMessage('hi');
    const started = store().lastActivityAt;
    expect(started).toBeTypeOf('number');
    useAssistantStore.setState({ lastActivityAt: 1 });
    store().touchActivity('other');
    expect(store().lastActivityAt).toBe(1);
    store().touchActivity('s1');
    expect(store().lastActivityAt).toBeGreaterThan(1);
  });

  it('opens and closes the panel and keeps the focus', () => {
    store().openPanel();
    store().setFocus({ collection: 'orders', path: 'get.yml' });
    expect(store().panelOpen).toBe(true);
    store().closePanel();
    expect(store().panelOpen).toBe(false);
    expect(store().focus).toEqual({ collection: 'orders', path: 'get.yml' });
  });

  it('activates the session started with the current token', () => {
    const token = store().beginSession('agent-1', 'ask');
    expect(store().session).toMatchObject({ status: 'starting', sessionId: '', mode: 'ask' });
    expect(store().activateSession(token, 's1', [])).toBe(true);
    expect(store().session).toMatchObject({ status: 'active', sessionId: 's1' });
  });

  it('refuses to activate a start that a newer start replaced', () => {
    const first = store().beginSession('agent-1', 'edit');
    const second = store().beginSession('agent-2', 'edit');
    expect(store().activateSession(first, 'old', [])).toBe(false);
    expect(store().activateSession(second, 'new', [])).toBe(true);
    expect(store().session?.sessionId).toBe('new');
  });

  it('refuses to activate a start that was ended while starting', () => {
    const token = store().beginSession('agent-1', 'edit');
    store().endSession();
    expect(store().activateSession(token, 's1', [])).toBe(false);
    expect(store().session?.status).toBe('ended');
  });

  it('records a failed start', () => {
    const token = store().beginSession('agent-1', 'edit');
    store().failStart(token, 'command not found');
    expect(store().session).toMatchObject({ status: 'error', error: 'command not found' });
  });

  it('opens one turn at a time', () => {
    activate();
    expect(store().appendUserMessage('one')).toBe(true);
    expect(store().appendUserMessage('two')).toBe(false);
    expect(store().messages.map((m) => m.kind)).toEqual(['user', 'agent']);
    expect(selectTurnRunning(store())).toBe(true);
  });

  it('does not open a turn without an active session', () => {
    expect(store().appendUserMessage('hi')).toBe(false);
    expect(store().messages).toEqual([]);
  });

  it('streams chunks into the reply and continues after a tool line', () => {
    activate();
    store().appendUserMessage('hi');
    store().appendChunk('s1', 'Let me look. ');
    store().upsertToolActivity('s1', {
      callId: 'c1',
      title: 'Reading GET /orders',
      status: 'in_progress',
    });
    store().appendChunk('s1', 'Found it.');
    expect(store().messages.map((m) => m.kind)).toEqual(['user', 'agent', 'tool', 'agent']);
    expect(lastMessage()).toMatchObject({ kind: 'agent', text: 'Found it.', streaming: true });
  });

  it('updates a tool line by call id and keeps its title', () => {
    activate();
    store().appendUserMessage('hi');
    store().upsertToolActivity('s1', { callId: 'c1', title: 'Running Login', status: 'pending' });
    store().upsertToolActivity('s1', { callId: 'c1', title: '', status: 'completed' });
    const tools = store().messages.filter((m) => m.kind === 'tool');
    expect(tools).toHaveLength(1);
    expect(tools[0]).toMatchObject({ title: 'Running Login', status: 'completed' });
  });

  it('ignores events for another session', () => {
    activate('s1');
    store().appendUserMessage('hi');
    store().appendChunk('other', 'x');
    store().upsertToolActivity('other', { callId: 'c1', title: 'Reading', status: 'pending' });
    store().setUsage('other', { used: 1, size: 2 });
    store().upsertProposal(makeProposal({ sessionId: 'other' }));
    store().completeMessage('other');
    expect(lastMessage()).toMatchObject({ kind: 'agent', text: '', streaming: true });
    expect(store().usage).toBeUndefined();
    expect(store().proposals).toEqual([]);
  });

  it('completes the turn', () => {
    activate();
    store().appendUserMessage('hi');
    store().appendChunk('s1', 'Done.');
    store().completeMessage('s1');
    expect(selectTurnRunning(store())).toBe(false);
    expect(lastMessage()).toMatchObject({ kind: 'agent', text: 'Done.', streaming: false });
  });

  it('fails the turn with the error on the last reply segment', () => {
    activate();
    store().appendUserMessage('hi');
    store().appendChunk('s1', 'partial');
    store().failMessage('s1', 'agent crashed');
    expect(store().session).toMatchObject({ status: 'error', error: 'agent crashed' });
    expect(lastMessage()).toMatchObject({
      kind: 'agent',
      text: 'partial',
      streaming: false,
      error: 'agent crashed',
    });
  });

  it('clears the focus when a fatal failure ends the session', () => {
    activate();
    store().setFocus({ collection: 'orders', path: 'get.yml' });
    store().failMessage('s1', 'busy', false);
    expect(store().focus).toEqual({ collection: 'orders', path: 'get.yml' });
    store().failMessage('s1', 'agent crashed');
    expect(store().focus).toBeUndefined();
  });

  it('clears the focus when the session ends', () => {
    activate();
    store().setFocus({ collection: 'orders', path: 'get.yml' });
    store().endSession();
    expect(store().focus).toBeUndefined();
  });

  it('keeps a focus set just before a session begins', () => {
    store().setFocus({ collection: 'orders', path: 'get.yml' });
    const token = store().beginSession('agent-1', 'ask');
    store().activateSession(token, 's1', []);
    expect(store().focus).toEqual({ collection: 'orders', path: 'get.yml' });
  });

  it('drops the proposals when a failure ends the session', () => {
    activate();
    store().upsertProposal(makeProposal());
    store().appendUserMessage('hi');
    store().failMessage('s1', 'agent crashed');
    expect(store().proposals).toEqual([]);
  });

  it('keeps the proposals on a non-fatal failure', () => {
    activate();
    store().upsertProposal(makeProposal());
    store().appendUserMessage('hi');
    store().failMessage('s1', 'busy', false);
    expect(store().proposals).toHaveLength(1);
  });

  it('keeps the session active on a non-fatal failure', () => {
    activate();
    store().appendUserMessage('hi');
    store().failMessage('s1', 'busy', false);
    expect(store().session?.status).toBe('active');
    expect(lastMessage()).toMatchObject({ streaming: false, error: 'busy' });
    expect(store().appendUserMessage('again')).toBe(true);
  });

  it('does not downgrade a resolved proposal back to pending', () => {
    activate();
    store().upsertProposal(makeProposal());
    store().resolveProposal('s1', 'p1', 'accepted');
    store().upsertProposal(makeProposal());
    expect(store().proposals[0].status).toBe('accepted');
  });

  it('ends the session, discards proposals and adds the notice', () => {
    activate();
    store().appendUserMessage('hi');
    store().upsertProposal(makeProposal());
    store().endSession('Workspace changed.');
    expect(store().session?.status).toBe('ended');
    expect(store().proposals).toEqual([]);
    expect(selectTurnRunning(store())).toBe(false);
    expect(lastMessage()).toMatchObject({ kind: 'notice', text: 'Workspace changed.' });
  });

  it('anchors a new proposal to the last message and keeps it on update', () => {
    activate();
    store().appendUserMessage('hi');
    const lastId = store().messages[store().messages.length - 1].id;
    store().upsertProposal(makeProposal());
    store().appendChunk('s1', 'more');
    store().upsertProposal(makeProposal({ summary: 'Changed' }));
    expect(store().proposalAnchors).toEqual({ p1: lastId });
  });

  it('anchors a proposal with no messages to the start', () => {
    activate();
    store().upsertProposal(makeProposal());
    expect(store().proposalAnchors).toEqual({ p1: null });
  });

  it('clears the anchors with the proposals on fatal failure and reset', () => {
    activate();
    store().upsertProposal(makeProposal());
    store().failMessage('s1', 'boom', true);
    expect(store().proposalAnchors).toEqual({});
    store().reset();
    expect(store().proposalAnchors).toEqual({});
  });

  it('upserts proposals by id and resolves their status', () => {
    activate();
    store().upsertProposal(makeProposal());
    store().upsertProposal(makeProposal({ summary: 'Changed summary' }));
    expect(store().proposals).toHaveLength(1);
    expect(store().proposals[0].summary).toBe('Changed summary');
    store().resolveProposal('s1', 'p1', 'rejected');
    expect(store().proposals[0].status).toBe('rejected');
  });

  it('stores config options and usage for the active session', () => {
    activate();
    store().setConfigOptions('s1', [
      { id: 'model', name: 'Model', category: 'model', currentValue: 'opus', choices: [] },
    ]);
    store().setUsage('s1', { used: 1200, size: 200000, costUsd: 0.02 });
    expect(store().session?.configOptions[0].currentValue).toBe('opus');
    expect(store().usage).toEqual({ used: 1200, size: 200000, costUsd: 0.02 });
  });

  it('starts a new session with an empty conversation', () => {
    activate();
    store().appendUserMessage('hi');
    store().endSession();
    store().beginSession('agent-1', 'edit');
    expect(store().messages).toEqual([]);
    expect(store().usage).toBeUndefined();
  });

  describe('permissions', () => {
    const request = (requestId: string) => ({
      requestId,
      callId: 'call-1',
      title: 'propose_changes',
      kind: 'other',
      summary: '{}',
      options: [
        {
          id: 'proceed_always_server',
          name: 'Allow all server tools for this session',
          kind: 'allow_always' as const,
        },
        { id: 'cancel', name: 'Deny', kind: 'reject_once' as const },
      ],
    });

    it('adds a pending request for the current session only', () => {
      activate('s1');
      store().addPermission('other', request('perm-1'));
      expect(store().permissions).toEqual([]);
      store().addPermission('s1', request('perm-1'));
      expect(store().permissions).toHaveLength(1);
      expect(store().permissions[0]).toMatchObject({ requestId: 'perm-1', status: 'pending' });
    });

    it('keeps parallel requests apart and resolves each by id', () => {
      activate();
      store().addPermission('s1', request('perm-1'));
      store().addPermission('s1', request('perm-2'));
      store().resolvePermission('s1', 'perm-2', 'cancel');
      const [first, second] = store().permissions;
      expect(first.status).toBe('pending');
      expect(second).toMatchObject({ status: 'answered', chosenName: 'Deny' });
    });

    it('shows a cancelled request as cancelled', () => {
      activate();
      store().addPermission('s1', request('perm-1'));
      store().resolvePermission('s1', 'perm-1', null);
      expect(store().permissions[0]).toMatchObject({ status: 'cancelled' });
      expect(store().permissions[0].chosenName).toBeUndefined();
    });

    it('ignores a duplicate request id', () => {
      activate();
      store().addPermission('s1', request('perm-1'));
      store().addPermission('s1', request('perm-1'));
      expect(store().permissions).toHaveLength(1);
    });

    it('cancels pending requests when the turn finishes or fails', () => {
      activate();
      store().appendUserMessage('hi');
      store().addPermission('s1', request('perm-1'));
      store().completeMessage('s1');
      expect(store().permissions[0].status).toBe('cancelled');

      store().appendUserMessage('again');
      store().addPermission('s1', request('perm-2'));
      store().failMessage('s1', 'boom', false);
      expect(store().permissions[1].status).toBe('cancelled');
    });

    it('does not reopen an answered request when the turn finishes', () => {
      activate();
      store().appendUserMessage('hi');
      store().addPermission('s1', request('perm-1'));
      store().resolvePermission('s1', 'perm-1', 'proceed_always_server');
      store().completeMessage('s1');
      expect(store().permissions[0].status).toBe('answered');
    });

    it('clears the requests when the session ends, fails fatally or is reset', () => {
      activate();
      store().addPermission('s1', request('perm-1'));
      store().endSession();
      expect(store().permissions).toEqual([]);

      activate('s2');
      store().addPermission('s2', request('perm-2'));
      store().failMessage('s2', 'dead', true);
      expect(store().permissions).toEqual([]);

      activate('s3');
      store().addPermission('s3', request('perm-3'));
      store().reset();
      expect(store().permissions).toEqual([]);
    });
  });
});
