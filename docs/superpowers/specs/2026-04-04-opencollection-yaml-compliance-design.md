# OpenCollection YAML Compliance — Design Spec

**Date:** 2026-04-04  
**Scope:** Full alignment of all Rocket-produced YAML files with OpenCollection 1.0.0 spec + Bruno interchange

---

## Problem Statement

Rocket writes three categories of YAML files to disk. Only request and folder files are currently spec-compliant. The remaining files use either a wrong version string, domain-native field names, or a flat structure that diverges from what the spec and Bruno expect.

This spec covers what each file should look like and how to achieve it without breaking existing workspaces (approach B: dual-read old format, always write new format).

---

## Scope

| File | Current state | Action needed |
|---|---|---|
| `opencollection.yml` | version `"0.1"` (wrong) | Fix version to `"1.0.0"` |
| `workspace.yml` | flat `name`/`description`, `type: embedded/external` | Add `info.name`, `docs`, drop `type` discriminant |
| `<env>.yml` (environments) | domain field names (`key`, `enabled`) | Use spec field names (`name`, `disabled`) via `OcEnvironment` |
| `folder.yml` | ✅ spec-compliant via `OcFolderInfo` | No change |
| Request `.yml` files | ✅ spec-compliant via `OcHttpRequest` etc. | No change |
| `_order.yml` | Rocket-internal, no spec equivalent | Out of scope |
| `workspaces.yml` | Rocket-internal, no spec equivalent | Out of scope |

---

## Section 1: opencollection.yml — Version String Fix

**Current output** (written by `collection_to_oc_collection` in `oc_conversions.rs`):
```yaml
opencollection: "0.1"
info:
  name: My Collection
```

**Target output:**
```yaml
opencollection: "1.0.0"
info:
  name: My Collection
```

**Change:** One line in `crates/rocket-infra/src/oc_conversions.rs`:
- `opencollection: Some("0.1".into())` → `opencollection: Some("1.0.0".into())`

**Backward compatibility:** Reading is not affected — the `opencollection` string is parsed but never validated against a specific version.

---

## Section 2: Environment Files — Spec Field Names

### Current YAML (written by `FsEnvironmentRepo::save`)

```yaml
name: Production
variables:
- key: BASE_URL
  value: https://api.example.com
  enabled: true
- key: API_KEY
  value: secret
  enabled: false
color: blue
```

The `variables` array uses domain field names: `key` (not `name`) and `enabled` (not `disabled: true`).

### Target YAML (spec-compliant)

```yaml
name: Production
color: blue
variables:
- name: BASE_URL
  value: https://api.example.com
- name: API_KEY
  value: secret
  disabled: true
```

- `name` field (not `key`)
- `disabled: true` for disabled variables (absent = enabled)

### How it works

The `OcEnvironment` struct in `opencollection.rs` already uses the correct spec field names.  
The `Environment ↔ OcEnvironment` conversions already exist in `oc_conversions.rs`.

**`FsEnvironmentRepo` changes:**

**`save()`** — convert to `OcEnvironment` before serializing:
```rust
let oc: OcEnvironment = env.clone().into();
let yaml = serde_yaml::to_string(&oc)?;
```

**`list()` and `get()`** — try new format first (approach B):
```rust
// Try OcEnvironment (new format: name/disabled)
if let Ok(oc) = serde_yaml::from_str::<OcEnvironment>(&content) {
    result.push(Environment::from(oc));
    continue;
}
// Fall back to domain Environment (old format: key/enabled)
if let Ok(env) = serde_yaml::from_str::<Environment>(&content) {
    result.push(env);
}
```

**Why the try-chain works:** `OcVariable.name` is a required non-defaulted `String`. Old files have `key: BASE_URL` with no `name` field — deserialization fails with "missing field 'name'", triggering the fallback. New files have `name: BASE_URL` — succeeds immediately.

---

## Section 3: workspace.yml — New Format

### Current YAML

```yaml
name: Sage Network
description: Optional description
collections:
- name: my-api
  type: embedded
- name: external-api
  type: external
  path: /absolute/path/to/api
environments:
  activeEnvironment: Production
globalEnvironment: Production
```

### Target YAML (spec-compatible, Bruno-interoperable)

```yaml
opencollection: "1.0.0"
info:
  name: Sage Network
  type: workspace
collections:
- name: my-api
  path: collections/my-api
- name: external-api
  path: /absolute/path/to/api
docs: Optional description
environments:
  activeEnvironment: Production
globalEnvironment: Production
```

**Key mapping decisions:**

| Old field | New field | Notes |
|---|---|---|
| `name` | `info.name` | Moved under `info` block |
| `description` | `docs` | Renamed to match spec |
| `collections[*].type` | (dropped) | Inferred from `path.is_absolute()` |
| `collections[*].path` | `collections[*].path` | Embedded = `collections/<name>` relative; External = absolute path |
| `environments.*` | `environments.*` | Unchanged |
| `globalEnvironment` | `globalEnvironment` | Rocket extension at root, other tools ignore unknown fields |

**Embedded collection path convention:** Embedded collections → `collections/<name>` (relative). This matches Bruno's convention and allows other tools to locate collections.

### New serde structs (in `opencollection.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcWorkspaceInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub workspace_type: Option<String>,  // "workspace"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcWorkspaceCollectionRef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcWorkspaceEnvironments {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_environment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcWorkspaceConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opencollection: Option<String>,   // "1.0.0"
    pub info: OcWorkspaceInfo,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collections: Vec<OcWorkspaceCollectionRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environments: Option<OcWorkspaceEnvironments>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global_environment: Option<String>,
}
```

### Conversions (in `oc_conversions.rs`)

```rust
impl From<OcWorkspaceCollectionRef> for CollectionReference {
    fn from(r: OcWorkspaceCollectionRef) -> Self {
        match r.path {
            Some(p) if p.is_absolute() => CollectionReference {
                name: r.name,
                ref_type: CollectionRefType::External,
                path: Some(p),
            },
            _ => CollectionReference {
                name: r.name,
                ref_type: CollectionRefType::Embedded,
                path: None,
            },
        }
    }
}

impl From<CollectionReference> for OcWorkspaceCollectionRef {
    fn from(r: CollectionReference) -> Self {
        OcWorkspaceCollectionRef {
            path: match r.ref_type {
                CollectionRefType::Embedded => Some(PathBuf::from(format!("collections/{}", r.name))),
                CollectionRefType::External => r.path,
            },
            name: r.name,
        }
    }
}

impl From<OcWorkspaceConfig> for WorkspaceConfig {
    fn from(oc: OcWorkspaceConfig) -> Self {
        WorkspaceConfig {
            name: oc.info.name,
            description: oc.docs,
            collections: oc.collections.into_iter().map(CollectionReference::from).collect(),
            environments: WorkspaceEnvironmentsConfig {
                active_environment: oc.environments.and_then(|e| e.active_environment),
            },
            global_environment: oc.global_environment,
        }
    }
}

impl From<WorkspaceConfig> for OcWorkspaceConfig {
    fn from(w: WorkspaceConfig) -> Self {
        let has_active_env = w.environments.active_environment.is_some();
        OcWorkspaceConfig {
            opencollection: Some("1.0.0".into()),
            info: OcWorkspaceInfo {
                name: w.name,
                workspace_type: Some("workspace".into()),
            },
            collections: w.collections.into_iter().map(OcWorkspaceCollectionRef::from).collect(),
            docs: w.description,
            environments: if has_active_env {
                Some(OcWorkspaceEnvironments {
                    active_environment: w.environments.active_environment,
                })
            } else {
                None
            },
            global_environment: w.global_environment,
        }
    }
}
```

### `FsWorkspaceConfigRepo` changes

**Dual-read in `load()`** — `OcWorkspaceConfig.info` is a required field; old format has no `info` key, so deserialization fails → fallback to old `WorkspaceConfig` serde:

```rust
// Try new format (OcWorkspaceConfig — has info.name)
if let Ok(oc) = serde_yaml::from_str::<OcWorkspaceConfig>(&content) {
    return Ok(WorkspaceConfig::from(oc));
}
// Fall back to old format (WorkspaceConfig — has name at root)
serde_yaml::from_str::<WorkspaceConfig>(&content)
    .map_err(|e| DomainError::InvalidInput(format!("Failed to parse workspace.yml: {e}")))
```

**Always write new format in `save()`:**
```rust
let oc = OcWorkspaceConfig::from(config.clone());
let content = serde_yaml::to_string(&oc)?;
```

### Why the try-chain works

| File format | `OcWorkspaceConfig` try | `WorkspaceConfig` try |
|---|---|---|
| New (`info: { name: ... }`) | ✅ Succeeds (has `info`) | Would fail (no `name` at root) |
| Old (`name: ...` at root) | ❌ Fails (no `info` key) | ✅ Succeeds (has `name`) |

Both structs have their key discriminator field as required/non-defaulted, so one exactly succeeds per format.

---

## File Map

| File | Change |
|---|---|
| `crates/rocket-infra/src/opencollection.rs` | Add `OcWorkspaceInfo`, `OcWorkspaceCollectionRef`, `OcWorkspaceEnvironments`, `OcWorkspaceConfig` structs |
| `crates/rocket-infra/src/oc_conversions.rs` | Fix `"0.1"` → `"1.0.0"`; add `WorkspaceConfig ↔ OcWorkspaceConfig` and `CollectionReference ↔ OcWorkspaceCollectionRef` From impls |
| `crates/rocket-infra/src/fs_workspace_config_repo.rs` | Use `OcWorkspaceConfig` for save; dual-read in load |
| `crates/rocket-infra/src/fs_environment_repo.rs` | Use `OcEnvironment` for save; dual-read in list/get |

No changes to domain crates (`rocket-workspace`, `rocket-environment`).
