# Exclusive Create for Non-HTTP Requests, Frontend Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** The Create dialog and the WebSocket creator call the new exclusive create commands, so an existing GraphQL, gRPC or WebSocket name shows an "already exists" toast and writes nothing.

**Architecture:** Three `invoke` wrappers in `tauri-api.ts` for the commands from plan 1. The dialog branches and `createWebSocketItem` switch from the overwriting `save*` wrappers to them. The existing `createRequestErrorMessage` toast maps the error.

**Tech Stack:** TypeScript, React, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-29-exclusive-create-non-http-requests-design.md`

## File Structure

- `src/lib/tauri-api.ts`: `createGraphQlRequest`, `createGrpcRequest`, `createWebSocketRequest`.
- `src/components/request/CreateRequestDialog.tsx`: GraphQL and gRPC branches.
- `src/lib/websocket-create.ts`: `createWebSocketItem`.
- Tests: `src/components/request/__tests__/CreateRequestDialog.test.tsx`, `CreateRequestDialog.grpc.test.tsx`, `CreateRequestDialog.websocket.test.tsx`, `src/lib/__tests__/websocket-create.test.ts`.

## Global Constraints

- Do not change `saveGraphQlRequest`, `saveGrpcRequest`, `saveWebSocketRequest`, `saveTabRequest` or `save-tab-request.ts`. Tab save, auto-save and "Save to collection" keep overwriting.
- The wrappers use the same payload shape as the matching `save*` wrapper and the command names `create_graphql_request`, `create_grpc_request`, `create_websocket_request`.
- Edit only the files listed. No formatting, import-order or clippy edits elsewhere. `yarn check` is read-only (`biome check src/ scripts/`): run it and fix what it reports by hand in the listed files, or run `bunx biome check --write <the listed files>`. Never run `yarn lint` or `yarn format`.
- Test names are plain sentences like the existing `it('...')` cases. Append to the existing test files.
- shadcn/ui and lucide-react only. Checks: `yarn tsc --noEmit`, `yarn check`, `yarn test --run <patterns>`.

## Review Focus

1. A rejection with `Already exists: col/foo.yml` shows the toast `A request named "foo" already exists here. Pick another name.` and opens no tab: `shows the already exists toast and opens no tab` (Tasks 1 and 2).
2. The dialog no longer calls the overwriting `save*` wrapper for these three types: `creates through the exclusive wrapper, not the save wrapper` (Tasks 1 and 2).
3. `save-tab-request.ts` still calls the `save*` wrappers: no edit to that file and its existing tests keep passing (Task 1 Step 5).

---

### Task 1: Wrappers and the GraphQL and gRPC dialog branches

**Files:**
- Modify: `src/lib/tauri-api.ts:935-954`
- Modify: `src/components/request/CreateRequestDialog.tsx:91-139`
- Test: `src/components/request/__tests__/CreateRequestDialog.test.tsx`, `src/components/request/__tests__/CreateRequestDialog.grpc.test.tsx`

**Interfaces:**
- Produces in `tauri-api.ts`: `createGraphQlRequest(collection: string, path: string, request: GraphQlRequest): Promise<GraphQlRequest>` (`invoke('create_graphql_request', { collection, path, request })`), `createGrpcRequest(...): Promise<GrpcRequest>` (`'create_grpc_request'`) and `createWebSocketRequest(...): Promise<WebSocketRequest>` (`'create_websocket_request'`), built like `saveGraphQlRequest`, `saveGrpcRequest` and `saveWebSocketRequest` next to them.
- The dialog's GraphQL branch (`:92`) calls `createGraphQlRequest` and its gRPC branch (`:117`) calls `createGrpcRequest`, with the arguments the save calls had. The catch block at `:174-177` stays.

- [ ] **Step 1: Read** `CreateRequestDialog.tsx:60-181`, `src/lib/create-request-error.ts`, `CreateRequestDialog.test.tsx` (describe "CreateRequestDialog graphql") and `CreateRequestDialog.grpc.test.tsx`. Their `vi.mock('@/lib/tauri-api', ...)` uses `importActual` plus `vi.fn()` overrides, and `beforeAll` stubs the Radix Select pointer APIs.

- [ ] **Step 2: Write the failing tests.** In each of the two files:
  - Add `createGraphQlRequest` (or `createGrpcRequest`) to the `vi.mock` override list.
  - Change the existing test (`saves a real GraphQL item, not an HTTP request tagged graphql`, and the gRPC equivalent) to expect the new wrapper: rename it `creates through the exclusive wrapper, not the save wrapper`, assert `createGraphQlRequest` was called once and `saveGraphQlRequest` and `saveRequest` were not.
  - Add, following the same click and type flow:

```tsx
it('shows the already exists toast and opens no tab', async () => {
  vi.mocked(tauriApi.createGraphQlRequest).mockRejectedValue(
    new Error('Already exists: col/foo.yml'),
  );
  // select the GraphQL type, type the name "foo" and a URL, click Create as the other tests do
  await waitFor(() =>
    expect(toast.error).toHaveBeenCalledWith(
      'A request named "foo" already exists here. Pick another name.',
    ),
  );
  expect(usePaneStore.getState().root).toEqual(initialRoot);
});
```

  Capture `initialRoot` after the `beforeEach` reset, and mock `sonner` as the file's neighbours do. The gRPC file gets the same test with `createGrpcRequest`.

- [ ] **Step 3: Run** `yarn test --run CreateRequestDialog`. Expected: the renamed and new tests fail (the dialog still calls `saveGraphQlRequest`, so `createGraphQlRequest` is not called and there is no toast). Vitest does not type-check, so a missing export shows up as `undefined`, not as an import error.

- [ ] **Step 4: Implement** the three wrappers and switch the two dialog branches.

- [ ] **Step 5: Run** `yarn test --run CreateRequestDialog save-tab-request`, `yarn tsc --noEmit` and `yarn check`. Expected: all pass, and the `save-tab-request` tests are unchanged.

---

### Task 2: WebSocket creation

**Files:**
- Modify: `src/lib/websocket-create.ts:11-26`
- Test: `src/lib/__tests__/websocket-create.test.ts`, `src/components/request/__tests__/CreateRequestDialog.websocket.test.tsx`

**Interfaces:**
- Consumes: `createWebSocketRequest` from Task 1.
- Produces: `createWebSocketItem` calls `createWebSocketRequest(collectionName, filePath, payload)` and keeps its return value (`webSocketToTab(saved, collectionName, saved.fileName ?? filePath)`). It is only used by the dialog.

- [ ] **Step 1: Read** `websocket-create.ts`, `websocket-create.test.ts` (the two existing tests at `:24` and `:37`) and `CreateRequestDialog.websocket.test.tsx`.

- [ ] **Step 2: Write the failing tests.**
  - `websocket-create.test.ts`: add `createWebSocketRequest` to the mock list; change the two existing tests to expect `createWebSocketRequest` (and assert `saveWebSocketRequest` and `saveRequest` are not called). Add `uses the exclusive create and passes the error through`: the mock rejects with `new Error('Already exists: col/ws.yml')` and `await expect(createWebSocketItem(...)).rejects.toThrow('Already exists')`.
  - `CreateRequestDialog.websocket.test.tsx`: add `createWebSocketRequest` to the mock list, rename the existing test to `creates through the exclusive wrapper, not the save wrapper` with the new expectations, and add `shows the already exists toast and opens no tab` with the same shape as Task 1.

- [ ] **Step 3: Run** `yarn test --run websocket-create CreateRequestDialog.websocket`. Expected: the renamed and new tests fail (`createWebSocketRequest` is not called).

- [ ] **Step 4: Implement** the one-line switch in `createWebSocketItem`.

- [ ] **Step 5: Run** `yarn test --run websocket-create CreateRequestDialog`, `yarn tsc --noEmit` and `yarn check`. Expected: all pass.

Next plan: docs/superpowers/plans/2026-10-29-exclusive-create-non-http-requests/plan-03-exclusive-create-non-http-requests.md
