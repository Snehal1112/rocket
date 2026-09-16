# Collection Runner Plan B: Request Execution Helper — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the runner a way to resolve variables and execute one
request from the collection tree, with identical behavior to sending
that same request from an open tab (same `{{var}}` resolution, same
"empty auth inherits the collection's auth" semantics).

**Architecture:** `src/lib/execute-request.ts`'s `resolveRequestFields(tabId, request)`
currently derives `collection`/`requestPath` from an open tab via
`findTabInTree`, then runs a 7-scope variable resolution pipeline. This
plan extracts that pipeline into a tab-independent
`resolveRequestFieldsForPath(collection, requestPath, request)`, with
`resolveRequestFields` becoming a two-line wrapper around it (a pure,
behavior-preserving refactor — same inputs still produce the same
outputs for the existing caller). A new `src/lib/runner-execute.ts` then
converts a backend `Request` (from `getCollection()`) into a
`RequestState` via the existing `mapApiRequestToState(req, true)` —
which is what already gives `authType: 'none'` the same "inherit the
collection's auth" meaning a sidebar-opened tab gets — resolves it with
the extracted function, and calls `executeRequest`.

**Tech Stack:** TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-09-17-collection-runner-frontend-design.md`

## Global Constraints

- No backend/Rust changes.
- `resolveRequestFields`'s existing behavior for its current (tab-based)
  caller must be unchanged after the refactor in Task 1 — this is a
  pure extraction, not a rewrite.
- Backend `Auth` (`src/lib/tauri-api.ts:37-43`) has no `'inherit'`
  variant; a persisted request's `auth.authType === 'none'` means
  "inherit the collection's auth" once passed through
  `mapApiRequestToState(req, true)`. The runner MUST go through that
  conversion — it must never read `request.auth` directly off the
  backend `Request` and pass it straight through, or collection-level
  auth silently stops applying.

---

### Task 1: Extract `resolveRequestFieldsForPath`

**Files:**
- Modify: `src/lib/execute-request.ts:148-245` (the `resolveRequestFields` function)
- Test: `src/lib/__tests__/execute-request.test.ts` (create)

**Interfaces:**
- Consumes: `getActiveVariables`/`getGlobalVariables`/`getProcessEnvVars`
  (private to this file), `getCollectionSettings`/`getFolderChainVariables`/`getRequestVariables`/`toApiBody`/`toApiAuth`
  (already imported in this file), `buildVariableContext`/`resolveWithContext`
  (`@/lib/variable-context`), `useCollectionAuthStore`, `useEnvStore`.
- Produces: `resolveRequestFieldsForPath(collection: string | undefined, requestPath: string | undefined, request: RequestState): Promise<ResolvedRequestFields>`
  — consumed by Task 2 of this plan.

- [ ] **Step 1: Write the failing test**

```ts
// src/lib/__tests__/execute-request.test.ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RequestState } from '@/types/pane-types';

vi.mock('@/lib/tauri-api', () => ({
  getCollectionSettings: vi.fn(async () => ({
    variables: [{ key: 'baseUrl', value: 'https://collection.example', initialValue: '', enabled: true, secret: false }],
    headers: [{ key: 'X-Collection', value: 'yes', enabled: true }],
  })),
  getFolderChainVariables: vi.fn(async () => []),
  getRequestVariables: vi.fn(async () => []),
}));

vi.mock('@/stores/env-store', () => ({
  useEnvStore: { getState: () => ({ activeEnvId: null, activeCollection: null }) },
}));

vi.mock('@/stores/collection-auth-store', () => ({
  useCollectionAuthStore: { getState: () => ({ getCollectionAuth: () => undefined }) },
}));

vi.mock('@/lib/query-client', () => ({
  getQueryClient: () => ({ getQueryData: () => undefined }),
}));

import { resolveRequestFieldsForPath } from '@/lib/execute-request';

function baseRequest(): RequestState {
  return {
    requestType: 'http',
    method: 'GET',
    url: '{{baseUrl}}/ping',
    pathParams: [],
    queryParams: [],
    headers: [{ id: '1', key: 'Accept', value: 'application/json', enabled: true }],
    body: { mode: 'none', content: '', formData: [] },
    auth: { authType: 'none' },
    settings: { verifySsl: true, followRedirects: true, maxRedirects: 5, timeoutMs: 0, encodeUrl: true },
    docs: null,
    tags: [],
    assertions: [],
    actions: [],
  };
}

describe('resolveRequestFieldsForPath', () => {
  beforeEach(() => vi.clearAllMocks());

  it('resolves {{var}} placeholders using collection variables', async () => {
    const resolved = await resolveRequestFieldsForPath('demo', 'ping.yml', baseRequest());
    expect(resolved.url).toBe('https://collection.example/ping');
  });

  it('merges collection headers under request headers, request wins on collision', async () => {
    const resolved = await resolveRequestFieldsForPath('demo', 'ping.yml', baseRequest());
    const keys = resolved.headers.map((h) => h.key);
    expect(keys).toContain('X-Collection');
    expect(keys).toContain('Accept');
  });

  it('passes collection and requestPath through unchanged', async () => {
    const resolved = await resolveRequestFieldsForPath('demo', 'ping.yml', baseRequest());
    expect(resolved.collection).toBe('demo');
    expect(resolved.requestPath).toBe('ping.yml');
  });

  it('works with collection and requestPath both undefined', async () => {
    const resolved = await resolveRequestFieldsForPath(undefined, undefined, baseRequest());
    expect(resolved.url).toBe('{{baseUrl}}/ping'); // no collection vars available, left unresolved
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn vitest run src/lib/__tests__/execute-request.test.ts`
Expected: FAIL — `resolveRequestFieldsForPath` is not exported yet.

- [ ] **Step 3: Extract the function**

In `src/lib/execute-request.ts`, replace the existing
`resolveRequestFields` function (lines 145-245) with:

```ts
// Builds the fully-resolved request fields for a given collection +
// request path. Applies the same 7-scope variable resolution as
// sendRequest() so that every caller (single-request send, the load
// test dialog, the collection runner) gets consistent {{var}}
// substitution, regardless of whether the request came from an open
// tab or a tree walk over getCollection().
export async function resolveRequestFieldsForPath(
  collection: string | undefined,
  requestPath: string | undefined,
  request: RequestState,
): Promise<ResolvedRequestFields> {
  const envVars = getActiveVariables();
  const globalVars = getGlobalVariables();
  const processEnvVars = getProcessEnvVars();

  let collectionVars: CollectionVariable[] = [];
  let collectionHeaders: { key: string; value: string; enabled: boolean }[] = [];
  if (collection) {
    try {
      const settings = await getCollectionSettings(collection);
      collectionVars = settings.variables;
      collectionHeaders = settings.headers.filter((h) => h.enabled);
    } catch {
      // Collection settings unavailable — proceed without collection vars/headers.
    }
  }

  let folderVars: CollectionVariable[] = [];
  if (collection && requestPath) {
    try {
      folderVars = await getFolderChainVariables(collection, requestPath);
    } catch {
      // Non-critical: fall back to empty vars if chain lookup fails.
    }
  }

  let requestVars: CollectionVariable[] = [];
  if (collection && requestPath) {
    try {
      requestVars = await getRequestVariables(collection, requestPath);
    } catch {
      // Non-critical: fall back to empty vars if lookup fails.
    }
  }

  const ctx = buildVariableContext({
    processEnvVars,
    globalVars,
    envVars,
    collectionVars,
    folderVars,
    requestVars,
  });
  const resolve = (text: string) => resolveWithContext(text, ctx);

  let resolvedUrl = resolve(request.url);
  for (const p of request.pathParams) {
    if (p.enabled && p.key && p.value) {
      resolvedUrl = resolvedUrl.replace(`:${p.key}`, encodeURIComponent(p.value));
    }
  }

  const resolvedHeaders: Header[] = request.headers
    .filter((h) => h.enabled)
    .map((h) => ({ key: resolve(h.key), value: resolve(h.value), enabled: h.enabled }));

  const resolvedBody = toApiBody(request.body, resolve);

  let authToResolve: AuthState = request.auth;
  if (request.auth.authType === 'inherit' && collection) {
    const storedAuth = useCollectionAuthStore.getState().getCollectionAuth(collection);
    if (storedAuth && storedAuth.authType !== 'none' && storedAuth.authType !== 'inherit') {
      authToResolve = storedAuth;
    }
  }
  const resolvedAuth = toApiAuth(authToResolve, resolve);

  const requestHeaderKeys = new Set(resolvedHeaders.map((h) => h.key.toLowerCase()));
  const effectiveHeaders: Header[] = [
    ...collectionHeaders
      .filter((h) => !requestHeaderKeys.has(h.key.toLowerCase()))
      .map((h) => ({ key: resolve(h.key), value: resolve(h.value), enabled: true })),
    ...resolvedHeaders,
  ];

  const resolvedQueryParams = request.queryParams
    .filter((p) => p.enabled)
    .map((p) => ({ key: resolve(p.key), value: resolve(p.value), enabled: p.enabled }));

  return {
    url: resolvedUrl,
    headers: effectiveHeaders,
    queryParams: resolvedQueryParams,
    body: resolvedBody,
    auth: resolvedAuth,
    collection,
    environmentName: useEnvStore.getState().activeEnvId ?? undefined,
    requestPath,
  };
}

// Builds the fully-resolved request fields for a given tab and request state.
// Thin wrapper: looks up the tab's collection/path, then delegates to
// resolveRequestFieldsForPath.
export async function resolveRequestFields(
  tabId: string,
  request: RequestState,
): Promise<ResolvedRequestFields> {
  const { root } = usePaneStore.getState();
  const found = findTabInTree(root, tabId);
  return resolveRequestFieldsForPath(found?.tab.source?.collection, found?.tab.source?.path, request);
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `yarn vitest run src/lib/__tests__/execute-request.test.ts`
Expected: PASS (4 tests)

- [ ] **Step 5: Run the full frontend test suite and type check**

Run: `yarn vitest run && yarn tsc --noEmit`
Expected: PASS — every existing caller of `resolveRequestFields`
(the request-send flow, `LoadTestDialog`) is unaffected since its
signature and behavior are unchanged.

- [ ] **Step 6: Commit**

```bash
git add src/lib/execute-request.ts src/lib/__tests__/execute-request.test.ts
git commit -m "refactor: extract tab-independent resolveRequestFieldsForPath"
```

---

### Task 2: `executeRunnerEntry` helper

**Files:**
- Create: `src/lib/runner-execute.ts`
- Test: `src/lib/__tests__/runner-execute.test.ts`

**Interfaces:**
- Consumes: `mapApiRequestToState` (`@/lib/pane-utils`),
  `resolveRequestFieldsForPath` (Task 1), `executeRequest` and `Request`/`ExecuteRequestInput`/`ExecuteRequestResponse`
  (`@/lib/tauri-api`).
- Produces: `executeRunnerEntry(collection: string, requestPath: string, request: Request, environmentName: string | undefined): Promise<RunnerExecutionOutcome>`
  where `RunnerExecutionOutcome = { status: 'passed' | 'failed'; result?: ExecuteRequestResponse; error?: string }`
  — consumed by Plan C Task 2 (`startRun`).

- [ ] **Step 1: Write the failing test**

```ts
// src/lib/__tests__/runner-execute.test.ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Request } from '@/lib/tauri-api';

vi.mock('@/lib/tauri-api', () => ({
  executeRequest: vi.fn(),
}));

vi.mock('@/lib/execute-request', () => ({
  resolveRequestFieldsForPath: vi.fn(async () => ({
    url: 'https://example.com/ping',
    headers: [],
    queryParams: [],
    body: undefined,
    auth: { authType: 'none' },
    collection: 'demo',
    environmentName: undefined,
    requestPath: 'ping.yml',
  })),
}));

import { executeRequest } from '@/lib/tauri-api';
import { executeRunnerEntry } from '@/lib/runner-execute';

function baseRequest(): Request {
  return {
    uid: 'r1',
    name: 'Ping',
    method: 'GET',
    url: 'https://example.com/ping',
    headers: [],
    auth: { authType: 'none' },
    fileName: 'ping.yml',
  };
}

describe('executeRunnerEntry', () => {
  beforeEach(() => vi.clearAllMocks());

  it('reports passed for a 2xx response with no failing tests', async () => {
    vi.mocked(executeRequest).mockResolvedValue({
      status: 200,
      statusText: 'OK',
      headers: [],
      body: '',
      durationMs: 10,
      ttfbMs: 5,
      sizeBytes: 0,
      testResults: [{ name: 'ok', status: 'passed', error: null }],
      consoleEntries: [],
      scriptError: null,
    });
    const outcome = await executeRunnerEntry('demo', 'ping.yml', baseRequest(), undefined);
    expect(outcome.status).toBe('passed');
    expect(outcome.result?.status).toBe(200);
  });

  it('reports failed for a non-2xx response', async () => {
    vi.mocked(executeRequest).mockResolvedValue({
      status: 500,
      statusText: 'Error',
      headers: [],
      body: '',
      durationMs: 10,
      ttfbMs: 5,
      sizeBytes: 0,
      testResults: [],
      consoleEntries: [],
      scriptError: null,
    });
    const outcome = await executeRunnerEntry('demo', 'ping.yml', baseRequest(), undefined);
    expect(outcome.status).toBe('failed');
  });

  it('reports failed when a test assertion fails', async () => {
    vi.mocked(executeRequest).mockResolvedValue({
      status: 200,
      statusText: 'OK',
      headers: [],
      body: '',
      durationMs: 10,
      ttfbMs: 5,
      sizeBytes: 0,
      testResults: [{ name: 'bad', status: 'failed', error: 'nope' }],
      consoleEntries: [],
      scriptError: null,
    });
    const outcome = await executeRunnerEntry('demo', 'ping.yml', baseRequest(), undefined);
    expect(outcome.status).toBe('failed');
  });

  it('reports failed when a script error is present', async () => {
    vi.mocked(executeRequest).mockResolvedValue({
      status: 200,
      statusText: 'OK',
      headers: [],
      body: '',
      durationMs: 10,
      ttfbMs: 5,
      sizeBytes: 0,
      testResults: [],
      consoleEntries: [],
      scriptError: 'ReferenceError: x is not defined',
    });
    const outcome = await executeRunnerEntry('demo', 'ping.yml', baseRequest(), undefined);
    expect(outcome.status).toBe('failed');
  });

  it('catches a thrown error from executeRequest and reports it', async () => {
    vi.mocked(executeRequest).mockRejectedValue(new Error('network down'));
    const outcome = await executeRunnerEntry('demo', 'ping.yml', baseRequest(), undefined);
    expect(outcome.status).toBe('failed');
    expect(outcome.error).toBe('network down');
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn vitest run src/lib/__tests__/runner-execute.test.ts`
Expected: FAIL — `src/lib/runner-execute.ts` does not exist.

- [ ] **Step 3: Implement the helper**

```ts
// src/lib/runner-execute.ts
import { resolveRequestFieldsForPath } from '@/lib/execute-request';
import { mapApiRequestToState } from '@/lib/pane-utils';
import {
  type ExecuteRequestInput,
  type ExecuteRequestResponse,
  type Request,
  executeRequest,
} from '@/lib/tauri-api';

export interface RunnerExecutionOutcome {
  status: 'passed' | 'failed';
  result?: ExecuteRequestResponse;
  error?: string;
}

// Executes one request from a collection tree walk (not an open tab),
// applying the same variable resolution and inherit-auth semantics a
// sidebar-opened tab gets, via mapApiRequestToState(request, true).
export async function executeRunnerEntry(
  collection: string,
  requestPath: string,
  request: Request,
  environmentName: string | undefined,
): Promise<RunnerExecutionOutcome> {
  try {
    const requestState = mapApiRequestToState(request, true);
    const resolved = await resolveRequestFieldsForPath(collection, requestPath, requestState);

    const input: ExecuteRequestInput = {
      method: request.method,
      url: resolved.url,
      headers: resolved.headers,
      queryParams: resolved.queryParams,
      body: resolved.body,
      auth: resolved.auth,
      options: {
        followRedirects: requestState.settings.followRedirects,
        timeoutMs: requestState.settings.timeoutMs,
        verifySsl: requestState.settings.verifySsl,
      },
      environmentName,
      collection,
      requestName: request.name,
      requestPath,
      preRequestScript: request.preRequestScript ?? undefined,
      postResponseScript: request.postResponseScript ?? undefined,
      testsScript: request.tests ?? undefined,
      assertions: request.assertions,
      tags: request.tags,
      actions: request.actions,
    };

    const result = await executeRequest(input);
    const hasFailingTest = result.testResults.some((t) => t.status === 'failed');
    const isErrorStatus = result.status < 200 || result.status >= 300;
    const failed = isErrorStatus || hasFailingTest || Boolean(result.scriptError);
    return { status: failed ? 'failed' : 'passed', result };
  } catch (err) {
    return { status: 'failed', error: err instanceof Error ? err.message : String(err) };
  }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `yarn vitest run src/lib/__tests__/runner-execute.test.ts`
Expected: PASS (5 tests)

- [ ] **Step 5: Run the full frontend test suite and type check**

Run: `yarn vitest run && yarn tsc --noEmit`
Expected: PASS, no new errors.

- [ ] **Step 6: Commit**

```bash
git add src/lib/runner-execute.ts src/lib/__tests__/runner-execute.test.ts
git commit -m "feat: add executeRunnerEntry for tree-walked request execution"
```
