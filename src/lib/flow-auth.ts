import { toPersistedAuth } from '@/lib/persisted-auth';
import type { Auth } from '@/lib/tauri-api';
import type { AuthState } from '@/types/pane-types';

type OAuth2State = NonNullable<AuthState['oauth2']>;

/** Seconds before expiry at which a token already counts as expired. */
const EXPIRY_MARGIN_SECONDS = 30;

/** The auth a new Auth node starts with. Never `none` or `inherit`. */
export const DEFAULT_AUTH_NODE_AUTH: Auth = { authType: 'bearer', token: '' };

const TYPE_LABELS: Record<string, string> = {
  basic: 'Basic',
  bearer: 'Bearer',
  'api-key': 'API key',
  'o-auth2': 'OAuth 2.0',
  'o-auth1': 'OAuth 1.0',
  'aws-sig-v4': 'AWS Signature',
  digest: 'Digest',
  wsse: 'WSSE',
  ntlm: 'NTLM',
};

const asRecord = (auth: Auth) => auth as unknown as Record<string, unknown>;

/** Key of one Auth node's in-memory state. Includes the flow, so a duplicated flow never shares tokens. */
export function flowAuthKey(collection: string, flowName: string, nodeId: string): string {
  return `${collection}::${flowName}::${nodeId}`;
}

export function isOAuth2(auth: Auth): boolean {
  return asRecord(auth).authType === 'o-auth2';
}

/** True for the grants that need a browser sign-in: authorization code and implicit. */
export function isInteractiveGrant(auth: Auth): boolean {
  if (!isOAuth2(auth)) return false;
  const flow = asRecord(auth).flow;
  return flow === 'authorization_code' || flow === 'implicit';
}

/** A short summary of an auth, such as "OAuth 2.0 · client credentials". */
export function describeAuth(auth: Auth): string {
  const record = asRecord(auth);
  const type = String(record.authType);
  const label = TYPE_LABELS[type] ?? type;
  if (type === 'o-auth2' && typeof record.flow === 'string') {
    return `${label} · ${record.flow.replace(/_/g, ' ')}`;
  }
  return label;
}

const EMPTY_TOKEN = {
  accessToken: '',
  refreshToken: '',
  expiresIn: null,
  tokenAcquiredAt: null,
  idToken: '',
  tokenType: '',
  responseScope: '',
  idTokenClaims: null,
  accessTokenClaims: null,
} as const;

/**
 * Returns `next`, with any fetched OAuth2 token cleared when the persisted
 * configuration differs from `prev`. A token fetched for one client id, URL or
 * scope must never be reused after the user edits those.
 */
export function resetTokenOnConfigChange(prev: AuthState | undefined, next: AuthState): AuthState {
  if (!prev || !next.oauth2) return next;
  const same = JSON.stringify(toPersistedAuth(prev)) === JSON.stringify(toPersistedAuth(next));
  if (same) return next;
  return { ...next, oauth2: { ...next.oauth2, ...EMPTY_TOKEN } };
}

/**
 * True when the token is past its lifetime. Without a lifetime there is
 * nothing to compare, so the token counts as usable.
 */
export function isTokenExpired(
  oauth: OAuth2State,
  nowSeconds: number = Math.floor(Date.now() / 1000),
): boolean {
  if (!oauth.expiresIn || !oauth.tokenAcquiredAt) return false;
  return nowSeconds >= oauth.tokenAcquiredAt + oauth.expiresIn - EXPIRY_MARGIN_SECONDS;
}
