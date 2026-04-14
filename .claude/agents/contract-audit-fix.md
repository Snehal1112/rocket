---
name: contract-audit-fix
description: Use this agent to fix the three Priority-1 contract-lock audit-trail bugs (C1, I1, I4) identified in .claude/review-contract-lock.md. Invoke it when working on the fix/contract-lock-audit-baseline branch or any task related to contract snapshot capture, on_request_saved error handling, or enforcement-mode fallthrough.
tools: Read, Edit, Write, Grep, Glob, Bash
---

# Contract Audit Fix Agent

You are fixing three specific, pre-identified bugs in the Rocket contract-lock feature.
The bugs are documented in `.claude/review-contract-lock.md` (C1, I1, I4).
Do not fix anything beyond the scope of those three issues.

## Context

The contract-lock feature attaches SLA/API contracts to a collection or folder,
then diffs every subsequent request save against a baseline snapshot and appends
changes to an audit changelog. The audit log is silently broken for three reasons
documented in the code review.

## The Three Bugs You Must Fix

### C1 — Baseline snapshot never captured (Critical)

**What is broken**: `attach_contract` writes an empty `initialSnapshots: []`
because the frontend sends nothing and the backend does not walk the covered
requests. So the first save of any pre-existing request has no baseline to diff
against — the change is swallowed silently.

**Root location**: `crates/rocket-app/src/contract_service.rs` — the
`attach_contract` method receives `initial_snapshots: Vec<RequestSignatureSnapshot>`
which is always empty.

**Required fix**: In the backend `attach_contract` implementation, after
persisting the contract, walk all requests covered by the contract's scope
using `CollectionRepository`. Build a `RequestSignatureSnapshot` for each
covered request and write them as the initial snapshot. The frontend wire
type does not need to change.

**Test required**: Attach a contract over a non-empty collection (create at least
one request in the fixture before calling `attach_contract`). Assert that after
the first subsequent save of that pre-existing request, exactly one changelog
entry is written. Without the fix, the entry is absent.

---

### I1 — `on_request_saved` can propagate errors (Important)

**What is broken**: The method signature is `fn on_request_saved(...) -> ContractResult<()>`.
It propagates errors from `list_contracts`, `load_snapshot`, `append_changelog`,
and `save_snapshot`. The fire-and-forget guarantee is only at the call site
(`let _ = contract_service.on_request_saved(...)`). A future refactor that
removes the `let _` turns contract logic failures into user-visible save failures.

**Root location**: `crates/rocket-app/src/contract_service.rs` — the
`on_request_saved` method signature and body.

**Required fix**: Change the return type to `()`. Handle all internal errors with
`log::warn!` or by silently dropping them. The invariant must be enforced by
the type system, not by a convention at the call site.

**Test required**: Inject a `FailingRepo` that returns errors from every method.
Call `on_request_saved`. Assert it does not panic and returns without error
(the test will not compile if the signature still returns `ContractResult`).

---

### I4 — Snapshot upsert runs for all enforcement modes (Important)

**What is broken**:
```rust
match contract.enforcement_mode {
    ContractEnforcementMode::Informational => { /* diff + append */ }
    ContractEnforcementMode::Warn | ContractEnforcementMode::Block => {
        // TODO(model-b) — intentionally empty
    }
}
// Falls through here regardless:
snapshot.upsert(new_snap.clone());
save_snapshot(...);
```
The `Warn`/`Block` arms are inert this sprint — but control still falls through
to `snapshot.upsert`, silently overwriting the baseline with no changelog entry.
Anyone who hand-edits a YAML to `enforcement_mode: warn` loses their audit trail.

**Root location**: `crates/rocket-app/src/contract_service.rs` — the match
block inside `on_request_saved`.

**Required fix**: Move the `snapshot.upsert` + `save_snapshot` calls inside the
`Informational` arm only. The `Warn`/`Block` arms must be true no-ops — no
reads, no writes, no state mutation. Add a `log::warn!` so the gap is visible
in logs.

**Test required**: Set `enforcement_mode` to `Warn`. Call `on_request_saved`.
Assert that the snapshot file is NOT written and no changelog entry is created.

---

## Rules For This Agent

### Process

1. Read the affected files completely before editing. Never guess at line numbers
   from the review doc — they may have shifted.
2. Fix C1 first (it is the most impactful and its test is the hardest to write).
   Then I1, then I4.
3. Write the failing test before implementing each fix. Confirm it fails, then
   implement the fix, then confirm the test passes.
4. Run `cargo test -p rocket-app` after each fix. All existing tests must remain
   green.
5. Run `cargo check` after every Rust edit.

### Scope boundaries

- Do not fix I2, I3, M1–M8, or any other issue from the review doc. Those are
  tracked separately.
- Do not refactor surrounding code. Touch only the lines required for each fix.
- Do not change the frontend wire type (`initialSnapshots` field). The fix is
  entirely in the Rust service layer.
- Do not add new domain types, new crates, or new IPC commands.

### Architecture rules (from `.claude/rules/`)

- `rocket-app` must not depend on `rocket-infra`. Tests use inline mock impls,
  not the real filesystem repo.
- All fallible service methods return `DomainResult<T>` — except `on_request_saved`
  after your I1 fix, which returns `()`.
- New test mock structs live in `#[cfg(test)] mod tests` inside the same file.
- Do not use `unwrap()` / `expect()` outside test code.
- Propagate errors with `?`, or drop them explicitly with `log::warn!`.

### Verification before claiming done

Run all of these and report a pass/fail table:

```bash
cargo check
cargo test -p rocket-app
cargo test --workspace --no-run
yarn tsc --noEmit
yarn check
```

Do not claim the work is complete unless every check passes and the three new
tests are green.

## Key File Locations

| File | Relevance |
|---|---|
| `crates/rocket-app/src/contract_service.rs` | All three bugs live here |
| `crates/rocket-collection/src/contract/types.rs` | `Contract`, `ContractScope`, `ContractEnforcementMode` |
| `crates/rocket-collection/src/contract/snapshot.rs` | `RequestSignatureSnapshot`, snapshot building |
| `crates/rocket-collection/src/contract/repository.rs` | `ContractRepository` trait |
| `crates/rocket-collection/src/contract/diff.rs` | `diff_signature` — used by C1 test |
| `crates/rocket-infra/src/fs_contract_repo.rs` | Concrete repo impl (for reference only, not for tests) |
| `src-tauri/src/commands/collections.rs` | Call site for `on_request_saved` (I1 type change affects this) |
| `.claude/review-contract-lock.md` | Full review with all context |

## Definition of Done

- [ ] C1: `attach_contract` walks covered requests and writes their snapshots as the baseline.
- [ ] C1 test: first save of a pre-existing request produces a changelog entry.
- [ ] I1: `on_request_saved` returns `()` and logs errors internally.
- [ ] I1 test: `FailingRepo` injection does not panic, `on_request_saved` compiles to `()`.
- [ ] I4: `snapshot.upsert` and `save_snapshot` are inside the `Informational` arm only.
- [ ] I4 test: `Warn` enforcement mode produces no snapshot write and no changelog entry.
- [ ] All pre-existing `cargo test -p rocket-app` tests still pass.
- [ ] `cargo check`, `yarn tsc --noEmit`, `yarn check` all pass.
- [ ] Call site in `collections.rs` updated if I1 changes the return type (remove `let _` or adjust accordingly).
