# Workspace Toolbar Plan 1: Foundation — Types, Stores & Git UI Removal

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the GitTab type, sandbox store, and extend pane store with collection-keyed tab state. Remove the old VSCode-style git UI from the sidebar and status bar.

**Architecture:** Extend `pane-types.ts` with a new `GitTab` interface and type guard. Create a `useSandboxStore` for JS sandbox mode. Extend `usePaneStore` with `activeCollection`, `collectionTabState`, and `switchCollection` action. Remove git sidebar tab, git bottom bar, and file-level git badges.

**Tech Stack:** TypeScript, Zustand, React, Vitest

**Spec:** `workspace-toolbar-design.md`

---

## Task 1: Add GitTab type and type guard to pane-types.ts

**Files:**
- Modify: `src/types/pane-types.ts`
- Test: `src/stores/__tests__/pane-store.test.ts` (type guard test added here)

- [ ] **Step 1: Write the failing test for isGitTab type guard**

Add to `src/stores/__tests__/pane-store.test.ts` at the bottom of the file, inside the existing `describe('pane-store', ...)` block:

```typescript
// At top of file, add to the import:
// import { isRequestTab, isGitTab } from '@/types/pane-types';

it('isGitTab returns true for git tabs and false for others', () => {
  const gitTab = {
    id: 'git:test',
    title: 'Git',
    tabType: 'git' as const,
    collectionName: 'test',
    collectionPath: '/path/to/test',
    isDirty: false,
  };
  const requestTab = makeTab();
  expect(isGitTab(gitTab)).toBe(true);
  expect(isGitTab(requestTab)).toBe(false);
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/stores/__tests__/pane-store.test.ts --reporter=verbose`
Expected: FAIL — `isGitTab` is not exported from `@/types/pane-types`

- [ ] **Step 3: Add GitTab interface and isGitTab type guard**

In `src/types/pane-types.ts`, add after the `ConflictTab` interface (around line 55):

```typescript
export interface GitTab extends BaseTab {
  tabType: 'git';
  collectionName: string;
  collectionPath: string;
}
```

Update the `Tab` union type (currently around line 58):

```typescript
export type Tab = RequestTab | CollectionTab | DiffTab | ConflictTab | GitTab;
```

Add the type guard after the existing `isConflictTab` function (around line 68):

```typescript
export function isGitTab(tab: Tab): tab is GitTab {
  return tab.tabType === 'git';
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/stores/__tests__/pane-store.test.ts --reporter=verbose`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/types/pane-types.ts src/stores/__tests__/pane-store.test.ts
git commit -m "feat(types): add GitTab interface and isGitTab type guard"
```

---

## Task 2: Create sandbox store

**Files:**
- Create: `src/stores/sandbox-store.ts`
- Create: `src/stores/__tests__/sandbox-store.test.ts`

- [ ] **Step 1: Write the failing test**

Create `src/stores/__tests__/sandbox-store.test.ts`:

```typescript
import { describe, it, expect, beforeEach } from 'vitest';
import { useSandboxStore } from '../sandbox-store';

describe('sandbox-store', () => {
  beforeEach(() => {
    localStorage.clear();
    useSandboxStore.setState({ mode: 'safe' });
  });

  it('defaults to safe mode', () => {
    expect(useSandboxStore.getState().mode).toBe('safe');
  });

  it('setMode changes the mode', () => {
    useSandboxStore.getState().setMode('developer');
    expect(useSandboxStore.getState().mode).toBe('developer');
  });

  it('persists mode to localStorage', () => {
    useSandboxStore.getState().setMode('developer');
    expect(localStorage.getItem('rocket-sandbox-mode')).toBe('developer');
  });

  it('reads initial mode from localStorage', () => {
    localStorage.setItem('rocket-sandbox-mode', 'developer');
    // Re-create store state from localStorage
    const stored = localStorage.getItem('rocket-sandbox-mode');
    const mode = stored === 'developer' ? 'developer' : 'safe';
    expect(mode).toBe('developer');
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/stores/__tests__/sandbox-store.test.ts --reporter=verbose`
Expected: FAIL — module not found

- [ ] **Step 3: Implement sandbox store**

Create `src/stores/sandbox-store.ts`:

```typescript
import { create } from 'zustand';

const STORAGE_KEY = 'rocket-sandbox-mode';

type SandboxMode = 'safe' | 'developer';

function readPersistedMode(): SandboxMode {
  const stored = localStorage.getItem(STORAGE_KEY);
  return stored === 'developer' ? 'developer' : 'safe';
}

interface SandboxState {
  mode: SandboxMode;
  setMode: (mode: SandboxMode) => void;
}

export const useSandboxStore = create<SandboxState>((set) => ({
  mode: readPersistedMode(),

  setMode(mode) {
    localStorage.setItem(STORAGE_KEY, mode);
    set({ mode });
  },
}));
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/stores/__tests__/sandbox-store.test.ts --reporter=verbose`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/stores/sandbox-store.ts src/stores/__tests__/sandbox-store.test.ts
git commit -m "feat(store): add sandbox store for JS safe/developer mode"
```

---

## Task 3: Extend pane store with collection-keyed tab state

**Files:**
- Modify: `src/stores/pane-store.ts`
- Modify: `src/types/pane-types.ts` (no changes needed — Tab type already done)
- Test: `src/stores/__tests__/pane-store.test.ts`

- [ ] **Step 1: Write the failing tests for switchCollection**

Add to `src/stores/__tests__/pane-store.test.ts`, inside the main `describe` block:

```typescript
// ── switchCollection ────────────────────────────────────────────────

it('switchCollection snapshots current tabs and restores target', () => {
  const tab1 = makeTab();
  const tab2 = makeTab();
  usePaneStore.getState().openTab(tab1);
  usePaneStore.getState().setActiveCollection('collectionA');

  // Switch to collectionB (no tabs yet)
  usePaneStore.getState().switchCollection('collectionB');
  const leafAfterSwitch = getLeaf();
  expect(leafAfterSwitch.tabs).toHaveLength(0);
  expect(usePaneStore.getState().activeCollection).toBe('collectionB');

  // Open a tab in collectionB
  usePaneStore.getState().openTab(tab2);

  // Switch back to collectionA — should restore tab1
  usePaneStore.getState().switchCollection('collectionA');
  const leafBack = getLeaf();
  expect(leafBack.tabs).toHaveLength(1);
  expect(leafBack.tabs[0].id).toBe(tab1.id);
  expect(leafBack.activeTabId).toBe(tab1.id);
});

it('switchCollection to never-opened collection shows empty tabs', () => {
  usePaneStore.getState().setActiveCollection('existingCol');
  usePaneStore.getState().openTab(makeTab());

  usePaneStore.getState().switchCollection('brandNewCol');
  const leaf = getLeaf();
  expect(leaf.tabs).toHaveLength(0);
  expect(leaf.activeTabId).toBe('');
});

it('getOpenTabCount returns correct count per collection', () => {
  usePaneStore.getState().setActiveCollection('colA');
  usePaneStore.getState().openTab(makeTab());
  usePaneStore.getState().openTab(makeTab());

  usePaneStore.getState().switchCollection('colB');
  usePaneStore.getState().openTab(makeTab());

  expect(usePaneStore.getState().getOpenTabCount('colA')).toBe(2);
  expect(usePaneStore.getState().getOpenTabCount('colB')).toBe(1);
  expect(usePaneStore.getState().getOpenTabCount('colC')).toBe(0);
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npx vitest run src/stores/__tests__/pane-store.test.ts --reporter=verbose`
Expected: FAIL — `setActiveCollection`, `switchCollection`, `getOpenTabCount` are not functions on the store

- [ ] **Step 3: Add collection-keyed state and actions to pane-store.ts**

In `src/stores/pane-store.ts`, add to the `PaneState` interface:

```typescript
// Add these fields after activeGroupId:
activeCollection: string | null;
collectionTabState: Record<string, { tabs: Tab[]; activeTabId: string }>;

// Add these actions:
setActiveCollection: (name: string) => void;
switchCollection: (name: string) => void;
getOpenTabCount: (collection: string) => number;
```

Update `buildInitialState` to include the new fields:

```typescript
function buildInitialState(): Pick<PaneState, 'root' | 'activeGroupId' | 'activeCollection' | 'collectionTabState'> {
  const leaf = createDefaultLeaf();
  return { root: leaf, activeGroupId: leaf.groupId, activeCollection: null, collectionTabState: {} };
}
```

Add the implementations inside the `create<PaneState>` call:

```typescript
setActiveCollection(name) {
  set({ activeCollection: name });
},

switchCollection(name) {
  const { root, activeGroupId, activeCollection, collectionTabState } = get();
  const activeLeaf = findActiveLeaf(root, activeGroupId);

  // Snapshot current collection's tabs
  const updatedState = { ...collectionTabState };
  if (activeCollection) {
    updatedState[activeCollection] = {
      tabs: activeLeaf.tabs,
      activeTabId: activeLeaf.activeTabId,
    };
  }

  // Restore target collection's tabs
  const targetState = updatedState[name];
  const restoredTabs = targetState?.tabs ?? [];
  const restoredActiveTabId = targetState?.activeTabId ?? '';

  const newRoot = updateLeaf(root, activeGroupId, (leaf) => ({
    ...leaf,
    tabs: restoredTabs,
    activeTabId: restoredActiveTabId,
  }));

  set({
    root: newRoot,
    activeCollection: name,
    collectionTabState: updatedState,
  });
},

getOpenTabCount(collection) {
  const { activeCollection, collectionTabState, root, activeGroupId } = get();
  if (collection === activeCollection) {
    const leaf = findActiveLeaf(root, activeGroupId);
    return leaf.tabs.length;
  }
  return collectionTabState[collection]?.tabs.length ?? 0;
},
```

Update the `reset` function to include the new fields:

```typescript
reset() {
  set(buildInitialState());
},
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run src/stores/__tests__/pane-store.test.ts --reporter=verbose`
Expected: PASS (all existing + new tests)

- [ ] **Step 5: Commit**

```bash
git add src/stores/pane-store.ts src/stores/__tests__/pane-store.test.ts
git commit -m "feat(store): add collection-keyed tab state with switchCollection"
```
