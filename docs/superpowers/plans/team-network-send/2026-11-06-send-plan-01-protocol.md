# Team network send: Plan 1, protocol

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 1 of 5.** Previous plan: the team network MVP plans (`docs/superpowers/plans/team-network/`, merged to `main`).
**Next plan to run: `docs/superpowers/plans/team-network-send/2026-11-06-send-plan-02-import-side.md`.** Do not start it without the user.
**Recommended model: Sonnet.**
**No commit during the plan.** The user reviews the whole plan, then commits through `dev-workflow-skills:1-git-commit` with explicit paths.

**Goal:** The peer protocol can carry a collection. `PROTOCOL_VERSION` is 2, `PeerMessage` has the six transfer frames (offer, answer, chunk, abort, chunk ack, result), and `rocket-network` holds the pure pieces every later plan builds on: the constants, the transfer id, the sender and receiver state machines, the offer limits, the `TransferSink` port and the `TransferEvent`. `rocket-audit` knows the two new event kinds. Nothing is sent yet.

**Architecture:** Everything new in `rocket-network` is pure: no clock reads (time is an `Instant` argument), no I/O, no logging. The state machines do not hold bytes or files. The receiver machine validates and decodes each chunk and hands the raw bytes back to the caller, which stages them (Plan 2 and 3). A failed check never panics and never returns an error to the socket loop: it moves the machine to a final state and returns a value the caller turns into a `TransferAbort`. The sender machine also accepts the receiver's `TransferResult` (`on_result`), so the sender only reports `Completed` after the receiver confirmed the import, and `Unconfirmed` only when the peer goes away or the result timeout passes after the receiver's final `ChunkAck` arrived (a peer lost before that ack is `Failed`). The sender machine keeps an ack window: at most `MAX_UNACKED_CHUNKS` (32) chunks sent and not acknowledged, reopened by the receiver's `ChunkAck` (sent every `ACK_EVERY_CHUNKS`, 16, chunks and after the last). The only edit outside `rocket-network` and `rocket-audit` is one match arm in `rocket-app` `connections.rs` (an or-pattern with all six variants), so the exhaustive `match` on `PeerMessage` still compiles. Plan 3 replaces that arm with the sink call.

**Tech Stack:** Rust (`rocket-network`, `rocket-audit`, one arm in `rocket-app`), serde, base64, rand.

## Spec

`docs/superpowers/specs/2026-11-06-team-network-send-collection-design.md`, sections 6 (protocol), 7 (limits), 9 (components, `rocket-network` and events), 11 (domain tests). Binding names: `docs/superpowers/plans/team-network-send/00-plan-index.md` ("Plan 1 produces").

## Global Constraints

- Pure crate rules from `crates/rocket-network/CLAUDE.md`: no I/O, no runtime, no new dependencies. `base64` and `rand` are already in `Cargo.toml`.
- Rust: no `unwrap()` in production code. `expect("...")` is fine in tests only.
- Wire types: snake_case JSON, fields stay snake_case. `PeerMessage` keeps `deny_unknown_fields`.
- `TransferEvent`: camelCase tag, snake_case variant fields, on purpose, like `NetworkEvent`. A `wire_contract_*` test pins the keys. `seq` and `tsMs` are added by `TauriEventBus`, never by the event.
- Persistence structs and wire types never get `rename_all = "camelCase"` except the event tag.
- `rocket-audit`: field names stay snake_case (`device_id`, `collection`, `outcome`).
- The variant name `send_collection` must stay unknown: an existing test uses it as the example of an unknown type. The new tags are `collection_offer`, `offer_answer`, `chunk`, `transfer_abort`, `chunk_ack`, `transfer_result`.
- Cargo commands always pass `-j4`, one crate at a time. Never `--workspace`. Never `cargo test` with `--all`.
- Comments are short full sentences that end with a period. No emojis.
- No TypeScript in this plan. Plan 5 adds the audit row and the event types.
- No commit during the plan.

## Review Focus

The ten most likely failure inputs, each with a test in the owning task:

1. **A chunk frame over 64 KiB after base64.** `CHUNK_BYTES` raw is 43 692 base64 characters, so a full chunk frame fits. A frame carrying an oversize `data` field is refused at `decode`, and the limit is exact. Tests: Task 1 `a_full_chunk_frame_fits_and_decodes_at_exactly_the_frame_limit`, `a_chunk_with_an_oversize_data_field_is_rejected`, `chunk_frame_boundary_is_exact`; Task 2 `an_oversize_data_string_aborts_without_decoding`.
2. **An offer with size 0, or one over the cap by one byte.** Size 0 is refused (`Empty`). 10 MiB passes and 10 MiB + 1 is `TooBig` (mapped to the `too_big` abort). Tests: Task 2 `an_offer_of_size_zero_is_refused`, `the_size_limit_is_exact`, `the_sender_refuses_the_same_offers`.
3. **`chunk_count` that lies about `size_bytes`.** Too high, too low and zero for a non-zero size are all refused. Tests: Task 2 `a_chunk_count_that_does_not_match_the_size_is_refused`.
4. **Duplicate transfer ids.** A second offer with a known id is ignored and must not consume a rate hit or disturb the first transfer. Tests: Task 2 `a_duplicate_offer_id_is_ignored_and_costs_no_rate_hit`. A frame for an unknown or finished id is ignored: `frames_after_a_final_state_are_ignored`, `unknown_ids_are_not_found_in_the_limits`.
5. **A transfer id that is not 32 lowercase hex characters.** Also covers upper case, multi-byte text of 32 bytes and an empty string. Tests: Task 2 `parse_rejects_anything_but_32_lowercase_hex`, `an_offer_with_a_bad_id_is_refused`.

6. **The sender machine is `Done` before the last frame is on the wire.** A cancel, a failed send or a peer failure report after that must still end the transfer, and the offer ttl must be shortenable for Plan 4 without touching the default. Tests: Task 2 `the_service_can_fail_a_sender_transfer_after_the_last_chunk`, `fail_with_ends_an_offered_or_sending_transfer`, `fail_with_completed_is_refused_and_changes_nothing`, `fail_with_after_expiry_returns_none`, `a_short_sender_offer_ttl_expires_the_offer_at_exactly_that_ttl`, `a_short_receiver_offer_ttl_expires_the_offer_at_exactly_that_ttl`, `the_default_sender_offer_ttl_is_the_constant`, `the_default_receiver_offer_ttl_is_the_constant`.

7. **The sender reports `Completed` before the receiver confirmed the import.** `Done` only means the last chunk frame was built. A `TransferResult` is accepted once, and only while the sender machine waits for it (`Done`, no result yet): a second result, one before the last chunk and one after an abort or expiry are ignored. `damaged`, `rejected` and `failed` become the real outcome, and a confirmed send can no longer be failed. The 60 s wait is a pinned constant. Tests: Task 1 `transfer_frames_have_the_expected_shape`, `result_outcomes_use_snake_case_names`, `spec_values_are_pinned`; Task 2 `a_result_while_done_ends_the_sender_transfer_with_the_real_outcome`, `a_second_result_is_ignored`, `a_result_before_the_last_chunk_is_ignored`, `a_result_after_abort_or_expiry_is_ignored`, `a_confirmed_send_cannot_be_failed_later`, `result_outcomes_map_to_outcomes`; Task 3 `finished_carries_a_null_or_a_string_message`.

8. **The sender outruns a slow receiver (index R2).** The sender machine refuses a chunk once 32 are unacked, an ack reopens the window, and an ack that goes backwards, past the chunks sent, or arrives before the accept fails the transfer. The receiver asks for an ack exactly every 16th chunk and on the last. Tests: Task 1 `transfer_frames_have_the_expected_shape` (the `chunk_ack` frame), `spec_values_are_pinned`, `transfer_limits_are_consistent`; Task 2 `the_sender_window_closes_after_32_unacked_chunks_and_an_ack_reopens_it`, `a_bad_ack_fails_the_sender_and_a_late_one_is_ignored`, `the_receiver_acks_every_16th_chunk_and_the_last`, `the_last_chunk_completes_the_receive`.
9. **A false failure after all bytes left (index R4).** A lost peer or the result timeout gives `Unconfirmed` only after the receiver's final `ChunkAck` arrived; a peer lost before that ack, even after the last chunk was sent, is `Failed` (index R4 as amended). Tests: Task 2 `losing_the_peer_after_the_final_ack_is_unconfirmed`, `losing_the_peer_after_the_last_chunk_but_before_the_final_ack_is_failed`, `the_result_timeout_after_the_final_ack_is_unconfirmed`, `a_result_that_overtakes_the_final_ack_is_accepted`, `a_result_after_abort_or_expiry_is_ignored`, `cancel_and_connection_loss_end_the_sender_transfer`, `direction_and_outcome_use_snake_case_names`; Task 3 `network_collection_kinds_serialize_with_snake_case_fields` (the audit `outcome`).
10. **A spoofing offer name (index R15).** Bidi overrides, zero-width characters and a BOM are refused like control characters, 200 characters pass and 201 do not, and the sender uses the same public check. Tests: Task 2 `offer_name_is_valid_follows_the_spec_rule`, `an_offer_with_a_bad_hash_or_name_is_refused`.

Also covered because they share the code paths: chunk before accept, out-of-order and surplus `seq`, data beyond `size_bytes`, short non-last chunks, bad base64, expiry exactly at `TRANSFER_OFFER_TTL`, the pending and per-peer limits, the `TransferSink` `on_peer_gone` hook (`a_sink_is_object_safe_and_gets_frames_and_peer_gone`) and the unknown field on each new frame.

## File Structure

| File | Change | Task |
|---|---|---|
| `crates/rocket-network/src/limits.rs` | Modify: version 2, eleven new constants (with `SENDER_RESULT_TIMEOUT`, `ACK_EVERY_CHUNKS`, `MAX_UNACKED_CHUNKS`), pinned tests | 1 |
| `crates/rocket-network/src/messages.rs` | Modify: `AbortReason`, `ResultOutcome`, six variants (with `ChunkAck`), tests, version 2 fixtures | 1 |
| `crates/rocket-app/src/network/connections.rs` | Modify: one match arm for the six variants | 1 |
| `crates/rocket-network/src/transfer.rs` | Create: `TransferId`, `chunk_count_for`, `Direction`, `Outcome`, `OfferError`, `TransferSink` (`on_frame`, `on_peer_gone`), `offer_name_is_valid`, `OutgoingTransfer` (with the ack window `window_open`, `unacked_chunks`, `on_ack`, and `fail_with`, `on_result`, `awaiting_result`, `result_timed_out`, `with_offer_ttl`), `IncomingTransfer` (with `ChunkStep::Data.ack` and `with_offer_ttl`), `TransferLimits` | 2 |
| `crates/rocket-network/src/lib.rs` | Modify: `pub mod transfer;` and re-exports | 2, 3 |
| `crates/rocket-network/src/events.rs` | Modify: `TransferEvent` (`Finished` carries `message`) and its tests | 3 |
| `crates/rocket-network/CLAUDE.md` | Modify: public types table | 3 |
| `crates/rocket-audit/src/event.rs` | Modify: two kinds with `outcome`, `tag()`, tests | 3 |
| `crates/rocket-audit/src/control.rs` | Modify: CC6.7 catalog entry, test | 3 |

Hard-coded version 1 found by grep (`grep -rnE "version: 1|\"version\": ?1|PROTOCOL_VERSION" crates/rocket-network crates/rocket-infra crates/rocket-app`), and what happens to each:

- `crates/rocket-network/src/limits.rs`: the pinned `assert_eq!(PROTOCOL_VERSION, 1)`. Changed to 2 in Task 1.
- `crates/rocket-network/src/messages.rs`: the `hello()` fixture, the shape test JSON and five inline Hello strings. Changed to 2 in Task 1.
- Left alone, because they are independent literals that are never compared with `PROTOCOL_VERSION`: `rocket-network/src/ports.rs` (lines 286, 299, 312, an `Announcement` and a scripted socket), `records.rs:168` (`protocol_version` on a `DiscoveredNode`), `wire.rs` (62, 70, 81, `HelloResponse` fixtures), `rocket-infra/src/network/mdns.rs` (307, 428, `Announcement` fixtures), `rocket-infra/src/network/server_tests.rs:161` (a rejected profile body).
- Already use the constant, so they follow the bump with no edit: all of `rocket-infra/src/network/{server_tests,peer_test_support,transport}.rs` and `rocket-app/src/network/{e2e_tests,service_tests,test_support,connections_tests,pairing_tests,pairing,handler,nodes,service,connections}.rs`. Two of them use `PROTOCOL_VERSION + 1` as the "other version" and stay correct.

Runtime effect of the bump (intended by the spec, section 6): a version 1 peer and a version 2 peer never talk. `run_socket` closes a socket whose Hello version differs (`version_matches` in `crates/rocket-app/src/network/connections.rs`) and marks the node `needs_update`, so the UI shows it as needing an update, and `crates/rocket-app/src/network/handler.rs` (`req.version != PROTOCOL_VERSION`) refuses a pairing request from a version 1 node. The team network is unreleased, so no field node is affected. Plan 5 repeats this in `docs/features/team-network.md`.

---

## Task 1: Constants, version 2 and the six frames

**Files:**
- Modify: `crates/rocket-network/src/limits.rs`
- Modify: `crates/rocket-network/src/messages.rs`
- Modify: `crates/rocket-app/src/network/connections.rs`

**Interfaces:**
- Consumes: `PeerMessage`, `MAX_FRAME_BYTES`, `DomainError::InvalidInput`.
- Produces: the constants of the index (including `SENDER_RESULT_TIMEOUT`, `ACK_EVERY_CHUNKS = 16`, `MAX_UNACKED_CHUNKS = 32`), `AbortReason`, `ResultOutcome` (`Completed | Damaged | Rejected | Failed`, snake_case), the variants `PeerMessage::{CollectionOffer, OfferAnswer, Chunk, TransferAbort, ChunkAck, TransferResult}` (`ChunkAck { transfer_id: String, next_seq: u32 }`, tag `chunk_ack`).

- [ ] **Step 1: Write the failing limits tests**

In `crates/rocket-network/src/limits.rs`, change the first line of `spec_values_are_pinned` to `assert_eq!(PROTOCOL_VERSION, 2);` and add these assertions at the end of that test:

```rust
        assert_eq!(MAX_TRANSFER_BYTES, 10 * 1024 * 1024);
        assert_eq!(CHUNK_BYTES, 32 * 1024);
        assert_eq!(TRANSFER_OFFER_TTL, Duration::from_secs(120));
        assert_eq!(MAX_PENDING_INCOMING, 3);
        assert_eq!(MAX_ACTIVE_PER_PEER, 1);
        assert_eq!(OFFERS_PER_PEER_PER_MINUTE, 5);
        assert_eq!(MAX_UNZIPPED_BYTES, 50 * 1024 * 1024);
        assert_eq!(MAX_ZIP_ENTRIES, 5000);
        assert_eq!(SENDER_RESULT_TIMEOUT, Duration::from_secs(60));
        assert_eq!(ACK_EVERY_CHUNKS, 16);
        assert_eq!(MAX_UNACKED_CHUNKS, 32);
```

Add two tests to the same module:

```rust
    #[test]
    fn transfer_limits_are_consistent() {
        assert_eq!(TRANSFER_OFFER_TTL, PAIRING_TTL);
        assert_eq!(OFFERS_PER_PEER_PER_MINUTE, PAIR_REQUESTS_PER_MINUTE);
        assert!(MAX_TRANSFER_BYTES < MAX_UNZIPPED_BYTES);
        // The sender gives up on the result sooner than an offer waits for its answer.
        assert!(SENDER_RESULT_TIMEOUT < TRANSFER_OFFER_TTL);
        // The receiver acks twice per window, so a sender never stalls on a healthy link.
        assert!(ACK_EVERY_CHUNKS < MAX_UNACKED_CHUNKS);
        assert_eq!(MAX_UNACKED_CHUNKS % ACK_EVERY_CHUNKS, 0);
    }

    #[test]
    fn a_full_chunk_fits_a_frame_with_room_for_the_envelope() {
        // Base64 grows 3 raw bytes to 4 characters, padded.
        let encoded = CHUNK_BYTES.div_ceil(3) * 4;
        assert_eq!(encoded, 43_692);
        // The envelope is the type tag, a 32 character id and a sequence number.
        assert!(encoded + 200 < MAX_FRAME_BYTES);
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -j4 -p rocket-network limits`
Expected: FAIL to compile, `cannot find value MAX_TRANSFER_BYTES in this scope`.

- [ ] **Step 3: Add the constants**

In `crates/rocket-network/src/limits.rs` change `PROTOCOL_VERSION` to 2 and add the constants after `DISPLAY_NAME_MAX_CHARS`:

```rust
/// The peer protocol version this build speaks.
pub const PROTOCOL_VERSION: u32 = 2;
```

```rust
/// The largest collection ZIP a node sends or accepts.
pub const MAX_TRANSFER_BYTES: u64 = 10 * 1024 * 1024;
/// Raw bytes in one chunk frame, before base64.
pub const CHUNK_BYTES: usize = 32 * 1024;
/// How long an offer waits for an answer, counted from its creation.
pub const TRANSFER_OFFER_TTL: Duration = Duration::from_secs(120);
/// Offers waiting for the user across all peers.
pub const MAX_PENDING_INCOMING: usize = 3;
/// Transfers per peer at a time, in each direction.
pub const MAX_ACTIVE_PER_PEER: usize = 1;
/// Offers accepted from one peer per minute.
pub const OFFERS_PER_PEER_PER_MINUTE: u32 = 5;
/// The most bytes a received ZIP may unzip to.
pub const MAX_UNZIPPED_BYTES: u64 = 50 * 1024 * 1024;
/// The most entries a received ZIP may hold.
pub const MAX_ZIP_ENTRIES: usize = 5000;
/// How long the sender waits for the receiver's `TransferResult` after the receiver's final
/// `ChunkAck`. When it passes, the sender reports `Unconfirmed`, not a failure.
pub const SENDER_RESULT_TIMEOUT: Duration = Duration::from_secs(60);
/// The receiver sends a `ChunkAck` after this many chunks accepted in order, and after the last.
pub const ACK_EVERY_CHUNKS: u32 = 16;
/// The most chunks the sender may have sent and not yet seen acknowledged.
pub const MAX_UNACKED_CHUNKS: u32 = 32;
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -j4 -p rocket-network limits`
Expected: PASS, 5 tests: `spec_values_are_pinned` (including `SENDER_RESULT_TIMEOUT`, `ACK_EVERY_CHUNKS`, `MAX_UNACKED_CHUNKS`), `transfer_limits_are_consistent`, `a_full_chunk_fits_a_frame_with_room_for_the_envelope`, `port_range_holds_ten_ports` and `unpaired_connections_stay_below_the_total`.

- [ ] **Step 5: Write the failing message tests**

In `crates/rocket-network/src/messages.rs`, first move the existing fixtures to version 2:

```bash
sed -i 's/"version":1/"version":2/g' crates/rocket-network/src/messages.rs
```

Then edit by hand: in `hello()` change `version: 1,` to `version: 2,`, and in `encoded_frames_have_the_expected_shape` change `"version": 1,` to `"version": 2,`.
Expected: `grep -n "version\": \?1\|version: 1" crates/rocket-network/src/messages.rs` prints nothing.

Extend the imports of the test module and add the new tests. At the top of `mod tests`:

```rust
    use crate::limits::{CHUNK_BYTES, DISPLAY_NAME_MAX_CHARS};
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use serde_json::json;

    const ID: &str = "0123456789abcdef0123456789abcdef";
    const SHA: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn transfer_messages() -> Vec<PeerMessage> {
        vec![
            PeerMessage::CollectionOffer {
                transfer_id: ID.into(),
                name: "Orders".into(),
                size_bytes: 70_000,
                chunk_count: 3,
                sha256: SHA.into(),
            },
            PeerMessage::OfferAnswer {
                transfer_id: ID.into(),
                accept: true,
            },
            PeerMessage::OfferAnswer {
                transfer_id: ID.into(),
                accept: false,
            },
            PeerMessage::Chunk {
                transfer_id: ID.into(),
                seq: 2,
                data: "AAEC".into(),
            },
            PeerMessage::TransferAbort {
                transfer_id: ID.into(),
                reason: AbortReason::Cancelled,
            },
            PeerMessage::ChunkAck {
                transfer_id: ID.into(),
                next_seq: 16,
            },
            PeerMessage::TransferResult {
                transfer_id: ID.into(),
                outcome: ResultOutcome::Completed,
            },
        ]
    }
```

(Remove the old `use crate::limits::DISPLAY_NAME_MAX_CHARS;` and `use serde_json::json;` lines of the module, they are replaced by the block above.)

Replace `every_variant_round_trips` with:

```rust
    #[test]
    fn every_variant_round_trips() {
        let mut all = vec![
            hello(),
            PeerMessage::Ping,
            PeerMessage::Pong,
            PeerMessage::Unpair,
        ];
        all.extend(transfer_messages());
        for reason in [
            AbortReason::Cancelled,
            AbortReason::Failed,
            AbortReason::TooBig,
            AbortReason::Busy,
            AbortReason::Expired,
        ] {
            all.push(PeerMessage::TransferAbort {
                transfer_id: ID.into(),
                reason,
            });
        }
        for outcome in [
            ResultOutcome::Completed,
            ResultOutcome::Damaged,
            ResultOutcome::Rejected,
            ResultOutcome::Failed,
        ] {
            all.push(PeerMessage::TransferResult {
                transfer_id: ID.into(),
                outcome,
            });
        }
        for message in all {
            let text = message.encode();
            assert_eq!(PeerMessage::decode(&text).expect("decodes"), message);
        }
    }

    #[test]
    fn transfer_frames_have_the_expected_shape() {
        let messages = transfer_messages();
        assert_eq!(
            messages[0].encode(),
            format!(
                r#"{{"type":"collection_offer","transfer_id":"{ID}","name":"Orders","size_bytes":70000,"chunk_count":3,"sha256":"{SHA}"}}"#
            )
        );
        assert_eq!(
            messages[1].encode(),
            format!(r#"{{"type":"offer_answer","transfer_id":"{ID}","accept":true}}"#)
        );
        assert_eq!(
            messages[2].encode(),
            format!(r#"{{"type":"offer_answer","transfer_id":"{ID}","accept":false}}"#)
        );
        assert_eq!(
            messages[3].encode(),
            format!(r#"{{"type":"chunk","transfer_id":"{ID}","seq":2,"data":"AAEC"}}"#)
        );
        assert_eq!(
            messages[4].encode(),
            format!(r#"{{"type":"transfer_abort","transfer_id":"{ID}","reason":"cancelled"}}"#)
        );
        assert_eq!(
            messages[5].encode(),
            format!(r#"{{"type":"chunk_ack","transfer_id":"{ID}","next_seq":16}}"#)
        );
        assert_eq!(
            messages[6].encode(),
            format!(r#"{{"type":"transfer_result","transfer_id":"{ID}","outcome":"completed"}}"#)
        );
        assert_eq!(messages.len(), 7, "six variants, the answer twice");
    }

    #[test]
    fn result_outcomes_use_snake_case_names() {
        for (outcome, name) in [
            (ResultOutcome::Completed, "completed"),
            (ResultOutcome::Damaged, "damaged"),
            (ResultOutcome::Rejected, "rejected"),
            (ResultOutcome::Failed, "failed"),
        ] {
            assert_eq!(serde_json::to_value(outcome).expect("json"), json!(name));
        }
    }

    #[test]
    fn abort_reasons_use_snake_case_names() {
        for (reason, name) in [
            (AbortReason::Cancelled, "cancelled"),
            (AbortReason::Failed, "failed"),
            (AbortReason::TooBig, "too_big"),
            (AbortReason::Busy, "busy"),
            (AbortReason::Expired, "expired"),
        ] {
            assert_eq!(serde_json::to_value(reason).expect("json"), json!(name));
        }
    }

    #[test]
    fn transfer_frames_reject_unknown_fields_and_bad_values() {
        let id = ID;
        for text in [
            format!(r#"{{"type":"offer_answer","transfer_id":"{id}","accept":true,"extra":1}}"#),
            format!(r#"{{"type":"chunk","transfer_id":"{id}","seq":0,"data":"","extra":1}}"#),
            format!(
                r#"{{"type":"transfer_abort","transfer_id":"{id}","reason":"failed","extra":1}}"#
            ),
            format!(
                r#"{{"type":"collection_offer","transfer_id":"{id}","name":"A","size_bytes":1,"chunk_count":1,"sha256":"{SHA}","extra":1}}"#
            ),
            format!(
                r#"{{"type":"transfer_result","transfer_id":"{id}","outcome":"completed","extra":1}}"#
            ),
            format!(r#"{{"type":"chunk_ack","transfer_id":"{id}","next_seq":16,"extra":1}}"#),
            // Missing fields.
            format!(r#"{{"type":"offer_answer","transfer_id":"{id}"}}"#),
            format!(r#"{{"type":"chunk","transfer_id":"{id}","seq":0}}"#),
            r#"{"type":"transfer_abort","reason":"failed"}"#.to_string(),
            format!(r#"{{"type":"transfer_result","transfer_id":"{id}"}}"#),
            format!(r#"{{"type":"chunk_ack","transfer_id":"{id}"}}"#),
            // Wrong types and values.
            format!(r#"{{"type":"offer_answer","transfer_id":"{id}","accept":"yes"}}"#),
            format!(r#"{{"type":"chunk","transfer_id":"{id}","seq":-1,"data":""}}"#),
            format!(r#"{{"type":"chunk","transfer_id":"{id}","seq":4294967296,"data":""}}"#),
            format!(r#"{{"type":"transfer_abort","transfer_id":"{id}","reason":"bogus"}}"#),
            format!(r#"{{"type":"transfer_result","transfer_id":"{id}","outcome":"cancelled"}}"#),
            format!(r#"{{"type":"chunk_ack","transfer_id":"{id}","next_seq":-1}}"#),
            format!(r#"{{"type":"chunk_ack","transfer_id":"{id}","next_seq":"16"}}"#),
            format!(
                r#"{{"type":"collection_offer","transfer_id":"{id}","name":"A","size_bytes":-1,"chunk_count":1,"sha256":"{SHA}"}}"#
            ),
        ] {
            let err = PeerMessage::decode(&text).expect_err("rejected");
            assert!(matches!(err, DomainError::InvalidInput(_)), "input {text}");
        }
    }

    #[test]
    fn a_full_chunk_frame_fits_and_decodes_at_exactly_the_frame_limit() {
        let raw = vec![0xABu8; CHUNK_BYTES];
        let message = PeerMessage::Chunk {
            transfer_id: ID.into(),
            seq: u32::MAX,
            data: STANDARD.encode(&raw),
        };
        let text = message.encode();
        assert!(text.len() < MAX_FRAME_BYTES, "a full chunk leaves room");
        assert_eq!(PeerMessage::decode(&text).expect("decodes"), message);

        // JSON allows trailing white space, so pad the same frame to the exact limit.
        let padded = format!("{text}{}", " ".repeat(MAX_FRAME_BYTES - text.len()));
        assert_eq!(padded.len(), MAX_FRAME_BYTES);
        assert_eq!(PeerMessage::decode(&padded).expect("limit is allowed"), message);

        let over = format!("{padded} ");
        assert!(PeerMessage::decode(&over).is_err());
    }

    #[test]
    fn chunk_frame_boundary_is_exact() {
        let empty = PeerMessage::Chunk {
            transfer_id: ID.into(),
            seq: 0,
            data: String::new(),
        };
        let base = empty.encode().len();
        let fits = PeerMessage::Chunk {
            transfer_id: ID.into(),
            seq: 0,
            data: "A".repeat(MAX_FRAME_BYTES - base),
        };
        assert_eq!(fits.encode().len(), MAX_FRAME_BYTES);
        assert_eq!(PeerMessage::decode(&fits.encode()).expect("fits"), fits);

        let over = PeerMessage::Chunk {
            transfer_id: ID.into(),
            seq: 0,
            data: "A".repeat(MAX_FRAME_BYTES - base + 1),
        };
        assert_eq!(over.encode().len(), MAX_FRAME_BYTES + 1);
        assert!(PeerMessage::decode(&over.encode()).is_err());
    }

    #[test]
    fn a_chunk_with_an_oversize_data_field_is_rejected() {
        let message = PeerMessage::Chunk {
            transfer_id: ID.into(),
            seq: 0,
            data: "A".repeat(MAX_FRAME_BYTES * 2),
        };
        let err = PeerMessage::decode(&message.encode()).expect_err("too large");
        match err {
            DomainError::InvalidInput(text) => {
                assert_eq!(text, "frame too large");
                assert!(!text.contains("AAAA"), "the error never echoes the frame");
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    #[test]
    fn a_name_cannot_push_an_offer_past_the_frame_limit() {
        let offer = PeerMessage::CollectionOffer {
            transfer_id: ID.into(),
            name: "n".repeat(MAX_FRAME_BYTES),
            size_bytes: 1,
            chunk_count: 1,
            sha256: SHA.into(),
        };
        assert!(PeerMessage::decode(&offer.encode()).is_err());
    }
```

- [ ] **Step 6: Run to verify it fails**

Run: `cargo test -j4 -p rocket-network messages`
Expected: FAIL to compile, `cannot find type AbortReason`, `cannot find type ResultOutcome`, no variant `CollectionOffer` and no variant `ChunkAck`.

- [ ] **Step 7: Add the variants**

In `crates/rocket-network/src/messages.rs`, replace the enum and its doc line with:

```rust
/// Why a transfer stopped. Sent inside `TransferAbort`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbortReason {
    Cancelled,
    Failed,
    TooBig,
    Busy,
    Expired,
}

/// What the receiver tells the sender once the import is done. Sent inside `TransferResult`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultOutcome {
    Completed,
    Damaged,
    Rejected,
    Failed,
}

/// A text frame on the peer WebSocket. Later projects add variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PeerMessage {
    Hello {
        version: u32,
        profile: NodeProfile,
    },
    Ping,
    Pong,
    Unpair,
    /// The sender offers a collection ZIP. `sha256` is lowercase hex.
    CollectionOffer {
        transfer_id: String,
        name: String,
        size_bytes: u64,
        chunk_count: u32,
        sha256: String,
    },
    /// The receiver accepts or declines an offer.
    OfferAnswer {
        transfer_id: String,
        accept: bool,
    },
    /// One piece of the ZIP. `data` is standard base64, padded.
    Chunk {
        transfer_id: String,
        seq: u32,
        data: String,
    },
    /// Either side stops the transfer.
    TransferAbort {
        transfer_id: String,
        reason: AbortReason,
    },
    /// The receiver acknowledges chunks: `next_seq` is how many it accepted in order. Sent
    /// after every `ACK_EVERY_CHUNKS` chunks and after the last one.
    ChunkAck {
        transfer_id: String,
        next_seq: u32,
    },
    /// The receiver reports the true outcome after the last chunk and the import.
    TransferResult {
        transfer_id: String,
        outcome: ResultOutcome,
    },
}
```

`decode` needs no change: `deny_unknown_fields` and the size check already apply to the new variants, and its `other => Ok(other)` arm covers them.

- [ ] **Step 8: Run to verify it passes**

Run: `cargo test -j4 -p rocket-network messages`
Expected: PASS, including `every_variant_round_trips`, `transfer_frames_have_the_expected_shape` (with the `chunk_ack` frame), `result_outcomes_use_snake_case_names`, `chunk_frame_boundary_is_exact` and the older tests (`decode_rejects_unknown_type_without_echoing_it` still passes because `send_collection` is not a variant).

- [ ] **Step 9: Keep the workspace compiling (rocket-app arm)**

The `match` in `run_socket` is exhaustive, so `cargo check -j4 -p rocket-app` now fails. In `crates/rocket-app/src/network/connections.rs`, after the `Some(PeerMessage::Hello { .. }) => return SocketEnd::Closed,` arm, add:

```rust
                    // Plan 3 routes the six transfer frames to the transfer sink. Until then
                    // they are ignored, so a version 2 peer can send them without effect.
                    Some(
                        PeerMessage::CollectionOffer { .. }
                        | PeerMessage::OfferAnswer { .. }
                        | PeerMessage::Chunk { .. }
                        | PeerMessage::TransferAbort { .. }
                        | PeerMessage::ChunkAck { .. }
                        | PeerMessage::TransferResult { .. },
                    ) => {}
```

This is deliberately an explicit or-pattern, not `_ => {}`, so a future variant still fails to compile here. This is the only edit outside `rocket-network` and `rocket-audit` in this plan.

- [ ] **Step 10: Verify the dependants**

Run: `cargo check -j4 -p rocket-app`
Expected: Finished, no errors.

Run: `cargo test -j4 -p rocket-network`
Expected: all tests pass.

Run: `cargo test -j4 -p rocket-infra network::`
Expected: all network tests pass (they use `PROTOCOL_VERSION`, so they follow the bump).

Run: `cargo test -j4 -p rocket-app network::`
Expected: all network tests pass, including the `PROTOCOL_VERSION + 1` mismatch tests and `e2e_tests`.

Run: `grep -rn "protocol_version\|protocolVersion\|PROTOCOL_VERSION" src src-tauri/src --include=*.ts --include=*.tsx --include=*.rs | grep -v "^src-tauri/src/.*PROTOCOL_VERSION"`
Expected: any TypeScript hit is a display value only. Record it for Plan 5 and do not edit TypeScript here. No hit means nothing to record.

---

## Task 2: Transfer ids, state machines and offer limits

**Files:**
- Create: `crates/rocket-network/src/transfer.rs`
- Modify: `crates/rocket-network/src/lib.rs`

**Interfaces:**
- Consumes: `limits::{CHUNK_BYTES, MAX_TRANSFER_BYTES, TRANSFER_OFFER_TTL, MAX_PENDING_INCOMING, MAX_ACTIVE_PER_PEER, OFFERS_PER_PEER_PER_MINUTE, ACK_EVERY_CHUNKS, MAX_UNACKED_CHUNKS}`, `PeerMessage`, `AbortReason`, `ResultOutcome`, `DeviceId` (`Copy`, `Hash`), `SlidingWindow::{new, try_hit}` from `rate.rs`.
- Produces (index names, binding): `TransferId` (`random`, `parse`, `as_str`), `chunk_count_for`, `offer_name_is_valid(&str) -> bool` (the spec section 6 name rule; Plan 3 uses it before a send to give "This collection name cannot be sent. Rename it and try again."), `Direction`, `Outcome` (with `Unconfirmed`, wire `unconfirmed`, and `From<ResultOutcome>`), `TransferSink` (two methods: `on_frame(&self, from: DeviceId, message: PeerMessage)` and `on_peer_gone(&self, device: DeviceId)`), `OutgoingTransfer`, `IncomingTransfer`. Extra, used by Plan 3: `OfferError`, `OutgoingState`, `IncomingState`, `ChunkStep`, `Admission`, `TransferLimits`, `OutgoingTransfer::fail_with(&mut self, Outcome) -> Option<Outcome>` (the sender twin of the receiver's method, works after `Done`), `OutgoingTransfer::on_result(&mut self, ResultOutcome) -> Option<Outcome>` and `OutgoingTransfer::awaiting_result(&self) -> bool` and `OutgoingTransfer::last_chunk_built(&self) -> bool` (true from the last chunk frame on, before the final ack; the sender twin of the receiver's final report; see the design notes), the ack window `OutgoingTransfer::window_open(&self) -> bool`, `OutgoingTransfer::unacked_chunks(&self) -> u32` and `OutgoingTransfer::on_ack(&mut self, next_seq: u32) -> Option<Outcome>`, `OutgoingTransfer::result_timed_out(&mut self) -> Option<Outcome>` (`Unconfirmed` while `awaiting_result`, which is true only after the final `ChunkAck`), and `ChunkStep::Data { bytes, done_bytes, last, ack: Option<PeerMessage> }` (the `ChunkAck` frame to send, `Some` on every 16th accepted chunk and on the last). Test seam for Plan 4: `#[doc(hidden)] pub fn with_offer_ttl(self, ttl: Duration) -> Self` on both `OutgoingTransfer` and `IncomingTransfer` (builder style, default `TRANSFER_OFFER_TTL`).

Design notes:

- Final states carry an `Outcome`. `outcome()` is `None` while the transfer is active. `Done` means all bytes moved (`Outcome::Completed`). The receiver service may still turn a `Done` transfer into `Damaged`, `Rejected` or `Failed` with `fail_with`, after it checked the hash or the import failed. The sender machine reaches `Done` the moment the last chunk frame is built, which is before that frame is sent and before the peer could report a failure, so `OutgoingTransfer::fail_with` works from `Done` too (cancel, a failed send of the last chunk and a peer failure report and the result timeout all arrive after `Done`).
- The sender machine stays `Done` with outcome `Completed` until the receiver's `TransferResult` arrives. `awaiting_result()` is true in exactly that window (`Done`, final ack received, no result yet). `on_result` accepts one result in that window: `Completed` keeps the machine `Done` and returns `Some(Completed)`, `Damaged`, `Rejected` and `Failed` end it `Aborted` with the same-named `Outcome`. Anything else returns `None` and changes nothing (a result before the last chunk, a second result, a result after an abort or expiry). After a confirmed `Completed` the machine can no longer be failed. The 60 s wait (`SENDER_RESULT_TIMEOUT`) is a timer in Plan 3 that calls `result_timed_out()`, which gives `Unconfirmed` (index R4). `awaiting_result()` is true only after the receiver's final `ChunkAck` (`next_seq == chunk_count`) arrived and before the result did; between building the last chunk and that ack the machine is `Done` without awaiting a result. `connection_lost()` while `awaiting_result()` gives `Unconfirmed`; before the final ack, including after the last chunk was built or sent, it gives `Failed`, and `result_timed_out()` is ignored there (the idle limit decides). `on_result` is accepted from the moment the last chunk is built, so a result that overtakes the final ack still counts. `Unconfirmed` leaves the machine `Done` with outcome `Unconfirmed` and no longer awaiting a result: it is final, a late result or `fail_with` changes nothing.
- Ack window (index R2). The sender counts `acked` (the highest `next_seq` seen). `next_chunk` refuses once `next_seq - acked == MAX_UNACKED_CHUNKS` (`window_open()` is false), so Plan 3 waits for an ack before it takes the next slice. `on_ack(next_seq)` is accepted in `Accepted`, `Sending` and in `Done` while `awaiting_result()`: an equal `next_seq` is a harmless repeat, a lower one (backwards) or one above the chunks sent ends the transfer `Failed`, and an ack while `Offered` is a protocol error and also `Failed`. After a final state an ack is ignored (`None`). The receiver puts the `ChunkAck` frame into `ChunkStep::Data.ack` when `next_seq % ACK_EVERY_CHUNKS == 0` or the chunk is the last, so the caller sends it right after staging the bytes.
- The offer lifetime is a field (`offer_ttl`, default `TRANSFER_OFFER_TTL`) that `with_offer_ttl` overrides. `expire_if_due`, `on_answer` and `respond` compare against the field, never the constant. The builder is `#[doc(hidden)]` and exists only so Plan 4's tests can shorten the offer lifetime.
- Only an `Offered` transfer expires. An accepted transfer has no timer in this plan; a stalled stream (a lost frame, a socket swapped by the duplicate rule, a sender waiting for an ack) ends by connection loss (`connection_lost`) or by the service's idle limit (Plan 3, `Timing.idle_limit`, 120 s), which calls `fail_with(Outcome::Failed)` (index R7).
- Chunks are strict. A non-last chunk carries exactly `CHUNK_BYTES` raw bytes and the last one exactly the remainder. This makes "data beyond `size_bytes`" and "short chunk" the same check: the decoded length must equal `min(remaining, CHUNK_BYTES)`.
- A name is validated like the other offer fields by `offer_name_is_valid` (spec section 6): not empty after trim, at most 200 characters, no control characters (Cc) and none of the bidi or zero-width format characters U+00AD, U+061C, U+180E, U+200B to U+200F, U+202A to U+202E, U+2060 to U+2064, U+2066 to U+2069, U+FEFF. The name is shown in the UI and later becomes a file stem, so an unbounded name or a name that reorders text in the dialog is refused here. `:` and Windows reserved names are allowed in an offer name; Plan 2 cleans them when it builds the folder name.

- [ ] **Step 1: Register the module and write the failing tests**

In `crates/rocket-network/src/lib.rs` add `pub mod transfer;` after `pub mod records;` and re-export (after the `records` re-export):

```rust
pub use transfer::{
    chunk_count_for, offer_name_is_valid, Admission, ChunkStep, Direction, IncomingState,
    IncomingTransfer, OfferError, OutgoingState, OutgoingTransfer, Outcome, TransferId,
    TransferLimits, TransferSink,
};
```

Also change the messages re-export to `pub use messages::{AbortReason, PeerMessage, ResultOutcome};`.

Create `crates/rocket-network/src/transfer.rs` with only the test module first (the types follow in Step 3, so the file does not compile yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::{
        ACK_EVERY_CHUNKS, CHUNK_BYTES, MAX_ACTIVE_PER_PEER, MAX_PENDING_INCOMING,
        MAX_TRANSFER_BYTES, MAX_UNACKED_CHUNKS, OFFERS_PER_PEER_PER_MINUTE, TRANSFER_OFFER_TTL,
    };
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    const SHA: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const ID: &str = "0123456789abcdef0123456789abcdef";
    const CB: u64 = CHUNK_BYTES as u64;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn id() -> TransferId {
        TransferId::parse(ID).expect("valid id")
    }

    fn outgoing(size: u64, now: Instant) -> OutgoingTransfer {
        OutgoingTransfer::new(id(), "Orders", size, SHA, now).expect("valid offer")
    }

    fn incoming(size: u64, now: Instant) -> IncomingTransfer {
        IncomingTransfer::from_offer(ID, "Orders", size, chunk_count_for(size), SHA, now)
            .expect("valid offer")
    }

    fn accepted(size: u64, now: Instant) -> IncomingTransfer {
        let mut transfer = incoming(size, now);
        assert!(transfer.respond(true, now).is_some());
        transfer
    }

    fn b64(len: usize) -> String {
        STANDARD.encode(vec![7u8; len])
    }

    fn peer(n: u8) -> DeviceId {
        DeviceId::from_bytes([n; 32])
    }

    struct RecordingSink(Mutex<Vec<(&'static str, DeviceId)>>);

    impl TransferSink for RecordingSink {
        fn on_frame(&self, from: DeviceId, _message: PeerMessage) {
            self.0.lock().expect("lock").push(("frame", from));
        }

        fn on_peer_gone(&self, device: DeviceId) {
            self.0.lock().expect("lock").push(("gone", device));
        }
    }

    /// The last chunk frame is built, but the receiver's final `ChunkAck` has not arrived.
    fn last_chunk_sent(now: Instant) -> OutgoingTransfer {
        let mut t = outgoing(10, now);
        t.on_answer(true, now);
        t.next_chunk(&[0u8; 10]).expect("last chunk");
        assert_eq!(t.state(), OutgoingState::Done);
        assert!(!t.awaiting_result(), "no final ack yet");
        t
    }

    /// The final `ChunkAck` (`next_seq == chunk_count`, 1 here) arrived: now the result is awaited.
    fn waiting_for_result(now: Instant) -> OutgoingTransfer {
        let mut t = last_chunk_sent(now);
        assert_eq!(t.on_ack(1), None);
        assert!(t.awaiting_result());
        t
    }

    // ---- ids and counts ----

    #[test]
    fn a_sink_is_object_safe_and_gets_frames_and_peer_gone() {
        let sink = Arc::new(RecordingSink(Mutex::new(Vec::new())));
        let dynamic: Arc<dyn TransferSink> = sink.clone();
        dynamic.on_frame(peer(1), PeerMessage::Ping);
        dynamic.on_peer_gone(peer(2));
        assert_eq!(
            *sink.0.lock().expect("lock"),
            vec![("frame", peer(1)), ("gone", peer(2))]
        );
    }

    #[test]
    fn random_ids_are_32_lowercase_hex_and_differ() {
        let a = TransferId::random();
        let b = TransferId::random();
        assert_eq!(a.as_str().len(), 32);
        assert!(a
            .as_str()
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)));
        assert_ne!(a, b);
        assert_eq!(TransferId::parse(a.as_str()).expect("round trip"), a);
    }

    #[test]
    fn parse_rejects_anything_but_32_lowercase_hex() {
        assert!(TransferId::parse(ID).is_ok());
        for bad in [
            "",
            "0123456789abcdef0123456789abcde",
            "0123456789abcdef0123456789abcdef0",
            "0123456789ABCDEF0123456789ABCDEF",
            "0123456789abcdef0123456789abcdeg",
            "0123456789abcdef0123456789abcde ",
            // 32 bytes long but 16 characters.
            "éééééééééééééééé",
        ] {
            assert!(TransferId::parse(bad).is_err(), "input {bad:?}");
        }
    }

    #[test]
    fn chunk_count_rounds_up() {
        assert_eq!(chunk_count_for(0), 0);
        assert_eq!(chunk_count_for(1), 1);
        assert_eq!(chunk_count_for(CB), 1);
        assert_eq!(chunk_count_for(CB + 1), 2);
        assert_eq!(chunk_count_for(2 * CB), 2);
        assert_eq!(chunk_count_for(MAX_TRANSFER_BYTES), 320);
        assert_eq!(chunk_count_for(MAX_TRANSFER_BYTES + 1), 321);
        assert_eq!(chunk_count_for(u64::MAX), u32::MAX);
    }

    #[test]
    fn direction_and_outcome_use_snake_case_names() {
        assert_eq!(serde_json::to_value(Direction::Send).expect("json"), json!("send"));
        assert_eq!(
            serde_json::to_value(Direction::Receive).expect("json"),
            json!("receive")
        );
        for (outcome, name) in [
            (Outcome::Completed, "completed"),
            (Outcome::Unconfirmed, "unconfirmed"),
            (Outcome::Declined, "declined"),
            (Outcome::Expired, "expired"),
            (Outcome::Cancelled, "cancelled"),
            (Outcome::Failed, "failed"),
            (Outcome::TooBig, "too_big"),
            (Outcome::Busy, "busy"),
            (Outcome::Damaged, "damaged"),
            (Outcome::Rejected, "rejected"),
        ] {
            assert_eq!(serde_json::to_value(outcome).expect("json"), json!(name));
        }
    }

    #[test]
    fn abort_reasons_map_to_outcomes() {
        assert_eq!(Outcome::from(AbortReason::Cancelled), Outcome::Cancelled);
        assert_eq!(Outcome::from(AbortReason::Failed), Outcome::Failed);
        assert_eq!(Outcome::from(AbortReason::TooBig), Outcome::TooBig);
        assert_eq!(Outcome::from(AbortReason::Busy), Outcome::Busy);
        assert_eq!(Outcome::from(AbortReason::Expired), Outcome::Expired);
    }

    #[test]
    fn result_outcomes_map_to_outcomes() {
        assert_eq!(Outcome::from(ResultOutcome::Completed), Outcome::Completed);
        assert_eq!(Outcome::from(ResultOutcome::Damaged), Outcome::Damaged);
        assert_eq!(Outcome::from(ResultOutcome::Rejected), Outcome::Rejected);
        assert_eq!(Outcome::from(ResultOutcome::Failed), Outcome::Failed);
    }

    // ---- offer validation (receiver) ----

    #[test]
    fn an_offer_of_size_zero_is_refused() {
        let err = IncomingTransfer::from_offer(ID, "Orders", 0, 0, SHA, Instant::now())
            .expect_err("empty");
        assert_eq!(err, OfferError::Empty);
        assert_eq!(err.abort_reason(), AbortReason::Failed);
    }

    #[test]
    fn the_size_limit_is_exact() {
        let now = Instant::now();
        assert!(IncomingTransfer::from_offer(
            ID,
            "Orders",
            MAX_TRANSFER_BYTES,
            chunk_count_for(MAX_TRANSFER_BYTES),
            SHA,
            now
        )
        .is_ok());
        let err = IncomingTransfer::from_offer(
            ID,
            "Orders",
            MAX_TRANSFER_BYTES + 1,
            chunk_count_for(MAX_TRANSFER_BYTES + 1),
            SHA,
            now,
        )
        .expect_err("one byte over");
        assert_eq!(err, OfferError::TooBig);
        assert_eq!(err.abort_reason(), AbortReason::TooBig);
        let err = IncomingTransfer::from_offer(ID, "Orders", u64::MAX, u32::MAX, SHA, now)
            .expect_err("huge");
        assert_eq!(err, OfferError::TooBig);
    }

    #[test]
    fn a_chunk_count_that_does_not_match_the_size_is_refused() {
        let now = Instant::now();
        for (size, count) in [(CB + 1, 1), (CB + 1, 3), (CB, 2), (10, 0), (10, 2)] {
            let err = IncomingTransfer::from_offer(ID, "Orders", size, count, SHA, now)
                .expect_err("lying count");
            assert_eq!(err, OfferError::ChunkCountMismatch, "size {size} count {count}");
            assert_eq!(err.abort_reason(), AbortReason::Failed);
        }
    }

    #[test]
    fn an_offer_with_a_bad_id_is_refused() {
        for bad in ["", "short", "0123456789ABCDEF0123456789ABCDEF"] {
            let err = IncomingTransfer::from_offer(bad, "Orders", 10, 1, SHA, Instant::now())
                .expect_err("bad id");
            assert_eq!(err, OfferError::BadId, "input {bad:?}");
        }
    }

    #[test]
    fn an_offer_with_a_bad_hash_or_name_is_refused() {
        let now = Instant::now();
        for bad in [&SHA[..63], &SHA.to_uppercase(), "", "zz"] {
            let err = IncomingTransfer::from_offer(ID, "Orders", 10, 1, bad, now)
                .expect_err("bad hash");
            assert_eq!(err, OfferError::BadHash, "input {bad:?}");
        }
        let long = "n".repeat(201);
        for bad in [
            "",
            "   ",
            "bad\u{1b}name",
            "line\nbreak",
            "evil\u{202e}gpj.exe",
            "zero\u{200b}width",
            "\u{feff}Orders",
            long.as_str(),
        ] {
            let err = IncomingTransfer::from_offer(ID, bad, 10, 1, SHA, now).expect_err("bad name");
            assert_eq!(err, OfferError::BadName, "input {bad:?}");
        }
        let longest = "n".repeat(200);
        assert!(IncomingTransfer::from_offer(ID, &longest, 10, 1, SHA, now).is_ok());
    }

    #[test]
    fn offer_name_is_valid_follows_the_spec_rule() {
        for good in ["Orders", "Orders: v2", "Café API", "NUL", "注文", &"n".repeat(200)] {
            assert!(offer_name_is_valid(good), "input {good:?}");
        }
        // 200 multi-byte characters still pass: the limit counts characters, not bytes.
        assert!(offer_name_is_valid(&"é".repeat(200)));
        assert!(!offer_name_is_valid(&"n".repeat(201)));
        assert!(!offer_name_is_valid(""));
        assert!(!offer_name_is_valid(" \t "));
        for bad in [
            '\u{0}', '\u{7}', '\u{7f}', '\u{85}', '\u{ad}', '\u{61c}', '\u{180e}', '\u{200b}',
            '\u{200d}', '\u{200f}', '\u{202a}', '\u{202e}', '\u{2060}', '\u{2064}', '\u{2066}',
            '\u{2069}', '\u{feff}',
        ] {
            let name = format!("Ord{bad}ers");
            assert!(!offer_name_is_valid(&name), "char U+{:04X}", bad as u32);
        }
        // The sender refuses the same names.
        assert_eq!(
            OutgoingTransfer::new(id(), "evil\u{202e}txt", 10, SHA, Instant::now())
                .expect_err("bidi"),
            OfferError::BadName
        );
    }

    #[test]
    fn the_sender_refuses_the_same_offers() {
        let now = Instant::now();
        assert_eq!(
            OutgoingTransfer::new(id(), "Orders", 0, SHA, now).expect_err("empty"),
            OfferError::Empty
        );
        assert!(OutgoingTransfer::new(id(), "Orders", MAX_TRANSFER_BYTES, SHA, now).is_ok());
        assert_eq!(
            OutgoingTransfer::new(id(), "Orders", MAX_TRANSFER_BYTES + 1, SHA, now)
                .expect_err("one byte over"),
            OfferError::TooBig
        );
        assert_eq!(
            OutgoingTransfer::new(id(), "Orders", 10, "nothex", now).expect_err("hash"),
            OfferError::BadHash
        );
    }

    // ---- sender ----

    #[test]
    fn the_sender_builds_the_offer_frame() {
        let t = outgoing(2 * CB + 10, Instant::now());
        assert_eq!(
            t.offer_message(),
            PeerMessage::CollectionOffer {
                transfer_id: ID.into(),
                name: "Orders".into(),
                size_bytes: 2 * CB + 10,
                chunk_count: 3,
                sha256: SHA.into(),
            }
        );
        assert_eq!(t.state(), OutgoingState::Offered);
        assert_eq!(t.outcome(), None);
        assert_eq!(t.total_bytes(), 2 * CB + 10);
    }

    #[test]
    fn the_sender_streams_every_chunk_after_an_accept() {
        let now = Instant::now();
        let mut t = outgoing(2 * CB + 10, now);
        assert_eq!(t.on_answer(true, now), None);
        assert_eq!(t.state(), OutgoingState::Accepted);

        let first = t.next_chunk(&vec![1u8; CHUNK_BYTES]).expect("chunk 0");
        assert_eq!(
            first,
            PeerMessage::Chunk {
                transfer_id: ID.into(),
                seq: 0,
                data: STANDARD.encode(vec![1u8; CHUNK_BYTES]),
            }
        );
        assert_eq!(t.state(), OutgoingState::Sending);
        assert_eq!(t.done_bytes(), CB);

        t.next_chunk(&vec![2u8; CHUNK_BYTES]).expect("chunk 1");
        assert_eq!(t.state(), OutgoingState::Sending);
        let last = t.next_chunk(&[3u8; 10]).expect("chunk 2");
        assert!(matches!(last, PeerMessage::Chunk { seq: 2, .. }));
        assert_eq!(t.state(), OutgoingState::Done);
        assert_eq!(t.outcome(), Some(Outcome::Completed));
        assert_eq!(t.done_bytes(), 2 * CB + 10);
        assert!(!t.is_active());
    }

    #[test]
    fn the_sender_refuses_chunks_it_may_not_send() {
        let now = Instant::now();
        let mut t = outgoing(CB + 10, now);
        assert!(t.next_chunk(&vec![0u8; CHUNK_BYTES]).is_err(), "before the accept");
        t.on_answer(true, now);
        assert!(t.next_chunk(&[0u8; 5]).is_err(), "a short non-last chunk");
        assert!(t.next_chunk(&vec![0u8; CHUNK_BYTES + 1]).is_err(), "too long");
        assert_eq!(t.done_bytes(), 0, "a refused chunk changes nothing");
        t.next_chunk(&vec![0u8; CHUNK_BYTES]).expect("chunk 0");
        assert!(t.next_chunk(&[0u8; 11]).is_err(), "beyond the size");
        t.next_chunk(&[0u8; 10]).expect("last chunk");
        assert!(t.next_chunk(&[0u8; 1]).is_err(), "after the last chunk");
    }

    #[test]
    fn a_decline_ends_the_sender_transfer() {
        let now = Instant::now();
        let mut t = outgoing(10, now);
        assert_eq!(t.on_answer(false, now), Some(Outcome::Declined));
        assert_eq!(t.state(), OutgoingState::Aborted);
        assert_eq!(t.outcome(), Some(Outcome::Declined));
        assert!(t.next_chunk(&[0u8; 10]).is_err());
    }

    #[test]
    fn an_offer_expires_exactly_at_the_ttl() {
        let t0 = Instant::now();
        let mut t = outgoing(10, t0);
        assert!(!t.expire_if_due(t0 + TRANSFER_OFFER_TTL - ms(1)));
        assert_eq!(t.state(), OutgoingState::Offered);
        assert!(t.expire_if_due(t0 + TRANSFER_OFFER_TTL));
        assert_eq!(t.state(), OutgoingState::Expired);
        assert_eq!(t.outcome(), Some(Outcome::Expired));
        assert!(!t.expire_if_due(t0 + TRANSFER_OFFER_TTL * 2), "only once");
    }

    #[test]
    fn a_late_answer_finds_the_offer_expired() {
        let t0 = Instant::now();
        let mut t = outgoing(10, t0);
        assert_eq!(t.on_answer(true, t0 + TRANSFER_OFFER_TTL), Some(Outcome::Expired));
        assert_eq!(t.state(), OutgoingState::Expired);

        let mut early = outgoing(10, t0);
        assert_eq!(early.on_answer(true, t0 + TRANSFER_OFFER_TTL - ms(1)), None);
        assert_eq!(early.state(), OutgoingState::Accepted);
    }

    #[test]
    fn an_accepted_transfer_does_not_expire() {
        let t0 = Instant::now();
        let mut t = outgoing(10, t0);
        t.on_answer(true, t0);
        assert!(!t.expire_if_due(t0 + TRANSFER_OFFER_TTL * 10));
        assert_eq!(t.state(), OutgoingState::Accepted);
    }

    #[test]
    fn a_second_answer_is_ignored() {
        let now = Instant::now();
        let mut t = outgoing(10, now);
        t.on_answer(true, now);
        assert_eq!(t.on_answer(false, now), None);
        assert_eq!(t.state(), OutgoingState::Accepted);
    }

    #[test]
    fn a_peer_abort_ends_an_active_sender_transfer_once() {
        let now = Instant::now();
        let mut offered = outgoing(10, now);
        assert_eq!(offered.on_abort(AbortReason::Busy), Some(Outcome::Busy));
        assert_eq!(offered.on_abort(AbortReason::Failed), None, "already final");
        assert_eq!(offered.outcome(), Some(Outcome::Busy));

        let mut sending = outgoing(CB + 1, now);
        sending.on_answer(true, now);
        sending.next_chunk(&vec![0u8; CHUNK_BYTES]).expect("chunk 0");
        assert_eq!(sending.on_abort(AbortReason::TooBig), Some(Outcome::TooBig));

        let mut done = outgoing(10, now);
        done.on_answer(true, now);
        done.next_chunk(&[0u8; 10]).expect("only chunk");
        assert_eq!(done.on_abort(AbortReason::Failed), None, "a finished send stays done");
        assert_eq!(done.outcome(), Some(Outcome::Completed));
    }

    #[test]
    fn fail_with_ends_an_offered_or_sending_transfer() {
        let now = Instant::now();
        let mut offered = outgoing(10, now);
        assert_eq!(offered.fail_with(Outcome::Failed), Some(Outcome::Failed));
        assert_eq!(offered.state(), OutgoingState::Aborted);
        assert_eq!(offered.outcome(), Some(Outcome::Failed));
        assert_eq!(offered.fail_with(Outcome::Failed), None, "idempotent");

        let mut sending = outgoing(CB + 1, now);
        sending.on_answer(true, now);
        sending.next_chunk(&vec![0u8; CHUNK_BYTES]).expect("chunk 0");
        assert_eq!(sending.state(), OutgoingState::Sending);
        assert_eq!(sending.fail_with(Outcome::Cancelled), Some(Outcome::Cancelled));
        assert_eq!(sending.state(), OutgoingState::Aborted);
        assert_eq!(sending.outcome(), Some(Outcome::Cancelled));
        assert_eq!(sending.fail_with(Outcome::Failed), None, "already final");
        assert_eq!(sending.outcome(), Some(Outcome::Cancelled));
    }

    #[test]
    fn the_service_can_fail_a_sender_transfer_after_the_last_chunk() {
        let now = Instant::now();
        let mut t = outgoing(10, now);
        t.on_answer(true, now);
        t.next_chunk(&[0u8; 10]).expect("last chunk");
        assert_eq!(t.state(), OutgoingState::Done);
        assert_eq!(t.cancel(), None, "cancel is too late after Done");
        assert_eq!(t.fail_with(Outcome::Failed), Some(Outcome::Failed));
        assert_eq!(t.state(), OutgoingState::Aborted);
        assert_eq!(t.outcome(), Some(Outcome::Failed));
        assert_eq!(t.fail_with(Outcome::Failed), None, "a second call is a no-op");
        assert_eq!(t.fail_with(Outcome::Completed), None, "not a failure");
        assert_eq!(t.outcome(), Some(Outcome::Failed));
    }

    #[test]
    fn fail_with_completed_is_refused_and_changes_nothing() {
        let now = Instant::now();
        let mut offered = outgoing(10, now);
        assert_eq!(offered.fail_with(Outcome::Completed), None);
        assert_eq!(offered.state(), OutgoingState::Offered);
        assert_eq!(offered.outcome(), None);

        let mut done = outgoing(10, now);
        done.on_answer(true, now);
        done.next_chunk(&[0u8; 10]).expect("last chunk");
        assert_eq!(done.fail_with(Outcome::Completed), None);
        assert_eq!(done.state(), OutgoingState::Done);
        assert_eq!(done.outcome(), Some(Outcome::Completed));
    }

    #[test]
    fn fail_with_after_expiry_returns_none() {
        let t0 = Instant::now();
        let mut t = outgoing(10, t0);
        assert!(t.expire_if_due(t0 + TRANSFER_OFFER_TTL));
        assert_eq!(t.fail_with(Outcome::Failed), None);
        assert_eq!(t.state(), OutgoingState::Expired);
        assert_eq!(t.outcome(), Some(Outcome::Expired));
    }

    #[test]
    fn a_short_sender_offer_ttl_expires_the_offer_at_exactly_that_ttl() {
        let t0 = Instant::now();
        let one = Duration::from_secs(1);
        let mut t = outgoing(10, t0).with_offer_ttl(one);
        assert!(!t.expire_if_due(t0 + one - ms(1)));
        assert_eq!(t.state(), OutgoingState::Offered);
        assert!(t.expire_if_due(t0 + one));
        assert_eq!(t.state(), OutgoingState::Expired);

        let mut late = outgoing(10, t0).with_offer_ttl(one);
        assert_eq!(late.on_answer(true, t0 + one), Some(Outcome::Expired));
    }

    #[test]
    fn the_default_sender_offer_ttl_is_the_constant() {
        let t0 = Instant::now();
        let mut t = outgoing(10, t0);
        assert!(!t.expire_if_due(t0 + Duration::from_secs(1)), "not the short test ttl");
        assert!(!t.expire_if_due(t0 + TRANSFER_OFFER_TTL - ms(1)));
        assert!(t.expire_if_due(t0 + TRANSFER_OFFER_TTL));
    }

    #[test]
    fn a_result_while_done_ends_the_sender_transfer_with_the_real_outcome() {
        for (result, outcome, state) in [
            (ResultOutcome::Completed, Outcome::Completed, OutgoingState::Done),
            (ResultOutcome::Damaged, Outcome::Damaged, OutgoingState::Aborted),
            (ResultOutcome::Rejected, Outcome::Rejected, OutgoingState::Aborted),
            (ResultOutcome::Failed, Outcome::Failed, OutgoingState::Aborted),
        ] {
            let mut t = waiting_for_result(Instant::now());
            assert!(t.awaiting_result(), "result {result:?}");
            assert_eq!(t.on_result(result), Some(outcome), "result {result:?}");
            assert_eq!(t.state(), state, "result {result:?}");
            assert_eq!(t.outcome(), Some(outcome), "result {result:?}");
            assert!(!t.awaiting_result(), "result {result:?}");
        }
    }

    #[test]
    fn a_second_result_is_ignored() {
        let mut confirmed = waiting_for_result(Instant::now());
        assert_eq!(confirmed.on_result(ResultOutcome::Completed), Some(Outcome::Completed));
        assert_eq!(confirmed.on_result(ResultOutcome::Failed), None);
        assert_eq!(confirmed.state(), OutgoingState::Done);
        assert_eq!(confirmed.outcome(), Some(Outcome::Completed));

        let mut damaged = waiting_for_result(Instant::now());
        assert_eq!(damaged.on_result(ResultOutcome::Damaged), Some(Outcome::Damaged));
        assert_eq!(damaged.on_result(ResultOutcome::Completed), None);
        assert_eq!(damaged.outcome(), Some(Outcome::Damaged));
    }

    #[test]
    fn a_result_before_the_last_chunk_is_ignored() {
        let now = Instant::now();
        let mut t = outgoing(CB + 1, now);
        assert!(!t.awaiting_result());
        assert_eq!(t.on_result(ResultOutcome::Completed), None, "offered");
        t.on_answer(true, now);
        assert_eq!(t.on_result(ResultOutcome::Completed), None, "accepted");
        t.next_chunk(&vec![0u8; CHUNK_BYTES]).expect("chunk 0");
        assert_eq!(t.on_result(ResultOutcome::Completed), None, "sending");
        assert_eq!(t.state(), OutgoingState::Sending);
        assert_eq!(t.outcome(), None);
        // The transfer still finishes normally and then waits for a real result.
        t.next_chunk(&[0u8; 1]).expect("last chunk");
        assert!(!t.awaiting_result(), "the final ack is still missing");
        assert_eq!(t.on_ack(2), None);
        assert!(t.awaiting_result());
        assert_eq!(t.on_result(ResultOutcome::Completed), Some(Outcome::Completed));
    }

    #[test]
    fn a_result_after_abort_or_expiry_is_ignored() {
        let t0 = Instant::now();
        let mut cancelled = outgoing(10, t0);
        cancelled.cancel();
        assert_eq!(cancelled.on_result(ResultOutcome::Completed), None);
        assert_eq!(cancelled.outcome(), Some(Outcome::Cancelled));

        let mut expired = outgoing(10, t0);
        expired.expire_if_due(t0 + TRANSFER_OFFER_TTL);
        assert_eq!(expired.on_result(ResultOutcome::Completed), None);
        assert_eq!(expired.outcome(), Some(Outcome::Expired));

        // The result timeout fired first: the late result changes nothing.
        let mut timed_out = waiting_for_result(t0);
        assert_eq!(timed_out.result_timed_out(), Some(Outcome::Unconfirmed));
        assert!(!timed_out.awaiting_result());
        assert_eq!(timed_out.on_result(ResultOutcome::Completed), None);
        assert_eq!(timed_out.state(), OutgoingState::Done);
        assert_eq!(timed_out.outcome(), Some(Outcome::Unconfirmed));

        // The service failed it after the last chunk (for example a failed send of that
        // chunk): the late result changes nothing either.
        let mut failed = waiting_for_result(t0);
        assert_eq!(failed.fail_with(Outcome::Failed), Some(Outcome::Failed));
        assert_eq!(failed.on_result(ResultOutcome::Completed), None);
        assert_eq!(failed.state(), OutgoingState::Aborted);
    }

    #[test]
    fn a_confirmed_send_cannot_be_failed_later() {
        let mut t = waiting_for_result(Instant::now());
        t.on_result(ResultOutcome::Completed);
        assert_eq!(t.fail_with(Outcome::Failed), None);
        assert_eq!(t.connection_lost(), None);
        assert_eq!(t.cancel(), None);
        assert_eq!(t.state(), OutgoingState::Done);
        assert_eq!(t.outcome(), Some(Outcome::Completed));
    }

    #[test]
    fn the_sender_window_closes_after_32_unacked_chunks_and_an_ack_reopens_it() {
        let now = Instant::now();
        let chunk = vec![0u8; CHUNK_BYTES];
        let mut t = outgoing(40 * CB, now);
        assert!(!t.window_open(), "no window before the accept");
        t.on_answer(true, now);
        for n in 0..MAX_UNACKED_CHUNKS {
            assert!(t.window_open(), "chunk {n}");
            t.next_chunk(&chunk).expect("inside the window");
        }
        assert_eq!(t.unacked_chunks(), MAX_UNACKED_CHUNKS);
        assert!(!t.window_open());
        assert!(t.next_chunk(&chunk).is_err(), "the 33rd unacked chunk is refused");
        assert_eq!(t.done_bytes(), 32 * CB, "a refused chunk changes nothing");

        assert_eq!(t.on_ack(ACK_EVERY_CHUNKS), None);
        assert_eq!(t.unacked_chunks(), 16);
        assert!(t.window_open());
        assert_eq!(t.on_ack(ACK_EVERY_CHUNKS), None, "a repeated ack is harmless");
        for _ in 0..8 {
            t.next_chunk(&chunk).expect("reopened");
        }
        assert_eq!(t.state(), OutgoingState::Done);
        assert!(!t.awaiting_result(), "all chunks are built but the final ack is missing");
        // The final ack arrives: only now the sender waits for the result.
        assert_eq!(t.on_ack(40), None);
        assert_eq!(t.unacked_chunks(), 0);
        assert!(t.awaiting_result());
    }

    #[test]
    fn a_bad_ack_fails_the_sender_and_a_late_one_is_ignored() {
        let now = Instant::now();
        let chunk = vec![0u8; CHUNK_BYTES];

        let mut early = outgoing(40 * CB, now);
        assert_eq!(early.on_ack(0), Some(Outcome::Failed), "an ack before the accept");

        let mut too_far = outgoing(40 * CB, now);
        too_far.on_answer(true, now);
        for _ in 0..5 {
            too_far.next_chunk(&chunk).expect("chunk");
        }
        assert_eq!(too_far.on_ack(6), Some(Outcome::Failed), "past the chunks sent");
        assert_eq!(too_far.state(), OutgoingState::Aborted);

        let mut backwards = outgoing(40 * CB, now);
        backwards.on_answer(true, now);
        for _ in 0..20 {
            backwards.next_chunk(&chunk).expect("chunk");
        }
        assert_eq!(backwards.on_ack(16), None);
        assert_eq!(backwards.on_ack(15), Some(Outcome::Failed), "backwards");
        assert_eq!(backwards.on_ack(16), None, "ignored once final");
        assert_eq!(backwards.outcome(), Some(Outcome::Failed));

        let mut confirmed = waiting_for_result(now);
        confirmed.on_result(ResultOutcome::Completed);
        assert_eq!(confirmed.on_ack(5), None, "ignored after the result");
        assert_eq!(confirmed.outcome(), Some(Outcome::Completed));
    }

    #[test]
    fn losing_the_peer_after_the_final_ack_is_unconfirmed() {
        let mut t = waiting_for_result(Instant::now());
        assert_eq!(t.connection_lost(), Some(Outcome::Unconfirmed));
        assert_eq!(t.state(), OutgoingState::Done);
        assert_eq!(t.outcome(), Some(Outcome::Unconfirmed));
        assert!(!t.awaiting_result());
        assert_eq!(t.connection_lost(), None, "only once");
        assert_eq!(t.result_timed_out(), None);
        assert_eq!(t.fail_with(Outcome::Failed), None, "unconfirmed is final");
        assert_eq!(t.on_result(ResultOutcome::Completed), None);
        assert_eq!(t.outcome(), Some(Outcome::Unconfirmed));
    }

    #[test]
    fn losing_the_peer_after_the_last_chunk_but_before_the_final_ack_is_failed() {
        let mut t = last_chunk_sent(Instant::now());
        assert_eq!(t.connection_lost(), Some(Outcome::Failed));
        assert_eq!(t.state(), OutgoingState::Aborted);
        assert_eq!(t.outcome(), Some(Outcome::Failed));
        assert!(!t.awaiting_result());
        assert_eq!(t.connection_lost(), None, "only once");
        assert_eq!(t.on_ack(1), None, "a late final ack changes nothing");
        assert_eq!(t.on_result(ResultOutcome::Completed), None);
        assert_eq!(t.outcome(), Some(Outcome::Failed));
    }

    #[test]
    fn the_result_timeout_after_the_final_ack_is_unconfirmed() {
        let now = Instant::now();
        let mut t = waiting_for_result(now);
        assert_eq!(t.result_timed_out(), Some(Outcome::Unconfirmed));
        assert_eq!(t.outcome(), Some(Outcome::Unconfirmed));
        assert_eq!(t.result_timed_out(), None, "only once");

        // Before the last chunk there is no result to wait for.
        let mut sending = outgoing(CB + 1, now);
        sending.on_answer(true, now);
        sending.next_chunk(&vec![0u8; CHUNK_BYTES]).expect("chunk 0");
        assert_eq!(sending.result_timed_out(), None);
        assert_eq!(sending.state(), OutgoingState::Sending);

        // Between the last chunk and the final ack the result timeout is ignored: it cannot
        // turn a send the receiver never acknowledged into `Unconfirmed`.
        let mut unacked = last_chunk_sent(now);
        assert_eq!(unacked.result_timed_out(), None);
        assert_eq!(unacked.state(), OutgoingState::Done);
        assert_eq!(unacked.outcome(), None, "still undecided");

        // A confirmed send cannot time out.
        let mut confirmed = waiting_for_result(now);
        confirmed.on_result(ResultOutcome::Completed);
        assert_eq!(confirmed.result_timed_out(), None);
        assert_eq!(confirmed.outcome(), Some(Outcome::Completed));
    }

    #[test]
    fn a_result_that_overtakes_the_final_ack_is_accepted() {
        let mut t = last_chunk_sent(Instant::now());
        assert_eq!(t.on_result(ResultOutcome::Completed), Some(Outcome::Completed));
        assert_eq!(t.outcome(), Some(Outcome::Completed));
    }

    #[test]
    fn cancel_and_connection_loss_end_the_sender_transfer() {
        let now = Instant::now();
        let mut t = outgoing(10, now);
        assert_eq!(
            t.cancel(),
            Some(PeerMessage::TransferAbort {
                transfer_id: ID.into(),
                reason: AbortReason::Cancelled,
            })
        );
        assert_eq!(t.outcome(), Some(Outcome::Cancelled));
        assert_eq!(t.cancel(), None, "only once");

        let mut lost = outgoing(10, now);
        assert_eq!(lost.connection_lost(), Some(Outcome::Failed), "lost before the last chunk");
        assert_eq!(lost.connection_lost(), None);
    }

    // ---- receiver ----

    #[test]
    fn the_receiver_starts_offered_and_pending() {
        let t = incoming(2 * CB + 10, Instant::now());
        assert_eq!(t.state(), IncomingState::Offered);
        assert_eq!(t.transfer_id().as_str(), ID);
        assert_eq!(t.name(), "Orders");
        assert_eq!(t.sha256(), SHA);
        assert_eq!(t.total_bytes(), 2 * CB + 10);
        assert_eq!(t.done_bytes(), 0);
        assert!(t.is_pending());
        assert!(t.is_active());
        assert_eq!(t.outcome(), None);
    }

    #[test]
    fn accept_and_decline_produce_the_answer_frames() {
        let now = Instant::now();
        let mut yes = incoming(10, now);
        assert_eq!(
            yes.respond(true, now),
            Some(PeerMessage::OfferAnswer {
                transfer_id: ID.into(),
                accept: true,
            })
        );
        assert_eq!(yes.state(), IncomingState::Accepted);
        assert!(!yes.is_pending());
        assert!(yes.is_active());
        assert_eq!(yes.respond(false, now), None, "answered once");
        assert_eq!(yes.state(), IncomingState::Accepted);

        let mut no = incoming(10, now);
        assert_eq!(
            no.respond(false, now),
            Some(PeerMessage::OfferAnswer {
                transfer_id: ID.into(),
                accept: false,
            })
        );
        assert_eq!(no.state(), IncomingState::Aborted);
        assert_eq!(no.outcome(), Some(Outcome::Declined));
        assert!(!no.is_active());
    }

    #[test]
    fn the_receiver_offer_expires_exactly_at_the_ttl() {
        let t0 = Instant::now();
        let mut t = incoming(10, t0);
        assert!(!t.expire_if_due(t0 + TRANSFER_OFFER_TTL - ms(1)));
        assert_eq!(t.state(), IncomingState::Offered);
        assert!(t.expire_if_due(t0 + TRANSFER_OFFER_TTL));
        assert_eq!(t.state(), IncomingState::Expired);
        assert_eq!(t.outcome(), Some(Outcome::Expired));
        assert!(!t.expire_if_due(t0 + TRANSFER_OFFER_TTL * 2));
    }

    #[test]
    fn answering_at_the_ttl_is_too_late_and_one_millisecond_before_is_not() {
        let t0 = Instant::now();
        let mut late = incoming(10, t0);
        assert_eq!(late.respond(true, t0 + TRANSFER_OFFER_TTL), None);
        assert_eq!(late.state(), IncomingState::Expired);
        assert_eq!(late.outcome(), Some(Outcome::Expired));

        let mut in_time = incoming(10, t0);
        assert!(in_time.respond(true, t0 + TRANSFER_OFFER_TTL - ms(1)).is_some());
        assert_eq!(in_time.state(), IncomingState::Accepted);
        assert!(!in_time.expire_if_due(t0 + TRANSFER_OFFER_TTL * 10), "no timer after accept");
    }

    #[test]
    fn a_chunk_before_the_accept_aborts_as_failed() {
        let now = Instant::now();
        let mut t = incoming(10, now);
        assert_eq!(t.on_chunk(0, &b64(10)), ChunkStep::Failed);
        assert_eq!(t.state(), IncomingState::Aborted);
        assert_eq!(t.outcome(), Some(Outcome::Failed));
        assert_eq!(t.done_bytes(), 0);
    }

    #[test]
    fn chunks_out_of_order_or_repeated_abort_as_failed() {
        let now = Instant::now();
        let mut skip = accepted(2 * CB, now);
        assert_eq!(skip.on_chunk(1, &b64(CHUNK_BYTES)), ChunkStep::Failed);
        assert_eq!(skip.outcome(), Some(Outcome::Failed));

        let mut repeat = accepted(2 * CB, now);
        assert!(matches!(
            repeat.on_chunk(0, &b64(CHUNK_BYTES)),
            ChunkStep::Data { .. }
        ));
        assert_eq!(repeat.on_chunk(0, &b64(CHUNK_BYTES)), ChunkStep::Failed);
        assert_eq!(repeat.state(), IncomingState::Aborted);
    }

    #[test]
    fn a_seq_at_or_past_the_chunk_count_aborts_as_failed() {
        let now = Instant::now();
        for seq in [2, 3, u32::MAX] {
            let mut t = accepted(2 * CB, now);
            assert_eq!(t.on_chunk(seq, &b64(CHUNK_BYTES)), ChunkStep::Failed, "seq {seq}");
        }
        // A surplus chunk after the last one is a frame for a finished transfer.
        let mut t = accepted(CB, now);
        assert!(matches!(t.on_chunk(0, &b64(CHUNK_BYTES)), ChunkStep::Data { last: true, .. }));
        assert_eq!(t.on_chunk(1, &b64(CHUNK_BYTES)), ChunkStep::Ignored);
        assert_eq!(t.outcome(), Some(Outcome::Completed), "the surplus chunk changes nothing");
    }

    #[test]
    fn data_beyond_the_size_or_a_short_chunk_aborts_as_failed() {
        let now = Instant::now();
        // The size is 10 and the chunk carries 11.
        let mut over = accepted(10, now);
        assert_eq!(over.on_chunk(0, &b64(11)), ChunkStep::Failed);

        // The last chunk carries one byte more than the remainder.
        let mut last = accepted(CB + 10, now);
        assert!(matches!(last.on_chunk(0, &b64(CHUNK_BYTES)), ChunkStep::Data { .. }));
        assert_eq!(last.on_chunk(1, &b64(11)), ChunkStep::Failed);

        // The last chunk is one byte short.
        let mut short_last = accepted(CB + 10, now);
        short_last.on_chunk(0, &b64(CHUNK_BYTES));
        assert_eq!(short_last.on_chunk(1, &b64(9)), ChunkStep::Failed);

        // A non-last chunk is short.
        let mut short = accepted(2 * CB, now);
        assert_eq!(short.on_chunk(0, &b64(CHUNK_BYTES - 1)), ChunkStep::Failed);

        // An empty chunk.
        let mut empty = accepted(10, now);
        assert_eq!(empty.on_chunk(0, ""), ChunkStep::Failed);
    }

    #[test]
    fn bad_base64_aborts_as_failed() {
        let now = Instant::now();
        for bad in ["not base64!", "AAE", "AAEC====", "AA EC"] {
            let mut t = accepted(3, now);
            assert_eq!(t.on_chunk(0, bad), ChunkStep::Failed, "input {bad:?}");
        }
        let mut ok = accepted(3, now);
        assert!(matches!(ok.on_chunk(0, "AAEC"), ChunkStep::Data { last: true, .. }));
    }

    #[test]
    fn an_oversize_data_string_aborts_without_decoding() {
        let now = Instant::now();
        let mut t = accepted(CB, now);
        let huge = "A".repeat(MAX_CHUNK_B64_CHARS + 4);
        assert_eq!(t.on_chunk(0, &huge), ChunkStep::Failed);
        assert_eq!(t.outcome(), Some(Outcome::Failed));
        assert_eq!(MAX_CHUNK_B64_CHARS, 43_692);
    }

    #[test]
    fn the_last_chunk_completes_the_receive() {
        let now = Instant::now();
        let mut t = accepted(2 * CB + 10, now);
        let first = t.on_chunk(0, &b64(CHUNK_BYTES));
        assert_eq!(
            first,
            ChunkStep::Data {
                bytes: vec![7u8; CHUNK_BYTES],
                done_bytes: CB,
                last: false,
                ack: None,
            }
        );
        assert_eq!(t.state(), IncomingState::Receiving);
        let second = t.on_chunk(1, &b64(CHUNK_BYTES));
        assert!(matches!(second, ChunkStep::Data { done_bytes, last: false, .. } if done_bytes == 2 * CB));
        let third = t.on_chunk(2, &b64(10));
        assert_eq!(
            third,
            ChunkStep::Data {
                bytes: vec![7u8; 10],
                done_bytes: 2 * CB + 10,
                last: true,
                ack: Some(PeerMessage::ChunkAck {
                    transfer_id: ID.into(),
                    next_seq: 3,
                }),
            }
        );
        assert_eq!(t.state(), IncomingState::Done);
        assert_eq!(t.outcome(), Some(Outcome::Completed));
        assert_eq!(t.done_bytes(), t.total_bytes());
        assert!(!t.is_active());
    }

    #[test]
    fn the_receiver_acks_every_16th_chunk_and_the_last() {
        let now = Instant::now();
        // 34 chunks: acks after chunk 16, chunk 32 and the last one.
        let mut t = accepted(33 * CB + 10, now);
        let mut acks = Vec::new();
        for seq in 0..34u32 {
            let data = if seq == 33 { b64(10) } else { b64(CHUNK_BYTES) };
            match t.on_chunk(seq, &data) {
                ChunkStep::Data { ack: Some(frame), .. } => acks.push((seq, frame)),
                ChunkStep::Data { ack: None, .. } => {}
                other => panic!("seq {seq}: {other:?}"),
            }
        }
        let ack = |next_seq| PeerMessage::ChunkAck {
            transfer_id: ID.into(),
            next_seq,
        };
        assert_eq!(
            acks,
            vec![
                (ACK_EVERY_CHUNKS - 1, ack(ACK_EVERY_CHUNKS)),
                (2 * ACK_EVERY_CHUNKS - 1, ack(2 * ACK_EVERY_CHUNKS)),
                (33, ack(34)),
            ]
        );
        // A single-chunk transfer acks its only (last) chunk.
        let mut one = accepted(10, now);
        assert!(matches!(
            one.on_chunk(0, &b64(10)),
            ChunkStep::Data { ack: Some(PeerMessage::ChunkAck { next_seq: 1, .. }), last: true, .. }
        ));
    }

    #[test]
    fn frames_after_a_final_state_are_ignored() {
        let now = Instant::now();
        let t0 = now;

        let mut declined = incoming(10, now);
        declined.respond(false, now);
        assert_eq!(declined.on_chunk(0, &b64(10)), ChunkStep::Ignored);
        assert_eq!(declined.outcome(), Some(Outcome::Declined));

        let mut expired = incoming(10, t0);
        expired.expire_if_due(t0 + TRANSFER_OFFER_TTL);
        assert_eq!(expired.on_chunk(0, &b64(10)), ChunkStep::Ignored);
        assert_eq!(expired.outcome(), Some(Outcome::Expired));

        let mut aborted = accepted(10, now);
        aborted.on_abort(AbortReason::Cancelled);
        assert_eq!(aborted.on_chunk(0, &b64(10)), ChunkStep::Ignored);
        assert_eq!(aborted.on_abort(AbortReason::Failed), None);
        assert_eq!(aborted.outcome(), Some(Outcome::Cancelled));

        let mut failed = accepted(10, now);
        assert_eq!(failed.on_chunk(5, &b64(10)), ChunkStep::Failed);
        assert_eq!(failed.on_chunk(0, &b64(10)), ChunkStep::Ignored, "a failed transfer stays failed");
    }

    #[test]
    fn a_peer_abort_maps_to_an_outcome() {
        let now = Instant::now();
        let mut offered = incoming(10, now);
        assert_eq!(offered.on_abort(AbortReason::Cancelled), Some(Outcome::Cancelled));
        let mut receiving = accepted(CB + 1, now);
        receiving.on_chunk(0, &b64(CHUNK_BYTES));
        assert_eq!(receiving.on_abort(AbortReason::Failed), Some(Outcome::Failed));
        let mut done = accepted(10, now);
        done.on_chunk(0, &b64(10));
        assert_eq!(done.on_abort(AbortReason::Failed), None, "all bytes already arrived");
    }

    #[test]
    fn the_service_can_fail_a_finished_receive() {
        let now = Instant::now();
        let mut t = accepted(10, now);
        t.on_chunk(0, &b64(10));
        assert_eq!(t.fail_with(Outcome::Completed), None, "not a failure");
        assert_eq!(t.fail_with(Outcome::Damaged), Some(Outcome::Damaged));
        assert_eq!(t.state(), IncomingState::Aborted);
        assert_eq!(t.outcome(), Some(Outcome::Damaged));
        assert_eq!(t.fail_with(Outcome::Rejected), None, "already final");

        let mut expired = incoming(10, now);
        expired.expire_if_due(now + TRANSFER_OFFER_TTL);
        assert_eq!(expired.fail_with(Outcome::Failed), None);
        assert_eq!(expired.outcome(), Some(Outcome::Expired));
    }

    #[test]
    fn a_short_receiver_offer_ttl_expires_the_offer_at_exactly_that_ttl() {
        let t0 = Instant::now();
        let one = Duration::from_secs(1);
        let mut t = incoming(10, t0).with_offer_ttl(one);
        assert!(!t.expire_if_due(t0 + one - ms(1)));
        assert_eq!(t.state(), IncomingState::Offered);
        assert!(t.expire_if_due(t0 + one));
        assert_eq!(t.state(), IncomingState::Expired);

        let mut late = incoming(10, t0).with_offer_ttl(one);
        assert_eq!(late.respond(true, t0 + one), None);
        assert_eq!(late.outcome(), Some(Outcome::Expired));
    }

    #[test]
    fn the_default_receiver_offer_ttl_is_the_constant() {
        let t0 = Instant::now();
        let mut t = incoming(10, t0);
        assert!(!t.expire_if_due(t0 + Duration::from_secs(1)), "not the short test ttl");
        assert!(!t.expire_if_due(t0 + TRANSFER_OFFER_TTL - ms(1)));
        assert!(t.expire_if_due(t0 + TRANSFER_OFFER_TTL));
    }

    #[test]
    fn connection_loss_fails_an_active_receive_only() {
        let now = Instant::now();
        let mut receiving = accepted(CB + 1, now);
        receiving.on_chunk(0, &b64(CHUNK_BYTES));
        assert_eq!(receiving.connection_lost(), Some(Outcome::Failed));
        assert_eq!(receiving.connection_lost(), None);

        let mut done = accepted(10, now);
        done.on_chunk(0, &b64(10));
        assert_eq!(done.connection_lost(), None, "the import does not depend on the socket");
        assert_eq!(done.outcome(), Some(Outcome::Completed));
    }

    #[test]
    fn the_abort_frame_names_the_transfer() {
        let t = incoming(10, Instant::now());
        assert_eq!(
            t.abort_message(AbortReason::Busy),
            PeerMessage::TransferAbort {
                transfer_id: ID.into(),
                reason: AbortReason::Busy,
            }
        );
    }

    // ---- limits ----

    #[test]
    fn a_duplicate_offer_id_is_ignored_and_costs_no_rate_hit() {
        let t0 = Instant::now();
        let mut limits = TransferLimits::new();
        let first = TransferId::random();
        assert_eq!(limits.admit_offer(&peer(1), &first, t0), Admission::Admit);
        for _ in 0..20 {
            assert_eq!(limits.admit_offer(&peer(1), &first, t0), Admission::Duplicate);
        }
        // The same id from another peer is still a duplicate.
        assert_eq!(limits.admit_offer(&peer(2), &first, t0), Admission::Duplicate);
        // The duplicates did not use the rate window: the id is released and 4 more fit.
        assert!(limits.release(&first));
        for _ in 0..(OFFERS_PER_PEER_PER_MINUTE - 1) {
            let next = TransferId::random();
            assert_eq!(limits.admit_offer(&peer(1), &next, t0), Admission::Admit);
            assert!(limits.release(&next));
        }
    }

    #[test]
    fn a_peer_gets_one_incoming_transfer_at_a_time() {
        let t0 = Instant::now();
        let mut limits = TransferLimits::new();
        let (a1, a2, b1) = (TransferId::random(), TransferId::random(), TransferId::random());
        assert_eq!(MAX_ACTIVE_PER_PEER, 1);
        assert_eq!(limits.admit_offer(&peer(1), &a1, t0), Admission::Admit);
        assert_eq!(
            limits.admit_offer(&peer(1), &a2, t0),
            Admission::Refuse(AbortReason::Busy)
        );
        assert_eq!(limits.admit_offer(&peer(2), &b1, t0), Admission::Admit);
        // An accepted transfer still occupies the peer.
        assert!(limits.mark_active(&a1));
        assert_eq!(
            limits.admit_offer(&peer(1), &a2, t0),
            Admission::Refuse(AbortReason::Busy)
        );
        // The refused id was not remembered.
        assert!(limits.release(&a1));
        assert_eq!(limits.admit_offer(&peer(1), &a2, t0), Admission::Admit);
    }

    #[test]
    fn the_fourth_pending_offer_across_peers_is_refused() {
        let t0 = Instant::now();
        let mut limits = TransferLimits::new();
        assert_eq!(MAX_PENDING_INCOMING, 3);
        let ids: Vec<TransferId> = (0..4).map(|_| TransferId::random()).collect();
        for (n, id) in ids.iter().take(3).enumerate() {
            assert_eq!(limits.admit_offer(&peer(n as u8 + 1), id, t0), Admission::Admit);
        }
        assert_eq!(limits.pending_incoming(), 3);
        assert_eq!(
            limits.admit_offer(&peer(4), &ids[3], t0),
            Admission::Refuse(AbortReason::Busy)
        );
        // Accepting one frees a pending slot, because it is no longer waiting for the user.
        assert!(limits.mark_active(&ids[0]));
        assert_eq!(limits.pending_incoming(), 2);
        assert_eq!(limits.admit_offer(&peer(4), &ids[3], t0), Admission::Admit);
    }

    #[test]
    fn offers_per_peer_per_minute_are_limited_and_the_window_slides() {
        let t0 = Instant::now();
        let mut limits = TransferLimits::new();
        for _ in 0..OFFERS_PER_PEER_PER_MINUTE {
            let id = TransferId::random();
            assert_eq!(limits.admit_offer(&peer(1), &id, t0), Admission::Admit);
            assert!(limits.release(&id));
        }
        let sixth = TransferId::random();
        assert_eq!(
            limits.admit_offer(&peer(1), &sixth, t0 + Duration::from_secs(59)),
            Admission::Refuse(AbortReason::Busy)
        );
        // Another peer has its own window.
        assert_eq!(
            limits.admit_offer(&peer(2), &TransferId::random(), t0 + Duration::from_secs(59)),
            Admission::Admit
        );
        assert_eq!(
            limits.admit_offer(&peer(1), &sixth, t0 + Duration::from_secs(60)),
            Admission::Admit
        );
    }

    #[test]
    fn outgoing_transfers_are_limited_per_peer_and_do_not_block_incoming() {
        let t0 = Instant::now();
        let mut limits = TransferLimits::new();
        let (out1, out2, inc) = (TransferId::random(), TransferId::random(), TransferId::random());
        assert!(limits.begin_outgoing(&peer(1), &out1));
        assert!(!limits.begin_outgoing(&peer(1), &out2), "one send per peer");
        assert!(!limits.begin_outgoing(&peer(1), &out1), "a known id");
        assert!(limits.begin_outgoing(&peer(2), &out2));
        assert_eq!(limits.admit_offer(&peer(1), &inc, t0), Admission::Admit);
        assert_eq!(limits.pending_incoming(), 1, "an outgoing transfer is never pending");
        assert!(limits.release(&out1));
        assert!(limits.begin_outgoing(&peer(1), &TransferId::random()));
    }

    #[test]
    fn unknown_ids_are_not_found_in_the_limits() {
        let mut limits = TransferLimits::new();
        let unknown = TransferId::random();
        assert!(!limits.release(&unknown));
        assert!(!limits.mark_active(&unknown));
        assert_eq!(limits.pending_incoming(), 0);
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -j4 -p rocket-network transfer`
Expected: FAIL to compile, `cannot find type TransferId in this scope` and similar (also no method `on_result`, `on_ack`, `window_open`, `result_timed_out`, no function `offer_name_is_valid`, no variant `Outcome::Unconfirmed` and no field `ack` on `ChunkStep::Data`).

- [ ] **Step 3: Write the implementation**

Put this above the `#[cfg(test)]` module in `crates/rocket-network/src/transfer.rs`:

```rust
//! Transfer ids, the sender and receiver state machines and the offer limits.
//!
//! Everything here is pure. The caller passes the time as an `Instant`, stages the bytes and
//! sends the frames. A failed check never returns an error to the socket loop: the machine
//! moves to a final state and the caller turns that into a `TransferAbort`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rand::RngCore;
use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

use crate::device::DeviceId;
use crate::limits::{
    ACK_EVERY_CHUNKS, CHUNK_BYTES, MAX_ACTIVE_PER_PEER, MAX_PENDING_INCOMING, MAX_TRANSFER_BYTES,
    MAX_UNACKED_CHUNKS, OFFERS_PER_PEER_PER_MINUTE, TRANSFER_OFFER_TTL,
};
use crate::messages::{AbortReason, PeerMessage, ResultOutcome};
use crate::rate::SlidingWindow;

/// The longest collection name in an offer, in characters.
const NAME_MAX_CHARS: usize = 200;
/// The most base64 characters one chunk may carry: `CHUNK_BYTES` raw, padded.
const MAX_CHUNK_B64_CHARS: usize = CHUNK_BYTES.div_ceil(3) * 4;

/// Ties the frames of one transfer together. 32 lowercase hex characters, not secret.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TransferId(String);

impl TransferId {
    /// A new random 128-bit id.
    pub fn random() -> TransferId {
        let mut bytes = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut bytes);
        let mut text = String::with_capacity(32);
        for byte in bytes {
            text.push_str(&format!("{byte:02x}"));
        }
        TransferId(text)
    }

    /// Accepts exactly 32 lowercase hex characters.
    pub fn parse(text: &str) -> DomainResult<TransferId> {
        let valid = text.len() == 32
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if valid {
            Ok(TransferId(text.to_string()))
        } else {
            Err(DomainError::InvalidInput("invalid transfer id".to_string()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// How many chunks carry `size_bytes`. Zero bytes need no chunk.
pub fn chunk_count_for(size_bytes: u64) -> u32 {
    u32::try_from(size_bytes.div_ceil(CHUNK_BYTES as u64)).unwrap_or(u32::MAX)
}

/// Which way a transfer goes, seen from this node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Send,
    Receive,
}

/// How a transfer ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Completed,
    /// The sender sent every chunk and the receiver acknowledged the final one, but the
    /// receiver's result never came: the peer went away after the final `ChunkAck`, or
    /// `SENDER_RESULT_TIMEOUT` passed. Not a failure. A peer lost before the final ack is `Failed`.
    Unconfirmed,
    Declined,
    Expired,
    Cancelled,
    Failed,
    TooBig,
    Busy,
    Damaged,
    Rejected,
}

impl From<AbortReason> for Outcome {
    fn from(reason: AbortReason) -> Outcome {
        match reason {
            AbortReason::Cancelled => Outcome::Cancelled,
            AbortReason::Failed => Outcome::Failed,
            AbortReason::TooBig => Outcome::TooBig,
            AbortReason::Busy => Outcome::Busy,
            AbortReason::Expired => Outcome::Expired,
        }
    }
}

impl From<ResultOutcome> for Outcome {
    fn from(result: ResultOutcome) -> Outcome {
        match result {
            ResultOutcome::Completed => Outcome::Completed,
            ResultOutcome::Damaged => Outcome::Damaged,
            ResultOutcome::Rejected => Outcome::Rejected,
            ResultOutcome::Failed => Outcome::Failed,
        }
    }
}

/// Receives the transfer frames that `NetworkService` does not handle itself.
pub trait TransferSink: Send + Sync {
    fn on_frame(&self, from: DeviceId, message: PeerMessage);

    /// Called when the peer socket ends or the pairing is removed. Both roles clean up at once.
    fn on_peer_gone(&self, device: DeviceId);
}

/// Why an offer is refused before any state exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferError {
    BadId,
    BadName,
    BadHash,
    Empty,
    TooBig,
    ChunkCountMismatch,
}

impl OfferError {
    /// The reason to put in the `TransferAbort` that answers a refused offer.
    pub fn abort_reason(self) -> AbortReason {
        match self {
            OfferError::TooBig => AbortReason::TooBig,
            _ => AbortReason::Failed,
        }
    }
}

/// Bidi controls, zero-width characters and other invisible format characters that can make
/// a name read differently from what it is.
fn is_spoofing_format_char(c: char) -> bool {
    matches!(
        c,
        '\u{ad}'
            | '\u{61c}'
            | '\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{feff}'
    )
}

/// The offer name rule of the spec (section 6). The sender checks it before a send, the
/// receiver on every offer.
pub fn offer_name_is_valid(name: &str) -> bool {
    !name.trim().is_empty()
        && name.chars().count() <= NAME_MAX_CHARS
        && !name
            .chars()
            .any(|c| c.is_control() || is_spoofing_format_char(c))
}

fn validate_offer(
    name: &str,
    size_bytes: u64,
    chunk_count: u32,
    sha256: &str,
) -> Result<(), OfferError> {
    if !offer_name_is_valid(name) {
        return Err(OfferError::BadName);
    }
    if size_bytes == 0 {
        return Err(OfferError::Empty);
    }
    if size_bytes > MAX_TRANSFER_BYTES {
        return Err(OfferError::TooBig);
    }
    if chunk_count != chunk_count_for(size_bytes) {
        return Err(OfferError::ChunkCountMismatch);
    }
    let hex = sha256.len() == 64
        && sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if !hex {
        return Err(OfferError::BadHash);
    }
    Ok(())
}

/// The raw length chunk number `next_seq` must have, given the bytes moved so far.
fn expected_len(size_bytes: u64, moved_bytes: u64) -> u64 {
    size_bytes.saturating_sub(moved_bytes).min(CHUNK_BYTES as u64)
}

fn offer_expired(created: Instant, now: Instant, ttl: Duration) -> bool {
    now.saturating_duration_since(created) >= ttl
}

fn abort_frame(id: &TransferId, reason: AbortReason) -> PeerMessage {
    PeerMessage::TransferAbort {
        transfer_id: id.as_str().to_string(),
        reason,
    }
}

fn ack_frame(id: &TransferId, next_seq: u32) -> PeerMessage {
    PeerMessage::ChunkAck {
        transfer_id: id.as_str().to_string(),
        next_seq,
    }
}

// ---- sender ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutgoingState {
    Offered,
    Accepted,
    Sending,
    Done,
    Aborted,
    Expired,
}

/// The sender side of one transfer.
#[derive(Debug, Clone)]
pub struct OutgoingTransfer {
    id: TransferId,
    name: String,
    size_bytes: u64,
    chunk_count: u32,
    sha256: String,
    created: Instant,
    offer_ttl: Duration,
    state: OutgoingState,
    next_seq: u32,
    acked: u32,
    sent_bytes: u64,
    /// The receiver's final `ChunkAck` (`next_seq == chunk_count`) arrived.
    final_acked: bool,
    /// The result question is settled: a result arrived, or the wait ended `Unconfirmed`.
    result_received: bool,
    outcome: Option<Outcome>,
}

impl OutgoingTransfer {
    /// Starts in `Offered`. The same checks as the receiver apply, so a bad offer never leaves.
    pub fn new(
        id: TransferId,
        name: &str,
        size_bytes: u64,
        sha256: &str,
        now: Instant,
    ) -> Result<OutgoingTransfer, OfferError> {
        let chunk_count = chunk_count_for(size_bytes);
        validate_offer(name, size_bytes, chunk_count, sha256)?;
        Ok(OutgoingTransfer {
            id,
            name: name.to_string(),
            size_bytes,
            chunk_count,
            sha256: sha256.to_string(),
            created: now,
            offer_ttl: TRANSFER_OFFER_TTL,
            state: OutgoingState::Offered,
            next_seq: 0,
            acked: 0,
            sent_bytes: 0,
            final_acked: false,
            result_received: false,
            outcome: None,
        })
    }

    /// Test seam: shortens the offer lifetime that otherwise is `TRANSFER_OFFER_TTL`.
    #[doc(hidden)]
    pub fn with_offer_ttl(mut self, ttl: Duration) -> Self {
        self.offer_ttl = ttl;
        self
    }

    pub fn transfer_id(&self) -> &TransferId {
        &self.id
    }

    pub fn state(&self) -> OutgoingState {
        self.state
    }

    /// `None` while the transfer is active.
    pub fn outcome(&self) -> Option<Outcome> {
        self.outcome
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            OutgoingState::Offered | OutgoingState::Accepted | OutgoingState::Sending
        )
    }

    pub fn done_bytes(&self) -> u64 {
        self.sent_bytes
    }

    pub fn total_bytes(&self) -> u64 {
        self.size_bytes
    }

    /// Chunks sent and not yet acknowledged.
    pub fn unacked_chunks(&self) -> u32 {
        self.next_seq - self.acked
    }

    /// True when the next chunk may be sent now: accepted, chunks left and room in the window.
    pub fn window_open(&self) -> bool {
        matches!(self.state, OutgoingState::Accepted | OutgoingState::Sending)
            && self.unacked_chunks() < MAX_UNACKED_CHUNKS
    }

    /// The receiver's `ChunkAck`. Returns `Some(Failed)` when the ack breaks a rule (before the
    /// accept, backwards, or past the chunks sent), `None` otherwise. Ignored once final.
    pub fn on_ack(&mut self, next_seq: u32) -> Option<Outcome> {
        let open = matches!(self.state, OutgoingState::Accepted | OutgoingState::Sending)
            || self.last_chunk_built();
        if self.state == OutgoingState::Offered {
            return self.finish(Outcome::Failed);
        }
        if !open {
            return None;
        }
        if next_seq < self.acked || next_seq > self.next_seq {
            return self.finish(Outcome::Failed);
        }
        self.acked = next_seq;
        if self.acked == self.chunk_count {
            self.final_acked = true;
        }
        None
    }

    pub fn offer_message(&self) -> PeerMessage {
        PeerMessage::CollectionOffer {
            transfer_id: self.id.as_str().to_string(),
            name: self.name.clone(),
            size_bytes: self.size_bytes,
            chunk_count: self.chunk_count,
            sha256: self.sha256.clone(),
        }
    }

    pub fn abort_message(&self, reason: AbortReason) -> PeerMessage {
        abort_frame(&self.id, reason)
    }

    /// Expires an unanswered offer at exactly the offer ttl. True when it changed.
    pub fn expire_if_due(&mut self, now: Instant) -> bool {
        if self.state == OutgoingState::Offered
            && offer_expired(self.created, now, self.offer_ttl)
        {
            self.finish(Outcome::Expired);
            return true;
        }
        false
    }

    /// Applies the receiver's answer. Returns the outcome when this call ended the transfer.
    /// An answer that arrives at or after the TTL finds the offer expired.
    pub fn on_answer(&mut self, accept: bool, now: Instant) -> Option<Outcome> {
        if self.expire_if_due(now) {
            return Some(Outcome::Expired);
        }
        if self.state != OutgoingState::Offered {
            return None;
        }
        if accept {
            self.state = OutgoingState::Accepted;
            None
        } else {
            self.finish(Outcome::Declined)
        }
    }

    /// Builds the next chunk frame from the next slice of the ZIP. The slice must be
    /// `CHUNK_BYTES` long, or the remainder for the last chunk.
    pub fn next_chunk(&mut self, raw: &[u8]) -> DomainResult<PeerMessage> {
        if !matches!(self.state, OutgoingState::Accepted | OutgoingState::Sending) {
            return Err(DomainError::InvalidInput(
                "the transfer is not accepted".to_string(),
            ));
        }
        if self.unacked_chunks() >= MAX_UNACKED_CHUNKS {
            return Err(DomainError::InvalidInput(
                "the ack window is full".to_string(),
            ));
        }
        let expected = expected_len(self.size_bytes, self.sent_bytes);
        if raw.len() as u64 != expected {
            return Err(DomainError::InvalidInput(
                "unexpected chunk length".to_string(),
            ));
        }
        let message = PeerMessage::Chunk {
            transfer_id: self.id.as_str().to_string(),
            seq: self.next_seq,
            data: STANDARD.encode(raw),
        };
        self.next_seq += 1;
        self.sent_bytes += expected;
        if self.next_seq == self.chunk_count {
            self.finish(Outcome::Completed);
        } else {
            self.state = OutgoingState::Sending;
        }
        Ok(message)
    }

    /// The peer stopped the transfer. Ignored once the transfer is final.
    pub fn on_abort(&mut self, reason: AbortReason) -> Option<Outcome> {
        if !self.is_active() {
            return None;
        }
        self.finish(Outcome::from(reason))
    }

    /// The user stopped the transfer. Returns the frame to send, once.
    pub fn cancel(&mut self) -> Option<PeerMessage> {
        if !self.is_active() {
            return None;
        }
        self.finish(Outcome::Cancelled);
        Some(abort_frame(&self.id, AbortReason::Cancelled))
    }

    /// The connection dropped. Only after the receiver's final `ChunkAck` arrived (the result
    /// is awaited) is it `Unconfirmed`: the receiver has every byte and the import may still
    /// succeed. Before the final ack, including after the last chunk was built or sent, it is
    /// `Failed` (the receiver may never have got the last chunk).
    pub fn connection_lost(&mut self) -> Option<Outcome> {
        if self.awaiting_result() {
            return self.finish(Outcome::Unconfirmed);
        }
        if !self.is_active() && !self.last_chunk_built() {
            return None;
        }
        self.finish(Outcome::Failed)
    }

    /// `SENDER_RESULT_TIMEOUT` passed. `Unconfirmed` while the result is awaited, else `None`
    /// (before the final ack there is no result timer; the idle limit applies instead).
    pub fn result_timed_out(&mut self) -> Option<Outcome> {
        if !self.awaiting_result() {
            return None;
        }
        self.finish(Outcome::Unconfirmed)
    }

    /// True after the last chunk frame was built and the receiver's final `ChunkAck` arrived,
    /// and before the receiver's result arrived. False between the last chunk and the final ack.
    pub fn awaiting_result(&self) -> bool {
        self.last_chunk_built() && self.final_acked
    }

    /// The last chunk frame was built and no result settled the transfer yet. The final
    /// `ChunkAck` may still be missing. Plan 3 uses it to know the stream is finished and to
    /// refuse a cancel; `awaiting_result()` is the stricter question.
    pub fn last_chunk_built(&self) -> bool {
        self.state == OutgoingState::Done && !self.result_received
    }

    /// The receiver reported the true outcome. Accepted once, after the last chunk was built
    /// (a result implies the final ack, so one that overtakes it counts as acked). `Completed` keeps the machine `Done`, the other results end it
    /// `Aborted` with the same-named outcome. Anything else returns `None` and changes nothing.
    pub fn on_result(&mut self, result: ResultOutcome) -> Option<Outcome> {
        if !self.last_chunk_built() {
            return None;
        }
        self.final_acked = true;
        self.result_received = true;
        match Outcome::from(result) {
            Outcome::Completed => Some(Outcome::Completed),
            failure => self.finish(failure),
        }
    }

    /// The service ends the transfer itself. Allowed from any state except an already final
    /// failure (`Aborted`, `Expired`), so it also works after `Done`: the machine is `Done` the
    /// moment the last chunk frame is built, and the result timeout fires after that. A send
    /// the receiver already confirmed stays `Completed`. `Completed` is not a failure.
    pub fn fail_with(&mut self, outcome: Outcome) -> Option<Outcome> {
        if outcome == Outcome::Completed {
            return None;
        }
        if matches!(self.state, OutgoingState::Aborted | OutgoingState::Expired)
            || (self.state == OutgoingState::Done && self.result_received)
        {
            return None;
        }
        self.finish(outcome)
    }

    fn finish(&mut self, outcome: Outcome) -> Option<Outcome> {
        self.state = match outcome {
            Outcome::Completed => OutgoingState::Done,
            // Every chunk left, so the machine stays `Done`, but the wait is over.
            Outcome::Unconfirmed => {
                self.result_received = true;
                OutgoingState::Done
            }
            Outcome::Expired => OutgoingState::Expired,
            _ => OutgoingState::Aborted,
        };
        self.outcome = Some(outcome);
        Some(outcome)
    }
}

// ---- receiver ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncomingState {
    Offered,
    Accepted,
    Receiving,
    Done,
    Aborted,
    Expired,
}

/// What `IncomingTransfer::on_chunk` asks the caller to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkStep {
    /// A frame for a finished transfer. Do nothing.
    Ignored,
    /// A valid chunk. Append `bytes` to the staged file. `last` is true for the final chunk.
    /// `ack` is the `ChunkAck` frame to send after staging: `Some` on every
    /// `ACK_EVERY_CHUNKS`-th accepted chunk and on the last one.
    Data {
        bytes: Vec<u8>,
        done_bytes: u64,
        last: bool,
        ack: Option<PeerMessage>,
    },
    /// The chunk broke a rule. The transfer is final: send `TransferAbort { failed }`, discard
    /// the staged file and report `Outcome::Failed`.
    Failed,
}

/// The receiver side of one transfer.
#[derive(Debug, Clone)]
pub struct IncomingTransfer {
    id: TransferId,
    name: String,
    size_bytes: u64,
    chunk_count: u32,
    sha256: String,
    created: Instant,
    offer_ttl: Duration,
    state: IncomingState,
    next_seq: u32,
    received_bytes: u64,
    outcome: Option<Outcome>,
}

impl IncomingTransfer {
    /// Checks an offer frame and starts in `Offered`. An error carries the reason to abort with.
    pub fn from_offer(
        transfer_id: &str,
        name: &str,
        size_bytes: u64,
        chunk_count: u32,
        sha256: &str,
        now: Instant,
    ) -> Result<IncomingTransfer, OfferError> {
        let id = TransferId::parse(transfer_id).map_err(|_| OfferError::BadId)?;
        validate_offer(name, size_bytes, chunk_count, sha256)?;
        Ok(IncomingTransfer {
            id,
            name: name.to_string(),
            size_bytes,
            chunk_count,
            sha256: sha256.to_string(),
            created: now,
            offer_ttl: TRANSFER_OFFER_TTL,
            state: IncomingState::Offered,
            next_seq: 0,
            received_bytes: 0,
            outcome: None,
        })
    }

    /// Test seam: shortens the offer lifetime that otherwise is `TRANSFER_OFFER_TTL`.
    #[doc(hidden)]
    pub fn with_offer_ttl(mut self, ttl: Duration) -> Self {
        self.offer_ttl = ttl;
        self
    }

    pub fn transfer_id(&self) -> &TransferId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub fn state(&self) -> IncomingState {
        self.state
    }

    /// `None` while the transfer is active.
    pub fn outcome(&self) -> Option<Outcome> {
        self.outcome
    }

    /// Waiting for the user's answer.
    pub fn is_pending(&self) -> bool {
        self.state == IncomingState::Offered
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            IncomingState::Offered | IncomingState::Accepted | IncomingState::Receiving
        )
    }

    pub fn done_bytes(&self) -> u64 {
        self.received_bytes
    }

    pub fn total_bytes(&self) -> u64 {
        self.size_bytes
    }

    pub fn abort_message(&self, reason: AbortReason) -> PeerMessage {
        abort_frame(&self.id, reason)
    }

    /// Expires an unanswered offer at exactly the offer ttl. True when it changed.
    pub fn expire_if_due(&mut self, now: Instant) -> bool {
        if self.state == IncomingState::Offered
            && offer_expired(self.created, now, self.offer_ttl)
        {
            self.finish(Outcome::Expired);
            return true;
        }
        false
    }

    /// The user's answer. Returns the frame to send, or `None` when the answer is not valid
    /// any more (already answered, final or expired). After a `None`, read `outcome()` to see
    /// whether the offer expired.
    pub fn respond(&mut self, accept: bool, now: Instant) -> Option<PeerMessage> {
        self.expire_if_due(now);
        if self.state != IncomingState::Offered {
            return None;
        }
        if accept {
            self.state = IncomingState::Accepted;
        } else {
            self.finish(Outcome::Declined);
        }
        Some(PeerMessage::OfferAnswer {
            transfer_id: self.id.as_str().to_string(),
            accept,
        })
    }

    /// Checks and decodes one chunk frame.
    pub fn on_chunk(&mut self, seq: u32, data: &str) -> ChunkStep {
        match self.state {
            IncomingState::Accepted | IncomingState::Receiving => {}
            // A chunk before the accept is a protocol error.
            IncomingState::Offered => {
                self.finish(Outcome::Failed);
                return ChunkStep::Failed;
            }
            IncomingState::Done | IncomingState::Aborted | IncomingState::Expired => {
                return ChunkStep::Ignored;
            }
        }
        let Some(bytes) = self.check_chunk(seq, data) else {
            self.finish(Outcome::Failed);
            return ChunkStep::Failed;
        };
        self.received_bytes += bytes.len() as u64;
        self.next_seq += 1;
        let last = self.next_seq == self.chunk_count;
        if last {
            self.finish(Outcome::Completed);
        } else {
            self.state = IncomingState::Receiving;
        }
        let ack = (last || self.next_seq % ACK_EVERY_CHUNKS == 0)
            .then(|| ack_frame(&self.id, self.next_seq));
        ChunkStep::Data {
            bytes,
            done_bytes: self.received_bytes,
            last,
            ack,
        }
    }

    /// The decoded bytes when the chunk is the one expected, `None` for any rule break.
    fn check_chunk(&self, seq: u32, data: &str) -> Option<Vec<u8>> {
        if seq != self.next_seq || seq >= self.chunk_count {
            return None;
        }
        if data.len() > MAX_CHUNK_B64_CHARS {
            return None;
        }
        let bytes = STANDARD.decode(data).ok()?;
        let expected = expected_len(self.size_bytes, self.received_bytes);
        (bytes.len() as u64 == expected).then_some(bytes)
    }

    /// The peer stopped the transfer. Ignored once all bytes arrived or the transfer is final.
    pub fn on_abort(&mut self, reason: AbortReason) -> Option<Outcome> {
        if !self.is_active() {
            return None;
        }
        self.finish(Outcome::from(reason))
    }

    /// The connection dropped. Ignored once all bytes arrived.
    pub fn connection_lost(&mut self) -> Option<Outcome> {
        if !self.is_active() {
            return None;
        }
        self.finish(Outcome::Failed)
    }

    /// The service ended the transfer after it checked the staged file or tried the import.
    /// Allowed from any state except an already final failure. `Completed` is not a failure.
    pub fn fail_with(&mut self, outcome: Outcome) -> Option<Outcome> {
        if outcome == Outcome::Completed {
            return None;
        }
        if matches!(self.state, IncomingState::Aborted | IncomingState::Expired) {
            return None;
        }
        self.finish(outcome)
    }

    fn finish(&mut self, outcome: Outcome) -> Option<Outcome> {
        self.state = match outcome {
            Outcome::Completed => IncomingState::Done,
            Outcome::Expired => IncomingState::Expired,
            _ => IncomingState::Aborted,
        };
        self.outcome = Some(outcome);
        Some(outcome)
    }
}

// ---- limits ----

/// What `TransferLimits::admit_offer` decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// Track the offer and show it to the user.
    Admit,
    /// The id is already known. Ignore the frame and leave the first transfer alone.
    Duplicate,
    /// Answer with `TransferAbort` and this reason.
    Refuse(AbortReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Pending,
    Active,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    peer: DeviceId,
    direction: Direction,
    slot: Slot,
}

/// Tracks the transfers a node has open and applies the offer limits. Pure.
#[derive(Debug, Default)]
pub struct TransferLimits {
    entries: HashMap<TransferId, Entry>,
    windows: HashMap<DeviceId, SlidingWindow>,
}

impl TransferLimits {
    pub fn new() -> TransferLimits {
        TransferLimits::default()
    }

    /// Decides on an incoming offer. A duplicate id costs nothing. Every other offer counts
    /// against the peer's rate window, even a refused one, so a flood stays limited.
    pub fn admit_offer(&mut self, peer: &DeviceId, id: &TransferId, now: Instant) -> Admission {
        if self.entries.contains_key(id) {
            return Admission::Duplicate;
        }
        let window = self.windows.entry(*peer).or_insert_with(|| {
            SlidingWindow::new(OFFERS_PER_PEER_PER_MINUTE as usize, Duration::from_secs(60))
        });
        if !window.try_hit(now) {
            return Admission::Refuse(AbortReason::Busy);
        }
        if self.pending_incoming() >= MAX_PENDING_INCOMING
            || self.open_for(peer, Direction::Receive) >= MAX_ACTIVE_PER_PEER
        {
            return Admission::Refuse(AbortReason::Busy);
        }
        self.entries.insert(
            id.clone(),
            Entry {
                peer: *peer,
                direction: Direction::Receive,
                slot: Slot::Pending,
            },
        );
        Admission::Admit
    }

    /// Claims the peer's one outgoing slot. False when the peer already has one or the id is known.
    pub fn begin_outgoing(&mut self, peer: &DeviceId, id: &TransferId) -> bool {
        if self.entries.contains_key(id)
            || self.open_for(peer, Direction::Send) >= MAX_ACTIVE_PER_PEER
        {
            return false;
        }
        self.entries.insert(
            id.clone(),
            Entry {
                peer: *peer,
                direction: Direction::Send,
                slot: Slot::Active,
            },
        );
        true
    }

    /// The user accepted: the offer no longer waits, but it still holds the peer's slot.
    pub fn mark_active(&mut self, id: &TransferId) -> bool {
        match self.entries.get_mut(id) {
            Some(entry) => {
                entry.slot = Slot::Active;
                true
            }
            None => false,
        }
    }

    /// Forgets a finished transfer. False for an unknown id.
    pub fn release(&mut self, id: &TransferId) -> bool {
        self.entries.remove(id).is_some()
    }

    /// Incoming offers that wait for the user, across all peers.
    pub fn pending_incoming(&self) -> usize {
        self.entries
            .values()
            .filter(|e| e.direction == Direction::Receive && e.slot == Slot::Pending)
            .count()
    }

    fn open_for(&self, peer: &DeviceId, direction: Direction) -> usize {
        self.entries
            .values()
            .filter(|e| e.peer == *peer && e.direction == direction)
            .count()
    }
}
```

`SlidingWindow` derives `Debug` and `Clone` already, so `TransferLimits` can derive `Debug`. `DeviceId` has a manual `Debug` impl and is `Copy`.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -j4 -p rocket-network transfer`
Expected: PASS, every test of Step 1 in `transfer::tests`, 0 failed (among them `a_sink_is_object_safe_and_gets_frames_and_peer_gone`, `result_outcomes_map_to_outcomes`, the five sender result tests, `offer_name_is_valid_follows_the_spec_rule`, the two ack window tests, the final-ack `Unconfirmed` and `Failed` tests and `the_receiver_acks_every_16th_chunk_and_the_last`). If `the_sender_window_closes_after_32_unacked_chunks_and_an_ack_reopens_it` fails on `window_open` before the accept, `window_open` must check the state, not only the count. If a `base64` negative case fails (for example `"AAE"` decodes), the engine is too lenient: keep `STANDARD` (padding required) and fix the test input, not the engine.

- [ ] **Step 5: Check the crate and the lints**

Run: `cargo check -j4 -p rocket-network`
Expected: Finished, no warnings (an unused import means a name in the `use` block is not needed: remove it).

Run: `cargo clippy -j4 -p rocket-network --all-targets`
Expected: no warnings in `transfer.rs`.

---

## Task 3: `TransferEvent` and the two audit kinds

**Files:**
- Modify: `crates/rocket-network/src/events.rs`
- Modify: `crates/rocket-network/src/lib.rs`
- Modify: `crates/rocket-network/CLAUDE.md`
- Modify: `crates/rocket-audit/src/event.rs`
- Modify: `crates/rocket-audit/src/control.rs`

**Interfaces:**
- Consumes: `Direction`, `Outcome` from Task 2, `PublishedEvent`.
- Produces: `TransferEvent::{OfferIncoming, Progress, Finished}` (derives `Debug, Clone, PartialEq, Eq, Serialize`; `Finished` carries `message: Option<String>`) on channel `network-transfer`; `AuditEventKind::{NetworkCollectionSent, NetworkCollectionReceived}`, each with `device_id` (short id), `collection` and `outcome: String` (the `Outcome` wire name: `completed` or `unconfirmed` for sent, `completed`, `damaged` or `rejected` for received, index R4 and R6), with tags `network_collection_sent`, `network_collection_received` under SOC 2 `CC6.7`. The audit crate does not depend on `rocket-network`, so `outcome` is a plain string; Plan 3 writes `serde` wire names.

- [ ] **Step 1: Write the failing event tests**

In `crates/rocket-network/src/events.rs`, add to the test module (after the existing tests, inside `mod tests`):

```rust
    fn transfer_cases() -> Vec<(TransferEvent, &'static [&'static str])> {
        vec![
            (
                TransferEvent::OfferIncoming {
                    transfer_id: "T".into(),
                    device_id: "ID".into(),
                    short_id: "ABCD-EFGH".into(),
                    display_name: "Ada".into(),
                    collection_name: "Orders".into(),
                    size_bytes: 1234,
                },
                &[
                    "type",
                    "transfer_id",
                    "device_id",
                    "short_id",
                    "display_name",
                    "collection_name",
                    "size_bytes",
                ][..],
            ),
            (
                TransferEvent::Progress {
                    transfer_id: "T".into(),
                    direction: Direction::Send,
                    done_bytes: 10,
                    total_bytes: 20,
                },
                &["type", "transfer_id", "direction", "done_bytes", "total_bytes"][..],
            ),
            (
                TransferEvent::Finished {
                    transfer_id: "T".into(),
                    direction: Direction::Receive,
                    outcome: Outcome::TooBig,
                    display_name: "Ada".into(),
                    collection_name: "Orders".into(),
                    message: None,
                },
                &[
                    "type",
                    "transfer_id",
                    "direction",
                    "outcome",
                    "display_name",
                    "collection_name",
                    "message",
                ][..],
            ),
        ]
    }

    fn transfer_keys(event: &TransferEvent) -> Vec<String> {
        let value = event.to_json().expect("serializes");
        let mut keys: Vec<String> = value
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    #[test]
    fn transfer_events_use_one_channel() {
        for (event, _) in transfer_cases() {
            assert_eq!(event.channel(), "network-transfer", "event {event:?}");
            assert!(event.also_emit_on().is_empty());
            assert!(event.written_collections().is_empty());
        }
    }

    // Wire contract: tag is camelCase, fields stay snake_case. A rename here
    // breaks the frontend listeners and needs its own decision.
    #[test]
    fn wire_contract_transfer_events() {
        for (event, expected) in transfer_cases() {
            let mut want: Vec<String> = expected.iter().map(|k| k.to_string()).collect();
            want.sort();
            assert_eq!(transfer_keys(&event), want, "event {event:?}");
        }
    }

    #[test]
    fn wire_contract_transfer_tags_are_camel_case() {
        let tags: Vec<String> = transfer_cases()
            .iter()
            .map(|(event, _)| {
                event.to_json().expect("serializes")["type"]
                    .as_str()
                    .expect("tag")
                    .to_string()
            })
            .collect();
        assert_eq!(tags, ["offerIncoming", "progress", "finished"]);
    }

    #[test]
    fn wire_contract_transfer_envelope_keys_are_free() {
        for (event, _) in transfer_cases() {
            let keys = transfer_keys(&event);
            for reserved in ["seq", "tsMs", "ts_ms", "workspaceId"] {
                assert!(
                    !keys.iter().any(|k| k == reserved),
                    "event {event:?} uses {reserved}"
                );
            }
        }
    }

    #[test]
    fn transfer_event_json_carries_the_values() {
        let progress = TransferEvent::Progress {
            transfer_id: "T1".into(),
            direction: Direction::Send,
            done_bytes: 10,
            total_bytes: 20,
        };
        assert_eq!(
            progress.to_json().expect("serializes"),
            json!({
                "type": "progress",
                "transfer_id": "T1",
                "direction": "send",
                "done_bytes": 10,
                "total_bytes": 20
            })
        );
        let finished = TransferEvent::Finished {
            transfer_id: "T1".into(),
            direction: Direction::Receive,
            outcome: Outcome::TooBig,
            display_name: "Ada".into(),
            collection_name: "Orders".into(),
            message: None,
        };
        assert_eq!(
            finished.to_json().expect("serializes"),
            json!({
                "type": "finished",
                "transfer_id": "T1",
                "direction": "receive",
                "outcome": "too_big",
                "display_name": "Ada",
                "collection_name": "Orders",
                "message": null
            })
        );
    }

    #[test]
    fn finished_carries_a_null_or_a_string_message() {
        let finished = |message: Option<&str>| TransferEvent::Finished {
            transfer_id: "T1".into(),
            direction: Direction::Receive,
            outcome: Outcome::Failed,
            display_name: "Ada".into(),
            collection_name: "Orders".into(),
            message: message.map(str::to_string),
        };
        let none = finished(None).to_json().expect("serializes");
        assert!(none.as_object().expect("an object").contains_key("message"));
        assert!(none["message"].is_null());
        let some = finished(Some("The bundle has no collection.")).to_json().expect("serializes");
        assert_eq!(some["message"], "The bundle has no collection.");
        // The event is cloneable, the service fans it out to the bus and its own state.
        assert_eq!(finished(Some("x")).clone(), finished(Some("x")));
    }

    #[test]
    fn transfer_events_can_be_downcast_and_are_not_network_events() {
        let event: Box<dyn PublishedEvent> = Box::new(TransferEvent::Progress {
            transfer_id: "T".into(),
            direction: Direction::Send,
            done_bytes: 0,
            total_bytes: 1,
        });
        assert!(event.as_any().downcast_ref::<TransferEvent>().is_some());
        assert!(event.as_any().downcast_ref::<NetworkEvent>().is_none());
    }
```

At the top of the test module add `use crate::transfer::{Direction, Outcome};` (the module already has `use super::*;`, so also add the same import at the top of the file in Step 3 and this one is then redundant: keep only the file-level import).

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -j4 -p rocket-network events`
Expected: FAIL to compile, `cannot find type TransferEvent in this scope`.

- [ ] **Step 3: Add the event**

In `crates/rocket-network/src/events.rs` add the import under the existing `use` lines:

```rust
use crate::transfer::{Direction, Outcome};
```

Update the module doc comment by appending one line: `//! `TransferEvent` follows the same rule on its own channel.` Then add after the `NetworkEvent` impl:

```rust
/// Events about collection transfers. Channel `network-transfer`.
///
/// Same wire rule as `NetworkEvent`: camelCase tag, snake_case fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TransferEvent {
    /// A paired node offered a collection. The frontend asks the user.
    OfferIncoming {
        transfer_id: String,
        device_id: String,
        short_id: String,
        display_name: String,
        collection_name: String,
        size_bytes: u64,
    },
    /// Bytes moved. The frontend hides this for a transfer that ends in under 300 ms.
    Progress {
        transfer_id: String,
        direction: Direction,
        done_bytes: u64,
        total_bytes: u64,
    },
    /// A transfer ended. `outcome` is the snake_case `Outcome` (`unconfirmed` included).
    /// `message` is extra text for the receiving side (the importer's failure text, or "The
    /// other node went offline."), `null` when there is none.
    Finished {
        transfer_id: String,
        direction: Direction,
        outcome: Outcome,
        display_name: String,
        collection_name: String,
        message: Option<String>,
    },
}

impl PublishedEvent for TransferEvent {
    fn channel(&self) -> &'static str {
        "network-transfer"
    }

    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
```

In `crates/rocket-network/src/lib.rs` change `pub use events::NetworkEvent;` to `pub use events::{NetworkEvent, TransferEvent};`.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -j4 -p rocket-network events`
Expected: PASS, the seven older tests and the seven new ones (14 in the module).

- [ ] **Step 5: Write the failing audit tests**

In `crates/rocket-audit/src/event.rs`, add to `mod tests`:

```rust
    #[test]
    fn network_collection_kinds_have_stable_tags() {
        let sent = AuditEventKind::NetworkCollectionSent {
            device_id: "ABCD-EFGH".into(),
            collection: "Orders".into(),
            outcome: "completed".into(),
        };
        let received = AuditEventKind::NetworkCollectionReceived {
            device_id: "ABCD-EFGH".into(),
            collection: "Orders".into(),
            outcome: "rejected".into(),
        };
        assert_eq!(sent.tag(), "network_collection_sent");
        assert_eq!(received.tag(), "network_collection_received");
    }

    #[test]
    fn network_collection_kinds_map_to_soc2_cc6_7_only() {
        let kinds = [
            AuditEventKind::NetworkCollectionSent {
                device_id: "ABCD-EFGH".into(),
                collection: "Orders".into(),
                outcome: "unconfirmed".into(),
            },
            AuditEventKind::NetworkCollectionReceived {
                device_id: "ABCD-EFGH".into(),
                collection: "Orders".into(),
                outcome: "damaged".into(),
            },
        ];
        for kind in kinds {
            let tag = kind.tag();
            let ev = SecurityAuditEvent::new("user", None, kind, "");
            assert!(
                ev.controls.iter().any(|c| c.code == "CC6.7"),
                "{tag} is not under CC6.7"
            );
            assert!(
                !ev.controls.iter().any(|c| c.code == "CC6.1"),
                "{tag} must not be under CC6.1"
            );
        }
    }

    #[test]
    fn network_collection_kinds_serialize_with_snake_case_fields() {
        for (kind, tag, outcome) in [
            (
                AuditEventKind::NetworkCollectionSent {
                    device_id: "ABCD-EFGH".into(),
                    collection: "Orders".into(),
                    outcome: "unconfirmed".into(),
                },
                "network_collection_sent",
                "unconfirmed",
            ),
            (
                AuditEventKind::NetworkCollectionReceived {
                    device_id: "ABCD-EFGH".into(),
                    collection: "Orders".into(),
                    outcome: "rejected".into(),
                },
                "network_collection_received",
                "rejected",
            ),
        ] {
            let value = serde_json::to_value(&kind).expect("kind serializes");
            assert_eq!(value["kind"], tag);
            assert_eq!(value["device_id"], "ABCD-EFGH");
            assert_eq!(value["collection"], "Orders");
            assert_eq!(value["outcome"], outcome);
            let back: AuditEventKind = serde_json::from_value(value).expect("kind round trips");
            assert_eq!(back, kind);
        }
    }
```

In `crates/rocket-audit/src/control.rs`, add to `mod tests`:

```rust
    #[test]
    fn cc6_7_lists_the_send_collection_kinds_and_keeps_the_old_ones() {
        for tag in [
            "collection_exported",
            "audit_evidence_exported",
            "network_collection_sent",
            "network_collection_received",
        ] {
            let controls = controls_for_kind(tag);
            assert!(
                controls
                    .iter()
                    .any(|c| c.framework == Framework::Soc2 && c.code == "CC6.7"),
                "{tag} missing from CC6.7"
            );
        }
    }
```

- [ ] **Step 6: Run to verify it fails**

Run: `cargo test -j4 -p rocket-audit network_collection`
Expected: FAIL to compile, `no variant named NetworkCollectionSent` (and, once the kinds exist without it, `no field outcome`).

- [ ] **Step 7: Add the kinds**

In `crates/rocket-audit/src/event.rs` add after `NetworkDeviceBlocked`:

```rust
    /// A collection left for a paired node. `device_id` is the short id (`XXXX-XXXX`).
    /// `outcome` is `completed` (the receiver confirmed), `unconfirmed` (the final ack came, no result) or `failed` (the stream ended before the final ack).
    NetworkCollectionSent {
        device_id: String,
        collection: String,
        outcome: String,
    },
    /// A collection from a paired node arrived. `device_id` is the short id. `outcome` is
    /// `completed` (imported), `damaged` (size or hash mismatch) or `rejected` (unsafe ZIP).
    NetworkCollectionReceived {
        device_id: String,
        collection: String,
        outcome: String,
    },
```

and in `tag()`:

```rust
            Self::NetworkCollectionSent { .. } => "network_collection_sent",
            Self::NetworkCollectionReceived { .. } => "network_collection_received",
```

In `crates/rocket-audit/src/control.rs` change the CC6.7 entry to:

```rust
        kinds: &[
            "collection_exported",
            "audit_evidence_exported",
            "network_collection_sent",
            "network_collection_received",
        ],
```

- [ ] **Step 8: Update the crate guide**

In `crates/rocket-network/CLAUDE.md`, change the `PeerMessage` row to mention the transfer frames and add rows to the Public Types table:

```markdown
| `PeerMessage` | WebSocket frame enum: Hello, Ping, Pong, Unpair and the six transfer frames (offer, answer, chunk, abort, chunk ack, result). `decode` enforces the 64 KiB limit and rejects unknown types. |
| `AbortReason`, `ResultOutcome` | Why a transfer stopped (`cancelled`, `failed`, `too_big`, `busy`, `expired`) and what the receiver reports at the end (`completed`, `damaged`, `rejected`, `failed`). |
| `TransferId`, `chunk_count_for`, `offer_name_is_valid`, `Direction`, `Outcome` | Transfer id (32 lowercase hex), chunk math, the offer name rule, direction and result names (`unconfirmed` when the sender got no result after the last chunk). |
| `OutgoingTransfer`, `IncomingTransfer`, `OfferError`, `ChunkStep` | Sender and receiver state machines. Time is passed in as `Instant`. They hold no bytes. The sender keeps an ack window of 32 chunks (`window_open`, `on_ack`) and stays `Done` until `on_result` brings the receiver's outcome, or ends `Unconfirmed` when the peer goes or the result times out. |
| `TransferLimits`, `Admission` | Pending, per-peer and rate limits for offers, plus duplicate-id detection. |
| `TransferSink` | Port with `on_frame` (the six transfer frames from `NetworkService`) and `on_peer_gone` (peer socket ended or pairing removed). |
| `NetworkEvent`, `TransferEvent` | Events for the frontend. Implement `PublishedEvent`. Channels `network-changed`, `network-pairing` and `network-transfer`. |
```

(Remove the old `PeerMessage` and `NetworkEvent` rows they replace.) In "Key Patterns" add: `- Transfer machines never read the clock and never touch bytes on disk. They return decoded chunk bytes to the caller.`

- [ ] **Step 9: Run to verify and finish**

Run: `cargo test -j4 -p rocket-audit`
Expected: all tests pass, including the three older network tests and the three `network_collection` tests with `outcome`.

Run: `cargo check -j4 -p rocket-audit`
Expected: Finished. If any other crate does an exhaustive match on `AuditEventKind` it fails here or below; the earlier grep found none in Rust (only the TypeScript row in `src/components/audit/AuditEventRow.tsx`, which is Plan 5).

Run: `cargo check -j4 -p rocket-app`
Expected: Finished (it depends on both crates).

Run: `cargo test -j4 -p rocket-network`
Expected: all tests pass.

Run: `cargo clippy -j4 -p rocket-network --all-targets`
Expected: no warnings.

Run: `cargo clippy -j4 -p rocket-audit --all-targets`
Expected: no warnings.

- [ ] **Step 10: Whole-plan verification**

Run these one at a time:

```bash
cargo check -j4 -p rocket-network
cargo check -j4 -p rocket-audit
cargo check -j4 -p rocket-app
cargo test -j4 -p rocket-network
cargo test -j4 -p rocket-audit
cargo test -j4 -p rocket-infra network::
cargo test -j4 -p rocket-app network::
cargo fmt -p rocket-network -- --check
cargo fmt -p rocket-audit -- --check
```

Expected: every command finishes green (if a `fmt` check prints a diff, run `cargo fmt -p <that crate>` and check again; never `cargo fmt --all`). `git status --short` lists only the files of the File Structure table plus this plan folder.

No commit; reviewed at the plan end.

---

## Self-review

Spec sections covered: 6 (the six variants including `ChunkAck`, `TransferResult` and `ResultOutcome`, the ack window and the offer name rule, version 2, the pinned test, frame limit, ignore-unknown-id and abort-as-failed rules as state-machine behaviour), 7 (all eleven new constants including `SENDER_RESULT_TIMEOUT`, `ACK_EVERY_CHUNKS` and `MAX_UNACKED_CHUNKS`, `MAX_PENDING_INCOMING`, per-peer and rate limits in `TransferLimits`; the two ZIP limits are only declared here and used by Plan 2), 9 (`rocket-network` pieces, `TransferSink` with `on_frame` and `on_peer_gone`, `TransferEvent` with `message` and its contract test, the minimal `run_socket` arm listing all six variants), 8 (the audit kinds with `outcome`, written by Plan 3 for `completed`/`unconfirmed`/`failed` sends and `completed`/`damaged`/`rejected` receives; the allow-list change is the `PeerMessage` variants; the sink routing is Plan 3), 11 (domain tests: expiry, abort, duplicate offer, unknown id, out-of-order and surplus chunks, exact 10 MiB boundary, wire contract tests for the six frames and the event keys, a result arriving while `Done`, before the last chunk, twice and after an abort, `Unconfirmed` only after the final ack (`Failed` before it), the ack window and the receiver's ack cadence). Task 2 also carries the additions Plans 3 and 4 ask for: `OutgoingTransfer::fail_with` (item 1 of its "Needs from Plan 1"), `OutgoingTransfer::on_result` with `awaiting_result` (rulings 1 of the index), and the `with_offer_ttl` test seam on both machines (item 2, used by Plan 4), plus the second-review additions of the index (R2 ack window, R4 `Unconfirmed`, R6 audit `outcome`, R15 name rule, R18 version note and comment fix). Not covered here on purpose: sender and receiver services (Plan 3), ZIP validation and staging (Plan 2), commands and e2e (Plan 4), TypeScript (Plan 5).
