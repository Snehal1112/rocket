# Dedup T2.21: rocket-environment and rocket-collection local cleanups

Ticket: https://github.com/Snehal1112/rocket/issues/76 (T2.21, tier 2, size S). Part of the duplicate-code review of 2026-10-07. This spec covers the required items of the ticket only.

## Goal

Remove the repeated precedence merge, the repeated snapshot entry building and secret truncation, and the repeated error construction in rocket-environment and rocket-collection, with no change in behavior.

## Background

Verified against `main` at `d73582e1`, all items of the ticket still exist:

- `crates/rocket-environment/src/context.rs`: `flatten` (about lines 34-44) and `flatten_with_process_env` (about lines 47-60) both write the seven `extend` lines `global_env`, `collection`, `external_secrets`, `env`, `folder`, `request`, `runtime`. The second function first inserts every `process_env` entry under the key `process.env.<name>`. Sixteen tests in the same file already pin precedence, including `process_env_shadowed_by_same_prefixed_user_var`.
- `crates/rocket-collection/src/contract/snapshot.rs`: the headers block, the query params block and `extract_form_fields` each repeat `iter().filter(enabled).map(KeyValueEntry { key, value }).collect()`. `auth_detail` truncates the Bearer token and the ApiKey value with the same `if s.len() > 8 { first 8 chars + ellipsis } else { s }` code. The length check counts bytes while the cut counts characters. There is a test for Bearer truncation and none for ApiKey.
- `crates/rocket-environment/src/client_certificate_validation.rs` has a private `fn invalid(message: String) -> DomainError`. `crates/rocket-environment/src/external_secret.rs` spells `DomainError::InvalidInput(..)` out five times in `validate_external_secret_bindings`. The tests of `external_secret.rs` check only that an error is returned, so no message text is pinned.
- Precedence order (OpenCollection spec reference, section 6, plus the external secrets layer of this repo): runtime, then request, then folder, then environment, then external secrets, then collection, then global environment, then process environment (only through `{{process.env.NAME}}`).

## Decisions

Each topic lists the question that was asked and the answer given.

- Scope. Question: how much of the ticket should this job cover (required items only, plus shared name validation, or everything including the `QueryParam` and `Header` merge)? Answer: required items only. The optional name validation helpers and the type merge are left out.
- Approach for the precedence merge. Question: call `flatten` and add process env afterwards, share an ordered list of layers, or use a macro? Answer: share an ordered list of layers. A private method `layers` returns the seven maps in precedence order, and both functions fold over it. Process env is simply the lowest layer, so no second merge rule exists. The macro option was rejected because precedence would stay written twice. The "call `flatten` and extend" option was rejected for this job because a plain `extend` would let process env beat a user variable, and a no-overwrite insert would be a second merge rule.
- Truncation. Question: keep the byte-length quirk or fix it to count characters? Answer: keep the behavior exactly, pin it with a test, and record the quirk as a follow-up. Fixing it would be a behavior change in a refactor ticket.
- Error messages. Question: keep the message text and pin it with tests, or only keep it? Answer: keep the text exactly and add tests that pin the five messages before the code changes.
- Home of the error helper. Question: crate-local `pub(crate)` helper or a new constructor on the shared error type? Answer: `pub(crate)` in rocket-environment. This keeps rocket-shared and every public API unchanged.
- Purpose and who benefits. Question: why do this and for whom? Answer: for maintainers. Precedence is written once, the snapshot and error code have one shape each, and a later change cannot diverge between two copies. Users see no change.
- Constraints. Question: which rules bind the work? Answer: the precedence order above is unchanged; rocket-environment and rocket-collection depend only on rocket-shared; no `unwrap` in production code; every cargo command uses `-j4`; targeted tests only, never `--workspace`.
- Success checks and how each is measured. Question: how will we know it is done? Answer: the acceptance criteria below, measured by the named tests and by `cargo check -j4` plus the targeted test runs.
- Edge cases and failure behavior. Question: which edge cases matter? Answer: an empty process environment, a process-env key that collides with a user variable (already pinned), a disabled entry (skipped), a secret of exactly 8 bytes and a short non-ASCII secret (pinned as is). No new failure path is added.
- Security and compatibility. Question: what must not weaken? Answer: secret values stay truncated in the contract snapshot; error messages never echo key text; on-disk formats, IPC contracts and the callers in rocket-app and rocket-infra do not change.
- Scope against related tickets. Question: which tickets or branches overlap? Answer: #55 (the single `Collection::validate_name` rule) and #70 (a shared `default_true`) are not touched; no open branch works on these files.
- Non-goals and rejected alternatives. See the Non-goals section. Rejected: shared name validation helpers, the `QueryParam`/`Header` merge, a `DomainError` constructor in rocket-shared, and fixing the truncation quirk.

## Non-goals

- Shared name validation (`non_empty_name`, `is_slug_char`) and any change to `Collection::validate_name`, `Workspace::validate_name`, `validate_component`, `Environment::validate_name` or the alias check in `external_secret.rs`.
- Merging `QueryParam` and `Header`, and any change to `GrpcMetadataEntry`.
- Fixing the byte-versus-character truncation quirk.
- Any change to the behavior of `client_certificate_validation.rs` beyond moving the `invalid` helper.
- Reducing the number of clones in `flatten`. The new fold clones values exactly as the old code did.
- Any change to rocket-app, rocket-infra, rocket-shared or src-tauri.

## Behavior that must not change

- `flatten()` and `flatten_with_process_env()` return the same map for every input as before, including key collisions between layers.
- Process env entries appear only under the `process.env.` prefix and lose to any user variable with the same key.
- The contract snapshot output for headers, query params, form fields, Bearer and ApiKey is byte for byte the same, including the ellipsis rule.
- Every error returned by `validate_external_secret_bindings` and `validate_client_certificates` keeps its variant and its message text.

## Design

### 1. Precedence merge in `context.rs`

A private method `layers(&self)` returns the seven maps in precedence order, lowest first: `global_env`, `collection`, `external_secrets`, `env`, `folder`, `request`, `runtime`. It is the only place that order is written. `flatten()` starts from an empty map and folds the layers into it. `flatten_with_process_env()` first inserts the `process.env.`-prefixed entries, then folds the same layers. Values are cloned exactly as before. The public API, the field names and the doc comments stay as they are.

### 2. Snapshot helpers in `snapshot.rs`

One private helper builds a `Vec<KeyValueEntry>` from the enabled items of a slice, using a small accessor that returns `(enabled, key, value)`. Headers, query params and form fields each call it. One private helper shortens a secret: if `s.len() > 8` it returns the first 8 characters followed by an ellipsis, otherwise `s` unchanged, keeping the byte-length check exactly. Bearer and ApiKey both call it.

### 3. Error helper

A small new module in rocket-environment holds `pub(crate) fn invalid(message: impl Into<String>) -> DomainError`, returning `DomainError::InvalidInput`. It moves out of `client_certificate_validation.rs`, whose call sites keep working unchanged. `external_secret.rs` uses it for its five errors, with identical message text. The module map in `crates/rocket-environment/CLAUDE.md` gets one line for the new module.

## Tests and measurement

Order of work in each piece: add the pinning tests, run them green against the old code, refactor, run them again.

- context: all sixteen existing precedence tests stay green; a new test checks that `flatten_with_process_env()` equals `flatten()` plus the prefixed process entries for a mix of colliding and non-colliding keys.
- snapshot: the existing snapshot tests stay green; new tests pin ApiKey truncation, a short non-ASCII secret (the quirk, with a comment saying it is intentional for now), and a secret of exactly 8 bytes.
- errors: new tests assert the five exact messages in `external_secret.rs`; the existing certificate validation tests stay green.
- Verification commands: `cargo check -j4`, `cargo test -j4 -p rocket-environment`, `cargo test -j4 -p rocket-collection`, `cargo test -j4 -p rocket-shared`.
- Plan layout: one plan with three tasks: (1) context.rs and its test, (2) snapshot.rs and its tests, (3) the error helper, its tests and the docs line.

## Acceptance criteria

- The precedence order is written once in `context.rs`, and `flatten` and `flatten_with_process_env` both use it.
- The precedence tests pass: the sixteen existing tests plus the new equality test.
- The enabled-entries building in `snapshot.rs` exists once and is used for headers, query params and form fields.
- The secret shortening in `snapshot.rs` exists once and is used for Bearer and ApiKey, with tests that pin its current output for ASCII, exactly 8 bytes and short non-ASCII input.
- `invalid` lives in one place in rocket-environment and `external_secret.rs` no longer spells out `DomainError::InvalidInput`.
- The five `external_secret.rs` messages are pinned by tests that pass before and after the change.
- `cargo check -j4` is clean, and the targeted tests of rocket-environment, rocket-collection and rocket-shared pass.
- No file outside crates/rocket-environment, crates/rocket-collection and the docs changes; no public API change.
