# Folder Settings, Plan 02: folder.yml persistence

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `FsCollectionRepo` reads and writes every folder tab section (headers, auth, variables, the three scripts, docs) in an OpenCollection-shaped `folder.yml`, and returns the settings of every ancestor folder of a request in one call. Existing folder variable reads and writes keep working exactly as before.

**Architecture:** A new conversion module, `crates/rocket-infra/src/conversions/folder_settings.rs`, maps between the domain `FolderSettings` (from Plan 01) and `OcFolder`. It reuses the existing `Header`, `Auth` (`persisted_oc_auth`), `CollectionVariable` and script (`scripts_from_oc`, `scripts_to_oc`) conversions, and only touches the sections the tab owns, so `info`, `request.metadata`, `request.settings` and `hooks` scripts survive a save. A new repo module, `crates/rocket-infra/src/fs_collection/folder_settings.rs`, holds `get_folder_settings`, `save_folder_settings`, `get_folder_chain_settings` and one shared read-modify-write helper, `edit_folder_yml`, that does path validation, the collection lock, creation of a missing `folder.yml` and the atomic write through `write_folder_yml`. `get_folder_variables` delegates to `get_folder_settings`. `save_folder_variables` delegates to `edit_folder_yml` and still edits only `request.variables`, so object-form docs and Bruno-written fields are never normalised by a variable save. `SharedPathCollectionRepo` forwards all three new methods.

**Tech Stack:** Rust (serde, serde_yaml, tempfile). Run tests with `cargo test -j4 -p rocket-infra <filter>`.

**Spec:** `docs/superpowers/specs/2026-10-07-folder-settings-design.md`, and the locked contract in `docs/superpowers/plans/folder-settings/00-plan-index.md`. On-disk rules: `docs/superpowers/specs/opencollection-spec-reference.md` sections 2.7 (Folder) and the `RequestDefaults` / `Script` shapes.

## Global Constraints

- Depends on Plan 01. `rocket_collection::FolderSettings` (fields `headers`, `auth`, `variables`, `pre_request_script`, `post_response_script`, `tests_script`, `docs`; derives `Debug, Clone, PartialEq, Default`) and the three defaulted `CollectionRepository` methods `get_folder_settings`, `save_folder_settings`, `get_folder_chain_settings` must already exist with the exact signatures in the plan index. Check with `grep -n "fn get_folder_chain_settings" crates/rocket-collection/src/repository.rs` and `grep -n "pub use folder_settings" crates/rocket-collection/src/lib.rs` before Task 1. Stop if either is missing.
- `folder.yml` stays strictly OpenCollection: only `info`, `request` (`headers`, `metadata`, `auth`, `variables`, `scripts`, `settings`) and `docs`. No Rocket-only key, ever. `FolderInfo.uid` is the one existing deferred key and is already in `KNOWN_DEFERRED`.
- Empty sections are omitted. A `request` block with nothing left in it is dropped by `save_folder_settings`. `save_folder_variables` keeps its current output byte for byte (it still writes `request: {}` when the list is empty), because its behaviour must not change.
- Never apply `#[serde(rename_all = "camelCase")]` to any `Oc*` struct. This plan adds no new serde struct.
- Never `unwrap()` in production paths. Tests use `.expect("reason")`. The existing `mutex.lock().unwrap_or_else(|e| e.into_inner())` pattern is the lock idiom.
- `get_folder_chain_variables` is not changed. It stays lenient and skips a broken `folder.yml`; the new `get_folder_chain_settings` reports it instead.
- Always pass `-j4` to cargo and name one crate. Never run `cargo test --workspace`.
- Commit with the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only (never `git add -A` or `git add .`) and commit with a pathspec. Peer sessions share this repo's index.

## Review Focus

1. A save keeps what the tab does not own: `info` (name, uid), `request.metadata`, `request.settings` and `hooks` script entries (Task 1 test `apply_keeps_metadata_settings_and_hooks`, Task 2 test `save_folder_settings_keeps_unknown_request_fields_on_disk`).
2. Empty sections are never written as empty lists or blank scripts (Task 1 tests `empty_settings_write_no_request_and_no_docs` and `blank_scripts_are_left_out`, Task 2 test `empty_settings_leave_no_empty_sections`).
3. Docs are read from a plain string or `{content, type}`, and an unchanged object-form doc is not flattened on save (Task 1 test `docs_object_form_is_read_and_kept_when_unchanged`).
4. `save_folder_variables` still only edits `request.variables` and keeps every other section, and `get_folder_variables` reads through `get_folder_settings` (Task 2 test `save_folder_variables_keeps_the_other_sections`, plus the existing `spec_folder_yml_with_object_docs_loads_and_keeps_its_identity` and `save_folder_variables_rejects_corrupt_folder_yml` in `fs_collection/tests.rs`).
5. A save to a folder that does not exist is `InvalidInput` and creates no directory (Task 2 test `save_folder_settings_to_a_missing_folder_is_invalid_input`).
6. A corrupt `folder.yml` is an error that names the folder, on read, on save (without overwriting the file) and in the chain (Task 2 test `corrupt_folder_yml_error_names_the_folder`, Task 3 test `chain_reports_a_corrupt_folder_by_name`).
7. The chain has exactly one entry per ancestor folder, outermost first, with `FolderSettings::default()` for a folder without `folder.yml` (Task 3 test `chain_has_one_entry_per_folder_outermost_first`).
8. Path traversal is rejected on get, save and chain (Task 2 test `folder_settings_path_traversal_is_rejected`, Task 3 test `chain_rejects_parent_dir_components`).
9. A fully populated `folder.yml` passes the schema guard for every sample auth (Task 3 test `fully_populated_folder_yml_only_uses_schema_keys`).
10. The runtime repo (`SharedPathCollectionRepo`) forwards all three methods instead of falling back to the trait defaults (Task 3 test `folder_settings_calls_reach_the_active_workspace`).

---

## Task 1: Conversions between `FolderSettings` and `OcFolder`

**Files:**
- Create: `crates/rocket-infra/src/conversions/folder_settings.rs`
- Modify: `crates/rocket-infra/src/conversions/mod.rs`
- Test: `crates/rocket-infra/src/conversions/folder_settings.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes:
  - `rocket_collection::{CollectionVariable, FolderSettings}` (Plan 01).
  - `rocket_shared::types::{Auth, Header}`, `rocket_shared::description::Description` (`content()`, `text()`, `typed()`).
  - `crate::oc::{OcFolder, OcFolderInfo, OcHttpRequestHeader, OcRequestDefaults, OcVariable}`.
  - `super::auth::persisted_oc_auth(auth: Auth) -> Option<OcAuth>` (`conversions/auth.rs:249`).
  - `super::request::scripts_from_oc(scripts: &[OcScript]) -> (Option<String>, Option<String>, Option<String>)` and `super::request::scripts_to_oc(pre: &Option<String>, post: &Option<String>, tests: &Option<String>) -> Vec<OcScript>` (`conversions/request.rs:214-251`, both `pub(super)`).
  - Existing `From` impls: `Header <-> OcHttpRequestHeader` (`conversions/header.rs`), `CollectionVariable <-> OcVariable` (`conversions/variables.rs`), `Auth <- OcAuth` (`conversions/auth.rs`).
- Produces (crate-internal, re-exported from `crate::conversions`):
  - `pub fn oc_folder_to_folder_settings(folder: &OcFolder) -> FolderSettings`
  - `pub fn apply_folder_settings(folder: &mut OcFolder, settings: &FolderSettings)`

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.** (Section 2.7 Folder, `RequestDefaults`, `Script` and the `docs` shape.)

- [ ] **Step 2: Write the failing conversion tests**

Create `crates/rocket-infra/src/conversions/folder_settings.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use rocket_collection::{CollectionVariable, FolderSettings};
    use rocket_shared::description::Description;
    use rocket_shared::types::{Auth, Header};

    use super::{apply_folder_settings, oc_folder_to_folder_settings};
    use crate::oc::{OcFolder, OcFolderInfo};

    fn bare_folder() -> OcFolder {
        OcFolder {
            info: OcFolderInfo {
                name: "auth".into(),
                uid: Some("f-1".into()),
                ..OcFolderInfo::default()
            },
            items: None,
            request: None,
            docs: None,
        }
    }

    fn parse(yaml: &str) -> OcFolder {
        serde_yaml::from_str(yaml).expect("fixture folder.yml")
    }

    fn full_settings() -> FolderSettings {
        FolderSettings {
            headers: vec![
                Header::new("X-Tenant", "acme"),
                Header::disabled("X-Debug", "1"),
            ],
            auth: Some(Auth::Bearer {
                token: "{{token}}".into(),
            }),
            variables: vec![CollectionVariable {
                key: "region".into(),
                value: "eu".into(),
                initial_value: "eu".into(),
                enabled: true,
                secret: false,
            }],
            pre_request_script: Some("console.log('pre');".into()),
            post_response_script: Some("console.log('post');".into()),
            tests_script: Some("test('ok', () => {});".into()),
            docs: Some("# Auth folder".into()),
        }
    }

    #[test]
    fn every_section_round_trips_through_the_oc_folder() {
        let mut folder = bare_folder();
        apply_folder_settings(&mut folder, &full_settings());
        assert_eq!(oc_folder_to_folder_settings(&folder), full_settings());
        assert_eq!(folder.info, bare_folder().info, "info must not change");
        assert!(folder.items.is_none());
    }

    #[test]
    fn inherit_auth_round_trips_and_none_auth_is_left_out() {
        let mut folder = bare_folder();
        let inherit = FolderSettings {
            auth: Some(Auth::Inherit),
            ..FolderSettings::default()
        };
        apply_folder_settings(&mut folder, &inherit);
        assert_eq!(
            oc_folder_to_folder_settings(&folder).auth,
            Some(Auth::Inherit)
        );

        let none = FolderSettings {
            auth: Some(Auth::None),
            ..FolderSettings::default()
        };
        apply_folder_settings(&mut folder, &none);
        assert!(folder.request.is_none(), "{:?}", folder.request);
    }

    #[test]
    fn empty_settings_write_no_request_and_no_docs() {
        let mut folder = parse(
            "info:\n  name: auth\n  type: folder\nrequest:\n  variables:\n  - name: token\n    value: abc\ndocs: old docs\n",
        );
        apply_folder_settings(&mut folder, &FolderSettings::default());
        assert!(folder.request.is_none(), "{:?}", folder.request);
        assert!(folder.docs.is_none(), "{:?}", folder.docs);
    }

    #[test]
    fn blank_scripts_are_left_out() {
        let mut folder = bare_folder();
        let settings = FolderSettings {
            pre_request_script: Some("  \n".into()),
            tests_script: Some(String::new()),
            ..FolderSettings::default()
        };
        apply_folder_settings(&mut folder, &settings);
        assert!(folder.request.is_none(), "{:?}", folder.request);
    }

    #[test]
    fn apply_keeps_metadata_settings_and_hooks() {
        let fixture = "info:\n  name: auth\n  type: folder\nrequest:\n  metadata:\n  - name: x-trace\n    value: '1'\n  settings:\n    timeout: 5000\n  scripts:\n  - type: hooks\n    code: onStart()\n  - type: before-request\n    code: old()\n";
        let before = parse(fixture).request.expect("fixture request");
        let mut folder = parse(fixture);
        apply_folder_settings(&mut folder, &full_settings());
        let after = folder.request.expect("request kept");

        assert_eq!(after.metadata, before.metadata);
        assert_eq!(after.settings, before.settings);
        let scripts = after.scripts.expect("scripts");
        let codes: Vec<(&str, &str)> = scripts
            .iter()
            .map(|s| (s.script_type.as_str(), s.code.as_str()))
            .collect();
        assert_eq!(
            codes,
            vec![
                ("before-request", "console.log('pre');"),
                ("after-response", "console.log('post');"),
                ("tests", "test('ok', () => {});"),
                ("hooks", "onStart()"),
            ]
        );
    }

    #[test]
    fn docs_object_form_is_read_and_kept_when_unchanged() {
        let mut folder = parse(
            "info:\n  name: auth\n  type: folder\ndocs:\n  content: '# Auth'\n  type: text/markdown\n",
        );
        let mut settings = oc_folder_to_folder_settings(&folder);
        assert_eq!(settings.docs.as_deref(), Some("# Auth"));

        apply_folder_settings(&mut folder, &settings);
        assert_eq!(
            folder.docs,
            Some(Description::typed("# Auth", "text/markdown"))
        );

        settings.docs = Some("# Changed".into());
        apply_folder_settings(&mut folder, &settings);
        assert_eq!(folder.docs, Some(Description::text("# Changed")));
    }
}
```

Register the module and its re-export in `crates/rocket-infra/src/conversions/mod.rs`. Replace:

```rust
mod environment;
mod folder;
mod grpc;
```

with:

```rust
mod environment;
mod folder;
mod folder_settings;
mod grpc;
```

and replace:

```rust
#[allow(unused_imports)]
pub use folder::{
    collection_to_oc_collection, folder_to_oc_folder, oc_collection_to_collection,
    oc_folder_to_folder, oc_item_to_collection_item,
};
```

with:

```rust
#[allow(unused_imports)]
pub use folder::{
    collection_to_oc_collection, folder_to_oc_folder, oc_collection_to_collection,
    oc_folder_to_folder, oc_item_to_collection_item,
};
#[allow(unused_imports)]
pub use folder_settings::{apply_folder_settings, oc_folder_to_folder_settings};
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-infra conversions::folder_settings`
Expected: FAIL to compile with `unresolved imports` for `apply_folder_settings` and `oc_folder_to_folder_settings`.

- [ ] **Step 4: Implement the conversions**

Put this above the test module in `crates/rocket-infra/src/conversions/folder_settings.rs`:

```rust
//! Converts between the domain `FolderSettings` and the OpenCollection `Folder`
//! shape of `folder.yml`. Only the sections the folder tab edits are touched.

use rocket_collection::{CollectionVariable, FolderSettings};
use rocket_shared::description::Description;
use rocket_shared::types::{Auth, Header};

use crate::oc::{OcFolder, OcHttpRequestHeader, OcRequestDefaults, OcVariable};

use super::auth::persisted_oc_auth;
use super::request::{scripts_from_oc, scripts_to_oc};

/// Script types the folder tab owns. Other entries, like `hooks`, are kept as they are.
const OWNED_SCRIPT_TYPES: [&str; 3] = ["before-request", "after-response", "tests"];

/// Reads the folder tab's sections out of a parsed `folder.yml`.
pub fn oc_folder_to_folder_settings(folder: &OcFolder) -> FolderSettings {
    let defaults = folder.request.clone().unwrap_or_default();
    let (pre_request_script, post_response_script, tests_script) =
        scripts_from_oc(defaults.scripts.as_deref().unwrap_or_default());
    FolderSettings {
        headers: defaults
            .headers
            .unwrap_or_default()
            .into_iter()
            .map(Header::from)
            .collect(),
        auth: defaults.auth.map(Auth::from),
        variables: defaults
            .variables
            .unwrap_or_default()
            .into_iter()
            .map(CollectionVariable::from)
            .collect(),
        pre_request_script,
        post_response_script,
        tests_script,
        docs: folder
            .docs
            .as_ref()
            .and_then(|d| d.content())
            .map(str::to_string),
    }
}

/// Writes the folder tab's sections into `folder`. `info`, `request.metadata`,
/// `request.settings` and script entries the tab does not own are kept. Empty
/// sections are left out, and `request` is dropped when nothing is left in it.
pub fn apply_folder_settings(folder: &mut OcFolder, settings: &FolderSettings) {
    let mut defaults = folder.request.take().unwrap_or_default();
    defaults.headers = non_empty(
        settings
            .headers
            .iter()
            .cloned()
            .map(OcHttpRequestHeader::from)
            .collect(),
    );
    defaults.auth = settings.auth.clone().and_then(persisted_oc_auth);
    defaults.variables = non_empty(
        settings
            .variables
            .iter()
            .cloned()
            .map(OcVariable::from)
            .collect(),
    );
    let mut scripts = scripts_to_oc(
        &non_blank(&settings.pre_request_script),
        &non_blank(&settings.post_response_script),
        &non_blank(&settings.tests_script),
    );
    scripts.extend(
        defaults
            .scripts
            .take()
            .unwrap_or_default()
            .into_iter()
            .filter(|s| !OWNED_SCRIPT_TYPES.contains(&s.script_type.as_str())),
    );
    defaults.scripts = non_empty(scripts);
    folder.request = if defaults == OcRequestDefaults::default() {
        None
    } else {
        Some(defaults)
    };
    folder.docs = docs_to_oc(folder.docs.take(), settings.docs.as_deref());
}

fn non_empty<T>(items: Vec<T>) -> Option<Vec<T>> {
    if items.is_empty() {
        None
    } else {
        Some(items)
    }
}

/// A script with only whitespace counts as no script.
fn non_blank(script: &Option<String>) -> Option<String> {
    script.clone().filter(|code| !code.trim().is_empty())
}

/// Keeps the existing docs, including the object form with a type, when the
/// content did not change. New content is written as a plain string.
fn docs_to_oc(existing: Option<Description>, docs: Option<&str>) -> Option<Description> {
    let docs = docs.filter(|d| !d.trim().is_empty())?;
    match existing {
        Some(current) if current.content() == Some(docs) => Some(current),
        _ => Some(Description::text(docs)),
    }
}
```

The two `pub fn`s have no caller outside tests until Task 2, so `cargo check` shows two `never used` warnings until then. Do not add `#[allow(dead_code)]`; Task 2 removes the warnings.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-infra conversions::folder_settings`
Expected: PASS, 6 tests.

Run: `cargo test -j4 -p rocket-infra conversions::`
Expected: PASS (no regression in the other conversion tests).

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path and commit with a pathspec:

```bash
git add crates/rocket-infra/src/conversions/folder_settings.rs crates/rocket-infra/src/conversions/mod.rs
git commit -m "<message from the skill>" -- crates/rocket-infra/src/conversions/folder_settings.rs crates/rocket-infra/src/conversions/mod.rs
```

Suggested subject: `feat(infra): convert folder settings to and from folder.yml`.

---

## Task 2: `get_folder_settings` and `save_folder_settings` on `FsCollectionRepo`

**Files:**
- Create: `crates/rocket-infra/src/fs_collection/folder_settings.rs`
- Modify: `crates/rocket-infra/src/fs_collection/mod.rs` (module list at lines 12-19, `use rocket_collection` at lines 6-9, `impl CollectionRepository for FsCollectionRepo` after `save_folder_variables` at lines 288-295)
- Modify: `crates/rocket-infra/src/fs_collection/variables.rs` (imports at lines 1-14, `save_folder_variables` at lines 68-112, `get_folder_variables` at lines 114-139)
- Test: `crates/rocket-infra/src/fs_collection/folder_settings.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes:
  - Task 1: `crate::conversions::{apply_folder_settings, oc_folder_to_folder_settings}`.
  - `super::folder_file::{read_folder_yml, write_folder_yml}` (`folder_file.rs:64-84`; `read_folder_yml` keeps the legacy-shape fallback and returns `DomainError::Internal("Failed to parse folder.yml: ...")`).
  - `FsCollectionRepo::{collection_mutex, collection_path, validate_path}` (`fs_collection/mod.rs:45-102`).
  - `rocket_collection::Collection::validate_name`.
  - Plan 01 trait methods (defaulted): `get_folder_settings`, `save_folder_settings`.
- Produces:
  - `pub(super) fn get_folder_settings(repo: &FsCollectionRepo, collection: &str, folder_path: &str) -> DomainResult<FolderSettings>`: `FolderSettings::default()` when `folder.yml` (or the folder) does not exist, an error naming the folder when the file does not parse.
  - `pub(super) fn save_folder_settings(repo: &FsCollectionRepo, collection: &str, folder_path: &str, settings: &FolderSettings) -> DomainResult<()>`: `InvalidInput` for `""` or a folder directory that does not exist; creates `folder.yml` when it is absent.
  - `pub(super) fn edit_folder_yml(repo: &FsCollectionRepo, collection: &str, folder_path: &str, require_existing_dir: bool, edit: impl FnOnce(&mut OcFolder)) -> DomainResult<()>`: the one read-modify-write path for `folder.yml`.
  - `impl CollectionRepository for FsCollectionRepo`: overrides `get_folder_settings` and `save_folder_settings`.
  - `variables::get_folder_variables` and `variables::save_folder_variables` keep their signatures and now go through this module.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.** (Section 2.7 Folder and the unbundled layout section on `folder.yml`.)

- [ ] **Step 2: Write the failing repository tests**

Create `crates/rocket-infra/src/fs_collection/folder_settings.rs` with only the test module. The tests call the trait, so they compile against Plan 01's default (`Err(Internal("folder settings not supported"))`) and fail at run time.

```rust
#[cfg(test)]
mod tests {
    use std::fs;

    use rocket_collection::{CollectionRepository, CollectionVariable, FolderSettings};
    use rocket_shared::error::DomainError;
    use rocket_shared::types::{Auth, Header};
    use serde_yaml::Value;
    use tempfile::TempDir;

    use crate::FsCollectionRepo;

    /// A collection `api` with one folder `users` created through the repo.
    fn setup() -> (TempDir, FsCollectionRepo) {
        let dir = TempDir::new().expect("tempdir");
        let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.create("api").expect("create collection");
        repo.create_folder("api", "users").expect("create folder");
        (dir, repo)
    }

    fn read_raw(dir: &TempDir, rel: &str) -> Value {
        let content =
            fs::read_to_string(dir.path().join("api").join(rel)).expect("read folder.yml");
        serde_yaml::from_str(&content).expect("valid yaml")
    }

    fn full_settings() -> FolderSettings {
        FolderSettings {
            headers: vec![
                Header::new("X-Tenant", "acme"),
                Header::disabled("X-Debug", "1"),
            ],
            auth: Some(Auth::Bearer {
                token: "{{token}}".into(),
            }),
            variables: vec![CollectionVariable {
                key: "region".into(),
                value: "eu".into(),
                initial_value: "eu".into(),
                enabled: true,
                secret: false,
            }],
            pre_request_script: Some("console.log('pre');".into()),
            post_response_script: Some("console.log('post');".into()),
            tests_script: Some("test('ok', () => {});".into()),
            docs: Some("# Users".into()),
        }
    }

    #[test]
    fn save_and_get_folder_settings_round_trip() {
        let (dir, repo) = setup();
        let uid_before = read_raw(&dir, "users/folder.yml")["info"]["uid"]
            .as_str()
            .map(str::to_string);
        assert!(uid_before.is_some(), "create_folder writes a uid");

        repo.save_folder_settings("api", "users", &full_settings())
            .expect("save");

        assert_eq!(
            repo.get_folder_settings("api", "users").expect("get"),
            full_settings()
        );
        let raw = read_raw(&dir, "users/folder.yml");
        assert_eq!(raw["info"]["name"].as_str(), Some("users"), "{raw:?}");
        assert_eq!(
            raw["info"]["uid"].as_str().map(str::to_string),
            uid_before,
            "{raw:?}"
        );
        assert_eq!(raw["docs"].as_str(), Some("# Users"), "{raw:?}");
    }

    #[test]
    fn get_folder_settings_without_folder_yml_is_default() {
        let (dir, repo) = setup();
        fs::create_dir_all(dir.path().join("api/billing")).expect("mkdir");
        assert_eq!(
            repo.get_folder_settings("api", "billing").expect("get"),
            FolderSettings::default()
        );
    }

    #[test]
    fn save_folder_settings_creates_folder_yml_when_absent() {
        let (dir, repo) = setup();
        // A folder made outside Rocket, with no folder.yml yet.
        fs::create_dir_all(dir.path().join("api/billing")).expect("mkdir");

        repo.save_folder_settings("api", "billing", &full_settings())
            .expect("save");

        let raw = read_raw(&dir, "billing/folder.yml");
        assert_eq!(raw["info"]["name"].as_str(), Some("billing"), "{raw:?}");
        assert_eq!(raw["info"]["type"].as_str(), Some("folder"), "{raw:?}");
        assert_eq!(
            repo.get_folder_settings("api", "billing").expect("get"),
            full_settings()
        );
    }

    #[test]
    fn save_folder_settings_to_a_missing_folder_is_invalid_input() {
        let (dir, repo) = setup();
        let err = repo
            .save_folder_settings("api", "ghost", &full_settings())
            .expect_err("missing folder");
        assert!(matches!(err, DomainError::InvalidInput(_)), "{err:?}");
        assert!(
            !dir.path().join("api/ghost").exists(),
            "no directory may be created"
        );
    }

    #[test]
    fn save_folder_settings_rejects_the_collection_root() {
        let (dir, repo) = setup();
        let err = repo
            .save_folder_settings("api", "", &full_settings())
            .expect_err("root");
        assert!(matches!(err, DomainError::InvalidInput(_)), "{err:?}");
        assert!(!dir.path().join("api/folder.yml").exists());
    }

    #[test]
    fn legacy_shape_folder_yml_still_reads() {
        let (dir, repo) = setup();
        fs::write(
            dir.path().join("api/users/folder.yml"),
            "name: users\nuid: legacy-uid\ntype: folder\nrequest:\n  headers:\n  - name: X-Legacy\n    value: '1'\n  variables:\n  - name: token\n    value: abc\n",
        )
        .expect("write legacy folder.yml");

        let settings = repo.get_folder_settings("api", "users").expect("get");
        assert_eq!(settings.headers, vec![Header::new("X-Legacy", "1")]);
        assert_eq!(settings.variables.len(), 1);
        assert_eq!(settings.variables[0].key, "token");
    }

    #[test]
    fn corrupt_folder_yml_error_names_the_folder() {
        let (dir, repo) = setup();
        let path = dir.path().join("api/users/folder.yml");
        fs::write(&path, "{{{{not valid yaml: [[[").expect("write");

        let read_err = repo
            .get_folder_settings("api", "users")
            .expect_err("corrupt read");
        assert!(read_err.to_string().contains("'users'"), "{read_err}");

        let save_err = repo
            .save_folder_settings("api", "users", &full_settings())
            .expect_err("corrupt save");
        assert!(save_err.to_string().contains("'users'"), "{save_err}");
        assert!(
            fs::read_to_string(&path)
                .expect("read")
                .contains("not valid yaml"),
            "the broken file must not be overwritten"
        );
    }

    #[test]
    fn folder_settings_path_traversal_is_rejected() {
        let (_dir, repo) = setup();
        let get = repo
            .get_folder_settings("api", "../../evil")
            .expect_err("traversal");
        assert!(
            matches!(get, DomainError::InvalidInput(_) | DomainError::NotFound(_)),
            "{get:?}"
        );
        let save = repo
            .save_folder_settings("api", "../../evil", &full_settings())
            .expect_err("traversal");
        assert!(
            matches!(save, DomainError::InvalidInput(_) | DomainError::NotFound(_)),
            "{save:?}"
        );
    }

    #[test]
    fn save_folder_settings_keeps_unknown_request_fields_on_disk() {
        let (dir, repo) = setup();
        fs::write(
            dir.path().join("api/users/folder.yml"),
            "info:\n  name: users\n  type: folder\nrequest:\n  metadata:\n  - name: x-trace\n    value: '1'\n  settings:\n    timeout: 5000\n  scripts:\n  - type: hooks\n    code: onStart()\n",
        )
        .expect("write fixture");

        repo.save_folder_settings("api", "users", &full_settings())
            .expect("save");

        let raw = read_raw(&dir, "users/folder.yml");
        assert_eq!(
            raw["request"]["metadata"][0]["name"].as_str(),
            Some("x-trace"),
            "{raw:?}"
        );
        assert!(
            raw["request"]["settings"]["timeout"].is_number(),
            "{raw:?}"
        );
        let hooks: Vec<&Value> = raw["request"]["scripts"]
            .as_sequence()
            .expect("scripts")
            .iter()
            .filter(|s| s["type"].as_str() == Some("hooks"))
            .collect();
        assert_eq!(hooks.len(), 1, "{raw:?}");
        assert_eq!(hooks[0]["code"].as_str(), Some("onStart()"));
    }

    #[test]
    fn empty_settings_leave_no_empty_sections() {
        let (dir, repo) = setup();
        repo.save_folder_settings("api", "users", &full_settings())
            .expect("save full");
        repo.save_folder_settings("api", "users", &FolderSettings::default())
            .expect("save empty");
        let raw = read_raw(&dir, "users/folder.yml");
        assert!(raw.get("request").is_none(), "{raw:?}");
        assert!(raw.get("docs").is_none(), "{raw:?}");
        assert_eq!(raw["info"]["name"].as_str(), Some("users"), "{raw:?}");
    }

    #[test]
    fn save_folder_variables_keeps_the_other_sections() {
        let (_dir, repo) = setup();
        repo.save_folder_settings("api", "users", &full_settings())
            .expect("save settings");
        let vars = vec![CollectionVariable {
            key: "page".into(),
            value: "2".into(),
            initial_value: "2".into(),
            enabled: true,
            secret: false,
        }];

        repo.save_folder_variables("api", "users", vars.clone())
            .expect("save vars");

        assert_eq!(
            repo.get_folder_settings("api", "users").expect("get"),
            FolderSettings {
                variables: vars.clone(),
                ..full_settings()
            }
        );
        assert_eq!(
            repo.get_folder_variables("api", "users").expect("vars"),
            vars
        );
    }
}
```

Register the module in `crates/rocket-infra/src/fs_collection/mod.rs`. Replace:

```rust
pub(crate) mod folder_file;
mod folders;
```

with:

```rust
pub(crate) mod folder_file;
mod folder_settings;
mod folders;
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-infra fs_collection::folder_settings`
Expected: FAIL. The tests compile, and all 11 fail: the Plan 01 trait defaults return `Err(Internal("folder settings not supported"))`, so every `.expect(...)` on a get or save panics with that message, `save_folder_settings_to_a_missing_folder_is_invalid_input` and `save_folder_settings_rejects_the_collection_root` fail their `matches!(err, DomainError::InvalidInput(_))` assertion, and `corrupt_folder_yml_error_names_the_folder` and `folder_settings_path_traversal_is_rejected` fail their message or variant assertion.

- [ ] **Step 4: Implement the repository functions**

Put this above the test module in `crates/rocket-infra/src/fs_collection/folder_settings.rs`:

```rust
//! Reads and writes the folder tab's sections of `folder.yml`.

use std::path::{Path, PathBuf};

use rocket_collection::{Collection, FolderSettings};
use rocket_shared::error::{DomainError, DomainResult};

use crate::conversions::{apply_folder_settings, oc_folder_to_folder_settings};
use crate::oc::{OcFolder, OcFolderInfo};

use super::folder_file::{read_folder_yml, write_folder_yml};
use super::FsCollectionRepo;

/// Resolves a folder path relative to the collection root. `""` is the root.
fn folder_dir(
    repo: &FsCollectionRepo,
    collection_dir: &Path,
    folder_path: &str,
) -> DomainResult<PathBuf> {
    if folder_path.is_empty() {
        Ok(collection_dir.to_path_buf())
    } else {
        repo.validate_path(collection_dir, Path::new(folder_path))
    }
}

/// Reads `folder.yml` and puts the folder path into a parse error.
fn read_named(path: &Path, folder_path: &str) -> DomainResult<OcFolder> {
    read_folder_yml(path).map_err(|e| match e {
        DomainError::Internal(msg) => {
            DomainError::Internal(format!("Folder '{folder_path}': {msg}"))
        }
        other => other,
    })
}

/// A new `folder.yml` for a directory that has none, named after the directory.
fn blank_folder(dir: &Path) -> OcFolder {
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    OcFolder {
        info: OcFolderInfo {
            name,
            ..OcFolderInfo::default()
        },
        items: None,
        request: None,
        docs: None,
    }
}

/// Reads a folder's own settings. A folder without `folder.yml` has default settings.
pub(super) fn get_folder_settings(
    repo: &FsCollectionRepo,
    collection: &str,
    folder_path: &str,
) -> DomainResult<FolderSettings> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);
    let dir = folder_dir(repo, &collection_dir, folder_path)?;
    let path = dir.join("folder.yml");
    if !path.exists() {
        return Ok(FolderSettings::default());
    }
    Ok(oc_folder_to_folder_settings(&read_named(
        &path,
        folder_path,
    )?))
}

/// Reads `folder.yml`, or starts a new one when it is absent, applies `edit` and
/// writes the file back atomically, all under the collection lock. With
/// `require_existing_dir`, a folder directory that does not exist is `InvalidInput`.
pub(super) fn edit_folder_yml(
    repo: &FsCollectionRepo,
    collection: &str,
    folder_path: &str,
    require_existing_dir: bool,
    edit: impl FnOnce(&mut OcFolder),
) -> DomainResult<()> {
    Collection::validate_name(collection)?;
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    let collection_dir = repo.collection_path(collection);
    let dir = folder_dir(repo, &collection_dir, folder_path)?;
    if require_existing_dir && !dir.is_dir() {
        return Err(DomainError::InvalidInput(format!(
            "Folder '{folder_path}' does not exist in collection '{collection}'"
        )));
    }
    let path = dir.join("folder.yml");
    let mut folder = if path.exists() {
        read_named(&path, folder_path)?
    } else {
        blank_folder(&dir)
    };
    edit(&mut folder);
    write_folder_yml(&path, &folder)
}

/// Saves a folder's own settings into its `folder.yml`. The collection root has
/// no `folder.yml`; its settings are the collection settings.
pub(super) fn save_folder_settings(
    repo: &FsCollectionRepo,
    collection: &str,
    folder_path: &str,
    settings: &FolderSettings,
) -> DomainResult<()> {
    if folder_path.is_empty() {
        return Err(DomainError::InvalidInput(
            "Folder settings need a folder path. Use the collection settings for the root."
                .into(),
        ));
    }
    edit_folder_yml(repo, collection, folder_path, true, |folder| {
        apply_folder_settings(folder, settings)
    })
}
```

Wire the trait in `crates/rocket-infra/src/fs_collection/mod.rs`. Replace:

```rust
use rocket_collection::{
    Collection, CollectionRepository, CollectionSettings, CollectionSummary, CollectionVariable,
    GraphQlRequest, GrpcRequest, Request, RequestKind, WebSocketRequest,
};
```

with:

```rust
use rocket_collection::{
    Collection, CollectionRepository, CollectionSettings, CollectionSummary, CollectionVariable,
    FolderSettings, GraphQlRequest, GrpcRequest, Request, RequestKind, WebSocketRequest,
};
```

and replace:

```rust
    fn save_folder_variables(
        &self,
        collection: &str,
        folder_path: &str,
        vars: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        variables::save_folder_variables(self, collection, folder_path, vars)
    }
```

with:

```rust
    fn save_folder_variables(
        &self,
        collection: &str,
        folder_path: &str,
        vars: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        variables::save_folder_variables(self, collection, folder_path, vars)
    }

    fn get_folder_settings(
        &self,
        collection: &str,
        folder_path: &str,
    ) -> DomainResult<FolderSettings> {
        folder_settings::get_folder_settings(self, collection, folder_path)
    }

    fn save_folder_settings(
        &self,
        collection: &str,
        folder_path: &str,
        settings: &FolderSettings,
    ) -> DomainResult<()> {
        folder_settings::save_folder_settings(self, collection, folder_path, settings)
    }
```

- [ ] **Step 5: Delegate the folder variable functions**

In `crates/rocket-infra/src/fs_collection/variables.rs`, replace the imports:

```rust
use crate::atomic_write;
use crate::oc::{
    OcFolder, OcFolderInfo, OcGraphQLRequest, OcGraphQLRequestRuntime, OcHttpRequest,
    OcGrpcRequest, OcHttpRequestRuntime, OcRequestDefaults, OcVariable, OcWebSocketRequest,
};

use super::folder_file::{parse_folder_yml, read_folder_yml, write_folder_yml};
use super::paths::resolve_request_path;
use super::FsCollectionRepo;
```

with:

```rust
use crate::atomic_write;
use crate::oc::{
    OcGraphQLRequest, OcGraphQLRequestRuntime, OcHttpRequest, OcGrpcRequest,
    OcHttpRequestRuntime, OcRequestDefaults, OcVariable, OcWebSocketRequest,
};

use super::folder_file::parse_folder_yml;
use super::folder_settings::{edit_folder_yml, get_folder_settings};
use super::paths::resolve_request_path;
use super::FsCollectionRepo;
```

Replace the whole body of `save_folder_variables` and `get_folder_variables` (lines 68-139, from `pub(super) fn save_folder_variables(` to the closing `}` of `get_folder_variables`) with:

```rust
pub(super) fn save_folder_variables(
    repo: &FsCollectionRepo,
    collection: &str,
    folder_path: &str,
    vars: Vec<CollectionVariable>,
) -> DomainResult<()> {
    let oc_vars: Vec<OcVariable> = vars.into_iter().map(OcVariable::from).collect();
    // Only `request.variables` changes. Docs, scripts and auth stay exactly as they
    // are on disk, and a folder directory that does not exist is still created.
    edit_folder_yml(repo, collection, folder_path, false, move |oc_folder| {
        let req_defaults = oc_folder.request.take().unwrap_or_default();
        oc_folder.request = Some(OcRequestDefaults {
            variables: if oc_vars.is_empty() {
                None
            } else {
                Some(oc_vars)
            },
            ..req_defaults
        });
    })
}

pub(super) fn get_folder_variables(
    repo: &FsCollectionRepo,
    collection: &str,
    folder_path: &str,
) -> DomainResult<Vec<CollectionVariable>> {
    Ok(get_folder_settings(repo, collection, folder_path)?.variables)
}
```

`get_folder_chain_variables`, `get_request_variables` and `save_request_variables` are not touched.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-infra fs_collection::folder_settings`
Expected: PASS, 11 tests.

Run: `cargo test -j4 -p rocket-infra fs_collection::`
Expected: PASS. This covers the existing folder variable tests in `fs_collection/tests.rs` (`folder_variables_roundtrip`, `save_folder_variables_rejects_corrupt_folder_yml`, `spec_folder_yml_with_object_docs_loads_and_keeps_its_identity`, `save_folder_variables_without_folder_yml_names_folder_after_its_directory`, the legacy-upgrade test) and the schema guard in `schema_shape_tests.rs`.

Run: `cargo check -j4 -p rocket-infra --tests`
Expected: no warnings from `conversions/folder_settings.rs`, `fs_collection/folder_settings.rs` or `fs_collection/variables.rs` (the Task 1 `never used` warnings are gone).

- [ ] **Step 7: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path and commit with a pathspec:

```bash
git add crates/rocket-infra/src/fs_collection/folder_settings.rs crates/rocket-infra/src/fs_collection/mod.rs crates/rocket-infra/src/fs_collection/variables.rs
git commit -m "<message from the skill>" -- crates/rocket-infra/src/fs_collection/folder_settings.rs crates/rocket-infra/src/fs_collection/mod.rs crates/rocket-infra/src/fs_collection/variables.rs
```

Suggested subject: `feat(infra): read and write folder settings in folder.yml`.

---

## Task 3: Folder settings chain, runtime repo delegation and schema guard

**Files:**
- Modify: `crates/rocket-infra/src/fs_collection/folder_settings.rs` (add `get_folder_chain_settings` and its tests)
- Modify: `crates/rocket-infra/src/fs_collection/mod.rs` (trait impl)
- Modify: `crates/rocket-infra/src/shared_path_collection_repo.rs` (imports at lines 6-9, impl after `save_folder_variables` at lines 223-231, test at the end of `mod tests`)
- Modify: `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs` (imports at lines 12-14, `check_request_defaults` at lines 316-327, `checker_flags_known_bad_shapes` at lines 839-860, new test at the end of the file)

**Interfaces:**
- Consumes: Task 2 `folder_dir` is not reused here (the chain walks segment by segment); `read_named`, `oc_folder_to_folder_settings`, `FsCollectionRepo::validate_path`, Plan 01 trait method `get_folder_chain_settings` (default `Ok(vec![])`).
- Produces:
  - `pub(super) fn get_folder_chain_settings(repo: &FsCollectionRepo, collection: &str, request_path: &str) -> DomainResult<Vec<FolderSettings>>`: outermost folder first, one entry per ancestor folder of `request_path`, `FolderSettings::default()` for a folder without `folder.yml`, an error naming the folder for one that does not parse, `InvalidInput` for a `..` or absolute component.
  - `impl CollectionRepository for FsCollectionRepo`: overrides `get_folder_chain_settings`.
  - `impl CollectionRepository for SharedPathCollectionRepo`: forwards `get_folder_settings`, `save_folder_settings` and `get_folder_chain_settings`.
  - Schema guard: `check_request_defaults` also checks `scripts`, `metadata` and `settings` entries.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.** (Section 2.7 Folder, `RequestDefaults`, and the folder-variable chain rules: walk the full ancestor chain, innermost wins.)

- [ ] **Step 2: Write the failing chain, delegation and schema tests**

Append these tests inside `mod tests` in `crates/rocket-infra/src/fs_collection/folder_settings.rs` (after `save_folder_variables_keeps_the_other_sections`):

```rust
    #[test]
    fn chain_has_one_entry_per_folder_outermost_first() {
        let (_dir, repo) = setup();
        // `users/bare` gets no folder.yml; create_folder only writes one for `inner`.
        repo.create_folder("api", "users/bare/inner")
            .expect("create inner");
        let outer = FolderSettings {
            headers: vec![Header::new("X-Outer", "1")],
            ..FolderSettings::default()
        };
        let inner = FolderSettings {
            headers: vec![Header::new("X-Inner", "2")],
            ..FolderSettings::default()
        };
        repo.save_folder_settings("api", "users", &outer)
            .expect("save outer");
        repo.save_folder_settings("api", "users/bare/inner", &inner)
            .expect("save inner");

        let chain = repo
            .get_folder_chain_settings("api", "users/bare/inner/list.yml")
            .expect("chain");

        assert_eq!(chain, vec![outer, FolderSettings::default(), inner]);
    }

    #[test]
    fn chain_of_a_root_level_request_is_empty() {
        let (_dir, repo) = setup();
        assert!(repo
            .get_folder_chain_settings("api", "list.yml")
            .expect("chain")
            .is_empty());
    }

    #[test]
    fn chain_reports_a_corrupt_folder_by_name() {
        let (dir, repo) = setup();
        repo.create_folder("api", "users/admin")
            .expect("create admin");
        fs::write(
            dir.path().join("api/users/admin/folder.yml"),
            "{{{{not valid yaml: [[[",
        )
        .expect("write");

        let err = repo
            .get_folder_chain_settings("api", "users/admin/list.yml")
            .expect_err("corrupt folder.yml");
        assert!(err.to_string().contains("'users/admin'"), "{err}");

        // The variables chain stays lenient and skips the broken file, as before.
        assert!(repo
            .get_folder_chain_variables("api", "users/admin/list.yml")
            .is_ok());
    }

    #[test]
    fn chain_rejects_parent_dir_components() {
        let (_dir, repo) = setup();
        let err = repo
            .get_folder_chain_settings("api", "users/../../evil/list.yml")
            .expect_err("traversal");
        assert!(matches!(err, DomainError::InvalidInput(_)), "{err:?}");
    }
```

Append this test at the end of `mod tests` in `crates/rocket-infra/src/shared_path_collection_repo.rs` (the module closes at line 396; put the test just before that closing `}`):

```rust
    #[test]
    fn folder_settings_calls_reach_the_active_workspace() {
        let (_dir, repo) = setup();
        repo.create("api").unwrap();
        repo.create_folder("api", "users").unwrap();
        let settings = rocket_collection::FolderSettings {
            headers: vec![rocket_shared::types::Header::new("X-Tenant", "acme")],
            ..Default::default()
        };

        repo.save_folder_settings("api", "users", &settings).unwrap();

        assert_eq!(repo.get_folder_settings("api", "users").unwrap(), settings);
        assert_eq!(
            repo.get_folder_chain_settings("api", "users/list.yml")
                .unwrap(),
            vec![settings]
        );
    }
```

In `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs`, replace the import:

```rust
use rocket_collection::{
    CollectionItem, CollectionRepository, CollectionSettings, CollectionVariable, Request,
};
```

with:

```rust
use rocket_collection::{
    CollectionItem, CollectionRepository, CollectionSettings, CollectionVariable, FolderSettings,
    Request,
};
```

Extend `checker_flags_known_bad_shapes` so the checker must look at folder scripts. Replace:

```rust
    // Legacy folder: `name` and `type` (2). Plus none auth, legacy pkce and implicit secret (1 each).
    assert_eq!(v.0.len(), 5, "{:#?}", v.0);
```

with:

```rust
    check_folder(
        &mut v,
        "folder script extra key",
        &parse("info:\n  name: f\n  type: folder\nrequest:\n  scripts:\n  - type: tests\n    code: x\n    enabled: true\n"),
    );
    // Legacy folder: `name` and `type` (2). Plus none auth, legacy pkce, implicit secret
    // and the folder script's `enabled` (1 each).
    assert_eq!(v.0.len(), 6, "{:#?}", v.0);
```

Append this test at the end of the file:

```rust
#[test]
fn fully_populated_folder_yml_only_uses_schema_keys() {
    let (dir, repo) = setup();
    repo.create("api").expect("create collection");
    repo.create_folder("api", "users").expect("create folder");
    let folder_yml = dir.path().join("api/users/folder.yml");
    // Sections a Bruno user may have written, which a save must keep valid.
    fs::write(
        &folder_yml,
        "info:\n  name: users\n  type: folder\nrequest:\n  metadata:\n  - name: x-trace\n    value: '1'\n  settings:\n    timeout: 5000\n  scripts:\n  - type: hooks\n    code: onStart()\n",
    )
    .expect("write fixture");

    for (auth_name, auth) in sample_auths() {
        repo.save_folder_settings(
            "api",
            "users",
            &FolderSettings {
                headers: vec![
                    Header::new("X-Tenant", "acme"),
                    Header::disabled("X-Debug", "1"),
                ],
                auth: Some(auth),
                variables: vec![CollectionVariable {
                    key: "fv".into(),
                    value: "x".into(),
                    initial_value: "x".into(),
                    enabled: false,
                    secret: false,
                }],
                pre_request_script: Some("console.log('pre');".into()),
                post_response_script: Some("console.log('post');".into()),
                tests_script: Some("test('ok', () => {});".into()),
                docs: Some("# Users".into()),
            },
        )
        .expect("save folder settings");

        let doc = read_yaml(&folder_yml);
        let mut v = Violations::default();
        check_folder(&mut v, &format!("users/folder.yml [{auth_name}]"), &doc);
        assert!(v.0.is_empty(), "schema violations:\n{}", v.0.join("\n"));
        assert!(doc["request"]["headers"].is_sequence(), "{doc:?}");
        assert!(doc["request"]["variables"].is_sequence(), "{doc:?}");
        assert_eq!(
            doc["request"]["scripts"].as_sequence().map(Vec::len),
            Some(4),
            "{doc:?}"
        );
        assert_eq!(doc["docs"].as_str(), Some("# Users"), "{doc:?}");
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-infra fs_collection::folder_settings::tests::chain`
Expected: FAIL. `chain_has_one_entry_per_folder_outermost_first` gets `[]` from the Plan 01 default, `chain_reports_a_corrupt_folder_by_name` and `chain_rejects_parent_dir_components` panic at `expect_err` on `Ok([])`. `chain_of_a_root_level_request_is_empty` already passes.

Run: `cargo test -j4 -p rocket-infra shared_path_collection_repo::tests::folder_settings_calls_reach_the_active_workspace`
Expected: FAIL with `folder settings not supported` (the trait default).

Run: `cargo test -j4 -p rocket-infra schema_shape_tests::checker_flags_known_bad_shapes`
Expected: FAIL, `left: 5, right: 6`, because `check_request_defaults` does not look at `scripts` yet.

- [ ] **Step 4: Implement the chain and the delegation**

In `crates/rocket-infra/src/fs_collection/folder_settings.rs`, replace the import line:

```rust
use std::path::{Path, PathBuf};
```

with:

```rust
use std::path::{Component, Path, PathBuf};
```

and add this function after `save_folder_settings` (above the test module):

```rust
/// Settings of every ancestor folder of a request, outermost first. A folder
/// without `folder.yml` gives `FolderSettings::default()`, so there is one entry
/// per folder level. A `folder.yml` that does not parse is an error naming that
/// folder; unlike `get_folder_chain_variables`, nothing is skipped.
pub(super) fn get_folder_chain_settings(
    repo: &FsCollectionRepo,
    collection: &str,
    request_path: &str,
) -> DomainResult<Vec<FolderSettings>> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);
    let parent = Path::new(request_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let mut segments = Vec::new();
    for component in parent.components() {
        match component {
            Component::Normal(segment) => segments.push(segment),
            Component::CurDir => {}
            _ => {
                return Err(DomainError::InvalidInput(format!(
                    "Invalid request path '{request_path}'"
                )))
            }
        }
    }

    let mut chain = Vec::with_capacity(segments.len());
    let mut rel = PathBuf::new();
    for segment in segments {
        rel.push(segment);
        let dir = repo.validate_path(&collection_dir, &rel)?;
        let path = dir.join("folder.yml");
        if path.exists() {
            let folder = read_named(&path, &rel.to_string_lossy())?;
            chain.push(oc_folder_to_folder_settings(&folder));
        } else {
            chain.push(FolderSettings::default());
        }
    }
    Ok(chain)
}
```

In `crates/rocket-infra/src/fs_collection/mod.rs`, add after the `save_folder_settings` method added in Task 2:

```rust
    fn get_folder_chain_settings(
        &self,
        collection: &str,
        request_path: &str,
    ) -> DomainResult<Vec<FolderSettings>> {
        folder_settings::get_folder_chain_settings(self, collection, request_path)
    }
```

In `crates/rocket-infra/src/shared_path_collection_repo.rs`, replace the import:

```rust
use rocket_collection::{
    Collection, CollectionRepository, CollectionSettings, CollectionSummary, CollectionVariable,
    GraphQlRequest, GrpcRequest, Request, RequestKind, WebSocketRequest,
};
```

with:

```rust
use rocket_collection::{
    Collection, CollectionRepository, CollectionSettings, CollectionSummary, CollectionVariable,
    FolderSettings, GraphQlRequest, GrpcRequest, Request, RequestKind, WebSocketRequest,
};
```

and replace:

```rust
    fn save_folder_variables(
        &self,
        collection: &str,
        folder_path: &str,
        vars: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        self.repo()
            .save_folder_variables(collection, folder_path, vars)
    }
```

with:

```rust
    fn save_folder_variables(
        &self,
        collection: &str,
        folder_path: &str,
        vars: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        self.repo()
            .save_folder_variables(collection, folder_path, vars)
    }

    fn get_folder_settings(
        &self,
        collection: &str,
        folder_path: &str,
    ) -> DomainResult<FolderSettings> {
        self.repo().get_folder_settings(collection, folder_path)
    }

    fn save_folder_settings(
        &self,
        collection: &str,
        folder_path: &str,
        settings: &FolderSettings,
    ) -> DomainResult<()> {
        self.repo()
            .save_folder_settings(collection, folder_path, settings)
    }

    fn get_folder_chain_settings(
        &self,
        collection: &str,
        request_path: &str,
    ) -> DomainResult<Vec<FolderSettings>> {
        self.repo()
            .get_folder_chain_settings(collection, request_path)
    }
```

- [ ] **Step 5: Extend the schema checker**

In `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs`, replace `check_request_defaults`:

```rust
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
```

with:

```rust
fn check_request_defaults(v: &mut Violations, at: &str, req: &Value) {
    v.keys("RequestDefaults", at, req, REQUEST_DEFAULTS);
    for h in seq(req.get("headers")) {
        v.keys("HttpRequestHeader", at, h, HEADER);
    }
    for m in seq(req.get("metadata")) {
        v.keys("GrpcMetadata", at, m, GRPC_METADATA);
    }
    for var in seq(req.get("variables")) {
        v.keys("Variable", at, var, VARIABLE);
    }
    for s in seq(req.get("scripts")) {
        v.keys("Script", at, s, SCRIPT);
    }
    if let Some(settings) = req.get("settings") {
        v.keys("RequestSettings", at, settings, HTTP_SETTINGS);
    }
    if let Some(auth) = req.get("auth") {
        check_auth(v, at, auth);
    }
}
```

`HTTP_SETTINGS` (`encodeUrl`, `timeout`, `followRedirects`, `maxRedirects`) is the same key set as the spec's generic request settings, which is what `OcRequestSettings` writes.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-infra fs_collection::folder_settings`
Expected: PASS, 15 tests.

Run: `cargo test -j4 -p rocket-infra shared_path_collection_repo::`
Expected: PASS.

Run: `cargo test -j4 -p rocket-infra schema_shape_tests::`
Expected: PASS, including `fully_populated_folder_yml_only_uses_schema_keys`, `checker_flags_known_bad_shapes` and the existing `written_collection_files_only_use_schema_keys` (which now also checks scripts, metadata and settings in `opencollection.yml` and `users/folder.yml`).

Run: `cargo test -j4 -p rocket-infra fs_collection::`
Expected: PASS.

Run: `cargo check -j4 -p rocket-infra --tests`
Expected: no new warnings in the touched files.

- [ ] **Step 7: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path and commit with a pathspec:

```bash
git add crates/rocket-infra/src/fs_collection/folder_settings.rs crates/rocket-infra/src/fs_collection/mod.rs crates/rocket-infra/src/shared_path_collection_repo.rs crates/rocket-infra/src/fs_collection/schema_shape_tests.rs
git commit -m "<message from the skill>" -- crates/rocket-infra/src/fs_collection/folder_settings.rs crates/rocket-infra/src/fs_collection/mod.rs crates/rocket-infra/src/shared_path_collection_repo.rs crates/rocket-infra/src/fs_collection/schema_shape_tests.rs
```

Suggested subject: `feat(infra): load the folder settings chain for a request`.

---

## Next Plan

[Plan 03: Script flow setting](2026-10-07-folder-settings-plan-03-script-flow-setting.md) (`extensions.bruno.scripts.flow` in `opencollection.yml`, `CollectionSettings.script_flow`). It depends on Plans 01 and 02. Chain to it automatically when this plan finishes. Plan 04 (IPC commands) also only needs this plan.
