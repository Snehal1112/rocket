# Team network: contract check between nodes — plan index

Spec (binding): `docs/superpowers/specs/2026-11-09-team-network-contract-check-design.md` (commit c986a3ab).
Built on: the sync series in `docs/superpowers/plans/team-network-sync/` (protocol v3, `FrameRouter`, audit pattern, `network-*` events).

## Rules

- Plans run in order. Each plan ends green: `cargo check -j4`, the named crate tests, `bun run tsc --noEmit`, `bun run check`, the named vitest files.
- At most 3 tasks per plan. Tasks that touch collections or contracts start with the OpenCollection spec read line.
- Commits use pathspecs only and go through `dev-workflow-skills:1-git-commit` (subagents use pathspec commits). Conventional subjects such as `feat(network): ...`.
- Frontend tasks (Plan 3 Tasks 2 and 3) MUST invoke `frontend-design:frontend-design` before writing components, then obey the hard rules: shadcn/ui primitives only, lucide-react icons only, Monaco (via `DiffViewer`) for diffs, narrow Zustand selectors, Bun only.
- Cargo: always `-j4`, never `--workspace` or `--all`. Production Rust paths return `DomainResult` and never panic on bad input.

## Plans

| # | File | Scope | Tasks | Model |
|---|---|---|---|---|
| 1 | `2026-11-09-contract-check-plan-01-wire-audit-matching.md` | Frames, limits, id, event enum, audit kinds, pure matching and verdict | 3 | Opus (Task 3), Sonnet (1, 2) |
| 2 | `2026-11-09-contract-check-plan-02-service-wiring.md` | Contract changes read, ports, `ContractCheckService`, router, commands | 3 | Opus |
| 3 | `2026-11-09-contract-check-plan-03-frontend.md` | API, store, bridge, consumer dialog, provider view, docs, manual checks | 3 | Sonnet |

## Cross-plan contract (names are binding)

### Plan 1 produces

```rust
// rocket-network/src/link.rs (hex_id! like SyncId)
pub struct ContractCheckId(String);            // random(), parse(&str), as_str()
// rocket-network/src/messages.rs
pub enum ContractPolicyWire { Strict, Lenient, AdditiveOk }          // snake_case
pub enum ContractChangeType { Changed, Added, Removed }              // snake_case
pub enum ContractVerdict { Pass, Break, NotUsed }                    // snake_case: pass, break, not_used
pub enum ContractCheckOutcome { Answered, Declined, Busy }           // snake_case
pub struct ContractChange { method, path_template, field, change_type, old_value: Option<String>, new_value: Option<String>, breaking: bool }
pub struct ContractCheckRow { index: u32, verdict: ContractVerdict }
PeerMessage::ContractCheckOffer { check_id: String, contract: String, version: String, policy: ContractPolicyWire, changes: Vec<ContractChange> }
PeerMessage::ContractCheckResult { check_id: String, outcome: ContractCheckOutcome, rows: Vec<ContractCheckRow> }  // rows omitted when empty
// rocket-network/src/limits.rs
CONTRACT_CHECK_MAX_CHANGES=200, CONTRACT_NAME_MAX=120, CONTRACT_VERSION_MAX=32, CONTRACT_PATH_MAX=512,
CONTRACT_FIELD_MAX=128, CONTRACT_VALUE_MAX=256, CONTRACT_CHECK_TTL=600s, CONTRACT_CHECK_PER_PEER=1, CONTRACT_CHECK_TOTAL=3
// rocket-network/src/events.rs  (channel "network-contract-check")
pub enum ContractCheckEvent {
  OfferReceived { check_id, device_id, contract, change_count },
  ResultReceived { check_id, device_id, outcome, rows: Vec<ContractCheckRowDto> },
  Expired { check_id },
}
// rocket-audit: AuditEventKind::NetworkContractCheckSent / NetworkContractCheckReceived { device_id, contract, outcome }
// rocket-app/src/network/contract_check_match.rs (pure)
pub fn verdicts(changes: &[ContractChange], policy: ContractPolicyWire, locals: &[LocalRequest]) -> Vec<(ContractVerdict, Vec<usize>)>
pub fn redact_change(c: ContractChange) -> ContractChange
```

### Plan 2 produces

```rust
// rocket-app/src/network/contract_check_ports.rs
pub trait ContractChangesPort: Send + Sync { fn pending_changes(&self, collection_root: &str, contract_id: &str) -> DomainResult<PendingContract>; }
pub trait LocalRequestsPort: Send + Sync { fn local_requests(&self) -> DomainResult<Vec<LocalRequest>>; }
// rocket-app/src/network/contract_check.rs
pub struct ContractCheckService;   // start, review, reply, decline, claims, on_frame, on_peer_gone, sweep
// src-tauri commands: network_contract_check_start, network_contract_check_review,
// network_contract_check_reply, network_contract_check_decline
```

### Plan 3 consumes the commands and the `network-contract-check` events above.

## Deviations from the spec (decided while planning)

1. **Matching lives in rocket-app, not rocket-network.** It needs `RequestSignatureSnapshot` from rocket-collection, and rocket-network stays pure. Cost if wrong: a file move.
2. **The verdict compares the offer's change with the consumer's own request fields directly.** The offer carries a change list, not a full signature, so `diff_signature` cannot run on the consumer side. The table in spec section 7 is applied field by field. A `body` change is `break` only when the policy marks it breaking and the request has a body. Cost if wrong: more mismatches on exotic fields.
3. **A new read-only `ContractService::pending_changes`** extracts the diff loop of `recompute_drift_for_collection` (there is no public "changes since snapshot" method today). Cost if wrong: a duplicate loop.
4. **The router gets a third service.** `FrameRouter::new(transfer, sync, contract_check)`; `ContractCheckService::claims` is true for the two new frames only.
