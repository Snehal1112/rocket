# OpenAPI sync UX, series 2: Plan Index

Spec: `docs/superpowers/specs/2026-10-27-openapi-sync-ux-design.md` (decision "The work is split into two plan series"). Series 1 (`../2026-10-27-openapi-sync-ux/`, plans 01-04) is done. Series 2 holds the stored spec copy and View spec, Reset with backups and Restore, and Connect with matching.

| Plan | File | Scope | Depends on |
|---|---|---|---|
| 01 | `plan-01-openapi-sync-ux-2.md` | Backups file, `OWNED_STATE_FILES`, applied spec copy (import, finalized apply, self-heal) | series 1 |
| 02 | `plan-02-openapi-sync-ux-2.md` | View spec: `ParsedSpec` read accessor, `openapi_sync_spec_view`, viewer page | 01 |
| 03 | `plan-03-openapi-sync-ux-2.md` | Reset, Restore, Discard backend and the `backups` list in `openapi_sync_changes` | 01 |
| 04 | `plan-04-openapi-sync-ux-2.md` | Reset UI in Your changes | 03 |
| 05 | `plan-05-openapi-sync-ux-2.md` | Connect backend: matching, preview, confirm | 01 |
| 06 | `plan-06-openapi-sync-ux-2.md` | Connect UI, docs, roadmap | 05 |

Run in order. Each plan has at most 3 tasks. Plans 02, 03 and 05 only need plan 01 but run in this order to keep one implementer per worktree.

## Series-wide rules

- Tasks touching collection files, requests or the listed crates start with `📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.`
- Cargo with `-j4 -p <crate>`; no `unwrap()` in production paths; camelCase serde on IPC DTOs only; new persisted fields use `#[serde(default)]`; shadcn/ui and lucide only; narrow Zustand selectors; Bun is not used here, the project commands are `yarn tsc --noEmit`, `yarn check`, `yarn test <name>`.
- Consent rules unchanged: apply, skip, reset, restore and discard never contact the network. Connect grants consent only for the link the user typed.
- No raw backend error text or spec content in IPC messages; fixed sentences only.
- All writes go through the existing file transaction and the shared collection lock (`collection_lock(root)`).
