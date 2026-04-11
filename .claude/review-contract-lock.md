# Code Review: Contract Lock (Plans 1–4)

**Commits reviewed**: `2f4ae14`, `99502ce`, `cb55868`, `a85239f` (range `4dd28e1..a85239f` on `main`)
**Design spec**: [`docs/superpowers/specs/2026-04-07-contract-lock-design.md`](../docs/superpowers/specs/2026-04-07-contract-lock-design.md)
**Review date**: 2026-04-11
**Reviewer**: `superpowers:code-reviewer` subagent (independent, no session history)
**Review status**: Post-merge — feature already landed on `origin/main` at `c2693c3`

---

## Executive Summary

The domain layer, persistence, and Model B seam are well-built and cleanly layered. `diff_signature` is correct — it uses `contains()` on key lists so `[A,B]` vs `[B,A]` does not produce spurious changelog entries. Plan deviations in `cb55868` are all justified against the real codebase. Tests assert meaningful state transitions, not just renders.

**However**, the reviewer identified **one Critical issue (C1)** and **four Important issues (I1–I4)** that compound to silently defeat the headline audit-log goal for the most common real-world scenario.

**Verdict**: Do not rely on the audit trail until C1 and I4 are fixed. The feature is only `Informational` this sprint, so there is no user-visible crash, but the data-loss window is real.

**Recommendation**: Open a `fix/contract-lock-audit-baseline` branch off `main` and address C1 + I1 + I4 as a small follow-up PR before Model B work begins.

---

## Strengths

1. **Spec-aligned domain types.** `Contract`, `ContractSnapshot`, `ContractChangelog`, `ContractScope`, `ContractEnforcementMode` match the spec almost verbatim (`crates/rocket-collection/src/contract/types.rs`, `snapshot.rs`, `changelog.rs`). `ContractStatus` is correctly *derived*, not persisted (`types.rs:22-32`) — an improvement over the spec.
2. **`diff_signature` is order-insensitive.** `crates/rocket-collection/src/contract/diff.rs:45-77` uses `contains()` for key lists, so field reordering produces zero spurious entries. Covered by `no_changes_returns_empty`.
3. **Plan deviations justified.** The `Auth` / `Body` / `Request` / `rocket` package corrections documented in commit `cb55868` are all correct against the actual codebase.
4. **Save-hook ordering.** `src-tauri/src/commands/collections.rs:61-73` runs `on_request_saved` *after* the save succeeds and discards the result via `let _ =`. Mutex is dropped before the hook call — no deadlock risk.
5. **Clean layering.** `MockContractRepo` lives inline in `contract_service.rs:138-206`, respecting the rocket-app ↛ rocket-infra boundary.
6. **Frontend tests assert behaviour.** The 34 new frontend tests cover state transitions and action wiring, not just render smoke tests.

---

## Critical Issues

### C1. Baseline snapshot is never captured — audit log silently broken for non-empty collections

**Locations**:
- `src/components/contract/AttachContractDialog.tsx:93` — hardcoded `initialSnapshots: []`
- `src/lib/tauri-api.ts:889-895` — docstring acknowledges "Frontend sends an empty array today"
- `crates/rocket-app/src/contract_service.rs:85` — `if let Some(old_snap) = snapshot.get(...)` branch

**What the spec requires** (`docs/superpowers/specs/2026-04-07-contract-lock-design.md:443`):
> "On submit: creates contract, **takes snapshot of all covered requests**, initialises empty changelog."

**What actually happens**:
When a contract is attached to a collection that already contains requests:
1. `attach_contract` writes an empty snapshot (frontend sends `[]`).
2. The first save of any pre-existing request has no baseline.
3. `on_request_saved` falls through the `if let Some(old_snap)` arm.
4. The snapshot is silently upserted with the current request shape.
5. **No changelog entry is written** — the change is invisible.

**Impact**: The first modification to every pre-existing request is lost from the audit trail. This is the most common real-world case (users attach contracts to existing collections, not empty ones). The must-have goal "Diff every subsequent save against the snapshot and append to an audit changelog" is not met.

**Fix options**:
- **Backend** (preferred): `attach_contract` walks all requests covered by the contract's scope via `CollectionRepository`, builds `RequestSignatureSnapshot` for each, and writes them as the initial snapshot. The frontend can continue sending `initialSnapshots: []`.
- **Frontend**: Walk the covered requests in `AttachContractDialog.handleSubmit`, build snapshots, send them in the IPC call. More coupling to the wire format; prefer the backend fix.
- **Hack**: Treat "first sight" in `on_request_saved` as an `Added` changelog event. Preserves the audit log but loses the distinction between "first saw this field" and "existed at contract-sign time".

**Severity**: Critical. The headline feature is broken for its primary use case.

---

## Important Issues

### I1. `on_request_saved` does not actually guarantee `Ok(())`

**Location**: `crates/rocket-app/src/contract_service.rs:71-75`

**Signature**:
```rust
pub fn on_request_saved(&self, ...) -> ContractResult<()>
```

`list_contracts` (`:76`), `load_snapshot` (`:83`), `append_changelog` (`:97`), and `save_snapshot` (`:109`) all propagate errors. The fire-and-forget semantics live only at the call site: `src-tauri/src/commands/collections.rs:71` does `let _ = contract_service.on_request_saved(...)`.

**Problems**:
1. The "always-Ok" guarantee is enforced by convention, not by the type system or a test.
2. None of the three service tests (`contract_service.rs:245-287`) asserts silent-failure behaviour with a failing repo.
3. A future refactor that propagates the error from `collections.rs` would turn contract logic failures into user-visible save failures — the exact outcome the fire-and-forget design is meant to prevent.

**Fix**: Either
- Change the signature to `fn on_request_saved(&self, ...) -> ()` and log errors internally (`log::warn!` or equivalent), **or**
- Add a test that injects a `FailingRepo` and asserts the hook returns `Ok(())`.

The type-level fix is preferable — it makes the invariant local to the service.

---

### I2. `list_contracts` filter is fragile to ID format changes

**Location**: `crates/rocket-infra/src/fs_contract_repo.rs:62-65`

```rust
if name.contains("-snapshot") || name.contains("-changelog") {
    continue;
}
```

ULIDs are `[0-9A-Z]{26}` — no hyphens — so the filter is safe today. But it is not self-documenting: if the ID format ever changes (e.g. to UUIDs, which contain hyphens), the filter silently breaks and `list_contracts` starts returning snapshot and changelog entries as if they were contracts.

**Fix**: Match the ULID format exactly, e.g.
```rust
// Accept only files whose stem is a valid ULID.
if Ulid::from_string(name.trim_end_matches(".yml")).is_err() {
    continue;
}
```
or at minimum a regex `^[0-9A-Z]{26}\.yml$`.

---

### I3. Save-hook path derivation is workspace-mode-specific

**Location**: `src-tauri/src/commands/collections.rs:68`

```rust
guard.join("collections").join(&collection)
```

This hand-rolls the collection root. `CollectionService::save_request` itself may resolve collection roots differently — single-workspace vs multi-workspace mode, or external-ref collections. If the two resolvers ever disagree, the hook operates on the wrong `.rocket/contracts/` directory and silently writes nothing.

**Fix**: Use a single shared resolver. Either
- Extract a helper on `CollectionService` that returns the canonical collection root, and call it from both the save path and the contract hook, **or**
- Have `CollectionService::save_request` pass the already-resolved root into the contract hook as a parameter.

---

### I4. Model B fallthrough silently destroys the audit trail

**Location**: `crates/rocket-app/src/contract_service.rs:100-108`

```rust
match contract.enforcement_mode {
    ContractEnforcementMode::Informational => { /* diff + append */ }
    ContractEnforcementMode::Warn | ContractEnforcementMode::Block => {
        // TODO(model-b)
    }
}
// Control reaches here regardless:
snapshot.upsert(new_snap.clone());
save_snapshot(...);
```

The `Warn | Block` arms are no-ops this sprint — that is intentional. But control still falls through to `snapshot.upsert(new_snap.clone())`, overwriting the baseline with the new shape. **If anyone ever hand-edits a contract YAML to `enforcement_mode: warn`, the pre-change snapshot is silently destroyed and no changelog entry is written.**

This turns the "Model B seam" from an inert forward-compatibility hook into a latent data-loss bug. The seam should be inert, not active.

**Fix**: Guard the snapshot upsert behind the `Informational` arm only:
```rust
match contract.enforcement_mode {
    ContractEnforcementMode::Informational => {
        // diff + append + upsert + save
    }
    ContractEnforcementMode::Warn | ContractEnforcementMode::Block => {
        log::warn!("contract {} has enforcement mode {:?} which is not yet implemented; skipping audit", contract.id, contract.enforcement_mode);
        // Do nothing.
    }
}
```

Alternatively, reject contracts with `Warn`/`Block` modes at `attach_contract` time until Model B lights them up.

---

### I5. C1 + I4 compound

C1 means every contract starts with an empty baseline. I4 means a future `Warn`/`Block` contract silently overwrites its baseline. The combination means a Model-B-enabled contract signed today would have no baseline to warn against and would actively lose its chance to build one. Fixing one without the other leaves the trap intact.

---

## Minor Issues

### M1. `ContractPanel` is a Dialog, not the spec's right-side Sheet

**Location**: `src/components/contract/ContractPanel.tsx:25-34`

The design spec calls for a 480-px right-side Sheet. The shadcn/ui `sheet` primitive is not installed in this project, so Dialog was used as the closest available alternative with a wider fixed max-width. The deviation is documented in the file's doc comment. Fine for this sprint; file a follow-up ticket to add the `sheet` primitive.

### M2. Multi-contract badge UX gap

**Location**: `src/components/contract/ContractBadge.tsx:30, 68-70`

When a collection has multiple contracts, the badge tooltip correctly shows "+N more", but the panel always opens the *first* contract. The other contracts are unreachable from the sidebar. This is a genuine UX gap, not just a polish item.

**Fix**: When `contracts.length > 1`, render a small dropdown or list on click instead of opening the first contract directly.

### M3. Changelogs grow unbounded

**Location**: `crates/rocket-infra/src/fs_contract_repo.rs:99-113`

`append_changelog` loads the entire file, extends it, and rewrites it. O(n) per save plus a full YAML rewrite. For a long-lived contract on an actively-edited collection, this degrades linearly. Not a blocker, but worth a ticket for a future append-mode writer or size cap.

### M4. `contractsFor` comment lies about stable identity

**Location**: `src/stores/contract-store.ts:29-30`

The doc comment claims "stable identity when unchanged", but the store itself returns a fresh `[]` on every miss. Stability is achieved only via the `EMPTY_CONTRACTS` module-level sentinel in `src/components/collections/CollectionNode.tsx`. Either move the sentinel into the store and use it from the selector, or fix the comment.

### M5. `daysFromToday()` timezone edge case

**Location**: `src/components/contract/__tests__/ContractPanel.test.tsx:67-72`

The helper is correct for the status-class assertions the tests actually make, but the `screen.getByText(expiry)` assertion at `:111` could flake in extreme positive timezones (UTC+13/14) because `setDate() + toISOString()` crosses the UTC day boundary. Very minor, and the CI environment is unlikely to be affected.

### M6. Rust service test coverage gaps

**Location**: `crates/rocket-app/src/contract_service.rs:245-287`

Missing test cases:
- Folder-scope `covers()` semantics
- Request-scope `covers()` semantics
- `Warn` and `Block` enforcement modes are inert (no writes, no changelog, no panic)
- Failing-repo silent-failure (tied to I1)

Good state-transition coverage for the happy path; the gaps are at the seams and invariants.

### M7. `Contract` struct has no `status` field

**Location**: `crates/rocket-collection/src/contract/types.rs`

The spec (`spec:258`) shows `pub status: ContractStatus`. The implementation correctly *derives* status via `Contract::status()` instead of persisting it — an improvement over the spec (avoids the "stored status drifts from expiry date" bug class). Worth noting in the plan deviations doc so future work doesn't try to "fix" it.

### M8. `Mutex::lock().unwrap()` in the save hook

**Location**: `src-tauri/src/commands/collections.rs:67`

```rust
let guard = active_workspace_path.lock().unwrap();
```

Consistent with the rest of the codebase, but the contract hook is explicitly "must not fail the user's save". A poisoned mutex turns `save_request` into a panic. Use `.lock().ok()` or `.lock().unwrap_or_else(|e| e.into_inner())` to make the hook truly crash-proof.

---

## Files Reviewed

**Rust**
- `/home/numericlabs/data/rocket/rocket/crates/rocket-collection/src/contract/types.rs`
- `/home/numericlabs/data/rocket/rocket/crates/rocket-collection/src/contract/snapshot.rs`
- `/home/numericlabs/data/rocket/rocket/crates/rocket-collection/src/contract/changelog.rs`
- `/home/numericlabs/data/rocket/rocket/crates/rocket-collection/src/contract/diff.rs`
- `/home/numericlabs/data/rocket/rocket/crates/rocket-collection/src/contract/repository.rs`
- `/home/numericlabs/data/rocket/rocket/crates/rocket-infra/src/fs_contract_repo.rs`
- `/home/numericlabs/data/rocket/rocket/crates/rocket-app/src/contract_service.rs`
- `/home/numericlabs/data/rocket/rocket/src-tauri/src/commands/contract.rs`
- `/home/numericlabs/data/rocket/rocket/src-tauri/src/commands/collections.rs`

**Frontend**
- `/home/numericlabs/data/rocket/rocket/src/lib/tauri-api.ts`
- `/home/numericlabs/data/rocket/rocket/src/stores/contract-store.ts`
- `/home/numericlabs/data/rocket/rocket/src/components/contract/AttachContractDialog.tsx`
- `/home/numericlabs/data/rocket/rocket/src/components/contract/ContractBadge.tsx`
- `/home/numericlabs/data/rocket/rocket/src/components/contract/ContractPanel.tsx`
- `/home/numericlabs/data/rocket/rocket/src/components/collections/CollectionNode.tsx`

**Tests**
- `/home/numericlabs/data/rocket/rocket/src/stores/__tests__/contract-store.test.ts`
- `/home/numericlabs/data/rocket/rocket/src/components/contract/__tests__/ContractBadge.test.tsx`
- `/home/numericlabs/data/rocket/rocket/src/components/contract/__tests__/AttachContractDialog.test.tsx`
- `/home/numericlabs/data/rocket/rocket/src/components/contract/__tests__/ContractPanel.test.tsx`

---

## Test Counts at Review Time

| Suite | Count | Status |
|---|---|---|
| Frontend (`yarn test --run`) | 238 | all passing |
| Rust (`cargo test --workspace`) | 601 | all passing |
| **Total** | **839** | **green** |

The reviewer did not rerun the suites; counts were verified by the implementer before merge.

---

## Recommended Follow-Up

### Priority 1 — Ship before Model B sprint (`fix/contract-lock-audit-baseline`)

- **C1** — Backend captures baseline snapshots at `attach_contract` time by walking the covered requests. Include a test that attaches a contract over a non-empty collection and verifies the first save of a pre-existing request produces a changelog entry.
- **I1** — Change `on_request_saved` signature to `()`, log errors internally. Add a `FailingRepo` test asserting no error propagates.
- **I4** — Move `snapshot.upsert` inside the `Informational` arm. Add a test that `Warn`/`Block` contracts do not mutate state.

Estimated effort: half a day, including tests.

### Priority 2 — Before Model B goes live

- **I2** — Tighten `list_contracts` filter to match ULID format exactly.
- **I3** — Unify collection root derivation between `save_request` and the contract hook.
- **M6** — Fill test gaps for folder/request scope `covers()` and Model B inertness.
- **M8** — Make `Mutex::lock()` poison-tolerant in the save hook.

### Priority 3 — Polish / tickets

- **M1** — Install shadcn `sheet` primitive and convert `ContractPanel`.
- **M2** — Multi-contract badge dropdown UX.
- **M3** — Bounded changelog storage or append-mode writer.
- **M4** — Move `EMPTY_CONTRACTS` sentinel into the store.
- **M5** — Timezone hardening on `daysFromToday()`.
- **M7** — Document the "status is derived, not persisted" deviation in the plan doc.

---

## Landing Decision

**Status**: Already merged to `main` at `c2693c3`. No revert recommended — the core domain, diff, persistence, and test layers are correct. The bugs are in the seams (C1, I1, I4), not the architecture.

**Action**: Track Priority 1 fixes on a dedicated follow-up branch before any Model B work begins. Until those land, treat the audit log as "best effort for changes after the first save of each request in an already-populated collection".

---

**Reviewer**: `superpowers:code-reviewer` (independent subagent)
**Review method**: Full diff read of `4dd28e1..a85239f` against the design spec; no bash/grep/glob tools available due to deleted worktree during session. Test counts and CI results were trusted from the implementer's verified runs.
