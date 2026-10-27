# OpenAPI sync UX 3: change link, values, next steps, skipped items, local sources

## Goal

Close the gaps the manual review of series 1 and 2 found: users cannot change the spec link, cannot see what they changed, get no next step after a failed check, cannot see or undo a skip, and cannot sync from a local file or a localhost server.

## Current state (verified in code on 2026-10-28)

- The source (`.rocket/openapi-source.yml`, committed) holds `location` (`SpecLocation::Github` or `Https` only), `spec_sha256`, `etag`, `last_modified`, `operations`, `source_id`, `pending_sha256`, `remote_refs`. Consent is machine-local and keyed by `source_id` plus the JSON of the location. The token is in the keychain, Github links only.
- The only way to change the link today is Disconnect, then Connect (this loses the baseline, skips and backups).
- A file import writes no source, so the collection does not sync and offers "Connect to an OpenAPI spec", which accepts links only.
- `parse_spec_url` and the fetcher accept `https` only. The first hop has no private-address check (https to a private address already works); redirects may not reach loopback, private addresses or http.
- "Your changes" rows carry field names only. `SyncItemDto::New` drops the converted request. Values show only in Spec updates, clipped at 500 characters.
- Failure states show text only. There is no Add token action. Skips live in git-ignored `openapi-local.json` as `key -> fingerprint` and show only as a count.

## Non-goals

- Version-to-version spec diff; scripts, tests and assertions in Your changes; a new private-network setting; watching a file with a filesystem watcher (a local file is read on each check).
- Changing consent, token or scheduler rules, except as listed below.

## Design

### 1. Change the spec link (series 3)

- A **Change link** form in the connection dialog. Two commands: `openapi_sync_change_link_preview(collection, link, token?)` and `openapi_sync_change_link_confirm(collection, link, token?, specSha256)`.
- Preview validates like Connect (`parse_spec_url`, real fetch, `ParsedSpec::parse`), refuses the current link (`SAME_LINK`) and an unconnected collection, and returns `{ specSha256, title, version, matched, added, removed, sameSpec }`. `matched` counts source operations that exist in the new spec. The UI warns when fewer than half match.
- Confirm re-fetches (stale check on `specSha256`), then under `collection_lock` and `recover_if_idle` commits one file transaction: new `location`, `etag` and `last_modified` cleared, `remote_refs` from the new bundle. The baseline, `operations`, `spec_sha256`, the applied copy, skips, backups and the interval stay. If the new spec differs from `spec_sha256`, it becomes the pending copy so the user reviews it in Spec updates like any other update; if it is identical, nothing is pending.
- After commit: consent is granted for the typed link (as import and Connect do). The token is kept only if the new link is a Github link with the same owner and repo; otherwise it is deleted. Failures after commit become warnings.

### 2. Values in Your changes and New items (series 3)

- `ChangeRowDto.changes: [{ field, baseline, yours }]` for modified rows when the baseline is trusted, reusing `merge_request` and its 500-character clip. `fields` stays.
- `SyncItemDto::New.preview: { method, url, headers, query, hasBody, authKind }` built from the converted request already in the plan (names only for headers and query, never values, so secrets in examples do not reach the UI).
- An **Open request** button on modified rows, and on added rows, opens the request tab. A helper `openSyncRequest` builds the tab like `RequestNode.createTab`.

### 3. Next steps on failures and empty states (series 3)

- The failed-check alert gets an action by code: `not_found` and `unauthorized` on a Github link give **Add access token** (opens the connection dialog at the token field); `network`, `status` and `rate_limited` give **Check now**.
- Empty Spec updates gets **Check now**. Empty Your changes says how changes appear. The connection dialog accepts `focus: 'token'`.

### 4. Skipped items (series 3)

- `openapi_sync_skipped(collection)` returns `[{ key, name, method, path }]` for items hidden by a skip (offline, from the pending copy). `openapi_sync_unskip(collection, keys)` removes keys from `local.skips` under the lock and refreshes `pending_count`. Neither contacts the network.
- The "N skipped, hidden" line becomes a button that opens a **Skipped** list with **Un-skip** and **Un-skip all**.
- The Import dialog says, in file mode, "A spec imported from a file is not kept in sync. Import from a link to get updates." Series 4 replaces this sentence.

### 5. Connection settings dialog (series 3)

- The dialog is named **Connection settings** everywhere. It shows a status line (last checked, or the failure sentence) and a **Check now** button. An interval of 5 minutes on a Github link without a token shows the hint "GitHub allows 60 requests an hour without a token. Pick a longer interval or add a token."

### 6. Localhost over http (series 4)

- `parse_spec_url` accepts `http://` only when the host is `localhost`, `127.0.0.1` or `[::1]`. It is stored as `Https { url }` (no new variant, no version bump). The fetcher's first hop allows http for those hosts only. Redirects to http stay blocked. Tokens are never sent over http (they are Github-only already).

### 7. Sync from a local file (series 4)

- A new `SpecLocation::File { path }`. Like Bruno, the path is stored relative to the collection root (`../specs/api.yaml` is allowed) so the committed source works on another machine that has the same layout; a path that cannot be relative (another drive) is stored absolute. It is resolved against the collection root at read time. A source file with this variant is written with `version: 2`; builds that only know version 1 refuse it, and a source with another location stays version 1. Display form is `file://<resolved absolute path>`; `parse_spec_target` parses `file://` or https and is used where consent round-trips.
- A `LocalSpecReader` trait (rocket-http) with `FsLocalSpecReader` (rocket-infra): regular files only, 25 MB cap, UTF-8. `check()` reads the file for a File location; change detection is the SHA-256 of the bundled text (no validators, no backoff). Relative `$ref`s are not followed and are reported with the existing unresolved-ref note.
- Consent is per machine, so a cloned collection asks for approval first (fail closed). A missing file shows the sentence "The spec file was not found on this computer." (`file_missing`; Bruno's equivalent is `SOURCE_FILE_NOT_FOUND`). A file that is not valid JSON or YAML gives the existing `not_a_spec` sentence.
- The Import dialog gets **Keep in sync with this file** (default on) in file mode. Connect and Change link accept a file through a Link/File switch.

### 8. Bruno parity additions (series 5)

Found by comparing with Bruno 3.2.0 (`2026-10-28-bruno-sync-parity.md`):

- **View spec detail:** request body schema, response schemas, security schemes and servers, as bounded trees (depth 6, 200 nodes, no examples).
- **Delete actions:** delete a request added by you, and delete an edited request the spec removed, each with a confirm that says it cannot be undone from the page (git can restore it). No network.
- **Merge:** urlencoded bodies merge per field; Reset replaces URL, params, headers, body and auth only (like Bruno) and keeps name, method and description; the known limits (arrays, top-level array bodies, Reset targets the baseline) are documented.
- **Review:** a **Keep my values** switch (default on, like Bruno's Preserve values) decides whether Accept all keeps the user's value on conflicts; a conflict count with a filter.
- **Failures:** the Overview and Spec updates pages show the check failure with the next-step button; saving settings that turn checks on runs one check.

## Error handling

- All new messages are fixed sentences added to `KNOWN` in `syncErrors.ts`; no raw backend or spec text over IPC.
- Change link, skip and unskip never run during an apply (`recover_if_idle`, Conflict).

## Acceptance criteria

1. A connected collection can move to another link; the baseline, skips and backups survive; a differing spec appears in Spec updates.
2. Moving to a non-matching repo drops the stored token; a same-repo move keeps it.
3. Your changes shows baseline and current values for modified rows and an Open request button; New items show a preview without header or query values.
4. A `not_found` failure on a Github link offers Add access token; a network failure offers Check now.
5. A skipped item can be listed and un-skipped without the network.
6. `http://localhost:3000/openapi.json` can be imported and synced; `http://example.com/x` is still refused.
7. A collection imported from a file with Keep in sync updates when the file changes.

## Decisions

- Link change keeps the baseline and routes the new spec through review. Options: (A) reset everything like a new connect, (B) keep the baseline and review the new spec as pending. Chosen: B. Decided by: call
- Localhost over http reuses `Https` rather than a new variant, because the host rule is enough and old builds then still read the file. Decided by: call
- A local file is a new variant with a version bump, because a path is not a URL and consent must key on it. Decided by: call
- This spec reverses the series 1 and 2 decisions "Connect accepts a link only" and "Connecting from a local file is out of scope". Decided by: call (the user asked for the local-file gap)
- "Keep my values" reverses the series 1 call "Accept all takes the spec on conflicts and warns first", because Bruno's Preserve values is on by default. The old behavior stays available with the switch off. Decided by: call (revisit if the user prefers the old default)
- Reset scope follows Bruno (no name, method or description). Decided by: call
- Loopback-only http stays stricter than Bruno (any http). Decided by: call
- Work is split into two plan series: series 3 (`openapi-sync-ux-3`, five plans) and series 4 (`openapi-sync-ux-4`, three plans for local sources). Decided by: call
