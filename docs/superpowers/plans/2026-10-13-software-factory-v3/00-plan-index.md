# Software Factory v3 plan series (Rust driver)

Spec: `docs/superpowers/specs/2026-10-13-software-factory-v3-rust-driver-design.md`. This series supersedes `docs/superpowers/plans/2026-10-13-software-factory-v2/`, which is kept only as a reference for the gap list and for ported rules.

Run the plans one at a time, in order. Each has at most 3 tasks, is executed by a `factory-builder` subagent through `superpowers:executing-plans` (skipping its final whole-branch review and ledger scripts), and ends with a `Next plan:` line. Do not start a plan before the previous one is merged. Plans 01 to 05 build the Rust driver in `tools/factory/`. Plan 06 cuts the factory over to it and deletes the Python. Plans 07 and 08 finish the dashboard and verify.

| Plan | File | Delivers | Status |
|---|---|---|---|
| 01 | `plan-01-core.md` | Workspace, board model, stage machine, ledger, config, risk, verdict, budget | not started |
| 02 | `plan-02-git.md` | The `git2` layer: diffs to files, preflight, no-ff merge, worktrees, sync | not started |
| 03 | `plan-03-agent-runner.md` | Probe of `claude -p` flags, headless runner, usage, events, budget kill, stall | not started |
| 04 | `plan-04-checks-and-engine.md` | Checks runner and the build, checks, review, decide stages | not started |
| 05 | `plan-05-merge-series-cli.md` | Merge, sync, series, lock, lease, cancel, cleanup, the `factory` CLI | not started |
| 06 | `plan-06-agents-command-cutover.md` | Agent prompts, thin `/factory`, hook and Python removal, docs | not started |
| 07 | `plan-07-dashboard.md` | Server and page: tokens, stage history, series chip, copy chips | not started |
| 08 | `plan-08-harness-and-verification.md` | `/verify-rocket` fix, v1 minors, end-to-end test, token report, browser check | not started |

## Notes for whoever runs these

- The plans were written in parallel. Plans 01 to 03 are the authority for signatures. A reconciliation pass fixed the calls in plans 04 and 05, but plans 06 to 08 still carry assumed names, each with a Step 0 that verifies the real code first.
- Plan 03 task 1 runs about ten tiny real `claude -p` calls (Haiku, 0.05 USD cap each). They are the only real model calls in the series. Its result decides which `claude` flags the driver may use.
- Plan 06 edits `.claude/settings.json` (removes the hooks, adds two allow entries) and deletes the Python files after a parity table. Review it before running it.
- Many plans are long because they inline full Rust code and tests. The builder should follow them task by task and not re-derive designs.
- Start with plan 01.
