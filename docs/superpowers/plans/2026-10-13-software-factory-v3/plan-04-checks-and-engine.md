# Software Factory v3 Plan 04: Checks and Engine (build, checks, review, decide)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:executing-plans, executed by a `factory-builder` subagent. Skip that skill's final whole-branch review and its ledger scripts and its progress file, because the factory's own reviewer and driver do those. Steps use checkbox (`- [ ]`) syntax for tracking.

> **Plans 01, 02 and 03 of this series must be finished and merged first.** This plan consumes `factory-core`, `factory-git` and `factory-agent`. Plans 01, 02 and 03 fix the exact names this plan calls (`plan-01-core.md`, `plan-02-git.md`, `plan-03-agent-runner.md`). Step 0 of Task 2 is a verification step: confirm the real source still matches the signatures written below. If one differs, fix it in the small adapter files `ports.rs`, `specs.rs` and `tests/common/mod.rs`, never the behavior.

**Goal:** Build the `factory-checks` crate (path to check mapping, enforced `-j4`, check runner, `checks.md` writer) and the first half of `factory-engine` (`Engine`, `build_stage`, `checks_stage`, `review_stage`, `Decision`, `decide`).

**Architecture:** `factory-checks` is pure and has no knowledge of the board. `factory-engine` owns the stage logic. It reads and writes the board and ledger only through `factory-core`, builds headless `AgentSpec` values for the builder and reviewer, runs them through any `AgentRunner`, and gets diffs from `factory-git`. A stage returns a `StageOutcome`; the loop that chains stages, plus merge, sync and series, is plan 05.

**Tech Stack:** Rust 2021, `thiserror` 1, `git2` 0.19 (engine dev-dependency, test fixtures only), `tempfile` 3 (dev). No async.

**Spec:** `docs/superpowers/specs/2026-10-13-software-factory-v3-rust-driver-design.md` (decisions 4, 5, 6, 7, 9, 12, 13, crate table, plan 04 row). Behavior ported from `.claude/commands/factory.md` (build, review stages, rounds), `.claude/agents/factory-builder.md`, `.claude/agents/factory-code-reviewer.md` and the rules of `docs/superpowers/plans/2026-10-13-software-factory-v2/plan-07-lean-mode.md`.

## Global Constraints

- Rust workspace at `tools/factory/` (own workspace, own target dir, not in the root workspace). Crate directories are `tools/factory/crates/<name>/`.
- Pinned versions: serde 1, serde_json 1, toml 0.8, thiserror 1, clap 4 (derive), git2 0.19, tempfile 3 (dev), fs2 0.4.
- Never `.unwrap()` anywhere, tests included (a harness hook blocks it in any Rust write). `.expect("...")` only in tests. Production code returns `Result` with `thiserror` error types.
- Commands: `cargo check -j4 --manifest-path tools/factory/Cargo.toml` and `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p <crate> <test_name>`. Never `--workspace`, never `--all`, never omit `-j4`, never set `CARGO_TARGET_DIR`.
- Comments are short full sentences that end with a period. No emojis.
- Commit steps: invoke the skill `dev-workflow-skills:1-git-commit`, but run its git commands inline with no helper agents. Use a pathspec commit: `git add <paths> && git commit --only -m "..." -- <paths>`. Never `git add -A` or `.`. Never `HARNESS_GATE=0`.
- Agents never write the board or the ledger. The engine records every change through `factory-core`.
- Spec decision 7: auto-merge needs no failing check, last verdict exactly `VERDICT: APPROVE`, diff class not risky, diff within the cap, budget not at stop. Otherwise a fix round (at most `fix_rounds`, default 2) or `needs-human`.
- Spec decision 6: a risky diff always goes to `needs-human` after the reviewer ran, with a summary of the risky paths. A missing or broken config makes every diff risky (this is `factory-core`'s `classify_paths`; do not reimplement it).
- Spec decision 9 defaults: diff cap 4000 changed lines, 2 fix rounds.
- A last-line verdict only counts when exact. A missing or malformed verdict counts as CHANGES.
- Cargo commands in `[[checks]]` always carry `-j4` and never `--workspace` or `--all`. A command that breaks this is rejected at config load.

## Review Focus

- A `cargo` check command with `--workspace`, `--all`, no `-j4`, or `-j 3` must be rejected, and `--all-targets` must stay allowed. Tested in Task 1.
- A check command whose program does not exist, or that hangs, must become a `FAIL` line (not a panic, not a hang). Tested in Task 1.
- A changed-path list that matches no rule (docs only) must still write a `checks.md` with a `PASS` line, so `can-auto-merge` finds evidence. Tested in Task 1.
- A resumed builder that makes no new commit, or a first run with no commit, must park the job, never loop. Tested in Task 2.
- A reviewer reply whose `VERDICT: APPROVE` is not the very last line (trailing text, code fence, lowercase) must count as CHANGES. Tested in Task 3.
- A risky diff with `VERDICT: APPROVE` and green checks must still be `NeedsHuman`, with the risky paths in the reason. Tested in Task 3.

---

## File Structure

| File | Task | Responsibility |
|---|---|---|
| `tools/factory/Cargo.toml` | 1 | Add the two new members only if the workspace lists members explicitly. |
| `tools/factory/crates/factory-checks/Cargo.toml` | 1 | Crate manifest. |
| `tools/factory/crates/factory-checks/src/lib.rs` | 1 | Re-exports and `CheckError`. |
| `tools/factory/crates/factory-checks/src/plan.rs` | 1 | `CheckPlan`, `plan_checks`, `validate_command`, `validate_config`. |
| `tools/factory/crates/factory-checks/src/run.rs` | 1 | `run_checks`, `ChecksReport`, `CheckResult`, trimming, `write_checks_md`. |
| `tools/factory/crates/factory-engine/Cargo.toml` | 2 | Crate manifest. |
| `tools/factory/crates/factory-engine/src/lib.rs` | 2 | `Engine`, `EngineError`, `StageOutcome`, re-exports. |
| `tools/factory/crates/factory-engine/src/ports.rs` | 2 | The only file that touches the `factory-core` board API and `factory-git` signatures. |
| `tools/factory/crates/factory-engine/src/specs.rs` | 2, 3 | The only file that builds `AgentSpec` values and prompt text. |
| `tools/factory/crates/factory-engine/src/build.rs` | 2 | `build_stage`. |
| `tools/factory/crates/factory-engine/src/checks_stage.rs` | 2 | `checks_stage`, fix round bookkeeping. |
| `tools/factory/crates/factory-engine/src/decide.rs` | 3 | `Decision`, `DecideInput`, `decide`, `risk_summary`. |
| `tools/factory/crates/factory-engine/src/review.rs` | 3 | `review_stage`, `apply_decision`, `final_checks`. |
| `tools/factory/crates/factory-engine/tests/common/mod.rs` | 2 | Temp repo, board fixture, scripted runner helpers. |
| `tools/factory/crates/factory-engine/tests/build_checks.rs` | 2 | Build and checks stage tests. |
| `tools/factory/crates/factory-engine/tests/review_decide.rs` | 3 | Review stage and decision tests. |

---

### Task 1: `factory-checks` crate

**Files:**
- Create: `tools/factory/crates/factory-checks/Cargo.toml`, `src/lib.rs`, `src/plan.rs`, `src/run.rs` (all under `tools/factory/crates/factory-checks/`)
- Modify: `tools/factory/Cargo.toml` (only if members are listed one by one)

**Interfaces:**
- Consumes (from plan 01, `factory-core`): `Config` with the public field `checks: Vec<CheckRule>`; `CheckRule { pub paths: Vec<String>, pub commands: Vec<String> }` with `CheckRule::matches(&self, path: &str) -> bool` (the `[[checks]]` table of `.claude/factory/config.toml`; it matches with plan 01's `glob_match`, so `*` stays inside one path segment, `**` spans segments, and the shipped pattern `crates/*/**` works); `write_atomic(path: &Path, bytes: &[u8]) -> Result<(), CoreError>`. Verify with `grep -n "pub struct CheckRule" -A8 tools/factory/crates/factory-core/src/*.rs`.
- Command templates may contain the placeholder `{crate}`. It expands once per distinct crate directory name found as `crates/<name>/` in a matching changed path. Path patterns are plan 01 globs (`CheckRule::matches`): `**` for everything, `dir/**` for anything under `dir/`, `**/*.ext` for an extension, and exact paths. Do not write a second glob matcher.
- Produces (exact):
  - `pub enum CheckError { Empty, MissingJobs(String), Forbidden { flag: &'static str, cmd: String }, Write(String) }` (thiserror)
  - `pub const DEFAULT_TIMEOUT: Duration` (900 s)
  - `pub struct CheckPlan { pub commands: Vec<String>, pub timeout: Duration }`
  - `pub fn validate_command(cmd: &str) -> Result<(), CheckError>`
  - `pub fn validate_config(cfg: &Config) -> Result<(), CheckError>`
  - `pub fn plan_checks(changed_paths: &[String], cfg: &Config) -> CheckPlan`
  - `pub struct CheckResult { pub command: String, pub passed: bool, pub summary: String, pub tail: String }`
  - `pub struct ChecksReport { pub results: Vec<CheckResult> }` with `all_passed(&self) -> bool`, `failures(&self) -> Vec<&CheckResult>`, `failure_text(&self) -> String`
  - `pub fn run_checks(worktree: &Path, plan: &CheckPlan) -> ChecksReport`
  - `pub fn trim_tail(text: &str, max_lines: usize) -> String`
  - `pub fn write_checks_md(path: &Path, report: &ChecksReport) -> Result<(), CheckError>`

- [ ] **Step 1: Create the crate skeleton**

`tools/factory/crates/factory-checks/Cargo.toml`:

```toml
[package]
name = "factory-checks"
version = "0.1.0"
edition = "2021"

[dependencies]
factory-core = { path = "../factory-core" }
thiserror = "1"

[dev-dependencies]
tempfile = "3"
```

`src/lib.rs`:

```rust
//! Path to check mapping and check execution for the factory.

mod plan;
mod run;

pub use plan::{plan_checks, validate_command, validate_config, CheckPlan, DEFAULT_TIMEOUT};
pub use run::{run_checks, trim_tail, write_checks_md, CheckResult, ChecksReport};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CheckError {
    #[error("empty check command")]
    Empty,
    #[error("cargo command must pass -j4: {0}")]
    MissingJobs(String),
    #[error("cargo command must not use {flag}: {cmd}")]
    Forbidden { flag: &'static str, cmd: String },
    #[error("could not write checks file: {0}")]
    Write(String),
}
```

If `tools/factory/Cargo.toml` lists members one by one, add `"crates/factory-checks"`. If it uses `members = ["crates/*"]`, leave it.

- [ ] **Step 2: Write the failing plan tests**

Create `src/plan.rs` with only the test module for now (the items it imports come in Step 4):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use factory_core::{CheckRule, Config};

    fn cfg(rules: Vec<(&[&str], &[&str])>) -> Config {
        let mut c = Config::default();
        c.checks = rules
            .into_iter()
            .map(|(p, cmds)| CheckRule {
                paths: p.iter().map(|s| s.to_string()).collect(),
                commands: cmds.iter().map(|s| s.to_string()).collect(),
            })
            .collect();
        c
    }

    fn paths(p: &[&str]) -> Vec<String> {
        p.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn rejects_cargo_without_j4() {
        assert!(matches!(validate_command("cargo check"), Err(crate::CheckError::MissingJobs(_))));
        assert!(matches!(validate_command("cargo test -j 3 -p a"), Err(crate::CheckError::MissingJobs(_))));
    }

    #[test]
    fn rejects_workspace_and_all() {
        assert!(matches!(validate_command("cargo test -j4 --workspace"), Err(crate::CheckError::Forbidden { .. })));
        assert!(matches!(validate_command("cargo check -j4 --all"), Err(crate::CheckError::Forbidden { .. })));
    }

    #[test]
    fn accepts_good_commands() {
        assert!(validate_command("cargo check -j4 --all-targets").is_ok());
        assert!(validate_command("cargo test -j 4 -p rocket-http").is_ok());
        assert!(validate_command("yarn tsc --noEmit").is_ok());
        assert!(matches!(validate_command("  "), Err(crate::CheckError::Empty)));
    }

    #[test]
    fn config_with_a_bad_command_is_rejected() {
        let c = cfg(vec![(&["**"], &["cargo test --workspace -j4"])]);
        assert!(validate_config(&c).is_err());
    }

    #[test]
    fn plan_is_ordered_and_deduplicated() {
        let c = cfg(vec![
            (&["crates/**"], &["cargo check -j4", "cargo test -j4 -p {crate}"]),
            (&["src-tauri/**"], &["cargo check -j4"]),
            (&["src/**"], &["yarn tsc --noEmit"]),
        ]);
        let plan = plan_checks(
            &paths(&["crates/rocket-http/src/a.rs", "crates/rocket-git/src/b.rs", "src-tauri/src/x.rs", "src/app.ts"]),
            &c,
        );
        assert_eq!(
            plan.commands,
            vec![
                "cargo check -j4",
                "cargo test -j4 -p rocket-git",
                "cargo test -j4 -p rocket-http",
                "yarn tsc --noEmit",
            ]
        );
    }

    #[test]
    fn plan_skips_unmatched_rules_and_unsafe_commands() {
        let c = cfg(vec![
            (&["docs/**"], &["yarn check"]),
            (&["**/*.md"], &["cargo test --workspace -j4"]),
        ]);
        assert!(plan_checks(&paths(&["src/a.ts"]), &c).commands.is_empty());
        assert!(plan_checks(&paths(&["README.md"]), &c).commands.is_empty());
    }

    #[test]
    fn crate_placeholder_without_a_crate_path_is_skipped() {
        let c = cfg(vec![(&["**"], &["cargo test -j4 -p {crate}"])]);
        assert!(plan_checks(&paths(&["README.md"]), &c).commands.is_empty());
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-checks plan`
Expected: compile error (`validate_command` and friends not found).

- [ ] **Step 4: Implement `plan.rs`**

Put this above the test module in `src/plan.rs`:

```rust
use std::collections::BTreeSet;
use std::time::Duration;

use factory_core::Config;

use crate::CheckError;

/// Default time limit for one check command.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(900);

/// The ordered, de-duplicated commands to run for one diff.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckPlan {
    pub commands: Vec<String>,
    pub timeout: Duration,
}

/// Rejects a cargo command that lacks `-j4` or uses `--workspace` or `--all`.
pub fn validate_command(cmd: &str) -> Result<(), CheckError> {
    let tokens: Vec<&str> = cmd.split_whitespace().collect();
    let Some(first) = tokens.first() else {
        return Err(CheckError::Empty);
    };
    if *first != "cargo" {
        return Ok(());
    }
    for flag in ["--workspace", "--all"] {
        if tokens.contains(&flag) {
            return Err(CheckError::Forbidden { flag, cmd: cmd.to_string() });
        }
    }
    let has_j4 = tokens.contains(&"-j4") || tokens.windows(2).any(|w| w[0] == "-j" && w[1] == "4");
    if has_j4 {
        Ok(())
    } else {
        Err(CheckError::MissingJobs(cmd.to_string()))
    }
}

/// Validates every command of every `[[checks]]` rule. The engine calls this at start up.
pub fn validate_config(cfg: &Config) -> Result<(), CheckError> {
    for rule in &cfg.checks {
        for cmd in &rule.commands {
            validate_command(cmd)?;
        }
    }
    Ok(())
}

/// Returns the directory name after `crates/` in a path, if there is one.
fn crate_of(path: &str) -> Option<String> {
    let mut parts = path.split('/');
    while let Some(part) = parts.next() {
        if part == "crates" {
            return parts.next().filter(|n| !n.is_empty()).map(|n| n.to_string());
        }
    }
    None
}

/// Chooses the check commands for a set of changed paths.
pub fn plan_checks(changed_paths: &[String], cfg: &Config) -> CheckPlan {
    let mut seen = BTreeSet::new();
    let mut commands = Vec::new();
    for rule in &cfg.checks {
        let matched: Vec<&String> = changed_paths
            .iter()
            .filter(|p| rule.matches(p))
            .collect();
        if matched.is_empty() {
            continue;
        }
        let crates: BTreeSet<String> = matched.iter().filter_map(|p| crate_of(p)).collect();
        for template in &rule.commands {
            let expanded: Vec<String> = if template.contains("{crate}") {
                crates.iter().map(|c| template.replace("{crate}", c)).collect()
            } else {
                vec![template.clone()]
            };
            for cmd in expanded {
                // A command that breaks the cargo rules never runs.
                if validate_command(&cmd).is_ok() && seen.insert(cmd.clone()) {
                    commands.push(cmd);
                }
            }
        }
    }
    CheckPlan { commands, timeout: DEFAULT_TIMEOUT }
}
```

The `crates` set is a `BTreeSet`, so the expansion order is alphabetical (the Step 2 test expects `rocket-git` before `rocket-http`).

- [ ] **Step 5: Run the plan tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-checks plan`
Expected: PASS (7 tests).

- [ ] **Step 6: Write the failing runner and writer tests**

Create `src/run.rs` with only this test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::CheckPlan;
    use std::time::Duration;

    fn plan(cmds: &[&str], secs: u64) -> CheckPlan {
        CheckPlan { commands: cmds.iter().map(|s| s.to_string()).collect(), timeout: Duration::from_secs(secs) }
    }

    #[test]
    fn passing_and_failing_commands() {
        let dir = tempfile::tempdir().expect("tempdir");
        let r = run_checks(dir.path(), &plan(&["true", "false", "echo hello world"], 10));
        assert_eq!(r.results.len(), 3);
        assert!(r.results[0].passed);
        assert!(!r.results[1].passed);
        assert!(r.results[2].passed);
        assert_eq!(r.results[2].summary, "hello world");
        assert!(!r.all_passed());
        assert_eq!(r.failures().len(), 1);
    }

    #[test]
    fn missing_program_is_a_failure_not_a_panic() {
        let dir = tempfile::tempdir().expect("tempdir");
        let r = run_checks(dir.path(), &plan(&["definitely-not-a-real-program-xyz"], 10));
        assert!(!r.results[0].passed);
        assert!(r.results[0].summary.contains("could not start"));
    }

    #[test]
    fn timeout_is_a_failure() {
        let dir = tempfile::tempdir().expect("tempdir");
        let r = run_checks(dir.path(), &plan(&["sleep 5"], 1));
        assert!(!r.results[0].passed);
        assert!(r.results[0].summary.contains("timed out"));
    }

    #[test]
    fn runs_in_the_worktree() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("marker.txt"), "x").expect("write");
        let r = run_checks(dir.path(), &plan(&["ls marker.txt"], 10));
        assert!(r.results[0].passed);
    }

    #[test]
    fn trim_keeps_the_last_lines() {
        let text: String = (1..=100).map(|n| format!("line {n}\n")).collect();
        let out = trim_tail(&text, 60);
        assert_eq!(out.lines().count(), 61);
        assert!(out.starts_with("... 40 earlier lines omitted"));
        assert!(out.ends_with("line 100"));
        assert_eq!(trim_tail("a\nb\n", 60), "a\nb");
    }

    #[test]
    fn failure_text_names_each_failing_command() {
        let report = ChecksReport {
            results: vec![
                CheckResult { command: "cargo check -j4".into(), passed: false, summary: "exit 101".into(), tail: "error[E0432]".into() },
                CheckResult { command: "yarn check".into(), passed: true, summary: "ok".into(), tail: String::new() },
            ],
        };
        let text = report.failure_text();
        assert!(text.contains("cargo check -j4 (exit 101)"));
        assert!(text.contains("error[E0432]"));
        assert!(!text.contains("yarn check"));
    }

    #[test]
    fn checks_md_has_one_evidence_line_per_command() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("checks.md");
        let report = ChecksReport {
            results: vec![
                CheckResult { command: "cargo check -j4".into(), passed: true, summary: "ok".into(), tail: String::new() },
                CheckResult { command: "yarn tsc --noEmit".into(), passed: false, summary: "exit 2\nsecond line".into(), tail: "TS2304".into() },
            ],
        };
        write_checks_md(&path, &report).expect("write");
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.starts_with("# Checks\n\n"));
        assert!(text.contains("\nPASS cargo check -j4 (ok)\n"));
        assert!(text.contains("\nFAIL yarn tsc --noEmit (exit 2 second line)\n"));
        assert!(!text.contains("TS2304"));
    }

    #[test]
    fn empty_report_still_records_a_pass_line() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("checks.md");
        write_checks_md(&path, &ChecksReport { results: vec![] }).expect("write");
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.contains("PASS none (no check is mapped to the changed paths)"));
    }
}
```

- [ ] **Step 7: Run to verify failure**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-checks run`
Expected: compile error (`run_checks` not found).

- [ ] **Step 8: Implement `run.rs`**

Put this above the test module:

```rust
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use factory_core::write_atomic;

use crate::plan::CheckPlan;
use crate::CheckError;

const TAIL_LINES: usize = 60;
const SUMMARY_CHARS: usize = 100;

/// The outcome of one check command.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckResult {
    pub command: String,
    pub passed: bool,
    /// A short one-line result. For a failure it is the exit status or timeout note.
    pub summary: String,
    /// The last 60 lines of output. Empty for a passing command.
    pub tail: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChecksReport {
    pub results: Vec<CheckResult>,
}

impl ChecksReport {
    /// An empty report passes.
    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.passed)
    }

    pub fn failures(&self) -> Vec<&CheckResult> {
        self.results.iter().filter(|r| !r.passed).collect()
    }

    /// The text handed to the builder: each failing command and its trimmed output.
    pub fn failure_text(&self) -> String {
        let mut out = String::new();
        for r in self.failures() {
            out.push_str(&format!("## {} ({})\n{}\n\n", r.command, r.summary, r.tail));
        }
        out
    }
}

/// Keeps the last `max_lines` lines and notes how many were dropped.
pub fn trim_tail(text: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = text.trim_end().lines().collect();
    if lines.len() <= max_lines {
        return lines.join("\n");
    }
    let dropped = lines.len() - max_lines;
    format!("... {dropped} earlier lines omitted\n{}", lines[dropped..].join("\n"))
}

fn one_line(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    flat.chars().take(SUMMARY_CHARS).collect()
}

/// Picks the line that best says what a passing command did.
fn pass_summary(output: &str) -> String {
    let lines: Vec<&str> = output.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let pick = lines.iter().rev().find(|l| l.contains("test result:")).or_else(|| lines.last());
    match pick {
        Some(l) => one_line(l),
        None => "ok".to_string(),
    }
}

fn read_all<R: Read>(reader: Option<R>) -> String {
    let mut buf = Vec::new();
    if let Some(mut r) = reader {
        let _ = r.read_to_end(&mut buf);
    }
    String::from_utf8_lossy(&buf).into_owned()
}

fn failed(command: &str, summary: String, tail: String) -> CheckResult {
    CheckResult { command: command.to_string(), passed: false, summary, tail }
}

fn run_one(worktree: &Path, command: &str, timeout: Duration) -> CheckResult {
    let mut tokens = command.split_whitespace();
    let Some(program) = tokens.next() else {
        return failed(command, "could not start: empty command".into(), String::new());
    };
    let mut child = match Command::new(program)
        .args(tokens)
        .current_dir(worktree)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return failed(command, format!("could not start: {e}"), String::new()),
    };
    let out_reader = child.stdout.take();
    let err_reader = child.stderr.take();
    let out_thread = thread::spawn(move || read_all(out_reader));
    let err_thread = thread::spawn(move || read_all(err_reader));
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                let _ = child.kill();
                return failed(command, format!("could not wait: {e}"), String::new());
            }
        }
    };
    let stdout = match out_thread.join() {
        Ok(s) => s,
        Err(_) => String::new(),
    };
    let stderr = match err_thread.join() {
        Ok(s) => s,
        Err(_) => String::new(),
    };
    // Stderr goes last because compilers print their errors there.
    let text = format!("{stdout}{stderr}");
    match status {
        None => failed(command, format!("timed out after {}s", timeout.as_secs()), trim_tail(&text, TAIL_LINES)),
        Some(s) if s.success() => CheckResult {
            command: command.to_string(),
            passed: true,
            summary: pass_summary(&text),
            tail: String::new(),
        },
        Some(s) => {
            let code = s.code().map(|c| c.to_string()).unwrap_or_else(|| "signal".to_string());
            failed(command, format!("exit {code}"), trim_tail(&text, TAIL_LINES))
        }
    }
}

/// Runs each command with the worktree as the working directory.
pub fn run_checks(worktree: &Path, plan: &CheckPlan) -> ChecksReport {
    ChecksReport { results: plan.commands.iter().map(|c| run_one(worktree, c, plan.timeout)).collect() }
}

/// Writes `checks.md`. This is the evidence format that `can-auto-merge` reads.
pub fn write_checks_md(path: &Path, report: &ChecksReport) -> Result<(), CheckError> {
    let mut body = String::from("# Checks\n\n");
    if report.results.is_empty() {
        body.push_str("PASS none (no check is mapped to the changed paths)\n");
    }
    for r in &report.results {
        let word = if r.passed { "PASS" } else { "FAIL" };
        body.push_str(&format!("{word} {} ({})\n", r.command, one_line(&r.summary)));
    }
    write_atomic(path, body.as_bytes()).map_err(|e| CheckError::Write(e.to_string()))
}
```

`unwrap_or_else` is allowed (it is not `.unwrap()`).

- [ ] **Step 9: Run the crate tests**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-checks`
Expected: PASS (15 tests). Then `cargo check -j4 --manifest-path tools/factory/Cargo.toml` must be clean.

- [ ] **Step 10: Commit**

Invoke skill `dev-workflow-skills:1-git-commit` (run its git commands inline, no helper agents):

```bash
git add tools/factory/crates/factory-checks tools/factory/Cargo.toml tools/factory/Cargo.lock
git commit --only -m "feat(factory): add checks crate with enforced -j4" -- tools/factory/crates/factory-checks tools/factory/Cargo.toml tools/factory/Cargo.lock
```

(Drop any path from the list that does not exist or did not change.)

---

### Task 2: `factory-engine` build and checks stages

**Files:**
- Create: everything under `tools/factory/crates/factory-engine/` named in the file table for Task 2 (`Cargo.toml`, `src/lib.rs`, `src/ports.rs`, `src/specs.rs`, `src/build.rs`, `src/checks_stage.rs`, `tests/common/mod.rs`, `tests/build_checks.rs`), and `tools/factory/crates/factory-core/tests/job_tokens.rs`
- Modify: `tools/factory/crates/factory-core/src/board.rs` and `src/lib.rs` (Step 0b adds `Job.prev_head` and `Job.tokens`), `tools/factory/Cargo.toml` (only if members are listed one by one)

**Interfaces:**
- Consumes `factory-checks` (Task 1), and the real signatures of plans 01, 02 and 03:
  - `factory-core` (plan 01): `Board::load(&Path) -> Result<Board, CoreError>`, `Board::save(&self, &Path) -> Result<(), CoreError>`, `Board::find(&self, id: &str) -> Result<&Job, CoreError>`, `Board::find_mut(&mut self, id: &str) -> Result<&mut Job, CoreError>`, `Board.jobs: Vec<Job>` (public, push a new job), `Job::new(id, dir, slug, created, feature, now: f64) -> Job` (sets `branch` to `factory/<id>-<slug>`, stage `spec`, `round` 0), `Job` fields `slug`, `branch`, `worktree: Option<String>`, `base: Option<String>`, `head: Option<String>`, `builder_id: Option<String>` (holds the agent session id), `round: u32`, `Job::set_stage(&mut self, to: Stage, reason: Option<&str>, force: bool, now: f64) -> Result<(), CoreError>` (a backward move such as review to build needs `force = true`), `Job::set_base(&mut self, sha: &str, force: bool) -> Result<(), CoreError>`, `Stage::{Spec, Build, Review, Approve, Merged, NeedsHuman}`, `Ledger::new(path)` with `task(n: u32, TaskState, Option<&str>)` and `ruling(&str)` (one line only, `Ruling: ...`), `TaskState::{Dispatched, ImplementerDone, Complete}`, `Config` fields `fix_rounds: u32`, `diff_cap_lines: u32`, `stall_minutes: u32`, `budget_weighted: u64`, `builder_model: String`, `reviewer_model: String`, `checks: Vec<CheckRule>`. Plan 01 has no `prev_head` and no per-job token total: Step 0b of this task adds `Job.prev_head: Option<String>` and `Job.tokens: JobTokens`.
  - `factory-git` (plan 02): `Repo::tip(&self, branch: &str) -> Result<String>`, `Repo::create_worktree(&self, job: &JobRef) -> Result<Worktree>` with `JobRef { id: String, slug: String }` and `Worktree { name, path: PathBuf, branch: String, base: String }` (it creates branch `factory/<id>-<slug>` from main and the worktree at `<repo>/.claude/worktrees/factory-<id>-<slug>`, so the engine does not choose a path), `Repo::changed_paths(&self, base: &str, head: &str) -> Result<Vec<String>>`, `Repo::diff_to_files(&self, base: &str, head: &str, dir: &Path) -> Result<DiffFiles>` with `DiffFiles { diff: PathBuf, stat: PathBuf, names: PathBuf, files, insertions, deletions }` and `DiffFiles::changed_lines(&self) -> usize` (the file names carry the two short shas, so two ranges never overwrite each other).
  - `factory-agent` (plan 03): `AgentRunner::run(&self, spec: &AgentSpec) -> AgentRun` (it never returns an error, a failure is `StopReason::Failed`), `AgentSpec::new(job_id: &str, role: &str, cwd: impl Into<PathBuf>, prompt: &str)` with public `model: Option<String>`, `resume: Option<String>`, `allowed_tools: Vec<String>`, `budget_weighted: Option<u64>`, `used_before: u64`, `stall_after: Option<Duration>`, `AgentRun { session_id: Option<String>, usage: Usage, weighted: u64, result_text: String, stop_reason: StopReason, turns, exit_code, error: Option<String> }`, `AgentRun::done(&str)` and `AgentRun::failed(&str)`, `StopReason::{Done, BudgetStop, Stalled, TurnLimit, Failed}`, `Usage { input_tokens, cache_creation_input_tokens, cache_read_input_tokens, output_tokens }` (all `u64`), `EventSink::new(path, job_id: &str)`, `FakeRunner::new(Vec<AgentRun>)` (replays fixed runs and records `calls()`, it cannot run code, so engine tests that need a commit use the `ScriptedRunner` of `tests/common/mod.rs`).
  - **Step 0 (do before anything else):** this is a verification step. Run `grep -rn "pub fn\|pub struct\|pub enum\|pub trait" tools/factory/crates/factory-core/src tools/factory/crates/factory-git/src tools/factory/crates/factory-agent/src` and confirm the signatures above still hold. They are copied from plans 01, 02 and 03, so the code below needs no adaptation. If the merged source differs from a plan, fix only `ports.rs`, `specs.rs` and `tests/common/mod.rs`, and say so in the commit message.
- Produces (exact):
  - `pub struct Engine<'a> { pub board_path: PathBuf, pub job_dir: PathBuf, pub job_id: String, pub plan_file: PathBuf, pub repo: &'a Repo, pub runner: &'a dyn AgentRunner, pub config: Config, pub events: &'a EventSink }`
  - `impl<'a> Engine<'a> { pub fn new(...) -> Result<Self, EngineError> }` (validates the config with `validate_config`; the argument order is the field order above)
  - `pub enum StageOutcome { Advanced(Stage), NeedsHuman(String) }`
  - `pub enum EngineError { Core(String), Git(String), Agent(String), Check(String), Io(String), Msg(String) }`
  - `Engine::build_stage(&self) -> Result<StageOutcome, EngineError>` and `Engine::checks_stage(&self) -> Result<StageOutcome, EngineError>`, plus free wrappers `pub fn build_stage(e: &Engine) -> ...` and `pub fn checks_stage(e: &Engine) -> ...`.
  - Board contract used by plan 05: after `build_stage` the stage is `review`, `head` is set (and `prev_head` on a fix round), `builder_id` holds the session id, `tokens` holds the summed usage of every agent run. The engine counts rounds from 1 and treats the board value 0 as 1. After `checks_stage` fails with rounds left, the stage is `build` (a forced backward move with a `Ruling:` ledger line), `round` is incremented and `changes-r<round>.md` exists in the job dir. `build_stage` on a later round reads that file.

- [ ] **Step 0: Verify the real core, git and agent signatures** with the grep above. Expect no differences.

- [ ] **Step 0b: Add `prev_head` and a token tally to `Job` in `factory-core`**

Plan 01's `Job` has neither field, and the engine needs both (the round diff and the budget). Add them with `#[serde(default)]` so v1 boards still load. In `tools/factory/crates/factory-core/src/board.rs`, above `Job`:

```rust
/// Token totals of one job, summed over its agent runs. New in v3, absent in v1 boards.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobTokens {
    pub input: u64,
    pub cache_creation: u64,
    pub output: u64,
    pub cache_read: u64,
}

impl JobTokens {
    pub fn is_zero(&self) -> bool {
        *self == JobTokens::default()
    }

    /// Adds one run. Saturates instead of overflowing.
    pub fn add(&mut self, input: u64, cache_creation: u64, output: u64, cache_read: u64) {
        self.input = self.input.saturating_add(input);
        self.cache_creation = self.cache_creation.saturating_add(cache_creation);
        self.output = self.output.saturating_add(output);
        self.cache_read = self.cache_read.saturating_add(cache_read);
    }

    /// The number the budget is measured in, see `weighted_tokens`.
    pub fn weighted(&self) -> u64 {
        crate::budget::weighted_tokens(self.input, self.cache_creation, self.output, self.cache_read)
    }
}
```

In `struct Job`, after `model`, add:

```rust
    /// The head before the latest fix round. New in v3, absent in v1 boards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_head: Option<String>,
    /// Summed agent token usage. New in v3, absent in v1 boards.
    #[serde(default, skip_serializing_if = "JobTokens::is_zero")]
    pub tokens: JobTokens,
```

In `Job::new`, after `model: None,` add `prev_head: None,` and `tokens: JobTokens::default(),`. In `src/lib.rs` change the board re-export to `pub use board::{Board, Job, JobTokens, ReviewState};`.

`tools/factory/crates/factory-core/tests/job_tokens.rs`:

```rust
use factory_core::{Job, JobTokens};

#[test]
fn a_fresh_job_serializes_without_the_new_fields_and_loads_back() {
    let job = Job::new("001", "2026-10-13-001-demo", "demo", "2026-10-13", "demo feature", 1.0);
    let text = serde_json::to_string(&job).expect("json");
    assert!(!text.contains("prev_head"), "{text}");
    assert!(!text.contains("tokens"), "{text}");
    let back: Job = serde_json::from_str(&text).expect("parse");
    assert_eq!(back, job);
}

#[test]
fn prev_head_and_tokens_round_trip() {
    let mut job = Job::new("001", "2026-10-13-001-demo", "demo", "2026-10-13", "demo feature", 1.0);
    job.prev_head = Some("abc".to_string());
    job.tokens.add(100, 50, 25, 1000);
    let back: Job = serde_json::from_str(&serde_json::to_string(&job).expect("json")).expect("parse");
    assert_eq!(back, job);
}

#[test]
fn tokens_add_and_weigh() {
    let mut t = JobTokens::default();
    t.add(100, 50, 25, 1000);
    t.add(1, 1, 1, 10);
    // 101 input, 51 cache creation, 26 output and 1010 / 10 cache read.
    assert_eq!(t.weighted(), 101 + 51 + 26 + 101);
}
```

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-core`. Expected: every plan 01 test and the 3 new tests pass.

- [ ] **Step 1: Create the crate manifest and skeleton**

`tools/factory/crates/factory-engine/Cargo.toml`:

```toml
[package]
name = "factory-engine"
version = "0.1.0"
edition = "2021"

[dependencies]
factory-core = { path = "../factory-core" }
factory-git = { path = "../factory-git" }
factory-agent = { path = "../factory-agent" }
factory-checks = { path = "../factory-checks" }
thiserror = "1"

[dev-dependencies]
git2 = "0.19"
tempfile = "3"
```

`src/lib.rs`:

```rust
//! The factory run loop stages: build, checks, review and decide.

mod build;
mod checks_stage;
mod decide;
mod ports;
mod review;
mod specs;

use std::path::PathBuf;

use factory_agent::{AgentRunner, EventSink};
use factory_checks::validate_config;
use factory_core::{Config, Stage};
use factory_git::Repo;
use thiserror::Error;

pub use build::build_stage;
pub use checks_stage::checks_stage;
pub use decide::{decide, risk_summary, DecideInput, Decision};
pub use review::{apply_decision, final_checks, review_stage, ReviewOutcome};

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("core: {0}")]
    Core(String),
    #[error("git: {0}")]
    Git(String),
    #[error("agent: {0}")]
    Agent(String),
    #[error("checks: {0}")]
    Check(String),
    #[error("io: {0}")]
    Io(String),
    #[error("{0}")]
    Msg(String),
}

/// What a stage did to the job. The caller chains stages by the new board stage.
#[derive(Debug, Clone, PartialEq)]
pub enum StageOutcome {
    Advanced(Stage),
    NeedsHuman(String),
}

pub struct Engine<'a> {
    pub board_path: PathBuf,
    pub job_dir: PathBuf,
    pub job_id: String,
    pub plan_file: PathBuf,
    pub repo: &'a Repo,
    pub runner: &'a dyn AgentRunner,
    pub config: Config,
    pub events: &'a EventSink,
}

impl<'a> Engine<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        board_path: PathBuf,
        job_dir: PathBuf,
        job_id: String,
        plan_file: PathBuf,
        repo: &'a Repo,
        runner: &'a dyn AgentRunner,
        config: Config,
        events: &'a EventSink,
    ) -> Result<Self, EngineError> {
        validate_config(&config).map_err(|e| EngineError::Check(e.to_string()))?;
        Ok(Self { board_path, job_dir, job_id, plan_file, repo, runner, config, events })
    }
}
```

Add `"crates/factory-engine"` to `tools/factory/Cargo.toml` members if they are explicit. Create empty `src/decide.rs` and `src/review.rs` stubs now only so the crate compiles in Step 5; they are filled in Task 3. Stub content for `decide.rs`: `pub use crate::EngineError as _Unused;` is not allowed as filler, so instead leave Task 3 items out of `lib.rs` for now: remove the `decide` and `review` lines (the `mod` lines and the two `pub use` lines) and add them back at the start of Task 3 Step 1.

- [ ] **Step 2: Write `ports.rs` (the only board adapter)**

```rust
use std::fmt::Display;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use factory_agent::{AgentRun, AgentSpec};
use factory_core::{Board, CoreError, Job, Ledger, Stage, TaskState};

use crate::{Engine, EngineError};

pub(crate) fn core_err<E: Display>(e: E) -> EngineError {
    EngineError::Core(e.to_string())
}
pub(crate) fn git_err<E: Display>(e: E) -> EngineError {
    EngineError::Git(e.to_string())
}
pub(crate) fn io_err<E: Display>(e: E) -> EngineError {
    EngineError::Io(e.to_string())
}

/// Seconds since the Unix epoch, the unit of `Job::stage_since`.
pub(crate) fn now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// A copy of the board fields the stages read.
#[derive(Debug, Clone)]
pub(crate) struct JobView {
    pub slug: String,
    pub branch: String,
    pub worktree: Option<PathBuf>,
    pub base: Option<String>,
    pub head: Option<String>,
    pub prev_head: Option<String>,
    pub session_id: Option<String>,
    pub round: u32,
}

impl Engine<'_> {
    pub(crate) fn job(&self) -> Result<JobView, EngineError> {
        let board = Board::load(&self.board_path).map_err(core_err)?;
        let job = board.find(&self.job_id).map_err(core_err)?;
        Ok(JobView {
            slug: job.slug.clone(),
            branch: job.branch.clone(),
            worktree: job.worktree.as_ref().map(PathBuf::from),
            base: job.base.clone(),
            head: job.head.clone(),
            prev_head: job.prev_head.clone(),
            session_id: job.builder_id.clone(),
            // Plan 01 starts a job at round 0. The engine counts rounds from 1.
            round: job.round.max(1),
        })
    }

    /// Loads the board, applies one change, and saves it atomically.
    pub(crate) fn update(&self, f: impl FnOnce(&mut Job) -> Result<(), CoreError>) -> Result<(), EngineError> {
        let mut board = Board::load(&self.board_path).map_err(core_err)?;
        let job = board.find_mut(&self.job_id).map_err(core_err)?;
        f(job).map_err(core_err)?;
        board.save(&self.board_path).map_err(core_err)
    }

    /// Moves the job to a stage. Only a fix round (review back to build) passes `force`.
    pub(crate) fn set_stage(&self, stage: Stage, reason: Option<&str>, force: bool) -> Result<(), EngineError> {
        self.update(|j| j.set_stage(stage, reason, force, now()))
    }

    fn ledger_file(&self) -> Ledger {
        Ledger::new(self.job_dir.join("progress.md"))
    }

    /// Appends a strict `Task N: <state> (detail)` line.
    pub(crate) fn ledger_task(&self, n: u32, state: TaskState, detail: &str) -> Result<(), EngineError> {
        self.ledger_file().task(n, state, Some(detail)).map_err(core_err)
    }

    /// Appends a `Ruling:` line. The ledger takes one line, so whitespace and newlines fold into single spaces.
    pub(crate) fn ruling(&self, text: &str) -> Result<(), EngineError> {
        let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
        self.ledger_file().ruling(&one_line).map_err(core_err)
    }

    /// Parks the job for the human with a reason, and tells the caller.
    pub(crate) fn park(&self, reason: String) -> Result<crate::StageOutcome, EngineError> {
        self.set_stage(Stage::NeedsHuman, Some(&reason), false)?;
        self.ruling(&format!("needs-human, {reason}"))?;
        Ok(crate::StageOutcome::NeedsHuman(reason))
    }

    /// Runs one agent and adds its token usage to the job tally. The runner never fails, a failure is a stop reason.
    pub(crate) fn run_agent(&self, spec: AgentSpec) -> Result<AgentRun, EngineError> {
        let run = self.runner.run(&spec);
        let u = run.usage;
        self.update(|j| {
            j.tokens.add(u.input_tokens, u.cache_creation_input_tokens, u.output_tokens, u.cache_read_input_tokens);
            Ok(())
        })?;
        Ok(run)
    }

    /// The job's weighted token tally from the board. A load error counts as zero, because `job()` already failed earlier in the same stage.
    pub(crate) fn job_weighted_tokens(&self) -> u64 {
        match Board::load(&self.board_path) {
            Ok(b) => b.find(&self.job_id).map(|j| j.tokens.weighted()).unwrap_or(0),
            Err(_) => 0,
        }
    }
}
```

- [ ] **Step 3: Write `specs.rs` (builder prompt and spec)**

```rust
use std::path::Path;
use std::time::Duration;

use factory_agent::AgentSpec;

use crate::ports::JobView;
use crate::Engine;

impl Engine<'_> {
    /// Sets the model, the budget and the stall limit from the config. Every agent spec goes through this.
    pub(crate) fn with_limits(&self, mut spec: AgentSpec, model: &str) -> AgentSpec {
        spec.model = Some(model.to_string());
        spec.budget_weighted = Some(self.config.budget_weighted);
        spec.used_before = self.job_weighted_tokens();
        spec.stall_after = Some(Duration::from_secs(u64::from(self.config.stall_minutes) * 60));
        spec
    }

    /// The headless builder for the first run of a job.
    pub(crate) fn builder_first_spec(&self, job: &JobView, cwd: &Path) -> AgentSpec {
        let prompt = format!(
            "You are the builder in Rocket's software factory. Your working directory is the job worktree, already on branch `{branch}`.\n\
             Read CLAUDE.md, the rule files for the areas you touch under .claude/rules/, and the plan at {plan}. Never edit the plan.\n\
             Do the plan task by task: write the failing test first, then the code, then run the checks.\n\
             Rules: run `cargo check -j4` and `cargo test -j4 -p <crate> <test_name>`; never use --workspace or --all; never set CARGO_TARGET_DIR; \
             run `yarn install --frozen-lockfile` once before the first yarn check; no unwrap in production Rust; no git CLI in Rust code; \
             shadcn/ui, lucide-react, SingleLineEditor and Monaco rules apply.\n\
             Commit each task with the dev-workflow-skills:1-git-commit skill and a pathspec commit (`git add <paths> && git commit --only -m \"...\" -- <paths>`). \
             Never use `git add -A`, `--all` or `.`. Never push, never merge, never touch the board or the ledger.\n\
             If you are blocked, say why in your final reply and stop. Your last line must be `HEAD: <sha of your last commit>`.",
            branch = job.branch,
            plan = self.plan_file.display(),
        );
        self.with_limits(AgentSpec::new(&self.job_id, "builder", cwd, &prompt), &self.config.builder_model)
    }

    /// The resumed builder for a fix round.
    pub(crate) fn builder_resume_spec(&self, job: &JobView, cwd: &Path, findings: &Path, session: &str) -> AgentSpec {
        let prompt = format!(
            "Fix only the problems listed in {findings}. Make new commits on branch `{branch}`. Do not amend or rewrite earlier commits. \
             Run the relevant checks (`cargo check -j4`, targeted `cargo test -j4 -p <crate> <test_name>`, yarn checks) before each commit. \
             Never push, never merge. Your last line must be `HEAD: <sha of your last commit>`.",
            findings = findings.display(),
            branch = job.branch,
        );
        let mut spec = AgentSpec::new(&self.job_id, "builder", cwd, &prompt);
        spec.resume = Some(session.to_string());
        self.with_limits(spec, &self.config.builder_model)
    }
}
```

- [ ] **Step 4: Write the test fixtures**

`tests/common/mod.rs` (written against the real plan 01 and plan 03 signatures):

```rust
#![allow(dead_code)]

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use factory_agent::{AgentRun, AgentRunner, AgentSpec, EventSink};
use factory_core::{Board, CheckRule, Config, Job};
use factory_engine::Engine;
use factory_git::Repo;
use git2::{Repository, Signature};

/// Commits one file in the repository at `dir` and returns the new sha.
pub fn commit_file(dir: &Path, name: &str, content: &str) -> String {
    let repo = Repository::open(dir).expect("open repo");
    std::fs::write(dir.join(name), content).expect("write file");
    let mut index = repo.index().expect("index");
    index.add_path(Path::new(name)).expect("add");
    index.write().expect("write index");
    let tree = repo.find_tree(index.write_tree().expect("tree")).expect("find tree");
    let sig = Signature::now("t", "t@example.com").expect("sig");
    let parents: Vec<git2::Commit> = match repo.head() {
        Ok(h) => vec![h.peel_to_commit().expect("commit")],
        Err(_) => vec![],
    };
    let refs: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, "test commit", &tree, &refs).expect("commit").to_string()
}

pub struct Fixture {
    pub tmp: tempfile::TempDir,
    pub main: PathBuf,
    pub board_path: PathBuf,
    pub job_dir: PathBuf,
    pub plan_file: PathBuf,
    pub repo: Repo,
    pub config: Config,
    pub events: EventSink,
}

impl Fixture {
    /// A temp repo on `main` with one commit, plus a board holding job `001`.
    pub fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let main = tmp.path().join("main");
        std::fs::create_dir_all(&main).expect("mkdir");
        let mut opts = git2::RepositoryInitOptions::new();
        opts.initial_head("main");
        Repository::init_opts(&main, &opts).expect("init");
        commit_file(&main, "README.md", "hello\n");
        let job_dir = tmp.path().join("job");
        std::fs::create_dir_all(&job_dir).expect("mkdir job");
        let plan_file = job_dir.join("plan.md");
        std::fs::write(&plan_file, "- [ ] Task 1\n").expect("plan");
        let board_path = tmp.path().join("board.json");
        let mut board = Board::default();
        board.jobs.push(Job::new("001", "2026-10-13-001-demo", "demo", "2026-10-13", "demo feature", 0.0));
        board.save(&board_path).expect("save board");
        let mut config = Config::default();
        config.fix_rounds = 2;
        config.diff_cap_lines = 4000;
        config.checks = vec![CheckRule { paths: vec!["**".into()], commands: vec!["true".into()] }];
        let repo = Repo::open(&main).expect("open");
        let events = EventSink::new(job_dir.join("events.jsonl"), "001");
        Self { tmp, main, board_path, job_dir, plan_file, repo, config, events }
    }

    pub fn engine<'a>(&'a self, runner: &'a dyn AgentRunner) -> Engine<'a> {
        Engine::new(
            self.board_path.clone(),
            self.job_dir.clone(),
            "001".to_string(),
            self.plan_file.clone(),
            &self.repo,
            runner,
            self.config.clone(),
            &self.events,
        )
        .expect("engine")
    }

    pub fn job(&self) -> Job {
        let board = Board::load(&self.board_path).expect("load");
        board.find("001").expect("job").clone()
    }
}
```

Plan 03's `FakeRunner` only replays fixed `AgentRun` values, so it cannot commit into the worktree. Add this runner to the same file. It does not change `factory-agent`. Each effect runs when the runner is called (it may commit into `spec.cwd`) and returns the reply text, which becomes an `AgentRun::done` with the fixed session id `"sess-1"` and zero usage:

```rust
pub type Effect = Box<dyn Fn(&AgentSpec) -> String + Send + Sync>;

/// Plays one effect per call and records every spec it was given.
pub struct ScriptedRunner {
    effects: Mutex<VecDeque<Effect>>,
    calls: Mutex<Vec<AgentSpec>>,
}

impl ScriptedRunner {
    pub fn last_spec(&self) -> Option<AgentSpec> {
        self.calls.lock().expect("calls").last().cloned()
    }
}

impl AgentRunner for ScriptedRunner {
    fn run(&self, spec: &AgentSpec) -> AgentRun {
        self.calls.lock().expect("calls").push(spec.clone());
        let next = self.effects.lock().expect("effects").pop_front();
        match next {
            Some(effect) => AgentRun { session_id: Some("sess-1".to_string()), ..AgentRun::done(&effect(spec)) },
            None => AgentRun::failed("script exhausted"),
        }
    }
}

pub fn scripted(effects: Vec<Effect>) -> ScriptedRunner {
    ScriptedRunner { effects: Mutex::new(effects.into()), calls: Mutex::new(Vec::new()) }
}
```

Tests below call `scripted(vec![...])`.

- [ ] **Step 5: Write the failing build stage tests**

`tests/build_checks.rs`:

```rust
mod common;

use common::{commit_file, scripted, Effect, Fixture, ScriptedRunner};
use factory_core::Stage;
use factory_engine::StageOutcome;

fn commit_effect(file: &'static str, lines: usize) -> Effect {
    Box::new(move |spec| {
        let body: String = (0..lines).map(|n| format!("line {n}\n")).collect();
        let sha = commit_file(&spec.cwd, file, &body);
        format!("done\nHEAD: {sha}")
    })
}

fn silent_effect() -> Effect {
    Box::new(|_| "nothing to do".to_string())
}

#[test]
fn first_build_records_head_session_and_moves_to_review() {
    let fx = Fixture::new();
    let runner = scripted(vec![commit_effect("a.txt", 3)]);
    let e = fx.engine(&runner);
    let out = e.build_stage().expect("build");
    assert_eq!(out, StageOutcome::Advanced(Stage::Review));
    let job = fx.job();
    assert!(job.head.is_some());
    assert_ne!(job.head, job.base);
    assert!(job.prev_head.is_none());
    assert_eq!(job.builder_id.as_deref(), Some("sess-1"));
    assert_eq!(job.stage, Stage::Review);
}

#[test]
fn first_build_with_no_commit_parks_the_job() {
    let fx = Fixture::new();
    let runner = scripted(vec![silent_effect()]);
    let out = fx.engine(&runner).build_stage().expect("build");
    match out {
        StageOutcome::NeedsHuman(r) => assert!(r.contains("no commits"), "{r}"),
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(fx.job().stage, Stage::NeedsHuman);
}

#[test]
fn resume_uses_the_session_and_records_prev_head() {
    let fx = Fixture::new();
    let runner = scripted(vec![commit_effect("a.txt", 3), commit_effect("b.txt", 2)]);
    let e = fx.engine(&runner);
    e.build_stage().expect("first");
    let first_head = fx.job().head.clone();
    // Simulate a failed check round: round 2, findings file, stage build.
    std::fs::write(fx.job_dir.join("changes-r2.md"), "fix it").expect("findings");
    let mut board = factory_core::Board::load(&fx.board_path).expect("load");
    let job = board.find_mut("001").expect("job");
    job.round = 2;
    job.set_stage(Stage::Build, None, true, 1.0).expect("stage");
    board.save(&fx.board_path).expect("save");
    let out = e.build_stage().expect("resume");
    assert_eq!(out, StageOutcome::Advanced(Stage::Review));
    let job = fx.job();
    assert_eq!(job.prev_head, first_head);
    assert_ne!(job.head, first_head);
}

#[test]
fn resume_with_no_new_commit_parks_the_job() {
    let fx = Fixture::new();
    let runner = scripted(vec![commit_effect("a.txt", 3), silent_effect()]);
    let e = fx.engine(&runner);
    e.build_stage().expect("first");
    std::fs::write(fx.job_dir.join("changes-r2.md"), "fix it").expect("findings");
    let mut board = factory_core::Board::load(&fx.board_path).expect("load");
    let job = board.find_mut("001").expect("job");
    job.round = 2;
    job.set_stage(Stage::Build, None, true, 1.0).expect("stage");
    board.save(&fx.board_path).expect("save");
    match e.build_stage().expect("resume") {
        StageOutcome::NeedsHuman(r) => assert!(r.contains("no new commit"), "{r}"),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn diff_over_the_cap_parks_the_job() {
    let mut fx = Fixture::new();
    fx.config.diff_cap_lines = 5;
    let runner = scripted(vec![commit_effect("big.txt", 50)]);
    match fx.engine(&runner).build_stage().expect("build") {
        StageOutcome::NeedsHuman(r) => assert!(r.contains("cap"), "{r}"),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn bad_config_is_rejected_when_the_engine_is_built() {
    let mut fx = Fixture::new();
    fx.config.checks = vec![factory_core::CheckRule {
        paths: vec!["**".into()],
        commands: vec!["cargo test --workspace -j4".into()],
    }];
    let runner = scripted(vec![]);
    let res = factory_engine::Engine::new(
        fx.board_path.clone(), fx.job_dir.clone(), "001".into(), fx.plan_file.clone(),
        &fx.repo, &runner, fx.config.clone(), &fx.events,
    );
    assert!(res.is_err());
}

fn built(fx: &Fixture) -> ScriptedRunner {
    let runner = scripted(vec![commit_effect("a.txt", 3)]);
    fx.engine(&runner).build_stage().expect("build");
    runner
}

#[test]
fn passing_checks_write_checks_md_and_stay_in_review() {
    let fx = Fixture::new();
    let runner = built(&fx);
    let out = fx.engine(&runner).checks_stage().expect("checks");
    assert_eq!(out, StageOutcome::Advanced(Stage::Review));
    let md = std::fs::read_to_string(fx.job_dir.join("checks.md")).expect("checks.md");
    assert!(md.contains("PASS true ("), "{md}");
}

#[test]
fn failing_checks_start_a_fix_round() {
    let mut fx = Fixture::new();
    fx.config.checks = vec![factory_core::CheckRule { paths: vec!["**".into()], commands: vec!["false".into()] }];
    let runner = built(&fx);
    let out = fx.engine(&runner).checks_stage().expect("checks");
    assert_eq!(out, StageOutcome::Advanced(Stage::Build));
    let job = fx.job();
    assert_eq!(job.round, 2);
    assert_eq!(job.stage, Stage::Build);
    let findings = std::fs::read_to_string(fx.job_dir.join("changes-r2.md")).expect("findings");
    assert!(findings.contains("false"), "{findings}");
    let md = std::fs::read_to_string(fx.job_dir.join("checks.md")).expect("checks.md");
    assert!(md.contains("FAIL false (exit 1)"), "{md}");
}

#[test]
fn failing_checks_beyond_fix_rounds_need_a_human() {
    let mut fx = Fixture::new();
    fx.config.fix_rounds = 1;
    fx.config.checks = vec![factory_core::CheckRule { paths: vec!["**".into()], commands: vec!["false".into()] }];
    let runner = built(&fx);
    let e = fx.engine(&runner);
    assert_eq!(e.checks_stage().expect("round 1"), StageOutcome::Advanced(Stage::Build));
    // Put the job back in review at round 2, as a second build would.
    let mut board = factory_core::Board::load(&fx.board_path).expect("load");
    board.find_mut("001").expect("job").set_stage(Stage::Review, None, false, 2.0).expect("stage");
    board.save(&fx.board_path).expect("save");
    match e.checks_stage().expect("round 2") {
        StageOutcome::NeedsHuman(r) => assert!(r.contains("fix rounds"), "{r}"),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn docs_only_diff_still_writes_a_pass_line() {
    let mut fx = Fixture::new();
    fx.config.checks = vec![factory_core::CheckRule { paths: vec!["src/**".into()], commands: vec!["false".into()] }];
    let runner = built(&fx);
    fx.engine(&runner).checks_stage().expect("checks");
    let md = std::fs::read_to_string(fx.job_dir.join("checks.md")).expect("checks.md");
    assert!(md.contains("PASS none"), "{md}");
}
```

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test build_checks`
Expected: compile error (`build_stage` not found).

- [ ] **Step 6: Implement `build.rs`**

```rust
use std::path::PathBuf;

use factory_agent::StopReason;
use factory_core::{Stage, TaskState};
use factory_git::JobRef;

use crate::ports::{git_err, now};
use crate::{Engine, EngineError, StageOutcome};

impl Engine<'_> {
    /// Runs the builder (first run) or resumes it (fix round), then records the new head.
    pub fn build_stage(&self) -> Result<StageOutcome, EngineError> {
        let job = self.job()?;
        let first = job.session_id.is_none();
        let (cwd, base, previous_head, branch) = if first {
            // The git layer picks the branch, the worktree path and the base (the main tip).
            let wt = self
                .repo
                .create_worktree(&JobRef { id: self.job_id.clone(), slug: job.slug.clone() })
                .map_err(git_err)?;
            let (path, base, branch) = (wt.path.to_string_lossy().to_string(), wt.base.clone(), wt.branch.clone());
            let (b, br) = (base.clone(), branch.clone());
            self.update(|j| {
                j.worktree = Some(path);
                j.branch = br;
                j.set_base(&b, false)
            })?;
            self.ledger_task(1, TaskState::Dispatched, &format!("BASE {base}, {}", self.config.builder_model))?;
            (wt.path, base, None, branch)
        } else {
            let cwd = job.worktree.clone().ok_or_else(|| EngineError::Msg("job has no worktree".into()))?;
            let base = job.base.clone().ok_or_else(|| EngineError::Msg("job has no base".into()))?;
            (cwd, base, job.head.clone(), job.branch.clone())
        };
        let spec = match (&job.session_id, first) {
            (Some(session), false) => {
                let findings = self.job_dir.join(format!("changes-r{}.md", job.round));
                if !findings.exists() {
                    return Err(EngineError::Msg(format!("missing findings file {}", findings.display())));
                }
                self.builder_resume_spec(&job, &cwd, &findings, session)
            }
            _ => self.builder_first_spec(&job, &cwd),
        };
        let run = self.run_agent(spec)?;
        let session = run.session_id.clone().or(job.session_id.clone());
        self.update(|j| {
            j.builder_id = session;
            Ok(())
        })?;
        if run.stop_reason != StopReason::Done {
            return self.park(format!("builder stopped: {:?}", run.stop_reason));
        }
        let head = self.repo.tip(&branch).map_err(git_err)?;
        if first && head == base {
            return self.park("builder made no commits".to_string());
        }
        if !first && Some(&head) == previous_head.as_ref() {
            return self.park("builder made no new commit on resume".to_string());
        }
        let info = self.repo.diff_to_files(&base, &head, &self.job_dir).map_err(git_err)?;
        let cap = self.config.diff_cap_lines as usize;
        if info.changed_lines() > cap {
            return self.park(format!("diff has {} changed lines, the cap is {cap}", info.changed_lines()));
        }
        let h = head.clone();
        self.update(|j| {
            j.prev_head = previous_head;
            j.head = Some(h);
            j.set_stage(Stage::Review, None, false, now())
        })?;
        self.ruling(&format!("head of round {} is {head}", job.round))?;
        Ok(StageOutcome::Advanced(Stage::Review))
    }

    /// The directory of the worktree, or an error when the job has none yet.
    pub(crate) fn worktree(&self) -> Result<PathBuf, EngineError> {
        self.job()?.worktree.ok_or_else(|| EngineError::Msg("job has no worktree".into()))
    }
}

pub fn build_stage(e: &Engine) -> Result<StageOutcome, EngineError> {
    e.build_stage()
}
```

A stage that updates the board can leave the stage as `review` while `build_stage` is mid-flight, so nothing else writes between the two `update` calls; plan 05 holds the job lock.

- [ ] **Step 7: Implement `checks_stage.rs`**

```rust
use factory_checks::{plan_checks, run_checks, write_checks_md, ChecksReport};
use factory_core::Stage;

use crate::ports::{git_err, io_err, now};
use crate::{Engine, EngineError, StageOutcome};

impl Engine<'_> {
    /// Runs the mapped checks over the whole job diff and writes `checks.md`.
    pub(crate) fn run_checks_now(&self) -> Result<ChecksReport, EngineError> {
        let job = self.job()?;
        let base = job.base.clone().ok_or_else(|| EngineError::Msg("job has no base".into()))?;
        let head = job.head.clone().ok_or_else(|| EngineError::Msg("job has no head".into()))?;
        let paths = self.repo.changed_paths(&base, &head).map_err(git_err)?;
        let plan = plan_checks(&paths, &self.config);
        let report = run_checks(&self.worktree()?, &plan);
        write_checks_md(&self.job_dir.join("checks.md"), &report).map_err(|e| EngineError::Check(e.to_string()))?;
        Ok(report)
    }

    /// Starts a fix round with `findings`, or parks the job when the rounds are used up.
    pub(crate) fn fix_or_park(&self, findings: &str, reason: &str) -> Result<StageOutcome, EngineError> {
        let job = self.job()?;
        let used = job.round.saturating_sub(1);
        if used >= self.config.fix_rounds {
            return self.park(format!("{reason} after {used} fix rounds"));
        }
        let next = job.round + 1;
        std::fs::write(self.job_dir.join(format!("changes-r{next}.md")), findings).map_err(io_err)?;
        // A fix round moves review back to build. That is a backward move, so it is forced and logged as a ruling.
        self.update(|j| {
            j.round = next;
            j.set_stage(Stage::Build, None, true, now())
        })?;
        self.ruling(&format!("fix round {next}, {reason}"))?;
        Ok(StageOutcome::Advanced(Stage::Build))
    }

    /// Runs the checks. On a failure the job goes back to `build` with the trimmed output.
    pub fn checks_stage(&self) -> Result<StageOutcome, EngineError> {
        let report = self.run_checks_now()?;
        let round = self.job()?.round;
        if report.all_passed() {
            self.ruling(&format!("checks round {round} pass"))?;
            return Ok(StageOutcome::Advanced(Stage::Review));
        }
        self.ruling(&format!("checks round {round} fail"))?;
        let findings = format!("The checks failed. Fix these failures only.\n\n{}", report.failure_text());
        self.fix_or_park(&findings, "checks failed")
    }
}

pub fn checks_stage(e: &Engine) -> Result<StageOutcome, EngineError> {
    e.checks_stage()
}
```

- [ ] **Step 8: Run the engine tests and a full check**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test build_checks`
Expected: PASS (10 tests). If a test fails because the merged source differs from the signatures in Step 0, fix the adapter file named there, not the test intent. Then `cargo check -j4 --manifest-path tools/factory/Cargo.toml`.

- [ ] **Step 9: Commit**

Invoke skill `dev-workflow-skills:1-git-commit`, inline git:

```bash
git add tools/factory/crates/factory-engine tools/factory/crates/factory-core tools/factory/Cargo.toml tools/factory/Cargo.lock
git commit --only -m "feat(factory): add engine build and checks stages" -- tools/factory/crates/factory-engine tools/factory/crates/factory-core tools/factory/Cargo.toml tools/factory/Cargo.lock
```

---

### Task 3: `review_stage`, `Decision` and `decide`

**Files:**
- Create: `tools/factory/crates/factory-engine/src/decide.rs`, `src/review.rs`, `tests/review_decide.rs`
- Modify: `tools/factory/crates/factory-engine/src/lib.rs` (add back `mod decide; mod review;` and the two `pub use` lines from Task 2 Step 1), `src/specs.rs` (add the reviewer spec)

**Interfaces:**
- Consumes: Task 2 (`Engine`, `StageOutcome`, `fix_or_park`, `run_checks_now`, `park`, `run_agent`, `worktree`); `factory-checks::ChecksReport`; `factory-core`: `Verdict::{Approve, Changes, Missing}`, `parse_verdict(&str) -> Verdict` (only an exact last line `VERDICT: APPROVE` or `VERDICT: CHANGES` counts), `DiffClass::{DocOnly, Normal, Risky}`, `classify_paths(&[String], &Config) -> DiffClass` (fails closed on a broken config), `BudgetState::{Ok, Warn, Stop}`, `budget_state(used: u64, budget: u64) -> BudgetState`, and `Config.budget_weighted: u64`. The budget `used` value is `Engine::job_weighted_tokens()` from Task 2 (it reads `Job.tokens.weighted()` from the board, which `run_agent` keeps current), never hard-coded to `Ok`. Verify the names with the grep of Task 2 Step 0.
- Produces (exact):
  - `pub enum Decision { AutoMerge, FixRound, NeedsHuman(String) }`
  - `pub struct DecideInput<'a> { pub checks_ok: bool, pub verdict: &'a Verdict, pub class: DiffClass, pub risky_summary: String, pub diff_lines: usize, pub diff_cap: usize, pub budget: BudgetState, pub round: u32, pub fix_rounds: u32 }`
  - `pub fn decide(input: &DecideInput) -> Decision`
  - `pub fn risk_summary(paths: &[String], cfg: &Config) -> String`
  - `pub struct ReviewOutcome { pub verdict: Verdict, pub review_path: PathBuf, pub decision: Decision }`
  - `Engine::review_stage(&self) -> Result<ReviewOutcome, EngineError>` and free wrapper `pub fn review_stage(e: &Engine) -> Result<ReviewOutcome, EngineError>`
  - `Engine::apply_decision(&self, outcome: &ReviewOutcome) -> Result<StageOutcome, EngineError>` (and free wrapper `apply_decision`): `AutoMerge` runs `final_checks`, then sets stage `approve`; `FixRound` writes `changes-r<round+1>.md` from the review text and sets stage `build`; `NeedsHuman` parks.
  - `Engine::final_checks(&self) -> Result<ChecksReport, EngineError>` (free wrapper `final_checks`): re-runs the mapped checks, including the cargo ones, on the final head over the whole job diff and rewrites `checks.md`.

- [ ] **Step 1: Re-enable the modules**

In `lib.rs` restore `mod decide;`, `mod review;`, `pub use decide::{decide, risk_summary, DecideInput, Decision};` and `pub use review::{apply_decision, final_checks, review_stage, ReviewOutcome};`. Create the two files with the code from Steps 3 and 5 after the tests exist.

- [ ] **Step 2: Write the failing decision tests**

`tests/review_decide.rs` (first part, pure):

```rust
mod common;

use common::{commit_file, scripted, Effect, Fixture, ScriptedRunner};
use factory_core::{BudgetState, DiffClass, Stage, Verdict};
use factory_engine::{decide, DecideInput, Decision, StageOutcome};

fn input(verdict: &Verdict) -> DecideInput<'_> {
    DecideInput {
        checks_ok: true,
        verdict,
        class: DiffClass::Normal,
        risky_summary: String::new(),
        diff_lines: 100,
        diff_cap: 4000,
        budget: BudgetState::Ok,
        round: 1,
        fix_rounds: 2,
    }
}

#[test]
fn clean_normal_diff_auto_merges() {
    assert_eq!(decide(&input(&Verdict::Approve)), Decision::AutoMerge);
}

#[test]
fn doc_only_diff_auto_merges() {
    let v = Verdict::Approve;
    let mut i = input(&v);
    i.class = DiffClass::DocOnly;
    assert_eq!(decide(&i), Decision::AutoMerge);
}

#[test]
fn risky_diff_is_always_needs_human_even_when_approved() {
    let v = Verdict::Approve;
    let mut i = input(&v);
    i.class = DiffClass::Risky;
    i.risky_summary = "risky paths (1): src-tauri/src/lib.rs".into();
    match decide(&i) {
        Decision::NeedsHuman(r) => assert!(r.contains("src-tauri/src/lib.rs"), "{r}"),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn stopped_budget_needs_a_human() {
    let v = Verdict::Approve;
    let mut i = input(&v);
    i.budget = BudgetState::Stop;
    assert!(matches!(decide(&i), Decision::NeedsHuman(r) if r.contains("budget")));
}

#[test]
fn warn_budget_still_merges() {
    let v = Verdict::Approve;
    let mut i = input(&v);
    i.budget = BudgetState::Warn;
    assert_eq!(decide(&i), Decision::AutoMerge);
}

#[test]
fn diff_over_cap_needs_a_human() {
    let v = Verdict::Approve;
    let mut i = input(&v);
    i.diff_lines = 4001;
    assert!(matches!(decide(&i), Decision::NeedsHuman(r) if r.contains("cap")));
}

#[test]
fn failing_checks_give_a_fix_round_then_a_human() {
    let v = Verdict::Approve;
    let mut i = input(&v);
    i.checks_ok = false;
    assert_eq!(decide(&i), Decision::FixRound);
    i.round = 3;
    assert!(matches!(decide(&i), Decision::NeedsHuman(r) if r.contains("fix rounds")));
}

#[test]
fn changes_and_missing_verdicts_give_a_fix_round_then_a_human() {
    for v in [Verdict::Changes, Verdict::Missing] {
        let mut i = input(&v);
        assert_eq!(decide(&i), Decision::FixRound);
        i.round = 3;
        assert!(matches!(decide(&i), Decision::NeedsHuman(_)));
    }
}
```

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test review_decide`
Expected: compile error (`decide` not found).

- [ ] **Step 3: Implement `decide.rs`**

```rust
use factory_core::{classify_paths, BudgetState, Config, DiffClass, Verdict};

/// What to do with a job after the reviewer ran.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    AutoMerge,
    FixRound,
    NeedsHuman(String),
}

pub struct DecideInput<'a> {
    pub checks_ok: bool,
    pub verdict: &'a Verdict,
    pub class: DiffClass,
    pub risky_summary: String,
    pub diff_lines: usize,
    pub diff_cap: usize,
    pub budget: BudgetState,
    pub round: u32,
    pub fix_rounds: u32,
}

/// Implements spec decisions 6 and 7. The order of the checks matters.
pub fn decide(i: &DecideInput) -> Decision {
    // Decision 6: a risky diff always stops for the human.
    if i.class == DiffClass::Risky {
        return Decision::NeedsHuman(format!("risky diff: {}", i.risky_summary));
    }
    if i.budget == BudgetState::Stop {
        return Decision::NeedsHuman("budget is stopped".to_string());
    }
    if i.diff_lines > i.diff_cap {
        return Decision::NeedsHuman(format!("diff has {} changed lines, the cap is {}", i.diff_lines, i.diff_cap));
    }
    let used = i.round.saturating_sub(1);
    let rounds_left = used < i.fix_rounds;
    if !i.checks_ok {
        return if rounds_left {
            Decision::FixRound
        } else {
            Decision::NeedsHuman(format!("checks failed after {used} fix rounds"))
        };
    }
    match i.verdict {
        Verdict::Approve => Decision::AutoMerge,
        // A missing verdict is never a pass.
        Verdict::Changes | Verdict::Missing => {
            if rounds_left {
                Decision::FixRound
            } else {
                Decision::NeedsHuman(format!("changes remain after {used} fix rounds"))
            }
        }
    }
}

/// Names the risky paths for the human, at most five, with the count of the rest.
pub fn risk_summary(paths: &[String], cfg: &Config) -> String {
    let risky: Vec<&String> = paths
        .iter()
        .filter(|p| classify_paths(std::slice::from_ref(*p), cfg) == DiffClass::Risky)
        .collect();
    let shown: Vec<&str> = risky.iter().take(5).map(|s| s.as_str()).collect();
    let more = risky.len().saturating_sub(shown.len());
    let tail = if more > 0 { format!(" (+{more} more)") } else { String::new() };
    format!("risky paths ({}): {}{}", risky.len(), shown.join(", "), tail)
}
```

Run the pure tests: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test review_decide decide`
Expected: the 8 decision tests above pass (the later tests are added in Step 4, so run by name filter).

- [ ] **Step 4: Write the failing review stage tests**

Append to `tests/review_decide.rs`:

```rust
fn build_and_pass_checks(fx: &Fixture, files: &[&'static str]) -> ScriptedRunner {
    let effects: Vec<Effect> = vec![Box::new({
        let files: Vec<&'static str> = files.to_vec();
        move |spec| {
            let mut sha = String::new();
            for f in &files {
                if let Some(parent) = std::path::Path::new(f).parent() {
                    std::fs::create_dir_all(spec.cwd.join(parent)).expect("mkdir");
                }
                sha = commit_file(&spec.cwd, f, "x\n");
            }
            format!("HEAD: {sha}")
        }
    })];
    let runner = scripted(effects);
    fx.engine(&runner).build_stage().expect("build");
    runner
}

fn reviewer(reply: &'static str) -> ScriptedRunner {
    scripted(vec![Box::new(move |_| reply.to_string())])
}

#[test]
fn reviewer_spec_is_read_only_and_tells_it_to_page_the_diff() {
    let fx = Fixture::new();
    build_and_pass_checks(&fx, &["a.txt"]);
    let rr = reviewer("ok\nVERDICT: APPROVE");
    let e = fx.engine(&rr);
    let job_dir = fx.job_dir.clone();
    e.review_stage().expect("review");
    let spec = rr.last_spec().expect("a spec was recorded");
    assert_eq!(spec.role, "reviewer");
    assert_eq!(spec.model, Some(fx.config.reviewer_model.clone()));
    assert_eq!(spec.allowed_tools, vec!["Read".to_string(), "Grep".to_string(), "Glob".to_string()]);
    assert_eq!(spec.budget_weighted, Some(fx.config.budget_weighted));
    assert!(spec.prompt.contains("offset"));
    assert!(spec.prompt.contains("1500"));
    assert!(spec.prompt.contains("60 lines"));
    assert!(job_dir.join("review-code-r1.md").exists());
}

#[test]
fn approve_on_a_normal_diff_is_auto_merge_and_apply_sets_approve() {
    let fx = Fixture::new();
    build_and_pass_checks(&fx, &["a.txt"]);
    let rr = reviewer("fine\nVERDICT: APPROVE");
    let e = fx.engine(&rr);
    let out = e.review_stage().expect("review");
    assert_eq!(out.decision, Decision::AutoMerge);
    assert_eq!(e.apply_decision(&out).expect("apply"), StageOutcome::Advanced(Stage::Approve));
    assert_eq!(fx.job().stage, Stage::Approve);
    let md = std::fs::read_to_string(fx.job_dir.join("checks.md")).expect("checks.md");
    assert!(md.contains("PASS"), "{md}");
}

#[test]
fn verdict_not_on_the_last_line_counts_as_changes() {
    for reply in ["VERDICT: APPROVE\nbut also this", "```\nVERDICT: APPROVE\n```", "verdict: approve", "looks good"] {
        let fx = Fixture::new();
        build_and_pass_checks(&fx, &["a.txt"]);
        let rr = reviewer(reply);
        let out = fx.engine(&rr).review_stage().expect("review");
        assert_eq!(out.verdict, Verdict::Missing, "{reply}");
        assert_eq!(out.decision, Decision::FixRound, "{reply}");
    }
}

#[test]
fn changes_verdict_starts_a_fix_round_with_the_review_text() {
    let fx = Fixture::new();
    build_and_pass_checks(&fx, &["a.txt"]);
    let rr = reviewer("1. [blocker] src/a.rs:3 bad\nVERDICT: CHANGES");
    let e = fx.engine(&rr);
    let out = e.review_stage().expect("review");
    assert_eq!(out.decision, Decision::FixRound);
    assert_eq!(e.apply_decision(&out).expect("apply"), StageOutcome::Advanced(Stage::Build));
    let job = fx.job();
    assert_eq!(job.round, 2);
    let findings = std::fs::read_to_string(fx.job_dir.join("changes-r2.md")).expect("findings");
    assert!(findings.contains("[blocker] src/a.rs:3"), "{findings}");
}

#[test]
fn risky_diff_with_approve_needs_a_human_naming_the_path() {
    let fx = Fixture::new();
    build_and_pass_checks(&fx, &["src-tauri/src/lib.rs"]);
    let rr = reviewer("fine\nVERDICT: APPROVE");
    let e = fx.engine(&rr);
    let out = e.review_stage().expect("review");
    match &out.decision {
        Decision::NeedsHuman(r) => assert!(r.contains("src-tauri/src/lib.rs"), "{r}"),
        other => panic!("unexpected {other:?}"),
    }
    assert!(matches!(e.apply_decision(&out).expect("apply"), StageOutcome::NeedsHuman(_)));
    assert_eq!(fx.job().stage, Stage::NeedsHuman);
}

#[test]
fn final_check_failure_blocks_auto_merge() {
    let mut fx = Fixture::new();
    build_and_pass_checks(&fx, &["a.txt"]);
    let rr = reviewer("fine\nVERDICT: APPROVE");
    // The checks pass during review, then the mapped command starts failing.
    let e = fx.engine(&rr);
    let out = e.review_stage().expect("review");
    assert_eq!(out.decision, Decision::AutoMerge);
    fx.config.checks = vec![factory_core::CheckRule { paths: vec!["**".into()], commands: vec!["false".into()] }];
    let e2 = fx.engine(&rr);
    assert_eq!(e2.apply_decision(&out).expect("apply"), StageOutcome::Advanced(Stage::Build));
    assert_eq!(fx.job().round, 2);
}
```

`ScriptedRunner::last_spec()` (Task 2, `tests/common/mod.rs`) returns a clone of the last `AgentSpec` it ran. Plan 03's `FakeRunner::calls()` is the equivalent for a runner that needs no side effects.

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test review_decide`
Expected: compile error (`review_stage` not found).

- [ ] **Step 5: Implement the reviewer spec and `review.rs`**

Append to `src/specs.rs`:

```rust
impl Engine<'_> {
    /// The combined reviewer. It is read-only and pages the diff to stay cheap.
    pub(crate) fn reviewer_spec(&self, cwd: &Path, diff: &Path, names: &Path, round: u32) -> AgentSpec {
        let prompt = format!(
            "You are the code reviewer in Rocket's software factory. You are read-only.\n\
             Inputs: the diff at {diff}, the list of changed files at {names}, the plan at {plan}, and the worktree at {cwd}. \
             Round {round}.\n\
             Read the diff in pages: use the Read tool with offset and limit 1500 lines per call. Do not read the whole file at once. \
             Read .claude/rules/rust-ddd-boundaries.md, .claude/rules/tauri-ipc-boundaries.md and .claude/rules/frontend-component-guardrails.md only for the areas the diff touches.\n\
             Check: the diff does what the plan asks and nothing else; correctness and edge cases; no unwrap in production Rust; \
             DDD placement; camelCase serde renames on IPC DTOs only; tests that would fail without the change; \
             hard rules (shadcn/ui, lucide-react, SingleLineEditor, Monaco, narrow Zustand selectors, no git CLI in Rust); no debug leftovers.\n\
             Reply in at most 60 lines. List findings as a numbered list with file:line, severity [blocker] or [minor], and a concrete fix. \
             Any blocker means CHANGES. Your last line, alone, must be exactly `VERDICT: APPROVE` or `VERDICT: CHANGES`.",
            diff = diff.display(),
            names = names.display(),
            plan = self.plan_file.display(),
            cwd = cwd.display(),
        );
        let mut spec = AgentSpec::new(&self.job_id, "reviewer", cwd, &prompt);
        spec.allowed_tools = vec!["Read".to_string(), "Grep".to_string(), "Glob".to_string()];
        self.with_limits(spec, &self.config.reviewer_model)
    }
}
```

`src/review.rs`:

```rust
use std::path::PathBuf;

use factory_checks::ChecksReport;
use factory_core::{budget_state, classify_paths, parse_verdict, Stage, Verdict};

use crate::decide::{decide, risk_summary, DecideInput, Decision};
use crate::ports::{git_err, io_err};
use crate::{Engine, EngineError, StageOutcome};

pub struct ReviewOutcome {
    pub verdict: Verdict,
    pub review_path: PathBuf,
    pub decision: Decision,
}

impl Engine<'_> {
    /// Writes the diff files, runs the reviewer once, saves its reply and decides.
    pub fn review_stage(&self) -> Result<ReviewOutcome, EngineError> {
        let job = self.job()?;
        let base = job.base.clone().ok_or_else(|| EngineError::Msg("job has no base".into()))?;
        let head = job.head.clone().ok_or_else(|| EngineError::Msg("job has no head".into()))?;
        // The reviewer sees only the new work of this round. Risk and size judge the whole job diff.
        let round_base = job.prev_head.clone().unwrap_or_else(|| base.clone());
        // The file names carry both short shas, so the round diff and the whole diff never overwrite each other.
        let round_diff = self.repo.diff_to_files(&round_base, &head, &self.job_dir).map_err(git_err)?;
        let whole = self.repo.diff_to_files(&base, &head, &self.job_dir).map_err(git_err)?;
        let paths = self.repo.changed_paths(&base, &head).map_err(git_err)?;

        let cwd = self.worktree()?;
        let spec = self.reviewer_spec(&cwd, &round_diff.diff, &round_diff.names, job.round);
        let run = self.run_agent(spec)?;
        let review_path = self.job_dir.join(format!("review-code-r{}.md", job.round));
        std::fs::write(&review_path, &run.result_text).map_err(io_err)?;

        let verdict = if run.stop_reason == factory_agent::StopReason::Done {
            parse_verdict(&run.result_text)
        } else {
            Verdict::Missing
        };
        self.ruling(&format!("review round {} verdict {:?}", job.round, verdict))?;

        let checks_ok = self.latest_checks_ok()?;
        let class = classify_paths(&paths, &self.config);
        let input = DecideInput {
            checks_ok,
            verdict: &verdict,
            class,
            risky_summary: risk_summary(&paths, &self.config),
            diff_lines: whole.changed_lines(),
            diff_cap: self.config.diff_cap_lines as usize,
            budget: budget_state(self.job_weighted_tokens(), self.config.budget_weighted),
            round: job.round,
            fix_rounds: self.config.fix_rounds,
        };
        let decision = decide(&input);
        Ok(ReviewOutcome { verdict, review_path, decision })
    }

    /// True when `checks.md` exists and has no FAIL line.
    fn latest_checks_ok(&self) -> Result<bool, EngineError> {
        let text = std::fs::read_to_string(self.job_dir.join("checks.md")).map_err(io_err)?;
        Ok(text.lines().any(|l| l.starts_with("PASS ")) && !text.lines().any(|l| l.starts_with("FAIL ")))
    }

    /// Re-runs the mapped checks over the whole job diff on the final head.
    pub fn final_checks(&self) -> Result<ChecksReport, EngineError> {
        self.run_checks_now()
    }

    /// Applies a decision to the board.
    pub fn apply_decision(&self, outcome: &ReviewOutcome) -> Result<StageOutcome, EngineError> {
        match &outcome.decision {
            Decision::NeedsHuman(reason) => self.park(reason.clone()),
            Decision::FixRound => {
                let text = std::fs::read_to_string(&outcome.review_path).map_err(io_err)?;
                let findings = format!("The reviewer asked for changes. Fix these findings only.\n\n{text}");
                self.fix_or_park(&findings, "review asked for changes")
            }
            Decision::AutoMerge => {
                let report = self.final_checks()?;
                if !report.all_passed() {
                    let findings = format!("The final checks failed. Fix these failures only.\n\n{}", report.failure_text());
                    return self.fix_or_park(&findings, "final checks failed");
                }
                self.set_stage(Stage::Approve, None, false)?;
                self.ruling("auto-merge approved, checks green, reviewer approved")?;
                Ok(StageOutcome::Advanced(Stage::Approve))
            }
        }
    }
}

pub fn review_stage(e: &Engine) -> Result<ReviewOutcome, EngineError> {
    e.review_stage()
}

pub fn apply_decision(e: &Engine, outcome: &ReviewOutcome) -> Result<StageOutcome, EngineError> {
    e.apply_decision(outcome)
}

pub fn final_checks(e: &Engine) -> Result<ChecksReport, EngineError> {
    e.final_checks()
}
```

`job_weighted_tokens` already exists in `ports.rs` (Task 2 Step 2), backed by the `Job.tokens` field of Task 2 Step 0b, so the budget is read from the board and never hard-coded.

- [ ] **Step 6: Run the engine tests**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine`
Expected: PASS (decision tests, review tests and the Task 2 tests). Then `cargo check -j4 --manifest-path tools/factory/Cargo.toml`. Never `--workspace`.

- [ ] **Step 7: Commit**

Invoke skill `dev-workflow-skills:1-git-commit`, inline git:

```bash
git add tools/factory/crates/factory-engine tools/factory/Cargo.lock
git commit --only -m "feat(factory): add review stage and decision rules" -- tools/factory/crates/factory-engine tools/factory/Cargo.lock
```

---

## Self-Review Notes

- Spec plan 04 row: `factory_checks::{CheckPlan, ChecksReport, plan_checks, run_checks, write_checks_md}` (Task 1) and `factory_engine::{Engine, build_stage, checks_stage, review_stage, Decision, decide}` (Tasks 2 and 3). Decisions 4, 5, 6, 7 are covered by the tests named in Review Focus. Merge, sync, series, lock, cancel and the CLI are plan 05.
- The board contract (stage `review` after build, `build` after a failed check or a CHANGES round, `approve` after an auto-merge decision, `needs-human` with a reason) is what plan 05's `run_job` loops on.

Next plan: docs/superpowers/plans/2026-10-13-software-factory-v3/plan-05-merge-series-cli.md
