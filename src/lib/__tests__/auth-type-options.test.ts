import { describe, expect, it } from 'vitest';
import { withCurrentAuthType } from '@/lib/auth-type-options';

const base = [
  { label: 'None', value: 'none' as const },
  { label: 'Basic', value: 'basic' as const },
];

describe('withCurrentAuthType', () => {
  it('adds ntlm only while the request already uses it', () => {
    expect(withCurrentAuthType(base, 'ntlm').map((o) => o.value)).toEqual([
      'none',
      'basic',
      'ntlm',
    ]);
  });

  it('leaves the list alone for other types', () => {
    expect(withCurrentAuthType(base, 'basic')).toBe(base);
    expect(withCurrentAuthType(base, 'digest')).toBe(base);
    expect(withCurrentAuthType(base, 'oauth1')).toBe(base);
  });
});
