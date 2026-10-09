import { describe, expect, it } from 'vitest';
import type { ReferenceItem } from '@/lib/assistant/types';
import { addChip, chipKey, type ComposerChip, MAX_CHIPS, removeChip } from '../chips';

const item = (n: number): ReferenceItem => ({
  kind: 'request',
  collection: 'shop',
  path: `r${n}.yml`,
  label: `GET r${n}`,
});

describe('chips', () => {
  it('adds a new chip', () => {
    const result = addChip([], item(1));
    expect(result.outcome).toBe('added');
    expect(result.chips).toEqual([{ key: chipKey(item(1)), item: item(1), focus: false }]);
  });

  it('refuses a duplicate', () => {
    const first = addChip([], item(1)).chips;
    const result = addChip(first, item(1));
    expect(result.outcome).toBe('duplicate');
    expect(result.chips).toBe(first);
  });

  it('refuses a ninth chip', () => {
    let chips: ComposerChip[] = [];
    for (let i = 0; i < MAX_CHIPS; i++) chips = addChip(chips, item(i)).chips;
    expect(chips).toHaveLength(MAX_CHIPS);
    const result = addChip(chips, item(99));
    expect(result.outcome).toBe('limit');
    expect(result.chips).toHaveLength(MAX_CHIPS);
  });

  it('tells a request apart from its last response', () => {
    expect(chipKey(item(1))).not.toBe(chipKey({ ...item(1), kind: 'last-response' }));
  });

  it('keeps two items apart even when a name holds a colon', () => {
    const a: ReferenceItem = { kind: 'request', collection: 'a:b', path: 'c', label: 'x' };
    const b: ReferenceItem = { kind: 'request', collection: 'a', path: 'b:c', label: 'x' };
    expect(chipKey(a)).not.toBe(chipKey(b));
    expect(addChip(addChip([], a).chips, b).outcome).toBe('added');
  });

  it('removes a chip by key', () => {
    const chips = addChip(addChip([], item(1)).chips, item(2)).chips;
    expect(removeChip(chips, chipKey(item(1))).map((c) => c.item)).toEqual([item(2)]);
  });
});
