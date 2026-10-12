import { describe, expect, it } from 'vitest';
import { groupToolRuns, type TimelineEntry } from '@/lib/assistant/timeline';
import type { AssistantMessage } from '@/stores/assistant-store';

const tool = (id: string): AssistantMessage => ({
  kind: 'tool',
  id,
  callId: id,
  title: `get_${id}()`,
  status: 'completed',
});
const msg = (message: AssistantMessage): TimelineEntry => ({ type: 'message', message });

describe('groupToolRuns', () => {
  it('folds consecutive tool messages into one entry with the first id', () => {
    const out = groupToolRuns([msg(tool('a')), msg(tool('b')), msg(tool('c'))]);
    expect(out).toHaveLength(1);
    const first = out[0];
    expect(first.type).toBe('tools');
    if (first.type === 'tools') {
      expect(first.id).toBe('a');
      expect(first.tools.map((t) => t.id)).toEqual(['a', 'b', 'c']);
    }
  });

  it('keeps separate runs apart when a message sits between them', () => {
    const user: AssistantMessage = { kind: 'user', id: 'u', text: 'hi' };
    const out = groupToolRuns([msg(tool('a')), msg(user), msg(tool('b')), msg(tool('c'))]);
    expect(out.map((e) => e.type)).toEqual(['tools', 'message', 'tools']);
  });

  it('splits a run at a proposal entry', () => {
    const proposal = { type: 'proposal', proposal: { id: 'p' } } as unknown as TimelineEntry;
    const out = groupToolRuns([msg(tool('a')), proposal, msg(tool('b'))]);
    expect(out.map((e) => e.type)).toEqual(['tools', 'proposal', 'tools']);
  });

  it('passes through entries without tools and handles empty input', () => {
    expect(groupToolRuns([])).toEqual([]);
    const user: AssistantMessage = { kind: 'user', id: 'u', text: 'hi' };
    expect(groupToolRuns([msg(user)])).toEqual([msg(user)]);
  });
});
