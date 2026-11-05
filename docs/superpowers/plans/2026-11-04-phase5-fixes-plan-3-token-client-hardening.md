# Phase 5 fixes, Plan 3: token client hardening (S-3, S-6, T-1, T-2, C-7)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every OAuth token POST has a timeout, a bounded response, no cross-host redirects, and error text that cannot leak a secret from the URL. `fetch_direct_grant_token` stops being a second, unguarded token POST.

**Architecture:** `ReqwestTokenClientProvider::post_form` (in `rocket-infra`) gets the guards. `fetch_direct_grant_token` is rewritten to call the same `post_form` and read `access_token` from the returned `OAuthToken`, so there is one token POST path.

**Tech Stack:** Rust, `rocket-infra` (`reqwest_executor.rs`), `wiremock` for the tests.

**Spec:** `.claude/reviews/2026-11-04-ddd-phase5-audit.md` (S-3, S-6, T-1, T-2, C-7).

## Global Constraints

- Values: connect timeout 10 s, total timeout 30 s, response body cap 1 MiB, error body excerpt at most 200 characters.
- A token POST never follows a redirect (`redirect::Policy::none()`); a 3xx is returned as an error `Token endpoint returned 3xx`.
- Error text never contains the request URL (use `reqwest::Error::without_url()`).
- Existing behaviour that stays: the vault-certificate trust gate runs before the POST; client certificates are presented by `client_for`.
- No `unwrap` in production code. `cargo` commands take `-j4`; never `--workspace`.
- 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (auth configuration).

## Review Focus

- A token endpoint that never answers fails after the timeout instead of hanging "Get token", refresh and Send.
- A 307 to another host is not followed and `client_secret` is not re-sent.
- A 500 response with a 10 MB body produces a short error and does not buffer the body.
- A query-type secret in the token URL does not appear in "Token request failed".
- The direct grants (client credentials, password) still return an access token, with the same client-credential placement (body or Basic header).

---

### Task 1: Guards on `post_form` (S-3, S-6, T-1, T-2)

**Files:**
- Modify: `crates/rocket-infra/src/reqwest_executor.rs` (`ReqwestTokenClientProvider::post_form` ~783-820, `ClientBuild` / `build_client` ~830-870 for a connect timeout)
- Test: same file or the existing token-provider test module, using `wiremock`

**Interfaces:**
- Produces: `post_form` keeps its trait signature (`TokenClientProvider::post_form(&self, token_url, verify_ssl, certificates, form, extra_headers) -> DomainResult<OAuthToken>`). A private `async fn read_capped_body(resp: reqwest::Response, max: usize) -> DomainResult<Vec<u8>>` reads the stream chunk by chunk and fails past the cap.
- `ClientBuild` gains `connect_timeout: Option<Duration>` (default `None`, so the executor clients are unchanged).

- [ ] **Step 1: Write the failing tests.** `token_post_times_out` (a mock that delays longer than an injected short total timeout; assert an error that mentions the timeout, in well under a second; make the timeout a field of the provider so the test can shorten it). `token_post_does_not_follow_a_redirect` (307 to a second mock; assert the second mock got zero requests and the error mentions 307). `token_post_error_body_is_short` (500 with a 5,000-char body; error length under 400). `token_post_rejects_an_oversized_success_body` (2 MiB JSON body). `token_post_error_has_no_url` (token URL with `?key=secret` and a closed port; assert the message does not contain `secret`).
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-infra token_post` — Expected: FAIL.
- [ ] **Step 3: Implement.** Build the token client with `follow_redirects = false`, `connect_timeout` 10 s, and send with `.timeout(30 s)`. Replace `format!("... {e}")` with `e.without_url()`. Read the body through `read_capped_body`, truncate the error excerpt to 200 characters (on a char boundary), parse the success body with `serde_json::from_slice`.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-infra reqwest_executor` — Expected: PASS.
- [ ] **Step 5: Commit** with the `dev-workflow-skills:1-git-commit` skill (pathspec `crates/rocket-infra/src/reqwest_executor.rs`).

### Task 2: One token POST path (C-7)

**Files:**
- Modify: `crates/rocket-infra/src/reqwest_executor.rs` (`fetch_direct_grant_token` ~1399-1471 and its callers)
- Test: same file, `wiremock`

**Interfaces:**
- Consumes: `post_form` from Task 1, `rocket_http::OAuthToken { access_token, .. }` (check the real field names in `rocket-http/src/oauth2.rs` before writing the test).
- Produces: `fetch_direct_grant_token` keeps its signature and its `DomainResult<String>` return. Its body builds the form (grant params, `scope`, and `client_id` / `client_secret` when placement is `body`), puts the Basic header into `extra_headers` otherwise, calls `post_form`, and returns `token.access_token`.

- [ ] **Step 1: Write the failing tests.** `direct_grant_sends_credentials_in_the_body` (placement `body`: the mock asserts `client_id` and `client_secret` in the form), `direct_grant_sends_a_basic_header_by_default` (header present, no secret in the form), `direct_grant_public_client_sends_no_credentials`, `direct_grant_times_out_like_post_form` (reuses the injected short timeout).
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-infra direct_grant` — Expected: the timeout test FAILS (no timeout today); the others pass against the old code, which proves the refactor keeps behaviour.
- [ ] **Step 3: Implement** the rewrite. Delete the duplicated client build and response parsing. Keep the identity and proxy handling by passing them the way `client_for` already takes them (read `client_for` first and reuse it, do not build a third client path).
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-infra`, `cargo check -j4`, `cargo clippy -j4 -p rocket-infra -- -D warnings` — Expected: PASS.
- [ ] **Step 5: Commit** (pathspec `crates/rocket-infra/src/reqwest_executor.rs`).
