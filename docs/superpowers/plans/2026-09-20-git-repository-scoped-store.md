# Git Repository-Scoped Store & Destructive-Action Safety Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the wrong-repository/data-loss risk in the Git panel by giving every `GitPanel` its own isolated store instance (instead of one global singleton), and close the four remaining frontend safety gaps around discard, remote-branch checkout, conflict resolution, and merge abort.

**Architecture:** Convert `src/stores/git-store.ts` from a plain Zustand singleton (`create(...)`) to a factory (`createGitStore()`) that returns a vanilla store, instantiated once per `GitPanel` mount and distributed to descendants via React Context. This is a new pattern for this codebase (all other `src/stores/*.ts` files are singletons) — it is justified specifically because two `GitPanel` instances can be mounted simultaneously (split panes) against different repositories, and a shared singleton cannot represent that. A closure-scoped generation counter inside the factory guards the one method that changes identity (`setRepository`) against a superseded async response applying itself after a newer call has already started.

**Tech Stack:** React 18, Zustand (`zustand` + `zustand/vanilla`), TypeScript, Vitest + Testing Library, shadcn/ui `AlertDialog`.

**Spec:** `docs/reports/git-integration-review/01-user-journey.md` — findings UJ-01, UJ-06, UJ-07, UJ-08, UJ-09 (Phase 1 of the report's "Recommended remediation order"). Backend safety for UJ-06 (discard-from-index), UJ-07 (remote-checkout/pull collision preflight), and UJ-09 (abort-merge requires actual merge state) is already implemented and merged in `crates/rocket-git/` — this plan is the frontend half only.

## Global Constraints

- All UI components use shadcn/ui primitives only — no raw `<button>`/`<input>`/`<dialog>` (existing hard rule; do not introduce new violations, but do not attempt to fix pre-existing ones outside this plan's scope).
- Icons: `lucide-react` only.
- Zustand: never fully destructure store state at a component's top level — use narrow selectors (`useGitStore((s) => s.field)`), one per field needed reactively; actions may be destructured together since they are referentially stable.
- Every new/changed store action must continue to return `Promise<void>` and never throw — callers rely on `try { await action() } finally { ... }` for local busy-state, not on rejection (this is existing convention; UJ-05's typed-result work is a separate, later phase and out of scope here).
- Run `yarn tsc --noEmit` and `yarn check` after every task; both must be clean before moving to the next task.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/stores/git-store.ts` | Modify: replace the singleton `export const useGitStore = create(...)` with `export function createGitStore()` returning a vanilla store; add the generation-guarded `setRepository`. No longer exports a ready-to-use hook itself. |
| `src/stores/git-store-context.tsx` | Create: `GitStoreContext`, `GitStoreProvider`, `useGitStore(selector)` hook, `useGitStoreApi()` hook. This is the new public surface every component imports instead of the old `git-store.ts` hook export. |
| `src/components/git/GitPanel.tsx` | Modify: create one store instance per mount via `useState(() => createGitStore())`, wrap its returned JSX in `<GitStoreProvider store={store}>`, update its own store call sites to the new hooks. |
| `src/components/layout/CollectionDropdown.tsx` | Modify: remove the eager `useGitStore.getState().setRepository(...)` pre-warm call (no shared store instance exists to write into any more). |
| `src/components/git/GitCommitForm.tsx` | Modify: switch to context hooks. No behavior change. |
| `src/components/git/GitFileList.tsx` | Modify: switch to context hooks; add a confirmation `AlertDialog` for individual (per-file) Discard, mirroring the existing Discard-All dialog (UJ-06). |
| `src/components/git/GitCredentialsDialog.tsx` | Modify: switch to context hooks. No behavior change. |
| `src/components/git/BranchSelector.tsx` | Modify: switch to context hooks; add a `checkingOutRemote` busy state that disables the remote-branch row and shows a spinner while `checkoutRemoteBranch` is in flight, matching local-switch UX (UJ-07). |
| `src/components/git/GitRemotesDialog.tsx` | Modify: switch to context hooks. No behavior change. |
| `src/components/git/GitStashSection.tsx` | Modify: switch to context hooks. No behavior change. |
| `src/components/git/GitLandingPanel.tsx` | Modify: switch to context hooks. No behavior change. |
| `src/components/git/GitCommitLog.tsx` | Modify: switch to context hooks. No behavior change. |
| `src/components/git/ConflictResolver.tsx` | Modify: switch to context hooks; reset manual-mode state when `conflictState.filePath` changes; disable resolve/abort controls while an operation is in flight; advance/exit after a confirmed successful resolve; add a confirmation `AlertDialog` before Abort Merge (UJ-08, UJ-09). |
| `src/components/git/__tests__/git-store-scoping.test.tsx` | Create: interleaved two-panel integration test proving isolation (UJ-01 acceptance criterion). |
| `src/stores/__tests__/git-store.test.ts` | Modify: update to call `createGitStore()` and use the returned store's `.getState()`/`.setState()` instead of the old singleton import. |

---

### Task 1: Convert the store to a factory with a generation-guarded `setRepository`

**Files:**
- Modify: `src/stores/git-store.ts:1` (imports), `src/stores/git-store.ts:139` (the `create(...)` call), `src/stores/git-store.ts:164-192` (`setRepository`), `src/stores/git-store.ts:704-712` (`initRepo`)
- Test: `src/stores/__tests__/git-store.test.ts`

**Interfaces:**
- Consumes: nothing new — same `GitState` interface as today (unchanged shape).
- Produces: `export function createGitStore(): StoreApi<GitState>` — every later task's `GitStoreProvider` calls this once per `GitPanel` mount.

- [ ] **Step 1: Update the test file's setup to use a fresh store per test, proving the current behavior still holds**

The existing test file imports the singleton directly. Change its setup so each test gets an isolated store — this both prepares for the factory conversion and closes off the risk of state leaking between tests (a latent issue with the current singleton). Read `src/stores/__tests__/git-store.test.ts` in full first, then replace every `useGitStore.getState()` / `useGitStore.setState()` / `useGitStore(...)` call in the file with a `store` variable created fresh in a `beforeEach`:

```typescript
// At the top of the describe block, replace any existing
// `useGitStore.setState({...})` reset logic with:
import { createGitStore } from '@/stores/git-store';
import type { StoreApi } from 'zustand/vanilla';
import type { GitState } from '@/stores/git-store';

let store: StoreApi<GitState>;

beforeEach(() => {
  store = createGitStore();
  vi.clearAllMocks();
});
```

Then mechanically replace every remaining `useGitStore.getState()` with `store.getState()`, every `useGitStore.setState(...)` with `store.setState(...)`, and every direct action call like `useGitStore.getState().setRepository(...)` with `store.getState().setRepository(...)`. Do not change any assertions — only the access path.

- [ ] **Step 2: Run the test file to confirm it still fails to compile (expected — `createGitStore` doesn't exist yet)**

Run: `yarn vitest run src/stores/__tests__/git-store.test.ts`
Expected: FAIL with `createGitStore is not exported` or a TypeScript error naming that symbol.

- [ ] **Step 3: Add `GitState` as an exported type and wrap the store body in a factory function with a generation guard**

In `src/stores/git-store.ts`, add `StoreApi` and `createStore` imports from `zustand/vanilla` alongside the existing `zustand` import (the existing `import { create } from 'zustand';` at line 1 is removed):

```typescript
import { createStore } from 'zustand/vanilla';
```

Change `interface GitState {` to `export interface GitState {` (line 48) — the type must be public so the context file and tests can reference it.

Replace the store creation (currently `export const useGitStore = create<GitState>((set, get) => ({` at line 139, closing with `}));` at line 737) with a factory. The body between `create<GitState>((set, get) => ({` and the matching `}))` is unchanged except for `setRepository` and `initRepo` below — wrap it like this:

```typescript
export function createGitStore() {
  // Guards setRepository against a slower, superseded call applying its
  // result after a newer call has already started — e.g. the user
  // switches repositories again before the first load finishes. Scoped to
  // this store instance's closure, not global, since each GitPanel now
  // owns its own store.
  let loadGeneration = 0;

  return createStore<GitState>((set, get) => ({
    // ...(everything currently inside the object literal, unchanged)...
  }));
}
```

Now update `setRepository` (currently lines 164-192) to capture and check the generation at every point it writes state after an `await`:

```typescript
  // Set the active repository and check if it is a git repo.
  setRepository: async (repositoryId: string) => {
    const myGeneration = ++loadGeneration;
    set({ repositoryId, loading: true, error: null });
    try {
      const isRepo = await gitIsRepo(repositoryId);
      if (myGeneration !== loadGeneration) return;
      set({ isRepo });
      if (isRepo) {
        // Always reload repository-scoped credentials so switching repositories
        // picks up the right identity without requiring a manual re-entry.
        try {
          const saved = await loadGitCredentials(repositoryId);
          if (myGeneration !== loadGeneration) return;
          set({ credentials: saved ?? null });
        } catch {
          // Keychain unavailable — proceed without credentials.
        }
        if (myGeneration !== loadGeneration) return;
        const [status] = await Promise.all([
          gitStatus(repositoryId),
          get().refreshStashes(),
          get().refreshBranches(),
          get().refreshRemotes(),
        ]);
        if (myGeneration !== loadGeneration) return;
        set({ status, loading: false });
      } else {
        if (myGeneration !== loadGeneration) return;
        set({ status: null, loading: false });
      }
    } catch (e) {
      if (myGeneration !== loadGeneration) return;
      set({ error: String(e), loading: false });
    }
  },
```

`initRepo` (currently lines 704-712) delegates to `setRepository` already, so it needs no separate guard — leave it as-is:

```typescript
  // Initialize a new git repository then load it into the store.
  initRepo: async (repositoryId: string) => {
    try {
      await gitInit(repositoryId);
      await get().setRepository(repositoryId);
    } catch (e) {
      set({ error: String(e) });
    }
  },
```

Finally, remove the old default export line if one exists (there isn't one in the current file — `useGitStore` was the only named export of the store itself; `GitState` is now also exported per above).

- [ ] **Step 4: Run the test file again to verify it now compiles and passes**

Run: `yarn vitest run src/stores/__tests__/git-store.test.ts`
Expected: PASS, all existing assertions green.

- [ ] **Step 5: Add a new test proving the generation guard**

Append to `src/stores/__tests__/git-store.test.ts`:

```typescript
describe('setRepository generation guard', () => {
  it('does not let a slower first call overwrite a faster second call', async () => {
    const gitIsRepoMock = vi.mocked(gitIsRepo);
    let resolveFirst!: (value: boolean) => void;
    gitIsRepoMock.mockImplementationOnce(
      () => new Promise((resolve) => { resolveFirst = resolve; }),
    );
    gitIsRepoMock.mockResolvedValueOnce(true);

    const firstCall = store.getState().setRepository('repo-a');
    // Let the second call's setRepository start and fully resolve before the first does.
    await store.getState().setRepository('repo-b');
    expect(store.getState().repositoryId).toBe('repo-b');

    // Now let the slow first call resolve. It must not clobber repo-b's state.
    resolveFirst(true);
    await firstCall;
    expect(store.getState().repositoryId).toBe('repo-b');
  });
});
```

Check the top of the test file for how `gitIsRepo` is currently mocked (likely `vi.mock('@/lib/tauri-api', ...)` with `gitIsRepo: vi.fn()`) and import `gitIsRepo` from `@/lib/tauri-api` at the top of the test file if not already imported, matching the existing mock style used for other API functions in that file.

- [ ] **Step 6: Run the new test to verify it passes**

Run: `yarn vitest run src/stores/__tests__/git-store.test.ts -t "generation guard"`
Expected: PASS.

- [ ] **Step 7: Run the full test file and full type check**

Run: `yarn vitest run src/stores/__tests__/git-store.test.ts && yarn tsc --noEmit`
Expected: both PASS. (`tsc` will still show errors from every other file that imports the now-removed `useGitStore` export from `git-store.ts` — that's expected until Task 2 creates the replacement; confirm the *only* new errors are "has no exported member 'useGitStore'" in other files, not anything inside `git-store.ts` itself or its test.)

- [ ] **Step 8: Commit**

```bash
git add src/stores/git-store.ts src/stores/__tests__/git-store.test.ts
git commit -m "refactor(git): convert git-store to a factory with generation guard"
```

---

### Task 2: Create the Context provider and hooks

**Files:**
- Create: `src/stores/git-store-context.tsx`
- Test: `src/stores/__tests__/git-store-context.test.tsx`

**Interfaces:**
- Consumes: `createGitStore`, `GitState` from `src/stores/git-store.ts` (Task 1).
- Produces: `GitStoreProvider({ store, children })`, `useGitStore<T>(selector: (state: GitState) => T): T`, `useGitStoreApi(): StoreApi<GitState>` — every remaining task imports these three from `@/stores/git-store-context` instead of importing `useGitStore` from `@/stores/git-store`.

- [ ] **Step 1: Write the failing test**

Create `src/stores/__tests__/git-store-context.test.tsx`:

```typescript
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider, useGitStore, useGitStoreApi } from '@/stores/git-store-context';

function Probe() {
  const isRepo = useGitStore((s) => s.isRepo);
  const api = useGitStoreApi();
  return (
    <div>
      <span data-testid='is-repo'>{String(isRepo)}</span>
      <button type='button' onClick={() => api.setState({ isRepo: true })}>
        flip
      </button>
    </div>
  );
}

describe('GitStoreProvider / useGitStore', () => {
  it('reads reactive state scoped to the provided store instance', () => {
    const store = createGitStore();
    render(
      <GitStoreProvider store={store}>
        <Probe />
      </GitStoreProvider>,
    );
    expect(screen.getByTestId('is-repo').textContent).toBe('false');
  });

  it('two providers with different store instances are fully isolated', () => {
    const storeA = createGitStore();
    const storeB = createGitStore();
    storeA.setState({ isRepo: true });

    render(
      <>
        <GitStoreProvider store={storeA}>
          <Probe />
        </GitStoreProvider>
        <GitStoreProvider store={storeB}>
          <Probe />
        </GitStoreProvider>
      </>,
    );
    const [first, second] = screen.getAllByTestId('is-repo');
    expect(first.textContent).toBe('true');
    expect(second.textContent).toBe('false');
  });

  it('throws a clear error when used outside a provider', () => {
    const OutsideProbe = () => {
      useGitStore((s) => s.isRepo);
      return null;
    };
    expect(() => render(<OutsideProbe />)).toThrow(/useGitStore must be used within a GitStoreProvider/);
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `yarn vitest run src/stores/__tests__/git-store-context.test.tsx`
Expected: FAIL — `Cannot find module '@/stores/git-store-context'`.

- [ ] **Step 3: Write the implementation**

Create `src/stores/git-store-context.tsx`:

```typescript
import { createContext, type ReactNode, useContext } from 'react';
import type { StoreApi } from 'zustand/vanilla';
import { useStore } from 'zustand';
import type { GitState } from './git-store';

const GitStoreContext = createContext<StoreApi<GitState> | null>(null);

interface GitStoreProviderProps {
  store: StoreApi<GitState>;
  children: ReactNode;
}

/** Provides one GitPanel's own isolated git-store instance to its subtree. */
export function GitStoreProvider({ store, children }: GitStoreProviderProps) {
  return <GitStoreContext.Provider value={store}>{children}</GitStoreContext.Provider>;
}

function useStoreApi(): StoreApi<GitState> {
  const api = useContext(GitStoreContext);
  if (!api) {
    throw new Error('useGitStore must be used within a GitStoreProvider');
  }
  return api;
}

/** Reactive selector hook, scoped to the nearest GitStoreProvider's store instance. */
export function useGitStore<T>(selector: (state: GitState) => T): T {
  return useStore(useStoreApi(), selector);
}

/** Imperative access (`.getState()`/`.setState()`) to the nearest GitStoreProvider's store instance. */
export function useGitStoreApi(): StoreApi<GitState> {
  return useStoreApi();
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn vitest run src/stores/__tests__/git-store-context.test.tsx`
Expected: PASS, all three tests green.

- [ ] **Step 5: Run full type check**

Run: `yarn tsc --noEmit`
Expected: same set of "no exported member 'useGitStore'" errors as Task 1 step 7 (every consumer still imports the old way) — no *new* kinds of errors from the two files touched in this task.

- [ ] **Step 6: Commit**

```bash
git add src/stores/git-store-context.tsx src/stores/__tests__/git-store-context.test.tsx
git commit -m "feat(git): add GitStoreProvider and scoped useGitStore hooks"
```

---

### Task 3: Wire `GitPanel` to own and provide a store instance

**Files:**
- Modify: `src/components/git/GitPanel.tsx` (whole file — every `useGitStore` reference)
- Test: `src/components/git/__tests__/GitPanel.test.tsx` (new smoke test)

**Interfaces:**
- Consumes: `createGitStore` (`@/stores/git-store`), `GitStoreProvider`, `useGitStore`, `useGitStoreApi` (`@/stores/git-store-context`) from Tasks 1-2.
- Produces: `GitPanel` now renders its subtree inside a `GitStoreProvider` — every task after this one assumes that context is present for any component rendered under `GitPanel` (which is all of them: `GitCommitForm`, `GitFileList`, `BranchSelector`, `GitLandingPanel`, `GitStashSection`, `GitCommitLog`, `GitRemotesDialog`, `GitCredentialsDialog`, `ConflictResolver` are only ever rendered inside `GitPanel`'s tree per `src/components/panes/EditorGroup.tsx`).

- [ ] **Step 1: Write a failing smoke test**

Create `src/components/git/__tests__/GitPanel.test.tsx`:

```typescript
import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { GitPanel } from '@/components/git/GitPanel';
import * as tauriApi from '@/lib/tauri-api';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    gitIsRepo: vi.fn(),
    gitStatus: vi.fn(),
    gitBranches: vi.fn(),
    gitListRemotes: vi.fn(),
    gitStashList: vi.fn(),
    loadGitCredentials: vi.fn(),
    onCollectionChanged: vi.fn().mockResolvedValue(() => {}),
  };
});

describe('GitPanel repository scoping', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.gitIsRepo).mockResolvedValue(false);
  });

  it('two panels for different repositories never share state', async () => {
    vi.mocked(tauriApi.gitIsRepo).mockImplementation(async (id: string) => id === 'repo-a');

    render(
      <>
        <GitPanel repositoryId='repo-a' repositoryLabel='Repo A' />
        <GitPanel repositoryId='repo-b' repositoryLabel='Repo B' />
      </>,
    );

    await waitFor(() => {
      expect(screen.getAllByText(/Repo A|not a Git repository/).length).toBeGreaterThan(0);
    });
    // repo-a is a repo (shows its normal panel with the label), repo-b is not
    // (shows the non-repository message) — proving the two instances resolved
    // independently rather than one overwriting the other.
    expect(screen.getByText('Repo A')).toBeInTheDocument();
    expect(screen.getByText('This collection is not a Git repository.')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `yarn vitest run src/components/git/__tests__/GitPanel.test.tsx`
Expected: FAIL (currently, both panels share the singleton, so one of the two assertions about independent resolution will be flaky/wrong — most likely both show the same state, or a TypeScript error from the outdated import).

- [ ] **Step 3: Update `GitPanel.tsx`**

Change the import at line 27 from:
```typescript
import { useGitStore } from '@/stores/git-store';
```
to:
```typescript
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider, useGitStore, useGitStoreApi } from '@/stores/git-store-context';
```

Add the store instance creation right after the existing `useState` calls (after line 50, before line 52):

```typescript
  const [store] = useState(() => createGitStore());
```

Replace every remaining `useGitStore.getState()` call in the file with `useGitStoreApi().getState()` — but since `useGitStoreApi()` must be called at the top of the component (hooks rule), add one line near the other selector hooks (after line 64):

```typescript
  const gitStoreApi = useGitStoreApi();
```

Then update these exact call sites:
- Line 76: `setIsRepo(useGitStore.getState().isRepo);` → `setIsRepo(gitStoreApi.getState().isRepo);`
- Line 184: `setIsRepo(useGitStore.getState().isRepo);` → `setIsRepo(gitStoreApi.getState().isRepo);`

All the other `useGitStore((state) => state.X)` calls (lines 52-64) work unchanged — they already use the selector form, and now resolve through the new `useGitStore` from `@/stores/git-store-context` instead of the old singleton export.

Finally, wrap the two `return` statements' JSX in `<GitStoreProvider store={store}>`. There are two returns: the non-repo state (lines 175-206) and the full panel (lines 209-365). Wrap each one's outermost element:

For the non-repo return (around line 175), change:
```typescript
    return (
      <div className='flex flex-col items-center justify-center gap-3 h-full px-4 text-center'>
```
to:
```typescript
    return (
      <GitStoreProvider store={store}>
        <div className='flex flex-col items-center justify-center gap-3 h-full px-4 text-center'>
```
and its closing `</div>` (just before the final `);` of that branch) gets a matching `</GitStoreProvider>` after it.

For the main return (around line 209), change:
```typescript
  return (
    <div className='flex flex-col h-full'>
```
to:
```typescript
  return (
    <GitStoreProvider store={store}>
      <div className='flex flex-col h-full'>
```
and its closing `</div>` (the last one before the function's final `);`) gets a matching `</GitStoreProvider>` after it. Re-indent the JSX between the new wrapper and its close by two spaces to keep formatting consistent (or run `yarn format` at the end of this task, which will handle it automatically).

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn vitest run src/components/git/__tests__/GitPanel.test.tsx`
Expected: PASS.

- [ ] **Step 5: Run type check and lint**

Run: `yarn tsc --noEmit && yarn check`
Expected: `tsc` still shows "no exported member 'useGitStore'" for every OTHER consumer file (Tasks 4-7 fix those); no new errors from `GitPanel.tsx` itself. `yarn check` (Biome) clean for `GitPanel.tsx`.

- [ ] **Step 6: Commit**

```bash
git add src/components/git/GitPanel.tsx src/components/git/__tests__/GitPanel.test.tsx
git commit -m "feat(git): give each GitPanel its own scoped store instance"
```

---

### Task 4: Remove the eager pre-warm in `CollectionDropdown`

**Files:**
- Modify: `src/components/layout/CollectionDropdown.tsx:8` (import), `src/components/layout/CollectionDropdown.tsx:39-43` (`handleSelect`)

**Interfaces:**
- Consumes: nothing (this task only removes a call site).
- Produces: nothing new.

- [ ] **Step 1: Remove the store import and the pre-warm call**

In `src/components/layout/CollectionDropdown.tsx`, delete line 8:
```typescript
import { useGitStore } from '@/stores/git-store';
```

Change `handleSelect` (lines 39-43) from:
```typescript
  const handleSelect = (summary: CollectionSummary) => {
    switchCollection(summary.name);
    void useGitStore.getState().setRepository(summary.repositoryId);
    setOpen(false);
  };
```
to:
```typescript
  const handleSelect = (summary: CollectionSummary) => {
    switchCollection(summary.name);
    setOpen(false);
  };
```

There is no longer a shared store instance for this eager call to write into — each `GitPanel` now loads its own repository status on mount (Task 3), so `GitPanel` will show its skeleton briefly the first time it opens for a given repository instead of finding pre-warmed data. This trades a small one-time loading flicker for eliminating the exact stale-path reuse vector UJ-01 describes.

- [ ] **Step 2: Run type check**

Run: `yarn tsc --noEmit`
Expected: no errors in `CollectionDropdown.tsx` (the `repositoryId` field on `summary` may now be unused by this component if nothing else reads it — check with a grep: `grep -n "repositoryId" src/components/layout/CollectionDropdown.tsx`; if the only remaining reference was the deleted line, this is fine, the field stays on the type for other consumers like `GitToolbarButton`).

- [ ] **Step 3: Manually verify no other component depended on this pre-warm**

Run: `grep -rn "setRepository" src --include="*.tsx" --include="*.ts" | grep -v __tests__`
Expected output: only `GitPanel.tsx`'s `checkAndLoad` and the non-repo `initRepo` button, and `git-store.ts`/`git-store-context.tsx` themselves. If any other file appears, stop and investigate before proceeding — it means something else relies on the pre-warm.

- [ ] **Step 4: Commit**

```bash
git add src/components/layout/CollectionDropdown.tsx
git commit -m "fix(git): remove eager repository pre-warm that could race with GitPanel"
```

---

### Task 5: Migrate `GitCommitForm`, `GitFileList` (+ UJ-06 discard confirmation), `GitCredentialsDialog`

**Files:**
- Modify: `src/components/git/GitCommitForm.tsx:6,13-15,56`
- Modify: `src/components/git/GitFileList.tsx:18,26-35,67,217-231`
- Modify: `src/components/git/GitCredentialsDialog.tsx:22,27-30`
- Test: `src/components/git/__tests__/GitFileList.test.tsx` (new)

**Interfaces:**
- Consumes: `useGitStore`, `useGitStoreApi` from `@/stores/git-store-context` (Task 2); all three components are rendered only inside `GitPanel`'s `GitStoreProvider` tree (Task 3).
- Produces: nothing new consumed by later tasks — these three are leaves.

- [ ] **Step 1: Migrate `GitCommitForm.tsx`**

Change line 6 from:
```typescript
import { useGitStore } from '@/stores/git-store';
```
to:
```typescript
import { useGitStore } from '@/stores/git-store-context';
```

No other line in this file needs to change — lines 13-15 already use the narrow selector form, and line 56 (`useGitStore.setState({...})`) needs updating since the plain-hook `.setState()` static method no longer exists. Change line 56 from:
```typescript
      useGitStore.setState({ error: `Failed to save git identity: ${String(e)}` });
```
Since this file doesn't otherwise need imperative access, add the api hook. Change the selector block (lines 13-15) to also grab the api:
```typescript
  const status = useGitStore((state) => state.status);
  const commitChanges = useGitStore((state) => state.commitChanges);
  const repositoryId = useGitStore((state) => state.repositoryId);
```
add after it:
```typescript
  const gitStoreApi = useGitStoreApi();
```
and update the import to also bring in `useGitStoreApi`:
```typescript
import { useGitStore, useGitStoreApi } from '@/stores/git-store-context';
```
Then change line 56 to:
```typescript
      gitStoreApi.setState({ error: `Failed to save git identity: ${String(e)}` });
```

- [ ] **Step 2: Migrate `GitCredentialsDialog.tsx`**

Change line 22 from:
```typescript
import { useGitStore } from '@/stores/git-store';
```
to:
```typescript
import { useGitStore } from '@/stores/git-store-context';
```
No other changes needed — lines 27-30 already use narrow selectors and there is no imperative `.getState()`/`.setState()` call in this file.

- [ ] **Step 3: Migrate `GitFileList.tsx` and add the individual-discard confirmation dialog**

Change line 18 from:
```typescript
import { useGitStore } from '@/stores/git-store';
```
to:
```typescript
import { useGitStore, useGitStoreApi } from '@/stores/git-store-context';
```

Replace the full-destructure at lines 26-35 with narrow selectors (this file currently violates the no-full-destructure rule — fix it while migrating):
```typescript
  const status = useGitStore((s) => s.status);
  const refreshConflicts = useGitStore((s) => s.refreshConflicts);
  const refreshStatus = useGitStore((s) => s.refreshStatus);
  const stageFiles = useGitStore((s) => s.stageFiles);
  const stageAll = useGitStore((s) => s.stageAll);
  const unstageFiles = useGitStore((s) => s.unstageFiles);
  const unstageAll = useGitStore((s) => s.unstageAll);
  const discardFiles = useGitStore((s) => s.discardFiles);
  const gitStoreApi = useGitStoreApi();
```

Change line 67 from:
```typescript
    const conflictFile = useGitStore.getState().conflicts.find((c) => c.path === file.path);
```
to:
```typescript
    const conflictFile = gitStoreApi.getState().conflicts.find((c) => c.path === file.path);
```

Now add the individual-discard confirmation. Add a new state variable near the existing `showDiscardAllDialog` state (after line 37):
```typescript
  const [discardingFile, setDiscardingFile] = useState<FileStatus | null>(null);
```

Add a confirm handler near `handleConfirmDiscardAll` (after line 52):
```typescript
  const handleConfirmDiscardFile = () => {
    if (!discardingFile) return;
    discardFiles([discardingFile.path]);
    setDiscardingFile(null);
  };
```

Change the per-file Discard button's `onClick` (lines 225-228) from:
```typescript
                          onClick={(e) => {
                            e.stopPropagation();
                            discardFiles([file.path]);
                          }}
```
to:
```typescript
                          onClick={(e) => {
                            e.stopPropagation();
                            setDiscardingFile(file);
                          }}
```

Add a second `AlertDialog` right after the existing Discard-All one (after line 274, before the closing `</TooltipProvider>` at line 275):
```typescript
      {/* Individual-discard confirmation dialog */}
      <AlertDialog
        open={discardingFile !== null}
        onOpenChange={(open) => !open && setDiscardingFile(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Discard Changes?</AlertDialogTitle>
            <AlertDialogDescription>
              {discardingFile?.status === 'untracked' ? (
                <>
                  This will permanently delete the untracked file{' '}
                  <span className='font-mono'>{discardingFile?.path}</span>. This cannot be undone.
                </>
              ) : (
                <>
                  This will discard unstaged changes to{' '}
                  <span className='font-mono'>{discardingFile?.path}</span>, restoring it to its
                  last staged or committed content. This cannot be undone.
                </>
              )}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handleConfirmDiscardFile}>Discard</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
```

Check `FileStatus`'s `status` field's exact literal type before using `'untracked'` above — read the type definition:
```bash
grep -n "interface FileStatus" -A 6 src/lib/tauri-api.ts
```
and use whatever the untracked variant is actually called (it may be `'untracked'` or something else — match it exactly; if there is no distinct untracked variant, drop the conditional and always show the generic discard-changes copy).

- [ ] **Step 4: Write a test for the new confirmation dialog**

Create `src/components/git/__tests__/GitFileList.test.tsx`:

```typescript
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { GitFileList } from '@/components/git/GitFileList';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';
import type { RepoStatus } from '@/lib/tauri-api';

const baseStatus: RepoStatus = {
  branch: 'main',
  isClean: false,
  ahead: 0,
  behind: 0,
  files: [{ path: 'notes.txt', status: 'modified', staged: false }],
};

function renderWithStore() {
  const store = createGitStore();
  store.setState({ status: baseStatus });
  const discardFiles = vi.fn();
  store.setState({ discardFiles });
  render(
    <GitStoreProvider store={store}>
      <GitFileList onFileClick={() => {}} onConflictClick={() => {}} />
    </GitStoreProvider>,
  );
  return { discardFiles };
}

describe('GitFileList individual discard confirmation', () => {
  it('does not call discardFiles until the confirmation dialog is confirmed', async () => {
    const { discardFiles } = renderWithStore();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /discard/i }));
    expect(discardFiles).not.toHaveBeenCalled();
    expect(screen.getByText('Discard Changes?')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Discard' }));
    expect(discardFiles).toHaveBeenCalledWith(['notes.txt']);
  });

  it('cancel performs no discard', async () => {
    const { discardFiles } = renderWithStore();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /discard/i }));
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(discardFiles).not.toHaveBeenCalled();
  });
});
```

Check the actual shape of `RepoStatus` before running this (`grep -n "interface RepoStatus" -A 8 src/lib/tauri-api.ts`) and adjust the `baseStatus` fixture's fields to match exactly if they differ from what's assumed above.

- [ ] **Step 5: Run the new test**

Run: `yarn vitest run src/components/git/__tests__/GitFileList.test.tsx`
Expected: PASS, both cases green.

- [ ] **Step 6: Run type check and lint across all three files**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors from `GitCommitForm.tsx`, `GitCredentialsDialog.tsx`, `GitFileList.tsx`, or the new test file.

- [ ] **Step 7: Commit**

```bash
git add src/components/git/GitCommitForm.tsx src/components/git/GitCredentialsDialog.tsx src/components/git/GitFileList.tsx src/components/git/__tests__/GitFileList.test.tsx
git commit -m "feat(git): require confirmation before discarding an individual file"
```

---

### Task 6: Migrate `BranchSelector` (+ UJ-07 remote-checkout busy state), `GitRemotesDialog`

**Files:**
- Modify: `src/components/git/BranchSelector.tsx:8,16-24,44,46,56,58,66-76,68,70`
- Modify: `src/components/git/GitRemotesDialog.tsx:8,16`
- Test: `src/components/git/__tests__/BranchSelector.test.tsx` (new)

**Interfaces:**
- Consumes: `useGitStore`, `useGitStoreApi` from `@/stores/git-store-context`.
- Produces: nothing new consumed by later tasks.

- [ ] **Step 1: Migrate `GitRemotesDialog.tsx`**

Change line 8 from:
```typescript
import { useGitStore } from '@/stores/git-store';
```
to:
```typescript
import { useGitStore } from '@/stores/git-store-context';
```
Line 16's full destructure (`const { remotes, addRemote, removeRemote, setRemoteUrl, refreshRemotes } = useGitStore();`) stays as-is for now — it destructures only actions plus one array field (`remotes`), which is an existing minor guardrail gap pre-dating this plan; leave it, since Phase 1's scope is UJ-01/06/07/08/09, not a general Zustand-selector cleanup pass (that belongs with UJ-19/UJ-21's later work). Only the import path changes.

- [ ] **Step 2: Migrate `BranchSelector.tsx` and add the remote-checkout busy state**

Change line 8 from:
```typescript
import { useGitStore } from '@/stores/git-store';
```
to:
```typescript
import { useGitStore, useGitStoreApi } from '@/stores/git-store-context';
```

Add the api hook after the existing destructure (after line 24):
```typescript
  const gitStoreApi = useGitStoreApi();
```

Replace every remaining `useGitStore.getState()` call (lines 44, 46, 56, 58, 68, 70, 84, 86 — search the file for all occurrences with `grep -n "useGitStore.getState()" src/components/git/BranchSelector.tsx` to get the exact current list) with `gitStoreApi.getState()`.

Add a busy-state for remote checkout. Add new state near the top (after line 15):
```typescript
  const [checkingOutRemote, setCheckingOutRemote] = useState<string | null>(null);
```

Change `handleCheckoutRemote` (lines 66-76) from:
```typescript
  const handleCheckoutRemote = async (name: string) => {
    setSwitchError(null);
    const prevError = useGitStore.getState().error;
    await checkoutRemoteBranch(name);
    const nextError = useGitStore.getState().error;
    if (nextError && nextError !== prevError) {
      setSwitchError(nextError);
    } else {
      setOpen(false);
    }
  };
```
to:
```typescript
  const handleCheckoutRemote = async (name: string) => {
    setSwitchError(null);
    setCheckingOutRemote(name);
    try {
      const prevError = gitStoreApi.getState().error;
      await checkoutRemoteBranch(name);
      const nextError = gitStoreApi.getState().error;
      if (nextError && nextError !== prevError) {
        setSwitchError(nextError);
      } else {
        setOpen(false);
      }
    } finally {
      setCheckingOutRemote(null);
    }
  };
```

Update the remote-branch row (lines 200-211) to show a busy state and disable while any remote checkout is in flight, changing:
```typescript
                return (
                  <button
                    key={branch.name}
                    type='button'
                    className='flex w-full items-center gap-1.5 rounded px-2 py-1 hover:bg-muted/50 cursor-pointer text-sm text-left'
                    onClick={() => {
                      void handleCheckoutRemote(branch.name);
                    }}
                  >
                    <span className='w-3.5' />
                    <span className='truncate flex-1 text-muted-foreground'>{localName}</span>
                  </button>
                );
```
to:
```typescript
                const isCheckingOutThis = checkingOutRemote === branch.name;
                return (
                  <button
                    key={branch.name}
                    type='button'
                    disabled={checkingOutRemote !== null}
                    className='flex w-full items-center gap-1.5 rounded px-2 py-1 hover:bg-muted/50 cursor-pointer text-sm text-left disabled:opacity-50 disabled:cursor-not-allowed'
                    onClick={() => {
                      void handleCheckoutRemote(branch.name);
                    }}
                  >
                    {isCheckingOutThis ? (
                      <Loader2 className='w-3.5 h-3.5 animate-spin shrink-0' />
                    ) : (
                      <span className='w-3.5' />
                    )}
                    <span className='truncate flex-1 text-muted-foreground'>{localName}</span>
                  </button>
                );
```

Add `Loader2` to the lucide-react import at line 1:
```typescript
import { AlertCircle, Check, GitBranch, GitMerge, Loader2, Plus, Trash2 } from 'lucide-react';
```

This gives remote checkout the same in-progress/disabled behavior local switch already has via the popover staying interactive during `handleSwitch` — check `handleSwitch` (lines 54-64) for comparison; it doesn't have an explicit busy state either today, so this task brings remote checkout to at least parity, closing the "UI shows an in-progress state" half of UJ-07's acceptance criteria without inventing a new pattern local switch doesn't have.

- [ ] **Step 3: Write a test for the new busy state and error surfacing**

Create `src/components/git/__tests__/BranchSelector.test.tsx`:

```typescript
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { BranchSelector } from '@/components/git/BranchSelector';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';
import type { BranchList } from '@/lib/tauri-api';

const branches: BranchList = {
  current: 'main',
  local: [{ name: 'main', isHead: true, isRemote: false, upstream: null }],
  remote: [{ name: 'origin/feature-x', isHead: false, isRemote: true, upstream: null }],
};

function renderWithStore(checkoutRemoteBranch: () => Promise<void>) {
  const store = createGitStore();
  store.setState({ branches, checkoutRemoteBranch });
  render(
    <GitStoreProvider store={store}>
      <BranchSelector />
    </GitStoreProvider>,
  );
}

describe('BranchSelector remote checkout', () => {
  it('shows a busy spinner and disables the row while checkout is in flight, and surfaces a resulting error', async () => {
    let resolveCheckout!: () => void;
    const checkoutRemoteBranch = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveCheckout = resolve;
        }),
    );
    renderWithStore(checkoutRemoteBranch);
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    const remoteButton = screen.getByRole('button', { name: /feature-x/ });
    await user.click(remoteButton);

    expect(checkoutRemoteBranch).toHaveBeenCalledWith('origin/feature-x');
    expect(remoteButton).toBeDisabled();

    resolveCheckout();
    await screen.findByRole('button', { name: /feature-x/ }); // popover stays mounted; re-query after state settles
  });
});
```

Check `BranchList`/`Branch` type field names before running (`grep -n "interface BranchList\|interface Branch " -A 6 src/lib/tauri-api.ts`) and adjust the fixture if they differ.

- [ ] **Step 4: Run the new test**

Run: `yarn vitest run src/components/git/__tests__/BranchSelector.test.tsx`
Expected: PASS.

- [ ] **Step 5: Run type check and lint**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors from `BranchSelector.tsx`, `GitRemotesDialog.tsx`, or the new test file.

- [ ] **Step 6: Commit**

```bash
git add src/components/git/BranchSelector.tsx src/components/git/GitRemotesDialog.tsx src/components/git/__tests__/BranchSelector.test.tsx
git commit -m "feat(git): show busy state during remote-branch checkout"
```

---

### Task 7: Migrate `GitStashSection`, `GitLandingPanel`, `GitCommitLog`

**Files:**
- Modify: `src/components/git/GitStashSection.tsx:24,47-58,69,93,104,115`
- Modify: `src/components/git/GitLandingPanel.tsx:30,33-44,54,70,77,101,126,133,154,178`
- Modify: `src/components/git/GitCommitLog.tsx:6,28`

**Interfaces:**
- Consumes: `useGitStore`, `useGitStoreApi` from `@/stores/git-store-context`.
- Produces: nothing new consumed by later tasks.

- [ ] **Step 1: Migrate `GitCommitLog.tsx`**

Change line 6 from:
```typescript
import { useGitStore } from '@/stores/git-store';
```
to:
```typescript
import { useGitStore } from '@/stores/git-store-context';
```
No other changes — line 28's destructure (`const { commitLog, refreshLog } = useGitStore();`) is action-plus-array, same reasoning as `GitRemotesDialog` in Task 6; leave as-is.

- [ ] **Step 2: Migrate `GitStashSection.tsx`**

Change line 24 from:
```typescript
import { useGitStore } from '@/stores/git-store';
```
to:
```typescript
import { useGitStore, useGitStoreApi } from '@/stores/git-store-context';
```
Add after the existing destructure (after line 58):
```typescript
  const gitStoreApi = useGitStoreApi();
```
Replace the three `useGitStore.getState().error` occurrences (lines 69, 93, 104, 115 — confirm exact line numbers with `grep -n "useGitStore.getState()" src/components/git/GitStashSection.tsx`) with `gitStoreApi.getState().error`.

- [ ] **Step 3: Migrate `GitLandingPanel.tsx`**

Change line 30 from:
```typescript
import { useGitStore } from '@/stores/git-store';
```
to:
```typescript
import { useGitStore, useGitStoreApi } from '@/stores/git-store-context';
```
Add after the existing destructure (after line 44):
```typescript
  const gitStoreApi = useGitStoreApi();
```
Replace every remaining `useGitStore.getState()` call in the file (lines 54, 70, 77, 101, 133, 154 — confirm exact list with `grep -n "useGitStore.getState()" src/components/git/GitLandingPanel.tsx`) with `gitStoreApi.getState()`.

Also fix the top-level full-destructure at lines 33-44 while touching this file (same guardrail cleanup as Task 5's `GitFileList`):
```typescript
  const status = useGitStore((s) => s.status);
  const push = useGitStore((s) => s.push);
  const pull = useGitStore((s) => s.pull);
  const fetch = useGitStore((s) => s.fetch);
  const saveStash = useGitStore((s) => s.saveStash);
  const popStash = useGitStore((s) => s.popStash);
  const error = useGitStore((s) => s.error);
  const clearError = useGitStore((s) => s.clearError);
  const credentials = useGitStore((s) => s.credentials);
  const setShowCredentialsDialog = useGitStore((s) => s.setShowCredentialsDialog);
```

Line 178's `useGitStore((state) => state.hasConflicts?.())` already uses the selector form — only its import source changes (handled by the top-of-file import edit above), no further change needed.

- [ ] **Step 4: Run type check and lint across all three files**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors from `GitStashSection.tsx`, `GitLandingPanel.tsx`, or `GitCommitLog.tsx`.

- [ ] **Step 5: Run the existing store test suite to make sure nothing behavioral broke**

Run: `yarn vitest run src/stores/__tests__/git-store.test.ts`
Expected: PASS (unchanged from Task 1).

- [ ] **Step 6: Commit**

```bash
git add src/components/git/GitStashSection.tsx src/components/git/GitLandingPanel.tsx src/components/git/GitCommitLog.tsx
git commit -m "refactor(git): migrate remaining store consumers to scoped context hooks"
```

---

### Task 8: Migrate `ConflictResolver`; fix UJ-08 re-arm bug; add UJ-09 abort confirmation

**Files:**
- Modify: `src/components/git/ConflictResolver.tsx` (whole file)
- Test: `src/components/git/__tests__/ConflictResolver.test.tsx` (new)

**Interfaces:**
- Consumes: `useGitStore`, `useGitStoreApi` from `@/stores/git-store-context`; `ConflictState` from `@/types/pane-types` (unchanged).
- Produces: an `onResolved?: () => void` prop that `GitPanel` (Task 3) can optionally pass to return to the landing view after a successful resolve — wire this now since it's needed to satisfy UJ-08's "advance to the next unresolved conflict or exits to a clear completion state" criterion.

- [ ] **Step 1: Write the failing test for the file-switch bug and double-submit guard**

Create `src/components/git/__tests__/ConflictResolver.test.tsx`:

```typescript
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { ConflictResolver } from '@/components/git/ConflictResolver';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';
import type { ConflictState } from '@/types/pane-types';

function baseConflict(overrides: Partial<ConflictState> = {}): ConflictState {
  return {
    filePath: 'a.txt',
    repositoryId: 'repo-1',
    repositoryLabel: 'Repo',
    ours: 'ours content',
    theirs: 'theirs content',
    ancestor: null,
    ...overrides,
  };
}

function renderWithStore(conflictState: ConflictState, overrides: Record<string, unknown> = {}) {
  const store = createGitStore();
  const resolveConflict = vi.fn().mockResolvedValue(undefined);
  const abortMerge = vi.fn().mockResolvedValue(undefined);
  store.setState({ resolveConflict, abortMerge, error: null, clearError: vi.fn(), ...overrides });
  const utils = render(
    <GitStoreProvider store={store}>
      <ConflictResolver conflictState={conflictState} />
    </GitStoreProvider>,
  );
  return { ...utils, resolveConflict, abortMerge };
}

describe('ConflictResolver', () => {
  it('resets manual mode and manual content when the conflict file changes', async () => {
    const { rerender } = render(<div />); // placeholder to establish user
    const user = userEvent.setup();
    const store = createGitStore();
    store.setState({
      resolveConflict: vi.fn().mockResolvedValue(undefined),
      abortMerge: vi.fn().mockResolvedValue(undefined),
      error: null,
      clearError: vi.fn(),
    });

    const view = render(
      <GitStoreProvider store={store}>
        <ConflictResolver conflictState={baseConflict({ ours: 'file A ours' })} />
      </GitStoreProvider>,
    );

    await user.click(screen.getByRole('button', { name: 'Edit Manually' }));
    const editorTextbox = screen.getByRole('textbox');
    expect(editorTextbox).toHaveValue('file A ours');
    await user.clear(editorTextbox);
    await user.type(editorTextbox, 'edited by user');

    view.rerender(
      <GitStoreProvider store={store}>
        <ConflictResolver conflictState={baseConflict({ filePath: 'b.txt', ours: 'file B ours' })} />
      </GitStoreProvider>,
    );

    // Manual mode must reset when the target file changes — otherwise the
    // edited content for a.txt could be saved into b.txt.
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Edit Manually' }));
    expect(screen.getByRole('textbox')).toHaveValue('file B ours');
  });

  it('disables Accept Ours/Theirs while a resolve is in flight and does not allow a second submit', async () => {
    let resolveResolve!: () => void;
    const resolveConflict = vi.fn(
      () => new Promise<void>((resolve) => { resolveResolve = resolve; }),
    );
    const { getByRole } = render(
      <GitStoreProvider
        store={(() => {
          const store = createGitStore();
          store.setState({ resolveConflict, abortMerge: vi.fn(), error: null, clearError: vi.fn() });
          return store;
        })()}
      >
        <ConflictResolver conflictState={baseConflict()} />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();

    const oursButton = getByRole('button', { name: 'Accept Ours' });
    await user.click(oursButton);
    expect(oursButton).toBeDisabled();
    await user.click(oursButton);
    expect(resolveConflict).toHaveBeenCalledTimes(1);

    resolveResolve();
  });

  it('requires confirmation before Abort Merge calls the store action', async () => {
    const abortMerge = vi.fn().mockResolvedValue(undefined);
    const store = createGitStore();
    store.setState({ resolveConflict: vi.fn(), abortMerge, error: null, clearError: vi.fn() });
    render(
      <GitStoreProvider store={store}>
        <ConflictResolver conflictState={baseConflict()} />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: 'Abort Merge' }));
    expect(abortMerge).not.toHaveBeenCalled();
    expect(screen.getByText(/abort/i, { selector: 'h2, [role="heading"], [class*=Title]' })).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Confirm Abort' }));
    expect(abortMerge).toHaveBeenCalledTimes(1);
  });
});
```

Note: the exact heading-lookup selector in the abort test may need adjusting once the dialog markup exists — if `getByText` with that selector doesn't match, use `screen.getByRole('alertdialog')` or `screen.getByText('Abort Merge?')` (whatever title string Step 3 below actually uses) instead; keep the assertion's intent (a confirmation surface appears and `abortMerge` is not called yet).

- [ ] **Step 2: Run it to verify it fails**

Run: `yarn vitest run src/components/git/__tests__/ConflictResolver.test.tsx`
Expected: FAIL on all three cases against the current implementation (no reset-on-file-change effect, no disabled state, no abort confirmation dialog).

- [ ] **Step 3: Rewrite `ConflictResolver.tsx`**

Replace the full file content:

```typescript
import '@/components/editor/monaco-setup';
import Editor from '@monaco-editor/react';
import { AlertCircle } from 'lucide-react';
import { useEffect, useState } from 'react';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { useMonacoTheme } from '@/components/editor/useMonacoTheme';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { useGitStore } from '@/stores/git-store-context';
import type { ConflictState } from '@/types/pane-types';

interface ConflictResolverProps {
  conflictState: ConflictState;
  /** Called after a resolve completes without error — lets the parent
   *  return to a landing view instead of leaving this resolver mounted
   *  and re-armed against a conflict that no longer exists. */
  onResolved?: () => void;
}

export function ConflictResolver({ conflictState, onResolved }: ConflictResolverProps) {
  const [manualMode, setManualMode] = useState(false);
  const [manualContent, setManualContent] = useState(conflictState.ours);
  const [busy, setBusy] = useState(false);
  const [showAbortConfirm, setShowAbortConfirm] = useState(false);
  const resolveConflict = useGitStore((s) => s.resolveConflict);
  const abortMerge = useGitStore((s) => s.abortMerge);
  const error = useGitStore((s) => s.error);
  const clearError = useGitStore((s) => s.clearError);
  const { themeName } = useMonacoTheme();

  // Reset manual-editing state whenever the target conflict changes — without
  // this, selecting a different conflicted file while in manual mode kept
  // editing the previous file's content and could save it into the new one.
  useEffect(() => {
    setManualMode(false);
    setManualContent(conflictState.ours);
  }, [conflictState.filePath, conflictState.ours]);

  const handleConfirmAbort = async () => {
    setShowAbortConfirm(false);
    setBusy(true);
    try {
      await abortMerge();
    } finally {
      setBusy(false);
    }
  };

  const handleResolve = async (resolution: 'ours' | 'theirs' | 'custom', content?: string) => {
    if (busy) return;
    setBusy(true);
    try {
      const res =
        resolution === 'custom'
          ? { resolution: 'custom' as const, content: content ?? '' }
          : { resolution };
      await resolveConflict(conflictState.filePath, res);
      onResolved?.();
    } finally {
      setBusy(false);
    }
  };

  const abortConfirmDialog = (
    <AlertDialog open={showAbortConfirm} onOpenChange={setShowAbortConfirm}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Abort Merge?</AlertDialogTitle>
          <AlertDialogDescription>
            This resets the working tree and index to <span className='font-mono'>HEAD</span>,
            discarding any staged or unstaged tracked changes made since the merge started,
            including any conflict resolutions you've already saved. Untracked files are not
            affected. This cannot be undone.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction onClick={handleConfirmAbort}>Confirm Abort</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );

  if (manualMode) {
    return (
      <div className='flex flex-col h-full'>
        <div className='flex items-center gap-2 border-b px-3 py-1.5'>
          <Badge variant='destructive' className='text-[9px]'>
            Conflict
          </Badge>
          <span className='font-mono text-sm truncate'>{conflictState.filePath}</span>
          <div className='ml-auto flex gap-1'>
            <Button
              variant='outline'
              size='sm'
              className='h-6 text-sm text-destructive'
              onClick={() => setShowAbortConfirm(true)}
              disabled={busy}
            >
              Abort Merge
            </Button>
            <Button
              variant='outline'
              size='sm'
              className='h-6 text-sm'
              onClick={() => setManualMode(false)}
              disabled={busy}
            >
              Back
            </Button>
            <Button
              size='sm'
              className='h-6 text-sm'
              onClick={() => handleResolve('custom', manualContent)}
              disabled={busy}
            >
              Save Resolution
            </Button>
          </div>
        </div>
        {error && (
          <div className='flex items-start gap-2 mx-3 mt-2 rounded-md bg-destructive/10 border border-destructive/30 px-3 py-2 text-xs text-destructive'>
            <AlertCircle className='h-3.5 w-3.5 shrink-0 mt-0.5' />
            <span className='flex-1 wrap-break-word'>{error}</span>
            <button
              type='button'
              className='shrink-0 hover:opacity-70 leading-none'
              onClick={clearError}
              aria-label='Dismiss error'
            >
              ×
            </button>
          </div>
        )}
        <div className='flex-1'>
          <Editor
            value={manualContent}
            onChange={(v) => setManualContent(v ?? '')}
            theme={themeName}
            options={{ minimap: { enabled: false }, fontSize: 12, scrollBeyondLastLine: false }}
          />
        </div>
        {abortConfirmDialog}
      </div>
    );
  }

  return (
    <div className='flex flex-col h-full'>
      <div className='flex items-center gap-2 border-b px-3 py-1.5'>
        <Badge variant='destructive' className='text-[9px]'>
          Conflict
        </Badge>
        <span className='font-mono text-sm truncate'>{conflictState.filePath}</span>
        <div className='ml-auto'>
          <Button
            variant='outline'
            size='sm'
            className='h-6 text-sm text-destructive'
            onClick={() => setShowAbortConfirm(true)}
            disabled={busy}
          >
            Abort Merge
          </Button>
        </div>
      </div>
      {error && (
        <div className='flex items-start gap-2 mx-3 mt-2 rounded-md bg-destructive/10 border border-destructive/30 px-3 py-2 text-xs text-destructive'>
          <AlertCircle className='h-3.5 w-3.5 shrink-0 mt-0.5' />
          <span className='flex-1 wrap-break-word'>{error}</span>
          <button
            type='button'
            className='shrink-0 hover:opacity-70 leading-none'
            onClick={clearError}
            aria-label='Dismiss error'
          >
            ×
          </button>
        </div>
      )}
      <div className='flex flex-1 min-h-0'>
        <div className='flex-1 flex flex-col border-r'>
          <div className='px-2 py-1 text-sm font-medium text-muted-foreground border-b'>Ours</div>
          <div className='flex-1'>
            <Editor
              value={conflictState.ours}
              theme={themeName}
              options={{
                readOnly: true,
                minimap: { enabled: false },
                fontSize: 12,
                scrollBeyondLastLine: false,
              }}
            />
          </div>
        </div>
        <div className='flex-1 flex flex-col'>
          <div className='px-2 py-1 text-sm font-medium text-muted-foreground border-b'>Theirs</div>
          <div className='flex-1'>
            <Editor
              value={conflictState.theirs}
              theme={themeName}
              options={{
                readOnly: true,
                minimap: { enabled: false },
                fontSize: 12,
                scrollBeyondLastLine: false,
              }}
            />
          </div>
        </div>
      </div>
      <div className='flex items-center gap-2 border-t px-3 py-2'>
        <Button
          variant='outline'
          size='sm'
          onClick={() => handleResolve('ours')}
          disabled={busy}
        >
          Accept Ours
        </Button>
        <Button
          variant='outline'
          size='sm'
          onClick={() => handleResolve('theirs')}
          disabled={busy}
        >
          Accept Theirs
        </Button>
        <Button
          variant='secondary'
          size='sm'
          onClick={() => setManualMode(true)}
          disabled={busy}
        >
          Edit Manually
        </Button>
      </div>
      {abortConfirmDialog}
    </div>
  );
}
```

This addresses UJ-08's core defects: the `useEffect` keyed on `conflictState.filePath` resets manual state on file switch; `busy` disables every action button (including the outer non-manual-mode ones) and is checked at the top of `handleResolve` as a second guard against a race between click and disabled-state re-render; and `onResolved` gives the caller (Task 9 wires it in `GitPanel`) a hook to leave the resolver on success. UJ-09's fix is the new `showAbortConfirm` dialog gating `abortMerge`.

Note: this task does not add "validate the target is still an unresolved index conflict before any write" (the other half of UJ-08's acceptance criteria, about the backend defaulting to empty content for an already-resolved/missing conflict) — that guard already exists at the backend (`conflict.rs`'s "prove this exact path is currently conflicted" check, confirmed present in the current codebase) — so the empty-file-write scenario UJ-08 originally described is already prevented server-side; this task's job was the frontend re-arm/race bugs only.

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn vitest run src/components/git/__tests__/ConflictResolver.test.tsx`
Expected: PASS on all three cases. If the abort dialog's title-lookup assertion in Step 1 doesn't match, adjust it to match the actual `AlertDialogTitle` text (`Abort Merge?`) now that it's implemented, per the note left in Step 1.

- [ ] **Step 5: Run type check and lint**

Run: `yarn tsc --noEmit && yarn check`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add src/components/git/ConflictResolver.tsx src/components/git/__tests__/ConflictResolver.test.tsx
git commit -m "fix(git): reset conflict resolver on file switch, guard double-submit, confirm abort"
```

---

### Task 9: Wire `onResolved` in `GitPanel`; final full-suite verification

**Files:**
- Modify: `src/components/git/GitPanel.tsx:317-329` (the `ConflictResolver` render site)

**Interfaces:**
- Consumes: `onResolved` prop added to `ConflictResolver` in Task 8.
- Produces: nothing further — this is the last task in the plan.

- [ ] **Step 1: Pass `onResolved` from `GitPanel`**

In `src/components/git/GitPanel.tsx`, change the `ConflictResolver` render (lines 317-329) from:
```typescript
            {rightPanel.kind === 'conflict' && (
              <Suspense fallback={null}>
                <ConflictResolver
                  conflictState={{
                    filePath: rightPanel.conflictFile.path,
                    repositoryId,
                    repositoryLabel,
                    ours: rightPanel.conflictFile.ours,
                    theirs: rightPanel.conflictFile.theirs,
                    ancestor: rightPanel.conflictFile.ancestor ?? null,
                  }}
                />
              </Suspense>
            )}
```
to:
```typescript
            {rightPanel.kind === 'conflict' && (
              <Suspense fallback={null}>
                <ConflictResolver
                  conflictState={{
                    filePath: rightPanel.conflictFile.path,
                    repositoryId,
                    repositoryLabel,
                    ours: rightPanel.conflictFile.ours,
                    theirs: rightPanel.conflictFile.theirs,
                    ancestor: rightPanel.conflictFile.ancestor ?? null,
                  }}
                  onResolved={() => setRightPanel({ kind: 'landing' })}
                />
              </Suspense>
            )}
```

This satisfies UJ-08's "On success, the UI advances to the next unresolved conflict or exits to a clear completion state" — exiting to landing is the simpler of the two acceptable behaviors named in that criterion; advancing directly to the next conflicted file would require `GitPanel` to look up the next entry in `status.files`, which is a reasonable follow-up but not required to close this finding (returning to landing already lets the always-visible `GitFileList` show the next conflicted row for the user to click).

- [ ] **Step 2: Run full type check, lint, and full test suite**

Run: `yarn tsc --noEmit && yarn check && yarn vitest run`
Expected: all clean, all tests pass (the full suite, not just the files touched in this plan — confirms nothing elsewhere regressed from the store's public shape changing).

- [ ] **Step 3: Manual verification in the running app**

Run `yarn tauri dev`. Then:
1. Open two collections in a split pane (drag a tab to create a second pane, or use the split-pane control) and open the Git panel for each. Confirm each panel's status/branch/commit list is independent — make a change in one collection's working tree and confirm only that panel's file list updates (the other should not refresh until its own debounce/effect fires for its own path).
2. In a repository with an unstaged change, click the per-file Discard trash icon and confirm the new confirmation dialog appears and names the file; cancel, then confirm, and verify the file is discarded only after confirming.
3. Create a real merge conflict (or use an existing fixture repo) and: (a) click Accept Ours and confirm the button disables during the (near-instant) operation and the resolver returns to the overview afterward; (b) reopen a different conflicted file in manual mode and confirm the editor shows that file's own `ours` content, not a leftover from the previous file; (c) click Abort Merge and confirm a confirmation dialog appears before anything happens.
4. In the branch popover, click a remote branch and confirm a spinner appears on that row while checkout is in flight.

- [ ] **Step 4: Commit**

```bash
git add src/components/git/GitPanel.tsx
git commit -m "feat(git): return to landing view after a successful conflict resolution"
```

---

## Self-Review Notes

**Spec coverage:**
- UJ-01 (repository scope race) → Tasks 1-4 (factory store, context, GitPanel wiring, pre-warm removal), verified by Task 3's two-panel test and Task 9's manual split-pane check.
- UJ-06 (individual discard, no confirmation) → Task 5.
- UJ-07 (remote checkout, no dirty preflight/busy state) → Task 6. Backend preflight already shipped separately (Task 2.1b of the backend plan); this task closes the frontend busy-state half.
- UJ-08 (conflict resolver re-arm / stale manual content) → Task 8 + Task 9's `onResolved` wiring.
- UJ-09 (abort merge, no confirmation) → Task 8.
- Full acceptance-criteria items this plan intentionally does NOT cover (left for later phases per the source report's own phasing): typed operation results (UJ-05, Phase 2), "advance directly to next conflict" as opposed to "return to landing" (UJ-08's second acceptable option), branch-delete/stash-drop confirmations (UJ-11, Phase 4), and the general Zustand full-destructure cleanup in `GitRemotesDialog`/`GitCommitLog` (deferred — action-only destructuring is not a correctness bug, just a style deviation, and refactoring it isn't necessary to close UJ-01/06/07/08/09).

**Placeholder scan:** every step above has real code; no "similar to Task N," no "add appropriate error handling," no unresolved TBDs.

**Type consistency:** `createGitStore()` (Task 1) → `GitStoreProvider`/`useGitStore`/`useGitStoreApi` (Task 2) → every consumer task imports exactly those three names from `@/stores/git-store-context`, never re-declaring or renaming them. `ConflictResolver`'s new `onResolved` prop (Task 8) is defined and consumed with the same name and signature (Task 9).
