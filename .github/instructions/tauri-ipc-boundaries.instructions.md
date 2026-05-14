---
description: "Use when editing src-tauri IPC and app wiring. Enforces command-layer boundaries, DTO mapping discipline, crate service orchestration via traits, and OpenCollection compliance checks for collection/environment IPC."
name: "Rocket Tauri IPC Boundaries"
applyTo:
  - "src-tauri/src/**/*.rs"
---

# Tauri IPC Boundaries

Use this instruction for Rust files in [src-tauri/src](../../src-tauri/src).

## Required Reads

1. [CLAUDE.md](../../CLAUDE.md)
2. Affected crate guidance in [crates](../../crates)
3. If task touches yml/collection/environment/request/auth/variable resolution, read [OpenCollection spec](../../docs/superpowers/specs/opencollection-spec-reference.md) first.

## Command-Layer Responsibilities

- Keep commands thin: validate, call service, map output/error.
- No domain business logic in command modules.
- Route through rocket-app traits/services.
- Keep concrete I/O in rocket-infra.

## DTO and Mapping Discipline

- Use dedicated IPC DTOs when transport contract differs.
- camelCase rename only on IPC DTOs.
- No camelCase rename on persistence structs.

## Error and Event Handling

- Avoid unwrap in production command paths.
- Map failures to stable IPC-facing errors.

## OpenCollection Trigger for IPC Work

If a Tauri command touches collection/environment data or persistence behavior, include this as the first plan step:

📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

## Verification Before Completion

- cargo check and focused crate checks
- targeted Rust tests when available
- yarn tsc --noEmit for DTO/frontend binding changes
