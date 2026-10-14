# Factory commits written by the 1-git-commit skill

Date: 2026-10-14. Status: design, revision 2 plus issue references and KB integration (decisions 14 to 16, added 2026-10-14), awaiting review. Area: `tools/factory` (Rust driver), `.claude/agents/`, `.claude/factory/`, `.claude/rules/harness.md`.

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
2. **Who runs the skill: a new headless `factory-committer` agent**, started by the driver after each build step and before each merge or sync. The builder never commits. The agent uses the skill with its amend and split flows and runs git commands inline. By default it also runs the skill's analysis steps (1, 1b, 1c) inline instead of dispatching subagents, which the skill itself allows as its fallback. A spike measured this at about a third of the cost (see Cost). `commit_subagents = true` in the config turns the subagents back on.
3. **Staging: the driver stages everything in the job worktree that is not gitignored**, through `git2`, before it starts the committer. The skill analyses staged changes, so the committer finds them ready. The worktree is dedicated to one job and one implementer, so every change belongs to the current task.
4. **The skill's questions get standing answers**, written in the prompt (table below). Nobody answers during a run.
5. **Amend is allowed only above a floor.** The floor is the base commit in the first build round, and the last reviewed head from then on. A commit at or below the floor is never rewritten, because the review and the merge are pinned to shas. The driver verifies this itself after the run, so it does not rely on the prompt alone.
6. **Model: `commit_model` in `.claude/factory/config.toml`, default `sonnet`.** The skill's own subagents name sonnet.
7. **Failure policy.** A run that fails, leaves changes uncommitted, or writes an invalid message gets one retry by resuming the committer session. After that the driver makes a plain `git2` commit (`chore: <task title>`) and writes a `Ruling:` line. A message problem never parks a job. A rewrite of history at or below the floor does park it.
8. **Gate.** The committer runs with `HARNESS_GATE=0`, set by the driver in the process environment. The builder's own verification and the driver's checks stage after the last task still cover the checks. The builder rule "never set `HARNESS_GATE=0`" stays for builders and reviewers.
9. **Merge and sync messages are written by the skill too, subject and body**, in a message-only mode. The driver appends a deterministic footer `Factory-Job: NNN` so merges stay greppable.
10. **Supersedes** the v2 spec (decision 7) and v3 spec (item 10), which fixed merge and sync messages as templates because the skill's helper agents were too expensive (about 700k tokens for one merge in job 001, a one-off measurement from the old design). Merge and sync no longer use helpers, and task commits run the analysis inline by default.
11. **Config keys.** `commit_model = "sonnet"`, `commit_subagents = false`, and `commit_budget_weighted = 60000`, a per-run cap on weighted tokens for one committer run, using the budget kill the runner already has. A run over the cap is a failed run: one retry, then the fallback commit.
12. **The driver precomputes the skill's analysis.** It runs the commands of steps 1 and 1b through `git2` (status, staged stat, staged diff, recent log, and the commits above the floor with their files) and puts the output in the prompt, telling the committer to treat it as the result of those commands. The skill's judgement (verdict, amend candidates, the step 1c judge) is unchanged. A spike showed 12 percent fewer weighted tokens and the same messages.
13. **The committer runs with a trimmed prefix**: `--strict-mcp-config` and `--tools "Bash,Read,Skill"` (message-only mode uses its own read-only list). A spike showed 21 percent fewer weighted tokens and the same messages. The runner gets these as spec fields, not hard coded flags.

14. **GitHub issue references (decided 2026-10-14).** A job that fixes a GitHub issue of the selected project records the issue numbers on the board as an optional `issues` list (older boards load without it, and `factory-server` ignores unknown fields). The driver fills it from the feature text of `factory new` (for example `GitHub issue #20 (https://github.com/<owner>/<repo>/issues/20)`) (plan 06 narrowed this, see the status note below). Every commit of that job references the issue, in the footer the skill already supports:
    - Task, legacy and fix commits: `Relates to: #N`.
    - The merge commit: `Fixes: #N`, so the issue closes when the merge reaches the default branch. The factory never pushes, so this happens when the user pushes.
    - The sync commit: `Relates to: #N`.
    - Several issues give one footer line with commas (`Relates to: #20, #21`, `Fixes: #20, #21`).
    The skill writes the footer from the prompt. The driver does not trust that: the validator checks for it (section 3), and the merge footer is appended by the driver next to `Factory-Job: NNN`, so a merge never lacks it.
    Plan 06 status: issue numbers come from the feature text only. A series `spec.md` is not read for them, because a spec that merely cites an issue would otherwise close it with `Fixes:`. Builders and the reviewer also get the KB folder as `--add-dir`, since it sits outside the job worktree.
15. **KB read side (decided 2026-10-14).** `kb_path` in `config.toml` names the Rocket knowledge base (`~/data/rocket/Nl-knowledge-base/rocketapi`). Builder and reviewer prompts both get one line when it is set: read the KB `README.md` index first, then only the topic pages that apply to the task (the `kb-lookup` approach), and never write to the KB. The KB is context, not truth: source and the spec win on any conflict, and a builder reports a stale page in its reply.
16. **KB refresh after each merge (decided 2026-10-14).** After a job's merge lands on the main checkout, the driver starts a headless `factory-kb-refresher` agent that runs the `kb-refresh` skill for the Rocket KB. It may use the skill's own subagents. The `Workflow` tool is not used. Rules for the run:
    - It diffs since the last verified commit, re-verifies only the affected topic pages against source, and edits only files under `kb_path`.
    - Model `kb_model` (default `sonnet`), capped by `kb_budget_weighted` per run.
    - It never blocks or parks anything: the merge is already done. A failed or over-budget run writes `Ruling: kb refresh failed, <reason>` and the job stays merged. `factory kb-refresh` reruns it by hand.
    - The driver checks afterwards that the main checkout of Rocket is unchanged by the run (same tip, clean tree). A change there is a `Ruling:` line and a stop of further KB refreshes until the user looks, because this agent must not touch source.
    - It leaves its edits uncommitted in the KB folder, and the reply lists the pages changed. Whether to commit them automatically is left to the user (see Cost and risks).
    - Usage is tallied with role `kb`.

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
- Invoke the `dev-workflow-skills:1-git-commit` skill and follow it. The driver's prompt says whether its steps 1, 1b and 1c run inline (the default: do them with your own tool calls, dispatch no subagent) or through the skill's subagents (`commit_subagents = true`).
- Use the standing answers from the prompt, and never ask the user anything.
- Allowed git: `status`, `diff`, `log`, `show`, `merge-base`, `add <explicit paths>`, `reset HEAD`, `commit`, `commit --amend`, `commit --fixup`, and `rebase -i --autosquash` with `GIT_SEQUENCE_EDITOR=true` exactly as the skill does it.
- Never: `push`, `merge`, `stash`, `git add -A`, `--all` or `.`, a rebase onto anything other than the autosquash form, a rewrite at or below the floor, or touching the ledger, the board or the job folder.
- Reply: the new commit shas, one line each, or `BLOCKED: <reason>` on the first line.

### 2. Driver flow for task, legacy and fix commits

After the builder run (and before the next task, the review or the next stage):
1. Check the branch tip did not move. A builder that committed anyway parks the job with `builder committed on its own`.
2. Check the worktree has changes. A clean tree parks as `task N changed nothing`, `builder made no changes`, or `builder made no changes on resume`, replacing the "no commit" parks.
3. Stage everything not ignored with `git2` (new `factory-git` function).
4. Record the set of commits in `floor..tip`, then start the committer (`cwd` is the worktree, model `commit_model`, `HARNESS_GATE=0`). The prompt holds the kind (task, legacy, fix), the plan title, the task title, the spec goal line, the job's `issues` list with the footer to use (`Relates to: #N`, decision 14), the floor sha, and the standing answers.
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

**Subject validator:** a Conventional Commits type with an optional scope, a description, at most 72 characters (the skill asks for 50, and the driver allows slack so a 51 character subject does not fail), no trailing period, and a blank line after the subject when a body exists. When the job has `issues`, every new or rewritten commit also needs the `Relates to: #N` footer naming all of them.

A retry resumes the committer session with the finding text. A `BLOCKED:` reply, a missing plugin (`Unknown skill`), a timeout or a stall counts as a failed run: one retry, then the fallback.

### 4. Merge and sync commits (message-only mode)

There are no staged changes in a merge, and the driver makes the commit with `git2` (`commit_merge`), so the skill runs in a message-only mode:
- The prompt says: no staged changes exist, do not run `git add`, `commit`, `reset` or `rebase`, skip steps 1b to 1d, 5 and the split flow, and run the skill's step 1 commands inline against `git diff <main>...<head>` (stat and diff) and `git log <main>..<head>` instead of `git diff --staged`. That skips the step 1 subagent, which would also be hard coded to staged changes.
- The prompt names the job, the plan title, the check result from `checks.md`, the job's `issues`, and for a sync that it merges main into the job branch. It tells the skill to leave the issue footer out of the message, because the driver appends it.
- The reply carries the message between `---BEGIN MESSAGE---` and `---END MESSAGE---`.
- The run is read-only: `--allowedTools` is `Skill`, `Read`, `Grep`, `Glob`, and `Bash` limited to `git diff`, `git log`, `git show` and `git status`.
- The driver validates the message, appends `Fixes: #N` (merge) or `Relates to: #N` (sync) when the job has `issues`, then `Factory-Job: NNN`, and passes it to `merge_no_ff` or `merge_main_into`.
- It runs after the preflight and the `base_state` decision and just before the git2 merge, so a refused merge costs no call.

Retry once on an invalid or missing message. Then the deterministic fallback: `chore(factory): merge job NNN <plan title>` (or `sync main into job NNN`), a body listing the task commit subjects and the check result, and the same footers (issue and `Factory-Job`). Merge parents, head check and sync limits are unchanged. Tests and docs that pin the old `merge job 001 hello` subject change to the new behaviour.

**Plan 05 status.** Message runs use only `Read,Skill` (no Grep, Glob or Bash). The driver-appended `Fixes`/`Relates to`/`Factory-Job` footers land in plan 06. Until then the fallback message is the bare `chore(factory): merge job <id> <slug>` or `chore(factory): sync main into job <id> <slug>`, with no body or footer.

### 5. Config, runner and git layer

- `config.toml` gets `commit_model = "sonnet"`, `commit_subagents = false` and `commit_budget_weighted = 60000` (all with serde defaults, so older configs load). The runner applies the budget to the committer spec through `with_limits`' existing budget fields, as a per-run cap instead of the job cap.
- `config.toml` also gets `kb_path` (default unset, which turns the KB read side and the refresh off), `kb_model = "sonnet"` and `kb_budget_weighted` (default 150000, a guess until the first real run measures it). Same serde defaults.
- The board job gets an optional `issues` list (numbers). `factory new` parses `#N` from a `GitHub issue #N` feature text and from the spec. Both are optional, so old boards and `factory-server` are unaffected.
- A new agent file `.claude/agents/factory-kb-refresher.md` (headless, same lint rules as the committer) and a `KbRefresher` port in `ports.rs` with a fake for tests. It runs with `cwd` at the main checkout and an allowed-write scope of `kb_path` only. The `factory kb-refresh` command runs it by hand.
- `AgentSpec` gets an `env` list, passed to the child process by `claude.rs`. The committer sets `HARNESS_GATE=0`. No other agent sets env.
- `factory-git` gets: stage everything not ignored in a worktree, list the commits in `floor..tip`, an ancestry check, and a clean-tree check. All through `git2`.
- New engine module `commit.rs` reusing the `AgentRunner` port, so tests use `ScriptedRunner`. The message-only mode lives in `message.rs`. It owns the prompt text, the standing answers, the verification table, the retry and the fallback. The prompt text lives in the repo, in `.claude/factory/commit-context.md` (the factory context and standing answers only, not skill text), read from the main checkout like `config.toml`. A missing file is a failed run.
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
- **Issue references:** a job with one issue and with two; task, fix and legacy commits need `Relates to`; the merge gets `Fixes` and `Factory-Job` appended by the driver; the sync gets `Relates to`; a commit missing the footer fails the validator (retry, then ruling); a job without issues adds no footer; `factory new` parses `GitHub issue #20 (...)`.
- **KB:** builder and reviewer prompts carry the KB line only when `kb_path` is set; the refresher runs after a merge with a fake and never parks on failure; a changed Rocket tip or dirty tree after the run writes the ruling and stops later refreshes; an over-budget run is a failed run; `factory kb-refresh` runs it by hand.
- **Prompt switches:** the committer prompt says "inline, no subagents" when `commit_subagents` is false and does not say it when true. A run over `commit_budget_weighted` counts as a failed run.
- **Real run:** one small job on a throwaway plan. Read the commits, run `factory tokens`, and record the `committer` weighted tokens per commit, cold and warm, in `software-factory.md`.

## Cost and risks

- **Cost, measured by a spike on 2026-10-14.** The skill ran headless (`claude -p --model sonnet`) on a scratch Rust repo with a 16 line staged change that overlaps an earlier commit (the worst case: it triggers the amend search and judge). Weighted = input + cache creation + output + cache read / 10, as the job budget counts it. Cold cache, one run each:

  | Variant | Subagents | Weighted tokens | Raw tokens | Cost (USD) | Commits |
  |---|---|---|---|---|---|
  | Skill as written | 3 (steps 1, 1b, 1c) | 86,333 | 332,624 | 0.205 | 2 (split) |
  | Steps 1, 1b, 1c inline | 0 | 28,998 | 109,142 | 0.064 | 2 (split) |

  Both variants wrote conventional, lowercase subjects with a short why-body, and both split the change into two commits. The inline run is about a third of the cost with the same message quality, so inline is the default.
- **A second spike tested two cost levers**, same scratch repo, same method, runs back to back (the control is a repeat of the inline variant):

  | Variant | Weighted | Raw | Cost (USD) | Turns | Cache creation | Cache read |
  |---|---|---|---|---|---|---|
  | Control: inline | 29,436 | 110,159 | 0.065 | 5 | 19,799 | 89,692 |
  | Analysis precomputed in the prompt | 25,886 | 80,834 | 0.060 | 4 | 19,232 | 61,053 |
  | Trimmed run (`--strict-mcp-config --tools "Bash,Read,Skill"`) | 23,226 | 74,975 | 0.055 | 6 | 16,762 | 57,498 |
  | Both | 22,536 | 55,655 | 0.056 | 4 | 18,221 | 36,798 |
  | Both, rerun in the same directory a minute later | 21,182 | 55,696 | 0.052 | 4 | 16,702 | 38,348 |

  The two levers together save about 23 percent of weighted tokens. The commit messages are equivalent in every run (the render change gets `feat` or `fix` from run to run, which is noise).
- **Where the rest of the cost sits.** Cache creation (16K to 20K tokens, counted in full) is about 75 percent of every run. It is the first call of a fresh session writing its fixed prefix. The rerun in the same directory a minute later still created 16,702 tokens, so fresh headless runs do not reuse each other's cache, and the reason is not known (a per-session element in the prefix, or the cache lifetime). Cross-run cache warmth is therefore not assumed. An earlier estimate of about 11,500 weighted tokens for a warm run is withdrawn: it assumed warm reuse that this check did not show.
- **Why the interactive cost is lower.** The roughly 12K the user sees is the marginal cost of the skill inside an already-running session whose prefix is cached. A one-shot headless run pays the prefix itself. Resuming one committer session per job (`--resume`) could recover that, but only when the cache is still live between commits, and builder runs between commits last minutes. This needs a measurement before it is a design decision (see Plan shape, plan 2).
- **Limits of the spike.** One run per variant, a tiny diff, and a scratch repo, so the fixed prompt did not include Rocket's `CLAUDE.md` and rule files, which a worktree run adds to every turn. Treat the numbers as an order of magnitude. The real run on a throwaway plan replaces them in `software-factory.md`.
- **Budget.** The committer runs count against the 3,000,000 weighted token job budget, as every agent does, and each run is also capped by `commit_budget_weighted`. The default of 60,000 passes a cold inline run (about 29,000) with margin and stops a runaway one. It would stop a cold run with subagents (about 86,000), which is intended: turning `commit_subagents` on means raising the cap too.
- **The skill changes under us.** The factory uses the installed plugin, so a new plugin version changes the commit behaviour without a review. This is wanted, and the driver-side checks bound the damage. A pinned plugin version is a possible follow-up.
- **Plugin missing in the headless run.** Handled as a failed run: one retry, then the fallback commit.
- **Amend rewrites unreviewed commits.** Earlier tasks' shas change before review. The ledger audit lines record it. After a review, the floor protects the reviewed commits.
- **A rebase conflict.** The skill aborts the rebase and the standing answer makes a new commit.
- **The gate does not fire for committer commits.** Compile errors surface in the checks stage and go back to the builder as a fix round.
- **Stray files.** Staging everything can include a scratch file. The builder file keeps its guidance, and the reviewer sees the full diff.
- **Auto-closing issues.** `Fixes: #N` on the merge closes the issue when the merge reaches the default branch, even if the work was only partly right. The factory does not push, so the user's push is the last gate. A job that only helps an issue (not a fix) should be created without the issue number, or the user can reword the merge before pushing.
- **KB refresh cost and scope.** Each merge adds one headless run that reads a diff and several KB pages, which is likely tens of thousands of weighted tokens, and more if the skill dispatches its subagents. `kb_budget_weighted` caps it. The first real run measures it and `software-factory.md` records the number.
- **KB lives outside the repo.** The refresher edits files under `kb_path`, which is not part of the Rocket worktree, so the driver's worktree checks do not cover it. The only guard is the allowed-write scope and the post-run check that Rocket itself is unchanged. The edits stay uncommitted until the user reviews them. Open point: commit them automatically, with the same commit skill, once the first few runs look right.
- **Stale KB reads.** A builder or reviewer can follow a stale page. The prompt line ranks source and the spec above the KB, and the refresh after each merge keeps pages near the current source.
- **Building the factory with the factory.** This changes the driver, so it must not run as a factory job. Build it in a normal session, one plan at a time. `config.toml`, `Cargo.toml` and the harness rules are in `risk_paths` regardless.

## Plan shape

Seven plans, at most three tasks each, run in order. Plans 1 to 5 are the commit work, with the issue references (decision 14) folded in where they say. Plans 6 and 7 are the KB work (decisions 15 and 16):

1. **Git layer.** Stage, floor and ancestry functions, the commit listing, clean-tree check, with tests.
2. **Runner, config and agent file.** `AgentSpec.env`, `commit_model`, `factory-committer.md`, the lint changes.
3. **Committer module.** `Committer` port, `commit.rs`, `commit-context.md`, the validator, the verification table, retry and fallback, with fake-runner tests.
4. **Build flow.** Task, legacy and fix commits through the committer, the builder prompts and agent file, the guards, engine tests.
5. **Merge, sync and rollout.** Message-only mode, merge and sync wiring, docs and the harness rule, end to end tests, one real run.
6. **Issue references and KB read side.** The board `issues` field and its parsing in `factory new`, the footer in the committer prompt, the validator check, the driver-appended merge and sync footers, `kb_path` and the KB line in builder and reviewer prompts, with tests. This plan touches plans 3 to 5 code, so it runs after them.
7. **KB refresh after merge.** `factory-kb-refresher.md` and its lint pins, the `KbRefresher` port, the post-merge call, the post-run guard, the `factory kb-refresh` command, the `kb` usage role, docs, and one real run that measures the cost.
