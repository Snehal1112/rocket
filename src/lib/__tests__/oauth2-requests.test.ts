import { describe, expect, it } from 'vitest';
import { buildGetTokenRequest, buildRefreshRequest } from '@/lib/oauth2-requests';
import { fromPersistedAuth } from '@/lib/persisted-auth';
import type { Auth } from '@/lib/tauri-api';

function oauthState(flow = 'authorization_code') {
  const state = fromPersistedAuth({
    authType: 'o-auth2',
    flow,
    authorizationUrl: 'https://idp.example.com/authorize',
    accessTokenUrl: 'https://idp.example.com/token',
    refreshTokenUrl: 'https://idp.example.com/refresh',
    callbackUrl: 'https://app.example.com/cb',
    credentials: { clientId: '{{cid}}', clientSecret: 's3cret', placement: 'basic_auth_header' },
    scope: 'read',
  } as unknown as Auth);
  if (!state.oauth2) throw new Error('expected an oauth2 state');
  return state.oauth2;
}

const rv = (s: string) => s.replace('{{cid}}', 'resolved-cid');
const target = { collection: 'api', environmentName: 'dev', requestPath: undefined };

describe('buildGetTokenRequest', () => {
  it('maps the OAuth2 state to a get-token request with variables resolved', () => {
    const request = buildGetTokenRequest(oauthState(), rv, target);
    expect(request).toMatchObject({
      grantType: 'authorization_code',
      authorizationUrl: 'https://idp.example.com/authorize',
      tokenUrl: 'https://idp.example.com/token',
      callbackUrl: 'https://app.example.com/cb',
      clientId: 'resolved-cid',
      clientSecret: 's3cret',
      scope: 'read',
      clientAuthentication: 'header',
      collection: 'api',
      environmentName: 'dev',
    });
    expect(request.forceReauth).toBeUndefined();
  });

  it('sets forceReauth only when asked', () => {
    expect(buildGetTokenRequest(oauthState(), rv, target, { forceReauth: true }).forceReauth).toBe(
      true,
    );
  });

  it('turns empty optional fields into undefined', () => {
    const oauth = { ...oauthState(), scope: '', state: '', clientSecret: '' };
    const request = buildGetTokenRequest(oauth, rv, target);
    expect(request.scope).toBeUndefined();
    expect(request.state).toBeUndefined();
    expect(request.clientSecret).toBeUndefined();
  });
});

describe('buildRefreshRequest', () => {
  it('maps the OAuth2 state to a refresh request with variables resolved', () => {
    const oauth = { ...oauthState(), refreshToken: 'ref-{{cid}}' };
    const request = buildRefreshRequest(oauth, rv, target);
    expect(request).toMatchObject({
      refreshToken: 'ref-resolved-cid',
      tokenUrl: 'https://idp.example.com/token',
      refreshTokenUrl: 'https://idp.example.com/refresh',
      clientId: 'resolved-cid',
      clientAuthentication: 'header',
      collection: 'api',
      environmentName: 'dev',
    });
  });
});
