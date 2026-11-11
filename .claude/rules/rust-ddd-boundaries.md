# Rust DDD Boundaries

## Required Reads

1. [../../CLAUDE.md](../../CLAUDE.md)
2. Target crate guidance in [../../crates](../../crates)
3. For yml/collection/environment/request/auth/variable-resolution scope, read [../../docs/superpowers/specs/opencollection-spec-reference.md](../../docs/superpowers/specs/opencollection-spec-reference.md).

## Placement Rules

- Domain crates: logic and traits only.
- rocket-infra: concrete I/O only.
- rocket-app: trait-first, no infra concrete coupling.
- No git CLI shell-outs.

## Error and Serde Rules

- Avoid unwrap in production paths.
- Keep persistence backward compatibility (optional fields/defaults).
- Keep persistence forward compatible: an older build must start and work on data a newer build wrote. A new variant in a stored enum or a new entry kind in a stored list must not stop startup or a load.
  - A log (jsonl) keeps an unknown event as raw JSON (`AuditEventKind::Unknown`) and never rewrites it.
  - A list file (yml) reads through `rocket-infra` `TolerantList`: unreadable entries are hidden and written back unchanged.
  - Never turn an unknown value into a permissive default (for example an unknown proxy mode into `system`). Fail closed instead.
- camelCase rename only on IPC DTOs.
- Do not apply camelCase rename on persistence structs.

## OpenCollection Trigger

If scope includes yml/collection/environment/request/auth/variable-resolution/related tauri commands:

📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

## Verification

- cargo check
- focused cargo tests
- bun run tsc --noEmit for DTO wiring changes
