# Response examples

A request can hold named examples of a response (`examples:` in its `.yml`). They are shown nested under the request in the sidebar.

## Save and view

- Send an HTTP request, then press Save as example in the response toolbar. The button sits in the response toolbar, so it shows only once a response exists. It is disabled with one reason per cause (`saveExampleDisabledReason`). The first match wins, in this order:
  1. Try tab: "Temporary tab: not saved".
  2. Not an HTTP request: "Examples are only for HTTP requests".
  3. Request not saved: "Save the request first".
  4. No response yet: "Send the request first".
- Saving an example does not mark the request dirty and does not call `save_request`. It calls `add_request_example`.
- Open an example from the sidebar for a read-only tab (`ExampleTab`, `ExampleTabView`). Rename and delete it from the sidebar. An open tab is retitled or closed with it.

## Warnings in the dialog

The save dialog lists notes in this order. Each shows only when it applies.

1. "The request has unsaved changes. The example uses them, and the request file is not saved." Shows when the tab is dirty.
2. "Disabled params and headers are not saved." Shows when any query param, path param or request header is disabled.
3. "Secrets in headers and cookies are saved as written. Git commits mask them." Shows when a saved header name looks secret (authorization, cookie, token, secret, password, api key). Only enabled request headers and response headers count.

Two more notes come from the snapshot: the form-data or binary request body that is not saved, and the response body that is dropped.

## Snapshot shape

Built by `buildExampleSnapshot` (`src/lib/example-snapshot.ts`) from the request as it is in the editor, unsaved edits included.

- Request: method, URL, enabled params and headers, `{{vars}}` kept as written. Body: `json`, `text`, `xml`, `sparql` as `{type, data}`, `formurlencoded` as `form-urlencoded` rows. Form-data, binary and no body save no body.
- Response: status, status text, headers, body. The body type comes from `content-type` (`json`, `xml`, `html`, else `text`). A binary body, or one over the limit, saves as `{type: 'binary', data: ''}`. The dialog says so.
- Disabled params and headers are not saved. They were not sent, and an example describes what was sent. The dialog says so.
- A large or binary response body is dropped on purpose. Cutting it would give invalid JSON, and a higher cap would fight the 1 MiB YAML limit.

## Limits

- Response body: 512 KiB, measured in UTF-8 bytes.
- One example: 1 MiB of YAML (`MAX_EXAMPLE_BYTES` in `crates/rocket-infra/src/fs_collection/examples.rs`).
- Names: at most 120 characters, unique per request, compared case-insensitively.

## Secrets and export

- The file on disk keeps real values. They are masked only in the git commit. See `.claude/example-masking.md`.
- Exports to Bruno and Postman leave examples out. See `.claude/collection-export.md`.

## Try

Try on an example tab opens `openTryTab`: a normal request tab with `transient: true`, the parent's `source`, a title `Try: <name>` and a request from `exampleToRequestState`. That request takes method, URL, params, headers and body from the example (`{{vars}}` kept) and keeps the parent's auth, scripts, settings, assertions and actions. Every press opens a new tab. Variables, inherited auth, scripts and the trust gate resolve as for the parent because the source is the parent's.

A Try tab must never write to the parent. Guards:

- The store keeps `isDirty` false for it (`updateRequest`, `markDirty`). `savesToFile` in `pane-store.ts` gates close, tab switch, move, reset and close-all autosaves, and `dirtyRequestTabs`.
- `scheduleAutoSave`, `flushAutoSaves` and `saveTabRequest` refuse it.
- The Save button is disabled (`Temporary tab: not saved`), Ctrl+S and the tab menu Save do nothing, closing never prompts.
- Renaming the tab only retitles it. The tab menu Rename is disabled.
- Docs edits stay in memory. The request variables panel is not shown. Save as example is disabled.
- `applyAcceptedRequest` and the assistant's `findRequestTab` skip it.
- A "Temporary" badge shows in the request bar.

An absent `headers`, `params` or `body` in an example means empty. A Try tab never mixes in the parent's headers or body. An example with no `request` at all keeps the parent's request.

History: a Try send is recorded in history under the title `Try: <example name>` with no request path, so a history entry cannot write to the parent.

Known: declarative actions with `scope: request` or `folder` still write variables into the parent request or `folder.yml` during a Try send, exactly as sending the parent does.

## Not covered

- Examples for GraphQL, gRPC and WebSocket requests.
- Saving the editor state, not the saved file. An example is a snapshot of what produced the response, so unsaved edits are used and the dialog warns. Saving is not blocked, and the request file is not saved.
- Creating an example without sending a request.
- Editing an example in place, and form-data or binary request bodies in an example.
- Try tabs are not restored after an app restart.

## Manual check (not run yet)

Needs `yarn tauri dev`.

1. Send a login request and save `200 OK` as an example. See it nested in the sidebar and open it read-only.
2. Press Try, edit the copy, send. Check the badge, the disabled Save, Ctrl+S doing nothing, no prompt on close, and the parent request file unchanged.
3. Rename and delete the example. The open tab is retitled or closed.
4. Commit. The token and refresh token read `[masked]` in the commit, and the file on disk keeps them.
5. Export to Bruno and Postman. Examples are left out.
