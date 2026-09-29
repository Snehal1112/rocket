# Flow Wire Script Editor — Plan Index

**Spec:** `docs/superpowers/specs/2026-09-29-flow-wire-script-editor-design.md`
**Branch:** `worktree-flow-phase2-branching`
**Execution:** subagent-driven, one plan at a time, in order.

| # | Plan | Tasks | Depends on |
|---|---|---|---|
| 01 | [Backend script rule and logs](2026-09-29-flow-wire-script-plan-01-backend.md) | 1. Expression-or-return script wrapper with coercion · 2. Report script console output on each step | — |
| 02 | [Frontend dialog, editing and logs](2026-09-29-flow-wire-script-plan-02-frontend.md) | 1. Push step logs to the Console · 2. Wire script dialog with Monaco · 3. Reopen a wire on double-click | Plan 01 (the `logs` field) |

Each plan has at most 3 tasks.
