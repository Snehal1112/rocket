# Folder Settings, Plan 06: Runtime script and test chain

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** every send of a request runs the pre-request, post-response and tests scripts of the folders above it, in the order `chain_scripts` gives for the collection's `script_flow`, with the same sandbox mode, `require()` scope, vault hold-back and secret-use scan as the request's own scripts. A folder script error names its folder.

**Architecture:** `begin_phases` reads the folder chain once (`get_folder_chain_settings`) and builds a `PhaseScripts` value (three ordered lists of `ChainedScript { source, code }`) that it stores on `PhaseState`. The three phase methods (`run_before_request_phase`, `run_after_response_phase`, `run_tests_phase`) loop over their list instead of reading `input.*_script` directly. Because the single send (`execute`, `execute_capturing`), the Collection Runner, Flow (saved and inline nodes, and Flow polls) and GraphQL all drive these same phase methods, every caller gets the chain through one place. Ordering is never re-implemented: `PhaseScripts::assemble` asks `rocket_collection::chain_scripts` for the order by passing index markers in place of the script text, then maps each marker back to its script and its folder path. `references_alias` (the check that refuses a send when a failing RocketVault binding is used) also scans folder scripts, headers and auth.

**Tech Stack:** Rust (tokio tests, async-trait), crate `rocket-app` only. Run Rust tests with `cargo test -j4 -p rocket-app <name>`.

**Spec:** [2026-10-07-folder-settings-design.md](../../specs/2026-10-07-folder-settings-design.md), sections "Runtime rules" and "Error handling". Locked names: [00-plan-index.md](00-plan-index.md).

**Depends on:**
- Plan 01: `rocket_collection::{FolderSettings, ScriptFlow, ScriptPhase, chain_scripts}` re-exported at the crate root. `ScriptFlow` and `ScriptPhase` derive `Copy` (plan 01 Interfaces). `CollectionSettings.script_flow: ScriptFlow`.
- Plan 02: `FsCollectionRepo::get_folder_chain_settings` returns exactly one entry per ancestor folder, outermost first, `FolderSettings::default()` for a folder without `folder.yml`, and an error naming the folder for a `folder.yml` that does not parse.
- Plan 03: `script_flow` is read from `extensions.bruno.scripts.flow` by `get_settings`.
- Plan 05: runtime header and auth inheritance in `execution_service.rs`. See "Coordination with Plan 05" below.

Check before Task 1:

```bash
grep -n "pub use folder_settings" crates/rocket-collection/src/lib.rs
grep -n "derive(Debug, Clone, Copy, PartialEq, Eq" crates/rocket-collection/src/folder_settings.rs
grep -n "pub script_flow" crates/rocket-collection/src/settings.rs
grep -n "fn get_folder_chain_settings" crates/rocket-infra/src/fs_collection/folder_settings.rs
```

Expected: the re-export line lists `chain_scripts`, `FolderSettings`, `ScriptFlow` and `ScriptPhase`; two derive lines with `Copy` (on `ScriptFlow` and `ScriptPhase`); one `script_flow` field; one chain function. Stop if any is missing and finish the plan that owns it first.

## Decisions recorded by this plan

- **One engine run per script, not one concatenated script.** The engine (`ScriptEngine::execute(ScriptContext)`) runs one `code` string per context and returns one `ScriptResult`, which `rocket-app` applies afterwards. Running each script separately keeps four things that concatenation would break: (1) two scripts that both declare `const token` would be a `SyntaxError`; (2) an uncaught throw in one script would silently drop every script after it in the same source text, and the error could not be attributed to a folder; (3) a `require()` from a folder script resolves exactly like one from the request script, because both get the same `file_scope`; (4) `ScriptError` events and `script_error` can name the folder. The scripts share state the way the phases already do: each run gets a fresh `ScriptContext` built from the current `state.var_ctx` and `state.http_request`, so a request mutation (`req.setHeader`, `req.setUrl`, ...) and a runtime variable (`rok.setVar`) from an earlier script are visible to every later script, in the same phase and in later phases. Environment, global and collection writes are persisted after each script by `apply_script_side_effects`, exactly as today. They are not copied back into `state.var_ctx` (that is today's behaviour between phases as well, and is unchanged).
- **Errors.** The first error in phase order is the one kept in `script_error`. A failed script ends its phase: later scripts of that phase do not run, because a later script normally depends on what the failed one set up. The declarative actions of that phase, the send and the later phases still run, exactly as a request script error does today. A request script's error text is unchanged. A folder script's error reads `Folder "<folder path>" <phase> script: <message>`, in `script_error` and in the `ScriptError` event.
- **`rok.runner.skipRequest()` in a folder script.** It sets `state.skip_request`, as today. In a Collection Runner step (`ExecutionMode::Runner`) it also ends the pre-request chain, because the request will not be sent. In a single send (`ExecutionMode::Standalone`) skip stays a no-op, as `execute_ignores_skip_request_and_still_sends` already requires, so the chain continues.
- **`setMethod` warning.** Within one script, a thrown error still replaces that script's own invalid-`setMethod` warning, as today. Across scripts, the first recorded error wins.
- **Request-guard (SSRF) check.** Each script's `req.setUrl()` is checked against the URL before that script, so a folder script's redirect is checked when it runs. A blocked redirect ends the phase with `Err`, as today.
- **Callers that run no folder scripts.** An inline Flow node (request path `__flow_inline__/<node id>`, built in `flow_execution_service.rs`) is not part of the collection tree, so it inherits no folder chain. GraphQL schema introspection (`graphql_schema::introspection_input`) already drops the request's scripts, assertions and actions. It must drop folder scripts as well, otherwise a folder post-response script could write variables from an introspection response. A new IPC-compatible field `ExecuteRequestInput.skip_folder_scripts: bool` (`#[serde(default)]`, so the frontend needs no change) does this. The load test runs no scripts at all (`run_load_test` only calls `resolve_request`), so nothing changes there.
- **Frontend send path.** `src/lib/execute-request.ts` already sends `collection`, `requestPath` and the request's own three scripts. The backend loads the folder scripts, so the frontend needs no change.
- **Test result labels.** `rocket_scripting::TestResult` is `{ name, status, error }` with no source field, and the IPC `IpcTestResult` and the `TestsCompleted` event mirror it. Labelling which folder a `rok.test()` result came from is out of scope. Console entries are not labelled either.
- **Collection-level scripts.** The spec's runtime table puts the collection above all folders, but `CollectionSettings` has no script fields today, and `chain_scripts` takes none. The chain is folders plus request. Collection scripts are out of scope.

## Coordination with Plan 05

Plan 05 also reads the folder chain in `execution_service.rs` (headers and auth). The index requires one `get_folder_chain_settings` call per execution. Before Task 2, run:

```bash
grep -n "get_folder_chain_settings" crates/rocket-app/src/execution_service.rs crates/rocket-app/src/execution_service/*.rs
```

- No match: add `folder_chain` as written in Task 2.
- A match inside a helper method that takes `&ExecuteRequestInput` and returns `DomainResult<Vec<FolderSettings>>`: do not add `folder_chain`. Use that method everywhere this plan calls `self.folder_chain(input)`, and add the `__flow_inline__/` early return from Task 2 to it if it lacks one (Task 2 test `inline_flow_requests_and_introspection_run_no_folder_scripts` checks it).
- A match where `begin_phases` (or `resolve_request`, which `begin_phases` calls) already holds the chain in a local: reuse that local for `PhaseScripts::assemble` in `begin_phases` instead of a second read.

`references_alias` reads the chain a second time, but only when a RocketVault binding failed to resolve, which is rare. A normal send still reads it once.

## Global Constraints

- Never call `unwrap()` in production paths. Tests may use `.expect("reason")`.
- Always pass `-j4` to cargo and target one crate. Never run `cargo test --workspace`.
- No `#[serde(rename_all = "camelCase")]` change. `ExecuteRequestInput` is an IPC DTO that already has it, and the new field is `#[serde(default)]`.
- Do not add phase logic to only one caller (`crates/rocket-app/CLAUDE.md`, "Phase-callable execution"). All chain logic lives in `begin_phases` and the three phase methods.
- Request script behaviour must not change: existing `execution_service`, `collection_runner_service`, `flow_*` and `graphql_*` tests pass unchanged.
- Commit with the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only (never `git add -A` or `git add .`), then commit with the same pathspec. Peer sessions share this repo's index.
- Code comments are short full sentences ending with a punctuation mark.

## Review Focus

1. Sandwich order: pre-request runs outer folder, inner folder, request; post-response and tests run request, inner folder, outer folder (Task 1 test `sandwich_wraps_the_request_in_its_folders`, Task 2 test `sandwich_flow_runs_folder_and_request_scripts_in_spec_order`).
2. Sequential order: every phase runs outer folder, inner folder, request (Task 1 test `sequential_runs_folders_first_in_every_phase`, Task 2 test `sequential_flow_runs_folders_first_in_every_phase`).
3. Two folders with identical script text keep separate entries and their own folder names, which is why ordering goes through index markers (Task 1 test `identical_scripts_in_two_folders_keep_their_own_labels`).
4. A later script sees the request mutations and runtime variables an earlier script left, in the same phase and in later phases (Task 2 test `a_later_script_sees_the_request_and_variables_an_earlier_one_left`).
5. A folder script error is reported as `Folder "<path>" <phase> script: <message>` in `script_error` and in the `ScriptError` event, ends its phase, keeps the first error over later ones, and does not stop the send or the later phases (Task 2 test `a_folder_script_error_names_the_folder_and_ends_its_phase`). A request script's error text is unchanged (Task 1 test `errors_name_the_folder_and_leave_request_errors_as_they_were`, plus the existing execution_service tests).
6. `skipRequest()` from a folder script ends the pre-request chain in a run and is ignored by a single send (Task 2 tests `skip_request_from_a_folder_script_ends_the_chain_in_a_run` and `skip_request_from_a_folder_script_is_ignored_by_a_single_send`).
7. A folder with no scripts, and a request with no scripts at all, behave exactly as before (Task 1 test `blank_and_missing_scripts_are_skipped`, Task 2 test `a_folder_without_scripts_changes_nothing`).
8. Folder scripts run with the collection's sandbox mode and `require()` file scope, the same values the request script gets (Task 2 test `folder_scripts_run_with_the_request_scripts_sandbox_and_file_scope`).
9. Inline Flow requests and GraphQL introspection run no folder scripts (Task 2 test `inline_flow_requests_and_introspection_run_no_folder_scripts`, updated `introspection_input_drops_scripts_assertions_actions_and_history`).
10. A folder script that reads a failing RocketVault secret blocks the send exactly as a request script would; a commented-out read does not; a folder header naming the secret blocks it too (Task 3 tests `a_folder_script_reading_a_failing_secret_blocks_the_send`, `a_commented_out_folder_secret_read_does_not_block_the_send`, `a_folder_header_naming_a_failing_secret_blocks_the_send`).

---

## Task 1: Assemble the per-phase script chain with folder labels

**Files:**
- Create: `crates/rocket-app/src/execution_service/script_chain.rs`
- Modify: `crates/rocket-app/src/execution_service.rs` (module declaration after `pub mod websocket_resolution;`, line 27)
- Test: `crates/rocket-app/src/execution_service/script_chain.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes (plan 01): `rocket_collection::{chain_scripts, FolderSettings, ScriptFlow, ScriptPhase}`, with `pub fn chain_scripts(phase: ScriptPhase, flow: ScriptFlow, folders: &[FolderSettings], request_script: Option<&str>) -> Vec<String>`.
- Produces (crate-internal, in `crate::execution_service::script_chain`):
  - `pub(crate) enum ScriptSource { Folder(String), Request }`
  - `pub(crate) struct ChainedScript { pub source: ScriptSource, pub code: String }`
  - `impl ChainedScript { pub(crate) fn attribute(&self, phase: &str, message: &str) -> String }`
  - `pub(crate) struct PhaseScripts { pub pre_request: Vec<ChainedScript>, pub post_response: Vec<ChainedScript>, pub tests: Vec<ChainedScript> }` with `Default`
  - `impl PhaseScripts { pub(crate) fn assemble(folders: &[FolderSettings], labels: &[String], flow: ScriptFlow, pre_request: Option<&str>, post_response: Option<&str>, tests: Option<&str>) -> Self }`
  - `pub(crate) fn folder_labels(request_path: &str, count: usize) -> Vec<String>`

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.** (Section 2.7 Folder and the `Script` sub-type: `before-request`, `after-response`, `tests`.)

- [ ] **Step 2: Declare the module and write the failing tests**

In `crates/rocket-app/src/execution_service.rs`, after line 27 (`pub mod websocket_resolution;`), add:

```rust
pub(crate) mod script_chain;
```

Create `crates/rocket-app/src/execution_service/script_chain.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn folder(pre: &str, post: &str, tests: &str) -> FolderSettings {
        FolderSettings {
            pre_request_script: Some(pre.to_string()),
            post_response_script: Some(post.to_string()),
            tests_script: Some(tests.to_string()),
            ..FolderSettings::default()
        }
    }

    fn from_folder(label: &str, code: &str) -> ChainedScript {
        ChainedScript {
            source: ScriptSource::Folder(label.to_string()),
            code: code.to_string(),
        }
    }

    fn from_request(code: &str) -> ChainedScript {
        ChainedScript {
            source: ScriptSource::Request,
            code: code.to_string(),
        }
    }

    fn labels() -> Vec<String> {
        vec!["api".to_string(), "api/users".to_string()]
    }

    fn two_folders() -> Vec<FolderSettings> {
        vec![
            folder("o-pre", "o-post", "o-test"),
            folder("i-pre", "i-post", "i-test"),
        ]
    }

    #[test]
    fn sandwich_wraps_the_request_in_its_folders() {
        let scripts = PhaseScripts::assemble(
            &two_folders(),
            &labels(),
            ScriptFlow::Sandwich,
            Some("r-pre"),
            Some("r-post"),
            Some("r-test"),
        );
        assert_eq!(
            scripts.pre_request,
            vec![
                from_folder("api", "o-pre"),
                from_folder("api/users", "i-pre"),
                from_request("r-pre"),
            ]
        );
        assert_eq!(
            scripts.post_response,
            vec![
                from_request("r-post"),
                from_folder("api/users", "i-post"),
                from_folder("api", "o-post"),
            ]
        );
        assert_eq!(
            scripts.tests,
            vec![
                from_request("r-test"),
                from_folder("api/users", "i-test"),
                from_folder("api", "o-test"),
            ]
        );
    }

    #[test]
    fn sequential_runs_folders_first_in_every_phase() {
        let scripts = PhaseScripts::assemble(
            &two_folders(),
            &labels(),
            ScriptFlow::Sequential,
            Some("r-pre"),
            Some("r-post"),
            Some("r-test"),
        );
        assert_eq!(
            scripts.pre_request,
            vec![
                from_folder("api", "o-pre"),
                from_folder("api/users", "i-pre"),
                from_request("r-pre"),
            ]
        );
        assert_eq!(
            scripts.post_response,
            vec![
                from_folder("api", "o-post"),
                from_folder("api/users", "i-post"),
                from_request("r-post"),
            ]
        );
        assert_eq!(
            scripts.tests,
            vec![
                from_folder("api", "o-test"),
                from_folder("api/users", "i-test"),
                from_request("r-test"),
            ]
        );
    }

    #[test]
    fn blank_and_missing_scripts_are_skipped() {
        let folders = vec![
            FolderSettings::default(),
            FolderSettings {
                pre_request_script: Some("  \n".to_string()),
                tests_script: Some("i-test".to_string()),
                ..FolderSettings::default()
            },
        ];
        let scripts = PhaseScripts::assemble(
            &folders,
            &labels(),
            ScriptFlow::Sandwich,
            Some(" "),
            None,
            Some("r-test"),
        );
        assert!(scripts.pre_request.is_empty());
        assert!(scripts.post_response.is_empty());
        assert_eq!(
            scripts.tests,
            vec![from_request("r-test"), from_folder("api/users", "i-test")]
        );
    }

    #[test]
    fn no_folders_gives_only_the_request_scripts() {
        let scripts =
            PhaseScripts::assemble(&[], &[], ScriptFlow::Sandwich, Some("r-pre"), None, None);
        assert_eq!(scripts.pre_request, vec![from_request("r-pre")]);
        assert!(scripts.post_response.is_empty());
        assert!(scripts.tests.is_empty());
        assert_eq!(
            PhaseScripts::assemble(&[], &[], ScriptFlow::Sequential, None, None, None),
            PhaseScripts::default()
        );
    }

    #[test]
    fn identical_scripts_in_two_folders_keep_their_own_labels() {
        let folders = vec![folder("same", "", ""), folder("same", "", "")];
        let scripts =
            PhaseScripts::assemble(&folders, &labels(), ScriptFlow::Sandwich, None, None, None);
        assert_eq!(
            scripts.pre_request,
            vec![from_folder("api", "same"), from_folder("api/users", "same")]
        );
    }

    #[test]
    fn script_text_is_kept_as_written() {
        let folders = vec![folder("  rok.setVar('a', 1);\n", "", "")];
        let scripts = PhaseScripts::assemble(
            &folders,
            &["api".to_string()],
            ScriptFlow::Sandwich,
            None,
            None,
            None,
        );
        assert_eq!(scripts.pre_request[0].code, "  rok.setVar('a', 1);\n");
    }

    #[test]
    fn folder_labels_name_each_ancestor_folder() {
        assert_eq!(
            folder_labels("api/users/get.yml", 2),
            vec!["api".to_string(), "api/users".to_string()]
        );
        assert!(folder_labels("get.yml", 0).is_empty());
        assert!(folder_labels("", 0).is_empty());
    }

    #[test]
    fn folder_labels_fall_back_when_the_chain_does_not_match_the_path() {
        assert_eq!(
            folder_labels("api/get.yml", 2),
            vec!["folder level 1".to_string(), "folder level 2".to_string()]
        );
    }

    #[test]
    fn a_missing_label_falls_back_to_the_folder_level() {
        let scripts = PhaseScripts::assemble(
            &two_folders(),
            &[],
            ScriptFlow::Sandwich,
            None,
            None,
            None,
        );
        assert_eq!(
            scripts.pre_request,
            vec![
                from_folder("folder level 1", "o-pre"),
                from_folder("folder level 2", "i-pre"),
            ]
        );
    }

    #[test]
    fn errors_name_the_folder_and_leave_request_errors_as_they_were() {
        assert_eq!(
            from_request("x").attribute("before-request", "boom"),
            "boom"
        );
        assert_eq!(
            from_folder("api/users", "x").attribute("tests", "boom"),
            "Folder \"api/users\" tests script: boom"
        );
    }
}
```

- [ ] **Step 3: Run the tests and watch them fail**

Run: `cargo test -j4 -p rocket-app script_chain`

Expected: FAIL to compile (`cannot find type FolderSettings in this scope`, `cannot find type PhaseScripts`, `cannot find function folder_labels`, `cannot find type ScriptSource`).

- [ ] **Step 4: Write the implementation**

Add at the top of `crates/rocket-app/src/execution_service/script_chain.rs`, above the test module:

```rust
//! The folder script chain of one request.
//!
//! The order rule lives in `rocket_collection::chain_scripts` only. This module
//! pairs each script with the folder it came from, so an error can name it.

use rocket_collection::{chain_scripts, FolderSettings, ScriptFlow, ScriptPhase as ChainPhase};

/// Where a chained script came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScriptSource {
    /// A folder's `folder.yml`, named by its path relative to the collection root.
    Folder(String),
    /// The request's own script.
    Request,
}

/// One script of a phase, in run order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChainedScript {
    pub source: ScriptSource,
    pub code: String,
}

impl ChainedScript {
    /// The error text for this script. A request script keeps the raw message,
    /// so the error text users see today does not change.
    pub(crate) fn attribute(&self, phase: &str, message: &str) -> String {
        match &self.source {
            ScriptSource::Request => message.to_string(),
            ScriptSource::Folder(label) => {
                format!("Folder \"{label}\" {phase} script: {message}")
            }
        }
    }
}

/// The scripts of every phase of one request, in run order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PhaseScripts {
    pub pre_request: Vec<ChainedScript>,
    pub post_response: Vec<ChainedScript>,
    pub tests: Vec<ChainedScript>,
}

impl PhaseScripts {
    /// Builds every phase from the folder chain (outermost first) and the
    /// request's own scripts. `labels[i]` names `folders[i]`.
    pub(crate) fn assemble(
        folders: &[FolderSettings],
        labels: &[String],
        flow: ScriptFlow,
        pre_request: Option<&str>,
        post_response: Option<&str>,
        tests: Option<&str>,
    ) -> Self {
        Self {
            pre_request: phase_scripts(ChainPhase::PreRequest, flow, folders, labels, pre_request),
            post_response: phase_scripts(
                ChainPhase::PostResponse,
                flow,
                folders,
                labels,
                post_response,
            ),
            tests: phase_scripts(ChainPhase::Tests, flow, folders, labels, tests),
        }
    }
}

/// Stands in for the request's script when `chain_scripts` orders the markers.
const REQUEST_MARKER: &str = "request";

/// One phase in `chain_scripts` order.
///
/// `chain_scripts` returns script text only, and two folders may hold the same
/// text. So each folder's script is replaced by its index before ordering, and
/// each returned index is mapped back to its script and folder name.
fn phase_scripts(
    phase: ChainPhase,
    flow: ScriptFlow,
    folders: &[FolderSettings],
    labels: &[String],
    request_script: Option<&str>,
) -> Vec<ChainedScript> {
    let codes: Vec<Option<&str>> = folders
        .iter()
        .map(|folder| folder_script(folder, phase))
        .collect();
    let markers: Vec<FolderSettings> = codes
        .iter()
        .enumerate()
        .map(|(index, code)| marker_folder(phase, code.map(|_| index.to_string())))
        .collect();
    let request_code = request_script.filter(|code| !code.trim().is_empty());

    chain_scripts(phase, flow, &markers, request_code.map(|_| REQUEST_MARKER))
        .into_iter()
        .filter_map(|marker| {
            if marker == REQUEST_MARKER {
                return request_code.map(|code| ChainedScript {
                    source: ScriptSource::Request,
                    code: code.to_string(),
                });
            }
            let index: usize = marker.parse().ok()?;
            let code = codes.get(index).copied().flatten()?;
            let label = labels
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("folder level {}", index + 1));
            Some(ChainedScript {
                source: ScriptSource::Folder(label),
                code: code.to_string(),
            })
        })
        .collect()
}

/// The folder's script for one phase, or `None` when it is missing or blank.
fn folder_script(folder: &FolderSettings, phase: ChainPhase) -> Option<&str> {
    let code = match phase {
        ChainPhase::PreRequest => folder.pre_request_script.as_deref(),
        ChainPhase::PostResponse => folder.post_response_script.as_deref(),
        ChainPhase::Tests => folder.tests_script.as_deref(),
    };
    code.filter(|code| !code.trim().is_empty())
}

/// A folder whose only content is `marker` as its script for `phase`.
fn marker_folder(phase: ChainPhase, marker: Option<String>) -> FolderSettings {
    let mut folder = FolderSettings::default();
    match phase {
        ChainPhase::PreRequest => folder.pre_request_script = marker,
        ChainPhase::PostResponse => folder.post_response_script = marker,
        ChainPhase::Tests => folder.tests_script = marker,
    }
    folder
}

/// Names for the `count` folders above `request_path`, outermost first, such as
/// `api` and `api/users` for `api/users/get.yml`. When the chain length does not
/// match the path, the folders are named by level instead.
pub(crate) fn folder_labels(request_path: &str, count: usize) -> Vec<String> {
    let segments: Vec<&str> = request_path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let dirs = &segments[..segments.len().saturating_sub(1)];
    if dirs.len() == count {
        (1..=count).map(|n| dirs[..n].join("/")).collect()
    } else {
        (1..=count).map(|n| format!("folder level {n}")).collect()
    }
}
```

The `pub(crate)` items are unused until Task 2, so `cargo` prints `dead_code` warnings for this commit only. That is expected.

- [ ] **Step 5: Run the tests and watch them pass**

Run: `cargo test -j4 -p rocket-app script_chain`

Expected: PASS, 10 tests in `execution_service::script_chain::tests`.

Run: `cargo check -j4 -p rocket-app`

Expected: success (only the `dead_code` warnings named above).

- [ ] **Step 6: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only:

```bash
git add crates/rocket-app/src/execution_service/script_chain.rs crates/rocket-app/src/execution_service.rs
```

Commit with the same pathspec and a conventional message such as `feat(app): assemble the folder script chain per phase`.

---

## Task 2: Run the chain in every phase for every caller

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs` (imports line 7; `ExecuteRequestInput` lines 37-95; `PhaseState` lines 220-245; `run_script_phase` lines 1064-1105; `begin_phases` lines 1379-1415; the script blocks of `run_before_request_phase` lines 1433-1561, `run_after_response_phase` lines 1640-1682 and `run_tests_phase` lines 1696-1733; `sample_input` line 2565). Line numbers are before Plan 05; find each block by the quoted anchor text.
- Modify: `crates/rocket-app/src/execution_service/script_chain.rs` (add `folder_chain` and the `service_tests` module)
- Modify: `crates/rocket-app/src/test_doubles.rs` (`InMemoryCollectionRepo`, `SharedCollectionRepo`)
- Modify: `crates/rocket-app/src/graphql_schema.rs` (`introspection_input` line 267, test line 418)
- Modify: `crates/rocket-app/src/runner_sequence.rs` (line 208), `crates/rocket-app/src/graphql_request.rs` (line 330), `crates/rocket-app/src/load_test_service.rs` (lines 345 and 443), `crates/rocket-app/src/vault_certificates.rs` (line 711): the new field in each `ExecuteRequestInput` literal
- Test: `crates/rocket-app/src/execution_service/script_chain.rs` (`#[cfg(test)] mod service_tests`)

**Interfaces:**
- Consumes: Task 1 `PhaseScripts`, `ChainedScript`, `folder_labels`; plan 01 `CollectionRepository::get_folder_chain_settings(&self, collection: &str, request_path: &str) -> DomainResult<Vec<FolderSettings>>` and `CollectionSettings.script_flow`; existing `RequestExecutionService::{begin_phases, run_before_request_phase, run_after_response_phase, run_tests_phase, execute}`, `PhaseState`, `ExecutionMode`; test doubles `EmptySecretManagerRepo, FakeSecretStore, FakeVaultSecretFetcher, InMemoryHistoryRepo, NullCookieRepo, NullEnvRepo, RecordingExecutor, RecordingPublisher, SharedExecutor, SharedHistoryRepo, SharedPublisher`.
- Produces:
  - `ExecuteRequestInput.skip_folder_scripts: bool` (`#[serde(default)]`).
  - `PhaseState.scripts: PhaseScripts`.
  - `impl RequestExecutionService { pub(crate) fn folder_chain(&self, input: &ExecuteRequestInput) -> DomainResult<Vec<FolderSettings>> }` (see "Coordination with Plan 05").
  - `run_script_phase(&self, script: &ChainedScript, ctx: ScriptContext, request_name: &str, phase: &str, all_console: &mut Vec<ConsoleEntry>) -> ScriptResult` (private; the first parameter was `_code: &str`).
  - Test doubles: `InMemoryCollectionRepo::with_folder_chain(collection: Collection, folder_chain: Vec<FolderSettings>, root: Option<std::path::PathBuf>) -> Arc<Self>`; `InMemoryCollectionRepo` and `SharedCollectionRepo` answer `get_folder_chain_settings` and `collection_root_path`.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.** (Section 2.7 Folder, section 6 Variable Scopes, and the script phases.) Then run the Plan 05 check from "Coordination with Plan 05" and follow the matching branch.

- [ ] **Step 2: Extend the test doubles**

In `crates/rocket-app/src/test_doubles.rs`, change the `rocket_collection` import to:

```rust
use rocket_collection::{
    Collection, CollectionRepository, CollectionSettings, CollectionSummary, CollectionVariable,
    FolderSettings, Request as CollectionRequest,
};
```

Replace the `InMemoryCollectionRepo` struct and its inherent `impl` (lines 34-43) with:

```rust
/// Collection repo backed by one in-memory `Collection`.
pub struct InMemoryCollectionRepo {
    collection: Collection,
    folder_chain: Vec<FolderSettings>,
    root: Option<std::path::PathBuf>,
}

impl InMemoryCollectionRepo {
    pub fn new(collection: Collection) -> Arc<Self> {
        Self::with_folder_chain(collection, Vec::new(), None)
    }

    /// Every request sits below `folder_chain`, outermost folder first. `root`
    /// is the collection directory scripts may `require()` from.
    pub fn with_folder_chain(
        collection: Collection,
        folder_chain: Vec<FolderSettings>,
        root: Option<std::path::PathBuf>,
    ) -> Arc<Self> {
        Arc::new(Self {
            collection,
            folder_chain,
            root,
        })
    }
}
```

In `impl CollectionRepository for InMemoryCollectionRepo`, after `save_request_variables`, add:

```rust
    fn collection_root_path(&self, _: &str) -> DomainResult<std::path::PathBuf> {
        self.root
            .clone()
            .ok_or_else(|| DomainError::Internal("collection root path is not available".into()))
    }
    fn get_folder_chain_settings(&self, _: &str, _: &str) -> DomainResult<Vec<FolderSettings>> {
        Ok(self.folder_chain.clone())
    }
```

In `impl CollectionRepository for SharedCollectionRepo`, after `save_request_variables`, add:

```rust
    fn collection_root_path(&self, n: &str) -> DomainResult<std::path::PathBuf> {
        self.0.collection_root_path(n)
    }
    fn get_folder_chain_settings(&self, a: &str, b: &str) -> DomainResult<Vec<FolderSettings>> {
        self.0.get_folder_chain_settings(a, b)
    }
```

`InMemoryCollectionRepo::new` keeps its behaviour: no chain, and `collection_root_path` returns the same error the trait default returns.

- [ ] **Step 3: Write the failing service tests**

Append to `crates/rocket-app/src/execution_service/script_chain.rs`:

```rust
#[cfg(test)]
mod service_tests {
    use super::*;
    use crate::execution_service::{ExecuteRequestInput, RequestExecutionService};
    use crate::test_doubles::{
        EmptySecretManagerRepo, FakeSecretStore, FakeVaultSecretFetcher, InMemoryCollectionRepo,
        InMemoryHistoryRepo, NullCookieRepo, NullEnvRepo, RecordingExecutor, RecordingPublisher,
        SharedCollectionRepo, SharedExecutor, SharedHistoryRepo, SharedPublisher,
    };
    use async_trait::async_trait;
    use rocket_collection::settings::SandboxMode as CollectionSandboxMode;
    use rocket_collection::{Collection, CollectionSettings};
    use rocket_http::RequestOptions;
    use rocket_scripting::{
        ExecutionMode, HeaderMutation, RequestMutations, SandboxMode, ScriptContext,
        ScriptEngine, ScriptFileScope, ScriptResult,
    };
    use rocket_shared::error::DomainResult;
    use rocket_shared::events::DomainEvent;
    use rocket_shared::types::{Auth, HttpMethod};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    /// What one engine call saw.
    #[derive(Debug, Clone)]
    struct Seen {
        /// `"<phase>:<code>"`.
        call: String,
        headers: Vec<(String, String)>,
        runtime: HashMap<String, String>,
        sandbox: SandboxMode,
        file_scope: Option<ScriptFileScope>,
    }

    /// Engine that answers a canned result per script text and records every call.
    struct CodeEngine {
        results: Mutex<HashMap<String, ScriptResult>>,
        seen: Mutex<Vec<Seen>>,
    }

    impl CodeEngine {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                results: Mutex::new(HashMap::new()),
                seen: Mutex::new(Vec::new()),
            })
        }
        fn on(&self, code: &str, result: ScriptResult) {
            self.results
                .lock()
                .expect("lock")
                .insert(code.to_string(), result);
        }
        fn seen(&self) -> Vec<Seen> {
            self.seen.lock().expect("lock").clone()
        }
        fn calls(&self) -> Vec<String> {
            self.seen().into_iter().map(|seen| seen.call).collect()
        }
    }

    #[async_trait]
    impl ScriptEngine for CodeEngine {
        async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
            self.seen.lock().expect("lock").push(Seen {
                call: format!("{}:{}", ctx.phase.as_str(), ctx.code),
                headers: ctx
                    .request
                    .headers
                    .iter()
                    .map(|h| (h.key.clone(), h.value.clone()))
                    .collect(),
                runtime: ctx.variables.runtime.clone(),
                sandbox: ctx.sandbox_mode,
                file_scope: ctx.file_scope.clone(),
            });
            Ok(self
                .results
                .lock()
                .expect("lock")
                .get(&ctx.code)
                .cloned()
                .unwrap_or_default())
        }
    }

    struct SharedCodeEngine(Arc<CodeEngine>);

    #[async_trait]
    impl ScriptEngine for SharedCodeEngine {
        async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
            self.0.execute(ctx).await
        }
    }

    struct Harness {
        svc: RequestExecutionService,
        engine: Arc<CodeEngine>,
        executor: Arc<RecordingExecutor>,
        publisher: Arc<RecordingPublisher>,
    }

    fn harness(
        settings: CollectionSettings,
        chain: Vec<FolderSettings>,
        root: Option<PathBuf>,
    ) -> Harness {
        let mut collection = Collection::new("col");
        collection.settings = settings;
        let repo = InMemoryCollectionRepo::with_folder_chain(collection, chain, root);
        let engine = CodeEngine::new();
        let executor = RecordingExecutor::new();
        let publisher = RecordingPublisher::new();
        let svc = RequestExecutionService::new(
            Box::new(NullEnvRepo),
            Arc::new(SharedExecutor(Arc::clone(&executor))),
            Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
            Box::new(SharedCollectionRepo(repo)),
            Box::new(NullCookieRepo),
            Box::new(SharedPublisher(Arc::clone(&publisher))),
            Box::new(EmptySecretManagerRepo),
            Arc::new(FakeSecretStore("client-secret".into())),
            FakeVaultSecretFetcher::new(HashMap::new()),
        )
        .with_script_engine(Box::new(SharedCodeEngine(Arc::clone(&engine))));
        Harness {
            svc,
            engine,
            executor,
            publisher,
        }
    }

    fn settings(flow: ScriptFlow) -> CollectionSettings {
        CollectionSettings {
            script_flow: flow,
            ..CollectionSettings::default()
        }
    }

    /// Outer folder `api`, inner folder `api/users`, each with all three scripts.
    fn chain() -> Vec<FolderSettings> {
        vec![
            FolderSettings {
                pre_request_script: Some("o-pre".into()),
                post_response_script: Some("o-post".into()),
                tests_script: Some("o-test".into()),
                ..FolderSettings::default()
            },
            FolderSettings {
                pre_request_script: Some("i-pre".into()),
                post_response_script: Some("i-post".into()),
                tests_script: Some("i-test".into()),
                ..FolderSettings::default()
            },
        ]
    }

    /// A request at `api/users/get.yml` in collection `col`.
    pub(super) fn input(
        pre: Option<&str>,
        post: Option<&str>,
        tests: Option<&str>,
    ) -> ExecuteRequestInput {
        ExecuteRequestInput {
            method: HttpMethod::Get,
            url: "https://api.example.com/users/1".into(),
            headers: vec![],
            query_params: vec![],
            body: None,
            auth: Auth::None,
            options: RequestOptions::default(),
            environment_name: None,
            collection: Some("col".into()),
            request_name: Some("Get user".into()),
            request_path: Some("api/users/get.yml".into()),
            tags: vec![],
            path_params: vec![],
            pre_request_script: pre.map(str::to_string),
            post_response_script: post.map(str::to_string),
            tests_script: tests.map(str::to_string),
            global_env_name: None,
            assertions: vec![],
            actions: vec![],
            request_guard_policy: rocket_workspace::RequestGuardPolicy::default(),
            skip_history: false,
            flow_vars: HashMap::new(),
            skip_folder_scripts: false,
        }
    }

    #[tokio::test]
    async fn sandwich_flow_runs_folder_and_request_scripts_in_spec_order() {
        let h = harness(settings(ScriptFlow::Sandwich), chain(), None);
        h.svc
            .execute(input(Some("r-pre"), Some("r-post"), Some("r-test")))
            .await
            .expect("execute");
        assert_eq!(
            h.engine.calls(),
            vec![
                "before-request:o-pre",
                "before-request:i-pre",
                "before-request:r-pre",
                "after-response:r-post",
                "after-response:i-post",
                "after-response:o-post",
                "tests:r-test",
                "tests:i-test",
                "tests:o-test",
            ]
        );
    }

    #[tokio::test]
    async fn sequential_flow_runs_folders_first_in_every_phase() {
        let h = harness(settings(ScriptFlow::Sequential), chain(), None);
        h.svc
            .execute(input(Some("r-pre"), Some("r-post"), Some("r-test")))
            .await
            .expect("execute");
        assert_eq!(
            h.engine.calls(),
            vec![
                "before-request:o-pre",
                "before-request:i-pre",
                "before-request:r-pre",
                "after-response:o-post",
                "after-response:i-post",
                "after-response:r-post",
                "tests:o-test",
                "tests:i-test",
                "tests:r-test",
            ]
        );
    }

    #[tokio::test]
    async fn a_later_script_sees_the_request_and_variables_an_earlier_one_left() {
        let h = harness(settings(ScriptFlow::Sandwich), chain(), None);
        h.engine.on(
            "o-pre",
            ScriptResult {
                request_mutations: Some(RequestMutations {
                    headers: vec![HeaderMutation::Set {
                        name: "X-Folder".into(),
                        value: "outer".into(),
                    }],
                    ..Default::default()
                }),
                runtime_vars: HashMap::from([("token".to_string(), serde_json::json!("abc"))]),
                ..Default::default()
            },
        );
        h.svc
            .execute(input(Some("r-pre"), Some("r-post"), None))
            .await
            .expect("execute");

        let seen = h.engine.seen();
        let request_pre = seen
            .iter()
            .find(|s| s.call == "before-request:r-pre")
            .expect("request script ran");
        assert!(request_pre
            .headers
            .contains(&("X-Folder".to_string(), "outer".to_string())));
        assert_eq!(
            request_pre.runtime.get("token").map(String::as_str),
            Some("abc")
        );
        let outer_post = seen
            .iter()
            .find(|s| s.call == "after-response:o-post")
            .expect("folder post-response script ran");
        assert_eq!(
            outer_post.runtime.get("token").map(String::as_str),
            Some("abc")
        );
    }

    #[tokio::test]
    async fn a_folder_script_error_names_the_folder_and_ends_its_phase() {
        let h = harness(settings(ScriptFlow::Sandwich), chain(), None);
        h.engine.on(
            "i-pre",
            ScriptResult {
                error: Some("boom".into()),
                ..Default::default()
            },
        );
        h.engine.on(
            "o-post",
            ScriptResult {
                error: Some("later".into()),
                ..Default::default()
            },
        );
        let out = h
            .svc
            .execute(input(Some("r-pre"), Some("r-post"), None))
            .await
            .expect("execute");

        let expected = "Folder \"api/users\" before-request script: boom";
        assert_eq!(out.script_error.as_deref(), Some(expected));
        assert_eq!(
            h.engine.calls(),
            vec![
                "before-request:o-pre",
                "before-request:i-pre",
                "after-response:r-post",
                "after-response:i-post",
                "after-response:o-post",
                "tests:i-test",
                "tests:o-test",
            ],
            "the request's pre-request script is skipped, later phases still run"
        );
        assert_eq!(
            h.executor.sent_urls().len(),
            1,
            "a script error does not stop the send"
        );
        let events = h.publisher.events();
        assert!(events.iter().any(|e| matches!(
            e,
            DomainEvent::ScriptError { phase, message, .. }
                if phase == "before-request" && message == expected
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            DomainEvent::ScriptError { message, .. }
                if message == "Folder \"api\" after-response script: later"
        )));
    }

    #[tokio::test]
    async fn skip_request_from_a_folder_script_ends_the_chain_in_a_run() {
        let h = harness(settings(ScriptFlow::Sandwich), chain(), None);
        h.engine.on(
            "o-pre",
            ScriptResult {
                skip_request: true,
                ..Default::default()
            },
        );
        let inp = input(Some("r-pre"), None, None);
        let mut state = h
            .svc
            .begin_phases(&inp, &HashMap::new())
            .expect("begin phases");
        h.svc
            .run_before_request_phase(&inp, ExecutionMode::Runner, &mut state)
            .await
            .expect("before-request phase");
        assert!(state.skip_request);
        assert_eq!(h.engine.calls(), vec!["before-request:o-pre"]);
    }

    #[tokio::test]
    async fn skip_request_from_a_folder_script_is_ignored_by_a_single_send() {
        let h = harness(settings(ScriptFlow::Sandwich), chain(), None);
        h.engine.on(
            "o-pre",
            ScriptResult {
                skip_request: true,
                ..Default::default()
            },
        );
        h.svc
            .execute(input(Some("r-pre"), None, None))
            .await
            .expect("execute");
        assert_eq!(
            h.engine.calls()[..3].to_vec(),
            vec![
                "before-request:o-pre",
                "before-request:i-pre",
                "before-request:r-pre",
            ]
        );
        assert_eq!(h.executor.sent_urls().len(), 1);
    }

    #[tokio::test]
    async fn a_folder_without_scripts_changes_nothing() {
        let h = harness(
            settings(ScriptFlow::Sandwich),
            vec![FolderSettings::default()],
            None,
        );
        let out = h
            .svc
            .execute(input(Some("r-pre"), None, None))
            .await
            .expect("execute");
        assert_eq!(h.engine.calls(), vec!["before-request:r-pre"]);
        assert!(out.script_error.is_none());

        let h = harness(
            settings(ScriptFlow::Sandwich),
            vec![FolderSettings::default()],
            None,
        );
        h.svc
            .execute(input(None, None, None))
            .await
            .expect("execute");
        assert!(h.engine.calls().is_empty());
        assert_eq!(h.executor.sent_urls().len(), 1);
    }

    #[tokio::test]
    async fn folder_scripts_run_with_the_request_scripts_sandbox_and_file_scope() {
        let mut developer = settings(ScriptFlow::Sandwich);
        developer.sandbox_mode = CollectionSandboxMode::Developer;
        let root = PathBuf::from("/tmp/rocket-folder-chain-test");
        let h = harness(developer, chain(), Some(root.clone()));
        h.svc
            .execute(input(Some("r-pre"), None, None))
            .await
            .expect("execute");

        let expected_scope = Some(ScriptFileScope {
            collection_root: root,
            additional_roots: vec![],
        });
        let seen = h.engine.seen();
        assert_eq!(seen.len(), 7, "three pre-request, two post-response, two tests");
        for call in &seen {
            assert_eq!(call.sandbox, SandboxMode::Developer, "{}", call.call);
            assert_eq!(call.file_scope, expected_scope, "{}", call.call);
        }
    }

    #[tokio::test]
    async fn inline_flow_requests_and_introspection_run_no_folder_scripts() {
        let h = harness(settings(ScriptFlow::Sandwich), chain(), None);
        let mut inline = input(Some("r-pre"), None, None);
        inline.request_path = Some("__flow_inline__/node-1".into());
        h.svc.execute(inline).await.expect("execute");
        assert_eq!(h.engine.calls(), vec!["before-request:r-pre"]);

        let h = harness(settings(ScriptFlow::Sandwich), chain(), None);
        let mut introspection = input(Some("r-pre"), None, None);
        introspection.skip_folder_scripts = true;
        h.svc.execute(introspection).await.expect("execute");
        assert_eq!(h.engine.calls(), vec!["before-request:r-pre"]);
    }
}
```

In `crates/rocket-app/src/graphql_schema.rs`, in the test `introspection_input_drops_scripts_assertions_actions_and_history` (line 418), after `assert!(out.tests_script.is_none());` add:

```rust
        assert!(out.skip_folder_scripts, "introspection runs no folder scripts");
```

- [ ] **Step 4: Run the tests and watch them fail**

Run: `cargo test -j4 -p rocket-app script_chain`

Expected: FAIL to compile (`struct ExecuteRequestInput has no field named skip_folder_scripts`).

- [ ] **Step 5: Add `skip_folder_scripts` to `ExecuteRequestInput` and every literal**

In `crates/rocket-app/src/execution_service.rs`, inside `ExecuteRequestInput`, after the `flow_vars` field (line 94), add:

```rust
    /// When true, no folder scripts run for this send. GraphQL introspection
    /// sets it, because it runs none of the request's own scripts either.
    #[serde(default)]
    pub skip_folder_scripts: bool,
```

Add `skip_folder_scripts: false,` on the line after `flow_vars: ...,` in each of these `ExecuteRequestInput` literals: `crates/rocket-app/src/runner_sequence.rs` line 208, `crates/rocket-app/src/graphql_request.rs` line 330, `crates/rocket-app/src/load_test_service.rs` lines 345 and 443, `crates/rocket-app/src/vault_certificates.rs` line 711, `crates/rocket-app/src/execution_service.rs` line 2565 (`sample_input`). Run `cargo check -j4 -p rocket-app --tests` afterwards. If it names another literal (for example one Plan 05 added), add the same line there.

In `crates/rocket-app/src/graphql_schema.rs`, in `introspection_input`, after `input.tests_script = None;` add:

```rust
    input.skip_folder_scripts = true;
```

- [ ] **Step 6: Add the chain loader**

In `crates/rocket-app/src/execution_service/script_chain.rs`, extend the imports at the top to:

```rust
use super::{ExecuteRequestInput, RequestExecutionService};
use rocket_collection::{chain_scripts, FolderSettings, ScriptFlow, ScriptPhase as ChainPhase};
use rocket_shared::error::DomainResult;
```

and add, after `folder_labels`:

```rust
/// Request path prefix of an inline Flow node. It must match the path built in
/// `flow_execution_service.rs` for `RequestSource::Inline`.
const FLOW_INLINE_PREFIX: &str = "__flow_inline__/";

impl RequestExecutionService {
    /// The folders above the request, outermost first. A request without a
    /// collection or path, and an inline Flow request, have none. A `folder.yml`
    /// that cannot be read is an error naming the folder, never an empty chain.
    pub(crate) fn folder_chain(
        &self,
        input: &ExecuteRequestInput,
    ) -> DomainResult<Vec<FolderSettings>> {
        let (Some(collection), Some(path)) =
            (input.collection.as_deref(), input.request_path.as_deref())
        else {
            return Ok(Vec::new());
        };
        if path.starts_with(FLOW_INLINE_PREFIX) {
            return Ok(Vec::new());
        }
        self.collection_repo
            .get_folder_chain_settings(collection, path)
    }
}
```

- [ ] **Step 7: Store the chain on `PhaseState` in `begin_phases`**

In `crates/rocket-app/src/execution_service.rs`, change line 7 to:

```rust
use rocket_collection::{
    settings::SandboxMode as CollectionSandboxMode, CollectionRepository, FolderSettings,
    ScriptFlow,
};
```

and after the `pub(crate) mod script_chain;` line add:

```rust
use self::script_chain::{folder_labels, ChainedScript, PhaseScripts};
```

In `PhaseState`, after the `file_scope` field, add:

```rust
    /// Scripts of every phase in run order: the folder chain and the request's
    /// own script, built once in `begin_phases`.
    pub scripts: PhaseScripts,
```

In `begin_phases`, replace the block from `let (sandbox_mode, file_scope) = match input.collection.as_deref() {` through the end of the `Ok(PhaseState { ... })` expression with:

```rust
        let (sandbox_mode, file_scope, script_flow) = match input.collection.as_deref() {
            Some(col) => {
                let settings = self.collection_repo.get_settings(col).unwrap_or_default();
                let mode = match settings.sandbox_mode {
                    CollectionSandboxMode::Safe => SandboxMode::Safe,
                    CollectionSandboxMode::Developer => SandboxMode::Developer,
                };
                // A collection whose directory cannot be resolved just gets no scope.
                let scope = self
                    .collection_repo
                    .collection_root_path(col)
                    .ok()
                    .map(|root| ScriptFileScope {
                        collection_root: root,
                        additional_roots: settings
                            .script_context_roots
                            .iter()
                            .map(std::path::PathBuf::from)
                            .collect(),
                    });
                (mode, scope, settings.script_flow)
            }
            None => (SandboxMode::Safe, None, ScriptFlow::default()),
        };

        // One chain read per execution. A folder.yml that cannot be read fails
        // the send here, with an error that names the folder.
        let folder_chain = self.folder_chain(input)?;
        let script_folders: &[FolderSettings] = if input.skip_folder_scripts {
            &[]
        } else {
            &folder_chain
        };
        let labels = folder_labels(
            input.request_path.as_deref().unwrap_or_default(),
            script_folders.len(),
        );
        let scripts = PhaseScripts::assemble(
            script_folders,
            &labels,
            script_flow,
            input.pre_request_script.as_deref(),
            input.post_response_script.as_deref(),
            input.tests_script.as_deref(),
        );

        Ok(PhaseState {
            http_request,
            var_ctx,
            script_error: None,
            console: Vec::new(),
            test_results: Vec::new(),
            next_request: None,
            skip_request: false,
            vault_forms: crate::redaction::secret_forms(external_secrets.values()),
            sandbox_mode,
            file_scope,
            scripts,
        })
```

If the Plan 05 check found that `begin_phases` already holds the chain in a local, use that local instead of `let folder_chain = self.folder_chain(input)?;`.

- [ ] **Step 8: Attribute errors in `run_script_phase`**

Replace the whole `run_script_phase` method (anchor `async fn run_script_phase(`) with:

```rust
    /// Runs one chained script. A failure is published as `ScriptError` and
    /// returned in `error`. Both name the folder when the script came from one.
    async fn run_script_phase(
        &self,
        script: &ChainedScript,
        ctx: ScriptContext,
        request_name: &str,
        phase: &str,
        all_console: &mut Vec<ConsoleEntry>,
    ) -> ScriptResult {
        let engine = match self.script_engine.as_ref() {
            Some(e) => e,
            None => return ScriptResult::default(),
        };
        match engine.execute(ctx).await {
            Ok(mut result) => {
                if let Some(err) = result.error.take() {
                    let message = script.attribute(phase, &err);
                    self.events.publish(DomainEvent::ScriptError {
                        request_name: request_name.to_string(),
                        phase: phase.to_string(),
                        message: message.clone(),
                    });
                    result.error = Some(message);
                }
                all_console.extend(result.console_entries.clone());
                result
            }
            Err(e) => {
                let message = script.attribute(phase, &e.to_string());
                self.events.publish(DomainEvent::ScriptError {
                    request_name: request_name.to_string(),
                    phase: phase.to_string(),
                    message: message.clone(),
                });
                // Carry the failure in `error` as well. Callers build
                // ExecuteOutput.script_error from this field only, so without
                // it a timed-out script would fire an event but show nothing
                // in the request's own error surface.
                ScriptResult {
                    error: Some(message),
                    ..Default::default()
                }
            }
        }
    }
```

- [ ] **Step 9: Loop over the chain in the three phase methods**

In `run_before_request_phase`, replace the block from `if let Some(code) = &input.pre_request_script {` through its matching closing brace (the line before `// ── Before-request actions (runtime.actions, set-variable) ─────────────`) with:

```rust
        // The folder chain and the request's own script, in `chain_scripts`
        // order. Each script is its own engine run, and sees the request and
        // the variables the scripts before it left behind.
        let scripts = state.scripts.pre_request.clone();
        for script in &scripts {
            let ctx = ScriptContext::before_request(
                script.code.clone(),
                state.var_ctx.clone(),
                state.http_request.clone(),
                env_name.clone(),
                request_name.clone(),
                input.tags.clone(),
                input.path_params.clone(),
            )
            .with_execution_mode(mode)
            .with_sandbox_mode(state.sandbox_mode)
            .with_file_scope(state.file_scope.clone());
            let had_error = state.script_error.is_some();
            let result = self
                .run_script_phase(
                    script,
                    ctx,
                    &request_name,
                    "before-request",
                    &mut state.console,
                )
                .await;

            // Apply request mutations.
            if let Some(ref mutations) = result.request_mutations {
                if let Some(ref url) = mutations.url {
                    let original_url = state.http_request.url.clone();
                    // Deliberate: an early return here also discards any
                    // next_request/runtime_vars this same script set below.
                    // A script whose req.setUrl() just tripped the SSRF
                    // guard does not get to steer the run via
                    // setNextRequest() or leave variables behind either.
                    self.check_request_guard(&original_url, url, &input.request_guard_policy)?;
                    state.http_request.url = url.clone();
                }
                if let Some(ref method_str) = mutations.method {
                    if let Ok(m) = method_str.parse() {
                        state.http_request.method = m;
                    } else {
                        tracing::warn!(
                            method = %method_str,
                            "req.setMethod() called with an unrecognized HTTP method, ignored"
                        );
                        state.script_error.get_or_insert_with(|| format!(
                            "req.setMethod('{method_str}') is not a valid HTTP method — ignored."
                        ));
                    }
                }
                // Apply header mutations in the order the script issued them —
                // e.g. deleteHeader() then setHeader() on the same name must
                // result in the header being present, not dropped.
                for mutation in &mutations.headers {
                    match mutation {
                        rocket_scripting::HeaderMutation::Set { name, value } => {
                            if let Some(h) = state
                                .http_request
                                .headers
                                .iter_mut()
                                .find(|h| h.key.eq_ignore_ascii_case(name))
                            {
                                h.value = value.clone();
                            } else {
                                state.http_request.headers.push(Header::new(name, value));
                            }
                        }
                        rocket_scripting::HeaderMutation::Delete { name } => {
                            state
                                .http_request
                                .headers
                                .retain(|h| !h.key.eq_ignore_ascii_case(name));
                        }
                    }
                }
                if let Some(ms) = mutations.timeout_ms {
                    state.http_request.options.timeout_ms = ms;
                }
                if let Some(ref body_val) = mutations.body {
                    // A JS object/array is unambiguously meant as JSON. A string
                    // may be non-JSON text (XML, plain text, etc) — respect an
                    // explicit Content-Type header the script already set instead
                    // of forcing JSON, which would mislabel the body on the wire.
                    let mode = if body_val.is_object() || body_val.is_array() {
                        rocket_shared::types::BodyMode::Json
                    } else {
                        body_mode_from_content_type(&state.http_request.headers)
                    };
                    let content = body_val
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| body_val.to_string());
                    state.http_request.body = Some(rocket_shared::types::Body {
                        mode,
                        content: Some(content),
                        form_data: None,
                        file_path: None,
                    });
                }
                if let Some(n) = mutations.max_redirects {
                    state.http_request.options.max_redirects = Some(n);
                }
            }

            self.apply_script_side_effects(
                &result,
                input.environment_name.as_deref(),
                input.global_env_name.as_deref(),
                input.collection.as_deref(),
                &mut state.var_ctx,
                VaultGuard {
                    forms: &state.vault_forms,
                    console: &mut state.console,
                },
            );

            // The first error in phase order is kept. Within one script, a
            // thrown error still replaces its own setMethod warning.
            let failed = result.error.is_some();
            if failed && !had_error {
                state.script_error = result.error;
            }

            // Runner controls. `execute()` never reads these; the runner
            // checks them after every phase that ran (spec §4).
            if result.skip_request {
                state.skip_request = true;
            }
            if result.next_request.is_some() {
                state.next_request = result.next_request.clone();
            }

            // A failed script ends its phase. In a run, skipRequest() ends it
            // too, because the request will not be sent.
            if failed || (result.skip_request && mode == ExecutionMode::Runner) {
                break;
            }
        }
```

The mutation block is the existing code, moved out one nesting level and otherwise unchanged.

In `run_after_response_phase`, replace the block from `if let Some(code) = &input.post_response_script {` through its matching closing brace (the last statement of the method) with:

```rust
        let scripts = state.scripts.post_response.clone();
        for script in &scripts {
            let ctx = ScriptContext::after_response(
                script.code.clone(),
                state.var_ctx.clone(),
                state.http_request.clone(),
                response.clone(),
                env_name.clone(),
                request_name.clone(),
                input.tags.clone(),
                input.path_params.clone(),
            )
            .with_execution_mode(mode)
            .with_sandbox_mode(state.sandbox_mode)
            .with_file_scope(state.file_scope.clone());
            let result = self
                .run_script_phase(
                    script,
                    ctx,
                    &request_name,
                    "after-response",
                    &mut state.console,
                )
                .await;
            self.apply_script_side_effects(
                &result,
                input.environment_name.as_deref(),
                input.global_env_name.as_deref(),
                input.collection.as_deref(),
                &mut state.var_ctx,
                VaultGuard {
                    forms: &state.vault_forms,
                    console: &mut state.console,
                },
            );
            let failed = result.error.is_some();
            if failed && state.script_error.is_none() {
                state.script_error = result.error;
            }
            if result.next_request.is_some() {
                state.next_request = result.next_request.clone();
            }
            // A failed script ends its phase.
            if failed {
                break;
            }
        }
```

In `run_tests_phase`, replace the block from `if let Some(code) = &input.tests_script {` through its matching closing brace (the last statement of the method) with:

```rust
        let scripts = state.scripts.tests.clone();
        for script in &scripts {
            let ctx = ScriptContext::tests(
                script.code.clone(),
                state.var_ctx.clone(),
                state.http_request.clone(),
                response.clone(),
                env_name.clone(),
                request_name.clone(),
                input.tags.clone(),
                input.path_params.clone(),
            )
            .with_execution_mode(mode)
            .with_sandbox_mode(state.sandbox_mode)
            .with_file_scope(state.file_scope.clone());
            let result = self
                .run_script_phase(script, ctx, &request_name, "tests", &mut state.console)
                .await;
            self.apply_script_side_effects(
                &result,
                input.environment_name.as_deref(),
                input.global_env_name.as_deref(),
                input.collection.as_deref(),
                &mut state.var_ctx,
                VaultGuard {
                    forms: &state.vault_forms,
                    console: &mut state.console,
                },
            );
            state.test_results.extend(result.test_results.clone());
            let failed = result.error.is_some();
            if failed && state.script_error.is_none() {
                state.script_error = result.error;
            }
            if result.next_request.is_some() {
                state.next_request = result.next_request.clone();
            }
            // A failed script ends its phase.
            if failed {
                break;
            }
        }
```

- [ ] **Step 10: Run the tests and watch them pass**

Run: `cargo test -j4 -p rocket-app script_chain`

Expected: PASS, 19 tests (10 in `script_chain::tests`, 9 in `script_chain::service_tests`).

Run: `cargo test -j4 -p rocket-app`

Expected: PASS. The existing request-script, runner, Flow, GraphQL and load-test tests pass unchanged, which proves the request-only path behaves as before. This is a single-crate run, not `--workspace`.

Run: `cargo check -j4 -p rocket` (the `src-tauri` package)

Expected: success. `src-tauri` builds no `ExecuteRequestInput` literal, but it deserializes the struct from IPC.

- [ ] **Step 11: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only:

```bash
git add crates/rocket-app/src/execution_service.rs crates/rocket-app/src/execution_service/script_chain.rs crates/rocket-app/src/test_doubles.rs crates/rocket-app/src/graphql_schema.rs crates/rocket-app/src/runner_sequence.rs crates/rocket-app/src/graphql_request.rs crates/rocket-app/src/load_test_service.rs crates/rocket-app/src/vault_certificates.rs
```

Add any extra file Step 5's `cargo check` pointed at. Commit with the same pathspec and a conventional message such as `feat(app): run folder scripts in every execution phase`.

---

## Task 3: Secret-use scan covers the folder chain, and docs

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs` (`references_alias`, lines 491-540)
- Modify: `crates/rocket-app/src/execution_service/script_chain.rs` (add `script_mentions`, `folder_mentions` and the `secret_scan_tests` module)
- Modify: `crates/rocket-app/CLAUDE.md` (Key Patterns)
- Test: `crates/rocket-app/src/execution_service/script_chain.rs` (`#[cfg(test)] mod secret_scan_tests`)

**Interfaces:**
- Consumes: Task 2 `folder_chain`, `service_tests::input`, `InMemoryCollectionRepo::with_folder_chain`; existing `RequestExecutionService::execute`, test doubles `FakeSecretManagerRepo, FakeSecretStore, InMemoryHistoryRepo, NullCookieRepo, RecordingExecutor, SharedCollectionRepo, SharedExecutor, SharedHistoryRepo, StaticEnvRepo`; `rocket_environment::{Environment, ExternalSecretBinding, ExternalSecretRef, SecretManagerConnection, VaultSecretFetcher}`.
- Produces (crate-internal, in `script_chain`):
  - `pub(crate) fn script_mentions(script: &str, needle: &str) -> bool`
  - `pub(crate) fn folder_mentions(folder: &FolderSettings, needle: &str, with_scripts: bool) -> bool`

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.** (Section 6 Variable Scopes and the `{{alias.secretName}}` RocketVault reference form.)

- [ ] **Step 2: Write the failing tests**

Append to `crates/rocket-app/src/execution_service/script_chain.rs`:

```rust
#[cfg(test)]
mod secret_scan_tests {
    use super::service_tests::input;
    use super::*;
    use crate::execution_service::RequestExecutionService;
    use crate::test_doubles::{
        FakeSecretManagerRepo, FakeSecretStore, InMemoryCollectionRepo, InMemoryHistoryRepo,
        NullCookieRepo, RecordingExecutor, SharedCollectionRepo, SharedExecutor,
        SharedHistoryRepo, StaticEnvRepo,
    };
    use async_trait::async_trait;
    use rocket_collection::Collection;
    use rocket_environment::{
        Environment, ExternalSecretBinding, ExternalSecretRef, SecretManagerConnection,
        VaultSecretFetcher,
    };
    use rocket_shared::error::DomainError;
    use rocket_shared::events::NullEventPublisher;
    use rocket_shared::types::{Auth, Header};
    use std::sync::Arc;

    /// Every secret fetch fails, as when the vault rejects the credentials.
    struct FailingFetcher;

    #[async_trait]
    impl VaultSecretFetcher for FailingFetcher {
        async fn list_secrets(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
        ) -> DomainResult<Vec<ExternalSecretRef>> {
            Ok(Vec::new())
        }

        async fn get_secret_value(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
            _secret_id: &str,
        ) -> DomainResult<Option<String>> {
            Err(DomainError::Internal("vault rejected the credentials".into()))
        }

        async fn test_connection(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
        ) -> DomainResult<()> {
            Ok(())
        }
    }

    fn connection() -> SecretManagerConnection {
        SecretManagerConnection {
            id: "conn-1".to_string(),
            label: "Test".to_string(),
            base_url: "https://vault.internal:8774".to_string(),
            client_id: "rocketapi".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
            provider: Default::default(),
            config: None,
        }
    }

    /// Environment `prod` with one binding, `payments.apiKey`.
    fn environment() -> Environment {
        let mut env = Environment::new("prod");
        env.external_secrets.push(ExternalSecretBinding {
            alias: "payments".to_string(),
            connection_id: "conn-1".to_string(),
            vault_name: "prod-vault".to_string(),
            secret_names: vec![ExternalSecretRef {
                name: "apiKey".to_string(),
                secret_id: "sec-1".to_string(),
            }],
        });
        env
    }

    fn service(folder: FolderSettings) -> (RequestExecutionService, Arc<RecordingExecutor>) {
        let repo =
            InMemoryCollectionRepo::with_folder_chain(Collection::new("col"), vec![folder], None);
        let executor = RecordingExecutor::new();
        let svc = RequestExecutionService::new(
            Box::new(StaticEnvRepo(environment())),
            Arc::new(SharedExecutor(Arc::clone(&executor))),
            Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
            Box::new(SharedCollectionRepo(repo)),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(FakeSecretManagerRepo(connection())),
            Arc::new(FakeSecretStore("client-secret".into())),
            Arc::new(FailingFetcher),
        );
        (svc, executor)
    }

    /// The request itself never mentions `payments`.
    fn prod_input() -> super::ExecuteRequestInput {
        let mut inp = input(None, None, None);
        inp.environment_name = Some("prod".into());
        inp
    }

    #[tokio::test]
    async fn a_folder_script_reading_a_failing_secret_blocks_the_send() {
        let (svc, executor) = service(FolderSettings {
            pre_request_script: Some("rok.getSecretVar(\"payments.apiKey\")".into()),
            ..FolderSettings::default()
        });
        assert!(svc.execute(prod_input()).await.is_err());
        assert!(executor.sent_urls().is_empty());
    }

    #[tokio::test]
    async fn a_commented_out_folder_secret_read_does_not_block_the_send() {
        let (svc, executor) = service(FolderSettings {
            tests_script: Some(
                "  // rok.getSecretVar(\"payments.apiKey\")\nconsole.log(1)".into(),
            ),
            ..FolderSettings::default()
        });
        assert!(svc.execute(prod_input()).await.is_ok());
        assert_eq!(executor.sent_urls().len(), 1);
    }

    #[tokio::test]
    async fn a_folder_header_naming_a_failing_secret_blocks_the_send() {
        let (svc, executor) = service(FolderSettings {
            headers: vec![Header::new("X-Api-Key", "{{payments.apiKey}}")],
            ..FolderSettings::default()
        });
        assert!(svc.execute(prod_input()).await.is_err());
        assert!(executor.sent_urls().is_empty());
    }

    #[test]
    fn folder_mentions_skips_scripts_only_when_asked() {
        let scripted = FolderSettings {
            post_response_script: Some("rok.getSecretVar('payments.apiKey')".into()),
            ..FolderSettings::default()
        };
        assert!(folder_mentions(&scripted, "payments.", true));
        assert!(!folder_mentions(&scripted, "payments.", false));

        let with_auth = FolderSettings {
            auth: Some(Auth::Bearer {
                token: "{{payments.token}}".into(),
            }),
            ..FolderSettings::default()
        };
        assert!(folder_mentions(&with_auth, "payments.", false));
        assert!(!folder_mentions(&FolderSettings::default(), "payments.", true));
    }

    #[test]
    fn script_mentions_ignores_full_line_comments() {
        assert!(script_mentions("const k = rok.getSecretVar('payments.apiKey');", "payments."));
        assert!(!script_mentions("   // rok.getSecretVar('payments.apiKey')", "payments."));
    }
}
```

In `service_tests`, `input` is already `pub(super)`, so `secret_scan_tests` can import it.

- [ ] **Step 3: Run the tests and watch them fail**

Run: `cargo test -j4 -p rocket-app secret_scan_tests`

Expected: FAIL to compile (`cannot find function folder_mentions`, `cannot find function script_mentions`).

- [ ] **Step 4: Write the implementation**

In `crates/rocket-app/src/execution_service/script_chain.rs`, after the `impl RequestExecutionService` block, add:

```rust
/// Whether a script uses `needle` on a line that is not a full-line `//` comment.
pub(crate) fn script_mentions(script: &str, needle: &str) -> bool {
    script
        .lines()
        .any(|line| !line.trim_start().starts_with("//") && line.contains(needle))
}

/// Whether a folder sends or reads `needle`: in its scripts (when `with_scripts`
/// is set), its headers or its auth. Folder variables are checked with the
/// other variable scopes. Like the request check, this may over-match, never
/// under-match.
pub(crate) fn folder_mentions(folder: &FolderSettings, needle: &str, with_scripts: bool) -> bool {
    let scripts = [
        &folder.pre_request_script,
        &folder.post_response_script,
        &folder.tests_script,
    ];
    if with_scripts
        && scripts
            .iter()
            .filter_map(|script| script.as_deref())
            .any(|script| script_mentions(script, needle))
    {
        return true;
    }
    serde_json::to_string(&folder.headers).map_or(true, |text| text.contains(needle))
        || serde_json::to_string(&folder.auth).map_or(true, |text| text.contains(needle))
}
```

In `crates/rocket-app/src/execution_service.rs`, change the `use self::script_chain::...` line to:

```rust
use self::script_chain::{
    folder_labels, folder_mentions, script_mentions, ChainedScript, PhaseScripts,
};
```

In `references_alias`, replace the block from `let scripts = [` through the `return true;` and closing brace of the first `if` (lines 501-516) with:

```rust
        let scripts = [
            &input.pre_request_script,
            &input.post_response_script,
            &input.tests_script,
        ];
        if scripts
            .iter()
            .filter_map(|script| script.as_deref())
            .any(|script| script_mentions(script, &needle))
        {
            return true;
        }
        // Folder scripts run, and folder headers and auth are sent, with this
        // request too. A chain that cannot be read is not checked here, because
        // `begin_phases` then fails the send with an error naming the folder.
        if let Ok(chain) = self.folder_chain(input) {
            let with_scripts = !input.skip_folder_scripts;
            if chain
                .iter()
                .any(|folder| folder_mentions(folder, &needle, with_scripts))
            {
                return true;
            }
        }
```

The comment above the block (`A full-line // comment in a script is not a use, ...`) and the rest of the method stay unchanged.

- [ ] **Step 5: Run the tests and watch them pass**

Run: `cargo test -j4 -p rocket-app secret_scan_tests`

Expected: PASS, 5 tests.

Run: `cargo test -j4 -p rocket-app execute_ignores_a_commented_out_secret_reference_in_a_script`

Expected: PASS (the request-script scan still works through `script_mentions`).

Run: `cargo test -j4 -p rocket-app script_chain`

Expected: PASS, 24 tests.

- [ ] **Step 6: Document the chain in the crate guide**

In `crates/rocket-app/CLAUDE.md`, under `## Key Patterns`, after the **Phase-callable execution** bullet, add:

```markdown
- **Folder script chain.** `begin_phases` reads the folder chain once (`folder_chain`, which calls `get_folder_chain_settings`) and stores `PhaseScripts` on `PhaseState`, ordered by `rocket_collection::chain_scripts` and `CollectionSettings.script_flow` (`execution_service/script_chain.rs`). Each script is its own engine run, built from the current `state.var_ctx` and `state.http_request`, so later scripts see earlier mutations and runtime variables. Sandbox mode, file scope and vault hold-back are the request script's. The first error wins and ends its phase; a folder script's error reads `Folder "<path>" <phase> script: <message>`. In a run, `skipRequest()` ends the pre-request chain; a single send ignores it. Inline Flow requests (`__flow_inline__/`) and GraphQL introspection (`skip_folder_scripts`) run no folder scripts. `references_alias` also scans folder scripts, headers and auth. Test results carry no folder label.
```

- [ ] **Step 7: Final checks**

Run: `cargo test -j4 -p rocket-app`

Expected: PASS.

Run: `cargo check -j4 -p rocket` (the `src-tauri` package)

Expected: success.

Run: `cargo clippy -j4 -p rocket-app --tests`

Expected: no new warning in `script_chain.rs` or in the changed parts of `execution_service.rs`.

- [ ] **Step 8: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only:

```bash
git add crates/rocket-app/src/execution_service.rs crates/rocket-app/src/execution_service/script_chain.rs crates/rocket-app/CLAUDE.md
```

Commit with the same pathspec and a conventional message such as `feat(app): scan folder scripts, headers and auth for failing vault secrets`.

---

## Next Plan

[Plan 07: Folder variable access from scripts](2026-10-07-folder-settings-plan-07-folder-var-script-api.md). It depends on this plan (folder scripts run through `chain_scripts` with the shared `PhaseState.var_ctx`) and on plan 02. Its `input()` helper in `folder_var_script_tests.rs` builds an `ExecuteRequestInput` literal, which needs the `skip_folder_scripts: false` field this plan adds. Chain to it automatically when this one finishes.
