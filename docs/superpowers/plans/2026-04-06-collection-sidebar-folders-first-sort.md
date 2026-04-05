# Collection Sidebar: Folders-First Sort — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Sort the collection sidebar tree so folders always appear before requests at every level, with each group sorted alphabetically (case-insensitive).

**Architecture:** A single pure utility function `sortItemsFoldersFirst` is added to `src/lib/collection-utils.ts`. Both `CollectionNode` and `FolderNode` call it on their `filteredItems` array before rendering. No backend changes.

**Tech Stack:** TypeScript, React, Vitest (test runner matching the rest of `src/lib/__tests__/`)

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `src/lib/collection-utils.ts` | Create | Exports `sortItemsFoldersFirst` pure function |
| `src/lib/__tests__/collection-utils.test.ts` | Create | Unit tests for the sort function |
| `src/components/collections/CollectionNode.tsx` | Modify (line 206) | Apply sort after filter before render |
| `src/components/collections/FolderNode.tsx` | Modify (line 143) | Apply sort after filter before render |

---

## Task 1: Create the sort utility with tests (TDD)

**Files:**
- Create: `src/lib/collection-utils.ts`
- Create: `src/lib/__tests__/collection-utils.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `src/lib/__tests__/collection-utils.test.ts`:

```typescript
import { describe, expect, it } from 'vitest';
import { sortItemsFoldersFirst } from '../collection-utils';
import type { CollectionItem } from '../tauri-api';

// Minimal helpers to build test fixtures without filling every field.
const folder = (name: string): CollectionItem =>
  ({ type: 'folder', uid: name, name, items: [] }) as CollectionItem;

const request = (name: string): CollectionItem =>
  ({
    type: 'request',
    uid: name,
    name,
    method: 'GET',
    url: '',
    headers: [],
    body: { mode: 'none' },
    auth: { type: 'none' },
  }) as CollectionItem;

describe('sortItemsFoldersFirst', () => {
  it('returns empty array unchanged', () => {
    expect(sortItemsFoldersFirst([])).toEqual([]);
  });

  it('sorts all folders alphabetically', () => {
    const result = sortItemsFoldersFirst([folder('Zebra'), folder('alpha'), folder('Mango')]);
    expect(result.map((i) => i.name)).toEqual(['alpha', 'Mango', 'Zebra']);
  });

  it('sorts all requests alphabetically', () => {
    const result = sortItemsFoldersFirst([request('Zebra'), request('alpha'), request('Mango')]);
    expect(result.map((i) => i.name)).toEqual(['alpha', 'Mango', 'Zebra']);
  });

  it('places all folders before all requests', () => {
    const result = sortItemsFoldersFirst([
      request('A Request'),
      folder('Z Folder'),
      request('B Request'),
      folder('A Folder'),
    ]);
    expect(result.map((i) => i.name)).toEqual(['A Folder', 'Z Folder', 'A Request', 'B Request']);
  });

  it('sorts case-insensitively within each group', () => {
    const result = sortItemsFoldersFirst([folder('zebra'), folder('Alpha'), folder('MANGO')]);
    expect(result.map((i) => i.name)).toEqual(['Alpha', 'MANGO', 'zebra']);
  });

  it('does not mutate the input array', () => {
    const input = [request('B'), folder('A')];
    const copy = [...input];
    sortItemsFoldersFirst(input);
    expect(input).toEqual(copy);
  });
});
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cd /home/numericlabs/data/rocket/rocket
yarn test src/lib/__tests__/collection-utils.test.ts
```

Expected: `Cannot find module '../collection-utils'` or similar import error.

- [ ] **Step 3: Implement the utility**

Create `src/lib/collection-utils.ts`:

```typescript
import type { CollectionItem } from './tauri-api';

/**
 * Returns a new array with folders sorted before requests.
 * Within each group items are sorted alphabetically (case-insensitive).
 * The input array is not mutated.
 */
export function sortItemsFoldersFirst(items: CollectionItem[]): CollectionItem[] {
  return [...items].sort((a, b) => {
    const aIsFolder = a.type === 'folder' ? 0 : 1;
    const bIsFolder = b.type === 'folder' ? 0 : 1;
    if (aIsFolder !== bIsFolder) return aIsFolder - bIsFolder;
    return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
  });
}
```

- [ ] **Step 4: Run tests to confirm they pass**

```bash
yarn test src/lib/__tests__/collection-utils.test.ts
```

Expected: all 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/lib/collection-utils.ts src/lib/__tests__/collection-utils.test.ts
git commit -m "feat(ui): add sortItemsFoldersFirst utility with tests"
```

---

## Task 2: Apply sort in CollectionNode

**Files:**
- Modify: `src/components/collections/CollectionNode.tsx` (around line 201–206)

- [ ] **Step 1: Import the utility**

At the top of `CollectionNode.tsx`, add the import alongside existing lib imports:

```typescript
import { sortItemsFoldersFirst } from '@/lib/collection-utils';
```

- [ ] **Step 2: Apply sort to filteredItems**

Find this block (lines 201–206):

```typescript
  const rawItems = collection?.root.items ?? [];
  const filteredItems = filter
    ? rawItems.filter(
        (item) => item.type !== 'request' || item.name.toLowerCase().includes(filter.toLowerCase()),
      )
    : rawItems;
```

Replace with:

```typescript
  const rawItems = collection?.root.items ?? [];
  const filteredItems = sortItemsFoldersFirst(
    filter
      ? rawItems.filter(
          (item) => item.type !== 'request' || item.name.toLowerCase().includes(filter.toLowerCase()),
        )
      : rawItems,
  );
```

- [ ] **Step 3: Run the full test suite**

```bash
yarn test
```

Expected: all tests pass, no TypeScript errors.

- [ ] **Step 4: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add src/components/collections/CollectionNode.tsx
git commit -m "feat(ui): sort collection root items folders-first"
```

---

## Task 3: Apply sort in FolderNode

**Files:**
- Modify: `src/components/collections/FolderNode.tsx` (around line 139–143)

- [ ] **Step 1: Import the utility**

At the top of `FolderNode.tsx`, add the import alongside existing lib imports:

```typescript
import { sortItemsFoldersFirst } from '@/lib/collection-utils';
```

- [ ] **Step 2: Apply sort to filteredItems**

Find this block (lines 139–143):

```typescript
  const filteredItems = filter
    ? items.filter(
        (item) => item.type !== 'request' || item.name.toLowerCase().includes(filter.toLowerCase()),
      )
    : items;
```

Replace with:

```typescript
  const filteredItems = sortItemsFoldersFirst(
    filter
      ? items.filter(
          (item) => item.type !== 'request' || item.name.toLowerCase().includes(filter.toLowerCase()),
        )
      : items,
  );
```

- [ ] **Step 3: Run the full test suite**

```bash
yarn test
```

Expected: all tests pass.

- [ ] **Step 4: TypeScript and lint check**

```bash
yarn tsc --noEmit && yarn check
```

Expected: no errors or warnings.

- [ ] **Step 5: Commit**

```bash
git add src/components/collections/FolderNode.tsx
git commit -m "feat(ui): sort folder children folders-first"
```

---

## Done

After Task 3, every level of the collection tree renders folders before requests, each group alphabetically sorted. The sort logic lives in one tested function with no backend changes.
