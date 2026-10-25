# OpenAPI Import Command and Dialog Implementation Plan (Plan 3 of the OpenAPI series)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A user can import an OpenAPI spec from a local file or a pasted link in the Import dialog, choosing tag or path folders, and ends with a new collection (plus a source record when the spec came from a link).

**Architecture:** Two new `ImportService` methods in `rocket-import` do the work: read a file, or fetch through the `SpecFetcher` port, then call `import_openapi_text` and write the source record. Two thin Tauri commands call them. The dialog gets an "OpenAPI" tab. Sync (token, schedule, review) is not part of this plan.

**Tech Stack:** Rust (`rocket-import`, `rocket-http` port, `src-tauri`), React + TypeScript, shadcn/ui, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-25-openapi-import-and-sync-design.md`. Series index: `00-plan-index.md`.

## Global Constraints

- Link rules (from Plan 2, enforced by `parse_spec_url` and `SpecFetcher`): https only, at most 5 redirects, 30 s timeout, body capped at 25 MB (`DEFAULT_MAX_SPEC_BYTES`). A pasted GitHub `blob`, `raw`, `raw.githubusercontent.com` or contents-API link works.
- No token in this plan. A private repository fails with a message that says private repositories are not supported yet.
- The source record never holds a token or any secret. It is written only for link imports, because `SpecLocation` has only `Github` and `Https`.
- `_workspace_id` is ignored by `ImportService`, as for WSDL. Writes go to the active workspace path.
- Rust: never unwrap errors in production paths, camelCase renames on IPC DTOs only, `-j4` on every cargo command, never `--workspace`.
- Frontend: shadcn/ui primitives (`Button`, `Input`, `Label`, `RadioGroup`) and lucide icons only. No raw `<button>` or `<input>` in new code. Narrow Zustand selectors.
- Commits go through `dev-workflow-skills:1-git-commit` with explicit paths.
- Tasks that touch `rocket-import`, `rocket-collection` or `rocket-infra` start with: "Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`."

## Review Focus

- A link that returns an HTML page, not a spec: clear error, and no empty collection left behind (Task 1).
- A failed fetch (404, 403, 429, timeout): clear error per cause, no collection folder created (Task 1).
- A private GitHub link with no token: the message says private repositories are not supported yet, not just "not found" (Task 1).
- The source file cannot be written after the collection was created: the import still succeeds and the user is told sync is not set up (Task 1).
- A link pinned to a commit SHA: a warning that the spec will never change (Task 1). A pasted link with surrounding spaces or a newline works (Task 3).

---

### Task 1: `import_openapi_file` and `import_openapi_url` on `ImportService`

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-import/Cargo.toml` (add `rocket-http.workspace = true`; dev-dependency `tokio` with `macros` and `rt`, `async-trait`).
- Modify: `crates/rocket-import/src/error.rs` (add `ImportError::SpecUnavailable(String)`, displayed as its message).
- Modify: `crates/rocket-import/src/openapi/layout.rs` (add `GroupBy::parse`).
- Modify: `crates/rocket-import/src/importer.rs` (add `pub(crate) fn collections_root(&self) -> PathBuf` returning `workspace_path/collections`).
- Create: `crates/rocket-import/src/openapi/entry.rs` (an `impl ImportService` block, the result type, failure messages and tests); declare it in `openapi/mod.rs`.
- Modify: `crates/rocket-import/src/lib.rs` (re-export `OpenApiImportResult`).
- Modify: `crates/rocket-import/CLAUDE.md` (a short "Importing from a file or a link" subsection).

**Interfaces:**
- Consumes: `import_openapi_text(text, source_label, group_by, workspace_id) -> ImportResult<OpenApiImportOutcome>`; `parse_spec_url`, `fetch_target(&SpecLocation, false) -> FetchTarget`; `source_record(...)`; `SpecFetcher::fetch(SpecFetchRequest) -> Result<SpecFetchOutcome, SpecFetchFailure>`; `OpenApiSourceRepository::save(&Path, &OpenApiSource)`.
- Produces:
  - `GroupBy::parse(text: &str) -> Option<GroupBy>`; accepts `"tags"` and `"paths"` only.
  - `pub struct OpenApiImportResult { pub report: ImportReport, pub collection: String, pub source_url: Option<String>, pub warnings: Vec<String> }`.
  - `ImportService::import_openapi_file(&self, path: &Path, group_by: GroupBy, workspace_id: &str) -> ImportResult<OpenApiImportResult>`. `source_url` is `None`, no source file is written.
  - `async ImportService::import_openapi_url(&self, fetcher: &dyn SpecFetcher, sources: &dyn OpenApiSourceRepository, link: &str, group_by: GroupBy, workspace_id: &str) -> ImportResult<OpenApiImportResult>`. `source_url` is `location.display_url()`.
  - `fn describe_fetch_failure(failure: &SpecFetchFailure) -> String` (private, tested through the public methods and directly).

- [ ] **Step 1: Write the failing tests** in `entry.rs` (`#[tokio::test]`, a `FakeFetcher` that returns a canned `Result` and records the last `SpecFetchRequest`, a `FailingSources` repo whose `save` errors, and the real `FsOpenApiSourceRepo` from the dev-dependency `rocket-infra`). Reuse `tests/fixtures/openapi/orders-3.0.json`. Use `ImportService::new_with_workspace_path(tmp)`.
  - `url_import_creates_collection_and_source_file`: https link, fetcher returns the fixture with `etag: Some("\"e1\"")`. Assert the collection folder exists, `result.source_url == Some(link)`, and the loaded source has `SpecLocation::Https`, `group_by == "tags"`, `etag == Some("\"e1\"")`, 3 operations.
  - `github_blob_link_is_fetched_from_the_raw_host_without_a_token`: assert the recorded request URL starts with `https://raw.githubusercontent.com/`, `bearer_token` is `None`, and the source location is `SpecLocation::Github`.
  - `failed_fetch_creates_no_collection`: fetcher returns `Err(SpecFetchFailure::NotFound)`. Assert `Err(ImportError::SpecUnavailable(_))` and that `tmp/collections` has no entries.
  - `html_body_creates_no_collection`: body `<html>…</html>`. Assert `Err(ImportError::NotAnOpenApiSpec(_))` and no collection folder.
  - `source_file_failure_does_not_fail_the_import`: use `FailingSources`. Assert `Ok`, the collection exists, and `warnings` has one entry containing `"sync"`.
  - `commit_sha_ref_warns_that_nothing_will_change`: link `https://github.com/o/r/blob/0123456789abcdef0123456789abcdef01234567/api.yaml`. Assert one warning containing `"commit"`. A branch ref (`main`) gives none.
  - `file_import_writes_no_source_file`: write the fixture to `tmp/orders.json`, call `import_openapi_file`. Assert `source_url == None` and `<collection>/.rocket/openapi-source.yml` does not exist.
  - `file_over_the_size_cap_is_rejected`: a file of `DEFAULT_MAX_SPEC_BYTES + 1` bytes gives `Err(ImportError::SpecUnavailable(_))` without reading it into memory first (check the length with `metadata`).
  - `failure_messages_name_the_cause`: `Unauthorized` and `NotFound` messages both contain `"private"` and `"not supported yet"`; `RateLimited { retry_after_secs: Some(60) }` contains `"60"`; `TooLarge` contains `"25 MB"`; `NotHttps` contains `"https"`.
  - `group_by_parse_accepts_only_the_two_layouts`: `"tags"`, `"paths"` parse; `"Tags"`, `""`, `"tag"` give `None`.
- [ ] **Step 2: Run to verify they fail.** Run: `cargo test -j4 -p rocket-import openapi::entry`. Expected: compile errors for the missing items.
- [ ] **Step 3: Implement** the items in the Interfaces block. URL flow: `parse_spec_url` → `fetch_target(&location, false)` → fetch with `max_bytes: DEFAULT_MAX_SPEC_BYTES`, no validators, no token → `NotModified` is an error (`SpecUnavailable`) → `String::from_utf8` (invalid is `NotAnOpenApiSpec`) → `import_openapi_text(text, source_label, ...)` with the last path segment of the URL as the label → `source_record(..., body, etag, last_modified, &outcome)` → `sources.save(&collections_root().join(&outcome.collection), &record)`; a save error becomes a warning ("Sync could not be set up: …"), never an error. Commit-SHA test: ref is 7 to 40 hex characters only.
- [ ] **Step 4: Run to verify they pass.** Run: `cargo test -j4 -p rocket-import openapi::entry` then `cargo check -j4`. Expected: all pass, no warnings.
- [ ] **Step 5: Document and commit.** Add the CLAUDE.md subsection. Use `dev-workflow-skills:1-git-commit` with the changed paths. Message: `feat(import): import an OpenAPI spec from a file or a link`.

---

### Task 2: Tauri commands and the TypeScript API

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src-tauri/src/commands/import.rs` (two commands, one IPC DTO, `parse_group_by`).
- Modify: `src-tauri/src/lib.rs` (build `ReqwestSpecFetcher::new().with_proxy(Arc::clone(&shared_proxy))` next to `shared_proxy`, `.manage(Arc<dyn rocket_http::SpecFetcher>)`, register both commands in `generate_handler!`).
- Modify: `src/lib/tauri-api.ts` (types and two functions next to `importWsdl`).
- Test: `src/lib/__tests__/tauri-api.openapi.test.ts` (follow the closest existing `tauri-api` test for the `invoke` mock; create it if none).
- Modify: the IPC doc that lists import commands, if one exists (check with grep; skip if none).

**Interfaces:**
- Consumes: Task 1 methods, `FsOpenApiSourceRepo`, the managed `Arc<Mutex<PathBuf>>` workspace path.
- Produces (Rust):
  - `import_openapi_file(path: String, group_by: String, target_workspace_id: String, workspace_path: State<..>) -> Result<OpenApiImportDto, String>`
  - `import_openapi_url(link: String, group_by: String, target_workspace_id: String, workspace_path: State<..>, fetcher: State<'_, Arc<dyn SpecFetcher>>) -> Result<OpenApiImportDto, String>`
  - `#[serde(rename_all = "camelCase")] struct OpenApiImportDto { report: ImportReport, collection: String, source_url: Option<String>, warnings: Vec<String> }`
  - `fn parse_group_by(text: &str) -> Result<GroupBy, String>`; the error is `"folder layout must be tags or paths"`.
- Produces (TS):
  - `export type OpenApiGroupBy = 'tags' | 'paths';`
  - `export interface OpenApiImportResult { report: ImportReport; collection: string; sourceUrl: string | null; warnings: string[] }`
  - `importOpenApiFile(path: string, groupBy: OpenApiGroupBy, targetWorkspaceId: string): Promise<OpenApiImportResult>` invoking `import_openapi_file` with `{ path, groupBy, targetWorkspaceId }`.
  - `importOpenApiUrl(link: string, groupBy: OpenApiGroupBy, targetWorkspaceId: string): Promise<OpenApiImportResult>` invoking `import_openapi_url` with `{ link, groupBy, targetWorkspaceId }`.

- [ ] **Step 1: Write the failing tests.** Rust, in `import.rs`: `parse_group_by_accepts_tags_and_paths_only` (`"tags"`, `"paths"` ok; `"folders"` is the error text above). TS: `importOpenApiUrl` calls `invoke('import_openapi_url', { link, groupBy: 'paths', targetWorkspaceId: 'ws1' })` and returns its value; same for `importOpenApiFile` with `path`.
- [ ] **Step 2: Run to verify they fail.** Run: `cargo test -j4 -p <src-tauri package name> parse_group_by` (read the name from `src-tauri/Cargo.toml`) and `yarn test tauri-api.openapi`. Expected: FAIL, items not defined.
- [ ] **Step 3: Implement** the commands as thin wrappers: lock the workspace path, build the service with the existing `make_import_service`, call the Task 1 method with `FsOpenApiSourceRepo`, map the result into the DTO and errors with `to_string()`. Wire the fetcher and handlers in `lib.rs`. Add the TS types and functions.
- [ ] **Step 4: Run to verify they pass.** Run: `cargo check -j4`, the Rust test above, `yarn tsc --noEmit`, `yarn test tauri-api.openapi`, `yarn check`. Expected: all pass.
- [ ] **Step 5: Commit** with `dev-workflow-skills:1-git-commit`. Message: `feat(import): add OpenAPI import commands and API`.

---

### Task 3: OpenAPI tab in the Import dialog

**Files:**
- Modify: `src/components/import/ImportCollectionDialog.tsx`
- Modify: `src/components/import/importSources.ts` (add `'openapi-file'` to `SourceKind`; `describeSource('openapi-file')` is `'OpenAPI file'`)
- Test: `src/components/import/ImportCollectionDialog.test.tsx` (extend the `@/lib/tauri-api` mock with `importOpenApiFile` and `importOpenApiUrl`)

**Interfaces:**
- Consumes: `importOpenApiFile`, `importOpenApiUrl`, `OpenApiGroupBy`, `OpenApiImportResult` from Task 2.
- Produces: no new exports. `ImportSource` gains `'openapi'`.

Behavior:
- A fourth button "OpenAPI" next to Bruno, Postman and WSDL. It is not shown when `createWorkspace` is true, because `ImportService` ignores the workspace id and the collection would land in the wrong workspace.
- Description: "Import an OpenAPI 3.0, 3.1 or Swagger 2.0 spec (JSON or YAML) from a link or a file. GitHub file links work. Private repositories are not supported yet."
- Source switch "Link" (default) or "File". Link: an `Input` (placeholder `https://github.com/owner/repo/blob/main/openapi.yaml`). File: the existing picker pattern with filters `json`, `yaml`, `yml`.
- Layout: a `RadioGroup` labelled "Group folders by" with "Tag" (default, value `tags`) and "Path" (value `paths`).
- Import is disabled until a link is typed or a file is chosen. The link is trimmed before sending. On failure the error text shows under the form, the dialog returns to the picking state and the typed link is kept.
- The done screen reuses the existing summary and adds the `warnings` as an amber list.

- [ ] **Step 1: Write the failing tests** (Vitest, `userEvent`):
  - `offers an OpenAPI option` and `hides the OpenAPI option when creating a workspace` (render with `createWorkspace`).
  - `disables Import until a link is typed`.
  - `imports a pasted link with the default tag layout`: type `  https://example.com/o.json\n`, click Import, expect `importOpenApiUrl` called with `('https://example.com/o.json', 'tags', 'ws1')`.
  - `sends the path layout when Path is chosen`.
  - `imports a chosen file`: mock the picker to return `/tmp/o.yaml`, expect `importOpenApiFile('/tmp/o.yaml', 'tags', 'ws1')`.
  - `shows the error and keeps the link when the import fails`: reject with `new Error('private repositories are not supported yet')`; expect the text visible and the input still holding the link.
  - `lists warnings on the done screen`: resolve with `warnings: ['This link points at a commit']`; expect the text.
- [ ] **Step 2: Run to verify they fail.** Run: `yarn test ImportCollectionDialog`. Expected: the new tests FAIL.
- [ ] **Step 3: Implement** the tab, state (`openapiMode`, `link`, `groupBy`, `warnings`) and the `handleImport` branch for `importSource === 'openapi'`. Reset the new state in `handleClose` and `switchImportSource`. New controls use shadcn primitives only.
- [ ] **Step 4: Run to verify they pass.** Run: `yarn test ImportCollectionDialog`, `yarn tsc --noEmit`, `yarn check`. Expected: all pass.
- [ ] **Step 5: Commit** with `dev-workflow-skills:1-git-commit`. Message: `feat(import): add an OpenAPI tab to the import dialog`. Then update `00-plan-index.md` (P3 row done, hashes) and `docs/roadmaps/roadmap.md` R-69 (import shipped; sync is R-70).

---

## Self-review notes

- Spec coverage: file and URL import, GitHub links, layout chosen at import, provenance written at import, commit-SHA warning, failure states with a reason. Token, schedule and review are Plans 4 to 9.
- Deviation from the index: its "TS api and query" task has no query. The dialog already calls the API directly and refreshes through `onImportComplete`, so the API functions are folded into Task 2.
- Not covered here (deferred): token for private repositories (Plan 4), a first-URL private-IP check, blocking file writes on a large import (about 45 s at 5,000 operations, runs on an async worker, not the UI thread).
