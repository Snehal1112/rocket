# OpenAPI sync UX, series 5: Bruno parity gaps, Plan Index

Spec: `docs/superpowers/specs/2026-10-28-openapi-sync-ux-3-design.md` (section 8). Parity table: `docs/superpowers/specs/2026-10-28-bruno-sync-parity.md` (rows marked **Gap**). Series 3 runs first. Series 4 and series 5 are independent of each other; run 5 before 4 if you want parity features before local files.

| Plan | File | Scope | Depends on |
|---|---|---|---|
| 01 | `plan-01-openapi-sync-ux-5.md` | View spec detail: body schema, response schemas, security, servers | series 2 |
| 02 | `plan-02-openapi-sync-ux-5.md` | Delete a request added by you, delete an edited request the spec removed | series 3 plan 02 |
| 03 | `plan-03-openapi-sync-ux-5.md` | Merge parity: urlencoded bodies, Reset scope, documented limits | series 2 |
| 04 | `plan-04-openapi-sync-ux-5.md` | Review parity: Keep my values switch, conflict count and filter, confirm summary | series 1 |
| 05 | `plan-05-openapi-sync-ux-5.md` | Failure banners with actions on Overview and Spec updates, re-check after enabling checks | series 3 plans 03 and 05 |

Each plan has at most 3 tasks. Cargo with `-j4 -p <crate>`; frontend commands are `yarn tsc --noEmit`, `yarn check`, `yarn test <name>`.

## Series-wide rules

- Tasks touching collection files, requests or the listed crates start with `📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.`
- No panicking unwrap calls in production paths; camelCase serde on IPC DTOs only; new persisted fields use `#[serde(default)]`; shadcn/ui and lucide-react only; comments are short full sentences.
- Fixed sentences only over IPC; spec text renders as plain text; nothing here contacts the network except `check`.
- Deleting a request goes through the file transaction and `collection_lock(root)`, re-checks the file's uid before removing it, and is never run during an apply.
- Bruno parity: a difference from the parity table needs a `Ruling:` line in the ledger.
- Fixtures: `world()` and `check_with` in `crates/rocket-app/src/openapi_sync_apply_tests.rs`; `crates/rocket-import/src/openapi/sync/` tests for merge; frontend `__tests__/fixtures.ts`.
