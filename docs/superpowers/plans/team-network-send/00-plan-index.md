# Team network: send a collection — plan index

Spec: `docs/superpowers/specs/2026-11-06-team-network-send-collection-design.md` (binding). Previous work: the team network MVP, Plans 1 to 5 in `docs/superpowers/plans/team-network/`, all on `main`.

Run the plans in order. Each plan ends green (`cargo check -j4`, the named crate tests, `bun run tsc --noEmit`, `bun run check`, the named vitest files) before the next starts. No commit during a plan. The user reviews each plan, then commits through `dev-workflow-skills:1-git-commit` with explicit paths.

| # | File | Scope | Tasks |
|---|---|---|---|
| 1 | `2026-11-06-send-plan-01-protocol.md` | `rocket-network`: limits, version 2, six `PeerMessage` variants, transfer state machines with the ack window, `TransferSink`, `TransferEvent`; `rocket-audit`: two kinds with an `outcome` | 3 |
| 2 | `2026-11-06-send-plan-02-import-side.md` | `rocket-import` strict ZIP validator and `created_roots`, `rocket-infra` staging, `rocket-app` ports, `src-tauri` import adapter with a private temp dir | 3 |
| 3 | `2026-11-06-send-plan-03-service.md` | `NetworkService::send_to` and sink hook, `TransferService` sender and receiver | 3 |
| 4 | `2026-11-06-send-plan-04-tauri-e2e.md` | Tauri commands, event channel, wiring, end-to-end tests over the real transport | 3 |
| 5 | `2026-11-06-send-plan-05-frontend.md` | API wrappers, store slice, bridge, incoming dialog and toast, send dialog and menu item, audit rows, docs, manual checks | 3 |

## Cross-plan contract (names are binding; a plan that needs a change records a ruling in its ledger)

### Plan 1 produces (`crates/rocket-network`)

```rust
// limits.rs
pub const PROTOCOL_VERSION: u32 = 2;                 // v1 peers show NeedsUpdate and their pairing requests are refused
pub const MAX_TRANSFER_BYTES: u64 = 10 * 1024 * 1024;
pub const CHUNK_BYTES: usize = 32 * 1024;
pub const TRANSFER_OFFER_TTL: Duration = Duration::from_secs(120);
pub const MAX_PENDING_INCOMING: usize = 3;
pub const MAX_ACTIVE_PER_PEER: usize = 1;
pub const OFFERS_PER_PEER_PER_MINUTE: u32 = 5;
pub const MAX_UNZIPPED_BYTES: u64 = 50 * 1024 * 1024;
pub const MAX_ZIP_ENTRIES: usize = 5000;
pub const SENDER_RESULT_TIMEOUT: Duration = Duration::from_secs(60);
pub const ACK_EVERY_CHUNKS: u32 = 16;
pub const MAX_UNACKED_CHUNKS: u32 = 32;

// messages.rs: six new PeerMessage variants
// (type tags: collection_offer, offer_answer, chunk, transfer_abort, chunk_ack, transfer_result)
CollectionOffer { transfer_id: String, name: String, size_bytes: u64, chunk_count: u32, sha256: String }
OfferAnswer     { transfer_id: String, accept: bool }
Chunk           { transfer_id: String, seq: u32, data: String }          // data is base64 (standard alphabet, padded)
TransferAbort   { transfer_id: String, reason: AbortReason }
ChunkAck        { transfer_id: String, next_seq: u32 }                   // count of chunks accepted in order
TransferResult  { transfer_id: String, outcome: ResultOutcome }
#[serde(rename_all = "snake_case")] pub enum AbortReason { Cancelled, Failed, TooBig, Busy, Expired }
#[serde(rename_all = "snake_case")] pub enum ResultOutcome { Completed, Damaged, Rejected, Failed }

// transfer.rs (new, pure)
pub struct TransferId(String);   // 32 lowercase hex chars; TransferId::random(); TransferId::parse(&str) -> DomainResult<TransferId>; as_str()
pub fn chunk_count_for(size_bytes: u64) -> u32;          // ceil(size / CHUNK_BYTES), 0 bytes -> 0
pub fn offer_name_is_valid(name: &str) -> bool;          // not blank, <= 200 chars, no Cc, no bidi/zero-width format chars (spec section 6)
pub enum Direction { Send, Receive }                     // serde snake_case
pub enum Outcome { Completed, Unconfirmed, Declined, Expired, Cancelled, Failed, TooBig, Busy, Damaged, Rejected }  // serde snake_case
pub trait TransferSink: Send + Sync {
    fn on_frame(&self, from: DeviceId, message: PeerMessage);
    fn on_peer_gone(&self, device: DeviceId);
}
// state machines (plain structs, no I/O, time passed in as Instant): OutgoingTransfer, IncomingTransfer
//   OutgoingTransfer::window_open(&self) -> bool                         // sent - acked < MAX_UNACKED_CHUNKS
//   OutgoingTransfer::on_ack(&mut self, next_seq: u32) -> Option<Outcome> // Some(Failed) for a backwards or too-far ack
//   OutgoingTransfer::connection_lost / result_timed_out                 // Unconfirmed only while awaiting_result (true after the final ChunkAck), Failed before; result_timed_out is ignored before the final ack
//   OutgoingTransfer::fail_with(&mut self, Outcome) -> Option<Outcome>   // sender twin of the receiver's; works after Done
//   ChunkStep::Data { bytes, done_bytes, last, ack: Option<PeerMessage> } // ack is Some every 16th chunk and on the last
//   #[doc(hidden)] with_offer_ttl(self, Duration) -> Self                 // on both machines; default TRANSFER_OFFER_TTL

// events.rs: a second event enum on channel "network-transfer"
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TransferEvent {
    OfferIncoming { transfer_id, device_id, short_id, display_name, collection_name, size_bytes },
    Progress      { transfer_id, direction, done_bytes, total_bytes },
    Finished      { transfer_id, direction, outcome, display_name, collection_name, message: Option<String> },
}   // variant fields snake_case on the wire like NetworkEvent; message is null when none
```

`crates/rocket-audit`: `AuditEventKind::NetworkCollectionSent { device_id: String, collection: String, outcome: String }` (outcome `completed`, `unconfirmed` or `failed`) and `NetworkCollectionReceived { device_id: String, collection: String, outcome: String }` (outcome `completed`, `damaged` or `rejected`), tags `network_collection_sent`, `network_collection_received`, both under control `CC6.7`. `device_id` is the short id.

### Plan 2 produces

```rust
// rocket-import: pub mod transfer_zip (keeps its own MAX_ZIP_ENTRIES / MAX_UNZIPPED_BYTES, pinned equal in rocket-app)
pub fn validate(files: &dyn ImportFiles, path: &Path) -> ImportResult<()>;     // strict, spec section 8 (also ':', reserved names, trailing dot/space, Cc/Cf, case-folded duplicates)
// rocket-import: ImportService records the collection roots it created in this run
impl ImportService { pub fn created_roots(&self) -> Vec<PathBuf>; }           // filled before files are written, so an Err still reports them

// Staging types StagedFile, StagedZip, TransferStaging are defined in crates/rocket-network/src/staging.rs
// and re-exported from rocket_app::network::transfer_ports (rocket-infra cannot depend on rocket-app).
// rocket-app: crates/rocket-app/src/network/transfer_ports.rs
pub struct ImportedCollection { pub name: String }
pub struct StagedFile { pub path: PathBuf, pub len: u64, pub sha256: String }   // sha256 is lowercase hex
pub trait StagedZip: Send { fn append(&mut self, bytes: &[u8]) -> DomainResult<()>; fn finish(self: Box<Self>) -> DomainResult<StagedFile>; fn discard(self: Box<Self>); }
pub trait TransferStaging: Send + Sync { fn create(&self, transfer_id: &str) -> DomainResult<Box<dyn StagedZip>>; fn purge(&self) -> DomainResult<()>; fn release(&self, file: &StagedFile) -> DomainResult<()>; }
pub trait CollectionImportPort: Send + Sync {
    fn existing_names(&self) -> DomainResult<Vec<String>>;
    fn import_zip(&self, zip_path: &Path, display_name: &str) -> DomainResult<ImportedCollection>;   // copies to <private dir>/<display_name>.zip, validates the copy, imports it, never overwrites; on failure removes only created_roots()
}
pub fn receiver_display_name(offered: &str, sender: &str, existing: &[String]) -> String;   // offered, else "<offered> (from <sender>)"

// rocket-infra: FsTransferStaging::new(dir: PathBuf)    // implements TransferStaging
// src-tauri: ImporterAdapter::new(active_workspace_path: Arc<Mutex<PathBuf>>, temp_root: PathBuf)  // temp_root = <network dir>/import-tmp
//            ImporterAdapter::purge_temp(&self) -> DomainResult<()>  // removes only stale per-process dirs (lock file free, or no lock file and older than this process)
```

### Plan 3 produces (`crates/rocket-app/src/network`)

```rust
impl NetworkService {
    pub async fn send_to(&self, device: &DeviceId, message: PeerMessage) -> DomainResult<()>;   // 2 s bounded send; InvalidInput("That node is offline.") when no connection
    pub fn set_transfer_sink(&self, sink: Arc<dyn TransferSink>);
    pub fn peer_label(&self, device: &DeviceId) -> Option<(String, String)>;                     // (display name, short id) of a paired device
}
// run_socket: every transfer frame counts as a pong; Chunk and ChunkAck skip the per-frame block-list read.
pub struct TransferDeps { pub network: Arc<dyn PeerSender>, pub export: Arc<CollectionExportService>, pub importer: Arc<dyn CollectionImportPort>, pub staging: Arc<dyn TransferStaging>, pub events: Arc<dyn EventPublisher>, pub audit: Arc<dyn SecurityAuditPublisher> }
pub struct SendStarted { pub transfer_id: String, pub notes: Vec<String> }
impl TransferService {
    pub fn new(deps: TransferDeps) -> Arc<TransferService>;            // registers itself as the sink
    #[doc(hidden)] pub fn new_with_timing(deps: TransferDeps, timing: Timing) -> Arc<TransferService>;   // Timing { offer_ttl, idle_limit, result_timeout }
    pub async fn send(&self, device: &DeviceId, collection: &str, include_environments: bool) -> DomainResult<SendStarted>;
    pub async fn respond(&self, transfer_id: &str, accept: bool) -> DomainResult<()>;
    pub async fn cancel(&self, transfer_id: &str) -> DomainResult<()>;   // either direction; a pending incoming offer is declined
    // Test hooks, #[cfg(test)] pub(crate) only (the e2e tests live in the same crate):
    // offer_raw(&self, device, name, bytes) -> DomainResult<String>, set_chunk_delay(&self, Duration), set_tamper(&self, Option<u32>)
}
```

### Plan 4 produces

Tauri commands (also in `src-tauri/app-commands.txt` and `generate_handler!`): `network_send_collection(device_id: String, collection: String, include_environments: bool) -> Result<SendStartedDto, DomainError>` with `SendStartedDto { transfer_id, notes }` (camelCase: `transferId`, `notes`), `network_respond_transfer(transfer_id: String, accept: bool) -> Result<(), DomainError>`, `network_cancel_transfer(transfer_id: String) -> Result<(), DomainError>`. Event channel `network-transfer` carries `TransferEvent` JSON. Startup purges the staging dir and calls `ImporterAdapter::purge_temp`, logging any error with `tracing::warn!` (no unwrap).

### Plan 5 consumes

The three commands and the `network-transfer` channel above. TypeScript names: `sendNetworkCollection(deviceId, collection, includeEnvironments): Promise<{ transferId: string; notes: string[] }>`, `respondNetworkTransfer(transferId, accept): Promise<void>`, `cancelNetworkTransfer(transferId): Promise<void>`, `onNetworkTransfer(handler)`, types `NetworkTransferEvent` (`finished` carries `message: string | null`), `TransferOutcome` (includes `'unconfirmed'`), `TransferDirection`; store: `IncomingTransfer`, `ActiveTransfer`, `transfers` slice in `network-store`; components `SendCollectionDialog`, `IncomingTransferDialog`, `IncomingTransferToast`; hook `useIncomingTransferNotice`. Audit kinds `network_collection_sent` and `network_collection_received` carry `outcome`.

## Plan file rules

- Header and sections as in the MVP plans: goal, architecture, tech stack, spec, Global Constraints, Review Focus (each line with its test in the owning task), File Structure table, then at most 3 tasks.
- Every task: Files, Interfaces (Consumes and Produces), then checkbox steps in RED, GREEN order. Test steps carry the full test code. Code steps carry the full code unless the signature and tests fully determine the body. Every command step has an `Expected:` line.
- First step of any task that touches collections, import, the trust gate or YAML: `📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.`
- Last step of the last task of each plan: `No commit; reviewed at the plan end.` plus the plan's whole verification command.
- Cargo commands always `-j4`, one crate at a time. Bun for the frontend.

## Rulings after the cross-plan review (these override any older text in the plans)

1. **Result frame (now one of six; see R2 for `ChunkAck`).** `PeerMessage::TransferResult { transfer_id: String, outcome: ResultOutcome }` (type tag `transfer_result`), `ResultOutcome` is a snake_case enum `Completed | Damaged | Rejected | Failed`. The receiver sends it after the last chunk and after the import finished. The sender keeps its transfer open until that frame arrives, up to `SENDER_RESULT_TIMEOUT = 60 s` (new constant in `limits.rs`), then reports the real outcome and writes `NetworkCollectionSent` for `Completed` (and, per R4 as amended, for `Unconfirmed` and for a stream lost before the final ack as `Failed`). The 3 s `SENDER_RESULT_GRACE` is removed. `TransferAbort` stays for early aborts and cancel. A sender `cancel` after the last chunk is refused with InvalidInput("Already sent. Waiting for the result.").
2. **`TransferSink` has two methods:** `on_frame(&self, from: DeviceId, message: PeerMessage)` and `on_peer_gone(&self, device: DeviceId)`. `NetworkService` calls `on_peer_gone` when `run_socket` ends for a device and from `remove_local`. The receiver then runs `connection_lost()` on that peer's incoming transfers and the sender fails that peer's outgoing transfers. The 120 s idle limit stays as a fallback and is a service FIELD (not a const) that the test seam can shorten.
3. **Test seam.** `TransferService::new_with_timing(deps, Timing { offer_ttl, idle_limit, result_timeout })` (`#[doc(hidden)]`): the service stores `offer_ttl` and calls `.with_offer_ttl(ttl)` on every machine it builds, and the timers use the same values.
4. **`TransferEvent::Finished`** gains `message: Option<String>` (snake_case key `message`, `null` when none). The receiver puts the importer's failure text there. TypeScript: `message: string | null`.
5. **Send command return.** `network_send_collection` returns a camelCase DTO `SendStartedDto { transfer_id: String, notes: Vec<String> }` (serde renames to `transferId`, `notes`); `notes` are the human lines of `ExportReport` items for what the export dropped or blanked. TypeScript `sendNetworkCollection(...) : Promise<{ transferId: string; notes: string[] }>`. The send dialog shows the notes after Send.
6. **Cancel in the UI.** The "Sent to <name>. Waiting for them to accept." toast has a Cancel action that calls `cancelNetworkTransfer`. The incoming toast has only a Review action (no "Later").
7. **`ImporterAdapter::new(active_workspace_path: Arc<Mutex<PathBuf>>, temp_root: PathBuf)`** and it builds its own `ImportServiceFactory`. Plan 4 passes `Arc::clone(&active_workspace_path)` and `network_dir.join("import-tmp")`.
8. **Import failure cleanup (amended by R3 and R5).** A failed import removes exactly the collection folders the importer reports in `created_roots()`. Temp ZIP copies live in the adapter's private per-process dir under the network dir, are deleted on every exit path, and only stale dirs are purged at app start together with the staging dir (`purge`).
9. **Display name cap by BYTES** (file systems cap names at 255 bytes): the receiver display name is at most 120 bytes after the ` (from <sender>)` suffix, cut on a character boundary.
10. **`send` order.** Check the peer is online (`send_to` precheck via `PeerSender::is_online`) before exporting. An empty collection is refused.
11. **Worker design.** `on_frame` stays synchronous and cheap. `finalize` (import) and every outbound `tell` run as their own spawned tasks, never inline in the single frame worker, so a long import cannot back up other peers' chunks.
12. **Superseded by second-review ruling R2.** Pings and chunks still share the bounded outbound channel, but the ack window keeps at most 32 chunk frames in it and every inbound transfer frame counts as a pong.

## Rulings after the second review (these override any older text, including the rulings above)

Sources: the coverage, edge and reality reviews of 2026-11-07. "Cost if wrong" is what happens if the ruling turns out to be the wrong call.

1. **R1 Scripts (edge C1, reality I-1).** No script gate is built. A received collection has no trust record, so it gets no Developer mode, extra script folders, agent runs, host env or vault access until the user allows them; its scripts DO run in the Safe sandbox on the first Send. Spec decision 6, section 3 and section 8, the Plan 5 incoming dialog warning, the Plan 5 feature doc and manual check 8 say exactly this. The offer has no script count and nothing is unpacked before Accept, so the dialog shows the warning text only. The Plan 2 and Plan 4 trust fixture uses `extensions:\n  rocketapi:\n    sandboxMode: developer` (the real key, `crates/rocket-infra/src/fs_collection/settings.rs`) and first proves the requested mode reads back as `Developer`, so the `Safe` assertion can fail. Cost if wrong: a hostile teammate's script can call `rok.sendRequest` with Safe-sandbox data on the first Send; a script gate becomes its own project.
2. **R2 Flow control (edge I1).** Sixth frame `ChunkAck { transfer_id, next_seq: u32 }` (tag `chunk_ack`; `next_seq` matches `Chunk.seq`). The receiver acks after every `ACK_EVERY_CHUNKS = 16` chunks accepted in order and after the last chunk. The sender keeps at most `MAX_UNACKED_CHUNKS = 32` chunks unacked and waits otherwise; no ack for the idle limit (120 s default, `Timing.idle_limit`) ends it `Failed` with `TransferAbort { failed }`. A backwards or too-far ack is `Failed`. Any inbound transfer frame refreshes presence (counts as a pong). Every older variant count in the plans now says six. Cost if wrong: about one ack round trip per 512 KiB slows a fast LAN slightly.
3. **R3 Cleanup (edge I4).** `ImportService::created_roots()` reports the exact folders the importer created in this run, recorded before any file is written so an `Err` still has them. A failed transfer import removes only those. The collection-list diff is gone, and the old ruling that tolerated it is withdrawn. Test: a collection created concurrently by someone else survives a failed import. Cost if wrong: a root created by the importer but not recorded stays behind as a half-written collection the user deletes by hand.
4. **R4 Unconfirmed (edge I2).** `Outcome::Unconfirmed` (wire `unconfirmed`). The sender ends with it only when the receiver's final `ChunkAck` (the ack for the last chunk) was received and then the peer goes away or `SENDER_RESULT_TIMEOUT` passes without a result. A peer that vanishes, or a stream that stalls, before the final ack is `Failed` ("Sending failed.") even when the last chunk was already sent; the result timer starts only at the final ack, and the idle limit guards the wait for it. User decision, amends the earlier rule. Sender text "Sent, but the other node did not confirm.", shown as an info toast (not an error). `NetworkCollectionSent` is written with `outcome: "unconfirmed"`. `NetworkCollectionSent` is written with `outcome: "failed"` for the lost-before-final-ack case. Cost if wrong: the user may resend and get `Orders (from Ana)` next to `Orders`.
5. **R5 Private temp dir (edge I5).** The adapter copies the staged ZIP into its own per-process dir (`tempfile` random name, mode 0700) under `<network dir>/import-tmp/`, holding a `lock` file locked with `std::fs::File::try_lock` (std since Rust 1.89; the toolchain is 1.99) for the process lifetime. `validate` and the root check run on that copy, which is the file imported. `purge_temp` removes a dir only when its lock can be taken (owner gone) or it has no lock file and is older than this process start; never its own or a live instance's dir. Cost if wrong: on a filesystem without advisory locks a stale dir stays until the next start that can lock it.
6. **R6 Audit (edge I6).** Both kinds carry `outcome`. Receiver writes `network_collection_received` for `completed`, `damaged` and `rejected`; sender writes `network_collection_sent` for `completed`, `unconfirmed` and (stream lost after the last chunk, before the final ack, see R4) `failed`. Plan 5 shows the outcome in the audit row label. Cost if wrong: a few more audit rows per hostile peer.
7. **R7 Lost frames (edge I3).** No retransmit. A dropped chunk fails at the next chunk (sequence rule, both sides `Failed` at once). A stalled stream (lost tail, socket swapped by the duplicate rule, worker queue overflow) ends `Failed` within the idle limit on whichever side waits for a frame; a sender that sent the last chunk but got no final `ChunkAck` ends `Failed` (peer gone at once, a stall at the idle limit), and only a sender whose final ack arrived ends `Unconfirmed` by R4. A socket swap is not treated as peer gone. Plan 3 tests `a_dropped_chunk_fails_both_sides_at_the_next_chunk` and `a_stalled_stream_fails_both_sides_within_the_idle_limit`. The receiver can still show "Receiving failed." after 120 s when a chunk is lost, and the sender now shows "Sending failed." in that case (before, a sender past its last chunk showed "Sent, but the other node did not confirm."). Cost if wrong: a flaky link waits up to 120 s before the failure shows; the sender reports a false "Sending failed" when the receiver got everything but the final ack itself was lost, and a retry may then import a duplicate (`Orders (from Ana)` next to `Orders`).
8. **R8 Test hooks (coverage I1, reality M-7).** `offer_raw`, `set_chunk_delay` and `set_tamper` are `#[cfg(test)] pub(crate)`, with any field they need also `#[cfg(test)]`. This works because the e2e tests are `crates/rocket-app/src/network/e2e_tests.rs` and `e2e_transfer_tests.rs`, inside `rocket-app`. `new_with_timing` stays `#[doc(hidden)] pub`. No `test-support` cargo feature. Cost if wrong: none at runtime; a future out-of-crate test needs a feature then.
9. **R9 Cancel tests (coverage I2).** Plan 3 tests a `TransferAbort { cancelled }` for a pending offer (Finished Cancelled, counts back to (0,0), slot freed) and the receiver's own cancel of a pending offer (sends `OfferAnswer { accept: false }`, Finished Declined). Plan 4 adds both cases end to end.
10. **R10 Environments off (coverage I3).** Plan 3 tests `send(.., false)` with an environment file in the fake source and asserts no `environments/` entry in the offered ZIP; Plan 4 does the same end to end; Plan 5 keeps the dialog default `false` test.
11. **R11 Manual check 12 (coverage I4).** It now checks a second Send to the same busy peer ("A transfer to that node is already running."). The `busy` toast is covered only by the automated `the_fourth_pending_offer_is_told_busy`.
12. **R12 Keychain path (reality M-1).** No automated test drives `env_secret_store` (it touches the real OS keychain). Manual check 13 pushes a collection with environments and a secret variable and checks the value is blank and no keychain prompt appears. Cost if wrong: a keychain prompt or error on the first environment import, found by the manual check.
13. **R13 Block check per frame (reality M-2).** `Chunk` and `ChunkAck` skip the per-frame `is_blocked` YAML read; every other frame keeps it. A block during a transfer unpairs, closes the socket and ends the transfer through `on_peer_gone`. Plan 3 tests both. Cost if wrong: none found; a blocked peer's chunks after the block can only reach a socket that is already closing.
14. **R14 Size check (coverage M6).** After `finish` the receiver compares `StagedFile.len` with `size_bytes` as well as the SHA-256; either mismatch is `Damaged` ("The collection arrived damaged and was not imported."). Tested in Plan 3.
15. **R15 Names (coverage M8, edge M3, M4).** The offer name rule (not blank, at most 200 chars, no Cc, no bidi/zero-width chars) is now in the spec. A send with a bad name fails with "This collection name cannot be sent. Rename it and try again." (tested). The receiver folder name maps Windows reserved names (append `_`), replaces `:` and trims trailing dots and spaces; the ZIP entry check refuses `:`, reserved names, trailing dot or space, Cc/Cf characters and case-folded duplicates. Not done: NFC normalisation before the clash check (no unicode crate in the tree). Cost if wrong: on APFS an NFD twin gets the importer's `-1` suffix instead of `(from <sender>)`.
16. **R16 ZIP limits copy (coverage M5).** `rocket-import` keeps its own copy of the two ZIP limits; `the_zip_limits_equal_the_protocol_limits` in `rocket-app` pins them. The spec section 7 now says so.
17. **R17 Startup purge (coverage M1, reality M-8, edge M6).** `lib.rs` calls one helper `purge_transfer_temp(&staging, &importer)` that logs each error with `tracing::warn!`; a unit test in `src-tauri` calls the helper and checks both stale dirs are gone, and a grep step checks `lib.rs` calls it.
18. **R18 Small fixes.** Plan 1 stale test comment fixed (coverage M3); Plan 4 Task 3 gets a real failing step and the broken grep is replaced (coverage M2); the receive-side `too_big` text is removed (coverage M9); the design-system page step is removed from Plan 5 (coverage M10); `cargo fmt` uses `-p` everywhere (reality M-5); Plan 5 stages only its own hunk of `.claude/docs-index.md` with `git add -p` (reality M-6); Plan 1 and the feature doc state that version 2 makes v1 peers show NeedsUpdate and refuses their pairing (reality M-10).
19. **R19 UI minors (edge M1, M2, M11).** The receiving view has a Cancel; a peer cancel shows "<name> cancelled the transfer." on the sender (a local cancel shows nothing). A pending offer dropped by a disconnect ends with `Finished { outcome: failed, message: "The other node went offline." }` and the receiver shows that message. The sender's waiting toast switches to "Sending to <name>..." on the first `progress` and drops Cancel once the last chunk is sent.

Not fixed, on purpose:

20. **Webview reload mid-offer (edge M12).** No list command. After a full reload the pending offer is not shown; it expires after 2 minutes and the sender sees "The request expired." Cost if wrong: a user who reloads must ask the sender to resend.
21. **Crash during import (edge M5).** A quit or crash mid-import can leave a half-written collection; there is no startup sweep of the workspace. Cost if wrong: the user deletes one partial collection by hand.
22. **Linked external collections (edge M7).** `existing_names` lists only `<workspace>/collections`. Cost if wrong: a second "Orders" in the sidebar, no data lost.
23. **Workspace switch mid-transfer (edge M8).** The collection lands in the workspace active when the import starts; cleanup uses `created_roots`, so it is correct whichever workspace that is. Cost if wrong: the user moves it.
24. **Suffix race (edge M9).** Two imports finalizing at once may give `Orders-1` instead of `Orders (from Ben)`. Cost if wrong: a cosmetic name.
25. **Block during import (edge M10).** An import already past `final_view` finishes. Cost if wrong: one collection from a just-blocked peer, still untrusted per R1.
26. **No log test (coverage M7).** No test asserts that nothing about the payload is logged at info level; review checks it. Cost if wrong: a collection name in an info log.
27. **Worker queue overflow (reality M-3).** The queue stays 256 with `try_send`; with the ack window one peer has at most 32 chunk frames in flight, and an overflow fails safe by R7. Cost if wrong: a rare `Failed` under many simultaneous peers.
28. **Reference cycle (reality M-4).** `NetworkService` and `TransferService` hold each other for the app lifetime. Cost if wrong: none, both are singletons.
29. **DSM line references (reality M-9).** Moot, the design-system step is removed.
