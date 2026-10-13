# Software Factory v2, Plan 03: Main Moved, Lifecycle and Locking

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. It is executed by a `factory-builder` subagent. Steps use checkbox (`- [ ]`) syntax for tracking. **Plans 01 and 02 of this series must be finished and merged before this plan starts.** This plan uses only what they produce: `ctl show`, the board field `prev_head`, `ctl diff-range`, `ctl clear-builder`, the `set-stage` stage guard with `--force`, and `ctl record-checks`. If one of those is missing, stop and report it instead of rebuilding it.

**Goal:** Make a job survive a moved `main`, make concurrent sessions and half-finished jobs safe, and make the merge step fail cleanly.

**Architecture:** `factory-ctl` gains an `flock` board lock around every mutating call, a job lease, `cancel`, `cleanup`, `base-state`, `sync-base`, `sync-diff` and `merge-preflight`. The `/factory` command gains a "Sync flow" (merge `main` into the job branch inside the job worktree, rerun the checks through the builder, one scoped code review of the sync, no history rewrite) and a hardened merge step. The orchestrator stays a thin caller of ctl.

**Tech Stack:** Python 3 stdlib (`fcntl`), bash tests with `jq` and `flock` (util-linux), markdown command and doc files. No Rust or TypeScript is touched.

**Spec:** [docs/superpowers/specs/2026-10-13-software-factory-v2-design.md](../../specs/2026-10-13-software-factory-v2-design.md), section C (gaps C1 to C7) and decisions 6 and 7. Design doc to keep in sync: [.claude/software-factory.md](../../../../.claude/software-factory.md).

## Global Constraints

- `cargo` is not needed in this plan. If you run it anyway, pass `-j4`, and never `--workspace` or `--all`.
- Commits use the plugin skill `dev-workflow-skills:1-git-commit` (invoke it with the Skill tool), conventional commit format, and a pathspec commit: `git add <paths> && git commit --only -m "..." -- <paths>`. Never `git add -A`, `--all` or `.`. Never set `HARNESS_GATE=0`. Never push.
- No bare `git stash` or `git stash pop`.
- Test fixtures that need git use temp repos under `$tmp` (the `mktemp -d` directory in `run.sh`) only. Never run a fixture against the real repo. Every ctl call in a fixture sets `FACTORY_ROOT` to the temp repo.
- `board.json` is written only by `factory-ctl`. Ledger writes go through ctl only (spec decision 8).
- The orchestrator never rebases and never rewrites history, and the factory never pushes.
- `rocket-*` Rust crates and `src/` are out of scope. The factory server ignores unknown board fields (serde default), so the new fields `lease`, `cancelled`, `syncs` and `cleaned` need no server change.
- Code comments are short full sentences that end with a period. No emojis.
- Match the existing style: `factory-ctl` is plain Python with `die()`, `repo_root()`, `find_job()`, `git(root, ...)`. Tests use `check "<description>" <command...>` in `.claude/factory/tests/run.sh`.

## Review Focus

- Two sessions calling ctl at the same moment must not collide on ids or lose a board update. A held lock must fail a mutating call after a bounded wait and never block a read. Tested in Task 1.
- `ctl cleanup` must never delete uncommitted work, never delete unmerged commits unless the user says `--discard-commits`, and never touch the branch the main checkout is on. Tested in Task 1.
- A job whose base is not on `main` (unrelated commits baked in by `baseRef: head`) must never be silently synced, because its unrelated commits would then merge into `main`. It goes to the human. Tested in Task 2.
- A sync that cannot finish (conflict, dirty worktree, or `main` moving again and again) must park the job in `needs-human` instead of looping. The sync count is capped. Tested in Task 2.
- A non-conflict `git merge` failure (for example an untracked file that would be overwritten) must leave `main` exactly as it was, and the job must land in `needs-human` with the real error. Tested in Task 3.
- A short rework note such as `all green` must be passed to the builder unchanged and leave no stray `rework-note.md`. Linted in Task 3.

---

## File structure

- Modify: `.claude/factory/factory-ctl` (all new subcommands and the lock).
- Modify: `.claude/factory/tests/run.sh` (new fixture sections, inserted before the `# ---- /factory command ----` line).
- Modify: `.claude/commands/factory.md` (Sync flow, cancel, cleanup, lease, merge step, rework note rule).
- Modify: `.claude/software-factory.md` (doc sync).

New board fields: `lease` (`null` or `{"owner": str, "expires": float}`), `cancelled` (bool, set only by `cancel`), `syncs` (int, set by `sync-base`), `cleaned` (bool, set by `cleanup`).

---

### Task 1: Board lock, job lease, `ctl cancel`, `ctl cleanup`

**Files:**
- Modify: `.claude/factory/factory-ctl`
- Modify: `.claude/factory/tests/run.sh` (insert before `# ---- /factory command ----`)

**Interfaces:**
- Consumes: existing `repo_root()`, `load_board()`, `save_board()`, `find_job()`, `job_dir()`, `git()`, `die()`. From plan 01, the `set-stage` stage guard (a new job moves `spec` to a later stage without `--force`).
- Produces:
  - `board_lock(root)` context manager. Every ctl command named in `MUTATING` runs inside it. Wait is `FACTORY_LOCK_WAIT` seconds (default 15).
  - `ctl lease <id> --owner <name> [--ttl <seconds>] [--release]`.
  - `ctl cancel <id> [--reason <text>] [--force]`.
  - `ctl cleanup <id> [--discard-commits]`.
  - helpers `append_ledger(root, job, line)`, `is_ancestor(root, a, b)`, `resolve_commit(root, rev)`, `worktree_of_branch(root, branch)`.
  - new-job writes `"lease": None`.

- [ ] **Step 1: Write the failing tests**

In `.claude/factory/tests/run.sh`, insert this block directly before the line `# ---- /factory command ----`:

```bash
# ---- board lock, lease, cancel and cleanup ----
cr="$tmp/cr"
git init -q -b main "$cr"
git -C "$cr" -c user.email=t@t -c user.name=t commit -q --allow-empty -m init
for i in 1 2 3 4 5 6 7 8; do
  FACTORY_ROOT="$cr" python3 "$ctl" new-job "Job $i" --date 2026-10-13 >/dev/null 2>&1 &
done
wait
cboard="$cr/.superpowers/factory/board.json"
cl() { FACTORY_ROOT="$cr" python3 "$ctl" "$@"; }
check "ctl: 8 parallel new-job calls give 8 distinct ids" \
  jq -e '[.jobs[].id] | (length == 8 and (unique | length) == 8)' "$cboard"
check "ctl: a new job has a null lease" jq -e '.jobs[0].lease == null' "$cboard"

flock "$cr/.superpowers/factory/board.json.lock" sleep 2 &
sleep 0.5
check "ctl: a held board lock fails a mutating call after the wait" \
  bash -c "! FACTORY_ROOT='$cr' FACTORY_LOCK_WAIT=0.3 python3 '$ctl' set-stage 1 build"
check "ctl: a held board lock does not block status" cl status
wait
check "ctl: the lock is free again afterwards" cl set-stage 1 build

check "ctl: lease is granted" cl lease 1 --owner alice --ttl 60
check "ctl: lease is stored with owner" jq -e '.jobs[0].lease | select(.owner == "alice")' "$cboard"
check "ctl: another owner cannot take a live lease" \
  bash -c "! FACTORY_ROOT='$cr' python3 '$ctl' lease 1 --owner bob --ttl 60"
check "ctl: the owner can renew its lease" cl lease 1 --owner alice --ttl 60
check "ctl: another owner cannot release a live lease" \
  bash -c "! FACTORY_ROOT='$cr' python3 '$ctl' lease 1 --owner bob --release"
check "ctl: the owner can release" cl lease 1 --owner alice --release
check "ctl: a released lease is null" jq -e '.jobs[0].lease == null' "$cboard"
cl lease 1 --owner alice --ttl 0 >/dev/null
check "ctl: an expired lease can be taken over" cl lease 1 --owner bob --ttl 60

cl lease 3 --owner alice --ttl 60 >/dev/null
check "ctl: cancel refuses a job with a live lease" \
  bash -c "! FACTORY_ROOT='$cr' python3 '$ctl' cancel 3 --reason nope"
check "ctl: cancel --force overrides a live lease" cl cancel 3 --reason "not needed" --force
check "ctl: cancel parks the job in needs-human with a reason" \
  jq -e '.jobs[2] | select(.stage == "needs-human" and .cancelled == true and (.reason | startswith("cancelled")) and .lease == null)' "$cboard"
check "ctl: cancel writes a ledger ruling" \
  grep -q '^Ruling: job cancelled' "$cr/.superpowers/sdd/2026-10-13-003-job-3/progress.md"

lr="$tmp/lr"
git init -q -b main "$lr"
gl() { git -C "$lr" -c user.email=t@t -c user.name=t "$@"; }
gl commit -q --allow-empty -m init
ll() { FACTORY_ROOT="$lr" python3 "$ctl" "$@"; }
lbase="$(gl rev-parse main)"

ll new-job "Alpha" --date 2026-10-13 >/dev/null
gl worktree add -q "$tmp/wt1" -b factory/001-alpha
git -C "$tmp/wt1" -c user.email=t@t -c user.name=t commit -q --allow-empty -m work
lhead="$(git -C "$tmp/wt1" rev-parse HEAD)"
ll set-stage 1 approve --worktree "$tmp/wt1" --base "$lbase" --head "$lhead" >/dev/null
check "ctl: cleanup refuses a job that is neither merged nor cancelled" \
  bash -c "! FACTORY_ROOT='$lr' python3 '$ctl' cleanup 1"
check "ctl: a refused cleanup keeps the worktree" test -d "$tmp/wt1"
gl merge -q --no-ff -m merge factory/001-alpha
echo scratch >"$tmp/wt1/dirty.txt"
check "ctl: cleanup refuses a worktree with untracked work" \
  bash -c "! FACTORY_ROOT='$lr' python3 '$ctl' cleanup 1"
check "ctl: a refused cleanup keeps the dirty worktree and the branch" \
  bash -c "test -f '$tmp/wt1/dirty.txt' && git -C '$lr' rev-parse --verify -q refs/heads/factory/001-alpha"
rm "$tmp/wt1/dirty.txt"
check "ctl: cleanup removes a merged job's worktree and branch" ll cleanup 1
check "ctl: the worktree directory is gone" test ! -d "$tmp/wt1"
check "ctl: the branch is gone" bash -c "! git -C '$lr' rev-parse --verify -q refs/heads/factory/001-alpha"
check "ctl: the main checkout is still on main" test "$(gl branch --show-current)" = main
check "ctl: cleanup records the result on the board" \
  jq -e '.jobs[0] | select(.worktree == null and .cleaned == true)' "$lr/.superpowers/factory/board.json"
ll set-stage 1 merged >/dev/null
check "ctl: cancel refuses a merged job" bash -c "! FACTORY_ROOT='$lr' python3 '$ctl' cancel 1"

ll new-job "Beta" --date 2026-10-13 >/dev/null
gl worktree add -q "$tmp/wt2" -b factory/002-beta
git -C "$tmp/wt2" -c user.email=t@t -c user.name=t commit -q --allow-empty -m beta-work
bhead="$(git -C "$tmp/wt2" rev-parse HEAD)"
ll set-stage 2 build --worktree "$tmp/wt2" --base "$lbase" --head "$bhead" >/dev/null
ll cancel 2 --reason "dropped" >/dev/null
check "ctl: cleanup refuses unmerged commits without --discard-commits" \
  bash -c "! FACTORY_ROOT='$lr' python3 '$ctl' cleanup 2"
check "ctl: the refused cleanup kept the branch" gl rev-parse --verify -q refs/heads/factory/002-beta
check "ctl: cleanup --discard-commits removes a cancelled job" ll cleanup 2 --discard-commits
check "ctl: the cancelled job's branch is gone" bash -c "! git -C '$lr' rev-parse --verify -q refs/heads/factory/002-beta"

ll new-job "Gamma" --date 2026-10-13 >/dev/null
gl switch -q -c factory/003-gamma
ll cancel 3 --reason "dropped" >/dev/null
check "ctl: cleanup refuses the branch the main checkout is on" \
  bash -c "! FACTORY_ROOT='$lr' python3 '$ctl' cleanup 3 --discard-commits"
check "ctl: the main checkout stays on that branch" test "$(gl branch --show-current)" = factory/003-gamma
gl switch -q main
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bash .claude/factory/tests/run.sh 2>&1 | grep -E 'FAIL|failed|all passed'`
Expected: many `FAIL: ctl: ...` lines for the new block (the `lease`, `cancel`, `cleanup` subcommands do not exist, and the parallel `new-job` calls collide) and a final `N failed`.

- [ ] **Step 3: Add the lock, helpers and new-job field**

In `.claude/factory/factory-ctl`, add `contextlib` and `fcntl` to the imports (keep the list alphabetical):

```python
import argparse
import contextlib
import fcntl
import json
```

Below the `REVIEW_STATES = ...` line add:

```python
LOCK_WAIT_SECONDS = 15
# Every command that writes the board or a ledger runs under the board lock.
# Read-only commands (status, show, diff-range, is-merged, root, base-state,
# sync-diff, merge-preflight) never take it.
MUTATING = {
    "new-job", "set-stage", "ledger", "tick-plan", "lease", "cancel", "cleanup",
    "sync-base", "clear-builder", "record-checks",
}
```

Below `board_path()` add:

```python
def lock_path(root):
    return os.path.join(root, ".superpowers", "factory", "board.json.lock")


@contextlib.contextmanager
def board_lock(root):
    """Hold an exclusive flock on board.json.lock, or fail after a bounded wait."""
    path = lock_path(root)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    wait = float(os.environ.get("FACTORY_LOCK_WAIT", LOCK_WAIT_SECONDS))
    deadline = time.time() + wait
    handle = open(path, "a", encoding="utf-8")
    try:
        while True:
            try:
                fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except BlockingIOError:
                if time.time() >= deadline:
                    die("the board is locked by another factory-ctl call, try again")
                time.sleep(0.05)
        yield
    finally:
        handle.close()
```

Below `git()` add the shared helpers and move the ledger write into one place:

```python
def resolve_commit(root, rev):
    out = git(root, "rev-parse", "--verify", "-q", f"{rev}^{{commit}}")
    if out.returncode != 0:
        die(f"cannot resolve {rev!r} to a commit")
    return out.stdout.strip()


def is_ancestor(root, older, newer):
    return git(root, "merge-base", "--is-ancestor", older, newer).returncode == 0


def append_ledger(root, job, line):
    with open(os.path.join(job_dir(root, job), "progress.md"), "a", encoding="utf-8") as handle:
        handle.write(line.rstrip("\n") + "\n")


def worktree_of_branch(root, branch):
    """Return the path of the worktree that has the branch checked out, or None."""
    out = git(root, "worktree", "list", "--porcelain")
    path = None
    for line in out.stdout.splitlines():
        if line.startswith("worktree "):
            path = line[len("worktree "):]
        elif line == f"branch refs/heads/{branch}":
            return path
    return None


def lease_is_live(job, now):
    held = job.get("lease")
    return held is not None and held["expires"] > now
```

In `cmd_new_job`, add `"lease": None,` after the `"reason": None,` line of the job dict. Change `cmd_ledger` to use the helper:

```python
def cmd_ledger(args):
    root = repo_root()
    job = find_job(load_board(root), args.id)
    append_ledger(root, job, args.line)
```

Replace `main()`:

```python
def main():
    args = build_parser().parse_args()
    if args.command in MUTATING:
        with board_lock(repo_root()):
            args.func(args)
    else:
        args.func(args)
```

If plan 01 already changed `main()`, keep its body and only add the `MUTATING` branch around `args.func(args)`.

- [ ] **Step 4: Add `lease`, `cancel` and `cleanup`**

Add these functions above `build_parser()`:

```python
def cmd_lease(args):
    root = repo_root()
    board = load_board(root)
    job = find_job(board, args.id)
    now = time.time()
    held = job.get("lease")
    live = lease_is_live(job, now)
    if live and held["owner"] != args.owner:
        left = int(held["expires"] - now)
        die(f"job {job['id']} is leased by {held['owner']} for another {left} seconds")
    job["lease"] = None if args.release else {"owner": args.owner, "expires": now + args.ttl}
    save_board(root, board)


def cmd_cancel(args):
    root = repo_root()
    board = load_board(root)
    job = find_job(board, args.id)
    if job["stage"] == "merged":
        die("job is already merged, nothing to cancel")
    if lease_is_live(job, time.time()) and not args.force:
        die(f"job is leased by {job['lease']['owner']}, use --force to cancel anyway")
    reason = args.reason or "cancelled by the user"
    if job["stage"] != "needs-human":
        job["stage_since"] = time.time()
    job["stage"] = "needs-human"
    job["reason"] = f"cancelled: {reason}"
    job["cancelled"] = True
    job["lease"] = None
    save_board(root, board)
    append_ledger(root, job, f"Ruling: job cancelled ({reason})")


def cmd_cleanup(args):
    root = repo_root()
    board = load_board(root)
    job = find_job(board, args.id)
    branch = job["branch"]
    if not branch.startswith("factory/"):
        die(f"refusing to clean up branch {branch!r}, it is not a factory branch")
    if git(root, "branch", "--show-current").stdout.strip() == branch:
        die("the main checkout is on the job branch, refusing to remove it")
    head = job.get("head")
    merged = bool(head) and head != job.get("base") and is_ancestor(root, head, "main")
    if not merged and not job.get("cancelled"):
        die("job is neither merged nor cancelled, refusing to clean up")
    tip = git(root, "rev-parse", "--verify", "-q", f"refs/heads/{branch}").stdout.strip()
    if tip and not is_ancestor(root, tip, "main") and not args.discard_commits:
        die(f"branch {branch} has commits that are not on main, use --discard-commits to drop them")
    path = worktree_of_branch(root, branch) or job.get("worktree")
    if path and os.path.isdir(path):
        if os.path.realpath(path) == os.path.realpath(root):
            die("the job worktree is the main checkout, refusing to remove it")
        status = git(path, "status", "--porcelain")
        if status.returncode != 0:
            die(f"cannot read the status of {path}: {status.stderr.strip()}")
        if status.stdout.strip():
            die(f"worktree {path} has uncommitted or untracked work, refusing to remove it")
        removed = git(root, "worktree", "remove", path)
        if removed.returncode != 0:
            die(f"git worktree remove failed: {removed.stderr.strip()}")
    git(root, "worktree", "prune")
    if tip:
        flag = "-D" if args.discard_commits else "-d"
        deleted = git(root, "branch", flag, branch)
        if deleted.returncode != 0:
            die(f"git branch {flag} failed: {deleted.stderr.strip()}")
    job["worktree"] = None
    job["builder_id"] = None
    job["cleaned"] = True
    save_board(root, board)
    append_ledger(root, job, f"Ruling: cleanup removed worktree {path or 'none'} and branch {branch}")
```

In `build_parser()`, before `return parser`, add:

```python
    lease = sub.add_parser("lease")
    lease.add_argument("id")
    lease.add_argument("--owner", required=True)
    lease.add_argument("--ttl", type=int, default=1800)
    lease.add_argument("--release", action="store_true")
    lease.set_defaults(func=cmd_lease)

    cancel = sub.add_parser("cancel")
    cancel.add_argument("id")
    cancel.add_argument("--reason")
    cancel.add_argument("--force", action="store_true")
    cancel.set_defaults(func=cmd_cancel)

    cleanup = sub.add_parser("cleanup")
    cleanup.add_argument("id")
    cleanup.add_argument("--discard-commits", dest="discard_commits", action="store_true")
    cleanup.set_defaults(func=cmd_cleanup)
```

The checks run before any change (merged or cancelled, commits, main branch, dirty worktree), so a refusal never leaves a half-removed job.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `bash .claude/factory/tests/run.sh 2>&1 | grep -E 'FAIL|failed|all passed'`
Expected: `all passed` and no `FAIL` line. If a parallel `new-job` check is flaky, the lock is wrong: do not add retries to the test.

- [ ] **Step 6: Commit**

Invoke the skill `dev-workflow-skills:1-git-commit` and let it draft the message (for example `feat(factory): lock the board and add lease, cancel and cleanup`), then:

```bash
git add .claude/factory/factory-ctl .claude/factory/tests/run.sh && git commit --only -m "<message from the skill>" -- .claude/factory/factory-ctl .claude/factory/tests/run.sh
```

---

### Task 2: Main moved recovery (`base-state`, `sync-base`, `sync-diff`, Sync flow)

**Files:**
- Modify: `.claude/factory/factory-ctl`
- Modify: `.claude/factory/tests/run.sh` (append after the Task 1 block, still before `# ---- /factory command ----`)
- Modify: `.claude/commands/factory.md`

**Interfaces:**
- Consumes: Task 1 helpers `append_ledger`, `resolve_commit`, `is_ancestor`; `ctl show <id>` (plan 01, JSON of one job); `ctl set-stage ... --force` (plan 01); `ctl record-checks <id> --file <path>` (plan 02); the builder `CHECKS:` block (plan 02).
- Produces:
  - `compute_base_state(root, job)` returning `(token, main_sha)` with token in `ok`, `main-moved`, `base-not-main`, `merged`.
  - `ctl base-state <id>` prints `<token> <main sha>`, exit 0 always (read-only).
  - `ctl sync-base <id> --new-base <sha> [--head <sha>]` the only path that changes `base` without `--force-base`. It writes `base`, optionally `head`, increments `syncs` (cap `MAX_SYNCS = 2`) and appends a `Ruling:` ledger line.
  - `ctl sync-diff <id> --old-head <sha>` prints the diff `<old head>..<job head>` limited to the paths the job changes relative to its base.

- [ ] **Step 1: Write the failing tests**

Append this block in `.claude/factory/tests/run.sh`, after the Task 1 block and before `# ---- /factory command ----`:

```bash
# ---- main moved, base-state, sync-base and sync-diff ----
mr="$tmp/mr"
git init -q -b main "$mr"
gm() { git -C "$mr" -c user.email=t@t -c user.name=t "$@"; }
gm commit -q --allow-empty -m init
echo one >"$mr/shared.txt"
gm add shared.txt
gm commit -q -m shared
lm() { FACTORY_ROOT="$mr" python3 "$ctl" "$@"; }
mboard="$mr/.superpowers/factory/board.json"
mledger="$mr/.superpowers/sdd/2026-10-13-001-moved/progress.md"
# Usage: move_main <name>. Commits one new file on main and prints the new main sha.
move_main() {
  echo "$1" >"$mr/$1.txt"
  gm add "$1.txt"
  gm commit -q -m "$1"
  gm rev-parse main
}

mbase="$(gm rev-parse main)"
lm new-job "Moved" --date 2026-10-13 >/dev/null
gm worktree add -q "$tmp/mwt" -b factory/001-moved
gw() { git -C "$tmp/mwt" -c user.email=t@t -c user.name=t "$@"; }
echo work >"$tmp/mwt/job.txt"
gw add job.txt
gw commit -q -m job
mhead="$(gw rev-parse HEAD)"
lm set-stage 1 approve --worktree "$tmp/mwt" --base "$mbase" --head "$mhead" >/dev/null
check "ctl: base-state is ok on an untouched main" test "$(lm base-state 1)" = "ok $mbase"

m1="$(move_main other1)"
check "ctl: base-state says main-moved after an unrelated commit on main" test "$(lm base-state 1)" = "main-moved $m1"

board_before="$(cat "$mboard")"
check "ctl: sync-base refuses a new base the head does not contain" \
  bash -c "! FACTORY_ROOT='$mr' python3 '$ctl' sync-base 1 --new-base '$m1'"
check "ctl: sync-base refuses a new base that is not on main" \
  bash -c "! FACTORY_ROOT='$mr' python3 '$ctl' sync-base 1 --new-base '$mhead'"
check "ctl: a refused sync-base leaves the board unchanged" test "$board_before" = "$(cat "$mboard")"

gw merge -q --no-ff -m sync1 "$m1"
nh1="$(gw rev-parse HEAD)"
check "ctl: sync-base moves the base and head after the sync merge" lm sync-base 1 --new-base "$m1" --head "$nh1"
check "ctl: sync-base stores base, head and the sync count" \
  jq -e --arg b "$m1" --arg h "$nh1" '.jobs[0] | select(.base == $b and .head == $h and .syncs == 1)' "$mboard"
check "ctl: sync-base writes a ledger ruling" grep -q '^Ruling: base synced' "$mledger"
check "ctl: base-state is ok after the sync" test "$(lm base-state 1)" = "ok $m1"
check "ctl: sync-base with the same base is refused" \
  bash -c "! FACTORY_ROOT='$mr' python3 '$ctl' sync-base 1 --new-base '$m1' --head '$nh1'"

echo fix >>"$tmp/mwt/job.txt"
gw commit -q -am fix
fix="$(gw rev-parse HEAD)"
lm set-stage 1 approve --head "$fix" >/dev/null
lm sync-diff 1 --old-head "$mhead" >"$tmp/sync.diff"
check "ctl: sync-diff shows changes to the job's own files" grep -q '^+fix' "$tmp/sync.diff"
check "ctl: sync-diff leaves out the files that only main changed" bash -c "! grep -q other1 '$tmp/sync.diff'"

m2="$(move_main other2)"
gw merge -q --no-ff -m sync2 "$m2"
nh2="$(gw rev-parse HEAD)"
check "ctl: a second sync is allowed" lm sync-base 1 --new-base "$m2" --head "$nh2"
m3="$(move_main other3)"
gw merge -q --no-ff -m sync3 "$m3"
nh3="$(gw rev-parse HEAD)"
check "ctl: a third sync is refused, main keeps moving" \
  bash -c "! FACTORY_ROOT='$mr' python3 '$ctl' sync-base 1 --new-base '$m3' --head '$nh3'"

# A job whose base is not on main must never be synced.
lm new-job "Side" --date 2026-10-13 >/dev/null
gm worktree add -q "$tmp/mwt2" -b factory/002-side main
gx() { git -C "$tmp/mwt2" -c user.email=t@t -c user.name=t "$@"; }
echo unrelated >"$tmp/mwt2/unrelated.txt"
gx add unrelated.txt
gx commit -q -m unrelated
sbase="$(gx rev-parse HEAD)"
echo mine >"$tmp/mwt2/mine.txt"
gx add mine.txt
gx commit -q -m mine
shead="$(gx rev-parse HEAD)"
lm set-stage 2 approve --worktree "$tmp/mwt2" --base "$sbase" --head "$shead" >/dev/null
check "ctl: base-state says base-not-main when the base is off main" \
  bash -c "test \"\$(FACTORY_ROOT='$mr' python3 '$ctl' base-state 2 | cut -d' ' -f1)\" = base-not-main"
check "ctl: sync-base refuses a job whose base is not on main" \
  bash -c "! FACTORY_ROOT='$mr' python3 '$ctl' sync-base 2 --new-base '$m3' --head '$shead'"
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bash .claude/factory/tests/run.sh 2>&1 | grep -E 'FAIL|failed|all passed'`
Expected: `FAIL` for every check that calls `base-state`, `sync-base` or `sync-diff` (unknown subcommand).

- [ ] **Step 3: Implement the ctl subcommands**

In `.claude/factory/factory-ctl`, below `LOCK_WAIT_SECONDS`, add `MAX_SYNCS = 2`. Add above `build_parser()`:

```python
def compute_base_state(root, job):
    """Classify the job against main: ok, main-moved, base-not-main or merged."""
    if not job.get("base") or not job.get("head"):
        die("job has no recorded base and head")
    main = resolve_commit(root, "main")
    head = resolve_commit(root, job["head"])
    base = resolve_commit(root, job["base"])
    if is_ancestor(root, head, main):
        return "merged", main
    merge_base = git(root, "merge-base", head, main)
    if merge_base.returncode != 0 or merge_base.stdout.strip() != base:
        return "base-not-main", main
    if is_ancestor(root, main, head):
        return "ok", main
    return "main-moved", main


def cmd_base_state(args):
    root = repo_root()
    job = find_job(load_board(root), args.id)
    token, main = compute_base_state(root, job)
    print(f"{token} {main}")


def cmd_sync_base(args):
    root = repo_root()
    board = load_board(root)
    job = find_job(board, args.id)
    if not job.get("base") or not job.get("head"):
        die("job has no recorded base and head")
    if job.get("syncs", 0) >= MAX_SYNCS:
        die(f"job was already synced {MAX_SYNCS} times, main keeps moving, ask the user")
    old_base = resolve_commit(root, job["base"])
    old_head = resolve_commit(root, job["head"])
    new_base = resolve_commit(root, args.new_base)
    new_head = resolve_commit(root, args.head) if args.head else old_head
    if new_base == old_base:
        die("the new base equals the old base, nothing to sync")
    if not is_ancestor(root, new_base, "main"):
        die(f"{new_base[:7]} is not on main")
    if not is_ancestor(root, old_base, new_base):
        die("the new base must descend from the old base, the old base is not on main")
    if not is_ancestor(root, old_head, new_head):
        die("the new head must contain the old head, history is never rewritten")
    if not is_ancestor(root, new_base, new_head):
        die("the new head must contain the new base, merge main into the job branch first")
    job["base"] = new_base
    job["head"] = new_head
    job["syncs"] = job.get("syncs", 0) + 1
    save_board(root, board)
    append_ledger(
        root,
        job,
        f"Ruling: base synced {old_base[:7]} to {new_base[:7]}, head {old_head[:7]} to {new_head[:7]}, "
        "main moved, main was merged into the job branch",
    )


def cmd_sync_diff(args):
    root = repo_root()
    job = find_job(load_board(root), args.id)
    if not job.get("base") or not job.get("head"):
        die("job has no recorded base and head")
    old_head = resolve_commit(root, args.old_head)
    names = git(root, "diff", "--name-only", "-z", job["base"], job["head"]).stdout.split("\0")
    paths = [name for name in names if name]
    if not paths:
        return
    out = git(root, "diff", f"{old_head}..{job['head']}", "--", *paths)
    if out.returncode != 0:
        die(out.stderr.strip() or "git diff failed")
    sys.stdout.write(out.stdout)
```

Add to `build_parser()` before `return parser`:

```python
    base_state = sub.add_parser("base-state")
    base_state.add_argument("id")
    base_state.set_defaults(func=cmd_base_state)

    sync_base = sub.add_parser("sync-base")
    sync_base.add_argument("id")
    sync_base.add_argument("--new-base", dest="new_base", required=True)
    sync_base.add_argument("--head")
    sync_base.set_defaults(func=cmd_sync_base)

    sync_diff = sub.add_parser("sync-diff")
    sync_diff.add_argument("id")
    sync_diff.add_argument("--old-head", dest="old_head", required=True)
    sync_diff.set_defaults(func=cmd_sync_diff)
```

`sync-diff` limits the diff to the files the job changes against the (new) base, so it shows only how the sync and any fix commits changed the job's own files, not everything that came in from `main`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `bash .claude/factory/tests/run.sh 2>&1 | grep -E 'FAIL|failed|all passed'`
Expected: `all passed`.

- [ ] **Step 5: Write the failing command lint**

Append inside the Task 2 block of `run.sh` (end of the block):

```bash
cmdfile="$repo/.claude/commands/factory.md"
for word in 'base-state' 'sync-base' 'sync-diff' 'Sync flow' 'base-not-main' 'main-moved' 'sync conflict' 'never rewrite'; do
  grep -qF -- "$word" "$cmdfile" && pass "command mentions: $word" || fail "command mentions: $word"
done
```

Run: `bash .claude/factory/tests/run.sh 2>&1 | grep -E 'FAIL'`
Expected: `FAIL: command mentions: ...` for each word.

- [ ] **Step 6: Add the Sync flow to `.claude/commands/factory.md`**

Add this paragraph to the stage `approve` section, as its first step, before the approver is dispatched (keep plan 01's `is-merged` check before it):

```markdown
Before the approver runs, check the base: `ctl base-state <id>`. The first word is the state. `ok`: continue. `main-moved`: run the Sync flow below, then continue. `base-not-main`: `ctl set-stage <id> needs-human --reason "base is not on main (unrelated commits in the job)"`, stop, and tell the user to rework or cancel the job. `merged`: skip to the merge step.
```

Add this section directly above `## Merge step`:

```markdown
## Sync flow

Used when `ctl base-state <id>` says `main-moved`. It brings `main` into the job branch with one merge commit inside the job worktree. Never rebase, never rewrite history, never reset a branch. Never run it for `base-not-main`.

1. Read `ctl show <id>`. The worktree path must exist and `git -C "<worktree>" status --porcelain` must be empty, and `git -C "<worktree>" branch --show-current` must be the job branch. The worktree head must equal the board head. Otherwise `ctl set-stage <id> needs-human --force --reason "sync needs a clean job worktree"` and stop.
2. Remember `<old head>` (the board head) and `<main sha>` (the sha printed by `ctl base-state`). Draft the commit message as in "Commit message" below, with the fallback `chore(factory): sync job <id> <slug> with main`. Run `git -C "<worktree>" merge --no-ff <main sha> -m "<message>"`.
3. A conflict or any other failure: run `git -C "<worktree>" merge --abort` (ignore the error if no merge is in progress), then `ctl set-stage <id> needs-human --force --reason "sync conflict with main"` and stop. A sync that fails is for the user to resolve. The factory never resolves conflicts on its own.
4. `ctl sync-base <id> --new-base <main sha> --head <new head>` where `<new head>` is `git -C "<worktree>" rev-parse HEAD`. If it refuses because the job was synced twice already, `ctl set-stage <id> needs-human --force --reason "main keeps moving"` and stop.
5. Resume the builder with SendMessage (builder fallback rules as in the `build` stage): "main was merged into your branch at <main sha>. Rerun the checks in your worktree, fix only what the merge broke, commit with a pathspec, never rewrite history, and end with a CHECKS block and the WORKTREE, BRANCH, BASE and HEAD lines." Save the CHECKS block as `checks-sync-<main7>.md` in the job directory and run `ctl record-checks <id> --file <that file>`. A missing CHECKS block or any failing line means `ctl set-stage <id> needs-human --force --reason "checks failed after sync"` and stop. If the HEAD line differs from the board head, `ctl set-stage <id> <current stage> --head <HEAD>` and `ctl ledger <id> "Head sync <main7>: <sha>"`.
6. `ctl sync-diff <id> --old-head <old head>` saved as `review-sync-<main7>.diff` in the job directory. An empty diff means no review is needed: go to step 8.
7. Dispatch ONE reviewer, `factory-code-reviewer`, with that diff, `spec.md` and the worktree path, and the instruction that it covers only how the sync and fixes changed the job's own files. Save the reply as `review-code-sync-<main7>.md`. The last line must be `VERDICT: APPROVE`. Anything else, including no `VERDICT:` line, means `ctl set-stage <id> needs-human --force --reason "sync review found changes"` and stop. There is no second sync review loop.
8. `ctl ledger <id> "Ruling: sync with main <main7> done, checks rerun, sync diff reviewed"` and `ctl set-stage <id> approve --force`. Return to the caller: the `approve` stage runs the approver, the merge step starts again from its step 1.
```

Add this paragraph under `## Merge step` at its end for now (Task 3 rewrites the merge step as a whole):

```markdown
If the merge preflight says `main-moved`, run the Sync flow once, then restart this merge step.
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `bash .claude/factory/tests/run.sh 2>&1 | grep -E 'FAIL|failed|all passed'`
Expected: `all passed`. (The command lint word `sync conflict` and `never rewrite` come from step 3 and the first sentence of the Sync flow: "Never rebase, never rewrite history". If `never rewrite` does not match, make the sentence read "never rewrite history" in lowercase.)

- [ ] **Step 8: Commit**

Invoke the skill `dev-workflow-skills:1-git-commit` (for example `feat(factory): recover a job when main moved`), then:

```bash
git add .claude/factory/factory-ctl .claude/factory/tests/run.sh .claude/commands/factory.md && git commit --only -m "<message from the skill>" -- .claude/factory/factory-ctl .claude/factory/tests/run.sh .claude/commands/factory.md
```

---

### Task 3: Merge failure handling, merge commit message, rework note rule, doc sync, verification

**Files:**
- Modify: `.claude/factory/factory-ctl`
- Modify: `.claude/factory/tests/run.sh` (append after the Task 2 block, still before `# ---- /factory command ----`)
- Modify: `.claude/commands/factory.md`
- Modify: `.claude/software-factory.md`

**Interfaces:**
- Consumes: `compute_base_state()` from Task 2, the Sync flow, `ctl cancel`, `ctl cleanup`, `ctl lease` from Tasks 1 and 2, `ctl is-merged`, `ctl tick-plan`.
- Produces: `ctl merge-preflight <id>` printing `ok <main sha>` (exit 0) or `<token> [reason]` (exit 1) with token in `main-busy`, `main-moved`, `base-not-main`, `merged`. The command's merge step, `cancel` and `cleanup` subcommands, lease use and the rework note rule.

- [ ] **Step 1: Write the failing tests**

Append in `.claude/factory/tests/run.sh`, after the Task 2 block and before `# ---- /factory command ----`:

```bash
# ---- merge preflight and merge failure behavior ----
pr="$tmp/pr"
git init -q -b main "$pr"
gp() { git -C "$pr" -c user.email=t@t -c user.name=t "$@"; }
gp commit -q --allow-empty -m init
echo base >"$pr/f.txt"
gp add f.txt
gp commit -q -m file
lp() { FACTORY_ROOT="$pr" python3 "$ctl" "$@"; }
pbase="$(gp rev-parse main)"
lp new-job "Race" --date 2026-10-13 >/dev/null
gp worktree add -q "$tmp/pwt" -b factory/001-race
echo job >"$tmp/pwt/job.txt"
git -C "$tmp/pwt" add job.txt
git -C "$tmp/pwt" -c user.email=t@t -c user.name=t commit -q -m job
phead="$(git -C "$tmp/pwt" rev-parse HEAD)"
lp set-stage 1 approve --worktree "$tmp/pwt" --base "$pbase" --head "$phead" >/dev/null

check "ctl: merge-preflight is ok on a clean main" test "$(lp merge-preflight 1)" = "ok $pbase"
echo dirty >>"$pr/f.txt"
out="$(lp merge-preflight 1 2>/dev/null)"; rc=$?
check "ctl: merge-preflight says main-busy for tracked changes" bash -c "test '$rc' -eq 1 && case '$out' in main-busy*) true;; *) false;; esac"
gp checkout -q -- f.txt
gp switch -q -c other
out="$(lp merge-preflight 1 2>/dev/null)"; rc=$?
check "ctl: merge-preflight says main-busy when main is not checked out" bash -c "test '$rc' -eq 1 && case '$out' in main-busy*) true;; *) false;; esac"
gp switch -q main
echo more >"$pr/more.txt"
gp add more.txt
gp commit -q -m more
out="$(lp merge-preflight 1 2>/dev/null)"; rc=$?
check "ctl: merge-preflight says main-moved after main advanced" bash -c "test '$rc' -eq 1 && case '$out' in main-moved*) true;; *) false;; esac"

# A non-conflict merge failure must leave main untouched. Here an untracked file blocks the merge.
gp switch -q -c collide
echo a >"$pr/clash.txt"
gp add clash.txt
gp commit -q -m clash
collide="$(gp rev-parse HEAD)"
gp switch -q main
echo b >"$pr/clash.txt"
main_before="$(gp rev-parse HEAD)"
gp merge -q --no-ff -m "merge collide" "$collide" >/dev/null 2>&1
rc=$?
check "git: an untracked-file collision fails the merge" test "$rc" -ne 0
check "git: the failed merge leaves main HEAD unchanged" test "$(gp rev-parse HEAD)" = "$main_before"
check "git: the failed merge leaves no merge in progress" bash -c "! git -C '$pr' rev-parse -q --verify MERGE_HEAD"
check "git: the failed merge has no conflicted paths" test -z "$(gp diff --name-only --diff-filter=U)"

for word in 'merge-preflight' 'MERGE_HEAD' 'verbatim' 'rework-note.md' 'dev-workflow-skills:1-git-commit' \
  'fell back' 'cancel' 'cleanup' 'lease' 'merge failed' 'HEAD^2'; do
  grep -qF -- "$word" "$cmdfile" && pass "command mentions: $word" || fail "command mentions: $word"
done
```

Note: `cmdfile` is defined at the end of the Task 2 block. If you run this block alone, define `cmdfile="$repo/.claude/commands/factory.md"` first.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bash .claude/factory/tests/run.sh 2>&1 | grep -E 'FAIL|failed|all passed'`
Expected: `FAIL` for the four `merge-preflight` checks and for the new command words. The four plain `git:` checks already pass (they only describe git behavior the command text relies on).

- [ ] **Step 3: Implement `ctl merge-preflight`**

Add above `build_parser()` in `.claude/factory/factory-ctl`:

```python
def cmd_merge_preflight(args):
    root = repo_root()
    job = find_job(load_board(root), args.id)

    def refuse(token, reason=""):
        print(f"{token} {reason}".strip())
        sys.exit(1)

    if git(root, "branch", "--show-current").stdout.strip() != "main":
        refuse("main-busy", "the main checkout is not on main")
    if git(root, "status", "--porcelain", "--untracked-files=no").stdout.strip():
        refuse("main-busy", "the main checkout has tracked changes")
    if git(root, "rev-parse", "-q", "--verify", "MERGE_HEAD").returncode == 0:
        refuse("main-busy", "a merge is already in progress on main")
    state, main = compute_base_state(root, job)
    if state != "ok":
        refuse(state)
    print(f"ok {main}")
```

In `build_parser()` before `return parser`:

```python
    preflight = sub.add_parser("merge-preflight")
    preflight.add_argument("id")
    preflight.set_defaults(func=cmd_merge_preflight)
```

Run: `bash .claude/factory/tests/run.sh 2>&1 | grep -E 'FAIL'`
Expected: only the `command mentions:` lines still fail.

- [ ] **Step 4: Rewrite the merge step in `.claude/commands/factory.md`**

Replace the whole `## Merge step` section (including the one-line paragraph that Task 2 added) with the text below. Keep plan 01's rule that the merge uses the recorded head sha, not the branch name.

````markdown
## Merge step

1. `ctl is-merged <id>`. Exit 0 with `merged <sha>` means the user already merged it: skip to step 7 with that sha. Output `not merged` means continue. Any other output (the "no commits" or "no recorded head" error on stderr) means `ctl set-stage <id> needs-human --reason "<the error text>"`, then stop and tell the user.
2. Draft the merge commit message first, because it can take a while and the race check below must come right before the merge. See "Commit message" below.
3. `ctl merge-preflight <id>`. It checks that the main checkout is on `main`, has no tracked changes and no merge in progress, and that the job base is still right. The first word of its output decides:
   - `ok <main sha>`: remember `<main sha>` and go on at once to step 4.
   - `main-moved`: run the Sync flow once, then restart this merge step at step 1. A job that was already synced twice goes to `needs-human` ("main keeps moving").
   - `base-not-main`: `ctl set-stage <id> needs-human --reason "base is not on main (unrelated commits in the job)"` and stop.
   - `main-busy <why>`: `ctl set-stage <id> needs-human --reason "main busy (<why>)"` and stop. Tell the user they can merge by hand and run `/factory approve <id>` again.
4. Right away: `git -C "<main>" merge --no-ff <job head sha> -m "<subject>" -m "<body>"`. Use the recorded head sha, never the branch name. Never push. Never rebase or rewrite history.
5. If step 4 exits non-zero, the merge failed. Do exactly this, in order:
   1. If `git -C "<main>" rev-parse -q --verify MERGE_HEAD` succeeds, run `git -C "<main>" merge --abort`.
   2. `git -C "<main>" rev-parse HEAD` must equal `<main sha>` from step 3 and `git -C "<main>" status --porcelain --untracked-files=no` must be empty. If not, `ctl ledger <id> "Ruling: main differs after a failed merge, human check needed"`.
   3. If `git -C "<main>" diff --name-only --diff-filter=U` listed paths, the reason is `merge conflict`. Otherwise the reason is `merge failed: <first line of the git error>` (for example an untracked file that would be overwritten, or a hook failure).
   4. `ctl set-stage <id> needs-human --reason "<reason>"` and stop. Never retry the merge on your own.
6. If step 4 succeeded, check `git -C "<main>" rev-parse HEAD^2`. It must equal the job head sha. If not, `ctl set-stage <id> needs-human --reason "merge parent mismatch"` and stop with a warning, because main already moved. Otherwise the merge sha is `git -C "<main>" rev-parse HEAD`.
7. `ctl tick-plan <id> --merge <merge sha>`, then `ctl set-stage <id> merged`. The worktree and branch stay until the user runs `/factory cleanup <id>`.

### Commit message

Both the merge commit and the sync commit use this rule. Invoke the skill `dev-workflow-skills:1-git-commit` (Skill tool) with this request: "Draft the commit message for a no-ff merge of job <id> <slug>, commits <base7>..<head7>. Do not commit." Use its subject and body only if the subject matches `type(scope): text` with a type of feat, fix, chore, docs, test or refactor. If the skill is unavailable, fails or returns something else, use `chore(factory): merge job <id> <slug>` (for a sync: `chore(factory): sync job <id> <slug> with main`) and run `ctl ledger <id> "Ruling: merge message fell back to the conventional default (<reason>)"`.
````

- [ ] **Step 5: Add cancel, cleanup, lease and the rework note rule to the command**

In `.claude/commands/factory.md`:

1. Change the `argument-hint` line to `"<feature> | next | approve <job-id> | rework <job-id> <note> | cancel <job-id> | cleanup <job-id> | status"`.
2. In the paragraph that starts `Pick the subcommand by exact shape only:` add `cancel <digits>` and `cleanup <digits>` to the list of exact shapes.
3. Add this paragraph below the paragraph about the main checkout:

```markdown
Several sessions can share the board. Pick one owner name for this session the first time you need it, `orchestrator-<unix seconds>`, and reuse it. `ctl` takes a file lock for every write, so a call can fail with "the board is locked": wait a few seconds and retry once.
```

4. At the start of `## next`, after the job is picked, add: "Take the lease first: `ctl lease <id> --owner <owner> --ttl 1800`. If it fails because another owner holds it, report that and stop. When the stage is done, or you stop for any reason, `ctl lease <id> --owner <owner> --release`. A crashed session's lease expires by itself after 30 minutes."
5. Add these two sections above `## Sync flow`:

```markdown
## cancel `<job-id>`

For any job that is not `merged`. `ctl cancel <id> --reason "<text from the user, or none>"`. If the builder is running, tell the user to stop it, then add `--force`. A cancelled job shows in `needs-human` with the reason `cancelled: ...`, and `rework` refuses it (the board field `cancelled` is true). Nothing is deleted: tell the user they can run `/factory cleanup <id>`.

## cleanup `<job-id>`

Only for a job that is merged or cancelled. `ctl cleanup <id>`. It removes the job worktree and branch, and refuses when the worktree has uncommitted or untracked work, when the branch has commits that are not on `main`, or when the main checkout is on the job branch. For a cancelled job with unmerged commits, ask the user whether to drop them, and only then add `--discard-commits`. Report the ctl output.
```

6. Replace the rework section's note handling with the text below (keep plan 01's wording about the `plan.md` and spec route unchanged):

```markdown
A note of at most 200 characters on a single line is short. Pass a short note to the builder verbatim, as the whole SendMessage body: do not expand it, restate it or add instructions, even for notes like `all green`. Record it with `ctl ledger <id> "Ruling: rework note: <note>"`. Do not create `rework-note.md` for a short note. A longer note is saved as `rework-note.md` in the job directory and the builder gets that path. After the builder reply has been handled (the handoff lines were read, or the build stage stopped), remove the file: `rm -f "<job dir>/rework-note.md"`.
```

In the `build` stage step 1, replace "or `rework-note.md`" with "or the short rework note verbatim, or the path of `rework-note.md`".

- [ ] **Step 6: Sync the design doc `.claude/software-factory.md`**

1. In the `## Job model` list, add this bullet after the `Files per job` bullet group:

```markdown
- Concurrency: every mutating `factory-ctl` call runs under an `flock` on `.superpowers/factory/board.json.lock` (wait 15 seconds, `FACTORY_LOCK_WAIT` overrides it). A job has a `lease` (`owner`, `expires`) that the orchestrator takes for a stage with `ctl lease`, so two sessions do not work on one job. A crashed session's lease expires after 30 minutes.
- Lifecycle: `ctl cancel` parks a job in `needs-human` with `cancelled: true`. `ctl cleanup` removes the worktree and branch of a merged or cancelled job, and refuses on uncommitted work, unmerged commits (unless `--discard-commits`) or the main checkout's branch.
```

2. Replace the whole `## Merge step` section with the same seven steps as in `.claude/commands/factory.md` (shortened to prose, same order, same reason strings), and add a short `### Main moved` subsection: `ctl base-state` returns `ok`, `main-moved`, `base-not-main` or `merged`. `main-moved` runs the Sync flow: merge `main` into the job branch inside the job worktree (no rebase, no history rewrite), `ctl sync-base`, the builder reruns the checks, one `factory-code-reviewer` review of `ctl sync-diff`, then back to approve. A sync is allowed twice per job. `base-not-main` (unrelated commits baked into the base by `baseRef: head`) always goes to a human.
3. In `## Commands`, add `/factory cancel <job-id>` and `/factory cleanup <job-id>` bullets, and change the rework bullet to say a short note is passed verbatim and leaves no note file.
4. In the compatibility table, change the row `Peer sessions share the main checkout's git state` to say the merge step runs `ctl merge-preflight` right before the merge and aborts cleanly on any failure.
5. In `## Open items`, add: "Plan 03 of the v2 series built the lock, lease, cancel, cleanup, sync and merge failure handling. Real-run check still open: one sync on a real moved main."

- [ ] **Step 7: Run all tests and verify**

Run: `bash .claude/factory/tests/run.sh 2>&1 | grep -E 'FAIL|failed|all passed'`
Expected: `all passed` and no `FAIL`.

Run: `python3 -I -c "import ast,sys; ast.parse(open('.claude/factory/factory-ctl').read())" && python3 .claude/factory/factory-ctl --help | head -5`
Expected: no error, and the usage text lists the subcommands.

Run: `git status --short`
Expected: only these four files are modified: `.claude/factory/factory-ctl`, `.claude/factory/tests/run.sh`, `.claude/commands/factory.md`, `.claude/software-factory.md` (the spec file under `docs/superpowers/specs/` may show as untracked, leave it). If any fixture touched the real repo, stop and report.

Run: `git branch --list 'factory/*'`
Expected: the same list as before the plan (fixtures only used temp repos).

- [ ] **Step 8: Commit**

Invoke the skill `dev-workflow-skills:1-git-commit` (for example `feat(factory): harden the merge step and document the lifecycle`), then:

```bash
git add .claude/factory/factory-ctl .claude/factory/tests/run.sh .claude/commands/factory.md .claude/software-factory.md && git commit --only -m "<message from the skill>" -- .claude/factory/factory-ctl .claude/factory/tests/run.sh .claude/commands/factory.md .claude/software-factory.md
```

The commit gate runs no check for these file types (no `.ts`, `.tsx` or `.rs`), so it passes quickly. Never set `HARNESS_GATE=0`.

---

## Self-review (done by the plan author)

- Spec coverage: C1 (Sync flow, `sync-base`, `base-state`, the approve-stage check), C2 (`base-not-main` token, never synced), C3 (lock plus lease, Task 1), C4 (`merge-preflight`, failure steps, parent check, Task 3), C5 (`cancel`, `cleanup`), C6 (Commit message rule with the conventional fallback and ledger note), C7 (verbatim short note, no stray file), decisions 6 and 7, and the interface row for plan 03 (`sync-base`, `cancel`, `cleanup`, `flock` lock, `lease` field). Extra helpers `base-state`, `sync-diff`, `merge-preflight` are additions that plan 04 does not need.
- Plans 01 and 02 are used only through `ctl show`, `set-stage --force`, `record-checks` and the `CHECKS:` block. `prev_head` and `diff-range` are not touched, so a sync cannot confuse plan 01's re-review range.
- Types: `compute_base_state()` returns `(token, main_sha)` everywhere; `append_ledger(root, job, line)`, `is_ancestor(root, older, newer)` and `resolve_commit(root, rev)` keep one signature across tasks; board fields are `lease`, `cancelled`, `syncs`, `cleaned`.

Next plan: docs/superpowers/plans/2026-10-13-software-factory-v2/plan-04-multi-plan-series.md
