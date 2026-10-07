# Folder Settings, Plan 01: Domain model and resolution helpers

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `rocket-collection` gains the `FolderSettings` value object, the `ScriptFlow` and `ScriptPhase` enums, the pure helpers `inherited_headers`, `resolve_folder_auth` and `chain_scripts`, a `script_flow` field on `CollectionSettings`, and three defaulted `CollectionRepository` methods. Nothing reads or writes `folder.yml` yet (Plan 02), and nothing runs the helpers yet (Plans 05 and 06).

**Architecture:** A new module `crates/rocket-collection/src/folder_settings.rs` holds the types and the helpers. It is pure: no I/O, no serde on `FolderSettings` (the on-disk shape belongs to `rocket-infra`, the IPC shape to `src-tauri`). `ScriptFlow` lives in the same module and is used by `CollectionSettings.script_flow` with `#[serde(default)]`, so old JSON and old IPC payloads still deserialize. The three new repository methods have default bodies, so the 13 existing `CollectionRepository` impls keep compiling without edits. Two struct literals in `rocket-infra` list every `CollectionSettings` field and get `script_flow: ScriptFlow::default()`; Plan 03 replaces the one in `fs_collection/settings.rs` with a real read from `extensions.bruno.scripts.flow`.

**Tech Stack:** Rust (serde, serde_json for tests). Run tests with `cargo test -j4 -p rocket-collection <filter>`.

**Spec:** `docs/superpowers/specs/2026-10-07-folder-settings-design.md` (Runtime rules section) and the locked contract in `docs/superpowers/plans/folder-settings/00-plan-index.md`. On-disk rules: `docs/superpowers/specs/opencollection-spec-reference.md`.

## Global Constraints

- Names and signatures come from the locked contract in `00-plan-index.md`. Do not rename anything. If a deviation is unavoidable, update the index and every plan that mentions the name.
- `rocket-collection` is a pure domain crate. No filesystem access, no `std::fs`, no `std::process`.
- No panicking `.unwrap` calls in production paths. Tests may use `.expect("reason")`.
- `FolderSettings` gets no serde derives and no `#[serde(rename_all = "camelCase")]`. It is a domain value object, not a persistence struct and not an IPC DTO. `ScriptFlow` is `#[serde(rename_all = "lowercase")]` because `CollectionSettings` (domain JSON doubles as IPC JSON in this codebase) now carries it.
- Header keys match exactly (case-sensitive), the same way `merge_headers` in `crates/rocket-app/src/execution_service.rs:2189` does today. Do not introduce a second, different matching rule.
- Always pass `-j4` to cargo and target one crate with `-p`. Never run `cargo test --workspace` or `--all`.
- Commit with the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only (never `git add -A` or `git add .`) and commit with a pathspec. Peer sessions share this repo's index.

## Review Focus

1. A disabled header at any level must never shadow an enabled header from an outer level, and a disabled header never appears in the output (Task 2 tests `inherited_headers_disabled_folder_header_does_not_shadow` and `inherited_headers_disabled_collection_header_is_dropped`).
2. Inner beats outer for headers, and the replaced header keeps the outer header's position (Task 2 test `inherited_headers_inner_folder_beats_outer_and_collection`).
3. `resolve_folder_auth` skips `None`, `Some(Auth::None)` and `Some(Auth::Inherit)` and returns the innermost real auth (Task 2 tests `resolve_folder_auth_innermost_wins` and `resolve_folder_auth_skips_inherit_none_and_missing`).
4. Sandwich order is outer to inner then request for pre-request, and request then inner to outer for post-response and tests. Sequential is outer to inner then request for every phase (Task 2 tests `chain_scripts_sandwich_*` and `chain_scripts_sequential_*`).
5. Blank and whitespace-only scripts are skipped at every level, including the request's own script (Task 2 test `chain_scripts_skips_blank_scripts`).
6. A `CollectionSettings` JSON without `scriptFlow` still deserializes, defaulting to sandwich (Task 1 test `script_flow_defaults_to_sandwich_when_absent_from_json`).
7. A repository impl that implements only the required methods still compiles and gets the documented defaults (Task 3 tests `minimal_impl_gets_folder_settings_defaults` and `default_get_folder_chain_settings_is_empty`).

---

## Task 1: `FolderSettings`, `ScriptFlow`, `ScriptPhase` and `CollectionSettings.script_flow`

**Files:**
- Create: `crates/rocket-collection/src/folder_settings.rs`
- Modify: `crates/rocket-collection/src/lib.rs` (module list lines 1-14, re-exports lines 17-35)
- Modify: `crates/rocket-collection/src/settings.rs` (imports lines 1-4, struct lines 33-61, tests lines 162-175)
- Modify: `crates/rocket-infra/src/fs_collection/settings.rs` (import line 4, struct literal lines 141-158)
- Modify: `crates/rocket-infra/src/conversions/folder.rs` (imports lines 1-5, struct literal lines 139-155)
- Test: `crates/rocket-collection/src/folder_settings.rs` (in-module `tests`), `crates/rocket-collection/src/settings.rs` (in-module `tests`)

**Interfaces:**
- Consumes: `rocket_shared::types::{Auth, Header}`, `crate::settings::CollectionVariable`.
- Produces:
  - `pub struct FolderSettings { pub headers: Vec<Header>, pub auth: Option<Auth>, pub variables: Vec<CollectionVariable>, pub pre_request_script: Option<String>, pub post_response_script: Option<String>, pub tests_script: Option<String>, pub docs: Option<String> }`, derives `Debug, Clone, PartialEq, Default`.
  - `pub enum ScriptFlow { Sandwich, Sequential }`, derives `Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize`, `#[serde(rename_all = "lowercase")]`, default `Sandwich`.
  - `pub enum ScriptPhase { PreRequest, PostResponse, Tests }`, derives `Debug, Clone, Copy, PartialEq, Eq`.
  - `CollectionSettings.script_flow: ScriptFlow` with `#[serde(default)]` (JSON key `scriptFlow`).
  - Crate-root re-exports `rocket_collection::{FolderSettings, ScriptFlow, ScriptPhase}`.

- [ ] **Step 1: Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

This task adds a `CollectionSettings` field and touches two `rocket-infra` collection conversion sites. Read the folder and `RequestDefaults` sections so the field names line up with Plan 02 and Plan 03.

- [ ] **Step 2: Write the failing tests**

Create `crates/rocket-collection/src/folder_settings.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_settings_default_is_empty() {
        let settings = FolderSettings::default();
        assert!(settings.headers.is_empty());
        assert_eq!(settings.auth, None);
        assert!(settings.variables.is_empty());
        assert_eq!(settings.pre_request_script, None);
        assert_eq!(settings.post_response_script, None);
        assert_eq!(settings.tests_script, None);
        assert_eq!(settings.docs, None);
    }

    #[test]
    fn script_flow_defaults_to_sandwich() {
        assert_eq!(ScriptFlow::default(), ScriptFlow::Sandwich);
    }

    #[test]
    fn script_flow_serializes_lowercase() {
        let json = |f: ScriptFlow| serde_json::to_string(&f).expect("serialize");
        assert_eq!(json(ScriptFlow::Sandwich), "\"sandwich\"");
        assert_eq!(json(ScriptFlow::Sequential), "\"sequential\"");
        let parsed: ScriptFlow = serde_json::from_str("\"sequential\"").expect("deserialize");
        assert_eq!(parsed, ScriptFlow::Sequential);
    }

    #[test]
    fn script_flow_rejects_unknown_value() {
        assert!(serde_json::from_str::<ScriptFlow>("\"bogus\"").is_err());
    }

    #[test]
    fn script_phase_is_copy_and_comparable() {
        let phase = ScriptPhase::Tests;
        let copy = phase;
        assert_eq!(phase, copy);
        assert_ne!(ScriptPhase::PreRequest, ScriptPhase::PostResponse);
    }
}
```

In `crates/rocket-collection/src/lib.rs`, add the module after `pub mod folder;` (line 3):

```rust
pub mod folder;
pub mod folder_settings;
pub mod graphql_request;
```

In `crates/rocket-collection/src/settings.rs`, append these two tests inside the existing `mod tests`, after `sandbox_mode_developer_roundtrips_as_camel_case` (before the closing `}` on line 176):

```rust
    #[test]
    fn script_flow_defaults_to_sandwich_when_absent_from_json() {
        let json = r#"{"headers":[],"variables":[]}"#;
        let settings: CollectionSettings = serde_json::from_str(json).expect("deserialize");
        assert_eq!(settings.script_flow, ScriptFlow::Sandwich);
    }

    #[test]
    fn script_flow_sequential_roundtrips_as_camel_case() {
        let settings = CollectionSettings {
            script_flow: ScriptFlow::Sequential,
            ..Default::default()
        };
        let json = serde_json::to_string(&settings).expect("serialize");
        assert!(
            json.contains(r#""scriptFlow":"sequential""#),
            "expected camelCase scriptFlow field, got {json}"
        );
        let round: CollectionSettings = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(round.script_flow, ScriptFlow::Sequential);
    }
```

- [ ] **Step 3: Run the tests and watch them fail**

Run: `cargo test -j4 -p rocket-collection script_flow`

Expected: FAIL to compile (`cannot find type FolderSettings`, `ScriptFlow`, `ScriptPhase` in this scope; `no field script_flow on type CollectionSettings`).

- [ ] **Step 4: Add the types**

Put this above the test module in `crates/rocket-collection/src/folder_settings.rs`:

```rust
//! Folder-level settings stored in `folder.yml`, and the pure helpers that
//! apply a folder chain to one request.

use rocket_shared::types::{Auth, Header};
use serde::{Deserialize, Serialize};

use crate::settings::CollectionVariable;

/// Settings one folder applies to every request below it.
///
/// This is a domain value object. It has no serde derives, because the on-disk
/// shape belongs to `rocket-infra` and the IPC shape belongs to `src-tauri`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FolderSettings {
    /// Default headers for every request below this folder.
    pub headers: Vec<Header>,
    /// Folder auth. `None` and `Some(Auth::Inherit)` both mean "no folder auth".
    pub auth: Option<Auth>,
    /// Folder variables. They apply before the request runs only.
    pub variables: Vec<CollectionVariable>,
    /// Script of OpenCollection type `before-request`.
    pub pre_request_script: Option<String>,
    /// Script of OpenCollection type `after-response`.
    pub post_response_script: Option<String>,
    /// Script of OpenCollection type `tests`.
    pub tests_script: Option<String>,
    /// Markdown docs content.
    pub docs: Option<String>,
}

/// Order in which collection, folder and request scripts run.
/// Persisted at `extensions.bruno.scripts.flow` in `opencollection.yml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScriptFlow {
    /// Pre-request runs outer to inner. Post-response and tests run inner to outer.
    #[default]
    Sandwich,
    /// Every phase runs outer to inner.
    Sequential,
}

/// One script phase of a request run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptPhase {
    PreRequest,
    PostResponse,
    Tests,
}
```

In `crates/rocket-collection/src/settings.rs`, add the import after line 4:

```rust
use rocket_shared::types::{Auth, Header};
use serde::{Deserialize, Serialize};

use crate::folder_settings::ScriptFlow;
```

Add the field at the end of `CollectionSettings`, after `script_context_roots` (line 60):

```rust
    pub script_context_roots: Vec<String>,

    /// Script run order for this collection. Absent means sandwich.
    /// Persisted at `extensions.bruno.scripts.flow` (Plan 03).
    #[serde(default)]
    pub script_flow: ScriptFlow,
}
```

In `crates/rocket-collection/src/lib.rs`, add the re-export after the `folder` re-export (line 19):

```rust
pub use folder::{CollectionItem, Folder, OpaqueProtocolItem};
pub use folder_settings::{FolderSettings, ScriptFlow, ScriptPhase};
```

- [ ] **Step 5: Run the crate tests and watch them pass**

Run: `cargo test -j4 -p rocket-collection script_flow`

Expected: PASS (`script_flow_defaults_to_sandwich`, `script_flow_serializes_lowercase`, `script_flow_rejects_unknown_value`, `script_flow_defaults_to_sandwich_when_absent_from_json`, `script_flow_sequential_roundtrips_as_camel_case`).

Run: `cargo test -j4 -p rocket-collection`

Expected: PASS, every older test included.

- [ ] **Step 6: Watch the downstream crate fail**

Run: `cargo check -j4 -p rocket-infra`

Expected: FAIL with `error[E0063]: missing field 'script_flow' in initializer of 'CollectionSettings'` at `crates/rocket-infra/src/fs_collection/settings.rs:141` and `crates/rocket-infra/src/conversions/folder.rs:139`. These are the only two literals in the workspace that list every field without `..Default::default()`. All other constructions (in `rocket-app`, `rocket-import`, `rocket-infra` tests and `rocket-collection` tests) already use struct update syntax.

- [ ] **Step 7: Fix the two infra literals**

In `crates/rocket-infra/src/fs_collection/settings.rs`, change line 4:

```rust
use rocket_collection::{Collection, CollectionSettings, CollectionVariable, ScriptFlow};
```

and add the field to the literal inside `if let Some(defaults) = oc.request` (after `script_context_roots,` on line 157):

```rust
            sandbox_mode,
            script_context_roots,
            // Plan 03 reads this from `extensions.bruno.scripts.flow`.
            script_flow: ScriptFlow::default(),
        })
```

In `crates/rocket-infra/src/conversions/folder.rs`, add an import after line 4:

```rust
use rocket_collection::settings::{CollectionSettings, CollectionVariable, SandboxMode};
use rocket_collection::ScriptFlow;
```

and add the field to the literal inside `if let Some(defaults) = oc.request` (after `script_context_roots: vec![],` on line 154):

```rust
            sandbox_mode: SandboxMode::Safe,
            script_context_roots: vec![],
            script_flow: ScriptFlow::default(),
        }
```

- [ ] **Step 8: Prove nothing else breaks**

Run each, one at a time:

- `cargo check -j4 -p rocket-infra`
- `cargo check -j4 -p rocket-app`
- `cargo check -j4 -p rocket-import`
- `cargo check -j4 -p rocket`

Expected: all PASS with no new warnings. (`rocket` is the `src-tauri` package. It returns `CollectionSettings` over IPC in `src-tauri/src/commands/collections.rs:345`, so the frontend now receives an extra `scriptFlow` key. The TS type ignores it until Plan 03 or 04 adds it.)

Run: `cargo test -j4 -p rocket-infra settings`

Expected: PASS (the existing settings roundtrip tests in `fs_collection/tests.rs` and `fs_collection/settings.rs` still pass).

- [ ] **Step 9: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only:

```bash
git add crates/rocket-collection/src/folder_settings.rs crates/rocket-collection/src/lib.rs \
  crates/rocket-collection/src/settings.rs \
  crates/rocket-infra/src/fs_collection/settings.rs crates/rocket-infra/src/conversions/folder.rs
```

Commit with the same pathspec (`git commit --only -m "..." -- <the paths above>`). Suggested subject: `feat(collection): add FolderSettings and ScriptFlow types`.

---

## Task 2: Pure resolution helpers `inherited_headers`, `resolve_folder_auth`, `chain_scripts`

**Files:**
- Modify: `crates/rocket-collection/src/folder_settings.rs` (helpers below the `ScriptPhase` enum, tests appended to `mod tests`)
- Modify: `crates/rocket-collection/src/lib.rs` (the `folder_settings` re-export line added in Task 1)
- Test: `crates/rocket-collection/src/folder_settings.rs` (in-module `tests`)

**Interfaces:**
- Consumes: `FolderSettings`, `ScriptFlow`, `ScriptPhase` (Task 1), `rocket_shared::types::{Auth, Header}` (`Header::new(key, value)` builds an enabled header).
- Produces:
  - `pub fn inherited_headers(collection: &[Header], folders: &[FolderSettings]) -> Vec<Header>`
  - `pub fn resolve_folder_auth(folders: &[FolderSettings]) -> Option<Auth>`
  - `pub fn chain_scripts(phase: ScriptPhase, flow: ScriptFlow, folders: &[FolderSettings], request_script: Option<&str>) -> Vec<String>`
  - Crate-root re-exports `rocket_collection::{chain_scripts, inherited_headers, resolve_folder_auth}`.

- [ ] **Step 1: Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

This task defines header, auth and script inheritance. Read the auth (`inherit`) and scripts sections.

- [ ] **Step 2: Write the failing tests**

Append inside `mod tests` in `crates/rocket-collection/src/folder_settings.rs`:

```rust
    fn header(key: &str, value: &str) -> Header {
        Header::new(key, value)
    }

    fn disabled(key: &str, value: &str) -> Header {
        Header {
            enabled: false,
            ..Header::new(key, value)
        }
    }

    fn with_headers(headers: Vec<Header>) -> FolderSettings {
        FolderSettings {
            headers,
            ..Default::default()
        }
    }

    fn with_auth(auth: Option<Auth>) -> FolderSettings {
        FolderSettings {
            auth,
            ..Default::default()
        }
    }

    fn bearer(token: &str) -> Auth {
        Auth::Bearer {
            token: token.to_string(),
        }
    }

    fn with_scripts(name: &str) -> FolderSettings {
        FolderSettings {
            pre_request_script: Some(format!("{name}-pre")),
            post_response_script: Some(format!("{name}-post")),
            tests_script: Some(format!("{name}-tests")),
            ..Default::default()
        }
    }

    fn pairs(headers: &[Header]) -> Vec<(String, String)> {
        headers
            .iter()
            .map(|h| (h.key.clone(), h.value.clone()))
            .collect()
    }

    fn strs(scripts: &[&str]) -> Vec<String> {
        scripts.iter().map(|s| s.to_string()).collect()
    }

    // inherited_headers

    #[test]
    fn inherited_headers_empty_inputs_return_empty() {
        assert!(inherited_headers(&[], &[]).is_empty());
    }

    #[test]
    fn inherited_headers_no_folders_returns_enabled_collection_headers() {
        let collection = vec![header("A", "1"), header("B", "2")];
        assert_eq!(
            pairs(&inherited_headers(&collection, &[])),
            vec![("A".into(), "1".into()), ("B".into(), "2".into())]
        );
    }

    #[test]
    fn inherited_headers_folder_adds_new_key_after_collection() {
        let collection = vec![header("A", "1")];
        let folders = vec![with_headers(vec![header("B", "2")])];
        assert_eq!(
            pairs(&inherited_headers(&collection, &folders)),
            vec![("A".into(), "1".into()), ("B".into(), "2".into())]
        );
    }

    #[test]
    fn inherited_headers_inner_folder_beats_outer_and_collection() {
        let collection = vec![header("X", "collection"), header("Y", "collection")];
        let folders = vec![
            with_headers(vec![header("X", "outer")]),
            with_headers(vec![header("X", "inner")]),
        ];
        // The replaced header keeps the position of the outermost occurrence.
        assert_eq!(
            pairs(&inherited_headers(&collection, &folders)),
            vec![
                ("X".into(), "inner".into()),
                ("Y".into(), "collection".into())
            ]
        );
    }

    #[test]
    fn inherited_headers_disabled_folder_header_does_not_shadow() {
        let collection = vec![header("X", "collection")];
        let folders = vec![
            with_headers(vec![header("X", "outer")]),
            with_headers(vec![disabled("X", "inner")]),
        ];
        assert_eq!(
            pairs(&inherited_headers(&collection, &folders)),
            vec![("X".into(), "outer".into())]
        );
    }

    #[test]
    fn inherited_headers_disabled_collection_header_is_dropped() {
        let collection = vec![disabled("X", "collection"), header("Y", "1")];
        let result = inherited_headers(&collection, &[]);
        assert_eq!(pairs(&result), vec![("Y".into(), "1".into())]);
        assert!(result.iter().all(|h| h.enabled));
    }

    #[test]
    fn inherited_headers_key_match_is_exact_like_merge_headers() {
        let collection = vec![header("X-Token", "a")];
        let folders = vec![with_headers(vec![header("x-token", "b")])];
        assert_eq!(
            pairs(&inherited_headers(&collection, &folders)),
            vec![("X-Token".into(), "a".into()), ("x-token".into(), "b".into())]
        );
    }

    // resolve_folder_auth

    #[test]
    fn resolve_folder_auth_empty_list_is_none() {
        assert_eq!(resolve_folder_auth(&[]), None);
    }

    #[test]
    fn resolve_folder_auth_innermost_wins() {
        let folders = vec![
            with_auth(Some(bearer("outer"))),
            with_auth(Some(bearer("inner"))),
        ];
        assert_eq!(resolve_folder_auth(&folders), Some(bearer("inner")));
    }

    #[test]
    fn resolve_folder_auth_skips_inherit_none_and_missing() {
        let folders = vec![
            with_auth(Some(bearer("outer"))),
            with_auth(Some(Auth::Inherit)),
            with_auth(Some(Auth::None)),
            with_auth(None),
        ];
        assert_eq!(resolve_folder_auth(&folders), Some(bearer("outer")));
    }

    #[test]
    fn resolve_folder_auth_all_inherit_or_none_is_none() {
        let folders = vec![
            with_auth(Some(Auth::Inherit)),
            with_auth(Some(Auth::None)),
            with_auth(None),
        ];
        assert_eq!(resolve_folder_auth(&folders), None);
    }

    #[test]
    fn resolve_folder_auth_keeps_any_auth_type() {
        let basic = Auth::Basic {
            username: "u".into(),
            password: "p".into(),
        };
        let folders = vec![with_auth(Some(basic.clone())), with_auth(None)];
        assert_eq!(resolve_folder_auth(&folders), Some(basic));
    }

    // chain_scripts

    fn two_folders() -> Vec<FolderSettings> {
        vec![with_scripts("outer"), with_scripts("inner")]
    }

    #[test]
    fn chain_scripts_sandwich_pre_request_is_outer_inner_request() {
        let out = chain_scripts(
            ScriptPhase::PreRequest,
            ScriptFlow::Sandwich,
            &two_folders(),
            Some("req"),
        );
        assert_eq!(out, strs(&["outer-pre", "inner-pre", "req"]));
    }

    #[test]
    fn chain_scripts_sandwich_post_response_is_request_inner_outer() {
        let out = chain_scripts(
            ScriptPhase::PostResponse,
            ScriptFlow::Sandwich,
            &two_folders(),
            Some("req"),
        );
        assert_eq!(out, strs(&["req", "inner-post", "outer-post"]));
    }

    #[test]
    fn chain_scripts_sandwich_tests_is_request_inner_outer() {
        let out = chain_scripts(
            ScriptPhase::Tests,
            ScriptFlow::Sandwich,
            &two_folders(),
            Some("req"),
        );
        assert_eq!(out, strs(&["req", "inner-tests", "outer-tests"]));
    }

    #[test]
    fn chain_scripts_sequential_pre_request_is_outer_inner_request() {
        let out = chain_scripts(
            ScriptPhase::PreRequest,
            ScriptFlow::Sequential,
            &two_folders(),
            Some("req"),
        );
        assert_eq!(out, strs(&["outer-pre", "inner-pre", "req"]));
    }

    #[test]
    fn chain_scripts_sequential_post_response_is_outer_inner_request() {
        let out = chain_scripts(
            ScriptPhase::PostResponse,
            ScriptFlow::Sequential,
            &two_folders(),
            Some("req"),
        );
        assert_eq!(out, strs(&["outer-post", "inner-post", "req"]));
    }

    #[test]
    fn chain_scripts_sequential_tests_is_outer_inner_request() {
        let out = chain_scripts(
            ScriptPhase::Tests,
            ScriptFlow::Sequential,
            &two_folders(),
            Some("req"),
        );
        assert_eq!(out, strs(&["outer-tests", "inner-tests", "req"]));
    }

    #[test]
    fn chain_scripts_empty_folder_list_returns_request_only_for_every_phase_and_flow() {
        for flow in [ScriptFlow::Sandwich, ScriptFlow::Sequential] {
            for phase in [ScriptPhase::PreRequest, ScriptPhase::PostResponse, ScriptPhase::Tests] {
                assert_eq!(chain_scripts(phase, flow, &[], Some("req")), strs(&["req"]));
                assert!(chain_scripts(phase, flow, &[], None).is_empty());
            }
        }
    }

    #[test]
    fn chain_scripts_skips_blank_scripts() {
        let folders = vec![
            FolderSettings {
                pre_request_script: Some("   \n\t".into()),
                ..Default::default()
            },
            FolderSettings {
                pre_request_script: Some(String::new()),
                ..Default::default()
            },
            FolderSettings::default(),
            FolderSettings {
                pre_request_script: Some("kept".into()),
                ..Default::default()
            },
        ];
        assert_eq!(
            chain_scripts(ScriptPhase::PreRequest, ScriptFlow::Sandwich, &folders, Some("  ")),
            strs(&["kept"])
        );
        assert_eq!(
            chain_scripts(ScriptPhase::PreRequest, ScriptFlow::Sequential, &folders, Some("")),
            strs(&["kept"])
        );
    }

    #[test]
    fn chain_scripts_only_uses_the_requested_phase() {
        let folders = vec![FolderSettings {
            pre_request_script: Some("pre".into()),
            ..Default::default()
        }];
        assert!(chain_scripts(ScriptPhase::Tests, ScriptFlow::Sandwich, &folders, None).is_empty());
        assert!(
            chain_scripts(ScriptPhase::PostResponse, ScriptFlow::Sequential, &folders, None)
                .is_empty()
        );
    }

    #[test]
    fn chain_scripts_keeps_script_text_unchanged() {
        let folders = vec![FolderSettings {
            tests_script: Some("  test('a');\n".into()),
            ..Default::default()
        }];
        assert_eq!(
            chain_scripts(ScriptPhase::Tests, ScriptFlow::Sandwich, &folders, None),
            strs(&["  test('a');\n"])
        );
    }
```

- [ ] **Step 3: Run the tests and watch them fail**

Run: `cargo test -j4 -p rocket-collection folder_settings`

Expected: FAIL to compile (`cannot find function inherited_headers`, `resolve_folder_auth`, `chain_scripts` in this scope).

- [ ] **Step 4: Implement the helpers**

In `crates/rocket-collection/src/folder_settings.rs`, add below the `ScriptPhase` enum (above `#[cfg(test)]`):

```rust
/// Collection headers, then folder headers outermost first, merged into one list.
///
/// A more specific level replaces a header with the same key and keeps its
/// position. Keys match exactly, like `merge_headers` in `rocket-app`.
/// Disabled headers never shadow an outer header and are left out of the result.
/// The request's own headers are merged afterwards by the caller.
pub fn inherited_headers(collection: &[Header], folders: &[FolderSettings]) -> Vec<Header> {
    let levels =
        std::iter::once(collection).chain(folders.iter().map(|f| f.headers.as_slice()));
    let mut merged: Vec<Header> = Vec::new();
    for level in levels {
        for header in level.iter().filter(|h| h.enabled) {
            match merged.iter_mut().find(|m| m.key == header.key) {
                Some(existing) => *existing = header.clone(),
                None => merged.push(header.clone()),
            }
        }
    }
    merged
}

/// The innermost folder auth that is set and is not `Auth::None` or `Auth::Inherit`.
///
/// `folders` is ordered outermost first. Returns `None` when no folder sets auth,
/// so the caller can fall back to the collection auth.
pub fn resolve_folder_auth(folders: &[FolderSettings]) -> Option<Auth> {
    folders.iter().rev().find_map(|folder| match &folder.auth {
        None | Some(Auth::None) | Some(Auth::Inherit) => None,
        Some(auth) => Some(auth.clone()),
    })
}

/// Scripts for one phase in run order, including the request's own script.
///
/// `folders` is ordered outermost first. Sandwich runs pre-request scripts outer
/// to inner and then the request, and runs post-response and tests scripts from
/// the request out to the outermost folder. Sequential runs every phase outer to
/// inner and then the request. Blank scripts are skipped. Script text is kept as is.
pub fn chain_scripts(
    phase: ScriptPhase,
    flow: ScriptFlow,
    folders: &[FolderSettings],
    request_script: Option<&str>,
) -> Vec<String> {
    let folder_scripts: Vec<String> = folders
        .iter()
        .filter_map(|folder| non_blank(phase_script(folder, phase)))
        .collect();
    let request = non_blank(request_script);
    let request_first = flow == ScriptFlow::Sandwich && phase != ScriptPhase::PreRequest;

    let mut ordered = Vec::with_capacity(folder_scripts.len() + 1);
    if request_first {
        ordered.extend(request);
        ordered.extend(folder_scripts.into_iter().rev());
    } else {
        ordered.extend(folder_scripts);
        ordered.extend(request);
    }
    ordered
}

/// The folder's script for one phase.
fn phase_script(settings: &FolderSettings, phase: ScriptPhase) -> Option<&str> {
    match phase {
        ScriptPhase::PreRequest => settings.pre_request_script.as_deref(),
        ScriptPhase::PostResponse => settings.post_response_script.as_deref(),
        ScriptPhase::Tests => settings.tests_script.as_deref(),
    }
}

/// An owned copy of the script, or `None` when it is missing or only whitespace.
fn non_blank(script: Option<&str>) -> Option<String> {
    script
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
}
```

In `crates/rocket-collection/src/lib.rs`, widen the re-export added in Task 1:

```rust
pub use folder_settings::{
    chain_scripts, inherited_headers, resolve_folder_auth, FolderSettings, ScriptFlow,
    ScriptPhase,
};
```

- [ ] **Step 5: Run the tests and watch them pass**

Run: `cargo test -j4 -p rocket-collection folder_settings`

Expected: PASS, 26 tests in `folder_settings::tests` (5 from Task 1, 21 new).

Run: `cargo clippy -j4 -p rocket-collection -- -D warnings`

Expected: PASS with no warnings from `folder_settings.rs`. If clippy flags a warning in an older file, leave it alone and note it in the commit body.

- [ ] **Step 6: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only:

```bash
git add crates/rocket-collection/src/folder_settings.rs crates/rocket-collection/src/lib.rs
```

Commit with the same pathspec (`git commit --only -m "..." -- crates/rocket-collection/src/folder_settings.rs crates/rocket-collection/src/lib.rs`). Suggested subject: `feat(collection): add folder header, auth and script chain helpers`.

---

## Task 3: Defaulted `CollectionRepository` folder settings methods

**Files:**
- Modify: `crates/rocket-collection/src/repository.rs` (imports lines 3-12, new methods after `save_folder_variables` at line 230, tests lines 248-257)
- Modify: `crates/rocket-collection/CLAUDE.md` (add a short `FolderSettings` section after the `CollectionSettings` section)
- Test: `crates/rocket-collection/src/repository.rs` (in-module `tests`)

**Interfaces:**
- Consumes: `FolderSettings` (Task 1), `DomainError::Internal(String)` and `DomainResult<T>` from `rocket_shared::error`.
- Produces (on `trait CollectionRepository`, all with default bodies):
  - `fn get_folder_settings(&self, collection: &str, folder_path: &str) -> DomainResult<FolderSettings>`, default `Err(DomainError::Internal("folder settings not supported".into()))`.
  - `fn save_folder_settings(&self, collection: &str, folder_path: &str, settings: &FolderSettings) -> DomainResult<()>`, same default error.
  - `fn get_folder_chain_settings(&self, collection: &str, request_path: &str) -> DomainResult<Vec<FolderSettings>>`, default `Ok(vec![])`. Outermost folder first.

- [ ] **Step 1: Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

These methods are the repository seam for `folder.yml`. Read the folder section so the doc comments match what Plan 02 implements.

- [ ] **Step 2: Write the failing tests**

Replace the whole `#[cfg(test)] mod tests` block at the end of `crates/rocket-collection/src/repository.rs` (lines 248-257) with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Implements only the required methods, like the older test doubles in
    /// other crates. If a new method had no default, this would not compile.
    struct MinimalRepo;

    fn unused<T>() -> DomainResult<T> {
        Err(DomainError::Internal("unused in this test".into()))
    }

    impl CollectionRepository for MinimalRepo {
        fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
            unused()
        }
        fn get(&self, _name: &str) -> DomainResult<Collection> {
            unused()
        }
        fn get_summaries(&self, _name: &str) -> DomainResult<Collection> {
            unused()
        }
        fn create(&self, _name: &str) -> DomainResult<Collection> {
            unused()
        }
        fn delete(&self, _name: &str) -> DomainResult<()> {
            unused()
        }
        fn rename(&self, _old_name: &str, _new_name: &str) -> DomainResult<()> {
            unused()
        }
        fn get_request(&self, _collection: &str, _path: &str) -> DomainResult<Request> {
            unused()
        }
        fn save_request(
            &self,
            _collection: &str,
            _path: &str,
            _request: &Request,
        ) -> DomainResult<String> {
            unused()
        }
        fn rename_request(
            &self,
            _collection: &str,
            _old_path: &str,
            _new_path: &str,
        ) -> DomainResult<()> {
            unused()
        }
        fn delete_request(&self, _collection: &str, _path: &str) -> DomainResult<()> {
            unused()
        }
        fn create_folder(&self, _collection: &str, _path: &str) -> DomainResult<()> {
            unused()
        }
        fn delete_folder(&self, _collection: &str, _path: &str) -> DomainResult<()> {
            unused()
        }
        fn move_item(
            &self,
            _src_collection: &str,
            _src_path: &str,
            _dst_collection: &str,
            _dst_path: &str,
        ) -> DomainResult<()> {
            unused()
        }
        fn reorder_items(
            &self,
            _collection: &str,
            _folder_path: &str,
            _ordered_names: &[String],
        ) -> DomainResult<()> {
            unused()
        }
        fn get_settings(&self, _name: &str) -> DomainResult<CollectionSettings> {
            unused()
        }
        fn save_settings(&self, _name: &str, _settings: &CollectionSettings) -> DomainResult<()> {
            unused()
        }
        fn get_folder_chain_variables(
            &self,
            _collection: &str,
            _request_path: &str,
        ) -> DomainResult<Vec<CollectionVariable>> {
            unused()
        }
        fn get_folder_variables(
            &self,
            _collection: &str,
            _folder_path: &str,
        ) -> DomainResult<Vec<CollectionVariable>> {
            unused()
        }
        fn save_folder_variables(
            &self,
            _collection: &str,
            _folder_path: &str,
            _vars: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            unused()
        }
        fn get_request_variables(
            &self,
            _collection: &str,
            _request_path: &str,
        ) -> DomainResult<Vec<CollectionVariable>> {
            unused()
        }
        fn save_request_variables(
            &self,
            _collection: &str,
            _request_path: &str,
            _vars: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            unused()
        }
    }

    #[test]
    fn trait_is_object_safe() {
        // Compile-time check.
        fn _assert_object_safe(_: Box<dyn CollectionRepository>) {}
        let _boxed: Box<dyn CollectionRepository> = Box::new(MinimalRepo);
    }

    #[test]
    fn minimal_impl_gets_folder_settings_defaults() {
        let repo: Box<dyn CollectionRepository> = Box::new(MinimalRepo);

        let read = repo.get_folder_settings("c", "a/b");
        assert!(
            matches!(&read, Err(DomainError::Internal(msg)) if msg == "folder settings not supported"),
            "unexpected get_folder_settings default: {read:?}"
        );

        let saved = repo.save_folder_settings("c", "", &FolderSettings::default());
        assert!(
            matches!(&saved, Err(DomainError::Internal(msg)) if msg == "folder settings not supported"),
            "unexpected save_folder_settings default: {saved:?}"
        );
    }

    #[test]
    fn default_get_folder_chain_settings_is_empty() {
        let repo = MinimalRepo;
        let chain = repo
            .get_folder_chain_settings("c", "a/b/request.yml")
            .expect("default chain is Ok");
        assert_eq!(chain, Vec::<FolderSettings>::new());
    }
}
```

- [ ] **Step 3: Run the tests and watch them fail**

Run: `cargo test -j4 -p rocket-collection repository::tests`

Expected: FAIL to compile (`no method named get_folder_settings found`, same for `save_folder_settings` and `get_folder_chain_settings`; `cannot find type FolderSettings in this scope`).

- [ ] **Step 4: Add the defaulted methods**

In `crates/rocket-collection/src/repository.rs`, add the import after line 5 (`use crate::collection::Collection;`):

```rust
use crate::collection::Collection;
use crate::folder_settings::FolderSettings;
```

Insert after `save_folder_variables` (after line 230, before the `get_request_variables` doc comment):

```rust
    /// Read one folder's own settings from its folder.yml (no chain walk).
    /// `folder_path` is relative to the collection root, `""` for the root.
    /// The default body keeps test doubles compiling; real repositories override it.
    fn get_folder_settings(
        &self,
        _collection: &str,
        _folder_path: &str,
    ) -> DomainResult<FolderSettings> {
        Err(DomainError::Internal(
            "folder settings not supported".into(),
        ))
    }

    /// Persist one folder's settings to its folder.yml.
    /// Keys the domain does not model (for example `request.metadata`) are kept by the implementation.
    fn save_folder_settings(
        &self,
        _collection: &str,
        _folder_path: &str,
        _settings: &FolderSettings,
    ) -> DomainResult<()> {
        Err(DomainError::Internal(
            "folder settings not supported".into(),
        ))
    }

    /// Settings of every folder above a request, outermost folder first.
    /// The default returns no folders, so repositories without folder.yml
    /// support run requests with collection settings only.
    fn get_folder_chain_settings(
        &self,
        _collection: &str,
        _request_path: &str,
    ) -> DomainResult<Vec<FolderSettings>> {
        Ok(vec![])
    }
```

In `crates/rocket-collection/CLAUDE.md`, add this section after the `CollectionSettings` section (before `## CollectionRepository`):

```markdown
## FolderSettings

`folder_settings.rs` holds `FolderSettings` (headers, auth, variables, three phase scripts, docs), `ScriptFlow` (`sandwich` default, `sequential`) and `ScriptPhase`. It has no serde derives: `rocket-infra` owns the `folder.yml` shape and `src-tauri` owns the IPC DTO.

- `inherited_headers` merges collection then folder headers (outermost first). Inner replaces outer by exact key. Disabled headers never shadow and are dropped.
- `resolve_folder_auth` returns the innermost folder auth that is not `None` or `Inherit`.
- `chain_scripts` orders one phase's scripts for a `ScriptFlow`, request script included, blanks skipped.
- `CollectionRepository::{get_folder_settings, save_folder_settings, get_folder_chain_settings}` have defaults so test doubles keep compiling.
```

- [ ] **Step 5: Run the tests and watch them pass**

Run: `cargo test -j4 -p rocket-collection repository::tests`

Expected: PASS (`trait_is_object_safe`, `minimal_impl_gets_folder_settings_defaults`, `default_get_folder_chain_settings_is_empty`).

Run: `cargo test -j4 -p rocket-collection`

Expected: PASS, every test in the crate.

- [ ] **Step 6: Prove the existing impls still compile**

The 13 existing `CollectionRepository` impls (`FsCollectionRepo`, `SharedPathCollectionRepo` and the test doubles in `rocket-app` and `src-tauri`) must not need edits. Run each, one at a time:

- `cargo check -j4 -p rocket-infra --tests`
- `cargo check -j4 -p rocket-app --tests`
- `cargo check -j4 -p rocket`

Expected: all PASS with no new warnings.

- [ ] **Step 7: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only:

```bash
git add crates/rocket-collection/src/repository.rs crates/rocket-collection/CLAUDE.md
```

Commit with the same pathspec (`git commit --only -m "..." -- crates/rocket-collection/src/repository.rs crates/rocket-collection/CLAUDE.md`). Suggested subject: `feat(collection): add defaulted folder settings repository methods`.

---

## Next Plan

[Plan 02: folder.yml persistence](2026-10-07-folder-settings-plan-02-folder-yml-persistence.md). It depends on this plan (`FolderSettings`, the three defaulted `CollectionRepository` methods) and implements them in `rocket-infra` (`fs_collection/folder_file.rs`, `fs_collection/variables.rs`, `fs_collection/folders.rs`). Chain to it automatically when this one finishes.
