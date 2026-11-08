# Team network sync: Plan 2, storage and merge

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 2 of 5.** Previous plan: `docs/superpowers/plans/team-network-sync/2026-11-08-sync-plan-01-domain.md` (must be done and green; it adds `Manifest`, `LinkId`, `LinkRecord`, `LinkState` and the `LinkRepository` port to `rocket-network`).
**Next plan to run: `docs/superpowers/plans/team-network-sync/2026-11-08-sync-plan-03-service.md`**
**Recommended model: Sonnet.**

**Goal:** Everything `SyncService` (Plan 3) needs from the disk and from the collection, with no service logic yet. `rocket-export` merges a received file into a local file without losing local secrets and local-only keys (spec decision 11). `rocket-infra` stores links and their base copies. `rocket-app` has the `SyncCollectionPort` (manifest, export of chosen files, atomic apply) with a real implementation over the Bruno export, the collection lock and the journaled file transaction, plus in-memory fakes for the Plan 3 tests.

**Architecture:** `rocket_export::sync_merge::merge_file` parses both files as `serde_yaml::Value` and merges by key: the peer's value wins, except that a local filled scalar is never replaced by a blank one and the keys the export removes always stay local. Lists are replaced by the peer's list, then a received row that has the same name (and type) as a local row is merged with that row, so a blank received value keeps the local one. `rocket_export::sync_zip::zip_entries` reads a Bruno ZIP and strips its single root folder. `rocket_export::is_synced_path` is the one path filter of the sync, taken from the export filter. `FsLinkRepo` keeps `<dir>/<link_id>/link.yml` and `base/<path>`, hash-checked. `ExportBackedSyncCollections` (in `crates/rocket-app/src/network/sync_collections.rs`) builds the manifest from `CollectionExportService::export_bruno(collection, false)` (so a manifest hash is the hash of the same scrubbed bytes the first send put in its ZIP), and applies changes inside `collection_lock` with one `CollectionFileTx`. It holds only ports (`CollectionExportService`, `CollectionRepository`, `CollectionFileTxFactory`); it makes no filesystem call and names no infra type.

**Tech Stack:** Rust 1.89 or later. No new dependency: `rocket-export` already has `zip`, `serde_yaml` and `rocket-shared`; `rocket-infra` already has `rocket-network`, `sha2`, `serde_yaml`, `tempfile`; `rocket-app` already has `rocket-export`, `rocket-import`, `sha2` and (dev) `zip`, `tempfile`, `rocket-infra` with `test-support`.

**Spec:** `docs/superpowers/specs/2026-11-08-team-network-sync-collection-design.md` decision 11, sections 5, 7, 11 and 12. Index: `docs/superpowers/plans/team-network-sync/00-plan-index.md` (binding names; the changes this plan makes to it are in "Rulings").

## Rulings (recorded here, because they differ from or extend the index)

1. **`ExportResult` does not exist in `rocket-export`; the crate uses `DomainResult`.** `merge_file` returns `DomainResult<Vec<u8>>`. Invalid YAML is `DomainError::InvalidInput` with a fixed text that never quotes the file.
2. **"Trust-gate keys" are not separate keys.** The three trust requests (`sandboxMode`, `scripts.additionalContextRoots`, `agentAutonomyEnabled`) all live under `extensions.rocketapi` (`rocket-infra/src/fs_collection/settings.rs`), which the export removes whole. So the local-only set is exactly: top-level `uid`, `info.uid`, `extensions.rocketapi` (whole subtree) and every `examples` list. The grants themselves are in `~/.rocket-api/trust.yml`, outside any collection, so no apply can reach them.
3. **A local-owned key never comes from the peer.** In a merge into an existing file, a received `uid`, `info.uid`, `extensions.rocketapi` or `examples` is ignored, also when the local file lacks it. So a peer cannot switch on Developer mode in a file that already exists here. A file that is new to the node is written as received (spec decision 11); its `extensions.rocketapi` is covered by the trust gate (no record, Safe sandbox), as for the first send.
4. **`manifest_from_zip` changes shape.** The index signature `manifest_from_zip(zip_path: &Path)` would make `rocket-app` open a file. It becomes `manifest_from_zip(files: &dyn ImportFiles, zip_path: &Path) -> DomainResult<Manifest>`. The link also needs the base copies for `LinkRepository::save`, which the index function cannot give, so this plan adds `base_from_zip(files, zip_path) -> DomainResult<ZipBase>` and `base_from_reader(reader) -> DomainResult<ZipBase>` (for the sender, whose ZIP is in memory), with `ZipBase { manifest, files }`. All of them keep only paths that pass `is_synced_path`: a first send with "include environments" puts `environments/*` in the ZIP, and those paths are never in a later manifest (the export runs without environments), so the planner would otherwise see them as deleted.
5. **`LinkRepository::save` takes a delta.** `base_files` are the copies to write or overwrite. A copy already stored for a path that is still in `record.base` is kept, so a state-only save passes `&[]`. A stored copy whose path is not in `record.base` is removed. Every `(path, bytes)` must be a key of `record.base` with a matching SHA-256, else `InvalidInput`. `read_base_file` returns `None` when the stored bytes do not match `record.base` (a crash between the copies and `link.yml`), so a diff never shows a wrong base.
6. **A damaged `link.yml` reads as a stub, not as absent.** `LinkRecord` has no way to hold "unknown". The repo renames the file to `link.yml.corrupt-<secs>`, writes a stub `link.yml` (state `Broken`, empty `base`, the fields it can salvage from the damaged text, a zero device id and the name `Unknown collection` for the rest) and returns it. The next read returns the same stub with no second quarantine. `FsLinkRepo` also implements `QuarantineReport`.
7. **The apply path filter.** `rocket_export::is_synced_path(rel)` is true only for a path the export without environments can carry (`.yml`, `.yaml`, `.proto`, `.js`; not `workspace.yml`, `.git`, `node_modules`, `flows/`, hidden top-level folders or `environments/`). `apply` refuses every other path with `InvalidInput`. So a peer cannot overwrite the receiver's environments (which hold its secrets) through a sync.
8. **`SyncCollectionPort` methods block.** Plan 3 calls them inside `tokio::task::spawn_blocking` and never while holding its own state lock.
9. **Plan 1 must derive `Debug, Clone, PartialEq` and serde (`Serialize`, `Deserialize`) on `Manifest`, `LinkRecord`, `LinkState`, and the id types.** `DeviceId` already serializes as base32. If Plan 1 differs, fix it there, not here.

## Path mapping (Bruno export to collection paths)

- `export_bruno(collection, false)` returns a ZIP whose entries are `<stem>/<rel_path>`, where `<stem>` is the collection name with `/ \ : * ? " < > |` and control characters replaced by `_` (`safe_file_stem`). It is always one segment, and there are no directory entries.
- A manifest path is the entry name with the first segment removed. That is exactly the collection-relative path with `/` separators. `apply` passes the same string to `CollectionFileTx` (`RelPath::parse`).
- Paths are not renamed, so the mapping is lossless for paths. The export leaves out: `workspace.yml`, `.git/`, `node_modules/`, `flows/`, hidden top-level folders (`.rocket/`), `environments/`, and every file that is not `.yml`, `.yaml`, `.proto` or `.js`. Those files are never synced and never deleted by a sync.
- The content is lossy by design (the export scrubs it). Known limits, each pinned by a test or listed here:
  - Blanked secrets, removed `uid`, `extensions.rocketapi` and `examples` do not travel. `merge_file` puts the local ones back. A merged file exports to the same bytes as the received file (`the_merged_file_exports_to_the_received_bytes`), so both sides keep one hash.
  - A legacy flat `folder.yml` (`name`, `type`, `uid` at top level) is exported in the `info` shape. After a Put it is rewritten in the `info` shape here, and keeps its `uid` (`a_legacy_folder_file_is_rewritten_to_the_info_shape`).
  - A blank cannot be told from an empty value. If the peer clears a plain value here (for example a description), the local value stays (`a_peer_that_clears_a_plain_value_does_not_clear_it_here`). The next sync then sees this file as changed here and sends it back. Accepted limit (spec section 5).
  - Case-only differences in a path on a case-insensitive disk: `PutAs` and `Rename` targets are checked with `CollectionRepository::path_exists`, which folds case. A plain `Put` of `Users/a.yml` over `users/a.yml` is not detected. Accepted limit.
  - A `Delete` or `Rename` can leave an empty folder behind. Accepted limit.

## Global Constraints

- Cargo commands always carry `-j4`, one crate at a time. No `cargo test --workspace` or `--all`.
- No `unwrap()` in production paths and none in new Rust text a hook checks; tests use `expect("reason")`. Mutex locks use `.lock().unwrap_or_else(|e| e.into_inner())` (in `rocket-infra` network code, `lock_ignore_poison`).
- `rocket-export` stays pure: no filesystem, no clock. `rocket-app` imports no `rocket-infra` type outside `#[cfg(test)]` and makes no `std::fs` call. `rocket-infra` holds the I/O.
- Persistence structs snake_case. `link.yml` is `LinkRecord` as `serde_yaml` writes it. No camelCase rename anywhere in this plan.
- Code comments are short, full sentences that end with a punctuation mark.
- Errors never quote file contents, secret values or a peer's text. Paths may be named.
- The tests of one crate stay in that crate. Fakes for Plan 3 are `#[cfg(test)] pub(crate)` in `rocket-app`.
- No commit during the plan. The user reviews, then commits through `dev-workflow-skills:1-git-commit`, staging explicit paths.

## Review Focus (failure modes, each with its test)

1. **Local secrets, uids or settings lost by a Put.** A Put must never wipe a token, `uid`, `extensions.rocketapi` or saved examples. Tests: `a_local_secret_survives_a_received_blank`, `local_only_keys_survive`, `a_blank_row_value_keeps_the_local_value_of_the_row_with_the_same_name`, `a_secret_variable_data_survives_a_blank`, `apply_put_merges_and_keeps_local_secret_and_uid`.
2. **A peer smuggles a local-only or trust key, or a path outside the sync.** Tests: `received_local_only_keys_are_ignored`, `apply_refuses_paths_the_export_does_not_carry`, `is_synced_path_mirrors_the_export_filter`.
3. **A half-applied sync.** One bad change or a failure at any journal step leaves every file as it was. Tests: `apply_with_a_bad_change_changes_nothing`, `apply_with_invalid_yaml_changes_nothing`, `apply_rolls_back_every_file_when_the_commit_fails_at_any_step`, `apply_waits_for_the_collection_lock`.
4. **A damaged, escaping or stale base store.** Tests: `a_damaged_link_is_quarantined_once_and_reads_as_broken`, `path_traversal_in_a_base_path_is_refused`, `a_base_copy_that_does_not_match_the_record_reads_as_missing`, `a_save_refuses_a_copy_that_does_not_match_the_manifest`, `delete_removes_the_folder`.
5. **Manifest drift (a sync that always finds changes).** The two bases must come from the same bytes, and a secret edit must not count. Tests: `manifest_is_stable_across_two_calls`, `a_secret_change_does_not_change_the_manifest`, `the_first_send_zip_gives_the_same_manifest`, `a_zip_base_skips_paths_the_sync_does_not_carry`, `the_merged_file_exports_to_the_received_bytes`, `apply_put_merges_and_keeps_local_secret_and_uid` (the manifest hash equals the hash of the received bytes after the Put).

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `crates/rocket-export/src/sync_merge.rs` | Create | `merge_file` and its tests |
| `crates/rocket-export/src/sync_zip.rs` | Create | `zip_entries` (strip the single root folder), tests for it and for `is_synced_path` |
| `crates/rocket-export/src/bruno.rs` | Edit | `is_noise` and `rocket_only_root` become `pub(crate)`; new `pub fn is_synced_path` |
| `crates/rocket-export/src/lib.rs` | Edit | `pub mod sync_merge; pub mod sync_zip;` and the re-exports |
| `crates/rocket-export/CLAUDE.md` | Edit | Module map rows for the two modules |
| `crates/rocket-infra/src/network/links.rs` | Create | `FsLinkRepo` and its tests |
| `crates/rocket-infra/src/network/mod.rs` | Edit | `pub mod links;` and `pub use links::FsLinkRepo;` |
| `crates/rocket-infra/src/lib.rs` | Edit | Re-export `FsLinkRepo` |
| `crates/rocket-infra/CLAUDE.md` | Edit | One row in the public types table |
| `crates/rocket-app/src/collection_export_service.rs` | Edit | `raw_files` (the unscrubbed local files, for the merge) |
| `crates/rocket-app/src/network/sync_ports.rs` | Create | `SyncFile`, `FileChange`, `SyncCollectionPort`, `ZipBase`, `base_from_reader`, `base_from_zip`, `manifest_from_zip`, `sha256_hex` |
| `crates/rocket-app/src/network/sync_collections.rs` | Create | `ExportBackedSyncCollections` |
| `crates/rocket-app/src/network/sync_collections_tests.rs` | Create | Tests over a real folder |
| `crates/rocket-app/src/network/sync_fakes.rs` | Create | `FakeSyncCollections`, `FakeLinkRepo` (test only) |
| `crates/rocket-app/src/network/mod.rs` | Edit | Declare the four modules and re-export |
| `crates/rocket-app/CLAUDE.md` | Edit | One row for `ExportBackedSyncCollections` |

---

## Task 1: `rocket-export`, the file merge, the ZIP reader and the path filter

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:** see the table (the six `rocket-export` rows).

**Interfaces**

Consumes: `serde_yaml::{Value, Mapping}`, `zip::ZipArchive`, `rocket_shared::error::{DomainError, DomainResult}`, `bruno_zip` (tests).

Produces:

```rust
// sync_merge.rs
pub fn merge_file(local: Option<&[u8]>, received: &[u8]) -> DomainResult<Vec<u8>>;
// sync_zip.rs
pub const MAX_ZIP_ENTRIES: usize = 5000;
pub const MAX_UNZIPPED_BYTES: u64 = 50 * 1024 * 1024;
pub fn zip_entries<R: std::io::Read + std::io::Seek>(reader: R) -> DomainResult<Vec<(String, Vec<u8>)>>;
// bruno.rs
pub fn is_synced_path(rel_path: &str) -> bool;
```

The merge rules (stated once, tested below):

- `local` is `None`: the received bytes are returned unchanged, after a check that they parse as YAML.
- Both sides are parsed as `serde_yaml::Value`. A side that does not parse gives `InvalidInput("The received file is not valid YAML.")` or `InvalidInput("The local file is not valid YAML.")`, and the caller writes nothing.
- Mappings: the result has the received keys in the received order. For a key present on both sides the two values are merged. A key that only the local side has is dropped (the peer removed it), except the local-owned keys.
- Local-owned keys (ruling 2 and 3): `uid` at the top level, `uid` in the top-level `info`, `rocketapi` in the top-level `extensions`, and `examples` at any depth when either side holds a list there. They always come from the local file, and are absent when the local file lacks them. If a local-only mapping holds an owned key deeper down, only that skeleton is kept.
- Scalars: a local filled scalar (non-empty string, number, boolean) is kept when the received value is blank (null or the empty string). Anything else takes the received value.
- Lists: replaced by the received list. Then each received row that is a mapping with a `name` (or `key`) is merged, as a mapping, with the first unused local row that has the same name and the same `type` (both missing counts as equal). Rows without a name are replaced as they are. This is the explicit list rule: lists are replaced by the peer's list, then any local row whose name matches a received row keeps its local value where the received value is blank.

### Steps

- [ ] **Step 1: Read the spec reference.** 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Read `crates/rocket-export/CLAUDE.md` and `crates/rocket-export/src/bruno.rs` (`is_noise`, `rocket_only_root`, `strip_uids`, `drop_examples`).

- [ ] **Step 2: RED, the merge tests.** Create `crates/rocket-export/src/sync_merge.rs` with only a stub and the tests, so the tests fail to pass first:

```rust
//! The file merge of a sync (spec decision 11 of the sync design).

use rocket_shared::error::{DomainError, DomainResult};

/// Merges a received (scrubbed) file into the local file.
pub fn merge_file(_local: Option<&[u8]>, _received: &[u8]) -> DomainResult<Vec<u8>> {
    Err(DomainError::Internal("not built".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bruno::bruno_zip;
    use crate::ports::SourceFile;
    use crate::sync_zip::zip_entries;
    use serde_yaml::Value;
    use std::io::Cursor;

    fn parse(bytes: &[u8]) -> Value {
        serde_yaml::from_slice(bytes).expect("valid yaml")
    }

    fn merged(local: &str, received: &str) -> Value {
        let bytes = merge_file(Some(local.as_bytes()), received.as_bytes()).expect("merges");
        parse(&bytes)
    }

    /// The bytes the export puts in the ZIP for one file.
    fn exported(rel: &str, raw: &str) -> Vec<u8> {
        let file = SourceFile {
            rel_path: rel.to_string(),
            bytes: raw.as_bytes().to_vec(),
        };
        let (zip, _) = bruno_zip(vec![file], "c", false).expect("exports");
        zip_entries(Cursor::new(zip)).expect("entries").remove(0).1
    }

    #[test]
    fn a_new_file_is_written_as_received() {
        let received = b"info:\n  name: R\nhttp:\n  url: https://x\n";
        let out = merge_file(None, received).expect("merges");
        assert_eq!(out, received.to_vec());
    }

    #[test]
    fn a_new_file_that_is_not_yaml_is_refused() {
        let err = merge_file(None, b"a: [1, 2").expect_err("refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn invalid_yaml_on_either_side_is_refused() {
        let ok = "a: 1\n";
        let bad = "a: [1, 2";
        let received_bad = merge_file(Some(ok.as_bytes()), bad.as_bytes()).expect_err("refused");
        let local_bad = merge_file(Some(bad.as_bytes()), ok.as_bytes()).expect_err("refused");
        assert!(matches!(received_bad, DomainError::InvalidInput(_)));
        assert!(matches!(local_bad, DomainError::InvalidInput(_)));
        // The message never quotes the file.
        assert!(!received_bad.to_string().contains("[1, 2"));
    }

    #[test]
    fn a_local_secret_survives_a_received_blank() {
        let local = "uid: r1\ninfo:\n  name: Login\nhttp:\n  url: https://old\n  headers:\n    - name: Authorization\n      value: Bearer abc\n    - name: Accept\n      value: text/plain\nauth:\n  type: bearer\n  token: s3cret\n";
        let received = "info:\n  name: Login\nhttp:\n  url: https://new\n  headers:\n    - name: Authorization\n      value: ''\n    - name: Accept\n      value: application/json\nauth:\n  type: bearer\n  token: ''\n";
        let v = merged(local, received);
        assert_eq!(v["http"]["url"].as_str(), Some("https://new"));
        assert_eq!(v["http"]["headers"][0]["value"].as_str(), Some("Bearer abc"));
        assert_eq!(
            v["http"]["headers"][1]["value"].as_str(),
            Some("application/json")
        );
        assert_eq!(v["auth"]["token"].as_str(), Some("s3cret"));
    }

    #[test]
    fn a_number_secret_survives_a_received_blank() {
        let v = merged("auth:\n  token: 123456\n", "auth:\n  token: ''\n");
        assert_eq!(v["auth"]["token"].as_u64(), Some(123456));
    }

    #[test]
    fn a_secret_variable_data_survives_a_blank() {
        let local = "variables:\n  - name: api\n    value:\n      type: secret\n      data: abc\n";
        let received = "variables:\n  - name: api\n    value:\n      type: secret\n      data: ''\n";
        let v = merged(local, received);
        assert_eq!(v["variables"][0]["value"]["data"].as_str(), Some("abc"));
    }

    #[test]
    fn local_only_keys_survive() {
        let local = "uid: top-uid\ninfo:\n  name: R\n  uid: info-uid\nextensions:\n  rocketapi:\n    sandboxMode: developer\n    agentAutonomyEnabled: true\n    scripts:\n      additionalContextRoots:\n        - /opt/lib\nexamples:\n  - name: ok\n    response:\n      status: 200\nhttp:\n  url: https://old\n";
        let received = "info:\n  name: R\nhttp:\n  url: https://new\n";
        let v = merged(local, received);
        assert_eq!(v["uid"].as_str(), Some("top-uid"));
        assert_eq!(v["info"]["uid"].as_str(), Some("info-uid"));
        let api = &v["extensions"]["rocketapi"];
        assert_eq!(api["sandboxMode"].as_str(), Some("developer"));
        assert_eq!(api["agentAutonomyEnabled"].as_bool(), Some(true));
        assert_eq!(
            api["scripts"]["additionalContextRoots"][0].as_str(),
            Some("/opt/lib")
        );
        assert_eq!(v["examples"][0]["name"].as_str(), Some("ok"));
        assert_eq!(v["http"]["url"].as_str(), Some("https://new"));
    }

    #[test]
    fn local_only_keys_survive_when_the_received_file_has_no_extensions_or_info() {
        let local = "info:\n  uid: i\nextensions:\n  rocketapi:\n    sandboxMode: developer\n";
        let v = merged(local, "http:\n  url: https://x\n");
        assert_eq!(v["info"]["uid"].as_str(), Some("i"));
        assert_eq!(
            v["extensions"]["rocketapi"]["sandboxMode"].as_str(),
            Some("developer")
        );
    }

    #[test]
    fn received_local_only_keys_are_ignored() {
        let local = "uid: good\ninfo:\n  name: R\nhttp:\n  url: https://a\n";
        let received = "uid: evil\ninfo:\n  name: R\n  uid: evil2\nextensions:\n  rocketapi:\n    sandboxMode: developer\n  bruno:\n    keep: 1\nexamples:\n  - name: x\nhttp:\n  url: https://b\n";
        let v = merged(local, received);
        assert_eq!(v["uid"].as_str(), Some("good"));
        assert!(v["info"]["uid"].is_null());
        assert!(v["extensions"]["rocketapi"].is_null());
        assert_eq!(v["extensions"]["bruno"]["keep"].as_u64(), Some(1));
        assert!(v["examples"].is_null());
        assert_eq!(v["http"]["url"].as_str(), Some("https://b"));
    }

    #[test]
    fn the_peers_value_wins_on_other_keys() {
        let local = "info:\n  name: R\n  seq: 1\nhttp:\n  method: GET\n  url: https://a\ndocs:\n  content: old docs\n";
        let received = "info:\n  name: R2\n  seq: 3\nhttp:\n  method: POST\n  url: https://b\n";
        let v = merged(local, received);
        assert_eq!(v["info"]["name"].as_str(), Some("R2"));
        assert_eq!(v["info"]["seq"].as_u64(), Some(3));
        assert_eq!(v["http"]["method"].as_str(), Some("POST"));
        assert_eq!(v["http"]["url"].as_str(), Some("https://b"));
        // The peer removed the key, so it goes.
        assert!(v["docs"].is_null());
    }

    #[test]
    fn lists_are_replaced_by_the_peers_list() {
        let local = "http:\n  headers:\n    - name: A\n      value: '1'\n    - name: B\n      value: '2'\n    - name: C\n      value: '3'\n";
        let received = "http:\n  headers:\n    - name: B\n      value: two\n    - name: D\n      value: '4'\n";
        let v = merged(local, received);
        let rows = v["http"]["headers"].as_sequence().expect("list");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["name"].as_str(), Some("B"));
        assert_eq!(rows[0]["value"].as_str(), Some("two"));
        assert_eq!(rows[1]["name"].as_str(), Some("D"));
        assert_eq!(rows[1]["value"].as_str(), Some("4"));
    }

    #[test]
    fn a_blank_row_value_keeps_the_local_value_of_the_row_with_the_same_name() {
        let local = "http:\n  headers:\n    - name: Authorization\n      value: tok\n    - name: X-Dup\n      value: one\n    - name: X-Dup\n      value: two\n    - name: Old\n      value: gone\n";
        let received = "http:\n  headers:\n    - name: X-Dup\n      value: ''\n    - name: Authorization\n      value: ''\n    - name: X-Dup\n      value: ''\n    - name: New\n      value: ''\n";
        let v = merged(local, received);
        let rows = v["http"]["headers"].as_sequence().expect("list");
        let got: Vec<(&str, &str)> = rows
            .iter()
            .map(|r| {
                (
                    r["name"].as_str().expect("name"),
                    r["value"].as_str().expect("value"),
                )
            })
            .collect();
        // The received order wins. Duplicates pair up in order. A new row stays blank.
        assert_eq!(
            got,
            vec![
                ("X-Dup", "one"),
                ("Authorization", "tok"),
                ("X-Dup", "two"),
                ("New", "")
            ]
        );
    }

    #[test]
    fn rows_with_the_same_name_and_another_type_do_not_mix() {
        let local = "http:\n  params:\n    - name: id\n      type: query\n      value: q\n    - name: id\n      type: path\n      value: '7'\n";
        let received = "http:\n  params:\n    - name: id\n      type: path\n      value: ''\n    - name: id\n      type: query\n      value: ''\n";
        let v = merged(local, received);
        assert_eq!(v["http"]["params"][0]["value"].as_str(), Some("7"));
        assert_eq!(v["http"]["params"][1]["value"].as_str(), Some("q"));
    }

    #[test]
    fn a_peer_that_clears_a_plain_value_does_not_clear_it_here() {
        // Known limit: a blank cannot be told from an empty value.
        let v = merged("docs:\n  content: old\n", "docs:\n  content: ''\n");
        assert_eq!(v["docs"]["content"].as_str(), Some("old"));
    }

    #[test]
    fn a_legacy_folder_file_is_rewritten_to_the_info_shape() {
        let local = "name: users\ntype: folder\nuid: f1\nseq: 2\n";
        let received = "info:\n  name: users\n  type: folder\n  seq: 2\n";
        let v = merged(local, received);
        assert_eq!(v["info"]["name"].as_str(), Some("users"));
        assert!(v["name"].is_null());
        assert!(v["type"].is_null());
        assert!(v["seq"].is_null());
        assert_eq!(v["uid"].as_str(), Some("f1"));
    }

    #[test]
    fn the_merged_file_exports_to_the_received_bytes() {
        let local = "uid: local-uid\ninfo:\n  name: Login\n  type: http\n  uid: local-info\nextensions:\n  rocketapi:\n    sandboxMode: developer\nexamples:\n  - name: e1\nhttp:\n  method: GET\n  url: https://old\n  headers:\n    - name: Authorization\n      value: Bearer LOCALSECRET\n";
        let peer = "uid: peer-uid\ninfo:\n  name: Login\n  type: http\nhttp:\n  method: POST\n  url: https://new\n  headers:\n    - name: Authorization\n      value: Bearer PEERSECRET\n    - name: Accept\n      value: '*/*'\n";
        let received = exported("Login.yml", peer);
        let merged_bytes = merge_file(Some(local.as_bytes()), &received).expect("merges");
        let text = String::from_utf8(merged_bytes).expect("utf8");
        // The local secret is still there ...
        assert!(text.contains("LOCALSECRET"));
        // ... and the export of the merged file is the received file, so both sides hash alike.
        assert_eq!(exported("Login.yml", &text), received);
    }
}
```

- [ ] **Step 3: RED, the ZIP reader and path filter tests.** Create `crates/rocket-export/src/sync_zip.rs` with a stub and tests:

```rust
//! Reads a Bruno export ZIP back into collection-relative files.

use std::io::{Read, Seek};

use rocket_shared::error::{DomainError, DomainResult};

/// The most entries a sync ZIP may hold, as for a transfer ZIP.
pub const MAX_ZIP_ENTRIES: usize = 5000;
/// The most bytes a sync ZIP may hold unzipped, as for a transfer ZIP.
pub const MAX_UNZIPPED_BYTES: u64 = 50 * 1024 * 1024;

/// Entries of a ZIP with the single root folder removed, sorted by path.
pub fn zip_entries<R: Read + Seek>(_reader: R) -> DomainResult<Vec<(String, Vec<u8>)>> {
    Err(DomainError::Internal("not built".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bruno::{bruno_zip, is_synced_path};
    use crate::ports::SourceFile;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn make_zip(entries: &[(&str, &str)]) -> Cursor<Vec<u8>> {
        let mut w = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, body) in entries {
            w.start_file(*name, SimpleFileOptions::default())
                .expect("start");
            w.write_all(body.as_bytes()).expect("write");
        }
        Cursor::new(w.finish().expect("finish").into_inner())
    }

    #[test]
    fn the_single_root_folder_is_stripped_and_the_entries_are_sorted() {
        let zip = make_zip(&[
            ("My API (copy)/users/get.yml", "b: 2"),
            ("My API (copy)/opencollection.yml", "a: 1"),
        ]);
        let got = zip_entries(zip).expect("reads");
        let names: Vec<&str> = got.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(names, vec!["opencollection.yml", "users/get.yml"]);
        assert_eq!(got[0].1, b"a: 1".to_vec());
    }

    #[test]
    fn an_empty_zip_has_no_entries() {
        let got = zip_entries(make_zip(&[])).expect("reads");
        assert!(got.is_empty());
    }

    #[test]
    fn two_root_folders_are_refused() {
        let zip = make_zip(&[("a/x.yml", "1"), ("b/y.yml", "2")]);
        assert!(matches!(
            zip_entries(zip),
            Err(DomainError::InvalidInput(_))
        ));
    }

    #[test]
    fn a_file_outside_a_root_folder_is_refused() {
        assert!(matches!(
            zip_entries(make_zip(&[("x.yml", "1")])),
            Err(DomainError::InvalidInput(_))
        ));
    }

    #[test]
    fn unsafe_names_are_refused() {
        for name in ["r/../x.yml", "r/./x.yml", "r//x.yml", "r\\x.yml", "/r/x.yml"] {
            assert!(
                matches!(
                    zip_entries(make_zip(&[(name, "1")])),
                    Err(DomainError::InvalidInput(_))
                ),
                "{name} should be refused"
            );
        }
    }

    #[test]
    fn a_duplicate_path_is_refused() {
        let zip = make_zip(&[("r/x.yml", "1"), ("r/x.yml", "2")]);
        assert!(zip_entries(zip).is_err());
    }

    #[test]
    fn is_synced_path_mirrors_the_export_filter() {
        let cases = [
            ("opencollection.yml", true),
            ("users/get.yml", true),
            ("users/get.yaml", true),
            ("protos/a.proto", true),
            ("lib/util.js", true),
            ("environments/dev.yml", false),
            ("flows/f.yml", false),
            (".rocket/c.yml", false),
            ("workspace.yml", false),
            (".git/config", false),
            ("node_modules/a.js", false),
            ("notes.txt", false),
        ];
        let files: Vec<SourceFile> = cases
            .iter()
            .map(|(path, _)| SourceFile {
                rel_path: path.to_string(),
                bytes: b"a: 1\n".to_vec(),
            })
            .collect();
        let (zip, _) = bruno_zip(files, "c", false).expect("exports");
        let carried: Vec<String> = zip_entries(Cursor::new(zip))
            .expect("reads")
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        for (path, expected) in cases {
            assert_eq!(is_synced_path(path), expected, "{path}");
            assert_eq!(
                carried.iter().any(|p| p == path),
                expected,
                "{path} against the export"
            );
        }
    }
}
```

- [ ] **Step 4: Wire the modules.** In `crates/rocket-export/src/lib.rs` replace

```rust
mod sensitive;
pub mod tree;
```

with

```rust
mod sensitive;
pub mod sync_merge;
pub mod sync_zip;
pub mod tree;
```

and replace `pub use bruno::bruno_zip;` with `pub use bruno::{bruno_zip, is_synced_path};`, and add after `pub use sensitive::is_sensitive_name;` the two lines `pub use sync_merge::merge_file;` and `pub use sync_zip::zip_entries;`.

In `crates/rocket-export/src/bruno.rs` replace `fn is_noise(rel_path: &str) -> bool {` with `pub(crate) fn is_noise(rel_path: &str) -> bool {`, and `fn rocket_only_root(rel_path: &str) -> Option<&str> {` with `pub(crate) fn rocket_only_root(rel_path: &str) -> Option<&str> {`. Add after `fn is_yaml` (anchor: the line `matches!(extension(rel_path), "yml" | "yaml")` and its closing brace):

```rust
/// True for a collection-relative path that the Bruno export without environments carries.
/// The sync uses this one rule for its manifests and its apply, so it cannot drift from the
/// export.
pub fn is_synced_path(rel_path: &str) -> bool {
    if is_noise(rel_path) || rocket_only_root(rel_path).is_some() {
        return false;
    }
    if rel_path.starts_with("environments/") {
        return false;
    }
    matches!(extension(rel_path), "yml" | "yaml" | "proto" | "js")
}
```

- [ ] **Step 5: RED check.** Run `cargo test -j4 -p rocket-export sync_`. Expected: the build succeeds and the tests that need the real code FAIL (`a_new_file_is_written_as_received`, `the_single_root_folder_is_stripped_and_the_entries_are_sorted`, ...), with `not built` in the messages. `is_synced_path_mirrors_the_export_filter` also fails until `zip_entries` is real.

- [ ] **Step 6: GREEN, `zip_entries`.** Replace the stub body in `sync_zip.rs`. The algorithm the signature does not fix:
  - Open `zip::ZipArchive::new(reader)`; failure is `InvalidInput("The archive cannot be read.")`. More than `MAX_ZIP_ENTRIES` entries is `InvalidInput("The archive has too many files.")`.
  - Skip an entry whose name ends with `/`. Refuse (`InvalidInput("The archive has an unsafe file name.")`) a name that contains `\`, starts with `/`, has no `/`, or whose first segment or any later segment is empty, `.` or `..`.
  - All files must share one first segment (the root). A second root is `InvalidInput("The archive has more than one root folder.")`.
  - Sum `entry.size()`; above `MAX_UNZIPPED_BYTES` is `InvalidInput("The archive is too big.")`. Read each entry with `(&mut entry).take(MAX_UNZIPPED_BYTES + 1).read_to_end(&mut bytes)`, an I/O failure is `DomainError::Io("The archive cannot be read.")`.
  - Push `(rest_of_name, bytes)`. Sort by path. A duplicate path is `InvalidInput("The archive has the same file twice.")`.

- [ ] **Step 7: GREEN, `merge_file`.** Replace the stub in `sync_merge.rs` with this implementation (add `use serde_yaml::{Mapping, Value};`):

```rust
pub fn merge_file(local: Option<&[u8]>, received: &[u8]) -> DomainResult<Vec<u8>> {
    let received_value: Value = serde_yaml::from_slice(received)
        .map_err(|_| DomainError::InvalidInput("The received file is not valid YAML.".into()))?;
    let Some(local) = local else {
        return Ok(received.to_vec());
    };
    let local_value: Value = serde_yaml::from_slice(local)
        .map_err(|_| DomainError::InvalidInput("The local file is not valid YAML.".into()))?;
    let merged = merge_value(&local_value, &received_value, &mut Vec::new());
    serde_yaml::to_string(&merged)
        .map(String::into_bytes)
        .map_err(|_| DomainError::Internal("The merged file could not be written.".into()))
}

fn merge_value(local: &Value, received: &Value, path: &mut Vec<String>) -> Value {
    match (local, received) {
        (Value::Mapping(l), Value::Mapping(r)) => Value::Mapping(merge_map(l, r, path)),
        (Value::Sequence(l), Value::Sequence(r)) => Value::Sequence(merge_rows(l, r, path)),
        (l, r) if is_blank(r) && is_filled_scalar(l) => l.clone(),
        (_, r) => r.clone(),
    }
}

fn is_blank(value: &Value) -> bool {
    matches!(value, Value::Null) || matches!(value, Value::String(s) if s.is_empty())
}

fn is_filled_scalar(value: &Value) -> bool {
    match value {
        Value::String(s) => !s.is_empty(),
        Value::Number(_) | Value::Bool(_) => true,
        _ => false,
    }
}

/// The keys the export removes. They always come from the local file.
fn is_local_owned(path: &[String], key: &str, local: Option<&Value>, received: Option<&Value>) -> bool {
    let path: Vec<&str> = path.iter().map(String::as_str).collect();
    match (path.as_slice(), key) {
        ([], "uid") | (["info"], "uid") | (["extensions"], "rocketapi") => true,
        (_, "examples") => {
            matches!(local, Some(Value::Sequence(_))) || matches!(received, Some(Value::Sequence(_)))
        }
        _ => false,
    }
}

fn merge_map(local: &Mapping, received: &Mapping, path: &mut Vec<String>) -> Mapping {
    let mut out = Mapping::new();
    for (key, value) in received {
        let name = key.as_str().unwrap_or("");
        if is_local_owned(path, name, local.get(key), Some(value)) {
            continue;
        }
        let merged = match local.get(key) {
            Some(l) => {
                path.push(name.to_string());
                let v = merge_value(l, value, path);
                path.pop();
                v
            }
            None => value.clone(),
        };
        out.insert(key.clone(), merged);
    }
    for (key, value) in local {
        let name = key.as_str().unwrap_or("");
        let theirs = received.get(key);
        if is_local_owned(path, name, Some(value), theirs) {
            out.insert(key.clone(), value.clone());
        } else if theirs.is_none() {
            // The peer dropped this key. Only owned keys deeper down are kept.
            if let Value::Mapping(inner) = value {
                path.push(name.to_string());
                let kept = merge_map(inner, &Mapping::new(), path);
                path.pop();
                if !kept.is_empty() {
                    out.insert(key.clone(), Value::Mapping(kept));
                }
            }
        }
    }
    out
}

/// A row is identified by its name (or key) and its type.
fn row_id(row: &Mapping) -> Option<(String, Option<String>)> {
    let name = row.get("name").or_else(|| row.get("key")).and_then(Value::as_str)?;
    let kind = row.get("type").and_then(Value::as_str).map(str::to_string);
    Some((name.to_string(), kind))
}

fn merge_rows(local: &[Value], received: &[Value], path: &mut Vec<String>) -> Vec<Value> {
    let mut used = vec![false; local.len()];
    let mut out = Vec::with_capacity(received.len());
    for row in received {
        let (Value::Mapping(r), Some(id)) = (row, row_of(row)) else {
            out.push(row.clone());
            continue;
        };
        let found = local.iter().enumerate().find(|(i, l)| {
            !used[*i] && matches!(l, Value::Mapping(m) if row_id(m).as_ref() == Some(&id))
        });
        match found {
            Some((i, Value::Mapping(l))) => {
                used[i] = true;
                out.push(Value::Mapping(merge_map(l, r, path)));
            }
            _ => out.push(row.clone()),
        }
    }
    out
}

fn row_of(value: &Value) -> Option<(String, Option<String>)> {
    match value {
        Value::Mapping(m) => row_id(m),
        _ => None,
    }
}
```

- [ ] **Step 8: GREEN check.** Run `cargo test -j4 -p rocket-export sync_`. Expected: `test result: ok.` with 0 failed (16 merge tests plus the 7 ZIP and filter tests, all passing). Then `cargo test -j4 -p rocket-export` (the whole crate, including the Postman golden test) passes.

- [ ] **Step 9: Lint.** Run `cargo clippy -j4 -p rocket-export -- -D warnings`. Expected: no warnings. Fix `clippy::type_complexity` on `row_id` with a small `type RowId = (String, Option<String>);` if it fires.

- [ ] **Step 10: Docs.** In `crates/rocket-export/CLAUDE.md`, add two rows to the module map after the `names.rs` row: `` `sync_merge.rs` | `merge_file(local, received)`: the sync's file merge (peer wins; local filled scalar kept over a blank; `uid`, `info.uid`, `extensions.rocketapi`, `examples` stay local; lists replaced, rows matched by name and type) `` and `` `sync_zip.rs` | `zip_entries`: reads a Bruno ZIP and strips its single root folder; `bruno.rs` also holds `is_synced_path`, the one path filter of the sync ``. Add under "Layering" nothing else.

---

## Task 2: `rocket-infra`, `FsLinkRepo`

**Files:** the four `rocket-infra` rows of the table.

**Interfaces**

Consumes: `rocket_network::link::{LinkId, LinkRecord, LinkState, Manifest}`, `rocket_network::ports::{LinkRepository, QuarantineReport}`, `rocket_network::device::DeviceId`, `rocket_collection::file_tx::validate_rel_path`, `crate::atomic_write`, `crate::yaml_io::quarantine_corrupt`, `crate::network::lock::lock_ignore_poison`.

Produces:

```rust
pub struct FsLinkRepo { /* dir, store mutex, quarantined name */ }
impl FsLinkRepo {
    /// Does not touch the disk. The folder is created by the first `save`.
    pub fn new(dir: PathBuf) -> FsLinkRepo;
}
impl LinkRepository for FsLinkRepo { /* list, get, save, delete, read_base_file */ }
impl QuarantineReport for FsLinkRepo { /* take_quarantined */ }
```

Layout: `<dir>/<link_id>/link.yml` and `<dir>/<link_id>/base/<path>` (path segments separated by `/`). One store mutex covers every call. The semantics are in rulings 5 and 6.

### Steps

- [ ] **Step 1: RED, the tests.** Create `crates/rocket-infra/src/network/links.rs` with the type, a stub that returns `Err(DomainError::Internal("not built".into()))` from every trait method, and these tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_network::device::DeviceId;
    use rocket_network::link::{LinkState, Manifest};
    use tempfile::TempDir;

    fn sha(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(bytes))
    }

    fn record(id: &LinkId, files: &[(&str, &[u8])]) -> LinkRecord {
        let mut base = Manifest::new();
        for (path, bytes) in files {
            base.insert(path, &sha(bytes)).expect("manifest entry");
        }
        LinkRecord {
            link_id: id.clone(),
            peer_device_id: DeviceId::from_bytes([7u8; 32]),
            collection_name: "Orders".to_string(),
            collection_root: "/data/collections/Orders".to_string(),
            collection_uid: Some("col-uid".to_string()),
            base,
            state: LinkState::InSync,
            updated_at: "2026-11-08T10:00:00Z".to_string(),
        }
    }

    fn owned(files: &[(&str, &[u8])]) -> Vec<(String, Vec<u8>)> {
        files
            .iter()
            .map(|(p, b)| (p.to_string(), b.to_vec()))
            .collect()
    }

    fn repo() -> (TempDir, FsLinkRepo) {
        let dir = TempDir::new().expect("tempdir");
        let repo = FsLinkRepo::new(dir.path().join("links"));
        (dir, repo)
    }

    #[test]
    fn a_saved_link_round_trips() {
        let (_dir, repo) = repo();
        let id = LinkId::random();
        let files: [(&str, &[u8]); 2] = [("a.yml", b"a: 1\n"), ("users/b.yml", b"b: 2\n")];
        let rec = record(&id, &files);
        repo.save(&rec, &owned(&files)).expect("saves");
        assert_eq!(repo.get(&id).expect("gets"), Some(rec.clone()));
        assert_eq!(repo.list().expect("lists"), vec![rec]);
    }

    #[test]
    fn an_unknown_link_is_none_and_an_empty_store_lists_nothing() {
        let (_dir, repo) = repo();
        assert_eq!(repo.get(&LinkId::random()).expect("gets"), None);
        assert!(repo.list().expect("lists").is_empty());
    }

    #[test]
    fn base_files_come_back_byte_for_byte() {
        let (_dir, repo) = repo();
        let id = LinkId::random();
        let crlf: &[u8] = b"a: 1\r\nb: caf\xc3\xa9\r\n";
        let binary: &[u8] = &[0, 159, 146, 150, 255, 10];
        let files: [(&str, &[u8]); 2] = [("deep/er/a.yml", crlf), ("lib/blob.js", binary)];
        repo.save(&record(&id, &files), &owned(&files)).expect("saves");
        assert_eq!(
            repo.read_base_file(&id, "deep/er/a.yml").expect("reads"),
            Some(crlf.to_vec())
        );
        assert_eq!(
            repo.read_base_file(&id, "lib/blob.js").expect("reads"),
            Some(binary.to_vec())
        );
        assert_eq!(repo.read_base_file(&id, "nope.yml").expect("reads"), None);
    }

    #[test]
    fn a_state_only_save_keeps_the_base_copies() {
        let (_dir, repo) = repo();
        let id = LinkId::random();
        let files: [(&str, &[u8]); 1] = [("a.yml", b"a: 1\n")];
        let mut rec = record(&id, &files);
        repo.save(&rec, &owned(&files)).expect("saves");
        rec.state = LinkState::OutOfDate;
        repo.save(&rec, &[]).expect("saves the state");
        assert_eq!(repo.get(&id).expect("gets").expect("exists").state, LinkState::OutOfDate);
        assert_eq!(
            repo.read_base_file(&id, "a.yml").expect("reads"),
            Some(b"a: 1\n".to_vec())
        );
    }

    #[test]
    fn a_save_replaces_a_changed_copy_and_prunes_a_path_that_left_the_manifest() {
        let (dir, repo) = repo();
        let id = LinkId::random();
        let first: [(&str, &[u8]); 2] = [("a.yml", b"old\n"), ("gone/b.yml", b"b\n")];
        repo.save(&record(&id, &first), &owned(&first)).expect("saves");
        let second: [(&str, &[u8]); 1] = [("a.yml", b"new\n")];
        repo.save(&record(&id, &second), &owned(&second)).expect("saves");
        assert_eq!(
            repo.read_base_file(&id, "a.yml").expect("reads"),
            Some(b"new\n".to_vec())
        );
        assert!(!dir
            .path()
            .join("links")
            .join(id.as_str())
            .join("base/gone/b.yml")
            .exists());
    }

    #[test]
    fn a_save_refuses_a_copy_that_does_not_match_the_manifest() {
        let (dir, repo) = repo();
        let id = LinkId::random();
        let rec = record(&id, &[("a.yml", b"right\n")]);
        let err = repo
            .save(&rec, &owned(&[("a.yml", b"wrong\n")]))
            .expect_err("refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        let not_listed = repo
            .save(&rec, &owned(&[("other.yml", b"x\n")]))
            .expect_err("refused");
        assert!(matches!(not_listed, DomainError::InvalidInput(_)));
        // Nothing was written.
        assert!(!dir.path().join("links").join(id.as_str()).join("link.yml").exists());
    }

    #[test]
    fn a_base_copy_that_does_not_match_the_record_reads_as_missing() {
        let (dir, repo) = repo();
        let id = LinkId::random();
        let files: [(&str, &[u8]); 1] = [("a.yml", b"a: 1\n")];
        repo.save(&record(&id, &files), &owned(&files)).expect("saves");
        let copy = dir.path().join("links").join(id.as_str()).join("base/a.yml");
        std::fs::write(copy, b"tampered\n").expect("tamper");
        assert_eq!(repo.read_base_file(&id, "a.yml").expect("reads"), None);
    }

    #[test]
    fn a_damaged_link_is_quarantined_once_and_reads_as_broken() {
        let (dir, repo) = repo();
        let id = LinkId::random();
        let files: [(&str, &[u8]); 1] = [("a.yml", b"a: 1\n")];
        repo.save(&record(&id, &files), &owned(&files)).expect("saves");
        let folder = dir.path().join("links").join(id.as_str());
        std::fs::write(
            folder.join("link.yml"),
            "collection_name: Orders\n: : not yaml [",
        )
        .expect("damage");

        let first = repo.get(&id).expect("gets").expect("a stub");
        assert_eq!(first.state, LinkState::Broken);
        assert_eq!(first.link_id, id);
        assert!(first.base.len() == 0);
        let second = repo.get(&id).expect("gets").expect("the same stub");
        assert_eq!(second, first);

        let corrupt: Vec<String> = std::fs::read_dir(&folder)
            .expect("lists")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("link.yml.corrupt-"))
            .collect();
        assert_eq!(corrupt.len(), 1, "one quarantined copy: {corrupt:?}");
        assert_eq!(repo.take_quarantined(), Some("link.yml".to_string()));
        assert_eq!(repo.take_quarantined(), None);
        // A broken link has no base, so no base copy is served.
        assert_eq!(repo.read_base_file(&id, "a.yml").expect("reads"), None);
    }

    #[test]
    fn delete_removes_the_folder() {
        let (dir, repo) = repo();
        let id = LinkId::random();
        let files: [(&str, &[u8]); 1] = [("a.yml", b"a: 1\n")];
        repo.save(&record(&id, &files), &owned(&files)).expect("saves");
        repo.delete(&id).expect("deletes");
        assert!(!dir.path().join("links").join(id.as_str()).exists());
        assert_eq!(repo.get(&id).expect("gets"), None);
        repo.delete(&id).expect("a second delete is fine");
    }

    #[test]
    fn path_traversal_in_a_base_path_is_refused() {
        let (dir, repo) = repo();
        let id = LinkId::random();
        for bad in ["../evil.yml", "/abs.yml", "a/../b.yml", ".rocket/x.yml", "a\\b.yml", ""] {
            let rec = record(&id, &[]);
            // The record does not list the path, so a path check must fire first or the
            // manifest check does. Either way the save is refused and nothing escapes.
            let err = repo.save(&rec, &owned(&[(bad, b"x")])).expect_err("refused");
            assert!(matches!(err, DomainError::InvalidInput(_)), "{bad}");
            let read = repo.read_base_file(&id, bad).expect_err("refused");
            assert!(matches!(read, DomainError::InvalidInput(_)), "{bad}");
        }
        // Only the links folder may exist next to the temp root contents.
        let names: Vec<String> = std::fs::read_dir(dir.path())
            .expect("lists")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().all(|n| n == "links"), "{names:?}");
        assert!(!dir.path().join("evil.yml").exists());
    }

    #[test]
    fn a_folder_that_is_not_a_link_id_is_ignored_by_list() {
        let (dir, repo) = repo();
        std::fs::create_dir_all(dir.path().join("links").join("not-a-link")).expect("mkdir");
        std::fs::write(dir.path().join("links").join("stray.txt"), "x").expect("write");
        assert!(repo.list().expect("lists").is_empty());
    }
}
```

- [ ] **Step 2: Wire the module.** In `crates/rocket-infra/src/network/mod.rs` add `pub mod links;` (after `pub(crate) mod lock;`) and `pub use links::FsLinkRepo;` (after `pub use identity_store::KeychainDeviceIdentity;`). In `crates/rocket-infra/src/lib.rs`, in the block that starts `pub use network::{` and lists `FsBlockedDeviceRepo, FsManualAddressRepo,` add `FsLinkRepo,` in sorted position. Run `cargo test -j4 -p rocket-infra network::links`. Expected: the build succeeds and the tests FAIL with `not built`.

- [ ] **Step 3: GREEN, the implementation.** Code in `links.rs` (above the tests):

```rust
//! Links of the sync feature: one folder per link under `~/.rocket-api/network/links/`.
//!
//! `<link_id>/link.yml` is the `LinkRecord`. `<link_id>/base/<path>` holds one copy of each
//! file at its agreed version. Machine-local: never inside a workspace or collection.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use rocket_collection::file_tx::validate_rel_path;
use rocket_network::device::DeviceId;
use rocket_network::link::{LinkId, LinkRecord, LinkState, Manifest};
use rocket_network::ports::{LinkRepository, QuarantineReport};
use rocket_shared::error::{DomainError, DomainResult};
use sha2::{Digest, Sha256};

use crate::atomic_write;
use crate::network::lock::lock_ignore_poison;
use crate::yaml_io::quarantine_corrupt;

pub struct FsLinkRepo {
    dir: PathBuf,
    store: Mutex<()>,
    quarantined: Mutex<Option<String>>,
}
```

Methods and their rules (each rule is pinned by a test above):

  - `new(dir)` stores the path and touches no disk.
  - `folder(id)` is `dir.join(id.as_str())`. `base_path(id, path)` calls `validate_rel_path(path)?` (an `InvalidInput` for an empty, absolute, `..`, backslash, colon, device-name or `.rocket` path) and joins `folder(id)/base` with each `/` segment pushed separately.
  - `read_record(&self, id)` (the lock is already held): read `folder/link.yml` with `fs::read`. `NotFound` is `Ok(None)`. Parse with `serde_yaml::from_slice::<LinkRecord>`; keep it only when `record.link_id == *id`. Any other read error is `DomainError::Io("Failed to read link.yml: ...")`. Bytes that do not parse (or a parse with another id) are damaged: call `quarantine_corrupt(&path)` (map an error to `DomainError::Io`), build the stub below, write it with `atomic_write`, remember the name `"link.yml"` in `quarantined`, log `tracing::warn!(link = %id.as_str(), "link file was damaged and has been set aside")` (never the content), and return `Some(stub)`.
  - The stub: parse the damaged bytes (lossy UTF-8) as `serde_yaml::Value`; `peer_device_id` from its `peer_device_id` string with `DeviceId::parse_base32`, else `DeviceId::from_bytes([0u8; 32])`; `collection_name` from its string, else `"Unknown collection"`; `collection_root` likewise else `""`; `collection_uid` a non-empty string else `None`; `base: Manifest::new()`; `state: LinkState::Broken`; `updated_at` its string, else `""`.
  - `get` locks, calls `read_record`. `list` locks, reads the folder names of `dir` (a missing `dir` is an empty list), keeps those where `LinkId::parse(name)` is `Ok` and the entry is a directory, calls `read_record` for each, drops `None`, and sorts by `link_id.as_str()`.
  - `save(record, base_files)`: before any write, for each `(path, bytes)` run `validate_rel_path(path)?`, require `record.base.get(path) == Some(sha256 hex of bytes)` (else `InvalidInput("A base copy does not match the manifest.")`). Then lock; write each copy with `atomic_write` (it creates the parents); then write `link.yml` with `atomic_write` (`serde_yaml::to_string(record)`, a failure is `DomainError::Serialization`); then prune: walk `folder/base`, remove every file whose relative path is not a key of `record.base`, then remove empty directories bottom-up, ignoring errors. An `io::Error` becomes `DomainError::Io("Failed to write the link: ...")`.
  - `delete(id)`: lock, `fs::remove_dir_all(folder(id))`; `NotFound` is fine.
  - `read_base_file(id, path)`: `base_path` first (so a bad path is refused before anything is read). Lock. `read_record` gives the record, `None` or a record without the path in `base` gives `Ok(None)`. Read the copy; `NotFound` gives `Ok(None)`. If its SHA-256 differs from `record.base.get(path)`, log a warning (path only) and return `Ok(None)`. Else `Ok(Some(bytes))`.
  - `QuarantineReport::take_quarantined` is `lock_ignore_poison(&self.quarantined).take()`.

  Every `Mutex` lock uses `lock_ignore_poison`.

- [ ] **Step 4: GREEN check.** Run `cargo test -j4 -p rocket-infra network::links`. Expected: `test result: ok.` with 11 passed. Then `cargo test -j4 -p rocket-infra network::` (the other network tests still pass) and `cargo clippy -j4 -p rocket-infra -- -D warnings` (no warnings).

- [ ] **Step 5: Docs.** In `crates/rocket-infra/CLAUDE.md` add a row to the public types table: `FsLinkRepo | rocket_network::LinkRepository, QuarantineReport | Sync links at <network dir>/links/<link_id>/: link.yml (snake_case, atomic_write, one store mutex) and base/<path> copies (hash-checked on save and on read). save takes a delta of base copies and prunes paths that left the manifest. A damaged link.yml is renamed link.yml.corrupt-<secs> once and the link reads as a Broken stub. A bad base path is InvalidInput. |`.

---

## Task 3: `rocket-app`, `SyncCollectionPort`, the manifest builder, the apply and the fakes

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:** the nine `rocket-app` rows of the table.

**Interfaces**

Consumes: `CollectionExportService::export_bruno(collection, false)` and the new `raw_files`; `rocket_export::{merge_file, zip_entries, is_synced_path, SourceFile}`; `rocket_network::link::Manifest`; `rocket_collection::{CollectionRepository, CollectionFileTxFactory, RecoveryOutcome, validate_rel_path}`; `crate::collection_lock::collection_lock` and `with_collection_lock`; `rocket_import::ImportFiles`.

Produces (the index shapes, plus ruling 4):

```rust
// sync_ports.rs
pub struct SyncFile { pub path: String, pub bytes: Vec<u8> }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChange {
    Put { path: String, bytes: Vec<u8> },
    PutAs { path: String, bytes: Vec<u8> },
    Delete { path: String },
    Rename { from: String, to: String },
}
pub trait SyncCollectionPort: Send + Sync {
    fn exists(&self, collection: &str) -> DomainResult<bool>;
    fn collection_uid(&self, collection: &str) -> DomainResult<Option<String>>;
    fn manifest(&self, collection: &str) -> DomainResult<Manifest>;
    fn export_files(&self, collection: &str, paths: &[String]) -> DomainResult<Vec<SyncFile>>;
    fn apply(&self, collection: &str, changes: &[FileChange]) -> DomainResult<()>;
}
pub struct ZipBase { pub manifest: Manifest, pub files: Vec<(String, Vec<u8>)> }
pub fn sha256_hex(bytes: &[u8]) -> String;
pub fn base_from_reader<R: std::io::Read + std::io::Seek>(reader: R) -> DomainResult<ZipBase>;
pub fn base_from_zip(files: &dyn ImportFiles, zip_path: &Path) -> DomainResult<ZipBase>;
pub fn manifest_from_zip(files: &dyn ImportFiles, zip_path: &Path) -> DomainResult<Manifest>;
// sync_collections.rs
pub struct ExportBackedSyncCollections { /* export, collections, tx */ }
impl ExportBackedSyncCollections {
    pub fn new(
        export: Arc<CollectionExportService>,
        collections: Arc<dyn CollectionRepository>,
        tx: Arc<dyn CollectionFileTxFactory>,
    ) -> ExportBackedSyncCollections;
}
// collection_export_service.rs
impl CollectionExportService { pub fn raw_files(&self, collection: &str) -> DomainResult<Vec<SourceFile>>; }
```

Semantics of `apply` (changes are planned first against an overlay of the local files, so a failing change touches nothing; then staged in one transaction):

  - Every path (`path`, `from`, `to`) must pass `is_synced_path` and `validate_rel_path`, else `InvalidInput("The sync names a file it may not write.")`.
  - `Put`: for `.yml`/`.yaml` the bytes written are `merge_file(local_bytes_or_none, bytes)?`; other extensions are written as received. A path the overlay does not hold is a new file, written as received.
  - `PutAs`: the path must be free, else `AlreadyExists`. Free means: if the overlay has an entry for it (put or deleted earlier in this call) use that, else `CollectionRepository::path_exists` is false. Written as received (a `.yml` is still checked with `merge_file(None, ..)`).
  - `Delete`: stages a delete (a missing file is fine).
  - `Rename`: `from` must be in the overlay (else `NotFound`), `to` must be free (same rule as `PutAs`, else `AlreadyExists`). Stages a put of the local bytes at `to` and a delete of `from`. The local bytes move as they are, so the local secrets travel with the file.
  - The whole call holds `collection_lock(repo, root)`, then `tx.begin(root)`, stages, `commit`. A staging error calls `abort`. A commit error other than `Conflict` calls `recover`: `Finished` means the apply completed and is `Ok`; anything else is `Err(Internal("<error>; nothing was changed"))`.
  - An empty change list returns `Ok(())` without opening a transaction.

`manifest` and `export_files` run `export_bruno(collection, false)` under the same lock, then `zip_entries`. `manifest` hashes each entry with SHA-256 (lowercase hex) into `Manifest::insert`; an insert error (bad path, too many files) is returned as is. `export_files` returns the requested paths in the requested order; a path the export does not carry is `NotFound("A requested file is not in the collection.")`.

### Steps

- [ ] **Step 1: Read.** 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Read `crates/rocket-app/src/collection_lock.rs`, `crates/rocket-app/src/openapi_sync_apply.rs` (the `begin`, `stage_all`, `commit`, `recover` block), and `crates/rocket-app/src/network/transfer_fakes.rs` (fake style).

- [ ] **Step 2: RED, `raw_files` test.** In `crates/rocket-app/src/collection_export_service.rs`, in the `tests` module (after the `export_bruno_names_the_file` test), add:

```rust
    #[test]
    fn raw_files_returns_the_unscrubbed_files() {
        let service = CollectionExportService::new(Box::new(OneFile));
        let files = service.raw_files("my-api").expect("lists");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].rel_path, "opencollection.yml");
    }
```

  Then add the method, anchored on the text `    pub fn export_bruno(\n        &self,\n        collection: &str,\n        include_environments: bool,\n    ) -> DomainResult<ExportOutput> {`, inserted just before it:

```rust
    /// The raw files of the collection as the file source lists them, secrets included. The
    /// sync apply reads the local files through this to merge a received file into them. It
    /// takes no lock, so the caller holds the collection lock.
    pub fn raw_files(&self, collection: &str) -> DomainResult<Vec<rocket_export::SourceFile>> {
        self.files.list_files(collection)
    }
```

  Run `cargo test -j4 -p rocket-app raw_files_returns`. Expected: PASS (the test is written with the code in one step because the method is one line).

- [ ] **Step 3: RED, the port types and ZIP base tests.** Create `crates/rocket-app/src/network/sync_ports.rs` with the types and trait of "Produces", `sha256_hex` and stubs `base_from_reader` returning `Err(DomainError::Internal("not built".into()))` (so `base_from_zip` and `manifest_from_zip` are real wrappers), and these tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_infra::FsImportFiles;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn make_zip(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut w = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, body) in entries {
            w.start_file(*name, SimpleFileOptions::default())
                .expect("start");
            w.write_all(body.as_bytes()).expect("write");
        }
        w.finish().expect("finish").into_inner()
    }

    #[test]
    fn a_zip_base_has_collection_relative_paths_and_hashes() {
        let zip = make_zip(&[
            ("Orders/opencollection.yml", "a: 1\n"),
            ("Orders/users/get.yml", "b: 2\n"),
        ]);
        let base = base_from_reader(Cursor::new(zip)).expect("reads");
        let paths: Vec<&str> = base.manifest.paths().collect();
        assert_eq!(paths, vec!["opencollection.yml", "users/get.yml"]);
        assert_eq!(base.manifest.get("users/get.yml"), Some(sha256_hex(b"b: 2\n").as_str()));
        assert_eq!(base.files[1], ("users/get.yml".to_string(), b"b: 2\n".to_vec()));
    }

    #[test]
    fn a_zip_base_skips_paths_the_sync_does_not_carry() {
        // A first send with "include environments" puts environments in the ZIP.
        let zip = make_zip(&[
            ("Orders/opencollection.yml", "a: 1\n"),
            ("Orders/environments/dev.yml", "name: dev\n"),
            ("Orders/notes.txt", "x"),
        ]);
        let base = base_from_reader(Cursor::new(zip)).expect("reads");
        let paths: Vec<&str> = base.manifest.paths().collect();
        assert_eq!(paths, vec!["opencollection.yml"]);
        assert_eq!(base.files.len(), 1);
    }

    #[test]
    fn a_zip_base_reads_a_file_through_the_import_port() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("sent.zip");
        std::fs::write(&path, make_zip(&[("Orders/a.yml", "a: 1\n")])).expect("write");
        let manifest = manifest_from_zip(&FsImportFiles, &path).expect("reads");
        assert_eq!(manifest.len(), 1);
        let base = base_from_zip(&FsImportFiles, &path).expect("reads");
        assert_eq!(base.manifest, manifest);
    }

    #[test]
    fn a_zip_that_is_not_a_zip_is_refused() {
        let err = base_from_reader(Cursor::new(b"nope".to_vec())).expect_err("refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }
}
```

  Wire the modules in `crates/rocket-app/src/network/mod.rs`: add `pub mod sync_collections;` and `pub mod sync_ports;` after `mod status;` (keep the list sorted), add `#[cfg(test)] #[allow(dead_code)] pub(crate) mod sync_fakes;` after the `transfer_fakes` block, and `#[cfg(test)] #[path = "sync_collections_tests.rs"] mod sync_collections_tests;` after `service_tests`. Add re-exports at the end: `pub use sync_collections::ExportBackedSyncCollections;` and `pub use sync_ports::{base_from_reader, base_from_zip, manifest_from_zip, sha256_hex, FileChange, SyncCollectionPort, SyncFile, ZipBase};`. Create empty-stub files for `sync_collections.rs`, `sync_collections_tests.rs` and `sync_fakes.rs` with a one-line module doc so the crate builds. Run `cargo test -j4 -p rocket-app network::sync_ports`. Expected: FAIL with `not built` (the `not a zip` test already fails on the stub too).

- [ ] **Step 4: GREEN, the ZIP base.** In `sync_ports.rs`:

```rust
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

pub fn base_from_reader<R: Read + Seek>(reader: R) -> DomainResult<ZipBase> {
    let mut manifest = Manifest::new();
    let mut files = Vec::new();
    for (path, bytes) in rocket_export::zip_entries(reader)? {
        if !rocket_export::is_synced_path(&path) {
            continue;
        }
        manifest.insert(&path, &sha256_hex(&bytes))?;
        files.push((path, bytes));
    }
    Ok(ZipBase { manifest, files })
}

pub fn base_from_zip(files: &dyn ImportFiles, zip_path: &Path) -> DomainResult<ZipBase> {
    let reader = files.open_read(zip_path)?;
    base_from_reader(reader)
}

pub fn manifest_from_zip(files: &dyn ImportFiles, zip_path: &Path) -> DomainResult<Manifest> {
    base_from_zip(files, zip_path).map(|base| base.manifest)
}
```

  (`open_read` returns `std::io::Result`, which `?` maps to `DomainError::Io`. `zip_entries` already returns `InvalidInput` for a bad archive.) Add the `use` lines: `std::io::{Read, Seek}`, `std::path::Path`, `rocket_import::ImportFiles`, `rocket_network::link::Manifest`, `rocket_shared::error::{DomainError, DomainResult}`. Run `cargo test -j4 -p rocket-app network::sync_ports`. Expected: 4 passed.

- [ ] **Step 5: RED, the port tests over a real folder.** Fill `crates/rocket-app/src/network/sync_collections_tests.rs`:

```rust
//! `ExportBackedSyncCollections` over a real folder, a real export and the real journaled
//! file transaction.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rocket_collection::{CollectionFileTxFactory, CollectionRepository};
use rocket_export::{bruno_zip, zip_entries, SourceFile};
use rocket_infra::{FsCollectionFileSource, FsCollectionRepo, FsFileTxFactory};
use rocket_shared::error::DomainError;
use tempfile::TempDir;

use super::sync_collections::ExportBackedSyncCollections;
use super::sync_ports::{base_from_reader, sha256_hex, FileChange, SyncCollectionPort};
use crate::collection_export_service::CollectionExportService;
use crate::collection_lock::collection_lock;

const OC: &str = "opencollection: \"1.0.0\"\nuid: col-uid-1\ninfo:\n  name: c\n";
const LOGIN: &str = "uid: req-uid-1\ninfo:\n  name: Login\n  type: http\nhttp:\n  method: POST\n  url: https://api.test/login\n  headers:\n    - name: Authorization\n      value: Bearer LOCALSECRET\n    - name: Accept\n      value: application/json\nexamples:\n  - name: ok\n";
const FOLDER: &str = "info:\n  name: users\n  type: folder\n";
const GET: &str = "info:\n  name: Get\n  type: http\nhttp:\n  method: GET\n  url: https://api.test/users\n";

struct Rig {
    _ws: TempDir,
    port: Arc<ExportBackedSyncCollections>,
    repo: Arc<FsCollectionRepo>,
    export: Arc<CollectionExportService>,
    root: PathBuf,
}

fn rig_with(tx: Arc<dyn CollectionFileTxFactory>) -> Rig {
    let ws = TempDir::new().expect("tempdir");
    let collections = ws.path().join("collections");
    let root = collections.join("c");
    std::fs::create_dir_all(&root).expect("mkdir");
    let repo = Arc::new(FsCollectionRepo::new_standalone(collections));
    let source = FsCollectionFileSource::new(Arc::new(Mutex::new(ws.path().to_path_buf())));
    let export = Arc::new(CollectionExportService::new(Box::new(source)));
    let port = Arc::new(ExportBackedSyncCollections::new(
        Arc::clone(&export),
        repo.clone() as Arc<dyn CollectionRepository>,
        tx,
    ));
    Rig { _ws: ws, port, repo, export, root }
}

fn rig() -> Rig {
    rig_with(Arc::new(FsFileTxFactory::new()))
}

fn write(rig: &Rig, rel: &str, text: &str) {
    let path = rig.root.join(rel);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, text).expect("write");
}

fn read(rig: &Rig, rel: &str) -> String {
    std::fs::read_to_string(rig.root.join(rel)).expect("read")
}

fn seed(rig: &Rig) {
    write(rig, "opencollection.yml", OC);
    write(rig, "Login.yml", LOGIN);
    write(rig, "users/folder.yml", FOLDER);
    write(rig, "users/Get.yml", GET);
}

/// Every file under the root except the transaction's own `.rocket` folder.
fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, prefix: &str, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).expect("lists") {
            let entry = entry.expect("entry");
            let name = entry.file_name().to_string_lossy().into_owned();
            if prefix.is_empty() && name == ".rocket" {
                continue;
            }
            let rel = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
            if entry.file_type().expect("type").is_dir() {
                walk(&entry.path(), &rel, out);
            } else {
                out.insert(rel, std::fs::read(entry.path()).expect("read"));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, "", &mut out);
    out
}

/// The bytes the export puts in the ZIP for one raw file, as a peer would send them.
fn scrubbed(rel: &str, raw: &str) -> Vec<u8> {
    let file = SourceFile { rel_path: rel.to_string(), bytes: raw.as_bytes().to_vec() };
    let (zip, _) = bruno_zip(vec![file], "c", false).expect("exports");
    zip_entries(Cursor::new(zip)).expect("entries").remove(0).1
}

#[test]
fn exists_and_collection_uid_follow_the_collection() {
    let rig = rig();
    seed(&rig);
    assert!(rig.port.exists("c").expect("exists"));
    assert!(!rig.port.exists("missing").expect("exists"));
    assert_eq!(rig.port.collection_uid("c").expect("uid"), Some("col-uid-1".to_string()));
}

#[test]
fn manifest_is_stable_across_two_calls() {
    let rig = rig();
    seed(&rig);
    let first = rig.port.manifest("c").expect("manifest");
    let second = rig.port.manifest("c").expect("manifest");
    assert_eq!(first, second);
    assert_eq!(first.base_hash(), second.base_hash());
}

#[test]
fn manifest_paths_have_no_root_folder_and_skip_files_the_sync_does_not_carry() {
    let rig = rig();
    seed(&rig);
    write(&rig, "environments/dev.yml", "name: dev\nvariables: []\n");
    write(&rig, "flows/f.yml", "name: f\n");
    write(&rig, ".rocket/contracts/c.yml", "a: 1\n");
    write(&rig, "notes.txt", "x");
    write(&rig, ".git/config", "x");
    let manifest = rig.port.manifest("c").expect("manifest");
    let paths: Vec<&str> = manifest.paths().collect();
    assert_eq!(
        paths,
        vec!["Login.yml", "opencollection.yml", "users/Get.yml", "users/folder.yml"]
    );
}

#[test]
fn a_secret_change_does_not_change_the_manifest() {
    let rig = rig();
    seed(&rig);
    let before = rig.port.manifest("c").expect("manifest");
    write(&rig, "Login.yml", &LOGIN.replace("LOCALSECRET", "OTHERSECRET"));
    let after_secret = rig.port.manifest("c").expect("manifest");
    assert_eq!(before, after_secret);
    // A real change to the same file does change that path and no other.
    write(&rig, "Login.yml", &LOGIN.replace("api.test/login", "api.test/signin"));
    let after_url = rig.port.manifest("c").expect("manifest");
    assert_ne!(before.get("Login.yml"), after_url.get("Login.yml"));
    assert_eq!(before.get("users/Get.yml"), after_url.get("users/Get.yml"));
}

#[test]
fn the_first_send_zip_gives_the_same_manifest() {
    let rig = rig();
    seed(&rig);
    write(&rig, "environments/dev.yml", "name: dev\nvariables: []\n");
    // The first send may include environments. The base must not.
    let sent = rig.export.export_bruno("c", true).expect("exports").bytes;
    let base = base_from_reader(Cursor::new(sent)).expect("base");
    assert_eq!(base.manifest, rig.port.manifest("c").expect("manifest"));
}

#[test]
fn export_files_returns_scrubbed_bytes_in_the_asked_order() {
    let rig = rig();
    seed(&rig);
    let files = rig
        .port
        .export_files("c", &["users/Get.yml".to_string(), "Login.yml".to_string()])
        .expect("exports");
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["users/Get.yml", "Login.yml"]);
    let login = String::from_utf8(files[1].bytes.clone()).expect("utf8");
    assert!(!login.contains("LOCALSECRET"));
    assert!(!login.contains("req-uid-1"));
    let manifest = rig.port.manifest("c").expect("manifest");
    assert_eq!(manifest.get("Login.yml"), Some(sha256_hex(&files[1].bytes).as_str()));
}

#[test]
fn export_files_refuses_a_path_the_collection_does_not_carry() {
    let rig = rig();
    seed(&rig);
    let err = rig.port.export_files("c", &["nope.yml".to_string()]).expect_err("refused");
    assert!(matches!(err, DomainError::NotFound(_)));
}

#[test]
fn apply_put_merges_and_keeps_local_secret_and_uid() {
    let rig = rig();
    seed(&rig);
    let peer = "info:\n  name: Login\n  type: http\nhttp:\n  method: POST\n  url: https://api.test/v2/login\n  headers:\n    - name: Authorization\n      value: Bearer PEERSECRET\n    - name: Accept\n      value: application/json\n";
    let received = scrubbed("Login.yml", peer);
    rig.port
        .apply("c", &[FileChange::Put { path: "Login.yml".into(), bytes: received.clone() }])
        .expect("applies");
    let text = read(&rig, "Login.yml");
    assert!(text.contains("https://api.test/v2/login"));
    assert!(text.contains("LOCALSECRET"));
    assert!(!text.contains("PEERSECRET"));
    assert!(text.contains("uid: req-uid-1"));
    assert!(text.contains("examples"));
    // Both sides now hash alike: the manifest entry is the hash of the received bytes.
    let manifest = rig.port.manifest("c").expect("manifest");
    assert_eq!(manifest.get("Login.yml"), Some(sha256_hex(&received).as_str()));
}

#[test]
fn apply_put_of_a_new_file_writes_it_as_received() {
    let rig = rig();
    seed(&rig);
    let received = scrubbed("users/New.yml", "info:\n  name: New\n  type: http\nhttp:\n  method: GET\n  url: https://x\n");
    rig.port
        .apply("c", &[FileChange::Put { path: "users/New.yml".into(), bytes: received.clone() }])
        .expect("applies");
    assert_eq!(std::fs::read(rig.root.join("users/New.yml")).expect("read"), received);
}

#[test]
fn apply_put_as_writes_as_received_and_refuses_an_existing_path() {
    let rig = rig();
    seed(&rig);
    let received = scrubbed("Login.yml", "info:\n  name: Login (from peer)\n  type: http\n");
    rig.port
        .apply("c", &[FileChange::PutAs { path: "Login (from peer).yml".into(), bytes: received.clone() }])
        .expect("applies");
    assert_eq!(std::fs::read(rig.root.join("Login (from peer).yml")).expect("read"), received);
    // Mine stays.
    assert!(read(&rig, "Login.yml").contains("LOCALSECRET"));

    let before = snapshot(&rig.root);
    let err = rig
        .port
        .apply("c", &[FileChange::PutAs { path: "login.yml".into(), bytes: received }])
        .expect_err("a case variant is taken");
    assert!(matches!(err, DomainError::AlreadyExists(_)));
    assert_eq!(snapshot(&rig.root), before);
}

#[test]
fn apply_rename_moves_the_file_with_its_local_content() {
    let rig = rig();
    seed(&rig);
    rig.port
        .apply("c", &[FileChange::Rename { from: "Login.yml".into(), to: "auth/Login.yml".into() }])
        .expect("applies");
    assert!(!rig.root.join("Login.yml").exists());
    let moved = read(&rig, "auth/Login.yml");
    assert!(moved.contains("LOCALSECRET"));
    assert!(moved.contains("uid: req-uid-1"));
}

#[test]
fn apply_rename_refuses_a_missing_source_and_a_taken_target() {
    let rig = rig();
    seed(&rig);
    let before = snapshot(&rig.root);
    let missing = rig
        .port
        .apply("c", &[FileChange::Rename { from: "Nope.yml".into(), to: "X.yml".into() }])
        .expect_err("refused");
    assert!(matches!(missing, DomainError::NotFound(_)));
    let taken = rig
        .port
        .apply("c", &[FileChange::Rename { from: "Login.yml".into(), to: "users/Get.yml".into() }])
        .expect_err("refused");
    assert!(matches!(taken, DomainError::AlreadyExists(_)));
    assert_eq!(snapshot(&rig.root), before);
}

#[test]
fn apply_delete_removes_the_file() {
    let rig = rig();
    seed(&rig);
    rig.port
        .apply("c", &[FileChange::Delete { path: "users/Get.yml".into() }])
        .expect("applies");
    assert!(!rig.root.join("users/Get.yml").exists());
    assert!(rig.root.join("Login.yml").exists());
}

#[test]
fn apply_with_a_bad_change_changes_nothing() {
    let rig = rig();
    seed(&rig);
    let before = snapshot(&rig.root);
    let changes = vec![
        FileChange::Put { path: "Login.yml".into(), bytes: scrubbed("Login.yml", "info:\n  name: Login\n  type: http\n") },
        FileChange::Delete { path: "users/Get.yml".into() },
        FileChange::Rename { from: "Missing.yml".into(), to: "Other.yml".into() },
    ];
    assert!(rig.port.apply("c", &changes).is_err());
    assert_eq!(snapshot(&rig.root), before);
}

#[test]
fn apply_with_invalid_yaml_changes_nothing() {
    let rig = rig();
    seed(&rig);
    let before = snapshot(&rig.root);
    let changes = vec![
        FileChange::Put { path: "Login.yml".into(), bytes: scrubbed("Login.yml", "info:\n  name: Login\n  type: http\n") },
        FileChange::Put { path: "users/Get.yml".into(), bytes: b"a: [1, 2".to_vec() },
    ];
    let err = rig.port.apply("c", &changes).expect_err("refused");
    assert!(matches!(err, DomainError::InvalidInput(_)));
    assert_eq!(snapshot(&rig.root), before);
}

#[test]
fn apply_refuses_paths_the_export_does_not_carry() {
    let rig = rig();
    seed(&rig);
    write(&rig, "environments/prod.yml", "name: prod\nvariables:\n  - name: k\n    value: SECRETVALUE\n");
    let before = snapshot(&rig.root);
    let body = b"a: 1\n".to_vec();
    for bad in ["environments/prod.yml", ".rocket/x.yml", "../x.yml", "flows/a.yml", "workspace.yml", "notes.txt", "/abs.yml", ""] {
        let put = rig.port.apply("c", &[FileChange::Put { path: bad.into(), bytes: body.clone() }]);
        assert!(matches!(put, Err(DomainError::InvalidInput(_))), "put {bad}");
        let delete = rig.port.apply("c", &[FileChange::Delete { path: bad.into() }]);
        assert!(matches!(delete, Err(DomainError::InvalidInput(_))), "delete {bad}");
    }
    let rename = rig.port.apply("c", &[FileChange::Rename { from: "Login.yml".into(), to: "environments/x.yml".into() }]);
    assert!(matches!(rename, Err(DomainError::InvalidInput(_))));
    assert_eq!(snapshot(&rig.root), before);
}

#[test]
fn an_empty_change_list_is_a_no_op() {
    let rig = rig();
    seed(&rig);
    let before = snapshot(&rig.root);
    rig.port.apply("c", &[]).expect("applies");
    assert_eq!(snapshot(&rig.root), before);
}

#[test]
fn apply_rolls_back_every_file_when_the_commit_fails_at_any_step() {
    let changes = vec![
        FileChange::Put { path: "Login.yml".into(), bytes: scrubbed("Login.yml", "info:\n  name: Login\n  type: http\nhttp:\n  url: https://changed\n") },
        FileChange::Put { path: "new/Fresh.yml".into(), bytes: scrubbed("new/Fresh.yml", "info:\n  name: Fresh\n  type: http\n") },
        FileChange::Delete { path: "users/Get.yml".into() },
    ];
    let mut failed = 0;
    for step in 0..40 {
        let rig = rig_with(Arc::new(FsFileTxFactory::with_fault(step)));
        seed(&rig);
        let before = snapshot(&rig.root);
        let result = rig.port.apply("c", &changes);
        let after = snapshot(&rig.root);
        match result {
            Err(_) => {
                failed += 1;
                assert_eq!(after, before, "a fault at step {step} left a half-applied sync");
            }
            Ok(()) => {
                assert!(after.contains_key("new/Fresh.yml"), "step {step}");
                assert!(!after.contains_key("users/Get.yml"), "step {step}");
                assert!(String::from_utf8_lossy(&after["Login.yml"]).contains("https://changed"));
            }
        }
    }
    assert!(failed >= 3, "the fault hooks should fail early steps, failed {failed}");
}

#[test]
fn apply_waits_for_the_collection_lock() {
    let rig = rig();
    seed(&rig);
    let lock = collection_lock(rig.repo.as_ref(), &rig.root);
    let guard = lock.lock();
    let done = Arc::new(AtomicBool::new(false));
    let worker = {
        let port = Arc::clone(&rig.port);
        let done = Arc::clone(&done);
        std::thread::spawn(move || {
            port.apply("c", &[FileChange::Delete { path: "users/Get.yml".into() }])
                .expect("applies");
            done.store(true, Ordering::SeqCst);
        })
    };
    std::thread::sleep(Duration::from_millis(200));
    assert!(!done.load(Ordering::SeqCst), "apply ran while the collection lock was held");
    assert!(rig.root.join("users/Get.yml").exists());
    drop(guard);
    worker.join().expect("join");
    assert!(done.load(Ordering::SeqCst));
    assert!(!rig.root.join("users/Get.yml").exists());
}
```

  Create `ExportBackedSyncCollections` as a struct whose methods return `Err(DomainError::Internal("not built".into()))` (stub), so the file builds. Run `cargo test -j4 -p rocket-app network::sync_collections`. Expected: the tests FAIL with `not built`.

- [ ] **Step 6: GREEN, `ExportBackedSyncCollections`.** In `sync_collections.rs` (imports: `std::collections::BTreeMap`, `std::io::Cursor`, `std::path::Path`, `std::sync::Arc`, `rocket_collection::{validate_rel_path, CollectionFileTxFactory, CollectionRepository, RecoveryOutcome}`, `rocket_export::{is_synced_path, merge_file, zip_entries}`, `rocket_network::link::Manifest`, `rocket_shared::error::{DomainError, DomainResult}`, `crate::collection_export_service::CollectionExportService`, `crate::collection_lock::{collection_lock, with_collection_lock}`, `super::sync_ports::{sha256_hex, FileChange, SyncCollectionPort, SyncFile}`):

```rust
pub struct ExportBackedSyncCollections {
    export: Arc<CollectionExportService>,
    collections: Arc<dyn CollectionRepository>,
    tx: Arc<dyn CollectionFileTxFactory>,
}

/// What the plan stages in the transaction.
enum Staged {
    Put(String, Vec<u8>),
    Delete(String),
}

const REFUSED: &str = "The sync names a file it may not write.";

fn checked(path: &str) -> DomainResult<()> {
    if !is_synced_path(path) {
        return Err(DomainError::InvalidInput(REFUSED.into()));
    }
    validate_rel_path(path).map_err(|_| DomainError::InvalidInput(REFUSED.into()))
}

fn is_yaml(path: &str) -> bool {
    path.ends_with(".yml") || path.ends_with(".yaml")
}
```

  Methods:
  - `repo(&self)` returns the pinned repository when there is one, as `OpenApiSyncService` does: `let pinned = self.collections.pinned(); let repo = pinned.as_deref().unwrap_or(self.collections.as_ref());` (keep both in the calling method so the borrow lives).
  - `exported_entries(&self, collection) -> DomainResult<Vec<(String, Vec<u8>)>>`: `let out = self.export.export_bruno(collection, false)?; zip_entries(Cursor::new(out.bytes))`. It does not filter, because the export without environments already carries only synced paths.
  - `exists`: `match repo.collection_root_path(collection) { Ok(_) => Ok(true), Err(DomainError::NotFound(_)) => Ok(false), Err(e) => Err(e) }`.
  - `collection_uid`: `Ok(repo.collection_identity(collection)?.uid)`.
  - `manifest`: `with_collection_lock(repo, collection, || self.exported_entries(collection))?`, then insert each `(path, sha256_hex(&bytes))` into a `Manifest::new()`.
  - `export_files`: the same locked read into a `BTreeMap`, then map `paths` in order, `NotFound("A requested file is not in the collection.")` for a miss.
  - `apply`: as in the semantics list above. The planning is a free function `plan_changes(repo, collection, local: BTreeMap<String, Vec<u8>>, changes) -> DomainResult<Vec<Staged>>` that keeps `overlay: BTreeMap<String, Option<Vec<u8>>>` (starts empty; `Some` is a put, `None` is a delete) and a `lookup` closure: overlay entry first, else `local.get`. `is_free(path)`: overlay entry decides (`None` means free, `Some` means taken); with no overlay entry, free is `!repo.path_exists(collection, path)? && !local.contains_key(path)`. Staging errors from `stage_put`/`stage_delete` call `tx.abort()` and return the error. After a failed `commit`, follow the recovery rules in the semantics list.

  Run `cargo test -j4 -p rocket-app network::sync_collections`. Expected: `test result: ok.` with 19 passed.

- [ ] **Step 7: RED then GREEN, the fakes.** Fill `crates/rocket-app/src/network/sync_fakes.rs` (test-only, `pub(crate)`):

```rust
//! In-memory fakes of the sync ports, for the `SyncService` tests of Plan 3.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use rocket_network::link::{LinkId, LinkRecord, Manifest};
use rocket_network::ports::LinkRepository;
use rocket_shared::error::{DomainError, DomainResult};

use super::sync_ports::{sha256_hex, FileChange, SyncCollectionPort, SyncFile};

struct FakeCollection {
    uid: Option<String>,
    /// The exported (scrubbed) view of each file.
    files: BTreeMap<String, Vec<u8>>,
}

/// Collections kept in memory. `apply` uses the real `merge_file` and swaps all files at
/// once, so it is atomic. `fail_next_apply` makes the next apply fail before any change.
#[derive(Default)]
pub(crate) struct FakeSyncCollections {
    state: Mutex<HashMap<String, FakeCollection>>,
    fail_apply: AtomicBool,
    applied: Mutex<Vec<(String, Vec<FileChange>)>>,
}
```

  Methods on `FakeSyncCollections` (all `pub(crate)`): `new() -> Self`, `add(&self, name: &str, uid: Option<&str>, files: &[(&str, &str)])`, `set_file(&self, name: &str, path: &str, text: &str)`, `remove_file(&self, name: &str, path: &str)`, `file(&self, name: &str, path: &str) -> Option<Vec<u8>>`, `paths(&self, name: &str) -> Vec<String>`, `fail_next_apply(&self)`, `applied(&self) -> Vec<(String, Vec<FileChange>)>`. `impl SyncCollectionPort` follows the real semantics: `manifest` hashes the stored bytes; `export_files` returns them in the asked order (`NotFound` for a miss); `apply` works on a clone and applies `Put` (`merge_file` for `.yml`/`.yaml` when the file exists), `PutAs` (`AlreadyExists` when taken), `Delete`, `Rename` (`NotFound` / `AlreadyExists`), and replaces the stored map only when every change worked; it records the call in `applied` even when it fails. A collection that was never added is `NotFound` for every method except `exists` (false).

  `FakeLinkRepo { links: Mutex<HashMap<String, (LinkRecord, HashMap<String, Vec<u8>>)>> }` implements `LinkRepository` with the delta rule of ruling 5 (write the given copies, drop copies whose path left `record.base`), `list` sorted by link id, `read_base_file` returns the stored copy when present. Plus `pub(crate) fn new() -> Self`.

  Tests, in a `#[cfg(test)] mod tests` inside `sync_fakes.rs`:

```rust
    #[test]
    fn the_fake_apply_merges_like_the_real_one_and_is_atomic() {
        let fake = FakeSyncCollections::new();
        fake.add("c", Some("u"), &[("a.yml", "http:\n  url: https://a\n  token: s\n"), ("b.yml", "x: 1\n")]);
        let received = b"http:\n  url: https://b\n  token: ''\n".to_vec();
        fake.apply("c", &[FileChange::Put { path: "a.yml".into(), bytes: received }]).expect("applies");
        let text = String::from_utf8(fake.file("c", "a.yml").expect("file")).expect("utf8");
        assert!(text.contains("https://b") && text.contains("token: s"));

        let before = fake.file("c", "b.yml");
        let failing = [
            FileChange::Delete { path: "b.yml".into() },
            FileChange::Rename { from: "gone.yml".into(), to: "z.yml".into() },
        ];
        assert!(fake.apply("c", &failing).is_err());
        assert_eq!(fake.file("c", "b.yml"), before);

        fake.fail_next_apply();
        assert!(fake.apply("c", &[FileChange::Delete { path: "b.yml".into() }]).is_err());
        assert!(fake.file("c", "b.yml").is_some());
        fake.apply("c", &[FileChange::Delete { path: "b.yml".into() }]).expect("applies");
        assert!(fake.file("c", "b.yml").is_none());
    }

    #[test]
    fn the_fake_manifest_hashes_the_stored_bytes() {
        let fake = FakeSyncCollections::new();
        fake.add("c", None, &[("a.yml", "a: 1\n")]);
        let manifest = fake.manifest("c").expect("manifest");
        assert_eq!(manifest.get("a.yml"), Some(sha256_hex(b"a: 1\n").as_str()));
        assert!(!fake.exists("nope").expect("exists"));
    }
```

  and a `FakeLinkRepo` round trip test `the_fake_link_repo_keeps_a_delta_and_prunes` (save with one copy, save state-only with `&[]`, read the copy; save a record without that path, copy gone). Build the `LinkRecord` as in the `FsLinkRepo` tests (`DeviceId::from_bytes([7u8; 32])`). Run `cargo test -j4 -p rocket-app network::sync_fakes`. Expected: 3 passed.

- [ ] **Step 8: Verify the crate.** Run, one after the other:
  - `cargo check -j4 -p rocket-app` (no warnings about unused imports),
  - `cargo test -j4 -p rocket-app network::sync_` (expected: `test result: ok.`, 26 passed: 4 ports, 19 collections, 3 fakes),
  - `cargo test -j4 -p rocket-app network::` (the transfer and pairing tests still pass),
  - `cargo clippy -j4 -p rocket-app -- -D warnings` (no warnings).

- [ ] **Step 9: Layering check.** Run `grep -n "rocket_infra\|std::fs" crates/rocket-app/src/network/sync_ports.rs crates/rocket-app/src/network/sync_collections.rs`. Expected: no output (the test file and the fakes may use infra types; these two production files may not).

- [ ] **Step 10: Docs.** In `crates/rocket-app/CLAUDE.md` add a row to the Public Types table: `ExportBackedSyncCollections | The real SyncCollectionPort (src/network/sync_collections.rs). manifest and export_files run export_bruno(collection, false) under the collection lock and strip the root folder (zip_entries), so a manifest hash is the hash of the scrubbed export bytes. apply plans every change against the local files first (only is_synced_path paths; Put merges YAML through rocket_export::merge_file; PutAs and Rename need a free target), then stages them in one CollectionFileTx under collection_lock and recovers a failed commit. Methods block: callers use spawn_blocking. |` and one for `sync_ports.rs` (`manifest_from_zip`, `base_from_zip`, `base_from_reader`: the base of a link from the sent ZIP, environments left out).

- [ ] **Step 11: Whole-plan verification, then stop.** Run each command; every one must end green:

```bash
cargo check -j4 -p rocket-export
cargo test -j4 -p rocket-export
cargo clippy -j4 -p rocket-export -- -D warnings
cargo check -j4 -p rocket-infra
cargo test -j4 -p rocket-infra network::
cargo clippy -j4 -p rocket-infra -- -D warnings
cargo check -j4 -p rocket-app
cargo test -j4 -p rocket-app network::
cargo clippy -j4 -p rocket-app -- -D warnings
cargo fmt -p rocket-export -p rocket-infra -p rocket-app -- --check
```

  Expected: every `cargo check` finishes with `Finished`, every test run ends `test result: ok.` with 0 failed, clippy prints no warnings, `cargo fmt --check` prints nothing. There is no frontend change in this plan, so `bun run tsc --noEmit` and `bun run check` are not needed here (Plan 4 and 5 run them).

  Record one `Ruling:` line per ruling above in `.superpowers/sdd/2026-11-08-sync-plan-02/progress.md`.

  No commit; reviewed at the plan end.
