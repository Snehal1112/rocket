# OpenAPI sync UX 3: Next steps Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Failed checks and empty screens tell the user what to do and offer the button.

**Architecture:** Frontend only. A pure `nextStep(status)` function maps the stored failure code and the location to an action; the header alert, the Spec updates empty state and the Your changes empty state render it. The connection dialog accepts a `focus` prop.

**Tech Stack:** React + TypeScript, shadcn/ui, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-28-openapi-sync-ux-3-design.md` (section 3)

## File Structure

| File | Change |
|---|---|
| `src/components/collections/openapi-sync/{nextStep.ts (new),OpenApiSyncHeader.tsx,OpenApiSyncTab.tsx,OpenApiSyncChanges.tsx,OpenApiSyncConnectionDialog.tsx}` | Mapping, buttons, `focus` prop |
| `__tests__/{nextStep.test.ts (new),OpenApiSyncHeader.test.tsx,OpenApiSyncTab.test.tsx,OpenApiSyncChanges.test.tsx,OpenApiSyncConnectionDialog.test.tsx}` | Tests |

## Review Focus

1. Add access token must not show for a non-Github link (tokens are Github-only). Pinned by `no_token_action_for_a_non_github_link`.
2. Check now must stay disabled without consent. Pinned by `check_now_in_the_alert_respects_consent`.
3. A code the mapping does not know must show no action, not a wrong one. Pinned by `an_unknown_code_has_no_action`.
4. Opening the dialog at the token field must not break the other sections. Pinned by `focus_token_scrolls_to_the_access_section`.

---

### Task 1: nextStep mapping and dialog focus

**Interfaces:**
- Consumes: `OpenApiSyncStatus` (`lastStatus` code, `location`, `consent`), `isGithubLocation`.
- Produces: `nextStep(status: OpenApiSyncStatus): { kind: 'add_token' | 'check_now' | 'none'; label: string }`. Rules: `not_found` and `unauthorized` on a Github link give `add_token` ("Add access token"); `network`, `status`, `rate_limited`, `too_many_redirects` give `check_now` ("Check now"); anything else, or consent not granted, gives `none`. Dialog prop `focus?: 'token'` that scrolls to and focuses the token input on open.

- [ ] **Step 1: Read** `OpenApiSyncHeader.tsx` (the failed-check alert and where it reads the code), `CheckNowButton`, and the dialog's Access section.
- [ ] **Step 2: Write the failing tests:** `nextStep.test.ts` (`not_found_on_github_offers_a_token`, `no_token_action_for_a_non_github_link`, `network_offers_check_now`, `an_unknown_code_has_no_action`, `no_action_without_consent`); dialog `focus_token_scrolls_to_the_access_section`.
- [ ] **Step 3: Run to see them fail.** `yarn test nextStep`.
- [ ] **Step 4: Implement** the function and the `focus` prop.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: green.

### Task 2: Failed-check alert action

**Interfaces:**
- Consumes: Task 1, the header alert, `useCheckNow`.
- Produces: the destructive alert shows the action button from `nextStep`: **Add access token** opens the connection dialog with `focus: 'token'`; **Check now** reuses the header's check handler and shows pending state.

- [ ] **Step 1: Read** the alert JSX and how `CheckNowButton` is wired.
- [ ] **Step 2: Write the failing tests** (`OpenApiSyncHeader.test.tsx`): `a_not_found_failure_offers_add_access_token`, `add_access_token_opens_the_dialog_at_the_token`, `a_network_failure_offers_check_now`, `check_now_in_the_alert_respects_consent`.
- [ ] **Step 3: Run to see them fail.** `yarn test OpenApiSyncHeader`.
- [ ] **Step 4: Implement** the button.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: green.

### Task 3: Empty states

**Interfaces:**
- Consumes: the Spec updates `EmptyState` ("No spec updates waiting"), Your changes empty text, `useCheckNow`.
- Produces: the Spec updates empty state gets a **Check now** button and the line "Last checked <relative time>." (or "Not checked yet."); Your changes empty state reads "You have not changed anything. Edit a synced request and it shows up here with the values you changed."

- [ ] **Step 1: Read** both empty states and their tests.
- [ ] **Step 2: Write the failing tests:** `the_empty_updates_state_offers_check_now`, `the_empty_updates_state_says_when_it_was_last_checked`, `the_empty_changes_state_explains_how_changes_appear`.
- [ ] **Step 3: Run to see them fail.** `yarn test OpenApiSyncTab`.
- [ ] **Step 4: Implement** the text and button.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: green.

Next plan: docs/superpowers/plans/2026-10-28-openapi-sync-ux-3/plan-04-openapi-sync-ux-3.md
