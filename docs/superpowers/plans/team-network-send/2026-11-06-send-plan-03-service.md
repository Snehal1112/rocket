# Team network send: Plan 3, service

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 3 of 5.** Previous: `2026-11-06-send-plan-02-import-side.md` (and Plan 01, protocol; both must be applied and green). Next: `2026-11-06-send-plan-04-tauri-e2e.md`. Recommended model: Sonnet. Index (binding names): `docs/superpowers/plans/team-network-send/00-plan-index.md`. No commit during the plan.

**Goal:** `NetworkService` can send one frame to one paired device, hands the six transfer frames to a sink (each one counts as a pong; `Chunk` and `ChunkAck` skip the per-frame block-list read) and tells the sink when a peer is gone (socket ended or pairing removed). A new `TransferService` sends a collection (export, size check, offer, stream chunks inside the 32-chunk ack window, wait for the receiver's `TransferResult`, `Unconfirmed` when it never comes after the receiver's final `ChunkAck`, `Failed` when the stream ends before that ack) and receives one (limits, pending offers, accept or decline, staged chunks, a `ChunkAck` every 16 chunks and after the last, size and hash check, import, cleanup, `TransferResult` back). Everything is tested with fakes. No Tauri code and no real transport yet (Plan 4).

**Architecture:** Task 1 adds `send_to`, `is_online`, `set_transfer_sink`, `peer_label`, `deliver_transfer_frame` and `notify_peer_gone` to `NetworkService` in a new file `network/transfer_link.rs`, one match arm in `run_socket` (plus the block-list guard change and the pong refresh), and two `notify_peer_gone` calls (after every `run_socket` in `run_device`, and in `remove_local`). Task 2 adds `network/transfer.rs` (the `PeerSender` trait, `Timing`, `TransferDeps`, `TransferService`, the frame queue) and `network/transfer_send.rs` (sender side, `SendStarted`, the Plan 4 test hooks). Task 3 adds `network/transfer_receive.rs` (receiver side) and wires it into `transfer.rs`. `TransferService` never touches a socket, a file or the workspace: it uses `PeerSender`, `TransferStaging` (`create`, `release`), `CollectionImportPort`, `CollectionExportService`, `EventPublisher` and `SecurityAuditPublisher`. It holds the Plan 01 `OutgoingTransfer` and `IncomingTransfer` machines plus one `TransferLimits` and delegates every rule check to them; the service itself only does I/O and orchestration (frames, staging, hashing, import, events, audit, timers, the ack window wait, the sender's wait for the result, the peer-gone cleanup and the idle fallback). All state sits behind one `std::sync::Mutex`.

**Tech Stack:** Rust, tokio (`test-util` paused time in tests), async-trait, sha2, base64, `rocket-network` (Plan 01), the ports in `network/transfer_ports.rs` (Plan 02).

**Spec:** `docs/superpowers/specs/2026-11-06-team-network-send-collection-design.md`, sections 2 (decisions 3, 4, 6), 3, 5, 6, 7, 8 (first, second, fourth, fifth and last bullets), 9 (`rocket-app`), 10, 11 (service tests).

## Needs from Plan 1 and Plan 2

Plan 01 and Plan 02 (as reviewed) already provide every name below. This plan uses them exactly as written. Task 1 Step 1 and Task 2 Step 2 grep for them and stop if one is missing.

- Plan 01 `limits.rs`: `SENDER_RESULT_TIMEOUT` (60 s), `ACK_EVERY_CHUNKS` (16), `MAX_UNACKED_CHUNKS` (32).
- Plan 01 `messages.rs`: `PeerMessage::TransferResult { transfer_id, outcome: ResultOutcome }`, `ResultOutcome` (`Completed | Damaged | Rejected | Failed`) and `PeerMessage::ChunkAck { transfer_id, next_seq: u32 }` (tag `chunk_ack`). Six transfer variants in total.
- Plan 01 `transfer.rs`: `TransferSink::{on_frame, on_peer_gone}`; `Outcome` with `Unconfirmed` (wire `unconfirmed`), `From<ResultOutcome>` and `From<AbortReason>`; `offer_name_is_valid(&str) -> bool`; `OutgoingTransfer::{fail_with, on_result, awaiting_result, with_offer_ttl, window_open, on_ack, connection_lost, result_timed_out}`; `IncomingTransfer::with_offer_ttl`; `ChunkStep::Data { bytes, done_bytes, last, ack: Option<PeerMessage> }`.
  - `awaiting_result()` is true only after the receiver's final `ChunkAck` (`next_seq == chunk_count`) arrived and until a result arrives; `last_chunk_built()` is true from the moment the last chunk frame is built (before the final ack). `on_result` accepts one result from `last_chunk_built()` on (a result that overtakes the final ack counts): `Completed` keeps the machine `Done` and returns `Some(Completed)`, the other three end it with the same-named `Outcome`. A confirmed `Completed` can no longer be failed.
  - `fail_with` ends a sender that is `Offered`, `Sending` or `Done` and still waiting for the result.
  - `window_open()` is true while fewer than `MAX_UNACKED_CHUNKS` chunks are sent and not acknowledged. `on_ack(next_seq)` returns `None` for a good ack (also one that arrives after the last chunk) and `Some(Failed)` when `next_seq` goes backwards or past the chunks sent (the machine is then final).
  - `connection_lost()` returns `Some(Unconfirmed)` while `awaiting_result()` (final ack received) and `Some(Failed)` otherwise, also after the last chunk was built but before the final ack. `result_timed_out()` returns `Some(Unconfirmed)` while `awaiting_result()` and `None` otherwise. The result timer starts at the final ack and only calls `result_timed_out()` on an entry that still `awaiting_result()`. `unacked_chunks() -> u32` exists too; the service does not need it.
  - `on_abort` still ignores a `TransferAbort` once the last chunk is built (`last_chunk_built()`), so the service ends that case with `fail_with(Outcome::from(reason))` (already the code in `abort_outgoing`).
  - `ChunkStep::Data.ack` is `Some(ChunkAck)` after every 16th chunk accepted in order and on the last chunk, `None` otherwise.
- Plan 01 `rocket-audit`: `NetworkCollectionSent { device_id, collection, outcome: String }` and `NetworkCollectionReceived { device_id, collection, outcome: String }`; `outcome` is the `Outcome` wire name.
- Plan 01 `events.rs`: `TransferEvent::Finished { .., message: Option<String> }`, and `TransferEvent` is `Clone`.
- Plan 02: `TransferStaging::release`, `UNSAFE_ZIP_MESSAGE` (public const in `transfer_ports.rs`), and the importer adapter returns `DomainError::InvalidInput(UNSAFE_ZIP_MESSAGE)` only for validator refusals.
- `rocket_export::{ExportReport, ExportNote, ExportAction}` (existing).

## Rulings recorded by this plan

1. **`PeerSender` trait.** `TransferService` talks to the network through a small trait `PeerSender { send_to, peer_label, is_online, set_transfer_sink }` declared in `network/transfer.rs`. `NetworkService` implements it. `TransferDeps.network` is therefore `Arc<dyn PeerSender>`, not `Arc<NetworkService>` as the index sketch says. Plan 4 is unaffected: `TransferDeps { network: network_svc.clone(), .. }` coerces. Unit tests use a fake peer set, so no sockets and no real `NetworkService`.
2. **One queue, one worker, order kept.** `TransferSink::on_frame` and `on_peer_gone` are sync and run inside the network loops, so they must not block and must keep order. Both push an item (a frame or a peer-gone notice) into one bounded queue (256, `try_send`). A full queue drops a frame (a missing chunk then fails the transfer by the sequence rule), but a peer-gone notice that does not fit runs inline, so it is never lost. One worker task handles the items in order. Every handler is sync and cheap: the slow work (the finalize step with the hash check and the import, and every outbound `tell`) runs as its own spawned task, never inside the worker (index ruling 11), so one long import cannot hold back another peer's chunks. The worker starts lazily on the first `on_frame`, `on_peer_gone`, `send` or `respond`, because `TransferService::new` is called in `src-tauri` setup where no runtime may be current. Paths the user starts (`respond`, `cancel`) are not on the worker and await their `send_to` directly, so they can return its error.
3. **The Plan 01 state machines own every rule.** `Out` holds an `OutgoingTransfer` and `In` holds an `IncomingTransfer`, and `Inner` holds one `TransferLimits`. The service never re-checks a rule: offer shape, size and chunk count come from `OutgoingTransfer::new` and `IncomingTransfer::from_offer` (`OfferError`), offer expiry from `expire_if_due`, `on_answer` and `respond`, chunk order, bounds and base64 from `next_chunk` and `on_chunk` (`ChunkStep`), the result from `on_result`, per-peer and global caps and the offer rate from `TransferLimits::{admit_offer, begin_outgoing, mark_active, release}` (`Admission`). The service only does I/O and orchestration. The machines take a `std::time::Instant`. The service passes `tokio::time::Instant::now().into_std()` (`TransferService::now`), which follows the paused clock in tests.
4. **Timing seam (index ruling 3).** `TransferService::new_with_timing(deps, Timing { offer_ttl, idle_limit, result_timeout })` (`#[doc(hidden)]`); `new` uses `Timing::default()` (120 s, 120 s, `SENDER_RESULT_TIMEOUT`). The service stores the values, calls `.with_offer_ttl(timing.offer_ttl)` on every machine it builds (the sender machine is built inside a `spawn_blocking` closure, so the value is captured by copy) and the timers sleep the same values. So a test that shortens `offer_ttl` shortens the machine and the timer together.
5. **The sender waits for the receiver's result (index ruling 1, second-review R4).** The sender machine is `Done` as soon as the last chunk frame is built. The service keeps the entry until the receiver's `TransferResult` arrives, up to `timing.result_timeout`. `Completed` is reported when the result says `completed`. `damaged`, `rejected` and `failed` end the send with the same-named outcome. The 60 s result timer starts only when the receiver's final `ChunkAck` arrives. A missing result after that timeout ends it as `Unconfirmed` through `OutgoingTransfer::result_timed_out` ("Sent, but the other node did not confirm." on the frontend), not as a failure, because the receiver confirmed every chunk. Without the final ack the send is not `Unconfirmed`: the idle limit (`timing.idle_limit`, counted from the last ack) ends it `Failed` ("Sending failed.") and tells the peer `failed`. `NetworkCollectionSent` is audited for `Completed`, `Unconfirmed` and a sender-side `Failed` after the last chunk (ruling 9). A `TransferAbort` from the peer in that window ends it with the mapped outcome. A result for an unknown or finished id, from another device, or before the last chunk is ignored. There is no result grace window any more. A sender `cancel` after the last chunk is refused with `InvalidInput("Already sent. Waiting for the result.")`.
6. **Peer gone (index ruling 2, second-review R4 and R19).** `NetworkService` calls `TransferSink::on_peer_gone` when a socket ends and when a pairing is removed. `TransferService` then ends every transfer of that device, both roles, at once: a sender entry with `OutgoingTransfer::connection_lost()` (`Unconfirmed` only after the receiver's final `ChunkAck`, `Failed` before it, even when the last chunk was already sent), a receiver entry with `connection_lost()` (its staged file is discarded). A pending incoming offer ended this way emits `Finished { outcome: failed, message: Some("The other node went offline.") }`; every other receiver drop keeps `message: None`. Nothing is told to the gone peer. A receiver entry that is already finalizing only gets its machine failed; the finalize task ends it (ruling 16). The 120 s idle limit stays as a fallback for a half-open connection and is `timing.idle_limit`, a field, so the test seam can shorten it.
7. **Staged files: `discard` or `release`, never `purge`.** A receive that ends before its last chunk (abort, cancel, refused chunk, peer gone, idle drop, failed accept) calls `StagedZip::discard`. After the last chunk `finish()` returns a `StagedFile`, and every exit after that point calls `TransferStaging::release(&file)` once (hash mismatch, interrupted, import success, `Rejected`, import failure, a panicked import task). A failing `release` is logged and does not change the transfer result; the start-up purge in Plan 4 is the safety net. The service never calls `purge`.
8. **Reject versus fail.** `CollectionImportPort::import_zip` returning `DomainError::InvalidInput(m)` with `m == UNSAFE_ZIP_MESSAGE` (exact text) means the strict validator refused the ZIP (`Outcome::Rejected`, `Finished.message` is that text). Any other error, including another `InvalidInput`, is `Outcome::Failed` and its text goes into `Finished.message` on the receiving side. A damaged payload is `Damaged` with no message. The sender only sees the four `ResultOutcome` values.
9. **Audit ids and outcomes (second-review R6).** `NetworkCollectionSent` and `NetworkCollectionReceived` carry the short device id (`XXXX-XXXX`), as spec section 8 says, and `outcome`, the `Outcome` wire name from `outcome_name` in `transfer.rs`. The sender audits `completed`, `unconfirmed`, and `failed` when it lost the stream (peer gone, idle stall, failed frame send) after the last chunk was built; a failure the receiver reported (`TransferAbort { failed }` or a `failed` result) is not audited. The receiver audits `completed` (collection = the imported name), `damaged` and `rejected` (collection = the offered name). Nothing else is audited. `shortDeviceId` on the frontend leaves a short id unchanged.
10. **Empty collection.** An export with zero files is refused before any offer: "There is nothing to send in that collection."
11. **`cancel` is idempotent, with two exceptions.** An unknown or finished id is `Ok(())`. A pending incoming offer is declined. A sender cancel after the last chunk is refused (ruling 5). A receiver cancel while the import runs is ignored (the import is imminent and cannot be undone).
12. **Admission order.** For an incoming offer the service parses the id (a bad id is ignored, nothing can be named in a reply), asks `TransferLimits::admit_offer` first (so every new offer counts against the peer's rate window, even a malformed one), then builds the `IncomingTransfer`. A rejected offer releases its limits entry and is answered with `OfferError::abort_reason()`.
13. **Words for `OfferError` on the send side (second-review R15).** `OutgoingTransfer::new` decides whether the offer is acceptable. The service only words the refusal (`offer_error`): the 10 MiB boundary itself is a Plan 1 test. `OfferError::BadName` is `InvalidInput("This collection name cannot be sent. Rename it and try again.")`, and `send` checks `offer_name_is_valid` before any export, so a bad name costs no export. The ZIP is hashed before the machine is built, because the machine takes the hash.
14. **Send order (index ruling 10).** `send` checks, in this order: the name is not blank, the name passes `offer_name_is_valid`, the peer is paired (`peer_label`), the peer is online (`PeerSender::is_online`), then it claims the peer's single outgoing slot with `TransferLimits::begin_outgoing`, then it exports. So an offline or busy peer is refused without any export. `offer_bytes` (hash, machine, offer frame, timer) is the part after the export; `offer_raw` (ruling 17) claims a slot and calls it.
15. **`send` returns `SendStarted { transfer_id, notes }` (index ruling 5).** `notes` has one line per `ExportReport` item whose action is not `Preserved` or `Translated` (so skipped and blanked items), formatted `<feature>: <detail>` or, when the item has no detail, `<feature>: skipped` or `<feature>: blanked`. Identical lines are listed once. `export_notes` is a pure function with its own test.
16. **Finalize interlock (review item I7).** At the last chunk the receiver entry stays in the map with `finalizing = true` and its staged file is taken out for `finish`. An abort or a peer-gone notice that arrives while it finalizes only fails the machine (`fail_with`); the finalize task checks the machine after `finish` and before the import, and ends the transfer without importing when it was failed. An entry that is finalizing keeps its slot in the limits and is skipped by the idle watch. Once the import has run the result is `Completed` even if an abort came later, because the collection exists.
17. **Hooks for Plan 4 (second-review R8).** `#[cfg(test)] pub(crate)` on `TransferService`, and the fields they set (`chunk_delay`, `tamper`) are `#[cfg(test)]` too, so a release build has no way to flip a byte or offer raw bytes. This works because the e2e tests (`network/e2e_tests.rs`, `network/e2e_transfer_tests.rs`) are inside `rocket-app`; no `test-support` feature. `new_with_timing` stays `#[doc(hidden)] pub`. The hooks: `offer_raw(&self, device, name, bytes) -> DomainResult<String>` (`claim_send`, then `offer_bytes`, free the slot on error, return the id), `set_chunk_delay(&self, Duration)` (a sleep before each chunk is taken, before the lock) and `set_tamper(&self, Option<u32>)` (flips the first byte of a copied slice of chunk `seq` before `next_chunk`, so the receiver's hash check fails). Plan 4 calls exactly these.
18. **Size and hash check after `finish` (second-review R14).** The machine only releases exact-length chunks, but a broken staging can still lose or add bytes, so the finalize task compares `StagedFile.len` with the offered `size_bytes` and then the SHA-256. Either mismatch is `Damaged` ("The collection arrived damaged and was not imported.").
19. **Ack window, sender (second-review R2).** `Out` holds `last_ack: Instant` (set at the offer, the accept and every good ack) and `ack_signal: Arc<tokio::sync::Notify>`. The stream asks `window_open()` under the lock before each chunk. When it is closed the stream waits on `ack_signal` or until `last_ack + timing.idle_limit`; at that deadline, with the window still closed, the send ends `Failed` and the peer is told `TransferAbort { failed }`. After the last chunk a `spawn_final_ack_timer` applies the same deadline until the final `ChunkAck` arrives (that ack also starts the result timer); `Out.all_chunks_out`, `Out.audit_failed` and `Out.result_timer_started` carry that state. `on_ack` uses `notify_one`, which keeps a permit when nobody waits yet, so an ack that arrives before the wait is never lost. A bad ack (`on_ack` returns `Some(Failed)`) ends the send and tells the peer `failed`.
20. **Ack window, receiver (second-review R2).** Each incoming entry has an ack pump from Accept on: an unbounded channel and one spawned task that sends its frames one after the other with `tell`. `ChunkStep::Data.ack` goes into that channel after the append succeeded. The pump keeps acks in order (two spawned `tell_later` calls could swap, and a backwards ack fails the send by the Plan 1 rule). The pump ends when the entry is dropped (its sender goes with it).
21. **Liveness and the block check in `run_socket` (second-review R2, R13).** Every transfer frame calls `presence.on_pong()` before it goes to the sink, so the ping rule cannot mark a busy peer offline. The `is_blocked` arm (one YAML read per frame) now skips `Chunk` and `ChunkAck`: those only touch a transfer admitted after an offer or answer that did pass the check, and a block goes through `unpair_by_user` -> `remove_local`, which closes the socket and calls `on_peer_gone`. No per-transfer cache is needed.
22. **Lost frames (second-review R7).** No retransmit. A dropped chunk fails at the next chunk (the receiver's sequence rule, `TransferAbort { failed }` to the sender, both sides `Failed`). A stream that stops (lost tail, socket swapped by the duplicate rule, a full worker queue) ends `Failed` on the receiver at `timing.idle_limit`, which tells the sender `failed`; a sender with a closed window fails at its own idle limit (ruling 19); a sender past the last chunk but before the receiver's final `ChunkAck` fails at its own idle limit (ruling 19) and audits `failed`; only after the final ack does a missing result end `Unconfirmed` at `result_timeout`, unless the receiver's `failed` arrives first. A socket swap is not reported as peer gone.
23. **Import cleanup is not this plan's job (second-review R3).** The service never deletes a collection. On a failed import the Plan 2 adapter removes exactly the folders in `ImportService::created_roots()`; there is no before/after comparison of the collection list anywhere.
## Global Constraints

- `-j4` on every cargo command. Never `cargo test --workspace` or `--all`. Filter by test module, never by `transfer`: that word also matches the `transfer_ports` tests of Plan 02. Use `cargo test -j4 -p rocket-app transfer_link_tests` (then `transfer_send_tests`, `transfer_receive_tests`) and `cargo check -j4 -p rocket-app`. Several filters go after `--`.
- No bare unwrap call in production code. Tests use `expect("reason")`. A repo hook rejects Rust text that contains a bare unwrap call, tests included.
- Never hold the `std::sync::Mutex` guard across an `.await`. Take data out, drop the guard, then await. File work inside the lock is allowed only when it is sync and short (`StagedZip::append`, `TransferStaging::create`). `discard`, `finish` and `release` run outside the lock. Publishing an event is sync and non-blocking, so a `Progress` event may be published under the lock to keep it ordered before the `Finished` of the same transfer.
- `rocket-app` holds only `Arc<dyn Trait>` ports. It never names an infra type.
- Blocking work (export, hashing, `finish`, `existing_names`, `import_zip`) runs in `tokio::task::spawn_blocking`. `release` is one file delete and runs inline.
- Time in tests. Tests that never reach a `spawn_blocking` use `#[tokio::test(start_paused = true)]` and `settle()` from `test_support`. A test that does reach one (a send, or the finalize step of a receive) uses the real clock and `wait_for` (polling, defined in `transfer_fakes.rs`), because paused time can jump over the 120 s timers while a blocking task runs. A sender test that needs the clock calls `tokio::time::pause()` once, after `send().await` returned.
- Events and audit are fire-and-forget. A failed `send_to` of an abort frame is logged at debug level and ignored. Nothing logs a collection name, a payload or a full device id.
- Comments are short full sentences that end with a period. No emojis.
- No commit during the plan. The user reviews the whole plan, then commits through `dev-workflow-skills:1-git-commit` with explicit paths.

## Review Focus

1. **Cancel races the last chunk.** Before the last chunk a cancel ends the send once as `Cancelled`. After the last chunk (final ack or not) it is refused ("Already sent. Waiting for the result."), the send goes on and completes only when the receiver's result says so. Tests: `cancel_while_streaming_ends_once_as_cancelled`, `cancel_after_the_last_chunk_is_refused_and_the_send_still_completes` (Task 2).
2. **Accept arrives after expiry.** Receiver side: `respond(accept)` at the deadline sends no `OfferAnswer`, creates no staged file, returns "The request expired." and `Finished` is emitted once. Sender side: an `OfferAnswer` after the offer expired is ignored and no chunk is sent. Tests: `accepting_after_expiry_sends_nothing_and_reports_expired` (Task 3), `an_answer_after_the_offer_expired_is_ignored` (Task 2).
3. **A peer sends chunks without accepting.** A `Chunk` for a pending (unaccepted) offer aborts it with `failed`, creates no staged file and later `respond` fails. Test: `chunks_before_accept_abort_the_transfer` (Task 3).
4. **Two simultaneous offers from one peer.** The second is answered `busy`, whether the first is pending or already receiving. On the sender side a second `send` to the same peer is refused before any export. Tests: `a_second_offer_from_the_same_peer_is_busy` (Task 3), `a_second_send_to_the_same_peer_is_refused` (Task 2).
5. **Refused before any export.** An empty collection, an offline node and an unpaired node are refused with no offer, and the export is never called for the last two. Tests: `an_empty_collection_is_refused_before_any_offer`, `sending_to_an_unpaired_or_offline_node_is_refused` (Task 2).
6. **Frames from a blocked device.** An offer, answer, abort or result from a blocked device removes the connection first and never reaches the sink; `Chunk` and `ChunkAck` skip that read (ruling 21) and a real block ends the transfer through `on_peer_gone`. Tests: `a_frame_from_a_blocked_device_never_reaches_the_sink`, `chunk_and_ack_frames_skip_the_block_list_read`, `blocking_a_paired_device_reports_the_peer_gone` (Task 1).
7. **Every exit path frees the staged file.** Unfinished: `discard`. Finished: `release`. Success, abort, peer gone, idle drop, damaged, rejected, import failure, cancel, interrupted finalize, and a failing `release` that must not change the result. Tests use `FakeStaging::assert_clean` in each path (Task 3), plus `a_failing_release_does_not_change_the_result`.
8. **The service wires the machines and adds no rule.** Every limit is answered through the machine's answer: `the_fourth_pending_offer_is_busy`, `refused_offers_are_answered_with_the_reason_of_the_check`, `a_refused_chunk_fails_the_transfer_and_cleans_up` (Task 3). The rule tables themselves are Plan 1 tests.
9. **Sent means imported, and an unconfirmed send is not a failure.** The sender reports `Completed` only after the receiver's `completed` result; `damaged`, `rejected` and `failed` results end it with the same outcome; a missing result after the receiver's final `ChunkAck` ends it as `Unconfirmed` after the timeout and is audited with `outcome: "unconfirmed"`; a stream lost before that ack (peer gone or idle stall) ends it `Failed` and is audited with `outcome: "failed"`; a receiver `TransferAbort { failed }` while waiting is `Failed`; a result from another device or before the last chunk is ignored. Tests: `an_accepted_offer_streams_every_chunk_and_completes_on_the_result`, `a_damaged_result_ends_the_send_as_damaged`, `a_rejected_result_ends_the_send_as_rejected`, `a_failed_result_ends_the_send_as_failed`, `a_missing_result_times_out_as_unconfirmed`, `the_result_timer_starts_only_with_the_final_ack`, `no_final_ack_for_the_idle_limit_fails_the_send_even_after_the_last_chunk`, `a_failure_report_after_the_last_chunk_fails_the_send`, `a_result_before_the_last_chunk_is_ignored`, `frames_from_another_device_are_ignored` (Task 2); `accept_stages_imports_with_the_display_name_and_cleans_up`, `a_tampered_payload_is_damaged`, `a_rejected_zip_is_reported_rejected`, `an_import_failure_is_reported_failed` (Task 3).
10. **A dead connection ends transfers at once.** `run_socket` ending and a removed pairing both reach the sink, and both roles clean up with no idle wait. Tests: `a_socket_that_ends_reports_the_peer_gone`, `a_peer_unpair_reports_the_peer_gone`, `removing_the_pairing_reports_the_peer_gone` (Task 1); `a_peer_gone_after_the_final_ack_is_unconfirmed`, `a_peer_gone_after_the_last_chunk_but_before_the_final_ack_fails_the_send`, `a_peer_gone_fails_an_unanswered_offer` (Task 2); `peer_gone_drops_a_pending_offer` (with "The other node went offline."), `peer_gone_drops_a_receiving_transfer_at_once` (Task 3).
11. **An abort or a gone peer during the finalize step never imports.** Tests: `an_abort_during_the_finish_stops_the_import`, `peer_gone_during_the_finish_stops_the_import` (Task 3).
12. **A slow import does not hold back other peers.** Test: `a_slow_import_does_not_block_other_peers` (Task 3).
13. **The timing seam shortens the machine and the timer together.** Tests: `a_short_offer_ttl_expires_the_offer_at_that_ttl`, `a_short_result_timeout_ends_the_send_unconfirmed_at_that_timeout` (Task 2); `a_short_offer_ttl_expires_a_pending_offer_at_that_ttl`, `a_short_idle_limit_drops_the_receive_at_that_limit` (Task 3).
14. **The send notes.** Skipped and blanked report items become one line each; kept and translated items do not. Test: `notes_list_skipped_and_blanked_items_only` (Task 2).
15. **Hooks Plan 4 relies on, test builds only.** `offer_raw`, `set_chunk_delay`, `set_tamper` behave as ruling 17 says and are `#[cfg(test)] pub(crate)`. Tests: `offer_raw_offers_the_given_bytes_and_returns_the_id`, `offer_raw_frees_the_slot_when_the_offer_fails`, `the_chunk_delay_hook_waits_before_each_chunk`, `the_tamper_hook_flips_a_byte_of_the_chosen_chunk` (Task 2); Task 3 Step 9 greps that no hook is `pub` without `#[cfg(test)]`.
16. **Slow link: the ack window holds.** The sender never has more than 32 chunks out without an ack, and a peer that stops acking fails the send at the idle limit. Without the window the first test sees every chunk at once. Tests: `the_sender_never_has_more_than_32_chunks_unacked`, `no_ack_for_the_idle_limit_fails_the_send`, `an_ack_past_the_chunks_sent_fails_the_send` (Task 2); `the_receiver_acks_every_16_chunks_and_the_last` (Task 3).
17. **A busy transfer keeps the peer online.** Transfer frames count as pongs. Test: `transfer_frames_keep_a_peer_online_without_pongs` (Task 1).
18. **Lost frames end the transfer.** Tests: `a_dropped_chunk_fails_both_sides_at_the_next_chunk`, `a_stalled_stream_fails_both_sides_within_the_idle_limit` (Task 3, two rigs joined by a relay).
19. **Cancel of a pending offer from either side.** Tests: `a_sender_cancel_of_a_pending_offer_closes_it`, `cancelling_a_pending_offer_declines_it` (Task 3).
20. **Environments stay out unless asked for.** A source with an environment file sends no `environments/` entry with `include_environments = false`, and does with `true`. Test: `a_send_without_environments_carries_no_environment_files` (Task 2).
21. **A size mismatch is damaged too.** Test: `a_wrong_staged_length_is_damaged` (Task 3).
22. **Bad names and audit outcomes.** Tests: `a_bad_collection_name_is_refused_with_the_rename_text`, `outcome_names_match_the_wire_names` (Task 2); the receiver audit rows in `accept_stages_imports_with_the_display_name_and_cleans_up`, `a_tampered_payload_is_damaged`, `a_rejected_zip_is_reported_rejected` (Task 3).
## File Structure

| File | Change | Responsibility |
|---|---|---|
| `crates/rocket-app/src/network/service.rs` | modify | New field `transfer_sink` |
| `crates/rocket-app/src/network/transfer_link.rs` | create | `send_to`, `is_online`, `set_transfer_sink`, `peer_label`, `deliver_transfer_frame`, `notify_peer_gone` on `NetworkService` |
| `crates/rocket-app/src/network/connections.rs` | modify | `run_socket` passes the six frames to the sink, counts each as a pong, and skips the block-list read for `Chunk` and `ChunkAck`; `run_device` (after every `run_socket`) and `remove_local` call `notify_peer_gone` |
| `crates/rocket-app/src/network/transfer.rs` | create | `PeerSender`, `Timing`, `TransferDeps`, `TransferService` (`new`, `new_with_timing`, `#[cfg(test)]` hooks), `outcome_name`, `Inner` (the `Out` and `In` maps and one `TransferLimits`), queue of frames and peer-gone notices, worker, `tell_later`, helpers |
| `crates/rocket-app/src/network/transfer_send.rs` | create | Sender side around `OutgoingTransfer`: `SendStarted`, `send`, `export_notes`, `offer_bytes`, `offer_raw`, answers, acks, streaming in the ack window, result wait, expiry, cancel, peer gone |
| `crates/rocket-app/src/network/transfer_receive.rs` | create | Receiver side around `IncomingTransfer`: offers, admission, `respond`, chunks, the ack pump, spawned finalize with size and hash check, `TransferResult`, receiver audit, `release`, idle drop, peer gone |
| `crates/rocket-app/src/network/mod.rs` | modify | Module list, test modules, re-exports |
| `crates/rocket-app/src/lib.rs` | modify | Re-export `PeerSender`, `SendStarted`, `Timing`, `TransferDeps`, `TransferService` |
| `crates/rocket-app/src/network/transfer_fakes.rs` | create (test) | Fake peers, staging (with `release`, a finish gate and a wrong-length switch), importer (with an import gate), events, rig, payload helpers, `Link` (relays frames between two rigs) |
| `crates/rocket-app/src/network/transfer_link_tests.rs` | create (test) | Task 1 tests (16) |
| `crates/rocket-app/src/network/transfer_send_tests.rs` | create (test) | Task 2 tests (46) |
| `crates/rocket-app/src/network/transfer_receive_tests.rs` | create (test) | Task 3 tests (37) |
| `crates/rocket-app/CLAUDE.md` | modify | Two rows in the Public Types table |

Per-task checks: `cargo check -j4 -p rocket-app`, then the module filter of the task (`transfer_link_tests`, `transfer_send_tests`, `transfer_receive_tests`). Never the bare word `transfer`.

---

### Task 1: NetworkService send_to, sink hook, peer_label and peer gone

**Files:**
- Modify: `crates/rocket-app/src/network/service.rs`, `crates/rocket-app/src/network/connections.rs`, `crates/rocket-app/src/network/mod.rs`
- Create: `crates/rocket-app/src/network/transfer_link.rs`, `crates/rocket-app/src/network/transfer_link_tests.rs`

**Interfaces:**

```rust
// Consumes (Plan 01)
rocket_network::{TransferSink, PeerMessage, ResultOutcome, AbortReason, DeviceId}
// TransferSink::{on_frame, on_peer_gone}
// PeerMessage::{CollectionOffer, OfferAnswer, Chunk, TransferAbort, ChunkAck, TransferResult}

// Produces
impl NetworkService {
    pub async fn send_to(&self, device: &DeviceId, message: PeerMessage) -> DomainResult<()>;
    pub fn is_online(&self, device: &DeviceId) -> bool;                          // a live connection that can take a frame
    pub fn set_transfer_sink(&self, sink: Arc<dyn TransferSink>);
    pub fn peer_label(&self, device: &DeviceId) -> Option<(String, String)>;   // (display name, short id)
    pub(super) fn deliver_transfer_frame(&self, from: &DeviceId, message: PeerMessage);
    pub(super) fn notify_peer_gone(&self, device: &DeviceId);                   // TransferSink::on_peer_gone
}
```

Design notes:

- `send_to` copies the outbound sender out of the connection entry, drops the lock, then sends with a 2 s timeout (the pattern of `unpair_by_user`). No entry, no outbound sender (offline) or a closed channel gives `InvalidInput("That node is offline.")`. A timeout gives `Io`.
- `peer_label` returns `None` for a device that is not paired or is blocked, so the transfer service needs no second check. The name comes from `known_name` (live name, else the name saved at pairing).
- The sink is stored apart from `State`, because `prepare()` resets `State` on every start and the sink must survive that.
- In `run_socket` the new arm goes after the blocked-device arm, so an offer, answer, abort or result from a blocked device removes the connection first and never reaches the sink. The blocked arm's guard becomes `needs_block_check(message) && self.is_blocked(id)`: `Chunk` and `ChunkAck` skip the YAML read (ruling 21), every other frame keeps it.
- The new arm calls `self.with_conn(id, |c| c.presence.on_pong())` before it hands the frame on, so a peer that streams chunks (or acks) is alive even when its pongs wait behind the chunks (ruling 21).
- `is_online` is true when the connection entry has an outbound sender (it is cleared when the socket closes).
- Peer gone: `run_device` calls `notify_peer_gone` right after `run_socket` returns, whatever the `SocketEnd` (closed, stopped, removed, unpaired), and `remove_local` calls it after it dropped the connection entry (unpair by the user, by the peer, block). One end can report twice (the peer's `Unpair` runs `remove_local` and then ends the socket); the transfer service treats a gone device with no open transfer as a no-op. A socket replaced by a winning duplicate inside `run_socket` is not an end. No lock is held while the sink runs.

- [ ] **Step 1: Check the Plan 01 and Plan 02 names this plan uses**

Run:
```bash
cd /home/numericlabs/data/rocket/rocket
grep -n "TransferSink\|AbortReason\|TransferEvent\|TransferId\|chunk_count_for\|Direction\|Outcome" crates/rocket-network/src/lib.rs
grep -n "enum TransferEvent" -A22 crates/rocket-network/src/events.rs
grep -n "derive" -A1 crates/rocket-network/src/transfer.rs | grep -B1 "enum"
grep -n "CollectionOffer" crates/rocket-app/src/network/connections.rs
ls crates/rocket-app/src/network/transfer_ports.rs
grep -n "transfer_ports" crates/rocket-app/src/network/mod.rs
grep -n "fn on_peer_gone\|fn on_result\|fn awaiting_result\|fn with_offer_ttl\|fn fail_with\|ResultOutcome" crates/rocket-network/src/transfer.rs
grep -n "SENDER_RESULT_TIMEOUT" crates/rocket-network/src/limits.rs
grep -n "TransferResult\|ChunkAck" crates/rocket-network/src/messages.rs crates/rocket-app/src/network/connections.rs
```
Expected: the first command lists the re-exports of `TransferSink`, `AbortReason`, `ResultOutcome`, `TransferEvent`, `TransferId`, `chunk_count_for`, `Direction`, `Outcome` at the `rocket_network` crate root. If one lives only under a module (for example `rocket_network::transfer::Direction`), use that path in every `use` of this plan. The second command shows `OfferIncoming`, `Progress` and `Finished` with the fields from the index (`String` ids and names, `u64` sizes, `Direction`, `Outcome`) and `message: Option<String>` on `Finished`. The derive listing shows `Debug, Clone, Copy, PartialEq, Eq` on `Direction`, `Outcome` and `AbortReason` (the service copies them); `TransferEvent` must also be `Clone`. If a derive is missing, add it in the Plan 01 file (a one-line edit): the tests compare these values. `connections.rs` has one arm for the six new variants (the Plan 01 placeholder, including `ChunkAck` and `TransferResult`). `transfer_ports.rs` exists and `mod.rs` declares `transfer_ports`. The last three commands list `on_peer_gone`, `on_result`, `awaiting_result`, `with_offer_ttl` and `fail_with` (both machines) in `transfer.rs`, `SENDER_RESULT_TIMEOUT` in `limits.rs` and the `TransferResult` variant. If one is missing, stop and fix Plan 01 first.

- [ ] **Step 2: Write the failing tests**

Create `crates/rocket-app/src/network/transfer_link_tests.rs`:

```rust
//! Tests for `send_to`, the transfer sink hook and `peer_label`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rocket_network::limits::{PING_INTERVAL, PROTOCOL_VERSION};
use rocket_network::{AbortReason, DeviceId, PeerMessage, ResultOutcome, TransferSink};
use rocket_shared::error::DomainError;

use super::test_support::*;
use super::{NetworkHandle, NodeStatus};

#[derive(Default)]
struct RecordingSink {
    frames: Mutex<Vec<(DeviceId, PeerMessage)>>,
    gone: Mutex<Vec<DeviceId>>,
}

impl RecordingSink {
    fn frames(&self) -> Vec<(DeviceId, PeerMessage)> {
        self.frames.lock().expect("sink lock").clone()
    }

    fn gone(&self) -> Vec<DeviceId> {
        self.gone.lock().expect("sink lock").clone()
    }

    /// The sink was told at least once, and only about `device`.
    fn gone_only(&self, device: &DeviceId) -> bool {
        let gone = self.gone();
        !gone.is_empty() && gone.iter().all(|d| d == device)
    }
}

impl TransferSink for RecordingSink {
    fn on_frame(&self, from: DeviceId, message: PeerMessage) {
        self.frames.lock().expect("sink lock").push((from, message));
    }

    fn on_peer_gone(&self, device: DeviceId) {
        self.gone.lock().expect("sink lock").push(device);
    }
}

/// A node whose only paired device is `peer_seed`, reachable at its fixed route.
async fn solo(peer_seed: u8) -> (Node, NetworkHandle) {
    let a = node(1, "ann");
    a.paired.seed(identity(peer_seed).device_id, "Peer");
    a.connector
        .add_route(addr(peer_seed), Route::fixed(identity(peer_seed), "Peer"));
    let handle = a.svc.start().await;
    (a, handle)
}

/// Queues a scripted socket and adds the manual address, so the node dials and gets it.
async fn go_online(a: &Node, peer_seed: u8) -> Remote {
    let (socket, remote) = scripted_socket(identity(peer_seed), a.id(), PROTOCOL_VERSION);
    a.connector.push_socket(socket);
    a.svc
        .add_address("127.0.0.1", addr(peer_seed).port())
        .await
        .expect("add address");
    settle().await;
    remote
}

fn peer() -> DeviceId {
    identity(2).device_id
}

fn transfer_id() -> String {
    "0".repeat(32)
}

/// One frame of each of the six transfer variants (the chunk is at index 2, the abort at 3,
/// the ack at 5).
fn frames() -> Vec<PeerMessage> {
    vec![
        PeerMessage::CollectionOffer {
            transfer_id: transfer_id(),
            name: "Orders".into(),
            size_bytes: 10,
            chunk_count: 1,
            sha256: "0".repeat(64),
        },
        PeerMessage::OfferAnswer {
            transfer_id: transfer_id(),
            accept: true,
        },
        PeerMessage::Chunk {
            transfer_id: transfer_id(),
            seq: 0,
            data: "AAAA".into(),
        },
        PeerMessage::TransferAbort {
            transfer_id: transfer_id(),
            reason: AbortReason::Cancelled,
        },
        PeerMessage::TransferResult {
            transfer_id: transfer_id(),
            outcome: ResultOutcome::Completed,
        },
        PeerMessage::ChunkAck {
            transfer_id: transfer_id(),
            next_seq: 16,
        },
    ]
}

#[tokio::test(start_paused = true)]
async fn the_six_transfer_frames_reach_the_sink_with_the_sender_id() {
    let (a, _h) = solo(2).await;
    let sink = Arc::new(RecordingSink::default());
    a.svc.set_transfer_sink(sink.clone());
    let remote = go_online(&a, 2).await;

    for frame in frames() {
        remote.to_service.send(frame).await.expect("frame sent");
    }
    settle().await;

    let got = sink.frames();
    assert!(got.iter().all(|(from, _)| *from == peer()));
    let messages: Vec<PeerMessage> = got.into_iter().map(|(_, m)| m).collect();
    assert_eq!(messages, frames(), "all six arrive, in order");
}

#[tokio::test(start_paused = true)]
async fn a_frame_from_a_blocked_device_never_reaches_the_sink() {
    let (a, _h) = solo(2).await;
    let sink = Arc::new(RecordingSink::default());
    a.svc.set_transfer_sink(sink.clone());
    let remote = go_online(&a, 2).await;
    assert_eq!(row_status(&a, &peer()), Some(NodeStatus::Online));

    a.blocked.put(peer(), "Peer");
    // The offer is checked against the block list (chunks and acks are not, see below).
    remote
        .to_service
        .send(frames().remove(0))
        .await
        .expect("frame sent");
    settle().await;

    assert!(sink.frames().is_empty(), "the sink saw nothing");
    assert_eq!(
        row_status(&a, &peer()),
        Some(NodeStatus::Offline),
        "the connection was removed first"
    );
    let err = a
        .svc
        .send_to(&peer(), frames().remove(3))
        .await
        .expect_err("no connection is left");
    assert!(matches!(err, DomainError::InvalidInput(_)));
}

#[tokio::test(start_paused = true)]
async fn frames_without_a_sink_are_dropped_quietly() {
    let (a, _h) = solo(2).await;
    let remote = go_online(&a, 2).await;

    remote
        .to_service
        .send(frames().remove(0))
        .await
        .expect("frame sent");
    settle().await;

    assert_eq!(row_status(&a, &peer()), Some(NodeStatus::Online));
}

#[tokio::test(start_paused = true)]
async fn send_to_an_offline_peer_is_refused() {
    let (a, _h) = solo(2).await;

    let err = a
        .svc
        .send_to(&peer(), frames().remove(3))
        .await
        .expect_err("offline");

    assert!(matches!(&err, DomainError::InvalidInput(m) if m == "That node is offline."));
}

#[tokio::test(start_paused = true)]
async fn send_to_an_online_peer_delivers_the_frame() {
    let (a, _h) = solo(2).await;
    let mut remote = go_online(&a, 2).await;
    let message = frames().remove(1);

    a.svc
        .send_to(&peer(), message.clone())
        .await
        .expect("delivered");

    assert_eq!(remote.from_service.try_recv(), Ok(message));
}

#[tokio::test(start_paused = true)]
async fn send_to_gives_up_when_the_peer_does_not_read() {
    let (a, _h) = solo(2).await;
    // The remote end is kept but never read, so the 16-slot channel fills up.
    let _remote = go_online(&a, 2).await;
    for _ in 0..16 {
        a.svc
            .send_to(&peer(), frames().remove(1))
            .await
            .expect("room left");
    }

    let started = tokio::time::Instant::now();
    let err = a
        .svc
        .send_to(&peer(), frames().remove(1))
        .await
        .expect_err("channel full");

    assert!(matches!(err, DomainError::Io(_)));
    assert!(started.elapsed() >= Duration::from_secs(2));
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test(start_paused = true)]
async fn send_to_a_closed_socket_reads_as_offline() {
    let (a, _h) = solo(2).await;
    let remote = go_online(&a, 2).await;
    drop(remote);
    settle().await;

    let err = a
        .svc
        .send_to(&peer(), frames().remove(1))
        .await
        .expect_err("closed");

    assert!(matches!(&err, DomainError::InvalidInput(m) if m == "That node is offline."));
}

#[tokio::test(start_paused = true)]
async fn peer_label_names_a_paired_unblocked_device_only() {
    let (a, _h) = solo(2).await;

    assert_eq!(
        a.svc.peer_label(&peer()),
        Some(("Peer".to_string(), peer().short()))
    );
    assert_eq!(a.svc.peer_label(&identity(9).device_id), None, "not paired");

    a.blocked.put(peer(), "Peer");
    assert_eq!(a.svc.peer_label(&peer()), None, "blocked");
}

#[tokio::test(start_paused = true)]
async fn a_socket_that_ends_reports_the_peer_gone() {
    let (a, _h) = solo(2).await;
    let sink = Arc::new(RecordingSink::default());
    a.svc.set_transfer_sink(sink.clone());
    let remote = go_online(&a, 2).await;
    assert!(sink.gone().is_empty(), "a live socket is not gone");

    drop(remote);
    settle().await;

    assert!(sink.gone_only(&peer()), "the sink was told about the peer");
}

#[tokio::test(start_paused = true)]
async fn a_peer_unpair_reports_the_peer_gone() {
    let (a, _h) = solo(2).await;
    let sink = Arc::new(RecordingSink::default());
    a.svc.set_transfer_sink(sink.clone());
    let remote = go_online(&a, 2).await;

    remote
        .to_service
        .send(PeerMessage::Unpair)
        .await
        .expect("frame sent");
    settle().await;

    assert!(sink.gone_only(&peer()));
}

#[tokio::test(start_paused = true)]
async fn removing_the_pairing_reports_the_peer_gone() {
    let (a, _h) = solo(2).await;
    let sink = Arc::new(RecordingSink::default());
    a.svc.set_transfer_sink(sink.clone());
    let _remote = go_online(&a, 2).await;

    a.svc.remove_paired(&peer()).await.expect("removed");
    settle().await;

    assert!(sink.gone_only(&peer()));
}

#[tokio::test(start_paused = true)]
async fn removing_an_offline_pairing_reports_the_peer_gone() {
    let (a, _h) = solo(2).await;
    let sink = Arc::new(RecordingSink::default());
    a.svc.set_transfer_sink(sink.clone());

    a.svc.remove_paired(&peer()).await.expect("removed");
    settle().await;

    assert!(sink.gone_only(&peer()), "no socket was open, the pairing went");
}

#[tokio::test(start_paused = true)]
async fn transfer_frames_keep_a_peer_online_without_pongs() {
    let (a, _h) = solo(2).await;
    let sink = Arc::new(RecordingSink::default());
    a.svc.set_transfer_sink(sink.clone());
    let remote = go_online(&a, 2).await;

    // The remote never answers a ping. Three unanswered pings would mark it offline, but a
    // chunk after every ping counts as the answer.
    for _ in 0..4 {
        tokio::time::advance(PING_INTERVAL).await;
        settle().await;
        remote
            .to_service
            .send(frames().remove(2))
            .await
            .expect("frame sent");
        settle().await;
    }

    assert_eq!(row_status(&a, &peer()), Some(NodeStatus::Online));
    assert_eq!(sink.frames().len(), 4);
}

#[tokio::test(start_paused = true)]
async fn chunk_and_ack_frames_skip_the_block_list_read() {
    let (a, _h) = solo(2).await;
    let sink = Arc::new(RecordingSink::default());
    a.svc.set_transfer_sink(sink.clone());
    let remote = go_online(&a, 2).await;
    // Only the list changes here, so the socket stays open. A real block also unpairs and
    // closes the socket, which is the next test.
    a.blocked.put(peer(), "Peer");

    let all = frames();
    for frame in [all[2].clone(), all[5].clone()] {
        remote.to_service.send(frame).await.expect("frame sent");
    }
    settle().await;
    assert_eq!(sink.frames().len(), 2, "the chunk and the ack were not checked");
    assert_eq!(row_status(&a, &peer()), Some(NodeStatus::Online));

    remote
        .to_service
        .send(all[3].clone())
        .await
        .expect("frame sent");
    settle().await;
    assert_eq!(sink.frames().len(), 2, "the abort was checked and refused");
    assert_eq!(row_status(&a, &peer()), Some(NodeStatus::Offline));
}

#[tokio::test(start_paused = true)]
async fn blocking_a_paired_device_reports_the_peer_gone() {
    let (a, _h) = solo(2).await;
    let sink = Arc::new(RecordingSink::default());
    a.svc.set_transfer_sink(sink.clone());
    let _remote = go_online(&a, 2).await;

    // `block_peer` is `pub(super)` in `pairing.rs`, so this test module can call it. If
    // `identity` does not return a `PeerIdentity`, build one the way `pairing_tests.rs` does.
    a.svc.block_peer(&identity(2), "Peer").await;
    settle().await;

    assert!(sink.gone_only(&peer()), "a block ends the open transfers at once");
}

#[tokio::test(start_paused = true)]
async fn is_online_follows_the_connection() {
    let (a, _h) = solo(2).await;
    assert!(!a.svc.is_online(&peer()), "not connected yet");
    assert!(!a.svc.is_online(&identity(9).device_id), "unknown device");

    let remote = go_online(&a, 2).await;
    assert!(a.svc.is_online(&peer()));

    drop(remote);
    settle().await;
    assert!(!a.svc.is_online(&peer()), "the socket closed");
}
```

In `crates/rocket-app/src/network/mod.rs`, add the test module next to the other test modules:

```rust
#[cfg(test)]
#[path = "transfer_link_tests.rs"]
mod transfer_link_tests;
```

- [ ] **Step 3: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket-app transfer_link_tests`
Expected: FAIL to compile, `error[E0599]: no method named `set_transfer_sink` found for struct `Arc<NetworkService>`` (also `send_to`, `is_online` and `peer_label`). Once those exist (after Step 5) and before Step 6, `the_six_transfer_frames_reach_the_sink_with_the_sender_id`, `transfer_frames_keep_a_peer_online_without_pongs` and `chunk_and_ack_frames_skip_the_block_list_read` FAIL (the Plan 1 placeholder arm drops the frames, nothing counts as a pong and the blocked arm runs for every frame).

- [ ] **Step 4: Add the sink field**

In `crates/rocket-app/src/network/service.rs`, add the import `use rocket_network::TransferSink;` to the existing `rocket_network` import list (keep the list sorted). In `pub struct NetworkService`, after `paired_lock`, add:

```rust
    /// Receives the transfer frames. It lives outside `State` because `prepare` resets `State`
    /// on every start. The cycle with the transfer service lasts as long as the app.
    pub(super) transfer_sink: Mutex<Option<Arc<dyn TransferSink>>>,
```

In `NetworkService::new`, inside the `Arc::new_cyclic` literal after `paired_lock: Mutex::new(()),` add:

```rust
            transfer_sink: Mutex::new(None),
```

- [ ] **Step 5: Write `transfer_link.rs`**

Create `crates/rocket-app/src/network/transfer_link.rs`:

```rust
//! The transfer hooks of `NetworkService`: a bounded send to one device, the label of a paired
//! device, and the sink that receives the transfer frames.

use std::sync::Arc;
use std::time::Duration;

use rocket_network::{DeviceId, PeerMessage, TransferSink};
use rocket_shared::error::{DomainError, DomainResult};

use super::service::NetworkService;

/// How long a frame may wait for room on the connection.
const SEND_TIMEOUT: Duration = Duration::from_secs(2);

fn offline_error() -> DomainError {
    DomainError::InvalidInput("That node is offline.".to_string())
}

impl NetworkService {
    /// Sends one frame on the live connection of a paired device. The wait for room is bounded.
    pub async fn send_to(&self, device: &DeviceId, message: PeerMessage) -> DomainResult<()> {
        let outbound = self
            .lock()
            .conns
            .get(device)
            .and_then(|c| c.outbound.clone());
        let Some(outbound) = outbound else {
            return Err(offline_error());
        };
        match tokio::time::timeout(SEND_TIMEOUT, outbound.send(message)).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(offline_error()),
            Err(_) => Err(DomainError::Io(
                "The node did not take the message in time.".to_string(),
            )),
        }
    }

    /// True when the device has a live connection that can take a frame.
    pub fn is_online(&self, device: &DeviceId) -> bool {
        self.lock()
            .conns
            .get(device)
            .is_some_and(|c| c.outbound.is_some())
    }

    /// Sets the receiver of the transfer frames. A second call replaces the first.
    pub fn set_transfer_sink(&self, sink: Arc<dyn TransferSink>) {
        *self
            .transfer_sink
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(sink);
    }

    /// The display name and short id of a paired device. A device that is not paired, or is
    /// blocked, has no label.
    pub fn peer_label(&self, device: &DeviceId) -> Option<(String, String)> {
        if !self.paired_and_not_blocked(device) {
            return None;
        }
        Some((self.known_name(device), device.short()))
    }

    /// Hands a transfer frame to the sink. Without a sink the frame is dropped.
    pub(super) fn deliver_transfer_frame(&self, from: &DeviceId, message: PeerMessage) {
        let sink = self
            .transfer_sink
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(sink) = sink {
            sink.on_frame(*from, message);
        }
    }

    /// Tells the sink that the connection to a device ended or its pairing was removed.
    /// Without a sink nothing happens. No lock is held while the sink runs.
    pub(super) fn notify_peer_gone(&self, device: &DeviceId) {
        let sink = self
            .transfer_sink
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(sink) = sink {
            sink.on_peer_gone(*device);
        }
    }
}
```

- [ ] **Step 6: Pass the frames to the sink and report a gone peer**

In `crates/rocket-app/src/network/connections.rs`, in `run_socket`, change the guard of the blocked arm from `Some(_) if self.is_blocked(id) => {` to:

```rust
                    Some(ref message) if needs_block_check(message) && self.is_blocked(id) => {
```

and add this free function next to `version_matches`:

```rust
/// Chunks and acks belong to a transfer that an offer or an answer opened after this check
/// passed. A block closes the socket, which ends the transfer, so they skip the file read.
fn needs_block_check(message: &PeerMessage) -> bool {
    !matches!(
        message,
        PeerMessage::Chunk { .. } | PeerMessage::ChunkAck { .. }
    )
}
```

Then find the arm Plan 1 added for the new variants (`grep -n "CollectionOffer" crates/rocket-app/src/network/connections.rs`). Replace that arm with the one below. It must stay after the blocked arm.

```rust
                    // The transfer service owns these frames. Any of them shows the peer is
                    // alive, so a long stream cannot starve the ping rule.
                    Some(
                        message @ (PeerMessage::CollectionOffer { .. }
                        | PeerMessage::OfferAnswer { .. }
                        | PeerMessage::Chunk { .. }
                        | PeerMessage::TransferAbort { .. }
                        | PeerMessage::ChunkAck { .. }
                        | PeerMessage::TransferResult { .. }),
                    ) => {
                        self.with_conn(id, |c| c.presence.on_pong());
                        self.deliver_transfer_frame(id, message);
                    }
```

In `run_device`, replace the call `match self.run_socket(&id, socket, &mut inbox, &mut stop).await {` with the two statements below, and keep the arms of the `match` as they are:

```rust
                let end = self.run_socket(&id, socket, &mut inbox, &mut stop).await;
                // Whatever the end, open transfers with this device must not wait for a timer.
                self.notify_peer_gone(&id);
                match end {
```

In `remove_local`, right after `drop(entry);` add:

```rust
        // The pairing and the connection are gone, so are the open transfers.
        self.notify_peer_gone(id);
```

In `mod.rs`, add `mod transfer_link;` to the module list (after `mod status;`, keep it before `transfer_ports` if that is declared).
- [ ] **Step 7: Run the tests**

Run: `cargo test -j4 -p rocket-app transfer_link_tests`
Expected: PASS, the 16 tests in `network::transfer_link_tests` pass, 0 failed. If `transfer_frames_keep_a_peer_online_without_pongs` sees `Offline`, the new arm does not call `on_pong`. If `chunk_and_ack_frames_skip_the_block_list_read` sees 0 frames, the guard still reads the list for chunks. If `send_to_gives_up_when_the_peer_does_not_read` fails with `Ok`, the scripted channel is larger than 16: read `scripted_socket` in `test_support.rs` and use its capacity. If `a_socket_that_ends_reports_the_peer_gone` sees no call, `notify_peer_gone` is missing after `run_socket` in `run_device`; if `removing_an_offline_pairing_reports_the_peer_gone` sees none, it is missing in `remove_local`.

- [ ] **Step 8: Check the existing network tests**

Run: `cargo check -j4 -p rocket-app` then `cargo test -j4 -p rocket-app network`
Expected: Finished with no error, then PASS with 0 failed (every earlier network test still passes).

---

### Task 2: TransferService, sender side

**Files:**
- Create: `crates/rocket-app/src/network/transfer.rs`, `crates/rocket-app/src/network/transfer_send.rs`, `crates/rocket-app/src/network/transfer_fakes.rs`, `crates/rocket-app/src/network/transfer_send_tests.rs`
- Modify: `crates/rocket-app/src/network/mod.rs`, `crates/rocket-app/src/lib.rs`

**Interfaces:**

```rust
// Consumes (Plan 01, 02, Task 1)
CollectionExportService::export_bruno(&self, collection: &str, include_environments: bool) -> DomainResult<ExportOutput>   // ExportOutput { bytes, file_name, report: ExportReport }
rocket_export::{ExportReport, ExportNote, ExportAction}
rocket_network::{TransferId, TransferLimits, OutgoingTransfer, OutgoingState, OfferError, TransferEvent, Direction, Outcome, AbortReason, ResultOutcome, TransferSink, PeerMessage, offer_name_is_valid}
OutgoingTransfer::{new, with_offer_ttl, offer_message, abort_message, expire_if_due, on_answer, next_chunk, on_abort, on_result, cancel, fail_with, connection_lost, result_timed_out, window_open, on_ack, state, outcome, awaiting_result, done_bytes, total_bytes}
TransferLimits::{begin_outgoing, release}
rocket_network::limits::{CHUNK_BYTES, MAX_TRANSFER_BYTES, TRANSFER_OFFER_TTL, SENDER_RESULT_TIMEOUT, MAX_FRAME_BYTES, MAX_UNACKED_CHUNKS}
AuditEventKind::NetworkCollectionSent { device_id, collection, outcome }   // Plan 1, outcome is the wire name
super::transfer_ports::{CollectionImportPort, TransferStaging, UNSAFE_ZIP_MESSAGE}   // TransferStaging::release is used in Task 3

// Produces
#[async_trait]
pub trait PeerSender: Send + Sync {
    async fn send_to(&self, device: &DeviceId, message: PeerMessage) -> DomainResult<()>;
    fn peer_label(&self, device: &DeviceId) -> Option<(String, String)>;
    fn is_online(&self, device: &DeviceId) -> bool;
    fn set_transfer_sink(&self, sink: Arc<dyn TransferSink>);
}   // NetworkService implements it
#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct Timing { pub offer_ttl: Duration, pub idle_limit: Duration, pub result_timeout: Duration }   // Default: 120 s, 120 s, SENDER_RESULT_TIMEOUT
pub struct TransferDeps { pub network: Arc<dyn PeerSender>, pub export: Arc<CollectionExportService>, pub importer: Arc<dyn CollectionImportPort>, pub staging: Arc<dyn TransferStaging>, pub events: Arc<dyn EventPublisher>, pub audit: Arc<dyn SecurityAuditPublisher> }
pub struct SendStarted { pub transfer_id: String, pub notes: Vec<String> }
impl TransferService {
    pub fn new(deps: TransferDeps) -> Arc<TransferService>;       // registers itself as the sink; new_with_timing(deps, Timing::default())
    #[doc(hidden)] pub fn new_with_timing(deps: TransferDeps, timing: Timing) -> Arc<TransferService>;
    pub async fn send(&self, device: &DeviceId, collection: &str, include_environments: bool) -> DomainResult<SendStarted>;
    pub async fn cancel(&self, transfer_id: &str) -> DomainResult<()>;
    // respond() arrives in Task 3
    // Hooks for Plan 4 (ruling 17), test builds only
    #[cfg(test)] pub(crate) async fn offer_raw(&self, device: &DeviceId, name: &str, bytes: Vec<u8>) -> DomainResult<String>;
    #[cfg(test)] pub(crate) fn set_chunk_delay(&self, delay: Duration);
    #[cfg(test)] pub(crate) fn set_tamper(&self, seq: Option<u32>);
    pub(super) fn on_ack(&self, from: &DeviceId, id: &TransferId, next_seq: u32);
    pub(super) fn claim_send(&self, device: &DeviceId) -> DomainResult<TransferId>;
    pub(super) async fn offer_bytes(&self, device: &DeviceId, id: &TransferId, collection: &str, zip: Vec<u8>) -> DomainResult<()>;
}
impl TransferSink for TransferService { fn on_frame(&self, from: DeviceId, message: PeerMessage); fn on_peer_gone(&self, device: DeviceId); }
pub(super) fn outcome_name(outcome: Outcome) -> &'static str;   // the Outcome wire name, for the audit `outcome` field
```
Design notes:

- `send` order (ruling 14): blank name refused, peer must be paired (`peer_label`), peer must be online (`is_online`), claim the peer's outgoing slot (`claim_send`, `TransferLimits::begin_outgoing`), export in `spawn_blocking` (also the empty check and the notes), then `offer_bytes`: hash in `spawn_blocking`, build the `OutgoingTransfer` (it checks size, name, hash and chunk count) and give it `timing.offer_ttl`, register the entry, send `CollectionOffer` (`machine.offer_message()`), start the offer timer. Any error before the entry exists releases the slot. A failed offer send removes the entry again and returns the error.
- The machine owns the rules. The service calls `on_answer`, `next_chunk`, `on_abort`, `on_result`, `cancel`, `fail_with`, `expire_if_due` and reads `state`, `outcome`, `awaiting_result`, `last_chunk_built`, `done_bytes` and `total_bytes`. It does not look at sizes, sequence numbers or the clock itself, except to pass `TransferService::now()`.
- After the last chunk the machine is `Done` and `last_chunk_built()`; it is `awaiting_result()` only once the receiver's final `ChunkAck` arrived. The entry stays until the receiver's `TransferResult` arrives (`on_result`), the peer aborts (`fail_with`, so a receiver `failed` ends it `Failed`), the peer is gone (`connection_lost()`: `Unconfirmed` after the final ack, `Failed` before it), the idle limit passes with no final ack (`Failed`, the peer is told `failed`), or `timing.result_timeout` passes after the final ack (`result_timed_out()`, `Unconfirmed`). The single rule that keeps `Finished` unique: only the code that removes the entry from `outgoing` emits `Finished`, and the outcome it reports is `machine.outcome()`.
- Streaming runs in its own task. Each loop sleeps the chunk-delay hook (test builds only), checks `window_open()` under the lock (closed: wait for `ack_signal` or the idle deadline, ruling 19), takes the next slice from the ZIP (a copy with one flipped byte when the tamper hook names this sequence number, test builds only), asks `next_chunk` for the frame (an `Err` means the transfer is not streaming any more, so the loop ends), sends it with `send_to` (bounded by the 2 s timeout) and emits `Progress` while the entry still exists. A send error ends the transfer as `Failed` through `fail_with`.
- Tests build the real `CollectionExportService` over an in-memory `CollectionFileSource`, so the offer carries a real ZIP size and hash. The 10 MiB boundary is a Plan 1 test. Here the refusal is proven over the real service with a 10.5 MiB incompressible `.js` file, and `offer_error` pins the wording. The notes are proven on a hand-built `ExportReport`, because the in-memory source cannot make the exporter skip or blank anything.
- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

- [ ] **Step 2: Check the Plan 01 and Plan 02 names this task uses**

Run:
```bash
cd /home/numericlabs/data/rocket/rocket
grep -n "pub fn \(new\|offer_message\|abort_message\|expire_if_due\|on_answer\|next_chunk\|on_abort\|on_result\|awaiting_result\|with_offer_ttl\|cancel\|connection_lost\|result_timed_out\|window_open\|on_ack\|fail_with\|state\|outcome\|done_bytes\|total_bytes\|is_active\|offer_name_is_valid\)" crates/rocket-network/src/transfer.rs
grep -n "ChunkAck\|MAX_UNACKED_CHUNKS\|ACK_EVERY_CHUNKS\|Unconfirmed" crates/rocket-network/src/messages.rs crates/rocket-network/src/limits.rs crates/rocket-network/src/transfer.rs
grep -n "NetworkCollectionSent\|NetworkCollectionReceived" -A4 crates/rocket-audit/src/event.rs
grep -n "impl From<\(ResultOutcome\|AbortReason\)> for Outcome" crates/rocket-network/src/transfer.rs
grep -n "pub fn \(admit_offer\|begin_outgoing\|mark_active\|release\)" crates/rocket-network/src/transfer.rs
grep -n "fn release\|fn purge\|fn discard" crates/rocket-network/src/staging.rs crates/rocket-app/src/network/transfer_ports.rs
```
Expected: the first command lists the sender methods once (`new`, `offer_message`, `abort_message`, `expire_if_due`, `on_answer`, `next_chunk`, `on_abort`, `on_result`, `awaiting_result`, `with_offer_ttl`, `cancel`, `connection_lost`, `result_timed_out`, `window_open`, `on_ack`, `state`, `outcome`, `done_bytes`, `total_bytes`, `is_active`), the receiver methods and the free function `offer_name_is_valid`; the second shows the `ChunkAck` variant, both ack constants and `Outcome::Unconfirmed`; the third shows an `outcome: String` field on both audit kinds; `fail_with` and `with_offer_ttl` must appear twice (receiver and sender). If a sender one is missing, stop and fix Plan 01. The second command shows both `From` impls on `Outcome`. The third command lists the four `TransferLimits` methods. The fourth shows `release` on `TransferStaging` (Plan 02). If it is missing, stop and add it to Plan 02 first (`fn release(&self, file: &StagedFile) -> DomainResult<()>`). Task 2 itself never calls `release`, but the fake staging in Step 3 implements it.

- [ ] **Step 3: Write the fakes**

Create `crates/rocket-app/src/network/transfer_fakes.rs`:

```rust
//! Fakes for the transfer service: peers, staging, importer, events and a rig that wires them.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use rocket_audit::publisher::RecordingSecurityAuditPublisher;
use rocket_audit::AuditEventKind;
use rocket_export::{CollectionFileSource, SourceFile};
use rocket_network::limits::CHUNK_BYTES;
use rocket_network::{
    chunk_count_for, AbortReason, DeviceId, Direction, Outcome, PeerMessage, ResultOutcome,
    TransferEvent, TransferSink,
};
use std::io::{Cursor, Read};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{EventPublisher, PublishedEvent};
use sha2::{Digest, Sha256};
use tokio::sync::Notify;

use super::transfer::{PeerSender, Timing, TransferDeps, TransferService};
use super::transfer_ports::{
    CollectionImportPort, ImportedCollection, StagedFile, StagedZip, TransferStaging,
    UNSAFE_ZIP_MESSAGE,
};
use crate::collection_export_service::CollectionExportService;

// ----- payloads -----

/// Bytes that do not compress, so a ZIP of them is about as big as they are.
pub(crate) fn noise(len: usize) -> Vec<u8> {
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

/// A valid transfer id (32 lowercase hex characters) for `n`.
pub(crate) fn tid(n: u8) -> String {
    format!("{n:032x}")
}

pub(crate) fn answer(id: &str, accept: bool) -> PeerMessage {
    PeerMessage::OfferAnswer {
        transfer_id: id.to_string(),
        accept,
    }
}

pub(crate) fn abort(id: &str, reason: AbortReason) -> PeerMessage {
    PeerMessage::TransferAbort {
        transfer_id: id.to_string(),
        reason,
    }
}

pub(crate) fn result(id: &str, outcome: ResultOutcome) -> PeerMessage {
    PeerMessage::TransferResult {
        transfer_id: id.to_string(),
        outcome,
    }
}

pub(crate) fn ack(id: &str, next_seq: u32) -> PeerMessage {
    PeerMessage::ChunkAck {
        transfer_id: id.to_string(),
        next_seq,
    }
}

/// The entry names of a ZIP held in memory.
pub(crate) fn zip_entry_names(bytes: &[u8]) -> Vec<String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("a zip");
    (0..archive.len())
        .map(|i| {
            let mut entry = archive.by_index(i).expect("an entry");
            let mut sink = Vec::new();
            entry.read_to_end(&mut sink).expect("readable");
            entry.name().to_string()
        })
        .collect()
}

/// The bytes a sender would stream, with helpers that build the matching frames.
#[derive(Clone)]
pub(crate) struct Payload {
    pub bytes: Vec<u8>,
}

impl Payload {
    pub fn new(len: usize) -> Payload {
        Payload { bytes: noise(len) }
    }

    pub fn sha(&self) -> String {
        format!("{:x}", Sha256::digest(&self.bytes))
    }

    pub fn count(&self) -> u32 {
        chunk_count_for(self.bytes.len() as u64)
    }

    pub fn offer(&self, id: &str, name: &str) -> PeerMessage {
        PeerMessage::CollectionOffer {
            transfer_id: id.to_string(),
            name: name.to_string(),
            size_bytes: self.bytes.len() as u64,
            chunk_count: self.count(),
            sha256: self.sha(),
        }
    }

    pub fn chunk(&self, id: &str, seq: u32) -> PeerMessage {
        let start = seq as usize * CHUNK_BYTES;
        let end = (start + CHUNK_BYTES).min(self.bytes.len());
        PeerMessage::Chunk {
            transfer_id: id.to_string(),
            seq,
            data: STANDARD.encode(&self.bytes[start..end]),
        }
    }
}

/// Polls `check` until it is true. Fails the test after about 5 seconds.
pub(crate) async fn wait_for(what: &str, mut check: impl FnMut() -> bool) {
    for _ in 0..500 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

// ----- peers -----

struct Hold {
    seq: u32,
    reached: Arc<Notify>,
    release: Arc<Notify>,
}

/// The network as the transfer service sees it: labels, online flags and a log of sent frames.
#[derive(Default)]
pub(crate) struct FakePeers {
    labels: Mutex<HashMap<DeviceId, String>>,
    online: Mutex<HashSet<DeviceId>>,
    sent: Mutex<Vec<(DeviceId, PeerMessage)>>,
    sink: Mutex<Option<Arc<dyn TransferSink>>>,
    hold: Mutex<Option<Hold>>,
}

impl FakePeers {
    /// A paired, online peer.
    pub fn add(&self, device: DeviceId, name: &str) {
        self.labels
            .lock()
            .expect("labels lock")
            .insert(device, name.to_string());
        self.online.lock().expect("online lock").insert(device);
    }

    pub fn set_online(&self, device: &DeviceId, online: bool) {
        let mut set = self.online.lock().expect("online lock");
        if online {
            set.insert(*device);
        } else {
            set.remove(device);
        }
    }

    /// The peer is no longer paired or is blocked.
    pub fn forget(&self, device: &DeviceId) {
        self.labels.lock().expect("labels lock").remove(device);
    }

    pub fn sent(&self) -> Vec<(DeviceId, PeerMessage)> {
        self.sent.lock().expect("sent lock").clone()
    }

    pub fn sent_to(&self, device: &DeviceId) -> Vec<PeerMessage> {
        self.sent()
            .into_iter()
            .filter(|(to, _)| to == device)
            .map(|(_, message)| message)
            .collect()
    }

    pub fn sink(&self) -> Option<Arc<dyn TransferSink>> {
        self.sink.lock().expect("sink lock").clone()
    }

    /// Parks the send of chunk `seq` until `release` is notified. `reached` fires when it parks.
    pub fn hold_chunk(&self, seq: u32) -> (Arc<Notify>, Arc<Notify>) {
        let reached = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        *self.hold.lock().expect("hold lock") = Some(Hold {
            seq,
            reached: reached.clone(),
            release: release.clone(),
        });
        (reached, release)
    }
}

#[async_trait]
impl PeerSender for FakePeers {
    async fn send_to(&self, device: &DeviceId, message: PeerMessage) -> DomainResult<()> {
        let hold = {
            let guard = self.hold.lock().expect("hold lock");
            match (&*guard, &message) {
                (Some(h), PeerMessage::Chunk { seq, .. }) if h.seq == *seq => {
                    Some((h.reached.clone(), h.release.clone()))
                }
                _ => None,
            }
        };
        if let Some((reached, release)) = hold {
            reached.notify_one();
            release.notified().await;
        }
        if !self.online.lock().expect("online lock").contains(device) {
            return Err(DomainError::InvalidInput("That node is offline.".into()));
        }
        self.sent.lock().expect("sent lock").push((*device, message));
        Ok(())
    }

    fn peer_label(&self, device: &DeviceId) -> Option<(String, String)> {
        self.labels
            .lock()
            .expect("labels lock")
            .get(device)
            .map(|name| (name.clone(), device.short()))
    }

    fn is_online(&self, device: &DeviceId) -> bool {
        self.online.lock().expect("online lock").contains(device)
    }

    fn set_transfer_sink(&self, sink: Arc<dyn TransferSink>) {
        *self.sink.lock().expect("sink lock") = Some(sink);
    }
}

// ----- events -----

#[derive(Default)]
pub(crate) struct RecordingTransferEvents(Mutex<Vec<TransferEvent>>);

impl RecordingTransferEvents {
    pub fn all(&self) -> Vec<TransferEvent> {
        self.0.lock().expect("events lock").clone()
    }

    /// `(transfer id, display name, short id, collection, size)` of every `OfferIncoming`.
    pub fn incoming(&self) -> Vec<(String, String, String, String, u64)> {
        self.all()
            .into_iter()
            .filter_map(|e| match e {
                TransferEvent::OfferIncoming {
                    transfer_id,
                    display_name,
                    short_id,
                    collection_name,
                    size_bytes,
                    ..
                } => Some((transfer_id, display_name, short_id, collection_name, size_bytes)),
                _ => None,
            })
            .collect()
    }

    pub fn progress(&self) -> Vec<(Direction, u64, u64)> {
        self.all()
            .into_iter()
            .filter_map(|e| match e {
                TransferEvent::Progress {
                    direction,
                    done_bytes,
                    total_bytes,
                    ..
                } => Some((direction, done_bytes, total_bytes)),
                _ => None,
            })
            .collect()
    }

    /// `(transfer id, direction, outcome)` of every `Finished`.
    pub fn finished(&self) -> Vec<(String, Direction, Outcome)> {
        self.all()
            .into_iter()
            .filter_map(|e| match e {
                TransferEvent::Finished {
                    transfer_id,
                    direction,
                    outcome,
                    ..
                } => Some((transfer_id, direction, outcome)),
                _ => None,
            })
            .collect()
    }

    /// The `message` of every `Finished`, in order.
    pub fn finished_messages(&self) -> Vec<Option<String>> {
        self.all()
            .into_iter()
            .filter_map(|e| match e {
                TransferEvent::Finished { message, .. } => Some(message),
                _ => None,
            })
            .collect()
    }
}

impl EventPublisher for RecordingTransferEvents {
    fn publish(&self, event: &dyn PublishedEvent) {
        if let Some(event) = event.downcast_ref::<TransferEvent>() {
            self.0.lock().expect("events lock").push(event.clone());
        }
    }
}

// ----- staging -----

#[derive(Default)]
pub(crate) struct StagingCounters {
    created: AtomicUsize,
    discarded: AtomicUsize,
    finished: AtomicUsize,
    released: AtomicUsize,
    purges: AtomicUsize,
}

impl StagingCounters {
    pub fn created(&self) -> usize {
        self.created.load(Ordering::SeqCst)
    }
    pub fn discarded(&self) -> usize {
        self.discarded.load(Ordering::SeqCst)
    }
    pub fn finished(&self) -> usize {
        self.finished.load(Ordering::SeqCst)
    }
    pub fn released(&self) -> usize {
        self.released.load(Ordering::SeqCst)
    }
    pub fn purges(&self) -> usize {
        self.purges.load(Ordering::SeqCst)
    }
}

#[derive(Default)]
pub(crate) struct FakeStaging {
    pub counters: Arc<StagingCounters>,
    pub fail_create: AtomicBool,
    pub fail_release: AtomicBool,
    /// The finished file reports one byte more than was appended, with the right hash.
    pub wrong_len: Arc<AtomicBool>,
    gate: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}

impl FakeStaging {
    /// The next created file waits inside `finish` until the returned sender sends (or drops).
    /// Call it before the transfer is accepted. `finished()` counts the call before it waits.
    pub fn hold_finish(&self) -> std::sync::mpsc::Sender<()> {
        let (release, gate) = std::sync::mpsc::channel();
        *self.gate.lock().expect("gate lock") = Some(gate);
        release
    }

    /// Every file that was created was discarded or finished, every finished file was
    /// released, and the service never purged (the purge belongs to start-up).
    pub fn assert_clean(&self) {
        let c = &self.counters;
        assert_eq!(
            c.created(),
            c.discarded() + c.finished(),
            "every staged file is discarded or finished"
        );
        assert_eq!(c.released(), c.finished(), "every finished file is released");
        assert_eq!(c.purges(), 0, "the service never purges");
    }
}

struct FakeStagedZip {
    id: String,
    bytes: Vec<u8>,
    counters: Arc<StagingCounters>,
    wrong_len: Arc<AtomicBool>,
    gate: Option<std::sync::mpsc::Receiver<()>>,
}

impl StagedZip for FakeStagedZip {
    fn append(&mut self, bytes: &[u8]) -> DomainResult<()> {
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    fn finish(self: Box<Self>) -> DomainResult<StagedFile> {
        self.counters.finished.fetch_add(1, Ordering::SeqCst);
        if let Some(gate) = &self.gate {
            // Runs in a blocking task, so waiting here does not stall the test runtime.
            let _ = gate.recv();
        }
        let extra = u64::from(self.wrong_len.load(Ordering::SeqCst));
        Ok(StagedFile {
            path: PathBuf::from(format!("/staged/{}.zip", self.id)),
            len: self.bytes.len() as u64 + extra,
            sha256: format!("{:x}", Sha256::digest(&self.bytes)),
        })
    }

    fn discard(self: Box<Self>) {
        self.counters.discarded.fetch_add(1, Ordering::SeqCst);
    }
}

impl TransferStaging for FakeStaging {
    fn create(&self, transfer_id: &str) -> DomainResult<Box<dyn StagedZip>> {
        if self.fail_create.load(Ordering::SeqCst) {
            return Err(DomainError::Io("disk full".into()));
        }
        self.counters.created.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(FakeStagedZip {
            id: transfer_id.to_string(),
            bytes: Vec::new(),
            counters: self.counters.clone(),
            wrong_len: self.wrong_len.clone(),
            gate: self.gate.lock().expect("gate lock").take(),
        }))
    }

    fn release(&self, _file: &StagedFile) -> DomainResult<()> {
        self.counters.released.fetch_add(1, Ordering::SeqCst);
        if self.fail_release.load(Ordering::SeqCst) {
            return Err(DomainError::Io("cannot delete".into()));
        }
        Ok(())
    }

    fn purge(&self) -> DomainResult<()> {
        self.counters.purges.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

// ----- importer -----

#[derive(Clone, Copy, Default)]
pub(crate) enum ImportMode {
    #[default]
    Accept,
    /// The strict validator refused the ZIP: `InvalidInput` with `UNSAFE_ZIP_MESSAGE`.
    Reject,
    /// Any other failure.
    Fail,
    /// `InvalidInput` with another text. It must not count as a rejection.
    OtherInvalid,
}

#[derive(Default)]
pub(crate) struct FakeImporter {
    pub existing: Mutex<Vec<String>>,
    pub mode: Mutex<ImportMode>,
    calls: Mutex<Vec<(PathBuf, String)>>,
    gate: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}

impl FakeImporter {
    /// The next import waits (after it was recorded in `calls`) until the sender sends or drops.
    pub fn hold_import(&self) -> std::sync::mpsc::Sender<()> {
        let (release, gate) = std::sync::mpsc::channel();
        *self.gate.lock().expect("gate lock") = Some(gate);
        release
    }

    pub fn set_mode(&self, mode: ImportMode) {
        *self.mode.lock().expect("mode lock") = mode;
    }

    pub fn calls(&self) -> Vec<(PathBuf, String)> {
        self.calls.lock().expect("calls lock").clone()
    }
}

impl CollectionImportPort for FakeImporter {
    fn existing_names(&self) -> DomainResult<Vec<String>> {
        Ok(self.existing.lock().expect("existing lock").clone())
    }

    fn import_zip(&self, zip_path: &Path, display_name: &str) -> DomainResult<ImportedCollection> {
        self.calls
            .lock()
            .expect("calls lock")
            .push((zip_path.to_path_buf(), display_name.to_string()));
        let gate = self.gate.lock().expect("gate lock").take();
        if let Some(gate) = gate {
            // Runs in a blocking task, so waiting here does not stall the test runtime.
            let _ = gate.recv();
        }
        let mode = *self.mode.lock().expect("mode lock");
        match mode {
            ImportMode::Accept => Ok(ImportedCollection {
                name: display_name.to_string(),
            }),
            ImportMode::Reject => Err(DomainError::InvalidInput(UNSAFE_ZIP_MESSAGE.into())),
            ImportMode::Fail => Err(DomainError::Internal("import failed".into())),
            ImportMode::OtherInvalid => Err(DomainError::InvalidInput("bad folder layout".into())),
        }
    }
}

// ----- the rig -----

struct FakeFiles {
    files: Vec<(String, Vec<u8>)>,
    calls: Arc<AtomicUsize>,
}

impl CollectionFileSource for FakeFiles {
    fn list_files(&self, _collection: &str) -> DomainResult<Vec<SourceFile>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self
            .files
            .iter()
            .map(|(path, bytes)| SourceFile {
                rel_path: path.clone(),
                bytes: bytes.clone(),
            })
            .collect())
    }
}
/// A small collection whose ZIP is about 100 KiB, so it needs four chunks.
pub(crate) fn default_files() -> Vec<(String, Vec<u8>)> {
    vec![
        (
            "opencollection.yml".to_string(),
            b"opencollection: \"1.0.0\"\ninfo:\n  name: orders\n".to_vec(),
        ),
        ("scripts/data.js".to_string(), noise(100 * 1024)),
    ]
}

pub(crate) struct Rig {
    pub svc: Arc<TransferService>,
    pub peers: Arc<FakePeers>,
    pub events: Arc<RecordingTransferEvents>,
    pub audit: Arc<RecordingSecurityAuditPublisher>,
    pub staging: Arc<FakeStaging>,
    pub importer: Arc<FakeImporter>,
    export_calls: Arc<AtomicUsize>,
}

impl Rig {
    pub fn audited(&self) -> Vec<AuditEventKind> {
        self.audit.records().into_iter().map(|r| r.kind).collect()
    }

    /// How often the export read the collection's files.
    pub fn exports(&self) -> usize {
        self.export_calls.load(Ordering::SeqCst)
    }
}

pub(crate) fn rig() -> Rig {
    rig_with(default_files())
}

pub(crate) fn rig_with(files: Vec<(String, Vec<u8>)>) -> Rig {
    rig_timed(files, Timing::default())
}

/// A rig whose service runs with the given waits (the test seam of index ruling 3).
pub(crate) fn rig_timed(files: Vec<(String, Vec<u8>)>, timing: Timing) -> Rig {
    let peers = Arc::new(FakePeers::default());
    let events = Arc::new(RecordingTransferEvents::default());
    let audit = Arc::new(RecordingSecurityAuditPublisher::new());
    let staging = Arc::new(FakeStaging::default());
    let importer = Arc::new(FakeImporter::default());
    let export_calls = Arc::new(AtomicUsize::new(0));
    let export = Arc::new(CollectionExportService::new(Box::new(FakeFiles {
        files,
        calls: export_calls.clone(),
    })));
    let svc = TransferService::new_with_timing(
        TransferDeps {
            network: peers.clone(),
            export,
            importer: importer.clone(),
            staging: staging.clone(),
            events: events.clone(),
            audit: audit.clone(),
        },
        timing,
    );
    Rig {
        svc,
        peers,
        events,
        audit,
        staging,
        importer,
        export_calls,
    }
}

// ----- two rigs joined -----

/// Two rigs joined by a relay: frames that `a` sends to `b_id` reach `b` as coming from
/// `a_id`, and back. `drop` decides which frames from `a` to `b` are lost on the way.
pub(crate) struct Link<'r> {
    pub a: &'r Rig,
    pub a_id: DeviceId,
    pub b: &'r Rig,
    pub b_id: DeviceId,
    a_seen: usize,
    b_seen: usize,
}

impl<'r> Link<'r> {
    /// `a` knows `b_id` as `b_name` and `b` knows `a_id` as `a_name`; both are paired and online.
    pub fn new(a: &'r Rig, a_id: DeviceId, a_name: &str, b: &'r Rig, b_id: DeviceId, b_name: &str) -> Link<'r> {
        a.peers.add(b_id, b_name);
        b.peers.add(a_id, a_name);
        Link { a, a_id, b, b_id, a_seen: 0, b_seen: 0 }
    }

    /// Moves every frame sent since the last call, both ways. Returns how many moved.
    pub fn pump(&mut self, drop: &dyn Fn(&PeerMessage) -> bool) -> usize {
        let from_a = self.a.peers.sent_to(&self.b_id);
        let from_b = self.b.peers.sent_to(&self.a_id);
        let mut moved = 0;
        for frame in from_a.iter().skip(self.a_seen) {
            if !drop(frame) {
                self.b.svc.on_frame(self.a_id, frame.clone());
            }
            moved += 1;
        }
        for frame in from_b.iter().skip(self.b_seen) {
            self.a.svc.on_frame(self.b_id, frame.clone());
            moved += 1;
        }
        self.a_seen = from_a.len();
        self.b_seen = from_b.len();
        moved
    }

    /// Pumps on the real clock until `done` holds. Fails the test after about 5 seconds.
    pub async fn run_until(&mut self, drop: &dyn Fn(&PeerMessage) -> bool, done: impl Fn() -> bool) {
        for _ in 0..500 {
            self.pump(drop);
            if done() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the linked transfer did not end");
    }
}
```

- [ ] **Step 4: Write the failing sender tests**

Create `crates/rocket-app/src/network/transfer_send_tests.rs`:

```rust
//! Sender-side tests for `TransferService`, with fake peers and a real export over memory files.
//!
//! The rules (offer shape, expiry, chunk order, limits) are tested in `rocket-network`. These
//! tests prove the wiring: a frame in, the right event or frame out, and one `Finished`.

use std::sync::Arc;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use rocket_audit::AuditEventKind;
use rocket_export::{ExportAction, ExportNote, ExportReport};
use rocket_network::limits::{
    MAX_FRAME_BYTES, MAX_TRANSFER_BYTES, MAX_UNACKED_CHUNKS, TRANSFER_OFFER_TTL,
};
use rocket_network::{
    chunk_count_for, AbortReason, DeviceId, Direction, OfferError, Outcome, PeerMessage,
    ResultOutcome, TransferEvent, TransferId, TransferSink,
};
use rocket_shared::error::DomainError;
use sha2::{Digest, Sha256};

use super::test_support::{identity, node, settle};
use super::transfer::{outcome_name, PeerSender, Timing};
use super::transfer_fakes::*;
use super::transfer_send::{export_notes, offer_error};

const ANA: u8 = 2;

fn dev(seed: u8) -> DeviceId {
    identity(seed).device_id
}

fn rig_with_ana() -> Rig {
    let rig = rig();
    rig.peers.add(dev(ANA), "Ana");
    rig
}

/// Sends "orders" to Ana, then freezes the clock so the test can move it.
async fn start(rig: &Rig) -> String {
    let started = rig
        .svc
        .send(&dev(ANA), "orders", false)
        .await
        .expect("the send starts");
    tokio::time::pause();
    settle().await;
    started.transfer_id
}

/// `(size, chunk count, sha256)` of the first offer sent to Ana.
fn offer_of(rig: &Rig) -> (u64, u32, String) {
    match rig.peers.sent_to(&dev(ANA)).first() {
        Some(PeerMessage::CollectionOffer {
            size_bytes,
            chunk_count,
            sha256,
            ..
        }) => (*size_bytes, *chunk_count, sha256.clone()),
        other => panic!("expected an offer first, got {other:?}"),
    }
}

fn chunks_sent(rig: &Rig) -> Vec<(u32, String)> {
    rig.peers
        .sent_to(&dev(ANA))
        .into_iter()
        .filter_map(|m| match m {
            PeerMessage::Chunk { seq, data, .. } => Some((seq, data)),
            _ => None,
        })
        .collect()
}

fn aborts_sent(rig: &Rig) -> Vec<AbortReason> {
    rig.peers
        .sent_to(&dev(ANA))
        .into_iter()
        .filter_map(|m| match m {
            PeerMessage::TransferAbort { reason, .. } => Some(reason),
            _ => None,
        })
        .collect()
}

/// Moves the clock past the wait for the receiver's result.
async fn past_the_result_wait() {
    tokio::time::advance(Timing::default().result_timeout + Duration::from_millis(1)).await;
    settle().await;
}

/// Starts a send, lets Ana accept and waits until every chunk is out.
async fn delivered(rig: &Rig) -> String {
    let id = start(rig).await;
    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;
    id
}

/// A delivered send whose receiver also sent the final `ChunkAck`, so the sender now waits
/// for the result (and only now may a lost peer or the result timeout give `Unconfirmed`).
async fn delivered_and_acked(rig: &Rig) -> String {
    let id = delivered(rig).await;
    let (_, count, _) = offer_of(rig);
    rig.svc.on_frame(dev(ANA), ack(&id, count));
    settle().await;
    id
}

/// A delivered send whose receiver answered with `outcome`.
async fn finish_with(outcome: ResultOutcome) -> (Rig, String) {
    let rig = rig_with_ana();
    let id = delivered(&rig).await;
    rig.svc.on_frame(dev(ANA), result(&id, outcome));
    settle().await;
    (rig, id)
}

#[tokio::test]
async fn new_registers_the_service_as_the_sink() {
    let rig = rig();
    assert!(rig.peers.sink().is_some());
}

#[test]
fn the_network_service_is_a_peer_sender() {
    let a = node(1, "ann");
    let _sender: Arc<dyn PeerSender> = a.svc.clone();
}

#[tokio::test]
async fn send_offers_the_collection_with_size_and_hash() {
    let rig = rig_with_ana();
    let id = start(&rig).await;

    assert!(TransferId::parse(&id).is_ok());
    let sent = rig.peers.sent_to(&dev(ANA));
    assert_eq!(sent.len(), 1, "only the offer, no chunk before the answer");
    let PeerMessage::CollectionOffer {
        transfer_id,
        name,
        size_bytes,
        chunk_count,
        sha256,
    } = &sent[0]
    else {
        panic!("expected an offer, got {:?}", sent[0]);
    };
    assert_eq!(transfer_id, &id);
    assert_eq!(name, "orders");
    assert!(*size_bytes > 100 * 1024 && *size_bytes < MAX_TRANSFER_BYTES);
    assert_eq!(*chunk_count, chunk_count_for(*size_bytes));
    assert_eq!(sha256.len(), 64);
    assert!(sha256
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
}

#[tokio::test]
async fn an_accepted_offer_streams_every_chunk_and_completes_on_the_result() {
    let rig = rig_with_ana();
    let id = start(&rig).await;
    let (size, count, sha) = offer_of(&rig);

    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;

    let chunks = chunks_sent(&rig);
    assert_eq!(chunks.len() as u32, count);
    for (index, (seq, _)) in chunks.iter().enumerate() {
        assert_eq!(*seq, index as u32, "in order");
    }
    let mut joined = Vec::new();
    for (_, data) in &chunks {
        joined.extend(STANDARD.decode(data).expect("base64"));
    }
    assert_eq!(joined.len() as u64, size);
    assert_eq!(format!("{:x}", Sha256::digest(&joined)), sha);
    for (_, message) in rig.peers.sent() {
        assert!(message.encode().len() <= MAX_FRAME_BYTES, "frame fits");
    }
    assert_eq!(
        rig.events.progress().last().copied(),
        Some((Direction::Send, size, size))
    );
    assert!(
        rig.events.finished().is_empty(),
        "sent is not reported before the receiver confirms the import"
    );
    assert!(rig.audited().is_empty());

    rig.svc
        .on_frame(dev(ANA), result(&id, ResultOutcome::Completed));
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id.clone(), Direction::Send, Outcome::Completed)]
    );
    assert_eq!(
        rig.audited(),
        vec![AuditEventKind::NetworkCollectionSent {
            device_id: dev(ANA).short(),
            collection: "orders".into(),
            outcome: "completed".into(),
        }]
    );
}

#[tokio::test]
async fn a_decline_ends_the_send_as_declined() {
    let rig = rig_with_ana();
    let id = start(&rig).await;

    rig.svc.on_frame(dev(ANA), answer(&id, false));
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Declined)]
    );
    assert!(chunks_sent(&rig).is_empty());
    assert!(rig.audited().is_empty());
}

#[tokio::test]
async fn an_unanswered_offer_expires_and_tells_the_peer() {
    let rig = rig_with_ana();
    let id = start(&rig).await;

    tokio::time::advance(TRANSFER_OFFER_TTL + Duration::from_secs(1)).await;
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Expired)]
    );
    assert_eq!(aborts_sent(&rig), vec![AbortReason::Expired]);
}

#[tokio::test]
async fn an_answer_after_the_offer_expired_is_ignored() {
    let rig = rig_with_ana();
    let id = start(&rig).await;
    tokio::time::advance(TRANSFER_OFFER_TTL + Duration::from_secs(1)).await;
    settle().await;

    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;

    assert!(chunks_sent(&rig).is_empty(), "no chunk after expiry");
    assert_eq!(rig.events.finished().len(), 1, "still one result");
}

#[tokio::test]
async fn a_peer_abort_ends_the_send_with_the_mapped_outcome() {
    let rig = rig_with_ana();
    let id = start(&rig).await;

    rig.svc.on_frame(dev(ANA), abort(&id, AbortReason::Busy));
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Busy)]
    );
    assert!(rig.audited().is_empty());
}

#[tokio::test]
async fn frames_from_another_device_are_ignored() {
    let rig = rig_with_ana();
    rig.peers.add(dev(3), "Ben");
    let id = start(&rig).await;

    rig.svc.on_frame(dev(3), answer(&id, true));
    rig.svc.on_frame(dev(3), abort(&id, AbortReason::Failed));
    settle().await;
    assert!(chunks_sent(&rig).is_empty());
    assert!(rig.events.finished().is_empty());

    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;
    rig.svc
        .on_frame(dev(3), result(&id, ResultOutcome::Completed));
    settle().await;

    assert!(
        rig.events.finished().is_empty(),
        "only the peer itself can confirm the import"
    );
    assert!(rig.audited().is_empty());
}

#[tokio::test]
async fn cancel_before_the_answer_aborts_and_finishes_cancelled() {
    let rig = rig_with_ana();
    let id = start(&rig).await;

    rig.svc.cancel(&id).await.expect("cancels");

    assert_eq!(aborts_sent(&rig), vec![AbortReason::Cancelled]);
    assert_eq!(
        rig.events.finished(),
        vec![(id.clone(), Direction::Send, Outcome::Cancelled)]
    );
    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;
    assert!(chunks_sent(&rig).is_empty(), "a late accept does nothing");
}

#[tokio::test]
async fn cancel_while_streaming_ends_once_as_cancelled() {
    let rig = rig_with_ana();
    let id = start(&rig).await;
    let (reached, release) = rig.peers.hold_chunk(1);

    rig.svc.on_frame(dev(ANA), answer(&id, true));
    reached.notified().await;
    rig.svc.cancel(&id).await.expect("cancels");
    release.notify_one();
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Cancelled)],
        "exactly one result"
    );
    assert_eq!(aborts_sent(&rig), vec![AbortReason::Cancelled]);
    assert!(rig.audited().is_empty(), "nothing was sent in the audit sense");
    assert!(
        matches!(rig.events.all().last(), Some(TransferEvent::Finished { .. })),
        "no progress after the end"
    );
}

#[tokio::test]
async fn cancel_after_the_last_chunk_is_refused_and_the_send_still_completes() {
    let rig = rig_with_ana();
    let id = start(&rig).await;
    let (_, count, _) = offer_of(&rig);
    let (reached, release) = rig.peers.hold_chunk(count - 1);

    rig.svc.on_frame(dev(ANA), answer(&id, true));
    reached.notified().await;
    let err = rig.svc.cancel(&id).await.expect_err("too late to cancel");
    release.notify_one();
    settle().await;

    assert!(
        matches!(&err, DomainError::InvalidInput(m) if m == "Already sent. Waiting for the result.")
    );
    assert!(aborts_sent(&rig).is_empty(), "nothing was aborted");
    assert!(rig.events.finished().is_empty(), "still waiting");

    rig.svc
        .on_frame(dev(ANA), result(&id, ResultOutcome::Completed));
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Completed)]
    );
    assert_eq!(rig.audited().len(), 1);
}

#[tokio::test]
async fn a_peer_going_offline_mid_send_fails_the_send() {
    let rig = rig_with_ana();
    let id = start(&rig).await;
    let (reached, release) = rig.peers.hold_chunk(1);

    rig.svc.on_frame(dev(ANA), answer(&id, true));
    reached.notified().await;
    rig.peers.set_online(&dev(ANA), false);
    release.notify_one();
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Failed)]
    );
    assert_eq!(chunks_sent(&rig).len(), 1, "only chunk 0 got through");
    assert!(rig.audited().is_empty());
}

#[tokio::test]
async fn a_failure_report_after_the_last_chunk_fails_the_send() {
    let rig = rig_with_ana();
    let id = delivered(&rig).await;
    assert!(rig.events.finished().is_empty(), "delivered, waiting");

    rig.svc.on_frame(dev(ANA), abort(&id, AbortReason::Failed));
    settle().await;
    past_the_result_wait().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Failed)],
        "one result, and the end of the wait adds none"
    );
    assert!(rig.audited().is_empty());
}

#[tokio::test]
async fn cancel_after_completion_is_a_no_op() {
    let rig = rig_with_ana();
    let id = delivered(&rig).await;
    rig.svc
        .on_frame(dev(ANA), result(&id, ResultOutcome::Completed));
    settle().await;
    assert_eq!(rig.events.finished().len(), 1);

    rig.svc.cancel(&id).await.expect("a no-op");

    assert_eq!(rig.events.finished().len(), 1);
    assert!(aborts_sent(&rig).is_empty());
}

#[tokio::test]
async fn a_second_send_to_the_same_peer_is_refused() {
    let rig = rig_with_ana();
    let _first = start(&rig).await;

    let err = rig
        .svc
        .send(&dev(ANA), "orders", false)
        .await
        .expect_err("one at a time");

    assert!(matches!(&err, DomainError::InvalidInput(m) if m.contains("already running")));
    let offers = rig
        .peers
        .sent_to(&dev(ANA))
        .into_iter()
        .filter(|m| matches!(m, PeerMessage::CollectionOffer { .. }))
        .count();
    assert_eq!(offers, 1);
    assert_eq!(rig.exports(), 1, "the refused send did not export");
}

#[tokio::test]
async fn an_empty_collection_is_refused_before_any_offer() {
    let rig = rig_with(Vec::new());
    rig.peers.add(dev(ANA), "Ana");

    let err = rig
        .svc
        .send(&dev(ANA), "orders", false)
        .await
        .expect_err("nothing to send");

    assert!(
        matches!(&err, DomainError::InvalidInput(m) if m == "There is nothing to send in that collection.")
    );
    assert!(rig.peers.sent().is_empty());
}

#[tokio::test]
async fn a_collection_over_the_limit_is_refused_before_any_offer() {
    let rig = rig_with(vec![
        (
            "opencollection.yml".to_string(),
            b"opencollection: \"1.0.0\"\ninfo:\n  name: orders\n".to_vec(),
        ),
        (
            "scripts/big.js".to_string(),
            noise(MAX_TRANSFER_BYTES as usize + 512 * 1024),
        ),
    ]);
    rig.peers.add(dev(ANA), "Ana");

    let err = rig
        .svc
        .send(&dev(ANA), "orders", false)
        .await
        .expect_err("too big");

    assert!(
        matches!(&err, DomainError::InvalidInput(m) if m.starts_with("This collection is too big to send (") && m.ends_with(" MiB, limit 10)."))
    );
    assert!(rig.peers.sent().is_empty(), "no offer for a big collection");
}

#[test]
fn the_too_big_refusal_names_the_size_rounded_up() {
    let err = offer_error(OfferError::TooBig, MAX_TRANSFER_BYTES as usize + 1);

    assert!(matches!(
        &err,
        DomainError::InvalidInput(m)
            if m == "This collection is too big to send (10.1 MiB, limit 10)."
    ));
}

#[tokio::test]
async fn sending_to_an_unpaired_or_offline_node_is_refused() {
    let rig = rig_with_ana();

    let err = rig
        .svc
        .send(&dev(9), "orders", false)
        .await
        .expect_err("not paired");
    assert!(matches!(&err, DomainError::InvalidInput(m) if m == "That node is not paired."));

    rig.peers.set_online(&dev(ANA), false);
    let err = rig
        .svc
        .send(&dev(ANA), "orders", false)
        .await
        .expect_err("offline");
    assert!(matches!(&err, DomainError::InvalidInput(m) if m == "That node is offline."));
    assert!(rig.peers.sent().is_empty());

    assert_eq!(rig.exports(), 0, "a refused send never reaches the export");

    // The refused sends left nothing behind, so a retry works.
    rig.peers.set_online(&dev(ANA), true);
    start(&rig).await;
}

#[tokio::test]
async fn a_blank_collection_name_is_refused() {
    let rig = rig_with_ana();

    let err = rig
        .svc
        .send(&dev(ANA), "   ", false)
        .await
        .expect_err("blank");

    assert!(matches!(err, DomainError::InvalidInput(_)));
    assert!(rig.peers.sent().is_empty());
}

#[tokio::test]
async fn a_peer_can_be_sent_to_again_after_the_send_ended() {
    let rig = rig_with_ana();
    let first = rig
        .svc
        .send(&dev(ANA), "orders", false)
        .await
        .expect("the first send starts")
        .transfer_id;
    rig.svc.cancel(&first).await.expect("cancels");

    let second = rig.svc.send(&dev(ANA), "orders", false).await;

    assert!(second.is_ok(), "the first send freed the peer's slot");
}

#[tokio::test]
async fn a_damaged_result_ends_the_send_as_damaged() {
    let (rig, id) = finish_with(ResultOutcome::Damaged).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Damaged)]
    );
    assert!(rig.audited().is_empty());
}

#[tokio::test]
async fn a_rejected_result_ends_the_send_as_rejected() {
    let (rig, id) = finish_with(ResultOutcome::Rejected).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Rejected)]
    );
    assert!(rig.audited().is_empty());
}

#[tokio::test]
async fn a_failed_result_ends_the_send_as_failed() {
    let (rig, id) = finish_with(ResultOutcome::Failed).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Failed)]
    );
    assert!(rig.audited().is_empty());
}

/// The audit row a send to Ana writes, with `outcome`.
fn sent_row(outcome: &str) -> AuditEventKind {
    AuditEventKind::NetworkCollectionSent {
        device_id: dev(ANA).short(),
        collection: "orders".into(),
        outcome: outcome.into(),
    }
}

#[tokio::test]
async fn a_missing_result_times_out_as_unconfirmed() {
    let rig = rig_with_ana();
    let id = delivered_and_acked(&rig).await;

    tokio::time::advance(Timing::default().result_timeout - Duration::from_secs(1)).await;
    settle().await;
    assert!(rig.events.finished().is_empty(), "still waiting");

    past_the_result_wait().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id.clone(), Direction::Send, Outcome::Unconfirmed)],
        "every byte left, so this is not a failure"
    );
    assert_eq!(rig.audited(), vec![sent_row("unconfirmed")], "the egress is recorded");
    assert!(matches!(
        rig.events.all().last(),
        Some(TransferEvent::Finished { .. })
    ));

    rig.svc
        .on_frame(dev(ANA), result(&id, ResultOutcome::Completed));
    settle().await;
    assert_eq!(rig.events.finished().len(), 1, "a late result is ignored");
    assert_eq!(rig.audited().len(), 1);
}

#[tokio::test]
async fn a_result_before_the_last_chunk_is_ignored() {
    let rig = rig_with_ana();
    let id = start(&rig).await;
    let (_, count, _) = offer_of(&rig);

    rig.svc
        .on_frame(dev(ANA), result(&id, ResultOutcome::Completed));
    settle().await;
    assert!(rig.events.finished().is_empty());
    assert!(rig.audited().is_empty());

    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;
    assert_eq!(chunks_sent(&rig).len() as u32, count, "the send went on");
}

#[tokio::test]
async fn a_peer_gone_after_the_final_ack_is_unconfirmed() {
    let rig = rig_with_ana();
    let id = delivered_and_acked(&rig).await;

    rig.svc.on_peer_gone(dev(ANA));
    settle().await;
    past_the_result_wait().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Unconfirmed)],
        "one result, no wait for the timeout, and not a failure"
    );
    assert_eq!(rig.audited(), vec![sent_row("unconfirmed")]);
}

#[tokio::test]
async fn a_peer_gone_after_the_last_chunk_but_before_the_final_ack_fails_the_send() {
    let rig = rig_with_ana();
    let id = delivered(&rig).await;
    assert!(rig.events.finished().is_empty(), "delivered, no final ack yet");

    rig.svc.on_peer_gone(dev(ANA));
    settle().await;
    past_the_result_wait().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id.clone(), Direction::Send, Outcome::Failed)],
        "the receiver may never have got the last chunk, so this is a failure"
    );
    assert_eq!(rig.audited(), vec![sent_row("failed")], "the egress is recorded");
    assert!(aborts_sent(&rig).is_empty(), "nothing is told to a gone peer");

    // A late final ack or result changes nothing.
    let (_, count, _) = offer_of(&rig);
    rig.svc.on_frame(dev(ANA), ack(&id, count));
    rig.svc
        .on_frame(dev(ANA), result(&id, ResultOutcome::Completed));
    settle().await;
    assert_eq!(rig.events.finished().len(), 1);
    assert_eq!(rig.audited().len(), 1);
}

#[tokio::test]
async fn no_final_ack_for_the_idle_limit_fails_the_send_even_after_the_last_chunk() {
    let timing = Timing {
        idle_limit: Duration::from_secs(10),
        ..Timing::default()
    };
    let rig = rig_timed(default_files(), timing);
    rig.peers.add(dev(ANA), "Ana");
    let id = delivered(&rig).await;

    tokio::time::advance(Duration::from_secs(9)).await;
    settle().await;
    assert!(rig.events.finished().is_empty(), "still within the idle limit");

    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Failed)],
        "no final ack means failed, not unconfirmed"
    );
    assert_eq!(aborts_sent(&rig), vec![AbortReason::Failed]);
    assert_eq!(rig.audited(), vec![sent_row("failed")]);
}

#[tokio::test]
async fn the_result_timer_starts_only_with_the_final_ack() {
    let rig = rig_with_ana();
    let id = delivered(&rig).await;

    // The 60 s result timeout passes with no final ack: it must not end the send.
    past_the_result_wait().await;
    assert!(rig.events.finished().is_empty(), "no result timer before the final ack");

    // The final ack starts the wait; now the result timeout gives `Unconfirmed`.
    let (_, count, _) = offer_of(&rig);
    rig.svc.on_frame(dev(ANA), ack(&id, count));
    settle().await;
    assert!(rig.events.finished().is_empty());
    past_the_result_wait().await;
    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Unconfirmed)]
    );
    assert_eq!(rig.audited(), vec![sent_row("unconfirmed")]);
}

#[tokio::test]
async fn a_peer_gone_fails_an_unanswered_offer() {
    let rig = rig_with_ana();
    let id = start(&rig).await;

    rig.svc.on_peer_gone(dev(ANA));
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Failed)]
    );
    assert!(aborts_sent(&rig).is_empty(), "nothing is told to a gone peer");
    let again = rig.svc.send(&dev(ANA), "orders", false).await;
    assert!(again.is_ok(), "the peer's slot is free again");
}

#[tokio::test]
async fn a_peer_gone_for_another_device_changes_nothing() {
    let rig = rig_with_ana();
    rig.peers.add(dev(3), "Ben");
    let _id = start(&rig).await;

    rig.svc.on_peer_gone(dev(3));
    settle().await;

    assert!(rig.events.finished().is_empty());
}

#[tokio::test]
async fn offer_raw_offers_the_given_bytes_and_returns_the_id() {
    let rig = rig_with_ana();
    let payload = Payload::new(40 * 1024);

    let id = rig
        .svc
        .offer_raw(&dev(ANA), "raw", payload.bytes.clone())
        .await
        .expect("offered");

    assert!(TransferId::parse(&id).is_ok());
    match rig.peers.sent_to(&dev(ANA)).first() {
        Some(PeerMessage::CollectionOffer {
            transfer_id,
            name,
            size_bytes,
            sha256,
            ..
        }) => {
            assert_eq!(transfer_id, &id);
            assert_eq!(name, "raw");
            assert_eq!(*size_bytes, 40 * 1024);
            assert_eq!(sha256, &payload.sha());
        }
        other => panic!("expected an offer, got {other:?}"),
    }
    assert_eq!(rig.exports(), 0, "no export for raw bytes");
    let again = rig
        .svc
        .offer_raw(&dev(ANA), "raw", payload.bytes)
        .await
        .expect_err("one at a time");
    assert!(matches!(&again, DomainError::InvalidInput(m) if m.contains("already running")));
}

#[tokio::test]
async fn offer_raw_frees_the_slot_when_the_offer_fails() {
    let rig = rig_with_ana();

    let err = rig
        .svc
        .offer_raw(&dev(9), "raw", vec![1; 10])
        .await
        .expect_err("not paired");
    assert!(matches!(&err, DomainError::InvalidInput(m) if m == "That node is not paired."));

    rig.peers.add(dev(9), "Zed");
    let ok = rig.svc.offer_raw(&dev(9), "raw", vec![1; 10]).await;
    assert!(ok.is_ok(), "the failed offer released the slot");
}

#[tokio::test]
async fn the_chunk_delay_hook_waits_before_each_chunk() {
    let rig = rig_with_ana();
    let id = start(&rig).await;
    rig.svc.set_chunk_delay(Duration::from_secs(5));

    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;
    assert!(chunks_sent(&rig).is_empty(), "the first chunk waits too");

    tokio::time::advance(Duration::from_secs(5) + Duration::from_millis(1)).await;
    settle().await;
    assert_eq!(chunks_sent(&rig).len(), 1, "one chunk per delay");
}

#[tokio::test]
async fn the_tamper_hook_flips_a_byte_of_the_chosen_chunk() {
    let rig = rig_with_ana();
    let id = start(&rig).await;
    let (_, count, sha) = offer_of(&rig);
    rig.svc.set_tamper(Some(1));

    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;

    let chunks = chunks_sent(&rig);
    assert_eq!(chunks.len() as u32, count, "every chunk is still sent");
    let mut joined = Vec::new();
    for (_, data) in &chunks {
        joined.extend(STANDARD.decode(data).expect("base64"));
    }
    assert_ne!(format!("{:x}", Sha256::digest(&joined)), sha, "the hash no longer fits");
}

#[test]
fn notes_list_skipped_and_blanked_items_only() {
    let note = |path: &str, feature: &str, action: ExportAction, detail: Option<&str>| ExportNote {
        path: path.to_string(),
        feature: feature.to_string(),
        action,
        detail: detail.map(str::to_string),
    };
    let report = ExportReport {
        exported: 5,
        items: vec![
            note("flows/a.yml", "flow", ExportAction::Skipped, None),
            note("flows/b.yml", "flow", ExportAction::Skipped, None),
            note("auth", "bearer token", ExportAction::Blanked, Some("value removed")),
            note("x.yml", "digest auth", ExportAction::Blanked, None),
            note("scripts/a.js", "rok script", ExportAction::Preserved, None),
            note("req.yml", "oauth2", ExportAction::Translated, Some("converted")),
        ],
    };

    assert_eq!(
        export_notes(&report),
        vec![
            "flow: skipped".to_string(),
            "bearer token: value removed".to_string(),
            "digest auth: blanked".to_string(),
        ],
        "one line per kind of item, kept and translated items left out"
    );
    assert!(export_notes(&ExportReport::default()).is_empty());
}

#[tokio::test]
async fn a_short_offer_ttl_expires_the_offer_at_that_ttl() {
    let timing = Timing {
        offer_ttl: Duration::from_secs(5),
        ..Timing::default()
    };
    let rig = rig_timed(default_files(), timing);
    rig.peers.add(dev(ANA), "Ana");
    let id = start(&rig).await;

    tokio::time::advance(Duration::from_secs(6)).await;
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Expired)],
        "the machine and the timer both follow the short ttl"
    );
    assert_eq!(aborts_sent(&rig), vec![AbortReason::Expired]);
}

#[tokio::test]
async fn a_short_result_timeout_ends_the_send_unconfirmed_at_that_timeout() {
    let timing = Timing {
        result_timeout: Duration::from_secs(5),
        ..Timing::default()
    };
    let rig = rig_timed(default_files(), timing);
    rig.peers.add(dev(ANA), "Ana");
    let id = delivered_and_acked(&rig).await;

    tokio::time::advance(Duration::from_secs(6)).await;
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Unconfirmed)]
    );
}

/// A collection whose ZIP needs about 96 chunks, three times the ack window.
fn big_files() -> Vec<(String, Vec<u8>)> {
    vec![
        (
            "opencollection.yml".to_string(),
            b"opencollection: \"1.0.0\"\ninfo:\n  name: orders\n".to_vec(),
        ),
        ("scripts/big.js".to_string(), noise(3 * 1024 * 1024)),
    ]
}

#[tokio::test]
async fn the_sender_never_has_more_than_32_chunks_unacked() {
    // A slow link: the receiver acks only when the test says so.
    let rig = rig_with(big_files());
    rig.peers.add(dev(ANA), "Ana");
    let id = start(&rig).await;
    let (_, count, _) = offer_of(&rig);
    assert!(count > 2 * MAX_UNACKED_CHUNKS, "the payload must outgrow the window");

    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;
    assert_eq!(
        chunks_sent(&rig).len() as u32,
        MAX_UNACKED_CHUNKS,
        "the stream stops at the window without an ack"
    );

    let mut acked = 0u32;
    while acked < count {
        acked = (acked + 16).min(count);
        rig.svc.on_frame(dev(ANA), ack(&id, acked));
        settle().await;
        let sent = chunks_sent(&rig).len() as u32;
        assert!(sent - acked <= MAX_UNACKED_CHUNKS, "sent {sent}, acked {acked}");
        assert!(sent <= count);
    }
    assert_eq!(chunks_sent(&rig).len() as u32, count, "every chunk went out in the end");

    rig.svc
        .on_frame(dev(ANA), result(&id, ResultOutcome::Completed));
    settle().await;
    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Completed)]
    );
}

#[tokio::test]
async fn no_ack_for_the_idle_limit_fails_the_send() {
    let timing = Timing {
        idle_limit: Duration::from_secs(10),
        ..Timing::default()
    };
    let rig = rig_timed(big_files(), timing);
    rig.peers.add(dev(ANA), "Ana");
    let id = start(&rig).await;
    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;

    tokio::time::advance(Duration::from_secs(9)).await;
    settle().await;
    assert!(rig.events.finished().is_empty(), "still within the idle limit");

    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;
    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Failed)]
    );
    assert_eq!(aborts_sent(&rig), vec![AbortReason::Failed]);
    assert!(rig.audited().is_empty());
}

#[tokio::test]
async fn an_ack_past_the_chunks_sent_fails_the_send() {
    let rig = rig_with(big_files());
    rig.peers.add(dev(ANA), "Ana");
    let id = start(&rig).await;
    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;

    rig.svc.on_frame(dev(ANA), ack(&id, 1000));
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Failed)]
    );
    assert_eq!(aborts_sent(&rig), vec![AbortReason::Failed]);
    assert_eq!(chunks_sent(&rig).len() as u32, MAX_UNACKED_CHUNKS, "no chunk after it");
}

#[tokio::test]
async fn a_bad_collection_name_is_refused_with_the_rename_text() {
    let rig = rig_with_ana();

    // U+202E turns the rest of the name around on screen.
    let err = rig
        .svc
        .send(&dev(ANA), "orders\u{202E}gpj", false)
        .await
        .expect_err("a spoofing name");

    assert!(matches!(
        &err,
        DomainError::InvalidInput(m)
            if m == "This collection name cannot be sent. Rename it and try again."
    ));
    assert!(rig.peers.sent().is_empty());
    assert_eq!(rig.exports(), 0, "the name is checked before the export");
    assert!(matches!(
        offer_error(OfferError::BadName, 10),
        DomainError::InvalidInput(m)
            if m == "This collection name cannot be sent. Rename it and try again."
    ));
    start(&rig).await;
}

#[tokio::test]
async fn a_send_without_environments_carries_no_environment_files() {
    let mut files = default_files();
    files.push((
        "environments/prod.yml".to_string(),
        b"name: prod\nvariables:\n  - name: host\n    value: example.org\n".to_vec(),
    ));
    for (include, expect_env) in [(false, false), (true, true)] {
        let rig = rig_with(files.clone());
        rig.peers.add(dev(ANA), "Ana");
        let id = rig
            .svc
            .send(&dev(ANA), "orders", include)
            .await
            .expect("the send starts")
            .transfer_id;
        tokio::time::pause();
        rig.svc.on_frame(dev(ANA), answer(&id, true));
        settle().await;

        let mut zip = Vec::new();
        for (_, data) in chunks_sent(&rig) {
            zip.extend(STANDARD.decode(data).expect("base64"));
        }
        let has_env = zip_entry_names(&zip)
            .iter()
            .any(|name| name.contains("environments/"));
        assert_eq!(has_env, expect_env, "include_environments = {include}");
        tokio::time::resume();
    }
}

#[test]
fn outcome_names_match_the_wire_names() {
    for outcome in [
        Outcome::Completed,
        Outcome::Unconfirmed,
        Outcome::Declined,
        Outcome::Expired,
        Outcome::Cancelled,
        Outcome::Failed,
        Outcome::TooBig,
        Outcome::Busy,
        Outcome::Damaged,
        Outcome::Rejected,
    ] {
        let wire = serde_json::to_string(&outcome).expect("serializes");
        assert_eq!(wire, format!("\"{}\"", outcome_name(outcome)));
    }
}
```

In `crates/rocket-app/src/network/mod.rs`, add (next to the other test modules):

```rust
// The transfer fakes are shared by the send and receive tests.
#[cfg(test)]
#[allow(dead_code)]
pub(crate) mod transfer_fakes;
#[cfg(test)]
#[path = "transfer_send_tests.rs"]
mod transfer_send_tests;
```

- [ ] **Step 5: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket-app transfer_send_tests`
Expected: FAIL to compile, `error[E0432]: unresolved import `super::transfer``, then the same for `transfer_send` (`export_notes`) and `outcome_name`. The Task 1 tests still compile. (Each new test is written against the final behaviour: with a stream that ignores the window, `the_sender_never_has_more_than_32_chunks_unacked` sees every chunk at once; with `fail_with(Failed)` on a gone peer, `a_peer_gone_after_the_final_ack_is_unconfirmed` sees `Failed`; with a result timer started at the last chunk, `a_peer_gone_after_the_last_chunk_but_before_the_final_ack_fails_the_send` and `the_result_timer_starts_only_with_the_final_ack` see `Unconfirmed`; without the final-ack idle guard, `no_final_ack_for_the_idle_limit_fails_the_send_even_after_the_last_chunk` never ends.) (The fakes also fail on `fn release` until Plan 02's trait has it; that is the Step 2 check.)

- [ ] **Step 6: Write `transfer.rs`**

Create `crates/rocket-app/src/network/transfer.rs`:

```rust
//! The transfer service: sends one collection to a paired node and receives one from it.
//!
//! The service reaches the network through `PeerSender` (the network service in production).
//! Frames and peer-gone notices arrive through `TransferSink`, go into one bounded queue and
//! are handled by one worker in order. Every rule (offer shape, expiry, chunk order, limits)
//! lives in the Plan 01 machines in `rocket-network`; this file and its siblings do the I/O
//! around them. The worker only runs short sync handlers; the finalize step and every outbound
//! frame of a handler run as their own tasks. State lives behind one `std::sync::Mutex` that is
//! never held across an `.await`. Whoever removes a transfer from the state emits its single
//! `Finished` event.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use rocket_audit::{AuditActor, AuditEventKind, SecurityAuditPublisher};
use rocket_network::limits::{SENDER_RESULT_TIMEOUT, TRANSFER_OFFER_TTL};
use rocket_network::{
    DeviceId, Outcome, PeerMessage, TransferEvent, TransferId, TransferLimits, TransferSink,
};
use rocket_shared::error::DomainResult;
use rocket_shared::events::EventPublisher;
use tokio::sync::mpsc;

use crate::collection_export_service::CollectionExportService;

use super::service::NetworkService;
use super::transfer_ports::{CollectionImportPort, TransferStaging};
use super::transfer_send::Out;

/// Items that may wait for the worker. A full queue drops a frame; the lost frame ends its
/// transfer by the sequence rule or the idle limit (ruling 22).
const FRAME_QUEUE: usize = 256;

/// A receive that gets no frame, or a send that gets no ack, for this long is failed. The
/// peer-gone hook ends a dead connection at once; this is the fallback for a half-open one.
const DEFAULT_IDLE_LIMIT: Duration = Duration::from_secs(120);

/// The wire name of an outcome, for the audit `outcome` field. A test pins it to serde.
pub(super) fn outcome_name(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Completed => "completed",
        Outcome::Unconfirmed => "unconfirmed",
        Outcome::Declined => "declined",
        Outcome::Expired => "expired",
        Outcome::Cancelled => "cancelled",
        Outcome::Failed => "failed",
        Outcome::TooBig => "too_big",
        Outcome::Busy => "busy",
        Outcome::Damaged => "damaged",
        Outcome::Rejected => "rejected",
    }
}

/// What the worker handles, in arrival order.
enum Item {
    Frame(DeviceId, PeerMessage),
    Gone(DeviceId),
}

/// The three waits of the service. Only tests change them (`new_with_timing`).
#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// How long an offer may stay unanswered. The machines and the offer timers use it.
    pub offer_ttl: Duration,
    /// How long an accepted receive may stay silent, and how long a send with a full ack
    /// window waits for the next `ChunkAck`.
    pub idle_limit: Duration,
    /// How long the sender waits for the receiver's `TransferResult`.
    pub result_timeout: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            offer_ttl: TRANSFER_OFFER_TTL,
            idle_limit: DEFAULT_IDLE_LIMIT,
            result_timeout: SENDER_RESULT_TIMEOUT,
        }
    }
}

/// What the transfer service needs from the network node.
#[async_trait]
pub trait PeerSender: Send + Sync {
    /// Sends one frame to a paired, online device. Offline is `InvalidInput`.
    async fn send_to(&self, device: &DeviceId, message: PeerMessage) -> DomainResult<()>;
    /// The display name and short id of a paired, unblocked device.
    fn peer_label(&self, device: &DeviceId) -> Option<(String, String)>;
    /// True when the device has a live connection.
    fn is_online(&self, device: &DeviceId) -> bool;
    /// Sets the receiver of the transfer frames and peer-gone notices.
    fn set_transfer_sink(&self, sink: Arc<dyn TransferSink>);
}

#[async_trait]
impl PeerSender for NetworkService {
    async fn send_to(&self, device: &DeviceId, message: PeerMessage) -> DomainResult<()> {
        NetworkService::send_to(self, device, message).await
    }

    fn peer_label(&self, device: &DeviceId) -> Option<(String, String)> {
        NetworkService::peer_label(self, device)
    }

    fn is_online(&self, device: &DeviceId) -> bool {
        NetworkService::is_online(self, device)
    }

    fn set_transfer_sink(&self, sink: Arc<dyn TransferSink>) {
        NetworkService::set_transfer_sink(self, sink);
    }
}

/// Everything the service needs from outside. All of it is a port or a service.
pub struct TransferDeps {
    pub network: Arc<dyn PeerSender>,
    pub export: Arc<CollectionExportService>,
    pub importer: Arc<dyn CollectionImportPort>,
    pub staging: Arc<dyn TransferStaging>,
    pub events: Arc<dyn EventPublisher>,
    pub audit: Arc<dyn SecurityAuditPublisher>,
}

/// The open transfers and the offer limits. `limits` knows every transfer in both maps.
#[derive(Default)]
pub(super) struct Inner {
    pub outgoing: HashMap<TransferId, Out>,
    pub limits: TransferLimits,
}

impl Inner {
    /// Removes an outgoing transfer and frees its slot in the limits.
    pub(super) fn take_out(&mut self, id: &TransferId) -> Option<Out> {
        self.limits.release(id);
        self.outgoing.remove(id)
    }
}

pub struct TransferService {
    me: Weak<TransferService>,
    pub(super) deps: TransferDeps,
    pub(super) timing: Timing,
    inner: Mutex<Inner>,
    frames: mpsc::Sender<Item>,
    worker: Mutex<Option<mpsc::Receiver<Item>>>,
    /// Test hook: a sleep before each chunk is taken. Not in a release build.
    #[cfg(test)]
    pub(super) chunk_delay: Mutex<Duration>,
    /// Test hook: the chunk with this sequence number is sent with one byte flipped. Not in a
    /// release build.
    #[cfg(test)]
    pub(super) tamper: Mutex<Option<u32>>,
}

impl TransferService {
    /// Builds the service and registers it as the frame sink of the network node.
    pub fn new(deps: TransferDeps) -> Arc<TransferService> {
        Self::new_with_timing(deps, Timing::default())
    }

    /// Like `new`, with other waits. Tests use it to shorten the offer lifetime, the idle
    /// limit and the wait for the result.
    #[doc(hidden)]
    pub fn new_with_timing(deps: TransferDeps, timing: Timing) -> Arc<TransferService> {
        let (frames, queue) = mpsc::channel(FRAME_QUEUE);
        let svc = Arc::new_cyclic(|me| TransferService {
            me: me.clone(),
            deps,
            timing,
            inner: Mutex::new(Inner::default()),
            frames,
            worker: Mutex::new(Some(queue)),
            #[cfg(test)]
            chunk_delay: Mutex::new(Duration::ZERO),
            #[cfg(test)]
            tamper: Mutex::new(None),
        });
        let sink: Arc<dyn TransferSink> = svc.clone();
        svc.deps.network.set_transfer_sink(sink);
        svc
    }

    /// Test hook: wait this long before each chunk is taken from the ZIP.
    #[cfg(test)]
    pub(crate) fn set_chunk_delay(&self, delay: Duration) {
        *self.chunk_delay.lock().unwrap_or_else(|e| e.into_inner()) = delay;
    }

    /// Test hook: flip one byte of the chunk with this sequence number before it is framed.
    #[cfg(test)]
    pub(crate) fn set_tamper(&self, seq: Option<u32>) {
        *self.tamper.lock().unwrap_or_else(|e| e.into_inner()) = seq;
    }

    pub(super) fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(super) fn arc(&self) -> Option<Arc<TransferService>> {
        self.me.upgrade()
    }

    /// The time the machines are given. It follows the tokio clock, so paused time works in tests.
    pub(super) fn now() -> Instant {
        tokio::time::Instant::now().into_std()
    }

    pub(super) fn emit(&self, event: TransferEvent) {
        self.deps.events.publish(&event);
    }

    pub(super) fn audit(&self, kind: AuditEventKind) {
        self.deps.audit.publish(AuditActor::User.into(), None, kind);
    }

    /// Sends a frame and logs a failure. Used where the answer to a failure is "carry on".
    pub(super) async fn tell(&self, device: &DeviceId, message: PeerMessage) {
        if let Err(e) = self.deps.network.send_to(device, message).await {
            tracing::debug!(code = "network_transfer_send_failed", error = %e, "frame not sent");
        }
    }

    /// Sends a frame from its own task. The frame worker uses this, so a slow peer never holds
    /// back the other transfers.
    pub(super) fn tell_later(&self, device: DeviceId, message: PeerMessage) {
        let Some(me) = self.arc() else {
            return;
        };
        drop(tokio::spawn(async move { me.tell(&device, message).await }));
    }

    /// Starts the worker on first use. Every caller runs inside the tokio runtime.
    pub(super) fn ensure_worker(&self) {
        let queue = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(queue) = queue {
            drop(tokio::spawn(Self::run_worker(self.me.clone(), queue)));
        }
    }

    async fn run_worker(me: Weak<TransferService>, mut queue: mpsc::Receiver<Item>) {
        while let Some(item) = queue.recv().await {
            let Some(svc) = me.upgrade() else {
                break;
            };
            match item {
                Item::Frame(from, message) => svc.handle_frame(from, message),
                Item::Gone(device) => svc.handle_gone(&device),
            }
        }
    }

    /// Puts an item into the queue. A full queue drops a frame, but a peer-gone notice must
    /// not be lost, so it runs right here.
    fn enqueue(&self, item: Item) {
        match self.frames.try_send(item) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(Item::Gone(device)))
            | Err(mpsc::error::TrySendError::Closed(Item::Gone(device))) => {
                self.handle_gone(&device);
            }
            Err(_) => {
                tracing::debug!(code = "network_transfer_frame_dropped", "frame queue is full");
            }
        }
    }

    fn handle_frame(&self, from: DeviceId, message: PeerMessage) {
        match message {
            PeerMessage::OfferAnswer {
                transfer_id,
                accept,
            } => {
                if let Ok(id) = TransferId::parse(&transfer_id) {
                    self.on_answer(&from, &id, accept);
                }
            }
            PeerMessage::TransferAbort {
                transfer_id,
                reason,
            } => {
                if let Ok(id) = TransferId::parse(&transfer_id) {
                    self.abort_outgoing(&from, &id, reason);
                }
            }
            PeerMessage::TransferResult {
                transfer_id,
                outcome,
            } => {
                if let Ok(id) = TransferId::parse(&transfer_id) {
                    self.on_result(&from, &id, outcome);
                }
            }
            PeerMessage::ChunkAck {
                transfer_id,
                next_seq,
            } => {
                if let Ok(id) = TransferId::parse(&transfer_id) {
                    self.on_ack(&from, &id, next_seq);
                }
            }
            // The receiver side handles offers and chunks (Task 3).
            _ => {}
        }
    }

    /// The device's connection ended or its pairing was removed: end its transfers now.
    pub(super) fn handle_gone(&self, device: &DeviceId) {
        self.outgoing_gone(device);
        // The receiver side joins in Task 3.
    }
}

impl TransferSink for TransferService {
    fn on_frame(&self, from: DeviceId, message: PeerMessage) {
        self.ensure_worker();
        self.enqueue(Item::Frame(from, message));
    }

    fn on_peer_gone(&self, device: DeviceId) {
        self.ensure_worker();
        self.enqueue(Item::Gone(device));
    }
}
```

- [ ] **Step 7: Write `transfer_send.rs`**

Create `crates/rocket-app/src/network/transfer_send.rs`:

```rust
//! The sender side of a transfer, around one `OutgoingTransfer` per send.
//!
//! The machine decides what is allowed. The service exports and hashes the ZIP, sends the
//! frames the machine builds, runs the timers and reports the result. After the last chunk
//! the machine is `Done` and waits for the receiver's final `ChunkAck`, then for its
//! `TransferResult`; the entry stays until the result, an abort, a gone peer, the idle limit
//! (no final ack: `Failed`) or `Timing::result_timeout` (after the final ack: `Unconfirmed`).

use std::sync::Arc;
use std::time::Instant;

use rocket_audit::AuditEventKind;
use rocket_export::{ExportAction, ExportReport};
use rocket_network::limits::{CHUNK_BYTES, MAX_TRANSFER_BYTES};
use rocket_network::{
    offer_name_is_valid, AbortReason, DeviceId, Direction, OfferError, Outcome, OutgoingState,
    OutgoingTransfer, PeerMessage, ResultOutcome, TransferEvent, TransferId,
};
use rocket_shared::error::{DomainError, DomainResult};
use sha2::{Digest, Sha256};
use tokio::sync::Notify;

use super::transfer::{outcome_name, TransferService};

const MIB: u64 = 1024 * 1024;

/// What `send` returns: the transfer id and one line per thing the export left out or blanked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendStarted {
    pub transfer_id: String,
    pub notes: Vec<String>,
}

/// One outgoing transfer.
pub(super) struct Out {
    pub device: DeviceId,
    pub peer_name: String,
    pub collection: String,
    pub zip: Vec<u8>,
    pub machine: OutgoingTransfer,
    /// The offer, the accept or the last good `ChunkAck`. The idle limit counts from here.
    pub last_ack: Instant,
    /// Wakes the stream when an ack opens the window. `notify_one` keeps a permit.
    pub ack_signal: Arc<Notify>,
    /// The last chunk frame was built (every byte of the ZIP is out or on its way).
    pub all_chunks_out: bool,
    /// Set by the paths where the sender itself lost the stream (peer gone, idle stall, failed
    /// frame send) after `all_chunks_out`: then the `Failed` end is audited as `failed`.
    pub audit_failed: bool,
    /// The 60 s result timer runs only once the receiver's final `ChunkAck` arrived.
    pub result_timer_started: bool,
}

/// What the stream does next.
enum StreamStep {
    /// Send this frame; then `done` of `total` bytes are out, and `last` says it was the last.
    Send {
        device: DeviceId,
        frame: PeerMessage,
        done: u64,
        total: u64,
        last: bool,
    },
    /// The ack window is full. Wait for the signal, or give up at the deadline.
    Wait(Arc<Notify>, Instant),
}

/// The words for an offer the machine refused. `len` is the ZIP size, rounded up to one decimal.
pub(super) fn offer_error(error: OfferError, len: usize) -> DomainError {
    let text = match error {
        OfferError::Empty => "There is nothing to send in that collection.".to_string(),
        OfferError::TooBig => {
            let tenths = (len as u64 * 10).div_ceil(MIB);
            format!(
                "This collection is too big to send ({}.{} MiB, limit {}).",
                tenths / 10,
                tenths % 10,
                MAX_TRANSFER_BYTES / MIB
            )
        }
        OfferError::BadName => {
            "This collection name cannot be sent. Rename it and try again.".to_string()
        }
        OfferError::BadId | OfferError::BadHash | OfferError::ChunkCountMismatch => {
            return DomainError::Internal("The offer could not be built.".to_string());
        }
    };
    DomainError::InvalidInput(text)
}

/// One line per report item that was skipped or blanked, formatted `<feature>: <detail>` (the
/// action word when there is no detail). Kept and translated items are left out, and the same
/// line is listed once.
pub(super) fn export_notes(report: &ExportReport) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for note in &report.items {
        let word = match note.action {
            ExportAction::Skipped => "skipped",
            ExportAction::Blanked => "blanked",
            ExportAction::Preserved | ExportAction::Translated => continue,
        };
        let line = format!(
            "{}: {}",
            note.feature,
            note.detail.as_deref().unwrap_or(word)
        );
        if !lines.contains(&line) {
            lines.push(line);
        }
    }
    lines
}

fn already_running() -> DomainError {
    DomainError::InvalidInput("A transfer to that node is already running.".to_string())
}

fn not_paired() -> DomainError {
    DomainError::InvalidInput("That node is not paired.".to_string())
}

fn offline() -> DomainError {
    DomainError::InvalidInput("That node is offline.".to_string())
}

fn already_sent() -> DomainError {
    DomainError::InvalidInput("Already sent. Waiting for the result.".to_string())
}

impl TransferService {
    /// Builds the ZIP, offers it to `device` and returns the transfer id and the export notes.
    /// The stream starts when the peer accepts.
    pub async fn send(
        &self,
        device: &DeviceId,
        collection: &str,
        include_environments: bool,
    ) -> DomainResult<SendStarted> {
        self.ensure_worker();
        let collection = collection.trim().to_string();
        if collection.is_empty() {
            return Err(DomainError::InvalidInput(
                "Pick a collection to send.".to_string(),
            ));
        }
        // The same name rule as the receiver's, before any export work (ruling 13).
        if !offer_name_is_valid(&collection) {
            return Err(offer_error(OfferError::BadName, 0));
        }
        if self.deps.network.peer_label(device).is_none() {
            return Err(not_paired());
        }
        // Refuse an offline node before any export work (ruling 14).
        if !self.deps.network.is_online(device) {
            return Err(offline());
        }
        let id = self.claim_send(device)?;
        let sent = self
            .export_and_offer(device, &id, &collection, include_environments)
            .await;
        match sent {
            Ok(notes) => Ok(SendStarted {
                transfer_id: id.as_str().to_string(),
                notes,
            }),
            Err(e) => {
                // A failure after the entry exists already freed the slot. This is for the rest.
                self.lock().limits.release(&id);
                Err(e)
            }
        }
    }

    /// Claims the peer's one outgoing slot. A second send to the same peer is refused here.
    pub(super) fn claim_send(&self, device: &DeviceId) -> DomainResult<TransferId> {
        let id = TransferId::random();
        if self.lock().limits.begin_outgoing(device, &id) {
            Ok(id)
        } else {
            Err(already_running())
        }
    }

    /// Test hook for Plan 4: offers raw bytes (not an export) and returns the transfer id.
    /// It claims the peer's slot, offers and frees the slot again when the offer fails.
    #[cfg(test)]
    pub(crate) async fn offer_raw(
        &self,
        device: &DeviceId,
        name: &str,
        bytes: Vec<u8>,
    ) -> DomainResult<String> {
        self.ensure_worker();
        let id = self.claim_send(device)?;
        match self.offer_bytes(device, &id, name, bytes).await {
            Ok(()) => Ok(id.as_str().to_string()),
            Err(e) => {
                self.lock().limits.release(&id);
                Err(e)
            }
        }
    }

    async fn export_and_offer(
        &self,
        device: &DeviceId,
        id: &TransferId,
        collection: &str,
        include_environments: bool,
    ) -> DomainResult<Vec<String>> {
        let export = Arc::clone(&self.deps.export);
        let name = collection.to_string();
        let (zip, notes) = tokio::task::spawn_blocking(
            move || -> DomainResult<(Vec<u8>, Vec<String>)> {
                let out = export.export_bruno(&name, include_environments)?;
                if out.report.exported == 0 {
                    return Err(DomainError::InvalidInput(
                        "There is nothing to send in that collection.".to_string(),
                    ));
                }
                let notes = export_notes(&out.report);
                Ok((out.bytes, notes))
            },
        )
        .await
        .map_err(|e| DomainError::Internal(format!("The export task failed: {e}")))??;
        self.offer_bytes(device, id, collection, zip).await?;
        Ok(notes)
    }

    /// Everything after the export: hash, build the machine, register, send the offer, start
    /// the offer timer. The caller has claimed the slot for `id` and frees it on an error.
    pub(super) async fn offer_bytes(
        &self,
        device: &DeviceId,
        id: &TransferId,
        collection: &str,
        zip: Vec<u8>,
    ) -> DomainResult<()> {
        let Some((peer_name, _short)) = self.deps.network.peer_label(device) else {
            return Err(not_paired());
        };
        let machine_id = id.clone();
        let name = collection.to_string();
        // Read the clock and the ttl here: the closure runs on a blocking thread.
        let now = Self::now();
        let ttl = self.timing.offer_ttl;
        let (zip, machine) = tokio::task::spawn_blocking(move || {
            let len = zip.len();
            let sha256 = format!("{:x}", Sha256::digest(&zip));
            OutgoingTransfer::new(machine_id, &name, len as u64, &sha256, now)
                .map(|machine| (zip, machine.with_offer_ttl(ttl)))
                .map_err(|e| offer_error(e, len))
        })
        .await
        .map_err(|e| DomainError::Internal(format!("The hash task failed: {e}")))??;

        let offer = machine.offer_message();
        self.lock().outgoing.insert(
            id.clone(),
            Out {
                device: *device,
                peer_name,
                collection: collection.to_string(),
                zip,
                machine,
                last_ack: now,
                ack_signal: Arc::new(Notify::new()),
                all_chunks_out: false,
                audit_failed: false,
                result_timer_started: false,
            },
        );
        if let Err(e) = self.deps.network.send_to(device, offer).await {
            self.take_out(id);
            return Err(e);
        }
        self.spawn_offer_timer(id.clone());
        Ok(())
    }

    /// Cancels a transfer this node runs. An unknown or finished id is not an error. After the
    /// last chunk a send can no longer be cancelled.
    pub async fn cancel(&self, transfer_id: &str) -> DomainResult<()> {
        let Ok(id) = TransferId::parse(transfer_id) else {
            return Ok(());
        };
        self.cancel_outgoing(&id).await?;
        Ok(())
    }

    // ----- frames from the peer -----

    /// The peer answered the offer. The machine decides: accepted, declined or too late.
    pub(super) fn on_answer(&self, from: &DeviceId, id: &TransferId, accept: bool) {
        enum Next {
            Ignore,
            Stream,
            End,
        }
        let now = Self::now();
        let next = {
            let mut inner = self.lock();
            match inner.outgoing.get_mut(id) {
                Some(out) if out.device == *from => {
                    let was_offered = out.machine.state() == OutgoingState::Offered;
                    if out.machine.on_answer(accept, now).is_some() {
                        Next::End
                    } else if was_offered && out.machine.state() == OutgoingState::Accepted {
                        // The idle limit for the first window counts from the accept.
                        out.last_ack = now;
                        Next::Stream
                    } else {
                        Next::Ignore
                    }
                }
                _ => Next::Ignore,
            }
        };
        match next {
            Next::Ignore => {}
            Next::Stream => self.spawn_stream(id.clone()),
            Next::End => {
                if let Some(out) = self.take_out(id) {
                    // Only a late accept needs a word to the peer. A decline came from it.
                    let expired = out.machine.outcome() == Some(Outcome::Expired);
                    let notice = (out.device, out.machine.abort_message(AbortReason::Expired));
                    self.finish_out(id, out);
                    if expired {
                        self.tell_later(notice.0, notice.1);
                    }
                }
            }
        }
    }

    /// The peer aborted one of our transfers. Returns false when the id is not ours.
    pub(super) fn abort_outgoing(
        &self,
        from: &DeviceId,
        id: &TransferId,
        reason: AbortReason,
    ) -> bool {
        let ended = {
            let mut inner = self.lock();
            let Some(out) = inner.outgoing.get_mut(id) else {
                return false;
            };
            if out.device != *from {
                return false;
            }
            // After the last chunk the machine is `Done` and ignores the abort; a failure
            // report while the sender waits for the result still counts.
            out.machine
                .on_abort(reason)
                .or_else(|| out.machine.fail_with(Outcome::from(reason)))
                .is_some()
        };
        if ended {
            if let Some(out) = self.take_out(id) {
                self.finish_out(id, out);
            }
        }
        true
    }

    /// The receiver's verdict after the import. The machine accepts it once, and only after the
    /// last chunk. A result from another device is ignored.
    pub(super) fn on_result(&self, from: &DeviceId, id: &TransferId, outcome: ResultOutcome) {
        let ended = {
            let mut inner = self.lock();
            match inner.outgoing.get_mut(id) {
                Some(out) if out.device == *from => out.machine.on_result(outcome).is_some(),
                _ => false,
            }
        };
        if ended {
            if let Some(out) = self.take_out(id) {
                self.finish_out(id, out);
            }
        }
    }

    /// The receiver confirmed `next_seq` chunks. The machine checks the number: a good ack
    /// restarts the idle clock and wakes the stream, a bad one ends the send as `Failed`.
    pub(super) fn on_ack(&self, from: &DeviceId, id: &TransferId, next_seq: u32) {
        let now = Self::now();
        let (bad, start_result_timer) = {
            let mut inner = self.lock();
            let Some(out) = inner.outgoing.get_mut(id).filter(|o| o.device == *from) else {
                return;
            };
            if out.machine.on_ack(next_seq).is_some() {
                (true, false)
            } else {
                out.last_ack = now;
                out.ack_signal.notify_one();
                // Only the final ack starts the wait for the result (and so the only
                // `Unconfirmed` end). Before it a lost peer is `Failed`.
                let start = out.machine.awaiting_result() && !out.result_timer_started;
                out.result_timer_started |= start;
                (false, start)
            }
        };
        if start_result_timer {
            self.spawn_result_timer(id.clone());
        }
        if bad {
            if let Some(out) = self.take_out(id) {
                let device = out.device;
                let frame = out.machine.abort_message(AbortReason::Failed);
                self.finish_out(id, out);
                self.tell_later(device, frame);
            }
        }
    }

    /// The device's connection ended or its pairing was removed. Every send to it ends now:
    /// `Unconfirmed` only after the receiver's final `ChunkAck`, `Failed` before it (also after
    /// the last chunk was sent). Nothing is told to the gone peer.
    pub(super) fn outgoing_gone(&self, device: &DeviceId) {
        let gone: Vec<(TransferId, Out)> = {
            let mut inner = self.lock();
            let ids: Vec<TransferId> = inner
                .outgoing
                .iter()
                .filter(|(_, out)| out.device == *device)
                .map(|(id, _)| id.clone())
                .collect();
            ids.into_iter()
                .filter_map(|id| inner.take_out(&id).map(|out| (id, out)))
                .collect()
        };
        for (id, mut out) in gone {
            if out.machine.connection_lost().is_none() {
                out.machine.fail_with(Outcome::Failed);
            }
            self.finish_out_lost(&id, out);
        }
    }

    // ----- helpers -----

    fn take_out(&self, id: &TransferId) -> Option<Out> {
        self.lock().take_out(id)
    }

    /// Removes the transfer only while `wanted` accepts it.
    fn take_out_if(&self, id: &TransferId, wanted: impl Fn(&Out) -> bool) -> Option<Out> {
        let mut inner = self.lock();
        let ready = inner.outgoing.get(id).is_some_and(wanted);
        if ready {
            inner.take_out(id)
        } else {
            None
        }
    }

    /// The one place a send ends. The caller has already removed the transfer, and the machine
    /// holds the outcome. `Completed` and `Unconfirmed` are audited: the bytes left the machine.
    /// A `Failed` end is audited only when `audit_failed` is set (the stream was lost after
    /// the last chunk but before the final ack); a failure the receiver reported is not.
    fn finish_out(&self, id: &TransferId, out: Out) {
        let outcome = out.machine.outcome().unwrap_or(Outcome::Failed);
        self.emit(TransferEvent::Finished {
            transfer_id: id.as_str().to_string(),
            direction: Direction::Send,
            outcome,
            display_name: out.peer_name.clone(),
            collection_name: out.collection.clone(),
            message: None,
        });
        if matches!(outcome, Outcome::Completed | Outcome::Unconfirmed)
            || (outcome == Outcome::Failed && out.audit_failed)
        {
            self.audit(AuditEventKind::NetworkCollectionSent {
                device_id: out.device.short(),
                collection: out.collection,
                outcome: outcome_name(outcome).to_string(),
            });
        }
    }

    /// Ends a send the sender itself lost: the stream is gone, stalled or a frame did not go
    /// out. After the last chunk (`all_chunks_out`) the `Failed` end is audited as `failed`.
    fn finish_out_lost(&self, id: &TransferId, mut out: Out) {
        out.audit_failed = out.all_chunks_out;
        self.finish_out(id, out);
    }

    /// Ends a send the service gave up on (a failed send of a frame).
    fn fail_out(&self, id: &TransferId, outcome: Outcome) {
        if let Some(mut out) = self.take_out(id) {
            out.machine.fail_with(outcome);
            self.finish_out_lost(id, out);
        }
    }

    /// Cancels a send. `Ok(false)` means there was no such send. After the last chunk the
    /// cancel is refused, because the receiver may already be importing.
    async fn cancel_outgoing(&self, id: &TransferId) -> DomainResult<bool> {
        let taken = {
            let mut inner = self.lock();
            let waiting = match inner.outgoing.get(id) {
                None => return Ok(false),
                Some(out) => out.machine.last_chunk_built(),
            };
            if waiting {
                return Err(already_sent());
            }
            inner.take_out(id)
        };
        let Some(mut out) = taken else {
            return Ok(false);
        };
        let frame = out.machine.cancel();
        let device = out.device;
        self.finish_out(id, out);
        if let Some(frame) = frame {
            self.tell(&device, frame).await;
        }
        Ok(true)
    }

    // ----- timers and the stream -----

    fn spawn_offer_timer(&self, id: TransferId) {
        let Some(me) = self.arc() else {
            return;
        };
        let ttl = self.timing.offer_ttl;
        drop(tokio::spawn(async move {
            tokio::time::sleep(ttl).await;
            me.expire_out(&id);
        }));
    }

    /// Ends an unanswered offer when the machine says it is due.
    fn expire_out(&self, id: &TransferId) {
        let now = Self::now();
        let due = self
            .lock()
            .outgoing
            .get_mut(id)
            .is_some_and(|out| out.machine.expire_if_due(now));
        if !due {
            return;
        }
        if let Some(out) = self.take_out(id) {
            let device = out.device;
            let frame = out.machine.abort_message(AbortReason::Expired);
            self.finish_out(id, out);
            self.tell_later(device, frame);
        }
    }

    fn spawn_stream(&self, id: TransferId) {
        let Some(me) = self.arc() else {
            return;
        };
        drop(tokio::spawn(async move { me.stream(&id).await }));
    }

    /// Gives up on a result that never comes. Started by the receiver's final `ChunkAck`, so
    /// the receiver has every byte and the send ends `Unconfirmed`.
    fn spawn_result_timer(&self, id: TransferId) {
        let Some(me) = self.arc() else {
            return;
        };
        let wait = self.timing.result_timeout;
        drop(tokio::spawn(async move {
            tokio::time::sleep(wait).await;
            if let Some(mut out) = me.take_out_if(&id, |o| o.machine.awaiting_result()) {
                out.machine.result_timed_out();
                me.finish_out(&id, out);
            }
        }));
    }

    /// Guards the wait for the final `ChunkAck` after the last chunk. When no ack of any kind
    /// arrives for `timing.idle_limit` the send ends `Failed` (audited `failed`), never
    /// `Unconfirmed`. Stops as soon as the final ack arrived or the send ended.
    fn spawn_final_ack_timer(&self, id: TransferId) {
        let Some(me) = self.arc() else {
            return;
        };
        drop(tokio::spawn(async move {
            loop {
                let deadline = {
                    let inner = me.lock();
                    match inner.outgoing.get(&id) {
                        Some(o) if !o.machine.awaiting_result() => {
                            o.last_ack + me.timing.idle_limit
                        }
                        _ => return,
                    }
                };
                tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
                if me.ack_overdue(&id) {
                    return;
                }
            }
        }));
    }

    /// Sends every chunk in order, at most `MAX_UNACKED_CHUNKS` ahead of the receiver's acks
    /// (ruling 19). Each send is bounded by the connection's own timeout.
    async fn stream(&self, id: &TransferId) {
        loop {
            #[cfg(test)]
            {
                let delay = *self.chunk_delay.lock().unwrap_or_else(|e| e.into_inner());
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
            }
            #[cfg(test)]
            let tamper = *self.tamper.lock().unwrap_or_else(|e| e.into_inner());
            #[cfg(not(test))]
            let tamper: Option<u32> = None;
            let step = {
                let mut inner = self.lock();
                let Some(out) = inner.outgoing.get_mut(id) else {
                    return;
                };
                if !out.machine.window_open() && out.machine.is_active() {
                    StreamStep::Wait(out.ack_signal.clone(), out.last_ack + self.timing.idle_limit)
                } else {
                    let start = out.machine.done_bytes() as usize;
                    let end = (start + CHUNK_BYTES).min(out.zip.len());
                    // The tamper hook sends a changed copy of one chunk; the ZIP stays intact.
                    let mut copy: Vec<u8>;
                    let raw: &[u8] = if tamper == Some((start / CHUNK_BYTES) as u32) {
                        copy = out.zip[start..end].to_vec();
                        if let Some(byte) = copy.first_mut() {
                            *byte ^= 0xFF;
                        }
                        &copy
                    } else {
                        &out.zip[start..end]
                    };
                    match out.machine.next_chunk(raw) {
                        Ok(frame) => StreamStep::Send {
                            device: out.device,
                            frame,
                            done: out.machine.done_bytes(),
                            total: out.machine.total_bytes(),
                            last: {
                                out.all_chunks_out = out.machine.last_chunk_built();
                                out.all_chunks_out
                            },
                        },
                        // The machine says the transfer is not streaming (ended meanwhile).
                        Err(_) => return,
                    }
                }
            };
            let (device, frame, done, total, last) = match step {
                StreamStep::Wait(signal, deadline) => {
                    tokio::select! {
                        _ = signal.notified() => {}
                        _ = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => {
                            if self.ack_overdue(id) {
                                return;
                            }
                        }
                    }
                    continue;
                }
                StreamStep::Send {
                    device,
                    frame,
                    done,
                    total,
                    last,
                } => (device, frame, done, total, last),
            };
            if self.deps.network.send_to(&device, frame).await.is_err() {
                self.fail_out(id, Outcome::Failed);
                return;
            }
            {
                // The entry may have left while the frame was in flight. Then nothing more is said.
                let inner = self.lock();
                if !inner.outgoing.contains_key(id) {
                    return;
                }
                self.emit(TransferEvent::Progress {
                    transfer_id: id.as_str().to_string(),
                    direction: Direction::Send,
                    done_bytes: done,
                    total_bytes: total,
                });
            }
            if last {
                // No result timer yet: the receiver's final `ChunkAck` starts it. Until then
                // the idle limit guards the missing ack and a stall ends `Failed`.
                self.spawn_final_ack_timer(id.clone());
                return;
            }
        }
    }

    /// The idle deadline of a full window passed. True when the send ended here (no ack came
    /// meanwhile): it is `Failed` and the peer is told `failed`.
    fn ack_overdue(&self, id: &TransferId) -> bool {
        let now = Self::now();
        let limit = self.timing.idle_limit;
        let Some(out) = self.take_out_if(id, |o| {
            !o.machine.window_open()
                && !o.machine.awaiting_result()
                && o.last_ack + limit <= now
        }) else {
            // An ack came first, or the send already ended: the loop looks again.
            return !self.lock().outgoing.contains_key(id);
        };
        let mut out = out;
        out.machine.fail_with(Outcome::Failed);
        let device = out.device;
        let frame = out.machine.abort_message(AbortReason::Failed);
        self.finish_out_lost(id, out);
        self.tell_later(device, frame);
        true
    }
}
```

- [ ] **Step 8: Register the modules and the re-exports**

In `crates/rocket-app/src/network/mod.rs`, add to the module list `mod transfer;` and `mod transfer_send;`, and next to the other `pub use` lines:

```rust
pub use transfer::{PeerSender, Timing, TransferDeps, TransferService};
pub use transfer_send::SendStarted;
```

In `crates/rocket-app/src/lib.rs`, extend the `pub use network::{ ... }` block (the one that lists `NetworkDeps, NetworkHandle, NetworkService`) with `PeerSender, SendStarted, Timing, TransferDeps, TransferService`. Run `cargo fmt -p rocket-app` afterwards so the list is sorted.

Check that Plan 04 will find the Plan 02 ports through `rocket_app`:
Run: `grep -n "transfer_ports\|CollectionImportPort" crates/rocket-app/src/network/mod.rs crates/rocket-app/src/lib.rs`
Expected: a `pub use` of `CollectionImportPort`, `ImportedCollection`, `StagedFile`, `StagedZip`, `TransferStaging` and `receiver_display_name`. If there is none, add `pub use transfer_ports::{receiver_display_name, CollectionImportPort, ImportedCollection, StagedFile, StagedZip, TransferStaging};` to `network/mod.rs` and the same names to the `lib.rs` block (Plan 04 needs them).

- [ ] **Step 9: Run the sender tests**

Run: `cargo test -j4 -p rocket-app transfer_send_tests`
Expected: PASS, the 46 tests in `network::transfer_send_tests` pass, 0 failed. If `the_sender_never_has_more_than_32_chunks_unacked` sees every chunk at once, `stream` does not ask `window_open()`; if it hangs after the first ack, `on_ack` did not `notify_one`. If `no_ack_for_the_idle_limit_fails_the_send` never ends, the wait has no deadline. If an `Unconfirmed` test sees `Failed`, `outgoing_gone` or the result timer still calls `fail_with(Failed)`; if a before-the-final-ack test sees `Unconfirmed` or no `failed` audit row, the result timer starts at the last chunk instead of at the final `ChunkAck`, or `finish_out_lost` does not set `audit_failed`. `a_collection_over_the_limit_is_refused_before_any_offer` may take a few seconds (it zips 10.5 MiB). If a paused-time test hangs, a guard is held across an `.await`. If `cancel_after_the_last_chunk_is_refused_and_the_send_still_completes` shows an abort frame or a `Cancelled`, `cancel_outgoing` did not check `last_chunk_built()` before taking the entry. If `an_accepted_offer_streams_every_chunk_and_completes_on_the_result` shows a `Finished` before the result frame, the stream ended the entry itself. If a late answer test shows a chunk, the service skipped the `on_answer` result. If `the_tamper_hook_flips_a_byte_of_the_chosen_chunk` sees the offered hash, the copy was made after `next_chunk`.

- [ ] **Step 10: Check the crate and the earlier tests**

Run: `cargo check -j4 -p rocket-app` then `cargo test -j4 -p rocket-app network`
Expected: Finished with no error (an unused-code warning for the receive-only parts is not expected yet; `Inner` has two fields), then PASS with 0 failed.

---

### Task 3: TransferService, receiver side

**Files:**
- Create: `crates/rocket-app/src/network/transfer_receive.rs`, `crates/rocket-app/src/network/transfer_receive_tests.rs`
- Modify: `crates/rocket-app/src/network/transfer.rs`, `crates/rocket-app/src/network/transfer_send.rs`, `crates/rocket-app/src/network/mod.rs`, `crates/rocket-app/CLAUDE.md`

**Interfaces:**

```rust
// Consumes (Plan 01, Plan 02, Task 2)
rocket_network::{IncomingTransfer, IncomingState, ChunkStep, Admission, OfferError, TransferLimits, TransferId, Outcome, AbortReason, ResultOutcome}   // ChunkStep::Data carries `ack: Option<PeerMessage>`
AuditEventKind::NetworkCollectionReceived { device_id, collection, outcome }   // outcome: completed | damaged | rejected
IncomingTransfer::{from_offer, with_offer_ttl, respond, on_chunk, on_abort, connection_lost, fail_with, expire_if_due, abort_message, name, sha256, done_bytes, total_bytes, is_pending, outcome}
TransferLimits::{admit_offer, mark_active, release}
super::transfer_ports::{receiver_display_name, StagedFile, StagedZip, TransferStaging, CollectionImportPort, UNSAFE_ZIP_MESSAGE}
TransferStaging::{create, release}                       // release: Plan 02, deletes a finished staged zip
super::transfer::Timing                                  // offer_ttl and idle_limit are used here

// Produces
impl TransferService {
    pub async fn respond(&self, transfer_id: &str, accept: bool) -> DomainResult<()>;
}
// items handled by the worker: CollectionOffer, Chunk, TransferAbort (incoming transfers), peer gone (incoming transfers)
// the receiver sends ChunkAck (through the per-transfer ack pump) every 16 chunks and after the last,
// and TransferResult { Completed | Damaged | Rejected | Failed } after the finalize step
```

Design notes:

- An offer from a device without a label (not paired, or blocked) is ignored. A transfer id that does not parse is ignored. A transfer id already known (pending or receiving) is `Admission::Duplicate` and ignored, so a duplicate frame gives one pending row and one `OfferIncoming`.
- Admission (ruling 12): `admit_offer` first (rate, per-peer count, global pending count; a duplicate costs nothing), then `IncomingTransfer::from_offer` (shape, size, chunk count) with `.with_offer_ttl(timing.offer_ttl)`. Refusals answer `TransferAbort` with the reason the machine names: `Admission::Refuse(reason)` (always `busy`) or `OfferError::abort_reason()` (`too_big` or `failed`). A refused offer leaves nothing in the limits.
- `respond(accept)` calls `IncomingTransfer::respond(accept, now)`. `None` means the offer expired or was answered already. On accept the service creates the staged file, stores it in the entry, calls `mark_active` and only then sends the `OfferAnswer` frame the machine returned, so no chunk can arrive before its record.
- A chunk goes to `IncomingTransfer::on_chunk`. `Ignored` does nothing. `Failed` removes the entry, discards the staged file if there is one and answers `TransferAbort { failed }`. `Data` is appended to the staged file (an append error ends the machine with `fail_with(Outcome::Failed)`), refreshes the idle clock, publishes `Progress` and, when `ack` is `Some`, puts the ack into the entry's ack pump (ruling 20), still under the lock so acks keep their order.
- After the last chunk (`Data { last: true }`) the entry stays in the map with `finalizing = true` (ruling 16) and the staged file goes to a spawned finalize task. That task finishes the file (`spawn_blocking`), checks that the machine was not failed meanwhile (an abort or a gone peer), compares the staged length with `size_bytes` and the SHA-256 (either mismatch is `Damaged`, ruling 18), imports in `spawn_blocking` (`existing_names` and `receiver_display_name` run there too) and ends with `Completed`, `Rejected` or `Failed` (ruling 8). Every path after `finish` calls `release(&file)` once (ruling 7). The end emits `Finished` (with `message` for `Rejected` and `Failed`), audits `Completed`, `Damaged` and `Rejected` with their outcome (ruling 9), and sends `TransferResult` to the sender from its own task. An interrupted transfer sends no result.
- Expiry of a pending offer (`expire_if_due`), the idle drop of a receiving transfer (`connection_lost`, after `timing.idle_limit`), the peer-gone notice and a peer abort (`on_abort`) each remove the entry before they emit, so the single `Finished` rule holds. A finalizing entry is never removed by them (ruling 16). A pending offer dropped by the peer-gone notice carries `message: Some("The other node went offline.")` (ruling 6); a peer `TransferAbort { cancelled }` for a pending offer ends it `Cancelled` and frees its slot.
- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

- [ ] **Step 2: Write the failing receiver tests**

Create `crates/rocket-app/src/network/transfer_receive_tests.rs`:

```rust
//! Receiver-side tests for `TransferService`, with fake peers, staging and importer.
//!
//! The rule tables (limits, chunk bounds, offer shape) are tested in `rocket-network`. These
//! tests prove that the service wires the machines: a frame in, the right event or frame out,
//! and the staged file freed on every exit.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use rocket_audit::AuditEventKind;
use rocket_network::limits::{MAX_TRANSFER_BYTES, TRANSFER_OFFER_TTL};
use rocket_network::limits::ACK_EVERY_CHUNKS;
use rocket_network::{
    chunk_count_for, AbortReason, DeviceId, Direction, Outcome, PeerMessage, ResultOutcome,
    TransferSink,
};

use super::test_support::{identity, settle};
use super::transfer::Timing;
use super::transfer_fakes::*;
use super::transfer_ports::UNSAFE_ZIP_MESSAGE;

const ANA: u8 = 2;
const BEN: u8 = 3;

fn dev(seed: u8) -> DeviceId {
    identity(seed).device_id
}

/// Four paired, online peers: Ana, Ben, Cy and Di.
fn rig_peers() -> Rig {
    let rig = rig();
    for (seed, name) in [(2u8, "Ana"), (3, "Ben"), (4, "Cy"), (5, "Di")] {
        rig.peers.add(dev(seed), name);
    }
    rig
}

async fn push(rig: &Rig, seed: u8, message: PeerMessage) {
    rig.svc.on_frame(dev(seed), message);
    settle().await;
}

/// Sends the offer and waits until it shows up as incoming.
async fn offer(rig: &Rig, seed: u8, id: &str, payload: &Payload) {
    let before = rig.events.incoming().len();
    rig.svc.on_frame(dev(seed), payload.offer(id, "orders"));
    wait_for("the incoming offer", || rig.events.incoming().len() > before).await;
}

fn feed(rig: &Rig, seed: u8, id: &str, payload: &Payload) {
    for seq in 0..payload.count() {
        rig.svc.on_frame(dev(seed), payload.chunk(id, seq));
    }
}

fn reasons(rig: &Rig, seed: u8) -> Vec<AbortReason> {
    rig.peers
        .sent_to(&dev(seed))
        .into_iter()
        .filter_map(|m| match m {
            PeerMessage::TransferAbort { reason, .. } => Some(reason),
            _ => None,
        })
        .collect()
}

async fn wait_for_finish(rig: &Rig) {
    wait_for("a finished event", || !rig.events.finished().is_empty()).await;
}

/// The `TransferResult` outcomes sent to a peer.
fn results(rig: &Rig, seed: u8) -> Vec<ResultOutcome> {
    rig.peers
        .sent_to(&dev(seed))
        .into_iter()
        .filter_map(|m| match m {
            PeerMessage::TransferResult { outcome, .. } => Some(outcome),
            _ => None,
        })
        .collect()
}

/// The result frame leaves from its own task, so it can trail the `Finished` event.
async fn wait_for_result(rig: &Rig, seed: u8) {
    wait_for("the result frame", || !results(rig, seed).is_empty()).await;
}

/// The `ChunkAck` numbers sent to a peer, in order.
fn acks(rig: &Rig, seed: u8) -> Vec<u32> {
    rig.peers
        .sent_to(&dev(seed))
        .into_iter()
        .filter_map(|m| match m {
            PeerMessage::ChunkAck { next_seq, .. } => Some(next_seq),
            _ => None,
        })
        .collect()
}

/// The audit row a receive from Ana writes.
fn received_row(collection: &str, outcome: &str) -> AuditEventKind {
    AuditEventKind::NetworkCollectionReceived {
        device_id: dev(ANA).short(),
        collection: collection.into(),
        outcome: outcome.into(),
    }
}

fn manual_offer(id: &str, name: &str, size: u64, count: u32, sha: &str) -> PeerMessage {
    PeerMessage::CollectionOffer {
        transfer_id: id.to_string(),
        name: name.to_string(),
        size_bytes: size,
        chunk_count: count,
        sha256: sha.to_string(),
    }
}

#[tokio::test(start_paused = true)]
async fn an_offer_shows_up_once() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);

    offer(&rig, ANA, &id, &payload).await;
    push(&rig, ANA, payload.offer(&id, "orders")).await;

    assert_eq!(
        rig.events.incoming(),
        vec![(
            id.clone(),
            "Ana".to_string(),
            dev(ANA).short(),
            "orders".to_string(),
            payload.bytes.len() as u64
        )],
        "a duplicate offer adds no second row"
    );
    assert_eq!(rig.svc.counts(), (1, 0));
    assert_eq!(rig.staging.counters.created(), 0, "nothing staged before Accept");
    assert!(rig.importer.calls().is_empty(), "nothing imported before Accept");
}

#[tokio::test]
async fn accept_stages_imports_with_the_display_name_and_cleans_up() {
    let rig = rig_peers();
    *rig.importer.existing.lock().expect("existing lock") = vec!["orders".to_string()];
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    rig.svc.respond(&id, true).await.expect("accepts");
    assert!(rig.peers.sent_to(&dev(ANA)).contains(&answer(&id, true)));
    assert_eq!(rig.staging.counters.created(), 1);
    feed(&rig, ANA, &id, &payload);
    wait_for_finish(&rig).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id.clone(), Direction::Receive, Outcome::Completed)]
    );
    let calls = rig.importer.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, PathBuf::from(format!("/staged/{id}.zip")));
    assert_eq!(calls[0].1, "orders (from Ana)");
    assert_eq!(rig.audited(), vec![received_row("orders (from Ana)", "completed")]);
    wait_for_result(&rig, ANA).await;
    assert_eq!(results(&rig, ANA), vec![ResultOutcome::Completed]);
    assert!(reasons(&rig, ANA).is_empty(), "a result, not an abort");
    assert_eq!(rig.events.finished_messages(), vec![None]);
    let size = payload.bytes.len() as u64;
    assert_eq!(
        rig.events.progress().last().copied(),
        Some((Direction::Receive, size, size))
    );
    assert_eq!(rig.staging.counters.finished(), 1);
    assert_eq!(rig.staging.counters.released(), 1, "the finished zip is released");
    rig.staging.assert_clean();
    assert_eq!(rig.svc.counts(), (0, 0));
}

#[tokio::test]
async fn a_free_name_is_imported_as_offered() {
    let rig = rig_peers();
    let payload = Payload::new(40 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    rig.svc.respond(&id, true).await.expect("accepts");
    feed(&rig, ANA, &id, &payload);
    wait_for_finish(&rig).await;

    assert_eq!(rig.importer.calls()[0].1, "orders");
}

#[tokio::test(start_paused = true)]
async fn decline_answers_false_and_finishes_declined() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    rig.svc.respond(&id, false).await.expect("declines");

    assert!(rig.peers.sent_to(&dev(ANA)).contains(&answer(&id, false)));
    assert_eq!(
        rig.events.finished(),
        vec![(id.clone(), Direction::Receive, Outcome::Declined)]
    );
    assert_eq!(rig.staging.counters.created(), 0);
    assert!(rig.svc.respond(&id, true).await.is_err(), "answered once");
}

#[tokio::test(start_paused = true)]
async fn an_unanswered_offer_expires() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    tokio::time::advance(TRANSFER_OFFER_TTL + Duration::from_secs(1)).await;
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id.clone(), Direction::Receive, Outcome::Expired)]
    );
    assert_eq!(reasons(&rig, ANA), vec![AbortReason::Expired]);
    assert_eq!(rig.svc.counts(), (0, 0));
    assert!(rig.svc.respond(&id, true).await.is_err());
    assert_eq!(rig.staging.counters.created(), 0);
}

#[tokio::test(start_paused = true)]
async fn accepting_after_expiry_sends_nothing_and_reports_expired() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    tokio::time::advance(TRANSFER_OFFER_TTL).await;
    let err = rig
        .svc
        .respond(&id, true)
        .await
        .expect_err("the offer is over");
    settle().await;

    assert!(
        matches!(&err, rocket_shared::error::DomainError::InvalidInput(m) if m == "The request expired.")
    );
    assert!(
        !rig.peers.sent_to(&dev(ANA)).contains(&answer(&id, true)),
        "no accept goes out"
    );
    assert_eq!(rig.staging.counters.created(), 0);
    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Expired)],
        "one result, whichever side saw the deadline first"
    );
}

#[tokio::test(start_paused = true)]
async fn the_fourth_pending_offer_is_busy() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    offer(&rig, 2, &tid(1), &payload).await;
    offer(&rig, 3, &tid(2), &payload).await;
    offer(&rig, 4, &tid(3), &payload).await;

    push(&rig, 5, payload.offer(&tid(4), "orders")).await;

    assert_eq!(reasons(&rig, 5), vec![AbortReason::Busy]);
    assert_eq!(rig.events.incoming().len(), 3);
    assert_eq!(rig.svc.counts(), (3, 0));
}

#[tokio::test(start_paused = true)]
async fn a_second_offer_from_the_same_peer_is_busy() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    offer(&rig, ANA, &tid(1), &payload).await;

    push(&rig, ANA, payload.offer(&tid(2), "orders")).await;
    assert_eq!(reasons(&rig, ANA), vec![AbortReason::Busy], "first still pending");

    rig.svc.respond(&tid(1), true).await.expect("accepts");
    push(&rig, ANA, payload.offer(&tid(3), "orders")).await;
    assert_eq!(
        reasons(&rig, ANA),
        vec![AbortReason::Busy, AbortReason::Busy],
        "first is receiving"
    );
    assert_eq!(rig.events.incoming().len(), 1, "only the first was shown");
}

#[tokio::test(start_paused = true)]
async fn refused_offers_are_answered_with_the_reason_of_the_check() {
    let rig = rig_peers();
    let sha = "a".repeat(64);
    let too_big = MAX_TRANSFER_BYTES + 1;

    push(
        &rig,
        2,
        manual_offer(&tid(1), "orders", too_big, chunk_count_for(too_big), &sha),
    )
    .await;
    push(&rig, 3, manual_offer(&tid(2), "orders", 100, 5, &sha)).await;
    push(&rig, 4, manual_offer("not-an-id", "orders", 100, 1, &sha)).await;

    assert_eq!(reasons(&rig, 2), vec![AbortReason::TooBig]);
    assert_eq!(reasons(&rig, 3), vec![AbortReason::Failed], "a lying chunk count");
    assert!(
        rig.peers.sent_to(&dev(4)).is_empty(),
        "a bad id cannot be named in a reply"
    );
    assert!(rig.events.incoming().is_empty());
    assert_eq!(rig.svc.counts(), (0, 0), "a refused offer leaves nothing behind");
}

#[tokio::test(start_paused = true)]
async fn offers_from_unpaired_peers_are_ignored() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);

    push(&rig, 9, payload.offer(&tid(1), "orders")).await;
    rig.peers.forget(&dev(ANA));
    push(&rig, ANA, payload.offer(&tid(2), "orders")).await;

    assert!(rig.events.all().is_empty());
    assert!(rig.peers.sent().is_empty(), "not even an abort goes out");
}

#[tokio::test(start_paused = true)]
async fn chunks_before_accept_abort_the_transfer() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    push(&rig, ANA, payload.chunk(&id, 0)).await;

    assert_eq!(reasons(&rig, ANA), vec![AbortReason::Failed]);
    assert_eq!(
        rig.events.finished(),
        vec![(id.clone(), Direction::Receive, Outcome::Failed)]
    );
    assert_eq!(rig.staging.counters.created(), 0);
    assert!(rig.svc.respond(&id, true).await.is_err());
}

#[tokio::test(start_paused = true)]
async fn a_refused_chunk_fails_the_transfer_and_cleans_up() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");

    // Chunk 1 before chunk 0: the machine refuses it, the service must clean up.
    push(&rig, ANA, payload.chunk(&id, 1)).await;

    assert_eq!(reasons(&rig, ANA), vec![AbortReason::Failed]);
    assert_eq!(
        rig.events.finished(),
        vec![(id.clone(), Direction::Receive, Outcome::Failed)]
    );
    assert_eq!(rig.staging.counters.discarded(), 1);
    rig.staging.assert_clean();
    assert_eq!(rig.svc.counts(), (0, 0));

    // Garbage that is not base64 ends the next transfer the same way.
    let next = tid(2);
    offer(&rig, ANA, &next, &payload).await;
    rig.svc.respond(&next, true).await.expect("accepts");
    push(
        &rig,
        ANA,
        PeerMessage::Chunk {
            transfer_id: next.clone(),
            seq: 0,
            data: "not base64!".to_string(),
        },
    )
    .await;
    assert_eq!(rig.events.finished().len(), 2);
    assert_eq!(rig.staging.counters.discarded(), 2);
    rig.staging.assert_clean();
}

#[tokio::test]
async fn a_tampered_payload_is_damaged() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let mut tampered = payload.clone();
    tampered.bytes[50] ^= 0xFF;
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    rig.svc.respond(&id, true).await.expect("accepts");
    feed(&rig, ANA, &id, &tampered);
    wait_for_finish(&rig).await;
    wait_for_result(&rig, ANA).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Damaged)]
    );
    assert!(rig.importer.calls().is_empty(), "a damaged file is never imported");
    assert_eq!(results(&rig, ANA), vec![ResultOutcome::Damaged]);
    assert!(reasons(&rig, ANA).is_empty(), "the verdict is a result, not an abort");
    assert_eq!(
        rig.audited(),
        vec![received_row("orders", "damaged")],
        "a tampered payload is evidence"
    );
    assert_eq!(rig.staging.counters.released(), 1, "the damaged zip is released");
    rig.staging.assert_clean();
}

#[tokio::test]
async fn a_rejected_zip_is_reported_rejected() {
    let rig = rig_peers();
    rig.importer.set_mode(ImportMode::Reject);
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    rig.svc.respond(&id, true).await.expect("accepts");
    feed(&rig, ANA, &id, &payload);
    wait_for_finish(&rig).await;
    wait_for_result(&rig, ANA).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Rejected)]
    );
    assert_eq!(
        rig.events.finished_messages(),
        vec![Some(UNSAFE_ZIP_MESSAGE.to_string())]
    );
    assert_eq!(rig.importer.calls().len(), 1);
    assert_eq!(results(&rig, ANA), vec![ResultOutcome::Rejected]);
    assert_eq!(
        rig.audited(),
        vec![received_row("orders", "rejected")],
        "a hostile ZIP is evidence"
    );
    rig.staging.assert_clean();
}

#[tokio::test]
async fn an_import_failure_is_reported_failed() {
    let rig = rig_peers();
    rig.importer.set_mode(ImportMode::Fail);
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    rig.svc.respond(&id, true).await.expect("accepts");
    feed(&rig, ANA, &id, &payload);
    wait_for_finish(&rig).await;
    wait_for_result(&rig, ANA).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Failed)]
    );
    let messages = rig.events.finished_messages();
    assert!(
        matches!(&messages[0], Some(text) if text.contains("import failed")),
        "the importer's text reaches the event: {messages:?}"
    );
    assert_eq!(results(&rig, ANA), vec![ResultOutcome::Failed]);
    assert!(rig.audited().is_empty());
    rig.staging.assert_clean();
}

#[tokio::test]
async fn invalid_input_with_another_text_is_a_failure_not_a_rejection() {
    let rig = rig_peers();
    rig.importer.set_mode(ImportMode::OtherInvalid);
    let payload = Payload::new(40 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    rig.svc.respond(&id, true).await.expect("accepts");
    feed(&rig, ANA, &id, &payload);
    wait_for_finish(&rig).await;
    wait_for_result(&rig, ANA).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Failed)],
        "only the exact unsafe-ZIP text is a rejection"
    );
    let messages = rig.events.finished_messages();
    assert!(matches!(&messages[0], Some(text) if text.contains("bad folder layout")));
    assert_eq!(results(&rig, ANA), vec![ResultOutcome::Failed]);
    rig.staging.assert_clean();
}

#[tokio::test]
async fn a_failing_release_does_not_change_the_result() {
    let rig = rig_peers();
    rig.staging.fail_release.store(true, Ordering::SeqCst);
    let payload = Payload::new(40 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    rig.svc.respond(&id, true).await.expect("accepts");
    feed(&rig, ANA, &id, &payload);
    wait_for_finish(&rig).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Completed)],
        "the import succeeded, a stuck temp file is only logged"
    );
    assert_eq!(rig.audited().len(), 1);
    assert_eq!(rig.staging.counters.released(), 1, "release was tried once");
}

#[tokio::test]
async fn a_chunk_after_the_transfer_finished_is_ignored() {
    let rig = rig_peers();
    let payload = Payload::new(40 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");
    feed(&rig, ANA, &id, &payload);
    wait_for_finish(&rig).await;
    wait_for_result(&rig, ANA).await;
    let events = rig.events.all().len();
    let sent = rig.peers.sent().len();

    rig.svc.on_frame(dev(ANA), payload.chunk(&id, 0));
    rig.svc
        .on_frame(dev(ANA), payload.chunk(&tid(9), 0));
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(rig.events.all().len(), events, "no new event");
    assert_eq!(rig.peers.sent().len(), sent, "no new frame");
    assert_eq!(rig.staging.counters.created(), 1, "no new staged file");
    assert_eq!(rig.importer.calls().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_peer_abort_while_receiving_cleans_up() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");
    push(&rig, ANA, payload.chunk(&id, 0)).await;

    push(&rig, ANA, abort(&id, AbortReason::Cancelled)).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Cancelled)]
    );
    assert_eq!(rig.staging.counters.discarded(), 1);
    rig.staging.assert_clean();
    assert_eq!(rig.svc.counts(), (0, 0));
}

#[tokio::test(start_paused = true)]
async fn cancel_while_receiving_aborts_and_cleans_up() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");
    push(&rig, ANA, payload.chunk(&id, 0)).await;

    rig.svc.cancel(&id).await.expect("cancels");
    push(&rig, ANA, payload.chunk(&id, 1)).await;

    assert_eq!(reasons(&rig, ANA), vec![AbortReason::Cancelled]);
    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Cancelled)]
    );
    assert_eq!(rig.staging.counters.discarded(), 1);
    rig.staging.assert_clean();
}

#[tokio::test(start_paused = true)]
async fn a_silent_peer_drops_the_receive_after_the_idle_limit() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");
    push(&rig, ANA, payload.chunk(&id, 0)).await;
    let almost = Timing::default().idle_limit - Duration::from_secs(5);

    tokio::time::advance(almost).await;
    settle().await;
    assert!(rig.events.finished().is_empty(), "still within the limit");
    push(&rig, ANA, payload.chunk(&id, 1)).await;

    tokio::time::advance(almost).await;
    settle().await;
    assert!(rig.events.finished().is_empty(), "a chunk restarts the clock");

    tokio::time::advance(Duration::from_secs(10)).await;
    settle().await;
    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Failed)]
    );
    assert_eq!(reasons(&rig, ANA), vec![AbortReason::Failed]);
    assert_eq!(rig.staging.counters.discarded(), 1);
    rig.staging.assert_clean();
    assert_eq!(rig.svc.counts(), (0, 0));
}

#[tokio::test(start_paused = true)]
async fn an_abort_from_another_device_is_ignored() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    push(&rig, BEN, abort(&id, AbortReason::Cancelled)).await;
    push(&rig, BEN, payload.chunk(&id, 0)).await;

    assert!(rig.events.finished().is_empty());
    assert_eq!(rig.svc.counts(), (1, 0), "Ana's offer is untouched");
}

#[tokio::test(start_paused = true)]
async fn a_peer_can_offer_again_after_its_transfer_ended() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    offer(&rig, ANA, &tid(1), &payload).await;
    rig.svc.respond(&tid(1), false).await.expect("declines");

    offer(&rig, ANA, &tid(2), &payload).await;

    assert_eq!(rig.events.incoming().len(), 2, "the first transfer freed the peer's slot");
    assert!(reasons(&rig, ANA).is_empty(), "no busy answer");
}

#[tokio::test(start_paused = true)]
async fn peer_gone_drops_a_pending_offer() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    rig.svc.on_peer_gone(dev(ANA));
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Failed)]
    );
    assert_eq!(
        rig.events.finished_messages(),
        vec![Some("The other node went offline.".to_string())],
        "the receiver never accepted, so it is not a receive failure"
    );
    assert_eq!(rig.svc.counts(), (0, 0));
    assert!(rig.peers.sent().is_empty(), "nothing is told to a gone peer");
    assert_eq!(rig.staging.counters.created(), 0);
}

#[tokio::test(start_paused = true)]
async fn peer_gone_drops_a_receiving_transfer_at_once() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");
    push(&rig, ANA, payload.chunk(&id, 0)).await;

    // No advance of the clock: the idle limit is only the fallback.
    rig.svc.on_peer_gone(dev(ANA));
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Failed)]
    );
    assert_eq!(rig.staging.counters.discarded(), 1);
    rig.staging.assert_clean();
    assert_eq!(rig.svc.counts(), (0, 0));
    assert!(reasons(&rig, ANA).is_empty(), "nothing is told to a gone peer");
    assert_eq!(rig.events.finished_messages(), vec![None], "an accepted receive just failed");
}

#[tokio::test(start_paused = true)]
async fn peer_gone_for_another_device_changes_nothing() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    offer(&rig, ANA, &tid(1), &payload).await;

    rig.svc.on_peer_gone(dev(BEN));
    settle().await;

    assert!(rig.events.finished().is_empty());
    assert_eq!(rig.svc.counts(), (1, 0), "Ana's offer is untouched");
}

#[tokio::test]
async fn an_abort_during_the_finish_stops_the_import() {
    let rig = rig_peers();
    let release = rig.staging.hold_finish();
    let payload = Payload::new(40 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");
    feed(&rig, ANA, &id, &payload);
    wait_for("the finish to start", || rig.staging.counters.finished() == 1).await;

    rig.svc.on_frame(dev(ANA), abort(&id, AbortReason::Cancelled));
    tokio::time::sleep(Duration::from_millis(50)).await;
    release.send(()).expect("the finish waits");
    wait_for_finish(&rig).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Cancelled)]
    );
    assert!(rig.importer.calls().is_empty(), "an aborted transfer is never imported");
    assert!(rig.audited().is_empty());
    assert!(results(&rig, ANA).is_empty(), "the sender aborted, it needs no result");
    assert_eq!(rig.staging.counters.released(), 1);
    rig.staging.assert_clean();
    assert_eq!(rig.svc.counts(), (0, 0));
}

#[tokio::test]
async fn peer_gone_during_the_finish_stops_the_import() {
    let rig = rig_peers();
    let release = rig.staging.hold_finish();
    let payload = Payload::new(40 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");
    feed(&rig, ANA, &id, &payload);
    wait_for("the finish to start", || rig.staging.counters.finished() == 1).await;

    rig.svc.on_peer_gone(dev(ANA));
    tokio::time::sleep(Duration::from_millis(50)).await;
    release.send(()).expect("the finish waits");
    wait_for_finish(&rig).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Failed)]
    );
    assert!(rig.importer.calls().is_empty());
    assert!(results(&rig, ANA).is_empty(), "nobody is left to tell");
    rig.staging.assert_clean();
    assert_eq!(rig.svc.counts(), (0, 0));
}

#[tokio::test]
async fn a_slow_import_does_not_block_other_peers() {
    let rig = rig_peers();
    let release = rig.importer.hold_import();
    let payload = Payload::new(40 * 1024);
    offer(&rig, ANA, &tid(1), &payload).await;
    rig.svc.respond(&tid(1), true).await.expect("accepts");
    feed(&rig, ANA, &tid(1), &payload);
    wait_for("the import to start", || !rig.importer.calls().is_empty()).await;

    // The import is parked. Another peer's offer must still be handled.
    offer(&rig, BEN, &tid(2), &payload).await;
    assert_eq!(rig.events.incoming().len(), 2);
    assert!(rig.events.finished().is_empty());

    release.send(()).expect("the import waits");
    wait_for_finish(&rig).await;
    assert_eq!(
        rig.events.finished(),
        vec![(tid(1), Direction::Receive, Outcome::Completed)]
    );
}

#[tokio::test(start_paused = true)]
async fn a_short_offer_ttl_expires_a_pending_offer_at_that_ttl() {
    let timing = Timing {
        offer_ttl: Duration::from_secs(5),
        ..Timing::default()
    };
    let rig = rig_timed(default_files(), timing);
    rig.peers.add(dev(ANA), "Ana");
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    tokio::time::advance(Duration::from_secs(6)).await;
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Expired)],
        "the machine and the timer both follow the short ttl"
    );
    assert_eq!(reasons(&rig, ANA), vec![AbortReason::Expired]);
}

#[tokio::test(start_paused = true)]
async fn a_short_idle_limit_drops_the_receive_at_that_limit() {
    let timing = Timing {
        idle_limit: Duration::from_secs(10),
        ..Timing::default()
    };
    let rig = rig_timed(default_files(), timing);
    rig.peers.add(dev(ANA), "Ana");
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");
    push(&rig, ANA, payload.chunk(&id, 0)).await;

    tokio::time::advance(Duration::from_secs(11)).await;
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Failed)]
    );
    rig.staging.assert_clean();
}

#[tokio::test]
async fn the_receiver_acks_every_16_chunks_and_the_last() {
    let rig = rig_peers();
    // 40 chunks: acks at 16, 32 and 40.
    let payload = Payload::new(40 * 32 * 1024 - 100);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");

    for seq in 0..15 {
        rig.svc.on_frame(dev(ANA), payload.chunk(&id, seq));
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(acks(&rig, ANA).is_empty(), "no ack before the 16th chunk");

    for seq in 15..payload.count() {
        rig.svc.on_frame(dev(ANA), payload.chunk(&id, seq));
    }
    wait_for_finish(&rig).await;
    wait_for("the last ack", || acks(&rig, ANA).len() == 3).await;

    assert_eq!(payload.count(), 40);
    assert_eq!(
        acks(&rig, ANA),
        vec![ACK_EVERY_CHUNKS, 2 * ACK_EVERY_CHUNKS, 40],
        "in order, one every 16 chunks and one for the last"
    );
}

#[tokio::test]
async fn a_wrong_staged_length_is_damaged() {
    let rig = rig_peers();
    rig.staging.wrong_len.store(true, Ordering::SeqCst);
    let payload = Payload::new(40 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    rig.svc.respond(&id, true).await.expect("accepts");
    feed(&rig, ANA, &id, &payload);
    wait_for_finish(&rig).await;
    wait_for_result(&rig, ANA).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Damaged)],
        "the hash fits, the size does not"
    );
    assert!(rig.importer.calls().is_empty());
    assert_eq!(results(&rig, ANA), vec![ResultOutcome::Damaged]);
    rig.staging.assert_clean();
}

#[tokio::test(start_paused = true)]
async fn a_sender_cancel_of_a_pending_offer_closes_it() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    push(&rig, ANA, abort(&id, AbortReason::Cancelled)).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id.clone(), Direction::Receive, Outcome::Cancelled)]
    );
    assert_eq!(rig.svc.counts(), (0, 0));
    assert!(rig.peers.sent().is_empty(), "the sender ended it, nothing goes back");
    let err = rig.svc.respond(&id, true).await.expect_err("the offer is gone");
    assert!(
        matches!(&err, rocket_shared::error::DomainError::InvalidInput(m) if m == "The request expired.")
    );
    offer(&rig, ANA, &tid(2), &payload).await;
    assert!(reasons(&rig, ANA).is_empty(), "the peer's slot was freed, no busy");
}

#[tokio::test(start_paused = true)]
async fn cancelling_a_pending_offer_declines_it() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    rig.svc.cancel(&id).await.expect("cancels");

    assert!(rig.peers.sent_to(&dev(ANA)).contains(&answer(&id, false)));
    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Declined)]
    );
    assert_eq!(rig.svc.counts(), (0, 0));
    assert_eq!(rig.staging.counters.created(), 0);
}

#[tokio::test]
async fn a_dropped_chunk_fails_both_sides_at_the_next_chunk() {
    let sender = rig();
    let receiver = rig();
    let mut link = Link::new(&sender, dev(ANA), "Ana", &receiver, dev(BEN), "Ben");
    let id = sender
        .svc
        .send(&dev(BEN), "orders", false)
        .await
        .expect("the send starts")
        .transfer_id;
    let no_loss = |_: &PeerMessage| false;
    link.run_until(&no_loss, || !receiver.events.incoming().is_empty()).await;
    receiver.svc.respond(&id, true).await.expect("accepts");

    // Chunk 1 is lost on the way; chunk 2 then breaks the sequence rule.
    let lose_one = |m: &PeerMessage| matches!(m, PeerMessage::Chunk { seq: 1, .. });
    link.run_until(&lose_one, || {
        !sender.events.finished().is_empty() && !receiver.events.finished().is_empty()
    })
    .await;

    assert_eq!(
        receiver.events.finished(),
        vec![(id.clone(), Direction::Receive, Outcome::Failed)]
    );
    assert_eq!(
        sender.events.finished(),
        vec![(id, Direction::Send, Outcome::Failed)]
    );
    assert!(receiver.importer.calls().is_empty());
    receiver.staging.assert_clean();
    assert!(sender.audited().is_empty());
}

#[tokio::test]
async fn a_stalled_stream_fails_both_sides_within_the_idle_limit() {
    // Real clock, so the limit is short.
    let timing = Timing {
        idle_limit: Duration::from_millis(500),
        ..Timing::default()
    };
    let sender = rig_timed(default_files(), timing);
    let receiver = rig_timed(default_files(), timing);
    let mut link = Link::new(&sender, dev(ANA), "Ana", &receiver, dev(BEN), "Ben");
    let id = sender
        .svc
        .send(&dev(BEN), "orders", false)
        .await
        .expect("the send starts")
        .transfer_id;
    let no_loss = |_: &PeerMessage| false;
    link.run_until(&no_loss, || !receiver.events.incoming().is_empty()).await;
    receiver.svc.respond(&id, true).await.expect("accepts");

    // Every chunk after the first is lost: a lost tail, or a socket swapped by the duplicate
    // rule. Nothing more arrives, so only the idle limit can end it.
    let lose_tail = |m: &PeerMessage| matches!(m, PeerMessage::Chunk { seq, .. } if *seq >= 1);
    let started = std::time::Instant::now();
    link.run_until(&lose_tail, || {
        !sender.events.finished().is_empty() && !receiver.events.finished().is_empty()
    })
    .await;

    assert!(started.elapsed() >= Duration::from_millis(500), "the idle limit ended it");
    assert_eq!(
        receiver.events.finished(),
        vec![(id.clone(), Direction::Receive, Outcome::Failed)]
    );
    assert_eq!(
        sender.events.finished(),
        vec![(id, Direction::Send, Outcome::Failed)],
        "the receiver's failed reached the sender before its 60 s result wait"
    );
    receiver.staging.assert_clean();
}
```

In `crates/rocket-app/src/network/mod.rs`, add the test module:

```rust
#[cfg(test)]
#[path = "transfer_receive_tests.rs"]
mod transfer_receive_tests;
```

- [ ] **Step 3: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket-app transfer_receive_tests`
Expected: FAIL to compile, `error[E0599]: no method named `respond` found for struct `Arc<TransferService>`` (also `counts`). The Task 1 and Task 2 tests still compile. (Written against the final behaviour: a receiver without the ack pump fails `the_receiver_acks_every_16_chunks_and_the_last`, and one without the length check fails `a_wrong_staged_length_is_damaged` with `Completed`.)

- [ ] **Step 4: Write `transfer_receive.rs`**

Create `crates/rocket-app/src/network/transfer_receive.rs`:

```rust
//! The receiver side of a transfer, around one `IncomingTransfer` per offer.
//!
//! The machine decides what is allowed: the shape of an offer, expiry, the order and size of
//! chunks. `TransferLimits` decides how many offers are open. The service stages the bytes,
//! checks the hash, imports, publishes events and audit, sends the `TransferResult` and runs
//! the timers. An offer is pending until the user answers; nothing is written to the workspace
//! before Accept, and the import never overwrites. The handlers that run on the frame worker
//! are sync; the finalize step and the outbound frames run as their own tasks.

use std::sync::Arc;
use std::time::{Duration, Instant};

use rocket_audit::AuditEventKind;
use rocket_network::{
    AbortReason, Admission, ChunkStep, DeviceId, Direction, IncomingTransfer, Outcome,
    PeerMessage, ResultOutcome, TransferEvent, TransferId,
};
use rocket_shared::error::{DomainError, DomainResult};
use tokio::sync::mpsc;

use super::transfer::{outcome_name, Inner, TransferService};
use super::transfer_ports::{
    receiver_display_name, StagedFile, StagedZip, UNSAFE_ZIP_MESSAGE,
};

/// The text of a pending offer whose sender went away before the user answered.
const WENT_OFFLINE: &str = "The other node went offline.";

/// The fields of a `CollectionOffer` frame.
pub(super) struct OfferFrame {
    pub transfer_id: String,
    pub name: String,
    pub size_bytes: u64,
    pub chunk_count: u32,
    pub sha256: String,
}

/// One incoming transfer, from the offer to the end of the import.
pub(super) struct In {
    pub device: DeviceId,
    pub from_name: String,
    pub machine: IncomingTransfer,
    /// Exists from Accept until the last chunk or the end of the transfer.
    pub staged: Option<Box<dyn StagedZip>>,
    pub last_activity: Instant,
    /// True from the last chunk until the finalize task ends the transfer. Only that task
    /// removes the entry then; an abort or a gone peer just fails the machine.
    pub finalizing: bool,
    /// The ack pump (ruling 20), from Accept on. Dropping the entry ends the pump.
    pub acks: Option<mpsc::UnboundedSender<PeerMessage>>,
}

enum Admit {
    Ignore,
    Refuse(AbortReason),
    Queued,
}

enum Answer {
    /// Unknown, answered already or not ours to answer.
    Gone,
    Expired(Box<In>),
    Declined(Box<In>, PeerMessage),
    Accepted(DeviceId, PeerMessage),
    StageFailed(Box<In>, DomainError),
}

enum Step {
    Ignore,
    Continue,
    Failed(Box<In>),
    Finalize(Box<dyn StagedZip>),
}

/// What the finalize task needs to know about the entry, read under the lock.
struct FinalView {
    size_bytes: u64,
    sha256: String,
    name: String,
    from_name: String,
    /// Set when the machine was failed meanwhile (an abort or a gone peer).
    interrupted: Option<Outcome>,
}

fn expired_error() -> DomainError {
    DomainError::InvalidInput("The request expired.".to_string())
}

/// The frame that refuses an offer for which no machine exists.
fn refusal(id: &TransferId, reason: AbortReason) -> PeerMessage {
    PeerMessage::TransferAbort {
        transfer_id: id.as_str().to_string(),
        reason,
    }
}

/// The verdict the sender is told. Other outcomes (declined, expired, cancelled) never get one.
fn result_for(outcome: Outcome) -> Option<ResultOutcome> {
    match outcome {
        Outcome::Completed => Some(ResultOutcome::Completed),
        Outcome::Damaged => Some(ResultOutcome::Damaged),
        Outcome::Rejected => Some(ResultOutcome::Rejected),
        Outcome::Failed => Some(ResultOutcome::Failed),
        _ => None,
    }
}

impl Inner {
    /// Removes an incoming transfer and frees its slot in the limits.
    fn take_in(&mut self, id: &TransferId) -> Option<In> {
        self.limits.release(id);
        self.incoming.remove(id)
    }
}

impl TransferService {
    /// The user's answer to a pending offer. Accepting stages the file and tells the sender
    /// to stream. An expired or unknown offer is `InvalidInput("The request expired.")`.
    pub async fn respond(&self, transfer_id: &str, accept: bool) -> DomainResult<()> {
        self.ensure_worker();
        let Ok(id) = TransferId::parse(transfer_id) else {
            return Err(expired_error());
        };
        match self.answer_offer(&id, accept) {
            Answer::Gone => Err(expired_error()),
            Answer::Expired(entry) => {
                let device = entry.device;
                let frame = entry.machine.abort_message(AbortReason::Expired);
                self.drop_in(&id, *entry);
                self.tell(&device, frame).await;
                Err(expired_error())
            }
            Answer::Declined(entry, frame) => {
                let device = entry.device;
                self.drop_in(&id, *entry);
                self.tell(&device, frame).await;
                Ok(())
            }
            Answer::StageFailed(entry, error) => {
                let device = entry.device;
                let frame = entry.machine.abort_message(AbortReason::Failed);
                self.drop_in(&id, *entry);
                self.tell(&device, frame).await;
                Err(error)
            }
            Answer::Accepted(device, frame) => {
                if let Err(e) = self.deps.network.send_to(&device, frame).await {
                    let entry = self.lock().take_in(&id);
                    if let Some(mut entry) = entry {
                        entry.machine.fail_with(Outcome::Failed);
                        self.drop_in(&id, entry);
                    }
                    return Err(e);
                }
                self.spawn_idle_watch(id);
                Ok(())
            }
        }
    }

    /// Asks the machine and acts on its answer under the lock. The staged file is created and
    /// stored here, before the `OfferAnswer` frame is sent, so no chunk finds the entry without it.
    fn answer_offer(&self, id: &TransferId, accept: bool) -> Answer {
        let now = Self::now();
        let mut guard = self.lock();
        let inner = &mut *guard;
        let Some(entry) = inner.incoming.get_mut(id) else {
            return Answer::Gone;
        };
        let Some(frame) = entry.machine.respond(accept, now) else {
            // The machine refused the answer: expired, or answered before. Only an expired
            // offer leaves the map; an accepted entry must stay.
            if entry.machine.outcome() == Some(Outcome::Expired) {
                return match inner.take_in(id) {
                    Some(entry) => Answer::Expired(Box::new(entry)),
                    None => Answer::Gone,
                };
            }
            return Answer::Gone;
        };
        if !accept {
            return match inner.take_in(id) {
                Some(entry) => Answer::Declined(Box::new(entry), frame),
                None => Answer::Gone,
            };
        }
        match self.deps.staging.create(id.as_str()) {
            Ok(staged) => {
                entry.staged = Some(staged);
                entry.last_activity = now;
                let device = entry.device;
                entry.acks = self.spawn_ack_pump(device);
                inner.limits.mark_active(id);
                Answer::Accepted(device, frame)
            }
            Err(error) => {
                entry.machine.fail_with(Outcome::Failed);
                match inner.take_in(id) {
                    Some(entry) => Answer::StageFailed(Box::new(entry), error),
                    None => Answer::Gone,
                }
            }
        }
    }

    // ----- frames from the peer -----

    /// A `CollectionOffer` frame.
    pub(super) fn on_offer(&self, from: DeviceId, frame: OfferFrame) {
        let Some((from_name, short_id)) = self.deps.network.peer_label(&from) else {
            return;
        };
        let Ok(id) = TransferId::parse(&frame.transfer_id) else {
            return;
        };
        match self.admit(&from, &from_name, &id, &frame) {
            Admit::Ignore => {}
            Admit::Refuse(reason) => {
                self.tell_later(from, refusal(&id, reason));
            }
            Admit::Queued => {
                self.emit(TransferEvent::OfferIncoming {
                    transfer_id: frame.transfer_id,
                    device_id: from.to_base32(),
                    short_id,
                    display_name: from_name,
                    collection_name: frame.name,
                    size_bytes: frame.size_bytes,
                });
                self.spawn_pending_expiry(id);
            }
        }
    }

    /// The limits decide first, then the machine checks the offer. Both answers are the
    /// machine's own; the service only turns them into a queue entry or a refusal.
    fn admit(&self, from: &DeviceId, from_name: &str, id: &TransferId, frame: &OfferFrame) -> Admit {
        let now = Self::now();
        let ttl = self.timing.offer_ttl;
        let mut guard = self.lock();
        let inner = &mut *guard;
        match inner.limits.admit_offer(from, id, now) {
            Admission::Duplicate => return Admit::Ignore,
            Admission::Refuse(reason) => return Admit::Refuse(reason),
            Admission::Admit => {}
        }
        let machine = IncomingTransfer::from_offer(
            &frame.transfer_id,
            &frame.name,
            frame.size_bytes,
            frame.chunk_count,
            &frame.sha256,
            now,
        )
        .map(|machine| machine.with_offer_ttl(ttl));
        match machine {
            Ok(machine) => {
                inner.incoming.insert(
                    id.clone(),
                    In {
                        device: *from,
                        from_name: from_name.to_string(),
                        machine,
                        staged: None,
                        last_activity: now,
                        finalizing: false,
                        acks: None,
                    },
                );
                Admit::Queued
            }
            Err(error) => {
                inner.limits.release(id);
                Admit::Refuse(error.abort_reason())
            }
        }
    }

    /// A `Chunk` frame. The last chunk starts the finalize task and returns at once.
    pub(super) fn on_chunk(&self, from: DeviceId, id: &TransferId, seq: u32, data: &str) {
        match self.take_chunk_step(&from, id, seq, data) {
            Step::Ignore | Step::Continue => {}
            Step::Failed(entry) => {
                let device = entry.device;
                let frame = entry.machine.abort_message(AbortReason::Failed);
                self.drop_in(id, *entry);
                self.tell_later(device, frame);
            }
            Step::Finalize(staged) => self.spawn_finalize(id.clone(), staged),
        }
    }

    /// Feeds a chunk to the machine under the lock and acts on its answer. The append is a
    /// short sync write. `Progress` is published here so it stays ahead of a later `Finished`.
    fn take_chunk_step(&self, from: &DeviceId, id: &TransferId, seq: u32, data: &str) -> Step {
        let now = Self::now();
        let mut guard = self.lock();
        let inner = &mut *guard;
        let Some(entry) = inner.incoming.get_mut(id) else {
            return Step::Ignore;
        };
        if entry.device != *from {
            return Step::Ignore;
        }
        let last = match entry.machine.on_chunk(seq, data) {
            ChunkStep::Ignored => return Step::Ignore,
            ChunkStep::Failed => None,
            ChunkStep::Data {
                bytes,
                done_bytes,
                last,
                ack,
            } => match entry.staged.as_mut().map(|staged| staged.append(&bytes)) {
                Some(Ok(())) => {
                    entry.last_activity = now;
                    self.emit_receive_progress(id, done_bytes, entry.machine.total_bytes());
                    // Queued under the lock, so the pump sends the acks in chunk order.
                    if let (Some(ack), Some(pump)) = (ack, entry.acks.as_ref()) {
                        let _ = pump.send(ack);
                    }
                    Some(last)
                }
                // A write error or a missing file ends the transfer.
                _ => {
                    entry.machine.fail_with(Outcome::Failed);
                    None
                }
            },
        };
        match last {
            // The entry stays while chunks are coming.
            Some(false) => Step::Continue,
            // The entry stays, marked, while the finalize task works (ruling 16).
            Some(true) => {
                entry.finalizing = true;
                match entry.staged.take() {
                    Some(staged) => Step::Finalize(staged),
                    None => {
                        entry.machine.fail_with(Outcome::Failed);
                        match inner.take_in(id) {
                            Some(entry) => Step::Failed(Box::new(entry)),
                            None => Step::Ignore,
                        }
                    }
                }
            }
            None => match inner.take_in(id) {
                Some(entry) => Step::Failed(Box::new(entry)),
                None => Step::Ignore,
            },
        }
    }

    /// The peer aborted a transfer it offered or is streaming.
    pub(super) fn abort_incoming(&self, from: &DeviceId, id: &TransferId, reason: AbortReason) {
        let entry = {
            let mut guard = self.lock();
            let inner = &mut *guard;
            let Some(e) = inner.incoming.get_mut(id).filter(|e| e.device == *from) else {
                return;
            };
            if e.finalizing {
                // The finalize task owns the entry. It sees the failed machine before it imports.
                e.machine.fail_with(Outcome::from(reason));
                return;
            }
            if e.machine.on_abort(reason).is_some() {
                inner.take_in(id)
            } else {
                None
            }
        };
        if let Some(entry) = entry {
            self.drop_in(id, entry);
        }
    }

    /// The device's connection ended or its pairing was removed. Every transfer from it that
    /// is not importing yet is dropped now; one that is importing is failed and ends itself.
    /// A pending offer says the sender went offline (ruling 6).
    pub(super) fn incoming_gone(&self, device: &DeviceId) {
        let dropped: Vec<(TransferId, In, Option<String>)> = {
            let mut guard = self.lock();
            let inner = &mut *guard;
            let ids: Vec<TransferId> = inner
                .incoming
                .iter()
                .filter(|(_, e)| e.device == *device)
                .map(|(id, _)| id.clone())
                .collect();
            let mut dropped = Vec::new();
            for id in ids {
                let Some(e) = inner.incoming.get_mut(&id) else {
                    continue;
                };
                if e.finalizing {
                    e.machine.fail_with(Outcome::Failed);
                    continue;
                }
                let message = e.machine.is_pending().then(|| WENT_OFFLINE.to_string());
                if e.machine.connection_lost().is_some() {
                    if let Some(entry) = inner.take_in(&id) {
                        dropped.push((id, entry, message));
                    }
                }
            }
            dropped
        };
        for (id, entry, message) in dropped {
            self.drop_in_with(&id, entry, message);
        }
    }

    /// The user cancels a transfer that is not ours to send. A pending offer is declined. A
    /// transfer that is already importing is left alone.
    pub(super) async fn cancel_incoming(&self, id: &TransferId) -> bool {
        let (pending, finalizing) = self
            .lock()
            .incoming
            .get(id)
            .map(|e| (e.machine.is_pending(), e.finalizing))
            .unwrap_or((false, false));
        if finalizing {
            return false;
        }
        if pending {
            return self.respond(id.as_str(), false).await.is_ok();
        }
        let entry = {
            let mut guard = self.lock();
            let inner = &mut *guard;
            let ended = inner
                .incoming
                .get_mut(id)
                .is_some_and(|e| e.machine.fail_with(Outcome::Cancelled).is_some());
            if ended {
                inner.take_in(id)
            } else {
                None
            }
        };
        let Some(entry) = entry else {
            return false;
        };
        let device = entry.device;
        let frame = entry.machine.abort_message(AbortReason::Cancelled);
        self.drop_in(id, entry);
        self.tell(&device, frame).await;
        true
    }

    // ----- finishing -----

    fn spawn_finalize(&self, id: TransferId, staged: Box<dyn StagedZip>) {
        let Some(me) = self.arc() else {
            staged.discard();
            return;
        };
        drop(tokio::spawn(async move { me.finalize(&id, staged).await }));
    }

    /// Checks the staged file and imports it. Every exit after `finish` releases the file.
    async fn finalize(&self, id: &TransferId, staged: Box<dyn StagedZip>) {
        let finished = tokio::task::spawn_blocking(move || staged.finish()).await;
        let Ok(Ok(file)) = finished else {
            self.conclude(id, Outcome::Failed, None, None, true);
            return;
        };
        let Some(view) = self.final_view(id) else {
            self.release_staged(&file);
            return;
        };
        // An abort or a gone peer during the finish ends the transfer without an import.
        if let Some(outcome) = view.interrupted {
            self.release_staged(&file);
            self.conclude(id, outcome, None, None, false);
            return;
        }
        // Both the size and the hash must fit (ruling 18).
        let intact = file.len == view.size_bytes && file.sha256.eq_ignore_ascii_case(&view.sha256);
        let result = if intact {
            self.import_staged(&file, &view.name, &view.from_name).await
        } else {
            Err((Outcome::Damaged, None))
        };
        self.release_staged(&file);
        match result {
            Ok(imported) => self.conclude(id, Outcome::Completed, None, Some(imported), true),
            Err((outcome, message)) => self.conclude(id, outcome, message, None, true),
        }
    }

    fn final_view(&self, id: &TransferId) -> Option<FinalView> {
        let inner = self.lock();
        let entry = inner.incoming.get(id)?;
        Some(FinalView {
            size_bytes: entry.machine.total_bytes(),
            sha256: entry.machine.sha256().to_string(),
            name: entry.machine.name().to_string(),
            from_name: entry.from_name.clone(),
            interrupted: entry
                .machine
                .outcome()
                .filter(|outcome| *outcome != Outcome::Completed),
        })
    }

    /// Imports the staged ZIP. The error is the outcome to report and the text for the event.
    async fn import_staged(
        &self,
        file: &StagedFile,
        collection: &str,
        from_name: &str,
    ) -> Result<String, (Outcome, Option<String>)> {
        let importer = Arc::clone(&self.deps.importer);
        let path = file.path.clone();
        let collection = collection.to_string();
        let from_name = from_name.to_string();
        let imported = tokio::task::spawn_blocking(move || {
            let existing = importer.existing_names()?;
            let display = receiver_display_name(&collection, &from_name, &existing);
            importer.import_zip(&path, &display)
        })
        .await;
        match imported {
            Ok(Ok(done)) => Ok(done.name),
            // Only the strict validator's exact text is a rejection.
            Ok(Err(DomainError::InvalidInput(text))) if text == UNSAFE_ZIP_MESSAGE => {
                Err((Outcome::Rejected, Some(text)))
            }
            Ok(Err(error)) => Err((Outcome::Failed, Some(error.to_string()))),
            Err(_) => Err((Outcome::Failed, None)),
        }
    }

    /// Ends a transfer that reached the finalize step: removes the entry, emits the single
    /// `Finished`, audits `Completed`, `Damaged` and `Rejected` (ruling 9) and tells the sender
    /// the verdict from its own task. `imported` is the name the collection got.
    fn conclude(
        &self,
        id: &TransferId,
        outcome: Outcome,
        message: Option<String>,
        imported: Option<String>,
        tell_sender: bool,
    ) {
        let Some(entry) = self.lock().take_in(id) else {
            return;
        };
        let In {
            device,
            from_name,
            machine,
            ..
        } = entry;
        let collection = imported.unwrap_or_else(|| machine.name().to_string());
        self.emit_finished(id, &from_name, &collection, outcome, message);
        if matches!(
            outcome,
            Outcome::Completed | Outcome::Damaged | Outcome::Rejected
        ) {
            self.audit(AuditEventKind::NetworkCollectionReceived {
                device_id: device.short(),
                collection,
                outcome: outcome_name(outcome).to_string(),
            });
        }
        if tell_sender {
            if let Some(verdict) = result_for(outcome) {
                self.tell_later(
                    device,
                    PeerMessage::TransferResult {
                        transfer_id: id.as_str().to_string(),
                        outcome: verdict,
                    },
                );
            }
        }
    }

    /// Deletes a finished staged file. A failure is logged; the start-up purge is the net.
    fn release_staged(&self, file: &StagedFile) {
        if let Err(e) = self.deps.staging.release(file) {
            tracing::warn!(code = "network_transfer_release_failed", error = %e, "staged file stays");
        }
    }

    /// Throws away an unfinished receive and reports the machine's outcome. The entry is
    /// already removed, so this is the one place its `Finished` is emitted.
    fn drop_in(&self, id: &TransferId, entry: In) {
        self.drop_in_with(id, entry, None);
    }

    /// `drop_in` with a `Finished.message`.
    fn drop_in_with(&self, id: &TransferId, entry: In, message: Option<String>) {
        let In {
            from_name,
            machine,
            staged,
            ..
        } = entry;
        if let Some(staged) = staged {
            staged.discard();
        }
        let outcome = machine.outcome().unwrap_or(Outcome::Failed);
        self.emit_finished(id, &from_name, machine.name(), outcome, message);
    }

    /// One task per accepted receive that sends its acks one after the other, so they never
    /// overtake each other (ruling 20). It ends when the entry, and so the sender half, goes.
    fn spawn_ack_pump(&self, device: DeviceId) -> Option<mpsc::UnboundedSender<PeerMessage>> {
        let me = self.arc()?;
        let (pump, mut queue) = mpsc::unbounded_channel::<PeerMessage>();
        drop(tokio::spawn(async move {
            while let Some(frame) = queue.recv().await {
                me.tell(&device, frame).await;
            }
        }));
        Some(pump)
    }

    fn emit_finished(
        &self,
        id: &TransferId,
        from_name: &str,
        collection: &str,
        outcome: Outcome,
        message: Option<String>,
    ) {
        self.emit(TransferEvent::Finished {
            transfer_id: id.as_str().to_string(),
            direction: Direction::Receive,
            outcome,
            display_name: from_name.to_string(),
            collection_name: collection.to_string(),
            message,
        });
    }

    fn emit_receive_progress(&self, id: &TransferId, done: u64, total: u64) {
        self.emit(TransferEvent::Progress {
            transfer_id: id.as_str().to_string(),
            direction: Direction::Receive,
            done_bytes: done,
            total_bytes: total,
        });
    }

    // ----- timers -----

    fn spawn_pending_expiry(&self, id: TransferId) {
        let Some(me) = self.arc() else {
            return;
        };
        let ttl = self.timing.offer_ttl;
        drop(tokio::spawn(async move {
            tokio::time::sleep(ttl).await;
            me.expire_in(&id).await;
        }));
    }

    /// Ends an unanswered offer when the machine says it is due.
    async fn expire_in(&self, id: &TransferId) {
        let now = Self::now();
        let entry = {
            let mut guard = self.lock();
            let inner = &mut *guard;
            let due = inner
                .incoming
                .get_mut(id)
                .is_some_and(|e| e.machine.expire_if_due(now));
            if due {
                inner.take_in(id)
            } else {
                None
            }
        };
        if let Some(entry) = entry {
            let device = entry.device;
            let frame = entry.machine.abort_message(AbortReason::Expired);
            self.drop_in(id, entry);
            self.tell(&device, frame).await;
        }
    }

    /// Drops a receive that gets no frame for `Timing::idle_limit`. This is the fallback for a
    /// half-open connection; the peer-gone hook ends a dead one at once. A transfer that is
    /// importing is not watched.
    fn spawn_idle_watch(&self, id: TransferId) {
        enum Idle {
            Gone,
            Wait(Duration),
            Dropped(Box<In>),
        }
        let Some(me) = self.arc() else {
            return;
        };
        let limit = self.timing.idle_limit;
        drop(tokio::spawn(async move {
            loop {
                let step = {
                    let now = TransferService::now();
                    let mut guard = me.lock();
                    let inner = &mut *guard;
                    let due = inner
                        .incoming
                        .get(&id)
                        .map(|e| (e.finalizing, e.last_activity + limit));
                    match due {
                        None | Some((true, _)) => Idle::Gone,
                        Some((false, due)) if due > now => {
                            Idle::Wait(due.saturating_duration_since(now))
                        }
                        Some(_) => {
                            let ended = inner
                                .incoming
                                .get_mut(&id)
                                .is_some_and(|e| e.machine.connection_lost().is_some());
                            match ended.then(|| inner.take_in(&id)).flatten() {
                                Some(entry) => Idle::Dropped(Box::new(entry)),
                                None => Idle::Gone,
                            }
                        }
                    }
                };
                match step {
                    Idle::Gone => return,
                    Idle::Wait(wait) => tokio::time::sleep(wait).await,
                    Idle::Dropped(entry) => {
                        let device = entry.device;
                        let frame = entry.machine.abort_message(AbortReason::Failed);
                        me.drop_in(&id, *entry);
                        me.tell(&device, frame).await;
                        return;
                    }
                }
            }
        }));
    }

    /// `(pending, accepted)` incoming transfers, for tests.
    #[cfg(test)]
    pub(super) fn counts(&self) -> (usize, usize) {
        let inner = self.lock();
        let pending = inner
            .incoming
            .values()
            .filter(|e| e.machine.is_pending())
            .count();
        (pending, inner.incoming.len() - pending)
    }
}
```

- [ ] **Step 5: Wire the receiver into `transfer.rs`, `transfer_send.rs` and `mod.rs`**

In `crates/rocket-app/src/network/transfer.rs`:

1. Add the import (next to `use super::transfer_send::Out;`):

```rust
use super::transfer_receive::{In, OfferFrame};
```

2. Replace the `Inner` struct with:

```rust
/// The open transfers and the offer limits. `limits` knows every transfer in both maps.
#[derive(Default)]
pub(super) struct Inner {
    pub outgoing: HashMap<TransferId, Out>,
    pub incoming: HashMap<TransferId, In>,
    pub limits: TransferLimits,
}
```
(keep the `impl Inner { take_out }` block as it is; `take_in` lives in `transfer_receive.rs`).

3. Replace the body of `handle_frame` with:

```rust
    fn handle_frame(&self, from: DeviceId, message: PeerMessage) {
        match message {
            PeerMessage::CollectionOffer {
                transfer_id,
                name,
                size_bytes,
                chunk_count,
                sha256,
            } => {
                let frame = OfferFrame {
                    transfer_id,
                    name,
                    size_bytes,
                    chunk_count,
                    sha256,
                };
                self.on_offer(from, frame);
            }
            PeerMessage::OfferAnswer {
                transfer_id,
                accept,
            } => {
                if let Ok(id) = TransferId::parse(&transfer_id) {
                    self.on_answer(&from, &id, accept);
                }
            }
            PeerMessage::Chunk {
                transfer_id,
                seq,
                data,
            } => {
                if let Ok(id) = TransferId::parse(&transfer_id) {
                    self.on_chunk(from, &id, seq, &data);
                }
            }
            PeerMessage::TransferAbort {
                transfer_id,
                reason,
            } => {
                if let Ok(id) = TransferId::parse(&transfer_id) {
                    if !self.abort_outgoing(&from, &id, reason) {
                        self.abort_incoming(&from, &id, reason);
                    }
                }
            }
            PeerMessage::TransferResult {
                transfer_id,
                outcome,
            } => {
                if let Ok(id) = TransferId::parse(&transfer_id) {
                    self.on_result(&from, &id, outcome);
                }
            }
            _ => {}
        }
    }
```

4. Replace the body of `handle_gone` with:

```rust
    pub(super) fn handle_gone(&self, device: &DeviceId) {
        self.outgoing_gone(device);
        self.incoming_gone(device);
    }
```

In `crates/rocket-app/src/network/transfer_send.rs`, replace `cancel` with:

```rust
    /// Cancels a transfer, sent or received. An unknown or finished id is not an error. After
    /// the last chunk a send can no longer be cancelled. A receive that is importing is left alone.
    pub async fn cancel(&self, transfer_id: &str) -> DomainResult<()> {
        let Ok(id) = TransferId::parse(transfer_id) else {
            return Ok(());
        };
        if self.cancel_outgoing(&id).await? {
            return Ok(());
        }
        self.cancel_incoming(&id).await;
        Ok(())
    }
```

In `crates/rocket-app/src/network/mod.rs`, add `mod transfer_receive;` to the module list.
- [ ] **Step 6: Run the receiver tests**

Run: `cargo test -j4 -p rocket-app transfer_receive_tests`
Expected: PASS, the 37 tests in `network::transfer_receive_tests` pass, 0 failed. If `the_receiver_acks_every_16_chunks_and_the_last` sees no ack, `take_chunk_step` drops `ChunkStep::Data.ack` or the accept did not start the pump. If `a_wrong_staged_length_is_damaged` imports, `finalize` compares the hash only. If `a_stalled_stream_fails_both_sides_within_the_idle_limit` times out, the idle watch is not running for the accepted entry. If `a_second_offer_from_the_same_peer_is_busy` shows three incoming rows, `mark_active` was not called on accept (the accepted entry must still hold the peer's slot). If a finalize test times out in `wait_for`, a `spawn_blocking` future was awaited under paused time: the test must not use `start_paused`. If `accepting_after_expiry_sends_nothing_and_reports_expired` shows two `Finished` events, `respond` and the expiry task both removed the entry: `take_in` must be the only gate. If `assert_clean` reports `released` below `finished`, one exit after `finish` skipped `release_staged`. If `an_abort_during_the_finish_stops_the_import` shows an import call, `finalize` did not read `FinalView.interrupted` before the import, or `abort_incoming` removed the finalizing entry instead of failing its machine. If `a_slow_import_does_not_block_other_peers` hangs, the import ran on the frame worker instead of the spawned finalize task. If a test sees `reasons` where it expects `results`, `conclude` sent an abort instead of the `TransferResult`.

- [ ] **Step 7: Run all transfer and network tests**

Run: `cargo test -j4 -p rocket-app -- transfer_link_tests transfer_send_tests transfer_receive_tests` then `cargo test -j4 -p rocket-app network`
Expected: PASS both. The first runs 16 + 46 + 37 = 99 tests (the filters after `--` match the three test modules only, not the `transfer_ports` tests), 0 failed. The second shows no failure in the earlier pairing, presence and end-to-end tests, and the `transfer_ports` tests.

- [ ] **Step 8: Update the crate guide**

In `crates/rocket-app/CLAUDE.md`, in the Public Types table, after the `NetworkHandle` row, add:

```markdown
| `TransferService` | Send a collection to a paired node and receive one (`src/network/transfer*.rs`). It holds the `rocket-network` `OutgoingTransfer` and `IncomingTransfer` machines and one `TransferLimits`, and delegates every rule to them (offer shape, expiry, chunk order, caps). The service does the I/O: `send` (returns `SendStarted { transfer_id, notes }`) checks the node is paired and online, exports with `CollectionExportService::export_bruno` in `spawn_blocking`, hashes the ZIP and offers it; after Accept it streams 32 KiB base64 chunks, at most 32 ahead of the receiver's `ChunkAck` (no ack for the idle limit fails the send), and then waits for the receiver's `TransferResult` (up to `SENDER_RESULT_TIMEOUT`, 60 s) before it reports `Completed`; a gone peer or a missing result after the receiver's final `ChunkAck` is `Unconfirmed`; a gone peer or a stall before that ack is `Failed`. Both are audited with their outcome. The receiver stages chunks through `TransferStaging`, acks every 16 chunks and the last through one ordered pump per transfer, checks the size and the SHA-256, imports through `CollectionImportPort` (the exact `UNSAFE_ZIP_MESSAGE` is `Rejected`, any other error `Failed` with its text in `Finished.message`), frees the file with `TransferStaging::release` (`discard` for an unfinished one; the service never purges), audits `completed`, `damaged` and `rejected`, and sends `TransferResult`. Frames and `on_peer_gone` notices arrive through `TransferSink` into one queue and one worker; the finalize step and outbound frames run as their own tasks. `NetworkService` counts every transfer frame as a pong, skips the block-list read for `Chunk` and `ChunkAck`, and reports a gone peer (socket ended, pairing removed); both roles end that peer's transfers at once; a 120 s idle limit (a field of `Timing`) is the fallback. Whoever removes a transfer emits its single `Finished`. `new_with_timing` is a `#[doc(hidden)] pub` seam; `offer_raw`, `set_chunk_delay` and `set_tamper` are `#[cfg(test)] pub(crate)` hooks for the end-to-end tests in this crate, absent from a release build. |
| `PeerSender` | The four calls `TransferService` needs from the node (`send_to`, `peer_label`, `is_online`, `set_transfer_sink`). `NetworkService` implements it. |
```

- [ ] **Step 9: Verify the whole plan**

Run, one after the other:
```bash
cargo fmt -p rocket-app --check
cargo check -j4 -p rocket-app
cargo clippy -j4 -p rocket-app --all-targets
cargo test -j4 -p rocket-app -- transfer_link_tests transfer_send_tests transfer_receive_tests
cargo test -j4 -p rocket-app network
grep -n -B1 "fn offer_raw\|fn set_chunk_delay\|fn set_tamper" crates/rocket-app/src/network/transfer.rs crates/rocket-app/src/network/transfer_send.rs
```
Expected: `fmt --check` prints no diff (if it does, run `cargo fmt -p rocket-app` and re-run); `check` finishes with no error; `clippy` shows no warning in `network/transfer*.rs` (fix any: a long function, a needless borrow); the first test run passes 99 tests (16 + 46 + 37) with 0 failed; the `network` run passes with 0 failed; the grep shows `#[cfg(test)]` on the line above each of the three hooks and no `pub fn` or `#[doc(hidden)]` there. The plain `cargo check` above builds without `cfg(test)`, so it already proves no release code reaches a hook or a hook field.

No commit; reviewed at the plan end.

---

## Self-review

Spec coverage:

- Section 2 decisions 3 (never overwrite: `respond` is the only way to a write, import goes through `CollectionImportPort`), 4 (asked every time, 2 minute expiry from the machine, no auto-accept) and 6 (the import is the Plan 02 path with no trust record, so no Developer mode, host env or vault; the scripts still run in the Safe sandbox, and this plan adds no trust record and no script gate).
- Section 3 success criteria for decline, 2 minute expiry, dropped connection (peer-gone hook, idle fallback, failed `send_to`), damaged payload, unsafe ZIP (tests in Tasks 2 and 3) and the size limit (`a_collection_over_the_limit_is_refused_before_any_offer`; the exact boundary is Plan 1's `the_size_limit_is_exact`).
- Section 5 flow steps 2 to 6 (export and hash, offer, accept, chunks in the ack window, size and hash check and import, `TransferResult`, finished events on both sides; the sender shows Sent only after the result and `Unconfirmed` when it never comes).
- Section 6 protocol rules: unknown or finished id ignored, `Chunk` before accept aborts with `failed`, out-of-order, past-count and surplus data abort, the ack window and the bad-ack rule, transfer frames as liveness, the block-check skip for `Chunk` and `ChunkAck`, lost frames (dropped chunk, stalled stream), the offer name rule on the send side. The rules sit in the Plan 1 machines; the service tests prove the frames, the cleanup and the single `Finished`.
- Section 7 limits: all of them are `TransferLimits` and machine checks from Plan 1 (`MAX_TRANSFER_BYTES`, `CHUNK_BYTES`, `TRANSFER_OFFER_TTL`, `MAX_PENDING_INCOMING`, `MAX_ACTIVE_PER_PEER` in both directions, `OFFERS_PER_PEER_PER_MINUTE`); this plan proves the `busy` answer. `MAX_UNZIPPED_BYTES` and `MAX_ZIP_ENTRIES` belong to Plan 02.
- Section 8: the sink never touches collections, nothing written before Accept, staged files freed (`discard` or `release`) on every exit path, the audit kinds with the short id and the outcome (sender `completed`, `unconfirmed` and `failed`, receiver `completed`, `damaged` and `rejected`), nothing logged about names or payloads. The service never deletes a collection; failed-import cleanup is the adapter's `created_roots` rule (Plan 2).
- Section 9 `rocket-app` list: `send_to`, `set_transfer_sink`, the `run_socket` hand-off for six frames, `on_peer_gone` from `run_socket` end and `remove_local`, sender and receiver duties, `TransferEvent` publishing.
- Section 10 error table, rows offline, decline, receiver cancel, sender offline while pending ("The other node went offline." in `Finished.message`), expiry, over 10 MiB, too many offers, connection drop or idle, drop after the final ack (`Unconfirmed`), drop after the last chunk but before the final ack (`Failed`), bad name, hash or size mismatch, unsafe ZIP, import fails (the sender sees the last four through `TransferResult`; the receiver's `Finished.message` carries the importer text).
- Section 11 service tests: accept, decline, expiry, hash and size mismatch, cancel by either side including a pending offer, a second offer from the same peer, three pending and a fourth, the ack window on a slow link, a dropped chunk, a stalled stream, peer gone after the last chunk, `include_environments = false` with environments in the source, the result frame, peer gone.

Review-fix coverage (cross-plan review, Plan 03 items): `SendStarted` with notes (ruling 15, `notes_list_skipped_and_blanked_items_only`); `TransferResult` and `SENDER_RESULT_TIMEOUT` and no result grace window (ruling 5); `on_peer_gone` hooked in `run_socket` (through `run_device`) and `remove_local` with a recording-sink test for each (Task 1); `new_with_timing` with `with_offer_ttl` on every machine, including the one built inside `spawn_blocking` (ruling 4); `is_online` precheck before the export (ruling 14); spawned finalize and spawned outbound frames with `on_frame` sync (ruling 2); the finalize re-check for an abort or a gone peer (ruling 16); cancel after the last chunk refused (ruling 5); `offer_raw`, chunk-delay and tamper hooks for Plan 4 (ruling 17); the exact `UNSAFE_ZIP_MESSAGE` comparison and the importer text in `Finished.message` (ruling 8).

Second-review coverage (index rulings R2, R4, R6, R7, R8, R9, R10, R13, R14, R15, R19): rulings 5, 6, 9, 13, 17 to 23 of this plan, with the tests in Review Focus items 6, 9, 10, 13, 15 to 22.

Test counts, counted from the code in this file: Task 1 has 16 tests (8 earlier plus 8: the six-frame sink order, peer gone (3), `is_online`, pongs, the block-check skip, a real block), Task 2 has 46 (37 plus the window, no-ack, bad-ack, bad-name, environments and outcome-name tests, and the three final-ack tests), Task 3 has 37 (31 plus the acks, wrong length, two pending-cancel and two linked lost-frame tests). The Expected lines use the module filters `transfer_link_tests`, `transfer_send_tests` and `transfer_receive_tests`, which match exactly these tests; the bare word `transfer` is never used because it also matches the `transfer_ports` tests.

Not covered here on purpose: Tauri commands, the `network-transfer` channel wiring and the end-to-end tests over the real transport (Plan 04); the ZIP validator and the staging implementation (Plan 02); the machine and limit rule tests (Plan 01); all frontend and docs (Plan 05).
