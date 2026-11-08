# Team network: keep a sent collection in sync — plan index

Spec: `docs/superpowers/specs/2026-11-08-team-network-sync-collection-design.md` (binding). Previous work: the team network MVP (`docs/superpowers/plans/team-network/`) and send a collection (`docs/superpowers/plans/team-network-send/`), both on `main`.

Run the plans in order. Each plan ends green (`cargo check -j4`, the named crate tests, `bun run tsc --noEmit`, `bun run check`, the named vitest files) before the next starts. No commit during a plan. The user reviews each plan, then commits through the `dev-workflow-skills:1-git-commit` skill, staging explicit paths only (never `git add -A` or `.`). Subjects use conventional commits (`feat(network): ...`, `test(network): ...`, `docs(network): ...`).

| # | File | Scope | Tasks | Model |
|---|---|---|---|---|
| 1 | `2026-11-08-sync-plan-01-domain.md` | `rocket-network`: manifest, `base_hash`, link types, the three-way planner with renames and conflict resolution, protocol version 3, three `Sync*` messages, `keep_in_sync` on the offer, `SyncEvent`, `LinkRepository` port; `rocket-audit`: three kinds | 3 | Sonnet |
| 2 | `2026-11-08-sync-plan-02-storage-merge.md` | `rocket-export`: the YAML file merge (decision 11); `rocket-infra`: `FsLinkRepo`; `rocket-app`: `SyncCollectionPort` ports and the manifest builder over the export | 3 | Sonnet |
| 3 | `2026-11-08-sync-plan-03-service.md` | `rocket-app` `SyncService` (links, sync sessions on the transfer machines, planning, decisions, apply), the frame router, link creation at the first send in `TransferService` | 3 | Opus |
| 4 | `2026-11-08-sync-plan-04-tauri-e2e.md` | Tauri commands, DTOs, the `network-sync` event channel, wiring, end-to-end tests over the real transport | 3 | Sonnet |
| 5 | `2026-11-08-sync-plan-05-frontend.md` | API wrappers, store slice, bridge, review and confirm dialogs, conflict diff, menu items, link list, docs and manual checks | 3 | Sonnet |

## Deviations from the spec (decided while planning)

1. **Three new messages, not five.** The spec lists `SyncOffer`, `SyncAbort`, `SyncPlan`, `SyncDecision` and `SyncResult`. A manifest or a plan of 5000 paths does not fit the 64 KiB frame cap (`MAX_FRAME_BYTES`). So every payload (manifest, plan with decisions, files ZIP) travels as a transfer on the existing machines (`OutgoingTransfer`, `IncomingTransfer`, `Chunk`, `ChunkAck`, `OfferAnswer`, `TransferAbort`, `TransferResult`), announced by one `SyncOffer`. The new variants are `SyncOffer`, `SyncAbort` and `SyncResult`. Cost if wrong: none, the wire is new in this version.
2. **The manifest travels without a prompt.** The receiving node accepts a `SyncOffer` of kind `manifest` on its own, within the existing caps (pending-incoming cap, per-peer rate, one sync per link), because it carries only paths and hashes. The user is asked after planning (the review dialog), which is where the spec asks. Cost if wrong: a paired peer can make a node compute a plan. It writes nothing.
3. **Both bases are computed from the same bytes at the first send.** Sender and receiver each hash the entries of the ZIP that was sent (path with the single root folder stripped). So `base_hash` is equal on both sides without a message to agree on it.

## Cross-plan contract (names are binding; a plan that needs a change records a ruling in its ledger)

### Plan 1 produces (`crates/rocket-network`, `crates/rocket-audit`)

```rust
// limits.rs
pub const PROTOCOL_VERSION: u32 = 3;
pub const MAX_MANIFEST_ENTRIES: usize = 5000;        // equals MAX_ZIP_ENTRIES
pub const SYNC_PAYLOAD_NAME_MAX: usize = 32;

// link.rs (pure)
pub struct LinkId(String);       // 32 lowercase hex; LinkId::random(); LinkId::parse(&str) -> DomainResult<LinkId>; as_str()
pub struct SyncId(String);       // same shape as LinkId, one per sync run
pub struct Manifest(BTreeMap<String, String>);   // relative path ('/' separated) -> lowercase hex SHA-256
impl Manifest {
    pub fn new() -> Manifest;
    pub fn insert(&mut self, path: &str, sha256: &str) -> DomainResult<()>;   // rejects bad paths and bad hashes (rules in the spec, section 7), at most MAX_MANIFEST_ENTRIES
    pub fn get(&self, path: &str) -> Option<&str>;
    pub fn paths(&self) -> impl Iterator<Item = &str>;
    pub fn len(&self) -> usize;
    pub fn base_hash(&self) -> String;       // SHA-256 over the sorted lines "<path> NUL <hash> LF", lowercase hex
    pub fn to_json(&self) -> DomainResult<Vec<u8>>;      // {"entries":[{"path":..,"sha256":..}]} sorted by path
    pub fn from_json(bytes: &[u8]) -> DomainResult<Manifest>;
}
#[serde(rename_all = "snake_case")] pub enum LinkState { InSync, ChangesWaiting, OutOfDate, Broken }
pub struct LinkRecord {        // persistence struct, snake_case serde
    pub link_id: LinkId, pub peer_device_id: DeviceId, pub collection_name: String,
    pub collection_root: String, pub collection_uid: Option<String>,
    pub base: Manifest, pub state: LinkState, pub updated_at: String,   // RFC 3339
}
impl LinkRecord { pub fn base_hash(&self) -> String; }

// planner.rs (pure)
pub enum SyncAction {                   // what the PLANNING node does; "mine" is the planning node
    Pull { path: String },                         // take theirs: merge their file into mine (new file: write as received)
    PullAs { path: String, as_path: String },      // keep both: their file is written at as_path, mine stays
    Push { path: String },                         // send mine
    DeleteMine { path: String }, DeleteTheirs { path: String },
    RenameMine { from: String, to: String }, RenameTheirs { from: String, to: String },
}
pub enum ConflictKind { BothChanged, DeleteVsEdit, BothAdded, RenameVsEdit }
pub struct Conflict { pub path: String, pub kind: ConflictKind, pub mine: Option<String>, pub theirs: Option<String> }  // hashes; None = deleted or absent
#[serde(rename_all = "snake_case")] pub enum Resolution { KeepMine, KeepTheirs, KeepBoth }
pub struct Decision { pub path: String, pub resolution: Resolution }
pub struct SyncPlan { pub actions: Vec<SyncAction>, pub conflicts: Vec<Conflict> }
pub fn plan_sync(base: &Manifest, mine: &Manifest, theirs: &Manifest) -> SyncPlan;      // the table and rename rules of spec section 6, deterministic order (sorted by path)
pub fn resolve(plan: &SyncPlan, decisions: &[Decision], taken: &Manifest) -> DomainResult<Vec<SyncAction>>;
    // every conflict needs exactly one decision (else InvalidInput); KeepBoth picks as_path "<stem> (from peer)<ext>", then " 2", " 3"... until it is not in `taken`
pub fn new_base(base: &Manifest, mine: &Manifest, theirs: &Manifest, actions: &[SyncAction], decisions: &[Decision]) -> Manifest;
    // the manifest both sides store after a successful apply: for every path that exists after the sync, the agreed post-sync hash (the winner's hash; for KeepBoth the original path takes mine and as_path takes theirs); deleted paths are absent

// messages.rs additions to PeerMessage
SyncOffer  { sync_id: String, link_id: String, base_hash: String, kind: SyncPayloadKind, transfer_id: String, size_bytes: u64, chunk_count: u32, sha256: String }
SyncAbort  { sync_id: String, reason: SyncAbortReason }
SyncResult { sync_id: String, outcome: SyncOutcome }
#[serde(rename_all = "snake_case")] pub enum SyncPayloadKind { Manifest, Plan, Files }
#[serde(rename_all = "snake_case")] pub enum SyncAbortReason { BaseMismatch, LinkMissing, Busy, Cancelled, Declined, Expired, Failed, TooBig }
#[serde(rename_all = "snake_case")] pub enum SyncOutcome { Applied, Failed }
// CollectionOffer gains: #[serde(default)] keep_in_sync: bool, #[serde(default)] link_id: Option<String>
// The Plan payload is JSON {"wanted":[path..],"deletes":[path..],"renames":[{"from":..,"to":..}],"decisions":[{"path":..,"resolution":..}]}: type SyncPlanPayload { to_json, from_json } in link.rs.

// events.rs: a third event enum on channel "network-sync" (camelCase tag, snake_case fields, helper DTOs camelCase)
pub enum SyncEvent {
    ReviewReady   { sync_id, link_id, device_id, short_id, display_name, collection_name, summary: SyncSummary, conflicts: Vec<ConflictView>, role: SyncRole },
    ConfirmNeeded { sync_id, link_id, device_id, short_id, display_name, collection_name, summary: SyncSummary },
    Progress      { sync_id, done_bytes, total_bytes },
    Finished      { sync_id, link_id, display_name, collection_name, outcome: SyncFinish, message: Option<String> },
    LinkChanged   { link_id },
}
// SyncSummary { added_here, changed_here, deleted_here, sent_added, sent_changed, sent_deleted, scripts_changed: Vec<String>, auth_changed: Vec<String> } (counts u32, lists are paths)
// ConflictView { path, kind, mine_hash: Option<String>, theirs_hash: Option<String> }
// SyncRole { Planner, Starter } ; SyncFinish { Applied, Declined, Expired, Cancelled, Failed, BaseMismatch, LinkMissing, Unconfirmed }

// ports.rs
pub trait LinkRepository: Send + Sync {
    fn list(&self) -> DomainResult<Vec<LinkRecord>>;
    fn get(&self, id: &LinkId) -> DomainResult<Option<LinkRecord>>;
    fn save(&self, record: &LinkRecord, base_files: &[(String, Vec<u8>)]) -> DomainResult<()>;   // writes link.yml and the base copies atomically
    fn delete(&self, id: &LinkId) -> DomainResult<()>;
    fn read_base_file(&self, id: &LinkId, path: &str) -> DomainResult<Option<Vec<u8>>>;
}
```

`rocket-audit`: `AuditEventKind::{NetworkSyncSent, NetworkSyncReceived, NetworkLinkRemoved}`, tags `network_sync_sent`, `network_sync_received`, `network_link_removed`. Fields: `device_id: String`, `collection: String`, `outcome: String` for the first two; `device_id`, `collection` for the third. The reader tolerates unknown kinds at startup (done: `AuditLogRepository::head_hash`).

### Plan 2 produces

```rust
// rocket-export: pub mod sync_merge
pub fn merge_file(local: Option<&[u8]>, received: &[u8]) -> ExportResult<Vec<u8>>;
    // spec decision 11: None -> received as is; else the peer's value for every key, except (a) a local non-blank scalar is never replaced by a blank/empty received scalar, (b) every key the export removes is kept from local: top-level `uid`, `info.uid`, `extensions.rocketapi`, the trust-gate keys, `examples`. Output is valid YAML; invalid input -> Err and nothing written.
// rocket-infra: FsLinkRepo::new(dir: PathBuf)   // implements rocket_network::LinkRepository; <dir>/<link_id>/link.yml + base/<path>; atomic_write; a damaged link.yml is renamed link.yml.corrupt-<secs> and the link reads as Broken
// rocket-app: crates/rocket-app/src/network/sync_ports.rs
pub struct SyncFile { pub path: String, pub bytes: Vec<u8> }
pub enum FileChange { Put { path: String, bytes: Vec<u8> }, PutAs { path: String, bytes: Vec<u8> }, Delete { path: String }, Rename { from: String, to: String } }
pub trait SyncCollectionPort: Send + Sync {
    fn exists(&self, collection: &str) -> DomainResult<bool>;
    fn collection_uid(&self, collection: &str) -> DomainResult<Option<String>>;
    fn manifest(&self, collection: &str) -> DomainResult<Manifest>;                                  // runs the export in memory and hashes each file, paths without the root folder
    fn export_files(&self, collection: &str, paths: &[String]) -> DomainResult<Vec<SyncFile>>;
    fn apply(&self, collection: &str, changes: &[FileChange]) -> DomainResult<()>;                   // under collection_lock, one CollectionFileTx, Put merges via merge_file, PutAs writes as received, rolls back on any error
}
pub fn manifest_from_zip(zip_path: &Path) -> DomainResult<Manifest>;      // the base at the first send: entries of the sent ZIP, single root folder stripped
```

### Plan 3 produces (`crates/rocket-app/src/network`)

```rust
pub struct SyncDeps { pub network: Arc<dyn PeerSender>, pub links: Arc<dyn LinkRepository>, pub collections: Arc<dyn SyncCollectionPort>, pub events: Arc<dyn EventPublisher>, pub audit: Arc<dyn AuditPublisher>, pub staging: Arc<dyn TransferStaging> }
impl SyncService {
    pub fn new(deps: SyncDeps) -> Arc<SyncService>;
    #[doc(hidden)] pub fn new_with_timing(deps: SyncDeps, timing: Timing) -> Arc<SyncService>;
    pub fn create_link(&self, record: LinkRecord, base_files: Vec<(String, Vec<u8>)>) -> DomainResult<()>;   // called by TransferService at the first send, both sides
    pub async fn start(&self, link_id: &LinkId) -> DomainResult<SyncId>;                                      // starter side
    pub async fn review(&self, sync_id: &SyncId, accept: bool, decisions: Vec<Decision>) -> DomainResult<()>;   // planner side
    pub async fn confirm(&self, sync_id: &SyncId, accept: bool) -> DomainResult<()>;                          // starter side
    pub async fn cancel(&self, sync_id: &SyncId) -> DomainResult<()>;
    pub fn links(&self) -> DomainResult<Vec<LinkView>>;
    pub async fn unlink(&self, link_id: &LinkId) -> DomainResult<()>;
}
// frame router: crates/rocket-app/src/network/frame_router.rs: FrameRouter implements TransferSink; NetworkService.set_transfer_sink(router). Routes SyncOffer/SyncAbort/SyncResult and any frame whose transfer_id belongs to a sync session to SyncService, all else to TransferService.
// TransferService::send gains: keep_in_sync: bool. When true it makes a LinkId, puts link_id and keep_in_sync on CollectionOffer, and on Completed result creates the link (sender: manifest_from_zip of the sent ZIP); the receiver creates its link after a successful import (same function on the received ZIP).
```

### Plan 4 produces

Tauri commands (also in `src-tauri/app-commands.txt` and `generate_handler!`): `network_send_collection(device_id, collection, include_environments, keep_in_sync: bool)` (extends the existing command), `network_sync_start(link_id: String) -> SyncStartedDto { sync_id }`, `network_sync_review(sync_id: String, accept: bool, decisions: Vec<DecisionDto>)`, `network_sync_confirm(sync_id: String, accept: bool)`, `network_sync_cancel(sync_id: String)`, `network_list_links() -> Vec<LinkDto>`, `network_unlink(link_id: String)`. `DecisionDto { path, resolution }`, `LinkDto { linkId, deviceId, displayName, shortId, collectionName, state, updatedAt }` (camelCase). Event channel `network-sync` carries `SyncEvent`.

### Plan 5 consumes

The commands and the channel above. TypeScript: `startNetworkSync(linkId)`, `reviewNetworkSync(syncId, accept, decisions)`, `confirmNetworkSync(syncId, accept)`, `cancelNetworkSync(syncId)`, `listNetworkLinks()`, `unlinkNetworkLink(linkId)`, `onNetworkSync(handler)`; `sendNetworkCollection(deviceId, collection, includeEnvironments, keepInSync)`.

## Plan file rules

- Header and sections as in the MVP and send plans: goal, architecture, tech stack, spec, Global Constraints, Review Focus (each line with its test in the owning task), File Structure table, then at most 3 tasks.
- Every task: Files, Interfaces (Consumes and Produces), then checkbox steps in RED, GREEN order. Test steps carry the full test code. Code steps carry the signature and the values the spec pins, and a body only for an algorithm the signature and tests do not determine. Every command step names the command and the expected output.
- First step of any task that touches collections, import, the trust gate or YAML: `📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.`
- Last step of the last task of each plan: `No commit; reviewed at the plan end.` plus the plan's whole verification command.
- Cargo commands always `-j4`, one crate at a time. Bun for the frontend.
- No `unwrap` in production paths and none in new Rust text a hook checks; tests use `expect("reason")`. Mutex locks use `.lock().unwrap_or_else(|e| e.into_inner())`.
- Persistence structs snake_case; camelCase only on IPC DTOs. Event enums: tag camelCase, fields snake_case, helper DTOs camelCase, pinned by `wire_contract_*` tests.
- A new Tauri command goes into `generate_handler!` and into `src-tauri/app-commands.txt` (sorted).
- Line numbers drift. Anchor every edit on quoted text.

## Ledger

Keep `.superpowers/sdd/2026-11-08-sync-plan-NN/progress.md` per the repo harness rules, one `Ruling:` line per decision made while implementing.

## Rulings after the cross-plan review (these override any older text in the index and in the plans)

Source: the five plan writers' reports of 2026-11-08, where the plans disagreed with the index or with each other. "Cost if wrong" is what happens if the ruling turns out to be the wrong call.

1. **R1 Conflict content (plans 3, 4, 5).** The planner needs the starter's file text before its user decides. Plan 3 ruling S1 stands: two Plan payloads, a request (`wanted` only) and then the final plan, with a `Files` payload for the conflict files between them. `SyncService::conflict_files(&self, sync_id: &SyncId, path: &str) -> DomainResult<ConflictFiles>` with `ConflictFiles { mine: Option<String>, theirs: Option<String> }` (UTF-8 text, a file that is not valid UTF-8 gives `None`). Plan 4 adds `network_sync_conflict_files(sync_id, path) -> ConflictFilesDto { mine, theirs }` (camelCase, in `app-commands.txt`). Plan 5 uses it. Cost if wrong: the diff shows no text and the three choices still work.
2. **R2 Plan payload fields (plans 1, 3).** `SyncPlanPayload` gains `sends: Vec<PlanFile>` (`PlanFile { path, sha256 }`) and `new_base_hash: Option<String>`, both `#[serde(default)]`. The starter recomputes its new base itself and compares the hash, so the base is never read from the peer. Plan 1 owns the type. `new_base(...)` is used by the planner; the starter builds its base from its own post-apply manifest.
3. **R3 Types in `SyncDeps`.** `audit` is `Arc<dyn SecurityAuditPublisher>` (the trait `TransferDeps` already uses). `staging` is dropped, because payloads stay in memory (at most 10 MiB, one session per peer).
4. **R4 First-send base (plans 2, 3, 4).** Plan 2 owns the base functions: `base_from_reader(reader) -> DomainResult<ZipBase>` and `base_from_zip(files: &dyn ImportFiles, path: &Path) -> DomainResult<ZipBase>` with `ZipBase { manifest: Manifest, files: Vec<(String, Vec<u8>)> }`. `manifest_from_zip` in this index is replaced by them. Plan 3 uses `base_from_reader(Cursor)` for the sender's in-memory ZIP and drops its own `base_from_zip(&[u8])`. All of them drop paths that fail `is_synced_path`.
5. **R5 Port additions (plans 2, 3).** `SyncCollectionPort` gains `collection_root(&self, collection: &str) -> DomainResult<String>` and `apply_if_unchanged(&self, collection: &str, expected: &Manifest, changes: &[FileChange]) -> DomainResult<()>` (checks the manifest inside the collection lock and returns `Conflict` when it differs). Plan 3 uses `apply_if_unchanged` for every apply. `FileChange` derives `Debug, Clone, PartialEq`. All port methods block, so Plan 3 calls them from `spawn_blocking`.
6. **R6 How `TransferService` reaches `SyncService` (plans 3, 4).** Through `TransferService::set_sync(&self, sync: Arc<SyncService>)`, not a `TransferDeps` field. Wiring order in `lib.rs` (Plan 4 follows this): `TransferService::new`, `SyncService::new`, `transfer.set_sync(sync)`, `FrameRouter::new(transfer, sync)`, `network.set_transfer_sink(router)`. `TransferService::send` gets `keep_in_sync: bool` as its last parameter.
7. **R7 Names.** The port implementor is `ExportBackedSyncCollections` (Plan 2), not `FsSyncCollections`. `LinkView` is defined by Plan 3 with the fields of `LinkDto` (`link_id`, `device_id`, `short_id`, `display_name`, `collection_name`, `state`, `updated_at`). Plan 4 uses it.
8. **R8 Casing.** Helper DTOs inside events and IPC (`SyncSummary`, `ConflictView`, `ConflictFilesDto`) are camelCase (`mineHash`, `addedHere`, `scriptsChanged`). Event variant fields stay snake_case. Plan 1 follows this where it wrote `mine_hash`.
9. **R9 Link repository semantics (plans 2, 3).** `LinkRepository::save(record, base_files)`: `base_files` is a delta. A stored copy whose path is still in `record.base` is kept, so a state-only save passes `&[]`. A copy whose path left `record.base` is pruned. Each `(path, bytes)` must hash to the value in `record.base`, else `InvalidInput`. A damaged `link.yml` is quarantined and replaced by a stub (state `Broken`, empty base, zero device id, name `Unknown collection`); Plan 3 treats a zero device id as an unknown device.
10. **R10 Local-only keys.** The export removes `extensions.rocketapi` whole, so the local-only set in decision 11 is `uid`, `info.uid`, `extensions.rocketapi` and `examples`. Trust grants live in `~/.rocket-api/trust.yml`, outside the collection. A received `uid`, `info.uid`, `extensions.rocketapi` or `examples` is ignored on a merge into an existing file.
11. **R11 One path filter.** `is_synced_path` (Plan 2) decides which paths can be in a manifest and which can be applied. `environments/`, `flows/`, hidden folders, `workspace.yml`, files other than yml, yaml, proto and js, and any path with `..` are refused, so a peer cannot overwrite the receiver's environments.
12. **R12 Rename markers (plan 1).** `plan_sync` leaves the rename action of a `RenameVsEdit` conflict in `plan.actions` as a marker, and `resolve` replaces it. Only the output of `resolve` may be applied, and summary counts ignore marker renames. Keep both emits `RenameTheirs`, `PullAs` and `Push`; apply runs renames, then deletes, then copies.
13. **R13 Unlink on unpair (plan 3).** Removing a pairing deletes the links of that device on that side, owned by `SyncService` (spec section 5).
14. **R14 ZIP size.** The 10 MiB cap is on the ZIP bytes, as in the first send. The spec wording "10 MiB of changed files" means a ZIP of at most 10 MiB.
15. **R15 Plan 1 touches `rocket-app`.** The three new `PeerMessage` variants and the two new `CollectionOffer` fields break `rocket-app` (the `run_socket` match, `handle_frame`, struct literals in fakes and tests), so Plan 1 edits those sites to stay green. Plan 3 does not repeat them.
16. **R16 Progress.** `SyncEvent::Progress` is emitted for the `Files` payloads only, with totals that cover the whole sync, so Cancel does not flicker. `ReviewReady.role` is `Planner` for the review dialog and `Starter` is never used on it (the starter gets `ConfirmNeeded`).

Open for the user, not decided here:

17. **Unconfirmed apply (Plan 3 ruling S7).** The starter applies first and the planner applies only after the starter's `applied`. If the starter's result was sent and no answer came, the starter keeps the new base and marks the link out of date. If the planner in fact failed, the next sync ends in a base mismatch and the user must unlink and send again. Cost if wrong: one unlink and resend after a rare double failure.
