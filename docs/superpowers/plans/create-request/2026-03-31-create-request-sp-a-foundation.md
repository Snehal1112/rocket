# Create Request — SP-A: Foundation

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Lay the two shared primitives that all three Create Request sub-projects depend on — a `sanitizeFilename` utility and an `openEphemeralTab` store action.

**Architecture:** `sanitizeFilename` is a pure function in `src/lib/filename-utils.ts` with no dependencies. `openEphemeralTab` is a new action on the existing `usePaneStore` Zustand store; it opens a `RequestTab` with no `source` (no collection/path binding) and an optional `requestType` field. Both are fully unit-tested before any UI work begins.

**Tech Stack:** TypeScript, Vitest, Zustand (`usePaneStore`), existing `src/lib/pane-utils.ts` (`createDefaultRequest`), existing `src/types/pane-types.ts` (`RequestState`, `RequestTab`).

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `src/lib/filename-utils.ts` | **Create** | `sanitizeFilename(displayName): string` — maps unsafe chars to `-`, appends `.yml` |
| `src/lib/__tests__/filename-utils.test.ts` | **Create** | Unit tests for all sanitizeFilename edge cases |
| `src/types/pane-types.ts` | **Modify** | Add `requestType` field to `RequestState` |
| `src/lib/pane-utils.ts` | **Modify** | Add `requestType: 'http'` default to `createDefaultRequest()` |
| `src/stores/pane-store.ts` | **Modify** | Add `openEphemeralTab` action to `PaneState` interface + implementation |
| `src/stores/__tests__/pane-store.test.ts` | **Modify** | Add two new tests for `openEphemeralTab` |

---

## Chunk 1: `sanitizeFilename` utility

### Task 1: Pure filename sanitization function

**Files:**
- Create: `src/lib/filename-utils.ts`
- Create: `src/lib/__tests__/filename-utils.test.ts`

Bruno replaces filesystem-unsafe characters (`/`, `\`, `:`, `*`, `?`, `"`, `<`, `>`, `|`, `[`, `]`) with `-` in the stored `.yml` filename while preserving the original display name in the UI. RocketAPI already uses `.yml` throughout.

- [ ] **Step 1: Write failing tests**

```ts
// src/lib/__tests__/filename-utils.test.ts
import { describe, it, expect } from 'vitest';
import { sanitizeFilename } from '../filename-utils';

describe('sanitizeFilename', () => {
  it('returns plain names unchanged (with .yml appended)', () => {
    expect(sanitizeFilename('Get Users')).toBe('Get Users.yml');
  });

  it('replaces forward slash with dash', () => {
    expect(sanitizeFilename('users/create')).toBe('users-create.yml');
  });

  it('replaces backslash with dash', () => {
    expect(sanitizeFilename('users\\create')).toBe('users-create.yml');
  });

  it('replaces square brackets', () => {
    expect(sanitizeFilename('items[0]')).toBe('items-0-.yml');
  });

  it('replaces asterisk', () => {
    expect(sanitizeFilename('search*')).toBe('search-.yml');
  });

  it('replaces colon', () => {
    expect(sanitizeFilename('GET: users')).toBe('GET- users.yml');
  });

  it('replaces all unsafe chars in a realistic request name', () => {
    expect(sanitizeFilename('GET /users/:id [v2]*')).toBe('GET -users--id -v2--.yml');
  });

  it('trims leading and trailing whitespace before sanitizing', () => {
    expect(sanitizeFilename('  hello  ')).toBe('hello.yml');
  });

  it('collapses consecutive dashes into one', () => {
    expect(sanitizeFilename('a//b')).toBe('a-b.yml');
  });

  it('falls back to "request" when result would be empty', () => {
    expect(sanitizeFilename('///')).toBe('request.yml');
  });

  it('falls back to "request" for blank input', () => {
    expect(sanitizeFilename('   ')).toBe('request.yml');
  });
});
```

- [ ] **Step 2: Run to verify all fail**

```bash
cd <project-root>
yarn vitest run src/lib/__tests__/filename-utils.test.ts
```

Expected: all 11 tests FAIL with `Cannot find module '../filename-utils'`.

- [ ] **Step 3: Implement `sanitizeFilename`**

```ts
// src/lib/filename-utils.ts

/**
 * Characters that are unsafe on common filesystems.
 * Covers: / \ : * ? " < > | [ ]
 */
const UNSAFE_RE = /[/\\:*?"<>|[\]]/g;

/**
 * Converts a display name into a filesystem-safe `.yml` filename.
 *
 * Rules:
 * - Leading/trailing whitespace is stripped
 * - Unsafe characters are replaced with `-`
 * - Consecutive dashes are collapsed to a single `-`
 * - Falls back to "request" if the result is empty
 * - Always appends `.yml`
 *
 * The original display name is never mutated — it is only used
 * to derive the on-disk filename.
 *
 * @example
 * sanitizeFilename('GET /users/:id [v2]*') // → 'GET -users--id -v2--.yml'
 * sanitizeFilename('Get Users')            // → 'Get Users.yml'
 * sanitizeFilename('///')                  // → 'request.yml'
 */
export function sanitizeFilename(displayName: string): string {
  const sanitized = displayName
    .trim()
    .replace(UNSAFE_RE, '-')
    .replace(/-{2,}/g, '-')
    .trim();

  return `${sanitized || 'request'}.yml`;
}
```

- [ ] **Step 4: Run tests — expect all pass**

```bash
yarn vitest run src/lib/__tests__/filename-utils.test.ts
```

Expected: 11 tests PASS, 0 failures.

- [ ] **Step 5: Commit**

```bash
git add src/lib/filename-utils.ts src/lib/__tests__/filename-utils.test.ts
git commit -m "feat: add sanitizeFilename utility for filesystem-safe request filenames"
```

---

## Chunk 2: `requestType` on `RequestState` + `openEphemeralTab` store action

### Task 2: Add `requestType` to types and `openEphemeralTab` to pane-store

**Files:**
- Modify: `src/types/pane-types.ts`
- Modify: `src/lib/pane-utils.ts`
- Modify: `src/stores/pane-store.ts`
- Modify: `src/stores/__tests__/pane-store.test.ts`

`RequestState` needs a `requestType` discriminator so the UI can render the correct editor (HTTP vs GraphQL vs gRPC vs WebSocket). `openEphemeralTab` creates a sourceless `RequestTab` — no collection binding, no file on disk — ready for the user to fill in before saving.

- [ ] **Step 1: Check whether `requestType` already exists on `RequestState`**

```bash
grep -n "requestType" src/types/pane-types.ts src/lib/pane-utils.ts
```

If `requestType` is already present, skip Steps 2–3 and proceed to Step 4.

- [ ] **Step 2: Add `requestType` to `RequestState`**

In `src/types/pane-types.ts`, find the `RequestState` interface/type and add:

```ts
requestType: 'http' | 'graphql' | 'grpc' | 'websocket';
```

Place it as the first field for visibility. The full field should look like:

```ts
export interface RequestState {
  requestType: 'http' | 'graphql' | 'grpc' | 'websocket';
  method: HttpMethod;
  url: string;
  // ... rest of existing fields unchanged
}
```

- [ ] **Step 3: Add `requestType` default to `createDefaultRequest`**

In `src/lib/pane-utils.ts`, find `createDefaultRequest()` and add:

```ts
requestType: 'http',
```

as the first field of the returned object. The function should now return something like:

```ts
export function createDefaultRequest(): RequestState {
  return {
    requestType: 'http',
    method: 'GET',
    url: '',
    // ... rest of existing fields unchanged
  };
}
```

- [ ] **Step 4: Write failing tests for `openEphemeralTab`**

In `src/stores/__tests__/pane-store.test.ts`, add inside the existing `describe('pane-store')` block:

```ts
describe('openEphemeralTab', () => {
  it('opens a request tab with no source and title "Untitled"', () => {
    usePaneStore.getState().openEphemeralTab();
    const leaf = getLeaf();
    expect(leaf.tabs).toHaveLength(1);
    const tab = leaf.tabs[0];
    expect(tab.tabType).toBe('request');
    expect(tab.title).toBe('Untitled');
    if (isRequestTab(tab)) {
      expect(tab.source).toBeUndefined();
      expect(tab.isDirty).toBe(false);
      expect(tab.request.requestType).toBe('http');
    }
  });

  it('openEphemeralTab with "graphql" sets requestType correctly', () => {
    usePaneStore.getState().openEphemeralTab('graphql');
    const leaf = getLeaf();
    const tab = leaf.tabs[0];
    expect(isRequestTab(tab)).toBe(true);
    if (isRequestTab(tab)) {
      expect(tab.request.requestType).toBe('graphql');
    }
  });
});
```

- [ ] **Step 5: Run to verify both new tests fail**

```bash
yarn vitest run src/stores/__tests__/pane-store.test.ts
```

Expected: 2 new tests FAIL with `openEphemeralTab is not a function`. All pre-existing tests must still PASS.

- [ ] **Step 6: Add `openEphemeralTab` to `PaneState` interface**

In `src/stores/pane-store.ts`, add to the `PaneState` interface (after `openTab`):

```ts
openEphemeralTab: (requestType?: 'http' | 'graphql' | 'grpc' | 'websocket') => void;
```

- [ ] **Step 7: Implement `openEphemeralTab`**

In the `usePaneStore` implementation, add after the `openTab` implementation:

```ts
openEphemeralTab(requestType = 'http') {
  const tab: RequestTab = {
    id: crypto.randomUUID(),
    title: 'Untitled',
    tabType: 'request',
    request: { ...createDefaultRequest(), requestType },
    response: null,
    isDirty: false,
    // Intentionally no `source` — ephemeral until saved to a collection.
  };
  get().openTab(tab);
},
```

Make sure `createDefaultRequest` is imported at the top of `pane-store.ts`:

```ts
import { createDefaultRequest, /* ...other imports */ } from '@/lib/pane-utils';
```

Also ensure `RequestTab` is imported from `@/types/pane-types`.

- [ ] **Step 8: Run full pane-store test suite — expect all pass**

```bash
yarn vitest run src/stores/__tests__/pane-store.test.ts
```

Expected: all pre-existing tests PASS + 2 new tests PASS.

- [ ] **Step 9: Run full test suite to check for regressions from `requestType` addition**

```bash
yarn vitest run
```

Expected: no new failures. If TypeScript errors appear about `requestType` missing in other places that construct `RequestState`, add `requestType: 'http'` to each call site.

- [ ] **Step 10: Commit**

```bash
git add src/types/pane-types.ts src/lib/pane-utils.ts src/stores/pane-store.ts src/stores/__tests__/pane-store.test.ts
git commit -m "feat: add requestType to RequestState and openEphemeralTab to pane-store"
```

---

## Definition of Done

- [ ] `yarn vitest run src/lib/__tests__/filename-utils.test.ts` → 11 PASS
- [ ] `yarn vitest run src/stores/__tests__/pane-store.test.ts` → all pre-existing + 2 new PASS
- [ ] `yarn vitest run` → no regressions across full suite
- [ ] `sanitizeFilename` is exported from `src/lib/filename-utils.ts`
- [ ] `openEphemeralTab` is on `PaneState` interface and implemented in `usePaneStore`
- [ ] No UI changes in this SP — zero modifications to any component files
