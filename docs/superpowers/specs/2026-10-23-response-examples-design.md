# Response examples: save a response as an example of an endpoint

Date: 2026-10-23. Status: draft for approval.

## Intent

A user sends a request and sees a response. They want to keep that response as a named example of the endpoint (a 200, a 401, an edge case), see it again later, and share it with the team through git. Bruno calls this "Response Examples". Rocket already stores examples in a request `.yml` (`examples:`) and masks their secrets at commit time. It has no UI to create, view or manage them. The user checked the response panel and found no Save button. This design adds the missing UI and the commands behind it.

## Goals

- A "Save as example" button in the response toolbar (next to the layout toggle and copy icon). It opens a small dialog asking for a name, then saves the example.
- An example stores the request as declared in the editor (method, URL, params, headers, body, with `{{vars}}` left as written) and the response (status, headers, body). Orchestrator's call, replacing an earlier "resolved values" idea: unresolved templates keep resolved secrets out of the file and let "Try" re-send under any environment. Auth injected by the executor and cookie-jar cookies are not part of the request snapshot. The response is stored as received.
- Examples show nested under their request in the sidebar. Clicking one opens a read-only tab: saved request on one side, saved response on the other.
- Rename and delete an example from its sidebar context menu.
- A "Try" button on the example tab opens an editable copy of the example's request in a temporary tab. The temporary tab is not written to disk. Sending it never changes the saved request or the example.
- Examples are stored in the existing `examples:` block of the request `.yml`, so they travel through git with the collection.

## Non-goals

- Editing an example's request or response in place. Examples are snapshots.
- Bruno's "Create Example without sending" (from the request context menu). Possible follow-up.
- Examples for non-HTTP kinds (GraphQL, gRPC, WebSocket). HTTP requests only, unless the builder finds GraphQL trivially free.
- Exporting examples. They stay out of the Bruno and Postman exports, as `.claude/collection-export.md` already documents.
- Changes to masking beyond the gaps listed under "Decisions made with the user".

## Anti-goals (a failure even if it technically works)

- A save that silently drops or rewrites other examples, the request's variables, or any other part of the request file.
- Real secrets leaking out in a commit or an export. The existing commit masking and the export exclusion must keep working with examples the new UI creates.
- Saving a request edit erasing examples.

## Decisions made with the user

- Placement of saved examples: nested under the request in the sidebar (Bruno style), not a dropdown in the response panel.
- Secrets: real values stay on disk. Masking happens only at commit (existing `mask_examples`) and examples stay out of exports as today. No mask at save time.
- Masking scope (user decision, replaces "do not change masking rules"): extend `mask_examples` in `crates/rocket-git/src/sanitize.rs`, with tests, to cover the gaps it has today: non-JSON bodies (form-urlencoded and text with secret-named keys or JWT-shaped values, XML secret-named elements), `Set-Cookie` and `Cookie` values whatever the cookie name, and secrets in JSON keys with harmless names where detectable by value shape (JWT-like, long opaque token-like strings next to a secret-named sibling is the builder's call). Update `.claude/example-masking.md` to match. Anything still uncovered stays listed there as "Not covered".

## Constraints and facts from the code

- Domain type: `HttpRequestExample { name, description?, request?, response? }` in `crates/rocket-shared/src/action.rs`. `request` and `response` are opaque `serde_yaml::Value` snapshots. This design needs a documented shape for those two snapshots (below), because commit masking reads them.
- The frontend `Request` interface in `src/lib/tauri-api.ts` does not carry `examples`. `save_request` in `crates/rocket-infra/src/fs_collection/requests.rs` keeps the existing examples whenever the incoming request has none. Two consequences:
  - Saving a request from the UI never touches examples. Keep that.
  - Deleting the last example cannot go through `save_request`, because an empty list means "keep what is on disk". Examples need their own commands.
- Add dedicated IPC commands for examples (list, add, rename, delete), kept thin, routed through a `rocket-app` service and a `rocket-infra` implementation. Each command reads the request file, changes only `examples`, and writes it back atomically. Builder decides exact command names and DTOs. camelCase only on IPC DTOs, never on persistence structs.
- Commit masking (`crates/rocket-git/src/sanitize.rs`, see `.claude/example-masking.md`) masks secret-named header and param values, secret query values in the example URL, secret JSON body keys, and JWT-shaped JSON strings. The snapshot shape written by the new save must be one that `mask_examples` already understands. Read `sanitize.rs` and its tests first and match their expectations. If the current shape cannot hold what the UI needs, extend masking in the same change, with tests. Do not guess.
- The user's example response in the report was a login reply with `token` and `refresh_token`. Use it as a manual test: after a commit, those values must show as `[masked]` in the commit and stay real on disk.
- The response toolbar lives in `src/components/response/ResponseBodyViewer.tsx` and its parent in `src/components/request/RequestPanel.tsx`.
- Rules that apply: shadcn/ui primitives only, lucide-react icons only, Monaco for any multi-line viewer, narrow Zustand selectors, no unwrap in production Rust, this touches collection and request files so read `docs/superpowers/specs/opencollection-spec-reference.md` first, and the new IPC commands need `.claude/tauri-commands.md`.

## Behaviour details

- The Save button is enabled only when a response exists for the open request. While no response, it is disabled or hidden (builder's call).
- The dialog defaults the name to something useful, for example `<status> <reason>` such as `200 OK`. If a name already exists on that request, the dialog blocks the duplicate or appends a number (builder's call, but never silently overwrite).
- Saving an example does not mark the request tab as having unsaved changes, because it does not change the request's own fields.
- Binary or very large bodies: builder decides a sensible cap or skip, and tells the user in the dialog. Do not write megabytes into a `.yml` without a limit.
- The example tab is read-only and uses the same body viewer as live responses where practical.
- Renaming or deleting a request must keep its examples with it (rename moves the file) or remove them (delete removes the file). Check that the sidebar handles an expanded request with children correctly.

## Calls made after the builder check (orchestrator's, marked for review)

- Snapshot source: the frontend builds the snapshot from the open request and the live response it already holds, and sends it to the add command. The backend validates the name and enforces the size cap. If the live response lacks something needed (raw bytes, content type), the builder adds it to the execute result.
- Snapshot shape: follow the OpenCollection `HttpRequestExample` shape in the spec reference. The viewer reads leniently, so examples written by Bruno import or by hand still display.
- Identity: request path plus example name. Names are unique per request. Rename while the tab is open updates or closes that tab. A `git pull` that changes the file refreshes the sidebar by the existing refresh path (builder's call on event vs refetch).
- Try tab: a temporary, non-persisted request tab that reuses the request editor and carries the parent request's collection and folder scope, so variables, inherited auth, scripts and the trust gate behave as for the parent. It is discarded on close and on app restart. Whether its sends appear in history is the builder's call; it must not be written to the collection.

## Left to the builder

- Command names, DTO shapes, file layout of the new service.
- Snapshot field names, as long as masking understands them.
- Sidebar visual details (icon, indent, status badge) within the existing sidebar style.
- Tab kind and store wiring for the read-only example tab and the temporary "Try" tab.

## Verification

- Rust: unit tests for add, rename, delete, duplicate names, and that other `.yml` fields (variables, auth, scripts) survive. A test that a commit masks an example made by the new save path.
- Frontend: tests for the button, the dialog and the sidebar nesting. `yarn tsc --noEmit`, `yarn check`.
- Manual in the real app: send a login request, save an example, see it nested in the sidebar, open it, press Try, send, commit and confirm masked tokens, export and confirm examples are left out.
