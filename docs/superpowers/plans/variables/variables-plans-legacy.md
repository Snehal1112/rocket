# Variables — Implementation Plans

> **For agentic workers:** Use `superpowers:subagent-driven-development`.  
> Execute plans in order — each depends on the previous.

---

## Execution Order

```
Plan 0 → Plan 1 → Plan 2 → Plan 3 → Plan 4 → Plan 5
  Env      Rust    Backend  Frontend  Overlay  UI
 Arch Fix  Context Commands  Store    + Send  Editors
```

---

# Plan 0 — Environment Architecture Fix

**Goal:** Move environment file storage from workspace-level to per-collection. Split `FsEnvironmentRepo` into two usages (global vs collection). Make all Tauri env commands collection-scoped. Wire env reload to collection-change events.

**Must run first** — every other plan assumes collection-scoped environments.

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

# Plan 1 — VariableContext (Rust)

**Goal:** Add `VariableContext` struct to `rocket-environment` crate for merging all 7 scopes.

**Depends on:** Plan 0

## File Map

| File | Change |
|---|---|
| `crates/rocket-environment/src/context.rs` | New — VariableContext struct |
| `crates/rocket-environment/src/lib.rs` | Export VariableContext |

---

### Task 1: Implement VariableContext

- [ ] **Step 1: Write failing tests in context.rs**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn m(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test] fn env_beats_collection() {
        let ctx = VariableContext { env: m(&[("k","env")]), collection: m(&[("k","col")]), ..Default::default() };
        assert_eq!(ctx.flatten().get("k").unwrap(), "env");
    }
    #[test] fn folder_beats_env() {
        let ctx = VariableContext { folder: m(&[("k","folder")]), env: m(&[("k","env")]), ..Default::default() };
        assert_eq!(ctx.flatten().get("k").unwrap(), "folder");
    }
    #[test] fn request_beats_folder() {
        let ctx = VariableContext { request: m(&[("k","req")]), folder: m(&[("k","folder")]), ..Default::default() };
        assert_eq!(ctx.flatten().get("k").unwrap(), "req");
    }
    #[test] fn runtime_beats_all() {
        let ctx = VariableContext { runtime: m(&[("k","rt")]), request: m(&[("k","req")]), ..Default::default() };
        assert_eq!(ctx.flatten().get("k").unwrap(), "rt");
    }
    #[test] fn global_beats_nothing_but_process() {
        let ctx = VariableContext { global_env: m(&[("k","global")]), collection: m(&[("k","col")]), ..Default::default() };
        assert_eq!(ctx.flatten().get("k").unwrap(), "col"); // collection beats global
    }
    #[test] fn process_env_uses_dotted_key() {
        let ctx = VariableContext { process_env: m(&[("API_KEY","secret")]), ..Default::default() };
        let flat = ctx.flatten_with_process_env();
        assert!(flat.get("API_KEY").is_none());
        assert_eq!(flat.get("process.env.API_KEY").unwrap(), "secret");
    }
    #[test] fn env_beats_global() {
        let ctx = VariableContext { env: m(&[("t","env")]), global_env: m(&[("t","global")]), ..Default::default() };
        assert_eq!(ctx.flatten().get("t").unwrap(), "env");
    }
    #[test] fn empty_is_empty() {
        assert!(VariableContext::default().flatten().is_empty());
    }
    #[test] fn folder_chain_innermost_wins() {
        // folder field already contains merged result from backend chain walk
        let ctx = VariableContext { folder: m(&[("k","inner")]), env: m(&[("k","env")]), ..Default::default() };
        assert_eq!(ctx.flatten().get("k").unwrap(), "inner");
    }
}
```

- [ ] **Step 2: Run tests — expect FAIL**

```bash
cargo test -p rocket-environment -- context::tests 2>&1 | tail -5
```

- [ ] **Step 3: Implement**

```rust
// crates/rocket-environment/src/context.rs
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct VariableContext {
    pub runtime:     HashMap<String, String>,
    pub request:     HashMap<String, String>,
    pub folder:      HashMap<String, String>,
    pub env:         HashMap<String, String>,
    pub collection:  HashMap<String, String>,
    pub global_env:  HashMap<String, String>,
    pub process_env: HashMap<String, String>,
}

impl VariableContext {
    /// Merge all scopes. Insertion order: global → collection → env → folder → request → runtime.
    /// Each later layer overwrites earlier on collision. process_env excluded (use flatten_with_process_env).
    pub fn flatten(&self) -> HashMap<String, String> {
        let mut out = HashMap::new();
        out.extend(self.global_env.clone());
        out.extend(self.collection.clone());
        out.extend(self.env.clone());
        out.extend(self.folder.clone());
        out.extend(self.request.clone());
        out.extend(self.runtime.clone());
        out
    }

    /// Same as flatten() but also includes process.env.KEY prefixed entries (lowest priority).
    pub fn flatten_with_process_env(&self) -> HashMap<String, String> {
        let mut out = HashMap::new();
        for (k, v) in &self.process_env {
            out.insert(format!("process.env.{}", k), v.clone());
        }
        out.extend(self.global_env.clone());
        out.extend(self.collection.clone());
        out.extend(self.env.clone());
        out.extend(self.folder.clone());
        out.extend(self.request.clone());
        out.extend(self.runtime.clone());
        out
    }
}
```

- [ ] **Step 4: Export from lib.rs**

```rust
pub mod context;
pub use context::VariableContext;
```

- [ ] **Step 5: Run tests — expect PASS**

```bash
cargo test -p rocket-environment -- context::tests
cargo clippy -p rocket-environment
```

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-environment/src/context.rs crates/rocket-environment/src/lib.rs
git commit -m "feat(environment): VariableContext — merges all 7 scopes with correct priority"
```

---

# Plan 2 — Backend Commands

**Goal:** Add global env selection, process env, folder chain variables, and request variables Tauri commands. Implement full Rust logic for folder chain walk and `initialValue` fallback.

**Depends on:** Plan 1

## File Map

| File | Change |
|---|---|
| `crates/rocket-workspace/src/workspace.rs` | Add `global_environment: Option<String>` |
| `crates/rocket-app/src/workspace_service.rs` | Add get/set global env name |
| `crates/rocket-collection/src/collection.rs` | Add `initial_value` to `CollectionVariable` |
| `crates/rocket-infra/src/oc_conversions.rs` | Update conversion helpers |
| `crates/rocket-infra/src/fs_collection_repo.rs` | Implement folder chain + request var methods |
| `src-tauri/src/commands/environments.rs` | Global env + process env commands |
| `src-tauri/src/commands/collections.rs` | Folder chain + request var commands |
| `src-tauri/src/lib.rs` | Register all new commands |

---

## Chunk 1: Global env selection (workspace.yml)

### Task 1: Workspace model + service

- [ ] **Step 1: Add global_environment to WorkspaceConfig**

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub global_environment: Option<String>,
```

- [ ] **Step 2: Add get/set to WorkspaceService**

```rust
pub fn get_global_environment_name(&self) -> DomainResult<Option<String>> {
    Ok(self.repo.load_active_workspace_config()?.global_environment)
}

pub fn set_global_environment(&self, name: Option<String>) -> DomainResult<()> {
    if let Some(ref n) = name { self.env_repo.get(n)?; } // validate exists
    let mut config = self.repo.load_active_workspace_config()?;
    config.global_environment = name;
    self.repo.save_workspace_config(&config)
}
```

- [ ] **Step 3: Tests**

```rust
#[test] fn workspace_global_environment_roundtrip() {
    let yaml = "name: test\nglobalEnvironment: staging";
    let ws: WorkspaceConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(ws.global_environment, Some("staging".into()));
}
#[test] fn workspace_global_environment_defaults_none() {
    let ws: WorkspaceConfig = serde_yaml::from_str("name: test").unwrap();
    assert!(ws.global_environment.is_none());
}
```

- [ ] **Step 4: Tauri commands**

```rust
#[tauri::command]
pub fn get_global_environment_name(
    workspace_svc: tauri::State<WorkspaceService>,
) -> Result<Option<String>, String> {
    workspace_svc.get_global_environment_name().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_global_environment(
    name: Option<String>,
    workspace_svc: tauri::State<WorkspaceService>,
) -> Result<(), String> {
    workspace_svc.set_global_environment(name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_process_env_vars() -> std::collections::HashMap<String, String> {
    std::env::vars().collect()
}
```

- [ ] **Step 5: Register commands in lib.rs**

```
commands::environments::get_global_environment_name,
commands::environments::set_global_environment,
commands::environments::get_process_env_vars,
```

- [ ] **Step 6: Build + commit**

```bash
cargo test -p rocket-workspace && cargo check --workspace
git add crates/ src-tauri/
git commit -m "feat: global env selection + process env Tauri commands"
```

---

## Chunk 2: CollectionVariable initialValue (C2)

### Task 2: Add initial_value to Rust type + update conversions

- [ ] **Step 1: Update CollectionVariable struct**

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionVariable {
    pub key:           String,
    pub value:         String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub initial_value: String,   // fallback when value empty; committed to Git
    pub enabled:       bool,
    pub secret:        bool,
}
```

- [ ] **Step 2: Update oc_conversions.rs**

```rust
pub fn oc_variable_to_collection_variable(v: OcVariable) -> CollectionVariable {
    let val = v.value.as_ref().map(|vv| vv.as_str().to_string()).unwrap_or_default();
    CollectionVariable {
        key:           v.name,
        value:         val.clone(),
        initial_value: val,   // same source for now; can diverge with local overrides
        enabled:       !v.disabled.unwrap_or(false),
        secret:        false,
    }
}

pub fn collection_variable_to_oc_variable(cv: CollectionVariable) -> OcVariable {
    // Use value if set; fall back to initial_value
    let effective = if !cv.value.is_empty() { cv.value } else { cv.initial_value };
    OcVariable {
        name:        cv.key,
        value:       if effective.is_empty() { None } else { Some(VariableValue::simple(effective)) },
        description: None,
        disabled:    if cv.enabled { None } else { Some(true) },
    }
}
```

- [ ] **Step 3: Build check**

```bash
cargo check --workspace 2>&1 | grep "^error"
```

- [ ] **Step 4: Commit**

```bash
git add crates/
git commit -m "feat: add initial_value to CollectionVariable + update OC conversions"
```

---

## Chunk 3: Folder chain + request variable commands (C1 + C3)

### Task 3: FsCollectionRepo implementations + Tauri commands

- [ ] **Step 1: Implement get_folder_chain_variables in FsCollectionRepo**

```rust
fn get_folder_chain_variables(
    &self,
    collection: &str,
    request_path: &str,
) -> DomainResult<Vec<CollectionVariable>> {
    let collection_path = self.collection_path(collection);
    let path = std::path::Path::new(request_path);

    // Decompose "auth/oauth/refresh.yml" → ["auth", "oauth"]
    let dir_components: Vec<&str> = path
        .parent()
        .unwrap_or(std::path::Path::new(""))
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    // Walk outermost → innermost; inner folder overwrites outer on collision
    let mut merged: std::collections::HashMap<String, CollectionVariable> =
        std::collections::HashMap::new();

    let mut current = collection_path.clone();
    for segment in &dir_components {
        current = current.join(segment);
        let folder_yml = current.join("folder.yml");
        if folder_yml.exists() {
            if let Ok(content) = std::fs::read_to_string(&folder_yml) {
                if let Ok(info) = serde_yaml::from_str::<OcFolderInfo>(&content) {
                    if let Some(req) = info.request {
                        if let Some(vars) = req.variables {
                            for v in vars {
                                let cv = oc_variable_to_collection_variable(v);
                                if cv.enabled {
                                    merged.insert(cv.key.clone(), cv);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let mut result: Vec<CollectionVariable> = merged.into_values().collect();
    result.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(result)
}
```

- [ ] **Step 2: Tests for folder chain**

```rust
#[test]
fn get_folder_chain_merges_parent_folders() {
    let tmp = TempDir::new().unwrap();
    let repo = FsCollectionRepo::new(tmp.path().to_path_buf());
    repo.create("my-api").unwrap();

    // auth/folder.yml
    let auth_dir = tmp.path().join("my-api/auth");
    std::fs::create_dir_all(&auth_dir).unwrap();
    std::fs::write(auth_dir.join("folder.yml"),
        "name: auth\ntype: folder\nrequest:\n  variables:\n    - name: host\n      value: outer\n    - name: authBase\n      value: https://auth.example.com\n",
    ).unwrap();

    // auth/oauth/folder.yml — overrides "host"
    let oauth_dir = auth_dir.join("oauth");
    std::fs::create_dir_all(&oauth_dir).unwrap();
    std::fs::write(oauth_dir.join("folder.yml"),
        "name: oauth\ntype: folder\nrequest:\n  variables:\n    - name: host\n      value: inner\n    - name: oauthFlow\n      value: pkce\n",
    ).unwrap();

    let vars = repo.get_folder_chain_variables("my-api", "auth/oauth/refresh.yml").unwrap();
    let map: std::collections::HashMap<_, _> =
        vars.iter().map(|v| (v.key.as_str(), v.value.as_str())).collect();

    assert_eq!(map["host"], "inner", "inner folder overrides outer");
    assert_eq!(map["authBase"], "https://auth.example.com");
    assert_eq!(map["oauthFlow"], "pkce");
}

#[test]
fn get_folder_chain_root_request_returns_empty() {
    let tmp = TempDir::new().unwrap();
    let repo = FsCollectionRepo::new(tmp.path().to_path_buf());
    repo.create("my-api").unwrap();
    assert!(repo.get_folder_chain_variables("my-api", "get-users.yml").unwrap().is_empty());
}

#[test]
fn get_folder_chain_skips_disabled_vars() {
    let tmp = TempDir::new().unwrap();
    let repo = FsCollectionRepo::new(tmp.path().to_path_buf());
    repo.create("my-api").unwrap();
    let auth_dir = tmp.path().join("my-api/auth");
    std::fs::create_dir_all(&auth_dir).unwrap();
    std::fs::write(auth_dir.join("folder.yml"),
        "name: auth\ntype: folder\nrequest:\n  variables:\n    - name: skip\n      value: nope\n      disabled: true\n",
    ).unwrap();
    assert!(repo.get_folder_chain_variables("my-api", "auth/login.yml").unwrap().is_empty());
}
```

- [ ] **Step 3: Implement save_folder_variables + get/save_request_variables**

```rust
fn save_folder_variables(
    &self, collection: &str, folder_path: &str, vars: Vec<CollectionVariable>,
) -> DomainResult<()> {
    let yml = self.collection_path(collection).join(folder_path).join("folder.yml");
    let mut info: OcFolderInfo = if yml.exists() {
        serde_yaml::from_str(&std::fs::read_to_string(&yml)?).unwrap_or_default()
    } else {
        OcFolderInfo { name: folder_path.split('/').last().unwrap_or(folder_path).to_string(), ..Default::default() }
    };
    let oc_vars: Vec<OcVariable> = vars.into_iter().map(collection_variable_to_oc_variable).collect();
    info.request = Some(OcRequestDefaults {
        variables: if oc_vars.is_empty() { None } else { Some(oc_vars) },
        ..info.request.unwrap_or_default()
    });
    std::fs::create_dir_all(yml.parent().unwrap())?;
    std::fs::write(&yml, serde_yaml::to_string(&info).map_err(|e| DomainError::Internal(e.to_string()))?)?;
    Ok(())
}

fn get_request_variables(&self, collection: &str, request_path: &str) -> DomainResult<Vec<CollectionVariable>> {
    let path = self.collection_path(collection)
        .join(format!("{}.yml", request_path.trim_end_matches(".yml")));
    if !path.exists() { return Ok(Vec::new()); }
    let req: OcHttpRequest = serde_yaml::from_str(&std::fs::read_to_string(&path)?)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    Ok(req.runtime.map(|r| r.variables.into_iter().map(oc_variable_to_collection_variable).collect()).unwrap_or_default())
}

fn save_request_variables(&self, collection: &str, request_path: &str, vars: Vec<CollectionVariable>) -> DomainResult<()> {
    let path = self.collection_path(collection)
        .join(format!("{}.yml", request_path.trim_end_matches(".yml")));
    if !path.exists() { return Err(DomainError::NotFound(format!("Request '{}'", request_path))); }
    let mut req: OcHttpRequest = serde_yaml::from_str(&std::fs::read_to_string(&path)?)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    req.runtime = Some(OcHttpRequestRuntime {
        variables: vars.into_iter().map(collection_variable_to_oc_variable).collect(),
        ..req.runtime.unwrap_or_default()
    });
    std::fs::write(&path, serde_yaml::to_string(&req).map_err(|e| DomainError::Internal(e.to_string()))?)?;
    Ok(())
}
```

- [ ] **Step 4: Add to CollectionRepository trait**

```rust
fn get_folder_chain_variables(&self, collection: &str, request_path: &str) -> DomainResult<Vec<CollectionVariable>>;
fn save_folder_variables(&self, collection: &str, folder_path: &str, vars: Vec<CollectionVariable>) -> DomainResult<()>;
fn get_request_variables(&self, collection: &str, request_path: &str) -> DomainResult<Vec<CollectionVariable>>;
fn save_request_variables(&self, collection: &str, request_path: &str, vars: Vec<CollectionVariable>) -> DomainResult<()>;
```

- [ ] **Step 5: Tauri commands**

```rust
#[tauri::command]
pub fn get_folder_chain_variables(
    collection: String, request_path: String,
    collection_svc: tauri::State<CollectionService>,
) -> Result<Vec<CollectionVariable>, String> {
    collection_svc.get_folder_chain_variables(&collection, &request_path).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn save_folder_variables(
    collection: String, folder_path: String, variables: Vec<CollectionVariable>,
    collection_svc: tauri::State<CollectionService>,
) -> Result<(), String> {
    collection_svc.save_folder_variables(&collection, &folder_path, variables).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn get_request_variables(
    collection: String, request_path: String,
    collection_svc: tauri::State<CollectionService>,
) -> Result<Vec<CollectionVariable>, String> {
    collection_svc.get_request_variables(&collection, &request_path).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn save_request_variables(
    collection: String, request_path: String, variables: Vec<CollectionVariable>,
    collection_svc: tauri::State<CollectionService>,
) -> Result<(), String> {
    collection_svc.save_request_variables(&collection, &request_path, variables).map_err(|e| e.to_string())
}
```

- [ ] **Step 6: Register in lib.rs + run tests + commit**

```bash
cargo test --workspace
git add crates/ src-tauri/
git commit -m "feat: folder chain variables + request variables commands"
```

---

# Plan 3 — Frontend Store + Resolution Pipeline

**Goal:** Wire all 7 scopes into `buildVariableContext`. Update `execute-request.ts` to resolve all fields correctly including form bodies and header keys.

**Depends on:** Plan 2

## File Map

| File | Change |
|---|---|
| `src/lib/tauri-api.ts` | Add 5 new wrappers |
| `src/types/index.ts` | Add `initialValue?` to `CollectionVariable` |
| `src/store/env-store.ts` | Add global env + process env state |
| `src/lib/variable-context.ts` | New — `buildVariableContext` |
| `src/lib/__tests__/variable-context.test.ts` | New — 9 tests |
| `src/lib/execute-request.ts` | Full resolution pipeline |

---

## Chunk 1: Types + API bridge

### Task 1: Update CollectionVariable type + add wrappers

- [ ] **Step 1: Add initialValue to type**

In `src/types/index.ts`:
```ts
export interface CollectionVariable {
  key:           string
  value:         string
  initialValue?: string   // fallback when value empty; shared default in Git
  enabled:       boolean
  secret:        boolean
}
```

- [ ] **Step 2: Add new Tauri API wrappers**

```ts
// Global env (selection pointer)
export const getGlobalEnvironmentName = () =>
  invoke<string | null>('get_global_environment_name');
export const setGlobalEnvironment = (name: string | null) =>
  invoke<void>('set_global_environment', { name });

// Process env
export const getProcessEnvVars = () =>
  invoke<Record<string, string>>('get_process_env_vars');

// Folder variables — takes request_path; server walks full parent chain
export const getFolderChainVariables = (collection: string, requestPath: string) =>
  invoke<CollectionVariable[]>('get_folder_chain_variables', { collection, requestPath });
export const saveFolderVariables = (collection: string, folderPath: string, variables: CollectionVariable[]) =>
  invoke<void>('save_folder_variables', { collection, folderPath, variables });

// Request variables
export const getRequestVariables = (collection: string, requestPath: string) =>
  invoke<CollectionVariable[]>('get_request_variables', { collection, requestPath });
export const saveRequestVariables = (collection: string, requestPath: string, variables: CollectionVariable[]) =>
  invoke<void>('save_request_variables', { collection, requestPath, variables });
```

- [ ] **Step 3: Commit**

```bash
git add src/types/index.ts src/lib/tauri-api.ts
git commit -m "feat(api): add global env, process env, folder chain, request var wrappers"
```

---

## Chunk 2: useEnvStore global env + process env

### Task 2: Add global env + process env to store

- [ ] **Step 1: Add state + actions**

```ts
// State
globalEnvName:   string | null   // = null
globalEnv:       Environment | null  // = null
processEnvVars:  Record<string, string>  // = {}

// Actions
fetchGlobalEnv: async () => {
  const name = await getGlobalEnvironmentName();
  if (!name) { set({ globalEnvName: null, globalEnv: null }); return; }
  const { activeCollection } = get();
  if (activeCollection) {
    try {
      const env = await getEnvironment(activeCollection, name);
      set({ globalEnvName: name, globalEnv: env }); return;
    } catch {}
  }
  set({ globalEnvName: name, globalEnv: null });
},

setGlobalEnv: async (name: string | null) => {
  await setGlobalEnvironment(name);
  await useEnvStore.getState().fetchGlobalEnv();
},

loadProcessEnvVars: async () => {
  set({ processEnvVars: await getProcessEnvVars() });
},

getGlobalVariables: () => {
  const { globalEnv } = get();
  if (!globalEnv) return {};
  return Object.fromEntries(
    globalEnv.variables.filter(v => v.enabled).map(v => [v.key, v.value])
  );
},
```

- [ ] **Step 2: Tests**

```ts
it('getGlobalVariables returns enabled vars only', () => {
  useEnvStore.setState({ globalEnv: {
    name: 'shared', variables: [
      { key: 'A', value: 'a', enabled: true },
      { key: 'B', value: 'b', enabled: false },
    ]
  }});
  const vars = useEnvStore.getState().getGlobalVariables();
  expect(vars['A']).toBe('a');
  expect(vars['B']).toBeUndefined();
});
it('fetchGlobalEnv null clears state', async () => {
  vi.mocked(getGlobalEnvironmentName).mockResolvedValue(null);
  await useEnvStore.getState().fetchGlobalEnv();
  expect(useEnvStore.getState().globalEnv).toBeNull();
});
```

- [ ] **Step 3: Commit**

```bash
git add src/store/env-store.ts
git commit -m "feat(store): add global env + process env to useEnvStore"
```

---

## Chunk 3: buildVariableContext

### Task 3: Create variable-context.ts

- [ ] **Step 1: Write tests**

```ts
// src/lib/__tests__/variable-context.test.ts
import { buildVariableContext, resolveWithContext } from '../variable-context';

const cv = (key: string, value: string, initialValue?: string) =>
  ({ key, value, initialValue, enabled: true, secret: false });

describe('buildVariableContext', () => {
  it('env beats collection', () =>
    expect(buildVariableContext({ collectionVars: [cv('k','col')], envVars: { k:'env' } })['k']).toBe('env'));
  it('folder beats env', () =>
    expect(buildVariableContext({ folderVars: [cv('k','folder')], envVars: { k:'env' } })['k']).toBe('folder'));
  it('request beats folder', () =>
    expect(buildVariableContext({ requestVars: [cv('k','req')], folderVars: [cv('k','folder')] })['k']).toBe('req'));
  it('runtime beats request', () =>
    expect(buildVariableContext({ runtimeVars: { k:'rt' }, requestVars: [cv('k','req')] })['k']).toBe('rt'));
  it('collection beats global', () =>
    expect(buildVariableContext({ collectionVars: [cv('k','col')], globalVars: { k:'global' } })['k']).toBe('col'));
  it('env beats global', () =>
    expect(buildVariableContext({ envVars: { k:'env' }, globalVars: { k:'global' } })['k']).toBe('env'));
  it('process.env uses dotted key', () => {
    const ctx = buildVariableContext({ processEnvVars: { API:'secret' } });
    expect(ctx['API']).toBeUndefined();
    expect(ctx['process.env.API']).toBe('secret');
  });
  it('initialValue fallback when value empty', () =>
    expect(buildVariableContext({ collectionVars: [cv('k','','default')] })['k']).toBe('default'));
  it('disabled vars excluded', () => {
    const ctx = buildVariableContext({ collectionVars: [{ key:'k', value:'v', enabled:false, secret:false }] });
    expect(ctx['k']).toBeUndefined();
  });
});

describe('resolveWithContext', () => {
  it('resolves vars', () =>
    expect(resolveWithContext('{{a}}/{{b}}', { a:'x', b:'y' })).toBe('x/y'));
  it('leaves unknown as-is', () =>
    expect(resolveWithContext('{{miss}}', {})).toBe('{{miss}}'));
  it('handles whitespace in braces', () =>
    expect(resolveWithContext('{{ key }}', { key:'val' })).toBe('val'));
  it('resolves process.env.KEY', () =>
    expect(resolveWithContext('{{process.env.K}}', { 'process.env.K':'v' })).toBe('v'));
});
```

- [ ] **Step 2: Implement variable-context.ts**

```ts
// src/lib/variable-context.ts
import type { CollectionVariable } from '@/types';

const VAR_REGEX = /\{\{\s*([\w.]+)\s*\}\}/g;

function varsToMap(vars: CollectionVariable[]): Record<string, string> {
  const out: Record<string, string> = {};
  for (const v of vars) {
    if (!v.enabled || !v.key) continue;
    const val = v.value || v.initialValue || '';  // C2: initialValue fallback
    if (val) out[v.key] = val;
  }
  return out;
}

export function buildVariableContext(params: {
  runtimeVars?:    Record<string, string>
  requestVars?:    CollectionVariable[]
  folderVars?:     CollectionVariable[]   // already chain-merged by backend
  collectionVars?: CollectionVariable[]
  envVars?:        Record<string, string>
  globalVars?:     Record<string, string>
  processEnvVars?: Record<string, string>
}): Record<string, string> {
  const ctx: Record<string, string> = {};
  // Lowest priority first — each layer overwrites on collision
  for (const [k, v] of Object.entries(params.processEnvVars ?? {}))
    ctx[`process.env.${k}`] = v;
  Object.assign(ctx, params.globalVars ?? {});
  Object.assign(ctx, varsToMap(params.collectionVars ?? []));
  Object.assign(ctx, params.envVars ?? {});           // env beats collection
  Object.assign(ctx, varsToMap(params.folderVars ?? []));
  Object.assign(ctx, varsToMap(params.requestVars ?? []));
  Object.assign(ctx, params.runtimeVars ?? {});       // runtime wins all
  return ctx;
}

export function resolveWithContext(template: string, ctx: Record<string, string>): string {
  return template.replace(VAR_REGEX, (match, key) => key in ctx ? ctx[key] : match);
}

export function resolveMapWithContext(
  map: Record<string, string>, ctx: Record<string, string>
): Record<string, string> {
  return Object.fromEntries(Object.entries(map).map(([k, v]) => [k, resolveWithContext(v, ctx)]));
}
```

- [ ] **Step 3: Run tests + commit**

```bash
npx vitest run src/lib/__tests__/variable-context.test.ts
git add src/lib/variable-context.ts src/lib/__tests__/variable-context.test.ts
git commit -m "feat: buildVariableContext — merges all 7 scopes"
```

---

## Chunk 4: execute-request.ts — full resolution

### Task 4: Replace resolveVariables with full pipeline

- [ ] **Step 1: Build context**

Replace the existing resolution setup in `sendRequest`:

```ts
import { buildVariableContext, resolveWithContext } from '@/lib/variable-context';
import { getFolderChainVariables, getRequestVariables } from '@/lib/tauri-api';

const envStore       = useEnvStore.getState();
const envVars        = envStore.getActiveVariables();
const globalVars     = envStore.getGlobalVariables();
const processEnvVars = envStore.processEnvVars;
const collectionVars = useCollectionsStore.getState().collectionVariables ?? [];
const collection     = found?.tab.source?.collection;

// C1: server walks full parent folder chain — just pass request_path
let folderVars: CollectionVariable[] = [];
if (collection && request.path) {
  try { folderVars = await getFolderChainVariables(collection, request.path); } catch {}
}

let requestVars: CollectionVariable[] = [];
if (collection && request.path) {
  try { requestVars = await getRequestVariables(collection, request.path); } catch {}
}

const ctx = buildVariableContext({ processEnvVars, globalVars, envVars, collectionVars, folderVars, requestVars });
const resolve = (text: string) => resolveWithContext(text, ctx);
```

- [ ] **Step 2: Resolve all fields**

```ts
const resolvedUrl          = resolve(request.url);
const resolvedHeaders      = request.headers
  .filter(h => h.enabled)
  .map(h => ({ ...h, key: resolve(h.key), value: resolve(h.value) }));  // both key AND value
const resolvedQueryParams  = (request.queryParams ?? []).filter(p => p.enabled)
  .map(p => ({ ...p, value: resolve(p.value) }));
const resolvedPathParams   = (request.pathParams ?? []).filter(p => p.enabled)
  .map(p => ({ ...p, value: resolve(p.value) }));
const resolvedAuth         = resolveAuthFields(request.auth, resolve);
const resolvedBody         = resolveBody(request.body, resolve);
```

- [ ] **Step 3: Add resolveAuthFields helper**

```ts
function resolveAuthFields(auth: RequestAuth | undefined, resolve: (s: string) => string) {
  if (!auth) return auth;
  switch (auth.type) {
    case 'bearer':  return { ...auth, token: resolve(auth.token ?? '') };
    case 'basic':   return { ...auth, username: resolve(auth.username ?? ''), password: resolve(auth.password ?? '') };
    case 'apikey':  return { ...auth, key: resolve(auth.key ?? ''), value: resolve(auth.value ?? '') };
    case 'oauth2':  return { ...auth,
      clientId: resolve(auth.clientId ?? ''), clientSecret: resolve(auth.clientSecret ?? ''),
      accessTokenUrl: resolve(auth.accessTokenUrl ?? ''), authorizationUrl: resolve(auth.authorizationUrl ?? ''),
      scope: resolve(auth.scope ?? '') };
    case 'awsv4':   return { ...auth,
      accessKeyId: resolve(auth.accessKeyId ?? ''), secretAccessKey: resolve(auth.secretAccessKey ?? ''),
      region: resolve(auth.region ?? ''), service: resolve(auth.service ?? '') };
    default: return auth;
  }
}
```

- [ ] **Step 4: Add resolveBody helper (C3)**

```ts
function resolveBody(body: RequestBody | undefined, resolve: (s: string) => string) {
  if (!body) return body;
  switch (body.mode) {
    case 'json': case 'xml': case 'text': case 'sparql': case 'graphql':
      return { ...body, content: resolve(body.content ?? '') };
    case 'formUrlEncoded': case 'multipart':
      // C3: resolve each field value individually — NOT as a single string
      return { ...body, params: (body.params ?? []).map(p =>
        p.enabled ? { ...p, value: resolve(p.value) } : p) };
    default: return body;  // binary/file: not interpolated
  }
}
```

- [ ] **Step 5: Tests**

```ts
it('resolves form-urlencoded field values individually', async () => {
  // setup env with TOKEN=abc
  const request = makeRequest({ body: { mode: 'formUrlEncoded',
    params: [{ key: 'token', value: '{{TOKEN}}', enabled: true }] }});
  const sent = await captureRequest(() => sendRequest('t1', request));
  expect(sent.body.params[0].value).toBe('abc');
});

it('resolves both header key and value', async () => {
  const request = makeRequest({ headers: [{ key: '{{HNAME}}', value: '{{HVAL}}', enabled: true }] });
  const sent = await captureRequest(() => sendRequest('t1', request));
  expect(sent.headers[0].key).toBe('X-My-Header');
  expect(sent.headers[0].value).toBe('my-value');
});

it('uses initialValue when value is empty', async () => {
  useCollectionsStore.setState({ collectionVariables: [
    { key: 'base', value: '', initialValue: 'https://api.example.com', enabled: true, secret: false }
  ]});
  const request = makeRequest({ url: '{{base}}/users' });
  const sent = await captureRequest(() => sendRequest('t1', request));
  expect(sent.url).toBe('https://api.example.com/users');
});
```

- [ ] **Step 6: Run full suite + commit**

```bash
npx vitest run && yarn tsc --noEmit
git add src/lib/execute-request.ts
git commit -m "feat: full variable resolution pipeline in execute-request.ts"
```

---

# Plan 4 — URL Overlay + Environment Switcher

**Goal:** Scope-coloured badges in `VariableAwareUrlInput`. Two-section `EnvironmentSwitcher`. Verify all fields resolve.

**Depends on:** Plan 3

## File Map

| File | Change |
|---|---|
| `src/lib/url-variables.ts` | Add `VariableSource`, `VariableScopeEntry`, `buildScopedContext`, `sourceBadgeClass` |
| `src/components/request/VariableAwareUrlInput.tsx` | Accept `scopedContext`, render badges |
| `src/components/request/RequestPanel.tsx` | Build + pass `scopedContext` |
| `src/components/layout/EnvironmentSwitcher.tsx` | Two-section dropdown |

---

## Chunk 1: Scope-aware overlay

### Task 1: Update url-variables.ts

- [ ] **Step 1: Add types**

```ts
export type VariableSource = 'runtime'|'request'|'folder'|'environment'|'collection'|'global'|'process'

export interface VariableScopeEntry {
  value:  string
  source: VariableSource
  label:  string    // "Staging", "Collection", "Folder", etc.
  secret: boolean   // true → show ●●●● in tooltip
}

export interface UrlToken {
  type: 'text' | 'variable'
  value: string       // var name without braces, or raw text
  start: number
  end: number
  resolved?: string
  source?: VariableSource
  sourceLabel?: string
  secret?: boolean
}
```

- [ ] **Step 2: Add buildScopedContext**

```ts
export function buildScopedContext(params: {
  runtimeVars?:    Record<string, string>
  requestVars?:    CollectionVariable[]
  folderVars?:     CollectionVariable[]
  collectionVars?: CollectionVariable[]
  envVars?:        Record<string, string>
  envLabel?:       string
  globalVars?:     Record<string, string>
  processEnvVars?: Record<string, string>
}): Map<string, VariableScopeEntry> {
  const out = new Map<string, VariableScopeEntry>();
  const add = (k: string, v: string, source: VariableSource, label: string, secret = false) =>
    out.set(k, { value: v, source, label, secret });

  for (const [k, v] of Object.entries(params.processEnvVars ?? {}))
    add(`process.env.${k}`, v, 'process', 'Process Env');
  for (const [k, v] of Object.entries(params.globalVars ?? {}))
    add(k, v, 'global', 'Global');
  for (const [k, v] of Object.entries(params.envVars ?? {}))
    add(k, v, 'environment', params.envLabel ?? 'Environment');
  for (const v of (params.collectionVars ?? []).filter(v => v.enabled)) {
    const val = v.value || v.initialValue || '';
    if (val) add(v.key, val, 'collection', 'Collection', v.secret);
  }
  for (const v of (params.folderVars ?? []).filter(v => v.enabled)) {
    const val = v.value || v.initialValue || '';
    if (val) add(v.key, val, 'folder', 'Folder', v.secret);
  }
  for (const v of (params.requestVars ?? []).filter(v => v.enabled)) {
    const val = v.value || v.initialValue || '';
    if (val) add(v.key, val, 'request', 'Request', v.secret);
  }
  for (const [k, v] of Object.entries(params.runtimeVars ?? {}))
    add(k, v, 'runtime', 'Runtime');
  return out;
}
```

- [ ] **Step 3: Add sourceBadgeClass**

```ts
export function sourceBadgeClass(source: VariableSource): string {
  const classes: Record<VariableSource, string> = {
    runtime:     'bg-orange-500/15 text-orange-700 dark:text-orange-400',
    request:     'bg-purple-500/15 text-purple-700 dark:text-purple-400',
    folder:      'bg-amber-500/15 text-amber-700 dark:text-amber-400',
    environment: 'bg-primary/15 text-primary',
    collection:  'bg-blue-500/15 text-blue-700 dark:text-blue-400',
    global:      'bg-teal-500/15 text-teal-700 dark:text-teal-400',
    process:     'bg-muted text-muted-foreground',
  };
  return classes[source];
}
```

- [ ] **Step 4: Commit**

```bash
git add src/lib/url-variables.ts
git commit -m "feat: add VariableSource + buildScopedContext + scope badges to url-variables"
```

---

### Task 2: Update VariableAwareUrlInput + RequestPanel

- [ ] **Step 1: Add scopedContext prop to VariableAwareUrlInput**

```tsx
interface VariableAwareUrlInputProps {
  value: string
  onChange: (value: string) => void
  onKeyDown?: React.KeyboardEventHandler<HTMLInputElement>
  onCurlImport?: (parsed: ParsedCurl) => void
  scopedContext?: Map<string, VariableScopeEntry>
  // legacy props kept for backward compat
  collectionVariables?: Record<string, string>
  pathParams?: Record<string, string>
  queryParams?: Record<string, string>
  placeholder?: string
  className?: string
}
```

- [ ] **Step 2: Use scopedContext in token rendering**

When rendering variable token in overlay, look up from `scopedContext`:
```tsx
const entry = scopedContext?.get(token.value);
const resolvedValue = entry?.secret ? '●●●●' : (entry?.value ?? '');
const badgeClass = entry ? sourceBadgeClass(entry.source) : 'bg-destructive/15 text-destructive';
const label = entry?.sourceLabel ?? 'Unresolved';
```

- [ ] **Step 3: Wire in RequestPanel**

```tsx
const scopedContext = useMemo(() => buildScopedContext({
  envVars, envLabel: activeEnvId ?? undefined,
  globalVars, processEnvVars, collectionVars: collectionVariables ?? [],
  folderVars, requestVars,
}), [envVars, activeEnvId, globalVars, processEnvVars, collectionVariables, folderVars, requestVars]);
```

Pass `scopedContext` to `<VariableAwareUrlInput>`.

- [ ] **Step 4: Commit**

```bash
git add src/components/request/
git commit -m "feat: scope badges in VariableAwareUrlInput"
```

---

## Chunk 2: Environment Switcher

### Task 3: Two-section dropdown

**File:** `src/components/layout/EnvironmentSwitcher.tsx`

- [ ] **Step 1: Restructure dropdown**

```tsx
const globalEnvName = useEnvStore(s => s.globalEnvName);
const setGlobalEnv  = useEnvStore(s => s.setGlobalEnv);
const activeEnvId   = useEnvStore(s => s.activeEnvId);
const setActiveEnv  = useEnvStore(s => s.setActiveEnv);
const environments  = useEnvStore(s => s.environments);

// Dropdown structure:
// ── Global section ──────────────────────────────
//   No Global Environment  ← deselect option
//   <env> ✓  (one per env file)
// ── separator ────────────────────────────────────
// ── Environment section ──────────────────────────
//   No Environment  ← deselect option
//   <env> ✓  (one per env file)
// ── separator ────────────────────────────────────
//   Manage Environments → opens EnvironmentsDialog
```

Global section selects use `setGlobalEnv(name)`. Environment section uses `setActiveEnv(name)`. Both sections show the same list of env files.

- [ ] **Step 2: Update trigger button**

```tsx
<Button variant="ghost" size="sm" className="h-7 gap-1.5 px-2 text-xs">
  {globalEnvName && (
    <span className="h-2 w-2 rounded-full bg-teal-500 shrink-0"
      title={`Global: ${globalEnvName}`} />
  )}
  <span className={!activeEnvId ? 'text-muted-foreground' : ''}>
    {activeEnvId ?? 'No Environment'}
  </span>
  <ChevronDown className="h-3 w-3 opacity-50" />
</Button>
```

- [ ] **Step 3: Commit**

```bash
git add src/components/layout/EnvironmentSwitcher.tsx
git commit -m "feat: split EnvironmentSwitcher into Global + Environment sections"
```

---

# Plan 5 — UI Editors for Folder + Request Variables

**Goal:** Add `FolderVariablesPopover` (sidebar context menu) and `RequestVariablesPanel` (new Variables tab in request editor).

**Depends on:** Plan 4

## File Map

| File | Change |
|---|---|
| `src/components/collections/FolderVariablesPopover.tsx` | New |
| `src/components/layout/CollectionsSidebar.tsx` | Add "Variables" to folder menu |
| `src/components/request/RequestVariablesPanel.tsx` | New |
| `src/components/request/RequestPanel.tsx` | Add Variables tab |

---

## Chunk 1: Folder variables editor

### Task 1: FolderVariablesPopover

- [ ] **Step 1: Create component**

```tsx
// Props
interface FolderVariablesPopoverProps {
  open: boolean
  onClose: () => void
  collection: string
  folderPath: string    // e.g. "auth" or "auth/oauth" — immediate parent only
  folderName: string
}
```

On open: call `getFolderChainVariables(collection, `${folderPath}/placeholder`)` to show inherited vars (read-only), and load this folder's own vars separately for editing.

Show two sections:
1. **This folder's variables** — editable table (key / value / enabled / delete)
2. **Inherited from parent folders** — read-only, greyed out, shows which folder each comes from

Save button calls `saveFolderVariables(collection, folderPath, vars)`.

All interactive elements must use shadcn/ui (`Dialog`, `Input`, `Switch`, `Button`).

- [ ] **Step 2: Wire into CollectionsSidebar**

Add to folder `DropdownMenuContent`:
```tsx
<DropdownMenuItem onSelect={() => { setFolderVarsTarget({collection, path, name}); setFolderVarsOpen(true); }}>
  <Variable className="h-3.5 w-3.5 mr-2" />
  Variables
</DropdownMenuItem>
```

- [ ] **Step 3: Commit**

```bash
git add src/components/collections/FolderVariablesPopover.tsx src/components/layout/CollectionsSidebar.tsx
git commit -m "feat: FolderVariablesPopover + sidebar wiring"
```

---

## Chunk 2: Request variables editor

### Task 2: RequestVariablesPanel + Variables tab

- [ ] **Step 1: Create RequestVariablesPanel**

```tsx
interface RequestVariablesPanelProps {
  collection: string
  requestPath: string
}
```

On mount: call `getRequestVariables(collection, requestPath)`.

Show:
- Description: "Request variables are available to this request only. They have higher priority than folder, environment, and collection variables."
- Variable table: key / value / enabled toggle / delete
- "Add variable" button
- Auto-save on blur OR explicit Save button (match collection variables tab UX)

- [ ] **Step 2: Add Variables tab to RequestPanel**

```tsx
<TabsTrigger value="variables">
  Variables
  {requestVarCount > 0 && (
    <span className="ml-1.5 text-[10px] bg-muted px-1 rounded">{requestVarCount}</span>
  )}
</TabsTrigger>

<TabsContent value="variables">
  {tab.source?.collection && tab.source?.path ? (
    <RequestVariablesPanel
      collection={tab.source.collection}
      requestPath={tab.source.path}
    />
  ) : (
    <p className="p-4 text-sm text-muted-foreground">
      Save this request to a collection before adding request variables.
    </p>
  )}
</TabsContent>
```

- [ ] **Step 3: Final smoke test**

```bash
cargo tauri dev
```

- [ ] Create env in collection A → verify file at `collections/A/environments/`
- [ ] Switch collection → env list changes
- [ ] Add folder variable at `auth/` → use `{{folderVar}}` in request inside `auth/oauth/` → verify it resolves
- [ ] Request at `auth/oauth/refresh.yml` with parent folder vars → chain merged correctly
- [ ] Set `initialValue` on collection var, leave `value` empty → resolves to `initialValue`
- [ ] Form-urlencoded body with `{{TOKEN}}` in a field value → field resolved, not whole body string
- [ ] Header `{{HEADER_NAME}}: {{HEADER_VALUE}}` → both key and value resolved
- [ ] URL overlay shows correct badge colour per scope; secret vars show `●●●●`
- [ ] Global env selection persists across collection switches

- [ ] **Step 4: Commit**

```bash
git add src/components/request/
git commit -m "feat: RequestVariablesPanel + Variables tab in request editor"
git commit -m "chore: variables system complete — all 7 scopes wired"
```
