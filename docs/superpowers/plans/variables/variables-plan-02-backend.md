# Plan 2 — Backend Commands

> **For agentic workers:** Use `superpowers:subagent-driven-development`.
> Read `docs/superpowers/specs/variables-design.md` before starting.

**Depends on:** Plan 1  
**Spec:** `docs/superpowers/specs/variables-design.md`

Covers three critical implementation gaps:
- **C1** — Folder chain walk (full parent `folder.yml` inheritance, innermost wins)
- **C2** — `initialValue` fallback in `CollectionVariable` (value || initialValue)
- **C3** — Rust-side OC conversion helpers for structured body types

**Goal:** Add global env selection, process env, folder chain variables, and request variables Tauri commands. Implement full Rust logic including folder chain walk and `initialValue` fallback.

---

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
