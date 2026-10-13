# Software Factory v3 Plan 07: Dashboard for the Rust Driver

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. This plan is executed by a factory-builder subagent, which skips that skill's final whole-branch review, its ledger scripts and its progress file. Steps use checkbox (`- [ ]`) syntax for tracking. **Plans 01 to 06 must be finished first.** Plan 01 fixes the board fields, plan 03 writes `events.jsonl`, plan 06 deletes the Python hook and `factory-ctl`, so this plan must not call either.

**Goal:** Adapt `tools/factory-server/` to the Rust driver: attribute agents by `job_id`, show stage history, per-job tokens with a budget bar, a series chip and copy-to-clipboard command chips, and close the dashboard gaps E1 to E9 that still apply.

**Architecture:** The server stays a read-only Rust process over `board.json` and `events.jsonl`. New board and event fields are parsed leniently, so a bad value is dropped and never becomes `board_error`. The page stays plain DOM; a small `patch.js` morphs only changed nodes. Tests: Rust unit tests, `node --test`, and a Bash smoke test that writes the two files directly (no Python, no `factory-ctl`).

**Tech Stack:** Rust (tiny_http, serde), vanilla JS, Bash with `jq` and `curl`.

**Spec:** `docs/superpowers/specs/2026-10-13-software-factory-v3-rust-driver-design.md` (decisions 9 and 12, gap row E, table row 07). Earlier art ported here: `docs/superpowers/plans/2026-10-13-software-factory-v2/plan-05-dashboard-server-hook.md` (server and page tasks only, the hook tasks are dropped) and gaps E1 to E9 of `docs/superpowers/specs/2026-10-13-software-factory-v2-design.md`. Design doc: `.claude/software-factory.md`.

## Global Constraints

- `cargo` always with `-j4`, never `--workspace` or `--all`. The server crate has its own manifest: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml` and `cargo check -j4 --manifest-path tools/factory-server/Cargo.toml`.
- Never use `.unwrap()` in Rust, tests included (a harness hook blocks it). Tests return `Result` and use `?` or `.ok_or("...")?`.
- Files only under `tools/factory-server/` and `.claude/software-factory.md`. No new libraries, no external requests, plain DOM.
- Every value from `board.json` or the event log goes through `escapeHtml` before it reaches an HTML string. Class names built from data are whitelisted. Status is never shown by colour alone.
- The server never writes `board.json` or `events.jsonl`.
- Stage names are unchanged: `spec`, `build`, `review`, `approve`, `merged`, `needs-human`.
- Board fields read (written by factory-core, plan 01): per job `stage_history` (list of `{"stage": str, "ts": unix seconds}`), `series` (`{"id", "index" 1-based, "total", "next_plan"}`), `tokens` (`{"weighted": u64, "raw": u64}`), optional `budget_weighted` (u64). Event fields read (written by plan 03): the v1 fields plus `job_id` (string) and `tokens` (`{"weighted": u64, "raw": u64}`, usage of that event). **Open check for the builder:** read `tools/factory/crates/factory-core/src/` (Job) and `tools/factory/crates/factory-agent/src/` (EventSink) first. If a field name differs, change only `model.rs` and the fixtures in this plan, keeping the API names below.
- Server API additions: `skipped_lines`, per job `stage_durations`, `token_view`, per agent `job_id` and `tokens_weighted`.
- Comments: short full sentences ending in a period. No emojis.
- Commits: invoke skill `dev-workflow-skills:1-git-commit`, run its git commands inline with no helper agents, then a pathspec commit (`git add <paths> && git commit --only -m "..." -- <paths>`). Never `git add -A` or `git add .`.

## Review Focus

- A board with an old or malformed `stage_history`, `series` or `tokens`, or a `tokens` value that is a string, must load; the bad value is dropped and `board_error` stays empty. Task 1.
- An event with no `job_id` must stay unassigned and must never land on a card by stage or cwd. Two jobs in one stage must not mix their agents. Task 1.
- A multi-megabyte log must not flood the registry, and the cut first line must not count as skipped. Task 1.
- Zero, missing or non-numeric tokens, budget, series or duration values must not show `NaN`, a negative bar or a bar over 100 percent. Task 2.
- A merged or needs-human card must not show a growing time in stage. Task 1 (data) and Task 2 (page).
- The smoke test must not flake on fixed sleeps and must print `server.log` on failure. Task 3.

## File Structure

| File | Change |
|---|---|
| `tools/factory-server/src/model.rs` | Modify: lenient `stage_history`, `series`, `tokens`, `budget_weighted` on `Job`; `job_id` and `tokens` on `Event` |
| `tools/factory-server/src/state.rs` | Modify: durations, token view, `skipped_lines`, `job_id` attribution, stale at 600 s |
| `tools/factory-server/src/events.rs` | Modify: `job_id` and tokens on agents, `reset`, `prune`, windowed `Tailer`, rotation flag, `pump` |
| `tools/factory-server/src/server.rs` | Modify: `CSP` constant, `pump`, serve `/patch.js` |
| `tools/factory-server/static/patch.js` | Create: minimal DOM morph |
| `tools/factory-server/static/board.js`, `app.js`, `dashboard.html`, `dashboard.css` | Modify |
| `tools/factory-server/tests/board.test.js` | Modify |
| `tools/factory-server/tests/patch.test.js` | Create |
| `tools/factory-server/tests/smoke.sh`, `seed-demo.sh` | Modify: new fields, no Python |
| `tools/factory-server/README.md`, `.claude/software-factory.md` | Modify: dashboard docs |

---

### Task 1: Server model and state

Deliverable: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml` passes with the new behavior and `/api/state` carries the new fields.

**Files:**
- Modify: `tools/factory-server/src/model.rs`, `state.rs`, `events.rs`, `server.rs`

**Interfaces:**
- Consumes: the board and event fields in Global Constraints.
- Produces: `model::{StageEntry, Series, Tokens}`; `Job.stage_history: Vec<StageEntry>`, `Job.series: Option<Series>`, `Job.tokens: Option<Tokens>`, `Job.budget_weighted: Option<u64>`; `Event.job_id: Option<String>`, `Event.tokens: Option<Tokens>`; `AgentState.job_id: Option<String>`, `AgentState.tokens_weighted: u64`. `state::{StageDuration, TokenView, stage_durations(job, now), token_view(job)}`, `JobView.stage_durations`, `JobView.token_view: Option<TokenView>`, `StateView.skipped_lines: u64`. `events::{pump, LOG_WINDOW_BYTES}`, `Tailer::with_window(path, bytes)`, `Tailer::take_rotated()`, `Registry::reset()`, `Registry::prune(now)`. `server::CSP`.

#### Part A: lenient model

- [ ] **Step 1: Write the failing model tests**

In `state.rs` tests, extend the `job()` helper literal with `stage_history: Vec::new(), series: None, tokens: None, budget_weighted: None,` after `reviews,`. Replace the `start` helper with a job-id aware one and keep a short alias:

```rust
    fn start_for(registry: &mut Registry, ts: f64, id: &str, kind: &str, job_id: Option<&str>) {
        let job = job_id.map_or(String::new(), |j| format!(r#","job_id":"{j}""#));
        registry.ingest_line(&format!(
            r#"{{"ts":{ts},"hook_event_name":"SubagentStart","agent_id":"{id}","agent_type":"{kind}"{job}}}"#
        ));
    }
```

Delete the old `start` helper and these tests, which test the removed cwd and stage fallbacks: `the_builder_id_wins_over_the_stage_rule`, `a_worktree_path_attaches_the_agent`, `the_stage_rule_attaches_all_reviewers_to_the_review_job`, `an_agent_from_an_earlier_stage_does_not_attach_to_a_later_job`, `an_empty_worktree_does_not_attach_an_arbitrary_agent`. In the remaining tests replace `start(&mut registry, T, ID, KIND, CWD)` with `start_for(&mut registry, T, ID, KIND, Some("001"))` where the test expects the agent on job 001, and with `None` where it expects the agent unassigned (`a_recent_done_unassigned_agent_is_kept`, `a_running_unassigned_agent_with_recent_activity_is_kept`, `a_stale_unassigned_agent_older_than_two_hours_is_dropped`). In `a_done_agent_without_an_end_time_is_dropped` replace `cwd: None,` with `cwd: None, job_id: None, tokens_weighted: 0,`. In `a_silent_running_agent_becomes_stale` change `20.0 + 1801.0` to `20.0 + 601.0`. In `a_tool_event_keeps_an_agent_running` change the build_state time `2000.0` to `1300.0`. Add:

```rust
    fn board_from(jobs_json: &str) -> Result<Board, Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("board.json");
        let base = r#"{"id":"001","dir":"d","slug":"s","created":"2026-10-13","feature":"f","branch":"b","stage":"build","round":0,"stage_since":5.0,"reviews":{}"#;
        std::fs::write(&path, format!(r#"{{"jobs":[{base}{jobs_json}}}]}}"#))?;
        let (board, error) = load_board(&path);
        assert!(error.is_none(), "got: {error:?}");
        Ok(board)
    }

    #[test]
    fn an_old_board_without_the_new_fields_still_loads() -> TestResult {
        let board = board_from("")?;
        let item = &board.jobs[0];
        assert!(item.stage_history.is_empty());
        assert!(item.series.is_none() && item.tokens.is_none() && item.budget_weighted.is_none());
        Ok(())
    }

    #[test]
    fn malformed_new_fields_are_dropped_not_fatal() -> TestResult {
        let board = board_from(
            r#","series":{"index":"two"},"tokens":"lots","budget_weighted":"x","stage_history":[{"stage":"spec","ts":1.0},"junk",{"stage":"build"}]"#,
        )?;
        let item = &board.jobs[0];
        assert!(item.series.is_none() && item.tokens.is_none() && item.budget_weighted.is_none());
        assert_eq!(item.stage_history.len(), 1);
        Ok(())
    }

    #[test]
    fn valid_new_fields_load() -> TestResult {
        let board = board_from(
            r#","series":{"id":"s1","index":2,"total":4,"next_plan":"p.md"},"tokens":{"weighted":500,"raw":4000},"budget_weighted":1000"#,
        )?;
        let item = &board.jobs[0];
        let series = item.series.as_ref().ok_or("no series")?;
        assert_eq!((series.index, series.total), (2, 4));
        assert_eq!(item.tokens.as_ref().map(|t| (t.weighted, t.raw)), Some((500, 4000)));
        assert_eq!(item.budget_weighted, Some(1000));
        Ok(())
    }
```

In `events.rs` tests add:

```rust
    #[test]
    fn job_id_and_tokens_are_read_and_a_bad_tokens_value_is_dropped() {
        let mut registry = Registry::new();
        registry.ingest_line(r#"{"ts":1.0,"hook_event_name":"SubagentStart","agent_id":"a","agent_type":"t","job_id":"001"}"#);
        registry.ingest_line(r#"{"ts":2.0,"hook_event_name":"PostToolUse","agent_id":"a","tool_name":"Edit","job_id":"001","tokens":{"weighted":30,"raw":300}}"#);
        registry.ingest_line(r#"{"ts":3.0,"hook_event_name":"PostToolUse","agent_id":"a","tool_name":"Edit","tokens":"bad"}"#);
        let agents = registry.agents();
        assert_eq!(agents[0].job_id.as_deref(), Some("001"));
        assert_eq!(agents[0].tokens_weighted, 30);
        assert_eq!(registry.skipped_lines, 0);
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml 2>&1 | tail -20`
Expected: compile errors (`stage_history`, `job_id` and the other new fields are missing).

- [ ] **Step 3: Implement the model**

In `model.rs` change the import to `use serde::{Deserialize, Deserializer, Serialize};`. Add after `Event`'s current fields `#[serde(default)] pub job_id: Option<String>,` and `#[serde(default, deserialize_with = "lenient")] pub tokens: Option<Tokens>,`. Add above `Job`:

```rust
/// Weighted and raw token usage.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Tokens {
    pub weighted: u64,
    #[serde(default)]
    pub raw: u64,
}

/// One stage change in a job's history.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct StageEntry {
    pub stage: String,
    pub ts: f64,
}

/// Where a job sits in a multi-plan series. The index is 1-based.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Series {
    pub index: u32,
    pub total: u32,
    #[serde(default)]
    pub id: serde_json::Value,
    #[serde(default)]
    pub next_plan: serde_json::Value,
}

/// A bad optional value becomes none, so it never makes the whole board unreadable.
fn lenient<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let value = Option::<serde_json::Value>::deserialize(d)?;
    Ok(value.and_then(|v| serde_json::from_value(v).ok()))
}

/// Bad history entries are dropped one by one, and a non-array value gives an empty list.
fn lenient_history<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<StageEntry>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect()
        })
        .unwrap_or_default())
}
```

In `Job`, after `reviews` add:

```rust
    #[serde(default, deserialize_with = "lenient_history")]
    pub stage_history: Vec<StageEntry>,
    #[serde(default, deserialize_with = "lenient")]
    pub series: Option<Series>,
    #[serde(default, deserialize_with = "lenient")]
    pub tokens: Option<Tokens>,
    #[serde(default, deserialize_with = "lenient")]
    pub budget_weighted: Option<u64>,
```

In `AgentState` add `pub job_id: Option<String>,` and `pub tokens_weighted: u64,` after `cwd`.

In `events.rs` `entry()` set `job_id: event.job_id.clone(), tokens_weighted: 0,` in the literal; in `entry()`'s callers nothing else. In `on_tool` (before `push_recent`) add:

```rust
        if agent.job_id.is_none() {
            agent.job_id = event.job_id.clone();
        }
        if let Some(tokens) = &event.tokens {
            agent.tokens_weighted = agent.tokens_weighted.saturating_add(tokens.weighted);
        }
```

Do the same job-id fill in `on_start` and `on_stop` right after `let agent = self.entry(event, id);`, and in `on_stop` also add the tokens line.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml 2>&1 | tail -15`
Expected: pass for model tests (state tests that need Part B may not compile yet; finish Part B before running the whole suite, but run `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml events::` now).

#### Part B: durations, tokens, attribution

- [ ] **Step 5: Write the failing state tests**

Add to the `state.rs` tests module:

```rust
    fn entry(stage: &str, ts: f64) -> StageEntry {
        StageEntry { stage: stage.to_string(), ts }
    }

    #[test]
    fn durations_sum_repeated_stages_and_run_the_current_one_to_now() {
        let mut item = job("001", "review", 400.0);
        item.stage_history = vec![
            entry("spec", 0.0), entry("build", 100.0), entry("review", 300.0),
            entry("build", 350.0), entry("review", 400.0),
        ];
        let durations = stage_durations(&item, 460.0);
        let get = |name: &str| durations.iter().find(|d| d.stage == name);
        assert_eq!(get("spec").map(|d| d.seconds), Some(100.0));
        assert_eq!(get("build").map(|d| d.seconds), Some(250.0));
        assert_eq!(get("review").map(|d| (d.seconds, d.current)), Some((110.0, true)));
    }

    #[test]
    fn merged_and_needs_human_have_no_growing_duration() {
        for last in ["merged", "needs-human"] {
            let mut item = job("001", last, 300.0);
            item.stage_history = vec![entry("spec", 0.0), entry("build", 100.0), entry(last, 300.0)];
            let durations = stage_durations(&item, 99_999.0);
            assert!(durations.iter().all(|d| d.stage != last), "{last}");
            assert_eq!(durations.len(), 2);
        }
    }

    #[test]
    fn a_job_without_history_falls_back_to_stage_since_and_a_clock_step_is_not_negative() {
        let durations = stage_durations(&job("001", "build", 100.0), 160.0);
        assert_eq!((durations.len(), durations[0].seconds), (1, 60.0));
        let mut item = job("001", "build", 100.0);
        item.stage_history = vec![entry("spec", 200.0), entry("build", 100.0)];
        assert!(stage_durations(&item, 150.0).iter().all(|d| d.seconds >= 0.0));
    }

    #[test]
    fn the_token_view_has_a_fraction_between_zero_and_one() -> TestResult {
        let mut item = job("001", "build", 1.0);
        assert!(token_view(&item).is_none());
        item.tokens = Some(Tokens { weighted: 750_000, raw: 5_000_000 });
        let view = token_view(&item).ok_or("none")?;
        assert_eq!(view.budget, 3_000_000);
        assert_eq!(view.fraction, 0.25);
        item.budget_weighted = Some(500_000);
        item.tokens = Some(Tokens { weighted: 900_000, raw: 0 });
        assert_eq!(token_view(&item).ok_or("none")?.fraction, 1.0);
        item.budget_weighted = Some(0);
        assert_eq!(token_view(&item).ok_or("none")?.fraction, 0.0);
        Ok(())
    }

    #[test]
    fn agents_attach_only_by_job_id() {
        let mut board = Board::default();
        board.jobs.push(job("001", "review", 10.0));
        board.jobs.push(job("002", "review", 10.0));
        let mut registry = Registry::new();
        start_for(&mut registry, 20.0, "r1", "factory-code-reviewer", Some("002"));
        start_for(&mut registry, 21.0, "r2", "factory-code-reviewer", None);
        start_for(&mut registry, 22.0, "r3", "factory-builder", Some("999"));
        let view = build_state(&board, &registry, 30.0, None);
        assert!(view.jobs[0].agents.is_empty());
        assert_eq!(view.jobs[1].agents.len(), 1);
        assert_eq!(view.unassigned_agents.len(), 2);
    }

    #[test]
    fn skipped_lines_and_stage_durations_are_in_the_serialized_state() -> TestResult {
        let mut registry = Registry::new();
        registry.ingest_line("garbage");
        registry.ingest_line("more garbage");
        let mut board = Board::default();
        board.jobs.push(job("001", "build", 1.0));
        let json = serde_json::to_value(build_state(&board, &registry, 5.0, None))?;
        assert_eq!(json["skipped_lines"], 2);
        assert!(json["jobs"][0]["stage_durations"].is_array());
        assert!(json["jobs"][0]["token_view"].is_null());
        Ok(())
    }
```

- [ ] **Step 6: Run to verify failure**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml 2>&1 | tail -20`
Expected: compile errors (`stage_durations`, `token_view`, `Tokens` in state).

- [ ] **Step 7: Implement**

In `state.rs`: import `use crate::model::{AgentState, Board, Job, RecentEvent, StageEntry, Tokens};` (`Tokens` only if used in non-test code; otherwise import it in the tests module). Set `const STALE_SECS: f64 = 600.0;` and add `const DEFAULT_BUDGET_WEIGHTED: u64 = 3_000_000;`. Add `pub stage_durations: Vec<StageDuration>, pub token_view: Option<TokenView>,` to `JobView` and `pub skipped_lines: u64,` to `StateView`. Add:

```rust
#[derive(Debug, Serialize, PartialEq)]
pub struct StageDuration {
    pub stage: String,
    pub seconds: f64,
    /// True for the stage the job is in now.
    pub current: bool,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TokenView {
    pub weighted: u64,
    pub raw: u64,
    pub budget: u64,
    /// Weighted over budget, always between 0 and 1.
    pub fraction: f64,
}

fn is_terminal(stage: &str) -> bool {
    matches!(stage, "merged" | "needs-human")
}

/// Time spent per stage, summed over repeats. A terminal stage that is still current shows
/// nothing, because its time would only grow.
pub fn stage_durations(job: &Job, now: f64) -> Vec<StageDuration> {
    let history: Vec<StageEntry> = if job.stage_history.is_empty() {
        vec![StageEntry { stage: job.stage.clone(), ts: job.stage_since }]
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
            None => out.push(StageDuration { stage: entry.stage.clone(), seconds, current }),
        }
    }
    out
}

/// Token totals and the budget fraction for the card bar.
pub fn token_view(job: &Job) -> Option<TokenView> {
    let tokens = job.tokens.as_ref()?;
    let budget = job.budget_weighted.unwrap_or(DEFAULT_BUDGET_WEIGHTED);
    let fraction = if budget == 0 {
        0.0
    } else {
        (tokens.weighted as f64 / budget as f64).clamp(0.0, 1.0)
    };
    Some(TokenView { weighted: tokens.weighted, raw: tokens.raw, budget, fraction })
}
```

Remove `stage_for_agent_type`, `owns_agent` and `fits_stage`. Replace the attribution in `build_state` with:

```rust
        let index = agent
            .job_id
            .as_deref()
            .and_then(|id| jobs.iter().position(|view| view.job.id == id));
```

and add to the `JobView` literal `stage_durations: stage_durations(job, now), token_view: token_view(job),` and to the `StateView` literal `skipped_lines: registry.skipped_lines,`. Update the stale doc/test imports so `cargo check` has no unused warnings.

- [ ] **Step 8: Run to verify pass**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml 2>&1 | tail -15`
Expected: all pass.

- [ ] **Step 9: Commit**

Invoke skill `dev-workflow-skills:1-git-commit`, then:

```bash
git add tools/factory-server/src/model.rs tools/factory-server/src/state.rs tools/factory-server/src/events.rs && git commit --only -m "feat(factory): read stage history, series and tokens, attribute agents by job id" -- tools/factory-server/src/model.rs tools/factory-server/src/state.rs tools/factory-server/src/events.rs
```

#### Part C: bounded log reader, pruning and CSP

- [ ] **Step 10: Write the failing tests**

In `events.rs` tests add (the module has `Mutex`-free imports, so add `use std::sync::Mutex;` in the tests module if absent):

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

In `server.rs` tests add:

```rust
    #[test]
    fn the_csp_blocks_base_tags_and_form_posts() {
        for part in ["default-src 'self'", "frame-ancestors 'none'", "base-uri 'none'", "form-action 'none'"] {
            assert!(CSP.contains(part), "missing {part}");
        }
    }
```

and in the existing `replies_carry_security_headers` test append before `Ok(())`: `assert!(reply.contains("base-uri 'none'"), "got: {reply}"); assert!(reply.contains("form-action 'none'"), "got: {reply}");`.

- [ ] **Step 11: Run to verify failure**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml 2>&1 | tail -20`
Expected: compile errors (`with_window`, `take_rotated`, `pump`, `prune`, `CSP`).

- [ ] **Step 12: Implement**

In `events.rs` add `use std::sync::Mutex;` and below `REOPEN_GRACE_SECS`:

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

Add to `impl Registry`:

```rust
    /// Forgets everything, for a log that was replaced.
    pub fn reset(&mut self) {
        self.agents.clear();
        self.recent.clear();
        self.skipped_lines = 0;
    }

    /// Drops agents idle for a day, then the oldest ones beyond the cap.
    pub fn prune(&mut self, now: f64) {
        self.agents.retain(|_, agent| now - last_activity(agent) <= PRUNE_AFTER_SECS);
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

Replace the `Tailer` struct and impl with the windowed version:

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
    /// True until the first read of a file, when the window applies.
    fresh: bool,
    /// True while the cut first line of a windowed read is still to be dropped.
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

    /// True once after the file was truncated, replaced or deleted. The first read is not rotation.
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
        complete
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect()
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

In `server.rs`: import `use crate::events::{pump, Registry, Tailer, LOG_WINDOW_BYTES};`, add

```rust
/// Content security policy for every reply.
pub const CSP: &str = "default-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";
```

use `CSP.as_bytes()` in the security header array, and make the `spawn_tailer` closure:

```rust
        let mut tailer = Tailer::with_window(path, LOG_WINDOW_BYTES);
        loop {
            pump(&mut tailer, &app.registry, now_secs());
            std::thread::sleep(POLL_INTERVAL);
        }
```

- [ ] **Step 13: Run to verify pass**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml 2>&1 | tail -15` then `cargo check -j4 --manifest-path tools/factory-server/Cargo.toml 2>&1 | tail -5`
Expected: all tests pass and no warnings.

- [ ] **Step 14: Commit**

Invoke skill `dev-workflow-skills:1-git-commit`, then:

```bash
git add tools/factory-server/src/events.rs tools/factory-server/src/server.rs && git commit --only -m "feat(factory): bound the log reader, reset on rotation and tighten the CSP" -- tools/factory-server/src/events.rs tools/factory-server/src/server.rs
```

---

### Task 2: Page

Deliverable: `node --test tools/factory-server/tests/*.test.js` and the Rust tests pass, and the page shows the new data.

**Files:**
- Create: `tools/factory-server/static/patch.js`, `tools/factory-server/tests/patch.test.js`
- Modify: `tools/factory-server/static/board.js`, `app.js`, `dashboard.html`, `dashboard.css`, `tools/factory-server/src/server.rs`, `tools/factory-server/tests/board.test.js`

**Interfaces:**
- Consumes: Task 1 API: `skipped_lines`, `jobs[].series`, `jobs[].stage_durations`, `jobs[].token_view` (`weighted`, `raw`, `budget`, `fraction`).
- Produces: `FactoryBoard.{copyChip, seriesChip, durationsLine, tokensBar, announcement}`, `FactoryPatch.patchChildren(liveParent, nextParent)`, route `/patch.js`.

- [ ] **Step 1: Write the failing board tests**

Append to `tests/board.test.js`:

```js
test("renderCard hides time in stage for merged and needs-human cards", () => {
  assert.match(board.renderCard(makeJob({ stage: "build" })), /in stage/);
  assert.doesNotMatch(board.renderCard(makeJob({ stage: "merged" })), /in stage/);
  assert.doesNotMatch(board.renderCard(makeJob({ stage: "needs-human", reason: "x" })), /in stage/);
});

test("durationsLine shows per-stage times and marks the current one", () => {
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
  assert.match(board.renderCard(makeJob({ series: { index: 2, total: 3 } })), /plan 2 of 3/);
  assert.doesNotMatch(board.renderCard(makeJob({})), /class="series"/);
});

test("tokensBar shows weighted tokens, a bounded bar and a warning level", () => {
  assert.equal(board.tokensBar(makeJob({})), "");
  const half = board.tokensBar(makeJob({ token_view: { weighted: 1500000, raw: 9000000, budget: 3000000, fraction: 0.5 } }));
  assert.match(half, /1\.5M of 3M/);
  assert.match(half, /width:50%/);
  assert.match(half, /tokens-ok/);
  const high = board.tokensBar(makeJob({ token_view: { weighted: 2500000, raw: 0, budget: 3000000, fraction: 0.83 } }));
  assert.match(high, /tokens-warn/);
  const over = board.tokensBar(makeJob({ token_view: { weighted: 9, raw: 0, budget: 3, fraction: 7 } }));
  assert.match(over, /width:100%/);
  assert.match(over, /tokens-stop/);
  const bad = board.tokensBar(makeJob({ token_view: { weighted: "x", raw: 0, budget: 0, fraction: Number.NaN } }));
  assert.doesNotMatch(bad, /NaN|undefined/);
});

test("copyChip is a button that carries the escaped command", () => {
  const html = board.copyChip('factory rework 003 <note> "x"');
  assert.match(html, /^<button type="button" class="cmd" data-copy="/);
  assert.match(html, /data-copy="factory rework 003 &lt;note&gt; &quot;x&quot;"/);
  assert.doesNotMatch(html, /<note>/);
});

test("needs-human cards and the summary use copy chips for approve, rework and cancel", () => {
  const card = board.renderCard(makeJob({ id: "003", stage: "needs-human", reason: "r" }));
  assert.match(card, /data-copy="factory approve 003"/);
  assert.match(card, /data-copy="factory rework 003 &lt;note&gt;"/);
  assert.match(card, /data-copy="factory cancel 003"/);
  const summary = board.renderSummary(makeState([makeJob({ id: "004", stage: "needs-human", reason: "r" })]));
  assert.match(summary, /data-copy="factory approve 004"/);
});

test("renderFeed reports skipped log lines only when there are some", () => {
  assert.doesNotMatch(board.renderFeed(makeState([])), /skipped/);
  assert.match(board.renderFeed(makeState([], { skipped_lines: 3 })), /3 event log lines skipped/);
  assert.match(board.renderFeed(makeState([], { skipped_lines: 1, recent: [{ ts: 1, kind: "tool" }] })), /1 event log line skipped/);
});

test("announcement is short and depends only on the level text", () => {
  assert.equal(board.announcement({ level: "ok", text: "Live, updated 10:00:01" }), "Dashboard is live");
  assert.equal(board.announcement({ level: "error", text: "Server unreachable" }), "Server unreachable");
});
```

Run: `node --test tools/factory-server/tests/board.test.js`
Expected: FAIL (new functions missing).

- [ ] **Step 2: Implement the renderers in `board.js`**

Add before `renderCard`:

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
    const k = Math.min(Math.max(index, 1), total);
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

  function formatTokens(count) {
    const n = Number(count);
    if (!Number.isFinite(n) || n < 0) return "0";
    if (n >= 1e6) return `${Number((n / 1e6).toFixed(1))}M`;
    if (n >= 1e3) return `${Math.round(n / 1e3)}K`;
    return String(Math.round(n));
  }

  // The bar is capped at 100 percent. The level is also written as text, never colour alone.
  function tokensBar(job) {
    const view = job.token_view;
    if (!view || !Number.isFinite(view.weighted)) return "";
    const fraction = Number.isFinite(view.fraction) ? Math.min(Math.max(view.fraction, 0), 1) : 0;
    const level = fraction >= 1 ? "stop" : fraction >= 0.8 ? "warn" : "ok";
    const percent = Math.round(fraction * 100);
    const budget = Number.isFinite(view.budget) && view.budget > 0 ? ` of ${formatTokens(view.budget)}` : "";
    return (
      `<div class="tokens tokens-${level}" title="Weighted tokens against the job budget. Raw: ${escapeHtml(formatTokens(view.raw))}">` +
      `<span class="tokens-text">${formatTokens(view.weighted)}${budget} tokens (${percent}%${level === "ok" ? "" : `, ${level}`})</span>` +
      `<span class="tokens-bar" aria-hidden="true"><span class="tokens-fill" style="width:${percent}%"></span></span></div>`
    );
  }

  function announcement(header) {
    return header.level === "ok" ? "Dashboard is live" : header.text;
  }
```

In `renderCard`: replace the `reason` block with

```js
    const reason =
      job.stage === "needs-human"
        ? `<p class="reason">${escapeHtml(job.reason || "needs a human")}</p>` +
          `<p class="hint">${copyChip(`factory approve ${job.id}`)}${copyChip(`factory rework ${job.id} <note>`)}${copyChip(`factory cancel ${job.id}`)}</p>`
        : "";
    const showAge = job.stage !== "merged" && job.stage !== "needs-human";
```

In the card head insert `seriesChip(job.series) +` right after the `card-stage` span. Replace the `<span class="age">` string with `(showAge ? `<span class="age">${formatAge(job.seconds_in_stage)} in stage</span>` : "")`. After the `.meta` div add `durationsLine(job) + tokensBar(job) +`. In `summaryJob` replace the `summary-actions` span with `<span class="summary-actions">${copyChip(`factory approve ${job.id}`)}${copyChip(`factory rework ${job.id} <note>`)}</span></li>`. In `renderFeed` compute `const skipped = Number(state.skipped_lines) || 0;` and `const note = skipped ? `<p class="warning">${skipped} event log ${plural(skipped, "line", "lines")} skipped</p>` : "";`, and put `${note}` before `</section>` in both returned strings. Export the five new functions in `api`.

Run: `node --test tools/factory-server/tests/board.test.js` and expect all pass. Note: the empty-feed return must also carry the note.

- [ ] **Step 3: Write the failing patch tests**

Create `tests/patch.test.js` with a tiny DOM stub that records writes:

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

function run(live, next) {
  writes = [];
  patch.patchChildren(live, next);
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

test("a different tag replaces the node and an unchanged element object is kept", () => {
  const live = el("div", {}, [el("p", {}, [txt("a")])]);
  assert.deepEqual(run(live, el("div", {}, [el("span", {}, [txt("a")])])), ["replace"]);
  const button = el("button", { "data-copy": "x" }, [txt("x")]);
  const tree = el("div", {}, [el("article", {}, [el("p", {}, [txt("old")]), button])]);
  run(tree, el("div", {}, [el("article", {}, [el("p", {}, [txt("new")]), el("button", { "data-copy": "x" }, [txt("x")])])]));
  assert.equal(tree.childNodes[0].childNodes[1], button);
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

- [ ] **Step 5: Serve `patch.js` (test first)**

In `server.rs` tests, in `static_assets_are_served_with_the_right_content_type` add before the page request: `let patch = route(&app, "GET", "/patch.js", host, 1, 5.0); assert_eq!(patch.status, 200); assert_eq!(patch.content_type, "text/javascript; charset=utf-8");`. Run `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml static_assets` and expect FAIL (404). Add `const PATCH_JS: &str = include_str!("../static/patch.js");` and the route `"/patch.js" => asset("text/javascript; charset=utf-8", PATCH_JS),`. Re-run and expect PASS.

- [ ] **Step 6: Wire the page**

`dashboard.html`: replace the `#status` paragraph (drop `role="status"`) and add the separate announcement element:

```html
      <p id="status" class="status status-warn">
        <span class="dot" aria-hidden="true"></span><span id="status-text">Connecting</span>
      </p>
      <p id="announce" class="sr-only" role="status" aria-live="polite"></p>
```

Add `<p id="toast" class="toast" role="status" hidden></p>` just before the `</div>` that closes `.page`. In the Commands panel replace each `<code>X</code>` with `<button type="button" class="cmd" data-copy="Y" title="Copy to the clipboard">Y</button>`, where `Y` is the same text with the `/factory` prefix kept for slash commands (`/factory &lt;feature&gt;`, `/factory next` is removed, add `factory run`, `factory approve &lt;job-id&gt;`, `factory rework &lt;job-id&gt; &lt;note&gt;`, `factory cancel &lt;job-id&gt;`, `factory status`). Each `dd` explains the command in one short phrase. Add `<script src="/patch.js"></script>` before `/board.js`.

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

The toast sits outside the patched regions, so polling never resets it. In `dashboard.css` check variable names with `grep -n "^  --" tools/factory-server/static/dashboard.css` (they are `--border`, `--muted`, `--primary`, `--foreground`, `--card`, `--muted-foreground`, `--success`, `--attention`, `--destructive`, `--font`), replace any `.hint code` rule with `.hint .cmd { display: block; }` and append:

```css
.cmd { display: inline-block; max-width: 100%; padding: 1px 6px; border: 1px solid var(--border); border-radius: 3px; background: var(--muted); color: inherit; font: 0.9em var(--font); text-align: left; overflow-wrap: anywhere; cursor: pointer; }
.cmd:hover { border-color: var(--primary); }
.cmd:focus-visible { outline: 2px solid var(--primary); outline-offset: 2px; }
.series { padding: 1px 6px; border: 1px solid var(--border); border-radius: 999px; color: var(--muted-foreground); font-weight: 500; white-space: nowrap; }
.durations { display: flex; flex-wrap: wrap; gap: 0 0.8rem; margin: 0.35rem 0 0; color: var(--muted-foreground); font-size: 12px; }
.duration-now { color: var(--foreground); font-weight: 500; }
.tokens { margin: 0.35rem 0 0; font-size: 12px; }
.tokens-bar { display: block; height: 4px; margin-top: 3px; border-radius: 2px; background: var(--muted); overflow: hidden; }
.tokens-fill { display: block; height: 100%; background: var(--success); }
.tokens-warn .tokens-fill { background: var(--attention); }
.tokens-stop .tokens-fill { background: var(--destructive); }
.sr-only { position: absolute; width: 1px; height: 1px; margin: -1px; padding: 0; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; border: 0; }
.toast { position: fixed; left: 50%; bottom: 1rem; transform: translateX(-50%); max-width: calc(100% - 2rem); margin: 0; padding: 0.4rem 0.8rem; border: 1px solid var(--border); border-radius: 6px; background: var(--card); color: var(--foreground); font-size: 12px; overflow-wrap: anywhere; }
```

The inline `style="width:N%"` on the fill needs the CSP to allow inline styles. It does not (`default-src 'self'`). So do not use an inline style: render the bar as `<progress class="tokens-bar" max="100" value="N">` is a form control and a "raw primitive" only for the Rocket app rules, not for this tool, but a plain DOM approach that keeps the CSP is better: have `board.js` emit `data-pct="N"` and have `app.js` set `fill.style.width` through the CSSOM after each patch (CSSOM writes are allowed by CSP). Apply this: in `tokensBar` emit `<span class="tokens-fill" data-pct="${percent}"></span>` instead of the inline style, change the board test `width:50%` and `width:100%` assertions to `data-pct="50"` and `data-pct="100"`, and add to `app.js` after the four `setHtml` calls:

```js
    for (const fill of document.querySelectorAll(".tokens-fill")) {
      const width = `${fill.getAttribute("data-pct") || 0}%`;
      if (fill.style.width !== width) fill.style.width = width;
    }
```

Because `patchChildren` removes style attributes that the new tree lacks, the CSSOM width would be wiped each patch. To avoid that, add `style` to the keep list: in `syncAttributes` skip the name `style` in the removal loop (`if (attr.name !== "style" && !names.has(attr.name))`), and add a patch test `a style set from script is not removed by a patch`. Write that test first in `patch.test.js`:

```js
test("a style attribute set from script survives a patch", () => {
  const live = el("div", {}, [el("span", { class: "f", style: "width: 50%;" }, [])]);
  assert.deepEqual(run(live, el("div", {}, [el("span", { class: "f" }, [])])), []);
});
```

Run it, see it fail, then apply the `syncAttributes` change and see it pass.

Run: `node --test tools/factory-server/tests/*.test.js` and `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml`
Expected: all pass.

- [ ] **Step 7: Commit**

Invoke skill `dev-workflow-skills:1-git-commit`, then:

```bash
git add tools/factory-server/static/patch.js tools/factory-server/static/board.js tools/factory-server/static/app.js tools/factory-server/static/dashboard.html tools/factory-server/static/dashboard.css tools/factory-server/src/server.rs tools/factory-server/tests/board.test.js tools/factory-server/tests/patch.test.js && git commit --only -m "feat(factory): tokens bar, series chip, stage times, copy chips and minimal DOM patching" -- tools/factory-server/static/patch.js tools/factory-server/static/board.js tools/factory-server/static/app.js tools/factory-server/static/dashboard.html tools/factory-server/static/dashboard.css tools/factory-server/src/server.rs tools/factory-server/tests/board.test.js tools/factory-server/tests/patch.test.js
```

---

### Task 3: Smoke test, seeding and docs

Deliverable: the smoke test passes against the real server using only the new files, the seed script builds a demo root, and the docs match.

**Files:**
- Modify: `tools/factory-server/tests/smoke.sh`, `tools/factory-server/tests/seed-demo.sh`, `tools/factory-server/README.md`, `.claude/software-factory.md`

**Interfaces:**
- Consumes: the board and event formats from Global Constraints. No `factory-ctl`, no hook, no Python.
- Produces: `smoke.sh` and `seed-demo.sh` that write `board.json` and `events.jsonl` directly.

- [ ] **Step 1: Rewrite `smoke.sh` (it fails against the old script first)**

Run the old script once: `bash tools/factory-server/tests/smoke.sh 2>&1 | tail -5`. Expected: it fails because `factory-ctl` and the hook no longer exist. Replace the file with:

```bash
#!/usr/bin/env bash
# End-to-end check: board.json and events.jsonl feed the real server, and the API shows them.
set -u

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tool="$(cd "$here/.." && pwd)"

tmp="$(mktemp -d "${TMPDIR:-/tmp}/factory-smoke.XXXXXX")"
pid=""
fails=0
cleanup() {
  if [ -n "$pid" ]; then kill "$pid" 2>/dev/null; fi
  if [ "$fails" -ne 0 ] && [ -f "$tmp/server.log" ]; then
    echo "--- server.log ---"
    cat "$tmp/server.log"
  fi
  rm -rf "$tmp"
}
trap cleanup EXIT

pass() { echo "PASS: $1"; }
fail() { echo "FAIL: $1"; fails=$((fails + 1)); }
check() {
  local description="$1"
  shift
  if "$@" >/dev/null 2>&1; then pass "$description"; else fail "$description"; fi
}

cargo build -j4 --manifest-path "$tool/Cargo.toml" -q || { echo "FAIL: build"; exit 1; }
bin="$tool/target/debug/factory-server"

root="$tmp/root"
data="$root/.superpowers/factory"
mkdir -p "$data"
board="$data/board.json"
log="$data/events.jsonl"

# Writes the board with one job in the given stage.
write_board() {
  local stage="$1" now
  now="$(date +%s)"
  cat >"$board.tmp" <<JSON
{"jobs":[{"id":"001","dir":"2026-10-13-001-smoke","slug":"smoke","created":"2026-10-13","feature":"Smoke test job","branch":"factory/001-smoke","stage":"$stage","round":0,"stage_since":$now,"reviews":{"security":"pending","ux":"pending","ui":"pending","code":"pending"},"stage_history":[{"stage":"spec","ts":$((now - 120))},{"stage":"$stage","ts":$now}],"series":{"id":"s1","index":2,"total":3,"next_plan":"p3.md"},"tokens":{"weighted":600000,"raw":4000000}}]}
JSON
  mv "$board.tmp" "$board"
}

event() { printf '%s\n' "$1" >>"$log"; }

port="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
url="http://127.0.0.1:$port"
state() { curl -fs "$url/api/state"; }

# Polls until a jq filter is true on the state, for at most 10 seconds.
wait_for() {
  local filter="$1"
  for _ in $(seq 1 100); do
    if state 2>/dev/null | jq -e "$filter" >/dev/null 2>&1; then return 0; fi
    sleep 0.1
  done
  return 1
}
await() {
  local description="$1" filter="$2"
  if wait_for "$filter"; then pass "$description"; else fail "$description"; fi
}

write_board build
: >"$log"
chmod 600 "$log"
"$bin" --root "$root" --port "$port" >"$tmp/server.log" 2>&1 &
pid=$!
for _ in $(seq 1 100); do
  if curl -fs "$url/healthz" >/dev/null 2>&1; then break; fi
  sleep 0.1
done

now="$(date +%s)"
check "health endpoint answers" curl -fs "$url/healthz"
await "the job shows its stage, series and tokens" '.jobs[0] | select(.id=="001" and .stage=="build" and .series.index==2 and .token_view.fraction==0.2)'
await "per-stage durations come from the stage history" '.jobs[0].stage_durations | map(.stage) == ["spec","build"]'

event "{\"ts\":$now,\"hook_event_name\":\"SubagentStart\",\"agent_id\":\"b1\",\"agent_type\":\"factory-builder\",\"job_id\":\"001\"}"
event "{\"ts\":$now,\"hook_event_name\":\"PostToolUse\",\"agent_id\":\"b1\",\"tool_name\":\"Read\",\"tool_target\":\"CLAUDE.md\",\"job_id\":\"001\",\"tokens\":{\"weighted\":10,\"raw\":100}}"
await "the builder attaches to the job by job_id with its live tool" '.jobs[0].agents[0] | select(.agent_type=="factory-builder" and .last_tool=="Read" and .status=="running" and .tokens_weighted==10)'

event "{\"ts\":$now,\"hook_event_name\":\"SubagentStart\",\"agent_id\":\"x1\",\"agent_type\":\"factory-code-reviewer\"}"
await "an event without job_id stays unassigned" '.unassigned_agents | map(.agent_id) == ["x1"]'

write_board review
await "a stage change shows up on the next poll" '.jobs[0] | select(.stage=="review")'

event "{\"ts\":$((now + 5)),\"hook_event_name\":\"SubagentStop\",\"agent_id\":\"b1\",\"job_id\":\"001\",\"last_message\":\"done\"}"
await "a stopped agent is marked done" '.jobs[0].agents[0] | select(.status=="done")'

write_board merged
await "a merged job has no growing merged duration" '.jobs[0].stage_durations | all(.stage != "merged")'

check "the page is served" bash -c "curl -fs $url/ | grep -q 'Software factory'"
check "board.js and patch.js are served" bash -c "curl -fs $url/board.js | grep -q FactoryBoard && curl -fs $url/patch.js | grep -q FactoryPatch"
check "the CSP forbids base and form targets" bash -c "curl -sI $url/ | grep -i content-security-policy | grep -q \"base-uri 'none'; form-action 'none'\""
code="$(curl -s -o /dev/null -w '%{http_code}' -H 'Host: evil.example' "$url/api/state")"
if [ "$code" = "403" ]; then pass "a foreign Host header gets 403"; else fail "a foreign Host header gets 403 (got $code)"; fi

echo "garbage line" >>"$log"
await "skipped log lines are counted in the state" '.skipped_lines >= 1'
check "a garbage line does not stop the server" curl -fs "$url/healthz"

# A broken new field must not turn the board into an error.
cat >"$board" <<JSON
{"jobs":[{"id":"001","dir":"d","slug":"s","created":"2026-10-13","feature":"f","branch":"b","stage":"build","round":0,"stage_since":1,"reviews":{},"series":{"index":"x"},"tokens":"lots","stage_history":["junk"]}]}
JSON
await "a malformed series, tokens or history does not give a board error" '.board_error == null and (.jobs | length) == 1'

if [ "$fails" -eq 0 ]; then echo "all passed"; else echo "$fails failed"; fi
exit $((fails > 0))
```

- [ ] **Step 2: Run to verify**

Run: `bash tools/factory-server/tests/smoke.sh 2>&1 | tail -20`
Expected: every line `PASS`, last line `all passed`. If a check fails, `server.log` is printed. Fix the script or the code, not the check.

- [ ] **Step 3: Rewrite `seed-demo.sh`**

Replace the file with a version that writes the two files directly and is idempotent (it removes the old demo data under the given root before writing, but refuses a root that is not empty and not a previous demo):

```bash
#!/usr/bin/env bash
# Builds a demo factory root with jobs in every stage, for a visual check of the dashboard.
# Usage: seed-demo.sh <dir>. Safe to run again on the same dir.
set -eu

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
root="${1:?usage: seed-demo.sh <dir>}"
data="$root/.superpowers/factory"
marker="$data/.demo"

if [ -d "$data" ] && [ ! -f "$marker" ] && [ -n "$(ls -A "$data")" ]; then
  echo "refusing to overwrite $data: it is not a demo root" >&2
  exit 1
fi
mkdir -p "$data"
touch "$marker"
log="$data/events.jsonl"
: >"$log"
chmod 600 "$log"

now="$(date +%s)"
reviews() { printf '{"security":"%s","ux":"%s","ui":"%s","code":"%s"}' "$1" "$2" "$3" "$4"; }
job() {
  # id feature stage reason round reviews history series tokens
  printf '{"id":"%s","dir":"2026-10-13-%s-demo","slug":"demo","created":"2026-10-13","feature":"%s","branch":"factory/%s-demo","stage":"%s","reason":%s,"round":%s,"stage_since":%s,"reviews":%s,"stage_history":%s,"series":%s,"tokens":%s}' \
    "$1" "$1" "$2" "$1" "$3" "$4" "$5" "$((now - 300))" "$6" "$7" "$8" "$9"
}
none='{"security":"pending","ux":"pending","ui":"pending","code":"pending"}'
hist() { printf '[{"stage":"spec","ts":%s},{"stage":"build","ts":%s},{"stage":"%s","ts":%s}]' "$((now - 1500))" "$((now - 900))" "$1" "$((now - 300))"; }

{
  printf '{"jobs":['
  job 001 "Add a request timeout setting" build null 0 "$none" "$(hist build)" '{"id":"s1","index":2,"total":3,"next_plan":"p3.md"}' '{"weighted":2500000,"raw":14000000}'
  printf ','
  job 002 "Fix collection import crash on empty folders" review null 1 "$(reviews pass skipped skipped running)" "$(hist review)" null '{"weighted":900000,"raw":5200000}'
  printf ','
  job 003 "Rename the environment variables panel" needs-human '"touches the collection trust gate"' 2 "$(reviews changes pass pass pass)" "$(hist needs-human)" null '{"weighted":3100000,"raw":17000000}'
  printf ','
  job 004 "Show response size in the status bar" merged null 0 "$(reviews pass pass pass pass)" "$(hist merged)" null '{"weighted":400000,"raw":2300000}'
  printf ','
  job 005 "Add a gRPC reflection toggle" spec null 0 "$none" "[{\"stage\":\"spec\",\"ts\":$((now - 300))}]" null null
  printf ']}\n'
} >"$data/board.json.tmp"
mv "$data/board.json.tmp" "$data/board.json"

event() { printf '%s\n' "$1" >>"$log"; }
event "{\"ts\":$((now - 60)),\"hook_event_name\":\"SubagentStart\",\"agent_id\":\"b-1\",\"agent_type\":\"factory-builder\",\"job_id\":\"001\"}"
event "{\"ts\":$((now - 30)),\"hook_event_name\":\"PostToolUse\",\"agent_id\":\"b-1\",\"tool_name\":\"Edit\",\"tool_target\":\"src/components/request/TimeoutField.tsx\",\"job_id\":\"001\",\"tokens\":{\"weighted\":1200,\"raw\":9000}}"
event "{\"ts\":$((now - 20)),\"hook_event_name\":\"SubagentStart\",\"agent_id\":\"r-2\",\"agent_type\":\"factory-code-reviewer\",\"job_id\":\"002\"}"
event "{\"ts\":$((now - 10)),\"hook_event_name\":\"PostToolUse\",\"agent_id\":\"r-2\",\"tool_name\":\"Read\",\"tool_target\":\"crates/rocket-import/src/lib.rs\",\"job_id\":\"002\"}"
event "{\"ts\":$((now - 5)),\"hook_event_name\":\"SubagentStart\",\"agent_id\":\"s-5\",\"agent_type\":\"factory-spec-writer\"}"

echo "Demo root ready: $root"
echo "Start the server:"
echo "  cargo run -j4 --manifest-path $repo/tools/factory-server/Cargo.toml -- --root $root --port 8765"
echo "Then open http://127.0.0.1:8765/"
```

Verify it, including a second run and the JSON validity:

```bash
d="$(mktemp -d)"; bash tools/factory-server/tests/seed-demo.sh "$d" >/dev/null && bash tools/factory-server/tests/seed-demo.sh "$d" >/dev/null && jq -e '.jobs | length == 5' "$d/.superpowers/factory/board.json" && wc -l "$d/.superpowers/factory/events.jsonl"; rm -rf "$d"
```

Expected: `true` and `5 .../events.jsonl` (the second run does not double the lines).

- [ ] **Step 4: Docs**

`tools/factory-server/README.md`: change the Run command to `--root "$(git rev-parse --show-toplevel)"` run from the main checkout (the folder that holds `.superpowers/`; from a worktree use the path of the main checkout). In "What it reads": `board.json` is written by the `factory` driver (`tools/factory/`), `events.jsonl` is written by the driver too, with `job_id` and `tokens` on every event, and there is no hook. Add the bullets: events without a `job_id` show under "Agents without a job"; at startup only the last 4 MiB of the log is read; agents idle for a day are dropped (cap 500); the registry resets when the log is truncated, replaced or deleted (rotate by moving the file); an agent silent for 10 minutes shows `stale`. In the Endpoints table add `/patch.js` and change the `/api/state` row to `generated_at`, `board_error`, `skipped_lines`, `jobs[]` (with `stage_history`, `stage_durations`, `series`, `token_view`), `unassigned_agents[]`, `recent[]`. Add that the CSP has `base-uri 'none'` and `form-action 'none'`. Tests section: `node --test tools/factory-server/tests/board.test.js tools/factory-server/tests/patch.test.js`.

`.claude/software-factory.md`, section "Live tracking dashboard": rewrite its data-flow line to `factory driver -> .superpowers/factory/{board.json,events.jsonl} -> factory-server -> dashboard.html`, remove every mention of the log-event hook and of matching by cwd or stage, and state: agents are matched to a job only by `job_id`; the card shows job id, optional `plan k of N` chip (hidden for a single plan), feature, branch, builder round, review chips, time in stage (hidden once merged or needs-human), time per stage from `stage_history`, weighted tokens against the budget with a bar (text level `warn` at 80 percent, `stop` at 100), and copy chips for `factory approve <id>`, `factory rework <id> <note>` and `factory cancel <id>`; the page patches only changed text, the status line is not a live region and one short announcement is made when the connection level changes. Run: `grep -n "log-event\|by the cwd\|never rotated" .claude/software-factory.md` and fix any remaining match in the dashboard section (matches elsewhere belong to plan 06).

- [ ] **Step 5: Final verification**

Run:

```bash
cargo test -j4 --manifest-path tools/factory-server/Cargo.toml 2>&1 | tail -5
cargo check -j4 --manifest-path tools/factory-server/Cargo.toml 2>&1 | tail -3
node --test tools/factory-server/tests/board.test.js tools/factory-server/tests/patch.test.js 2>&1 | tail -8
bash tools/factory-server/tests/smoke.sh 2>&1 | tail -4
```

Expected: tests pass, check has no warnings, node tests pass, smoke prints `all passed`. Record the commands and results in the handoff.

- [ ] **Step 6: Commit**

Invoke skill `dev-workflow-skills:1-git-commit`, then:

```bash
git add tools/factory-server/tests/smoke.sh tools/factory-server/tests/seed-demo.sh tools/factory-server/README.md .claude/software-factory.md && git commit --only -m "test(factory): smoke and seed the dashboard with driver fields and update docs" -- tools/factory-server/tests/smoke.sh tools/factory-server/tests/seed-demo.sh tools/factory-server/README.md .claude/software-factory.md
```

---

## Self-Review

- **Spec coverage:** attribution by `job_id` and removal of cwd and stage fallbacks (T1 B), lenient new fields (T1 A), durations without growing merged and needs-human time (T1 B, T2), tokens, budget fraction and bar (T1 B, T2), series chip hidden for a single plan (T2), `skipped_lines` (T1 B, T2), startup window, cut-line drop, prune, reset, 10 minute stale, CSP (T1 C), copy chips for approve, rework and cancel (T2), no aria-live churn, minimal patching, plain DOM (T2), idempotent seed, polling smoke, `server.log` on failure, docs and the final commands (T3).
- **Placeholders:** none. The only conditional is the explicit open check on field names against plans 01 and 03, with the rule to touch `model.rs` only.
- **Type consistency:** `StageEntry`, `Series`, `Tokens`, `StageDuration`, `TokenView`, `stage_durations`, `token_view`, `pump`, `Tailer::with_window`, `take_rotated`, `Registry::reset` and `prune`, `CSP`, `copyChip`, `seriesChip`, `durationsLine`, `tokensBar`, `announcement` and `FactoryPatch.patchChildren` keep one name and signature throughout.
- **Known limits:** per-job tokens come from the board (written by factory-core), per-agent tokens are the sum of event tokens and are informational. The tokens bar width is set through the CSSOM because the CSP forbids inline styles.

Next plan: docs/superpowers/plans/2026-10-13-software-factory-v3/plan-08-harness-and-verification.md
