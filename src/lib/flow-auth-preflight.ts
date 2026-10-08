import { buildOAuth2VarContext } from '@/lib/execute-request';
import {
  flowAuthKey,
  flowAuthState,
  isInteractiveGrant,
  isOAuth2,
  isTokenExpired,
  oauth2Fingerprint,
} from '@/lib/flow-auth';
import {
  buildGetTokenRequest,
  buildRefreshRequest,
  type OAuth2Target,
} from '@/lib/oauth2-requests';
import {
  type FlowAuthToken,
  type FlowNode,
  type OAuth2TokenResult,
  oauth2GetToken,
  oauth2RefreshToken,
} from '@/lib/tauri-api';
import { resolveWithContext } from '@/lib/variable-context';
import { useFlowAuthStore } from '@/stores/flow-auth-store';
import type { AuthState } from '@/types/pane-types';

type OAuth2State = NonNullable<AuthState['oauth2']>;
export type AuthNode = FlowNode & { kind: Extract<FlowNode['kind'], { kind: 'Auth' }> };

export interface FlowAuthPreflightInput {
  collection: string;
  flowName: string;
  nodes: FlowNode[];
  environmentName?: string;
  /** The active global environment; part of the token key, like `environmentName`. */
  globalEnvName?: string;
}

/** What `authenticateAuthNode` needs to find a node's stored token and resolve its variables. */
export type AuthNodeScope = Omit<FlowAuthPreflightInput, 'nodes'>;

/** Where a token came from. `backend` means the run fetches it. `discarded` means it was not saved. */
export type AuthenticateSource = 'stored' | 'refreshed' | 'signed-in' | 'backend' | 'discarded';

export interface AuthenticateResult {
  accessToken?: string;
  source: AuthenticateSource;
}

export interface AuthenticateOptions {
  /** Skip the stored token and the refresh, and sign in again. */
  force?: boolean;
  /**
   * Asked after each network wait. When it returns false the token is dropped
   * unwritten, because the node was edited while the provider was answering.
   */
  shouldWrite?: () => boolean;
}

const isAuthNode = (node: FlowNode): node is AuthNode => node.kind.kind === 'Auth';

// The token the request will carry: the ID token when the node says so.
const pickToken = (oauth: OAuth2State): string =>
  oauth.tokenSource === 'idToken' ? oauth.idToken : oauth.accessToken;

// Folds a get-token or refresh result into the OAuth2 state. A refresh keeps
// the old refresh and ID tokens when the server does not return new ones.
function applyResult(
  prev: OAuth2State,
  result: OAuth2TokenResult,
  refreshed: boolean,
): OAuth2State {
  return {
    ...prev,
    accessToken: result.access_token,
    refreshToken: result.refresh_token || (refreshed ? prev.refreshToken : ''),
    expiresIn: typeof result.expires_in === 'number' ? result.expires_in : null,
    tokenAcquiredAt: Math.floor(Date.now() / 1000),
    idToken: result.id_token || (refreshed ? prev.idToken : ''),
    tokenType: result.token_type || (refreshed ? prev.tokenType : ''),
    responseScope: result.scope || (refreshed ? prev.responseScope : ''),
    accessTokenClaims: null,
    idTokenClaims: null,
  };
}

const messageOf = (err: unknown): string => (err instanceof Error ? err.message : String(err));

interface AuthenticateEnv {
  scope: AuthNodeScope;
  rv: (s: string) => string;
  target: OAuth2Target;
}

// The variable resolution of the pre-run step. Also the token fingerprint's
// resolution; the editor's `flowAuthResolver` builds the same context, so both
// recognise each other's tokens.
async function resolveEnv(scope: AuthNodeScope): Promise<AuthenticateEnv> {
  const varCtx = await buildOAuth2VarContext(scope.collection);
  return {
    scope,
    rv: (s) => resolveWithContext(s, varCtx),
    target: { collection: scope.collection, environmentName: scope.environmentName },
  };
}

// One OAuth 2.0 Auth node: a valid stored token is reused; an expired one with a
// refresh token is refreshed; otherwise an interactive grant prompts. Static
// auth types and non-interactive grants with no usable token resolve to
// `backend`. Rejects with a readable error when a sign-in fails.
async function authenticate(
  env: AuthenticateEnv,
  node: AuthNode,
  options: AuthenticateOptions = {},
): Promise<AuthenticateResult> {
  const { scope, rv, target } = env;
  const key = flowAuthKey(
    scope.collection,
    scope.flowName,
    node.id,
    scope.environmentName,
    scope.globalEnvName,
  );
  const store = useFlowAuthStore.getState();
  // The in-memory entry can be stale if the persisted auth changed outside
  // the editor (undo, reload), and its token stale if a variable behind the
  // configuration changed value; it is used only while both still match.
  const state = flowAuthState(store.getEntry(key), node.kind.auth, rv);
  const oauth = state.oauth2;
  if (!oauth) return { source: 'backend' };

  const current = pickToken(oauth);
  if (!options.force && current && !isTokenExpired(oauth)) {
    return { accessToken: current, source: 'stored' };
  }

  // Writes the new state unless the caller says the node changed while it waited.
  const write = (next: OAuth2State): boolean => {
    if (options.shouldWrite && !options.shouldWrite()) return false;
    store.setAuth(key, { ...state, oauth2: next }, oauth2Fingerprint(next, rv));
    return true;
  };

  if (!options.force && oauth.refreshToken && (oauth.refreshTokenUrl || oauth.tokenUrl)) {
    try {
      const next = applyResult(
        oauth,
        await oauth2RefreshToken(buildRefreshRequest(oauth, rv, target)),
        true,
      );
      if (!write(next)) return { source: 'discarded' };
      const token = pickToken(next);
      if (token) return { accessToken: token, source: 'refreshed' };
    } catch {
      // A refresh that fails falls through to a sign-in.
    }
  }

  if (isInteractiveGrant(node.kind.auth)) {
    let next: OAuth2State;
    try {
      next = applyResult(
        oauth,
        await oauth2GetToken(buildGetTokenRequest(oauth, rv, target)),
        false,
      );
    } catch (err) {
      throw new Error(`Sign-in for Auth node "${node.kind.label}" failed: ${messageOf(err)}`);
    }
    if (!write(next)) return { source: 'discarded' };
    const token = pickToken(next);
    return token ? { accessToken: token, source: 'signed-in' } : { source: 'signed-in' };
  }
  // A non-interactive grant with no usable token is fetched by the backend.
  return { source: 'backend' };
}

/**
 * Makes one OAuth 2.0 Auth node ready: reuses, refreshes or signs in, with the
 * rules of the pre-run step, and keeps the token in the in-memory store. The
 * Authenticate button calls this. Pass `force` to sign in again, and
 * `shouldWrite` to drop a token when the node was edited meanwhile.
 */
export async function authenticateAuthNode(
  scope: AuthNodeScope,
  node: AuthNode,
  options: AuthenticateOptions = {},
): Promise<AuthenticateResult> {
  if (!isOAuth2(node.kind.auth)) return { source: 'backend' };
  return authenticate(await resolveEnv(scope), node, options);
}

/**
 * Makes sure every Auth node that needs a browser sign-in has a valid token
 * before a flow run starts, and returns the tokens to hand to `runFlow`, keyed
 * by node id. Per node it follows `authenticateAuthNode`. A non-interactive
 * grant with no usable token is left out, and the backend fetches it. Static
 * auth types need nothing here. Rejects with a readable error when a sign-in
 * fails.
 */
export async function collectFlowAuthTokens(
  input: FlowAuthPreflightInput,
): Promise<Record<string, FlowAuthToken>> {
  const tokens: Record<string, FlowAuthToken> = {};
  const nodes = input.nodes.filter(isAuthNode).filter((n) => isOAuth2(n.kind.auth));
  if (nodes.length === 0) return tokens;

  const env = await resolveEnv(input);
  for (const node of nodes) {
    const result = await authenticate(env, node);
    if (result.accessToken) tokens[node.id] = { accessToken: result.accessToken };
  }
  return tokens;
}
