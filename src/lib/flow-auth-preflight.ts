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
type AuthNode = FlowNode & { kind: Extract<FlowNode['kind'], { kind: 'Auth' }> };

export interface FlowAuthPreflightInput {
  collection: string;
  flowName: string;
  nodes: FlowNode[];
  environmentName?: string;
  /** The active global environment; part of the token key, like `environmentName`. */
  globalEnvName?: string;
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

/**
 * Makes sure every Auth node that needs a browser sign-in has a valid token
 * before a flow run starts, and returns the tokens to hand to `runFlow`, keyed
 * by node id.
 *
 * Per OAuth2 node: a valid stored token is reused; an expired one with a
 * refresh token is refreshed; otherwise an interactive grant prompts (the same
 * command as the Authentication tab). A non-interactive grant with no usable
 * token is left out, and the backend fetches it. Static auth types need
 * nothing here. Rejects with a readable error when a sign-in fails.
 */
export async function collectFlowAuthTokens(
  input: FlowAuthPreflightInput,
): Promise<Record<string, FlowAuthToken>> {
  const tokens: Record<string, FlowAuthToken> = {};
  const nodes = input.nodes.filter(isAuthNode).filter((n) => isOAuth2(n.kind.auth));
  if (nodes.length === 0) return tokens;

  // Also the token fingerprint's resolution; the editor's `flowAuthResolver`
  // builds the same context, so both recognise each other's tokens.
  const varCtx = await buildOAuth2VarContext(input.collection);
  const rv = (s: string) => resolveWithContext(s, varCtx);
  const target: OAuth2Target = {
    collection: input.collection,
    environmentName: input.environmentName,
  };

  for (const node of nodes) {
    const key = flowAuthKey(
      input.collection,
      input.flowName,
      node.id,
      input.environmentName,
      input.globalEnvName,
    );
    const store = useFlowAuthStore.getState();
    // The in-memory entry can be stale if the persisted auth changed outside
    // the editor (undo, reload), and its token stale if a variable behind the
    // configuration changed value; it is used only while both still match.
    const state = flowAuthState(store.getEntry(key), node.kind.auth, rv);
    const oauth = state.oauth2;
    if (!oauth) continue;

    const current = pickToken(oauth);
    if (current && !isTokenExpired(oauth)) {
      tokens[node.id] = { accessToken: current };
      continue;
    }

    const remember = (next: OAuth2State) => {
      store.setAuth(key, { ...state, oauth2: next }, oauth2Fingerprint(next, rv));
      const token = pickToken(next);
      if (token) tokens[node.id] = { accessToken: token };
    };

    if (oauth.refreshToken && (oauth.refreshTokenUrl || oauth.tokenUrl)) {
      try {
        remember(
          applyResult(
            oauth,
            await oauth2RefreshToken(buildRefreshRequest(oauth, rv, target)),
            true,
          ),
        );
        if (tokens[node.id]) continue;
      } catch {
        // A refresh that fails falls through to a sign-in.
      }
    }

    if (isInteractiveGrant(node.kind.auth)) {
      try {
        remember(
          applyResult(oauth, await oauth2GetToken(buildGetTokenRequest(oauth, rv, target)), false),
        );
      } catch (err) {
        throw new Error(`Sign-in for Auth node "${node.kind.label}" failed: ${messageOf(err)}`);
      }
    }
    // A non-interactive grant with no usable token is fetched by the backend.
  }
  return tokens;
}
