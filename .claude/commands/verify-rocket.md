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

3. For rust or all, check only the packages that changed. From repo root:
- List changed paths: `git diff --name-only main -- crates src-tauri` plus `git status --porcelain -- crates src-tauri`.
- Map `crates/<name>/...` to package `<name>`, and `src-tauri/...` to package `rocket`. Drop duplicates.
- If no crate changed, report "no Rust crate changed" and run no cargo command.
- For each changed Rocket package, one at a time, run:
  - cargo check -j4 -p <crate>
  - cargo test -j4 -p <crate> --no-run
- Never run a workspace-wide build or test, and never leave out `-j4`. The harness guard denies both.

4. For frontend or all, run from repo root:
- yarn tsc --noEmit
- yarn check

5. Report output in a strict table, one row per command that ran:

| Check | Status | Notes |
|---|---|---|
| cargo check -j4 -p <crate> | pass/fail | error count or key failure line |
| cargo test -j4 -p <crate> --no-run | pass/fail | error count or key failure line |
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
