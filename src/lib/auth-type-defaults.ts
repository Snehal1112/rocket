import type { AuthTypeOption } from '@/lib/auth-type-options';
import type { AuthState } from '@/types/pane-types';

// Types an Auth flow node can use. Never none or inherit: validation V13 rejects them.
export const AUTH_NODE_TYPE_OPTIONS: AuthTypeOption[] = [
  { label: 'Basic', value: 'basic' },
  { label: 'Digest', value: 'digest' },
  { label: 'Bearer', value: 'bearer' },
  { label: 'API Key', value: 'api-key' },
  { label: 'OAuth 2.0', value: 'oauth2' },
  { label: 'AWS Sig v4', value: 'aws-sig-v4' },
  { label: 'WSSE', value: 'wsse' },
];

/**
 * Builds the AuthState for a newly chosen type, with the same per-type defaults as
 * CollectionOverviewTab.handleAuthTypeChange. Sub-state already on `prev` is reused.
 */
export function authStateForType(authType: AuthState['authType'], prev: AuthState): AuthState {
  const next: AuthState = { authType };
  if (authType === 'basic') next.basic = prev.basic ?? { username: '', password: '' };
  if (authType === 'digest') next.digest = prev.digest ?? { username: '', password: '' };
  if (authType === 'wsse') next.wsse = prev.wsse ?? { username: '', password: '' };
  if (authType === 'bearer') next.bearer = prev.bearer ?? { token: '' };
  if (authType === 'api-key') next.apiKey = prev.apiKey ?? { key: '', value: '', addTo: 'header' };
  if (authType === 'oauth2')
    next.oauth2 = prev.oauth2 ?? {
      grantType: 'client_credentials',
      authorizationUrl: '',
      tokenUrl: '',
      callbackUrl: 'https://exchange4all.local/webapp/#oidc-callback',
      clientId: '',
      clientSecret: '',
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
      usePkce: true,
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
  if (authType === 'aws-sig-v4')
    next.awsSigV4 = prev.awsSigV4 ?? {
      accessKey: '',
      secretKey: '',
      region: '',
      service: '',
      sessionToken: '',
    };
  if (authType === 'oauth1')
    next.oauth1 = prev.oauth1 ?? { signatureMethod: 'HMAC-SHA1', placement: 'header' };
  return next;
}
