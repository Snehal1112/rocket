# OpenAPI sync UX 3: Connection settings dialog Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The connection dialog shows the current state, lets the user check right there, warns about a rate-limit trap, and has one consistent name.

**Architecture:** Frontend only, on top of plan 01 (Change link) and plan 03 (`focus` prop). The dialog gains a status summary row, a hint under the frequency select, and a rename of the title and every entry point.

**Tech Stack:** React + TypeScript, shadcn/ui, lucide-react, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-28-openapi-sync-ux-3-design.md` (section 5)

## File Structure

| File | Change |
|---|---|
| `src/components/collections/openapi-sync/{OpenApiSyncConnectionDialog.tsx,OpenApiSyncHeader.tsx,OpenApiSyncOverview.tsx,openApiSyncView.ts}` | Status row, hint, rename |
| `docs/features/openapi-sync-ux.md`, `docs/roadmaps/roadmap.md` | Docs for series 3 |
| tests | `OpenApiSyncConnectionDialog.test.tsx`, `OpenApiSyncHeader.test.tsx`, `OpenApiSyncOverview.test.tsx`, `openApiSyncView.test.ts` |

## Review Focus

1. The hint must not show when a token is stored or the link is not Github. Pinned by `no_rate_limit_hint_with_a_token`.
2. Check now in the dialog must respect consent. Pinned by `check_now_is_disabled_without_approval`.
3. The renamed title must not break the existing tests or a11y names. Pinned by `every_entry_point_opens_connection_settings`.
4. A failed check must show the fixed sentence, never the raw message. Pinned by `the_status_row_shows_the_failure_sentence`.

---

### Task 1: Status row and Check now

**Interfaces:**
- Consumes: `OpenApiSyncStatus` (`lastCheckedAt`, `lastStatus`, `lastMessage`), `CheckNowButton`, `formatRelative` (as the header uses it).
- Produces: a status row at the top of the dialog: "Last checked <time>. No problems." or the failure sentence, with a **Check now** button (the shared component, disabled without approval).

- [ ] **Step 1: Read** the header's last-checked text and `CheckNowButton`.
- [ ] **Step 2: Write the failing tests:** `the_status_row_shows_the_last_check`, `the_status_row_shows_the_failure_sentence`, `check_now_in_the_dialog_runs_a_check`, `check_now_is_disabled_without_approval`.
- [ ] **Step 3: Run to see them fail.** `yarn test OpenApiSyncConnectionDialog`.
- [ ] **Step 4: Implement** the row.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: green.

### Task 2: Rate-limit hint

**Interfaces:**
- Consumes: the frequency select, `isGithubLocation`, the token-stored flag in the dialog.
- Produces: under the select, when the interval is `every_5_minutes`, the link is a Github link and no token is stored: "GitHub allows 60 requests an hour without a token. Pick a longer interval or add a token." with an **Add token** link that focuses the token field.

- [ ] **Step 1: Read** the Automatic checks section.
- [ ] **Step 2: Write the failing tests:** `the_hint_shows_for_five_minutes_on_github_without_a_token`, `no_rate_limit_hint_with_a_token`, `no_rate_limit_hint_for_a_non_github_link`, `no_rate_limit_hint_for_a_longer_interval`, `add_token_in_the_hint_focuses_the_token_field`.
- [ ] **Step 3: Run to see them fail.** `yarn test OpenApiSyncConnectionDialog`.
- [ ] **Step 4: Implement** the hint.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: green.

### Task 3: One name and docs

**Interfaces:**
- Consumes: every string "Edit connection settings" (dialog title, header menu, Overview link, import warning `CHECKS_NOT_ENABLED` in `src-tauri/src/commands/import.rs`).
- Produces: the title and entry points read **Connection settings**; the import warning says "Open the collection menu, then OpenAPI sync, then Connection settings."; `docs/features/openapi-sync-ux.md` documents Change link, values, next steps, Skipped and the dialog, with manual checks 13 to 20; the roadmap lists series 3.

- [ ] **Step 1: Read** with `rg "Edit connection settings"` across `src` and `src-tauri`.
- [ ] **Step 2: Write the failing tests:** `every_entry_point_opens_connection_settings` (header menu, button and Overview link all open a dialog titled "Connection settings"); update the Rust test that pins the import warning text.
- [ ] **Step 3: Run to see them fail.** `yarn test openapi-sync`, `cargo test -j4 -p rocket check_for_updates`.
- [ ] **Step 4: Implement** the rename and write the docs.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync CollectionNode`, `cargo check -j4`. Expected: green.

Series 3 is complete here. Next series: docs/superpowers/plans/2026-10-28-openapi-sync-ux-4/00-plan-index.md
