import { describe, expect, it } from 'vitest';
import { toPersistedHeaders } from '../persisted-headers';

describe('toPersistedHeaders', () => {
  it('drops rows with a blank key', () => {
    expect(
      toPersistedHeaders([
        { key: '', value: 'draft-value', enabled: true },
        { key: 'X-Real', value: 'v', enabled: true },
      ]),
    ).toEqual([{ key: 'X-Real', value: 'v', enabled: true }]);
  });

  it('preserves disabled rows instead of dropping them', () => {
    expect(toPersistedHeaders([{ key: 'X-Off', value: 'v', enabled: false }])).toEqual([
      { key: 'X-Off', value: 'v', enabled: false },
    ]);
  });

  it('preserves a blank-key row only if it is also enabled=false and has a key — i.e. never', () => {
    // A blank-key disabled row is still a draft row and should still be dropped.
    expect(toPersistedHeaders([{ key: '', value: '', enabled: false }])).toEqual([]);
  });

  it('preserves both enabled and disabled real rows, in order', () => {
    expect(
      toPersistedHeaders([
        { key: 'A', value: '1', enabled: true },
        { key: 'B', value: '2', enabled: false },
        { key: 'C', value: '3', enabled: true },
      ]),
    ).toEqual([
      { key: 'A', value: '1', enabled: true },
      { key: 'B', value: '2', enabled: false },
      { key: 'C', value: '3', enabled: true },
    ]);
  });

  it('handles an empty list', () => {
    expect(toPersistedHeaders([])).toEqual([]);
  });

  it('drops any extra fields on the row (e.g. a UI-only id)', () => {
    expect(
      toPersistedHeaders([{ id: 'row-1', key: 'X', value: 'v', enabled: true } as never]),
    ).toEqual([{ key: 'X', value: 'v', enabled: true }]);
  });
});
