# Software Factory for Rocket

Status: v3, a Rust driver. Design: `docs/superpowers/specs/2026-10-13-software-factory-v3-rust-driver-design.md`. Plans: `docs/superpowers/plans/2026-10-13-software-factory-v3/`. v1 (LLM orchestrator, Python helper) and the v2 plan series are superseded.

The Rust driver does everything mechanical: state, checks, diffs, merge, sync, series, budget and events. The LLM is used only where judgment is needed: design and planning with the user, one builder per plan, one reviewer per diff.

## Pipeline

```
/factory <feature>  (interactive, with the user)
  superpowers:brainstorming -> spec.md   (staging folder .superpowers/sdd/draft-<slug>/)
  superpowers:writing-plans -> plan-01-<slug>.md ... (at most 3 tasks each, Next plan: line)
  factory new <slug> '<feature>' --plans <staging folder>, then factory run <id> --series  (background Bash, session idle)

factory run (no LLM tokens):
  worktree -> builder (claude -p, Sonnet) -> checks -> reviewer (claude -p, Sonnet)
                 ^                              |  red: resume builder with trimmed output
                 |  CHANGES, at most 2 rounds   |
                 +------------------------------+
  APPROVE + no risk + under size cap + budget ok -> merge reviewed sha to local main -> next plan
  otherwise                                      -> needs-human (factory approve | factory rework | factory cancel)
```

## Job model

- One job is one feature. It holds a series of plans, each of at most 3 tasks, run one after another, never in parallel.
- Job id `NNN`, branch `factory/NNN-<slug>`, worktree `.claude/worktrees/factory-NNN-<slug>`. The driver creates the worktree with `git2` and starts the builder inside it.
- Job directory `.superpowers/sdd/YYYY-MM-DD-NNN-<slug>/` (gitignored): `spec.md`, `plan.md` (the current plan), `progress.md` (the ledger), `checks.md`, `review-*.diff`, review replies.
- `.superpowers/factory/board.json`: stage per job. Stages `spec`, `build`, `review`, `approve`, `merged`, `needs-human`. Same names and fields as v1, so the dashboard keeps working.
- `.superpowers/factory/events.jsonl`: events written by the driver, each with the job id.
- Only the driver writes the ledger, board and events. Agents and the `/factory` command never do.
- Config: `.claude/factory/config.toml` (`risk_paths`, `diff_cap_lines`, `budget_weighted`, `fix_rounds`, `stall_minutes`, `builder_model`, `reviewer_model`, `[[checks]]`).

## Modes

- Default: builder and reviewer run on Sonnet.
- `/factory <feature> --opus`: this job uses Opus (`factory new --model opus`). The model is never in the agent files, the driver passes it.

## Agents (`.claude/agents/`)

| Agent | Tools | Duties |
|---|---|---|
| `factory-builder` | all | Runs headless in the worktree. `superpowers:executing-plans` with an inline fallback (no final review, ledger scripts or progress file), TDD, systematic debugging on a failing check, verification before finishing. One commit per task through `dev-workflow-skills:1-git-commit`, git commands inline, no amend. At most 2 helper agents per task, none for commits. Reports a blocker as `BLOCKED:` in the result text. |
| `factory-code-reviewer` | Read, Grep, Glob | One combined review: correctness, DDD placement, serde, error handling, persistence compatibility, tests, and the hard rules (shadcn, lucide, SingleLineEditor vs Monaco, Zustand selectors, no unwrap, no git CLI). At most 60 lines, findings tagged `[blocker]` or `[minor]`, last line `VERDICT: APPROVE` or `VERDICT: CHANGES`. |

There are no spec-writer, security, ux, ui or approver agents. Risky diffs go to the human.

## Checks

The driver runs the checks itself, chosen from the changed paths by `config.toml`: `cargo check -j4`, targeted `cargo test -j4 -p <crate>`, `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern>`, and the factory crate checks. A failure resumes the builder with trimmed output. The reviewer never sees a red build. Results go to `checks.md`.

## Budget

Weighted tokens per job: input + cache creation + output + cache read / 10. Defaults: 3,000,000 per job, warn at 80 percent, kill the agent and park the job at 100 percent. Diff cap 4,000 changed lines, 2 fix rounds, a stall limit without events.

## Merge rules

Auto-merge needs all of: no check failure, the latest reviewer verdict is exactly `VERDICT: APPROVE`, the diff is not risky, the diff is within the size cap, the budget is not at stop. Otherwise a fix round (at most 2) or `needs-human`.

Risky means the diff touches any `risk_paths` entry: the collection trust gate, auth, secrets or `{{process.env}}`, persistence formats, `src-tauri/`, `DomainEvent`, `Cargo.toml`, `Cargo.lock`, `package.json`, `tauri.conf.json`, CSP or the harness hooks. A missing or broken config makes every diff risky.

The driver merges the reviewed sha only: it checks the branch tip equals the recorded head, preflights main (on `main`, clean, not mid-merge), makes a no-ff merge commit `chore(factory): merge job NNN <slug>`, and verifies the second parent. If main moved it merges main into the job branch inside the worktree, re-runs checks and the reviewer on the sync diff (at most 2 syncs). A conflict goes to `needs-human`. It never pushes and never rewrites history.

## How to run

Build once: `cargo build -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli`. Binary: `tools/factory/target/debug/factory`.

| Command | Does |
|---|---|
| `/factory <feature> [--opus]` | Brainstorm and plan with the user, then `factory new` and `factory run <id> --series` in the background. |
| `/factory status` | `factory status`. |
| `/factory approve <id>` | Release a `needs-human` job to the merge step. |
| `/factory rework <id> <note>` | Send a `needs-human` job back to the builder with a one-line note. |
| `/factory cancel <id>` | Stop and park a job. |
| `/factory cleanup <id>` | Remove the worktree of a finished job. |

The command matches subcommands by exact shape, so feature text such as `next button for pagination` stays a feature. The session does not poll while the driver runs. The harness notifies it on exit.

`factory new` validates the plan chain, so the plans are drafted in a staging folder first. The driver cannot ask questions during a run, so plans must be complete. Harness rules the driver keeps: every cargo call has `-j4`, no `--workspace`, no `CARGO_TARGET_DIR`, pathspec commits only, one implementer per worktree.

## Dashboard

`tools/factory-server/` (own workspace, not shipped) reads `board.json` and `events.jsonl` and serves a local page. It never writes them, and the factory works the same when it is not running. Run it with `cargo run -j4 --manifest-path tools/factory-server/Cargo.toml -- --root <main checkout>`. Token display, stage history and series chips come with plan 07 of the v3 series.

## Token comparison

| | Raw tokens | Weighted tokens | Status |
|---|---|---|---|
| Job 001 (v1, Opus builder, orchestrator session, 6 agents) | 12.75M | 2.2M weighted | measured baseline |
| New design, comparable job | not stated | about 0.9M weighted | estimate |

The baseline: the orchestrator was 53.6 percent of the raw tokens and commit helper agents about 7 percent. The estimate is arithmetic, not a measurement: it removes those two shares and the five extra reviewers (about 39 percent of the baseline remains, 2.2M x 0.39), and it ignores the interactive brainstorming and planning tokens, which the user spends in their own session. Plan 08 measures a real job, and that number replaces the estimate.

## Test coverage of the removed v1 helper

The v1 Python helper and the event hook are deleted. Their behaviors are pinned by Rust tests:

- ids, slugs, board writes, stage moves: `factory-core` (`ids::tests`, `stage::tests`, `board::tests`, `fsutil::tests`).
- ledger lines and `Task N: complete` at merge: `factory-core` `ledger::tests`, `factory-engine` `tick::tests`.
- merged check and main root: `factory-git` `is_merged_*`, `open_reports_the_checkout_root_from_a_subdirectory`.
- review chips on the board: `factory-engine` `review_state_*` and `a_fix_round_resets_the_review_state`.
- events: `factory-agent` `sink_*` tests, and `tests/privacy.rs` for url query stripping, concurrent appenders and no tool output.
- job id on each event: `run_series_chains_plans_and_each_job_writes_its_own_job_id_into_events`.
- agent and command files, settings, cutover: `factory-cli` `tests/agent_lint.rs`.

## Open items

- The new driver has only run against a fake `claude` in tests. The first real run is the plan 08 dry run.
- `/verify-rocket` still conflicts with the harness guard. Plan 08 fixes it.
- The dashboard look was never checked in a browser. Plan 08 checks it.
