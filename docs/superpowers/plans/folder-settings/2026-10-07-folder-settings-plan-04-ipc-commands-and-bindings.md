# Folder settings, Plan 04: IPC commands, DTOs, domain event and TS bindings

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The frontend can read and write one folder's settings (headers, auth, variables, three scripts, docs) over IPC. A save publishes a `FolderSettingsSaved` domain event that reaches the frontend on the existing `collection-changed` channel.

**Architecture:** `rocket-shared` gains `DomainEvent::FolderSettingsSaved { collection, folder_path }`, routed to `collection-changed` by `TauriEventBus`. `CollectionService` gains `get_folder_settings` and `save_folder_settings`, which call the repository methods from Plan 02 and publish the event only after a successful save. `src-tauri` gains a camelCase `FolderSettingsDto` in its own module `commands/folder_settings_dto.rs`, with lossless `From` conversions in both directions, plus two thin commands in `commands/collections.rs` that reject escaping folder paths and otherwise pass `DomainError` through unchanged (it serializes to its stable `Display` string). The frontend gains the `FolderSettings` TS type next to `CollectionSettings`, the `getFolderSettings` and `saveFolderSettings` wrappers next to the folder-variable wrappers, and the `FOLDER_SETTINGS_SAVED_EVENT` constant.

**Tech Stack:** Rust (serde, serde_json, thiserror), Tauri 2 IPC, React + TypeScript, Vitest. Rust tests run with `cargo test -j4 -p <crate> <name>`. The `src-tauri` package is named `rocket`.

**Spec:** [docs/superpowers/specs/2026-10-07-folder-settings-design.md](../../specs/2026-10-07-folder-settings-design.md). Locked names: [00-plan-index.md](00-plan-index.md), section "IPC (plan 04)".

**Depends on:** Plan 02. It provides `CollectionRepository::get_folder_settings` and `save_folder_settings`, and their `FsCollectionRepo` implementations. Plan 01 provides `rocket_collection::FolderSettings` (re-exported at the crate root, fields exactly as in the index, derives `Debug, Clone, PartialEq, Default`).

## Global Constraints

- `#[serde(rename_all = "camelCase")]` goes on `FolderSettingsDto` only. Do not add serde attributes to the domain `FolderSettings` or to any `Oc*` persistence struct.
- The DTO reuses the IPC shapes that `get_collection_settings` already sends for the same data: `rocket_shared::types::Header`, `rocket_shared::types::Auth` and `rocket_collection::CollectionVariable`. Do not define new header, auth or variable DTOs.
- Commands stay thin: validate the folder path, call `CollectionService`, convert the result. No business logic in `src-tauri`.
- Errors cross IPC as `DomainError`, which serializes to its `Display` string (for example `Invalid input: ...`). Do not invent a new error type.
- `DomainEvent` field names stay snake_case on the wire (`folder_path`), like the existing `FolderVariablesSaved`.
- Never call `.unwrap` on a `Result` or `Option` in production paths. Tests may use `.expect("reason")`.
- Always pass `-j4` to cargo and target one package. Never run `cargo test --workspace` or `--all`.
- Commit with the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only (never `git add -A` or `git add .`) and commit with a pathspec, because peer sessions share this repo's index.
- Frontend: no UI in this plan. Tasks that touch `src/` end with `yarn tsc --noEmit` and `yarn check`.

## Review Focus

1. A failed save must not publish `FolderSettingsSaved`, otherwise the sidebar refreshes for a write that never happened (Task 1 test `failed_folder_settings_save_publishes_no_event`).
2. The event wire tag is exactly `folderSettingsSaved` with `collection` and `folder_path`, and the TS constant matches it (Task 1 test `folder_settings_saved_wire_shape`, Task 3 test `FOLDER_SETTINGS_SAVED_EVENT matches the Rust event tag`).
3. Domain to DTO and back is lossless for every field. Both `From` impls destructure the source fully, so a new domain field fails to compile instead of being dropped silently (Task 2 test `folder_settings_dto_round_trip_is_lossless`).
4. The DTO speaks camelCase (`preRequestScript`, `postResponseScript`, `testsScript`) and omits empty optional keys (Task 2 tests `folder_settings_dto_uses_camel_case_keys` and `folder_settings_dto_default_omits_optional_keys`).
5. A partial payload from the frontend (missing lists, `auth` set to `inherit`) deserializes with defaults (Task 2 test `folder_settings_dto_accepts_a_partial_payload`).
6. A folder path that is absolute or contains `..` is refused at the command layer with a stable `Invalid input: ...` string, before any service call (Task 2 test `folder_settings_path_rejects_escaping_paths`).
7. The wrappers send the argument names Tauri expects: `collection`, `folderPath` and `settings` (Task 3 tests `getFolderSettings calls get_folder_settings with collection and folderPath` and `saveFolderSettings sends the settings under the settings key`).

---

## Task 1: `FolderSettingsSaved` event and `CollectionService` methods

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-shared/src/events.rs` (variant after `RequestVariablesSaved` at lines 169-172, test after `request_variables_saved_wire_shape` at lines 811-823)
- Modify: `src-tauri/src/tauri_event_bus.rs` (exhaustive match arm at lines 56-61)
- Modify: `crates/rocket-app/src/collection_service.rs` (imports at lines 5-8, methods after `save_folder_variables` which ends at line 423, new test module at end of file after line 1279)
- Test: same files (inline `#[cfg(test)]` modules)

**Interfaces:**
- Consumes (Plan 02): `CollectionRepository::get_folder_settings(&self, collection: &str, folder_path: &str) -> DomainResult<FolderSettings>` and `CollectionRepository::save_folder_settings(&self, collection: &str, folder_path: &str, settings: &FolderSettings) -> DomainResult<()>`. Consumes (Plan 01): `rocket_collection::FolderSettings`.
- Produces: `DomainEvent::FolderSettingsSaved { collection: String, folder_path: String }`; `CollectionService::get_folder_settings(&self, collection: &str, folder_path: &str) -> DomainResult<FolderSettings>`; `CollectionService::save_folder_settings(&self, collection: &str, folder_path: &str, settings: &FolderSettings) -> DomainResult<()>`.

- [ ] **Step 1: Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

Also read `crates/rocket-shared/CLAUDE.md` and `crates/rocket-app/CLAUDE.md`. Confirm Plan 02 is merged: `rg -n "fn save_folder_settings" crates/rocket-collection/src/repository.rs crates/rocket-infra/src/fs_collection/mod.rs` must show both the trait method and the `FsCollectionRepo` impl. Stop and report if it does not.

- [ ] **Step 2: Write the failing event test**

In `crates/rocket-shared/src/events.rs`, add this test right after `request_variables_saved_wire_shape` (the test that ends at line 823):

```rust
    #[test]
    fn folder_settings_saved_wire_shape() {
        let event = DomainEvent::FolderSettingsSaved {
            collection: "my-api".into(),
            folder_path: "auth/login".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"folderSettingsSaved","collection":"my-api","folder_path":"auth/login"}"#
        );
    }
```

- [ ] **Step 3: Run it to verify it fails**

Run: `cargo test -j4 -p rocket-shared folder_settings_saved_wire_shape`
Expected: FAIL to compile with `no variant named FolderSettingsSaved found for enum DomainEvent`.

- [ ] **Step 4: Add the variant**

In `crates/rocket-shared/src/events.rs`, replace lines 169-172:

```rust
    RequestVariablesSaved {
        collection: String,
        request_path: String,
    },
```

with:

```rust
    RequestVariablesSaved {
        collection: String,
        request_path: String,
    },
    /// A folder's settings (folder.yml) were saved from the Folder Settings tab.
    FolderSettingsSaved {
        collection: String,
        folder_path: String,
    },
```

- [ ] **Step 5: Route the event in `TauriEventBus`**

The match in `src-tauri/src/tauri_event_bus.rs` is exhaustive, so `src-tauri` does not compile until the variant has an arm. Replace lines 56-61:

```rust
            DomainEvent::FolderCreated { .. }
            | DomainEvent::FolderDeleted { .. }
            | DomainEvent::ItemsReordered { .. }
            | DomainEvent::CollectionSettingsSaved { .. }
            | DomainEvent::FolderVariablesSaved { .. }
            | DomainEvent::RequestVariablesSaved { .. } => "collection-changed",
```

with:

```rust
            DomainEvent::FolderCreated { .. }
            | DomainEvent::FolderDeleted { .. }
            | DomainEvent::ItemsReordered { .. }
            | DomainEvent::CollectionSettingsSaved { .. }
            | DomainEvent::FolderVariablesSaved { .. }
            | DomainEvent::FolderSettingsSaved { .. }
            | DomainEvent::RequestVariablesSaved { .. } => "collection-changed",
```

- [ ] **Step 6: Run the event test and the src-tauri check**

Run: `cargo test -j4 -p rocket-shared folder_settings_saved_wire_shape`
Expected: PASS.

Run: `cargo check -j4 -p rocket`
Expected: PASS (no non-exhaustive match error).

- [ ] **Step 7: Write the failing service tests**

Append this new test module at the very end of `crates/rocket-app/src/collection_service.rs` (after the `websocket_tests` module, line 1279). It uses the real `FsCollectionRepo` from Plan 02 so it also proves the service reaches the repository. `rocket-infra` and `tempfile` are already dev-dependencies of `rocket-app`.

```rust
#[cfg(test)]
mod folder_settings_tests {
    use super::*;
    use rocket_shared::types::Header;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Recorder(Mutex<Vec<DomainEvent>>);

    struct SharedRecorder(Arc<Recorder>);

    impl EventPublisher for SharedRecorder {
        fn publish(&self, event: DomainEvent) {
            self.0 .0.lock().expect("lock").push(event);
        }
    }

    /// A service over a temp collection "api" with one folder "auth".
    fn service(dir: &std::path::Path) -> (CollectionService, Arc<Recorder>) {
        let repo = rocket_infra::FsCollectionRepo::new_standalone(dir.to_path_buf());
        repo.create("api").expect("create collection");
        repo.create_folder("api", "auth").expect("create folder");
        let recorder = Arc::new(Recorder::default());
        let svc = CollectionService::new(
            Box::new(repo),
            Box::new(SharedRecorder(Arc::clone(&recorder))),
        );
        (svc, recorder)
    }

    fn sample() -> FolderSettings {
        FolderSettings {
            headers: vec![Header::new("X-Team", "core")],
            pre_request_script: Some("console.log('auth pre');".into()),
            docs: Some("# Auth folder".into()),
            ..FolderSettings::default()
        }
    }

    #[test]
    fn save_folder_settings_persists_and_emits_folder_settings_saved() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (svc, recorder) = service(dir.path());

        svc.save_folder_settings("api", "auth", &sample())
            .expect("save_folder_settings");

        assert_eq!(
            svc.get_folder_settings("api", "auth").expect("get_folder_settings"),
            sample()
        );
        let events = recorder.0.lock().expect("lock");
        assert!(
            matches!(
                events.as_slice(),
                [DomainEvent::FolderSettingsSaved { collection, folder_path }]
                    if collection == "api" && folder_path == "auth"
            ),
            "expected one FolderSettingsSaved, got {events:?}"
        );
    }

    #[test]
    fn get_folder_settings_publishes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (svc, recorder) = service(dir.path());

        let settings = svc.get_folder_settings("api", "auth").expect("get");

        assert_eq!(settings, FolderSettings::default());
        assert!(recorder.0.lock().expect("lock").is_empty());
    }

    #[test]
    fn failed_folder_settings_save_publishes_no_event() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (svc, recorder) = service(dir.path());

        // The spec makes a save to a missing folder an InvalidInput error (Plan 02).
        let result = svc.save_folder_settings("api", "ghost", &sample());

        assert!(result.is_err(), "a save to a missing folder must fail");
        assert!(
            recorder.0.lock().expect("lock").is_empty(),
            "a failed save must not publish an event"
        );
    }
}
```

- [ ] **Step 8: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-app folder_settings_tests`
Expected: FAIL to compile with `no method named save_folder_settings found for struct CollectionService` (and the same for `get_folder_settings`).

- [ ] **Step 9: Implement the service methods**

In `crates/rocket-app/src/collection_service.rs`, replace the import at lines 5-8:

```rust
use rocket_collection::{
    Collection, CollectionRepository, CollectionSummary, CollectionVariable, GraphQlRequest,
    GrpcRequest, Request, RequestKind, WebSocketRequest,
};
```

with:

```rust
use rocket_collection::{
    Collection, CollectionRepository, CollectionSummary, CollectionVariable, FolderSettings,
    GraphQlRequest, GrpcRequest, Request, RequestKind, WebSocketRequest,
};
```

Then insert these two methods directly after `save_folder_variables` (after its closing `}` at line 423, before `get_request_variables`):

```rust
    /// Reads one folder's own settings from its folder.yml. No chain walk.
    pub fn get_folder_settings(
        &self,
        collection: &str,
        folder_path: &str,
    ) -> DomainResult<FolderSettings> {
        self.repo.get_folder_settings(collection, folder_path)
    }

    /// Writes one folder's settings, then tells listeners the folder changed.
    pub fn save_folder_settings(
        &self,
        collection: &str,
        folder_path: &str,
        settings: &FolderSettings,
    ) -> DomainResult<()> {
        self.repo
            .save_folder_settings(collection, folder_path, settings)?;
        self.events.publish(DomainEvent::FolderSettingsSaved {
            collection: collection.to_string(),
            folder_path: folder_path.to_string(),
        });
        Ok(())
    }
```

The `MockCollectionRepo` in the existing `tests` module needs no change: Plan 02 gives both repository methods a default body.

- [ ] **Step 10: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app folder_settings_tests`
Expected: PASS (3 tests).

Run: `cargo test -j4 -p rocket-app collection_service`
Expected: PASS (the existing service tests still compile and pass).

- [ ] **Step 11: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path and commit with the same pathspec:

```bash
git add crates/rocket-shared/src/events.rs src-tauri/src/tauri_event_bus.rs \
  crates/rocket-app/src/collection_service.rs
git commit --only -m "<message from the skill>" -- crates/rocket-shared/src/events.rs \
  src-tauri/src/tauri_event_bus.rs crates/rocket-app/src/collection_service.rs
```

Suggested subject: `feat(app): add folder settings service methods and saved event`.

---

## Task 2: `FolderSettingsDto`, Tauri commands and registration

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `src-tauri/src/commands/folder_settings_dto.rs`
- Modify: `src-tauri/src/commands/mod.rs` (add the module after `pub mod flow;`)
- Modify: `src-tauri/src/commands/collections.rs` (imports at lines 1-13, commands after `save_folder_variables` which ends at line 532, tests in the `tests` module at lines 563-579)
- Modify: `src-tauri/src/lib.rs` (handler list, after `commands::collections::save_folder_variables,` at line 649)
- Test: `src-tauri/src/commands/folder_settings_dto.rs` and `src-tauri/src/commands/collections.rs` (inline `#[cfg(test)]` modules)

**Interfaces:**
- Consumes (Task 1): `CollectionService::get_folder_settings`, `CollectionService::save_folder_settings`. Consumes (Plan 01): `rocket_collection::FolderSettings`.
- Produces:
  - `pub struct FolderSettingsDto { headers: Vec<Header>, auth: Option<Auth>, variables: Vec<CollectionVariable>, pre_request_script: Option<String>, post_response_script: Option<String>, tests_script: Option<String>, docs: Option<String> }` (camelCase on the wire), with `impl From<FolderSettings> for FolderSettingsDto` and `impl From<FolderSettingsDto> for FolderSettings`.
  - `#[tauri::command] pub fn get_folder_settings(collection: String, folder_path: String, svc: State<'_, CollectionService>) -> Result<FolderSettingsDto, DomainError>`
  - `#[tauri::command] pub fn save_folder_settings(collection: String, folder_path: String, settings: FolderSettingsDto, svc: State<'_, CollectionService>) -> Result<(), DomainError>`
  - private `fn validate_folder_path(folder_path: &str) -> Result<(), DomainError>` in `collections.rs`.

- [ ] **Step 1: Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

Also read `.claude/rules/tauri-ipc-boundaries.md`. Note how `CollectionSettings` crosses IPC today: `get_collection_settings` returns the domain struct directly because it carries camelCase serde. `FolderSettings` carries no serde, so it crosses through the DTO below and reuses the same `Header`, `Auth` and `CollectionVariable` wire shapes.

- [ ] **Step 2: Write the failing DTO tests**

Create `src-tauri/src/commands/folder_settings_dto.rs` with only the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn full_domain() -> FolderSettings {
        FolderSettings {
            headers: vec![
                Header::new("X-Team", "core"),
                Header::disabled("X-Debug", "1"),
            ],
            auth: Some(Auth::Bearer {
                token: "{{token}}".into(),
            }),
            variables: vec![CollectionVariable {
                key: "baseUrl".into(),
                value: "https://api.example.com".into(),
                initial_value: String::new(),
                enabled: true,
                secret: false,
            }],
            pre_request_script: Some("console.log('pre');".into()),
            post_response_script: Some("console.log('post');".into()),
            tests_script: Some("test('ok', () => {});".into()),
            docs: Some("# Auth".into()),
        }
    }

    #[test]
    fn folder_settings_dto_round_trip_is_lossless() {
        let dto = FolderSettingsDto::from(full_domain());
        assert_eq!(FolderSettings::from(dto), full_domain());
    }

    #[test]
    fn folder_settings_dto_uses_camel_case_keys() {
        let json = serde_json::to_value(FolderSettingsDto::from(full_domain()))
            .expect("serialize dto");
        assert_eq!(json["preRequestScript"], "console.log('pre');");
        assert_eq!(json["postResponseScript"], "console.log('post');");
        assert_eq!(json["testsScript"], "test('ok', () => {});");
        assert_eq!(json["docs"], "# Auth");
        assert_eq!(json["auth"]["authType"], "bearer");
        assert_eq!(json["headers"][1]["enabled"], false);
        assert_eq!(json["variables"][0]["initialValue"], "");
        assert!(json.get("pre_request_script").is_none());
    }

    #[test]
    fn folder_settings_dto_default_omits_optional_keys() {
        let json = serde_json::to_value(FolderSettingsDto::default()).expect("serialize dto");
        assert_eq!(json, serde_json::json!({ "headers": [], "variables": [] }));
    }

    #[test]
    fn folder_settings_dto_accepts_a_partial_payload() {
        let dto: FolderSettingsDto =
            serde_json::from_str(r#"{"docs":"hi","auth":{"authType":"inherit"}}"#)
                .expect("partial payload");
        let settings = FolderSettings::from(dto);
        assert_eq!(settings.docs.as_deref(), Some("hi"));
        assert_eq!(settings.auth, Some(Auth::Inherit));
        assert!(settings.headers.is_empty());
        assert!(settings.variables.is_empty());
        assert!(settings.tests_script.is_none());
    }
}
```

In `src-tauri/src/commands/mod.rs`, add the module line directly after `pub mod flow;`:

```rust
pub mod folder_settings_dto;
```

- [ ] **Step 3: Run them to verify they fail**

Run: `cargo test -j4 -p rocket --lib folder_settings_dto`
Expected: FAIL to compile (`cannot find type FolderSettingsDto in this scope`, and `FolderSettings`, `Header`, `Auth`, `CollectionVariable` not found).

- [ ] **Step 4: Implement the DTO**

Put this above the test module in `src-tauri/src/commands/folder_settings_dto.rs`:

```rust
//! IPC DTO for one folder's settings.
//!
//! The domain `FolderSettings` carries no serde. This DTO owns the camelCase
//! wire shape. Headers, auth and variables reuse the shapes that
//! `get_collection_settings` already sends, so the frontend editors work for both.

use rocket_collection::{CollectionVariable, FolderSettings};
use rocket_shared::types::{Auth, Header};
use serde::{Deserialize, Serialize};

/// Folder settings as the frontend reads and writes them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSettingsDto {
    #[serde(default)]
    pub headers: Vec<Header>,
    /// `None` and `Some(Auth::Inherit)` both mean the folder sets no auth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
    /// Pre-request folder variables.
    #[serde(default)]
    pub variables: Vec<CollectionVariable>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_request_script: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_response_script: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests_script: Option<String>,
    /// Markdown docs content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
}

impl From<FolderSettings> for FolderSettingsDto {
    fn from(settings: FolderSettings) -> Self {
        // Destructure fully, so a new domain field fails to compile here.
        let FolderSettings {
            headers,
            auth,
            variables,
            pre_request_script,
            post_response_script,
            tests_script,
            docs,
        } = settings;
        Self {
            headers,
            auth,
            variables,
            pre_request_script,
            post_response_script,
            tests_script,
            docs,
        }
    }
}

impl From<FolderSettingsDto> for FolderSettings {
    fn from(dto: FolderSettingsDto) -> Self {
        // Destructure fully, so a new DTO field fails to compile here.
        let FolderSettingsDto {
            headers,
            auth,
            variables,
            pre_request_script,
            post_response_script,
            tests_script,
            docs,
        } = dto;
        Self {
            headers,
            auth,
            variables,
            pre_request_script,
            post_response_script,
            tests_script,
            docs,
        }
    }
}
```

- [ ] **Step 5: Run the DTO tests to verify they pass**

Run: `cargo test -j4 -p rocket --lib folder_settings_dto`
Expected: PASS (4 tests).

- [ ] **Step 6: Write the failing path-validation tests**

In `src-tauri/src/commands/collections.rs`, add these tests inside the existing `mod tests` (after `collection_summary_dto_contains_scoped_repository_id`, before the module's closing `}` at line 579):

```rust
    #[test]
    fn folder_settings_path_accepts_root_and_nested_folders() {
        assert!(validate_folder_path("").is_ok());
        assert!(validate_folder_path("auth").is_ok());
        assert!(validate_folder_path("auth/login").is_ok());
    }

    #[test]
    fn folder_settings_path_rejects_escaping_paths() {
        for bad in ["../outside", "auth/../../x", "/etc"] {
            let err = validate_folder_path(bad).expect_err(bad);
            assert!(matches!(err, DomainError::InvalidInput(_)), "{bad}: {err:?}");
            let wire = serde_json::to_value(&err).expect("serialize error");
            assert_eq!(
                wire,
                serde_json::json!(format!(
                    "Invalid input: folder path must stay inside the collection: {bad}"
                ))
            );
        }
    }
```

- [ ] **Step 7: Run them to verify they fail**

Run: `cargo test -j4 -p rocket --lib folder_settings_path`
Expected: FAIL to compile (`cannot find function validate_folder_path in this scope`).

- [ ] **Step 8: Implement the commands and the validator**

In `src-tauri/src/commands/collections.rs`, replace the import block at lines 3-6:

```rust
use rocket_collection::{
    Collection, CollectionSummary, CollectionVariable, GraphQlRequest, GrpcRequest, Request,
    WebSocketRequest,
};
```

with:

```rust
use rocket_collection::{
    Collection, CollectionSummary, CollectionVariable, FolderSettings, GraphQlRequest,
    GrpcRequest, Request, WebSocketRequest,
};
```

and add this line after `use rocket_workspace::RepositoryId;` (line 8):

```rust
use super::folder_settings_dto::FolderSettingsDto;
```

Then insert after `save_folder_variables` (after its closing `}` at line 532, before `get_request_variables`):

```rust
/// Refuses folder paths that are absolute or climb out of the collection.
/// The repository checks again, this keeps a bad path from reaching it.
fn validate_folder_path(folder_path: &str) -> Result<(), DomainError> {
    use std::path::Component;
    let escapes = Path::new(folder_path).components().any(|c| {
        matches!(
            c,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    });
    if escapes {
        return Err(DomainError::InvalidInput(format!(
            "folder path must stay inside the collection: {folder_path}"
        )));
    }
    Ok(())
}

#[tauri::command]
pub fn get_folder_settings(
    collection: String,
    folder_path: String,
    svc: State<'_, CollectionService>,
) -> Result<FolderSettingsDto, DomainError> {
    validate_folder_path(&folder_path)?;
    svc.get_folder_settings(&collection, &folder_path)
        .map(FolderSettingsDto::from)
}

#[tauri::command]
pub fn save_folder_settings(
    collection: String,
    folder_path: String,
    settings: FolderSettingsDto,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    validate_folder_path(&folder_path)?;
    svc.save_folder_settings(&collection, &folder_path, &FolderSettings::from(settings))
}
```

In `src-tauri/src/lib.rs`, in the `tauri::generate_handler![...]` list, replace line 649:

```rust
            commands::collections::save_folder_variables,
```

with:

```rust
            commands::collections::save_folder_variables,
            commands::collections::get_folder_settings,
            commands::collections::save_folder_settings,
```

- [ ] **Step 9: Run the tests and checks to verify they pass**

Run: `cargo test -j4 -p rocket --lib folder_settings`
Expected: PASS (4 DTO tests and 2 path tests).

Run: `cargo check -j4 -p rocket`
Expected: PASS, with no new warnings.

Run: `cargo clippy -j4 -p rocket`
Expected: no warnings on lines this task added. Warnings that already existed elsewhere are out of scope.

- [ ] **Step 10: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path and commit with the same pathspec:

```bash
git add src-tauri/src/commands/folder_settings_dto.rs src-tauri/src/commands/mod.rs \
  src-tauri/src/commands/collections.rs src-tauri/src/lib.rs
git commit --only -m "<message from the skill>" -- src-tauri/src/commands/folder_settings_dto.rs \
  src-tauri/src/commands/mod.rs src-tauri/src/commands/collections.rs src-tauri/src/lib.rs
```

Suggested subject: `feat(ipc): add folder settings commands and DTO`.

---

## Task 3: Frontend `FolderSettings` type, wrappers and event constant

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src/lib/tauri-api.ts` (type after `CollectionSettings` at lines 83-91, wrappers after `saveFolderVariables` at lines 1524-1528, event constant and field near `CollectionChangedEvent` at lines 1335-1344)
- Test: `src/lib/__tests__/tauri-api.test.ts` (import at line 3, new `describe` at the end of the file after line 103)

**Interfaces:**
- Consumes (Task 2): Tauri commands `get_folder_settings` (args `collection`, `folderPath`) returning the `FolderSettingsDto` JSON, and `save_folder_settings` (args `collection`, `folderPath`, `settings`). Consumes (Task 1): the `collection-changed` payload `{ type: 'folderSettingsSaved', collection, folder_path }`.
- Produces:
  - `export interface FolderSettings { headers: Header[]; auth?: Auth; variables: CollectionVariable[]; preRequestScript?: string; postResponseScript?: string; testsScript?: string; docs?: string }`
  - `export const getFolderSettings: (collection: string, folderPath: string) => Promise<FolderSettings>`
  - `export const saveFolderSettings: (collection: string, folderPath: string, settings: FolderSettings) => Promise<void>`
  - `export const FOLDER_SETTINGS_SAVED_EVENT = 'folderSettingsSaved'`
  - `CollectionChangedEvent.folder_path?: string`

- [ ] **Step 1: Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

Also read `.claude/rules/frontend-component-guardrails.md` and the existing mock setup at the top of `src/lib/__tests__/tauri-api.test.ts` (`vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))`).

- [ ] **Step 2: Write the failing Vitest tests**

In `src/lib/__tests__/tauri-api.test.ts`, replace line 3:

```ts
import { isGitSshTrustFailure, parseGitNetworkError } from '../tauri-api';
```

with:

```ts
import {
  FOLDER_SETTINGS_SAVED_EVENT,
  type FolderSettings,
  getFolderSettings,
  isGitSshTrustFailure,
  parseGitNetworkError,
  saveFolderSettings,
} from '../tauri-api';
```

Append at the end of the file:

```ts
describe('folder settings bindings', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it('getFolderSettings calls get_folder_settings with collection and folderPath', async () => {
    const settings: FolderSettings = { headers: [], variables: [], docs: '# Auth' };
    vi.mocked(invoke).mockResolvedValueOnce(settings);

    await expect(getFolderSettings('my-api', 'auth/login')).resolves.toEqual(settings);

    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith('get_folder_settings', {
      collection: 'my-api',
      folderPath: 'auth/login',
    });
  });

  it('saveFolderSettings sends the settings under the settings key', async () => {
    const settings: FolderSettings = {
      headers: [{ key: 'X-Team', value: 'core', enabled: true }],
      auth: { authType: 'bearer', token: '{{token}}' },
      variables: [],
      preRequestScript: "console.log('pre');",
      testsScript: "test('ok', () => {});",
    };
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    await saveFolderSettings('my-api', 'auth', settings);

    expect(invoke).toHaveBeenCalledWith('save_folder_settings', {
      collection: 'my-api',
      folderPath: 'auth',
      settings,
    });
  });

  it('FOLDER_SETTINGS_SAVED_EVENT matches the Rust event tag', () => {
    // Must equal the tag asserted in folder_settings_saved_wire_shape (rocket-shared).
    expect(FOLDER_SETTINGS_SAVED_EVENT).toBe('folderSettingsSaved');
  });
});
```

- [ ] **Step 3: Run them to verify they fail**

Run: `yarn test --run src/lib/__tests__/tauri-api.test.ts`
Expected: FAIL (`getFolderSettings is not a function`, the new imports are undefined).

- [ ] **Step 4: Add the type, wrappers and event constant**

In `src/lib/tauri-api.ts`, insert directly after the `CollectionSettings` interface (after its closing `}` at line 91):

```ts

/** One folder's own settings from its folder.yml. Mirrors `FolderSettingsDto` in Rust. */
export interface FolderSettings {
  headers: Header[];
  /** Absent or `inherit` both mean the folder sets no auth. */
  auth?: Auth;
  /** Pre-request folder variables. */
  variables: CollectionVariable[];
  preRequestScript?: string;
  postResponseScript?: string;
  testsScript?: string;
  /** Markdown docs content. */
  docs?: string;
}
```

Replace the `saveFolderVariables` wrapper (lines 1524-1528):

```ts
export const saveFolderVariables = (
  collection: string,
  folderPath: string,
  variables: CollectionVariable[],
) => invoke<void>('save_folder_variables', { collection, folderPath, vars: variables });
```

with:

```ts
export const saveFolderVariables = (
  collection: string,
  folderPath: string,
  variables: CollectionVariable[],
) => invoke<void>('save_folder_variables', { collection, folderPath, vars: variables });

// Folder settings: headers, auth, vars, scripts and docs of one folder.yml (no chain walk).
export const getFolderSettings = (collection: string, folderPath: string) =>
  invoke<FolderSettings>('get_folder_settings', { collection, folderPath });
export const saveFolderSettings = (
  collection: string,
  folderPath: string,
  settings: FolderSettings,
) => invoke<void>('save_folder_settings', { collection, folderPath, settings });
```

Replace the `CollectionChangedEvent` interface (lines 1335-1344):

```ts
export interface CollectionChangedEvent {
  type: string;
  /** Null when a watched file is outside any collection. */
  collection?: string | null;
  name?: string;
  oldName?: string;
  newName?: string;
  path?: string;
  eventType?: string;
}
```

with:

```ts
/** `type` of the collection-changed payload sent after a folder settings save. */
export const FOLDER_SETTINGS_SAVED_EVENT = 'folderSettingsSaved';

export interface CollectionChangedEvent {
  type: string;
  /** Null when a watched file is outside any collection. */
  collection?: string | null;
  name?: string;
  oldName?: string;
  newName?: string;
  path?: string;
  /** Set by folder events. Snake case, because Rust event fields are sent as is. */
  folder_path?: string;
  eventType?: string;
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `yarn test --run src/lib/__tests__/tauri-api.test.ts`
Expected: PASS (all existing tests plus the 3 new ones).

- [ ] **Step 6: Type check and lint**

Run: `yarn tsc --noEmit`
Expected: PASS.

Run: `yarn check`
Expected: PASS. If Biome only reports the import order in `tauri-api.test.ts`, run `yarn format`, confirm the diff touches only that import, and run `yarn check` again.

- [ ] **Step 7: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path and commit with the same pathspec:

```bash
git add src/lib/tauri-api.ts src/lib/__tests__/tauri-api.test.ts
git commit --only -m "<message from the skill>" -- src/lib/tauri-api.ts \
  src/lib/__tests__/tauri-api.test.ts
```

Suggested subject: `feat(api): add folder settings bindings`.

---

## Final verification

Run:
- `cargo test -j4 -p rocket-shared folder_settings_saved_wire_shape`
- `cargo test -j4 -p rocket-app folder_settings_tests`
- `cargo test -j4 -p rocket --lib folder_settings`
- `cargo check -j4 -p rocket`
- `yarn test --run src/lib/__tests__/tauri-api.test.ts`
- `yarn tsc --noEmit`
- `yarn check`

Expected: all PASS. No UI exists yet, so there is no manual check in this plan. Plan 08 opens the tab and Plan 09 is the first to call these wrappers.

---

## Next Plan

**Execution order:** this is plan 04 of 12. The series runs strictly one plan at a time, in numeric order (see [00-plan-index.md](00-plan-index.md)).

**Next to execute:** [Plan 05: Runtime header and auth inheritance](2026-10-07-folder-settings-plan-05-runtime-headers-auth.md).

**Instruction for the executing Claude:** when every task in this plan is done and its verification commands (listed in the last step of each task) pass, record "Task N: complete" for each task in the ledger at `.superpowers/sdd/folder-settings/progress.md`, then open Plan 05 and start its first task immediately. Do not ask the user for consent between plans. Stop and report to the user instead if a verification command fails, if a plan's stated prerequisite is missing, or if you must deviate from the locked contract in the index.

**Notes for the next plan (from the plan author):**

[Plan 05: Runtime header and auth inheritance](2026-10-07-folder-settings-plan-05-runtime-headers-auth.md). It depends on Plans 01 and 02, not on this plan. Plans 08 to 11 depend only on this plan, so they may run next instead (see the recommended order in [00-plan-index.md](00-plan-index.md)). Chain to the next plan automatically when this one finishes.
