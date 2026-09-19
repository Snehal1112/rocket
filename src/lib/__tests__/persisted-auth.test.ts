import { describe, expect, it } from 'vitest';
import { fromPersistedAuth, toPersistedAuth } from '../persisted-auth';
import type { Auth } from '../tauri-api';

describe('toPersistedAuth', () => {
  it('maps none and inherit to authType none', () => {
    expect(toPersistedAuth({ authType: 'none' })).toEqual({ authType: 'none' });
    expect(toPersistedAuth({ authType: 'inherit' })).toEqual({ authType: 'none' });
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
    const persisted = { authType: 'wsse' } as unknown as Auth;
    expect(fromPersistedAuth(persisted, 'inherit')).toEqual({ authType: 'inherit' });
  });
});

describe('round-trip: toPersistedAuth(fromPersistedAuth(x)) is stable', () => {
  const cases: Auth[] = [
    { authType: 'none' },
    { authType: 'basic', username: 'u', password: 'p' },
    { authType: 'bearer', token: 't' },
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
