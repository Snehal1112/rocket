# Flow Auth Node — Plan 5: Frontend node, editor and wiring

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 5 of 7.** Previous plan: `docs/superpowers/plans/2026-10-02-flow-auth-node-04-ipc.md` (must be merged and green).
**Next plan to run: `docs/superpowers/plans/2026-10-02-flow-auth-node-06-preflight.md`**
**Recommended model: Sonnet.**

**Goal:** Users can add an Auth node to a flow, configure any auth type with the existing Authentication editor (including "Get New Access Token" for OAuth2), toggle "apply to inherited auth", and wire the node into a request's new `auth` handle. Tokens fetched in the editor stay in memory only.

**Architecture:** An in-memory Zustand store (`flow-auth-store`) holds the full `AuthState` (with any fetched token) per `collection::flow::node`. The persisted node keeps only the auth configuration (`toPersistedAuth`). A pure helper clears a stored token whenever the configuration changes, so a stale token for an old client id is never reused. Plan 6 reads this store for the pre-run prompt.

**Tech Stack:** React 18, TypeScript, Zustand, shadcn/ui, lucide-react, React Flow (`@xyflow/react`), Vitest + Testing Library.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-10-02-flow-auth-node-design.md`.
- shadcn/ui primitives only (no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`); icons from `lucide-react` only; no inline SVG.
- Single-line variable-aware fields use `SingleLineEditor` (already inside `AuthEditor`); never Monaco for them.
- Zustand: never fully destructure store state at the top of a component; use narrow selectors.
- A token is never persisted: the node's `auth` goes through `toPersistedAuth`, which has no token field.
- Conventional commits.
- If `yarn check` reports formatting or import-order findings, run `yarn lint` (auto-fix), then re-run `yarn check`.
- Verification per task: `yarn tsc --noEmit`, `yarn check`, the task's `yarn test <path>`.

## File Structure

| File | Change |
|---|---|
| `src/lib/flow-auth.ts` | **Create**: key, labels, interactive-grant check, token reset, defaults |
| `src/stores/flow-auth-store.ts` | **Create**: in-memory `AuthState` per node |
| `src/components/flow/nodes/AuthNode.tsx` | **Create**: canvas node |
| `src/components/flow/FlowCanvas.tsx` | Register `Auth` node type |
| `src/components/flow/NodePalette.tsx` | "Auth" menu item |
| `src/components/flow/nodes/RequestNode.tsx` | `auth` target handle row |
| `src/components/flow/properties/AuthNodeEditor.tsx` | **Create**: properties editor |
| `src/components/flow/properties/NodePropertiesPanel.tsx` | Use `AuthNodeEditor`, accept `flowName` |
| `src/components/flow/FlowPane.tsx` | Pass `flowName` to the panel |
| Tests | `src/lib/__tests__/flow-auth.test.ts`, `src/stores/__tests__/flow-auth-store.test.ts`, node/editor/palette tests |

---

### Task 1: Helpers and in-memory store

**Files:**
- Create: `src/lib/flow-auth.ts`
- Create: `src/stores/flow-auth-store.ts`
- Test: `src/lib/__tests__/flow-auth.test.ts`
- Test: `src/stores/__tests__/flow-auth-store.test.ts`

**Interfaces:**
- Consumes: `fromPersistedAuth`, `toPersistedAuth` (`@/lib/persisted-auth`), `Auth` (`@/lib/tauri-api`), `AuthState` (`@/types/pane-types`).
- Produces (used by Tasks 2–3 and Plan 6):

```ts
export function flowAuthKey(collection: string, flowName: string, nodeId: string): string;
export function describeAuth(auth: Auth): string;
export function isInteractiveGrant(auth: Auth): boolean;
export function isOAuth2(auth: Auth): boolean;
export function resetTokenOnConfigChange(prev: AuthState | undefined, next: AuthState): AuthState;
export const DEFAULT_AUTH_NODE_AUTH: Auth;
export function isTokenExpired(oauth: NonNullable<AuthState['oauth2']>, nowSeconds?: number): boolean;
export const useFlowAuthStore: UseBoundStore<StoreApi<{
  auths: Record<string, AuthState>;
  setAuth(key: string, auth: AuthState): void;
  getAuth(key: string): AuthState | undefined;
  clearAuth(key: string): void;
}>>;
```

- [ ] **Step 1: Write the failing helper tests**

Create `src/lib/__tests__/flow-auth.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import {
  DEFAULT_AUTH_NODE_AUTH,
  describeAuth,
  flowAuthKey,
  isInteractiveGrant,
  isOAuth2,
  isTokenExpired,
  resetTokenOnConfigChange,
} from '@/lib/flow-auth';
import { fromPersistedAuth } from '@/lib/persisted-auth';
import type { Auth } from '@/lib/tauri-api';
import type { AuthState } from '@/types/pane-types';

const oauth = (flow: string): Auth =>
  ({
    authType: 'o-auth2',
    flow,
    accessTokenUrl: 'https://idp.example.com/token',
    credentials: { clientId: 'cid', clientSecret: 'secret' },
  }) as unknown as Auth;

describe('flowAuthKey', () => {
  it('joins collection, flow and node so tokens never cross flows', () => {
    expect(flowAuthKey('api', 'login', 'n1')).toBe('api::login::n1');
    expect(flowAuthKey('api', 'login', 'n1')).not.toBe(flowAuthKey('api', 'other', 'n1'));
  });
});

describe('describeAuth', () => {
  it('names the type and, for OAuth2, the grant', () => {
    expect(describeAuth({ authType: 'bearer', token: 't' })).toBe('Bearer');
    expect(describeAuth(oauth('client_credentials'))).toBe('OAuth 2.0 · client credentials');
    expect(describeAuth(oauth('authorization_code'))).toBe('OAuth 2.0 · authorization code');
  });
});

describe('isInteractiveGrant / isOAuth2', () => {
  it('is true only for authorization code and implicit', () => {
    expect(isInteractiveGrant(oauth('authorization_code'))).toBe(true);
    expect(isInteractiveGrant(oauth('implicit'))).toBe(true);
    expect(isInteractiveGrant(oauth('client_credentials'))).toBe(false);
    expect(isInteractiveGrant(oauth('resource_owner_password_credentials'))).toBe(false);
    expect(isInteractiveGrant({ authType: 'bearer', token: 't' })).toBe(false);
  });

  it('isOAuth2 matches only OAuth2', () => {
    expect(isOAuth2(oauth('implicit'))).toBe(true);
    expect(isOAuth2({ authType: 'bearer', token: 't' })).toBe(false);
  });
});

describe('DEFAULT_AUTH_NODE_AUTH', () => {
  it('is a concrete auth type, never none or inherit', () => {
    expect(['none', 'inherit']).not.toContain(DEFAULT_AUTH_NODE_AUTH.authType);
  });
});

describe('resetTokenOnConfigChange', () => {
  const withToken = (clientId: string): AuthState => {
    const base = fromPersistedAuth({
      authType: 'o-auth2',
      flow: 'client_credentials',
      accessTokenUrl: 'https://idp.example.com/token',
      credentials: { clientId, clientSecret: 's' },
    } as unknown as Auth);
    return {
      ...base,
      oauth2: {
        ...(base.oauth2 as NonNullable<AuthState['oauth2']>),
        accessToken: 'tok-123456',
        refreshToken: 'ref-123456',
        expiresIn: 3600,
        tokenAcquiredAt: 1000,
      },
    };
  };

  it('keeps a token when only the token fields changed', () => {
    const prev = withToken('cid');
    const next = {
      ...prev,
      oauth2: { ...(prev.oauth2 as NonNullable<AuthState['oauth2']>), accessToken: 'tok-999999' },
    };
    expect(resetTokenOnConfigChange(prev, next).oauth2?.accessToken).toBe('tok-999999');
  });

  it('drops the token when the configuration changed', () => {
    const prev = withToken('cid');
    const next = withToken('other-client');
    const result = resetTokenOnConfigChange(prev, next);
    expect(result.oauth2?.accessToken).toBe('');
    expect(result.oauth2?.refreshToken).toBe('');
    expect(result.oauth2?.expiresIn).toBeNull();
    expect(result.oauth2?.tokenAcquiredAt).toBeNull();
    expect(result.oauth2?.clientId).toBe('other-client');
  });

  it('passes through when there was no previous state or no OAuth2', () => {
    const next = withToken('cid');
    expect(resetTokenOnConfigChange(undefined, next)).toBe(next);
    const basic: AuthState = { authType: 'basic', basic: { username: 'u', password: 'p' } };
    expect(resetTokenOnConfigChange(basic, basic)).toBe(basic);
  });
});

describe('isTokenExpired', () => {
  const o = (patch: Partial<NonNullable<AuthState['oauth2']>>) =>
    ({ accessToken: 'tok', expiresIn: 60, tokenAcquiredAt: 1000, ...patch }) as NonNullable<
      AuthState['oauth2']
    >;

  it('is false without a token lifetime (cannot tell, so try it)', () => {
    expect(isTokenExpired(o({ expiresIn: null }), 5000)).toBe(false);
    expect(isTokenExpired(o({ tokenAcquiredAt: null }), 5000)).toBe(false);
  });

  it('is true once acquired + expiresIn has passed, with a 30 second margin', () => {
    expect(isTokenExpired(o({}), 1000 + 29)).toBe(false);
    expect(isTokenExpired(o({}), 1000 + 31)).toBe(true);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `yarn test src/lib/__tests__/flow-auth.test.ts`
Expected: FAIL (module not found).

- [ ] **Step 3: Implement `src/lib/flow-auth.ts`**

```ts
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
```

- [ ] **Step 4: Run the helper tests**

Run: `yarn test src/lib/__tests__/flow-auth.test.ts`
Expected: PASS.

- [ ] **Step 5: Write the failing store test**

Create `src/stores/__tests__/flow-auth-store.test.ts`:

```ts
import { beforeEach, describe, expect, it } from 'vitest';
import { useFlowAuthStore } from '@/stores/flow-auth-store';

describe('flow-auth-store', () => {
  beforeEach(() => {
    useFlowAuthStore.setState({ auths: {} });
  });

  it('stores and returns an auth state by key', () => {
    useFlowAuthStore.getState().setAuth('k1', { authType: 'bearer', bearer: { token: 't' } });
    expect(useFlowAuthStore.getState().getAuth('k1')).toEqual({
      authType: 'bearer',
      bearer: { token: 't' },
    });
    expect(useFlowAuthStore.getState().getAuth('missing')).toBeUndefined();
  });

  it('clears one key without touching the others', () => {
    const { setAuth, clearAuth, getAuth } = useFlowAuthStore.getState();
    setAuth('k1', { authType: 'none' });
    setAuth('k2', { authType: 'none' });
    clearAuth('k1');
    expect(getAuth('k1')).toBeUndefined();
    expect(getAuth('k2')).toEqual({ authType: 'none' });
  });

  it('is never persisted to browser storage', () => {
    useFlowAuthStore.getState().setAuth('k1', { authType: 'bearer', bearer: { token: 'secret-1' } });
    expect(JSON.stringify({ ...localStorage })).not.toContain('secret-1');
    expect(JSON.stringify({ ...sessionStorage })).not.toContain('secret-1');
  });
});
```

- [ ] **Step 6: Run to verify it fails, then implement the store**

Run: `yarn test src/stores/__tests__/flow-auth-store.test.ts` (FAIL: module not found).

Create `src/stores/flow-auth-store.ts`:

```ts
import { create } from 'zustand';
import type { AuthState } from '@/types/pane-types';

// In-memory only. Holds each Auth node's full AuthState, including any fetched
// OAuth2 token, keyed by `flowAuthKey(collection, flow, node)`. Nothing here is
// persisted: no `persist` middleware, no storage, lost on reload by design.
interface FlowAuthStore {
  auths: Record<string, AuthState>;
  setAuth: (key: string, auth: AuthState) => void;
  getAuth: (key: string) => AuthState | undefined;
  clearAuth: (key: string) => void;
}

export const useFlowAuthStore = create<FlowAuthStore>()((set, get) => ({
  auths: {},

  setAuth(key, auth) {
    set({ auths: { ...get().auths, [key]: auth } });
  },

  getAuth(key) {
    return get().auths[key];
  },

  clearAuth(key) {
    const { [key]: _removed, ...rest } = get().auths;
    set({ auths: rest });
  },
}));
```

- [ ] **Step 7: Run the checks**

Run: `yarn test src/lib/__tests__/flow-auth.test.ts src/stores/__tests__/flow-auth-store.test.ts && yarn tsc --noEmit && yarn check`
Expected: all PASS.

- [ ] **Step 8: Commit**

```bash
git add src/lib/flow-auth.ts src/stores/flow-auth-store.ts src/lib/__tests__/flow-auth.test.ts src/stores/__tests__/flow-auth-store.test.ts
git commit -m "feat(flow): add in-memory Auth node state and helpers"
```

---

### Task 2: Auth node on the canvas, palette entry, `auth` handle on requests

**Files:**
- Create: `src/components/flow/nodes/AuthNode.tsx`
- Modify: `src/components/flow/FlowCanvas.tsx`
- Modify: `src/components/flow/NodePalette.tsx`
- Modify: `src/components/flow/nodes/RequestNode.tsx`
- Test: `src/components/flow/nodes/__tests__/AuthNode.test.tsx`
- Test: `src/components/flow/__tests__/NodePalette.test.tsx`
- Test: `src/components/flow/nodes/__tests__/RequestNode.test.tsx`

**Interfaces:**
- Consumes: `describeAuth`, `DEFAULT_AUTH_NODE_AUTH` (Task 1), `AUTH_HANDLE`, `RESULT_HANDLE` (`@/lib/flow-handles`).
- Produces: `AuthNodeData` and an `Auth` entry in `nodeTypes`; palette creates `{ kind: 'Auth', label: 'New Auth', auth: DEFAULT_AUTH_NODE_AUTH, applyToInherit: true }` with id `auth-…`; Request nodes expose a fifth target handle `auth`.

- [ ] **Step 1: Write the failing node test**

Create `src/components/flow/nodes/__tests__/AuthNode.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { ReactFlowProvider } from '@xyflow/react';
import { describe, expect, it, vi } from 'vitest';
import { AuthNode, type AuthNodeData } from '../AuthNode';
import { FlowNodeActionsContext } from '../FlowNodeActionsContext';

const kind = {
  kind: 'Auth' as const,
  label: 'Sign in',
  auth: { authType: 'bearer' as const, token: 't' },
  applyToInherit: true,
};

function renderAuth(data: AuthNodeData) {
  const actions = { updateNodeKind: vi.fn(), removeSwitchCase: vi.fn(), openProperties: vi.fn() };
  render(
    <ReactFlowProvider>
      <FlowNodeActionsContext.Provider value={actions}>
        <AuthNode
          id='a1'
          type='Auth'
          data={data}
          selected={false}
          dragging={false}
          zIndex={0}
          isConnectable
          draggable
          selectable
          deletable
          positionAbsoluteX={0}
          positionAbsoluteY={0}
        />
      </FlowNodeActionsContext.Provider>
    </ReactFlowProvider>,
  );
}

describe('AuthNode', () => {
  it('shows the label, the auth summary and one result handle, no inputs', () => {
    renderAuth({ kind, status: 'idle' });
    const card = screen.getByTestId('auth-node-card');
    expect(screen.getByText('Sign in')).toBeInTheDocument();
    expect(screen.getByTestId('auth-node-summary')).toHaveTextContent('Bearer');
    expect(card.querySelectorAll('.react-flow__handle.target')).toHaveLength(0);
    const sources = [...card.querySelectorAll('.react-flow__handle.source')].map((h) =>
      h.getAttribute('data-handleid'),
    );
    expect(sources).toEqual(['result']);
  });

  it('says when it applies to inherited auth', () => {
    renderAuth({ kind, status: 'idle' });
    expect(screen.getByTestId('auth-node-applies')).toHaveTextContent('Applies to inherited auth');
  });

  it('does not say so when it does not apply', () => {
    renderAuth({ kind: { ...kind, applyToInherit: false }, status: 'idle' });
    expect(screen.queryByTestId('auth-node-applies')).not.toBeInTheDocument();
  });

  it('never renders token or secret values', () => {
    renderAuth({
      kind: { ...kind, auth: { authType: 'bearer', token: 'super-secret-token' } },
      status: 'idle',
    });
    expect(screen.queryByText(/super-secret-token/)).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `yarn test src/components/flow/nodes/__tests__/AuthNode.test.tsx`
Expected: FAIL (module not found).

- [ ] **Step 3: Create the node component**

Create `src/components/flow/nodes/AuthNode.tsx`:

```tsx
import { Handle, type NodeProps, Position } from '@xyflow/react';
import { KeyRound } from 'lucide-react';
import { describeAuth } from '@/lib/flow-auth';
import { RESULT_HANDLE } from '@/lib/flow-handles';
import type { FlowNodeKind, FlowNodeStatus, FlowSkipReason } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { NodeMenuButton } from './NodeMenuButton';
import { NodeStatusCaption } from './NodeStatusCaption';
import { nodeStatusClassName } from './nodeStatus';

export type AuthNodeData = {
  kind: Extract<FlowNodeKind, { kind: 'Auth' }>;
  status: FlowNodeStatus;
  error?: string;
  progress?: string;
  skipReason?: FlowSkipReason;
  /** Set when a save was rejected because of this node. */
  hasCycleError?: boolean;
};

// An Auth node has no inputs. It shows only what kind of auth it holds, never
// a credential value.
export function AuthNode({ id, data, isConnectable }: NodeProps & { data: AuthNodeData }) {
  const { kind, status } = data;
  return (
    <div
      data-testid='auth-node-card'
      data-status={status}
      className={cn(
        'w-56 rounded-md border bg-card text-card-foreground text-xs shadow-sm',
        nodeStatusClassName(status, data.skipReason),
        data.hasCycleError && 'ring-2 ring-red-500',
      )}
    >
      <div className='flex items-center gap-1.5 border-b px-2 py-1.5'>
        <KeyRound className='h-3.5 w-3.5 shrink-0 text-muted-foreground' aria-hidden='true' />
        <span className='font-mono text-[10px] text-muted-foreground'>Auth</span>
        <span className='truncate font-medium'>{kind.label}</span>
        <NodeMenuButton nodeId={id} label={kind.label} />
      </div>

      <NodeStatusCaption
        status={status}
        skipReason={data.skipReason}
        error={data.error}
        progress={data.progress}
      />

      <div className='space-y-0.5 px-2 py-1.5'>
        <p data-testid='auth-node-summary' className='truncate'>
          {describeAuth(kind.auth)}
        </p>
        {kind.applyToInherit && (
          <p data-testid='auth-node-applies' className='truncate text-muted-foreground'>
            Applies to inherited auth
          </p>
        )}
      </div>

      <div className='relative flex justify-end px-2 pb-1.5 pr-4'>
        <span className='text-muted-foreground'>result</span>
        <Handle
          type='source'
          id={RESULT_HANDLE}
          position={Position.Right}
          isConnectable={isConnectable}
          className='!h-2 !w-2'
        />
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Register the node type**

In `src/components/flow/FlowCanvas.tsx`, add `import { AuthNode } from './nodes/AuthNode';` next to the other node imports and add `Auth: AuthNode,` to the `nodeTypes` object.

- [ ] **Step 5: Write the failing palette test**

Append inside the `describe` of `src/components/flow/__tests__/NodePalette.test.tsx`:

```tsx
  it('adds an Auth node that applies to inherited auth by default', async () => {
    const onAddNode = vi.fn();
    render(<NodePalette onAddNode={onAddNode} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /Add node/ }));
    await user.click(screen.getByRole('menuitem', { name: 'Auth' }));
    const node = onAddNode.mock.calls[0][0];
    expect(node.id).toMatch(/^auth-/);
    expect(node.position).toEqual({ x: 100, y: 100 });
    expect(node.kind).toEqual({
      kind: 'Auth',
      label: 'New Auth',
      auth: { authType: 'bearer', token: '' },
      applyToInherit: true,
    });
  });
```

- [ ] **Step 6: Add the palette entry**

In `src/components/flow/NodePalette.tsx`: add `KeyRound` to the `lucide-react` import list (keep it alphabetical: after `Hourglass`), add `import { DEFAULT_AUTH_NODE_AUTH } from '@/lib/flow-auth';` with the other `@/lib` imports, and add this item inside `<DropdownMenuContent>` after the last existing `<DropdownMenuItem>`:

```tsx
          <DropdownMenuItem
            onSelect={() =>
              onAddNode({
                id: newNodeId('auth'),
                kind: {
                  kind: 'Auth',
                  label: 'New Auth',
                  auth: DEFAULT_AUTH_NODE_AUTH,
                  applyToInherit: true,
                },
                position: defaultPosition,
              })
            }
          >
            <KeyRound className='mr-2 h-3.5 w-3.5' aria-hidden='true' />
            Auth
          </DropdownMenuItem>
```

- [ ] **Step 7: Update and extend the RequestNode test, then add the handle**

In `src/components/flow/nodes/__tests__/RequestNode.test.tsx`, change the existing assertion in `exposes trigger, url, headers, and body target handles plus one result source handle` from

```tsx
    expect(targets).toEqual(['trigger', 'url', 'headers', 'body']);
```

to

```tsx
    expect(targets).toEqual(['trigger', 'url', 'headers', 'body', 'auth']);
```

and rename the test title to `exposes trigger, url, headers, body and auth target handles plus one result source handle`.

In `src/components/flow/nodes/RequestNode.tsx`, add `AUTH_HANDLE` to the `@/lib/flow-handles` import, and add this row directly after the Body row (the `<div ...>` containing `id='body'`) and before the `{/* Repeat until has no handle ... */}` comment:

```tsx
        <div
          data-testid='request-node-auth-row'
          className='relative flex items-center gap-1.5 pl-2'
        >
          <Handle
            type='target'
            id={AUTH_HANDLE}
            position={Position.Left}
            isConnectable={isConnectable}
            className='!h-2 !w-2'
          />
          <span className='text-muted-foreground'>Auth</span>
        </div>
```

- [ ] **Step 8: Run the checks**

Run: `yarn test src/components/flow && yarn tsc --noEmit && yarn check`
Expected: all PASS. If another existing flow test counts a Request node's handles or rows, update it the same way as in Step 7 and mention it in the commit message.

- [ ] **Step 9: Commit**

```bash
git add src/components/flow/nodes/AuthNode.tsx src/components/flow/FlowCanvas.tsx src/components/flow/NodePalette.tsx src/components/flow/nodes/RequestNode.tsx src/components/flow/nodes/__tests__ src/components/flow/__tests__/NodePalette.test.tsx
git commit -m "feat(flow): add the Auth node to the canvas and an auth handle to requests"
```

---

### Task 3: Auth node properties editor

**Files:**
- Create: `src/components/flow/properties/AuthNodeEditor.tsx`
- Modify: `src/components/flow/properties/NodePropertiesPanel.tsx`
- Modify: `src/components/flow/FlowPane.tsx`
- Test: `src/components/flow/properties/__tests__/AuthNodeEditor.test.tsx`

**Interfaces:**
- Consumes: `AuthEditor` (`@/components/request/AuthEditor`), `fromPersistedAuth` / `toPersistedAuth`, `flowAuthKey`, `resetTokenOnConfigChange`, `useFlowAuthStore`, `useEnvStore`.
- Produces: `AuthNodeEditor({ kind, onChange, collection, flowName, nodeId })`; `NodePropertiesPanel` gains an optional `flowName?: string` prop.

- [ ] **Step 1: Write the failing editor test**

Create `src/components/flow/properties/__tests__/AuthNodeEditor.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { flowAuthKey } from '@/lib/flow-auth';
import type { FlowNodeKind } from '@/lib/tauri-api';
import { useFlowAuthStore } from '@/stores/flow-auth-store';
import type { AuthState } from '@/types/pane-types';
import { AuthNodeEditor } from '../AuthNodeEditor';

// The real AuthEditor pulls in CodeMirror and OAuth2 sections. A small stand-in
// with the same props lets these tests drive onChange directly.
vi.mock('@/components/request/AuthEditor', () => ({
  AuthEditor: (props: { auth: AuthState; onChange: (a: AuthState) => void }) => (
    <div>
      <span data-testid='auth-type'>{props.auth.authType}</span>
      <button
        type='button'
        onClick={() =>
          props.onChange({ authType: 'bearer', bearer: { token: 'typed-token-123456' } })
        }
      >
        make bearer
      </button>
    </div>
  ),
}));

type AuthKind = Extract<FlowNodeKind, { kind: 'Auth' }>;
const kind: AuthKind = {
  kind: 'Auth',
  label: 'Sign in',
  auth: { authType: 'basic', username: 'u', password: 'p' },
  applyToInherit: true,
};

describe('AuthNodeEditor', () => {
  beforeEach(() => {
    useFlowAuthStore.setState({ auths: {} });
  });

  it('shows the label and loads the persisted auth into the editor', () => {
    render(<AuthNodeEditor kind={kind} onChange={vi.fn()} collection='api' flowName='login' nodeId='n1' />);
    expect(screen.getByLabelText('Label')).toHaveValue('Sign in');
    expect(screen.getByTestId('auth-type')).toHaveTextContent('basic');
  });

  it('reports the whole node with a persisted auth when the auth changes', async () => {
    const onChange = vi.fn();
    render(<AuthNodeEditor kind={kind} onChange={onChange} collection='api' flowName='login' nodeId='n1' />);
    await userEvent.click(screen.getByRole('button', { name: 'make bearer' }));
    expect(onChange).toHaveBeenLastCalledWith({
      ...kind,
      auth: { authType: 'bearer', token: 'typed-token-123456' },
    });
  });

  it('keeps the full state, with any token, only in the in-memory store', async () => {
    render(<AuthNodeEditor kind={kind} onChange={vi.fn()} collection='api' flowName='login' nodeId='n1' />);
    await userEvent.click(screen.getByRole('button', { name: 'make bearer' }));
    const stored = useFlowAuthStore.getState().getAuth(flowAuthKey('api', 'login', 'n1'));
    expect(stored?.authType).toBe('bearer');
  });

  it('toggles apply to inherited auth', async () => {
    const onChange = vi.fn();
    render(<AuthNodeEditor kind={kind} onChange={onChange} collection='api' flowName='login' nodeId='n1' />);
    await userEvent.click(screen.getByRole('switch', { name: 'Apply to inherited auth' }));
    expect(onChange).toHaveBeenLastCalledWith({ ...kind, applyToInherit: false });
  });

  it('edits the label', async () => {
    const onChange = vi.fn();
    render(<AuthNodeEditor kind={kind} onChange={onChange} collection='api' flowName='login' nodeId='n1' />);
    await userEvent.type(screen.getByLabelText('Label'), '!');
    expect(onChange).toHaveBeenLastCalledWith({ ...kind, label: 'Sign in!' });
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `yarn test src/components/flow/properties/__tests__/AuthNodeEditor.test.tsx`
Expected: FAIL (module not found).

- [ ] **Step 3: Create the editor**

Create `src/components/flow/properties/AuthNodeEditor.tsx`:

```tsx
import { useCallback, useMemo } from 'react';
import { AuthEditor } from '@/components/request/AuthEditor';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { flowAuthKey, resetTokenOnConfigChange } from '@/lib/flow-auth';
import { fromPersistedAuth, toPersistedAuth } from '@/lib/persisted-auth';
import type { FlowNodeKind } from '@/lib/tauri-api';
import { useEnvStore } from '@/stores/env-store';
import { useFlowAuthStore } from '@/stores/flow-auth-store';
import type { AuthState } from '@/types/pane-types';
import { LabelField } from './LabelField';

type AuthKind = Extract<FlowNodeKind, { kind: 'Auth' }>;

export function AuthNodeEditor({
  kind,
  onChange,
  collection,
  flowName,
  nodeId,
}: {
  kind: AuthKind;
  onChange: (kind: FlowNodeKind) => void;
  collection: string;
  flowName: string;
  nodeId: string;
}) {
  const key = flowAuthKey(collection, flowName, nodeId);
  const stored = useFlowAuthStore((s) => s.auths[key]);
  const setAuth = useFlowAuthStore((s) => s.setAuth);
  const environmentName = useEnvStore((s) => s.activeEnvId) ?? undefined;

  // The store holds the full state, including a fetched token. A node that was
  // never edited in this session falls back to its persisted configuration.
  const state: AuthState = useMemo(() => stored ?? fromPersistedAuth(kind.auth), [stored, kind.auth]);

  const handleAuthChange = useCallback(
    (next: AuthState) => {
      // A token fetched for the old configuration must not outlive an edit to it.
      const safe = resetTokenOnConfigChange(state, next);
      setAuth(key, safe);
      // Only the configuration is persisted: toPersistedAuth has no token field.
      onChange({ ...kind, auth: toPersistedAuth(safe) });
    },
    [key, kind, onChange, setAuth, state],
  );

  return (
    <div className='space-y-3'>
      <LabelField value={kind.label} onChange={(label) => onChange({ ...kind, label })} />

      <div className='flex items-start justify-between gap-3'>
        <div className='space-y-0.5'>
          <Label htmlFor='auth-node-apply' className='text-xs'>
            Apply to inherited auth
          </Label>
          <p className='text-xs text-muted-foreground'>
            Every request in this flow whose auth is set to inherit uses this credential. A
            request with its own auth keeps it.
          </p>
        </div>
        <Switch
          id='auth-node-apply'
          aria-label='Apply to inherited auth'
          checked={kind.applyToInherit}
          onCheckedChange={(applyToInherit) => onChange({ ...kind, applyToInherit })}
        />
      </div>

      <AuthEditor
        auth={state}
        onChange={handleAuthChange}
        collection={collection}
        environmentName={environmentName}
      />
    </div>
  );
}
```

The store key is built from `collection`, `flowName` and `nodeId`, so a duplicated flow never shares tokens with the original.

- [ ] **Step 4: Use it in the properties panel**

In `src/components/flow/properties/NodePropertiesPanel.tsx`:
- add `import { AuthNodeEditor } from './AuthNodeEditor';`
- extend `editorFor` with a `flowName: string` parameter right after `collection` and update its call site accordingly (the panel passes its `flowName ?? ''` prop);
- replace the placeholder `case 'Auth':` (added in Plan 1) with:

```tsx
    case 'Auth':
      return (
        <AuthNodeEditor
          // Keyed by node, so one node's editor state never carries to another.
          key={node.id}
          kind={kind}
          nodeId={node.id}
          collection={collection}
          flowName={flowName}
          onChange={onChange}
        />
      );
```

- add an optional prop to `NodePropertiesPanel`'s props and type: `flowName = '',` in the destructuring list and `flowName?: string;` in the type.

In `src/components/flow/FlowPane.tsx`, in the `<NodePropertiesPanel ... />` element, add `flowName={flowName}` after `collection={collectionName}`. (`flowName` is the variable FlowPane already uses in `onStepLogs`; if it is named differently in scope, use `tab.flowName ?? ''`.)

- [ ] **Step 5: Run the checks**

Run: `yarn test src/components/flow && yarn tsc --noEmit && yarn check`
Expected: all PASS, including the existing `NodePropertiesPanel` and `FlowPane` tests (they do not pass `flowName`, which defaults to `''`).

- [ ] **Step 6: Commit**

```bash
git add src/components/flow/properties/AuthNodeEditor.tsx src/components/flow/properties/NodePropertiesPanel.tsx src/components/flow/FlowPane.tsx src/components/flow/properties/__tests__/AuthNodeEditor.test.tsx
git commit -m "feat(flow): edit Auth nodes with the Authentication editor"
```

---

**End of Plan 5.** Verify: `yarn tsc --noEmit`, `yarn check`, `yarn test src/components/flow src/lib src/stores`.

**Next plan to run: `docs/superpowers/plans/2026-10-02-flow-auth-node-06-preflight.md`** (pre-run authentication prompt/refresh and passing tokens to `runFlow`).
