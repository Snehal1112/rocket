# Flow request references survive moves and deletes (F-01)

## Goal

When a saved request or folder moves inside a collection, every flow in that collection that references it is rewritten to the new path. When one is deleted, the delete dialog first names the flows that use it.

## Overview

A `Request` node in a flow can point at a saved request with `RequestSource::Saved { request_path }`. The path is relative to the collection root, for example `auth/login.yml` (`crates/rocket-flow/src/node.rs:152-155`). Nothing keeps that path in step with the collection tree. After a move or a delete the flow still holds the old path. The flow saves fine, and fails only when it runs. Flow roadmap item F-01 records this as a Tier 1 bug.

Evidence: `CollectionService::move_item`, `move_item_no_replace`, `delete_request` and `delete_folder` never touch flows (`crates/rocket-app/src/collection_service.rs:326-333`, `373-390`, `391-400`, `436-453`). A folder rename in the sidebar is a `move_item` call (`src/components/collections/FolderNode.tsx:122`), so it breaks flows too.

## Current state

| Operation | Where | Effect on flows today |
|---|---|---|
| Request rename | `collection_service.rs:224-275` | Changes only the `name` field. The file name stays, so flows keep working. |
| Folder rename | `FolderNode.tsx:122` calls `moveItem` | Breaks every Saved reference below the folder. |
| Drag and drop move | `CollectionsSidebar.tsx:203` | Breaks the reference. A move to another collection breaks it for good. |
| Assistant proposals | `proposal_service.rs:723,739` call `move_item_no_replace` | Same break. |
| Delete request or folder | `CollectionsSidebar.tsx:129-135` | Breaks the reference, no warning. |
| Lint | `lint.rs:52` `saved_request_exists` returns `None`; `flow_service.rs:60` passes `NoLintContext` | No signal. The real dangling-path lint (F-21) is only on the unmerged branch `worktree-flow-tier2`. |

Other facts:
- `CollectionService` is built twice in `src-tauri/src/lib.rs` (`:385` and `:399`, the second inside `ProposalService`). `FlowService` is built later (`:715`) over `SharedPathFlowRepo`.
- `CollectionService` takes optional parts through builder methods, such as `with_trust_store` (`collection_service.rs:34`).
- `FlowRepository` offers `list`, `get`, `save`, `delete`, `rename` (`crates/rocket-flow/src/flow.rs:59-80`). `list` skips an unparseable flow file (`fs_flow_repo.rs` test `list_skips_malformed_entry_and_continues`).
- Open flow tabs keep their own copy of the nodes plus an undo history with a `saved` snapshot (`pane-store.ts:644-655`). `renameFolderTabs` (`pane-store.ts:1575`) is the precedent for retargeting tabs after a move.

Gaps: no rewrite on move, no lookup before delete, no event for open tabs.

## Non-goals

- Edits made outside the app (git pull, file manager). The app cannot see them.
- The dangling-path lint (F-21) and the rest of F-20 to F-30. They stay on `worktree-flow-tier2`.
- Rewriting or removing nodes on delete. A deleted request leaves the flow as it is.
- Unparseable flow files. `list` hides them today (F-29 is separate).
- Inline requests. They hold no path.

## Design

Flow:

```
move_item(src, dst)  -> repo move ok -> same collection? -> FlowReferenceUpdater.rewrite -> event "flow-request-paths-rewritten"
                                     -> other collection -> no rewrite (treated like a delete)
delete dialog open   -> flows_using_request(collection, path) -> names listed in the dialog
```

### 1. Pure path logic in `rocket-flow`

```rust
/// True when `request_path` is `target` or lies below it as a folder.
pub fn path_is_within(request_path: &str, target: &str) -> bool;

/// Rewrites every Saved reference at or below `from` to `to`. Returns the count.
pub fn rewrite_saved_paths(flow: &mut Flow, from: &str, to: &str) -> usize;

/// True when any Saved reference is at or below `target`.
pub fn uses_saved_path(flow: &Flow, target: &str) -> bool;
```

Matching is exact, or `target` followed by `/`, compared on whole segments and case sensitive. `auth` matches `auth/login.yml` but not `author/x.yml`. A rewrite replaces the matching prefix only: moving `auth` to `identity` turns `auth/login.yml` into `identity/login.yml`. A moved request (`a.yml` to `b/a.yml`) matches by equality. Only `RequestSource::Saved` nodes are touched.

### 2. `FlowReferenceUpdater` trait in `rocket-app`

```rust
pub trait FlowReferenceUpdater: Send + Sync {
    fn rewrite_request_paths(&self, collection: &str, from: &str, to: &str) -> FlowRewriteReport;
    fn flows_using(&self, collection: &str, path: &str) -> DomainResult<Vec<String>>;
}

pub struct FlowRewriteReport { pub rewritten: Vec<String>, pub failed: Vec<FlowRewriteFailure> }
pub struct FlowRewriteFailure { pub flow: String, pub message: String }
```

`FlowService` implements it. `rewrite_request_paths` walks `repo.list`, reads each flow with `repo.get`, rewrites it, and saves it through `repo.save` only when the count is above zero. It does not call `validate`, because a path change cannot change the graph. A `get` or `save` error goes into `failed` with the error text, and the walk continues. A flow that fails to read is never written. `flows_using` returns the flow names that use the path, sorted, without duplicates. A list error is returned as `Err`. `rewrite_request_paths` reports a list error as one `failed` entry named `(all flows)`, so the move still succeeds. `flows_using` rejects bad arguments with `DomainError::InvalidInput`: an invalid collection name (`Collection::validate_name`), an empty path, an absolute path, or a path with a `..` component.

### 3. Hook in `CollectionService`

Add `with_flow_references(Arc<dyn FlowReferenceUpdater>)` next to `with_trust_store`. Without it nothing changes, so tests and other callers are unaffected. `move_item` and `move_item_no_replace`:

1. Run the repo move. On error, return it and do nothing else.
2. When `src_collection == dst_collection`, `dst_path` is not empty and `src_path != dst_path` (both with a trailing `/` stripped), call `rewrite_request_paths(collection, src_path, dst_path)`.
3. When the report has any rewritten or failed entry, publish `DomainEvent::FlowRequestPathsRewritten { collection, from, to, flows, failed }` (fire and forget).
4. Return `Ok(())`. A failed rewrite never fails the move.

A move to another collection runs no rewrite. The service calls `flows_using(src_collection, src_path)` instead, and the event carries those flows in `failed` with the message `moved out of the collection`. `delete_request` and `delete_folder` are unchanged.

### 4. Lookup command

`flows_using_request(collection: String, path: String) -> Vec<String>` in `src-tauri/src/commands/flow.rs`. It calls `FlowReferenceUpdater::flows_using` on the managed `FlowService` and maps nothing else, so the validation above lives in the domain and the command stays thin.

### 5. Wiring in `lib.rs`

Build one `Arc<FlowService>` early, over its own `SharedPathFlowRepo` on the same `active_workspace_path`, as the second `CollectionService` already does (`lib.rs:399`). Pass it to both `CollectionService` builders. The managed `FlowService` stays as it is.

### 6. Event and frontend

`dst_path` is the full new path of the item, never only the target folder: the repo renames `src` onto `dst` (`crates/rocket-infra/src/fs_collection/folders.rs:314-370`). The event payload is the `DomainEvent` itself, which `TauriEventBus::publish` emits as is (`tauri_event_bus.rs:15-17`). Its tag is `type`, and the fields are single words (`collection`, `from`, `to`, `flows`, `failed`), so they read the same in snake and camel case. `TauriEventBus` maps the variant to `flow-request-paths-rewritten`. A listener hook in the app shell does three things:
- calls `usePaneStore.rewriteFlowRequestPaths(collection, from, to)`;
- invalidates `flowKeys.collection(collection)`;
- shows a warning toast naming each `failed` flow and its message.

`rewriteFlowRequestPaths` uses `updateTabEverywhere`. For every flow tab of that collection it rewrites the Saved paths in the nodes and in every snapshot of the tab's history (`saved`, past and future). The dirty flag is not touched, so a clean tab stays clean and a dirty tab stays dirty. Parked tabs in `collectionTabState` are patched too.

### 7. Delete dialog

When the delete target is a request or a folder, `CollectionsSidebar` calls `flowsUsingRequest(collection, path)` as the dialog opens. A non-empty result adds the line `Used by N flows: A, B.` The Delete button stays enabled. A lookup error shows nothing and logs to the console. The line reads `Used by 1 flow: A.` for one flow and `Used by 2 flows: A, B.` for more. The toast reads `Flows not updated: A (message); B (message)`.

### 8. Docs and roadmap

The roadmap says to update it when work ships. This work:
- sets F-01 to `done` in `docs/roadmaps/flow-roadmap.md` and adds follow-up rows at the end of the file (real-app check, the lint after delete, external edits), using the next free IDs;
- adds `request_refs.rs` to the module map of `crates/rocket-flow/CLAUDE.md`;
- adds `docs/features/flow-request-refs.md` (how the rewrite works, the event, the limits) and one reference line in the project `CLAUDE.md`.

## Error handling and edge cases

| Case | Behaviour |
|---|---|
| Repo move fails | Error returned, no rewrite, no event. |
| `src_path == dst_path` | No rewrite. |
| Flow file unparseable | Not listed by the repo today, so not touched (F-29 follow-up). |
| `get` or `save` of one flow fails | Listed in `failed`, other flows still rewritten, toast names it. |
| Move to another collection | No rewrite. Flows that used the item are in `failed` with `moved out of the collection`. |
| Folder prefix like `auth` vs `author` | `author/x.yml` is untouched. |
| Flow with several Saved nodes on the item | All are rewritten, the flow is listed once. |
| Same flow open in a tab with unsaved edits | Tab patched in place, edits kept, still dirty. |
| User saves the stale tab in the instant before the event | The old path is written back once. Accepted, the tab is patched right after. |
| Lookup fails in the delete dialog | No list shown, delete still possible. |
| Lookup called with an empty path, an absolute path or `..` | `InvalidInput`, no flow is read. |
| The flow list itself fails during a rewrite | One `failed` entry named `(all flows)`, the move still succeeds. |
| A flow of another collection uses the same path | Untouched. Only the moved item's collection is walked. |
| Delete | Flows are left as they are. |
| `move_item` replaces an existing destination | Flows that pointed at the replaced request now point at the moved one. Accepted, it is how the move already behaves. |
| `dst_path` empty (the "move to collection" menu passes `''` with another collection, `RequestNode.tsx:377`) | No rewrite. It is a cross-collection move, handled as above. |
| Run cache and run history | Not rewritten. The cache fingerprint covers the node kind, so a partial run after a rewrite asks for a fresh full run (F-105). Accepted. |
| Flow saved through the second `FlowService` instance | That instance holds only the repository, no events or audit, so nothing is bypassed. |

## Testing

- `rocket-flow` (`cargo test -j4 -p rocket-flow request_refs`): `path_is_within_matches_exact_and_folder_prefix`, `path_is_within_does_not_match_a_longer_sibling_name`, `path_is_within_never_matches_an_empty_target`, `rewrite_saved_paths_replaces_a_folder_prefix`, `rewrite_saved_paths_replaces_an_exact_request_path`, `rewrite_saved_paths_ignores_inline_requests`, `uses_saved_path_is_true_for_a_nested_request`.
- `rocket-app`, updater (`cargo test -j4 -p rocket-app flow_references`): `rewrite_request_paths_rewrites_and_saves_matching_flows`, `rewrite_request_paths_skips_a_flow_that_does_not_match`, `rewrite_request_paths_reports_a_failed_save_and_continues`, `rewrite_request_paths_does_not_write_a_flow_it_could_not_read`, `rewrite_request_paths_reports_a_list_error`, `rewrite_request_paths_saves_a_flow_that_would_fail_validation`, `flows_using_lists_each_flow_once_sorted`, `flows_using_rejects_bad_arguments`.
- `rocket-app`, service (`cargo test -j4 -p rocket-app move_item`): `move_item_rewrites_flows_in_the_same_collection`, `move_item_does_not_rewrite_when_the_move_fails`, `move_item_to_another_collection_reports_dangling_flows`, `move_item_to_another_collection_ignores_a_lookup_error`, `move_item_no_replace_rewrites_flows`, `move_item_succeeds_when_a_flow_rewrite_fails`, `move_item_without_an_updater_behaves_as_before`, `move_item_skips_the_rewrite_for_an_empty_destination_or_same_path`, `move_item_strips_trailing_slashes_before_rewriting`.
- `rocket-app`, real filesystem (`cargo test -j4 -p rocket-app flow_request_ref_e2e`): `moving_a_request_rewrites_the_saved_flow_on_disk`, `renaming_a_folder_rewrites_every_flow_below_it`, `a_corrupt_flow_file_is_left_untouched_and_the_move_succeeds`, `a_flow_of_another_collection_is_untouched`, `a_move_between_collections_reports_the_dangling_flow`.
- `src-tauri` (`cargo test -j4 -p rocket flow_request_paths_rewritten_has_its_own_channel`).
- Frontend: `yarn test pane-store-flow-request-paths --run` (`rewriteFlowRequestPaths patches nodes and history snapshots`, `... keeps a clean tab clean and a dirty tab dirty`, `... patches parked collection tabs`, `... leaves other collections and unrelated paths alone`, `... ignores tabs without history or collection`), `yarn test useFlowRequestPathEvents --run` (`an event with flows patches open tabs and refreshes the flow list`, `an event with only failures shows the toast and patches nothing`, `the hook unsubscribes on unmount`), `yarn test CollectionsSidebar --run` (`delete dialog lists the flows that use the request`, `delete dialog for a folder lists the flows that use it`, `delete dialog stays usable when the lookup fails`, `delete dialog shows nothing when no flow uses the request`).
- Checks: `cargo check -j4 -p <crate>` for each crate touched (`rocket-flow`, `rocket-shared`, `rocket-app`, `rocket`), `yarn tsc --noEmit`, `yarn check`.

## Files changed

- `crates/rocket-flow/src/` (new path helpers, exported from `lib.rs`)
- `crates/rocket-app/src/flow_service.rs`, `collection_service.rs`, `lib.rs`
- `crates/rocket-shared/src/events.rs`
- `src-tauri/src/tauri_event_bus.rs`, `lib.rs`, `commands/flow.rs`
- `crates/rocket-app/src/flow_references.rs` (new), `crates/rocket-app/src/flow_request_ref_e2e_tests.rs` (new), `crates/rocket-app/src/test_doubles.rs`
- `src/lib/tauri-api.ts`, `src/lib/flow-request-paths.ts` (new), `src/stores/pane-store.ts`, `src/components/layout/CollectionsSidebar.tsx`, `src/hooks/useFlowRequestPathEvents.ts` (new), `src/App.tsx`
- Docs: `docs/roadmaps/flow-roadmap.md`, `docs/features/flow-request-refs.md` (new), `crates/rocket-flow/CLAUDE.md`, `CLAUDE.md`

## Constraints

- Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.
- `rocket-flow` stays free of I/O. `rocket-app` holds trait objects only. Commands stay thin.
- No `unwrap` calls in production paths. Tests use `.expect("...")`.
- The event is a `DomainEvent` variant, so it adds no rename attribute: its fields are single words and read the same in snake and camel case. Flow files keep their current field names.
- Cargo commands are per crate, `cargo check -j4 -p <crate>` and `cargo test -j4 -p <crate> <filter>`, never a workspace command.
- shadcn primitives and lucide icons only. No full destructure of Zustand state.
- Existing `CollectionService` tests and callers keep passing without an updater.

## Acceptance criteria

- Moving a request inside a collection rewrites every Saved reference to it in that collection's flows, and the flow runs.
- Moving or renaming a folder rewrites every Saved reference below it, by whole path segments.
- `move_item` and `move_item_no_replace` both do this, so drag and drop, folder rename and assistant proposals are covered.
- A failed rewrite of one flow never fails the move and never stops the other flows. It is named in a warning toast.
- A flow that cannot be read is never overwritten.
- An open flow tab, clean or dirty, shows the new path with its unsaved edits intact, and Save does not write the old path back.
- A move to another collection reports the flows that now dangle.
- The delete dialog for a request or folder lists the flows that use it, and Delete still works, including when the lookup fails.
- A collection with no flows behaves exactly as before.
- With the real file repositories, moving a request or renaming a folder leaves the flow file on disk with the new path, and that path resolves to a saved request (test `flow_request_ref_e2e`).
- A flow file that cannot be parsed is left byte for byte as it was.
- `flows_using` rejects an empty path, an absolute path and a `..` path with `InvalidInput`.
- F-01 is `done` in `docs/roadmaps/flow-roadmap.md`, with follow-up rows added, and the docs in section 8 exist.

## Out of scope

- The dangling-path lint and other F-20 to F-30 items.
- Handling of changes made outside the app.
- Removing or marking nodes after a delete.

## Follow-ups

- Show the dangling-path lint after a delete once `worktree-flow-tier2` merges (F-21).
- Surface unparseable flow files so a rewrite can report them (F-29).
- Rewrite or warn on changes made outside the app, for example on a git pull.

## Decisions

- When a request or folder is moved, what happens to flows that reference it? Options: rewrite the flows automatically, block the move, warn only. Chosen: rewrite the flows automatically. Decided by: user
- When a request or folder is deleted, what happens? Options: warn before deleting, block the delete, rewrite or remove nodes. Chosen: warn before deleting. Decided by: call
- If a flow cannot be rewritten, what happens? Options: move first then rewrite and report failures, rewrite first all or nothing, move then undo on failure. Chosen: move first, rewrite each flow, skip and name failures in a warning, never overwrite a corrupt file. Decided by: user
- A request moved out of its collection is treated like a delete, since flows hold paths inside their own collection. Decided by: call
- Open flow tabs when a rewrite happens? Options: patch open tabs in place via an event, reload clean tabs only, close or lock affected tabs. Chosen: patch open tabs in place, unsaved edits survive, clean stays clean, dirty stays dirty. Decided by: user
- How does the delete warning work? Options: ask the backend first and list flows in the confirm dialog, warn after the delete only, no pre-delete warning. Chosen: new read-only lookup command, the confirm dialog lists the flows, delete stays allowed. Decided by: user
- Where does the logic live? Options: CollectionService through a FlowReferenceUpdater trait implemented by FlowService, Tauri command layer, subscriber on ItemMoved. Chosen: CollectionService through the trait, wired in lib.rs. Decided by: user
- Edits made outside the app (git pull, file manager) are out of scope. Decided by: call
- The dangling-path lint (F-21) is only on the unmerged branch worktree-flow-tier2. Options: do not depend on it, merge the branch first, build a minimal check here. Chosen: do not depend on it, this spec gives its own signals. Decided by: user
- The rewrite result reaches the frontend through one new event, not through the move_item return value, so assistant-applied moves are covered too. Decided by: call
- Lookup matches a path exactly or by folder prefix with one rule, so the command needs no folder flag. Decided by: call
- Open tab patching also rewrites the undo, redo and saved snapshots, so undo does not bring the old path back. Decided by: call
- Paths are compared with a trailing `/` stripped, and a same-collection rewrite needs a non-empty `dst_path`. Decided by: call
- Stale run cache entries and run history are left alone, since the cache fingerprint already forces a fresh run. Decided by: call
- A request moved out of its collection uses the same warning toast as a failed rewrite, with the message `moved out of the collection`. Decided by: call
- Argument validation for the lookup lives in `FlowService::flows_using`, not in the command, so it is tested without Tauri. Decided by: call
- A list error during a rewrite becomes one `failed` entry named `(all flows)`. Decided by: call
- Docs and roadmap updates are part of this work, as the project rules ask. Decided by: call
- A single shared `Arc<FlowService>` is built early in lib.rs for the two CollectionService instances, following the proposal service precedent. Decided by: call
