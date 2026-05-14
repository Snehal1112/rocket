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
- camelCase rename only on IPC DTOs.
- Do not apply camelCase rename on persistence structs.

## OpenCollection Trigger

If scope includes yml/collection/environment/request/auth/variable-resolution/related tauri commands:

📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

## Verification

- cargo check
- focused cargo tests
- yarn tsc --noEmit for DTO wiring changes
