# Factory Accurate Usage, Plan 01: usage fix, per-run record and report

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task, executed by a `factory-builder` subagent. Skip that skill's final whole-branch review, its ledger scripts and its progress file (the driver owns the ledger). Steps use checkbox (`- [ ]`) syntax. This is the only plan of the series.

**Goal:** Record correct output-token counts for every agent run, log enough per-run detail to break a job's cost down, and show it in `factory tokens`.

**Architecture:** `factory-agent` takes the field-wise maximum of the per-message usage sum and the `result` event usage as the final run usage, and remembers the first message's start-up context. `factory-engine` writes the extra per-run fields to `usage.jsonl`. `factory-cli` reads them, prints a per-run table and flags jobs that still have old lines.

**Tech Stack:** Rust 2021, serde_json, tempfile (tests). Workspace at `tools/factory/`.

**Spec:** `docs/superpowers/specs/2026-10-16-factory-accurate-usage-design.md`

## Global Constraints

- Commands run from the repo root as `cargo check -j4 --manifest-path tools/factory/Cargo.toml` and `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p <crate> <test_name>`. Never `--workspace`, never omit `-j4`, never set `CARGO_TARGET_DIR`.
- No panicking unwrap calls in production code. `.expect("...")` only inside tests.
- Comments are short full sentences ending in a period. No emojis.
- Board JSON stays compatible with `tools/factory-server`. This plan does not change the board. `usage.jsonl` fields are all optional on read.
- The live budget kill keeps using the per-message sum (`StreamState::usage()`). Only the final tally changes.
- No dollar amounts, buckets or dashboard changes.
- The builder never commits. The driver commits. The commit steps below are written for a human running the plan by hand: pathspec commit only (`git add <paths> && git commit --only -m "..." -- <paths>`), message through the skill `dev-workflow-skills:1-git-commit`, never `git add -A` or `git add .`.

## Review Focus

1. A result usage that is missing, smaller or zero must not lower the final usage. Pinned in Task 1.
2. A run killed by a budget, stall or turn limit has no `result` event. It must fall back to the message sum and still log a line whose `stop` says why. Pinned in Tasks 1 and 2.
3. Old `usage.jsonl` lines (no new fields), a garbage line and new lines in one file must all load, with the job flagged as under-counted. Pinned in Task 3.
4. A failed run (`AgentRun::failed`, all counters zero) still writes a line with `stop` `failed`. Pinned in Task 2.
5. Huge counters must not overflow `start_ctx`. Pinned in Task 1.

## File Structure

```
tools/factory/crates/factory-agent/src/stream.rs    final_usage(), start_ctx(), first-message capture
tools/factory/crates/factory-agent/src/run.rs       AgentRun.start_ctx, StopReason::as_str
tools/factory/crates/factory-agent/src/claude.rs    use final_usage and start_ctx for AgentRun
tools/factory/crates/factory-agent/tests/stream.rs  stream merge tests
tools/factory/crates/factory-engine/src/ports.rs    usage_line(), run_agent logs the new fields
tools/factory/crates/factory-engine/tests/runloop.rs  usage.jsonl fields test
tools/factory/crates/factory-cli/src/tokens.rs      new record fields, render_detail, under_counted
tools/factory/crates/factory-cli/src/commands.rs    factory tokens prints detail and the note
docs/superpowers/specs/2026-10-16-factory-accurate-usage-design.md  field table: task becomes tasks_done
.claude/software-factory.md                         rulings and token section
```

---

### Task 1: Final usage and start-up context in factory-agent

**Files:**
- Modify: `tools/factory/crates/factory-agent/src/stream.rs`
- Modify: `tools/factory/crates/factory-agent/src/run.rs`
- Modify: `tools/factory/crates/factory-agent/src/claude.rs`
- Modify: `tools/factory/crates/factory-agent/tests/stream.rs`
- Modify: `.claude/software-factory.md` (Rulings section)

**Interfaces:**
- Produces: `StreamState::final_usage(&self) -> Usage`, `StreamState::start_ctx(&self) -> u64`, `AgentRun.start_ctx: u64` (0 in `AgentRun::done`), `StopReason::as_str(self) -> &'static str` returning `done`, `budget-stop`, `stalled`, `turn-limit` or `failed`.
- Consumes: existing `Usage::max_with`, `StreamState::usage`.

- [ ] **Step 1: Write the failing tests**

Append to `tools/factory/crates/factory-agent/tests/stream.rs`:

```rust
fn feed_lines(lines: &[&str]) -> StreamState {
    let mut state = StreamState::default();
    for line in lines {
        state.feed(line);
    }
    state
}

const MSG_1: &str = r#"{"type":"assistant","message":{"id":"msg_1","role":"assistant","content":[],"usage":{"input_tokens":10,"cache_creation_input_tokens":100,"cache_read_input_tokens":1000,"output_tokens":1}},"session_id":"s"}"#;
const MSG_2: &str = r#"{"type":"assistant","message":{"id":"msg_2","role":"assistant","content":[],"usage":{"input_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":1100,"output_tokens":2}},"session_id":"s"}"#;

#[test]
fn final_usage_takes_the_larger_result_output() {
    let result = r#"{"type":"result","subtype":"success","is_error":false,"result":"ok","session_id":"s","usage":{"input_tokens":15,"cache_creation_input_tokens":100,"cache_read_input_tokens":2100,"output_tokens":500}}"#;
    let state = feed_lines(&[MSG_1, MSG_2, result]);
    // The live sum keeps the partial snapshots, the final usage uses the result event.
    assert_eq!(state.usage().output_tokens, 3);
    assert_eq!(state.final_usage().output_tokens, 500);
    assert_eq!(state.final_usage().cache_read_input_tokens, 2100);
}

#[test]
fn final_usage_never_drops_below_the_message_sum() {
    let result = r#"{"type":"result","subtype":"success","is_error":false,"result":"ok","session_id":"s","usage":{"input_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":0}}"#;
    let state = feed_lines(&[MSG_1, MSG_2, result]);
    assert_eq!(state.final_usage(), state.usage());
}

#[test]
fn final_usage_without_a_result_event_is_the_message_sum() {
    let state = feed_lines(&[MSG_1, MSG_2]);
    assert!(!state.has_result());
    assert_eq!(state.final_usage(), state.usage());
    assert_eq!(state.final_usage().output_tokens, 3);
}

#[test]
fn final_usage_takes_each_field_independently() {
    // The result is larger in cache read only, the message sum is larger in input.
    let result = r#"{"type":"result","subtype":"success","is_error":false,"result":"ok","session_id":"s","usage":{"input_tokens":1,"cache_creation_input_tokens":100,"cache_read_input_tokens":9000,"output_tokens":3}}"#;
    let state = feed_lines(&[MSG_1, MSG_2, result]);
    let u = state.final_usage();
    assert_eq!(u.input_tokens, 15);
    assert_eq!(u.cache_read_input_tokens, 9000);
}

#[test]
fn start_ctx_is_the_first_messages_cache_creation_plus_read() {
    let state = feed_lines(&[MSG_1, MSG_2]);
    assert_eq!(state.start_ctx(), 100 + 1000);
}

#[test]
fn start_ctx_is_zero_without_messages() {
    assert_eq!(StreamState::default().start_ctx(), 0);
}

#[test]
fn start_ctx_does_not_overflow() {
    let big = format!(
        r#"{{"type":"assistant","message":{{"id":"m","role":"assistant","content":[],"usage":{{"input_tokens":0,"cache_creation_input_tokens":{max},"cache_read_input_tokens":{max},"output_tokens":0}}}}}}"#,
        max = u64::MAX
    );
    let state = feed_lines(&[&big]);
    assert_eq!(state.start_ctx(), u64::MAX);
}

#[test]
fn stop_reason_names_are_stable() {
    use factory_agent::StopReason;
    assert_eq!(StopReason::Done.as_str(), "done");
    assert_eq!(StopReason::BudgetStop.as_str(), "budget-stop");
    assert_eq!(StopReason::Stalled.as_str(), "stalled");
    assert_eq!(StopReason::TurnLimit.as_str(), "turn-limit");
    assert_eq!(StopReason::Failed.as_str(), "failed");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-agent --test stream final_usage`
Expected: compile error, `final_usage` / `start_ctx` / `as_str` not found.

- [ ] **Step 3: Implement**

In `stream.rs`, add a field and capture it. In the struct:

```rust
    first_usage: Option<Usage>,
```

In `on_assistant`, inside `if let Some(id) = ...` after `let seen = ...;`:

```rust
            if self.first_usage.is_none() {
                self.first_usage = Some(seen);
            }
```

Add the methods after `usage()`:

```rust
    /// Usage of a finished run: per field, the larger of the message sum and the result event.
    /// The message snapshots carry partial output counts, the result event carries the run total.
    pub fn final_usage(&self) -> Usage {
        let sum = self.usage();
        match self.result_usage {
            Some(result) => sum.max_with(&result),
            None => sum,
        }
    }

    /// Context the run started with: cache creation plus cache read of the first message.
    pub fn start_ctx(&self) -> u64 {
        self.first_usage.map_or(0, |u| u.cache_creation_input_tokens.saturating_add(u.cache_read_input_tokens))
    }
```

In `run.rs`, add to `AgentRun` the field `pub start_ctx: u64,` (after `turns`), set `start_ctx: 0,` in `AgentRun::done`, and add:

```rust
impl StopReason {
    /// Stable name for logs.
    pub fn as_str(self) -> &'static str {
        match self {
            StopReason::Done => "done",
            StopReason::BudgetStop => "budget-stop",
            StopReason::Stalled => "stalled",
            StopReason::TurnLimit => "turn-limit",
            StopReason::Failed => "failed",
        }
    }
}
```

In `claude.rs`, change `let usage = state.usage();` (after the child exits) to `let usage = state.final_usage();`, and add `start_ctx: state.start_ctx(),` to the `AgentRun { .. }` literal. Leave the live `state.usage().weighted()` in the read loop unchanged.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-agent`
Expected: all pass, including the existing `stream_dedups_message_ids` (result equals the sum there).

- [ ] **Step 5: Probe a real stream**

Run one tiny real run and compare totals (a few cents of subscription use):

```bash
mkdir -p /tmp/claude-1000/probe && cd /tmp/claude-1000/probe
echo 'Use the Agent tool once to ask a helper to answer 1+1, then reply with the answer.' \
  | claude -p --output-format stream-json --verbose --model haiku > stream.jsonl
python3 -I - <<'E'
import json
sum_ = {}
res = None
for l in open('stream.jsonl'):
    try: j = json.loads(l)
    except Exception: continue
    if j.get('type') == 'assistant':
        m = j['message']; sum_[m['id']] = m.get('usage', {})
    if j.get('type') == 'result': res = j.get('usage')
tot = {k: sum(u.get(k, 0) for u in sum_.values()) for k in ('input_tokens','cache_creation_input_tokens','cache_read_input_tokens','output_tokens')}
print('message sum:', tot)
print('result     :', res)
E
```

Record the two lines in `.claude/software-factory.md`, section Rulings, as one line: `Ruling (2026-10-16): probe of a real stream: message sum <...>, result usage <...>. The result event covers <the whole run including the helper | only the main thread>. final_usage takes the larger per field.` If `claude` cannot be run from this session, write `not probed, unit tests only` and say so in the reply.

- [ ] **Step 6: Check and commit**

Run: `cargo check -j4 --manifest-path tools/factory/Cargo.toml` (expected: clean), then commit the changed paths with the skill message `fix(factory): count the result event usage in the run total`.

---

### Task 2: Per-run record in usage.jsonl

**Files:**
- Modify: `tools/factory/crates/factory-engine/src/ports.rs`
- Modify: `tools/factory/crates/factory-engine/tests/runloop.rs`
- Modify: `docs/superpowers/specs/2026-10-16-factory-accurate-usage-design.md`

**Interfaces:**
- Consumes: `AgentRun.start_ctx`, `StopReason::as_str` from Task 1; `AgentSpec.role`, `.model`, `.resume`; `JobView.round`, `.task_index`; `now()` in `ports.rs`.
- Produces: each `usage.jsonl` line has the existing keys (`job`, `role`, `input`, `cache_creation`, `output`, `cache_read`) plus `ts` (u64 seconds), `model` (string or null), `turns`, `resumed`, `stop`, `round`, `start_ctx`, `usage_v` (always 2), and `tasks_done` on builder lines only.
- Note: the spec table called the field `task`. This plan logs `tasks_done` (the board `task_index`, the number of finished tasks when the run starts), because the engine cannot tell a task-N run from a fix round at that point, and the raw count is unambiguous. Step 1 updates the spec to match.

- [ ] **Step 1: Update the spec table**

In `docs/superpowers/specs/2026-10-16-factory-accurate-usage-design.md`, replace the row

`| `task` | job `task_index` plus 1, builder runs of a task plan only |`

with

`| `tasks_done` | job `task_index` at the start of the run (finished tasks), builder runs only |`

and replace `and `task` for builder runs of a task plan` in the acceptance criteria with `and `tasks_done` for builder runs`.

- [ ] **Step 2: Write the failing tests**

Add at the end of `ports.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use factory_agent::{AgentRun, AgentSpec, StopReason, Usage};

    fn run_with(stop: StopReason) -> AgentRun {
        AgentRun {
            usage: Usage { input_tokens: 1, cache_creation_input_tokens: 2, cache_read_input_tokens: 3, output_tokens: 4 },
            turns: 7,
            start_ctx: 55,
            stop_reason: stop,
            ..AgentRun::done("x")
        }
    }

    #[test]
    fn usage_line_has_the_counters_and_the_run_details() {
        let mut spec = AgentSpec::new("001", "builder", "/w", "p");
        spec.model = Some("sonnet".to_string());
        spec.resume = Some("sess".to_string());
        let line = usage_line("001", &spec, &run_with(StopReason::Done), 2, 1, 1_700_000_000);
        assert_eq!(line["job"], "001");
        assert_eq!(line["role"], "builder");
        assert_eq!(line["input"], 1);
        assert_eq!(line["cache_creation"], 2);
        assert_eq!(line["cache_read"], 3);
        assert_eq!(line["output"], 4);
        assert_eq!(line["ts"], 1_700_000_000u64);
        assert_eq!(line["model"], "sonnet");
        assert_eq!(line["turns"], 7);
        assert_eq!(line["resumed"], true);
        assert_eq!(line["stop"], "done");
        assert_eq!(line["round"], 2);
        assert_eq!(line["start_ctx"], 55);
        assert_eq!(line["usage_v"], 2);
        assert_eq!(line["tasks_done"], 1);
    }

    #[test]
    fn tasks_done_is_written_for_builder_lines_only() {
        let spec = AgentSpec::new("001", "reviewer", "/w", "p");
        let line = usage_line("001", &spec, &run_with(StopReason::Done), 1, 3, 1);
        assert!(line.get("tasks_done").is_none(), "{line}");
        assert!(line["model"].is_null());
        assert_eq!(line["resumed"], false);
    }

    #[test]
    fn a_killed_run_names_its_stop_reason() {
        let spec = AgentSpec::new("001", "builder", "/w", "p");
        let line = usage_line("001", &spec, &run_with(StopReason::BudgetStop), 1, 0, 1);
        assert_eq!(line["stop"], "budget-stop");
    }

    #[test]
    fn a_failed_run_still_gets_a_line_with_zero_counters() {
        let spec = AgentSpec::new("001", "builder", "/w", "p");
        let line = usage_line("001", &spec, &AgentRun::failed("boom"), 1, 0, 1);
        assert_eq!(line["stop"], "failed");
        assert_eq!(line["input"], 0);
        assert_eq!(line["output"], 0);
        assert_eq!(line["usage_v"], 2);
    }
}
```

Add to `tools/factory/crates/factory-engine/tests/runloop.rs`, after `every_agent_run_appends_a_usage_line_with_its_role`:

```rust
#[test]
fn usage_lines_carry_the_run_details() {
    let fx = common::SeriesFixture::new("fine\nVERDICT: APPROVE");
    let plan = fx.plan_file("plan-01-a.md", 1, "none");
    let job = create_job(&fx.driver, "hello", "say hello", Some(&plan), None).expect("create");
    assert_eq!(run_job(&fx.driver, &job.id).expect("run"), RunEnd::Merged);
    let text = std::fs::read_to_string(fx.driver.job_dir(&job).join("usage.jsonl")).expect("usage log");
    let lines: Vec<serde_json::Value> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    assert!(!lines.is_empty());
    for v in &lines {
        assert_eq!(v["usage_v"], 2, "{v}");
        assert!(v["ts"].as_u64().unwrap_or(0) > 0, "{v}");
        assert_eq!(v["stop"], "done", "{v}");
        assert!(v["round"].as_u64().unwrap_or(0) >= 1, "{v}");
        assert!(v["resumed"].is_boolean(), "{v}");
        assert_eq!(v.get("tasks_done").is_some(), v["role"] == "builder", "{v}");
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine usage_line`
Expected: compile error, `usage_line` not found.

- [ ] **Step 4: Implement**

In `ports.rs`, replace `log_usage` and its call. The call in `run_agent` becomes `self.log_usage(&spec, &run);` (keep `let u = run.usage;`, the board tally still needs it). The method and the new free function:

```rust
    /// Appends one usage line per agent run to `usage.jsonl` in the job folder, for `factory tokens`.
    /// A write error is ignored, because the report is informational.
    fn log_usage(&self, spec: &AgentSpec, run: &AgentRun) {
        use std::io::Write;
        // A board read error falls back to round 1 and zero tasks, the line is still worth writing.
        let (round, tasks_done) = self.job().map_or((1, 0), |j| (j.round, j.task_index));
        let line = usage_line(&self.job_id, spec, run, round, tasks_done, now().max(0.0) as u64);
        let file = std::fs::OpenOptions::new().create(true).append(true).open(self.job_dir.join("usage.jsonl"));
        if let Ok(mut f) = file {
            let _ = writeln!(f, "{line}");
        }
    }
```

```rust
/// Builds one `usage.jsonl` line. `usage_v` 2 marks lines with the corrected output count.
pub(crate) fn usage_line(job_id: &str, spec: &AgentSpec, run: &AgentRun, round: u32, tasks_done: u32, ts: u64) -> serde_json::Value {
    let u = &run.usage;
    let mut line = serde_json::json!({
        "job": job_id,
        "role": spec.role,
        "input": u.input_tokens,
        "cache_creation": u.cache_creation_input_tokens,
        "output": u.output_tokens,
        "cache_read": u.cache_read_input_tokens,
        "ts": ts,
        "model": spec.model,
        "turns": run.turns,
        "resumed": spec.resume.is_some(),
        "stop": run.stop_reason.as_str(),
        "round": round,
        "start_ctx": run.start_ctx,
        "usage_v": 2,
    });
    if spec.role == "builder" {
        if let Some(obj) = line.as_object_mut() {
            obj.insert("tasks_done".to_string(), serde_json::json!(tasks_done));
        }
    }
    line
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine`
Expected: all pass, including the existing roles test.

- [ ] **Step 6: Check and commit**

Run: `cargo check -j4 --manifest-path tools/factory/Cargo.toml` (clean). Commit `ports.rs`, `runloop.rs` and the spec file with the skill message `feat(factory): log per-run details in usage.jsonl`.

---

### Task 3: Per-run table and the under-count note in factory tokens

**Files:**
- Modify: `tools/factory/crates/factory-cli/src/tokens.rs`
- Modify: `tools/factory/crates/factory-cli/src/commands.rs`
- Modify: `.claude/software-factory.md` (Token baseline section)

**Interfaces:**
- Consumes: `usage.jsonl` lines from Task 2 (all new keys optional).
- Produces: `UsageRecord` gains `usage_v: u64`, `model: String`, `turns: u64`, `resumed: bool`, `stop: String`, `round: u64`, `tasks_done: Option<u64>`, `start_ctx: u64`, `ts: u64`, and derives `Default`. `pub fn under_counted(records: &[UsageRecord]) -> bool` (any record with `usage_v` below 2). `pub fn render_detail(records: &[UsageRecord]) -> String`.

- [ ] **Step 1: Write the failing tests**

In the `tests` module of `tokens.rs`, replace the `rec` helper with the version below and add the new tests:

```rust
    fn rec(role: &str, input: u64, cc: u64, out: u64, cr: u64) -> UsageRecord {
        UsageRecord { job: "001".into(), role: role.into(), input, cache_creation: cc, output: out, cache_read: cr, usage_v: 2, ..Default::default() }
    }

    #[test]
    fn old_lines_are_flagged_as_under_counted() {
        let old = UsageRecord { usage_v: 0, ..rec("builder", 1, 1, 1, 1) };
        assert!(under_counted(&[rec("reviewer", 1, 1, 1, 1), old]));
        assert!(!under_counted(&[rec("reviewer", 1, 1, 1, 1)]));
        assert!(!under_counted(&[]));
    }

    #[test]
    fn load_reads_old_new_and_garbage_lines_together() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let log = dir.path().join("usage.jsonl");
        std::fs::write(
            &log,
            concat!(
                "{\"job\":\"001\",\"role\":\"builder\",\"input\":1,\"cache_creation\":2,\"output\":3,\"cache_read\":40}\n",
                "garbage line\n",
                "{\"job\":\"001\",\"role\":\"builder\",\"input\":1,\"cache_creation\":2,\"output\":300,\"cache_read\":40,",
                "\"ts\":5,\"model\":\"sonnet\",\"turns\":9,\"resumed\":true,\"stop\":\"done\",\"round\":2,\"tasks_done\":1,\"start_ctx\":77,\"usage_v\":2}\n"
            ),
        )?;
        let records = load_usage(&log, "001")?;
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].usage_v, 0);
        assert_eq!(records[0].tasks_done, None);
        assert_eq!(records[1].usage_v, 2);
        assert_eq!(records[1].turns, 9);
        assert!(records[1].resumed);
        assert_eq!(records[1].tasks_done, Some(1));
        assert_eq!(records[1].start_ctx, 77);
        assert!(under_counted(&records));
        Ok(())
    }

    #[test]
    fn detail_lists_one_row_per_run_with_its_weighted_cost() {
        let r = UsageRecord { turns: 9, resumed: true, round: 2, tasks_done: Some(1), stop: "done".into(), ..rec("builder", 100, 200, 300, 1_000) };
        let text = render_detail(&[r, rec("reviewer", 1, 0, 0, 0)]);
        assert!(text.contains("builder"), "{text}");
        assert!(text.contains("resumed"), "{text}");
        assert!(text.contains("turns 9"), "{text}");
        assert!(text.contains("weighted 700"), "{text}");
        assert!(text.contains("reviewer"), "{text}");
    }

    #[test]
    fn detail_of_nothing_is_empty() {
        assert_eq!(render_detail(&[]), "");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli tokens::tests`
Expected: compile errors, new fields and functions not found.

- [ ] **Step 3: Implement**

In `tokens.rs`, derive `Default` on `UsageRecord` and add the fields:

```rust
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UsageRecord {
    pub job: String,
    pub role: String,
    pub input: u64,
    pub cache_creation: u64,
    pub output: u64,
    pub cache_read: u64,
    /// 2 for lines with the corrected output count, 0 for old lines.
    pub usage_v: u64,
    pub model: String,
    pub turns: u64,
    pub resumed: bool,
    pub stop: String,
    pub round: u64,
    pub tasks_done: Option<u64>,
    pub start_ctx: u64,
    pub ts: u64,
}
```

In `load_usage`, extend the `out.push(UsageRecord { ... })` with:

```rust
            usage_v: n("usage_v"),
            model: v["model"].as_str().unwrap_or("").to_string(),
            turns: n("turns"),
            resumed: v["resumed"].as_bool().unwrap_or(false),
            stop: v["stop"].as_str().unwrap_or("").to_string(),
            round: n("round"),
            tasks_done: v["tasks_done"].as_u64(),
            start_ctx: n("start_ctx"),
            ts: n("ts"),
```

Add the functions:

```rust
/// True when any line predates the corrected output count.
pub fn under_counted(records: &[UsageRecord]) -> bool {
    records.iter().any(|r| r.usage_v < 2)
}

/// One row per run in log order, so resumes, fix rounds and tasks show up as separate costs.
pub fn render_detail(records: &[UsageRecord]) -> String {
    if records.is_empty() {
        return String::new();
    }
    let mut out = String::from("  runs\n");
    for (i, r) in records.iter().enumerate() {
        let task = r.tasks_done.map_or(String::new(), |n| format!(" done {n}"));
        let resumed = if r.resumed { " resumed" } else { "" };
        out.push_str(&format!(
            "  {:<3} {:<10} round {}{task}{resumed} turns {} weighted {}\n",
            i + 1,
            r.role,
            r.round.max(1),
            r.turns,
            weighted(r)
        ));
    }
    out
}
```

In `commands.rs` `tokens()`, after `let report = ...;` build the full text:

```rust
    let mut text = format!(
        "input {}\ncache_creation {}\noutput {}\ncache_read {}\nweighted {weighted} ({pct}% of {budget})\n{}",
        t.input, t.cache_creation, t.output, t.cache_read, report.trim_end()
    );
    let detail = crate::tokens::render_detail(&records);
    if !detail.is_empty() {
        text.push('\n');
        text.push_str(detail.trim_end());
    }
    if crate::tokens::under_counted(&records) {
        text.push_str("\n  note: output under-counted, some lines predate the corrected count");
    }
    Ok(text)
```

(replacing the old `Ok(format!(...))`).

In `.claude/software-factory.md`, section "Token baseline", add one paragraph after the `factory tokens <id>` paragraph:

`Since 2026-10-16 each run's output is the larger of the message sum and the result event, and each usage.jsonl line has ts, model, turns, resumed, stop, round, start_ctx, usage_v 2 and, for builder runs, tasks_done. Lines without usage_v are old and their output counts are under-counted, so factory tokens prints a note for such a job. factory tokens also prints one row per run. Jobs 001 to 011 are all old.`

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli`
Expected: all pass, including the existing render tests and e2e.

- [ ] **Step 5: Final checks and commit**

Run `cargo check -j4 --manifest-path tools/factory/Cargo.toml`, then `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-agent`, `-p factory-engine` and `-p factory-cli` (one crate per call). Then run `cargo build -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli` and `tools/factory/target/debug/factory tokens 010` from the repo root. Expected: the totals, a runs table (old lines print round 1, turns 0) and the under-count note. Commit `tokens.rs`, `commands.rs` and `.claude/software-factory.md` with the skill message `feat(factory): show per-run rows in factory tokens`.

Next plan: none
