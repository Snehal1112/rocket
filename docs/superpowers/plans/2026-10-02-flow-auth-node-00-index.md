# Flow Auth Node — Plan Index

Spec: `docs/superpowers/specs/2026-10-02-flow-auth-node-design.md`

Run the plans **in order**. Each plan has at most 3 tasks, ends with a green
build, and names the next plan at its top and bottom. Start a fresh Claude
session per plan and say: *"Execute `docs/superpowers/plans/<file>` with
superpowers:subagent-driven-development."*

| # | Plan file | Scope | Model |
|---|---|---|---|
| 1 | `2026-10-02-flow-auth-node-01-domain.md` | `rocket-flow` node + validation, IPC DTO, TS type | Sonnet |
| 2 | `2026-10-02-flow-auth-node-02-credentials.md` | backend credential resolution + token fetcher | Opus |
| 3 | `2026-10-02-flow-auth-node-03-executor.md` | Auth node execution, inherit substitution, `auth` wire | Opus |
| 4 | `2026-10-02-flow-auth-node-04-ipc.md` | `authTokens` on `run_flow`, startup wiring, `runFlow` signature | Sonnet |
| 5 | `2026-10-02-flow-auth-node-05-frontend-node.md` | store, Auth node, palette, editor, `auth` handle | Sonnet |
| 6 | `2026-10-02-flow-auth-node-06-preflight.md` | pre-run authentication prompt/refresh | Sonnet |
| 7 | `2026-10-02-flow-auth-node-07-verify.md` | end-to-end tests, security review, docs, final verification | Opus |

Related but separate: the uncommitted `inherit` fix in
`flow_execution_service.rs`, `runner_sequence.rs`, `execute-request.ts` and its
test. Commit it on its own (`fix: preserve inherit auth in flow and runner
sends`) before starting Plan 1, so Plan 1 starts from a clean tree.

Out of scope (follow-ups): Collection Runner support, "use collection auth" as
a node source, mid-run token refresh.
