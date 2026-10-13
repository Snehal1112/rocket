# Software Factory v2 Plan 05: Dashboard, Server and Hook

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. This plan is executed by a factory-builder subagent. Steps use checkbox (`- [ ]`) syntax for tracking. **Plans 01 to 04 must be finished first** (they change `factory-ctl`, `factory.md` and the board fields this plan reads).

**Goal:** Close the dashboard, server and hook gaps E1 to E9 and the series chip (D5): redact and tighten the event log, keep stage history and per-stage times, attribute agents without guessing, bound the log reader, and fix the page (series chip, copy chips, no aria-live churn, minimal DOM patching).

**Architecture:** The hook (`log-event`) logs less and writes a private file. `factory-ctl set-stage` appends to a `stage_history` list on the board. The Rust server reads `stage_history` and `series` from `board.json`, derives per-stage durations, reads only the end of a big log, prunes its agent map, resets on rotation and exposes `skipped_lines`. The page stays plain DOM and string renderers; a new `patch.js` morphs only changed nodes into the live DOM.

**Tech Stack:** Python 3 (hook and ctl), Bash test scripts, Rust (tiny_http, serde), vanilla JS tested with `node --test`.

**Spec:** `docs/superpowers/specs/2026-10-13-software-factory-v2-design.md` (gaps E1 to E9, D5, row "05" of "Interfaces between plans"). Design doc: `.claude/software-factory.md`.

## Global Constraints

- Commands: `cargo` always with `-j4`, never `--workspace` or `--all`. Server tasks run `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml` and `cargo check -j4 --manifest-path tools/factory-server/Cargo.toml`.
- No `unwrap()` in Rust production paths. Tests use `Result` returns and `?`, as the existing tests do.
- Dashboard stays outside the Rocket app: files only under `tools/factory-server/`, `.claude/factory/`, `.claude/software-factory.md`. No new libraries, no external requests, plain DOM.
- Every value from `board.json` or the event log goes through `escapeHtml` before it reaches an HTML string. Class names built from data are whitelisted.
- Status is never shown by colour alone.
- The hook always exits 0 and stays async. It never blocks an agent.
- The server never writes `board.json` or `events.jsonl`.
- Board fields from this plan: `stage_history` (list of `{"stage", "ts"}`), read-only `series` (written by plan 04: `id`, `index`, `total`, `next_plan`). Server state adds `skipped_lines` and per-job `stage_durations`.
- Already fixed in commit `c0794226`, do not redo: `SubagentStop` from an untyped agent that never started is ignored, and helper agents get no entry. Also already in the hook: `url` targets lose their query and fragment, and `PostToolUse` without `agent_id` is skipped.
- Code comments: short full sentences ending in a period. No emojis.
- Commits: invoke skill `dev-workflow-skills:1-git-commit`, then a pathspec commit (`git add <paths> && git commit --only -m "..." -- <paths>`). Never `git add -A` or `git add .`.

## Review Focus

- A Bash command that starts with a secret assignment or holds a token in its arguments must leave only the command name in the log. A pre-existing mode 0644 log must be tightened to 0600. Tested in Task 1.
- Two jobs in the same stage and an agent with no worktree match: the agent must show as unassigned, never on the first card. Tested in Task 2.
- An old `board.json` with no `stage_history`, or a malformed `series` or history entry, must not turn the whole board into `board_error`. Tested in Task 2.
- A log much larger than the read window must not flood the registry, and the cut first line must not count as a skipped line. Tested in Task 2.
- A series or duration value that is missing, zero or not a number must not break a card or show `NaN`. Tested in Task 3.

## File Structure

| File | Change |
|---|---|
| `.claude/factory/hooks/log-event` | Modify: command name only, userinfo strip, mode 0600 |
| `.claude/factory/factory-ctl` | Modify: `set-stage` and `new-job` write `stage_history` |
| `.claude/factory/tests/run.sh` | Modify: hook and ctl checks |
| `tools/factory-server/src/model.rs` | Modify: `StageEntry`, `Series`, lenient fields on `Job` |
| `tools/factory-server/src/state.rs` | Modify: durations, `skipped_lines`, unique-stage attribution, stale at 10 minutes |
| `tools/factory-server/src/events.rs` | Modify: `reset`, `prune`, windowed `Tailer`, rotation flag, `pump` |
| `tools/factory-server/src/server.rs` | Modify: CSP constant, use `pump`, serve `/patch.js` |
| `tools/factory-server/static/patch.js` | Create: minimal DOM morph |
| `tools/factory-server/static/board.js`, `app.js`, `dashboard.html`, `dashboard.css` | Modify |
| `tools/factory-server/tests/board.test.js`, `smoke.sh` | Modify |
| `tools/factory-server/README.md`, `.claude/software-factory.md` | Modify: doc sync |

`.claude/settings.json` needs no change: the three hook events are already wired and async.

---

### Task 1: Hook redaction and private log

Covers E6 and the hook half of E1. What is left of E1 after `c0794226` is the interrupted resumed builder: it never gets a `SubagentStop`, and no hook can know that. Task 2 handles it by marking such agents stale after 10 minutes without an event.

**Files:**
- Modify: `.claude/factory/hooks/log-event`
- Test: `.claude/factory/tests/run.sh`

**Interfaces:**
- Consumes: none.
- Produces: `tool_target` of a Bash event is the command name only (at most 40 characters, `"shell"` when the name is not a plain word). A `url` target has no userinfo, query or fragment. The log file is mode 0600 whether new or old.

- [ ] **Step 1: Replace the old long-command test and add failing tests**

In `.claude/factory/tests/run.sh`, replace the two lines that start with `long=` through `check "logger: trims a long command to 80 chars" ...` (keep the `run_logger` call between them) so that the block reads:

```bash
long="$(python3 -c 'print("x" * 500)')"
run_logger "{\"hook_event_name\":\"PostToolUse\",\"agent_id\":\"a1\",\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"$long\"},\"tool_response\":\"SECRET-OUTPUT\",\"cwd\":\"/x\"}"
check "logger: a very long command name is cut to 40 chars" jq -e 'select(.tool_name=="Bash" and (.tool_target | length) == 40)' "$log"
check "logger: never stores tool_response" bash -c "! grep -q SECRET-OUTPUT '$log'"
run_logger '{"hook_event_name":"PostToolUse","agent_id":"a1","tool_name":"Bash","tool_input":{"command":"API_KEY=hunter2 curl -H \"Authorization: Bearer TOPSECRET\" https://x.example"},"cwd":"/x"}'
check "logger: a Bash command logs only its name" jq -e 'select(.tool_name=="Bash" and .tool_target=="curl")' "$log"
check "logger: a secret in the command never reaches the log" bash -c "! grep -q -e hunter2 -e TOPSECRET '$log'"
run_logger '{"hook_event_name":"PostToolUse","agent_id":"a1","tool_name":"Bash","tool_input":{"command":"cd /secret/place && make deploy"},"cwd":"/x"}'
check "logger: a compound command logs the first name only" bash -c "jq -e 'select(.tool_target==\"cd\")' '$log' && ! grep -q 'secret/place' '$log'"
run_logger '{"hook_event_name":"PostToolUse","agent_id":"a1","tool_name":"Bash","tool_input":{"command":"TOKEN=\"s3 cret\" curl x"},"cwd":"/x"}'
check "logger: a quoted assignment gives the safe name shell" bash -c "jq -e 'select(.tool_target==\"shell\")' '$log' && ! grep -q 's3' '$log'"
run_logger '{"hook_event_name":"PostToolUse","agent_id":"a1","tool_name":"WebFetch","tool_input":{"url":"https://user:pw123@x.example/p?q=1"},"cwd":"/x"}'
check "logger: a url target loses its userinfo" bash -c "jq -e 'select(.tool_target==\"https://x.example/p\")' '$log' && ! grep -q pw123 '$log'"

mlog="$tmp/mode.jsonl"
printf '{"hook_event_name":"SubagentStart","agent_id":"m1","cwd":"/x"}' | FACTORY_EVENT_LOG="$mlog" python3 "$logger"
check "logger: a new log file is mode 600" test "$(stat -c %a "$mlog")" = 600
olog="$tmp/old-mode.jsonl"
: >"$olog"
chmod 644 "$olog"
printf '{"hook_event_name":"SubagentStart","agent_id":"m2","cwd":"/x"}' | FACTORY_EVENT_LOG="$olog" python3 "$logger"
check "logger: an existing 644 log is tightened to 600" test "$(stat -c %a "$olog")" = 600
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bash .claude/factory/tests/run.sh 2>&1 | grep -E "FAIL|passed|failed"`
Expected: FAIL lines for the long-name, name-only, secret, compound, quoted, userinfo and both mode checks.

- [ ] **Step 3: Implement**

In `.claude/factory/hooks/log-event`:

Add `import re` after `import os`. Replace `MAX_COMMAND = 80` with:

```python
MAX_COMMAND_NAME = 40
ASSIGNMENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")
SAFE_NAME = re.compile(r"^[A-Za-z0-9_.+-]+$")
USERINFO = re.compile(r"^([A-Za-z][A-Za-z0-9+.-]*://)[^/?#]*@")
```

Add these two functions before `tool_target`:

```python
def command_name(command):
    """Return only the program name of a shell command, never its arguments."""
    for token in command.split():
        if ASSIGNMENT.match(token):
            continue
        name = os.path.basename(token)
        if SAFE_NAME.match(name):
            return name[:MAX_COMMAND_NAME]
        return "shell"
    return ""


def clean_url(value):
    """Drop userinfo, query and fragment from a url."""
    value = USERINFO.sub(r"\1", value)
    return value.split("?", 1)[0].split("#", 1)[0]
```

In `tool_target`, replace the two lines under `if key == "url":` with `value = clean_url(value)`, and replace `return command[:MAX_COMMAND]` with `return command_name(command)`.

In `main`, replace the `fd = os.open(...)` line with:

```python
        fd = os.open(path, os.O_WRONLY | os.O_APPEND | os.O_CREAT, 0o600)
        try:
            os.fchmod(fd, 0o600)
            os.write(fd, line.encode("utf-8"))
        finally:
            os.close(fd)
```
and delete the old `try: os.write ... finally: os.close(fd)` block that followed it (so the file is opened once and closed once).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `bash .claude/factory/tests/run.sh 2>&1 | grep -E "FAIL|passed|failed"`
Expected: `all passed` and no FAIL line.

- [ ] **Step 5: Commit**

Invoke skill `dev-workflow-skills:1-git-commit`, then:

```bash
git add .claude/factory/hooks/log-event .claude/factory/tests/run.sh && git commit --only -m "fix(factory): log only command names and keep the event log private" -- .claude/factory/hooks/log-event .claude/factory/tests/run.sh
```

---

### Task 2: Stage history, attribution and a bounded log reader

Covers E2, E3 (data), E4, E5 (data), E9, D5 (data) and the stale half of E1.

**Files:**
- Modify: `.claude/factory/factory-ctl`, `.claude/factory/tests/run.sh`
- Modify: `tools/factory-server/src/model.rs`, `state.rs`, `events.rs`, `server.rs`

**Interfaces:**
- Consumes: plan 04 writes `series` on a job as `{"id", "index", "total", "next_plan"}`.
- Produces: board `jobs[].stage_history = [{"stage": str, "ts": float}]`. API `skipped_lines: u64`, per job `series` (object or null) and `stage_durations: [{"stage", "seconds", "current"}]`. Rust: `events::pump(tailer, registry, now)`, `Tailer::with_window(path, bytes)`, `Registry::reset()`, `Registry::prune(now)`, `server::CSP`.

#### Part A: ctl stage history

- [ ] **Step 1: Write the failing ctl tests**

In `.claude/factory/tests/run.sh`, directly after the check `ctl: --reset-reviews sets every review to pending`, add:

```bash
check "ctl: new-job starts the stage history with spec" \
  jq -e '.jobs[1].stage_history | length == 1 and .[0].stage == "spec"' "$board"
check "ctl: set-stage appends every stage change to the history" \
  jq -e '.jobs[0].stage_history | map(.stage) == ["spec","build","review"]' "$board"
python3 "$ctl" set-stage 1 review >/dev/null
check "ctl: set-stage in the same stage does not append" \
  jq -e '.jobs[0].stage_history | length == 3' "$board"
check "ctl: history timestamps never go backward" \
  jq -e '.jobs[0].stage_history | [.[].ts] | . == sort' "$board"
jq 'del(.jobs[2].stage_history)' "$board" >"$board.tmp" && mv "$board.tmp" "$board"
python3 "$ctl" set-stage 3 build >/dev/null
check "ctl: an old job without history gets its current stage seeded first" \
  jq -e '.jobs[2].stage_history | map(.stage) == ["spec","build"]' "$board"
```

- [ ] **Step 2: Run to verify failure**

Run: `bash .claude/factory/tests/run.sh 2>&1 | grep -E "FAIL|passed|failed"`
Expected: FAIL on the five new ctl checks.

- [ ] **Step 3: Implement in `factory-ctl`**

Add above `cmd_new_job`:

```python
def record_stage(job, stage, now):
    """Append a stage change to the history, seeding it for jobs that have none."""
    history = job.setdefault(
        "stage_history", [{"stage": job["stage"], "ts": job.get("stage_since", now)}]
    )
    history.append({"stage": stage, "ts": now})
```

In `cmd_new_job`, compute `now = time.time()` before `board["jobs"].append(`, and in the new job dict set `"stage_since": now,` and add `"stage_history": [{"stage": "spec", "ts": now}],` next to it.

In `cmd_set_stage`, replace the block

```python
    if job["stage"] != args.stage:
        job["stage"] = args.stage
        job["stage_since"] = time.time()
```
with
```python
    if job["stage"] != args.stage:
        now = time.time()
        record_stage(job, args.stage, now)
        job["stage"] = args.stage
        job["stage_since"] = now
```
If plans 01 or 03 already wrapped this block (backward-move check, lock), keep their code and only add the `record_stage` call and the shared `now` where `stage_since` is set.

- [ ] **Step 4: Run to verify pass**

Run: `bash .claude/factory/tests/run.sh 2>&1 | grep -E "FAIL|passed|failed"`
Expected: `all passed`.

- [ ] **Step 5: Commit**

Invoke skill `dev-workflow-skills:1-git-commit`, then:

```bash
git add .claude/factory/factory-ctl .claude/factory/tests/run.sh && git commit --only -m "feat(factory): record stage history in ctl set-stage" -- .claude/factory/factory-ctl .claude/factory/tests/run.sh
```

#### Part B: model and durations

- [ ] **Step 6: Write the failing model and duration tests**

In `tools/factory-server/src/state.rs` tests module, first update the `job()` helper: add `stage_history: Vec::new(),` and `series: None,` after `reviews,` in the `Job { ... }` literal. Then add:

```rust
    #[test]
    fn an_old_board_without_history_or_series_still_loads() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("board.json");
        std::fs::write(
            &path,
            r#"{"jobs":[{"id":"001","dir":"d","slug":"s","created":"2026-10-13","feature":"f","branch":"b","stage":"build","round":0,"stage_since":5.0,"reviews":{}}]}"#,
        )?;
        let (board, error) = load_board(&path);
        assert!(error.is_none());
        assert!(board.jobs[0].stage_history.is_empty());
        assert!(board.jobs[0].series.is_none());
        Ok(())
    }

    #[test]
    fn a_malformed_series_or_history_entry_is_dropped_not_fatal() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("board.json");
        std::fs::write(
            &path,
            r#"{"jobs":[{"id":"001","dir":"d","slug":"s","created":"2026-10-13","feature":"f","branch":"b","stage":"build","round":0,"stage_since":5.0,"reviews":{},"series":{"index":"two"},"stage_history":[{"stage":"spec","ts":1.0},"junk",{"stage":"build"}]}]}"#,
        )?;
        let (board, error) = load_board(&path);
        assert!(error.is_none(), "got: {error:?}");
        assert!(board.jobs[0].series.is_none());
        assert_eq!(board.jobs[0].stage_history.len(), 1);
        Ok(())
    }

    #[test]
    fn a_valid_series_loads_with_its_numbers() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("board.json");
        std::fs::write(
            &path,
            r#"{"jobs":[{"id":"001","dir":"d","slug":"s","created":"2026-10-13","feature":"f","branch":"b","stage":"build","round":0,"stage_since":5.0,"reviews":{},"series":{"id":"s1","index":2,"total":4,"next_plan":"p.md"}}]}"#,
        )?;
        let (board, _) = load_board(&path);
        let series = board.jobs[0].series.as_ref().ok_or("no series")?;
        assert_eq!((series.index, series.total), (2, 4));
        Ok(())
    }

    fn entry(stage: &str, ts: f64) -> StageEntry {
        StageEntry {
            stage: stage.to_string(),
            ts,
        }
    }

    #[test]
    fn durations_sum_repeated_stages_and_run_the_current_one_to_now() {
        let mut item = job("001", "review", 400.0);
        item.stage_history = vec![
            entry("spec", 0.0),
            entry("build", 100.0),
            entry("review", 300.0),
            entry("build", 350.0),
            entry("review", 400.0),
        ];
        let durations = stage_durations(&item, 460.0);
        let get = |name: &str| durations.iter().find(|d| d.stage == name);
        assert_eq!(get("spec").map(|d| d.seconds), Some(100.0));
        assert_eq!(get("build").map(|d| d.seconds), Some(250.0));
        assert_eq!(get("review").map(|d| (d.seconds, d.current)), Some((110.0, true)));
    }

    #[test]
    fn a_merged_job_shows_no_growing_duration_for_merged() {
        let mut item = job("001", "merged", 300.0);
        item.stage_history = vec![entry("spec", 0.0), entry("build", 100.0), entry("merged", 300.0)];
        let durations = stage_durations(&item, 99_999.0);
        assert!(durations.iter().all(|d| d.stage != "merged"));
        assert_eq!(durations.len(), 2);
    }

    #[test]
    fn a_job_without_history_falls_back_to_stage_since() {
        let item = job("001", "build", 100.0);
        let durations = stage_durations(&item, 160.0);
        assert_eq!(durations.len(), 1);
        assert_eq!(durations[0].stage, "build");
        assert_eq!(durations[0].seconds, 60.0);
    }

    #[test]
    fn a_clock_step_backward_never_gives_a_negative_duration() {
        let mut item = job("001", "build", 100.0);
        item.stage_history = vec![entry("spec", 200.0), entry("build", 100.0)];
        assert!(stage_durations(&item, 150.0).iter().all(|d| d.seconds >= 0.0));
    }
```

- [ ] **Step 7: Run to verify failure**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml 2>&1 | tail -20`
Expected: compile errors (`StageEntry`, `stage_durations`, fields not found).

- [ ] **Step 8: Implement model and durations**

In `model.rs`, change the first import to `use serde::{Deserialize, Deserializer, Serialize};` and add after `Event`:

```rust
/// One stage change in a job's history.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct StageEntry {
    pub stage: String,
    pub ts: f64,
}

/// Where a job sits in a multi-plan series. Plan 04 writes this on the board.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Series {
    pub index: u32,
    pub total: u32,
    #[serde(default)]
    pub id: serde_json::Value,
    #[serde(default)]
    pub next_plan: serde_json::Value,
}

/// A bad series must not make the whole board unreadable, so it becomes none.
fn lenient_series<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Series>, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(d)?;
    Ok(value.and_then(|v| serde_json::from_value(v).ok()))
}

/// Bad history entries are dropped one by one for the same reason.
fn lenient_history<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<StageEntry>, D::Error> {
    let values = Vec::<serde_json::Value>::deserialize(d)?;
    Ok(values
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
}
```

In `Job`, after `pub reviews: BTreeMap<String, String>,` add:

```rust
    #[serde(default, deserialize_with = "lenient_history")]
    pub stage_history: Vec<StageEntry>,
    #[serde(default, deserialize_with = "lenient_series")]
    pub series: Option<Series>,
```

In `state.rs`: change the import to `use crate::model::{AgentState, Board, Job, RecentEvent, StageEntry};`, add to `JobView` the field `pub stage_durations: Vec<StageDuration>,`, add to `StateView` the field `pub skipped_lines: u64,`, and add above `load_board`:

```rust
#[derive(Debug, Serialize, PartialEq)]
pub struct StageDuration {
    pub stage: String,
    pub seconds: f64,
    /// True for the stage the job is in now.
    pub current: bool,
}

fn is_terminal(stage: &str) -> bool {
    matches!(stage, "merged" | "needs-human")
}

/// Time spent per stage, summed over repeats. A terminal stage that is still current shows nothing,
/// because its time would only grow.
pub fn stage_durations(job: &Job, now: f64) -> Vec<StageDuration> {
    let history: Vec<StageEntry> = if job.stage_history.is_empty() {
        vec![StageEntry {
            stage: job.stage.clone(),
            ts: job.stage_since,
        }]
    } else {
        job.stage_history.clone()
    };
    let mut out: Vec<StageDuration> = Vec::new();
    for (i, entry) in history.iter().enumerate() {
        let current = i + 1 == history.len();
        if current && is_terminal(&entry.stage) {
            continue;
        }
        let end = if current { now } else { history[i + 1].ts };
        let seconds = (end - entry.ts).max(0.0);
        match out.iter_mut().find(|d| d.stage == entry.stage) {
            Some(existing) => {
                existing.seconds += seconds;
                existing.current |= current;
            }
            None => out.push(StageDuration {
                stage: entry.stage.clone(),
                seconds,
                current,
            }),
        }
    }
    out
}
```

In `build_state`, add `stage_durations: stage_durations(job, now),` to the `JobView` literal and `skipped_lines: registry.skipped_lines,` to the `StateView` literal.

- [ ] **Step 9: Run to verify pass**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml 2>&1 | tail -15`
Expected: all tests pass.

- [ ] **Step 10: Commit**

Invoke skill `dev-workflow-skills:1-git-commit`, then:

```bash
git add tools/factory-server/src/model.rs tools/factory-server/src/state.rs && git commit --only -m "feat(factory): read stage history and series, derive stage durations" -- tools/factory-server/src/model.rs tools/factory-server/src/state.rs
```

#### Part C: attribution, stale time and skipped lines

- [ ] **Step 11: Write the failing tests**

In `state.rs` tests, add:

```rust
    #[test]
    fn two_jobs_in_one_stage_do_not_guess_an_agent_onto_the_first_card() {
        let mut board = Board::default();
        board.jobs.push(job("001", "review", 10.0));
        board.jobs.push(job("002", "review", 10.0));
        let mut registry = Registry::new();
        start(&mut registry, 20.0, "r1", "factory-code-reviewer", "/elsewhere");
        let view = build_state(&board, &registry, 30.0, None);
        assert!(view.jobs[0].agents.is_empty());
        assert!(view.jobs[1].agents.is_empty());
        assert_eq!(view.unassigned_agents.len(), 1);
    }

    #[test]
    fn a_unique_stage_match_still_attaches_the_spec_writer() {
        let mut board = Board::default();
        board.jobs.push(job("001", "build", 10.0));
        board.jobs.push(job("002", "spec", 10.0));
        let mut registry = Registry::new();
        start(&mut registry, 20.0, "s1", "factory-spec-writer", "/repo");
        let view = build_state(&board, &registry, 30.0, None);
        assert!(view.jobs[0].agents.is_empty());
        assert_eq!(view.jobs[1].agents.len(), 1);
    }

    #[test]
    fn the_worktree_still_wins_when_two_jobs_share_a_stage() {
        let mut board = Board::default();
        board.jobs.push(job("001", "build", 10.0));
        let mut second = job("002", "build", 10.0);
        second.worktree = Some("/wt/two".to_string());
        board.jobs.push(second);
        let mut registry = Registry::new();
        start(&mut registry, 20.0, "b1", "factory-builder", "/wt/two/crates");
        let view = build_state(&board, &registry, 30.0, None);
        assert!(view.jobs[0].agents.is_empty());
        assert_eq!(view.jobs[1].agents.len(), 1);
    }

    #[test]
    fn skipped_lines_are_reported_in_the_state() -> TestResult {
        let mut registry = Registry::new();
        registry.ingest_line("garbage");
        registry.ingest_line("more garbage");
        let view = build_state(&Board::default(), &registry, 1.0, None);
        assert_eq!(serde_json::to_value(&view)?["skipped_lines"], 2);
        Ok(())
    }

    #[test]
    fn an_agent_silent_for_ten_minutes_is_stale() {
        let mut board = Board::default();
        board.jobs.push(job("001", "build", 10.0));
        let mut registry = Registry::new();
        start(&mut registry, 20.0, "b1", "factory-builder", "/repo");
        let quiet = build_state(&board, &registry, 20.0 + 599.0, None);
        assert_eq!(quiet.jobs[0].agents[0].status, "running");
        let stale = build_state(&board, &registry, 20.0 + 601.0, None);
        assert_eq!(stale.jobs[0].agents[0].status, "stale");
    }
```

Also in the existing test `a_tool_event_keeps_an_agent_running`, change `build_state(&board, &registry, 2000.0, None)` to `build_state(&board, &registry, 1300.0, None)` (the tool event is at 1000, so it is still inside the new 600 second window).

- [ ] **Step 12: Run to verify failure**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml 2>&1 | tail -20`
Expected: FAIL on the first and last new tests (the stage fallback and the 1800 second constant).

- [ ] **Step 13: Implement**

In `state.rs`: change `const STALE_SECS: f64 = 1800.0;` to `const STALE_SECS: f64 = 600.0;` (an interrupted resumed builder gets no stop event, so silence is the signal). Replace the `.or_else(|| jobs.iter().position(|view| fits_stage(&view.job, &agent)))` line with `.or_else(|| unique_stage_match(&jobs, &agent))` and add above `keep_unassigned`:

```rust
/// A stage match is only trusted when exactly one job fits. With several, the agent stays
/// unassigned rather than landing on the wrong card.
fn unique_stage_match(jobs: &[JobView], agent: &AgentState) -> Option<usize> {
    let mut found = None;
    for (i, view) in jobs.iter().enumerate() {
        if fits_stage(&view.job, agent) {
            if found.is_some() {
                return None;
            }
            found = Some(i);
        }
    }
    found
}
```

Series needs no extra attribution rule: the plans of one series run one at a time, each in its own worktree, so the worktree match above already separates them.

- [ ] **Step 14: Run to verify pass**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml 2>&1 | tail -15`
Expected: all pass.

- [ ] **Step 15: Commit**

Invoke skill `dev-workflow-skills:1-git-commit`, then:

```bash
git add tools/factory-server/src/state.rs && git commit --only -m "fix(factory): attribute agents only on a unique match and mark silent agents stale" -- tools/factory-server/src/state.rs
```

#### Part D: bounded log reader and CSP

- [ ] **Step 16: Write the failing tests**

In `events.rs` tests, add (the module already imports `std::io::Write` and `TestResult`):

```rust
    fn numbered_lines(count: usize) -> String {
        (0..count).map(|k| format!("{{\"n\":{}}}\n", 100 + k)).collect()
    }

    #[test]
    fn a_windowed_tailer_reads_only_the_end_and_drops_the_cut_line() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("events.jsonl");
        std::fs::write(&path, numbered_lines(100))?;
        let mut tailer = Tailer::with_window(path, 55);
        let lines = tailer.poll();
        assert_eq!(lines.len(), 5);
        assert_eq!(lines[0], "{\"n\":195}");
        assert_eq!(lines[4], "{\"n\":199}");
        Ok(())
    }

    #[test]
    fn a_windowed_read_does_not_count_the_cut_line_as_skipped() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("events.jsonl");
        let mut text = String::new();
        for k in 0..200 {
            text.push_str(&format!(
                "{{\"ts\":{k}.0,\"hook_event_name\":\"SubagentStart\",\"agent_id\":\"a{k}\",\"agent_type\":\"t\"}}\n"
            ));
        }
        std::fs::write(&path, text)?;
        let registry = Mutex::new(Registry::new());
        let mut tailer = Tailer::with_window(path, 500);
        pump(&mut tailer, &registry, 200.0);
        let guard = registry.lock().map_err(|_| "poisoned")?;
        assert_eq!(guard.skipped_lines, 0);
        assert!(guard.agents().len() < 10);
        assert!(guard.agents().iter().any(|a| a.agent_id == "a199"));
        Ok(())
    }

    #[test]
    fn a_log_smaller_than_the_window_is_read_whole() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("events.jsonl");
        std::fs::write(&path, numbered_lines(10))?;
        let mut tailer = Tailer::with_window(path, 1_000_000);
        assert_eq!(tailer.poll().len(), 10);
        Ok(())
    }

    #[test]
    fn rotation_is_flagged_for_a_truncated_or_deleted_file_but_not_at_startup() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("events.jsonl");
        let mut tailer = Tailer::new(path.clone());
        tailer.poll();
        assert!(!tailer.take_rotated(), "a missing file at startup is not rotation");
        std::fs::write(&path, numbered_lines(10))?;
        tailer.poll();
        assert!(!tailer.take_rotated(), "the first read is not rotation");
        std::fs::write(&path, numbered_lines(2))?;
        tailer.poll();
        assert!(tailer.take_rotated(), "a shorter file is rotation");
        assert!(!tailer.take_rotated(), "the flag clears once taken");
        std::fs::remove_file(&path)?;
        tailer.poll();
        assert!(tailer.take_rotated(), "a deleted file is rotation");
        Ok(())
    }

    #[test]
    fn pump_resets_the_registry_when_the_log_is_replaced() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("events.jsonl");
        std::fs::write(&path, format!("{START}\n"))?;
        let registry = Mutex::new(Registry::new());
        let mut tailer = Tailer::new(path.clone());
        pump(&mut tailer, &registry, 101.0);
        assert_eq!(registry.lock().map_err(|_| "poisoned")?.agents().len(), 1);
        std::fs::write(
            &path,
            "{\"ts\":1.0,\"hook_event_name\":\"SubagentStart\",\"agent_id\":\"z\",\"agent_type\":\"t\"}\n",
        )?;
        pump(&mut tailer, &registry, 2.0);
        let guard = registry.lock().map_err(|_| "poisoned")?;
        let ids: Vec<String> = guard.agents().iter().map(|a| a.agent_id.clone()).collect();
        assert_eq!(ids, vec!["z".to_string()]);
        Ok(())
    }

    #[test]
    fn prune_drops_agents_idle_for_a_day() {
        let mut registry = Registry::new();
        registry.ingest_line(START);
        registry.prune(100.0 + 86_000.0);
        assert_eq!(registry.agents().len(), 1);
        registry.prune(100.0 + 86_401.0);
        assert!(registry.agents().is_empty());
    }

    #[test]
    fn prune_caps_the_agent_map_and_drops_the_oldest_first() {
        let mut registry = Registry::new();
        for k in 0..600 {
            registry.ingest_line(&format!(
                "{{\"ts\":{k}.0,\"hook_event_name\":\"SubagentStart\",\"agent_id\":\"a{k}\",\"agent_type\":\"t\"}}"
            ));
        }
        registry.prune(700.0);
        let agents = registry.agents();
        assert_eq!(agents.len(), 500);
        assert!(agents.iter().all(|a| a.agent_id != "a0"));
        assert!(agents.iter().any(|a| a.agent_id == "a599"));
    }
```

In `server.rs` tests, add:

```rust
    #[test]
    fn the_csp_blocks_base_tags_and_form_posts() {
        assert!(CSP.contains("base-uri 'none'"));
        assert!(CSP.contains("form-action 'none'"));
        assert!(CSP.contains("default-src 'self'"));
        assert!(CSP.contains("frame-ancestors 'none'"));
    }
```
and in the existing `replies_carry_security_headers` test append before `Ok(())`:

```rust
        assert!(reply.contains("base-uri 'none'"), "got: {reply}");
        assert!(reply.contains("form-action 'none'"), "got: {reply}");
```

- [ ] **Step 17: Run to verify failure**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml 2>&1 | tail -20`
Expected: compile errors (`with_window`, `take_rotated`, `pump`, `prune`, `CSP`).

- [ ] **Step 18: Implement**

In `events.rs`: add `use std::sync::Mutex;` to the imports, and below `REOPEN_GRACE_SECS` add:

```rust
/// Agents with no event for this long are dropped from the map.
const PRUNE_AFTER_SECS: f64 = 86_400.0;
const MAX_AGENTS: usize = 500;
/// At startup only this much of the end of the log is read.
pub const LOG_WINDOW_BYTES: u64 = 4 * 1024 * 1024;

fn last_activity(agent: &AgentState) -> f64 {
    agent
        .ended_ts
        .unwrap_or(0.0)
        .max(agent.last_tool_ts.unwrap_or(0.0))
        .max(agent.started_ts)
}
```

Add these methods to `impl Registry` (after `recent`):

```rust
    /// Forgets everything, for a log that was replaced.
    pub fn reset(&mut self) {
        self.agents.clear();
        self.recent.clear();
        self.skipped_lines = 0;
    }

    /// Drops agents idle for a day, then the oldest ones beyond the cap.
    pub fn prune(&mut self, now: f64) {
        self.agents
            .retain(|_, agent| now - last_activity(agent) <= PRUNE_AFTER_SECS);
        if self.agents.len() <= MAX_AGENTS {
            return;
        }
        let mut order: Vec<(f64, String)> = self
            .agents
            .values()
            .map(|agent| (last_activity(agent), agent.agent_id.clone()))
            .collect();
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        let excess = self.agents.len() - MAX_AGENTS;
        for (_, id) in order.into_iter().take(excess) {
            self.agents.remove(&id);
        }
    }
```

Replace the `Tailer` struct and its `impl` with this (the `poll` doc comment and body keep their meaning; the new parts are the window, `fresh`, `skip_partial` and `rotated`):

```rust
/// Reads new complete lines from a growing file.
#[derive(Debug)]
pub struct Tailer {
    path: PathBuf,
    offset: u64,
    partial: String,
    /// Device and inode of the file read last, to notice a replaced file.
    ident: Option<(u64, u64)>,
    /// How much of the end of a file the first read may take.
    window: u64,
    /// True until the first read of a file, when the window is applied.
    fresh: bool,
    /// True while the first, cut line of a windowed read is still to be dropped.
    skip_partial: bool,
    rotated: bool,
}

impl Tailer {
    pub fn new(path: PathBuf) -> Self {
        Self::with_window(path, u64::MAX)
    }

    pub fn with_window(path: PathBuf, window: u64) -> Self {
        Self {
            path,
            offset: 0,
            partial: String::new(),
            ident: None,
            window,
            fresh: true,
            skip_partial: false,
            rotated: false,
        }
    }

    /// True once after the file was truncated, replaced or deleted. The first read of a file is not rotation.
    pub fn take_rotated(&mut self) -> bool {
        std::mem::take(&mut self.rotated)
    }

    fn restart(&mut self) {
        self.offset = 0;
        self.partial.clear();
        self.fresh = true;
        self.skip_partial = false;
    }

    /// Returns the complete lines added since the last call. A missing file gives no lines.
    /// A shorter file than before is treated as a new file and read from the start.
    pub fn poll(&mut self) -> Vec<String> {
        let Ok(mut file) = File::open(&self.path) else {
            // A deleted file starts fresh when it comes back.
            if self.ident.is_some() {
                self.rotated = true;
            }
            self.restart();
            self.ident = None;
            return Vec::new();
        };
        let Ok(meta) = file.metadata() else {
            return Vec::new();
        };
        let ident = (meta.dev(), meta.ino());
        if self.ident.is_some_and(|old| old != ident) {
            self.rotated = true;
            self.restart();
        }
        self.ident = Some(ident);
        let len = meta.len();
        if len < self.offset {
            self.rotated = true;
            self.restart();
        }
        if self.fresh {
            self.fresh = false;
            if len > self.window {
                self.offset = len - self.window;
                self.skip_partial = true;
            }
        }
        if len == self.offset {
            return Vec::new();
        }
        if file.seek(SeekFrom::Start(self.offset)).is_err() {
            return Vec::new();
        }
        let mut bytes = Vec::new();
        if file.read_to_end(&mut bytes).is_err() {
            return Vec::new();
        }
        self.offset += bytes.len() as u64;
        self.partial.push_str(&String::from_utf8_lossy(&bytes));
        if self.skip_partial {
            match self.partial.find('\n') {
                Some(idx) => {
                    self.partial.drain(..=idx);
                    self.skip_partial = false;
                }
                None => {
                    self.partial.clear();
                    return Vec::new();
                }
            }
        }
        let Some(idx) = self.partial.rfind('\n') else {
            return Vec::new();
        };
        let rest = self.partial.split_off(idx + 1);
        let complete = std::mem::replace(&mut self.partial, rest);
        let mut lines = Vec::new();
        for line in complete.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                lines.push(trimmed.to_string());
            }
        }
        lines
    }
}

/// One tailer step: read new lines, reset the registry on rotation, ingest, then prune.
pub fn pump(tailer: &mut Tailer, registry: &Mutex<Registry>, now: f64) {
    let lines = tailer.poll();
    let rotated = tailer.take_rotated();
    if let Ok(mut guard) = registry.lock() {
        if rotated {
            guard.reset();
        }
        for line in &lines {
            guard.ingest_line(line);
        }
        guard.prune(now);
    }
}
```

In `server.rs`: change the events import to `use crate::events::{pump, Registry, Tailer, LOG_WINDOW_BYTES};`, add below `POLL_INTERVAL`:

```rust
/// Content security policy for every reply.
pub const CSP: &str =
    "default-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";
```
In `serve`, replace the byte string `&b"default-src 'self'; frame-ancestors 'none'"[..]` with `CSP.as_bytes()`. Replace the body of the `std::thread::spawn` closure in `spawn_tailer` with:

```rust
        let mut tailer = Tailer::with_window(path, LOG_WINDOW_BYTES);
        loop {
            pump(&mut tailer, &app.registry, now_secs());
            std::thread::sleep(POLL_INTERVAL);
        }
```

- [ ] **Step 19: Run to verify pass**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml 2>&1 | tail -15`
Expected: all pass. Then: `cargo check -j4 --manifest-path tools/factory-server/Cargo.toml` with no warnings.

- [ ] **Step 20: Commit**

Invoke skill `dev-workflow-skills:1-git-commit`, then:

```bash
git add tools/factory-server/src/events.rs tools/factory-server/src/server.rs && git commit --only -m "feat(factory): bound the log reader, reset on rotation and tighten the CSP" -- tools/factory-server/src/events.rs tools/factory-server/src/server.rs
```

---

### Task 3: Page fixes, series chip, copy chips and docs

Covers E3 (display), E5 (display), E7, E8, D5 (display) and the doc sync.

**Files:**
- Create: `tools/factory-server/static/patch.js`
- Modify: `tools/factory-server/static/board.js`, `app.js`, `dashboard.html`, `dashboard.css`
- Modify: `tools/factory-server/src/server.rs` (serve `/patch.js`)
- Modify: `tools/factory-server/tests/board.test.js`, `tests/smoke.sh`
- Modify: `tools/factory-server/README.md`, `.claude/software-factory.md`

**Interfaces:**
- Consumes: API fields from Task 2: `skipped_lines`, `jobs[].series`, `jobs[].stage_durations`.
- Produces: `FactoryBoard.copyChip(command)`, `FactoryBoard.seriesChip(series)`, `FactoryBoard.durationsLine(job)`, `FactoryBoard.announcement(header)`; `FactoryPatch.patchChildren(oldParent, newParent)`.

- [ ] **Step 1: Write the failing board tests**

Append to `tools/factory-server/tests/board.test.js`:

```js
test("renderCard hides time in stage for merged and needs-human cards", () => {
  assert.match(board.renderCard(makeJob({ stage: "build" })), /in stage/);
  assert.doesNotMatch(board.renderCard(makeJob({ stage: "merged" })), /in stage/);
  assert.doesNotMatch(board.renderCard(makeJob({ stage: "needs-human", reason: "x" })), /in stage/);
});

test("durationsLine shows per-stage times and the current stage", () => {
  const html = board.durationsLine(
    makeJob({
      stage_durations: [
        { stage: "spec", seconds: 120, current: false },
        { stage: "build", seconds: 1860, current: true },
      ],
    }),
  );
  assert.match(html, /Spec 2m/);
  assert.match(html, /Build 31m/);
  assert.match(html, /duration-now/);
});

test("durationsLine ignores bad entries and shows nothing without data", () => {
  assert.equal(board.durationsLine(makeJob({})), "");
  const html = board.durationsLine(
    makeJob({
      stage_durations: [
        { stage: "<script>", seconds: 5 },
        { stage: "build", seconds: Number.NaN },
        { stage: "review", seconds: 60 },
      ],
    }),
  );
  assert.doesNotMatch(html, /script|NaN/);
  assert.match(html, /Review 1m/);
});

test("seriesChip shows plan k of N and hides for a single plan or bad numbers", () => {
  assert.match(board.seriesChip({ index: 2, total: 4 }), />plan 2 of 4</);
  assert.equal(board.seriesChip({ index: 1, total: 1 }), "");
  assert.equal(board.seriesChip(null), "");
  assert.equal(board.seriesChip({ index: "x", total: "y" }), "");
  assert.match(board.seriesChip({ index: 9, total: 4 }), />plan 4 of 4</);
  assert.match(board.seriesChip({ index: 0, total: 4 }), />plan 1 of 4</);
});

test("renderCard puts the series chip in the card head", () => {
  const html = board.renderCard(makeJob({ series: { index: 2, total: 3 } }));
  assert.match(html, /plan 2 of 3/);
  assert.doesNotMatch(board.renderCard(makeJob({})), /class="series"/);
});

test("copyChip is a button that carries the escaped command", () => {
  const html = board.copyChip('/factory rework 003 <note> "x"');
  assert.match(html, /^<button type="button" class="cmd" data-copy="/);
  assert.match(html, /data-copy="\/factory rework 003 &lt;note&gt; &quot;x&quot;"/);
  assert.doesNotMatch(html, /<note>/);
});

test("needs-human cards and the summary use copy chips", () => {
  const card = board.renderCard(makeJob({ id: "003", stage: "needs-human", reason: "r" }));
  assert.match(card, /data-copy="\/factory approve 003"/);
  assert.match(card, /data-copy="\/factory rework 003 &lt;note&gt;"/);
  const summary = board.renderSummary(makeState([makeJob({ id: "004", stage: "needs-human", reason: "r" })]));
  assert.match(summary, /data-copy="\/factory approve 004"/);
});

test("renderFeed reports skipped log lines only when there are some", () => {
  assert.doesNotMatch(board.renderFeed(makeState([])), /skipped/);
  assert.match(board.renderFeed(makeState([], { skipped_lines: 3 })), /3 event log lines skipped/);
  assert.match(board.renderFeed(makeState([], { skipped_lines: 1, recent: [{ ts: 1, kind: "tool" }] })), /1 event log line skipped/);
});

test("announcement is short and changes only with the level", () => {
  assert.equal(board.announcement({ level: "ok", text: "Live, updated 10:00:01" }), "Dashboard is live");
  assert.equal(board.announcement({ level: "error", text: "Server unreachable" }), "Server unreachable");
  assert.equal(board.announcement({ level: "warn", text: "board.json is invalid: x" }), "board.json is invalid: x");
});
```

Run: `node --test tools/factory-server/tests/board.test.js`
Expected: FAIL (the new functions are missing and `<code>` is still used).

- [ ] **Step 2: Implement the renderers in `board.js`**

Add after `const FEED_LIMIT = 15;`:

```js
  // Plan 04 writes the series index. Set this to 0 if its index is zero-based.
  const SERIES_INDEX_BASE = 1;
```

Add these functions before `renderCard`:

```js
  function copyChip(command) {
    const safe = escapeHtml(command);
    return `<button type="button" class="cmd" data-copy="${safe}" title="Copy to the clipboard">${safe}</button>`;
  }

  function seriesChip(series) {
    if (!series) return "";
    const total = Number(series.total);
    const index = Number(series.index);
    if (!Number.isFinite(total) || !Number.isFinite(index) || total < 2) return "";
    const k = Math.min(Math.max(index - SERIES_INDEX_BASE + 1, 1), total);
    return `<span class="series" title="Plan ${k} of ${total} in this series">plan ${k} of ${total}</span>`;
  }

  function durationsLine(job) {
    const known = (job.stage_durations || []).filter(
      (item) => item && STAGES.some((s) => s.key === item.stage) && Number.isFinite(item.seconds),
    );
    if (!known.length) return "";
    const parts = known.map((item) => {
      const title = STAGES.find((s) => s.key === item.stage).title;
      return `<span class="duration${item.current ? " duration-now" : ""}">${title} ${formatAge(item.seconds)}</span>`;
    });
    return `<p class="durations" title="Time spent in each stage">${parts.join("")}</p>`;
  }

  function announcement(header) {
    if (header.level === "ok") return "Dashboard is live";
    return header.text;
  }
```

In `renderCard`: replace the `reason` block with

```js
    const reason =
      job.stage === "needs-human"
        ? `<p class="reason">${escapeHtml(job.reason || "needs a human")}</p>` +
          `<p class="hint">${copyChip(`/factory approve ${job.id}`)}${copyChip(`/factory rework ${job.id} <note>`)}</p>`
        : "";
    const showAge = job.stage !== "merged" && job.stage !== "needs-human";
```
In the card head add `seriesChip(job.series) +` right after the `card-stage` span (before the `<time>`), replace the age span with `(showAge ? `<span class="age">${formatAge(job.seconds_in_stage)} in stage</span>` : "") +`, and add `durationsLine(job) +` right after the `</div>` of `.meta` (before `chips`).

In `summaryJob`, replace the `summary-actions` span with:

```js
      `<span class="summary-actions">${copyChip(`/factory approve ${job.id}`)}` +
      `${copyChip(`/factory rework ${job.id} <note>`)}</span></li>`
```
and remove the now unused `const id = escapeHtml(job.id);` there only if `id` is unused; it is still used by the job-id span, so keep it.

In `renderFeed`, compute `const skipped = Number(state.skipped_lines) || 0;` and `const note = skipped ? `<p class="warning">${skipped} event log ${plural(skipped, "line", "lines")} skipped</p>` : "";`, append `${note}` before the closing `</section>` in both returned strings. `plural` is defined later in the file as a function declaration, so it is available.

Export `copyChip`, `seriesChip`, `durationsLine`, `announcement` in the `api` object.

Run: `node --test tools/factory-server/tests/board.test.js`
Expected: all pass.

- [ ] **Step 3: Write the failing DOM patch tests**

Create `tools/factory-server/tests/patch.test.js`:

```js
"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const patch = require("../static/patch.js");

let writes = [];

class TextNode {
  constructor(value) {
    this.nodeType = 3;
    this.nodeName = "#text";
    this.value = value;
  }
  get nodeValue() {
    return this.value;
  }
  set nodeValue(next) {
    writes.push(`text:${next}`);
    this.value = next;
  }
}

class ElementNode {
  constructor(tag, attrs, kids) {
    this.nodeType = 1;
    this.nodeName = tag.toUpperCase();
    this.attrs = new Map(Object.entries(attrs || {}));
    this.childNodes = (kids || []).slice();
  }
  get attributes() {
    return [...this.attrs].map(([name, value]) => ({ name, value }));
  }
  getAttribute(name) {
    return this.attrs.has(name) ? this.attrs.get(name) : null;
  }
  setAttribute(name, value) {
    writes.push(`attr:${name}=${value}`);
    this.attrs.set(name, String(value));
  }
  removeAttribute(name) {
    writes.push(`unattr:${name}`);
    this.attrs.delete(name);
  }
  appendChild(node) {
    writes.push("append");
    this.childNodes.push(node);
  }
  removeChild(node) {
    writes.push("remove");
    this.childNodes.splice(this.childNodes.indexOf(node), 1);
  }
  replaceChild(next, old) {
    writes.push("replace");
    this.childNodes[this.childNodes.indexOf(old)] = next;
  }
}

const el = (tag, attrs, kids) => new ElementNode(tag, attrs, kids);
const txt = (value) => new TextNode(value);

function run(oldTree, newTree) {
  writes = [];
  patch.patchChildren(oldTree, newTree);
  return writes;
}

test("identical trees cause no writes", () => {
  const make = () => el("div", {}, [el("p", { class: "a" }, [txt("hello")]), txt("x")]);
  assert.deepEqual(run(make(), make()), []);
});

test("a changed text node is the only write", () => {
  const live = el("div", {}, [el("p", { class: "a" }, [txt("1m in stage")])]);
  const next = el("div", {}, [el("p", { class: "a" }, [txt("2m in stage")])]);
  assert.deepEqual(run(live, next), ["text:2m in stage"]);
  assert.equal(live.childNodes[0].childNodes[0].nodeValue, "2m in stage");
});

test("a changed or removed attribute is patched in place", () => {
  const live = el("div", {}, [el("p", { class: "a", title: "t" }, [])]);
  const next = el("div", {}, [el("p", { class: "b" }, [])]);
  assert.deepEqual(run(live, next), ["attr:class=b", "unattr:title"]);
});

test("extra new children are appended and extra old children removed", () => {
  const live = el("div", {}, [el("p", {}, [])]);
  assert.deepEqual(run(live, el("div", {}, [el("p", {}, []), el("p", {}, [])])), ["append"]);
  assert.deepEqual(run(live, el("div", {}, [])), ["remove", "remove"]);
  assert.equal(live.childNodes.length, 0);
});

test("a different tag replaces the node", () => {
  const live = el("div", {}, [el("p", {}, [txt("a")])]);
  assert.deepEqual(run(live, el("div", {}, [el("span", {}, [txt("a")])])), ["replace"]);
});

test("the same element object is kept when only a deep text changes", () => {
  const button = el("button", { "data-copy": "x" }, [txt("x")]);
  const live = el("div", {}, [el("article", {}, [el("p", {}, [txt("old")]), button])]);
  const next = el("div", {}, [el("article", {}, [el("p", {}, [txt("new")]), el("button", { "data-copy": "x" }, [txt("x")])])]);
  run(live, next);
  assert.equal(live.childNodes[0].childNodes[1], button);
});
```

Run: `node --test tools/factory-server/tests/patch.test.js`
Expected: FAIL (`Cannot find module '../static/patch.js'`).

- [ ] **Step 4: Implement `patch.js`**

Create `tools/factory-server/static/patch.js`:

```js
"use strict";

// Morphs a live DOM subtree into a freshly rendered one and writes only what differs.
(function (root) {
  function syncAttributes(live, next) {
    const wanted = Array.from(next.attributes);
    for (const attr of wanted) {
      if (live.getAttribute(attr.name) !== attr.value) live.setAttribute(attr.name, attr.value);
    }
    const names = new Set(wanted.map((attr) => attr.name));
    for (const attr of Array.from(live.attributes)) {
      if (!names.has(attr.name)) live.removeAttribute(attr.name);
    }
  }

  function patchNode(parent, live, next) {
    if (live.nodeType !== next.nodeType || live.nodeName !== next.nodeName) {
      parent.replaceChild(next, live);
      return;
    }
    if (live.nodeType === 3) {
      if (live.nodeValue !== next.nodeValue) live.nodeValue = next.nodeValue;
      return;
    }
    if (live.nodeType === 1) {
      syncAttributes(live, next);
      patchChildren(live, next);
    }
  }

  function patchChildren(liveParent, nextParent) {
    const live = Array.from(liveParent.childNodes);
    const next = Array.from(nextParent.childNodes);
    for (let i = 0; i < next.length; i++) {
      if (i < live.length) patchNode(liveParent, live[i], next[i]);
      else liveParent.appendChild(next[i]);
    }
    for (let i = live.length - 1; i >= next.length; i--) liveParent.removeChild(live[i]);
  }

  const api = { patchChildren };
  if (typeof module !== "undefined" && module.exports) {
    module.exports = api;
  } else {
    root.FactoryPatch = api;
  }
})(typeof window !== "undefined" ? window : globalThis);
```

Run: `node --test tools/factory-server/tests/patch.test.js`
Expected: all pass.

- [ ] **Step 5: Serve `patch.js` (failing test first)**

In `server.rs` tests, extend `static_assets_are_served_with_the_right_content_type` before `let page = ...`:

```rust
        let patch = route(&app, "GET", "/patch.js", host, 1, 5.0);
        assert_eq!(patch.status, 200);
        assert_eq!(patch.content_type, "text/javascript; charset=utf-8");
```
Run `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml static_assets` and expect FAIL (404). Then add `const PATCH_JS: &str = include_str!("../static/patch.js");` next to the other consts and the route `"/patch.js" => asset("text/javascript; charset=utf-8", PATCH_JS),` next to `"/app.js"`. Re-run: expect PASS.

- [ ] **Step 6: Wire the page**

`dashboard.html`:
- Replace the status paragraph with a non-live one plus one separate polite region that changes only when the level changes:

```html
      <p id="status" class="status status-warn">
        <span class="dot" aria-hidden="true"></span><span id="status-text">Connecting</span>
      </p>
      <p id="announce" class="sr-only" role="status" aria-live="polite"></p>
```
- Before `</div>` that closes `.page`, add `<p id="toast" class="toast" role="status" hidden></p>`.
- In the Commands panel replace each `<code>X</code>` with `<button type="button" class="cmd" data-copy="X" title="Copy to the clipboard">X</button>` using the same text for X (`/factory &lt;feature&gt;` and the other four, keeping the HTML entities in both places).
- Add `<script src="/patch.js"></script>` before `/board.js`.

`app.js`: replace `setHtml` and `render`, and add the copy handler:

```js
  function setText(el, text) {
    if (el.textContent !== text) el.textContent = text;
  }

  function setHtml(id, html) {
    if (shown[id] === html) return;
    shown[id] = html;
    const template = document.createElement("template");
    template.innerHTML = html;
    FactoryPatch.patchChildren(byId(id), template.content);
  }

  let lastLevel = null;

  function render(state, error) {
    const view = state || lastState;
    if (view) {
      setHtml("summary", FactoryBoard.renderSummary(view));
      setHtml("board", FactoryBoard.renderBoard(view));
      setHtml("others", FactoryBoard.renderUnassigned(view));
      setHtml("feed", FactoryBoard.renderFeed(view));
    }
    const header = FactoryBoard.headerStatus(state, error, lastOk);
    const className = `status status-${header.level}`;
    if (byId("status").className !== className) byId("status").className = className;
    setText(byId("status-text"), header.text);
    if (header.level !== lastLevel) {
      lastLevel = header.level;
      setText(byId("announce"), FactoryBoard.announcement(header));
    }
    byId("board").classList.toggle("is-stale", Boolean(error));
  }

  let toastTimer = null;

  function showToast(message) {
    const toast = byId("toast");
    toast.textContent = message;
    toast.hidden = false;
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toast.hidden = true;
      toast.textContent = "";
    }, 2000);
  }

  function legacyCopy(text) {
    const area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    let ok = false;
    try {
      ok = document.execCommand("copy");
    } catch (error) {
      ok = false;
    }
    area.remove();
    return ok;
  }

  async function copyText(text) {
    let ok = false;
    try {
      await navigator.clipboard.writeText(text);
      ok = true;
    } catch (error) {
      ok = legacyCopy(text);
    }
    showToast(ok ? `Copied ${text}` : "Copy failed. Select the command by hand.");
  }

  document.addEventListener("click", (event) => {
    const source = event.target instanceof Element ? event.target.closest("[data-copy]") : null;
    if (source) copyText(source.getAttribute("data-copy") || "");
  });
```
The toast lives outside the patched regions, so the poll never resets it.

`dashboard.css`: replace the `.hint code` rule with `.hint .cmd { display: block; }`, and append:

```css
.cmd { display: inline-block; max-width: 100%; padding: 1px 6px; border: 1px solid var(--border); border-radius: 3px; background: var(--muted); color: inherit; font: 0.9em var(--font); text-align: left; overflow-wrap: anywhere; cursor: pointer; }
.cmd:hover { border-color: var(--primary); }
.cmd:focus-visible { outline: 2px solid var(--primary); outline-offset: 2px; }
.series { padding: 1px 6px; border: 1px solid var(--border); border-radius: 999px; color: var(--muted-foreground); font-weight: 500; white-space: nowrap; }
.durations { display: flex; flex-wrap: wrap; gap: 0 0.8rem; margin: 0.35rem 0 0; color: var(--muted-foreground); font-size: 12px; }
.duration-now { color: var(--foreground); font-weight: 500; }
.sr-only { position: absolute; width: 1px; height: 1px; margin: -1px; padding: 0; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; border: 0; }
.toast { position: fixed; left: 50%; bottom: 1rem; transform: translateX(-50%); max-width: calc(100% - 2rem); margin: 0; padding: 0.4rem 0.8rem; border: 1px solid var(--border); border-radius: 6px; background: var(--card); color: var(--foreground); font-size: 12px; overflow-wrap: anywhere; }
```
Before writing, check `grep -n "^\s*--foreground\|--card:" tools/factory-server/static/dashboard.css`; if a variable name differs, use the real name from `:root`.

Run: `node --test tools/factory-server/tests/*.test.js` and `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml`
Expected: all pass.

- [ ] **Step 7: Extend the smoke test**

In `tools/factory-server/tests/smoke.sh`, after the check `a stage change shows up on the next poll` add:

```bash
check "the stage history has spec then build" bash -c "curl -fs http://127.0.0.1:$port/api/state | jq -e '.jobs[0].stage_history | map(.stage) == [\"spec\",\"build\"]'"
check "per-stage durations are reported" bash -c "curl -fs http://127.0.0.1:$port/api/state | jq -e '.jobs[0].stage_durations | map(.stage) == [\"spec\",\"build\"]'"
check "the event log is mode 600" test "$(stat -c %a "$FACTORY_EVENT_LOG")" = 600
```
Before the final summary lines, after the garbage-line check, add:

```bash
check "skipped log lines are counted in the state" bash -c "curl -fs http://127.0.0.1:$port/api/state | jq -e '.skipped_lines >= 1'"
check "patch.js is served" bash -c "curl -fs http://127.0.0.1:$port/patch.js | grep -q FactoryPatch"
check "the CSP forbids base and form targets" bash -c "curl -sI http://127.0.0.1:$port/ | grep -i content-security-policy | grep -q \"base-uri 'none'; form-action 'none'\""
```

Run: `bash tools/factory-server/tests/smoke.sh`
Expected: `all passed`.

- [ ] **Step 8: Sync the docs**

`tools/factory-server/README.md`: add `/patch.js` to the assets row; change the `/api/state` row to list `skipped_lines`, `jobs[]` (with `stage_history`, `series`, `stage_durations`), `unassigned_agents[]`, `recent[]`; under "What it reads" add: the server reads only the last 4 MiB of `events.jsonl` at startup, drops agents idle for a day (cap 500), clears its registry when the log is truncated, replaced or deleted (rotate by moving the file), and marks an agent `stale` after 10 minutes without an event; the Tests section lists `node --test tools/factory-server/tests/*.test.js`.

`.claude/software-factory.md`, in "Live tracking dashboard":
- Replace the bullet `- The log is never rotated. ...` with: `- Rotation: the server reads only the last 4 MiB at startup, prunes idle agents and resets when the file is truncated, replaced or deleted. To rotate, move the file. The log is mode 0600 and holds no Bash arguments: a Bash event logs the command name only, and url targets lose userinfo, query and fragment.`
- Replace the `- Card: ...` bullet with: `- Card: job id, optional "plan k of N" chip for a series job, feature text, branch, builder round 0 to 2, four review chips, time in stage (hidden once merged or needs-human), time spent per stage from the board's `stage_history`, and copy-to-clipboard chips for the approve and rework commands.`
- Replace the sentence `Hook events are matched to a job by the cwd of the worktree.` with: `Hook events are matched to a job by builder id or worktree cwd, or by stage when exactly one job is in that stage. Otherwise the agent shows under "Agents without a job". An agent with no event for 10 minutes shows stale, which is how an interrupted resumed builder appears.`
- Add: `- The page patches only changed text nodes, the status line is not a live region, and one short announcement is made when the connection level changes. The CSP has `base-uri 'none'` and `form-action 'none'`. The header shows skipped log lines when there are any.`

Run: `grep -n "never rotated\|Hook events are matched to a job by the cwd" .claude/software-factory.md` and expect no output.

- [ ] **Step 9: Full verification**

Run:
```bash
node --test tools/factory-server/tests/*.test.js
cargo test -j4 --manifest-path tools/factory-server/Cargo.toml
cargo check -j4 --manifest-path tools/factory-server/Cargo.toml
bash .claude/factory/tests/run.sh | tail -3
bash tools/factory-server/tests/smoke.sh | tail -3
```
Expected: every command passes. Record the exact commands and results in the `CHECKS:` block of the handoff.

- [ ] **Step 10: Commit**

Invoke skill `dev-workflow-skills:1-git-commit`, then:

```bash
git add tools/factory-server/static/patch.js tools/factory-server/static/board.js tools/factory-server/static/app.js tools/factory-server/static/dashboard.html tools/factory-server/static/dashboard.css tools/factory-server/src/server.rs tools/factory-server/tests/board.test.js tools/factory-server/tests/patch.test.js tools/factory-server/tests/smoke.sh tools/factory-server/README.md .claude/software-factory.md && git commit --only -m "feat(factory): series chip, stage times, copy chips and minimal DOM patching" -- tools/factory-server/static/patch.js tools/factory-server/static/board.js tools/factory-server/static/app.js tools/factory-server/static/dashboard.html tools/factory-server/static/dashboard.css tools/factory-server/src/server.rs tools/factory-server/tests/board.test.js tools/factory-server/tests/patch.test.js tools/factory-server/tests/smoke.sh tools/factory-server/README.md .claude/software-factory.md
```

---

## Self-Review

- **Spec coverage:** E1 (`c0794226` covered untyped stops; stale at 10 minutes in T2 Part C), E2 (T2 C), E3 (T2 A, B and T3 durations), E4 (T2 D), E5 (T2 B and T3 feed note), E6 (T1), E7 (T3 copy chips), E8 (T3 announce and patch), E9 (T2 D), D5 (T2 B model, T3 chip). Interfaces row 05: `stage_history`, `series`, `skipped_lines`, per-stage durations are all produced. Known limit: the series needs no extra attribution rule, because plans of a series run one at a time in separate worktrees.
- **Placeholders:** none. Edits to existing files quote the anchor text.
- **Type consistency:** `StageEntry`, `Series`, `StageDuration`, `stage_durations`, `pump`, `Tailer::with_window`, `take_rotated`, `Registry::reset` and `prune`, `CSP`, `copyChip`, `seriesChip`, `durationsLine`, `announcement` and `FactoryPatch.patchChildren` have the same names and signatures in every task that uses them.
- **Open check for the builder:** plan 04 defines whether `series.index` is 1-based. If it is 0-based, set `SERIES_INDEX_BASE = 0` in `board.js` and change the `index: 2` fixture expectations in the seriesChip test accordingly.

Next plan: docs/superpowers/plans/2026-10-13-software-factory-v2/plan-06-harness-and-cleanup.md
