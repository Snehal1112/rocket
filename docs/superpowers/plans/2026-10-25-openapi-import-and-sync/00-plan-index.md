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
| P3 | `plan-03-import-command-and-dialog.md`: import from a file or a link, Tauri commands, OpenAPI tab in the Import dialog | 1 `import_openapi_file` and `import_openapi_url` on `ImportService`; 2 Tauri commands and TS api (no query layer needed); 3 dialog tab (link or file, layout choice) | P2 | sonnet; opus reviews task 1 | written, waiting for review |

## Series 2: sync (outline, each plan is written after the one before it is merged)

| ID | Plan | Needs |
|---|---|---|
| P4 | Conditional fetch with ETag and body hash, token in the keychain, sync settings (interval, last check) | P2 |
| P5 | Three-way diff and per-field merge engine (pure) | P1, P2 |
| P6 | Apply approved changes with a staging area and recovery journal | P5 |
| P7 | Scheduler: supervisor, intervals, jitter, backoff, launch catch-up, notice | P4 |
| P8 | Spec sync tab: list and detail | P5, P6 |
| P9 | Spec sync tab: skip, apply, Check now, interval settings, token field | P7, P8 |

## Decisions log

- Ruling (2026-10-25): the design is approved. Import ships before sync. Provenance lives in a sidecar file in the collection folder (`openapi-source.yml`), keyed by request `uid`, because `Request` has no extension slot.
- Ruling (2026-10-25): the sidecar lives at `<collection>/.rocket/openapi-source.yml`, not at the collection root. A dot folder is already hidden from the collection loader and the Bruno export, so no loader change is needed (the contract feature uses `.rocket/contracts/` the same way). It never holds a token.
