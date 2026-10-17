# Dedup T2.21 Plan 1: Precedence, Snapshot Helpers, invalid() Reuse

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove three duplications in `rocket-environment` and `rocket-collection` without changing behaviour.

**Architecture:** Pure refactors inside existing files. A precedence test lands before the `flatten` refactor. No new crates, no public API changes beyond `pub(crate)`.

**Tech Stack:** Rust, `cargo test -j4 -p <crate>`.

**Spec:** GitHub issue #76 (T2.21). Index: `2026-10-17-dedup-t2-21-index.md`.

## Global Constraints

- Behaviour, on-disk formats and IPC contracts must not change.
- Cargo commands always pass `-j4`; targeted `-p <crate>` tests only, never `--workspace`.
- No unwrap calls in production paths. No `git add -A`, `--all` or `.`; add explicit paths.
- Commit with the `dev-workflow-skills:1-git-commit` skill, conventional message (`refactor:` or `test:`).
- Comments are short full sentences ending in a period.

## Review Focus

- `flatten_with_process_env` must keep `process.env.*` as the lowest priority, so a user variable literally named `process.env.X` still wins (existing test `process_env_shadowed_by_same_prefixed_user_var`).
- Truncation must stay byte-length based for the `> 8` check and char based for the `take(8)`, with the `…` suffix, so `"supersec…"` and a 9-byte multibyte token behave as before.
- Disabled headers, query params and form fields stay excluded from the snapshot.

---

### Task 1: One precedence chain for flatten

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (variable scope).

**Files:**
- Modify: `crates/rocket-environment/src/context.rs` (impl at lines 30-61, tests module)

**Interfaces:**
- Consumes: nothing.
- Produces: unchanged public `VariableContext::flatten(&self) -> HashMap<String, String>` and `flatten_with_process_env(&self) -> HashMap<String, String>`.

- [ ] **Step 1: Add the pinning test `precedence_order_is_pinned_for_both_flatten_variants`** in the `tests` module. Layers lowest to highest: `global_env`, `collection`, `external_secrets`, `env`, `folder`, `request`, `runtime`. For each index `i`, build a context where layers `0..=i` all hold key `"k"` set to the layer's name, then assert both `flatten()` and `flatten_with_process_env()` return the layer-`i` name for `"k"`. Also assert `flatten_with_process_env()` returns `"process"` for `"process.env.k"` when `process_env` holds `k`.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-environment precedence_order_is_pinned` and confirm PASS (it pins current behaviour, so it passes before the refactor).
- [ ] **Step 3: Refactor.** Make `flatten_with_process_env` build the `process.env.`-prefixed map first, then `out.extend(self.flatten())`. Update the doc comments so the order is written once, on `flatten`.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-environment` and confirm all PASS.
- [ ] **Step 5: Commit** with the git-commit skill. Paths: `crates/rocket-environment/src/context.rs`. Suggested type: `refactor:`.

---

### Task 2: Snapshot helpers

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-collection/src/contract/snapshot.rs` (`from_request` lines 47-81, `auth_detail` 126-144, `extract_form_fields` 171-192)

**Interfaces:**
- Consumes: `rocket_shared::types::{Header, QueryParam, FormDataEntry}` field shapes (`key`, `value`, `enabled`).
- Produces (private, in `snapshot.rs`):
  - `fn enabled_entries<'a, I>(items: I) -> Vec<KeyValueEntry>` where `I: IntoIterator<Item = (&'a str, &'a str, bool)>`, returning key/value pairs for items whose bool is true, in order.
  - `fn truncate_secret(value: &str) -> String` returning `value` unchanged when `value.len() <= 8`, otherwise the first 8 chars followed by `…`.

- [ ] **Step 1: Add tests** in the existing `tests` module: `truncate_secret_keeps_short_values` (`"short"` and an 8-byte value unchanged), `truncate_secret_cuts_long_values` (`"supersecrettoken"` gives `"supersec…"`), `enabled_entries_skips_disabled_and_keeps_order`. Existing tests for headers, query params, form fields and bearer detail already pin the call-site behaviour.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-collection snapshot` and confirm the new tests FAIL to compile (helpers missing).
- [ ] **Step 3: Implement both helpers** and use them: `enabled_entries` for headers, query params and form fields (map each source item to a `(&str, &str, bool)` tuple); `truncate_secret` in the `Auth::Bearer` arm and the `Auth::ApiKey` arm. The `len() > 8` byte check and `chars().take(8)` stay exactly as they are inside the helper.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-collection` and confirm all PASS.
- [ ] **Step 5: Commit** with the git-commit skill. Paths: `crates/rocket-collection/src/contract/snapshot.rs`.

---

### Task 3: Reuse invalid() in external_secret

**Files:**
- Modify: `crates/rocket-environment/src/client_certificate_validation.rs:214` (visibility only)
- Modify: `crates/rocket-environment/src/external_secret.rs:33-67`

**Interfaces:**
- Consumes: `fn invalid(message: String) -> DomainError` (existing, private).
- Produces: the same function as `pub(crate) fn invalid`.

- [ ] **Step 1: Confirm the pin.** The tests `validate_bindings_rejects_empty_dotted_or_duplicate_alias` and `validate_bindings_rejects_missing_connection_or_vault` already assert `InvalidInput`. Add `validate_bindings_messages_are_unchanged` asserting the exact message text for the empty-alias case (`"external secret binding alias must not be empty"`) and the dotted-alias case (`"external secret alias 'pay.ments' may only contain letters, digits, '_' or '-'"`).
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-environment external_secret` and confirm PASS before the change.
- [ ] **Step 3: Change `invalid` to `pub(crate)`.** Import it in `external_secret.rs` via `crate::client_certificate_validation::invalid` and replace every `DomainError::InvalidInput(..)` in `validate_external_secret_bindings` with `invalid(..)`. Keep `DomainError` imported only if still used elsewhere in the file; otherwise drop it from the production imports (the tests module keeps its own use via `super::*`, so re-import there if needed).
- [ ] **Step 4: Run** `cargo check -j4 -p rocket-environment` and `cargo test -j4 -p rocket-environment`, confirm PASS with no new warnings.
- [ ] **Step 5: Commit** with the git-commit skill. Paths: both files above.
