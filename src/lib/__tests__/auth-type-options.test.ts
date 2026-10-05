import { describe, expect, it } from 'vitest';
import { NTLM_OPTION, OAUTH1_OPTION, withCurrentAuthType } from '@/lib/auth-type-options';

const base = [
  { label: 'None', value: 'none' as const },
  { label: 'Basic', value: 'basic' as const },
];

describe('withCurrentAuthType', () => {
  it('has no read-only extras left: every auth type is a normal option', () => {
    for (const current of ['ntlm', 'oauth1', 'digest', 'basic'] as const) {
      expect(withCurrentAuthType(base, current)).toBe(base);
    }
  });

  it('exports the shared options with their labels', () => {
    expect(NTLM_OPTION).toEqual({ label: 'NTLM', value: 'ntlm' });
    expect(OAUTH1_OPTION).toEqual({ label: 'OAuth 1.0', value: 'oauth1' });
  });
});
