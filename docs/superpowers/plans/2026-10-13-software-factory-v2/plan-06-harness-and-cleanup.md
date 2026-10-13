# Software Factory v2 Plan 06: Harness and Cleanup

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task, executed by a factory-builder subagent. Steps use checkbox (`- [ ]`) syntax for tracking. Plans 01 to 05 of this series must be finished and merged first, because this plan edits files they change (`factory-ctl`, the agent prompts, `log-event`, the server and the dashboard page). Read each file as it is on disk before editing and adapt the code below to it. Do not redo anything plans 01 to 05 already did.

**Goal:** Make `/verify-rocket` obey the harness guard, clear the small v1 leftovers that no earlier plan covers, look at the real dashboard in a headless browser, and close the series with synced docs, a roadmap entry and a success-check recipe for the user.

**Architecture:** Three independent tasks. Task 1 changes one command doc, one line of the root `CLAUDE.md` and adds a lint to the harness tests that runs every cargo command found in those docs through the real `bash-guard`. Task 2 adds tests first, then small fixes, in `factory-ctl`, `board.js`, `smoke.sh` and `seed-demo.sh`. Task 3 adds a screenshot script and then writes docs.

**Tech Stack:** Bash, Python 3 (`factory-ctl`), vanilla JavaScript with Node 22 `node --test`, a headless Chrome or Firefox, Markdown.

**Spec:** [docs/superpowers/specs/2026-10-13-software-factory-v2-design.md](../../specs/2026-10-13-software-factory-v2-design.md), gaps E10, E11, E12 and the success check. Design doc: [.claude/software-factory.md](../../../../.claude/software-factory.md).

## Global Constraints

- Cargo commands use `-j4` and never `--workspace` or `--all`. Factory-server commands are `cargo check -j4 --manifest-path tools/factory-server/Cargo.toml` and the same shape for `test`.
- Commits use the skill `dev-workflow-skills:1-git-commit` and a pathspec commit: `git add <paths> && git commit --only -m "..." -- <paths>`. Never `git add -A`, `--all` or `.`. Conventional commit format. If the skill cannot run in the subagent, say so in the handoff and use a conventional message.
- Ledger writes go through `ctl` only (spec decision 8). The builder never edits the ledger by hand.
- Docs stay under `.claude/`. Root `CLAUDE.md` stays small: change one existing line, add no section.
- The dashboard is dev tooling: files only under `tools/factory-server/`. No external requests, no new npm packages.
- Every value from `board.json` or the event log goes through `escapeHtml` before it reaches an HTML string.
- Rust: no `unwrap()`. This plan changes no Rust.
- Code comments are short full sentences ending in a period. No emojis.
- The success-check dry run is written for the user and is not executed by the builder.

## Review Focus

- A cargo command that is added to `verify-rocket.md` or `CLAUDE.md` later without `-j4` must fail a test. Tested in Task 1.
- `/verify-rocket` on a change that touches no Rust crate must say "no Rust crate changed" and not run a workspace-wide build. Task 1, command text.
- A plan file with two lines for the same task number must not write two ledger lines. Tested in Task 2.
- A crash while writing `plan.md` in `tick-plan` must not leave a `.tmp` file in the job folder. Tested in Task 2.
- A job whose stage the page does not know must be listed on the page, not only counted. Tested in Task 2.
- `renderBoard(null)` or a feed event with no timestamp must not blank the page. Tested in Task 2.
- Running `seed-demo.sh` twice on the same folder must not fail on a second `git init` or double the jobs, and it must refuse a folder it did not create. Tested in Task 2.
- No headless browser installed must be reported as BLOCKED with manual steps, not skipped silently. Task 3.

## File Structure

| File | Task | Change |
|---|---|---|
| `.claude/commands/verify-rocket.md` | 1 | Per-crate `-j4` checks, no workspace command. |
| `CLAUDE.md` | 1 | Two commands lines and one prose line get `-j4`. |
| `.claude/harness/tests/run.sh` | 1 | Lint: cargo commands in the two docs pass the guard. |
| `.claude/factory/tests/run.sh` | 2 | Last-line verdict lint, maxTurns lint, strong `is-merged` checks, `tick-plan` tests, seed-demo test. |
| `.claude/factory/factory-ctl` | 2 | `tick-plan`: unique task numbers and temp file cleanup. |
| `tools/factory-server/static/board.js` | 2 | Null-safe renders, unknown-stage column, `clock` fallback, merged tie-break, `datetime` attributes. |
| `tools/factory-server/tests/board.test.js` | 2 | Tests for the above and for escaping. |
| `tools/factory-server/tests/smoke.sh` | 2 | Polling, server log on failure, no dead helper. |
| `tools/factory-server/tests/seed-demo.sh` | 2 | Idempotent, refuses foreign folders, explains the sleep. |
| `tools/factory-server/tests/screenshots.sh` | 3 | New: seeds, serves, screenshots. |
| `.claude/software-factory.md` | 3 | Dated ruling, open items cleaned, success check. |
| `.claude/roadmap.md` | 3 | Factory v2 row and decision line. |

## v1 minors triage (E12)

This table is the decision record. Task 2 implements every "fix here" row. Every other row needs no work, but Task 3 copies the "dropped" rows into the design doc as known and accepted.

| v1 minor (ledger) | Verdict |
|---|---|
| log-event keeps url query strings, log is 0644 | covered by plan 05 |
| log never rotated, registry not reset on rotation, in-place truncation | covered by plan 05 |
| `skipped_lines` never shown | covered by plan 05 |
| security headers, header test does not check the full CSP | covered by plan 05 |
| `new-job` half-created job, `--date` and `--slug` unvalidated, board.json mode 0600 | covered by plan 01 and plan 05 |
| corrupt `board.json` traceback, half-written board shows invalid | covered by plan 01 |
| merge commit message not via commit skill | covered by plan 03 |
| builder id vs hook agent id | covered by plan 01 (confirmed in job 001) |
| builder-fallback overwrites the board base | covered by plan 01 (A9 base guard) |
| `Head round` ledger line wording in `factory.md` | covered by plan 01 (`prev_head`, `diff-range`) |
| hardcoded `main`, merge step should use `git -C <main checkout>` | covered by plan 03 (base check, `sync-base`) |
| same-stage ties go to the first job, `innerHTML` rewritten every second | covered by plan 05 (E2, E8) |
| builder text "never edit plan.md" ambiguous | covered by plan 02 (prompt rewrite) |
| verdict lint does not check the line is last | fix here (Task 2) |
| `maxTurns` not linted | fix here (Task 2) |
| `! is-merged` tests pass on any error, `name3` unused | fix here (Task 2) |
| `tick-plan` temp file not cleaned, duplicate task numbers | fix here (Task 2) |
| smoke test uses fixed sleeps, prints no `server.log` on failure, dead `api()` helper, weak garbage-line check | fix here (Task 2) |
| `seed-demo.sh` not idempotent, `sleep` unexplained | fix here (Task 2) |
| unknown-stage jobs counted but not listed | fix here (Task 2) |
| `clock()` with missing ts shows `Invalid D` | fix here (Task 2) |
| `renderBoard(null)` throws | fix here (Task 2) |
| merged sort has no tie-break | fix here (Task 2) |
| `<time>` has no `datetime` attribute | fix here (Task 2) |
| escaping tests missing for agent fields, slug, branch, reason, feed target | fix here (Task 2) |
| muted text contrast, duplicated `.dot` rules | checked by eye in Task 3, fixed there only if the screenshots show a real problem |
| `app.js` in-flight guard | already present (`inFlight` in `app.js`), nothing to do |
| subcommand dispatch on first word collides with feature text such as "status page for X" | dropped, the orchestrator passes the feature as a quoted argument to `new-job` and nothing else parses it |
| `needs-human` without `--reason` | dropped, the card already falls back to the text "needs a human" |
| `os.write` short write and lone surrogates dropped silently | dropped, the hook must never block an agent, and a lost line is only a missing feed row |
| every tool call spawns python3 and git | dropped, the hook is async and the cost is accepted |
| `tool_target` and `last_message` truncation not covered by tests | covered by plan 05 (it rewrites the hook and its tests); if a case is still missing after plan 05, note it in the handoff |
| spec-writer `Write` cannot be scoped to the job dir | dropped, the platform cannot scope a tool to a path |
| lossy utf8 decode per chunk | dropped, hook output is ASCII |
| server test minors (stale test never sets `last_tool_ts`, `keep_unassigned` paths untested, socket test thread never stopped, tests use `expect`, port TOCTOU) | dropped, plan 05 reworks `state.rs` and these are test hygiene with no user impact |

---

### Task 1: Make `/verify-rocket` and the docs obey the harness guard

**Files:**
- Modify: `.claude/commands/verify-rocket.md`
- Modify: `CLAUDE.md` (lines 21, 22 and 55 at the time of writing)
- Modify: `.claude/harness/tests/run.sh` (insert before the final `echo` summary)

**Interfaces:**
- Consumes: `guard_case <allow|deny> <rule> <command>` and `payload` from the existing harness test file. The guard denies `cargo test|check|clippy|build` without `-j`, and `cargo test` with `--workspace` or `--all`.
- Produces: a lint that every `cargo check|test|clippy|build` command written in `verify-rocket.md` or `CLAUDE.md` is allowed by `bash-guard`.

- [ ] **Step 1: Write the failing lint**

In `.claude/harness/tests/run.sh`, insert this block above the last `echo` and the `ALL PASSED` line. Use `grep -n '^echo$' .claude/harness/tests/run.sh | tail -1` to find the spot.

```bash
# ---- cargo commands written in the docs pass the guard ----
for doc in "$repo/.claude/commands/verify-rocket.md" "$repo/CLAUDE.md"; do
  found=0
  while IFS= read -r c; do
    found=$((found + 1))
    guard_case allow "" "$c"
  done < <(grep -oE 'cargo (check|test|clippy|build)[^`|]*' "$doc" | sed 's/[[:space:]]*$//')
  if [ "$found" -gt 0 ]; then pass "doc lint: found $found cargo commands in $(basename "$doc")"; else fail "doc lint: no cargo commands found in $(basename "$doc")"; fi
done
if grep -qE -- '--workspace|cargo [a-z]+ --all' "$repo/.claude/commands/verify-rocket.md"; then
  fail "doc lint: verify-rocket.md has no --workspace or --all"
else
  pass "doc lint: verify-rocket.md has no --workspace or --all"
fi
```

- [ ] **Step 2: Run it and see it fail**

Run: `bash .claude/harness/tests/run.sh 2>&1 | grep -E 'FAIL|doc lint'`
Expected: FAIL lines for the bare `cargo check`, `cargo test --workspace --no-run` and the bare `cargo test -p <crate>` in `CLAUDE.md`.

- [ ] **Step 3: Rewrite `/verify-rocket`**

Replace steps 3 to 5 of `.claude/commands/verify-rocket.md` (everything from `3. For rust or all` to the end of the table) with the text below. Keep the frontmatter, steps 1 and 2 and the Conventions section.

````markdown
3. For rust or all, check only the crates that changed. From repo root:
- List changed paths: `git diff --name-only main -- crates src-tauri` plus `git status --porcelain -- crates src-tauri`.
- Map `crates/<name>/...` to the package `<name>` and `src-tauri/...` to the package `rocket`. Drop duplicates.
- If no crate changed, report "no Rust crate changed" and run no cargo command.
- For each changed package, one at a time, run:
  - cargo check -j4 -p <crate>
  - cargo test -j4 -p <crate> --no-run
- Never run a workspace-wide build or test, and never leave out `-j4`. The harness guard denies both.

4. For frontend or all, run from repo root:
- yarn tsc --noEmit
- yarn check

5. Report output in a strict table, one row per command that ran:

| Check | Status | Notes |
|---|---|---|
| cargo check -j4 -p <crate> | pass/fail | error count or key failure line |
| cargo test -j4 -p <crate> --no-run | pass/fail | error count or key failure line |
| yarn tsc --noEmit | pass/fail | error count or key failure line |
| yarn check | pass/fail | error count or key failure line |
````

- [ ] **Step 4: Fix the three lines in `CLAUDE.md`**

Make exactly these edits (keep everything else):

```text
cargo check               # fast Rust validation       ->  cargo check -j4            # fast Rust validation
cargo test -p <crate> <test_name>                      ->  cargo test -j4 -p <crate> <test_name>
`cargo check` for fast validation.                     ->  `cargo check -j4` for fast validation.
```

- [ ] **Step 5: Run the lint and the whole harness suite**

Run: `bash .claude/harness/tests/run.sh 2>&1 | tail -8`
Expected: `ALL PASSED`, with `doc lint` PASS lines for both files.

- [ ] **Step 6: Prove the lint catches a regression**

Run: `cp .claude/commands/verify-rocket.md /tmp/vr.bak 2>/dev/null; printf '\n- cargo check\n' >> .claude/commands/verify-rocket.md; bash .claude/harness/tests/run.sh 2>&1 | grep -c '^FAIL'; mv /tmp/vr.bak .claude/commands/verify-rocket.md`
Expected: a count of at least 1, then the file is restored. Run `git diff --stat .claude/commands/verify-rocket.md` and confirm only the step 3 to 5 rewrite remains.

- [ ] **Step 7: Commit**

Invoke the skill `dev-workflow-skills:1-git-commit`, then:

```bash
git add .claude/commands/verify-rocket.md CLAUDE.md .claude/harness/tests/run.sh
git commit --only -m "fix(harness): make verify-rocket obey the cargo guard" -- .claude/commands/verify-rocket.md CLAUDE.md .claude/harness/tests/run.sh
```

---

### Task 2: Fix the remaining v1 minors, test first

**Files:**
- Modify: `.claude/factory/tests/run.sh`, `.claude/factory/factory-ctl`
- Modify: `tools/factory-server/static/board.js`, `tools/factory-server/tests/board.test.js`
- Modify: `tools/factory-server/tests/smoke.sh`, `tools/factory-server/tests/seed-demo.sh`

**Interfaces:**
- Consumes: `check`, `pass`, `fail` helpers and the `fm` function in `.claude/factory/tests/run.sh`; `FactoryBoard` exports; `cmd_tick_plan` in `factory-ctl`. Plans 01 to 05 changed several of these. Re-read each file first and keep their names.
- Produces: no new public names, except the helper `check_fails` (tests only) and a CSS class `column-unknown` (the unknown-stage column).

Part A: factory test suite (`.claude/factory/tests/run.sh`).

- [ ] **Step 1: Write the failing lints and checks**

Add after the existing verdict loop (search for `agent approver: decision lines`):

```bash
# The two verdict lines must be the last two non-empty lines of each reviewer prompt.
for role in security ux ui code; do
  f="$agents/factory-$role-reviewer.md"
  tail2="$(grep -v '^[[:space:]]*$' "$f" | tail -n 2 | paste -sd '|')"
  [ "$tail2" = "VERDICT: APPROVE|VERDICT: CHANGES" ] \
    && pass "agent $role reviewer: verdict lines are last" || fail "agent $role reviewer: verdict lines are last"
done
tail2="$(grep -v '^[[:space:]]*$' "$agents/factory-approver.md" | tail -n 2 | paste -sd '|')"
[ "$tail2" = "DECISION: APPROVE|DECISION: ESCALATE" ] \
  && pass "agent approver: decision lines are last" || fail "agent approver: decision lines are last"

# Every factory agent has a turn limit between 1 and 200.
for f in "$agents"/factory-*.md; do
  turns="$(fm "$f" maxTurns)"
  if [[ "$turns" =~ ^[0-9]+$ ]] && [ "$turns" -ge 1 ] && [ "$turns" -le 200 ]; then
    pass "agent $(basename "$f" .md): maxTurns is $turns"
  else
    fail "agent $(basename "$f" .md): maxTurns is a number from 1 to 200 (got '$turns')"
  fi
done
```

If plan 02 intentionally put text after the verdict lines in a prompt, change that one lint to the new final lines and say so in the handoff. If the builder agent has no `maxTurns` (plan 02 gap B5), the lint failing is correct: add `maxTurns: 150` to `.claude/agents/factory-builder.md` frontmatter only if plan 02 did not already set it, and report it.

Replace the two weak `is-merged` checks and use `name3`:

```bash
# Usage: check_fails <description> <expected stderr text> <command...>
check_fails() {
  local description="$1" want="$2" out rc
  shift 2
  out="$("$@" 2>&1)"
  rc=$?
  if [ "$rc" -ne 0 ] && grep -qF -- "$want" <<<"$out"; then pass "$description"; else fail "$description (rc=$rc, out=$out)"; fi
}
```

Define it next to `check`, then replace:

```bash
check "ctl: is-merged refuses a job with no commits" bash -c "! python3 '$ctl' is-merged 2"
check "ctl: is-merged refuses a job with no recorded head" bash -c "! python3 '$ctl' is-merged 3"
```

with:

```bash
check_fails "ctl: is-merged refuses a job with no commits" "head equals base" python3 "$ctl" is-merged 2
check_fails "ctl: is-merged refuses a job with no recorded head" "no recorded head" python3 "$ctl" is-merged 3
check "ctl: the third job folder exists" test -d "$fr/.superpowers/sdd/$name3"
```

Add `tick-plan` tests after the `tick-plan backfills` check:

```bash
dup="$fr/.superpowers/sdd/$name2"
printf '%s\n' '# Plan' '- [ ] Task 1: one' '- [ ] Task 2: two' '- [ ] Task 2: two again' >"$dup/plan.md"
python3 "$ctl" tick-plan 2 --merge aaa111 >/dev/null
check "ctl: tick-plan with duplicate task numbers writes one ledger line each" \
  bash -c "test \"\$(grep -c '^Task 2: complete' '$dup/progress.md')\" -eq 1 && test \"\$(grep -c '^Task 1: complete' '$dup/progress.md')\" -eq 1"
check "ctl: tick-plan leaves no temp file after success" bash -c "! ls '$dup'/*.tmp"

printf '%s\n' '# Plan' '- [ ] Task 1: one' >"$dup/plan.md"
check "ctl: tick-plan removes its temp file when the write fails" python3 - "$ctl" "$dup" <<'PYEOF'
import glob
import importlib.machinery
import importlib.util
import os
import sys

loader = importlib.machinery.SourceFileLoader("factory_ctl", sys.argv[1])
spec = importlib.util.spec_from_loader("factory_ctl", loader)
mod = importlib.util.module_from_spec(spec)
loader.exec_module(mod)


def boom(*_args, **_kwargs):
    raise OSError("replace failed")


mod.os.replace = boom
args = mod.build_parser().parse_args(["tick-plan", "2", "--merge", "bbb222"])
try:
    args.func(args)
except OSError:
    pass
leftover = glob.glob(os.path.join(sys.argv[2], "*.tmp"))
assert not leftover, leftover
PYEOF
```

Part B: `seed-demo` test. Add before `# ---- /factory command ----`:

```bash
# ---- seed-demo ----
seed="$repo/tools/factory-server/tests/seed-demo.sh"
bash "$seed" "$tmp/seed" >/dev/null 2>&1
check "seed-demo: first run makes five jobs" test "$(FACTORY_ROOT="$tmp/seed" python3 "$ctl" status | wc -l)" -eq 5
check "seed-demo: a second run on the same folder succeeds" bash "$seed" "$tmp/seed"
check "seed-demo: a second run still makes five jobs" test "$(FACTORY_ROOT="$tmp/seed" python3 "$ctl" status | wc -l)" -eq 5
mkdir -p "$tmp/foreign" && touch "$tmp/foreign/keep.txt"
check_fails "seed-demo: refuses a non-empty folder it did not create" "not a seed-demo folder" bash "$seed" "$tmp/foreign"
check "seed-demo: the foreign folder is untouched" test -f "$tmp/foreign/keep.txt"
```

Careful: the `unset FACTORY_ROOT` later in the file does not matter, because each seed check sets `FACTORY_ROOT` inline.

- [ ] **Step 2: Run and see failures**

Run: `bash .claude/factory/tests/run.sh 2>&1 | grep FAIL`
Expected: FAIL for the temp-file test, the duplicate test (two Task 2 lines), the seed-demo second run and refusal. The verdict, maxTurns and `is-merged` lints should pass or show real prompt gaps.

- [ ] **Step 3: Fix `tick-plan` in `factory-ctl`**

Dedupe numbers and clean the temp file:

```python
    numbers = list(dict.fromkeys(re.findall(r"^- \[[ x]\] Task (\d+):", updated, flags=re.MULTILINE)))
```

```python
    if ticked:
        fd, tmp = tempfile.mkstemp(dir=base, suffix=".tmp")
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as handle:
                handle.write(updated)
            os.replace(tmp, plan_path)
        except BaseException:
            try:
                os.unlink(tmp)
            except FileNotFoundError:
                pass
            raise
```

Keep the existing ledger-first write order. If plan 03 wrapped `tick-plan` in the board lock, keep that wrapper.

- [ ] **Step 4: Make `seed-demo.sh` idempotent**

Replace the lines from `mkdir -p "$root"` to the `export FACTORY_EVENT_LOG` line with:

```bash
marker="$root/.seed-demo-root"
case "$root" in
  ""|"/") echo "seed-demo: refusing root '$root'" >&2; exit 1 ;;
esac
if [ -d "$root" ] && [ -n "$(ls -A "$root")" ]; then
  if [ -f "$marker" ]; then
    # Reset a folder this script made earlier.
    rm -rf "$root/.git" "$root/.superpowers" "$root/.claude"
  else
    echo "seed-demo: $root is not a seed-demo folder and is not empty" >&2
    exit 1
  fi
fi
mkdir -p "$root"
touch "$marker"
git init -q -b main "$root"
git -C "$root" -c user.email=t@t -c user.name=t commit -q --allow-empty -m init
export FACTORY_ROOT="$root"
export FACTORY_EVENT_LOG="$log"
```

Change the `sleep 1` line to:

```bash
# Events must be newer than every stage_since, or the server will not attach them to the jobs.
sleep 1
```

Change the final help text to use the same port variable: add `port="${2:-8765}"` after `root=` and print `--port $port` and `http://127.0.0.1:$port/`. Update the usage comment to `seed-demo.sh <dir> [port]`.

- [ ] **Step 5: Run the factory suite**

Run: `bash .claude/factory/tests/run.sh 2>&1 | tail -5`
Expected: `all passed`.

- [ ] **Step 6: Commit part A and B**

Invoke the skill `dev-workflow-skills:1-git-commit`, then:

```bash
git add .claude/factory/tests/run.sh .claude/factory/factory-ctl tools/factory-server/tests/seed-demo.sh
git commit --only -m "fix(factory): tighten lints, tick-plan safety and seed-demo reruns" -- .claude/factory/tests/run.sh .claude/factory/factory-ctl tools/factory-server/tests/seed-demo.sh
```

If you also changed `.claude/agents/factory-builder.md`, add it to both path lists.

Part C: dashboard render fixes (`tools/factory-server/`). Plan 05 changed `board.js` (series chip, durations, accessibility). Re-read the current file, then apply the same behaviours to what is there.

- [ ] **Step 7: Write the failing JS tests**

Append to `tools/factory-server/tests/board.test.js`:

```js
const HOSTILE = `<img src=x onerror=alert(1)>`;

test("clock shows dashes for a missing or invalid timestamp", () => {
  assert.equal(board.clock(undefined), "--:--:--");
  assert.equal(board.clock(null), "--:--:--");
  assert.equal(board.clock("soon"), "--:--:--");
});

test("render functions accept a missing state", () => {
  assert.doesNotThrow(() => board.renderBoard(null));
  assert.doesNotThrow(() => board.renderBoard(undefined));
  assert.doesNotThrow(() => board.renderUnassigned(null));
  assert.doesNotThrow(() => board.renderFeed(null));
  assert.doesNotThrow(() => board.renderSummary(null));
  assert.match(board.renderBoard(null), /Nothing here/);
});

test("renderBoard lists a job with an unknown stage in its own column", () => {
  const html = board.renderBoard(makeState([makeJob({ id: "007", stage: "limbo" })]));
  assert.match(html, /column-unknown/);
  assert.match(html, /data-job="007"/);
  assert.match(html, /1 job with an unknown stage/);
});

test("renderBoard has no unknown column when every stage is known", () => {
  assert.doesNotMatch(board.renderBoard(makeState([makeJob()])), /column-unknown/);
});

test("renderBoard breaks a stage_since tie by job id, newest first", () => {
  const jobs = [];
  for (let i = 1; i <= 11; i += 1) {
    jobs.push(makeJob({ id: String(i).padStart(3, "0"), stage: "merged", stage_since: 50 }));
  }
  const html = board.renderBoard(makeState(jobs));
  assert.match(html, /data-job="011"/);
  assert.doesNotMatch(html, /data-job="001"/);
});

test("time elements carry a machine readable datetime", () => {
  const card = board.renderCard(makeJob({ created: "2026-10-13" }));
  assert.match(card, /<time class="job-date" datetime="2026-10-13">/);
  const feed = board.renderFeed(makeState([], { recent: [{ ts: 150, kind: "tool", agent_type: "factory-builder" }] }));
  assert.match(feed, /<time datetime="1970-01-01T00:02:30\.000Z">/);
});

test("a feed event without a timestamp renders without a datetime", () => {
  const feed = board.renderFeed(makeState([], { recent: [{ kind: "tool", agent_type: "factory-builder" }] }));
  assert.doesNotMatch(feed, /datetime/);
  assert.match(feed, /--:--:--/);
});

test("renderCard escapes slug, branch, reason, builder and worktree", () => {
  const html = board.renderCard(
    makeJob({ slug: HOSTILE, branch: HOSTILE, worktree: HOSTILE, builder_id: HOSTILE, stage: "needs-human", reason: HOSTILE }),
  );
  assert.doesNotMatch(html, /<img/);
  assert.match(html, /&lt;img src=x onerror=alert\(1\)&gt;/);
});

test("agentLine escapes the type, tool, target and message", () => {
  const running = board.agentLine(
    { agent_type: HOSTILE, status: "running", last_tool: HOSTILE, last_target: HOSTILE },
    200,
  );
  assert.doesNotMatch(running, /<img/);
  const done = board.agentLine({ agent_type: "factory-builder", status: "done", last_message: HOSTILE }, 200);
  assert.doesNotMatch(done, /<img/);
});

test("renderFeed escapes the target and the kind", () => {
  const html = board.renderFeed(
    makeState([], { recent: [{ ts: 150, kind: HOSTILE, agent_type: HOSTILE, tool: "Read", target: HOSTILE }] }),
  );
  assert.doesNotMatch(html, /<img/);
});
```

- [ ] **Step 8: Run and see them fail**

Run: `node --test tools/factory-server/tests/*.test.js 2>&1 | grep -E '^not ok|^# (pass|fail)'`
Expected: failures for clock, missing state, unknown column, tie-break and datetime. The escaping tests likely pass already, since the code escapes everything. They stay as regression tests.

- [ ] **Step 9: Implement in `board.js`**

```js
  function clock(ts) {
    if (!Number.isFinite(ts)) return "--:--:--";
    return new Date(ts * 1000).toTimeString().slice(0, 8);
  }

  function isoTime(ts) {
    return Number.isFinite(ts) ? new Date(ts * 1000).toISOString() : "";
  }
```

In `renderCard`, replace the `<time class="job-date">` part with:

```js
      `<time class="job-date" datetime="${escapeHtml(job.created)}">${escapeHtml(job.created)}</time></header>` +
```

In `renderFeed`, start with `state = state || {};` and build the time tag as:

```js
      const iso = isoTime(event.ts);
      const stamp = iso ? `<time datetime="${iso}">` : `<time>`;
      return (
        `${"<li>"}${stamp}${clock(event.ts)}</time> ` +
```

(keep the rest of the row as it is). In `renderUnassigned`, `renderSummary` and `renderBoard`, add `state = state || {};` as the first line. In `renderBoard`, change the sort to:

```js
          .sort((a, b) => (b.stage_since || 0) - (a.stage_since || 0) || String(b.id).localeCompare(String(a.id)))
```

and build the unknown column after the `STAGES.map(...)` columns, before the warning:

```js
    const strays = jobs.filter((job) => !known.has(job.stage));
    const strayColumn = strays.length
      ? `<section class="column panel column-unknown"><h2>Unknown stage<span class="count">${strays.length}</span></h2>` +
        strays.map((job) => renderCard(job, state.generated_at)).join("") +
        `</section>`
      : "";
```

Return `columns.join("") + strayColumn + warning`. Add this rule to `dashboard.css` next to the other `.column-*` rules, using the same border variable the other columns use: `.column-unknown { border-color: var(--warn); }`. If `--warn` does not exist, use whichever variable `.warning` uses.

- [ ] **Step 10: Run the JS tests**

Run: `node --test tools/factory-server/tests/*.test.js 2>&1 | grep -E '^# (pass|fail)'`
Expected: `# fail 0`.

Part D: smoke test (`tools/factory-server/tests/smoke.sh`).

- [ ] **Step 11: Fix the smoke test**

Add a polling helper after `check()`:

```bash
# Usage: wait_for <seconds> <command...>. Polls every 0.1 s until the command succeeds.
wait_for() {
  local limit="$1" i
  shift
  for ((i = 0; i < limit * 10; i++)); do
    if "$@" >/dev/null 2>&1; then return 0; fi
    sleep 0.1
  done
  return 1
}
state_has() { curl -fs "http://127.0.0.1:$port/api/state" | jq -e "$1" >/dev/null; }
```

Change the cleanup trap so a failing run keeps its evidence:

```bash
cleanup() {
  if [ -n "$pid" ]; then kill "$pid" 2>/dev/null; fi
  if [ "${fails:-0}" -gt 0 ] && [ -f "$tmp/server.log" ]; then
    echo "---- server.log ----"
    cat "$tmp/server.log"
  fi
  rm -rf "$tmp"
}
```

Replace the start loop with a polled start that fails loudly:

```bash
if ! wait_for 10 curl -fs "http://127.0.0.1:$port/healthz"; then
  fail "the server starts"
  exit 1
fi
```

Delete the unused `api()` helper. Replace each `sleep` followed by a `check ... jq` with `wait_for 5 state_has '<same jq filter>'` inside `check`, for example:

```bash
check "the spec writer attaches to the job with its live tool" \
  wait_for 5 state_has '.jobs[0].agents[0] | select(.agent_type=="factory-spec-writer" and .last_tool=="Read" and .status=="running")'
```

Do the same for the stage change, the stopped agent (`.unassigned_agents[0] | select(.status=="done")`) and the new-job check. Keep the single `sleep 1` before the first event only if the stage-time rule needs it, and add the comment `# Events must be newer than the job's stage_since.`. Strengthen the garbage-line check:

```bash
echo "garbage line" >>"$FACTORY_EVENT_LOG"
printf '%s' "{\"hook_event_name\":\"SubagentStart\",\"agent_id\":\"s2\",\"agent_type\":\"factory-code-reviewer\",\"cwd\":\"$root\"}" | python3 "$logger"
check "a garbage line does not stop later events from being read" \
  wait_for 5 state_has '[.unassigned_agents[], (.jobs[].agents[])] | map(select(.agent_type=="factory-code-reviewer")) | length >= 1'
```

- [ ] **Step 12: Run the smoke test and the Rust format check**

Run: `bash tools/factory-server/tests/smoke.sh 2>&1 | tail -15`
Expected: every line PASS and `all passed`.
Run: `cargo fmt --check --manifest-path tools/factory-server/Cargo.toml && cargo check -j4 --manifest-path tools/factory-server/Cargo.toml`
Expected: no output from fmt, then a clean check.

- [ ] **Step 13: Commit part C and D**

Invoke the skill `dev-workflow-skills:1-git-commit`, then:

```bash
git add tools/factory-server/static/board.js tools/factory-server/static/dashboard.css tools/factory-server/tests/board.test.js tools/factory-server/tests/smoke.sh
git commit --only -m "fix(factory): harden dashboard rendering and the smoke test" -- tools/factory-server/static/board.js tools/factory-server/static/dashboard.css tools/factory-server/tests/board.test.js tools/factory-server/tests/smoke.sh
```

---

### Task 3: Browser check, doc sync, roadmap and the success-check recipe

**Files:**
- Create: `tools/factory-server/tests/screenshots.sh`
- Modify: `.claude/software-factory.md`, `.claude/roadmap.md`
- Modify only if the screenshots show a real defect: `tools/factory-server/static/dashboard.css`

**Interfaces:**
- Consumes: `seed-demo.sh <dir> [port]` from Task 2, the built `factory-server` binary, `/healthz`.
- Produces: PNG files under `.superpowers/factory/screenshots/` (not committed), a dated ruling and the success-check section.

On this machine `google-chrome` (Chrome 155) and `firefox` exist and a Playwright Chromium cache is in `~/.cache/ms-playwright`, but `playwright` itself has no CLI. The script uses the browser binary directly, so it needs no npm install.

- [ ] **Step 1: Write the script**

Create `tools/factory-server/tests/screenshots.sh`:

```bash
#!/usr/bin/env bash
# Seeds the demo board, serves it on a scratch port and screenshots the page with a headless browser.
# Usage: screenshots.sh [out-dir]. Exit 0 on success, 3 when no headless browser is installed.
set -u

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tool="$(cd "$here/.." && pwd)"
out="${1:-$(cd "$tool/../.." && pwd)/.superpowers/factory/screenshots}"
mkdir -p "$out"

browser=""
for name in google-chrome chromium chromium-browser firefox; do
  if command -v "$name" >/dev/null 2>&1; then browser="$name"; break; fi
done
if [ -z "$browser" ]; then
  echo "BLOCKED: no headless browser found (tried google-chrome, chromium, chromium-browser, firefox)." >&2
  exit 3
fi

tmp="$(mktemp -d "${TMPDIR:-/tmp}/factory-shots.XXXXXX")"
pid=""
cleanup() {
  if [ -n "$pid" ]; then kill "$pid" 2>/dev/null; fi
  if [ -f "$tmp/server.log" ] && [ "${failed:-0}" -ne 0 ]; then cat "$tmp/server.log"; fi
  rm -rf "$tmp"
}
trap cleanup EXIT
failed=0

port="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
bash "$here/seed-demo.sh" "$tmp/root" "$port" >/dev/null || { failed=1; echo "FAIL: seed" >&2; exit 1; }
cargo build -j4 --manifest-path "$tool/Cargo.toml" -q || { failed=1; echo "FAIL: build" >&2; exit 1; }
"$tool/target/debug/factory-server" --root "$tmp/root" --port "$port" >"$tmp/server.log" 2>&1 &
pid=$!
for _ in $(seq 1 100); do
  if curl -fs "http://127.0.0.1:$port/healthz" >/dev/null 2>&1; then break; fi
  sleep 0.1
done
curl -fs "http://127.0.0.1:$port/healthz" >/dev/null 2>&1 || { failed=1; echo "FAIL: server did not start" >&2; exit 1; }

url="http://127.0.0.1:$port/"
# Usage: shot <file> <width> <height> [extra chrome flags...]
shot() {
  local file="$1" width="$2" height="$3"
  shift 3
  if [ "$browser" = firefox ]; then
    firefox --headless --screenshot "$file" --window-size "$width,$height" "$url" >/dev/null 2>&1
  else
    "$browser" --headless=new --no-sandbox --disable-gpu --hide-scrollbars "$@" \
      --virtual-time-budget=4000 --window-size="$width,$height" --screenshot="$file" "$url" >/dev/null 2>&1
  fi
  if [ -s "$file" ]; then echo "wrote $file"; else echo "FAIL: no screenshot $file" >&2; failed=1; fi
}

shot "$out/desktop-dark.png" 1440 1500
shot "$out/phone-dark.png" 400 2600
shot "$out/desktop-light.png" 1440 1500 --blink-settings=preferredColorScheme=1
if [ -s "$out/desktop-dark.png" ] && [ -s "$out/desktop-light.png" ] && cmp -s "$out/desktop-dark.png" "$out/desktop-light.png"; then
  echo "NOTE: the light screenshot equals the dark one, so the light theme was not exercised."
fi
exit "$failed"
```

- [ ] **Step 2: Run it**

Run: `chmod +x tools/factory-server/tests/screenshots.sh && bash tools/factory-server/tests/screenshots.sh; echo "exit $?"`
Expected: three `wrote ...png` lines and `exit 0`. If it prints `BLOCKED` and `exit 3`, do not invent screenshots: skip to Step 5 and write the BLOCKED text there.

- [ ] **Step 3: Look at the screenshots**

Use the Read tool on each PNG under `.superpowers/factory/screenshots/` (it shows images). Check and write the findings in the handoff reply, one line each:

- Six stage columns plus the summary strip at the top are visible on the desktop shot. Seed data shows jobs in spec, build, review, needs-human and merged.
- Needs-human card shows its reason and the two copy commands. Review chips show text, not color only.
- The phone shot at 400 px: no horizontal page scroll, the board scrolls inside its own container, text is not clipped, tap targets are not tiny.
- Muted text and the dimmed stale board are readable (contrast looks at least as strong as normal body text on the same background).
- The light shot differs from the dark one and is readable, or the NOTE line says it could not be exercised.

- [ ] **Step 4: Fix only real defects**

For each clear defect (overflow, clipped text, unreadable contrast), make the smallest change in `tools/factory-server/static/dashboard.css`, rerun Step 2 and look again. Remove duplicated `.dot` rules only if you touch that area. A defect that needs a redesign is not fixed here: list it in the handoff as a follow-up and in the open items list in Step 6. Run `node --test tools/factory-server/tests/*.test.js` and `bash tools/factory-server/tests/smoke.sh` after any CSS change.

- [ ] **Step 5: Write the design doc changes in `.claude/software-factory.md`**

Re-read the file as it is after plans 01 to 05. Then:

1. Replace the whole `## Open items` list with only items that are still true after plans 01 to 05 and this plan. Remove these, which are now settled: the builder id match (confirmed in job 001), "The dashboard look has not been checked in a browser" and "`/verify-rocket` conflicts with the harness guard". Keep any item that a plan 01 to 05 section of this same file still lists as open. Add these lines:

```markdown
- Success check (dry run of a two-plan feature through `/factory`) has not been run yet. The user runs it, see "Success check" below.
- Light theme in a headless browser: only checked if the screenshot differs from the dark one (see the 2026-10-13 ruling).
```

If Step 4 left any follow-up, add one line per follow-up.

2. Add a section `## Known and accepted v1 minors` with the "dropped" rows of the triage table in this plan (subcommand dispatch on the first word, `needs-human` without `--reason`, silent short writes, python spawn per tool call, spec-writer `Write` scope, lossy utf8, server test hygiene), one bullet each with its reason.

3. Add the dated ruling at the end of the rulings or decisions part of the file (create a `## Rulings` section if none exists). Use exactly one of these two forms, depending on the Step 2 result.

If screenshots were taken:

```markdown
- Ruling (2026-10-13): the dashboard was checked in a headless `<browser name and version>` with `tools/factory-server/tests/screenshots.sh` at 1440 px and 400 px, dark theme and an attempted light theme. Findings: <the findings from Step 3, one sentence>. A human look in a real window is still welcome, but no longer blocks anything.
```

If no browser was found:

```markdown
- Ruling (2026-10-13): BLOCKED. No headless browser was installed on the builder machine, so the dashboard look is unchecked. Manual steps: run `bash tools/factory-server/tests/seed-demo.sh /tmp/factory-demo 8765`, start the printed server command, open http://127.0.0.1:8765/ and check dark, light and a 400 px wide window against the checklist in `tools/factory-server/tests/screenshots.sh`'s plan (six columns, summary strip, readable chips, no sideways page scroll).
```

4. Add a section `## Success check (dry run, run by the user)`:

````markdown
## Success check (dry run, run by the user)

Run this once after plans 01 to 06 are merged. It uses a real feature, so it costs real agent time. Use a small feature that needs two plans, for example: "Add a per-request timeout setting and show it in the request tab".

1. In a fresh Claude Code session on `main` (clean tree), start the dashboard:
   `cargo run -j4 --manifest-path tools/factory-server/Cargo.toml -- --root "$(pwd)" --port 8765`
   and open http://127.0.0.1:8765/.
2. Run `/factory Add a per-request timeout setting and show it in the request tab`. Ask the spec writer for a two-plan series if it proposes one plan.
3. Watch the dashboard and do not step in. Expect, in order:
   - The spec stage ends with a spec and plan files that end in `Next plan:` lines.
   - The build runs in a worktree. The card shows plan 1 of 2 and its worktree and builder id.
   - A `checks.md` file appears in the job folder (`ls .superpowers/sdd/*-001-*/checks.md`).
   - Four reviews run (UX and UI only when `src/` changed). The approver says `DECISION: APPROVE` with no escalation for a missing check.
   - The merge commit contains the reviewed head sha (`git log --merges -1 --format=%P` shows it as the second parent), and its message came from the commit skill.
   - Plan 2 starts on its own as job 002. The card shows plan 2 of 2.
   - Per-stage times on the cards look right: merged cards do not keep counting.
4. Pass means: no manual intervention, no `needs-human` card, both plans merged, `git status` clean in the main checkout, and `ctl status` showing both jobs merged.
5. Clean up with `/factory cleanup <id>` for each job and `git branch --list 'factory/*'` to confirm nothing is left.

If something needs a manual step, note which step and open a follow-up row in `.claude/roadmap.md`.
````

- [ ] **Step 6: Roadmap entry**

Read the last `R-` id in `.claude/roadmap.md` (R-59 at the time of writing) and use the next free number, called `R-NN` below. Add this row to the quality and hygiene table in section 8 (the table that holds R-57 to R-59), after the last row:

```markdown
| R-NN | Software factory v2 series (plans 01 to 06): orchestrator robustness, evidence and prompts, main-moved handling, multi-plan series, dashboard and hook fixes, harness and cleanup | built on branch `worktree-software-factory`, not merged; success-check dry run pending (user) | spec `docs/superpowers/specs/2026-10-13-software-factory-v2-design.md`, plans `docs/superpowers/plans/2026-10-13-software-factory-v2/`, design doc `.claude/software-factory.md` | Run the success check in `.claude/software-factory.md`, then merge. Open follow-ups are in its open items list. |
```

Set the status text to what is true when you finish (for example "merged to local main, not pushed" if the orchestrator already merged). Add one line to the Decisions log at the end of the file:

```markdown
- Ruling (2026-10-13): the factory v2 series closes with plan 06. `/verify-rocket` now checks changed crates with `-j4` and no workspace command, and a harness test lints the cargo commands in its doc and in `CLAUDE.md`.
```

- [ ] **Step 7: Verify everything**

Run, one per line:
- `bash .claude/harness/tests/run.sh 2>&1 | tail -3` expected `ALL PASSED`
- `bash .claude/factory/tests/run.sh 2>&1 | tail -3` expected `all passed`
- `node --test tools/factory-server/tests/*.test.js 2>&1 | grep -E '^# (pass|fail)'` expected `# fail 0`
- `bash tools/factory-server/tests/smoke.sh 2>&1 | tail -3` expected `all passed`
- `git status --short` expected: only the files this plan names, plus untracked screenshots under `.superpowers/` which must not be added.

- [ ] **Step 8: Commit**

Invoke the skill `dev-workflow-skills:1-git-commit`, then:

```bash
git add tools/factory-server/tests/screenshots.sh .claude/software-factory.md .claude/roadmap.md
git commit --only -m "docs(factory): record the browser check, sync docs and add the success check" -- tools/factory-server/tests/screenshots.sh .claude/software-factory.md .claude/roadmap.md
```

Add `tools/factory-server/static/dashboard.css` to both path lists if Step 4 changed it.

- [ ] **Step 9: Handoff**

The reply must contain: the result of each Step 7 command, the screenshot findings (or `BLOCKED` with the manual steps), any prompt gap the new lints found (for example a missing builder `maxTurns`), the commit shas, and a one-line reminder that the success-check dry run in `.claude/software-factory.md` is for the user to run.

---

## Self-review notes

- Spec coverage: E10 is Task 3 steps 1 to 6; E11 is Task 1; E12 is the triage table plus Task 2; the success check is Task 3 step 5 part 4.
- Names used across tasks: `check_fails`, `column-unknown`, `seed-demo.sh <dir> [port]`, `screenshots.sh [out-dir]` are defined where first used and used the same way later.
- Task count is 3, every command uses `-j4` and none uses `--workspace`.

Next plan: none (series complete)
