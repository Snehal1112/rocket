#!/usr/bin/env bash
# Self-contained tests for the harness. Exits non-zero on any failure.
set -u

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
harness="$(cd "$here/.." && pwd)"
repo="$(cd "$harness/../.." && pwd)"
export PYTHONDONTWRITEBYTECODE=1
tmp="$(mktemp -d "${TMPDIR:-/tmp}/harness-test.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
export HARNESS_LOG_DIR="$tmp/log"

fails=0
pass() { echo "PASS: ${1//$'\n'/ }"; }
fail() { echo "FAIL: ${1//$'\n'/ }"; fails=$((fails + 1)); }

payload() {
  python3 -c 'import json,sys;print(json.dumps({"tool_name":sys.argv[3],"tool_input":{"command":sys.argv[1]},"cwd":sys.argv[2]}))' "$1" "${2:-$tmp}" "${3:-Bash}" 2>/dev/null
}

# Usage: guard_case <allow|deny> <rule-or-""> <command>
guard_case() {
  local want="$1" rule="$2" cmd="$3" out rc
  out="$(payload "$cmd" | bash "$harness/hooks/bash-guard")"
  rc=$?
  if [ "$want" = allow ]; then
    if [ $rc -eq 0 ] && [ -z "$out" ]; then pass "allow: ${cmd:0:60}"; else fail "allow: ${cmd:0:60} (rc=$rc)"; fi
  else
    if [ $rc -eq 2 ] && grep -q '"permissionDecision": "deny"' <<<"$out" && grep -q "harness:$rule" <<<"$out"; then
      pass "deny($rule): ${cmd:0:60}"
    else
      fail "deny($rule): ${cmd:0:60} (rc=$rc)"
    fi
  fi
}

# Guard denies.
guard_case deny cargo-no-j "cargo test -p rocket-http"
guard_case deny cargo-no-j "cargo check"
guard_case deny cargo-no-j "cd x && cargo clippy --all-targets"
guard_case deny cargo-no-j "cargo build --release"
guard_case deny cargo-test-workspace "cargo test -j4 --workspace"
guard_case deny cargo-test-workspace "cargo test -j4 --all"
guard_case deny git-stash-bare "git stash"
guard_case deny git-stash-bare "git stash pop"
guard_case deny git-stash-bare "git status; git stash"
guard_case deny git-add-all "git add -A"
guard_case deny git-add-all "git add --all"
guard_case deny git-add-all "git add ."
guard_case deny git-add-all "echo hi | cat && git add . && git status"

# Guard allows.
guard_case allow "" "cargo test -j4 -p x"
guard_case allow "" "cargo check -j4"
guard_case allow "" "cargo fmt"
guard_case allow "" "git stash push -u -m tag"
guard_case allow "" "git stash list"
guard_case allow "" "git stash apply abc123"
guard_case allow "" "git stash drop stash@{0}"
guard_case allow "" "git add src/a.rs docs/b.md"
guard_case allow "" "echo 'run git add -A and cargo test later'"
guard_case allow "" $'git commit -m "$(cat <<\'EOF\'\nfix: mention git add -A and cargo test\n\ngit stash pop\nEOF\n)"'
guard_case allow "" $'cat <<EOF\ngit add .\ncargo test\nEOF'
guard_case allow "" "not valid 'quote"

# Non-Bash tools and bad payloads pass.
out="$(payload "git add -A" "$tmp" Edit | bash "$harness/hooks/bash-guard")"; rc=$?
[ $rc -eq 0 ] && [ -z "$out" ] && pass "allow: non-Bash tool" || fail "allow: non-Bash tool"
out="$(echo 'not json' | bash "$harness/hooks/bash-guard")"; rc=$?
[ $rc -eq 0 ] && [ -z "$out" ] && pass "allow: unparseable payload" || fail "allow: unparseable payload"

# Guard log.
if [ -f "$tmp/log/guard.log" ] && [ "$(awk -F'\t' 'NF==3' "$tmp/log/guard.log" | wc -l)" -eq "$(wc -l <"$tmp/log/guard.log")" ] \
  && grep -q $'\tgit-add-all\t' "$tmp/log/guard.log"; then
  pass "guard log has timestamp, rule and command columns"
else
  fail "guard log format"
fi

# Commit gate with fake yarn and cargo.
repo2="$tmp/gaterepo"
mkdir -p "$repo2" "$tmp/bin"
git -C "$repo2" init -q
cat >"$tmp/bin/yarn" <<EOF
#!/usr/bin/env bash
echo "yarn \$*" >>"$tmp/calls"
[ -n "\${FAKE_FAIL:-}" ] && { echo "boom from yarn"; exit 1; }
exit 0
EOF
cat >"$tmp/bin/cargo" <<EOF
#!/usr/bin/env bash
echo "cargo \$*" >>"$tmp/calls"
exit 0
EOF
chmod +x "$tmp/bin/yarn" "$tmp/bin/cargo"
gate() { payload "$1" "$repo2" | PATH="$tmp/bin:$PATH" bash "$harness/hooks/commit-gate"; }

: >"$tmp/calls"
gate "git status && cargo check -j4" >/dev/null; rc=$?
[ $rc -eq 0 ] && [ ! -s "$tmp/calls" ] && pass "gate: non-commit runs nothing" || fail "gate: non-commit runs nothing"

echo x >"$repo2/a.ts"; git -C "$repo2" add a.ts
HARNESS_GATE=0 gate "git commit -m x" >/dev/null; rc=$?
[ $rc -eq 0 ] && [ ! -s "$tmp/calls" ] && pass "gate: HARNESS_GATE=0 skips" || fail "gate: HARNESS_GATE=0 skips"

out="$(gate "git commit -m x")"; rc=$?
if [ $rc -eq 0 ] && grep -q "yarn tsc --noEmit" "$tmp/calls" && grep -q "yarn check" "$tmp/calls" && ! grep -q cargo "$tmp/calls"; then
  pass "gate: staged ts runs tsc and check"
else
  fail "gate: staged ts runs tsc and check"
fi

: >"$tmp/calls"
out="$(FAKE_FAIL=1 gate "git commit -m x")"; rc=$?
if [ $rc -eq 2 ] && grep -q "boom from yarn" <<<"$out" && grep -q commit-gate-fail "$tmp/log/guard.log"; then
  pass "gate: failing check denies with output and logs"
else
  fail "gate: failing check denies (rc=$rc)"
fi

git -C "$repo2" reset -q; echo y >"$repo2/b.rs"; git -C "$repo2" add b.rs
: >"$tmp/calls"
gate "git commit -m x" >/dev/null; rc=$?
if [ $rc -eq 0 ] && grep -q "cargo check -j4" "$tmp/calls" && ! grep -q yarn "$tmp/calls"; then
  pass "gate: staged rs runs cargo check -j4"
else
  fail "gate: staged rs runs cargo check -j4"
fi

# Install and uninstall on a temp copy.
proj="$tmp/proj"
mkdir -p "$proj/.claude/rules"
cp "$repo/.claude/settings.json" "$proj/.claude/settings.json"
cp "$repo/.claude/rules/00-shortcuts.md" "$proj/.claude/rules/00-shortcuts.md"
# The pre-harness files must not already contain harness entries.
cp "$proj/.claude/settings.json" "$tmp/settings.orig"
cp "$proj/.claude/rules/00-shortcuts.md" "$tmp/shortcuts.orig"
if grep -q "harness" "$tmp/settings.orig"; then
  echo "NOTE: repo settings already installed, stripping first."
  HARNESS_ROOT="$proj" bash "$harness/uninstall.sh" >/dev/null
  cp "$proj/.claude/settings.json" "$tmp/settings.orig"
  cp "$proj/.claude/rules/00-shortcuts.md" "$tmp/shortcuts.orig"
fi

# Use a copy of the harness dir inside the temp project so purge cannot touch the repo.
cp -r "$harness" "$proj/.claude/harness"
rm -rf "$proj/.claude/harness/log" "$proj/.claude/harness/backup" "$proj/.claude/harness/manifest.json"
ph="$proj/.claude/harness"

bash "$ph/install.sh" --root "$proj" >/dev/null && pass "install runs" || fail "install runs"
cp "$proj/.claude/settings.json" "$tmp/settings.1"
cp "$proj/.claude/rules/00-shortcuts.md" "$tmp/shortcuts.1"
cp "$ph/manifest.json" "$tmp/manifest.1"
HARNESS_ROOT="$proj" bash "$ph/install.sh" >/dev/null && pass "install runs twice" || fail "install runs twice"
cmp -s "$tmp/settings.1" "$proj/.claude/settings.json" && cmp -s "$tmp/shortcuts.1" "$proj/.claude/rules/00-shortcuts.md" \
  && cmp -s "$tmp/manifest.1" "$ph/manifest.json" && pass "install is idempotent" || fail "install is idempotent"
python3 - "$proj/.claude/settings.json" <<'EOF' && pass "install content is correct" || fail "install content is correct"
import json, sys
d = json.load(open(sys.argv[1]))
pre = d["hooks"]["PreToolUse"]
cmds = [h["command"] for e in pre for h in e["hooks"]]
assert any("rust-safety-pretool" in c for c in cmds)
assert sum("/.claude/harness/hooks/" in c for c in cmds) == 2
gate = [h for e in pre for h in e["hooks"] if "commit-gate" in h["command"]][0]
assert gate["timeout"] == 600
assert all(e["matcher"] == "Bash" for e in pre if "harness" in json.dumps(e))
assert len(d["permissions"]["deny"]) == 3
assert d["worktree"]["baseRef"] == "head"
EOF
[ "$(grep -c 'harness:begin' "$proj/.claude/rules/00-shortcuts.md")" -eq 1 ] && pass "shortcuts block added once" || fail "shortcuts block added once"
cmp -s "$tmp/settings.orig" "$ph/backup/settings.json.pre-harness" && pass "backup equals original" || fail "backup equals original"

bash "$ph/uninstall.sh" --root "$proj" >/dev/null && pass "uninstall runs" || fail "uninstall runs"
cmp -s "$tmp/settings.orig" "$proj/.claude/settings.json" && pass "settings.json byte-identical after uninstall" || fail "settings.json byte-identical after uninstall"
cmp -s "$tmp/shortcuts.orig" "$proj/.claude/rules/00-shortcuts.md" && pass "00-shortcuts.md byte-identical after uninstall" || fail "00-shortcuts.md byte-identical after uninstall"
[ ! -e "$ph/manifest.json" ] && [ -d "$ph/hooks" ] && pass "manifest removed, files left inert" || fail "manifest removed, files left inert"

# Uninstall without a manifest still removes entries.
bash "$ph/install.sh" --root "$proj" >/dev/null
rm -f "$ph/manifest.json"
bash "$ph/uninstall.sh" --root "$proj" >/dev/null
cmp -s "$tmp/settings.orig" "$proj/.claude/settings.json" && cmp -s "$tmp/shortcuts.orig" "$proj/.claude/rules/00-shortcuts.md" \
  && pass "uninstall without manifest restores files" || fail "uninstall without manifest restores files"

# Purge.
bash "$ph/install.sh" --root "$proj" >/dev/null
mkdir -p "$proj/.claude/skills/resume-plan"; touch "$proj/.claude/rules/harness.md"
bash "$ph/uninstall.sh" --root "$proj" --purge >/dev/null
if [ ! -e "$proj/.claude/harness" ] && [ ! -e "$proj/.claude/skills/resume-plan" ] && [ ! -e "$proj/.claude/rules/harness.md" ] \
  && cmp -s "$tmp/settings.orig" "$proj/.claude/settings.json"; then
  pass "purge deletes harness files"
else
  fail "purge deletes harness files"
fi

echo
if [ "$fails" -eq 0 ]; then echo "ALL PASSED"; else echo "$fails FAILED"; exit 1; fi
