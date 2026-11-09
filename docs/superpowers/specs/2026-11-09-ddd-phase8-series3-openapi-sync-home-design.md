# Phase 8 series 3: a home for OpenAPI sync (D-81)

Status: approved 2026-11-09 (scope chosen by the user: new crate `rocket-openapi`). Roadmap: `docs/roadmaps/ddd-roadmap.md` Phase 8, D-81. Decisions: `docs/engineering/ddd-decisions.md` (D-02 split by context). Follows series 2 (`2026-11-09-ddd-phase8-series2-domain-rules-design.md`).

## Goal

The OpenAPI sync context (about 20 `openapi_*` files and their tests in `rocket-app`) becomes the crate `rocket-openapi`. `rocket-app` keeps the background scheduler and the wiring. No behavior change, no change to any file format, IPC command or event wire shape.

## Non-goals

- Any behavior, JSON, file-format or IPC change.
- Moving `OpenApiSyncScheduler` (see the rulings) or the importer in `rocket-import`.
- Series 4 (flow runtime) and the D-85 note.

## Current state (checked 2026-11-09 on this branch)

The sync code in `crates/rocket-app/src` is about 7,000 non-test lines and 7,000 test lines:
- `openapi_sync_service.rs` (2,617 lines with tests): `OpenApiSyncService` and its builder (`with_apply`, `with_lock_keys`, `with_local`, `with_consent`, `with_backups`), `CheckOutcome`, `SyncStatusView`, `SyncTarget`, `sha256_hex`, `safe_message`.
- Seven more files add `impl OpenApiSyncService` blocks: `openapi_sync_apply.rs` (also `ApplyDeps`, `RendererFactory`, `flatten_requests`), `_changes`, `_delete`, `_reset`, `_skips`, `_spec_view`, `_spec_source`, plus `openapi_connect.rs` and `openapi_change_link.rs`. Rust allows an `impl` only in the crate that owns the type, so all of these must move in one step.
- Leaves: `openapi_sync_view.rs` (view DTOs, imports only `rocket_import::SyncPlan`) and `openapi_sync_events.rs` (`OpenApiSyncEvent`, imports only `rocket-shared`).
- `openapi_sync_scheduler.rs` (212 lines): a tokio supervisor loop. It uses `crate::stop_signal::sleep_or_stop`, which `network/` also uses.
- Domain parts are already in `rocket-collection` (`openapi_source`, `openapi_baseline`, `openapi_local`, `openapi_consent`, `openapi_backup`, `openapi_schedule`) and in `rocket-import`; the ports are `SpecFetcher` and `LocalSpecReader` (`rocket-http`) and `SecretStore` (`rocket-environment`).
- The only `rocket-app` helpers the sync code needs: `collection_lock::collection_lock` (10 uses in sync files; also used by `CollectionService`, `execution_service`, `collection_trust`, `network/sync_collections`), and `stop_signal` (scheduler only). Test code also touches `test_doubles` (`NullCollectionRepo` at one site in `openapi_sync_service.rs`, `RecordingPublisher`, `SharedPublisher`, `InMemoryTrustStore` in `openapi_sync_lock_tests.rs`), `CollectionService` and `CollectionTrustService` (`openapi_sync_lock_tests.rs`).
- Consumers: `src-tauri` (`commands/import.rs`, `commands/openapi_sync.rs`, `tauri_event_bus.rs`, `lib.rs`, `commands/workspaces.rs`) through `rocket_app::...`; `rocket-app/src/lib.rs` re-exports the sync names. `rocket-infra` does not depend on `rocket-app`.

## Design

Two plans, at most three tasks each. Every move is a `git mv` (or cut and paste where the source stays), tests move with the code, no re-export or shim at the old location. Search every target with `rg` over whole directories (`crates`, `src-tauri`, including `tests` and benches).

### Plan 1: the foundations

1. **Lock to `rocket-collection`.** `crates/rocket-app/src/collection_lock.rs` moves to `crates/rocket-collection/src/collection_lock.rs` (std only, no new dependency), made `pub` where it was `pub(crate)`. Every user imports it from `rocket_collection::collection_lock`. Its module doc and tests move with it. The lock key comes from `CollectionRepository`, which already lives there.
2. **Crate and leaves.** Create `crates/rocket-openapi` (workspace member and `[workspace.dependencies]` entry like the other crates). Move `openapi_sync_events.rs` and `openapi_sync_view.rs` into it as `events.rs` and `view.rs`, modules `rocket_openapi::events` and `rocket_openapi::view`. Dependencies at this point: `rocket-shared`, `rocket-import`, `serde`. `rocket-app` and `src-tauri` gain a dependency on it and switch their imports. Create `crates/rocket-openapi/CLAUDE.md` (role, dependency direction, module map). The root `CLAUDE.md` crate table gets a row.

### Plan 2: the service move

1. **Atomic move.** `git mv` the service, the `impl` files and their tests into `crates/rocket-openapi/src/` as: `service.rs`, `apply.rs`, `changes.rs`, `delete.rs`, `reset.rs`, `skips.rs`, `spec_view.rs`, `spec_source.rs`, `connect.rs`, `change_link.rs` (tests as `*_tests.rs` beside them, keeping the existing `#[path]` chain). `OpenApiSyncService`, `CheckOutcome`, `SyncStatusView`, `SyncTarget`, `safe_message`, `ApplyDeps`, `RendererFactory`, `DeleteTarget`, `ResetReport`, `SkippedRow`, the connect and change-link types and `request_digest` are exported from the crate root. `pub(crate)` items that `rocket-app` (scheduler, lock tests) or `src-tauri` need become `pub`; nothing else changes visibility. `rocket-app/src/lib.rs` drops the sync modules and re-exports; `src-tauri` imports from `rocket_openapi`. `OpenApiSyncScheduler` stays in `rocket-app` and imports the service, `CheckOutcome` and `OpenApiSyncEvent` from `rocket_openapi`.
2. **Test placement.** `openapi_sync_lock_tests.rs` stays in `rocket-app` (it tests the sync service against `CollectionService` and `CollectionTrustService` together). The test in `openapi_sync_service.rs` that uses `NullCollectionRepo::on_lock_key` moves to `openapi_sync_lock_tests.rs`. The tests that use `rocket_infra::FsOpenApi*` need `rocket-infra` as a dev-dependency of the new crate (no cycle: `rocket-infra` does not depend on `rocket-app` or `rocket-openapi`). Scheduler tests stay in `rocket-app`.
3. **Docs and closing.** `rocket-app/CLAUDE.md` (the sync-scheduler section and the module list keep only what stays; the rest moves into `crates/rocket-openapi/CLAUDE.md`), root `CLAUDE.md` crate table, `.claude/docs-index.md` if it names moved files, `docs/features/openapi-sync*.md` file references, glossary if it names the module, roadmap D-81 `done`, a decisions-log line, the index row for series 3, and this spec's status.

Expected dependencies of `rocket-openapi` (report each one the compiler asks for beyond these): `rocket-shared`, `rocket-collection`, `rocket-import`, `rocket-http`, `rocket-environment`, `serde`, `serde_json`, `serde_yaml`, `chrono`, `sha2`, `tracing`, `async-trait`; dev: `rocket-infra`, `tempfile`, `tokio` if a test needs it. It must not depend on `rocket-app`, `rocket-git`, `rocket-workspace` (unless `rocket-import` forces it transitively), `rocket-acp` or `rocket-network`.

## Rulings

- Ruling (2026-11-09): new crate `rocket-openapi`, not `rocket-collection`. Why: the user chose it; the service orchestrates ports, `rocket-import` and file transactions, which `rocket-collection` (no I/O, no importer) must not depend on. Cost if wrong: merge the crate into another one; the move is mechanical.
- Ruling (2026-11-09): `collection_lock` moves to `rocket-collection`, not into the new crate. Why: `CollectionService`, `execution_service`, `collection_trust` and network sync share the same locks with the sync apply (lock order is documented in the module), so the lock must sit below all of them. Cost if wrong: move it again; it is 209 lines of std code.
- Ruling (2026-11-09): `OpenApiSyncScheduler` and `stop_signal` stay in `rocket-app`. Why: the scheduler is a tokio supervisor loop, a runtime concern like `network/`; `stop_signal` is shared with `network/` and would force tokio into `rocket-shared` or `rocket-collection`. The roadmap's "scheduler" means the rules, which already live in `rocket-collection::openapi_schedule`. Cost if wrong: move the scheduler later with a tokio-based shared home.
- Ruling (2026-11-09): the service files cannot move in pieces. Why: Rust `impl` blocks must sit in the crate that defines `OpenApiSyncService`. Plan 2 task 1 is therefore one large mechanical commit. Cost if wrong: none; a smaller step is not possible.
- Ruling (2026-11-09): `openapi_sync_lock_tests.rs` stays in `rocket-app`. Why: it exercises the sync service together with `CollectionService` and `CollectionTrustService`, which stay there. Cost if wrong: one test file in the wrong crate.
