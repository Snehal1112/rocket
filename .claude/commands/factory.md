---
description: Run Rocket's software factory (spec, build, review, approve, merge)
argument-hint: "<feature> | next | approve <job-id> | rework <job-id> <note> | status"
---

# Factory

Arguments: $ARGUMENTS

You are the orchestrator. The design is in `.claude/software-factory.md`. Use `python3 .claude/factory/factory-ctl` (called `ctl` below) for every board, ledger and checkbox change. Never edit `board.json` or tick plan checkboxes by hand. Do not do the agents' work yourself.

Job files live in `.superpowers/sdd/<date>-<id>-<slug>/` in the main checkout. Use absolute paths when you talk to agents. Job ids are the 3 digits in the folder name.

The main checkout is the output of `ctl root`. Call it `<main>` below. Run every git command as `git -C "<main>" ...`, never in your own working directory. Commits made by the builder live in the shared object store, so diffs never need the builder worktree.

Pick the subcommand by exact shape only: `status` alone, `next` alone, `approve <digits>`, `rework <digits> <note>`. Anything else is a feature text. For example, "next button for pagination" is a feature.

## status

Run `ctl status` and show the result.

## `<feature>`: new job

1. `ctl new-job "<feature>"`. It prints the folder name and creates the job in stage `spec`.
2. Dispatch the `factory-spec-writer` agent with the feature text and the absolute job directory.
3. Check that `spec.md` and `plan.md` exist and that `plan.md` has 1 to 3 lines starting with `- [ ] Task`. If not, `ctl set-stage <id> needs-human --reason "bad plan"` and stop.
4. `ctl ledger <id> "Ruling: plan accepted, <n> tasks"` and `ctl set-stage <id> build`. Report the job and tell the user to run `/factory next`.

## next

Take the oldest job whose stage is not `merged` or `needs-human` (see `ctl status`). Do one stage, then report.

### stage `spec`

This is the stage spec case: the job was created but the spec writer did not finish. Rerun steps 2 to 4 of the new job flow: dispatch `factory-spec-writer` with the feature text from the board and the job directory, validate `spec.md` and `plan.md`, write the ledger ruling, then `ctl set-stage <id> build`.

### stage `build`

1. First run (no builder id on the board): dispatch `factory-builder` with the absolute job directory, `plan.md` path and the job branch name. Right after the dispatch returns, run `ctl set-stage <id> build --builder-id <agent id>` before you wait on any output. Resume (builder id set): use SendMessage to that builder with the path of `changes-r<round>.md` (the current round, already incremented by the review stage) or `rework-note.md`. If SendMessage to the recorded builder fails, dispatch a NEW `factory-builder` with a prompt saying: resume branch `<branch>`, the old worktree is `<worktree>`, fix `<findings file>`. Then record its id as above.
2. `ctl ledger <id> "Task <n>: dispatched (BASE <sha>, opus, agent <id>)"` for the first run.
3. Read the last four lines of the reply: `WORKTREE:`, `BRANCH:`, `BASE:`, `HEAD:`. If any is missing, `ctl set-stage <id> needs-human --reason "builder gave no handoff lines"` and stop. If `HEAD` equals `BASE`, `ctl set-stage <id> needs-human --reason "builder made no commits"` and stop.
4. `ctl set-stage <id> review --worktree <path> --builder-id <agent id> --head <sha> --reset-reviews`, and add `--base <sha>` (from the BASE line) ONLY when `base` is still null in `board.json` (the first run). On any resume or builder fallback keep the board's existing base and ignore the BASE line, so the approve diff always covers the whole job. Then `ctl ledger <id> "Head round <round>: <sha>"` with the new head sha.

### stage `review`

1. Write the diff from the main checkout. The current round's `Head round` line is already in the job's `progress.md` (build step 4 wrote it). If the ledger has only one `Head round` line, the diff is `git -C "<main>" diff <base>..<head>` with the builder's BASE from the board. Otherwise the previous head is the sha on the SECOND-TO-LAST `Head round` line, and the diff is `git -C "<main>" diff <previous head>..<head>`. Do not compute the previous head from the round number, because a rework repeats round numbers. Save it as `review-<base7>..<head7>.diff` in the job directory.
2. UX and UI reviewers run only if the diff has a line starting `diff --git a/src/`. Otherwise `ctl set-stage <id> review --review ux=skipped --review ui=skipped`.
3. Mark the reviewers you dispatch with `--review <role>=running`, then dispatch the reviewers that step 2 did not skip, in one message, in parallel: `factory-security-reviewer`, `factory-ux-reviewer`, `factory-ui-reviewer`, `factory-code-reviewer`. Give each the diff path, `spec.md` path and worktree path. Reviewers never run cargo or yarn.
4. Save each full reply as `review-<role>-r<round>.md`. The verdict is the last line. A reply with no `VERDICT:` line, or any line other than the two allowed values, counts as CHANGES with the finding "no VERDICT line". Never treat it as a pass.
5. Record each result with `ctl set-stage <id> review --review <role>=pass` or `=changes`.
6. All pass (skipped counts as pass): `ctl set-stage <id> approve`.
7. Any CHANGES and round is below 2: combine the findings into `changes-r<round+1>.md` (the new round number), then `ctl set-stage <id> build --round <round+1>`.
8. Any CHANGES and round is already 2: `ctl set-stage <id> needs-human --reason "changes remain after round 2"`.

### stage `approve`

Write the full diff `git -C "<main>" diff <base>..<final head>` as `review-<base7>..<head7>.diff` in the job directory. Dispatch `factory-approver` with the job directory, that diff path and all review files. Save the reply as `approval.md`. The last line must be `DECISION: APPROVE` or `DECISION: ESCALATE`. Anything else counts as ESCALATE.

- `DECISION: APPROVE`: run the merge step.
- `DECISION: ESCALATE`: `ctl set-stage <id> needs-human --reason "<first line of the approver summary>"`.

## approve `<job-id>`

Only for a job in `needs-human` or `approve`. Run the merge step. It also works after the user merged the branch by hand.

## rework `<job-id>` `<note>`

Only for a job in `needs-human`. Save the note as `rework-note.md` in the job directory. If the job has no `plan.md`, `ctl set-stage <id> spec` and rerun the spec writer with the note, then continue as in the `spec` stage. Otherwise `ctl set-stage <id> build` (the round is unchanged), then do the `build` stage as a resume.

## Merge step

1. `ctl is-merged <id>`. Exit 0 with `merged <sha>` means the user already merged it: skip to step 5 with that sha. Output `not merged` means continue to step 2. Any other output (the "no commits" or "no recorded head" error, printed on stderr) means `ctl set-stage <id> needs-human --reason "<the error text>"`, then stop and tell the user.
2. The main checkout must be on `main` with no tracked changes (`git -C "<main>" branch --show-current`, `git -C "<main>" status --porcelain --untracked-files=no`). If not, `ctl set-stage <id> needs-human --reason "main busy"` and stop. Tell the user they can merge by hand and run `/factory approve <id>` again.
3. Main must not have moved: `git -C "<main>" merge-base --is-ancestor HEAD <job head>` must succeed, so main HEAD is the job's base or an ancestor of the job head. If it fails, `ctl set-stage <id> needs-human --reason "main busy (main moved)"` and stop, with the same advice to the user.
4. `git -C "<main>" merge --no-ff <branch> -m "chore(factory): merge job <id> <slug>"`. On a conflict run `git -C "<main>" merge --abort`, then `ctl set-stage <id> needs-human --reason "merge conflict"` and stop. Never push. Never rebase or rewrite history.
5. `ctl tick-plan <id> --merge <sha of main HEAD>`, then `ctl set-stage <id> merged`.
