# .env Status and Picker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The `.env file` row in Manage Environments shows whether the file was found and how many keys it holds, and a button picks the file with the system dialog.

**Architecture:** The loader gets a `describe` method that reports the resolved path, existence and key count without returning values. `RequestExecutionService` exposes `dot_env_status`, a thin Tauri command wraps it, and the dialog reads it through a TanStack Query hook. The picker uses `@tauri-apps/plugin-dialog` and a pure helper that stores a path relative to the base when it is inside it.

**Tech Stack:** Rust, Tauri IPC, React, TypeScript, shadcn/ui, lucide-react, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-18-env-extends-dotenv-design.md`, amended by [index.md](index.md). Depends on plan 1 being merged.

## Global Constraints

- The status never returns `.env` values, only path, existence and a key count.
- IPC DTOs use `#[serde(rename_all = "camelCase")]`. Commands stay thin (`.claude/rules/tauri-ipc-boundaries.md`).
- UI: shadcn primitives (`Button`, `Input`), `lucide-react` icons, no raw `<button>`. Zustand: narrow selectors.
- `yarn tsc --noEmit`, `yarn check`, `cargo check -j4` pass before each commit. Commit through `dev-workflow-skills:1-git-commit`.

## Review Focus

- Gate denied: the status says "not read" and does not touch the file.
- A relative path with `..`: the status shows the refusal text, not a stack trace.
- The picked file is outside the base folder: stored absolute. A Windows path with `\` separators is handled.
- The picked path equals the base folder prefix of a sibling (`/col2/.env` vs base `/col`): not treated as inside.
- The user clears the box: the status falls back to the default file.

---

### Task 1: Describe a .env file and the status command

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-environment/src/dot_env.rs` (trait and DTO type)
- Modify: `crates/rocket-infra/src/fs_dot_env_loader.rs`
- Modify: `crates/rocket-app/src/execution_service.rs`
- Modify: `src-tauri/src/commands/environments.rs`, `src-tauri/src/lib.rs` (register the command)
- Test: the `tests` modules of the three crates and the commands file

**Interfaces:**
- Consumes: `resolve_dot_env_path`, `load`, `load_default`, `process_env_allowed`, `pinned_collection_dir`.
- Produces:
  - `pub struct DotEnvFileInfo { pub path: Option<String>, pub exists: bool, pub key_count: usize, pub error: Option<String> }` and `fn describe(&self, raw_path: Option<&str>, base: Option<&Path>) -> DotEnvFileInfo` on `DotEnvLoader` (`None` raw path means the default `<base>/.env`).
  - `RequestExecutionService::dot_env_status(&self, collection: Option<&str>, environment: Option<&str>) -> DotEnvStatus` where `DotEnvStatus { base_dir: Option<String>, info: DotEnvFileInfo, blocked: bool, uses_default: bool }`. `blocked` is true when the gate denies, and then the file is not touched.
  - Tauri command `get_dot_env_status(collection: Option<String>, environment: Option<String>) -> DotEnvStatusDto` (camelCase: `baseDir`, `path`, `exists`, `keyCount`, `error`, `blocked`, `usesDefault`).

- [ ] **Step 1: Write the failing tests:**
  - infra: `describe_reports_path_exists_and_key_count`, `describe_of_a_missing_file_is_not_an_error_and_exists_false`, `describe_of_a_dotdot_path_reports_the_refusal`, `describe_never_contains_values` (the struct has no value field; assert key count only).
  - app: `dot_env_status_is_blocked_when_the_gate_denies_and_reads_nothing`, `dot_env_status_uses_the_default_when_no_path_is_set`, `dot_env_status_uses_the_explicit_path_of_the_environment`.
  - commands: `status_dto_serialises_in_camel_case`.

- [ ] **Step 2: Run to verify they fail:** `cargo test -j4 -p rocket-infra describe_`, `cargo test -j4 -p rocket-app dot_env_status`, `cargo test -j4 -p rocket-tauri status_dto` (use the real package name of `src-tauri` from its `Cargo.toml`). Expected: FAIL.

- [ ] **Step 3: Implement** `describe` in `FsDotEnvLoader` (share the parse with `load`; count keys, drop values), `dot_env_status` in the service (resolve the base like `process_env_for` does: collection folder, or the workspace base when `collection` is `None`; pick the environment's `dot_env_file_path`), and the thin command plus its DTO and registration in `lib.rs`.

- [ ] **Step 4: Run to verify they pass:** the commands above plus `cargo check -j4`. Expected: PASS.

- [ ] **Step 5: Commit** `feat(env): add a status command for the .env file`.

---

### Task 2: Status line in the dialog

**Files:**
- Modify: `src/lib/tauri-api.ts` (next to `getProcessEnvVars`, line 1684)
- Modify: `src/lib/queries/environment-queries.ts`
- Modify: `src/components/environments/EnvironmentDialog.tsx` (the `.env file` row, lines 541-553)
- Test: `src/components/environments/EnvironmentDialog.test.tsx`

**Interfaces:**
- Consumes: Tauri command `get_dot_env_status` from Task 1.
- Produces: `interface DotEnvStatus { baseDir: string | null; path: string | null; exists: boolean; keyCount: number; error: string | null; blocked: boolean; usesDefault: boolean }`, `getDotEnvStatus(collection: string | null, environment: string | null): Promise<DotEnvStatus>` in `tauri-api.ts`, and `useDotEnvStatus(collection: string | null, environment: string | null, dotEnvFilePath: string | undefined)` in `environment-queries.ts` (the path is part of the query key so editing it refreshes the status).

- [ ] **Step 1: Write the failing tests** in `EnvironmentDialog.test.tsx` (mock `getDotEnvStatus`):
  - `shows_found_with_the_key_count` (text `Found .env, 4 keys`).
  - `shows_not_found_for_a_missing_explicit_file` (text starts with `Not found`).
  - `shows_nothing_extra_for_a_missing_default_file` (default and missing: text `No .env file in this folder`).
  - `shows_not_read_when_blocked` (text mentions host environment access).
  - `shows_the_error_text_in_amber`.

- [ ] **Step 2: Run to verify they fail:** `yarn test EnvironmentDialog`. Expected: FAIL.

- [ ] **Step 3: Implement** the API function and hook, then render one muted `<p>` under the `.env file` row. An amber `AlertTriangle` (as the existing warnings do) for `error` and `blocked`. Never render values.

- [ ] **Step 4: Run to verify they pass:** `yarn test EnvironmentDialog`, `yarn tsc --noEmit`, `yarn check`. Expected: PASS.

- [ ] **Step 5: Commit** `feat(env): show the .env file status in the environment dialog`.

---

### Task 3: File picker

**Files:**
- Create: `src/lib/dot-env-path.ts`
- Modify: `src/components/environments/EnvironmentDialog.tsx`
- Test: `src/lib/__tests__/dot-env-path.test.ts`, `src/components/environments/EnvironmentDialog.test.tsx`

**Interfaces:**
- Consumes: `DotEnvStatus.baseDir` from Task 2, `open` from `@tauri-apps/plugin-dialog` (import as in `ImportCollectionDialog.tsx`).
- Produces: `toStoredDotEnvPath(picked: string, baseDir: string | null): string` in `src/lib/dot-env-path.ts`.

- [ ] **Step 1: Write the failing tests.** In `dot-env-path.test.ts`:
  - `a_file_inside_the_base_is_stored_relative` (`('/col/config/dev.env','/col')` gives `config/dev.env`).
  - `a_file_directly_in_the_base_is_stored_as_its_name`.
  - `a_file_outside_the_base_is_stored_absolute`.
  - `a_sibling_folder_with_the_same_prefix_is_outside` (`('/col2/.env','/col')` stays absolute).
  - `windows_separators_are_handled` (`('C:\\col\\a\\.env','C:\\col')` gives `a/.env`).
  - `a_null_base_keeps_the_path_as_picked`.

  In the dialog test: `the_pick_button_sets_the_stored_path` (mock `open` to resolve `'/col/.env.prod'`, expect `updateEnvField` result `.env.prod`) and `cancelling_the_picker_changes_nothing` (mock resolves `null`).

- [ ] **Step 2: Run to verify they fail:** `yarn test dot-env-path EnvironmentDialog`. Expected: FAIL.

- [ ] **Step 3: Implement** `toStoredDotEnvPath` (normalise `\` to `/` for the comparison, require the base plus a separator as prefix). Add a `Button` with the lucide `FolderOpen` icon and `aria-label='Pick .env file'` beside the input. It calls `open({ multiple: false, directory: false, defaultPath: baseDir ?? undefined })` and sets `dotEnvFilePath` to `toStoredDotEnvPath(result, baseDir)`.

- [ ] **Step 4: Run to verify they pass:** the command above, `yarn tsc --noEmit`, `yarn check`. Expected: PASS.

- [ ] **Step 5: Manual check** with `yarn tauri dev`: pick a `.env` inside and outside a collection folder, confirm the status line and that `{{process.env.NAME}}` resolves in a request. Record the result in `.claude/env-extends-dotenv-spec-notes.md`.

- [ ] **Step 6: Commit** `feat(env): add a file picker for the .env path`.
