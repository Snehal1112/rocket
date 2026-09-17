# Collection Runner Plan A: Types and Tree Flattening — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the `RunnerTab` tab type and a pure utility that flattens a
collection/folder tree into an ordered, run-ready list of requests.

**Architecture:** `RunnerTab` joins the existing `Tab` union in
`src/types/pane-types.ts`, following the exact pattern of `ContractTab`
(a `BaseTab` extension with a `tabType` discriminant and a paired
`isRunnerTab` type guard). `flattenRunnerEntries` is a new pure function
in `src/lib/runner-flatten.ts` that walks a `Collection`'s folder tree
(as returned by `getCollection()`) the same way `FolderNode.tsx` already
does for rendering, producing a flat, ordered `RunnerRequestEntry[]`.

**Tech Stack:** TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-09-17-collection-runner-frontend-design.md`

## Global Constraints

- Tab discriminant field is `tabType` (not `type` — `type` is reserved
  for `PaneNode`'s split/leaf discriminant). Every new tab-related type
  in this plan uses `tabType`.
- No backend/Rust changes in this plan or any collection-runner plan.
- The spec (and early drafts of this plan) called the collection
  identifier `collectionRoot`, but `getCollection()`/`getCollectionSettings()`/
  `getFolderChainVariables()`/`getRequestVariables()`/`ExecuteRequestInput.collection`
  all key off the collection's short **name** (`getCollection(summary.name)`,
  confirmed at `src/components/collections/CollectionNode.tsx:141`), not its
  absolute root path (that's what `ContractTab.collectionRoot` is for, a
  separate, contract-IPC-specific field). Every collection-runner type and
  function in this plan set therefore uses `collectionName`, matching the
  identifier actually passed to those calls.
- Request tree order must exactly match what the sidebar displays: apply
  `sortItemsFoldersFirst` (from `src/lib/collection-utils.ts`) at every
  folder level, and build paths using the same formula `FolderNode.tsx`
  uses: folder path = `${basePath}/${item.dirName ?? item.name}`,
  request path = `${basePath}/${item.fileName ?? item.name}`, with the
  root basePath being `''` (matching `CollectionNode.tsx`'s
  `basePath={folderDirName}` seed for its top-level folder items, and
  `onNewFolder(summary.name, '')` for direct root-level items).

---

### Task 1: RunnerTab type and type guard

**Files:**
- Modify: `src/types/pane-types.ts`
- Test: `src/types/__tests__/pane-types.test.ts` (create — no existing
  file for this module; follow the plain `describe`/`it` Vitest style
  used elsewhere, e.g. `src/lib/__tests__/collection-utils.test.ts`)

**Interfaces:**
- Produces: `RunnerTab`, `RunnerRunState`, `RunnerRequestEntry`,
  `isRunnerTab(tab: Tab): tab is RunnerTab` — consumed by every later
  task in Plans C, D, and E.

- [ ] **Step 1: Write the failing test**

```ts
// src/types/__tests__/pane-types.test.ts
import { describe, expect, it } from 'vitest';
import { isRunnerTab } from '@/types/pane-types';
import type { RequestTab, RunnerTab } from '@/types/pane-types';

describe('isRunnerTab', () => {
  it('returns true for a runner tab', () => {
    const tab: RunnerTab = {
      id: 'runner-1',
      title: 'Runner',
      isDirty: false,
      tabType: 'runner',
      collectionName: 'demo',
      runState: 'idle',
      requests: [],
    };
    expect(isRunnerTab(tab)).toBe(true);
  });

  it('returns false for a non-runner tab', () => {
    const tab: RequestTab = {
      id: 'req-1',
      title: 'Request',
      isDirty: false,
      tabType: 'request',
      request: {} as RequestTab['request'],
      response: null,
    };
    expect(isRunnerTab(tab)).toBe(false);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn vitest run src/types/__tests__/pane-types.test.ts`
Expected: FAIL — `isRunnerTab` is not exported, `RunnerTab` type does
not exist (TypeScript/Vitest compile error).

- [ ] **Step 3: Add the types and guard**

In `src/types/pane-types.ts`, add after the existing `ContractDiffTab`
definitions (after `isContractDiffTab`, before `export type Tab =`):

```ts
export type RunnerRunState = 'idle' | 'running' | 'stopped' | 'done';

export interface RunnerRequestEntry {
  requestPath: string;
  // Full backend Request, not just name/method — startRun (Plan C) needs
  // the whole object to call executeRunnerEntry. Components read
  // entry.request.name / entry.request.method for display.
  request: import('@/lib/tauri-api').Request;
  included: boolean;
  status: 'pending' | 'running' | 'passed' | 'failed' | 'skipped';
  result?: import('@/lib/tauri-api').ExecuteRequestResponse;
  error?: string;
}

export interface RunnerTab extends BaseTab {
  tabType: 'runner';
  collectionName: string | null;
  folderPath?: string;
  runState: RunnerRunState;
  requests: RunnerRequestEntry[];
}

export function isRunnerTab(tab: Tab): tab is RunnerTab {
  return tab.tabType === 'runner';
}
```

Then add `RunnerTab` to the `Tab` union:

```ts
export type Tab =
  | RequestTab
  | CollectionTab
  | WorkspaceTab
  | DiffTab
  | ConflictTab
  | GitTab
  | ContractTab
  | ContractDiffTab
  | RunnerTab;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `yarn vitest run src/types/__tests__/pane-types.test.ts`
Expected: PASS (2 tests)

- [ ] **Step 5: Run the full type check**

Run: `yarn tsc --noEmit`
Expected: no new errors (the `Tab` union change is additive; nothing
else references it exhaustively without a `default`/fallback case yet —
this is verified for real in Plan E Task 2 when `EditorGroup.tsx`'s
ternary chain is extended).

- [ ] **Step 6: Commit**

```bash
git add src/types/pane-types.ts src/types/__tests__/pane-types.test.ts
git commit -m "feat: add RunnerTab type and isRunnerTab guard"
```

---

### Task 2: Recursive tree-flattening utility

**Files:**
- Create: `src/lib/runner-flatten.ts`
- Test: `src/lib/__tests__/runner-flatten.test.ts`

**Interfaces:**
- Consumes: `Collection`, `Folder`, `CollectionItem`, `Request` (from
  `@/lib/tauri-api`), `sortItemsFoldersFirst` (from
  `@/lib/collection-utils`), `RunnerRequestEntry` (Task 1).
- Produces: `flattenRunnerEntries(collection: Collection, folderPath?: string): RunnerRequestEntry[]`
  — consumed by Plan C Task 1 (`openRunnerTab`) and Plan C Task 2
  (`rerunAll`).

- [ ] **Step 1: Write the failing test**

```ts
// src/lib/__tests__/runner-flatten.test.ts
import { describe, expect, it } from 'vitest';
import { flattenRunnerEntries } from '@/lib/runner-flatten';
import type { Collection } from '@/lib/tauri-api';

function makeCollection(): Collection {
  return {
    name: 'demo',
    settings: { headers: [], variables: [] } as unknown as Collection['settings'],
    root: {
      uid: 'root',
      name: 'demo',
      items: [
        {
          type: 'request',
          uid: 'r1',
          name: 'Root Request',
          method: 'GET',
          url: 'https://example.com/root',
          headers: [],
          auth: { authType: 'none' },
          fileName: 'root-request.yml',
        },
        {
          type: 'folder',
          uid: 'f1',
          name: 'Auth',
          dirName: 'auth',
          items: [
            {
              type: 'request',
              uid: 'r2',
              name: 'Login',
              method: 'POST',
              url: 'https://example.com/login',
              headers: [],
              auth: { authType: 'none' },
              fileName: 'login.yml',
            },
            {
              type: 'folder',
              uid: 'f2',
              name: 'Nested',
              dirName: 'nested',
              items: [
                {
                  type: 'request',
                  uid: 'r3',
                  name: 'Refresh',
                  method: 'POST',
                  url: 'https://example.com/refresh',
                  headers: [],
                  auth: { authType: 'none' },
                  fileName: 'refresh.yml',
                },
              ],
            },
          ],
        },
      ],
    },
  };
}

describe('flattenRunnerEntries', () => {
  it('flattens the whole collection in tree order, folders first', () => {
    const entries = flattenRunnerEntries(makeCollection());
    // sortItemsFoldersFirst applies at every level, so within `auth/`
    // the `nested` folder sorts before the `login` request.
    expect(entries.map((e) => e.requestPath)).toEqual([
      'auth/nested/refresh.yml',
      'auth/login.yml',
      'root-request.yml',
    ]);
  });

  it('defaults every entry to included and pending', () => {
    const entries = flattenRunnerEntries(makeCollection());
    for (const e of entries) {
      expect(e.included).toBe(true);
      expect(e.status).toBe('pending');
    }
  });

  it('scopes to a folder when folderPath is given', () => {
    const entries = flattenRunnerEntries(makeCollection(), 'auth');
    expect(entries.map((e) => e.requestPath)).toEqual([
      'auth/nested/refresh.yml',
      'auth/login.yml',
    ]);
  });

  it('scopes to a nested folder', () => {
    const entries = flattenRunnerEntries(makeCollection(), 'auth/nested');
    expect(entries.map((e) => e.requestPath)).toEqual(['auth/nested/refresh.yml']);
  });

  it('returns an empty array for a folder with no requests', () => {
    const collection = makeCollection();
    collection.root.items = [
      { type: 'folder', uid: 'f1', name: 'Empty', dirName: 'empty', items: [] },
    ];
    expect(flattenRunnerEntries(collection)).toEqual([]);
  });

  it('returns an empty array when folderPath does not exist', () => {
    expect(flattenRunnerEntries(makeCollection(), 'does-not-exist')).toEqual([]);
  });

  it('carries the full request object onto each entry', () => {
    const entries = flattenRunnerEntries(makeCollection());
    expect(entries[0].request).toMatchObject({ name: 'Refresh', method: 'POST', uid: 'r3' });
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn vitest run src/lib/__tests__/runner-flatten.test.ts`
Expected: FAIL — `src/lib/runner-flatten.ts` does not exist.

- [ ] **Step 3: Implement the utility**

```ts
// src/lib/runner-flatten.ts
import { sortItemsFoldersFirst } from '@/lib/collection-utils';
import type { Collection, CollectionItem, Folder } from '@/lib/tauri-api';
import type { RunnerRequestEntry } from '@/types/pane-types';

// Finds the Folder at `folderPath` (relative to the collection root),
// walking the same dirName/name formula FolderNode.tsx uses to build
// paths while rendering the sidebar tree. Returns null if not found.
function findFolder(folder: Folder, basePath: string, targetPath: string): Folder | null {
  if (basePath === targetPath) return folder;
  for (const item of folder.items) {
    if (item.type !== 'folder') continue;
    const childPath = basePath ? `${basePath}/${item.dirName ?? item.name}` : (item.dirName ?? item.name);
    if (targetPath === childPath || targetPath.startsWith(`${childPath}/`)) {
      const found = findFolder(item, childPath, targetPath);
      if (found) return found;
    }
  }
  return null;
}

// Recursively collects every request under `folder` into `out`, in the
// same folders-first, alphabetical order the sidebar tree renders.
function collect(folder: Folder, basePath: string, out: RunnerRequestEntry[]): void {
  const items: CollectionItem[] = sortItemsFoldersFirst(folder.items);
  for (const item of items) {
    if (item.type === 'folder') {
      const childPath = basePath ? `${basePath}/${item.dirName ?? item.name}` : (item.dirName ?? item.name);
      collect(item, childPath, out);
    } else if (item.type === 'request') {
      const requestPath = basePath ? `${basePath}/${item.fileName ?? item.name}` : (item.fileName ?? item.name);
      out.push({
        requestPath,
        request: item,
        included: true,
        status: 'pending',
      });
    }
    // 'summary' items never appear in a getCollection() result (only in
    // getCollectionSummaries()); flattenRunnerEntries is only ever
    // called with a full Collection, so no 'summary' branch is needed.
  }
}

// Flattens a collection (or one folder within it) into an ordered list
// of runnable requests, matching the order the sidebar tree displays.
// Every entry starts out included and pending.
export function flattenRunnerEntries(
  collection: Collection,
  folderPath?: string,
): RunnerRequestEntry[] {
  const startFolder = folderPath ? findFolder(collection.root, '', folderPath) : collection.root;
  if (!startFolder) return [];
  const out: RunnerRequestEntry[] = [];
  collect(startFolder, folderPath ?? '', out);
  return out;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `yarn vitest run src/lib/__tests__/runner-flatten.test.ts`
Expected: PASS (7 tests)

- [ ] **Step 5: Run the full frontend test suite and type check**

Run: `yarn vitest run && yarn tsc --noEmit`
Expected: PASS, no new errors.

- [ ] **Step 6: Commit**

```bash
git add src/lib/runner-flatten.ts src/lib/__tests__/runner-flatten.test.ts
git commit -m "feat: add flattenRunnerEntries collection tree utility"
```
