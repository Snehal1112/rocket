# History Resend Trust Gate Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** A request resent from a History tab keeps the host environment trust gate for `{{process.env.NAME}}`, and a History entry with no collection gets no host environment.

**Architecture:** A History tab carries its entry's collection in a new `historyCollection` field, never in `source`. One helper turns a tab into a process env scope (a collection name, `CLOSED_PROCESS_ENV` or `undefined`). The scope flows into `getProcessEnvVars`, the withheld warning and the variable preview. The backend is unchanged.

**Tech Stack:** TypeScript, React, Vitest, TanStack Query, Zustand.

**Spec:** `docs/superpowers/specs/2026-10-29-history-resend-trust-gate-design.md`

## File Structure

- `src/types/pane-types.ts`: add `historyCollection?: string | null` to `RequestTab`.
- `src/lib/execute-request.ts`: re-export of `CLOSED_PROCESS_ENV` and `ProcessEnvScope`, `processEnvScopeOf`, the scope parameter and its callers.
- `src/lib/process-env-gate.ts`: defines `CLOSED_PROCESS_ENV` and `ProcessEnvScope`; the first parameter of the warning takes a `ProcessEnvScope`.
- `src/components/history/HistoryPanel.tsx`: `openEntry` sets `historyCollection`.
- `src/lib/queries/environment-queries.ts`: `useProcessEnvVars(collection, enabled = true)`.
- `src/components/request/RequestPanel.tsx`: uses the scope for the preview.
- Tests: `src/lib/__tests__/process-env-scope.test.ts`, `src/lib/__tests__/process-env-gate.test.ts`, `src/components/history/__tests__/HistoryPanel.test.tsx`, a new `src/lib/queries/__tests__/environment-queries.process.test.tsx`.

## Global Constraints

- Do not touch `src-tauri`, `crates` or `docs/features/collection-trust.md`. They match the factory risk rules and stop the job for a human.
- Do not set `source` on a History tab. `source` turns on autosave, the save UI, collection switching and collection variables.
- Edit only the files listed above. No formatting, import-order or clippy edits in any other file. `yarn check` is read-only (`biome check src/ scripts/`). Run it, fix what it reports by hand in the listed files only, or run `bunx biome check --write <the listed files>`. Never run `yarn lint` or `yarn format`: they rewrite all of `src/`.
- Do not edit `src/lib/__tests__/execute-request.oauth.test.ts` (its name matches the `**/*auth*` risk rule and would stop the job). Read it only, for the tab-literal pattern and the executor stub.
- Test names are plain sentences, like the existing `it('...')` cases. Append to the existing test files.
- `getProcessEnvVars` in `execute-request.ts` is module-private. Tests reach it through `buildOAuth2VarContext(collection, path, scope)` and `resolveRequestFields` with a pane-store tab.
- UI uses shadcn/ui primitives and lucide-react icons only. Zustand selectors stay narrow.
- Checks: `yarn tsc --noEmit`, `yarn check`, and `yarn test --run <patterns>` (plain `yarn test` is watch mode).
- New test files are allowed only where no existing file fits (the one new file is named in Task 3).
- Leave the `collection={tab.source?.collection}` props in `RequestPanel.tsx` (about `:1233-1330`) alone. Switching them to `historyCollection` would load collection variables, which the spec forbids.

## Review Focus

1. A History tab saved into a collection has a `source`, so `source.collection` wins over `historyCollection`: `lets source win over historyCollection` (Task 2).
2. A closed History tab sends no process env call at all, not even for a request with no placeholder: `makes no process env call for a history tab without a collection` (Task 2).
3. A History tab never loads collection variables or headers, because only the process env scope changes: `does not load collection variables or headers for a history tab` (Task 2).
4. A closed request with no `{{process.env.` text shows no warning: `stays silent for a closed scope without the placeholder` (Task 1).
5. The variable preview of a closed History tab shows no host value even when a scratch tab already filled the `null` scope cache with the host environment: `returns no variables when disabled even with a warm cache` (Task 3).
6. A History tab that is closed or parked mid-send does not fall back to full access: `treats a missing history tab as closed` (Task 2).

---

### Task 1: Scope type, helper and closed-scope warning

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src/types/pane-types.ts:30-34`
- Modify: `src/lib/execute-request.ts` (near `NO_PROCESS_ENV` at `:97`, `getProcessEnvVars` at `:103-119`; re-exports and `processEnvScopeOf`)
- Modify: `src/lib/process-env-gate.ts:19-53` (the early return at `:24` and the trust and toast block both change)
- Test: `src/lib/__tests__/process-env-gate.test.ts`

**Interfaces:**
- Produces:
  - `RequestTab.historyCollection?: string | null` (set only for `tabType: 'history'`).
  - In `process-env-gate.ts` (not `execute-request.ts`, which imports the gate, so a symbol defined there would be a circular import): `export const CLOSED_PROCESS_ENV: unique symbol` and `export type ProcessEnvScope = string | typeof CLOSED_PROCESS_ENV | undefined`. `execute-request.ts` re-exports them as `export { CLOSED_PROCESS_ENV } from '@/lib/process-env-gate'` and `export type { ProcessEnvScope } from '@/lib/process-env-gate'` (a plain `export { }` of the type fails under `isolatedModules`). The symbol is only read inside function bodies, so the existing import cycle through `pane-store` and `runner-execute` is harmless.
  - In `execute-request.ts`: `export function processEnvScopeOf(tab: Tab | undefined, tabId?: string): ProcessEnvScope`. When `tab` is `undefined` and `tabId` starts with `history-`, it returns `CLOSED_PROCESS_ENV` (a History tab that is closed mid-send or parked by a collection switch must not fall back to full access).
  - `getProcessEnvVars(scope: ProcessEnvScope): Promise<ProcessEnv>` returns `NO_PROCESS_ENV` at once for `CLOSED_PROCESS_ENV`, with no backend call.
  - `warnIfProcessEnvWithheld(scope: ProcessEnvScope | null, parts: unknown[], requestName?: string): Promise<void>`. A string scope behaves as today. `CLOSED_PROCESS_ENV` with the placeholder in `parts` shows the console entry and toast with the message below and no Review action. `undefined` or `null` returns at once.
  - `processEnvScopeOf(tab)`: `tab.source?.collection` if the tab has a `source`; else `historyCollection ?? CLOSED_PROCESS_ENV` for a `tabType === 'history'` tab; else `undefined`.

Closed-scope message (exact): `Host environment variables are not available for a request reopened from History without a collection, so {{process.env.NAME}} was sent unresolved.`

- [ ] **Step 1: Read** `execute-request.ts:95-125`, `process-env-gate.ts`, `pane-types.ts:20-40` and the existing tests in `process-env-gate.test.ts`.

- [ ] **Step 2: Write the failing tests** in `process-env-gate.test.ts` (import `CLOSED_PROCESS_ENV` from `@/lib/process-env-gate`), using the file's `beforeEach` and `makeTrust`:

```ts
const CLOSED_MESSAGE =
  'Host environment variables are not available for a request reopened from History without a collection, so {{process.env.NAME}} was sent unresolved.';

it('warns for a closed scope when the request uses process.env', async () => {
  await warnIfProcessEnvWithheld(CLOSED_PROCESS_ENV, [{ url: 'https://e.example/{{process.env.HOME}}' }], 'Get');
  expect(toast.warning).toHaveBeenCalledTimes(1);
  const entries = useConsoleStore.getState().entries;
  expect(entries).toHaveLength(1);
  expect(entries[0]).toMatchObject({ kind: 'script', level: 'warn', requestName: 'Get' });
  expect(entries[0].message).toBe(CLOSED_MESSAGE);
  expect(toast.warning).toHaveBeenCalledWith(CLOSED_MESSAGE, expect.anything());
  const options = vi.mocked(toast.warning).mock.calls[0]?.[1] as { action?: unknown } | undefined;
  expect(options?.action).toBeUndefined();
  expect(tauriApi.getCollectionTrust).not.toHaveBeenCalled();
});

it('stays silent for a closed scope without the placeholder', async () => {
  await warnIfProcessEnvWithheld(CLOSED_PROCESS_ENV, [{ url: 'https://e.example/' }], 'Get');
  expect(toast.warning).not.toHaveBeenCalled();
  expect(useConsoleStore.getState().entries).toHaveLength(0);
});
```

- [ ] **Step 3: Run** `yarn test --run process-env-gate`. Expected: `warns for a closed scope when the request uses process.env` fails with `toast.warning` called 0 times (vitest does not type-check, so the missing export is `undefined` at run time and the function returns early). `stays silent for a closed scope without the placeholder` already passes and is a guard.

- [ ] **Step 4: Implement** the field, the symbol, the type, `processEnvScopeOf`, the early return in `getProcessEnvVars`, and the closed branch in `warnIfProcessEnvWithheld`. The toast for the closed branch has no `action`.

- [ ] **Step 5: Run** `yarn test --run process-env-gate`, then `yarn tsc --noEmit` and `yarn check`. Expected: the two new tests and the existing ones pass, no type errors.

---

### Task 2: Thread the scope through field resolution and send

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src/lib/execute-request.ts` (`resolveRequestFieldsForPath` `:311-319`, `resolveRequestFields` `:477-490`, `buildOAuth2VarContext` `:653-686`, `sendRequest` `:696-704`)
- Test: `src/lib/__tests__/process-env-scope.test.ts`

**Interfaces:**
- Consumes: `ProcessEnvScope`, `processEnvScopeOf`, `getProcessEnvVars(scope)` from Task 1.
- Produces:
  - `resolveRequestFieldsForPath(collection, path, request, forSend = false, processEnvScope?: ProcessEnvScope)`. When `processEnvScope === undefined` it uses `collection`, so `runner-execute.ts` needs no change. The scope is used only for the process env.
  - `buildOAuth2VarContext(collection, requestPath?, processEnvScope?: ProcessEnvScope)` with the same default. `flow-auth-preflight.ts` needs no change.
  - `resolveRequestFields(tabId, request, forSend)` and `sendRequest` compute `processEnvScopeOf(found?.tab, tabId)` and pass it, including to `warnIfProcessEnvWithheld`.

- [ ] **Step 1: Read** the functions above, the existing `process-env-scope.test.ts` and `execute-request.oauth.test.ts` (read only). The tab-based tests need `usePaneStore.getState().reset()` then `openTab(<tab literal>)`, with the request from `createDefaultRequest()` (`@/lib/pane-utils`). Copy only the tab-literal pattern and the `executeRequest` stub from the oauth test, not its `tauri-api` mock: `process-env-scope.test.ts` already mocks `getProcessEnvVars` and `getCollectionSettings`.
  - Add to its `beforeEach`: `usePaneStore.getState().reset(); useEnvStore.setState({ activeEnvId: null, activeCollection: null });`. Give every test its own tab id (`history-h1`, `history-h2`, and so on), because `openTab` only re-activates an open tab with the same id.
  - Extend the file's `tauri-api` mock with `executeRequest: vi.fn()`, `getCollectionTrust: vi.fn()` and `getFolderChainVariables: vi.fn()`, and add `vi.mock('sonner', () => ({ toast: { warning: vi.fn() } }))`. `executeRequest` returning `undefined` makes `sendRequest` take its catch branch, which is enough.

- [ ] **Step 2: Write the failing tests** in `process-env-scope.test.ts`. Example (the first one):

```ts
it('asks for the process env of the history collection', async () => {
  vi.mocked(tauriApi.getProcessEnvVars).mockResolvedValue(pe({ HOME: '/home/u' }));
  const ctx = await buildOAuth2VarContext(undefined, undefined, 'clone');
  expect(tauriApi.getProcessEnvVars).toHaveBeenCalledWith('clone', null, null);
  expect(ctx['process.env.HOME']).toBe('/home/u');
});
```

That is test 1 of 8. The other seven (guards are marked; a guard already passes before the change):

| Test | Setup | Assertion |
|---|---|---|
| `gets no host environment and makes no call for a history tab without a collection` | `getProcessEnvVars` mocked to resolve `pe({ HOME: '/home/u' })`; history tab, `historyCollection: null`, URL `https://e.example/{{process.env.HOME}}`; call `resolveRequestFields(tabId, request)` | `getProcessEnvVars` not called, `resolved.url` is still `https://e.example/{{process.env.HOME}}` |
| `keeps full access for a scratch tab` (guard) | `useEnvStore.setState({ activeCollection: 'other' })`; `tabType: 'request'` tab with no `source`; same call | `getProcessEnvVars` called with `(null, null, null)` |
| `does not load collection variables or headers for a history tab` | history tab with `historyCollection: 'clone'` | process env asked for `'clone'`; `getCollectionSettings` and `getFolderChainVariables` not called; `resolved.collection` is `undefined` |
| `lets source win over historyCollection` (guard) | history tab with `source: { collection: 'a', path: 'x.yml' }` and `historyCollection: 'b'` | process env asked for `'a'`, and never called with `'b'` as first argument |
| `treats a missing history tab as closed` | call `resolveRequestFields('history-gone', request)` with no such tab open | `getProcessEnvVars` not called |
| `sends a history tab without a collection with no process env call and the closed warning` | history tab, `historyCollection: null`, URL with `{{process.env.HOME}}`; `await sendRequest(tabId, request)` | `getProcessEnvVars` not called, `executeRequest.mock.calls[0][0].url` contains `{{process.env.HOME}}`, its `collection` is `undefined`, `toast.warning` called with the closed message |
| `warns with the history collection on a resend` | history tab, `historyCollection: 'clone'`, `getCollectionTrust` resolving `makeTrust()` (from `@/test/trust-fixtures`); `await sendRequest(tabId, request)` | `getCollectionTrust` called with `'clone'`, and every `getProcessEnvVars` call has `'clone'` as its first argument |

The first (example) test also asserts `expect(tauriApi.getCollectionSettings).not.toHaveBeenCalled()`.

- [ ] **Step 3: Run** `yarn test --run process-env-scope`. Expected: these fail: the example test, `gets no host environment…`, `does not load collection variables…`, `treats a missing history tab as closed`, and the two `sendRequest` tests. These already pass and are regression guards: `keeps full access for a scratch tab` and `lets source win over historyCollection`.

- [ ] **Step 4: Implement** the optional parameters with the default, and pass `processEnvScopeOf(found?.tab, tabId)` from `resolveRequestFields` and from `sendRequest`, to all three of its uses: the warning, the pre-send `buildOAuth2VarContext` call and the field resolution. Leaving the pre-send `buildOAuth2VarContext` call without the scope would still fetch the full host environment.

- [ ] **Step 5: Run** `yarn test --run process-env-scope process-env-gate`, `yarn test --run execute-request`, `yarn tsc --noEmit` and `yarn check`. Expected: all pass, and the existing scratch and collection cases are unchanged.

---

### Task 3: History tab field, preview hook and panel

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src/components/history/HistoryPanel.tsx:149-163`
- Modify: `src/lib/queries/environment-queries.ts:74-82`
- Modify: `src/components/request/RequestPanel.tsx:572`
- Test: `src/components/history/__tests__/HistoryPanel.test.tsx`, new `src/lib/queries/__tests__/environment-queries.process.test.tsx`

**Interfaces:**
- Consumes: `processEnvScopeOf`, `CLOSED_PROCESS_ENV` from Task 1.
- Produces:
  - `openEntry` sets `historyCollection: entry.collection ?? null` on the History tab and sets no `source`.
  - `useProcessEnvVars(collection: string | null | undefined = null, enabled = true)`: passes `enabled` to `useQuery`, and uses `select: enabled ? (env) => env.vars : () => NO_VARS` with a module-level `const NO_VARS: Record<string, string> = {}`. A disabled query still returns cached data for its key, and a scratch tab fills the `null` key with the host environment, so the hook must return the empty map itself.
  - `RequestPanel` computes `const scope = processEnvScopeOf(tab)` and calls `useProcessEnvVars(typeof scope === 'string' ? scope : null, scope !== CLOSED_PROCESS_ENV)`, then `const processEnvVars = scope === CLOSED_PROCESS_ENV ? NO_PROCESS_ENV_VARS : (data ?? NO_PROCESS_ENV_VARS)` with a module-level `const NO_PROCESS_ENV_VARS: Record<string, string> = {}` (a stable reference, so `scopedContext` does not recompute every render). Import `processEnvScopeOf` and `CLOSED_PROCESS_ENV` from `@/lib/execute-request`, which already imports at `:66`.

- [ ] **Step 1: Read** `HistoryPanel.tsx:140-170`, the existing `HistoryPanel.test.tsx` (the `entry()` fixture at `:27-39`) and `environment-queries.global.test.tsx`. Its `wrapper` and its `vi.mock('@/lib/tauri-api')` are local and have no `getProcessEnvVars`, so the new test file defines its own `QueryClientProvider` wrapper and its own mock: `vi.mock('@/lib/tauri-api', async () => ({ ...(await vi.importActual<typeof tauriApi>('@/lib/tauri-api')), getProcessEnvVars: vi.fn() }))`.

- [ ] **Step 2: Write the failing tests.**

HistoryPanel: no existing test clicks a row or reads the pane store. Give `entry(id, runSource?)` a third optional parameter `collection?: string` that is put on the returned object. Call `usePaneStore.getState().reset()` in `beforeEach`. Use distinct ids per test (`h1`, `h2`), mock `listHistory` to resolve `[entry('h1', undefined, 'clone')]`, click with `await userEvent.click(await screen.findByRole('button', { name: 'Open GET https://api.example.com/h1' }))`, then read the tab with `findTabInTree(usePaneStore.getState().root, 'history-h1')?.tab` (narrow it to a `RequestTab`):
- `sets historyCollection and no source when the entry has a collection`: expect `historyCollection` to be `'clone'` and `source` undefined.
- `sets historyCollection to null when the entry has none` (id `h2`, no collection): expect `historyCollection` `null`.

environment-queries.process.test.tsx: mock `@/lib/tauri-api` as described in Step 1, create a fresh `QueryClient` per test (`retry: false`), and a wrapper `({ children }) => <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>`. Import `environmentKeys` from `@/lib/queries/environment-queries`:

```ts
it('does not call the backend when disabled', async () => {
  const { result } = renderHook(() => useProcessEnvVars('clone', false), { wrapper });
  expect(result.current.fetchStatus).toBe('idle');
  expect(tauriApi.getProcessEnvVars).not.toHaveBeenCalled();
});

it('returns no variables when disabled even with a warm cache', async () => {
  queryClient.setQueryData(environmentKeys.process(null), { vars: { HOME: '/home/u' }, dotEnvKeys: [] });
  const { result } = renderHook(() => useProcessEnvVars(null, false), { wrapper });
  expect(result.current.data).toEqual({});
});
```

- [ ] **Step 3: Run** `yarn test --run HistoryPanel environment-queries`. Expected: the two HistoryPanel tests fail (`historyCollection` is `undefined`), `does not call the backend when disabled` fails (`fetchStatus` is `'fetching'`, the second argument is ignored), and `returns no variables when disabled even with a warm cache` fails (it returns the cached host map).

- [ ] **Step 4: Implement** the three changes above. `RequestPanel` has no test of its own (its tests time out at full CPU); the hook-level warm-cache test guards the data path, and the builder reports in its notes that the panel change was checked by reading the diff.

- [ ] **Step 5: Run** `yarn test --run HistoryPanel environment-queries process-env-scope process-env-gate`, `yarn tsc --noEmit` and `yarn check`. Expected: all pass.

Next plan: none
