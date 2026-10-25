# OpenAPI import and timed sync: design

Date: 2026-10-25. Status: draft for approval. Prior art: Bruno "OpenAPI Sync" (beta since v3.2.0).

## Intent

Teams publish an OpenAPI spec, often as a file in a GitHub repo, and the spec keeps changing. Today Rocket cannot import OpenAPI at all, so a user has to build requests by hand and keep them current by hand.

This feature has two parts, built in this order:

1. **OpenAPI import.** Turn a spec (file or URL) into a collection of requests grouped in folders.
2. **Timed sync.** The user gives one fixed URL once. Rocket re-checks it on a schedule, shows what changed, and applies only what the user approves, without destroying what the user wrote.

The point of the second part is that the user's own work (scripts, tests, docs, examples, edited values) is never lost when the spec changes.

## Goals

- Import OpenAPI 3.0, 3.1 and Swagger 2.0, in JSON or YAML, from a local file or an `https` URL.
- A pasted GitHub link works: `github.com/.../blob/...`, `raw.githubusercontent.com/...` and the contents-API form are all accepted and converted to a fetchable address.
- Folder layout is chosen at import time: by tag (default) or by URL path. This follows Bruno. The choice is saved and reused by every later sync, so the layout does not drift.
- The user sets a sync interval: 1 hour, 1 day, 7 days, monthly, or off. A "Check now" action also exists.
- Every update goes through a review. Nothing in the collection changes until the user approves.
- Private repositories work through an optional token kept in the OS keychain.
- Works the way a Rocket user expects: same collection format, same folders, same request editor.

## Non-goals (first version)

- Syncing while Rocket is closed. Checks run only while the app is open, plus one catch-up check on launch when a check is overdue.
- A command-line sync for CI (Bruno has an open request for this; it is out of scope).
- Hosts other than GitHub with a login (GitLab, Bitbucket, plain `https` with credentials). A plain public `https` URL is allowed; credentialed access for other hosts comes later.
- Syncing several spec versions of one API, or merging two specs into one collection.
- Webhooks or push updates.

## Anti-goals (a failure even if it technically works)

- A sync that overwrites or deletes something the user wrote without their approval.
- A token, or any secret value, written to a file in the collection (collections are committed to git) or to a log.
- A silent background change: the user must always be told a check found something.
- A half-applied update after a failed fetch or a crash: an update either applies as approved or not at all.

## What the user decided

- Updates are reviewed and approved before they apply (not applied automatically).
- The review is a **"Spec sync" tab**, opened like any other tab. The left side lists changed endpoints in three groups (New, Changed, Removed) with a checkbox each. The right side shows the selected endpoint's changes field by field, and lists what stays as the user has it. Buttons: skip this, apply selected. The header shows the last check time and the interval, and holds "Check now".
- Bruno's behaviour for local edits: the user's values, scripts, tests, assertions, docs and examples are kept. If the user and the spec changed the same thing, the user's value wins by default.
- Folder layout follows Bruno: tag (default) or path, picked at import.
- The first version is the scope in Goals and Non-goals above.

## Calls made by the designer (not yet confirmed by the user)

- A change the user skips stays in the list until the spec changes that endpoint again.
- A notice appears on the collection when a check finds changes; clicking it opens the Spec sync tab.
- **Removed endpoints.** If the user never edited the request, approving the removal deletes it. If the user edited it, approving keeps it as a normal request that no longer follows the spec, and the review says so.
- Request name: the spec `summary`, else `operationId`, else `METHOD /path`. Very long summaries are cut. (Bruno users complain about long Markdown descriptions being used as names.)
- `servers[0].url` becomes a collection variable (for example `baseUrl`) used in every request URL, not a hard-coded host. Secret values from security schemes are never imported; only the auth type and field names are.
- Intervals are a fixed list, not free-form.
- Hosts at launch: GitHub (public, and private with a token) and any public `https` URL.
- Reject `http`, `file` and other schemes; cap redirects (about 5); cap body size; set a timeout; never send the token to a different host after a redirect.
- If a fetch or parse fails, keep the last good copy, show the reason, and apply nothing.
- If the upstream ref is an immutable commit SHA, warn that nothing will ever change.

## Merge and identity rules (added after the builder check)

Answered by "follow Bruno":
- **User wins per field, not per request.** Merge is field by field: JSON bodies by key path, form fields, params and headers by name (keeping the user's value and enabled state), `{{var}}` references kept. Example: the user edits a header while the spec changes the body, so both apply. The user changes a param value while the spec removes that param, so the removal is shown in the review as a conflict and the user chooses. Same auth type keeps the user's auth values; a different type takes the spec's.
- Scripts, tests, assertions, actions, docs, examples, settings, tags and sequence are never touched by a sync.

Designer's calls:
- **"Edited by the user"** means: any difference between the current request and its stored baseline, in any field, including scripts, tests, docs and examples. The baseline covers the imported fields plus a hash of the rest, so an edit anywhere counts. When in doubt, treat the request as edited.
- **Renames in the spec.** A changed `operationId`, path or method shows as one Removed plus one New. For a request the user edited, approving the removal keeps it (detached), so nothing is lost. The review may pair the two when method and summary match and say "possibly renamed". Duplicate keys (no `operationId`, same method and path) get a numeric suffix in the order they appear.
- **User moves or renames a synced request or folder.** Identity is a stored operation key, not the path, so a moved request stays where the user put it and still syncs. New endpoints go into the folder for their tag (or path); that folder is recreated if the user deleted it.
- **One whole collection per source.** Sync is set up when the collection is created by an OpenAPI import. Attaching sync to an existing collection, and folder-level sources, are not in this version.
- **Applying is safe, not a database transaction.** Write the approved changes to a staging area, then move them in; if any step fails, restore what was changed. A small journal lets the next launch finish or undo an interrupted apply.
- **Skipping.** A skipped change does not move the baseline. The skip is stored with a hash of the spec version that was skipped, so it stays hidden until the spec's version of that endpoint differs from the skipped one.
- **Server URL.** The first entry of `servers` becomes the `baseUrl` collection variable; server variables use their defaults. If the spec's server changes it appears in the review as a "Variables" item, and the user's edit wins.
- **Bodies.** Example bodies are generated from the spec's example or schema at import and are part of the baseline. A schema change shows as a changed body; a user's body edit wins.
- **Schedules.** Checks run only for collections in the active workspace. The overdue test is per collection. The "changes found" notice survives a restart until applied or dismissed.
- **Token scope.** One token per collection, stored under that collection's id. Reuse across collections is a later improvement.
- **Size.** Cap the fetched body at 25 MB and design for specs up to about 5,000 operations (virtualize the review list; parse off the UI thread).

## Facts the builder needs

**Rocket today** (checked 2026-10-25):
- There is no OpenAPI importer. `rocket-import` handles Bruno, Postman and WSDL, always from a local file, always into a new collection (`crates/rocket-import/CLAUDE.md`, `importer.rs`). Import commands take a local path (`src-tauri/src/commands/import.rs`); the dialog is `src/components/import/ImportCollectionDialog.tsx` with source kinds in `importSources.ts`.
- Rocket exports a contract as OpenAPI 3.0.3 (`contract_service.rs:712`); that is the opposite direction and not reusable for parsing.
- Imports do not record where a request came from. Request identity is its file path (a `uid` is derived from the relative path when missing), which breaks if the file is renamed. `save_request` keeps only variables and examples when the incoming request has them empty, so a plain overwrite would erase scripts, tests, assertions and docs. A sync needs its own merge.
- Persistence has `extensions.rocketapi` slots on collection settings that can carry extra data without breaking older files (`crates/rocket-collection/src/settings.rs`). Persistence structs must not use camelCase renaming; only IPC DTOs do.
- The contract drift feature has a request signature snapshot and changelog (`crates/rocket-collection/src/contract/snapshot.rs`, `contract_service.rs`). It diffs method, URL, headers, query, body and auth only, not scripts or docs. It is a pattern to learn from, not a ready merge engine.
- There is no scheduler or background-task runner. All timers today are user-started or UI-only. App exit is handled in `src-tauri/src/lib.rs` (`RunEvent::Exit`).
- There is no import-from-URL anywhere. Outbound HTTP uses shared `reqwest` with proxy and TLS handling in `reqwest_executor.rs`. Some clients elsewhere have no timeout.
- Secrets go through the OS keychain wrapper (`crates/rocket-infra/src/secret_store.rs`, one service namespace per feature) and the `SecretStore` trait in `rocket-environment`. Git HTTPS tokens use a similar keychain entry.
- Project rules that apply: DDD placement (domain and traits in domain crates, I/O in `rocket-infra`, `rocket-app` trait-first, thin Tauri commands), shadcn/ui and lucide-react only, narrow Zustand selectors, no unwrapping of errors in production paths, no shelling out to `git`, read `docs/superpowers/specs/opencollection-spec-reference.md` before touching collection or YAML code.

**GitHub fetching** (from GitHub's docs, researched this session):
- A `blob` link is an HTML page; the raw file is `raw.githubusercontent.com/{owner}/{repo}/{ref}/{path}`. A ref containing `/` makes that form ambiguous; the contents API (`api.github.com/repos/{o}/{r}/contents/{path}?ref=`) avoids it.
- Use the contents API with `Accept: application/vnd.github.raw+json` and a token for private repos; use the raw host for tokenless public fetches. A fine-grained token needs Contents read.
- Send conditional requests (ETag, Last-Modified) and also store a body hash as a fallback. An authenticated 304 does not count against the rate limit; unauthenticated limits are 60 per hour per IP and are shared by everything on that IP.
- Hourly polling of one spec is one request per hour.

**Bruno** (for reference, not to copy): desktop only, checks every 5 minutes by default, per-endpoint review with a "preserve values" default, scripts and tests kept, free tier capped at 5 syncs per month, complaints about missed edits, items landing outside the folder structure, and slow first syncs. Where Bruno stores the source and interval is not confirmed.

## Open for the builder

- The OpenAPI parser: write one or use a crate; how much of `$ref`, `allOf`, `oneOf` and examples to resolve into bodies.
- What provenance to store and where: the source descriptor (host, owner, repo, ref, path), interval, layout choice, last check time, ETag and body hash, and per request an operation key (`operationId`, else method plus path) with a baseline hash of the imported fields. The constraints: it must support a three-way compare (baseline, current file, new spec), work offline, survive renaming a request file, and never contain a secret.
- Whether to keep a copy of the last spec on disk and where.
- The scheduler design: one supervisor started at app start and stopped on exit and on workspace switch, jitter, backoff on 403 or 429 (respect `Retry-After`), and the catch-up check at launch.
- Which crate owns the URL fetcher and the trait boundary.
- How a sync interacts with flows that reference a request, with the examples masking rule for git commits, and with a collection under git.
- Test strategy; `wiremock` is the project's HTTP mock.

## Plan split (proposal)

- Plan series 1, import: parser, converter, folder layout, file and URL import, GitHub URL handling, provenance written at import.
- Plan series 2, sync: fetcher with conditional requests, merge engine, scheduler, the Spec sync tab, notice and settings, token storage.
Each plan holds at most three tasks, per the project's planning rule.
