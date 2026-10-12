# Software Factory Plan 02: Rust Event Server

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A small Rust program that tails the factory event log, reads the job board, and serves one JSON state document that the dashboard polls.

**Architecture:** A background thread tails `.superpowers/factory/events.jsonl` by byte offset and folds events into an in-memory agent registry. Each HTTP request reads `board.json` fresh, joins the board with the registry, and returns `GET /api/state` as JSON. Only `127.0.0.1` is bound, only `GET` is served, and the `Host` header is checked.

**Tech Stack:** Rust 2021, `tiny_http`, `serde`, `serde_json`, `tempfile` (dev only). No async runtime, no file-watcher crate.

**Spec:** [.claude/software-factory.md](../../../../.claude/software-factory.md). Plan index: [00-plan-index.md](00-plan-index.md). Plan 01 must be finished first: this plan reads the files that plan 01 produces.

## Global Constraints

- The crate lives in `tools/factory-server/` with its own `[workspace]` table. It is not added to the root `Cargo.toml` members, and nothing in `src-tauri` or the Rocket app depends on it.
- All cargo commands: `cargo <cmd> -j4 --manifest-path tools/factory-server/Cargo.toml`. Never `--workspace` or `--all`. The crate gets its own `target/` (already covered by the root `.gitignore`).
- No `unwrap` calls anywhere, tests included. The repo's `rust-safety-pretool` hook blocks the pattern in every Rust write. Use `?`, `match`, `let ... else` or `expect("reason")` in tests.
- The server is read-only: it never writes `board.json`, the ledgers or the event log.
- Bind `127.0.0.1` only. Serve `GET` only. Reject any `Host` header other than `127.0.0.1:<port>` or `localhost:<port>` with 403.
- The state JSON field names are a contract for plan 03: `generated_at`, `board_error`, `jobs[]` (all board fields plus `agents[]` and `seconds_in_stage`), `unassigned_agents[]`, `recent[]`.
- Commits use the `dev-workflow-skills:1-git-commit` skill, a pathspec, conventional format. `cargo check -j4` for the root workspace is run by the commit gate only for staged files outside this crate, so each task also runs the crate's own check.
- No cost tracking.

## Gate Before Starting

Read the `## Probe results` section of `00-plan-index.md`. If it says `PostToolUse` did not carry `agent_id` inside subagents, continue anyway: the server still works, the cards show agent start and stop without the live tool line. Tell the user once, at the start, that the live tool detail will be missing.

## Review Focus

- A half-written last line in `events.jsonl` must not be parsed or lost: it is held until its newline arrives. Tested in Task 1.
- The log being truncated or deleted and recreated must not stop the tail. Tested in Task 1.
- A garbage line must be skipped and counted, never crash the server. Tested in Task 1.
- A missing `board.json` means "no jobs", an invalid one must show `board_error`, neither may crash. Tested in Task 2.
- An agent that never sent a stop (crashed session) must show as `stale`, not as running forever. Tested in Task 2.
- A request from a web page on another origin (DNS rebinding) must get 403. Tested in Task 3.

---

### Task 1: Crate scaffold, models, event registry and log tailer

**Files:**
- Create: `tools/factory-server/Cargo.toml`
- Create: `tools/factory-server/src/main.rs`
- Create: `tools/factory-server/src/model.rs`
- Create: `tools/factory-server/src/events.rs`

**Interfaces:**
- Consumes: the JSON lines written by `.claude/factory/hooks/log-event` (plan 01 task 1): `ts`, `hook_event_name`, `session_id`, `agent_id`, `agent_type`, `tool_name`, `tool_target`, `cwd`, `last_message`.
- Produces: `model::{Event, Job, Board, AgentState, RecentEvent}`, `events::Registry` with `new()`, `ingest_line(&mut self, &str) -> bool`, `ingest(&mut self, &Event)`, `agents(&self) -> Vec<AgentState>`, `recent(&self) -> Vec<RecentEvent>` (newest first) and the public field `skipped_lines: u64`, and `events::Tailer` with `new(PathBuf)` and `poll(&mut self) -> Vec<String>`.

- [ ] **Step 1: Create the manifest and the module skeleton**

`tools/factory-server/Cargo.toml`:

```toml
[package]
name = "factory-server"
version = "0.1.0"
edition = "2021"
description = "Local dashboard server for Rocket's software factory"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tiny_http = "0.12"

[dev-dependencies]
tempfile = "3"

[workspace]
```

`tools/factory-server/src/main.rs` (replaced in Task 3):

```rust
mod events;
mod model;

fn main() {}
```

`tools/factory-server/src/model.rs`:

```rust
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One line of `events.jsonl`, as written by the log-event hook.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Event {
    pub ts: f64,
    pub hook_event_name: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub agent_type: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_target: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub last_message: Option<String>,
}

/// One job in `board.json`, as written by `factory-ctl`. Unknown fields are ignored.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Job {
    pub id: String,
    pub dir: String,
    pub slug: String,
    pub created: String,
    pub feature: String,
    pub branch: String,
    #[serde(default)]
    pub worktree: Option<String>,
    #[serde(default)]
    pub builder_id: Option<String>,
    pub stage: String,
    pub round: u32,
    #[serde(default)]
    pub reason: Option<String>,
    pub stage_since: f64,
    pub reviews: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Board {
    #[serde(default)]
    pub jobs: Vec<Job>,
}

/// What the registry knows about one subagent.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AgentState {
    pub agent_id: String,
    pub agent_type: String,
    /// "running" or "done". The state builder may report "stale" instead of "running".
    pub status: String,
    pub started_ts: f64,
    pub ended_ts: Option<f64>,
    pub last_tool: Option<String>,
    pub last_target: Option<String>,
    pub last_tool_ts: Option<f64>,
    pub tool_count: u32,
    pub last_message: Option<String>,
    pub cwd: Option<String>,
}

/// One entry of the live feed.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RecentEvent {
    pub ts: f64,
    /// "start", "stop" or "tool".
    pub kind: String,
    pub agent_id: Option<String>,
    pub agent_type: Option<String>,
    pub tool: Option<String>,
    pub target: Option<String>,
}
```

- [ ] **Step 2: Write the failing tests**

Create `tools/factory-server/src/events.rs` with only the tests first (the types they use do not exist yet, so it fails to compile, which is the expected failure):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const START: &str = r#"{"ts":100.0,"hook_event_name":"SubagentStart","agent_id":"a1","agent_type":"factory-builder","cwd":"/wt"}"#;
    const TOOL: &str = r#"{"ts":101.5,"hook_event_name":"PostToolUse","agent_id":"a1","tool_name":"Edit","tool_target":"src/App.tsx","cwd":"/wt"}"#;
    const STOP: &str = r#"{"ts":120.0,"hook_event_name":"SubagentStop","agent_id":"a1","last_message":"done"}"#;

    #[test]
    fn start_tool_stop_updates_one_agent() {
        let mut registry = Registry::new();
        assert!(registry.ingest_line(START));
        assert!(registry.ingest_line(TOOL));
        let agents = registry.agents();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].status, "running");
        assert_eq!(agents[0].agent_type, "factory-builder");
        assert_eq!(agents[0].last_tool.as_deref(), Some("Edit"));
        assert_eq!(agents[0].last_target.as_deref(), Some("src/App.tsx"));
        assert_eq!(agents[0].tool_count, 1);
        assert!(registry.ingest_line(STOP));
        let agents = registry.agents();
        assert_eq!(agents[0].status, "done");
        assert_eq!(agents[0].ended_ts, Some(120.0));
        assert_eq!(agents[0].last_message.as_deref(), Some("done"));
    }

    #[test]
    fn a_restart_after_stop_runs_again_and_resets_the_start_time() {
        let mut registry = Registry::new();
        registry.ingest_line(START);
        registry.ingest_line(STOP);
        registry.ingest_line(
            r#"{"ts":200.0,"hook_event_name":"SubagentStart","agent_id":"a1","agent_type":"factory-builder"}"#,
        );
        let agents = registry.agents();
        assert_eq!(agents[0].status, "running");
        assert_eq!(agents[0].started_ts, 200.0);
        assert_eq!(agents[0].ended_ts, None);
    }

    #[test]
    fn a_tool_event_without_an_agent_id_is_ignored() {
        let mut registry = Registry::new();
        registry.ingest_line(r#"{"ts":1.0,"hook_event_name":"PostToolUse","tool_name":"Bash"}"#);
        assert!(registry.agents().is_empty());
        assert!(registry.recent().is_empty());
    }

    #[test]
    fn garbage_lines_are_skipped_and_counted() {
        let mut registry = Registry::new();
        assert!(!registry.ingest_line("not json"));
        assert!(!registry.ingest_line(r#"{"hook_event_name":"Stop"}"#));
        assert_eq!(registry.skipped_lines, 2);
        assert!(registry.agents().is_empty());
    }

    #[test]
    fn recent_is_newest_first_and_capped() {
        let mut registry = Registry::new();
        for i in 0..60 {
            registry.ingest_line(&format!(
                r#"{{"ts":{i}.0,"hook_event_name":"PostToolUse","agent_id":"a1","tool_name":"Read"}}"#
            ));
        }
        let recent = registry.recent();
        assert_eq!(recent.len(), 40);
        assert_eq!(recent[0].ts, 59.0);
    }

    #[test]
    fn tailer_holds_a_partial_line_until_its_newline() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("events.jsonl");
        let mut tailer = Tailer::new(path.clone());
        assert!(tailer.poll().is_empty(), "a missing file gives no lines");
        let mut file = std::fs::File::create(&path)?;
        file.write_all(b"{\"a\":1}\n{\"b\":")?;
        assert_eq!(tailer.poll(), vec![r#"{"a":1}"#.to_string()]);
        file.write_all(b"2}\n")?;
        assert_eq!(tailer.poll(), vec![r#"{"b":2}"#.to_string()]);
        assert!(tailer.poll().is_empty());
        Ok(())
    }

    #[test]
    fn tailer_starts_over_when_the_file_is_truncated() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("events.jsonl");
        std::fs::write(&path, "{\"a\":1}\n{\"b\":2}\n")?;
        let mut tailer = Tailer::new(path.clone());
        assert_eq!(tailer.poll().len(), 2);
        std::fs::write(&path, "{\"c\":3}\n")?;
        assert_eq!(tailer.poll(), vec![r#"{"c":3}"#.to_string()]);
        Ok(())
    }
}
```

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml`
Expected: compile errors for `Registry` and `Tailer` not found.

- [ ] **Step 3: Implement the registry and the tailer**

Put this above the `#[cfg(test)]` block in `tools/factory-server/src/events.rs`:

```rust
use std::collections::{BTreeMap, VecDeque};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use crate::model::{AgentState, Event, RecentEvent};

const MAX_RECENT: usize = 40;

/// Folds hook events into per-agent state and a short live feed.
#[derive(Debug, Default)]
pub struct Registry {
    agents: BTreeMap<String, AgentState>,
    recent: VecDeque<RecentEvent>,
    pub skipped_lines: u64,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses one JSONL line and applies it. Returns false for a line that is not a valid event.
    pub fn ingest_line(&mut self, line: &str) -> bool {
        match serde_json::from_str::<Event>(line) {
            Ok(event) => {
                self.ingest(&event);
                true
            }
            Err(_) => {
                self.skipped_lines += 1;
                false
            }
        }
    }

    pub fn ingest(&mut self, event: &Event) {
        match event.hook_event_name.as_str() {
            "SubagentStart" => self.on_start(event),
            "SubagentStop" => self.on_stop(event),
            "PostToolUse" => self.on_tool(event),
            _ => {}
        }
    }

    pub fn agents(&self) -> Vec<AgentState> {
        self.agents.values().cloned().collect()
    }

    /// Newest first.
    pub fn recent(&self) -> Vec<RecentEvent> {
        self.recent.iter().rev().cloned().collect()
    }

    fn entry(&mut self, event: &Event, agent_id: &str) -> &mut AgentState {
        self.agents
            .entry(agent_id.to_string())
            .or_insert_with(|| AgentState {
                agent_id: agent_id.to_string(),
                agent_type: event.agent_type.clone().unwrap_or_default(),
                status: "running".to_string(),
                started_ts: event.ts,
                ended_ts: None,
                last_tool: None,
                last_target: None,
                last_tool_ts: None,
                tool_count: 0,
                last_message: None,
                cwd: event.cwd.clone(),
            })
    }

    fn on_start(&mut self, event: &Event) {
        let Some(id) = event.agent_id.as_deref() else {
            return;
        };
        let agent = self.entry(event, id);
        if agent.status == "done" {
            agent.started_ts = event.ts;
        }
        agent.status = "running".to_string();
        agent.ended_ts = None;
        if let Some(kind) = event.agent_type.as_deref() {
            if !kind.is_empty() {
                agent.agent_type = kind.to_string();
            }
        }
        self.push_recent(event, "start");
    }

    fn on_stop(&mut self, event: &Event) {
        let Some(id) = event.agent_id.as_deref() else {
            return;
        };
        let agent = self.entry(event, id);
        agent.status = "done".to_string();
        agent.ended_ts = Some(event.ts);
        if let Some(message) = &event.last_message {
            agent.last_message = Some(message.clone());
        }
        self.push_recent(event, "stop");
    }

    fn on_tool(&mut self, event: &Event) {
        // Tool calls of the orchestrator itself carry no agent id and are not tracked.
        let Some(id) = event.agent_id.as_deref() else {
            return;
        };
        let agent = self.entry(event, id);
        agent.tool_count += 1;
        agent.last_tool = event.tool_name.clone();
        agent.last_target = event.tool_target.clone();
        agent.last_tool_ts = Some(event.ts);
        if agent.cwd.is_none() {
            agent.cwd = event.cwd.clone();
        }
        self.push_recent(event, "tool");
    }

    fn push_recent(&mut self, event: &Event, kind: &str) {
        self.recent.push_back(RecentEvent {
            ts: event.ts,
            kind: kind.to_string(),
            agent_id: event.agent_id.clone(),
            agent_type: event.agent_type.clone(),
            tool: event.tool_name.clone(),
            target: event.tool_target.clone(),
        });
        while self.recent.len() > MAX_RECENT {
            self.recent.pop_front();
        }
    }
}

/// Reads new complete lines from a growing file.
#[derive(Debug)]
pub struct Tailer {
    path: PathBuf,
    offset: u64,
    partial: String,
}

impl Tailer {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            offset: 0,
            partial: String::new(),
        }
    }

    /// Returns the complete lines added since the last call. A missing file gives no lines.
    /// A shorter file than before is treated as a new file and read from the start.
    pub fn poll(&mut self) -> Vec<String> {
        let Ok(mut file) = File::open(&self.path) else {
            return Vec::new();
        };
        let Ok(meta) = file.metadata() else {
            return Vec::new();
        };
        let len = meta.len();
        if len < self.offset {
            self.offset = 0;
            self.partial.clear();
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
        let mut lines = Vec::new();
        while let Some(pos) = self.partial.find('\n') {
            let line: String = self.partial.drain(..=pos).collect();
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                lines.push(trimmed.to_string());
            }
        }
        lines
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml`
Expected: 7 tests pass. Warnings about unused items are fine at this stage.

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill with a pathspec commit of `tools/factory-server/Cargo.toml`, `tools/factory-server/Cargo.lock`, `tools/factory-server/src/main.rs`, `tools/factory-server/src/model.rs`, `tools/factory-server/src/events.rs`.
Suggested type: `feat(factory): add the event registry and log tailer`.

---

### Task 2: State builder

**Files:**
- Create: `tools/factory-server/src/state.rs`
- Modify: `tools/factory-server/src/main.rs` (add `mod state;`)

**Interfaces:**
- Consumes: `Registry::agents()`, `Registry::recent()`, `model::{Board, Job, AgentState, RecentEvent}` from Task 1.
- Produces: `state::load_board(&Path) -> (Board, Option<String>)`, `state::build_state(&Board, &Registry, now: f64, board_error: Option<String>) -> StateView`, and the serializable `StateView { generated_at, board_error, jobs: Vec<JobView>, unassigned_agents, recent }` and `JobView { #[flatten] job, agents, seconds_in_stage }`. Agent status values: `running`, `done`, `stale`.

Agent-to-job rules, in order: (1) `agent_id == job.builder_id`; (2) the agent's `cwd` is inside `job.worktree`; (3) the agent type belongs to the job's current stage (`factory-spec-writer` to `spec`, `factory-builder` to `build`, the four reviewers to `review`, `factory-approver` to `approve`) and the agent started at or after `job.stage_since`, first matching job on the board. Anything left is unassigned, and is kept only while running or if it ended less than 10 minutes ago. A running agent with no activity for 30 minutes is `stale`.

- [ ] **Step 1: Write the failing tests**

Create `tools/factory-server/src/state.rs` with the tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Registry;
    use crate::model::Job;
    use std::collections::BTreeMap;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn job(id: &str, stage: &str, stage_since: f64) -> Job {
        let mut reviews = BTreeMap::new();
        for role in ["security", "ux", "ui", "code"] {
            reviews.insert(role.to_string(), "pending".to_string());
        }
        Job {
            id: id.to_string(),
            dir: format!("2026-10-13-{id}-x"),
            slug: "x".to_string(),
            created: "2026-10-13".to_string(),
            feature: "feature".to_string(),
            branch: format!("factory/{id}-x"),
            worktree: None,
            builder_id: None,
            stage: stage.to_string(),
            round: 0,
            reason: None,
            stage_since,
            reviews,
        }
    }

    fn start(registry: &mut Registry, ts: f64, id: &str, kind: &str, cwd: &str) {
        registry.ingest_line(&format!(
            r#"{{"ts":{ts},"hook_event_name":"SubagentStart","agent_id":"{id}","agent_type":"{kind}","cwd":"{cwd}"}}"#
        ));
    }

    #[test]
    fn a_missing_board_file_means_no_jobs() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (board, error) = load_board(&dir.path().join("board.json"));
        assert!(board.jobs.is_empty());
        assert!(error.is_none());
        Ok(())
    }

    #[test]
    fn an_invalid_board_file_reports_an_error() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("board.json");
        std::fs::write(&path, "{ nope")?;
        let (board, error) = load_board(&path);
        assert!(board.jobs.is_empty());
        assert!(error.is_some_and(|text| text.contains("invalid")));
        Ok(())
    }

    #[test]
    fn a_valid_board_file_loads() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("board.json");
        std::fs::write(
            &path,
            r#"{"jobs":[{"id":"001","dir":"d","slug":"s","created":"2026-10-13","feature":"f","branch":"b","stage":"build","round":1,"stage_since":5.0,"reviews":{"security":"pass"},"base":"abc"}]}"#,
        )?;
        let (board, error) = load_board(&path);
        assert!(error.is_none());
        assert_eq!(board.jobs[0].round, 1);
        Ok(())
    }

    #[test]
    fn the_builder_id_wins_over_the_stage_rule() {
        let mut board = Board::default();
        let mut first = job("001", "review", 10.0);
        first.builder_id = Some("b1".to_string());
        board.jobs.push(first);
        board.jobs.push(job("002", "build", 10.0));
        let mut registry = Registry::new();
        start(&mut registry, 20.0, "b1", "factory-builder", "/elsewhere");
        let view = build_state(&board, &registry, 30.0, None);
        assert_eq!(view.jobs[0].agents.len(), 1);
        assert!(view.jobs[1].agents.is_empty());
    }

    #[test]
    fn a_worktree_path_attaches_the_agent() {
        let mut board = Board::default();
        board.jobs.push(job("001", "approve", 10.0));
        let mut second = job("002", "approve", 10.0);
        second.worktree = Some("/repo/.claude/worktrees/factory-002".to_string());
        board.jobs.push(second);
        let mut registry = Registry::new();
        start(&mut registry, 20.0, "x1", "other", "/repo/.claude/worktrees/factory-002/src");
        let view = build_state(&board, &registry, 30.0, None);
        assert_eq!(view.jobs[1].agents.len(), 1);
    }

    #[test]
    fn the_stage_rule_attaches_all_reviewers_to_the_review_job() {
        let mut board = Board::default();
        board.jobs.push(job("001", "review", 10.0));
        let mut registry = Registry::new();
        for (i, kind) in ["factory-security-reviewer", "factory-ux-reviewer", "factory-ui-reviewer", "factory-code-reviewer"]
            .iter()
            .enumerate()
        {
            start(&mut registry, 20.0 + i as f64, &format!("r{i}"), kind, "/repo");
        }
        let view = build_state(&board, &registry, 30.0, None);
        assert_eq!(view.jobs[0].agents.len(), 4);
        assert!(view.unassigned_agents.is_empty());
    }

    #[test]
    fn an_agent_from_an_earlier_stage_does_not_attach_to_a_later_job() {
        let mut board = Board::default();
        board.jobs.push(job("002", "spec", 500.0));
        let mut registry = Registry::new();
        start(&mut registry, 100.0, "s1", "factory-spec-writer", "/repo");
        registry.ingest_line(r#"{"ts":150.0,"hook_event_name":"SubagentStop","agent_id":"s1"}"#);
        let view = build_state(&board, &registry, 800.0, None);
        assert!(view.jobs[0].agents.is_empty());
        assert!(view.unassigned_agents.is_empty(), "ended more than 10 minutes ago");
    }

    #[test]
    fn a_silent_running_agent_becomes_stale() {
        let mut board = Board::default();
        board.jobs.push(job("001", "build", 10.0));
        let mut registry = Registry::new();
        start(&mut registry, 20.0, "b1", "factory-builder", "/repo");
        let fresh = build_state(&board, &registry, 100.0, None);
        assert_eq!(fresh.jobs[0].agents[0].status, "running");
        let old = build_state(&board, &registry, 20.0 + 1801.0, None);
        assert_eq!(old.jobs[0].agents[0].status, "stale");
    }

    #[test]
    fn seconds_in_stage_and_the_board_error_are_reported() {
        let mut board = Board::default();
        board.jobs.push(job("001", "build", 100.0));
        let view = build_state(&board, &Registry::new(), 160.0, Some("oops".to_string()));
        assert_eq!(view.jobs[0].seconds_in_stage, 60.0);
        assert_eq!(view.board_error.as_deref(), Some("oops"));
        assert_eq!(view.generated_at, 160.0);
    }

    #[test]
    fn the_state_serializes_with_flattened_job_fields() -> TestResult {
        let mut board = Board::default();
        board.jobs.push(job("001", "build", 100.0));
        let view = build_state(&board, &Registry::new(), 160.0, None);
        let json = serde_json::to_value(&view)?;
        assert_eq!(json["jobs"][0]["id"], "001");
        assert_eq!(json["jobs"][0]["stage"], "build");
        assert!(json["jobs"][0]["agents"].is_array());
        Ok(())
    }
}
```

Add `mod state;` to `main.rs`. Run `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml`.
Expected: compile errors for `load_board` and `build_state`.

- [ ] **Step 2: Implement the state builder**

Put this above the `#[cfg(test)]` block in `tools/factory-server/src/state.rs`:

```rust
use std::path::Path;

use serde::Serialize;

use crate::events::Registry;
use crate::model::{AgentState, Board, Job, RecentEvent};

const STALE_SECS: f64 = 1800.0;
const KEEP_DONE_SECS: f64 = 600.0;

#[derive(Debug, Serialize)]
pub struct JobView {
    #[serde(flatten)]
    pub job: Job,
    pub agents: Vec<AgentState>,
    pub seconds_in_stage: f64,
}

#[derive(Debug, Serialize)]
pub struct StateView {
    pub generated_at: f64,
    pub board_error: Option<String>,
    pub jobs: Vec<JobView>,
    pub unassigned_agents: Vec<AgentState>,
    pub recent: Vec<RecentEvent>,
}

/// Reads `board.json`. A missing file means no jobs. A bad file gives no jobs and an error text.
pub fn load_board(path: &Path) -> (Board, Option<String>) {
    match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<Board>(&text) {
            Ok(board) => (board, None),
            Err(err) => (
                Board::default(),
                Some(format!("board.json is invalid: {err}")),
            ),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => (Board::default(), None),
        Err(err) => (
            Board::default(),
            Some(format!("board.json is unreadable: {err}")),
        ),
    }
}

fn stage_for_agent_type(agent_type: &str) -> Option<&'static str> {
    match agent_type {
        "factory-spec-writer" => Some("spec"),
        "factory-builder" => Some("build"),
        "factory-security-reviewer"
        | "factory-ux-reviewer"
        | "factory-ui-reviewer"
        | "factory-code-reviewer" => Some("review"),
        "factory-approver" => Some("approve"),
        _ => None,
    }
}

fn with_status(mut agent: AgentState, now: f64) -> AgentState {
    if agent.status == "running" {
        let last = agent
            .last_tool_ts
            .unwrap_or(agent.started_ts)
            .max(agent.started_ts);
        if now - last > STALE_SECS {
            agent.status = "stale".to_string();
        }
    }
    agent
}

fn owns_agent(job: &Job, agent: &AgentState) -> bool {
    if job.builder_id.as_deref() == Some(agent.agent_id.as_str()) {
        return true;
    }
    match (&job.worktree, &agent.cwd) {
        (Some(worktree), Some(cwd)) => Path::new(cwd).starts_with(worktree),
        _ => false,
    }
}

fn fits_stage(job: &Job, agent: &AgentState) -> bool {
    stage_for_agent_type(&agent.agent_type) == Some(job.stage.as_str())
        && agent.started_ts >= job.stage_since
}

fn keep_unassigned(agent: &AgentState, now: f64) -> bool {
    match agent.status.as_str() {
        "done" => agent
            .ended_ts
            .is_some_and(|ended| now - ended <= KEEP_DONE_SECS),
        _ => true,
    }
}

pub fn build_state(
    board: &Board,
    registry: &Registry,
    now: f64,
    board_error: Option<String>,
) -> StateView {
    let mut jobs: Vec<JobView> = board
        .jobs
        .iter()
        .map(|job| JobView {
            job: job.clone(),
            agents: Vec::new(),
            seconds_in_stage: (now - job.stage_since).max(0.0),
        })
        .collect();
    let mut unassigned = Vec::new();
    for agent in registry.agents() {
        let agent = with_status(agent, now);
        let index = jobs
            .iter()
            .position(|view| owns_agent(&view.job, &agent))
            .or_else(|| jobs.iter().position(|view| fits_stage(&view.job, &agent)));
        match index {
            Some(i) => jobs[i].agents.push(agent),
            None => {
                if keep_unassigned(&agent, now) {
                    unassigned.push(agent);
                }
            }
        }
    }
    for view in &mut jobs {
        view.agents.sort_by(|a, b| a.started_ts.total_cmp(&b.started_ts));
    }
    unassigned.sort_by(|a, b| a.started_ts.total_cmp(&b.started_ts));
    StateView {
        generated_at: now,
        board_error,
        jobs,
        unassigned_agents: unassigned,
        recent: registry.recent(),
    }
}
```

- [ ] **Step 3: Run the tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml`
Expected: 17 tests pass (7 from task 1, 10 new).

- [ ] **Step 4: Commit**

Use the `dev-workflow-skills:1-git-commit` skill with a pathspec commit of `tools/factory-server/src/state.rs`, `tools/factory-server/src/main.rs`.
Suggested type: `feat(factory): join the board and agent registry into one state view`.

---

### Task 3: HTTP server, arguments and placeholder page

**Files:**
- Create: `tools/factory-server/src/server.rs`
- Create: `tools/factory-server/static/dashboard.html` (placeholder, replaced by plan 03)
- Modify: `tools/factory-server/src/main.rs`

**Interfaces:**
- Consumes: `state::{load_board, build_state}`, `events::{Registry, Tailer}` from Tasks 1 and 2.
- Produces: the program `factory-server [--root PATH] [--port N]` (default root `.`, default port `8765`), serving `GET /` and `GET /dashboard.html` (the page), `GET /api/state` (JSON), `GET /healthz`. Plan 03 adds more static assets next to `dashboard.html`. Exposed items: `server::{App, Reply, route, host_allowed, parse_args, Config, spawn_tailer, serve}`.

- [ ] **Step 1: Write the failing tests and the placeholder page**

`tools/factory-server/static/dashboard.html`:

```html
<!doctype html>
<meta charset="utf-8">
<title>Software factory</title>
<h1>Software factory</h1>
<pre id="out">loading</pre>
<script>
  async function tick() {
    try {
      const response = await fetch("/api/state", { cache: "no-store" });
      document.getElementById("out").textContent = JSON.stringify(await response.json(), null, 2);
    } catch (error) {
      document.getElementById("out").textContent = "server unreachable: " + error.message;
    }
  }
  setInterval(tick, 1000);
  tick();
</script>
```

Create `tools/factory-server/src/server.rs` with the tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const JOB: &str = r#"{"jobs":[{"id":"001","dir":"d","slug":"s","created":"2026-10-13","feature":"f","branch":"b","stage":"spec","round":0,"stage_since":1.0,"reviews":{"security":"pending","ux":"pending","ui":"pending","code":"pending"}}]}"#;

    fn app_with_board(board: Option<&str>) -> Result<(tempfile::TempDir, Arc<App>), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let factory = dir.path().join(".superpowers").join("factory");
        std::fs::create_dir_all(&factory)?;
        if let Some(text) = board {
            std::fs::write(factory.join("board.json"), text)?;
        }
        let app = Arc::new(App {
            root: dir.path().to_path_buf(),
            registry: Mutex::new(Registry::new()),
        });
        Ok((dir, app))
    }

    fn http_get(port: u16, path: &str, host: &str) -> Result<String, std::io::Error> {
        let mut stream = TcpStream::connect(("127.0.0.1", port))?;
        write!(stream, "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n")?;
        let mut out = String::new();
        stream.read_to_string(&mut out)?;
        Ok(out)
    }

    #[test]
    fn parse_args_defaults_and_overrides() {
        let none: Vec<String> = Vec::new();
        let config = parse_args(&none).expect("defaults parse");
        assert_eq!(config.port, 8765);
        assert_eq!(config.root, PathBuf::from("."));
        let args: Vec<String> = ["--root", "/r", "--port", "9000"].iter().map(|s| s.to_string()).collect();
        let config = parse_args(&args).expect("overrides parse");
        assert_eq!(config.port, 9000);
        assert_eq!(config.root, PathBuf::from("/r"));
    }

    #[test]
    fn parse_args_rejects_bad_input() {
        let bad_port: Vec<String> = ["--port", "abc"].iter().map(|s| s.to_string()).collect();
        assert!(parse_args(&bad_port).is_err());
        let missing: Vec<String> = ["--root"].iter().map(|s| s.to_string()).collect();
        assert!(parse_args(&missing).is_err());
        let unknown: Vec<String> = ["--nope"].iter().map(|s| s.to_string()).collect();
        assert!(parse_args(&unknown).is_err());
    }

    #[test]
    fn only_local_hosts_with_the_right_port_are_allowed() {
        assert!(host_allowed(Some("127.0.0.1:8765"), 8765));
        assert!(host_allowed(Some("localhost:8765"), 8765));
        assert!(!host_allowed(Some("evil.example"), 8765));
        assert!(!host_allowed(Some("127.0.0.1:9999"), 8765));
        assert!(!host_allowed(None, 8765));
    }

    #[test]
    fn routes_answer_the_expected_status_codes() -> TestResult {
        let (_dir, app) = app_with_board(Some(JOB))?;
        let host = Some("127.0.0.1:1");
        assert_eq!(route(&app, "GET", "/healthz", host, 1, 5.0).status, 200);
        assert_eq!(route(&app, "GET", "/", host, 1, 5.0).status, 200);
        assert_eq!(route(&app, "GET", "/dashboard.html?x=1", host, 1, 5.0).status, 200);
        assert_eq!(route(&app, "GET", "/nope", host, 1, 5.0).status, 404);
        assert_eq!(route(&app, "POST", "/api/state", host, 1, 5.0).status, 405);
        assert_eq!(route(&app, "GET", "/api/state", Some("evil.example"), 1, 5.0).status, 403);
        Ok(())
    }

    #[test]
    fn api_state_joins_the_board_and_the_registry() -> TestResult {
        let (_dir, app) = app_with_board(Some(JOB))?;
        if let Ok(mut registry) = app.registry.lock() {
            registry.ingest_line(
                r#"{"ts":2.0,"hook_event_name":"SubagentStart","agent_id":"s1","agent_type":"factory-spec-writer","cwd":"/r"}"#,
            );
        }
        let reply = route(&app, "GET", "/api/state", Some("127.0.0.1:1"), 1, 5.0);
        assert_eq!(reply.status, 200);
        let json: serde_json::Value = serde_json::from_slice(&reply.body)?;
        assert_eq!(json["jobs"][0]["id"], "001");
        assert_eq!(json["jobs"][0]["agents"][0]["agent_type"], "factory-spec-writer");
        assert!(json["board_error"].is_null());
        Ok(())
    }

    #[test]
    fn api_state_without_a_board_file_has_no_jobs_and_no_error() -> TestResult {
        let (_dir, app) = app_with_board(None)?;
        let reply = route(&app, "GET", "/api/state", Some("127.0.0.1:1"), 1, 5.0);
        let json: serde_json::Value = serde_json::from_slice(&reply.body)?;
        assert_eq!(json["jobs"].as_array().map(|jobs| jobs.len()), Some(0));
        assert!(json["board_error"].is_null());
        Ok(())
    }

    #[test]
    fn api_state_reports_an_invalid_board_file() -> TestResult {
        let (_dir, app) = app_with_board(Some("{ nope"))?;
        let reply = route(&app, "GET", "/api/state", Some("127.0.0.1:1"), 1, 5.0);
        let json: serde_json::Value = serde_json::from_slice(&reply.body)?;
        assert!(json["board_error"].as_str().is_some_and(|text| text.contains("invalid")));
        Ok(())
    }

    #[test]
    fn serves_state_over_a_real_socket_and_rejects_a_foreign_host() -> TestResult {
        let (_dir, app) = app_with_board(Some(JOB))?;
        let server = tiny_http::Server::http("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = server.server_addr().to_ip().ok_or("no ip address")?.port();
        let served = Arc::clone(&app);
        std::thread::spawn(move || serve(server, served, port));
        let good = http_get(port, "/api/state", &format!("127.0.0.1:{port}"))?;
        assert!(good.starts_with("HTTP/1.1 200"), "got: {good}");
        assert!(good.contains(r#""id":"001""#));
        let bad = http_get(port, "/api/state", "evil.example")?;
        assert!(bad.starts_with("HTTP/1.1 403"), "got: {bad}");
        Ok(())
    }
}
```

Replace `main.rs` with the module list only for now:

```rust
mod events;
mod model;
mod server;
mod state;

fn main() {}
```

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml`
Expected: compile errors for `App`, `route`, `parse_args`, `serve` and the other names.

- [ ] **Step 2: Implement the server**

Put this above the `#[cfg(test)]` block in `tools/factory-server/src/server.rs`:

```rust
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::events::{Registry, Tailer};
use crate::state::{build_state, load_board};

const DASHBOARD_HTML: &str = include_str!("../static/dashboard.html");
const POLL_INTERVAL: Duration = Duration::from_millis(500);

pub struct Config {
    pub root: PathBuf,
    pub port: u16,
}

pub struct App {
    pub root: PathBuf,
    pub registry: Mutex<Registry>,
}

pub struct Reply {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

pub fn parse_args(args: &[String]) -> Result<Config, String> {
    let mut root = PathBuf::from(".");
    let mut port: u16 = 8765;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--root" => {
                root = PathBuf::from(iter.next().ok_or("--root needs a path")?);
            }
            "--port" => {
                let raw = iter.next().ok_or("--port needs a number")?;
                port = raw.parse::<u16>().map_err(|_| format!("bad port {raw:?}"))?;
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(Config { root, port })
}

pub fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs_f64())
        .unwrap_or(0.0)
}

/// Only the loopback names with our own port are accepted, which blocks DNS-rebinding requests.
pub fn host_allowed(host: Option<&str>, port: u16) -> bool {
    match host {
        Some(value) => {
            value == format!("127.0.0.1:{port}") || value == format!("localhost:{port}")
        }
        None => false,
    }
}

fn text(status: u16, body: &str) -> Reply {
    Reply {
        status,
        content_type: "text/plain; charset=utf-8",
        body: body.as_bytes().to_vec(),
    }
}

pub fn route(
    app: &App,
    method: &str,
    url: &str,
    host: Option<&str>,
    port: u16,
    now: f64,
) -> Reply {
    if !host_allowed(host, port) {
        return text(403, "forbidden host");
    }
    if method != "GET" {
        return text(405, "method not allowed");
    }
    let path = url.split('?').next().unwrap_or("/");
    match path {
        "/" | "/dashboard.html" => Reply {
            status: 200,
            content_type: "text/html; charset=utf-8",
            body: DASHBOARD_HTML.as_bytes().to_vec(),
        },
        "/healthz" => text(200, "ok"),
        "/api/state" => api_state(app, now),
        _ => text(404, "not found"),
    }
}

fn api_state(app: &App, now: f64) -> Reply {
    let board_path = app
        .root
        .join(".superpowers")
        .join("factory")
        .join("board.json");
    let (board, board_error) = load_board(&board_path);
    let view = match app.registry.lock() {
        Ok(registry) => build_state(&board, &registry, now, board_error),
        Err(_) => return text(500, "state lock poisoned"),
    };
    match serde_json::to_vec(&view) {
        Ok(body) => Reply {
            status: 200,
            content_type: "application/json",
            body,
        },
        Err(_) => text(500, "could not encode state"),
    }
}

/// Tails the event log in a background thread and feeds the registry.
pub fn spawn_tailer(app: Arc<App>) {
    let path = app
        .root
        .join(".superpowers")
        .join("factory")
        .join("events.jsonl");
    std::thread::spawn(move || {
        let mut tailer = Tailer::new(path);
        loop {
            let lines = tailer.poll();
            if !lines.is_empty() {
                if let Ok(mut registry) = app.registry.lock() {
                    for line in &lines {
                        registry.ingest_line(line);
                    }
                }
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    });
}

pub fn serve(server: tiny_http::Server, app: Arc<App>, port: u16) {
    for request in server.incoming_requests() {
        let host = request
            .headers()
            .iter()
            .find(|header| header.field.equiv("Host"))
            .map(|header| header.value.as_str().to_string());
        let reply = route(
            &app,
            request.method().as_str(),
            request.url(),
            host.as_deref(),
            port,
            now_secs(),
        );
        let mut response =
            tiny_http::Response::from_data(reply.body).with_status_code(reply.status);
        if let Ok(header) =
            tiny_http::Header::from_bytes(&b"Content-Type"[..], reply.content_type.as_bytes())
        {
            response.add_header(header);
        }
        if let Ok(header) = tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"no-store"[..]) {
            response.add_header(header);
        }
        let _ = request.respond(response);
    }
}
```

Replace `main.rs`:

```rust
mod events;
mod model;
mod server;
mod state;

use std::sync::{Arc, Mutex};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = match server::parse_args(&args) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("factory-server: {message}");
            eprintln!("usage: factory-server [--root PATH] [--port N]");
            std::process::exit(2);
        }
    };
    let http = match tiny_http::Server::http(("127.0.0.1", config.port)) {
        Ok(http) => http,
        Err(error) => {
            eprintln!(
                "factory-server: cannot listen on port {}: {error}",
                config.port
            );
            std::process::exit(1);
        }
    };
    let app = Arc::new(server::App {
        root: config.root.clone(),
        registry: Mutex::new(events::Registry::new()),
    });
    server::spawn_tailer(Arc::clone(&app));
    println!(
        "factory-server: http://127.0.0.1:{}/ (root {})",
        config.port,
        config.root.display()
    );
    server::serve(http, app, config.port);
}
```

- [ ] **Step 3: Run the tests and the checks**

Run:
- `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml`
- `cargo clippy -j4 --manifest-path tools/factory-server/Cargo.toml`

Expected: all 25 tests pass (17 earlier, 8 new). Fix any clippy warning that is not about an item that plan 03 will use.

- [ ] **Step 4: Run it by hand**

```bash
cargo run -j4 --manifest-path tools/factory-server/Cargo.toml -- --root "$PWD" --port 8765
```

In another terminal: `curl -s http://127.0.0.1:8765/api/state | jq .` should print `jobs: []`. Then `curl -s -H 'Host: evil.example' -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8765/api/state` should print `403`. Stop the server with Ctrl+C.

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill with a pathspec commit of `tools/factory-server/src/server.rs`, `tools/factory-server/src/main.rs`, `tools/factory-server/static/dashboard.html`, `tools/factory-server/Cargo.lock`.
Suggested type: `feat(factory): serve the factory state over local http`.
