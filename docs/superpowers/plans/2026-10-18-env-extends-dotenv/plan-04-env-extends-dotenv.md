# Environment dialog UI and docs Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** A user sets "Inherits from" and the `.env` path in the environment dialog, sees inherited rows read-only with their source, and the docs describe the real behavior.

**Architecture:** The dialog keeps editing the raw environment. A pure helper lists the valid parents. Inherited rows come from the resolved query of plan 3 and are rendered read-only below the editable rows. The global environments tab reuses the same pieces.

**Tech Stack:** React, TypeScript, shadcn/ui, lucide-react, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-18-env-extends-dotenv-design.md`

## Global Constraints

- Use shadcn/ui primitives and lucide-react icons only. No raw `<button>`, `<input>`, `<select>`, `<dialog>`.
- A single-line variable-aware field is `SingleLineEditor`. The `.env` path is a plain path, so it is a shadcn `Input`.
- Zustand: never fully destructure store state at component top level.
- Saving never writes inherited rows into the child. The "Inherits from" picker never offers the environment itself or any environment that would create a cycle.
- A missing parent shows a warning and does not block the dialog or the save.
- Documentation goes under `.claude` or the crate's `CLAUDE.md`, and the project `CLAUDE.md` stays small.
- Verify with `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern>`.

## Post-Implementation Review

Run after Task 3, on the finished feature, as the last review of the series. Do the app checks with `yarn tauri dev`.

- [ ] `yarn test`, `yarn tsc --noEmit` and `yarn check` pass. Evidence: the command output.
- [ ] In the environment dialog, "Inherits from" lists the other environments, never the one being edited or one of its descendants. Evidence: the picker contents for a three-level chain.
- [ ] Choosing a parent shows its variables as read-only rows with the source name, and saving does not copy them into the child's file. Evidence: the dialog and the saved environment file.
- [ ] A secret inherited from the parent shows masked, and the reveal toggle rules for secrets still apply. Evidence: the row in the dialog.
- [ ] Setting the `.env` path and sending a request that uses `{{process.env.NAME}}` works, and denying the host environment gate in the trust settings stops it. Evidence: the send result in both states.
- [ ] The same controls work on a global environment. Evidence: the global environments tab.
- [ ] The docs are accurate: `crates/rocket-environment/CLAUDE.md` no longer claims upstream handling, and `.claude/roadmap.md` lists the two follow-ups. Evidence: the diff of those files.
- [ ] Every acceptance criterion of the spec has evidence from this review or an earlier plan's review. Evidence: a one-line mapping, written into the ledger.

## Review Focus

- A parent that was deleted after the child was saved: the picker shows the stored name with a warning and the save keeps it (Task 1).
- An environment with no parent candidates (the only environment): the picker is disabled, not empty and open (Task 1).
- A child that overrides every parent variable: the inherited section shows no rows, not a stale list (Task 2).
- Inherited secret rows: shown masked, never the secret value (Task 2).
- A global environment's `.env` path field and picker behave the same as in a collection (Task 3).

## File Structure

- Create `src/components/environments/extends-candidates.ts`: `extendsCandidates`.
- Modify `src/components/environments/EnvironmentDialog.tsx` (picker, path field), `src/components/environments/VariableTable.tsx` (inherited rows).
- Modify `src/components/workspace/WorkspaceEnvironmentsTab.tsx` (same controls for global environments).
- Modify `crates/rocket-environment/CLAUDE.md`, `.claude/collection-trust.md`, `.claude/roadmap.md`, `docs/superpowers/specs/opencollection-spec-reference.md` (a short note under the Environment type).

---

### Task 1: The picker and the path field

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `src/components/environments/extends-candidates.ts`
- Modify: `src/components/environments/EnvironmentDialog.tsx`
- Test: `src/components/environments/extends-candidates.test.ts`, `src/components/environments/EnvironmentDialog.test.tsx`

**Interfaces:**
- Consumes: `Environment` (`name`, `extends`, `dotEnvFilePath`) from `src/lib/tauri-api.ts`.
- Produces: `extendsCandidates(envs: Environment[], currentName: string): string[]` (names that are not the environment, not a descendant of it, sorted).

- [ ] **Step 1: Write the failing tests**
  - `extends_candidates_exclude_the_environment_itself`.
  - `extends_candidates_exclude_a_descendant_that_would_create_a_cycle`.
  - `extends_candidates_keep_unrelated_environments_sorted`.
  - `dialog_saves_the_chosen_parent_in_extends`, `dialog_saves_the_env_path_in_dotEnvFilePath`.
  - `dialog_clears_extends_when_no_parent_is_chosen`.
  - `dialog_keeps_a_deleted_parent_name_and_shows_a_warning`.
  - `picker_is_disabled_when_there_are_no_candidates`.
  - The existing `saves clientCertificates, extends, dotEnvFilePath, color and description unchanged` test still passes.

- [ ] **Step 2: Run to verify they fail:** `yarn test extends-candidates EnvironmentDialog`.

- [ ] **Step 3: Implement** the helper and the two controls (a shadcn `Select` labelled "Inherits from" with a "None" choice, and an `Input` labelled ".env file path"), wired into the dialog's existing draft and save state.

- [ ] **Step 4: Run to verify they pass:** the command above, `yarn tsc --noEmit`, `yarn check`. Expected: PASS.

---

### Task 2: Inherited rows, read-only with their source

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src/components/environments/VariableTable.tsx`, `src/components/environments/EnvironmentDialog.tsx`
- Test: `src/components/environments/VariableTable.test.tsx`, `src/components/environments/EnvironmentDialog.test.tsx`

**Interfaces:**
- Consumes: `useResolvedEnvironment(collection, name)` and `ResolvedVariable { key, value, enabled, secret, source }` from plan 3.
- Produces: `VariableTable` gains an optional prop `inheritedRows?: ResolvedVariable[]` rendered after the editable rows, read-only.

- [ ] **Step 1: Write the failing tests**
  - `inherited_rows_show_the_source_environment_name`.
  - `inherited_rows_are_read_only`.
  - `a_variable_overridden_by_the_child_is_not_listed_as_inherited`.
  - `an_inherited_secret_row_is_masked`.
  - `saving_does_not_include_inherited_rows`.
  - `a_missing_parent_shows_a_warning_and_save_still_works`.

- [ ] **Step 2: Run to verify they fail:** `yarn test VariableTable EnvironmentDialog`.

- [ ] **Step 3: Implement.** Filter resolved rows to those whose `source` differs from the edited environment's name. Pass the resulting rows to `VariableTable`. Render the resolved `warnings` in the dialog with a lucide icon.

- [ ] **Step 4: Run to verify they pass:** the command above, `yarn tsc --noEmit`, `yarn check`. Expected: PASS.

---

### Task 3: Global environments and documentation

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src/components/workspace/WorkspaceEnvironmentsTab.tsx`
- Modify: `crates/rocket-environment/CLAUDE.md`, `.claude/collection-trust.md`, `.claude/roadmap.md`, `docs/superpowers/specs/opencollection-spec-reference.md`
- Test: `src/components/workspace/WorkspaceEnvironmentsTab.test.tsx`

**Interfaces:**
- Consumes: `extendsCandidates`, `useResolvedEnvironment(null, name)`, the dialog controls of Tasks 1 and 2.
- Produces: no new public names.

- [ ] **Step 1: Write the failing tests**
  - `global_environments_offer_the_picker_and_the_path_field`.
  - `a_global_environment_lists_only_global_environments_as_candidates`.
  - `global_inherited_rows_show_their_source`.

- [ ] **Step 2: Run to verify they fail:** `yarn test WorkspaceEnvironmentsTab`.

- [ ] **Step 3: Implement the global tab changes, then the docs.**
  - `crates/rocket-environment/CLAUDE.md`: replace the false line about `extends` and `dot_env_file_path` with the real behavior (`flatten.rs`, `dot_env.rs`, what is and is not inherited) and add both modules to the module map.
  - `.claude/collection-trust.md`: add that `.env` files are read only when the host environment gate allows, that the OS environment wins, and that `.env` values are masked in the Console.
  - `.claude/roadmap.md`: record the shipped work and the two follow-ups from the spec.
  - `docs/superpowers/specs/opencollection-spec-reference.md`: one note under the Environment type that `extends` and `dotEnvFilePath` are acted on, with the same-store and process.env rules.

- [ ] **Step 4: Run to verify they pass:** `yarn test WorkspaceEnvironmentsTab`, `yarn tsc --noEmit`, `yarn check`. Expected: PASS.

Next plan: none
