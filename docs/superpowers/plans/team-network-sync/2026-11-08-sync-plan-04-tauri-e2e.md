# Team Network Sync Plan 04: Tauri Commands, Wiring and End-to-End Tests

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 4 of 5, title "Team network sync: Plan 4, Tauri and end to end".** Previous: `2026-11-08-sync-plan-03-service.md` (01 domain, 02 storage and merge and 03 service must be applied and green). Next: `2026-11-08-sync-plan-05-frontend.md`. Recommended model: Sonnet. Index (binding names): `docs/superpowers/plans/team-network-sync/00-plan-index.md`. No commit during the plan.

**Goal:** A user can keep a sent collection in sync from the app. `network_send_collection` gains `keep_in_sync`; six new commands (`network_sync_start`, `network_sync_review`, `network_sync_confirm`, `network_sync_cancel`, `network_list_links`, `network_unlink`) drive `SyncService`; `SyncEvent` reaches the webview on the `network-sync` channel; the service, the link repository and the frame router are wired at startup. Then the whole feature is proven over the real transport: two real nodes on loopback link a collection at the first send and then sync it, with a clean two-way sync, each of the three conflict choices, a delete against an edit, a rename in each direction, a tampered payload, a base mismatch, an unlink on one side, a pairing removed during a sync, a declined review and a cancel. The receiving copy keeps its own token, `uid`, `extensions.rocketapi` settings and saved examples after the other side's edit arrives (spec section 3).

**Architecture:**
- `src-tauri` gets a new module `commands/network_sync.rs` with the six sync commands and their IPC DTOs (`SyncStartedDto`, `DecisionDto`, `LinkDto`, all camelCase). The commands parse ids, call `SyncService` through `State<'_, Arc<SyncService>>` and return its `DomainError` unchanged. The existing `network_send_collection` in `commands/network.rs` only gains the `keep_in_sync: bool` parameter and passes it on. The channel `network-sync` is not added to any table: `TauriEventBus` takes it from `PublishedEvent::channel()` (Plan 1) and adds `seq` and `tsMs`, so a bus test pins it, the same way `network-transfer` is pinned.
- `lib.rs` builds, in this order: `FsLinkRepo` under `network_dir.join("links")`, the `SyncCollectionPort` implementation of Plan 2, `SyncService` (sharing the transfer staging folder that is purged at start), then `TransferService` (which now also holds the `SyncService`, for `create_link` at the first send), then `FrameRouter` over both, and last `network_svc.set_transfer_sink(router)`. `TransferService::new` registers itself as the sink; the router replaces it, so the router call must come after it. There is no new shutdown hook: sync sessions end through `on_peer_gone` when `NetworkSlot::stop_for_good` closes the node's sockets, and an unfinished sync leaves only staged files that the next start purges.
- The end-to-end tests are a third unit-test module in `rocket-app`, `network/e2e_sync_tests.rs`, next to `e2e_tests.rs` and `e2e_transfer_tests.rs`, reusing the real two-node harness (`online_pair`, `wait_until`, `row`, `pair`). Each node gets a real `NetworkService`, a real `TransferService`, a real `SyncService`, a real `FrameRouter`, a real `FsLinkRepo` in a temp folder and a real `FsTransferStaging`. **Test-only fakes:** `MemCollections` (an in-memory implementation of `SyncCollectionPort` that runs the real export for manifests and files and the real `rocket_export::sync_merge::merge_file` for `Put`, and applies all-or-nothing), `SyncImport` (the import port of the first send: it validates the ZIP with the real validator and seeds `MemCollections`), and `WireTap` (a `PeerSender` decorator around the real `NetworkService` that can flip one base64 character of the first chunk of one sync payload kind). The shipped adapters live in `src-tauri` or in Plan 2 and are covered by their own tests.
- **No new production seam.** Tampering uses the `PeerSender` trait that `SyncDeps.network` already takes, so no `#[cfg(test)]` hook is needed in `SyncService`. Expiry is not tested here: the 120 s offer lifetime and the idle limit need paused time and stay in Plan 3's service tests. Dropped connections end through `on_peer_gone`.
- Links are seeded in most tests with `SyncService::create_link` on both sides (the index-pinned call the first send also uses) from the same exported bytes, so a test starts from a known base. One test (`the_first_send_with_keep_in_sync_creates_equal_links_on_both_sides`) goes through the real `TransferService::send(.., keep_in_sync = true)` and real import path and proves the two bases are equal without any extra message.

**Tech Stack:** Rust, Tauri 2, tokio (real time), `rocket-network`, `rocket-app`, `rocket-infra` (`FsLinkRepo`, `FsTransferStaging`, `test-support`), `rocket-export`, `serde_json`, `zip` 2, `tempfile`.

**Spec:** `docs/superpowers/specs/2026-11-08-team-network-sync-collection-design.md`, sections 6 (flow and planner table), 8 (errors), 9 (interface, `src-tauri` part), 11 (placement) and 12 (testing), and the success criteria in section 3.

Prerequisites: Plans 01 to 03 of this series are applied. Names used from them (all from the index contract; confirm the module paths in Step 0b of each task because the index fixes names, not file paths):
- `rocket_network`: `LinkId`, `SyncId`, `Manifest`, `LinkRecord`, `LinkState`, `LinkRepository`, `Decision`, `Resolution`, `ConflictKind`, `SyncEvent` (variants `ReviewReady`, `ConfirmNeeded`, `Progress`, `Finished`, `LinkChanged`), `SyncSummary`, `ConflictView`, `SyncRole`, `SyncFinish`, `SyncPayloadKind`, `PeerMessage::{SyncOffer, Chunk}`.
- `rocket_app`: `SyncService` (`new`, `create_link`, `start`, `review`, `confirm`, `cancel`, `links`, `unlink`), `SyncDeps`, `LinkView`, `FrameRouter`, `SyncCollectionPort`, `FileChange`, `SyncFile`, `manifest_from_zip`, `TransferService::send` with the `keep_in_sync` argument, `TransferDeps` (with the field that carries the `SyncService`), `PeerSender`.
- `rocket_infra`: `FsLinkRepo::new(dir)`, `FsTransferStaging::new(dir)`; `rocket_export::sync_merge::merge_file`.
- `rocket_audit`: `AuditEventKind::{NetworkSyncSent, NetworkSyncReceived, NetworkLinkRemoved}`.

Names the index does not pin and this plan therefore assumes (Step 0b of the owning task confirms each one; a different real name is a one-line change at the places listed):
1. `LinkView { link_id: String, device_id: String, short_id: String, display_name: String, collection_name: String, state: LinkState, updated_at: String }` (only `link_dto_from` in Task 1 reads it).
2. `FrameRouter::new(transfer: Arc<TransferService>, sync: Arc<SyncService>) -> Arc<FrameRouter>` (lib.rs and the test harness).
3. `TransferDeps` has a field `sync: Arc<SyncService>` (lib.rs and the test harness).
4. `TransferService::send(&self, device: &DeviceId, collection: &str, include_environments: bool, keep_in_sync: bool)` (the command and the harness).
5. The `SyncCollectionPort` implementation for the real app is `FsSyncCollections::new(Arc<CollectionExportService>, Arc<Mutex<PathBuf>>)` (lib.rs only).
6. The audit `outcome` strings of the sync kinds are the snake_case `SyncOutcome` values: `applied` and `failed` (tests of the audit lines only).
7. `ImportedCollection` is still `{ name }` (the test import fake only).

## Global Constraints

- `-j4` on every cargo command, one crate at a time. Never `cargo test --workspace` or `--all`. The `src-tauri` package is named `rocket` (`src-tauri/Cargo.toml`), so use `cargo check -j4 -p rocket` and `cargo test -j4 -p rocket <filter>`.
- No `unwrap()` in production code. Tests use `expect("reason")` (a repo hook rejects a bare unwrap call, tests included). Mutex locks use `.lock().unwrap_or_else(|e| e.into_inner())` in new helpers.
- Never hold a `std::sync::Mutex` guard across an `.await`.
- Commands stay thin: validate, call the service, map the error. No sync logic in `commands/network_sync.rs`. A new command goes into `src-tauri/app-commands.txt` (sorted) and `generate_handler!`, or it is denied at runtime.
- `#[serde(rename_all = "camelCase")]` only on IPC DTOs. This plan adds three: `SyncStartedDto { syncId }`, `LinkDto { linkId, deviceId, displayName, shortId, collectionName, state, updatedAt }` and the input `DecisionDto { path, resolution }`, each pinned by a `wire_contract_*` test. The event payload keeps its own wire format (camelCase tag, snake_case variant fields, camelCase helper DTOs) and is pinned by `wire_contract_sync_events_keep_the_key_styles`. Do not rename.
- No production hook is added for the tests. If a test needs a seam, it is a test-only decorator or fake in `e2e_sync_tests.rs`.
- `cargo fmt` always names the crate: `cargo fmt -p <crate> -- --check`. Never `--all`, never a bare `cargo fmt`.
- Logs carry codes and counts, never collection names, file paths, file contents or full device ids.
- Code comments are short, full sentences that end with a period. No emojis.
- Every wait in the e2e tests goes through `wait_until` (20 s limit), never a fixed sleep. This plan adds no fixed sleep.
- No commit during the plan. The user reviews it at the end, then commits through `dev-workflow-skills:1-git-commit` with explicit paths (suggested split: `feat(network): sync commands, event channel and wiring`, `test(network): end-to-end sync tests`).
- Line numbers drift. Anchor every edit on quoted text.

## Review Focus

Each line names the test that pins it.

1. **The receiver's local-only data survives an incoming edit.** B keeps its own token, `uid`, `extensions.rocketapi` and examples, and A never receives them. Task 3: `b_keeps_its_token_uid_extensions_and_examples_after_a_edit_syncs`.
2. **Each conflict choice gives the expected files on both sides, and both bases are updated.** Task 3: `a_conflict_keep_mine_gives_the_planners_version_on_both_sides`, `a_conflict_keep_theirs_gives_the_starters_version_on_both_sides`, `a_conflict_keep_both_keeps_the_original_path_and_adds_a_from_peer_copy`.
3. **A rename is a rename, not a delete plus an add.** The receiver's local keys are still in the moved file, in both directions. Task 3: `a_rename_by_the_starter_is_a_rename_on_the_planner`, `a_rename_by_the_planner_is_a_rename_on_the_starter`.
4. **A delete against an edit is a conflict, a delete alone is taken.** Task 3: `a_delete_nobody_touched_is_taken`, `a_delete_against_an_edit_keep_mine_restores_the_file_on_both_sides`, `a_delete_against_an_edit_keep_theirs_deletes_it_on_both_sides`.
5. **A damaged payload, a declined review, a cancel, a pairing removed mid-sync, a base mismatch and a missing link each change no file and no base.** Task 3: `a_tampered_payload_changes_nothing`, `declining_the_review_changes_nothing`, `the_starter_cancels_while_the_planner_reviews`, `removing_the_pairing_during_a_sync_ends_it_with_no_change`, `a_base_mismatch_ends_the_sync_before_any_review`, `an_unlink_on_one_side_ends_the_next_sync_with_link_missing`.
6. **The frame router sends each frame to the right service.** A first send and a sync run at the same time and both finish. Task 3: `a_send_and_a_sync_can_run_at_the_same_time`; the first send path: `the_first_send_with_keep_in_sync_creates_equal_links_on_both_sides` and `a_send_without_keep_in_sync_creates_no_link`.
7. **The wiring order is right** (`SyncService`, then `TransferService`, then `FrameRouter`, then `set_transfer_sink`). Task 2: a grep that prints the four calls in that order. The e2e harness builds the same order.
8. **Every new command is reachable.** Task 1: `the_sync_commands_are_in_the_acl_list` and the existing `app_commands_file_matches_generate_handler`; Task 3 final step greps all seven names in both lists.
9. **The channel and the envelope.** Task 1: `sync_events_emit_on_the_network_sync_channel`, `wire_contract_sync_events_keep_the_key_styles`.

Also pinned: the first send creates equal links with equal base hashes on both sides without a message (Task 3, test 1), and a secret value never crosses (Task 3, tests 2 and 6 to 8 assert the other side's token is absent).

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `src-tauri/src/commands/network_sync.rs` | create | Six sync commands, `SyncStartedDto`, `DecisionDto`, `LinkDto`, id and resolution parsing, DTO and ACL tests |
| `src-tauri/src/commands/mod.rs` | modify | `pub mod network_sync;` |
| `src-tauri/src/commands/network.rs` | modify | `network_send_collection` gains `keep_in_sync: bool` |
| `src-tauri/app-commands.txt` | modify | Add the six new command names (sorted) |
| `src-tauri/src/lib.rs` | modify | `generate_handler!`, build and manage `SyncService`, link repo, router, `set_transfer_sink` |
| `src-tauri/src/tauri_event_bus.rs` | modify | Tests: `network-sync` channel, envelope and key styles of `SyncEvent` |
| `crates/rocket-app/src/lib.rs` | modify (only if needed) | Re-export `SyncService`, `LinkView` and `FrameRouter` for the Tauri crate |
| `crates/rocket-app/src/network/mod.rs` | modify | Register `e2e_sync_tests` (test builds only) |
| `crates/rocket-app/src/network/test_support.rs` | modify | `RecordingEvents` also records `SyncEvent` (`syncs()`), unless Plan 3 already did |
| `crates/rocket-app/src/network/e2e_sync_tests.rs` | create | Sync harness, fakes (`MemCollections`, `SyncImport`, `WireTap`), fixtures and the end-to-end tests |
| `crates/rocket-app/src/network/e2e_tests.rs`, `e2e_transfer_tests.rs` | none | Their `pub(super)` helpers (`online_pair`, `wait_until`, `row`, `pair`, `E2eNode`, `file`, `normal`, `zip_entries`, `count_files`) are reused as they are |

---

### Task 1: Commands, DTOs, ACL entries and the event channel

**Files:**
- Create: `src-tauri/src/commands/network_sync.rs`
- Modify: `src-tauri/src/commands/mod.rs`, `src-tauri/src/commands/network.rs`, `src-tauri/app-commands.txt`, `src-tauri/src/lib.rs` (the `generate_handler!` list only), `src-tauri/src/tauri_event_bus.rs`
- Modify only if a name is not yet exported: `crates/rocket-app/src/lib.rs`

**Interfaces:**
- Consumes: `SyncService::{start, review, confirm, cancel, links, unlink}`, `TransferService::send(.., keep_in_sync)`, `LinkId::parse`, `SyncId::parse`, `Decision`, `Resolution`, `LinkState`, `LinkView`, `SyncEvent` and its helper types, the existing `parse_device_id`, `emit_channels` and `envelope` of the bus.
- Produces (binding, from the index): `network_send_collection(device_id, collection, include_environments, keep_in_sync: bool) -> Result<SendStartedDto, DomainError>`, `network_sync_start(link_id: String) -> Result<SyncStartedDto, DomainError>`, `network_sync_review(sync_id: String, accept: bool, decisions: Vec<DecisionDto>) -> Result<(), DomainError>`, `network_sync_confirm(sync_id: String, accept: bool) -> Result<(), DomainError>`, `network_sync_cancel(sync_id: String) -> Result<(), DomainError>`, `network_list_links() -> Result<Vec<LinkDto>, DomainError>`, `network_unlink(link_id: String) -> Result<(), DomainError>`. `SyncStartedDto { sync_id }`, `LinkDto { link_id, device_id, display_name, short_id, collection_name, state, updated_at }` (`state` is one of `in_sync`, `changes_waiting`, `out_of_date`, `broken`), `DecisionDto { path, resolution }` with `resolution` one of `keep_mine`, `keep_theirs`, `keep_both`. Managed state `Arc<SyncService>` (added in Task 2).

Rules the tests pin:
- The DTOs serialize or deserialize with exactly the camelCase keys above.
- Unknown resolution text and malformed ids are `DomainError::InvalidInput` with a fixed sentence, never the raw input.
- The six new names are in `app-commands.txt`.
- `SyncEvent` leaves on exactly one channel, `network-sync`, with `seq` and `tsMs` added, tag camelCase, variant fields snake_case, helper DTO keys camelCase.

- [ ] **Step 0: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 0b: Confirm the names this task imports**

Run:

```bash
grep -rn "pub struct SyncService\|pub struct LinkView\|pub struct FrameRouter\|pub struct SyncDeps" crates/rocket-app/src
grep -rn "pub async fn start\|pub async fn review\|pub async fn confirm\|pub async fn cancel\|pub fn links\|pub async fn unlink" crates/rocket-app/src/network
grep -rn "pub struct LinkView" -A12 crates/rocket-app/src/network
grep -n "pub async fn send" -A8 crates/rocket-app/src/network/transfer_send.rs
grep -n "SyncEvent\|SyncSummary\|ConflictView\|SyncRole\|SyncFinish\|LinkId\|SyncId\|Decision\|Resolution\|LinkState" crates/rocket-network/src/lib.rs
grep -n "pub use" crates/rocket-app/src/lib.rs | grep -i "network"
```

Expected: each type is found once. Note (a) the real field list of `LinkView` and adapt `link_dto_from` below if it differs from assumption 1; (b) the real argument order of `send` (assumption 4); (c) whether `rocket_network` re-exports the sync names at its crate root (every snippet below uses the crate-root path; otherwise use the module path, for example `rocket_network::events::SyncEvent`); (d) that `SyncId::parse(&str) -> DomainResult<SyncId>` exists next to `LinkId::parse`. If `rocket-app/src/lib.rs` re-exports `NetworkService` through `pub use network::{...}` but does not list `SyncService`, `LinkView` and `FrameRouter`, add them in Step 3a. `SyncEvent` is `Debug, Clone, PartialEq, Eq, Serialize` like `TransferEvent` (the e2e tests compare finish outcomes, so `SyncFinish`, `SyncRole`, `ConflictKind`, `Resolution` and `SyncPayloadKind` must also derive `Debug, Clone, Copy, PartialEq, Eq`); if one lacks `Copy` or `PartialEq`, add the derive in `rocket-network` (a one-line change in a Plan 1 file) and record a ruling.

- [ ] **Step 1: Write the failing tests**

(a) Create `src-tauri/src/commands/network_sync.rs` with only the test module for now (Step 3 fills in the rest above it). The tests:

```rust
#[cfg(test)]
mod tests {
    use rocket_app::LinkView;
    use rocket_network::{LinkId, LinkState, Resolution, SyncId};
    use rocket_shared::error::DomainError;

    use super::*;

    fn keys(value: &serde_json::Value) -> Vec<String> {
        let mut keys: Vec<String> = value
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    fn sample_view(state: LinkState) -> LinkView {
        LinkView {
            link_id: LinkId::random().as_str().to_string(),
            device_id: "ABCDEFGH".to_string(),
            short_id: "K7QF-2M9X".to_string(),
            display_name: "Ana".to_string(),
            collection_name: "Orders".to_string(),
            state,
            updated_at: "2026-11-08T10:00:00+00:00".to_string(),
        }
    }

    #[test]
    fn wire_contract_sync_started_dto_uses_camel_case_keys() {
        let dto = SyncStartedDto {
            sync_id: "0123456789abcdef0123456789abcdef".to_string(),
        };
        let value = serde_json::to_value(&dto).expect("json");
        assert_eq!(keys(&value), vec!["syncId"]);
        assert_eq!(value["syncId"], "0123456789abcdef0123456789abcdef");
    }

    #[test]
    fn wire_contract_link_dto_uses_camel_case_keys() {
        let dto = link_dto_from(sample_view(LinkState::InSync));
        let value = serde_json::to_value(&dto).expect("json");
        assert_eq!(
            keys(&value),
            vec![
                "collectionName",
                "deviceId",
                "displayName",
                "linkId",
                "shortId",
                "state",
                "updatedAt"
            ]
        );
        assert_eq!(value["collectionName"], "Orders");
        assert_eq!(value["state"], "in_sync");
    }

    #[test]
    fn link_states_map_to_the_four_wire_names() {
        let names: Vec<&str> = [
            LinkState::InSync,
            LinkState::ChangesWaiting,
            LinkState::OutOfDate,
            LinkState::Broken,
        ]
        .iter()
        .map(link_state_name)
        .collect();
        assert_eq!(
            names,
            vec!["in_sync", "changes_waiting", "out_of_date", "broken"]
        );
    }

    #[test]
    fn wire_contract_decision_dto_reads_camel_case_keys() {
        let dto: DecisionDto = serde_json::from_value(serde_json::json!({
            "path": "users/get.yml",
            "resolution": "keep_both"
        }))
        .expect("a decision parses");
        assert_eq!(dto.path, "users/get.yml");
        assert_eq!(dto.resolution, "keep_both");
    }

    #[test]
    fn decisions_parse_the_three_resolutions_and_refuse_others() {
        let dtos = vec![
            DecisionDto {
                path: "a.yml".into(),
                resolution: "keep_mine".into(),
            },
            DecisionDto {
                path: "b.yml".into(),
                resolution: "keep_theirs".into(),
            },
            DecisionDto {
                path: "c.yml".into(),
                resolution: "keep_both".into(),
            },
        ];
        let decisions = to_decisions(dtos).expect("all three parse");
        assert_eq!(decisions[0].resolution, Resolution::KeepMine);
        assert_eq!(decisions[1].resolution, Resolution::KeepTheirs);
        assert_eq!(decisions[2].resolution, Resolution::KeepBoth);
        assert_eq!(decisions[2].path, "c.yml");

        let err = to_decisions(vec![DecisionDto {
            path: "a.yml".into(),
            resolution: "secret-marker-text".into(),
        }])
        .expect_err("an unknown resolution is refused");
        assert_eq!(
            err,
            DomainError::InvalidInput(
                "The decision must be keep_mine, keep_theirs or keep_both.".to_string()
            )
        );
    }

    #[test]
    fn ids_parse_or_are_invalid_input() {
        let link = LinkId::random();
        assert_eq!(
            parse_link_id(link.as_str()).expect("a fresh id parses").as_str(),
            link.as_str()
        );
        let sync = SyncId::random();
        assert_eq!(
            parse_sync_id(sync.as_str()).expect("a fresh id parses").as_str(),
            sync.as_str()
        );
        assert_eq!(
            parse_link_id("not-an-id").expect_err("refused"),
            DomainError::InvalidInput("That link id is not valid.".to_string())
        );
        assert_eq!(
            parse_sync_id("not-an-id").expect_err("refused"),
            DomainError::InvalidInput("That sync id is not valid.".to_string())
        );
    }

    #[test]
    fn the_sync_commands_are_in_the_acl_list() {
        let list = include_str!("../../app-commands.txt");
        for name in [
            "network_send_collection",
            "network_sync_start",
            "network_sync_review",
            "network_sync_confirm",
            "network_sync_cancel",
            "network_list_links",
            "network_unlink",
        ] {
            assert!(
                list.lines().any(|line| line.trim() == name),
                "{name} is missing from app-commands.txt"
            );
        }
    }
}
```

If Step 0b found that `SyncId` has no `random()` (Plan 1 gives `LinkId::random()` and says `SyncId` has the same shape), build the id from a `LinkId` text instead: `SyncId::parse(LinkId::random().as_str())`.

(b) In `src-tauri/src/tauri_event_bus.rs`, inside `mod tests`, add the import `use rocket_network::{ConflictKind, ConflictView, SyncEvent, SyncFinish, SyncRole, SyncSummary};` next to the other `use` lines, and add these tests after the transfer tests:

```rust
    fn sync_samples() -> Vec<SyncEvent> {
        let summary = SyncSummary {
            added_here: 1,
            changed_here: 2,
            deleted_here: 0,
            sent_added: 0,
            sent_changed: 1,
            sent_deleted: 0,
            scripts_changed: vec!["users/get.yml".to_string()],
            auth_changed: Vec::new(),
        };
        vec![
            SyncEvent::ReviewReady {
                sync_id: "s1".into(),
                link_id: "l1".into(),
                device_id: "dev".into(),
                short_id: "K7QF-2M9X".into(),
                display_name: "Ann".into(),
                collection_name: "Orders".into(),
                summary: summary.clone(),
                conflicts: vec![ConflictView {
                    path: "users/get.yml".into(),
                    kind: ConflictKind::BothChanged,
                    mine_hash: Some("aa".repeat(32)),
                    theirs_hash: None,
                }],
                role: SyncRole::Planner,
            },
            SyncEvent::ConfirmNeeded {
                sync_id: "s1".into(),
                link_id: "l1".into(),
                device_id: "dev".into(),
                short_id: "K7QF-2M9X".into(),
                display_name: "Ann".into(),
                collection_name: "Orders".into(),
                summary,
            },
            SyncEvent::Progress {
                sync_id: "s1".into(),
                done_bytes: 10,
                total_bytes: 100,
            },
            SyncEvent::Finished {
                sync_id: "s1".into(),
                link_id: "l1".into(),
                display_name: "Ann".into(),
                collection_name: "Orders".into(),
                outcome: SyncFinish::Applied,
                message: None,
            },
            SyncEvent::LinkChanged {
                link_id: "l1".into(),
            },
        ]
    }

    fn sorted_keys(value: &serde_json::Value) -> Vec<String> {
        let mut keys: Vec<String> = value
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    fn sorted(mut names: Vec<&str>) -> Vec<String> {
        names.sort_unstable();
        names.into_iter().map(str::to_string).collect()
    }

    #[test]
    fn sync_events_emit_on_the_network_sync_channel() {
        for event in sync_samples() {
            assert_eq!(emit_channels(&event), vec!["network-sync"]);
        }
    }

    #[test]
    fn wire_contract_sync_events_keep_the_key_styles() {
        let samples = sync_samples();
        let envelope_of = |event: &SyncEvent| envelope(event, 7, 1234).expect("envelope");

        let review = envelope_of(&samples[0]);
        assert_eq!(review["type"], "reviewReady");
        assert_eq!(review["seq"], 7);
        assert_eq!(review["tsMs"], 1234);
        assert_eq!(
            sorted_keys(&review),
            sorted(vec![
                "collection_name",
                "conflicts",
                "device_id",
                "display_name",
                "link_id",
                "role",
                "seq",
                "short_id",
                "summary",
                "sync_id",
                "tsMs",
                "type"
            ])
        );
        assert_eq!(
            sorted_keys(&review["summary"]),
            sorted(vec![
                "addedHere",
                "authChanged",
                "changedHere",
                "deletedHere",
                "scriptsChanged",
                "sentAdded",
                "sentChanged",
                "sentDeleted"
            ])
        );
        assert_eq!(
            sorted_keys(&review["conflicts"][0]),
            sorted(vec!["kind", "mineHash", "path", "theirsHash"])
        );
        assert!(review["conflicts"][0]["theirsHash"].is_null());
        assert!(review["role"].is_string());

        let confirm = envelope_of(&samples[1]);
        assert_eq!(confirm["type"], "confirmNeeded");
        assert_eq!(
            sorted_keys(&confirm),
            sorted(vec![
                "collection_name",
                "device_id",
                "display_name",
                "link_id",
                "seq",
                "short_id",
                "summary",
                "sync_id",
                "tsMs",
                "type"
            ])
        );

        let progress = envelope_of(&samples[2]);
        assert_eq!(progress["type"], "progress");
        assert_eq!(
            sorted_keys(&progress),
            sorted(vec!["done_bytes", "seq", "sync_id", "total_bytes", "tsMs", "type"])
        );

        let finished = envelope_of(&samples[3]);
        assert_eq!(finished["type"], "finished");
        assert!(finished["message"].is_null(), "no message is a JSON null");
        assert!(finished["outcome"].is_string());
        assert_eq!(
            sorted_keys(&finished),
            sorted(vec![
                "collection_name",
                "display_name",
                "link_id",
                "message",
                "outcome",
                "seq",
                "sync_id",
                "tsMs",
                "type"
            ])
        );

        let changed = envelope_of(&samples[4]);
        assert_eq!(changed["type"], "linkChanged");
        assert_eq!(
            sorted_keys(&changed),
            sorted(vec!["link_id", "seq", "tsMs", "type"])
        );
    }
```

- [ ] **Step 2: Run the tests and see them fail**

Register the module so the failure is about missing items and not a missing file. In `src-tauri/src/commands/mod.rs`, add `pub mod network_sync;` right after `pub mod network;`.

Run: `cargo test -j4 -p rocket network_sync`

Expected: FAIL to compile with "cannot find struct `SyncStartedDto` in this scope" and the same for `DecisionDto`, `link_dto_from`, `link_state_name`, `to_decisions`, `parse_link_id`, `parse_sync_id`. The bus tests (`cargo test -j4 -p rocket sync_events`) already PASS if Plan 1 is applied, because the channel comes from `SyncEvent::channel()`. That is the point of the pin: it fails only if someone changes the channel or the wire format. If they fail to compile, fix the `rocket_network` import path from Step 0b.

- [ ] **Step 3: Write the commands**

At the top of `src-tauri/src/commands/network_sync.rs` (above the test module) add:

```rust
//! Team network sync commands. They stay thin: parse text, call `SyncService`, map the result.
//! DTOs are camelCase and live here only; the domain types are not renamed.

use std::sync::Arc;

use rocket_app::{LinkView, SyncService};
use rocket_network::{Decision, LinkId, LinkState, Resolution, SyncId};
use rocket_shared::error::DomainError;
use serde::{Deserialize, Serialize};
use tauri::State;

// ----- DTOs -----

/// What the UI gets after a sync started.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStartedDto {
    pub sync_id: String,
}

/// One conflict outcome sent by the review dialog. `resolution` is `keep_mine`, `keep_theirs`
/// or `keep_both`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionDto {
    pub path: String,
    pub resolution: String,
}

/// One link for the menu status line and the link list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkDto {
    pub link_id: String,
    pub device_id: String,
    pub display_name: String,
    pub short_id: String,
    pub collection_name: String,
    /// `in_sync`, `changes_waiting`, `out_of_date` or `broken`.
    pub state: &'static str,
    /// RFC 3339.
    pub updated_at: String,
}

fn link_state_name(state: &LinkState) -> &'static str {
    match state {
        LinkState::InSync => "in_sync",
        LinkState::ChangesWaiting => "changes_waiting",
        LinkState::OutOfDate => "out_of_date",
        LinkState::Broken => "broken",
    }
}

fn link_dto_from(view: LinkView) -> LinkDto {
    LinkDto {
        link_id: view.link_id,
        device_id: view.device_id,
        display_name: view.display_name,
        short_id: view.short_id,
        collection_name: view.collection_name,
        state: link_state_name(&view.state),
        updated_at: view.updated_at,
    }
}

// ----- parsing -----

fn parse_link_id(raw: &str) -> Result<LinkId, DomainError> {
    LinkId::parse(raw)
        .map_err(|_| DomainError::InvalidInput("That link id is not valid.".to_string()))
}

fn parse_sync_id(raw: &str) -> Result<SyncId, DomainError> {
    SyncId::parse(raw)
        .map_err(|_| DomainError::InvalidInput("That sync id is not valid.".to_string()))
}

fn parse_resolution(raw: &str) -> Result<Resolution, DomainError> {
    match raw {
        "keep_mine" => Ok(Resolution::KeepMine),
        "keep_theirs" => Ok(Resolution::KeepTheirs),
        "keep_both" => Ok(Resolution::KeepBoth),
        _ => Err(DomainError::InvalidInput(
            "The decision must be keep_mine, keep_theirs or keep_both.".to_string(),
        )),
    }
}

fn to_decisions(dtos: Vec<DecisionDto>) -> Result<Vec<Decision>, DomainError> {
    dtos.into_iter()
        .map(|dto| {
            Ok(Decision {
                resolution: parse_resolution(&dto.resolution)?,
                path: dto.path,
            })
        })
        .collect()
}

// ----- commands -----

/// Starts a sync of one link. The peer is asked; nothing is written here yet.
#[tauri::command]
pub async fn network_sync_start(
    link_id: String,
    svc: State<'_, Arc<SyncService>>,
) -> Result<SyncStartedDto, DomainError> {
    let id = parse_link_id(&link_id)?;
    let sync_id = svc.start(&id).await?;
    Ok(SyncStartedDto {
        sync_id: sync_id.as_str().to_string(),
    })
}

/// The planner's answer to the review dialog: accept with one decision per conflict, or decline.
#[tauri::command]
pub async fn network_sync_review(
    sync_id: String,
    accept: bool,
    decisions: Vec<DecisionDto>,
    svc: State<'_, Arc<SyncService>>,
) -> Result<(), DomainError> {
    let id = parse_sync_id(&sync_id)?;
    let decisions = to_decisions(decisions)?;
    svc.review(&id, accept, decisions).await
}

/// The starter's answer to the confirm dialog.
#[tauri::command]
pub async fn network_sync_confirm(
    sync_id: String,
    accept: bool,
    svc: State<'_, Arc<SyncService>>,
) -> Result<(), DomainError> {
    let id = parse_sync_id(&sync_id)?;
    svc.confirm(&id, accept).await
}

/// Cancels a sync from either side. Nothing is applied.
#[tauri::command]
pub async fn network_sync_cancel(
    sync_id: String,
    svc: State<'_, Arc<SyncService>>,
) -> Result<(), DomainError> {
    let id = parse_sync_id(&sync_id)?;
    svc.cancel(&id).await
}

/// Every link of this node.
#[tauri::command]
pub fn network_list_links(
    svc: State<'_, Arc<SyncService>>,
) -> Result<Vec<LinkDto>, DomainError> {
    Ok(svc.links()?.into_iter().map(link_dto_from).collect())
}

/// Deletes the link and its base on this side. The collection is not touched.
#[tauri::command]
pub async fn network_unlink(
    link_id: String,
    svc: State<'_, Arc<SyncService>>,
) -> Result<(), DomainError> {
    let id = parse_link_id(&link_id)?;
    svc.unlink(&id).await
}
```

Now the send command. In `src-tauri/src/commands/network.rs` replace

```rust
    include_environments: bool,
    svc: State<'_, Arc<TransferService>>,
) -> Result<SendStartedDto, DomainError> {
    let id = parse_device_id(&device_id)?;
    let started = svc.send(&id, &collection, include_environments).await?;
```

with

```rust
    include_environments: bool,
    keep_in_sync: bool,
    svc: State<'_, Arc<TransferService>>,
) -> Result<SendStartedDto, DomainError> {
    let id = parse_device_id(&device_id)?;
    let started = svc
        .send(&id, &collection, include_environments, keep_in_sync)
        .await?;
```

and extend its doc comment with one line: `/// With `keep_in_sync` the first send also creates a link on both sides.`

- [ ] **Step 3a: Re-export the types from `rocket-app` (only if Step 0b found them missing)**

In `crates/rocket-app/src/lib.rs`, add `FrameRouter`, `LinkView` and `SyncService` to the existing `pub use network::{ ... }` list (keep the names already there). Run: `cargo check -j4 -p rocket-app`. Expected: finished with no errors.

- [ ] **Step 4: Register the commands**

In `src-tauri/app-commands.txt`, keep the file sorted. The network block becomes:

```
network_add_address
network_cancel_pairing
network_cancel_transfer
network_list_addresses
network_list_blocked
network_list_links
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
network_sync_cancel
network_sync_confirm
network_sync_review
network_sync_start
network_unblock
network_unlink
```

In `src-tauri/src/lib.rs`, in `generate_handler!`, after the line `commands::network::network_cancel_transfer,` add:

```rust
            commands::network_sync::network_sync_start,
            commands::network_sync::network_sync_review,
            commands::network_sync::network_sync_confirm,
            commands::network_sync::network_sync_cancel,
            commands::network_sync::network_list_links,
            commands::network_sync::network_unlink,
```

- [ ] **Step 5: Run the tests and see them pass**

Run: `cargo test -j4 -p rocket network_sync`

Expected: 7 passed (`wire_contract_sync_started_dto_uses_camel_case_keys`, `wire_contract_link_dto_uses_camel_case_keys`, `link_states_map_to_the_four_wire_names`, `wire_contract_decision_dto_reads_camel_case_keys`, `decisions_parse_the_three_resolutions_and_refuse_others`, `ids_parse_or_are_invalid_input`, `the_sync_commands_are_in_the_acl_list`).

Run: `cargo test -j4 -p rocket sync_events`

Expected: 2 passed (`sync_events_emit_on_the_network_sync_channel`, `wire_contract_sync_events_keep_the_key_styles`). If the key-style test fails on a helper key (for example `addedHere`), Plan 1's `SyncSummary` serde rename differs from the index; fix Plan 1 (index: helper DTOs are camelCase), never the test.

Run: `cargo test -j4 -p rocket ipc_acl`

Expected: all PASS, including `app_commands_file_matches_generate_handler` and `app_commands_file_is_sorted_and_unique`.

Run: `cargo check -j4 -p rocket`

Expected: finished with no errors. A "mismatched types" error at `svc.send(` means the real `send` argument order differs from assumption 4; fix the one call.

- [ ] **Step 6: Note on what is not unit-tested here**

The commands take a Tauri `State`, which needs a running app, and they hold no logic beyond the id and resolution parsing (pinned above) and the `LinkView` to `LinkDto` field copy (pinned by `wire_contract_link_dto_uses_camel_case_keys`). The behaviour behind them is proven end to end in Task 3 through the same `SyncService` calls. Record in the ledger: `Ruling: no command-level unit test; thin commands, parsing and DTO keys covered, behaviour covered by the e2e tests.`

---

### Task 2: Startup wiring

**Files:**
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `FsLinkRepo::new`, the Plan 2 `SyncCollectionPort` implementation, `SyncService::new(SyncDeps)`, `TransferService::new` with its sync field, `FrameRouter::new`, `NetworkService::set_transfer_sink`, the existing `network_dir`, `network_svc`, `transfer_staging`, `export_svc`, `active_workspace_path`, `audit_publisher`, `tauri_event_bus::TauriEventBus`.
- Produces: managed state `Arc<SyncService>`; the router as the node's transfer sink.

Rules:
- The order is link repo, collections port, `SyncService`, `TransferService`, `FrameRouter`, `set_transfer_sink`. `TransferService::new` registers itself as the sink, so the router must be set after it or every sync frame would be dropped by the transfer machines.
- The link folder is `~/.rocket-api/network/links` and never inside a workspace (spec decision 8).
- `SyncService` shares the transfer staging folder that `purge_transfer_temp` empties at start.

- [ ] **Step 0b: Confirm the constructor shapes**

Run:

```bash
grep -rn "impl SyncCollectionPort for" crates src-tauri/src
grep -rn "pub struct FsLinkRepo" -A6 crates/rocket-infra/src
grep -rn "pub fn new" -B2 -A8 crates/rocket-app/src/network/frame_router.rs
grep -n "pub struct TransferDeps" -A14 crates/rocket-app/src/network/transfer.rs
grep -n "pub struct SyncDeps" -A10 crates/rocket-app/src/network/sync*.rs
```

Expected: exactly one non-test implementor of `SyncCollectionPort` (assumption 5, `FsSyncCollections`); `FsLinkRepo::new(PathBuf)`; `FrameRouter::new(transfer, sync)`; `TransferDeps` has the sync field (assumption 3); `SyncDeps` has the six fields of the index. If the implementor is missing, Plan 2 is not complete: stop. Use the real constructor argument list of the implementor in Step 1; every argument must be a value that already exists in `lib.rs` before the network block (`export_svc`, `active_workspace_path`, `data_dir`, `trust_store` and so on). `SyncDeps.audit` takes the same publisher `TransferDeps.audit` takes (`audit_publisher.clone()`).

- [ ] **Step 1: Build the services at startup**

In `src-tauri/src/lib.rs`, replace (anchored on quoted text)

```rust
            let transfer_svc = rocket_app::TransferService::new(rocket_app::TransferDeps {
                network: Arc::clone(&network_svc) as Arc<dyn rocket_app::PeerSender>,
                export: Arc::clone(&export_svc),
                importer,
                staging: transfer_staging,
                events: Arc::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
                audit: audit_publisher.clone(),
            });
            app.manage(network_svc);
            app.manage(transfer_svc);
```

with

```rust
            // Team network sync: links live under ~/.rocket-api/network/links, never in a
            // workspace. The sync service shares the staging folder purged just above.
            let link_repo = Arc::new(rocket_infra::FsLinkRepo::new(network_dir.join("links")));
            let sync_collections = Arc::new(rocket_app::FsSyncCollections::new(
                Arc::clone(&export_svc),
                Arc::clone(&active_workspace_path),
            ));
            let sync_svc = rocket_app::SyncService::new(rocket_app::SyncDeps {
                network: Arc::clone(&network_svc) as Arc<dyn rocket_app::PeerSender>,
                links: link_repo,
                collections: sync_collections,
                events: Arc::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
                audit: audit_publisher.clone(),
                staging: transfer_staging.clone(),
            });
            let transfer_svc = rocket_app::TransferService::new(rocket_app::TransferDeps {
                network: Arc::clone(&network_svc) as Arc<dyn rocket_app::PeerSender>,
                export: Arc::clone(&export_svc),
                importer,
                staging: transfer_staging,
                events: Arc::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
                audit: audit_publisher.clone(),
                sync: Arc::clone(&sync_svc),
            });
            // `TransferService::new` made itself the sink. The router replaces it, so it comes
            // last: sync frames go to the sync service, all others to the transfer service.
            let frame_router = rocket_app::FrameRouter::new(
                Arc::clone(&transfer_svc),
                Arc::clone(&sync_svc),
            );
            network_svc.set_transfer_sink(frame_router);
            app.manage(network_svc);
            app.manage(transfer_svc);
            app.manage(sync_svc);
```

`FsLinkRepo::new` creates its folder on first save (Plan 2). `transfer_staging` is an `Arc<FsTransferStaging>`; the struct field expects `Arc<dyn TransferStaging>` and the coercion is implicit, so the `.clone()` for `SyncDeps` and the move into `TransferDeps` both compile.

- [ ] **Step 2: Check the build and the order**

Run: `cargo check -j4 -p rocket`

Expected: finished with no errors. A "use of moved value" on `network_svc` or `transfer_staging` means a clone was missed.

Run:

```bash
grep -n "SyncService::new(\|TransferService::new(\|FrameRouter::new(\|set_transfer_sink(" src-tauri/src/lib.rs
```

Expected: exactly four lines, in this order: `SyncService::new(`, `TransferService::new(`, `FrameRouter::new(`, `set_transfer_sink(`.

Run:

```bash
grep -n "purge_transfer_temp" src-tauri/src/lib.rs
```

Expected: exactly one line, and its line number is smaller than the one of `SyncService::new(` (the staging folder is purged before any service can use it).

- [ ] **Step 3: Run the Tauri tests that touch the wiring**

Run: `cargo test -j4 -p rocket ipc_acl`

Expected: all PASS.

Run: `cargo test -j4 -p rocket import_wiring`

Expected: all PASS (the startup purge test of the send series is unchanged).

- [ ] **Step 4: Note on what is not unit-tested here**

The `setup` closure needs a running app. The order is proven by the grep above and by the e2e harness in Task 3 (`attach_sync` repeats the same construction order against real services). Record in the ledger: `Ruling: no shutdown hook for SyncService; sessions end through on_peer_gone when the node handle stops, staged files are purged at the next start.`

---

### Task 3: End to end over the real transport

**Files:**
- Modify: `crates/rocket-app/src/network/mod.rs`, `crates/rocket-app/src/network/test_support.rs`
- Create: `crates/rocket-app/src/network/e2e_sync_tests.rs`

**Interfaces:**
- Consumes: from the send series `online_pair`, `wait_until`, `E2eNode`, `file`, `normal`, `zip_entries`, `count_files`; from Plans 1 to 3 the names listed under Prerequisites.
- Produces (test-only): `RecordingEvents::syncs`, and in `e2e_sync_tests.rs` the harness `Peer`, `Rig`, `seed_pair`, `run_sync`, the fakes `MemCollections`, `SyncImport`, `WireTap` and the 20 tests listed in Step 8. No production or earlier-plan code changes.

Rules the tests pin (spec sections 3, 6, 8 and 12):
- Both sides store equal bases at the first send and keep them equal after every sync that applies (`base_hash` equal, the stored base equal to the manifest of the local export, state `in_sync`).
- Each outcome of the planner table that matters at the user level: take theirs, send mine, take a delete, three conflict choices, delete against edit, rename in both directions.
- The receiving copy keeps its own token, `uid`, `extensions.rocketapi` and examples; a secret never reaches the other copy.
- Every failure ends both sides without applying and with no file or base changed.

Decisions recorded here:
- **Tampering uses a `PeerSender` decorator, not a hook.** `WireTap` wraps the real `NetworkService` in `SyncDeps.network`. It remembers the `kind` of each `SyncOffer` it sends and, when asked, flips the first base64 character of chunk 0 of the payloads of one kind. The receiver then sees the bytes a hostile hop would deliver, with the hash computed before the flip. Record as `Ruling:`.
- **Convergence is checked on the manifests.** Raw files differ between the two copies by design (local secrets and keys), so the tests compare `Manifest::base_hash()` of each side's export and the stored base, never raw bytes.
- **The planner is the node that receives the sync.** In the harness A is the starter and B the planner, except in the rename test where the roles swap on purpose.

- [ ] **Step 0: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 0b: Confirm names and the module layout**

Run:

```bash
grep -n "pub(super) fn \(online_pair\|wait_until\|file\|normal\|zip_entries\|count_files\)\|pub(super) async fn online_pair" crates/rocket-app/src/network/e2e_tests.rs crates/rocket-app/src/network/e2e_transfer_tests.rs
grep -n "pub fn manifest_from_zip\|pub trait SyncCollectionPort\|pub enum FileChange\|pub struct SyncFile" -r crates/rocket-app/src
grep -n "pub fn create_link\|pub async fn start\|pub async fn unlink" crates/rocket-app/src/network/sync*.rs
grep -rn "pub struct ImportedCollection" -A6 crates/rocket-app/src/network/transfer_ports.rs
grep -rn "FsLinkRepo" crates/rocket-infra/src/lib.rs
grep -n "RecordingEvents" -B2 -A2 crates/rocket-app/src/network/test_support.rs | head -30
grep -n "SyncEvent" crates/rocket-app/src/network/test_support.rs
sed -n 1,40p crates/rocket-app/src/network/mod.rs
```

Expected: the helpers are `pub(super)`; the port types exist; `ImportedCollection` is `{ name }` (assumption 7; if Plan 3 added fields, fill them in `SyncImport::import_zip` below with `root = format!("/mem/{display_name}")` and `uid = None`); `FsLinkRepo` is reachable as `rocket_infra::FsLinkRepo` (otherwise use the path the grep shows in `use` below and in Task 2); `RecordingEvents` is a tuple struct; and `test_support.rs` has no `SyncEvent` yet (if Plan 3 already records it, skip Step 3 and use its accessor name instead of `syncs()`). Note the module path of `SyncService`, `SyncDeps`, `FrameRouter`, `SyncCollectionPort`, `FileChange`, `SyncFile` and `manifest_from_zip`: the test file imports them through `super::{...}` and `super::sync_ports::{...}`; adapt the two `use` blocks if they live elsewhere.

- [ ] **Step 1: Write the harness and the first failing tests**

Create `crates/rocket-app/src/network/e2e_sync_tests.rs`:

```rust
//! End-to-end sync tests: two real nodes over real TLS on 127.0.0.1, each with a real
//! `TransferService`, `SyncService`, `FrameRouter`, `FsLinkRepo` and `FsTransferStaging`.
//!
//! Test-only fakes: `MemCollections` (the collection port, over the real export and the real
//! file merge), `SyncImport` (the import of the first send) and `WireTap` (a sender that can
//! damage one payload kind). The wiring order mirrors `src-tauri/src/lib.rs`.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use rocket_audit::AuditEventKind;
use rocket_export::{CollectionFileSource, SourceFile};
use rocket_infra::{FsLinkRepo, FsTransferStaging};
use rocket_network::{
    ConflictKind, ConflictView, Decision, DeviceId, Direction, LinkId, LinkRecord, LinkRepository,
    LinkState, Manifest, Outcome, PeerMessage, Resolution, SyncEvent, SyncFinish, SyncId,
    SyncPayloadKind, SyncRole, SyncSummary, TransferEvent, TransferSink,
};
use rocket_shared::error::{DomainError, DomainResult};
use tempfile::TempDir;

use super::e2e_tests::{online_pair, wait_until, E2eNode};
use super::e2e_transfer_tests::{count_files, file, normal, zip_entries};
use super::sync_ports::{manifest_from_zip, FileChange, SyncCollectionPort, SyncFile};
use super::{
    CollectionImportPort, FrameRouter, ImportedCollection, NetworkHandle, NetworkService,
    PeerSender, SyncDeps, SyncService, TransferDeps, TransferService, TransferStaging,
    UNSAFE_ZIP_MESSAGE,
};
use crate::collection_export_service::CollectionExportService;

// ----- fixtures -----

const O: &str = "Orders";
const GET: &str = "users/get.yml";
const LIST: &str = "orders/list.yml";

const URL0: &str = "https://api.example.test/users";
const URL_A: &str = "https://api.example.test/users-from-ann";
const URL_B: &str = "https://api.example.test/users-from-ben";
const LIST0: &str = "https://api.example.test/orders";
const LIST_B: &str = "https://api.example.test/orders-from-ben";

const A_TOKEN: &str = "A-TOKEN-91c4d2";
const B_TOKEN: &str = "B-TOKEN-5e21aa";
const A_UID: &str = "a-uid-12e0";
const B_UID: &str = "b-uid-77c3";
const B_EXAMPLE: &str = "b-example-body-3f9a";

/// Keys only A keeps: a uid.
const A_LOCAL: &str = "uid: a-uid-12e0\n";
/// Keys only B keeps: a uid, a Rocket-only extension and a saved example. The export removes
/// all three, so they never change a hash.
const B_LOCAL: &str = "uid: b-uid-77c3\nextensions:\n  rocketapi:\n    sandboxMode: developer\nexamples:\n  - name: ok\n    response:\n      body:\n        data: b-example-body-3f9a\n";

fn get_yaml(url: &str, token: &str, local: &str) -> String {
    format!(
        "info:\n  name: get\nhttp:\n  method: GET\n  url: {url}\n  headers:\n    - name: Authorization\n      value: Bearer {token}\n{local}"
    )
}

fn list_yaml(url: &str) -> String {
    format!("info:\n  name: list\nhttp:\n  method: GET\n  url: {url}\n")
}

/// The same four files on both sides. Only the secrets and the local-only keys differ.
fn collection(token: &str, local: &str) -> Vec<SourceFile> {
    vec![
        file(
            "opencollection.yml",
            "opencollection: \"1.0.0\"\ninfo:\n  name: \"Orders\"\n",
        ),
        file("users/folder.yml", "info:\n  name: users\n"),
        file(GET, &get_yaml(URL0, token, local)),
        file(LIST, &list_yaml(LIST0)),
    ]
}

// ----- fakes -----

/// Collections held in memory: name to (relative path to bytes). The bytes are the node's own
/// files, local secrets and keys included.
#[derive(Default)]
struct MemCollections {
    files: Mutex<BTreeMap<String, BTreeMap<String, Vec<u8>>>>,
}

/// A fixed file list that ignores the collection name, for one export run.
struct Fixed(Vec<SourceFile>);

impl CollectionFileSource for Fixed {
    fn list_files(&self, _collection: &str) -> DomainResult<Vec<SourceFile>> {
        Ok(self.0.clone())
    }
}

/// Reads the live files of a `MemCollections`, for the `TransferService` export.
struct ColsSource(Arc<MemCollections>);

impl CollectionFileSource for ColsSource {
    fn list_files(&self, collection: &str) -> DomainResult<Vec<SourceFile>> {
        self.0.source_files(collection)
    }
}

impl MemCollections {
    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, BTreeMap<String, Vec<u8>>>> {
        self.files.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn seed(&self, name: &str, files: Vec<SourceFile>) {
        let map = files.into_iter().map(|f| (f.rel_path, f.bytes)).collect();
        self.lock().insert(name.to_string(), map);
    }

    fn names(&self) -> Vec<String> {
        self.lock().keys().cloned().collect()
    }

    fn source_files(&self, name: &str) -> DomainResult<Vec<SourceFile>> {
        let all = self.lock();
        let map = all
            .get(name)
            .ok_or_else(|| DomainError::NotFound(name.to_string()))?;
        Ok(map
            .iter()
            .map(|(path, bytes)| SourceFile {
                rel_path: path.clone(),
                bytes: bytes.clone(),
            })
            .collect())
    }

    fn text(&self, name: &str, path: &str) -> Option<String> {
        self.lock()
            .get(name)
            .and_then(|m| m.get(path))
            .map(|b| String::from_utf8_lossy(b).to_string())
    }

    fn paths(&self, name: &str) -> Vec<String> {
        self.lock()
            .get(name)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    fn write(&self, name: &str, path: &str, text: &str) {
        self.lock()
            .entry(name.to_string())
            .or_default()
            .insert(path.to_string(), text.as_bytes().to_vec());
    }

    fn remove(&self, name: &str, path: &str) {
        if let Some(map) = self.lock().get_mut(name) {
            map.remove(path);
        }
    }

    fn rename(&self, name: &str, from: &str, to: &str) {
        if let Some(map) = self.lock().get_mut(name) {
            if let Some(bytes) = map.remove(from) {
                map.insert(to.to_string(), bytes);
            }
        }
    }

    /// The scrubbed export of the collection as ZIP bytes (the real export).
    fn export_zip(&self, name: &str) -> DomainResult<Vec<u8>> {
        let files = self.source_files(name)?;
        let service = CollectionExportService::new(Box::new(Fixed(files)));
        Ok(service.export_bruno(name, false)?.bytes)
    }
}

impl SyncCollectionPort for MemCollections {
    fn exists(&self, collection: &str) -> DomainResult<bool> {
        Ok(self.lock().contains_key(collection))
    }

    fn collection_uid(&self, _collection: &str) -> DomainResult<Option<String>> {
        Ok(None)
    }

    fn manifest(&self, collection: &str) -> DomainResult<Manifest> {
        let zip = self.export_zip(collection)?;
        let dir = TempDir::new().map_err(|e| DomainError::Internal(e.to_string()))?;
        let path = dir.path().join("export.zip");
        std::fs::write(&path, &zip).map_err(|e| DomainError::Internal(e.to_string()))?;
        manifest_from_zip(&path)
    }

    fn export_files(&self, collection: &str, paths: &[String]) -> DomainResult<Vec<SyncFile>> {
        let zip = self.export_zip(collection)?;
        let mut out = Vec::new();
        for (entry, bytes) in zip_entries(&zip) {
            // The single root folder is stripped, as the manifest does.
            let Some((_, rest)) = entry.split_once('/') else {
                continue;
            };
            if paths.iter().any(|p| p == rest) {
                out.push(SyncFile {
                    path: rest.to_string(),
                    bytes,
                });
            }
        }
        if out.len() != paths.len() {
            return Err(DomainError::NotFound("a wanted file".to_string()));
        }
        Ok(out)
    }

    fn apply(&self, collection: &str, changes: &[FileChange]) -> DomainResult<()> {
        let mut all = self.lock();
        let mut working = all
            .get(collection)
            .cloned()
            .ok_or_else(|| DomainError::NotFound(collection.to_string()))?;
        for change in changes {
            match change {
                FileChange::Put { path, bytes } => {
                    let local = working.get(path).map(Vec::as_slice);
                    let merged = rocket_export::sync_merge::merge_file(local, bytes)
                        .map_err(|e| DomainError::InvalidInput(e.to_string()))?;
                    working.insert(path.clone(), merged);
                }
                FileChange::PutAs { path, bytes } => {
                    working.insert(path.clone(), bytes.clone());
                }
                FileChange::Delete { path } => {
                    working.remove(path);
                }
                FileChange::Rename { from, to } => {
                    let bytes = working
                        .remove(from)
                        .ok_or_else(|| DomainError::NotFound(from.clone()))?;
                    working.insert(to.clone(), bytes);
                }
            }
        }
        // All or nothing: the working copy replaces the collection only when every change ran.
        all.insert(collection.to_string(), working);
        Ok(())
    }
}

/// The import of the first send. It runs the real ZIP validator, then seeds the collection.
struct SyncImport {
    cols: Arc<MemCollections>,
}

impl CollectionImportPort for SyncImport {
    fn existing_names(&self) -> DomainResult<Vec<String>> {
        Ok(self.cols.names())
    }

    fn import_zip(&self, zip_path: &Path, display_name: &str) -> DomainResult<ImportedCollection> {
        rocket_import::transfer_zip::validate(&rocket_infra::FsImportFiles, zip_path)
            .map_err(|_| DomainError::InvalidInput(UNSAFE_ZIP_MESSAGE.to_string()))?;
        let bytes = std::fs::read(zip_path).map_err(|e| DomainError::Internal(e.to_string()))?;
        let files = zip_entries(&bytes)
            .into_iter()
            .filter_map(|(entry, data)| {
                entry.split_once('/').map(|(_, rest)| SourceFile {
                    rel_path: rest.to_string(),
                    bytes: data,
                })
            })
            .collect();
        self.cols.seed(display_name, files);
        Ok(ImportedCollection {
            name: display_name.to_string(),
        })
    }
}

/// A sender that can damage the first chunk of one payload kind. It sits between the
/// `SyncService` and the real `NetworkService`, so the receiver sees what a hostile hop would
/// deliver.
struct WireTap {
    inner: Arc<NetworkService>,
    kinds: Mutex<HashMap<String, SyncPayloadKind>>,
    tamper: Mutex<Option<SyncPayloadKind>>,
}

impl WireTap {
    fn new(inner: Arc<NetworkService>) -> Arc<WireTap> {
        Arc::new(WireTap {
            inner,
            kinds: Mutex::new(HashMap::new()),
            tamper: Mutex::new(None),
        })
    }

    /// Damages chunk 0 of every payload of this kind from now on.
    fn tamper(&self, kind: Option<SyncPayloadKind>) {
        *self.tamper.lock().unwrap_or_else(|e| e.into_inner()) = kind;
    }

    fn maybe_tamper(&self, message: PeerMessage) -> PeerMessage {
        if let PeerMessage::SyncOffer {
            transfer_id, kind, ..
        } = &message
        {
            self.kinds
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(transfer_id.clone(), *kind);
            return message;
        }
        if let PeerMessage::Chunk {
            transfer_id,
            seq,
            data,
        } = &message
        {
            let target = *self.tamper.lock().unwrap_or_else(|e| e.into_inner());
            let kind = self
                .kinds
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(transfer_id)
                .copied();
            if *seq == 0 && target.is_some() && kind == target {
                return PeerMessage::Chunk {
                    transfer_id: transfer_id.clone(),
                    seq: *seq,
                    data: flip_first_char(data),
                };
            }
        }
        message
    }
}

/// Changes the first base64 character, so the first decoded byte changes.
fn flip_first_char(data: &str) -> String {
    let mut chars: Vec<char> = data.chars().collect();
    if let Some(first) = chars.first_mut() {
        *first = if *first == 'A' { 'B' } else { 'A' };
    }
    chars.into_iter().collect()
}

#[async_trait]
impl PeerSender for WireTap {
    async fn send_to(&self, device: &DeviceId, message: PeerMessage) -> DomainResult<()> {
        let message = self.maybe_tamper(message);
        PeerSender::send_to(self.inner.as_ref(), device, message).await
    }

    fn peer_label(&self, device: &DeviceId) -> Option<(String, String)> {
        PeerSender::peer_label(self.inner.as_ref(), device)
    }

    fn is_online(&self, device: &DeviceId) -> bool {
        PeerSender::is_online(self.inner.as_ref(), device)
    }

    fn set_transfer_sink(&self, sink: Arc<dyn TransferSink>) {
        PeerSender::set_transfer_sink(self.inner.as_ref(), sink);
    }
}

// ----- the harness -----

/// A node with the whole sync stack on top of its real network service.
struct Peer {
    node: E2eNode,
    cols: Arc<MemCollections>,
    sync: Arc<SyncService>,
    transfer: Arc<TransferService>,
    links: Arc<FsLinkRepo>,
    wire: Arc<WireTap>,
    link_dir: TempDir,
    staging_dir: TempDir,
}

/// Builds the services in the order `lib.rs` uses: sync service, transfer service, router,
/// then the router becomes the sink.
fn attach_sync(node: E2eNode, collections: Vec<(&str, Vec<SourceFile>)>) -> Peer {
    let staging_dir = TempDir::new().expect("tempdir");
    let link_dir = TempDir::new().expect("tempdir");
    let staging = Arc::new(FsTransferStaging::new(staging_dir.path().to_path_buf()));
    staging.purge().expect("purge at start");
    let cols = Arc::new(MemCollections::default());
    for (name, files) in collections {
        cols.seed(name, files);
    }
    let links = Arc::new(FsLinkRepo::new(link_dir.path().to_path_buf()));
    let wire = WireTap::new(Arc::clone(&node.svc));
    let sync = SyncService::new(SyncDeps {
        network: wire.clone(),
        links: links.clone(),
        collections: cols.clone(),
        events: node.events.clone(),
        audit: node.audit.clone(),
        staging: staging.clone(),
    });
    let export = Arc::new(CollectionExportService::new(Box::new(ColsSource(
        Arc::clone(&cols),
    ))));
    let transfer = TransferService::new_with_timing(
        TransferDeps {
            network: node.svc.clone(),
            export,
            importer: Arc::new(SyncImport { cols: cols.clone() }),
            staging,
            events: node.events.clone(),
            audit: node.audit.clone(),
            sync: Arc::clone(&sync),
        },
        normal(),
    );
    let router = FrameRouter::new(Arc::clone(&transfer), Arc::clone(&sync));
    node.svc.set_transfer_sink(router);
    Peer {
        node,
        cols,
        sync,
        transfer,
        links,
        wire,
        link_dir,
        staging_dir,
    }
}

impl Peer {
    fn id(&self) -> DeviceId {
        self.node.id()
    }

    fn syncs(&self) -> Vec<SyncEvent> {
        self.node.events.syncs()
    }

    /// The review of this sync, if this node showed it.
    fn review_ready(&self, sid: &str) -> Option<(SyncSummary, Vec<ConflictView>, SyncRole)> {
        self.syncs().into_iter().find_map(|e| match e {
            SyncEvent::ReviewReady {
                sync_id,
                summary,
                conflicts,
                role,
                ..
            } if sync_id == sid => Some((summary, conflicts, role)),
            _ => None,
        })
    }

    /// The confirm summary of this sync, if this node showed it.
    fn confirm_summary(&self, sid: &str) -> Option<SyncSummary> {
        self.syncs().into_iter().find_map(|e| match e {
            SyncEvent::ConfirmNeeded {
                sync_id, summary, ..
            } if sync_id == sid => Some(summary),
            _ => None,
        })
    }

    /// How this sync ended on this node, if it ended.
    fn finish(&self, sid: &str) -> Option<(SyncFinish, Option<String>)> {
        self.syncs().into_iter().find_map(|e| match e {
            SyncEvent::Finished {
                sync_id,
                outcome,
                message,
                ..
            } if sync_id == sid => Some((outcome, message)),
            _ => None,
        })
    }

    fn manifest(&self, name: &str) -> Manifest {
        self.cols.manifest(name).expect("a manifest")
    }

    fn link(&self, id: &LinkId) -> Option<LinkRecord> {
        self.links.get(id).expect("the link store reads")
    }

    fn text(&self, path: &str) -> String {
        self.cols
            .text(O, path)
            .unwrap_or_else(|| panic!("{path} is missing"))
    }

    fn audited_sync(&self, sent: bool) -> bool {
        self.node.audited().iter().any(|k| match k {
            AuditEventKind::NetworkSyncSent { collection, .. } if sent => collection == O,
            AuditEventKind::NetworkSyncReceived { collection, .. } if !sent => collection == O,
            _ => false,
        })
    }
}

/// A started pair with one linked collection. The handles keep the nodes running.
struct Rig {
    a: Peer,
    b: Peer,
    link: LinkId,
    _ha: NetworkHandle,
    _hb: NetworkHandle,
}

/// A pair with equal exports and a link on both sides, made with `create_link` from the same
/// exported bytes.
async fn seed_pair() -> Rig {
    let (a, b, ha, hb) = online_pair().await;
    let a = attach_sync(a, vec![(O, collection(A_TOKEN, A_LOCAL))]);
    let b = attach_sync(b, vec![(O, collection(B_TOKEN, B_LOCAL))]);
    let manifest = a.manifest(O);
    assert_eq!(
        manifest.base_hash(),
        b.manifest(O).base_hash(),
        "the fixtures export to the same files; a difference here means the export keeps a secret or a local key"
    );
    let link = LinkId::random();
    for (me, other) in [(&a, &b), (&b, &a)] {
        let paths: Vec<String> = manifest.paths().map(str::to_string).collect();
        let base_files = me
            .cols
            .export_files(O, &paths)
            .expect("base files")
            .into_iter()
            .map(|f| (f.path, f.bytes))
            .collect();
        me.sync
            .create_link(
                LinkRecord {
                    link_id: link.clone(),
                    peer_device_id: other.id(),
                    collection_name: O.to_string(),
                    collection_root: format!("/mem/{O}"),
                    collection_uid: None,
                    base: manifest.clone(),
                    state: LinkState::InSync,
                    updated_at: chrono::Utc::now().to_rfc3339(),
                },
                base_files,
            )
            .expect("the link is created");
    }
    Rig {
        a,
        b,
        link,
        _ha: ha,
        _hb: hb,
    }
}

/// What the planner showed before it accepted.
struct Ran {
    sync_id: SyncId,
    sid: String,
    summary: SyncSummary,
    conflicts: Vec<ConflictView>,
}

/// Starts a sync and waits for the planner's review. Does not answer it.
async fn start_and_wait_review(starter: &Peer, planner: &Peer, link: &LinkId) -> Ran {
    let sync_id = starter.sync.start(link).await.expect("the sync starts");
    let sid = sync_id.as_str().to_string();
    wait_until("the planner shows the review", || {
        planner.review_ready(&sid).is_some()
    })
    .await;
    let (summary, conflicts, role) = planner.review_ready(&sid).expect("the review event");
    assert_eq!(role, SyncRole::Planner);
    Ran {
        sync_id,
        sid,
        summary,
        conflicts,
    }
}

/// The planner accepts with `decisions`, the starter confirms if it is asked, and the call
/// returns once both sides show an end.
async fn accept_and_finish(starter: &Peer, planner: &Peer, ran: &Ran, decisions: Vec<Decision>) {
    planner
        .sync
        .review(&ran.sync_id, true, decisions)
        .await
        .expect("the planner accepts");
    wait_until("the starter is asked or has ended", || {
        starter.confirm_summary(&ran.sid).is_some() || starter.finish(&ran.sid).is_some()
    })
    .await;
    if starter.finish(&ran.sid).is_none() {
        starter
            .sync
            .confirm(&ran.sync_id, true)
            .await
            .expect("the starter confirms");
    }
    wait_until("both sides finished", || {
        starter.finish(&ran.sid).is_some() && planner.finish(&ran.sid).is_some()
    })
    .await;
}

fn no_decisions(_: &[ConflictView]) -> Vec<Decision> {
    Vec::new()
}

/// One decision per conflict, all with the same resolution.
fn choose(resolution: Resolution) -> impl Fn(&[ConflictView]) -> Vec<Decision> {
    move |conflicts| {
        conflicts
            .iter()
            .map(|c| Decision {
                path: c.path.clone(),
                resolution,
            })
            .collect()
    }
}

/// A full sync: start, review, confirm, wait for both ends.
async fn run_sync(
    starter: &Peer,
    planner: &Peer,
    link: &LinkId,
    decide: impl Fn(&[ConflictView]) -> Vec<Decision>,
) -> Ran {
    let ran = start_and_wait_review(starter, planner, link).await;
    let decisions = decide(&ran.conflicts);
    accept_and_finish(starter, planner, &ran, decisions).await;
    ran
}

/// Both copies export the same files and both stored bases are that manifest, in sync.
fn assert_converged(rig: &Rig) {
    let hash_a = rig.a.manifest(O).base_hash();
    let hash_b = rig.b.manifest(O).base_hash();
    assert_eq!(hash_a, hash_b, "both copies export the same files");
    for peer in [&rig.a, &rig.b] {
        let record = peer.link(&rig.link).expect("the link is kept");
        assert_eq!(record.base_hash(), hash_a, "the stored base is the agreed manifest");
        assert_eq!(record.state, LinkState::InSync);
    }
}

/// The stored base hash of a side, to prove it did not move.
fn base_hash_of(peer: &Peer, link: &LinkId) -> String {
    peer.link(link).expect("the link is kept").base_hash()
}

fn applied(peer: &Peer, sid: &str) -> bool {
    matches!(peer.finish(sid), Some((SyncFinish::Applied, _)))
}

/// Waits for the first incoming offer on `peer` and returns its transfer id.
async fn wait_offer_id(peer: &Peer) -> String {
    wait_until("the offer shows", || {
        peer.node
            .events
            .transfers()
            .iter()
            .any(|e| matches!(e, TransferEvent::OfferIncoming { .. }))
    })
    .await;
    peer.node
        .events
        .transfers()
        .into_iter()
        .find_map(|e| match e {
            TransferEvent::OfferIncoming { transfer_id, .. } => Some(transfer_id),
            _ => None,
        })
        .expect("an offer")
}

fn link_count(peer: &Peer) -> usize {
    peer.links.list().map(|l| l.len()).unwrap_or(0)
}

// ----- the first send creates the link -----

#[tokio::test]
async fn the_first_send_with_keep_in_sync_creates_equal_links_on_both_sides() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach_sync(a, vec![(O, collection(A_TOKEN, A_LOCAL))]);
    let b = attach_sync(b, vec![]);

    let id = a
        .transfer
        .send(&b.id(), O, false, true)
        .await
        .expect("the offer is sent")
        .transfer_id;
    assert_eq!(wait_offer_id(&b).await, id);
    b.transfer.respond(&id, true).await.expect("accept");
    wait_until("the send completed on both sides", || {
        a.node.events.finished_outcomes().contains(&(Direction::Send, Outcome::Completed))
            && b.node
                .events
                .finished_outcomes()
                .contains(&(Direction::Receive, Outcome::Completed))
    })
    .await;
    wait_until("both links exist", || link_count(&a) == 1 && link_count(&b) == 1).await;

    let on_a = a.links.list().expect("list").remove(0);
    let on_b = b.links.list().expect("list").remove(0);
    assert_eq!(on_a.link_id.as_str(), on_b.link_id.as_str(), "one link id");
    assert_eq!(on_a.peer_device_id, b.id());
    assert_eq!(on_b.peer_device_id, a.id());
    assert_eq!(on_a.collection_name, O);
    assert_eq!(on_b.collection_name, O);
    assert_eq!(on_a.state, LinkState::InSync);
    assert_eq!(on_b.state, LinkState::InSync);
    assert_eq!(
        on_a.base_hash(),
        on_b.base_hash(),
        "both bases come from the same sent bytes"
    );
    assert_eq!(on_a.base_hash(), a.manifest(O).base_hash());
    assert_eq!(on_b.base_hash(), b.manifest(O).base_hash());

    // The link works: an edit on A reaches B through the first sync.
    a.cols.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));
    let rig = Rig {
        a,
        b,
        link: on_a.link_id.clone(),
        _ha,
        _hb,
    };
    let ran = run_sync(&rig.a, &rig.b, &rig.link, no_decisions).await;
    assert!(applied(&rig.b, &ran.sid));
    assert!(rig.b.text(GET).contains(URL_A));
    assert_converged(&rig);
}

#[tokio::test]
async fn a_send_without_keep_in_sync_creates_no_link() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach_sync(a, vec![(O, collection(A_TOKEN, A_LOCAL))]);
    let b = attach_sync(b, vec![]);

    let id = a
        .transfer
        .send(&b.id(), O, false, false)
        .await
        .expect("the offer is sent")
        .transfer_id;
    wait_offer_id(&b).await;
    b.transfer.respond(&id, true).await.expect("accept");
    wait_until("the send completed on both sides", || {
        a.node.events.finished_outcomes().contains(&(Direction::Send, Outcome::Completed))
            && b.node
                .events
                .finished_outcomes()
                .contains(&(Direction::Receive, Outcome::Completed))
    })
    .await;
    assert_eq!(link_count(&a), 0, "no link on the sender");
    assert_eq!(link_count(&b), 0, "no link on the receiver");
    assert!(b.cols.names().contains(&O.to_string()), "the collection did arrive");
}

#[tokio::test]
async fn the_seeded_copies_start_from_one_base() {
    let rig = seed_pair().await;
    assert_converged(&rig);
    assert_ne!(rig.a.text(GET), rig.b.text(GET), "the raw files differ by design");
    assert!(rig.b.text(GET).contains(B_TOKEN) && !rig.b.text(GET).contains(A_TOKEN));
}
```

The tests at the end of this step need three things Step 3 adds: `RecordingEvents::syncs`, the module registration and the `FsLinkRepo` import path from Step 0b. Tests for the sync scenarios follow in Steps 4 to 7.

- [ ] **Step 2: Register the module and see the tests fail**

In `crates/rocket-app/src/network/mod.rs`, add next to the other test modules:

```rust
#[cfg(test)]
#[path = "e2e_sync_tests.rs"]
mod e2e_sync_tests;
```

Run: `cargo test -j4 -p rocket-app e2e_sync`

Expected: FAIL to compile. The errors name `RecordingEvents::syncs` as missing (Step 3 adds it). Errors naming `SyncService`, `FrameRouter`, `SyncDeps`, `FileChange`, `manifest_from_zip` or the `sync` field of `TransferDeps` mean Plans 1 to 3 are not applied or their names differ from the index: stop and fix that first (assumptions 2 and 3 above, then the `use` blocks of this file).

- [ ] **Step 3: Record sync events in the test support**

Skip this step if Step 0b found that `test_support.rs` already records `SyncEvent`.

In `crates/rocket-app/src/network/test_support.rs`, add `SyncEvent` to the existing `rocket_network` import, and change the recorder. Replace

```rust
pub(crate) struct RecordingEvents(Mutex<Vec<NetworkEvent>>, Mutex<Vec<TransferEvent>>);
```

with

```rust
pub(crate) struct RecordingEvents(
    Mutex<Vec<NetworkEvent>>,
    Mutex<Vec<TransferEvent>>,
    Mutex<Vec<SyncEvent>>,
);
```

add to `impl RecordingEvents`:

```rust
    /// Every sync event, in order.
    pub fn syncs(&self) -> Vec<SyncEvent> {
        self.2.lock().expect("events lock").clone()
    }
```

and extend the `publish` body of `impl EventPublisher for RecordingEvents` with:

```rust
        if let Some(event) = event.downcast_ref::<SyncEvent>() {
            self.2.lock().expect("events lock").push(event.clone());
        }
```

Run: `cargo check -j4 -p rocket-app --tests`

Expected: finished with no errors. A `PartialEq`, `Copy` or `Clone` error on a sync type names a derive Plan 1 left out (Task 1 Step 0b); add it in `rocket-network` and record a ruling.

- [ ] **Step 4: Run the first three tests**

Run: `cargo test -j4 -p rocket-app e2e_sync`

Expected: 3 passed (`the_first_send_with_keep_in_sync_creates_equal_links_on_both_sides`, `a_send_without_keep_in_sync_creates_no_link`, `the_seeded_copies_start_from_one_base`). If `the_seeded_copies_start_from_one_base` fails on the manifest hash, the export keeps a value that differs between the fixtures (for example the `Authorization` value): read what `rocket-export` scrubs and fix the fixture, never the assertion. If a test in this group fails inside Plan 3 code, fix it there with superpowers:systematic-debugging, record a `Ruling:` line and never weaken the test.

- [ ] **Step 5: Write the clean sync, the local keys and the rename tests**

Append to `e2e_sync_tests.rs`:

```rust
// ----- a clean two-way sync -----

#[tokio::test]
async fn a_clean_two_way_sync_applies_both_edits_and_updates_both_bases() {
    let rig = seed_pair().await;
    let (a, b) = (&rig.a, &rig.b);
    // A edits request 1, B edits request 2.
    a.cols.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));
    b.cols.write(O, LIST, &list_yaml(LIST_B));
    assert_ne!(a.manifest(O).base_hash(), b.manifest(O).base_hash());

    let ran = run_sync(a, b, &rig.link, no_decisions).await;

    assert!(ran.conflicts.is_empty(), "nothing to decide");
    assert_eq!(ran.summary.changed_here, 1, "B takes A's edit");
    assert_eq!(ran.summary.sent_changed, 1, "B sends its own edit");
    assert_eq!(ran.summary.added_here + ran.summary.deleted_here, 0);
    assert_eq!(ran.summary.sent_added + ran.summary.sent_deleted, 0);
    assert!(ran.summary.scripts_changed.is_empty() && ran.summary.auth_changed.is_empty());
    assert!(applied(a, &ran.sid) && applied(b, &ran.sid));

    assert!(b.text(GET).contains(URL_A), "B has A's edit");
    assert!(a.text(LIST).contains(LIST_B), "A has B's edit");
    assert_converged(&rig);

    // Audit: the starter sent, the planner received, with the short peer id.
    assert!(a.audited_sync(true) && b.audited_sync(false));
    assert!(a.node.audited().iter().any(|k| matches!(
        k,
        AuditEventKind::NetworkSyncSent { device_id, outcome, .. }
            if *device_id == b.id().short() && outcome == "applied"
    )));
}

#[tokio::test]
async fn b_keeps_its_token_uid_extensions_and_examples_after_a_edit_syncs() {
    let rig = seed_pair().await;
    let (a, b) = (&rig.a, &rig.b);
    let a_before = a.text(GET);
    a.cols.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));
    let a_edited = a.text(GET);

    let ran = run_sync(a, b, &rig.link, no_decisions).await;
    assert!(applied(b, &ran.sid));

    let b_now = b.text(GET);
    assert!(b_now.contains(URL_A), "B has the new url");
    assert!(!b_now.contains(URL0), "the old url is gone");
    assert!(b_now.contains(B_TOKEN), "B still has its own token");
    assert!(b_now.contains(B_UID), "B still has its own uid");
    assert!(b_now.contains("sandboxMode: developer"), "B's rocketapi settings stay");
    assert!(b_now.contains(B_EXAMPLE), "B's saved example stays");
    assert!(!b_now.contains(A_TOKEN), "A's token never crossed");
    assert!(!b_now.contains(A_UID), "A's uid never crossed");

    // A took nothing, and nothing of B's local data reached A.
    assert_eq!(a.text(GET), a_edited, "A's file is untouched by the sync");
    assert_ne!(a.text(GET), a_before);
    assert!(!a.text(GET).contains(B_TOKEN));
    assert!(!a.text(GET).contains(B_UID));
    assert!(!a.text(GET).contains(B_EXAMPLE));
    assert_converged(&rig);
}

// ----- conflicts -----

/// A and B both change request 1 and the planner (B) answers with `resolution`.
async fn conflict_run(resolution: Resolution) -> (Rig, Ran) {
    let rig = seed_pair().await;
    rig.a.cols.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));
    rig.b.cols.write(O, GET, &get_yaml(URL_B, B_TOKEN, B_LOCAL));
    let hash_a = rig.a.manifest(O).get(GET).map(str::to_string);
    let hash_b = rig.b.manifest(O).get(GET).map(str::to_string);

    let ran = run_sync(&rig.a, &rig.b, &rig.link, choose(resolution)).await;

    assert_eq!(ran.conflicts.len(), 1, "exactly one conflict");
    let conflict = &ran.conflicts[0];
    assert_eq!(conflict.path, GET);
    assert_eq!(conflict.kind, ConflictKind::BothChanged);
    assert_eq!(conflict.mine_hash, hash_b, "mine is the planner's hash");
    assert_eq!(conflict.theirs_hash, hash_a, "theirs is the starter's hash");
    assert!(applied(&rig.a, &ran.sid) && applied(&rig.b, &ran.sid));
    (rig, ran)
}

#[tokio::test]
async fn a_conflict_keep_mine_gives_the_planners_version_on_both_sides() {
    let (rig, _) = conflict_run(Resolution::KeepMine).await;
    assert!(rig.a.text(GET).contains(URL_B) && !rig.a.text(GET).contains(URL_A));
    assert!(rig.b.text(GET).contains(URL_B));
    assert!(rig.a.text(GET).contains(A_TOKEN), "A keeps its own token");
    assert!(rig.b.text(GET).contains(B_TOKEN), "B keeps its own token");
    assert!(!rig.a.text(GET).contains(B_TOKEN) && !rig.b.text(GET).contains(A_TOKEN));
    assert_converged(&rig);
}

#[tokio::test]
async fn a_conflict_keep_theirs_gives_the_starters_version_on_both_sides() {
    let (rig, _) = conflict_run(Resolution::KeepTheirs).await;
    assert!(rig.a.text(GET).contains(URL_A) && !rig.a.text(GET).contains(URL_B));
    assert!(rig.b.text(GET).contains(URL_A) && !rig.b.text(GET).contains(URL_B));
    assert!(rig.a.text(GET).contains(A_TOKEN));
    assert!(rig.b.text(GET).contains(B_TOKEN));
    assert_converged(&rig);
}

#[tokio::test]
async fn a_conflict_keep_both_keeps_the_original_path_and_adds_a_from_peer_copy() {
    let (rig, _) = conflict_run(Resolution::KeepBoth).await;
    let copy = "users/get (from peer).yml";
    for peer in [&rig.a, &rig.b] {
        assert!(
            peer.cols.paths(O).contains(&copy.to_string()),
            "the second copy exists: {:?}",
            peer.cols.paths(O)
        );
        assert_eq!(peer.cols.paths(O).len(), 5, "four files plus the copy");
        assert!(peer.text(GET).contains(URL_B), "the original path holds the planner's version");
        assert!(peer.text(copy).contains(URL_A), "the copy holds the starter's version");
    }
    assert!(
        !rig.b.text(copy).contains(A_TOKEN),
        "the copy B received carries no secret of A"
    );
    assert_converged(&rig);
}

// ----- deletes -----

#[tokio::test]
async fn a_delete_nobody_touched_is_taken() {
    let rig = seed_pair().await;
    rig.a.cols.remove(O, LIST);

    let ran = run_sync(&rig.a, &rig.b, &rig.link, no_decisions).await;

    assert!(ran.conflicts.is_empty());
    assert_eq!(ran.summary.deleted_here, 1, "B loses one file");
    assert!(rig.b.cols.text(O, LIST).is_none(), "the file is gone on B");
    assert!(rig.a.cols.text(O, LIST).is_none());
    assert_converged(&rig);
}

#[tokio::test]
async fn a_delete_against_an_edit_keep_mine_restores_the_file_on_both_sides() {
    let rig = seed_pair().await;
    rig.a.cols.remove(O, LIST);
    rig.b.cols.write(O, LIST, &list_yaml(LIST_B));

    let ran = run_sync(&rig.a, &rig.b, &rig.link, choose(Resolution::KeepMine)).await;

    assert_eq!(ran.conflicts.len(), 1);
    assert_eq!(ran.conflicts[0].path, LIST);
    assert_eq!(ran.conflicts[0].kind, ConflictKind::DeleteVsEdit);
    assert!(ran.conflicts[0].mine_hash.is_some(), "the planner still has the file");
    assert!(ran.conflicts[0].theirs_hash.is_none(), "the starter deleted it");
    assert!(rig.a.text(LIST).contains(LIST_B), "A gets the file back");
    assert!(rig.b.text(LIST).contains(LIST_B));
    assert_converged(&rig);
}

#[tokio::test]
async fn a_delete_against_an_edit_keep_theirs_deletes_it_on_both_sides() {
    let rig = seed_pair().await;
    rig.a.cols.remove(O, LIST);
    rig.b.cols.write(O, LIST, &list_yaml(LIST_B));

    let ran = run_sync(&rig.a, &rig.b, &rig.link, choose(Resolution::KeepTheirs)).await;

    assert_eq!(ran.conflicts[0].kind, ConflictKind::DeleteVsEdit);
    assert!(rig.a.cols.text(O, LIST).is_none());
    assert!(rig.b.cols.text(O, LIST).is_none(), "B's edit is dropped by its own choice");
    assert_converged(&rig);
}

// ----- renames -----

const FETCH: &str = "users/fetch.yml";

#[tokio::test]
async fn a_rename_by_the_starter_is_a_rename_on_the_planner() {
    let rig = seed_pair().await;
    rig.a.cols.rename(O, GET, FETCH);

    let ran = run_sync(&rig.a, &rig.b, &rig.link, no_decisions).await;

    assert!(ran.conflicts.is_empty());
    assert!(rig.b.cols.text(O, GET).is_none(), "the old path is gone on B");
    let moved = rig.b.text(FETCH);
    // A delete plus an add would have written the blanked export and lost these.
    assert!(moved.contains(B_TOKEN), "the moved file keeps B's token");
    assert!(moved.contains(B_UID), "the moved file keeps B's uid");
    assert!(moved.contains(B_EXAMPLE), "the moved file keeps B's example");
    assert_converged(&rig);
}

#[tokio::test]
async fn a_rename_by_the_planner_is_a_rename_on_the_starter() {
    let rig = seed_pair().await;
    rig.b.cols.rename(O, GET, FETCH);

    // A starts, B plans: B renamed, so the starter is told to move its file.
    let ran = run_sync(&rig.a, &rig.b, &rig.link, no_decisions).await;

    assert!(ran.conflicts.is_empty());
    assert!(rig.a.cols.text(O, GET).is_none(), "the old path is gone on A");
    let moved = rig.a.text(FETCH);
    assert!(moved.contains(A_TOKEN), "the moved file keeps A's token");
    assert!(moved.contains(A_UID), "the moved file keeps A's uid");
    assert!(!moved.contains(B_TOKEN));
    assert_converged(&rig);
}
```

- [ ] **Step 6: Run the new tests and see them pass**

Run: `cargo test -j4 -p rocket-app e2e_sync`

Expected: 13 passed (the 3 of Step 4 plus `a_clean_two_way_sync_applies_both_edits_and_updates_both_bases`, `b_keeps_its_token_uid_extensions_and_examples_after_a_edit_syncs`, `a_conflict_keep_mine_gives_the_planners_version_on_both_sides`, `a_conflict_keep_theirs_gives_the_starters_version_on_both_sides`, `a_conflict_keep_both_keeps_the_original_path_and_adds_a_from_peer_copy`, `a_delete_nobody_touched_is_taken`, `a_delete_against_an_edit_keep_mine_restores_the_file_on_both_sides`, `a_delete_against_an_edit_keep_theirs_deletes_it_on_both_sides`, `a_rename_by_the_starter_is_a_rename_on_the_planner`, `a_rename_by_the_planner_is_a_rename_on_the_starter`). Two assertions encode choices to confirm against Plan 3: the audit `outcome` text `applied` (assumption 6) and the summary counters (`changed_here` counts files the node showing the dialog writes, `sent_*` the files it sends). If a counter differs, the Plan 1 type docs decide: fix the assertion to the documented meaning and record a `Ruling:`. A failure in the file contents is a bug in Plan 2 (merge) or Plan 3 (apply): fix it there, never the test.

- [ ] **Step 7: Write the failure and safety tests**

Append to `e2e_sync_tests.rs`:

```rust
// ----- failures: nothing changes -----

/// The files and the stored bases of both sides, to compare before and after a failure.
fn snapshot(rig: &Rig) -> (Vec<(String, String)>, String, String) {
    let files = [&rig.a, &rig.b]
        .iter()
        .flat_map(|peer| {
            peer.cols
                .paths(O)
                .into_iter()
                .map(|p| (format!("{}:{p}", peer.node.name), peer.text(&p)))
                .collect::<Vec<_>>()
        })
        .collect();
    (
        files,
        base_hash_of(&rig.a, &rig.link),
        base_hash_of(&rig.b, &rig.link),
    )
}

#[tokio::test]
async fn a_tampered_payload_changes_nothing() {
    let rig = seed_pair().await;
    rig.a.cols.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));
    let before = snapshot(&rig);
    // A's files ZIP reaches B with one damaged character.
    rig.a.wire.tamper(Some(SyncPayloadKind::Files));

    let ran = start_and_wait_review(&rig.a, &rig.b, &rig.link).await;
    rig.b
        .sync
        .review(&ran.sync_id, true, Vec::new())
        .await
        .expect("the planner accepts");
    wait_until("both sides ended", || {
        rig.a.finish(&ran.sid).is_some() && rig.b.finish(&ran.sid).is_some()
    })
    .await;

    assert!(matches!(
        rig.b.finish(&ran.sid),
        Some((SyncFinish::Failed, _))
    ));
    assert!(!applied(&rig.a, &ran.sid), "the starter did not apply");
    assert!(!rig.b.text(GET).contains(URL_A), "B did not take the damaged file");
    assert_eq!(snapshot(&rig), before, "no file and no base moved");
    wait_until("staging is empty on B", || {
        count_files(rig.b.staging_dir.path()) == 0
    })
    .await;
}

#[tokio::test]
async fn declining_the_review_changes_nothing() {
    let rig = seed_pair().await;
    rig.a.cols.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));
    let before = snapshot(&rig);

    let ran = start_and_wait_review(&rig.a, &rig.b, &rig.link).await;
    rig.b
        .sync
        .review(&ran.sync_id, false, Vec::new())
        .await
        .expect("the planner declines");
    wait_until("both sides ended", || {
        rig.a.finish(&ran.sid).is_some() && rig.b.finish(&ran.sid).is_some()
    })
    .await;

    assert!(matches!(
        rig.a.finish(&ran.sid),
        Some((SyncFinish::Declined, _))
    ));
    assert!(matches!(
        rig.b.finish(&ran.sid),
        Some((SyncFinish::Declined, _))
    ));
    assert_eq!(snapshot(&rig), before);
}

#[tokio::test]
async fn the_starter_cancels_while_the_planner_reviews() {
    let rig = seed_pair().await;
    rig.a.cols.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));
    let before = snapshot(&rig);

    let ran = start_and_wait_review(&rig.a, &rig.b, &rig.link).await;
    rig.a.sync.cancel(&ran.sync_id).await.expect("the starter cancels");
    wait_until("both sides ended", || {
        rig.a.finish(&ran.sid).is_some() && rig.b.finish(&ran.sid).is_some()
    })
    .await;

    assert!(matches!(
        rig.a.finish(&ran.sid),
        Some((SyncFinish::Cancelled, _))
    ));
    assert!(matches!(
        rig.b.finish(&ran.sid),
        Some((SyncFinish::Cancelled, _))
    ));
    assert!(
        rig.b.sync.review(&ran.sync_id, true, Vec::new()).await.is_err(),
        "a late accept is refused"
    );
    assert_eq!(snapshot(&rig), before);
}

#[tokio::test]
async fn removing_the_pairing_during_a_sync_ends_it_with_no_change() {
    let rig = seed_pair().await;
    rig.a.cols.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));
    let before = snapshot(&rig);

    let ran = start_and_wait_review(&rig.a, &rig.b, &rig.link).await;
    rig.b
        .node
        .svc
        .remove_paired(&rig.a.id())
        .await
        .expect("B unpairs A");

    wait_until("both sides ended", || {
        rig.a.finish(&ran.sid).is_some() && rig.b.finish(&ran.sid).is_some()
    })
    .await;
    assert!(!applied(&rig.a, &ran.sid) && !applied(&rig.b, &ran.sid));
    assert!(
        rig.b.sync.review(&ran.sync_id, true, Vec::new()).await.is_err(),
        "the session is gone"
    );
    assert_eq!(snapshot(&rig), before, "no file and no base moved");
    wait_until("staging is empty on both sides", || {
        count_files(rig.a.staging_dir.path()) == 0 && count_files(rig.b.staging_dir.path()) == 0
    })
    .await;
}

#[tokio::test]
async fn a_base_mismatch_ends_the_sync_before_any_review() {
    let rig = seed_pair().await;
    rig.a.cols.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));
    // B's stored base drifts: an empty manifest has another base hash.
    let mut skewed = rig.b.link(&rig.link).expect("the link");
    skewed.base = Manifest::new();
    rig.b.links.save(&skewed, &[]).expect("save the skewed base");
    let before = snapshot(&rig);

    let sync_id = rig.a.sync.start(&rig.link).await.expect("the sync starts");
    let sid = sync_id.as_str().to_string();
    wait_until("the starter ended", || rig.a.finish(&sid).is_some()).await;

    assert!(matches!(
        rig.a.finish(&sid),
        Some((SyncFinish::BaseMismatch, _))
    ));
    assert!(rig.b.review_ready(&sid).is_none(), "the planner never asked its user");
    assert_eq!(snapshot(&rig), before);
}

#[tokio::test]
async fn an_unlink_on_one_side_ends_the_next_sync_with_link_missing() {
    let rig = seed_pair().await;
    rig.b.sync.unlink(&rig.link).await.expect("B unlinks");

    assert!(rig.b.link(&rig.link).is_none(), "B's link is gone");
    assert_eq!(
        count_files(&rig.b.link_dir.path().join(rig.link.as_str())),
        0,
        "B's base copies are gone"
    );
    assert!(rig.b.cols.text(O, GET).is_some(), "B's collection is untouched");
    assert!(rig.b.node.audited().iter().any(|k| matches!(
        k,
        AuditEventKind::NetworkLinkRemoved { device_id, collection }
            if *device_id == rig.a.id().short() && collection == O
    )));

    rig.a.cols.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));
    let a_files = rig.a.text(GET);
    let sync_id = rig.a.sync.start(&rig.link).await.expect("the sync starts");
    let sid = sync_id.as_str().to_string();
    wait_until("the starter ended", || rig.a.finish(&sid).is_some()).await;

    assert!(matches!(
        rig.a.finish(&sid),
        Some((SyncFinish::LinkMissing, _))
    ));
    assert_eq!(
        rig.a.link(&rig.link).expect("A keeps the record").state,
        LinkState::Broken,
        "A marks its link broken"
    );
    assert_eq!(rig.a.text(GET), a_files, "A's collection is untouched");
    assert!(!rig.b.text(GET).contains(URL_A), "B got nothing");
}

// ----- the router -----

#[tokio::test]
async fn a_send_and_a_sync_can_run_at_the_same_time() {
    let rig = seed_pair().await;
    rig.a.cols.seed("Extras", vec![file(
        "opencollection.yml",
        "opencollection: \"1.0.0\"\ninfo:\n  name: \"Extras\"\n",
    )]);
    rig.a.cols.write(O, GET, &get_yaml(URL_A, A_TOKEN, A_LOCAL));

    // The sync is waiting for the planner's user when the send starts.
    let ran = start_and_wait_review(&rig.a, &rig.b, &rig.link).await;
    let send_id = rig
        .a
        .transfer
        .send(&rig.b.id(), "Extras", false, false)
        .await
        .expect("the offer is sent")
        .transfer_id;
    assert_eq!(wait_offer_id(&rig.b).await, send_id);
    rig.b.transfer.respond(&send_id, true).await.expect("accept");
    wait_until("the send completed", || {
        rig.a
            .node
            .events
            .finished_outcomes()
            .contains(&(Direction::Send, Outcome::Completed))
    })
    .await;
    assert!(rig.b.cols.names().contains(&"Extras".to_string()));

    // The sync is still open and finishes normally.
    assert!(rig.a.finish(&ran.sid).is_none());
    accept_and_finish(&rig.a, &rig.b, &ran, Vec::new()).await;
    assert!(applied(&rig.a, &ran.sid) && applied(&rig.b, &ran.sid));
    assert!(rig.b.text(GET).contains(URL_A));
    assert_converged(&rig);
}
```

- [ ] **Step 8: Run all the sync tests and the older e2e tests**

Run: `cargo test -j4 -p rocket-app e2e_sync`

Expected: 20 passed. In order: `the_first_send_with_keep_in_sync_creates_equal_links_on_both_sides`, `a_send_without_keep_in_sync_creates_no_link`, `the_seeded_copies_start_from_one_base`, `a_clean_two_way_sync_applies_both_edits_and_updates_both_bases`, `b_keeps_its_token_uid_extensions_and_examples_after_a_edit_syncs`, `a_conflict_keep_mine_gives_the_planners_version_on_both_sides`, `a_conflict_keep_theirs_gives_the_starters_version_on_both_sides`, `a_conflict_keep_both_keeps_the_original_path_and_adds_a_from_peer_copy`, `a_delete_nobody_touched_is_taken`, `a_delete_against_an_edit_keep_mine_restores_the_file_on_both_sides`, `a_delete_against_an_edit_keep_theirs_deletes_it_on_both_sides`, `a_rename_by_the_starter_is_a_rename_on_the_planner`, `a_rename_by_the_planner_is_a_rename_on_the_starter`, `a_tampered_payload_changes_nothing`, `declining_the_review_changes_nothing`, `the_starter_cancels_while_the_planner_reviews`, `removing_the_pairing_during_a_sync_ends_it_with_no_change`, `a_base_mismatch_ends_the_sync_before_any_review`, `an_unlink_on_one_side_ends_the_next_sync_with_link_missing`, `a_send_and_a_sync_can_run_at_the_same_time`.

Two outcome names encode Plan 3 choices to confirm against its code: a damaged payload ends the planner as `SyncFinish::Failed` (the enum has no `Damaged`, index), and a cancel ends both sides `Cancelled`. If Plan 3 maps a case to another variant, fix the assertion to the documented mapping and record a `Ruling:`; the "no file and no base moved" assertions are never relaxed.

Run: `cargo test -j4 -p rocket-app e2e`

Expected: every test passes (the older e2e tests, the send tests and the 20 new ones), 0 failed. If a send test fails to compile because `TransferDeps` now has a sync field, Plan 3 missed `e2e_transfer_tests.rs::attach_in`; fix that literal there by adding the field Plan 3 chose, and record a ruling.

- [ ] **Step 9: Whole-plan verification**

Run, one crate at a time:

```bash
cargo fmt -p rocket-app -- --check
cargo fmt -p rocket -- --check
cargo check -j4 -p rocket-app --tests
cargo test -j4 -p rocket-app e2e
cargo check -j4 -p rocket
cargo test -j4 -p rocket network_sync
cargo test -j4 -p rocket sync_events
cargo test -j4 -p rocket ipc_acl
cargo test -j4 -p rocket import_wiring
cargo clippy -j4 -p rocket-app --tests
```

Expected: both `fmt` checks print nothing; every `check` finishes; `e2e` passes with 0 failed; `network_sync` passes 7; `sync_events` passes 2; `ipc_acl` and `import_wiring` pass; clippy reports no warnings in the new code (if it flags `clone_on_copy` in the test file, remove the `.clone()`).

Then check that every new command is registered in both lists:

```bash
for c in network_send_collection network_sync_start network_sync_review network_sync_confirm network_sync_cancel network_list_links network_unlink; do
  grep -qx "$c" src-tauri/app-commands.txt || echo "MISSING in app-commands.txt: $c"
  grep -q "commands::[a-z_]*::$c," src-tauri/src/lib.rs || echo "MISSING in generate_handler: $c"
done
grep -n "SyncService::new(\|TransferService::new(\|FrameRouter::new(\|set_transfer_sink(" src-tauri/src/lib.rs
```

Expected: the loop prints nothing; the grep prints four lines in the order `SyncService::new(`, `TransferService::new(`, `FrameRouter::new(`, `set_transfer_sink(`.

No commit; reviewed at the plan end.

---

## Self-review

- Spec section 3 success criteria: A and B edit different requests (`a_clean_two_way_sync...`); same request, three choices (`a_conflict_keep_*`); rename is a rename (`a_rename_by_*`); B keeps token, uid, `extensions.rocketapi` and examples after A's edit (`b_keeps_its_token_uid_extensions_and_examples_after_a_edit_syncs`); delete untouched and delete against edit (`a_delete_*`); declining, a dropped connection (pairing removal), a damaged payload and a base mismatch end with no change (Review Focus 5); a secret never reaches the other copy (asserted in the keep-own-keys, conflict and keep-both tests). Not covered here on purpose: ignoring for 2 minutes (paused time, Plan 3), the crash during apply (fault hooks of the file transaction, Plan 2 and 3), the exact 10 MiB boundary (pure checks in Plans 1 and 3), a synced script or auth marked in the dialog and no trust record (Plan 3 service tests and Plan 2 apply tests; `scripts_changed` and `auth_changed` are asserted empty in the clean sync only), an unsafe file (Plan 2 validator and Plan 3 service tests).
- Spec section 12 e2e list: clean two-way sync, a conflict with each of the three choices, a delete against an edit, a rename, a tampered payload, a base mismatch, an unlink on one side, a pairing removed during a sync: all present. Added: first send with and without keep in sync, decline, cancel, send plus sync together.
- Spec section 11 `src-tauri` row: commands, event channel, `app-commands.txt`, camelCase DTOs, wire contract tests: Task 1. Startup: Task 2.
- Index "Plan 4 produces": the seven commands, `DecisionDto`, `LinkDto` keys (`linkId, deviceId, displayName, shortId, collectionName, state, updatedAt`), the `network-sync` channel: Task 1. `SyncStartedDto { sync_id }` serializes as `syncId`.
- Name consistency: `network_sync_start/review/confirm/cancel`, `network_list_links`, `network_unlink` match the index and Plan 5's wrappers. `Resolution` wire values `keep_mine/keep_theirs/keep_both` match the index serde rule.
- No new production API: the only new code in production files is the command module, the `keep_in_sync` parameter and the lib.rs wiring. Test seams are the `PeerSender` trait (decorator `WireTap`), the `SyncCollectionPort` and `CollectionImportPort` traits (fakes), `LinkRepository::save` (base drift) and `SyncService::create_link` (seeding), all already in the contract.
- Rulings to record in the ledger: no command-level unit test; no shutdown hook for `SyncService`; tamper through a `PeerSender` decorator, not a hook or a relay; convergence compared on manifests not raw bytes; the assumed names 1 to 7 as confirmed in Step 0b; what Step 8 found about the `SyncFinish` mapping and the audit outcome text.
