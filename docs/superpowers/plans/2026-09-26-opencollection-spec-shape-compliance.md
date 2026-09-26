# OpenCollection Spec Shape Compliance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the files Rocket writes (`folder.yml`, request `.yml` files, auth blocks) match the OpenCollection v1.0.0 schema shapes for the contained persistence-layer subset, keep every existing on-disk file loading, and add a regression test that fails on any key the schema rejects.

**Architecture:** All fixes live in the serde and persistence layer (`crates/rocket-infra/src/oc/*`, `conversions/*`, `fs_collection/*`) plus the domain PKCE struct in `crates/rocket-shared/src/oauth2.rs` and small frontend mapping changes. Every shape change uses the project's dual-read rule from `docs/superpowers/specs/2026-04-04-opencollection-yaml-compliance-design.md`: try the new (spec) shape first, fall back to the old shape on a `serde_yaml` error, with the two shapes told apart by a required field that only the new shape has. Rocket always writes the new shape. The final task adds a structural key check whose allow-lists are copied from the live schema.

**Tech Stack:** Rust (serde, serde_yaml, serde_json, tempfile), Tauri IPC, React + TypeScript (Vitest).

**Spec:** `docs/superpowers/specs/opencollection-spec-reference.md` (partly out of date, see "Doc corrections" below). Live schema: `https://schema.opencollection.com/opencollection/v1.0.0.json`.

## Global Constraints

- Every task opens with `📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.` (hard project rule from `CLAUDE.md` and `.claude/rules/*.md`).
- Where that doc conflicts with "Doc corrections (use these)" in the Deferred section below, trust this plan.
- No `unwrap()`/`expect()` in production code paths. Tests may use them, as the existing tests do.
- Dual-read: parse the new shape first and fall back to the old shape only on a `serde_yaml` error. Never write the old shape again.
- Never write `items` into `folder.yml` or `opencollection.yml`. Rocket uses the unbundled layout.
- `oc/*` structs spell fields exactly as the spec does. Several spec fields are camelCase, and `oc/auth.rs` already uses `#[serde(rename_all = "camelCase")]` for them. Follow that existing pattern. Do not add camelCase renames to any other persistence struct.
- Rust verification: `cargo check` (workspace root), `cargo test -p rocket-infra <filter>`, `cargo test -p rocket-shared <filter>`, and the full `cargo test -p rocket-infra` at the end of each Rust task.
- Run `cargo fmt` before each Rust commit. The code blocks in this plan are not guaranteed to be rustfmt-clean.
- Frontend verification: `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern>`. If `yarn check` reports only import order or formatting, fix it with `yarn lint` / `yarn format` and rerun.
- Commits: stage the files listed in the step, then invoke the `dev-workflow-skills:1-git-commit` skill (Skill tool). Do not run a freeform `git commit -m`. Each commit step gives the conventional-commit subject the skill should end up with.
- Only one frontend rule applies here: shadcn/ui primitives only. No task in this plan adds UI elements.

## Review Focus

These are five input classes that the spec implies but that no happy-path test covers. They are the ones most likely to hit a user first. Each has a test in the task that owns the code.

1. **A `folder.yml` in the new shape must be understood by every reader, including `read_uid_from_yaml`.** If one reader still expects the bare shape, it fails to parse, falls back to generating a fresh uid, and the folder gets a new identity on every load, which breaks tab dedup. Tests: Task 1, `folder_uid_is_stable_across_reloads`, plus `legacy_bare_folder_yml_still_loads_and_is_upgraded_on_write` (a legacy file with folder variables must still feed `get_folder_chain_variables`).
2. **A broken HTTP request file must not be misread as another item kind.** `OcItem` is untagged and `OcFolder` needs only `info`, so an HTTP file missing `http.method` matches `OcItem::Folder`. A naive dispatch would turn it into a phantom folder. Test: Task 2, `build_folder_tree_skips_http_file_missing_method_instead_of_misreading_it`.
3. **Old PKCE blocks with `enabled: false`.** This is the only case where the field inversion changes behavior. Such blocks exist in request YAML and in domain JSON (IPC payloads and history entries). Tests: Task 3, `pkce_reads_legacy_enabled_field` (JSON, domain type) and `oc_auth_oauth2_pkce_spec_and_legacy_fields` (YAML, persistence type).
4. **A hand-written or Bruno-exported `client_credentials` block with no `clientSecret` key.** Today `client_secret: String` is required, so the whole request file fails to parse and vanishes from the tree. Test: Task 6, `oauth2_client_credentials_without_client_secret_key_parses`.
5. **Collection-level "No Auth" and "Inherit" across save and reload.** "No Auth" must leave no `auth` key and no empty `request:` block. "Inherit" must come back as inherit and must not collapse to "none" in the collection overview. Tests: Task 5, `save_settings_omits_none_auth_instead_of_writing_type_none` and `save_settings_writes_inherit_as_spec_string` (Rust), plus the `fromPersistedAuth` inherit tests (TypeScript).

---

## Deferred — Separate Plans Needed

**Moved out of scope while writing this plan (the original brief listed it as item 5):**

- **Moving `uid` (HttpRequest, FolderInfo), `verifySsl` (HttpRequestSettings), `externalSecrets` (Environment) and `initial` (Variable) under `extensions.rocketapi`.** The live schema (downloaded 2026-09-26) defines `extensions` **only on the collection root** (`opencollection.yml`). `HttpRequest`, `Folder`/`FolderInfo`, `Environment`, `Variable` and `HttpRequestSettings` all have `additionalProperties: false` and no `extensions` property. Writing `extensions.rocketapi.*` into those files would swap one rejected key for another. The only schema-legal homes for this data are the root `extensions` block in `opencollection.yml` (a path-keyed map that `move_item`/`rename_request`/`delete_*` must keep in sync) or a sidecar file the schema never sees. Either choice is a new persistence concept with rename and delete consistency rules, so it needs its own design. `initial` in particular drives the `value ?? initialValue` rule (spec reference §6). Until then, Task 7's regression test lists these keys in `KNOWN_DEFERRED` so it stays green without hiding new violations.

**Deferred per the original brief:**

- **OAuth1 auth (`type: oauth1`).** Needs a new domain `Auth` variant end to end (rocket-shared, the rocket-http executor, rocket-infra persistence), not just a serde field. `AuthOAuth1` does exist in the live schema, but its shape has not been reviewed.
- **`FileBodyVariant.contentType`.** The schema marks it required, but the domain `Body` has no MIME-type field for file bodies. That is a domain-model change, not persistence serde.
- **Functional gaps from the audit.** Digest/NTLM/WSSE are never sent. OAuth2 password, auth-code and implicit are not applied at send time. `clientCertificates` are never loaded for mTLS. Environment `extends`/`dotEnvFilePath` are unused. `tokenConfig`, auto-fetch and auto-refresh are never read. This is HTTP execution engine work (`rocket-http`, `reqwest_executor.rs`).
- **Variable-resolution gaps.** The backend never fills the global environment, `{{process.env.FOO}}` is dead in the backend, and OAuth2 merges variables its own way. This is execution-service architecture work.
- **Correcting `docs/superpowers/specs/opencollection-spec-reference.md`.** Pure documentation, no code risk. Until it is fixed, use "Doc corrections" below.

**Found while verifying against the live schema (not in the audit, all out of this plan's scope):**

- `HttpRequestRuntime` and `WebSocketRequestRuntime` have no `auth` property, but `OcHttpRequestRuntime.auth` / `OcWebSocketRequestRuntime.auth` write one (domain `Request.runtime_auth`).
- `OAuth2Settings` allows only `autoFetchToken` and `autoRefreshToken`. Rocket also writes `verifySsl` and `useSystemBrowser` (`rocket_shared::oauth2::OAuth2Settings`).
- `OAuth2AdditionalParameter` allows only `name`, `value` and `placement`. Rocket always writes `enabled` (`default_true`, never skipped).
- `RequestDefaults.settings` is `{http: HttpRequestSettings, graphql: GraphQLRequestSettings}`, but `OcRequestSettings` is flat. Rocket never writes it today (`settings: None`), so it is latent.
- Sidebar summaries (`load_request_summary` in `fs_collection/tree.rs`) keep skipping GraphQL, gRPC and WebSocket files. A `RequestSummary` has `method`/`url` and opens through `get_request`, which only parses `OcHttpRequest`. Showing these items needs a UI surface first, and so does opening or editing opaque items (Task 2 hides them in the tree components).

**Doc corrections (use these, not the stale doc):**

- Schema URL: `https://schema.opencollection.com/opencollection/v1.0.0.json` (the URL in the doc returns 404).
- `OAuth2PKCE` is `{disabled: bool, method: "S256"|"plain"}`. Absent `disabled` means PKCE is on. There is no `enabled`.
- Implicit-flow `credentials` is `{clientId}` only, with no `clientSecret`.
- `WebSocketRequest` has `settings: {timeout, keepAliveInterval}`, each a `number` or the string `"inherit"`.
- `FolderInfo` is `{name, description, type, seq, tags}`. Folder request defaults live on `Folder.request`, not `FolderInfo.request`.
- The `Auth` oneOf has no `"none"` member. "No auth" is written by omitting `auth`.
- `extensions` exists only on the collection root.
- Proxy is `{disabled, inherit, config}`, which is already correct in code.

---

## File Structure

| File | Responsibility | Task |
|---|---|---|
| `crates/rocket-infra/src/fs_collection/folder_file.rs` (new) | The only reader and writer of `folder.yml`: spec `Folder` shape, legacy fallback | 1 |
| `crates/rocket-infra/src/oc/folder.rs` | Drop the non-spec `OcFolderInfo.request` | 1 |
| `crates/rocket-infra/src/fs_collection/{tree,paths,folders,variables}.rs`, `crates/rocket-infra/src/migration.rs` | Route every `folder.yml` access through `folder_file` | 1 |
| `crates/rocket-infra/src/conversions/folder.rs` | `oc_item_to_collection_item`: one place that maps `OcItem` to a domain tree item | 2 |
| `crates/rocket-infra/src/fs_collection/tree.rs` | Load GraphQL, gRPC and WebSocket files as `OpaqueItem` | 2 |
| `src/lib/tauri-api.ts`, `src/components/collections/{FolderNode,CollectionNode}.tsx`, `src/lib/contracts/collectPaths.ts` | Accept and skip `opaque` tree items | 2 |
| `crates/rocket-shared/src/oauth2.rs`, `crates/rocket-infra/src/oc/auth.rs`, `crates/rocket-infra/src/conversions/auth.rs`, `src/lib/oauth2-mapping.ts` | PKCE `disabled` field with legacy `enabled` read | 3 |
| `crates/rocket-infra/src/oc/websocket.rs` | WebSocket `settings` block | 4 |
| `crates/rocket-infra/src/conversions/{auth,request,folder}.rs`, `crates/rocket-infra/src/fs_collection/settings.rs`, `src/lib/persisted-auth.ts`, `src/lib/tauri-api.ts` | Never write `{type: none}`, keep `inherit` distinct | 5 |
| `crates/rocket-infra/src/oc/auth.rs`, `crates/rocket-infra/src/conversions/auth.rs` | Optional `clientSecret`, omitted for the implicit flow | 6 |
| `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs` (new) | Schema allow-list regression guard | 7 |

Task order matters. Task 4's repo-level test needs Task 2. Task 7 needs Tasks 1 to 6.

---

### Task 1: Write `folder.yml` in the spec `Folder` shape

**Files:**
- Create: `crates/rocket-infra/src/fs_collection/folder_file.rs`
- Modify: `crates/rocket-infra/src/fs_collection/mod.rs:12-20` (module list)
- Modify: `crates/rocket-infra/src/oc/folder.rs:13-44` (`OcFolderInfo` and its `Default`)
- Modify: `crates/rocket-infra/src/fs_collection/tree.rs:8,66-86`
- Modify: `crates/rocket-infra/src/fs_collection/paths.rs:8,38-58`
- Modify: `crates/rocket-infra/src/fs_collection/folders.rs:11,190-208,275-285`
- Modify: `crates/rocket-infra/src/fs_collection/variables.rs:7,51-56,82-102,118-131`
- Modify: `crates/rocket-infra/src/migration.rs:10,224-241`
- Modify: `crates/rocket-infra/src/conversions/folder.rs:94-102,113-121,252-260` (drop `request: None` from three `OcFolderInfo` literals)
- Modify: `crates/rocket-infra/CLAUDE.md` ("On-disk format" paragraph)
- Test: `crates/rocket-infra/src/fs_collection/tests.rs` (new tests, and update `legacy_uid_migrated_into_folder_yml` at ~515-545)
- Test: `crates/rocket-infra/src/migration.rs` (test at ~424-447)

**Interfaces:**
- Consumes: `crate::oc::{OcFolder, OcFolderInfo, OcRequestDefaults}`, `crate::atomic_write`.
- Produces, in `crate::fs_collection::folder_file`:
  - `pub(crate) fn parse_folder_yml(content: &str) -> Result<OcFolder, serde_yaml::Error>`
  - `pub(crate) fn read_folder_yml(path: &Path) -> DomainResult<OcFolder>`
  - `pub(crate) fn write_folder_yml(path: &Path, folder: &OcFolder) -> DomainResult<()>`
  - `pub(crate) fn new_folder(name: String, uid: String) -> OcFolder`
- Produces: `OcFolderInfo` without a `request` field. Folder request defaults are only on `OcFolder.request`.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Where it conflicts with "Doc corrections (use these)" in this plan's Deferred section, trust the plan.

- [ ] **Step 2: Write the failing repo-level tests**

Append to `crates/rocket-infra/src/fs_collection/tests.rs`:

```rust
fn read_yaml_value(path: &std::path::Path) -> serde_yaml::Value {
    serde_yaml::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn create_folder_writes_spec_folder_shape() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "auth").unwrap();

    let raw = read_yaml_value(&dir.path().join("my-api/auth/folder.yml"));
    assert_eq!(raw["info"]["name"].as_str(), Some("auth"), "{raw:?}");
    assert_eq!(raw["info"]["type"].as_str(), Some("folder"), "{raw:?}");
    assert!(raw.get("name").is_none(), "folder.yml must not be a bare FolderInfo: {raw:?}");
    assert!(raw.get("items").is_none(), "items must never be written: {raw:?}");
}

#[test]
fn folder_uid_is_stable_across_reloads() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "auth").unwrap();

    let first = repo.get("my-api").unwrap().root.find_folder("auth").unwrap().uid.clone();
    let second = repo.get("my-api").unwrap().root.find_folder("auth").unwrap().uid.clone();
    assert!(!first.is_empty());
    assert_eq!(first, second, "folder uid must not regenerate on every load");
}

#[test]
fn legacy_bare_folder_yml_still_loads_and_is_upgraded_on_write() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    let folder_dir = dir.path().join("my-api/auth");
    fs::create_dir_all(&folder_dir).unwrap();
    fs::write(
        folder_dir.join("folder.yml"),
        "name: Auth Flows\nuid: legacy-folder-uid\ntype: folder\nrequest:\n  variables:\n  - name: token\n    value: abc\n",
    )
    .unwrap();

    // Legacy file loads with its uid and display name.
    let col = repo.get("my-api").unwrap();
    let folder = col
        .root
        .items
        .iter()
        .find_map(|i| match i {
            rocket_collection::CollectionItem::Folder(f) => Some(f),
            _ => None,
        })
        .unwrap();
    assert_eq!(folder.uid, "legacy-folder-uid");
    assert_eq!(folder.name, "Auth Flows");

    // Legacy folder variables still feed the chain.
    let req = rocket_collection::Request::new("Login", HttpMethod::Post, "https://example.com");
    repo.save_request("my-api", "auth/login.yml", &req).unwrap();
    let chain = repo.get_folder_chain_variables("my-api", "auth/login.yml").unwrap();
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].key, "token");

    // The next write upgrades the file to the spec shape and keeps the uid.
    repo.save_folder_variables(
        "my-api",
        "auth",
        vec![CollectionVariable {
            key: "token".into(),
            value: "xyz".into(),
            initial_value: String::new(),
            enabled: true,
            secret: false,
        }],
    )
    .unwrap();
    let raw = read_yaml_value(&folder_dir.join("folder.yml"));
    assert_eq!(raw["info"]["uid"].as_str(), Some("legacy-folder-uid"), "{raw:?}");
    assert_eq!(raw["info"]["name"].as_str(), Some("Auth Flows"), "{raw:?}");
    assert!(raw["info"].get("request").is_none(), "request defaults must leave info: {raw:?}");
    assert_eq!(raw["request"]["variables"][0]["name"].as_str(), Some("token"), "{raw:?}");
    assert_eq!(repo.get_folder_variables("my-api", "auth").unwrap()[0].value, "xyz");
}

#[test]
fn rename_folder_keeps_spec_shape_and_uid() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "old-name").unwrap();
    let before = read_yaml_value(&dir.path().join("my-api/old-name/folder.yml"));
    let uid = before["info"]["uid"].as_str().unwrap().to_string();

    repo.move_item("my-api", "old-name", "my-api", "new-name").unwrap();

    let after = read_yaml_value(&dir.path().join("my-api/new-name/folder.yml"));
    assert_eq!(after["info"]["name"].as_str(), Some("new-name"), "{after:?}");
    assert_eq!(after["info"]["uid"].as_str(), Some(uid.as_str()), "{after:?}");
}
```

In `crates/rocket-infra/src/migration.rs`, in the test that ends with `assert!(content.contains("folder-uid"));` (~line 447), add this after that line:

```rust
        let raw: serde_yaml::Value = serde_yaml::from_str(&content).unwrap();
        assert_eq!(raw["info"]["uid"].as_str(), Some("folder-uid"), "{content}");
        assert_eq!(raw["info"]["name"].as_str(), Some("auth"), "{content}");
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p rocket-infra -- create_folder_writes_spec_folder_shape folder_uid_is_stable_across_reloads legacy_bare_folder_yml rename_folder_keeps_spec_shape_and_uid migration::`
Expected: FAIL. `create_folder_writes_spec_folder_shape`, `legacy_bare_folder_yml_still_loads_and_is_upgraded_on_write`, `rename_folder_keeps_spec_shape_and_uid` and the migration test fail on `raw["info"][...]` returning `None`, because the file is still a bare `FolderInfo`. `folder_uid_is_stable_across_reloads` already passes. It guards against a regression in Step 4.

- [ ] **Step 4: Create `folder_file.rs`**

Create `crates/rocket-infra/src/fs_collection/folder_file.rs`:

```rust
//! Reads and writes `folder.yml` in the OpenCollection `Folder` shape
//! (`info` + `request` + `docs`). Files written before this change use a bare
//! `FolderInfo` shape with request defaults nested inside it, and are still read.

use std::fs;
use std::path::Path;

use rocket_shared::description::Description;
use rocket_shared::error::{DomainError, DomainResult};
use serde::Deserialize;

use crate::atomic_write;
use crate::oc::{OcFolder, OcFolderInfo, OcRequestDefaults};

/// Legacy `folder.yml` shape: folder info at the top level, request defaults inside it.
#[derive(Deserialize)]
struct LegacyFolderYml {
    name: String,
    #[serde(default)]
    uid: Option<String>,
    #[serde(default)]
    description: Option<Description>,
    #[serde(default, rename = "type")]
    folder_type: Option<String>,
    #[serde(default)]
    seq: Option<u32>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    request: Option<OcRequestDefaults>,
}

impl From<LegacyFolderYml> for OcFolder {
    fn from(old: LegacyFolderYml) -> Self {
        OcFolder {
            info: OcFolderInfo {
                name: old.name,
                uid: old.uid,
                description: old.description,
                folder_type: old.folder_type,
                seq: old.seq,
                tags: old.tags,
            },
            items: None,
            request: old.request,
            docs: None,
        }
    }
}

/// Parses `folder.yml` content. The spec shape is tried first. Its required
/// `info` key never appears in the legacy shape, so a legacy file fails that
/// parse and falls back to `LegacyFolderYml`. When both fail, the spec-shape
/// error is returned.
pub(crate) fn parse_folder_yml(content: &str) -> Result<OcFolder, serde_yaml::Error> {
    match serde_yaml::from_str::<OcFolder>(content) {
        Ok(folder) => Ok(folder),
        Err(spec_err) => serde_yaml::from_str::<LegacyFolderYml>(content)
            .map(OcFolder::from)
            .map_err(|_| spec_err),
    }
}

/// Reads and parses a `folder.yml` file.
pub(crate) fn read_folder_yml(path: &Path) -> DomainResult<OcFolder> {
    let content = fs::read_to_string(path)?;
    parse_folder_yml(&content)
        .map_err(|e| DomainError::Internal(format!("Failed to parse folder.yml: {e}")))
}

/// Writes `folder.yml` in the spec shape. Items live as separate files in the
/// unbundled layout, so `items` is always dropped before writing.
pub(crate) fn write_folder_yml(path: &Path, folder: &OcFolder) -> DomainResult<()> {
    let on_disk = OcFolder {
        items: None,
        ..folder.clone()
    };
    let yaml = serde_yaml::to_string(&on_disk)
        .map_err(|e| DomainError::Internal(format!("Failed to serialize folder.yml: {e}")))?;
    atomic_write(path, yaml.as_bytes())?;
    Ok(())
}

/// Builds the `folder.yml` content for a brand-new folder.
pub(crate) fn new_folder(name: String, uid: String) -> OcFolder {
    OcFolder {
        info: OcFolderInfo {
            name,
            uid: Some(uid),
            ..OcFolderInfo::default()
        },
        items: None,
        request: None,
        docs: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_folder_yml_reads_spec_shape() {
        let yaml = "info:\n  name: auth\n  uid: f-1\n  type: folder\nrequest:\n  variables:\n  - name: token\n    value: abc\n";
        let folder = parse_folder_yml(yaml).expect("spec shape parses");
        assert_eq!(folder.info.name, "auth");
        assert_eq!(folder.info.uid.as_deref(), Some("f-1"));
        let vars = folder.request.and_then(|r| r.variables).expect("vars");
        assert_eq!(vars[0].name, "token");
    }

    #[test]
    fn parse_folder_yml_falls_back_to_legacy_shape_and_lifts_request() {
        let yaml = "name: auth\nuid: f-1\ntype: folder\nrequest:\n  variables:\n  - name: token\n    value: abc\n";
        let folder = parse_folder_yml(yaml).expect("legacy shape parses");
        assert_eq!(folder.info.name, "auth");
        assert_eq!(folder.info.uid.as_deref(), Some("f-1"));
        let vars = folder
            .request
            .and_then(|r| r.variables)
            .expect("vars lifted to Folder.request");
        assert_eq!(vars[0].name, "token");
    }

    #[test]
    fn parse_folder_yml_rejects_garbage() {
        assert!(parse_folder_yml("{{{{not valid yaml: [[[").is_err());
    }

    #[test]
    fn write_folder_yml_emits_spec_shape_without_items() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir.path().join("folder.yml");
        let mut folder = new_folder("auth".into(), "f-1".into());
        folder.items = Some(Vec::new());
        write_folder_yml(&path, &folder).expect("write");

        let raw: serde_yaml::Value =
            serde_yaml::from_str(&fs::read_to_string(&path).expect("read")).expect("yaml");
        assert!(raw.get("info").is_some(), "must be wrapped in info: {raw:?}");
        assert!(raw.get("name").is_none(), "no bare top-level name: {raw:?}");
        assert!(raw.get("items").is_none(), "items must never be written: {raw:?}");
        assert_eq!(raw["info"]["name"].as_str(), Some("auth"));
    }
}
```

Register it in `crates/rocket-infra/src/fs_collection/mod.rs`. The line must come before `mod folders;` because `migration.rs` also uses it:

```rust
pub(crate) mod folder_file;
mod folders;
```

- [ ] **Step 5: Remove `request` from `OcFolderInfo`**

In `crates/rocket-infra/src/oc/folder.rs`, delete these lines from `OcFolderInfo` (~27-29):

```rust
    /// Folder-level request defaults (variables, auth, headers).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<OcRequestDefaults>,
```

Also delete `request: None,` from `impl Default for OcFolderInfo` (~41). `OcRequestDefaults` is still used by `OcFolder.request`, so keep its import.

In `crates/rocket-infra/src/conversions/folder.rs`, delete the `request: None,` line inside each of the three `OcFolderInfo { ... }` literals (~101, ~120, ~259). Keep the `request: None,` lines that belong to `OcFolder { ... }` literals.

- [ ] **Step 6: Route every `folder.yml` reader and writer through `folder_file`**

`crates/rocket-infra/src/fs_collection/tree.rs`: change the import on line 8 to `use crate::oc::OcHttpRequest;`, add `use super::folder_file::parse_folder_yml;`, and replace the inner block at ~71-80 with:

```rust
        if let Ok(content) = fs::read_to_string(&folder_yml) {
            if let Ok(oc_folder) = parse_folder_yml(&content) {
                if let Some(ref uid) = oc_folder.info.uid {
                    if !uid.is_empty() {
                        folder.uid = uid.clone();
                    }
                }
                folder.name = oc_folder.info.name;
            }
        }
```

`crates/rocket-infra/src/fs_collection/paths.rs`: change line 8 to `use crate::oc::OcCollection;`, add `use super::folder_file::{parse_folder_yml, write_folder_yml};`, and replace the folder branch at ~41-56 with:

```rust
        if let Ok(content) = fs::read_to_string(&folder_path) {
            if let Ok(mut oc_folder) = parse_folder_yml(&content) {
                if let Some(ref uid) = oc_folder.info.uid {
                    if !uid.is_empty() {
                        return uid.clone();
                    }
                }
                let uid = read_legacy_uid(dir);
                oc_folder.info.uid = Some(uid.clone());
                if write_folder_yml(&folder_path, &oc_folder).is_ok() {
                    cleanup_legacy_uid(dir);
                }
                return uid;
            }
        }
```

`crates/rocket-infra/src/fs_collection/folders.rs`: change line 11 to `use crate::oc::{OcCollection, OcInfo};`, add `use super::folder_file::{new_folder, parse_folder_yml, write_folder_yml};`, and replace lines ~195-206 in `create_folder` (from `let info = OcFolderInfo {` through the `atomic_write(...)` call) with:

```rust
    write_folder_yml(
        &dir_path.join("folder.yml"),
        &new_folder(folder_name, generate_uid()),
    )?;
```

In `move_item`, replace the block at ~276-285 with:

```rust
        if folder_yml.exists() {
            let content = fs::read_to_string(&folder_yml)?;
            if let Ok(mut oc_folder) = parse_folder_yml(&content) {
                oc_folder.info.name = new_name;
                write_folder_yml(&folder_yml, &oc_folder)?;
            }
        }
```

`crates/rocket-infra/src/fs_collection/variables.rs`: change line 7 to

```rust
use crate::oc::{
    OcFolder, OcFolderInfo, OcHttpRequest, OcHttpRequestRuntime, OcRequestDefaults, OcVariable,
};
```

and add `use super::folder_file::{parse_folder_yml, read_folder_yml, write_folder_yml};`. In `get_folder_chain_variables`, replace ~51-56 with:

```rust
        let Ok(oc_folder) = parse_folder_yml(&content) else {
            continue;
        };
        let Some(req) = oc_folder.request else {
            continue;
        };
```

In `save_folder_variables`, replace ~83-102 (from `let mut info: OcFolderInfo =` through `atomic_write(...)?;`) with:

```rust
    let mut oc_folder = if folder_yml_path.exists() {
        read_folder_yml(&folder_yml_path)?
    } else {
        OcFolder {
            info: OcFolderInfo::default(),
            items: None,
            request: None,
            docs: None,
        }
    };
    let oc_vars: Vec<OcVariable> = vars.into_iter().map(OcVariable::from).collect();
    let req_defaults = oc_folder.request.take().unwrap_or_default();
    oc_folder.request = Some(OcRequestDefaults {
        variables: if oc_vars.is_empty() {
            None
        } else {
            Some(oc_vars)
        },
        ..req_defaults
    });
    write_folder_yml(&folder_yml_path, &oc_folder)?;
```

In `get_folder_variables`, replace ~122-131 with:

```rust
    let oc_folder = read_folder_yml(&folder_yml)?;
    let vars = oc_folder
        .request
        .and_then(|r| r.variables)
        .unwrap_or_default()
        .into_iter()
        .map(CollectionVariable::from)
        .collect();
```

`crates/rocket-infra/src/migration.rs`: change line 10 to `use crate::oc::{OcCollection, OcInfo};`, add `use crate::fs_collection::folder_file::{new_folder, write_folder_yml};`, and replace ~226-241 with:

```rust
            if !folder_yml.exists() {
                let uid = read_legacy_uid_value(&path);
                write_folder_yml(&folder_yml, &new_folder(name.clone(), uid))?;
            }
```

- [ ] **Step 7: Update the existing legacy-uid test**

In `crates/rocket-infra/src/fs_collection/tests.rs::legacy_uid_migrated_into_folder_yml` (~515-545), remove `use crate::oc::OcFolderInfo;` and replace the four "strip uid" lines (from `let content = fs::read_to_string(folder_dir.join("folder.yml")).unwrap();` through `fs::write(folder_dir.join("folder.yml"), yaml).unwrap();`) with:

```rust
    let content = fs::read_to_string(folder_dir.join("folder.yml")).unwrap();
    let mut folder = crate::fs_collection::folder_file::parse_folder_yml(&content).unwrap();
    folder.info.uid = None;
    fs::write(folder_dir.join("folder.yml"), serde_yaml::to_string(&folder).unwrap()).unwrap();
```

- [ ] **Step 8: Update the crate doc**

In `crates/rocket-infra/CLAUDE.md`, in the "**On-disk format.**" paragraph, replace `` `folder.yml` (subfolder metadata) `` with `` `folder.yml` (spec `Folder` shape: `info`/`request`/`docs`, read and written only through `fs_collection/folder_file.rs`, which also reads the legacy bare-`FolderInfo` shape) ``.

- [ ] **Step 9: Run the tests to verify they pass**

Run: `cargo check -p rocket-infra && cargo test -p rocket-infra`
Expected: PASS. This includes the four new repo tests, the four `folder_file::tests`, the migration test, `legacy_uid_migrated_into_folder_yml`, `save_folder_variables_rejects_corrupt_folder_yml` (garbage content still errors and is not overwritten) and `get_folder_chain_variables_returns_folder_vars`.

- [ ] **Step 10: Commit**

```bash
git add crates/rocket-infra/src/fs_collection/folder_file.rs crates/rocket-infra/src/fs_collection/mod.rs crates/rocket-infra/src/oc/folder.rs crates/rocket-infra/src/fs_collection/tree.rs crates/rocket-infra/src/fs_collection/paths.rs crates/rocket-infra/src/fs_collection/folders.rs crates/rocket-infra/src/fs_collection/variables.rs crates/rocket-infra/src/migration.rs crates/rocket-infra/src/conversions/folder.rs crates/rocket-infra/src/fs_collection/tests.rs crates/rocket-infra/CLAUDE.md
```

Invoke the `dev-workflow-skills:1-git-commit` skill. Target subject: `fix(infra): write folder.yml in the OpenCollection Folder shape`.

---

### Task 2: Load GraphQL, gRPC and WebSocket files instead of dropping them

**Files:**
- Modify: `crates/rocket-infra/src/conversions/folder.rs:9-78,128-192` (add `oc_item_to_collection_item`, use it in both tree converters)
- Modify: `crates/rocket-infra/src/conversions/mod.rs:24-28` (re-export)
- Modify: `crates/rocket-infra/src/fs_collection/tree.rs:7-34`
- Modify: `src/lib/tauri-api.ts:153-156`
- Modify: `src/components/collections/FolderNode.tsx:345`
- Modify: `src/components/collections/CollectionNode.tsx:573`
- Modify: `src/lib/contracts/collectPaths.ts:14-25`
- Test: `crates/rocket-infra/src/fs_collection/tests.rs`
- Test (create): `src/lib/contracts/collectPaths.test.ts`

**Interfaces:**
- Consumes: `folder_file::parse_folder_yml` (Task 1, already wired into `tree.rs`).
- Produces: `pub fn oc_item_to_collection_item(item: OcItem) -> Option<CollectionItem>` in `crate::conversions`.
- Produces: `fn opaque_items(folder: &rocket_collection::Folder) -> Vec<&rocket_collection::folder::OpaqueProtocolItem>`, a test helper in `fs_collection/tests.rs` that Task 4 reuses.
- Produces (TS): `export interface OpaqueProtocolItem`, and `CollectionItem` gains `({ type: 'opaque' } & OpaqueProtocolItem)`.

Decisions baked in:
- `ScriptFile` is not a `CollectionItem` variant (`crates/rocket-collection/src/folder.rs:20-33`: `Request | Folder | OpaqueItem | Summary`), so script files are skipped quietly, not as corrupt.
- `build_folder_tree_summaries` / `load_request_summary` are left alone. Summaries open through `get_request`, which only parses HTTP, so these items stay out of the sidebar. See Deferred.
- `oc_item_to_collection_item` (new, used by `tree.rs`) calls `oc_folder_to_folder` for nested folder items, so `oc_folder_to_folder` becomes reachable from production and loses its `#[allow(dead_code)]`. `folder_to_oc_folder`, `oc_collection_to_collection` and `collection_to_oc_collection` are still used only by tests (the bundled layout, which Rocket does not write), so they keep the attribute with a comment saying why.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Where it conflicts with "Doc corrections (use these)" in this plan's Deferred section, trust the plan.

- [ ] **Step 2: Write the failing Rust tests**

Append to `crates/rocket-infra/src/fs_collection/tests.rs`:

```rust
const GRAPHQL_ITEM_YML: &str = "info:\n  name: List Users\n  type: graphql\ngraphql:\n  url: https://api.example.com/graphql\n  body:\n    query: '{ users { id } }'\n";
const GRPC_ITEM_YML: &str = "info:\n  name: Get User\n  type: grpc\ngrpc:\n  url: grpc://api.example.com\n  method: users.UserService/GetUser\n  methodType: unary\n";
const WEBSOCKET_ITEM_YML: &str = "info:\n  name: Chat\n  type: websocket\nwebsocket:\n  url: wss://chat.example.com/ws\n";

fn opaque_items(
    folder: &rocket_collection::Folder,
) -> Vec<&rocket_collection::folder::OpaqueProtocolItem> {
    folder
        .items
        .iter()
        .filter_map(|i| match i {
            rocket_collection::CollectionItem::OpaqueItem(o) => Some(o),
            _ => None,
        })
        .collect()
}

#[test]
fn build_folder_tree_loads_non_http_items_as_opaque() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "realtime").unwrap();
    let col_dir = dir.path().join("my-api");
    fs::write(col_dir.join("list-users.yml"), GRAPHQL_ITEM_YML).unwrap();
    fs::write(col_dir.join("get-user.yml"), GRPC_ITEM_YML).unwrap();
    fs::write(col_dir.join("realtime/chat.yml"), WEBSOCKET_ITEM_YML).unwrap();

    let col = repo.get("my-api").unwrap();
    let mut root: Vec<(&str, &str)> = opaque_items(&col.root)
        .iter()
        .map(|o| (o.protocol.as_str(), o.name.as_str()))
        .collect();
    root.sort();
    assert_eq!(root, vec![("graphql", "List Users"), ("grpc", "Get User")]);

    let realtime = col.root.find_folder("realtime").unwrap();
    let ws = opaque_items(realtime);
    assert_eq!(ws.len(), 1);
    assert_eq!(ws[0].protocol, "websocket");
    assert_eq!(ws[0].name, "Chat");
    assert_eq!(
        ws[0].raw["websocket"]["url"].as_str(),
        Some("wss://chat.example.com/ws")
    );
}

#[test]
fn build_folder_tree_skips_script_files_without_dropping_siblings() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(
        dir.path().join("my-api/setup.yml"),
        "type: script\nscript: ./scripts/setup.js\n",
    )
    .unwrap();
    let req = rocket_collection::Request::new("Good", HttpMethod::Get, "https://example.com");
    repo.save_request("my-api", "good.yml", &req).unwrap();

    let col = repo.get("my-api").unwrap();
    assert_eq!(col.root.items.len(), 1, "{:?}", col.root.items);
    assert!(matches!(
        &col.root.items[0],
        rocket_collection::CollectionItem::Request(r) if r.name == "Good"
    ));
}

#[test]
fn build_folder_tree_skips_http_file_missing_method_instead_of_misreading_it() {
    // OcItem is untagged and OcFolder needs only `info`, so a broken HTTP file
    // would match OcItem::Folder. It must be skipped as corrupt, never turned
    // into a phantom folder.
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(
        dir.path().join("my-api/broken.yml"),
        "info:\n  name: Broken\n  type: http\nhttp:\n  url: https://example.com\n",
    )
    .unwrap();

    let col = repo.get("my-api").unwrap();
    assert!(col.root.items.is_empty(), "{:?}", col.root.items);
}

#[test]
fn get_summaries_skips_non_http_items_without_error() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(dir.path().join("my-api/list-users.yml"), GRAPHQL_ITEM_YML).unwrap();
    let req = rocket_collection::Request::new("Good", HttpMethod::Get, "https://example.com");
    repo.save_request("my-api", "good.yml", &req).unwrap();

    let col = repo.get_summaries("my-api").unwrap();
    assert_eq!(col.root.items.len(), 1);
    assert!(matches!(
        &col.root.items[0],
        rocket_collection::CollectionItem::Summary(s) if s.name == "Good"
    ));
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p rocket-infra build_folder_tree_loads_non_http_items_as_opaque`
Expected: FAIL. The `root` vector is empty (`left: []`) because non-HTTP files are dropped with "skipping corrupt request file". The other three tests pass before and after the change. They guard the new fallback.

- [ ] **Step 4: Add `oc_item_to_collection_item` and reuse it**

In `crates/rocket-infra/src/conversions/folder.rs`, add above `oc_folder_to_folder`:

```rust
/// Converts one parsed OpenCollection item into a domain tree item.
/// GraphQL, gRPC and WebSocket items become `OpaqueItem`s that hold their raw
/// YAML, so nothing is lost on load. Script files are not tree items in the
/// domain model, so they return `None`.
pub fn oc_item_to_collection_item(item: OcItem) -> Option<CollectionItem> {
    match item {
        OcItem::Http(req) => Some(CollectionItem::Request(Box::new(
            oc_http_request_to_request(req),
        ))),
        OcItem::Folder(f) => Some(CollectionItem::Folder(oc_folder_to_folder(f))),
        OcItem::ScriptFile(_) => None,
        OcItem::GraphQL(gql) => {
            let name = gql.info.name.clone();
            opaque_item("graphql", name, &OcItem::GraphQL(gql))
        }
        OcItem::Grpc(grpc) => {
            let name = grpc.info.name.clone();
            opaque_item("grpc", name, &OcItem::Grpc(grpc))
        }
        OcItem::WebSocket(ws) => {
            let name = ws.info.name.clone();
            opaque_item("websocket", name, &OcItem::WebSocket(ws))
        }
    }
}

/// Wraps a non-HTTP item as an opaque tree item holding its raw YAML.
fn opaque_item(protocol: &str, name: String, item: &OcItem) -> Option<CollectionItem> {
    serde_yaml::to_value(item).ok().map(|raw| {
        CollectionItem::OpaqueItem(OpaqueProtocolItem {
            protocol: protocol.into(),
            name,
            raw,
        })
    })
}
```

Replace the whole `.filter_map(|item| match &item { ... })` closure in `oc_folder_to_folder` (~18-66) with `.filter_map(oc_item_to_collection_item)`. Do the same in `oc_collection_to_collection` (~143-191).

Delete `#[allow(dead_code)]` above `oc_folder_to_folder` (line 10). It is now reachable from `tree.rs` through `oc_item_to_collection_item`. Above `folder_to_oc_folder`, `oc_collection_to_collection` and `collection_to_oc_collection`, replace `#[allow(dead_code)]` with:

```rust
// Used only by tests: this converts the bundled layout, which Rocket does not write.
#[allow(dead_code)]
```

In `crates/rocket-infra/src/conversions/mod.rs`, extend the folder re-export:

```rust
pub use folder::{
    collection_to_oc_collection, folder_to_oc_folder, oc_collection_to_collection,
    oc_folder_to_folder, oc_item_to_collection_item,
};
```

- [ ] **Step 5: Use it in `build_folder_tree`**

In `crates/rocket-infra/src/fs_collection/tree.rs`, replace the imports on lines 7-8 with:

```rust
use crate::conversions::{oc_http_request_to_request, oc_item_to_collection_item};
use crate::oc::{OcHttpRequest, OcItem};
```

Replace `build_folder_tree` (lines 12-34) with:

```rust
pub(super) fn build_folder_tree(current: &Path) -> DomainResult<Folder> {
    build_tree(current, &mut |path, entry_name| {
        let content = fs::read_to_string(path)?;
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let loaded = match ext {
            "yml" | "yaml" => load_yaml_item(&content),
            _ => serde_json::from_str::<rocket_collection::Request>(&content)
                .map(|r| Some(CollectionItem::Request(Box::new(r))))
                .map_err(|e| e.to_string()),
        };
        match loaded {
            Ok(Some(CollectionItem::Request(mut request))) => {
                request.file_name = Some(entry_name.to_string());
                Ok(Some(CollectionItem::Request(request)))
            }
            Ok(other) => Ok(other),
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "skipping corrupt request file");
                Ok(None)
            }
        }
    })
}

/// Parses one `.yml` item file. HTTP is tried first so a broken HTTP file keeps
/// its precise parse error. Other protocols are recognised through the untagged
/// `OcItem` enum. A file that matches only `OcItem::Folder` is a broken request,
/// since a folder is a directory and never a single file, so it is reported with
/// the HTTP parse error.
fn load_yaml_item(content: &str) -> Result<Option<CollectionItem>, String> {
    let http_err = match serde_yaml::from_str::<OcHttpRequest>(content) {
        Ok(req) => {
            return Ok(Some(CollectionItem::Request(Box::new(
                oc_http_request_to_request(req),
            ))))
        }
        Err(e) => e,
    };
    match serde_yaml::from_str::<OcItem>(content) {
        Ok(OcItem::Folder(_)) | Err(_) => Err(http_err.to_string()),
        Ok(OcItem::ScriptFile(_)) => {
            tracing::debug!("skipping script file; scripts are not collection tree items");
            Ok(None)
        }
        Ok(item) => Ok(oc_item_to_collection_item(item)),
    }
}
```

`DomainError` is still used by `load_request_summary`, so keep that import.

- [ ] **Step 6: Run the Rust tests to verify they pass**

Run: `cargo check -p rocket-infra && cargo test -p rocket-infra`
Expected: PASS. This includes the four new tests, `build_folder_tree_skips_corrupt_request_file`, and the existing `conversions::tests` that call `oc_folder_to_folder` / `oc_collection_to_collection`.

- [ ] **Step 7: Write the failing frontend test**

Create `src/lib/contracts/collectPaths.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { CollectionItem } from '@/lib/tauri-api';
import { collectPaths } from './collectPaths';

describe('collectPaths', () => {
  it('skips opaque protocol items so they are never treated as request paths', () => {
    const items: CollectionItem[] = [
      {
        type: 'summary',
        uid: 'r1',
        name: 'Get Users',
        method: 'GET',
        url: '/users',
        fileName: 'get-users.yml',
      },
      { type: 'opaque', protocol: 'graphql', name: 'List Users', raw: {} },
      {
        type: 'folder',
        uid: 'f1',
        name: 'auth',
        dirName: 'auth',
        items: [{ type: 'opaque', protocol: 'websocket', name: 'Chat', raw: {} }],
      },
    ];
    const folders: string[] = [];
    const requests: string[] = [];
    collectPaths(items, '', folders, requests);
    expect(folders).toEqual(['auth']);
    expect(requests).toEqual(['get-users.yml']);
  });
});
```

Run: `yarn test collectPaths`
Expected: FAIL. `requests` also contains `'List Users'` and `'auth/Chat'`.

- [ ] **Step 8: Add the TS type and skip opaque items**

In `src/lib/tauri-api.ts`, replace the `CollectionItem` union (~153-156) with:

```ts
/** Non-HTTP item (GraphQL, gRPC, WebSocket) kept as raw YAML. Not shown or editable in the UI yet. */
export interface OpaqueProtocolItem {
  protocol: 'graphql' | 'grpc' | 'websocket';
  name: string;
  raw: unknown;
}

export type CollectionItem =
  | ({ type: 'request' } & Request)
  | ({ type: 'folder' } & Folder)
  | ({ type: 'summary' } & RequestSummary)
  | ({ type: 'opaque' } & OpaqueProtocolItem);
```

This union change causes exactly three `yarn tsc --noEmit` errors (confirmed while writing this plan). Fix each one.

`src/components/collections/FolderNode.tsx:345`, replace `if (item.type === 'summary') return null;` with:

```tsx
            if (item.type === 'summary' || item.type === 'opaque') return null;
```

`src/components/collections/CollectionNode.tsx:573`, make the same replacement:

```tsx
            if (item.type === 'summary' || item.type === 'opaque') return null;
```

`src/lib/contracts/collectPaths.ts`, replace the `} else {` branch (~20-24) with:

```ts
    } else if (item.type !== 'opaque') {
      const seg = item.fileName ?? item.name;
      const path = prefix ? `${prefix}/${seg}` : seg;
      requests.push(path);
    }
```

- [ ] **Step 9: Run the frontend checks to verify they pass**

Run: `yarn tsc --noEmit && yarn check && yarn test collectPaths`
Expected: PASS with no type errors.

- [ ] **Step 10: Commit**

```bash
git add crates/rocket-infra/src/conversions/folder.rs crates/rocket-infra/src/conversions/mod.rs crates/rocket-infra/src/fs_collection/tree.rs crates/rocket-infra/src/fs_collection/tests.rs src/lib/tauri-api.ts src/components/collections/FolderNode.tsx src/components/collections/CollectionNode.tsx src/lib/contracts/collectPaths.ts src/lib/contracts/collectPaths.test.ts
```

Invoke the `dev-workflow-skills:1-git-commit` skill. Target subject: `fix(infra): load GraphQL, gRPC and WebSocket items as opaque tree items`.

---

### Task 3: OAuth2 PKCE uses `disabled`, not `enabled`

**Files:**
- Modify: `crates/rocket-shared/src/oauth2.rs:19-25` (struct), tests at `:239-248` and `:378-404`
- Modify: `crates/rocket-infra/src/oc/auth.rs:3,104-110`
- Modify: `crates/rocket-infra/src/conversions/auth.rs:2-5,115,152-157,340,391-396`
- Modify: `crates/rocket-infra/src/conversions/tests.rs:918-921`
- Modify: `src/lib/oauth2-mapping.ts:17-20,92-95,217`
- Test: `crates/rocket-infra/src/oc/mod.rs` (next to `oc_auth_oauth2_authorization_code_yaml`, ~235-250)
- Test (create): `src/lib/__tests__/oauth2-mapping.test.ts`

**Interfaces:**
- Produces: `rocket_shared::oauth2::OAuth2PKCE { pub disabled: Option<bool>, pub method: Option<String> }` and `OAuth2PKCE::is_enabled(&self) -> bool`. Deserialize accepts the legacy `enabled` and normalizes it. Serialize writes only `disabled` (and only when `true`) plus `method`.
- Produces: `crate::oc::OcOAuth2PKCE` becomes `pub type OcOAuth2PKCE = rocket_shared::oauth2::OAuth2PKCE;`, reusing the domain type the same way `OcAuthTyped::OAuth2` already reuses `OAuth2Settings`/`OAuth2TokenConfig`.
- IPC contract: the domain struct is sent to the frontend as `pkce: { disabled?: boolean | null, method?: string | null }`.

Call sites checked: in Rust only `conversions/auth.rs` reads or writes the PKCE flag. `rocket-app/oauth2_service.rs` and `src-tauri/commands/oauth2.rs` use a separate `use_pkce` bool from the request DTO. In the frontend only `src/lib/oauth2-mapping.ts` touches `pkce`.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Where it conflicts with "Doc corrections (use these)" in this plan's Deferred section, trust the plan. PKCE is `{disabled, method}`, not the doc's `{enabled, method}`.

- [ ] **Step 2: Write the failing domain tests**

In `crates/rocket-shared/src/oauth2.rs` tests, replace `pkce_config` (~239-248) with:

```rust
    #[test]
    fn pkce_config() {
        let pkce = OAuth2PKCE {
            disabled: None,
            method: Some("S256".into()),
        };
        let json = serde_json::to_string(&pkce).unwrap();
        assert!(!json.contains("enabled"), "legacy field must never be written: {json}");
        let back: OAuth2PKCE = serde_json::from_str(&json).unwrap();
        assert_eq!(pkce, back);
        assert!(back.is_enabled());
    }

    #[test]
    fn pkce_disabled_serializes_as_spec_field() {
        let pkce = OAuth2PKCE {
            disabled: Some(true),
            method: None,
        };
        assert_eq!(serde_json::to_string(&pkce).unwrap(), r#"{"disabled":true}"#);
        assert!(!pkce.is_enabled());
    }

    #[test]
    fn pkce_reads_legacy_enabled_field() {
        let on: OAuth2PKCE = serde_json::from_str(r#"{"enabled":true,"method":"S256"}"#).unwrap();
        assert_eq!(
            on,
            OAuth2PKCE {
                disabled: None,
                method: Some("S256".into())
            }
        );
        let off: OAuth2PKCE = serde_json::from_str(r#"{"enabled":false}"#).unwrap();
        assert_eq!(
            off,
            OAuth2PKCE {
                disabled: Some(true),
                method: None
            }
        );
    }

    #[test]
    fn pkce_disabled_wins_over_legacy_enabled() {
        let p: OAuth2PKCE = serde_json::from_str(r#"{"disabled":false,"enabled":false}"#).unwrap();
        assert!(p.is_enabled());
    }

    #[test]
    fn pkce_normalises_disabled_false_to_absent() {
        let p: OAuth2PKCE = serde_json::from_str(r#"{"disabled":false}"#).unwrap();
        assert_eq!(p.disabled, None);
    }
```

In `authorization_code_flow_with_pkce` (~392-395), change the PKCE literal to:

```rust
            pkce: Some(OAuth2PKCE {
                disabled: None,
                method: Some("S256".into()),
            }),
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p rocket-shared pkce`
Expected: FAIL to compile with `error[E0560]: struct `OAuth2PKCE` has no field named `disabled`` and `no method named `is_enabled``.

- [ ] **Step 4: Implement the domain struct**

In `crates/rocket-shared/src/oauth2.rs`, replace lines 19-25 with:

```rust
/// PKCE settings. The spec field is `disabled`, and an absent value means PKCE is on.
/// Data written before 2026-09 used `enabled`, so deserialization accepts both
/// and normalizes to `disabled`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", from = "OAuth2PKCEWire")]
pub struct OAuth2PKCE {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>, // "S256" | "plain"
}

impl OAuth2PKCE {
    /// Returns true unless PKCE was explicitly disabled.
    pub fn is_enabled(&self) -> bool {
        !self.disabled.unwrap_or(false)
    }
}

/// Input shape that accepts both the spec field (`disabled`) and the legacy field (`enabled`).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OAuth2PKCEWire {
    #[serde(default)]
    disabled: Option<bool>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    method: Option<String>,
}

impl From<OAuth2PKCEWire> for OAuth2PKCE {
    fn from(w: OAuth2PKCEWire) -> Self {
        // `disabled` wins when both are present, because it is the spec field.
        let is_disabled = w.disabled.or(w.enabled.map(|on| !on)).unwrap_or(false);
        OAuth2PKCE {
            disabled: is_disabled.then_some(true),
            method: w.method,
        }
    }
}
```

Run: `cargo test -p rocket-shared oauth2`
Expected: PASS.

- [ ] **Step 5: Write the failing persistence tests**

In `crates/rocket-infra/src/oc/mod.rs` tests, next to `oc_auth_oauth2_authorization_code_yaml` (~235), add:

```rust
    #[test]
    fn oc_auth_oauth2_pkce_spec_and_legacy_fields() {
        let cases = [
            ("type: oauth2\nflow: authorization_code\npkce:\n  disabled: true\n  method: S256", false),
            ("type: oauth2\nflow: authorization_code\npkce:\n  enabled: false", false),
            ("type: oauth2\nflow: authorization_code\npkce:\n  enabled: true", true),
            ("type: oauth2\nflow: authorization_code\npkce:\n  method: S256", true),
        ];
        for (yaml, expect_enabled) in cases {
            let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
            let OcAuth::Typed(typed) = auth else {
                panic!("expected typed auth for {yaml}")
            };
            let OcAuthTyped::OAuth2 { pkce, .. } = *typed else {
                panic!("expected OAuth2 for {yaml}")
            };
            assert_eq!(pkce.expect("pkce").is_enabled(), expect_enabled, "{yaml}");
        }
    }

    #[test]
    fn oc_auth_oauth2_pkce_writes_disabled_not_enabled() {
        let auth = OcAuth::Typed(Box::new(OcAuthTyped::OAuth2 {
            flow: "authorization_code".into(),
            access_token_url: None,
            refresh_token_url: None,
            authorization_url: None,
            callback_url: None,
            credentials: None,
            resource_owner: None,
            scope: None,
            state: None,
            pkce: Some(OcOAuth2PKCE {
                disabled: Some(true),
                method: Some("S256".into()),
            }),
            additional_parameters: Box::new(None),
            token_config: Box::new(None),
            settings: None,
        }));
        let yaml = serde_yaml::to_string(&auth).unwrap();
        assert!(yaml.contains("disabled: true"), "{yaml}");
        assert!(!yaml.contains("enabled:"), "{yaml}");
    }
```

In `crates/rocket-infra/src/conversions/tests.rs::oauth2_auth_code_full_roundtrip` (~919-922), change the PKCE literal to:

```rust
        pkce: Some(OAuth2PKCE {
            disabled: None,
            method: Some("S256".into()),
        }),
```

Run: `cargo test -p rocket-infra pkce`
Expected: FAIL to compile. `OcOAuth2PKCE` has no field `disabled`, and `is_enabled` is not found.

- [ ] **Step 6: Alias the persistence type to the domain type**

In `crates/rocket-infra/src/oc/auth.rs`, change line 3 to:

```rust
use rocket_shared::oauth2::{
    OAuth2AdditionalParameters, OAuth2PKCE, OAuth2Settings, OAuth2TokenConfig,
};
```

Replace the `OcOAuth2PKCE` struct (~104-110) with:

```rust
/// OAuth2 PKCE configuration. Reuses the domain type, which already has the spec
/// shape (`disabled`, `method`) and also reads the legacy `enabled` field.
pub type OcOAuth2PKCE = OAuth2PKCE;
```

In `crates/rocket-infra/src/conversions/auth.rs`:
- Remove `OAuth2PKCE` from the `use rocket_shared::oauth2::{...}` list (lines 2-5).
- Line ~115: `pkce: pkce.map(oc_pkce_to_domain),` becomes `pkce,`.
- Line ~340: `pkce.map(domain_pkce_to_oc),` becomes `pkce,`.
- Delete `fn oc_pkce_to_domain` (~152-157) and `fn domain_pkce_to_oc` (~391-396).

- [ ] **Step 7: Run the Rust tests to verify they pass**

Run: `cargo check && cargo test -p rocket-shared && cargo test -p rocket-infra`
Expected: PASS. `cargo check` at the workspace root confirms that no other crate builds `OAuth2PKCE { enabled, .. }`. `oc_auth_oauth2_authorization_code_yaml` (legacy `enabled: true`) still passes.

- [ ] **Step 8: Write the failing frontend test**

Create `src/lib/__tests__/oauth2-mapping.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { AuthState } from '@/types/pane-types';
import { type ApiOAuth2Auth, apiAuthToOAuth2State, oauth2StateToApiAuth } from '../oauth2-mapping';

type OAuth2State = NonNullable<AuthState['oauth2']>;

function authCodeState(usePkce: boolean): OAuth2State {
  return {
    grantType: 'authorization_code',
    authorizationUrl: 'https://auth.example.com/authorize',
    tokenUrl: 'https://auth.example.com/token',
    callbackUrl: '',
    clientId: 'id',
    clientSecret: 'secret',
    scope: '',
    state: '',
    username: '',
    password: '',
    clientAuthentication: 'body',
    headerPrefix: 'Bearer',
    addTokenTo: 'header',
    verifySsl: true,
    accessToken: '',
    refreshToken: '',
    expiresIn: null,
    tokenAcquiredAt: null,
    usePkce,
    useSystemBrowser: false,
    tokenSource: 'accessToken',
    tokenId: '',
    refreshTokenUrl: '',
    autoFetchToken: true,
    autoRefreshToken: false,
    authParams: [],
    tokenParams: [],
    refreshParams: [],
    idToken: '',
    tokenType: '',
    responseScope: '',
    idTokenClaims: null,
    accessTokenClaims: null,
  };
}

const authCodeApi = (pkce: ApiOAuth2Auth['pkce']): ApiOAuth2Auth => ({
  authType: 'o-auth2',
  flow: 'authorization_code',
  pkce,
});

describe('oauth2 PKCE mapping', () => {
  it('writes PKCE on as no disabled flag', () => {
    expect(oauth2StateToApiAuth(authCodeState(true)).pkce).toEqual({
      disabled: null,
      method: 'S256',
    });
  });

  it('writes PKCE off as disabled: true', () => {
    expect(oauth2StateToApiAuth(authCodeState(false)).pkce).toEqual({
      disabled: true,
      method: null,
    });
  });

  it('reads disabled: true as PKCE off', () => {
    expect(apiAuthToOAuth2State(authCodeApi({ disabled: true })).usePkce).toBe(false);
  });

  it('reads a missing or method-only pkce block as PKCE on', () => {
    expect(apiAuthToOAuth2State(authCodeApi(null)).usePkce).toBe(true);
    expect(apiAuthToOAuth2State(authCodeApi({ method: 'S256' })).usePkce).toBe(true);
  });
});
```

Run: `yarn test oauth2-mapping`
Expected: FAIL. The writer emits `{ enabled: ..., method: ... }`, and `{disabled: true}` reads back as `usePkce: true`.

- [ ] **Step 9: Update the frontend mapping**

In `src/lib/oauth2-mapping.ts`, replace `ApiOAuth2PKCE` (~17-20) with:

```ts
interface ApiOAuth2PKCE {
  disabled?: boolean | null;
  method?: string | null;
}
```

Replace the PKCE builder (~92-95) with:

```ts
  const pkce: ApiOAuth2PKCE | null =
    state.grantType === 'authorization_code'
      ? { disabled: state.usePkce ? null : true, method: state.usePkce ? 'S256' : null }
      : null;
```

Replace line ~217 with:

```ts
    usePkce: !(auth.pkce?.disabled ?? false),
```

- [ ] **Step 10: Run the frontend checks to verify they pass**

Run: `yarn tsc --noEmit && yarn check && yarn test oauth2-mapping && yarn test persisted-auth`
Expected: PASS.

- [ ] **Step 11: Commit**

```bash
git add crates/rocket-shared/src/oauth2.rs crates/rocket-infra/src/oc/auth.rs crates/rocket-infra/src/oc/mod.rs crates/rocket-infra/src/conversions/auth.rs crates/rocket-infra/src/conversions/tests.rs src/lib/oauth2-mapping.ts src/lib/__tests__/oauth2-mapping.test.ts
```

Invoke the `dev-workflow-skills:1-git-commit` skill. Target subject: `fix(auth): persist OAuth2 PKCE as spec disabled flag`.

---

### Task 4: Keep the WebSocket `settings` block

**Files:**
- Modify: `crates/rocket-infra/src/oc/websocket.rs:6,72-81`
- Test: `crates/rocket-infra/src/oc/mod.rs` (next to `oc_websocket_request_yaml`, ~572-590)
- Test: `crates/rocket-infra/src/fs_collection/tests.rs`

**Interfaces:**
- Consumes: `InheritableNumber` (`oc/auth.rs`), and the `opaque_items` test helper and repo-level opaque loading from Task 2.
- Produces: `pub struct OcWebSocketRequestSettings { pub timeout: Option<InheritableNumber>, pub keep_alive_interval: Option<InheritableNumber> }` and `OcWebSocketRequest.settings: Option<OcWebSocketRequestSettings>`.

The schema types both values as `number | "inherit"`, so they use `InheritableNumber`, not `f64`. There is no domain conversion for `OcWebSocketRequest` (it only exists inside `OpaqueItem.raw`), so this task only needs to keep the field through parse and re-serialize.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Where it conflicts with "Doc corrections (use these)" in this plan's Deferred section, trust the plan. The doc's WebSocket section is missing `settings`.

- [ ] **Step 2: Write the failing tests**

In `crates/rocket-infra/src/oc/mod.rs` tests, add:

```rust
    #[test]
    fn oc_websocket_settings_survive_item_roundtrip() {
        let yaml = "info:\n  name: Chat\n  type: websocket\nwebsocket:\n  url: wss://chat.example.com/ws\nsettings:\n  timeout: 5000\n  keepAliveInterval: inherit\n";
        let item: OcItem = serde_yaml::from_str(yaml).unwrap();
        let OcItem::WebSocket(ref ws) = item else {
            panic!("expected WebSocket, got {item:?}")
        };
        let settings = ws.settings.as_ref().expect("settings parsed");
        assert_eq!(settings.timeout, Some(InheritableNumber::Value(5000.0)));
        assert_eq!(
            settings.keep_alive_interval,
            Some(InheritableNumber::Inherit("inherit".into()))
        );

        let out = serde_yaml::to_string(&item).unwrap();
        let back: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(back["settings"]["timeout"].as_f64(), Some(5000.0), "{out}");
        assert_eq!(back["settings"]["keepAliveInterval"].as_str(), Some("inherit"), "{out}");
    }
```

Append to `crates/rocket-infra/src/fs_collection/tests.rs`:

```rust
#[test]
fn websocket_settings_preserved_in_opaque_item() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(
        dir.path().join("my-api/chat.yml"),
        "info:\n  name: Chat\n  type: websocket\nwebsocket:\n  url: wss://chat.example.com/ws\nsettings:\n  timeout: 5000\n  keepAliveInterval: 30000\n",
    )
    .unwrap();

    let col = repo.get("my-api").unwrap();
    let ws = opaque_items(&col.root);
    assert_eq!(ws.len(), 1);
    assert_eq!(ws[0].raw["settings"]["timeout"].as_f64(), Some(5000.0), "{:?}", ws[0].raw);
    assert_eq!(
        ws[0].raw["settings"]["keepAliveInterval"].as_f64(),
        Some(30000.0),
        "{:?}",
        ws[0].raw
    );
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p rocket-infra websocket_settings`
Expected: FAIL to compile with `no field `settings` on type `&OcWebSocketRequest``. The repo-level test `websocket_settings_preserved_in_opaque_item` compiles by itself and fails at runtime (`left: None, right: Some(5000.0)`) because the parse drops `settings`, so `OpaqueItem.raw` never contains it.

- [ ] **Step 4: Add the struct and field**

In `crates/rocket-infra/src/oc/websocket.rs`, change line 6 to `use super::auth::{InheritableNumber, OcAuth};` and add above `OcWebSocketRequest`:

```rust
/// WebSocket request settings. Schema: { timeout, keepAliveInterval }, each a number or "inherit".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcWebSocketRequestSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<InheritableNumber>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_alive_interval: Option<InheritableNumber>,
}
```

In `OcWebSocketRequest`, between `runtime` and `docs`, add:

```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<OcWebSocketRequestSettings>,
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo check -p rocket-infra && cargo test -p rocket-infra`
Expected: PASS, including `oc_websocket_request_yaml` (a file without `settings` still parses).

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-infra/src/oc/websocket.rs crates/rocket-infra/src/oc/mod.rs crates/rocket-infra/src/fs_collection/tests.rs
```

Invoke the `dev-workflow-skills:1-git-commit` skill. Target subject: `fix(infra): keep WebSocket request settings on round-trip`.

---

### Task 5: Never write `auth: {type: none}`, and keep "inherit" distinct from "none"

**Files:**
- Modify: `crates/rocket-infra/src/conversions/auth.rs` (add `persisted_oc_auth`)
- Modify: `crates/rocket-infra/src/conversions/request.rs:1-10,129-133`
- Modify: `crates/rocket-infra/src/fs_collection/settings.rs:8,139-176`
- Modify: `crates/rocket-infra/src/conversions/folder.rs:270-306` (`collection_to_oc_collection`, for consistency)
- Modify: `src/lib/tauri-api.ts:37-43`
- Modify: `src/lib/persisted-auth.ts:25-31,85-89`
- Test: `crates/rocket-infra/src/fs_collection/tests.rs`
- Test: `src/lib/__tests__/persisted-auth.test.ts:6-9,132-140,217-232`

**Interfaces:**
- Produces: `pub fn persisted_oc_auth(auth: Auth) -> Option<OcAuth>` in `crate::conversions`. It returns `None` for `Auth::None` and `Some(OcAuth::from(auth))` otherwise.
- Produces (TS): `Auth` gains `{ authType: 'inherit' }`. `toPersistedAuth` maps `'inherit'` to `{authType: 'inherit'}`, and `fromPersistedAuth` maps `{authType: 'inherit'}` to `{authType: 'inherit'}` whatever the fallback.

Decisions baked in:
- `OcAuthTyped::None` stays deserializable, because files already on disk contain `{type: none}`. `From<Auth> for OcAuth` is unchanged. Only the write sites go through `persisted_oc_auth`. The remaining unguarded writer is `runtime_auth` (request `runtime.auth`). That key is itself not in the schema (see Deferred), so it is left alone to avoid changing `Some(Auth::None)` round-trip semantics there.
- The frontend keeps sending `{authType: 'none'}` for "No Auth". `Request.auth` is a required `Auth` in Rust, and the backend now omits it on disk. `fromPersistedAuth` keeps mapping `'none'` to the caller's fallback. That is deliberate: existing requests saved before this fix have no `auth` key because "inherit" used to be collapsed to "none", and in collection context they must keep loading as inherit.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Where it conflicts with "Doc corrections (use these)" in this plan's Deferred section, trust the plan. `Auth` has no `"none"` member.

- [ ] **Step 2: Write the failing Rust tests**

Append to `crates/rocket-infra/src/fs_collection/tests.rs`:

```rust
#[test]
fn save_settings_omits_none_auth_instead_of_writing_type_none() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    let settings = CollectionSettings {
        auth: Some(rocket_shared::types::Auth::None),
        ..Default::default()
    };
    repo.save_settings("my-api", &settings).unwrap();

    let content = fs::read_to_string(dir.path().join("my-api/opencollection.yml")).unwrap();
    assert!(!content.contains("type: none"), "{content}");
    let raw: serde_yaml::Value = serde_yaml::from_str(&content).unwrap();
    assert!(raw.get("request").is_none(), "no empty request block: {content}");
    assert_eq!(repo.get_settings("my-api").unwrap().auth, None);
}

#[test]
fn save_settings_writes_inherit_as_spec_string() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    let settings = CollectionSettings {
        auth: Some(rocket_shared::types::Auth::Inherit),
        ..Default::default()
    };
    repo.save_settings("my-api", &settings).unwrap();

    let content = fs::read_to_string(dir.path().join("my-api/opencollection.yml")).unwrap();
    assert!(content.contains("auth: inherit"), "{content}");
    assert_eq!(
        repo.get_settings("my-api").unwrap().auth,
        Some(rocket_shared::types::Auth::Inherit)
    );
}

#[test]
fn legacy_type_none_collection_auth_still_loads() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(
        dir.path().join("my-api/opencollection.yml"),
        "opencollection: \"1.0.0\"\ninfo:\n  name: my-api\nrequest:\n  auth:\n    type: none\n",
    )
    .unwrap();
    assert_eq!(
        repo.get_settings("my-api").unwrap().auth,
        Some(rocket_shared::types::Auth::None)
    );
}

#[test]
fn save_request_omits_none_auth() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    let req = rocket_collection::Request::new("Ping", HttpMethod::Get, "https://example.com");
    repo.save_request("my-api", "ping.yml", &req).unwrap();
    let content = fs::read_to_string(dir.path().join("my-api/ping.yml")).unwrap();
    assert!(!content.contains("auth"), "{content}");
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p rocket-infra save_settings_omits_none_auth_instead_of_writing_type_none`
Expected: FAIL with `type: none` present in `opencollection.yml`. `save_settings_writes_inherit_as_spec_string`, `legacy_type_none_collection_auth_still_loads` and `save_request_omits_none_auth` already pass. They pin behavior the change must keep.

- [ ] **Step 4: Add `persisted_oc_auth` and use it at the write sites**

In `crates/rocket-infra/src/conversions/auth.rs`, add after `impl From<Auth> for OcAuth { ... }`:

```rust
/// Converts a domain auth into the value written to disk. The spec's `Auth`
/// has no "none" member, so `Auth::None` is written by leaving the field out.
/// `OcAuthTyped::None` is still read, for files written before this rule.
pub fn persisted_oc_auth(auth: Auth) -> Option<OcAuth> {
    match auth {
        Auth::None => None,
        other => Some(OcAuth::from(other)),
    }
}
```

In `crates/rocket-infra/src/conversions/request.rs`, add `use super::auth::persisted_oc_auth;` next to the other `super::` imports and replace ~129-133 with:

```rust
        auth: persisted_oc_auth(req.auth.clone()),
```

In `crates/rocket-infra/src/fs_collection/settings.rs`, change line 8 to `use crate::oc::{OcCollection, OcHttpRequestHeader, OcInfo, OcRequestDefaults, OcVariable};`, add `use crate::conversions::persisted_oc_auth;`, and replace ~139-141 plus the `auth:` line at ~158 so that the block reads:

```rust
    // Build OcRequestDefaults from settings. "No auth" is written by omission.
    let auth = settings.auth.clone().and_then(persisted_oc_auth);
    let has_defaults =
        !settings.headers.is_empty() || auth.is_some() || !settings.variables.is_empty();

    oc.request = if has_defaults {
        Some(OcRequestDefaults {
            headers: if settings.headers.is_empty() {
                None
            } else {
                Some(
                    settings
                        .headers
                        .iter()
                        .cloned()
                        .map(OcHttpRequestHeader::from)
                        .collect(),
                )
            },
            metadata: None,
            auth,
            variables: if settings.variables.is_empty() {
                None
            } else {
                Some(
                    settings
                        .variables
                        .iter()
                        .cloned()
                        .map(OcVariable::from)
                        .collect(),
                )
            },
            scripts: None,
            settings: None,
        })
    } else {
        None
    };
```

In `crates/rocket-infra/src/conversions/folder.rs::collection_to_oc_collection`, add `use super::auth::persisted_oc_auth;` at the top of the file. Replace the start of the `let request = { ... }` block (~270-273) with:

```rust
    let auth = col.settings.auth.and_then(persisted_oc_auth);
    let request = {
        let has_defaults = !col.settings.headers.is_empty()
            || auth.is_some()
            || !col.settings.variables.is_empty();
```

Replace `auth: col.settings.auth.map(OcAuth::from),` (~288) with `auth,`.

- [ ] **Step 5: Run the Rust tests to verify they pass**

Run: `cargo check -p rocket-infra && cargo test -p rocket-infra`
Expected: PASS, including `settings_roundtrip`, `settings_file_not_counted_as_request` and the four new tests.

- [ ] **Step 6: Write the failing frontend tests**

In `src/lib/__tests__/persisted-auth.test.ts`, replace the first test in `describe('toPersistedAuth', ...)` (~6-9) with:

```ts
  it('maps none to authType none (the backend omits it on disk)', () => {
    expect(toPersistedAuth({ authType: 'none' })).toEqual({ authType: 'none' });
  });

  it('keeps inherit as authType inherit instead of collapsing it to none', () => {
    expect(toPersistedAuth({ authType: 'inherit' })).toEqual({ authType: 'inherit' });
  });
```

In `describe('fromPersistedAuth', ...)`, add:

```ts
  it('reads an explicit inherit as inherit even where the fallback is none', () => {
    expect(fromPersistedAuth({ authType: 'inherit' } as Auth, 'none')).toEqual({
      authType: 'inherit',
    });
  });

  it('keeps mapping none to the caller fallback, so older requests saved without auth still inherit', () => {
    expect(fromPersistedAuth({ authType: 'none' }, 'inherit')).toEqual({ authType: 'inherit' });
    expect(fromPersistedAuth({ authType: 'none' }, 'none')).toEqual({ authType: 'none' });
  });
```

In the round-trip `cases` array (~218-232), add `{ authType: 'inherit' } as Auth,` after `{ authType: 'none' },`.

Run: `yarn test persisted-auth`
Expected: FAIL. `toPersistedAuth({authType:'inherit'})` returns `{authType:'none'}`, and `fromPersistedAuth({authType:'inherit'}, 'none')` returns `{authType:'none'}`.

- [ ] **Step 7: Update the TS type and mapping**

In `src/lib/tauri-api.ts`, add a member to the `Auth` union (~37-43) right after `| { authType: 'none' }`:

```ts
  | { authType: 'inherit' }
```

In `src/lib/persisted-auth.ts`, add this directly after `const authType = a.authType as string;` (~31):

```ts
  if (authType === 'inherit') return { authType: 'inherit' };
```

Replace the combined case (~87-89) with:

```ts
    case 'none':
      return { authType: 'none' };
    case 'inherit':
      return { authType: 'inherit' };
```

- [ ] **Step 8: Run the frontend checks to verify they pass**

Run: `yarn tsc --noEmit && yarn check && yarn test persisted-auth && yarn test pane-utils && yarn test request-save-mapper && yarn test auto-save`
Expected: PASS. Adding `'inherit'` to `Auth` causes no new type errors (checked while writing this plan).

- [ ] **Step 9: Commit**

```bash
git add crates/rocket-infra/src/conversions/auth.rs crates/rocket-infra/src/conversions/request.rs crates/rocket-infra/src/conversions/folder.rs crates/rocket-infra/src/fs_collection/settings.rs crates/rocket-infra/src/fs_collection/tests.rs src/lib/tauri-api.ts src/lib/persisted-auth.ts src/lib/__tests__/persisted-auth.test.ts
```

Invoke the `dev-workflow-skills:1-git-commit` skill. Target subject: `fix(auth): omit none auth on disk and persist inherit explicitly`.

---

### Task 6: Omit `clientSecret` for the OAuth2 implicit flow

**Files:**
- Modify: `crates/rocket-infra/src/oc/auth.rs:87-95`
- Modify: `crates/rocket-infra/src/conversions/auth.rs:137-143,360-364,376-382`
- Modify: `crates/rocket-infra/src/conversions/tests.rs:239-243`
- Test: `crates/rocket-infra/src/conversions/tests.rs`

**Interfaces:**
- Produces: `OcOAuth2Credentials { client_id: String, client_secret: Option<String>, placement: Option<String> }`. `clientSecret` is omitted when `None`.
- Domain `OAuth2ClientCredentials.client_secret` stays `String`. `None` on read becomes `""`, and every non-implicit flow writes `Some(secret)`.

Construction sites checked (grep `OcOAuth2Credentials`): `conversions/auth.rs` (`oc_creds_to_domain`, the implicit arm of `domain_oauth2_to_oc_fields`, `domain_creds_to_oc`) and `conversions/tests.rs:239`. `reqwest_executor.rs` uses the domain `OAuth2ClientCredentials`, which does not change.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Where it conflicts with "Doc corrections (use these)" in this plan's Deferred section, trust the plan. Implicit `credentials` is `{clientId}` only.

- [ ] **Step 2: Write the failing tests**

Append to `crates/rocket-infra/src/conversions/tests.rs`:

```rust
#[test]
fn oauth2_implicit_writes_no_client_secret() {
    use rocket_shared::oauth2::OAuth2Flow;
    let auth = Auth::OAuth2(Box::new(OAuth2Flow::Implicit {
        authorization_url: "https://auth.example.com/authorize".into(),
        callback_url: None,
        client_id: "spa-client".into(),
        scope: None,
        state: None,
        additional_parameters: None,
        token_config: None,
        settings: None,
    }));
    let yaml = serde_yaml::to_string(&OcAuth::from(auth.clone())).unwrap();
    assert!(yaml.contains("clientId: spa-client"), "{yaml}");
    assert!(!yaml.contains("clientSecret"), "implicit credentials hold only clientId: {yaml}");
    let back: Auth = serde_yaml::from_str::<OcAuth>(&yaml).unwrap().into();
    assert_eq!(back, auth);
}

#[test]
fn oauth2_implicit_legacy_empty_client_secret_still_loads() {
    use rocket_shared::oauth2::OAuth2Flow;
    let yaml = "type: oauth2\nflow: implicit\nauthorizationUrl: https://auth.example.com/authorize\ncredentials:\n  clientId: spa-client\n  clientSecret: ''\n";
    let auth: Auth = serde_yaml::from_str::<OcAuth>(yaml).unwrap().into();
    match auth {
        Auth::OAuth2(flow) => assert!(
            matches!(*flow, OAuth2Flow::Implicit { ref client_id, .. } if client_id == "spa-client"),
            "{flow:?}"
        ),
        other => panic!("expected OAuth2, got {other:?}"),
    }
}

#[test]
fn oauth2_client_credentials_keeps_client_secret() {
    use rocket_shared::oauth2::{OAuth2ClientCredentials, OAuth2Flow};
    let auth = Auth::OAuth2(Box::new(OAuth2Flow::ClientCredentials {
        access_token_url: "https://auth.example.com/token".into(),
        refresh_token_url: None,
        credentials: OAuth2ClientCredentials {
            client_id: "svc".into(),
            client_secret: "s3cret".into(),
            placement: None,
        },
        scope: None,
        additional_parameters: None,
        token_config: None,
        settings: None,
    }));
    let yaml = serde_yaml::to_string(&OcAuth::from(auth.clone())).unwrap();
    assert!(yaml.contains("clientSecret: s3cret"), "{yaml}");
    let back: Auth = serde_yaml::from_str::<OcAuth>(&yaml).unwrap().into();
    assert_eq!(back, auth);
}

#[test]
fn oauth2_client_credentials_without_client_secret_key_parses() {
    use rocket_shared::oauth2::OAuth2Flow;
    let yaml = "type: oauth2\nflow: client_credentials\naccessTokenUrl: https://auth.example.com/token\ncredentials:\n  clientId: svc\n";
    let oc: OcAuth = serde_yaml::from_str(yaml).expect("a missing clientSecret must not reject the file");
    match Auth::from(oc) {
        Auth::OAuth2(flow) => match *flow {
            OAuth2Flow::ClientCredentials { credentials, .. } => {
                assert_eq!(credentials.client_id, "svc");
                assert_eq!(credentials.client_secret, "");
            }
            other => panic!("expected client_credentials, got {other:?}"),
        },
        other => panic!("expected OAuth2, got {other:?}"),
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p rocket-infra oauth2_`
Expected: FAIL. `oauth2_implicit_writes_no_client_secret` finds `clientSecret: ''` in the YAML. `oauth2_client_credentials_without_client_secret_key_parses` panics on `expect` with "data did not match any variant of untagged enum OcAuth".

- [ ] **Step 4: Make `client_secret` optional**

In `crates/rocket-infra/src/oc/auth.rs`, replace `pub client_secret: String,` (~92) with:

```rust
    /// Absent for the implicit flow, whose spec credentials hold only `clientId`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
```

In `crates/rocket-infra/src/conversions/auth.rs`:
- `oc_creds_to_domain` (~140): `client_secret: c.client_secret,` becomes `client_secret: c.client_secret.unwrap_or_default(),`.
- The implicit arm of `domain_oauth2_to_oc_fields` (~362): `client_secret: String::new(),` becomes `client_secret: None,`.
- `domain_creds_to_oc` (~379): `client_secret: c.client_secret,` becomes `client_secret: Some(c.client_secret),`.

In `crates/rocket-infra/src/conversions/tests.rs::auth_oauth2_client_credentials_oc_to_domain` (~241): `client_secret: "s".into(),` becomes `client_secret: Some("s".into()),`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo check -p rocket-infra && cargo test -p rocket-infra`
Expected: PASS, including `oauth2_auth_code_full_roundtrip` and `oc_auth_oauth2_authorization_code_yaml`.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-infra/src/oc/auth.rs crates/rocket-infra/src/conversions/auth.rs crates/rocket-infra/src/conversions/tests.rs
```

Invoke the `dev-workflow-skills:1-git-commit` skill. Target subject: `fix(auth): omit clientSecret for OAuth2 implicit credentials`.

---

### Task 7: Schema allow-list regression test

**Files:**
- Create: `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs`
- Modify: `crates/rocket-infra/src/fs_collection/mod.rs:19-20` (register the test module)

**Interfaces:**
- Consumes: everything from Tasks 1 to 6. The spec `folder.yml` shape (1), `OpaqueItem` loading (2), PKCE `disabled` (3), WebSocket `settings` (4), `none` omitted (5) and implicit credentials without a secret (6) are all needed for the main test to pass with only the `KNOWN_DEFERRED` exemptions.
- Produces: `KNOWN_DEFERRED`, the single list of schema violations still tracked by the Deferred section. Delete an entry the moment its fix lands.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Where it conflicts with "Doc corrections (use these)" in this plan's Deferred section, trust the plan. The allow-lists below come from the live schema, not from that doc.

- [ ] **Step 2: Write the test module**

Create `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs`:

```rust
//! Regression guard for OpenCollection v1.0.0 `additionalProperties: false`.
//! The allow-lists are copied from
//! https://schema.opencollection.com/opencollection/v1.0.0.json (checked 2026-09-26).
//! A failure means Rocket wrote a key that the schema rejects.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use dashmap::DashMap;
use rocket_collection::settings::SandboxMode;
use rocket_collection::{
    CollectionItem, CollectionRepository, CollectionSettings, CollectionVariable, Request,
};
use rocket_shared::oauth2::{
    OAuth2ClientCredentials, OAuth2Flow, OAuth2PKCE, OAuth2ResourceOwner, OAuth2Settings,
};
use rocket_shared::types::{
    Auth, Body, BodyMode, FormDataEntry, FormDataType, Header, HttpMethod, PathParam, QueryParam,
    RequestSettingValue, RequestSettings,
};
use serde_yaml::Value;
use tempfile::TempDir;

use super::FsCollectionRepo;

/// Keys that still break the schema and are tracked in the plan's Deferred
/// section. Remove an entry as soon as its fix lands.
const KNOWN_DEFERRED: &[&str] = &[
    "HttpRequest.uid",
    "FolderInfo.uid",
    "HttpRequestSettings.verifySsl",
    "HttpRequestRuntime.auth",
    "Variable.initial",
    "OAuth2Settings.verifySsl",
    "OAuth2Settings.useSystemBrowser",
    "OAuth2AdditionalParameter.enabled",
];

const COLLECTION_INFO: &[&str] = &["name", "summary", "version", "authors"];
const ITEM_INFO: &[&str] = &["name", "description", "type", "seq", "tags"];
const FOLDER: &[&str] = &["info", "items", "request", "docs"];
const REQUEST_DEFAULTS: &[&str] = &["headers", "metadata", "auth", "variables", "scripts", "settings"];
const HTTP_REQUEST: &[&str] = &["info", "http", "runtime", "settings", "examples", "docs"];
const HTTP_DETAILS: &[&str] = &["method", "url", "headers", "params", "body", "auth"];
const HTTP_RUNTIME: &[&str] = &["variables", "scripts", "assertions", "actions"];
const HTTP_SETTINGS: &[&str] = &["encodeUrl", "timeout", "followRedirects", "maxRedirects"];
const HEADER: &[&str] = &["name", "value", "description", "disabled"];
const PARAM: &[&str] = &["name", "value", "description", "type", "disabled"];
const BODY: &[&str] = &["type", "data"];
const FORM_FIELD: &[&str] = &["name", "value", "description", "disabled"];
const MULTIPART_PART: &[&str] = &["name", "type", "value", "description", "contentType", "disabled"];
const FILE_VARIANT: &[&str] = &["filePath", "contentType", "selected"];
const VARIABLE: &[&str] = &["name", "value", "description", "disabled"];
const SCRIPT: &[&str] = &["type", "code"];
const GRAPHQL_REQUEST: &[&str] = &["info", "graphql", "runtime", "settings", "docs"];
const GRAPHQL_DETAILS: &[&str] = &["method", "url", "headers", "params", "body", "auth"];
const GRAPHQL_BODY: &[&str] = &["query", "variables"];
const WEBSOCKET_REQUEST: &[&str] = &["info", "websocket", "runtime", "settings", "docs"];
const WEBSOCKET_DETAILS: &[&str] = &["url", "headers", "message", "auth"];
const WEBSOCKET_SETTINGS: &[&str] = &["timeout", "keepAliveInterval"];
const WEBSOCKET_MESSAGE: &[&str] = &["type", "data"];
const AUTH_USER_PASS: &[&str] = &["type", "username", "password"];
const AUTH_BEARER: &[&str] = &["type", "token"];
const AUTH_APIKEY: &[&str] = &["type", "key", "value", "placement"];
const AUTH_NTLM: &[&str] = &["type", "username", "password", "domain"];
const AUTH_AWSV4: &[&str] = &[
    "type", "accessKeyId", "secretAccessKey", "sessionToken", "service", "region", "profileName",
];
const OAUTH2_CLIENT_CREDENTIALS_FLOW: &[&str] = &[
    "type", "flow", "accessTokenUrl", "refreshTokenUrl", "credentials", "scope",
    "additionalParameters", "tokenConfig", "settings",
];
const OAUTH2_PASSWORD_FLOW: &[&str] = &[
    "type", "flow", "accessTokenUrl", "refreshTokenUrl", "credentials", "resourceOwner", "scope",
    "additionalParameters", "tokenConfig", "settings",
];
const OAUTH2_AUTH_CODE_FLOW: &[&str] = &[
    "type", "flow", "authorizationUrl", "accessTokenUrl", "refreshTokenUrl", "callbackUrl",
    "credentials", "scope", "state", "pkce", "additionalParameters", "tokenConfig", "settings",
];
const OAUTH2_IMPLICIT_FLOW: &[&str] = &[
    "type", "flow", "authorizationUrl", "callbackUrl", "credentials", "scope", "state",
    "additionalParameters", "tokenConfig", "settings",
];
const OAUTH2_CLIENT_CREDENTIALS: &[&str] = &["clientId", "clientSecret", "placement"];
const OAUTH2_IMPLICIT_CREDENTIALS: &[&str] = &["clientId"];
const OAUTH2_RESOURCE_OWNER: &[&str] = &["username", "password"];
const OAUTH2_PKCE: &[&str] = &["disabled", "method"];
const OAUTH2_SETTINGS: &[&str] = &["autoFetchToken", "autoRefreshToken"];
const OAUTH2_TOKEN_CONFIG: &[&str] = &["id", "placement", "source"];
const OAUTH2_ADDITIONAL_PARAMETER: &[&str] = &["name", "value", "placement"];
const OAUTH2_PARAMS_TOKEN_ONLY: &[&str] = &["accessTokenRequest", "refreshTokenRequest"];
const OAUTH2_PARAMS_AUTH_CODE: &[&str] =
    &["authorizationRequest", "accessTokenRequest", "refreshTokenRequest"];
const OAUTH2_PARAMS_IMPLICIT: &[&str] = &["authorizationRequest"];

#[derive(Default)]
struct Violations(Vec<String>);

impl Violations {
    /// Records every key of `value` that is neither allowed for `ty` nor a known deferred key.
    fn keys(&mut self, ty: &str, at: &str, value: &Value, allowed: &[&str]) {
        let Some(map) = value.as_mapping() else {
            return;
        };
        for (key, _) in map.iter() {
            let key = key.as_str().unwrap_or("<non-string key>");
            let tagged = format!("{ty}.{key}");
            if !allowed.contains(&key) && !KNOWN_DEFERRED.contains(&tagged.as_str()) {
                self.0.push(format!("{at}: `{key}` is not allowed on {ty}"));
            }
        }
    }
}

fn seq<'a>(value: Option<&'a Value>) -> impl Iterator<Item = &'a Value> + 'a {
    value.and_then(Value::as_sequence).into_iter().flatten()
}

fn auth_keys(auth_type: &str, flow: Option<&str>) -> Option<&'static [&'static str]> {
    match (auth_type, flow) {
        ("basic", _) | ("digest", _) | ("wsse", _) => Some(AUTH_USER_PASS),
        ("bearer", _) => Some(AUTH_BEARER),
        ("apikey", _) => Some(AUTH_APIKEY),
        ("ntlm", _) => Some(AUTH_NTLM),
        ("awsv4", _) => Some(AUTH_AWSV4),
        ("oauth2", Some("client_credentials")) => Some(OAUTH2_CLIENT_CREDENTIALS_FLOW),
        ("oauth2", Some("resource_owner_password_credentials")) => Some(OAUTH2_PASSWORD_FLOW),
        ("oauth2", Some("authorization_code")) => Some(OAUTH2_AUTH_CODE_FLOW),
        ("oauth2", Some("implicit")) => Some(OAUTH2_IMPLICIT_FLOW),
        _ => None,
    }
}

fn check_auth(v: &mut Violations, at: &str, auth: &Value) {
    if auth.as_str() == Some("inherit") {
        return;
    }
    let auth_type = auth.get("type").and_then(Value::as_str).unwrap_or("");
    let flow = auth.get("flow").and_then(Value::as_str);
    let Some(allowed) = auth_keys(auth_type, flow) else {
        v.0.push(format!("{at}: auth type `{auth_type}` (flow {flow:?}) is not a spec Auth member"));
        return;
    };
    v.keys(&format!("Auth[{auth_type}]"), at, auth, allowed);
    if auth_type != "oauth2" {
        return;
    }
    let flow = flow.unwrap_or("");
    if let Some(creds) = auth.get("credentials") {
        let allowed = if flow == "implicit" {
            OAUTH2_IMPLICIT_CREDENTIALS
        } else {
            OAUTH2_CLIENT_CREDENTIALS
        };
        v.keys("OAuth2Credentials", at, creds, allowed);
    }
    if let Some(ro) = auth.get("resourceOwner") {
        v.keys("OAuth2ResourceOwner", at, ro, OAUTH2_RESOURCE_OWNER);
    }
    if let Some(pkce) = auth.get("pkce") {
        v.keys("OAuth2PKCE", at, pkce, OAUTH2_PKCE);
    }
    if let Some(settings) = auth.get("settings") {
        v.keys("OAuth2Settings", at, settings, OAUTH2_SETTINGS);
    }
    if let Some(tc) = auth.get("tokenConfig") {
        v.keys("OAuth2TokenConfig", at, tc, OAUTH2_TOKEN_CONFIG);
    }
    if let Some(ap) = auth.get("additionalParameters") {
        let groups = match flow {
            "authorization_code" => OAUTH2_PARAMS_AUTH_CODE,
            "implicit" => OAUTH2_PARAMS_IMPLICIT,
            _ => OAUTH2_PARAMS_TOKEN_ONLY,
        };
        v.keys("OAuth2AdditionalParameters", at, ap, groups);
        if let Some(map) = ap.as_mapping() {
            for (_, list) in map.iter() {
                for p in seq(Some(list)) {
                    v.keys("OAuth2AdditionalParameter", at, p, OAUTH2_ADDITIONAL_PARAMETER);
                }
            }
        }
    }
}

fn check_http_body(v: &mut Violations, at: &str, body: &Value) {
    v.keys("HttpRequestBody", at, body, BODY);
    let (ty, allowed) = match body.get("type").and_then(Value::as_str) {
        Some("form-urlencoded") => ("FormUrlEncodedField", FORM_FIELD),
        Some("multipart-form") => ("MultipartFormPart", MULTIPART_PART),
        Some("file") => ("FileBodyVariant", FILE_VARIANT),
        _ => return,
    };
    for entry in seq(body.get("data")) {
        v.keys(ty, at, entry, allowed);
    }
}

fn check_request_defaults(v: &mut Violations, at: &str, req: &Value) {
    v.keys("RequestDefaults", at, req, REQUEST_DEFAULTS);
    for h in seq(req.get("headers")) {
        v.keys("HttpRequestHeader", at, h, HEADER);
    }
    for var in seq(req.get("variables")) {
        v.keys("Variable", at, var, VARIABLE);
    }
    if let Some(auth) = req.get("auth") {
        check_auth(v, at, auth);
    }
}

fn check_collection_root(v: &mut Violations, at: &str, doc: &Value) {
    // The root object allows extra keys (it is where `extensions` lives), but its `info` and `request` do not.
    if let Some(info) = doc.get("info") {
        v.keys("Info", at, info, COLLECTION_INFO);
    }
    if let Some(req) = doc.get("request") {
        check_request_defaults(v, at, req);
    }
}

fn check_folder(v: &mut Violations, at: &str, doc: &Value) {
    v.keys("Folder", at, doc, FOLDER);
    if let Some(info) = doc.get("info") {
        v.keys("FolderInfo", at, info, ITEM_INFO);
    }
    if let Some(req) = doc.get("request") {
        check_request_defaults(v, at, req);
    }
}

fn check_http_request(v: &mut Violations, at: &str, doc: &Value) {
    v.keys("HttpRequest", at, doc, HTTP_REQUEST);
    if let Some(info) = doc.get("info") {
        v.keys("HttpRequestInfo", at, info, ITEM_INFO);
    }
    if let Some(http) = doc.get("http") {
        v.keys("HttpRequestDetails", at, http, HTTP_DETAILS);
        for h in seq(http.get("headers")) {
            v.keys("HttpRequestHeader", at, h, HEADER);
        }
        for p in seq(http.get("params")) {
            v.keys("HttpRequestParam", at, p, PARAM);
        }
        if let Some(body) = http.get("body") {
            check_http_body(v, at, body);
        }
        if let Some(auth) = http.get("auth") {
            check_auth(v, at, auth);
        }
    }
    if let Some(rt) = doc.get("runtime") {
        v.keys("HttpRequestRuntime", at, rt, HTTP_RUNTIME);
        for var in seq(rt.get("variables")) {
            v.keys("Variable", at, var, VARIABLE);
        }
        for s in seq(rt.get("scripts")) {
            v.keys("Script", at, s, SCRIPT);
        }
    }
    if let Some(settings) = doc.get("settings") {
        v.keys("HttpRequestSettings", at, settings, HTTP_SETTINGS);
    }
}

fn check_graphql_request(v: &mut Violations, at: &str, doc: &Value) {
    v.keys("GraphQLRequest", at, doc, GRAPHQL_REQUEST);
    if let Some(info) = doc.get("info") {
        v.keys("GraphQLRequestInfo", at, info, ITEM_INFO);
    }
    if let Some(gql) = doc.get("graphql") {
        v.keys("GraphQLRequestDetails", at, gql, GRAPHQL_DETAILS);
        for h in seq(gql.get("headers")) {
            v.keys("HttpRequestHeader", at, h, HEADER);
        }
        for p in seq(gql.get("params")) {
            v.keys("HttpRequestParam", at, p, PARAM);
        }
        if let Some(body) = gql.get("body").filter(|b| b.is_mapping()) {
            v.keys("GraphQLBody", at, body, GRAPHQL_BODY);
        }
        if let Some(auth) = gql.get("auth") {
            check_auth(v, at, auth);
        }
    }
    if let Some(settings) = doc.get("settings") {
        v.keys("GraphQLRequestSettings", at, settings, HTTP_SETTINGS);
    }
}

fn check_websocket_request(v: &mut Violations, at: &str, doc: &Value) {
    v.keys("WebSocketRequest", at, doc, WEBSOCKET_REQUEST);
    if let Some(info) = doc.get("info") {
        v.keys("WebSocketRequestInfo", at, info, ITEM_INFO);
    }
    if let Some(ws) = doc.get("websocket") {
        v.keys("WebSocketRequestDetails", at, ws, WEBSOCKET_DETAILS);
        for h in seq(ws.get("headers")) {
            v.keys("HttpRequestHeader", at, h, HEADER);
        }
        if let Some(msg) = ws.get("message").filter(|m| m.is_mapping()) {
            v.keys("WebSocketMessage", at, msg, WEBSOCKET_MESSAGE);
        }
        if let Some(auth) = ws.get("auth") {
            check_auth(v, at, auth);
        }
    }
    if let Some(settings) = doc.get("settings") {
        v.keys("WebSocketRequestSettings", at, settings, WEBSOCKET_SETTINGS);
    }
}

fn setup() -> (TempDir, FsCollectionRepo) {
    let dir = TempDir::new().expect("tempdir");
    let repo = FsCollectionRepo::new(dir.path().to_path_buf(), Arc::new(DashMap::new()));
    (dir, repo)
}

fn read_yaml(path: &Path) -> Value {
    serde_yaml::from_str(&fs::read_to_string(path).expect("read yaml file")).expect("valid yaml")
}

fn sample_bodies() -> Vec<(&'static str, Body)> {
    let raw = |mode: BodyMode, content: &str| Body {
        mode,
        content: Some(content.into()),
        form_data: None,
        file_path: None,
    };
    let form = |mode: BodyMode, entry_type: FormDataType, content_type: Option<String>| Body {
        mode,
        content: None,
        form_data: Some(vec![FormDataEntry {
            key: "k".into(),
            value: "v".into(),
            entry_type,
            enabled: false,
            content_type,
            description: None,
        }]),
        file_path: None,
    };
    vec![
        ("json", raw(BodyMode::Json, "{}")),
        ("xml", raw(BodyMode::Xml, "<a/>")),
        ("text", raw(BodyMode::Text, "hi")),
        ("sparql", raw(BodyMode::Sparql, "SELECT * WHERE {}")),
        ("form-urlencoded", form(BodyMode::FormUrlEncoded, FormDataType::Text, None)),
        (
            "multipart",
            form(BodyMode::FormData, FormDataType::File, Some("image/png".into())),
        ),
        (
            "file",
            Body {
                mode: BodyMode::Binary,
                content: None,
                form_data: None,
                file_path: Some("/tmp/upload.bin".into()),
            },
        ),
    ]
}

fn sample_auths() -> Vec<(&'static str, Auth)> {
    let creds = || OAuth2ClientCredentials {
        client_id: "cid".into(),
        client_secret: "csecret".into(),
        placement: Some("basic_auth_header".into()),
    };
    let oauth_settings = || {
        Some(OAuth2Settings {
            auto_fetch_token: Some(true),
            auto_refresh_token: Some(false),
            verify_ssl: None,
            use_system_browser: None,
        })
    };
    vec![
        ("basic", Auth::Basic { username: "u".into(), password: "p".into() }),
        ("bearer", Auth::Bearer { token: "t".into() }),
        (
            "apikey",
            Auth::ApiKey { key: "X-Key".into(), value: "v".into(), placement: "header".into() },
        ),
        ("digest", Auth::Digest { username: "u".into(), password: "p".into() }),
        (
            "ntlm",
            Auth::Ntlm { username: "u".into(), password: "p".into(), domain: "CORP".into() },
        ),
        ("wsse", Auth::Wsse { username: "u".into(), password: "p".into() }),
        (
            "awsv4",
            Auth::AwsSigV4 {
                access_key: "AKIA".into(),
                secret_key: "s".into(),
                region: "us-east-1".into(),
                service: "execute-api".into(),
                session_token: Some("st".into()),
                profile_name: None,
            },
        ),
        ("inherit", Auth::Inherit),
        ("none", Auth::None),
        (
            "oauth2-client-credentials",
            Auth::OAuth2(Box::new(OAuth2Flow::ClientCredentials {
                access_token_url: "https://auth.example.com/token".into(),
                refresh_token_url: None,
                credentials: creds(),
                scope: Some("read".into()),
                additional_parameters: None,
                token_config: None,
                settings: oauth_settings(),
            })),
        ),
        (
            "oauth2-password",
            Auth::OAuth2(Box::new(OAuth2Flow::ResourceOwnerPassword {
                access_token_url: "https://auth.example.com/token".into(),
                refresh_token_url: None,
                credentials: creds(),
                resource_owner: Some(OAuth2ResourceOwner {
                    username: "u".into(),
                    password: "p".into(),
                }),
                scope: None,
                additional_parameters: None,
                token_config: None,
                settings: oauth_settings(),
            })),
        ),
        (
            "oauth2-auth-code",
            Auth::OAuth2(Box::new(OAuth2Flow::AuthorizationCode {
                authorization_url: "https://auth.example.com/authorize".into(),
                access_token_url: "https://auth.example.com/token".into(),
                refresh_token_url: None,
                callback_url: Some("http://localhost/cb".into()),
                credentials: creds(),
                scope: Some("openid".into()),
                state: Some("xyz".into()),
                pkce: Some(OAuth2PKCE { disabled: Some(true), method: Some("S256".into()) }),
                additional_parameters: None,
                token_config: None,
                settings: oauth_settings(),
            })),
        ),
        (
            "oauth2-implicit",
            Auth::OAuth2(Box::new(OAuth2Flow::Implicit {
                authorization_url: "https://auth.example.com/authorize".into(),
                callback_url: None,
                client_id: "cid".into(),
                scope: None,
                state: None,
                additional_parameters: None,
                token_config: None,
                settings: oauth_settings(),
            })),
        ),
    ]
}

fn full_request(name: &str, body: Body, auth: Auth) -> Request {
    let mut req = Request::new(name, HttpMethod::Post, "https://api.example.com/users/:id");
    req.headers = vec![
        Header::new("Accept", "application/json"),
        Header { key: "X-Off".into(), value: "1".into(), enabled: false, description: None },
    ];
    req.query_params = vec![QueryParam {
        key: "page".into(),
        value: "1".into(),
        enabled: true,
        description: None,
    }];
    req.path_params = vec![PathParam { name: "id".into(), value: "42".into(), description: None }];
    req.body = Some(body);
    req.auth = auth;
    req.pre_request_script = Some("console.log('pre')".into());
    req.settings = Some(RequestSettings {
        encode_url: Some(RequestSettingValue::Value(true)),
        timeout: Some(RequestSettingValue::Value(3000.0)),
        follow_redirects: Some(RequestSettingValue::Inherit("inherit".into())),
        max_redirects: Some(RequestSettingValue::Value(5.0)),
        verify_ssl: None,
    });
    req.variables = vec![CollectionVariable {
        key: "rv".into(),
        value: "1".into(),
        initial_value: String::new(),
        enabled: true,
        secret: false,
    }];
    req
}

const GRAPHQL_FIXTURE: &str = "info:\n  name: List Users\n  type: graphql\n  seq: 3\ngraphql:\n  method: POST\n  url: https://api.example.com/graphql\n  headers:\n  - name: Accept\n    value: application/json\n  body:\n    query: '{ users { id } }'\n    variables: '{\"first\": 10}'\n  auth:\n    type: bearer\n    token: t\nsettings:\n  timeout: 1000\ndocs: GraphQL docs\n";

const WEBSOCKET_FIXTURE: &str = "info:\n  name: Chat\n  type: websocket\nwebsocket:\n  url: wss://chat.example.com/ws\n  headers:\n  - name: Origin\n    value: https://example.com\n  message:\n    type: json\n    data: '{\"hello\": true}'\nsettings:\n  timeout: 5000\n  keepAliveInterval: 30000\ndocs: WS docs\n";

#[test]
fn written_collection_files_only_use_schema_keys() {
    let (dir, repo) = setup();
    repo.create("api").unwrap();
    repo.save_settings(
        "api",
        &CollectionSettings {
            docs: Some("docs".into()),
            auth: Some(Auth::None),
            headers: vec![Header::new("X-Tenant", "acme")],
            variables: vec![CollectionVariable {
                key: "base".into(),
                value: "https://x".into(),
                initial_value: "https://x".into(),
                enabled: true,
                secret: false,
            }],
            sandbox_mode: SandboxMode::Developer,
        },
    )
    .unwrap();
    repo.create_folder("api", "users").unwrap();
    repo.save_folder_variables(
        "api",
        "users",
        vec![CollectionVariable {
            key: "fv".into(),
            value: "x".into(),
            initial_value: String::new(),
            enabled: false,
            secret: false,
        }],
    )
    .unwrap();

    let mut written = Vec::new();
    for (body_name, body) in sample_bodies() {
        for (auth_name, auth) in sample_auths() {
            let req = full_request(&format!("{body_name} {auth_name}"), body.clone(), auth);
            // save_request may normalize the file name, so keep the path it reports.
            let rel = repo
                .save_request("api", &format!("users/{body_name}-{auth_name}.yml"), &req)
                .unwrap();
            written.push(rel);
        }
    }

    let col_dir = dir.path().join("api");
    fs::write(col_dir.join("users/list-users-gql.yml"), GRAPHQL_FIXTURE).unwrap();
    fs::write(col_dir.join("users/chat-ws.yml"), WEBSOCKET_FIXTURE).unwrap();

    let mut v = Violations::default();
    check_collection_root(&mut v, "opencollection.yml", &read_yaml(&col_dir.join("opencollection.yml")));
    check_folder(&mut v, "users/folder.yml", &read_yaml(&col_dir.join("users/folder.yml")));
    for rel in &written {
        check_http_request(&mut v, rel, &read_yaml(&col_dir.join(rel)));
    }

    // Non-HTTP items are never rewritten by the repo. Their raw values are what
    // Rocket would write if it did, so they are checked through the loaded tree.
    let col = repo.get("api").unwrap();
    let users = col.root.find_folder("users").expect("users folder");
    let mut protocols = Vec::new();
    for item in &users.items {
        if let CollectionItem::OpaqueItem(o) = item {
            protocols.push(o.protocol.clone());
            match o.protocol.as_str() {
                "graphql" => check_graphql_request(&mut v, &o.name, &o.raw),
                "websocket" => check_websocket_request(&mut v, &o.name, &o.raw),
                other => panic!("unexpected protocol {other}"),
            }
        }
    }
    protocols.sort();
    assert_eq!(protocols, vec!["graphql", "websocket"], "non-HTTP fixtures must load");
    assert!(v.0.is_empty(), "schema violations:\n{}", v.0.join("\n"));
}

#[test]
fn checker_flags_known_bad_shapes() {
    let mut v = Violations::default();
    let parse = |yaml: &str| -> Value { serde_yaml::from_str(yaml).expect("fixture yaml") };
    check_folder(&mut v, "legacy folder", &parse("name: legacy\ntype: folder\n"));
    check_auth(&mut v, "none auth", &parse("type: none\n"));
    check_auth(
        &mut v,
        "legacy pkce",
        &parse("type: oauth2\nflow: authorization_code\npkce:\n  enabled: true\n"),
    );
    check_auth(
        &mut v,
        "implicit secret",
        &parse("type: oauth2\nflow: implicit\ncredentials:\n  clientId: a\n  clientSecret: ''\n"),
    );
    // Legacy folder: `name` and `type` (2). Plus none auth, legacy pkce and implicit secret (1 each).
    assert_eq!(v.0.len(), 5, "{:#?}", v.0);
}
```

Register it in `crates/rocket-infra/src/fs_collection/mod.rs`, after the existing `#[cfg(test)] mod tests;`:

```rust
#[cfg(test)]
mod schema_shape_tests;
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p rocket-infra schema_shape_tests`
Expected: PASS for both tests, because Tasks 1 to 6 have landed. If `written_collection_files_only_use_schema_keys` fails, the message lists each file and key. Fix the writer. Do not add to `KNOWN_DEFERRED`.

- [ ] **Step 4: Prove the guard bites**

Temporarily delete the line `"HttpRequest.uid",` from `KNOWN_DEFERRED`.
Run: `cargo test -p rocket-infra written_collection_files_only_use_schema_keys`
Expected: FAIL with lines like `users/json-basic.yml: `uid` is not allowed on HttpRequest`, one per written request file.
Restore the line and rerun. Expected: PASS.

- [ ] **Step 5: Full verification**

Run: `cargo check && cargo test -p rocket-shared && cargo test -p rocket-infra && yarn tsc --noEmit && yarn check && yarn test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-infra/src/fs_collection/schema_shape_tests.rs crates/rocket-infra/src/fs_collection/mod.rs
```

Invoke the `dev-workflow-skills:1-git-commit` skill. Target subject: `test(infra): guard written collection files against OpenCollection schema keys`.
