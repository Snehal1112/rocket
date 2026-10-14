# Factory spec gate: command, docs and lint Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `/factory` force the full `superpowers:brainstorming` interview and `superpowers:writing-plans` flow, with a coverage gate, committed spec and plans, repo-relative paths and a pathspec commit rule, and pin all of it in the lint test.

**Architecture:** Text edits to `.claude/commands/factory.md` and `.claude/software-factory.md`, then new literals in `tools/factory/crates/factory-cli/tests/agent_lint.rs` that fail if the command text drifts. Plan 1 (`plan-01-driver-fixes.md`) made the driver accept `--spec` only with `--plans` and parse sections correctly. This plan depends on that behavior but not on its code.

**Tech Stack:** Markdown command and docs, one Rust integration test file in the `tools/factory` workspace.

**Spec:** The approved "Group A" list from the 2026-10-14 factory audit, items 3 to 10 (the command text, docs and lint items). Related design: `docs/superpowers/specs/2026-10-13-software-factory-v3-rust-driver-design.md`.

## Global Constraints

- Cargo for this workspace: `cargo <cmd> -j4 --manifest-path tools/factory/Cargo.toml`. Never `--workspace` or `--all`.
- Keep every literal that `agent_lint.rs` already pins in `factory.md` (see the test `command_is_thin_and_pins_the_shapes`). The forbidden words `factory-ctl`, `python3`, `factory-spec-writer`, `factory-approver`, `set-stage`, `SendMessage` and the text `.superpowers/sdd/draft-` must not appear in `factory.md`.
- Keep the pinned literals in `.claude/software-factory.md` (test `docs_describe_the_new_design`), among them `every job's folder` and `Acceptance criteria`.
- A repo hook blocks any edit that contains the call-with-parentheses spelling of the unwrap shortcut. Do not write it in any file.
- The `/factory` command stays thin: it names steps and gates, it does not copy the skills' text.

## Review Focus

- A single question and a single "yes" must no longer satisfy the interview: the text needs a per-topic gate and a `## Decisions` section (Task 1, pinned in Task 3).
- The spec and plan commits must name a pathspec so files already staged by anything else stay out (Task 1, pinned in Task 3).
- Plan headers and `Next plan:` lines must use repo-relative paths so a committed series works in any checkout (Task 1).
- A user request for changes after the spec commit must have a rule: edit, self-review, commit again, ask again (Task 1).
- The branch caveat (job branches start from `main`) must be stated once in the command and once in the docs (Tasks 1 and 2).

---

### Task 1: Command text

**Files:**
- Modify: `.claude/commands/factory.md`

Each step below is an exact replacement inside the `## \`<feature>\`: new job` section. Replace only the quoted old text.

- [ ] **Step 1: Task list and branch caveat (end of the intro paragraph)**

Old text:

```
There is one copy of each, and both are committed for future reference. Nothing is drafted in a gitignored folder.
```

New text:

```
There is one copy of each, and both are committed for future reference. Nothing is drafted in a gitignored folder. Keep a visible task list (TaskCreate and TaskUpdate) for steps 1 to 6 and update it at each milestone. The spec and the plans are committed on the branch this session is on. Job branches start from `main`, and the driver copies the spec and the plans into the job folders, so the jobs run either way. Tell the user which branch holds the docs, and that they reach `main` only when that branch merges.
```

- [ ] **Step 2: Coverage gate (step 1, the question bullets)**

Old text:

```
   - The questions must cover, and the spec must record the answers to: purpose and who benefits, constraints, success checks and how each is measured, edge cases and failure behavior, security and compatibility rules that must not weaken, scope against related tickets and branches, non-goals, and the alternatives that were rejected and why.
```

New text:

```
   - The questions must cover these topics: purpose and who benefits, constraints, success checks and how each is measured, edge cases and failure behavior, security and compatibility rules that must not weaken, scope against related tickets and branches, non-goals, and the alternatives that were rejected and why.
   - Coverage gate: ask at least one question per topic, or have the user say that the topic does not apply. Do not write the spec until every topic has an answer or an `n/a: <reason>` line from the user. The spec has a `## Decisions` section that lists each topic with the question asked and the answer, or the `n/a: <reason>` line.
```

- [ ] **Step 3: Spec shape (step 1, the save bullet)**

Old text:

```
and no `TBD`, `TODO` or `FIXME`, because the driver refuses a spec without them. Add Decisions, Behavior that must not change, Design, Tests and measurement and Non-goals sections as the topic needs.
```

New text:

```
and no `TBD`, `TODO`, `FIXME` or `???` outside backticks and code fences, because the driver refuses a spec without them. Headings must be exact level-2 lines such as `## Goal`. The spec also has `## Decisions` and `## Non-goals`, and Behavior that must not change, Design and Tests and measurement sections as the topic needs.
```

- [ ] **Step 4: Pathspec commit and the revision rule (step 1, last bullet)**

Old text:

```
Commit the spec through the `dev-workflow-skills:1-git-commit` skill, add the explicit path only, then show the spec to the user and wait for an explicit approval. Do not write any plan before that approval.
```

New text:

```
Stage the spec path, check that `git diff --cached --name-only` lists only that path, and commit through the `dev-workflow-skills:1-git-commit` skill with a pathspec (`git commit --only -- <spec path>`), so files staged by anything else stay out. Then show the spec to the user and wait for an explicit approval. If the user asks for changes, edit the spec, run the self-review again, commit it the same way and ask for approval again. Do not write any plan before that approval.
```

- [ ] **Step 5: Repo-relative spec path in the plan header (step 2)**

Old text:

```
the absolute path of the spec file, Global Constraints
```

New text:

```
the repo-relative path of the spec file (`docs/superpowers/specs/<date>-<slug>-design.md`), Global Constraints
```

- [ ] **Step 6: Repo-relative `Next plan:` (step 2, last bullet)**

Old text:

```
the last line of the file is `Next plan: <absolute path of the next plan file>`, or exactly `Next plan: none` in the last plan
```

New text:

```
the last line of the file is `Next plan: docs/superpowers/plans/<date>-<slug>/plan-NN-<slug>.md` (the repo-relative path of the next plan file), or exactly `Next plan: none` in the last plan
```

- [ ] **Step 7: Pathspec commit and the revision rule for plans (step 3)**

Old text:

```
After that approval commit the plans through the `dev-workflow-skills:1-git-commit` skill, adding the explicit paths only.
```

New text:

```
After that approval stage the plan paths, check that `git diff --cached --name-only` lists only them, and commit through the `dev-workflow-skills:1-git-commit` skill with a pathspec (`git commit --only -- <plan paths>`). If the user asks for plan changes, edit the plans, commit them the same way and ask for approval again.
```

- [ ] **Step 8: Check the edits**

Run:

```bash
grep -c "Coverage gate" .claude/commands/factory.md
grep -c "git commit --only -- <spec path>" .claude/commands/factory.md
grep -c "git commit --only -- <plan paths>" .claude/commands/factory.md
grep -c "repo-relative" .claude/commands/factory.md
grep -c "absolute path" .claude/commands/factory.md
grep -c "staging" .claude/commands/factory.md
```
Expected: `1`, `1`, `1`, at least `2`, `0`, `0`.

---

### Task 2: Docs sentences

**Files:**
- Modify: `.claude/software-factory.md`

- [ ] **Step 1: Spec check rules and path form**

Old text (one sentence in the `factory new --plans` bullet):

```
and no `TBD`, `TODO`, `FIXME` or `???` outside code, else `new` creates no job.
```

New text:

```
and no `TBD`, `TODO`, `FIXME` or `???` outside code, else `new` creates no job. A section ends only at a level-1 or level-2 heading outside code fences, and only an exact `## Goal` line names the Goal. `--spec` needs `--plans`. Paths in a committed series are repo-relative, and so are the `Next plan:` lines.
```

- [ ] **Step 2: Interview gate and branch caveat (the paragraph that starts with `factory new` validates the spec)**

Old text:

```
and may use read-only exploration subagents for the code survey.
```

New text:

```
and may use read-only exploration subagents for the code survey. The spec must carry a `## Decisions` section with one answer or `n/a: <reason>` per interview topic. The spec and the plans are committed on the session's current branch, with a pathspec commit. Job branches start from `main`, so the docs reach `main` only when that branch merges. The jobs themselves run from the copies in the job folders.
```

- [ ] **Step 3: Check**

Run:

```bash
grep -c "repo-relative" .claude/software-factory.md
grep -c "reach \`main\` only when that branch merges" .claude/software-factory.md
```
Expected: `1` and `1`.

---

### Task 3: Lint pins, full verification and commits

**Files:**
- Modify: `tools/factory/crates/factory-cli/tests/agent_lint.rs`

**Interfaces:**
- Consumes: the exact phrases written in Task 1 and Task 2.

- [ ] **Step 1: Tighten the `--spec` pin and add the new pins**

In `command_is_thin_and_pins_the_shapes`, replace the line

```rust
            "--spec",
```

with

```rust
            "--plans docs/superpowers/plans/<date>-<slug> --spec docs/superpowers/specs/<date>-<slug>-design.md",
```

and replace the line

```rust
            "dev-workflow-skills:1-git-commit",
```

with

```rust
            "dev-workflow-skills:1-git-commit",
            "git commit --only -- <spec path>",
            "git commit --only -- <plan paths>",
            "Coverage gate",
            "n/a: <reason>",
            "## Decisions",
            "ask for approval again",
            "repo-relative",
            "Keep a visible task list",
            "only when that branch merges",
```

- [ ] **Step 2: Drop the brittle forbid**

In the `forbid_all` list of the same test, delete the line `"staging folder",` and the comment line above the two entries so it reads:

```rust
            "SendMessage",
            // Nothing is drafted in a gitignored folder any more.
            ".superpowers/sdd/draft-",
        ],
```

- [ ] **Step 3: Pin the docs**

In `docs_describe_the_new_design`, add these literals to its `require_all` list for `.claude/software-factory.md`:

```rust
            "only an exact `## Goal` line",
            "`--spec` needs `--plans`",
            "n/a: <reason>",
```

- [ ] **Step 4: Run the lint test**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli --test agent_lint`
Expected: all pass. If a pin fails, the phrase in Task 1 or 2 differs from the pin by a character. Fix the pin to match the text you wrote, not the other way round, unless the text deviates from this plan.

- [ ] **Step 5: Run the whole factory suite and clippy**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine -p factory-cli`
Expected: all pass.

Run: `cargo clippy -j4 --manifest-path tools/factory/Cargo.toml --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 6: Rebuild the binary**

Run: `cargo build -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli`
Expected: finishes without errors.

- [ ] **Step 7: Commit**

Stage the three files, check the staged list, and commit through the `dev-workflow-skills:1-git-commit` skill with a pathspec:

```bash
git add .claude/commands/factory.md .claude/software-factory.md tools/factory/crates/factory-cli/tests/agent_lint.rs
git diff --cached --name-only
```
Expected: exactly those three paths. Commit message: `feat(factory): force the full brainstorming interview`. Then stage and commit this plans folder the same way with `docs(factory): add spec gate fix plans`.

Next plan: none
