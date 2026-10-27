# OpenAPI sync UX, series 4: Plan Index

Spec: `docs/superpowers/specs/2026-10-28-openapi-sync-ux-3-design.md` (sections 6 and 7). Parity table: `docs/superpowers/specs/2026-10-28-bruno-sync-parity.md`. Series 3 (`../2026-10-28-openapi-sync-ux-3/`) runs first.

| Plan | File | Scope | Depends on |
|---|---|---|---|
| 01 | `plan-01-openapi-sync-ux-4.md` | Localhost over http: URL parser, fetcher first hop, copy | series 3 |
| 02 | `plan-02-openapi-sync-ux-4.md` | Local file source: `SpecLocation::File`, reader, check, import | 01 |
| 03 | `plan-03-openapi-sync-ux-4.md` | Local file UI: import checkbox, Connect and Change link file mode, display, docs | 02 |

Run in order. Each plan has at most 3 tasks. Cargo with `-j4 -p <crate>`; frontend commands are `yarn tsc --noEmit`, `yarn check`, `yarn test <name>`.

## Series-wide rules

- Tasks touching collection files, requests or the listed crates start with `📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.`
- No panicking unwrap calls in production paths; camelCase serde on IPC DTOs only; new persisted fields use `#[serde(default)]`; shadcn/ui and lucide-react only; comments are short full sentences.
- Consent rules unchanged: a file or localhost link is approved on this machine before anything is read or fetched. A file is read only inside `check`, Connect preview/confirm, Change link preview/confirm and the import command.
- Fixed sentences only over IPC; never a file path or file content in an error. Add new sentences to `KNOWN` in `syncErrors.ts`.
- A file source never receives a token. Token commands refuse it with the existing Github-only sentence.
- Older builds must still read every source that is not a File source (`version` stays 1 for them).
