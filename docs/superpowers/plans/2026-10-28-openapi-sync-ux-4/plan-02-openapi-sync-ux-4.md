# OpenAPI sync UX 4: Local file source Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A collection can sync from a spec file on disk, like Bruno's "Upload file", with consent, change detection by hash and the same review flow.

**Architecture:** A `SpecLocation::File { path }` variant (path relative to the collection root when possible) and a `LocalSpecReader` trait with a filesystem impl. `check`, Connect, Change link and the import command read through one `SpecSources` pair (fetcher plus reader) so the rest of the flow (bundle, pending, plan, apply) is shared.

**Tech Stack:** Rust (`rocket-collection`, `rocket-http`, `rocket-infra`, `rocket-import`, `rocket-app`, `src-tauri`).

**Spec:** `docs/superpowers/specs/2026-10-28-openapi-sync-ux-3-design.md` (section 7)

## File Structure

| File | Change |
|---|---|
| `crates/rocket-collection/src/openapi_source.rs` | `SpecLocation::File`, version 2 on write, `display_url`, `ref_is_immutable` |
| `crates/rocket-http/src/spec_fetch.rs` | `LocalSpecReader` trait, `LocalReadFailure` |
| `crates/rocket-infra/src/{fs_local_spec_reader.rs (new),lib.rs}` | `FsLocalSpecReader` |
| `crates/rocket-import/src/openapi/{source_url.rs,entry.rs}` | `parse_spec_target`, `fetch_spec_bundle` routing, `import_openapi_file` linked mode |
| `crates/rocket-app/src/{openapi_sync_service.rs,openapi_connect.rs,openapi_change_link.rs}` | `check` branch, `safe_message("file_missing")`, token refusal |
| `src-tauri/src/commands/{import.rs,openapi_sync.rs}`, `src-tauri/src/lib.rs` | Wire the reader; `import_openapi_file(.., keep_synced)` |

## Review Focus

1. A source written for a File location must be refused by version-1 readers, and other sources must stay version 1. Pinned by `a_file_source_is_written_as_version_2` and `a_github_source_stays_version_1`.
2. A cloned collection must not read a file before consent. Pinned by `no_read_without_consent`.
3. A path that escapes via symlink to a non-regular file, a directory, a device or a file over 25 MB must fail with a fixed sentence and leak no path. Pinned by `unreadable_files_give_fixed_sentences`.
4. A missing file must not wipe the pending copy or count as a spec change. Pinned by `a_missing_file_changes_nothing`.
5. Unchanged content must not create a pending update even if the mtime changed. Pinned by `same_bytes_are_not_an_update`.
6. Disconnect and Change link must not touch the file itself. Pinned by `disconnect_never_writes_the_spec_file`.

---

### Task 1: Location variant, reader and parser

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Interfaces:**
- Consumes: `SpecLocation` (`openapi_source.rs:160`), `SOURCE_FILE_VERSION`, `parse_spec_url`, `read_capped` (`entry.rs:87`).
- Produces: `SpecLocation::File { path: String }` (serde kind `file`); `OpenApiSource` is saved with `version: 2` only when the location is `File` (reading accepts 1 and 2 and refuses higher); `display_url()` gives `file://<path as stored>` and `SpecLocation::resolve_file(&self, root: &Path) -> Option<PathBuf>`; `parse_spec_target(input: &str, root: &Path) -> ImportResult<SpecLocation>` (https/localhost via `parse_spec_url`, or `file://` or an absolute or relative file path, stored relative to `root` when it can be); trait `LocalSpecReader { fn read(&self, path: &Path, max_bytes: usize) -> Result<Vec<u8>, LocalReadFailure> }` with `LocalReadFailure::{NotFound, NotAFile, TooLarge, Io}`; `FsLocalSpecReader` (regular file only, follows no directories, 25 MB cap).

- [ ] **Step 1: Read** `openapi_source.rs` (save/load version handling), `source_url.rs`, `reqwest_spec_fetcher.rs` for the trait style.
- [ ] **Step 2: Write the failing tests:** `a_file_source_is_written_as_version_2`, `a_github_source_stays_version_1`, `a_version_3_source_is_refused`, `file_location_round_trips_through_yaml`, `parse_spec_target_stores_a_relative_path_inside_or_beside_the_collection`, `parse_spec_target_keeps_an_absolute_path_when_no_relative_form_exists`, `parse_spec_target_refuses_other_schemes` (`ftp://`, `javascript:`), `resolve_file_joins_the_collection_root`, `fs_reader_reads_a_regular_file`, `unreadable_files_give_fixed_sentences` (directory, missing, over cap).
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-collection openapi_source`.
- [ ] **Step 4: Implement** the variant (update every exhaustive `match` the compiler reports with `cargo check -j4`), the parser and the reader.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-collection`, `-p rocket-import openapi`, `-p rocket-infra fs_local_spec_reader`. Expected: green.

### Task 2: check, Connect and Change link read through the pair

**Interfaces:**
- Consumes: Task 1; `fetch_spec_bundle`, `check`, `consent_state_of`, `grant_consent_for_link` (parse with `parse_spec_target`), `set_token`.
- Produces: `SpecSources { fetcher: Arc<dyn SpecFetcher>, reader: Arc<dyn LocalSpecReader> }` handed to the service; `fetch_spec_bundle(sources, root, link, token)` routes File locations to the reader (no etag, no remote refs; relative `$ref`s stay unresolved with the existing note); `check` for a File source reads, hashes and applies the normal pending rules with no backoff or validators; `safe_message("file_missing")` = "The spec file was not found on this computer."; a token on a File link gives the existing Github-only refusal; consent keys on the JSON of the stored location.

- [ ] **Step 1: Read** `check`, `gated_source`, the Connect and Change link preview paths.
- [ ] **Step 2: Write the failing tests** (apply-tests world with a `FakeReader`): `a_file_source_check_finds_a_changed_spec`, `same_bytes_are_not_an_update`, `a_missing_file_changes_nothing`, `no_read_without_consent`, `a_file_check_sends_no_validators_and_has_no_backoff`, `a_token_is_refused_for_a_file_link`, `connect_preview_reads_a_file_link`, `change_link_to_a_file_clears_the_token`, `disconnect_never_writes_the_spec_file`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-app file_source`.
- [ ] **Step 4: Implement** the routing and the branch.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-app openapi`, `cargo check -j4`. Expected: green.

### Task 3: Import from a file keeps the link

**Interfaces:**
- Consumes: `import_openapi_file` (`entry.rs`), `import_openapi_url`'s source/baseline/applied writes, `grant_consent_for_link`, `set_interval`, the Tauri command `import_openapi_file`.
- Produces: `ImportService::import_openapi_file_linked(path, group_by, workspace_id, root_hint)` that also writes the source (relative path), baseline and applied copy, like the URL import; the command gains `keep_synced: Option<bool>` (default false keeps today's behavior), then grants consent for the typed path, enables the default interval, and returns `sourceUrl` as the `file://` display form; failures after import are warnings (`CONSENT_NOT_SAVED`, `CHECKS_NOT_ENABLED`).

- [ ] **Step 1: Read** the URL import path and the command in `import.rs:321`.
- [ ] **Step 2: Write the failing tests:** `a_linked_file_import_writes_source_baseline_and_applied_copy`, `an_unlinked_file_import_still_writes_nothing`, `the_command_grants_consent_when_keep_synced`, `the_command_warns_when_consent_cannot_be_saved`, `a_linked_import_then_a_changed_file_gives_a_pending_update`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-import linked`.
- [ ] **Step 4: Implement** the linked import and the command parameter.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-import openapi`, `cargo test -j4 -p rocket import_openapi_file`, `cargo check -j4`. Expected: green.

Next plan: docs/superpowers/plans/2026-10-28-openapi-sync-ux-4/plan-03-openapi-sync-ux-4.md
