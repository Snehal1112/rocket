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
- `factory new --plans` makes one job per plan at once. Plan 1 starts at `build`. Plans 2 to N wait at `spec` and share a series id `S<n>` (`series.id` on the board). `spec.md` is copied into plan 1's job folder only, so builders of later plans are not pointed at it. `run --series` runs the next waiting job when the plan before it merges, and `run <id>` on a waiting job moves it to `build`. A series with one plan has no id. A series made before this change has no id either, and its next job is created after the previous one merges.
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

`tools/factory-server/` (own workspace, not shipped) reads `board.json` and `events.jsonl` and serves a local page. It never writes them, and the factory works the same when it is not running. Run it with `cargo run -j4 --manifest-path tools/factory-server/Cargo.toml -- --root <main checkout>`.

Data flow: `factory driver -> .superpowers/factory/{board.json,events.jsonl} -> factory-server -> dashboard.html`.

- Agents are matched to a job only by the `job_id` on their events. An event without `job_id` shows under "Agents without a job". There is no matching by cwd or stage.
- The card shows the job id, a `plan k of N` chip (hidden for a single plan), feature, branch, builder round, review chips, time in stage (hidden once merged or needs-human), time per stage from `stage_history` when the board has it, weighted tokens against the budget with a bar (text level `warn` at 80 percent, `stop` at 100), and copy chips for `factory approve <id>`, `factory rework <id> <note>` and `factory cancel <id>`.
- The server reads the board's four token counters and weights them like the driver (cache reads count one tenth). New fields that are malformed are dropped, never a board error. The budget defaults to 3,000,000 weighted tokens when the board has no `budget_weighted`.
- Only the last 4 MiB of the log is read at startup. Agents idle for a day are dropped (cap 500). The registry resets when the log is truncated, replaced or deleted. An agent silent for 10 minutes shows `stale`.
- The page patches only changed text. The status line is not a live region. One short announcement is made when the connection level changes. Bar widths are set through the CSSOM because the CSP forbids inline styles.

## Per-task builders

- The driver runs one fresh `factory-builder` agent per `### Task N:` heading of `plan.md`, one after another, each as its own `claude -p --agent factory-builder` run (`--max-turns 120`). A plan with no task headings keeps the old whole-plan builder.
- Each build call handles one task while the board field `task_index` is below the task count. The prompt names the plan, the spec, the base, the current tip, the earlier commit subjects and "Do only Task N". The agent makes exactly one commit. A task with no commit parks the job with `task N made no commit`.
- Ledger lines: `Ruling: per-task builders, <total> tasks` once, then `Task N: dispatched (BASE <sha>, <model>)` and `Task N: implementer done (<short sha>)` per task. `tick_plan` still writes `Task N: complete` after the merge.
- Board field `task_index` (number of finished, committed tasks, skipped when 0): a budget or stall kill in task 2 leaves task 1 committed, and a `rework` resumes at `task_index`. After the last task the driver records `head`, checks the diff cap and moves to review once. Fix rounds and `rework` then resume the last task's session and never restart the task loop. The reviewer diff `prev_head..head` is set once, after the last task.
- The dashboard reads the ledger marker `per-task builders` to show each task's true state (`review` after its implementer is done, `active` while dispatched, `pending` before). Jobs without the marker keep the stage-based display.
- The driver now passes `--agent factory-builder` for the whole-plan, task and fix builders. Before this it never did.
- Token model (modelled, not measured): a fresh agent for task k pays off when the context it would have inherited, P_k, exceeds `(9F + (n_k+9)S) / n_k` (F = fixed start of about 19K to 38K tokens, S = spec plus plan size, n_k = turns of the task). Estimated saving: about 11 percent on a 2-task run, about 17 percent on a 3-task run, slightly negative on very short plans. The main gain is accurate per-task state on the card.

## Token comparison

| | Raw tokens | Weighted tokens | Status |
|---|---|---|---|
| Job 001 (v1, Opus builder, orchestrator session, 6 agents) | 12.75M | 2.2M weighted | measured baseline |
| New design, comparable job | not stated | about 0.9M weighted | estimate |

The baseline: the orchestrator was 53.6 percent of the raw tokens and commit helper agents about 7 percent. The estimate is arithmetic, not a measurement: it removes those two shares and the five extra reviewers (about 39 percent of the baseline remains, 2.2M x 0.39), and it ignores the interactive brainstorming and planning tokens, which the user spends in their own session. Plan 08 measures a real job, and that number replaces the estimate.

## Token baseline

Measured on job 001 (v1, LLM orchestrator, full review set): about 12.75M raw tokens, about 2.17M weighted (input + cache creation + output + cache read / 10), orchestrator 53.6 percent of the total, commit helper agents about 7 percent, diff of 23 KB read by 5 reviewers.

`factory tokens <id>` prints the board totals, then raw and weighted totals per agent role and the percent of that weighted baseline. The per-role numbers come from `usage.jsonl` in the job folder, one line per agent run, written by the engine. Compare like with like:

| Measure | Job 001 (v1) | Lean job target |
|---|---|---|
| Weighted total for the whole job | about 2.17M | well below, aim under 1.1M for a comparable small job |
| Orchestrator share | 53.6 percent | not counted: the driver is Rust, the interactive session only plans |
| Reviewer runs per clean round | 5 | 1 |
| Commit helper agents | 2 or more per commit | 0 |

The plan-time session (brainstorming and writing plans) is interactive and is not in `factory tokens`. Note it by hand from the session's usage line when comparing totals. The target is a guideline taken from the spec's success check ("well below 2.2M weighted"), not a gate.

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

- Dry run of a two-plan feature through `/factory` has not been run yet. The user runs it, see "Dry run recipe" below.
- Real-agent behavior (the actual `claude -p` flags, resume, permission mode) is only proven by that dry run. The scenario tests use `FakeRunner`-style scripted runners.
- Light theme in a headless browser counts as checked only if the screenshot differs from the dark one. In the plan 08 run, headless Chrome stayed on the light scheme for all three shots, so the dark theme is still unchecked by eye. `factory-shots` names the shots dark and light by intent, not by result.
- `factory-shots` needs a fresh `cargo build` of the server and `google-chrome`, `chromium` or `firefox` on the PATH. It sends a `Host` header, because the server refuses a request without one.
- `smoke.sh` and `seed-demo.sh` were not reworked in plan 08.

## Rulings

- Ruling (2026-10-13): the dashboard was checked in a headless Google Chrome 155 with `factory-shots` at 1440 px and 400 px. Findings: five stage columns, summary strip, token bar with warn level, review chips with text, the needs-human card with its copy chips and the series chip all render. At 400 px the board scrolls inside its own container and the page does not scroll sideways. The dark and light themes were not told apart by the browser, so only the light look was seen. A human look in a real window is welcome but blocks nothing. The v1 minors are triaged in plan 08 of the v3 series: the Python ctl and hook minors were dropped with the deleted code.
- Ruling (2026-10-13): `factory-core` writes `stage_history` (`{stage, at}`, unix seconds) and `budget_weighted` to the board, because the dashboard reads both. The engine writes one `usage.jsonl` line per agent run in the job folder for `factory tokens`.

## Dry run recipe (run by the user)

Run this once after plans 01 to 08 are merged. It uses real agents, so it costs real tokens. Pick a small feature that needs two plans, for example "Add a per-request timeout setting and show it in the request tab".

1. In a fresh Claude Code session on `main` with a clean tree, start the dashboard: `cargo run -j4 --manifest-path tools/factory-server/Cargo.toml -- --root "$(pwd)" --port 8765`, and open http://127.0.0.1:8765/.
2. Run `/factory <the feature>`. In the session, brainstorm and approve a spec, then a two-plan series (plans end with `Next plan:` lines). Note the session usage line, this is the interactive cost.
3. When the command starts `factory run` in the background, leave the session idle and watch the dashboard. Expect, in order:
   - A job card in build with its worktree and the token counter rising.
   - A `checks.md` in the job folder (`ls .superpowers/sdd/*/checks.md`) with every check passing, or the builder resumed once after a failure.
   - One reviewer run, then the stage moves to approve and merged. A diff that touches a risk path (auth, `src-tauri/`, `Cargo.toml` and so on) parks in needs-human instead. Run `factory approve <id>` after reading the diff.
   - The merge commit has the message `chore(factory): merge job NNN <slug>` and its second parent is the reviewed sha (`git log --merges -1 --format=%P`).
   - Plan 2 starts by itself as the next job and merges the same way.
4. Read the cost: `factory tokens 001` and `factory tokens 002`. Each prints runs, raw and weighted tokens per agent role, and the percent of the job 001 weighted baseline. Add the two jobs and the interactive session to compare with the 2.17M weighted of job 001. Check that the reviewer role shows one run per clean round, and that no commit helper agents appear.
5. Pass criteria (the spec's success check): the interactive session was the only place the user spent tokens by hand and sat idle while the driver worked, the driver built in a worktree, ran the checks, ran one reviewer, merged the reviewed sha and started plan 2, the dashboard showed live stages and per-job tokens, no stage needed manual repair, `git status` is clean in the main checkout, and the logged weighted total is well below 2.17M (aim under 1.1M for a comparable small feature).
6. Clean up with `factory cleanup <id>` for each job and confirm `git branch --list 'factory/*'` shows nothing left.

If any step needs a manual action, note the step and add a follow-up row to `.claude/roadmap.md`.
