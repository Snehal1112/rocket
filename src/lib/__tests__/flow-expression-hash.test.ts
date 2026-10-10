import { describe, expect, it } from 'vitest';
import { expressionHash } from '../flow-expression-hash';

describe('expressionHash', () => {
  it('matches the vectors pinned in flow_trace.rs', () => {
    expect(expressionHash('response.body')).toBe('eb16adfc');
    expect(expressionHash('')).toBe('811c9dc5');
    expect(expressionHash('é')).toBe('1e9de8c1');
    expect(expressionHash('a→b')).toBe('af11e8a0');
  });

  it('always gives eight lowercase hex digits', () => {
    for (const text of ['a', 'x'.repeat(5000), '\n']) {
      expect(expressionHash(text)).toMatch(/^[0-9a-f]{8}$/);
    }
  });
});
