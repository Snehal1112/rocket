# Export name-based scrub: shared rule and Bruno ZIP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Bruno ZIP blanks credential-like values in header, param, form and OAuth2 parameter rows, in URL query strings and OAuth URL fields, and in variables, using one shared name rule that matches the last word of a name.

**Architecture:** A new pure module `sensitive.rs` in `rocket-export` holds `is_sensitive_name`. `scrub.rs` (YAML side) calls it for every key/value row location and extends the URL and variable scrubs. No new crates, no I/O.

**Tech Stack:** Rust (serde_yaml).

**Spec:** `docs/superpowers/specs/2026-10-23-export-name-scrub-design.md` (decisions and the exact rule). Plan 2 (`plan-02-postman-and-docs.md`) applies the same rule to Postman and updates the dialog and docs.

## Global Constraints

- Domain logic only in `rocket-export`; no panicking unwrap calls in production paths; tests use `.expect("...")`; cargo per crate with `-j4` (`cargo test -j4 -p rocket-export`, `cargo check -j4 -p rocket-export`), never a workspace-wide run.
- The rule matches the **last word** of a name (and the listed two-word endings), never a substring. The allow-list wins over every other rule.
- A value is blanked only when it is a non-empty literal and not `is_variable_reference`. A lone `{{ref}}` is always kept.
- Report notes: `ExportAction::Blanked`, the location's feature name, detail `<name> (by name)`. Existing note texts for the five headers (`Sensitive header`) stay unchanged.
- Identifiers stay as written: basic/digest/NTLM usernames, awsv4 `accessKeyId`, OAuth2 `clientId`.
- `crates/rocket-app/src/redaction.rs` and the TypeScript header lists are not touched (Console masking keeps its five names).
- Bodies, scripts, descriptions, `.js` and `.proto` files are not scanned.

## Review Focus

1. `token_type=Bearer`, `page_token=abc`, `max_tokens=10`, `author=x` and `Cache-Key: x` keep their values, while `access_token=abc` is blanked: pinned in Task 1 by `name_rule_table` and in Task 2 by `params_are_scrubbed_by_name`.
2. A lone reference `?access_token={{t}}` or a param `api_key: "{{k}}"` is kept: pinned in Task 2 by `references_in_sensitive_rows_are_kept`.
3. An OAuth2 URL with a password in userinfo or `client_secret=x` in the query is blanked, and the same URL with `{{ref}}` is kept: pinned in Task 3 by `oauth_url_fields_are_scrubbed`.
4. A secret variable holding `{{ref}}` keeps the reference, while a literal is blanked: pinned in Task 3 by `secret_variable_keeps_reference_and_blanks_literal`.
5. A non-secret variable named `plain_name` keeps its value while `api_token` is blanked, in a collection file and in an environment file: pinned in Task 3 by `variables_are_blanked_by_name`.

---

### Task 1: The shared name rule

**Files:**
- Create: `crates/rocket-export/src/sensitive.rs`
- Modify: `crates/rocket-export/src/lib.rs` (add `mod sensitive;` and `pub use sensitive::is_sensitive_name;`), `crates/rocket-export/src/scrub.rs` (`is_sensitive_header`)

**Interfaces:**
- Produces: `pub fn is_sensitive_name(name: &str) -> bool` (rule in the spec section "The name rule"). `scrub::is_sensitive_header(name)` keeps its signature and now returns `is_sensitive_name(name)`, which is a superset of the five headers.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (section on headers and params) and the design spec above.** Then read `crates/rocket-export/src/scrub.rs:1-35` and the `is_variable_reference_cases` test.

- [ ] **Step 2: Write the failing test `name_rule_table` in `sensitive.rs`.** One table-driven test asserting each row:

  - Sensitive (true): `Authorization`, `Proxy-Authorization`, `Cookie`, `Set-Cookie`, `X-Api-Key`, `api_key`, `apiKey`, `APIKey`, `apikey`, `access_token`, `accessToken`, `refresh_token`, `sessionToken`, `X-Auth-Token`, `X-Amz-Security-Token`, `client_secret`, `clientSecret`, `db_password`, `password`, `passwd`, `pass`, `pwd`, `private_key`, `privateKey`, `secret_key`, `client_assertion`, `signature`, `X-Hub-Signature`.
  - Not sensitive (false): `token_type`, `tokenType`, `page_token`, `next_token`, `prev_token`, `max_tokens`, `author`, `keyword`, `monkey`, `sort_key`, `Cache-Key`, `key`, `session_id`, `auth_type`, `grant_type`, `bypass`, `compass`, `passport`, `tokenizer`, `X-Request-Id`, `Content-Type`, `scope`, `state`, the empty string.

  Add `is_sensitive_header_covers_the_five_names` in `scrub.rs` tests asserting the five original header names stay true (case-insensitive).

- [ ] **Step 3: Run `cargo test -j4 -p rocket-export name_rule_table`.** Expected: compile error `cannot find function 'is_sensitive_name'`.

- [ ] **Step 4: Implement `is_sensitive_name`.** Split into lowercase words at non-alphanumeric characters, lower-to-upper boundaries and acronym boundaries (`APIKey` gives `api`, `key`). Check the allow-list (words joined with `_`) first, then the last-word set, the single-word `pass` and `pwd`, and the two-word endings, all as listed in the spec. Keep it one small module with one `const` list per rule.

- [ ] **Step 5: Wire `is_sensitive_header` to `is_sensitive_name`.** Update its doc comment. Run `cargo test -j4 -p rocket-export` and `cargo check -j4 -p rocket-export`. Expected: all tests pass, including the existing header tests, no warnings.

- [ ] **Step 6: Commit** `feat(export): add shared name rule for secret-like names`. Stage explicit paths only (`sensitive.rs`, `lib.rs`, `scrub.rs`).

### Task 2: Bruno key/value rows and URL query pairs

**Files:**
- Modify: `crates/rocket-export/src/scrub.rs` (`walk`, `scrub_headers`, `yaml_scrub_url`, `yaml_scrub_auth`, new row helper)

**Interfaces:**
- Consumes: `is_sensitive_name`, `blank_string`, `blank_note` (existing).
- Produces: the YAML walk scrubs these keys, at any depth, by row `name` (rows are mappings with `name` and `value`):
  - `params` (HTTP and GraphQL query and path rows), feature `Query parameter`
  - `headers` and `metadata` (already scrubbed by `scrub_headers`; now use the new rule, feature `Sensitive header` unchanged)
  - form `data` rows of `form-urlencoded` and `multipart-form` bodies (text parts only, not file parts), feature `Form field`
  - OAuth2 `additionalParameters.{authorizationRequest, accessTokenRequest, refreshTokenRequest}` rows, feature `OAuth2 parameter`
  - the query string of every string `url` (existing `yaml_scrub_url` also blanks the value of each `name=value` pair whose name matches, feature `URL query`)

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (request and auth sections).** Then read `crates/rocket-infra/src/oc/http.rs:26-28,57-59,77-81,106-120` (row shapes: `params`, form `data`, multipart `data`), `crates/rocket-infra/src/oc/auth.rs:83` and the `additionalParameters` item shape (`name`, `value`, `placement`, `enabled`) in `crates/rocket-shared/src/oauth2.rs:81-103`, and the existing `scrub_headers` and `yaml_scrub_url`.

- [ ] **Step 2: Write the failing tests in `scrub.rs`.**
  - `params_are_scrubbed_by_name`: request YAML with `http.params: [{name: access_token, value: abc, type: query}, {name: page_token, value: p1, type: query}, {name: author, value: me, type: query}]`. Expect only the first value `""` and one `Blanked` note (feature `Query parameter`, detail `access_token (by name)`).
  - `references_in_sensitive_rows_are_kept`: `params` row `api_key: "{{k}}"`, header `X-Auth-Token: "{{t}}"`, url `https://x.test/p?access_token={{t}}`. Expect all unchanged and no notes.
  - `headers_are_blanked_by_name`: headers `X-Auth-Token: abc`, `X-Request-Id: 1`, `Authorization: Bearer abc`. Expect the first and third blanked, the second unchanged.
  - `grpc_and_websocket_rows_use_the_name_rule`: `grpc.metadata` row `x-auth-token: abc` and `websocket.headers` row `Cookie: a=b`. Both values blanked.
  - `form_rows_are_scrubbed_by_name`: urlencoded body `data: [{name: client_secret, value: s}, {name: grant_type, value: password}]` and multipart text part `password: p`, file part named `secret_file`. Expect `client_secret` and multipart `password` blanked, `grant_type` and the file part unchanged.
  - `additional_parameters_are_scrubbed_by_name`: OAuth2 auth with `additionalParameters.accessTokenRequest: [{name: client_assertion, value: jwt, placement: body, enabled: true}, {name: audience, value: api}]`. Expect only the first blanked.
  - `url_query_pairs_are_scrubbed_by_name`: `http.url: https://x.test/p?a=1&access_token=abc&token_type=Bearer&sig=ok` gives `...?a=1&access_token=&token_type=Bearer&sig=ok`; a URL without a query is unchanged; userinfo blanking still works on the same URL.

- [ ] **Step 3: Run `cargo test -j4 -p rocket-export scrub`.** Expected: the seven new tests fail.

- [ ] **Step 4: Implement.** Add one helper that blanks the `value` of each row in a sequence when the row `name` matches (and, for form rows, when the row is a text part; a file part has `filePath` or `type: file`), called from `walk` for `params`, form `data` and `additionalParameters`. Switch `scrub_headers` to the new rule through `is_sensitive_header`. Extend `yaml_scrub_url` to split the query on `&` and blank values by name, leaving the `#fragment` and unparsable segments as they are. Keep `walk` key-based and depth-agnostic. Disabled rows are blanked too.

- [ ] **Step 5: Run `cargo test -j4 -p rocket-export` and `cargo check -j4 -p rocket-export`.** Expected: all pass, no warnings.

- [ ] **Step 6: Commit** `feat(export): blank secret-like rows by name in the Bruno ZIP`. Stage `scrub.rs` only.

### Task 3: OAuth URL fields and variables

**Files:**
- Modify: `crates/rocket-export/src/scrub.rs` (`yaml_scrub_auth`, `scrub_variables`, `clear_secret_value`)

**Interfaces:**
- Consumes: `yaml_scrub_url`, `is_sensitive_name`, `is_variable_reference`.
- Produces:
  - `yaml_scrub_auth` runs `yaml_scrub_url` on the string fields `accessTokenUrl`, `refreshTokenUrl`, `authorizationUrl` and `callbackUrl` of an auth mapping (OAuth2 and OAuth1).
  - `scrub_variables` blanks `value` and `initial` of a row when `secret: true` **and** the value is a literal (a lone `{{ref}}` is kept now); and blanks a non-secret row when its `name` matches the rule and the value is a literal. Typed secret form `{type, data}` follows the same literal-only rule for `data`. Notes: `Secret variable` (unchanged) and `Variable` with detail `<name> (by name)`.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (variables and auth sections).** Then read `scrub.rs:83-96` (`clear_secret_value`) and `scrub.rs:204-230`. The comment "Secret values are always blanked, even a reference, per the spec" is superseded by decision 4 of the design spec: update the comment and the `.claude` doc line in plan 2.

- [ ] **Step 2: Write the failing tests.**
  - `oauth_url_fields_are_scrubbed`: OAuth2 auth with `accessTokenUrl: https://u:pw@idp.test/token?client_secret=x&a=1`, `authorizationUrl: https://idp.test/auth?state=s`, `refreshTokenUrl: "{{base}}/r?token={{t}}"`, plus OAuth1 `callbackUrl: https://u:pw@app.test/cb`. Expect `accessTokenUrl` becomes `https://u:@idp.test/token?client_secret=&a=1`, the others unchanged except the OAuth1 callback `https://u:@app.test/cb`.
  - `secret_variable_keeps_reference_and_blanks_literal`: rows `a: {secret: true, value: "{{ref}}"}` and `b: {secret: true, value: top, initial: top}` plus a typed secret `c: {secret: true, value: {type: secret, data: lit}}`. Expect `a` unchanged, `b` and `c.data` blanked, notes only for `b` and `c`.
  - `variables_are_blanked_by_name`: rows `api_token: abc`, `plain_name: abc`, `db_password: "{{ref}}"`; run on a collection-file shape (`request.variables` or top-level `variables`) and on an environment-file shape (`variables:` list). Expect `api_token` blanked with a `Variable` note, the other two unchanged.
  - Update the existing secret-variable test that asserts a reference is blanked, so it matches decision 4.

- [ ] **Step 3: Run `cargo test -j4 -p rocket-export scrub`.** Expected: the new tests fail, and the old reference test fails until updated.

- [ ] **Step 4: Implement.** Call `yaml_scrub_url` for the four URL keys from `yaml_scrub_auth` (string values only). Rework `clear_secret_value` to skip lone references, and add the by-name branch to `scrub_variables` for non-secret rows. Keep `blank_string` as the single place that decides "literal".

- [ ] **Step 5: Run `cargo test -j4 -p rocket-export`, `cargo check -j4 -p rocket-export` and `cargo test -j4 -p rocket-app collection_export`.** Expected: all pass, no warnings.

- [ ] **Step 6: Commit** `feat(export): scrub OAuth URLs and variables in the Bruno ZIP`. Stage `scrub.rs` only.

Next plan: docs/superpowers/plans/2026-10-23-export-name-scrub/plan-02-postman-and-docs.md
