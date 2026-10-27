# OpenAPI sync UX, series 3: Plan Index

Spec: `docs/superpowers/specs/2026-10-28-openapi-sync-ux-3-design.md`. Series 1 and 2 are done. Series 4 (`../2026-10-28-openapi-sync-ux-4/`) adds localhost and local-file sources and runs after this series.

| Plan | File | Scope | Depends on |
|---|---|---|---|
| 01 | `plan-01-openapi-sync-ux-3.md` | Change the spec link: service preview/confirm, commands, dialog form | series 2 |
| 02 | `plan-02-openapi-sync-ux-3.md` | Values in Your changes, New item preview, Open request | series 2 |
| 03 | `plan-03-openapi-sync-ux-3.md` | Next steps on failures and empty states | 01 (dialog) |
| 04 | `plan-04-openapi-sync-ux-3.md` | Skipped list and Un-skip, import-dialog note | series 2 |
| 05 | `plan-05-openapi-sync-ux-3.md` | Connection settings dialog: status line, Check now, rate-limit hint, rename | 01, 03 |

Run in order. Each plan has at most 3 tasks. Cargo with `-j4 -p <crate>`; frontend commands are `yarn tsc --noEmit`, `yarn check`, `yarn test <name>` (this project uses Yarn).

## Series-wide rules

- Tasks touching collection files, requests or the listed crates start with `📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.`
- No panicking unwrap calls in production paths; camelCase serde on IPC DTOs only; new persisted fields use `#[serde(default)]`; shadcn/ui and lucide-react only; narrow Zustand selectors; comments are short full sentences.
- Consent rules unchanged: only `check`, Change link preview and Change link confirm contact the network. Skip, unskip, open request and the new read models never do.
- No raw backend error text or spec content in IPC messages; fixed sentences only, added to `KNOWN` in `syncErrors.ts`.
- All writes go through the file transaction and `collection_lock(root)`.
- Test fixtures: `world()` and `check_with` in `crates/rocket-app/src/openapi_sync_apply_tests.rs`; frontend `__tests__/fixtures.ts` (`makeStatus`, `makePlan`, `newItem`, `changedItem`).
- Bruno parity: before each plan starts, compare its behavior with `docs/superpowers/specs/2026-10-28-bruno-sync-parity.md`; a difference needs a `Ruling:` in the ledger.
