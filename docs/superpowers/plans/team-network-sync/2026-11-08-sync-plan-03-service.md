# Team network sync: Plan 3, service

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 3 of 5.** Previous: `2026-11-08-sync-plan-01-domain.md` and `2026-11-08-sync-plan-02-storage-merge.md` (both applied and green). Next: `2026-11-08-sync-plan-04-tauri-e2e.md`. Recommended model: Opus. Index (binding names): `docs/superpowers/plans/team-network-sync/00-plan-index.md`. No commit during the plan.

**Goal:** Two paired nodes that hold a linked collection can sync it. `SyncService` holds the links (create at the first send, list, unlink, removal with the pairing), runs one sync session per peer in either role (starter or planner) on top of the existing `OutgoingTransfer` and `IncomingTransfer` machines for its three payload kinds (manifest, plan, files), plans with the Plan 1 three-way planner, asks the planner's user (`ReviewReady`, `review`) and the starter's user (`ConfirmNeeded`, `confirm`), applies through `SyncCollectionPort`, agrees on the new base without ever reading it from the peer, and reports `SyncResult`, events and audit. A new `FrameRouter` is the one `TransferSink` of the node: sync frames go to `SyncService`, everything else to `TransferService`. `TransferService::send` gains `keep_in_sync`, and both ends of a completed linked send create their link from the same ZIP bytes. Everything is tested with fakes. No Tauri command and no real transport (Plan 4).

**Architecture:** `SyncService` is a sibling of `TransferService` in `crates/rocket-app/src/network/`, with the same shape: one `std::sync::Mutex` over its state, one bounded frame queue and one worker that only runs short sync handlers, and spawned tasks for every slow step (link reads, manifest export, planning, file export, ZIP pack and unpack, apply, link save, outbound frames). Its payloads are plain `OutgoingTransfer` and `IncomingTransfer` machines whose `CollectionOffer` is never sent: the service announces each payload with one `SyncOffer` built from the machine's offer fields, and the receiving node answers it with `OfferAnswer { accept: true }` on its own. The bytes stay in memory (at most `MAX_TRANSFER_BYTES` per payload). `FrameRouter` asks `SyncService::claims(from, frame)` (a short lock and a map lookup) and hands the frame to one service. Pure rules (path checks, plan checks, the starter's new base, the split of planner actions, the summary, the script and auth marks, the stop reasons and their fixed texts) live in `sync_checks.rs` and are tested as functions.

**Tech Stack:** Rust, tokio (`test-util`), async-trait, sha2, base64, serde_json, serde_yaml, `zip` 2 (moved from dev-dependencies to dependencies of `rocket-app`, in memory only), `rocket-network` (Plan 1), the `sync_ports.rs` ports (Plan 2), the transfer service (send project).

**Spec:** `docs/superpowers/specs/2026-11-08-team-network-sync-collection-design.md`, sections 2 (decisions 2, 3, 4, 5, 7, 8, 9, 11), 3, 5, 6, 7, 8, 10, 11 (`rocket-app` row), 12 (`rocket-app` bullet). Index deviations 1 to 3 apply.

## Needs from Plan 1 and Plan 2

Task 1 Step 2 greps for each name and stops (a `Ruling:` line in the ledger, then a fix in the owning plan's file) if one is missing.

- Plan 1 `rocket-network`: `limits::{PROTOCOL_VERSION = 3, MAX_MANIFEST_ENTRIES, SYNC_PAYLOAD_NAME_MAX}`; `LinkId`, `SyncId` (`random`, `parse`, `as_str`, `Clone + Eq + Hash + Debug`); `Manifest` (`new`, `insert`, `get`, `paths`, `len`, `base_hash`, `to_json`, `from_json`, `Clone + PartialEq + Debug`); `LinkState`, `LinkRecord` (`Clone + Debug`); `SyncAction`, `ConflictKind`, `Conflict`, `Resolution`, `Decision`, `SyncPlan`, `plan_sync`, `resolve`, `new_base`; `PeerMessage::{SyncOffer, SyncAbort, SyncResult}`, `SyncPayloadKind`, `SyncAbortReason`, `SyncOutcome`; `CollectionOffer { .., keep_in_sync: bool, link_id: Option<String> }`; `SyncPlanPayload { wanted: Vec<String>, deletes: Vec<String>, renames: Vec<PlanRename { from, to }>, decisions: Vec<Decision> }` with `to_json` and `from_json`; `SyncEvent`, `SyncSummary`, `ConflictView`, `SyncRole`, `SyncFinish` (`SyncEvent: Clone`, implements `PublishedEvent`); `LinkRepository`.
  - If the rename item of `SyncPlanPayload` has another name, this plan's `PlanRename` means that type (one `Ruling:` line).
  - `Manifest::insert` is the path rule of spec section 7 (relative, `/` separated, no `..`, no empty segment, no `.rocket` segment, no device names). This plan never re-implements it; `sync_checks::path_allowed` calls it.
- Plan 1 `rocket-audit`: `AuditEventKind::{NetworkSyncSent { device_id, collection, outcome }, NetworkSyncReceived { device_id, collection, outcome }, NetworkLinkRemoved { device_id, collection }}`.
- Plan 1 keeps `rocket-app` green after the new `PeerMessage` variants and the two new `CollectionOffer` fields: `run_socket` has an arm for the three `Sync*` variants and `TransferService::handle_frame` destructures `CollectionOffer { .., }` with the new fields. If Plan 1 did not do this, Task 1 Step 9 below does it (it is written so that it works either way).
- Plan 2 `crates/rocket-app/src/network/sync_ports.rs`: `SyncFile { path, bytes }`, `FileChange::{Put, PutAs, Delete, Rename}` (`Debug + Clone + PartialEq`), `SyncCollectionPort::{exists, collection_uid, manifest, export_files, apply}`, `manifest_from_zip(&Path)`.
- Plan 2 also adds `SyncCollectionPort::collection_root(&self, collection: &str) -> DomainResult<String>` (the canonical folder path the link stores, spec section 5). The index does not list it. If Plan 2 did not add it, Task 1 Step 3 adds it to the trait (one `Ruling:` line) and to the Plan 2 adapter (`collection_root` returns the canonical path of `<workspace>/collections/<name>` through the same repository the adapter already uses for `exists`).

## Rulings recorded by this plan

1. **S1 Two plan payloads.** A session carries at most two `Plan` payloads from the planner: a *request* (`wanted` set, everything else empty, `new_base_hash: None`) and a *final* plan (`wanted` empty, `new_base_hash: Some`). The planner needs the starter's bytes of every file it may take, and of every conflict, before its user decides: the conflict diff (spec section 6, step 6) and the script and auth marks (spec section 3) need them. So the request goes out right after planning, the starter answers with a `Files` payload, and only then is `ReviewReady` published. The request is skipped when nothing is wanted. The starter accepts a final plan in either `AwaitPlan` phase. Cost if wrong: one extra small transfer per sync.
2. **S2 `SyncPlanPayload` gains two fields** in `crates/rocket-network/src/link.rs` (a Plan 1 file, Task 2 Step 2): `#[serde(default)] sends: Vec<PlanFile>` with `PlanFile { path: String, sha256: String }` (the files the planner will send, with their hashes) and `#[serde(default)] new_base_hash: Option<String>`. The starter never sees the planner's manifest, so without them it can neither check the files it receives against the plan (spec section 7, third bullet) nor know whether files come at all. The starter computes its own new base from its own manifest and the plan (`starter_new_base`) and refuses the plan when the hash differs. Only the hash crosses the wire (spec section 7, last bullet). Cost if wrong: none at runtime; a reviewer who wants the payload frozen moves the two fields into Plan 1.
3. **S3 Payload transport.** Each payload is one transfer id with one machine on each side. `OutgoingTransfer::new(id, kind_name(kind), len, sha, now)` and `IncomingTransfer::from_offer(.., kind_name(kind), ..)`, with `kind_name` = `"manifest"`, `"plan"`, `"files"` (each within `SYNC_PAYLOAD_NAME_MAX`). The machine's `offer_message()` is destructured into a `SyncOffer`; the `CollectionOffer` never leaves. The receiving node registers the transfer id, then sends `OfferAnswer { accept: true }` itself, so the id is known to the router before any chunk can come. The receiver acks as the transfer receiver does (every 16 chunks and the last, through one ack pump per payload). The sender's payload ends at the final `ChunkAck` (`awaiting_result()`); no `TransferResult` is sent for a sync payload, and one that arrives is ignored. The bytes are kept in memory: the machine caps a payload at `MAX_TRANSFER_BYTES`, and one session per peer (S6) caps the memory at 20 MiB per peer. `SyncDeps.staging` is kept because the index names it, and this plan does not use it (Plan 4 passes the same `FsTransferStaging`).
4. **S4 Worker design (send ruling R11).** `on_frame` and `on_peer_gone` enqueue into a bounded queue (`SYNC_FRAME_QUEUE = 256`, `try_send`); a full queue drops a frame (the session then fails by the sequence rule or its idle wait), a peer-gone notice that does not fit runs inline. One worker handles items in order and only runs sync handlers that take the lock, change the session, and spawn. Link reads and writes, `manifest`, `plan_sync`, `export_files`, ZIP pack, unpack and validation, SHA-256, `apply` and every outbound frame run in their own spawned task (`spawn_blocking` for the blocking ones). A task that finishes looks the session up again by `SyncId` and stops when it is gone or its phase changed, so whoever removes a session emits its single `Finished`. The lock is never held across an `.await`.
5. **S5 Caps and rate limits, frame by frame.** `SyncService` holds its own `TransferLimits` (`sync_limits`) with the same constants; it never touches `TransferService`'s, and collection offers never reach it. So a sync cannot take a collection transfer slot (no starvation), and nothing a sync does can open a collection transfer (no bypass). What counts:
   - `SyncOffer { kind: manifest }` (a new session): `sync_limits.admit_offer(from, transfer_id, now)`, so it costs one hit in the per-peer window of `OFFERS_PER_PEER_PER_MINUTE` even when refused, and it is refused `busy` above `MAX_PENDING_INCOMING` sessions waiting for this node's user or when the peer already has a session here. The entry stays `Pending` until the review is answered (`mark_active`), and is released when the session ends.
   - `start` (a new outgoing session): `sync_limits.begin_outgoing(peer, key)`, one per peer.
   - `SyncOffer { kind: plan | files }`: no limits call. It is admitted only for the session's own `sync_id`, from the session's peer, with the session's `link_id` and `base_hash`, in the one phase that expects it; any other is a protocol error that ends that session `Failed` with `SyncAbort { failed }`, and a `SyncOffer` of those kinds for an unknown `sync_id` is ignored.
   - `OfferAnswer`, `Chunk`, `ChunkAck`, `TransferAbort`, `TransferResult` with a registered sync transfer id from the session's peer: no limits call; the machines bound them (the ack window, exact chunk lengths, the chunk count). The same frames with any other id go to `TransferService` and count there as before.
   - `SyncAbort`, `SyncResult`: no limits call; they only end or advance a session that exists, and are ignored otherwise.
   - The sync queue is separate from the transfer queue, so a flood of sync frames cannot fill the transfer queue.
   - `run_socket`: `SyncOffer`, `SyncAbort` and `SyncResult` count as a pong and keep the per-frame block-list read; `Chunk` and `ChunkAck` keep skipping it (send ruling R13).
6. **S6 One session per peer.** At most one session (either role) per peer, which also gives one per link. `start` refuses a second one with "A sync with that node is already running."; a `SyncOffer { manifest }` from a peer with a session is answered `SyncAbort { busy }`. Two users pressing Sync at each other at the same moment both see "The other node is busy with another sync. Try again." Cost if wrong: a user with two linked collections to one peer syncs them one after the other.
7. **S7 Apply order and the unconfirmed rule.** The starter applies first (after its user confirmed) and then sends `SyncResult { applied }`; the planner applies only after that frame. So a decline, an expiry or a cancel on either side leaves both collections unchanged (spec section 3), and the planner, which applies last, never has an unconfirmed outcome. The base is written only when both applies are known to have succeeded:
   - Starter, peer `applied`: new base, `Finished { applied }`.
   - Starter, peer `failed` (or a `SyncAbort` while waiting for the result): old base, link `out_of_date`, `Finished { failed, "The other node could not apply the changes." }`. The next sync starts from the old bases on both sides and finds the starter's files already equal to the planner's (spec section 6, step 9).
   - Starter, its own `SyncResult` could not be sent (`send_to` failed): the planner certainly did not apply. Old base, `out_of_date`, `Finished { unconfirmed, "Applied here, but the other node did not get the result. Sync again." }`.
   - Starter, its own `SyncResult` was sent, then the peer went away or `timing.result_timeout` passed: **unconfirmed**. New base, link `out_of_date`, `Finished { unconfirmed, "Applied here, but the other node did not confirm." }`, audited `unconfirmed`. The planner had every byte and its user's accept, so it most likely applied. Cost if wrong: if the planner did fail, the next sync ends with the base-mismatch text and the user unlinks and sends again.
   - Planner: a peer-gone notice during its apply does not stop the apply (as the transfer finalize step); the link is saved only while the peer is still paired.
8. **S8 Base agreement.** Each side computes its new base locally. The planner uses `new_base(base, mine, theirs, actions, decisions)` and checks before it sends the final plan that it equals both `apply_to_manifest(mine, local ops, theirs)` (what it will have) and `starter_new_base(theirs, final)` (what the starter will compute); a difference is an internal error that ends the session `Failed` before anything is sent. The starter recomputes and compares with `new_base_hash`. Nothing a peer sends is ever stored as a base. Base files are the bytes that crossed the wire or the bytes this node exported for the plan; a path whose agreed hash equals the old base keeps its old base file; any other path is exported again after the apply and stored only when its hash matches.
9. **S9 The collection lock.** `SyncCollectionPort::apply` takes the collection lock and runs one file transaction (Plan 2). This plan cannot hold that lock between planning and apply, because the port has no call that spans both. Instead every apply is preceded, in the same blocking task, by a fresh `manifest()` that must equal the manifest the plan was built from (planner: `mine`; starter: the manifest it offered); a difference ends the session `Failed` with "The collection changed during the sync. Sync again." and `apply` is not called. A local save between that check and `apply` is the remaining window (milliseconds). Cost if wrong: one save made in that window is merged as if it were the planned version. The full fix is a port call `apply_if_unchanged(collection, expected: &Manifest, changes)` that checks inside the lock (reported to the index).
10. **S10 What every received thing must pass before it is used.** Manifest: `Manifest::from_json` (paths, hashes, count) plus `path_allowed` for every path. Request plan: every `wanted` path is in the manifest this node offered. Final plan: every path passes `path_allowed`; `deletes` and every rename `from` are in the offered manifest; a rename `to` is not in it after the deletes and renames; `sends` paths are unique and their hashes are 64 lowercase hex; `new_base_hash` equals `starter_new_base(offered, plan)`. Files: the whole payload's size and SHA-256 match its `SyncOffer` (the machine checks every chunk; the service checks the total after the last chunk); the ZIP passes `rocket_import::transfer_zip::validate_bytes` (the strict validator); its entry set equals the expected set exactly (planner: `wanted`; starter: the `sends` paths) and each entry's SHA-256 equals the expected hash (planner: the starter's manifest; starter: `sends`). A wrong set is "unsafe", a wrong hash is "damaged". All of this runs before any dialog or apply.
11. **S11 Settings files.** `path_allowed` refuses `opencollection.yml` and `bruno.json` anywhere but the collection root, so a sync cannot create a nested collection. `review` refuses `keep_both` for a conflict on `opencollection.yml` or any `folder.yml` ("Keep both is not possible for collection or folder settings."), so a settings file is always a `Put` (merged by Plan 2's `merge_file`, which keeps the local `extensions.rocketapi`) and never a `PutAs` (written as received). Trust itself lives in `~/.rocket-api/trust.yml`, outside every collection, and `SyncDeps` has no trust port, so a sync cannot write a trust record.
12. **S12 An empty sync.** When the plan has no action and no conflict, the planner skips the review and sends an empty final plan at once; the starter skips the confirm; nobody calls `apply`; both sides still store the new base (both may have made the same change). The planner ends `Finished { applied, "Already in sync." }`; the starter ends `Finished { applied }` with no text, because an empty final plan only says that nothing changes on the starter (the planner may still take files).
13. **S13 Audit.** One row per session that ended after at least one chunk of any payload left this node, with the outcome's wire name (`finish_name`): the starter writes `NetworkSyncSent`, the planner `NetworkSyncReceived`; `device_id` is the short id, `collection` the local collection name. A session refused before any chunk (busy, base mismatch, link missing, a start that failed) writes none. `unlink` and a removed pairing write one `NetworkLinkRemoved` per link. Nothing else is audited.
14. **S14 Fixed texts.** Every `Finished.message` and every error is a fixed sentence from `sync_checks.rs`. No peer text, file name, path or file content is ever put in an event, an error or a log line above debug.
15. **S15 Link state.** Stored by the service: `in_sync` or `changes_waiting` after a successful sync (the fresh manifest after the apply equals the new base, or not), `out_of_date` after a failed or unconfirmed one, `broken` when this node finds the folder or uid changed or the peer answers `link_missing`. A base mismatch leaves the state alone. `links()` derives, without saving: `broken` when the collection is gone or its uid or root differ from the record, `changes_waiting` when a stored `in_sync` link's fresh manifest hash differs from its base hash. `links()` blocks (it runs the export per link); Plan 4 calls it in `spawn_blocking`. A state change rewrites the record with its current base files (`LinkRepository` has no state-only call).
16. **S16 Pairing removed.** On `on_peer_gone` the worker spawns a blocking task that asks `peer_label(device)`; only when it is `None` (not paired or blocked) are that peer's links deleted, each with `NetworkLinkRemoved` and `LinkChanged`. A dropped connection keeps the links. `peer_label` reads a read error as "not paired", so the task asks twice, 50 ms apart, before it deletes. Cost if wrong: two failed reads in a row remove the links and the user sends again with Keep in sync.
17. **S17 Cancel.** Before this node's dialog: `SyncAbort { cancelled }` and `Finished { cancelled }`. During this node's dialog: the same as Decline. After this node's user answered: refused with "The sync is already being applied." An unknown or finished `sync_id` is `Ok(())`.
18. **S18 TransferService integration.** `send(device, collection, include_environments, keep_in_sync)`. With `keep_in_sync` the sender makes a `LinkId` and puts `keep_in_sync: true, link_id: Some(..)` on the `CollectionOffer`; `keep_in_sync` with `include_environments` is refused before any export ("Keep in sync does not send environments. Turn one of them off."), because environments are not synced (spec decision 6) and would show as deletions at the first sync. On `Completed` (the receiver's `completed` result) the sender creates its link; `Unconfirmed` and every other outcome create none. The receiver keeps a copy of the chunks of a `keep_in_sync` offer in memory and creates its link after a successful import, under the imported name. Both call `SyncService::link_first_transfer(link_id, peer, collection, zip)` in `spawn_blocking`, which builds the base with `base_from_zip` (the same bytes on both sides, index deviation 3) and the record with `collection_root` and `collection_uid`. A failure there is logged at warn with a code and never changes the transfer outcome. `TransferService::set_sync(Arc<SyncService>)` is the hook; without it `keep_in_sync` is refused ("Keep in sync is not available."). A `keep_in_sync` offer whose `link_id` does not parse is refused `TransferAbort { failed }`.
19. **S19 `base_from_zip` instead of `manifest_from_zip` at runtime.** The sender has its ZIP in memory and both sides need the file bytes as well as the hashes, so the base is built by `sync_payload::base_from_zip(&[u8]) -> DomainResult<(Manifest, Vec<(String, Vec<u8>)>)>` (strict validation first, the single root folder stripped). The test `base_from_zip_matches_manifest_from_zip` pins it to Plan 2's `manifest_from_zip` over the same bytes.
20. **S20 Names the index does not give.** `SyncDeps.audit` is `Arc<dyn SecurityAuditPublisher>` (the index writes `AuditPublisher`, which does not exist). `LinkView { link_id, device_id, display_name, short_id, collection_name, state, updated_at }` (matches Plan 4's `LinkDto`). `ConflictFiles { mine: Option<String>, theirs: Option<String> }` and `SyncService::conflict_files(sync_id, path)` give Plan 4 and Plan 5 the two texts of a conflict diff (lossy UTF-8; the planner already holds both). `TransferService::set_sync`, `TransferService::has_transfer` (`pub(crate)`), `SyncService::link_first_transfer` and `SyncService::claims` (`pub(crate)`), `FrameRouter::new(transfer, sync) -> Arc<FrameRouter>`.
21. **S21 In-memory ZIP.** `zip = "2"` moves from `[dev-dependencies]` to `[dependencies]` of `rocket-app` (the same version `rocket-export` and `rocket-import` use). `rocket-import` gains `pub fn validate_bytes(bytes: &[u8]) -> ImportResult<()>` (the private `validate_archive` over a `Cursor`). Nothing touches a file.

## The session state machine

Times: `T` = `timing.offer_ttl` (120 s), `I` = `timing.idle_limit` (120 s), `R` = `timing.result_timeout` (60 s). "Payload out" is: `SyncOffer` sent, the answer awaited (`I`), chunks streamed inside the ack window, the final `ChunkAck` awaited (`I`). "Payload in" is: `OfferAnswer { accept: true }` sent, chunks awaited (`I` since the last one), size and SHA-256 checked. `Finished` is always the one `SyncEvent::Finished` of the session; "audit" is the S13 row and is written only when a chunk of this node left.

### Starter (the node whose user pressed Sync)

| # | State | Event | Next | Frames sent | Events | Audit, link | Test |
|---|---|---|---|---|---|---|---|
| S1 | none | `start` passes every check | `SendManifest` | `SyncOffer { manifest }` | none | none | `start_offers_the_manifest_with_the_base_hash` |
| S2 | none | `start` fails a check (no link, broken, not paired, offline, busy, folder or uid changed) | none | none | none | broken link saved for the folder case | `start_refuses_with_the_reason_and_sends_nothing` |
| S3 | `SendManifest` | final `ChunkAck` | `AwaitPlan` (`T + I`) | chunks | none | none | `start_offers_the_manifest_with_the_base_hash` |
| S4 | `AwaitPlan` | `SyncOffer { plan }`, request | `RecvPlan` then `Exporting` | `OfferAnswer` | none | none | `a_request_plan_is_answered_with_the_wanted_files` |
| S5 | `Exporting` | wanted files exported, hashes equal the offered manifest | `SendFiles` | `SyncOffer { files }` | `Progress` | none | `a_request_plan_is_answered_with_the_wanted_files` |
| S6 | `SendFiles` | final `ChunkAck` | `AwaitPlan` (`T + I`) | chunks | `Progress` | none | `a_request_plan_is_answered_with_the_wanted_files` |
| S7 | `AwaitPlan` | `SyncOffer { plan }`, final, valid, `sends` not empty | `AwaitFiles` (`I`) | `OfferAnswer` | none | none | `a_final_plan_with_files_asks_for_confirmation` |
| S8 | `AwaitFiles` | files payload valid | `AwaitConfirm` (`T`) | `OfferAnswer` | `Progress`, `ConfirmNeeded` | none | `a_final_plan_with_files_asks_for_confirmation` |
| S9 | `AwaitPlan` | final plan valid, `sends` empty, deletes or renames | `AwaitConfirm` (`T`) | `OfferAnswer` | `ConfirmNeeded` | none | `a_final_plan_with_only_deletes_asks_for_confirmation` |
| S10 | `AwaitPlan` | final plan with no change for this node | `Applying` (no `apply` call) | `OfferAnswer` | none | none | `an_empty_sync_ends_applied_without_dialogs` |
| S11 | `AwaitConfirm` | `confirm(true)` | `Applying` | none | none | none | `the_starter_applies_then_reports_and_stores_the_base_on_applied` |
| S12 | `Applying` | fresh manifest equal, `apply` ok, `SyncResult` sent | `AwaitResult` (`R`) | `SyncResult { applied }` | none | none | `the_starter_applies_then_reports_and_stores_the_base_on_applied` |
| S13 | `AwaitResult` | `SyncResult { applied }` | end | none | `Finished { applied }`, `LinkChanged` | `applied`; new base, `in_sync` or `changes_waiting` | `the_starter_applies_then_reports_and_stores_the_base_on_applied` |
| S14 | `AwaitResult` | `SyncResult { failed }` or any `SyncAbort` | end | none | `Finished { failed }`, `LinkChanged` | `failed`; old base, `out_of_date` | `a_peer_apply_failure_keeps_the_old_base_out_of_date` |
| S15 | `AwaitResult` | peer gone, or `R` passes | end | none | `Finished { unconfirmed }`, `LinkChanged` | `unconfirmed`; new base, `out_of_date` | `a_peer_gone_after_our_result_is_unconfirmed_with_the_new_base`, `no_result_within_the_timeout_is_unconfirmed` |
| S16 | `Applying` | `SyncResult` send fails | end | none | `Finished { unconfirmed }`, `LinkChanged` | `unconfirmed`; old base, `out_of_date` | `a_result_that_cannot_be_sent_keeps_the_old_base` |
| S17 | `Applying` | `apply` fails (rolled back) | end | `SyncResult { failed }` | `Finished { failed }`, `LinkChanged` | `failed`; old base, `out_of_date` | `an_apply_failure_rolls_back_and_goes_out_of_date` |
| S18 | `Applying` | fresh manifest differs | end | `SyncResult { failed }` | `Finished { failed, changed }` | `failed`; old base, `out_of_date` | `a_local_edit_before_the_apply_stops_it` |
| S19 | `AwaitConfirm` | `confirm(false)` or `cancel` | end | `SyncAbort { declined }` | `Finished { declined }` | `declined` | `declining_the_confirm_changes_nothing` |
| S20 | `AwaitConfirm` | `T` passes | end | `SyncAbort { expired }` | `Finished { expired }` | `expired` | `an_unanswered_confirm_expires` |
| S21 | any state before `AwaitConfirm` | `cancel` | end | `SyncAbort { cancelled }` | `Finished { cancelled }` | `cancelled` | `cancel_before_the_dialog_ends_the_sync_on_both_sides` |
| S22 | `Applying`, `AwaitResult` | `cancel` | same | none | none | none (error "The sync is already being applied.") | `cancel_after_confirm_is_refused` |
| S23 | any state before `Applying` | `SyncAbort { r }` | end | none | `Finished { map(r) }` | outcome; `link_missing` saves `broken` | `each_abort_reason_ends_the_starter_with_its_outcome` |
| S24 | any state before `Applying` | peer gone | end | none | `Finished { failed, went offline }` | `failed` | `a_peer_gone_before_the_apply_changes_nothing` |
| S25 | `RecvPlan`, `AwaitFiles` | a bad payload (damaged, unsafe, bad plan, base hash differs) | end | `SyncAbort { failed }` | `Finished { failed, text }` | `failed` | Task 3 safety tests |
| S26 | waiting state (`AwaitPlan`, `AwaitFiles`, payload out or in) | its wait passes | end | `SyncAbort { failed }` | `Finished { failed, stalled }` | `failed` | `a_silent_planner_fails_the_starter_after_the_wait` |
| S27 | `Exporting` | exported file hash differs from the offered manifest | end | `SyncAbort { failed }` | `Finished { failed, changed }` | `failed` | `a_local_edit_before_the_export_stops_the_sync` |
| S28 | `Exporting` | files ZIP above `MAX_TRANSFER_BYTES` | end | `SyncAbort { too_big }` | `Finished { failed, too big with size }` | `failed` | `too_big_changes_are_refused_before_any_offer` |
| S29 | any | a frame out of phase (a second manifest kind, a `SyncOffer` with another `link_id` or `base_hash`) | end | `SyncAbort { failed }` | `Finished { failed, bad plan }` | `failed` | `a_sync_offer_out_of_phase_ends_the_session` |

### Planner (the node that received the manifest)

| # | State | Event | Next | Frames sent | Events | Audit, link | Test |
|---|---|---|---|---|---|---|---|
| P1 | none | `SyncOffer { manifest }` refused by `sync_limits` or S6 | none | `SyncAbort { busy }` | none | none | `a_fourth_waiting_sync_is_busy`, `a_second_session_with_one_peer_is_busy`, `the_sync_rate_window_refuses_the_sixth_start_in_a_minute` |
| P2 | none | `SyncOffer { manifest }` admitted | `Checking` | none | none | none | `a_matching_manifest_is_planned_and_reviewed` |
| P3 | `Checking` | no link, link of another peer, or link `broken` | end | `SyncAbort { link_missing }` | none | none | `an_unknown_or_broken_link_is_link_missing` |
| P4 | `Checking` | collection gone, uid or root changed | end | `SyncAbort { link_missing }` | none | link saved `broken` | `a_moved_collection_is_link_missing_and_broken` |
| P5 | `Checking` | `base_hash` differs | end | `SyncAbort { base_mismatch }` | none | none | `a_base_mismatch_is_answered_and_changes_nothing` |
| P6 | `Checking` | link ok | `RecvManifest` | `OfferAnswer` | none | none | `a_matching_manifest_is_planned_and_reviewed` |
| P7 | `RecvManifest` | manifest valid | `Planning` | none | none | none | `a_matching_manifest_is_planned_and_reviewed` |
| P8 | `Planning` | plan has wanted paths | `SendRequest` then `AwaitFiles` (`I`) | `SyncOffer { plan }` (request) | none | none | `a_matching_manifest_is_planned_and_reviewed` |
| P9 | `AwaitFiles` | files payload valid | `AwaitReview` (`T`) | `OfferAnswer` | `Progress`, `ReviewReady { role: planner }` | none | `a_matching_manifest_is_planned_and_reviewed` |
| P10 | `Planning` | nothing wanted, plan not empty | `AwaitReview` (`T`) | none | `ReviewReady` | none | `a_delete_only_plan_is_reviewed_without_a_request` |
| P11 | `Planning` | plan empty | `Finalizing` | none | none | none | `an_empty_sync_ends_applied_without_dialogs` |
| P12 | `AwaitReview` | `review(false)` or `cancel` | end | `SyncAbort { declined }` | `Finished { declined }` | `declined` | `declining_the_review_changes_nothing_on_both_sides` |
| P13 | `AwaitReview` | `T` passes | end | `SyncAbort { expired }` | `Finished { expired }` | `expired` | `an_unanswered_review_expires_on_both_sides` |
| P14 | `AwaitReview` | `review(true, d)` with bad decisions | same | none | none | none (error) | `review_needs_one_valid_decision_per_conflict` |
| P15 | `AwaitReview` | `review(true, d)` valid | `Finalizing` | none | none | `mark_active` | `a_matching_manifest_is_planned_and_reviewed` |
| P16 | `Finalizing` | sends exported, hashes equal `mine`, base checks agree | `SendFinal` | `SyncOffer { plan }` (final) | none | none | `the_final_plan_lists_the_peer_changes_and_the_new_base_hash` |
| P17 | `SendFinal` | final ack, `sends` not empty | `SendFiles` | `SyncOffer { files }`, chunks | `Progress` | none | `the_final_plan_lists_the_peer_changes_and_the_new_base_hash` |
| P18 | `SendFinal`, `SendFiles` | final ack | `AwaitStarter` (`T + I`) | none | none | none | `the_final_plan_lists_the_peer_changes_and_the_new_base_hash` |
| P19 | `AwaitStarter` | `SyncResult { applied }` | `Applying` | none | none | none | `the_planner_applies_only_after_the_starter_applied` |
| P20 | `Applying` | fresh manifest equal, `apply` ok | end | `SyncResult { applied }` | `Finished { applied }`, `LinkChanged` | `applied`; new base | `the_planner_applies_only_after_the_starter_applied` |
| P21 | `Applying` | `apply` fails or fresh manifest differs | end | `SyncResult { failed }` | `Finished { failed }`, `LinkChanged` | `failed`; old base, `out_of_date` | `a_planner_apply_failure_tells_the_starter_and_goes_out_of_date` |
| P22 | `Applying` | peer gone | `Applying` (continues) | the result send fails quietly | `Finished { applied }` | `applied`; new base only while still paired | `a_peer_gone_during_the_planner_apply_does_not_stop_it` |
| P23 | `AwaitStarter` | `SyncResult { failed }` | end | none | `Finished { failed }`, `LinkChanged` | `failed`; `out_of_date` | `a_starter_apply_failure_marks_the_planner_out_of_date` |
| P24 | `AwaitStarter` | `SyncAbort { declined, expired, cancelled }` | end | none | `Finished { map(r) }` | outcome | `declining_the_confirm_changes_nothing` |
| P25 | `AwaitReview`, `AwaitStarter`, `Applying` | `cancel` | same | none | none | none (error "The sync is already being applied." after the review was answered; in `AwaitReview` it declines, P12) | `cancel_after_confirm_is_refused` |
| P26 | any state before `AwaitReview` | `cancel` | end | `SyncAbort { cancelled }` | `Finished { cancelled }` | `cancelled` if a chunk left | `cancel_before_the_dialog_ends_the_sync_on_both_sides` |
| P27 | any state before `Applying` | peer gone | end | none | `Finished { failed, went offline }` | `failed` if a chunk left | `a_peer_gone_before_the_apply_changes_nothing` |
| P28 | `RecvManifest`, `AwaitFiles` | a bad payload (damaged, unsafe, bad manifest) | end | `SyncAbort { failed }` | `Finished { failed, text }` | `failed` if a chunk left | Task 3 safety tests |
| P29 | waiting state | its wait passes | end | `SyncAbort { failed }` | `Finished { failed, stalled }` | `failed` if a chunk left | `a_silent_starter_fails_the_planner_after_the_wait` |
| P30 | `Finalizing` | an exported send differs from `mine` | end | `SyncAbort { failed }` | `Finished { failed, changed }` | none | `a_local_edit_during_the_review_stops_the_sync` |
| P31 | `Finalizing` | files ZIP above `MAX_TRANSFER_BYTES` | end | `SyncAbort { too_big }` | `Finished { failed, too big }` | `failed` | `too_big_changes_are_refused_before_any_offer` |

`map(r)`: `declined` -> `Declined`, `expired` -> `Expired`, `cancelled` -> `Cancelled`, `base_mismatch` -> `BaseMismatch`, `link_missing` -> `LinkMissing`, `busy` -> `Failed` with the busy text, `too_big` -> `Failed` with "The changes are too big to sync (limit 10 MiB).", `failed` -> `Failed` with "The sync failed on the other node."

## Global Constraints

- `-j4` on every cargo command, one crate at a time. Never `cargo test --workspace` or `--all`. Filters: `sync_tests`, `sync_run_tests`, `sync_safety_tests`, `sync_checks`, `frame_router`, `transfer_link_tests`, `transfer_send_tests`, `transfer_receive_tests`, `e2e_transfer_tests`. Never the bare word `sync` (it matches the OpenAPI sync tests).
- No bare unwrap call anywhere in the new text, tests included (a repo hook rejects it). Tests use `expect("reason")`. Mutex locks use `.lock().unwrap_or_else(|e| e.into_inner())` in production code and `.expect("... lock")` in fakes.
- Never hold the `std::sync::Mutex` guard across an `.await`. Publishing an event under the lock is allowed (the publisher only enqueues), so a `Progress` stays before its `Finished`.
- `rocket-app` holds only `Arc<dyn Trait>` ports and never names an infra type. No file is read or written by this plan's code; ZIP work is in memory.
- Blocking work runs in `tokio::task::spawn_blocking`: every `LinkRepository` and `SyncCollectionPort` call, `plan_sync`, `resolve`, `new_base`, ZIP pack and unpack, SHA-256 of a payload, JSON of a manifest or a plan.
- Time in tests: the tests reach `spawn_blocking`, so they use the real clock, `wait_for` and short `Timing` values (`fast()` in `sync_fakes.rs`: `offer_ttl` 300 ms, `idle_limit` 300 ms, `result_timeout` 300 ms). A test of a 120 s default only checks `Timing::default()`.
- Events and audit are fire-and-forget. Logs carry codes and counts only; never a collection name, a path, a payload or a full device id.
- `#[cfg(test)] pub(crate)` for every test hook, and the fields they need are `#[cfg(test)]` too. `new_with_timing` stays `#[doc(hidden)] pub`.
- Comments are short full sentences that end with a period. No emojis.
- No commit during the plan. The user reviews the whole plan, then commits through `dev-workflow-skills:1-git-commit` with explicit paths.

## Review Focus

1. **The state table is the code.** Every row S1 to S29 and P1 to P31 has the test named in its row; a reviewer checks each test drives exactly that row (the state before, the event, the frames, the events, the audit and the link). Tests: as listed in the tables (Tasks 2 and 3).
2. **The worker never waits.** `on_frame` only enqueues; the worker only locks, decides and spawns. Test: `a_slow_apply_does_not_hold_back_another_peer` (Task 3) and the grep step in Task 3 that no `.await` and no port call appears in a `handle_*` function.
3. **Routing cannot be hijacked.** A sync transfer id is claimed only for its own peer; a `SyncOffer` that reuses an id `TransferService` knows is dropped; a `CollectionOffer` always reaches `TransferService`. Tests: `the_router_sends_sync_frames_to_sync_and_others_to_transfer`, `frames_of_a_sync_transfer_id_go_to_sync_only_from_its_peer`, `a_sync_offer_reusing_a_transfer_id_is_dropped` (Task 1).
4. **Caps (S5, S6).** Tests: `a_fourth_waiting_sync_is_busy`, `a_second_session_with_one_peer_is_busy`, `the_sync_rate_window_refuses_the_sixth_start_in_a_minute`, `follow_up_sync_offers_do_not_count_against_the_rate_window`, `a_sync_does_not_use_the_collection_transfer_slots`, `sync_frames_reach_the_sink_and_count_as_pongs` (Tasks 1 and 3).
5. **Unconfirmed (S7).** Tests: `a_peer_gone_after_our_result_is_unconfirmed_with_the_new_base`, `no_result_within_the_timeout_is_unconfirmed`, `a_result_that_cannot_be_sent_keeps_the_old_base`, `a_peer_gone_during_the_planner_apply_does_not_stop_it` (Task 2).
6. **Nothing is written before both users said yes, and decline or expiry changes nothing.** Tests: `the_planner_applies_only_after_the_starter_applied`, `declining_the_review_changes_nothing_on_both_sides`, `declining_the_confirm_changes_nothing`, `an_unanswered_review_expires_on_both_sides`, `an_unanswered_confirm_expires` (Tasks 2 and 3).
7. **Safety rules of spec section 7.** Path validation of every received path (`a_manifest_with_an_unsafe_path_is_refused`, `a_request_for_a_path_not_offered_is_refused`, `a_final_plan_with_an_unsafe_or_unknown_path_is_refused`, `a_files_zip_with_an_extra_or_missing_entry_is_unsafe`, `a_files_zip_that_fails_the_strict_validator_is_unsafe`, `a_nested_collection_file_is_refused`), SHA-256 and size before apply (`a_damaged_payload_is_refused_before_any_dialog`, `a_file_whose_hash_differs_from_the_plan_is_damaged`), trust never written (`a_sync_writes_only_through_the_collection_port`), Developer mode (`a_synced_collection_file_is_always_merged_never_written_raw`, `keep_both_is_refused_for_settings_files`), the lock (`a_local_edit_before_the_apply_stops_it`, `a_local_edit_during_the_review_stops_the_sync`), the base never read from the peer (`a_final_plan_whose_base_hash_disagrees_is_refused`, `the_stored_base_is_computed_locally_on_both_sides`), pairing removed mid-sync (`removing_the_pairing_mid_sync_ends_it_with_no_change`). All in Task 3 except `the_stored_base_is_computed_locally_on_both_sides` (Task 2).
8. **Spec section 8 failure rows.** Declined, expired, peer left, connection dropped, too big, damaged, unsafe, base mismatch, link missing, folder or uid changed, conflict dialog cancelled, apply fails: one test each (the "Spec 8" list in Task 3).
9. **First send creates both links from the same bytes.** Tests: `both_ends_of_a_linked_send_store_the_same_base_hash`, `an_unconfirmed_send_creates_no_link`, `a_failed_import_creates_no_link` (Task 1).
10. **No peer text reaches the user.** Test: `every_finished_message_is_one_of_the_fixed_texts` (Task 3).
11. **Audit follows S13.** Tests: `a_refusal_before_any_chunk_writes_no_audit`, `finish_names_match_the_wire_names` and the audit assertions in the Task 2 happy path.

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `crates/rocket-app/Cargo.toml` | modify | `zip = "2"` moves to `[dependencies]` |
| `crates/rocket-import/src/transfer_zip.rs` | modify | `pub fn validate_bytes(bytes: &[u8]) -> ImportResult<()>` plus its test |
| `crates/rocket-network/src/link.rs` | modify (Plan 1 file) | `PlanFile`, `SyncPlanPayload.sends`, `SyncPlanPayload.new_base_hash` (S2) plus round-trip tests |
| `crates/rocket-app/src/network/sync_ports.rs` | modify only if Plan 2 lacks it | `collection_root` on `SyncCollectionPort` |
| `crates/rocket-app/src/network/frame_router.rs` | create | `FrameRouter` |
| `crates/rocket-app/src/network/sync.rs` | create | `SyncDeps`, `SyncService` (`new`, `new_with_timing`, `create_link`, `links`, `unlink`, `cancel`, `conflict_files`, `link_first_transfer`, `claims`), `LinkView`, `ConflictFiles`, `Session`, `Phase`, the queue and worker, peer gone, `end_session` (the one place a session ends) |
| `crates/rocket-app/src/network/sync_payload.rs` | create | `kind_name`, `PayloadOut`, `PayloadIn`, `sync_offer`, payload stream and receive with the ack pump, `pack_zip`, `unpack_zip`, `base_from_zip`, `sha_hex` |
| `crates/rocket-app/src/network/sync_starter.rs` | create | `start`, `confirm`, the starter handlers and tasks |
| `crates/rocket-app/src/network/sync_planner.rs` | create | the planner handlers and tasks, `review` |
| `crates/rocket-app/src/network/sync_checks.rs` | create | pure rules: `Stop`, texts, `path_allowed`, `check_request`, `check_final`, `starter_new_base`, `split_actions`, `apply_to_manifest`, `entries_match`, `file_marks`, summaries, `finish_name`, `keep_both_allowed` (with unit tests in the file) |
| `crates/rocket-app/src/network/transfer.rs` | modify | `sync` field, `set_sync`, `has_transfer`, frame destructure |
| `crates/rocket-app/src/network/transfer_send.rs` | modify | `send(.., keep_in_sync)`, link on the offer, link on `Completed` |
| `crates/rocket-app/src/network/transfer_receive.rs` | modify | `OfferFrame` fields, the in-memory copy, link after import |
| `crates/rocket-app/src/network/connections.rs` | modify | `run_socket` arm for the three `Sync*` frames |
| `crates/rocket-app/src/network/mod.rs` | modify | modules, test modules, re-exports |
| `crates/rocket-app/src/lib.rs` | modify | re-export `SyncService`, `SyncDeps`, `LinkView`, `ConflictFiles`, `FrameRouter` |
| `src-tauri/src/commands/network.rs` | modify | `svc.send(&id, &collection, include_environments, false)` (Plan 4 adds the parameter to the command) |
| `crates/rocket-app/src/network/transfer_fakes.rs` | modify (test) | `FakeFiles` and `export_of` become `pub(crate)`; `Payload::offer` sets the two new fields |
| `crates/rocket-app/src/network/sync_fakes.rs` | create (test) | `FakeLinks`, `FakeCollections`, `RecordingSyncEvents`, `SyncRig`, `SyncPair`, the scripted-peer helpers |
| `crates/rocket-app/src/network/sync_tests.rs` | create (test) | Task 1 tests |
| `crates/rocket-app/src/network/sync_run_tests.rs` | create (test) | Task 2 tests |
| `crates/rocket-app/src/network/sync_safety_tests.rs` | create (test) | Task 3 tests |
| `crates/rocket-app/src/network/transfer_link_tests.rs` | modify (test) | `sync_frames_reach_the_sink_and_count_as_pongs` |
| `crates/rocket-app/src/network/transfer_send_tests.rs`, `transfer_receive_tests.rs`, `e2e_transfer_tests.rs` | modify (test) | `send(.., false)` call sites |
| `crates/rocket-app/CLAUDE.md` | modify | rows for `SyncService` and `FrameRouter`, the `TransferService` row gains `keep_in_sync` |

---

### Task 1: Frame router, the sync registry, links, and the link at the first send

**Files:**
- Modify: `crates/rocket-app/Cargo.toml`, `crates/rocket-import/src/transfer_zip.rs`, `crates/rocket-app/src/network/{transfer.rs, transfer_send.rs, transfer_receive.rs, connections.rs, mod.rs, transfer_fakes.rs, transfer_link_tests.rs, transfer_send_tests.rs, transfer_receive_tests.rs, e2e_transfer_tests.rs}`, `crates/rocket-app/src/lib.rs`, `src-tauri/src/commands/network.rs`, and `crates/rocket-app/src/network/sync_ports.rs` only if `collection_root` is missing
- Create: `crates/rocket-app/src/network/{frame_router.rs, sync.rs, sync_payload.rs, sync_checks.rs, sync_fakes.rs, sync_tests.rs}`

**Interfaces:**

```rust
// Consumes (Plan 1, Plan 2, send project)
rocket_network::{LinkId, SyncId, LinkRecord, LinkState, LinkRepository, Manifest, PeerMessage, SyncEvent, SyncPayloadKind,
    SyncAbortReason, SyncOutcome, TransferSink, TransferLimits, TransferId, OutgoingTransfer, IncomingTransfer, DeviceId}
super::sync_ports::{SyncCollectionPort, SyncFile, FileChange}
super::transfer::{PeerSender, Timing, TransferService}
rocket_import::transfer_zip::validate_bytes          // added in this task

// Produces
pub struct SyncDeps { pub network: Arc<dyn PeerSender>, pub links: Arc<dyn LinkRepository>,
    pub collections: Arc<dyn SyncCollectionPort>, pub events: Arc<dyn EventPublisher>,
    pub audit: Arc<dyn SecurityAuditPublisher>, pub staging: Arc<dyn TransferStaging> }
pub struct LinkView { pub link_id: String, pub device_id: String, pub display_name: String, pub short_id: String,
    pub collection_name: String, pub state: LinkState, pub updated_at: String }
pub struct ConflictFiles { pub mine: Option<String>, pub theirs: Option<String> }
impl SyncService {
    pub fn new(deps: SyncDeps) -> Arc<SyncService>;                                  // does not register as the sink
    #[doc(hidden)] pub fn new_with_timing(deps: SyncDeps, timing: Timing) -> Arc<SyncService>;
    pub fn create_link(&self, record: LinkRecord, base_files: Vec<(String, Vec<u8>)>) -> DomainResult<()>;
    pub fn links(&self) -> DomainResult<Vec<LinkView>>;                               // blocks (S15)
    pub async fn unlink(&self, link_id: &LinkId) -> DomainResult<()>;
    pub(crate) fn link_first_transfer(&self, link_id: LinkId, peer: DeviceId, collection: &str, zip: &[u8]) -> DomainResult<()>;  // blocks
    pub(crate) fn claims(&self, from: &DeviceId, message: &PeerMessage) -> bool;
}
impl TransferSink for SyncService
pub struct FrameRouter;  impl FrameRouter { pub fn new(transfer: Arc<TransferService>, sync: Arc<SyncService>) -> Arc<FrameRouter>; }
impl TransferSink for FrameRouter
impl TransferService {
    pub fn set_sync(&self, sync: Arc<SyncService>);
    pub(crate) fn has_transfer(&self, id: &TransferId) -> bool;
    pub async fn send(&self, device: &DeviceId, collection: &str, include_environments: bool, keep_in_sync: bool) -> DomainResult<SendStarted>;
}
// sync_payload.rs
pub(super) fn pack_zip(files: &[SyncFile]) -> DomainResult<Vec<u8>>;
pub(super) fn unpack_zip(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, Stop>;
pub(super) fn base_from_zip(bytes: &[u8]) -> DomainResult<(Manifest, Vec<(String, Vec<u8>)>)>;
pub(super) fn sha_hex(bytes: &[u8]) -> String;
// rocket-import
pub fn validate_bytes(bytes: &[u8]) -> ImportResult<()>;
```

- [ ] **Step 1: Read the reference.** 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Then read `crates/rocket-app/CLAUDE.md` (the `TransferService` row) and `crates/rocket-app/src/network/transfer.rs`.

- [ ] **Step 2: Check the names this plan needs.**

Run:
```bash
grep -n "pub struct LinkId\|pub struct SyncId\|pub struct Manifest\|pub struct LinkRecord\|pub enum LinkState\|pub fn plan_sync\|pub fn resolve\|pub fn new_base\|pub struct SyncPlanPayload\|pub struct SyncPlan\b" crates/rocket-network/src/*.rs
grep -n "SyncOffer\|SyncAbort\|SyncResult\|keep_in_sync\|link_id" crates/rocket-network/src/messages.rs
grep -n "pub enum SyncEvent\|pub enum SyncFinish\|pub enum SyncRole\|pub struct SyncSummary\|pub struct ConflictView" crates/rocket-network/src/events.rs
grep -n "pub trait LinkRepository" crates/rocket-network/src/ports.rs
grep -n "NetworkSyncSent\|NetworkSyncReceived\|NetworkLinkRemoved" crates/rocket-audit/src/*.rs
grep -n "pub trait SyncCollectionPort\|fn collection_root\|pub enum FileChange\|pub struct SyncFile\|pub fn manifest_from_zip" crates/rocket-app/src/network/sync_ports.rs
```
Expected: every name prints at least one line. `fn collection_root` may be missing (Step 3). For any other missing name: stop, write a `Ruling:` line in `.superpowers/sdd/2026-11-08-sync-plan-03/progress.md` and fix it in the owning plan first.

- [ ] **Step 3: `collection_root` on the port (only if Step 2 found none).** Add to `SyncCollectionPort` in `sync_ports.rs`:

```rust
    /// The canonical folder path of the collection, as the link stores it (spec section 5).
    fn collection_root(&self, collection: &str) -> DomainResult<String>;
```
and implement it in the Plan 2 adapter with the repository the adapter already uses for `exists` (canonical path of the collection folder, `NotFound` when the collection is gone). Write `Ruling: S20 collection_root added to SyncCollectionPort by Plan 3.` in the ledger.

Run: `cargo check -j4 -p rocket-app`
Expected: `Finished` with no error.

- [ ] **Step 4: Write the failing test for `validate_bytes`.** Add to the `tests` module of `crates/rocket-import/src/transfer_zip.rs`:

```rust
    #[test]
    fn validate_bytes_runs_the_strict_check_in_memory() {
        let good = zip_with(&[("a.yml", b"x")]);
        assert!(validate_bytes(&good).is_ok());

        let unsafe_name = zip_with(&[("../a.yml", b"x")]);
        assert!(matches!(
            validate_bytes(&unsafe_name),
            Err(ImportError::UnsafeTransferZip(_))
        ));

        let twins = zip_with(&[("A.yml", b"x"), ("a.yml", b"y")]);
        assert!(matches!(
            validate_bytes(&twins),
            Err(ImportError::UnsafeTransferZip(_))
        ));

        let mut trailing = good.clone();
        trailing.push(0);
        assert!(validate_bytes(&trailing).is_err(), "a byte after the end record");
        assert!(validate_bytes(b"not a zip").is_err());
    }
```
`zip_with(&[(&str, &[u8])]) -> Vec<u8>` is the module's existing in-memory ZIP helper; if its name differs, use that helper (grep `fn .*zip` in the tests module).

Run: `cargo test -j4 -p rocket-import validate_bytes_runs_the_strict_check_in_memory`
Expected: FAIL, `cannot find function validate_bytes`.

- [ ] **Step 5: Add `validate_bytes`.** In `transfer_zip.rs`, after `validate`:

```rust
/// The same strict check as `validate`, for a ZIP held in memory (a sync payload).
pub fn validate_bytes(bytes: &[u8]) -> ImportResult<()> {
    validate_archive(std::io::Cursor::new(bytes))
}
```

Run: `cargo test -j4 -p rocket-import transfer_zip`
Expected: PASS, every `transfer_zip` test green.

- [ ] **Step 6: Move `zip` to the dependencies of `rocket-app`.** In `crates/rocket-app/Cargo.toml` remove `zip = "2"` from `[dev-dependencies]` and add `zip = "2"` to `[dependencies]` (keep the sorted order the file uses).

Run: `cargo check -j4 -p rocket-app`
Expected: `Finished`, no error.

- [ ] **Step 7: Write the fakes.** Make two helpers of `transfer_fakes.rs` shared: rename nothing, change `struct FakeFiles` to `pub(crate) struct FakeFiles` and add

```rust
/// An export service over fixed files, for rigs that need a `TransferService`.
pub(crate) fn export_of(files: Vec<(String, Vec<u8>)>) -> Arc<CollectionExportService> {
    Arc::new(CollectionExportService::new(Box::new(FakeFiles {
        files,
        calls: Arc::new(AtomicUsize::new(0)),
    })))
}
```
and make `Payload::offer` build `PeerMessage::CollectionOffer { .., keep_in_sync: false, link_id: None }`.

Create `crates/rocket-app/src/network/sync_fakes.rs`:

```rust
//! Fakes for the sync service: links, collections held in memory, events, rigs and the
//! scripted peer that plays the other node frame by frame.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use rocket_audit::publisher::RecordingSecurityAuditPublisher;
use rocket_audit::AuditEventKind;
use rocket_network::limits::{ACK_EVERY_CHUNKS, CHUNK_BYTES};
use rocket_network::{
    chunk_count_for, DeviceId, LinkId, LinkRecord, LinkRepository, LinkState, Manifest,
    PeerMessage, SyncAbortReason, SyncEvent, SyncFinish, SyncOutcome, SyncPayloadKind,
    TransferSink,
};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{EventPublisher, PublishedEvent};
use sha2::{Digest, Sha256};

use super::frame_router::FrameRouter;
use super::sync::{SyncDeps, SyncService};
use super::sync_payload::pack_zip;
use super::sync_ports::{FileChange, SyncCollectionPort, SyncFile};
use super::test_support::identity;
use super::transfer::{Timing, TransferDeps, TransferService};
pub(crate) use super::transfer_fakes::wait_for;
use super::transfer_fakes::{
    default_files, export_of, FakeImporter, FakePeers, FakeStaging, RecordingTransferEvents,
};

/// The node under test, as its peer knows it.
pub(crate) const BO: u8 = 1;
/// The peer.
pub(crate) const ANA: u8 = 2;
/// A second peer.
pub(crate) const BEN: u8 = 3;
pub(crate) const ORDERS: &str = "orders";
pub(crate) const ROOT: &str = "/ws/collections/orders";
pub(crate) const UID: &str = "uid-orders";

pub(crate) type Files = BTreeMap<String, Vec<u8>>;

pub(crate) fn dev(seed: u8) -> DeviceId {
    identity(seed).device_id
}

pub(crate) fn lid(n: u8) -> LinkId {
    LinkId::parse(&format!("{n:032x}")).expect("a link id")
}

/// A payload transfer id the scripted peer uses: 32 lowercase hex characters.
pub(crate) fn ptid(n: u8) -> String {
    format!("f{n:031x}")
}

pub(crate) fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn files(list: &[(&str, &str)]) -> Files {
    list.iter()
        .map(|(p, b)| (p.to_string(), b.as_bytes().to_vec()))
        .collect()
}

pub(crate) fn manifest_of(files: &Files) -> Manifest {
    let mut m = Manifest::new();
    for (path, bytes) in files {
        m.insert(path, &sha(bytes)).expect("a valid test path");
    }
    m
}

pub(crate) fn zip_of(files: &Files) -> Vec<u8> {
    let list: Vec<SyncFile> = files
        .iter()
        .map(|(path, bytes)| SyncFile {
            path: path.clone(),
            bytes: bytes.clone(),
        })
        .collect();
    pack_zip(&list).expect("a zip")
}

/// Short waits, so the expiry and stall tests run in well under a second each.
pub(crate) fn fast() -> Timing {
    Timing {
        offer_ttl: Duration::from_millis(300),
        idle_limit: Duration::from_millis(300),
        result_timeout: Duration::from_millis(300),
    }
}

pub(crate) fn record(link: &LinkId, peer: u8, base: &Files) -> LinkRecord {
    LinkRecord {
        link_id: link.clone(),
        peer_device_id: dev(peer),
        collection_name: ORDERS.to_string(),
        collection_root: ROOT.to_string(),
        collection_uid: Some(UID.to_string()),
        base: manifest_of(base),
        state: LinkState::InSync,
        updated_at: "2026-11-08T10:00:00+00:00".to_string(),
    }
}

// ----- links -----

#[derive(Default)]
pub(crate) struct FakeLinks {
    records: Mutex<BTreeMap<String, (LinkRecord, Vec<(String, Vec<u8>)>)>>,
    pub fail_save: AtomicBool,
    saves: AtomicUsize,
}

impl FakeLinks {
    pub fn record(&self, id: &LinkId) -> Option<LinkRecord> {
        let map = self.records.lock().expect("links lock");
        map.get(id.as_str()).map(|(r, _)| r.clone())
    }

    pub fn base_files(&self, id: &LinkId) -> Vec<(String, Vec<u8>)> {
        let map = self.records.lock().expect("links lock");
        map.get(id.as_str()).map(|(_, f)| f.clone()).unwrap_or_default()
    }

    pub fn count(&self) -> usize {
        self.records.lock().expect("links lock").len()
    }

    pub fn saves(&self) -> usize {
        self.saves.load(Ordering::SeqCst)
    }
}

impl LinkRepository for FakeLinks {
    fn list(&self) -> DomainResult<Vec<LinkRecord>> {
        let map = self.records.lock().expect("links lock");
        Ok(map.values().map(|(r, _)| r.clone()).collect())
    }

    fn get(&self, id: &LinkId) -> DomainResult<Option<LinkRecord>> {
        Ok(self.record(id))
    }

    fn save(&self, record: &LinkRecord, base_files: &[(String, Vec<u8>)]) -> DomainResult<()> {
        if self.fail_save.load(Ordering::SeqCst) {
            return Err(DomainError::Io("link not saved".into()));
        }
        self.records.lock().expect("links lock").insert(
            record.link_id.as_str().to_string(),
            (record.clone(), base_files.to_vec()),
        );
        self.saves.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn delete(&self, id: &LinkId) -> DomainResult<()> {
        self.records.lock().expect("links lock").remove(id.as_str());
        Ok(())
    }

    fn read_base_file(&self, id: &LinkId, path: &str) -> DomainResult<Option<Vec<u8>>> {
        Ok(self
            .base_files(id)
            .into_iter()
            .find(|(p, _)| p == path)
            .map(|(_, b)| b))
    }
}

// ----- collections -----

pub(crate) struct FakeColl {
    pub uid: Option<String>,
    pub root: String,
    pub files: Files,
}

/// Collections in memory. "Export" is the identity: a file's wire bytes are its bytes. `Put`
/// stands for Plan 2's merge and stores the received bytes; the merge itself is Plan 2's test.
#[derive(Default)]
pub(crate) struct FakeCollections {
    colls: Mutex<HashMap<String, FakeColl>>,
    applied: Mutex<Vec<(String, Vec<FileChange>)>>,
    pub fail_apply: AtomicBool,
    manifests: AtomicUsize,
    edit_after: Mutex<Option<(usize, String, Vec<u8>)>>,
    apply_gate: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}

impl FakeCollections {
    pub fn add(&self, name: &str, files: Files) {
        let root = if name == ORDERS {
            ROOT.to_string()
        } else {
            format!("/ws/collections/{name}")
        };
        self.colls.lock().expect("colls lock").insert(
            name.to_string(),
            FakeColl {
                uid: Some(UID.to_string()),
                root,
                files,
            },
        );
    }

    pub fn set_uid(&self, name: &str, uid: Option<&str>) {
        if let Some(c) = self.colls.lock().expect("colls lock").get_mut(name) {
            c.uid = uid.map(str::to_string);
        }
    }

    pub fn set_root(&self, name: &str, root: &str) {
        if let Some(c) = self.colls.lock().expect("colls lock").get_mut(name) {
            c.root = root.to_string();
        }
    }

    pub fn remove(&self, name: &str) {
        self.colls.lock().expect("colls lock").remove(name);
    }

    pub fn files(&self, name: &str) -> Files {
        let map = self.colls.lock().expect("colls lock");
        map.get(name).map(|c| c.files.clone()).unwrap_or_default()
    }

    pub fn put(&self, name: &str, path: &str, bytes: &[u8]) {
        if let Some(c) = self.colls.lock().expect("colls lock").get_mut(name) {
            c.files.insert(path.to_string(), bytes.to_vec());
        }
    }

    pub fn applied(&self) -> Vec<(String, Vec<FileChange>)> {
        self.applied.lock().expect("applied lock").clone()
    }

    pub fn manifest_calls(&self) -> usize {
        self.manifests.load(Ordering::SeqCst)
    }

    /// The `n + 1`-th `manifest` call and every later read see `path` written: a local save
    /// made while the sync runs.
    pub fn edit_after_manifests(&self, n: usize, path: &str, bytes: &[u8]) {
        *self.edit_after.lock().expect("edit lock") = Some((n, path.to_string(), bytes.to_vec()));
    }

    /// Holds the next `apply` until the returned sender sends or is dropped.
    pub fn hold_apply(&self) -> std::sync::mpsc::Sender<()> {
        let (tx, rx) = std::sync::mpsc::channel();
        *self.apply_gate.lock().expect("gate lock") = Some(rx);
        tx
    }

    fn coll<R>(&self, name: &str, f: impl FnOnce(&FakeColl) -> R) -> DomainResult<R> {
        let map = self.colls.lock().expect("colls lock");
        map.get(name)
            .map(f)
            .ok_or_else(|| DomainError::NotFound("no such collection".into()))
    }
}

impl SyncCollectionPort for FakeCollections {
    fn exists(&self, collection: &str) -> DomainResult<bool> {
        Ok(self.colls.lock().expect("colls lock").contains_key(collection))
    }

    fn collection_uid(&self, collection: &str) -> DomainResult<Option<String>> {
        self.coll(collection, |c| c.uid.clone())
    }

    fn collection_root(&self, collection: &str) -> DomainResult<String> {
        self.coll(collection, |c| c.root.clone())
    }

    fn manifest(&self, collection: &str) -> DomainResult<Manifest> {
        let n = self.manifests.fetch_add(1, Ordering::SeqCst);
        let edit = {
            let mut slot = self.edit_after.lock().expect("edit lock");
            match slot.as_ref() {
                Some((after, _, _)) if n >= *after => slot.take(),
                _ => None,
            }
        };
        if let Some((_, path, bytes)) = edit {
            self.put(collection, &path, &bytes);
        }
        self.coll(collection, |c| manifest_of(&c.files))
    }

    fn export_files(&self, collection: &str, paths: &[String]) -> DomainResult<Vec<SyncFile>> {
        let all = self.coll(collection, |c| c.files.clone())?;
        paths
            .iter()
            .map(|p| {
                all.get(p)
                    .map(|b| SyncFile {
                        path: p.clone(),
                        bytes: b.clone(),
                    })
                    .ok_or_else(|| DomainError::NotFound("no such file".into()))
            })
            .collect()
    }

    fn apply(&self, collection: &str, changes: &[FileChange]) -> DomainResult<()> {
        let gate = self.apply_gate.lock().expect("gate lock").take();
        if let Some(gate) = gate {
            // Runs in a blocking task, so waiting here does not stall the test runtime.
            let _ = gate.recv();
        }
        self.applied
            .lock()
            .expect("applied lock")
            .push((collection.to_string(), changes.to_vec()));
        if self.fail_apply.load(Ordering::SeqCst) {
            return Err(DomainError::Io("apply failed".into()));
        }
        let mut copy = self.coll(collection, |c| c.files.clone())?;
        for change in changes {
            match change {
                FileChange::Put { path, bytes } | FileChange::PutAs { path, bytes } => {
                    copy.insert(path.clone(), bytes.clone());
                }
                FileChange::Delete { path } => {
                    copy.remove(path)
                        .ok_or_else(|| DomainError::NotFound("no such file".into()))?;
                }
                FileChange::Rename { from, to } => {
                    let bytes = copy
                        .remove(from)
                        .ok_or_else(|| DomainError::NotFound("no such file".into()))?;
                    copy.insert(to.clone(), bytes);
                }
            }
        }
        if let Some(c) = self.colls.lock().expect("colls lock").get_mut(collection) {
            c.files = copy;
        }
        Ok(())
    }
}

// ----- events -----

#[derive(Default)]
pub(crate) struct RecordingSyncEvents(Mutex<Vec<SyncEvent>>);

impl RecordingSyncEvents {
    pub fn all(&self) -> Vec<SyncEvent> {
        self.0.lock().expect("events lock").clone()
    }

    pub fn reviews(&self) -> Vec<SyncEvent> {
        self.all()
            .into_iter()
            .filter(|e| matches!(e, SyncEvent::ReviewReady { .. }))
            .collect()
    }

    pub fn confirms(&self) -> Vec<SyncEvent> {
        self.all()
            .into_iter()
            .filter(|e| matches!(e, SyncEvent::ConfirmNeeded { .. }))
            .collect()
    }

    /// `(sync_id, outcome, message)` of every `Finished`.
    pub fn finished(&self) -> Vec<(String, SyncFinish, Option<String>)> {
        self.all()
            .into_iter()
            .filter_map(|e| match e {
                SyncEvent::Finished {
                    sync_id,
                    outcome,
                    message,
                    ..
                } => Some((sync_id, outcome, message)),
                _ => None,
            })
            .collect()
    }

    pub fn link_changes(&self) -> usize {
        self.all()
            .iter()
            .filter(|e| matches!(e, SyncEvent::LinkChanged { .. }))
            .count()
    }
}

impl EventPublisher for RecordingSyncEvents {
    fn publish(&self, event: &dyn PublishedEvent) {
        if let Some(event) = event.downcast_ref::<SyncEvent>() {
            self.0.lock().expect("events lock").push(event.clone());
        }
    }
}

// ----- the rig -----

pub(crate) struct SyncRig {
    pub sync: Arc<SyncService>,
    pub transfer: Arc<TransferService>,
    pub router: Arc<FrameRouter>,
    pub peers: Arc<FakePeers>,
    pub events: Arc<RecordingSyncEvents>,
    pub transfer_events: Arc<RecordingTransferEvents>,
    pub audit: Arc<RecordingSecurityAuditPublisher>,
    pub links: Arc<FakeLinks>,
    pub colls: Arc<FakeCollections>,
    pub importer: Arc<FakeImporter>,
    pub staging: Arc<FakeStaging>,
}

pub(crate) fn sync_rig() -> SyncRig {
    sync_rig_timed(fast())
}

/// A transfer service and a sync service over the same fake peers, wired as Plan 4 wires
/// them: the transfer service knows the sync service and the router is the sink.
pub(crate) fn sync_rig_timed(timing: Timing) -> SyncRig {
    let peers = Arc::new(FakePeers::default());
    let events = Arc::new(RecordingSyncEvents::default());
    let transfer_events = Arc::new(RecordingTransferEvents::default());
    let audit = Arc::new(RecordingSecurityAuditPublisher::new());
    let links = Arc::new(FakeLinks::default());
    let colls = Arc::new(FakeCollections::default());
    let importer = Arc::new(FakeImporter::default());
    let staging = Arc::new(FakeStaging::default());
    let transfer = TransferService::new_with_timing(
        TransferDeps {
            network: peers.clone(),
            export: export_of(default_files()),
            importer: importer.clone(),
            staging: staging.clone(),
            events: transfer_events.clone(),
            audit: audit.clone(),
        },
        timing,
    );
    let sync = SyncService::new_with_timing(
        SyncDeps {
            network: peers.clone(),
            links: links.clone(),
            collections: colls.clone(),
            events: events.clone(),
            audit: audit.clone(),
            staging: staging.clone(),
        },
        timing,
    );
    transfer.set_sync(sync.clone());
    let router = FrameRouter::new(transfer.clone(), sync.clone());
    peers.set_transfer_sink(router.clone());
    SyncRig {
        sync,
        transfer,
        router,
        peers,
        events,
        transfer_events,
        audit,
        links,
        colls,
        importer,
        staging,
    }
}

impl SyncRig {
    pub fn audited(&self) -> Vec<AuditEventKind> {
        self.audit.records().into_iter().map(|r| r.kind).collect()
    }

    /// A frame from `seed` arrives at this node.
    pub fn push(&self, seed: u8, message: PeerMessage) {
        self.router.on_frame(dev(seed), message);
    }

    pub fn sent(&self, seed: u8) -> Vec<PeerMessage> {
        self.peers.sent_to(&dev(seed))
    }

    pub fn aborts(&self, seed: u8) -> Vec<SyncAbortReason> {
        self.sent(seed)
            .into_iter()
            .filter_map(|m| match m {
                PeerMessage::SyncAbort { reason, .. } => Some(reason),
                _ => None,
            })
            .collect()
    }

    pub fn results(&self, seed: u8) -> Vec<SyncOutcome> {
        self.sent(seed)
            .into_iter()
            .filter_map(|m| match m {
                PeerMessage::SyncResult { outcome, .. } => Some(outcome),
                _ => None,
            })
            .collect()
    }

    /// Ana is paired and online, `orders` holds `mine`, and link `link` with Ana has base `base`.
    pub fn linked(&self, link: &LinkId, base: &Files, mine: &Files) {
        self.peers.add(dev(ANA), "Ana");
        self.colls.add(ORDERS, mine.clone());
        let base_files: Vec<(String, Vec<u8>)> =
            base.iter().map(|(p, b)| (p.clone(), b.clone())).collect();
        self.links
            .save(&record(link, ANA, base), &base_files)
            .expect("link saved");
    }
}

// ----- the scripted peer -----

/// The fields of one `SyncOffer` frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Offered {
    pub sync_id: String,
    pub link_id: String,
    pub base_hash: String,
    pub kind: SyncPayloadKind,
    pub transfer_id: String,
    pub size_bytes: u64,
    pub chunk_count: u32,
    pub sha256: String,
}

pub(crate) fn as_offer(m: &PeerMessage) -> Option<Offered> {
    match m {
        PeerMessage::SyncOffer {
            sync_id,
            link_id,
            base_hash,
            kind,
            transfer_id,
            size_bytes,
            chunk_count,
            sha256,
        } => Some(Offered {
            sync_id: sync_id.clone(),
            link_id: link_id.clone(),
            base_hash: base_hash.clone(),
            kind: *kind,
            transfer_id: transfer_id.clone(),
            size_bytes: *size_bytes,
            chunk_count: *chunk_count,
            sha256: sha256.clone(),
        }),
        _ => None,
    }
}

pub(crate) fn offer_frame(o: &Offered) -> PeerMessage {
    PeerMessage::SyncOffer {
        sync_id: o.sync_id.clone(),
        link_id: o.link_id.clone(),
        base_hash: o.base_hash.clone(),
        kind: o.kind,
        transfer_id: o.transfer_id.clone(),
        size_bytes: o.size_bytes,
        chunk_count: o.chunk_count,
        sha256: o.sha256.clone(),
    }
}

/// The offer the scripted peer makes for `bytes`.
pub(crate) fn offered(
    sync_id: &str,
    link: &LinkId,
    base_hash: &str,
    kind: SyncPayloadKind,
    transfer_id: &str,
    bytes: &[u8],
) -> Offered {
    Offered {
        sync_id: sync_id.to_string(),
        link_id: link.as_str().to_string(),
        base_hash: base_hash.to_string(),
        kind,
        transfer_id: transfer_id.to_string(),
        size_bytes: bytes.len() as u64,
        chunk_count: chunk_count_for(bytes.len() as u64),
        sha256: sha(bytes),
    }
}

pub(crate) fn chunk_frames(transfer_id: &str, bytes: &[u8]) -> Vec<PeerMessage> {
    bytes
        .chunks(CHUNK_BYTES)
        .enumerate()
        .map(|(seq, part)| PeerMessage::Chunk {
            transfer_id: transfer_id.to_string(),
            seq: seq as u32,
            data: STANDARD.encode(part),
        })
        .collect()
}

/// The `n`-th (from 0) `SyncOffer` of `kind` this node sent to `seed`.
pub(crate) async fn nth_offer(rig: &SyncRig, seed: u8, kind: SyncPayloadKind, n: usize) -> Offered {
    let mut found = None;
    wait_for("a sync offer", || {
        found = rig
            .sent(seed)
            .iter()
            .filter_map(as_offer)
            .filter(|o| o.kind == kind)
            .nth(n);
        found.is_some()
    })
    .await;
    found.expect("an offer")
}

/// Plays the receiving peer of one payload this node offers: answers, takes every chunk,
/// acks as the real receiver does and returns the bytes.
pub(crate) async fn take_payload(
    rig: &SyncRig,
    seed: u8,
    kind: SyncPayloadKind,
    n: usize,
) -> (Offered, Vec<u8>) {
    let offer = nth_offer(rig, seed, kind, n).await;
    rig.push(
        seed,
        PeerMessage::OfferAnswer {
            transfer_id: offer.transfer_id.clone(),
            accept: true,
        },
    );
    let mut acked = 0u32;
    for _ in 0..2000 {
        let chunks: Vec<String> = rig
            .sent(seed)
            .into_iter()
            .filter_map(|m| match m {
                PeerMessage::Chunk {
                    transfer_id, data, ..
                } if transfer_id == offer.transfer_id => Some(data),
                _ => None,
            })
            .collect();
        let got = chunks.len() as u32;
        let ack_to = if got == offer.chunk_count {
            got
        } else {
            got - got % ACK_EVERY_CHUNKS
        };
        if ack_to > acked {
            rig.push(
                seed,
                PeerMessage::ChunkAck {
                    transfer_id: offer.transfer_id.clone(),
                    next_seq: ack_to,
                },
            );
            acked = ack_to;
        }
        if got == offer.chunk_count {
            let mut bytes = Vec::new();
            for data in chunks {
                bytes.extend(STANDARD.decode(data).expect("base64"));
            }
            return (offer, bytes);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("the payload did not arrive");
}

/// Plays the sending peer of one payload: offers, waits for the answer, sends every chunk.
pub(crate) async fn give_payload(rig: &SyncRig, seed: u8, offer: &Offered, bytes: &[u8]) {
    rig.push(seed, offer_frame(offer));
    wait_for("the answer to the payload", || {
        rig.sent(seed).iter().any(|m| {
            matches!(m, PeerMessage::OfferAnswer { transfer_id, accept: true }
                if *transfer_id == offer.transfer_id)
        })
    })
    .await;
    for chunk in chunk_frames(&offer.transfer_id, bytes) {
        rig.push(seed, chunk);
    }
}

// ----- two nodes -----

/// Two rigs joined by a relay: Bo (`a`) and Ana (`b`). Frames `a` sends to Ana reach `b`
/// from Bo, and back. `drop` decides which frames from `a` to `b` are lost.
pub(crate) struct SyncPair {
    pub a: SyncRig,
    pub b: SyncRig,
    a_seen: usize,
    b_seen: usize,
}

impl SyncPair {
    pub fn new() -> SyncPair {
        let a = sync_rig();
        let b = sync_rig();
        a.peers.add(dev(ANA), "Ana");
        b.peers.add(dev(BO), "Bo");
        SyncPair {
            a,
            b,
            a_seen: 0,
            b_seen: 0,
        }
    }

    /// Both hold `orders`, linked by `link` with the same base; Bo has `bo`, Ana has `ana`.
    pub fn linked(link: &LinkId, base: &Files, bo: &Files, ana: &Files) -> SyncPair {
        let pair = SyncPair::new();
        let base_files: Vec<(String, Vec<u8>)> =
            base.iter().map(|(p, b)| (p.clone(), b.clone())).collect();
        pair.a.colls.add(ORDERS, bo.clone());
        pair.b.colls.add(ORDERS, ana.clone());
        pair.a
            .links
            .save(&record(link, ANA, base), &base_files)
            .expect("a link");
        pair.b
            .links
            .save(&record(link, BO, base), &base_files)
            .expect("b link");
        pair
    }

    pub fn pump(&mut self, drop: &dyn Fn(&PeerMessage) -> bool) -> usize {
        let from_a = self.a.sent(ANA);
        let from_b = self.b.sent(BO);
        let mut moved = 0;
        for frame in from_a.iter().skip(self.a_seen) {
            if !drop(frame) {
                self.b.push(BO, frame.clone());
            }
            moved += 1;
        }
        for frame in from_b.iter().skip(self.b_seen) {
            self.a.push(ANA, frame.clone());
            moved += 1;
        }
        self.a_seen = from_a.len();
        self.b_seen = from_b.len();
        moved
    }

    /// Pumps on the real clock until `done` holds. Fails the test after about 5 seconds.
    pub async fn run_until(&mut self, done: impl Fn(&SyncPair) -> bool) {
        for _ in 0..500 {
            self.pump(&|_| false);
            if done(self) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the linked sync did not reach the expected state");
    }
}
```

- [ ] **Step 8: Write the failing Task 1 tests.** Create `crates/rocket-app/src/network/sync_tests.rs`:

```rust
//! Task 1 tests: the router, links, unlink, removal with the pairing, and the link at the
//! first linked send.

use std::io::Write;
use std::sync::Arc;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use rocket_audit::AuditEventKind;
use rocket_network::{
    LinkId, LinkState, PeerMessage, ResultOutcome, SyncAbortReason, SyncId,
    SyncPayloadKind, TransferEvent, TransferSink,
};
use rocket_shared::error::DomainError;

use super::sync_fakes::*;
use super::sync_payload::base_from_zip;
use super::sync_ports::manifest_from_zip;

fn base() -> Files {
    files(&[("opencollection.yml", "name: orders\n"), ("a.yml", "url: a\n")])
}

/// A ZIP laid out as the Bruno export lays it out: one root folder.
fn rooted_zip(root: &str, files: &Files) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default();
        for (path, bytes) in files {
            zip.start_file(format!("{root}/{path}"), opts).expect("entry");
            zip.write_all(bytes).expect("bytes");
        }
        zip.finish().expect("zip");
    }
    out.into_inner()
}

fn collection_offer(id: &str, bytes: &[u8], link: Option<&LinkId>) -> PeerMessage {
    PeerMessage::CollectionOffer {
        transfer_id: id.to_string(),
        name: ORDERS.to_string(),
        size_bytes: bytes.len() as u64,
        chunk_count: rocket_network::chunk_count_for(bytes.len() as u64),
        sha256: sha(bytes),
        keep_in_sync: link.is_some(),
        link_id: link.map(|l| l.as_str().to_string()),
    }
}

// ----- the router -----

#[tokio::test]
async fn the_router_sends_sync_frames_to_sync_and_others_to_transfer() {
    let rig = sync_rig();
    rig.peers.add(dev(ANA), "Ana");
    let zip = rooted_zip(ORDERS, &base());

    rig.push(ANA, collection_offer(&ptid(1), &zip, None));
    rig.push(
        ANA,
        PeerMessage::SyncAbort {
            sync_id: SyncId::random().as_str().to_string(),
            reason: SyncAbortReason::Cancelled,
        },
    );
    wait_for("the incoming offer", || {
        rig.transfer_events
            .all()
            .iter()
            .any(|e| matches!(e, TransferEvent::OfferIncoming { .. }))
    })
    .await;
    wait_for("the sync frame", || rig.sync.frames_seen() == 1).await;
}

#[tokio::test]
async fn frames_of_a_sync_transfer_id_go_to_sync_only_from_its_peer() {
    let rig = sync_rig();
    rig.peers.add(dev(ANA), "Ana");
    rig.peers.add(dev(BEN), "Ben");
    let id = rig.sync.claim_transfer_for_test(dev(ANA));
    let ack = PeerMessage::ChunkAck {
        transfer_id: id.clone(),
        next_seq: 1,
    };

    assert!(rig.sync.claims(&dev(ANA), &ack), "its own peer");
    assert!(!rig.sync.claims(&dev(BEN), &ack), "another peer");
    assert!(
        !rig.sync.claims(&dev(ANA), &collection_offer(&id, b"x", None)),
        "a collection offer is never a sync frame"
    );
    assert!(rig.sync.claims(
        &dev(BEN),
        &PeerMessage::SyncResult {
            sync_id: SyncId::random().as_str().to_string(),
            outcome: rocket_network::SyncOutcome::Applied,
        }
    ));
}

#[tokio::test]
async fn a_sync_offer_reusing_a_transfer_id_is_dropped() {
    let rig = sync_rig();
    rig.linked(&lid(1), &base(), &base());
    let zip = rooted_zip(ORDERS, &base());
    rig.push(ANA, collection_offer(&ptid(1), &zip, None));
    wait_for("the incoming offer", || {
        !rig.transfer_events.all().is_empty()
    })
    .await;
    let manifest = manifest_of(&base()).to_json().expect("json");
    let offer = offered(
        SyncId::random().as_str(),
        &lid(1),
        &manifest_of(&base()).base_hash(),
        SyncPayloadKind::Manifest,
        &ptid(1),
        &manifest,
    );

    rig.push(ANA, offer_frame(&offer));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    assert_eq!(rig.sync.frames_seen(), 0, "the router dropped it");
    assert!(rig.sent(ANA).is_empty(), "no answer of any kind");
}

#[tokio::test]
async fn peer_gone_reaches_both_services() {
    let rig = sync_rig();
    rig.peers.add(dev(ANA), "Ana");
    let zip = rooted_zip(ORDERS, &base());
    rig.push(ANA, collection_offer(&ptid(1), &zip, None));
    wait_for("the incoming offer", || !rig.transfer_events.all().is_empty()).await;

    rig.router.on_peer_gone(dev(ANA));

    wait_for("the pending offer ended", || {
        rig.transfer_events
            .all()
            .iter()
            .any(|e| matches!(e, TransferEvent::Finished { .. }))
    })
    .await;
    wait_for("the sync service saw it", || rig.sync.gone_seen() == 1).await;
}

#[tokio::test]
async fn sync_service_new_does_not_take_the_sink() {
    let rig = sync_rig();
    let sink = rig.peers.sink().expect("a sink");
    let sink_ptr = Arc::as_ptr(&sink) as *const ();
    let router_ptr = Arc::as_ptr(&rig.router) as *const ();
    assert_eq!(sink_ptr, router_ptr, "the router stays the sink");
}

// ----- links -----

#[tokio::test]
async fn create_link_stores_the_record_and_announces_it() {
    let rig = sync_rig();
    rig.peers.add(dev(ANA), "Ana");
    let files: Vec<(String, Vec<u8>)> = base().into_iter().collect();

    rig.sync
        .create_link(record(&lid(1), ANA, &base()), files.clone())
        .expect("created");

    assert_eq!(rig.links.record(&lid(1)), Some(record(&lid(1), ANA, &base())));
    assert_eq!(rig.links.base_files(&lid(1)), files);
    assert_eq!(rig.events.link_changes(), 1);
}

#[tokio::test]
async fn create_link_refuses_an_existing_link_id() {
    let rig = sync_rig();
    rig.linked(&lid(1), &base(), &base());

    let err = rig
        .sync
        .create_link(record(&lid(1), ANA, &base()), Vec::new())
        .expect_err("exists");

    assert!(matches!(err, DomainError::AlreadyExists(_)));
    assert_eq!(rig.links.saves(), 1, "only the first save");
}

#[tokio::test]
async fn links_show_the_peer_label_and_derived_state() {
    let rig = sync_rig();
    rig.linked(&lid(1), &base(), &base());

    let views = rig.sync.links().expect("links");
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].link_id, lid(1).as_str());
    assert_eq!(views[0].device_id, dev(ANA).to_base32());
    assert_eq!(views[0].display_name, "Ana");
    assert_eq!(views[0].short_id, dev(ANA).short());
    assert_eq!(views[0].collection_name, ORDERS);
    assert_eq!(views[0].state, LinkState::InSync);

    rig.colls.put(ORDERS, "a.yml", b"url: changed\n");
    assert_eq!(rig.sync.links().expect("links")[0].state, LinkState::ChangesWaiting);

    rig.colls.set_uid(ORDERS, Some("another"));
    assert_eq!(rig.sync.links().expect("links")[0].state, LinkState::Broken);
    rig.colls.set_uid(ORDERS, Some(UID));
    rig.colls.set_root(ORDERS, "/elsewhere/orders");
    assert_eq!(rig.sync.links().expect("links")[0].state, LinkState::Broken);
    rig.colls.remove(ORDERS);
    assert_eq!(rig.sync.links().expect("links")[0].state, LinkState::Broken);
    assert_eq!(
        rig.links.record(&lid(1)).expect("kept").state,
        LinkState::InSync,
        "links() derives, it does not save"
    );

    rig.peers.forget(&dev(ANA));
    let view = &rig.sync.links().expect("links")[0];
    assert_eq!(view.display_name, "Unknown node");
    assert_eq!(view.short_id, dev(ANA).short());
}

#[tokio::test]
async fn unlink_deletes_the_link_and_audits() {
    let rig = sync_rig();
    rig.linked(&lid(1), &base(), &base());

    rig.sync.unlink(&lid(1)).await.expect("unlinked");

    assert_eq!(rig.links.count(), 0);
    assert_eq!(rig.events.link_changes(), 1);
    assert_eq!(
        rig.audited(),
        vec![AuditEventKind::NetworkLinkRemoved {
            device_id: dev(ANA).short(),
            collection: ORDERS.to_string(),
        }]
    );
    assert!(rig.sent(ANA).is_empty(), "the peer learns at its next sync");
    assert_eq!(rig.colls.files(ORDERS), base(), "the collection is untouched");
}

#[tokio::test]
async fn unlinking_an_unknown_link_is_not_found() {
    let rig = sync_rig();
    let err = rig.sync.unlink(&lid(9)).await.expect_err("unknown");
    assert!(matches!(&err, DomainError::NotFound(m) if m == "That link does not exist."));
}

#[tokio::test]
async fn removing_the_pairing_deletes_the_peers_links() {
    let rig = sync_rig();
    rig.linked(&lid(1), &base(), &base());
    rig.peers.forget(&dev(ANA));

    rig.router.on_peer_gone(dev(ANA));

    wait_for("the link is gone", || rig.links.count() == 0).await;
    wait_for("the audit row", || {
        rig.audited().contains(&AuditEventKind::NetworkLinkRemoved {
            device_id: dev(ANA).short(),
            collection: ORDERS.to_string(),
        })
    })
    .await;
}

#[tokio::test]
async fn a_dropped_connection_keeps_the_links() {
    let rig = sync_rig();
    rig.linked(&lid(1), &base(), &base());

    rig.router.on_peer_gone(dev(ANA));
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    assert_eq!(rig.links.count(), 1);
    assert!(rig.audited().is_empty());
}

// ----- the first linked send -----

/// Collects the chunks of the one collection transfer this node sent to Ana.
fn sent_zip(rig: &SyncRig) -> (String, Vec<u8>, u32) {
    let mut id = String::new();
    let mut bytes = Vec::new();
    let mut count = 0;
    for m in rig.sent(ANA) {
        match m {
            PeerMessage::CollectionOffer { transfer_id, .. } => id = transfer_id,
            PeerMessage::Chunk { data, .. } => {
                bytes.extend(STANDARD.decode(data).expect("base64"));
                count += 1;
            }
            _ => {}
        }
    }
    (id, bytes, count)
}

async fn linked_send_until_last_ack(rig: &SyncRig) -> (String, Vec<u8>) {
    rig.peers.add(dev(ANA), "Ana");
    rig.colls.add(ORDERS, base());
    let started = rig
        .transfer
        .send(&dev(ANA), ORDERS, false, true)
        .await
        .expect("offered");
    rig.push(
        ANA,
        PeerMessage::OfferAnswer {
            transfer_id: started.transfer_id.clone(),
            accept: true,
        },
    );
    let mut sent = (String::new(), Vec::new(), 0);
    wait_for("every chunk", || {
        sent = sent_zip(rig);
        sent.2 == rocket_network::chunk_count_for(sent.1.len() as u64) && sent.2 > 0
    })
    .await;
    rig.push(
        ANA,
        PeerMessage::ChunkAck {
            transfer_id: started.transfer_id.clone(),
            next_seq: sent.2,
        },
    );
    (started.transfer_id, sent.1)
}

#[tokio::test]
async fn a_keep_in_sync_send_puts_the_link_on_the_offer() {
    let rig = sync_rig();
    rig.peers.add(dev(ANA), "Ana");
    rig.colls.add(ORDERS, base());

    rig.transfer
        .send(&dev(ANA), ORDERS, false, true)
        .await
        .expect("offered");

    let offer = rig
        .sent(ANA)
        .into_iter()
        .find(|m| matches!(m, PeerMessage::CollectionOffer { .. }))
        .expect("an offer");
    match offer {
        PeerMessage::CollectionOffer {
            keep_in_sync,
            link_id,
            ..
        } => {
            assert!(keep_in_sync);
            LinkId::parse(&link_id.expect("a link id")).expect("a valid link id");
        }
        _ => unreachable!("filtered above"),
    }
    assert_eq!(rig.links.count(), 0, "no link before the result");
}

#[tokio::test]
async fn a_completed_keep_in_sync_send_creates_the_sender_link() {
    let rig = sync_rig();
    let (id, zip) = linked_send_until_last_ack(&rig).await;

    rig.push(
        ANA,
        PeerMessage::TransferResult {
            transfer_id: id,
            outcome: ResultOutcome::Completed,
        },
    );

    wait_for("the sender link", || rig.links.count() == 1).await;
    let (expected, expected_files) = base_from_zip(&zip).expect("a base");
    let link = rig.links.list_for_test().remove(0);
    assert_eq!(link.base, expected);
    assert_eq!(link.peer_device_id, dev(ANA));
    assert_eq!(link.collection_name, ORDERS);
    assert_eq!(link.collection_root, ROOT);
    assert_eq!(link.collection_uid.as_deref(), Some(UID));
    assert_eq!(link.state, LinkState::InSync);
    assert_eq!(rig.links.base_files(&link.link_id), expected_files);
    assert_eq!(rig.events.link_changes(), 1);
}

#[tokio::test]
async fn an_unconfirmed_send_creates_no_link() {
    let rig = sync_rig();
    let _ = linked_send_until_last_ack(&rig).await;

    rig.router.on_peer_gone(dev(ANA));

    wait_for("the send ended", || {
        rig.transfer_events
            .all()
            .iter()
            .any(|e| matches!(e, TransferEvent::Finished { .. }))
    })
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(rig.links.count(), 0);
}

#[tokio::test]
async fn keep_in_sync_with_environments_is_refused() {
    let rig = sync_rig();
    rig.peers.add(dev(ANA), "Ana");

    let err = rig
        .transfer
        .send(&dev(ANA), ORDERS, true, true)
        .await
        .expect_err("refused");

    assert!(matches!(&err, DomainError::InvalidInput(m)
        if m == "Keep in sync does not send environments. Turn one of them off."));
    assert!(rig.sent(ANA).is_empty());
}

#[tokio::test]
async fn keep_in_sync_without_a_sync_service_is_refused() {
    let rig = super::transfer_fakes::rig();
    rig.peers.add(dev(ANA), "Ana");

    let err = rig
        .svc
        .send(&dev(ANA), "orders", false, true)
        .await
        .expect_err("refused");

    assert!(matches!(&err, DomainError::InvalidInput(m) if m == "Keep in sync is not available."));
}

async fn receive_linked(rig: &SyncRig, link: Option<&LinkId>) {
    rig.peers.add(dev(ANA), "Ana");
    rig.colls.add(ORDERS, base());
    let zip = rooted_zip(ORDERS, &base());
    rig.push(ANA, collection_offer(&ptid(1), &zip, link));
    wait_for("the incoming offer", || !rig.transfer_events.all().is_empty()).await;
    rig.transfer.respond(&ptid(1), true).await.expect("accepted");
    for chunk in chunk_frames(&ptid(1), &zip) {
        rig.push(ANA, chunk);
    }
}

#[tokio::test]
async fn a_keep_in_sync_receive_creates_the_receiver_link_after_import() {
    let rig = sync_rig();
    receive_linked(&rig, Some(&lid(4))).await;

    wait_for("the receiver link", || rig.links.count() == 1).await;
    let link = rig.links.record(&lid(4)).expect("the offered link id");
    assert_eq!(link.base, manifest_of(&base()), "the root folder is stripped");
    assert_eq!(link.peer_device_id, dev(ANA));
    assert_eq!(link.collection_name, ORDERS, "the imported name");
    assert_eq!(link.state, LinkState::InSync);
}

#[tokio::test]
async fn a_plain_receive_creates_no_link() {
    let rig = sync_rig();
    receive_linked(&rig, None).await;
    wait_for("the import", || !rig.importer.calls().is_empty()).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(rig.links.count(), 0);
}

#[tokio::test]
async fn a_failed_import_creates_no_link() {
    let rig = sync_rig();
    rig.importer
        .set_mode(super::transfer_fakes::ImportMode::Fail);
    receive_linked(&rig, Some(&lid(4))).await;
    wait_for("the import", || !rig.importer.calls().is_empty()).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(rig.links.count(), 0);
}

#[tokio::test]
async fn a_keep_in_sync_offer_with_a_bad_link_id_is_refused() {
    let rig = sync_rig();
    rig.peers.add(dev(ANA), "Ana");
    let zip = rooted_zip(ORDERS, &base());
    let mut offer = collection_offer(&ptid(1), &zip, Some(&lid(4)));
    if let PeerMessage::CollectionOffer { link_id, .. } = &mut offer {
        *link_id = Some("not-hex".into());
    }

    rig.push(ANA, offer);

    wait_for("the refusal", || {
        rig.sent(ANA).iter().any(|m| matches!(m,
            PeerMessage::TransferAbort { reason: rocket_network::AbortReason::Failed, .. }))
    })
    .await;
    assert!(rig.transfer_events.all().is_empty(), "never shown");
}

#[tokio::test]
async fn both_ends_of_a_linked_send_store_the_same_base_hash() {
    let mut pair = SyncPair::new();
    pair.a.colls.add(ORDERS, base());
    pair.b.colls.add(ORDERS, base());
    let started = pair
        .a
        .transfer
        .send(&dev(ANA), ORDERS, false, true)
        .await
        .expect("offered");
    pair.run_until(|p| !p.b.transfer_events.all().is_empty()).await;
    pair.b
        .transfer
        .respond(&started.transfer_id, true)
        .await
        .expect("accepted");

    pair.run_until(|p| p.a.links.count() == 1 && p.b.links.count() == 1)
        .await;

    let a = pair.a.links.list_for_test().remove(0);
    let b = pair.b.links.list_for_test().remove(0);
    assert_eq!(a.link_id, b.link_id);
    assert_eq!(a.base_hash(), b.base_hash());
    assert_eq!(a.peer_device_id, dev(ANA));
    assert_eq!(b.peer_device_id, dev(BO));
}

#[test]
fn base_from_zip_matches_manifest_from_zip() {
    let zip = rooted_zip(ORDERS, &base());
    let dir = tempfile::tempdir().expect("a temp dir");
    let path = dir.path().join("sent.zip");
    std::fs::write(&path, &zip).expect("written");

    let (ours, files) = base_from_zip(&zip).expect("ours");
    let theirs = manifest_from_zip(&path).expect("Plan 2's");

    assert_eq!(ours, theirs);
    assert_eq!(ours, manifest_of(&base()));
    assert_eq!(files, base().into_iter().collect::<Vec<_>>());
}

#[test]
fn base_from_zip_refuses_an_unsafe_zip() {
    let zip = rooted_zip("../x", &base());
    assert!(base_from_zip(&zip).is_err());
}

#[test]
fn pack_and_unpack_round_trip_in_path_order() {
    let files = base();
    let zip = zip_of(&files);
    assert_eq!(super::sync_payload::unpack_zip(&zip).ok(), Some(files.clone()));
    assert_eq!(zip_of(&files), zip, "the same files give the same bytes");
}
```
Add `pub fn list_for_test(&self) -> Vec<LinkRecord>` to `FakeLinks` (returns `self.list().expect("list")`).

Register the new test modules in `mod.rs` (Step 13 adds the rest):

```rust
#[cfg(test)]
#[allow(dead_code, clippy::type_complexity)]
pub(crate) mod sync_fakes;
#[cfg(test)]
#[path = "sync_tests.rs"]
mod sync_tests;
```

Run: `cargo test -j4 -p rocket-app sync_tests`
Expected: FAIL to compile (`SyncService`, `FrameRouter`, `sync_payload` and the new `send` parameter are missing).

- [ ] **Step 9: `run_socket` and the frame destructure.** In `connections.rs`, the arm that hands transfer frames to the sink becomes (only add the three variants if Plan 1 has not):

```rust
                    // The transfer and sync services own these frames. Any of them shows the
                    // peer is alive, so a long stream cannot starve the ping rule.
                    Some(
                        message @ (PeerMessage::CollectionOffer { .. }
                        | PeerMessage::OfferAnswer { .. }
                        | PeerMessage::Chunk { .. }
                        | PeerMessage::TransferAbort { .. }
                        | PeerMessage::ChunkAck { .. }
                        | PeerMessage::TransferResult { .. }
                        | PeerMessage::SyncOffer { .. }
                        | PeerMessage::SyncAbort { .. }
                        | PeerMessage::SyncResult { .. }),
                    ) => {
```
`needs_block_check` is unchanged: the three `Sync*` frames keep the block-list read. In `transfer.rs`, `handle_frame` destructures `PeerMessage::CollectionOffer { transfer_id, name, size_bytes, chunk_count, sha256, keep_in_sync, link_id }` and passes the two new fields into `OfferFrame`.

Add to `transfer_link_tests.rs`:

```rust
fn sync_frames() -> Vec<PeerMessage> {
    vec![
        PeerMessage::SyncOffer {
            sync_id: "1".repeat(32),
            link_id: "2".repeat(32),
            base_hash: "0".repeat(64),
            kind: rocket_network::SyncPayloadKind::Manifest,
            transfer_id: transfer_id(),
            size_bytes: 10,
            chunk_count: 1,
            sha256: "0".repeat(64),
        },
        PeerMessage::SyncAbort {
            sync_id: "1".repeat(32),
            reason: rocket_network::SyncAbortReason::Busy,
        },
        PeerMessage::SyncResult {
            sync_id: "1".repeat(32),
            outcome: rocket_network::SyncOutcome::Applied,
        },
    ]
}

#[tokio::test(start_paused = true)]
async fn sync_frames_reach_the_sink_and_count_as_pongs() {
    let (a, _h) = solo(2).await;
    let sink = Arc::new(RecordingSink::default());
    a.svc.set_transfer_sink(sink.clone());
    let remote = go_online(&a, 2).await;

    // The remote never answers a ping; one sync frame after every ping counts as the answer.
    for frame in sync_frames().into_iter().cycle().take(4) {
        tokio::time::advance(PING_INTERVAL).await;
        settle().await;
        remote.to_service.send(frame).await.expect("frame sent");
        settle().await;
    }

    assert_eq!(row_status(&a, &peer()), Some(NodeStatus::Online));
    let got: Vec<PeerMessage> = sink.frames().into_iter().map(|(_, m)| m).collect();
    assert_eq!(got.len(), 4);
    assert_eq!(got[..3], sync_frames()[..]);
}

#[tokio::test(start_paused = true)]
async fn a_sync_offer_from_a_blocked_device_never_reaches_the_sink() {
    let (a, _h) = solo(2).await;
    let sink = Arc::new(RecordingSink::default());
    a.svc.set_transfer_sink(sink.clone());
    let remote = go_online(&a, 2).await;
    a.blocked.put(peer(), "Peer");

    remote
        .to_service
        .send(sync_frames().remove(0))
        .await
        .expect("frame sent");
    settle().await;

    assert!(sink.frames().is_empty(), "the block list was read");
    assert_eq!(row_status(&a, &peer()), Some(NodeStatus::Offline));
}
```
Also add the two new fields (`keep_in_sync: false, link_id: None`) to the `CollectionOffer` in `frames()` of that file if Plan 1 has not.

- [ ] **Step 10: `sync_payload.rs`, the ZIP half.** Create the file with these items now (the stream half comes in Task 2):

```rust
//! Sync payloads: the in-memory ZIP of files, the base of a first send, and (Task 2) the
//! payload machines and their stream.

/// Every entry gets this time, so the same files always give the same bytes.
fn entry_time() -> zip::DateTime { zip::DateTime::default() }

/// A ZIP of `files` in path order, deflated, no folder entries, a fixed time per entry.
pub(super) fn pack_zip(files: &[SyncFile]) -> DomainResult<Vec<u8>>;

/// The entries of a received files ZIP after the strict validator passed. A refusal of the
/// validator (or any reader error) is `Stop::Unsafe`. Folder entries are skipped.
pub(super) fn unpack_zip(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, Stop>;

/// The base of a first send: strict validation, then every file with the single root folder
/// stripped (`<root>/a.yml` -> `a.yml`), hashed into a `Manifest`. A ZIP without exactly one
/// root folder, or a path `Manifest::insert` refuses, is `InvalidInput("The sent collection
/// cannot be linked.")`. The files come back in path order.
pub(super) fn base_from_zip(bytes: &[u8]) -> DomainResult<(Manifest, Vec<(String, Vec<u8>)>)>;

pub(super) fn sha_hex(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
```
`unpack_zip` reads at most `MAX_UNZIPPED_BYTES` in total (the validator already enforced it; reading uses `take` as a second guard).

- [ ] **Step 11: `sync_checks.rs`, the part Task 1 needs.** Create the file with `Stop` and its texts (all of S14 now, so later tasks only use them):

```rust
//! Pure rules of the sync: which paths are allowed, what a received plan must satisfy, the
//! starter's new base, the planner's split of actions, summaries, marks and fixed texts.

pub(super) const WENT_OFFLINE: &str = "The other node went offline.";
pub(super) const DAMAGED: &str = "The changes arrived damaged. Nothing was changed.";
pub(super) const UNSAFE: &str = "The changes were refused because a file is not safe. Nothing was changed.";
pub(super) const BAD_PLAN: &str = "The other node sent a sync plan that cannot be used. Nothing was changed.";
pub(super) const BASE_MISMATCH: &str = "The two copies disagree about the last agreed version. Unlink and send the collection again.";
pub(super) const LINK_MISSING: &str = "The link is missing on the other side. Unlink and send the collection again.";
pub(super) const FOLDER_CHANGED: &str = "The collection folder or uid changed. Unlink and send the collection again.";
pub(super) const CHANGED: &str = "The collection changed during the sync. Sync again.";
pub(super) const APPLY_FAILED: &str = "The changes could not be applied. Nothing was changed.";
pub(super) const PEER_APPLY_FAILED: &str = "The other node could not apply the changes.";
pub(super) const PEER_FAILED: &str = "The sync failed on the other node.";
pub(super) const PEER_TOO_BIG: &str = "The changes are too big to sync (limit 10 MiB).";
pub(super) const BUSY: &str = "The other node is busy with another sync. Try again.";
pub(super) const STALLED: &str = "The other node stopped answering.";
pub(super) const UNCONFIRMED: &str = "Applied here, but the other node did not confirm.";
pub(super) const RESULT_NOT_SENT: &str = "Applied here, but the other node did not get the result. Sync again.";
pub(super) const ALREADY_IN_SYNC: &str = "Already in sync.";
pub(super) const INTERNAL: &str = "The sync could not be planned.";

/// Why a session stops. Each maps to one outcome, one fixed text and the frame to send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Stop {
    Damaged, Unsafe, BadPlan, Changed, TooBig(u64), ApplyFailed, PeerApplyFailed, WentOffline,
    Stalled, Internal,
    Declined, Expired, Cancelled,   // this node's user or timer
    Peer(SyncAbortReason),          // the peer's SyncAbort
}

impl Stop {
    /// Declined -> Declined, Expired -> Expired, Cancelled -> Cancelled, Peer(r) -> map(r) of the
    /// state table, every other stop -> Failed.
    pub(super) fn finish(self) -> SyncFinish;
    /// The fixed text: Damaged DAMAGED, Unsafe UNSAFE, BadPlan BAD_PLAN, Changed CHANGED,
    /// ApplyFailed APPLY_FAILED, PeerApplyFailed PEER_APPLY_FAILED, WentOffline WENT_OFFLINE,
    /// Stalled STALLED, Internal INTERNAL, Declined/Expired/Cancelled None, Peer(r) as `map(r)`.
    /// `TooBig(len)` is "The changes are too big to sync (X.Y MiB, limit 10)." with the same
    /// rounding as `offer_error` in `transfer_send.rs`.
    pub(super) fn message(self) -> Option<String>;
    /// The abort to tell the peer: TooBig too_big, Declined declined, Expired expired, Cancelled
    /// cancelled, WentOffline, PeerApplyFailed and Peer(_) none, every other stop failed.
    pub(super) fn abort(self) -> Option<SyncAbortReason>;
}

/// The wire name of a finish, for the audit `outcome` field. A test pins it to serde.
pub(super) fn finish_name(finish: SyncFinish) -> &'static str;
```
Write the bodies as the doc comments say, and these unit tests at the bottom of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finish_names_match_the_wire_names() {
        for f in [
            SyncFinish::Applied, SyncFinish::Declined, SyncFinish::Expired, SyncFinish::Cancelled,
            SyncFinish::Failed, SyncFinish::BaseMismatch, SyncFinish::LinkMissing, SyncFinish::Unconfirmed,
        ] {
            let wire = serde_json::to_value(f).expect("json");
            assert_eq!(wire.as_str(), Some(finish_name(f)));
        }
    }

    #[test]
    fn each_peer_abort_maps_to_its_outcome_and_text() {
        use SyncAbortReason as R;
        let rows = [
            (R::Declined, SyncFinish::Declined, None),
            (R::Expired, SyncFinish::Expired, None),
            (R::Cancelled, SyncFinish::Cancelled, None),
            (R::BaseMismatch, SyncFinish::BaseMismatch, Some(BASE_MISMATCH)),
            (R::LinkMissing, SyncFinish::LinkMissing, Some(LINK_MISSING)),
            (R::Busy, SyncFinish::Failed, Some(BUSY)),
            (R::TooBig, SyncFinish::Failed, Some(PEER_TOO_BIG)),
            (R::Failed, SyncFinish::Failed, Some(PEER_FAILED)),
        ];
        for (reason, finish, text) in rows {
            let stop = Stop::Peer(reason);
            assert_eq!(stop.finish(), finish, "{reason:?}");
            assert_eq!(stop.message().as_deref(), text, "{reason:?}");
            assert_eq!(stop.abort(), None, "nothing is sent back for a peer abort");
        }
    }

    #[test]
    fn too_big_names_the_size_and_tells_the_peer_too_big() {
        let stop = Stop::TooBig(10 * 1024 * 1024 + 1);
        assert_eq!(stop.finish(), SyncFinish::Failed);
        assert_eq!(
            stop.message().as_deref(),
            Some("The changes are too big to sync (10.1 MiB, limit 10).")
        );
        assert_eq!(stop.abort(), Some(SyncAbortReason::TooBig));
        assert_eq!(Stop::WentOffline.abort(), None);
        assert_eq!(Stop::Damaged.abort(), Some(SyncAbortReason::Failed));
    }
}
```

- [ ] **Step 12: `sync.rs`.** Create it with the module doc, the types, the registry, the worker and the link calls. Signatures and the parts the tests do not fully pin:

```rust
//! The sync service: keeps linked collections in step with one paired node.
//!
//! Same shape as the transfer service: one queue, one worker that only runs short sync
//! handlers, spawned tasks for the slow work, one `std::sync::Mutex` never held across an
//! `.await`, and whoever removes a session emits its single `Finished`. The payloads ride the
//! `rocket-network` transfer machines (`sync_payload.rs`); the starter and planner halves are
//! `sync_starter.rs` and `sync_planner.rs`; the pure rules are `sync_checks.rs`.

const SYNC_FRAME_QUEUE: usize = 256;
const UNKNOWN_NODE: &str = "Unknown node";
/// The gap between the two "is it still paired" reads before links are deleted (S16).
const UNPAIR_RECHECK: Duration = Duration::from_millis(50);

pub struct SyncDeps { /* the six fields of the Interfaces block */ }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkView { /* the seven fields of the Interfaces block */ }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictFiles { pub mine: Option<String>, pub theirs: Option<String> }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    // starter
    SendManifest, AwaitPlan, RecvPlan, Exporting, SendFiles, AwaitFiles, RecvFiles, AwaitConfirm, Applying, AwaitResult,
    // planner (shares SendFiles, AwaitFiles, RecvFiles, Applying)
    Checking, RecvManifest, Planning, SendRequest, AwaitReview, Finalizing, SendFinal, AwaitStarter,
}

pub(super) struct Session {
    pub sync_id: SyncId,
    pub role: SyncRole,
    pub device: DeviceId,
    pub peer_name: String,
    pub short_id: String,
    pub link_id: LinkId,
    pub link: Option<LinkRecord>,  // the record as read at the start (planner: from `Checking` on)
    pub base_hash: String,         // the base hash at the start; every SyncOffer of the session carries it
    pub phase: Phase,
    pub phase_since: Instant,      // when the phase began; the watchdog counts from here
    pub gen: u64,                  // bumped on every phase change; tasks compare it
    pub limits_key: TransferId,    // the session's entry in `sync_limits`
    pub out: Option<PayloadOut>,
    pub inc: Option<PayloadIn>,
    pub mine: Option<Manifest>,    // this node's manifest the plan is built from
    pub theirs: Option<Manifest>,  // planner: the starter's manifest
    pub plan: Option<SyncPlan>,    // planner
    pub wanted: BTreeSet<String>,  // planner: asked for; starter: was asked for
    pub received: BTreeMap<String, Vec<u8>>,       // checked files from the peer
    pub mine_conflicts: BTreeMap<String, Vec<u8>>, // planner: its own export of each conflict path
    pub decisions: Vec<Decision>,
    pub final_plan: Option<SyncPlanPayload>,
    pub changes: Vec<FileChange>,  // what this node applies, in apply order
    pub sent_files: BTreeMap<String, Vec<u8>>,     // the bytes this node sent
    pub new_base: Option<Manifest>,
    pub chunk_out: bool,           // a chunk of this node left (S13)
    pub answered: bool,            // this node's user answered its dialog (S17)
    pub peer_gone: bool,           // set while Applying (S7)
}

#[derive(Default)]
pub(super) struct SyncInner {
    pub sessions: HashMap<SyncId, Session>,
    pub transfers: HashMap<TransferId, (DeviceId, SyncId)>,  // the router's lookup
    pub starting: HashSet<DeviceId>,   // `start` claimed this peer and is still exporting
    pub settling: HashSet<DeviceId>,   // a session ended and its link update still runs (S6)
    pub limits: TransferLimits,
}

/// What a session's end does to its link. The update runs in its own blocking task after the
/// `Finished` event; the peer stays busy (`settling`) until it is done.
pub(super) enum LinkUpdate {
    Keep,
    State(LinkState),
    /// `known` holds the bytes this node sent or received; the rest of the base files come
    /// from the old base or a fresh export (S8). `state: None` derives `in_sync` or
    /// `changes_waiting` from a fresh manifest.
    NewBase { base: Manifest, known: BTreeMap<String, Vec<u8>>, state: Option<LinkState> },
}

pub struct SyncService {
    me: Weak<SyncService>,
    pub(super) deps: SyncDeps,
    pub(super) timing: Timing,
    inner: Mutex<SyncInner>,
    frames: mpsc::Sender<Item>,
    worker: Mutex<Option<mpsc::Receiver<Item>>>,
    #[cfg(test)] frames_seen: AtomicUsize,
    #[cfg(test)] gone_seen: AtomicUsize,
}
```

`new`, `new_with_timing`, `lock`, `arc`, `now`, `emit`, `ensure_worker`, `run_worker`, `enqueue` and `tell`/`tell_later` are the same as in `transfer.rs` (copy them; `enqueue` runs a `Gone` inline when the queue is full). `new_with_timing` does not call `set_transfer_sink`. The `TransferSink` impl: `on_frame` increments `frames_seen` under `#[cfg(test)]`, `ensure_worker`, `enqueue(Item::Frame(..))`; `on_peer_gone` increments `gone_seen`, `ensure_worker`, `enqueue(Item::Gone(..))`. `handle_frame` matches the frame kinds and calls the handlers of Tasks 2 and 3 (in this task: a `match` whose arms are empty except `_ => {}`; Task 2 fills them).

```rust
    /// True when the router must hand this frame to the sync service: a `Sync*` frame, or a
    /// transfer frame whose id is a sync payload of this very peer. Short lock, one lookup.
    pub(crate) fn claims(&self, from: &DeviceId, message: &PeerMessage) -> bool {
        let id = match message {
            PeerMessage::SyncOffer { .. } | PeerMessage::SyncAbort { .. } | PeerMessage::SyncResult { .. } => return true,
            PeerMessage::OfferAnswer { transfer_id, .. }
            | PeerMessage::Chunk { transfer_id, .. }
            | PeerMessage::ChunkAck { transfer_id, .. }
            | PeerMessage::TransferAbort { transfer_id, .. }
            | PeerMessage::TransferResult { transfer_id, .. } => transfer_id,
            _ => return false,
        };
        let Ok(id) = TransferId::parse(id) else { return false };
        self.lock().transfers.get(&id).is_some_and(|(device, _)| device == from)
    }

    /// Stores a new link. An existing link id is `AlreadyExists("That link already exists.")`.
    /// Publishes `LinkChanged`. Blocks (a repository call).
    pub fn create_link(&self, record: LinkRecord, base_files: Vec<(String, Vec<u8>)>) -> DomainResult<()>;

    /// The link of the first linked send, on either end (S18). Builds the base with
    /// `base_from_zip`, reads `collection_root` and `collection_uid`, and calls `create_link`
    /// with state `in_sync` and `updated_at` now (RFC 3339, UTC). Blocks.
    pub(crate) fn link_first_transfer(&self, link_id: LinkId, peer: DeviceId, collection: &str, zip: &[u8]) -> DomainResult<()>;

    /// Every link with its derived state (S15), sorted by collection name then link id. Blocks.
    pub fn links(&self) -> DomainResult<Vec<LinkView>>;

    /// Deletes a link and its base. A session of that link is cancelled first (S17 rules, it
    /// sends `SyncAbort { cancelled }` when it may). Audits `NetworkLinkRemoved` and publishes
    /// `LinkChanged`. An unknown link is `NotFound("That link does not exist.")`.
    pub async fn unlink(&self, link_id: &LinkId) -> DomainResult<()>;

    /// Test hook: registers a sync payload id for `device`, as an outgoing payload would.
    #[cfg(test)]
    pub(crate) fn claim_transfer_for_test(&self, device: DeviceId) -> String;
    #[cfg(test)] pub(crate) fn frames_seen(&self) -> usize;
    #[cfg(test)] pub(crate) fn gone_seen(&self) -> usize;
```
`claim_transfer_for_test` inserts `(TransferId::random(), (device, SyncId::random()))` into `transfers` and returns the id text.

`handle_gone(device)`: ends that device's sessions (Task 2 fills the per-phase rules; in this task it only spawns the S16 link check): spawn a blocking task that returns early when `peer_label(device)` is `Some`, sleeps `UNPAIR_RECHECK`, asks again, and only when both are `None` deletes every link whose `peer_device_id == device`, each followed by `NetworkLinkRemoved { device_id: device.short(), collection }` and `LinkChanged`.

- [ ] **Step 13: `frame_router.rs`, `mod.rs`, `lib.rs`.**

```rust
//! The one frame sink of the node: sync frames to the sync service, the rest to the transfer
//! service. Both get every peer-gone notice.

pub struct FrameRouter {
    transfer: Arc<TransferService>,
    sync: Arc<SyncService>,
}

impl FrameRouter {
    pub fn new(transfer: Arc<TransferService>, sync: Arc<SyncService>) -> Arc<FrameRouter> {
        Arc::new(FrameRouter { transfer, sync })
    }
}

impl TransferSink for FrameRouter {
    fn on_frame(&self, from: DeviceId, message: PeerMessage) {
        if let PeerMessage::SyncOffer { transfer_id, .. } = &message {
            // A sync payload may not take over an id the transfer service already uses.
            let taken = TransferId::parse(transfer_id).is_ok_and(|id| self.transfer.has_transfer(&id));
            if taken {
                tracing::debug!(code = "network_sync_offer_id_taken", "sync offer dropped");
                return;
            }
        }
        if self.sync.claims(&from, &message) {
            self.sync.on_frame(from, message);
        } else {
            self.transfer.on_frame(from, message);
        }
    }

    fn on_peer_gone(&self, device: DeviceId) {
        self.transfer.on_peer_gone(device);
        self.sync.on_peer_gone(device);
    }
}
```
`mod.rs`: `mod frame_router; mod sync; mod sync_checks; mod sync_payload; mod sync_planner; mod sync_starter;` (the last two may be empty files with their module doc until Task 2), re-exports `pub use frame_router::FrameRouter; pub use sync::{ConflictFiles, LinkView, SyncDeps, SyncService};`. `crates/rocket-app/src/lib.rs` re-exports the same five names next to the transfer ones.

- [ ] **Step 14: TransferService.** In `transfer.rs` add the field `sync: Mutex<Option<Arc<SyncService>>>` (initialised `None`), and:

```rust
    /// Gives the transfer service the sync service, so a linked send can create its link.
    pub fn set_sync(&self, sync: Arc<SyncService>) { /* store it */ }

    /// True when a transfer with this id is open in either direction.
    pub(crate) fn has_transfer(&self, id: &TransferId) -> bool {
        let inner = self.lock();
        inner.outgoing.contains_key(id) || inner.incoming.contains_key(id)
    }

    /// Creates the link of a linked transfer from its own task. A failure is logged, never shown.
    pub(super) fn spawn_link(&self, link: LinkId, peer: DeviceId, collection: String, zip: Vec<u8>);
```
`spawn_link` clones the `SyncService` out of the mutex (nothing when it is `None`), runs `link_first_transfer` in `spawn_blocking`, and logs an error with `tracing::warn!(code = "network_link_create_failed", "link not created")` (the error text at debug only).

In `transfer_send.rs`:
- `send` gains `keep_in_sync: bool`. After the name and before the paired check: `keep_in_sync && include_environments` is the S18 refusal; `keep_in_sync` with no sync service is "Keep in sync is not available.". Both before any export.
- `Out` gains `link: Option<LinkId>`. `offer_bytes` takes `link: Option<LinkId>`; the offer frame is `machine.offer_message()` with `keep_in_sync` and `link_id` set from it (a small `fn with_link(frame, link: Option<&LinkId>) -> PeerMessage`). `offer_raw` passes `None`.
- `finish_out`: when the outcome is `Completed` and `out.link` is `Some`, call `spawn_link(link, out.device, out.collection.clone(), out.zip)` after the event and the audit.

In `transfer_receive.rs`:
- `OfferFrame` gains `keep_in_sync: bool, link_id: Option<String>`. `admit`: with `keep_in_sync`, `link_id` must parse with `LinkId::parse`, else the offer is refused `AbortReason::Failed` (and its limits entry released, as for any refused offer).
- `In` gains `keep: Option<(LinkId, Vec<u8>)>`; `take_chunk_step` appends the chunk bytes to it after a good `append`.
- `finalize`: after `Ok(imported)`, when `keep` is `Some`, `spawn_link(link, device, imported_name, bytes)`. `final_view` returns the `keep` taken out of the entry.

Change every call `send(&x, y, z)` to `send(&x, y, z, false)` in `transfer_send_tests.rs`, `transfer_receive_tests.rs`, `e2e_transfer_tests.rs`, and in `src-tauri/src/commands/network.rs` (`svc.send(&id, &collection, include_environments, false)`; Plan 4 replaces the `false` with the command parameter).

- [ ] **Step 15: Run the Task 1 tests.**

Run: `cargo test -j4 -p rocket-app sync_tests`
Expected: PASS, 25 tests.

Run: `cargo test -j4 -p rocket-app sync_checks`
Expected: PASS, 3 tests.

Run: `cargo test -j4 -p rocket-app transfer_link_tests`
Expected: PASS, the 16 earlier tests and the 2 new ones.

Run: `cargo test -j4 -p rocket-app -- transfer_send_tests transfer_receive_tests e2e_transfer_tests`
Expected: PASS, no test changed its result.

Run: `cargo check -j4 -p rocket`
Expected: `Finished`, no error (the Tauri crate builds with the new `send` parameter).

---

### Task 2: One sync run, from the manifest to both results

**Files:**
- Modify: `crates/rocket-network/src/link.rs` (S2), `crates/rocket-app/src/network/{sync.rs, sync_payload.rs, sync_checks.rs, sync_fakes.rs, mod.rs}`
- Create (fill): `crates/rocket-app/src/network/{sync_starter.rs, sync_planner.rs, sync_run_tests.rs}`

**Interfaces:**

```rust
// Consumes (Plan 1)
rocket_network::{plan_sync, resolve, new_base, SyncAction, SyncPlan, Conflict, ConflictKind, Decision, Resolution,
    SyncSummary, ConflictView, SyncRole, SyncFinish, SyncPlanPayload, PlanRename, SyncOutcome}
// Produces (rocket-network, S2)
pub struct PlanFile { pub path: String, pub sha256: String }       // in link.rs, snake_case serde
// SyncPlanPayload gains: #[serde(default)] pub sends: Vec<PlanFile>, #[serde(default)] pub new_base_hash: Option<String>
// Produces (rocket-app)
impl SyncService {
    pub async fn start(&self, link_id: &LinkId) -> DomainResult<SyncId>;
    pub async fn review(&self, sync_id: &SyncId, accept: bool, decisions: Vec<Decision>) -> DomainResult<()>;
    pub async fn confirm(&self, sync_id: &SyncId, accept: bool) -> DomainResult<()>;
    pub async fn cancel(&self, sync_id: &SyncId) -> DomainResult<()>;
    pub fn conflict_files(&self, sync_id: &SyncId, path: &str) -> DomainResult<ConflictFiles>;
    #[cfg(test)] pub(crate) fn phase_of(&self, sync_id: &SyncId) -> Option<Phase>;
}
// sync_checks.rs
pub(super) fn path_allowed(path: &str) -> bool;
pub(super) fn keep_both_allowed(path: &str) -> bool;
pub(super) fn check_request(plan: &SyncPlanPayload, offered: &Manifest) -> Result<BTreeSet<String>, Stop>;
pub(super) fn check_final(plan: &SyncPlanPayload, offered: &Manifest) -> Result<Manifest, Stop>;
pub(super) fn starter_new_base(offered: &Manifest, plan: &SyncPlanPayload) -> Result<Manifest, Stop>;
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct Split { pub deletes: Vec<String>, pub renames: Vec<(String, String)>, pub puts: Vec<String>,
    pub put_as: Vec<(String, String)>, pub peer_deletes: Vec<String>, pub peer_renames: Vec<(String, String)>,
    pub sends: BTreeSet<String> }
pub(super) fn split_actions(actions: &[SyncAction], mine: &Manifest) -> Split;
pub(super) fn wanted_paths(plan: &SyncPlan) -> BTreeSet<String>;
pub(super) fn apply_to_manifest(mine: &Manifest, split: &Split, theirs: &Manifest) -> Result<Manifest, Stop>;
pub(super) fn entries_match(files: &BTreeMap<String, Vec<u8>>, expected: &BTreeMap<String, String>) -> Result<(), Stop>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub(super) struct Marks { pub script: bool, pub auth: bool }
pub(super) fn file_marks(bytes: &[u8]) -> Marks;
pub(super) fn planner_summary(plan: &SyncPlan, mine: &Manifest, theirs: &Manifest, received: &BTreeMap<String, Vec<u8>>) -> SyncSummary;
pub(super) fn starter_summary(plan: &SyncPlanPayload, offered: &Manifest, wanted: usize, received: &BTreeMap<String, Vec<u8>>) -> SyncSummary;
```

- [ ] **Step 1: Read the reference.** 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Then read spec section 6 again (the table and the rename rule) and the Plan 1 doc comments of `plan_sync`, `resolve` and `new_base`.

- [ ] **Step 2: Write the failing payload test (S2).** In the tests module of `crates/rocket-network/src/link.rs`:

```rust
    #[test]
    fn a_plan_payload_round_trips_sends_and_the_new_base_hash() {
        let plan = SyncPlanPayload {
            wanted: vec![],
            deletes: vec!["d.yml".into()],
            renames: vec![],
            decisions: vec![],
            sends: vec![PlanFile {
                path: "a.yml".into(),
                sha256: "a".repeat(64),
            }],
            new_base_hash: Some("b".repeat(64)),
        };
        let bytes = plan.to_json().expect("json");
        assert_eq!(SyncPlanPayload::from_json(&bytes).expect("parsed"), plan);

        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("a value");
        assert_eq!(json["sends"][0]["path"], "a.yml");
        assert_eq!(json["sends"][0]["sha256"], "a".repeat(64));
        assert_eq!(json["new_base_hash"], "b".repeat(64));

        let request = br#"{"wanted":["a.yml"],"deletes":[],"renames":[],"decisions":[]}"#;
        let parsed = SyncPlanPayload::from_json(request).expect("a request without the new keys");
        assert!(parsed.sends.is_empty());
        assert_eq!(parsed.new_base_hash, None);
    }
```

Run: `cargo test -j4 -p rocket-network a_plan_payload_round_trips_sends_and_the_new_base_hash`
Expected: FAIL to compile, `PlanFile` and the two fields do not exist.

- [ ] **Step 3: Add the fields.** In `link.rs`:

```rust
/// A file the planner sends, with the hash the starter must find in the files payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanFile {
    pub path: String,
    pub sha256: String,
}
```
and on `SyncPlanPayload` (derive `PartialEq` if it lacks it): `#[serde(default)] pub sends: Vec<PlanFile>` and `#[serde(default)] pub new_base_hash: Option<String>`. Re-export `PlanFile` from the crate root next to `SyncPlanPayload`. Write `Ruling: S2 SyncPlanPayload gains sends and new_base_hash (Plan 3).` in the ledger. Fix every struct literal of `SyncPlanPayload` in Plan 1's tests with `sends: vec![], new_base_hash: None`.

Run: `cargo test -j4 -p rocket-network link`
Expected: PASS, every `link` test.

- [ ] **Step 4: Write the failing tests of the pure rules.** Append to `sync_checks.rs`:

```rust
#[cfg(test)]
mod rule_tests {
    use std::collections::{BTreeMap, BTreeSet};

    use rocket_network::{Manifest, PlanFile, PlanRename, SyncAction, SyncPlan, SyncPlanPayload};

    use super::*;
    use crate::network::sync_payload::sha_hex;

    fn h(c: char) -> String {
        c.to_string().repeat(64)
    }

    fn m(list: &[(&str, char)]) -> Manifest {
        let mut x = Manifest::new();
        for (path, c) in list {
            x.insert(path, &h(*c)).expect("a valid test path");
        }
        x
    }

    fn payload() -> SyncPlanPayload {
        SyncPlanPayload {
            wanted: vec![],
            deletes: vec![],
            renames: vec![],
            decisions: vec![],
            sends: vec![],
            new_base_hash: None,
        }
    }

    fn rename(from: &str, to: &str) -> PlanRename {
        PlanRename {
            from: from.into(),
            to: to.into(),
        }
    }

    fn send(path: &str, c: char) -> PlanFile {
        PlanFile {
            path: path.into(),
            sha256: h(c),
        }
    }

    fn owned(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn path_allowed_follows_the_manifest_rule_and_refuses_nested_collection_files() {
        for ok in ["a.yml", "folder/a.yml", "opencollection.yml", "bruno.json", "f/folder.yml", "a (from peer).yml"] {
            assert!(path_allowed(ok), "{ok}");
        }
        for bad in ["", "../a.yml", "/a.yml", "a//b.yml", ".rocket/x.yml", "f/opencollection.yml", "f/Bruno.json", "CON.yml"] {
            assert!(!path_allowed(bad), "{bad}");
        }
    }

    #[test]
    fn keep_both_is_only_for_request_files() {
        assert!(keep_both_allowed("a.yml"));
        assert!(keep_both_allowed("f/a.yml"));
        assert!(!keep_both_allowed("opencollection.yml"));
        assert!(!keep_both_allowed("folder.yml"));
        assert!(!keep_both_allowed("f/g/folder.yml"));
    }

    #[test]
    fn a_request_names_only_offered_paths_and_nothing_else() {
        let offered = m(&[("a.yml", 'a'), ("b.yml", 'b')]);
        let mut p = payload();
        p.wanted = vec!["a.yml".into()];
        assert_eq!(check_request(&p, &offered), Ok(BTreeSet::from(["a.yml".to_string()])));

        p.wanted = vec!["c.yml".into()];
        assert_eq!(check_request(&p, &offered), Err(Stop::Unsafe), "not offered");
        p.wanted = vec!["../a.yml".into()];
        assert_eq!(check_request(&p, &offered), Err(Stop::Unsafe), "unsafe");
        p.wanted = vec!["a.yml".into(), "a.yml".into()];
        assert_eq!(check_request(&p, &offered), Err(Stop::BadPlan), "twice");
        p.wanted = vec![];
        assert_eq!(check_request(&p, &offered), Err(Stop::BadPlan), "an empty request is never sent");

        let mut q = payload();
        q.wanted = vec!["a.yml".into()];
        q.deletes = vec!["b.yml".into()];
        assert_eq!(check_request(&q, &offered), Err(Stop::BadPlan), "a request carries nothing else");
    }

    #[test]
    fn the_starter_new_base_applies_deletes_then_renames_then_sends() {
        let offered = m(&[("a.yml", 'a'), ("b.yml", 'b'), ("c.yml", 'c')]);
        let mut p = payload();
        p.deletes = vec!["c.yml".into()];
        p.renames = vec![rename("b.yml", "c.yml")];
        p.sends = vec![send("a.yml", 'd'), send("n.yml", 'e')];

        assert_eq!(
            starter_new_base(&offered, &p),
            Ok(m(&[("a.yml", 'd'), ("c.yml", 'b'), ("n.yml", 'e')]))
        );
    }

    #[test]
    fn a_valid_final_plan_returns_the_new_base() {
        let offered = m(&[("a.yml", 'a'), ("b.yml", 'b')]);
        let mut p = payload();
        p.deletes = vec!["a.yml".into()];
        p.sends = vec![send("b.yml", 'c')];
        let expected = m(&[("b.yml", 'c')]);
        p.new_base_hash = Some(expected.base_hash());

        assert_eq!(check_final(&p, &offered), Ok(expected));
    }

    #[test]
    fn a_final_plan_is_refused_for_each_broken_rule() {
        let offered = m(&[("a.yml", 'a'), ("b.yml", 'b')]);
        let any = Some(h('0'));
        let cases: Vec<(&str, Box<dyn Fn(&mut SyncPlanPayload)>, Stop)> = vec![
            ("unsafe delete", Box::new(|p| p.deletes = vec!["../a.yml".into()]), Stop::Unsafe),
            ("unknown delete", Box::new(|p| p.deletes = vec!["x.yml".into()]), Stop::Unsafe),
            ("delete twice", Box::new(|p| p.deletes = vec!["a.yml".into(), "a.yml".into()]), Stop::Unsafe),
            ("rename of an unknown file", Box::new(|p| p.renames = vec![rename("x.yml", "y.yml")]), Stop::Unsafe),
            ("rename over a file", Box::new(|p| p.renames = vec![rename("a.yml", "b.yml")]), Stop::Unsafe),
            ("rename to an unsafe path", Box::new(|p| p.renames = vec![rename("a.yml", "../y.yml")]), Stop::Unsafe),
            ("rename to a nested collection", Box::new(|p| p.renames = vec![rename("a.yml", "f/opencollection.yml")]), Stop::Unsafe),
            ("send to an unsafe path", Box::new(|p| p.sends = vec![send(".rocket/x.yml", 'c')]), Stop::Unsafe),
            ("send twice", Box::new(|p| p.sends = vec![send("n.yml", 'c'), send("n.yml", 'd')]), Stop::Unsafe),
            ("send a deleted path", Box::new(|p| { p.deletes = vec!["a.yml".into()]; p.sends = vec![send("a.yml", 'c')]; }), Stop::Unsafe),
            ("send with a bad hash", Box::new(|p| p.sends = vec![PlanFile { path: "n.yml".into(), sha256: "XYZ".into() }]), Stop::BadPlan),
            ("decision on an unsafe path", Box::new(|p| p.decisions = vec![rocket_network::Decision { path: "../a.yml".into(), resolution: rocket_network::Resolution::KeepMine }]), Stop::Unsafe),
            ("a final plan that also wants files", Box::new(|p| p.wanted = vec!["a.yml".into()]), Stop::BadPlan),
        ];
        for (what, edit, stop) in cases {
            let mut p = payload();
            p.new_base_hash = any.clone();
            edit(&mut p);
            assert_eq!(check_final(&p, &offered), Err(stop), "{what}");
        }

        let mut no_hash = payload();
        no_hash.deletes = vec!["a.yml".into()];
        assert_eq!(check_final(&no_hash, &offered), Err(Stop::BadPlan), "no new base hash");
        no_hash.new_base_hash = Some(m(&[("a.yml", 'a')]).base_hash());
        assert_eq!(check_final(&no_hash, &offered), Err(Stop::BadPlan), "a hash the starter does not reach");
    }

    #[test]
    fn split_puts_each_action_on_its_side() {
        let mine = m(&[("k.yml", 'a'), ("s.yml", 'b'), ("r.yml", 'c'), ("d.yml", 'd')]);
        let actions = vec![
            SyncAction::Pull { path: "p.yml".into() },
            SyncAction::PullAs { path: "k.yml".into(), as_path: "k (from peer).yml".into() },
            SyncAction::Push { path: "s.yml".into() },
            SyncAction::DeleteMine { path: "d.yml".into() },
            SyncAction::DeleteTheirs { path: "t.yml".into() },
            SyncAction::RenameMine { from: "r.yml".into(), to: "r2.yml".into() },
            SyncAction::RenameTheirs { from: "q.yml".into(), to: "q2.yml".into() },
        ];

        let s = split_actions(&actions, &mine);

        assert_eq!(s.puts, vec!["p.yml"]);
        assert_eq!(s.put_as, owned(&[("k.yml", "k (from peer).yml")]));
        assert_eq!(s.deletes, vec!["d.yml"]);
        assert_eq!(s.renames, owned(&[("r.yml", "r2.yml")]));
        assert_eq!(s.peer_deletes, vec!["t.yml"]);
        assert_eq!(
            s.peer_renames,
            owned(&[("k.yml", "k (from peer).yml"), ("q.yml", "q2.yml")]),
            "keep both moves the peer's copy aside"
        );
        assert_eq!(
            s.sends,
            BTreeSet::from(["k.yml".to_string(), "s.yml".to_string()]),
            "and sends it mine for the original path"
        );
    }

    #[test]
    fn keep_both_moves_already_listed_by_resolve_are_not_doubled() {
        let mine = m(&[("k.yml", 'a')]);
        let actions = vec![
            SyncAction::PullAs { path: "k.yml".into(), as_path: "k (from peer).yml".into() },
            SyncAction::RenameTheirs { from: "k.yml".into(), to: "k (from peer).yml".into() },
            SyncAction::Push { path: "k.yml".into() },
        ];
        let s = split_actions(&actions, &mine);
        assert_eq!(s.peer_renames.len(), 1);
        assert_eq!(s.sends.len(), 1);
    }

    #[test]
    fn keep_both_without_a_local_file_sends_nothing_for_the_path() {
        let s = split_actions(
            &[SyncAction::PullAs { path: "k.yml".into(), as_path: "k (from peer).yml".into() }],
            &Manifest::new(),
        );
        assert!(s.sends.is_empty());
        assert_eq!(s.peer_renames, owned(&[("k.yml", "k (from peer).yml")]));
    }

    #[test]
    fn the_planner_manifest_after_its_changes() {
        let mine = m(&[("d.yml", '1'), ("r.yml", '2'), ("p.yml", '3'), ("k.yml", '4')]);
        let theirs = m(&[("p.yml", '5'), ("k.yml", '6')]);
        let s = Split {
            deletes: vec!["d.yml".into()],
            renames: owned(&[("r.yml", "r2.yml")]),
            puts: vec!["p.yml".into()],
            put_as: owned(&[("k.yml", "k (from peer).yml")]),
            ..Split::default()
        };

        assert_eq!(
            apply_to_manifest(&mine, &s, &theirs),
            Ok(m(&[("r2.yml", '2'), ("p.yml", '5'), ("k.yml", '4'), ("k (from peer).yml", '6')]))
        );
        let missing = Split { puts: vec!["x.yml".into()], ..Split::default() };
        assert_eq!(apply_to_manifest(&mine, &missing, &theirs), Err(Stop::Internal));
    }

    #[test]
    fn wanted_paths_are_pulls_and_conflicts_the_peer_still_has() {
        let plan = SyncPlan {
            actions: vec![
                SyncAction::Pull { path: "p.yml".into() },
                SyncAction::Push { path: "s.yml".into() },
                SyncAction::RenameMine { from: "r.yml".into(), to: "r2.yml".into() },
            ],
            conflicts: vec![
                rocket_network::Conflict { path: "c.yml".into(), kind: rocket_network::ConflictKind::BothChanged, mine: Some(h('1')), theirs: Some(h('2')) },
                rocket_network::Conflict { path: "e.yml".into(), kind: rocket_network::ConflictKind::DeleteVsEdit, mine: Some(h('1')), theirs: None },
            ],
        };
        assert_eq!(
            wanted_paths(&plan),
            BTreeSet::from(["c.yml".to_string(), "p.yml".to_string()])
        );
    }

    #[test]
    fn received_entries_must_be_exactly_the_expected_set_with_their_hashes() {
        let files: BTreeMap<String, Vec<u8>> = [("a.yml".to_string(), b"x".to_vec())].into();
        let good: BTreeMap<String, String> = [("a.yml".to_string(), sha_hex(b"x"))].into();
        assert_eq!(entries_match(&files, &good), Ok(()));

        let mut more = good.clone();
        more.insert("b.yml".into(), sha_hex(b"y"));
        assert_eq!(entries_match(&files, &more), Err(Stop::Unsafe), "an entry is missing");
        assert_eq!(entries_match(&files, &BTreeMap::new()), Err(Stop::Unsafe), "an entry not in the plan");
        let wrong: BTreeMap<String, String> = [("a.yml".to_string(), sha_hex(b"z"))].into();
        assert_eq!(entries_match(&files, &wrong), Err(Stop::Damaged));
    }

    #[test]
    fn marks_find_scripts_and_auth() {
        let none = Marks { script: false, auth: false };
        assert_eq!(file_marks(b"http:\n  url: x\n"), none);
        assert!(file_marks(b"runtime:\n  scripts:\n    - type: before-request\n      code: x\n").script);
        assert!(file_marks(b"tests: \"expect(1)\"\n").script);
        assert_eq!(file_marks(b"scripts: []\n"), none, "an empty list is no script");
        assert_eq!(file_marks(b"http:\n  auth: inherit\n"), none);
        assert_eq!(file_marks(b"http:\n  auth:\n    type: none\n"), none);
        assert!(file_marks(b"http:\n  auth:\n    type: bearer\n    token: ''\n").auth);
        assert_eq!(
            file_marks(b": not yaml: ["),
            Marks { script: true, auth: true },
            "an unreadable file counts as both"
        );
    }

    #[test]
    fn the_planner_summary_counts_each_side_and_marks_incoming_files() {
        let mine = m(&[("c.yml", '1'), ("d.yml", '2'), ("r.yml", '3'), ("s.yml", '4'), ("x.yml", '5')]);
        let theirs = m(&[("c.yml", '6'), ("n.yml", '7'), ("s.yml", '8'), ("t.yml", '9')]);
        let plan = SyncPlan {
            actions: vec![
                SyncAction::Pull { path: "c.yml".into() },
                SyncAction::Pull { path: "n.yml".into() },
                SyncAction::DeleteMine { path: "d.yml".into() },
                SyncAction::RenameMine { from: "r.yml".into(), to: "r2.yml".into() },
                SyncAction::Push { path: "s.yml".into() },
                SyncAction::Push { path: "x.yml".into() },
                SyncAction::DeleteTheirs { path: "t.yml".into() },
            ],
            conflicts: vec![],
        };
        let received: BTreeMap<String, Vec<u8>> = [
            ("c.yml".to_string(), b"scripts:\n  - code: x\n".to_vec()),
            ("n.yml".to_string(), b"auth:\n  type: basic\n".to_vec()),
        ]
        .into();

        let s = planner_summary(&plan, &mine, &theirs, &received);

        assert_eq!(
            (s.added_here, s.changed_here, s.deleted_here, s.sent_added, s.sent_changed, s.sent_deleted),
            (1, 2, 1, 1, 1, 1)
        );
        assert_eq!(s.scripts_changed, vec!["c.yml"]);
        assert_eq!(s.auth_changed, vec!["n.yml"]);
    }

    #[test]
    fn the_starter_summary_counts_its_own_changes() {
        let offered = m(&[("a.yml", '1'), ("b.yml", '2'), ("r.yml", '3')]);
        let mut p = payload();
        p.deletes = vec!["b.yml".into()];
        p.renames = vec![rename("r.yml", "r2.yml")];
        p.sends = vec![send("a.yml", '4'), send("n.yml", '5')];
        let received: BTreeMap<String, Vec<u8>> = [
            ("a.yml".to_string(), b"url: a\n".to_vec()),
            ("n.yml".to_string(), b"tests: x\n".to_vec()),
        ]
        .into();

        let s = starter_summary(&p, &offered, 2, &received);

        assert_eq!(
            (s.added_here, s.changed_here, s.deleted_here, s.sent_added, s.sent_changed, s.sent_deleted),
            (1, 2, 1, 0, 2, 0)
        );
        assert_eq!(s.scripts_changed, vec!["n.yml"]);
        assert!(s.auth_changed.is_empty());
    }
}
```

Run: `cargo test -j4 -p rocket-app sync_checks`
Expected: FAIL to compile, the functions do not exist yet.

- [ ] **Step 5: Write the pure rules.** In `sync_checks.rs`, with these bodies where the signature does not decide:

```rust
/// A hash for path probes; `Manifest::insert` only checks its shape.
const PROBE_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

pub(super) fn path_allowed(path: &str) -> bool {
    if Manifest::new().insert(path, PROBE_HASH).is_err() {
        return false;
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    let nested = path.contains('/');
    let collection_file = name.eq_ignore_ascii_case("opencollection.yml") || name.eq_ignore_ascii_case("bruno.json");
    !(nested && collection_file)
}

pub(super) fn keep_both_allowed(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    path != "opencollection.yml" && !name.eq_ignore_ascii_case("folder.yml")
}
```
(`Manifest::insert` needs `&mut self`; use a `let mut probe = Manifest::new();`.)

- `check_request`: `BadPlan` unless `deletes`, `renames`, `decisions` and `sends` are empty and `new_base_hash` is `None`; `BadPlan` for an empty or repeated `wanted`; `Unsafe` for a path that fails `path_allowed` or `offered.get(path)` is `None`.
- `check_final`, in this order: `BadPlan` unless `wanted` is empty and `new_base_hash` is `Some`; every path of `deletes`, `renames` (both ends), `sends` and `decisions` passes `path_allowed` (`Unsafe`); every `sends` hash is 64 lowercase hex (`BadPlan`); repeated `deletes`, rename sources, rename targets or `sends` paths are `Unsafe`; a `sends` path that is also deleted is `Unsafe`; then `starter_new_base` (its `Unsafe`); then the hash compare (`BadPlan`).
- `starter_new_base`: copy `offered` into a `BTreeMap<String, String>`; remove each delete (`Unsafe` when absent); take each rename source out (`Unsafe` when absent), then insert each target with its source's hash (`Unsafe` when the target is present); insert each send's hash; build a `Manifest` with `insert` (`Unsafe` on a refusal; above `MAX_MANIFEST_ENTRIES` is `BadPlan`).
- `split_actions`: the mapping is `Pull` -> `puts`; `PullAs { path, as_path }` -> `put_as (path, as_path)`, `peer_renames (path, as_path)` and, when `mine` has `path`, `sends path`; `Push` -> `sends`; `DeleteMine` -> `deletes`; `DeleteTheirs` -> `peer_deletes`; `RenameMine` -> `renames`; `RenameTheirs` -> `peer_renames`. Every list is sorted and has no repeats.
- `wanted_paths`: the `Pull` and `PullAs` paths plus the path of every conflict whose `theirs` is `Some`.
- `apply_to_manifest`: deletes, then renames, then `puts` (the hash is `theirs[path]`), then `put_as` (the hash of `theirs[peer path]` at the local path); any missing source is `Internal`.
- `entries_match`: the key sets differ -> `Unsafe`; a hash differs -> `Damaged`.
- `file_marks`: parse with `serde_yaml::from_slice::<serde_yaml::Value>`; an error gives both marks. Walk every mapping at any depth: a key `script`, `scripts` or `tests` whose value is not null, an empty string, an empty list or an empty mapping sets `script`; a key `auth` whose value is not null, not the string `inherit` or `none`, and not a mapping whose `type` or `mode` is `inherit` or `none`, sets `auth`.
- `planner_summary`: per action: `Pull` (changed when `mine` has the path, else added), `PullAs` (added), `DeleteMine` (deleted), `RenameMine` (changed) count "here"; `Push` (changed when `theirs` has the path, else added), `DeleteTheirs` (deleted), `RenameTheirs` (changed) count "sent". `scripts_changed` and `auth_changed` list, sorted, the `received` paths whose marks are set.
- `starter_summary`: `sends` (changed when `offered` has the path, else added), `deletes` (deleted) and `renames` (changed) count "here"; `sent_changed` is `wanted`, the other two sent counts are 0; the marks as above.

Run: `cargo test -j4 -p rocket-app sync_checks`
Expected: PASS, 18 tests (3 from Task 1, 15 here).

- [ ] **Step 6: Add the flow helpers to `sync_fakes.rs`.** Append:

```rust
// ----- flow helpers shared by the Task 2 and Task 3 tests -----

/// The link every flow test uses.
pub(crate) const L: u8 = 1;

pub(crate) fn base() -> Files {
    files(&[
        ("opencollection.yml", "name: orders\n"),
        ("a.yml", "url: a\n"),
        ("b.yml", "url: b\n"),
    ])
}

pub(crate) fn with(f: &Files, path: &str, text: &str) -> Files {
    let mut f = f.clone();
    f.insert(path.to_string(), text.as_bytes().to_vec());
    f
}

pub(crate) fn without(f: &Files, path: &str) -> Files {
    let mut f = f.clone();
    f.remove(path);
    f
}

pub(crate) fn bh() -> String {
    manifest_of(&base()).base_hash()
}

pub(crate) fn empty_plan() -> rocket_network::SyncPlanPayload {
    rocket_network::SyncPlanPayload {
        wanted: vec![],
        deletes: vec![],
        renames: vec![],
        decisions: vec![],
        sends: vec![],
        new_base_hash: None,
    }
}

pub(crate) fn request(wanted: &[&str]) -> rocket_network::SyncPlanPayload {
    let mut p = empty_plan();
    p.wanted = wanted.iter().map(|s| s.to_string()).collect();
    p
}

/// A final plan for a starter that offered `offered`, and the files the starter has after it.
/// The new base hash is the one the starter must reach.
pub(crate) fn final_plan(
    offered: &Files,
    deletes: &[&str],
    renames: &[(&str, &str)],
    sends: &[(&str, &str)],
) -> (rocket_network::SyncPlanPayload, Files) {
    let mut p = empty_plan();
    p.deletes = deletes.iter().map(|s| s.to_string()).collect();
    p.renames = renames
        .iter()
        .map(|(from, to)| rocket_network::PlanRename {
            from: from.to_string(),
            to: to.to_string(),
        })
        .collect();
    p.sends = sends
        .iter()
        .map(|(path, text)| rocket_network::PlanFile {
            path: path.to_string(),
            sha256: sha(text.as_bytes()),
        })
        .collect();
    let mut after = offered.clone();
    for d in deletes {
        after.remove(*d);
    }
    for (from, to) in renames {
        if let Some(bytes) = after.remove(*from) {
            after.insert(to.to_string(), bytes);
        }
    }
    for (path, text) in sends {
        after.insert(path.to_string(), text.as_bytes().to_vec());
    }
    p.new_base_hash = Some(manifest_of(&after).base_hash());
    (p, after)
}

/// Bo starts the sync of link `L`; the scripted Ana takes the manifest.
pub(crate) async fn bo_starts(rig: &SyncRig) -> (rocket_network::SyncId, Offered, Vec<u8>) {
    let sid = rig.sync.start(&lid(L)).await.expect("started");
    let (offer, bytes) = take_payload(rig, ANA, SyncPayloadKind::Manifest, 0).await;
    (sid, offer, bytes)
}

pub(crate) async fn ana_sends_plan(rig: &SyncRig, sid: &str, n: u8, plan: &rocket_network::SyncPlanPayload) {
    let bytes = plan.to_json().expect("json");
    let offer = offered(sid, &lid(L), &bh(), SyncPayloadKind::Plan, &ptid(n), &bytes);
    give_payload(rig, ANA, &offer, &bytes).await;
}

pub(crate) async fn ana_sends_files(rig: &SyncRig, sid: &str, n: u8, zip: &[u8]) {
    let offer = offered(sid, &lid(L), &bh(), SyncPayloadKind::Files, &ptid(n), zip);
    give_payload(rig, ANA, &offer, zip).await;
}

/// The scripted Ana starts a sync with Bo as the planner. Returns the sync id.
pub(crate) async fn ana_starts(rig: &SyncRig, theirs: &Files) -> rocket_network::SyncId {
    let sid = rocket_network::SyncId::random();
    let bytes = manifest_of(theirs).to_json().expect("json");
    let offer = offered(sid.as_str(), &lid(L), &bh(), SyncPayloadKind::Manifest, &ptid(1), &bytes);
    give_payload(rig, ANA, &offer, &bytes).await;
    sid
}

pub(crate) async fn wait_finished(rig: &SyncRig) -> (SyncFinish, Option<String>) {
    wait_for("the finished event", || !rig.events.finished().is_empty()).await;
    let (_, finish, message) = rig.events.finished().remove(0);
    (finish, message)
}

pub(crate) async fn wait_phase(rig: &SyncRig, sid: &rocket_network::SyncId, phase: super::sync::Phase) {
    wait_for("the phase", || rig.sync.phase_of(sid) == Some(phase)).await;
}

pub(crate) async fn wait_link(rig: &SyncRig, check: impl Fn(&LinkRecord) -> bool) -> LinkRecord {
    wait_for("the link update", || rig.links.record(&lid(L)).is_some_and(|r| check(&r))).await;
    rig.links.record(&lid(L)).expect("the link")
}
```

- [ ] **Step 7: Write the failing run tests.** Create `crates/rocket-app/src/network/sync_run_tests.rs`:

```rust
//! Task 2 tests: one sync run row by row (the scripted peer plays the other node), then over
//! a relay between two real nodes.

use rocket_audit::AuditEventKind;
use rocket_network::{
    ConflictKind, Decision, LinkState, Manifest, PeerMessage, PlanFile, PlanRename, Resolution,
    SyncEvent, SyncFinish, SyncOutcome, SyncPayloadKind, SyncPlanPayload, SyncRole,
};
use rocket_shared::error::DomainError;

use super::sync::{ConflictFiles, Phase};
use super::sync_checks::{ALREADY_IN_SYNC, FOLDER_CHANGED, RESULT_NOT_SENT, UNCONFIRMED};
use super::sync_fakes::*;
use super::sync_payload::unpack_zip;
use super::sync_ports::FileChange;

fn put(path: &str, text: &str) -> FileChange {
    FileChange::Put {
        path: path.to_string(),
        bytes: text.as_bytes().to_vec(),
    }
}

// ----- starter, row by row -----

#[tokio::test]
async fn start_offers_the_manifest_with_the_base_hash() {
    let rig = sync_rig();
    let mine = with(&base(), "a.yml", "url: a2\n");
    rig.linked(&lid(L), &base(), &mine);

    let (sid, offer, bytes) = bo_starts(&rig).await;

    assert_eq!(offer.sync_id, sid.as_str());
    assert_eq!(offer.link_id, lid(L).as_str());
    assert_eq!(offer.base_hash, bh());
    assert_eq!(Manifest::from_json(&bytes).expect("a manifest"), manifest_of(&mine));
    wait_phase(&rig, &sid, Phase::AwaitPlan).await;
    assert!(rig.events.all().is_empty(), "no dialog on the starter yet");
    assert!(rig.colls.applied().is_empty());
}

async fn assert_refused(rig: &SyncRig, text: &str) {
    let err = rig.sync.start(&lid(L)).await.expect_err("refused");
    assert!(
        matches!(&err, DomainError::InvalidInput(m) if m == text),
        "{err:?} is not {text}"
    );
}

#[tokio::test]
async fn start_refuses_with_the_reason_and_sends_nothing() {
    let rig = sync_rig();
    assert_refused(&rig, "That link does not exist.").await;

    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let mut broken = rig.links.record(&lid(L)).expect("link");
    broken.state = LinkState::Broken;
    rig.links.save(&broken, &[]).expect("saved");
    assert_refused(&rig, "This link is broken. Unlink and send the collection again.").await;

    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    rig.peers.forget(&dev(ANA));
    assert_refused(&rig, "That node is not paired.").await;

    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    rig.peers.set_online(&dev(ANA), false);
    assert_refused(&rig, "That node is offline.").await;

    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    rig.colls.set_uid(ORDERS, Some("another"));
    assert_refused(&rig, FOLDER_CHANGED).await;
    wait_link(&rig, |r| r.state == LinkState::Broken).await;
    assert!(rig.sent(ANA).is_empty(), "no refused start sends a frame");

    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    rig.sync.start(&lid(L)).await.expect("the first");
    assert_refused(&rig, "A sync with that node is already running.").await;
    let offers = rig.sent(ANA).iter().filter_map(as_offer).count();
    assert_eq!(offers, 1, "only the first start offered");
    assert!(rig.audited().is_empty(), "a refused start writes no audit");
}

#[tokio::test]
async fn a_request_plan_is_answered_with_the_wanted_files() {
    let rig = sync_rig();
    let mine = with(&base(), "a.yml", "url: a2\n");
    rig.linked(&lid(L), &base(), &mine);
    let (sid, ..) = bo_starts(&rig).await;

    ana_sends_plan(&rig, sid.as_str(), 2, &request(&["a.yml"])).await;
    let (offer, zip) = take_payload(&rig, ANA, SyncPayloadKind::Files, 0).await;

    assert_eq!(offer.sync_id, sid.as_str());
    assert_eq!(offer.base_hash, bh());
    assert_eq!(unpack_zip(&zip).expect("a valid zip"), files(&[("a.yml", "url: a2\n")]));
    wait_phase(&rig, &sid, Phase::AwaitPlan).await;
    assert!(rig.events.all().iter().any(
        |e| matches!(e, SyncEvent::Progress { sync_id, .. } if sync_id == sid.as_str())
    ));
    assert!(rig.colls.applied().is_empty());
}

#[tokio::test]
async fn a_final_plan_with_files_asks_for_confirmation() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    let sends = [("b.yml", "url: b2\n"), ("n.yml", "url: n\n")];
    let (plan, _) = final_plan(&base(), &[], &[], &sends);

    ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;
    wait_phase(&rig, &sid, Phase::AwaitFiles).await;
    assert!(rig.events.confirms().is_empty(), "not before the files");
    ana_sends_files(&rig, sid.as_str(), 3, &zip_of(&files(&sends))).await;

    wait_for("the confirm", || rig.events.confirms().len() == 1).await;
    match &rig.events.confirms()[0] {
        SyncEvent::ConfirmNeeded {
            sync_id,
            link_id,
            device_id,
            short_id,
            display_name,
            collection_name,
            summary,
        } => {
            assert_eq!(sync_id, sid.as_str());
            assert_eq!(link_id, lid(L).as_str());
            assert_eq!(device_id, &dev(ANA).to_base32());
            assert_eq!(short_id, &dev(ANA).short());
            assert_eq!(display_name, "Ana");
            assert_eq!(collection_name, ORDERS);
            assert_eq!((summary.added_here, summary.changed_here, summary.deleted_here), (1, 1, 0));
        }
        _ => unreachable!("filtered by confirms()"),
    }
    assert_eq!(rig.sync.phase_of(&sid), Some(Phase::AwaitConfirm));
    assert!(rig.colls.applied().is_empty(), "nothing is written before the confirm");
}

#[tokio::test]
async fn a_final_plan_with_only_deletes_asks_for_confirmation() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    let (plan, _) = final_plan(&base(), &["b.yml"], &[], &[]);

    ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;

    wait_for("the confirm", || rig.events.confirms().len() == 1).await;
    match &rig.events.confirms()[0] {
        SyncEvent::ConfirmNeeded { summary, .. } => assert_eq!(summary.deleted_here, 1),
        _ => unreachable!("filtered by confirms()"),
    }
    assert!(
        rig.sent(ANA).iter().filter_map(as_offer).all(|o| o.kind == SyncPayloadKind::Manifest),
        "the starter sent no files"
    );
}

/// Bo starts, Ana answers with a final plan that sends b2 and n, and Bo confirms.
async fn starter_confirmed(rig: &SyncRig) -> (rocket_network::SyncId, Files) {
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(rig).await;
    let sends = [("b.yml", "url: b2\n"), ("n.yml", "url: n\n")];
    let (plan, after) = final_plan(&base(), &[], &[], &sends);
    ana_sends_plan(rig, sid.as_str(), 2, &plan).await;
    ana_sends_files(rig, sid.as_str(), 3, &zip_of(&files(&sends))).await;
    wait_phase(rig, &sid, Phase::AwaitConfirm).await;
    rig.sync.confirm(&sid, true).await.expect("confirmed");
    (sid, after)
}

#[tokio::test]
async fn the_starter_applies_then_reports_and_stores_the_base_on_applied() {
    let rig = sync_rig();
    let (sid, after) = starter_confirmed(&rig).await;

    wait_for("our result", || rig.results(ANA) == vec![SyncOutcome::Applied]).await;
    let applied = rig.colls.applied();
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0].0, ORDERS);
    assert_eq!(applied[0].1, vec![put("b.yml", "url: b2\n"), put("n.yml", "url: n\n")]);
    assert_eq!(rig.sync.phase_of(&sid), Some(Phase::AwaitResult));
    assert_eq!(
        rig.links.record(&lid(L)).expect("link").base,
        manifest_of(&base()),
        "the base waits for the peer's result"
    );

    rig.push(
        ANA,
        PeerMessage::SyncResult {
            sync_id: sid.as_str().to_string(),
            outcome: SyncOutcome::Applied,
        },
    );

    assert_eq!(wait_finished(&rig).await, (SyncFinish::Applied, None));
    let link = wait_link(&rig, |r| r.base == manifest_of(&after)).await;
    assert_eq!(link.state, LinkState::InSync);
    assert_eq!(rig.links.base_files(&lid(L)), after.clone().into_iter().collect::<Vec<_>>());
    assert_eq!(rig.colls.files(ORDERS), after);
    assert_eq!(
        rig.audited(),
        vec![AuditEventKind::NetworkSyncSent {
            device_id: dev(ANA).short(),
            collection: ORDERS.to_string(),
            outcome: "applied".to_string(),
        }]
    );
    wait_for("link changed", || rig.events.link_changes() == 1).await;
    assert_eq!(rig.sync.phase_of(&sid), None, "the session is gone");
}

#[tokio::test]
async fn a_peer_gone_after_our_result_is_unconfirmed_with_the_new_base() {
    let rig = sync_rig();
    let (_sid, after) = starter_confirmed(&rig).await;
    wait_for("our result", || rig.results(ANA) == vec![SyncOutcome::Applied]).await;

    rig.router.on_peer_gone(dev(ANA));

    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Unconfirmed, Some(UNCONFIRMED.to_string()))
    );
    let link = wait_link(&rig, |r| r.base == manifest_of(&after)).await;
    assert_eq!(link.state, LinkState::OutOfDate);
    assert_eq!(rig.links.count(), 1, "still paired, so the link stays");
    assert_eq!(
        rig.audited(),
        vec![AuditEventKind::NetworkSyncSent {
            device_id: dev(ANA).short(),
            collection: ORDERS.to_string(),
            outcome: "unconfirmed".to_string(),
        }]
    );
}

#[tokio::test]
async fn no_result_within_the_timeout_is_unconfirmed() {
    let rig = sync_rig();
    let (_sid, after) = starter_confirmed(&rig).await;

    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Unconfirmed, Some(UNCONFIRMED.to_string()))
    );
    let link = wait_link(&rig, |r| r.base == manifest_of(&after)).await;
    assert_eq!(link.state, LinkState::OutOfDate);
}

#[tokio::test]
async fn a_result_that_cannot_be_sent_keeps_the_old_base() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    let (plan, _) = final_plan(&base(), &["b.yml"], &[], &[]);
    ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;
    wait_phase(&rig, &sid, Phase::AwaitConfirm).await;
    rig.peers.set_online(&dev(ANA), false);

    rig.sync.confirm(&sid, true).await.expect("confirmed");

    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Unconfirmed, Some(RESULT_NOT_SENT.to_string()))
    );
    assert_eq!(rig.colls.applied().len(), 1, "this side applied");
    let link = wait_link(&rig, |r| r.state == LinkState::OutOfDate).await;
    assert_eq!(link.base, manifest_of(&base()), "the planner surely did not apply");
}

// ----- planner, row by row -----

#[tokio::test]
async fn a_matching_manifest_is_planned_and_reviewed() {
    let rig = sync_rig();
    let mine = with(&base(), "b.yml", "url: b-bo\n");
    rig.linked(&lid(L), &base(), &mine);
    let theirs = with(&with(&base(), "a.yml", "url: a-ana\n"), "c.yml", "url: c\n");

    let sid = ana_starts(&rig, &theirs).await;
    let (offer, bytes) = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
    let req = SyncPlanPayload::from_json(&bytes).expect("a plan");
    assert_eq!(offer.sync_id, sid.as_str());
    assert_eq!(offer.base_hash, bh());
    assert_eq!(req.wanted, vec!["a.yml".to_string(), "c.yml".to_string()]);
    assert!(req.new_base_hash.is_none() && req.deletes.is_empty() && req.sends.is_empty());
    assert!(rig.events.reviews().is_empty(), "not before the files");

    let incoming = files(&[("a.yml", "url: a-ana\n"), ("c.yml", "url: c\n")]);
    ana_sends_files(&rig, sid.as_str(), 2, &zip_of(&incoming)).await;

    wait_for("the review", || rig.events.reviews().len() == 1).await;
    match &rig.events.reviews()[0] {
        SyncEvent::ReviewReady {
            sync_id,
            link_id,
            device_id,
            display_name,
            collection_name,
            summary,
            conflicts,
            role,
            ..
        } => {
            assert_eq!(sync_id, sid.as_str());
            assert_eq!(link_id, lid(L).as_str());
            assert_eq!(device_id, &dev(ANA).to_base32());
            assert_eq!(display_name, "Ana");
            assert_eq!(collection_name, ORDERS);
            assert_eq!(*role, SyncRole::Planner);
            assert!(conflicts.is_empty());
            assert_eq!(
                (summary.added_here, summary.changed_here, summary.deleted_here),
                (1, 1, 0)
            );
            assert_eq!(
                (summary.sent_added, summary.sent_changed, summary.sent_deleted),
                (0, 1, 0)
            );
        }
        _ => unreachable!("filtered by reviews()"),
    }
    assert!(rig.colls.applied().is_empty());

    rig.sync.review(&sid, true, vec![]).await.expect("accepted");
    let (_, fin) = take_payload(&rig, ANA, SyncPayloadKind::Plan, 1).await;
    let fin = SyncPlanPayload::from_json(&fin).expect("the final plan");
    assert_eq!(
        fin.sends,
        vec![PlanFile {
            path: "b.yml".into(),
            sha256: sha(b"url: b-bo\n"),
        }]
    );
    assert!(fin.wanted.is_empty() && fin.deletes.is_empty() && fin.renames.is_empty());
    let after = with(&with(&mine, "a.yml", "url: a-ana\n"), "c.yml", "url: c\n");
    assert_eq!(fin.new_base_hash, Some(manifest_of(&after).base_hash()));
    assert!(rig.colls.applied().is_empty(), "the planner waits for the starter");
}

#[tokio::test]
async fn a_delete_only_plan_is_reviewed_without_a_request() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());

    let sid = ana_starts(&rig, &without(&base(), "a.yml")).await;

    wait_for("the review", || rig.events.reviews().len() == 1).await;
    match &rig.events.reviews()[0] {
        SyncEvent::ReviewReady { summary, .. } => assert_eq!(summary.deleted_here, 1),
        _ => unreachable!("filtered by reviews()"),
    }
    assert!(
        rig.sent(ANA).iter().filter_map(as_offer).next().is_none(),
        "nothing was requested"
    );
    assert_eq!(rig.sync.phase_of(&sid), Some(Phase::AwaitReview));
}

/// Bo (planner) renamed b.yml to b2.yml and added n.yml; Ana deleted a.yml. Runs to the
/// state where Bo waits for the starter. Returns the sync id and Bo's files after the sync.
async fn planner_waiting(rig: &SyncRig) -> (rocket_network::SyncId, Files) {
    let mut mine = without(&base(), "b.yml");
    mine.insert("b2.yml".into(), b"url: b\n".to_vec());
    let mine = with(&mine, "n.yml", "url: n\n");
    rig.linked(&lid(L), &base(), &mine);
    let sid = ana_starts(rig, &without(&base(), "a.yml")).await;
    wait_phase(rig, &sid, Phase::AwaitReview).await;
    rig.sync.review(&sid, true, vec![]).await.expect("accepted");
    (sid, without(&mine, "a.yml"))
}

#[tokio::test]
async fn the_final_plan_lists_the_peer_changes_and_the_new_base_hash() {
    let rig = sync_rig();
    let (sid, after) = planner_waiting(&rig).await;

    let (_, fin) = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
    let fin = SyncPlanPayload::from_json(&fin).expect("the final plan");
    assert_eq!(
        fin.renames,
        vec![PlanRename {
            from: "b.yml".into(),
            to: "b2.yml".into(),
        }],
        "a rename by hash travels as a rename"
    );
    assert_eq!(
        fin.sends,
        vec![PlanFile {
            path: "n.yml".into(),
            sha256: sha(b"url: n\n"),
        }]
    );
    assert!(fin.deletes.is_empty());
    assert_eq!(fin.new_base_hash, Some(manifest_of(&after).base_hash()));

    let (_, zip) = take_payload(&rig, ANA, SyncPayloadKind::Files, 0).await;
    assert_eq!(unpack_zip(&zip).expect("a zip"), files(&[("n.yml", "url: n\n")]));
    wait_phase(&rig, &sid, Phase::AwaitStarter).await;
    assert!(rig.colls.applied().is_empty());
}

#[tokio::test]
async fn the_planner_applies_only_after_the_starter_applied() {
    let rig = sync_rig();
    let (sid, after) = planner_waiting(&rig).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Files, 0).await;
    wait_phase(&rig, &sid, Phase::AwaitStarter).await;
    assert!(rig.colls.applied().is_empty());

    rig.push(
        ANA,
        PeerMessage::SyncResult {
            sync_id: sid.as_str().to_string(),
            outcome: SyncOutcome::Applied,
        },
    );

    wait_for("our result", || rig.results(ANA) == vec![SyncOutcome::Applied]).await;
    assert_eq!(
        rig.colls.applied()[0].1,
        vec![FileChange::Delete { path: "a.yml".into() }]
    );
    assert_eq!(wait_finished(&rig).await, (SyncFinish::Applied, None));
    let link = wait_link(&rig, |r| r.base == manifest_of(&after)).await;
    assert_eq!(link.state, LinkState::InSync);
    assert_eq!(
        rig.audited(),
        vec![AuditEventKind::NetworkSyncReceived {
            device_id: dev(ANA).short(),
            collection: ORDERS.to_string(),
            outcome: "applied".to_string(),
        }]
    );
}

#[tokio::test]
async fn a_peer_gone_during_the_planner_apply_does_not_stop_it() {
    let rig = sync_rig();
    let (sid, after) = planner_waiting(&rig).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Files, 0).await;
    wait_phase(&rig, &sid, Phase::AwaitStarter).await;
    let gate = rig.colls.hold_apply();
    rig.push(
        ANA,
        PeerMessage::SyncResult {
            sync_id: sid.as_str().to_string(),
            outcome: SyncOutcome::Applied,
        },
    );
    wait_phase(&rig, &sid, Phase::Applying).await;

    rig.router.on_peer_gone(dev(ANA));
    drop(gate);

    assert_eq!(wait_finished(&rig).await, (SyncFinish::Applied, None));
    assert_eq!(rig.colls.files(ORDERS), after);
    wait_link(&rig, |r| r.base == manifest_of(&after)).await;
}

// ----- two nodes -----

/// After the review: confirms on the starter when it asks (a plan that changes nothing
/// there skips the confirm), then runs until both sides finished.
async fn confirm_if_asked(pair: &mut SyncPair, sid: &rocket_network::SyncId) {
    pair.run_until(|p| p.a.events.confirms().len() == 1 || !p.a.events.finished().is_empty())
        .await;
    if pair.a.events.confirms().len() == 1 {
        assert!(
            pair.a.colls.applied().is_empty() && pair.b.colls.applied().is_empty(),
            "nothing is written before the confirm"
        );
        pair.a.sync.confirm(sid, true).await.expect("confirmed");
    }
    pair.run_until(|p| !p.a.events.finished().is_empty() && !p.b.events.finished().is_empty())
        .await;
}

/// Bo starts, Ana reviews with `decisions`, Bo confirms when asked; runs until both finished.
async fn full_sync(pair: &mut SyncPair, decisions: Vec<Decision>) -> rocket_network::SyncId {
    let sid = pair.a.sync.start(&lid(L)).await.expect("started");
    pair.run_until(|p| p.b.events.reviews().len() == 1).await;
    assert!(pair.a.colls.applied().is_empty() && pair.b.colls.applied().is_empty());
    pair.b.sync.review(&sid, true, decisions).await.expect("reviewed");
    confirm_if_asked(pair, &sid).await;
    sid
}

async fn both_bases(pair: &mut SyncPair, a_files: &Files, b_files: &Files) {
    let (ma, mb) = (manifest_of(a_files), manifest_of(b_files));
    pair.run_until(|p| {
        p.a.links.record(&lid(L)).is_some_and(|r| r.base == ma)
            && p.b.links.record(&lid(L)).is_some_and(|r| r.base == mb)
    })
    .await;
}

#[tokio::test]
async fn a_two_way_sync_merges_both_edits_and_updates_both_bases() {
    let bo = with(&base(), "a.yml", "url: a-bo\n");
    let ana = with(&base(), "b.yml", "url: b-ana\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &ana);

    let sid = full_sync(&mut pair, vec![]).await;

    let both = with(&bo, "b.yml", "url: b-ana\n");
    assert_eq!(pair.a.colls.files(ORDERS), both);
    assert_eq!(pair.b.colls.files(ORDERS), both);
    assert_eq!(pair.a.events.finished(), vec![(sid.as_str().to_string(), SyncFinish::Applied, None)]);
    assert_eq!(pair.b.events.finished(), vec![(sid.as_str().to_string(), SyncFinish::Applied, None)]);
    both_bases(&mut pair, &both, &both).await;
    let (a, b) = (
        pair.a.links.record(&lid(L)).expect("a"),
        pair.b.links.record(&lid(L)).expect("b"),
    );
    assert_eq!(a.base_hash(), b.base_hash());
    assert_eq!((a.state, b.state), (LinkState::InSync, LinkState::InSync));
    assert_eq!(
        pair.a.audited(),
        vec![AuditEventKind::NetworkSyncSent {
            device_id: dev(ANA).short(),
            collection: ORDERS.into(),
            outcome: "applied".into(),
        }]
    );
    assert_eq!(
        pair.b.audited(),
        vec![AuditEventKind::NetworkSyncReceived {
            device_id: dev(BO).short(),
            collection: ORDERS.into(),
            outcome: "applied".into(),
        }]
    );
}

#[tokio::test]
async fn the_stored_base_is_computed_locally_on_both_sides() {
    let bo = with(&base(), "a.yml", "url: a-bo\n");
    let ana = with(&base(), "b.yml", "url: b-ana\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &ana);
    full_sync(&mut pair, vec![]).await;
    let (a_files, b_files) = (pair.a.colls.files(ORDERS), pair.b.colls.files(ORDERS));

    both_bases(&mut pair, &a_files, &b_files).await;

    assert_eq!(
        pair.a.links.base_files(&lid(L)),
        a_files.into_iter().collect::<Vec<_>>(),
        "every base file is a byte copy of the agreed file"
    );
    assert_eq!(pair.b.links.base_files(&lid(L)), b_files.into_iter().collect::<Vec<_>>());
}

#[tokio::test]
async fn a_conflict_resolves_with_each_choice() {
    let bo = with(&base(), "a.yml", "url: a-bo\n");
    let ana = with(&base(), "a.yml", "url: a-ana\n");
    let cases = [
        (Resolution::KeepMine, with(&base(), "a.yml", "url: a-ana\n")),
        (Resolution::KeepTheirs, with(&base(), "a.yml", "url: a-bo\n")),
        (
            Resolution::KeepBoth,
            with(&with(&base(), "a.yml", "url: a-ana\n"), "a (from peer).yml", "url: a-bo\n"),
        ),
    ];
    for (resolution, expected) in cases {
        let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &ana);
        let sid = pair.a.sync.start(&lid(L)).await.expect("started");
        pair.run_until(|p| p.b.events.reviews().len() == 1).await;
        match &pair.b.events.reviews()[0] {
            SyncEvent::ReviewReady { conflicts, .. } => {
                assert_eq!(conflicts.len(), 1);
                assert_eq!(conflicts[0].path, "a.yml");
                assert_eq!(conflicts[0].kind, ConflictKind::BothChanged);
                assert_eq!(conflicts[0].mine_hash, Some(sha(b"url: a-ana\n")));
                assert_eq!(conflicts[0].theirs_hash, Some(sha(b"url: a-bo\n")));
            }
            _ => unreachable!("filtered by reviews()"),
        }
        assert_eq!(
            pair.b.sync.conflict_files(&sid, "a.yml").expect("both texts"),
            ConflictFiles {
                mine: Some("url: a-ana\n".into()),
                theirs: Some("url: a-bo\n".into()),
            }
        );
        pair.b
            .sync
            .review(&sid, true, vec![Decision { path: "a.yml".into(), resolution }])
            .await
            .expect("decided");
        confirm_if_asked(&mut pair, &sid).await;

        assert_eq!(pair.a.colls.files(ORDERS), expected, "{resolution:?} on the starter");
        assert_eq!(pair.b.colls.files(ORDERS), expected, "{resolution:?} on the planner");
        both_bases(&mut pair, &expected, &expected).await;
    }
}

#[tokio::test]
async fn a_rename_travels_as_a_rename() {
    let mut bo = without(&base(), "b.yml");
    bo.insert("f/c.yml".into(), b"url: b\n".to_vec());
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &base());

    full_sync(&mut pair, vec![]).await;

    assert_eq!(
        pair.b.colls.applied()[0].1,
        vec![FileChange::Rename { from: "b.yml".into(), to: "f/c.yml".into() }],
        "not a delete plus an add"
    );
    assert_eq!(pair.b.colls.files(ORDERS), bo);
}

#[tokio::test]
async fn a_delete_of_an_untouched_file_is_taken() {
    let bo = without(&base(), "a.yml");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &base());

    full_sync(&mut pair, vec![]).await;

    assert_eq!(pair.b.colls.applied()[0].1, vec![FileChange::Delete { path: "a.yml".into() }]);
    assert_eq!(pair.b.colls.files(ORDERS), bo);
}

#[tokio::test]
async fn a_delete_against_an_edit_is_a_conflict() {
    let bo = without(&base(), "a.yml");
    let ana = with(&base(), "a.yml", "url: a-ana\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &ana);
    pair.a.sync.start(&lid(L)).await.expect("started");

    pair.run_until(|p| p.b.events.reviews().len() == 1).await;

    match &pair.b.events.reviews()[0] {
        SyncEvent::ReviewReady { conflicts, .. } => {
            assert_eq!(conflicts.len(), 1);
            assert_eq!(conflicts[0].kind, ConflictKind::DeleteVsEdit);
        }
        _ => unreachable!("filtered by reviews()"),
    }
}

#[tokio::test]
async fn an_empty_sync_ends_applied_without_dialogs() {
    let mut pair = SyncPair::linked(&lid(L), &base(), &base(), &base());
    let sid = pair.a.sync.start(&lid(L)).await.expect("started");

    pair.run_until(|p| !p.a.events.finished().is_empty() && !p.b.events.finished().is_empty())
        .await;

    assert_eq!(
        pair.a.events.finished(),
        vec![(sid.as_str().to_string(), SyncFinish::Applied, None)]
    );
    assert_eq!(
        pair.b.events.finished(),
        vec![(sid.as_str().to_string(), SyncFinish::Applied, Some(ALREADY_IN_SYNC.to_string()))]
    );
    assert!(pair.a.events.reviews().is_empty() && pair.b.events.reviews().is_empty());
    assert!(pair.a.events.confirms().is_empty());
    assert!(pair.a.colls.applied().is_empty() && pair.b.colls.applied().is_empty());
    both_bases(&mut pair, &base(), &base()).await;
}
```
Register the module in `mod.rs`: `#[cfg(test)] #[path = "sync_run_tests.rs"] mod sync_run_tests;`.

Run: `cargo test -j4 -p rocket-app sync_run_tests`
Expected: FAIL to compile (`start`, `review`, `confirm`, `phase_of`, `conflict_files` do not exist).

- [ ] **Step 8: The payload half of `sync_payload.rs`.**

```rust
pub(super) fn kind_name(kind: SyncPayloadKind) -> &'static str;   // "manifest", "plan", "files"

/// One payload this node sends.
pub(super) struct PayloadOut {
    pub kind: SyncPayloadKind,
    pub machine: OutgoingTransfer,
    pub bytes: Vec<u8>,
    pub last_ack: Instant,          // the offer, the answer or the last good ack
    pub ack_signal: Arc<Notify>,
    pub acked: u32,
}

/// One payload this node receives. The bytes stay in memory (S3).
pub(super) struct PayloadIn {
    pub kind: SyncPayloadKind,
    pub machine: IncomingTransfer,
    pub bytes: Vec<u8>,
    pub last_activity: Instant,
    pub acks: Option<mpsc::UnboundedSender<PeerMessage>>,
}

/// Hashes the bytes and builds the machine. `TooBig` and `Empty` come back as the machine's
/// `OfferError`; the caller turns `TooBig` into `Stop::TooBig(len)`.
pub(super) fn build_out(kind: SyncPayloadKind, bytes: Vec<u8>, now: Instant, ttl: Duration) -> Result<PayloadOut, OfferError>;

/// The `SyncOffer` for a payload, from the machine's own offer fields.
pub(super) fn sync_offer(out: &PayloadOut, sync_id: &SyncId, link_id: &LinkId, base_hash: &str) -> PeerMessage {
    match out.machine.offer_message() {
        PeerMessage::CollectionOffer { transfer_id, size_bytes, chunk_count, sha256, .. } => PeerMessage::SyncOffer {
            sync_id: sync_id.as_str().to_string(),
            link_id: link_id.as_str().to_string(),
            base_hash: base_hash.to_string(),
            kind: out.kind,
            transfer_id, size_bytes, chunk_count, sha256,
        },
        // `offer_message` only builds a CollectionOffer; this arm never runs.
        other => other,
    }
}
```
On `SyncService` (in `sync_payload.rs`, an `impl SyncService` block):
- `offer_payload(&self, sync_id, kind, bytes) -> Result<(), Stop>`: `build_out` (in the caller's blocking task); under the lock store `out` on the session, insert the id into `transfers`, set the phase (`SendManifest`, `SendRequest`, `SendFinal` or `SendFiles`); then `tell_later` the `SyncOffer`. A failed send of the offer ends the session `WentOffline` (`end_session` from the spawned task).
- `on_payload_answer(from, id, accept)`: only for the session's current `out` and its peer; `accept` starts `spawn_payload_stream(sync_id)`; `false` ends the session `BadPlan` (a sync payload is never declined).
- `spawn_payload_stream`: the loop of `TransferService::stream` with these differences: the entry is `session.out`; the first chunk sets `session.chunk_out = true`; `Progress` is published for `Files` only; after the last chunk it waits for the final ack under the idle watchdog.
- `on_payload_ack(from, id, next_seq)`: `machine.on_ack`; a bad ack ends the session `Damaged`; a good one refreshes `last_ack`, wakes the stream, and when `machine.awaiting_result()` the payload is done: `out = None`, the id leaves `transfers`, and `payload_sent(sync_id, kind)` runs (Task 2 Step 9 and 10).
- `accept_payload(sync_id, offer)`: `IncomingTransfer::from_offer(.., kind_name(kind), ..)` with the ttl; a refused offer ends the session (`TooBig` -> `Stop::TooBig(size)`, else `BadPlan`); else store `inc`, insert the id into `transfers`, start the ack pump, set the phase (`RecvManifest`, `RecvPlan` or `RecvFiles`), then `tell_later(OfferAnswer { accept: true })`.
- `on_payload_chunk(from, id, seq, data)`: `machine.on_chunk`; `Failed` ends the session `Damaged`; `Data` appends, refreshes `last_activity`, queues the ack on the pump, publishes `Progress` for `Files`; on `last` the bytes are taken out, the id leaves `transfers`, and a blocking task checks `bytes.len() == total_bytes` and `sha_hex(bytes) == machine.sha256()` (`Damaged` otherwise) and calls `payload_received(sync_id, kind, bytes)`.
- `on_payload_abort(from, id, _reason)`: a `TransferAbort` for a sync payload ends the session `Peer(SyncAbortReason::Failed)`.
- A `TransferResult` for a sync payload is ignored.

- [ ] **Step 9: The session core in `sync.rs`.**

```rust
    /// The one place a session ends: removes it, frees its ids and limits entry, publishes
    /// `Finished`, audits (S13), tells the peer `abort` from its own task, and runs `link` in a
    /// blocking task that publishes `LinkChanged` when it saved. The peer stays in `settling`
    /// until that task is done.
    pub(super) fn end_session(&self, sync_id: &SyncId, finish: SyncFinish, message: Option<String>,
        abort: Option<SyncAbortReason>, link: LinkUpdate);

    /// `end_session` with the outcome, text and frame of a `Stop`.
    pub(super) fn stop(&self, sync_id: &SyncId, stop: Stop, link: LinkUpdate) {
        self.end_session(sync_id, stop.finish(), stop.message(), stop.abort(), link);
    }

    /// Ends a session that the user never saw (planner `Checking` refusals): no event, no audit.
    pub(super) fn drop_quietly(&self, sync_id: &SyncId, abort: SyncAbortReason);

    /// Moves a session to `phase`, bumps `gen`, resets `phase_since`. Returns the new gen.
    pub(super) fn set_phase(&self, sync_id: &SyncId, phase: Phase) -> Option<u64>;
```
`end_session` builds the `NetworkSyncSent` (starter) or `NetworkSyncReceived` (planner) row with `outcome: finish_name(finish)` only when `chunk_out`. The `NewBase` update: base files are `known` for each path whose hash matches, the old base file (`read_base_file`) when the hash equals the old base's hash for that path, else one `export_files` call for the rest, each kept only when its hash matches (S8); `state: None` is `in_sync` when a fresh `manifest()` equals `base`, else `changes_waiting`; the record is saved only while `peer_label(device)` is `Some` (S7, S16).

The watchdog: one task per session, spawned when the session is inserted, that loops on `deadline(&Session) -> Option<Instant>` and ends the session when it passes with the gen unchanged:

```rust
    /// When the current phase gives up. `None`: a task owns the phase and ends it itself.
    fn deadline(&self, s: &Session) -> Option<Instant> {
        let t = self.timing;
        match s.phase {
            Phase::AwaitConfirm | Phase::AwaitReview => Some(s.phase_since + t.offer_ttl),
            Phase::AwaitResult => Some(s.phase_since + t.result_timeout),
            Phase::AwaitPlan | Phase::AwaitStarter => Some(s.phase_since + t.offer_ttl + t.idle_limit),
            Phase::AwaitFiles | Phase::RecvManifest | Phase::RecvPlan | Phase::RecvFiles => {
                let last = s.inc.as_ref().map_or(s.phase_since, |i| i.last_activity.max(s.phase_since));
                Some(last + t.idle_limit)
            }
            Phase::SendManifest | Phase::SendRequest | Phase::SendFinal | Phase::SendFiles => {
                Some(s.out.as_ref().map_or(s.phase_since, |o| o.last_ack) + t.idle_limit)
            }
            Phase::Checking | Phase::Planning | Phase::Exporting | Phase::Finalizing | Phase::Applying => None,
        }
    }
```
When it passes: `AwaitConfirm`, `AwaitReview` -> `Stop::Expired`; `AwaitResult` -> `Unconfirmed` (S7, new base, `out_of_date`); every other phase -> `Stop::Stalled`. With `None` the watchdog sleeps `timing.idle_limit` and looks again; it returns when the session is gone.

`handle_frame` now dispatches: `SyncOffer { kind: Manifest }` -> `planner_on_manifest_offer`; other `SyncOffer` -> `on_follow_up_offer` (S5: the session's id, peer, link and base hash, the expected phase and kind, else `BadPlan`; unknown `sync_id` ignored); `SyncAbort` -> the session of that id and peer ends `Stop::Peer(reason)` (in `Applying` or `AwaitResult` on the starter: `PeerApplyFailed` with `State(OutOfDate)`, S14); `SyncResult` -> `starter_on_result` or `planner_on_result`; the five transfer frames -> the payload handlers of Step 8.

`handle_gone(device)` (extends Task 1): every session of that device: in `Applying` set `peer_gone`; in `AwaitResult` end `Unconfirmed` (S15); else `Stop::WentOffline` with `Keep`. Then the S16 link check.

`cancel(sync_id)`: unknown -> `Ok(())`; `answered` -> `InvalidInput("The sync is already being applied.")`; in `AwaitReview` -> `review(false)`; in `AwaitConfirm` -> `confirm(false)`; else `Stop::Cancelled`.

`conflict_files(sync_id, path)`: a planner session in `AwaitReview`, else `InvalidInput("The sync is no longer waiting for an answer.")`; `mine` from `mine_conflicts`, `theirs` from `received`, both `String::from_utf8_lossy`.

- [ ] **Step 10: The starter half, `sync_starter.rs`.**

```rust
//! The starter side of a sync: offer the manifest, answer a request with the wanted files,
//! check the final plan and the files, ask the user, apply first, report, store the base.

impl SyncService {
    /// Starts a sync of one link. Checks, in order: the link exists ("That link does not
    /// exist."), is not broken ("This link is broken. Unlink and send the collection again."),
    /// the peer is paired ("That node is not paired.") and online ("That node is offline."),
    /// no session, start or settle runs for the peer ("A sync with that node is already
    /// running."). Then, in a blocking task: the collection still exists with the link's uid
    /// and root (else the link is saved `broken` and FOLDER_CHANGED is returned), its manifest
    /// is built and offered. Returns the sync id once the offer frame was sent; a failed send
    /// removes the session without an event.
    pub async fn start(&self, link_id: &LinkId) -> DomainResult<SyncId>;

    /// The starter user's answer to `ConfirmNeeded`. `false` ends the sync `Declined`.
    /// `InvalidInput("The sync is no longer waiting for an answer.")` when not in AwaitConfirm.
    pub async fn confirm(&self, sync_id: &SyncId, accept: bool) -> DomainResult<()>;

    pub(super) fn starter_payload_sent(&self, sync_id: &SyncId, kind: SyncPayloadKind);   // -> AwaitPlan
    pub(super) fn starter_payload_received(&self, sync_id: &SyncId, kind: SyncPayloadKind, bytes: Vec<u8>);
    pub(super) fn starter_on_result(&self, from: &DeviceId, sync_id: &SyncId, outcome: SyncOutcome);
}
```
`start` claims the peer in `starting` before the blocking work and frees it on every exit; `sync_limits.begin_outgoing(peer, key)` is taken when the session is inserted. The session's `mine` is the offered manifest.

`starter_payload_received`, in its blocking task:
- `Plan`: `SyncPlanPayload::from_json` (`BadPlan`). With `new_base_hash: None`: `check_request` against `mine`, phase `Exporting`, `export_files(wanted)`, every exported hash must equal `mine` (`Changed`), `pack_zip`, a ZIP above `MAX_TRANSFER_BYTES` is `TooBig(len)` before any offer, store `sent_files`, `offer_payload(Files)`. With `Some`: `check_final` against `mine` gives `new_base`; store the plan; when `sends` is not empty -> `AwaitFiles`; when the plan changes nothing here -> `Applying` with no `apply` (S12); else `AwaitConfirm` and `ConfirmNeeded` with `starter_summary`.
- `Files`: `unpack_zip`, `entries_match` with the `sends` map; store `received`; `AwaitConfirm` and `ConfirmNeeded`.

`confirm(true)` sets `answered`, phase `Applying`, and spawns the apply task: fresh `manifest()` equal to `mine` (else `Changed`, `SyncResult { failed }`, `State(OutOfDate)`); `changes` in apply order (`Delete`, `Rename`, then `Put` for each send with its received bytes); `apply` when `changes` is not empty (an error: `ApplyFailed`, `SyncResult { failed }`, `State(OutOfDate)`); then `send_to(SyncResult { applied })` awaited: an error ends `Unconfirmed` with RESULT_NOT_SENT and `State(OutOfDate)` (S16 row); success -> `AwaitResult`.

`starter_on_result` in `AwaitResult`: `Applied` -> `Finished { applied }` (no text, S12) and `NewBase { base: new_base, known: received + sent_files, state: None }`; `Failed` -> `PeerApplyFailed` with `State(OutOfDate)`. A result in any other phase ends the session `BadPlan`.

- [ ] **Step 11: The planner half, `sync_planner.rs`.**

```rust
//! The planner side of a sync: check the link and the base, plan, ask for the files it needs,
//! show the review, send the final plan and its files, apply after the starter, report.

impl SyncService {
    pub(super) fn planner_on_manifest_offer(&self, from: DeviceId, offer: SyncOfferFields);
    pub(super) fn planner_payload_sent(&self, sync_id: &SyncId, kind: SyncPayloadKind);
    pub(super) fn planner_payload_received(&self, sync_id: &SyncId, kind: SyncPayloadKind, bytes: Vec<u8>);

    /// The planner user's answer to `ReviewReady`. `false` ends the sync `Declined`. With
    /// `true`, every conflict needs one decision (`resolve` decides; its `InvalidInput` is
    /// returned and the review stays open), a decision on a path that is not a conflict is
    /// `InvalidInput("That file is not a conflict.")`, and `keep_both` on a settings file is
    /// `InvalidInput("Keep both is not possible for collection or folder settings.")`.
    pub async fn review(&self, sync_id: &SyncId, accept: bool, decisions: Vec<Decision>) -> DomainResult<()>;

    pub(super) fn planner_on_result(&self, from: &DeviceId, sync_id: &SyncId, outcome: SyncOutcome);
}
```
`SyncOfferFields` is a plain struct of the `SyncOffer` fields (defined in `sync.rs`).

`planner_on_manifest_offer` (worker, no I/O): the peer has a label (else ignore); the ids parse (else ignore); `sync_limits.admit_offer(from, transfer_id, now)` (`Refuse` -> `SyncAbort { busy }`, `Duplicate` -> ignore); a session, start or settle for the peer -> release the entry and `SyncAbort { busy }` (S6); else insert a `Checking` session and spawn the check task: `links.get` (none, another peer or `broken` -> `drop_quietly(link_missing)`), the collection exists with the link's uid and root (else save `broken`, `drop_quietly(link_missing)`), `record.base_hash() == offer.base_hash` (else `drop_quietly(base_mismatch)`), then store the link and `accept_payload`.

`planner_payload_received`:
- `Manifest` (blocking, phase `Planning`): `Manifest::from_json` (`Unsafe`) and `path_allowed` on every path (`Unsafe`); `mine = manifest()`; `plan_sync(&link.base, &mine, &theirs)`; `mine_conflicts` = `export_files` of the conflict paths that `mine` has; `wanted = wanted_paths(&plan)`. An empty plan -> `Finalizing` with no decisions (S12). Wanted paths -> `offer_payload(Plan, request)`. Otherwise -> `AwaitReview` and `ReviewReady`.
- `Files` (blocking): `unpack_zip`; `entries_match` against `wanted` with the hashes of `theirs`; store `received`; `AwaitReview` and `ReviewReady` with `planner_summary` and one `ConflictView` per conflict.

`review(true, d)` runs in one blocking task, under no lock: `resolve(&plan, &d, &taken)` (`taken` = every path of `mine` and `theirs`); `split_actions`; `new_base(..)`; the three-way check of S8 (`Internal` on a difference); `export_files(sends)` with each hash equal to `mine` (`Changed`); `pack_zip` of the sends with the `TooBig` check; the final payload (`deletes = peer_deletes`, `renames = peer_renames`, `decisions = d`, `sends` with `mine`'s hashes, `new_base_hash`); `changes` in apply order (`Delete`, `Rename`, `Put` for `puts`, `PutAs` for `put_as`). Only then: `answered`, `mark_active`, `offer_payload(Plan, final)`; `planner_payload_sent(Plan)` offers the files when there are sends, else -> `AwaitStarter`; `planner_payload_sent(Files)` -> `AwaitStarter`.

`review` returns `Ok(())` once the final plan is offered or once the session ended for a reason of its own (`Changed`, `TooBig`, `Internal`); that reason reaches the user through `Finished`. Only the decision errors above are returned as errors.

`planner_on_result` in `AwaitStarter`: `Applied` -> phase `Applying`, blocking task: fresh `manifest()` equal to `mine` (else `Changed`, `SyncResult { failed }`, `State(OutOfDate)`), `apply` when `changes` is not empty (error: `ApplyFailed`, `SyncResult { failed }`, `State(OutOfDate)`), `tell(SyncResult { applied })` (an error is ignored, S7), `Finished { applied }` (ALREADY_IN_SYNC for an empty plan) with `NewBase { base, known: received + sent_files, state: None }`. `Failed` -> `PeerApplyFailed` with `State(OutOfDate)`.

- [ ] **Step 12: Run the Task 2 tests.**

Run: `cargo test -j4 -p rocket-app sync_run_tests`
Expected: PASS, 21 tests.

Run: `cargo test -j4 -p rocket-app sync_tests`
Expected: PASS, 25 tests.

Run: `cargo test -j4 -p rocket-network link`
Expected: PASS.

---

### Task 3: Failure and safety paths

**Files:**
- Modify: `crates/rocket-app/src/network/{sync.rs, sync_starter.rs, sync_planner.rs, sync_payload.rs, sync_fakes.rs, sync_run_tests.rs, mod.rs}`, `crates/rocket-app/CLAUDE.md`
- Create: `crates/rocket-app/src/network/sync_safety_tests.rs`

**Interfaces:**

```rust
// Consumes: everything Tasks 1 and 2 produced.
// Produces: no new public name. Every failure row of the state tables and every spec section 7
// rule now has its test; the code changes of this task are the fixes those tests need.
```

- [ ] **Step 1: Read the reference.** 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Then read spec sections 7 and 8 again and `docs/features/collection-trust.md` (where trust lives).

- [ ] **Step 2: Share the two flow helpers.** Move `starter_confirmed` and `planner_waiting` from `sync_run_tests.rs` to the flow helpers of `sync_fakes.rs` as `pub(crate) async fn` (bodies unchanged; they use `wait_phase`, `bo_starts`, `ana_starts`, `final_plan`).

Run: `cargo test -j4 -p rocket-app sync_run_tests`
Expected: PASS, 21 tests.

- [ ] **Step 3: Write the failing failure and safety tests.** Create `crates/rocket-app/src/network/sync_safety_tests.rs`:

```rust
//! Task 3 tests: every failure row of the state tables, the spec section 7 safety rules and
//! the spec section 8 errors. Each one checks that no file changed where the spec says so.

use std::io::Write;

use rocket_audit::AuditEventKind;
use rocket_network::limits::MAX_TRANSFER_BYTES;
use rocket_network::{
    Decision, LinkState, PeerMessage, Resolution, SyncAbortReason, SyncFinish, SyncId,
    SyncOutcome, SyncPayloadKind,
};
use rocket_shared::error::DomainError;

use super::sync::Phase;
use super::sync_checks::*;
use super::sync_fakes::*;
use super::sync_ports::FileChange;
use super::transfer_fakes::noise;

fn raw_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, bytes) in entries {
            zip.start_file(*name, opts).expect("entry");
            zip.write_all(bytes).expect("bytes");
        }
        zip.finish().expect("zip");
    }
    out.into_inner()
}

fn manifest_json(entries: &[(&str, &str)]) -> Vec<u8> {
    let list: Vec<serde_json::Value> = entries
        .iter()
        .map(|(path, hash)| serde_json::json!({ "path": path, "sha256": hash }))
        .collect();
    serde_json::to_vec(&serde_json::json!({ "entries": list })).expect("json")
}

/// The scripted Ana offers raw manifest bytes for link `L` with `base_hash`.
async fn ana_offers_manifest(rig: &SyncRig, bytes: &[u8], base_hash: &str) -> SyncId {
    let sid = SyncId::random();
    let offer = offered(sid.as_str(), &lid(L), base_hash, SyncPayloadKind::Manifest, &ptid(1), bytes);
    give_payload(rig, ANA, &offer, bytes).await;
    sid
}

/// Pushes a manifest offer from `seed` without waiting for an answer.
fn manifest_offer_from(rig: &SyncRig, seed: u8, link: u8, base_hash: &str, n: u8) {
    let bytes = manifest_of(&base()).to_json().expect("json");
    let sid = SyncId::random();
    let offer = offered(sid.as_str(), &lid(link), base_hash, SyncPayloadKind::Manifest, &ptid(n), &bytes);
    rig.push(seed, offer_frame(&offer));
}

fn assert_unchanged(rig: &SyncRig, files: &Files) {
    assert!(rig.colls.applied().is_empty(), "apply was not called");
    assert_eq!(&rig.colls.files(ORDERS), files);
}

fn audit_row(starter: bool, peer: u8, outcome: &str) -> AuditEventKind {
    if starter {
        AuditEventKind::NetworkSyncSent {
            device_id: dev(peer).short(),
            collection: ORDERS.into(),
            outcome: outcome.into(),
        }
    } else {
        AuditEventKind::NetworkSyncReceived {
            device_id: dev(peer).short(),
            collection: ORDERS.into(),
            outcome: outcome.into(),
        }
    }
}

/// Start, accept the review, confirm when asked, run to the end.
async fn full_sync_simple(pair: &mut SyncPair) {
    let sid = pair.a.sync.start(&lid(L)).await.expect("started");
    pair.run_until(|p| p.b.events.reviews().len() == 1).await;
    pair.b.sync.review(&sid, true, vec![]).await.expect("accepted");
    pair.run_until(|p| p.a.events.confirms().len() == 1 || !p.a.events.finished().is_empty()).await;
    if pair.a.events.confirms().len() == 1 {
        pair.a.sync.confirm(&sid, true).await.expect("confirmed");
    }
    pair.run_until(|p| !p.a.events.finished().is_empty() && !p.b.events.finished().is_empty()).await;
}

// ----- starter failure rows -----

#[tokio::test]
async fn a_peer_apply_failure_keeps_the_old_base_out_of_date() {
    for use_abort in [false, true] {
        let rig = sync_rig();
        let (sid, _after) = starter_confirmed(&rig).await;
        wait_for("our result", || rig.results(ANA) == vec![SyncOutcome::Applied]).await;
        let answer = if use_abort {
            PeerMessage::SyncAbort { sync_id: sid.as_str().into(), reason: SyncAbortReason::Failed }
        } else {
            PeerMessage::SyncResult { sync_id: sid.as_str().into(), outcome: SyncOutcome::Failed }
        };

        rig.push(ANA, answer);

        assert_eq!(
            wait_finished(&rig).await,
            (SyncFinish::Failed, Some(PEER_APPLY_FAILED.to_string()))
        );
        let link = wait_link(&rig, |r| r.state == LinkState::OutOfDate).await;
        assert_eq!(link.base, manifest_of(&base()), "the old base is kept");
        assert_eq!(rig.audited(), vec![audit_row(true, ANA, "failed")]);
    }
}

#[tokio::test]
async fn an_apply_failure_rolls_back_and_goes_out_of_date() {
    let rig = sync_rig();
    rig.colls.fail_apply.store(true, std::sync::atomic::Ordering::SeqCst);

    let _ = starter_confirmed(&rig).await;

    wait_for("our result", || rig.results(ANA) == vec![SyncOutcome::Failed]).await;
    assert_eq!(wait_finished(&rig).await, (SyncFinish::Failed, Some(APPLY_FAILED.to_string())));
    assert_eq!(rig.colls.files(ORDERS), base(), "rolled back");
    let link = wait_link(&rig, |r| r.state == LinkState::OutOfDate).await;
    assert_eq!(link.base, manifest_of(&base()));
}

#[tokio::test]
async fn a_local_edit_before_the_apply_stops_it() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    let (plan, _) = final_plan(&base(), &["b.yml"], &[], &[]);
    ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;
    wait_phase(&rig, &sid, Phase::AwaitConfirm).await;
    rig.colls.edit_after_manifests(rig.colls.manifest_calls(), "a.yml", b"url: edited\n");

    rig.sync.confirm(&sid, true).await.expect("confirmed");

    assert_eq!(wait_finished(&rig).await, (SyncFinish::Failed, Some(CHANGED.to_string())));
    assert!(rig.colls.applied().is_empty(), "apply was never called");
    assert_eq!(rig.results(ANA), vec![SyncOutcome::Failed]);
    wait_link(&rig, |r| r.state == LinkState::OutOfDate).await;
}

#[tokio::test]
async fn an_unanswered_confirm_expires() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    let (plan, _) = final_plan(&base(), &["b.yml"], &[], &[]);
    ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;
    wait_phase(&rig, &sid, Phase::AwaitConfirm).await;

    assert_eq!(wait_finished(&rig).await, (SyncFinish::Expired, None));
    assert_eq!(rig.aborts(ANA), vec![SyncAbortReason::Expired]);
    assert_unchanged(&rig, &base());
    let err = rig.sync.confirm(&sid, true).await.expect_err("too late");
    assert!(matches!(&err, DomainError::InvalidInput(m) if m == "The sync is no longer waiting for an answer."));
}

#[tokio::test]
async fn cancel_after_confirm_is_refused() {
    let rig = sync_rig();
    let gate = rig.colls.hold_apply();
    let (sid, _) = starter_confirmed(&rig).await;
    wait_phase(&rig, &sid, Phase::Applying).await;
    let err = rig.sync.cancel(&sid).await.expect_err("refused");
    assert!(matches!(&err, DomainError::InvalidInput(m) if m == "The sync is already being applied."));
    drop(gate);

    let rig = sync_rig();
    let (sid, _) = planner_waiting(&rig).await;
    let err = rig.sync.cancel(&sid).await.expect_err("refused");
    assert!(matches!(&err, DomainError::InvalidInput(m) if m == "The sync is already being applied."));

    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let sid = ana_starts(&rig, &without(&base(), "a.yml")).await;
    wait_phase(&rig, &sid, Phase::AwaitReview).await;
    rig.sync.cancel(&sid).await.expect("cancel in the dialog declines");
    assert_eq!(wait_finished(&rig).await, (SyncFinish::Declined, None));
    assert_eq!(rig.aborts(ANA), vec![SyncAbortReason::Declined]);
    assert!(rig.sync.cancel(&sid).await.is_ok(), "a finished id is fine");
}

#[tokio::test]
async fn each_abort_reason_ends_the_starter_with_its_outcome() {
    use SyncAbortReason as R;
    let rows = [
        (R::Declined, SyncFinish::Declined, None),
        (R::Expired, SyncFinish::Expired, None),
        (R::Cancelled, SyncFinish::Cancelled, None),
        (R::BaseMismatch, SyncFinish::BaseMismatch, Some(BASE_MISMATCH)),
        (R::LinkMissing, SyncFinish::LinkMissing, Some(LINK_MISSING)),
        (R::Busy, SyncFinish::Failed, Some(BUSY)),
        (R::TooBig, SyncFinish::Failed, Some(PEER_TOO_BIG)),
        (R::Failed, SyncFinish::Failed, Some(PEER_FAILED)),
    ];
    for (reason, finish, text) in rows {
        let rig = sync_rig();
        rig.linked(&lid(L), &base(), &base());
        let (sid, ..) = bo_starts(&rig).await;

        rig.push(ANA, PeerMessage::SyncAbort { sync_id: sid.as_str().into(), reason });

        assert_eq!(
            wait_finished(&rig).await,
            (finish, text.map(str::to_string)),
            "{reason:?}"
        );
        assert_unchanged(&rig, &base());
        assert!(rig.aborts(ANA).is_empty(), "nothing is sent back");
        assert_eq!(rig.audited(), vec![audit_row(true, ANA, finish_name(finish))]);
        let want = if reason == R::LinkMissing { LinkState::Broken } else { LinkState::InSync };
        wait_link(&rig, |r| r.state == want).await;
    }
}

#[tokio::test]
async fn a_peer_gone_before_the_apply_changes_nothing() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    let (plan, _) = final_plan(&base(), &["b.yml"], &[], &[]);
    ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;
    wait_phase(&rig, &sid, Phase::AwaitConfirm).await;

    rig.router.on_peer_gone(dev(ANA));

    assert_eq!(wait_finished(&rig).await, (SyncFinish::Failed, Some(WENT_OFFLINE.to_string())));
    assert!(rig.sync.confirm(&sid, true).await.is_err());
    assert_unchanged(&rig, &base());

    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let sid = ana_starts(&rig, &without(&base(), "a.yml")).await;
    wait_phase(&rig, &sid, Phase::AwaitReview).await;

    rig.router.on_peer_gone(dev(ANA));

    assert_eq!(wait_finished(&rig).await, (SyncFinish::Failed, Some(WENT_OFFLINE.to_string())));
    assert!(rig.sync.review(&sid, true, vec![]).await.is_err());
    assert_unchanged(&rig, &base());
    assert_eq!(rig.links.count(), 1, "a dropped connection keeps the link");
}

#[tokio::test]
async fn a_silent_planner_fails_the_starter_after_the_wait() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let _ = bo_starts(&rig).await;

    assert_eq!(wait_finished(&rig).await, (SyncFinish::Failed, Some(STALLED.to_string())));
    assert_eq!(rig.aborts(ANA), vec![SyncAbortReason::Failed]);

    // A peer that answers the manifest offer and never acks stalls the stream the same way.
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    rig.sync.start(&lid(L)).await.expect("started");
    let offer = nth_offer(&rig, ANA, SyncPayloadKind::Manifest, 0).await;
    rig.push(ANA, PeerMessage::OfferAnswer { transfer_id: offer.transfer_id, accept: true });

    assert_eq!(wait_finished(&rig).await, (SyncFinish::Failed, Some(STALLED.to_string())));
}

#[tokio::test]
async fn a_local_edit_before_the_export_stops_the_sync() {
    let rig = sync_rig();
    let mine = with(&base(), "a.yml", "url: a2\n");
    rig.linked(&lid(L), &base(), &mine);
    let (sid, ..) = bo_starts(&rig).await;
    rig.colls.put(ORDERS, "a.yml", b"url: a3\n");

    ana_sends_plan(&rig, sid.as_str(), 2, &request(&["a.yml"])).await;

    assert_eq!(wait_finished(&rig).await, (SyncFinish::Failed, Some(CHANGED.to_string())));
    assert_eq!(rig.aborts(ANA), vec![SyncAbortReason::Failed]);
    assert!(
        rig.sent(ANA).iter().filter_map(as_offer).all(|o| o.kind != SyncPayloadKind::Files),
        "no files were offered"
    );
}

#[tokio::test]
async fn too_big_changes_are_refused_before_any_offer() {
    let rig = sync_rig();
    let mut mine = base();
    mine.insert("big.yml".into(), noise(MAX_TRANSFER_BYTES as usize));
    rig.linked(&lid(L), &base(), &mine);
    let (sid, ..) = bo_starts(&rig).await;

    ana_sends_plan(&rig, sid.as_str(), 2, &request(&["big.yml"])).await;

    let (finish, message) = wait_finished(&rig).await;
    assert_eq!(finish, SyncFinish::Failed);
    let message = message.expect("a text");
    assert!(message.starts_with("The changes are too big to sync ("), "{message}");
    assert!(message.ends_with(" MiB, limit 10)."), "{message}");
    assert_eq!(rig.aborts(ANA), vec![SyncAbortReason::TooBig]);
    assert!(rig.sent(ANA).iter().filter_map(as_offer).all(|o| o.kind != SyncPayloadKind::Files));

    // Just under the cap passes: the files payload is offered and arrives whole.
    let rig = sync_rig();
    let mut mine = base();
    mine.insert("big.yml".into(), noise(MAX_TRANSFER_BYTES as usize - 64 * 1024));
    rig.linked(&lid(L), &base(), &mine);
    let (sid, ..) = bo_starts(&rig).await;
    ana_sends_plan(&rig, sid.as_str(), 2, &request(&["big.yml"])).await;
    let (offer, zip) = take_payload(&rig, ANA, SyncPayloadKind::Files, 0).await;
    assert!(offer.size_bytes <= MAX_TRANSFER_BYTES);
    assert_eq!(zip.len() as u64, offer.size_bytes);
}

#[tokio::test]
async fn a_sync_offer_out_of_phase_ends_the_session() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    let zip = zip_of(&files(&[("b.yml", "x")]));
    rig.push(
        ANA,
        offer_frame(&offered(sid.as_str(), &lid(L), &bh(), SyncPayloadKind::Files, &ptid(2), &zip)),
    );
    assert_eq!(wait_finished(&rig).await, (SyncFinish::Failed, Some(BAD_PLAN.to_string())));
    assert_eq!(rig.aborts(ANA), vec![SyncAbortReason::Failed]);

    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    let plan = request(&["a.yml"]).to_json().expect("json");
    let other_base = "e".repeat(64);
    rig.push(
        ANA,
        offer_frame(&offered(sid.as_str(), &lid(L), &other_base, SyncPayloadKind::Plan, &ptid(2), &plan)),
    );
    assert_eq!(wait_finished(&rig).await, (SyncFinish::Failed, Some(BAD_PLAN.to_string())));

    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let _ = bo_starts(&rig).await;
    let stray = SyncId::random();
    rig.push(
        ANA,
        offer_frame(&offered(stray.as_str(), &lid(L), &bh(), SyncPayloadKind::Plan, &ptid(2), &plan)),
    );
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(rig.events.finished().is_empty(), "an unknown sync id is ignored");
}

// ----- planner failure rows and caps -----

/// Pairs Ana, Ben, Cy and Di, each with its own link (1 to 4) to `orders`.
fn hub() -> SyncRig {
    let rig = sync_rig();
    rig.colls.add(ORDERS, base());
    let base_files: Vec<(String, Vec<u8>)> = base().into_iter().collect();
    for (n, (seed, name)) in [(ANA, "Ana"), (BEN, "Ben"), (4, "Cy"), (5, "Di")].into_iter().enumerate() {
        rig.peers.add(dev(seed), name);
        rig.links
            .save(&record(&lid(n as u8 + 1), seed, &base()), &base_files)
            .expect("link");
    }
    rig
}

#[tokio::test]
async fn a_fourth_waiting_sync_is_busy() {
    let rig = hub();
    let theirs = manifest_of(&without(&base(), "a.yml")).to_json().expect("json");
    for (n, seed) in [(1u8, ANA), (2, BEN), (3, 4)] {
        let sid = SyncId::random();
        let offer = offered(sid.as_str(), &lid(n), &bh(), SyncPayloadKind::Manifest, &ptid(n), &theirs);
        give_payload(&rig, seed, &offer, &theirs).await;
    }
    wait_for("three reviews", || rig.events.reviews().len() == 3).await;

    let sid = SyncId::random();
    let offer = offered(sid.as_str(), &lid(4), &bh(), SyncPayloadKind::Manifest, &ptid(4), &theirs);
    rig.push(5, offer_frame(&offer));

    wait_for("the busy answer", || rig.aborts(5) == vec![SyncAbortReason::Busy]).await;
    assert_eq!(rig.events.reviews().len(), 3);
}

#[tokio::test]
async fn a_second_session_with_one_peer_is_busy() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    rig.sync.start(&lid(L)).await.expect("Bo starts");

    manifest_offer_from(&rig, ANA, L, &bh(), 9);

    wait_for("the busy answer", || rig.aborts(ANA) == vec![SyncAbortReason::Busy]).await;
    assert!(rig.audited().is_empty());
}

#[tokio::test]
async fn the_sync_rate_window_refuses_the_sixth_start_in_a_minute() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let wrong = "e".repeat(64);
    for n in 1..=5u8 {
        manifest_offer_from(&rig, ANA, L, &wrong, n);
        wait_for("the base mismatch", || rig.aborts(ANA).len() == n as usize).await;
    }

    manifest_offer_from(&rig, ANA, L, &bh(), 6);

    wait_for("the sixth answer", || rig.aborts(ANA).len() == 6).await;
    assert_eq!(rig.aborts(ANA)[5], SyncAbortReason::Busy);
    assert!(rig.aborts(ANA)[..5].iter().all(|r| *r == SyncAbortReason::BaseMismatch));
}

#[tokio::test]
async fn follow_up_sync_offers_do_not_count_against_the_rate_window() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let wrong = "e".repeat(64);
    for n in 1..=4u8 {
        manifest_offer_from(&rig, ANA, L, &wrong, n);
        wait_for("the base mismatch", || rig.aborts(ANA).len() == n as usize).await;
    }
    let theirs = with(&base(), "a.yml", "url: a-ana\n");

    // The fifth start uses the last hit; its files offer must still be admitted.
    let sid = ana_starts(&rig, &theirs).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
    ana_sends_files(&rig, sid.as_str(), 7, &zip_of(&files(&[("a.yml", "url: a-ana\n")]))).await;

    wait_for("the review", || rig.events.reviews().len() == 1).await;
    assert_eq!(rig.aborts(ANA).len(), 4, "no busy answer");
}

#[tokio::test]
async fn a_sync_does_not_use_the_collection_transfer_slots() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let sid = ana_starts(&rig, &without(&base(), "a.yml")).await;
    wait_phase(&rig, &sid, Phase::AwaitReview).await;

    let zip = zip_of(&base());
    rig.push(
        ANA,
        PeerMessage::CollectionOffer {
            transfer_id: ptid(40),
            name: ORDERS.into(),
            size_bytes: zip.len() as u64,
            chunk_count: rocket_network::chunk_count_for(zip.len() as u64),
            sha256: sha(&zip),
            keep_in_sync: false,
            link_id: None,
        },
    );
    wait_for("the collection offer is shown", || !rig.transfer_events.all().is_empty()).await;
    rig.transfer
        .send(&dev(ANA), ORDERS, false, false)
        .await
        .expect("a collection send to the same peer is not blocked by the sync");
}

#[tokio::test]
async fn an_unknown_or_broken_link_is_link_missing() {
    let rig = sync_rig();
    rig.peers.add(dev(ANA), "Ana");
    rig.colls.add(ORDERS, base());
    manifest_offer_from(&rig, ANA, L, &bh(), 1);
    wait_for("link missing", || rig.aborts(ANA) == vec![SyncAbortReason::LinkMissing]).await;

    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let mut broken = rig.links.record(&lid(L)).expect("link");
    broken.state = LinkState::Broken;
    rig.links.save(&broken, &[]).expect("saved");
    manifest_offer_from(&rig, ANA, L, &bh(), 1);
    wait_for("link missing", || rig.aborts(ANA) == vec![SyncAbortReason::LinkMissing]).await;

    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    rig.peers.add(dev(BEN), "Ben");
    manifest_offer_from(&rig, BEN, L, &bh(), 1);
    wait_for("link missing", || rig.aborts(BEN) == vec![SyncAbortReason::LinkMissing]).await;
    assert!(rig.events.all().is_empty(), "the user never sees a refused offer");
    assert!(rig.audited().is_empty());
}

#[tokio::test]
async fn a_moved_collection_is_link_missing_and_broken() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    rig.colls.set_root(ORDERS, "/elsewhere/orders");

    manifest_offer_from(&rig, ANA, L, &bh(), 1);

    wait_for("link missing", || rig.aborts(ANA) == vec![SyncAbortReason::LinkMissing]).await;
    wait_link(&rig, |r| r.state == LinkState::Broken).await;
    assert_unchanged(&rig, &base());
}

#[tokio::test]
async fn a_base_mismatch_is_answered_and_changes_nothing() {
    let rig = sync_rig();
    let other_base = with(&base(), "a.yml", "url: other\n");
    rig.peers.add(dev(ANA), "Ana");
    rig.colls.add(ORDERS, base());
    let base_files: Vec<(String, Vec<u8>)> = other_base.clone().into_iter().collect();
    rig.links.save(&record(&lid(L), ANA, &other_base), &base_files).expect("link");

    manifest_offer_from(&rig, ANA, L, &bh(), 1);

    wait_for("the mismatch", || rig.aborts(ANA) == vec![SyncAbortReason::BaseMismatch]).await;
    assert!(rig.events.all().is_empty());
    assert!(rig.audited().is_empty(), "no chunk left this node");
    assert_eq!(rig.links.record(&lid(L)).expect("link").base, manifest_of(&other_base));
    assert_unchanged(&rig, &base());
}

#[tokio::test]
async fn review_needs_one_valid_decision_per_conflict() {
    let bo = with(&base(), "a.yml", "url: a-bo\n");
    let ana = with(&base(), "a.yml", "url: a-ana\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &ana);
    let sid = pair.a.sync.start(&lid(L)).await.expect("started");
    pair.run_until(|p| p.b.events.reviews().len() == 1).await;

    let err = pair.b.sync.review(&sid, true, vec![]).await.expect_err("no decision");
    assert!(matches!(err, DomainError::InvalidInput(_)));
    let err = pair
        .b
        .sync
        .review(&sid, true, vec![Decision { path: "b.yml".into(), resolution: Resolution::KeepMine }])
        .await
        .expect_err("not a conflict");
    assert!(matches!(&err, DomainError::InvalidInput(m) if m == "That file is not a conflict."));
    assert_eq!(pair.b.sync.phase_of(&sid), Some(Phase::AwaitReview), "the review stays open");

    pair.b
        .sync
        .review(&sid, true, vec![Decision { path: "a.yml".into(), resolution: Resolution::KeepMine }])
        .await
        .expect("now valid");
}

#[tokio::test]
async fn declining_the_review_changes_nothing_on_both_sides() {
    let bo = with(&base(), "a.yml", "url: a-bo\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &base());
    let sid = pair.a.sync.start(&lid(L)).await.expect("started");
    pair.run_until(|p| p.b.events.reviews().len() == 1).await;

    pair.b.sync.review(&sid, false, vec![]).await.expect("declined");
    pair.run_until(|p| !p.a.events.finished().is_empty() && !p.b.events.finished().is_empty()).await;

    assert_eq!(pair.a.events.finished()[0].1, SyncFinish::Declined);
    assert_eq!(pair.b.events.finished()[0].1, SyncFinish::Declined);
    assert_unchanged(&pair.a, &bo);
    assert_unchanged(&pair.b, &base());
    assert_eq!(pair.a.links.record(&lid(L)).expect("a").base, manifest_of(&base()));
    assert_eq!(pair.b.links.record(&lid(L)).expect("b").base, manifest_of(&base()));
    assert_eq!(pair.a.audited(), vec![audit_row(true, ANA, "declined")]);
    assert_eq!(pair.b.audited(), vec![audit_row(false, BO, "declined")], "the request left Ana");
}

#[tokio::test]
async fn declining_the_confirm_changes_nothing() {
    let bo = with(&base(), "a.yml", "url: a-bo\n");
    let ana = with(&base(), "b.yml", "url: b-ana\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &ana);
    let sid = pair.a.sync.start(&lid(L)).await.expect("started");
    pair.run_until(|p| p.b.events.reviews().len() == 1).await;
    pair.b.sync.review(&sid, true, vec![]).await.expect("accepted");
    pair.run_until(|p| p.a.events.confirms().len() == 1).await;

    pair.a.sync.confirm(&sid, false).await.expect("declined");
    pair.run_until(|p| !p.a.events.finished().is_empty() && !p.b.events.finished().is_empty()).await;

    assert_eq!(pair.a.events.finished()[0].1, SyncFinish::Declined);
    assert_eq!(pair.b.events.finished()[0].1, SyncFinish::Declined);
    assert_unchanged(&pair.a, &bo);
    assert_unchanged(&pair.b, &ana);
}

#[tokio::test]
async fn an_unanswered_review_expires_on_both_sides() {
    let bo = with(&base(), "a.yml", "url: a-bo\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &base());
    pair.a.sync.start(&lid(L)).await.expect("started");
    pair.run_until(|p| p.b.events.reviews().len() == 1).await;

    pair.run_until(|p| !p.a.events.finished().is_empty() && !p.b.events.finished().is_empty()).await;

    assert_eq!(pair.b.events.finished()[0].1, SyncFinish::Expired);
    assert_eq!(pair.a.events.finished()[0].1, SyncFinish::Expired);
    assert_unchanged(&pair.a, &bo);
    assert_unchanged(&pair.b, &base());
}

#[tokio::test]
async fn cancel_before_the_dialog_ends_the_sync_on_both_sides() {
    let bo = with(&base(), "a.yml", "url: a-bo\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &base());
    let sid = pair.a.sync.start(&lid(L)).await.expect("started");
    pair.run_until(|p| p.b.events.reviews().len() == 1).await;

    pair.a.sync.cancel(&sid).await.expect("cancelled");
    pair.run_until(|p| !p.a.events.finished().is_empty() && !p.b.events.finished().is_empty()).await;

    assert_eq!(pair.a.events.finished()[0].1, SyncFinish::Cancelled);
    assert_eq!(pair.b.events.finished()[0].1, SyncFinish::Cancelled);
    assert_unchanged(&pair.a, &bo);
    assert_unchanged(&pair.b, &base());

    // The planner cancels while it waits for the starter's files.
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let sid = ana_starts(&rig, &with(&base(), "a.yml", "url: a-ana\n")).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
    wait_phase(&rig, &sid, Phase::AwaitFiles).await;
    rig.sync.cancel(&sid).await.expect("cancelled");
    assert_eq!(wait_finished(&rig).await, (SyncFinish::Cancelled, None));
    assert_eq!(rig.aborts(ANA), vec![SyncAbortReason::Cancelled]);
    assert_eq!(rig.audited(), vec![audit_row(false, ANA, "cancelled")]);
}

#[tokio::test]
async fn a_planner_apply_failure_tells_the_starter_and_goes_out_of_date() {
    let bo = with(&base(), "a.yml", "url: a-bo\n");
    let ana = with(&base(), "b.yml", "url: b-ana\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &ana);
    pair.b.colls.fail_apply.store(true, std::sync::atomic::Ordering::SeqCst);

    full_sync_simple(&mut pair).await;

    assert_eq!(pair.b.events.finished()[0].1, SyncFinish::Failed);
    assert_eq!(pair.b.events.finished()[0].2.as_deref(), Some(APPLY_FAILED));
    assert_eq!(pair.a.events.finished()[0].2.as_deref(), Some(PEER_APPLY_FAILED));
    assert_eq!(pair.b.colls.files(ORDERS), ana, "the planner rolled back");
    let both = with(&bo, "b.yml", "url: b-ana\n");
    assert_eq!(pair.a.colls.files(ORDERS), both, "the starter had applied");
    pair.run_until(|p| {
        p.a.links.record(&lid(L)).is_some_and(|r| r.state == LinkState::OutOfDate)
            && p.b.links.record(&lid(L)).is_some_and(|r| r.state == LinkState::OutOfDate)
    })
    .await;
    assert_eq!(pair.a.links.record(&lid(L)).expect("a").base, manifest_of(&base()));
    assert_eq!(pair.b.links.record(&lid(L)).expect("b").base, manifest_of(&base()));

    // The next sync starts from the old bases and finds the starter's files already equal.
    pair.b.colls.fail_apply.store(false, std::sync::atomic::Ordering::SeqCst);
    let sid = pair.a.sync.start(&lid(L)).await.expect("started again");
    pair.run_until(|p| p.b.events.reviews().len() == 1).await;
    pair.b.sync.review(&sid, true, vec![]).await.expect("accepted");
    pair.run_until(|p| p.b.events.finished().len() == 2 && p.a.events.finished().len() == 2).await;
    assert_eq!(pair.b.events.finished()[1].1, SyncFinish::Applied);
    assert_eq!(pair.b.colls.files(ORDERS), both);
    assert_eq!(pair.a.events.confirms().len(), 1, "the second sync asks the starter nothing");
}

#[tokio::test]
async fn a_starter_apply_failure_marks_the_planner_out_of_date() {
    let bo = with(&base(), "a.yml", "url: a-bo\n");
    let ana = with(&base(), "b.yml", "url: b-ana\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &ana);
    pair.a.colls.fail_apply.store(true, std::sync::atomic::Ordering::SeqCst);

    full_sync_simple(&mut pair).await;

    assert_eq!(pair.a.events.finished()[0].2.as_deref(), Some(APPLY_FAILED));
    assert_eq!(pair.b.events.finished()[0].2.as_deref(), Some(PEER_APPLY_FAILED));
    assert_unchanged(&pair.b, &ana);
    assert_eq!(pair.a.colls.files(ORDERS), bo);
    pair.run_until(|p| p.b.links.record(&lid(L)).is_some_and(|r| r.state == LinkState::OutOfDate)).await;
}

#[tokio::test]
async fn a_silent_starter_fails_the_planner_after_the_wait() {
    let rig = sync_rig();
    let (sid, _) = planner_waiting(&rig).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Files, 0).await;
    wait_phase(&rig, &sid, Phase::AwaitStarter).await;

    assert_eq!(wait_finished(&rig).await, (SyncFinish::Failed, Some(STALLED.to_string())));
    assert_eq!(rig.aborts(ANA), vec![SyncAbortReason::Failed]);
    assert!(rig.colls.applied().is_empty());
}

#[tokio::test]
async fn a_local_edit_during_the_review_stops_the_sync() {
    let rig = sync_rig();
    let mine = with(&base(), "n.yml", "url: n\n");
    rig.linked(&lid(L), &base(), &mine);
    let sid = ana_starts(&rig, &without(&base(), "a.yml")).await;
    wait_phase(&rig, &sid, Phase::AwaitReview).await;
    rig.colls.put(ORDERS, "n.yml", b"url: n-edited\n");

    rig.sync.review(&sid, true, vec![]).await.expect("the reason goes to Finished");

    assert_eq!(wait_finished(&rig).await, (SyncFinish::Failed, Some(CHANGED.to_string())));
    assert_eq!(rig.aborts(ANA), vec![SyncAbortReason::Failed]);
    assert!(rig.sent(ANA).iter().filter_map(as_offer).next().is_none(), "no final plan");
    assert!(rig.colls.applied().is_empty());
}

// ----- spec section 7 safety rules -----

#[tokio::test]
async fn a_manifest_with_an_unsafe_path_is_refused() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let bytes = manifest_json(&[("../SECRET-TOKEN.yml", &"a".repeat(64))]);

    ana_offers_manifest(&rig, &bytes, &bh()).await;

    assert_eq!(wait_finished(&rig).await, (SyncFinish::Failed, Some(UNSAFE.to_string())));
    assert_eq!(rig.aborts(ANA), vec![SyncAbortReason::Failed]);
    assert!(rig.events.reviews().is_empty());
    for event in rig.events.all() {
        let json = serde_json::to_string(&event).expect("json");
        assert!(!json.contains("SECRET-TOKEN"), "no peer text in any event");
    }
    assert_unchanged(&rig, &base());
}

#[tokio::test]
async fn a_nested_collection_file_is_refused() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let mut theirs = manifest_of(&base());
    theirs.insert("f/opencollection.yml", &"a".repeat(64)).expect("Plan 1 allows the path");
    let bytes = theirs.to_json().expect("json");

    ana_offers_manifest(&rig, &bytes, &bh()).await;

    assert_eq!(wait_finished(&rig).await, (SyncFinish::Failed, Some(UNSAFE.to_string())));
    assert_unchanged(&rig, &base());
}

#[tokio::test]
async fn a_request_for_a_path_not_offered_is_refused() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;

    ana_sends_plan(&rig, sid.as_str(), 2, &request(&["environments/prod.yml"])).await;

    assert_eq!(wait_finished(&rig).await, (SyncFinish::Failed, Some(UNSAFE.to_string())));
    assert!(rig.sent(ANA).iter().filter_map(as_offer).all(|o| o.kind != SyncPayloadKind::Files));
}

#[tokio::test]
async fn a_final_plan_with_an_unsafe_or_unknown_path_is_refused() {
    let cases: Vec<(Vec<&str>, Vec<(&str, &str)>)> = vec![
        (vec!["../x.yml"], vec![]),
        (vec!["x.yml"], vec![]),
        (vec![], vec![("a.yml", "f/opencollection.yml")]),
        (vec![], vec![("a.yml", ".rocket/a.yml")]),
    ];
    for (deletes, renames) in cases {
        let rig = sync_rig();
        rig.linked(&lid(L), &base(), &base());
        let (sid, ..) = bo_starts(&rig).await;
        let mut plan = empty_plan();
        plan.deletes = deletes.iter().map(|s| s.to_string()).collect();
        plan.renames = renames
            .iter()
            .map(|(f, t)| rocket_network::PlanRename { from: f.to_string(), to: t.to_string() })
            .collect();
        plan.new_base_hash = Some(bh());

        ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;

        assert_eq!(
            wait_finished(&rig).await,
            (SyncFinish::Failed, Some(UNSAFE.to_string())),
            "{deletes:?} {renames:?}"
        );
        assert!(rig.events.confirms().is_empty());
        assert_unchanged(&rig, &base());
    }
}

#[tokio::test]
async fn a_final_plan_whose_base_hash_disagrees_is_refused() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    let (mut plan, _) = final_plan(&base(), &["b.yml"], &[], &[]);
    plan.new_base_hash = Some(manifest_of(&base()).base_hash());

    ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;

    assert_eq!(wait_finished(&rig).await, (SyncFinish::Failed, Some(BAD_PLAN.to_string())));
    assert!(rig.events.confirms().is_empty());
    assert_unchanged(&rig, &base());
    assert_eq!(
        rig.links.record(&lid(L)).expect("link").base,
        manifest_of(&base()),
        "the peer's idea of the base is never stored"
    );
}

#[tokio::test]
async fn a_files_zip_with_an_extra_or_missing_entry_is_unsafe() {
    // Starter: the plan sends b.yml, the ZIP also holds x.yml.
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    let (plan, _) = final_plan(&base(), &[], &[], &[("b.yml", "url: b2\n")]);
    ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;
    let zip = zip_of(&files(&[("b.yml", "url: b2\n"), ("x.yml", "url: x\n")]));
    ana_sends_files(&rig, sid.as_str(), 3, &zip).await;
    assert_eq!(wait_finished(&rig).await, (SyncFinish::Failed, Some(UNSAFE.to_string())));
    assert!(rig.events.confirms().is_empty());
    assert_unchanged(&rig, &base());

    // Planner: it asked for a.yml and c.yml, the ZIP holds only a.yml.
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let theirs = with(&with(&base(), "a.yml", "url: a2\n"), "c.yml", "url: c\n");
    let sid = ana_starts(&rig, &theirs).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
    ana_sends_files(&rig, sid.as_str(), 2, &zip_of(&files(&[("a.yml", "url: a2\n")]))).await;
    assert_eq!(wait_finished(&rig).await, (SyncFinish::Failed, Some(UNSAFE.to_string())));
    assert!(rig.events.reviews().is_empty());
}

#[tokio::test]
async fn a_files_zip_that_fails_the_strict_validator_is_unsafe() {
    let mut trailing = zip_of(&files(&[("b.yml", "url: b2\n")]));
    trailing.push(0);
    for zip in [
        raw_zip(&[("B.yml", b"url: b2\n"), ("b.yml", b"url: b2\n")]),
        raw_zip(&[("../b.yml", b"url: b2\n")]),
        trailing,
    ] {
        let rig = sync_rig();
        rig.linked(&lid(L), &base(), &base());
        let (sid, ..) = bo_starts(&rig).await;
        let (plan, _) = final_plan(&base(), &[], &[], &[("b.yml", "url: b2\n")]);
        ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;

        ana_sends_files(&rig, sid.as_str(), 3, &zip).await;

        assert_eq!(wait_finished(&rig).await, (SyncFinish::Failed, Some(UNSAFE.to_string())));
        assert_unchanged(&rig, &base());
    }
}

#[tokio::test]
async fn a_damaged_payload_is_refused_before_any_dialog() {
    // The offer's hash does not match the bytes that arrive (same length).
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let theirs = with(&base(), "a.yml", "url: a2\n");
    let sid = ana_starts(&rig, &theirs).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
    let zip = zip_of(&files(&[("a.yml", "url: a2\n")]));
    let mut offer = offered(sid.as_str(), &lid(L), &bh(), SyncPayloadKind::Files, &ptid(2), &zip);
    offer.sha256 = sha(b"something else");
    give_payload(&rig, ANA, &offer, &zip).await;
    assert_eq!(wait_finished(&rig).await, (SyncFinish::Failed, Some(DAMAGED.to_string())));
    assert!(rig.events.reviews().is_empty());

    // A chunk out of order fails at once.
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let bytes = manifest_of(&base()).to_json().expect("json");
    let sid = SyncId::random();
    let offer = offered(sid.as_str(), &lid(L), &bh(), SyncPayloadKind::Manifest, &ptid(1), &bytes);
    rig.push(ANA, offer_frame(&offer));
    wait_for("the answer", || !rig.sent(ANA).is_empty()).await;
    let mut chunk = chunk_frames(&ptid(1), &bytes).remove(0);
    if let PeerMessage::Chunk { seq, .. } = &mut chunk {
        *seq = 1;
    }
    rig.push(ANA, chunk);
    assert_eq!(wait_finished(&rig).await, (SyncFinish::Failed, Some(DAMAGED.to_string())));
}

#[tokio::test]
async fn a_file_whose_hash_differs_from_the_plan_is_damaged() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    let (plan, _) = final_plan(&base(), &[], &[], &[("b.yml", "url: b2\n")]);
    ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;

    ana_sends_files(&rig, sid.as_str(), 3, &zip_of(&files(&[("b.yml", "url: evil\n")]))).await;

    assert_eq!(wait_finished(&rig).await, (SyncFinish::Failed, Some(DAMAGED.to_string())));
    assert!(rig.events.confirms().is_empty());
    assert_unchanged(&rig, &base());
}

#[tokio::test]
async fn a_sync_writes_only_through_the_collection_port() {
    let bo = with(&without(&base(), "b.yml"), "a.yml", "url: a-bo\n");
    let ana = with(&base(), "c.yml", "url: c\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &ana);
    let saves_before = (pair.a.links.saves(), pair.b.links.saves());

    full_sync_simple(&mut pair).await;
    pair.run_until(|p| p.a.links.saves() > saves_before.0 && p.b.links.saves() > saves_before.1).await;

    // SyncDeps has no trust port: the collection port and the link repository are the only
    // writers, and every write names a path inside the collection.
    for rig in [&pair.a, &pair.b] {
        for (collection, changes) in rig.colls.applied() {
            assert_eq!(collection, ORDERS);
            for change in changes {
                let paths = match &change {
                    FileChange::Put { path, .. } | FileChange::PutAs { path, .. } | FileChange::Delete { path } => vec![path.clone()],
                    FileChange::Rename { from, to } => vec![from.clone(), to.clone()],
                };
                for path in paths {
                    assert!(path_allowed(&path), "{path}");
                    assert_ne!(path, "trust.yml");
                }
            }
        }
    }
    assert_eq!(pair.a.links.saves(), saves_before.0 + 1, "one link save per side");
    assert_eq!(pair.b.links.saves(), saves_before.1 + 1);
}

#[tokio::test]
async fn a_synced_collection_file_is_always_merged_never_written_raw() {
    // A hostile peer whose collection file asks for Developer mode. The real export strips
    // `extensions.rocketapi` and Plan 2's merge_file keeps the local one (its own test); here
    // the service must hand the file to the merge (Put), never write it as received (PutAs),
    // and trust lives in ~/.rocket-api/trust.yml, which no sync can reach.
    let hostile = "name: orders\nextensions:\n  rocketapi:\n    sandboxMode: developer\n";
    let bo = with(&base(), "opencollection.yml", hostile);
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &base());

    full_sync_simple(&mut pair).await;

    let changes: Vec<FileChange> = pair.b.colls.applied().into_iter().flat_map(|(_, c)| c).collect();
    assert!(changes.contains(&FileChange::Put {
        path: "opencollection.yml".into(),
        bytes: hostile.as_bytes().to_vec(),
    }));
    assert!(!changes.iter().any(|c| matches!(c, FileChange::PutAs { .. })));
}

#[tokio::test]
async fn keep_both_is_refused_for_settings_files() {
    let bo = with(&base(), "opencollection.yml", "name: orders-bo\n");
    let ana = with(&base(), "opencollection.yml", "name: orders-ana\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &ana);
    let sid = pair.a.sync.start(&lid(L)).await.expect("started");
    pair.run_until(|p| p.b.events.reviews().len() == 1).await;

    let err = pair
        .b
        .sync
        .review(&sid, true, vec![Decision { path: "opencollection.yml".into(), resolution: Resolution::KeepBoth }])
        .await
        .expect_err("refused");

    assert!(matches!(&err, DomainError::InvalidInput(m)
        if m == "Keep both is not possible for collection or folder settings."));
    pair.b
        .sync
        .review(&sid, true, vec![Decision { path: "opencollection.yml".into(), resolution: Resolution::KeepTheirs }])
        .await
        .expect("keep theirs is fine");
}

#[tokio::test]
async fn removing_the_pairing_mid_sync_ends_it_with_no_change() {
    let bo = with(&base(), "a.yml", "url: a-bo\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &base());
    pair.a.sync.start(&lid(L)).await.expect("started");
    pair.run_until(|p| p.b.events.reviews().len() == 1).await;

    // Ana removes the pairing; Bo hears `Unpair`, which removes it there too.
    pair.b.peers.forget(&dev(BO));
    pair.b.router.on_peer_gone(dev(BO));
    pair.a.peers.forget(&dev(ANA));
    pair.a.router.on_peer_gone(dev(ANA));

    pair.run_until(|p| !p.a.events.finished().is_empty() && !p.b.events.finished().is_empty()).await;
    assert_eq!(pair.a.events.finished()[0].2.as_deref(), Some(WENT_OFFLINE));
    assert_eq!(pair.b.events.finished()[0].2.as_deref(), Some(WENT_OFFLINE));
    assert_unchanged(&pair.a, &bo);
    assert_unchanged(&pair.b, &base());
    pair.run_until(|p| p.a.links.count() == 0 && p.b.links.count() == 0).await;
}

#[tokio::test]
async fn unlinking_during_a_sync_cancels_it() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    wait_phase(&rig, &sid, Phase::AwaitPlan).await;

    rig.sync.unlink(&lid(L)).await.expect("unlinked");

    assert_eq!(wait_finished(&rig).await, (SyncFinish::Cancelled, None));
    assert_eq!(rig.aborts(ANA), vec![SyncAbortReason::Cancelled]);
    assert_eq!(rig.links.count(), 0);
}

// ----- worker, texts, audit -----

#[tokio::test]
async fn a_slow_apply_does_not_hold_back_another_peer() {
    let rig = sync_rig();
    let (sid, _) = planner_waiting(&rig).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Files, 0).await;
    wait_phase(&rig, &sid, Phase::AwaitStarter).await;
    rig.peers.add(dev(BEN), "Ben");
    let base_files: Vec<(String, Vec<u8>)> = base().into_iter().collect();
    rig.links.save(&record(&lid(2), BEN, &base()), &base_files).expect("Ben's link");
    let gate = rig.colls.hold_apply();
    rig.push(ANA, PeerMessage::SyncResult { sync_id: sid.as_str().into(), outcome: SyncOutcome::Applied });
    wait_phase(&rig, &sid, Phase::Applying).await;

    let ben = SyncId::random();
    let bytes = manifest_of(&base()).to_json().expect("json");
    let offer = offered(ben.as_str(), &lid(2), &bh(), SyncPayloadKind::Manifest, &ptid(30), &bytes);
    give_payload(&rig, BEN, &offer, &bytes).await;

    wait_for("Ben's review while Ana's apply is held", || rig.events.reviews().len() == 2).await;
    drop(gate);
    wait_for("Ana's sync ended", || !rig.events.finished().is_empty()).await;
}

#[test]
fn every_finished_message_is_one_of_the_fixed_texts() {
    let fixed = [
        WENT_OFFLINE, DAMAGED, UNSAFE, BAD_PLAN, BASE_MISMATCH, LINK_MISSING, CHANGED, APPLY_FAILED,
        PEER_APPLY_FAILED, PEER_FAILED, PEER_TOO_BIG, BUSY, STALLED, INTERNAL,
    ];
    let mut stops = vec![
        Stop::Damaged, Stop::Unsafe, Stop::BadPlan, Stop::Changed, Stop::ApplyFailed, Stop::PeerApplyFailed,
        Stop::WentOffline, Stop::Stalled, Stop::Internal, Stop::Declined, Stop::Expired, Stop::Cancelled,
    ];
    for r in [
        SyncAbortReason::BaseMismatch, SyncAbortReason::LinkMissing, SyncAbortReason::Busy, SyncAbortReason::Cancelled,
        SyncAbortReason::Declined, SyncAbortReason::Expired, SyncAbortReason::Failed, SyncAbortReason::TooBig,
    ] {
        stops.push(Stop::Peer(r));
    }
    for stop in stops {
        if let Some(text) = stop.message() {
            assert!(fixed.contains(&text.as_str()), "{stop:?}: {text}");
        }
    }
}

#[tokio::test]
async fn a_refusal_before_any_chunk_writes_no_audit() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    manifest_offer_from(&rig, ANA, L, &"e".repeat(64), 1);
    wait_for("the mismatch", || rig.aborts(ANA).len() == 1).await;
    manifest_offer_from(&rig, ANA, 9, &bh(), 2);
    wait_for("link missing", || rig.aborts(ANA).len() == 2).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(rig.audited().is_empty());
    assert!(rig.events.finished().is_empty());
}
```
Register the module in `mod.rs`: `#[cfg(test)] #[path = "sync_safety_tests.rs"] mod sync_safety_tests;`.

Spec section 8 coverage, one line each: declined `declining_the_review_changes_nothing_on_both_sides`, `declining_the_confirm_changes_nothing`; expired `an_unanswered_review_expires_on_both_sides`, `an_unanswered_confirm_expires`; the peer left `a_peer_gone_before_the_apply_changes_nothing`; the connection dropped `a_silent_planner_fails_the_starter_after_the_wait`, `a_silent_starter_fails_the_planner_after_the_wait`; too big `too_big_changes_are_refused_before_any_offer`; damaged `a_damaged_payload_is_refused_before_any_dialog`, `a_file_whose_hash_differs_from_the_plan_is_damaged`; unsafe file `a_manifest_with_an_unsafe_path_is_refused`, `a_files_zip_that_fails_the_strict_validator_is_unsafe`; bases differ `a_base_mismatch_is_answered_and_changes_nothing`, `each_abort_reason_ends_the_starter_with_its_outcome`; link missing `an_unknown_or_broken_link_is_link_missing`; folder or uid changed `start_refuses_with_the_reason_and_sends_nothing` (Task 2), `a_moved_collection_is_link_missing_and_broken`; the peer needs an update: a version 2 peer never gets a live connection (`run_socket` refuses it as `NeedsUpdate`), so `start` answers "That node is offline." (the offline case of `start_refuses_with_the_reason_and_sends_nothing`); conflict dialog cancelled `cancel_after_confirm_is_refused` (a cancel in the review declines); apply fails `an_apply_failure_rolls_back_and_goes_out_of_date`, `a_planner_apply_failure_tells_the_starter_and_goes_out_of_date`, `a_starter_apply_failure_marks_the_planner_out_of_date`.

Run: `cargo test -j4 -p rocket-app sync_safety_tests`
Expected: FAIL. The tests compile; the ones whose rule Task 2 did not write yet fail (for example `a_nested_collection_file_is_refused`, `follow_up_sync_offers_do_not_count_against_the_rate_window`, `unlinking_during_a_sync_cancels_it`). Note which fail in the ledger.

- [ ] **Step 4: Make every failure test pass.** Fix only what a failing test names, in the module the state table points to: the refusals and their order in `planner_on_manifest_offer` and the check task (P1 to P5), the follow-up offer rule (S5, S29), the watchdog outcomes (S20, S26, P13, P29), `cancel` and `unlink` (S17, `unlinking_during_a_sync_cancels_it`), the S7 link updates (S14, S16, S17, P21, P23), the S10 checks and their order (`Unsafe` before `Damaged` before `BadPlan`), and S11 in `review`. No new public name.

Run: `cargo test -j4 -p rocket-app sync_safety_tests`
Expected: PASS, 45 tests.

- [ ] **Step 5: Check the worker stays short (Review Focus 2) and the hooks.**

Run:
```bash
awk '/fn (handle_frame|handle_gone|on_payload_|planner_on_|starter_on_|on_follow_up_offer)/,/^    }$/' crates/rocket-app/src/network/sync*.rs | grep -n "\.await\|deps\.links\.\|deps\.collections\."
```
Expected: no output (the handlers only lock, decide and spawn; port calls sit inside the spawned closures, which this range does not reach because each spawn calls a named method).

Run:
```bash
grep -n "pub(crate) fn \(claim_transfer_for_test\|frames_seen\|gone_seen\|phase_of\)" -B1 crates/rocket-app/src/network/sync.rs
```
Expected: each line is preceded by `#[cfg(test)]`.

Run:
```bash
grep -rnE 'unwrap[(][)]' crates/rocket-app/src/network/sync*.rs crates/rocket-app/src/network/frame_router.rs
```
Expected: no output.

- [ ] **Step 6: Document.** In `crates/rocket-app/CLAUDE.md`, add two rows to the Public Types table after `TransferService`:
  - `SyncService`: "Keeps a linked collection in step with one paired node (`src/network/sync*.rs`). Links: `create_link` (at the first linked send, both ends, from the same ZIP bytes), `links` (derived state, blocks), `unlink`, removal when the pairing goes. A sync: the starter offers its manifest (`start`), the planner checks the link and `base_hash`, plans with `plan_sync`, asks for the files it needs, publishes `ReviewReady` (`review`, `conflict_files`), sends the final plan with the new base hash and its files; the starter checks everything, publishes `ConfirmNeeded` (`confirm`), applies first and reports; the planner applies after the starter's `applied`. Payloads ride the transfer machines, announced by `SyncOffer`, in memory, with their own `TransferLimits` (one session per peer, the pending cap, the per-peer start rate). Every received path, hash and size is checked before a dialog (spec section 7); a fresh manifest is compared before every apply; the base is computed locally and only its hash crosses the wire. An unconfirmed sync keeps the new base and marks the link `out_of_date`. Same worker design as `TransferService`. `new_with_timing` is the test seam."
  - `FrameRouter`: "The node's one `TransferSink`: `Sync*` frames and frames of a sync payload id from its own peer go to `SyncService`, the rest to `TransferService`; both get every peer-gone notice."
  - In the `TransferService` row add: "`send` takes `keep_in_sync`; a completed linked send creates the link on both ends (`set_sync`)."

- [ ] **Step 7: Format and lint.**

Run: `cargo fmt -p rocket-app -p rocket-network -p rocket-import`
Expected: no output.

Run: `cargo clippy -j4 -p rocket-app --tests -- -D warnings`
Expected: `Finished`, no warning.

- [ ] **Step 8: No commit; reviewed at the plan end.** Run the whole-plan verification:

```bash
cargo check -j4 -p rocket-network && cargo test -j4 -p rocket-network link \
  && cargo test -j4 -p rocket-import transfer_zip \
  && cargo check -j4 -p rocket-app \
  && cargo test -j4 -p rocket-app -- sync_tests sync_run_tests sync_safety_tests sync_checks \
  && cargo test -j4 -p rocket-app -- transfer_link_tests transfer_send_tests transfer_receive_tests e2e_transfer_tests e2e_tests \
  && cargo check -j4 -p rocket
```
Expected: every command ends with `test result: ok` or `Finished`, no failure. Then write `Task 3: complete` in `.superpowers/sdd/2026-11-08-sync-plan-03/progress.md`. The user reviews the plan's diff and commits through `dev-workflow-skills:1-git-commit` with explicit paths.
