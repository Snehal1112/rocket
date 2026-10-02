import type { OAuth2GetTokenRequest, OAuth2RefreshRequest } from '@/lib/tauri-api';
import type { AuthState } from '@/types/pane-types';

type OAuth2State = NonNullable<AuthState['oauth2']>;
type Resolve = (s: string) => string;

/** The variable-resolution context the backend commands use to find collection and environment values. */
export interface OAuth2Target {
  collection?: string;
  environmentName?: string;
  requestPath?: string;
}

type Params = OAuth2State['authParams'];

// Resolves `{{variables}}` in both keys and values. An empty list is sent as undefined.
const resolveParams = (params: Params, rv: Resolve) =>
  params.length ? params.map((p) => ({ ...p, key: rv(p.key), value: rv(p.value) })) : undefined;

/** Builds the `oauth2_get_token` request. The frontend pre-resolves variables because the backend only reads global environments. */
export function buildGetTokenRequest(
  oauth: OAuth2State,
  rv: Resolve,
  target: OAuth2Target,
  opts?: { forceReauth?: boolean },
): OAuth2GetTokenRequest {
  return {
    grantType: oauth.grantType,
    authorizationUrl: rv(oauth.authorizationUrl) || undefined,
    tokenUrl: rv(oauth.tokenUrl) || undefined,
    callbackUrl: rv(oauth.callbackUrl) || undefined,
    clientId: rv(oauth.clientId),
    clientSecret: oauth.clientSecret ? rv(oauth.clientSecret) : undefined,
    scope: oauth.scope ? rv(oauth.scope) : undefined,
    state: oauth.state ? rv(oauth.state) : undefined,
    username: oauth.username ? rv(oauth.username) : undefined,
    password: oauth.password ? rv(oauth.password) : undefined,
    clientAuthentication: oauth.clientAuthentication,
    usePkce: oauth.usePkce,
    useSystemBrowser: oauth.useSystemBrowser,
    verifySsl: oauth.verifySsl,
    authParams: resolveParams(oauth.authParams, rv),
    tokenParams: resolveParams(oauth.tokenParams, rv),
    refreshParams: resolveParams(oauth.refreshParams, rv),
    collection: target.collection,
    environmentName: target.environmentName,
    requestPath: target.requestPath,
    forceReauth: opts?.forceReauth || undefined,
  };
}

/** Builds the `oauth2_refresh_token` request. */
export function buildRefreshRequest(
  oauth: OAuth2State,
  rv: Resolve,
  target: OAuth2Target,
): OAuth2RefreshRequest {
  return {
    refreshToken: rv(oauth.refreshToken),
    tokenUrl: rv(oauth.tokenUrl),
    refreshTokenUrl: oauth.refreshTokenUrl ? rv(oauth.refreshTokenUrl) : undefined,
    clientId: rv(oauth.clientId),
    clientSecret: oauth.clientSecret ? rv(oauth.clientSecret) : undefined,
    scope: oauth.scope ? rv(oauth.scope) : undefined,
    clientAuthentication: oauth.clientAuthentication,
    verifySsl: oauth.verifySsl,
    refreshParams: resolveParams(oauth.refreshParams, rv),
    collection: target.collection,
    environmentName: target.environmentName,
    requestPath: target.requestPath,
  };
}
