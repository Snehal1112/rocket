import { describe, expect, it } from 'vitest';
import {
  isValidMethodToken,
  normalizeMethod,
  STANDARD_METHODS,
  withCurrentMethod,
} from '@/lib/method-options';

describe('method-options', () => {
  it('lists the nine standard methods', () => {
    expect(STANDARD_METHODS).toEqual([
      'GET',
      'POST',
      'PUT',
      'PATCH',
      'DELETE',
      'OPTIONS',
      'HEAD',
      'TRACE',
      'CONNECT',
    ]);
  });

  it('accepts HTTP tokens and rejects everything else', () => {
    expect(isValidMethodToken('PURGE')).toBe(true);
    expect(isValidMethodToken('M-SEARCH')).toBe(true);
    for (const bad of ['', 'GET ME', 'A/B', 'BAD\n', 'café', 'A'.repeat(65)]) {
      expect(isValidMethodToken(bad)).toBe(false);
    }
  });

  it('upper-cases standard names, keeps custom tokens as typed and returns null for junk', () => {
    expect(normalizeMethod(' trace ')).toBe('TRACE');
    expect(normalizeMethod('Purge')).toBe('Purge');
    expect(normalizeMethod('bad method')).toBeNull();
    expect(normalizeMethod('   ')).toBeNull();
  });

  it('adds the current method to the options only when it is not already there', () => {
    expect(withCurrentMethod(STANDARD_METHODS, 'GET')).toBe(STANDARD_METHODS);
    expect(withCurrentMethod(['GET'], 'PURGE')).toEqual(['GET', 'PURGE']);
  });
});
