# OpenAPI sync UX 4: Localhost over http Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A spec served on `http://localhost:PORT/...` (a dev server) can be imported, connected and synced, while every other http link is still refused.

**Architecture:** `parse_spec_url` accepts http for loopback hosts and stores `SpecLocation::Https { url }` (no new variant). The fetcher's first hop allows http for the same hosts through one shared predicate. Redirects stay https-only.

**Tech Stack:** Rust (`rocket-import`, `rocket-http`, `rocket-infra`, `rocket-app`), React + TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-28-openapi-sync-ux-3-design.md` (section 6)

## File Structure

| File | Change |
|---|---|
| `crates/rocket-http/src/spec_fetch.rs` | `is_loopback_http(&Url) -> bool`, used by the first-hop rule |
| `crates/rocket-import/src/openapi/source_url.rs` | Scheme gate |
| `crates/rocket-infra/src/reqwest_spec_fetcher.rs` | First-hop rule |
| `crates/rocket-app/src/openapi_sync_service.rs` | `safe_message("not_https")` text |
| `src/components/collections/openapi-sync/{openApiSyncView.ts,syncErrors.ts}`, `src/components/import/ImportCollectionDialog.tsx`, `ConnectOpenApiDialog.tsx` | Copy, helper tests |

## Review Focus

1. Lookalike hosts must stay refused: `http://localhost.evil.com`, `http://127.0.0.1.evil.com`, `http://user@localhost`, `http://0.0.0.0`, `http://192.168.1.5`. Pinned by `lookalike_and_private_http_hosts_are_refused`.
2. A redirect from a localhost link to http must still be blocked. Pinned by `a_redirect_to_http_is_still_blocked`.
3. A bearer token must never go over http. Pinned by `a_token_is_not_sent_over_http`.
4. The stored link must round-trip for consent (`display_url` then `parse_spec_url`). Pinned by `a_localhost_link_round_trips_for_consent`.
5. Query strings with secret-like names are still rejected. Pinned by `secret_query_names_are_still_rejected_on_localhost`.

---

### Task 1: URL parser and shared predicate

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Interfaces:**
- Consumes: `parse_spec_url` (`source_url.rs:23`), `redirect_target_allowed` (`spec_fetch.rs`), `SpecLocation::display_url`.
- Produces: `rocket_http::spec_fetch::is_loopback_http(url: &Url) -> bool` (true for scheme http and host `localhost`, `127.0.0.1` or `::1`, no userinfo); `parse_spec_url` accepts those and returns `SpecLocation::Https { url }` with the fragment removed.

- [ ] **Step 1: Read** both files and their test modules.
- [ ] **Step 2: Write the failing tests:** `http_localhost_is_accepted_as_https_location`, `http_loopback_ip_is_accepted`, `lookalike_and_private_http_hosts_are_refused`, `https_links_behave_as_before`, `secret_query_names_are_still_rejected_on_localhost`, `a_localhost_link_round_trips_for_consent`; in `spec_fetch.rs` `is_loopback_http_table`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-import source_url`.
- [ ] **Step 4: Implement** the predicate and the gate.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-import openapi`, `cargo test -j4 -p rocket-http spec_fetch`. Expected: green.

### Task 2: Fetcher first hop and messages

**Interfaces:**
- Consumes: Task 1 predicate, `ReqwestSpecFetcher` (`strict`, `MAX_SPEC_REDIRECTS`), `safe_message`.
- Produces: in strict mode the first hop passes when the scheme is https or `is_loopback_http(first)`; redirect hops keep `redirect_target_allowed`; the bearer token is dropped for any http URL; `safe_message("not_https")` reads "The link must use https. Plain http works only for localhost."

- [ ] **Step 1: Read** `reqwest_spec_fetcher.rs` and its wiremock tests.
- [ ] **Step 2: Write the failing tests:** `a_loopback_http_first_hop_is_fetched` (wiremock on 127.0.0.1 with `strict` on), `a_non_loopback_http_first_hop_is_not_https`, `a_redirect_to_http_is_still_blocked`, `a_token_is_not_sent_over_http`; in `openapi_sync_service.rs` `not_https_message_mentions_localhost`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-infra reqwest_spec_fetcher`.
- [ ] **Step 4: Implement** the rule and the text.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-infra`, `cargo test -j4 -p rocket-app not_https`, `cargo check -j4`. Expected: green.

### Task 3: Frontend copy and docs

**Interfaces:**
- Consumes: `isGithubLocation`, `splitLocation`, the link inputs of the import and Connect dialogs, `syncErrors.ts`.
- Produces: link placeholders and hints read "https link, or http://localhost for a local server"; `syncErrors` maps the new `not_https` sentence; `splitLocation("http://localhost:3000/openapi.json")` shows host and path; `isGithubLocation` stays false for localhost; `docs/features/openapi-sync-ux.md` documents the localhost rule and manual check 21.

- [ ] **Step 1: Read** the two dialogs' link fields and `openApiSyncView.test.ts`.
- [ ] **Step 2: Write the failing tests:** `split_location_handles_a_localhost_link`, `a_localhost_link_is_not_github`, `the_import_dialog_hint_mentions_localhost`, `syncerrors_maps_the_not_https_sentence`.
- [ ] **Step 3: Run to see them fail.** `yarn test openapi-sync ImportCollectionDialog`.
- [ ] **Step 4: Implement** the copy and the doc section.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync ImportCollectionDialog`. Expected: green.

Next plan: docs/superpowers/plans/2026-10-28-openapi-sync-ux-4/plan-02-openapi-sync-ux-4.md
