# Architecture Plan 3 — Layout Store + Persistence

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move `sidebarWidth`, `isConsoleOpen`, and `consoleHeight` from `App.tsx` local state into `layout-store`, persist all three values to disk via `ui-state.ts`, and restore them on startup.

**Architecture:** `layout-store.ts` gains three new fields and setters. `ui-state.ts` reads them during save and hydrates them on restore. `App.tsx` removes three `useState` calls and reads from the store instead. The dead `sidebarCollapsed` state is removed entirely.

**Prerequisite:** Plans 1 and 2 do not need to be complete — this plan is fully independent and can run in parallel.

**Tech Stack:** Zustand v5, existing `ui-state.ts` / `tauri-api.ts` persistence layer.

**Spec:** `docs/superpowers/specs/2026-05-01-tanstack-query-layout-store-design.md`

---

### Task 1: Extend layout-store with sidebar and console state

**Files:**
- Modify: `src/stores/layout-store.ts`
- Test: `src/stores/__tests__/layout-store.test.ts`

- [ ] **Step 1: Write the failing tests**

Open `src/stores/__tests__/layout-store.test.ts`. The existing file tests `requestLayout`. Add tests for the three new fields:

```ts
import { useLayoutStore } from '../layout-store';

describe('layout-store — sidebar and console state', () => {
  beforeEach(() => {
    useLayoutStore.setState({
      sidebarWidth: 280,
      isConsoleOpen: false,
      consoleHeight: 280,
    });
  });

  it('setSidebarWidth updates sidebarWidth', () => {
    useLayoutStore.getState().setSidebarWidth(350);
    expect(useLayoutStore.getState().sidebarWidth).toBe(350);
  });

  it('setConsoleOpen toggles isConsoleOpen', () => {
    useLayoutStore.getState().setConsoleOpen(true);
    expect(useLayoutStore.getState().isConsoleOpen).toBe(true);
    useLayoutStore.getState().setConsoleOpen(false);
    expect(useLayoutStore.getState().isConsoleOpen).toBe(false);
  });

  it('setConsoleHeight updates consoleHeight', () => {
    useLayoutStore.getState().setConsoleHeight(400);
    expect(useLayoutStore.getState().consoleHeight).toBe(400);
  });

  it('has correct default values', () => {
    const s = useLayoutStore.getState();
    expect(s.sidebarWidth).toBe(280);
    expect(s.isConsoleOpen).toBe(false);
    expect(s.consoleHeight).toBe(280);
  });
});
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
yarn test layout-store
```

Expected: 4 new tests fail with "setSidebarWidth is not a function" or similar.

- [ ] **Step 3: Extend `src/stores/layout-store.ts`**

Replace the full contents:

```ts
import { create } from 'zustand';

type RequestLayout = 'stacked' | 'side-by-side';

interface LayoutStore {
  requestLayout: RequestLayout;
  sidebarWidth: number;
  isConsoleOpen: boolean;
  consoleHeight: number;

  setRequestLayout: (dir: RequestLayout) => void;
  setSidebarWidth: (w: number) => void;
  setConsoleOpen: (open: boolean) => void;
  setConsoleHeight: (h: number) => void;
}

export const useLayoutStore = create<LayoutStore>()((set) => ({
  requestLayout: 'stacked',
  sidebarWidth: 280,
  isConsoleOpen: false,
  consoleHeight: 280,

  setRequestLayout: (dir) => set({ requestLayout: dir }),
  setSidebarWidth: (w) => set({ sidebarWidth: w }),
  setConsoleOpen: (open) => set({ isConsoleOpen: open }),
  setConsoleHeight: (h) => set({ consoleHeight: h }),
}));
```

- [ ] **Step 4: Run tests to confirm they pass**

```bash
yarn test layout-store
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/stores/layout-store.ts src/stores/__tests__/layout-store.test.ts
git commit -m "feat: extend layout-store with sidebar width, console open/height"
```

---

### Task 2: Extend UiState type and persistence functions

**Files:**
- Modify: `src/lib/tauri-api.ts`
- Modify: `src/lib/ui-state.ts`
- Test: `src/lib/__tests__/ui-state.test.ts` (new)

- [ ] **Step 1: Write the failing test**

Create `src/lib/__tests__/ui-state.test.ts`:

```ts
import { vi, it, expect, beforeEach, describe } from 'vitest';

// Mock tauri-api before importing ui-state
vi.mock('@/lib/tauri-api', () => ({
  saveUiState: vi.fn().mockResolvedValue(undefined),
  loadUiState: vi.fn(),
}));

vi.mock('@/stores/pane-store', () => ({
  usePaneStore: {
    getState: () => ({
      isWorkspaceMode: () => false,
      activeCollection: 'my-collection',
      root: { type: 'leaf', tabs: [], activeTabId: '', groupId: 'g1' },
    }),
  },
}));

vi.mock('@/stores/layout-store', () => ({
  useLayoutStore: {
    getState: () => ({
      requestLayout: 'stacked',
      sidebarWidth: 350,
      isConsoleOpen: true,
      consoleHeight: 400,
    }),
    subscribe: vi.fn(() => () => {}),
  },
}));

import { saveUiState } from '@/lib/tauri-api';
import { scheduleSaveUiState } from '@/lib/ui-state';

describe('scheduleSaveUiState', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
  });

  it('persists sidebarWidth, isConsoleOpen, consoleHeight from layout-store', async () => {
    scheduleSaveUiState();
    await vi.runAllTimersAsync();

    expect(saveUiState).toHaveBeenCalledWith(
      expect.objectContaining({
        sidebarWidth: 350,
        isConsoleOpen: true,
        consoleHeight: 400,
      }),
    );
  });
});
```

- [ ] **Step 2: Run test to confirm it fails**

```bash
yarn test ui-state
```

Expected: test fails because `saveUiState` is not called with those fields.

- [ ] **Step 3: Extend `UiState` interface in `src/lib/tauri-api.ts`**

Find the `UiState` interface (around line 904) and add three optional fields:

```ts
export interface UiState {
  activeMode: 'workspace' | 'collection';
  workspaceTabs?: UiStateWorkspaceTabs;
  layoutDirection?: 'stacked' | 'side-by-side';
  activeCollection?: string;
  collectionTabs?: UiStateCollectionTab[];
  sidebarWidth?: number;
  isConsoleOpen?: boolean;
  consoleHeight?: number;
}
```

- [ ] **Step 4: Extend `scheduleSaveUiState` in `src/lib/ui-state.ts`**

In `scheduleSaveUiState`, the `uiState` object is built inside the `setTimeout` callback. Add the three new fields:

```ts
const uiState: UiState = {
  activeMode: isWsMode ? 'workspace' : 'collection',
  layoutDirection: layoutState.requestLayout,
  sidebarWidth: layoutState.sidebarWidth,
  isConsoleOpen: layoutState.isConsoleOpen,
  consoleHeight: layoutState.consoleHeight,
};
```

- [ ] **Step 5: Run test to confirm it passes**

```bash
yarn test ui-state
```

Expected: test passes.

- [ ] **Step 6: Commit**

```bash
git add src/lib/tauri-api.ts src/lib/ui-state.ts src/lib/__tests__/ui-state.test.ts
git commit -m "feat: persist sidebarWidth, isConsoleOpen, consoleHeight in UiState"
```

---

### Task 3: Wire App.tsx to layout-store and restore state on startup

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/lib/ui-state.ts`

- [ ] **Step 1: Add layout-store subscription to `ui-state.ts`**

`scheduleSaveUiState` is currently triggered only by `usePaneStore.subscribe(...)`. Add a parallel subscription for `layout-store` changes.

At the bottom of `src/lib/ui-state.ts`, export a new setup function:

```ts
import { useLayoutStore } from '@/stores/layout-store';

export function subscribeLayoutStoreToUiState(): () => void {
  return useLayoutStore.subscribe(scheduleSaveUiState);
}
```

- [ ] **Step 2: Call `subscribeLayoutStoreToUiState` in App.tsx**

In `src/App.tsx`, import and call it alongside the existing pane-store subscription:

```ts
import { restoreUiState, scheduleSaveUiState, subscribeLayoutStoreToUiState } from '@/lib/ui-state';

// Inside App(), in the useEffect that sets up subscriptions:
useEffect(() => {
  const unsubPane = usePaneStore.subscribe(scheduleSaveUiState);
  const unsubLayout = subscribeLayoutStoreToUiState();
  return () => {
    unsubPane();
    unsubLayout();
  };
}, []);
```

- [ ] **Step 3: Remove local state from App.tsx and read from layout-store**

In `src/App.tsx`, remove these four lines:

```ts
const [sidebarWidth, setSidebarWidth] = useState(280);
const [sidebarCollapsed] = useState(false);   // dead state — delete entirely
const [isConsoleOpen, setIsConsoleOpen] = useState(false);
const [consoleHeight, setConsoleHeight] = useState(280);
```

Replace with:

```ts
const sidebarWidth = useLayoutStore((s) => s.sidebarWidth);
const setSidebarWidth = useLayoutStore((s) => s.setSidebarWidth);
const isConsoleOpen = useLayoutStore((s) => s.isConsoleOpen);
const setConsoleOpen = useLayoutStore((s) => s.setConsoleOpen);
const consoleHeight = useLayoutStore((s) => s.consoleHeight);
const setConsoleHeight = useLayoutStore((s) => s.setConsoleHeight);
```

Update all usages in JSX:
- `setIsConsoleOpen((o) => !o)` → `setConsoleOpen(!isConsoleOpen)`
- The `sidebarCollapsed` guard `{!sidebarCollapsed && (...)}` — remove the guard entirely, always render the sidebar block (it was never collapsible)

- [ ] **Step 4: Restore layout state on startup in App.tsx**

In the `init()` async function inside the startup `useEffect`, after `restoreUiState()` returns, add hydration for the three new fields:

```ts
const uiState = await restoreUiState();
if (uiState?.layoutDirection) {
  useLayoutStore.getState().setRequestLayout(uiState.layoutDirection);
}
if (uiState?.sidebarWidth) {
  useLayoutStore.getState().setSidebarWidth(uiState.sidebarWidth);
}
if (uiState?.isConsoleOpen !== undefined) {
  useLayoutStore.getState().setConsoleOpen(uiState.isConsoleOpen);
}
if (uiState?.consoleHeight) {
  useLayoutStore.getState().setConsoleHeight(uiState.consoleHeight);
}
```

- [ ] **Step 5: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Expected: no errors. In particular, confirm there are no remaining references to `sidebarCollapsed` or `setIsConsoleOpen`.

```bash
grep -rn "sidebarCollapsed\|setIsConsoleOpen" src/
```

Expected: no output.

- [ ] **Step 6: Run all tests**

```bash
yarn test
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/App.tsx src/lib/ui-state.ts
git commit -m "feat: App.tsx reads layout state from store, persists and restores on startup"
```
