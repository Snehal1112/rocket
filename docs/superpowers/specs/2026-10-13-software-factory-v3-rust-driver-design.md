# Software Factory v3: a Rust driver instead of an LLM orchestrator

Status: draft for review, 2026-10-13. Supersedes `2026-10-13-software-factory-v2-design.md` (kept for its gap list, which this spec maps below). Builds on v1 in `.claude/software-factory.md`.

## Why

The v1 factory works but is costly. Job 001 used about 12.75M raw tokens (about 2.2M weighted). 53.6 percent of that was the orchestrator, which is an LLM session re-reading a context of up to 155K on every turn to do mechanical work: run `ctl`, write diffs, dispatch agents, merge. A further 7 percent was commit helper agents, and the full review set (spec writer, four reviewers, approver) ran on a diff of 23 KB.

The cheapest effective design moves everything mechanical into a Rust program and keeps the LLM only where judgment is needed:

- design and plan with the user: `superpowers:brainstorming` and `superpowers:writing-plans` in the interactive session (the user wanted exactly this);
- build: one headless builder agent per plan;
- review: one headless reviewer, after deterministic checks already pass;
- everything else (state, checks, diffs, merge, sync, series, budget, token logging): Rust, zero LLM tokens.

## Decisions

1. **Rust, `git2`, own workspace** at `tools/factory/` (not in the root workspace, own target dir, like `tools/factory-server/`). `git2` follows Rocket's rule against shelling out to git. The builder agent still uses git itself inside its worktree. No Python and no shell scripts in the new design.
2. **The driver is a CLI, `factory`.** The interactive `/factory` command becomes thin: it runs the two superpowers skills with the user, saves the plans, then runs `factory new` and `factory run` as a background Bash command and goes idle. The session is not polled, so it costs almost nothing while the driver works.
3. **Headless agents through `claude -p`** with `--output-format stream-json`. The driver reads the stream, so it gets exact token usage, the session id for `--resume`, and tool events for the dashboard, with no hook. Which `claude` flags exist is probed in plan 03 task 1 before anything depends on them.
4. **Checks before review.** The driver runs the checks itself (`cargo check -j4`, targeted `cargo test -j4 -p <crate>`, `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern>`, and the factory crate checks), chosen from the changed paths by `.claude/factory/config.toml`. A failure resumes the builder with the trimmed failure output. No reviewer tokens are spent on a red build. Results go to `checks.md`, which is the evidence for the merge rule.
5. **One reviewer, Sonnet.** `factory-code-reviewer` covers correctness, DDD placement, serde rules, tests and the hard rules (shadcn, lucide, editors, Zustand, no unwrap, no git CLI). No Opus reviewer, no approver agent.
6. **Risky diffs stop for the human.** If the diff touches the collection trust gate, auth, secrets or `{{process.env}}`, persistence formats, the IPC surface (`src-tauri/`), `DomainEvent`, `Cargo.toml`, `Cargo.lock`, `package.json`, `tauri.conf.json`, CSP, or the harness hooks, the job goes to `needs-human` with a summary after the reviewer runs. The user reviews and runs `factory approve <id>`. The path list is in `config.toml` (`risk_paths`). A missing or broken config makes every diff risky.
7. **Auto-merge rule:** checks have no failure, the latest reviewer verdict is exactly `VERDICT: APPROVE`, the diff class is not risky, the diff is within the size cap, and the budget is not at stop. Otherwise a fix round (at most 2) or `needs-human`.
8. **Models:** builder and reviewer default to Sonnet. `model = "opus"` per job (`factory new --model opus`) or in config. This reverses the 2026-10-13 choice of an Opus builder, by the user's cost request.
9. **Budget is enforced, not advisory.** The driver tallies weighted tokens (input + cache creation + output + cache read / 10) per job, warns at 80 percent, and kills the agent and parks the job at 100 percent. Defaults: 3,000,000 weighted per job, 4,000 changed lines diff cap, 2 fix rounds, 40 builder turns per 10 minutes without events as the stall limit.
10. **Merge reviewed sha.** The driver merges the recorded head sha, checks that the branch tip equals it, runs a preflight on main (on `main`, clean, not mid-merge), makes a no-ff merge commit with a fixed message `chore(factory): merge job NNN <slug>`, and verifies the second parent. If main moved it merges main into the job branch inside the worktree, re-runs checks and the reviewer once on the sync diff (at most 2 syncs), no history rewrite. A conflict goes to `needs-human`.
11. **Series:** plans are files with a final `Next plan: <path|none>` line. `factory run` builds plan 1, merges, then starts the next plan, one at a time, stopping on `needs-human`.
12. **Board and events stay JSON and server-compatible.** `board.json` keeps the v1 stage names (`spec`, `build`, `review`, `approve`, `merged`, `needs-human`) and field names, so `tools/factory-server/` keeps working. The driver writes `events.jsonl` itself with the job id in every event, which removes the cwd-based attribution gap and the Python hook.
13. **Ledger writes only by the driver.** Agents never write the ledger or board.
14. **Docs under `.claude/`**, project `CLAUDE.md` stays small.

## Gap map (v2 spec letters)

| v2 group | Fate in v3 |
|---|---|
| A (orchestrator and ctl robustness) | Solved by design: reviewed sha merge, `prev_head`, resume check, stage machine with guarded transitions, ids from board plus directories plus branches, atomic writes. Plans 01, 02, 04, 05. |
| B (evidence and prompts) | `checks.md` written by the driver, builder prompt rewritten, no builder ledger writes, commit fallback is the driver. Plans 04, 06. |
| C (main moved, lifecycle, locking) | Plan 02 (sync, preflight, cleanup) and plan 05 (lock, lease, cancel, cleanup, merge failures). |
| D (series) | Plan 05 and plan 06. No nested subagents are needed. |
| E (dashboard, hook) | The Python hook is removed (driver emits events). Server and page work in plan 07. `/verify-rocket`, browser check, v1 minors in plan 08. |
| F (token efficiency) | The whole design. Plan 03 (usage, budget kill, stall), plan 04 (checks before review), plan 05 (diff cap). |

## Crate layout

`tools/factory/Cargo.toml` is a workspace. All crates avoid `unwrap` in production paths, use `-j4`, never `--workspace` for tests (run `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p <crate>`).

| Crate | Kind | Responsibility | Plan |
|---|---|---|---|
| `factory-core` | lib | Board and job model (serde, compatible with the v1 `board.json`), ids and slugs, ledger, stage machine, `config.toml` (`Config`), risk classification (`DiffClass`), verdict parsing (`Verdict`), budget math (`BudgetState`), atomic file writes | 01 |
| `factory-git` | lib | `git2` wrapper: `Repo`, worktree create and remove, diff to files (`DiffFiles`), ancestry and merged checks, preflight, no-ff merge, sync main into branch | 02 |
| `factory-agent` | lib | `AgentRunner` trait, `ClaudeRunner` (headless `claude -p`), `FakeRunner` for tests, stream-json parser, usage tally, event sink (`events.jsonl`), budget kill, stall detector | 03 |
| `factory-checks` | lib | Path to check mapping, check execution with enforced `-j4`, output trimming, `checks.md` writer, `ChecksReport` | 04 |
| `factory-engine` | lib | The run loop: build, checks, review, decide, fix rounds, merge, sync, series | 04 and 05 |
| `factory-cli` | bin `factory` | clap commands over the engine: `new`, `run`, `status`, `show`, `approve`, `rework`, `cancel`, `cleanup`, `tokens` | 05 |

## Interfaces between plans

Later plans may use only what earlier rows produce.

| Plan | Produces |
|---|---|
| 01 | `factory_core::{Board, Job, Stage, ReviewState, Ledger, Config, DiffClass, classify_paths, Verdict, parse_verdict, BudgetState, weighted_tokens, budget_state, write_atomic, job_dir_name, next_job_id}`. `Config` loads `.claude/factory/config.toml` (keys `risk_paths`, `diff_cap_lines`, `budget_weighted`, `fix_rounds`, `stall_minutes`, `builder_model`, `reviewer_model`, `[[checks]]`). Board JSON round-trips the real job 001 board. |
| 02 | `factory_git::{Repo, Worktree, DiffFiles, MergeOutcome, Preflight}`. `Repo::open`, `create_worktree`, `remove_worktree`, `diff_to_files`, `changed_paths`, `is_ancestor`, `is_merged`, `tip`, `preflight_main`, `merge_no_ff`, `merge_main_into`. |
| 03 | `factory_agent::{AgentRunner, AgentSpec, AgentRun, Usage, ClaudeRunner, FakeRunner, EventSink, StopReason}`. `AgentRunner::run(spec) -> AgentRun` with `session_id`, `usage`, `result_text`, `stop_reason`. A probe report of the real `claude` flags in `.claude/software-factory.md`. |
| 04 | `factory_checks::{CheckPlan, ChecksReport, plan_checks, run_checks, write_checks_md}`. `factory_engine::{Engine, build_stage, checks_stage, review_stage, Decision, decide}`. |
| 05 | `factory_engine::{merge_stage, sync_stage, run_job, run_series}`. Series parsing (`Next plan:`). Lock and lease. `factory` binary with all subcommands. |
| 06 | Rewritten agent files, thin `/factory` command, hook removed from `settings.json`, Python `factory-ctl` and its tests deleted after parity. |
| 07 | Server shows tokens, stage history, series chip, copy chips. |
| 08 | No interfaces. |

## Plan series

Folder `docs/superpowers/plans/2026-10-13-software-factory-v3/`. Eight plans, at most 3 tasks each, built in order by a `factory-builder` subagent running `superpowers:executing-plans` (skipping its final whole-branch review and ledger scripts). Each ends with a `Next plan:` line.

| Plan | File | Delivers |
|---|---|---|
| 01 | `plan-01-core.md` | Workspace, board model, stage machine, ledger, config, risk, verdict, budget |
| 02 | `plan-02-git.md` | The `git2` layer |
| 03 | `plan-03-agent-runner.md` | Probe of `claude` flags, headless runner, usage, events, budget kill, stall |
| 04 | `plan-04-checks-and-engine.md` | Checks runner and the build, checks, review, decide loop |
| 05 | `plan-05-merge-series-cli.md` | Merge, sync, series, lock, lease, cancel, cleanup, the CLI |
| 06 | `plan-06-agents-command-cutover.md` | Agent prompts, thin `/factory`, hook removal, Python removal, docs |
| 07 | `plan-07-dashboard.md` | Server and page changes |
| 08 | `plan-08-harness-and-verification.md` | `/verify-rocket` fix, v1 minors, browser check, end-to-end dry run, token measurement |

## Out of scope

- Authenticated approve and rework actions on the dashboard (copy buttons only).
- Money cost in currency. Token counts and the weighted budget only.
- Parallel jobs.
- Pushing to a remote. The factory never pushes.
- Rewriting `tools/factory-server/` in another language.

## Success check

After plan 08, a lean job on a small feature: the user brainstorms and plans in the session (the only interactive tokens), `factory run` builds in a worktree, runs the checks, runs one reviewer, merges the reviewed sha, and starts plan 2, with the interactive session idle meanwhile. The dashboard shows live stages and per-job tokens. The logged weighted total for a comparable job is well below the 2.2M weighted of job 001. This is checked by the token measurement in plan 08, not assumed.

## Risks

- The `claude` headless flags (`--agent`, `--resume`, permission mode, tool allow list) must be confirmed by the probe in plan 03. If `--agent` is unavailable the driver passes the agent prompt as the system or user prompt instead.
- The driver cannot ask the user questions during a run, so the plan must be complete. The builder reports blockers in its result and the job goes to `needs-human`.
- A fake `claude` runner is used in tests, so real-agent behavior is first exercised in the plan 08 dry run.
