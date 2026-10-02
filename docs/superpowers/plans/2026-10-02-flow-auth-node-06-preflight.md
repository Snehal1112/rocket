# Flow Auth Node — Plan 6: Pre-run authentication

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 6 of 7.** Previous plan: `docs/superpowers/plans/2026-10-02-flow-auth-node-05-frontend-node.md` (must be merged and green).
**Next plan to run: `docs/superpowers/plans/2026-10-02-flow-auth-node-07-verify.md`**
**Recommended model: Sonnet.**

**Goal:** When the user clicks Run, make sure every Auth node that needs a browser sign-in has a valid token — reuse a valid one, refresh an expired one, or prompt (the same way the collection Authentication tab does) — then pass the tokens to `run_flow`. A failed sign-in stops the run before it starts.

**Architecture:** Two small reusable pieces are extracted from `execute-request.ts` (the OAuth2 variable context and the get-token/refresh request builders) so the pre-run step and the Request tab share one mapping. A pure async function `collectFlowAuthTokens` walks the flow's Auth nodes using the in-memory `flow-auth-store` from Plan 5. `FlowToolbar` calls it through a new `onPrepareAuth` prop (supplied by `FlowPane`) between "save" and "run".

**Tech Stack:** TypeScript, React, Zustand, Vitest + Testing Library.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-10-02-flow-auth-node-design.md`.
- Only interactive grants (authorization code, implicit) ever prompt. Non-interactive grants and static types are left to the backend (Plans 2–3).
- Tokens live only in the in-memory `flow-auth-store` and in the `runFlow` payload. Nothing here writes to disk, `localStorage` or logs; never `console.log` a token.
- `runFlow` keeps its four-argument call when there are no tokens, so existing tests and flows without Auth nodes are unchanged.
- A failed sign-in shows a toast and does not start the run.
- shadcn/ui primitives only; narrow Zustand selectors; conventional commits.
- If `yarn check` reports formatting or import-order findings, run `yarn lint` (auto-fix), then re-run `yarn check`.
- Verification per task: `yarn tsc --noEmit`, `yarn check`, the task's `yarn test <path>`.

## File Structure

| File | Change |
|---|---|
| `src/lib/execute-request.ts` | Export `buildOAuth2VarContext`; use the request builders |
| `src/lib/oauth2-requests.ts` | **Create**: `buildGetTokenRequest`, `buildRefreshRequest` |
| `src/lib/flow-auth-preflight.ts` | **Create**: `collectFlowAuthTokens` |
| `src/components/flow/FlowToolbar.tsx` | `onPrepareAuth` prop, pass tokens to `runFlow` |
| `src/components/flow/FlowPane.tsx` | Supply `onPrepareAuth` |
| Tests | `src/lib/__tests__/oauth2-requests.test.ts`, `src/lib/__tests__/flow-auth-preflight.test.ts`, `src/components/flow/__tests__/FlowToolbar.test.tsx` |

---

### Task 1: Extract the shared OAuth2 request builders

**Files:**
- Create: `src/lib/oauth2-requests.ts`
- Modify: `src/lib/execute-request.ts`
- Test: `src/lib/__tests__/oauth2-requests.test.ts`

**Interfaces:**
- Consumes: `OAuth2GetTokenRequest`, `OAuth2RefreshRequest` (`@/lib/tauri-api`), `AuthState['oauth2']`.
- Produces (used by Task 2):

```ts
export interface OAuth2Target { collection?: string; environmentName?: string; requestPath?: string }
export function buildGetTokenRequest(oauth: OAuth2State, rv: (s: string) => string,
  target: OAuth2Target, opts?: { forceReauth?: boolean }): OAuth2GetTokenRequest;
export function buildRefreshRequest(oauth: OAuth2State, rv: (s: string) => string,
  target: OAuth2Target): OAuth2RefreshRequest;
// in execute-request.ts
export async function buildOAuth2VarContext(collection: string | undefined,
  requestPath?: string): Promise<Record<string, string>>;
```

- [ ] **Step 1: Write the failing builder tests**

Create `src/lib/__tests__/oauth2-requests.test.ts`:

```ts
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
```

- [ ] **Step 2: Run to verify it fails**

Run: `yarn test src/lib/__tests__/oauth2-requests.test.ts`
Expected: FAIL (module not found).

- [ ] **Step 3: Create the builders**

Create `src/lib/oauth2-requests.ts`:

```ts
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
```

- [ ] **Step 4: Run the builder tests**

Run: `yarn test src/lib/__tests__/oauth2-requests.test.ts`
Expected: PASS.

- [ ] **Step 5: Use the builders and extract the variable context in `execute-request.ts`**

Add `import { buildGetTokenRequest, buildRefreshRequest } from '@/lib/oauth2-requests';` to the `@/lib` imports.

In `maybeAutoRefreshOrFetchToken`, replace the auto-refresh `try` body's request construction — everything from `const resolvedRefreshParams = ...` through the closing `});` of `oauth2RefreshToken({ ... })` — with:

```ts
      const result = await oauth2RefreshToken(
        buildRefreshRequest(oauth, rv, { collection, environmentName, requestPath }),
      );
```

and replace the auto-fetch `try` body's request construction — from `const resolvedAuthParams = ...` through the closing `});` of `oauth2GetToken({ ... })` — with:

```ts
      const result = await oauth2GetToken(
        buildGetTokenRequest(oauth, rv, { collection, environmentName, requestPath }),
      );
```

Leave the `applyToken({...})` calls that follow each of them exactly as they are.

Add this exported function directly above `sendRequest`:

```ts
// Builds the variable context used to pre-resolve OAuth2 fields before they
// reach the Tauri command. The backend env_repo only covers the workspace-level
// (global) environment directory, not collection-scoped environments, so
// variable resolution for OAuth2 must happen on the frontend. Shared by the
// Request tab and the Flow pre-run step.
export async function buildOAuth2VarContext(
  collection: string | undefined,
  requestPath?: string,
): Promise<Record<string, string>> {
  let collectionVars: CollectionVariable[] = [];
  if (collection) {
    try {
      const settings = await getCollectionSettings(collection);
      collectionVars = settings.variables;
    } catch {
      // Non-critical.
    }
  }
  let folderVars: CollectionVariable[] = [];
  let requestVars: CollectionVariable[] = [];
  if (collection && requestPath) {
    try {
      folderVars = await getFolderChainVariables(collection, requestPath);
    } catch {
      // Non-critical.
    }
    try {
      requestVars = await getRequestVariables(collection, requestPath);
    } catch {
      // Non-critical.
    }
  }
  return buildVariableContext({
    processEnvVars: getProcessEnvVars(),
    globalVars: getGlobalVariables(),
    envVars: getActiveVariables(),
    collectionVars,
    folderVars,
    requestVars,
  });
}
```

In `sendRequest`, replace the block from the comment `// Build a variable context for pre-resolving OAuth2 fields ...` through the end of the `const preVarCtx = buildVariableContext({ ... });` statement with:

```ts
  const preVarCtx = await buildOAuth2VarContext(preCollection, preRequestPath);
```

- [ ] **Step 6: Run the whole frontend lib and Request-tab suites**

Run: `yarn test src/lib src/components/request && yarn tsc --noEmit && yarn check`
Expected: all PASS. If `tsc` or biome reports an unused import or variable in `execute-request.ts` (for example `CollectionVariable` is still used, but a leftover `resolvedRefreshParams` local is not), remove it.

- [ ] **Step 7: Commit**

```bash
git add src/lib/oauth2-requests.ts src/lib/execute-request.ts src/lib/__tests__/oauth2-requests.test.ts
git commit -m "refactor: share the OAuth2 request builders and variable context"
```

---

### Task 2: The pre-run token collector

**Files:**
- Create: `src/lib/flow-auth-preflight.ts`
- Test: `src/lib/__tests__/flow-auth-preflight.test.ts`

**Interfaces:**
- Consumes: Task 1 (`buildGetTokenRequest`, `buildRefreshRequest`, `buildOAuth2VarContext`), Plan 5 (`flowAuthKey`, `isInteractiveGrant`, `isOAuth2`, `isTokenExpired`, `useFlowAuthStore`), Plan 4 (`FlowAuthToken`).
- Produces (used by Task 3):

```ts
export interface FlowAuthPreflightInput {
  collection: string; flowName: string; nodes: FlowNode[]; environmentName?: string;
}
export async function collectFlowAuthTokens(
  input: FlowAuthPreflightInput,
): Promise<Record<string, FlowAuthToken>>;   // rejects with a readable Error when a sign-in fails
```

- [ ] **Step 1: Write the failing tests**

Create `src/lib/__tests__/flow-auth-preflight.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { collectFlowAuthTokens } from '@/lib/flow-auth-preflight';
import { flowAuthKey } from '@/lib/flow-auth';
import { fromPersistedAuth } from '@/lib/persisted-auth';
import * as tauriApi from '@/lib/tauri-api';
import type { Auth, FlowNode } from '@/lib/tauri-api';
import { useFlowAuthStore } from '@/stores/flow-auth-store';
import type { AuthState } from '@/types/pane-types';

vi.mock('@/lib/execute-request', () => ({
  buildOAuth2VarContext: vi.fn().mockResolvedValue({ cid: 'resolved-cid' }),
}));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, oauth2GetToken: vi.fn(), oauth2RefreshToken: vi.fn() };
});

const oauthAuth = (flow: string): Auth =>
  ({
    authType: 'o-auth2',
    flow,
    authorizationUrl: 'https://idp.example.com/authorize',
    accessTokenUrl: 'https://idp.example.com/token',
    callbackUrl: 'https://app.example.com/cb',
    credentials: { clientId: '{{cid}}', clientSecret: 's' },
  }) as unknown as Auth;

const authNode = (id: string, auth: Auth, label = 'Sign in'): FlowNode => ({
  id,
  kind: { kind: 'Auth', label, auth, applyToInherit: true },
  position: { x: 0, y: 0 },
});

const input = (nodes: FlowNode[]) => ({ collection: 'api', flowName: 'login', nodes });
const key = (nodeId: string) => flowAuthKey('api', 'login', nodeId);

const result = (over: Partial<tauriApi.OAuth2TokenResult> = {}): tauriApi.OAuth2TokenResult => ({
  access_token: 'new-access-123456',
  token_type: 'Bearer',
  expires_in: 3600,
  refresh_token: 'new-refresh-123456',
  ...over,
});

/** Seeds the in-memory store with a token state for `nodeId`. */
function seed(nodeId: string, auth: Auth, patch: Record<string, unknown>) {
  const base = fromPersistedAuth(auth);
  const state: AuthState = {
    ...base,
    oauth2: { ...(base.oauth2 as NonNullable<AuthState['oauth2']>), ...patch },
  };
  useFlowAuthStore.getState().setAuth(key(nodeId), state);
}

describe('collectFlowAuthTokens', () => {
  beforeEach(() => {
    useFlowAuthStore.setState({ auths: {} });
    vi.mocked(tauriApi.oauth2GetToken).mockReset();
    vi.mocked(tauriApi.oauth2RefreshToken).mockReset();
  });

  it('returns nothing and calls nothing for a flow without Auth nodes', async () => {
    const tokens = await collectFlowAuthTokens(input([]));
    expect(tokens).toEqual({});
    expect(tauriApi.oauth2GetToken).not.toHaveBeenCalled();
  });

  it('ignores static auth types', async () => {
    const node = authNode('a', { authType: 'bearer', token: 't' });
    expect(await collectFlowAuthTokens(input([node]))).toEqual({});
    expect(tauriApi.oauth2GetToken).not.toHaveBeenCalled();
  });

  it('prompts for an interactive grant with no token, with variables resolved, and keeps the token in memory', async () => {
    vi.mocked(tauriApi.oauth2GetToken).mockResolvedValue(result());
    const node = authNode('a', oauthAuth('authorization_code'));

    const tokens = await collectFlowAuthTokens({ ...input([node]), environmentName: 'dev' });

    expect(tokens).toEqual({ a: { accessToken: 'new-access-123456' } });
    expect(tauriApi.oauth2GetToken).toHaveBeenCalledTimes(1);
    expect(tauriApi.oauth2GetToken).toHaveBeenCalledWith(
      expect.objectContaining({
        grantType: 'authorization_code',
        clientId: 'resolved-cid',
        collection: 'api',
        environmentName: 'dev',
      }),
    );
    expect(useFlowAuthStore.getState().getAuth(key('a'))?.oauth2?.accessToken).toBe(
      'new-access-123456',
    );
  });

  it('reuses a valid stored token without prompting', async () => {
    const auth = oauthAuth('authorization_code');
    seed('a', auth, {
      accessToken: 'stored-123456',
      expiresIn: 3600,
      tokenAcquiredAt: Math.floor(Date.now() / 1000),
    });

    const tokens = await collectFlowAuthTokens(input([authNode('a', auth)]));

    expect(tokens).toEqual({ a: { accessToken: 'stored-123456' } });
    expect(tauriApi.oauth2GetToken).not.toHaveBeenCalled();
    expect(tauriApi.oauth2RefreshToken).not.toHaveBeenCalled();
  });

  it('refreshes an expired token that has a refresh token, without prompting', async () => {
    const auth = oauthAuth('authorization_code');
    seed('a', auth, {
      accessToken: 'old-123456',
      refreshToken: 'ref-123456',
      expiresIn: 60,
      tokenAcquiredAt: 1,
    });
    vi.mocked(tauriApi.oauth2RefreshToken).mockResolvedValue(result());

    const tokens = await collectFlowAuthTokens(input([authNode('a', auth)]));

    expect(tokens).toEqual({ a: { accessToken: 'new-access-123456' } });
    expect(tauriApi.oauth2RefreshToken).toHaveBeenCalledTimes(1);
    expect(tauriApi.oauth2GetToken).not.toHaveBeenCalled();
  });

  it('falls back to a prompt when the refresh fails', async () => {
    const auth = oauthAuth('authorization_code');
    seed('a', auth, {
      accessToken: 'old-123456',
      refreshToken: 'ref-123456',
      expiresIn: 60,
      tokenAcquiredAt: 1,
    });
    vi.mocked(tauriApi.oauth2RefreshToken).mockRejectedValue(new Error('invalid_grant'));
    vi.mocked(tauriApi.oauth2GetToken).mockResolvedValue(result({ access_token: 'prompted-123456' }));

    const tokens = await collectFlowAuthTokens(input([authNode('a', auth)]));

    expect(tokens).toEqual({ a: { accessToken: 'prompted-123456' } });
  });

  it('leaves a non-interactive grant without a token to the backend', async () => {
    const node = authNode('a', oauthAuth('client_credentials'));
    expect(await collectFlowAuthTokens(input([node]))).toEqual({});
    expect(tauriApi.oauth2GetToken).not.toHaveBeenCalled();
  });

  it('passes a valid stored token for a non-interactive grant', async () => {
    const auth = oauthAuth('client_credentials');
    seed('a', auth, {
      accessToken: 'stored-123456',
      expiresIn: 3600,
      tokenAcquiredAt: Math.floor(Date.now() / 1000),
    });
    const tokens = await collectFlowAuthTokens(input([authNode('a', auth)]));
    expect(tokens).toEqual({ a: { accessToken: 'stored-123456' } });
  });

  it('rejects with the node label when a sign-in fails', async () => {
    vi.mocked(tauriApi.oauth2GetToken).mockRejectedValue(new Error('window closed'));
    const node = authNode('a', oauthAuth('implicit'), 'Corporate SSO');

    await expect(collectFlowAuthTokens(input([node]))).rejects.toThrow(
      'Sign-in for Auth node "Corporate SSO" failed: window closed',
    );
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `yarn test src/lib/__tests__/flow-auth-preflight.test.ts`
Expected: FAIL (module not found).

- [ ] **Step 3: Implement**

Create `src/lib/flow-auth-preflight.ts`:

```ts
import { buildOAuth2VarContext } from '@/lib/execute-request';
import { flowAuthKey, isInteractiveGrant, isOAuth2, isTokenExpired } from '@/lib/flow-auth';
import { buildGetTokenRequest, buildRefreshRequest, type OAuth2Target } from '@/lib/oauth2-requests';
import { fromPersistedAuth } from '@/lib/persisted-auth';
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
}

const isAuthNode = (node: FlowNode): node is AuthNode => node.kind.kind === 'Auth';

// The token the request will carry: the ID token when the node says so.
const pickToken = (oauth: OAuth2State): string =>
  oauth.tokenSource === 'idToken' ? oauth.idToken : oauth.accessToken;

// Folds a get-token or refresh result into the OAuth2 state. A refresh keeps
// the old refresh and ID tokens when the server does not return new ones.
function applyResult(prev: OAuth2State, result: OAuth2TokenResult, refreshed: boolean): OAuth2State {
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

  const varCtx = await buildOAuth2VarContext(input.collection);
  const rv = (s: string) => resolveWithContext(s, varCtx);
  const target: OAuth2Target = {
    collection: input.collection,
    environmentName: input.environmentName,
  };

  for (const node of nodes) {
    const key = flowAuthKey(input.collection, input.flowName, node.id);
    const store = useFlowAuthStore.getState();
    const state = store.getAuth(key) ?? fromPersistedAuth(node.kind.auth);
    const oauth = state.oauth2;
    if (!oauth) continue;

    const current = pickToken(oauth);
    if (current && !isTokenExpired(oauth)) {
      tokens[node.id] = { accessToken: current };
      continue;
    }

    const remember = (next: OAuth2State) => {
      store.setAuth(key, { ...state, oauth2: next });
      const token = pickToken(next);
      if (token) tokens[node.id] = { accessToken: token };
    };

    if (oauth.refreshToken && (oauth.refreshTokenUrl || oauth.tokenUrl)) {
      try {
        remember(
          applyResult(oauth, await oauth2RefreshToken(buildRefreshRequest(oauth, rv, target)), true),
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
```

- [ ] **Step 4: Run the tests**

Run: `yarn test src/lib/__tests__/flow-auth-preflight.test.ts && yarn tsc --noEmit && yarn check`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src/lib/flow-auth-preflight.ts src/lib/__tests__/flow-auth-preflight.test.ts
git commit -m "feat(flow): authenticate interactive Auth nodes before a run"
```

---

### Task 3: Run the pre-run step from the toolbar

**Files:**
- Modify: `src/components/flow/FlowToolbar.tsx`
- Modify: `src/components/flow/FlowPane.tsx`
- Test: `src/components/flow/__tests__/FlowToolbar.test.tsx`

**Interfaces:**
- Consumes: `collectFlowAuthTokens` (Task 2), `runFlow(..., authTokens?)` (Plan 4).
- Produces: `FlowToolbar` prop `onPrepareAuth?: () => Promise<Record<string, FlowAuthToken> | null>`; `null` or a rejection aborts the run.

- [ ] **Step 1: Write the failing toolbar tests**

In `src/components/flow/__tests__/FlowToolbar.test.tsx`, add this mock below the existing `vi.mock('@/lib/execute-request', ...)` block, plus the import:

```tsx
vi.mock('sonner', () => ({ toast: { error: vi.fn() } }));
```

```tsx
import { toast } from 'sonner';
```

Add these tests inside the `describe('FlowToolbar', ...)` block, after the test `reads the global environment name fresh at click-time, not from an earlier render`:

```tsx
  it('passes the tokens from onPrepareAuth to runFlow', async () => {
    const onPrepareAuth = vi.fn().mockResolvedValue({ a: { accessToken: 'tok-123456' } });
    renderToolbar({ onPrepareAuth });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() =>
      expect(tauriApi.runFlow).toHaveBeenCalledWith('my-collection', 'my-flow', null, null, {
        a: { accessToken: 'tok-123456' },
      }),
    );
  });

  it('keeps the four-argument runFlow call when there are no tokens', async () => {
    const onPrepareAuth = vi.fn().mockResolvedValue({});
    renderToolbar({ onPrepareAuth });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() =>
      expect(tauriApi.runFlow).toHaveBeenCalledWith('my-collection', 'my-flow', null, null),
    );
  });

  it('does not start the run when the sign-in step fails, and says why', async () => {
    const onPrepareAuth = vi
      .fn()
      .mockRejectedValue(new Error('Sign-in for Auth node "SSO" failed: window closed'));
    renderToolbar({ onPrepareAuth });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith(
        expect.stringContaining('Sign-in for Auth node "SSO" failed: window closed'),
      ),
    );
    expect(tauriApi.runFlow).not.toHaveBeenCalled();
    // The Run button works again.
    expect(screen.getByRole('button', { name: 'Run' })).toBeEnabled();
  });

  it('does not start the run when onPrepareAuth returns null', async () => {
    const onPrepareAuth = vi.fn().mockResolvedValue(null);
    renderToolbar({ onPrepareAuth });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(onPrepareAuth).toHaveBeenCalled());
    expect(tauriApi.runFlow).not.toHaveBeenCalled();
  });

  it('runs onPrepareAuth after onBeforeRun saves', async () => {
    const order: string[] = [];
    const onBeforeRun = vi.fn(async () => {
      order.push('save');
      return true;
    });
    const onPrepareAuth = vi.fn(async () => {
      order.push('auth');
      return {};
    });
    renderToolbar({ onBeforeRun, onPrepareAuth });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalled());
    expect(order).toEqual(['save', 'auth']);
  });
```

- [ ] **Step 2: Run to verify they fail**

Run: `yarn test src/components/flow/__tests__/FlowToolbar.test.tsx`
Expected: the five new tests FAIL (no `onPrepareAuth` prop; `runFlow` never receives tokens).

- [ ] **Step 3: Implement the toolbar change**

In `src/components/flow/FlowToolbar.tsx`:

- extend the `@/lib/tauri-api` import with `type FlowAuthToken,`;
- add to `FlowToolbarProps`:

```ts
  // Runs after onBeforeRun and before the run starts. Authenticates Auth nodes
  // and returns the tokens to hand to the run. Returning null, or rejecting,
  // aborts the run.
  onPrepareAuth?: () => Promise<Record<string, FlowAuthToken> | null>;
```

- add `onPrepareAuth,` to the destructured props of `FlowToolbar`;
- in `handleRun`, directly after the `if (onBeforeRun) { ... }` block and before `cleanupListeners();`, add:

```ts
    let authTokens: Record<string, FlowAuthToken> | undefined;
    if (onPrepareAuth) {
      try {
        const prepared = await onPrepareAuth();
        if (prepared === null) {
          isStartingRef.current = false;
          return;
        }
        authTokens = prepared;
      } catch (err) {
        toast.error(err instanceof Error ? err.message : String(err));
        isStartingRef.current = false;
        return;
      }
    }
```

- replace the `runFlow` call

```ts
      const summary = await runFlow(collection, flowName, environmentName, globalEnvName ?? null);
```

with

```ts
      // Tokens are sent only when there are some, so a flow without Auth nodes
      // calls the command exactly as before.
      const summary =
        authTokens && Object.keys(authTokens).length > 0
          ? await runFlow(collection, flowName, environmentName, globalEnvName ?? null, authTokens)
          : await runFlow(collection, flowName, environmentName, globalEnvName ?? null);
```

- [ ] **Step 4: Supply `onPrepareAuth` from `FlowPane`**

In `src/components/flow/FlowPane.tsx`, add the import `import { collectFlowAuthTokens } from '@/lib/flow-auth-preflight';` and, on the `<FlowToolbar ... />` element, after `onBeforeRun={handleBeforeRun}` add:

```tsx
              onPrepareAuth={() =>
                collectFlowAuthTokens({
                  collection: collectionName,
                  flowName,
                  // Read at click time, after onBeforeRun saved unsaved edits.
                  nodes: latestFlowTab()?.nodes ?? tab.nodes,
                  environmentName: activeEnvironmentName ?? undefined,
                })
              }
```

If `flowName` or `collectionName` are not the names used in that scope, use the same expressions the neighboring `onStepLogs` and `<NodePropertiesPanel collection=...>` props already use.

- [ ] **Step 5: Run the checks**

Run: `yarn test src/components/flow src/lib && yarn tsc --noEmit && yarn check`
Expected: all PASS, including every pre-existing `FlowToolbar` and `FlowPane` test.

- [ ] **Step 6: Commit**

```bash
git add src/components/flow/FlowToolbar.tsx src/components/flow/FlowPane.tsx src/components/flow/__tests__/FlowToolbar.test.tsx
git commit -m "feat(flow): sign in to Auth nodes before a run starts"
```

---

**End of Plan 6.** Verify: `yarn tsc --noEmit`, `yarn check`, `yarn test src/lib src/components/flow src/components/request src/stores`.

**Next plan to run: `docs/superpowers/plans/2026-10-02-flow-auth-node-07-verify.md`** (end-to-end tests, security review, docs, final verification).
