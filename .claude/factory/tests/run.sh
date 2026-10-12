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
  if fm "$file" description | grep -qF ': '; then fail "agent $name: description has no colon-space"; else pass "agent $name: description has no colon-space"; fi
  case "$kind" in
    read-only)
      [ "$(fm "$file" tools)" = "Read, Grep, Glob" ] && pass "agent $name: tools are exactly Read, Grep, Glob" || fail "agent $name: tools are exactly Read, Grep, Glob"
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
check "ctl: tick-plan twice does not duplicate Task 2 either" test "$(grep -c '^Task 2: complete' "$ledger")" -eq 1
printf '%s\n' '# SDD ledger' >"$ledger"
printf '%s\n' '# Plan' '- [x] Task 1: first' '- [x] Task 2: second' >"$plan"
python3 "$ctl" tick-plan 1 --merge def456 >/dev/null
python3 "$ctl" tick-plan 1 --merge def456 >/dev/null
check "ctl: tick-plan backfills the ledger for a hand-ticked plan" \
  bash -c "test \"\$(grep -c '^Task 1: complete' '$ledger')\" -eq 1 && test \"\$(grep -c '^Task 2: complete' '$ledger')\" -eq 1"

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

# ---- summary ----
if [ "$fails" -eq 0 ]; then echo "all passed"; else echo "$fails failed"; fi
exit $((fails > 0))
