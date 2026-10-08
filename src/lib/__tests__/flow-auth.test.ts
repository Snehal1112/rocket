import { describe, expect, it } from 'vitest';
import {
  DEFAULT_AUTH_NODE_AUTH,
  describeAuth,
  flowAuthKey,
  flowAuthState,
  isInteractiveGrant,
  isOAuth2,
  isTokenExpired,
  oauth2Fingerprint,
  oauth2TokenStatus,
  pickAuthState,
  resetTokenOnConfigChange,
} from '@/lib/flow-auth';
import { fromPersistedAuth, toPersistedAuth } from '@/lib/persisted-auth';
import type { Auth } from '@/lib/tauri-api';
import type { AuthState } from '@/types/pane-types';

const oauth = (flow: string): Auth =>
  ({
    authType: 'o-auth2',
    flow,
    accessTokenUrl: 'https://idp.example.com/token',
    credentials: { clientId: 'cid', clientSecret: 'secret' },
  }) as unknown as Auth;

describe('flowAuthKey', () => {
  it('joins collection, flow and node so tokens never cross flows', () => {
    expect(flowAuthKey('api', 'login', 'n1', 'dev', 'g')).toBe('api::login::dev::g::n1');
    expect(flowAuthKey('api', 'login', 'n1', null, null)).toBe('api::login::::::n1');
    expect(flowAuthKey('api', 'login', 'n1', 'dev', null)).not.toBe(
      flowAuthKey('api', 'other', 'n1', 'dev', null),
    );
  });

  it('differs by environment, and between no environment and a named one', () => {
    expect(flowAuthKey('api', 'login', 'n1', 'prod', null)).not.toBe(
      flowAuthKey('api', 'login', 'n1', 'staging', null),
    );
    expect(flowAuthKey('api', 'login', 'n1', null, null)).not.toBe(
      flowAuthKey('api', 'login', 'n1', 'prod', null),
    );
    expect(flowAuthKey('api', 'login', 'n1', undefined, null)).toBe(
      flowAuthKey('api', 'login', 'n1', null, null),
    );
  });

  it('differs by global environment, and between no global environment and a named one', () => {
    expect(flowAuthKey('api', 'login', 'n1', 'dev', null)).not.toBe(
      flowAuthKey('api', 'login', 'n1', 'dev', 'g1'),
    );
    expect(flowAuthKey('api', 'login', 'n1', 'dev', 'g1')).not.toBe(
      flowAuthKey('api', 'login', 'n1', 'dev', 'g2'),
    );
    // null, undefined and '' all mean "no global environment".
    expect(flowAuthKey('api', 'login', 'n1', 'dev', undefined)).toBe(
      flowAuthKey('api', 'login', 'n1', 'dev', null),
    );
    expect(flowAuthKey('api', 'login', 'n1', 'dev', '')).toBe(
      flowAuthKey('api', 'login', 'n1', 'dev', null),
    );
    // The environment and the global environment never swap places.
    expect(flowAuthKey('api', 'login', 'n1', 'a', null)).not.toBe(
      flowAuthKey('api', 'login', 'n1', null, 'a'),
    );
  });
});

describe('oauth2Fingerprint', () => {
  const o2 = (patch: Partial<NonNullable<AuthState['oauth2']>> = {}) => {
    const base = fromPersistedAuth({
      authType: 'o-auth2',
      flow: 'authorization_code',
      authorizationUrl: '{{host}}/authorize',
      accessTokenUrl: '{{host}}/token',
      credentials: { clientId: '{{cid}}', clientSecret: 's' },
    } as unknown as Auth).oauth2 as NonNullable<AuthState['oauth2']>;
    return { ...base, ...patch };
  };
  const rvFor = (ctx: Record<string, string>) => (s: string) =>
    s.replace(/\{\{\s*([\w.-]+)\s*\}\}/g, (m, k: string) => (k in ctx ? ctx[k] : m));

  it('is the same for the same resolved values', () => {
    const rv = rvFor({ host: 'https://idp', cid: 'a' });
    expect(oauth2Fingerprint(o2(), rv)).toBe(oauth2Fingerprint(o2(), rv));
  });

  it('changes when a variable behind a token-relevant field resolves differently', () => {
    expect(oauth2Fingerprint(o2(), rvFor({ host: 'https://idp', cid: 'a' }))).not.toBe(
      oauth2Fingerprint(o2(), rvFor({ host: 'https://idp', cid: 'b' })),
    );
    expect(oauth2Fingerprint(o2(), rvFor({ host: 'https://prod', cid: 'a' }))).not.toBe(
      oauth2Fingerprint(o2(), rvFor({ host: 'https://staging', cid: 'a' })),
    );
  });

  it('compares resolved values, not templates', () => {
    expect(oauth2Fingerprint(o2(), rvFor({ host: 'https://idp', cid: 'a' }))).toBe(
      oauth2Fingerprint(o2({ clientId: 'a' }), rvFor({ host: 'https://idp' })),
    );
  });

  it('covers scope, credentials and additional params', () => {
    const rv = rvFor({ host: 'https://idp', cid: 'a' });
    const fp = oauth2Fingerprint(o2(), rv);
    expect(oauth2Fingerprint(o2({ scope: 'admin' }), rv)).not.toBe(fp);
    expect(oauth2Fingerprint(o2({ username: 'bob' }), rv)).not.toBe(fp);
    expect(oauth2Fingerprint(o2({ password: 'pw' }), rv)).not.toBe(fp);
    expect(oauth2Fingerprint(o2({ clientSecret: 'other' }), rv)).not.toBe(fp);
    expect(oauth2Fingerprint(o2({ tokenSource: 'idToken' }), rv)).not.toBe(fp);
    expect(
      oauth2Fingerprint(
        o2({
          tokenParams: [{ key: 'audience', value: 'x', sendIn: 'body', enabled: true }],
        } as Partial<NonNullable<AuthState['oauth2']>>),
        rv,
      ),
    ).not.toBe(fp);
  });

  it('ignores the token itself and fields that do not affect which token is fetched', () => {
    const rv = rvFor({ host: 'https://idp', cid: 'a' });
    expect(
      oauth2Fingerprint(o2({ accessToken: 'tok-1', headerPrefix: 'Token', expiresIn: 5 }), rv),
    ).toBe(oauth2Fingerprint(o2(), rv));
  });

  it('does not contain the resolved secret in clear text', () => {
    const fp = oauth2Fingerprint(o2({ clientSecret: 'super-secret-value' }), (s) => s);
    expect(fp).not.toContain('super-secret-value');
  });

  it('is stable for dynamic variables, which resolve to a new value each time', () => {
    const rv = (s: string) => s.replace(/\{\{\$randomUUID\}\}/g, () => String(Math.random()));
    const auth = o2({ scope: 'openid {{$randomUUID}}' });
    expect(oauth2Fingerprint(auth, rv)).toBe(oauth2Fingerprint(auth, rv));
  });
});

describe('flowAuthState', () => {
  const persisted = {
    authType: 'o-auth2',
    flow: 'authorization_code',
    authorizationUrl: 'https://idp/authorize',
    accessTokenUrl: 'https://idp/token',
    credentials: { clientId: '{{cid}}', clientSecret: 's' },
  } as unknown as Auth;
  const rvFor = (cid: string) => (s: string) => s.replace('{{cid}}', cid);
  const stored = (): AuthState => {
    const base = fromPersistedAuth(persisted);
    return {
      ...base,
      oauth2: {
        ...(base.oauth2 as NonNullable<AuthState['oauth2']>),
        accessToken: 'tok-123456',
        refreshToken: 'ref-123456',
        expiresIn: 3600,
        tokenAcquiredAt: 1000,
      },
    };
  };

  it('keeps the stored token while the resolved configuration matches its fingerprint', () => {
    const auth = stored();
    const fingerprint = oauth2Fingerprint(
      auth.oauth2 as NonNullable<AuthState['oauth2']>,
      rvFor('a'),
    );
    expect(flowAuthState({ auth, fingerprint }, persisted, rvFor('a'))).toBe(auth);
  });

  it('clears the token when a variable value changed since it was fetched', () => {
    const auth = stored();
    const fingerprint = oauth2Fingerprint(
      auth.oauth2 as NonNullable<AuthState['oauth2']>,
      rvFor('a'),
    );
    const result = flowAuthState({ auth, fingerprint }, persisted, rvFor('b'));
    expect(result.oauth2?.accessToken).toBe('');
    expect(result.oauth2?.refreshToken).toBe('');
    expect(result.oauth2?.expiresIn).toBeNull();
    expect(result.oauth2?.clientId).toBe('{{cid}}');
  });

  it('clears a token stored without a fingerprint', () => {
    const result = flowAuthState({ auth: stored() }, persisted, rvFor('a'));
    expect(result.oauth2?.accessToken).toBe('');
  });

  it('keeps a stored state that has no token', () => {
    const auth = fromPersistedAuth(persisted);
    expect(flowAuthState({ auth }, persisted, rvFor('a'))).toBe(auth);
  });

  it('falls back to the persisted auth when nothing is stored', () => {
    expect(flowAuthState(undefined, persisted, rvFor('a'))).toEqual(fromPersistedAuth(persisted));
  });
});

describe('describeAuth', () => {
  it('names the type and, for OAuth2, the grant', () => {
    expect(describeAuth({ authType: 'bearer', token: 't' })).toBe('Bearer');
    expect(describeAuth(oauth('client_credentials'))).toBe('OAuth 2.0 · client credentials');
    expect(describeAuth(oauth('authorization_code'))).toBe('OAuth 2.0 · authorization code');
  });
});

describe('isInteractiveGrant / isOAuth2', () => {
  it('is true only for authorization code and implicit', () => {
    expect(isInteractiveGrant(oauth('authorization_code'))).toBe(true);
    expect(isInteractiveGrant(oauth('implicit'))).toBe(true);
    expect(isInteractiveGrant(oauth('client_credentials'))).toBe(false);
    expect(isInteractiveGrant(oauth('resource_owner_password_credentials'))).toBe(false);
    expect(isInteractiveGrant({ authType: 'bearer', token: 't' })).toBe(false);
  });

  it('isOAuth2 matches only OAuth2', () => {
    expect(isOAuth2(oauth('implicit'))).toBe(true);
    expect(isOAuth2({ authType: 'bearer', token: 't' })).toBe(false);
  });
});

describe('DEFAULT_AUTH_NODE_AUTH', () => {
  it('is a concrete auth type, never none or inherit', () => {
    expect(['none', 'inherit']).not.toContain(DEFAULT_AUTH_NODE_AUTH.authType);
  });
});

describe('resetTokenOnConfigChange', () => {
  const withToken = (clientId: string): AuthState => {
    const base = fromPersistedAuth({
      authType: 'o-auth2',
      flow: 'client_credentials',
      accessTokenUrl: 'https://idp.example.com/token',
      credentials: { clientId, clientSecret: 's' },
    } as unknown as Auth);
    return {
      ...base,
      oauth2: {
        ...(base.oauth2 as NonNullable<AuthState['oauth2']>),
        accessToken: 'tok-123456',
        refreshToken: 'ref-123456',
        expiresIn: 3600,
        tokenAcquiredAt: 1000,
      },
    };
  };

  it('keeps a token when only the token fields changed', () => {
    const prev = withToken('cid');
    const next = {
      ...prev,
      oauth2: { ...(prev.oauth2 as NonNullable<AuthState['oauth2']>), accessToken: 'tok-999999' },
    };
    expect(resetTokenOnConfigChange(prev, next).oauth2?.accessToken).toBe('tok-999999');
  });

  it('drops the token when the configuration changed', () => {
    const prev = withToken('cid');
    const next = withToken('other-client');
    const result = resetTokenOnConfigChange(prev, next);
    expect(result.oauth2?.accessToken).toBe('');
    expect(result.oauth2?.refreshToken).toBe('');
    expect(result.oauth2?.expiresIn).toBeNull();
    expect(result.oauth2?.tokenAcquiredAt).toBeNull();
    expect(result.oauth2?.clientId).toBe('other-client');
  });

  it('keeps a token when the header prefix is emptied (same persisted configuration)', () => {
    const prev = withToken('cid');
    const next = {
      ...prev,
      oauth2: { ...(prev.oauth2 as NonNullable<AuthState['oauth2']>), headerPrefix: '' },
    };
    expect(resetTokenOnConfigChange(prev, next).oauth2?.accessToken).toBe('tok-123456');
  });

  it('passes through when there was no previous state or no OAuth2', () => {
    const next = withToken('cid');
    expect(resetTokenOnConfigChange(undefined, next)).toBe(next);
    const basic: AuthState = { authType: 'basic', basic: { username: 'u', password: 'p' } };
    expect(resetTokenOnConfigChange(basic, basic)).toBe(basic);
  });
});

describe('isTokenExpired', () => {
  const o = (patch: Partial<NonNullable<AuthState['oauth2']>>) =>
    ({ accessToken: 'tok', expiresIn: 60, tokenAcquiredAt: 1000, ...patch }) as NonNullable<
      AuthState['oauth2']
    >;

  it('is false without a token lifetime (cannot tell, so try it)', () => {
    expect(isTokenExpired(o({ expiresIn: null }), 5000)).toBe(false);
    expect(isTokenExpired(o({ tokenAcquiredAt: null }), 5000)).toBe(false);
  });

  it('is true once acquired + expiresIn has passed, with a 30 second margin', () => {
    expect(isTokenExpired(o({}), 1000 + 29)).toBe(false);
    expect(isTokenExpired(o({}), 1000 + 31)).toBe(true);
  });
});

describe('pickAuthState', () => {
  const persisted = oauth('client_credentials');

  it('falls back to the persisted auth when nothing is stored', () => {
    expect(pickAuthState(undefined, persisted)).toEqual(fromPersistedAuth(persisted));
  });

  it('keeps the stored state, with its token, while it matches the persisted auth', () => {
    const base = fromPersistedAuth(persisted);
    const stored: AuthState = {
      ...base,
      oauth2: { ...(base.oauth2 as NonNullable<AuthState['oauth2']>), accessToken: 'tok' },
    };
    expect(pickAuthState(stored, persisted)).toBe(stored);
  });

  it('drops a stored state whose configuration no longer matches', () => {
    const stored = fromPersistedAuth({ authType: 'bearer', token: 'old' });
    expect(pickAuthState(stored, persisted)).toEqual(fromPersistedAuth(persisted));
  });
  it('matches a stored state with an empty header prefix against its own persisted form', () => {
    // The persisted mapping is not idempotent here: an empty prefix persists as
    // "Bearer", which reads back as "Bearer" and then persists with no token config.
    const base = fromPersistedAuth(oauth('authorization_code'));
    const stored: AuthState = {
      ...base,
      oauth2: {
        ...(base.oauth2 as NonNullable<AuthState['oauth2']>),
        headerPrefix: '',
        tokenSource: 'accessToken',
        tokenId: '',
        addTokenTo: 'header',
        accessToken: 'tok-123456',
      },
    };
    expect(pickAuthState(stored, toPersistedAuth(stored))).toBe(stored);
  });

  it('matches stored states against their persisted form across grants, prefixes and placements', () => {
    const grants = ['client_credentials', 'password', 'authorization_code', 'implicit'];
    for (const grant of grants)
      for (const headerPrefix of ['', 'Bearer', 'Token'])
        for (const addTokenTo of ['header', 'queryParams'] as const)
          for (const tokenSource of ['accessToken', 'idToken'] as const)
            for (const tokenId of ['', 'tid']) {
              const base = fromPersistedAuth(oauth(grant));
              const stored: AuthState = {
                ...base,
                oauth2: {
                  ...(base.oauth2 as NonNullable<AuthState['oauth2']>),
                  headerPrefix,
                  addTokenTo,
                  tokenSource,
                  tokenId,
                  accessToken: 'tok-123456',
                },
              };
              expect(
                pickAuthState(stored, toPersistedAuth(stored)),
                `${grant}/${headerPrefix}/${addTokenTo}/${tokenSource}/${tokenId}`,
              ).toBe(stored);
            }
  });
});

describe('oauth2TokenStatus', () => {
  const o = (patch: Partial<NonNullable<AuthState['oauth2']>>) =>
    ({
      accessToken: 'tok',
      idToken: '',
      refreshToken: '',
      tokenSource: 'accessToken',
      expiresIn: 3600,
      tokenAcquiredAt: 1000,
      ...patch,
    }) as NonNullable<AuthState['oauth2']>;

  it('is none without state, without a token, or with only a refresh token', () => {
    expect(oauth2TokenStatus(undefined, 1000)).toEqual({ kind: 'none' });
    expect(oauth2TokenStatus(o({ accessToken: '' }), 1000)).toEqual({ kind: 'none' });
    expect(oauth2TokenStatus(o({ accessToken: '', refreshToken: 'r' }), 1000)).toEqual({
      kind: 'none',
    });
  });

  it('is valid with the expiry time while the token has lifetime left', () => {
    expect(oauth2TokenStatus(o({}), 1100)).toEqual({ kind: 'valid', expiresAt: 4600 });
  });

  it('is valid without an expiry time when the token has no lifetime', () => {
    expect(oauth2TokenStatus(o({ expiresIn: null }), 9999)).toEqual({
      kind: 'valid',
      expiresAt: null,
    });
  });

  it('is expired once the lifetime is used up, with the 30 second margin', () => {
    expect(oauth2TokenStatus(o({}), 1000 + 3600 - 31)).toEqual({ kind: 'valid', expiresAt: 4600 });
    expect(oauth2TokenStatus(o({}), 1000 + 3600 - 29)).toEqual({ kind: 'expired' });
  });

  it('reads the ID token when the node uses it', () => {
    expect(
      oauth2TokenStatus(o({ tokenSource: 'idToken', accessToken: 'a', idToken: '' }), 1100),
    ).toEqual({ kind: 'none' });
    expect(
      oauth2TokenStatus(o({ tokenSource: 'idToken', accessToken: '', idToken: 'id' }), 1100),
    ).toEqual({ kind: 'valid', expiresAt: 4600 });
  });
});
