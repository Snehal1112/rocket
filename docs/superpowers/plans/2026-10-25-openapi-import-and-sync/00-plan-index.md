# OpenAPI import and sync: plan index

Spec: `docs/superpowers/specs/2026-10-25-openapi-import-and-sync-design.md`. Each plan holds at most three tasks. Plan files are written one at a time, just before they run, because each one names the real signatures left by the plan before it. Status here is the source of truth.

Rules for every plan:
- Tasks that touch `rocket-import`, `rocket-collection`, `rocket-infra` or collection files start with: "Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`."
- Cargo commands use `-j4` and `-p <crate>`. Never `--workspace`.
- shadcn/ui primitives and lucide icons only, narrow Zustand selectors, never `unwrap()` in production paths, camelCase renames on IPC DTOs only.
- Commits go through `dev-workflow-skills:1-git-commit` with explicit paths, never `git add -A`.
- Gates before a commit: `cargo check -j4` and targeted `cargo test -j4 -p <crate> <name>` for Rust, plus `yarn tsc --noEmit`, `yarn check` and targeted `yarn test` for TypeScript.

## Series 1: import

| ID | Plan | Tasks | Needs | Models | Status |
|---|---|---|---|---|---|
| P1 | `plan-01-parse-and-convert.md`: OpenAPI reader, request converter, layout and import service (pure, no network) | 1 reader and neutral model; 2 operation to request, naming, baseline hash; 3 tag and path layout, `import_openapi_text` | none | sonnet; opus reviews task 3 | done on branch openapi-import (94e54652..aaeefd2c), final review fixed, not yet merged to main |
| P2 | `plan-02-fetch-and-source.md`: spec link parser, safe fetcher (port in `rocket-http`, reqwest impl in `rocket-infra`), source record and sidecar repo | 1 `SpecLocation` and `parse_spec_url`; 2 `SpecFetcher` and `ReqwestSpecFetcher`; 3 `.rocket/openapi-source.yml` repo and `source_record` | P1 | sonnet; opus reviews task 2 | done on branch openapi-import (8afbd278..76e39a59), final review fixed, not yet merged to main |
| P3 | `plan-03-import-command-and-dialog.md`: import from a file or a link, Tauri commands, OpenAPI tab in the Import dialog | 1 `import_openapi_file` and `import_openapi_url` on `ImportService`; 2 Tauri commands and TS api (no query layer needed); 3 dialog tab (link or file, layout choice) | P2 | sonnet; opus reviews task 1 | done on branch openapi-import (0d3d659c..db9dede4), final review clean, not yet merged to main |

## Series 2: sync (outline, each plan is written after the one before it is merged)

| ID | Plan | Needs |
|---|---|---|
| P4 | `plan-04-check-and-token.md`: sync state in the source file, `OpenApiSyncService` (conditional fetch, pending copy, token in the keychain), import with a token. Done on branch openapi-import (a26a3384..71594576), final review clean | P2, P3 |
| P5 | Three-way diff and per-field merge engine (pure) | P1, P2 |
| P6 | Apply approved changes with a staging area and recovery journal | P5 |
| P7 | Scheduler: supervisor, intervals, jitter, backoff, launch catch-up, notice | P4 |
| P8 | Spec sync tab: list and detail | P5, P6 |
| P9 | Spec sync tab: skip, apply, Check now, interval settings, token field | P7, P8 |

## Decisions log

- Ruling (2026-10-25): the design is approved. Import ships before sync. Provenance lives in a sidecar file in the collection folder (`openapi-source.yml`), keyed by request `uid`, because `Request` has no extension slot.
- Ruling (2026-10-25): the sidecar lives at `<collection>/.rocket/openapi-source.yml`, not at the collection root. A dot folder is already hidden from the collection loader and the Bruno export, so no loader change is needed (the contract feature uses `.rocket/contracts/` the same way). It never holds a token.
- Ruling (2026-10-26, Plan 4): a locked keychain must not break checks for public repositories. `check` reads the token with `get_strict`, falls back to a tokenless fetch on a keychain error, and says so only when that fetch fails with not found or unauthorized. A tokenless 404 now tells the user to add an access token for a private GitHub repository.
- Ruling (2026-10-26, Plan 4): the default sync interval is off, so a new import makes no background requests until the user opts in.

## Carry-forward from the Plan 4 final review

- **P9 must decide before `check` is reachable from the UI (Important).** The token is keyed only by `source_id`, which sits in a committed YAML file. A collection from someone else that reuses a known `source_id` and points `location` at a private GitHub file would make the victim's check send the victim's token to that path, and the content could land in `.rocket/openapi-pending.spec`. Fix by gating `check` behind the collection trust gate, or by binding the keychain entry to the location (store owner and repo with the token and refuse on a mismatch). Update the spec line "one token per collection" too. Copies of a folder share one token by design.
- P5/P6: before applying, verify `sha256(load_pending()) == pending_sha256`. On a 304 with `pending_sha256` set but the pending file missing, drop the stored ETag and Last-Modified and fetch again, or the check stays on "changed" with nothing to apply.
- P7: every check rewrites `openapi-source.yml` (`last_checked_at`, `last_status`), so a collection under git gets a diff each interval. Decide where timing state lives before the scheduler.
- P9: `status().has_token` uses the lenient keychain read; use the strict read. An unknown `sync_interval` or `state` value is a load error; decide on forward compatibility.
- Small: `OpenApiImportResult.source_id` is returned even when the source file save failed (return `None`); the TS `OpenApiImportResult` lacks `sourceId`.

## Series 2 decisions (2026-10-26)

- Ruling (user): keep a copy of the last applied spec at `<collection>/.rocket/openapi-applied.spec`. The converter is deterministic, so the per-field baseline is rebuilt by converting that copy. Collections imported before this have no copy; their first sync only knows whether a request was edited, and treats an edited request as the user's (nothing is overwritten).
- Ruling (user): the spec copies (`openapi-applied.spec`, `openapi-pending.spec`) live in `.rocket/` and are git-ignored there through a `.rocket/.gitignore` that the code writes. A fresh clone has no copies, so its first check fetches the spec and treats it as the new baseline question described above.
- Ruling (user delegated, designer's call): "possibly renamed" pairing is in the first version as a **hint only**. P5 detects candidates and returns them in the diff; P8 shows them; P9 adds a "Link as rename" action that re-points the stored operation key to the new one so the user's edits stay and the request is merged as Changed. Nothing is ever paired automatically. Rules: pair only when exactly one Removed and one New candidate match on each side, by (a) same operationId with a new method or path, (b) same method and same path template with a new operationId, or (c) same method and summary with a new path. Ambiguous matches are not offered.
