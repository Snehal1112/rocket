# Provider-Neutral Agent Credential Error Text Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reword the user-facing errors of the agent-credential path so they no longer name RocketVault, because Azure Key Vault connections use the same path. Wording only: no type, enum, serde or persisted-shape change.

**Architecture:** Three error strings in `rocket-app` change (two in `agent_config_service.rs`, one in `secret_manager_service.rs`). No test asserts the old text today (the existing tests match only the `DomainError` variant), so the tests are written first to pin the new text, fail, and then pass after the source edit. The frontend matches only on `credential_env_var may contain only`, so the last step of the single task is a grep check that proves it.

**Tech Stack:** Rust (`rocket-app`, `rocket-shared::DomainError`), Vitest, TypeScript.

**Spec:** `docs/superpowers/specs/2026-10-14-ai-agents-dialog-redesign-design.md` (section "Rust changes (wording only)"). Index: `docs/superpowers/plans/2026-10-14-ai-agents-dialog-redesign/00-plan-index.md`.

## Global Constraints

- Commits: conventional commit format, through the `dev-workflow-skills:1-git-commit` skill, with a pathspec (`git add <paths>` and `git commit --only -- <paths>`). Never `git add -A`, `--all` or `.`.
- Checks: `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern>`. Rust: `cargo check -j4` and `cargo test -j4 -p <crate> <name>`. Never `cargo test --workspace`.
- Persisted data is unchanged: `CredentialSource` stays `vault | env | keychain` and the `vault_*` fields stay. Do not touch enum names, serde attributes or persisted shapes.
- No OpenCollection pre-read is needed: plan 4 changes error text in `rocket-app` only.
- Rust production code: never a panicking unwrap call; map errors explicitly. A repo hook blocks any file write whose content contains that literal call, so never type it, even in comments or docs. Test code uses `expect` / `expect_err` / `let ... else`.
- Serde: `#[serde(rename_all = "camelCase")]` on IPC DTOs only. Not touched here.
- Rust comments are short full sentences that end with a punctuation mark.

## Review Focus

1. Connection deleted after save: `resolve_credential_errors_when_vault_connection_deleted_after_save` must now pin the new "secret manager connection" text, not only the `NotFound` variant (pinned in Task 1, Step 1).
2. Secret deleted in the provider: `resolve_credential_errors_when_secret_stale` pins "credential no longer exists in the secret manager" (Task 1, Step 1).
3. Provider without a fetcher or connection unknown at save time: `save_rejects_unknown_vault_connection_id` pins "any configured secret manager connection". The spec says a save-time fetcher check is out of scope, so no new validation is added (Task 1, Step 1).
4. Error text shown in the toast: `saveErrorMessage` in `AgentConfigsDialog.tsx` falls through to `Could not save agent: <text>` for these strings, so the new text is what the user reads. Task 1 Step 7 greps `src` to prove nothing depends on the old text. Plan 3 rewrites that dialog, so the plan uses grep only and no line numbers there.
5. Strings in other places (docs, other languages, comments, worktrees): Task 1 Step 6 greps `crates/`, `src-tauri/` and `src/` for any remaining `RocketVault` text in an error or format string of the agent credential path. Old plan and spec docs under `docs/superpowers/` are historical records and stay unchanged. `.claude/worktrees/` copies are other branches' checkouts and are not touched.

---

### Task 1: Reword the credential error strings, pin them with tests and check the frontend

**Files:**
- Verify only (modify only if the Step 7 grep finds a match): `src/`
- Modify: `crates/rocket-app/src/agent_config_service.rs` (source lines 63 and 202, doc comment line 105; tests at lines 767-778, 805-815, 880-910)
- Modify: `crates/rocket-app/src/secret_manager_service.rs` (source line 115; tests at lines 679-698 and 769-788)

**Interfaces:**
- Consumes: `DomainError::InvalidInput(String)` and `DomainError::NotFound(String)` from `rocket_shared::error`; `AgentConfigService::save`, `AgentConfigService::resolve_credential`, `SecretManagerService::fetch_secret_names`, `SecretManagerService::resolve_secret_value`.
- Produces: no new items. Same variants, same signatures; only the message text changes.

Old to new strings (exact):

| # | Location | Old | New |
|---|---|---|---|
| 1 | `agent_config_service.rs:63` | `"agent '{}': vault_connection_id '{}' does not match any configured RocketVault connection"` | `"agent '{}': vault_connection_id '{}' does not match any configured secret manager connection"` |
| 2 | `agent_config_service.rs:202` | `"agent '{}': credential no longer exists in RocketVault — reconfigure this agent"` | `"agent '{}': credential no longer exists in the secret manager — reconfigure this agent"` |
| 3 | `secret_manager_service.rs:115` | `DomainError::NotFound(id.to_string())` (message is only the bare id; rendered as `Not found: <id>`) | `DomainError::NotFound(format!("secret manager connection '{id}'"))` |
| 4 | `agent_config_service.rs:105` (doc comment, not user facing) | `Resolves the agent's credential from its source: RocketVault, a` | `Resolves the agent's credential from its source: a secret manager, a` |

Note on item 3: `secret_manager_service.rs` has no other user-facing text that names RocketVault. The bare id message is what `resolve_credential` surfaces when the connection was deleted after the agent was saved. `DomainError::NotFound` already renders as `Not found: {0}`, so the new text names the thing (style of `agent config '{id}'` in `agent_config_service.rs`) and does not repeat "not found"; it renders as `Not found: secret manager connection '<id>'`. `vault_secret_resolution.rs:34` has the same bare id but belongs to the environment external-secrets path, so it is out of scope.

- [ ] **Step 1: Update the asserting tests first**

In `crates/rocket-app/src/agent_config_service.rs`, replace the test `save_rejects_unknown_vault_connection_id` (line 767) with:

```rust
    #[test]
    fn save_rejects_unknown_vault_connection_id() {
        let service = service_with(Ok(None), false);
        let err = service
            .save(sample_config("agent-1", "no-such-conn"))
            .expect_err("must reject unknown vault_connection_id");
        let DomainError::InvalidInput(message) = err else {
            panic!("expected InvalidInput, got {err:?}");
        };
        assert!(
            message.contains("any configured secret manager connection"),
            "unexpected text: {message}"
        );
        assert!(
            !message.contains("RocketVault"),
            "the text must be provider neutral: {message}"
        );
        assert!(
            service.list().expect("list").is_empty(),
            "a rejected save must not persist the config"
        );
    }
```

Replace the end of `resolve_credential_errors_when_secret_stale` (the `.expect_err("stale vault secret must be a hard error");` line and the `assert!(matches!(err, DomainError::NotFound(_)));` after it) with:

```rust
            .expect_err("stale vault secret must be a hard error");

        let DomainError::NotFound(message) = err else {
            panic!("expected NotFound, got {err:?}");
        };
        assert!(
            message.contains("credential no longer exists in the secret manager"),
            "unexpected text: {message}"
        );
        assert!(
            !message.contains("RocketVault"),
            "the text must be provider neutral: {message}"
        );
```

In `resolve_credential_errors_when_vault_connection_deleted_after_save` (line 880), replace its final `assert!(matches!(err, DomainError::NotFound(_)));` with:

```rust
        let DomainError::NotFound(message) = err else {
            panic!("expected NotFound, got {err:?}");
        };
        assert!(
            message.contains("secret manager connection 'conn-1'"),
            "unexpected text: {message}"
        );
```

In `crates/rocket-app/src/secret_manager_service.rs`, in `unknown_connection_id_errors_clearly` (line 680) and `resolve_secret_value_unknown_connection_errors` (line 770), replace the final assertion block in each:

```rust
        assert!(
            matches!(result, Err(DomainError::NotFound(_))),
            "expected DomainError::NotFound for an unknown connection id, got {result:?}"
        );
```

with:

```rust
        let Err(DomainError::NotFound(message)) = result else {
            panic!("expected DomainError::NotFound for an unknown connection id, got {result:?}");
        };
        assert_eq!(
            message,
            "secret manager connection 'no-such-conn'"
        );
```

- [ ] **Step 2: Run the tests and confirm they FAIL**

Run each (one command per name, all must fail on the new text assertions):

```bash
cargo test -j4 -p rocket-app save_rejects_unknown_vault_connection_id
cargo test -j4 -p rocket-app resolve_credential_errors_when_secret_stale
cargo test -j4 -p rocket-app resolve_credential_errors_when_vault_connection_deleted_after_save
cargo test -j4 -p rocket-app unknown_connection_id_errors_clearly
cargo test -j4 -p rocket-app resolve_secret_value_unknown_connection_errors
```

Expected: FAIL for each, with `unexpected text: agent 'Claude': vault_connection_id 'no-such-conn' does not match any configured RocketVault connection` style output, or `left: "no-such-conn"` for the two secret manager tests.

- [ ] **Step 3: Change the source text**

In `crates/rocket-app/src/agent_config_service.rs` line 63, replace:

```rust
                    "agent '{}': vault_connection_id '{}' does not match any configured RocketVault connection",
```

with:

```rust
                    "agent '{}': vault_connection_id '{}' does not match any configured secret manager connection",
```

At line 202, replace:

```rust
                "agent '{}': credential no longer exists in RocketVault — reconfigure this agent",
```

with:

```rust
                "agent '{}': credential no longer exists in the secret manager — reconfigure this agent",
```

At line 105, replace the doc comment line:

```rust
    /// Resolves the agent's credential from its source: RocketVault, a
```

with:

```rust
    /// Resolves the agent's credential from its source: a secret manager, a
```

In `crates/rocket-app/src/secret_manager_service.rs` line 115, replace:

```rust
            .ok_or_else(|| DomainError::NotFound(id.to_string()))?;
```

with:

```rust
            .ok_or_else(|| {
                DomainError::NotFound(format!("secret manager connection '{id}'"))
            })?;
```

- [ ] **Step 4: Rerun the five tests and confirm they PASS**

```bash
cargo test -j4 -p rocket-app save_rejects_unknown_vault_connection_id
cargo test -j4 -p rocket-app resolve_credential_errors_when_secret_stale
cargo test -j4 -p rocket-app resolve_credential_errors_when_vault_connection_deleted_after_save
cargo test -j4 -p rocket-app unknown_connection_id_errors_clearly
cargo test -j4 -p rocket-app resolve_secret_value_unknown_connection_errors
```

Expected: each reports `test result: ok. 1 passed`.

- [ ] **Step 5: Run the two touched test modules and cargo check**

```bash
cargo test -j4 -p rocket-app agent_config_service
cargo test -j4 -p rocket-app secret_manager_service
cargo check -j4
```

Expected: all tests in both modules pass; `cargo check -j4` finishes with no errors or new warnings.

- [ ] **Step 6: Grep for leftovers in code, tests and the frontend**

```bash
grep -rn "no longer exists in RocketVault\|configured RocketVault connection" crates src-tauri src
grep -rn "NotFound(id.to_string())" crates/rocket-app/src/secret_manager_service.rs
```

Expected: no output from either command. (Old text in `docs/superpowers/` plans and specs is historical and stays.)

- [ ] **Step 7: Confirm the frontend does not depend on the old text**

```bash
grep -rn "RocketVault connection\|no longer exists in RocketVault" src
```

Expected: no output. If it prints nothing, no frontend change is needed and nothing more runs here. `RocketVault` hits in `src/` that are provider labels or certificate copy do not match this pattern and are not backend error matches. If a match appears, fix that file, add it to the pathspec in Step 8, and run `yarn tsc --noEmit`, `yarn test AgentConfigsDialog` and `yarn check`. Run those three only when a frontend file was changed.

- [ ] **Step 8: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill, then commit with a pathspec:

```bash
git add crates/rocket-app/src/agent_config_service.rs crates/rocket-app/src/secret_manager_service.rs
git commit --only -m "fix(app): make agent credential errors provider neutral" -- crates/rocket-app/src/agent_config_service.rs crates/rocket-app/src/secret_manager_service.rs
```

If Step 7 changed a frontend file, add that exact path to both the `git add` and the pathspec, and use a message such as `fix(app): make agent credential errors provider neutral` still (one commit for the whole task).
