import { describe, expect, it } from 'vitest';
import { newEntityId, newNodeId } from '../flow-ids';

describe('flow ids', () => {
  it('prefixes node ids and never repeats one', () => {
    const ids = new Set(Array.from({ length: 50 }, () => newNodeId('input')));
    expect(ids.size).toBe(50);
    for (const id of ids) expect(id.startsWith('input-')).toBe(true);
  });

  it('makes unique entity ids', () => {
    expect(newEntityId()).not.toBe(newEntityId());
  });
});
