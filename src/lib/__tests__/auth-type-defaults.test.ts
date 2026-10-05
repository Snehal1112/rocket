import { describe, expect, it } from 'vitest';
import { AUTH_NODE_TYPE_OPTIONS, authStateForType } from '@/lib/auth-type-defaults';

describe('authStateForType', () => {
  const none = { authType: 'none' } as const;

  it('defaults oauth1 to HMAC-SHA1 with the header placement and keeps existing fields', () => {
    expect(authStateForType('oauth1', none).oauth1).toEqual({
      signatureMethod: 'HMAC-SHA1',
      placement: 'header',
    });
    const prev = { authType: 'oauth1', oauth1: { consumerKey: 'ck', extra: 1 } } as const;
    expect(authStateForType('oauth1', prev).oauth1).toEqual({ consumerKey: 'ck', extra: 1 });
  });

  it('gives each type its sub-state', () => {
    expect(authStateForType('basic', none).basic).toEqual({ username: '', password: '' });
    expect(authStateForType('digest', none).digest).toEqual({ username: '', password: '' });
    expect(authStateForType('wsse', none).wsse).toEqual({ username: '', password: '' });
    expect(authStateForType('bearer', none).bearer).toEqual({ token: '' });
    expect(authStateForType('api-key', none).apiKey).toEqual({
      key: '',
      value: '',
      addTo: 'header',
    });
    expect(authStateForType('aws-sig-v4', none).awsSigV4).toEqual({
      accessKey: '',
      secretKey: '',
      region: '',
      service: '',
      sessionToken: '',
    });
  });

  it('defaults oauth2 to client credentials with auto fetch', () => {
    const s = authStateForType('oauth2', none);
    expect(s.authType).toBe('oauth2');
    expect(s.oauth2?.grantType).toBe('client_credentials');
    expect(s.oauth2?.autoFetchToken).toBe(true);
    expect(s.oauth2?.callbackUrl).toBe('https://exchange4all.local/webapp/#oidc-callback');
  });

  it('keeps sub-state already on prev and drops other types', () => {
    const prev = { authType: 'bearer', bearer: { token: 'abc' } } as const;
    expect(authStateForType('bearer', prev).bearer).toEqual({ token: 'abc' });
    const back = authStateForType('basic', prev);
    expect(back.bearer).toBeUndefined();
  });

  it('defaults ntlm to empty credentials and keeps existing ones', () => {
    expect(authStateForType('ntlm', none).ntlm).toEqual({ username: '', password: '', domain: '' });
    const prev = { authType: 'ntlm', ntlm: { username: 'u', password: 'p', domain: 'D' } } as const;
    expect(authStateForType('ntlm', prev).ntlm).toEqual({
      username: 'u',
      password: 'p',
      domain: 'D',
    });
  });
});

describe('AUTH_NODE_TYPE_OPTIONS', () => {
  it('excludes none and inherit', () => {
    const values: string[] = AUTH_NODE_TYPE_OPTIONS.map((o) => o.value);
    expect(values).not.toContain('none');
    expect(values).not.toContain('inherit');
    expect(values).toContain('oauth2');
    expect(values).toHaveLength(7);
  });
});
