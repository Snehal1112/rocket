# Factory driver-owned commits

Date: 2026-10-14. Status: design, awaiting review. Area: `tools/factory` (Rust driver), `.claude/agents/factory-builder.md`, `.claude/factory/`.

## Problem

Factory commits are weak. The history shows two causes.

1. **Builder task commits are inconsistent.** The builder is told to use `dev-workflow-skills:1-git-commit` but also "no helper agents", "never amend", "headless, nobody can answer". The skill needs subagents (steps 1, 1b, 1c), has an amend flow and asks the user questions, so the builder skips it or follows part of it. The prompt in `factory-engine/src/specs.rs:65-68` also shows a bare `-m "..."` one-liner, and builders copy that shape. Examples: `2abd7b73`, `de3d55da`, `1f5d44d1`, `f4258243`, `35c2e051`, `f3e3315e`.
2. **Driver merge and sync commits carry no information.** `chore(factory): merge job NNN <slug>` (`factory-engine/src/merge.rs:81`) and `chore(factory): sync main into job NNN <slug>` (`sync.rs:28`) have an empty body and a slug as the only description. Examples: `cf10598e`, `f20c2c04`, `f445e6a5`, `85c45b56`, `e5bd4fb0`, `76dca977`.

The plugin skill loads fine in a headless run (no `--bare`, no `--disable-slash-commands`, user-scope plugin enabled). The conflict is between the skill's workflow and the builder's rules, not availability.

## Goal

Every commit the factory makes has a message written under the rules of the `1-git-commit` skill (Snehal1112/dev-workflow-skills, `skills/1-git-commit/SKILL.md`), adapted only where Rocket's conventions or a headless run require it. The driver, not the builder, makes every commit.

## Non-goals

- No rewrite of existing history. Past weak commits stay.
- No change to merge mechanics, preflight, sync limits or parent verification.
- No change to the harness hooks. The commit-gate hook simply does not fire for `git2` commits.
- No commit splitting, amend, fixup or user confirmation. Those parts of the skill cannot run headless.
- No per-task check loop in the driver (see decision 3).

## Decisions (from the design discussion)

1. **Scope: all commits, driver-owned.** Task commits, the legacy whole-plan commit, fix-round commits, and the merge and sync commits.
2. **Staging: everything changed in the job worktree that is not gitignored.** The worktree is dedicated to one job with one implementer at a time, so every change belongs to the current task.
3. **No replacement for the commit-gate hook.** The builder's own verification step still runs the checks the plan names. The driver's checks stage still runs after the last task, and a failure still goes back to the builder as a fix round.
4. **Failure handling: retry once, then fall back.** Fallback message is `chore: <task title>` (no body), plus a `Ruling:` ledger line. A commit message never parks a job.
5. **Model: `commit_model`, default `sonnet`**, a new key in `.claude/factory/config.toml`.
6. **Supersedes** v2 spec decision 7 and v3 spec item 10 ("merge and sync messages are fixed templates, no skill call"). Those fixed the cost of the skill's helper agents (about 700k tokens for one merge in job 001). The new design makes one text-only call with no helpers and a bounded input, so the cost is small and measurable (see Cost).

## Design

### 1. Vendored prompt: `.claude/factory/commit-prompt.md`

The prompt reuses the skill text as it is wherever the skill applies, and says up front what it changes. The file has four parts, in this order.

1. **Header.** Names the source (`Snehal1112/dev-workflow-skills`, `skills/1-git-commit/SKILL.md`, plugin 1.0.2, commit `404e160`) and says the verbatim sections are copied, so a drift is visible in review.
2. **Factory overrides.** A short block placed first so it wins over the copied text.
3. **Skill sections, verbatim.** See the table.
4. **Input and output contract.** What the driver sends and what the reply must be.

| Skill section | Treatment |
|---|---|
| Step 2, Seven Rules (SKILL.md:184-194) | Verbatim. |
| Step 3, structure template, Conventional Commits format, subject types (:196-233) | Verbatim, plus the override that the conventional form is required, not optional. |
| Scope guidelines (:235-251) | Copied, with the example list of scopes replaced by Rocket's crate names and areas (`infra`, `http`, `collection`, `flow`, `factory`, `frontend` and so on), taken from the existing history. |
| Subject, body, footer guidelines (:253-291) | Verbatim, with the issue-reference footers kept (`Fixes: #N`, `Relates to: #N`) because Rocket's history already uses them. |
| Step 4, quality checklist (:293-306) | Verbatim, as the self-check the model runs before replying. |
| Examples, anti-patterns (:328-424) | Copied, with examples rewritten to Rocket code (the skill's are PHP/MAPI). The anti-pattern list stays word for word. |
| Context-specific guidelines (:426-473) | Verbatim. |
| Code review philosophy (:475-495) | Verbatim. |
| Other considerations (:587-593) | Verbatim, except the merge-commit line, which is replaced by the factory's merge rule. |
| Steps 1, 1b, 1c, 1d (subagents, amend, interactive) | Dropped. A headless text call has no tools and nobody to ask. |
| Step 5 (heredoc `git commit`) | Dropped. The driver commits through `git2`. |
| Commit splitting strategy (:499-585) | Dropped. One task is one commit. |
| GitLab-specific features, MR references (:604-625) | Dropped. Rocket uses GitHub. |

**Overrides (the only deliberate differences from the skill):**

- The conventional form `type(scope): subject` is required, as in `CLAUDE.md`. The skill's "capitalize the subject line" rule is dropped: the type is lowercase and the description after the colon starts lowercase, like the repo's history (`fix(factory): clear cancelled flag on rework`). The prompt states this once, in the override block.
- The 50 character subject limit and the imperative mood stay. The driver's validator is looser than the prompt (see below), so a 51 character subject is not a failure.
- Reply with the message text only: no preamble, no code fence, no commentary.
- Mention an issue only when the input names one. Never invent an issue number.

### 2. Config

`.claude/factory/config.toml` gets `commit_model = "sonnet"`. In `factory-core` the field has a serde default of `"sonnet"`, so existing configs still load. The diff line cap given to the composer is a constant in `commit.rs` (1500 lines), not config.

### 3. Git layer (`factory-git`)

A new `Repo` function commits the worktree on its job branch.

- Stages every modified, deleted and new file that is not gitignored, writes the tree, and commits with the given message on the branch tip. Returns `Some(sha)` or `None` when the tree is clean.
- Uses the existing `signature()` fallback, so authorship stays the user's git identity.
- Runs through `git2`, so no hook runs and no `git` CLI is used.
- A function that returns the diff stat and a line-capped diff of the staged change, for the composer input, in the same module as the existing `diff_to_files`.

### 4. Composer (`factory-engine/src/commit.rs`)

A `Committer` trait in `ports.rs` has two operations: compose a message from a request, and make the commit. The engine depends on the trait, so tests use a fake.

**Request.** Kind (`Task`, `LegacyPlan`, `Fix`, `Merge`, `Sync`), the plan title, the task title when there is one, the spec goal line when `spec.md` exists, the diff stat, the line-capped diff (task, plan and fix) or the list of task commit subjects (merge and sync), the check result summary from `checks.md` (merge and sync), and any issue numbers found in the spec.

**Call.** One `claude -p` run through the existing runner, role `committer`:

- `--model <commit_model>`, `--tools ""` (confirmed in `claude --help`: `""` disables all tools), `--max-turns 1`, no `--agent`, no resume.
- The prompt is the vendored file plus the request, sent on stdin.
- The working directory is the job directory, not the worktree, so the project `CLAUDE.md` and rule files are not loaded into a text call.
- Usage goes into the job token tally and `usage.jsonl` with role `committer`, so `factory tokens` shows it.

**Validation.** The reply is accepted when:

- the first line matches `type(scope)?: description` with a type from the Conventional Commits set;
- the subject is at most 72 characters and has no trailing period;
- a blank line follows the subject, when a body is present;
- there are no code fences, and no text before the subject.

Body wrapping is not enforced, because the driver cannot rewrap safely.

**Retry and fallback.** A failed run (non-zero exit, timeout, stall, empty reply) or an invalid reply gets one retry with the validation error appended. A second failure uses the fallback: `chore: <task title>` for task and fix commits, `chore: <plan title>` for the legacy plan commit. Merge and sync use the deterministic subject and body described below. A fallback writes `Ruling: commit message fallback, <reason>` to the ledger.

**Prompt file location.** The composer reads `.claude/factory/commit-prompt.md` from the main checkout, as `config.toml` is read. A missing file is the same as a failed call: fallback plus a ruling.

### 5. Build flow (`factory-engine/src/build.rs`, `specs.rs`)

- **Task builders.** The builder does the task and its verification and stops without committing. The driver checks that the worktree has changes, composes a message, and commits. A clean tree parks the job with `task N changed nothing` (replacing `task N made no commit`). `task_index`, the ledger lines `Task N: dispatched` and `Task N: implementer done (<short sha>)`, and the board fields keep their meaning.
- **Legacy whole-plan builder.** One commit at the end of the run with kind `LegacyPlan`. A clean tree parks with `builder made no changes`.
- **Fix rounds and rework.** The resumed builder fixes and stops without committing. The driver commits the change with kind `Fix`, with the findings file or rework note headline in the request, so the model usually picks `fix:`. A clean tree after a resume parks with `builder made no changes on resume`, replacing today's check for an unmoved tip.
- **Prompts.** The `Commit each task...` and `Make exactly one commit...` sentences and the `HEAD: <sha>` last-line instruction are removed from the three builder prompts in `specs.rs`. They say the driver commits, and the builder must not run `git commit`.
- **Guard.** After a builder run the driver verifies the branch tip did not move. A builder that committed anyway is treated as a protocol violation: the job parks with `builder committed on its own`, because a clean-tree assumption no longer holds.

### 6. Merge and sync commits (`merge.rs`, `sync.rs`)

- **Subject.** Deterministic and greppable, keeping the prefix that tests and docs pin: `chore(factory): merge job NNN <plan title>` and `chore(factory): sync main into job NNN`. The plan title replaces the slug, falling back to the slug when the plan has no title.
- **Body.** Composed with the prompt, kind `Merge` or `Sync`. The input is the list of task commit subjects (`git log base..head`), the diff stat and the check result from `checks.md`. The body says what landed and the check result.
- **Fallback.** The deterministic subject, and a body that lists the task commit subjects and the check result with no model call.
- The merge commit's parents, the head check, the preflight and the sync limits are unchanged.

### 7. Agent file, tests and docs

- `.claude/agents/factory-builder.md`: the Commits section is replaced by "never run `git commit`, `git add`, amend, rebase, reset or push; leave your changes in the worktree, the driver commits". `Never use a helper agent for commits` goes with it.
- `factory-cli/tests/agent_lint.rs` (`builder_body_pins_the_rules`, lines 126-175): remove the pins for `dev-workflow-skills:1-git-commit`, `run its git commands yourself`, `One commit per task` and `pathspec`. Add pins for the no-commit rule. The other pins stay.
- `.claude/software-factory.md`: update the builder row, the commit sentence, the merge-rules paragraph (the new merge message), the checks note (no per-commit gate), the config key list, and add the `committer` role to the token model.
- `.claude/roadmap.md`: add the item if it tracks factory work.
- Existing tests that pin `starts_with("chore(factory): merge job 001")` keep passing, because the prefix is kept. `factory-engine/tests/merge.rs:14` asserts the whole first line equals `chore(factory): merge job 001 hello`, so it changes to the new subject.

## Testing

- **Composer unit tests (fake runner):** a valid reply, an invalid reply then a valid retry, two failures giving the fallback, an empty reply, a reply with a preamble or code fence, a missing prompt file.
- **Validator unit tests:** accepted and rejected subjects, including the 72 character cap, a trailing period and a bad type.
- **Git layer (`tempfile` repo):** a dirty tree, a clean tree, a new file, a deleted file, a gitignored file, authorship fallback.
- **Engine:** the per-task loop with a builder fake that writes files and does not commit; a clean-tree park; a builder that commits on its own parks; a fix round commits with kind `Fix`; merge and sync bodies for the success and fallback cases.
- **End to end (`factory-cli/tests/e2e.rs`, `new_to_merged.rs`):** adjusted so the builder fake leaves changes uncommitted, and the merge subject check covers the new title.
- **Lint:** `agent_lint.rs` runs over the changed agent file.
- **Real run:** one small job on a throwaway plan, to read the commits and measure the `committer` cost with `factory tokens`.

## Cost and risks

- **Cost.** One Sonnet call per task, per fix round, per sync and per merge. The input is the vendored prompt (about 3 to 4k tokens) plus the stat and a diff of at most 1500 lines. The reply is short. The measured number from the real run replaces this estimate in `software-factory.md`, as the v3 token comparison does.
- **User-level instructions.** The call runs with the user's global `CLAUDE.md` loaded (it tells sessions to invoke the commit skill). With `--tools ""` the model cannot call it, and validation plus the fallback cover a reply that tries. If the real run shows it is a problem, the plan adds a system prompt override.
- **Stray files.** Staging everything can include a scratch file. The builder file keeps its "no scratch files" guidance, and the reviewer sees the full diff.
- **A builder that still runs `git commit`.** Guarded by the lint pin and the tip check in section 5.
- **Drift from the plugin.** The vendored sections can fall behind the skill. The header records the source version, and a re-sync is a manual copy.
- **Building the factory with the factory.** This change edits the driver that runs jobs, so it should not run as a factory job. A job would commit with the old behaviour half way through its own change. Build it in a normal session, one plan at a time. `config.toml` and any `Cargo.toml` change are in `risk_paths` regardless.

## Plan shape

Four plans, at most three tasks each, run in order:

1. **Prompt and git layer.** `commit-prompt.md`, `commit_model` in config, the `factory-git` commit function and diff helper, with tests.
2. **Composer.** `Committer` port, `commit.rs`, validator, retry and fallback, usage logging, with tests.
3. **Build flow.** Task, legacy and fix-round commits through the composer, the builder prompts in `specs.rs`, the clean-tree and tip guards, engine tests.
4. **Merge, sync and rollout.** Merge and sync bodies, agent file, lint pins, docs, end to end tests, one real run.
