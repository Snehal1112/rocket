import { describe, expect, it } from 'vitest';
import {
  DEFAULT_AUTH_NODE_AUTH,
  describeAuth,
  flowAuthKey,
  isInteractiveGrant,
  isOAuth2,
  isTokenExpired,
  resetTokenOnConfigChange,
} from '@/lib/flow-auth';
import { fromPersistedAuth } from '@/lib/persisted-auth';
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
    expect(flowAuthKey('api', 'login', 'n1')).toBe('api::login::n1');
    expect(flowAuthKey('api', 'login', 'n1')).not.toBe(flowAuthKey('api', 'other', 'n1'));
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
