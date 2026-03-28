# Collection Variable Self-Echo Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Suppress duplicate collection-variable reads caused by the
frontend consuming its own websocket echo after `pm.variables.set(...)`
persists `collection.bru`.

**Architecture:** Keep collection-variable saves locally authoritative,
record a short-lived suppression marker for successful local saves, and
teach websocket `collection.bru` handling to consume one matching marker
instead of refetching variables. Preserve existing refresh behavior for
external edits and non-variable file changes.

**Tech Stack:** React 19, TypeScript, Zustand, Vitest, Vite

---

### Task 1: Add failing tests for collection-variable self-echo suppression

**Files:**
- Modify: `frontend/src/store/__tests__/collections-store.fetch-dedupe.test.ts`
- Create or Modify: `frontend/src/App.test.tsx`
- Check: `frontend/src/App.tsx`
- Check: `frontend/src/store/collections.ts`

**Step 1: Write the failing tests**

Add test coverage for two levels:

- store-level behavior proving a successful
  `saveCollectionVariables('snehal', vars)` can register a local
  suppression marker
- app-level websocket behavior proving a `file_change` for
  `collection.bru` in collection `snehal` does not call
  `fetchCollectionVariables('snehal')` when a fresh suppression marker
  exists

Also add negative cases:

- another collection still refetches
- another file path still follows normal refresh behavior
- expired suppression markers do not block refetch

**Step 2: Run the focused tests to confirm failure**

Run:

```bash
cd frontend && yarn test src/store/__tests__/collections-store.fetch-dedupe.test.ts src/App.test.tsx
```

Expected: FAIL because there is no self-echo suppression mechanism yet.

**Step 3: Commit test-only updates if needed**

```bash
git add frontend/src/store/__tests__/collections-store.fetch-dedupe.test.ts frontend/src/App.test.tsx
git commit -m "test(frontend): cover variable self-echo suppression"
```

### Task 2: Implement suppression marker state in the collections store

**Files:**
- Modify: `frontend/src/store/collections.ts`
- Test: `frontend/src/store/__tests__/collections-store.fetch-dedupe.test.ts`

**Step 1: Add the minimal state and helpers**

Add store support for a short-lived suppression marker keyed by
collection name.

The implementation should include:

- a place to record the timestamp or token of the last successful local
  collection-variable save
- a helper to decide whether a websocket `collection.bru` echo for a
  given collection should be suppressed
- one-time consumption semantics so only the matching echo is skipped

Keep the shape minimal and internal to the store unless broader
exposure is required for tests or `App.tsx`.

**Step 2: Update `saveCollectionVariables()`**

After a successful `apiService.saveCollectionVariables(name, vars)`,
keep the existing local `collectionVariables` update and register the
suppression marker for that collection.

Do not register the marker on failed saves.

**Step 3: Run the focused store test**

Run:

```bash
cd frontend && yarn test src/store/__tests__/collections-store.fetch-dedupe.test.ts
```

Expected: PASS.

**Step 4: Commit**

```bash
git add frontend/src/store/collections.ts frontend/src/store/__tests__/collections-store.fetch-dedupe.test.ts
git commit -m "fix(store): track collection variable self-echo"
```

### Task 3: Consume the suppression marker in websocket handling

**Files:**
- Modify: `frontend/src/App.tsx`
- Test: `frontend/src/App.test.tsx`

**Step 1: Implement the minimal websocket change**

In the `collection.bru` branch of websocket `file_change` handling:

- check whether the store says this event is a recent local save echo
- if yes, consume the marker and skip `fetchCollectionVariables()`
- if no, continue to fetch collection variables normally

Leave handling for:

- `environments/*`
- collection tree changes
- collection list refreshes

unchanged except where necessary to wire in the suppression check.

**Step 2: Run the focused app test**

Run:

```bash
cd frontend && yarn test src/App.test.tsx
```

Expected: PASS.

**Step 3: Commit**

```bash
git add frontend/src/App.tsx frontend/src/App.test.tsx
git commit -m "fix(sync): suppress collection variable self-echo"
```

### Task 4: Verify the reported send scenario

**Files:**
- Check: `frontend/src/App.tsx`
- Check: `frontend/src/store/collections.ts`
- Check: `frontend/src/components/request-builder/useRequestBuilderState.ts`

**Step 1: Run targeted automated tests**

Run:

```bash
cd frontend && yarn test src/store/__tests__/collections-store.fetch-dedupe.test.ts src/store/__tests__/history-store.fetch-dedupe.test.ts src/components/request-builder/useRequestBuilderState.test.tsx src/App.test.tsx
```

Expected: PASS.

**Step 2: Manually verify the browser network panel**

Run:

```bash
cd frontend && yarn dev --host 127.0.0.1 --port 5173
```

Open the request that uses `pm.variables.set(...)`, click `Send`, and
confirm:

- one `GET /health`
- one `POST /api/v1/requests/send`
- one `GET /api/v1/history?limit=50`
- one `POST /api/v1/collections/:name/variables`
- no duplicate `GET /api/v1/collections/:name/variables` from the
  websocket echo

Also verify that editing `collection.bru` externally still triggers one
variables refresh.

**Step 3: Commit any final test adjustments**

```bash
git add frontend/src/store/__tests__/collections-store.fetch-dedupe.test.ts frontend/src/store/__tests__/history-store.fetch-dedupe.test.ts frontend/src/components/request-builder/useRequestBuilderState.test.tsx frontend/src/App.test.tsx frontend/src/App.tsx frontend/src/store/collections.ts
git commit -m "fix(send): dedupe collection variable echo refresh"
```
