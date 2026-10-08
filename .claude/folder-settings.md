# Folder settings

Clicking a folder in the sidebar opens a Folder Settings tab with the sub-tabs Headers, Script, Test, Vars, Auth and Docs. The settings live in the folder's `folder.yml` and apply to every request below it when it runs.

Design: `docs/superpowers/specs/2026-10-07-folder-settings-design.md`. Plans and the locked names: `docs/superpowers/plans/folder-settings/00-plan-index.md`. On-disk shape and resolution order: `docs/superpowers/specs/opencollection-spec-reference.md` sections 2.7, 5 and 6.

## Where it lives

- `rocket-collection`, module `folder_settings`: `FolderSettings`, `ScriptFlow`, `ScriptPhase` and the pure helpers `inherited_headers`, `resolve_folder_auth`, `chain_scripts`. No I/O. `CollectionSettings.script_flow` holds the collection's flow.
- `rocket-collection` `CollectionRepository`: `get_folder_settings`, `save_folder_settings`, `get_folder_chain_settings`. All three have defaults, so existing test doubles still compile. `get_folder_variables`, `save_folder_variables` and `get_folder_chain_variables` read and write the same `request.variables`.
- `rocket-infra` `fs_collection/folder_file.rs`: reads and writes `folder.yml` (spec shape, legacy shape fallback). The settings methods are in `fs_collection/folder_settings.rs`, and the variable methods in `fs_collection/variables.rs`. `conversions/folder_settings.rs` holds the mapping and `folder_oc_variables`. `fs_collection/settings.rs` reads and writes `extensions.bruno.scripts.flow` in `opencollection.yml`.
- `rocket-shared` events: `DomainEvent::FolderSettingsSaved { collection, folder_path }`, published by `rocket-app` `collection_service.rs` after a save.
- `rocket-app` `execution_service.rs`: `resolve_request` merges folder headers and auth, and the script phases use `chain_scripts` (`execution_service/script_chain.rs`). The folder chain comes from `get_folder_chain_settings` via `folder_chain`. Folder scope for scripts is the `rok` folder-variable getter (`getFolderVar`).
- `src-tauri/src/commands/collections.rs`: thin `get_folder_settings` and `save_folder_settings` commands. The camelCase `FolderSettingsDto` is in `src-tauri/src/commands/folder_settings_dto.rs`.
- Frontend: `src/lib/tauri-api.ts` (`getFolderSettings`, `saveFolderSettings`, the `FolderSettings` type), `src/stores/pane-store.ts` (`openFolderTab`, `updateFolderSection`), `src/hooks/useFolderSettings.ts`, `src/components/collections/FolderSettingsTab.tsx` with its sub-tab components in `src/components/collections/folder-settings/`, and `src/components/collections/FolderNode.tsx` (click opens the tab, the chevron still expands).
- Tests: Bruno compatibility in `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs`; end to end execution in `crates/rocket-app/src/execution_service/folder_chain_e2e_tests.rs`.

## Rules that are easy to break

- `folder.yml` is strictly OpenCollection (`additionalProperties: false`). No Rocket-only keys. The schema guard in `schema_shape_tests.rs` fails if one appears. Do not add to `KNOWN_DEFERRED` for a folder key.
- A save reads the file first and only replaces `headers`, `auth`, `variables`, the three typed scripts and `docs`. `request.metadata`, `request.settings`, `info.seq` and `hooks` scripts must survive. An empty section is omitted.
- `CollectionVariable` has no description field. Folder saves keep each variable's existing description by name (`folder_oc_variables` in `conversions/folder_settings.rs`). Loading fills `initial_value` from `value`, so an equal pair means no initial value and `initial` is not written.
- `docs` is read as a string or `{ content, type }` and written as a plain string.
- Absent auth and `inherit` both mean no folder auth. The folder Auth UI saves Inherit as an absent key. A request set to `none` also inherits (existing known difference, spec reference section 3.1).
- On the backend, header names match exactly (no case folding), like the collection-versus-request merge. The frontend send path (`src/lib/folder-inheritance.ts`) matches names without case. A disabled header never shadows another.
- Script flow is per collection, at `extensions.bruno.scripts.flow`. There is no per-folder flow and no UI for it. `save_settings` must keep every other key under `extensions`.
- Collection-level scripts in `opencollection.yml` are preserved but not run. Only folder and request scripts are chained.
- Folder variables are pre-request only. A script can still set variables after a response.
- A `folder.yml` that does not parse during a send is an error naming the folder. The sidebar tree load is more forgiving and must stay light, which is why `FolderSettings` is not a field of `Folder`.
- The Folder Settings save is read-then-patch: it re-reads the baseline and applies only the edited fields. The hook does not listen to `folderSettingsSaved`. A future listener must ignore its own saves and must not reload while the tab is dirty.
- Scripts that use `bru.*` stay non-portable. There is no `bru` alias, parity goes in the `rok` namespace.

## Known gaps

These were found in review and left on purpose. Check them before building on the area.

Scope not covered:
- gRPC requests do not inherit folder settings. `grpc_service.rs` does not read the folder chain.
- Collection-level scripts do not exist as a feature. They are preserved in `opencollection.yml`, never run.
- Importers do not read or write the script flow (`extensions.bruno.scripts.flow`). A Bruno collection imported through `rocket-import` ends with the default sandwich flow. Hand edits to `opencollection.yml` are kept by Rocket saves.
- There is no `folder.bru` importer. A Bruno `.bru` collection loses folder-level settings on import.
- Folder OAuth2 on Flow and the collection runner has no interactive token step. A non-client-credentials grant (auth code, password, implicit) with no cached token sends no Authorization header, silently. Folder OAuth2 goes out as a plain bearer and ignores `addTokenTo` and `headerPrefix`.

Header and auth behavior:
- `inherited_headers` collapses same-name duplicates inside one level when a folder sets enabled headers. The legacy `merge_headers` keeps them (for example two `Set-Cookie`). With no enabled folder headers the legacy merge is used.
- Header-name matching is case-sensitive on the backend and case-insensitive on the frontend send path (existing rules, kept as is).
- The backend treats a request's `none` auth like `inherit`, so a frontend `none` request gets folder auth.

Folder OAuth2 token cache (frontend `folder-auth-store`):
- The fingerprint compares raw `{{var}}` strings, so an environment switch does not invalidate a cached token. This is systemic, the collection and request stores do the same.
- There is no expiry check on a cached folder token.
- Tokens are not cleared when a folder is renamed or deleted. `clearFolderAuth` wiring is a follow-up.
- Fetching a token marks the tab dirty even when the persisted shape did not change.

Execution:
- The folder chain is read twice per execution, once for the secret scan in `execute` and once in `begin_phases`. A transient failure between the reads could let an unscanned folder script run. The effect is an empty secret substitution, not a leak. Fix: read once and pass it to both.
- Folder variables marked secret are not added to the secret values, so they are not redacted in script console output. Collection variables are. `rok.getFolderVar` makes them easier to reach.
- `getFolderVar` returns `""` for a missing key.
- `FsCollectionRepo::validate_path` is a lexical check and lets `ghost/../../evil` pass (it predates this work). `save_folder_variables` could create directories outside the collection. Consider rejecting parent-dir components.

File churn and UI:
- A first save of a Bruno `folder.yml` rewrites `timeout: 5000` as `5000.0`, because `InheritableNumber` is an f64. The value is still a schema number.
- `MarkdownEditor` edit mode is a shadcn Textarea, not Monaco. The Docs tab (and the collection and workspace docs tabs) inherit this.

## Manual checklist

Automated tests cannot see the real window. The user runs these. Do not tick them on the user's behalf.

Run `yarn tauri dev`. In a throwaway collection make a folder `outer` that holds a folder `inner`, and a request `ping` in `inner` that points at an echo endpoint (for example `https://httpbin.org/anything`).

Tab basics:
- [ ] Click folder `outer`. A Folder Settings tab opens with Headers, Script, Test, Vars, Auth and Docs. The chevron still expands and collapses without opening a tab. The folder menu has Settings.
- [ ] Click `outer` again. The same tab is focused, no second tab opens. Open `inner` and confirm it gets its own tab.
- [ ] Press Enter on a folder row. It does not open the tab (the Settings menu item does). Confirm this is acceptable.
- [ ] Rename a folder from the dropdown menu, then rename another from the context menu. Both work, and an open tab for the folder follows the new name. (The rename test runs in jsdom with a mock, so the dropdown path has no real-window coverage.)
- [ ] Delete a folder that has an open Folder Settings tab. The tab closes or shows a clear not-found state, with no crash.

Layout:
- [ ] Check the sidebar tree indent. There is an extra 12px spacer next to the 16px chevron, so the gutter may look shifted.
- [ ] Docs tab: the pane fills the height (`h-full` inside an `overflow-auto` container) and scrolls correctly.
- [ ] Script and Test tabs: the Monaco editor has a sensible height (same `h-full` inside `overflow-auto` risk).

Behavior:
- [ ] Headers: in `outer` add `X-Outer: outer` and a disabled `X-Col: shadow`. In `inner` add `X-Shared: inner`. Save. The Save button clears its dirty marker. Send `ping`. The echo shows `X-Outer` and `X-Shared`, and no `X-Col`.
- [ ] Vars: add `region = eu` in `outer`, save, reload the app window, and confirm it is still there. Use `{{region}}` in the URL or a header and confirm it resolves.
- [ ] Auth: set `outer` to Bearer `outer-token`, then set `inner` to `inherit`. Set the request auth to Inherit. Send `ping`. The echo shows `Authorization: Bearer outer-token`. Change `inner` to Bearer `inner-token`, save, send again. It shows `inner-token`.
- [ ] Script order: in `outer` add a pre-request script `console.log('outer pre')` and in `inner` `console.log('inner pre')`. Add a post-response script in each that logs `outer post` and `inner post`, and `console.log('req post')` in the request's own post-response script. Send. Pre runs outer, inner. Post runs request, inner, outer.
- [ ] Test: add `test('folder test', function () {})` in `outer`'s Test tab. Send and confirm it appears in the test results.
- [ ] Nested run: send `ping` and confirm headers, auth and script order are all right together.
- [ ] Docs: write Markdown in `outer`'s Docs tab, save, reload, and confirm it renders.
- [ ] Script flow: add `extensions:\n  bruno:\n    scripts:\n      flow: sequential` to `opencollection.yml` by hand, reload, and send `ping`. Post-response order is now outer, inner, request. Then remove the key (or set the default) and confirm sandwich order returns. Try both flows.
- [ ] Folder OAuth2: set `outer` auth to OAuth2 client credentials and fetch a token. Then edit the client id. The token must clear.

File and Bruno compatibility:
- [ ] Open `outer/folder.yml` in a text editor. It has only `info`, `request` and `docs`. `request` has only `headers`, `auth`, `variables`, `scripts`. No `scriptFlow`, `initialValue` or empty lists.
- [ ] Edit `outer/folder.yml` by hand: add `request.metadata`, `request.settings.timeout: 5000` and a `hooks` script. Reload the collection, change a header in the tab and save. All hand-added keys are still in the file. (Expect `timeout: 5000` to become `5000.0`.)
- [ ] Hard compatibility check: open a collection authored in Bruno that has folder settings, edit a folder in Rocket, save, and reopen the collection in Bruno. It must load with no schema error and show the folder's settings. Include a `folder.yml` that has `request.metadata`, `request.settings` and `hooks`.

Report any failing item with the folder tab's state and the `folder.yml` text, so the failure can be traced to the owning plan.

## Deferred

`folder.bru` import, folder post-response variables, per-folder script flow, a UI for the script flow.
