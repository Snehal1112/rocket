# Per-task builders Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task, executed by a `factory-builder` subagent, skipping that skill's final whole-branch review, ledger scripts and progress file. Plans 01 to 08 are merged; this plan edits their code.

**Goal:** The driver runs one fresh builder agent per plan task, one after another, so the ledger and the dashboard know which task is running, and each agent starts with a small context.

**Architecture:** `build_stage` keeps its place in the run loop but handles one task per call while the board field `task_index` is below the task count. Each call writes real `Task N: dispatched` and `Task N: implementer done` ledger lines. After the last task it records `head`, the diff cap and the move to Review exactly as today. Fix rounds and `rework` still resume the last task's session. A plan with no `### Task` headings keeps the old whole-plan single builder. The driver also starts passing `--agent factory-builder`, which it never did.

**Tech Stack:** Rust workspace `tools/factory/` (crates `factory-core`, `factory-git`, `factory-agent`, `factory-engine`, `factory-cli`), the dashboard server `tools/factory-server/`.

**Spec:** the findings of the token investigation of 2026-10-13, summarised here. A fresh agent for task k pays off only when the context it would have inherited, P_k, exceeds `(9F + (n_k+9)S) / n_k` (F = fixed start about 19K to 38K tokens, S = spec plus plan size, n_k = turns of the task). Modelled saving: about 11 percent on a 2-task run, about 17 percent on a 3-task run, slightly negative on very short plans. The main gain is accurate per-task state on the card.

## Global Constraints

- Cargo: always `-j4`, always `--manifest-path tools/factory/Cargo.toml -p <crate>` (server: `tools/factory-server/Cargo.toml`), never `--workspace`, never set `CARGO_TARGET_DIR`.
- A hook blocks the unwrap call in any Rust write. Use `.expect("reason")` in tests and `?` or a mapped error in production code.
- No test may call the real `claude` binary. Use `ScriptedRunner`, `RoleRunner` or `FakeRunner`.
- Persistence structs get no `camelCase` rename, and every new `Job` field is `#[serde(default)]` and skipped when it holds its default, so old boards load and the job 001 board fixture still round-trips byte for byte.
- Comments are short full sentences ending in a period. No emojis.
- Commit with a pathspec (`git add <paths> && git commit --only -m "..." -- <paths>`), invoke `dev-workflow-skills:1-git-commit` for the message rules but run its git commands inline with no helper agents. Never `git add -A` or `.`.

## Review Focus

- A plan with no `### Task` headings (or no `plan.md`) still runs the old single builder.
- A task that makes no commit parks the job with the task number in the reason.
- A budget or stall kill in task 2 leaves task 1 committed. After a `rework` the loop resumes at `task_index`, not at task 1.
- After the last task, fix rounds and `rework` resume the last task's session and never restart the per-task loop.
- The reviewer's round diff (`prev_head..head`) is set once, after the last task, never per task.
- The dashboard shows each task's true state, and legacy jobs without the new ledger marker keep today's stage-based display.

---

### Task 1: Task parser, board field and a commit list helper

📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

**Files:**
- Create: `tools/factory/crates/factory-engine/src/tasks.rs`
- Modify: `tools/factory/crates/factory-engine/src/lib.rs` (declare the module), `series.rs` and `tick.rs` (reuse the parser, keep their behaviour)
- Modify: `tools/factory/crates/factory-core/src/board.rs` (`Job.task_index`), `tools/factory/crates/factory-engine/src/ports.rs` (`JobView` carries it)
- Modify: `tools/factory/crates/factory-git/src/repo.rs` (or the file that holds `tip`) with `Repo::log_oneline`; test in `tools/factory/crates/factory-git/tests/repo_basics.rs`

**Interfaces:**
- Produces: `pub struct PlanTask { pub n: u32, pub title: String }`, `pub fn parse_tasks(plan: &str) -> Vec<PlanTask>`, `Job.task_index: u32` (number of tasks whose agent finished and committed), `Repo::log_oneline(&self, base: &str, head: &str) -> Result<Vec<String>>` (lines `<short sha> <subject>`, oldest first).

- [ ] **Step 1: Write the failing tests.** In `tasks.rs` add unit tests: `### Task 1: first` and `### Task 2: second` give `[PlanTask{n:1,..},..]`; a template heading `### Task N: x` is ignored; the checkbox form `- [ ] Task 3: third` is accepted; text with no tasks gives an empty list; the title is trimmed. In `board_roundtrip.rs` assert that the real job 001 fixture still round-trips and that a job with `task_index` 0 serialises without the key. In `repo_basics.rs` add a temp-repo test: after two commits `log_oneline(base, head)` returns two lines, oldest first, and `log_oneline(head, head)` is empty.
- [ ] **Step 2: Run them and see them fail** (`cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine tasks::`, then `-p factory-core`, then `-p factory-git`). Expected: compile errors or failures for the missing items.
- [ ] **Step 3: Implement.** `parse_tasks` reads lines: `### Task N: title` or `- [ ] Task N: title` / `- [x] Task N: title`, `N` must parse as `u32`. Make `series.rs::read_plan` and `tick.rs` call it instead of their own copies, and keep every existing test of theirs green. Add `#[serde(default, skip_serializing_if = "is_zero_u32")] pub task_index: u32` to `Job`, initialise it in `Job::new`, and copy it into `JobView`. Implement `Repo::log_oneline` with `git2` (revwalk from `head`, hide `base`, reverse to oldest first).
- [ ] **Step 4: Run the three crates' tests.** `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-core`, `-p factory-git`, `-p factory-engine`. Expected: all pass.
- [ ] **Step 5: Commit** `feat(factory): parse plan tasks and track the task index`.

---

### Task 2: The per-task build loop

📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

**Files:**
- Modify: `tools/factory/crates/factory-engine/src/build.rs`, `specs.rs`, `runloop.rs`
- Test: `tools/factory/crates/factory-engine/tests/build_checks.rs` (extend), `tools/factory/crates/factory-engine/tests/runloop.rs` (adjust)

**Interfaces:**
- Consumes: Task 1 (`parse_tasks`, `Job.task_index`, `Repo::log_oneline`).
- Produces: `Engine::builder_task_spec(&self, job: &JobView, cwd: &Path, task: &PlanTask, base: &str, tip: &str, earlier: &[String]) -> AgentSpec`; per-task ledger lines `Task N: dispatched (BASE <sha>, <model>)` and `Task N: implementer done (<short sha>)`; one ledger line `Ruling: per-task builders, <total> tasks` written before the first task.

- [ ] **Step 1: Write the failing tests** in `build_checks.rs` with a 2-task plan (`### Task 1` and `### Task 2` headings in the job's `plan.md`) and two scripted builder effects that each make one commit: (a) after the first `build_stage` call the job is still in Build, `task_index == 1`, the ledger has `Task 1: dispatched`, `Task 1: implementer done` and the `per-task builders` ruling, and the prompt of the first builder call names "Task 1" and not "Task 2"; (b) the second call moves the job to Review, sets `head` to the tip, `prev_head` to the head before the build (None on a first build), `task_index == 2`, and the ledger has the `Task 2` lines; the second prompt contains the first commit's subject from `log_oneline`; (c) a second builder that makes no commit parks the job with a reason containing "task 2"; (d) a plan with no task headings runs one whole-plan builder exactly as before (reuse the existing first-build test); (e) after a CHANGES fix round the resume uses the stored session and the per-task loop is not restarted (`task_index` stays at the total); (f) budget: with `used_before` close to the budget the first task is killed and the job parks with `task_index == 0`, and a later `rework` resumes at task 1 with the note appended to the task prompt and `rework-note.md` removed afterwards.
- [ ] **Step 2: Run them and see them fail.** `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine build_checks`.
- [ ] **Step 3: Implement.** In `build_stage` compute `tasks = parse_tasks(plan.md)`. If it is non-empty and `job.task_index < tasks.len()` take the task path: create the worktree and set the base when `job.worktree` is none (detect the first run by `worktree.is_none()`, not by `session_id`); write the `per-task builders` ruling once, when `task_index == 0`; write `Task N: dispatched (BASE <sha>, <model>)` with the real task number; run `builder_task_spec`; park on a non-`Done` stop with "builder stopped"; compare `repo.tip(branch)` with the tip taken before the run and park with "task N made no commit" if equal; write `Task N: implementer done`; set `builder_id` to the run's session and `task_index += 1` on the board. If tasks remain return the same `StageOutcome` the build stage returns when it stays in Build. After the last task do exactly what the first-run tail does today (head, `prev_head`, diff cap against `base..head`, ledger ruling for the round head, stage Review). When `task_index >= tasks.len()` or the plan has no tasks keep the existing fix and whole-plan paths unchanged. `builder_task_spec` builds a short prompt with the plan path, the spec path (`spec.md` in the job directory when it exists), the branch, the base and the current tip, the `log_oneline` lines of earlier commits, "Do only Task N: <title>. Earlier tasks are already committed, do not redo them. Later tasks belong to other agents.", the rule "make exactly one commit, for this task, and end your reply with the line `HEAD: <sha>`", and "Never sleep or poll, never run a command in the background and wait for it; if a command needs more than ten minutes say so and stop." Set `spec.agent = Some("factory-builder".into())` in the task, whole-plan and fix specs so the agent file loads, and set `spec.max_turns = Some(120)`. In `run_job_inner` raise the loop limit by the task count of the plan so the per-task calls do not use up the fix-round allowance, and renew the lease between calls as the loop already does.
- [ ] **Step 4: Run the engine tests.** `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine`. Fix any test that counted builder calls for a plan with several tasks (the roles in `usage.jsonl` of `runloop.rs` and the CLI e2e tests): update the expected count, never loosen an assertion. Then `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli`.
- [ ] **Step 5: Commit** `feat(factory): run one builder agent per plan task`.

---

### Task 3: Agent file, dashboard state and docs

**Files:**
- Modify: `.claude/agents/factory-builder.md`, `tools/factory/crates/factory-cli/tests/agent_lint.rs` (only if a pinned phrase changes)
- Modify: `tools/factory-server/src/plan.rs` (+ its tests), `.claude/software-factory.md`

**Interfaces:**
- Consumes: the `Ruling: per-task builders` and per-task ledger lines from Task 2.

- [ ] **Step 1: Write the failing server tests** in `plan.rs`: with a ledger that contains `Ruling: per-task builders, 3 tasks`, `Task 1: implementer done (abc)` and `Task 2: dispatched (BASE abc, sonnet)`, `load_plan` at stage `build` gives task 1 `review`, task 2 `active`, task 3 `pending` (the stage rule must not mark every open task active). Without the ruling line the old stage-based rule still applies (the existing tests stay green). At stage `review` with the ruling present every open task is `review`.
- [ ] **Step 2: Run them and see them fail.** `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml plan::`.
- [ ] **Step 3: Implement.** In `load_plan`, read `progress.md` once and pass a flag `per_task` (the text contains `per-task builders`) to `settle_by_stage`; when `per_task` and the stage is `build`, return without changing task states.
- [ ] **Step 4: Update `.claude/agents/factory-builder.md`** for a per-task run: the description says it builds one task of a plan; the input names the plan, the spec and the task number; "do only Task N, earlier tasks are already committed, later tasks belong to other agents"; "exactly one commit, for this task"; "one line in the reply, for Task N"; keep the rules about no sleeping, no helper agents for commits, `-j4`, no unwrap in production code, and the `HEAD:` last line. Keep `maxTurns` and the front matter valid for `agent_lint.rs`, and add one lint check that the file says "Do only Task".
- [ ] **Step 5: Document it** in `.claude/software-factory.md`: the per-task flow, the new ledger lines, the `task_index` board field, the token model (the break-even rule and the 11 to 17 percent estimate, labelled as modelled, not measured), and that the driver now passes `--agent factory-builder`.
- [ ] **Step 6: Run everything.** `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-core`, `-p factory-git`, `-p factory-agent`, `-p factory-engine`, `-p factory-cli`, then `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml` and `node --test tools/factory-server/tests/board.test.js tools/factory-server/tests/patch.test.js`. Expected: all pass.
- [ ] **Step 7: Commit** `feat(factory): show per-task state and document per-task builders`.

Next plan: none
