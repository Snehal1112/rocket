# Authenticate Button Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a person sign in an OAuth 2.0 Auth node from its properties panel, and see at a glance whether the node holds a valid token, without starting a flow run.

**Architecture:** The per-node sign-in logic that lives inside the loop of `collectFlowAuthTokens` is extracted into an exported `authenticateAuthNode(scope, node, options)`. The pre-run step calls it for each node, so run and button share one code path and one set of store-write rules. A new `AuthenticateButton` component calls it, shows a token status, and guards against a double click and against the node's configuration changing while the sign-in window is open. `AuthNodeEditor` renders the button for interactive grants. Frontend only.

**Tech Stack:** React, TypeScript, Zustand (`flow-auth-store`), Vitest and Testing Library, shadcn `Button` and `Badge`, lucide icons.

**Spec:** Roadmap item F-47 in `.claude/flow-roadmap.md`. Flow auth spec: `docs/superpowers/specs/2026-10-02-flow-auth-node-design.md` (the Authenticate button, line 159). Design notes: `docs/superpowers/plans/flow-tier3/01-design-notes.md` (section P14).

**Decision assumed:** D4, interactive grants only. The button appears for authorization code and implicit grants. Client-credentials and password grants are fetched by the backend at run time and get no button. To include them later, drop the `isInteractiveGrant` check in `AuthNodeEditor` and let `authenticateAuthNode` call `oauth2GetToken` for them (a one-line change in the last branch of `authenticate`).

**Depends on:** plan P3 merged (this plan edits the P3 version of `AuthNodeEditor.tsx`, which uses `useCollectionVariableContext`).

## What was verified before writing

- `AuthNodeEditor` already embeds `OAuth2AuthEditor`, which has its own "Get New Access Token" and "Refresh" buttons that write through `handleAuthChange`. The new button does not replace them. It adds a one-click path at the node level that reuses the pre-run rules (reuse a valid token, else refresh, else sign in), and a token status.
- `collectFlowAuthTokens` (`src/lib/flow-auth-preflight.ts:75-150`) builds the variable context once, then loops over the OAuth 2.0 Auth nodes. The loop body is the sign-in logic. A closure `remember` writes the new token with `store.setAuth(key, { ...state, oauth2: next }, oauth2Fingerprint(next, rv))`.
- The token store key is `flowAuthKey(collection, flowName, nodeId, environmentName, globalEnvName)` (`flow-auth.ts:36-44`). The editor builds it from `activeEnvId` and the global environment name from `useGlobalEnvironmentName()`, which is the same query-cache entry the pre-run step reads through `getActiveGlobalEnvName()`.
- `isTokenExpired` and `isInteractiveGrant` already exist in `src/lib/flow-auth.ts`. A token status helper does not.
- Existing preflight tests (`src/lib/__tests__/flow-auth-preflight.test.ts`) have the helpers `oauthAuth`, `authNode`, `seed`, `result`, `key` and `DEFAULT_VARS`, and mock `@/lib/execute-request` and the two `oauth2*` commands. Task 1 reuses them.
- `AuthNodeEditor.test.tsx` mocks `AuthEditor`, the environment queries and `getCollectionSettings`, but not `@/lib/execute-request`. Once the editor imports the button, that module is in the import graph, so Task 3 mocks it.

## Global Constraints

- shadcn/ui primitives only (`Button`, `Badge`), `lucide-react` icons only, no raw `<button>`.
- Zustand: never fully destructure store state at component top level. Use narrow selectors.
- No Rust changes.
- A token or secret must never be placed in the DOM, a `title` attribute, a `data-*` attribute or a log line. Status text says "valid", "expired" or "none", never the token.
- Before starting each task, read `docs/superpowers/specs/opencollection-spec-reference.md` (auth configuration).
- Code comments are short full sentences that end with a punctuation mark.
- Commits use conventional commit format and are created through the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only. Never `git add -A`, `--all` or `.`.
- Gates before each commit: `yarn tsc --noEmit`, `yarn check`, and the targeted `yarn test <pattern>` listed in the task.
- Only one implementer at a time touches `flow-auth-preflight.ts` and `AuthNodeEditor.tsx`.
- Not in scope: signing in non-interactive grants (D4), a "sign out" action, persisting tokens, a visible countdown.

## Review Focus

Failure modes the spec implies but the obvious tests would miss, most likely first:

1. A sign-in that finishes after the node's configuration was edited writes a token for the old configuration (the F-10 race). Pinned in Task 1 (`shouldWrite` tests) and Task 3 (edit during sign-in).
2. A second click, or a click while a sign-in is pending, opens a second browser window. Pinned in Task 2.
3. The extraction changes the pre-run behavior: the variable context is rebuilt per node, a refresh failure no longer falls through to a sign-in, or a non-interactive grant now prompts. Pinned in Task 1 (the existing tests plus a call-count test).
4. A token appears in the DOM or an attribute. Pinned in Task 2.
5. The button shows for a non-interactive grant, or the token is stored under a different environment key than the one the status reads. Pinned in Task 3.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/lib/flow-auth-preflight.ts` (modify) | Extract `authenticateAuthNode`; `collectFlowAuthTokens` calls the shared function. |
| `src/lib/flow-auth.ts` (modify) | `oauth2TokenStatus(oauth, nowSeconds?)`. |
| `src/components/flow/properties/AuthenticateButton.tsx` (new) | The button, the token status badge and the result messages. |
| `src/components/flow/properties/AuthNodeEditor.tsx` (modify) | Renders the button for interactive OAuth 2.0 grants. |

Interfaces (names and signatures fixed here):

```ts
// flow-auth-preflight.ts
export type AuthNode = FlowNode & { kind: Extract<FlowNode['kind'], { kind: 'Auth' }> };
export type AuthNodeScope = Omit<FlowAuthPreflightInput, 'nodes'>;
export type AuthenticateSource = 'stored' | 'refreshed' | 'signed-in' | 'backend' | 'discarded';
export interface AuthenticateResult { accessToken?: string; source: AuthenticateSource }
export interface AuthenticateOptions {
  /** Skip the stored token and the refresh, and sign in again. */
  force?: boolean;
  /** Asked after each network wait. When it returns false the token is not written. */
  shouldWrite?: () => boolean;
}
export function authenticateAuthNode(
  scope: AuthNodeScope, node: AuthNode, options?: AuthenticateOptions,
): Promise<AuthenticateResult>;

// flow-auth.ts
export type Oauth2TokenStatus =
  | { kind: 'none' }
  | { kind: 'valid'; expiresAt: number | null }  // seconds since epoch, null without a lifetime
  | { kind: 'expired' };
export function oauth2TokenStatus(oauth: OAuth2State | undefined, nowSeconds?: number): Oauth2TokenStatus;

// AuthenticateButton.tsx
export function AuthenticateButton(props: {
  node: AuthNode; scope: AuthNodeScope; oauth: OAuth2State | undefined;
}): JSX.Element;
```

---

### Task 1: Extract `authenticateAuthNode`

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src/lib/flow-auth-preflight.ts` (types near line 24-34, `collectFlowAuthTokens` at lines 75-150)
- Modify: `src/lib/__tests__/flow-auth-preflight.test.ts` (import line 4, new `describe` at the end)

**Interfaces:**
- Produces: `AuthNode`, `AuthNodeScope`, `AuthenticateSource`, `AuthenticateResult`, `AuthenticateOptions`, `authenticateAuthNode`.

Behavior preserved from `collectFlowAuthTokens` (every existing test must stay green): a valid stored token is reused; an expired token with a refresh token is refreshed; a refresh that fails (or yields no token) falls through to a sign-in for an interactive grant; a non-interactive grant with no usable token is left to the backend; a failed sign-in throws `Sign-in for Auth node "<label>" failed: <message>`; the variable context is built once per call.

- [ ] **Step 1: Write the failing tests**

In `src/lib/__tests__/flow-auth-preflight.test.ts`, change the import on line 4 to:

```ts
import {
  type AuthNode,
  authenticateAuthNode,
  collectFlowAuthTokens,
} from '@/lib/flow-auth-preflight';
```

and append at the end of the file:

```ts
describe('authenticateAuthNode', () => {
  const scope = { collection: 'api', flowName: 'login' };
  const asAuthNode = (n: FlowNode) => n as AuthNode;

  beforeEach(() => {
    useFlowAuthStore.setState({ auths: {} });
    vi.mocked(tauriApi.oauth2GetToken).mockReset();
    vi.mocked(tauriApi.oauth2RefreshToken).mockReset();
    vi.mocked(buildOAuth2VarContext).mockReset();
    vi.mocked(buildOAuth2VarContext).mockResolvedValue(DEFAULT_VARS);
  });

  it('signs in an interactive grant and keeps the token in memory', async () => {
    vi.mocked(tauriApi.oauth2GetToken).mockResolvedValue(result());
    const node = asAuthNode(authNode('a', oauthAuth('authorization_code')));

    const out = await authenticateAuthNode(scope, node);

    expect(out).toEqual({ accessToken: 'new-access-123456', source: 'signed-in' });
    expect(useFlowAuthStore.getState().getAuth(key('a'))?.oauth2?.accessToken).toBe(
      'new-access-123456',
    );
  });

  it('keys the stored token by the scope environment and global environment', async () => {
    vi.mocked(tauriApi.oauth2GetToken).mockResolvedValue(result());
    const node = asAuthNode(authNode('a', oauthAuth('authorization_code')));

    await authenticateAuthNode({ ...scope, environmentName: 'dev', globalEnvName: 'g1' }, node);

    expect(useFlowAuthStore.getState().getAuth(key('a', 'dev', 'g1'))).toBeDefined();
    expect(useFlowAuthStore.getState().getAuth(key('a'))).toBeUndefined();
  });

  it('reuses a valid stored token without prompting', async () => {
    const auth = oauthAuth('authorization_code');
    seed('a', auth, {
      accessToken: 'stored-123456',
      expiresIn: 3600,
      tokenAcquiredAt: Math.floor(Date.now() / 1000),
    });

    const out = await authenticateAuthNode(scope, asAuthNode(authNode('a', auth)));

    expect(out).toEqual({ accessToken: 'stored-123456', source: 'stored' });
    expect(tauriApi.oauth2GetToken).not.toHaveBeenCalled();
  });

  it('signs in again when forced, even with a valid token and a refresh token', async () => {
    const auth = oauthAuth('authorization_code');
    seed('a', auth, {
      accessToken: 'stored-123456',
      refreshToken: 'ref-123456',
      expiresIn: 3600,
      tokenAcquiredAt: Math.floor(Date.now() / 1000),
    });
    vi.mocked(tauriApi.oauth2GetToken).mockResolvedValue(
      result({ access_token: 'forced-123456' }),
    );

    const out = await authenticateAuthNode(scope, asAuthNode(authNode('a', auth)), {
      force: true,
    });

    expect(out).toEqual({ accessToken: 'forced-123456', source: 'signed-in' });
    expect(tauriApi.oauth2RefreshToken).not.toHaveBeenCalled();
    expect(useFlowAuthStore.getState().getAuth(key('a'))?.oauth2?.accessToken).toBe(
      'forced-123456',
    );
  });

  it('refreshes an expired token before prompting', async () => {
    const auth = oauthAuth('authorization_code');
    seed('a', auth, {
      accessToken: 'old-123456',
      refreshToken: 'ref-123456',
      expiresIn: 60,
      tokenAcquiredAt: 1,
    });
    vi.mocked(tauriApi.oauth2RefreshToken).mockResolvedValue(result());

    const out = await authenticateAuthNode(scope, asAuthNode(authNode('a', auth)));

    expect(out).toEqual({ accessToken: 'new-access-123456', source: 'refreshed' });
    expect(tauriApi.oauth2GetToken).not.toHaveBeenCalled();
  });

  it('leaves a non-interactive grant without a token to the backend', async () => {
    const out = await authenticateAuthNode(
      scope,
      asAuthNode(authNode('a', oauthAuth('client_credentials'))),
    );
    expect(out).toEqual({ source: 'backend' });
    expect(tauriApi.oauth2GetToken).not.toHaveBeenCalled();
  });

  it('does nothing for an auth that is not OAuth 2.0', async () => {
    const out = await authenticateAuthNode(
      scope,
      asAuthNode(authNode('a', { authType: 'bearer', token: 't' })),
    );
    expect(out).toEqual({ source: 'backend' });
    expect(buildOAuth2VarContext).not.toHaveBeenCalled();
  });

  it('does not write the token when shouldWrite says the node changed, after a sign-in', async () => {
    vi.mocked(tauriApi.oauth2GetToken).mockResolvedValue(result());
    const node = asAuthNode(authNode('a', oauthAuth('authorization_code')));

    const out = await authenticateAuthNode(scope, node, { shouldWrite: () => false });

    expect(out).toEqual({ source: 'discarded' });
    expect(tauriApi.oauth2GetToken).toHaveBeenCalledTimes(1);
    expect(useFlowAuthStore.getState().getEntry(key('a'))).toBeUndefined();
  });

  it('does not write, and does not then prompt, when shouldWrite rejects a refresh', async () => {
    const auth = oauthAuth('authorization_code');
    seed('a', auth, {
      accessToken: 'old-123456',
      refreshToken: 'ref-123456',
      expiresIn: 60,
      tokenAcquiredAt: 1,
    });
    vi.mocked(tauriApi.oauth2RefreshToken).mockResolvedValue(result());

    const out = await authenticateAuthNode(scope, asAuthNode(authNode('a', auth)), {
      shouldWrite: () => false,
    });

    expect(out).toEqual({ source: 'discarded' });
    expect(tauriApi.oauth2GetToken).not.toHaveBeenCalled();
    expect(useFlowAuthStore.getState().getAuth(key('a'))?.oauth2?.accessToken).toBe('old-123456');
  });

  it('rejects with the node label when the sign-in fails, and stores nothing', async () => {
    vi.mocked(tauriApi.oauth2GetToken).mockRejectedValue(new Error('window closed'));
    const node = asAuthNode(authNode('a', oauthAuth('implicit'), 'Corporate SSO'));

    await expect(authenticateAuthNode(scope, node)).rejects.toThrow(
      'Sign-in for Auth node "Corporate SSO" failed: window closed',
    );
    expect(useFlowAuthStore.getState().getEntry(key('a'))).toBeUndefined();
  });
});

describe('collectFlowAuthTokens variable context', () => {
  it('builds the variable context once for several nodes', async () => {
    useFlowAuthStore.setState({ auths: {} });
    vi.mocked(buildOAuth2VarContext).mockReset();
    vi.mocked(buildOAuth2VarContext).mockResolvedValue(DEFAULT_VARS);
    vi.mocked(tauriApi.oauth2GetToken).mockReset();
    vi.mocked(tauriApi.oauth2GetToken).mockResolvedValue(result());

    await collectFlowAuthTokens(
      input([
        authNode('a', oauthAuth('authorization_code')),
        authNode('b', oauthAuth('implicit')),
      ]),
    );

    expect(buildOAuth2VarContext).toHaveBeenCalledTimes(1);
    expect(tauriApi.oauth2GetToken).toHaveBeenCalledTimes(2);
  });
});
```

- [ ] **Step 2: Run to verify the new tests fail**

Run: `yarn test src/lib/__tests__/flow-auth-preflight.test.ts`
Expected: FAIL, `authenticateAuthNode` is not exported. The original `collectFlowAuthTokens` tests still pass.

- [ ] **Step 3: Extract the function**

In `src/lib/flow-auth-preflight.ts`:

1. Change `type AuthNode = ...` to an export:

```ts
export type AuthNode = FlowNode & { kind: Extract<FlowNode['kind'], { kind: 'Auth' }> };
```

2. After the `FlowAuthPreflightInput` interface, add:

```ts
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
```

3. Replace everything from the doc comment of `collectFlowAuthTokens` to the end of the file with:

```ts
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
      next = applyResult(oauth, await oauth2GetToken(buildGetTokenRequest(oauth, rv, target)), false);
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
```

- [ ] **Step 4: Run to verify all preflight tests pass**

Run: `yarn test src/lib/__tests__/flow-auth-preflight.test.ts src/lib/__tests__/flow-auth.test.ts src/components/flow`
Expected: PASS, including every original `collectFlowAuthTokens` test (they pin the preserved behavior).

- [ ] **Step 5: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors (an unused `FlowAuthPreflightInput` field or import fails `yarn check`).

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/lib/flow-auth-preflight.ts src/lib/__tests__/flow-auth-preflight.test.ts`
Suggested subject: `refactor(flow): extract authenticateAuthNode from the pre-run step`.

---

### Task 2: Token status helper and the `AuthenticateButton` component

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src/lib/flow-auth.ts` (new function after `isTokenExpired`)
- Modify: `src/lib/__tests__/flow-auth.test.ts` (import, new `describe`)
- Create: `src/components/flow/properties/AuthenticateButton.tsx`
- Create: `src/components/flow/properties/__tests__/AuthenticateButton.test.tsx`

**Interfaces:**
- Produces: `oauth2TokenStatus`, `Oauth2TokenStatus`, `AuthenticateButton`.
- Consumes: `authenticateAuthNode`, `AuthNode`, `AuthNodeScope`, `AuthenticateResult` (Task 1).

Button behavior, fixed here:

- Label `Authenticate` with a `LogIn` icon when there is no valid token (none or expired). `Authenticate again` with a `KeyRound` icon when the token is valid; that click passes `force: true`. While busy: `Signing in…` with a spinning `Loader2`, the button disabled and `aria-busy`.
- A badge shows `No token`, `Token expired`, `Token valid`, or `Token valid until HH:MM`.
- Result lines: success and info use `role="status"`, errors use `role="alert"`. Texts: `Signed in.`, `Token refreshed.`, `Already signed in.`, `The sign-in settings changed, so the new token was not saved.`, `The provider returned no token.`, and for a rejected sign-in the error message.
- `shouldWrite` compares the node's persisted auth (as JSON) at click time with the latest render.

- [ ] **Step 1: Write the failing status tests**

In `src/lib/__tests__/flow-auth.test.ts`, add `oauth2TokenStatus` to the import list from `@/lib/flow-auth` (sorted), and append:

```ts
describe('oauth2TokenStatus', () => {
  const o = (patch: Partial<NonNullable<AuthState['oauth2']>>) =>
    ({
      accessToken: 'tok',
      idToken: '',
      refreshToken: '',
      tokenSource: 'accessToken',
      expiresIn: 3600,
      tokenAcquiredAt: 1000,
      ...patch,
    }) as NonNullable<AuthState['oauth2']>;

  it('is none without state, without a token, or with only a refresh token', () => {
    expect(oauth2TokenStatus(undefined, 1000)).toEqual({ kind: 'none' });
    expect(oauth2TokenStatus(o({ accessToken: '' }), 1000)).toEqual({ kind: 'none' });
    expect(oauth2TokenStatus(o({ accessToken: '', refreshToken: 'r' }), 1000)).toEqual({
      kind: 'none',
    });
  });

  it('is valid with the expiry time while the token has lifetime left', () => {
    expect(oauth2TokenStatus(o({}), 1100)).toEqual({ kind: 'valid', expiresAt: 4600 });
  });

  it('is valid without an expiry time when the token has no lifetime', () => {
    expect(oauth2TokenStatus(o({ expiresIn: null }), 9999)).toEqual({
      kind: 'valid',
      expiresAt: null,
    });
  });

  it('is expired once the lifetime is used up, with the 30 second margin', () => {
    expect(oauth2TokenStatus(o({}), 1000 + 3600 - 31)).toEqual({ kind: 'valid', expiresAt: 4600 });
    expect(oauth2TokenStatus(o({}), 1000 + 3600 - 29)).toEqual({ kind: 'expired' });
  });

  it('reads the ID token when the node uses it', () => {
    expect(
      oauth2TokenStatus(o({ tokenSource: 'idToken', accessToken: 'a', idToken: '' }), 1100),
    ).toEqual({ kind: 'none' });
    expect(
      oauth2TokenStatus(o({ tokenSource: 'idToken', accessToken: '', idToken: 'id' }), 1100),
    ).toEqual({ kind: 'valid', expiresAt: 4600 });
  });
});
```

- [ ] **Step 2: Write the failing button tests**

Create `src/components/flow/properties/__tests__/AuthenticateButton.test.tsx`:

```tsx
import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { AuthenticateResult, AuthNode } from '@/lib/flow-auth-preflight';
import { fromPersistedAuth } from '@/lib/persisted-auth';
import type { Auth } from '@/lib/tauri-api';
import { createDeferred } from '@/test/deferred';
import type { AuthState } from '@/types/pane-types';
import { AuthenticateButton } from '../AuthenticateButton';

const preflight = vi.hoisted(() => ({ authenticateAuthNode: vi.fn() }));
vi.mock('@/lib/flow-auth-preflight', () => preflight);

type OAuth2State = NonNullable<AuthState['oauth2']>;
const TOKEN = 'secret-access-token-123456';

const oauthAuth = (scope = ''): Auth =>
  ({
    authType: 'o-auth2',
    flow: 'authorization_code',
    accessTokenUrl: 'https://idp/token',
    authorizationUrl: 'https://idp/authorize',
    credentials: { clientId: 'cid', clientSecret: '{{secret}}' },
    scope,
  }) as unknown as Auth;

const authNode = (auth: Auth = oauthAuth()): AuthNode => ({
  id: 'a1',
  position: { x: 0, y: 0 },
  kind: { kind: 'Auth', label: 'Sign in', auth, applyToInherit: true },
});

const scope = {
  collection: 'api',
  flowName: 'login',
  environmentName: 'dev',
  globalEnvName: 'global',
};

const oauth = (patch: Partial<OAuth2State> = {}): OAuth2State => ({
  ...(fromPersistedAuth(oauthAuth()).oauth2 as OAuth2State),
  ...patch,
});
const now = () => Math.floor(Date.now() / 1000);

const ui = (node = authNode(), state: OAuth2State | undefined = oauth()) => (
  <AuthenticateButton node={node} scope={scope} oauth={state} />
);

describe('AuthenticateButton', () => {
  beforeEach(() => preflight.authenticateAuthNode.mockReset());

  it('offers Authenticate and says there is no token', () => {
    render(ui());
    expect(screen.getByRole('button', { name: 'Authenticate' })).toBeEnabled();
    expect(screen.getByText('No token')).toBeInTheDocument();
  });

  it('shows the expiry time of a valid token and offers to authenticate again', () => {
    render(ui(authNode(), oauth({ accessToken: TOKEN, expiresIn: 3600, tokenAcquiredAt: now() })));
    expect(screen.getByText(/^Token valid until \d{1,2}:\d{2}/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Authenticate again' })).toBeInTheDocument();
  });

  it('shows a valid token without a lifetime as valid', () => {
    render(ui(authNode(), oauth({ accessToken: TOKEN, expiresIn: null })));
    expect(screen.getByText('Token valid')).toBeInTheDocument();
  });

  it('shows an expired token and offers a plain Authenticate', () => {
    render(ui(authNode(), oauth({ accessToken: TOKEN, expiresIn: 60, tokenAcquiredAt: 1 })));
    expect(screen.getByText('Token expired')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Authenticate' })).toBeInTheDocument();
  });

  it('signs in once, shows busy state, and ignores clicks while it is pending', async () => {
    const pending = createDeferred<AuthenticateResult>();
    preflight.authenticateAuthNode.mockReturnValue(pending.promise);
    render(ui());

    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));
    const busy = screen.getByRole('button', { name: 'Signing in…' });
    expect(busy).toBeDisabled();
    expect(busy).toHaveAttribute('aria-busy', 'true');
    await userEvent.click(busy);

    expect(preflight.authenticateAuthNode).toHaveBeenCalledTimes(1);
    expect(preflight.authenticateAuthNode).toHaveBeenCalledWith(
      scope,
      expect.objectContaining({ id: 'a1' }),
      { force: false, shouldWrite: expect.any(Function) },
    );

    await act(async () => pending.resolve({ accessToken: TOKEN, source: 'signed-in' }));
    expect(await screen.findByRole('status')).toHaveTextContent('Signed in.');
    expect(screen.getByRole('button', { name: 'Authenticate' })).toBeEnabled();
  });

  it('forces a new sign-in when a valid token is held', async () => {
    preflight.authenticateAuthNode.mockResolvedValue({ accessToken: TOKEN, source: 'signed-in' });
    render(ui(authNode(), oauth({ accessToken: TOKEN, expiresIn: 3600, tokenAcquiredAt: now() })));

    await userEvent.click(screen.getByRole('button', { name: 'Authenticate again' }));

    expect(preflight.authenticateAuthNode).toHaveBeenCalledWith(
      scope,
      expect.anything(),
      expect.objectContaining({ force: true }),
    );
  });

  it('reports a refresh', async () => {
    preflight.authenticateAuthNode.mockResolvedValue({ accessToken: TOKEN, source: 'refreshed' });
    render(ui());
    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));
    expect(await screen.findByRole('status')).toHaveTextContent('Token refreshed.');
  });

  it('shows a failed sign-in as an alert and lets the user try again', async () => {
    preflight.authenticateAuthNode.mockRejectedValue(
      new Error('Sign-in for Auth node "Sign in" failed: window closed'),
    );
    render(ui());

    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Sign-in for Auth node "Sign in" failed: window closed',
    );
    expect(screen.getByRole('button', { name: 'Authenticate' })).toBeEnabled();
  });

  it('says so when the provider returns no token', async () => {
    preflight.authenticateAuthNode.mockResolvedValue({ source: 'signed-in' });
    render(ui());
    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('The provider returned no token.');
  });

  it('only lets the token be written while the node configuration is unchanged', async () => {
    preflight.authenticateAuthNode.mockReturnValue(new Promise(() => undefined));
    const { rerender } = render(ui());
    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));
    const options = preflight.authenticateAuthNode.mock.calls[0][2] as {
      shouldWrite: () => boolean;
    };

    expect(options.shouldWrite()).toBe(true);
    rerender(ui(authNode(oauthAuth('changed-scope'))));
    expect(options.shouldWrite()).toBe(false);
  });

  it('tells the user when the new token was not saved', async () => {
    preflight.authenticateAuthNode.mockResolvedValue({ source: 'discarded' });
    render(ui());
    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));
    expect(await screen.findByRole('status')).toHaveTextContent(
      'The sign-in settings changed, so the new token was not saved.',
    );
  });

  it('never puts the token in the DOM or in an attribute', async () => {
    preflight.authenticateAuthNode.mockResolvedValue({ accessToken: TOKEN, source: 'signed-in' });
    render(ui(authNode(), oauth({ accessToken: TOKEN, expiresIn: 3600, tokenAcquiredAt: now() })));
    await userEvent.click(screen.getByRole('button', { name: 'Authenticate again' }));
    await screen.findByRole('status');
    expect(screen.getByTestId('authenticate-section').outerHTML).not.toContain(TOKEN);
  });

  it('ignores a result that arrives after the component unmounted', async () => {
    const errors = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const pending = createDeferred<AuthenticateResult>();
    preflight.authenticateAuthNode.mockReturnValue(pending.promise);
    const { unmount } = render(ui());
    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));

    unmount();
    await act(async () => pending.resolve({ accessToken: TOKEN, source: 'signed-in' }));

    expect(errors).not.toHaveBeenCalled();
    errors.mockRestore();
  });
});
```

- [ ] **Step 3: Run to verify they fail**

Run: `yarn test src/lib/__tests__/flow-auth.test.ts src/components/flow/properties/__tests__/AuthenticateButton.test.tsx`
Expected: FAIL (`oauth2TokenStatus` is not exported; the component does not exist).

- [ ] **Step 4: Add the status helper**

In `src/lib/flow-auth.ts`, directly after `isTokenExpired`, add:

```ts
/** What an Auth node holds: no token, a usable one (with its expiry), or an expired one. */
export type Oauth2TokenStatus =
  | { kind: 'none' }
  | { kind: 'valid'; expiresAt: number | null }
  | { kind: 'expired' };

/**
 * The token status of an OAuth2 state, by the token the node sends (the ID token
 * when the node says so). `expiresAt` is in seconds since the epoch, and null when
 * the token has no lifetime. Never returns the token.
 */
export function oauth2TokenStatus(
  oauth: OAuth2State | undefined,
  nowSeconds: number = Math.floor(Date.now() / 1000),
): Oauth2TokenStatus {
  if (!oauth) return { kind: 'none' };
  const token = oauth.tokenSource === 'idToken' ? oauth.idToken : oauth.accessToken;
  if (!token) return { kind: 'none' };
  if (isTokenExpired(oauth, nowSeconds)) return { kind: 'expired' };
  const expiresAt =
    oauth.expiresIn && oauth.tokenAcquiredAt ? oauth.tokenAcquiredAt + oauth.expiresIn : null;
  return { kind: 'valid', expiresAt };
}
```

- [ ] **Step 5: Write the component**

Create `src/components/flow/properties/AuthenticateButton.tsx`:

```tsx
import { KeyRound, Loader2, LogIn } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { type Oauth2TokenStatus, oauth2TokenStatus } from '@/lib/flow-auth';
import {
  type AuthenticateResult,
  type AuthNode,
  type AuthNodeScope,
  authenticateAuthNode,
} from '@/lib/flow-auth-preflight';
import type { AuthState } from '@/types/pane-types';

type OAuth2State = NonNullable<AuthState['oauth2']>;

interface Message {
  kind: 'status' | 'error';
  text: string;
}

function describeStatus(status: Oauth2TokenStatus): string {
  switch (status.kind) {
    case 'none':
      return 'No token';
    case 'expired':
      return 'Token expired';
    case 'valid': {
      if (status.expiresAt === null) return 'Token valid';
      const time = new Date(status.expiresAt * 1000).toLocaleTimeString([], {
        hour: '2-digit',
        minute: '2-digit',
      });
      return `Token valid until ${time}`;
    }
  }
}

function messageFor(result: AuthenticateResult): Message {
  switch (result.source) {
    case 'discarded':
      return {
        kind: 'status',
        text: 'The sign-in settings changed, so the new token was not saved.',
      };
    case 'backend':
      return { kind: 'status', text: 'This grant is fetched when the flow runs.' };
    case 'stored':
      return { kind: 'status', text: 'Already signed in.' };
    case 'refreshed':
      return { kind: 'status', text: 'Token refreshed.' };
    case 'signed-in':
      return result.accessToken
        ? { kind: 'status', text: 'Signed in.' }
        : { kind: 'error', text: 'The provider returned no token.' };
  }
}

/**
 * Signs in an interactive OAuth 2.0 Auth node without starting a run, and shows
 * whether the node holds a valid token. The token stays in the in-memory store;
 * this component never renders it.
 */
export function AuthenticateButton({
  node,
  scope,
  oauth,
}: {
  node: AuthNode;
  scope: AuthNodeScope;
  oauth: OAuth2State | undefined;
}) {
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<Message | null>(null);
  const busyRef = useRef(false);
  const mountedRef = useRef(true);
  // The node's persisted auth as of the latest render. A sign-in compares it with
  // the value at click time, to notice an edit made while the window was open.
  const latestAuthRef = useRef('');
  latestAuthRef.current = JSON.stringify(node.kind.auth);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const status = oauth2TokenStatus(oauth);
  const signedIn = status.kind === 'valid';

  const run = async () => {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    setMessage(null);
    const authAtClick = latestAuthRef.current;
    try {
      const result = await authenticateAuthNode(scope, node, {
        force: signedIn,
        shouldWrite: () => latestAuthRef.current === authAtClick,
      });
      if (mountedRef.current) setMessage(messageFor(result));
    } catch (err) {
      if (mountedRef.current) {
        setMessage({ kind: 'error', text: err instanceof Error ? err.message : String(err) });
      }
    } finally {
      busyRef.current = false;
      if (mountedRef.current) setBusy(false);
    }
  };

  const Icon = signedIn ? KeyRound : LogIn;
  return (
    <div data-testid='authenticate-section' className='space-y-1.5'>
      <div className='flex items-center gap-2'>
        <Button
          type='button'
          size='sm'
          variant={signedIn ? 'outline' : 'default'}
          className='h-8 gap-1.5 text-xs'
          disabled={busy}
          aria-busy={busy}
          onClick={() => void run()}
        >
          {busy ? (
            <Loader2 className='h-3.5 w-3.5 animate-spin' aria-hidden='true' />
          ) : (
            <Icon className='h-3.5 w-3.5' aria-hidden='true' />
          )}
          {busy ? 'Signing in…' : signedIn ? 'Authenticate again' : 'Authenticate'}
        </Button>
        <Badge variant='secondary' className='text-xs font-normal'>
          {describeStatus(status)}
        </Badge>
      </div>
      {message?.kind === 'error' && (
        <p role='alert' className='break-words text-xs text-red-600'>
          {message.text}
        </p>
      )}
      {message?.kind === 'status' && (
        <p role='status' className='text-xs text-muted-foreground'>
          {message.text}
        </p>
      )}
      <p className='text-xs text-muted-foreground'>
        Opens the provider's sign-in page. The token stays in memory and is never saved to the flow
        file.
      </p>
    </div>
  );
}
```

- [ ] **Step 6: Run to verify the tests pass**

Run: `yarn test src/lib/__tests__/flow-auth.test.ts src/components/flow/properties/__tests__/AuthenticateButton.test.tsx`
Expected: PASS.

- [ ] **Step 7: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/lib/flow-auth.ts src/lib/__tests__/flow-auth.test.ts src/components/flow/properties/AuthenticateButton.tsx src/components/flow/properties/__tests__/AuthenticateButton.test.tsx`
Suggested subject: `feat(flow): add the Authenticate button and token status`.

---

### Task 3: Show the button in `AuthNodeEditor`

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src/components/flow/properties/AuthNodeEditor.tsx`
- Modify: `src/components/flow/properties/__tests__/AuthNodeEditor.test.tsx`

**Interfaces:**
- Consumes: `AuthenticateButton`, `AuthNode`, `AuthNodeScope`, `isInteractiveGrant`.

- [ ] **Step 1: Update the test file's mocks**

In `src/components/flow/properties/__tests__/AuthNodeEditor.test.tsx`:

1. Add a mock for the module the button pulls in (below the existing `vi.mock('@/lib/queries/environment-queries', ...)` block). The values match what the editor resolves with, so the stored token's fingerprint matches:

```tsx
vi.mock('@/lib/execute-request', () => ({
  buildOAuth2VarContext: vi.fn(async () => ({
    clientId: 'dev-client',
    tokenUrl: 'https://idp/token',
    tenant: 'acme',
  })),
}));
```

2. In the existing `vi.mock('@/lib/tauri-api', ...)` block, add the two commands next to `getCollectionSettings`:

```tsx
  oauth2GetToken: vi.fn(),
  oauth2RefreshToken: vi.fn(),
```

3. Add imports at the top: `import { createDeferred } from '@/test/deferred';` and `import * as tauriApi from '@/lib/tauri-api';` (the file already imports types from `@/lib/tauri-api`; keep the existing type import and add the namespace import beside it).

4. In the `AuthEditor` stand-in, add one more button after `clear token`:

```tsx
        <button
          type='button'
          onClick={() =>
            props.auth.oauth2 &&
            props.onChange({ ...props.auth, oauth2: { ...props.auth.oauth2, scope: 'changed' } })
          }
        >
          change scope
        </button>
```

- [ ] **Step 2: Write the failing tests**

Append to the same file a new top-level `describe`:

```tsx
describe('AuthNodeEditor Authenticate button', () => {
  const grantKind = (flow: string): AuthKind => ({
    ...kind,
    auth: {
      authType: 'o-auth2',
      flow,
      accessTokenUrl: '{{tokenUrl}}',
      authorizationUrl: 'https://idp/authorize',
      credentials: { clientId: '{{clientId}}', clientSecret: '{{secret}}' },
    } as unknown as Auth,
  });
  const tokenResult = {
    access_token: 'authenticated-token-123456',
    token_type: 'Bearer',
    expires_in: 3600,
    refresh_token: 'refresh-123456',
  };
  const renderGrant = (flow: string) =>
    render(
      <AuthNodeEditor
        kind={grantKind(flow)}
        onChange={vi.fn()}
        collection='api'
        flowName='login'
        nodeId='n1'
      />,
    );

  beforeEach(() => {
    useFlowAuthStore.setState({ auths: {} });
    useEnvStore.setState({ activeEnvId: 'dev', activeCollection: 'api' });
    globalEnvState.name = 'global';
    vi.mocked(tauriApi.oauth2GetToken).mockReset();
  });

  it.each(['authorization_code', 'implicit'])('shows the button for the %s grant', (flow) => {
    renderGrant(flow);
    expect(screen.getByRole('button', { name: 'Authenticate' })).toBeInTheDocument();
    expect(screen.getByText('No token')).toBeInTheDocument();
  });

  it.each(['client_credentials', 'resource_owner_password_credentials'])(
    'shows no button for the %s grant',
    (flow) => {
      renderGrant(flow);
      expect(screen.queryByRole('button', { name: /^Authenticate/ })).not.toBeInTheDocument();
    },
  );

  it('shows no button for a static auth type', () => {
    render(
      <AuthNodeEditor
        kind={kind}
        onChange={vi.fn()}
        collection='api'
        flowName='login'
        nodeId='n1'
      />,
    );
    expect(screen.queryByRole('button', { name: /^Authenticate/ })).not.toBeInTheDocument();
  });

  it('signs in, stores the token under the displayed key and shows the status', async () => {
    vi.mocked(tauriApi.oauth2GetToken).mockResolvedValue(tokenResult);
    renderGrant('authorization_code');
    await waitFor(() =>
      expect(authEditorProps.last?.variableContext?.get('tokenUrl')?.value).toBe(
        'https://idp/token',
      ),
    );

    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));

    expect(await screen.findByRole('status')).toHaveTextContent('Signed in.');
    expect(tauriApi.oauth2GetToken).toHaveBeenCalledWith(
      expect.objectContaining({ grantType: 'authorization_code', clientId: 'dev-client' }),
    );
    // The token sits under the key the editor reads, so the status and the editor see it.
    const entry = useFlowAuthStore
      .getState()
      .getEntry(flowAuthKey('api', 'login', 'n1', 'dev', 'global'));
    expect(entry?.auth.oauth2?.accessToken).toBe('authenticated-token-123456');
    expect(screen.getByText(/^Token valid until/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Authenticate again' })).toBeInTheDocument();
    expect(screen.getByTestId('authenticate-section').outerHTML).not.toContain(
      'authenticated-token-123456',
    );
  });

  it('uses no more than one sign-in for two quick clicks', async () => {
    const pending = createDeferred<typeof tokenResult>();
    vi.mocked(tauriApi.oauth2GetToken).mockReturnValue(pending.promise);
    renderGrant('authorization_code');

    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));
    await userEvent.click(screen.getByRole('button', { name: 'Signing in…' }));

    expect(tauriApi.oauth2GetToken).toHaveBeenCalledTimes(1);
    pending.resolve(tokenResult);
    expect(await screen.findByRole('status')).toBeInTheDocument();
  });

  it('shows a failed sign-in as an alert and stores no token', async () => {
    vi.mocked(tauriApi.oauth2GetToken).mockRejectedValue(new Error('window closed'));
    renderGrant('authorization_code');

    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Sign-in for Auth node "Sign in" failed: window closed',
    );
    expect(
      useFlowAuthStore.getState().getEntry(flowAuthKey('api', 'login', 'n1', 'dev', 'global')),
    ).toBeUndefined();
  });

  it('drops the token when the configuration is edited while the sign-in window is open', async () => {
    const pending = createDeferred<typeof tokenResult>();
    vi.mocked(tauriApi.oauth2GetToken).mockReturnValue(pending.promise);
    // Feeds each reported node back in, as the flow pane does.
    function Harness() {
      const [current, setCurrent] = useState<AuthKind>(grantKind('authorization_code'));
      return (
        <AuthNodeEditor
          kind={current}
          onChange={(k) => setCurrent(k as AuthKind)}
          collection='api'
          flowName='login'
          nodeId='n1'
        />
      );
    }
    render(<Harness />);
    await userEvent.click(screen.getByRole('button', { name: 'Authenticate' }));

    await userEvent.click(screen.getByRole('button', { name: 'change scope' }));
    pending.resolve(tokenResult);

    expect(await screen.findByRole('status')).toHaveTextContent(
      'The sign-in settings changed, so the new token was not saved.',
    );
    const entry = useFlowAuthStore
      .getState()
      .getEntry(flowAuthKey('api', 'login', 'n1', 'dev', 'global'));
    expect(entry?.auth.oauth2?.accessToken ?? '').toBe('');
    expect(screen.getByText('No token')).toBeInTheDocument();
  });
});
```

- [ ] **Step 3: Run to verify they fail**

Run: `yarn test src/components/flow/properties/__tests__/AuthNodeEditor.test.tsx`
Expected: the new `Authenticate button` tests FAIL (no button). The existing tests PASS (the mock additions are inert).

- [ ] **Step 4: Render the button**

In `src/components/flow/properties/AuthNodeEditor.tsx`:

1. Add imports (keep Biome's order):

```tsx
import type { AuthNode, AuthNodeScope } from '@/lib/flow-auth-preflight';
import { AuthenticateButton } from './AuthenticateButton';
```

and add `isInteractiveGrant` to the existing `@/lib/flow-auth` import list.

2. After the `handleAuthChange` definition and before the `return`, add:

```tsx
  // The node and scope the Authenticate button signs in with. The scope holds the
  // same environment names as `key` above, so the token lands where `stored` reads it.
  const authNode = useMemo<AuthNode>(
    () => ({ id: nodeId, kind, position: { x: 0, y: 0 } }),
    [nodeId, kind],
  );
  const authScope = useMemo<AuthNodeScope>(
    () => ({
      collection,
      flowName,
      environmentName: activeEnvId ?? undefined,
      globalEnvName: globalEnvName ?? undefined,
    }),
    [collection, flowName, activeEnvId, globalEnvName],
  );
```

3. In the JSX, directly after the closing `/>` of `<AuthEditor ... />` (the last child of the root `div`), add:

```tsx
      {isInteractiveGrant(kind.auth) && (
        <AuthenticateButton node={authNode} scope={authScope} oauth={state.oauth2} />
      )}
```

- [ ] **Step 5: Run to verify the tests pass**

Run: `yarn test src/components/flow src/lib/__tests__/flow-auth-preflight.test.ts`
Expected: PASS, including the unchanged token-fingerprint tests of `AuthNodeEditor` and the new button tests.

- [ ] **Step 6: Gates, manual check and commit**

Run: `yarn tsc --noEmit && yarn check && yarn test src/components/flow src/lib src/hooks`
Expected: all pass.

Manual check once in the real app (`yarn tauri dev`) against an identity provider you can sign in to: add an Auth node, choose OAuth 2.0 with the authorization code grant, click Authenticate, complete the sign-in. The badge reads `Token valid until HH:MM`. Run the flow: no second sign-in prompt appears (the pre-run step reuses the stored token). Click `Authenticate again`: the sign-in window opens again. Record the result in the review note.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/components/flow/properties/AuthNodeEditor.tsx src/components/flow/properties/__tests__/AuthNodeEditor.test.tsx`
Suggested subject: `feat(flow): sign in an Auth node from its properties panel`.

---

## Self-Review

- **Spec coverage:** Authenticate button, token status (None, Valid, Expired), success line and error alert (Tasks 2 and 3). Extraction with the same store-write semantics (Task 1). F-10 race: `shouldWrite` plus the UI test. D4 stated and reversible.
- **Placeholders:** none. Every step shows code or an exact edit.
- **Type consistency:** `AuthNode`, `AuthNodeScope`, `AuthenticateResult`, `AuthenticateOptions` are defined in Task 1 and used with the same names in Tasks 2 and 3. `oauth2TokenStatus` returns `Oauth2TokenStatus`, matched by `describeStatus`. `AuthenticateButton` props are `node`, `scope`, `oauth` in the component, its tests and the editor.
- **Review Focus coverage:** item 1 is the two `shouldWrite` tests in Task 1, the `only lets the token be written` test in Task 2 and the `drops the token` test in Task 3; item 2 is the busy tests in Tasks 2 and 3; item 3 is the unchanged original preflight tests plus the call-count test; item 4 is `never puts the token in the DOM` and the `outerHTML` check in Task 3; item 5 is the `shows no button` tests and the stored-key assertion.
- **Known gaps:** a sign-in error message comes from the provider and is shown as text; the redaction pass of plan P15 is not applied to it. The browser sign-in itself (window, callback) is not exercised by tests, only by the manual check.
