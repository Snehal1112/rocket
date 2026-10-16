# Factory cost reduction: one committer per job and task-only builder prompts

Follow-up to the accurate usage, failure details and cost report work (merged as 0bc7a363). It uses the measurements that work produced to cut the weighted token cost of a factory job without lowering code or commit-message quality.

## Goal

Cut the weighted tokens of a factory job by running the committer once per job instead of once per task, and by giving each builder only its own task instead of the whole plan and spec.

## Background

Measured on 9 existing jobs with usage data (45 agent runs, 3.03M weighted tokens, all approximate because output counts predate the usage fix):

- Cache creation is 66% of the weighted cost, cache reads 33%, input and output under 1%.
- Builder runs average 82k weighted each (27 runs, about 3 per job): cache creation 49k, cache reads 324k (32k weighted).
- Committer runs average 39k weighted each (11 runs). Their output is about 60 tokens, so almost all of the cost is input.
- A run that starts with nothing cached costs about 20k weighted (the merge-message run: 20k cache creation, no cache read). The rest of the committer's cost is its own input (staged diff up to 60 KB, `commit-context.md`, the skill text).
- Each builder is told to read the whole `plan.md` (16 to 31 KB) and `spec.md` (10 to 13 KB), but does only one task of at most three. Those tokens are written to the cache once and then re-read on every turn.
- The committer runs after every builder task (`build_task`, then `commit_work`), always as a cold session. Builders are cold per task by design (see `.claude/software-factory.md`, "Per-task builders"), and that stays.

## Decisions

- The `dev-workflow-skills:1-git-commit` skill keeps writing every commit message. Commit quality is not traded for tokens.
- The system prompt, tools, agent files and project instructions are not slimmed. The user rejected that for now.
- Builders stay cold and fresh per task. Builder session reuse and one branch per spec are out of scope: the first saves little (a resumed session re-reads the earlier tasks' context on every turn) and the second saves almost no tokens and touches the board, checks, review, sync, merge and cleanup.
- Both changes have a config switch that restores today's behavior.
- No LLM calls are added. The savings are measured with the existing `factory tokens` and `factory costs`.

## Design

### 1. One committer run per job (`commit_batch`, default on)

In `factory-engine` (`build.rs`, `commit.rs`):

- After each builder task, the driver stages everything (`stage_all`) and makes a snapshot commit through git2 with the subject `wip: task N <title>`, with no agent run. An empty stage still parks the job with `task N changed nothing`. The existing guard that a builder committed on its own (tip moved) still applies. The next builder still sees the earlier commits list, because the snapshots are commits on the job branch.
- After the last task, before `finish_build`, the driver records the snapshot tip in the ledger and board, soft-resets the branch to `base`, so every change is staged, and runs one committer with a new `CommitKind::Batch`. Its floor is `base`.
- The Batch prompt is the usual `commit-context.md`, then `## This run` with the plan title, the task titles and for each task the files its snapshot changed (as hints for grouping), the staged diff, and the other facts the committer prompt already carries. The staged diff cut stays at 60 KB. The committer uses `dev-workflow-skills:1-git-commit` as today. Its split path produces focused commits, normally one per task. No rebase is needed, so the committer's allowed git commands do not change.
- The post-run checks stay: HEAD on the branch, floor is an ancestor, clean tree, at least one new commit, a net count within `MAX_COMMITS_PER_RUN` (4), valid messages with the `Relates to:` footer, and one retry through `--resume` of the committer session.
- If the committer still fails, the fallback rebuilds one `chore: <task title>` commit per snapshot, from the recorded snapshot tip, instead of one commit for the whole job. No work is lost: the snapshots stay reachable from the recorded tip.
- Fix rounds are unchanged: each keeps its own Fix committer run with the previous head as floor.
- With `commit_batch` off, `build_task` runs `commit_work` per task as before.

### 2. Task-only builder prompts (`builder_excerpt`, default on)

In `factory-engine` (`specs.rs`, `build.rs`) and the builder agent file:

- The driver builds the task prompt from: the plan header (goal, architecture, global constraints, review focus), the full text of `### Task N:` for the task being run, and the spec's goal and acceptance criteria.
- The prompt still names the full `plan.md` and `spec.md` paths and says to open them only when the task refers to something outside the excerpt.
- `.claude/agents/factory-builder.md` (the "Start" step that tells the builder to read the plan and spec) says to use the excerpt in the prompt.
- If the headings cannot be parsed into the header, the task and the spec parts, the builder gets today's prompt. A whole-plan builder (a plan with no `### Task N:` headings) is unchanged. Fix and rework resumes are unchanged.

### 3. Config and documentation

- `commit_batch` and `builder_excerpt` in `.claude/factory/config.toml`, both booleans, default true. A typo or non-boolean value fails closed like the other config keys.
- `.claude/software-factory.md` documents the batch committer, the snapshot commits, the excerpt prompt and both switches.

## Testing

- `factory-engine`, snapshot and batch: a 3-task plan makes three `wip:` snapshots and one committer run; the final branch has no `wip:` commits and 1 to 4 commits above `base`; an empty task still parks with `task N changed nothing`; a builder that commits on its own still trips the guard; a committer failure after the retry yields one `chore:` commit per snapshot with the snapshot tip recorded; `commit_batch = false` runs one committer per task as before; a fix round still gets its own Fix committer run.
- `factory-engine`, excerpt: the prompt for task 2 contains the plan header and the task 2 text, does not contain the task 1 or task 3 bodies, and still names both file paths; an unparsable plan falls back to the full prompt; `builder_excerpt = false` gives the old prompt.
- `factory-core`: both keys parse, default to true, and reject non-booleans and typos.
- Prompt size: a test pins that the excerpt prompt is smaller than the full-file prompt for a multi-task plan.
- Measurement (manual, after a few new jobs): `factory tokens <id>` and `factory costs` show lower weighted cost per job and per role. The target is about 80k less on the committer side and 30 to 60k less on the builder side. The new `usage_v` 2 lines make the numbers exact.

## Acceptance criteria

- A 3-task plan runs exactly one committer agent run in the build stage (plus fix rounds), and ends with one to four conventional commits with the `Relates to:` footer, no `wip:` commits.
- A builder prompt for task N contains the plan header, the task N text and the spec goal and acceptance criteria, and not the other tasks' bodies.
- Both switches restore today's behavior when set to false, and both fail closed on bad values.
- Park, rework, budget, review, merge and sync behave as before. The existing engine tests pass, with the committer fakes updated for the batch flow.
- `.claude/software-factory.md` documents the change.
- No dollar amounts, no new LLM calls, no change to the system prompt, tools or agent instructions beyond the single builder "Start" step.
