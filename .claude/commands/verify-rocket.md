---
description: Run Rocket verification checks after implementation with scope-aware commands and strict pass/fail reporting
argument-hint: "[scope: rust|frontend|all (default)]"
---

# Verify Rocket

Run verification after code changes.

Scope argument: $ARGUMENTS

- rust
- frontend
- all (default)

## Instructions

1. Resolve scope:
- Empty means all.
- Valid values: rust, frontend, all.

2. Run checks in parallel when possible.

3. For rust or all, run from repo root:
- cargo check
- cargo test --workspace --no-run

4. For frontend or all, run from repo root:
- yarn tsc --noEmit
- yarn check

5. Report output in a strict table:

| Check | Status | Notes |
|---|---|---|
| cargo check | pass/fail | error count or key failure line |
| cargo test --workspace --no-run | pass/fail | error count or key failure line |
| yarn tsc --noEmit | pass/fail | error count or key failure line |
| yarn check | pass/fail | error count or key failure line |

6. If any check fails:
- Include relevant raw error snippets.
- Do not claim success.

7. If all selected checks pass:
- State verification passed with one-line readiness note.

## Conventions

- Use commands from [../../CLAUDE.md](../../CLAUDE.md).
- Keep reporting factual and concise.
- Do not fix failures unless explicitly asked.
