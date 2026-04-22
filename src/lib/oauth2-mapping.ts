import type { AuthState, OAuth2AdditionalParam, OAuth2JwtClaims } from '@/types/pane-types';

// Domain types as they appear over IPC (matching Rust serde output).
// These mirror `Auth::OAuth2(OAuth2Flow)` serialized as JSON.

interface ApiOAuth2Credentials {
  clientId: string;
  clientSecret: string;
  placement?: string | null;
}

interface ApiOAuth2ResourceOwner {
  username: string;
  password: string;
}

interface ApiOAuth2PKCE {
  enabled: boolean;
  method?: string | null;
}

interface ApiOAuth2AdditionalParameter {
  name: string;
  value: string;
  placement?: string | null;
}

interface ApiOAuth2AdditionalParameters {
  authorizationRequest?: ApiOAuth2AdditionalParameter[] | null;
  accessTokenRequest?: ApiOAuth2AdditionalParameter[] | null;
  refreshTokenRequest?: ApiOAuth2AdditionalParameter[] | null;
}

interface ApiOAuth2TokenPlacement {
  header?: string;
  query?: string;
}

interface ApiOAuth2TokenConfig {
  id?: string | null;
  placement?: ApiOAuth2TokenPlacement | null;
}

interface ApiOAuth2Settings {
  autoFetchToken?: boolean | null;
  autoRefreshToken?: boolean | null;
  verifySsl?: boolean | null;
}

export interface ApiOAuth2Auth {
  [key: string]: unknown;
  authType: 'o-auth2';
  flow: string;
  authorizationUrl?: string;
  accessTokenUrl?: string;
  refreshTokenUrl?: string;
  callbackUrl?: string;
  credentials?: ApiOAuth2Credentials;
  resourceOwner?: ApiOAuth2ResourceOwner;
  scope?: string | null;
  state?: string | null;
  pkce?: ApiOAuth2PKCE | null;
  additionalParameters?: ApiOAuth2AdditionalParameters | null;
  tokenConfig?: ApiOAuth2TokenConfig | null;
  settings?: ApiOAuth2Settings | null;
  clientId?: string;
}

type OAuth2State = NonNullable<AuthState['oauth2']>;

// ─── Frontend → IPC (save path) ─────────────────────────────────────

function frontendParamsToApi(params: OAuth2AdditionalParam[]): ApiOAuth2AdditionalParameter[] {
  return params
    .filter((p) => p.enabled)
    .map((p) => ({
      name: p.key,
      value: p.value,
      placement: p.sendIn === 'queryparams' ? 'query' : 'body',
    }));
}

export function oauth2StateToApiAuth(state: OAuth2State): ApiOAuth2Auth {
  const credentials: ApiOAuth2Credentials = {
    clientId: state.clientId,
    clientSecret: state.clientSecret,
    placement: state.clientAuthentication === 'header' ? 'basic_auth_header' : 'body',
  };

  const pkce: ApiOAuth2PKCE | null =
    state.grantType === 'authorization_code'
      ? { enabled: state.usePkce, method: state.usePkce ? 'S256' : null }
      : null;

  const additionalParameters: ApiOAuth2AdditionalParameters | null = (() => {
    const auth = frontendParamsToApi(state.authParams);
    const token = frontendParamsToApi(state.tokenParams);
    const refresh = frontendParamsToApi(state.refreshParams);
    if (!auth.length && !token.length && !refresh.length) return null;
    return {
      authorizationRequest: auth.length ? auth : null,
      accessTokenRequest: token.length ? token : null,
      refreshTokenRequest: refresh.length ? refresh : null,
    };
  })();

  const tokenConfig: ApiOAuth2TokenConfig | null = (() => {
    const hasId = state.tokenId.trim() !== '';
    const hasPlacement = state.addTokenTo === 'queryParams' || state.headerPrefix !== 'Bearer';
    if (!hasId && !hasPlacement) return null;
    const placement: ApiOAuth2TokenPlacement =
      state.addTokenTo === 'queryParams'
        ? { query: 'access_token' }
        : { header: state.headerPrefix || 'Bearer' };
    return {
      id: hasId ? state.tokenId : null,
      placement,
    };
  })();

  const settings: ApiOAuth2Settings = {
    autoFetchToken: state.autoFetchToken,
    autoRefreshToken: state.autoRefreshToken,
    verifySsl: state.verifySsl,
  };

  const base = {
    authType: 'o-auth2' as const,
    additionalParameters,
    tokenConfig,
    settings,
  };

  switch (state.grantType) {
    case 'client_credentials':
      return {
        ...base,
        flow: 'client_credentials',
        accessTokenUrl: state.tokenUrl,
        refreshTokenUrl: state.refreshTokenUrl || undefined,
        credentials,
        scope: state.scope || null,
      };
    case 'password':
      return {
        ...base,
        flow: 'resource_owner_password_credentials',
        accessTokenUrl: state.tokenUrl,
        refreshTokenUrl: state.refreshTokenUrl || undefined,
        credentials,
        resourceOwner: { username: state.username, password: state.password },
        scope: state.scope || null,
      };
    case 'authorization_code':
      return {
        ...base,
        flow: 'authorization_code',
        authorizationUrl: state.authorizationUrl,
        accessTokenUrl: state.tokenUrl,
        refreshTokenUrl: state.refreshTokenUrl || undefined,
        callbackUrl: state.callbackUrl || undefined,
        credentials,
        scope: state.scope || null,
        state: state.state || null,
        pkce,
      };
    case 'implicit':
      return {
        ...base,
        flow: 'implicit',
        authorizationUrl: state.authorizationUrl,
        callbackUrl: state.callbackUrl || undefined,
        clientId: state.clientId,
        scope: state.scope || null,
        state: state.state || null,
      };
    default:
      return { ...base, flow: 'client_credentials' };
  }
}

// ─── IPC → Frontend (load path) ─────────────────────────────────────

function apiParamsToFrontend(
  params?: ApiOAuth2AdditionalParameter[] | null,
): OAuth2AdditionalParam[] {
  if (!params) return [];
  return params.map((p) => ({
    key: p.name,
    value: p.value,
    sendIn: p.placement === 'query' ? 'queryparams' : 'body',
    enabled: true,
  }));
}

export function apiAuthToOAuth2State(auth: ApiOAuth2Auth): OAuth2State {
  const creds = auth.credentials;
  const settings = auth.settings;
  const tc = auth.tokenConfig;
  const ap = auth.additionalParameters;

  const base = {
    clientId: creds?.clientId ?? auth.clientId ?? '',
    clientSecret: creds?.clientSecret ?? '',
    clientAuthentication: (creds?.placement === 'basic_auth_header' ? 'header' : 'body') as
      | 'header'
      | 'body',
    scope: auth.scope ?? '',
    state: auth.state ?? '',
    usePkce: auth.pkce?.enabled ?? true,
    useSystemBrowser: false,
    tokenSource: 'accessToken' as const,
    tokenId: tc?.id ?? '',
    headerPrefix: tc?.placement?.header ?? 'Bearer',
    addTokenTo: (tc?.placement?.query ? 'queryParams' : 'header') as 'header' | 'queryParams',
    refreshTokenUrl: auth.refreshTokenUrl ?? '',
    autoFetchToken: settings?.autoFetchToken ?? true,
    autoRefreshToken: settings?.autoRefreshToken ?? false,
    verifySsl: settings?.verifySsl ?? true,
    authParams: apiParamsToFrontend(ap?.authorizationRequest),
    tokenParams: apiParamsToFrontend(ap?.accessTokenRequest),
    refreshParams: apiParamsToFrontend(ap?.refreshTokenRequest),
    accessToken: '',
    refreshToken: '',
    expiresIn: null,
    tokenAcquiredAt: null,
    idToken: '',
    tokenType: '',
    responseScope: '',
    idTokenClaims: null as OAuth2JwtClaims | null,
  };

  switch (auth.flow) {
    case 'client_credentials':
      return {
        ...base,
        grantType: 'client_credentials',
        authorizationUrl: '',
        tokenUrl: auth.accessTokenUrl ?? '',
        callbackUrl: '',
        username: '',
        password: '',
      };
    case 'resource_owner_password_credentials':
      return {
        ...base,
        grantType: 'password',
        authorizationUrl: '',
        tokenUrl: auth.accessTokenUrl ?? '',
        callbackUrl: '',
        username: auth.resourceOwner?.username ?? '',
        password: auth.resourceOwner?.password ?? '',
      };
    case 'authorization_code':
      return {
        ...base,
        grantType: 'authorization_code',
        authorizationUrl: auth.authorizationUrl ?? '',
        tokenUrl: auth.accessTokenUrl ?? '',
        callbackUrl: auth.callbackUrl ?? '',
        username: '',
        password: '',
      };
    case 'implicit':
      return {
        ...base,
        grantType: 'implicit',
        authorizationUrl: auth.authorizationUrl ?? '',
        tokenUrl: '',
        callbackUrl: auth.callbackUrl ?? '',
        username: '',
        password: '',
      };
    default:
      return {
        ...base,
        grantType: 'client_credentials',
        authorizationUrl: '',
        tokenUrl: '',
        callbackUrl: '',
        username: '',
        password: '',
      };
  }
}
