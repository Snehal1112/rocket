# Team Network Send Plan 04: Tauri Commands, Wiring and End-to-End Tests

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 4 of 5, title "Team network send: Plan 4, Tauri and end to end".** Previous: `2026-11-06-send-plan-03-service.md` (01 protocol, 02 import side and 03 service must be applied and green). Next: `2026-11-06-send-plan-05-frontend.md`. Recommended model: Sonnet. Index (binding names): `docs/superpowers/plans/team-network-send/00-plan-index.md`. No commit during the plan.

**Goal:** A user can push a collection from the app. Three Tauri commands (`network_send_collection`, `network_respond_transfer`, `network_cancel_transfer`) drive `TransferService`, transfer events reach the webview on the `network-transfer` channel, and the service is wired at startup with the staging folder purged. Then the whole feature is proven over the real transport: two real nodes on loopback push a real Bruno ZIP, and the failure and safety cases (decline, cancel of a pending offer by either side, expiry, dropped connection, a receiver gone after the final ack, a slow receiver that keeps the ack window, tampered chunk, unsafe ZIP, size limit, busy, secret leak, a send without environments) each end as the spec says.

**Architecture:**
- `src-tauri` gets three thin commands. They parse the device id with `parse_device_id`, call `TransferService` through `State<'_, Arc<TransferService>>` and return its `DomainError` unchanged. `network_send_collection` maps the service's `SendStarted` to the camelCase `SendStartedDto { transferId, notes }`. `lib.rs` builds the service after the network service, from the shared export service, `ImporterAdapter::new(Arc::clone(&active_workspace_path), network_dir.join("import-tmp"))` (its private per-process temp dir; stale dirs are purged at start), `FsTransferStaging` under `~/.rocket-api/network/transfers`, the `TauriEventBus` and the audit publisher. The channel `network-transfer` is not added to any table: `TauriEventBus` takes the channel from `PublishedEvent::channel()` and adds `seq` and `tsMs`, so a bus test pins it.
- The end-to-end tests are a second unit-test module in `rocket-app`, `network/e2e_transfer_tests.rs`, next to `e2e_tests.rs` and reusing its real two-node harness (`online_pair`, `wait_until`, `pair`). Each node gets a real `TransferService`, a real `CollectionExportService` over an in-memory `CollectionFileSource` and a real `FsTransferStaging` in a temp folder. **Only the import is faked** (`RecordingImport`), because the shipped `ImporterAdapter` lives in `src-tauri` and `rocket-app` must not depend on it. The fake runs the real strict ZIP validator (`rocket_import::transfer_zip::validate`) before it records the call, so an unsafe ZIP is refused exactly as in the app. A separate `src-tauri` test proves the real adapter gives a received collection no trust record, so it runs in the Safe sandbox with Developer mode, host env and vault denied (index R1).
- Real sockets mean real time (the existing e2e note: a paused clock breaks real IO). So offer expiry is shortened through Plan 03's `TransferService::new_with_timing(deps, Timing { offer_ttl, idle_limit, result_timeout })`, and slow or hostile senders come from Plan 03's test hooks on `TransferService` (`offer_raw`, `set_chunk_delay`, `set_tamper`). Index ruling R8: the hooks are `#[cfg(test)] pub(crate)`, which works because these e2e tests are unit-test modules inside `rocket-app` (`network/e2e_tests.rs`, `network/e2e_transfer_tests.rs`); there is no `test-support` feature for them. This plan adds no hook of its own. A slow receiver is a test-only staging wrapper (`SlowStaging`), not a hook. Dropped connections end through `on_peer_gone` (the socket ended or the pairing was removed), so the drop and unpair tests keep the production idle limit (120 s) and still finish well inside `wait_until`'s 20 s: no test waits for an idle timeout. The ack window (index R2) is checked end to end with a slow receiver; the idle-limit and liveness rules themselves need paused time and stay in Plan 03.
- The sender reports `Completed` only when the receiver's `TransferResult` frame arrives (after the import). The e2e tests pin that with a receiver import that is held open, and pin the other outcomes (`Damaged`, `Rejected`, `Failed` with the importer's message) as the sender sees them. A receiver that goes away after its final `ChunkAck` leaves the sender `Unconfirmed` (index R4 as amended), audited with `outcome: "unconfirmed"`. A peer lost before that ack is `Failed`, covered by Plan 03 service tests (the final-ack timing cannot be forced over a real socket).

**Tech Stack:** Rust, Tauri 2, tokio (real time), `rocket-network`, `rocket-app`, `rocket-infra` (`FsTransferStaging`, `test-support`), `rocket-export`, `rocket-import`, `zip` 2, `sha2`, `tempfile`.

**Spec:** `docs/superpowers/specs/2026-11-06-team-network-send-collection-design.md`, sections 2 (decisions 3, 4 and 6), 3, 5, 6, 7, 8, 9 (`src-tauri`), 10 and 11 (end to end, secret check).

Prerequisites: Plans 01 to 03 of this series are applied. Names used from them (all from the index contract; confirm the module paths in Step 0b of Task 1 because the index fixes names, not file paths):
- `rocket_network`: `TransferEvent` (variants `OfferIncoming`, `Progress`, `Finished`, where `Finished` has `message: Option<String>`), `Direction`, `Outcome` (with `Unconfirmed`), `limits::{TRANSFER_OFFER_TTL, SENDER_RESULT_TIMEOUT, MAX_PENDING_INCOMING, MAX_TRANSFER_BYTES, CHUNK_BYTES, MAX_UNACKED_CHUNKS}`.
- `rocket_app`: `TransferService` (`new`, `new_with_timing`, `send -> DomainResult<SendStarted>`, `respond`, `cancel`, and the `#[cfg(test)] pub(crate)` hooks `offer_raw`, `set_chunk_delay`, `set_tamper`), `Timing`, `SendStarted { transfer_id, notes }`, `TransferDeps`, `CollectionImportPort`, `ImportedCollection`, `TransferStaging`, `StagedZip`, `UNSAFE_ZIP_MESSAGE`, and `NetworkService::peer_label`. `TransferSink::on_peer_gone` and `PeerSender::is_online` are called by the service, not by this plan.
- `rocket_infra`: `FsTransferStaging::new(dir)`.
- `rocket_import::transfer_zip::validate(files: &dyn ImportFiles, path)`, called with `&rocket_infra::FsImportFiles`.
- `src-tauri`: `ImporterAdapter::new(active_workspace_path: Arc<Mutex<PathBuf>>, temp_root: PathBuf)` (it builds its own `ImportServiceFactory`) and `ImporterAdapter::purge_temp(&self) -> DomainResult<()>` (removes only stale per-process dirs, index R5).
- `rocket_audit`: `AuditEventKind::{NetworkCollectionSent, NetworkCollectionReceived}`, each `{ device_id, collection, outcome: String }` (index R6).

## Global Constraints

- `-j4` on every cargo command, one crate at a time. Never `cargo test --workspace` or `--all`. The `src-tauri` package is named `rocket` (`src-tauri/Cargo.toml`), so use `cargo check -j4 -p rocket` and `cargo test -j4 -p rocket <filter>`.
- No `unwrap()` in production code. Tests use `expect("reason")` (a repo hook rejects a bare unwrap call, tests included). Mutex locks use `.lock().unwrap_or_else(|e| e.into_inner())` in new helpers.
- Never hold a `std::sync::Mutex` guard across an `.await`.
- Commands stay thin: validate, call the service, map the error. No transfer logic in `commands/network.rs`. A new command goes into `src-tauri/app-commands.txt` (sorted) and `generate_handler!`, or it is denied at runtime.
- `#[serde(rename_all = "camelCase")]` only on IPC DTOs. This plan adds one: `SendStartedDto { transfer_id, notes }`, the return of `network_send_collection`, pinned by a `keys()` test. The other two commands return `()`. The event payload keeps its own wire format: camelCase tag, snake_case variant fields. Do not rename.
- Test hooks on `TransferService` (`offer_raw`, `set_chunk_delay`, `set_tamper`) are `#[cfg(test)] pub(crate)` (index R8), with any field they need also `#[cfg(test)]`. Nothing in a release build can flip a byte or offer raw bytes. `new_with_timing` stays `#[doc(hidden)] pub`.
- `cargo fmt` always names the crate: `cargo fmt -p <crate> -- --check`. Never `--all`, never a bare `cargo fmt`.
- Logs carry codes and counts, never collection names, ZIP bytes or full device ids.
- Code comments are short, full sentences that end with a period. No emojis.
- Every wait in the e2e tests goes through `wait_until` (20 s limit), never a fixed sleep. The only fixed sleeps are the three "nothing happened" checks (Task 2, sender waits for the result, and Task 3, over-limit and empty collection), and the assertions after them hold whether or not the sleep was long enough. The slow receiver's `std::thread::sleep` in `SlowStaging::append` is the simulated slow disk, not a wait.
- No commit during the plan. The user reviews it at the end, then commits through `dev-workflow-skills:1-git-commit` with explicit paths (suggested split: `feat(network): send collection commands and wiring`, `test(network): end-to-end send tests`).
- Line numbers drift. Anchor every edit on quoted text.

## Review Focus

Each line names the test that pins it.

1. **The receiver app closes mid-transfer and restarts.** Staged chunk files must not survive. Task 3: `a_restart_purges_files_a_crash_left_behind` (a staged file is left on disk on purpose, then the same wiring as `lib.rs` runs `purge()`), and `a_dropped_receiver_fails_the_sender_and_a_restart_leaves_no_staged_files`.
2. **Two nodes push to each other at the same time.** One transfer per direction per peer is allowed, so both must complete without mixing the two transfer ids. Task 2: `two_nodes_pushing_to_each_other_at_once_both_complete`.
3. **A collection name with spaces and unicode.** The offered name, the import display name and the ZIP root folder keep it. Task 2: `a_name_with_spaces_and_unicode_survives_the_trip`.
4. **A very small collection.** One file, one chunk, still verified and imported. Task 2: `a_one_file_collection_travels_in_a_single_chunk`.
5. **A transfer starts while the peer unpairs.** The unpair closes the socket mid-stream. Both sides must finish without importing and staging must be empty. Task 3: `unpairing_during_a_transfer_ends_both_sides_and_imports_nothing`.

6. **The sender says "Sent" before the receiver finished importing.** The sender must stay open until the receiver's `TransferResult`, and write the `NetworkCollectionSent` audit only for `Completed`. Task 2: `the_sender_shows_sent_only_after_the_receivers_result`; a damaged payload gives the sender `Damaged`, not a generic failure: Task 3 `a_tampered_chunk_is_damaged_and_nothing_is_imported`.
7. **A peer vanishes and nobody waits for an idle timeout.** The drop and unpair tests run with the 120 s production idle limit and end through `on_peer_gone`. Task 3: `a_dropped_sender_fails_the_receiver_and_clears_staging`, `unpairing_during_a_transfer_ends_both_sides_and_imports_nothing`.
8. **The receiver vanishes after it acknowledged the last chunk.** The sender must not say "Sending failed": it ends `Unconfirmed` and audits `NetworkCollectionSent { outcome: "unconfirmed" }`. Task 3: `a_receiver_gone_after_the_final_ack_leaves_the_sender_unconfirmed`.
9. **A slow receiver.** The sender may never be more than the ack window ahead. Task 3: `a_slow_receiver_keeps_the_sender_inside_the_ack_window`.
10. **A pending offer is cancelled.** By the sender: the receiver ends `Cancelled` and a late accept fails. By the receiver: the sender ends `Declined`. Task 2: `the_sender_cancels_a_pending_offer_and_the_receiver_closes_it`, `the_receiver_cancels_a_pending_offer_and_the_sender_sees_declined`.
11. **"Include environments" is off.** No `environments/` entry reaches the receiver although the source has one. Task 3: `a_send_without_environments_carries_no_environment_file`.
12. **The startup purge is really called.** Task 1: `the_startup_purge_clears_staging_and_stale_temp_dirs` plus a grep that `lib.rs` calls `purge_transfer_temp`.

Also pinned: the `network-transfer` channel, the `seq`/`tsMs` envelope and the `message` key of `Finished` (Task 1), the `SendStartedDto` keys (Task 1), the secret check, the unsafe ZIP and a failing import with its message (Task 3), the receiver audits `damaged` and `rejected` (Task 3), and a received ZIP that asks for Developer mode gets no trust record (Task 3, `src-tauri`).

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `src-tauri/src/commands/network.rs` | modify | Three transfer commands, `SendStartedDto`, ACL list test, DTO keys test |
| `src-tauri/app-commands.txt` | modify | Add the three command names (sorted) |
| `src-tauri/src/lib.rs` | modify | `generate_handler!`, build and manage `TransferService`, call `import_wiring::purge_transfer_temp` at start, keep `Arc` clones of the export and network services |
| `src-tauri/src/tauri_event_bus.rs` | modify | Tests: `network-transfer` channel, `seq` and `tsMs` envelope, `message` key |
| `crates/rocket-app/src/lib.rs` | modify (only if needed) | Re-export the transfer types the Tauri crate names (`SendStarted` too) |
| `crates/rocket-app/src/network/mod.rs` | modify | Register `e2e_transfer_tests` (test builds only) |
| `crates/rocket-app/src/network/e2e_tests.rs` | modify | Make the harness helpers `pub(super)` (the file holds 8 older tests; they are not changed) |
| `crates/rocket-app/src/network/test_support.rs` | modify | `RecordingEvents` also records `TransferEvent` |
| the files of `TransferService` (Plan 3) | none | The seam (`new_with_timing`, `Timing`, and the `#[cfg(test)] pub(crate)` hooks `offer_raw`, `set_chunk_delay`, `set_tamper`) comes from Plan 03 and is only confirmed here |
| `crates/rocket-app/src/network/e2e_transfer_tests.rs` | create | Send harness, fakes (`RecordingImport`, `SlowStaging`), fixtures and the end-to-end tests |
| `src-tauri/src/import_wiring.rs` | modify | `purge_transfer_temp` startup helper and its test (Task 1); test: a received ZIP that asks for Developer mode is imported with no trust record (Task 3) |

---

### Task 1: Tauri commands, event channel and startup wiring

**Files:**
- Modify: `src-tauri/src/commands/network.rs`, `src-tauri/app-commands.txt`, `src-tauri/src/lib.rs`, `src-tauri/src/tauri_event_bus.rs`, `src-tauri/src/import_wiring.rs` (the startup purge helper and its test)
- Modify only if a name is not yet exported: `crates/rocket-app/src/lib.rs`

**Interfaces:**
- Consumes: `TransferService::{send, respond, cancel}` with `send(&self, device: &DeviceId, collection: &str, include_environments: bool) -> DomainResult<SendStarted>` and `pub struct SendStarted { pub transfer_id: String, pub notes: Vec<String> }` (Plan 03; `notes` are the human lines for what the export skipped or blanked), `TransferDeps`, `FsTransferStaging::new`, `ImporterAdapter::new(Arc<Mutex<PathBuf>>, PathBuf)` and `ImporterAdapter::purge_temp(&self) -> DomainResult<()>` (Plan 02), `TransferEvent::channel()` (through `PublishedEvent`), the existing `parse_device_id`, `NetworkService`, `audit_publisher`, `export_svc`, `import_wiring::ImportServiceFactory`.
- Produces (binding, from the index ruling 5): `network_send_collection(device_id: String, collection: String, include_environments: bool) -> Result<SendStartedDto, DomainError>` with `#[serde(rename_all = "camelCase")] pub struct SendStartedDto { pub transfer_id: String, pub notes: Vec<String> }` (wire keys `transferId`, `notes`), `network_respond_transfer(transfer_id: String, accept: bool) -> Result<(), DomainError>`, `network_cancel_transfer(transfer_id: String) -> Result<(), DomainError>`. Managed state `Arc<TransferService>`. Event channel `network-transfer` carrying `TransferEvent` JSON plus `seq` and `tsMs`. `pub(crate) fn purge_transfer_temp(staging: &dyn rocket_app::TransferStaging, importer: &ImporterAdapter)` in `import_wiring.rs` (index R17).

Rules the tests pin:
- `TransferEvent` events leave on exactly one channel, `network-transfer`, with `seq` and `tsMs` added to a payload whose tag is camelCase and whose variant fields stay snake_case.
- `SendStartedDto` serializes to exactly the keys `notes` and `transferId`.
- `Finished` carries a `message` key (`null` when none) in the wire format.
- The three command names are in the ACL list and the handler list (the existing `app_commands_file_matches_generate_handler` and `app_commands_file_is_sorted_and_unique` stay green).
- The startup purge removes a staged file a crash left and the temp dir of an instance that exited, logs every error with `tracing::warn!` and never unwraps; `lib.rs` calls it.

- [ ] **Step 0: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 0b: Confirm the names this task imports**

Run:

```bash
grep -rn "pub struct TransferDeps\|pub struct TransferService\|pub trait TransferStaging\|pub trait CollectionImportPort" crates/rocket-app/src
grep -n "Transfer\|CollectionImportPort\|ImportedCollection" crates/rocket-app/src/lib.rs crates/rocket-app/src/network/mod.rs
grep -rn "pub struct FsTransferStaging" crates/rocket-infra/src
grep -rn "pub struct ImporterAdapter\|impl ImporterAdapter\|pub fn purge_temp" src-tauri/src
grep -rn "pub struct SendStarted" crates/rocket-app/src
grep -n "TransferEvent\|Direction\|Outcome" crates/rocket-network/src/lib.rs
```

Expected: each type is found once, `ImporterAdapter::new` takes `(Arc<Mutex<PathBuf>>, PathBuf)` and `purge_temp` exists and returns `DomainResult<()>`. Also note the name of the lock file Plan 2 writes in each per-process temp dir (index R5 calls it `lock`); the purge test below uses that name. Note the module paths. If `rocket-app/src/lib.rs` re-exports `NetworkService` through `pub use network::{...}` but does not list `SendStarted`, `TransferDeps`, `TransferService`, `TransferStaging` and `CollectionImportPort`, add them to that `pub use network::{ ... }` list in Step 3a. If `rocket_network` does not re-export `TransferEvent`, `Direction` and `Outcome` at its crate root, use the module path in the tests below (for example `rocket_network::events::TransferEvent`). Every snippet below uses the crate-root path.

- [ ] **Step 1: Write the failing tests**

(a) In `src-tauri/src/tauri_event_bus.rs`, inside `mod tests`, add the import `use rocket_network::{Direction, Outcome, TransferEvent};` next to the other `use` lines, and add these tests after `envelope_adds_seq_and_ts_ms_to_the_event_object`:

```rust
    #[test]
    fn transfer_events_emit_on_the_network_transfer_channel() {
        let progress = TransferEvent::Progress {
            transfer_id: "t1".into(),
            direction: Direction::Send,
            done_bytes: 10,
            total_bytes: 100,
        };
        assert_eq!(emit_channels(&progress), vec!["network-transfer"]);
        let finished = TransferEvent::Finished {
            transfer_id: "t1".into(),
            direction: Direction::Receive,
            outcome: Outcome::Completed,
            display_name: "Ann".into(),
            collection_name: "Orders".into(),
            message: None,
        };
        assert_eq!(emit_channels(&finished), vec!["network-transfer"]);
    }

    #[test]
    fn transfer_events_carry_seq_and_ts_ms_and_keep_snake_case_fields() {
        let event = TransferEvent::Finished {
            transfer_id: "t1".into(),
            direction: Direction::Receive,
            outcome: Outcome::Declined,
            display_name: "Ann".into(),
            collection_name: "Orders".into(),
            message: None,
        };
        let value = envelope(&event, 7, 1234).expect("envelope");
        assert_eq!(value["type"], "finished");
        assert_eq!(value["seq"], 7);
        assert_eq!(value["tsMs"], 1234);
        assert_eq!(value["direction"], "receive");
        assert_eq!(value["outcome"], "declined");
        assert!(value["message"].is_null(), "no message is a JSON null");
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "collection_name",
                "direction",
                "display_name",
                "message",
                "outcome",
                "seq",
                "transfer_id",
                "tsMs",
                "type"
            ]
        );
    }

    #[test]
    fn a_receiver_failure_message_travels_in_the_finished_event() {
        let event = TransferEvent::Finished {
            transfer_id: "t1".into(),
            direction: Direction::Receive,
            outcome: Outcome::Failed,
            display_name: "Ann".into(),
            collection_name: "Orders".into(),
            message: Some("The import failed.".into()),
        };
        let value = envelope(&event, 8, 99).expect("envelope");
        assert_eq!(value["message"], "The import failed.");
        assert_eq!(value["outcome"], "failed");
    }
```

(b) In `src-tauri/src/commands/network.rs`, inside `mod tests`, add after `device_ids_parse_or_are_invalid_input`:

```rust
    #[test]
    fn the_transfer_commands_are_in_the_acl_list() {
        let list = include_str!("../../app-commands.txt");
        for name in [
            "network_send_collection",
            "network_respond_transfer",
            "network_cancel_transfer",
        ] {
            assert!(
                list.lines().any(|line| line.trim() == name),
                "{name} is missing from app-commands.txt"
            );
        }
    }

    #[test]
    fn the_send_started_dto_uses_camel_case_keys() {
        let dto = SendStartedDto::from(SendStarted {
            transfer_id: "t1".to_string(),
            notes: vec!["flow: skipped".to_string()],
        });
        let value = serde_json::to_value(&dto).expect("json");
        assert_eq!(keys(&value), vec!["notes", "transferId"]);
        assert_eq!(value["transferId"], "t1");
        assert_eq!(value["notes"][0], "flow: skipped");
    }
```

(c) In `src-tauri/src/import_wiring.rs`, inside `mod tests` (Plan 2 already imports `Arc`, `Mutex` and `ImporterAdapter` there), add:

```rust
    #[test]
    fn the_startup_purge_clears_staging_and_stale_temp_dirs() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let staging = rocket_infra::FsTransferStaging::new(root.path().join("transfers"));
        let mut left = rocket_app::TransferStaging::create(&staging, "0123456789abcdef0123456789abcdef")
            .expect("staged file");
        left.append(b"half a zip").expect("append");
        // A crash never runs finish or discard.
        std::mem::forget(left);

        // The temp dir of an instance that exited: its lock file exists and nobody holds it.
        let temp_root = root.path().join("import-tmp");
        let stale = temp_root.join("4242-stale");
        std::fs::create_dir_all(&stale).expect("stale dir");
        std::fs::write(stale.join("lock"), b"").expect("stale lock file");
        std::fs::write(stale.join("Orders.zip"), b"old copy").expect("stale copy");
        let ws = tempfile::TempDir::new().expect("workspace");
        let adapter =
            ImporterAdapter::new(Arc::new(Mutex::new(ws.path().to_path_buf())), temp_root);

        purge_transfer_temp(&staging, &adapter);

        let staged_left = std::fs::read_dir(root.path().join("transfers"))
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert_eq!(staged_left, 0, "no staged file survives the start");
        assert!(!stale.exists(), "the temp dir of an exited instance is removed");
    }
```

Use the lock file name found in Step 0b if Plan 2 chose another one.

- [ ] **Step 2: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket the_send_started_dto_uses_camel_case_keys`

Expected: FAIL to compile with "cannot find struct `SendStartedDto` in this scope" and "cannot find function `purge_transfer_temp` in this scope". Once Step 3 adds the DTO, `cargo test -j4 -p rocket the_transfer_commands_are_in_the_acl_list` still FAILS with "network_send_collection is missing from app-commands.txt" until Step 4. The bus tests (`cargo test -j4 -p rocket transfer_events`) already PASS if Plan 1 is applied (it adds `message` to `Finished`), because the channel comes from `TransferEvent::channel()`. That is the point of the pin: it fails only if someone changes the channel or the wire format. If they fail to compile, fix the `rocket_network` import path from Step 0b.

- [ ] **Step 3: Add the three commands**

In `src-tauri/src/commands/network.rs`, extend the first `use rocket_app::{...}` to add `TransferService`:

```rust
use rocket_app::{
    NetworkHandle, NetworkService, NetworkState, NetworkStatus, NodeGroup, NodeStatus, NodeView,
    PairingDecision, PairingStarted, SendStarted, TransferService,
};
```

Add this block after `network_unblock` and before the `// ----- the running node -----` line:

```rust
// ----- send a collection -----

/// What the UI gets after a push started: the transfer id and one line per item the export
/// dropped or blanked.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendStartedDto {
    pub transfer_id: String,
    pub notes: Vec<String>,
}

impl From<SendStarted> for SendStartedDto {
    fn from(started: SendStarted) -> Self {
        Self {
            transfer_id: started.transfer_id,
            notes: started.notes,
        }
    }
}

/// Starts a push of one collection to a paired node. Returns the transfer id and the notes.
#[tauri::command]
pub async fn network_send_collection(
    device_id: String,
    collection: String,
    include_environments: bool,
    svc: State<'_, Arc<TransferService>>,
) -> Result<SendStartedDto, DomainError> {
    let id = parse_device_id(&device_id)?;
    let started = svc.send(&id, &collection, include_environments).await?;
    Ok(SendStartedDto::from(started))
}

/// Accepts or declines an incoming offer.
#[tauri::command]
pub async fn network_respond_transfer(
    transfer_id: String,
    accept: bool,
    svc: State<'_, Arc<TransferService>>,
) -> Result<(), DomainError> {
    svc.respond(&transfer_id, accept).await
}

/// Cancels a transfer from either side.
#[tauri::command]
pub async fn network_cancel_transfer(
    transfer_id: String,
    svc: State<'_, Arc<TransferService>>,
) -> Result<(), DomainError> {
    svc.cancel(&transfer_id).await
}
```

- [ ] **Step 3a: Re-export the types from `rocket-app` (only if Step 0b found them missing)**

In `crates/rocket-app/src/lib.rs`, add the missing names to the existing list:

```rust
pub use network::{
    paired_limit_message, CollectionImportPort, ImportedCollection, NetworkDeps, NetworkHandle,
    NetworkService, NetworkState, NetworkStatus, SendStarted, TransferDeps, TransferService,
    TransferStaging,
    // keep the names that are already in this list
};
```

Run: `cargo check -j4 -p rocket-app`

Expected: finished with no errors.

- [ ] **Step 4: Register the commands**

In `src-tauri/app-commands.txt`, keep the file sorted. The network block becomes:

```
network_add_address
network_cancel_pairing
network_cancel_transfer
network_list_addresses
network_list_blocked
network_list_nodes
network_mark_intro_seen
network_remove_address
network_remove_paired
network_request_pairing
network_respond_pairing
network_respond_transfer
network_send_collection
network_set_profile
network_set_visibility
network_status
network_unblock
```

In `src-tauri/src/lib.rs`, in `generate_handler!`, after the line `commands::network::network_unblock,` add:

```rust
            commands::network::network_send_collection,
            commands::network::network_respond_transfer,
            commands::network::network_cancel_transfer,
```

- [ ] **Step 5: Build the service at startup**

In `src-tauri/src/lib.rs`, make three edits, each anchored on quoted text.

(1) The export service is managed before the network block, and `TransferService` needs it too. Change

```rust
            app.manage(export_svc);
```

to

```rust
            app.manage(Arc::clone(&export_svc));
```

(2) Replace

```rust
            app.manage(network_svc);
            app.manage(commands::network::NetworkSlot::<rocket_app::NetworkHandle>::default());
```

with

```rust
            // Team network send: one transfer service on top of the network service. `new`
            // registers it as the network service's transfer sink. Files are staged under
            // ~/.rocket-api/network/transfers and anything a crash left there is deleted now.
            let transfer_staging = Arc::new(rocket_infra::FsTransferStaging::new(
                network_dir.join("transfers"),
            ));
            // The adapter copies a received ZIP into its own private temp dir while it imports.
            // A crash can leave a dir behind, so stale ones go at start with the staging folder.
            let importer = Arc::new(import_wiring::ImporterAdapter::new(
                Arc::clone(&active_workspace_path),
                network_dir.join("import-tmp"),
            ));
            import_wiring::purge_transfer_temp(transfer_staging.as_ref(), importer.as_ref());
            let transfer_svc = rocket_app::TransferService::new(rocket_app::TransferDeps {
                network: Arc::clone(&network_svc),
                export: Arc::clone(&export_svc),
                importer,
                staging: transfer_staging,
                events: Arc::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
                audit: audit_publisher.clone(),
            });
            app.manage(network_svc);
            app.manage(transfer_svc);
            app.manage(commands::network::NetworkSlot::<rocket_app::NetworkHandle>::default());
```

(3) In `src-tauri/src/import_wiring.rs`, after `impl CollectionImportPort for ImporterAdapter { .. }` and before `#[cfg(test)] mod tests`, add the helper that `lib.rs` calls:

```rust
/// Startup cleanup for the team network send: staged chunk files and the adapter's stale
/// temp dirs. A failure is logged and the start goes on.
pub(crate) fn purge_transfer_temp(
    staging: &dyn rocket_app::TransferStaging,
    importer: &ImporterAdapter,
) {
    if let Err(err) = staging.purge() {
        tracing::warn!(code = "transfer_staging_purge_failed", error = %err, "startup");
    }
    if let Err(err) = importer.purge_temp() {
        tracing::warn!(code = "transfer_temp_purge_failed", error = %err, "startup");
    }
}
```

(4) `ImporterAdapter::new(active_workspace_path: Arc<Mutex<PathBuf>>, temp_root: PathBuf)` is the Plan 02 signature (index ruling 7; it builds its own `ImportServiceFactory`, which only holds the shared path, so the adapter and the import commands both follow a workspace switch). `purge_temp` removes only stale per-process dirs (index R5), so a second running instance keeps its copy. `export_svc` is already an `Arc<CollectionExportService>` (it is built with `Arc::new(...)` in `lib.rs`), so `Arc::clone(&export_svc)` matches `TransferDeps::export`.

Run: `cargo check -j4 -p rocket`

Expected: finished with no errors. A "use of moved value" on `export_svc` or `network_svc` means edit (1) or the `Arc::clone` calls were missed.

- [ ] **Step 6: Run the tests and see them pass**

Run: `cargo test -j4 -p rocket the_transfer_commands_are_in_the_acl_list`

Expected: PASS.

Run: `cargo test -j4 -p rocket ipc_acl`

Expected: all PASS, including `app_commands_file_matches_generate_handler` and `app_commands_file_is_sorted_and_unique`.

Run: `cargo test -j4 -p rocket transfer_events`

Expected: 2 passed (`transfer_events_emit_on_the_network_transfer_channel`, `transfer_events_carry_seq_and_ts_ms_and_keep_snake_case_fields`).

Run: `cargo test -j4 -p rocket a_receiver_failure_message_travels_in_the_finished_event`

Expected: 1 passed.

Run: `cargo test -j4 -p rocket the_send_started_dto_uses_camel_case_keys`

Expected: 1 passed.

Run: `cargo test -j4 -p rocket network`

Expected: the existing network command tests and the new one PASS.

Run: `cargo test -j4 -p rocket the_startup_purge_clears_staging_and_stale_temp_dirs`

Expected: 1 passed. If the stale dir survives, Plan 2's `purge_temp` does not treat an unlocked lock file as stale: fix Plan 2's code (index R5), not this test.

Run:

```bash
grep -n "purge_transfer_temp" src-tauri/src/lib.rs
grep -n "purge_temp();" src-tauri/src/lib.rs
```

Expected: the first prints exactly one line (the startup call); the second prints nothing (no bare call that drops a `DomainResult`).

- [ ] **Step 7: Note on what is not unit-tested here**

The commands take a Tauri `State`, which needs a running app, and they hold no logic beyond `parse_device_id` (already tested by `device_ids_parse_or_are_invalid_input`) and the `SendStarted` to `SendStartedDto` field copy (keys pinned by `the_send_started_dto_uses_camel_case_keys`). The startup purge is unit-tested through `purge_transfer_temp` and the grep above proves `lib.rs` calls it. The wiring (the sink is set) is proven end to end in Tasks 2 and 3 with the same construction order. Record this in the ledger as `Ruling: no command-level unit test; thin commands, parse covered, behaviour covered by the e2e tests.`

---

### Task 2: End to end over the real transport, the happy path and the refusals

**Files:**
- Modify: `crates/rocket-app/src/network/mod.rs`, `crates/rocket-app/src/network/e2e_tests.rs`, `crates/rocket-app/src/network/test_support.rs`
- Create: `crates/rocket-app/src/network/e2e_transfer_tests.rs`

**Interfaces:**
- Consumes: `online_pair`, `pair`, `wait_until`, `row`, `e2e_node`, `E2eNode` (made `pub(super)`), `CollectionExportService::new(Box<dyn CollectionFileSource>)`, `FsTransferStaging`, `TransferService::new_with_timing(deps, Timing { offer_ttl, idle_limit, result_timeout })` and `send -> DomainResult<SendStarted>` (Plan 03), `TransferDeps`, `UNSAFE_ZIP_MESSAGE`, `CollectionImportPort`, `rocket_import::transfer_zip::validate(&rocket_infra::FsImportFiles, path)`, `receiver_display_name` (through the service), `TransferEvent`.
- Produces (test-only unless stated): `RecordingEvents::{transfers, finished_outcomes}` in `test_support`; the harness `Xfer`, `attach`, `normal`, `short_offer`, `RecordingImport` (with `hold_imports`, `entered`, `fail_next`), `MemSource` and fixtures in `e2e_transfer_tests.rs`, reused by Task 3. No production or Plan 03 code changes.

Rules the tests pin:
- A pushed collection is imported once, with the display name the receiver chose, from ZIP bytes that parse and carry the collection's files, and the offered size equals the ZIP length.
- The sender audits `NetworkCollectionSent`, the receiver audits `NetworkCollectionReceived` (short device id, collection name, `outcome: "completed"`). Both sides publish `Progress` and `Finished { completed }`; the receiver also publishes `OfferIncoming` before `Finished`.
- The sender finishes `Completed` and audits `NetworkCollectionSent` only after the receiver's `TransferResult`, which the receiver sends after the import. While the import is still running the sender has no `Finished`.
- A decline imports nothing and the sender sees `Declined`. An unanswered offer ends `Expired` on both sides and a late accept is an error.
- A sender cancel of a pending offer ends the receiver `Cancelled` and a late accept is an error. A receiver cancel of a pending offer is a decline, so the sender sees `Declined` (Plan 03 ruling 11, index R9).
- A name clash gives `<name> (from <sender>)`.
- Sending to an offline peer fails with the offline error and no offer reaches anyone; the online check runs before the export (an unknown collection name gives the same offline error). An empty collection is refused before any offer.
- Spaces and unicode in the name, a one-file collection, and two simultaneous opposite pushes all work.

Decisions recorded here:
- **Time.** Tokio paused time is not usable with real sockets (the clock would jump while IO is pending and break TLS and the 2 s bounded send). The offer TTL is therefore shortened through Plan 03's `Timing`, and the TTL test uses 1 s. All other timings stay at production values (`idle_limit` 120 s, `result_timeout` `SENDER_RESULT_TIMEOUT`), which proves no test depends on an idle wait.
- **Hash.** `export_bruno` stamps each ZIP entry with the current time, so the expected bytes cannot be rebuilt in the test. The happy path compares size and content; the hash check itself is proven by the tampered-chunk test in Task 3.

- [ ] **Step 0: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 0b: Confirm names and the module layout**

Run:

```bash
grep -rn "pub fn peer_label\|pub async fn send_to\|pub fn set_transfer_sink\|fn is_online" crates/rocket-app/src/network
grep -rn "pub fn new_with_timing\|pub struct Timing\|pub struct SendStarted" crates/rocket-app/src/network
grep -rn "spawn_blocking" crates/rocket-app/src/network/transfer_receive.rs
grep -n "derive" -B1 -A1 crates/rocket-network/src/transfer.rs | grep -n "Direction\|Outcome"
sed -n 1,40p crates/rocket-app/src/network/mod.rs
```

Expected: `new_with_timing`, `Timing` (fields `offer_ttl`, `idle_limit`, `result_timeout`) and `SendStarted` are found once each; the four `NetworkService` items exist; the import in `transfer_receive.rs` runs inside `spawn_blocking`. The last one matters: the held import in `the_sender_shows_sent_only_after_the_receivers_result` blocks its thread, and these tests run on the single-threaded test runtime (only the slow-receiver test in Task 3 uses the multi-thread one), so an import called inline on the async worker would freeze every node. If it is inline, fix Plan 03 (the import is sync filesystem work and belongs in `spawn_blocking`), do not change the test. Also `Direction` and `Outcome` derive `Debug, Clone, Copy, PartialEq, Eq` (the tests compare them; if a derive is missing, add it in `rocket-network`, a one-line change, and record a ruling). Note the module that defines `TransferService`, `TransferDeps`, `CollectionImportPort`, `ImportedCollection` and `TransferStaging`: the new test file imports them through `super::{...}`; if they live in a submodule that `network/mod.rs` does not re-export to `super`, import them by their module path instead.

- [ ] **Step 1: Write the harness and the failing tests**

Create `crates/rocket-app/src/network/e2e_transfer_tests.rs`:

```rust
//! End-to-end send tests: two real nodes over real TLS on 127.0.0.1, a real
//! `CollectionExportService` over an in-memory file source and a real `FsTransferStaging`
//! in a temp folder.
//!
//! Only the import is a fake. The shipped adapter lives in `src-tauri` and `rocket-app`
//! must not depend on it. The fake runs the real strict ZIP validator first, so an unsafe
//! ZIP is refused the way the app refuses it.

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use rocket_audit::AuditEventKind;
use rocket_export::{CollectionFileSource, SourceFile};
use rocket_infra::FsTransferStaging;
use rocket_network::limits::{SENDER_RESULT_TIMEOUT, TRANSFER_OFFER_TTL};
use rocket_network::{DeviceId, Direction, Outcome, TransferEvent};
use rocket_shared::error::{DomainError, DomainResult};
use tempfile::TempDir;

use super::e2e_tests::{online_pair, wait_until, E2eNode};
use super::{
    CollectionImportPort, ImportedCollection, Timing, TransferDeps, TransferService,
    TransferStaging, UNSAFE_ZIP_MESSAGE,
};
use crate::collection_export_service::CollectionExportService;

/// The production timings. Nothing in these tests waits for the idle limit.
pub(super) fn normal() -> Timing {
    Timing {
        offer_ttl: TRANSFER_OFFER_TTL,
        idle_limit: Duration::from_secs(120),
        result_timeout: SENDER_RESULT_TIMEOUT,
    }
}

/// Production timings except an offer that expires after 1 s.
pub(super) fn short_offer() -> Timing {
    Timing {
        offer_ttl: Duration::from_secs(1),
        ..normal()
    }
}

// ----- fakes -----

/// Collections held in memory, keyed by name.
pub(super) struct MemSource(HashMap<String, Vec<SourceFile>>);

impl CollectionFileSource for MemSource {
    fn list_files(&self, collection: &str) -> DomainResult<Vec<SourceFile>> {
        self.0
            .get(collection)
            .cloned()
            .ok_or_else(|| DomainError::NotFound(collection.to_string()))
    }
}

/// The import port of a receiver. It records what the service imported.
#[derive(Default)]
pub(super) struct RecordingImport {
    existing: Mutex<Vec<String>>,
    calls: Mutex<Vec<(String, Vec<u8>)>>,
    /// While true, `import_zip` waits after it counted its entry (a slow import).
    hold: AtomicBool,
    entered: AtomicUsize,
    /// The next import fails with this text, as an importer failure (not an unsafe ZIP).
    fail_next: Mutex<Option<String>>,
}

impl RecordingImport {
    fn with_existing(names: &[&str]) -> Arc<Self> {
        let import = RecordingImport::default();
        *import.existing() = names.iter().map(|n| n.to_string()).collect();
        Arc::new(import)
    }

    fn existing(&self) -> MutexGuard<'_, Vec<String>> {
        self.existing.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Makes every import wait (or stop waiting). The import runs on a blocking thread.
    pub(super) fn hold_imports(&self, on: bool) {
        self.hold.store(on, Ordering::SeqCst);
    }

    /// How many imports started.
    pub(super) fn entered(&self) -> usize {
        self.entered.load(Ordering::SeqCst)
    }

    /// Makes the next import fail with `text`.
    pub(super) fn fail_next(&self, text: &str) {
        *self.fail_next.lock().unwrap_or_else(|e| e.into_inner()) = Some(text.to_string());
    }

    /// The `(display name, ZIP bytes)` of every import, in order.
    fn calls(&self) -> Vec<(String, Vec<u8>)> {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl CollectionImportPort for RecordingImport {
    fn existing_names(&self) -> DomainResult<Vec<String>> {
        Ok(self.existing().clone())
    }

    fn import_zip(&self, zip_path: &Path, display_name: &str) -> DomainResult<ImportedCollection> {
        self.entered.fetch_add(1, Ordering::SeqCst);
        while self.hold.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(10));
        }
        // Same order and errors as the shipped adapter: a validator refusal is exactly
        // `InvalidInput(UNSAFE_ZIP_MESSAGE)`, any other failure is another variant.
        rocket_import::transfer_zip::validate(&rocket_infra::FsImportFiles, zip_path)
            .map_err(|_| DomainError::InvalidInput(UNSAFE_ZIP_MESSAGE.to_string()))?;
        if let Some(text) = self
            .fail_next
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            return Err(DomainError::Internal(text));
        }
        let bytes = std::fs::read(zip_path).map_err(|e| DomainError::Internal(e.to_string()))?;
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((display_name.to_string(), bytes));
        Ok(ImportedCollection {
            name: display_name.to_string(),
        })
    }
}

// ----- the harness -----

/// A node with a real transfer service on top of its real network service.
pub(super) struct Xfer {
    pub(super) node: E2eNode,
    pub(super) svc: Arc<TransferService>,
    pub(super) import: Arc<RecordingImport>,
    pub(super) staging_dir: TempDir,
}

impl Xfer {
    pub(super) fn id(&self) -> DeviceId {
        self.node.id()
    }

    pub(super) fn events(&self) -> Vec<TransferEvent> {
        self.node.events.transfers()
    }

    /// The `(direction, outcome)` of every `Finished` event, in order.
    pub(super) fn finished(&self) -> Vec<(Direction, Outcome)> {
        self.node.events.finished_outcomes()
    }

    pub(super) fn has_finished(&self, direction: Direction, outcome: Outcome) -> bool {
        self.finished().contains(&(direction, outcome))
    }

    /// The `message` of every `Finished` event, in order.
    pub(super) fn finished_messages(&self) -> Vec<Option<String>> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                TransferEvent::Finished { message, .. } => Some(message),
                _ => None,
            })
            .collect()
    }

    /// The `(transfer id, collection name, size)` of every incoming offer.
    pub(super) fn offers(&self) -> Vec<(String, String, u64)> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                TransferEvent::OfferIncoming {
                    transfer_id,
                    collection_name,
                    size_bytes,
                    ..
                } => Some((transfer_id, collection_name, size_bytes)),
                _ => None,
            })
            .collect()
    }

    /// The `(done, total)` of every progress event for one direction.
    pub(super) fn progress(&self, which: Direction) -> Vec<(u64, u64)> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                TransferEvent::Progress {
                    direction,
                    done_bytes,
                    total_bytes,
                    ..
                } if direction == which => Some((done_bytes, total_bytes)),
                _ => None,
            })
            .collect()
    }

    /// Files left in this node's staging folder.
    pub(super) fn staged_files(&self) -> usize {
        count_files(self.staging_dir.path())
    }
}

pub(super) fn count_files(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| {
                    let path = entry.path();
                    if path.is_dir() {
                        count_files(&path)
                    } else {
                        1
                    }
                })
                .sum()
        })
        .unwrap_or(0)
}

/// Puts a transfer service on `node`. `sources` are the collections it can send, `existing`
/// are collection names the receiver already has. The wiring mirrors `src-tauri/src/lib.rs`:
/// purge the staging folder first, then build the service.
pub(super) fn attach(
    node: E2eNode,
    sources: Vec<(&str, Vec<SourceFile>)>,
    existing: &[&str],
    timing: Timing,
) -> Xfer {
    attach_in(node, sources, existing, timing, TempDir::new().expect("tempdir"))
}

pub(super) fn attach_in(
    node: E2eNode,
    sources: Vec<(&str, Vec<SourceFile>)>,
    existing: &[&str],
    timing: Timing,
    staging_dir: TempDir,
) -> Xfer {
    let staging = Arc::new(FsTransferStaging::new(staging_dir.path().to_path_buf()));
    staging.purge().expect("purge at start");
    let map: HashMap<String, Vec<SourceFile>> = sources
        .into_iter()
        .map(|(name, files)| (name.to_string(), files))
        .collect();
    let export = Arc::new(CollectionExportService::new(Box::new(MemSource(map))));
    let import = RecordingImport::with_existing(existing);
    let svc = TransferService::new_with_timing(
        TransferDeps {
            network: Arc::clone(&node.svc),
            export,
            importer: import.clone(),
            staging,
            events: node.events.clone(),
            audit: node.audit.clone(),
        },
        timing,
    );
    Xfer {
        node,
        svc,
        import,
        staging_dir,
    }
}

// ----- fixtures -----

pub(super) fn file(path: &str, text: &str) -> SourceFile {
    SourceFile {
        rel_path: path.to_string(),
        bytes: text.as_bytes().to_vec(),
    }
}

/// A small collection with a folder and a request.
pub(super) fn orders(name: &str) -> Vec<SourceFile> {
    vec![
        file(
            "opencollection.yml",
            &format!("opencollection: \"1.0.0\"\ninfo:\n  name: \"{name}\"\n"),
        ),
        file("users/folder.yml", "info:\n  name: users\n"),
        file(
            "users/get.yml",
            "info:\n  name: get\nhttp:\n  method: GET\n  url: https://api.example.test/users\n",
        ),
    ]
}

/// The smallest collection that exports: one file.
pub(super) fn tiny(name: &str) -> Vec<SourceFile> {
    vec![file(
        "opencollection.yml",
        &format!("opencollection: \"1.0.0\"\ninfo:\n  name: \"{name}\"\n"),
    )]
}

/// Bytes a ZIP cannot shrink, so a fixture has a known minimum size.
pub(super) fn noise(len: usize) -> Vec<u8> {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

/// `orders` plus a script file of incompressible bytes, so the ZIP spans several chunks.
pub(super) fn bulky(name: &str, js_len: usize) -> Vec<SourceFile> {
    let mut files = orders(name);
    files.push(SourceFile {
        rel_path: "shared/blob.js".to_string(),
        bytes: noise(js_len),
    });
    files
}

/// Every entry of a ZIP as `(name, bytes)`.
pub(super) fn zip_entries(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("a valid zip");
    (0..archive.len())
        .map(|i| {
            let mut entry = archive.by_index(i).expect("entry");
            let mut data = Vec::new();
            entry.read_to_end(&mut data).expect("entry bytes");
            (entry.name().to_string(), data)
        })
        .collect()
}

/// Waits for the first offer on `receiver` and returns `(transfer id, name, size)`.
pub(super) async fn wait_offer(receiver: &Xfer) -> (String, String, u64) {
    wait_until("the receiver shows the offer", || !receiver.offers().is_empty()).await;
    receiver.offers().remove(0)
}

/// Sends, waits for the offer, accepts it and waits until both sides finished.
async fn push_and_accept(sender: &Xfer, receiver: &Xfer, name: &str) -> String {
    let id = sender
        .svc
        .send(&receiver.id(), name, false)
        .await
        .expect("the offer is sent")
        .transfer_id;
    let (offer_id, _, _) = wait_offer(receiver).await;
    assert_eq!(offer_id, id, "both sides use the sender's transfer id");
    receiver.svc.respond(&id, true).await.expect("accept");
    wait_until("both sides finished", || {
        !sender.finished().is_empty() && !receiver.finished().is_empty()
    })
    .await;
    id
}

// ----- the happy path -----

#[tokio::test]
async fn a_pushed_collection_is_imported_once_and_both_sides_see_it_finish() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], normal());
    let b = attach(b, vec![], &[], normal());

    let id = push_and_accept(&a, &b, "Orders").await;

    assert!(a.has_finished(Direction::Send, Outcome::Completed));
    assert!(b.has_finished(Direction::Receive, Outcome::Completed));

    // The receiver imported once, under the offered name, from a real Bruno ZIP.
    let calls = b.import.calls();
    assert_eq!(calls.len(), 1, "imported exactly once");
    let (display, bytes) = &calls[0];
    assert_eq!(display, "Orders");
    let names: Vec<String> = zip_entries(bytes).into_iter().map(|(n, _)| n).collect();
    assert!(names.contains(&"Orders/opencollection.yml".to_string()));
    assert!(names.contains(&"Orders/users/get.yml".to_string()));
    let (_, offered_name, offered_size) = b.offers().remove(0);
    assert_eq!(offered_name, "Orders");
    assert_eq!(offered_size, bytes.len() as u64, "the offer states the exact size");

    // Progress ended at the full size on both sides.
    assert_eq!(a.progress(Direction::Send).last().map(|p| p.0), Some(offered_size));
    assert_eq!(b.progress(Direction::Receive).last().map(|p| p.0), Some(offered_size));

    // The offer came before the end on the receiver.
    let kinds: Vec<&str> = b
        .events()
        .iter()
        .map(|e| match e {
            TransferEvent::OfferIncoming { .. } => "offer",
            TransferEvent::Progress { .. } => "progress",
            TransferEvent::Finished { .. } => "finished",
        })
        .collect();
    assert_eq!(kinds.first(), Some(&"offer"));
    assert_eq!(kinds.last(), Some(&"finished"));

    // Audit uses the short device id and never the payload.
    assert!(a.node.audited().contains(&AuditEventKind::NetworkCollectionSent {
        device_id: b.id().short(),
        collection: "Orders".to_string(),
        outcome: "completed".to_string(),
    }));
    assert!(b
        .node
        .audited()
        .contains(&AuditEventKind::NetworkCollectionReceived {
            device_id: a.id().short(),
            collection: "Orders".to_string(),
            outcome: "completed".to_string(),
        }));
    let _ = id;

    // Nothing is left in staging.
    wait_until("staging is empty", || b.staged_files() == 0).await;
}

#[tokio::test]
async fn declining_imports_nothing_and_the_sender_sees_declined() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], normal());
    let b = attach(b, vec![], &[], normal());

    let id = a
        .svc
        .send(&b.id(), "Orders", false)
        .await
        .expect("offer sent")
        .transfer_id;
    wait_offer(&b).await;
    b.svc.respond(&id, false).await.expect("decline");

    wait_until("the sender sees Declined", || {
        a.has_finished(Direction::Send, Outcome::Declined)
    })
    .await;
    assert!(b.import.calls().is_empty(), "nothing was imported");
    assert_eq!(b.staged_files(), 0);
    assert!(!a.node.audited().iter().any(|k| matches!(
        k,
        AuditEventKind::NetworkCollectionSent { .. }
    )));
}

#[tokio::test]
async fn an_offer_nobody_answers_expires_on_both_sides_and_a_late_accept_fails() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], short_offer());
    let b = attach(b, vec![], &[], short_offer());

    let id = a
        .svc
        .send(&b.id(), "Orders", false)
        .await
        .expect("offer sent")
        .transfer_id;
    wait_offer(&b).await;

    wait_until("both sides show Expired", || {
        a.has_finished(Direction::Send, Outcome::Expired)
            && b.has_finished(Direction::Receive, Outcome::Expired)
    })
    .await;
    assert!(
        b.svc.respond(&id, true).await.is_err(),
        "a late accept is refused"
    );
    assert!(b.import.calls().is_empty());
    assert_eq!(b.staged_files(), 0);
}

#[tokio::test]
async fn the_sender_cancels_a_pending_offer_and_the_receiver_closes_it() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], normal());
    let b = attach(b, vec![], &[], normal());

    let id = a
        .svc
        .send(&b.id(), "Orders", false)
        .await
        .expect("offer sent")
        .transfer_id;
    wait_offer(&b).await;
    a.svc.cancel(&id).await.expect("the sender cancels");

    // The receiver gets `TransferAbort { cancelled }` for a pending offer and closes it.
    wait_until("the receiver ends Cancelled", || {
        b.has_finished(Direction::Receive, Outcome::Cancelled)
    })
    .await;
    assert!(
        b.svc.respond(&id, true).await.is_err(),
        "an accept after the cancel is refused"
    );
    assert!(b.import.calls().is_empty(), "nothing was imported");
    assert_eq!(b.staged_files(), 0);
    assert!(!a.has_finished(Direction::Send, Outcome::Completed));
}

#[tokio::test]
async fn the_receiver_cancels_a_pending_offer_and_the_sender_sees_declined() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], normal());
    let b = attach(b, vec![], &[], normal());

    let id = a
        .svc
        .send(&b.id(), "Orders", false)
        .await
        .expect("offer sent")
        .transfer_id;
    wait_offer(&b).await;
    // Plan 03 ruling 11: the receiver's cancel of a pending offer is a decline.
    b.svc.cancel(&id).await.expect("the receiver cancels");

    wait_until("the sender sees Declined", || {
        a.has_finished(Direction::Send, Outcome::Declined)
    })
    .await;
    assert!(b.import.calls().is_empty(), "nothing was imported");
    assert_eq!(b.staged_files(), 0);
    assert!(!a.node.audited().iter().any(|k| matches!(
        k,
        AuditEventKind::NetworkCollectionSent { .. }
    )));
}

#[tokio::test]
async fn a_name_clash_gets_the_from_sender_suffix() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], normal());
    let b = attach(b, vec![], &["Orders"], normal());

    push_and_accept(&a, &b, "Orders").await;

    let calls = b.import.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "Orders (from ann)", "ann is the sender's display name");
    assert!(b.has_finished(Direction::Receive, Outcome::Completed));
}

#[tokio::test]
async fn sending_to_an_offline_peer_fails_before_any_offer() {
    let (a, b, _ha, hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], normal());
    let b = attach(b, vec![], &[], normal());

    hb.stop();
    drop(hb);
    wait_until("A shows B as not online", || {
        super::e2e_tests::row(&a.node, &b.id()) != Some(super::NodeStatus::Online)
    })
    .await;

    let err = a
        .svc
        .send(&b.id(), "Orders", false)
        .await
        .expect_err("the peer is offline");
    assert_eq!(err, DomainError::InvalidInput("That node is offline.".into()));
    // The online check runs before the export, so a collection that does not exist gives
    // the same error.
    let err = a
        .svc
        .send(&b.id(), "Nope", false)
        .await
        .expect_err("the peer is offline");
    assert_eq!(err, DomainError::InvalidInput("That node is offline.".into()));
    assert!(a.events().is_empty(), "no transfer was started");
    assert!(b.events().is_empty(), "no offer reached the peer");
    assert!(!a.node.audited().iter().any(|k| matches!(
        k,
        AuditEventKind::NetworkCollectionSent { .. }
    )));
}

#[tokio::test]
async fn a_collection_with_no_files_is_refused_before_any_offer() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Empty", vec![])], &[], normal());
    let b = attach(b, vec![], &[], normal());

    let err = a
        .svc
        .send(&b.id(), "Empty", false)
        .await
        .expect_err("an empty collection is refused");
    assert!(
        matches!(err, DomainError::InvalidInput(_)),
        "unexpected error: {err}"
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(a.events().is_empty(), "no transfer was started");
    assert!(b.events().is_empty(), "no offer reached the peer");
}

#[tokio::test]
async fn the_sender_shows_sent_only_after_the_receivers_result() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], normal());
    let b = attach(b, vec![], &[], normal());
    b.import.hold_imports(true);

    let id = a
        .svc
        .send(&b.id(), "Orders", false)
        .await
        .expect("offer sent")
        .transfer_id;
    wait_offer(&b).await;
    b.svc.respond(&id, true).await.expect("accept");
    wait_until("the receiver started the import", || b.import.entered() == 1).await;

    // Every chunk is out and the receiver is still importing, so it has sent no result. This
    // sleep only gives a wrong early "Sent" time to show up; the asserts hold either way.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(a.finished().is_empty(), "the sender must wait for the result");
    assert!(!a.node.audited().iter().any(|k| matches!(
        k,
        AuditEventKind::NetworkCollectionSent { .. }
    )));

    b.import.hold_imports(false);
    wait_until("both sides completed", || {
        a.has_finished(Direction::Send, Outcome::Completed)
            && b.has_finished(Direction::Receive, Outcome::Completed)
    })
    .await;
    assert!(a.node.audited().contains(&AuditEventKind::NetworkCollectionSent {
        device_id: b.id().short(),
        collection: "Orders".to_string(),
        outcome: "completed".to_string(),
    }));
}

// ----- edge inputs -----

#[tokio::test]
async fn a_name_with_spaces_and_unicode_survives_the_trip() {
    let name = "Caf\u{e9} Orders \u{6771}\u{4eac} v2";
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![(name, orders(name))], &[], normal());
    let b = attach(b, vec![], &[], normal());

    push_and_accept(&a, &b, name).await;

    let (_, offered_name, _) = b.offers().remove(0);
    assert_eq!(offered_name, name);
    let calls = b.import.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, name, "the display name keeps spaces and unicode");
    let root = format!("{name}/opencollection.yml");
    assert!(
        zip_entries(&calls[0].1).iter().any(|(n, _)| *n == root),
        "the ZIP root folder keeps the name"
    );
}

#[tokio::test]
async fn a_one_file_collection_travels_in_a_single_chunk() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Tiny", tiny("Tiny"))], &[], normal());
    let b = attach(b, vec![], &[], normal());

    push_and_accept(&a, &b, "Tiny").await;

    let calls = b.import.calls();
    assert_eq!(calls.len(), 1);
    assert!(
        calls[0].1.len() < rocket_network::limits::CHUNK_BYTES,
        "the whole ZIP fits one chunk"
    );
    assert_eq!(zip_entries(&calls[0].1).len(), 1);
    assert!(a.has_finished(Direction::Send, Outcome::Completed));
}

#[tokio::test]
async fn two_nodes_pushing_to_each_other_at_once_both_complete() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(
        a,
        vec![("FromAnn", orders("FromAnn"))],
        &[],
        normal(),
    );
    let b = attach(
        b,
        vec![("FromBen", orders("FromBen"))],
        &[],
        normal(),
    );

    let (id_ab, id_ba) = tokio::join!(
        a.svc.send(&b.id(), "FromAnn", false),
        b.svc.send(&a.id(), "FromBen", false)
    );
    let id_ab = id_ab.expect("A's offer is sent").transfer_id;
    let id_ba = id_ba.expect("B's offer is sent").transfer_id;
    assert_ne!(id_ab, id_ba, "two transfers, two ids");

    wait_until("each node shows the other's offer", || {
        !a.offers().is_empty() && !b.offers().is_empty()
    })
    .await;
    b.svc.respond(&id_ab, true).await.expect("B accepts A's");
    a.svc.respond(&id_ba, true).await.expect("A accepts B's");

    wait_until("both directions completed on both nodes", || {
        a.has_finished(Direction::Send, Outcome::Completed)
            && a.has_finished(Direction::Receive, Outcome::Completed)
            && b.has_finished(Direction::Send, Outcome::Completed)
            && b.has_finished(Direction::Receive, Outcome::Completed)
    })
    .await;
    assert_eq!(b.import.calls().len(), 1);
    assert_eq!(a.import.calls().len(), 1);
    assert_eq!(b.import.calls()[0].0, "FromAnn");
    assert_eq!(a.import.calls()[0].0, "FromBen");
    wait_until("both staging folders are empty", || {
        a.staged_files() == 0 && b.staged_files() == 0
    })
    .await;
}
```

The unused `Write` import above is for Task 3's `bad_zip`; if the compiler warns before Task 3, add `#[allow(unused_imports)]` on that line for now and remove it in Task 3.

- [ ] **Step 2: Run the tests and see them fail**

Register the module first so the failure is about missing items and not a missing file. In `crates/rocket-app/src/network/mod.rs`, add next to the other test modules:

```rust
#[cfg(test)]
#[path = "e2e_transfer_tests.rs"]
mod e2e_transfer_tests;
```

Run: `cargo test -j4 -p rocket-app e2e_transfer`

Expected: FAIL to compile. The errors name `E2eNode`/`online_pair`/`wait_until` as private and `RecordingEvents::transfers` and `finished_outcomes` as missing. These are the two things Step 3 adds. `new_with_timing`, `Timing` and `SendStarted` already exist from Plan 03; an error naming them means Plan 03 is not applied or its names differ from the index, so stop and fix that first.

- [ ] **Step 3: Make the harness shareable and record transfer events**

(a) In `crates/rocket-app/src/network/e2e_tests.rs`, widen visibility. Change each of these (anchored on the quoted text):

```rust
struct E2eNode {
    name: String,
    svc: Arc<NetworkService>,
```

to

```rust
pub(super) struct E2eNode {
    pub(super) name: String,
    pub(super) svc: Arc<NetworkService>,
```

and give the remaining fields `pub(super)` too: `key`, `identity`, `paired`, `blocked`, `events`, `audit`. Then change `fn id(&self)`, `fn port(&self)`, `fn addr(&self)` and `fn audited(&self)` in `impl E2eNode` to `pub(super) fn`, and change these free functions to `pub(super)`: `e2e_node`, `wait_until`, `row`, `pair`, `online_pair`. Nothing else changes.

Run: `cargo check -j4 -p rocket-app --tests`

Expected: only the errors about `transfers` and `finished_outcomes` remain.

(b) In `crates/rocket-app/src/network/test_support.rs`, add `use rocket_network::{Direction, Outcome, TransferEvent};` (merge it into the existing `rocket_network` import) and change the recorder. Replace

```rust
pub(crate) struct RecordingEvents(Mutex<Vec<NetworkEvent>>);
```

with

```rust
pub(crate) struct RecordingEvents(Mutex<Vec<NetworkEvent>>, Mutex<Vec<TransferEvent>>);
```

add to `impl RecordingEvents`:

```rust
    /// Every transfer event, in order.
    pub fn transfers(&self) -> Vec<TransferEvent> {
        self.1.lock().expect("events lock").clone()
    }

    /// The `(direction, outcome)` of every `Finished` transfer event, in order.
    pub fn finished_outcomes(&self) -> Vec<(Direction, Outcome)> {
        self.transfers()
            .into_iter()
            .filter_map(|e| match e {
                TransferEvent::Finished {
                    direction, outcome, ..
                } => Some((direction, outcome)),
                _ => None,
            })
            .collect()
    }
```

and replace the `publish` body of `impl EventPublisher for RecordingEvents` with:

```rust
    fn publish(&self, event: &dyn PublishedEvent) {
        if let Some(event) = event.downcast_ref::<NetworkEvent>() {
            self.0.lock().expect("events lock").push(event.clone());
        }
        if let Some(event) = event.downcast_ref::<TransferEvent>() {
            self.1.lock().expect("events lock").push(event.clone());
        }
    }
```

(c) No change to `TransferService` here. The timing seam is Plan 03's `new_with_timing` (the service stores `offer_ttl` and builds every machine with it, so the TTL test below needs nothing else).

Run: `cargo check -j4 -p rocket-app --tests`

Expected: finished with no errors. A missing `PartialEq` or `Copy` on `Direction` or `Outcome` shows here; add the derive in `rocket-network` (Step 0b).

- [ ] **Step 4: Run the tests and see them pass**

Run: `cargo test -j4 -p rocket-app e2e_transfer`

Expected: 12 passed (`a_pushed_collection_is_imported_once_and_both_sides_see_it_finish`, `declining_imports_nothing_and_the_sender_sees_declined`, `an_offer_nobody_answers_expires_on_both_sides_and_a_late_accept_fails`, `the_sender_cancels_a_pending_offer_and_the_receiver_closes_it`, `the_receiver_cancels_a_pending_offer_and_the_sender_sees_declined`, `a_name_clash_gets_the_from_sender_suffix`, `sending_to_an_offline_peer_fails_before_any_offer`, `a_collection_with_no_files_is_refused_before_any_offer`, `the_sender_shows_sent_only_after_the_receivers_result`, `a_name_with_spaces_and_unicode_survives_the_trip`, `a_one_file_collection_travels_in_a_single_chunk`, `two_nodes_pushing_to_each_other_at_once_both_complete`). If a test fails, the bug is in Plan 3 code: fix it there with superpowers:systematic-debugging, record a `Ruling:` line and never weaken the test. Two assertions encode Plan 3 choices to confirm against its code: the audit `device_id` is the short id (spec section 8 forbids the full id), and the receiver's audit `collection` is the imported display name while the sender's is the offered name (equal in these tests).

Run: `cargo test -j4 -p rocket-app e2e`

Expected: the 8 older e2e tests plus the 12 new ones pass (20).

- [ ] **Step 5: Check the crate**

Run: `cargo check -j4 -p rocket-app --tests`

Expected: finished with no errors or warnings from the new code.

---

### Task 3: End to end failure and safety cases

**Files:**
- Modify: `crates/rocket-app/src/network/e2e_transfer_tests.rs`, `src-tauri/src/import_wiring.rs` (a test only). No production file and no Plan 03 file changes.

**Interfaces:**
- Consumes: the Task 2 harness (`Xfer`, `attach`, `attach_in`, `normal`, `bulky`, `noise`, `zip_entries`, `count_files`, `wait_offer`, `RecordingImport::fail_next`), `NetworkService::remove_paired`, `FsTransferStaging`, `TransferStaging`, `StagedZip`, `StagedFile`, `limits::{MAX_UNACKED_CHUNKS, CHUNK_BYTES}`, and Plan 03's test hooks (below).
- Produces (test-only): `join_peer`, `SlowStaging` (a staging wrapper whose `append` sleeps, the slow receiver disk) and `attach_slow`, the failure tests, and the adapter trust test in `src-tauri`. No hook is added here.

The hooks come from Plan 03 (index R8: all `#[cfg(test)] pub(crate)`, reachable here because this file is a unit-test module of `rocket-app`; this plan only calls them):

- `TransferService::offer_raw(&self, device: &DeviceId, name: &str, bytes: Vec<u8>) -> DomainResult<String>`: claims the peer's send slot (`claim_send`), runs the real `offer_bytes` with the given bytes, releases the slot on error and returns the transfer id. It skips the export step only.
- `TransferService::set_chunk_delay(&self, delay: Duration)`: the sender sleeps `delay` before each chunk's locked block (default zero).
- `TransferService::set_tamper(&self, seq: Option<u32>)`: when `Some(n)`, the sender flips the first byte of a copy of chunk `n` before it is framed (default `None`).

This plan adds no second set and no `test-support` feature. If Step 0b finds a hook without `#[cfg(test)]`, Plan 03 is not done: fix it there.

Rules the tests pin:
- A dropped connection mid-transfer ends the receiver with `Finished { failed }` and an empty staging folder, whichever side was stopped. A sender whose peer vanishes sees `Failed`. Both end through `on_peer_gone`, not the idle limit (120 s in these tests), so each test finishes inside 20 s.
- A flipped byte in one chunk ends the receiver with `Damaged`, imports nothing and leaves nothing staged, and the sender ends `Damaged` (it learns it from the receiver's `TransferResult`) and audits no `NetworkCollectionSent`. The receiver audits `NetworkCollectionReceived { outcome: "damaged" }` (index R6).
- A ZIP with a `../` entry ends with `Rejected` on both sides and no import. The receiver audits `NetworkCollectionReceived { outcome: "rejected" }`; the sender audits nothing.
- A receiver gone after it acknowledged the last chunk (the final `ChunkAck`) leaves the sender `Unconfirmed`, never `Failed`, and the sender audits `NetworkCollectionSent { outcome: "unconfirmed" }` (index R4 as amended). A receiver gone before the final ack is `Failed` (Plan 03 tests).
- A slow receiver: the sender is never more than `MAX_UNACKED_CHUNKS` chunks (plus one chunk of sampling slack) ahead of what the receiver accepted, and the transfer still completes (index R2).
- A send with "Include environments" off carries no `environments/` entry although the source has one (index R10).
- An importer failure (not an unsafe ZIP) ends the receiver `Failed` with the importer's text in `Finished.message`, and the sender `Failed`.
- A collection over 10 MiB is refused before any offer, with the "too big" text.
- Three pending offers from three peers are accepted; a fourth peer is told busy.
- **The secret check:** the received ZIP, every entry unzipped, holds neither a literal token header value nor a secret variable value, and non-secret values survive (a positive control, so the scan is known to read real content).
- Unpairing during a transfer, and a restart after a crash, leave no staged files.

Decisions recorded here:
- **Tampering uses a sender hook, not a relay.** A real man-in-the-middle on the established `/peer` socket would need a third node paired with both sides and a forwarder for all frames. It would also test a case the transport already forbids: TLS with pinned paired keys. The hook flips the byte after the hash was computed, so the receiver sees the same bytes on the wire as it would from a hostile hop. Record as `Ruling:`.
- **Over-limit uses the real export.** A script file of incompressible bytes makes a real ZIP over 10 MiB, so the check is exercised through `send`, not a hook. The exact boundary (10 MiB passes, +1 byte fails) is a pure check pinned by Plans 01 and 03 and is not repeated here, because a 10 MiB raw payload would not be a valid ZIP.

- [ ] **Step 0: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 0b: Confirm the Plan 03 hooks and drop handling**

Run:

```bash
grep -rn "fn offer_raw\|fn set_chunk_delay\|fn set_tamper" crates/rocket-app/src/network
grep -rn -B3 "fn offer_raw\|fn set_chunk_delay\|fn set_tamper" crates/rocket-app/src/network | grep -c "cfg(test)"
grep -rn "Outcome::Unconfirmed\|ChunkAck" crates/rocket-app/src/network/*.rs
grep -rn "fn on_peer_gone" crates/rocket-app/src/network crates/rocket-network/src
grep -rn "on_peer_gone" crates/rocket-app/src/network/connections.rs crates/rocket-app/src/network/service.rs
grep -n "Outcome::Failed\|Outcome::Damaged\|Outcome::Rejected\|Outcome::Busy" crates/rocket-app/src/network/*.rs
```

Expected: `offer_raw`, `set_chunk_delay` and `set_tamper` exist once each, the count line prints `3` (each is `#[cfg(test)]`), and `Outcome::Unconfirmed` and `ChunkAck` are used by the service; `on_peer_gone` is declared on `TransferSink`, implemented by `TransferService`, and called from `run_socket`'s end and from `remove_local`; the outcomes are mapped as in the spec. If a hook or a call is missing, Plan 03 is not complete: fix Plan 03, never weaken a test here.

- [ ] **Step 1: Write the failing tests**

Add these imports to the top of `e2e_transfer_tests.rs` (merge with the existing ones; remove the `#[allow(unused_imports)]` note from Task 2 if you added one):

```rust
use rocket_network::limits::{CHUNK_BYTES, MAX_PENDING_INCOMING, MAX_TRANSFER_BYTES, MAX_UNACKED_CHUNKS};

use super::e2e_tests::{e2e_node, pair, row};
use super::{NetworkHandle, NodeStatus, StagedFile, StagedZip};
```

The tests below call `attach_slow` and `SlowStaging`, which Step 3 adds after the failing run.

Then append:

```rust
// ----- more fixtures -----

/// A ZIP with one entry that escapes its folder.
fn bad_zip() -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("../evil.yml", zip::write::SimpleFileOptions::default())
        .expect("start entry");
    writer.write_all(b"info:\n  name: evil\n").expect("write entry");
    let bytes = writer.finish().expect("finish").into_inner();
    let archive = zip::ZipArchive::new(Cursor::new(bytes.clone())).expect("reads back");
    assert_eq!(
        archive.file_names().next(),
        Some("../evil.yml"),
        "the writer kept the unsafe name"
    );
    bytes
}

/// A fresh started node, linked to and paired with `hub`, with `hub` as the initiator.
async fn join_peer(hub: &E2eNode, name: &str) -> (E2eNode, NetworkHandle) {
    let node = e2e_node(name);
    let handle = node.svc.start().await;
    node.svc
        .add_address("127.0.0.1", hub.port())
        .await
        .expect("peer saves the hub address");
    hub.svc
        .add_address("127.0.0.1", node.port())
        .await
        .expect("hub saves the peer address");
    wait_until("each lists the other", || {
        row(&node, &hub.id()).is_some() && row(hub, &node.id()).is_some()
    })
    .await;
    pair(hub, &node).await;
    wait_until("both rows show Online", || {
        row(&node, &hub.id()) == Some(NodeStatus::Online)
            && row(hub, &node.id()) == Some(NodeStatus::Online)
    })
    .await;
    (node, handle)
}

const SLOW_CHUNK: Duration = Duration::from_millis(40);

/// True once the receiver got some bytes but not all of them.
fn mid_transfer(receiver: &Xfer) -> bool {
    receiver
        .progress(Direction::Receive)
        .iter()
        .any(|(done, total)| *done > 0 && done < total)
}

// ----- dropped connection -----

#[tokio::test]
async fn a_dropped_sender_fails_the_receiver_and_clears_staging() {
    let (a, b, ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Big", bulky("Big", 600 * 1024))], &[], normal());
    let b = attach(b, vec![], &[], normal());
    a.svc.set_chunk_delay(SLOW_CHUNK);

    let id = a.svc.send(&b.id(), "Big", false).await.expect("offer sent").transfer_id;
    wait_offer(&b).await;
    b.svc.respond(&id, true).await.expect("accept");
    wait_until("the receiver got some chunks", || mid_transfer(&b)).await;

    ha.stop();
    drop(ha);

    wait_until("the receiver ends Failed", || {
        b.has_finished(Direction::Receive, Outcome::Failed)
    })
    .await;
    assert!(b.import.calls().is_empty(), "a partial transfer is never imported");
    wait_until("the staged file is deleted", || b.staged_files() == 0).await;
}

#[tokio::test]
async fn a_dropped_receiver_fails_the_sender_and_a_restart_leaves_no_staged_files() {
    let (a, b, _ha, hb) = online_pair().await;
    let a = attach(a, vec![("Big", bulky("Big", 600 * 1024))], &[], normal());
    let b = attach(b, vec![], &[], normal());
    a.svc.set_chunk_delay(SLOW_CHUNK);

    let id = a.svc.send(&b.id(), "Big", false).await.expect("offer sent").transfer_id;
    wait_offer(&b).await;
    b.svc.respond(&id, true).await.expect("accept");
    wait_until("the receiver got some chunks", || mid_transfer(&b)).await;

    hb.stop();
    drop(hb);

    wait_until("the sender ends Failed", || {
        a.has_finished(Direction::Send, Outcome::Failed)
    })
    .await;
    assert!(b.import.calls().is_empty());

    // The receiver app restarts: a new staging object on the same folder runs the startup purge.
    let restarted = FsTransferStaging::new(b.staging_dir.path().to_path_buf());
    restarted.purge().expect("purge after restart");
    assert_eq!(count_files(b.staging_dir.path()), 0, "no staged file survives a restart");
}

#[tokio::test]
async fn a_restart_purges_files_a_crash_left_behind() {
    let dir = TempDir::new().expect("tempdir");
    let staging = FsTransferStaging::new(dir.path().to_path_buf());
    let mut staged = staging
        .create("0123456789abcdef0123456789abcdef")
        .expect("create the staged file");
    staged.append(b"half a zip").expect("append");
    // A crash never runs finish or discard.
    std::mem::forget(staged);
    assert!(count_files(dir.path()) >= 1, "the crash left a staged file");

    // The same order as `lib.rs`: purge first, then build the service.
    let node = attach_in(
        e2e_node("ben"),
        vec![],
        &[],
        normal(),
        dir,
    );
    assert_eq!(node.staged_files(), 0, "the startup purge removed it");
}

// ----- unpair during a transfer -----

#[tokio::test]
async fn unpairing_during_a_transfer_ends_both_sides_and_imports_nothing() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Big", bulky("Big", 600 * 1024))], &[], normal());
    let b = attach(b, vec![], &[], normal());
    a.svc.set_chunk_delay(SLOW_CHUNK);

    let id = a.svc.send(&b.id(), "Big", false).await.expect("offer sent").transfer_id;
    wait_offer(&b).await;
    b.svc.respond(&id, true).await.expect("accept");
    wait_until("the receiver got some chunks", || mid_transfer(&b)).await;

    b.node.svc.remove_paired(&a.id()).await.expect("B unpairs A");

    let ended = |x: &Xfer, d: Direction| {
        x.has_finished(d, Outcome::Failed) || x.has_finished(d, Outcome::Cancelled)
    };
    wait_until("both sides ended without completing", || {
        ended(&a, Direction::Send) && ended(&b, Direction::Receive)
    })
    .await;
    assert!(!a.has_finished(Direction::Send, Outcome::Completed));
    assert!(!b.has_finished(Direction::Receive, Outcome::Completed));
    assert!(b.import.calls().is_empty());
    wait_until("staging is empty and both unpaired", || {
        b.staged_files() == 0
            && !a.node.paired.contains(&b.id())
            && !b.node.paired.contains(&a.id())
    })
    .await;
}

// ----- damaged and unsafe payloads -----

#[tokio::test]
async fn a_tampered_chunk_is_damaged_and_nothing_is_imported() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Big", bulky("Big", 100 * 1024))], &[], normal());
    let b = attach(b, vec![], &[], normal());
    a.svc.set_tamper(Some(1));

    let id = a.svc.send(&b.id(), "Big", false).await.expect("offer sent").transfer_id;
    let (_, _, size) = wait_offer(&b).await;
    assert!(size > 2 * rocket_network::limits::CHUNK_BYTES as u64, "several chunks");
    b.svc.respond(&id, true).await.expect("accept");

    wait_until("the receiver ends Damaged", || {
        b.has_finished(Direction::Receive, Outcome::Damaged)
    })
    .await;
    wait_until("the sender ends Damaged", || {
        a.has_finished(Direction::Send, Outcome::Damaged)
    })
    .await;
    assert!(!a.node.audited().iter().any(|k| matches!(
        k,
        AuditEventKind::NetworkCollectionSent { .. }
    )));
    assert!(b.import.calls().is_empty(), "a damaged payload is never imported");
    // A tampered payload is evidence, so the receiver audits it (index R6).
    assert!(b.node.audited().iter().any(|k| matches!(
        k,
        AuditEventKind::NetworkCollectionReceived { outcome, .. } if outcome == "damaged"
    )));
    wait_until("staging is empty", || b.staged_files() == 0).await;
}

#[tokio::test]
async fn an_unsafe_zip_is_rejected_and_nothing_is_imported() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![], &[], normal());
    let b = attach(b, vec![], &[], normal());

    let id = a
        .svc
        .offer_raw(&b.id(), "Evil", bad_zip())
        .await
        .expect("a well-formed offer goes out");
    wait_offer(&b).await;
    b.svc.respond(&id, true).await.expect("accept");

    wait_until("the receiver ends Rejected", || {
        b.has_finished(Direction::Receive, Outcome::Rejected)
    })
    .await;
    assert!(b.import.calls().is_empty(), "an unsafe ZIP is never imported");
    wait_until("the sender ends Rejected", || {
        a.has_finished(Direction::Send, Outcome::Rejected)
    })
    .await;
    assert!(!a.has_finished(Direction::Send, Outcome::Completed));
    assert!(b.node.audited().iter().any(|k| matches!(
        k,
        AuditEventKind::NetworkCollectionReceived { outcome, .. } if outcome == "rejected"
    )));
    assert!(!a.node.audited().iter().any(|k| matches!(
        k,
        AuditEventKind::NetworkCollectionSent { .. }
    )));
    wait_until("staging is empty", || b.staged_files() == 0).await;
}

const IMPORT_FAILURE: &str = "import-failure-marker-3b8e61d4";

#[tokio::test]
async fn a_failing_import_fails_both_sides_and_carries_the_importer_message() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], normal());
    let b = attach(b, vec![], &[], normal());
    b.import.fail_next(IMPORT_FAILURE);

    let id = a
        .svc
        .send(&b.id(), "Orders", false)
        .await
        .expect("offer sent")
        .transfer_id;
    wait_offer(&b).await;
    b.svc.respond(&id, true).await.expect("accept");

    wait_until("the receiver ends Failed", || {
        b.has_finished(Direction::Receive, Outcome::Failed)
    })
    .await;
    wait_until("the sender ends Failed", || {
        a.has_finished(Direction::Send, Outcome::Failed)
    })
    .await;
    assert!(
        b.finished_messages()
            .iter()
            .flatten()
            .any(|m| m.contains(IMPORT_FAILURE)),
        "the importer's text reaches the Finished event"
    );
    assert!(!b.node.audited().iter().any(|k| matches!(
        k,
        AuditEventKind::NetworkCollectionReceived { .. }
    )));
    assert!(!a.node.audited().iter().any(|k| matches!(
        k,
        AuditEventKind::NetworkCollectionSent { .. }
    )));
    wait_until("staging is empty", || b.staged_files() == 0).await;
}

// ----- limits -----

#[tokio::test]
async fn a_collection_over_the_limit_is_refused_before_any_offer() {
    let (a, b, _ha, _hb) = online_pair().await;
    let over = MAX_TRANSFER_BYTES as usize + 1024 * 1024;
    let a = attach(a, vec![("Huge", bulky("Huge", over))], &[], normal());
    let b = attach(b, vec![], &[], normal());

    let err = a
        .svc
        .send(&b.id(), "Huge", false)
        .await
        .expect_err("over the limit");
    assert!(
        err.to_string().contains("too big to send"),
        "unexpected error: {err}"
    );

    // Nothing may reach the peer, so give a stray frame time to show up.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(b.events().is_empty(), "no offer was made");
    assert!(a.events().is_empty(), "no transfer was started");
    assert!(!a.node.audited().iter().any(|k| matches!(
        k,
        AuditEventKind::NetworkCollectionSent { .. }
    )));
}

#[tokio::test]
async fn the_fourth_pending_offer_is_told_busy() {
    let (hub, first, _hh, _h1) = online_pair().await;
    let hub = attach(hub, vec![], &[], normal());
    let mut senders = vec![attach(
        first,
        vec![("C0", orders("C0"))],
        &[],
        normal(),
    )];
    let mut handles = Vec::new();
    for i in 1..=MAX_PENDING_INCOMING {
        let (node, handle) = join_peer(&hub.node, &format!("peer{i}")).await;
        handles.push(handle);
        let name = format!("C{i}");
        senders.push(attach(
            node,
            vec![(name.as_str(), orders(&name))],
            &[],
            normal(),
        ));
    }
    assert_eq!(senders.len(), MAX_PENDING_INCOMING + 1);

    // One offer from each of the first three peers, one at a time, so the order is known.
    for (i, sender) in senders.iter().take(MAX_PENDING_INCOMING).enumerate() {
        sender
            .svc
            .send(&hub.id(), &format!("C{i}"), false)
            .await
            .expect("offer sent");
        wait_until("the hub shows the offer", || hub.offers().len() == i + 1).await;
    }

    // The fourth peer is over the limit of pending offers.
    let fourth = &senders[MAX_PENDING_INCOMING];
    fourth
        .svc
        .send(&hub.id(), &format!("C{MAX_PENDING_INCOMING}"), false)
        .await
        .expect("the offer frame is sent");
    wait_until("the fourth sender is told busy", || {
        fourth.has_finished(Direction::Send, Outcome::Busy)
    })
    .await;
    assert_eq!(hub.offers().len(), MAX_PENDING_INCOMING, "still three pending");
    for sender in senders.iter().take(MAX_PENDING_INCOMING) {
        assert!(sender.finished().is_empty(), "the first three are still waiting");
    }
    drop(handles);
}

// ----- the secret check -----

const TOKEN: &str = "TOKEN-LITERAL-4f9c2e81";
const SECRET_VAR: &str = "SECRET-VALUE-b71d08c3";
const SECRET_ENV: &str = "ENV-SECRET-93ab55e0";
// Distinct long markers for values that must survive, so the positive control cannot match
// by accident.
const PLAIN_HEADER: &str = "header-plain-marker-5c1f02be";
const PLAIN_REGION: &str = "region-plain-marker-7d3e91aa";
const PLAIN_HOST: &str = "host-plain-marker-a86d4417";

fn secret_collection() -> Vec<SourceFile> {
    vec![
        file(
            "opencollection.yml",
            "opencollection: \"1.0.0\"\ninfo:\n  name: Secrets\n",
        ),
        file(
            "users/get.yml",
            &format!(
                "info:\n  name: get\nhttp:\n  method: GET\n  url: https://api.example.test/orders\n  headers:\n    - name: Authorization\n      value: Bearer {TOKEN}\n    - name: X-Plain\n      value: {PLAIN_HEADER}\nruntime:\n  variables:\n    - name: region\n      value: {PLAIN_REGION}\n    - name: session_id\n      value: {SECRET_VAR}\n      secret: true\n"
            ),
        ),
        file(
            "environments/local.yml",
            &format!(
                "name: local\nvariables:\n  - name: db\n    value: {SECRET_ENV}\n    secret: true\n  - name: host\n    value: {PLAIN_HOST}\n"
            ),
        ),
    ]
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

#[tokio::test]
async fn a_pushed_collection_carries_neither_the_token_nor_the_secret_variable() {
    // The fixture really holds the values, so the test would catch a leak.
    for needle in [TOKEN, SECRET_VAR, SECRET_ENV, PLAIN_HEADER, PLAIN_REGION, PLAIN_HOST] {
        assert!(secret_collection().iter().any(|f| contains(&f.bytes, needle)));
    }

    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Secrets", secret_collection())], &[], normal());
    let b = attach(b, vec![], &[], normal());

    // Environments are included on purpose: their secret values must be blanked too.
    let started = a
        .svc
        .send(&b.id(), "Secrets", true)
        .await
        .expect("offer sent");
    let id = started.transfer_id.clone();
    // The notes name what was blanked or skipped, never the values themselves. If Plan 03
    // lists no note for this fixture, check what `ExportReport` holds for it; do not drop
    // the non-empty assert.
    assert!(!started.notes.is_empty(), "the blanked secrets are reported");
    for note in &started.notes {
        for needle in [TOKEN, SECRET_VAR, SECRET_ENV] {
            assert!(!note.contains(needle), "a note holds a secret value: {note}");
        }
    }
    wait_offer(&b).await;
    b.svc.respond(&id, true).await.expect("accept");
    wait_until("both sides finished", || {
        a.has_finished(Direction::Send, Outcome::Completed)
            && b.has_finished(Direction::Receive, Outcome::Completed)
    })
    .await;

    let calls = b.import.calls();
    assert_eq!(calls.len(), 1);
    let zip_bytes = &calls[0].1;
    assert!(!contains(zip_bytes, TOKEN) && !contains(zip_bytes, SECRET_VAR));

    // Unzip and scan every entry, not only the raw bytes: deflate could hide a value.
    let entries = zip_entries(zip_bytes);
    assert!(
        entries.iter().any(|(n, _)| n.ends_with("environments/local.yml")),
        "the environment file was sent, so its secret was scrubbed and not left out"
    );
    for (name, data) in &entries {
        for needle in [TOKEN, SECRET_VAR, SECRET_ENV] {
            assert!(!contains(data, needle), "{name} holds a secret value");
        }
    }
    // Positive control: values that are not secret survive, so the scan reads real content.
    let all: Vec<u8> = entries.iter().flat_map(|(_, d)| d.clone()).collect();
    for marker in [PLAIN_HEADER, PLAIN_REGION, PLAIN_HOST] {
        assert!(contains(&all, marker), "{marker} did not survive the trip");
    }
}

#[tokio::test]
async fn a_send_without_environments_carries_no_environment_file() {
    // The source really has an environment file, so the test would catch a service that
    // ignores the flag.
    assert!(secret_collection()
        .iter()
        .any(|f| f.rel_path.starts_with("environments/")));
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Secrets", secret_collection())], &[], normal());
    let b = attach(b, vec![], &[], normal());

    // `push_and_accept` sends with `include_environments = false`, the dialog's default.
    push_and_accept(&a, &b, "Secrets").await;

    assert!(b.has_finished(Direction::Receive, Outcome::Completed));
    let calls = b.import.calls();
    assert_eq!(calls.len(), 1);
    let entries = zip_entries(&calls[0].1);
    let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    assert!(
        names.iter().any(|n| n.ends_with("users/get.yml")),
        "the requests were sent: {names:?}"
    );
    assert!(
        !names.iter().any(|n| n.contains("environments/")),
        "no environment entry may be sent: {names:?}"
    );
    for (name, data) in &entries {
        assert!(!contains(data, PLAIN_HOST), "{name} holds an environment value");
    }
}

// ----- after the final ack -----

#[tokio::test]
async fn a_receiver_gone_after_the_final_ack_leaves_the_sender_unconfirmed() {
    let (a, b, _ha, hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], normal());
    let b = attach(b, vec![], &[], normal());
    b.import.hold_imports(true);

    let id = a
        .svc
        .send(&b.id(), "Orders", false)
        .await
        .expect("offer sent")
        .transfer_id;
    wait_offer(&b).await;
    b.svc.respond(&id, true).await.expect("accept");
    // The import started, so every chunk arrived and the sender waits for the result.
    wait_until("the receiver started the import", || b.import.entered() == 1).await;
    // The receiver sent the final `ChunkAck` before it started the import; give it a moment
    // to reach the sender. Only after that ack does a lost peer give `Unconfirmed`.
    tokio::time::sleep(Duration::from_millis(300)).await;

    hb.stop();
    drop(hb);

    wait_until("the sender ends Unconfirmed", || {
        a.has_finished(Direction::Send, Outcome::Unconfirmed)
    })
    .await;
    assert!(
        !a.has_finished(Direction::Send, Outcome::Failed),
        "the bytes left, so the sender does not report a failure"
    );
    assert!(a.node.audited().contains(&AuditEventKind::NetworkCollectionSent {
        device_id: b.id().short(),
        collection: "Orders".to_string(),
        outcome: "unconfirmed".to_string(),
    }));
    // Let the held import thread end.
    b.import.hold_imports(false);
}

// ----- flow control -----

/// The simulated slow disk of the receiver: every append waits this long first.
const SLOW_APPEND: Duration = Duration::from_millis(20);

// A multi-thread runtime, so the receiver's slow disk blocks only one worker thread and the
// sender is free to run ahead if nothing holds it back. On the single-thread runtime the
// sleep would also stop the sender, and the test could pass without the window.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_slow_receiver_keeps_the_sender_inside_the_ack_window() {
    let (a, b, _ha, _hb) = online_pair().await;
    // About 3 MiB of incompressible bytes, so about 96 chunks: three windows.
    let a = attach(a, vec![("Slow", bulky("Slow", 3 * 1024 * 1024))], &[], normal());
    let b = attach_slow(b, SLOW_APPEND);

    let id = a.svc.send(&b.id(), "Slow", false).await.expect("offer sent").transfer_id;
    let (_, _, size) = wait_offer(&b).await;
    assert!(
        size / CHUNK_BYTES as u64 > 2 * u64::from(MAX_UNACKED_CHUNKS),
        "the payload spans several windows"
    );
    b.svc.respond(&id, true).await.expect("accept");

    // Plan 03 emits one `Progress` per chunk on both sides, so the last one is the count so
    // far. The sender is read first: the receiver's count only grows, so the gap measured is
    // never smaller than the real one. One chunk of slack covers a receiver that emits its
    // progress just after it sent the ack.
    let limit = (u64::from(MAX_UNACKED_CHUNKS) + 1) * CHUNK_BYTES as u64;
    let last_done = |x: &Xfer, d: Direction| x.progress(d).last().map(|p| p.0).unwrap_or(0);
    let mut widest = 0u64;
    wait_until("both sides completed", || {
        let sent = last_done(&a, Direction::Send);
        let received = last_done(&b, Direction::Receive);
        widest = widest.max(sent.saturating_sub(received));
        a.has_finished(Direction::Send, Outcome::Completed)
            && b.has_finished(Direction::Receive, Outcome::Completed)
    })
    .await;
    assert!(
        widest <= limit,
        "the sender ran {widest} bytes ahead, the window allows {limit}"
    );
    assert!(widest > 0, "the sampler saw the sender ahead at least once");
    assert_eq!(b.import.calls().len(), 1, "the slow transfer still imports");
}
```

- [ ] **Step 2: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket-app e2e_transfer`

Expected: FAIL to compile with "cannot find function `attach_slow` in this scope" (the slow receiver helper Step 3 adds). No other error: an error naming `set_chunk_delay`, `set_tamper`, `offer_raw`, `Outcome::Unconfirmed` or the `outcome` field of an audit kind means Plan 01 or 03 is not applied as the index says, so stop and fix that first.

- [ ] **Step 3: Add the slow receiver helper**

Append to `e2e_transfer_tests.rs`, after `join_peer`:

```rust
/// A slow receiver disk: every append sleeps first, then the real staging writes.
pub(super) struct SlowStaging {
    inner: FsTransferStaging,
    delay: Duration,
}

struct SlowZip {
    inner: Box<dyn StagedZip>,
    delay: Duration,
}

impl StagedZip for SlowZip {
    fn append(&mut self, bytes: &[u8]) -> DomainResult<()> {
        std::thread::sleep(self.delay);
        self.inner.append(bytes)
    }

    fn finish(self: Box<Self>) -> DomainResult<StagedFile> {
        self.inner.finish()
    }

    fn discard(self: Box<Self>) {
        self.inner.discard()
    }
}

impl TransferStaging for SlowStaging {
    fn create(&self, transfer_id: &str) -> DomainResult<Box<dyn StagedZip>> {
        Ok(Box::new(SlowZip {
            inner: self.inner.create(transfer_id)?,
            delay: self.delay,
        }))
    }

    fn purge(&self) -> DomainResult<()> {
        self.inner.purge()
    }

    fn release(&self, file: &StagedFile) -> DomainResult<()> {
        self.inner.release(file)
    }
}

/// A receiving node whose staging is slow. It has nothing to send.
fn attach_slow(node: E2eNode, delay: Duration) -> Xfer {
    let staging_dir = TempDir::new().expect("tempdir");
    let staging = Arc::new(SlowStaging {
        inner: FsTransferStaging::new(staging_dir.path().to_path_buf()),
        delay,
    });
    staging.purge().expect("purge at start");
    let export = Arc::new(CollectionExportService::new(Box::new(MemSource(HashMap::new()))));
    let import = RecordingImport::with_existing(&[]);
    let svc = TransferService::new_with_timing(
        TransferDeps {
            network: Arc::clone(&node.svc),
            export,
            importer: import.clone(),
            staging,
            events: node.events.clone(),
            audit: node.audit.clone(),
        },
        normal(),
    );
    Xfer {
        node,
        svc,
        import,
        staging_dir,
    }
}
```

Run: `cargo check -j4 -p rocket-app --tests`

Expected: finished with no errors. If `StagedFile` or `StagedZip` is not visible through `super`, import them from the module Step 0b of Task 2 noted (`transfer_ports`). If the e2e harness cannot run on the multi-thread runtime (a `!Send` value held across an `.await` in a helper), the compiler names it here; fix the helper, keep the flavor.

- [ ] **Step 4: Run the tests and see them pass**

Run: `cargo test -j4 -p rocket-app e2e_transfer`

Expected: the 12 Task 2 tests and these 13 pass: `a_dropped_sender_fails_the_receiver_and_clears_staging`, `a_dropped_receiver_fails_the_sender_and_a_restart_leaves_no_staged_files`, `a_restart_purges_files_a_crash_left_behind`, `unpairing_during_a_transfer_ends_both_sides_and_imports_nothing`, `a_tampered_chunk_is_damaged_and_nothing_is_imported`, `an_unsafe_zip_is_rejected_and_nothing_is_imported`, `a_failing_import_fails_both_sides_and_carries_the_importer_message`, `a_collection_over_the_limit_is_refused_before_any_offer`, `the_fourth_pending_offer_is_told_busy`, `a_pushed_collection_carries_neither_the_token_nor_the_secret_variable`, `a_send_without_environments_carries_no_environment_file`, `a_receiver_gone_after_the_final_ack_leaves_the_sender_unconfirmed`, `a_slow_receiver_keeps_the_sender_inside_the_ack_window`. Total 25.

If `a_slow_receiver_keeps_the_sender_inside_the_ack_window` fails on the `widest <= limit` assert, the sender does not wait for `ChunkAck` (Plan 03 `stream` must check `window_open()` before each chunk); if it times out, the receiver does not send `ChunkAck` every 16 chunks. If `a_receiver_gone_after_the_final_ack_leaves_the_sender_unconfirmed` sees `Failed`, the sender's peer-gone path ignores `awaiting_result()` or the sleep before `hb.stop()` is too short for the final ack to arrive (index R4 as amended).

If a drop test times out waiting for `Failed`, `on_peer_gone` is not reaching the service (check `run_socket`'s end and `remove_local` from Step 0b); the idle limit is 120 s here on purpose, so it can never be the reason a test passes. Fix Plan 3 code, never the assertion. If the sender of a dropped connection ends `Cancelled` instead of `Failed`, the spec table (section 10) says "Sending failed", so fix the service.

- [ ] **Step 5: Run the e2e suite ten times**

Run:

```bash
for i in 1 2 3 4 5 6 7 8 9 10; do cargo test -j4 -p rocket-app e2e_transfer 2>&1 | tail -n 3; done
```

Expected: ten times `test result: ok. 25 passed`. Any failure is a timing bug: read which `wait_until` message timed out and fix the cause, do not lengthen the wait.

- [ ] **Step 6: Write the adapter trust test in `src-tauri`**

Plan 2 already pins the adapter's trust rule in `an_imported_collection_has_no_trust_record_and_no_elevated_access`. This test adds the received-collection case end to end through the adapter: the display name with the ` (from <sender>)` suffix, a script entry, and a hostile `opencollection.yml` that asks for Developer mode with the real key (`extensions.rocketapi.sandboxMode`, read by `crates/rocket-infra/src/fs_collection/settings.rs`; a `config:` key is ignored and would prove nothing). `rocket_export::bruno_zip` strips the `rocketapi` namespace, so the ZIP is written by hand with Plan 2's `write_zip` test helper. First confirm the helper and Plan 2's test exist:

```bash
grep -n "fn write_zip\|fn an_imported_collection_has_no_trust_record_and_no_elevated_access" src-tauri/src/import_wiring.rs
grep -n "rocketapi" src-tauri/src/import_wiring.rs
```

Expected: the first prints two lines (the helper and Plan 2's test); the second prints at least one line (Plan 2's fixture uses the real key, index R1). If Plan 2's fixture still says `config:`, Plan 2 is not done: fix it there first.

Then, in `src-tauri/src/import_wiring.rs`, inside `mod tests`, add this test (merge the imports with the ones already there: `rocket_app::CollectionImportPort`, `rocket_collection::CollectionRepository`, `rocket_collection::settings::SandboxMode`, `rocket_infra::{FsCollectionRepo, FsCollectionTrustStore}` and `effective_capabilities` from the same path Plan 2's test uses):

```rust
    #[test]
    fn a_received_zip_asking_for_developer_mode_gets_no_trust_record() {
        let ws = tempfile::TempDir::new().expect("workspace");
        std::fs::create_dir_all(ws.path().join("collections")).expect("collections dir");
        let shared = Arc::new(Mutex::new(ws.path().to_path_buf()));
        let adapter = ImporterAdapter::new(shared, ws.path().join("import-tmp"));

        let zips = tempfile::TempDir::new().expect("zip dir");
        let zip_path = zips.path().join("Orders.zip");
        write_zip(
            &zip_path,
            &[
                (
                    "Orders/opencollection.yml",
                    "opencollection: \"1.0.0\"\ninfo:\n  name: Orders\nextensions:\n  rocketapi:\n    sandboxMode: developer\n",
                ),
                (
                    "Orders/users/get.yml",
                    "info:\n  name: get\nhttp:\n  method: GET\n  url: https://x.test\nruntime:\n  scripts:\n  - type: before-request\n    code: \"rok.setVar('a', 1)\"\n",
                ),
            ],
        );

        let imported = adapter
            .import_zip(&zip_path, "Orders (from ann)")
            .expect("the import succeeds");
        assert_eq!(imported.name, "Orders (from ann)");

        // The requested mode reached the disk, so the next assert can fail.
        let repo = FsCollectionRepo::new_standalone(ws.path().join("collections"));
        let requested = repo.get_settings(&imported.name).expect("settings");
        assert_eq!(requested.sandbox_mode, SandboxMode::Developer, "the ZIP asked for it");

        // No trust record exists, so the effective mode is Safe: scripts run in the Safe
        // sandbox, and Developer mode, host env, vault and agent runs stay denied.
        let trust_file = ws.path().join("trust.yml");
        let store = FsCollectionTrustStore::new(trust_file.clone());
        let identity = repo.collection_identity(&imported.name).expect("identity");
        assert_eq!(store.grant_for(&identity).expect("read"), None, "no trust record");
        let effective = effective_capabilities(&repo, &store, &imported.name);
        assert_eq!(effective.sandbox_mode, SandboxMode::Safe);
        assert!(!effective.process_env && !effective.agent_run);
        assert!(!effective.vault_values && !effective.vault_certificates);
        assert!(!trust_file.exists(), "importing never writes a trust grant");
    }
```

- [ ] **Step 7: Run the adapter test, and see the guard fail once**

Run: `cargo test -j4 -p rocket a_received_zip_asking_for_developer_mode_gets_no_trust_record`

Expected: PASS (this pins Plan 2 behaviour, so there is no new code to write). To prove the test can fail, change `extensions:\n  rocketapi:\n    sandboxMode: developer` in the fixture to `config:\n  sandboxMode: developer` and run it again. Expected: FAIL on `the ZIP asked for it` (left `Safe`, right `Developer`). Put the real key back and run once more. Expected: PASS. If the first run fails on `the ZIP asked for it` with the real key, the importer drops `extensions`: record `Ruling: importer strips extensions.rocketapi`, delete only that one assert, and keep the rest. If the collection folder is not `collections/Orders (from ann)`, read what `ImportService` created and fix the name, never the trust asserts.

- [ ] **Step 8: Final verification**

Run, one crate at a time:

```bash
cargo fmt -p rocket-app -- --check
cargo fmt -p rocket -- --check
cargo check -j4 -p rocket-app --tests
cargo test -j4 -p rocket-app e2e
cargo check -j4 -p rocket
cargo test -j4 -p rocket transfer
cargo test -j4 -p rocket ipc_acl
cargo test -j4 -p rocket import_wiring
cargo clippy -j4 -p rocket-app --tests
```

Expected: both `fmt` checks print nothing; every `check` finishes; `e2e` passes all 8 older and 25 new tests; the `rocket` filters pass; clippy reports no warnings in the new code.

No commit; reviewed at the plan end.

---

## Self-review

- Spec section 2 (decisions 3 name clash and never overwrite, 4 ask every time and 2 minute expiry, 6 no trust record, scripts in the Safe sandbox): Task 2 (`a_name_clash_gets_the_from_sender_suffix`, `an_offer_nobody_answers_expires...`), Task 3 (`a_received_zip_asking_for_developer_mode_gets_no_trust_record`, with the real `extensions.rocketapi.sandboxMode` key and a check that the requested mode reached the disk). Scripts are not gated (index R1), so no test claims a script does not run.
- Section 3 success criteria: push and import, decline, expiry, dropped connection, damaged payload, unsafe ZIP each end with no collection (Tasks 2 and 3); the secret check; the exact size limit is refused before any offer (Task 3, with the exact boundary left to Plans 01 and 03 and said so).
- Section 5 flow and section 6 protocol over the real `/peer` socket: Task 2 happy path and both pending-offer cancels, Task 3 tampered chunk, busy, the ack window with a slow receiver (`a_slow_receiver_keeps_the_sender_inside_the_ack_window`) and `Unconfirmed` after the last chunk. The idle-limit, lost-frame and liveness rules need paused time and are Plan 03's service tests.
- Section 7 limits: `MAX_PENDING_INCOMING` (fourth offer busy), `MAX_TRANSFER_BYTES` (over-limit), `CHUNK_BYTES` (single chunk and multi-chunk fixtures). `MAX_UNZIPPED_BYTES` and `MAX_ZIP_ENTRIES` stay in Plan 02's validator tests.
- Section 8 security: staging deleted on success, abort, expiry and at start (restart test, dropped-connection tests, `the_startup_purge_clears_staging_and_stale_temp_dirs`), audit with the short id and `outcome` (`completed`, `unconfirmed` on the sender; `completed`, `damaged`, `rejected` on the receiver), unsafe ZIP refused, no write before Accept (declined, expired and cancelled tests assert no import).
- Section 9 `src-tauri`: commands, `app-commands.txt`, the `network-transfer` channel, wiring with `ImporterAdapter::new(.., network_dir.join("import-tmp"))` and the startup purge through `purge_transfer_temp`, which logs every error and is called from `lib.rs` (Task 1, unit test plus grep).
- Section 10 errors table: decline, sender cancels a pending offer, expired, over 10 MiB, busy, dropped, dropped after the final ack (`Unconfirmed`; before it, `Failed`, in Plan 03), damaged, unsafe ZIP. "Import fails" is covered by Plan 03's service tests with a failing port.
- Section 11 testing: end to end over the real transport (push, decline, drop, tamper, throttled receiver, cancel of a pending offer by either side, a send without environments), secret check, wire contract for the channel and envelope.
- The result frame: the sender stays open until the receiver's `TransferResult` (Task 2 `the_sender_shows_sent_only_after_the_receivers_result`), and `Damaged`, `Rejected` and `Failed` (with the importer's message) reach the sender and the receiver event (Task 3). A peer that vanishes ends both sides through `on_peer_gone`, with the production 120 s idle limit (Task 3 drop and unpair tests).
- Section 10 "Import fails" is covered end to end here (`a_failing_import_fails_both_sides_and_carries_the_importer_message`), the service-level cases stay in Plan 03.
- Not covered here on purpose: frontend (Plan 05), the rate limit of 5 offers a minute (pinned by Plan 01's `offers_per_peer_per_minute_are_limited_and_the_window_slides` on `TransferLimits`; the service only calls `admit_offer`; no end to end test), the `SENDER_RESULT_TIMEOUT` expiry itself (a Plan 03 service test with paused time), manual checks (Plan 05).
- Index R8: the hooks are `#[cfg(test)] pub(crate)` and every mention in this plan says so; Task 3 Step 0b counts the `cfg(test)` lines.
- Rulings to record in the ledger: no command-level unit test; tamper through a sender hook, not a relay; the slow receiver is a test staging wrapper on a multi-thread runtime, not a hook; expiry through Plan 03's `new_with_timing`, not a paused clock; the import is faked in `rocket-app` and the real adapter is tested in `src-tauri`; what Step 0b of Task 3 found about the Plan 03 hooks and `on_peer_gone`.
