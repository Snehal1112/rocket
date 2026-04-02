# Plan 0 — Environment Architecture Fix

> **For agentic workers:** Use `superpowers:subagent-driven-development`.
> Read `docs/superpowers/specs/variables-design.md` before starting.

**Depends on:** Nothing — run first.  
**Spec:** `docs/superpowers/specs/variables-design.md`

Every subsequent plan assumes environments are stored per-collection and `useEnvStore` is collection-scoped.

**Goal:** Move environment file storage from workspace-level to per-collection. Split `FsEnvironmentRepo` into two usages (global vs collection). Make all Tauri env commands collection-scoped. Wire env reload to collection-change events.

**Must run first** — every other plan assumes collection-scoped environments.

---

## File Map

| File | Change |
|---|---|
| `src-tauri/src/lib.rs` | Add `CollectionsBasePath` state; remove singleton `EnvironmentService` |
| `src-tauri/src/commands/environments.rs` | Add `collection:` param to 4 commands + `env_service_for()` helper |
| `src/lib/tauri-api.ts` | Update 4 env wrappers to include `collection` |
| `src/store/env-store.ts` | Collection-scoped state + `loadEnvironments(collection)` |
| `src/components/layout/CollectionsSidebar.tsx` | Trigger env reload on collection click |
| `src/components/environments/EnvironmentsDialog.tsx` | Read from store, not direct API |
| `src/App.tsx` | Load envs on startup + workspace-switched event |

---

## Chunk 1: Backend — collection-scoped env commands

### Task 1: CollectionsBasePath + updated env commands

**Files:** `src-tauri/src/lib.rs`, `src-tauri/src/commands/environments.rs`

- [ ] **Step 1: Add CollectionsBasePath state**

In `src-tauri/src/lib.rs`:

```rust
pub struct CollectionsBasePath(pub std::path::PathBuf);
```

In app setup block:
```rust
let collections_base = data_dir.join("collections");
app.manage(CollectionsBasePath(collections_base));
app.manage(Arc::clone(&event_publisher) as Arc<dyn EventPublisher + Send + Sync>);
// Remove: app.manage(env_svc);  ← delete this line
```

- [ ] **Step 2: Add env_service_for() helpers**

In `src-tauri/src/commands/environments.rs`:

```rust
fn env_service_for(
    collection: &str,
    collections_base: &std::path::Path,
    publisher: Arc<dyn EventPublisher + Send + Sync>,
) -> EnvironmentService {
    let env_dir = collections_base.join(collection).join("environments");
    std::fs::create_dir_all(&env_dir).ok();
    EnvironmentService::new(Box::new(FsEnvironmentRepo::new(env_dir)), publisher)
}

fn global_env_service(
    workspace_path: &std::path::Path,
    publisher: Arc<dyn EventPublisher + Send + Sync>,
) -> EnvironmentService {
    let env_dir = workspace_path.join("environments");
    std::fs::create_dir_all(&env_dir).ok();
    EnvironmentService::new(Box::new(FsEnvironmentRepo::new(env_dir)), publisher)
}
```

- [ ] **Step 3: Update all four env commands**

```rust
#[tauri::command]
pub fn list_environments(
    collection: String,
    base: tauri::State<CollectionsBasePath>,
    publisher: tauri::State<Arc<dyn EventPublisher + Send + Sync>>,
) -> Result<Vec<Environment>, String> {
    env_service_for(&collection, &base.0, Arc::clone(&publisher))
        .list().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_environment(
    collection: String, name: String,
    base: tauri::State<CollectionsBasePath>,
    publisher: tauri::State<Arc<dyn EventPublisher + Send + Sync>>,
) -> Result<Environment, String> {
    env_service_for(&collection, &base.0, Arc::clone(&publisher))
        .get(&name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_environment(
    collection: String, env: Environment,
    base: tauri::State<CollectionsBasePath>,
    publisher: tauri::State<Arc<dyn EventPublisher + Send + Sync>>,
) -> Result<(), String> {
    env_service_for(&collection, &base.0, Arc::clone(&publisher))
        .save(&env).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_environment(
    collection: String, name: String,
    base: tauri::State<CollectionsBasePath>,
    publisher: tauri::State<Arc<dyn EventPublisher + Send + Sync>>,
) -> Result<(), String> {
    env_service_for(&collection, &base.0, Arc::clone(&publisher))
        .delete(&name).map_err(|e| e.to_string())
}
```

- [ ] **Step 4: Build check**

```bash
cargo check --workspace 2>&1 | grep "^error" | head -20
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/
git commit -m "feat(tauri): env commands now collection-scoped via CollectionsBasePath"
```

---

## Chunk 2: Frontend store — collection-scoped

### Task 2: useEnvStore collection scoping

**Files:** `src/store/env-store.ts`

- [ ] **Step 1: Add activeCollection and update loadEnvironments**

```ts
activeCollection: string | null   // initialize to null

loadEnvironments: async (collection: string) => {
  set({ isLoading: true, activeCollection: collection });
  try {
    const envs = await listEnvironments(collection);
    set({ environments: envs, isLoading: false });
    const stored = localStorage.getItem(`rocket-api:active-env:${collection}`);
    set({ activeEnvId: envs.find(e => e.name === stored)?.name ?? null });
  } catch {
    set({ isLoading: false, environments: [] });
  }
},

setActiveEnv: (name: string | null) => {
  const { activeCollection } = get();
  set({ activeEnvId: name });
  if (activeCollection && name)
    localStorage.setItem(`rocket-api:active-env:${activeCollection}`, name);
  else if (activeCollection)
    localStorage.removeItem(`rocket-api:active-env:${activeCollection}`);
},

saveEnvironment: async (env: Environment) => {
  const { activeCollection } = get();
  if (!activeCollection) throw new Error('No active collection');
  await saveEnvironment(activeCollection, env);
  await get().loadEnvironments(activeCollection);
},

deleteEnvironment: async (name: string) => {
  const { activeCollection } = get();
  if (!activeCollection) throw new Error('No active collection');
  await deleteEnvironment(activeCollection, name);
  await get().loadEnvironments(activeCollection);
},
```

- [ ] **Step 2: Tests**

```ts
it('loadEnvironments sets activeCollection', async () => {
  vi.mocked(listEnvironments).mockResolvedValue([{ name: 'staging', variables: [] }]);
  await useEnvStore.getState().loadEnvironments('my-api');
  expect(useEnvStore.getState().activeCollection).toBe('my-api');
});

it('setActiveEnv scopes localStorage key', () => {
  useEnvStore.setState({ activeCollection: 'my-api' });
  useEnvStore.getState().setActiveEnv('staging');
  expect(localStorage.getItem('rocket-api:active-env:my-api')).toBe('staging');
});
```

- [ ] **Step 3: Commit**

```bash
git add src/store/env-store.ts src/store/__tests__/env-store.test.ts
git commit -m "feat(store): useEnvStore now collection-scoped"
```

---

## Chunk 3: API bridge + call sites

### Task 3: Update tauri-api.ts

- [ ] **Step 1: Update env wrappers**

```ts
export const listEnvironments = (collection: string) =>
  invoke<Environment[]>('list_environments', { collection });

export const getEnvironment = (collection: string, name: string) =>
  invoke<Environment>('get_environment', { collection, name });

export const saveEnvironment = (collection: string, env: Environment) =>
  invoke<void>('save_environment', { collection, env });

export const deleteEnvironment = (collection: string, name: string) =>
  invoke<void>('delete_environment', { collection, name });
```

- [ ] **Step 2: Find and fix all call sites**

```bash
grep -rn "listEnvironments(\|getEnvironment(\|saveEnvironment(\|deleteEnvironment(" \
  src/ --include="*.ts" --include="*.tsx"
```

For each result outside `tauri-api.ts`, add the active collection name as the first argument.

- [ ] **Step 3: TypeScript check + commit**

```bash
yarn tsc --noEmit 2>&1 | head -10
git add src/ && git commit -m "feat(api): env commands now collection-scoped"
```

---

## Chunk 4: Collection-change wiring + EnvironmentsDialog

### Task 4: Wire env reload to collection changes

**Files:** `src/App.tsx`, `src/components/layout/CollectionsSidebar.tsx`, `src/components/environments/EnvironmentsDialog.tsx`

- [ ] **Step 1: Reload on collection click**

In `CollectionsSidebar.tsx`, after `setActiveCollection(collection)`:
```ts
useEnvStore.getState().loadEnvironments(collection.name);
```

- [ ] **Step 2: Reload on workspace switch + startup**

In `App.tsx`:
```ts
// On startup:
const init = async () => {
  await fetchCollections();
  const active = useCollectionsStore.getState().activeCollection;
  if (active) await useEnvStore.getState().loadEnvironments(active.name);
  await useEnvStore.getState().loadProcessEnvVars();
  await useEnvStore.getState().fetchGlobalEnv();
};

// On workspace-switched event:
listen('workspace-switched', async () => {
  await fetchCollections();
  const first = useCollectionsStore.getState().collections[0];
  if (first) await useEnvStore.getState().loadEnvironments(first.name);
  await useEnvStore.getState().fetchGlobalEnv();
});
```

- [ ] **Step 3: Reload on file-watcher env changes**

In `onCollectionChanged` handler:
```ts
if (event.path?.includes('/environments/') && event.collection) {
  const { activeCollection } = useEnvStore.getState();
  if (event.collection === activeCollection)
    useEnvStore.getState().loadEnvironments(activeCollection);
}
```

- [ ] **Step 4: Clean up EnvironmentsDialog**

Replace direct `listEnvironments()` calls in `EnvironmentsDialog.tsx` with store reads:
```ts
const environments    = useEnvStore(s => s.environments);
const activeCollection = useEnvStore(s => s.activeCollection);
const saveEnv         = useEnvStore(s => s.saveEnvironment);
const deleteEnv       = useEnvStore(s => s.deleteEnvironment);
```

Add guard when no collection active:
```tsx
if (!activeCollection) return (
  <p className="text-sm text-muted-foreground p-4">
    Select a collection to manage its environments.
  </p>
);
```

- [ ] **Step 5: Run full suite + commit**

```bash
npx vitest run && cargo test --workspace
git add src/ && git commit -m "feat: reload envs on collection change; clean EnvironmentsDialog"
```

---
