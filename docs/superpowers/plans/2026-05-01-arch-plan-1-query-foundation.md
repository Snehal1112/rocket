# Architecture Plan 1 — TanStack Query Foundation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Install TanStack Query, wire up QueryClientProvider, and create the workspace and environment query/mutation hooks.

**Architecture:** `tauri-api.ts` stays untouched as the raw IPC layer. A new `src/lib/queries/` directory holds domain-scoped hook files. Each hook wraps existing `tauri-api.ts` functions. No store changes yet — that is Plan 2.

**Tech Stack:** `@tanstack/react-query` v5, React 19, TypeScript strict, existing `tauri-api.ts` IPC functions.

**Spec:** `docs/superpowers/specs/2026-05-01-tanstack-query-layout-store-design.md`

---

### Task 1: Install TanStack Query and wire QueryClientProvider

**Files:**
- Modify: `package.json` (dependency added via yarn)
- Modify: `src/main.tsx`

- [ ] **Step 1: Install the package**

```bash
yarn add @tanstack/react-query
```

Expected output: `@tanstack/react-query` appears in `package.json` dependencies.

- [ ] **Step 2: Add QueryClient and provider to `src/main.tsx`**

Open `src/main.tsx`. After the existing imports, add:

```tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: 2,
      refetchOnWindowFocus: false,
    },
  },
});
```

Then wrap the `<App />` render:

```tsx
ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </React.StrictMode>,
);
```

The full updated `src/main.tsx`:

```tsx
import React from 'react';
import ReactDOM from 'react-dom/client';
import './globals.css';
import App from './App';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

const storedTheme = localStorage.getItem('rocket-theme');
const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
if (storedTheme === 'dark' || (!storedTheme && prefersDark)) {
  document.documentElement.classList.add('dark');
} else {
  document.documentElement.classList.remove('dark');
}

import editorWorker from 'monaco-editor/esm/vs/editor/editor.worker?worker';
import jsonWorker from 'monaco-editor/esm/vs/language/json/json.worker?worker';

self.MonacoEnvironment = {
  getWorker(_: unknown, label: string) {
    if (label === 'json') return new jsonWorker();
    return new editorWorker();
  },
};

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: 2,
      refetchOnWindowFocus: false,
    },
  },
});

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </React.StrictMode>,
);
```

- [ ] **Step 3: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add package.json yarn.lock src/main.tsx
git commit -m "feat: install TanStack Query and wire QueryClientProvider"
```

---

### Task 2: Create workspace query and mutation hooks

**Files:**
- Create: `src/lib/queries/workspace-queries.ts`

- [ ] **Step 1: Create the queries directory and workspace hooks file**

Create `src/lib/queries/workspace-queries.ts` with the full content below. These hooks wrap the existing `tauri-api.ts` functions — do not change `tauri-api.ts`.

```ts
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  closeWorkspace,
  createWorkspace,
  deleteWorkspace,
  getActiveWorkspace,
  getMultiWorkspaceMode,
  listWorkspaces,
  openWorkspaceFromDisk,
  pinWorkspace,
  renameWorkspace,
  setMultiWorkspaceMode,
  switchWorkspace,
  unpinWorkspace,
  updateWorkspaceDescription,
} from '@/lib/tauri-api';

export const workspaceKeys = {
  all: ['workspaces'] as const,
  active: ['workspaces', 'active'] as const,
  multiMode: ['workspaces', 'multiMode'] as const,
};

export function useWorkspaces() {
  return useQuery({
    queryKey: workspaceKeys.all,
    queryFn: listWorkspaces,
  });
}

export function useActiveWorkspace() {
  return useQuery({
    queryKey: workspaceKeys.active,
    queryFn: getActiveWorkspace,
  });
}

export function useMultiWorkspaceMode() {
  return useQuery({
    queryKey: workspaceKeys.multiMode,
    queryFn: getMultiWorkspaceMode,
  });
}

export function useCreateWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ name, path }: { name: string; path: string }) => createWorkspace(name, path),
    onSuccess: () => qc.invalidateQueries({ queryKey: workspaceKeys.all }),
  });
}

export function useSwitchWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => switchWorkspace(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: workspaceKeys.all });
      qc.invalidateQueries({ queryKey: workspaceKeys.active });
    },
  });
}

export function useRenameWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, newName }: { id: string; newName: string }) => renameWorkspace(id, newName),
    onSuccess: () => qc.invalidateQueries({ queryKey: workspaceKeys.all }),
  });
}

export function useCloseWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => closeWorkspace(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: workspaceKeys.all }),
  });
}

export function useDeleteWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteWorkspace(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: workspaceKeys.all }),
  });
}

export function usePinWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => pinWorkspace(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: workspaceKeys.all }),
  });
}

export function useUnpinWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unpinWorkspace(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: workspaceKeys.all }),
  });
}

export function useUpdateWorkspaceDescription() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, description }: { id: string; description: string | null }) =>
      updateWorkspaceDescription(id, description),
    onSuccess: () => qc.invalidateQueries({ queryKey: workspaceKeys.all }),
  });
}

export function useOpenWorkspaceFromDisk() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (path: string) => openWorkspaceFromDisk(path),
    onSuccess: () => qc.invalidateQueries({ queryKey: workspaceKeys.all }),
  });
}

export function useSetMultiWorkspaceMode() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (enabled: boolean) => setMultiWorkspaceMode(enabled),
    onSuccess: () => qc.invalidateQueries({ queryKey: workspaceKeys.multiMode }),
  });
}
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/lib/queries/workspace-queries.ts
git commit -m "feat: add workspace query and mutation hooks"
```

---

### Task 3: Create environment query and mutation hooks

**Files:**
- Create: `src/lib/queries/environment-queries.ts`

- [ ] **Step 1: Create environment hooks file**

Create `src/lib/queries/environment-queries.ts`:

```ts
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  deleteEnvironment,
  deleteGlobalEnvironment,
  getGlobalEnvironment,
  getGlobalEnvironmentName,
  getProcessEnvVars,
  listEnvironments,
  listGlobalEnvironments,
  saveEnvironment,
  saveGlobalEnvironment,
  setGlobalEnvironment,
  type Environment,
} from '@/lib/tauri-api';

export const environmentKeys = {
  collection: (collectionName: string) => ['environments', collectionName] as const,
  globalName: ['environments', 'global', 'name'] as const,
  global: (name: string) => ['environments', 'global', name] as const,
  globalList: ['environments', 'global', 'list'] as const,
  process: ['environments', 'process'] as const,
};

export function useEnvironments(collectionName: string | null) {
  return useQuery({
    queryKey: environmentKeys.collection(collectionName ?? ''),
    queryFn: () => listEnvironments(collectionName!),
    enabled: !!collectionName,
  });
}

export function useGlobalEnvironmentName() {
  return useQuery({
    queryKey: environmentKeys.globalName,
    queryFn: getGlobalEnvironmentName,
  });
}

export function useGlobalEnvironment(name: string | null) {
  return useQuery({
    queryKey: environmentKeys.global(name ?? ''),
    queryFn: () => getGlobalEnvironment(name!),
    enabled: !!name,
  });
}

export function useGlobalEnvironments() {
  return useQuery({
    queryKey: environmentKeys.globalList,
    queryFn: listGlobalEnvironments,
  });
}

export function useProcessEnvVars() {
  return useQuery({
    queryKey: environmentKeys.process,
    queryFn: getProcessEnvVars,
    staleTime: Number.POSITIVE_INFINITY,
  });
}

export function useSaveEnvironment(collectionName: string | null) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (env: Environment) => saveEnvironment(collectionName!, env),
    onSuccess: () => {
      if (collectionName) {
        qc.invalidateQueries({ queryKey: environmentKeys.collection(collectionName) });
      }
    },
  });
}

export function useDeleteEnvironment(collectionName: string | null) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (name: string) => deleteEnvironment(collectionName!, name),
    onSuccess: () => {
      if (collectionName) {
        qc.invalidateQueries({ queryKey: environmentKeys.collection(collectionName) });
      }
    },
  });
}

export function useSetGlobalEnvironment() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (name: string | null) => setGlobalEnvironment(name),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: environmentKeys.globalName });
    },
  });
}

export function useSaveGlobalEnvironment() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (env: Environment) => saveGlobalEnvironment(env),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: environmentKeys.globalList });
      qc.invalidateQueries({ queryKey: environmentKeys.globalName });
    },
  });
}

export function useDeleteGlobalEnvironment() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (name: string) => deleteGlobalEnvironment(name),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: environmentKeys.globalList });
    },
  });
}
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/lib/queries/environment-queries.ts
git commit -m "feat: add environment query and mutation hooks"
```
