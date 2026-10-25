import { describe, expect, it } from 'vitest';
import { storedToChatMessages } from '../stored-messages';

describe('storedToChatMessages', () => {
  it('turns every stored kind into a live message, proposals into notices', () => {
    const out = storedToChatMessages([
      { kind: 'user', text: 'hi' },
      { kind: 'agent', text: 'hello' },
      { kind: 'tool', title: 'Reading GET /a', status: 'completed' },
      { kind: 'notice', text: 'ended' },
      { kind: 'proposal', summary: 'Rename X', status: 'accepted' },
    ]);
    expect(out.map((m) => m.kind)).toEqual(['user', 'agent', 'tool', 'notice', 'notice']);
    expect(out[1]).toMatchObject({ streaming: false, text: 'hello' });
    expect(out[4]).toMatchObject({ text: 'Proposal: Rename X (accepted)' });
    expect(new Set(out.map((m) => m.id)).size).toBe(out.length);
  });
});
