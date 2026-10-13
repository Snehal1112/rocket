# Software Factory v2 plan series

Spec: `docs/superpowers/specs/2026-10-13-software-factory-v2-design.md`

Run the plans one at a time, in order. Each plan has at most 3 tasks, is executed by a `factory-builder` subagent through `superpowers:executing-plans`, and ends with a `Next plan:` line. Do not start a plan before the previous one is merged. Plans 01 to 06 edit the same files (`factory-ctl`, `factory.md`, `run.sh`), so the later plans re-read each file before editing.

| Plan | File | Delivers | Gaps | Status |
|---|---|---|---|---|
| 01 | `plan-01-orchestrator-robustness.md` | Merge the reviewed sha, durable prev head, resume checks, reviewer skip rule, id and stage safety, `ctl show` | A1-A10 | not started |
| 02 | `plan-02-evidence-and-prompts.md` | `checks.md` evidence, approver context, builder and spec-writer prompt fixes | B1-B10 | not started |
| 03 | `plan-03-main-moved-and-lifecycle.md` | Base check, `sync-base`, lock and lease, cancel and cleanup, merge failure handling | C1-C7 | not started |
| 04 | `plan-04-multi-plan-series.md` | Nesting probe, series in ctl and spec writer, orchestrator chaining, `executing-plans` in the builder | D1-D5 | not started |
| 05 | `plan-05-dashboard-server-hook.md` | Hook redaction, stage history, attribution, log rotation, series chip, page fixes | E1-E9 | not started |
| 06 | `plan-06-harness-and-cleanup.md` | `/verify-rocket` and `CLAUDE.md` fixes, v1 minors, browser check, doc sync | E10-E12 | not started |
| 07 | `plan-07-lean-mode.md` | Lean default mode, one combined reviewer, risk escalation, `--full` opt-in | F1, F4, F5, F7 | not started |
| 08 | `plan-08-token-efficiency.md` | Diff and context hygiene, commit ceremony and helper cap, token logging and budget | F1-F9 | not started |

## Known soft spots (checked in review, accepted)

- Plan 01 picks the last reviewer verdict by file modification time. A round number in the file name would be stricter. Fix it in plan 01 task 2 if a reviewer objects.
- Plan 04's probe depends on `claude -p --output-format stream-json` exposing subagent messages. If not, it records `BASIS=self-report`.
- Plan 05 assumes `series.index` is 1-based (matches plan 04). One constant, `SERIES_INDEX_BASE`, flips it.
- Plan 06's light-theme screenshot flag is a guess. The script prints a note when it fails to switch.

Start with plan 01.
