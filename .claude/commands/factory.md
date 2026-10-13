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

The driver validates the plans when it creates the job, so the plans must exist before `<factory> new`. Draft them in a staging folder `.superpowers/sdd/draft-<slug>/` (gitignored), where `<slug>` is a short kebab-case name you pick from the feature.

1. Invoke `superpowers:brainstorming` yourself, with the user, in this session. You may ask the user questions. Save the resulting spec as `spec.md` in the staging folder. Use no subagents.
2. Invoke `superpowers:writing-plans` yourself. Save every plan in the staging folder as `plan-01-<slug>.md`, `plan-02-<slug>.md` and so on, not under `docs/`. Rules for each plan:
   - at most 3 tasks, each ending with an independently testable deliverable, each task heading written as `### Task N: <title>`
   - the header says the plan is executed by a `factory-builder` subagent through `superpowers:executing-plans`, skipping that skill's final whole-branch review, ledger scripts and progress file
   - every task that touches yml, collection or environment data, auth, variable resolution or the crates named in `CLAUDE.md` starts with the line to read `docs/superpowers/specs/opencollection-spec-reference.md`
   - the last line of the file is `Next plan: <absolute path of the next plan file>`, or exactly `Next plan: none` in the last plan
3. Always validate before creating the job: every plan has at most 3 tasks (count the `### Task` headings), each plan's last line starts with `Next plan:`, the chain of paths leads from `plan-01-<slug>.md` to the final `Next plan: none`, every plan file in the folder is on that chain, and `spec.md` exists. If something is wrong, fix the plan with the user. Do not run an invalid series.
4. Run `<factory> new <slug> '<feature>' --plans <staging folder>`, adding `--model opus` when `--opus` was given. Escape the feature text as above, so quotes, `$`, backticks and newlines stay literal. Its output is `created job <id> in <folder>`. Read the job id and the folder name from it. Then copy `spec.md` to `.superpowers/sdd/<folder>/spec.md`.
5. Run `<factory> run <id> --series` as a Bash command with `run_in_background` set to true. Then tell the user in one line that the factory is running and stop. Do not poll, do not sleep, do not read logs. The harness notifies you when the command exits.
6. When the notification arrives, run `<factory> status` once and report the line for this job. If it is `needs-human`, add that the user can run `/factory approve <id>` or `/factory rework <id> <note>`.

The driver never asks questions during a run, so the plans must be complete before step 5. A blocker found by the builder moves the job to `needs-human`.
