# OpenAPI sync UX 5: Failure banners and re-check Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Match Bruno's failure handling: the Overview and Spec updates pages show the failed check with the next-step button, and turning checks on in the settings runs one check right away.

**Architecture:** Frontend only. A shared `CheckFailureBanner` renders the stored failure sentence and the `nextStep` action (series 3 plan 03); Overview and Spec updates mount it; the connection dialog runs `useCheckNow` once after a Save that changes the interval from Off to a frequency.

**Tech Stack:** React + TypeScript, shadcn/ui, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-28-openapi-sync-ux-3-design.md` (section 8); parity table section 2.

## File Structure

| File | Change |
|---|---|
| `src/components/collections/openapi-sync/{CheckFailureBanner.tsx (new),OpenApiSyncOverview.tsx,OpenApiSyncTab.tsx,OpenApiSyncHeader.tsx,OpenApiSyncConnectionDialog.tsx}` | Banner, mounts, re-check |
| `docs/features/openapi-sync-ux.md`, `docs/roadmaps/roadmap.md` | Docs, manual checks 35 to 38 |

## Review Focus

1. The banner must show the fixed sentence, never the raw message or a path. Pinned by `the_banner_shows_the_fixed_sentence`.
2. No banner without a failed check, and none while consent is missing (the consent banner owns that state). Pinned by `no_banner_when_the_last_check_succeeded` and `no_banner_without_consent`.
3. The re-check must run only on Off to on, and only with consent. Pinned by `the_check_runs_when_checks_are_turned_on` and `no_check_when_the_interval_only_changes`.
4. Two banners must not show at once with the header alert. Pinned by `only_one_failure_banner_per_page`.

---

### Task 1: CheckFailureBanner

**Interfaces:**
- Consumes: `OpenApiSyncStatus` (`lastStatus`, `lastMessage`, `consent`), `nextStep(status)` (series 3 plan 03), `useCheckNow`, the connection dialog opener with `focus`, the header's failed-check alert (which this component replaces).
- Produces: `CheckFailureBanner({ collection, status, onOpenSettings }: { collection: string; status: OpenApiSyncStatus; onOpenSettings: (focus?: 'token' | 'link') => void })`: a destructive `Alert` titled "Failed to check for spec updates" with the fixed sentence and one button from `nextStep` (**Add access token**, **Check now**, **Choose another link**/**Choose another file**, or **Connection settings** when the kind is `none`); renders nothing unless the last check failed and consent is granted.

- [ ] **Step 1: Read** the header alert and `nextStep.ts`.
- [ ] **Step 2: Write the failing tests** (`CheckFailureBanner.test.tsx`): `the_banner_shows_the_fixed_sentence`, `no_banner_when_the_last_check_succeeded`, `no_banner_without_consent`, `a_token_failure_offers_add_access_token`, `a_network_failure_offers_check_now`, `an_unknown_code_offers_connection_settings`, `the_button_opens_the_dialog_at_the_right_field`.
- [ ] **Step 3: Run to see them fail.** `yarn test CheckFailureBanner`.
- [ ] **Step 4: Implement** the component.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: green.

### Task 2: Mount it on the pages

**Interfaces:**
- Consumes: Task 1, `OpenApiSyncHeader.tsx` (alert), `OpenApiSyncOverview.tsx`, the Spec updates empty state.
- Produces: the banner at the top of Overview and of the Spec updates tab (as Bruno's Overview banner and "Unable to check for updates" state); the header alert is removed so one banner shows per page; the Spec updates empty state reads "Unable to check for updates. Fix the connection issue above and check again." while the last check failed.

- [ ] **Step 1: Read** the three mount points and their tests.
- [ ] **Step 2: Write the failing tests:** `overview_shows_the_failure_banner`, `spec_updates_shows_the_failure_banner_and_the_empty_text`, `only_one_failure_banner_per_page`, `the_header_alert_is_gone`, `a_successful_check_clears_the_banner`.
- [ ] **Step 3: Run to see them fail.** `yarn test OpenApiSyncOverview OpenApiSyncTab OpenApiSyncHeader`.
- [ ] **Step 4: Implement** the mounts and remove the header alert.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: green.

### Task 3: Re-check after enabling checks, docs

**Interfaces:**
- Consumes: the interval Select and `useSetInterval` in the connection dialog, `useCheckNow`, consent state.
- Produces: after a successful interval change from Off to a frequency, with consent granted, the dialog runs `useCheckNow` once and shows the outcome in the status row (series 3 plan 05); no check on a frequency-to-frequency change or when turning Off; `docs/features/openapi-sync-ux.md` and the roadmap list series 5, manual checks 35 to 38; the parity table rows marked Planned are updated to Done where this series shipped.

- [ ] **Step 1: Read** the dialog's interval handler and its tests.
- [ ] **Step 2: Write the failing tests:** `the_check_runs_when_checks_are_turned_on`, `no_check_when_the_interval_only_changes`, `no_check_when_turning_checks_off`, `no_check_without_consent`, `the_outcome_shows_in_the_status_row`.
- [ ] **Step 3: Run to see them fail.** `yarn test OpenApiSyncConnectionDialog`.
- [ ] **Step 4: Implement** the call and update the docs.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync CollectionNode`. Expected: green.

Series 5 is complete here. A whole-feature review and manual checks 13 to 38 follow.
