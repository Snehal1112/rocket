# Factory commits written by the 1-git-commit skill

Date: 2026-10-14. Status: design, revision 2, awaiting review. Area: `tools/factory` (Rust driver), `.claude/agents/`, `.claude/factory/`, `.claude/rules/harness.md`.

Revision 2 replaces the first version (commit `3930d2a1`), which trimmed the skill into a vendored text prompt. The requirement is stronger: commit history is a first-class deliverable, so the factory runs the real skill, with its subagents, amend and split flows, and changes the factory's own rules to allow that.

## Problem

Factory commits are weak. The history shows two causes.

1. **Builder task commits are inconsistent.** The builder is told to use `dev-workflow-skills:1-git-commit` but also "no helper agents", "never amend", "headless, nobody can answer". The skill needs subagents (steps 1, 1b, 1c), has an amend flow and asks the user questions, so the builder skips it or follows part of it. The prompt in `factory-engine/src/specs.rs:65-68` also shows a bare `-m "..."` one-liner, and builders copy that shape. Examples: `2abd7b73`, `de3d55da`, `1f5d44d1`, `f4258243`, `35c2e051`, `f3e3315e`.
2. **Driver merge and sync commits carry no information.** `chore(factory): merge job NNN <slug>` (`factory-engine/src/merge.rs:81`) and `chore(factory): sync main into job NNN <slug>` (`sync.rs:28`) have an empty body and a slug as the only description. Examples: `cf10598e`, `f20c2c04`, `f445e6a5`, `85c45b56`, `e5bd4fb0`, `76dca977`.

The plugin skill loads in a headless run (no `--bare`, no `--disable-slash-commands`, user-scope plugin enabled). The failure is a conflict between the skill's workflow and the factory's rules, so the fix is to change the rules.

## Goal

Every commit and every merge or sync commit message the factory makes is written by `dev-workflow-skills:1-git-commit` (Snehal1112/dev-workflow-skills, `skills/1-git-commit/SKILL.md`), unmodified. The skill is the single source of truth for message quality. The factory supplies context and standing answers, and it enforces the safety rules itself.

## Non-goals

- No copy, fork or trimmed version of the skill.
- No rewrite of existing history, and never of any commit that was reviewed or merged.
- No push, no force-push. No change to the merge preflight, sync limits or parent verification.
- No per-task check loop in the driver.

## Decisions

1. **Scope: all commits.** Task commits, the legacy whole-plan commit, fix-round commits, and the merge and sync commits.
2. **Who runs the skill: a new headless `factory-committer` agent**, started by the driver after each build step and before each merge or sync. The builder never commits. The agent uses the skill with all of its flows (subagents, amend, split) and git commands inline.
3. **Staging: the driver stages everything in the job worktree that is not gitignored**, through `git2`, before it starts the committer. The skill analyses staged changes, so the committer finds them ready. The worktree is dedicated to one job and one implementer, so every change belongs to the current task.
4. **The skill's questions get standing answers**, written in the prompt (table below). Nobody answers during a run.
5. **Amend is allowed only above a floor.** The floor is the base commit in the first build round, and the last reviewed head from then on. A commit at or below the floor is never rewritten, because the review and the merge are pinned to shas. The driver verifies this itself after the run, so it does not rely on the prompt alone.
6. **Model: `commit_model` in `.claude/factory/config.toml`, default `sonnet`.** The skill's own subagents name sonnet.
7. **Failure policy.** A run that fails, leaves changes uncommitted, or writes an invalid message gets one retry by resuming the committer session. After that the driver makes a plain `git2` commit (`chore: <task title>`) and writes a `Ruling:` line. A message problem never parks a job. A rewrite of history at or below the floor does park it.
8. **Gate.** The committer runs with `HARNESS_GATE=0`, set by the driver in the process environment. The builder's own verification and the driver's checks stage after the last task still cover the checks. The builder rule "never set `HARNESS_GATE=0`" stays for builders and reviewers.
9. **Merge and sync messages are written by the skill too, subject and body**, in a message-only mode. The driver appends a deterministic footer `Factory-Job: NNN` so merges stay greppable.
10. **Supersedes** the v2 spec (decision 7) and v3 spec (item 10), which fixed merge and sync messages as templates because the skill's helper agents were too expensive (about 700k tokens for one merge in job 001). Merge and sync no longer use those helpers, and the task-commit cost is measured, not assumed (see Cost).

## Standing answers

The prompt gives the committer these answers to the skill's questions:

| The skill asks | Answer |
|---|---|
| Step 1 verdict is `split` and asks to confirm the split | Confirm. Commit each group, at most 4 commits per run. If there are more groups, merge the smallest. |
| One amend candidate matches | Amend it only if it is above the floor and not pushed. Otherwise make a new commit. |
| Two or more candidates match | Take the most recent one above the floor. If none is above the floor, make a new commit. |
| Amend target is HEAD | `git commit --amend`. |
| Amend target is not HEAD | `git commit --fixup` plus `GIT_SEQUENCE_EDITOR=true git rebase -i --autosquash <target>~1`. On a conflict: `git rebase --abort` and make a new commit. |
| Force-push after amending a pushed commit | Never. A pushed commit is not amended in the first place. |
| Anything else | Pick the option that makes a new commit and changes no existing commit. |

The prompt also states, once, that the project's commit style is Conventional Commits with a lowercase type (`type(scope): description`, as in `CLAUDE.md`). The skill's checklist says to follow the project's existing style, so this needs no edit to the skill. The scope is the crate or area, for example `infra`, `http`, `flow`, `factory`, `frontend`.

## Design

### 1. The committer agent

`.claude/agents/factory-committer.md`, headless, frontmatter `name`, `description`, `maxTurns` only (the existing lint bans `tools`, `model` and `isolation`).

Body rules:
- Invoke the `dev-workflow-skills:1-git-commit` skill and follow it. Let it dispatch its own subagents.
- Use the standing answers from the prompt, and never ask the user anything.
- Allowed git: `status`, `diff`, `log`, `show`, `merge-base`, `add <explicit paths>`, `reset HEAD`, `commit`, `commit --amend`, `commit --fixup`, and `rebase -i --autosquash` with `GIT_SEQUENCE_EDITOR=true` exactly as the skill does it.
- Never: `push`, `merge`, `stash`, `git add -A`, `--all` or `.`, a rebase onto anything other than the autosquash form, a rewrite at or below the floor, or touching the ledger, the board or the job folder.
- Reply: the new commit shas, one line each, or `BLOCKED: <reason>` on the first line.

### 2. Driver flow for task, legacy and fix commits

After the builder run (and before the next task, the review or the next stage):
1. Check the branch tip did not move. A builder that committed anyway parks the job with `builder committed on its own`.
2. Check the worktree has changes. A clean tree parks as `task N changed nothing`, `builder made no changes`, or `builder made no changes on resume`, replacing the "no commit" parks.
3. Stage everything not ignored with `git2` (new `factory-git` function).
4. Record the set of commits in `floor..tip`, then start the committer (`cwd` is the worktree, model `commit_model`, `HARNESS_GATE=0`). The prompt holds the kind (task, legacy, fix), the plan title, the task title, the spec goal line, issue numbers the spec names, the floor sha, and the standing answers.
5. Verify (section 3).

`task_index` keeps counting tasks. A task may now produce one or more commits, so the ledger line `Task N: implementer done (<short sha>)` records the new tip. An amend can rewrite an earlier task's commit, so the driver adds `Ruling: task N commit amended task K commit <old> to <new>` for the audit trail. Earlier ledger lines are not edited.

### 3. Verification after a committer run (driver side, never trusts the prompt)

| Check | If it fails |
|---|---|
| The floor sha is still an ancestor of the new tip | Park: `committer rewrote history at or below the floor`. No retry. |
| The index and worktree are clean | Retry once, then fallback commit through `git2` if still dirty. |
| The tip changed, or an amend rewrote a commit above the floor | Retry once. If still unchanged and the tree is clean, treat as `changed nothing` as in step 2. |
| Every new or rewritten commit passes the subject validator | Retry once. Then keep the commits and write `Ruling: commit message check failed <sha>, <reason>`. |
| At most 4 new commits | Same as the validator failure. |

**Subject validator:** a Conventional Commits type with an optional scope, a description, at most 72 characters (the skill asks for 50, and the driver allows slack so a 51 character subject does not fail), no trailing period, and a blank line after the subject when a body exists.

A retry resumes the committer session with the finding text. A `BLOCKED:` reply, a missing plugin (`Unknown skill`), a timeout or a stall counts as a failed run: one retry, then the fallback.

### 4. Merge and sync commits (message-only mode)

There are no staged changes in a merge, and the driver makes the commit with `git2` (`commit_merge`), so the skill runs in a message-only mode:
- The prompt says: no staged changes exist, do not run `git add`, `commit`, `reset` or `rebase`, skip steps 1b to 1d, 5 and the split flow, and run the skill's step 1 commands inline against `git diff <main>...<head>` (stat and diff) and `git log <main>..<head>` instead of `git diff --staged`. That skips the step 1 subagent, which would also be hard coded to staged changes.
- The prompt names the job, the plan title, the check result from `checks.md`, and for a sync that it merges main into the job branch.
- The reply carries the message between `---BEGIN MESSAGE---` and `---END MESSAGE---`.
- The run is read-only: `--allowedTools` is `Skill`, `Read`, `Grep`, `Glob`, and `Bash` limited to `git diff`, `git log`, `git show` and `git status`.
- The driver validates the message, appends `Factory-Job: NNN`, and passes it to `merge_no_ff` or `merge_main_into`.
- It runs after the preflight and the `base_state` decision and just before the git2 merge, so a refused merge costs no call.

Retry once on an invalid or missing message. Then the deterministic fallback: `chore(factory): merge job NNN <plan title>` (or `sync main into job NNN`), a body listing the task commit subjects and the check result, and the footer. Merge parents, head check and sync limits are unchanged. Tests and docs that pin the old `merge job 001 hello` subject change to the new behaviour.

### 5. Config, runner and git layer

- `config.toml` gets `commit_model = "sonnet"` (serde default, so older configs load).
- `AgentSpec` gets an `env` list, passed to the child process by `claude.rs`. The committer sets `HARNESS_GATE=0`. No other agent sets env.
- `factory-git` gets: stage everything not ignored in a worktree, list the commits in `floor..tip`, an ancestry check, and a clean-tree check. All through `git2`.
- New engine module `commit.rs` with a `Committer` port in `ports.rs`, so tests use a fake runner. It owns the prompt text, the standing answers, the verification table, the retry and the fallback. The prompt text lives in the repo, in `.claude/factory/commit-context.md` (the factory context and standing answers only, not skill text), read from the main checkout like `config.toml`. A missing file is a failed run.
- Usage of the committer is added to the job token tally and `usage.jsonl` with role `committer`.

### 6. Rules, tests and docs

- `.claude/agents/factory-builder.md`: the Commits section becomes "never run `git commit`, `git add`, amend, rebase, reset or push; leave your changes in the worktree, the driver commits". `Never use a helper agent for commits` goes with it.
- `.claude/rules/harness.md`: one added line: the committer agent is the only factory agent allowed to amend or rebase, only above the floor, and only with `HARNESS_GATE=0` set by the driver.
- `factory-cli/tests/agent_lint.rs`:
  - the agent list becomes `factory-builder.md`, `factory-code-reviewer.md`, `factory-committer.md`;
  - the builder pins for `dev-workflow-skills:1-git-commit`, `run its git commands yourself`, `One commit per task` and `pathspec` are replaced by a no-commit pin;
  - new pins and frontmatter checks for the committer: the skill name, `floor`, `Never push`, the standing answers wording.
- `.claude/software-factory.md`: the agents table, the commit and merge sentences, the merge message, the token model (new role), the config keys, the standing answers and the floor rule. The roadmap gets an item.

## Testing

- **Committer with a fake runner:** one commit; a split into two; an amend above the floor; an amend at or below the floor (parks); a rewrite of a reviewed commit (parks); a dirty tree after the run (retry, then fallback); an invalid subject (retry, then ruling); a `BLOCKED:` reply; an `Unknown skill` reply; more than 4 commits.
- **Merge and sync:** delimiter parsing, a missing delimiter, an invalid message, the footer, the fallback, no call when the merge is refused.
- **Validator:** accepted and rejected subjects, including 72 characters, a trailing period, a bad type, a missing blank line.
- **Git layer (`tempfile` repo):** stage new, modified and deleted files, a gitignored file, a clean tree, ancestry and the commit listing after an amend.
- **Engine:** the per-task loop with a builder fake that leaves changes, the guards (committed on its own, changed nothing), a fix round with the floor at the reviewed head.
- **End to end:** adjusted to the new merge subject and to a builder fake that does not commit.
- **Real run:** one small job on a throwaway plan. Read the commits, run `factory tokens`, and record the `committer` share.

## Cost and risks

- **Cost.** A task commit is the committer run plus the skill's subagents: step 1 (one call), step 1b (one call) and step 1c (one call per candidate). That is at least two helper calls and the main session for each commit, against one call in the earlier design. Merge and sync are cheap because they skip the subagents. The v2 data point (9 helpers, about 700k tokens, for one merge) predates the message-only mode. The real run measures the task-commit cost. If the `committer` role takes more than a quarter of a job's weighted tokens, the follow-up is a separate decision, for example skipping step 1b for a commit that touches only new files. This spec does not decide it.
- **Budget.** The committer runs count against the 3,000,000 weighted token job budget, as every agent does. A budget kill during a commit run parks the job as usual.
- **The skill changes under us.** The factory uses the installed plugin, so a new plugin version changes the commit behaviour without a review. This is wanted, and the driver-side checks bound the damage. A pinned plugin version is a possible follow-up.
- **Plugin missing in the headless run.** Handled as a failed run: one retry, then the fallback commit.
- **Amend rewrites unreviewed commits.** Earlier tasks' shas change before review. The ledger audit lines record it. After a review, the floor protects the reviewed commits.
- **A rebase conflict.** The skill aborts the rebase and the standing answer makes a new commit.
- **The gate does not fire for committer commits.** Compile errors surface in the checks stage and go back to the builder as a fix round.
- **Stray files.** Staging everything can include a scratch file. The builder file keeps its guidance, and the reviewer sees the full diff.
- **Building the factory with the factory.** This changes the driver, so it must not run as a factory job. Build it in a normal session, one plan at a time. `config.toml`, `Cargo.toml` and the harness rules are in `risk_paths` regardless.

## Plan shape

Five plans, at most three tasks each, run in order:

1. **Git layer.** Stage, floor and ancestry functions, the commit listing, clean-tree check, with tests.
2. **Runner, config and agent file.** `AgentSpec.env`, `commit_model`, `factory-committer.md`, the lint changes.
3. **Committer module.** `Committer` port, `commit.rs`, `commit-context.md`, the validator, the verification table, retry and fallback, with fake-runner tests.
4. **Build flow.** Task, legacy and fix commits through the committer, the builder prompts and agent file, the guards, engine tests.
5. **Merge, sync and rollout.** Message-only mode, merge and sync wiring, docs and the harness rule, end to end tests, one real run.
