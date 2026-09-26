import { describe, expect, it } from 'vitest';
import type { AuthState } from '@/types/pane-types';
import { type ApiOAuth2Auth, apiAuthToOAuth2State, oauth2StateToApiAuth } from '../oauth2-mapping';

type OAuth2State = NonNullable<AuthState['oauth2']>;

function authCodeState(usePkce: boolean): OAuth2State {
  return {
    grantType: 'authorization_code',
    authorizationUrl: 'https://auth.example.com/authorize',
    tokenUrl: 'https://auth.example.com/token',
    callbackUrl: '',
    clientId: 'id',
    clientSecret: 'secret',
    scope: '',
    state: '',
    username: '',
    password: '',
    clientAuthentication: 'body',
    headerPrefix: 'Bearer',
    addTokenTo: 'header',
    verifySsl: true,
    accessToken: '',
    refreshToken: '',
    expiresIn: null,
    tokenAcquiredAt: null,
    usePkce,
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
  };
}

const authCodeApi = (pkce: ApiOAuth2Auth['pkce']): ApiOAuth2Auth => ({
  authType: 'o-auth2',
  flow: 'authorization_code',
  pkce,
});

describe('oauth2 PKCE mapping', () => {
  it('writes PKCE on as no disabled flag', () => {
    expect(oauth2StateToApiAuth(authCodeState(true)).pkce).toEqual({
      disabled: null,
      method: 'S256',
    });
  });

  it('writes PKCE off as disabled: true', () => {
    expect(oauth2StateToApiAuth(authCodeState(false)).pkce).toEqual({
      disabled: true,
      method: null,
    });
  });

  it('reads disabled: true as PKCE off', () => {
    expect(apiAuthToOAuth2State(authCodeApi({ disabled: true })).usePkce).toBe(false);
  });

  it('reads a missing or method-only pkce block as PKCE on', () => {
    expect(apiAuthToOAuth2State(authCodeApi(null)).usePkce).toBe(true);
    expect(apiAuthToOAuth2State(authCodeApi({ method: 'S256' })).usePkce).toBe(true);
  });
});
