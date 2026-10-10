import { describe, expect, it } from 'vitest';
import type { ConversationRecord, StoredMessage } from '@/lib/tauri-api';
import {
  buildReplayPreface,
  DEFAULT_REPLAY_LIMITS,
  REPLAY_CLOSE,
  REPLAY_DROPPED_MARKER,
  REPLAY_OPEN,
  REPLAY_PREFACE_HEADER,
  replaySizeHint,
} from '../replay-preface';

function record(messages: StoredMessage[]): ConversationRecord {
  return {
    id: '0b6f3f4e-0000-4000-8000-000000000001',
    title: 'Why 401?',
    agentConfigId: 'agent-1',
    mode: 'agent',
    workspaceId: 'ws-1',
    createdAt: '2026-10-11T09:12:00Z',
    updatedAt: '2026-10-11T09:20:41Z',
    collections: [],
    messages,
  };
}

describe('buildReplayPreface', () => {
  it('lists the conversation oldest first, one line per tool or proposal, without notices', () => {
    const preface = buildReplayPreface(
      record([
        { kind: 'user', text: 'Why 401?' },
        { kind: 'tool', title: 'get_request:\n  login', status: 'completed' },
        { kind: 'notice', text: 'Session ended.' },
        { kind: 'proposal', summary: 'Refresh  the token', status: 'rejected' },
        { kind: 'agent', text: 'The token expired.' },
      ]),
    );
    expect(preface).toBe(
      [
        REPLAY_PREFACE_HEADER,
        REPLAY_OPEN,
        'User: Why 401?',
        'Tool call: get_request: login (completed)',
        'Proposed change: Refresh the token (rejected)',
        'Assistant: The token expired.',
        REPLAY_CLOSE,
      ].join('\n'),
    );
  });

  it('keeps the newest messages that fit and marks the dropped ones', () => {
    const messages: StoredMessage[] = Array.from({ length: 50 }, (_, i) => ({
      kind: 'user',
      text: `message ${i} ${'x'.repeat(40)}`,
    }));
    const preface = buildReplayPreface(record(messages), { maxChars: 1_000 });
    expect(preface.length).toBeLessThanOrEqual(1_000);
    expect(preface).toContain(REPLAY_DROPPED_MARKER);
    expect(preface).not.toContain('message 0 ');
    const numbers = preface
      .split('\n')
      .filter((line) => line.startsWith('User: '))
      .map((line) => Number(line.split(' ')[2]));
    expect(numbers[numbers.length - 1]).toBe(49);
    expect(numbers).toEqual([...numbers].sort((a, b) => a - b));
  });

  it('adds no marker when everything fits', () => {
    expect(buildReplayPreface(record([{ kind: 'user', text: 'hi' }]))).not.toContain(
      REPLAY_DROPPED_MARKER,
    );
  });

  it('keeps the end of a newest message longer than the limit', () => {
    const preface = buildReplayPreface(
      record([{ kind: 'agent', text: `${'a'.repeat(5_000)}END` }]),
      { maxChars: 500 },
    );
    expect(preface.length).toBeLessThanOrEqual(500);
    expect(preface).toContain('…');
    expect(preface).toContain('END');
  });

  it('never includes tool results', () => {
    const withResult = {
      kind: 'tool',
      title: 'run_request: login',
      status: 'completed',
      result: 'SECRET-BODY',
    } as unknown as StoredMessage;
    const preface = buildReplayPreface(record([withResult]));
    expect(preface).toContain('Tool call: run_request: login (completed)');
    expect(preface).not.toContain('SECRET-BODY');
  });

  it('uses a 12,000 character limit by default', () => {
    expect(DEFAULT_REPLAY_LIMITS.maxChars).toBe(12_000);
    const messages: StoredMessage[] = Array.from({ length: 400 }, (_, i) => ({
      kind: 'agent',
      text: `reply ${i} ${'y'.repeat(100)}`,
    }));
    expect(buildReplayPreface(record(messages)).length).toBeLessThanOrEqual(12_000);
  });
});

describe('replaySizeHint', () => {
  it('names the size in characters', () => {
    expect(replaySizeHint(8412)).toBe(
      'Earlier conversation attached as context: 8,412 characters. It goes out with your first message.',
    );
  });
});
