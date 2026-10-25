# Software Factory v2 Plan 07: Lean Mode

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:executing-plans, executed by a `factory-builder` subagent. Skip that skill's final whole-branch review and its ledger scripts, because the factory's reviewers and `ctl` do those. Steps use checkbox (`- [ ]`) syntax for tracking.

> **Plans 01 to 06 of this series must be finished and merged first.** This plan edits files those plans already changed (`factory-ctl`, `tests/run.sh`, `.claude/commands/factory.md`, the agent prompts and `.claude/software-factory.md`). Later plans moved wording and line numbers, so every edit below is found by its anchor text, never by a line number. Re-read each file before you edit it. If an anchor was reworded by an earlier plan, apply the edit by its meaning and keep the earlier plan's other additions.

**Goal:** Make a LEAN mode the default of `/factory`: the orchestrator runs `superpowers:brainstorming` and `superpowers:writing-plans` itself, one builder runs each plan, one combined code reviewer checks the diff, and only a risky diff gets the v1 review set and the approver. `/factory --full <feature>` keeps the v1 pipeline.

**Architecture:** `factory-ctl` gains the board field `mode`, `new-job --mode`, a `config.json` with `risk_paths`, and three subcommands: `classify-diff` (prints `doc-only`, `normal` or `risky`), `route` (prints the reviewer roles to run) and `can-auto-merge` (exit 0 when no approver is needed). The `/factory` command gets the `--full` flag, a lean spec and plan flow run in the main session, role routing through `ctl route`, and a lean gate in the approve stage. The code reviewer prompt gains the hard-rules checklist. Plan 02's `checks.md` stays the evidence that `can-auto-merge` reads, and plan 03's merge step is unchanged.

**Tech Stack:** Python 3 stdlib (`factory-ctl`), bash plus `jq` tests, Claude Code agent and command markdown, a JSON config file.

**Spec:** [docs/superpowers/specs/2026-10-13-software-factory-v2-design.md](../../specs/2026-10-13-software-factory-v2-design.md), decision 12, decision 9 (the spec-writer note), gaps F1, F4, F5 and F7, and the plan 07 row of "Interfaces between plans". Design doc: [.claude/software-factory.md](../../../../.claude/software-factory.md).

## Context: why lean

- Job 001 used about 12.75M raw tokens (11.76M of them cache reads, about 2.17M weighted) across 9 agent runs plus 11 helper agents. The orchestrator session was 53.6 percent of it (6.83M).
- The user asked to simply use the superpowers skills `superpowers:brainstorming` and `superpowers:writing-plans`. A plain superpowers flow does the same core work with far fewer agents.
- Lean mode drops the spec-writer agent, the security, UX and UI reviewers and the approver for ordinary diffs. It keeps one builder per plan, one combined Sonnet code reviewer, the recorded checks, and the merge step. A risky diff still gets the v1 review set and the approver.
- Lean mode does not fix the orchestrator's own context cost. Plan 08 does that (diff hygiene, commit ceremony, budget).

## Global Constraints

- One plan at a time, one builder at a time. No parallel builders in one worktree.
- Ledger and board changes go through `ctl` only, in lean mode as well (decision 8). Builders and reviewers never write the ledger.
- Docs stay under `.claude/`. `.claude/software-factory.md` is updated by this plan, `CLAUDE.md` stays small.
- Interface names produced by this plan, exactly: board field `mode` (`lean` default, `full`); `ctl new-job --mode lean|full`; `.claude/factory/config.json` with the key `risk_paths`; `ctl classify-diff --names-file <path>` prints `doc-only`, `normal` or `risky`; `ctl route <id> --names-file <path> [--escalate]` prints the roles to run on one line; `ctl can-auto-merge <id>` exits 0 only when `checks.md` has no `FAIL`, the latest code verdict is `APPROVE`, the diff class is not `risky` and the budget is not `stop`.
- Plan 02's `checks.md` format (`PASS ...` and `FAIL ...` lines) and plan 03's merge step are read, never changed.
- The risky list is the list in decision 12. It lives only in `.claude/factory/config.json`, key `risk_paths`.
- Test fixtures that need git use temp repos under `$tmp` (the `mktemp -d` directory in `run.sh`) only. Every ctl call in a fixture sets `FACTORY_ROOT` to the temp repo. Never run a fixture against the real repo.
- All `cargo` commands pass `-j4`. Never `cargo test --workspace` or `--all`. This plan needs no cargo run.
- Commits use a pathspec: `git add <paths> && git commit --only -m "..." -- <paths>`. Invoke the skill `dev-workflow-skills:1-git-commit` for the message, but run its git commands inline with no helper agents and no amend step (decision 11e). Never `git add -A`, `--all` or `.`. Never set `HARNESS_GATE=0`. Never push.
- Code comments are short full sentences that end with a period. No emojis.
- The command lint in `tests/run.sh` checks that every `ctl <word>` in `.claude/commands/factory.md` is a real subcommand. Never write ordinary prose such as "through ctl only" in that file. Write `ctl` followed by a lowercase word only when the word is a subcommand.
- Follow the existing style: `factory-ctl` has one `cmd_<name>(args)` per subcommand and `die()` for errors. Tests use `check "<description>" <command...>`, `pass`, `fail` and `prompt_has` in `.claude/factory/tests/run.sh`.
- This plan touches no collection, environment or auth code, so the OpenCollection read line does not apply.

## Review Focus

- A feature text such as "full screen mode", "make the --full flag work" or "next button for pagination" must stay an ordinary lean feature. Only a first word that is exactly `--full`, followed by more text, selects full mode. Pinned by the exact-shape rule text and its lint in Task 2.
- A risky diff must never auto-merge in lean mode, even when the last fix round touched only docs. `can-auto-merge` judges the whole job diff, and `route` unions the round diff with it. Tested in Task 1.
- A job on the board with no `mode` field (every job created before this plan) must keep the full pipeline, never silently turn lean. Tested in Task 1.
- A missing, garbage or empty `config.json` must fail closed: everything counts as risky, so no job auto-merges by accident. Tested in Task 1.
- A reviewer verdict that is stale or padded must not pass: the newest `review-code-r<n>.md` by modification time decides, and only an exact last line `VERDICT: APPROVE` counts. A security, UX or UI reviewer whose last verdict was CHANGES must not be skipped by routing, because skipping clears the CHANGES. Tested in Task 1.

## File Structure

| File | Task | Change |
|---|---|---|
| `.claude/factory/factory-ctl` | 1 | `mode`, `--mode`, config loader, `classify-diff`, `route`, `can-auto-merge`. |
| `.claude/factory/config.json` | 1 | New. The `risk_paths` list. |
| `.claude/factory/tests/run.sh` | 1, 2, 3 | Lean fixtures, command lint, prompt lint, doc lint. |
| `.claude/commands/factory.md` | 2 | `--full` flag, lean flow, routing, lean gate. |
| `.claude/agents/factory-code-reviewer.md` | 2 | Hard-rules checklist for lean mode. |
| `.claude/agents/factory-spec-writer.md` | 2 | Marked full mode only. |
| `.claude/software-factory.md` | 3 | Pipeline, Agents, modes, token note, dry run. |
| `docs/roadmaps/roadmap.md` | 3 | One note on the factory v2 row. |

---

### Task 1: ctl lean mode support

**Files:**
- Modify: `.claude/factory/factory-ctl`
- Create: `.claude/factory/config.json`
- Modify: `.claude/factory/tests/run.sh` (insert before `# ---- /factory command ----`)

**Interfaces:**
- Consumes: `load_board`, `find_job`, `job_dir`, `git`, `die`, `repo_root`, `cmd_new_job` and `build_parser` in `factory-ctl`. From plan 04, the variable `series_plan` (the result of `plan_series_job`, a tuple `(origin, chain, index)` or `None`) inside `cmd_new_job`. From plan 02, `checks.md` lines `PASS ...` and `FAIL ...`. From plan 01, the review file names `review-<role>-r<n>.md`.
- Produces: `DEFAULT_MODE`, `MODES`, `job_mode(job) -> str`, `load_config() -> dict`, `is_doc_path(path) -> bool`, `classify_paths(paths) -> str`, `newest_verdict(root, job, role) -> str | None`, `budget_state(root, job) -> str`, `cmd_classify_diff`, `cmd_route`, `cmd_can_auto_merge`. CLI as in Global Constraints. Environment variable `FACTORY_CONFIG` overrides the config path (tests only). Environment variable `FACTORY_BUDGET_STATE` (`ok`, `warn`, `stop`) is a test seam that plan 08 replaces with the real token budget.

- [ ] **Step 1: Write the failing ctl tests**

In `.claude/factory/tests/run.sh`, insert this block directly before the line `# ---- /factory command ----`:

```bash
# ---- lean mode: mode field, classify-diff, route and can-auto-merge ----
lm="$tmp/lm"
git init -q -b main "$lm"
glm() { git -C "$lm" -c user.email=t@t -c user.name=t "$@"; }
glm commit -q --allow-empty -m init
lc() { FACTORY_ROOT="$lm" python3 "$ctl" "$@"; }
lboard="$lm/.superpowers/factory/board.json"
lbase="$(glm rev-parse HEAD)"
lnames="$tmp/lnames.txt"

# Usage: lean_job <lean|full> <path>... . Makes a job whose head commit adds the paths and sets lean_id.
lean_job() {
  local mode="$1" p
  shift
  lc new-job "fixture" --mode "$mode" --date 2026-10-13 >/dev/null
  lean_id="$(jq '.jobs | length' "$lboard")"
  glm switch -q -c "lean$lean_id" "$lbase"
  for p in "$@"; do
    mkdir -p "$lm/$(dirname "$p")"
    echo "$lean_id" >"$lm/$p"
    glm add -- "$p"
  done
  glm commit -q -m "work $lean_id"
  lc set-stage "$lean_id" review --base "$lbase" --head "$(glm rev-parse HEAD)" >/dev/null
  glm switch -q main
}
ldir() { jq -r ".jobs[] | select(.id == \"$(printf '%03d' "$1")\") | .dir" "$lboard"; }
# Usage: lean_checks <id> <result lines...>. Writes checks.md the way plan 02's ctl does.
lean_checks() {
  local id="$1"
  shift
  { printf '# Checks\n\n'; printf '%s\n' "$@"; } >"$lm/.superpowers/sdd/$(ldir "$id")/checks.md"
}
# Usage: lean_verdict <id> <role> <round> <last line>. Writes a reviewer report.
lean_verdict() {
  printf '1. [minor] nit at a.rs:1\n\n%s\n' "$4" >"$lm/.superpowers/sdd/$(ldir "$1")/review-$2-r$3.md"
}

lc new-job "default mode job" --date 2026-10-13 >/dev/null
check "ctl: new-job without --mode makes a lean job" jq -e '.jobs[0].mode == "lean"' "$lboard"
check "ctl: new-job rejects an unknown mode" bash -c "! FACTORY_ROOT='$lm' python3 '$ctl' new-job x --mode turbo"
lean_job full crates/rocket-http/src/lib.rs
check "ctl: new-job --mode full is stored" jq -e '.jobs[-1].mode == "full"' "$lboard"

# classify-diff, with the shipped config.
classify() { python3 "$ctl" classify-diff --names-file "$lnames"; }
# Usage: cls_case <expected> <path>...
cls_case() {
  local want="$1"
  shift
  printf '%s\n' "$@" >"$lnames"
  check "ctl: classify-diff says $want for $*" test "$(classify)" = "$want"
}
cls_case doc-only docs/superpowers/plans/x.md .claude/commands/factory.md README.md
cls_case normal crates/rocket-http/src/lib.rs
cls_case normal docs/a.md src/components/Button.tsx
cls_case risky crates/rocket-collection/src/trust.rs
cls_case risky src/lib/secret-providers.ts
cls_case risky src/lib/inherited-auth.ts
cls_case risky crates/rocket-infra/src/fs_collection_repo.rs
cls_case risky crates/rocket-collection/src/model.rs
cls_case risky src-tauri/src/commands/collections.rs
cls_case risky crates/rocket-shared/src/event.rs
cls_case risky Cargo.lock
cls_case risky crates/rocket-http/Cargo.toml
cls_case risky package.json
cls_case risky src-tauri/tauri.conf.json
cls_case risky collections/demo/request.yml
cls_case risky .claude/harness/hooks/bash-guard
cls_case risky .claude/factory/hooks/log-event
cls_case risky .claude/settings.json
cls_case risky docs/a.md crates/rocket-infra/src/lib.rs
: >"$lnames"
check "ctl: classify-diff says normal for an empty names file" test "$(classify)" = "normal"
check "ctl: classify-diff rejects a missing names file" bash -c "! python3 '$ctl' classify-diff --names-file '$tmp/nope.txt'"

lcfg="$tmp/lcfg.json"
printf '%s\n' '{"risk_paths": ["only/*"]}' >"$lcfg"
printf '%s\n' only/x.rs >"$lnames"
check "config: a custom risk_paths list is used" test "$(FACTORY_CONFIG="$lcfg" classify)" = "risky"
printf '%s\n' Cargo.toml >"$lnames"
check "config: a custom risk_paths list replaces the shipped one" test "$(FACTORY_CONFIG="$lcfg" classify)" = "normal"
printf '%s\n' docs/a.md >"$lnames"
check "config: a missing config file fails closed to risky" test "$(FACTORY_CONFIG="$tmp/none.json" classify)" = "risky"
printf 'not json' >"$lcfg"
check "config: an invalid config file fails closed to risky" test "$(FACTORY_CONFIG="$lcfg" classify)" = "risky"
printf '%s\n' '{"risk_paths": []}' >"$lcfg"
check "config: an empty risk_paths list fails closed to risky" test "$(FACTORY_CONFIG="$lcfg" classify)" = "risky"
check "config: the shipped risk_paths is a non-empty list of strings" \
  jq -e '.risk_paths | (type == "array" and length > 0 and all(type == "string" and length > 0))' "$factory/config.json"

# route
# Usage: route_case <description> <expected> <job id> <round names...>
route_case() {
  local desc="$1" want="$2" id="$3"
  shift 3
  printf '%s\n' "$@" >"$lnames"
  check "ctl: route $desc" test "$(lc route "$id" --names-file "$lnames")" = "$want"
}
lean_job lean crates/rocket-http/src/lib.rs
route_case "runs only the code reviewer for a lean normal diff" code "$lean_id" crates/rocket-http/src/lib.rs
lean_job lean src/components/Button.tsx
route_case "runs only the code reviewer for a lean frontend diff" code "$lean_id" src/components/Button.tsx
lean_job lean src-tauri/src/commands/x.rs
route_case "adds security for a lean risky backend diff" "security code" "$lean_id" src-tauri/src/commands/x.rs
lean_job lean src/lib/secret-providers.ts
route_case "runs the v1 set for a lean risky frontend diff" "security ux ui code" "$lean_id" src/lib/secret-providers.ts
lean_job full src/components/Button.tsx
route_case "runs the v1 set for a full frontend diff" "security ux ui code" "$lean_id" src/components/Button.tsx
lean_job full crates/rocket-http/src/lib.rs
route_case "skips ux and ui for a full backend diff" "security code" "$lean_id" crates/rocket-http/src/lib.rs
lean_job full docs/a.md
route_case "skips security, ux and ui for a full doc-only diff" code "$lean_id" docs/a.md
lean_verdict "$lean_id" security 1 "VERDICT: CHANGES"
route_case "never skips security over an open CHANGES" "security code" "$lean_id" docs/a.md
lean_job full docs/a.md
lean_verdict "$lean_id" ux 1 "VERDICT: CHANGES"
route_case "never skips ux over an open CHANGES" "ux code" "$lean_id" docs/a.md
lean_job full src/a.tsx
route_case "runs ux and ui on the first look when the job touches src" "ux ui code" "$lean_id" docs/fix.md
lean_job lean src/components/Button.tsx
printf '%s\n' src/components/Button.tsx >"$lnames"
check "ctl: route --escalate runs the v1 set for a lean normal diff" \
  test "$(lc route "$lean_id" --names-file "$lnames" --escalate)" = "security ux ui code"
lean_job lean src-tauri/a.rs src/b.tsx
route_case "judges risk on the whole job diff, not only the round diff" "ux ui code" "$lean_id" docs/fix.md
lean_job lean src/components/Button.tsx
jq 'del(.jobs[-1].mode)' "$lboard" >"$lboard.new" && mv "$lboard.new" "$lboard"
route_case "treats a job with no mode field as full" "security ux ui code" "$lean_id" src/components/Button.tsx
check "ctl: route rejects an unknown job" bash -c "! FACTORY_ROOT='$lm' python3 '$ctl' route 99 --names-file '$lnames'"

# can-auto-merge
amm() { lc can-auto-merge "$1"; }
# Usage: amm_blocked <description> <id> <text the blocked line must contain>
amm_blocked() {
  local out rc
  out="$(amm "$2")"; rc=$?
  check "ctl: can-auto-merge blocks $1" bash -c "test '$rc' -eq 1 && printf '%s' '$out' | grep -qF '$3'"
}
lean_job lean crates/rocket-http/src/lib.rs
amm_id="$lean_id"
amm_blocked "a job with no checks and no review" "$amm_id" "no checks.md recorded"
lean_checks "$amm_id" "PASS cargo check -j4" "PASS cargo test -j4 -p rocket-http lib"
amm_blocked "a job with no code review" "$amm_id" "latest code review is not APPROVE"
lean_verdict "$amm_id" code 1 "VERDICT: CHANGES"
amm_blocked "a code review with CHANGES" "$amm_id" "latest code review is not APPROVE"
lean_verdict "$amm_id" code 1 "VERDICT: APPROVE"
check "ctl: can-auto-merge passes a clean lean normal job" test "$(amm "$amm_id")" = "ok"
check "ctl: can-auto-merge exits 0 for a clean lean normal job" amm "$amm_id"
check "ctl: can-auto-merge still passes at budget warn" env FACTORY_BUDGET_STATE=warn bash -c "FACTORY_ROOT='$lm' python3 '$ctl' can-auto-merge $amm_id"
out="$(FACTORY_BUDGET_STATE=stop amm "$amm_id")"; rc=$?
check "ctl: can-auto-merge blocks a stopped budget" bash -c "test '$rc' -eq 1 && printf '%s' '$out' | grep -qF 'budget is stopped'"
lean_checks "$amm_id" "PASS cargo check -j4" "FAIL yarn tsc --noEmit (2 errors)"
amm_blocked "a failing check" "$amm_id" "a check failed"
lean_checks "$amm_id" "# nothing useful"
amm_blocked "a checks file with no PASS line" "$amm_id" "no passing check recorded"
lean_checks "$amm_id" "PASS cargo check -j4"
printf '1. fine\n\nVERDICT: APPROVE\nthanks for the change\n' >"$lm/.superpowers/sdd/$(ldir "$amm_id")/review-code-r1.md"
amm_blocked "a verdict line that is not the last line" "$amm_id" "latest code review is not APPROVE"
review_dir="$lm/.superpowers/sdd/$(ldir "$amm_id")"
lean_verdict "$amm_id" code 1 "VERDICT: CHANGES"
lean_verdict "$amm_id" code 2 "VERDICT: APPROVE"
touch -d '2026-10-13 10:00:00' "$review_dir/review-code-r1.md"
touch -d '2026-10-13 11:00:00' "$review_dir/review-code-r2.md"
check "ctl: can-auto-merge uses the newest code review (APPROVE after CHANGES)" amm "$amm_id"
touch -d '2026-10-13 12:00:00' "$review_dir/review-code-r1.md"
amm_blocked "an older APPROVE under a newer CHANGES" "$amm_id" "latest code review is not APPROVE"

lean_job lean crates/rocket-infra/src/fs_x.rs
lean_checks "$lean_id" "PASS cargo check -j4"
lean_verdict "$lean_id" code 1 "VERDICT: APPROVE"
amm_blocked "a risky diff" "$lean_id" "diff class is risky"
lean_job lean docs/x.md
lean_checks "$lean_id" "PASS yarn check"
lean_verdict "$lean_id" code 1 "VERDICT: APPROVE"
check "ctl: can-auto-merge passes a doc-only lean job" amm "$lean_id"
lean_job full crates/rocket-http/src/lib.rs
lean_checks "$lean_id" "PASS cargo check -j4"
lean_verdict "$lean_id" code 1 "VERDICT: APPROVE"
amm_blocked "a full mode job" "$lean_id" "full mode job"
lc new-job "headless" --date 2026-10-13 >/dev/null
amm_blocked "a job with no recorded head" "$(jq '.jobs | length' "$lboard")" "no recorded base or head"

# A series keeps the mode of its origin job.
lc new-job "series origin" --mode full --date 2026-10-13 >/dev/null
so="$(jq '.jobs | length' "$lboard")"
sodir="$(ldir "$so")"
mkdir -p "$lm/.superpowers/sdd/$sodir/plans"
printf '%s\n' '# Plan 1' '- [ ] Task 1: first' "Next plan: .superpowers/sdd/$sodir/plans/plan-02-b.md" >"$lm/.superpowers/sdd/$sodir/plans/plan-01-a.md"
printf '%s\n' '# Plan 2' '- [ ] Task 1: second' 'Next plan: none' >"$lm/.superpowers/sdd/$sodir/plans/plan-02-b.md"
lc series-start "$so" >/dev/null
lc set-stage "$so" build >/dev/null
lc set-stage "$so" merged >/dev/null
lc new-job "series second" --series "$so" --plan-file ".superpowers/sdd/$sodir/plans/plan-02-b.md" --date 2026-10-13 >/dev/null
check "ctl: a later plan of a series inherits the origin mode" jq -e '.jobs[-1].mode == "full"' "$lboard"
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bash .claude/factory/tests/run.sh | grep -E '^FAIL' | head -40`
Expected: `FAIL:` lines for the new-job mode checks, every `classify-diff`, `config:`, `route` and `can-auto-merge` check, and a final `N failed`. The fixture helpers may print argparse errors, which is fine at this point.

- [ ] **Step 3: Create the config file**

Create `.claude/factory/config.json` with exactly:

```json
{
  "risk_paths": [
    "*trust*",
    "*auth*",
    "*secret*",
    "*process_env*",
    "*process-env*",
    "*.yml",
    "*.yaml",
    "crates/rocket-collection/*",
    "crates/rocket-environment/*",
    "crates/rocket-workspace/*",
    "crates/rocket-infra/*",
    "src-tauri/*",
    "crates/rocket-shared/*",
    "*Cargo.toml",
    "*Cargo.lock",
    "*package.json",
    "*tauri.conf.json",
    "*csp*",
    ".claude/harness/*",
    ".claude/hooks/*",
    ".claude/factory/hooks/*",
    ".claude/settings.json",
    ".claude/settings.local.json"
  ]
}
```

These globs are matched against the changed path, lower case, with `*` matching across `/`. They map to decision 12 as follows: trust gate, auth, secrets and `{{process.env}}` by name; persistence formats by `.yml` and the domain and infra crates that hold the serde persistence structs; the IPC surface by `src-tauri/`; `DomainEvent` by `rocket-shared`; the manifests, CSP and harness hooks by file. A name match over-flags (for example a file called `author.ts`), which is safe: it only sends the diff to the v1 review set.

- [ ] **Step 4: Implement the ctl changes**

Edit `.claude/factory/factory-ctl`.

4a. Add `import fnmatch` on its own line directly after `import argparse`.

4b. Directly after the line `REVIEW_STATES = (...)` (and after any constants that earlier plans put there) add:

```python
MODES = ("lean", "full")
DEFAULT_MODE = "lean"
# The shipped config.json holds the real list. This default fails closed: everything is risky.
CONFIG_DEFAULTS = {"risk_paths": ["*"]}
# A path with one of these file names is never treated as a doc, wherever it lives.
CODE_FILE_NAMES = (
    "Cargo.toml",
    "package.json",
    "tauri.conf.json",
    "settings.json",
    "settings.local.json",
)
DOC_DIRS = ("docs", ".claude")
APPROVE_LINE = "VERDICT: APPROVE"
CHANGES_LINE = "VERDICT: CHANGES"
```

4c. Directly before `def build_parser():` add:

```python
def job_mode(job):
    """Return the job mode. A job from before lean mode has no field and keeps the full pipeline."""
    return job.get("mode") or "full"


def config_path():
    override = os.environ.get("FACTORY_CONFIG")
    if override:
        return override
    return os.path.join(os.path.dirname(os.path.abspath(__file__)), "config.json")


def load_config():
    """Return the factory settings. A missing file, bad JSON or a bad value gives the default."""
    config = {
        key: list(value) if isinstance(value, list) else value
        for key, value in CONFIG_DEFAULTS.items()
    }
    try:
        with open(config_path(), encoding="utf-8") as handle:
            data = json.load(handle)
    except (OSError, ValueError):
        return config
    if not isinstance(data, dict):
        return config
    for key, default in CONFIG_DEFAULTS.items():
        value = data.get(key)
        if isinstance(default, list):
            if isinstance(value, list) and value and all(isinstance(item, str) and item for item in value):
                config[key] = value
        elif isinstance(value, int) and not isinstance(value, bool) and value > 0:
            config[key] = value
    return config


def is_doc_path(path):
    """A doc path is markdown, or lives under docs/ or .claude/, and is not a code-like file."""
    parts = path.split("/")
    if "hooks" in parts[:-1] or parts[-1] in CODE_FILE_NAMES:
        return False
    return path.endswith(".md") or parts[0] in DOC_DIRS


def is_risky_path(path, patterns):
    lowered = path.lower()
    return any(fnmatch.fnmatchcase(lowered, pattern.lower()) for pattern in patterns)


def classify_paths(paths):
    """Return risky, doc-only or normal for a list of changed paths."""
    patterns = load_config()["risk_paths"]
    if any(is_risky_path(path, patterns) for path in paths):
        return "risky"
    # An empty list is never doc-only, so an empty diff cannot skip a review.
    if paths and all(is_doc_path(path) for path in paths):
        return "doc-only"
    return "normal"


def read_text(path):
    try:
        with open(path, encoding="utf-8", errors="replace") as handle:
            return handle.read()
    except OSError as err:
        die(f"cannot read {path}: {err.strerror}")


def read_names(path):
    return [line.strip() for line in read_text(path).splitlines() if line.strip()]


def job_diff_names(root, job):
    """Return the changed paths of the whole job diff, or an empty list before a head is recorded."""
    if not job.get("base") or not job.get("head"):
        return []
    result = git(root, "diff", "--name-only", f"{job['base']}..{job['head']}")
    if result.returncode != 0:
        die(f"cannot diff {job['base']}..{job['head']}: {result.stderr.strip()}")
    return [line.strip() for line in result.stdout.splitlines() if line.strip()]


def newest_verdict(root, job, role):
    """Return the last line of the newest review-<role>-r<n>.md, or None when no report exists."""
    folder = job_dir(root, job)
    try:
        names = [n for n in os.listdir(folder) if re.fullmatch(rf"review-{role}-r\d+\.md", n)]
    except OSError:
        return None
    if not names:
        return None
    newest = max(names, key=lambda n: (os.path.getmtime(os.path.join(folder, n)), n))
    lines = [line.strip() for line in read_text(os.path.join(folder, newest)).splitlines() if line.strip()]
    return lines[-1] if lines else ""


def budget_state(_root, _job):
    """Return ok, warn or stop. Plan 08 replaces this with the real token budget."""
    state = os.environ.get("FACTORY_BUDGET_STATE", "ok")
    return state if state in ("ok", "warn", "stop") else "ok"


def cmd_classify_diff(args):
    print(classify_paths(read_names(args.names_file)))


def v1_roles(root, job, round_names, job_names):
    """Return the roles the v1 pipeline runs, with its skip rules."""
    roles = []
    if classify_paths(round_names) != "doc-only" or newest_verdict(root, job, "security") == CHANGES_LINE:
        roles.append("security")
    touches_src = any(path.startswith("src/") for path in round_names)
    for role in ("ux", "ui"):
        verdict = newest_verdict(root, job, role)
        first_look = verdict is None and any(path.startswith("src/") for path in job_names)
        # A role whose last verdict was CHANGES is never skipped, because skipping clears the CHANGES.
        if touches_src or verdict == CHANGES_LINE or first_look:
            roles.append(role)
    roles.append("code")
    return roles


def cmd_route(args):
    root = repo_root()
    job = find_job(load_board(root), args.id)
    round_names = read_names(args.names_file)
    job_names = job_diff_names(root, job)
    risky = classify_paths(round_names + job_names) == "risky"
    if job_mode(job) == "lean" and not risky and not args.escalate:
        print("code")
        return
    print(" ".join(v1_roles(root, job, round_names, job_names)))


def checks_blockers(root, job):
    path = os.path.join(job_dir(root, job), "checks.md")
    if not os.path.exists(path):
        return ["no checks.md recorded"]
    lines = read_text(path).splitlines()
    if any(line.startswith("FAIL ") for line in lines):
        return ["a check failed"]
    if not any(line.startswith("PASS ") for line in lines):
        return ["no passing check recorded"]
    return []


def cmd_can_auto_merge(args):
    root = repo_root()
    job = find_job(load_board(root), args.id)
    blockers = []
    if job_mode(job) != "lean":
        blockers.append("full mode job, the approver decides")
    blockers += checks_blockers(root, job)
    if newest_verdict(root, job, "code") != APPROVE_LINE:
        blockers.append("latest code review is not APPROVE")
    if not job.get("base") or not job.get("head"):
        blockers.append("job has no recorded base or head")
    elif classify_paths(job_diff_names(root, job)) == "risky":
        blockers.append("diff class is risky")
    if budget_state(root, job) == "stop":
        blockers.append("budget is stopped")
    if blockers:
        print("blocked: " + "; ".join(blockers))
        sys.exit(1)
    print("ok")
```

If plan 08 or another change already defined `read_text` or `is_doc_path` in the file, keep one copy.

4d. In `cmd_new_job`, make three edits and keep the changes plans 01, 03 and 04 made:

- Directly after the line `series_plan = plan_series_job(root, board, args)` add:

```python
    mode = args.mode or (job_mode(series_plan[0]) if series_plan else DEFAULT_MODE)
```

- In the job dict, directly after the line `"feature": args.feature,` add the line `"mode": mode,`.

4e. In `build_parser`, add to the `new-job` parser, next to its other `add_argument` lines:

```python
    new_job.add_argument("--mode", choices=MODES)
```

and directly before `return parser` add:

```python
    classify = sub.add_parser("classify-diff")
    classify.add_argument("--names-file", dest="names_file", required=True)
    classify.set_defaults(func=cmd_classify_diff)

    route = sub.add_parser("route")
    route.add_argument("id")
    route.add_argument("--names-file", dest="names_file", required=True)
    route.add_argument("--escalate", action="store_true")
    route.set_defaults(func=cmd_route)

    can_merge = sub.add_parser("can-auto-merge")
    can_merge.add_argument("id")
    can_merge.set_defaults(func=cmd_can_auto_merge)
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `bash .claude/factory/tests/run.sh | grep -E '^(FAIL|all passed|[0-9]+ failed)'`
Expected: `all passed` and no `FAIL:` line. If a `classify-diff` case fails, check the glob in `config.json` against the path in the test name. If a route case fails, print `lc route <id> --names-file "$lnames"` for that fixture and compare with the expected roles.

Run: `python3 -c "import ast,sys; ast.parse(open(sys.argv[1]).read())" .claude/factory/factory-ctl`
Expected: no output.

- [ ] **Step 6: Commit**

Invoke the skill `dev-workflow-skills:1-git-commit` for the message, running its git commands inline with no helper agents and no amend step. Suggested message: `feat(factory): add lean mode, risk classification and auto-merge check to ctl`. Then:

```bash
git add .claude/factory/factory-ctl .claude/factory/config.json .claude/factory/tests/run.sh && git commit --only -m "<message from the skill>" -- .claude/factory/factory-ctl .claude/factory/config.json .claude/factory/tests/run.sh
```

---

### Task 2: Command and agents for lean mode

**Files:**
- Modify: `.claude/factory/tests/run.sh`
- Modify: `.claude/commands/factory.md`
- Modify: `.claude/agents/factory-code-reviewer.md`
- Modify: `.claude/agents/factory-spec-writer.md`

**Interfaces:**
- Consumes: from Task 1, `ctl new-job --mode`, `ctl route`, `ctl classify-diff`, `ctl can-auto-merge`, `ctl show` (plan 01, prints the job JSON including `mode`). From plan 04, `ctl series-start`, `ctl check-series` and the new job flow steps 3 and 4. From plan 02, `ctl record-checks`, `prompt_has`, the `[blocker]` and `[minor]` tags. From plan 03, the merge step and its final `ctl tick-plan` step.
- Produces: the `--full` flag rule, the "Lean spec and plan flow" section, role routing in the review stage, the "Lean gate" in the approve stage, the `mode: lean` and `mode: full` reviewer prompt lines, the hard-rules checklist.

- [ ] **Step 1: Write the failing lint tests**

In `.claude/factory/tests/run.sh`, in the prompt-rules block (the `prompt_has` lines that plan 02 put before `# ---- factory-ctl ----`), add directly after the last line of that block:

```bash
prompt_has factory-code-reviewer 'mode: lean'
prompt_has factory-code-reviewer 'mode: full'
prompt_has factory-code-reviewer 'shadcn/ui primitives only'
prompt_has factory-code-reviewer 'lucide-react'
prompt_has factory-code-reviewer 'SingleLineEditor'
prompt_has factory-code-reviewer 'Monaco'
prompt_has factory-code-reviewer 'PromptEditor'
prompt_has factory-code-reviewer 'narrow selectors'
prompt_has factory-code-reviewer 'rename_all = "camelCase"'
prompt_has factory-code-reviewer 'unwrap()'
prompt_has factory-code-reviewer 'git2'
prompt_has factory-code-reviewer 'backward compatibility'
prompt_has factory-code-reviewer 'would fail without the change'
prompt_has factory-code-reviewer 'risky diff in lean mode'
prompt_has factory-spec-writer 'full mode only'
if fm "$agents/factory-spec-writer.md" description | grep -qF 'full mode only'; then
  pass "agent spec writer: description says full mode only"
else
  fail "agent spec writer: description says full mode only"
fi
```

Then, in the `# ---- /factory command ----` section, insert this block directly before the line `# ---- summary ----`:

```bash
for word in 'counts only when the very first word' 'make the --full flag work' 'ask the user for the feature text' \
  'Lean spec and plan flow' 'superpowers:brainstorming' 'superpowers:writing-plans' \
  'at most the questions' 'unavailable, done inline' 'Ruling: lean mode' 'ctl route' 'ctl classify-diff' \
  'mode: lean' 'mode: full' 'Lean gate' 'ctl can-auto-merge' '--escalate' 'Ruling: lean auto-merge' \
  'Ruling: lean escalation' 'continues straight into the approve stage' 'no approval.md'; do
  grep -qF -- "$word" "$cmd" && pass "command mentions: $word" || fail "command mentions: $word"
done
check "command: the argument hint lists --full" grep -q '^argument-hint:.*\[--full\] <feature>' "$cmd"
check "command: new-job is called with --mode full for the flag" grep -qF 'ctl new-job "<feature>" --mode full' "$cmd"
check "command: the spec writer is full mode only" grep -qF 'Full mode: dispatch `factory-spec-writer`' "$cmd"
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bash .claude/factory/tests/run.sh | grep -E '^FAIL' | head -60`
Expected: `FAIL:` for every new `prompt` check, the spec writer description check, and every new `command mentions:` and `command:` check.

- [ ] **Step 3: Edit the command: flag, argument hint and new job flow**

Edit `.claude/commands/factory.md`. Earlier plans reworded some lines, so find each place by the quoted start and keep their other additions.

(a) Frontmatter: in the `argument-hint:` line, replace the leading `"<feature> |` with `"[--full] <feature> |`.

(b) Directly after the sentence that ends `For example, "next button for pagination" is a feature.` add this paragraph:

```markdown
A feature may start with the flag `--full`. The flag counts only when the very first word of the arguments is exactly `--full` (two hyphens, lower case) and at least one more word follows. Then the job mode is `full` and everything after the flag is the feature text, even if it looks like a subcommand. In every other shape the mode is `lean`: "full screen mode" and "make the --full flag work" are lean features. A lone `--full` is not a feature: ask the user for the feature text.
```

(c) In the `## \`<feature>\`: new job` section, replace the step that starts ``1. `ctl new-job "<feature>"` `` with:

```markdown
1. Lean (the default): `ctl new-job "<feature>"`. Full, when the `--full` flag was given: `ctl new-job "<feature>" --mode full`, with the feature text that follows the flag. It prints the folder name and creates the job in stage `spec` with that mode.
```

Replace the step that starts `2. Dispatch the \`factory-spec-writer\` agent` with:

```markdown
2. Full mode: dispatch `factory-spec-writer` with the feature text and the absolute job directory. Lean mode: dispatch no agent. Run the Lean spec and plan flow (the next section) in this session, then continue with step 3.
```

(d) In `### stage \`spec\``, at the end of its paragraph add:

```markdown
For a lean job, rerun the Lean spec and plan flow instead of dispatching the spec writer. If `spec.md` already exists in the job directory, keep it and run only the plan skill.
```

(e) Directly above the line `## next`, add this section:

````markdown
## Lean spec and plan flow

Used for a lean job only (the default). You run the two skills yourself, in this session. The user is present and may answer questions. Nothing here is dispatched to an agent.

Mode summary, so you know what changes: lean uses no spec-writer agent, one `factory-builder` per plan, one combined `factory-code-reviewer`, and no approver unless the diff is risky or the lean gate blocks. Full mode keeps the v1 pipeline. Every ledger and board change in lean mode still goes through `ctl`.

1. `ctl ledger <id> "Ruling: lean mode, spec and plan written by the orchestrator"`.
2. Invoke the skill `superpowers:brainstorming` with the feature text. Ask the user at most the questions the skill itself needs to settle the design, one at a time as the skill says, and no extra confirmation questions of your own. If the user cannot answer (an unattended run), take the recommended option for each question and list every such choice under `Assumptions` in the spec. Where a skill says to save a document under `docs/superpowers/`, save it in the job directory instead: the spec is `<job dir>/spec.md`. Never commit it, never create a worktree and never start building: the builder does that. For work on yml, collection, environment, request or auth models, variable resolution or a collection-related Tauri command, read `docs/superpowers/specs/opencollection-spec-reference.md` first.
3. Invoke the skill `superpowers:writing-plans` with that spec. Save the plan in the job directory, not under `docs/superpowers/`. Make it fit the job model:
   - At most 3 tasks per plan. A single plan is `<job dir>/plan.md`. If the feature needs more, write a series: `<job dir>/plans/plan-NN-<slug>.md` (at most 6 plans), a table `<job dir>/plans/index.md` with one row per plan (number, file name, what it delivers), and a last line `Next plan: <path|none>` in every plan file. Paths are relative to `<main>`.
   - Directly under a `## Tasks` heading near the top of every plan file, list each task once as a checkbox line `- [ ] Task N: <title>`, in addition to the skill's own task headings. `ctl tick-plan` and the dashboard read only those lines.
   - Start every task that touches yml files, collection, environment, request or auth models, variable resolution, a Rust crate named in `CLAUDE.md`, or a collection-related Tauri command with the line `Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.`
   - Name the checks per task: `cargo check -j4`, `cargo test -j4 -p <crate> <test_name>`, `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern>`, and for `tools/factory-server/` tasks `cargo check -j4 --manifest-path tools/factory-server/Cargo.toml`. Never `--workspace`.
   - The skill ends by asking how to execute. Do not ask the user: the answer is "a `factory-builder` agent runs it with `superpowers:executing-plans`".
4. Check the plan with the same rules as step 3 of the new job flow. For a series, first run `ctl check-series <path of plan 01 relative to <main>>`, then `ctl series-start <id>`. For a single plan, check that `plan.md` has 1 to 3 lines starting with `- [ ] Task`. If a check fails, fix the plan files yourself once, because you wrote them, and check again. Only if it fails again, `ctl set-stage <id> needs-human --reason "bad plan"` and stop.
5. Continue with step 4 of the new job flow (the plan accepted ruling and `ctl set-stage <id> build`).

If the Skill tool is not available, or a skill call fails, do the same work inline and record it with `ctl ledger <id> "Ruling: <skill name> unavailable, done inline"`. Inline brainstorming: read the code the feature touches, ask the user only what the code cannot answer (at most 3 questions), then write `spec.md` with goal, scope, non-goals, acceptance criteria, risks and assumptions. Inline planning: write the plan files with exact file paths, a test-first step with real code per task, and the exact check commands.
````

- [ ] **Step 4: Edit the command: review routing, lean gate, merge minors**

Still in `.claude/commands/factory.md`:

(f) In `### stage \`review\``, replace the whole step that starts `Decide the UX and UI reviewers one by one` (plan 01 wrote it) with the step below. Keep the text of the diff-writing step before it. If plan 08 added a `ctl classify-diff` or security-skip step, keep the diff and names file writing, and let this step replace the skip rules, because `ctl route` applies them.

```markdown
Route the reviewers. The names file `review-<from7>..<to7>.names` of this round's diff must exist (write it by redirect: `git -C "<main>" diff --name-only <from>..<to> > <job dir>/review-<from7>..<to7>.names`). Run `ctl route <id> --names-file <job dir>/review-<from7>..<to7>.names`. It prints the roles to run on one line, for example `code` or `security ux ui code`. It already applies the mode, the risk class of the whole job diff, the doc-only rule for security, the `src/` rule for UX and UI, and the rule that a role whose last verdict was CHANGES is never skipped. Record every role that is not in the list with `ctl set-stage <id> review --review <role>=skipped`. A skipped role counts as pass. If the job is lean and the list has more than `code`, the diff is risky: run `ctl classify-diff --names-file <the names file>` once, then `ctl ledger <id> "Ruling: lean job is <class>, running the v1 review set"`.
```

In the step that dispatches the reviewers (it starts `Mark the reviewers you dispatch`), replace its list of four agents with: dispatch only the roles `ctl route` printed, in one message, in parallel (`factory-security-reviewer`, `factory-ux-reviewer`, `factory-ui-reviewer`, `factory-code-reviewer`). Then append to that step:

```markdown
Tell the `factory-code-reviewer` its mode in one line of its prompt. If `ctl route` printed only `code`, write `mode: lean`: it is the only reviewer and also checks the hard rules. Otherwise write `mode: full`.
```

In the step that handles "All pass" (it starts `All pass`), append:

```markdown
A lean job continues straight into the approve stage in the same turn, so the user does not need another `/factory next` for it.
```

(g) At the start of `### stage \`approve\``, before its first paragraph, add:

```markdown
#### Lean gate

Read the mode with `ctl show <id>`. A lean job takes the lean gate before anything else in this stage. A full job skips this section.

1. Run `ctl is-merged <id>` as in the next paragraph. Exit 0 with `merged <sha>` means the branch is already on main: go to the last merge step (`ctl tick-plan`) with that sha.
2. Run `ctl can-auto-merge <id>`. Exit 0 (it prints `ok`) means no approver: run `ctl ledger <id> "Ruling: lean auto-merge, checks pass, code review APPROVE, diff not risky"` and then the Merge step. There is no approval.md for a lean auto-merge. Skip the rest of this stage.
3. Exit 1 prints `blocked: <reasons>`. Run `ctl ledger <id> "Ruling: lean escalation, <reasons>"`. Write the whole job diff names by redirect (`git -C "<main>" diff --name-only <base>..<final head> > <job dir>/review-<base7>..<head7>.names`) and the full diff as the next paragraph says. Run `ctl route <id> --names-file <that names file> --escalate`. For every printed role whose state in `ctl show <id>` is not `pass`, run that reviewer now: mark it `running`, dispatch all of them in one message with the diff path, `spec.md` path and worktree path (tell `factory-code-reviewer` `mode: full`), save each reply as `review-<role>-r<round>.md`, and record `pass` or `changes` with `ctl set-stage <id> review --review <role>=<state>`. Any CHANGES: handle it exactly like the review stage does (combine the findings into `changes-r<round+1>.md`, then `ctl set-stage <id> build --round <round+1>`, or `needs-human` at round 2). When every printed role passed, continue with the approver in the next paragraph.
```

(h) In the Merge step, in the final step that runs `ctl tick-plan <id> --merge <merge sha>`, append this sentence:

```markdown
For a lean auto-merge there is no approval.md, so carry the `[minor]` findings of the newest `review-code-r<n>.md` instead: run `ctl ledger <id> "Minor: <item>"` for each and list them in your report.
```

(i) Sync the lint note: if the file has a sentence that says the approver always runs, change it to "the approver runs for full jobs and for lean jobs the lean gate blocks".

- [ ] **Step 5: Edit the agent prompts**

(a) `.claude/agents/factory-code-reviewer.md`: insert this block, followed by a blank line, directly before the line `Your last line, alone, must be exactly one of:`:

```
Hard rules check. The orchestrator tells you the mode in its prompt. When it says `mode: lean` you are the only reviewer of this diff, so also check every rule below and report each violation as a `[blocker]` finding with `file:line`. When it says `mode: full`, the security, UX and UI reviewers cover these rules, so skip this list.

- shadcn/ui primitives only: no raw `<button>`, `<input>`, `<dialog>`, `<select>` or `<form>` in `src/`.
- Icons come from `lucide-react` only: no inline `<svg>`.
- Single-line variable-aware fields use `SingleLineEditor` (CodeMirror 6). Multi-line editors use Monaco. The one exception is `PromptEditor`, the AI Assistant prompt editor.
- Zustand: use narrow selectors, never a full destructure of store state at the top of a component.
- serde: `#[serde(rename_all = "camelCase")]` on IPC DTOs only, never on persistence structs. Persistence structs keep backward compatibility (optional fields, defaults).
- Rust: no `unwrap()` in production paths, and no shelling out to the `git` CLI (use `git2`).
- Tests exist for the new behaviour and would fail without the change.
- If the diff touches the collection trust gate, auth, secrets, `{{process.env}}`, a persistence format, the IPC surface, a manifest or the harness hooks, report a `[blocker]` finding "risky diff in lean mode". Such a job should have been routed to the full review set.
```

(b) `.claude/agents/factory-spec-writer.md`: in the `description:` line, replace the sentence `Use at the start of a factory job.` with `Use at the start of a full mode factory job only.` Keep the description on one line and add no colon followed by a space. Then directly after the line that starts `You are the spec writer in Rocket's software factory.` add this paragraph:

```
You run in full mode only (`/factory --full`). In lean mode the orchestrator writes the spec and the plans itself, in the main session, with the same two skills, so you are never dispatched for a lean job.
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `bash .claude/factory/tests/run.sh | grep -E '^(FAIL|all passed|[0-9]+ failed)'`
Expected: `all passed`. If `command contract: ctl <word> is a real subcommand` fails, a sentence in `factory.md` has `ctl` followed by a lowercase word that is not a subcommand: reword it. If a `command mentions:` check fails, make the prose contain the exact phrase.

- [ ] **Step 7: Commit**

Invoke the skill `dev-workflow-skills:1-git-commit` for the message, inline, with no helper agents. Suggested: `feat(factory): run lean mode by default with a combined code reviewer`. Then:

```bash
git add .claude/factory/tests/run.sh .claude/commands/factory.md .claude/agents/factory-code-reviewer.md .claude/agents/factory-spec-writer.md && git commit --only -m "<message from the skill>" -- .claude/factory/tests/run.sh .claude/commands/factory.md .claude/agents/factory-code-reviewer.md .claude/agents/factory-spec-writer.md
```

---

### Task 3: Docs, token note and verification

**Files:**
- Modify: `.claude/software-factory.md`
- Modify: `docs/roadmaps/roadmap.md`
- Modify: `.claude/factory/tests/run.sh`

**Interfaces:**
- Consumes: the command and agent text from Task 2, the ctl names from Task 1. The doc sections `## Pipeline`, `## Job model`, `## Agents`, `## Approver risk rules`, `## Commands` and `## Open items` as plans 01 to 06 left them.
- Produces: a design doc that describes both modes, a token comparison note (measured numbers for job 001, an estimate for lean), a dry-run recipe, and doc lint tests.

- [ ] **Step 1: Write the failing doc tests**

In `.claude/factory/tests/run.sh`, insert this block directly before the line `# ---- summary ----`:

```bash
# ---- lean mode docs ----
doc="$repo/.claude/software-factory.md"
for word in 'Lean (default)' 'Full (opt-in)' '/factory --full' 'risk_paths' 'ctl can-auto-merge' 'ctl route' \
  'Token comparison' '12.75M' 'This is an estimate, not a measurement' 'Dry run for lean mode' 'one combined'; do
  grep -qF -- "$word" "$doc" && pass "doc mentions: $word" || fail "doc mentions: $word"
done
check "doc: the spec-writer row says full mode only" grep -qE 'factory-spec-writer.*[Ff]ull mode only' "$doc"
check "doc: the pipeline section shows the lean flow" bash -c "sed -n '/^## Pipeline/,/^## Job model/p' '$doc' | grep -qF 'ctl can-auto-merge'"
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bash .claude/factory/tests/run.sh | grep -E '^FAIL: doc'`
Expected: a `FAIL:` line for each new doc check.

- [ ] **Step 3: Rewrite the Pipeline section**

In `.claude/software-factory.md`, replace everything from the line `## Pipeline` up to, but not including, the line `## Job model` with:

````markdown
## Pipeline

A job has a mode, stored on the board as `mode`. Lean is the default. `/factory --full <feature>` opts into the full pipeline. A job on the board without a `mode` field (created before lean mode) keeps the full pipeline. A lean job that turns out risky keeps `mode: lean`, and the escalation is a `Ruling:` line in its ledger.

### Lean (default)

```
/factory <feature>
  orchestrator, main session: superpowers:brainstorming (may ask you) -> spec.md
                              superpowers:writing-plans             -> plan.md or plans/
        |  ctl series-start (series), or the 1 to 3 task check (single plan)
        v
  factory-builder (opus, own worktree, superpowers:executing-plans) -> CHECKS block -> ctl record-checks
        |
        v
  ctl route <id> --names-file ...      (ctl classify-diff: doc-only, normal or risky)
        |-- doc-only or normal -> factory-code-reviewer (sonnet, also checks the hard rules)
        |        |-- CHANGES (max 2 rounds) -> orchestrator resumes the builder
        |        '-- APPROVE -> ctl can-auto-merge exits 0 -> merge to local main (no approver)
        '-- risky, or ctl can-auto-merge blocked -> v1 review set -> approver
                 |-- APPROVE  -> merge to local main
                 '-- ESCALATE -> needs-human (/factory approve | /factory rework)
```

The orchestrator runs the two skills itself, so it may ask you the questions the skill needs, and nothing else. It saves `spec.md` and the plan files into the job directory. A plan check from plans 01 and 04 follows. One `factory-builder` runs each plan. One combined `factory-code-reviewer` (Sonnet) reviews the diff and also checks the hard rules (shadcn only, lucide only, `SingleLineEditor` vs Monaco, narrow Zustand selectors, serde camelCase on IPC DTOs only, no unwrap in production paths, no git CLI, persistence backward compatibility, tests present). There is no approver when `ctl can-auto-merge` exits 0, which needs: `checks.md` with no `FAIL`, the newest code verdict `APPROVE`, a diff class that is not `risky`, and a budget that is not `stop`. Lean jobs still use ctl for every ledger and board change.

Risky means the diff touches the collection trust gate, auth, secrets or `{{process.env}}`, a persistence format, the IPC surface or `DomainEvent`, a manifest (`Cargo.toml`, `Cargo.lock`, `package.json`), `tauri.conf.json`, the CSP, or the harness hooks. The path globs live in `.claude/factory/config.json`, key `risk_paths`. A missing or broken config makes everything risky. A risky job runs the v1 review set, then the approver.

### Full (opt-in)

```
/factory --full <feature> -> spec-writer (opus) -> builder (opus, own worktree) -> reviewers in parallel -> approver (opus)
                                                        ^                              |                       |-- APPROVE  -> merge to local main
                                                        |   any CHANGES (max 2)        |                       |-- ESCALATE -> needs-human
                                                        +------ orchestrator resumes ---+                           (/factory approve | /factory rework)
```

Reviewers: security (opus, skipped for a doc-only diff), ux (sonnet), ui design (sonnet), code (sonnet). UX and UI run when the round diff touches `src/`, or when their last verdict was CHANGES. The spec writer runs unattended: it picks the recommended option and records assumptions. `ctl route` applies the skip rules in both modes.

The orchestrator is the interactive session. It owns the round counter, writes the ledger and the board, and does the merge. In full mode it never does builder or reviewer work. In lean mode it also writes the spec and the plans.
````

- [ ] **Step 4: Update the Agents section and the other doc sections**

In `.claude/software-factory.md`, read each section as plans 01 to 06 left it, then:

1. In `## Agents`, add this paragraph directly under the heading (before the table): `Lean mode uses only \`factory-builder\` and one combined \`factory-code-reviewer\` for an ordinary diff. The spec writer, the security, UX and UI reviewers and the approver run in full mode, and in lean mode only when the diff is risky (or the lean gate blocks), except the spec writer, which is never used in lean mode.`
2. In the table, append to the end of the Rocket duties cell of each row, keeping the existing text:
   - Row `factory-spec-writer`: ` Full mode only. In lean mode the orchestrator runs the same two skills itself.`
   - Row `factory-code-reviewer`: ` In lean mode it is the one combined reviewer (\`mode: lean\` in its prompt) and also checks the hard rules: shadcn only, lucide only, SingleLineEditor vs Monaco, narrow Zustand selectors, serde camelCase on IPC DTOs only, no unwrap in production paths, no git CLI, persistence backward compatibility, tests present.`
   - Rows `factory-security-reviewer`, `factory-ux-reviewer`, `factory-ui-reviewer` and `factory-approver`: ` Full mode, and lean mode only for a risky diff or a blocked lean gate.`
3. In `## Job model`, add this bullet: `- Mode: the board field \`mode\` is \`lean\` (default) or \`full\`, set by \`ctl new-job --mode\`. A later plan of a series inherits the mode of its origin job. \`ctl route <id> --names-file <path> [--escalate]\` prints the reviewer roles to run, \`ctl classify-diff --names-file <path>\` prints \`doc-only\`, \`normal\` or \`risky\`, and \`ctl can-auto-merge <id>\` exits 0 when no approver is needed.`
4. In `## Approver risk rules`, add a last line: `The same list drives lean mode: \`.claude/factory/config.json\` (\`risk_paths\`) decides which diffs are risky. In lean mode the approver only runs for a risky diff or a blocked lean gate.`
5. In `## Commands`, change the line that starts `- \`/factory <feature>\`` to `- \`/factory [--full] <feature>\`: create a job. Lean (default): the orchestrator runs \`superpowers:brainstorming\` and \`superpowers:writing-plans\` in the main session. With \`--full\` (the first word, exactly), the spec-writer agent does it unattended.` Keep the rest of the line if plan 04 added series wording.
6. In `## Open items`, add: `- Lean mode (plan 07) is built and unit-tested. The first real lean run is open, see "Dry run for lean mode".`
7. In the `## Verdict contract` paragraph, append: `In lean mode the orchestrator decides the merge from the newest \`review-code-r<n>.md\` (by modification time, last line exactly \`VERDICT: APPROVE\`) through \`ctl can-auto-merge\`.`

- [ ] **Step 5: Add the token comparison and the dry run sections**

Append these two sections to `.claude/software-factory.md`, directly before `## Open items` if that is the last section, otherwise at the end of the file.

````markdown
## Token comparison

Measured on job 001 (full pipeline, issue 21, a 258-line test file): about 12.75M raw tokens, of which 11.76M were cache reads, so fresh input plus cache creation plus output was about 0.99M. With the weighted formula `input + cache creation + output + cache reads / 10` that is about 2.17M weighted. The orchestrator session was 53.6 percent of the raw total (6.83M, largest context 155K re-read each turn). There were 9 agent runs plus 11 helper agents.

| | Job 001, full (measured) | Lean (estimate) |
|---|---|---|
| Spec writer run | yes | none, the orchestrator writes spec and plan |
| Builder runs | 1 plus resume | 1 plus resume |
| Reviewer runs per round | security, ux, ui, code | code only, unless risky |
| Approver run | yes | none unless risky |
| Orchestrator share | 6.83M raw | about the same or a little higher (it now writes the spec and plan) |
| Raw total | about 12.75M | roughly 8M to 10M |

This is an estimate, not a measurement. It assumes the agent runs outside the orchestrator (about 5.9M raw in job 001) shrink by about half or more, because the spec writer, three reviewers, the approver and most helper agents are gone, while the builder stays. The orchestrator share is mostly session baggage that no mode removes. Plan 08 attacks it (diffs out of the context, commit ceremony, a token budget). After plan 08, measure the first real lean job with `ctl tokens` and replace this estimate with the number.

## Dry run for lean mode

Run this once after plans 01 to 07 are merged. It uses a real, tiny feature and real agent time. The builder of plan 07 does not run it.

1. In a fresh Claude Code session on `main` with a clean tree, start the dashboard: `cargo run -j4 --manifest-path tools/factory-server/Cargo.toml -- --root "$(pwd)" --port 8765`, and open http://127.0.0.1:8765/.
2. Ordinary lean job. Run `/factory Add a one-line note about lean mode to .claude/software-factory.md`. Expect: the session asks you at most the questions the brainstorming skill needs, `spec.md` and `plan.md` appear in the job folder, and `ctl status` shows the job in `build`. Run `/factory next`. Expect: a builder in a worktree, a `checks.md` file, one code review (`review-code-r1.md`, nothing for security, ux or ui), the ledger line `Ruling: lean auto-merge`, a merge commit on local main, and no `approval.md`.
3. Risky lean job. Run `/factory Add a dev-dependency note comment to a Cargo.toml in a scratch crate` (or any small change that touches a `Cargo.toml`). Expect: the ledger line `Ruling: lean job is risky, running the v1 review set`, security and code reviews, and an `approval.md` from the approver.
4. Full job. Run `/factory --full Add a one-line note about full mode to .claude/software-factory.md`. Expect: a spec-writer agent runs, and the reviewers and the approver run as in v1 (only roles `ctl route` lists).
5. Flag shape. Run `/factory full screen mode for the response viewer` and stop it after the job is created. Expect `ctl show <id>` to print `"mode": "lean"`.
6. Pass means: no manual intervention for step 2, the escalation in step 3 happened on its own, and `git status` is clean in the main checkout. Clean up with `/factory cleanup <id>` for each job.
````

- [ ] **Step 6: Roadmap note**

In `docs/roadmaps/roadmap.md`, find the row whose text contains `Software factory v2 series` (plan 06 added it). In that row, replace `plans 01 to 06` with `plans 01 to 08` and append to the same cell: `; lean mode is the default and \`/factory --full\` opts into the v1 pipeline (plan 07)`. If the row is missing, add a one-line note under the factory entry of the same file with that text instead. Do not create a new R-id.

- [ ] **Step 7: Full verification**

Run each command and read its output:

```bash
bash .claude/factory/tests/run.sh 2>&1 | tail -n 5
python3 -c "import ast,sys; ast.parse(open(sys.argv[1]).read())" .claude/factory/factory-ctl
jq -e '.risk_paths | length > 0' .claude/factory/config.json
git diff --stat
git status --short
git branch --list 'factory/*'
```

Expected: `all passed`, no syntax error, `true`, a diff that touches only the files named in this plan, and the same `factory/*` branch list as before the plan (fixtures use temp repos only). Do not run a real `/factory` job, the dry run in the doc is for the user.

- [ ] **Step 8: Commit**

Invoke the skill `dev-workflow-skills:1-git-commit` for the message, inline, with no helper agents. Suggested: `docs(factory): document lean mode, the token estimate and a dry run`. Then:

```bash
git add .claude/software-factory.md docs/roadmaps/roadmap.md .claude/factory/tests/run.sh && git commit --only -m "<message from the skill>" -- .claude/software-factory.md docs/roadmaps/roadmap.md .claude/factory/tests/run.sh
```

The reply must contain: the result of each Step 7 command, the commit shas, and a one-line reminder that the lean dry run in `.claude/software-factory.md` is for the user to run.

---

## Self-Review

- Spec coverage (decision 12 and the plan 07 interface row): board field `mode` and `new-job --mode` (Task 1), `config.json` `risk_paths` with the decision 12 list (Task 1, mapped in a note), `classify-diff` three-way output (Task 1), `route` for lean, risky and full including the v1 skip rules (Task 1), `can-auto-merge` with the four conditions (Task 1), the `--full` exact-shape flag, the lean spec and plan flow with both skills and the inline fallback, the question limit, saving into the job directory, and the plan checks from plans 01 and 04 (Task 2), no approver in lean when the gate passes and the v1 set plus approver otherwise (Task 2, Lean gate), one combined Sonnet code reviewer with the hard-rules checklist (Task 2), the spec writer marked full only (Tasks 2 and 3), docs, token note and dry run (Task 3). `checks.md` from plan 02 and plan 03's merge step are read only. Decision 9's spec-writer note is honored.
- Placeholder scan: the only substitution points are the commit messages, which the commit skill writes, and `<...>` values in command text that the orchestrator fills at run time.
- Type consistency: `job_mode`, `classify_paths`, `newest_verdict`, `budget_state`, `v1_roles`, `APPROVE_LINE` and `CHANGES_LINE` keep one signature across Task 1. The role order `security ux ui code` is the same in the code, the tests and the doc. The blocked messages used in the tests match the strings in `cmd_can_auto_merge` and `checks_blockers`.
- Interplay with plan 08 (written before this plan): plan 08 Task 1 also creates `config.json` and defines `classify-diff`, `load_config`, `is_doc_path` and `read_text`. After this plan those exist, so plan 08 must change them: modify `config.json` instead of creating it (keep `risk_paths`), extend `CONFIG_DEFAULTS` and `load_config` with its integer keys instead of redefining them, accept the three-way `classify-diff` output (`doc-only`, `normal`, `risky`) where it expects `doc-only` or `code`, skip its own security skip rule in the command text because `ctl route` applies it, and replace the `budget_state` body here with the real token check (drop the `FACTORY_BUDGET_STATE` seam).

Next plan: docs/superpowers/plans/2026-10-13-software-factory-v2/plan-08-token-efficiency.md
