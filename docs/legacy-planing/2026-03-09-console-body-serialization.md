# Console Body Serialization Fix Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Move `HttpResponse.body` serialization from the console store band-aid to the correct layer — `api.ts` — so all consumers of `HttpResponse` receive a guaranteed `string`.

**Architecture:** Add a `normalizeBody` private method to `ApiService` that converts any non-string body to pretty-printed JSON at the one construction point in `sendRequest`. Remove the now-redundant `typeof` guard from `console.ts`.

**Tech Stack:** TypeScript, Vitest, Zustand

---

### Task 1: Add `normalizeBody` to `ApiService` and apply it

**Files:**
- Modify: `frontend/src/lib/api.ts`

**Step 1: Write the failing test**

Create `frontend/src/lib/__tests__/api.test.ts` (or add to existing if present):

```ts
// This test verifies that sendRequest normalizes a non-string body
// We test this indirectly via the returned HttpResponse.body type guarantee
// — the real verification is the TypeScript compile + integration
```

Actually the normalization is a private method — verify via TypeScript instead.

**Step 1: Add `normalizeBody` private method**

In `frontend/src/lib/api.ts`, before the closing `}` of the `ApiService` class, add:

```ts
  private normalizeBody(body: unknown): string {
    if (typeof body === 'string') return body
    if (body == null) return ''
    return JSON.stringify(body, null, 2)
  }
```

**Step 2: Apply it at the success return site**

In `sendRequest`, change:

```diff
-       body: response.data.data.body,
+       body: this.normalizeBody(response.data.data.body),
```

**Step 3: Verify TypeScript compiles**

```bash
cd /home/numericlabs/data/rocket-api/frontend && yarn tsc --noEmit
```

Expected: no errors.

**Step 4: Run all tests to confirm nothing broke**

```bash
cd /home/numericlabs/data/rocket-api/frontend && yarn test --run 2>&1 | tail -10
```

Expected: all tests pass.

**Step 5: Commit**

```bash
cd /home/numericlabs/data/rocket-api
git add frontend/src/lib/api.ts
git commit -m "fix(api): normalize non-string response body to string in sendRequest"
```

---

### Task 2: Remove the band-aid from `console.ts`

**Files:**
- Modify: `frontend/src/store/console.ts`

**Step 1: Remove the `typeof` guard**

In `frontend/src/store/console.ts`, change:

```diff
-       responseBody: typeof res.body === 'string'
-         ? res.body
-         : JSON.stringify(res.body, null, 2),
+       responseBody: res.body,
```

**Step 2: Verify TypeScript compiles**

```bash
cd /home/numericlabs/data/rocket-api/frontend && yarn tsc --noEmit
```

Expected: no errors.

**Step 3: Run store tests**

```bash
cd /home/numericlabs/data/rocket-api/frontend && yarn test src/store/__tests__/console.test.ts --reporter=verbose 2>&1 | tail -15
```

Expected: 7 tests PASS.

**Step 4: Commit**

```bash
cd /home/numericlabs/data/rocket-api
git add frontend/src/store/console.ts
git commit -m "refactor(console): remove typeof body guard — normalized at API boundary"
```

---

### Task 3: Run full test suite and verify

**Step 1: Run all frontend tests**

```bash
cd /home/numericlabs/data/rocket-api/frontend && yarn test --reporter=verbose 2>&1 | tail -15
```

Expected: all test files pass.

**Step 2: Check for TypeScript errors**

```bash
cd /home/numericlabs/data/rocket-api/frontend && yarn tsc --noEmit
```

Expected: no errors.
