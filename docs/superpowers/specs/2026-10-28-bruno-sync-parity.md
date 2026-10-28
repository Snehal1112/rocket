# Bruno OpenAPI Sync parity

Researched 2026-10-28. Sources: Bruno docs (https://docs.usebruno.com/open-api/openapi-sync), the Bruno blog, discussion https://github.com/usebruno/bruno/discussions/7401, and the code of the installed Bruno 3.2.0 (`/opt/Bruno/resources/app.asar`: `src/ipc/openapi-sync.js`, the renderer bundle, `@usebruno/converters`), read-only. Rocket facts come from an audit of branch `openapi-remote-refs` on the same day. Used by `2026-10-28-openapi-sync-ux-3-design.md`.

Status words: **Done** (exists on the branch today), **Planned** (a plan covers it, not built), **Gap** (no plan before series 5), **Beyond** (Rocket does more), **Differs** (a deliberate difference).

## 1. Source and connection

| Feature | Bruno 3.2.0 (verified in code) | Rocket today | Status and plan |
|---|---|---|---|
| Source kinds | URL or local file (`.json`, `.yaml`, `.yml`); any `http`/`https` URL, no host rule | https link or `http://localhost`; a file source (relative path, approval, hash change detection) | Done: file source (series 4 plans 02 and 03) |
| Local-file storage | Picked absolute path stored collection-relative, resolved at read time | none | Done: series 4 plan 02 |
| Local-file reload | No watcher; re-read on a check (manual, timer, opening the tab) | n/a | Done: same rule, series 4 plan 02 |
| http / localhost | Any http or https URL | https only | Differs: loopback http only (series 4 plan 01), because Rocket asks consent but Bruno does not and a private-address first hop is a bigger risk here |
| Authenticated spec URLs | Not supported | Github token only | Done (Beyond) |
| Swagger 2.0 | Refused: "Swagger 2.0 is not supported. Please convert your spec to OpenAPI 3.x." | Imported | Beyond |
| Edit source later | `Edit connection settings` modal, `Spec Source` URL/File toggle; saving probes the new source, then re-checks; nothing is reset, the new source is diffed against the stored spec; no match-count warning | Not possible (Disconnect, then Connect) | Done: Change link (series 3 plan 01) with a match-count warning; a check right after checks are turned on (series 5 plan 05) |
| Invalid new source | Toast "The URL does not point to a valid OpenAPI 3.x specification" | n/a | Done: a fixed error sentence in the form |
| Auto-check | Switch plus 5/15/30/60 min; default 5; renderer tick every 5 minutes; no retry or backoff | Switch plus 5/15/30 min, hourly, daily, weekly, monthly; default 5; backoff on failures | Done (Beyond) |
| Rate-limit hint | none | none (only failure sentences after the fact) | Done: series 3 plan 05 (Beyond) |
| Manual check | `Check for updates` button in the card header | `Check now` | Done |
| Disconnect | Confirm "This will only disconnect the sync configuration. Your collection will remain intact." Deletes the stored spec copy | Disconnect dialog, requests stay | Done |
| Offline spec copy | App userData `specs/<uuid>`, not in the collection | `.rocket/openapi-applied.spec` in the collection (git-ignored) | Done (Differs, collection-local) |
| Stored copy missing | Banner "Last synced spec not found" with `Restore Spec File` | Check heals a missing copy automatically | Done (Beyond) |
| Quota | 5 syncs per 30 days unless licensed | none | Differs on purpose |

## 2. Pages, tabs and messages

| Feature | Bruno | Rocket today | Status and plan |
|---|---|---|---|
| Tabs | Overview, Collection Changes, Spec Updates | Overview, Spec updates (N), Your changes (N) | Done |
| Overview cards | Total, In sync, Changed, Spec updated pending; last synced; title and version | Endpoints, In sync with spec, Changed by you, Spec updates waiting; link, version, last checked, last synced | Done |
| Check failure banner | Overview: "Failed to check for spec updates" plus the error and `Update connection settings`; Spec Updates empty state: "Unable to check for updates / Fix the connection issue above and check again." | Done (was a header alert with text only) | Done: a banner with the action on Overview and Spec updates (series 5 plan 05) |
| Missing file | "Source file not found at <path>" with `Update connection settings` | n/a | Done: fixed sentence without a path and `Choose another file` (series 4 plan 03) |
| No updates | "No updates from the spec" | "No spec updates waiting" | Done |
| No local changes | "No changes in collection. The collection endpoints match the last synced spec." | "You have not changed anything." | Done: clearer text (series 3 plan 03) |
| View spec | Read-only viewer of the stored spec; falls back to the live source; "No spec file found. Sync your collection first." | Tag-grouped list with parameters and response codes only | Done: series 5 plan 01 |
| First connect of an existing collection | Matching by `METHOD:normalizedPath` only; no differences means sync starts at once; otherwise an "Initial sync required" banner and a Review page | Initial sync review with matched, ambiguous groups and a digest-checked confirm | Done (Beyond) |
| Collection Changes lists | Added, Deleted, Modified with counts | Modified, Added by you, Deleted by you, Reset by you | Done |
| Values in Your changes | Diff shown on the review page | Field names only | Done: series 3 plan 02 |
| Open request | `Open` on a locally added endpoint | none | Done: series 3 plan 02 |
| Tracked drift fields | URL, params, headers, body, auth mode (not scripts, tests, assertions, settings) | Name, method, URL, description, query, path, headers, form, body, auth | Done (Beyond) |

## 3. Review and apply

| Feature | Bruno | Rocket today | Status and plan |
|---|---|---|---|
| Bulk actions | `Accept All`, `Skip All`, `Keep Current`, `Confirm & Sync Collection` | Same names, Accept all warns on edited requests, Skip all asks first | Done |
| Per-endpoint choice | New: `Skip`/`Add`; modified: `Keep Current`/`Update`; removed: `Keep`/`Delete` | Per item approve or skip, per field Take spec/Keep yours; removed items are never preselected | Done (Beyond) |
| Preserve values | Toggle on the review page, ships in 3.2.0, default on | No global toggle; Accept all takes the spec on conflicts after a warning | Done: series 5 plan 04 |
| Conflicts | `N Conflicts`; button "Resolve N conflict(s) and sync" | Conflicts start as Keep yours; no count | Done: series 5 plan 04 |
| Diff preview | `View Spec Diff` button | Baseline / Yours / Spec per field, unchanged fields hidden | Done (Beyond) |
| Confirm dialog | `Confirm Sync`, `Confirm & Sync Collection` | Confirm and sync | Done; series 5 plan 04 adds counts if missing |
| Skipped items | In memory only. After sync a skipped new endpoint shows under `Deleted from Collection` with `Restore` and `Restore All` | Persisted in `openapi-local.json`; shown as a count only | Done: skipped list with Un-skip (series 3 plan 04) |
| Removed from spec | Accepting a removed endpoint deletes the file only if method and path still match | Deleted only when unedited; edited ones are detached and kept | Done (Beyond); Gap: no way to delete an edited removed request (series 5 plan 02) |
| Local-only requests | `Delete`/`Delete All Local Endpoints` with "cannot be undone" confirm; `Revert All Changes` | Listed as "Added by you", no delete action | Done: series 5 plan 02 |
| Reset | `Reset`/`Reset All` replaces URL, params, headers, body, auth; keeps scripts, tests, assertions; confirm "Your local changes will be lost." | Replaces 10 fields (also name, method, description), backup and Restore, refused with an untrusted baseline | Done (Beyond); Differs: scope, series 5 plan 03 aligns it |
| Reset source | The spec | The last synced baseline | Differs on purpose (documented in series 5 plan 03) |
| First sync | Overwrites, irreversible | Review first | Done (Safer) |

## 4. Merge rules

| Rule | Bruno | Rocket (file) | Status and plan |
|---|---|---|---|
| JSON body | Keeps user values, adds new fields with examples, drops removed | Three-way key-path merge to depth 64 (`json_merge.rs`) | Done (Beyond) |
| JSON arrays and top-level array bodies | Not documented | Whole value is a leaf; user edit wins | Differs, safe; documented in series 5 plan 03 |
| Urlencoded body | Keyed by field name | One text blob (`merge.rs:574`) | Done: series 5 plan 03 |
| Headers, query, path, form-data | Values and enabled kept, new added, removed deleted | Same, by name | Done |
| Auth | Same mode keeps values; mode change updates and shows in review | Same plus conflict handling | Done |
| `{{baseUrl}}` | Stays as written | Same | Done |
| Scripts, tests, assertions, settings | Always preserved | Always preserved | Done |
| Moved or renamed request | Not described | Followed by uid; user rename wins | Done (Beyond) |
| User-deleted request | Appears under `Deleted from Collection` with Restore | Stays deleted (listed under Deleted by you) | Done |

## 5. Folders and naming

| Rule | Bruno | Rocket | Status |
|---|---|---|---|
| Group by | `tags` or `paths`, chosen in the Import dialog only; Connect always uses tags | Tag or Path in Import and Connect; only for new folders; no regroup later | Done (Beyond); regroup is not a Bruno feature, so no gap |
| Tag mode | First tag, sanitized; no tag goes to the root | Same idea | Done |
| Path mode | First segment folder, deeper nest, root path in `Root`; Bruno's sync puts new requests at the root in path mode | Layout kept on every sync | Done (Better) |
| Request name | summary, else operationId, else description, else `METHOD path`; duplicates `name (METHOD)` then `name (n)` | summary, operationId, `METHOD path`; duplicate suffix `X (n)` | Done |
| Reserved names | `node_modules`, `.git`, `environments`, `mocks` fall back to the root | Not audited | Check in series 5 plan 03 task 3 |

## Rules for the plans

- A difference from this table needs a `Ruling:` line in the ledger. "Beyond" and "Better" rows are allowed.
- Rocket never deletes a request it cannot show the user first, and never contacts a link or file before consent. Those are deliberate differences from Bruno.
