import { describe, expect, it } from 'vitest';
import { fromPersistedAuth, toPersistedAuth } from '../persisted-auth';
import type { Auth } from '../tauri-api';

describe('toPersistedAuth', () => {
  it('maps none to authType none (the backend omits it on disk)', () => {
    expect(toPersistedAuth({ authType: 'none' })).toEqual({ authType: 'none' });
  });

  it('keeps inherit as authType inherit instead of collapsing it to none', () => {
    expect(toPersistedAuth({ authType: 'inherit' })).toEqual({ authType: 'inherit' });
  });

  it('maps basic auth', () => {
    expect(toPersistedAuth({ authType: 'basic', basic: { username: 'u', password: 'p' } })).toEqual(
      { authType: 'basic', username: 'u', password: 'p' },
    );
  });

  it('defaults missing basic fields to empty strings', () => {
    expect(toPersistedAuth({ authType: 'basic' })).toEqual({
      authType: 'basic',
      username: '',
      password: '',
    });
  });

  it('maps bearer auth', () => {
    expect(toPersistedAuth({ authType: 'bearer', bearer: { token: 't' } })).toEqual({
      authType: 'bearer',
      token: 't',
    });
  });

  it('maps api-key auth', () => {
    expect(
      toPersistedAuth({
        authType: 'api-key',
        apiKey: { key: 'X-Key', value: 'v', addTo: 'query' },
      }),
    ).toEqual({ authType: 'api-key', key: 'X-Key', value: 'v', placement: 'query' });
  });

  it('defaults api-key placement to header', () => {
    const result = toPersistedAuth({ authType: 'api-key' });
    expect(result).toMatchObject({ placement: 'header' });
  });

  it('maps aws-sig-v4 with all fields, omitting empty sessionToken/profileName', () => {
    expect(
      toPersistedAuth({
        authType: 'aws-sig-v4',
        awsSigV4: {
          accessKey: 'AKIA...',
          secretKey: 'secret',
          region: 'us-east-1',
          service: 'execute-api',
          sessionToken: '',
        },
      }),
    ).toEqual({
      authType: 'aws-sig-v4',
      accessKey: 'AKIA...',
      secretKey: 'secret',
      region: 'us-east-1',
      service: 'execute-api',
      sessionToken: undefined,
      profileName: undefined,
    });
  });

  it('preserves a non-empty sessionToken and profileName for aws-sig-v4', () => {
    expect(
      toPersistedAuth({
        authType: 'aws-sig-v4',
        awsSigV4: {
          accessKey: 'AKIA...',
          secretKey: 'secret',
          region: 'us-east-1',
          service: 'execute-api',
          sessionToken: 'session-abc',
          profileName: 'my-profile',
        },
      }),
    ).toMatchObject({ sessionToken: 'session-abc', profileName: 'my-profile' });
  });

  it('maps oauth2 via the shared oauth2-mapping adapter', () => {
    const result = toPersistedAuth({
      authType: 'oauth2',
      oauth2: {
        grantType: 'client_credentials',
        authorizationUrl: '',
        tokenUrl: 'https://auth.example.com/token',
        callbackUrl: '',
        clientId: 'id',
        clientSecret: 'secret',
        scope: 'read',
        state: '',
        username: '',
        password: '',
        clientAuthentication: 'header',
        headerPrefix: 'Bearer',
        addTokenTo: 'header',
        verifySsl: true,
        accessToken: '',
        refreshToken: '',
        expiresIn: null,
        tokenAcquiredAt: null,
        usePkce: false,
        useSystemBrowser: false,
        tokenSource: 'accessToken',
        tokenId: '',
        refreshTokenUrl: '',
        autoFetchToken: true,
        autoRefreshToken: false,
        authParams: [],
        tokenParams: [],
        refreshParams: [],
        idToken: '',
        tokenType: '',
        responseScope: '',
        idTokenClaims: null,
        accessTokenClaims: null,
      },
    });
    expect((result as { authType: string }).authType).toBe('o-auth2');
  });

  it('maps oauth2 with no state to none', () => {
    expect(toPersistedAuth({ authType: 'oauth2' })).toEqual({ authType: 'none' });
  });
});

describe('fromPersistedAuth', () => {
  it('falls back to none when auth is missing and no fallback given', () => {
    expect(fromPersistedAuth(undefined)).toEqual({ authType: 'none' });
    expect(fromPersistedAuth(null)).toEqual({ authType: 'none' });
  });

  it('falls back to inherit when explicitly requested (request loaded from a collection)', () => {
    expect(fromPersistedAuth(undefined, 'inherit')).toEqual({ authType: 'inherit' });
  });

  it('round-trips basic auth', () => {
    const persisted: Auth = { authType: 'basic', username: 'u', password: 'p' };
    expect(fromPersistedAuth(persisted)).toEqual({
      authType: 'basic',
      basic: { username: 'u', password: 'p' },
    });
  });

  it('round-trips bearer auth', () => {
    const persisted: Auth = { authType: 'bearer', token: 't' };
    expect(fromPersistedAuth(persisted)).toEqual({ authType: 'bearer', bearer: { token: 't' } });
  });

  it('round-trips api-key auth', () => {
    const persisted = {
      authType: 'api-key',
      key: 'X-Key',
      value: 'v',
      placement: 'query',
    } as unknown as Auth;
    expect(fromPersistedAuth(persisted)).toEqual({
      authType: 'api-key',
      apiKey: { key: 'X-Key', value: 'v', addTo: 'query' },
    });
  });

  it('round-trips aws-sig-v4 including sessionToken and profileName', () => {
    const persisted = {
      authType: 'aws-sig-v4',
      accessKey: 'AKIA...',
      secretKey: 'secret',
      region: 'us-east-1',
      service: 'execute-api',
      sessionToken: 'session-abc',
      profileName: 'my-profile',
    } as unknown as Auth;
    expect(fromPersistedAuth(persisted)).toEqual({
      authType: 'aws-sig-v4',
      awsSigV4: {
        accessKey: 'AKIA...',
        secretKey: 'secret',
        region: 'us-east-1',
        service: 'execute-api',
        sessionToken: 'session-abc',
        profileName: 'my-profile',
      },
    });
  });

  it('accepts the o-auth2 wire tag', () => {
    const persisted = {
      authType: 'o-auth2',
      flow: 'client_credentials',
      accessTokenUrl: 'https://auth.example.com/token',
      credentials: { clientId: 'id', clientSecret: 'secret' },
    } as unknown as Auth;
    expect(fromPersistedAuth(persisted).authType).toBe('oauth2');
  });

  it('defensively also accepts a bare oauth2 tag', () => {
    const persisted = {
      authType: 'oauth2',
      flow: 'client_credentials',
      accessTokenUrl: 'https://auth.example.com/token',
      credentials: { clientId: 'id', clientSecret: 'secret' },
    } as unknown as Auth;
    expect(fromPersistedAuth(persisted).authType).toBe('oauth2');
  });

  it('falls back to the given fallback for an unrecognized authType', () => {
    const persisted = { authType: 'kerberos' } as unknown as Auth;
    expect(fromPersistedAuth(persisted, 'inherit')).toEqual({ authType: 'inherit' });
  });

  it('reads an explicit inherit as inherit even where the fallback is none', () => {
    expect(fromPersistedAuth({ authType: 'inherit' } as Auth, 'none')).toEqual({
      authType: 'inherit',
    });
  });

  it('keeps mapping none to the caller fallback, so older requests saved without auth still inherit', () => {
    expect(fromPersistedAuth({ authType: 'none' }, 'inherit')).toEqual({ authType: 'inherit' });
    expect(fromPersistedAuth({ authType: 'none' }, 'none')).toEqual({ authType: 'none' });
  });
});

describe('round-trip: toPersistedAuth(fromPersistedAuth(x)) is stable', () => {
  const cases: Auth[] = [
    { authType: 'none' },
    { authType: 'inherit' } as Auth,
    { authType: 'basic', username: 'u', password: 'p' },
    { authType: 'bearer', token: 't' },
    { authType: 'digest', username: 'u', password: 'p' },
    { authType: 'wsse', username: 'u', password: 'p' },
    { authType: 'ntlm', username: 'u', password: 'p', domain: 'CORP' },
    {
      authType: 'o-auth1',
      consumerKey: 'ck',
      consumerSecret: 'cs',
      accessToken: 'at',
      accessTokenSecret: 'ats',
      signatureMethod: 'HMAC-SHA1',
      privateKey: { type: 'text', value: 'pem' },
      includeBodyHash: true,
    },
    { authType: 'api-key', key: 'k', value: 'v', placement: 'header' },
    {
      authType: 'aws-sig-v4',
      accessKey: 'a',
      secretKey: 's',
      region: 'r',
      service: 'svc',
      sessionToken: 'st',
      profileName: 'p',
    } as unknown as Auth,
  ];

  for (const persisted of cases) {
    it(`round-trips ${persisted.authType}`, () => {
      const state = fromPersistedAuth(persisted);
      expect(toPersistedAuth(state)).toEqual(persisted);
    });
  }
});

describe('digest, wsse, ntlm and oauth1 are kept, not reset to none', () => {
  it('reads digest and wsse into their own editor state', () => {
    expect(fromPersistedAuth({ authType: 'digest', username: 'u', password: 'p' })).toEqual({
      authType: 'digest',
      digest: { username: 'u', password: 'p' },
    });
    expect(fromPersistedAuth({ authType: 'wsse', username: 'u', password: 'p' })).toEqual({
      authType: 'wsse',
      wsse: { username: 'u', password: 'p' },
    });
  });

  it('reads ntlm with its domain', () => {
    expect(
      fromPersistedAuth({ authType: 'ntlm', username: 'u', password: 'p', domain: 'CORP' }),
    ).toEqual({ authType: 'ntlm', ntlm: { username: 'u', password: 'p', domain: 'CORP' } });
  });

  it('keeps every oauth1 field, including ones the UI does not know', () => {
    const persisted = {
      authType: 'o-auth1',
      consumerKey: 'ck',
      someFutureField: 42,
    } as unknown as Auth;
    const state = fromPersistedAuth(persisted);
    expect(state.authType).toBe('oauth1');
    expect(state.oauth1).toEqual({ consumerKey: 'ck', someFutureField: 42 });
    expect(toPersistedAuth(state)).toEqual(persisted);
  });

  it('defensively also accepts a bare oauth1 tag and writes the backend tag back', () => {
    const state = fromPersistedAuth({ authType: 'oauth1', consumerKey: 'ck' } as unknown as Auth);
    expect(state.authType).toBe('oauth1');
    expect(toPersistedAuth(state).authType).toBe('o-auth1');
  });

  it('saving a loaded digest request does not turn it into none', () => {
    const loaded = fromPersistedAuth({ authType: 'digest', username: 'u', password: 'p' });
    expect(toPersistedAuth(loaded)).toEqual({ authType: 'digest', username: 'u', password: 'p' });
  });
});
