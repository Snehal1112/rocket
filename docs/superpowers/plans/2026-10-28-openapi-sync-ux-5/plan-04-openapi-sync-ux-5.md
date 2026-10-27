# OpenAPI sync UX 5: Review parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Spec updates review matches Bruno's Preserve values and conflict handling: a **Keep my values** switch (on by default), a conflict count with a filter, and a confirm that summarizes what will happen.

**Architecture:** Frontend only. `buildSelection` and `useSyncSelection` learn a `keepMine` option; the actions bar shows the switch and the conflict count; the confirm dialog lists counts.

**Tech Stack:** React + TypeScript, shadcn/ui, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-28-openapi-sync-ux-3-design.md` (section 8; decision "Keep my values"); parity table section 3.

## File Structure

| File | Change |
|---|---|
| `src/components/collections/openapi-sync/{buildSelection.ts,useSyncSelection.ts}` | `keepMine` option |
| `src/components/collections/openapi-sync/{OpenApiSyncActionsBar.tsx,OpenApiSyncList.tsx,OpenApiSyncTab.tsx,BulkConfirmDialog.tsx,ConfirmSyncDialog.tsx (new, only if no such dialog exists)}` | Switch, count, filter, summary |
| `docs/features/openapi-sync-ux.md` | Document the switch and the count |

## Review Focus

1. With the switch on, Accept all must never overwrite a field the user edited. Pinned by `accept_all_keeps_edited_fields_when_keep_my_values_is_on`.
2. With the switch off, Accept all keeps today's behavior and its warning. Pinned by `accept_all_takes_the_spec_when_the_switch_is_off`.
3. Per-field choices the user made by hand must survive toggling the switch. Pinned by `a_manual_field_choice_survives_the_switch`.
4. The conflict count must match the rows that start as Keep yours. Pinned by `the_conflict_count_matches_the_conflict_rows`.
5. The switch state must not carry to another collection. Pinned by `the_switch_resets_per_collection`.

---

### Task 1: Keep my values

**Interfaces:**
- Consumes: `buildSelection(plan, selection)` (skip wins over approve), `useSyncSelection` (`choices: Map`, `carryOver`), `Accept all` handler and `BulkConfirmDialog` warning in `OpenApiSyncTab.tsx`, `FieldChange.outcome`.
- Produces: `useSyncSelection` state `keepMine: boolean` (default true, reset when the collection changes) with `setKeepMine`; `acceptAll()` approves every item and, when `keepMine` is true, sets each edited field's choice to keep yours (fields whose outcome is `conflict` or where `yours !== baseline`); when false it keeps today's behavior and the warning dialog. Manual per-field choices are never overwritten by toggling the switch.

- [ ] **Step 1: Read** `useSyncSelection.ts`, `buildSelection.ts` and their tests, and the Accept all path in the tab.
- [ ] **Step 2: Write the failing tests** (`useSyncSelection.test.ts`): `accept_all_keeps_edited_fields_when_keep_my_values_is_on`, `accept_all_takes_the_spec_when_the_switch_is_off`, `a_manual_field_choice_survives_the_switch`, `the_switch_resets_per_collection`, `the_switch_defaults_to_on`.
- [ ] **Step 3: Run to see them fail.** `yarn test useSyncSelection`.
- [ ] **Step 4: Implement** the state and the accept-all rule.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: green.

### Task 2: Switch, conflict count and filter

**Interfaces:**
- Consumes: Task 1; `OpenApiSyncActionsBar.tsx`; `plan` items of kind `changed` with `changes[].outcome`; `shadcn` `Switch`, `Badge`, `Toggle`.
- Produces: in the actions bar a `Switch` labelled **Keep my values** with the help line "Accept all keeps what you changed. Turn off to take the spec's values."; a badge **N conflicts** (N counts changed items with at least one `conflict` field; hidden at 0) that toggles a filter **Show conflicts only** in the list; the Accept all warning copy appears only when the switch is off.

- [ ] **Step 1: Read** the actions bar and list filtering code.
- [ ] **Step 2: Write the failing tests:** `the_switch_shows_on_by_default`, `the_conflict_count_matches_the_conflict_rows`, `no_conflict_badge_at_zero`, `the_badge_filters_the_list_to_conflicts`, `the_accept_all_warning_shows_only_when_the_switch_is_off`, `the_switch_has_an_accessible_name`.
- [ ] **Step 3: Run to see them fail.** `yarn test OpenApiSyncActionsBar OpenApiSyncList`.
- [ ] **Step 4: Implement** the switch, badge and filter.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: green.

### Task 3: Confirm summary and docs

**Interfaces:**
- Consumes: the Confirm and sync handler (`OpenApiSyncTab.tsx`), `plan.items`, the selection, `OpenApiSyncActionsBar` button.
- Produces: **Confirm and sync** first opens a dialog (read it first; if one exists, only add the missing lines) titled "Confirm sync" with the lines "N requests added", "N requests updated", "N requests deleted", "N changes kept as yours", "N skipped"; it cannot open with an empty selection; the doc section "Review" explains Keep my values and the conflict count, with manual checks 31 to 34.

- [ ] **Step 1: Read** the apply button path and any existing confirm.
- [ ] **Step 2: Write the failing tests:** `confirm_sync_lists_the_counts`, `confirm_sync_counts_kept_fields_as_kept`, `cancel_applies_nothing`, `confirm_calls_apply_once`, `the_button_is_disabled_with_nothing_selected`.
- [ ] **Step 3: Run to see them fail.** `yarn test OpenApiSyncTab`.
- [ ] **Step 4: Implement** the dialog and the doc section.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: green.

Next plan: docs/superpowers/plans/2026-10-28-openapi-sync-ux-5/plan-05-openapi-sync-ux-5.md
