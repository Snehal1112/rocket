# Team network sync: Plan 1, domain

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 1 of 5.** Previous work: the team network MVP (`docs/superpowers/plans/team-network/`) and send a collection (`docs/superpowers/plans/team-network-send/`), both merged to `main`.
**Next plan to run: `docs/superpowers/plans/team-network-sync/2026-11-08-sync-plan-02-storage-merge.md`.** Do not start it without the user.
**Recommended model: Sonnet.**
**No commit during the plan.** The user reviews the whole plan, then commits through `dev-workflow-skills:1-git-commit` with explicit paths.
**Ledger:** `.superpowers/sdd/2026-11-08-sync-plan-01/progress.md`, one `Ruling:` line per decision made while implementing.

**Goal:** The domain can describe, plan and resolve a two-way sync of a linked collection. `PROTOCOL_VERSION` is 3. `rocket-network` holds the link types (`LinkId`, `SyncId`, `Manifest` with `base_hash` and JSON, `LinkState`, `LinkRecord`, `SyncPlanPayload`), the pure three-way planner (`plan_sync`, `resolve`, `new_base`) with renames and conflicts, the three `Sync*` frames, `keep_in_sync` and `link_id` on `CollectionOffer`, `SyncEvent` and the `LinkRepository` port. `rocket-audit` knows three new event kinds. Nothing is sent, stored or applied yet.

**Architecture:** Everything new in `rocket-network` is pure: no clock, no I/O, no logging, no new dependencies. The planner works on `Manifest` values (path to SHA-256 of the scrubbed export file) and never sees bytes. `plan_sync` runs on the planning node with its own manifest as "mine" and the starter's manifest as "theirs", and returns actions that apply on their own plus conflicts. `resolve` turns the plan and the user's decisions into the final action list and refuses a plan with a missing decision. `new_base` gives the manifest both sides store after the apply. The new `PeerMessage` variants only carry ids and sizes: every manifest, plan and file bundle travels as a transfer on the existing machines (index deviation 1). The only edits outside `rocket-network` and `rocket-audit` are the ones that keep `rocket-app` compiling: one or-pattern in `connections.rs`, and the two new `CollectionOffer` fields in five places (listed in Task 3, step 12). Plan 3 replaces them with real routing.

**Tech Stack:** Rust (`rocket-network`, `rocket-audit`, small compile fixes in `rocket-app`), serde, sha2, rand.

## Spec

`docs/superpowers/specs/2026-11-08-team-network-sync-collection-design.md`, sections 2 (decisions 4, 5, 8, 10, 11), 5 (the link), 6 (flow, the table and the rename rule), 7 (protocol and safety), 10 (audit), 11 (placement), 12 (domain tests). Binding names: `docs/superpowers/plans/team-network-sync/00-plan-index.md` ("Plan 1 produces", and the three deviations at its top).

## Global Constraints

- Pure crate rules from `crates/rocket-network/CLAUDE.md`: no I/O, no runtime, no new dependencies. `serde`, `serde_json`, `sha2` and `rand` are already in `Cargo.toml`. No collection, YAML or trust code is touched, so the OpenCollection read is not needed.
- Rust: no `unwrap()` and no `expect()` in production code. `expect("reason")` is fine in tests only. Mutex locks (tests only here) use `.lock().unwrap_or_else(|e| e.into_inner())`.
- `PROTOCOL_VERSION` is exactly 3. `MAX_MANIFEST_ENTRIES` is 5000 and equals `MAX_ZIP_ENTRIES`. `SYNC_PAYLOAD_NAME_MAX` is 32.
- Ids (`LinkId`, `SyncId`) are 32 lowercase hex characters. A hash is 64 lowercase hex characters.
- `base_hash` is the SHA-256, lowercase hex, over the sorted lines `<path> NUL <hash> LF` (bytes `0x00` and `0x0A`, no other separator). The empty manifest hashes the empty input.
- A manifest path is `/` separated, 1 to 1024 bytes, relative, with no empty segment, no `.` or `..` segment, no `\`, no `:`, no control character, no segment equal to `.rocket` (any case), no segment ending in `.` or a space, and no device name (`CON`, `PRN`, `AUX`, `NUL`, `COM1` to `COM9`, `LPT1` to `LPT9`, any case, with or without an extension).
- Wire types are snake_case JSON and `PeerMessage` keeps `deny_unknown_fields`. The new frame tags are `sync_offer`, `sync_abort`, `sync_result`. `send_collection` must stay an unknown type: an existing test uses it.
- A `CollectionOffer` without a link encodes exactly as in version 2 (`keep_in_sync` false and `link_id` none are skipped on encode) and decodes with both defaults.
- `SyncEvent`: camelCase tag, snake_case variant fields, helper DTOs (`SyncSummary`, `ConflictView`) camelCase, channel `network-sync`. A `wire_contract_*` test pins the keys. `seq` and `tsMs` are added by `TauriEventBus`, never by the event.
- Persistence structs (`LinkRecord`, `Manifest` as stored) are snake_case and never get `rename_all = "camelCase"`.
- `rocket-audit`: field names stay snake_case (`device_id`, `collection`, `outcome`). Fields are device ids, collection names and an outcome word only, never file names or contents.
- Errors never quote a path, a hash or file content: the message is a fixed sentence.
- Cargo commands always pass `-j4`, one crate at a time. Never `--workspace`. Never `cargo test` with `--all`. `cargo fmt -p <crate>` only, never `--all`.
- Comments are short full sentences that end with a period. No emojis.
- No TypeScript in this plan. Plan 5 adds the audit row and the event types.
- No commit during the plan.

## Review Focus

The five input classes the tests must pin, each with its tests:

1. **A hostile or damaged manifest or plan.** `..`, an absolute path, a backslash, a drive letter, `.rocket`, a device name, a control character, an over-long path, a bad or upper-case hash, a duplicate path, one entry over `MAX_MANIFEST_ENTRIES`, a payload over the transfer cap and an unknown JSON field are all refused with a fixed message that does not repeat the input. The same path rule guards the plan payload. Tests (Task 1): `insert_rejects_unsafe_paths`, `insert_rejects_bad_hashes`, `a_manifest_holds_at_most_5000_entries`, `from_json_rejects_bad_input`, `from_json_accepts_exactly_the_cap`, `from_json_refuses_a_payload_over_the_transfer_cap_without_parsing`, `the_error_never_repeats_the_path`, `a_manifest_stored_as_a_map_is_validated_when_read`, `a_record_with_an_unsafe_base_path_or_a_bad_id_is_refused_on_read`, `ids_are_32_lowercase_hex_characters`. Task 2: `the_plan_payload_rejects_unsafe_paths_unknown_fields_and_too_many_entries`.
2. **A `base_hash` that differs between two nodes that agree.** It must not depend on insertion order, and any path or hash change must change it. A pinned vector protects the byte layout. Tests (Task 1): `base_hash_is_pinned_and_ignores_insertion_order`, `base_hash_changes_with_a_path_or_a_hash`, `json_round_trips`, `the_record_base_hash_is_the_hash_of_its_base`.
3. **A path in the wrong row of the table.** Every row of spec section 6, the role swap, rename by hash, a rename against an edit, a rename onto an existing path, a file renamed on both sides and a renamed-and-edited file (a delete plus an add) are pinned, and the plan is sorted. Tests (Task 2): `row_all_equal_does_nothing`, `row_only_theirs_changed_takes_theirs`, `row_only_mine_changed_keeps_mine_and_sends_it`, `row_same_change_on_both_sides_does_nothing`, `row_different_changes_conflict`, `row_deleted_here_and_untouched_there_takes_the_delete`, `row_deleted_here_and_edited_there_conflicts`, `row_new_file_here_is_kept_and_sent`, `row_new_file_there_is_taken`, `row_the_same_new_file_on_both_sides_does_nothing`, `row_two_different_new_files_conflict`, `rows_with_the_roles_swapped_follow_the_same_rules`, `swapping_the_two_sides_mirrors_the_plan`, `a_plan_is_sorted_by_path`, `a_rename_by_the_peer_is_a_rename_here_not_a_delete_and_an_add`, `a_rename_here_tells_the_peer_to_move_its_file`, `two_renames_of_equal_content_pair_in_path_order`, `a_renamed_and_edited_file_looks_like_a_delete_plus_an_add`, `a_rename_by_the_peer_against_an_edit_here_is_a_conflict`, `a_rename_here_against_an_edit_by_the_peer_is_a_conflict`, `a_rename_onto_a_path_the_other_side_holds_is_not_a_rename`, `a_file_renamed_on_both_sides_is_not_a_rename_either_way`, `a_removed_file_and_an_added_file_with_other_content_are_not_a_rename`.
4. **A decision that is missing, doubled, aimed at nothing, or leaves the two sides different.** `resolve` refuses a missing, duplicate or stray decision and a rename conflict without its marker. Keep-both names never collide with each other or with a path on either side. Every resolution of every conflict kind ends with equal manifests on both sides and `new_base` equals that state. Tests (Task 2): `resolve_refuses_a_missing_decision`, `resolve_refuses_a_duplicate_or_unknown_decision`, `resolve_refuses_a_rename_conflict_without_its_rename_marker`, `keep_mine_and_keep_theirs_pick_a_side`, `keep_both_on_a_delete_against_an_edit_keeps_the_edit`, `keep_both_names_the_second_copy_from_peer`, `keep_both_counts_up_until_the_name_is_free`, `keep_both_keeps_the_folder_and_handles_names_without_an_extension`, `two_keep_both_conflicts_never_pick_the_same_name`, `resolve_keeps_the_plain_actions_and_drops_the_rename_marker`, `rename_against_edit_resolves_each_way`, `new_base_is_the_agreed_state_for_a_clean_two_way_sync`, `new_base_after_keep_mine_keep_theirs_and_keep_both`, `new_base_drops_deleted_paths`, `every_resolution_of_every_conflict_kind_converges`.
5. **A wire change that breaks a version 2 frame, a v3 peer, or a frontend listener.** The version is 3 and a version 2 hello makes a node `NeedsUpdate`. A sync frame refuses unknown fields and bad values. An offer without sync fields decodes and encodes as before. The `SyncEvent` keys are pinned. Tests (Task 3): `spec_values_are_pinned` (Task 1), `a_peer_on_the_previous_protocol_version_needs_an_update`, `sync_frames_have_the_expected_shape`, `sync_enums_use_snake_case_names`, `sync_frames_reject_unknown_fields_and_bad_values`, `a_version_2_offer_without_sync_fields_still_decodes`, `an_offer_for_a_link_carries_both_fields`, `an_offer_without_a_link_encodes_as_in_version_2`, `an_offer_with_a_bad_keep_in_sync_is_rejected`, `sync_payload_names_fit_the_offer_name_rule`, `every_sync_frame_fits_the_frame_limit_with_room_to_spare`, `wire_contract_sync_events`, `wire_contract_sync_tags_are_camel_case`, `wire_contract_sync_envelope_keys_are_free`, `wire_contract_sync_helper_dtos_are_camel_case`, `sync_event_json_carries_the_values`, `sync_enums_serialize_as_snake_case`.

Also covered because they share the code paths: the id types, `LinkRecord` persistence (snake_case keys, optional uid), the `LinkRepository` object safety and a usable fake, the audit kinds (stable tags, snake_case fields, SOC 2 control mapping), and the unchanged send tests that now construct `CollectionOffer` with the two new fields.

## File Structure

| File | Change | Task |
|---|---|---|
| `crates/rocket-network/src/limits.rs` | Modify: version 3, `MAX_MANIFEST_ENTRIES`, `SYNC_PAYLOAD_NAME_MAX`, pinned tests | 1 |
| `crates/rocket-network/src/link.rs` | Create: `LinkId`, `SyncId`, `path_is_valid`, `is_sha256_hex`, `Manifest`, `LinkState`, `LinkRecord`; Task 2 adds `RenamePair`, `SyncPlanPayload` and two crate-private `Manifest` accessors | 1, 2 |
| `crates/rocket-network/src/planner.rs` | Create: `SyncAction`, `ConflictKind`, `Conflict`, `Resolution`, `Decision`, `SyncPlan`, `plan_sync`, `resolve`, `new_base` | 2 |
| `crates/rocket-network/src/messages.rs` | Modify: `SyncPayloadKind`, `SyncAbortReason`, `SyncOutcome`, three variants, two fields on `CollectionOffer`, version 3 fixtures, tests | 3 |
| `crates/rocket-network/src/events.rs` | Modify: `SyncEvent`, `SyncSummary`, `ConflictView`, `SyncRole`, `SyncFinish` and tests | 3 |
| `crates/rocket-network/src/ports.rs` | Modify: `LinkRepository` and tests | 3 |
| `crates/rocket-network/src/transfer.rs` | Modify: `offer_message` and its test build the offer with the two new fields | 3 |
| `crates/rocket-network/src/lib.rs` | Modify: `pub mod link; pub mod planner;` and re-exports | 1, 2, 3 |
| `crates/rocket-network/CLAUDE.md` | Modify: public types table | 3 |
| `crates/rocket-audit/src/event.rs` | Modify: three kinds, `tag()`, tests | 3 |
| `crates/rocket-audit/src/control.rs` | Modify: CC6.7 gets the two sync kinds, CC6.1 gets the link removal, test | 3 |
| `crates/rocket-app/src/network/connections.rs` | Modify: the or-pattern lists the three new variants | 3 |
| `crates/rocket-app/src/network/transfer.rs` | Modify: the `CollectionOffer` pattern gets `..` | 3 |
| `crates/rocket-app/src/network/transfer_fakes.rs`, `transfer_receive_tests.rs`, `transfer_link_tests.rs`, `transfer_send_tests.rs` | Modify: offers built or matched with the two new fields | 3 |
| `crates/rocket-app/src/network/connections_tests.rs` | Modify: one new test for a version 2 peer | 3 |

Existing tests that change (found by `grep -rnE "PROTOCOL_VERSION|version: ?[0-9]|\"version\": ?[0-9]" crates/rocket-network crates/rocket-infra crates/rocket-app`), and what happens to each:

- `limits::tests::spec_values_are_pinned` asserts `PROTOCOL_VERSION == 2`. Changed to 3 in Task 1.
- `messages::tests`: `hello()` (`version: 2`), the shape test JSON (`"version": 2`) and five inline Hello strings (`"version":2`). Changed to 3 in Task 3. The inline strings are only fixtures (`decode` does not check the version), changed so a reader is not misled.
- `messages::tests::transfer_messages` and `a_name_cannot_push_an_offer_past_the_frame_limit` build a `CollectionOffer`: they gain the two new fields.
- `transfer::tests::the_sender_builds_the_offer_frame` and `OutgoingTransfer::offer_message` build a `CollectionOffer`: they gain the two new fields (false and none).
- Left alone because they use `PROTOCOL_VERSION` or `PROTOCOL_VERSION + 1` and follow the bump: `rocket-app` `connections_tests::a_socket_with_another_protocol_version_needs_an_update`, `service_tests::a_node_with_another_protocol_version_needs_an_update`, `pairing_tests::a_bad_version_or_profile_is_a_bad_request`, and the fixtures of `rocket-infra/src/network/{server_tests,peer_test_support,transport}.rs`.
- Left alone because they are independent literals never compared with the constant: `wire.rs` (62, 70, 81), `records.rs:168`, `ports.rs` (286, 299, 312), `rocket-infra/src/network/mdns.rs` (307, 428), `server_tests.rs:161`.

Runtime effect of the bump (intended by spec decision 10): a version 2 peer and a version 3 peer never talk. `run_socket` closes a socket whose Hello version differs (`version_matches` in `crates/rocket-app/src/network/connections.rs`) and marks the node `needs_update`, and `handler.rs` refuses a pairing request from a version 2 node. The team network is unreleased. Plan 5 repeats this in `docs/features/team-network.md`.

---

## Task 1: Limits, ids, manifest and link record

**Files:**
- Modify: `crates/rocket-network/src/limits.rs`
- Create: `crates/rocket-network/src/link.rs`
- Modify: `crates/rocket-network/src/lib.rs`

**Interfaces:**
- Consumes: `DeviceId`, `DomainError::InvalidInput`, `MAX_TRANSFER_BYTES`.
- Produces: `PROTOCOL_VERSION = 3`, `MAX_MANIFEST_ENTRIES`, `SYNC_PAYLOAD_NAME_MAX`, `LinkId`, `SyncId` (`random`, `parse`, `as_str`, serde as a plain string validated on read), `path_is_valid`, `is_sha256_hex`, `Manifest` (`new`, `insert`, `get`, `paths`, `len`, `is_empty`, `base_hash`, `to_json`, `from_json`, serde as a path-to-hash map validated on read), `LinkState`, `LinkRecord` (`base_hash`).

- [ ] **Step 1: Write the failing limits tests**

In `crates/rocket-network/src/limits.rs`, change `assert_eq!(PROTOCOL_VERSION, 2);` (the first line of `spec_values_are_pinned`) to:

```rust
        assert_eq!(PROTOCOL_VERSION, 3);
```

and add these assertions after `assert_eq!(MAX_UNACKED_CHUNKS, 32);`:

```rust
        assert_eq!(MAX_MANIFEST_ENTRIES, 5000);
        assert_eq!(SYNC_PAYLOAD_NAME_MAX, 32);
```

Add one test after `a_full_chunk_fits_a_frame_with_room_for_the_envelope`:

```rust
    #[test]
    fn sync_limits_follow_the_zip_limit() {
        // A manifest lists the files of one collection ZIP, so it can never hold more.
        assert_eq!(MAX_MANIFEST_ENTRIES, MAX_ZIP_ENTRIES);
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -j4 -p rocket-network limits`
Expected: FAIL to compile, `cannot find value MAX_MANIFEST_ENTRIES in this scope`.

- [ ] **Step 3: Add the constants**

In `crates/rocket-network/src/limits.rs` change `pub const PROTOCOL_VERSION: u32 = 2;` to `pub const PROTOCOL_VERSION: u32 = 3;`, and add after `pub const MAX_UNACKED_CHUNKS: u32 = 32;`:

```rust
/// The most files one manifest lists. Equals `MAX_ZIP_ENTRIES`.
pub const MAX_MANIFEST_ENTRIES: usize = 5000;
/// The longest name a sync payload transfer carries in its offer.
pub const SYNC_PAYLOAD_NAME_MAX: usize = 32;
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -j4 -p rocket-network limits`
Expected: PASS, 6 tests (`spec_values_are_pinned`, `transfer_limits_are_consistent`, `a_full_chunk_fits_a_frame_with_room_for_the_envelope`, `sync_limits_follow_the_zip_limit`, `port_range_holds_ten_ports`, `unpaired_connections_stay_below_the_total`).

- [ ] **Step 5: Write the failing link tests**

Create `crates/rocket-network/src/link.rs` with only this test module. The module checks the id rules, the pinned `base_hash` vector (`278204d7...` is the SHA-256 of the bytes `a.yml NUL aaaa... LF b/c.yml NUL bbbb... LF`, computed with `sha256sum`), every path and hash rejection, the 5000 cap, the JSON shape and the record's persistence keys.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn two() -> Manifest {
        let mut manifest = Manifest::new();
        manifest.insert("b/c.yml", B).expect("valid entry");
        manifest.insert("a.yml", A).expect("valid entry");
        manifest
    }

    #[test]
    fn ids_are_32_lowercase_hex_characters() {
        let link = LinkId::random();
        assert_eq!(link.as_str().len(), 32);
        assert_eq!(LinkId::parse(link.as_str()).expect("round trip"), link);
        assert_ne!(LinkId::random(), LinkId::random());
        let sync = SyncId::random();
        assert_eq!(SyncId::parse(sync.as_str()).expect("round trip"), sync);
        for bad in [
            "",
            "abc",
            &"A".repeat(32),
            &"g".repeat(32),
            &"a".repeat(31),
            &"a".repeat(33),
            &"é".repeat(16),
        ] {
            assert!(LinkId::parse(bad).is_err(), "link id {bad:?}");
            assert!(SyncId::parse(bad).is_err(), "sync id {bad:?}");
        }
    }

    #[test]
    fn ids_serialize_as_plain_strings_and_validate_on_read() {
        let id = LinkId::parse("0123456789abcdef0123456789abcdef").expect("valid");
        assert_eq!(
            serde_json::to_string(&id).expect("json"),
            "\"0123456789abcdef0123456789abcdef\""
        );
        assert!(serde_json::from_str::<LinkId>("\"nope\"").is_err());
        assert_eq!(
            serde_json::from_str::<LinkId>("\"0123456789abcdef0123456789abcdef\"").expect("valid"),
            id
        );
    }

    #[test]
    fn base_hash_is_pinned_and_ignores_insertion_order() {
        // sha256 of "a.yml\0<A>\nb/c.yml\0<B>\n".
        assert_eq!(
            two().base_hash(),
            "278204d7086284145bc980a5fa47f7bc6abfe3e0d4670ded3ec18e4b7aaa096d"
        );
        let mut one = Manifest::new();
        one.insert("a.yml", A).expect("valid entry");
        assert_eq!(
            one.base_hash(),
            "79aa69741931a903111f2314d0fe1be6fb11b748eb6be1564ec1205c740a2ae1"
        );
        // The empty manifest hashes the empty string.
        assert_eq!(
            Manifest::new().base_hash(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let mut other = Manifest::new();
        other.insert("a.yml", A).expect("valid entry");
        other.insert("b/c.yml", B).expect("valid entry");
        assert_eq!(other.base_hash(), two().base_hash());
    }

    #[test]
    fn base_hash_changes_with_a_path_or_a_hash() {
        let mut changed_hash = two();
        changed_hash.insert("a.yml", B).expect("valid entry");
        assert_ne!(changed_hash.base_hash(), two().base_hash());
        let mut renamed = Manifest::new();
        renamed.insert("a2.yml", A).expect("valid entry");
        renamed.insert("b/c.yml", B).expect("valid entry");
        assert_ne!(renamed.base_hash(), two().base_hash());
    }

    #[test]
    fn the_manifest_reads_back_what_was_inserted() {
        let manifest = two();
        assert_eq!(manifest.len(), 2);
        assert!(!manifest.is_empty());
        assert_eq!(manifest.get("a.yml"), Some(A));
        assert_eq!(manifest.get("missing.yml"), None);
        assert_eq!(manifest.paths().collect::<Vec<_>>(), ["a.yml", "b/c.yml"]);
        let mut replaced = manifest.clone();
        replaced
            .insert("a.yml", B)
            .expect("an existing path can be replaced");
        assert_eq!(replaced.len(), 2);
        assert_eq!(replaced.get("a.yml"), Some(B));
    }

    #[test]
    fn insert_accepts_ordinary_paths() {
        let mut manifest = Manifest::new();
        for path in [
            "opencollection.yml",
            "folder/request.yml",
            "a/b/c/d.yml",
            "My Folder/get users (v2).yml",
            "über/ünï.yml",
            "scripts/pre.js",
            ".gitignore",
            "console.yml",
            "com10.yml",
            "x/.rocketapi.yml",
        ] {
            manifest
                .insert(path, A)
                .unwrap_or_else(|e| panic!("{path} should be accepted: {e:?}"));
        }
    }

    #[test]
    fn insert_rejects_unsafe_paths() {
        let long = "a/".repeat(600);
        for path in [
            "",
            "/etc/passwd",
            "/a.yml",
            "../a.yml",
            "a/../b.yml",
            "a/..",
            "..",
            ".",
            "./a.yml",
            "a/./b.yml",
            "a//b.yml",
            "a/",
            "a\\b.yml",
            "..\\a.yml",
            "C:\\a.yml",
            "C:/a.yml",
            ".rocket/state.yml",
            "a/.rocket/state.yml",
            ".ROCKET/x.yml",
            "a/.rocket",
            "CON",
            "con.yml",
            "a/NUL.txt",
            "aux/x.yml",
            "COM1",
            "x/lpt9.yml",
            "name.",
            "name /x.yml",
            "a\u{0}b.yml",
            "a\nb.yml",
            "a\u{7f}b.yml",
            long.as_str(),
        ] {
            let err = Manifest::new().insert(path, A).expect_err(path);
            assert!(matches!(err, DomainError::InvalidInput(_)), "path {path:?}");
        }
    }

    #[test]
    fn insert_rejects_bad_hashes() {
        let upper = "A".repeat(64);
        let short = "a".repeat(63);
        let long = "a".repeat(65);
        let not_hex = "g".repeat(64);
        for hash in [
            "",
            upper.as_str(),
            short.as_str(),
            long.as_str(),
            not_hex.as_str(),
            "é",
        ] {
            assert!(
                Manifest::new().insert("a.yml", hash).is_err(),
                "hash {hash:?}"
            );
        }
    }

    #[test]
    fn a_manifest_holds_at_most_5000_entries() {
        let mut manifest = Manifest::new();
        for n in 0..MAX_MANIFEST_ENTRIES {
            manifest
                .insert(&format!("r{n}.yml"), A)
                .expect("within the cap");
        }
        assert_eq!(manifest.len(), 5000);
        assert!(manifest.insert("one-too-many.yml", A).is_err());
        assert_eq!(manifest.len(), 5000);
        // Replacing an existing path is not a new entry.
        manifest.insert("r0.yml", B).expect("replaces");
    }

    #[test]
    fn to_json_lists_entries_sorted_by_path() {
        let text = String::from_utf8(two().to_json().expect("encodes")).expect("utf8");
        assert_eq!(
            text,
            format!(
                r#"{{"entries":[{{"path":"a.yml","sha256":"{A}"}},{{"path":"b/c.yml","sha256":"{B}"}}]}}"#
            )
        );
        assert_eq!(
            Manifest::new().to_json().expect("encodes"),
            br#"{"entries":[]}"#.to_vec()
        );
    }

    #[test]
    fn json_round_trips() {
        let manifest = two();
        let back = Manifest::from_json(&manifest.to_json().expect("encodes")).expect("decodes");
        assert_eq!(back, manifest);
        assert_eq!(back.base_hash(), manifest.base_hash());
    }

    #[test]
    fn from_json_rejects_bad_input() {
        let entry = |path: &str, hash: &str| format!(r#"{{"path":"{path}","sha256":"{hash}"}}"#);
        let wrap = |entries: &[String]| format!(r#"{{"entries":[{}]}}"#, entries.join(","));
        let too_many: Vec<String> = (0..=MAX_MANIFEST_ENTRIES)
            .map(|n| entry(&format!("r{n}.yml"), A))
            .collect();
        for (name, text) in [
            ("not json", "nope".to_string()),
            ("wrong shape", "[]".to_string()),
            ("missing entries", "{}".to_string()),
            ("unknown field", r#"{"entries":[],"extra":1}"#.to_string()),
            (
                "unknown entry field",
                format!(r#"{{"entries":[{{"path":"a.yml","sha256":"{A}","x":1}}]}}"#),
            ),
            ("dot dot", wrap(&[entry("../a.yml", A)])),
            ("absolute", wrap(&[entry("/a.yml", A)])),
            ("backslash", wrap(&[entry("a\\\\b.yml", A)])),
            ("rocket folder", wrap(&[entry(".rocket/x.yml", A)])),
            ("device name", wrap(&[entry("nul.yml", A)])),
            ("bad hex", wrap(&[entry("a.yml", &"z".repeat(64))])),
            ("upper case hex", wrap(&[entry("a.yml", &"A".repeat(64))])),
            ("short hash", wrap(&[entry("a.yml", "abc")])),
            (
                "duplicate path",
                wrap(&[entry("a.yml", A), entry("a.yml", B)]),
            ),
            ("over the cap", wrap(&too_many)),
        ] {
            let err = Manifest::from_json(text.as_bytes()).expect_err(name);
            assert!(matches!(err, DomainError::InvalidInput(_)), "case {name}");
        }
    }

    #[test]
    fn from_json_accepts_exactly_the_cap() {
        let mut manifest = Manifest::new();
        for n in 0..MAX_MANIFEST_ENTRIES {
            manifest
                .insert(&format!("r{n}.yml"), A)
                .expect("within the cap");
        }
        let back = Manifest::from_json(&manifest.to_json().expect("encodes")).expect("decodes");
        assert_eq!(back.len(), 5000);
    }

    #[test]
    fn from_json_refuses_a_payload_over_the_transfer_cap_without_parsing() {
        let huge = vec![b' '; MAX_TRANSFER_BYTES as usize + 1];
        assert!(Manifest::from_json(&huge).is_err());
    }

    #[test]
    fn the_error_never_repeats_the_path() {
        let err = Manifest::new()
            .insert("../secret-token-123", A)
            .expect_err("rejected");
        match err {
            DomainError::InvalidInput(text) => assert!(!text.contains("secret-token")),
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    #[test]
    fn a_manifest_stored_as_a_map_is_validated_when_read() {
        let manifest = two();
        let text = serde_json::to_string(&manifest).expect("json");
        assert_eq!(text, format!(r#"{{"a.yml":"{A}","b/c.yml":"{B}"}}"#));
        assert_eq!(
            serde_json::from_str::<Manifest>(&text).expect("valid"),
            manifest
        );
        let bad = format!(r#"{{"../a.yml":"{A}"}}"#);
        assert!(serde_json::from_str::<Manifest>(&bad).is_err());
    }

    fn record() -> LinkRecord {
        LinkRecord {
            link_id: LinkId::parse("0123456789abcdef0123456789abcdef").expect("valid"),
            peer_device_id: DeviceId::from_bytes([7; 32]),
            collection_name: "Orders".into(),
            collection_root: "/home/ada/collections/orders".into(),
            collection_uid: Some("col-1".into()),
            base: two(),
            state: LinkState::ChangesWaiting,
            updated_at: "2026-11-08T10:00:00Z".into(),
        }
    }

    #[test]
    fn link_states_use_snake_case_names() {
        for (state, name) in [
            (LinkState::InSync, "in_sync"),
            (LinkState::ChangesWaiting, "changes_waiting"),
            (LinkState::OutOfDate, "out_of_date"),
            (LinkState::Broken, "broken"),
        ] {
            assert_eq!(
                serde_json::to_value(state).expect("json"),
                serde_json::json!(name)
            );
            let back: LinkState = serde_json::from_value(serde_json::json!(name)).expect("reads");
            assert_eq!(back, state);
        }
    }

    #[test]
    fn a_link_record_round_trips_with_snake_case_keys() {
        let record = record();
        let value = serde_json::to_value(&record).expect("json");
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "base",
                "collection_name",
                "collection_root",
                "collection_uid",
                "link_id",
                "peer_device_id",
                "state",
                "updated_at"
            ]
        );
        assert_eq!(value["state"], "changes_waiting");
        assert_eq!(value["base"]["a.yml"], A);
        let back: LinkRecord = serde_json::from_value(value).expect("reads");
        assert_eq!(back, record);
    }

    #[test]
    fn a_record_without_a_uid_reads_back_with_none() {
        let mut value = serde_json::to_value(record()).expect("json");
        value
            .as_object_mut()
            .expect("an object")
            .remove("collection_uid");
        let back: LinkRecord = serde_json::from_value(value).expect("reads");
        assert_eq!(back.collection_uid, None);
    }

    #[test]
    fn a_record_with_an_unsafe_base_path_or_a_bad_id_is_refused_on_read() {
        let mut value = serde_json::to_value(record()).expect("json");
        value["base"] = serde_json::json!({ "../x.yml": A });
        assert!(serde_json::from_value::<LinkRecord>(value).is_err());
        let mut value = serde_json::to_value(record()).expect("json");
        value["link_id"] = serde_json::json!("not-an-id");
        assert!(serde_json::from_value::<LinkRecord>(value).is_err());
    }

    #[test]
    fn the_record_base_hash_is_the_hash_of_its_base() {
        assert_eq!(record().base_hash(), two().base_hash());
        assert_eq!(
            record().base_hash(),
            "278204d7086284145bc980a5fa47f7bc6abfe3e0d4670ded3ec18e4b7aaa096d"
        );
    }
}
```

In `crates/rocket-network/src/lib.rs` add `pub mod link;` after `pub mod limits;`.

- [ ] **Step 6: Run to verify it fails**

Run: `cargo test -j4 -p rocket-network link::`
Expected: FAIL to compile, `cannot find type Manifest in this scope` (and `LinkId`, `LinkRecord`).

- [ ] **Step 7: Implement `link.rs`**

Insert this code at the top of `crates/rocket-network/src/link.rs`, above the `#[cfg(test)]` line. The hex helpers, the id macro, the path rule and `base_hash` are fixed by the Global Constraints. `insert` is the only way into a `Manifest`, and the serde `Deserialize` goes through it, so a stored or received manifest is always valid.

```rust
//! Link types: ids, the manifest and the link record. Pure: no I/O.

use std::collections::BTreeMap;

use rand::RngCore;
use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

use crate::device::DeviceId;
use crate::limits::{MAX_MANIFEST_ENTRIES, MAX_TRANSFER_BYTES};

const PATH_MAX_BYTES: usize = 1024;

/// Whether the text is exactly 32 lowercase hex characters.
fn is_hex32(text: &str) -> bool {
    text.len() == 32
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
/// A new random 128-bit value as 32 lowercase hex characters.
fn random_hex32() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Defines an id type: 32 lowercase hex characters (a random 128-bit value), not secret.
macro_rules! hex_id {
    ($(#[$meta:meta])* $name:ident, $what:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// A new random id.
            pub fn random() -> $name {
                $name(random_hex32())
            }

            /// Accepts exactly 32 lowercase hex characters.
            pub fn parse(text: &str) -> DomainResult<$name> {
                if is_hex32(text) {
                    Ok($name(text.to_string()))
                } else {
                    Err(DomainError::InvalidInput(concat!("invalid ", $what).to_string()))
                }
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = DomainError;

            fn try_from(text: String) -> DomainResult<$name> {
                $name::parse(&text)
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> String {
                id.0
            }
        }
    };
}

hex_id!(
    /// Joins two linked copies of a collection. Made by the sender at the first send.
    LinkId,
    "link id"
);
hex_id!(
    /// One sync run. Made by the node that starts it.
    SyncId,
    "sync id"
);

/// Whether the text is a lowercase hex SHA-256 (64 characters).
pub fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_device_name(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or("").to_ascii_lowercase();
    let numbered = (stem.starts_with("com") || stem.starts_with("lpt"))
        && matches!(stem.as_bytes(), [_, _, _, b'1'..=b'9']);
    matches!(stem.as_str(), "con" | "prn" | "aux" | "nul") || numbered
}

/// Whether a collection-relative path is safe: `/` separated, inside the root, no `..`, no
/// `.rocket` folder, no device names, no drive letters, no control characters.
pub fn path_is_valid(path: &str) -> bool {
    if path.is_empty() || path.len() > PATH_MAX_BYTES || path.starts_with('/') {
        return false;
    }
    if path
        .chars()
        .any(|c| c.is_control() || c == '\\' || c == ':')
    {
        return false;
    }
    path.split('/').all(|segment| {
        !segment.is_empty()
            && segment != "."
            && segment != ".."
            && !segment.eq_ignore_ascii_case(".rocket")
            && !segment.ends_with('.')
            && !segment.ends_with(' ')
            && !is_device_name(segment)
    })
}

/// Relative path (`/` separated) to the lowercase hex SHA-256 of the exported file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Manifest(BTreeMap<String, String>);

impl Manifest {
    pub fn new() -> Manifest {
        Manifest(BTreeMap::new())
    }
    pub fn insert(&mut self, path: &str, sha256: &str) -> DomainResult<()> {
        if !path_is_valid(path) {
            return Err(DomainError::InvalidInput("invalid manifest path".into()));
        }
        if !is_sha256_hex(sha256) {
            return Err(DomainError::InvalidInput("invalid manifest hash".into()));
        }
        if !self.0.contains_key(path) && self.0.len() >= MAX_MANIFEST_ENTRIES {
            return Err(DomainError::InvalidInput(
                "manifest has too many entries".into(),
            ));
        }
        self.0.insert(path.to_string(), sha256.to_string());
        Ok(())
    }
    pub fn get(&self, path: &str) -> Option<&str> {
        self.0.get(path).map(String::as_str)
    }
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn base_hash(&self) -> String {
        let mut hasher = Sha256::new();
        for (path, hash) in &self.0 {
            hasher.update(path.as_bytes());
            hasher.update([0u8]);
            hasher.update(hash.as_bytes());
            hasher.update(b"\n");
        }
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
    fn from_checked(entries: BTreeMap<String, String>) -> DomainResult<Manifest> {
        let mut manifest = Manifest::new();
        for (path, hash) in &entries {
            manifest.insert(path, hash)?;
        }
        Ok(manifest)
    }
    pub fn to_json(&self) -> DomainResult<Vec<u8>> {
        let file = ManifestFile {
            entries: self
                .0
                .iter()
                .map(|(p, h)| ManifestEntry {
                    path: p.clone(),
                    sha256: h.clone(),
                })
                .collect(),
        };
        serde_json::to_vec(&file)
            .map_err(|_| DomainError::InvalidInput("manifest cannot be encoded".into()))
    }
    pub fn from_json(bytes: &[u8]) -> DomainResult<Manifest> {
        if bytes.len() as u64 > MAX_TRANSFER_BYTES {
            return Err(DomainError::InvalidInput("manifest too large".into()));
        }
        let file: ManifestFile = serde_json::from_slice(bytes)
            .map_err(|_| DomainError::InvalidInput("invalid manifest".into()))?;
        let mut manifest = Manifest::new();
        for entry in &file.entries {
            if manifest.0.contains_key(&entry.path) {
                return Err(DomainError::InvalidInput("duplicate manifest path".into()));
            }
            manifest.insert(&entry.path, &entry.sha256)?;
        }
        Ok(manifest)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestEntry {
    path: String,
    sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestFile {
    entries: Vec<ManifestEntry>,
}

impl Serialize for Manifest {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}
impl<'de> Deserialize<'de> for Manifest {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Manifest, D::Error> {
        let entries = BTreeMap::<String, String>::deserialize(d)?;
        Manifest::from_checked(entries).map_err(serde::de::Error::custom)
    }
}

/// What the user sees for a link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkState {
    InSync,
    /// A local change since the base.
    ChangesWaiting,
    /// A sync failed after the peer applied.
    OutOfDate,
    Broken,
}

/// One link from a local collection to a collection on one paired node.
///
/// A persistence struct: snake_case keys, never sent on the wire. The base files live next to
/// it in the repository (`LinkRepository::save`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkRecord {
    pub link_id: LinkId,
    pub peer_device_id: DeviceId,
    pub collection_name: String,
    /// Canonical path of the local collection folder.
    pub collection_root: String,
    /// The `uid` in the local `opencollection.yml`. A root that no longer holds it is broken.
    #[serde(default)]
    pub collection_uid: Option<String>,
    /// Path to hash as last agreed.
    pub base: Manifest,
    pub state: LinkState,
    /// RFC 3339.
    pub updated_at: String,
}

impl LinkRecord {
    /// The hash both sides compare at the start of a sync.
    pub fn base_hash(&self) -> String {
        self.base.base_hash()
    }
}
```

In `crates/rocket-network/src/lib.rs` add after `pub use identity::{DeviceKeyMaterial, PeerIdentity};`:

```rust
pub use link::{is_sha256_hex, path_is_valid, LinkId, LinkRecord, LinkState, Manifest, SyncId};
```

- [ ] **Step 8: Run to verify it passes**

Run: `cargo test -j4 -p rocket-network link::`
Expected: PASS, 21 tests.

Run: `cargo clippy -j4 -p rocket-network --all-targets`
Expected: no warnings.

---

## Task 2: Planner, resolve, new base and the plan payload

**Files:**
- Create: `crates/rocket-network/src/planner.rs`
- Modify: `crates/rocket-network/src/link.rs`
- Modify: `crates/rocket-network/src/lib.rs`

**Interfaces:**
- Consumes: `Manifest`, `path_is_valid`, `DomainError::InvalidInput`.
- Produces: `SyncAction`, `ConflictKind`, `Conflict`, `Resolution`, `Decision`, `SyncPlan`, `plan_sync`, `resolve`, `new_base`, and in `link.rs` `RenamePair` and `SyncPlanPayload` (`to_json`, `from_json`).

The algorithm, fixed here because the tests pin every case. "Mine" is the planning node.

**Rename pairing.** For one side, take the base paths that side no longer has, in ascending order. For each, take the first still unused path (ascending) that the side added (not in the base) and whose hash equals the removed path's base hash. That pair is a rename candidate `from -> to`. Candidates are found for theirs first, then for mine.

**Applying a candidate** (the renamer is one side, the other side is "other"). The candidate is dropped, and both paths go through the table like any other path, when other already holds `to`, or when other has no `from` (it deleted it too). Otherwise both paths are consumed:
- other's file at `from` equals the base: a clean rename. `RenameMine { from, to }` when theirs renamed, `RenameTheirs { from, to }` when mine renamed.
- other's file at `from` differs from the base: a `RenameVsEdit` conflict at path `from`, with `mine` and `theirs` the two hashes (the renamed content on the renamer's side). The same rename action stays in `actions` as a marker: it is the only place the new path `to` is recorded, and `resolve` replaces it.

**The table**, for each remaining path of the sorted union of base, mine and theirs, with `b`, `m`, `t` the optional hashes:
- `m == t`: nothing (covers all equal, the same change, deleted on both sides).
- else `b == m` (only theirs changed): `Pull { path }` when theirs has it, else `DeleteMine { path }`.
- else `b == t` (only mine changed): `Push { path }` when mine has it, else `DeleteTheirs { path }`.
- else a conflict: `BothAdded` when `b` is none, `DeleteVsEdit` when mine or theirs is absent, `BothChanged` otherwise.

**Order.** `actions` are sorted by their first path (`path`, or `from` for a rename). `conflicts` are sorted by `path`.

**Resolve.** Decisions are matched to conflicts by path. A decision for a path that is not a conflict, a path decided twice and a conflict without a decision are `InvalidInput`. The result is the plan's actions without the rename markers, then, per conflict in order, the actions below. For a table conflict (`m_has`, `t_has` say who still has the file):
- `KeepMine`: `Push { path }` when `m_has`, else `DeleteTheirs { path }`.
- `KeepTheirs`: `Pull { path }` when `t_has`, else `DeleteMine { path }`.
- `KeepBoth`: when only one side has the file, the file survives (`Push` or `Pull`). When both have it: `RenameTheirs { from: path, to: as_path }`, `PullAs { path, as_path }`, `Push { path }`, in that order. The peer moves its own file to `as_path` (keeping its secrets), this node writes the peer's exported file at `as_path`, and the peer then receives this node's file at `path`. `as_path` is `<dir><stem> (from peer)<ext>`, then `<stem> (from peer) 2<ext>`, ` 3` and so on, the first not in `taken` and not already chosen in this call. `ext` starts at the last `.` of the file name unless that is the first character. The name must pass `path_is_valid`.

For a `RenameVsEdit` conflict, `q` is the `to` of the marker, and the marker's kind tells who renamed (`RenameMine` means theirs renamed and mine was edited):

| Marker | KeepMine | KeepTheirs | KeepBoth |
|---|---|---|---|
| `RenameMine { p, q }` (theirs renamed, mine edited) | `Push p`, `DeleteTheirs q` | `DeleteMine p`, `Pull q` | `Push p`, `Pull q` |
| `RenameTheirs { p, q }` (mine renamed, theirs edited) | `DeleteTheirs p`, `Push q` | `Pull p`, `DeleteMine q` | `Pull p`, `Push q` |

A `RenameVsEdit` conflict without its marker in `plan.actions` is `InvalidInput`.

`taken` must hold every path present on either node. Plan 3 builds it as the union of both manifests.

**New base.** `new_base` returns the post-sync manifest as the planning node sees it: start from `mine`, apply the `RenameMine` actions, then `DeleteMine` (remove), `Pull` (the path takes `theirs`'s hash) and `PullAs` (`as_path` takes `theirs`'s hash for `path`). `Push`, `DeleteTheirs` and `RenameTheirs` change nothing here because they only change the other node, which ends in the same state (a test simulates both nodes and proves it). `base` and `decisions` are part of the index signature but unused, because the final actions already carry every decision.

- [ ] **Step 1: Write the failing planner tests**

Create `crates/rocket-network/src/planner.rs` with only this test module. The helper `h('1')` is a 64 character hash of one repeated digit, `one(base, mine, theirs)` plans a single path `r.yml`, `simulate` applies final actions to both sides the way the nodes do (renames, then deletes, then copies) and `assert_converges` proves both sides end equal and `new_base` equals that state.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// A 64 character hash made of one repeated hex digit.
    fn h(digit: char) -> String {
        digit.to_string().repeat(64)
    }

    fn manifest(entries: &[(&str, char)]) -> Manifest {
        let mut manifest = Manifest::new();
        for (path, digit) in entries {
            manifest.insert(path, &h(*digit)).expect("valid entry");
        }
        manifest
    }

    fn pull(path: &str) -> SyncAction {
        SyncAction::Pull { path: path.into() }
    }
    fn push(path: &str) -> SyncAction {
        SyncAction::Push { path: path.into() }
    }
    fn delete_mine(path: &str) -> SyncAction {
        SyncAction::DeleteMine { path: path.into() }
    }
    fn delete_theirs(path: &str) -> SyncAction {
        SyncAction::DeleteTheirs { path: path.into() }
    }
    fn rename_mine(from: &str, to: &str) -> SyncAction {
        SyncAction::RenameMine {
            from: from.into(),
            to: to.into(),
        }
    }
    fn rename_theirs(from: &str, to: &str) -> SyncAction {
        SyncAction::RenameTheirs {
            from: from.into(),
            to: to.into(),
        }
    }
    fn conflict(
        path: &str,
        kind: ConflictKind,
        mine: Option<char>,
        theirs: Option<char>,
    ) -> Conflict {
        Conflict {
            path: path.into(),
            kind,
            mine: mine.map(h),
            theirs: theirs.map(h),
        }
    }
    fn decide(path: &str, resolution: Resolution) -> Decision {
        Decision {
            path: path.into(),
            resolution,
        }
    }

    /// One path, three states. `None` means absent.
    fn one(base: Option<char>, mine: Option<char>, theirs: Option<char>) -> SyncPlan {
        let build = |digit: Option<char>| match digit {
            Some(d) => manifest(&[("r.yml", d)]),
            None => Manifest::new(),
        };
        plan_sync(&build(base), &build(mine), &build(theirs))
    }

    // ----- the table of spec section 6 -----

    #[test]
    fn row_all_equal_does_nothing() {
        assert_eq!(one(Some('1'), Some('1'), Some('1')), SyncPlan::default());
    }

    #[test]
    fn row_only_theirs_changed_takes_theirs() {
        let plan = one(Some('1'), Some('1'), Some('2'));
        assert_eq!(plan.actions, vec![pull("r.yml")]);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn row_only_mine_changed_keeps_mine_and_sends_it() {
        let plan = one(Some('1'), Some('2'), Some('1'));
        assert_eq!(plan.actions, vec![push("r.yml")]);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn row_same_change_on_both_sides_does_nothing() {
        assert_eq!(one(Some('1'), Some('2'), Some('2')), SyncPlan::default());
    }

    #[test]
    fn row_different_changes_conflict() {
        let plan = one(Some('1'), Some('2'), Some('3'));
        assert!(plan.actions.is_empty());
        assert_eq!(
            plan.conflicts,
            vec![conflict(
                "r.yml",
                ConflictKind::BothChanged,
                Some('2'),
                Some('3')
            )]
        );
    }

    #[test]
    fn row_deleted_here_and_untouched_there_takes_the_delete() {
        let plan = one(Some('1'), None, Some('1'));
        assert_eq!(plan.actions, vec![delete_theirs("r.yml")]);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn row_deleted_here_and_edited_there_conflicts() {
        let plan = one(Some('1'), None, Some('2'));
        assert!(plan.actions.is_empty());
        assert_eq!(
            plan.conflicts,
            vec![conflict(
                "r.yml",
                ConflictKind::DeleteVsEdit,
                None,
                Some('2')
            )]
        );
    }

    #[test]
    fn row_new_file_here_is_kept_and_sent() {
        let plan = one(None, Some('1'), None);
        assert_eq!(plan.actions, vec![push("r.yml")]);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn row_new_file_there_is_taken() {
        let plan = one(None, None, Some('1'));
        assert_eq!(plan.actions, vec![pull("r.yml")]);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn row_the_same_new_file_on_both_sides_does_nothing() {
        assert_eq!(one(None, Some('1'), Some('1')), SyncPlan::default());
    }

    #[test]
    fn row_two_different_new_files_conflict() {
        let plan = one(None, Some('1'), Some('2'));
        assert!(plan.actions.is_empty());
        assert_eq!(
            plan.conflicts,
            vec![conflict(
                "r.yml",
                ConflictKind::BothAdded,
                Some('1'),
                Some('2')
            )]
        );
    }

    #[test]
    fn rows_with_the_roles_swapped_follow_the_same_rules() {
        let plan = one(Some('1'), Some('1'), None);
        assert_eq!(plan.actions, vec![delete_mine("r.yml")]);
        let plan = one(Some('1'), Some('2'), None);
        assert!(plan.actions.is_empty());
        assert_eq!(
            plan.conflicts,
            vec![conflict(
                "r.yml",
                ConflictKind::DeleteVsEdit,
                Some('2'),
                None
            )]
        );
        // Deleted on both sides is the same change.
        assert_eq!(one(Some('1'), None, None), SyncPlan::default());
        assert_eq!(one(None, None, None), SyncPlan::default());
    }

    fn mirror(action: &SyncAction) -> SyncAction {
        match action {
            SyncAction::Pull { path } => push(path),
            SyncAction::Push { path } => pull(path),
            SyncAction::DeleteMine { path } => delete_theirs(path),
            SyncAction::DeleteTheirs { path } => delete_mine(path),
            SyncAction::RenameMine { from, to } => rename_theirs(from, to),
            SyncAction::RenameTheirs { from, to } => rename_mine(from, to),
            SyncAction::PullAs { .. } => panic!("the planner never emits PullAs"),
        }
    }

    #[test]
    fn swapping_the_two_sides_mirrors_the_plan() {
        let base = manifest(&[
            ("a.yml", '1'),
            ("b.yml", '1'),
            ("c.yml", '1'),
            ("d.yml", '1'),
            ("e.yml", '1'),
            ("f.yml", '1'),
            ("g.yml", '1'),
        ]);
        let mine = manifest(&[
            ("a.yml", '2'),
            ("b.yml", '1'),
            ("c.yml", '2'),
            ("e.yml", '1'),
            ("f.yml", '2'),
            ("n.yml", '3'),
            ("x/moved.yml", '1'),
        ]);
        let theirs = manifest(&[
            ("a.yml", '1'),
            ("b.yml", '2'),
            ("c.yml", '3'),
            ("d.yml", '1'),
            ("f.yml", '2'),
            ("m.yml", '4'),
        ]);
        let forward = plan_sync(&base, &mine, &theirs);
        let backward = plan_sync(&base, &theirs, &mine);
        let mirrored: Vec<SyncAction> = backward.actions.iter().map(mirror).collect();
        assert_eq!(forward.actions, mirrored);
        let swapped: Vec<Conflict> = backward
            .conflicts
            .iter()
            .map(|c| Conflict {
                path: c.path.clone(),
                kind: c.kind,
                mine: c.theirs.clone(),
                theirs: c.mine.clone(),
            })
            .collect();
        assert_eq!(forward.conflicts, swapped);
        assert!(!forward.actions.is_empty() && !forward.conflicts.is_empty());
    }

    #[test]
    fn a_plan_is_sorted_by_path() {
        let base = manifest(&[("b.yml", '1'), ("a.yml", '1')]);
        let mine = manifest(&[("b.yml", '1'), ("a.yml", '1'), ("c.yml", '2')]);
        let theirs = manifest(&[("b.yml", '2'), ("a.yml", '2'), ("0.yml", '3')]);
        let plan = plan_sync(&base, &mine, &theirs);
        assert_eq!(
            plan.actions,
            vec![pull("0.yml"), pull("a.yml"), pull("b.yml"), push("c.yml")]
        );
    }

    // ----- renames -----

    #[test]
    fn a_rename_by_the_peer_is_a_rename_here_not_a_delete_and_an_add() {
        let base = manifest(&[("a.yml", '1')]);
        let plan = plan_sync(&base, &base, &manifest(&[("b.yml", '1')]));
        assert_eq!(plan.actions, vec![rename_mine("a.yml", "b.yml")]);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn a_rename_here_tells_the_peer_to_move_its_file() {
        let base = manifest(&[("a.yml", '1')]);
        let plan = plan_sync(&base, &manifest(&[("x/b.yml", '1')]), &base);
        assert_eq!(plan.actions, vec![rename_theirs("a.yml", "x/b.yml")]);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn two_renames_of_equal_content_pair_in_path_order() {
        let base = manifest(&[("a.yml", '1'), ("b.yml", '1')]);
        let theirs = manifest(&[("d.yml", '1'), ("c.yml", '1')]);
        let plan = plan_sync(&base, &base, &theirs);
        assert_eq!(
            plan.actions,
            vec![rename_mine("a.yml", "c.yml"), rename_mine("b.yml", "d.yml")]
        );
    }

    #[test]
    fn a_renamed_and_edited_file_looks_like_a_delete_plus_an_add() {
        let base = manifest(&[("a.yml", '1')]);
        let plan = plan_sync(&base, &base, &manifest(&[("b.yml", '2')]));
        assert_eq!(plan.actions, vec![delete_mine("a.yml"), pull("b.yml")]);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn a_rename_by_the_peer_against_an_edit_here_is_a_conflict() {
        let base = manifest(&[("a.yml", '1')]);
        let plan = plan_sync(
            &base,
            &manifest(&[("a.yml", '2')]),
            &manifest(&[("b.yml", '1')]),
        );
        assert_eq!(
            plan.conflicts,
            vec![conflict(
                "a.yml",
                ConflictKind::RenameVsEdit,
                Some('2'),
                Some('1')
            )]
        );
        // The rename stays in the plan as the marker that tells resolve the new path.
        assert_eq!(plan.actions, vec![rename_mine("a.yml", "b.yml")]);
    }

    #[test]
    fn a_rename_here_against_an_edit_by_the_peer_is_a_conflict() {
        let base = manifest(&[("a.yml", '1')]);
        let plan = plan_sync(
            &base,
            &manifest(&[("b.yml", '1')]),
            &manifest(&[("a.yml", '2')]),
        );
        assert_eq!(
            plan.conflicts,
            vec![conflict(
                "a.yml",
                ConflictKind::RenameVsEdit,
                Some('1'),
                Some('2')
            )]
        );
        assert_eq!(plan.actions, vec![rename_theirs("a.yml", "b.yml")]);
    }

    #[test]
    fn a_rename_onto_a_path_the_other_side_holds_is_not_a_rename() {
        let base = manifest(&[("a.yml", '1')]);
        let mine = manifest(&[("a.yml", '1'), ("b.yml", '5')]);
        let theirs = manifest(&[("b.yml", '1')]);
        let plan = plan_sync(&base, &mine, &theirs);
        assert_eq!(plan.actions, vec![delete_mine("a.yml")]);
        assert_eq!(
            plan.conflicts,
            vec![conflict(
                "b.yml",
                ConflictKind::BothAdded,
                Some('5'),
                Some('1')
            )]
        );
    }

    #[test]
    fn a_file_renamed_on_both_sides_is_not_a_rename_either_way() {
        let base = manifest(&[("a.yml", '1')]);
        let plan = plan_sync(
            &base,
            &manifest(&[("b.yml", '1')]),
            &manifest(&[("c.yml", '1')]),
        );
        assert_eq!(plan.actions, vec![push("b.yml"), pull("c.yml")]);
        assert!(plan.conflicts.is_empty());
        let same = plan_sync(
            &base,
            &manifest(&[("b.yml", '1')]),
            &manifest(&[("b.yml", '1')]),
        );
        assert_eq!(same, SyncPlan::default());
    }

    #[test]
    fn a_removed_file_and_an_added_file_with_other_content_are_not_a_rename() {
        let base = manifest(&[("a.yml", '1')]);
        let plan = plan_sync(&base, &base, &manifest(&[("b.yml", '2')]));
        assert!(plan.actions.iter().all(|a| !matches!(
            a,
            SyncAction::RenameMine { .. } | SyncAction::RenameTheirs { .. }
        )));
    }

    // ----- resolve -----

    fn taken_of(mine: &Manifest, theirs: &Manifest) -> Manifest {
        let mut taken = Manifest::new();
        for source in [mine, theirs] {
            for path in source.paths() {
                taken
                    .insert(path, source.get(path).expect("listed path"))
                    .expect("valid entry");
            }
        }
        taken
    }

    #[test]
    fn resolve_refuses_a_missing_decision() {
        let plan = one(Some('1'), Some('2'), Some('3'));
        let err = resolve(&plan, &[], &Manifest::new()).expect_err("a conflict needs a decision");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn resolve_refuses_a_duplicate_or_unknown_decision() {
        let plan = one(Some('1'), Some('2'), Some('3'));
        let twice = [
            decide("r.yml", Resolution::KeepMine),
            decide("r.yml", Resolution::KeepTheirs),
        ];
        assert!(matches!(
            resolve(&plan, &twice, &Manifest::new()),
            Err(DomainError::InvalidInput(_))
        ));
        let stray = [
            decide("r.yml", Resolution::KeepMine),
            decide("other.yml", Resolution::KeepMine),
        ];
        assert!(matches!(
            resolve(&plan, &stray, &Manifest::new()),
            Err(DomainError::InvalidInput(_))
        ));
        // A plan without conflicts accepts no decisions at all.
        let clean = one(Some('1'), Some('1'), Some('2'));
        assert_eq!(
            resolve(&clean, &[], &Manifest::new()).expect("no conflicts"),
            vec![pull("r.yml")]
        );
        assert!(resolve(
            &clean,
            &[decide("r.yml", Resolution::KeepMine)],
            &Manifest::new()
        )
        .is_err());
    }

    #[test]
    fn keep_mine_and_keep_theirs_pick_a_side() {
        let both_changed = one(Some('1'), Some('2'), Some('3'));
        let taken = Manifest::new();
        assert_eq!(
            resolve(
                &both_changed,
                &[decide("r.yml", Resolution::KeepMine)],
                &taken
            )
            .expect("resolves"),
            vec![push("r.yml")]
        );
        assert_eq!(
            resolve(
                &both_changed,
                &[decide("r.yml", Resolution::KeepTheirs)],
                &taken
            )
            .expect("resolves"),
            vec![pull("r.yml")]
        );
        let delete_vs_edit = one(Some('1'), None, Some('2'));
        assert_eq!(
            resolve(
                &delete_vs_edit,
                &[decide("r.yml", Resolution::KeepMine)],
                &taken
            )
            .expect("resolves"),
            vec![delete_theirs("r.yml")]
        );
        assert_eq!(
            resolve(
                &delete_vs_edit,
                &[decide("r.yml", Resolution::KeepTheirs)],
                &taken
            )
            .expect("resolves"),
            vec![pull("r.yml")]
        );
        let edit_vs_delete = one(Some('1'), Some('2'), None);
        assert_eq!(
            resolve(
                &edit_vs_delete,
                &[decide("r.yml", Resolution::KeepMine)],
                &taken
            )
            .expect("resolves"),
            vec![push("r.yml")]
        );
        assert_eq!(
            resolve(
                &edit_vs_delete,
                &[decide("r.yml", Resolution::KeepTheirs)],
                &taken
            )
            .expect("resolves"),
            vec![delete_mine("r.yml")]
        );
    }

    #[test]
    fn keep_both_on_a_delete_against_an_edit_keeps_the_edit() {
        let taken = Manifest::new();
        let there = one(Some('1'), None, Some('2'));
        assert_eq!(
            resolve(&there, &[decide("r.yml", Resolution::KeepBoth)], &taken).expect("resolves"),
            vec![pull("r.yml")]
        );
        let here = one(Some('1'), Some('2'), None);
        assert_eq!(
            resolve(&here, &[decide("r.yml", Resolution::KeepBoth)], &taken).expect("resolves"),
            vec![push("r.yml")]
        );
    }

    #[test]
    fn keep_both_names_the_second_copy_from_peer() {
        let plan = one(Some('1'), Some('2'), Some('3'));
        let actions = resolve(
            &plan,
            &[decide("r.yml", Resolution::KeepBoth)],
            &manifest(&[("r.yml", '2')]),
        )
        .expect("resolves");
        assert_eq!(
            actions,
            vec![
                rename_theirs("r.yml", "r (from peer).yml"),
                SyncAction::PullAs {
                    path: "r.yml".into(),
                    as_path: "r (from peer).yml".into()
                },
                push("r.yml"),
            ]
        );
    }

    #[test]
    fn keep_both_counts_up_until_the_name_is_free() {
        let plan = one(Some('1'), Some('2'), Some('3'));
        let taken = manifest(&[
            ("r.yml", '2'),
            ("r (from peer).yml", '4'),
            ("r (from peer) 2.yml", '4'),
        ]);
        let actions =
            resolve(&plan, &[decide("r.yml", Resolution::KeepBoth)], &taken).expect("resolves");
        assert!(actions.contains(&SyncAction::PullAs {
            path: "r.yml".into(),
            as_path: "r (from peer) 3.yml".into()
        }));
    }

    #[test]
    fn keep_both_keeps_the_folder_and_handles_names_without_an_extension() {
        let base = manifest(&[("f/r.yml", '1'), ("f/folder", '1'), ("f/.hidden", '1')]);
        let mine = manifest(&[("f/r.yml", '2'), ("f/folder", '2'), ("f/.hidden", '2')]);
        let theirs = manifest(&[("f/r.yml", '3'), ("f/folder", '3'), ("f/.hidden", '3')]);
        let plan = plan_sync(&base, &mine, &theirs);
        let decisions = [
            decide("f/.hidden", Resolution::KeepBoth),
            decide("f/folder", Resolution::KeepBoth),
            decide("f/r.yml", Resolution::KeepBoth),
        ];
        let actions = resolve(&plan, &decisions, &taken_of(&mine, &theirs)).expect("resolves");
        let names: Vec<&str> = actions
            .iter()
            .filter_map(|a| match a {
                SyncAction::PullAs { as_path, .. } => Some(as_path.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            names,
            [
                "f/.hidden (from peer)",
                "f/folder (from peer)",
                "f/r (from peer).yml"
            ]
        );
    }

    #[test]
    fn two_keep_both_conflicts_never_pick_the_same_name() {
        let base = manifest(&[("a.yml", '1'), ("a (from peer).yml", '1')]);
        let mine = manifest(&[("a.yml", '2'), ("a (from peer).yml", '2')]);
        let theirs = manifest(&[("a.yml", '3'), ("a (from peer).yml", '3')]);
        let plan = plan_sync(&base, &mine, &theirs);
        let decisions = [
            decide("a (from peer).yml", Resolution::KeepBoth),
            decide("a.yml", Resolution::KeepBoth),
        ];
        let actions = resolve(&plan, &decisions, &taken_of(&mine, &theirs)).expect("resolves");
        let mut names: Vec<String> = actions
            .iter()
            .filter_map(|a| match a {
                SyncAction::PullAs { as_path, .. } => Some(as_path.clone()),
                _ => None,
            })
            .collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 2);
        assert!(names
            .iter()
            .all(|n| n != "a.yml" && n != "a (from peer).yml"));
    }

    #[test]
    fn resolve_keeps_the_plain_actions_and_drops_the_rename_marker() {
        let base = manifest(&[("a.yml", '1'), ("c.yml", '1')]);
        let mine = manifest(&[("a.yml", '2'), ("c.yml", '1'), ("n.yml", '5')]);
        let theirs = manifest(&[("b.yml", '1'), ("c.yml", '2')]);
        let plan = plan_sync(&base, &mine, &theirs);
        let actions = resolve(
            &plan,
            &[decide("a.yml", Resolution::KeepTheirs)],
            &taken_of(&mine, &theirs),
        )
        .expect("resolves");
        assert_eq!(
            actions,
            vec![
                pull("c.yml"),
                push("n.yml"),
                delete_mine("a.yml"),
                pull("b.yml")
            ]
        );
    }

    #[test]
    fn rename_against_edit_resolves_each_way() {
        let base = manifest(&[("a.yml", '1')]);
        let taken = Manifest::new();
        // The peer renamed, this side edited.
        let mine = manifest(&[("a.yml", '2')]);
        let theirs = manifest(&[("b.yml", '1')]);
        let plan = plan_sync(&base, &mine, &theirs);
        let go = |r| resolve(&plan, &[decide("a.yml", r)], &taken).expect("resolves");
        assert_eq!(
            go(Resolution::KeepMine),
            vec![push("a.yml"), delete_theirs("b.yml")]
        );
        assert_eq!(
            go(Resolution::KeepTheirs),
            vec![delete_mine("a.yml"), pull("b.yml")]
        );
        assert_eq!(go(Resolution::KeepBoth), vec![push("a.yml"), pull("b.yml")]);
        // This side renamed, the peer edited.
        let plan = plan_sync(&base, &theirs, &mine);
        let go = |r| resolve(&plan, &[decide("a.yml", r)], &taken).expect("resolves");
        assert_eq!(
            go(Resolution::KeepMine),
            vec![delete_theirs("a.yml"), push("b.yml")]
        );
        assert_eq!(
            go(Resolution::KeepTheirs),
            vec![pull("a.yml"), delete_mine("b.yml")]
        );
        assert_eq!(go(Resolution::KeepBoth), vec![pull("a.yml"), push("b.yml")]);
    }

    #[test]
    fn resolve_refuses_a_rename_conflict_without_its_rename_marker() {
        let plan = SyncPlan {
            actions: Vec::new(),
            conflicts: vec![conflict(
                "a.yml",
                ConflictKind::RenameVsEdit,
                Some('2'),
                Some('1'),
            )],
        };
        let err = resolve(
            &plan,
            &[decide("a.yml", Resolution::KeepMine)],
            &Manifest::new(),
        )
        .expect_err("malformed plan");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    // ----- new_base -----

    /// Applies final actions to both sides the way the nodes do: renames, then deletes, then copies.
    fn simulate(
        mine: &Manifest,
        theirs: &Manifest,
        actions: &[SyncAction],
    ) -> (Manifest, Manifest) {
        let to_map = |m: &Manifest| -> BTreeMap<String, String> {
            m.paths()
                .map(|p| (p.to_string(), m.get(p).expect("listed path").to_string()))
                .collect()
        };
        let (mut a, mut b) = (to_map(mine), to_map(theirs));
        for action in actions {
            match action {
                SyncAction::RenameMine { from, to } => {
                    if let Some(hash) = a.remove(from) {
                        a.insert(to.clone(), hash);
                    }
                }
                SyncAction::RenameTheirs { from, to } => {
                    if let Some(hash) = b.remove(from) {
                        b.insert(to.clone(), hash);
                    }
                }
                _ => {}
            }
        }
        for action in actions {
            match action {
                SyncAction::DeleteMine { path } => {
                    a.remove(path);
                }
                SyncAction::DeleteTheirs { path } => {
                    b.remove(path);
                }
                _ => {}
            }
        }
        for action in actions {
            match action {
                SyncAction::Pull { path } => {
                    a.insert(
                        path.clone(),
                        theirs.get(path).expect("theirs has it").to_string(),
                    );
                }
                SyncAction::PullAs { path, as_path } => {
                    a.insert(
                        as_path.clone(),
                        theirs.get(path).expect("theirs has it").to_string(),
                    );
                }
                SyncAction::Push { path } => {
                    b.insert(
                        path.clone(),
                        mine.get(path).expect("mine has it").to_string(),
                    );
                }
                _ => {}
            }
        }
        (Manifest::from_entries(a), Manifest::from_entries(b))
    }

    fn assert_converges(
        base: &Manifest,
        mine: &Manifest,
        theirs: &Manifest,
        decisions: &[Decision],
    ) -> Manifest {
        let plan = plan_sync(base, mine, theirs);
        let actions = resolve(&plan, decisions, &taken_of(mine, theirs)).expect("resolves");
        let (after_mine, after_theirs) = simulate(mine, theirs, &actions);
        assert_eq!(
            after_mine, after_theirs,
            "both sides end equal for {actions:?}"
        );
        let stored = new_base(base, mine, theirs, &actions, decisions);
        assert_eq!(stored, after_mine, "the stored base is the agreed state");
        stored
    }

    #[test]
    fn new_base_is_the_agreed_state_for_a_clean_two_way_sync() {
        let base = manifest(&[
            ("a.yml", '1'),
            ("b.yml", '1'),
            ("c.yml", '1'),
            ("d.yml", '1'),
            ("e.yml", '1'),
        ]);
        let mine = manifest(&[
            ("a.yml", '2'),
            ("b.yml", '1'),
            ("c.yml", '1'),
            ("e.yml", '1'),
            ("new-here.yml", '6'),
        ]);
        let theirs = manifest(&[
            ("a.yml", '1'),
            ("b.yml", '3'),
            ("c.yml", '1'),
            ("d.yml", '1'),
            ("moved/e.yml", '1'),
            ("new-there.yml", '7'),
        ]);
        let stored = assert_converges(&base, &mine, &theirs, &[]);
        let expected = manifest(&[
            ("a.yml", '2'),
            ("b.yml", '3'),
            ("c.yml", '1'),
            ("moved/e.yml", '1'),
            ("new-here.yml", '6'),
            ("new-there.yml", '7'),
        ]);
        assert_eq!(stored, expected);
    }

    #[test]
    fn new_base_after_keep_mine_keep_theirs_and_keep_both() {
        let base = manifest(&[("r.yml", '1')]);
        let mine = manifest(&[("r.yml", '2')]);
        let theirs = manifest(&[("r.yml", '3')]);
        assert_eq!(
            assert_converges(
                &base,
                &mine,
                &theirs,
                &[decide("r.yml", Resolution::KeepMine)]
            ),
            manifest(&[("r.yml", '2')])
        );
        assert_eq!(
            assert_converges(
                &base,
                &mine,
                &theirs,
                &[decide("r.yml", Resolution::KeepTheirs)]
            ),
            manifest(&[("r.yml", '3')])
        );
        // Keep both: the original path takes mine and the second copy takes theirs.
        assert_eq!(
            assert_converges(
                &base,
                &mine,
                &theirs,
                &[decide("r.yml", Resolution::KeepBoth)]
            ),
            manifest(&[("r.yml", '2'), ("r (from peer).yml", '3')])
        );
    }

    #[test]
    fn new_base_drops_deleted_paths() {
        let base = manifest(&[("a.yml", '1'), ("b.yml", '1')]);
        let mine = manifest(&[("b.yml", '1')]);
        let theirs = manifest(&[("a.yml", '1')]);
        // a.yml was deleted here and b.yml there, both untouched on the other side.
        assert_eq!(
            assert_converges(&base, &mine, &theirs, &[]),
            Manifest::new()
        );
        // A delete against an edit follows the decision.
        let mine = manifest(&[("b.yml", '1')]);
        let theirs = manifest(&[("a.yml", '2'), ("b.yml", '1')]);
        let keep_edit = assert_converges(
            &base,
            &mine,
            &theirs,
            &[decide("a.yml", Resolution::KeepTheirs)],
        );
        assert_eq!(keep_edit, manifest(&[("a.yml", '2'), ("b.yml", '1')]));
        let keep_delete = assert_converges(
            &base,
            &mine,
            &theirs,
            &[decide("a.yml", Resolution::KeepMine)],
        );
        assert_eq!(keep_delete, manifest(&[("b.yml", '1')]));
    }

    #[test]
    fn every_resolution_of_every_conflict_kind_converges() {
        let base = manifest(&[("a.yml", '1')]);
        let scenarios: Vec<(Manifest, Manifest)> = vec![
            (manifest(&[("a.yml", '2')]), manifest(&[("a.yml", '3')])),
            (Manifest::new(), manifest(&[("a.yml", '3')])),
            (manifest(&[("a.yml", '2')]), Manifest::new()),
            (manifest(&[("a.yml", '2')]), manifest(&[("b.yml", '1')])),
            (manifest(&[("b.yml", '1')]), manifest(&[("a.yml", '3')])),
        ];
        for (mine, theirs) in &scenarios {
            for resolution in [
                Resolution::KeepMine,
                Resolution::KeepTheirs,
                Resolution::KeepBoth,
            ] {
                assert_converges(&base, mine, theirs, &[decide("a.yml", resolution)]);
            }
        }
        let added_base = Manifest::new();
        for resolution in [
            Resolution::KeepMine,
            Resolution::KeepTheirs,
            Resolution::KeepBoth,
        ] {
            assert_converges(
                &added_base,
                &manifest(&[("n.yml", '2')]),
                &manifest(&[("n.yml", '3')]),
                &[decide("n.yml", resolution)],
            );
        }
    }
}
```

In `crates/rocket-network/src/lib.rs` add `pub mod planner;` after `pub mod pairing;`.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -j4 -p rocket-network planner::`
Expected: FAIL to compile, `cannot find type SyncPlan in this scope` (and `plan_sync`, `resolve`, `new_base`).

- [ ] **Step 3: Add the crate-private `Manifest` accessors**

In `crates/rocket-network/src/link.rs`, add this block directly above the line `impl Serialize for Manifest {`:

```rust
impl Manifest {
    /// The entries as a map, for the planner.
    pub(crate) fn entries(&self) -> &BTreeMap<String, String> {
        &self.0
    }

    /// Wraps entries the caller took from validated manifests.
    pub(crate) fn from_entries(entries: BTreeMap<String, String>) -> Manifest {
        Manifest(entries)
    }
}
```

- [ ] **Step 4: Implement `planner.rs`**

Insert this code at the top of `crates/rocket-network/src/planner.rs`, above the `#[cfg(test)]` line:

```rust
//! The three-way planner. Pure: no I/O, no clock.

use std::collections::{BTreeMap, BTreeSet};

use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

use crate::link::{path_is_valid, Manifest};

/// What the planning node does. "Mine" is the planning node, "theirs" the other one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncAction {
    Pull { path: String },
    PullAs { path: String, as_path: String },
    Push { path: String },
    DeleteMine { path: String },
    DeleteTheirs { path: String },
    RenameMine { from: String, to: String },
    RenameTheirs { from: String, to: String },
}

/// Why a path needs a decision from the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    BothChanged,
    DeleteVsEdit,
    BothAdded,
    RenameVsEdit,
}

/// A path both sides changed in different ways. Hashes are `None` when the file is absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub path: String,
    pub kind: ConflictKind,
    pub mine: Option<String>,
    pub theirs: Option<String>,
}

/// What the user chose for one conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    KeepMine,
    KeepTheirs,
    KeepBoth,
}

/// The choice for the conflict at `path`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub path: String,
    pub resolution: Resolution,
}

/// The result of the three-way compare: changes that apply on their own, and conflicts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SyncPlan {
    pub actions: Vec<SyncAction>,
    pub conflicts: Vec<Conflict>,
}

type Map = BTreeMap<String, String>;

fn renames_on(base: &Map, side: &Map) -> Vec<(String, String)> {
    let added: Vec<&String> = side.keys().filter(|p| !base.contains_key(*p)).collect();
    let mut used: BTreeSet<&String> = BTreeSet::new();
    let mut pairs = Vec::new();
    for (from, hash) in base.iter().filter(|(p, _)| !side.contains_key(*p)) {
        let target = added
            .iter()
            .find(|q| !used.contains(**q) && side.get(**q) == Some(hash));
        if let Some(to) = target {
            used.insert(*to);
            pairs.push((from.clone(), (*to).clone()));
        }
    }
    pairs
}

/// Plans a sync from the planning node's view: `mine` is its manifest, `theirs` the starter's.
///
/// Renames are paired first, then each remaining path goes through the three-way table. The
/// result is sorted by path. A `RenameMine` or `RenameTheirs` whose `from` is the path of a
/// `RenameVsEdit` conflict is a marker only: `resolve` replaces it.
pub fn plan_sync(base: &Manifest, mine: &Manifest, theirs: &Manifest) -> SyncPlan {
    let (b, m, t) = (base.entries(), mine.entries(), theirs.entries());
    let mut items: Vec<(String, SyncAction)> = Vec::new();
    let mut conflicts: Vec<Conflict> = Vec::new();
    let mut consumed: BTreeSet<String> = BTreeSet::new();

    for theirs_renamed in [true, false] {
        let (renamer, other) = if theirs_renamed { (t, m) } else { (m, t) };
        for (from, to) in renames_on(b, renamer) {
            if other.contains_key(&to) {
                continue;
            }
            let Some(other_hash) = other.get(&from) else {
                continue;
            };
            consumed.insert(from.clone());
            consumed.insert(to.clone());
            let action = if theirs_renamed {
                SyncAction::RenameMine {
                    from: from.clone(),
                    to: to.clone(),
                }
            } else {
                SyncAction::RenameTheirs {
                    from: from.clone(),
                    to: to.clone(),
                }
            };
            if b.get(&from) != Some(other_hash) {
                let (mine_hash, theirs_hash) = if theirs_renamed {
                    (Some(other_hash.clone()), renamer.get(&to).cloned())
                } else {
                    (renamer.get(&to).cloned(), Some(other_hash.clone()))
                };
                conflicts.push(Conflict {
                    path: from.clone(),
                    kind: ConflictKind::RenameVsEdit,
                    mine: mine_hash,
                    theirs: theirs_hash,
                });
            }
            items.push((from, action));
        }
    }

    let paths: BTreeSet<&String> = b.keys().chain(m.keys()).chain(t.keys()).collect();
    for path in paths {
        if consumed.contains(path) {
            continue;
        }
        let (bh, mh, th) = (b.get(path), m.get(path), t.get(path));
        if mh == th {
            continue;
        }
        let p = path.clone();
        if bh == mh {
            items.push((
                p.clone(),
                if th.is_some() {
                    SyncAction::Pull { path: p }
                } else {
                    SyncAction::DeleteMine { path: p }
                },
            ));
        } else if bh == th {
            items.push((
                p.clone(),
                if mh.is_some() {
                    SyncAction::Push { path: p }
                } else {
                    SyncAction::DeleteTheirs { path: p }
                },
            ));
        } else {
            let kind = if bh.is_none() {
                ConflictKind::BothAdded
            } else if mh.is_none() || th.is_none() {
                ConflictKind::DeleteVsEdit
            } else {
                ConflictKind::BothChanged
            };
            conflicts.push(Conflict {
                path: p,
                kind,
                mine: mh.cloned(),
                theirs: th.cloned(),
            });
        }
    }
    items.sort_by(|a, b| a.0.cmp(&b.0));
    conflicts.sort_by(|a, b| a.path.cmp(&b.path));
    SyncPlan {
        actions: items.into_iter().map(|(_, a)| a).collect(),
        conflicts,
    }
}

fn keep_both_path(path: &str, taken: &Manifest, chosen: &BTreeSet<String>) -> DomainResult<String> {
    let (dir, name) = match path.rfind('/') {
        Some(i) => path.split_at(i + 1),
        None => ("", path),
    };
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => name.split_at(i),
        _ => (name, ""),
    };
    let mut n = 1u32;
    loop {
        let suffix = if n == 1 {
            String::new()
        } else {
            format!(" {n}")
        };
        let candidate = format!("{dir}{stem} (from peer){suffix}{ext}");
        if !taken.entries().contains_key(&candidate) && !chosen.contains(&candidate) {
            return if path_is_valid(&candidate) {
                Ok(candidate)
            } else {
                Err(DomainError::InvalidInput(
                    "keep both path is not valid".into(),
                ))
            };
        }
        n += 1;
    }
}

fn pull_or_delete(has: bool, path: &str) -> SyncAction {
    if has {
        SyncAction::Pull { path: path.into() }
    } else {
        SyncAction::DeleteMine { path: path.into() }
    }
}
fn push_or_delete(has: bool, path: &str) -> SyncAction {
    if has {
        SyncAction::Push { path: path.into() }
    } else {
        SyncAction::DeleteTheirs { path: path.into() }
    }
}

/// Turns a plan and the user's decisions into the final actions.
///
/// Every conflict needs exactly one decision. `taken` must hold every path present on either
/// side, so a keep-both name never collides with a file on the other node.
pub fn resolve(
    plan: &SyncPlan,
    decisions: &[Decision],
    taken: &Manifest,
) -> DomainResult<Vec<SyncAction>> {
    let mut by_path: BTreeMap<&str, Resolution> = BTreeMap::new();
    for d in decisions {
        if !plan.conflicts.iter().any(|c| c.path == d.path) {
            return Err(DomainError::InvalidInput(
                "decision for unknown conflict".into(),
            ));
        }
        if by_path.insert(d.path.as_str(), d.resolution).is_some() {
            return Err(DomainError::InvalidInput("duplicate decision".into()));
        }
    }
    for c in &plan.conflicts {
        if !by_path.contains_key(c.path.as_str()) {
            return Err(DomainError::InvalidInput("missing decision".into()));
        }
    }
    let rename_conflicts: BTreeSet<&str> = plan
        .conflicts
        .iter()
        .filter(|c| c.kind == ConflictKind::RenameVsEdit)
        .map(|c| c.path.as_str())
        .collect();
    let is_provisional = |a: &SyncAction| matches!(a, SyncAction::RenameMine { from, .. } | SyncAction::RenameTheirs { from, .. } if rename_conflicts.contains(from.as_str()));
    let mut out: Vec<SyncAction> = plan
        .actions
        .iter()
        .filter(|a| !is_provisional(a))
        .cloned()
        .collect();
    let mut chosen = BTreeSet::new();
    for c in &plan.conflicts {
        let Some(resolution) = by_path.get(c.path.as_str()).copied() else {
            continue;
        };
        let path = c.path.as_str();
        match c.kind {
            ConflictKind::RenameVsEdit => {
                let renamed = plan.actions.iter().find_map(|a| match a {
                    SyncAction::RenameMine { from, to } if from == path => Some((true, to.clone())),
                    SyncAction::RenameTheirs { from, to } if from == path => {
                        Some((false, to.clone()))
                    }
                    _ => None,
                });
                let Some((theirs_renamed, to)) = renamed else {
                    return Err(DomainError::InvalidInput(
                        "rename conflict without rename".into(),
                    ));
                };
                let q = to.as_str();
                let del_mine = |p: &str| SyncAction::DeleteMine { path: p.into() };
                let del_theirs = |p: &str| SyncAction::DeleteTheirs { path: p.into() };
                let pull = |p: &str| SyncAction::Pull { path: p.into() };
                let push = |p: &str| SyncAction::Push { path: p.into() };
                let actions = match (theirs_renamed, resolution) {
                    (true, Resolution::KeepMine) => vec![push(path), del_theirs(q)],
                    (true, Resolution::KeepTheirs) => vec![del_mine(path), pull(q)],
                    (true, Resolution::KeepBoth) => vec![push(path), pull(q)],
                    (false, Resolution::KeepMine) => vec![del_theirs(path), push(q)],
                    (false, Resolution::KeepTheirs) => vec![pull(path), del_mine(q)],
                    (false, Resolution::KeepBoth) => vec![pull(path), push(q)],
                };
                out.extend(actions);
            }
            _ => match resolution {
                Resolution::KeepMine => out.push(push_or_delete(c.mine.is_some(), path)),
                Resolution::KeepTheirs => out.push(pull_or_delete(c.theirs.is_some(), path)),
                Resolution::KeepBoth => match (c.mine.is_some(), c.theirs.is_some()) {
                    (true, true) => {
                        let as_path = keep_both_path(path, taken, &chosen)?;
                        chosen.insert(as_path.clone());
                        out.push(SyncAction::RenameTheirs {
                            from: path.into(),
                            to: as_path.clone(),
                        });
                        out.push(SyncAction::PullAs {
                            path: path.into(),
                            as_path,
                        });
                        out.push(SyncAction::Push { path: path.into() });
                    }
                    (true, false) => out.push(SyncAction::Push { path: path.into() }),
                    _ => out.push(SyncAction::Pull { path: path.into() }),
                },
            },
        }
    }
    Ok(out)
}

/// The manifest both sides store after a successful apply, seen from the planning node.
///
/// `base` and `decisions` are part of the contract but unused: the final actions already carry
/// every decision, and the agreed state is `mine` with the planning node's own actions applied.
pub fn new_base(
    _base: &Manifest,
    mine: &Manifest,
    theirs: &Manifest,
    actions: &[SyncAction],
    _decisions: &[Decision],
) -> Manifest {
    let mut result: Map = mine.entries().clone();
    for action in actions {
        if let SyncAction::RenameMine { from, to } = action {
            if let Some(hash) = result.remove(from) {
                result.insert(to.clone(), hash);
            }
        }
    }
    for action in actions {
        match action {
            SyncAction::DeleteMine { path } => {
                result.remove(path);
            }
            SyncAction::Pull { path } => {
                if let Some(h) = theirs.entries().get(path) {
                    result.insert(path.clone(), h.clone());
                }
            }
            SyncAction::PullAs { path, as_path } => {
                if let Some(h) = theirs.entries().get(path) {
                    result.insert(as_path.clone(), h.clone());
                }
            }
            _ => {}
        }
    }
    Manifest::from_entries(result)
}
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -j4 -p rocket-network planner::`
Expected: PASS, 38 tests.

- [ ] **Step 6: Write the failing plan payload tests**

In the `tests` module of `crates/rocket-network/src/link.rs`, add this import next to `use super::*;`:

```rust
    use crate::planner::Resolution;
```

and these two tests at the end of the module:

```rust
    #[test]
    fn the_plan_payload_round_trips_with_its_pinned_shape() {
        let payload = SyncPlanPayload {
            wanted: vec!["a.yml".into()],
            deletes: vec!["b.yml".into()],
            renames: vec![RenamePair {
                from: "c.yml".into(),
                to: "d/c.yml".into(),
            }],
            decisions: vec![Decision {
                path: "e.yml".into(),
                resolution: Resolution::KeepBoth,
            }],
        };
        let text = String::from_utf8(payload.to_json().expect("encodes")).expect("utf8");
        assert_eq!(
            text,
            r#"{"wanted":["a.yml"],"deletes":["b.yml"],"renames":[{"from":"c.yml","to":"d/c.yml"}],"decisions":[{"path":"e.yml","resolution":"keep_both"}]}"#
        );
        assert_eq!(
            SyncPlanPayload::from_json(text.as_bytes()).expect("decodes"),
            payload
        );
        assert_eq!(
            SyncPlanPayload::from_json(&SyncPlanPayload::default().to_json().expect("encodes"))
                .expect("decodes"),
            SyncPlanPayload::default()
        );
    }

    #[test]
    fn the_plan_payload_rejects_unsafe_paths_unknown_fields_and_too_many_entries() {
        for text in [
            r#"{"wanted":["../a.yml"],"deletes":[],"renames":[],"decisions":[]}"#.to_string(),
            r#"{"wanted":[],"deletes":["/a.yml"],"renames":[],"decisions":[]}"#.to_string(),
            r#"{"wanted":[],"deletes":[],"renames":[{"from":"a.yml","to":".rocket/a.yml"}],"decisions":[]}"#.to_string(),
            r#"{"wanted":[],"deletes":[],"renames":[],"decisions":[{"path":"nul","resolution":"keep_mine"}]}"#.to_string(),
            r#"{"wanted":[],"deletes":[],"renames":[],"decisions":[{"path":"a.yml","resolution":"keep_all"}]}"#.to_string(),
            r#"{"wanted":[],"deletes":[],"renames":[],"decisions":[],"extra":1}"#.to_string(),
            r#"{"wanted":[]}"#.to_string(),
            "nope".to_string(),
            format!(r#"{{"wanted":[{}],"deletes":[],"renames":[],"decisions":[]}}"#, vec!["\"a.yml\""; MAX_MANIFEST_ENTRIES + 1].join(",")),
        ] {
            let err = SyncPlanPayload::from_json(text.as_bytes()).expect_err("rejected");
            assert!(matches!(err, DomainError::InvalidInput(_)), "input {text:.80}");
        }
    }
```

Run: `cargo test -j4 -p rocket-network link::`
Expected: FAIL to compile, `cannot find type SyncPlanPayload in this scope` (and `RenamePair`).

- [ ] **Step 7: Implement the plan payload**

In `crates/rocket-network/src/link.rs` add `use crate::planner::Decision;` to the `use crate::...` block, and add this code after `impl LinkRecord { ... }`, directly above the line `#[cfg(test)]`. The payload is the JSON of the Plan transfer: `{"wanted":[..],"deletes":[..],"renames":[{"from":..,"to":..}],"decisions":[{"path":..,"resolution":..}]}`. `from_json` bounds the size and every list, and checks every path with the manifest rule.

```rust
/// One file move in a `SyncPlanPayload`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenamePair {
    pub from: String,
    pub to: String,
}

/// The plan the planning node sends to the starting node as a transfer payload.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncPlanPayload {
    pub wanted: Vec<String>,
    pub deletes: Vec<String>,
    pub renames: Vec<RenamePair>,
    pub decisions: Vec<Decision>,
}

impl SyncPlanPayload {
    pub fn to_json(&self) -> DomainResult<Vec<u8>> {
        serde_json::to_vec(self)
            .map_err(|_| DomainError::InvalidInput("plan cannot be encoded".into()))
    }
    pub fn from_json(bytes: &[u8]) -> DomainResult<SyncPlanPayload> {
        if bytes.len() as u64 > MAX_TRANSFER_BYTES {
            return Err(DomainError::InvalidInput("plan too large".into()));
        }
        let payload: SyncPlanPayload = serde_json::from_slice(bytes)
            .map_err(|_| DomainError::InvalidInput("invalid plan".into()))?;
        let lists = [
            payload.wanted.len(),
            payload.deletes.len(),
            payload.renames.len(),
            payload.decisions.len(),
        ];
        if lists.iter().any(|n| *n > MAX_MANIFEST_ENTRIES) {
            return Err(DomainError::InvalidInput(
                "plan has too many entries".into(),
            ));
        }
        let paths = payload
            .wanted
            .iter()
            .chain(&payload.deletes)
            .map(String::as_str)
            .chain(
                payload
                    .renames
                    .iter()
                    .flat_map(|r| [r.from.as_str(), r.to.as_str()]),
            )
            .chain(payload.decisions.iter().map(|d| d.path.as_str()));
        for path in paths {
            if !path_is_valid(path) {
                return Err(DomainError::InvalidInput("invalid plan path".into()));
            }
        }
        Ok(payload)
    }
}
```

In `crates/rocket-network/src/lib.rs` replace the `pub use link::{ ... };` line of Task 1 with:

```rust
pub use link::{
    is_sha256_hex, path_is_valid, LinkId, LinkRecord, LinkState, Manifest, RenamePair, SyncId,
    SyncPlanPayload,
};
```

and add this after the `pub use pairing::{ ... };` block:

```rust
pub use planner::{
    new_base, plan_sync, resolve, Conflict, ConflictKind, Decision, Resolution, SyncAction,
    SyncPlan,
};
```

- [ ] **Step 8: Run to verify it passes**

Run: `cargo test -j4 -p rocket-network link::`
Expected: PASS, 23 tests.

Run: `cargo test -j4 -p rocket-network planner::`
Expected: PASS, 38 tests.

Run: `cargo clippy -j4 -p rocket-network --all-targets`
Expected: no warnings.

---

## Task 3: Sync frames, version 3, SyncEvent, the link port and the audit kinds

**Files:**
- Modify: `crates/rocket-network/src/messages.rs`
- Modify: `crates/rocket-network/src/transfer.rs`
- Modify: `crates/rocket-network/src/events.rs`
- Modify: `crates/rocket-network/src/ports.rs`
- Modify: `crates/rocket-network/src/lib.rs`
- Modify: `crates/rocket-network/CLAUDE.md`
- Modify: `crates/rocket-audit/src/event.rs`
- Modify: `crates/rocket-audit/src/control.rs`
- Modify: `crates/rocket-app/src/network/{connections,transfer,transfer_fakes,transfer_receive_tests,transfer_link_tests,transfer_send_tests,connections_tests}.rs`

**Interfaces:**
- Consumes: `ConflictKind`, `LinkId`, `LinkRecord`, `Manifest`, `offer_name_is_valid`, `SYNC_PAYLOAD_NAME_MAX`.
- Produces: `SyncPayloadKind` (with `transfer_name`), `SyncAbortReason`, `SyncOutcome`, `PeerMessage::{SyncOffer, SyncAbort, SyncResult}`, `keep_in_sync` and `link_id` on `PeerMessage::CollectionOffer`, `SyncEvent`, `SyncSummary`, `ConflictView`, `SyncRole`, `SyncFinish`, `LinkRepository`, `AuditEventKind::{NetworkSyncSent, NetworkSyncReceived, NetworkLinkRemoved}`.

- [ ] **Step 1: Write the failing message tests**

In `crates/rocket-network/src/messages.rs`, first move the version fixtures to 3:

```bash
sed -i 's/"version":2/"version":3/g' crates/rocket-network/src/messages.rs
```

Then edit by hand: in `hello()` change `version: 2,` to `version: 3,`, and in `encoded_frames_have_the_expected_shape` change `"version": 2,` to `"version": 3,`.
Expected: `grep -nE 'version": ?2|version: 2' crates/rocket-network/src/messages.rs` prints nothing.

Give the two existing `CollectionOffer` literals of the test module the new fields. In `transfer_messages()` replace

```rust
                chunk_count: 3,
                sha256: SHA.into(),
            },
            PeerMessage::OfferAnswer {
                transfer_id: ID.into(),
                accept: true,
            },
```

with

```rust
                chunk_count: 3,
                sha256: SHA.into(),
                keep_in_sync: false,
                link_id: None,
            },
            PeerMessage::OfferAnswer {
                transfer_id: ID.into(),
                accept: true,
            },
```

and in `a_name_cannot_push_an_offer_past_the_frame_limit` replace

```rust
            name: "n".repeat(MAX_FRAME_BYTES),
            size_bytes: 1,
            chunk_count: 1,
            sha256: SHA.into(),
```

with the same four lines plus `keep_in_sync: false,` and `link_id: None,`.

Extend the test imports at the top of `mod tests`: add `use crate::transfer::offer_name_is_valid;` and change `use crate::limits::{CHUNK_BYTES, DISPLAY_NAME_MAX_CHARS};` to `use crate::limits::{CHUNK_BYTES, DISPLAY_NAME_MAX_CHARS, SYNC_PAYLOAD_NAME_MAX};`. Add two constants after `const SHA: &str = ...;`:

```rust
    const SYNC_ID: &str = "fedcba9876543210fedcba9876543210";
    const LINK_ID: &str = "00112233445566778899aabbccddeeff";
```

Add the fixture after `transfer_messages()`:

```rust
    fn sync_messages() -> Vec<PeerMessage> {
        vec![
            PeerMessage::SyncOffer {
                sync_id: SYNC_ID.into(),
                link_id: LINK_ID.into(),
                base_hash: SHA.into(),
                kind: SyncPayloadKind::Manifest,
                transfer_id: ID.into(),
                size_bytes: 70_000,
                chunk_count: 3,
                sha256: SHA.into(),
            },
            PeerMessage::SyncAbort {
                sync_id: SYNC_ID.into(),
                reason: SyncAbortReason::BaseMismatch,
            },
            PeerMessage::SyncResult {
                sync_id: SYNC_ID.into(),
                outcome: SyncOutcome::Applied,
            },
        ]
    }
```

In `every_variant_round_trips`, replace the line `        all.extend(transfer_messages());` with:

```rust
        all.extend(transfer_messages());
        all.extend(sync_messages());
        for kind in [
            SyncPayloadKind::Manifest,
            SyncPayloadKind::Plan,
            SyncPayloadKind::Files,
        ] {
            all.push(PeerMessage::SyncOffer {
                sync_id: SYNC_ID.into(),
                link_id: LINK_ID.into(),
                base_hash: SHA.into(),
                kind,
                transfer_id: ID.into(),
                size_bytes: 1,
                chunk_count: 1,
                sha256: SHA.into(),
            });
        }
        for reason in [
            SyncAbortReason::BaseMismatch,
            SyncAbortReason::LinkMissing,
            SyncAbortReason::Busy,
            SyncAbortReason::Cancelled,
            SyncAbortReason::Declined,
            SyncAbortReason::Expired,
            SyncAbortReason::Failed,
            SyncAbortReason::TooBig,
        ] {
            all.push(PeerMessage::SyncAbort {
                sync_id: SYNC_ID.into(),
                reason,
            });
        }
        for outcome in [SyncOutcome::Applied, SyncOutcome::Failed] {
            all.push(PeerMessage::SyncResult {
                sync_id: SYNC_ID.into(),
                outcome,
            });
        }
        all.push(PeerMessage::CollectionOffer {
            transfer_id: ID.into(),
            name: "Orders".into(),
            size_bytes: 70_000,
            chunk_count: 3,
            sha256: SHA.into(),
            keep_in_sync: true,
            link_id: Some(LINK_ID.into()),
        });
```

Add these tests at the end of `mod tests` (before its closing brace):

```rust
    #[test]
    fn sync_frames_have_the_expected_shape() {
        let messages = sync_messages();
        assert_eq!(
            messages[0].encode(),
            format!(
                r#"{{"type":"sync_offer","sync_id":"{SYNC_ID}","link_id":"{LINK_ID}","base_hash":"{SHA}","kind":"manifest","transfer_id":"{ID}","size_bytes":70000,"chunk_count":3,"sha256":"{SHA}"}}"#
            )
        );
        assert_eq!(
            messages[1].encode(),
            format!(r#"{{"type":"sync_abort","sync_id":"{SYNC_ID}","reason":"base_mismatch"}}"#)
        );
        assert_eq!(
            messages[2].encode(),
            format!(r#"{{"type":"sync_result","sync_id":"{SYNC_ID}","outcome":"applied"}}"#)
        );
    }

    #[test]
    fn sync_enums_use_snake_case_names() {
        for (kind, name) in [
            (SyncPayloadKind::Manifest, "manifest"),
            (SyncPayloadKind::Plan, "plan"),
            (SyncPayloadKind::Files, "files"),
        ] {
            assert_eq!(serde_json::to_value(kind).expect("json"), json!(name));
        }
        for (reason, name) in [
            (SyncAbortReason::BaseMismatch, "base_mismatch"),
            (SyncAbortReason::LinkMissing, "link_missing"),
            (SyncAbortReason::Busy, "busy"),
            (SyncAbortReason::Cancelled, "cancelled"),
            (SyncAbortReason::Declined, "declined"),
            (SyncAbortReason::Expired, "expired"),
            (SyncAbortReason::Failed, "failed"),
            (SyncAbortReason::TooBig, "too_big"),
        ] {
            assert_eq!(serde_json::to_value(reason).expect("json"), json!(name));
        }
        for (outcome, name) in [
            (SyncOutcome::Applied, "applied"),
            (SyncOutcome::Failed, "failed"),
        ] {
            assert_eq!(serde_json::to_value(outcome).expect("json"), json!(name));
        }
    }

    #[test]
    fn sync_frames_reject_unknown_fields_and_bad_values() {
        let (sid, lid) = (SYNC_ID, LINK_ID);
        let offer = |tail: &str| {
            format!(
                r#"{{"type":"sync_offer","sync_id":"{sid}","link_id":"{lid}","base_hash":"{SHA}","kind":"manifest","transfer_id":"{ID}",{tail}}}"#
            )
        };
        for text in [
            // Unknown fields.
            offer(&format!(r#""size_bytes":1,"chunk_count":1,"sha256":"{SHA}","extra":1"#)),
            format!(r#"{{"type":"sync_abort","sync_id":"{sid}","reason":"failed","extra":1}}"#),
            format!(r#"{{"type":"sync_result","sync_id":"{sid}","outcome":"applied","extra":1}}"#),
            // Missing fields.
            offer(r#""size_bytes":1,"chunk_count":1"#),
            format!(r#"{{"type":"sync_abort","sync_id":"{sid}"}}"#),
            r#"{"type":"sync_result","outcome":"applied"}"#.to_string(),
            // Wrong values.
            offer(&format!(r#""size_bytes":-1,"chunk_count":1,"sha256":"{SHA}""#)),
            offer(&format!(r#""size_bytes":1,"chunk_count":4294967296,"sha256":"{SHA}""#)),
            offer(&format!(r#""size_bytes":1,"chunk_count":1,"sha256":"{SHA}""#))
                .replace(r#""kind":"manifest""#, r#""kind":"bogus""#),
            format!(r#"{{"type":"sync_abort","sync_id":"{sid}","reason":"bogus"}}"#),
            format!(r#"{{"type":"sync_result","sync_id":"{sid}","outcome":"completed"}}"#),
            format!(r#"{{"type":"sync_result","sync_id":"{sid}","outcome":"damaged"}}"#),
        ] {
            let err = PeerMessage::decode(&text).expect_err("rejected");
            assert!(matches!(err, DomainError::InvalidInput(_)), "input {text}");
        }
    }

    #[test]
    fn a_version_2_offer_without_sync_fields_still_decodes() {
        let text = format!(
            r#"{{"type":"collection_offer","transfer_id":"{ID}","name":"Orders","size_bytes":70000,"chunk_count":3,"sha256":"{SHA}"}}"#
        );
        match PeerMessage::decode(&text).expect("decodes") {
            PeerMessage::CollectionOffer {
                keep_in_sync,
                link_id,
                ..
            } => {
                assert!(!keep_in_sync);
                assert_eq!(link_id, None);
            }
            other => panic!("expected an offer, got {other:?}"),
        }
        // A null link id reads as none too.
        let with_null = text.replace(r#""sha256":"#, r#""link_id":null,"sha256":"#);
        assert!(PeerMessage::decode(&with_null).is_ok());
    }

    #[test]
    fn an_offer_for_a_link_carries_both_fields() {
        let offer = PeerMessage::CollectionOffer {
            transfer_id: ID.into(),
            name: "Orders".into(),
            size_bytes: 70_000,
            chunk_count: 3,
            sha256: SHA.into(),
            keep_in_sync: true,
            link_id: Some(LINK_ID.into()),
        };
        assert_eq!(
            offer.encode(),
            format!(
                r#"{{"type":"collection_offer","transfer_id":"{ID}","name":"Orders","size_bytes":70000,"chunk_count":3,"sha256":"{SHA}","keep_in_sync":true,"link_id":"{LINK_ID}"}}"#
            )
        );
        assert_eq!(PeerMessage::decode(&offer.encode()).expect("decodes"), offer);
    }

    #[test]
    fn an_offer_without_a_link_encodes_as_in_version_2() {
        let text = transfer_messages()[0].encode();
        assert!(!text.contains("keep_in_sync"));
        assert!(!text.contains("link_id"));
    }

    #[test]
    fn an_offer_with_a_bad_keep_in_sync_is_rejected() {
        for bad in [r#""keep_in_sync":"yes""#, r#""keep_in_sync":1"#, r#""link_id":5"#] {
            let text = format!(
                r#"{{"type":"collection_offer","transfer_id":"{ID}","name":"A","size_bytes":1,"chunk_count":1,"sha256":"{SHA}",{bad}}}"#
            );
            assert!(PeerMessage::decode(&text).is_err(), "input {bad}");
        }
    }

    #[test]
    fn sync_payload_names_fit_the_offer_name_rule() {
        for (kind, name) in [
            (SyncPayloadKind::Manifest, "manifest"),
            (SyncPayloadKind::Plan, "plan"),
            (SyncPayloadKind::Files, "files"),
        ] {
            assert_eq!(kind.transfer_name(), name);
            assert!(name.chars().count() <= SYNC_PAYLOAD_NAME_MAX);
            assert!(offer_name_is_valid(name));
        }
    }

    #[test]
    fn every_sync_frame_fits_the_frame_limit_with_room_to_spare() {
        let widest = PeerMessage::SyncOffer {
            sync_id: SYNC_ID.into(),
            link_id: LINK_ID.into(),
            base_hash: SHA.into(),
            kind: SyncPayloadKind::Manifest,
            transfer_id: ID.into(),
            size_bytes: u64::MAX,
            chunk_count: u32::MAX,
            sha256: SHA.into(),
        };
        let text = widest.encode();
        assert!(text.len() < 512, "a sync offer is a small frame");
        assert_eq!(PeerMessage::decode(&text).expect("decodes"), widest);
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -j4 -p rocket-network messages::`
Expected: FAIL to compile, `cannot find type SyncPayloadKind in this scope` and `struct variant CollectionOffer has no field named keep_in_sync`.

- [ ] **Step 3: Add the frames and the two offer fields**

In `crates/rocket-network/src/messages.rs`, replace the line `/// A text frame on the peer WebSocket. Later projects add variants.` with the three enums followed by that line:

```rust
/// What a sync transfer carries. The offer's `kind` tells the receiver how to treat the bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncPayloadKind {
    /// JSON of a `Manifest`.
    Manifest,
    /// JSON of a `SyncPlanPayload`.
    Plan,
    /// A ZIP of collection files.
    Files,
}

impl SyncPayloadKind {
    /// The name the transfer machines carry in the offer. At most `SYNC_PAYLOAD_NAME_MAX`.
    pub fn transfer_name(self) -> &'static str {
        match self {
            SyncPayloadKind::Manifest => "manifest",
            SyncPayloadKind::Plan => "plan",
            SyncPayloadKind::Files => "files",
        }
    }
}

/// Why a sync stopped. Sent inside `SyncAbort`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncAbortReason {
    BaseMismatch,
    LinkMissing,
    Busy,
    Cancelled,
    Declined,
    Expired,
    Failed,
    TooBig,
}

/// What a node reports once it has applied, or failed to apply, a sync. Sent inside `SyncResult`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncOutcome {
    Applied,
    Failed,
}

/// Serde helper: skips `keep_in_sync` when it is false, so an offer without a link looks the
/// same as in protocol version 2.
fn is_false(value: &bool) -> bool {
    !*value
}

/// A text frame on the peer WebSocket. Later projects add variants.
```

Replace the `CollectionOffer` variant

```rust
        chunk_count: u32,
        sha256: String,
    },
    /// The receiver accepts or declines an offer.
```

with

```rust
        chunk_count: u32,
        sha256: String,
        /// The sender wants a link, so the two copies can be synced later.
        #[serde(default, skip_serializing_if = "is_false")]
        keep_in_sync: bool,
        /// The link id the sender made, when `keep_in_sync` is true.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        link_id: Option<String>,
    },
    /// The receiver accepts or declines an offer.
```

Replace the `TransferResult` variant

```rust
    /// The receiver reports the true outcome after the last chunk and the import.
    TransferResult {
        transfer_id: String,
        outcome: ResultOutcome,
    },
}
```

with

```rust
    /// The receiver reports the true outcome after the last chunk and the import.
    TransferResult {
        transfer_id: String,
        outcome: ResultOutcome,
    },
    /// A node starts a sync of a linked collection. The payload of `kind` follows as the
    /// transfer `transfer_id` on the transfer machines. `base_hash` is the sender's base.
    SyncOffer {
        sync_id: String,
        link_id: String,
        base_hash: String,
        kind: SyncPayloadKind,
        transfer_id: String,
        size_bytes: u64,
        chunk_count: u32,
        sha256: String,
    },
    /// Either node stops a sync.
    SyncAbort {
        sync_id: String,
        reason: SyncAbortReason,
    },
    /// A node reports whether it applied the sync.
    SyncResult {
        sync_id: String,
        outcome: SyncOutcome,
    },
}
```

- [ ] **Step 4: Fix the offer builders in `rocket-network` and run**

In `crates/rocket-network/src/transfer.rs`, in `OutgoingTransfer::offer_message`, replace

```rust
            chunk_count: self.chunk_count,
            sha256: self.sha256.clone(),
        }
    }
```

with

```rust
            chunk_count: self.chunk_count,
            sha256: self.sha256.clone(),
            keep_in_sync: false,
            link_id: None,
        }
    }
```

and in the test `the_sender_builds_the_offer_frame` replace

```rust
                sha256: SHA.into(),
            }
        );
        assert_eq!(t.state(), OutgoingState::Offered);
```

with

```rust
                sha256: SHA.into(),
                keep_in_sync: false,
                link_id: None,
            }
        );
        assert_eq!(t.state(), OutgoingState::Offered);
```

In `crates/rocket-network/src/lib.rs` replace `pub use messages::{AbortReason, PeerMessage, ResultOutcome};` with:

```rust
pub use messages::{
    AbortReason, PeerMessage, ResultOutcome, SyncAbortReason, SyncOutcome, SyncPayloadKind,
};
```

Run: `cargo test -j4 -p rocket-network messages::`
Expected: PASS (the old tests with version 3 fixtures, the 3 sync frames in the round trip, and the 9 new tests).

Run: `cargo test -j4 -p rocket-network transfer::`
Expected: PASS (unchanged count).

- [ ] **Step 5: Write the failing event tests**

In `crates/rocket-network/src/events.rs` add the tests. Append this block at the end of `mod tests`, replacing the closing lines

```rust
        assert!(event.as_any().downcast_ref::<TransferEvent>().is_some());
        assert!(event.as_any().downcast_ref::<NetworkEvent>().is_none());
    }
}
```

with the same four lines followed by the new tests and the closing brace:

```rust
        assert!(event.as_any().downcast_ref::<TransferEvent>().is_some());
        assert!(event.as_any().downcast_ref::<NetworkEvent>().is_none());
    }

    fn summary() -> SyncSummary {
        SyncSummary {
            added_here: 1,
            changed_here: 2,
            deleted_here: 3,
            sent_added: 4,
            sent_changed: 5,
            sent_deleted: 6,
            scripts_changed: vec!["a/pre.js".into()],
            auth_changed: vec!["b.yml".into()],
        }
    }

    fn sync_cases() -> Vec<(SyncEvent, &'static [&'static str])> {
        vec![
            (
                SyncEvent::ReviewReady {
                    sync_id: "S".into(),
                    link_id: "L".into(),
                    device_id: "ID".into(),
                    short_id: "ABCD-EFGH".into(),
                    display_name: "Ada".into(),
                    collection_name: "Orders".into(),
                    summary: summary(),
                    conflicts: vec![ConflictView {
                        path: "r.yml".into(),
                        kind: ConflictKind::BothChanged,
                        mine_hash: Some("m".into()),
                        theirs_hash: None,
                    }],
                    role: SyncRole::Planner,
                },
                &[
                    "type",
                    "sync_id",
                    "link_id",
                    "device_id",
                    "short_id",
                    "display_name",
                    "collection_name",
                    "summary",
                    "conflicts",
                    "role",
                ][..],
            ),
            (
                SyncEvent::ConfirmNeeded {
                    sync_id: "S".into(),
                    link_id: "L".into(),
                    device_id: "ID".into(),
                    short_id: "ABCD-EFGH".into(),
                    display_name: "Ada".into(),
                    collection_name: "Orders".into(),
                    summary: summary(),
                },
                &[
                    "type",
                    "sync_id",
                    "link_id",
                    "device_id",
                    "short_id",
                    "display_name",
                    "collection_name",
                    "summary",
                ][..],
            ),
            (
                SyncEvent::Progress {
                    sync_id: "S".into(),
                    done_bytes: 10,
                    total_bytes: 20,
                },
                &["type", "sync_id", "done_bytes", "total_bytes"][..],
            ),
            (
                SyncEvent::Finished {
                    sync_id: "S".into(),
                    link_id: "L".into(),
                    display_name: "Ada".into(),
                    collection_name: "Orders".into(),
                    outcome: SyncFinish::BaseMismatch,
                    message: None,
                },
                &[
                    "type",
                    "sync_id",
                    "link_id",
                    "display_name",
                    "collection_name",
                    "outcome",
                    "message",
                ][..],
            ),
            (
                SyncEvent::LinkChanged {
                    link_id: "L".into(),
                },
                &["type", "link_id"][..],
            ),
        ]
    }

    fn value_keys(value: &serde_json::Value) -> Vec<String> {
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
    fn sync_events_use_one_channel() {
        for (event, _) in sync_cases() {
            assert_eq!(event.channel(), "network-sync", "event {event:?}");
            assert!(event.also_emit_on().is_empty());
            assert!(event.written_collections().is_empty());
        }
    }

    // Wire contract: tag is camelCase, fields stay snake_case. A rename here
    // breaks the frontend listeners and needs its own decision.
    #[test]
    fn wire_contract_sync_events() {
        for (event, expected) in sync_cases() {
            let mut want: Vec<String> = expected.iter().map(|k| k.to_string()).collect();
            want.sort();
            let value = event.to_json().expect("serializes");
            assert_eq!(value_keys(&value), want, "event {event:?}");
        }
    }

    #[test]
    fn wire_contract_sync_tags_are_camel_case() {
        let tags: Vec<String> = sync_cases()
            .iter()
            .map(|(event, _)| {
                event.to_json().expect("serializes")["type"]
                    .as_str()
                    .expect("tag")
                    .to_string()
            })
            .collect();
        assert_eq!(
            tags,
            [
                "reviewReady",
                "confirmNeeded",
                "progress",
                "finished",
                "linkChanged"
            ]
        );
    }

    #[test]
    fn wire_contract_sync_envelope_keys_are_free() {
        for (event, _) in sync_cases() {
            let keys = value_keys(&event.to_json().expect("serializes"));
            for reserved in ["seq", "tsMs", "ts_ms", "workspaceId"] {
                assert!(
                    !keys.iter().any(|k| k == reserved),
                    "event {event:?} uses {reserved}"
                );
            }
        }
    }

    #[test]
    fn wire_contract_sync_helper_dtos_are_camel_case() {
        let value = serde_json::to_value(summary()).expect("serializes");
        assert_eq!(
            value_keys(&value),
            [
                "addedHere",
                "authChanged",
                "changedHere",
                "deletedHere",
                "scriptsChanged",
                "sentAdded",
                "sentChanged",
                "sentDeleted"
            ]
        );
        let view = ConflictView {
            path: "r.yml".into(),
            kind: ConflictKind::DeleteVsEdit,
            mine_hash: None,
            theirs_hash: Some("t".into()),
        };
        let value = serde_json::to_value(view).expect("serializes");
        assert_eq!(value_keys(&value), ["kind", "mineHash", "path", "theirsHash"]);
        assert!(value["mineHash"].is_null());
        assert_eq!(value["kind"], "delete_vs_edit");
    }

    #[test]
    fn sync_event_json_carries_the_values() {
        let progress = SyncEvent::Progress {
            sync_id: "S1".into(),
            done_bytes: 10,
            total_bytes: 20,
        };
        assert_eq!(
            progress.to_json().expect("serializes"),
            json!({"type": "progress", "sync_id": "S1", "done_bytes": 10, "total_bytes": 20})
        );
        let finished = SyncEvent::Finished {
            sync_id: "S1".into(),
            link_id: "L1".into(),
            display_name: "Ada".into(),
            collection_name: "Orders".into(),
            outcome: SyncFinish::LinkMissing,
            message: None,
        };
        assert_eq!(
            finished.to_json().expect("serializes"),
            json!({
                "type": "finished",
                "sync_id": "S1",
                "link_id": "L1",
                "display_name": "Ada",
                "collection_name": "Orders",
                "outcome": "link_missing",
                "message": null
            })
        );
        let review = SyncEvent::ReviewReady {
            sync_id: "S1".into(),
            link_id: "L1".into(),
            device_id: "ID".into(),
            short_id: "ABCD-EFGH".into(),
            display_name: "Ada".into(),
            collection_name: "Orders".into(),
            summary: SyncSummary::default(),
            conflicts: Vec::new(),
            role: SyncRole::Starter,
        };
        let value = review.to_json().expect("serializes");
        assert_eq!(value["role"], "starter");
        assert_eq!(value["summary"]["addedHere"], 0);
        assert_eq!(value["summary"]["scriptsChanged"], json!([]));
        assert_eq!(value["conflicts"], json!([]));
    }

    #[test]
    fn sync_enums_serialize_as_snake_case() {
        for (role, name) in [(SyncRole::Planner, "planner"), (SyncRole::Starter, "starter")] {
            assert_eq!(serde_json::to_value(role).expect("json"), json!(name));
        }
        for (finish, name) in [
            (SyncFinish::Applied, "applied"),
            (SyncFinish::Declined, "declined"),
            (SyncFinish::Expired, "expired"),
            (SyncFinish::Cancelled, "cancelled"),
            (SyncFinish::Failed, "failed"),
            (SyncFinish::BaseMismatch, "base_mismatch"),
            (SyncFinish::LinkMissing, "link_missing"),
            (SyncFinish::Unconfirmed, "unconfirmed"),
        ] {
            assert_eq!(serde_json::to_value(finish).expect("json"), json!(name));
        }
        for (kind, name) in [
            (ConflictKind::BothChanged, "both_changed"),
            (ConflictKind::DeleteVsEdit, "delete_vs_edit"),
            (ConflictKind::BothAdded, "both_added"),
            (ConflictKind::RenameVsEdit, "rename_vs_edit"),
        ] {
            assert_eq!(serde_json::to_value(kind).expect("json"), json!(name));
        }
    }

    #[test]
    fn sync_events_can_be_downcast_and_are_not_other_events() {
        let event: Box<dyn PublishedEvent> = Box::new(SyncEvent::LinkChanged {
            link_id: "L".into(),
        });
        assert!(event.as_any().downcast_ref::<SyncEvent>().is_some());
        assert!(event.as_any().downcast_ref::<TransferEvent>().is_none());
        assert!(event.as_any().downcast_ref::<NetworkEvent>().is_none());
    }
}
```

- [ ] **Step 6: Run to verify it fails**

Run: `cargo test -j4 -p rocket-network events::`
Expected: FAIL to compile, `cannot find type SyncEvent in this scope`.

- [ ] **Step 7: Implement `SyncEvent`**

In `crates/rocket-network/src/events.rs` change the first `use` lines to add the planner type:

```rust
use crate::planner::ConflictKind;
use crate::transfer::{Direction, Outcome};
```

and add this code after `impl PublishedEvent for TransferEvent { ... }` (before `#[cfg(test)]`). Also extend the module doc comment's last line to `//! `TransferEvent` and `SyncEvent` follow the same rule on their own channels.`

```rust
/// Counts and lists for the review and confirm dialogs. Counts are files, lists are paths.
///
/// A helper DTO, so its keys are camelCase. "Here" is the node that shows the dialog.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub added_here: u32,
    pub changed_here: u32,
    pub deleted_here: u32,
    pub sent_added: u32,
    pub sent_changed: u32,
    pub sent_deleted: u32,
    /// Changed files that contain scripts.
    pub scripts_changed: Vec<String>,
    /// Changed files that contain auth.
    pub auth_changed: Vec<String>,
}

/// One conflict as the review dialog shows it. A hash is `None` for a deleted or absent file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictView {
    pub path: String,
    pub kind: ConflictKind,
    pub mine_hash: Option<String>,
    pub theirs_hash: Option<String>,
}

/// Which side of a sync this node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncRole {
    /// Received the offer and decides the conflicts.
    Planner,
    /// Pressed Sync.
    Starter,
}

/// How a sync ended, for the dialog and the toast.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncFinish {
    Applied,
    Declined,
    Expired,
    Cancelled,
    Failed,
    BaseMismatch,
    LinkMissing,
    /// This node applied, but the peer's result never came.
    Unconfirmed,
}

/// Events about syncing linked collections. Channel `network-sync`.
///
/// Same wire rule as `NetworkEvent`: camelCase tag, snake_case fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SyncEvent {
    /// The plan is ready and the user decides: accept, decline and each conflict.
    ReviewReady {
        sync_id: String,
        link_id: String,
        device_id: String,
        short_id: String,
        display_name: String,
        collection_name: String,
        summary: SyncSummary,
        conflicts: Vec<ConflictView>,
        role: SyncRole,
    },
    /// The starter confirms before anything is written.
    ConfirmNeeded {
        sync_id: String,
        link_id: String,
        device_id: String,
        short_id: String,
        display_name: String,
        collection_name: String,
        summary: SyncSummary,
    },
    /// Bytes moved. The frontend hides this for a sync that ends in under 300 ms.
    Progress {
        sync_id: String,
        done_bytes: u64,
        total_bytes: u64,
    },
    /// A sync ended. `message` is a fixed sentence or `null`, never file content.
    Finished {
        sync_id: String,
        link_id: String,
        display_name: String,
        collection_name: String,
        outcome: SyncFinish,
        message: Option<String>,
    },
    /// A link was created, removed or changed state. The frontend refetches the list.
    LinkChanged { link_id: String },
}

impl PublishedEvent for SyncEvent {
    fn channel(&self) -> &'static str {
        "network-sync"
    }

    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
```

In `crates/rocket-network/src/lib.rs` replace `pub use events::{NetworkEvent, TransferEvent};` with:

```rust
pub use events::{
    ConflictView, NetworkEvent, SyncEvent, SyncFinish, SyncRole, SyncSummary, TransferEvent,
};
```

Run: `cargo test -j4 -p rocket-network events::`
Expected: PASS (the older tests and the 8 new ones).

- [ ] **Step 8: Write the failing port test**

In `crates/rocket-network/src/ports.rs`, in `every_port_is_object_safe` add after `assert_dyn::<dyn PeerTransportFactory>();`:

```rust
        assert_dyn::<dyn LinkRepository>();
```

and add this test before `struct Collecting(Mutex<Vec<DiscoveryEvent>>);`:

```rust
    type Row = (LinkRecord, Vec<(String, Vec<u8>)>);

    #[derive(Default)]
    struct MemoryLinks(Mutex<Vec<Row>>);

    impl LinkRepository for MemoryLinks {
        fn list(&self) -> DomainResult<Vec<LinkRecord>> {
            let rows = self.0.lock().unwrap_or_else(|e| e.into_inner());
            Ok(rows.iter().map(|(record, _)| record.clone()).collect())
        }

        fn get(&self, id: &LinkId) -> DomainResult<Option<LinkRecord>> {
            Ok(self.list()?.into_iter().find(|r| &r.link_id == id))
        }

        fn save(&self, record: &LinkRecord, base_files: &[(String, Vec<u8>)]) -> DomainResult<()> {
            let mut rows = self.0.lock().unwrap_or_else(|e| e.into_inner());
            rows.retain(|(r, _)| r.link_id != record.link_id);
            rows.push((record.clone(), base_files.to_vec()));
            Ok(())
        }

        fn delete(&self, id: &LinkId) -> DomainResult<()> {
            let mut rows = self.0.lock().unwrap_or_else(|e| e.into_inner());
            rows.retain(|(r, _)| &r.link_id != id);
            Ok(())
        }

        fn read_base_file(&self, id: &LinkId, path: &str) -> DomainResult<Option<Vec<u8>>> {
            let rows = self.0.lock().unwrap_or_else(|e| e.into_inner());
            Ok(rows
                .iter()
                .filter(|(r, _)| &r.link_id == id)
                .flat_map(|(_, files)| files.iter())
                .find(|(p, _)| p == path)
                .map(|(_, bytes)| bytes.clone()))
        }
    }

    #[test]
    fn the_link_repository_is_usable_through_the_trait_object() {
        let repo: Arc<dyn LinkRepository> = Arc::new(MemoryLinks::default());
        let id = LinkId::parse("0123456789abcdef0123456789abcdef").expect("valid id");
        let record = LinkRecord {
            link_id: id.clone(),
            peer_device_id: DeviceId::from_bytes([3; 32]),
            collection_name: "Orders".into(),
            collection_root: "/c/orders".into(),
            collection_uid: None,
            base: Manifest::new(),
            state: LinkState::InSync,
            updated_at: "2026-11-08T10:00:00Z".into(),
        };
        assert_eq!(repo.get(&id).expect("reads"), None);
        repo.save(&record, &[("a.yml".to_string(), b"x".to_vec())])
            .expect("saves");
        assert_eq!(repo.list().expect("lists"), vec![record.clone()]);
        assert_eq!(repo.get(&id).expect("reads"), Some(record));
        assert_eq!(
            repo.read_base_file(&id, "a.yml").expect("reads"),
            Some(b"x".to_vec())
        );
        assert_eq!(repo.read_base_file(&id, "b.yml").expect("reads"), None);
        repo.delete(&id).expect("deletes");
        assert_eq!(repo.get(&id).expect("reads"), None);
        repo.delete(&id).expect("a second delete is fine");
    }

```

Run: `cargo test -j4 -p rocket-network ports::`
Expected: FAIL to compile, `cannot find trait LinkRepository in this scope`.

- [ ] **Step 9: Implement the port**

In `crates/rocket-network/src/ports.rs` add `use crate::link::{LinkId, LinkRecord};` after `use crate::identity::{DeviceKeyMaterial, PeerIdentity};`, and add after `pub trait NetworkSettingsRepository { ... }`:

```rust
/// Persistence of links and their base files. Implemented by `rocket-infra` (`FsLinkRepo`).
pub trait LinkRepository: Send + Sync {
    /// Every link. A link whose `link.yml` is damaged is returned as a `Broken` record.
    fn list(&self) -> DomainResult<Vec<LinkRecord>>;
    fn get(&self, id: &LinkId) -> DomainResult<Option<LinkRecord>>;
    /// Writes the record and replaces the base copies with `base_files` (relative path and
    /// bytes), all or nothing.
    fn save(&self, record: &LinkRecord, base_files: &[(String, Vec<u8>)]) -> DomainResult<()>;
    /// Removes the link and its base copies. A missing link is fine.
    fn delete(&self, id: &LinkId) -> DomainResult<()>;
    /// The base copy of one file. `None` when there is none. A path that fails
    /// `path_is_valid` is `InvalidInput`.
    fn read_base_file(&self, id: &LinkId, path: &str) -> DomainResult<Option<Vec<u8>>>;
}
```

In the `ports.rs` test module add `use crate::link::{LinkState, Manifest};` next to `use std::sync::Mutex;`.

In `crates/rocket-network/src/lib.rs` add `LinkRepository` to the `pub use ports::{ ... }` list (after `DiscoverySink`).

Run: `cargo test -j4 -p rocket-network ports::`
Expected: PASS.

- [ ] **Step 10: Write the failing audit tests**

In `crates/rocket-audit/src/event.rs`, add at the end of `mod tests` (before the closing brace):

```rust
    #[test]
    fn network_sync_kinds_have_stable_tags() {
        let sent = AuditEventKind::NetworkSyncSent {
            device_id: "ABCD-EFGH".into(),
            collection: "Orders".into(),
            outcome: "applied".into(),
        };
        let received = AuditEventKind::NetworkSyncReceived {
            device_id: "ABCD-EFGH".into(),
            collection: "Orders".into(),
            outcome: "failed".into(),
        };
        let removed = AuditEventKind::NetworkLinkRemoved {
            device_id: "ABCD-EFGH".into(),
            collection: "Orders".into(),
        };
        assert_eq!(sent.tag(), "network_sync_sent");
        assert_eq!(received.tag(), "network_sync_received");
        assert_eq!(removed.tag(), "network_link_removed");
    }

    #[test]
    fn network_sync_kinds_map_to_the_expected_controls() {
        let sent = SecurityAuditEvent::new(
            "user",
            None,
            AuditEventKind::NetworkSyncSent {
                device_id: "ABCD-EFGH".into(),
                collection: "Orders".into(),
                outcome: "applied".into(),
            },
            "",
        );
        let received = SecurityAuditEvent::new(
            "user",
            None,
            AuditEventKind::NetworkSyncReceived {
                device_id: "ABCD-EFGH".into(),
                collection: "Orders".into(),
                outcome: "declined".into(),
            },
            "",
        );
        for ev in [&sent, &received] {
            assert!(ev.controls.iter().any(|c| c.code == "CC6.7"));
            assert!(!ev.controls.iter().any(|c| c.code == "CC6.1"));
        }
        let removed = SecurityAuditEvent::new(
            "user",
            None,
            AuditEventKind::NetworkLinkRemoved {
                device_id: "ABCD-EFGH".into(),
                collection: "Orders".into(),
            },
            "",
        );
        assert!(removed.controls.iter().any(|c| c.code == "CC6.1"));
    }

    #[test]
    fn network_sync_kinds_serialize_with_snake_case_fields() {
        for (kind, tag, outcome) in [
            (
                AuditEventKind::NetworkSyncSent {
                    device_id: "ABCD-EFGH".into(),
                    collection: "Orders".into(),
                    outcome: "applied".into(),
                },
                "network_sync_sent",
                Some("applied"),
            ),
            (
                AuditEventKind::NetworkSyncReceived {
                    device_id: "ABCD-EFGH".into(),
                    collection: "Orders".into(),
                    outcome: "declined".into(),
                },
                "network_sync_received",
                Some("declined"),
            ),
            (
                AuditEventKind::NetworkLinkRemoved {
                    device_id: "ABCD-EFGH".into(),
                    collection: "Orders".into(),
                },
                "network_link_removed",
                None,
            ),
        ] {
            let value = serde_json::to_value(&kind).expect("kind serializes");
            assert_eq!(value["kind"], tag);
            assert_eq!(value["device_id"], "ABCD-EFGH");
            assert_eq!(value["collection"], "Orders");
            match outcome {
                Some(word) => assert_eq!(value["outcome"], word),
                None => assert!(value.get("outcome").is_none(), "the removal has no outcome"),
            }
            let back: AuditEventKind = serde_json::from_value(value).expect("kind round trips");
            assert_eq!(back, kind);
        }
    }
```

In `crates/rocket-audit/src/control.rs` add at the end of `mod tests`:

```rust
    #[test]
    fn the_sync_kinds_are_listed_in_the_catalog() {
        for tag in ["network_sync_sent", "network_sync_received"] {
            let controls = controls_for_kind(tag);
            assert!(
                controls
                    .iter()
                    .any(|c| c.framework == Framework::Soc2 && c.code == "CC6.7"),
                "{tag} missing from CC6.7"
            );
        }
        let controls = controls_for_kind("network_link_removed");
        assert!(
            controls
                .iter()
                .any(|c| c.framework == Framework::Soc2 && c.code == "CC6.1"),
            "network_link_removed missing from CC6.1"
        );
    }
```

Run: `cargo test -j4 -p rocket-audit`
Expected: FAIL to compile, `no variant named NetworkSyncSent found for enum AuditEventKind`.

- [ ] **Step 11: Implement the audit kinds**

In `crates/rocket-audit/src/event.rs`, after the `NetworkCollectionReceived { ... },` variant add:

```rust
    /// A sync this node started ended after the first chunk left the node, or was refused.
    /// `device_id` is the short id. `outcome` is `applied`, `failed`, `declined`, `cancelled`,
    /// `expired`, `base_mismatch`, `link_missing` or `unconfirmed`. Counts only, never paths.
    NetworkSyncSent {
        device_id: String,
        collection: String,
        outcome: String,
    },
    /// A sync a paired node started ended on this node. `device_id` is the short id. `outcome`
    /// uses the same words as `NetworkSyncSent`.
    NetworkSyncReceived {
        device_id: String,
        collection: String,
        outcome: String,
    },
    /// A link was removed here (Unlink or the pairing was removed). The files stay.
    NetworkLinkRemoved {
        device_id: String,
        collection: String,
    },
```

and in `tag()` after the `NetworkCollectionReceived` arm:

```rust
            Self::NetworkSyncSent { .. } => "network_sync_sent",
            Self::NetworkSyncReceived { .. } => "network_sync_received",
            Self::NetworkLinkRemoved { .. } => "network_link_removed",
```

In `crates/rocket-audit/src/control.rs` replace

```rust
            "network_device_unpaired",
            "network_device_blocked",
        ],
    },
    ControlEntry {
        framework: Framework::Soc2,
        code: "CC6.7",
```

with

```rust
            "network_device_unpaired",
            "network_device_blocked",
            "network_link_removed",
        ],
    },
    ControlEntry {
        framework: Framework::Soc2,
        code: "CC6.7",
```

and replace

```rust
            "network_collection_received",
        ],
    },
    ControlEntry {
        framework: Framework::Soc2,
        code: "CC7.2",
```

with

```rust
            "network_collection_received",
            "network_sync_sent",
            "network_sync_received",
        ],
    },
    ControlEntry {
        framework: Framework::Soc2,
        code: "CC7.2",
```

Run: `cargo test -j4 -p rocket-audit`
Expected: PASS, all tests (the older network tests and the four new ones).

- [ ] **Step 12: Keep `rocket-app` compiling**

Adding variants and fields breaks these sites (the compiler found no others). Fix each, anchored on the quoted text.

In `crates/rocket-app/src/network/connections.rs` replace

```rust
                        | PeerMessage::ChunkAck { .. }
                        | PeerMessage::TransferResult { .. }),
```

with

```rust
                        | PeerMessage::ChunkAck { .. }
                        | PeerMessage::TransferResult { .. }
                        | PeerMessage::SyncOffer { .. }
                        | PeerMessage::SyncAbort { .. }
                        | PeerMessage::SyncResult { .. }),
```

Leave the comment above it as it is.

In `crates/rocket-app/src/network/transfer.rs` (`handle_frame`) replace

```rust
                chunk_count,
                sha256,
            } => {
                let frame = OfferFrame {
```

with

```rust
                chunk_count,
                sha256,
                ..
            } => {
                let frame = OfferFrame {
```

In `crates/rocket-app/src/network/transfer_fakes.rs` replace

```rust
            chunk_count: self.count(),
            sha256: self.sha(),
        }
```

with the same two lines plus `            keep_in_sync: false,` and `            link_id: None,` before the closing brace.

In `crates/rocket-app/src/network/transfer_receive_tests.rs` replace

```rust
        chunk_count: count,
        sha256: sha.to_string(),
    }
```

with the same two lines plus `        keep_in_sync: false,` and `        link_id: None,`.

In `crates/rocket-app/src/network/transfer_link_tests.rs` (`frames()`) replace

```rust
            chunk_count: 1,
            sha256: "0".repeat(64),
        },
        PeerMessage::OfferAnswer {
```

with

```rust
            chunk_count: 1,
            sha256: "0".repeat(64),
            keep_in_sync: false,
            link_id: None,
        },
        PeerMessage::OfferAnswer {
```

In `crates/rocket-app/src/network/transfer_send_tests.rs` replace

```rust
        chunk_count,
        sha256,
    } = &sent[0]
```

with

```rust
        chunk_count,
        sha256,
        ..
    } = &sent[0]
```

In `crates/rocket-app/src/network/connections_tests.rs`, after `a_socket_with_another_protocol_version_needs_an_update` (before the line `// ----- duplicate sockets -----`) add:

```rust
#[tokio::test(start_paused = true)]
async fn a_peer_on_the_previous_protocol_version_needs_an_update() {
    // Version 2 is the build before keep-in-sync. It must never talk to version 3.
    let (a, _h) = solo(2).await;
    let (socket, remote) = scripted_socket(identity(2), a.id(), PROTOCOL_VERSION - 1);
    a.connector.push_socket(socket);
    a.svc
        .add_address("127.0.0.1", addr(2).port())
        .await
        .expect("add address");
    settle().await;

    assert_eq!(status_of(&a, 2), Some(NodeStatus::NeedsUpdate));
    assert!(remote.to_service.is_closed(), "the socket was dropped");
}

```

Run: `cargo check -j4 -p rocket-app --tests`
Expected: Finished. If the compiler names another `CollectionOffer` literal or pattern, add the two fields (or `..`) the same way.

Run: `cargo check -j4 -p rocket-infra --tests`
Expected: Finished.

Run: `cargo test -j4 -p rocket-app network::`
Expected: PASS, including `a_peer_on_the_previous_protocol_version_needs_an_update`, `a_socket_with_another_protocol_version_needs_an_update` and the existing send and receive tests.

Run: `cargo test -j4 -p rocket-infra network::`
Expected: PASS.

- [ ] **Step 13: Update the crate guide**

In `crates/rocket-network/CLAUDE.md`, replace the `PeerMessage` row with:

```markdown
| `PeerMessage` | WebSocket frame enum: Hello, Ping, Pong, Unpair, the six transfer frames (offer, answer, chunk, abort, chunk ack, result) and the three sync frames (`SyncOffer`, `SyncAbort`, `SyncResult`). `CollectionOffer` carries `keep_in_sync` and `link_id`. `decode` enforces the 64 KiB limit and rejects unknown types. |
| `SyncPayloadKind`, `SyncAbortReason`, `SyncOutcome` | What a sync transfer carries (`manifest`, `plan`, `files`), why a sync stopped, and the apply result (`applied`, `failed`). Every payload travels on the transfer machines, never inside a frame. |
```

and add these rows after the `TransferLimits` row:

```markdown
| `LinkId`, `SyncId`, `Manifest`, `path_is_valid`, `is_sha256_hex` | Link and sync ids (32 lowercase hex), the path-to-SHA-256 map with its safe-path rule, the 5000 entry cap, `base_hash` and JSON. |
| `LinkRecord`, `LinkState`, `SyncPlanPayload`, `RenamePair` | The persisted link (snake_case, with its base manifest), its state, and the JSON plan one node sends the other. |
| `plan_sync`, `resolve`, `new_base`, `SyncAction`, `SyncPlan`, `Conflict`, `ConflictKind`, `Decision`, `Resolution` | The pure three-way planner: the table of spec section 6, renames by hash, conflicts, the user's decisions and the base both sides store. A rename conflict keeps its rename in `actions` as a marker that `resolve` replaces. |
| `LinkRepository` | Port for link records and their base files. Implemented by `rocket-infra`. |
```

Replace the `NetworkEvent`, `TransferEvent` row with:

```markdown
| `NetworkEvent`, `TransferEvent`, `SyncEvent` | Events for the frontend. Implement `PublishedEvent`. Channels `network-changed`, `network-pairing`, `network-transfer` and `network-sync`. `SyncSummary`, `ConflictView`, `SyncRole` and `SyncFinish` are the helper types of `SyncEvent`. |
```

Replace the `TransferSink` row's first words so it reads `(the transfer and sync frames from NetworkService)`. Add to "Key Patterns": `- The planner never reads files or the clock. It works on manifests and returns actions; Plan 2 and 3 of the sync series do the I/O.`

- [ ] **Step 14: Format, lint, and run the whole plan verification**

Run: `cargo fmt -p rocket-network` then `cargo fmt -p rocket-audit` then `cargo fmt -p rocket-app`.
Expected: no output. `git status --short` lists only the files of the File Structure table plus this plan folder (if a crate that was not edited shows changes, revert them with `git checkout -- <path>` before going on).

Run these one at a time:

```bash
cargo check -j4 -p rocket-network
cargo check -j4 -p rocket-audit
cargo check -j4 -p rocket-app
cargo test -j4 -p rocket-network
cargo test -j4 -p rocket-audit
cargo test -j4 -p rocket-infra network::
cargo test -j4 -p rocket-app network::
cargo clippy -j4 -p rocket-network --all-targets
cargo clippy -j4 -p rocket-audit --all-targets
cargo fmt -p rocket-network -- --check
cargo fmt -p rocket-audit -- --check
cargo fmt -p rocket-app -- --check
```

Expected: every command finishes green with no warnings. `grep -rnE "unwrap\(\)|\.expect\(" crates/rocket-network/src/link.rs crates/rocket-network/src/planner.rs` lists only lines inside the `#[cfg(test)]` modules.

No commit; reviewed at the plan end.

---

## Self-review

Spec coverage: section 5 (the link: id, peer, root, uid, base manifest, `base_hash` over the sorted `<path> NUL <hash> LF` lines, the four states, snake_case record, base files behind a repository port, rename by content hash, the known renamed-and-edited limit); section 6 (the whole table, the role swap, rename before the table, a rename against an edit as a conflict, conflicts decided once by the planning node with keep mine, keep theirs and keep both, `wanted_paths` and `decisions` as the plan payload); section 7 (every received path checked against the collection rules, `.rocket`, `..`, device names, the 5000 entry cap, the 10 MiB cap on JSON payloads, the base never read from the peer: only `base_hash` crosses the wire); section 2 decisions 4, 5, 8, 10 and 11 (hash of the scrubbed bytes, per-request resolution, base outside the collection, new variants plus version 3, hashes of exported bytes); section 10 (three audit kinds with device id and counts only); section 11 (`rocket-network` pure pieces; the state machine of that row is not needed because the index moved every payload onto the transfer machines); section 12 (planner table tests for every row and the role swap, rename by hash, rename against edit, manifest and `base_hash` stability, wire contract tests for every new message and event).

Not covered here on purpose: the YAML merge and the file I/O (Plan 2), the sync service, the frame router and link creation at the first send (Plan 3), commands and the real transport (Plan 4), TypeScript (Plan 5).

Index names checked: every type, function and constant of "Plan 1 produces" exists with the signature of the index. Additions the index does not list: `RenamePair`, `path_is_valid`, `is_sha256_hex`, `Manifest::is_empty`, `SyncPayloadKind::transfer_name`, `Serialize`/`Deserialize` on `ConflictKind`, `Resolution` and `Decision`, two crate-private `Manifest` accessors, and the `CollectionOffer` serde skip rules. Behaviours that Plans 2 to 4 must know: `plan_sync` leaves a rename marker in `actions` for a `RenameVsEdit` conflict and only `resolve` output may be applied; `resolve` needs `taken` to be the union of both manifests; `new_base` is computed from the planning node's view, so the starting node (which never receives the planner's manifest) cannot call it and must build its base from the post-apply manifest or from the agreed state sent by the planner.
