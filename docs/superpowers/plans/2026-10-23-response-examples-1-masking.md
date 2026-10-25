# Response Examples, Plan 1: Commit Masking Gaps Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend `mask_examples` so form, text, XML and HTML bodies, recognisable secret values under harmless names, and URL credentials are masked in commits, with tests.

**Architecture:** All changes stay in `crates/rocket-git/src/sanitize.rs` (pure functions on `serde_yaml::Value` and text). `mask_side` gains body handling for every non-binary body type and a value-shape check that runs on header, param, query and JSON string values whatever their name. The on-disk file keeps real values. Only the index copy changes, as today.

**Tech Stack:** Rust, `serde_yaml`. No new dependencies (no regex crate: use hand-written scanning like the existing `mask_json_text`).

**Spec:** `docs/superpowers/specs/2026-10-23-response-examples-design.md`

## Global Constraints

- Masked value is the literal `[masked]` (`MASK`), JSON form `"[masked]"` (`MASKED_JSON`).
- Masking is idempotent: a second pass returns `None` from `mask_examples`.
- Only the `examples:` block is touched. `http.auth` and the rest of the file stay as they are.
- Working tree keeps real values. Do not change `mask_staged_examples` in `git2_service/staging.rs`.
- Rust: no panicking unwrap calls in production paths. Tests run with `cargo test -j4 -p rocket-git <test_name>`.
- Binary bodies (`type: binary`) are never edited.

## Review Focus

- A JSON body sent with `type: text` (a server that mislabels it) must still get its secret keys masked.
- `Set-Cookie` and `Cookie` header values are masked whatever the cookie name inside (the header name already matches `cookie`). Pin this with a test so a later refactor cannot lose it.
- A word like `token` inside ordinary prose (`"message": "token expired"`) or an XML element value that merely mentions a secret word must not be masked. Only secret-named keys and elements are.
- Masking `https://user:pass@host/path` must keep the host and path readable.
- An empty `data` or an absent `body` must not change the file or panic.

---

### Task 1: Mask non-JSON bodies

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (section 2.3, examples).

**Files:**
- Modify: `crates/rocket-git/src/sanitize.rs` (the `mask_side` body branch and new helpers, plus tests in the existing `tests` module)

**Interfaces:**
- Consumes: existing `is_secret_name(&str) -> bool`, `mask_named_entry(&mut Value) -> usize`, `mask_json_text(&str) -> (String, usize)`, `looks_like_jwt(&str) -> bool`, constants `MASK`, `MASKED_JSON`.
- Produces (all private to the module):
  - `fn mask_body(body: &mut Value) -> usize`: replaces the inline body branch of `mask_side`.
  - `fn mask_text_body(text: &str) -> (String, usize)`: masks secrets in a text, XML, HTML or SPARQL body.
  - `fn mask_form_text(text: &str) -> Option<(String, usize)>`: masks secret-named values in `a=b&c=d` text. Returns `None` when the text is not form-shaped.
  - `fn mask_xml_text(text: &str) -> (String, usize)`: masks the text content of secret-named elements (`<token>abc</token>`).

- [ ] **Step 1: Write the failing tests** in the `tests` module of `sanitize.rs`. Each test builds a one-example file string, calls `mask_examples`, and asserts on the output text:
  - `form_urlencoded_list_masks_secret_named_fields`: request body `type: form-urlencoded` with `data: [{name: username, value: ann}, {name: client_secret, value: s3}]`. Assert `client_secret` value is `[masked]`, `ann` stays, count is 1.
  - `text_body_that_holds_json_is_masked_as_json`: response body `type: text`, `data: '{"access_token": "abc", "ok": true}'`. Assert `"access_token": "[masked]"` and `"ok": true`.
  - `form_shaped_text_body_masks_secret_keys`: body `type: text`, `data: "user=ann&password=hunter2&page=2"`. Assert `password=[masked]`, `user=ann`, `page=2`.
  - `xml_body_masks_secret_named_elements`: `type: xml`, `data: "<r><token>abc</token><name>Ann</name></r>"`. Assert `<token>[masked]</token>` and `<name>Ann</name>`.
  - `jwt_inside_plain_text_is_masked`: `type: text`, `data: "got eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiIxIn0.c2ln ok"`. Assert the JWT is gone and `got ... ok` remains.
  - `prose_mentioning_a_secret_word_is_not_masked`: `type: text`, `data: "the token expired, please log in"`. Assert `mask_examples` returns `None`.
  - `binary_and_empty_bodies_are_left_alone`: one example with `type: binary` and `data: AAAA`, one with `data: ""`. Assert `None`.
  - `set_cookie_value_is_masked_whatever_the_cookie_name`: response header `{name: Set-Cookie, value: "theme=dark; Path=/"}`. Assert `[masked]` (pins current behaviour).

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -j4 -p rocket-git sanitize`
Expected: the new body tests FAIL (only the `set_cookie` test and the prose and binary tests may already pass).

- [ ] **Step 3: Implement `mask_body` and the helpers** in `sanitize.rs`, and call `mask_body` from `mask_side` where the inline `json` branch is. Rules:
  - `type: json`: current behaviour (`mask_json_text`).
  - `type: form-urlencoded` or `multipart-form` with a list `data`: run `mask_named_entry` on each entry that has `name` and a string `value`. Skip entries with no string `value` (file parts).
  - `type: binary` or `file`: untouched.
  - Any other type with a string `data` (`text`, `xml`, `html`, `sparql`, unknown): call `mask_text_body`.
  - `mask_text_body` order: if the trimmed text starts with `{` or `[`, use `mask_json_text`. Else if `mask_form_text` returns `Some`, use it. Else apply `mask_xml_text`. Finally mask every whitespace- or quote-delimited word that passes `looks_like_jwt`, replacing it with `MASK`.
  - `mask_form_text` returns `Some` only when the text has no whitespace or newline and every `&`-separated part contains `=`.
  - `mask_xml_text` scans for `<name>text</name>` where `is_secret_name(name)` and `text` has no `<`, and replaces `text` with `MASK`. Ignore attributes and self-closing tags.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -j4 -p rocket-git sanitize`
Expected: all `sanitize` tests PASS, including the existing ones.

- [ ] **Step 5: Run `cargo check -j4 -p rocket-git`**
Expected: no errors.

### Task 2: Mask secret-shaped values under harmless names, URL credentials, and update docs

**Files:**
- Modify: `crates/rocket-git/src/sanitize.rs`
- Modify: `docs/features/example-masking.md`

**Interfaces:**
- Consumes: Task 1's `mask_body`, `mask_text_body`.
- Produces: `fn looks_like_secret_value(text: &str) -> bool` (private). It replaces `looks_like_jwt` at every call site, including `mask_json_text`. `looks_like_jwt` stays as one of its checks.

- [ ] **Step 1: Write the failing tests** in the `tests` module:
  - `secret_shaped_values_are_detected`: `looks_like_secret_value` is true for the test JWT, `Bearer abc123`, `Basic dXNlcjpwYXNz`, `sk_live_abcdefghij`, `ghp_abcdefghijklmnop`, `github_pat_abc123`, `xoxb-123-abc`, `AKIAIOSFODNN7EXAMPLE`, and a string starting `-----BEGIN RSA PRIVATE KEY-----`. It is false for `application/json`, `Ann`, `550e8400-e29b-41d4-a716-446655440000` (a UUID), a 40-character git hash, and `Bearer` alone.
  - `harmless_header_name_with_secret_value_is_masked`: header `{name: X-Tenant, value: "Bearer abc123"}` becomes `[masked]`. A header `{name: X-Trace, value: 550e8400-e29b-41d4-a716-446655440000}` stays.
  - `harmless_json_key_with_secret_value_is_masked`: body `{"data": "ghp_abcdefghijklmnop", "id": "550e8400-e29b-41d4-a716-446655440000"}` masks only `data`.
  - `url_credentials_are_masked_and_host_kept`: request `url: https://ann:pw@api.io/v1?x=1` becomes `https://[masked]@api.io/v1?x=1`.
  - `url_query_value_with_secret_shape_is_masked`: `https://x.io/a?ref=ghp_abcdefghijklmnop&page=1` masks `ref` only.
  - `masking_stays_idempotent_with_new_rules`: run `mask_examples` twice over a file that triggers each new rule. Second call is `None`.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -j4 -p rocket-git sanitize`
Expected: the new tests FAIL.

- [ ] **Step 3: Implement `looks_like_secret_value`** with these checks, exactly: `looks_like_jwt`; starts with `Bearer ` or `Basic ` followed by at least one non-space character; starts with one of `sk_live_`, `sk_test_`, `ghp_`, `gho_`, `ghs_`, `github_pat_`, `xoxb-`, `xoxp-`, `xoxa-`, `xoxr-`, `xoxs-` followed by at least one more character; `AKIA` followed by exactly 16 uppercase ASCII letters or digits and nothing else; starts with `-----BEGIN ` and contains `PRIVATE KEY`. Do not add a generic "long random string" check: it would mask ids and hashes.
  Wire it in: `mask_named_entry` also masks when the entry value passes the check (any name); `mask_url` masks a query value that passes the check and replaces `user:pass@` (the userinfo before `@` in the authority) with `[masked]@`; `mask_json_text` and the plain-word pass in `mask_text_body` use it instead of `looks_like_jwt`.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -j4 -p rocket-git sanitize` then `cargo test -j4 -p rocket-git masks_only_the_examples_block`
Expected: PASS.

- [ ] **Step 5: Update `docs/features/example-masking.md`.** Rewrite the "Masked" and "Not covered" bullets to match: list the body types now handled, the value shapes (the exact list from Step 3), URL userinfo, and that `Set-Cookie` and `Cookie` headers are masked by header name. Under "Not covered" keep: secrets in non-secret-named keys whose values have no known shape, `http.auth.*` of the request itself, and multipart file parts. Replace the last bullet ("There is no UI to create or edit them yet") with a pointer to `docs/features/response-examples.md` (created in Plan 3).

- [ ] **Step 6: Run `cargo check -j4 -p rocket-git` and `cargo test -j4 -p rocket-git`**
Expected: PASS.
