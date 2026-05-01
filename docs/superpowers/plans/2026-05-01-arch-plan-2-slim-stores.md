# Architecture Plan 2 — Slim Down Stores + Wire Tauri Events to Query Cache

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove async server-state from `workspace-store` and `env-store`, replace their consumers with the query hooks from Plan 1, and reroute Tauri event listeners to `queryClient.invalidateQueries()`.

**Architecture:** After this plan, Zustand stores hold only UI state. `workspace-store` keeps `activeWorkspaceId` and `multiWorkspaceMode`. `env-store` keeps `activeEnvId` and `activeCollection`. All components that previously read `workspaces[]` or `environments[]` from stores now use query hooks. Tauri events invalidate the query cache instead of calling `set()`.

**Prerequisite:** Plan 1 must be complete — `src/lib/queries/workspace-queries.ts` and `src/lib/queries/environment-queries.ts` must exist.

**Tech Stack:** Zustand v5, `@tanstack/react-query` v5, `@tauri-apps/api/event`.

**Spec:** `docs/superpowers/specs/2026-05-01-tanstack-query-layout-store-design.md`

---

### Task 1: Slim down workspace-store and migrate its consumers

**Files:**
- Modify: `src/stores/workspace-store.ts`
- Modify: `src/components/title-bar/WorkspaceSwitcher.tsx`
- Modify: `src/components/panes/BreadcrumbBar.tsx`
- Modify: `src/components/layout/CollectionDropdown.tsx`
- Modify: `src/App.tsx`

- [ ] **Step 1: Replace `workspace-store.ts` with UI-state-only version**

The new store keeps only `activeWorkspaceId` and `multiWorkspaceMode`. All async fetch/mutate actions are deleted. Tauri event listeners move to `App.tsx` in Task 3 of this plan.

Replace the full contents of `src/stores/workspace-store.ts`:

```ts
import { create } from 'zustand';

interface WorkspaceState {
  activeWorkspaceId: string;
  multiWorkspaceMode: boolean;
  setActiveWorkspaceId: (id: string) => void;
  setMultiWorkspaceMode: (enabled: boolean) => void;
}

export const useWorkspaceStore = create<WorkspaceState>()((set) => ({
  activeWorkspaceId: '',
  multiWorkspaceMode: false,
  setActiveWorkspaceId: (id) => set({ activeWorkspaceId: id }),
  setMultiWorkspaceMode: (enabled) => set({ multiWorkspaceMode: enabled }),
}));
```

- [ ] **Step 2: Migrate WorkspaceSwitcher.tsx**

Open `src/components/title-bar/WorkspaceSwitcher.tsx`. Replace the store imports and usage.

Remove:
```ts
import { useWorkspaceStore } from '@/stores/workspace-store';
// and all these store selectors:
const workspaces = useWorkspaceStore((s) => s.workspaces);
const activeId = useWorkspaceStore((s) => s.activeWorkspaceId);
const switchWorkspace = useWorkspaceStore((s) => s.switchWorkspace);
const closeWorkspace = useWorkspaceStore((s) => s.closeWorkspace);
const deleteWorkspace = useWorkspaceStore((s) => s.deleteWorkspace);
```

Add:
```ts
import { useWorkspaceStore } from '@/stores/workspace-store';
import {
  useWorkspaces,
  useCloseWorkspace,
  useDeleteWorkspace,
  usePinWorkspace,
  useUnpinWorkspace,
  useSwitchWorkspace,
  useOpenWorkspaceFromDisk,
} from '@/lib/queries/workspace-queries';

const { data: workspaces = [] } = useWorkspaces();
const activeId = useWorkspaceStore((s) => s.activeWorkspaceId);
const switchWorkspaceMutation = useSwitchWorkspace();
const closeWorkspaceMutation = useCloseWorkspace();
const deleteWorkspaceMutation = useDeleteWorkspace();
const pinMutation = usePinWorkspace();
const unpinMutation = useUnpinWorkspace();
const openFromDiskMutation = useOpenWorkspaceFromDisk();
```

Replace all call sites:
- `switchWorkspace(id)` → `switchWorkspaceMutation.mutate(id)`
- `closeWorkspace(id)` → `closeWorkspaceMutation.mutate(id)`
- `deleteWorkspace(id)` → `deleteWorkspaceMutation.mutate(id)`
- `useWorkspaceStore.getState().unpinWorkspace(ws.id)` → `unpinMutation.mutate(ws.id)`
- `useWorkspaceStore.getState().pinWorkspace(ws.id)` → `pinMutation.mutate(ws.id)`
- `useWorkspaceStore.getState().openWorkspaceFromDisk(path)` → `await openFromDiskMutation.mutateAsync(path)`

- [ ] **Step 3: Migrate BreadcrumbBar.tsx and CollectionDropdown.tsx**

In `src/components/panes/BreadcrumbBar.tsx`, replace:
```ts
import { useWorkspaceStore } from '@/stores/workspace-store';
const workspaces = useWorkspaceStore((s) => s.workspaces);
const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
const switchWorkspace = useWorkspaceStore((s) => s.switchWorkspace);
```
With:
```ts
import { useWorkspaceStore } from '@/stores/workspace-store';
import { useWorkspaces, useSwitchWorkspace } from '@/lib/queries/workspace-queries';
const { data: workspaces = [] } = useWorkspaces();
const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
const switchWorkspaceMutation = useSwitchWorkspace();
```
Replace `switchWorkspace(id)` calls with `switchWorkspaceMutation.mutate(id)`.

In `src/components/layout/CollectionDropdown.tsx`, replace:
```ts
import { useWorkspaceStore } from '@/stores/workspace-store';
const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
const activeWorkspace = useWorkspaceStore((s) => { ... });
```
With:
```ts
import { useWorkspaceStore } from '@/stores/workspace-store';
import { useWorkspaces } from '@/lib/queries/workspace-queries';
const { data: workspaces = [] } = useWorkspaces();
const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
const activeWorkspace = workspaces.find((w) => w.id === activeWorkspaceId) ?? null;
```

- [ ] **Step 4: Update App.tsx startup — remove loadWorkspaces call**

In `src/App.tsx`, remove:
```ts
const loadWorkspaces = useWorkspaceStore((s) => s.loadWorkspaces);
```
And remove the `await loadWorkspaces()` call from the `init()` effect.

Replace the workspace lookup in `init()` that reads `useWorkspaceStore.getState().workspaces`:
```ts
// Before
const ws = useWorkspaceStore.getState().workspaces.find((w) => w.id === workspaceId);

// After — read from query cache directly
import { workspaceKeys } from '@/lib/queries/workspace-queries';
import { getQueryClient } from '@/lib/query-client';
const cachedWorkspaces = getQueryClient().getQueryData(workspaceKeys.all) as Workspace[] ?? [];
const ws = cachedWorkspaces.find((w) => w.id === workspaceId);
```

Also create `src/lib/query-client.ts` to export a singleton queryClient for non-hook access:

```ts
import { QueryClient } from '@tanstack/react-query';

let client: QueryClient | null = null;

export function setQueryClient(qc: QueryClient) {
  client = qc;
}

export function getQueryClient(): QueryClient {
  if (!client) throw new Error('QueryClient not initialised');
  return client;
}
```

In `src/main.tsx`, after creating `queryClient`, add:
```ts
import { setQueryClient } from '@/lib/query-client';
setQueryClient(queryClient);
```

- [ ] **Step 5: Verify TypeScript compiles with no errors**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add src/stores/workspace-store.ts \
        src/components/title-bar/WorkspaceSwitcher.tsx \
        src/components/panes/BreadcrumbBar.tsx \
        src/components/layout/CollectionDropdown.tsx \
        src/App.tsx \
        src/lib/query-client.ts \
        src/main.tsx
git commit -m "refactor: slim workspace-store to UI state, consumers use query hooks"
```

---

### Task 2: Slim down env-store and migrate its consumers

**Files:**
- Modify: `src/stores/env-store.ts`
- Modify: `src/components/layout/EnvironmentSwitcher.tsx`
- Modify: `src/components/environments/EnvironmentDialog.tsx`
- Modify: `src/lib/execute-request.ts`
- Modify: `src/App.tsx`

- [ ] **Step 1: Replace `env-store.ts` with UI-state-only version**

Replace the full contents of `src/stores/env-store.ts`:

```ts
import { create } from 'zustand';

interface EnvState {
  activeEnvId: string | null;
  activeCollection: string | null;
  setActiveEnvId: (id: string | null) => void;
  setActiveCollection: (name: string | null) => void;
}

export const useEnvStore = create<EnvState>()((set) => ({
  activeEnvId: null,
  activeCollection: null,
  setActiveEnvId: (id) => {
    set({ activeEnvId: id });
  },
  setActiveCollection: (name) => set({ activeCollection: name }),
}));
```

The `setActiveEnvId` setter must also persist the selection to localStorage (preserving existing behaviour):

```ts
setActiveEnvId: (id) => {
  const { activeCollection } = useEnvStore.getState();
  set({ activeEnvId: id });
  const key = activeCollection ? `rocket-api:active-env:${activeCollection}` : null;
  if (key) {
    if (id) localStorage.setItem(key, id);
    else localStorage.removeItem(key);
  }
},
```

- [ ] **Step 2: Migrate EnvironmentSwitcher.tsx**

Open `src/components/layout/EnvironmentSwitcher.tsx`. Replace store selectors:

Remove:
```ts
const environments = useEnvStore((s) => s.environments);
const activeEnvId = useEnvStore((s) => s.activeEnvId);
const setActiveEnv = useEnvStore((s) => s.setActiveEnv);
const globalEnvName = useEnvStore((s) => s.globalEnvName);
const setGlobalEnv = useEnvStore((s) => s.setGlobalEnv);
const globalEnvironments = useEnvStore((s) => s.globalEnvironments);
const loadGlobalEnvironments = useEnvStore((s) => s.loadGlobalEnvironments);
const createGlobalEnvironment = useEnvStore((s) => s.createGlobalEnvironment);
const createEnvironment = useEnvStore((s) => s.createEnvironment);
```

Add:
```ts
import { useEnvStore } from '@/stores/env-store';
import {
  useEnvironments,
  useGlobalEnvironmentName,
  useGlobalEnvironments,
  useSetGlobalEnvironment,
  useSaveEnvironment,
  useSaveGlobalEnvironment,
} from '@/lib/queries/environment-queries';

const activeCollection = useEnvStore((s) => s.activeCollection);
const activeEnvId = useEnvStore((s) => s.activeEnvId);
const setActiveEnvId = useEnvStore((s) => s.setActiveEnvId);
const { data: environments = [] } = useEnvironments(activeCollection);
const { data: globalEnvName = null } = useGlobalEnvironmentName();
const { data: globalEnvironments = [] } = useGlobalEnvironments();
const setGlobalEnvMutation = useSetGlobalEnvironment();
const saveEnvMutation = useSaveEnvironment(activeCollection);
const saveGlobalMutation = useSaveGlobalEnvironment();
```

Replace call sites:
- `setActiveEnv(id)` → `setActiveEnvId(id)`
- `setGlobalEnv(name)` → `setGlobalEnvMutation.mutate(name)`
- `loadGlobalEnvironments()` → removed (query auto-fetches)
- `createEnvironment(name)` → `saveEnvMutation.mutate({ name, variables: [] })` then `setActiveEnvId(name)`
- `createGlobalEnvironment(name)` → `saveGlobalMutation.mutate({ name, variables: [] })`
- `useWorkspaceStore.getState().activeWorkspaceId` → `useWorkspaceStore((s) => s.activeWorkspaceId)`

- [ ] **Step 3: Migrate EnvironmentDialog.tsx**

Open `src/components/environments/EnvironmentDialog.tsx`. Replace store imports:

Remove all `useEnvStore` selector lines and the direct `useEnvStore.setState(...)` calls.

Add:
```ts
import { useEnvStore } from '@/stores/env-store';
import {
  useEnvironments,
  useSaveEnvironment,
  useDeleteEnvironment,
} from '@/lib/queries/environment-queries';

const activeCollection = useEnvStore((s) => s.activeCollection);
const activeEnvId = useEnvStore((s) => s.activeEnvId);
const setActiveEnvId = useEnvStore((s) => s.setActiveEnvId);
const { data: environments = [] } = useEnvironments(activeCollection);
const saveMutation = useSaveEnvironment(activeCollection);
const deleteMutation = useDeleteEnvironment(activeCollection);
```

Replace:
- `createEnvironment(name)` → `saveMutation.mutate({ name, variables: [] })` then `setActiveEnvId(name)`
- `deleteEnvironmentStore(name)` → `deleteMutation.mutate(name)`
- All `useEnvStore.setState(...)` optimistic patches → replace with `saveMutation.mutate(updatedEnv)` (the query invalidation on success re-fetches the list)
- `useEnvStore.getState().activeEnvId` → `activeEnvId` (already in scope)
- `useEnvStore.getState().setActiveEnv(name)` → `setActiveEnvId(name)`

- [ ] **Step 4: Migrate execute-request.ts**

Open `src/lib/execute-request.ts`. The env store is accessed via `.getState()` to read variables at send-time (not in a React component). Replace the store reads with direct query cache reads:

Remove:
```ts
import { useEnvStore } from '@/stores/env-store';
```

Add:
```ts
import { useEnvStore } from '@/stores/env-store';
import { environmentKeys } from '@/lib/queries/environment-queries';
import { getQueryClient } from '@/lib/query-client';
import type { Environment } from '@/lib/tauri-api';
```

At each usage site, replace `useEnvStore.getState().environments` with:
```ts
const activeCollection = useEnvStore.getState().activeCollection;
const environments: Environment[] = activeCollection
  ? (getQueryClient().getQueryData(environmentKeys.collection(activeCollection)) ?? [])
  : [];
```

Replace `useEnvStore.getState().activeEnvId` with:
```ts
useEnvStore.getState().activeEnvId
```
(this stays — `activeEnvId` remains in the store)

Replace `useEnvStore.getState().globalEnv` reads with:
```ts
const globalEnvName = getQueryClient().getQueryData<string | null>(environmentKeys.globalName) ?? null;
const globalEnv: Environment | null = globalEnvName
  ? (getQueryClient().getQueryData(environmentKeys.global(globalEnvName)) ?? null)
  : null;
```

- [ ] **Step 5: Update App.tsx — remove env store bootstrap calls**

In `src/App.tsx`, remove:
```ts
void useEnvStore.getState().loadProcessEnvVars();
void useEnvStore.getState().fetchGlobalEnv();
void useEnvStore.getState().loadEnvironments(initialCollection);
```

These are now handled automatically by TanStack Query when components that call `useEnvironments()`, `useGlobalEnvironmentName()`, and `useProcessEnvVars()` mount.

Also remove the `useEnvStore` import from `App.tsx` if it is no longer used.

- [ ] **Step 6: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Expected: no errors. Fix any remaining type errors before committing.

- [ ] **Step 7: Commit**

```bash
git add src/stores/env-store.ts \
        src/components/layout/EnvironmentSwitcher.tsx \
        src/components/environments/EnvironmentDialog.tsx \
        src/lib/execute-request.ts \
        src/App.tsx
git commit -m "refactor: slim env-store to UI state, consumers use query hooks"
```

---

### Task 3: Reroute Tauri events to query cache invalidation

**Files:**
- Modify: `src/App.tsx`

The Tauri event listeners currently live in `workspace-store.ts` and call `useWorkspaceStore.setState(...)` directly. They need to move to `App.tsx` and call `queryClient.invalidateQueries()` instead.

- [ ] **Step 1: Add a `useAppEvents` hook in App.tsx**

At the top of `App.tsx`, add these imports:

```ts
import { listen } from '@tauri-apps/api/event';
import { useQueryClient } from '@tanstack/react-query';
import { workspaceKeys } from '@/lib/queries/workspace-queries';
import { environmentKeys } from '@/lib/queries/environment-queries';
import type { Workspace } from '@/lib/tauri-api';
```

Inside the `App` function, add a `useEffect` that registers all Tauri event listeners:

```ts
const queryClient = useQueryClient();

useEffect(() => {
  const unsubs = Promise.all([
    listen<Workspace>('workspace-created', () => {
      queryClient.invalidateQueries({ queryKey: workspaceKeys.all });
    }),
    listen<Workspace>('workspace-switched', ({ payload }) => {
      useWorkspaceStore.getState().setActiveWorkspaceId(payload.id);
      usePaneStore.getState().closeAll();
      usePaneStore.getState().openWorkspaceTabs(payload.id);
      useEnvStore.getState().setActiveCollection(null);
      queryClient.invalidateQueries({ queryKey: workspaceKeys.all });
      queryClient.invalidateQueries({ queryKey: workspaceKeys.active });
      queryClient.invalidateQueries({ queryKey: environmentKeys.globalName });
    }),
    listen<{ id: string; newName: string }>('workspace-renamed', () => {
      queryClient.invalidateQueries({ queryKey: workspaceKeys.all });
    }),
    listen<{ id: string }>('workspace-closed', () => {
      queryClient.invalidateQueries({ queryKey: workspaceKeys.all });
    }),
    listen<{ id: string }>('workspace-deleted', () => {
      queryClient.invalidateQueries({ queryKey: workspaceKeys.all });
    }),
    listen<{ id: string }>('workspace-pinned', () => {
      queryClient.invalidateQueries({ queryKey: workspaceKeys.all });
    }),
    listen<{ id: string }>('workspace-unpinned', () => {
      queryClient.invalidateQueries({ queryKey: workspaceKeys.all });
    }),
    listen<{ id: string; description: string | null }>('workspace-description-updated', () => {
      queryClient.invalidateQueries({ queryKey: workspaceKeys.all });
    }),
  ]);

  return () => {
    unsubs.then((fns) => fns.forEach((fn) => fn()));
  };
}, [queryClient]);
```

- [ ] **Step 2: Remove the old event subscription from workspace-store**

The `subscribeToEvents()` function and its `listen()` calls were deleted when we replaced `workspace-store.ts` in Task 1. Confirm the file no longer contains any `listen` import or call.

```bash
grep -n "listen\|subscribeToEvents" src/stores/workspace-store.ts
```

Expected: no output.

- [ ] **Step 3: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Run existing tests**

```bash
yarn test
```

Expected: all tests pass. The store tests for `workspace-store` and `env-store` will need updating — they test async actions that no longer exist. Delete those test cases and keep only tests for the remaining UI state setters.

- [ ] **Step 5: Commit**

```bash
git add src/App.tsx src/stores/workspace-store.ts src/stores/env-store.ts
git commit -m "refactor: Tauri events invalidate query cache instead of writing store directly"
```
