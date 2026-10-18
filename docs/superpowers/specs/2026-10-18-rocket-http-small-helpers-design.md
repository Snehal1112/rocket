# rocket-http small helpers design

Ticket: https://github.com/Snehal1112/rocket/issues/75

## Goal

Remove three small duplicates in the `rocket-http` crate without changing any behaviour: random alphanumeric strings, percent-encoding and JWT segment decoding each exist once afterwards, and every signer produces the same output as before.

## Background

Audit ticket #75 (T2.20, size S, crate `rocket-http`). Duplicates found: `digest_sig::generate_cnonce` (16 characters) and `oauth1_sig::generate_nonce` (32 characters) are the same random alphanumeric pipeline. `urlencoding::encode(x).into_owned()` appears in `oauth1_sig::percent_encode`, three times in `aws_sig.rs` (query key, query value, path segment) and in `path_params::encode_path_param_value`. `jwt.rs` decodes the header and the payload with a base64url decode, a JSON parse and near-identical error text. Intentionally left alone: HMAC wrappers, random-byte generators and SHA-256 helpers that differ in type or output encoding. Ticket #71 overlaps on `percent_encode` and is not part of this spec.

## Non-goals

- Any change to public behaviour, signer output, error text or on-disk or IPC formats.
- Merging the HMAC wrappers, byte generators or SHA-256 helpers.
- Work from #71 (moving OAuth2 URL building into `rocket-http`).

## Design

- A new crate-private `util` module in `rocket-http` holds `rand_alnum(n: usize) -> String` and `percent_encode(input: &str) -> String`.
- `generate_cnonce` (16) and `generate_nonce` (32) call `rand_alnum` and stay public. `oauth1_sig::percent_encode` and `path_params::encode_path_param_value` stay public as thin wrappers over `util::percent_encode`. The three inline sites in `aws_sig.rs` call `util::percent_encode` directly.
- `jwt.rs` gets a local `decode_segment(part, label)` that base64url-decodes and parses one segment. It produces exactly today's four messages: `Invalid JWT header encoding: {e}`, `Invalid JWT header JSON: {e}`, `Invalid JWT payload encoding: {e}` and `Invalid JWT payload JSON: {e}`.
- Tests are written before the refactor, and the old code is the reference: for each signer, golden tests with fixed inputs whose expected values are recorded from the code as it is before any change.

## Constraints

- No panicking unwrap calls in production paths. No `git` CLI. No new dependency.
- Cargo commands are per crate with `-j4`: `cargo check -j4 -p rocket-http` and `cargo test -j4 -p rocket-http`.


## Decisions

- Where do the shared helpers live, and what happens to the existing public names? Options: a crate-private `util` module with thin public wrappers kept; a public `util` module re-exported; delete the old public names and update callers. Chosen: a crate-private `util` module holding `rand_alnum(n)` and `percent_encode`, with `generate_cnonce`, `generate_nonce` and `oauth1_sig::percent_encode` kept as thin wrappers so no public API changes. Decided by: user
- Which `percent_encode` sites use the shared helper, and what happens to the JWT error text? Options: all five sites use `util::percent_encode` with `encode_path_param_value` kept as a public wrapper and `decode_segment(part, label)` keeping today's messages; same but standardise the JWT wording; leave `path_params.rs` alone. Chosen: all five sites (`oauth1_sig::percent_encode`, the three inline ones in `aws_sig.rs`, `path_params::encode_path_param_value`) use `util::percent_encode`, `encode_path_param_value` stays as a public wrapper, and `jwt.rs` gets a local `decode_segment(part, label)` that produces exactly the messages used today. Decided by: user
- How much new test coverage? Options: a few pinning tests; rely on the existing tests only; broader byte-for-byte signer comparison. Chosen: the broader comparison. Because the old code is gone after the refactor, it is done as golden tests: before touching any code, record the current output of each signer for fixed inputs, assert it in tests, and keep the tests green through the refactor. Where a signer reads the clock or randomness itself, the golden test covers the deterministic part (the canonical request or base string). Decided by: user
- Is the overlap with #71 handled here? Chosen: no, `percent_encode` here is only the crate-private helper, and #71 stays a separate ticket. Decided by: call

- Where do the golden tests live, and may they add seams? Chosen: tests live inside each module (`#[cfg(test)]`), so they can reach private functions such as the AWS `canonical_query` and path encoding. No clock or randomness parameter is added, because that would be a behaviour-adjacent code change. Decided by: call
- Which inputs do the golden tests use? Chosen: they include awkward encoding inputs: `~`, space, `+`, `%`, `*`, `/` in a path segment, and non-ASCII text, in addition to plain values. Decided by: call
- What does `decode_segment` return? Chosen: `Result<serde_json::Value, DomainError>`, because header and payload are both parsed as `serde_json::Value` today. Decided by: call
- Is the module name free? Chosen: `util` is declared as a private `mod util;` in `lib.rs`, and no `util` exists in the crate today. Decided by: call

## Acceptance criteria

- `rand_alnum`, `percent_encode` and the JWT segment decode each exist once in `rocket-http`, and no inline `thread_rng().sample_iter(&Alphanumeric)` pipeline or inline `urlencoding::encode(..).into_owned()` remains outside `util`.
- `generate_cnonce` still returns 16 characters, `generate_nonce` 32, both only ASCII letters and digits, and two calls differ.
- `oauth1_sig::percent_encode` and `path_params::encode_path_param_value` keep their public signatures and output.
- The four JWT error messages are exactly the ones listed in the Design section, pinned by tests.
- Golden tests recorded from the code before the refactor pass unchanged for the OAuth1, Digest and AWS signers and for path parameter encoding, and their inputs include `~`, space, `+`, `%`, `*`, `/` in a path segment and non-ASCII text.
- The existing `rocket-http` tests pass, and `cargo check -j4 -p rocket-http` is clean.
