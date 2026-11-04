# DDD Phase 6, Plan 3: effective_auth and add_scope (D-63, D-64)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The auth fallback chain and the scope-to-layer rules live in the domain crates, so `execution_service.rs` only calls them.

**Architecture:** `effective_auth(request, folders, collection)` joins `resolve_folder_auth` in `rocket-collection`. `VariableContext::add_scope` in `rocket-environment` owns the enabled filter, the empty-value rule and the secret marking. Behavior is unchanged.

**Spec:** `docs/roadmaps/ddd-roadmap.md` items D-63 and D-64.

## Global Constraints

- Tests first in the domain crate. No change to the wire format or to error texts.
- No `unwrap` in production code. `cargo` commands take `-j4`.

## Review Focus

- A request set to `Inherit` takes the folder auth, then the collection auth, then `None`.
- A declared variable with an empty value falls back to its initial value; with neither it is skipped, but a secret one still marks its name.
- An environment variable with an empty value is kept (it hides the same name in a lower scope).
- A secret value of 5 bytes is not masked; 6 bytes is.

---

### Task 1: effective_auth (D-63)

- [ ] **Step 1:** Write 5 failing tests in `folder_settings.rs` (`effective_auth_*`). **Step 2:** Run `cargo test -j4 -p rocket-collection effective_auth` — Expected: compile error. **Step 3:** Implement, re-export, replace `merge_auth` and move its tests. **Step 4:** Run `cargo test -j4 -p rocket-collection` and `cargo test -j4 -p rocket-app execution_service` — Expected: pass.

### Task 2: add_scope (D-64)

- [ ] **Step 1:** Write 9 failing tests in `context.rs` (`add_scope_*`). **Step 2:** Run `cargo test -j4 -p rocket-environment add_scope` — Expected: compile error. **Step 3:** Implement `Scope`, `ScopeEntry`, `add_scope`, `MIN_SECRET_VALUE_LEN`; switch `scopes_inner`. **Step 4:** Run `cargo test -j4 -p rocket-environment` and `cargo test -j4 -p rocket-app` — Expected: pass.

### Task 3: Docs

- [ ] **Step 1:** Update `crates/rocket-environment/CLAUDE.md` and the roadmap rows; run `cargo clippy -j4 -p rocket-environment -p rocket-collection -p rocket-app --all-targets` — Expected: no new warnings.
