# rocket-http small helpers Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** Random alphanumeric strings, percent-encoding and JWT segment decoding each exist once in `rocket-http`, and every signer produces the same output as before.

**Architecture:** Golden tests are recorded from the current code first, as failing tests whose assertion output supplies the expected values. A crate-private `util` module then takes over `rand_alnum` and `percent_encode`, and `jwt.rs` gets a local `decode_segment`. The golden tests stay green, unchanged, through both refactors.

**Tech Stack:** Rust, `rocket-http`, `urlencoding`, `rand`, `base64`, `serde_json`.

**Spec:** `docs/superpowers/specs/2026-10-18-rocket-http-small-helpers-design.md`

## Global Constraints

- No panicking unwrap calls in production paths. Tests use `.expect("...")`. No `git` CLI. No new dependency.
- Cargo commands are per crate with `-j4`: `cargo check -j4 -p rocket-http` and `cargo test -j4 -p rocket-http`.
- No change to public behaviour, signer output, error text or on-disk or IPC formats.
- `generate_cnonce` (16), `generate_nonce` (32), `oauth1_sig::percent_encode` and `path_params::encode_path_param_value` stay public with the same signatures.
- The four JWT messages stay exactly: `Invalid JWT header encoding: {e}`, `Invalid JWT header JSON: {e}`, `Invalid JWT payload encoding: {e}`, `Invalid JWT payload JSON: {e}`.
- Golden inputs include `~`, space, `+`, `%`, `*`, `/` in a path segment and non-ASCII text. Tests live inside each module, and no clock or randomness parameter is added.
- Code comments: short full sentences ending with a punctuation mark.

## Review Focus

1. **A `%` that is already an escape, or a lone `%`, in a path parameter or an AWS query value.** A user expects it encoded once, as today, so a signature still verifies. Pinned in Task 1 (`golden_encode_path_param_value_awkward`, `golden_sign_request_awkward_query_and_path_non_s3`) and Task 2 (`percent_encode_matches_urlencoding_for_awkward_inputs`).
2. **`rand_alnum(0)`.** A zero length must give an empty string, never a panic. Pinned in Task 2 (`rand_alnum_zero_is_empty`).
3. **A JWT segment that is valid base64url but not JSON, an empty segment, and a wrong part count.** A user pasting a bad token expects the same message as before. Pinned in Task 1 (`jwt_bad_header_json_message`, `jwt_empty_header_segment_message`, `jwt_wrong_part_count_message_is_unchanged`) and kept in Task 3.
4. **The S3 path.** AWS signs the S3 path as it is and every other service encodes it a second time, and the refactor must keep both. Pinned in Task 1 (`golden_sign_request_awkward_query_and_path_s3` and `..._non_s3`).
5. **A nonce or cnonce with the wrong length or a non-alphanumeric character.** The server rejects it, so it must stay 32 and 16 ASCII letters and digits and unique. Pinned in Task 2 (`generate_nonce_is_32_ascii_alphanumeric_and_unique`, `generate_cnonce_is_16_ascii_alphanumeric_and_unique`).

## File Structure

- Create `crates/rocket-http/src/util.rs`: `rand_alnum`, `percent_encode`, and their tests.
- Modify `crates/rocket-http/src/lib.rs`: add `mod util;`.
- Modify `crates/rocket-http/src/digest_sig.rs` (`generate_cnonce`, about line 57), `oauth1_sig.rs` (`generate_nonce` about 19, `percent_encode` about 14), `aws_sig.rs` (about 169, 170, 192), `path_params.rs` (`encode_path_param_value`, about 33), `jwt.rs` (`decode_jwt`, about 38-57).
- Tests are added inside the `#[cfg(test)]` module of each of those five files. Reuse the fixtures the existing tests build (`OAuth1Auth` as in `explicit_timestamp_and_nonce_override_the_defaults`, `Challenge` through `parse_challenges` as in `legacy_form_omits_qop_nc_and_cnonce`, `AwsCredentials` as in `test_sign_request_produces_valid_authorization`, `PathParam` as in the `path_params` tests).

---

### Task 1: Golden tests recorded before the refactor

**Files:**
- Test: `crates/rocket-http/src/oauth1_sig.rs`, `digest_sig.rs`, `aws_sig.rs`, `path_params.rs`, `jwt.rs` (inline test modules only, no production change)

**Interfaces:**
- Consumes: `oauth1_sig::signature_base_string(method, url, params) -> Result<String, String>`, `oauth1_sig::authorization_header(realm, oauth_params) -> String`, `oauth1_sig::sign(method, url, extra_params, body, &OAuth1Auth, default_timestamp, default_nonce) -> Result<Vec<(String, String)>, String>`, `digest_sig::authorize(challenge, username, password, method, uri, entity_body, nc, cnonce) -> Result<String, String>`, `aws_sig::sign_request(method, url, headers, body, &AwsCredentials, timestamp) -> Result<SignedHeaders, String>`, `path_params::encode_path_param_value(&str) -> String`, `path_params::substitute_path_params(url, params) -> String`, `jwt::decode_jwt(&str) -> Result<JwtClaims, DomainError>`. All take injected nonce, timestamp or cnonce, so no seam is needed.
- Produces: golden tests that pin today's output. Tasks 2 and 3 must keep them passing unchanged.

- [ ] **Step 1: Read the OpenCollection reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Nothing in this task changes collection data.

- [ ] **Step 2: Write the golden tests that compare with an empty recording**

Each golden test builds its fixed inputs, collects the actual outputs into a `Vec<String>`, and compares them with a `GOLDEN` constant that starts empty. Example for `oauth1_sig.rs`, repeated in the same shape for the other modules:

```rust
    const GOLDEN_BASE_STRINGS: &[&str] = &[];

    #[test]
    fn golden_signature_base_string_awkward_chars() {
        let awkward = ["a b", "~", "+", "%", "*", "é", ""];
        let mut cases: Vec<Vec<(String, String)>> = awkward
            .iter()
            .map(|v| vec![("k".to_string(), (*v).to_string())])
            .collect();
        cases.push(vec![
            ("dup".to_string(), "1".to_string()),
            ("dup".to_string(), "2".to_string()),
        ]);
        let actual: Vec<String> = cases
            .iter()
            .map(|params| {
                signature_base_string("GET", "https://example.com:8443/a b/ü?x=1", params)
                    .expect("base string")
            })
            .collect();
        assert_eq!(actual, GOLDEN_BASE_STRINGS);
    }
```

The cases per test:

| Module | Test name | Fixed inputs |
|---|---|---|
| `oauth1_sig` | `golden_signature_base_string_awkward_chars` | the code above |
| `oauth1_sig` | `golden_authorization_header_awkward_chars` | realm `None`, `Some("a b")`, `Some("")`, and one param value for each awkward input |
| `oauth1_sig` | `golden_sign_hmac_sha1_fixed_nonce` | `HMAC-SHA1`, timestamp `"1700000000"`, nonce `"fixednonce"`, a body, extra params with awkward values |
| `digest_sig` | `golden_authorize_fixed_cnonce` | challenges for `qop=auth`, `qop=auth-int` with a body, no qop and `algorithm=SHA-256`; nc `1` and `2`; cnonce `"0a4f113b"`; username and password holding `"` and a backslash |
| `aws_sig` | `golden_sign_request_awkward_query_and_path_non_s3` | service `execute-api`, timestamp `"20130524T000000Z"`, query keys and values with each awkward input, a repeated key, an empty value, a path with `//`, a trailing `/` and an encoded segment |
| `aws_sig` | `golden_sign_request_awkward_query_and_path_s3` | the same URLs with service `s3` |
| `path_params` | `golden_encode_path_param_value_awkward` | `""`, `"a b"`, `"~"`, `"+"`, `"%"`, `"*"`, `"a/b"`, `"é"`, `"a%20b"` |
| `path_params` | `golden_substitute_path_params_awkward` | a colon param and a brace param whose values hold `/`, `?`, `#`, `%` and `é` |

The JWT message tests need no recording. Add them to `jwt.rs`:

```rust
    fn b64(s: &str) -> String {
        URL_SAFE_NO_PAD.encode(s.as_bytes())
    }

    fn invalid(token: &str) -> String {
        match decode_jwt(token) {
            Err(DomainError::InvalidInput(m)) => m,
            Err(other) => panic!("expected InvalidInput, got {other:?}"),
            Ok(_) => panic!("expected an error for {token}"),
        }
    }

    #[test]
    fn jwt_bad_header_encoding_message() {
        assert!(invalid("!!!.e30.sig").starts_with("Invalid JWT header encoding: "));
    }

    #[test]
    fn jwt_bad_header_json_message() {
        let token = format!("{}.e30.sig", b64("not json"));
        assert!(invalid(&token).starts_with("Invalid JWT header JSON: "));
    }

    #[test]
    fn jwt_bad_payload_encoding_message() {
        let token = format!("{}.!!!.sig", b64("{}"));
        assert!(invalid(&token).starts_with("Invalid JWT payload encoding: "));
    }

    #[test]
    fn jwt_bad_payload_json_message() {
        let token = format!("{}.{}.sig", b64("{}"), b64("not json"));
        assert!(invalid(&token).starts_with("Invalid JWT payload JSON: "));
    }

    #[test]
    fn jwt_empty_header_segment_message() {
        let json_err = serde_json::from_slice::<serde_json::Value>(b"")
            .expect_err("empty input is not JSON");
        assert_eq!(invalid("..sig"), format!("Invalid JWT header JSON: {json_err}"));
    }

    #[test]
    fn jwt_wrong_part_count_message_is_unchanged() {
        for token in ["a.b", "a.b.c.d"] {
            assert_eq!(invalid(token), "Invalid JWT: expected 3 dot-separated parts.");
        }
    }
```

- [ ] **Step 3: Run the golden tests to verify they fail with the actual values**

Run: `cargo test -j4 -p rocket-http golden`
Expected: each golden test FAILS with an `assert_eq!` diff whose left side is the actual output on the unchanged code. The `jwt_` tests PASS.

- [ ] **Step 4: Record the actual values**

Paste each failing test's left-hand list into its `GOLDEN_*` constant, in order. Do not edit any production file in this task.

- [ ] **Step 5: Run to verify they pass on the unchanged code**

Run: `cargo test -j4 -p rocket-http golden` and `cargo test -j4 -p rocket-http jwt_`
Expected: PASS, 8 golden tests and 6 new `jwt_` tests.

---

### Task 2: The util module, nonces and percent-encoding

**Files:**
- Create: `crates/rocket-http/src/util.rs`
- Modify: `crates/rocket-http/src/lib.rs`, `digest_sig.rs`, `oauth1_sig.rs`, `aws_sig.rs`, `path_params.rs`
- Test: inline tests in `util.rs`, `digest_sig.rs`, `oauth1_sig.rs`, `path_params.rs`, plus the Task 1 goldens

**Interfaces:**
- Consumes: the Task 1 golden tests (they must stay green).
- Produces:
  - `pub(crate) fn rand_alnum(n: usize) -> String` in `util.rs` (n ASCII letters and digits).
  - `pub(crate) fn percent_encode(input: &str) -> String` in `util.rs` (the same output as `urlencoding::encode(input).into_owned()`).
  - `generate_cnonce()` and `generate_nonce()` call `rand_alnum(16)` and `rand_alnum(32)`. `oauth1_sig::percent_encode` and `path_params::encode_path_param_value` call `util::percent_encode`. The three inline sites in `aws_sig.rs` call `util::percent_encode`.

- [ ] **Step 1: Read the OpenCollection reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Nothing in this task changes collection data.

- [ ] **Step 2: Write the failing tests**

Create `crates/rocket-http/src/util.rs` with only the test module, and add `mod util;` to `lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const AWKWARD: &[&str] = &[
        "", "%", "a b", "~", "+", "*", "a/b", "é", "a%20b", "\u{1F680}",
    ];

    #[test]
    fn rand_alnum_has_the_requested_length() {
        for n in [1, 16, 32, 64] {
            assert_eq!(rand_alnum(n).len(), n);
        }
    }

    #[test]
    fn rand_alnum_zero_is_empty() {
        assert_eq!(rand_alnum(0), "");
    }

    #[test]
    fn rand_alnum_is_only_ascii_letters_and_digits() {
        assert!(rand_alnum(256).chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn rand_alnum_two_calls_differ() {
        assert_ne!(rand_alnum(32), rand_alnum(32));
    }

    #[test]
    fn percent_encode_matches_urlencoding_for_awkward_inputs() {
        for input in AWKWARD {
            assert_eq!(percent_encode(input), urlencoding::encode(input).into_owned(), "input: {input:?}");
        }
    }
}
```

Add to `digest_sig.rs` and `oauth1_sig.rs` (the first with `generate_cnonce` and 16, the second with `generate_nonce` and 32), and the wrapper tests:

```rust
    #[test]
    fn generate_nonce_is_32_ascii_alphanumeric_and_unique() {
        let all: Vec<String> = (0..50).map(|_| generate_nonce()).collect();
        assert!(all.iter().all(|n| n.len() == 32 && n.chars().all(|c| c.is_ascii_alphanumeric())));
        let unique: std::collections::HashSet<_> = all.iter().collect();
        assert_eq!(unique.len(), all.len());
    }

    #[test]
    fn public_percent_encode_still_matches_the_helper() {
        for input in ["", "%", "a b", "~", "+", "*", "a/b", "é"] {
            assert_eq!(percent_encode(input), crate::util::percent_encode(input));
        }
    }
```

In `digest_sig.rs` the nonce test is `generate_cnonce_is_16_ascii_alphanumeric_and_unique`. In `path_params.rs` add `public_encode_path_param_value_still_matches_the_helper` with the same loop over `encode_path_param_value`.

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-http util`
Expected: FAIL to compile with `cannot find function 'rand_alnum' in this scope` and `cannot find function 'percent_encode' in this scope`.

- [ ] **Step 4: Implement the module and the call sites**

Implement `rand_alnum` and `percent_encode` in `util.rs` above the tests, moving the existing `rand::thread_rng().sample_iter(&Alphanumeric)` pipeline and the `urlencoding::encode` call into them. Make the five call sites use them as listed in Interfaces, and remove the `rand` imports that become unused in `digest_sig.rs` and `oauth1_sig.rs`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-http util`, `cargo test -j4 -p rocket-http generate_`, `cargo test -j4 -p rocket-http public_`, `cargo test -j4 -p rocket-http golden`, then `cargo check -j4 -p rocket-http`
Expected: PASS, with the Task 1 goldens unchanged, and no warning.

---

### Task 3: JWT segment decoding and final checks

**Files:**
- Modify: `crates/rocket-http/src/jwt.rs`
- Test: the Task 1 JWT tests plus new `decode_segment_*` tests in `jwt.rs`

**Interfaces:**
- Consumes: the Task 1 JWT message tests.
- Produces: `fn decode_segment(part: &str, label: &str) -> Result<serde_json::Value, DomainError>` in `jwt.rs` (private). It decodes with `URL_SAFE_NO_PAD`, parses JSON, and builds `Invalid JWT {label} encoding: {e}` and `Invalid JWT {label} JSON: {e}` as `DomainError::InvalidInput`.

- [ ] **Step 1: Read the OpenCollection reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Nothing in this task changes collection data.

- [ ] **Step 2: Write the failing `decode_segment` tests**

```rust
    fn segment_error(part: &str, label: &str) -> String {
        match decode_segment(part, label) {
            Err(DomainError::InvalidInput(m)) => m,
            Err(other) => panic!("expected InvalidInput, got {other:?}"),
            Ok(v) => panic!("expected an error, got {v}"),
        }
    }

    #[test]
    fn decode_segment_returns_the_parsed_value() {
        let value = decode_segment(&b64(r#"{"a":1}"#), "header").expect("decode segment");
        assert_eq!(value, serde_json::json!({"a": 1}));
    }

    #[test]
    fn decode_segment_bad_encoding_uses_the_label() {
        assert!(segment_error("!!!", "header").starts_with("Invalid JWT header encoding: "));
        assert!(segment_error("!!!", "payload").starts_with("Invalid JWT payload encoding: "));
    }

    #[test]
    fn decode_segment_bad_json_uses_the_label() {
        let part = b64("not json");
        assert!(segment_error(&part, "header").starts_with("Invalid JWT header JSON: "));
        assert!(segment_error(&part, "payload").starts_with("Invalid JWT payload JSON: "));
    }

    #[test]
    fn decode_segment_empty_part_is_a_json_error() {
        assert!(segment_error("", "header").starts_with("Invalid JWT header JSON: "));
    }

    #[test]
    fn decode_segment_accepts_a_json_array_and_null() {
        assert_eq!(decode_segment(&b64("[]"), "payload").expect("array"), serde_json::json!([]));
        assert_eq!(decode_segment(&b64("null"), "payload").expect("null"), serde_json::Value::Null);
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-http decode_segment`
Expected: FAIL to compile with `cannot find function 'decode_segment' in this scope`.

- [ ] **Step 4: Implement `decode_segment` and use it in `decode_jwt`**

Add `decode_segment` to `jwt.rs`, and replace the two inline decode-and-parse blocks (header with label `"header"`, payload with label `"payload"`). Keep the part-count check and its message as they are.

- [ ] **Step 5: Run the whole crate and check**

Run: `cargo test -j4 -p rocket-http` and `cargo check -j4 -p rocket-http`
Expected: PASS, including the Task 1 goldens and the six `jwt_` message tests, with no warning from the new code.

- [ ] **Step 6: Confirm the duplicates are gone**

Run: `grep -n "Alphanumeric\|urlencoding::encode" crates/rocket-http/src/*.rs`
Expected: the only production hits are in `util.rs` (test code may still call `urlencoding::encode` as the reference).

Next plan: none
