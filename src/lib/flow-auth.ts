import { fromPersistedAuth, toPersistedAuth } from '@/lib/persisted-auth';
import type { Auth } from '@/lib/tauri-api';
import type { FlowAuthEntry } from '@/stores/flow-auth-store';
import type { AuthState, OAuth2AdditionalParam } from '@/types/pane-types';

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

/**
 * Key of one Auth node's in-memory state. Includes the flow, so a duplicated
 * flow never shares tokens, and the environment and the global environment, so
 * a token fetched for one environment is never sent to another. A missing
 * environment (null, undefined or '') is always the same key part.
 */
export function flowAuthKey(
  collection: string,
  flowName: string,
  nodeId: string,
  environmentName: string | null | undefined,
  globalEnvironmentName: string | null | undefined,
): string {
  return `${collection}::${flowName}::${environmentName || ''}::${globalEnvironmentName || ''}::${nodeId}`;
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

/** `state` with any fetched OAuth2 token, refresh token and lifetime cleared. */
export function withoutToken(state: AuthState): AuthState {
  if (!state.oauth2) return state;
  return { ...state, oauth2: { ...state.oauth2, ...EMPTY_TOKEN } };
}

const hasToken = (oauth: OAuth2State): boolean =>
  Boolean(oauth.accessToken || oauth.idToken || oauth.refreshToken);

// {{$dynamic}} placeholders produce a new value on every resolution. They are
// kept as written, so a fingerprint stays stable across calls.
const DYNAMIC_VAR = /(\{\{\s*\$[\w.-]+\s*\}\})/;

// cyrb53: a small, fast 53-bit string hash. Not a security measure; it only
// keeps resolved secrets (client secret, password) out of the fingerprint text.
function hash53(text: string): string {
  let h1 = 0xdeadbeef;
  let h2 = 0x41c6ce57;
  for (let i = 0; i < text.length; i++) {
    const ch = text.charCodeAt(i);
    h1 = Math.imul(h1 ^ ch, 2654435761);
    h2 = Math.imul(h2 ^ ch, 1597334677);
  }
  h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507) ^ Math.imul(h2 ^ (h2 >>> 13), 3266489909);
  h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507) ^ Math.imul(h1 ^ (h1 >>> 13), 3266489909);
  return (4294967296 * (2097151 & h2) + (h1 >>> 0)).toString(36);
}

/**
 * Identifies the configuration a token is fetched for, after variable
 * resolution: grant, URLs, client, credentials, scope, token source and the
 * additional parameters. Equal fingerprints mean a stored token was fetched
 * for the same values; a changed variable behind any of those fields changes
 * it. `rv` must be the same resolution the sign-in uses. Hashed, so it can sit
 * in memory next to the token without carrying resolved secrets; never log it.
 */
export function oauth2Fingerprint(oauth: OAuth2State, rv: (s: string) => string): string {
  const r = (s: string | undefined) =>
    (s ?? '')
      .split(DYNAMIC_VAR)
      .map((part, i) => (i % 2 === 1 ? part : rv(part)))
      .join('');
  const params = (list: OAuth2AdditionalParam[] | undefined) =>
    (list ?? []).map((p) => [r(p.key), r(p.value), p.sendIn, p.enabled]);
  return hash53(
    JSON.stringify([
      oauth.grantType,
      r(oauth.authorizationUrl),
      r(oauth.tokenUrl),
      r(oauth.refreshTokenUrl),
      r(oauth.callbackUrl),
      r(oauth.clientId),
      r(oauth.clientSecret),
      r(oauth.scope),
      r(oauth.username),
      r(oauth.password),
      oauth.clientAuthentication,
      oauth.tokenSource,
      params(oauth.authParams),
      params(oauth.tokenParams),
      params(oauth.refreshParams),
    ]),
  );
}

/**
 * The state an Auth node uses right now: `pickAuthState`, and a held OAuth2
 * token only while the resolved configuration still matches the fingerprint
 * stored with it. Otherwise (a variable value or the environment changed, or
 * no fingerprint) the token is cleared. `rv` is the current variable resolution.
 */
export function flowAuthState(
  entry: FlowAuthEntry | undefined,
  persisted: Auth,
  rv: (s: string) => string,
): AuthState {
  const state = pickAuthState(entry?.auth, persisted);
  if (!state.oauth2 || !hasToken(state.oauth2)) return state;
  if (entry?.fingerprint && entry.fingerprint === oauth2Fingerprint(state.oauth2, rv)) {
    return state;
  }
  return withoutToken(state);
}

/**
 * The fingerprint to store with `state`: set only when it holds an OAuth2
 * token, which was fetched for the configuration as `rv` resolves it now.
 */
export function fingerprintFor(state: AuthState, rv: (s: string) => string): string | undefined {
  return state.oauth2 && hasToken(state.oauth2) ? oauth2Fingerprint(state.oauth2, rv) : undefined;
}

/**
 * Returns `next`, with any fetched OAuth2 token cleared when the persisted
 * configuration differs from `prev`. A token fetched for one client id, URL or
 * scope must never be reused after the user edits those.
 */
export function resetTokenOnConfigChange(prev: AuthState | undefined, next: AuthState): AuthState {
  if (!prev || !next.oauth2) return next;
  // Canonical forms, as in pickAuthState: two states that persist the same
  // configuration once settled (an empty header prefix and "Bearer") are the same.
  const same = canonicalAuth(toPersistedAuth(prev)) === canonicalAuth(toPersistedAuth(next));
  if (same) return next;
  return withoutToken(next);
}

/**
 * The state an Auth node uses: the in-memory `stored` entry (which may hold a
 * fetched token) while its configuration still matches the persisted auth,
 * otherwise the persisted auth. The stored entry goes stale when the node
 * changes outside the editor (undo, reload, another edit).
 */
export function pickAuthState(stored: AuthState | undefined, persisted: Auth): AuthState {
  const fresh = fromPersistedAuth(persisted);
  if (!stored) return fresh;
  return canonicalAuth(toPersistedAuth(stored)) === canonicalAuth(persisted) ? stored : fresh;
}

// Upper bound on read/write round trips; the mapping settles after one or two.
const MAX_CANONICAL_ROUNDS = 4;

/**
 * A persisted auth in canonical form, as JSON: read back and written again
 * until it stops changing. The persisted mapping is not idempotent (an empty
 * OAuth2 header prefix is written as "Bearer", which then reads back and
 * writes as no token config), so two forms of the same configuration are
 * compared only after both have settled.
 */
function canonicalAuth(auth: Auth): string {
  let json = JSON.stringify(auth);
  for (let i = 0; i < MAX_CANONICAL_ROUNDS; i++) {
    const next = JSON.stringify(toPersistedAuth(fromPersistedAuth(JSON.parse(json) as Auth)));
    if (next === json) break;
    json = next;
  }
  return json;
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
