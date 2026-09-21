# Spec: Event Publishing for `CollectionService`

**Status:** Draft
**Severity:** Medium (reported bug: deleting a request via the collection tree's context menu does
not remove it from the visible tree)
**Related:** none — standalone fix, no dependency on other in-flight specs.

## 1. Problem

Right-click "Delete" on a request in the collection tree does not remove the request node from the
UI after the delete is confirmed, even though the backend deletion itself succeeds.

Root cause, confirmed by investigation (`crates/rocket-infra/src/fs_collection/requests.rs:118-127`,
`crates/rocket-app/src/collection_service.rs`, `crates/rocket-shared/src/events.rs`,
`src-tauri/src/tauri_event_bus.rs:26-32`, `src/components/collections/CollectionNode.tsx:159-188`):

- `CollectionService` (`crates/rocket-app/src/collection_service.rs`) is the **only** service in
  `rocket-app` that does not take a `Box<dyn EventPublisher>` — every sibling service
  (`EnvironmentService`, `CookieService`, `TemplateService`, `HistoryService`,
  `RequestExecutionService`, `CollectionRunnerService`, `GitAppService`, `WorkspaceService`) already
  follows the constructor-injection pattern documented in `crates/rocket-app/CLAUDE.md`: *"Every
  service takes `Box<dyn SomeRepository>` and `Box<dyn EventPublisher>` via its constructor."*
- `DomainEvent::RequestDeleted { collection, path }` (and five siblings: `CollectionCreated`,
  `CollectionDeleted`, `CollectionRenamed`, `RequestSaved`, `ItemMoved`) are already declared in
  `crates/rocket-shared/src/events.rs` and already routed to the Tauri `"collection-changed"`
  channel in `TauriEventBus::publish` (`src-tauri/src/tauri_event_bus.rs:26-32`) — but are never
  constructed anywhere, because `CollectionService` has no way to publish them.
- The frontend's listener (`CollectionNode.tsx:159-188`) is already correct: it matches generically
  on `event.collection ?? event.name` against the node's own collection name, and would call
  `refreshTree()` for any of these events if it ever received one. No frontend bug exists.
- The only thing that currently causes the tree to refresh after a mutation is
  `NotifyFileWatcher` (`crates/rocket-infra/src/file_watcher.rs`) picking up the OS-level
  filesystem notification and republishing it as `DomainEvent::FileChanged` — best-effort, async,
  and any watch error is silently dropped (`let Ok(event) = result else { return };`, line 29).
  This was a deliberate architecture choice (`src-tauri/src/lib.rs:195-196`: *"Application
  services — no event publishing. The file watcher is the single source of truth for sidebar
  updates."*), but it leaves every `CollectionService` mutation with no deterministic signal.

Confirmed non-causes: not specific to the right-click context menu (the "···" dropdown menu item
calls the identical `onDelete(...)` with identical arguments — `RequestNode.tsx:309` and `:379`);
not a recent regression (no relevant commit in the last 20 touching these files); not a
`FolderNode.tsx` stale-state issue (it is purely props-driven, no local caching).

## 2. Goal

Make every `CollectionService` mutation publish the `DomainEvent` that already describes what it
did, so the frontend tree refreshes deterministically on the operation's own success — not
dependent on the OS file watcher's best-effort delivery. The file watcher stays in place unchanged,
as a fallback for changes made outside the app (an external editor, a script writing files
directly).

## 3. Design

### 3.1 DDD placement

This follows the crate's existing pattern exactly — no new pattern is introduced:

- Event *types* live in `rocket-shared` (`crates/rocket-shared/src/events.rs`), the shared
  domain-events vocabulary already used by every other service.
- `CollectionService` (`rocket-app`, orchestration layer) depends only on the `EventPublisher`
  *trait*, injected via constructor (`Box<dyn EventPublisher>`) — no concrete `TauriEventBus` type
  ever appears in `rocket-app`, per `crates/rocket-app/CLAUDE.md`'s stated pattern.
- `rocket-infra`'s `FsCollectionRepo`/`SharedPathCollectionRepo` are untouched — they stay pure
  I/O. Publishing happens in `CollectionService`, after the repo call succeeds, matching
  `EnvironmentService::delete` (`environment_service.rs:58-62`) and every `GitAppService` method.
- The concrete `TauriEventBus` wiring stays in `src-tauri` (the composition root); `rocket-app`
  never touches Tauri.

### 3.2 New `DomainEvent` variants

Six of `CollectionService`'s fourteen mutating methods have no existing matching variant. Add these
to `crates/rocket-shared/src/events.rs`, following the existing `{ collection, path }`-shaped
convention used by `RequestSaved`/`RequestDeleted`/`ItemMoved`:

```rust
// Folder events
FolderCreated { collection: String, path: String },
FolderDeleted { collection: String, path: String },
ItemsReordered { collection: String, folder_path: String },

// Collection settings/variable events
CollectionSettingsSaved { collection: String },
FolderVariablesSaved { collection: String, folder_path: String },
RequestVariablesSaved { collection: String, request_path: String },
```

### 3.3 `CollectionService` → event mapping

All fourteen mutating methods in `crates/rocket-app/src/collection_service.rs` get a `.publish(...)`
call placed immediately after their `self.repo.*` call — since that call already uses `?` to
short-circuit on error, reaching the `.publish(...)` line means the mutation already succeeded.
This is the same fire-and-forget-after-success pattern every sibling service already uses (e.g.
`EnvironmentService::delete`, `environment_service.rs:58-62`).

| Method | Event published |
|---|---|
| `create` | `CollectionCreated { name }` *(existing variant, currently dead)* |
| `delete` | `CollectionDeleted { name }` *(existing, currently dead)* |
| `rename` | `CollectionRenamed { old_name, new_name }` *(existing, currently dead)* |
| `save_request` | `RequestSaved { collection, path }` *(existing, currently dead)* |
| `rename_request` | `RequestSaved { collection, path: old_path }` — it rewrites via the same `repo.save_request` call, so it's the same signal to the tree |
| `update_request_docs` | `RequestSaved { collection, path }` — same reasoning |
| `delete_request` | `RequestDeleted { collection, path }` *(existing, currently dead — the reported bug)* |
| `move_item` | `ItemMoved { src_collection, src_path, dst_collection, dst_path }` *(existing, currently dead)* |
| `create_folder` | `FolderCreated { collection, path }` *(new)* |
| `delete_folder` | `FolderDeleted { collection, path }` *(new)* |
| `reorder_items` | `ItemsReordered { collection, folder_path }` *(new)* |
| `save_settings` | `CollectionSettingsSaved { collection: name }` *(new)* |
| `save_folder_variables` | `FolderVariablesSaved { collection, folder_path }` *(new)* |
| `save_request_variables` | `RequestVariablesSaved { collection, request_path }` *(new)* |

(`rename_request`/`update_request_docs` are one row each in the method list above but appear
combined for brevity; both are still separate `CollectionService` methods, each gets its own
`.publish()` call.)

### 3.4 Event routing (`TauriEventBus`)

Add the six new variants to `src-tauri/src/tauri_event_bus.rs`'s match, routed to
`"collection-changed"` — the same channel every other collection/request/move event already uses:

```rust
DomainEvent::FolderCreated { .. }
| DomainEvent::FolderDeleted { .. }
| DomainEvent::ItemsReordered { .. }
| DomainEvent::CollectionSettingsSaved { .. }
| DomainEvent::FolderVariablesSaved { .. }
| DomainEvent::RequestVariablesSaved { .. } => "collection-changed",
```

### 3.5 Constructor wiring

`CollectionService::new`/`new_with_audit` gain an `events: Box<dyn EventPublisher>` parameter,
mirroring `EnvironmentService` exactly (`environment_service.rs:13-28`):

```rust
pub fn new(repo: Box<dyn CollectionRepository>, events: Box<dyn EventPublisher>) -> Self {
    Self { repo, events, audit: Arc::new(NullSecurityAuditPublisher) }
}

pub fn new_with_audit(
    repo: Box<dyn CollectionRepository>,
    events: Box<dyn EventPublisher>,
    audit: Arc<dyn SecurityAuditPublisher>,
) -> Self {
    Self { repo, events, audit }
}
```

`src-tauri/src/lib.rs:200-203` passes a real `TauriEventBus` (matching how `exec_svc`, `runner_svc`,
and `git_svc` are already wired at lines 228/253/258), and the now-inaccurate comment at lines
195-196 (*"Application services — no event publishing. The file watcher is the single source of
truth for sidebar updates."*) is removed/updated to reflect that the file watcher is now a
fallback, not the only source.

### 3.6 Frontend

No changes. `CollectionNode.tsx:159-188`'s matching logic is already generic and correct for all
six new event `type` strings — confirmed during investigation (none of them match the
`'collectionDeleted'`/`'collectionRenamed'`/`'branchSwitched'`/`'branchMerged'` skip checks, and
all carry a `collection` field that resolves to the short collection name).

## 4. Non-goals

- **`cookie_service.rs`/`template_service.rs`** also take an `EventPublisher` but never call
  `.publish()` — the same class of dead capability. Explicitly out of scope: nobody has reported
  cookies or templates going stale in the UI, and bundling an unrelated fix into this change
  balloons its blast radius for no reported benefit. Worth a follow-up item if it's ever reported.
- **`runner-*`/`history-changed`/`script-*` Tauri channels have zero frontend listeners** — the
  mirror-image gap (events published, nobody consuming). This is a frontend-feature gap, not a
  backend wiring fix, and is unrelated to the reported bug. Explicitly out of scope.
- Not changing `NotifyFileWatcher` or its restart-on-workspace-switch behavior at all — it remains
  the fallback for out-of-band filesystem changes.
- Not adding events for read-only `CollectionService` methods (`list`, `get`, `get_summaries`,
  `get_settings`, `get_folder_chain_variables`, `get_folder_variables`, `get_request_variables`) —
  nothing to signal, they don't mutate state.

## 5. Interfaces (for the implementation plan)

- `rocket_shared::events::DomainEvent` — six new variants (§3.2).
- `CollectionService::new(repo, events)` / `::new_with_audit(repo, events, audit)` — new
  `events: Box<dyn EventPublisher>` parameter on both.
- `TauriEventBus::publish` — six new match arms, all → `"collection-changed"`.
- `src-tauri/src/lib.rs` — `collection_svc` construction passes
  `Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone()))` instead of nothing.
- All fourteen `CollectionService` mutating methods gain a `.publish(...)` call (§3.3) — read-only
  methods are unchanged.

## 6. Testing

- `crates/rocket-shared/src/events.rs`: wire-shape serialization tests for the six new variants,
  matching the existing pattern (e.g. `runner_step_completed_wire_shape`) — assert the `type` tag
  serializes to the expected camelCase string and field names stay as declared (not camelCased,
  per the crate's existing `#[serde(rename_all = "camelCase")]` behavior on the enum tag only).
- `crates/rocket-app/src/collection_service.rs`: extend the existing test module with a capturing
  `EventPublisher` test double (mirroring the existing `CapturingPublisher` pattern already used
  for `SecurityAuditPublisher` in this same file) — one test per mutating method, asserting the
  right variant (and fields) is published after a successful call, and that a failed repo call
  (e.g. deleting a nonexistent request) publishes nothing.
- Update `make_service()` and the `delete_emits_security_audit_event` test's `new_with_audit` call
  to pass a `NullEventPublisher`/capturing publisher for the new constructor parameter.
- `src-tauri/src/lib.rs`: `cargo check` (or full `cargo build -p src-tauri`) to confirm the
  `collection_svc` construction site compiles with the new parameter.
- No frontend test changes required (§3.6) — existing `CollectionNode.test.tsx` coverage for the
  `collection-changed` listener already exercises the matching logic these new events flow through.

## 7. Acceptance criteria

1. Right-click "Delete" (and the "···" menu's "Delete") on a request removes it from the visible
   collection tree immediately after confirming, without needing a manual refresh or waiting on the
   file watcher.
2. The same holds for: creating a collection, deleting a collection, renaming a collection, saving
   a request (including via rename/docs-edit), moving an item, creating a folder, deleting a
   folder, reordering items, saving collection settings, saving folder variables, saving request
   variables — each is reflected in the tree/sidebar immediately after its own operation succeeds.
3. A failed mutation (e.g. deleting a request that doesn't exist) publishes no event.
4. `cargo test -p rocket-app` and `cargo test -p rocket-shared` pass.
5. `cargo check` (or equivalent) passes for `src-tauri` with the updated `CollectionService`
   construction.
6. `yarn vitest run src/components/collections/__tests__/CollectionNode.test.tsx` continues to
   pass unchanged.
