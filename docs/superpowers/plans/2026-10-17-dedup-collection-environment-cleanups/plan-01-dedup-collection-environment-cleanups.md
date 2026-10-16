# Dedup T2.21: Collection and Environment Cleanups Implementation Plan

> **For agentic workers:** This plan is executed by a `factory-builder` subagent through `superpowers:executing-plans`, skipping that skill's final whole-branch review, ledger scripts and progress file. The factory driver commits: skip every commit step and never run `git add` or `git commit`.

**Goal:** Remove the repeated precedence merge, the repeated snapshot entry building and secret shortening, and the repeated error construction in rocket-environment and rocket-collection, with no change in behavior.

**Architecture:** In `rocket-environment`, a private `layers()` method lists the seven scope maps in precedence order and both `flatten` functions fold over it. In `rocket-collection`, two private helpers in `contract/snapshot.rs` build the enabled key/value entries and shorten secrets. A crate-local `invalid()` helper in a new `rocket-environment` module replaces the spelled-out `DomainError::InvalidInput` in `external_secret.rs`. Every piece adds tests that pin the current behavior first, runs them green against the old code, then refactors.

**Tech Stack:** Rust (crates `rocket-environment`, `rocket-collection`, `rocket-shared` types). Cargo with `-j4`.

**Spec:** `docs/superpowers/specs/2026-10-17-dedup-collection-environment-cleanups-design.md`

## Global Constraints

- Always pass `-j4` to cargo. Use targeted `cargo test -j4 -p <crate> <filter>`. Never `--workspace` or `--all`. Never set `CARGO_TARGET_DIR`.
- Pure refactor: behavior, on-disk formats, IPC contracts and every public API stay exactly as they are. No file outside `crates/rocket-environment` and `crates/rocket-collection` changes.
- No unwrap calls and no expect calls in production code. Tests may use `expect`.
- Comments are short full sentences that end with a punctuation mark. No emojis.
- `rocket-environment` and `rocket-collection` depend only on `rocket-shared`. No new dependency.
- Do not run `git add`, `git commit` or any other git write command. The driver commits.
- Work only in your worktree. Read `crates/rocket-environment/CLAUDE.md` and `crates/rocket-collection/CLAUDE.md` first.

## Review Focus

- A process-env entry must never beat a user variable with the same prefixed key. Pinned by the existing `process_env_shadowed_by_same_prefixed_user_var` and the new `process_env_is_the_lowest_layer_and_the_rest_matches_flatten`.
- An empty context and an empty process environment still give an empty map. Pinned by the existing `empty_is_empty`.
- A secret of exactly 8 bytes gets no ellipsis, and a short non-ASCII secret (more than 8 bytes, 8 characters or fewer) still gets an ellipsis, because the length check counts bytes. Pinned by `auth_detail_secret_length_edges_are_kept` in Task 2.
- ApiKey truncation keeps the `key=value (placement)` shape. Pinned by `auth_detail_apikey_truncates_the_value` in Task 2.
- Disabled headers, query params and form fields are skipped and order is kept. Pinned by the existing `from_request_skips_disabled_headers_and_params` and `from_request_skips_disabled_form_fields`.
- The five `external_secret.rs` messages stay character for character. Pinned by `validate_bindings_messages_are_pinned` in Task 3.
- No unused import warning after the helper moves. Checked by `cargo check -j4` in Task 3.

---

### Task 1: One ordered layer list for flatten

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (the variable scope and precedence section).

**Files:**
- Modify: `crates/rocket-environment/src/context.rs` (the two functions at the top, tests at the bottom)

**Interfaces:**
- Produces: private `VariableContext::layers(&self) -> [&HashMap<String, String>; 7]` and private `VariableContext::merge_layers_into(&self, out: &mut HashMap<String, String>)`. `flatten` and `flatten_with_process_env` keep their public signatures.

- [ ] **Step 1: Add the pinning test**

In the `tests` module of `crates/rocket-environment/src/context.rs`, after the `process_env_uses_dotted_key` test, add:

```rust
    #[test]
    fn process_env_is_the_lowest_layer_and_the_rest_matches_flatten() {
        let ctx = VariableContext {
            process_env: m(&[("PATH", "/bin"), ("shared", "from-process")]),
            global_env: m(&[("process.env.shared", "from-global"), ("g", "1")]),
            collection: m(&[("c", "1")]),
            runtime: m(&[("r", "1")]),
            ..Default::default()
        };
        let mut expected = ctx.flatten();
        expected
            .entry("process.env.PATH".to_string())
            .or_insert_with(|| "/bin".to_string());
        expected
            .entry("process.env.shared".to_string())
            .or_insert_with(|| "from-process".to_string());
        let flat = ctx.flatten_with_process_env();
        assert_eq!(flat, expected);
        assert_eq!(flat.get("process.env.shared").expect("present"), "from-global");
    }
```

- [ ] **Step 2: Run the pinning tests against the old code**

Run: `cargo test -j4 -p rocket-environment context::`
Expected: PASS (all existing precedence tests plus the new one). The new test passes on the old code because it pins current behavior.

- [ ] **Step 3: Refactor the two functions**

Replace the whole `impl VariableContext { ... }` block (the `flatten` and `flatten_with_process_env` functions, lines 30-61) with:

```rust
impl VariableContext {
    /// The scope maps in precedence order, lowest first. This is the only place the order is written.
    /// Later layers overwrite earlier ones on key collision. `process_env` is not a layer here:
    /// `flatten_with_process_env` adds it below all of these.
    fn layers(&self) -> [&HashMap<String, String>; 7] {
        [
            &self.global_env,
            &self.collection,
            &self.external_secrets,
            &self.env,
            &self.folder,
            &self.request,
            &self.runtime,
        ]
    }

    /// Folds every layer into `out`, lowest precedence first.
    fn merge_layers_into(&self, out: &mut HashMap<String, String>) {
        for layer in self.layers() {
            out.extend(layer.clone());
        }
    }

    /// Merge all scopes except process_env.
    /// Insertion order: global_env → collection → external_secrets → env → folder → request → runtime.
    /// Later layers overwrite earlier on key collision.
    pub fn flatten(&self) -> HashMap<String, String> {
        let mut out = HashMap::new();
        self.merge_layers_into(&mut out);
        out
    }

    /// Same as flatten() but also inserts process env vars with "process.env." prefix (lowest priority).
    pub fn flatten_with_process_env(&self) -> HashMap<String, String> {
        let mut out = HashMap::new();
        for (k, v) in &self.process_env {
            out.insert(format!("process.env.{}", k), v.clone());
        }
        self.merge_layers_into(&mut out);
        out
    }
}
```

- [ ] **Step 4: Run the tests again**

Run: `cargo test -j4 -p rocket-environment context::`
Expected: PASS, the same tests as in Step 2.

- [ ] **Step 5: Check the crate**

Run: `cargo check -j4 -p rocket-environment`
Expected: no errors and no new warnings.

---

### Task 2: Snapshot helpers for entries and secret shortening

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-collection/src/contract/snapshot.rs` (`from_request`, `auth_detail`, `extract_form_fields`, tests)

**Interfaces:**
- Produces: private `enabled_entries<T>(items: &[T], parts: impl Fn(&T) -> (bool, &str, &str)) -> Vec<KeyValueEntry>` and private `shorten_secret(s: &str) -> String` in `snapshot.rs`.

- [ ] **Step 1: Add the pinning tests**

In the `tests` module of `snapshot.rs`, after `from_request_auth_detail_short_bearer`, add:

```rust
    #[test]
    fn auth_detail_apikey_truncates_the_value() {
        use rocket_shared::types::Auth;
        let req = Request::new("Get", HttpMethod::Get, "/x").with_auth(Auth::ApiKey {
            key: "X-Key".into(),
            value: "abcdefghijkl".into(),
            placement: "header".into(),
        });
        let snap = RequestSignatureSnapshot::from_request("x.yml", &req);
        assert_eq!(snap.auth_detail, "X-Key=abcdefgh… (header)");
        let short = Request::new("Get", HttpMethod::Get, "/x").with_auth(Auth::ApiKey {
            key: "k".into(),
            value: "abc".into(),
            placement: "query".into(),
        });
        let snap = RequestSignatureSnapshot::from_request("x.yml", &short);
        assert_eq!(snap.auth_detail, "k=abc (query)");
    }

    #[test]
    fn auth_detail_secret_length_edges_are_kept() {
        use rocket_shared::types::Auth;
        let detail = |token: &str| {
            let req = Request::new("Get", HttpMethod::Get, "/x").with_auth(Auth::Bearer {
                token: token.into(),
            });
            RequestSignatureSnapshot::from_request("x.yml", &req).auth_detail
        };
        // Exactly 8 bytes: no ellipsis.
        assert_eq!(detail("12345678"), "12345678");
        // 9 bytes: first 8 characters and an ellipsis.
        assert_eq!(detail("123456789"), "12345678…");
        // Five characters but ten bytes: the length check counts bytes, so the ellipsis is
        // added although nothing was cut. This is the current behavior and it is kept on purpose.
        assert_eq!(detail("ééééé"), "ééééé…");
        // Nine characters: the first eight characters and an ellipsis.
        assert_eq!(detail("ééééééééé"), "éééééééé…");
    }
```

- [ ] **Step 2: Run the pinning tests against the old code**

Run: `cargo test -j4 -p rocket-collection contract::snapshot`
Expected: PASS (existing snapshot tests plus the two new ones).

- [ ] **Step 3: Add the two helpers**

In `snapshot.rs`, directly after the `http_method_name` function (before `auth_type_name`), add:

```rust
/// Builds key and value entries from the enabled items of a list.
/// `parts` returns `(enabled, key, value)` of one item. Disabled items are skipped.
fn enabled_entries<T>(items: &[T], parts: impl Fn(&T) -> (bool, &str, &str)) -> Vec<KeyValueEntry> {
    items
        .iter()
        .filter_map(|item| {
            let (enabled, key, value) = parts(item);
            enabled.then(|| KeyValueEntry {
                key: key.to_string(),
                value: value.to_string(),
            })
        })
        .collect()
}

/// Shortens a secret for the snapshot: more than 8 bytes keeps the first 8 characters and adds
/// an ellipsis. The length check counts bytes and the cut counts characters, as it always did.
fn shorten_secret(s: &str) -> String {
    if s.len() > 8 {
        format!("{}…", s.chars().take(8).collect::<String>())
    } else {
        s.to_string()
    }
}
```

- [ ] **Step 4: Use the helpers in `from_request`**

Replace the `headers:` and `query_params:` fields of `from_request` (the two multi-line `.iter().filter(...).map(...).collect()` blocks) with:

```rust
            headers: enabled_entries(&request.headers, |h| {
                (h.enabled, h.key.as_str(), h.value.as_str())
            }),
            query_params: enabled_entries(&request.query_params, |q| {
                (q.enabled, q.key.as_str(), q.value.as_str())
            }),
```

- [ ] **Step 5: Use the helpers in `auth_detail`**

Replace the `Auth::Bearer { token } => { ... }` arm with:

```rust
        Auth::Bearer { token } => shorten_secret(token),
```

and replace the `Auth::ApiKey { key, value, placement } => { ... }` arm with:

```rust
        Auth::ApiKey {
            key,
            value,
            placement,
        } => format!("{}={} ({})", key, shorten_secret(value), placement),
```

- [ ] **Step 6: Use the helper in `extract_form_fields`**

Replace the body of the `BodyMode::FormUrlEncoded | BodyMode::FormData => body.form_data....unwrap_or_default(),` arm with:

```rust
        BodyMode::FormUrlEncoded | BodyMode::FormData => body
            .form_data
            .as_ref()
            .map(|entries| {
                enabled_entries(entries, |e| (e.enabled, e.key.as_str(), e.value.as_str()))
            })
            .unwrap_or_default(),
```

- [ ] **Step 7: Run the tests and check the crate**

Run: `cargo test -j4 -p rocket-collection contract::snapshot` then `cargo check -j4 -p rocket-collection`
Expected: PASS and no new warnings. If the compiler rejects `&request.headers` for the `&[T]` parameter, pass `request.headers.as_slice()` instead (same for the query params and `entries.as_slice()`).

---

### Task 3: Shared invalid() helper and pinned messages

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-environment/src/invalid.rs`
- Modify: `crates/rocket-environment/src/lib.rs`
- Modify: `crates/rocket-environment/src/client_certificate_validation.rs` (remove the private `invalid`, import the shared one)
- Modify: `crates/rocket-environment/src/external_secret.rs` (use `invalid`, add the message test)
- Modify: `crates/rocket-environment/CLAUDE.md` (one module map row)

**Interfaces:**
- Produces: `pub(crate) fn invalid(message: impl Into<String>) -> DomainError` in `crates/rocket-environment/src/invalid.rs`, reachable as `crate::invalid::invalid`.

- [ ] **Step 1: Add the message pinning test**

In the `tests` module of `external_secret.rs`, after `validate_bindings_rejects_missing_connection_or_vault`, add:

```rust
    fn message_of(bindings: &[ExternalSecretBinding]) -> String {
        match validate_external_secret_bindings(bindings).expect_err("must reject") {
            DomainError::InvalidInput(m) => m,
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    #[test]
    fn validate_bindings_messages_are_pinned() {
        assert_eq!(
            message_of(&[binding("")]),
            "external secret binding alias must not be empty"
        );
        assert_eq!(
            message_of(&[binding("pay.ments")]),
            "external secret alias 'pay.ments' may only contain letters, digits, '_' or '-'"
        );
        assert_eq!(
            message_of(&[binding("payments"), binding("payments")]),
            "external secret alias 'payments' is used by more than one binding"
        );
        let mut no_conn = binding("payments");
        no_conn.connection_id = "  ".to_string();
        assert_eq!(
            message_of(&[no_conn]),
            "external secret binding 'payments' has no connection selected"
        );
        let mut no_vault = binding("payments");
        no_vault.vault_name = String::new();
        assert_eq!(
            message_of(&[no_vault]),
            "external secret binding 'payments' has no vault name"
        );
    }
```

- [ ] **Step 2: Run the pinning tests against the old code**

Run: `cargo test -j4 -p rocket-environment external_secret::`
Expected: PASS (including `validate_bindings_messages_are_pinned`).

- [ ] **Step 3: Create the helper module**

Create `crates/rocket-environment/src/invalid.rs`:

```rust
//! The one way this crate builds an invalid input error.

use rocket_shared::error::DomainError;

/// Builds `DomainError::InvalidInput` from any text.
pub(crate) fn invalid(message: impl Into<String>) -> DomainError {
    DomainError::InvalidInput(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_wraps_str_and_string_in_invalid_input() {
        assert_eq!(invalid("a"), DomainError::InvalidInput("a".to_string()));
        assert_eq!(
            invalid(format!("b{}", 1)),
            DomainError::InvalidInput("b1".to_string())
        );
    }
}
```

If `DomainError` does not implement `PartialEq`, compare with `matches!(invalid("a"), DomainError::InvalidInput(m) if m == "a")` instead.

In `crates/rocket-environment/src/lib.rs`, add the line `mod invalid;` after `pub mod external_secret;`. It stays private to the crate.

- [ ] **Step 4: Move `invalid` out of `client_certificate_validation.rs`**

Delete the private function at the bottom of the file (just above `#[cfg(test)]`):

```rust
fn invalid(message: String) -> DomainError {
    DomainError::InvalidInput(message)
}
```

Add `use crate::invalid::invalid;` next to the other `use crate::...` lines at the top. The twelve `invalid(format!(...))` call sites need no change. Then run `cargo check -j4 -p rocket-environment`. If it warns that `DomainError` is an unused import in non-test code, change the top import to `use rocket_shared::error::DomainResult;` and add `use rocket_shared::error::DomainError;` inside the `tests` module (the tests use it through `use super::*`).

- [ ] **Step 5: Use `invalid` in `external_secret.rs`**

Add `use crate::invalid::invalid;` after the existing `use` lines. Replace the five error constructions in `validate_external_secret_bindings` so the function body reads:

```rust
pub fn validate_external_secret_bindings(bindings: &[ExternalSecretBinding]) -> DomainResult<()> {
    let mut seen = std::collections::HashSet::new();
    for binding in bindings {
        let alias = binding.alias.as_str();
        if alias.is_empty() {
            return Err(invalid("external secret binding alias must not be empty"));
        }
        if !alias
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(invalid(format!(
                "external secret alias '{alias}' may only contain letters, digits, '_' or '-'"
            )));
        }
        if !seen.insert(alias) {
            return Err(invalid(format!(
                "external secret alias '{alias}' is used by more than one binding"
            )));
        }
        if binding.connection_id.trim().is_empty() {
            return Err(invalid(format!(
                "external secret binding '{alias}' has no connection selected"
            )));
        }
        if binding.vault_name.trim().is_empty() {
            return Err(invalid(format!(
                "external secret binding '{alias}' has no vault name"
            )));
        }
    }
    Ok(())
}
```

Keep the doc comment above the function unchanged. If `cargo check` warns that `DomainError` is now unused outside tests, apply the same fix as in Step 4: keep only `DomainResult` in the top import and import `DomainError` inside the `tests` module.

- [ ] **Step 6: Add the module map row**

In `crates/rocket-environment/CLAUDE.md`, in the Module Map table, add a row after the `client_certificate_validation.rs` row:

```
| `invalid.rs` | `pub(crate) invalid(message)` — the one way this crate builds `DomainError::InvalidInput`; used by `client_certificate_validation.rs` and `external_secret.rs` |
```

- [ ] **Step 7: Run everything for the three crates**

Run these one after another:
- `cargo check -j4`
- `cargo test -j4 -p rocket-environment`
- `cargo test -j4 -p rocket-collection`
- `cargo test -j4 -p rocket-shared`

Expected: all PASS with no new warnings. Report the number of passing tests for each crate.

Next plan: none
