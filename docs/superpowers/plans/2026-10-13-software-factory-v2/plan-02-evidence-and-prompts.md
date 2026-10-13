# Software Factory v2 Plan 02: Evidence and Agent Prompts

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. This plan is executed by a factory-builder subagent. Steps use checkbox (`- [ ]`) syntax for tracking.

> **Plan 01 of this series (`plan-01-orchestrator-robustness.md`) must be finished and merged first.** This plan uses its `ctl show`, `prev_head`, `diff-range` and `clear-builder` only, and edits the command file that plan 01 already changed. Read the current `.claude/commands/factory.md` before editing it, and apply each command edit by the step's meaning if plan 01 moved the wording.

**Goal:** Make "checks passed" recorded evidence, give the approver the context it needs, and fix the builder, spec-writer and reviewer prompts that cost job 001 its manual interventions.

**Architecture:** The builder prints a `CHECKS:` block in its reply. The orchestrator saves it with the new `ctl record-checks`, which stores `checks.md` in the job directory. The approver reads `spec.md`, `checks.md`, the worktree path and the final round, and escalates on missing or failing checks. Builder prompt rules (no ledger writes, no sleeping, no post-approval checks, commit fallback) are pinned by lint tests in `tests/run.sh`.

**Tech Stack:** Python 3 stdlib (`factory-ctl`), bash (`tests/run.sh`), Claude Code agent and command markdown files.

**Spec:** `docs/superpowers/specs/2026-10-13-software-factory-v2-design.md`, section B (gaps B1-B10), decisions 3, 4, 5, 8, and the "Interfaces between plans" row for plan 02.

## Global Constraints

- All `cargo` commands pass `-j4`. No `cargo test --workspace` or `--all`. This plan runs no cargo command at all.
- Commits use the plugin skill `dev-workflow-skills:1-git-commit`, conventional commit format, and a pathspec commit: `git add <paths> && git commit --only -m "..." -- <paths>`. Never `git add -A`, `--all` or `.`.
- Never set `HARNESS_GATE=0`. Never push.
- Decision 8: ledger writes go through `ctl` only. Builders and reviewers never write the ledger.
- Decision 4: checks are evidence. The builder writes the exact commands and results in a `CHECKS:` block, the orchestrator saves it through ctl, the approver escalates when it is missing or any line fails.
- Decision 5: test-only dev-dependencies do not escalate unless `Cargo.lock` gains a crate or the dependency is not under `[dev-dependencies]`.
- Decision 3: the builder runs `superpowers:executing-plans` when the Skill tool works in the subagent, otherwise it follows the plan inline. It never fails because a helper cannot spawn.
- Interface produced: `ctl record-checks <id> --file <path>` writes `checks.md` in the job directory.
- Agent frontmatter keeps the models fixed: spec-writer opus, builder opus, security-reviewer opus, ux, ui and code reviewers sonnet, approver opus. Read-only agents keep tools exactly `Read, Grep, Glob`.
- Reviewer last line stays exactly `VERDICT: APPROVE` or `VERDICT: CHANGES`. Approver last line stays exactly `DECISION: APPROVE` or `DECISION: ESCALATE`.
- Code comments: short full sentences that end with a period. No emojis.

## Review Focus

- A `checks.md` that records a `FAIL` line must still be saved (the approver needs to see it) and must be reported as failing. Tested in Task 2.
- A checks file with no `PASS` or `FAIL` line (garbage or an empty reply) must be rejected and must not overwrite an earlier good `checks.md`. Tested in Task 2.
- A builder reply with no `CHECKS:` block must not get an invented one. The orchestrator records nothing and the approver escalates. Pinned in the command text (Task 2) and the approver prompt (Task 3).
- A builder that cannot run the commit skill must leave the work uncommitted and name the paths, so the orchestrator can commit. A reply with `UNCOMMITTED:` and a `HEAD` equal to `BASE` must not be treated as "builder made no commits". Pinned in Tasks 1 and 2.
- A dev-dependency in `Cargo.toml` that also adds a crate to `Cargo.lock`, or sits outside `[dev-dependencies]`, must still escalate. Pinned in Task 3.

---

### Task 1: Builder and spec-writer prompts

**Files:**
- Modify: `.claude/factory/tests/run.sh` (add a prompt-rules section before `# ---- factory-ctl ----`)
- Modify: `.claude/agents/factory-builder.md` (full body replacement plus `maxTurns`)
- Modify: `.claude/agents/factory-spec-writer.md` (step 4)

**Interfaces:**
- Consumes: nothing from plan 01.
- Produces: the builder reply contract that Task 2 parses. The reply ends with, in this order: an optional `UNCOMMITTED: <path> <path> ...` line, then a block that starts with a line `CHECKS:` and has one `PASS <command>` or `FAIL <command>` line per check, then the four lines `WORKTREE:`, `BRANCH:`, `BASE:`, `HEAD:`. The CHECKS block ends at the line that starts with `WORKTREE:`.

- [ ] **Step 1: Write the failing lint tests**

In `.claude/factory/tests/run.sh`, insert this block directly before the line `# ---- factory-ctl ----`:

```bash
# ---- prompt rules ----
# Usage: prompt_has <agent> <text>. Pins one rule in an agent prompt.
prompt_has() {
  grep -qF -- "$2" "$agents/$1.md" && pass "prompt $1: $2" || fail "prompt $1: $2"
}

[ "$(fm "$agents/factory-builder.md" maxTurns)" = "150" ] && pass "agent builder: maxTurns is 150" || fail "agent builder: maxTurns is 150"
prompt_has factory-builder 'superpowers:executing-plans'
prompt_has factory-builder 'superpowers:test-driven-development'
prompt_has factory-builder 'superpowers:verification-before-completion'
prompt_has factory-builder 'superpowers:systematic-debugging'
prompt_has factory-builder 'superpowers:receiving-code-review'
prompt_has factory-builder 'yarn test <pattern>'
prompt_has factory-builder 'cargo check -j4 --manifest-path tools/factory-server/Cargo.toml'
prompt_has factory-builder 'CHECKS:'
prompt_has factory-builder 'PASS <command>'
prompt_has factory-builder 'UNCOMMITTED:'
prompt_has factory-builder 'Never write to `progress.md`'
prompt_has factory-builder 'Never sleep'
prompt_has factory-builder 'already in your context'
prompt_has factory-builder 'final whole-branch code review'
prompt_has factory-builder 'clippy'
prompt_has factory-builder 'git rev-parse --show-toplevel'
prompt_has factory-spec-writer 'yarn test <pattern>'
prompt_has factory-spec-writer 'cargo check -j4 --manifest-path tools/factory-server/Cargo.toml'
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bash .claude/factory/tests/run.sh | grep -E '^FAIL: (prompt|agent builder: maxTurns)'`
Expected: a `FAIL:` line for every new check, and no `PASS:` line for any check added in this task.

- [ ] **Step 3: Replace the builder prompt**

Replace the whole file `.claude/agents/factory-builder.md` with this text. Only the `maxTurns` line is new in the frontmatter.

````markdown
---
name: factory-builder
description: Implements a factory plan task by task in an isolated worktree for Rocket, and fixes reviewer CHANGES when resumed. Use after the spec writer.
model: opus
isolation: worktree
maxTurns: 150
---
You are the builder in Rocket's software factory. You work in your own git worktree.

Input: the absolute path of the job directory, the branch name to use, and on a resume the findings file or note to fix.

Your worktree is created by the harness, so its path is not predictable and never has the form `factory-NNN-<slug>`. Find it with `git rev-parse --show-toplevel` and report exactly that path in the `WORKTREE:` line. If the orchestrator names an existing worktree for the branch, use that one with absolute paths and `git -C <that worktree>`.

First run:

1. Create the job branch: `git switch -c <branch>`. Note the starting commit as BASE. If the branch already exists, do not create it. If it is checked out in an existing worktree you were told about, work in that worktree. If it exists but is not checked out anywhere, run `git switch <branch>`.
2. `CLAUDE.md` and the files in `.claude/rules/` are already in your context, so do not read them again. Read the `CLAUDE.md` of each crate you touch (`crates/<name>/CLAUDE.md`, these are not preloaded), and the job's `spec.md` and `plan.md`. The job directory is outside your worktree, so use the absolute paths you were given. Never edit `plan.md`.
3. Do the plan task by task. Try the Skill tool with `superpowers:executing-plans` and follow it. Ignore any step of that skill that merges, pushes, opens a pull request or finishes the branch, because the orchestrator does those. Also skip its final whole-branch code review, its workspace and ledger scripts and its progress file, because the factory's reviewers and ctl do those, and a second review would cost tokens for no new evidence. Do not open the skills' bundled sub-documents unless a check fails and you need them. If the Skill tool or a helper subagent is not available, follow `plan.md` inline. Never stop because a helper cannot start. For each task write the failing test first, then the code, then run the task's checks.

   Superpowers skills the builder uses (try each with the Skill tool, and if one is not available follow the same rule inline):
   - `superpowers:test-driven-development` for every task that adds or changes behavior: failing test first, minimal code, rerun.
   - `superpowers:systematic-debugging` when a check or test fails. Find the root cause before you change code. Do not guess-fix.
   - `superpowers:verification-before-completion` before you print the `CHECKS:` block. Run the commands and read their output. Never print a `PASS` you did not see.
   - Do not use `superpowers:using-git-worktrees` (the harness made your worktree), `superpowers:finishing-a-development-branch` or `superpowers:requesting-code-review` (the orchestrator runs the reviewers and the merge), or `superpowers:brainstorming` and `superpowers:writing-plans` (the plan is fixed).

Checks. Run the checks the plan lists for each task. Choose from:

- Rust: `cargo check -j4` and `cargo test -j4 -p <crate> <test_name>`. Never use `--workspace` or `--all`.
- Frontend: `yarn tsc --noEmit`, `yarn check` and `yarn test <pattern>` for the test files you touched.
- Tasks that touch `tools/factory-server/`: `cargo check -j4 --manifest-path tools/factory-server/Cargo.toml`. The commit gate does not cover that crate.
- Run `yarn install --frozen-lockfile` once in the worktree before the first `yarn tsc` or `yarn check`. A fresh worktree has no `node_modules`, and the commit gate would deny every frontend commit.
- Always run `cargo check -j4` yourself before committing, so the commit gate starts from a warm target.
- Once the plan's checks pass and the work is committed, stop. Do not run clippy, release builds or extra suites after that. Never sleep or poll while you wait for a helper agent. Use its result when it returns, or carry on with the next piece of work.

Rocket hard rules:

- Never set `CARGO_TARGET_DIR`. Your worktree uses its own target directory.
- shadcn/ui primitives only, lucide-react icons only, `SingleLineEditor` for single-line variable-aware fields, Monaco for multi-line editors, narrow Zustand selectors.
- Rust: no unwrap in production paths, no git CLI, `#[serde(rename_all = "camelCase")]` on IPC DTOs only.
- Commit each task with the `dev-workflow-skills:1-git-commit` skill. Use a pathspec commit: `git add <paths> && git commit --only -m "..." -- <paths>`. Never `git add -A`, `--all` or `.`. Never set `HARNESS_GATE=0`. Never run a bare `git stash`.
- If the commit skill cannot run in your context, do not write a freeform commit message. Leave that task's files uncommitted and print one line `UNCOMMITTED: <path> <path> ...` (repo-relative paths, separated by single spaces) before the CHECKS block. The orchestrator commits them.
- Never write to `progress.md`, the ledger or any file in the job directory. The orchestrator owns them through ctl. Put anything the orchestrator should know in your reply text.
- Never push and never merge. The orchestrator does both.

On a resume: use `superpowers:receiving-code-review` (or follow it inline). Check each finding against the code before you act, fix what is correct, and say in your reply which finding you rejected and why instead of agreeing blindly. Fix only the listed findings, with new commits. Do not rewrite or amend earlier commits. Findings marked minor are optional: fix them only if the change is small. Rerun the plan's checks and print a fresh CHECKS block.

Your reply ends with these parts, in this order. The `UNCOMMITTED:` line only appears when needed. The CHECKS block has one line per check you ran, in the form `PASS <command>` or `FAIL <command>`, with a short result after it, for example `PASS cargo test -j4 -p rocket-infra mutual_tls_in_process (4 passed)`. Never write a PASS you did not see. A check you did not run is not listed.

UNCOMMITTED: <paths>
CHECKS:
PASS <command> (<short result>)
FAIL <command> (<short result>)

The last four lines must be exactly:

WORKTREE: <absolute path of your worktree>
BRANCH: <branch name>
BASE: <sha the branch started from>
HEAD: <sha of your last commit>

If you could not finish, say why above those lines and still print them.
````

- [ ] **Step 4: Update the spec-writer prompt**

In `.claude/agents/factory-spec-writer.md`, replace step 4's check sentence. Find:

```
   Each task names the exact files, a test-first step, and the checks to run: `cargo check -j4`, `cargo test -j4 -p <crate> <test_name>`, `yarn tsc --noEmit`, `yarn check`. Never use `--workspace`.
```

Replace with:

```
   Each task names the exact files, a test-first step, and the checks to run, chosen from: `cargo check -j4`, `cargo test -j4 -p <crate> <test_name>`, `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern>`. A task that touches `tools/factory-server/` also lists `cargo check -j4 --manifest-path tools/factory-server/Cargo.toml`, because the commit gate does not cover that crate. Never use `--workspace`. The builder records every listed check in its reply, so list only checks that matter.
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `bash .claude/factory/tests/run.sh | grep -E '^(FAIL|all passed|[0-9]+ failed)'`
Expected: `all passed` and no `FAIL:` lines.

- [ ] **Step 6: Commit**

Invoke the skill `dev-workflow-skills:1-git-commit`, then commit with a pathspec:

```bash
git add .claude/factory/tests/run.sh .claude/agents/factory-builder.md .claude/agents/factory-spec-writer.md && git commit --only -m "feat(factory): builder checks block, commit fallback and prompt rules" -- .claude/factory/tests/run.sh .claude/agents/factory-builder.md .claude/agents/factory-spec-writer.md
```

---

### Task 2: `ctl record-checks` and command wiring

**Files:**
- Modify: `.claude/factory/factory-ctl` (add `cmd_record_checks` and its parser entry)
- Modify: `.claude/factory/tests/run.sh` (ctl tests and command lint)
- Modify: `.claude/commands/factory.md` (build, review, approve and merge steps)

**Interfaces:**
- Consumes: the builder reply contract from Task 1 (`UNCOMMITTED:` line, `CHECKS:` block, four handoff lines). From plan 01: `ctl show`, `prev_head`, `diff-range`, `clear-builder` exist. This task does not call them.
- Produces: `ctl record-checks <id> --file <path>`. It reads the file, requires at least one line that starts with `PASS ` or `FAIL `, drops a leading `CHECKS:` line, writes `checks.md` in the job directory atomically (header `# Checks`, a blank line, then the lines), and prints `checks recorded: <n> pass, <m> fail`. Exit 0 even when `<m>` is above zero. Exit 1 with a message on stderr when the file is unreadable or has no result line, and the existing `checks.md` is left alone.

- [ ] **Step 1: Write the failing ctl tests**

In `.claude/factory/tests/run.sh`, insert this block directly after the line `check "ctl: root prints FACTORY_ROOT when set" test "$(python3 "$ctl" root)" = "$fr"`:

```bash
checks_in="$tmp/checks-in.txt"
checks_out="$fr/.superpowers/sdd/$name/checks.md"
printf '%s\n' 'CHECKS:' 'PASS cargo check -j4' 'PASS cargo test -j4 -p rocket-infra mutual_tls (4 passed)' >"$checks_in"
out="$(python3 "$ctl" record-checks 1 --file "$checks_in" 2>&1)"; rc=$?
check "ctl: record-checks reports the counts" test "$out" = "checks recorded: 2 pass, 0 fail" -a "$rc" -eq 0
check "ctl: record-checks writes checks.md" test -f "$checks_out"
check "ctl: record-checks keeps the result lines" grep -q '^PASS cargo check -j4$' "$checks_out"
check "ctl: record-checks drops the CHECKS: marker line" bash -c "! grep -q '^CHECKS:' '$checks_out'"
printf '%s\n' 'PASS cargo check -j4' 'FAIL yarn tsc --noEmit (2 errors)' >"$checks_in"
out="$(python3 "$ctl" record-checks 1 --file "$checks_in" 2>&1)"; rc=$?
check "ctl: record-checks stores a FAIL run and exits 0" test "$out" = "checks recorded: 1 pass, 1 fail" -a "$rc" -eq 0
check "ctl: record-checks overwrites the earlier checks.md" grep -q '^FAIL yarn tsc --noEmit' "$checks_out"
checks_before="$(cat "$checks_out")"
printf '%s\n' 'all green, trust me' >"$checks_in"
check "ctl: record-checks rejects a file with no PASS or FAIL line" bash -c "! python3 '$ctl' record-checks 1 --file '$checks_in'"
check "ctl: a rejected checks file leaves checks.md unchanged" test "$checks_before" = "$(cat "$checks_out")"
: >"$checks_in"
check "ctl: record-checks rejects an empty file" bash -c "! python3 '$ctl' record-checks 1 --file '$checks_in'"
check "ctl: record-checks rejects a missing file" bash -c "! python3 '$ctl' record-checks 1 --file '$tmp/nope.txt'"
check "ctl: record-checks rejects an unknown job" bash -c "! python3 '$ctl' record-checks 99 --file '$checks_in'"
```

Then, in the `# ---- /factory command ----` section, insert this block directly before the line `# ---- summary ----`:

```bash
for word in 'record-checks' 'checks.md' 'CHECKS:' 'UNCOMMITTED:' 'dev-workflow-skills:1-git-commit' \
  'UNRESOLVED MINORS' 'harness creates' 'no CHECKS block'; do
  grep -qF -- "$word" "$cmd" && pass "command mentions: $word" || fail "command mentions: $word"
done
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bash .claude/factory/tests/run.sh | grep -E '^FAIL: (ctl: record-checks|command mentions)'`
Expected: `FAIL:` for the ctl cases (argparse rejects the unknown subcommand) and for the eight new command words.

- [ ] **Step 3: Implement `cmd_record_checks`**

In `.claude/factory/factory-ctl`, add this function directly after `cmd_ledger`:

```python
def cmd_record_checks(args):
    root = repo_root()
    job = find_job(load_board(root), args.id)
    try:
        with open(args.file, encoding="utf-8") as handle:
            text = handle.read()
    except OSError as err:
        die(f"cannot read {args.file}: {err.strerror}")
    lines = [line.rstrip() for line in text.splitlines() if line.strip()]
    if lines and lines[0] == "CHECKS:":
        lines = lines[1:]
    passed = sum(1 for line in lines if re.match(r"PASS \S", line))
    failed = sum(1 for line in lines if re.match(r"FAIL \S", line))
    if passed + failed == 0:
        die("no PASS or FAIL lines in the checks file")
    base = job_dir(root, job)
    fd, tmp = tempfile.mkstemp(dir=base, suffix=".tmp")
    with os.fdopen(fd, "w", encoding="utf-8") as handle:
        handle.write("# Checks\n\n" + "\n".join(lines) + "\n")
    os.replace(tmp, os.path.join(base, "checks.md"))
    print(f"checks recorded: {passed} pass, {failed} fail")
```

In `build_parser`, directly after the `ledger.set_defaults(func=cmd_ledger)` line, add:

```python
    record = sub.add_parser("record-checks")
    record.add_argument("id")
    record.add_argument("--file", required=True)
    record.set_defaults(func=cmd_record_checks)
```

- [ ] **Step 4: Edit the command text**

Apply these edits to `.claude/commands/factory.md`. Plan 01 may have renumbered steps or reworded sentences, so find each place by the quoted start and keep plan 01's other additions.

(a) Stage `build`, step 1, first run. Directly after the sentence ending `and the job branch name.` add:

```
Tell the builder that the harness creates its worktree, so it must not expect a path and must report the real one in the `WORKTREE:` line. On a resume, also tell it to rerun the plan's checks and print a fresh `CHECKS:` block.
```

(b) Stage `build`, step 3 ("Read the last four lines of the reply"). Replace the whole step with:

```
3. Parse the reply. First look for an `UNCOMMITTED:` line. If it is there, the builder could not run the commit skill, so you commit its files. Invoke the skill `dev-workflow-skills:1-git-commit` for the message, then run this in the builder worktree (the only git command you run there): `git -C "<worktree>" add <paths> && git -C "<worktree>" commit --only -m "<message>" -- <paths>`. If the skill cannot run for you either, use a conventional message such as `feat(<area>): <task title>` and run `ctl ledger <id> "Ruling: orchestrator commit used a plain conventional message"`. Then take `HEAD` from `git -C "<worktree>" rev-parse HEAD` instead of the reply. Next read the last four lines: `WORKTREE:`, `BRANCH:`, `BASE:`, `HEAD:`. If any is missing, `ctl set-stage <id> needs-human --reason "builder gave no handoff lines"` and stop. If `HEAD` equals `BASE` (after any orchestrator commit), `ctl set-stage <id> needs-human --reason "builder made no commits"` and stop.
```

(c) Directly after that step add a new step:

```
3b. Save the evidence. Take the lines of the reply from `CHECKS:` up to, but not including, the line that starts with `WORKTREE:`. Write them with the Write tool to `<job dir>/checks-draft.txt`, run `ctl record-checks <id> --file "<job dir>/checks-draft.txt"`, then `rm "<job dir>/checks-draft.txt"`. If the reply has no `CHECKS:` block, or `ctl record-checks` exits non-zero, record nothing, never write a checks file yourself, and run `ctl ledger <id> "Ruling: builder gave no CHECKS block"`. The approver escalates on missing checks. A recorded run with `FAIL` lines is still saved.
```

(d) Stage `review`. In the step that combines findings into `changes-r<round+1>.md`, append:

```
List every `[blocker]` finding first. Then add a section `Minor (optional, fix only if the change is small)` with the `[minor]` findings of that round.
```

In the step that handles "All pass", append:

```
Keep the `[minor]` findings of the passing round in mind: the approver lists the unresolved ones.
```

(e) Stage `approve`. Replace the sentence `Dispatch \`factory-approver\` with the job directory, that diff path and all review files.` with:

```
Dispatch `factory-approver` with the job directory, that diff path, all `review-*.md` files, the path of `spec.md`, the path of `checks.md` (or the words `checks.md missing` if the file does not exist), the builder worktree path from the board, and the final round number from the board.
```

(f) Merge step, last step (`ctl tick-plan ...`). Append this sentence:

```
Then read the `UNRESOLVED MINORS:` section of `approval.md`. For each item, run `ctl ledger <id> "Minor: <item>"`, and list the items in your report to the user, so the minors are carried forward and not lost. Use no other ledger line form for them.
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `bash .claude/factory/tests/run.sh | grep -E '^(FAIL|all passed|[0-9]+ failed)'`
Expected: `all passed`.

- [ ] **Step 6: Commit**

Invoke the skill `dev-workflow-skills:1-git-commit`, then commit with a pathspec:

```bash
git add .claude/factory/factory-ctl .claude/factory/tests/run.sh .claude/commands/factory.md && git commit --only -m "feat(factory): record builder checks as evidence and commit fallback" -- .claude/factory/factory-ctl .claude/factory/tests/run.sh .claude/commands/factory.md
```

---

### Task 3: Approver and reviewer prompts, doc sync, verification

**Files:**
- Modify: `.claude/factory/tests/run.sh` (approver and reviewer lint)
- Modify: `.claude/agents/factory-approver.md` (full body replacement)
- Modify: `.claude/agents/factory-security-reviewer.md`, `factory-ux-reviewer.md`, `factory-ui-reviewer.md`, `factory-code-reviewer.md` (one paragraph each)
- Modify: `.claude/software-factory.md`

**Interfaces:**
- Consumes: `checks.md` format from Task 2 (`PASS ...` and `FAIL ...` lines). The approver input list from the command edit (e) in Task 2.
- Produces: reviewer findings tagged `[blocker]` or `[minor]` (the command text in Task 2 relies on the tags). `approval.md` with a `UNRESOLVED MINORS:` section just above the `DECISION:` line (Task 2 merge step relies on it).

- [ ] **Step 1: Write the failing lint tests**

In `.claude/factory/tests/run.sh`, insert this block directly after the `prompt_has factory-spec-writer ...` lines added in Task 1 (still before `# ---- factory-ctl ----`):

```bash
prompt_has factory-approver 'checks.md'
prompt_has factory-approver 'spec.md'
prompt_has factory-approver 'worktree'
prompt_has factory-approver 'final round'
prompt_has factory-approver '[dev-dependencies]'
prompt_has factory-approver 'Cargo.lock'
prompt_has factory-approver 'UNRESOLVED MINORS:'
for role in security ux ui code; do
  prompt_has "factory-$role-reviewer" '[minor]'
  prompt_has "factory-$role-reviewer" '[blocker]'
done
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bash .claude/factory/tests/run.sh | grep -E '^FAIL: prompt factory-(approver|[a-z]+-reviewer)'`
Expected: `FAIL:` lines for the approver and for all four reviewers.

- [ ] **Step 3: Replace the approver prompt**

Replace the whole file `.claude/agents/factory-approver.md` with:

````markdown
---
name: factory-approver
description: Final approver for a Rocket factory job. Reads all review reports and the diff, then decides APPROVE (merge) or ESCALATE (needs a human). Read-only.
model: opus
tools: Read, Grep, Glob
maxTurns: 30
---
You are the approver in Rocket's software factory. You are read-only.

Input: the job directory, the full diff file path, the review report files (`review-*.md`), the path of `spec.md`, the path of `checks.md` (or the words `checks.md missing`), the builder worktree path, and the final round number. Use the worktree to read surrounding code, for example the serde struct a diff line sits in, and `Cargo.lock`. Read `spec.md` before you judge the acceptance criteria.

Decide ESCALATE if any of these is true, otherwise APPROVE:

- `checks.md` is missing or has no `PASS` line. The builder's reply is not evidence, only `checks.md` is.
- `checks.md` has any `FAIL` line, or it does not list the checks the diff needs: `cargo check -j4` for any `.rs` or `Cargo.toml` change, `yarn tsc --noEmit` and `yarn check` for any `.ts` or `.tsx` change, `cargo check -j4 --manifest-path tools/factory-server/Cargo.toml` for any change under `tools/factory-server/`, and a test run that names the tests the diff adds.
- the diff touches the collection trust gate, auth, secrets, or `{{process.env}}` handling
- the diff changes a persistence format: `.yml` schemas or serde persistence structs
- the diff changes the Tauri IPC surface or adds or changes `DomainEvent` variants
- the diff changes `Cargo.toml`, `package.json`, `tauri.conf.json`, the CSP, or the harness hooks. One exception: a `Cargo.toml` change that only adds entries under `[dev-dependencies]` does not escalate when `Cargo.lock` in the worktree gains no new crate. Check that the diff has no `Cargo.lock` hunk that adds a `[[package]]` entry, and that the new lines in `Cargo.toml` are below a `[dev-dependencies]` header. A new crate in `Cargo.lock`, or a dependency outside `[dev-dependencies]`, escalates as before. Never apply the exception to `package.json`.
- a reviewer reported CHANGES that was not fixed
- any report of the final round (`review-*-r<final round>.md`) has no `VERDICT:` line. Older rounds were already handled, so ignore them for this rule.
- the spec's acceptance criteria are not all covered by the diff and its tests

Write a short reasoned summary: which rules you checked and what you found, with `file:line` for anything that triggered ESCALATE. When you ESCALATE, the first line of the summary is a one-line reason.

Then list every `[minor]` finding from the review reports of the final round that the diff does not fix. Use this exact heading, one item per line starting with `- `, and write `- none` when there are none:

UNRESOLVED MINORS:
- <role> <file:line> <finding>

Your last line, alone, must be exactly one of:

DECISION: APPROVE
DECISION: ESCALATE
````

- [ ] **Step 4: Add the finding tags to the four reviewers**

In each of the four files, insert the paragraph below on its own, followed by a blank line, directly before the line `Your last line, alone, must be exactly one of:`. The files are `.claude/agents/factory-security-reviewer.md`, `factory-ux-reviewer.md`, `factory-ui-reviewer.md` and `factory-code-reviewer.md`.

```
Start every finding with its severity tag, `[blocker]` or `[minor]`, for example `1. [minor] src/foo.ts:12 ...`. The orchestrator and the approver read these tags: a `[blocker]` means CHANGES, and each `[minor]` is carried forward to the approver and the ledger.
```

- [ ] **Step 5: Sync the design doc**

Edit `.claude/software-factory.md`:

(a) In the "Job model" section, replace `worktree \`.claude/worktrees/factory-NNN-<slug>\` (already gitignored, \`settings.json\` has \`worktree.baseRef: head\`)` with:

```
worktree created by the harness under `.claude/worktrees/` (the name is `agent-<id>`, not predictable; the builder reports the real path in its `WORKTREE:` line and the board stores it; the folder is already gitignored and `settings.json` has `worktree.baseRef: head`)
```

(b) In the "Files per job" list, replace the line that begins with `  - \`.superpowers/sdd/YYYY-MM-DD-NNN-<slug>/spec.md\`` with:

```
  - `.superpowers/sdd/YYYY-MM-DD-NNN-<slug>/spec.md`, `plan.md`, `task-N-brief.md`, `review-<base>..<head>.diff`, `review-<role>-r<round>.md`, `changes-r<round>.md`, `checks.md` (the builder's recorded checks, written by `ctl record-checks`), `approval.md`.
  - Only the orchestrator writes the ledger, through `ctl`. Builders and reviewers never do. Ledger lines are `Task N: complete (...)`, `Ruling:` lines and `Minor:` lines (unresolved minor findings carried forward from the approval).
```

(c) In the Agents table, replace the builder row's Rocket duties cell with: `One implementer per worktree. Follows the matching rule file. Runs \`superpowers:executing-plans\` when the Skill tool works in the subagent, otherwise follows the plan inline. Runs the plan's checks and prints them as a \`CHECKS:\` block (\`PASS\` or \`FAIL\` per command). Commits with \`dev-workflow-skills:1-git-commit\` and a pathspec. If the skill cannot run it prints \`UNCOMMITTED: <paths>\` and the orchestrator commits. Stops after the checks, no clippy or cold builds, no sleeping on helpers. \`maxTurns: 150\`.`

(d) In the Agents table, replace the approver row's Rocket duties cell with: `Reads the spec, \`checks.md\`, all verdict files and the diff (worktree path and final round are given), applies the risk rules, writes \`approval.md\` with an \`UNRESOLVED MINORS:\` section. Escalates on missing or failing checks.`

(e) In "Approver risk rules", add two bullets after the bullet `- any reviewer \`CHANGES\` that was overridden instead of fixed`:

```
- `checks.md` missing, with no `PASS` line, with any `FAIL` line, or missing a check the diff needs
- (exception to the `Cargo.toml` rule) a change that only adds `[dev-dependencies]` entries and adds no crate to `Cargo.lock` does not escalate
```

(f) In "Open items", replace the line beginning `- \`Skill\` inside a subagent is only reported` with:

```
- `Skill` inside a subagent: the builder tries the git-commit skill. If it cannot run, the builder prints `UNCOMMITTED: <paths>` and the orchestrator commits with the skill (plain conventional message as a last resort, noted in the ledger).
```

- [ ] **Step 6: Run the full factory test suite**

Run: `bash .claude/factory/tests/run.sh | grep -E '^(FAIL|all passed|[0-9]+ failed)'`
Expected: `all passed` and no `FAIL:` lines.

- [ ] **Step 7: Check the files by eye and by script**

Run: `python3 -m py_compile .claude/factory/factory-ctl && echo compiled; for f in .claude/agents/factory-*.md; do head -1 "$f" | grep -qx -- '---' || echo "bad frontmatter $f"; done; git status --short`
Expected: `compiled`, no `bad frontmatter` line, and `git status --short` listing only the files this plan changed. If `py_compile` left a `__pycache__` folder under `.claude/factory/`, delete it, because the suite runs with `PYTHONDONTWRITEBYTECODE=1` and the folder must not be committed.

- [ ] **Step 8: Commit**

Invoke the skill `dev-workflow-skills:1-git-commit`, then commit with a pathspec:

```bash
git add .claude/factory/tests/run.sh .claude/agents/factory-approver.md .claude/agents/factory-security-reviewer.md .claude/agents/factory-ux-reviewer.md .claude/agents/factory-ui-reviewer.md .claude/agents/factory-code-reviewer.md .claude/software-factory.md && git commit --only -m "feat(factory): approver checks rule, dev-dependency exception and minor findings" -- .claude/factory/tests/run.sh .claude/agents/factory-approver.md .claude/agents/factory-security-reviewer.md .claude/agents/factory-ux-reviewer.md .claude/agents/factory-ui-reviewer.md .claude/agents/factory-code-reviewer.md .claude/software-factory.md
```

---

## Self-review notes

- B1: Tasks 1 (CHECKS block), 2 (`record-checks`, command), 3 (approver escalation). B2: Task 2 (e) and Task 3 approver input. B3: Task 3 approver rule and doc. B4: Task 1 builder and spec-writer. B5: Task 1 `maxTurns: 150` with lint. B6: Task 1 `UNCOMMITTED:` and Task 2 (b). B7: Task 1 "Never write to `progress.md`", Task 3 doc. B8: Task 3 reviewer tags and `UNRESOLVED MINORS:`, Task 2 (d) and (f). B9: Task 1 worktree wording, Task 2 (a), Task 3 doc (a). B10: Task 1 no sleeping and stop-after-checks rule.
- Decisions 3, 4, 5 and 8 appear in Global Constraints and in the tasks above.
- Names used across tasks: `record-checks`, `checks.md`, `checks-draft.txt`, `CHECKS:`, `UNCOMMITTED:`, `UNRESOLVED MINORS:`, `[blocker]`, `[minor]`.

Next plan: docs/superpowers/plans/2026-10-13-software-factory-v2/plan-03-main-moved-and-lifecycle.md
