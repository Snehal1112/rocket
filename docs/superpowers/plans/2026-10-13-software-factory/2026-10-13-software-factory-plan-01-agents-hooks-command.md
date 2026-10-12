# Software Factory Plan 01: Agents, Hooks and `/factory` Command

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the interactive session everything it needs to run the factory pipeline: an event logger hook, seven factory subagents, a tested board and ledger helper, and the `/factory` command.

**Architecture:** Async Claude Code hooks append trimmed events to `.superpowers/factory/events.jsonl` in the main checkout. A small Python helper, `factory-ctl`, owns `board.json`, dated job folders, the ledger and plan checkbox ticking, so none of that is left to prompt text. The `/factory` command is the orchestrator procedure and calls `factory-ctl` and the subagents. The Rust server and the dashboard come in plans 02 and 03.

**Tech Stack:** bash, Python 3 stdlib (same as the existing harness hooks), `jq` in tests, Claude Code agent and command markdown files.

**Spec:** [.claude/software-factory.md](../../../../.claude/software-factory.md). Plan index: [00-plan-index.md](00-plan-index.md).

## Global Constraints

- All `cargo` commands pass `-j4`. No `cargo test --workspace` or `--all`.
- Commits use the plugin skill `dev-workflow-skills:1-git-commit`, conventional commit format, and a pathspec: `git add <paths> && git commit --only -m "..." -- <paths>`. Never `git add -A`, `--all` or `.`.
- Never set `HARNESS_GATE=0`. Never push.
- No banned unwrap pattern in any Rust file (none are written in this plan).
- Hook scripts always exit 0. A logging problem must never block an agent.
- `board.json` is written only by `factory-ctl`. Plan checkboxes are ticked only by `factory-ctl tick-plan`.
- Plan and job folders start with the creation date: `YYYY-MM-DD-NNN-<slug>`.
- Models are fixed in agent frontmatter: spec-writer opus, builder opus, security-reviewer opus, ux-reviewer sonnet, ui-reviewer sonnet, code-reviewer sonnet, approver opus.
- Reviewer verdict last line is exactly `VERDICT: APPROVE` or `VERDICT: CHANGES`. Approver last line is exactly `DECISION: APPROVE` or `DECISION: ESCALATE`.
- Nothing from this plan ships in the Rocket app. All files live under `.claude/`, `docs/` and `.superpowers/`.

## Review Focus

- Concurrent subagents appending to the event log at once must not interleave or corrupt lines. Tested in Task 1.
- A hook payload that is not JSON, or has huge or odd fields, must not crash the hook or leak `tool_response` into the log. Tested in Task 1.
- A job whose branch has no commits must not look merged, and a job whose branch was deleted after a manual merge must still be detected as merged. Tested in Task 3.
- Running `/factory approve` twice must not double-tick checkboxes or duplicate ledger lines. Tested in Task 3.
- A reviewer reply with no `VERDICT:` line must never count as a pass. Pinned in the command text and linted in Task 3.

---

### Task 1: Event logger hook and probe

**Files:**
- Create: `.claude/factory/hooks/log-event`
- Create: `.claude/factory/tests/run.sh`
- Modify: `.claude/settings.json`
- Modify: `docs/superpowers/plans/2026-10-13-software-factory/00-plan-index.md` (probe results)

**Interfaces:**
- Consumes: Claude Code hook payload JSON on stdin (`hook_event_name`, `session_id`, `agent_id`, `agent_type`, `tool_name`, `tool_input`, `tool_use_id`, `cwd`, `duration_ms`, `last_assistant_message`, `agent_transcript_path`, `task_id`, `task_subject`).
- Produces: one JSON line per event appended to `<main checkout>/.superpowers/factory/events.jsonl`, or to `$FACTORY_EVENT_LOG` when set. Fields: `ts` (seconds, float), the kept payload fields above, `tool_target` (file path, pattern, url, or first 80 chars of a command) and `last_message` (first 300 chars). `tool_response` and the full `tool_input` are never stored. Plan 02 reads this file.

- [ ] **Step 1: Write the failing tests**

Create `.claude/factory/tests/run.sh`:

```bash
#!/usr/bin/env bash
# Self-contained tests for the factory helpers. Exits non-zero on any failure.
set -u

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
factory="$(cd "$here/.." && pwd)"
repo="$(cd "$factory/../.." && pwd)"
export PYTHONDONTWRITEBYTECODE=1
tmp="$(mktemp -d "${TMPDIR:-/tmp}/factory-test.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

fails=0
pass() { echo "PASS: $1"; }
fail() { echo "FAIL: $1"; fails=$((fails + 1)); }
# Usage: check <description> <command...>
check() {
  local description="$1"
  shift
  if "$@" >/dev/null 2>&1; then pass "$description"; else fail "$description"; fi
}

# ---- log-event hook ----
logger="$factory/hooks/log-event"
log="$tmp/events.jsonl"

run_logger() {
  printf '%s' "$1" | FACTORY_EVENT_LOG="$log" python3 "$logger"
}

run_logger '{"hook_event_name":"SubagentStart","session_id":"s1","agent_id":"a1","agent_type":"factory-builder","cwd":"/x"}'
check "logger: appends one line for SubagentStart" test "$(wc -l <"$log")" -eq 1
check "logger: keeps agent_id and agent_type" jq -e 'select(.agent_id=="a1" and .agent_type=="factory-builder")' "$log"

long="$(python3 -c 'print("x" * 500)')"
run_logger "{\"hook_event_name\":\"PostToolUse\",\"agent_id\":\"a1\",\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"$long\"},\"tool_response\":\"SECRET-OUTPUT\",\"cwd\":\"/x\"}"
check "logger: trims a long command to 80 chars" jq -e 'select(.tool_name=="Bash" and (.tool_target | length) == 80)' "$log"
check "logger: never stores tool_response" bash -c "! grep -q SECRET-OUTPUT '$log'"

printf 'not json' | FACTORY_EVENT_LOG="$log" python3 "$logger"
rc=$?
if [ "$rc" -eq 0 ] && [ "$(wc -l <"$log")" -eq 2 ]; then
  pass "logger: invalid JSON exits 0 and writes nothing"
else
  fail "logger: invalid JSON exits 0 and writes nothing (rc=$rc)"
fi

printf '{"hook_event_name":"Stop","cwd":"/x"}' | FACTORY_EVENT_LOG="/proc/nope/events.jsonl" python3 "$logger"
rc=$?
if [ "$rc" -eq 0 ]; then pass "logger: unwritable log exits 0"; else fail "logger: unwritable log exits 0 (rc=$rc)"; fi

clog="$tmp/concurrent.jsonl"
for i in $(seq 1 20); do
  printf '{"hook_event_name":"PostToolUse","agent_id":"a%s","tool_name":"Read","cwd":"/x"}' "$i" |
    FACTORY_EVENT_LOG="$clog" python3 "$logger" &
done
wait
check "logger: 20 concurrent writers give 20 valid lines" \
  bash -c "test \$(wc -l < '$clog') -eq 20 && jq -e . '$clog' >/dev/null"

r="$tmp/repo"
git init -q -b main "$r"
git -C "$r" -c user.email=t@t -c user.name=t commit -q --allow-empty -m init
git -C "$r" worktree add -q "$r/wt" -b wt-branch
printf '{"hook_event_name":"SubagentStop","agent_id":"a2","cwd":"%s"}' "$r/wt" | python3 "$logger"
check "logger: a worktree cwd logs to the main checkout" test -s "$r/.superpowers/factory/events.jsonl"

# ---- settings ----
check "settings: factory hooks are registered and async" \
  jq -e '[.hooks.SubagentStart[0].hooks[0].async, .hooks.SubagentStop[0].hooks[0].async, .hooks.PostToolUse[0].hooks[0].async] | all' \
  "$repo/.claude/settings.json"

# ---- summary ----
if [ "$fails" -eq 0 ]; then echo "all passed"; else echo "$fails failed"; fi
exit $((fails > 0))
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bash .claude/factory/tests/run.sh`
Expected: FAIL lines for every logger and settings check, and a non-zero exit code.

- [ ] **Step 3: Write the hook script**

Create `.claude/factory/hooks/log-event` and make it executable (`chmod +x`):

```python
#!/usr/bin/env python3
"""Append a trimmed Claude Code hook event to the factory event log.

Always exits 0, so a logging problem can never block an agent.
"""
import json
import os
import subprocess
import sys
import time

KEEP = (
    "hook_event_name",
    "session_id",
    "agent_id",
    "agent_type",
    "tool_name",
    "tool_use_id",
    "cwd",
    "duration_ms",
    "task_id",
    "task_subject",
    "agent_transcript_path",
)
MAX_TEXT = 300
MAX_COMMAND = 80


def main_root(cwd):
    """Return the main checkout root, even when cwd is inside a worktree."""
    try:
        out = subprocess.run(
            ["git", "-C", cwd, "rev-parse", "--git-common-dir"],
            capture_output=True,
            text=True,
            timeout=3,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    common = out.stdout.strip()
    if out.returncode != 0 or not common:
        return None
    return os.path.dirname(os.path.normpath(os.path.join(cwd, common)))


def tool_target(tool_input):
    if not isinstance(tool_input, dict):
        return ""
    for key in ("file_path", "path", "pattern", "url"):
        value = tool_input.get(key)
        if isinstance(value, str):
            return value[:MAX_TEXT]
    command = tool_input.get("command")
    if isinstance(command, str):
        return command[:MAX_COMMAND]
    return ""


def build_event(payload):
    event = {"ts": round(time.time(), 3)}
    for key in KEEP:
        if key in payload:
            event[key] = payload[key]
    target = tool_target(payload.get("tool_input"))
    if target:
        event["tool_target"] = target
    message = payload.get("last_assistant_message")
    if isinstance(message, str):
        event["last_message"] = message[:MAX_TEXT]
    return event


def log_path(cwd):
    override = os.environ.get("FACTORY_EVENT_LOG")
    if override:
        return override
    root = main_root(cwd)
    if not root:
        return None
    return os.path.join(root, ".superpowers", "factory", "events.jsonl")


def main():
    try:
        payload = json.load(sys.stdin)
        if not isinstance(payload, dict):
            return
        path = log_path(payload.get("cwd") or os.getcwd())
        if not path:
            return
        line = json.dumps(build_event(payload), separators=(",", ":")) + "\n"
        os.makedirs(os.path.dirname(path), exist_ok=True)
        fd = os.open(path, os.O_WRONLY | os.O_APPEND | os.O_CREAT, 0o644)
        try:
            os.write(fd, line.encode("utf-8"))
        finally:
            os.close(fd)
    except Exception:
        return


if __name__ == "__main__":
    main()
    sys.exit(0)
```

- [ ] **Step 4: Run the tests to verify the logger checks pass**

Run: `bash .claude/factory/tests/run.sh`
Expected: every `logger:` line is PASS. Only the `settings:` line still fails.

- [ ] **Step 5: Register the hooks in `.claude/settings.json`**

The file is tab indented. In the `hooks` object, add three keys after the closing `]` of `PreToolUse` (the existing entries stay untouched). Replace:

```
		]
	},
	"permissions": {
```

with:

```
		],
		"SubagentStart": [
			{
				"matcher": "",
				"hooks": [
					{
						"type": "command",
						"command": "h=\"$(git rev-parse --show-toplevel 2>/dev/null)/.claude/factory/hooks/log-event\"; if [ -f \"$h\" ]; then python3 \"$h\"; fi",
						"async": true
					}
				]
			}
		],
		"SubagentStop": [
			{
				"matcher": "",
				"hooks": [
					{
						"type": "command",
						"command": "h=\"$(git rev-parse --show-toplevel 2>/dev/null)/.claude/factory/hooks/log-event\"; if [ -f \"$h\" ]; then python3 \"$h\"; fi",
						"async": true
					}
				]
			}
		],
		"PostToolUse": [
			{
				"matcher": "",
				"hooks": [
					{
						"type": "command",
						"command": "h=\"$(git rev-parse --show-toplevel 2>/dev/null)/.claude/factory/hooks/log-event\"; if [ -f \"$h\" ]; then python3 \"$h\"; fi",
						"async": true
					}
				]
			}
		]
	},
	"permissions": {
```

Run: `jq . .claude/settings.json >/dev/null && bash .claude/factory/tests/run.sh`
Expected: valid JSON, `all passed`.

- [ ] **Step 6: Probe the real hook payloads**

Settings reload automatically. If no events appear after the probe, restart the session once and repeat.

Dispatch one throwaway subagent (Agent tool, `subagent_type: general-purpose`, `model: haiku`) with this prompt:

> Run `ls` in the repo root with the Bash tool, then read `CLAUDE.md` with the Read tool. Report the first line of `CLAUDE.md`. Also say in one line whether a tool named Skill is in your tool list. Do not call it.

Then run:

```bash
jq -c 'select(.hook_event_name=="PostToolUse" or .hook_event_name|test("Subagent")) | {hook_event_name, agent_id, agent_type, tool_name, tool_target}' .superpowers/factory/events.jsonl | tail -12
```

Expected: a `SubagentStart` and `SubagentStop` pair with an `agent_id`, plus `PostToolUse` lines for Bash and Read that carry the same `agent_id`, and the orchestrator's own tool calls with no `agent_id`.

Append a `## Probe results` section to `00-plan-index.md` with three lines: whether `PostToolUse` carried `agent_id` and `agent_type` inside the subagent, whether the subagent reported having a Skill tool (this is the model's own statement, so treat it as indicative), and the date of the probe. If `agent_id` was missing from `PostToolUse`, write that plan 02 must match tool events to agents by `session_id` and the `SubagentStart`/`SubagentStop` time window.

- [ ] **Step 7: Commit**

Use the `dev-workflow-skills:1-git-commit` skill with a pathspec commit of:
`.claude/factory/hooks/log-event`, `.claude/factory/tests/run.sh`, `.claude/settings.json`, `docs/superpowers/plans/2026-10-13-software-factory/00-plan-index.md`.
Suggested type: `feat(factory): log subagent and tool events to a local jsonl file`.

---

### Task 2: Factory agents

**Files:**
- Create: `.claude/agents/factory-spec-writer.md`
- Create: `.claude/agents/factory-builder.md`
- Create: `.claude/agents/factory-security-reviewer.md`
- Create: `.claude/agents/factory-ux-reviewer.md`
- Create: `.claude/agents/factory-ui-reviewer.md`
- Create: `.claude/agents/factory-code-reviewer.md`
- Create: `.claude/agents/factory-approver.md`
- Modify: `.claude/factory/tests/run.sh` (agent lint)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: seven agent names usable as `subagent_type`: `factory-spec-writer`, `factory-builder`, `factory-security-reviewer`, `factory-ux-reviewer`, `factory-ui-reviewer`, `factory-code-reviewer`, `factory-approver`. The builder's last four lines are `WORKTREE: <abs path>`, `BRANCH: <name>`, `BASE: <sha>`, `HEAD: <sha>`. Each reviewer's last line is a `VERDICT:` line, the approver's is a `DECISION:` line. The `/factory` command (Task 3) parses these.

- [ ] **Step 1: Write the failing lint**

In `.claude/factory/tests/run.sh`, insert this block before the `# ---- summary ----` line:

```bash
# ---- agent files ----
agents="$repo/.claude/agents"

# Usage: fm <file> <key>. Prints the frontmatter value for the key.
fm() {
  awk -v k="$2" '
    NR == 1 && $0 != "---" { exit }
    /^---$/ { n++; if (n == 2) exit; next }
    n == 1 && index($0, k ":") == 1 { sub(k ":[ ]*", ""); print; exit }
  ' "$1"
}

# Usage: agent_case <name> <model> <read-only|writer|builder>
agent_case() {
  local name="$1" model="$2" kind="$3" file="$agents/$1.md"
  if [ ! -f "$file" ]; then fail "agent $name: file exists"; return; fi
  [ "$(fm "$file" name)" = "$name" ] && pass "agent $name: name matches file" || fail "agent $name: name matches file"
  [ "$(fm "$file" model)" = "$model" ] && pass "agent $name: model is $model" || fail "agent $name: model is $model"
  [ -n "$(fm "$file" description)" ] && pass "agent $name: has a description" || fail "agent $name: has a description"
  case "$kind" in
    read-only)
      if fm "$file" tools | grep -Eq 'Bash|Write|Edit'; then fail "agent $name: read-only tools"; else pass "agent $name: read-only tools"; fi
      ;;
    writer)
      fm "$file" tools | grep -q 'Write' && pass "agent $name: can write" || fail "agent $name: can write"
      if fm "$file" tools | grep -Eq 'Bash|Edit'; then fail "agent $name: no Bash or Edit"; else pass "agent $name: no Bash or Edit"; fi
      ;;
    builder)
      [ "$(fm "$file" isolation)" = "worktree" ] && pass "agent $name: isolation worktree" || fail "agent $name: isolation worktree"
      if [ -z "$(fm "$file" tools)" ]; then pass "agent $name: inherits all tools"; else fail "agent $name: inherits all tools"; fi
      for line in 'WORKTREE:' 'BRANCH:' 'BASE:' 'HEAD:' 'dev-workflow-skills:1-git-commit'; do
        grep -qF "$line" "$file" && pass "agent $name: mentions $line" || fail "agent $name: mentions $line"
      done
      ;;
  esac
}

agent_case factory-spec-writer opus writer
agent_case factory-builder opus builder
agent_case factory-security-reviewer opus read-only
agent_case factory-ux-reviewer sonnet read-only
agent_case factory-ui-reviewer sonnet read-only
agent_case factory-code-reviewer sonnet read-only
agent_case factory-approver opus read-only

for role in security ux ui code; do
  f="$agents/factory-$role-reviewer.md"
  grep -qxF 'VERDICT: APPROVE' "$f" && grep -qxF 'VERDICT: CHANGES' "$f" \
    && pass "agent $role reviewer: verdict lines" || fail "agent $role reviewer: verdict lines"
done
grep -qxF 'DECISION: APPROVE' "$agents/factory-approver.md" && grep -qxF 'DECISION: ESCALATE' "$agents/factory-approver.md" \
  && pass "agent approver: decision lines" || fail "agent approver: decision lines"
```

- [ ] **Step 2: Run the lint to verify it fails**

Run: `bash .claude/factory/tests/run.sh`
Expected: `agent ...: file exists` FAIL for all seven agents.

- [ ] **Step 3: Create the spec writer**

`.claude/agents/factory-spec-writer.md`:

````markdown
---
name: factory-spec-writer
description: Turns a feature request into a spec and a plan of at most 3 tasks for Rocket's software factory. Use at the start of a factory job.
model: opus
tools: Read, Grep, Glob, Write
maxTurns: 60
---
You are the spec writer in Rocket's software factory.

Input: the feature text and the absolute path of the job directory (`.superpowers/sdd/<date>-<id>-<slug>/`).

Do this:

1. Read `CLAUDE.md`, `.claude/rules/00-shortcuts.md` and the rule file for each area the feature touches. For yml, collection, environment, auth or variable-resolution work, read `docs/superpowers/specs/opencollection-spec-reference.md`.
2. Explore the code the feature touches. Cite files by path.
3. Write `spec.md` in the job directory with: goal, scope, non-goals, acceptance criteria, risks, assumptions.
4. Write `plan.md` in the job directory. At most 3 tasks. Each task starts with one unchecked checkbox line, then its steps:

   `- [ ] Task 1: <title>`

   Each task names the exact files, a test-first step, and the checks to run: `cargo check -j4`, `cargo test -j4 -p <crate> <test_name>`, `yarn tsc --noEmit`, `yarn check`. Never use `--workspace`.
5. Start every task that touches yml files, collection, environment, request or auth models, variable resolution, the crates rocket-collection, rocket-environment, rocket-http, rocket-import, rocket-workspace or rocket-infra, or a collection-related Tauri command with this line:

   `Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.`
6. If the feature needs more than 3 tasks, write only the first 3 into `plan.md` and list the rest under "Later plans" in `spec.md`.

Write only inside the job directory. Do not ask questions: record any assumption in `spec.md`.

Finish with a short message naming the two files you wrote and the task titles.
````

- [ ] **Step 4: Create the builder**

`.claude/agents/factory-builder.md`:

````markdown
---
name: factory-builder
description: Implements a factory plan task by task in an isolated worktree for Rocket, and fixes reviewer CHANGES when resumed. Use after the spec writer.
model: opus
isolation: worktree
---
You are the builder in Rocket's software factory. You work in your own git worktree.

Input: the absolute path of the job directory, the branch name to use, and on a resume the findings file or note to fix.

First run:

1. Create the job branch: `git switch -c <branch>`. Note the starting commit as BASE.
2. Read `CLAUDE.md`, the rule file for each area you touch (`.claude/rules/rust-ddd-boundaries.md`, `tauri-ipc-boundaries.md`, `frontend-component-guardrails.md`) and the job's `spec.md` and `plan.md`. The job directory is outside your worktree, so use the absolute paths you were given. Read it, never edit `plan.md`.
3. Do the plan task by task. For each task write the failing test first, then the code, then run the checks.

Rocket hard rules:

- Run `cargo check -j4` and `cargo test -j4 -p <crate> <test_name>`. Never use `--workspace` or `--all`. Run `yarn tsc --noEmit` and `yarn check` for frontend changes.
- Never set `CARGO_TARGET_DIR`. Your worktree uses its own target directory.
- shadcn/ui primitives only, lucide-react icons only, `SingleLineEditor` for single-line variable-aware fields, Monaco for multi-line editors, narrow Zustand selectors.
- Rust: no unwrap in production paths, no git CLI, `#[serde(rename_all = "camelCase")]` on IPC DTOs only.
- Commit each task with the `dev-workflow-skills:1-git-commit` skill. Use a pathspec commit: `git add <paths> && git commit --only -m "..." -- <paths>`. Never `git add -A`, `--all` or `.`. Never set `HARNESS_GATE=0`. Never run a bare `git stash`.
- Never push and never merge. The orchestrator does both.

On a resume: fix only the listed findings, with new commits. Do not rewrite or amend earlier commits.

Your last four lines must be exactly:

WORKTREE: <absolute path of your worktree>
BRANCH: <branch name>
BASE: <sha the branch started from>
HEAD: <sha of your last commit>

If you could not finish, say why above those four lines and still print them.
````

- [ ] **Step 5: Create the four reviewers and the approver**

`.claude/agents/factory-security-reviewer.md`:

````markdown
---
name: factory-security-reviewer
description: Security review of a factory diff for Rocket. Read-only. Ends with VERDICT APPROVE or CHANGES.
model: opus
tools: Read, Grep, Glob
maxTurns: 40
---
You are the security reviewer in Rocket's software factory. You are read-only.

Input: the path of a diff file, the path of the job's `spec.md`, and the worktree path for reading surrounding code. Review what the diff changes, plus the code it calls when you need it.

Check:

- the collection trust gate (requested versus allowed access) and the `{{process.env}}` gate, see `.claude/collection-trust.md`
- secrets and credentials: not logged, not stored in plain text, masked in conversations and errors
- auth configuration and variable resolution paths
- no shelling out to the git CLI, `git2` only
- no unwrap-style panics in production paths (tests excepted)
- Tauri IPC: new commands validate input, return stable errors, expose no filesystem or process access beyond what is needed
- CSP and `tauri.conf.json` changes
- new dependencies

Report findings as a numbered list. Each has `file:line`, a severity (blocker or minor) and a concrete fix. Any blocker means CHANGES.

Your last line, alone, must be exactly one of:

VERDICT: APPROVE
VERDICT: CHANGES
````

`.claude/agents/factory-ux-reviewer.md`:

````markdown
---
name: factory-ux-reviewer
description: UX review of a factory diff that touches the Rocket frontend. Read-only. Ends with VERDICT APPROVE or CHANGES.
model: sonnet
tools: Read, Grep, Glob
maxTurns: 40
---
You are the UX reviewer in Rocket's software factory. You are read-only.

Input: the path of a diff file, the path of the job's `spec.md`, and the worktree path.

Check the user-facing behaviour the diff creates:

- the main flow works end to end for a Postman-style API client user
- empty, loading and error states exist and say what to do next
- keyboard use: focus order, Enter and Escape behaviour, shortcuts not clashing with existing ones
- destructive actions are confirmed or undoable
- labels and messages are short and clear

Report findings as a numbered list with `file:line`, a severity (blocker or minor) and a concrete fix. Any blocker means CHANGES.

Your last line, alone, must be exactly one of:

VERDICT: APPROVE
VERDICT: CHANGES
````

`.claude/agents/factory-ui-reviewer.md`:

````markdown
---
name: factory-ui-reviewer
description: UI design review of a factory diff that touches the Rocket frontend. Read-only. Ends with VERDICT APPROVE or CHANGES.
model: sonnet
tools: Read, Grep, Glob
maxTurns: 40
---
You are the UI design reviewer in Rocket's software factory. You are read-only.

Input: the path of a diff file, the path of the job's `spec.md`, and the worktree path. Read `.claude/rules/frontend-component-guardrails.md` first.

Check against Rocket's hard rules:

- shadcn/ui primitives only: no raw `<button>`, `<input>`, `<dialog>`, `<select>` or `<form>`
- icons from `lucide-react` only, no inline SVG
- single-line variable-aware fields use `SingleLineEditor`, multi-line editors use Monaco (the AI Assistant `PromptEditor` is the one approved exception)
- Zustand: narrow selectors, no full destructuring of store state at the top of a component
- spacing, typography and colour follow the existing theme tokens, and work in light and dark themes

Report findings as a numbered list with `file:line`, a severity (blocker or minor) and a concrete fix. A broken hard rule is a blocker. Any blocker means CHANGES.

Your last line, alone, must be exactly one of:

VERDICT: APPROVE
VERDICT: CHANGES
````

`.claude/agents/factory-code-reviewer.md`:

````markdown
---
name: factory-code-reviewer
description: Code review of a factory diff for Rocket: correctness, DDD placement, serde rules, tests. Read-only. Ends with VERDICT APPROVE or CHANGES.
model: sonnet
tools: Read, Grep, Glob
maxTurns: 40
---
You are the code reviewer in Rocket's software factory. You are read-only.

Input: the path of a diff file, the path of the job's `spec.md`, and the worktree path. Read `.claude/rules/rust-ddd-boundaries.md`, `.claude/rules/tauri-ipc-boundaries.md` and the crate `CLAUDE.md` files for the crates the diff touches.

Check:

- the diff does what `spec.md` asks, and nothing outside it
- correctness: edge cases, error handling, no unwrap in production paths
- DDD placement: domain crates hold logic and traits only, `rocket-infra` holds I/O, `rocket-app` is trait-first, Tauri commands stay thin
- serde: `camelCase` renames on IPC DTOs only, persistence structs stay backward compatible with optional fields and defaults
- tests exist for the new behaviour and would fail without the change
- no leftover debug code, no dead code, no duplicated logic that already has a helper

Report findings as a numbered list with `file:line`, a severity (blocker or minor) and a concrete fix. Any blocker means CHANGES.

Your last line, alone, must be exactly one of:

VERDICT: APPROVE
VERDICT: CHANGES
````

`.claude/agents/factory-approver.md`:

````markdown
---
name: factory-approver
description: Final approver for a Rocket factory job. Reads all review reports and the diff, then decides APPROVE (merge) or ESCALATE (needs a human). Read-only.
model: opus
tools: Read, Grep, Glob
maxTurns: 30
---
You are the approver in Rocket's software factory. You are read-only.

Input: the job directory, the full diff file path, and the review report files (`review-*.md`).

Decide ESCALATE if any of these is true, otherwise APPROVE:

- the diff touches the collection trust gate, auth, secrets, or `{{process.env}}` handling
- the diff changes a persistence format: `.yml` schemas or serde persistence structs
- the diff changes the Tauri IPC surface or adds or changes `DomainEvent` variants
- the diff changes `Cargo.toml`, `package.json`, `tauri.conf.json`, the CSP, or the harness hooks
- a reviewer reported CHANGES that was not fixed
- any review report has no `VERDICT:` line
- the spec's acceptance criteria are not all covered by the diff and its tests

Write a short reasoned summary: which rules you checked and what you found, with `file:line` for anything that triggered ESCALATE. When you ESCALATE, the first line of the summary is a one-line reason.

Your last line, alone, must be exactly one of:

DECISION: APPROVE
DECISION: ESCALATE
````

- [ ] **Step 6: Run the lint to verify it passes**

Run: `bash .claude/factory/tests/run.sh`
Expected: `all passed`.

- [ ] **Step 7: Commit**

Use the `dev-workflow-skills:1-git-commit` skill with a pathspec commit of the seven `.claude/agents/factory-*.md` files and `.claude/factory/tests/run.sh`.
Suggested type: `feat(factory): add the seven factory subagents`.

---

### Task 3: `factory-ctl` helper and `/factory` command

**Files:**
- Create: `.claude/factory/factory-ctl`
- Create: `.claude/commands/factory.md`
- Modify: `.claude/factory/tests/run.sh` (ctl tests and command lint)

**Interfaces:**
- Consumes: the agent names and the `WORKTREE/BRANCH/BASE/HEAD`, `VERDICT:` and `DECISION:` lines from Task 2.
- Produces: the `factory-ctl` command line, used by the command and by plan 02:
  - `new-job "<feature>" [--slug S] [--date YYYY-MM-DD]` prints the folder name `YYYY-MM-DD-NNN-slug`.
  - `set-stage <id> <spec|build|review|approve|merged|needs-human> [--round N] [--review role=state]... [--reset-reviews] [--worktree P] [--builder-id I] [--base SHA] [--head SHA] [--reason T]`.
  - `ledger <id> "<line>"` appends a line to the job's `progress.md`.
  - `tick-plan <id> --merge SHA` ticks every `- [ ] Task N:` line in `plan.md` and appends `Task N: complete (merge SHA)` to the ledger, once per task.
  - `is-merged <id>` prints `merged <main sha>` and exits 0, or prints `not merged` and exits 1.
  - `status` prints one line per job.
  - `board.json` shape: `{"jobs": [{"id","dir","slug","created","feature","branch","worktree","builder_id","base","head","stage","round","reason","stage_since","reviews":{"security","ux","ui","code"}}]}` at `<main checkout>/.superpowers/factory/board.json`. Plan 02 reads it.

- [ ] **Step 1: Write the failing tests**

In `.claude/factory/tests/run.sh`, insert before the `# ---- summary ----` line:

```bash
# ---- factory-ctl ----
ctl="$factory/factory-ctl"
fr="$tmp/fr"
git init -q -b main "$fr"
git -C "$fr" -c user.email=t@t -c user.name=t commit -q --allow-empty -m init
export FACTORY_ROOT="$fr"
board="$fr/.superpowers/factory/board.json"

name="$(python3 "$ctl" new-job "Fix the import bug" --date 2026-10-13)"
check "ctl: new-job names the folder with the date" test "$name" = "2026-10-13-001-fix-the-import-bug"
check "ctl: new-job creates the ledger" test -f "$fr/.superpowers/sdd/$name/progress.md"
name2="$(python3 "$ctl" new-job "Second one" --date 2026-10-13)"
check "ctl: second job gets id 002" test "$name2" = "2026-10-13-002-second-one"
name3="$(python3 "$ctl" new-job "Third one" --date 2026-10-13)"

python3 "$ctl" set-stage 1 build --round 1 --review security=pass --review ux=skipped --worktree /w --builder-id agent-9 >/dev/null
check "ctl: set-stage stores stage, round, reviews and ids" \
  jq -e '.jobs[0] | select(.stage=="build" and .round==1 and .reviews.security=="pass" and .reviews.ux=="skipped" and .builder_id=="agent-9" and .worktree=="/w")' "$board"
check "ctl: set-stage rejects an unknown stage" bash -c "! python3 '$ctl' set-stage 1 nonsense"
check "ctl: set-stage rejects an unknown review state" bash -c "! python3 '$ctl' set-stage 1 build --review code=maybe"
before="$(cat "$board")"
check "ctl: a non-numeric job id fails" bash -c "! python3 '$ctl' set-stage abc build"
check "ctl: an unknown job id fails" bash -c "! python3 '$ctl' set-stage 99 build"
check "ctl: failed calls leave the board unchanged" test "$before" = "$(cat "$board")"
python3 "$ctl" set-stage 1 review --reset-reviews >/dev/null
check "ctl: --reset-reviews sets every review to pending" \
  jq -e '.jobs[0].reviews | [.security,.ux,.ui,.code] | all(. == "pending")' "$board"

python3 "$ctl" ledger 1 "Task 1: dispatched (BASE abc, opus)" >/dev/null
check "ctl: ledger appends a line" grep -q '^Task 1: dispatched' "$fr/.superpowers/sdd/$name/progress.md"

plan="$fr/.superpowers/sdd/$name/plan.md"
ledger="$fr/.superpowers/sdd/$name/progress.md"
printf '%s\n' '# Plan' '- [ ] Task 1: first' '- [ ] Task 2: second' 'text - [ ] Task 3: not a task line' >"$plan"
python3 "$ctl" tick-plan 1 --merge abc123 >/dev/null
check "ctl: tick-plan checks every task line" \
  bash -c "grep -q '^- \[x\] Task 1: first' '$plan' && grep -q '^- \[x\] Task 2: second' '$plan'"
check "ctl: tick-plan leaves other text alone" grep -q 'text - \[ \] Task 3' "$plan"
check "ctl: tick-plan writes ledger lines" grep -q '^Task 2: complete (merge abc123)' "$ledger"
python3 "$ctl" tick-plan 1 --merge abc123 >/dev/null
check "ctl: tick-plan twice does not duplicate ledger lines" test "$(grep -c '^Task 1: complete' "$ledger")" -eq 1

git -C "$fr" switch -q -c factory/001-fix-the-import-bug
git -C "$fr" -c user.email=t@t -c user.name=t commit -q --allow-empty -m work
head="$(git -C "$fr" rev-parse HEAD)"
base="$(git -C "$fr" rev-parse main)"
python3 "$ctl" set-stage 1 approve --base "$base" --head "$head" >/dev/null
check "ctl: is-merged says no before the merge" bash -c "! python3 '$ctl' is-merged 1"
git -C "$fr" switch -q main
git -C "$fr" -c user.email=t@t -c user.name=t merge -q --no-ff -m merge factory/001-fix-the-import-bug
check "ctl: is-merged says yes after a manual merge" python3 "$ctl" is-merged 1
git -C "$fr" branch -q -D factory/001-fix-the-import-bug
check "ctl: is-merged still works after the branch is deleted" python3 "$ctl" is-merged 1
python3 "$ctl" set-stage 2 approve --base "$base" --head "$base" >/dev/null
check "ctl: is-merged refuses a job with no commits" bash -c "! python3 '$ctl' is-merged 2"
check "ctl: is-merged refuses a job with no recorded head" bash -c "! python3 '$ctl' is-merged 3"
check "ctl: status lists every job" test "$(python3 "$ctl" status | wc -l)" -eq 3
unset FACTORY_ROOT

# ---- /factory command ----
cmd="$repo/.claude/commands/factory.md"
for word in 'new-job' 'set-stage' 'tick-plan' 'is-merged' 'status' 'next' 'approve' 'rework' \
  'factory-spec-writer' 'factory-builder' 'factory-security-reviewer' 'factory-ux-reviewer' \
  'factory-ui-reviewer' 'factory-code-reviewer' 'factory-approver' 'no VERDICT line' 'main busy'; do
  grep -qF -- "$word" "$cmd" && pass "command mentions: $word" || fail "command mentions: $word"
done
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bash .claude/factory/tests/run.sh`
Expected: FAIL for every `ctl:` and `command mentions:` line.

- [ ] **Step 3: Write `factory-ctl`**

Create `.claude/factory/factory-ctl` and make it executable (`chmod +x`):

```python
#!/usr/bin/env python3
"""Board and ledger helper for Rocket's software factory.

The orchestrator calls this instead of editing board.json, ledgers or plan
checkboxes by hand. Set FACTORY_ROOT to override the main checkout root.
"""
import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from datetime import date

STAGES = ("spec", "build", "review", "approve", "merged", "needs-human")
ROLES = ("security", "ux", "ui", "code")
REVIEW_STATES = ("pending", "running", "pass", "changes", "skipped")


def die(message):
    print(f"factory-ctl: {message}", file=sys.stderr)
    sys.exit(1)


def repo_root():
    override = os.environ.get("FACTORY_ROOT")
    if override:
        return override
    out = subprocess.run(
        ["git", "rev-parse", "--git-common-dir"], capture_output=True, text=True
    )
    if out.returncode != 0:
        die("not inside a git repository")
    return os.path.dirname(os.path.abspath(out.stdout.strip()))


def board_path(root):
    return os.path.join(root, ".superpowers", "factory", "board.json")


def load_board(root):
    try:
        with open(board_path(root), encoding="utf-8") as handle:
            return json.load(handle)
    except FileNotFoundError:
        return {"jobs": []}


def save_board(root, board):
    path = board_path(root)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=os.path.dirname(path), suffix=".tmp")
    with os.fdopen(fd, "w", encoding="utf-8") as handle:
        json.dump(board, handle, indent=2)
        handle.write("\n")
    os.replace(tmp, path)


def normalize_id(raw):
    if not re.fullmatch(r"\d+", raw):
        die(f"job id must be a number, got {raw!r}")
    return f"{int(raw):03d}"


def find_job(board, raw_id):
    job_id = normalize_id(raw_id)
    for job in board["jobs"]:
        if job["id"] == job_id:
            return job
    die(f"no job with id {job_id}")


def slugify(text):
    slug = re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")[:40].strip("-")
    return slug or "job"


def job_dir(root, job):
    return os.path.join(root, ".superpowers", "sdd", job["dir"])


def git(root, *args):
    return subprocess.run(["git", "-C", root, *args], capture_output=True, text=True)


def cmd_new_job(args):
    root = repo_root()
    board = load_board(root)
    number = max((int(job["id"]) for job in board["jobs"]), default=0) + 1
    job_id = f"{number:03d}"
    slug = slugify(args.slug or args.feature)
    stamp = args.date or date.today().isoformat()
    name = f"{stamp}-{job_id}-{slug}"
    path = os.path.join(root, ".superpowers", "sdd", name)
    os.makedirs(path)
    with open(os.path.join(path, "progress.md"), "w", encoding="utf-8") as handle:
        handle.write(f"# SDD ledger — plan: .superpowers/sdd/{name}/plan.md\n\n")
    board["jobs"].append(
        {
            "id": job_id,
            "dir": name,
            "slug": slug,
            "created": stamp,
            "feature": args.feature,
            "branch": f"factory/{job_id}-{slug}",
            "worktree": None,
            "builder_id": None,
            "base": None,
            "head": None,
            "stage": "spec",
            "round": 0,
            "reason": None,
            "stage_since": time.time(),
            "reviews": {role: "pending" for role in ROLES},
        }
    )
    save_board(root, board)
    print(name)


def parse_review(text):
    role, sep, state = text.partition("=")
    if not sep or role not in ROLES or state not in REVIEW_STATES:
        die(f"bad --review {text!r}, use role=state with role in {ROLES} and state in {REVIEW_STATES}")
    return role, state


def cmd_set_stage(args):
    root = repo_root()
    board = load_board(root)
    job = find_job(board, args.id)
    reviews = [parse_review(text) for text in args.review]
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
    for field in ("worktree", "builder_id", "base", "head"):
        value = getattr(args, field)
        if value is not None:
            job[field] = value
    save_board(root, board)


def cmd_ledger(args):
    root = repo_root()
    job = find_job(load_board(root), args.id)
    with open(os.path.join(job_dir(root, job), "progress.md"), "a", encoding="utf-8") as handle:
        handle.write(args.line.rstrip("\n") + "\n")


def cmd_tick_plan(args):
    root = repo_root()
    job = find_job(load_board(root), args.id)
    base = job_dir(root, job)
    plan_path = os.path.join(base, "plan.md")
    try:
        with open(plan_path, encoding="utf-8") as handle:
            text = handle.read()
    except FileNotFoundError:
        die("plan.md is missing")
    ticked = []

    def tick(match):
        ticked.append(match.group(2))
        return f"- [x] {match.group(1)}"

    updated = re.sub(r"^- \[ \] (Task (\d+):)", tick, text, flags=re.MULTILINE)
    if not ticked:
        print("nothing to tick")
        return
    with open(plan_path, "w", encoding="utf-8") as handle:
        handle.write(updated)
    with open(os.path.join(base, "progress.md"), "a", encoding="utf-8") as handle:
        for number in ticked:
            handle.write(f"Task {number}: complete (merge {args.merge})\n")
    print("ticked task " + ", ".join(ticked))


def cmd_is_merged(args):
    root = repo_root()
    job = find_job(load_board(root), args.id)
    if not job.get("head"):
        die("job has no recorded head, run set-stage --head first")
    if job.get("head") == job.get("base"):
        die("job has no commits, head equals base")
    result = git(root, "merge-base", "--is-ancestor", job["head"], "main")
    if result.returncode == 0:
        print("merged " + git(root, "rev-parse", "main").stdout.strip())
        return
    if result.returncode == 1:
        print("not merged")
        sys.exit(1)
    die(result.stderr.strip() or "git merge-base failed")


def cmd_status(_args):
    board = load_board(repo_root())
    if not board["jobs"]:
        print("no jobs")
        return
    for job in board["jobs"]:
        chips = " ".join(f"{role}:{job['reviews'][role]}" for role in ROLES)
        print(
            f"{job['id']}  {job['created']}  {job['stage']:<11} "
            f"round {job['round']}  {chips}  {job['slug']}"
        )


def build_parser():
    parser = argparse.ArgumentParser(prog="factory-ctl")
    sub = parser.add_subparsers(dest="command", required=True)

    new_job = sub.add_parser("new-job")
    new_job.add_argument("feature")
    new_job.add_argument("--slug")
    new_job.add_argument("--date")
    new_job.set_defaults(func=cmd_new_job)

    set_stage = sub.add_parser("set-stage")
    set_stage.add_argument("id")
    set_stage.add_argument("stage", choices=STAGES)
    set_stage.add_argument("--round", type=int)
    set_stage.add_argument("--review", action="append", default=[])
    set_stage.add_argument("--reset-reviews", action="store_true")
    set_stage.add_argument("--worktree")
    set_stage.add_argument("--builder-id", dest="builder_id")
    set_stage.add_argument("--base")
    set_stage.add_argument("--head")
    set_stage.add_argument("--reason")
    set_stage.set_defaults(func=cmd_set_stage)

    ledger = sub.add_parser("ledger")
    ledger.add_argument("id")
    ledger.add_argument("line")
    ledger.set_defaults(func=cmd_ledger)

    tick = sub.add_parser("tick-plan")
    tick.add_argument("id")
    tick.add_argument("--merge", required=True)
    tick.set_defaults(func=cmd_tick_plan)

    merged = sub.add_parser("is-merged")
    merged.add_argument("id")
    merged.set_defaults(func=cmd_is_merged)

    status = sub.add_parser("status")
    status.set_defaults(func=cmd_status)
    return parser


def main():
    args = build_parser().parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
```

- [ ] **Step 4: Run the tests to verify the ctl checks pass**

Run: `bash .claude/factory/tests/run.sh`
Expected: every `ctl:` line is PASS. Only the `command mentions:` lines still fail.

- [ ] **Step 5: Write the `/factory` command**

Create `.claude/commands/factory.md`:

````markdown
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
````

- [ ] **Step 6: Run the tests to verify everything passes**

Run: `bash .claude/factory/tests/run.sh`
Expected: `all passed`.

- [ ] **Step 7: Smoke test the command**

In the real checkout run `python3 .claude/factory/factory-ctl status`. Expected: `no jobs`. Do not run `/factory` on a real feature yet.

- [ ] **Step 8: Commit**

Use the `dev-workflow-skills:1-git-commit` skill with a pathspec commit of `.claude/factory/factory-ctl`, `.claude/commands/factory.md`, `.claude/factory/tests/run.sh`.
Suggested type: `feat(factory): add factory-ctl and the /factory command`.
