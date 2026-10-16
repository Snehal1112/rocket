# Factory accurate usage (sub-project A)

Part of a three-part factory improvement: A accurate usage, B failure details on `needs-human` cards, C cost report and estimate by job size. A comes first, because B and C and the later cost-reduction work depend on trustworthy numbers.

## Goal

Make the token numbers the factory records correct and detailed enough to break a job's cost down by run, task, round and resume, so that the cost report (sub-project C) and the cost-reduction work rest on measured data.

## Background

- `StreamState::usage()` in `tools/factory/crates/factory-agent/src/stream.rs` sums the last usage snapshot of each assistant message. Output counts in those snapshots are partial. In jobs 003 to 011 a whole builder run logs about 1K output tokens, which is implausible.
- The `result` event carries the run's final usage but is used only when no message usage was seen.
- `run_agent` in `tools/factory/crates/factory-engine/src/ports.rs` writes one `usage.jsonl` line per run with the role and four counters only. It cannot answer how much resumes, fix rounds or per-task start-up cost.
- The "1.5M tokens" seen per job is mostly raw cache reads, which the budget weights at one tenth. Weighted cost per plan in jobs 003 to 011 is 220K to 480K.

## Decisions

- Final usage per run is the field-wise maximum of the per-message sum and the `result` event usage. The maximum cannot regress when the result is missing or smaller.
- The live budget kill during a run keeps the per-message sum, because the `result` event only arrives at the end. The final tally on the board and in `usage.jsonl` uses the corrected numbers. The budget stop can therefore trigger slightly late inside one run.
- Old `usage.jsonl` lines are not rewritten. Lines without `usage_v` are treated as old, and `factory tokens` says "output under-counted" for a job that has them.
- No dollar amounts. Jobs run on the subscription login.
- Buckets, estimates and dashboard changes are out of scope. They belong to sub-project C.

## Design

### 1. Usage fix

In `StreamState`, add a method that returns the final usage after the run: `max` per field of the per-message sum and `result_usage`. `ClaudeRunner` uses it for `AgentRun.usage`. The live check against the budget still calls the per-message sum.

Before relying on it, confirm against one real `claude -p` stream that `result.usage` covers the whole run, including subagent turns. Record the finding in the ruling section of `.claude/software-factory.md`. If `result.usage` turns out to cover only the main thread, the maximum still holds and the note records the limit.

### 2. Per-run record

`run_agent` writes these extra fields to each `usage.jsonl` line. All are optional on read.

| Field | Source |
|---|---|
| `ts` | engine clock, unix seconds |
| `model` | `spec.model` |
| `turns` | `run.turns` |
| `resumed` | `spec.resume.is_some()` |
| `stop` | `run.stop_reason` |
| `task` | job `task_index` plus 1, builder runs of a task plan only |
| `round` | job `round` |
| `start_ctx` | cache creation plus cache read of the first assistant message |
| `usage_v` | constant `2` |

`start_ctx` needs the stream state to keep the first message's usage. It tests the model of a fixed start-up cost per agent.

### 3. Report

`factory tokens <id>` keeps the role totals and adds a per-run table with role, task, round, resumed, turns and weighted tokens. A job with any line that has no `usage_v` prints a note that its output counts are under-counted.

## Testing

- Stream merge unit tests: result usage larger than the snapshots, smaller, missing, and with subagent turns.
- `usage.jsonl` round trip for the old line shape (no new fields) and the new one.
- Runloop test: the new fields land in `usage.jsonl` for a builder task run and a resumed fix run.
- `factory tokens` render test for a mixed job (old and new lines).
- Checks: `cargo test -j4` for `factory-agent`, `factory-engine` and `factory-cli`, and `cargo check -j4`, run with `--manifest-path tools/factory/Cargo.toml`.

## Acceptance criteria

- A run's recorded output tokens equal the larger of the message sum and the result event value.
- Each new `usage.jsonl` line carries `ts`, `model`, `turns`, `resumed`, `stop`, `round` and `usage_v`, and `task` for builder runs of a task plan.
- Old lines still load, and `factory tokens` flags a job that has them.
- `factory tokens <id>` prints a per-run table.
- The live budget kill behaves as before.
- `.claude/software-factory.md` records the corrected measurement and the real-stream finding.
