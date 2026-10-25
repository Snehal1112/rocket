# Flow request refs (F-01)

Flows follow saved requests when they move. Delete is reported, not rewritten.

## Rewritten

- A request or folder moves inside one collection (`move_item`, `move_item_no_replace`).
- Covers drag and drop, folder rename and assistant proposals.
- A folder move rewrites every Saved ref below it, by whole path segments
  (`a/b` does not match `a/bc`).

## Only reported

- A move to another collection: the flows that now dangle are named in the toast.
- A delete: the delete dialog lists the flows that use the item first.
  Delete stays enabled, also when the lookup fails.

## Pieces

- `rocket-flow` `request_refs.rs`: pure path rewrite and lookup.
- `FlowReferenceUpdater`: trait used by the collection service.
- `CollectionService` hook: calls the updater after a move.
- Event `flow-request-paths-rewritten`: carries the rewritten flows and any `failed` entries.
- `flows_using_request` (IPC): lookup for the dialog.
- `rewriteFlowRequestPaths`: patches an open tab's nodes, keeping unsaved edits.
- `useFlowRequestPathEvents` hook (`src/hooks`): applies the event to tabs and the
  flow query cache, and shows a toast `Flows not updated: A (message)`.
- `CollectionsSidebar` delete dialog: line `Used by 2 flows: A, B.`

## Failure rules

- One failed flow never fails the move or the other flows.
- A flow that cannot be parsed is left byte for byte as it was.
- `flows_using` rejects an empty, absolute or `..` path with `InvalidInput`.

## Limits

- Edits outside the app (a git pull) are not rewritten (F-135).
- Unparseable flows are skipped and named.
- The run cache is not rewritten.
- There is a short instant between the file rewrite and the tab patch.
- The dangling-path lint after a delete waits for F-21 (F-134).

## Tests

- Rust: `rocket-flow` `request_refs.rs`, `rocket-app` collection service, and
  `flow_request_ref_e2e` with the real file repos.
- Frontend: `src/hooks/__tests__/useFlowRequestPathEvents.test.tsx`,
  `src/components/layout/__tests__/CollectionsSidebar.test.tsx`.
