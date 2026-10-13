# Software Factory v2 Plan 04: Multi-plan Series

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:executing-plans, executed by a factory-builder subagent. Steps use checkbox (`- [ ]`) syntax for tracking. Plans 01 to 03 of this series must be finished and merged first. This plan edits files those plans also changed (`factory-ctl`, `factory.md`, `run.sh`, the builder and spec-writer prompts). Re-read each file before you edit it and insert next to the existing code. Do not replace a file from the snippets below.

**Goal:** A feature that needs more than 3 tasks becomes one spec plus a chain of plans (at most 3 tasks each, each naming the next). The orchestrator runs the plans one after another, and each builder runs its plan through `superpowers:executing-plans` when the harness allows it.

**Architecture:** Task 1 probes headlessly whether a subagent can call the Skill tool and spawn a child agent, and records a dated ruling with builder wording for each outcome. Task 2 teaches the spec writer to emit `plans/plan-NN-*.md` files with `Next plan:` lines, and `factory-ctl` to validate that chain and track a `series` board field. Task 3 makes the orchestrator chain plans after each merge (never in parallel, stops on `needs-human`), changes the builder prompt, and syncs the docs.

**Tech Stack:** bash, Python 3 stdlib, `jq` in tests, `claude -p` (probe only), Claude Code agent and command markdown files.

**Spec:** [docs/superpowers/specs/2026-10-13-software-factory-v2-design.md](../../specs/2026-10-13-software-factory-v2-design.md), section D (D1 to D5, the dashboard chip is plan 05), decisions 1, 2 and 3, and the plan 04 row of "Interfaces between plans". Design doc: `.claude/software-factory.md`.

## Global Constraints

- Series plans run sequentially. No parallel builders in one worktree. One plan, one job, one builder at a time (decision 1).
- Chaining runs through the orchestrator. The `Next plan:` pointer is read by the orchestrator, which starts the next job (decision 2).
- The builder runs `superpowers:executing-plans` when the Skill tool works in the subagent. If the skill or a nested subagent is not available, the builder follows the plan inline. The builder never fails because a helper cannot spawn (decision 3).
- Each plan has at most 3 tasks. A plan file ends with a `Next plan: <path|none>` line.
- Interface produced by this plan, and the only names plan 05 may use: board field `series` with `id`, `index`, `total`, `next_plan`; `ctl new-job --series <id> --plan-file <path>`; `ctl next-plan <id>` prints the next plan path or `none`.
- From plans 01 to 03 this plan may use only: `ctl show <id>`, `ctl clear-builder`, `ctl set-stage` rejecting backward moves unless `--force`, ids from board plus directories plus branches, `ctl record-checks`, `ctl sync-base`, `ctl cancel`, `ctl cleanup`, the board lock and the `lease` field.
- All `cargo` commands pass `-j4`. Never `cargo test --workspace` or `--all`. (This plan needs no cargo run.)
- Commits use the plugin skill `dev-workflow-skills:1-git-commit` (Skill tool), conventional format, and a pathspec commit: `git add <paths> && git commit --only -m "..." -- <paths>`. Never `git add -A`, `--all` or `.`. Never set `HARNESS_GATE=0`. Never push.
- `board.json` is written only by `factory-ctl`. Ledger lines are written only through `ctl ledger` or ctl itself.
- Probe runs use scratch paths only (a `mktemp -d` directory outside the repo). The probe never touches the repo, `.superpowers/` or the event log.
- Code comments are short full sentences that end with a punctuation mark. No emojis.

## Review Focus

- A plan chain with a cycle (plan 02 points back to plan 01) must fail validation, not loop forever. Tested in Task 2.
- A plan with 4 tasks, no tasks, a missing file, a missing `Next plan:` last line, or a plan file that is not in the chain must fail validation with a clear message. Tested in Task 2.
- Starting plan k+1 while plan k is not merged (for example `needs-human`), or starting the same plan twice, must be refused. Tested in Task 2.
- A refused `new-job --series` must leave no orphan job folder and no board entry. Tested in Task 2.
- A probe that cannot run (no `claude` CLI, empty or garbage stream) must report `unknown`, never a false `yes`, and the builder prompt must still work when the Skill tool or nesting is missing. Tested in Tasks 1 and 3.

## File Structure

- Create `.claude/factory/probes/nesting-probe.sh`: headless probe runner (scratch dir, throwaway agent, `claude -p`).
- Create `.claude/factory/probes/classify-probe.py`: reads the probe stream and prints `SKILL=`, `NEST=`, `BASIS=` lines. Pure, testable offline.
- Modify `.claude/factory/factory-ctl`: `series` field, `new-job --series --plan-file`, `series-start`, `check-series`, `next-plan`, status suffix.
- Modify `.claude/factory/tests/run.sh`: tests for the classifier, the series commands and the prompt and command text.
- Modify `.claude/agents/factory-spec-writer.md`, `.claude/agents/factory-builder.md`, `.claude/commands/factory.md`, `.claude/software-factory.md`.

---

### Task 1: Nesting and Skill probe, and the ruling

**Files:**
- Create: `.claude/factory/probes/classify-probe.py`
- Create: `.claude/factory/probes/nesting-probe.sh`
- Modify: `.claude/factory/tests/run.sh` (insert a `# ---- probe classifier ----` section before the `# ---- summary ----` line)
- Modify: `.claude/software-factory.md` (new `## Nesting rulings` section, and the open item about `Skill` inside a subagent)

**Interfaces:**
- Consumes: the `claude` CLI on PATH (only for the real probe run, not for the tests).
- Produces: `nesting-probe.sh` prints three lines `SKILL=<yes|no|unknown>`, `NEST=<yes|no|unknown>`, `BASIS=<stream|self-report|none>`. A dated ruling in `.claude/software-factory.md` with the line `Observed outcome: A|B|C` that Task 3 reads. Outcomes: A = Skill works and nesting works, B = Skill works and nesting does not, C = Skill does not work or the probe result is unknown.

- [ ] **Step 1: Write the failing classifier tests**

Insert before the `# ---- summary ----` line of `.claude/factory/tests/run.sh`:

```bash
# ---- probe classifier ----
classify="$factory/probes/classify-probe.py"
pj="$tmp/probe"
mkdir -p "$pj"
outer_use='{"type":"assistant","parent_tool_use_id":null,"message":{"content":[{"type":"tool_use","id":"t0","name":"Agent","input":{}}]}}'
sub_skill_use='{"type":"assistant","parent_tool_use_id":"t0","message":{"content":[{"type":"tool_use","id":"t1","name":"Skill","input":{}}]}}'
sub_skill_ok='{"type":"user","parent_tool_use_id":"t0","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"loaded"}]}}'
sub_nest_use='{"type":"assistant","parent_tool_use_id":"t0","message":{"content":[{"type":"tool_use","id":"t2","name":"Agent","input":{}}]}}'
sub_nest_ok='{"type":"user","parent_tool_use_id":"t0","message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":"PONG"}]}}'
sub_nest_err='{"type":"user","parent_tool_use_id":"t0","message":{"content":[{"type":"tool_result","tool_use_id":"t2","is_error":true,"content":"no such tool"}]}}'
sub_read_use='{"type":"assistant","parent_tool_use_id":"t0","message":{"content":[{"type":"tool_use","id":"t3","name":"Read","input":{}}]}}'

printf '%s\n' "$outer_use" "$sub_skill_use" "$sub_skill_ok" "$sub_nest_use" "$sub_nest_ok" >"$pj/both.jsonl"
check "classifier: skill and nesting both succeed in the stream" \
  test "$(python3 "$classify" "$pj/both.jsonl" | paste -sd' ')" = "SKILL=yes NEST=yes BASIS=stream"
printf '%s\n' "$outer_use" "$sub_skill_use" "$sub_skill_ok" "$sub_nest_use" "$sub_nest_err" >"$pj/noNest.jsonl"
check "classifier: a failed child spawn gives NEST=no" \
  test "$(python3 "$classify" "$pj/noNest.jsonl" | paste -sd' ')" = "SKILL=yes NEST=no BASIS=stream"
printf '%s\n' "$outer_use" "$sub_read_use" >"$pj/neither.jsonl"
check "classifier: a subagent that never calls the tools gives no and no" \
  test "$(python3 "$classify" "$pj/neither.jsonl" | paste -sd' ')" = "SKILL=no NEST=no BASIS=stream"
printf '%s\n' "$outer_use" '{"type":"result","result":"SKILL_RESULT: ok\nNEST_RESULT: failed boom"}' >"$pj/self.jsonl"
check "classifier: with no subagent events it falls back to the self-report" \
  test "$(python3 "$classify" "$pj/self.jsonl" | paste -sd' ')" = "SKILL=yes NEST=no BASIS=self-report"
: >"$pj/empty.jsonl"
check "classifier: an empty stream is unknown" \
  test "$(python3 "$classify" "$pj/empty.jsonl" | paste -sd' ')" = "SKILL=unknown NEST=unknown BASIS=none"
printf 'garbage\n{not json\n' >"$pj/garbage.jsonl"
check "classifier: a garbage stream is unknown, never yes" \
  test "$(python3 "$classify" "$pj/garbage.jsonl" | paste -sd' ')" = "SKILL=unknown NEST=unknown BASIS=none"
check "probe: the runner script is executable" test -x "$factory/probes/nesting-probe.sh"
check "probe: the runner script parses" bash -n "$factory/probes/nesting-probe.sh"
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bash .claude/factory/tests/run.sh 2>&1 | grep -E 'classifier|probe|failed|passed'`
Expected: the new checks print `FAIL` (the files do not exist) and the last line is `N failed`.

- [ ] **Step 3: Write the classifier**

Create `.claude/factory/probes/classify-probe.py`:

```python
#!/usr/bin/env python3
"""Classify a stream-json log written by nesting-probe.sh.

Prints three lines: SKILL=<yes|no|unknown>, NEST=<yes|no|unknown> and
BASIS=<stream|self-report|none>. Messages with a parent_tool_use_id come from
a subagent. The probe agent is a subagent, so its own tool calls count. The
orchestrator's call that starts the probe agent has no parent and is ignored.
"""
import json
import re
import sys


def load(path):
    events = []
    with open(path, encoding="utf-8", errors="replace") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            try:
                value = json.loads(line)
            except json.JSONDecodeError:
                continue
            if isinstance(value, dict):
                events.append(value)
    return events


def blocks(event):
    message = event.get("message")
    if not isinstance(message, dict):
        return []
    content = message.get("content")
    if not isinstance(content, list):
        return []
    return [block for block in content if isinstance(block, dict)]


def from_stream(nested, names):
    """Return yes if a subagent call of one of the tools succeeded, else no."""
    used = set()
    for event in nested:
        for block in blocks(event):
            if block.get("type") == "tool_use" and block.get("name") in names:
                used.add(block.get("id"))
    for event in nested:
        for block in blocks(event):
            if block.get("type") == "tool_result" and block.get("tool_use_id") in used:
                if not block.get("is_error"):
                    return "yes"
    return "no"


def from_marker(events, marker):
    """Read SKILL_RESULT or NEST_RESULT from the final result text."""
    for event in reversed(events):
        if event.get("type") != "result":
            continue
        text = event.get("result")
        if not isinstance(text, str):
            continue
        found = re.search(rf"{marker}:\s*(ok|failed)", text)
        if found:
            return "yes" if found.group(1) == "ok" else "no"
    return "unknown"


def classify(events):
    nested = [event for event in events if event.get("parent_tool_use_id")]
    if nested:
        return from_stream(nested, {"Skill"}), from_stream(nested, {"Agent", "Task"}), "stream"
    skill = from_marker(events, "SKILL_RESULT")
    nest = from_marker(events, "NEST_RESULT")
    if skill == "unknown" and nest == "unknown":
        return "unknown", "unknown", "none"
    return skill, nest, "self-report"


def main():
    if len(sys.argv) != 2:
        print("usage: classify-probe.py <stream.jsonl>", file=sys.stderr)
        sys.exit(2)
    skill, nest, basis = classify(load(sys.argv[1]))
    print(f"SKILL={skill}")
    print(f"NEST={nest}")
    print(f"BASIS={basis}")


if __name__ == "__main__":
    main()
```

- [ ] **Step 4: Write the probe runner**

Create `.claude/factory/probes/nesting-probe.sh` and run `chmod +x` on it and on the classifier:

```bash
#!/usr/bin/env bash
# Headless probe: can a subagent call the Skill tool and spawn a child agent?
# It runs in a throwaway temp directory and never touches the repo.
# Set PROBE_KEEP=<dir> to keep the raw stream for evidence.
set -u

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

unknown() {
  echo "SKILL=unknown"
  echo "NEST=unknown"
  echo "BASIS=none"
  echo "nesting-probe: $1" >&2
  exit 0
}

command -v claude >/dev/null 2>&1 || unknown "the claude CLI is not on PATH"

work="$(mktemp -d "${TMPDIR:-/tmp}/factory-probe.XXXXXX")"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/.claude/agents"

cat >"$work/.claude/agents/probe-agent.md" <<'EOF'
---
name: probe-agent
description: Throwaway probe agent. Checks whether a subagent can use the Skill tool and spawn a child agent.
model: haiku
---
You are a probe. Do exactly these two steps and nothing else.

1. Call the Skill tool with skill "superpowers:executing-plans". After it returns, do not follow what the skill says. Print "SKILL_RESULT: ok" if the call returned content, or "SKILL_RESULT: failed <error>" if it errored or the tool does not exist.
2. Call the Agent tool once to start a general-purpose child agent with the prompt "Reply with the single word PONG.". Print "NEST_RESULT: ok" if it returned, or "NEST_RESULT: failed <error>" if it errored or the tool does not exist.

End your reply with those two lines only.
EOF

stream="$work/stream.jsonl"
(
  cd "$work" && claude -p "Run the probe-agent subagent. Reply with exactly the two result lines it prints." \
    --output-format stream-json --verbose --max-turns 12 \
    --allowedTools "Agent Task Skill" >"$stream" 2>"$work/stderr.txt"
) || true

if [ -n "${PROBE_KEEP:-}" ]; then
  mkdir -p "$PROBE_KEEP" && cp "$stream" "$PROBE_KEEP/stream.jsonl"
fi

echo "DATE=$(date +%F)"
echo "CLAUDE=$(claude --version 2>/dev/null | head -n1)"
python3 "$here/classify-probe.py" "$stream"
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `bash .claude/factory/tests/run.sh 2>&1 | grep -E 'classifier|probe|failed|all passed'`
Expected: every new check prints `PASS`, and the last line is `all passed`.

- [ ] **Step 6: Run the real probe**

Run: `PROBE_KEEP="$(mktemp -d)" bash .claude/factory/probes/nesting-probe.sh`
Expected: `DATE`, `CLAUDE`, `SKILL`, `NEST`, `BASIS` lines. Run it a second time to see whether the result is stable. If the two runs disagree, use the weaker result (`no` beats `yes`, `unknown` beats both). If the CLI is missing or not logged in, the result is `unknown` and the outcome is C. Write down the exact output.

- [ ] **Step 7: Record the dated ruling**

In `.claude/software-factory.md`, add this section directly above `## Open items`, then change the `Skill` open item (the bullet starting "`Skill` inside a subagent") to: `- Settled by the nesting ruling above: see "Observed outcome".`

Fill in the first two lines from step 6 and keep only one `Observed outcome` value. The three wordings stay in the doc, because the builder prompt copies the one that matches.

```markdown
## Nesting rulings

Ruling 2026-10-13: probe `.claude/factory/probes/nesting-probe.sh` (CLAUDE=<version from the probe>) gave SKILL=<value>, NEST=<value>, BASIS=<value>.
Observed outcome: <A, B or C>

| Outcome | Meaning | Builder wording |
|---|---|---|
| A | Skill works, nesting works | Run the plan with the Skill `superpowers:executing-plans`. If the skill or a plan step asks you to dispatch a helper subagent, you may do so. The helper works in your worktree, one at a time, and you review its result before you go on. |
| B | Skill works, nesting does not | Run the plan with the Skill `superpowers:executing-plans`. You cannot spawn subagents here, so if the skill or a plan step asks for a helper subagent, do that work yourself inline and say so in your reply. Never fail because a helper cannot be spawned. |
| C | Skill does not work, or the probe result is unknown | The Skill tool is not usable for plans here. Follow the plan inline, the way `executing-plans` does: read the whole plan critically first, raise a blocker in your reply instead of guessing, do the tasks in order with one test-first cycle per task, run the checks, commit per task, and stop at the end of this plan. |

Whatever the outcome, the builder never fails because a helper or a skill is unavailable. If a Skill call fails at run time, the builder switches to wording C. The hand-off from one plan to the next always goes through the orchestrator (decision 2), also when nesting works.
```

- [ ] **Step 8: Commit**

Invoke the skill `dev-workflow-skills:1-git-commit`, then commit with a pathspec:
`git add .claude/factory/probes/classify-probe.py .claude/factory/probes/nesting-probe.sh .claude/factory/tests/run.sh .claude/software-factory.md && git commit --only -m "feat(factory): probe subagent skill and nesting support" -- .claude/factory/probes/classify-probe.py .claude/factory/probes/nesting-probe.sh .claude/factory/tests/run.sh .claude/software-factory.md`

---

### Task 2: Spec-writer series output and ctl series support

**Files:**
- Modify: `.claude/agents/factory-spec-writer.md`
- Modify: `.claude/factory/factory-ctl`
- Modify: `.claude/factory/tests/run.sh` (insert a `# ---- series ----` section before `# ---- summary ----`)

**Interfaces:**
- Consumes: `find_job`, `load_board`, `save_board`, `job_dir`, `slugify`, `die`, `repo_root` (already in ctl), and whatever plan 01 and 03 added to `new-job` (id allocation, board lock). Keep those as they are.
- Produces:
  - Board field `series`: `null` or `{"id": "<origin job id>", "index": <1-based int>, "total": <int>, "next_plan": "<path relative to the main checkout>" | null}`.
  - `ctl check-series <plan-file>`: validates the chain from that file, prints each plan path then `ok <n> plans`, exit 1 with a message on any error.
  - `ctl series-start <id>`: for the job whose spec writer wrote `plans/`, validates the chain, copies plan 1 to `plan.md`, sets `series`.
  - `ctl new-job "<feature>" --series <origin id> --plan-file <path>`: creates the job for a later plan in stage `build`, with `plan.md` copied from the plan file and `spec.md` copied from the origin job.
  - `ctl next-plan <id>`: prints `series.next_plan` or `none`.
  - Plan file contract: tasks are `- [ ] Task N: <title>` lines (1 to 3), the last non-empty line is `Next plan: <path|none>`, paths are relative to the main checkout root.

- [ ] **Step 1: Write the failing series tests**

Insert before the `# ---- summary ----` line of `.claude/factory/tests/run.sh`:

```bash
# ---- series ----
sr="$tmp/sr"
git init -q -b main "$sr"
git -C "$sr" -c user.email=t@t -c user.name=t commit -q --allow-empty -m init
export FACTORY_ROOT="$sr"
sboard="$sr/.superpowers/factory/board.json"
sname="$(python3 "$ctl" new-job "Big feature" --date 2026-10-13)"
srel=".superpowers/sdd/$sname/plans"
sdir="$sr/$srel"
mkdir -p "$sdir"
printf '%s\n' '# Spec' 'Goal.' >"$sr/.superpowers/sdd/$sname/spec.md"

# Usage: mkplan <file> <task count> <next plan line value>
mkplan() {
  local file="$1" count="$2" next="$3" i
  : >"$file"
  printf '%s\n' '# Plan' >>"$file"
  for i in $(seq 1 "$count"); do printf '%s\n' "- [ ] Task $i: step $i" >>"$file"; done
  printf '\n%s\n' "Next plan: $next" >>"$file"
}

mkplan "$sdir/plan-01-a.md" 2 "$srel/plan-02-b.md"
mkplan "$sdir/plan-02-b.md" 3 "none"
out="$(python3 "$ctl" check-series "$srel/plan-01-a.md" 2>&1)"
check "series: check-series accepts a two plan chain" test "$(printf '%s' "$out" | tail -n1)" = "ok 2 plans"

mkplan "$sdir/plan-02-b.md" 3 "$srel/plan-01-a.md"
out="$(python3 "$ctl" check-series "$srel/plan-01-a.md" 2>&1)"; rc=$?
check "series: a cycle is rejected with a clear message" bash -c "test $rc -ne 0 && printf '%s' '$out' | grep -qi cycle"
mkplan "$sdir/plan-02-b.md" 4 "none"
check "series: a plan with 4 tasks is rejected" bash -c "! python3 '$ctl' check-series '$srel/plan-01-a.md'"
mkplan "$sdir/plan-02-b.md" 0 "none"
check "series: a plan with no tasks is rejected" bash -c "! python3 '$ctl' check-series '$srel/plan-01-a.md'"
mkplan "$sdir/plan-02-b.md" 3 "$srel/plan-09-missing.md"
check "series: a missing next plan file is rejected" bash -c "! python3 '$ctl' check-series '$srel/plan-01-a.md'"
printf '%s\n' '# Plan' '- [ ] Task 1: x' >"$sdir/plan-02-b.md"
check "series: a plan without a Next plan last line is rejected" bash -c "! python3 '$ctl' check-series '$srel/plan-01-a.md'"
mkplan "$sdir/plan-02-b.md" 3 "none"

mkplan "$sdir/plan-07-orphan.md" 1 "none"
check "series: series-start rejects a plan file that is not in the chain" bash -c "! python3 '$ctl' series-start 1"
rm "$sdir/plan-07-orphan.md"
python3 "$ctl" series-start 1 >/dev/null
check "series: series-start stores the series field" \
  jq -e --arg n "$srel/plan-02-b.md" '.jobs[0].series | select(.id=="001" and .index==1 and .total==2 and .next_plan==$n)' "$sboard"
check "series: series-start copies plan 1 to plan.md" \
  test "$(grep -c '^- \[ \] Task' "$sr/.superpowers/sdd/$sname/plan.md")" -eq 2
check "series: series-start writes a ledger ruling" grep -q '^Ruling: series of 2 plans' "$sr/.superpowers/sdd/$sname/progress.md"
check "series: series-start twice is refused" bash -c "! python3 '$ctl' series-start 1"
check "series: next-plan prints the next plan path" test "$(python3 "$ctl" next-plan 1)" = "$srel/plan-02-b.md"

dirs_before="$(ls "$sr/.superpowers/sdd" | wc -l)"
board_before="$(cat "$sboard")"
check "series: plan 2 cannot start while plan 1 is not merged" \
  bash -c "! python3 '$ctl' new-job 'Big feature part 2' --series 1 --plan-file '$srel/plan-02-b.md'"
check "series: a refused new-job leaves no orphan folder" test "$(ls "$sr/.superpowers/sdd" | wc -l)" -eq "$dirs_before"
check "series: a refused new-job leaves the board unchanged" test "$board_before" = "$(cat "$sboard")"
python3 "$ctl" set-stage 1 needs-human --reason "stuck" >/dev/null
check "series: plan 2 cannot start while plan 1 is needs-human" \
  bash -c "! python3 '$ctl' new-job 'Big feature part 2' --series 1 --plan-file '$srel/plan-02-b.md'"
python3 "$ctl" set-stage 1 merged >/dev/null
check "series: --series without --plan-file is refused" bash -c "! python3 '$ctl' new-job 'x' --series 1"
mkplan "$sr/stray.md" 1 "none"
check "series: a plan file outside the chain is refused" bash -c "! python3 '$ctl' new-job 'x' --series 1 --plan-file '$sr/stray.md'"
name2="$(python3 "$ctl" new-job "Big feature part 2" --series 1 --plan-file "$srel/plan-02-b.md" --date 2026-10-13)"
check "series: plan 2 starts after plan 1 is merged" test -n "$name2"
check "series: the new job is in stage build with its series field" \
  jq -e '.jobs[1] | select(.stage=="build" and .series.id=="001" and .series.index==2 and .series.total==2 and .series.next_plan==null)' "$sboard"
check "series: the new job has plan.md with 3 tasks" \
  test "$(grep -c '^- \[ \] Task' "$sr/.superpowers/sdd/$name2/plan.md")" -eq 3
check "series: the new job has the origin spec.md" grep -q '^Goal\.' "$sr/.superpowers/sdd/$name2/spec.md"
check "series: next-plan of the last plan is none" test "$(python3 "$ctl" next-plan 2)" = "none"
check "series: next-plan of a job without a series is none" bash -c "python3 '$ctl' new-job plain --date 2026-10-13 >/dev/null && test \"\$(python3 '$ctl' next-plan 3)\" = none"
check "series: the same plan cannot start twice" \
  bash -c "! python3 '$ctl' new-job 'again' --series 1 --plan-file '$srel/plan-02-b.md'"
check "series: status shows plan k/N" bash -c "python3 '$ctl' status | grep -q 'plan 2/2'"
unset FACTORY_ROOT

# ---- spec writer prompt ----
sw="$agents/factory-spec-writer.md"
for word in 'plans/plan-01' 'Next plan:' 'plans/index.md' 'at most 3 tasks' 'Next plan: none'; do
  grep -qF -- "$word" "$sw" && pass "spec writer mentions: $word" || fail "spec writer mentions: $word"
done
if grep -q 'Later plans' "$sw"; then fail "spec writer no longer defers to Later plans"; else pass "spec writer no longer defers to Later plans"; fi
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bash .claude/factory/tests/run.sh 2>&1 | grep -E 'series|spec writer|failed|all passed'`
Expected: the series checks print `FAIL` and the last line is `N failed`.

- [ ] **Step 3: Add the series code to ctl**

Read the current `.claude/factory/factory-ctl` first. Add `import shutil` with the other imports. Add this block after `git(...)`, before `cmd_new_job`:

```python
MAX_PLAN_TASKS = 3
MAX_SERIES_PLANS = 6
TASK_LINE = re.compile(r"^- \[[ x]\] Task \d+:", re.MULTILINE)
NEXT_LINE = re.compile(r"^Next plan:[ \t]*(\S+)[ \t]*$")


def plan_full_path(root, path):
    return path if os.path.isabs(path) else os.path.join(root, path)


def normalize_plan(root, path):
    """Return the plan path relative to the main checkout when it is inside it."""
    full = os.path.realpath(plan_full_path(root, path))
    root_real = os.path.realpath(root)
    if full.startswith(root_real + os.sep):
        return os.path.relpath(full, root_real)
    return full


def check_chain(root, first):
    """Validate the Next plan chain from the first plan and return its paths."""
    chain = []
    seen = set()
    current = normalize_plan(root, first)
    while True:
        if current in seen:
            die(f"cycle in the Next plan chain at {current}")
        seen.add(current)
        if len(chain) >= MAX_SERIES_PLANS:
            die(f"more than {MAX_SERIES_PLANS} plans in the chain")
        try:
            with open(plan_full_path(root, current), encoding="utf-8") as handle:
                text = handle.read()
        except FileNotFoundError:
            die(f"plan file is missing: {current}")
        count = len(TASK_LINE.findall(text))
        if count < 1 or count > MAX_PLAN_TASKS:
            die(f"{current} has {count} tasks, it must have 1 to {MAX_PLAN_TASKS}")
        lines = [line for line in text.splitlines() if line.strip()]
        found = NEXT_LINE.match(lines[-1]) if lines else None
        if not found:
            die(f"{current}: the last line must be 'Next plan: <path|none>'")
        chain.append(current)
        if found.group(1) == "none":
            return chain
        current = normalize_plan(root, found.group(1))


def series_plan_files(root, job):
    folder = os.path.join(job_dir(root, job), "plans")
    try:
        names = sorted(n for n in os.listdir(folder) if re.fullmatch(r"plan-\d+.*\.md", n))
    except FileNotFoundError:
        die(f"job {job['id']} has no plans/ folder")
    if not names:
        die(f"job {job['id']} has no plan-NN files in plans/")
    return [os.path.join(folder, name) for name in names]


def origin_chain(root, origin):
    """Return the validated chain of an origin job and reject orphan plan files."""
    files = series_plan_files(root, origin)
    chain = check_chain(root, files[0])
    listed = {os.path.realpath(plan_full_path(root, path)) for path in chain}
    orphans = [os.path.basename(f) for f in files if os.path.realpath(f) not in listed]
    if orphans:
        die("plan files not in the Next plan chain: " + ", ".join(orphans))
    return chain


def append_ledger(root, job, line):
    with open(os.path.join(job_dir(root, job), "progress.md"), "a", encoding="utf-8") as handle:
        handle.write(line.rstrip("\n") + "\n")


def plan_series_job(root, board, args):
    """Check a new-job series request before anything is created."""
    if bool(args.series) != bool(args.plan_file):
        die("--series and --plan-file must be given together")
    if not args.series:
        return None
    origin = find_job(board, args.series)
    info = origin.get("series")
    if not info or info["id"] != origin["id"]:
        die(f"job {origin['id']} is not the origin of a series, run series-start first")
    chain = origin_chain(root, origin)
    wanted = normalize_plan(root, args.plan_file)
    if wanted not in chain:
        die(f"{args.plan_file} is not in the series chain")
    index = chain.index(wanted) + 1
    started = {
        job["series"]["index"]: job
        for job in board["jobs"]
        if (job.get("series") or {}).get("id") == origin["id"]
    }
    if index in started:
        die(f"plan {index} was already started by job {started[index]['id']}")
    previous = started.get(index - 1)
    if previous is None or previous["stage"] != "merged":
        die(f"plan {index - 1} of the series must be merged before plan {index} starts")
    return origin, chain, index
```

In `cmd_new_job` (keep plan 01 and 03 changes such as id allocation and the lock). Make these four edits:

1. Right after `board = load_board(root)`, before any directory is created: `series_plan = plan_series_job(root, board, args)`.
2. Change the slug line to: `slug = slugify(args.slug or (os.path.splitext(os.path.basename(args.plan_file))[0] if series_plan else args.feature))`.
3. Add `"series": None,` to the job dict. Build the dict as a variable `job = {...}` if it is built inline in `append`, then add this before `board["jobs"].append(job)`:

```python
    if series_plan:
        origin, chain, index = series_plan
        job["stage"] = "build"
        job["series"] = {
            "id": origin["id"],
            "index": index,
            "total": len(chain),
            "next_plan": chain[index] if index < len(chain) else None,
        }
```

4. After the progress file is written (the directory exists) and before `save_board`:

```python
    if series_plan:
        shutil.copyfile(plan_full_path(root, chain[index - 1]), os.path.join(path, "plan.md"))
        origin_spec = os.path.join(job_dir(root, origin), "spec.md")
        if os.path.exists(origin_spec):
            shutil.copyfile(origin_spec, os.path.join(path, "spec.md"))
        with open(os.path.join(path, "progress.md"), "a", encoding="utf-8") as handle:
            handle.write(f"Ruling: plan {index} of {len(chain)} from series {origin['id']}\n")
```

Add these commands after `cmd_is_merged`:

```python
def cmd_check_series(args):
    root = repo_root()
    chain = check_chain(root, args.plan_file)
    for path in chain:
        print(path)
    print(f"ok {len(chain)} plans")


def cmd_series_start(args):
    root = repo_root()
    board = load_board(root)
    job = find_job(board, args.id)
    if job.get("series"):
        die(f"job {job['id']} already belongs to a series")
    chain = origin_chain(root, job)
    target = os.path.join(job_dir(root, job), "plan.md")
    if os.path.exists(target):
        die("plan.md already exists, refusing to overwrite it")
    shutil.copyfile(plan_full_path(root, chain[0]), target)
    job["series"] = {
        "id": job["id"],
        "index": 1,
        "total": len(chain),
        "next_plan": chain[1] if len(chain) > 1 else None,
    }
    save_board(root, board)
    append_ledger(root, job, f"Ruling: series of {len(chain)} plans, plan 1 of {len(chain)} started")
    print(f"series of {len(chain)} plans")


def cmd_next_plan(args):
    root = repo_root()
    job = find_job(load_board(root), args.id)
    following = (job.get("series") or {}).get("next_plan")
    if not following:
        print("none")
        return
    if not os.path.exists(plan_full_path(root, following)):
        die(f"next plan file is missing: {following}")
    print(following)
```

In `cmd_status`, append the series suffix to the printed line: compute `series = job.get("series")` and `tail = f"  plan {series['index']}/{series['total']}" if series else ""`, then add `{tail}` to the end of the f-string. In `build_parser` add `--series` and `--plan-file` (`dest="plan_file"`) to `new_job`, and:

```python
    check_series = sub.add_parser("check-series")
    check_series.add_argument("plan_file")
    check_series.set_defaults(func=cmd_check_series)

    series_start = sub.add_parser("series-start")
    series_start.add_argument("id")
    series_start.set_defaults(func=cmd_series_start)

    next_plan = sub.add_parser("next-plan")
    next_plan.add_argument("id")
    next_plan.set_defaults(func=cmd_next_plan)
```

If plan 03 added a board lock decorator or context manager around mutating commands, wrap `cmd_series_start` the same way.

- [ ] **Step 4: Run the ctl tests to verify they pass**

Run: `bash .claude/factory/tests/run.sh 2>&1 | grep -E 'series:|failed|all passed'`
Expected: every `series:` check prints `PASS`. The `spec writer` checks still fail until the next step.

- [ ] **Step 5: Change the spec-writer prompt**

In `.claude/agents/factory-spec-writer.md`, change the description line to `Turns a feature request into a spec and one or more plans of at most 3 tasks each for Rocket's software factory. Use at the start of a factory job.` (no colon-space). Replace step 4 and step 6 with the text below, renumber if needed, and keep steps 1 to 3 and 5 as they are:

```markdown
4. Decide how many plans the feature needs. One plan holds at most 3 tasks.

   - Up to 3 tasks: write `plan.md` in the job directory.
   - More than 3 tasks: write a series. Do not write `plan.md`. Write `plans/index.md` and one file per plan, `plans/plan-01-<slug>.md`, `plans/plan-02-<slug>.md` and so on, in the job directory. Write at most 6 plans. Each plan must be buildable and testable on its own, and later plans may rely on earlier ones.

   Every plan file has at most 3 tasks. Each task starts with one unchecked checkbox line, then its steps:

   `- [ ] Task 1: <title>`

   Each task names the exact files, a test-first step, and the checks to run: `cargo check -j4`, `cargo test -j4 -p <crate> <test_name>`, `yarn tsc --noEmit`, `yarn check`. Never use `--workspace`.

   Every plan file in a series ends with a `Next plan:` line as its last line. The path is relative to the main checkout: `.superpowers/sdd/<job folder name>/plans/plan-02-<slug>.md`. The job folder name is the last segment of the job directory you were given. The last plan ends with `Next plan: none`. No other line may start with `Next plan:`.

   `plans/index.md` is a table with one row per plan: number, file name, what it delivers.
```

and replace the old step 6 with:

```markdown
6. Never put work into a "Later plans" list that nothing reads. If the feature needs more than 6 plans, write the first 6 and list the remaining scope under "Not covered" in `spec.md`.
```

Change the Write-scope sentence to: `Write only inside the job directory (spec.md, plan.md, plans/).` and the final message sentence to: `Finish with a short message naming the files you wrote, the plan count and the task titles of each plan.`

- [ ] **Step 6: Run all tests**

Run: `bash .claude/factory/tests/run.sh 2>&1 | tail -n 5`
Expected: `all passed`. The existing agent test that rejects a colon-space in a description must still pass.

- [ ] **Step 7: Commit**

Invoke the skill `dev-workflow-skills:1-git-commit`, then:
`git add .claude/agents/factory-spec-writer.md .claude/factory/factory-ctl .claude/factory/tests/run.sh && git commit --only -m "feat(factory): add plan series support to ctl and the spec writer" -- .claude/agents/factory-spec-writer.md .claude/factory/factory-ctl .claude/factory/tests/run.sh`

---

### Task 3: Orchestrator chaining, builder prompt, docs and verification

**Files:**
- Modify: `.claude/commands/factory.md`
- Modify: `.claude/agents/factory-builder.md`
- Modify: `.claude/factory/tests/run.sh` (insert a `# ---- series prompts ----` section before `# ---- summary ----`)
- Modify: `.claude/software-factory.md`

**Interfaces:**
- Consumes: `ctl series-start`, `ctl next-plan`, `ctl new-job --series --plan-file`, `ctl check-series` and the board `series` field from Task 2. The `Observed outcome` line from Task 1.
- Produces: the `/factory series <job-id>` subcommand and the series step of the merge flow. Builder prompt that runs the plan through `superpowers:executing-plans`.

- [ ] **Step 1: Write the failing prompt tests**

Insert before the `# ---- summary ----` line of `.claude/factory/tests/run.sh`:

```bash
# ---- series prompts ----
for word in 'series-start' 'next-plan' '--series' '--plan-file' 'plans/index.md' 'Series step' \
  'series <digits>' 'plan <index> of <total>' 'never in parallel'; do
  grep -qF -- "$word" "$cmd" && pass "command mentions: $word" || fail "command mentions: $word"
done
check "command: the argument hint lists series" grep -q '^argument-hint:.*series <job-id>' "$cmd"
bld="$agents/factory-builder.md"
for word in 'superpowers:executing-plans' 'never fail because' 'inline' 'finishing-a-development-branch' 'plan <index> of <total>'; do
  grep -qF -- "$word" "$bld" && pass "builder mentions: $word" || fail "builder mentions: $word"
done
if grep -q 'Do the plan task by task' "$bld"; then fail "builder no longer says do the plan task by task"; else pass "builder no longer says do the plan task by task"; fi
doc="$repo/.claude/software-factory.md"
check "docs: a dated nesting ruling exists" grep -q '^Ruling 2026-' "$doc"
check "docs: the observed outcome is one of A, B or C" grep -Eq '^Observed outcome: [ABC]$' "$doc"
check "docs: the series model is documented" grep -q 'Next plan:' "$doc"
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bash .claude/factory/tests/run.sh 2>&1 | grep -E 'series prompts|command mentions|builder mentions|docs:|failed|all passed'`
Expected: the new command and builder checks print `FAIL`.

- [ ] **Step 3: Update `/factory`**

Read `.claude/commands/factory.md` first (plans 01 to 03 changed it). Make these edits.

1. Frontmatter hint: `argument-hint: "<feature> | next | approve <job-id> | rework <job-id> <note> | status | series <job-id>"`.
2. In the exact-shape sentence add `series <digits>`.
3. In the new job flow, replace step 3 and step 4 with:

```markdown
3. If the job directory has `plans/index.md`, the spec writer wrote a series. Run `ctl series-start <id>`. It checks the chain (every plan has 1 to 3 tasks and ends with a `Next plan:` line, no cycle, no orphan plan file), copies plan 1 to `plan.md` and sets the board `series` field. If it fails, `ctl set-stage <id> needs-human --reason "bad plan series: <error text>"` and stop. If there is no `plans/` folder, check that `plan.md` exists and has 1 to 3 lines starting with `- [ ] Task`, otherwise `ctl set-stage <id> needs-human --reason "bad plan"` and stop. Either way `spec.md` must exist.
4. `ctl ledger <id> "Ruling: plan accepted, <n> tasks"` (for a series: `Ruling: plan 1 of <N> accepted, <n> tasks`) and `ctl set-stage <id> build`. Report the job and tell the user to run `/factory next`.
```

4. In the `build` stage first-run dispatch, add this sentence: `If the board job has a series, also tell the builder: this is plan <index> of <total>, do only this plan, never start the next plan.`
5. Add a `series <job-id>` section after `rework`:

```markdown
## series `<job-id>`

Only for a merged job that belongs to a series. It runs the series step again. Use it when the session stopped between a merge and the start of the next plan.
```

6. Add a final merge step after the last numbered step of the Merge step: `6. Run the series step for this job.` Then add this section at the end of the file:

```markdown
## Series step

Plans of one series run one after another, never in parallel, and only through you.

1. `ctl next-plan <id>`. If it prints `none`, the job has no further plan: report "series complete" for a series job and do nothing for a plain job.
2. Otherwise read `series` from the board (`ctl show <id>` if plan 01's command is available, else `board.json` through `ctl status`). Create the next job: `ctl new-job "<feature> (plan <index+1> of <total>)" --series <series id> --plan-file <path from next-plan>`. It prints the folder name. The job starts in stage `build` with `plan.md` and `spec.md` already copied. If ctl refuses because that plan already started, or because a plan is not merged yet, report ctl's message and stop. Do not work around it.
3. Run the `build` stage for the new job as a first run, then report. The previous builder is finished and merged, so only one builder runs.
4. If a job lands in `needs-human`, the series stops there. The next plan is created only after that job is merged, which also happens through `/factory approve` and the merge step. Never create a job for plan k+1 while plan k is unmerged.
```

- [ ] **Step 4: Update the builder prompt**

Read `.claude/agents/factory-builder.md` first. Replace step 3 of "First run" (`3. Do the plan task by task. ...`) with the text below. Replace `<WORDING>` with the `Builder wording` text of the observed outcome (A, B or C) from `.claude/software-factory.md`, copied verbatim. Do not keep the other two.

```markdown
3. Run the plan. <WORDING>

   Whatever the outcome, you never fail because a skill or a helper subagent is not available. If a Skill call fails, or a helper cannot be spawned, do the work yourself inline and say so in your reply. Rules for plan execution:

   - The plan has at most 3 tasks. For each task write the failing test first, then the code, then run the checks.
   - You are already in your own worktree. Do not create another worktree and do not use the `finishing-a-development-branch` step: never merge, open a PR or push. The orchestrator merges.
   - Do not edit `plan.md`. Do not tick its checkboxes. The orchestrator does that after the merge.
   - If the job is part of a series, the orchestrator tells you "plan <index> of <total>". Do only this plan and never start the next plan, even if the plan file names one.
```

Keep the rest of the prompt (hard rules, commit rule, resume rule, the four closing lines).

- [ ] **Step 5: Sync the design doc**

In `.claude/software-factory.md`:

1. Replace the first bullet of "Job model" with: `- One job is one plan of at most 3 tasks. A feature that needs more is written as a series: one spec, several plans (\`plans/plan-NN-<slug>.md\` plus \`plans/index.md\` in the first job's folder). Each plan ends with a \`Next plan: <path|none>\` line. Plans run one after another, never in parallel. The board field \`series\` holds \`id\`, \`index\`, \`total\`, \`next_plan\`. \`ctl series-start\` validates the chain (1 to 3 tasks per plan, files exist, no cycle, no orphan plan) and \`ctl new-job --series <id> --plan-file <path>\` creates the job of a later plan, only after the previous plan is merged.`
2. In the Agents table, change the `factory-spec-writer` duty cell to mention one spec plus up to 6 plans of at most 3 tasks, and the `factory-builder` duty cell to add "runs the plan with `superpowers:executing-plans` when the Skill tool works, otherwise inline (see Nesting rulings)".
3. In the Commands section add: `- \`/factory series <job-id>\`: after a merge (or after a stop between a merge and the next plan), run the series step: create the job for the next plan and start its build. The merge step runs it automatically.`
4. Add a short "## Series chaining" section after "Merge step": after the merge of plan k the orchestrator runs `ctl next-plan`, creates the next job with that plan file, runs its build stage, and stops on `needs-human`. The hand-off always goes through the orchestrator, also when nesting works.
5. Add `Plan 04 (series) is in docs/superpowers/plans/2026-10-13-software-factory-v2/.` to the Status line.

- [ ] **Step 6: Run the full verification**

Run each of these and read the output:

```bash
bash .claude/factory/tests/run.sh 2>&1 | tail -n 5
python3 -c "import ast,sys; ast.parse(open(sys.argv[1]).read())" .claude/factory/factory-ctl
python3 -c "import ast,sys; ast.parse(open(sys.argv[1]).read())" .claude/factory/probes/classify-probe.py
bash -n .claude/factory/probes/nesting-probe.sh
git diff --stat
```

Expected: `all passed`, no syntax error, and a diff that touches only the files named in this plan. Then dry-check by hand with a scratch root: `FACTORY_ROOT="$(mktemp -d)"` after `git init`, create one origin job, write two plan files in its `plans/` folder, run `ctl series-start`, `ctl set-stage 1 merged`, `ctl new-job ... --series 1 --plan-file ...`, and read `ctl status`. Expected: `plan 1/2` and `plan 2/2`. No cargo run is needed.

- [ ] **Step 7: Commit**

Invoke the skill `dev-workflow-skills:1-git-commit`, then:
`git add .claude/commands/factory.md .claude/agents/factory-builder.md .claude/factory/tests/run.sh .claude/software-factory.md && git commit --only -m "feat(factory): chain plan series and run plans with executing-plans" -- .claude/commands/factory.md .claude/agents/factory-builder.md .claude/factory/tests/run.sh .claude/software-factory.md`

---

## Self-Review

- Spec coverage: D1 and D2 (series, `Next plan:`, machine-readable index) in Task 2. D3 and D4 (probe, `executing-plans`, both outcomes) in Tasks 1 and 3. D5 (plan k of N) in the board field and the status suffix, the dashboard chip is plan 05. Decisions 1, 2, 3 are in Global Constraints and enforced by `plan_series_job` (sequential) and the series step (through the orchestrator).
- Placeholder scan: the only substitution points are the probe values in the ruling (Task 1 step 7) and `<WORDING>` (Task 3 step 4). Both have all three or four concrete options written out in the plan.
- Type consistency: `series` is `{id, index, total, next_plan}` everywhere. `next_plan` is a path relative to the main checkout or `null`. `ctl next-plan` prints the path or `none`. Plan paths are normalized by `normalize_plan` in every comparison.

Next plan: docs/superpowers/plans/2026-10-13-software-factory-v2/plan-05-dashboard-server-hook.md
