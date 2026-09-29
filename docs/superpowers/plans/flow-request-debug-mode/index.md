# Flow Request Debug Mode — Plan Index

**Spec:** `docs/superpowers/specs/2026-09-29-flow-request-debug-mode-design.md`
**Branch:** `worktree-flow-phase2-branching`
**Execution:** subagent-driven, one plan at a time, in order.

| # | Plan | Tasks | Depends on |
|---|---|---|---|
| 01 | [Backend](2026-09-29-flow-debug-plan-01-backend.md) | 1. Debug flag and redaction helpers · 2. Capture the sent request and build the debug record · 3. Attach the debug record to Flow steps | — |
| 02 | [Frontend](2026-09-29-flow-debug-plan-02-frontend.md) | 1. Debug rows in the Console · 2. Debug mode toggle, badge and panel switch | Plan 01 (`debug` flag, `debugRequest`) |

Each plan has at most 3 tasks. Related but out of scope: #43, #44.
