import { describe, expect, it } from 'vitest';
import { withCurrentAuthType } from '@/lib/auth-type-options';

const base = [
  { label: 'None', value: 'none' as const },
  { label: 'Basic', value: 'basic' as const },
];

describe('withCurrentAuthType', () => {
  it('adds ntlm and oauth1 only while the request already uses them', () => {
    expect(withCurrentAuthType(base, 'ntlm').map((o) => o.value)).toEqual([
      'none',
      'basic',
      'ntlm',
    ]);
    expect(withCurrentAuthType(base, 'oauth1').map((o) => o.label)).toContain('OAuth 1.0');
  });

  it('leaves the list alone for other types', () => {
    expect(withCurrentAuthType(base, 'basic')).toBe(base);
    expect(withCurrentAuthType(base, 'digest')).toBe(base);
  });
});
