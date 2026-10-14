---
description: Run Rocket's software factory (brainstorm, plan, then the Rust driver builds, checks, reviews and merges)
argument-hint: "<feature> [--opus] | status | approve <id> | rework <id> <note> | cancel <id> | cleanup <id>"
---

# Factory

Arguments: $ARGUMENTS

You are a thin front end. The design is in `.claude/software-factory.md`. The Rust driver does all mechanical work. Never edit the board, the ledger, the events log or any plan checkbox, and do not do the builder's or reviewer's work.

`<factory>` means `tools/factory/target/debug/factory`, run from the current checkout. If that file does not exist, run `cargo build -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli` once, then continue.

## Pick the subcommand

Match by exact shape only, on the whole argument string:

- `status` alone
- `approve <digits>` (nothing after the digits)
- `cancel <digits>` (nothing after the digits)
- `cleanup <digits>` (nothing after the digits)
- `rework <digits> <note>` with a non-empty note

Anything else is a feature text, for example `next button for pagination`, `status page for jobs`, `approve button for forms` or `cancel dialog 12 layout`. A feature may start or end with the single token `--opus`. Remove that token from the text and pass `--model opus` to `<factory> new`. If the argument is `rework <digits>` with no note, print `rework needs a note: /factory rework <id> <note>` and stop without calling `<factory>`. If the argument is empty, print the argument hint and stop.

## Other subcommands

Each is one call and a one-line report of the output, nothing more:

- `status`: `<factory> status`
- `approve <id>`: `<factory> approve <id>`
- `rework <id> <note>`: `<factory> rework <id> '<note>'` (the driver accepts a one-line note of at most 200 characters and refuses anything else)
- `cancel <id>`: `<factory> cancel <id>`
- `cleanup <id>`: `<factory> cleanup <id>`

Always escape the note when you put it in the shell: wrap text in single quotes and write an embedded single quote as `'\''`.

## `<feature>`: new job

The driver validates the spec and the plans when it creates the job, so both must exist, and be committed, before `<factory> new`. Pick a short kebab-case `<slug>` from the feature and a date `<date>` (YYYY-MM-DD). The spec is `docs/superpowers/specs/<date>-<slug>-design.md`. The plans are in `docs/superpowers/plans/<date>-<slug>/` as `plan-01-<slug>.md`, `plan-02-<slug>.md` and so on. There is one copy of each, and both are committed for future reference. Nothing is drafted in a gitignored folder.

1. Invoke `superpowers:brainstorming` yourself, with the user, in this session, and follow it fully. Always take its full architectural path: never classify the feature as a spike or a bounded change, because both skip the spec and the plans that the driver needs. Announce the classification before the first question. Spec quality comes from the interview, so do not shorten it:
   - Read the code, the docs and the open tickets and branches that touch the area first. You may use read-only exploration subagents for the code survey. Do not use subagents for the interview, the design or the writing, and do not do the builder's or reviewer's work.
   - Ask clarifying questions one at a time, as many as it takes, until the user confirms your written-back understanding of the intended outcome. A ticket that looks clear is not a reason to skip questions.
   - The questions must cover, and the spec must record the answers to: purpose and who benefits, constraints, success checks and how each is measured, edge cases and failure behavior, security and compatibility rules that must not weaken, scope against related tickets and branches, non-goals, and the alternatives that were rejected and why.
   - Propose 2 to 3 approaches with trade-offs and a recommendation. Present the design in sections and get the user's approval after each section.
   - Save the spec as `docs/superpowers/specs/<date>-<slug>-design.md`. It must have a `## Goal` section of at least 8 words and a `## Acceptance criteria` section with one bullet per check, and no `TBD`, `TODO` or `FIXME`, because the driver refuses a spec without them. Add Decisions, Behavior that must not change, Design, Tests and measurement and Non-goals sections as the topic needs.
   - Run the skill's spec self-review (placeholders, contradictions, scope, ambiguity) and fix what it finds. Commit the spec through the `dev-workflow-skills:1-git-commit` skill, add the explicit path only, then show the spec to the user and wait for an explicit approval. Do not write any plan before that approval.
2. Invoke `superpowers:writing-plans` yourself. Save every plan in `docs/superpowers/plans/<date>-<slug>/`. Rules for each plan:
   - the header has Goal, Architecture, Tech Stack, the absolute path of the spec file, Global Constraints and Review Focus, as `superpowers:writing-plans` describes
   - no commit steps: the driver commits, so a plan that still has them is harmless but wasteful, because the builder is told to skip every commit step
   - at most 3 tasks, each ending with an independently testable deliverable, each task heading written as `### Task N: <title>`
   - each task lists exact files and its steps are 2 to 5 minute actions with exact code and exact commands; no placeholders such as TBD, "add error handling" or "similar to Task N"
   - the header also says the plan is executed by a `factory-builder` subagent through `superpowers:executing-plans`, skipping that skill's final whole-branch review, ledger scripts and progress file
   - every task that touches yml, collection or environment data, auth, variable resolution or the crates named in `CLAUDE.md` starts with the line to read `docs/superpowers/specs/opencollection-spec-reference.md`
   - every acceptance criterion of the spec is covered by a task, and the Review Focus lines name the tests that pin them
   - the last line of the file is `Next plan: <absolute path of the next plan file>`, or exactly `Next plan: none` in the last plan
3. Always validate before creating the job: every plan has at most 3 tasks (count the `### Task` headings), each plan's last line starts with `Next plan:`, the chain of paths leads from `plan-01-<slug>.md` to the final `Next plan: none`, every plan file in the folder is on that chain, and the spec file exists with a non-empty `## Goal` and `## Acceptance criteria` and no placeholder. If something is wrong, fix the plan with the user. Do not run an invalid series. Then list the plans for the user and wait for an explicit approval of the plans. After that approval commit the plans through the `dev-workflow-skills:1-git-commit` skill, adding the explicit paths only. Do not run `<factory> new` before that approval.
4. Run `<factory> new <slug> '<feature>' --plans docs/superpowers/plans/<date>-<slug> --spec docs/superpowers/specs/<date>-<slug>-design.md`, adding `--model opus` when `--opus` was given. Escape the feature text as above, so quotes, `$`, backticks and newlines stay literal. Its output is `created job <id> in <folder>`. Read the job id and the folder name from it. The driver checks the spec and copies it into every job's folder.
5. Run `<factory> run <id> --series` as a Bash command with `run_in_background` set to true. Then tell the user in one line that the factory is running and stop. Do not poll, do not sleep, do not read logs. The harness notifies you when the command exits.
6. When the notification arrives, run `<factory> status` once and report the line for this job. If it is `needs-human`, add that the user can run `/factory approve <id>` or `/factory rework <id> <note>`.

The driver never asks questions during a run, so the plans must be complete before step 5. A blocker found by the builder moves the job to `needs-human`.
