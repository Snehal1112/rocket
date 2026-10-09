import { beforeEach, describe, expect, it } from 'vitest';
import { makeProposal } from '@/test/assistant-fixtures';
import { type AssistantMessage, selectTurnRunning, useAssistantStore } from '../assistant-store';

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
});
