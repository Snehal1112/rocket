---
description: "Use when editing Rust crates in this repo. Enforces DDD trait-first boundaries, no I/O in domain crates, serde DTO vs persistence split, and OpenCollection pre-read requirements."
name: "Rocket Rust DDD Boundaries"
applyTo:
  - "crates/**/src/**/*.rs"
  - "crates/**/tests/**/*.rs"
---

# Rust DDD Boundaries

Use this for Rust crate edits.

## Required Reads

1. [CLAUDE.md](../../CLAUDE.md)
2. Target crate guidance in [crates](../../crates)
3. If task touches yml/collection/environment/request/auth/variable resolution, read [OpenCollection spec](../../docs/superpowers/specs/opencollection-spec-reference.md) first.

## Core Rules

- Domain crates: domain logic and traits only.
- rocket-infra: concrete I/O only.
- rocket-app: trait-first orchestration, no infra concrete coupling.
- Never shell out to git CLI.

## Error and Compatibility Rules

- Avoid unwrap in production paths.
- Preserve persistence compatibility with optional fields and serde defaults.

## Serde Boundary Rule

- camelCase rename only on IPC DTOs.
- No camelCase rename on persistence structs.

## OpenCollection Trigger

For affected tasks, include this first planning step:

📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

Trigger list: yml, collection/environment/request/auth, variable resolution/scope, related tauri commands.

## Verification Before Completion

- cargo check for changed crates
- focused cargo tests
- yarn tsc --noEmit when DTO wiring changes
