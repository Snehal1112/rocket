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
