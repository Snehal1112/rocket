# Frontend Architecture Modernisation — TanStack Query + Layout Store

**Date:** 2026-05-01
**Scope:** Data fetching layer (A) + Layout state management (B)
**Follow-up:** Module boundary splitting (C) — separate spec

---

## Context

The current frontend has two architectural gaps that this spec addresses:

1. **No data-fetching layer.** All async IPC calls are fire-and-forget (`void asyncFn()`) inside Zustand store actions. There is no caching, deduplication, retry, or coordinated loading state. Every store reinvents the same async pattern.

2. **Layout state is unmanaged.** `App.tsx` holds `sidebarWidth`, `isConsoleOpen`, and `consoleHeight` as local `useState`. These are never persisted. `sidebarCollapsed` is wired but never toggled — dead state.

---

## Guiding Principle

> **TanStack Query owns server state. Zustand owns UI state.**

Server state = anything that lives on disk or behind a Tauri IPC call and can become stale.
UI state = anything that exists only in memory to drive the interface (tabs, pane tree, active selections, layout dimensions).

These two responsibilities must not be mixed.

---

## Part A — TanStack Query Data Fetching Layer

### Installation

```bash
yarn add @tanstack/react-query
```

No other packages needed. DevTools (`@tanstack/react-query-devtools`) are optional and can be added later.

### QueryClient Setup

Add `QueryClientProvider` at the root in `src/main.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,       // 30s — IPC data doesn't change without an event
      retry: 2,
      refetchOnWindowFocus: false, // Tauri desktop app, not a browser tab
    },
  },
});

// Wrap <App /> with <QueryClientProvider client={queryClient}>
```

### Query Key Convention

All query keys are string arrays with a domain prefix:

```ts
['workspaces']
['workspaces', 'active']
['collections', workspaceId]
['environments', collectionName]
['environments', 'global', workspaceId]
['history', collectionName]
```

### Query Hook Files

Four files under `src/lib/queries/`:

#### `workspace-queries.ts`

```ts
useWorkspaces()         → useQuery(['workspaces'], listWorkspaces)
useActiveWorkspace()    → useQuery(['workspaces', 'active'], getActiveWorkspace)
useSwitchWorkspace()    → useMutation → invalidates ['workspaces'], ['environments']
useCreateWorkspace()    → useMutation → invalidates ['workspaces']
useRenameWorkspace()    → useMutation → invalidates ['workspaces']
useDeleteWorkspace()    → useMutation → invalidates ['workspaces']
usePinWorkspace()       → useMutation → invalidates ['workspaces']
useUnpinWorkspace()     → useMutation → invalidates ['workspaces']
```

#### `environment-queries.ts`

```ts
useEnvironments(collectionName)  → useQuery(['environments', collectionName], ...)
useGlobalEnv()                   → useQuery(['environments', 'global', workspaceId], ...)
useProcessEnvVars()              → useQuery(['environments', 'process'], ..., { staleTime: Infinity })
useSaveEnvironment()             → useMutation → invalidates ['environments', collectionName]
useDeleteEnvironment()           → useMutation → invalidates ['environments', collectionName]
```

#### `collection-queries.ts`

```ts
useCollections(workspaceId)   → useQuery(['collections', workspaceId], listCollections)
```

#### `history-queries.ts`

```ts
useHistory(collectionName)    → useQuery(['history', collectionName], ...)
```

### Tauri Event → Cache Invalidation

The existing `listen()` event handlers in `workspace-store.ts` currently call `set()` on the store directly. They move to `App.tsx` (or a dedicated `useAppEvents` hook) and call `queryClient.invalidateQueries()` instead:

```ts
// Before (workspace-store.ts)
listen('workspace-switched', ({ payload }) => {
  useWorkspaceStore.setState({ activeWorkspaceId: payload.id });
  ...
});

// After (App.tsx useEffect or useAppEvents hook)
listen('workspace-switched', ({ payload }) => {
  useWorkspaceStore.setState({ activeWorkspaceId: payload.id }); // only UI state
  queryClient.invalidateQueries({ queryKey: ['workspaces'] });
  queryClient.invalidateQueries({ queryKey: ['environments'] });
});
```

This keeps event subscriptions in one place and removes async logic from stores entirely.

### Zustand Store Changes

| Store | Keeps | Drops |
|---|---|---|
| `workspace-store.ts` | `activeWorkspaceId`, `multiWorkspaceMode` | `workspaces[]`, all async fetch/mutate actions |
| `env-store.ts` | `activeEnvId`, `activeCollection` | `environments[]`, `processEnvVars[]`, all fetch actions |

`workspace-store.ts` shrinks from ~200 lines to ~40.
`env-store.ts` shrinks from ~215 lines to ~30.

The `collectionTabState` keyed snapshot in `pane-store` is unaffected — it is pure UI state.

### Variable Context (env resolution)

`src/lib/variable-context.ts` and `useExecuteRequest.ts` currently read env state directly from `useEnvStore`. After this change they read from the query cache via the hook (`useEnvironments(collectionName)`). The resolved variable map is derived at render time, not stored.

---

## Part B — Layout Store + Persistence

### New Fields in `layout-store.ts`

```ts
interface LayoutStore {
  requestLayout: RequestLayout;       // existing
  sidebarWidth: number;               // new — default 280
  isConsoleOpen: boolean;             // new — default false
  consoleHeight: number;              // new — default 280

  setRequestLayout: (dir: RequestLayout) => void;
  setSidebarWidth: (w: number) => void;
  setConsoleOpen: (open: boolean) => void;
  setConsoleHeight: (h: number) => void;
}
```

### App.tsx Changes

Remove:
```ts
const [sidebarWidth, setSidebarWidth] = useState(280);
const [sidebarCollapsed] = useState(false);   // dead — deleted entirely
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

`sidebarCollapsed` is removed with no replacement. The sidebar is always shown in the current implementation.

### Persistence via ui-state.ts

`UiState` (the persisted shape in `tauri-api.ts`) gains three fields:

```ts
interface UiState {
  // existing
  activeMode: 'workspace' | 'collection';
  layoutDirection: 'stacked' | 'side-by-side';
  activeCollection?: string;
  workspaceTabs?: { workspaceId: string };
  collectionTabs?: UiStateCollectionTab[];

  // new
  sidebarWidth?: number;
  isConsoleOpen?: boolean;
  consoleHeight?: number;
}
```

`scheduleSaveUiState()` in `ui-state.ts` already reads `useLayoutStore.getState()` for `requestLayout`. It gains three more reads:

```ts
sidebarWidth: layoutState.sidebarWidth,
isConsoleOpen: layoutState.isConsoleOpen,
consoleHeight: layoutState.consoleHeight,
```

`restoreUiState()` in `App.tsx` already hydrates `requestLayout` into the store. It gains:

```ts
if (uiState.sidebarWidth) useLayoutStore.getState().setSidebarWidth(uiState.sidebarWidth);
if (uiState.isConsoleOpen !== undefined) useLayoutStore.getState().setConsoleOpen(uiState.isConsoleOpen);
if (uiState.consoleHeight) useLayoutStore.getState().setConsoleHeight(uiState.consoleHeight);
```

Auto-save already fires on `usePaneStore.subscribe(scheduleSaveUiState)`. A parallel subscription is added for `layout-store`:

```ts
useLayoutStore.subscribe(scheduleSaveUiState);
```

This fires the same debounced save (500ms) whenever any layout value changes, keeping persistence logic in one place.

---

## Files Changed

| File | Change |
|---|---|
| `src/main.tsx` | Add `QueryClient`, wrap with `QueryClientProvider` |
| `src/lib/queries/workspace-queries.ts` | New — query + mutation hooks |
| `src/lib/queries/environment-queries.ts` | New — query + mutation hooks |
| `src/lib/queries/collection-queries.ts` | New — query hooks |
| `src/lib/queries/history-queries.ts` | New — query hooks |
| `src/stores/workspace-store.ts` | Drop async actions and `workspaces[]` state |
| `src/stores/env-store.ts` | Drop async fetch actions and `environments[]` state |
| `src/stores/layout-store.ts` | Add 3 fields + setters |
| `src/lib/ui-state.ts` | Extend `UiState`, read layout fields in save, hydrate in restore |
| `src/lib/tauri-api.ts` | Add 3 fields to `UiState` type |
| `src/App.tsx` | Drop local state, read from layout-store, Tauri events → invalidateQueries |

---

## What Does Not Change

- `tauri-api.ts` raw IPC functions — untouched
- `pane-store.ts` — untouched (pure UI state, Zustand is correct here)
- All components — they call hooks instead of store actions, but the hook API surface is similar
- Tauri event listener registrations — moved to `App.tsx`, not removed
- Auto-save debounce timing and `scheduleSaveUiState` logic — extended, not rewritten

---

## Out of Scope (deferred to spec C)

- Splitting `tauri-api.ts` by domain
- Splitting `CollectionsSidebar.tsx`
- Any other component-level refactoring
