// Which OAuth2 settings a fetched token belongs to. A token is only valid for the config it
// was fetched with, so callers drop it once that config changes.
import type { AuthState } from '@/types/pane-types';

type OAuth2State = NonNullable<AuthState['oauth2']>;

/**
 * Whether two OAuth2 configs would mint the same token: grant type, token URL and client id.
 * The implicit grant gets its token from the authorization URL, so that is compared too.
 */
export function sameOAuth2TokenConfig(x: OAuth2State, y: OAuth2State): boolean {
  if (x.grantType !== y.grantType || x.tokenUrl !== y.tokenUrl || x.clientId !== y.clientId) {
    return false;
  }
  return x.grantType !== 'implicit' || x.authorizationUrl === y.authorizationUrl;
}

/** The config with every fetched token field cleared. */
export function withoutOAuth2Tokens(o: OAuth2State): OAuth2State {
  return {
    ...o,
    accessToken: '',
    refreshToken: '',
    expiresIn: null,
    tokenAcquiredAt: null,
    idToken: '',
    tokenType: '',
    responseScope: '',
    idTokenClaims: null,
    accessTokenClaims: null,
  };
}
