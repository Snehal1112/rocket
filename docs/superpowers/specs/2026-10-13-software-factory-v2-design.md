# Software Factory v2: closing the gaps

Status: draft for review, 2026-10-13. Builds on `.claude/software-factory.md` (v1, merged) and the plan series `docs/superpowers/plans/2026-10-13-software-factory/`.

## Why

v1 works end to end. Job 001 (issue 21, mTLS test) went from spec to merge with no review rounds. But a seven-part investigation (real run evidence, orchestrator flow, agent prompts, server and hook, multi-plan support, harness conflicts) found gaps. The real run needed three manual interventions and scored about 6 out of 10: usable, not autonomous.

This spec lists every confirmed gap, groups them into eight plans of at most three tasks, and fixes the interfaces between plans so that each plan can be written and built on its own.

## Evidence rules

- A gap is listed only if an investigator confirmed it with file and line or log evidence.
- Candidates that were disproved are listed at the end so nobody re-opens them.
- Tags: `[run]` seen in the real job 001 run, `[read]` found by reading code, `[ledger]` a deferred minor from the v1 ledgers.

## Confirmed gaps

### A. Orchestrator and ctl robustness (plan 01)

| # | Gap | Source |
|---|---|---|
| A1 | The merge step runs `git merge <branch>` but every check uses the recorded head sha. Commits past the reviewed head merge unreviewed. | read, `factory.md` merge step 4 |
| A2 | A builder that crashes on its first run leaves `builder_id` and `worktree` null. The id is recorded only after a foreground dispatch returns. A second `/factory next` can start a second builder or message a live one. | read |
| A3 | The previous head for a re-review diff is read from the ledger ("second-to-last Head round line"). `set-stage --head` and `ctl ledger` are two calls, so a crash between them yields the wrong range. | read |
| A4 | A resume with no new commits (head equals previous head) is not detected. Reviewers see an empty diff and approve. | read |
| A5 | UX and UI reviewers are skipped when the re-review diff has no `src/` change, even if their last verdict was CHANGES. The CHANGES is silently cleared, and the approver finds no file to object to. | read |
| A6 | `/factory next` in stage `approve` reruns the Opus approver before it checks `is-merged`. | read |
| A7 | `ctl new-job` creates the directory before it saves the board, with no `exist_ok`. A crash leaves an orphan and a retry raises `FileExistsError`. Ids come only from `board.json`, so a lost board restarts at 001 and collides with existing branches. | read |
| A8 | `rework` goes to the builder whenever `plan.md` exists, even for a job parked for "bad plan". The round counter is not reset. | read |
| A9 | `ctl set-stage` accepts any stage, so a mistyped stage moves a job backward. `ctl` cannot clear `builder_id` or `worktree`. `ctl status` hides worktree, head, base, builder id and reason, and there is no `ctl show`. | read |
| A10 | The doc and the command disagree: ledger line form `Task N: complete (...)`, the unused `implementer done` line, `SKIPPED` vs `skipped`, branch-tip vs recorded-head in the manual-merge check, the worktree path name. | read |

### B. Evidence and agent prompts (plan 02)

| # | Gap | Source |
|---|---|---|
| B1 | Nothing records that checks passed. The builder reports checks only in its reply. The approver reads files, so it escalated job 001 for "no clean check recorded". | run, approval.md of job 001 |
| B2 | The approver is not given the spec path, the worktree path or the final round number, and its risk rules ("serde persistence structs", "IPC surface") need more context than a diff has. | read |
| B3 | A test-only dev-dependency in `Cargo.toml` escalates like a real dependency. | run |
| B4 | The builder prompt has no `yarn test <pattern>` and no `cargo check -j4 --manifest-path tools/factory-server/Cargo.toml` for factory-server tasks. | read |
| B5 | The builder has no `maxTurns`. | read |
| B6 | The commit skill spawns subagents and may not work in a subagent. The doc says "the orchestrator commits" but neither the command nor the builder prompt does. | read |
| B7 | The builder wrote `Task 1-2: dispatched` and `Task N: complete` lines straight into the ledger, before review. The dashboard parser reads strict `Task N:` lines only. Prose lines landed in `progress.md`. | run |
| B8 | Minor reviewer findings are never carried forward. | read |
| B9 | The builder was told a worktree path that did not exist (`factory-NNN-<slug>`). The harness made `agent-<id>`. | run |
| B11 | The agents do not use the superpowers skills. The spec writer cannot call any skill, and the builder follows a hand-rolled procedure and names only the git-commit skill. See decision 9. | read |
| B10 | The builder used `sleep 60` waiting on helper agents, and ran clippy and cold builds after approval. 31 minutes of builder time for a 258-line test file. | run |

### C. Main moved, lifecycle and locking (plan 03)

| # | Gap | Source |
|---|---|---|
| C1 | "Main moved" has no recovery. Any unrelated commit on main after the job started blocks the merge. In job 001 the guard was bypassed by hand. A `needs-human` job 001 also leaves job 002 built on a stale main. | run, read |
| C2 | The job base may not be main. With `baseRef: head` and an orchestrator in a worktree, the base includes unrelated commits that then merge into main. The command never checks `base` against `merge-base`. | read |
| C3 | No locking and no job lease. Two sessions can pick the same id or overwrite each other's update. | read |
| C4 | A non-conflict `git merge` failure (untracked-file collision, hook failure) is not handled. There is a check-to-merge race on the shared checkout. | read |
| C5 | No way to cancel a job or remove its worktree and branch. | read |
| C6 | The merge commit message is freeform. The user's rule says every commit goes through `dev-workflow-skills:1-git-commit`. | read |
| C7 | A rework note of `all green` was expanded by the orchestrator into a long builder prompt and wasted two minutes. `rework-note.md` stayed as a stray file. | run |

### D. Multi-plan series (plan 04)

| # | Gap | Source |
|---|---|---|
| D1 | The job model is one feature, one job, one `plan.md` of at most 3 tasks. A bigger feature gets a "Later plans" list in `spec.md` that nothing reads. | read |
| D2 | There is no `Next plan:` pointer and no machine-readable plan index. | read |
| D3 | The builder prompt does not use `superpowers:executing-plans`. It says "do the plan task by task". | read |
| D4 | Nested subagents are unproven. Every agent is a leaf. The git-commit skill and the plan skills spawn subagents, and a subagent may not be able to. | read |
| D5 | The dashboard has no notion of "plan k of N". | read |

### E. Dashboard, server and hook (plans 05 and 06)

| # | Gap | Source |
|---|---|---|
| E1 | Phantom agents: about 60 untyped `SubagentStop` events, and helper `general-purpose` agents counted as job agents. Part of this was fixed in `c0794226`. A resumed builder that is interrupted never gets a `SubagentStop`, so it shows as running. | run |
| E2 | With two jobs in the same stage, an agent can land on the wrong card (`fits_stage` falls back to the first job). The spec writer has no worktree yet. | read |
| E3 | No stage history. `set-stage` overwrites `stage_since`. Merged and needs-human cards show an ever-growing "N in stage". | read |
| E4 | The log is never rotated. The tailer reads the whole file at start, the agent map is never pruned, and the registry is not reset on rotation. | read |
| E5 | `skipped_lines` is counted but never shown. | read |
| E6 | The hook logs the first 80 characters of every Bash command into a mode 0644 file. Commands can hold secrets. | read |
| E7 | No way to act from the page. The commands show as text only. | read |
| E8 | The `status` line is an `aria-live` region rewritten every second. The page replaces `innerHTML` whenever any text changes. | read |
| E9 | The CSP lacks `base-uri 'none'` and `form-action 'none'`. | read |
| E10 | The dashboard look was never checked in a browser. | v1 open item |
| E11 | `/verify-rocket` runs `cargo check` without `-j4` and `cargo test --workspace --no-run`. The harness guard denies both. `CLAUDE.md` also shows a bare `cargo check`. | read |
| E12 | Deferred v1 minors in the three plan ledgers (log-event URL query strings, tool_target tests, verdict lint, `new-job` argument validation, board.json mode, corrupt board traceback, smoke test polling, and others). | ledger |

### F. Token efficiency (plans 07 and 08)

Measured on job 001 (about 12.75M tokens, 11.76M of them cache reads; fresh input plus cache creation plus output is about 0.99M). The review is in the conversation that produced this spec. Evidence tags as above.

| # | Gap | Source |
|---|---|---|
| F1 | The orchestrator session is 53.6 percent of the job (6.83M, largest context 155K re-read each turn). The orchestrator is the interactive session, so part of this is session baggage, not factory design. Part is design: whole diffs and full replies pass through its context. | run, read |
| F2 | The commit skill spawns 2 or more helper agents per commit. Builder commits cost about 80k tokens per helper pair plus 5.4k tokens of skill text carried for about 20 turns, and the merge commit cost 9 helpers. | run |
| F3 | The builder polled with `sleep 60` and `sleep 45` for helpers that finished in 10 seconds. Each sleep is a turn that re-reads about 100k tokens. (Plan 02 bans sleeping.) | run |
| F4 | The same diff is read 5 times in a clean round (4 reviewers plus the approver). There is no size cap, and `Read` truncates at 2000 lines, so a long diff can be silently half-read. | read |
| F5 | The security reviewer on Opus runs on every diff, including doc-only ones. | read |
| F6 | Prompts tell the spec writer, builder and code reviewer to read `CLAUDE.md` and rule files that are already auto-loaded (about 5.5k tokens of fixed context per subagent). | read |
| F7 | The v2 skills add about 9k tokens to the builder and 7k to the spec writer, and `executing-plans` dispatches its own whole-branch review (30 to 60k tokens) and uses ledger scripts that conflict with decision 8. | read, plan review |
| F8 | No token or time budget, no usage logging (the hook drops usage), no cap on builder helper agents, no stall detector. Only the builder lacks `maxTurns`. | read |
| F9 | Sandbox-refused Bash calls (cd out of the worktree, heredoc appends) each cost a turn of 40 to 100k cached tokens. | run |

Already fine (do not re-open): model tiers (Opus only for spec writer, builder, security reviewer and approver, with the builder decided by the user), reviewers being read-only, one builder at a time, a cap of 2 review rounds and 3 tasks per plan, cargo output trimmed, no file read twice by the builder.

## Decisions (rulings for this spec)

1. **One plan at a time, one builder at a time.** Series plans run sequentially. No parallel builders in one worktree.
2. **Chaining runs through the orchestrator.** The `Next plan:` pointer is read by the orchestrator, which starts the next job. Reason: nested subagents are unproven (D4), and the orchestrator already owns the round counter and the merge. If plan 04 task 1 proves nesting works, the builder may use `executing-plans` with nested helpers inside one plan, but the hand-off to the next plan still goes through the orchestrator.
3. **The builder runs `superpowers:executing-plans`** when the Skill tool works in the subagent. If the skill or a nested subagent is not available, the builder follows the plan inline. The builder never fails because a helper cannot spawn.
4. **Checks are evidence.** The builder writes the exact commands and results into a `CHECKS:` block in its reply. The orchestrator saves it as `checks.md` through ctl. The approver escalates when it is missing or any line fails.
5. **Test-only dev-dependencies do not escalate** unless `Cargo.lock` gains a crate or the dependency is not under `[dev-dependencies]`.
6. **Main moved is recoverable.** The orchestrator merges `main` into the job branch in the job worktree through a new `ctl sync-base` flow, re-runs the checks and one scoped review of the sync. It does not rewrite history.
7. **The merge and sync commit messages are fixed conventional templates, written by the orchestrator with no skill call and no helper agents.** The commit skill spawns at least 2 helper agents per commit, and in job 001 the merge commit alone cost 9 helpers (about 700k tokens) for a diff that had nothing to analyze. Template: `chore(factory): merge job NNN <slug>`. Builder task commits still go through `dev-workflow-skills:1-git-commit` (decision 11 trims its ceremony).
8. **Ledger writes go through ctl only.** Builders and reviewers never write the ledger.
9. **Agents use the superpowers skills** (added after review). v1 only names `dev-workflow-skills:1-git-commit`, and the spec writer has no `Skill` tool at all.
   - Spec writer (plan 04, used in `full` mode only; in `lean` mode the orchestrator runs the same two skills itself, see decision 12): `superpowers:brainstorming` without questions (it picks the recommended option and records each choice under "Assumptions" in `spec.md`) and `superpowers:writing-plans`, saving into the job directory. Its `tools:` line gains `Skill`.
   - Builder (plans 02 and 04): `superpowers:executing-plans` for the plan, `superpowers:test-driven-development` per task, `superpowers:systematic-debugging` on a failing check, `superpowers:verification-before-completion` before the `CHECKS:` block, and `superpowers:receiving-code-review` on a resume. It does not use `using-git-worktrees`, `finishing-a-development-branch`, `requesting-code-review`, `brainstorming` or `writing-plans`, because the harness or the orchestrator owns those steps.
   - Reviewers and approver stay read-only without `Skill`. Their checklists already encode the review rules, and a skill call would add turns for no new evidence.
   - Every skill rule has an inline fallback, so a missing Skill tool never fails a job.
10. **Docs stay under `.claude/`.** The design doc `.claude/software-factory.md` is updated by each plan, and `CLAUDE.md` stays small.

11. **Token efficiency is a requirement** (added after the token review). Rules: (a) the orchestrator writes diffs to files by redirect and never holds a diff in its context; (b) reviewers page large diffs with offset and limit, and a diff over the size cap goes to `needs-human` with "split the plan"; (c) the approver gets the diff stat plus the review reports and reads hunks only for paths its risk rules flag; (d) the security reviewer is skipped for doc-only diffs; (e) the builder commits with the skill but runs its git commands inline, with no helper agents and no amend steps, and may spawn at most 2 helper agents per task; (f) agents do not re-read files that are already auto-loaded; (g) every job has a soft token budget with a warning at 80 percent and a stop at 100 percent; (h) usage is logged per agent and shown on the dashboard.

12. **Lean mode is the default, the full pipeline is opt-in** (added after the token review, by user decision). Job 001 cost 12.75M raw tokens with 9 agent runs plus 11 helpers. A plain superpowers flow does the same core work with far fewer agents.
   - **Lean (default):** the orchestrator itself runs `superpowers:brainstorming` (it may ask the user questions) and `superpowers:writing-plans` in the main session and saves spec and plans into the job directory. No spec-writer agent. One `factory-builder` per plan runs `superpowers:executing-plans`. One combined `factory-code-reviewer` (Sonnet) reviews the diff and also checks the hard rules (shadcn, lucide, editors, Zustand, serde, no unwrap, no git CLI). No approver agent: the orchestrator merges when `ctl can-auto-merge` passes (all checks `PASS`, reviewer verdict `APPROVE`, diff class not `risky`, budget not stopped).
   - **Risk escalation:** when the diff class is `risky`, the job runs the v1 review set instead: security (Opus), UX and UI when `src/` changed, code, then the approver. Risky means the diff touches the collection trust gate, auth, secrets or `{{process.env}}`, persistence formats (`.yml` schemas, serde persistence structs), the IPC surface (`src-tauri/`) or `DomainEvent`, `Cargo.toml`, `Cargo.lock`, `package.json`, `tauri.conf.json`, CSP, or the harness hooks. The path list lives in `.claude/factory/config.json` (`risk_paths`).
   - **Full (opt-in, `/factory --full <feature>`):** the v1 pipeline, with the spec-writer agent (decision 9 skills) for unattended runs, and every reviewer and the approver on every job.
   - Board field `mode` is `lean` or `full`. A lean job that turns out risky keeps `mode: lean` and records the escalation in the ledger.

## Interfaces between plans

Later plans rely on these names. Each plan produces its own and may use only the ones listed for earlier plans.

| Plan | Produces |
|---|---|
| 01 | `ctl show <id>` (JSON of one job). `ctl diff-range <id>` (prints `<from>..<to>` for the current round). Board field `prev_head`. `ctl clear-builder <id>`. `ctl set-stage` rejects backward moves unless `--force`. Ids come from the max of board, job directories and `factory/NNN-*` branches. Command merges `<head sha>`, not the branch. |
| 02 | `ctl record-checks <id> --file <path>` writes `checks.md` in the job directory. Builder handoff gains a `CHECKS:` block (`PASS`/`FAIL` lines) and an `UNCOMMITTED:` line when the commit skill cannot run. Reviewer findings carry `[blocker]` or `[minor]` tags. The approver prompt gets `spec.md`, `checks.md`, worktree path and final round, and writes an `UNRESOLVED MINORS:` section. |
| 03 | `ctl sync-base <id> --new-base <sha> [--head <sha>]`, `ctl base-state`, `ctl sync-diff`, `ctl merge-preflight`, `ctl lease`, `ctl cancel <id>`, `ctl cleanup <id>`. A board lock (`flock` on `board.json.lock`) around every mutating ctl call. Board fields `lease`, `cancelled`, `syncs`, `cleaned`. |
| 04 | Board field `series` with `id`, `index` (1-based), `total`, `next_plan`. `ctl new-job --series <id> --plan-file <path>`, `ctl series-start`, `ctl check-series`, `ctl next-plan <id>` (prints the next plan path or `none`). `/factory series <job-id>` recovers a crash between a merge and the next plan. Plan files end with a `Next plan:` line. |
| 05 | Board field `stage_history` (list of stage and timestamp). Server state exposes `skipped_lines`, `series` and per-stage durations. |
| 06 | No interfaces. |
| 07 | Board field `mode` (`lean` default, `full`). `ctl new-job --mode`. `/factory --full <feature>`. `.claude/factory/config.json` with `risk_paths`. `ctl classify-diff --names-file <path>` prints `doc-only`, `normal` or `risky`. `ctl route <id> --names-file <path>` prints the reviewer roles to run (lean and not risky: `code`; lean and risky, or full: the v1 set minus roles the v1 rules skip). `ctl can-auto-merge <id>` exits 0 only when `checks.md` has no `FAIL`, the latest code verdict is `APPROVE`, the class is not `risky` and the budget is not `stop`. Lean spec and plan flow in `factory.md`. Combined reviewer checklist. |
| 08 | `ctl diff-size`, `ctl tokens <id>`, `ctl budget-check <id>`, config keys `diff_cap_lines`, `budget_weighted`. Hook adds a `tokens` field at `SubagentStop`. |

## Plan series

Folder `docs/superpowers/plans/2026-10-13-software-factory-v2/`. Eight plans. Each plan has at most 3 tasks, ends with a `Next plan:` line, and is meant for a builder subagent running `superpowers:executing-plans`. The order is the build order.

| Plan | File | Delivers | Gaps |
|---|---|---|---|
| 01 | `plan-01-orchestrator-robustness.md` | Safer ctl and command: merge the reviewed sha, durable prev head, resume checks, reviewer skip rule, id and stage safety, `ctl show` | A1-A10 |
| 02 | `plan-02-evidence-and-prompts.md` | `checks.md` evidence, approver context, builder and spec-writer prompt fixes, commit fallback, ledger rules | B1-B10 |
| 03 | `plan-03-main-moved-and-lifecycle.md` | Base check, `sync-base`, lock and lease, cancel and cleanup, merge failure handling, merge commit message | C1-C7 |
| 04 | `plan-04-multi-plan-series.md` | Nesting probe, spec writer emits N plans with next pointers, series in ctl, orchestrator chains plans, builder uses `executing-plans` | D1-D5 (server chip in plan 05) |
| 05 | `plan-05-dashboard-server-hook.md` | Hook redaction and stop-event handling, stage history, attribution, log rotation, `skipped_lines`, series chip, page fixes | E1-E9 |
| 06 | `plan-06-harness-and-cleanup.md` | `/verify-rocket` and `CLAUDE.md` fixes, v1 minors triage, browser check of the dashboard, doc sync, roadmap entry | E10-E12 |
| 07 | `plan-07-lean-mode.md` | Lean default mode: orchestrator brainstorms and writes plans with the skills, one combined reviewer, risk classification and escalation, auto-merge rule, `--full` opt-in | F1, F4, F5, F7 |
| 08 | `plan-08-token-efficiency.md` | Diff and context hygiene, commit ceremony and helper cap, token logging and budget | F1-F9 |

## Out of scope

- Approve or rework buttons that act on the board (needs an authenticated POST and a path into the orchestrator session). Plan 05 adds copy-to-clipboard chips only.
- Money cost in currency. Plan 08 logs token counts and a soft budget only.
- Parallel jobs.
- Pushing to a remote. The factory still never pushes.

## Disproved (do not re-open)

- Reviewers are read-only but return their report as the reply, and the orchestrator saves it.
- The spec writer does not need to create folders. `ctl new-job` does.
- The builder already requires a failing test first, and the code reviewer checks it.
- Verdict formats are uniform and fail closed.
- `-j4`, the `--workspace` ban, pathspec commits and `HARNESS_GATE` are in the builder prompt.
- The server binds to 127.0.0.1 only, has a fixed route table (no path traversal), a Host allowlist, GET only, and escapes all dynamic strings.
- Partial log lines, replaced log files and garbage lines are handled.
- The hook always exits 0 and is async.
- `ctl` writes the board atomically. `is-merged` works after a manual merge or a deleted branch. A changed base needs `--force-base`.
- Skill use in a subagent, the agent id match and cwd matching of events to jobs were confirmed in the job 001 run.

## Success check

After plan 06, a dry run of `/factory` on a small feature produces a spec and a plan series, builds plan 1 in a worktree, approves from recorded evidence without a manual intervention, merges the reviewed sha, and then starts plan 2 on its own. The dashboard shows plan k of N and correct per-stage times.
