# Assistant Folder Tools Implementation Plan 01: Backend

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The workspace assistant can read a folder's settings and propose changes to them that the user accepts or rejects.

**Architecture:** `rocket-acp` gets the pure data (`FolderSettingsPatch`, the `UpdateFolderSettings` change). `McpToolService` gets a masked read, `get_folder_settings`. `ProposalService` gets prepare, still-applies and apply for the new change, saving through `CollectionService::save_folder_settings` with a fingerprint checked on the very copy it saves.

**Tech Stack:** Rust (`rocket-acp`, `rocket-app`, one DTO arm in `src-tauri`), serde_json, sha256 via the existing `sha256_hex`.

**Spec:** [../../specs/2026-10-11-assistant-folder-tools-design.md](../../specs/2026-10-11-assistant-folder-tools-design.md). Index and rulings: [00-index.md](00-index.md).

## Global Constraints

- `folder.yml` stays strictly OpenCollection. The write goes through `CollectionService::save_folder_settings` only. No Rocket-only key.
- No `auth` field in the patch. Folder auth is never changed by a proposal.
- `rocket-acp` has no dependency on `rocket-collection`: the patch uses `ProposedVariable`.
- A summary never holds a header value, variable value, script body or docs text.
- The fingerprint is checked on the very copy that is saved (no second unchecked read).
- Rust: no panicking unwrap calls in production paths. Comments are short full sentences ending with a period.
- Cargo: always `-j4`. Never `cargo test --workspace` or `--all`.
- Commit through the `dev-workflow-skills:1-git-commit` skill with explicit paths.

## Review Focus

- A patch whose strings contain the masked placeholder is refused, so a masked read can never be written back (pinned in T3).
- Two variables with the same key, or an empty key, in one patch are refused (T3).
- A script given as `""` clears that script and leaves the others (T3).
- A folder changed by the user between propose and accept makes the proposal `Stale` and writes nothing (T3).
- `auth` and every section the patch omits survive an accept (T3).

---

### Task 1: Types in `rocket-acp`, with the compile fixes they force

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-acp/src/proposal.rs`
- Modify: `crates/rocket-acp/src/lib.rs` (re-export)
- Modify: `crates/rocket-app/src/proposal_service.rs` (only the arms that keep the match exhaustive)
- Modify: `src-tauri/src/commands/agent_proposals.rs` (DTO and its `From` arm)
- Modify: `crates/rocket-acp/CLAUDE.md` (module map row)

**Interfaces:**
- Consumes: `rocket_shared::types::Header`.
- Produces: `ProposedVariable`, `FolderSettingsPatch` (with `changed_fields`, `is_empty`), `ProposedChange::UpdateFolderSettings`, and the DTO `ProposedChangeDto::UpdateFolderSettings` (tag `updateFolderSettings`).

- [ ] **Step 1: Write the failing tests** at the end of `mod tests` in `crates/rocket-acp/src/proposal.rs`:

```rust
    fn folder_update(patch: FolderSettingsPatch) -> ProposedChange {
        ProposedChange::UpdateFolderSettings {
            collection: "demo".into(),
            folder_path: "auth/v2".into(),
            patch,
            base_fingerprint: String::new(),
        }
    }

    #[test]
    fn a_folder_patch_lists_the_sections_it_sets_in_a_fixed_order() {
        assert!(FolderSettingsPatch::default().is_empty());
        let patch = FolderSettingsPatch {
            docs: Some("notes".into()),
            headers: Some(vec![]),
            pre_request_script: Some(String::new()),
            ..FolderSettingsPatch::default()
        };
        assert!(!patch.is_empty());
        assert_eq!(
            patch.changed_fields(),
            vec!["headers", "pre-request script", "docs"]
        );
    }

    #[test]
    fn a_folder_update_summary_names_sections_and_never_values() {
        let change = folder_update(FolderSettingsPatch {
            headers: Some(vec![Header::new("X-Key", "header-value-must-not-leak")]),
            variables: Some(vec![ProposedVariable {
                key: "HOST".into(),
                value: "variable-value-must-not-leak".into(),
                enabled: true,
            }]),
            tests_script: Some("script-body-must-not-leak".into()),
            ..FolderSettingsPatch::default()
        });
        let summary = change.summary();
        assert_eq!(
            summary,
            "Update headers, variables, tests script of folder 'auth/v2' in demo"
        );
        assert!(!summary.contains("must-not-leak"));
    }

    #[test]
    fn a_folder_update_has_a_collection_and_takes_a_fingerprint() {
        let mut change = folder_update(FolderSettingsPatch::default());
        assert_eq!(change.collection(), "demo");
        assert_eq!(change.base_fingerprint(), Some(""));
        change.set_base_fingerprint("abc".into());
        assert_eq!(change.base_fingerprint(), Some("abc"));
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p rocket-acp -j4 folder`
Expected: compile error, `FolderSettingsPatch` not found.

- [ ] **Step 3: Add the types** in `proposal.rs`, after `RequestPatch`'s `impl`:

```rust
/// One folder variable the agent proposes. There is no secret flag: a folder
/// variable is never secret at run time.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposedVariable {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

/// A partial update of a folder's settings. `None` keeps a section as it is.
/// A given section replaces the whole section. A script given as an empty
/// string clears it. Auth cannot be patched.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FolderSettingsPatch {
    pub headers: Option<Vec<Header>>,
    pub variables: Option<Vec<ProposedVariable>>,
    pub pre_request_script: Option<String>,
    pub post_response_script: Option<String>,
    pub tests_script: Option<String>,
    pub docs: Option<String>,
}

impl FolderSettingsPatch {
    /// Names of the sections this patch sets, in a fixed order.
    pub fn changed_fields(&self) -> Vec<&'static str> {
        let mut fields = Vec::new();
        if self.headers.is_some() {
            fields.push("headers");
        }
        if self.variables.is_some() {
            fields.push("variables");
        }
        if self.pre_request_script.is_some() {
            fields.push("pre-request script");
        }
        if self.post_response_script.is_some() {
            fields.push("post-response script");
        }
        if self.tests_script.is_some() {
            fields.push("tests script");
        }
        if self.docs.is_some() {
            fields.push("docs");
        }
        fields
    }

    /// True when the patch sets no section.
    pub fn is_empty(&self) -> bool {
        self.changed_fields().is_empty()
    }
}
```

Add the variant to `ProposedChange` (after `SetEnvVar`):

```rust
    UpdateFolderSettings {
        collection: String,
        folder_path: String,
        patch: FolderSettingsPatch,
        base_fingerprint: String,
    },
```

In `collection()` add `| ProposedChange::UpdateFolderSettings { collection, .. }` to the or-pattern. In `base_fingerprint()` and `set_base_fingerprint()` add `ProposedChange::UpdateFolderSettings { base_fingerprint, .. }` to the update-style or-patterns. In `summary()` add:

```rust
            ProposedChange::UpdateFolderSettings {
                collection,
                folder_path,
                patch,
                ..
            } => format!(
                "Update {} of folder '{folder_path}' in {collection}",
                patch.changed_fields().join(", ")
            ),
```

In `lib.rs`, add `FolderSettingsPatch, ProposedVariable` to the `pub use proposal::{...}` list.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p rocket-acp -j4 folder`
Expected: PASS (3 tests).

- [ ] **Step 5: Keep the workspace compiling.** Run `cargo check --workspace --all-targets -j4`. It fails on exhaustive matches. Fix them:

In `crates/rocket-app/src/proposal_service.rs`:
  - `normalize_paths`: add `ProposedChange::UpdateFolderSettings { folder_path, .. } => trim(folder_path),`.
  - `paths_of`: add `ProposedChange::UpdateFolderSettings { folder_path, .. } => vec![folder_path.as_str()],`.
  - `prepare`, `still_applies` and `apply`: add a temporary arm that Task 3 replaces. In `prepare`: `ProposedChange::UpdateFolderSettings { .. } => return Err(DomainError::InvalidInput("folder settings changes are not supported yet".to_string())),`. In `still_applies` and `apply`: `ProposedChange::UpdateFolderSettings { .. } => Ok(false),`. Mark each with the comment `// Replaced in plan 01 task 3.`

In `src-tauri/src/commands/agent_proposals.rs` add the DTO. Import `FolderSettingsPatch, ProposedVariable` from `rocket_acp::proposal`. Add the variant to `ProposedChangeDto` (after `SetEnvVar`):

```rust
    UpdateFolderSettings {
        collection: String,
        folder_path: String,
        patch: FolderSettingsPatchDto,
    },
```

and the DTOs and conversion:

```rust
/// Only the sections the patch sets are present.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSettingsPatchDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<Vec<Header>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<Vec<ProposedVariableDto>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_request_script: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_response_script: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tests_script: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposedVariableDto {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

impl From<ProposedVariable> for ProposedVariableDto {
    fn from(variable: ProposedVariable) -> Self {
        Self {
            key: variable.key,
            value: variable.value,
            enabled: variable.enabled,
        }
    }
}

impl From<FolderSettingsPatch> for FolderSettingsPatchDto {
    fn from(patch: FolderSettingsPatch) -> Self {
        Self {
            headers: patch.headers,
            variables: patch
                .variables
                .map(|list| list.into_iter().map(Into::into).collect()),
            pre_request_script: patch.pre_request_script,
            post_response_script: patch.post_response_script,
            tests_script: patch.tests_script,
            docs: patch.docs,
        }
    }
}
```

and the arm in `From<ProposedChange>`:

```rust
            ProposedChange::UpdateFolderSettings {
                collection,
                folder_path,
                patch,
                ..
            } => Self::UpdateFolderSettings {
                collection,
                folder_path,
                patch: patch.into(),
            },
```

Add a DTO test in `mod tests` of `agent_proposals.rs`:

```rust
    #[test]
    fn a_folder_update_dto_is_camel_case_and_omits_unset_sections() {
        let dto = ProposedChangeDto::from(ProposedChange::UpdateFolderSettings {
            collection: "demo".into(),
            folder_path: "auth".into(),
            patch: FolderSettingsPatch {
                tests_script: Some("x".into()),
                variables: Some(vec![ProposedVariable {
                    key: "HOST".into(),
                    value: "h".into(),
                    enabled: true,
                }]),
                ..FolderSettingsPatch::default()
            },
            base_fingerprint: "secret-fingerprint".into(),
        });
        let json = serde_json::to_value(&dto).expect("serialize");
        assert_eq!(json["op"], "updateFolderSettings");
        assert_eq!(json["folderPath"], "auth");
        assert_eq!(json["patch"]["testsScript"], "x");
        assert_eq!(json["patch"]["variables"][0]["key"], "HOST");
        assert!(json["patch"].get("headers").is_none());
        assert!(!json.to_string().contains("secret-fingerprint"));
    }
```

- [ ] **Step 6: Verify**

Run: `cargo test -p rocket-acp -j4 folder`, then the DTO test with `cargo test -p <src-tauri package name from src-tauri/Cargo.toml> -j4 folder_update_dto`, then `cargo check --workspace --all-targets -j4`.
Expected: PASS and a clean check.

- [ ] **Step 7: Docs.** In `crates/rocket-acp/CLAUDE.md` extend the `proposal.rs` module-map row with `FolderSettingsPatch`, `ProposedVariable`. Add one design-point line: the folder patch has no auth field and uses `ProposedVariable` because this crate must not depend on `rocket-collection`.

- [ ] **Step 8: Commit** via the `dev-workflow-skills:1-git-commit` skill. Paths: the five files above. Message: `feat(assistant): add the folder settings proposal type`.

---

### Task 2: `get_folder_settings` in `McpToolService`

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/mcp_read_views.rs`
- Modify: `crates/rocket-app/src/mcp_tool_service.rs`
- Modify: `crates/rocket-app/src/lib.rs` (re-export `FolderChildView`, `FolderView`)
- Modify: `crates/rocket-app/CLAUDE.md`

**Interfaces:**
- Consumes: `MaskedFolderSettings::from_settings`, `normalize_folder`, `folder_dir_name`, `CollectionRepository::{get_summaries, get_folder_settings}`.
- Produces: `FolderChildView`, `FolderView`, `McpToolService::get_folder_settings(&self, session_id: &str, collection: &str, folder_path: &str) -> DomainResult<FolderView>`.

- [ ] **Step 1: Write the failing tests** at the end of `mod tests` in `mcp_tool_service.rs` (next to the chip tests; `two_level_tree`, `service_with`, `plain_variable`, `sample_request` exist there):

```rust
    #[test]
    fn get_folder_settings_returns_masked_settings_and_the_children() {
        let repo = ConfigurableCollectionRepo::new();
        repo.with_request("my-api", "login.yml", sample_request("Login"));
        repo.with_summaries("my-api", two_level_tree());
        repo.with_folder_settings(
            "my-api",
            "auth",
            rocket_collection::FolderSettings {
                headers: vec![Header::new("X-Signature", "folder-sig-4242")],
                variables: vec![
                    plain_variable("clientSecret", "folder-secret-8888", true),
                    plain_variable("HOST", "api.example.com", false),
                ],
                pre_request_script: Some("rok.setVar('a', 1);".into()),
                ..Default::default()
            },
        );
        let publisher = RecordingPublisher::new();
        let svc = service_with(
            Arc::clone(&repo),
            FakeEnvRepoFactory::new(),
            Arc::clone(&publisher),
        );

        let view = svc
            .get_folder_settings("s1", "my-api", "/auth/")
            .expect("folder view");

        assert_eq!(view.path, "auth");
        assert_eq!(view.children.len(), 1);
        assert_eq!(view.children[0].name, "refresh.yml");
        assert_eq!(view.children[0].kind, "request");
        let text = serde_json::to_string(&view).expect("serialize");
        assert!(!text.contains("folder-sig-4242"), "{text}");
        assert!(!text.contains("folder-secret-8888"), "{text}");
        assert!(text.contains("api.example.com"));
        assert!(text.contains("rok.setVar('a', 1);"));
        assert!(publisher.events().iter().any(|e| matches!(
            e,
            DomainEvent::AcpToolInvoked { tool, summary, .. }
                if tool == "get_folder_settings" && !summary.contains("folder-sig-4242")
        )));
    }

    #[test]
    fn get_folder_settings_refuses_the_root_a_missing_folder_and_a_bad_path() {
        let repo = ConfigurableCollectionRepo::new();
        repo.with_request("my-api", "login.yml", sample_request("Login"));
        repo.with_summaries("my-api", two_level_tree());
        let publisher = RecordingPublisher::new();
        let svc = service_with(
            Arc::clone(&repo),
            FakeEnvRepoFactory::new(),
            Arc::clone(&publisher),
        );

        let root = svc.get_folder_settings("s1", "my-api", "");
        assert!(
            matches!(&root, Err(DomainError::InvalidInput(m)) if m.contains("get_collection_settings")),
            "{root:?}"
        );
        assert!(matches!(
            svc.get_folder_settings("s1", "my-api", "nope"),
            Err(DomainError::NotFound(_))
        ));
        assert!(matches!(
            svc.get_folder_settings("s1", "my-api", "../auth"),
            Err(DomainError::InvalidInput(_))
        ));
        assert!(matches!(
            svc.get_folder_settings("s1", "other-api", "auth"),
            Err(DomainError::NotFound(_))
        ));
        assert!(
            !publisher
                .events()
                .iter()
                .any(|e| matches!(e, DomainEvent::AcpToolInvoked { .. })),
            "a refused call must not publish an audit event"
        );
    }
```

Also add `("get_folder_settings", svc.get_folder_settings("s1", "nope", "auth").map(|_| ()))` to the result list of the existing test `read_tools_answer_not_found_for_a_missing_collection`, and `("get_folder_settings", svc.get_folder_settings("s1", outside, "auth").map(|_| ()))` to the list in `every_collection_tool_refuses_a_collection_outside_the_workspace`.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p rocket-app -j4 get_folder_settings`
Expected: compile error, no method `get_folder_settings`.

- [ ] **Step 3: Add the views** in `mcp_read_views.rs`, after `MaskedFolderSettings`'s `impl`:

```rust
/// One direct child of a folder, by the name the other tools expect.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct FolderChildView {
    pub name: String,
    /// `folder` or `request`.
    pub kind: &'static str,
}

/// What `get_folder_settings` returns: the folder's own masked settings and
/// its direct children, so the agent can move on to a request or subfolder.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct FolderView {
    pub collection: String,
    pub path: String,
    pub settings: MaskedFolderSettings,
    pub children: Vec<FolderChildView>,
}

/// The folder at `path` (directory names, `/` separated) in a summary tree.
pub(crate) fn find_folder<'a>(root: &'a Folder, path: &str) -> Option<&'a Folder> {
    let mut folder = root;
    for segment in path.split('/').filter(|s| !s.is_empty()) {
        folder = folder.items.iter().find_map(|item| match item {
            CollectionItem::Folder(sub) if folder_dir_name(sub) == segment => Some(&**sub),
            _ => None,
        })?;
    }
    Some(folder)
}

/// Direct subfolders and requests of a folder. Other item kinds are skipped.
pub(crate) fn folder_children(folder: &Folder) -> Vec<FolderChildView> {
    folder
        .items
        .iter()
        .filter_map(|item| match item {
            CollectionItem::Folder(sub) => Some(FolderChildView {
                name: folder_dir_name(sub),
                kind: "folder",
            }),
            CollectionItem::Summary(summary) => summary.file_name.clone().map(|name| {
                FolderChildView {
                    name,
                    kind: "request",
                }
            }),
            _ => None,
        })
        .collect()
}
```

If `folder_dir_name` returns `&str` or the deref `&**sub` does not type-check, adjust to the item's real types (see `walk_outline` in the same file for the working form). Add `CollectionItem` to the `rocket_collection` import if it is missing.

- [ ] **Step 4: Add the method** in `mcp_tool_service.rs`, after `get_collection_settings`:

```rust
    pub fn get_folder_settings(
        &self,
        session_id: &str,
        collection: &str,
        folder_path: &str,
    ) -> DomainResult<FolderView> {
        self.check_in_workspace(session_id, collection)?;
        let Some(path) = normalize_folder(folder_path)? else {
            return Err(DomainError::InvalidInput(
                "the collection root has no folder settings; use get_collection_settings"
                    .to_string(),
            ));
        };
        let tree = self.collection_repo.get_summaries(collection)?;
        let folder = find_folder(&tree.root, &path).ok_or_else(|| {
            DomainError::NotFound(format!("folder '{path}' in '{collection}'"))
        })?;
        let children = folder_children(folder);
        let settings = self
            .collection_repo
            .get_folder_settings(collection, &path)
            .map_err(|_| {
                DomainError::InvalidInput(format!(
                    "could not read the settings of folder '{path}' in '{collection}'"
                ))
            })?;
        let view = FolderView {
            collection: collection.to_string(),
            path: path.clone(),
            settings: MaskedFolderSettings::from_settings(&settings),
            children,
        };
        self.publish_tool_invoked(
            session_id,
            "get_folder_settings",
            format!("read the settings of folder '{path}' in '{collection}'"),
        );
        Ok(view)
    }
```

Import `FolderView`, `find_folder`, `folder_children`, `normalize_folder` and `MaskedFolderSettings` from `crate::mcp_read_views` as the file already imports its neighbors. Add `FolderChildView, FolderView` to the `pub use mcp_read_views::{...}` list in `lib.rs`. Update the module doc list of read tools at the top of `mcp_tool_service.rs` to include `get_folder_settings`.

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p rocket-app -j4 get_folder_settings` and `cargo test -p rocket-app -j4 missing_collection` and `cargo test -p rocket-app -j4 outside_the_workspace`
Expected: PASS.

- [ ] **Step 6: Checks.** `cargo check --workspace --all-targets -j4`.

- [ ] **Step 7: Docs.** In `crates/rocket-app/CLAUDE.md`, add `get_folder_settings` to the `McpToolService` row (masked folder view plus children; the root is refused; a missing folder is `NotFound`).

- [ ] **Step 8: Commit** via the commit skill. Paths: the five files above. Message: `feat(assistant): add the get_folder_settings read tool`.

---

### Task 3: `ProposalService` prepare, still-applies and apply

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (sections 2.7, 5, 6) and `docs/features/folder-settings.md`.

**Files:**
- Modify: `crates/rocket-app/src/proposal_service.rs`
- Modify: `crates/rocket-app/CLAUDE.md`

**Interfaces:**
- Consumes: `ProposedChange::UpdateFolderSettings`, `FolderSettingsPatch`, `ProposedVariable` (T1); `CollectionService::{get_folder_settings, save_folder_settings}`; `REDACTED`; existing `is_folder`, `unchanged`, `sha256_hex`, `refuse_masked_values`.
- Produces: the three arms replacing T1's temporary ones; `folder_fingerprint(&FolderSettings) -> String`; `apply_folder_patch`.

- [ ] **Step 1: Write the failing tests** in `mod tests` of `proposal_service.rs`. Add imports as needed (`FolderSettings`, `Auth`, `FolderSettingsPatch`, `ProposedVariable`). Helpers:

```rust
    fn folder_update(path: &str, patch: FolderSettingsPatch) -> ProposedChange {
        ProposedChange::UpdateFolderSettings {
            collection: "demo".into(),
            folder_path: path.into(),
            patch,
            base_fingerprint: String::new(),
        }
    }

    /// A "reports" folder with a bearer auth, old docs and one header.
    fn fixture_with_folder() -> Fixture {
        let f = fixture();
        f.repo.create_folder("demo", "reports").expect("create folder");
        f.repo
            .save_folder_settings(
                "demo",
                "reports",
                &FolderSettings {
                    headers: vec![Header::new("X-Old", "old")],
                    auth: Some(Auth::Bearer {
                        token: "folder-token-1".into(),
                    }),
                    pre_request_script: Some("old pre".into()),
                    tests_script: Some("old tests".into()),
                    docs: Some("old docs".into()),
                    ..Default::default()
                },
            )
            .expect("save folder settings");
        f
    }
```

Tests (each uses `fixture_with_folder()`):

```rust
    #[test]
    fn accepting_a_folder_update_changes_only_the_given_sections() {
        let f = fixture_with_folder();
        let ids = f
            .svc
            .propose(
                "s1",
                vec![folder_update(
                    "reports",
                    FolderSettingsPatch {
                        headers: Some(vec![Header::new("X-New", "new")]),
                        variables: Some(vec![ProposedVariable {
                            key: "HOST".into(),
                            value: "api.example.com".into(),
                            enabled: true,
                        }]),
                        ..FolderSettingsPatch::default()
                    },
                )],
            )
            .expect("propose");
        let accepted = f.svc.accept("s1", &ids[0]).expect("accept");
        assert_eq!(accepted.status, ProposalStatus::Accepted);

        let saved = f.repo.get_folder_settings("demo", "reports").expect("read");
        assert_eq!(saved.headers.len(), 1);
        assert_eq!(saved.headers[0].key, "X-New");
        assert_eq!(saved.variables.len(), 1);
        assert_eq!(saved.variables[0].key, "HOST");
        assert!(!saved.variables[0].secret);
        // Sections the patch omits, and auth, survive.
        assert_eq!(saved.pre_request_script.as_deref(), Some("old pre"));
        assert_eq!(saved.tests_script.as_deref(), Some("old tests"));
        assert_eq!(saved.docs.as_deref(), Some("old docs"));
        assert_eq!(
            saved.auth,
            Some(Auth::Bearer {
                token: "folder-token-1".into()
            })
        );
        assert!(f.events.events().iter().any(|e| matches!(
            e,
            DomainEvent::FolderSettingsSaved { folder_path, .. } if folder_path == "reports"
        )));
    }

    #[test]
    fn an_empty_script_string_clears_that_script_only() {
        let f = fixture_with_folder();
        let ids = f
            .svc
            .propose(
                "s1",
                vec![folder_update(
                    "reports",
                    FolderSettingsPatch {
                        tests_script: Some(String::new()),
                        ..FolderSettingsPatch::default()
                    },
                )],
            )
            .expect("propose");
        f.svc.accept("s1", &ids[0]).expect("accept");
        let saved = f.repo.get_folder_settings("demo", "reports").expect("read");
        assert_eq!(saved.tests_script, None);
        assert_eq!(saved.pre_request_script.as_deref(), Some("old pre"));
    }

    #[test]
    fn a_folder_edited_after_the_proposal_makes_it_stale_and_writes_nothing() {
        let f = fixture_with_folder();
        let ids = f
            .svc
            .propose(
                "s1",
                vec![folder_update(
                    "reports",
                    FolderSettingsPatch {
                        docs: Some("agent docs".into()),
                        ..FolderSettingsPatch::default()
                    },
                )],
            )
            .expect("propose");
        // The user changes only the auth by hand. That must also make it stale.
        let mut current = f.repo.get_folder_settings("demo", "reports").expect("read");
        current.auth = Some(Auth::Bearer {
            token: "changed".into(),
        });
        f.repo
            .save_folder_settings("demo", "reports", &current)
            .expect("manual save");

        let resolved = f.svc.accept("s1", &ids[0]).expect("accept");
        assert_eq!(resolved.status, ProposalStatus::Stale);
        let saved = f.repo.get_folder_settings("demo", "reports").expect("read");
        assert_eq!(saved.docs.as_deref(), Some("old docs"));
    }

    #[test]
    fn a_folder_update_refuses_the_root_a_missing_folder_and_an_empty_patch() {
        let f = fixture_with_folder();
        let docs = FolderSettingsPatch {
            docs: Some("x".into()),
            ..FolderSettingsPatch::default()
        };
        assert!(matches!(
            f.svc.propose("s1", vec![folder_update("", docs.clone())]),
            Err(DomainError::InvalidInput(_))
        ));
        assert!(matches!(
            f.svc.propose("s1", vec![folder_update("nope", docs.clone())]),
            Err(DomainError::NotFound(_))
        ));
        assert!(matches!(
            f.svc.propose(
                "s1",
                vec![folder_update("reports", FolderSettingsPatch::default())]
            ),
            Err(DomainError::InvalidInput(m)) if m.contains("changes no field")
        ));
        assert!(matches!(
            f.svc.propose("s1", vec![folder_update("../reports", docs)]),
            Err(DomainError::InvalidInput(_))
        ));
    }

    #[test]
    fn a_folder_update_refuses_duplicate_or_empty_variable_names() {
        let f = fixture_with_folder();
        let variable = |key: &str| ProposedVariable {
            key: key.into(),
            value: "v".into(),
            enabled: true,
        };
        for variables in [vec![variable("A"), variable("A")], vec![variable(" ")]] {
            let result = f.svc.propose(
                "s1",
                vec![folder_update(
                    "reports",
                    FolderSettingsPatch {
                        variables: Some(variables),
                        ..FolderSettingsPatch::default()
                    },
                )],
            );
            assert!(matches!(result, Err(DomainError::InvalidInput(_))), "{result:?}");
        }
    }

    #[test]
    fn a_folder_update_with_the_masked_placeholder_is_refused() {
        let f = fixture_with_folder();
        let masked = format!("prefix {REDACTED}");
        let patches = [
            FolderSettingsPatch {
                headers: Some(vec![Header::new("Authorization", masked.clone())]),
                ..FolderSettingsPatch::default()
            },
            FolderSettingsPatch {
                variables: Some(vec![ProposedVariable {
                    key: "K".into(),
                    value: masked.clone(),
                    enabled: true,
                }]),
                ..FolderSettingsPatch::default()
            },
            FolderSettingsPatch {
                pre_request_script: Some(masked.clone()),
                ..FolderSettingsPatch::default()
            },
            FolderSettingsPatch {
                docs: Some(masked),
                ..FolderSettingsPatch::default()
            },
        ];
        for patch in patches {
            let result = f.svc.propose("s1", vec![folder_update("reports", patch)]);
            assert!(
                matches!(&result, Err(DomainError::InvalidInput(m)) if m.contains("masked placeholder")),
                "{result:?}"
            );
        }
    }

    #[test]
    fn the_folder_fingerprint_covers_every_section_including_auth() {
        let base = FolderSettings::default();
        let changed = |edit: fn(&mut FolderSettings)| {
            let mut settings = base.clone();
            edit(&mut settings);
            folder_fingerprint(&settings)
        };
        let reference = folder_fingerprint(&base);
        assert_eq!(reference.len(), 64);
        for fingerprint in [
            changed(|s| s.headers.push(Header::new("A", "b"))),
            changed(|s| s.auth = Some(Auth::Bearer { token: "t".into() })),
            changed(|s| s.pre_request_script = Some("x".into())),
            changed(|s| s.post_response_script = Some("x".into())),
            changed(|s| s.tests_script = Some("x".into())),
            changed(|s| s.docs = Some("x".into())),
        ] {
            assert_ne!(fingerprint, reference);
        }
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p rocket-app -j4 folder_update`
Expected: FAIL (the temporary arm refuses with "not supported yet"; `folder_fingerprint` does not exist).

- [ ] **Step 3: Implement.** In `proposal_service.rs`:

Imports: add `FolderSettingsPatch` to the existing `rocket_acp` import and `CollectionVariable, FolderSettings` to the existing `rocket_collection` import.

Add the free functions near `env_var_fingerprint`:

```rust
/// Fingerprint of a folder's stored settings: headers, variables, the three
/// scripts, docs and auth. Auth is included so a manual auth change also makes
/// a pending proposal stale, although a proposal never writes it.
fn folder_fingerprint(settings: &FolderSettings) -> String {
    let headers: Vec<_> = settings
        .headers
        .iter()
        .map(|h| (&h.key, &h.value, h.enabled))
        .collect();
    let variables: Vec<_> = settings
        .variables
        .iter()
        .map(|v| (&v.key, &v.value, &v.initial_value, v.enabled, v.secret))
        .collect();
    let text = serde_json::json!({
        "headers": headers,
        "variables": variables,
        "auth": settings.auth,
        "pre": settings.pre_request_script,
        "post": settings.post_response_script,
        "tests": settings.tests_script,
        "docs": settings.docs,
    })
    .to_string();
    sha256_hex(&text)
}

/// Replaces the sections the patch sets. Auth is never touched.
fn apply_folder_patch(settings: &mut FolderSettings, patch: &FolderSettingsPatch) {
    if let Some(headers) = &patch.headers {
        settings.headers = headers.clone();
    }
    if let Some(variables) = &patch.variables {
        // An equal initial value means "no initial value" in folder.yml.
        settings.variables = variables
            .iter()
            .map(|v| CollectionVariable {
                key: v.key.clone(),
                value: v.value.clone(),
                initial_value: v.value.clone(),
                enabled: v.enabled,
                secret: false,
            })
            .collect();
    }
    if let Some(script) = &patch.pre_request_script {
        settings.pre_request_script = Some(script.clone());
    }
    if let Some(script) = &patch.post_response_script {
        settings.post_response_script = Some(script.clone());
    }
    if let Some(script) = &patch.tests_script {
        settings.tests_script = Some(script.clone());
    }
    if let Some(docs) = &patch.docs {
        settings.docs = Some(docs.clone());
    }
}

/// Refuses a variable list with an empty or repeated name.
fn check_folder_variables(patch: &FolderSettingsPatch) -> DomainResult<()> {
    let Some(variables) = &patch.variables else {
        return Ok(());
    };
    let mut seen = HashSet::new();
    for variable in variables {
        if variable.key.trim().is_empty() {
            return Err(DomainError::InvalidInput(
                "variable name cannot be empty".to_string(),
            ));
        }
        if !seen.insert(variable.key.as_str()) {
            return Err(DomainError::InvalidInput(format!(
                "duplicate folder variable '{}'",
                variable.key
            )));
        }
    }
    Ok(())
}
```

A method on `ProposalService`, next to `http_request_fingerprint`:

```rust
    /// The fingerprint of a folder's stored settings, read now.
    fn folder_settings_fingerprint(
        &self,
        collection: &str,
        folder_path: &str,
    ) -> DomainResult<String> {
        let settings = self.collections.get_folder_settings(collection, folder_path)?;
        Ok(folder_fingerprint(&settings))
    }
```

In `prepare`, replace the temporary arm:

```rust
            ProposedChange::UpdateFolderSettings {
                folder_path, patch, ..
            } => {
                if folder_path.is_empty() {
                    return Err(DomainError::InvalidInput(
                        "the collection root has no folder settings".to_string(),
                    ));
                }
                if patch.is_empty() {
                    return Err(DomainError::InvalidInput(
                        "the patch changes no field".to_string(),
                    ));
                }
                check_folder_variables(patch)?;
                let root = self.tree(&collection)?;
                if !is_folder(&root, folder_path) {
                    return Err(DomainError::NotFound(format!(
                        "folder '{folder_path}' in '{collection}'"
                    )));
                }
                Some(self.folder_settings_fingerprint(&collection, folder_path)?)
            }
```

In `still_applies`, replace the temporary arm:

```rust
            ProposedChange::UpdateFolderSettings {
                collection,
                folder_path,
                base_fingerprint,
                ..
            } => {
                let root = self.tree(collection)?;
                if !is_folder(&root, folder_path) {
                    return Ok(false);
                }
                unchanged(
                    self.folder_settings_fingerprint(collection, folder_path),
                    base_fingerprint,
                )
            }
```

In `apply`, replace the temporary arm. The fingerprint is checked on the very copy that is patched and saved:

```rust
            ProposedChange::UpdateFolderSettings {
                collection,
                folder_path,
                patch,
                base_fingerprint,
            } => {
                let mut settings = self.collections.get_folder_settings(collection, folder_path)?;
                if folder_fingerprint(&settings) != *base_fingerprint {
                    return Ok(false);
                }
                apply_folder_patch(&mut settings, patch);
                applied(
                    self.collections
                        .save_folder_settings(collection, folder_path, &settings),
                )
            }
```

In `refuse_masked_values`, add an arm before `_ => false`:

```rust
        ProposedChange::UpdateFolderSettings { patch, .. } => {
            patch
                .headers
                .iter()
                .flatten()
                .any(|h| pair_masked(&h.key, &h.value))
                || patch
                    .variables
                    .iter()
                    .flatten()
                    .any(|v| pair_masked(&v.key, &v.value))
                || [
                    &patch.pre_request_script,
                    &patch.post_response_script,
                    &patch.tests_script,
                    &patch.docs,
                ]
                .into_iter()
                .any(|text| text.as_deref().is_some_and(masked))
        }
```

Check `unchanged`'s signature (`fn unchanged(current: DomainResult<String>, base: &str)`) and adapt the call if the compiler asks.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p rocket-app -j4 folder_update` and `cargo test -p rocket-app -j4 folder_fingerprint`
Expected: PASS (7 tests).

- [ ] **Step 5: Regression.** Run `cargo test -p rocket-app -j4 proposal_service`. Expected: PASS. Then `cargo check --workspace --all-targets -j4` and `cargo clippy -p rocket-app -p rocket-acp -j4 -- -D warnings`.

- [ ] **Step 6: Docs.** In `crates/rocket-app/CLAUDE.md`, extend the `ProposalService` row: `UpdateFolderSettings` replaces the sections a patch sets through `save_folder_settings`; the fingerprint covers headers, variables, scripts, docs and auth, and is checked on the saved copy; a header replaced by a patch loses its description.

- [ ] **Step 7: Commit** via the commit skill. Paths: `crates/rocket-app/src/proposal_service.rs`, `crates/rocket-app/CLAUDE.md`. Message: `feat(assistant): apply folder settings proposals`.
