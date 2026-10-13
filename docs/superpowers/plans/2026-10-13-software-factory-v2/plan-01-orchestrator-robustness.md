# Software Factory v2 Plan 01: Orchestrator Robustness

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. This plan is executed by a `factory-builder` subagent. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close gaps A1 to A10 of the v2 spec: make `factory-ctl` and the `/factory` command safe against crashes, stale ranges, unreviewed commits, skipped reviewers, mistyped stages and id collisions.

**Architecture:** `factory-ctl` (Python stdlib) gains `show`, `diff-range`, `clear-builder`, a `prev_head` board field, a stage-move guard with `--force`, and ids taken from the board, the job folders and the `factory/NNN-*` branches. The `/factory` command (a prompt file) is edited to use them. Tests extend the existing bash suite. The design doc is synced in the task whose change needs it.

**Tech Stack:** Python 3 stdlib, bash plus `jq` in tests, markdown command and doc files. No Rust is touched.

**Spec:** [docs/superpowers/specs/2026-10-13-software-factory-v2-design.md](../../specs/2026-10-13-software-factory-v2-design.md), section A and the "Interfaces between plans" row for plan 01. Design doc: [.claude/software-factory.md](../../../../.claude/software-factory.md).

## Global Constraints

- One plan at a time, one builder at a time. No parallel builders in one worktree.
- Ledger writes go through `ctl` only. Builders and reviewers never write the ledger.
- Docs stay under `.claude/`. `.claude/software-factory.md` is updated by this plan, `CLAUDE.md` stays small.
- Plan 01 produces exactly these interface names: `ctl show <id>` (JSON of one job), `ctl diff-range <id>` (prints `<from>..<to>` for the current round), board field `prev_head`, `ctl clear-builder <id>`, `ctl set-stage` rejects backward moves unless `--force`, ids come from the max of board, job directories and `factory/NNN-*` branches, the command merges `<head sha>` and not the branch.
- All `cargo` commands pass `-j4`. No `cargo test --workspace` or `--all`. This plan runs no cargo command.
- Commits use a pathspec: `git add <paths> && git commit --only -m "..." -- <paths>`. Write the message with the skill `dev-workflow-skills:1-git-commit`, conventional format. Never `git add -A`, `--all` or `.`. Never set `HARNESS_GATE=0`. Never push.
- Code comments are short full sentences that end in a period. No emojis.
- Follow the existing style: `factory-ctl` uses one `cmd_<name>(args)` function per subcommand and `die()` for errors. Tests use `check "<description>" <command...>` in `.claude/factory/tests/run.sh`.

## Review Focus

- A resume where the builder made no new commit must never reach review or approval (empty diff approved). Tested in Task 1 (ctl refuses) and pinned in Task 2 (command text).
- A lost board, an orphan job folder or an existing `factory/NNN-*` branch must never produce a duplicate id. Tested in Task 1.
- A mistyped or stale `set-stage` must not move a job backward or reopen a merged job. Tested in Task 1.
- Commits added to the branch after review must never merge unreviewed. Pinned by the merge text and its contract test in Task 2.
- A UX or UI reviewer whose last verdict was CHANGES must never be skipped because the fix diff has no `src/` change. Pinned by the review text and its contract test in Task 2.
- A crash in the middle of a builder dispatch must never lead to a second live builder. Pinned by the `dispatching` marker text and its contract test in Task 2.

---

### Task 1: ctl features

**Files:**
- Modify: `.claude/factory/factory-ctl`
- Modify: `.claude/factory/tests/run.sh`
- Modify: `.claude/software-factory.md` (new "Board helper" section)

**Interfaces:**
- Consumes: the existing `load_board`, `save_board`, `find_job`, `git`, `die`, `STAGES`, `cmd_set_stage`, `cmd_new_job`, `cmd_status`, `build_parser` in `factory-ctl`.
- Produces: `next_job_number(root, board) -> int`, `check_stage_move(old, new, force)`, `cmd_show`, `cmd_diff_range`, `cmd_clear_builder`; CLI `ctl show <id>`, `ctl diff-range <id>`, `ctl clear-builder <id>`, `ctl set-stage ... --force`; board field `prev_head` (sha or null); `set-stage <id> review --head <sha>` fails with the text `head unchanged` when the sha equals the recorded head.

- [ ] **Step 1: Write the failing tests**

In `.claude/factory/tests/run.sh`, find these two lines:

```bash
check "ctl: root prints FACTORY_ROOT when set" test "$(python3 "$ctl" root)" = "$fr"
unset FACTORY_ROOT
```

Insert the block below between them (after the `root` check, before `unset FACTORY_ROOT`). At that point the board holds jobs 001 (approve), 002 (approve) and 003 (spec).

```bash

# ---- ctl: show, prev_head, diff-range, clear-builder, stage guard, ids ----
check "ctl: show prints one job as JSON" \
  bash -c "python3 '$ctl' show 3 | jq -e '.id == \"003\" and .prev_head == null'"
check "ctl: show rejects an unknown id" bash -c "! python3 '$ctl' show 99"
python3 "$ctl" set-stage 3 build --worktree /w3 --builder-id agent-3 >/dev/null
python3 "$ctl" clear-builder 3 >/dev/null
check "ctl: clear-builder nulls builder_id and worktree and keeps the stage" \
  jq -e '.jobs[] | select(.id == "003") | select(.builder_id == null and .worktree == null and .stage == "build")' "$board"
check "ctl: diff-range fails before a head is recorded" bash -c "! python3 '$ctl' diff-range 3"
python3 "$ctl" set-stage 3 review --base bbb222 --head aaa111 --reset-reviews >/dev/null
check "ctl: diff-range is base..head on the first round" test "$(python3 "$ctl" diff-range 3)" = "bbb222..aaa111"
python3 "$ctl" set-stage 3 build --round 1 >/dev/null
check "ctl: review back to build is allowed" jq -e '.jobs[] | select(.id == "003") | select(.stage == "build")' "$board"
python3 "$ctl" set-stage 3 review --head ccc333 >/dev/null
check "ctl: a new head stores the old head as prev_head" \
  jq -e '.jobs[] | select(.id == "003") | select(.prev_head == "aaa111" and .head == "ccc333")' "$board"
check "ctl: diff-range is prev_head..head on a later round" test "$(python3 "$ctl" diff-range 3)" = "aaa111..ccc333"
python3 "$ctl" set-stage 3 build --round 2 >/dev/null
board_before="$(cat "$board")"
check "ctl: review with an unchanged head is refused" bash -c "! python3 '$ctl' set-stage 3 review --head ccc333"
check "ctl: a refused unchanged head leaves the board unchanged" test "$board_before" = "$(cat "$board")"
check "ctl: a backward stage move is refused" bash -c "! python3 '$ctl' set-stage 3 spec"
check "ctl: --force allows a backward move" python3 "$ctl" set-stage 3 spec --force
python3 "$ctl" set-stage 3 merged >/dev/null
check "ctl: a merged job cannot move without --force" bash -c "! python3 '$ctl' set-stage 3 build"
check "ctl: a merged job cannot go to needs-human" bash -c "! python3 '$ctl' set-stage 3 needs-human --reason x"
mkdir -p "$fr/.superpowers/sdd/2026-10-13-007-orphan"
git -C "$fr" branch factory/009-ghost main
name4="$(python3 "$ctl" new-job "After orphans" --date 2026-10-13)"
check "ctl: new-job id skips orphan folders and factory branches" test "$name4" = "2026-10-13-010-after-orphans"
rm -f "$board"
name5="$(python3 "$ctl" new-job "After lost board" --date 2026-10-13)"
check "ctl: new-job id survives a lost board" test "$name5" = "2026-10-13-011-after-lost-board"
check "ctl: the rebuilt board holds the new job" jq -e '.jobs | length == 1' "$board"
check "ctl: new-job starts with prev_head null" jq -e '.jobs[0].prev_head == null' "$board"
```

- [ ] **Step 2: Run the tests and see them fail**

Run: `bash .claude/factory/tests/run.sh | grep -E '^(FAIL|all passed|[0-9]+ failed)'`
Expected: FAIL lines for the new `ctl:` checks (show, clear-builder, diff-range, guard, ids), and a final `N failed`.

- [ ] **Step 3: Implement the ctl changes**

Edit `.claude/factory/factory-ctl` in these places.

3a. After the line `REVIEW_STATES = ("pending", "running", "pass", "changes", "skipped")` add:

```python
STAGE_ORDER = {"spec": 0, "build": 1, "review": 2, "approve": 3, "merged": 4}
# A CHANGES round sends a job from review back to build. That is the only allowed step back.
ALLOWED_BACK = (("review", "build"),)
```

3b. After the `git` helper function (the one that ends `return subprocess.run(["git", "-C", root, *args], capture_output=True, text=True)`) add:

```python
def next_job_number(root, board):
    """Return the next free job number from the board, job folders and factory branches."""
    numbers = [int(job["id"]) for job in board["jobs"]]
    sdd = os.path.join(root, ".superpowers", "sdd")
    if os.path.isdir(sdd):
        for entry in os.listdir(sdd):
            found = re.match(r"\d{4}-\d{2}-\d{2}-(\d{3})-", entry)
            if found:
                numbers.append(int(found.group(1)))
    refs = git(root, "for-each-ref", "--format=%(refname:short)", "refs/heads/factory/")
    if refs.returncode == 0:
        for name in refs.stdout.split():
            found = re.match(r"factory/(\d{3})-", name)
            if found:
                numbers.append(int(found.group(1)))
    return max(numbers, default=0) + 1


def check_stage_move(old, new, force):
    """Refuse a backward or post-merge stage move unless force is set."""
    if force or old == new:
        return
    if old == "merged":
        die("job is merged, refusing to move it without --force")
    if old == "needs-human" or new == "needs-human":
        return
    if (old, new) in ALLOWED_BACK:
        return
    if STAGE_ORDER[new] < STAGE_ORDER[old]:
        die(f"refusing to move job backward from {old} to {new} without --force")
```

3c. In `cmd_new_job`, replace these lines:

```python
    number = max((int(job["id"]) for job in board["jobs"]), default=0) + 1
    job_id = f"{number:03d}"
```

with:

```python
    job_id = f"{next_job_number(root, board):03d}"
```

Replace:

```python
    os.makedirs(path)
    with open(os.path.join(path, "progress.md"), "w", encoding="utf-8") as handle:
        handle.write(f"# SDD ledger — plan: .superpowers/sdd/{name}/plan.md\n\n")
```

with:

```python
    os.makedirs(path, exist_ok=True)
    ledger_path = os.path.join(path, "progress.md")
    if not os.path.exists(ledger_path):
        with open(ledger_path, "w", encoding="utf-8") as handle:
            handle.write(f"# SDD ledger — plan: .superpowers/sdd/{name}/plan.md\n\n")
```

Replace `            "head": None,\n            "stage": "spec",` with:

```python
            "head": None,
            "prev_head": None,
            "stage": "spec",
```

3d. In `cmd_set_stage`, replace the part from `reviews = [...]` down to the `for field in ...` loop with this (keep the `save_board(root, board)` line that follows):

```python
    reviews = [parse_review(text) for text in args.review]
    check_stage_move(job["stage"], args.stage, args.force)
    if args.stage == "review" and args.head is not None and args.head == job.get("head"):
        die("head unchanged, the builder made no new commits")
    if job["stage"] != args.stage:
        job["stage"] = args.stage
        job["stage_since"] = time.time()
    job["reason"] = args.reason if args.stage == "needs-human" else None
    if args.round is not None:
        job["round"] = args.round
    if args.reset_reviews:
        job["reviews"] = {role: "pending" for role in ROLES}
    for role, state in reviews:
        job["reviews"][role] = state
    if args.base is not None and job.get("base") and args.base != job["base"] and not args.force_base:
        die(f"job already has base {job['base']}, refusing to change it to {args.base} without --force-base")
    if args.head is not None and args.head != job.get("head"):
        job["prev_head"] = job.get("head")
    for field in ("worktree", "builder_id", "base", "head"):
        value = getattr(args, field)
        if value is not None:
            job[field] = value
```

3e. After `cmd_set_stage` add:

```python
def cmd_show(args):
    job = find_job(load_board(repo_root()), args.id)
    print(json.dumps(job, indent=2))


def cmd_diff_range(args):
    job = find_job(load_board(repo_root()), args.id)
    start = job.get("prev_head") or job.get("base")
    end = job.get("head")
    if not start or not end:
        die("job needs a recorded base and head, run set-stage --base and --head first")
    print(f"{start}..{end}")


def cmd_clear_builder(args):
    root = repo_root()
    board = load_board(root)
    job = find_job(board, args.id)
    job["builder_id"] = None
    job["worktree"] = None
    save_board(root, board)
```

3f. Replace the body of the `for job in board["jobs"]:` loop in `cmd_status` with:

```python
    for job in board["jobs"]:
        chips = " ".join(f"{role}:{job['reviews'][role]}" for role in ROLES)
        head = (job.get("head") or "-")[:7]
        builder = job.get("builder_id") or "-"
        line = (
            f"{job['id']}  {job['created']}  {job['stage']:<11} "
            f"round {job['round']}  {chips}  head {head}  builder {builder}  {job['slug']}"
        )
        if job.get("reason"):
            line += f"  ({job['reason']})"
        print(line)
```

3g. In `build_parser`, after `set_stage.add_argument("--reason")` add `set_stage.add_argument("--force", action="store_true")`. After the `ledger` subparser block (before `tick = ...`) add:

```python
    show = sub.add_parser("show")
    show.add_argument("id")
    show.set_defaults(func=cmd_show)

    diff_range = sub.add_parser("diff-range")
    diff_range.add_argument("id")
    diff_range.set_defaults(func=cmd_diff_range)

    clear_builder = sub.add_parser("clear-builder")
    clear_builder.add_argument("id")
    clear_builder.set_defaults(func=cmd_clear_builder)
```

- [ ] **Step 4: Run the tests and see them pass**

Run: `bash .claude/factory/tests/run.sh | grep -E '^(FAIL|all passed|[0-9]+ failed)'`
Expected: `all passed` and no FAIL line. If an older `ctl:` check fails, the stage guard is too strict: re-read `check_stage_move` against the stage moves in the existing tests.

- [ ] **Step 5: Sync the design doc**

In `.claude/software-factory.md`, insert this section directly before the line `## Commands (`.claude/commands/factory.md`)`:

```markdown
## Board helper (`.claude/factory/factory-ctl`)

- `ctl show <id>` prints one job as JSON. `ctl status` also shows head, builder id and the needs-human reason.
- `ctl diff-range <id>` prints `<from>..<to>` for the current round: `prev_head..head`, or `base..head` on the first round. `set-stage --head` stores the old head as `prev_head` in the same write, so a crash cannot leave a wrong range.
- `ctl set-stage <id> review --head <sha>` fails with `head unchanged` when the sha is the recorded head, so a resume with no new commit never reaches review.
- `ctl set-stage` refuses a backward move (for example `build` to `spec`) and any move out of `merged`, unless `--force`. Allowed without `--force`: `review` to `build`, and any move into or out of `needs-human` (not out of `merged`).
- `ctl clear-builder <id>` sets `builder_id` and `worktree` to null.
- New job ids are the highest of the board, the job folders and the `factory/NNN-*` branches, plus one. `new-job` tolerates an existing folder.

```

- [ ] **Step 6: Run the tests again and commit**

Run: `bash .claude/factory/tests/run.sh | tail -1` (expected `all passed`).

Commit: invoke the skill `dev-workflow-skills:1-git-commit` for the message. Suggested type: `feat(factory): harden factory-ctl board, ids and stage moves`. Then:

```bash
git add .claude/factory/factory-ctl .claude/factory/tests/run.sh .claude/software-factory.md && git commit --only -m "<message from the skill>" -- .claude/factory/factory-ctl .claude/factory/tests/run.sh .claude/software-factory.md
```

---

### Task 2: Command changes

**Files:**
- Modify: `.claude/commands/factory.md`
- Modify: `.claude/factory/tests/run.sh`
- Modify: `.claude/software-factory.md` (merge step, reviewer rows, rework line)

**Interfaces:**
- Consumes: from Task 1, `ctl show`, `ctl diff-range`, `ctl clear-builder`, the `head unchanged` failure of `set-stage review --head`, and `prev_head`.
- Produces: the new command text. The builder id value `dispatching` marks a dispatch in flight.

- [ ] **Step 1: Write the failing contract test**

In `.claude/factory/tests/run.sh`, after the `/factory command` word loop (the `done` that follows the `grep -qF -- "$word" "$cmd"` line), add:

```bash
for word in 'diff-range' 'clear-builder' 'ctl show' 'dispatching' 'head unchanged' \
  'newest `review-<role>-r' 'merge --no-ff <head sha>' 'branch tip differs' 'First run `ctl is-merged <id>`' 'bad plan'; do
  grep -qF -- "$word" "$cmd" && pass "command mentions: $word" || fail "command mentions: $word"
done
```

Run: `bash .claude/factory/tests/run.sh | grep -E '^(FAIL|all passed|[0-9]+ failed)'`
Expected: 10 new FAIL lines for `command mentions:`.

- [ ] **Step 2: Rewrite build stage step 1 (gap A2)**

In `.claude/commands/factory.md`, replace the whole line that starts `1. First run (no builder id on the board)` (in the `### stage \`build\`` section) with:

```text
1. Read the job with `ctl show <id>`. Pick the case by `builder_id`:
   - Null (first run): run `ctl set-stage <id> build --builder-id dispatching` BEFORE the dispatch. Then dispatch `factory-builder` with the absolute job directory, the `plan.md` path and the job branch name. Prefer `run_in_background: true`: the call returns the agent id at once, so run `ctl set-stage <id> build --builder-id <agent id>` right away and then wait for the completion notice. If a background run is not possible, dispatch in the foreground and replace the `dispatching` marker with the agent id as soon as the call returns.
   - `dispatching`: an earlier dispatch never recorded an agent id. It may have crashed or may still be running. Do NOT dispatch and do NOT send a message. Tell the user to check the dashboard for a running `factory-builder`. If none runs and the user confirms, run `ctl clear-builder <id>`, then start a first run whose prompt also says: branch `<branch>` may already hold commits, continue from them and do not redo finished tasks.
   - An agent id (resume): use SendMessage to that builder with the path of `changes-r<round>.md` (the current round, already incremented by the review stage) or `rework-note.md`. If SendMessage to the recorded builder fails, run `ctl clear-builder <id>`, set the marker `dispatching` as above, and dispatch a NEW `factory-builder` with a prompt saying: resume branch `<branch>`, the old worktree is `<worktree>`, fix `<findings file>`. Then record its id as above.
```

- [ ] **Step 3: Rewrite build stage step 4 (gaps A3, A4)**

Replace the whole line that starts `4. \`ctl set-stage <id> review --worktree <path>` with:

```text
4. `ctl set-stage <id> review --worktree <path> --builder-id <agent id> --head <sha> --reset-reviews`, and add `--base <sha>` (from the BASE line) ONLY when `base` is still null in `ctl show <id>` (the first run). On any resume or builder fallback keep the board's existing base and ignore the BASE line, so the approve diff always covers the whole job. `ctl` stores the old head as `prev_head` in the same write. If this call fails with `head unchanged`, the builder made no new commit: `ctl set-stage <id> needs-human --reason "builder made no new commits"` and stop. Never send an empty diff to review. Then `ctl ledger <id> "Head round <round>: <sha>"` with the new head sha. The ledger line is an audit record only. The review range comes from `ctl diff-range`.
```

- [ ] **Step 4: Rewrite review stage steps 1 and 2 (gaps A3, A5)**

Replace the whole line that starts `1. Write the diff from the main checkout.` with:

```text
1. Run `ctl diff-range <id>`. It prints `<from>..<to>` for this round (`prev_head..head`, or `base..head` on the first round). Write `git -C "<main>" diff <from>..<to>` to the job directory as `review-<from7>..<to7>.diff`, where `<from7>` and `<to7>` are the first 7 characters of each sha.
```

Replace the whole line that starts `2. UX and UI reviewers run only if` with:

```text
2. Decide the UX and UI reviewers one by one. A role runs if ANY of these holds: (a) the round diff has a line starting `diff --git a/src/`; (b) the newest `review-<role>-r<n>.md` in the job directory (by modification time) ends with `VERDICT: CHANGES`; (c) no `review-<role>-r<n>.md` exists yet and the full job diff `git -C "<main>" diff <base>..<head>` has a line starting `diff --git a/src/`. Otherwise record `ctl set-stage <id> review --review <role>=skipped`. A role whose last verdict was CHANGES is never skipped, because skipping would clear the CHANGES without a re-check.
```

- [ ] **Step 5: Edit the approve stage (gap A6)**

Replace the line that starts `Write the full diff` (first paragraph of `### stage \`approve\``) with:

```text
First run `ctl is-merged <id>` (the same check as merge step 1). Exit 0 with `merged <sha>` means the branch is already on main: skip the approver and go to merge step 5 with that sha. Output `not merged` means continue. Any other output means `ctl set-stage <id> needs-human --reason "<the error text>"` and stop. Only then write the full diff `git -C "<main>" diff <base>..<final head>` as `review-<base7>..<head7>.diff` in the job directory. Dispatch `factory-approver` with the job directory, that diff path and all review files. Save the reply as `approval.md`. The last line must be `DECISION: APPROVE` or `DECISION: ESCALATE`. Anything else counts as ESCALATE.
```

- [ ] **Step 6: Edit the rework section (gap A8)**

Replace the whole paragraph under `## rework \`<job-id>\` \`<note>\`` with:

```text
Only for a job in `needs-human`. Save the note as `rework-note.md` in the job directory. Read the job with `ctl show <id>`. If the job has no `plan.md`, or its `reason` is `bad plan`, the plan is the problem: `ctl set-stage <id> spec --round 0`, rerun the spec writer with the note and the line "the previous plan was rejected, write a new `plan.md`", then continue as in the `spec` stage. Otherwise `ctl set-stage <id> build` (the round is unchanged), then do the `build` stage as a resume.
```

- [ ] **Step 7: Edit the merge step (gaps A1, A6)**

In `## Merge step`, replace the whole line starting `3. Main must not have moved` with:

```text
3. Read the job with `ctl show <id>`. Call its `head` the reviewed head. The branch tip must still be the reviewed head: `git -C "<main>" rev-parse --verify <branch>` must print the same sha. If it differs or the branch is gone, `ctl set-stage <id> needs-human --reason "branch tip differs from reviewed head"` and stop, because newer commits were never reviewed. Main must not have moved: `git -C "<main>" merge-base --is-ancestor HEAD <reviewed head>` must succeed, so main HEAD is the job's base or an ancestor of the reviewed head. If it fails, `ctl set-stage <id> needs-human --reason "main busy (main moved)"` and stop, with the same advice to the user.
```

Replace the whole line starting `4. \`git -C "<main>" merge --no-ff <branch>` with:

```text
4. `git -C "<main>" merge --no-ff <head sha> -m "chore(factory): merge job <id> <slug>"`, where `<head sha>` is the reviewed head from step 3, never the branch name. On a conflict run `git -C "<main>" merge --abort`, then `ctl set-stage <id> needs-human --reason "merge conflict"` and stop. Never push. Never rebase or rewrite history.
```

- [ ] **Step 8: Sync the design doc**

In `.claude/software-factory.md`:

Replace the whole `## Merge step` list (the lines numbered `0.` to `3.`) with:

```markdown
0. If the recorded job head is already an ancestor of `main` (`factory-ctl is-merged`), the human merged it by hand. Skip the merge and go straight to step 3, so `/factory approve` can be run again after a manual merge. The orchestrator uses the head of `main` at that moment as the merge reference. `is-merged` checks the recorded job head commit, not the branch name, so it still works if the branch was deleted after a manual merge. A job with no commits is never reported as merged. `/factory next` in stage `approve` runs this check before it starts the approver.
1. Main checkout must be on `main`, clean (`git status` empty for tracked files), and `HEAD` must still be the job's base or an ancestor of the reviewed head. Otherwise the job goes to `needs-human` with the reason "main busy". The human can then merge by hand and run `/factory approve` again.
2. The branch tip must equal the reviewed head recorded on the board, otherwise `needs-human` with "branch tip differs from reviewed head". Merge the sha, not the branch: `git merge --no-ff <head sha>`. Never push, never rebase or rewrite history that is not the job's own.
3. Run `factory-ctl tick-plan`, which writes `Task N: complete (merge <sha>)` ledger lines and ticks the plan boxes, then set the board stage to merged. The worktree is kept until the user removes it.
```

Replace the table rows for `factory-ux-reviewer` and `factory-ui-reviewer`: change `Runs only when the diff touches \`src/\`.` to `Runs when the round diff touches \`src/\`, or when its last verdict was CHANGES. Never skipped over an open CHANGES.` in both rows.

Replace the bullet `- \`/factory rework <job-id> <note>\`: ... Does not reset the round counter.` with:

```markdown
- `/factory rework <job-id> <note>`: send a `needs-human` job back to the builder with the note. Does not reset the round counter. If the job was parked for "bad plan" (or has no plan), it goes back to the spec writer instead, with the round reset to 0.
```

- [ ] **Step 9: Run the tests and commit**

Run: `bash .claude/factory/tests/run.sh | grep -E '^(FAIL|all passed|[0-9]+ failed)'`
Expected: `all passed`. If a `command mentions:` check fails, fix the prose so it contains the exact phrase.

Commit: invoke the skill `dev-workflow-skills:1-git-commit` for the message. Suggested type: `feat(factory): merge the reviewed sha and harden the build and review stages`. Then:

```bash
git add .claude/commands/factory.md .claude/factory/tests/run.sh .claude/software-factory.md && git commit --only -m "<message from the skill>" -- .claude/commands/factory.md .claude/factory/tests/run.sh .claude/software-factory.md
```

---

### Task 3: Doc and command contradictions, contract test, full verification

**Files:**
- Modify: `.claude/software-factory.md`
- Modify: `.claude/factory/tests/run.sh`

**Interfaces:**
- Consumes: the command and doc text from Tasks 1 and 2.
- Produces: tests that fail when doc and command drift apart again.

- [ ] **Step 1: Write the failing tests**

In `.claude/factory/tests/run.sh`, directly after the loop added in Task 2 Step 1, add:

```bash
# Every `ctl <subcommand>` in the command must exist in factory-ctl.
for sub in $(grep -o 'ctl [a-z][a-z-]*' "$cmd" | awk '{print $2}' | sort -u); do
  check "command contract: ctl $sub is a real subcommand" python3 "$ctl" "$sub" --help
done
# The doc must agree with the command (gap A10).
doc="$repo/.claude/software-factory.md"
check "doc: no unused 'implementer done' ledger line" bash -c "! grep -qF 'implementer done' '$doc'"
check "doc: reviewers are 'skipped' in lower case like the board" bash -c "! grep -qF 'SKIPPED' '$doc'"
check "doc: manual-merge check uses the recorded head, not the branch tip" bash -c "! grep -qF 'branch tip is already' '$doc'"
check "doc: worktree path is not the invented factory-NNN name" bash -c "! grep -qF 'worktrees/factory-NNN' '$doc'"
check "doc: complete line has the tick-plan form" grep -qF 'Task N: complete (merge <sha>)' "$doc"
check "doc: says the worktree comes from the WORKTREE line" grep -qF 'WORKTREE:' "$doc"
```

Run: `bash .claude/factory/tests/run.sh | grep -E '^(FAIL|all passed|[0-9]+ failed)'`
Expected: FAIL for the `doc:` checks about `implementer done`, `SKIPPED`, `worktrees/factory-NNN`, the complete form and `WORKTREE:`. The `ctl <sub>` checks should already pass. If one of them fails, the command prose has the text `ctl <word>` where the word is not a subcommand: reword that sentence.

- [ ] **Step 2: Fix the doc contradictions**

In `.claude/software-factory.md`:

Replace the bullet starting `- Job id \`NNN\`, branch` (Job model) with:

```markdown
- Job id `NNN`, branch `factory/NNN-<slug>`. The builder runs in a harness-made worktree (`isolation: worktree`, `settings.json` has `worktree.baseRef: head`). Its path is not predictable (it looks like `.claude/worktrees/agent-<id>`). The builder reports it on the `WORKTREE:` handoff line and the orchestrator records it on the board.
```

Replace the bullet starting `  - \`.superpowers/sdd/YYYY-MM-DD-NNN-<slug>/progress.md\`: the normal SDD ledger.` with:

```markdown
  - `.superpowers/sdd/YYYY-MM-DD-NNN-<slug>/progress.md`: the normal SDD ledger, written only through `factory-ctl ledger` and `tick-plan`. Lines: `Task N: dispatched (BASE <sha>, opus, agent <id>)`, `Head round <round>: <sha>`, `Ruling: <text>` and, at merge, `Task N: complete (merge <sha>)`. Builders never write it. `resume-plan` keeps working after `/clear`.
```

In the `## Verdict contract` paragraph, replace `are recorded as \`SKIPPED\`, which counts as pass` with `are recorded as \`skipped\`, which counts as pass`.

- [ ] **Step 3: Run the full verification**

Run each and confirm the expected result:

```bash
bash .claude/factory/tests/run.sh | tail -2
python3 -c "import ast; ast.parse(open('.claude/factory/factory-ctl').read()); print('ctl syntax ok')"
git status --short
```

Expected: the test tail shows `all passed`, `ctl syntax ok` prints, and `git status --short` lists only the files of this plan (`.claude/factory/factory-ctl`, `.claude/factory/tests/run.sh`, `.claude/commands/factory.md`, `.claude/software-factory.md`), all already committed except `.claude/software-factory.md` and `.claude/factory/tests/run.sh` from this task. No cargo or yarn command is needed: no Rust or TypeScript file changed.

- [ ] **Step 4: Spec coverage check**

Confirm by reading the files that each gap has its fix: A1 merge step 3 and 4 in `factory.md`; A2 build step 1; A3 `prev_head` and `diff-range`; A4 `head unchanged` in `set-stage` and build step 4; A5 review step 2; A6 approve stage paragraph; A7 `next_job_number` and `exist_ok`; A8 rework paragraph; A9 `show`, `clear-builder`, `check_stage_move`, status line; A10 this task. Fix any miss inline before the commit.

- [ ] **Step 5: Commit**

Invoke the skill `dev-workflow-skills:1-git-commit` for the message. Suggested type: `docs(factory): align the design doc with the command and add contract tests`. Then:

```bash
git add .claude/software-factory.md .claude/factory/tests/run.sh && git commit --only -m "<message from the skill>" -- .claude/software-factory.md .claude/factory/tests/run.sh
```

Next plan: docs/superpowers/plans/2026-10-13-software-factory-v2/plan-02-evidence-and-prompts.md
