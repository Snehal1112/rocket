# Software Factory v3, Plan 03: Agent Runner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task, executed by a `factory-builder` subagent. Skip that skill's final whole-branch review and its ledger scripts and its progress file. Steps use checkbox (`- [ ]`) syntax. Plan 01 (`factory-core`) must be finished first. Plan 02 (`factory-git`) is not needed.

**Goal:** Build the `factory-agent` crate: a probe of the real `claude` flags, a pure stream-json parser and event sink, and a `ClaudeRunner` that runs headless agents with budget kill and stall detection, plus a `FakeRunner` for engine tests.

**Architecture:** `claude -p --output-format stream-json` is spawned as a child process. A reader thread forwards stdout lines over a channel, so the main loop can poll for budget, turn and stall limits without ever blocking on a pipe. A pure `StreamState` folds lines into session id, per-message usage (deduplicated by message id), tool events and the final result. Events go to `events.jsonl` in the same shape the dashboard server reads, plus `job_id` and `tokens`.

**Tech Stack:** Rust (std threads and `mpsc`, no async), `serde`, `serde_json`, `thiserror`, `factory-core` (path dep), `tempfile` (dev). Real `claude` CLI for the probe only.

**Spec:** `docs/superpowers/specs/2026-10-13-software-factory-v3-rust-driver-design.md` (decisions 3, 8, 9, 12, risks section, and the plan 03 row of "Interfaces between plans"). Design doc: `.claude/software-factory.md`.

## Global Constraints

- Rust workspace at `tools/factory/` (own workspace, own target dir, not in the root workspace). New crate path: `tools/factory/crates/factory-agent/`.
- Pinned versions: `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`, `thiserror = "1"`, `tempfile = "3"` (dev only). No other new dependencies.
- Never `.unwrap()` anywhere, also not in tests and not in the fake binary. Use `.expect("...")` only inside tests. Production code returns `Result` with `thiserror` types or matches explicitly. Avoid `unwrap_or*` too, use `map_or` or `match`, so the harness hook has nothing to flag.
- Commands: `cargo check -j4 --manifest-path tools/factory/Cargo.toml` and `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-agent <test_name>`. Never `--workspace`, never omit `-j4`, never set `CARGO_TARGET_DIR`.
- Budget formula from the spec: weighted = input + cache creation + output + cache read / 10. Use `factory_core::weighted_tokens`, never re-implement it.
- The factory never pushes. Agents never write the ledger or the board. This crate writes only `events.jsonl` lines.
- Event JSON keeps the v1 server fields (`ts`, `hook_event_name`, `agent_id`, `agent_type`, `tool_name`, `tool_target`, `cwd`, `last_message`) and adds `job_id` and `tokens`. `hook_event_name` is one of `SubagentStart`, `PostToolUse`, `SubagentStop` (the only ones the server folds), plus `BudgetWarn` (ignored by the server).
- Comments are short full sentences ending in a period. No emojis.
- Commit steps: invoke the plugin skill `dev-workflow-skills:1-git-commit` but run its git commands inline with no helper agents. Use a pathspec commit: `git add <paths> && git commit --only -m "..." -- <paths>`. Never `git add -A`, `--all` or `.`. Never push.

## Review Focus

- Streaming duplicates: the same assistant message id repeats with growing `usage`. Expectation: counted once, latest values win. Pinned in Task 2 (`stream_dedups_message_ids`).
- Garbage, blank, truncated or non-UTF-8 stdout lines. Expectation: skipped and counted, no panic, no abort of the run. Pinned in Task 2 (`stream_skips_garbage_lines`) and Task 3 (`flood_does_not_deadlock`).
- An unwritable event log (parent path is a file). Expectation: the run still completes, `EventSink::failures()` counts the loss. Pinned in Task 2 (`sink_survives_write_errors`).
- A child that floods stdout and stderr, or hangs silently. Expectation: no deadlock on a full pipe, the stall detector kills it. Pinned in Task 3 (`flood_does_not_deadlock`, `silent_child_is_stalled`).
- A child that exits without a result line, or an `is_error` result. Expectation: `StopReason::Failed` carrying the stderr tail, never a false `Done`. Pinned in Task 3 (`no_result_is_failed`, `error_result_is_failed`).

## File Structure

- Create `tools/factory/crates/factory-agent/Cargo.toml`: crate manifest, `fake-claude` bin target.
- Create `tools/factory/crates/factory-agent/examples/probe.rs`: the real-`claude` probe (Task 1).
- Modify `.claude/software-factory.md`: dated probe report (Task 1).
- Create `src/lib.rs`: module wiring and re-exports.
- Create `src/usage.rs`: `Usage`.
- Create `src/stream.rs`: `StreamState`, `StreamEvent`.
- Create `src/spec.rs`: `AgentSpec`, `Caps`, `BudgetCheck` args builder.
- Create `src/run.rs`: `AgentRun`, `StopReason`.
- Create `src/events.rs`: `EventSink`, `AgentEvent`, `EventKind`.
- Create `src/runner.rs`: `AgentRunner` trait, `FakeRunner`.
- Create `src/claude.rs`: `ClaudeRunner`, `AgentError`.
- Create `src/bin/fake-claude.rs`: scripted stand-in for `claude`, selected by the first prompt line.
- Create `tests/fixtures/*.jsonl`, `tests/stream.rs`, `tests/runner.rs`.

---

### Task 1: Probe the real `claude` flags and record the report

**Files:**
- Create: `tools/factory/crates/factory-agent/Cargo.toml`
- Create: `tools/factory/crates/factory-agent/src/lib.rs` (one doc comment for now)
- Create: `tools/factory/crates/factory-agent/examples/probe.rs`
- Modify: `.claude/software-factory.md` (append a dated section)

**Interfaces:**
- Consumes: the `claude` CLI on PATH, and `tools/factory/Cargo.toml` from plan 01.
- Produces: a compiling `factory-agent` crate, the example `probe`, and a section `## claude headless probe (2026-10-13)` in `.claude/software-factory.md` containing a `Caps` table (`agent`, `max_turns`, `append_system_prompt`, `resume_keeps_session_id`, `verbose_required`, `prompt_on_stdin`). Tasks 2 and 3 read that table to set `Caps::default()`.

Known from `claude --version` 2.1.296 and `claude --help` (re-confirm, do not trust): `-p`, `--output-format`, `--verbose`, `--agent`, `--resume`, `--permission-mode`, `--allowedTools`, `--model`, `--append-system-prompt`, `--session-id`, `--include-partial-messages`, `--max-budget-usd` are listed. `--max-turns` was NOT in the help text. The probe settles each by a real call.

- [ ] **Step 1: Create the crate manifest**

Create `tools/factory/crates/factory-agent/Cargo.toml`:

```toml
[package]
name = "factory-agent"
version = "0.1.0"
edition = "2021"

[dependencies]
factory-core = { path = "../factory-core" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"

[dev-dependencies]
tempfile = "3"

[[bin]]
name = "fake-claude"
path = "src/bin/fake-claude.rs"
```

Open `tools/factory/Cargo.toml`. If `members` is not a glob like `"crates/*"`, add `"crates/factory-agent"`. Create `src/lib.rs` containing only `//! Headless agent runner for the software factory.`. Create a stub `src/bin/fake-claude.rs` with `fn main() {}` so the manifest resolves (Task 3 fills it).

- [ ] **Step 2: Write the probe example**

Create `examples/probe.rs`. It makes about ten tiny calls with `--model haiku` and `--max-budget-usd 0.05` in a temp dir, and prints a markdown report to stdout. It never touches the repo.

```rust
//! Probes which `claude` headless flags exist and how they behave.
//! Run: cargo run -j4 --manifest-path tools/factory/Cargo.toml -p factory-agent --example probe

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

struct Out {
    code: Option<i32>,
    stdout: String,
    stderr: String,
    timed_out: bool,
}

fn run(args: &[&str], stdin: Option<&str>, cwd: &Path) -> Result<Out, Box<dyn std::error::Error>> {
    let mut child = Command::new("claude")
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(mut s) = child.stdin.take() {
        if let Some(text) = stdin {
            let _ = s.write_all(text.as_bytes());
        }
    }
    let mut so = child.stdout.take().ok_or("no stdout")?;
    let mut se = child.stderr.take().ok_or("no stderr")?;
    let t_out = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = so.read_to_string(&mut s);
        s
    });
    let t_err = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = se.read_to_string(&mut s);
        s
    });
    let start = Instant::now();
    let mut timed_out = false;
    let code = loop {
        if let Some(status) = child.try_wait()? {
            break status.code();
        }
        if start.elapsed() > Duration::from_secs(120) {
            let _ = child.kill();
            timed_out = true;
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let stdout = t_out.join().map_err(|_| "stdout thread panicked")?;
    let stderr = t_err.join().map_err(|_| "stderr thread panicked")?;
    Ok(Out { code, stdout, stderr, timed_out })
}

/// Summarises a stream-json stdout: line types, session ids, message ids and usage places.
fn summarize(stdout: &str) -> String {
    let mut types = BTreeSet::new();
    let mut sessions = BTreeSet::new();
    let (mut assistant_lines, mut ids) = (0u32, BTreeSet::new());
    let (mut result_usage, mut result_line) = (false, String::new());
    let mut bad = 0u32;
    for line in stdout.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            bad += 1;
            continue;
        };
        let ty = v.get("type").and_then(|t| t.as_str()).map_or("?", |t| t).to_string();
        if let Some(s) = v.get("session_id").and_then(|s| s.as_str()) {
            sessions.insert(s.to_string());
        }
        if ty == "assistant" {
            assistant_lines += 1;
            if let Some(id) = v.pointer("/message/id").and_then(|i| i.as_str()) {
                ids.insert(id.to_string());
            }
        }
        if ty == "result" {
            result_usage = v.get("usage").is_some();
            result_line = line.chars().take(400).collect();
        }
        types.insert(ty);
    }
    format!(
        "types={types:?} sessions={sessions:?} assistant_lines={assistant_lines} unique_message_ids={} result_has_usage={result_usage} unparsable_lines={bad}\n    result_line={result_line}",
        ids.len()
    )
}

fn report(title: &str, out: &Out) {
    let err: String = out.stderr.chars().take(300).collect();
    println!("### {title}\n- exit={:?} timed_out={}\n- stderr={err:?}\n- {}\n", out.code, out.timed_out, summarize(&out.stdout));
}

fn first_session(stdout: &str) -> Option<String> {
    stdout.lines().find_map(|l| {
        let v: serde_json::Value = serde_json::from_str(l).ok()?;
        v.get("session_id")?.as_str().map(str::to_string)
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let cwd = dir.path();
    std::fs::create_dir_all(cwd.join(".claude/agents"))?;
    std::fs::write(
        cwd.join(".claude/agents/probe.md"),
        "---\nname: probe\ndescription: probe agent\n---\nYou end every reply with the word PROBED.\n",
    )?;

    println!("## Probe output\n");
    let help = run(&["--help"], None, cwd)?;
    println!("### claude --help flag presence");
    for flag in [
        "--print", "--output-format", "--verbose", "--agent", "--resume", "--permission-mode", "--allowedTools",
        "--model", "--max-turns", "--append-system-prompt", "--session-id", "--max-budget-usd", "--include-partial-messages",
    ] {
        println!("- {flag}: {}", if help.stdout.contains(flag) { "listed" } else { "NOT listed" });
    }
    println!();

    let base = ["-p", "--model", "haiku", "--max-budget-usd", "0.05"];
    let join = |extra: &[&str]| -> Vec<String> { base.iter().chain(extra.iter()).map(|s| s.to_string()).collect() };
    let call = |extra: &[&str], stdin: &str| -> Result<Out, Box<dyn std::error::Error>> {
        let args = join(extra);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run(&refs, Some(stdin), cwd)
    };

    report("stream-json without --verbose", &call(&["--output-format", "stream-json"], "Reply with exactly: PONG")?);
    let verbose = call(&["--output-format", "stream-json", "--verbose"], "Reply with exactly: PONG")?;
    report("stream-json with --verbose (prompt on stdin)", &verbose);
    report("max-turns 1", &call(&["--output-format", "stream-json", "--verbose", "--max-turns", "1"], "Reply with exactly: PONG")?);
    report("append-system-prompt", &call(&["--output-format", "stream-json", "--verbose", "--append-system-prompt", "End every reply with ZEBRA."], "Say hi.")?);
    report("permission-mode acceptEdits and allowedTools", &call(&["--output-format", "stream-json", "--verbose", "--permission-mode", "acceptEdits", "--allowedTools", "Read,Bash"], "Run `echo hi` with the Bash tool, then reply DONE.")?);
    report("agent probe", &call(&["--output-format", "stream-json", "--verbose", "--agent", "probe"], "Say hi.")?);
    if let Some(session) = first_session(&verbose.stdout) {
        let resumed = call(&["--output-format", "stream-json", "--verbose", "--resume", &session], "What did I ask you to reply with? One word.")?;
        println!("original session id: {session}");
        report("resume (compare session ids above with the original)", &resumed);
    } else {
        println!("### resume\n- skipped: no session id in the verbose stream\n");
    }
    Ok(())
}
```

- [ ] **Step 3: Check that it compiles**

Run: `cargo check -j4 --manifest-path tools/factory/Cargo.toml -p factory-agent --examples`
Expected: finishes with no errors. (`factory-core` must resolve from plan 01.)

- [ ] **Step 4: Run the probe for real**

Run: `cargo run -j4 --manifest-path tools/factory/Cargo.toml -p factory-agent --example probe 2>&1 | tee "$TMPDIR/probe.md"` (use the scratchpad directory if `TMPDIR` is unset). Expected: a markdown report. If `claude` is missing or not logged in, every section shows a nonzero exit; in that case write the report with the answer `unknown` for each row and still apply the fallbacks below.

- [ ] **Step 5: Record the report**

Append to `.claude/software-factory.md` a section headed `## claude headless probe (2026-10-13)` with: the `claude --version`, the raw findings per section above (shortened), and this table filled from the observed behavior (one row per question, answer `yes`, `no` or `unknown`):

| Question | Answer |
|---|---|
| `-p` works with the prompt on stdin | |
| `--output-format stream-json` needs `--verbose` | |
| `--agent <name>` loads `.claude/agents/<name>.md` | |
| `--resume <id>` continues, and the session id stays the same | |
| `--permission-mode acceptEdits` and `--allowedTools` accepted | |
| `--model` accepted | |
| `--max-turns` accepted | |
| `--append-system-prompt` accepted | |
| `system/init` line carries `session_id` | |
| Final line has `type:"result"` with `result`, `is_error`, `subtype`, `session_id`, `usage` | |
| Assistant lines carry `message.id` and `message.usage` | |
| One message id appears on several assistant lines | |

Also add the fallback rulings below, marking which apply:

- `--agent` missing or the agent not loaded: the driver sets `Caps.agent = false`. `AgentSpec.agent_prompt` (the agent file body, loaded by the engine) is sent with `--append-system-prompt`. If that flag is also missing, the prompt text is prepended to the user prompt on stdin.
- `--max-turns` missing: `Caps.max_turns = false`. The flag is not passed. The driver counts unique assistant message ids and kills the child past the limit (`StopReason::TurnLimit`). The driver enforces turns even when the flag exists.
- `--verbose` required for stream-json: always passed, no matter the answer.
- `--resume` changes the session id: the driver always takes the latest `session_id` seen (the `result` line is authoritative), so the engine stores the new id after every run.
- `--permission-mode` missing: the engine must not run headless builds. Stop and report to the user, do not invent a flag.

- [ ] **Step 6: Set `Caps::default()` constants**

Nothing to code yet. In Task 2 the `Caps::default()` values come from the table above (`agent`, `max_turns`, `append_system_prompt`). Keep the table in the doc exact because Task 2 copies from it.

- [ ] **Step 7: Commit**

Invoke `dev-workflow-skills:1-git-commit` (inline git, no helper agents), then:

```bash
git add tools/factory/Cargo.toml tools/factory/crates/factory-agent .claude/software-factory.md
git commit --only -m "chore(factory): probe claude headless flags" -- tools/factory/Cargo.toml tools/factory/crates/factory-agent .claude/software-factory.md
```

If `tools/factory/Cargo.toml` is unchanged, drop it from both path lists.

---

### Task 2: Pure parts: usage, stream parser, spec, run, event sink

**Files:**
- Create: `src/usage.rs`, `src/stream.rs`, `src/spec.rs`, `src/run.rs`, `src/events.rs`
- Modify: `src/lib.rs`
- Create: `tests/fixtures/ok.jsonl`, `tests/fixtures/garbage.jsonl`, `tests/fixtures/error_result.jsonl`
- Test: `tests/stream.rs`

**Interfaces:**
- Consumes: `factory_core::weighted_tokens`. Before writing code, open `tools/factory/crates/factory-core/src/` and read the exact signature of `weighted_tokens` and `budget_state`. This plan assumes `weighted_tokens(input: u64, cache_creation: u64, output: u64, cache_read: u64) -> u64` and `budget_state(used: u64, limit: u64) -> BudgetState` with variants `Ok`, `Warn`, `Stop`. If plan 01 differs, change only the call in `Usage::weighted` here and the match in Task 3 `claude.rs`.
- Produces (exact):

```rust
// usage.rs
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage { pub input_tokens: u64, pub cache_creation_input_tokens: u64, pub cache_read_input_tokens: u64, pub output_tokens: u64 }
impl Usage {
    pub fn from_value(v: &serde_json::Value) -> Usage;
    pub fn add(&self, other: &Usage) -> Usage;
    pub fn max_with(&self, other: &Usage) -> Usage;
    pub fn weighted(&self) -> u64;
    pub fn is_zero(&self) -> bool;
}
// stream.rs
pub enum StreamEvent { Init { session_id: String }, Tool { name: String, target: String }, Result { text: String, is_error: bool, subtype: String } }
pub struct StreamState { pub session_id: Option<String>, pub result_text: Option<String>, pub result_is_error: bool, pub result_subtype: Option<String>, pub skipped_lines: u64, /* private */ }
impl StreamState { pub fn feed(&mut self, line: &str) -> Vec<StreamEvent>; pub fn usage(&self) -> Usage; pub fn turns(&self) -> u32; pub fn has_result(&self) -> bool; }
// spec.rs
pub struct Caps { pub agent: bool, pub max_turns: bool, pub append_system_prompt: bool }
pub struct AgentSpec { pub job_id: String, pub role: String, pub cwd: PathBuf, pub prompt: String, pub agent: Option<String>, pub agent_prompt: Option<String>, pub model: Option<String>, pub resume: Option<String>, pub permission_mode: Option<String>, pub allowed_tools: Vec<String>, pub max_turns: Option<u32>, pub budget_weighted: Option<u64>, pub used_before: u64, pub stall_after: Option<Duration> }
impl AgentSpec { pub fn new(job_id: &str, role: &str, cwd: impl Into<PathBuf>, prompt: &str) -> AgentSpec; pub fn agent_id(&self) -> String; pub fn args(&self, caps: &Caps) -> Vec<String>; pub fn stdin_prompt(&self, caps: &Caps) -> String; }
// run.rs
pub enum StopReason { Done, BudgetStop, Stalled, TurnLimit, Failed }
pub struct AgentRun { pub session_id: Option<String>, pub usage: Usage, pub weighted: u64, pub result_text: String, pub stop_reason: StopReason, pub turns: u32, pub exit_code: Option<i32>, pub error: Option<String> }
impl AgentRun { pub fn done(text: &str) -> AgentRun; pub fn failed(msg: &str) -> AgentRun; }
// events.rs
pub enum EventKind { Start, Tool, Stop, BudgetWarn }
pub struct AgentEvent { pub kind: EventKind, pub agent_id: String, pub agent_type: String, pub tool_name: Option<String>, pub tool_target: Option<String>, pub cwd: Option<String>, pub last_message: Option<String>, pub tokens: u64 }
pub struct EventSink { /* path, job_id, failures */ }
impl EventSink { pub fn new(path: impl Into<PathBuf>, job_id: &str) -> EventSink; pub fn null(job_id: &str) -> EventSink; pub fn line(&self, ev: &AgentEvent, ts: f64) -> String; pub fn emit(&self, ev: &AgentEvent); pub fn failures(&self) -> u64; }
```

- [ ] **Step 1: Write the fixtures**

`tests/fixtures/ok.jsonl` (message `msg_1` streams twice with growing output; one tool use; a second message; a result):

```
{"type":"system","subtype":"init","session_id":"sess-1","model":"claude-sonnet-4-5","tools":["Read","Bash"]}
{"type":"assistant","message":{"id":"msg_1","role":"assistant","content":[{"type":"text","text":"Reading."}],"usage":{"input_tokens":10,"cache_creation_input_tokens":100,"cache_read_input_tokens":1000,"output_tokens":1}},"session_id":"sess-1"}
{"type":"assistant","message":{"id":"msg_1","role":"assistant","content":[{"type":"tool_use","id":"tu_1","name":"Read","input":{"file_path":"/w/a.rs"}}],"usage":{"input_tokens":10,"cache_creation_input_tokens":100,"cache_read_input_tokens":1000,"output_tokens":40}},"session_id":"sess-1"}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"tu_1","content":"fn main() {}"}]},"session_id":"sess-1"}
{"type":"assistant","message":{"id":"msg_2","role":"assistant","content":[{"type":"text","text":"All done."}],"usage":{"input_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":1100,"output_tokens":20}},"session_id":"sess-1"}
{"type":"result","subtype":"success","is_error":false,"result":"All done.","session_id":"sess-1","num_turns":2,"usage":{"input_tokens":15,"cache_creation_input_tokens":100,"cache_read_input_tokens":2100,"output_tokens":60}}
```

`tests/fixtures/garbage.jsonl` (note the blank line, truncated JSON and a bare word):

```
not json at all

{"type":"system","subtype":"init","session_id":"sess-g"}
{"type":"assistant","message":{"id":"msg_g","usage":{"input_tokens":1,"output_tokens":2}
{"type":"assistant","message":{"id":"msg_g","content":[],"usage":{"input_tokens":1,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":2}}}
{"type":"result","subtype":"success","is_error":false,"result":"ok","session_id":"sess-g"}
```

`tests/fixtures/error_result.jsonl`:

```
{"type":"system","subtype":"init","session_id":"sess-e"}
{"type":"result","subtype":"error_max_turns","is_error":true,"result":"","session_id":"sess-e","usage":{"input_tokens":3,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":4}}
```

- [ ] **Step 2: Write the failing tests**

Create `tests/stream.rs`:

```rust
use factory_agent::{AgentEvent, AgentSpec, Caps, EventKind, EventSink, StopReason, StreamEvent, StreamState, Usage};
use std::path::PathBuf;

fn feed_file(name: &str) -> (StreamState, Vec<StreamEvent>) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    let text = std::fs::read_to_string(path).expect("fixture");
    let mut state = StreamState::default();
    let mut events = Vec::new();
    for line in text.lines() {
        events.extend(state.feed(line));
    }
    (state, events)
}

#[test]
fn stream_dedups_message_ids() {
    let (state, _) = feed_file("ok.jsonl");
    let u = state.usage();
    // msg_1 counts once with its latest values, plus msg_2.
    assert_eq!(u.input_tokens, 15);
    assert_eq!(u.cache_creation_input_tokens, 100);
    assert_eq!(u.cache_read_input_tokens, 2100);
    assert_eq!(u.output_tokens, 60);
    assert_eq!(u.weighted(), 15 + 100 + 60 + 210);
    assert_eq!(state.turns(), 2);
}

#[test]
fn stream_reads_session_result_and_tools() {
    let (state, events) = feed_file("ok.jsonl");
    assert_eq!(state.session_id.as_deref(), Some("sess-1"));
    assert_eq!(state.result_text.as_deref(), Some("All done."));
    assert!(state.has_result());
    assert!(!state.result_is_error);
    let tools: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            StreamEvent::Tool { name, target } => Some((name.clone(), target.clone())),
            _ => None,
        })
        .collect();
    // The tool use repeats on streaming lines, but the tool id is reported once.
    assert_eq!(tools, vec![("Read".to_string(), "/w/a.rs".to_string())]);
}

#[test]
fn stream_skips_garbage_lines() {
    let (state, _) = feed_file("garbage.jsonl");
    assert_eq!(state.skipped_lines, 2);
    assert_eq!(state.session_id.as_deref(), Some("sess-g"));
    assert_eq!(state.usage().output_tokens, 2);
    assert!(state.has_result());
}

#[test]
fn stream_reports_error_result() {
    let (state, _) = feed_file("error_result.jsonl");
    assert!(state.result_is_error);
    assert_eq!(state.result_subtype.as_deref(), Some("error_max_turns"));
    // No per-message usage here, so the result usage is the fallback.
    assert_eq!(state.usage().output_tokens, 4);
}

#[test]
fn usage_add_and_max() {
    let a = Usage { input_tokens: 1, cache_creation_input_tokens: 2, cache_read_input_tokens: 3, output_tokens: 9 };
    let b = Usage { input_tokens: 4, cache_creation_input_tokens: 1, cache_read_input_tokens: 0, output_tokens: 5 };
    assert_eq!(a.add(&b).output_tokens, 14);
    assert_eq!(a.max_with(&b).input_tokens, 4);
    assert_eq!(a.max_with(&b).output_tokens, 9);
    assert!(Usage::default().is_zero());
}

fn caps(agent: bool, max_turns: bool, append: bool) -> Caps {
    Caps { agent, max_turns, append_system_prompt: append }
}

#[test]
fn args_full_spec() {
    let mut spec = AgentSpec::new("001", "builder", "/w", "do it");
    spec.agent = Some("factory-builder".into());
    spec.model = Some("sonnet".into());
    spec.resume = Some("sess-1".into());
    spec.permission_mode = Some("acceptEdits".into());
    spec.allowed_tools = vec!["Read".into(), "Bash".into()];
    spec.max_turns = Some(40);
    let args = spec.args(&caps(true, true, true));
    let joined = args.join(" ");
    assert!(joined.starts_with("-p --output-format stream-json --verbose"));
    for want in ["--model sonnet", "--resume sess-1", "--permission-mode acceptEdits", "--allowedTools Read,Bash", "--agent factory-builder", "--max-turns 40"] {
        assert!(joined.contains(want), "missing {want} in {joined}");
    }
    assert_eq!(spec.stdin_prompt(&caps(true, true, true)), "do it");
}

#[test]
fn args_fall_back_without_agent_or_max_turns() {
    let mut spec = AgentSpec::new("001", "builder", "/w", "do it");
    spec.agent = Some("factory-builder".into());
    spec.agent_prompt = Some("You are the builder.".into());
    spec.max_turns = Some(40);
    let joined = spec.args(&caps(false, false, true)).join(" ");
    assert!(!joined.contains("--agent"));
    assert!(!joined.contains("--max-turns"));
    assert!(joined.contains("--append-system-prompt You are the builder."));
    // With no append support the agent text moves into the user prompt.
    let no_append = caps(false, false, false);
    assert!(!spec.args(&no_append).join(" ").contains("--append-system-prompt"));
    assert_eq!(spec.stdin_prompt(&no_append), "You are the builder.\n\ndo it");
}

#[test]
fn args_omit_empty_options() {
    let spec = AgentSpec::new("001", "reviewer", "/w", "review");
    let joined = spec.args(&caps(true, true, true)).join(" ");
    assert!(!joined.contains("--resume"));
    assert!(!joined.contains("--allowedTools"));
    assert!(!joined.contains("--model"));
    assert_eq!(spec.agent_id(), "001-reviewer");
}

fn sample_event() -> AgentEvent {
    AgentEvent {
        kind: EventKind::Tool,
        agent_id: "001-builder".into(),
        agent_type: "factory-builder".into(),
        tool_name: Some("Read".into()),
        tool_target: Some("/w/a.rs".into()),
        cwd: Some("/w".into()),
        last_message: None,
        tokens: 385,
    }
}

#[test]
fn sink_line_has_server_fields_and_new_ones() {
    let sink = EventSink::null("001");
    let v: serde_json::Value = serde_json::from_str(&sink.line(&sample_event(), 12.5)).expect("json");
    assert_eq!(v["hook_event_name"], "PostToolUse");
    assert_eq!(v["agent_id"], "001-builder");
    assert_eq!(v["agent_type"], "factory-builder");
    assert_eq!(v["tool_name"], "Read");
    assert_eq!(v["tool_target"], "/w/a.rs");
    assert_eq!(v["cwd"], "/w");
    assert_eq!(v["ts"], 12.5);
    assert_eq!(v["job_id"], "001");
    assert_eq!(v["tokens"], 385);
    assert!(v.get("last_message").is_none());
}

#[test]
fn sink_trims_long_last_message_on_a_char_boundary() {
    let sink = EventSink::null("001");
    let mut ev = sample_event();
    ev.kind = EventKind::Stop;
    ev.last_message = Some("é".repeat(500));
    let v: serde_json::Value = serde_json::from_str(&sink.line(&ev, 1.0)).expect("json");
    assert_eq!(v["hook_event_name"], "SubagentStop");
    assert_eq!(v["last_message"].as_str().expect("str").chars().count(), 300);
}

#[test]
fn sink_appends_lines_and_creates_parent_dirs() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("deep/events.jsonl");
    let sink = EventSink::new(&path, "001");
    sink.emit(&sample_event());
    sink.emit(&sample_event());
    let text = std::fs::read_to_string(&path).expect("read");
    assert_eq!(text.lines().count(), 2);
    assert_eq!(sink.failures(), 0);
}

#[test]
fn sink_survives_write_errors() {
    let dir = tempfile::tempdir().expect("tmp");
    let blocker = dir.path().join("file");
    std::fs::write(&blocker, "x").expect("write");
    // The parent of the log is a regular file, so every write fails.
    let sink = EventSink::new(blocker.join("events.jsonl"), "001");
    sink.emit(&sample_event());
    sink.emit(&sample_event());
    assert_eq!(sink.failures(), 2);
}

#[test]
fn stop_reason_is_comparable() {
    assert_ne!(StopReason::Done, StopReason::Failed);
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-agent --test stream`
Expected: FAIL to compile (`unresolved imports factory_agent::...`).

- [ ] **Step 4: Write `usage.rs`**

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Token counts of one message or one run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub output_tokens: u64,
}

fn field(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).map_or(0, |n| n)
}

impl Usage {
    /// Reads a `usage` JSON object. Missing fields count as zero.
    pub fn from_value(v: &Value) -> Usage {
        Usage {
            input_tokens: field(v, "input_tokens"),
            cache_creation_input_tokens: field(v, "cache_creation_input_tokens"),
            cache_read_input_tokens: field(v, "cache_read_input_tokens"),
            output_tokens: field(v, "output_tokens"),
        }
    }

    pub fn add(&self, other: &Usage) -> Usage {
        Usage {
            input_tokens: self.input_tokens.saturating_add(other.input_tokens),
            cache_creation_input_tokens: self.cache_creation_input_tokens.saturating_add(other.cache_creation_input_tokens),
            cache_read_input_tokens: self.cache_read_input_tokens.saturating_add(other.cache_read_input_tokens),
            output_tokens: self.output_tokens.saturating_add(other.output_tokens),
        }
    }

    /// Field-wise maximum. A streaming message only ever grows, so this is the latest value.
    pub fn max_with(&self, other: &Usage) -> Usage {
        Usage {
            input_tokens: self.input_tokens.max(other.input_tokens),
            cache_creation_input_tokens: self.cache_creation_input_tokens.max(other.cache_creation_input_tokens),
            cache_read_input_tokens: self.cache_read_input_tokens.max(other.cache_read_input_tokens),
            output_tokens: self.output_tokens.max(other.output_tokens),
        }
    }

    /// Weighted tokens by the budget formula in `factory-core`.
    pub fn weighted(&self) -> u64 {
        factory_core::weighted_tokens(
            self.input_tokens,
            self.cache_creation_input_tokens,
            self.output_tokens,
            self.cache_read_input_tokens,
        )
    }

    pub fn is_zero(&self) -> bool {
        *self == Usage::default()
    }
}
```

- [ ] **Step 5: Write `stream.rs`**

```rust
use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::usage::Usage;

const MAX_COMMAND: usize = 80;
const MAX_TARGET: usize = 300;

/// Something the runner reacts to while reading the stream.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    Init { session_id: String },
    Tool { name: String, target: String },
    Result { text: String, is_error: bool, subtype: String },
}

/// Folds `claude -p --output-format stream-json` lines into run state.
#[derive(Debug, Default)]
pub struct StreamState {
    pub session_id: Option<String>,
    pub result_text: Option<String>,
    pub result_is_error: bool,
    pub result_subtype: Option<String>,
    pub skipped_lines: u64,
    usage_by_message: BTreeMap<String, Usage>,
    seen_tools: BTreeSet<String>,
    result_usage: Option<Usage>,
}

fn trim_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// Picks the same target the v1 hook picked: a path, pattern, url or a short command.
fn tool_target(input: &Value) -> String {
    for key in ["file_path", "path", "pattern", "url"] {
        if let Some(v) = input.get(key).and_then(Value::as_str) {
            let v = if key == "url" { v.split(['?', '#']).next().map_or(v, |p| p) } else { v };
            return trim_chars(v, MAX_TARGET);
        }
    }
    input.get("command").and_then(Value::as_str).map_or(String::new(), |c| trim_chars(c, MAX_COMMAND))
}

impl StreamState {
    /// Feeds one stdout line. Blank and unparsable lines are skipped and counted.
    pub fn feed(&mut self, line: &str) -> Vec<StreamEvent> {
        let line = line.trim();
        if line.is_empty() {
            return Vec::new();
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                self.skipped_lines += 1;
                return Vec::new();
            }
        };
        let mut out = Vec::new();
        let kind = value.get("type").and_then(Value::as_str).map_or("", |t| t);
        // The latest session id wins, because a resumed run may get a new one.
        if let Some(id) = value.get("session_id").and_then(Value::as_str) {
            self.session_id = Some(id.to_string());
            if kind == "system" && value.get("subtype").and_then(Value::as_str) == Some("init") {
                out.push(StreamEvent::Init { session_id: id.to_string() });
            }
        }
        match kind {
            "assistant" => self.on_assistant(&value, &mut out),
            "result" => self.on_result(&value, &mut out),
            _ => {}
        }
        out
    }

    fn on_assistant(&mut self, value: &Value, out: &mut Vec<StreamEvent>) {
        let Some(message) = value.get("message") else {
            return;
        };
        if let Some(id) = message.get("id").and_then(Value::as_str) {
            let seen = message.get("usage").map_or(Usage::default(), Usage::from_value);
            let merged = match self.usage_by_message.get(id) {
                Some(old) => old.max_with(&seen),
                None => seen,
            };
            self.usage_by_message.insert(id.to_string(), merged);
        }
        let Some(items) = message.get("content").and_then(Value::as_array) else {
            return;
        };
        for item in items {
            if item.get("type").and_then(Value::as_str) != Some("tool_use") {
                continue;
            }
            let tool_id = item.get("id").and_then(Value::as_str).map_or(String::new(), str::to_string);
            if !tool_id.is_empty() && !self.seen_tools.insert(tool_id) {
                continue;
            }
            let name = item.get("name").and_then(Value::as_str).map_or(String::new(), str::to_string);
            let target = item.get("input").map_or(String::new(), tool_target);
            out.push(StreamEvent::Tool { name, target });
        }
    }

    fn on_result(&mut self, value: &Value, out: &mut Vec<StreamEvent>) {
        let text = value.get("result").and_then(Value::as_str).map_or(String::new(), str::to_string);
        let is_error = value.get("is_error").and_then(Value::as_bool).map_or(false, |b| b);
        let subtype = value.get("subtype").and_then(Value::as_str).map_or(String::new(), str::to_string);
        self.result_usage = value.get("usage").map(Usage::from_value);
        self.result_text = Some(text.clone());
        self.result_is_error = is_error;
        self.result_subtype = Some(subtype.clone());
        out.push(StreamEvent::Result { text, is_error, subtype });
    }

    /// Sum of the latest usage of each unique message id. Falls back to the result usage when
    /// no per-message usage was seen.
    pub fn usage(&self) -> Usage {
        let sum = self.usage_by_message.values().fold(Usage::default(), |acc, u| acc.add(u));
        if sum.is_zero() {
            return self.result_usage.map_or(sum, |u| u);
        }
        sum
    }

    /// Number of unique assistant messages seen so far.
    pub fn turns(&self) -> u32 {
        u32::try_from(self.usage_by_message.len()).map_or(u32::MAX, |n| n)
    }

    pub fn has_result(&self) -> bool {
        self.result_text.is_some()
    }
}
```

Note the garbage fixture: its truncated assistant line and the bare word line are the 2 skipped lines. The blank line is not counted.

- [ ] **Step 6: Write `spec.rs` and `run.rs`**

`spec.rs`:

```rust
use std::path::PathBuf;
use std::time::Duration;

/// Which optional `claude` flags exist. Defaults come from the probe report in `.claude/software-factory.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    pub agent: bool,
    pub max_turns: bool,
    pub append_system_prompt: bool,
}

impl Default for Caps {
    fn default() -> Self {
        // Copy these three values from the Caps table of the 2026-10-13 probe report.
        Caps { agent: true, max_turns: false, append_system_prompt: true }
    }
}

/// Everything needed to run one headless agent.
#[derive(Debug, Clone)]
pub struct AgentSpec {
    pub job_id: String,
    /// Short role name such as `builder` or `reviewer`. Used for the event agent id.
    pub role: String,
    pub cwd: PathBuf,
    pub prompt: String,
    /// Agent name for `--agent`.
    pub agent: Option<String>,
    /// Agent file body, used when `--agent` is not available.
    pub agent_prompt: Option<String>,
    pub model: Option<String>,
    pub resume: Option<String>,
    pub permission_mode: Option<String>,
    pub allowed_tools: Vec<String>,
    pub max_turns: Option<u32>,
    /// Job budget in weighted tokens. `None` means no budget kill.
    pub budget_weighted: Option<u64>,
    /// Weighted tokens already spent by earlier runs of this job.
    pub used_before: u64,
    pub stall_after: Option<Duration>,
}

impl AgentSpec {
    pub fn new(job_id: &str, role: &str, cwd: impl Into<PathBuf>, prompt: &str) -> AgentSpec {
        AgentSpec {
            job_id: job_id.to_string(),
            role: role.to_string(),
            cwd: cwd.into(),
            prompt: prompt.to_string(),
            agent: None,
            agent_prompt: None,
            model: None,
            resume: None,
            permission_mode: None,
            allowed_tools: Vec::new(),
            max_turns: None,
            budget_weighted: None,
            used_before: 0,
            stall_after: None,
        }
    }

    /// Stable id for dashboard events, for example `001-builder`.
    pub fn agent_id(&self) -> String {
        format!("{}-{}", self.job_id, self.role)
    }

    /// Builds the argument list. The prompt itself goes to stdin, see `stdin_prompt`.
    pub fn args(&self, caps: &Caps) -> Vec<String> {
        let mut a: Vec<String> = ["-p", "--output-format", "stream-json", "--verbose"].iter().map(|s| s.to_string()).collect();
        let mut push = |flag: &str, value: &str| {
            a.push(flag.to_string());
            a.push(value.to_string());
        };
        if let Some(m) = &self.model {
            push("--model", m);
        }
        if let Some(s) = &self.resume {
            push("--resume", s);
        }
        if let Some(p) = &self.permission_mode {
            push("--permission-mode", p);
        }
        if !self.allowed_tools.is_empty() {
            push("--allowedTools", &self.allowed_tools.join(","));
        }
        match (&self.agent, &self.agent_prompt) {
            (Some(name), _) if caps.agent => push("--agent", name),
            (_, Some(text)) if caps.append_system_prompt => push("--append-system-prompt", text),
            _ => {}
        }
        if let (Some(n), true) = (self.max_turns, caps.max_turns) {
            push("--max-turns", &n.to_string());
        }
        a
    }

    /// The text written to stdin. The agent text is prepended when neither `--agent` nor
    /// `--append-system-prompt` can carry it.
    pub fn stdin_prompt(&self, caps: &Caps) -> String {
        let carried = (self.agent.is_some() && caps.agent) || caps.append_system_prompt;
        match (&self.agent_prompt, carried) {
            (Some(text), false) => format!("{text}\n\n{}", self.prompt),
            _ => self.prompt.clone(),
        }
    }
}
```

`run.rs`:

```rust
use crate::usage::Usage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Done,
    BudgetStop,
    Stalled,
    TurnLimit,
    Failed,
}

/// The outcome of one agent run.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentRun {
    pub session_id: Option<String>,
    pub usage: Usage,
    pub weighted: u64,
    pub result_text: String,
    pub stop_reason: StopReason,
    pub turns: u32,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
}

impl AgentRun {
    pub fn done(text: &str) -> AgentRun {
        AgentRun {
            session_id: None,
            usage: Usage::default(),
            weighted: 0,
            result_text: text.to_string(),
            stop_reason: StopReason::Done,
            turns: 0,
            exit_code: Some(0),
            error: None,
        }
    }

    pub fn failed(msg: &str) -> AgentRun {
        AgentRun {
            stop_reason: StopReason::Failed,
            error: Some(msg.to_string()),
            exit_code: None,
            ..AgentRun::done("")
        }
    }
}
```

- [ ] **Step 7: Write `events.rs`**

```rust
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};

const MAX_MESSAGE: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Start,
    Tool,
    Stop,
    BudgetWarn,
}

impl EventKind {
    /// The `hook_event_name` the dashboard server understands.
    pub fn hook_name(self) -> &'static str {
        match self {
            EventKind::Start => "SubagentStart",
            EventKind::Tool => "PostToolUse",
            EventKind::Stop => "SubagentStop",
            EventKind::BudgetWarn => "BudgetWarn",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentEvent {
    pub kind: EventKind,
    pub agent_id: String,
    pub agent_type: String,
    pub tool_name: Option<String>,
    pub tool_target: Option<String>,
    pub cwd: Option<String>,
    pub last_message: Option<String>,
    /// Weighted tokens of the job so far.
    pub tokens: u64,
}

/// Appends events to `events.jsonl`. A write error is counted and never returned.
#[derive(Debug)]
pub struct EventSink {
    path: Option<PathBuf>,
    job_id: String,
    failures: AtomicU64,
}

impl EventSink {
    pub fn new(path: impl Into<PathBuf>, job_id: &str) -> EventSink {
        EventSink { path: Some(path.into()), job_id: job_id.to_string(), failures: AtomicU64::new(0) }
    }

    /// A sink that writes nothing. Used in tests.
    pub fn null(job_id: &str) -> EventSink {
        EventSink { path: None, job_id: job_id.to_string(), failures: AtomicU64::new(0) }
    }

    pub fn failures(&self) -> u64 {
        self.failures.load(Ordering::Relaxed)
    }

    /// Renders one JSON line without the trailing newline.
    pub fn line(&self, ev: &AgentEvent, ts: f64) -> String {
        let mut m = Map::new();
        m.insert("ts".into(), json!(ts));
        m.insert("hook_event_name".into(), json!(ev.kind.hook_name()));
        m.insert("agent_id".into(), json!(ev.agent_id));
        m.insert("agent_type".into(), json!(ev.agent_type));
        if let Some(v) = &ev.tool_name {
            m.insert("tool_name".into(), json!(v));
        }
        if let Some(v) = &ev.tool_target {
            m.insert("tool_target".into(), json!(v));
        }
        if let Some(v) = &ev.cwd {
            m.insert("cwd".into(), json!(v));
        }
        if let Some(v) = &ev.last_message {
            let skip = v.chars().count().saturating_sub(MAX_MESSAGE);
            let tail: String = v.chars().skip(skip).collect();
            m.insert("last_message".into(), json!(tail));
        }
        m.insert("job_id".into(), json!(self.job_id));
        m.insert("tokens".into(), json!(ev.tokens));
        Value::Object(m).to_string()
    }

    /// Appends one event. Never fails the caller.
    pub fn emit(&self, ev: &AgentEvent) {
        let Some(path) = &self.path else {
            return;
        };
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
        let mut line = self.line(ev, ts);
        line.push('\n');
        let result = (|| -> std::io::Result<()> {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut f = OpenOptions::new().create(true).append(true).open(path)?;
            // One write call keeps concurrent appenders from interleaving a line.
            f.write_all(line.as_bytes())
        })();
        if result.is_err() {
            self.failures.fetch_add(1, Ordering::Relaxed);
        }
    }
}
```

- [ ] **Step 8: Wire `lib.rs`**

```rust
//! Headless agent runner for the software factory.

pub mod events;
pub mod run;
pub mod spec;
pub mod stream;
pub mod usage;

pub use events::{AgentEvent, EventKind, EventSink};
pub use run::{AgentRun, StopReason};
pub use spec::{AgentSpec, Caps};
pub use stream::{StreamEvent, StreamState};
pub use usage::Usage;
```

- [ ] **Step 9: Run the tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-agent --test stream`
Expected: all 13 tests PASS. If `weighted_tokens` has a different signature, fix only `Usage::weighted`.

- [ ] **Step 10: Commit**

Invoke `dev-workflow-skills:1-git-commit` (inline git), then:

```bash
git add tools/factory/crates/factory-agent
git commit --only -m "feat(factory): add agent stream parser, spec and event sink" -- tools/factory/crates/factory-agent
```

---

### Task 3: `AgentRunner`, `ClaudeRunner` and `FakeRunner`, tested against a fake `claude` binary

**Files:**
- Create: `src/runner.rs`, `src/claude.rs`
- Modify: `src/lib.rs`, `src/bin/fake-claude.rs`
- Test: `tests/runner.rs`

**Interfaces:**
- Consumes: everything from Task 2, `factory_core::budget_state` and its `BudgetState` enum (see the assumption in Task 2 and re-check the variant names).
- Produces (exact):

```rust
// runner.rs
pub trait AgentRunner { fn run(&self, spec: &AgentSpec) -> AgentRun; }
pub struct FakeRunner { /* script, calls */ }
impl FakeRunner { pub fn new(script: Vec<AgentRun>) -> FakeRunner; pub fn calls(&self) -> Vec<AgentSpec>; }
impl AgentRunner for FakeRunner { ... }
// claude.rs
pub struct ClaudeRunner { /* program, caps, sink */ }
impl ClaudeRunner { pub fn new(program: impl Into<std::ffi::OsString>, caps: Caps, sink: EventSink) -> ClaudeRunner; }
impl AgentRunner for ClaudeRunner { ... }
#[derive(thiserror::Error)] pub enum AgentError { Spawn(std::io::Error), Pipe, Wait(std::io::Error) }
```

`FakeRunner` pops one scripted `AgentRun` per call, records every spec, and returns `AgentRun::failed("fake script exhausted")` when the script is empty. The engine tests in plans 04 and 05 use it.

- [ ] **Step 1: Write the fake `claude` binary**

Replace `src/bin/fake-claude.rs`. It reads stdin to the end, takes the first prompt line as the mode, and prints stream-json. It never panics and ignores write errors (the runner may kill it).

```rust
//! Scripted stand-in for `claude -p`. The first line of stdin selects the behavior.

use std::io::{Read, Write};
use std::time::Duration;

fn emit(line: &str) {
    let mut out = std::io::stdout().lock();
    if writeln!(out, "{line}").is_err() || out.flush().is_err() {
        std::process::exit(0);
    }
}

fn init(session: &str) {
    emit(&format!(r#"{{"type":"system","subtype":"init","session_id":"{session}"}}"#));
}

fn assistant(id: &str, output: u64) {
    emit(&format!(
        r#"{{"type":"assistant","message":{{"id":"{id}","content":[],"usage":{{"input_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":{output}}}}},"session_id":"fake-1"}}"#
    ));
}

fn result(text: &str, is_error: bool, subtype: &str) {
    let text = text.replace('\\', "\\\\").replace('"', "\\\"");
    emit(&format!(
        r#"{{"type":"result","subtype":"{subtype}","is_error":{is_error},"result":"{text}","session_id":"fake-1"}}"#
    ));
}

fn main() {
    let mut prompt = String::new();
    let _ = std::io::stdin().read_to_string(&mut prompt);
    let mode = prompt.lines().next().map_or("", str::trim).to_string();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match mode.as_str() {
        "ok" => {
            init("fake-1");
            assistant("m1", 5);
            assistant("m1", 50);
            emit(r#"{"type":"assistant","message":{"id":"m2","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"echo hi"}}],"usage":{"output_tokens":10}}}"#);
            result("all good", false, "success");
        }
        "args" => {
            init("fake-1");
            result(&args.join(" "), false, "success");
        }
        "flood" => {
            init("fake-1");
            let junk = "x".repeat(200);
            for i in 0..20000 {
                emit(&format!("garbage {i} {junk}"));
                if i % 100 == 0 {
                    eprintln!("stderr noise {i} {junk}");
                }
            }
            // A non-UTF-8 line must not stop the reader.
            let _ = std::io::stdout().write_all(&[0xff, 0xfe, b'\n']);
            result("flooded", false, "success");
        }
        "silent" => {
            init("fake-1");
            std::thread::sleep(Duration::from_secs(30));
        }
        "budget" => {
            init("fake-1");
            for i in 0..1000 {
                assistant(&format!("b{i}"), 1000);
                std::thread::sleep(Duration::from_millis(20));
            }
            result("never reached", false, "success");
        }
        "turns" => {
            init("fake-1");
            for i in 0..50 {
                assistant(&format!("t{i}"), 1);
                std::thread::sleep(Duration::from_millis(20));
            }
            result("never reached", false, "success");
        }
        "error" => {
            init("fake-1");
            result("it broke", true, "error_during_execution");
        }
        "noresult" => {
            init("fake-1");
            eprintln!("fake claude crashed: boom");
            std::process::exit(3);
        }
        _ => {
            eprintln!("unknown mode {mode}");
            std::process::exit(2);
        }
    }
}
```

- [ ] **Step 2: Write the failing tests**

Create `tests/runner.rs`:

```rust
use std::time::{Duration, Instant};

use factory_agent::{AgentRun, AgentRunner, AgentSpec, Caps, ClaudeRunner, EventSink, FakeRunner, StopReason};

fn runner(sink: EventSink) -> ClaudeRunner {
    ClaudeRunner::new(env!("CARGO_BIN_EXE_fake-claude"), Caps { agent: true, max_turns: false, append_system_prompt: true }, sink)
}

fn spec(mode: &str, dir: &std::path::Path) -> AgentSpec {
    AgentSpec::new("001", "builder", dir, mode)
}

#[test]
fn ok_run_collects_session_usage_result_and_events() {
    let tmp = tempfile::tempdir().expect("tmp");
    let log = tmp.path().join("events.jsonl");
    let r = runner(EventSink::new(&log, "001"));
    let run = r.run(&spec("ok", tmp.path()));
    assert_eq!(run.stop_reason, StopReason::Done);
    assert_eq!(run.session_id.as_deref(), Some("fake-1"));
    assert_eq!(run.result_text, "all good");
    // m1 counts once at 50, m2 at 10.
    assert_eq!(run.usage.output_tokens, 60);
    assert_eq!(run.weighted, 60);
    assert_eq!(run.turns, 2);
    let lines: Vec<serde_json::Value> =
        std::fs::read_to_string(&log).expect("log").lines().map(|l| serde_json::from_str(l).expect("json")).collect();
    let names: Vec<&str> = lines.iter().map(|v| v["hook_event_name"].as_str().expect("name")).collect();
    assert_eq!(names, vec!["SubagentStart", "PostToolUse", "SubagentStop"]);
    assert_eq!(lines[1]["tool_name"], "Bash");
    assert_eq!(lines[1]["tool_target"], "echo hi");
    assert!(lines.iter().all(|v| v["job_id"] == "001" && v["agent_id"] == "001-builder"));
    assert_eq!(lines[2]["tokens"], 60);
}

#[test]
fn args_reach_the_child_and_prompt_goes_to_stdin() {
    let tmp = tempfile::tempdir().expect("tmp");
    let mut s = spec("args", tmp.path());
    s.model = Some("sonnet".into());
    s.resume = Some("sess-9".into());
    let run = runner(EventSink::null("001")).run(&s);
    assert_eq!(run.stop_reason, StopReason::Done);
    assert!(run.result_text.starts_with("-p --output-format stream-json --verbose"));
    assert!(run.result_text.contains("--model sonnet --resume sess-9"));
}

#[test]
fn error_result_is_failed() {
    let tmp = tempfile::tempdir().expect("tmp");
    let run = runner(EventSink::null("001")).run(&spec("error", tmp.path()));
    assert_eq!(run.stop_reason, StopReason::Failed);
    assert_eq!(run.result_text, "it broke");
}

#[test]
fn no_result_is_failed() {
    let tmp = tempfile::tempdir().expect("tmp");
    let run = runner(EventSink::null("001")).run(&spec("noresult", tmp.path()));
    assert_eq!(run.stop_reason, StopReason::Failed);
    assert_eq!(run.exit_code, Some(3));
    let err = run.error.expect("error text");
    assert!(err.contains("boom"), "stderr tail missing: {err}");
}

#[test]
fn missing_program_is_failed_not_a_panic() {
    let tmp = tempfile::tempdir().expect("tmp");
    let r = ClaudeRunner::new("/nonexistent/claude-binary", Caps::default(), EventSink::null("001"));
    let run = r.run(&spec("ok", tmp.path()));
    assert_eq!(run.stop_reason, StopReason::Failed);
    assert!(run.error.is_some());
}

#[test]
fn flood_does_not_deadlock() {
    let tmp = tempfile::tempdir().expect("tmp");
    let start = Instant::now();
    let run = runner(EventSink::null("001")).run(&spec("flood", tmp.path()));
    assert_eq!(run.stop_reason, StopReason::Done);
    assert_eq!(run.result_text, "flooded");
    assert!(start.elapsed() < Duration::from_secs(20));
}

#[test]
fn silent_child_is_stalled_and_killed() {
    let tmp = tempfile::tempdir().expect("tmp");
    let mut s = spec("silent", tmp.path());
    s.stall_after = Some(Duration::from_millis(400));
    let start = Instant::now();
    let run = runner(EventSink::null("001")).run(&s);
    assert_eq!(run.stop_reason, StopReason::Stalled);
    // The child sleeps 30 seconds, so a quick return proves it was killed.
    assert!(start.elapsed() < Duration::from_secs(10));
}

#[test]
fn budget_stop_kills_the_child() {
    let tmp = tempfile::tempdir().expect("tmp");
    let mut s = spec("budget", tmp.path());
    s.budget_weighted = Some(5000);
    let start = Instant::now();
    let run = runner(EventSink::null("001")).run(&s);
    assert_eq!(run.stop_reason, StopReason::BudgetStop);
    assert!(run.weighted >= 5000 && run.weighted < 20000, "weighted {}", run.weighted);
    assert!(start.elapsed() < Duration::from_secs(10));
}

#[test]
fn used_before_counts_toward_the_budget() {
    let tmp = tempfile::tempdir().expect("tmp");
    let mut s = spec("budget", tmp.path());
    s.budget_weighted = Some(5000);
    s.used_before = 4000;
    let run = runner(EventSink::null("001")).run(&s);
    assert_eq!(run.stop_reason, StopReason::BudgetStop);
    // Only 1000 more weighted tokens were needed, so well under 5000 came from this run.
    assert!(run.weighted < 5000, "weighted {}", run.weighted);
}

#[test]
fn turn_limit_is_enforced_by_the_driver() {
    let tmp = tempfile::tempdir().expect("tmp");
    let mut s = spec("turns", tmp.path());
    s.max_turns = Some(3);
    let run = runner(EventSink::null("001")).run(&s);
    assert_eq!(run.stop_reason, StopReason::TurnLimit);
    assert!(run.turns >= 4 && run.turns < 50, "turns {}", run.turns);
}

#[test]
fn event_log_failure_does_not_fail_the_run() {
    let tmp = tempfile::tempdir().expect("tmp");
    let blocker = tmp.path().join("file");
    std::fs::write(&blocker, "x").expect("write");
    let sink = EventSink::new(blocker.join("events.jsonl"), "001");
    let r = runner(sink);
    let run = r.run(&spec("ok", tmp.path()));
    assert_eq!(run.stop_reason, StopReason::Done);
}

#[test]
fn fake_runner_replays_script_and_records_calls() {
    let fake = FakeRunner::new(vec![AgentRun::done("one"), AgentRun::failed("two")]);
    let tmp = tempfile::tempdir().expect("tmp");
    let a = fake.run(&spec("a", tmp.path()));
    let b = fake.run(&spec("b", tmp.path()));
    let c = fake.run(&spec("c", tmp.path()));
    assert_eq!(a.result_text, "one");
    assert_eq!(b.stop_reason, StopReason::Failed);
    assert_eq!(c.error.as_deref(), Some("fake script exhausted"));
    let prompts: Vec<String> = fake.calls().into_iter().map(|s| s.prompt).collect();
    assert_eq!(prompts, vec!["a", "b", "c"]);
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-agent --test runner`
Expected: FAIL to compile (`ClaudeRunner`, `AgentRunner`, `FakeRunner` not found).

- [ ] **Step 4: Write `runner.rs`**

```rust
use std::collections::VecDeque;
use std::sync::Mutex;

use crate::run::AgentRun;
use crate::spec::AgentSpec;

/// Runs one agent to completion. Failures are returned as `StopReason::Failed`, never as a panic.
pub trait AgentRunner {
    fn run(&self, spec: &AgentSpec) -> AgentRun;
}

/// A scripted runner for engine tests. It replays queued runs in order.
#[derive(Debug, Default)]
pub struct FakeRunner {
    script: Mutex<VecDeque<AgentRun>>,
    calls: Mutex<Vec<AgentSpec>>,
}

impl FakeRunner {
    pub fn new(script: Vec<AgentRun>) -> FakeRunner {
        FakeRunner { script: Mutex::new(script.into()), calls: Mutex::new(Vec::new()) }
    }

    /// Every spec the runner was called with, in order.
    pub fn calls(&self) -> Vec<AgentSpec> {
        match self.calls.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }
}

impl AgentRunner for FakeRunner {
    fn run(&self, spec: &AgentSpec) -> AgentRun {
        match self.calls.lock() {
            Ok(mut g) => g.push(spec.clone()),
            Err(p) => p.into_inner().push(spec.clone()),
        }
        let next = match self.script.lock() {
            Ok(mut g) => g.pop_front(),
            Err(p) => p.into_inner().pop_front(),
        };
        next.map_or_else(|| AgentRun::failed("fake script exhausted"), |r| r)
    }
}
```

- [ ] **Step 5: Write `claude.rs`**

Design points to keep: the prompt is written on its own thread; stdout is read on a reader thread that always drains to EOF; stderr is drained into a 4 KB tail; the main loop polls with a 100 ms timeout so budget, turn and stall checks never wait on the pipe. When the run is stopped by the driver, the reader and stderr threads are detached and not joined, because a grandchild process may still hold the pipe open.

```rust
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use factory_core::{budget_state, BudgetState};

use crate::events::{AgentEvent, EventKind, EventSink};
use crate::run::{AgentRun, StopReason};
use crate::runner::AgentRunner;
use crate::spec::{AgentSpec, Caps};
use crate::stream::{StreamEvent, StreamState};

const POLL: Duration = Duration::from_millis(100);
const MAX_LINE: usize = 1 << 20;
const STDERR_TAIL: usize = 4096;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("cannot start the agent process: {0}")]
    Spawn(std::io::Error),
    #[error("the agent process has no stdout or stderr pipe")]
    Pipe,
    #[error("waiting for the agent process failed: {0}")]
    Wait(std::io::Error),
}

/// Runs headless `claude -p` and reads its stream-json output.
#[derive(Debug)]
pub struct ClaudeRunner {
    program: OsString,
    caps: Caps,
    sink: EventSink,
}

enum Msg {
    Line(String),
    Eof,
}

impl ClaudeRunner {
    pub fn new(program: impl Into<OsString>, caps: Caps, sink: EventSink) -> ClaudeRunner {
        ClaudeRunner { program: program.into(), caps, sink }
    }

    fn event(&self, spec: &AgentSpec, kind: EventKind, tokens: u64) -> AgentEvent {
        AgentEvent {
            kind,
            agent_id: spec.agent_id(),
            agent_type: spec.agent.clone().map_or_else(|| spec.role.clone(), |a| a),
            tool_name: None,
            tool_target: None,
            cwd: Some(spec.cwd.display().to_string()),
            last_message: None,
            tokens,
        }
    }

    fn run_inner(&self, spec: &AgentSpec) -> Result<AgentRun, AgentError> {
        let mut child = Command::new(&self.program)
            .args(spec.args(&self.caps))
            .current_dir(&spec.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(AgentError::Spawn)?;
        let (stdin, stdout, stderr) = (child.stdin.take(), child.stdout.take(), child.stderr.take());
        let (Some(stdout), Some(stderr)) = (stdout, stderr) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AgentError::Pipe);
        };

        let prompt = spec.stdin_prompt(&self.caps);
        let writer = thread::spawn(move || {
            if let Some(mut s) = stdin {
                // The child may exit before reading everything, so errors are ignored.
                let _ = s.write_all(prompt.as_bytes());
            }
        });
        let err_thread = thread::spawn(move || tail_of(stderr, STDERR_TAIL));
        let (tx, rx) = mpsc::channel::<Msg>();
        let reader = thread::spawn(move || read_lines(stdout, tx));

        let mut state = StreamState::default();
        let mut last_event = Instant::now();
        let mut stop: Option<StopReason> = None;
        let mut warned = false;
        self.sink.emit(&self.event(spec, EventKind::Start, spec.used_before));

        loop {
            match rx.recv_timeout(POLL) {
                Ok(Msg::Line(line)) => {
                    last_event = Instant::now();
                    let events = state.feed(&line);
                    let used = spec.used_before.saturating_add(state.usage().weighted());
                    for ev in events {
                        if let StreamEvent::Tool { name, target } = ev {
                            let mut e = self.event(spec, EventKind::Tool, used);
                            e.tool_name = Some(name);
                            e.tool_target = Some(target);
                            self.sink.emit(&e);
                        }
                    }
                    if let Some(limit) = spec.budget_weighted {
                        match budget_state(used, limit) {
                            BudgetState::Stop => {
                                stop = Some(StopReason::BudgetStop);
                                break;
                            }
                            BudgetState::Warn if !warned => {
                                warned = true;
                                self.sink.emit(&self.event(spec, EventKind::BudgetWarn, used));
                            }
                            _ => {}
                        }
                    }
                    if let Some(max) = spec.max_turns {
                        if state.turns() > max {
                            stop = Some(StopReason::TurnLimit);
                            break;
                        }
                    }
                }
                Ok(Msg::Eof) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {
                    if let Some(limit) = spec.stall_after {
                        if last_event.elapsed() >= limit {
                            stop = Some(StopReason::Stalled);
                            break;
                        }
                    }
                }
            }
        }

        let killed = stop.is_some();
        if killed {
            let _ = child.kill();
        }
        let status = child.wait().map_err(AgentError::Wait)?;
        let stderr_tail = if killed {
            // A grandchild may still hold the pipes, so the helper threads are left to end on their own.
            String::new()
        } else {
            let _ = writer.join();
            let _ = reader.join();
            err_thread.join().map_or(String::new(), |t| t)
        };

        let usage = state.usage();
        let weighted = usage.weighted();
        let exit_code = status.code();
        let result_text = state.result_text.clone().map_or(String::new(), |t| t);
        let (stop_reason, error) = match stop {
            Some(reason) => (reason, None),
            None if !state.has_result() => {
                let msg = format!("claude exited without a result (exit {exit_code:?}): {}", stderr_tail.trim());
                (StopReason::Failed, Some(msg))
            }
            None if state.result_is_error => {
                let subtype = state.result_subtype.clone().map_or(String::new(), |s| s);
                let reason = if subtype.contains("max_turns") { StopReason::TurnLimit } else { StopReason::Failed };
                (reason, Some(format!("claude reported {subtype}")))
            }
            None => (StopReason::Done, None),
        };

        let mut stop_event = self.event(spec, EventKind::Stop, spec.used_before.saturating_add(weighted));
        stop_event.last_message = Some(if result_text.is_empty() { error.clone().map_or(String::new(), |e| e) } else { result_text.clone() });
        self.sink.emit(&stop_event);

        Ok(AgentRun {
            session_id: state.session_id.clone(),
            usage,
            weighted,
            result_text,
            stop_reason,
            turns: state.turns(),
            exit_code,
            error,
        })
    }
}

impl AgentRunner for ClaudeRunner {
    fn run(&self, spec: &AgentSpec) -> AgentRun {
        match self.run_inner(spec) {
            Ok(run) => run,
            Err(e) => AgentRun::failed(&e.to_string()),
        }
    }
}

/// Forwards stdout lines. It always drains to EOF, so the child never blocks on a full pipe.
fn read_lines(stdout: impl Read, tx: mpsc::Sender<Msg>) {
    let mut reader = BufReader::new(stdout);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) => break,
            Ok(_) => {
                buf.truncate(MAX_LINE);
                let line = String::from_utf8_lossy(&buf).into_owned();
                if tx.send(Msg::Line(line)).is_err() {
                    // The runner stopped listening. Keep draining so the child is not blocked.
                    let _ = std::io::copy(&mut reader, &mut std::io::sink());
                    return;
                }
            }
            Err(_) => {
                let _ = std::io::copy(&mut reader, &mut std::io::sink());
                break;
            }
        }
    }
    let _ = tx.send(Msg::Eof);
}

/// Reads a stream to the end and keeps only the last `keep` bytes.
fn tail_of(mut r: impl Read, keep: usize) -> String {
    let mut tail: Vec<u8> = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match r.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                tail.extend_from_slice(&buf[..n]);
                if tail.len() > keep {
                    let cut = tail.len() - keep;
                    tail.drain(..cut);
                }
            }
        }
    }
    String::from_utf8_lossy(&tail).into_owned()
}
```

The `BufRead` import is used by `read_until`. If the compiler reports `Read` or `BufRead` as unused, remove the unused one.

- [ ] **Step 6: Wire `lib.rs`**

Add to `src/lib.rs`:

```rust
pub mod claude;
pub mod runner;

pub use claude::{AgentError, ClaudeRunner};
pub use runner::{AgentRunner, FakeRunner};
```

- [ ] **Step 7: Run the runner tests**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-agent --test runner`
Expected: all 12 tests PASS in well under a minute. Fix `budget_state` variant names here if plan 01 differs. If `used_before_counts_toward_the_budget` fails because `budget_state` stops at a different threshold than `used >= limit`, adjust the numeric bounds in the tests to the real threshold, not the runner.

- [ ] **Step 8: Final verification**

Run these in order:

```bash
cargo check -j4 --manifest-path tools/factory/Cargo.toml
cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-agent
```

Expected: check clean, every `factory-agent` test passes (stream and runner suites). Then confirm no `.unwrap()` crept into the crate:

```bash
grep -rn '\.unwrap()' tools/factory/crates/factory-agent/ || echo "no unwrap"
```

Expected: `no unwrap`.

- [ ] **Step 9: Commit**

Invoke `dev-workflow-skills:1-git-commit` (inline git), then:

```bash
git add tools/factory/crates/factory-agent
git commit --only -m "feat(factory): add claude runner with budget kill and stall detection" -- tools/factory/crates/factory-agent
```

---

## Self-Review Notes

- Spec coverage: probe and recorded report (Task 1), `Usage`, parser with per-message dedup, `AgentSpec`, `AgentRun`, `StopReason`, `EventSink` with `job_id` and `tokens` (Task 2), `AgentRunner`, `ClaudeRunner` with budget kill, stall kill, no pipe block, `FakeRunner`, fake binary tests (Task 3).
- Type consistency: `AgentRunner::run(&self, &AgentSpec) -> AgentRun` is used by the trait, both runners and every test. `Caps` has exactly `agent`, `max_turns`, `append_system_prompt` everywhere.
- Known external assumption: the signatures of `factory_core::weighted_tokens` and `budget_state`. Task 2 and Task 3 say where to adapt.
- Engine note for plan 04: `AgentSpec.used_before` is the job's weighted total so far, and the engine must store `AgentRun.session_id` after every run for `--resume`.

Next plan: docs/superpowers/plans/2026-10-13-software-factory-v3/plan-04-checks-and-engine.md
