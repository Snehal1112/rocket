import { describe, expect, it } from 'vitest';
import { isSensitiveHeader, REDACTED_VALUE } from '../sensitive-headers';

describe('isSensitiveHeader', () => {
  it('matches the backend list whatever the case', () => {
    for (const name of [
      'Authorization',
      'proxy-authorization',
      'COOKIE',
      'Set-Cookie',
      'X-Api-Key',
    ]) {
      expect(isSensitiveHeader(name)).toBe(true);
    }
  });

  it('leaves other headers alone', () => {
    expect(isSensitiveHeader('Content-Type')).toBe(false);
    expect(isSensitiveHeader('X-Api-Keys')).toBe(false);
  });

  it('uses the backend redaction marker', () => {
    expect(REDACTED_VALUE).toBe('••••••');
  });
});
