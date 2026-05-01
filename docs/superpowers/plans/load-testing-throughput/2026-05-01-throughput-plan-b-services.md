# Throughput-Target Phases — Plan B: Services + Tauri

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update `rocket-app::LoadTestService` and the Tauri command layer to validate that incoming `LoadTestConfigV2` uses a uniform target unit before running, and to surface a clear `DomainError::Validation` when it doesn't.

**Architecture:** Validation lives in the service layer, not in `run_load_test_v2`, because the service is the architectural validation boundary in this codebase (per the existing crate boundaries). The Tauri command stays a thin adapter — it calls the service and propagates the error to the frontend, which displays it in the load-test sidebar.

**Tech Stack:** Rust, `DomainError`, tauri command.

**Spec:** `docs/superpowers/specs/2026-05-01-load-test-enhanced-design.md`

**Depends on:** Plan A complete (`PhaseTarget`, `TargetUnit`, `has_uniform_target_unit()` exist).

---

## File Map

| File | Change |
|---|---|
| `crates/rocket-app/src/load_test_service.rs` | Add validation guard + new test |
| `crates/rocket-shared/src/error.rs` | (Possibly) confirm `DomainError::Validation` exists; if not, use whatever variant the codebase uses for input validation |

---

## Chunk 1: Service-layer validation

### Task 1: Reject mixed-unit configs at the service boundary

**Files:**
- Modify: `crates/rocket-app/src/load_test_service.rs`

- [ ] **Step 1: Identify the right `DomainError` variant**

```bash
grep -n "pub enum DomainError\|Validation\|InvalidInput\|BadRequest" crates/rocket-shared/src/error.rs | head -20
```

Note the variant name used in this codebase for input-validation failures (likely `DomainError::Validation(String)` or `DomainError::InvalidInput(String)`). Use that name in the next step. If neither exists, use whatever `DomainError` variant the existing service code uses for "bad input" — search:

```bash
grep -rn "DomainError::" crates/rocket-app/src/ | grep -v "Internal" | head -10
```

Pick the dominant pattern. Throughout the rest of this plan, write `DomainError::Validation` as a placeholder — substitute the real name.

- [ ] **Step 2: Read the existing `LoadTestService::run` signature**

```bash
cat crates/rocket-app/src/load_test_service.rs
```

Confirm it returns `DomainResult<LoadTestResult>`.

- [ ] **Step 3: Add the validation check**

In `crates/rocket-app/src/load_test_service.rs`, find the body of `run` (or whichever method calls `run_load_test_v2`). Just before the call, add:

```rust
        if !config.has_uniform_target_unit() {
            return Err(DomainError::Validation(
                "Load test phases must all use the same target unit \
                 (either all concurrency-based or all rps-based). \
                 Mixing units in a single run is not supported."
                    .into(),
            ));
        }
```

If the variant is named differently (e.g. `DomainError::InvalidInput`), substitute it.

If the file does not import `DomainError`, add to the top:

```rust
use rocket_shared::error::{DomainError, DomainResult};
```

(It almost certainly already imports `DomainResult`; add `DomainError` to the same line.)

- [ ] **Step 4: Add a unit test for the validation**

If a `#[cfg(test)] mod tests` block already exists, append; otherwise create one at the bottom of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_http::{LoadTestPhase, LoadTestConfigV2, PhaseKind, PhaseTarget, SuccessRule};

    fn mixed_config() -> LoadTestConfigV2 {
        LoadTestConfigV2 {
            phases: vec![
                LoadTestPhase {
                    kind: PhaseKind::Hold,
                    duration_secs: 5,
                    target: PhaseTarget::Concurrency(10),
                },
                LoadTestPhase {
                    kind: PhaseKind::Hold,
                    duration_secs: 5,
                    target: PhaseTarget::Rps(50),
                },
            ],
            success_rule: SuccessRule::default(),
            ring_buffer_size: 100,
        }
    }

    #[test]
    fn mixed_unit_config_is_rejected() {
        // We cannot actually run() without a wired RequestExecutionService,
        // but we can call the validation method directly to lock in the
        // contract. The service-level rejection is exercised by an
        // integration test in src-tauri once the command is wired.
        let cfg = mixed_config();
        assert!(!cfg.has_uniform_target_unit());
    }
}
```

If the existing test file already has imports for `LoadTestPhase`, `LoadTestConfigV2`, etc., dedupe.

- [ ] **Step 5: Compile + test**

```bash
cargo check -p rocket-app 2>&1 | tail -15
cargo test -p rocket-app load_test 2>&1 | tail -15
```

Expected: clean compile, all tests pass.

If `cargo check` fails with `error[E0609]: no field 'target_concurrency'` somewhere else in `rocket-app` (e.g. in another service that constructs `LoadTestPhase` for a test), update those construction sites: `target_concurrency: N` → `target: PhaseTarget::Concurrency(N)`. Add `PhaseTarget` to the relevant `use rocket_http::...` line.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/load_test_service.rs
git commit -m "feat(rocket-app): reject mixed-unit phase configs in LoadTestService"
```

---

## Chunk 2: Tauri command + workspace compile

### Task 2: Verify Tauri command propagates validation errors

**Files:**
- Modify: `src-tauri/src/commands/load_test.rs` (only if the command currently swallows errors)

- [ ] **Step 1: Read the existing command body**

```bash
cat src-tauri/src/commands/load_test.rs
```

Find `run_load_test_v2_command`. Confirm it does `.await?` on the service call so a `DomainError::Validation` returned from `LoadTestService::run` propagates to the frontend as a Tauri command error.

If it instead unwraps or maps the error, fix it so the `?` operator is used and the function signature returns `Result<(), DomainError>` (which it already should).

- [ ] **Step 2: Build the workspace**

```bash
cargo check 2>&1 | tail -20
```

Expected: clean compile. If you see `no field 'target_concurrency'` errors in `src-tauri`, fix the same way as in Chunk 1: replace with `target: PhaseTarget::Concurrency(N)` and add the import.

- [ ] **Step 3: Run the full Rust test suite**

```bash
cargo test 2>&1 | tail -20
```

Expected: every test passes across `rocket-http`, `rocket-app`, and any other crates.

- [ ] **Step 4: Commit (if any changes were made)**

```bash
git add src-tauri/src/commands/load_test.rs
git commit -m "fix(tauri): ensure run_load_test_v2_command propagates LoadTestService validation errors"
```

If the command was already correct, skip the commit.

---

## Verification Gate

```bash
cargo check 2>&1 | tail -5
cargo test 2>&1 | tail -10
```

Expected: workspace compiles, all tests pass. Plan B is then complete and Plan C (frontend) can begin.
