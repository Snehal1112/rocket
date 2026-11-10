# Contract Check Plan 2: Service and Wiring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `ContractCheckService` (provider and consumer sides), route the frames to it, wire it in the app and expose four commands.

**Architecture:** Two ports keep the service free of infra: `ContractChangesPort` (provider reads pending changes) and `LocalRequestsPort` (consumer reads its own requests). The service owns the pending checks, expiry and audit. `FrameRouter` gets a third service. Commands stay thin.

**Tech Stack:** Rust, Tauri 2, existing `PeerSender`, `EventPublisher`, `SecurityAuditPublisher`.

**Spec:** `docs/superpowers/specs/2026-11-09-team-network-contract-check-design.md` (sections 4, 6, 9). Index: `00-plan-index.md`. Needs Plan 1.

## Global Constraints

- One pending check per peer, three in total, else reply `busy`. Offer expiry 10 minutes (`CONTRACT_CHECK_TTL`).
- Frames from a peer that is not paired or is blocked are dropped (the existing check in `connections.rs`).
- Provider accepts a result once, only from the node the offer went to, with matching `check_id`, `rows.len()` equal to the change count, every `index` in range and unique. Anything else is dropped.
- Consumer never replies without the user's call to `reply`; `decline` sends `declined`.
- Consumer reads collections only through `LocalRequestsPort`, never writes, only the active workspace.
- Fixed error sentences, never peer strings. A bad offer: no reply, debug log, no audit.
- Provider message when the encoded offer exceeds the frame: `Check fewer changes at a time.`
- New commands are added to `src-tauri/app-commands.txt` (sorted) and `generate_handler!`.

## Review Focus

- Replayed `ContractCheckResult` (same `check_id` twice): second one dropped, no second audit row (Task 2 test).
- Result from the wrong device: dropped (Task 2 test).
- Fourth offer while three pending: `busy` goes back, nothing shown (Task 2 test).
- Offer expires while the dialog is open: `reply` fails with a fixed sentence and sends nothing (Task 2 test).
- A contract that has no changes since its snapshot: `start` refuses with a fixed sentence and sends nothing (Task 1 test).

---

### Task 1: Pending changes read and ports

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/contract_service.rs` (add `pending_changes`, reusing the loop at `recompute_drift_for_collection` ~l.675-742)
- Create: `crates/rocket-app/src/network/contract_check_ports.rs`
- Create: `src-tauri/src/network_contract_adapters.rs` (the two adapters)
- Modify: `crates/rocket-app/src/network/mod.rs`

**Interfaces:**
- Produces:

```rust
// contract_service.rs
impl ContractService {
    /// Read-only. Changes of the live collection against the signed snapshot.
    pub fn pending_changes(&self, collection_root: &Path, contract_id: Ulid) -> ContractResult<Vec<ChangelogEntry>>;
}
// contract_check_ports.rs
pub struct PendingContract { pub name: String, pub version: String, pub policy: ContractPolicyWire, pub changes: Vec<ContractChange> }
pub trait ContractChangesPort: Send + Sync { fn pending_changes(&self, collection_root: &str, contract_id: &str) -> DomainResult<PendingContract>; }
pub trait LocalRequestsPort: Send + Sync { fn local_requests(&self) -> DomainResult<Vec<LocalRequest>>; }
```

> **Ruling after Plan 1:** `diff_signature` writes field names `header.X`, `query_param.X`, `form_field.X`, `auth_type`, `auth_detail`, `method`, `url_pattern`, `body`. The wire and matching convention is `header:X`, `query:X`, `form:X`, `auth`, `body`, `path:X`. The adapter converts: `header.`→`header:`, `query_param.`→`query:`, `form_field.`→`form:`, `auth_type`/`auth_detail`→`auth`, `body`→`body`. `method` and `url_pattern` changes map to no field and are skipped. The adapter also skips entries with an empty method, path or field, and strips control characters from every string, because the receiver refuses such frames (Plan 1 Task 1). Tests: `adapter_converts_field_names_to_the_wire_convention`, `adapter_skips_unmapped_and_empty_entries`, `adapter_strips_control_characters`.

- [ ] **Step 1: Write failing tests**: `pending_changes_is_empty_when_nothing_changed`, `pending_changes_lists_an_added_header_and_a_removed_request` (reuse the fixtures of the existing drift tests), `pending_changes_writes_nothing` (file tree identical before and after). Adapter test `adapter_maps_entries_to_wire_changes`: `ChangelogEntry` with method and `http_path` becomes `ContractChange` (missing method or path: the entry is skipped; values cut by `cut_values`; sensitive values are redacted later by the service, not here).
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-app pending_changes` — expect FAIL.
- [ ] **Step 3: Implement** `pending_changes` by extracting the diff loop into a private helper shared with `recompute_drift_for_collection` (behaviour of the old method unchanged; its tests stay green). Implement the two adapters in `src-tauri` (`LocalRequestsPort` walks the active workspace's collections with the existing collection repository and builds `RequestSignatureSnapshot::from_request`).
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-app contract` and `cargo check -j4` — expect PASS.
- [ ] **Step 5: Commit** `feat(network): read pending contract changes for checks`.

### Task 2: ContractCheckService

**Files:**
- Create: `crates/rocket-app/src/network/contract_check.rs`, `contract_check_tests.rs`, `contract_check_fakes.rs` (fakes follow `sync_fakes.rs`)
- Modify: `crates/rocket-app/src/network/mod.rs`

**Interfaces:**
- Consumes: Plan 1 types and `verdicts`, `redact_change`; Task 1 ports; `PeerSender`, `EventPublisher`, `SecurityAuditPublisher`, `DeviceId`.
- Produces:

```rust
pub struct ContractCheckDeps { pub network: Arc<dyn PeerSender>, pub changes: Arc<dyn ContractChangesPort>,
    pub locals: Arc<dyn LocalRequestsPort>, pub events: Arc<dyn EventPublisher>, pub audit: Arc<dyn SecurityAuditPublisher> }
impl ContractCheckService {
    pub fn new(deps: ContractCheckDeps) -> Arc<Self>;
    pub fn start(&self, collection_root: &str, contract_id: &str, devices: &[DeviceId]) -> DomainResult<ContractCheckStarted>; // { check_ids: Vec<(DeviceId, ContractCheckId)>, changes: Vec<ContractChangeDto> }
    pub fn review(&self, check_id: &ContractCheckId) -> DomainResult<ContractCheckReview>; // per change: local match labels, verdict
    pub fn reply(&self, check_id: &ContractCheckId) -> DomainResult<()>;
    pub fn decline(&self, check_id: &ContractCheckId) -> DomainResult<()>;
    pub fn claims(&self, message: &PeerMessage) -> bool;               // true for the two frames
    pub fn on_frame(&self, from: DeviceId, message: PeerMessage);
    pub fn on_peer_gone(&self, device: DeviceId);
    pub fn sweep(&self, now: Instant);                                  // expires, emits Expired
}
```

- [ ] **Step 1: Write failing tests** (fake network records frames, fake clock via `sweep(now)`): `start_sends_one_offer_per_device_with_redacted_values`; `start_refuses_too_large_offer_with_the_fixed_sentence` (nothing sent); `start_refuses_when_there_are_no_changes`; `incoming_offer_emits_offer_received_and_waits_for_the_user`; `bad_offer_is_dropped_without_reply_or_audit`; `fourth_offer_gets_busy`; `second_offer_from_same_peer_gets_busy`; `review_lists_matches_and_verdicts_without_sending`; `reply_sends_one_row_per_change_in_order_and_audits_once`; `decline_sends_declined`; `reply_after_expiry_fails_and_sends_nothing`; provider side: `result_is_accepted_once` (replay dropped), `result_from_wrong_device_dropped`, `result_with_duplicate_or_out_of_range_index_dropped`, `result_with_wrong_row_count_dropped`, `accepted_result_emits_result_received_and_audits_sent`, `unknown_check_id_dropped`, `peer_gone_drops_its_pending`.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-app contract_check` — expect FAIL.
- [ ] **Step 3: Implement** the service with one `Mutex<State>` (provider map `check_id -> {device, change_count, deadline}`, consumer map `check_id -> {device, offer, deadline}`); never hold the lock while calling the network or ports. Audit kinds: `NetworkContractCheckSent` when a result is accepted (provider), `NetworkContractCheckReceived` when a reply or decline is sent (consumer), outcome strings `answered|declined|busy`.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-app contract_check`, `cargo clippy -j4 -p rocket-app` — expect PASS, no new lints.
- [ ] **Step 5: Commit** `feat(network): add contract check service`.

### Task 3: Router, wiring, commands and end-to-end

**Files:**
- Modify: `crates/rocket-app/src/network/connections.rs` (add the two variants to the or-pattern at ~l.392-402), `frame_router.rs` (third service; `on_frame`: contract-check `claims` first, then sync, then transfer; `on_peer_gone` calls all three), `frame_router` tests
- Modify: `src-tauri/src/lib.rs` (build the service after `sync_svc` ~l.1063, pass to `FrameRouter::new`, `app.manage`, register commands ~l.1262, a 30 s sweep ticker like the other network tickers)
- Create: `src-tauri/src/commands/network_contract_check.rs`; modify `src-tauri/src/commands/mod.rs`, `src-tauri/app-commands.txt`
- Test: `crates/rocket-app/tests` or the existing two-node rig used for sync e2e (`network_sync_e2e`), add `network_contract_check_e2e`

**Interfaces:**
- Produces commands (camelCase args, DTOs camelCase, errors `DomainError`):

```rust
network_contract_check_start(collection_root: String, contract_id: String, device_ids: Vec<String>) -> ContractCheckStartedDto
network_contract_check_review(check_id: String) -> ContractCheckReviewDto
network_contract_check_reply(check_id: String) -> ()
network_contract_check_decline(check_id: String) -> ()
```

- [ ] **Step 1: Write failing tests**: router test `contract_check_frames_go_to_the_contract_check_service_only`; connections test that both frames are delivered (not dropped as unknown); e2e on two real nodes: offer → toast event → review → reply → provider event with rows, and the Declined and Busy variants; command parse tests for bad ids (fixed sentence).
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-app frame_router`, then the e2e — expect FAIL.
- [ ] **Step 3: Implement** the router change, wiring order (service built before `FrameRouter::new`), the four thin commands (blocking work via `spawn_blocking` like `network_sync.rs`), `app-commands.txt` entries in sorted position, and the ticker.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-app network::`, `cargo test -j4 -p rocket-network`, `cargo check -j4`, `cargo clippy -j4 -p rocket-app` — expect PASS; `rocket` still starts (run `cargo check -j4` for `src-tauri`).
- [ ] **Step 5: Commit** `feat(network): route and expose contract checks`.
