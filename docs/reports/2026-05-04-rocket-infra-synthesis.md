# 🔹 SYNTHESIS — `rocket-infra` Code Review

## 1. Executive Summary (Top Risks)

The crate is structurally sound but carries **two crate-wide hazards and one growing god-module problem**:

1. **Path-traversal + symlink-following in destructive ops** is the single most serious finding. `Collection::validate_name` is enforced only on `create`/`rename`; every other entry-point joins the user-supplied `name` into `base_dir` directly, and `delete`/`delete_folder` call `fs::remove_dir_all` which follows symlinks. A malicious imported workspace (or attacker-controlled IPC argument) can wipe arbitrary user directories.
2. **Domain logic has leaked into `fs_collection_repo`** — folder-chain variable merge semantics, UID minting, filename-collision policy, and "default-on-corrupt" data-loss behavior are all encoded in infra. This violates the project's stated DDD contract and means the same rules cannot be reused or independently tested.
3. **`fs_collection_repo.rs` (1546 lines) and `oc_conversions.rs` (2516 lines) are god modules** with high duplication across the other `fs_*_repo.rs` siblings. ~250 lines of YAML I/O boilerplate are copy-pasted four-plus times.

Performance is acceptable for current scale but degrades sharply past ~1000 requests because `Collection::get` eagerly loads every request, `save_request` deep-clones the whole `Request`, and `serde_json::Value` round-trips dominate the conversion layer.

---

## 2. Critical Architectural Violations

| # | File:Line | Violation |
|---|-----------|-----------|
| A1 | `fs_collection_repo.rs:617-659` | Folder-chain variable merge ("inner wins, disabled doesn't shadow") is a **domain rule** living in infra. Belongs in `rocket-collection` or `rocket-environment::resolve`. |
| A2 | `oc_conversions.rs:933`, `fs_collection_repo.rs:418` | UID minting on deserialize/folder-create is a domain invariant — should live in `Request::new` / `Folder::new`. |
| A3 | `fs_collection_repo.rs:303-372` | Filename uniqueness policy (`"{stem} {n}.yml"`, 9999 cap) is a user-facing identifier rule disguised as infra. |
| A4 | `lib.rs:4-5` | `pub mod opencollection` and `pub mod oc_conversions` should be `pub(crate)` per CLAUDE.md; ACL types are leaking. |
| A5 | `Cargo.toml:21` + `tauri_tracing_layer.rs` | `tauri = "2"` dep in a "pure infra" crate — Tauri-aware tracing belongs in `src-tauri`. |
| A6 | `Cargo.toml:7` | `rocket-audit` uses `path =` while every other crate is `.workspace = true`. |
| A7 | `oc_conversions.rs:1057` (`ProtocolRequest`) | Effectively-domain enum exported from infra; either pull into `rocket-collection` or hide behind an `OcCodec` facade. |

---

## 3. Performance Risks

| # | Site | Impact |
|---|------|--------|
| P1 | `fs_collection_repo.rs:322` — `request_to_oc_http_request(request.clone())` | Full deep-clone of `Request` (incl. multi-MB bodies) on every save. **Highest user-visible win.** |
| P2 | `fs_collection_repo.rs:809-892` — `build_folder_tree` reads + parses every request file | ~1 s cold load at 5000 requests; serde_yaml is 5-10× slower than serde_json. |
| P3 | `fs_collection_repo.rs:815, 820-826` | `folder.yml` parsed twice per folder (UID read + tree build). |
| P4 | `fs_collection_repo.rs:617-659` — `get_folder_chain_variables` | Re-reads all ancestor folder.yml files on every request execution; no cache. |
| P5 | `oc_conversions.rs:910, 921, 1012, 1030` — `serde_json::Value` round-trips for `variables`/`examples`/`client_certificates` | Per-call allocation; root cause is `Vec<serde_json::Value>` placeholders in domain. |
| P6 | `fs_history_repo.rs:24-44` — `list()` parses every history file before truncating to limit | 100× speedup possible by sorting dir entries by mtime first (mirror `search()`). |
| P7 | `atomic_write.rs:36` — per-file `sync_data` | Bruno import of 5000 requests → 30-60 s of fsyncs on spinning disks; needs a bulk path. |
| P8 | `fs_collection_repo.rs:843, 851` — `into_owned()` inside sort comparator | O(N log N) String allocs per folder load. |

---

## 4. Security & Reliability Issues

### Critical
- **S1. Symlink traversal in `fs::remove_dir_all`** (`fs_collection_repo.rs:250, 438`): malicious workspace with a symlinked "folder" → user clicks delete → arbitrary directory wiped.
- **S2. Missing name validation on read/delete/save paths** (`fs_collection_repo.rs:102-104, 197, 245, 269, 303, 408, 432, 442, 488, 504, 538`): `..`/absolute paths reach the filesystem unchecked. Combined with S1, enables arbitrary deletion.

### High
- **S3. Symlink-following on read/migrate** (`fs_collection_repo.rs:868`, `migration.rs:115, 142`) — exfiltration of files outside workspace.
- **S4. Non-atomic writes for important state**: `fs_workspace_config_repo.rs:61`, `migration.rs:94/134/169/197`, `fs_collection_repo.rs:354-358`. Crash leaves truncated `workspace.yml` or half-migrated tree.
- **S5. RMW races** (`fs_collection_repo.rs:538-615, 661-691, 741-762, 474-482`): concurrent `save_settings`/variable saves silently drop updates — `atomic_write` does not serialize read-modify-write.
- **S6. Silent data loss on corrupt YAML** (`fs_collection_repo.rs:677, 884`): `unwrap_or_default()` on parse + later overwrite = user's content quietly gone.
- **S7. Migration not transactional, no backup, no lockfile** (`migration.rs`): bug in conversion permanently destroys legacy `.json`; concurrent app instances race.

### Medium
- **S8. `atomic_write.rs`** lacks parent-dir fsync, uses `sync_data` not `sync_all`, and the 32-bit-nanos tmp suffix can collide.
- **S9. Panics on poisoned mutex**: `fs_audit_log_repo.rs:48`, `file_watcher.rs:61, 67` — DoS in the audit/watch path.
- **S10. Audit log fragility** (`fs_audit_log_repo.rs:39`): one corrupt JSONL line aborts all reads → full audit history becomes unreadable (compliance impact).

---

## 5. Code Quality Problems

- **God modules**: `oc_conversions.rs` (2516 LOC), `opencollection.rs` (1770 LOC), `fs_collection_repo.rs` (1546 LOC, ~675 non-test). `save_request` is 79 lines mixing five concerns.
- **Duplication** (eliminable in one helper module):
  - **D1**: "list YAML files in dir" — 4× across env/template/history/cookie repos.
  - **D2**: `serde_yaml::to_string(...).map_err(...)` — 12+ sites in `fs_collection_repo` alone.
  - **D3**: "delete file if exists or NotFound" — 3× repos.
  - **D4**: `dir.join(format!("{name}.yml"))` — 3× repos.
  - **D5**: `fs::create_dir_all(parent)` *before* `atomic_write` — already done inside `atomic_write`; redundant at `fs_collection_repo.rs:329, 347, 460`.
  - **D6**: `.map_err(|e| DomainError::Internal(format!("Failed to parse {kind}: {e}")))` — 15+ sites.
- **Free conversion functions that should be `From`/`Into`**: `oc_variable_to_collection_variable`, `collection_variable_to_oc_variable` (`fs_collection_repo.rs:527, 681, 752`).
- **Naming drift**: `Oc*` prefix is opaque; `dir`/`base_dir`/`root` used inconsistently; `file_path()` method appears in 4 repos with different concrete meanings.
- **`fs_template_repo.rs:55` uses plain `fs::write`** instead of `atomic_write` — outright correctness bug.
- **Test gap**: `fs_contract_repo.rs` (modified in current working tree per `git status`) has zero tests; `shared_path_collection_repo.rs` has zero tests.

---

## 6. Top 5 Immediate Fixes

1. **Lock down filesystem boundaries** (Critical security):
   - Add `validated_collection_path(name)` calling `Collection::validate_name` + `validate_path`; route every public method through it.
   - Use `symlink_metadata` to refuse symlinks in `delete`, `delete_folder`, `build_folder_tree`, `migrate_directory`.
2. **Switch all important writes to `atomic_write`**: `fs_workspace_config_repo.rs:61`, all four `migration.rs` sites, the inline `OpenOptions::create_new` block at `fs_collection_repo.rs:349-373`. Also fix `fs_template_repo.rs:55`.
3. **Eliminate `request.clone()` in `save_request`** by changing `request_to_oc_http_request` (and dependent `From<Header>` etc.) to take `&Request`. Single biggest perf win.
4. **Stop silent data loss**: replace `unwrap_or_default()` on `OcFolderInfo` parse (`:677`) with a propagated error; in `build_folder_tree:884`, log the parse failure and surface a corrupted-request placeholder instead of dropping it.
5. **Harden `atomic_write.rs`**: switch to `sync_all`, add best-effort parent-dir fsync after rename, use `pid + nanos + counter` for tmp suffix.

---

## 7. Step-by-Step Refactoring Plan

**Phase 1 — Safety (1-2 days, zero behavior change for happy path):**
1. Land fixes #1, #2, #4, #5 from §6.
2. Replace `expect("…lock poisoned")` in `fs_audit_log_repo.rs:48` and `file_watcher.rs:61, 67` with `.into_inner()` recovery.
3. Add a per-canonical-path `Mutex` map (or `fs2` file lock) around all RMW sites in `fs_collection_repo.rs` (settings, folder vars, request vars, folder rename fixup).
4. Add a `.migration_in_progress` sentinel + per-collection `.legacy_backup/` snapshot before `migration.rs` mutates anything.

**Phase 2 — Centralize I/O (1 day, mechanical):**
1. Create `crate::yaml_io` with `read<T>`, `write<T>`, `read_dir<T>`, `delete_if_exists`, and `parse_with_ctx<T>`.
2. Migrate the four list-repos (env/template/history/cookie) to use it. Removes ~250 LOC duplication.
3. Drop redundant `fs::create_dir_all(parent)` calls before `atomic_write`.

**Phase 3 — Pull domain logic out of infra (2-3 days):**
1. Move folder-chain merge (`get_folder_chain_variables` body) to `rocket_collection::variables::merge_folder_chain`. Repo only fetches raw per-folder lists.
2. Move UID minting into domain constructors; conversion returns `Err(OcConversionError::MissingUid)` instead of inventing UIDs.
3. Move filename-collision policy (`"{stem} {n}.yml"`) into `rocket_collection`; repo gets an oracle "is this name taken?".
4. Convert free conversion fns to `From`/`Into` impls.

**Phase 4 — Split god modules (1-2 days):**
- Split `fs_collection_repo.rs` into a `fs_collection/` module: `paths.rs`, `requests.rs`, `folders.rs`, `settings.rs`, `variables.rs`, `legacy_migration.rs`. Target ≤300 lines per file.
- Split `oc_conversions.rs` by domain: `auth.rs`, `body.rs`, `request.rs`, `folder.rs`, `variables.rs`, `examples.rs`.
- Split `opencollection.rs` by protocol (HTTP/GraphQL/gRPC/WS).
- Mark `pub mod opencollection`/`oc_conversions` as `pub(crate)`; expose only an `OcCodec` facade.

**Phase 5 — Performance (1-2 days, after correctness):**
1. Lazy folder tree: `Collection::get` returns request *summaries*; full `Request` loaded on demand in `get_request`.
2. mtime-keyed parsed-YAML cache invalidated by the existing file watcher.
3. Replace `Vec<serde_json::Value>` placeholders for variables/examples/client_certificates with concrete domain types.
4. `fs_history_repo::list` — sort by mtime, truncate to `limit`, then read.
5. Bulk-import path with deferred fsync (one parent fsync per directory).

**Phase 6 — Move misplaced code:**
- `tauri_tracing_layer.rs` → `src-tauri/`. Drop `tauri` from `rocket-infra/Cargo.toml`. Switch `rocket-audit` to `.workspace = true`.

---

## 8. Target Architecture

```
crates/rocket-infra/src/
  lib.rs                     // re-exports only Fs*Repo + ReqwestExecutor
  yaml_io.rs                 // read<T>/write<T>/read_dir<T>/delete_if_exists
  atomic_write.rs            // hardened: sync_all + parent-dir fsync
  paths.rs                   // OcLayout: reserved names, extensions, path builders

  fs_collection/
    mod.rs                   // FsCollectionRepo struct + trait impl (orchestration only)
    paths.rs
    requests.rs
    folders.rs
    settings.rs
    variables.rs             // raw I/O only — merge logic lives in rocket-collection
    legacy_migration.rs
  fs_environment_repo.rs
  fs_template_repo.rs
  fs_history_repo.rs
  fs_cookie_repo.rs
  fs_contract_repo.rs        // + tests
  fs_audit_log_repo.rs
  fs_workspace_repo.rs
  fs_workspace_config_repo.rs
  fs_compliance_profile_repo.rs

  shared_path_collection_repo.rs   // + integration test for path-switch

  oc/                                // pub(crate) only
    mod.rs                           // OcCodec facade — the only public surface
    schema/                          // serde structs
      mod.rs http.rs graphql.rs grpc.rs websocket.rs
    convert/                         // From/Into impls
      auth.rs body.rs request.rs folder.rs variables.rs examples.rs

  reqwest_executor/
    mod.rs
    auth/                            // bearer / basic / api_key / oauth2 / digest
  file_watcher.rs
  migration.rs                       // transactional with sentinel + .legacy_backup/
```

Domain rules (variable merge, UID invariants, filename policy) live in `rocket-collection`. `rocket-app` depends only on traits. `rocket-infra` is exclusively I/O + ACL.

---

## 9. Long-term Recommendations

- **Evaluate JSON-on-disk** for request files. The on-disk format must be machine-friendly above all; serde_json parses 3-8× faster than serde_yaml and the OpenCollection spec already permits it. Keep `.yml` for human-edited files (collection settings, folder.yml).
- **Process-level lockfile** at `~/.rocket-api/.lock` via `fs2` to coordinate two app instances and migration.
- **Versioning + crash sentinels** for any future schema change — make it a pattern, not migration-specific.
- **Property tests** for round-trip `Request` ↔ `OcHttpRequest` covering every auth scheme and body type — currently only example-based.
- **Centralize `tracing::warn!` on all "silently swallowed" branches** (corrupt YAML, parse-failed `_order.yml`, ignored migration `let _ = remove_file`); even keeping the recovery, an audit trail prevents the "missing collection" mystery class.
- **Consider a `FileStore` trait** for the future, only if/when you need in-memory testing of `rocket-app` against real repo impls. Don't introduce it preemptively.
