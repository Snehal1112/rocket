---
description: Run Rocket's software factory (spec, build, review, approve, merge)
argument-hint: "<feature> | next | approve <job-id> | rework <job-id> <note> | status"
---

# Factory

Arguments: $ARGUMENTS

You are the orchestrator. The design is in `.claude/software-factory.md`. Use `python3 .claude/factory/factory-ctl` (called `ctl` below) for every board, ledger and checkbox change. Never edit `board.json` or tick plan checkboxes by hand. Do not do the agents' work yourself.

Job files live in `.superpowers/sdd/<date>-<id>-<slug>/` in the main checkout. Use absolute paths when you talk to agents. Job ids are the 3 digits in the folder name.

Pick the subcommand from the first word of the arguments: `status`, `next`, `approve`, `rework`. Anything else is a feature text.

## status

Run `ctl status` and show the result.

## `<feature>`: new job

1. `ctl new-job "<feature>"`. It prints the folder name and creates the job in stage `spec`.
2. Dispatch the `factory-spec-writer` agent with the feature text and the absolute job directory.
3. Check that `spec.md` and `plan.md` exist and that `plan.md` has 1 to 3 lines starting with `- [ ] Task`. If not, `ctl set-stage <id> needs-human --reason "bad plan"` and stop.
4. `ctl ledger <id> "Ruling: plan accepted, <n> tasks"` and `ctl set-stage <id> build`. Report the job and tell the user to run `/factory next`.

## next

Take the oldest job whose stage is not `merged` or `needs-human` (see `ctl status`). Do one stage, then report.

### stage `build`

1. First run (no builder id on the board): dispatch `factory-builder` with the absolute job directory, `plan.md` path and the job branch name. Resume (builder id set): use SendMessage to that builder with the path of `changes-r<round>.md` or `rework-note.md`.
2. `ctl ledger <id> "Task <n>: dispatched (BASE <sha>, opus, agent <id>)"` for the first run.
3. Read the last four lines of the reply: `WORKTREE:`, `BRANCH:`, `BASE:`, `HEAD:`. If any is missing, `ctl set-stage <id> needs-human --reason "builder gave no handoff lines"` and stop.
4. `ctl set-stage <id> review --worktree <path> --builder-id <agent id> --base <sha> --head <sha> --reset-reviews`.

### stage `review`

1. Write the diff: for round 0 use `git -C <worktree> diff <base>..<head>`, for later rounds only the fix commits since the previous head. Save it as `review-<base7>..<head7>.diff` in the job directory.
2. UX and UI reviewers run only if the diff has a line starting `diff --git a/src/`. Otherwise `ctl set-stage <id> review --review ux=skipped --review ui=skipped`.
3. Mark the reviewers you dispatch with `--review <role>=running`, then dispatch them in one message, in parallel: `factory-security-reviewer`, `factory-ux-reviewer`, `factory-ui-reviewer`, `factory-code-reviewer`. Give each the diff path, `spec.md` path and worktree path. Reviewers never run cargo or yarn.
4. Save each full reply as `review-<role>-r<round>.md`. The verdict is the last line. A reply with no `VERDICT:` line, or any line other than the two allowed values, counts as CHANGES with the finding "no VERDICT line". Never treat it as a pass.
5. Record each result with `ctl set-stage <id> review --review <role>=pass` or `=changes`.
6. All pass (skipped counts as pass): `ctl set-stage <id> approve`.
7. Any CHANGES and round is below 2: combine the findings into `changes-r<round>.md`, then `ctl set-stage <id> build --round <round+1>`.
8. Any CHANGES and round is already 2: `ctl set-stage <id> needs-human --reason "changes remain after round 2"`.

### stage `approve`

Dispatch `factory-approver` with the job directory, the full diff and all review files. Save the reply as `approval.md`. The last line must be `DECISION: APPROVE` or `DECISION: ESCALATE`. Anything else counts as ESCALATE.

- `DECISION: APPROVE`: run the merge step.
- `DECISION: ESCALATE`: `ctl set-stage <id> needs-human --reason "<first line of the approver summary>"`.

## approve `<job-id>`

Only for a job in `needs-human` or `approve`. Run the merge step. It also works after the user merged the branch by hand.

## rework `<job-id>` `<note>`

Only for a job in `needs-human`. Save the note as `rework-note.md` in the job directory, `ctl set-stage <id> build` (the round is unchanged), then do the `build` stage as a resume.

## Merge step

1. `ctl is-merged <id>`. If it prints `merged <sha>`, the user already merged it: skip to step 4 with that sha. If the job has no commits or no recorded head, stop and tell the user.
2. The main checkout must be on `main` with no tracked changes (`git branch --show-current`, `git status --porcelain --untracked-files=no`). If not, `ctl set-stage <id> needs-human --reason "main busy"` and stop. Tell the user they can merge by hand and run `/factory approve <id>` again.
3. `git merge --no-ff <branch> -m "chore(factory): merge job <id> <slug>"`. On a conflict run `git merge --abort`, then `ctl set-stage <id> needs-human --reason "merge conflict"` and stop. Never push. Never rebase or rewrite history.
4. `ctl tick-plan <id> --merge <sha of main HEAD>`, then `ctl set-stage <id> merged`.
