# Factory Ticket Hygiene, Plan 2: The Tracker Port and the Tickets Step

> **For agentic workers:** This plan is executed by a `factory-builder` subagent through `superpowers:executing-plans`, skipping that skill's final whole-branch review, ledger scripts and progress file. Steps use checkbox (`- [ ]`) syntax. The driver commits, so there are no commit steps.

**Goal:** Add the `Tracker` port with a `gh` implementation, the `tickets_step` that files follow-up tickets and closes the parent tickets, and the optional merge-time hook behind `tickets_on_merge`.

**Architecture:** A new `tracker.rs` holds the `Tracker` trait and `GhTracker`, which runs `gh` with an argument list and a timeout. `Driver` owns a `Box<dyn Tracker>` (default `GhTracker` in the main checkout, replaceable with `with_tracker`). A new `tickets.rs` holds `tickets_step`, which reads the refs from the feature text and the follow-ups from `spec.md`, and records every call in `tickets.json` in the job folder so a re-run never repeats a call. `merge_stage` runs the step best effort when the config key is on.

**Tech Stack:** Rust (`tools/factory` workspace), `serde_json`, `gh` CLI. All cargo commands run from `tools/factory/`.

**Spec:** `docs/superpowers/specs/2026-10-17-factory-ticket-hygiene-design.md`

## Global Constraints

- Run only `cargo check -j4 -p <crate>` and `cargo test -j4 -p <crate> <filter>` from `tools/factory/`. Never a bare `cargo check -j4`, never `--workspace`, never `--all`.
- No unwrap calls in production code (tests may use `expect`). Comments are short full sentences ending with a period. No emojis.
- Tests never call the real `gh`. Every test that reaches the tickets step gives the `Driver` a `FakeTracker`.
- `gh` is run with an argument list and never through a shell. Titles and bodies are single arguments.
- A `gh` failure never changes the merge result and never changes the job stage.
- No board field is added. Board JSON, the ledger format and `/api/state` do not change. The only new file is `tickets.json` in the job folder.
- Ruling: the spec lists the port in `ports.rs`, but `ports.rs` in this repository holds `Engine` helpers and not traits, so the trait lives in the new `tracker.rs`. The trait also has `repo_name`, which the spec's foreign-repository check needs.

## Review Focus

- A failure at each of the three stages (create, comment, close) followed by a re-run creates no duplicate ticket, comment or close. Pinned by `rerun_after_a_failed_create_files_once`, `rerun_after_a_failed_comment_does_not_create_again` and `rerun_after_a_failed_close_does_not_comment_again`.
- A parent that is already closed is not closed again and the step still succeeds. Pinned by `an_already_closed_parent_is_not_closed_again`.
- A job with no `spec.md` still comments and closes, and files nothing. Pinned by `no_spec_file_means_no_tickets_but_the_parent_closes`.
- `--dry-run` calls only read methods. Pinned by `dry_run_makes_only_read_calls`.
- A title with quotes, a `$(...)` and spaces reaches `gh` as one argument. Pinned by `create_args_keep_title_and_body_as_single_arguments` in `tracker.rs`.

---

### Task 1: The Tracker trait, GhTracker and the Driver field

**Files:**
- Create: `tools/factory/crates/factory-engine/src/tracker.rs`
- Modify: `tools/factory/crates/factory-engine/src/lib.rs` (`mod tracker;` and the re-export)
- Modify: `tools/factory/crates/factory-engine/src/compat.rs` (`Driver` field, `new`, `with_tracker`, `tracker`)

**Interfaces:**
- Produces: `pub trait Tracker` with `repo_name(&self) -> Result<String, String>`, `labels_of(&self, issue: u32) -> Result<Vec<String>, String>`, `is_closed(&self, issue: u32) -> Result<bool, String>`, `create(&self, title: &str, body: &str, labels: &[String]) -> Result<u32, String>`, `comment(&self, issue: u32, body: &str) -> Result<(), String>`, `close(&self, issue: u32) -> Result<(), String>`; `impl<T: Tracker + ?Sized> Tracker for std::sync::Arc<T>`; `pub struct GhTracker` with `GhTracker::new(dir: &Path) -> GhTracker`; `Driver::with_tracker(self, Box<dyn Tracker>) -> Driver` and `Driver::tracker(&self) -> &dyn Tracker`.

- [ ] **Step 1: Write the failing tests.** Create `tracker.rs` with only this test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_args_keep_title_and_body_as_single_arguments() {
        let title = "Fix \"quotes\" and $(rm -rf x) here";
        let body = "line one\nline two; echo hi";
        let args = create_args(title, body, &["dedup".to_string(), "size-S".to_string()]);
        assert_eq!(
            args,
            vec!["issue", "create", "--title", title, "--body", body, "--label", "dedup", "--label", "size-S"]
        );
    }

    #[test]
    fn created_number_is_read_from_the_url() {
        assert_eq!(parse_created("https://github.com/o/r/issues/109\n"), Some(109));
        assert_eq!(parse_created("nothing useful"), None);
        assert_eq!(parse_created(""), None);
    }

    #[test]
    fn view_json_gives_labels_and_state() {
        let json = r#"{"labels":[{"name":"dedup"},{"name":"tracking"}],"state":"OPEN"}"#;
        assert_eq!(parse_view(json), Ok((vec!["dedup".to_string(), "tracking".to_string()], false)));
        let closed = r#"{"labels":[],"state":"CLOSED"}"#;
        assert_eq!(parse_view(closed), Ok((vec![], true)));
        assert!(parse_view("not json").is_err());
    }

    #[test]
    fn comment_and_close_args() {
        assert_eq!(comment_args(76, "hi"), vec!["issue", "comment", "76", "--body", "hi"]);
        assert_eq!(close_args(76), vec!["issue", "close", "76", "--reason", "completed"]);
        assert_eq!(view_args(76), vec!["issue", "view", "76", "--json", "labels,state"]);
    }

    #[test]
    fn a_missing_gh_binary_is_an_error_not_a_panic() {
        let tracker = GhTracker::with_program("definitely-not-a-real-program-xyz", std::path::Path::new("."));
        assert!(tracker.repo_name().is_err());
    }
}
```

- [ ] **Step 2: Run to see them fail.** Add `mod tracker;` to `lib.rs`, then `cargo test -j4 -p factory-engine tracker`. Expected: compile errors, the items are not defined.

- [ ] **Step 3: Implement.** Put this above the test module in `tracker.rs`:

```rust
//! The tracker port: how the driver reaches GitHub issues. The real one runs `gh`, tests use a fake.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long one `gh` call may run.
const GH_TIMEOUT: Duration = Duration::from_secs(60);

/// The issue operations the tickets step needs. Every failure is a short message.
pub trait Tracker {
    /// The `owner/repo` of the repository the main checkout points at.
    fn repo_name(&self) -> Result<String, String>;
    fn labels_of(&self, issue: u32) -> Result<Vec<String>, String>;
    fn is_closed(&self, issue: u32) -> Result<bool, String>;
    /// Creates an issue and returns its number.
    fn create(&self, title: &str, body: &str, labels: &[String]) -> Result<u32, String>;
    fn comment(&self, issue: u32, body: &str) -> Result<(), String>;
    fn close(&self, issue: u32) -> Result<(), String>;
}

impl<T: Tracker + ?Sized> Tracker for Arc<T> {
    fn repo_name(&self) -> Result<String, String> {
        (**self).repo_name()
    }
    fn labels_of(&self, issue: u32) -> Result<Vec<String>, String> {
        (**self).labels_of(issue)
    }
    fn is_closed(&self, issue: u32) -> Result<bool, String> {
        (**self).is_closed(issue)
    }
    fn create(&self, title: &str, body: &str, labels: &[String]) -> Result<u32, String> {
        (**self).create(title, body, labels)
    }
    fn comment(&self, issue: u32, body: &str) -> Result<(), String> {
        (**self).comment(issue, body)
    }
    fn close(&self, issue: u32) -> Result<(), String> {
        (**self).close(issue)
    }
}

pub(crate) fn view_args(issue: u32) -> Vec<String> {
    ["issue", "view", &issue.to_string(), "--json", "labels,state"].iter().map(|s| s.to_string()).collect()
}

pub(crate) fn create_args(title: &str, body: &str, labels: &[String]) -> Vec<String> {
    let mut args: Vec<String> = ["issue", "create", "--title", title, "--body", body].iter().map(|s| s.to_string()).collect();
    for label in labels {
        args.push("--label".to_string());
        args.push(label.clone());
    }
    args
}

pub(crate) fn comment_args(issue: u32, body: &str) -> Vec<String> {
    ["issue", "comment", &issue.to_string(), "--body", body].iter().map(|s| s.to_string()).collect()
}

pub(crate) fn close_args(issue: u32) -> Vec<String> {
    ["issue", "close", &issue.to_string(), "--reason", "completed"].iter().map(|s| s.to_string()).collect()
}

/// The issue number at the end of the URL that `gh issue create` prints.
pub(crate) fn parse_created(output: &str) -> Option<u32> {
    output.trim().rsplit('/').next()?.parse().ok()
}

/// The label names and whether the issue is closed, from `gh issue view --json labels,state`.
pub(crate) fn parse_view(json: &str) -> Result<(Vec<String>, bool), String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("gh printed invalid JSON: {e}"))?;
    let labels = value["labels"]
        .as_array()
        .map(|list| list.iter().filter_map(|l| l["name"].as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let closed = value["state"].as_str().is_some_and(|s| s.eq_ignore_ascii_case("closed"));
    Ok((labels, closed))
}

/// Runs `gh` in the main checkout. It never uses a shell.
pub struct GhTracker {
    program: String,
    dir: PathBuf,
}

impl GhTracker {
    pub fn new(dir: &Path) -> GhTracker {
        GhTracker::with_program("gh", dir)
    }

    pub(crate) fn with_program(program: &str, dir: &Path) -> GhTracker {
        GhTracker { program: program.to_string(), dir: dir.to_path_buf() }
    }

    /// Runs one call and returns its trimmed standard output. A slow call is killed after the timeout.
    fn run(&self, args: &[String]) -> Result<String, String> {
        let name = args.iter().take(2).cloned().collect::<Vec<_>>().join(" ");
        let mut child = Command::new(&self.program)
            .args(args)
            .current_dir(&self.dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("{} could not start: {e}", self.program))?;
        let start = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if start.elapsed() > GH_TIMEOUT => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("{} {name} timed out", self.program));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(e) => return Err(format!("{} {name} failed: {e}", self.program)),
            }
        }
        let out = child.wait_with_output().map_err(|e| format!("{} {name} failed: {e}", self.program))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            let first = err.lines().next().unwrap_or("no message");
            return Err(format!("{} {name} failed: {first}", self.program));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
}

impl Tracker for GhTracker {
    fn repo_name(&self) -> Result<String, String> {
        let args: Vec<String> =
            ["repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"].iter().map(|s| s.to_string()).collect();
        let name = self.run(&args)?;
        if name.is_empty() {
            return Err("gh repo view printed no repository name".to_string());
        }
        Ok(name)
    }

    fn labels_of(&self, issue: u32) -> Result<Vec<String>, String> {
        Ok(parse_view(&self.run(&view_args(issue))?)?.0)
    }

    fn is_closed(&self, issue: u32) -> Result<bool, String> {
        Ok(parse_view(&self.run(&view_args(issue))?)?.1)
    }

    fn create(&self, title: &str, body: &str, labels: &[String]) -> Result<u32, String> {
        let out = self.run(&create_args(title, body, labels))?;
        parse_created(&out).ok_or_else(|| format!("gh issue create printed no issue URL: {out}"))
    }

    fn comment(&self, issue: u32, body: &str) -> Result<(), String> {
        self.run(&comment_args(issue, body)).map(|_| ())
    }

    fn close(&self, issue: u32) -> Result<(), String> {
        self.run(&close_args(issue)).map(|_| ())
    }
}
```

In `lib.rs` add `pub use tracker::{GhTracker, Tracker};`.

In `compat.rs`: add `tracker: Box<dyn crate::Tracker>,` to `Driver`; in `Driver::new` add `tracker: Box::new(crate::GhTracker::new(root)),`; add these two methods after `with_lease_owner`:

```rust
    /// Replaces the issue tracker. Tests pass a fake.
    pub fn with_tracker(mut self, tracker: Box<dyn crate::Tracker>) -> Driver {
        self.tracker = tracker;
        self
    }

    pub fn tracker(&self) -> &dyn crate::Tracker {
        self.tracker.as_ref()
    }
```

- [ ] **Step 4: Run the tests.** `cargo test -j4 -p factory-engine tracker` and `cargo check -j4 -p factory-engine`. Expected: pass.

### Task 2: tickets_step and the tickets.json marker

**Files:**
- Create: `tools/factory/crates/factory-engine/src/tickets.rs`
- Modify: `tools/factory/crates/factory-engine/src/lib.rs` (`mod tickets;` and `pub use tickets::{tickets_step, TicketsReport, TICKETS_FILE};`)
- Modify: `tools/factory/crates/factory-engine/tests/common/mod.rs` (add `FakeTracker` and `with_tracker`)
- Create: `tools/factory/crates/factory-engine/tests/tickets.rs`

**Interfaces:**
- Consumes: `Tracker`, `Driver::tracker`, `crate::follow_ups`, `factory_core::{parse_issue_refs, write_atomic}`.
- Produces: `pub const TICKETS_FILE: &str = "tickets.json"`; `pub struct TicketsReport { pub lines: Vec<String>, pub failed: bool, pub skipped: bool }` (derives `Debug, Clone, PartialEq, Eq, Default`); `pub fn tickets_step(driver: &Driver, id: &str, dry_run: bool) -> Result<TicketsReport, EngineError>`.

- [ ] **Step 1: Add the fake tracker to the test helpers.** Append to `tests/common/mod.rs`:

```rust
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct FakeState {
    pub repo: String,
    pub labels: HashMap<u32, Vec<String>>,
    pub closed: HashSet<u32>,
    pub next: u32,
    pub calls: Vec<String>,
    pub created: Vec<(String, String, Vec<String>)>,
    pub comments: Vec<(u32, String)>,
    pub closes: Vec<u32>,
    pub fail: HashSet<String>,
}

/// A tracker that records its calls. Clones share one state, so a test keeps a handle after the driver owns the box.
#[derive(Clone)]
pub struct FakeTracker {
    pub state: Arc<Mutex<FakeState>>,
}

impl FakeTracker {
    pub fn new(repo: &str) -> FakeTracker {
        let state = FakeState { repo: repo.to_string(), next: 200, ..FakeState::default() };
        FakeTracker { state: Arc::new(Mutex::new(state)) }
    }
    pub fn with_labels(self, issue: u32, labels: &[&str]) -> FakeTracker {
        self.state.lock().expect("lock").labels.insert(issue, labels.iter().map(|s| s.to_string()).collect());
        self
    }
    pub fn fail_on(&self, method: &str) {
        self.state.lock().expect("lock").fail.insert(method.to_string());
    }
    pub fn clear_failures(&self) {
        self.state.lock().expect("lock").fail.clear();
    }
    pub fn calls(&self) -> Vec<String> {
        self.state.lock().expect("lock").calls.clone()
    }
    pub fn count(&self, method: &str) -> usize {
        self.calls().iter().filter(|c| c.starts_with(method)).count()
    }
    fn enter(&self, call: String, method: &str) -> Result<std::sync::MutexGuard<'_, FakeState>, String> {
        let mut s = self.state.lock().expect("lock");
        s.calls.push(call);
        if s.fail.contains(method) {
            return Err(format!("fake {method} failure"));
        }
        Ok(s)
    }
}

impl factory_engine::Tracker for FakeTracker {
    fn repo_name(&self) -> Result<String, String> {
        Ok(self.enter("repo_name".into(), "repo_name")?.repo.clone())
    }
    fn labels_of(&self, issue: u32) -> Result<Vec<String>, String> {
        Ok(self.enter(format!("labels_of {issue}"), "labels_of")?.labels.get(&issue).cloned().unwrap_or_default())
    }
    fn is_closed(&self, issue: u32) -> Result<bool, String> {
        Ok(self.enter(format!("is_closed {issue}"), "is_closed")?.closed.contains(&issue))
    }
    fn create(&self, title: &str, body: &str, labels: &[String]) -> Result<u32, String> {
        let mut s = self.enter(format!("create {title}"), "create")?;
        s.next += 1;
        let number = s.next;
        s.created.push((title.to_string(), body.to_string(), labels.to_vec()));
        Ok(number)
    }
    fn comment(&self, issue: u32, body: &str) -> Result<(), String> {
        let mut s = self.enter(format!("comment {issue}"), "comment")?;
        s.comments.push((issue, body.to_string()));
        Ok(())
    }
    fn close(&self, issue: u32) -> Result<(), String> {
        let mut s = self.enter(format!("close {issue}"), "close")?;
        s.closes.push(issue);
        s.closed.insert(issue);
        Ok(())
    }
}

/// Gives the fixture's driver a tracker, keeping everything else.
pub fn with_tracker(fx: MergeFixture, tracker: FakeTracker) -> MergeFixture {
    let MergeFixture { tmp, main, driver, runner, id, head } = fx;
    MergeFixture { tmp, main, driver: driver.with_tracker(Box::new(tracker)), runner, id, head }
}

/// Sets the feature text of a job on the board, and writes `spec.md` into its folder when given.
pub fn set_feature(fx: &MergeFixture, feature: &str, spec: Option<&str>) {
    let text = feature.to_string();
    fx.driver.store().update_job(&fx.id, move |j| {
        j.feature = text;
        Ok(())
    }).expect("feature");
    if let Some(spec) = spec {
        let job = fx.driver.job(&fx.id).expect("job");
        std::fs::write(fx.driver.job_dir(&job).join("spec.md"), spec).expect("spec");
    }
}
```

`common/mod.rs` already imports `Arc` and `Mutex` from `std::sync`; if `Mutex` is not imported yet, add it to the existing `use std::sync::{...}` line. If `store().update_job` has a different closure return type than `Ok(())` of `Result<(), CoreError>`, follow the signature used in `merge.rs` (`driver.store().update_job(id, |job| ... )`). Also add `#![allow(dead_code)]` is already at the top of the file.

- [ ] **Step 2: Write the failing tests.** Create `tests/tickets.rs`:

```rust
mod common;
use common::{approved_job, set_feature, with_tracker, FakeTracker, MergeFixture};
use factory_engine::{merge_stage, tickets_step, MergeResult};

const SPEC_WITH_TWO: &str = "## Goal\nsomething with enough words to pass the check here\n\n## Follow-ups\n\n- Shared name helpers. More detail here.\n- Merge QueryParam and Header\n\n## Acceptance criteria\n- a\n";
const SPEC_NONE: &str = "## Goal\nsomething with enough words to pass the check here\n\n## Acceptance criteria\n- a\n";

/// A merged job `001` whose feature text cites issue 76, with a fake tracker for `o/r`.
fn merged(spec: Option<&str>, tracker: &FakeTracker) -> MergeFixture {
    let fx = with_tracker(approved_job(), tracker.clone());
    set_feature(&fx, "dedup cleanups https://github.com/o/r/issues/76", spec);
    assert!(matches!(merge_stage(&fx.driver, &fx.id).expect("merge"), MergeResult::Merged(_)));
    fx
}

fn tracker() -> FakeTracker {
    FakeTracker::new("o/r").with_labels(76, &["dedup", "refactor", "tracking", "tier-2"])
}

#[test]
fn files_one_ticket_per_follow_up_comments_once_and_closes() {
    let t = tracker();
    let fx = merged(Some(SPEC_WITH_TWO), &t);
    let report = tickets_step(&fx.driver, &fx.id, false).expect("step");
    assert!(!report.failed && !report.skipped, "{report:?}");
    let s = t.state.lock().expect("lock");
    assert_eq!(s.created.len(), 2);
    assert_eq!(s.created[0].0, "Shared name helpers");
    assert_eq!(s.created[1].0, "Merge QueryParam and Header");
    assert_eq!(s.created[0].2, vec!["dedup".to_string(), "refactor".to_string(), "tier-2".to_string()]);
    assert!(s.created[0].1.contains("#76"));
    assert_eq!(s.comments.len(), 1);
    assert_eq!(s.comments[0].0, 76);
    assert!(s.comments[0].1.contains("#201") && s.comments[0].1.contains("#202"), "{}", s.comments[0].1);
    assert_eq!(s.closes, vec![76]);
    let marker = std::fs::read_to_string(fx.driver.job_dir(&fx.driver.job(&fx.id).expect("job")).join("tickets.json")).expect("marker");
    assert!(marker.contains("Shared name helpers") && marker.contains("201"));
}

#[test]
fn no_spec_file_means_no_tickets_but_the_parent_closes() {
    let t = tracker();
    let fx = merged(None, &t);
    let report = tickets_step(&fx.driver, &fx.id, false).expect("step");
    assert!(!report.failed, "{report:?}");
    let s = t.state.lock().expect("lock");
    assert!(s.created.is_empty());
    assert_eq!(s.comments.len(), 1);
    assert!(s.comments[0].1.contains("No remaining work"), "{}", s.comments[0].1);
    assert_eq!(s.closes, vec![76]);
}

#[test]
fn an_empty_follow_ups_section_only_comments_and_closes() {
    let t = tracker();
    let fx = merged(Some("## Goal\nsomething with enough words to pass the check here\n\n## Follow-ups\n\n## Acceptance criteria\n- a\n"), &t);
    tickets_step(&fx.driver, &fx.id, false).expect("step");
    let s = t.state.lock().expect("lock");
    assert!(s.created.is_empty());
    assert_eq!(s.closes, vec![76]);
}

#[test]
fn an_already_closed_parent_is_not_closed_again() {
    let t = tracker();
    t.state.lock().expect("lock").closed.insert(76);
    let fx = merged(Some(SPEC_NONE), &t);
    let report = tickets_step(&fx.driver, &fx.id, false).expect("step");
    assert!(!report.failed, "{report:?}");
    assert_eq!(t.count("close"), 0);
}

#[test]
fn rerun_after_a_failed_create_files_once() {
    let t = tracker();
    let fx = merged(Some(SPEC_WITH_TWO), &t);
    t.fail_on("create");
    let first = tickets_step(&fx.driver, &fx.id, false).expect("step");
    assert!(first.failed);
    assert_eq!(t.count("comment"), 0, "the parent is not commented when filing failed");
    assert_eq!(t.count("close"), 0);
    t.clear_failures();
    let second = tickets_step(&fx.driver, &fx.id, false).expect("step");
    assert!(!second.failed, "{second:?}");
    let s = t.state.lock().expect("lock");
    assert_eq!(s.created.len(), 2);
    assert_eq!(s.comments.len(), 1);
    assert_eq!(s.closes, vec![76]);
}

#[test]
fn rerun_after_a_failed_comment_does_not_create_again() {
    let t = tracker();
    let fx = merged(Some(SPEC_WITH_TWO), &t);
    t.fail_on("comment");
    assert!(tickets_step(&fx.driver, &fx.id, false).expect("step").failed);
    assert_eq!(t.count("create"), 2);
    assert_eq!(t.count("close"), 0, "the parent is not closed before it is commented");
    t.clear_failures();
    assert!(!tickets_step(&fx.driver, &fx.id, false).expect("step").failed);
    assert_eq!(t.count("create"), 2);
    assert_eq!(t.state.lock().expect("lock").comments.len(), 1);
    assert_eq!(t.state.lock().expect("lock").closes, vec![76]);
}

#[test]
fn rerun_after_a_failed_close_does_not_comment_again() {
    let t = tracker();
    let fx = merged(Some(SPEC_NONE), &t);
    t.fail_on("close");
    assert!(tickets_step(&fx.driver, &fx.id, false).expect("step").failed);
    t.clear_failures();
    assert!(!tickets_step(&fx.driver, &fx.id, false).expect("step").failed);
    let s = t.state.lock().expect("lock");
    assert_eq!(s.comments.len(), 1);
    assert_eq!(s.closes, vec![76]);
}

#[test]
fn dry_run_makes_only_read_calls() {
    let t = tracker();
    let fx = merged(Some(SPEC_WITH_TWO), &t);
    let report = tickets_step(&fx.driver, &fx.id, true).expect("step");
    assert!(!report.failed, "{report:?}");
    for call in t.calls() {
        assert!(call == "repo_name" || call.starts_with("labels_of") || call.starts_with("is_closed"), "write call {call}");
    }
    let text = report.lines.join("\n");
    assert!(text.contains("would create: Shared name helpers"), "{text}");
    assert!(text.contains("would comment on #76"), "{text}");
    assert!(text.contains("would close #76"), "{text}");
    let job = fx.driver.job(&fx.id).expect("job");
    assert!(!fx.driver.job_dir(&job).join("tickets.json").exists());
}

#[test]
fn skips_a_job_that_is_not_merged_without_any_call() {
    let t = tracker();
    let fx = with_tracker(approved_job(), t.clone());
    set_feature(&fx, "work on #76", Some(SPEC_WITH_TWO));
    let report = tickets_step(&fx.driver, &fx.id, false).expect("step");
    assert!(report.skipped && !report.failed);
    assert!(report.lines.join(" ").contains("not merged"), "{report:?}");
    assert!(t.calls().is_empty());
}

#[test]
fn skips_a_job_that_is_not_the_last_plan() {
    let t = tracker();
    let fx = merged(Some(SPEC_WITH_TWO), &t);
    fx.driver
        .store()
        .update_job(&fx.id, |j| {
            j.series = Some(factory_core::SeriesPos { plan_file: "p".into(), index: 1, total: 2, id: None });
            Ok(())
        })
        .expect("series");
    let report = tickets_step(&fx.driver, &fx.id, false).expect("step");
    assert!(report.skipped, "{report:?}");
    assert!(report.lines.join(" ").contains("last plan"), "{report:?}");
    assert_eq!(t.count("create"), 0);
}

#[test]
fn skips_a_feature_without_issue_refs() {
    let t = tracker();
    let fx = merged(Some(SPEC_WITH_TWO), &t);
    set_feature(&fx, "no tickets here", None);
    let report = tickets_step(&fx.driver, &fx.id, false).expect("step");
    assert!(report.skipped, "{report:?}");
    assert!(report.lines.join(" ").contains("no issue"), "{report:?}");
}

#[test]
fn skips_a_url_for_another_repository() {
    let t = tracker();
    let fx = merged(Some(SPEC_WITH_TWO), &t);
    set_feature(&fx, "see https://github.com/other/thing/issues/5", None);
    let report = tickets_step(&fx.driver, &fx.id, false).expect("step");
    assert!(report.skipped, "{report:?}");
    assert!(report.lines.join(" ").contains("other/thing"), "{report:?}");
    assert_eq!(t.count("create"), 0);
}

#[test]
fn several_parents_each_get_one_comment_and_one_close() {
    let t = tracker().with_labels(77, &["bug"]);
    let fx = merged(Some(SPEC_NONE), &t);
    set_feature(&fx, "fix #76 and #77", None);
    tickets_step(&fx.driver, &fx.id, false).expect("step");
    let s = t.state.lock().expect("lock");
    assert_eq!(s.comments.iter().map(|c| c.0).collect::<Vec<_>>(), vec![76, 77]);
    assert_eq!(s.closes, vec![76, 77]);
}

#[test]
fn a_failing_tracker_leaves_the_merged_job_merged() {
    let t = tracker();
    let fx = merged(Some(SPEC_WITH_TWO), &t);
    t.fail_on("repo_name");
    let report = tickets_step(&fx.driver, &fx.id, false).expect("step");
    assert!(report.failed);
    assert_eq!(fx.driver.job(&fx.id).expect("job").stage, factory_core::Stage::Merged);
}
```

- [ ] **Step 3: Run to see them fail.** Add `mod tickets;` to `lib.rs` (create the file empty first), then `cargo test -j4 -p factory-engine --test tickets`. Expected: compile errors, `tickets_step` is not defined.

- [ ] **Step 4: Implement.** Write `tickets.rs`:

```rust
//! The tickets step: once the last plan of a job has merged, file the follow-ups as new tickets,
//! comment on the parent tickets and close them. Every call is recorded in `tickets.json` so a re-run
//! never repeats one.

use std::path::Path;

use factory_core::{parse_issue_refs, write_atomic, Stage};
use serde_json::{json, Value};

use crate::{follow_ups, Driver, EngineError};

/// The marker file in the job folder.
pub const TICKETS_FILE: &str = "tickets.json";

/// What the step did or would do, one line per action.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TicketsReport {
    pub lines: Vec<String>,
    pub failed: bool,
    pub skipped: bool,
}

impl TicketsReport {
    fn skip(mut self, why: String) -> TicketsReport {
        self.skipped = true;
        self.lines.push(format!("skipped: {why}"));
        self
    }

    fn fail(&mut self, what: String) {
        self.failed = true;
        self.lines.push(format!("failed: {what}"));
    }
}

/// The calls that already happened.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Marker {
    created: Vec<(String, u32)>,
    commented: Vec<u32>,
    closed: Vec<u32>,
}

impl Marker {
    fn load(path: &Path) -> Result<Marker, EngineError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Marker::default()),
            Err(e) => return Err(e.into()),
        };
        let v: Value = serde_json::from_str(&text)
            .map_err(|e| EngineError::Msg(format!("{} is not valid JSON, fix or remove it: {e}", path.display())))?;
        let numbers = |key: &str| -> Vec<u32> {
            v[key]
                .as_array()
                .map(|list| list.iter().filter_map(|n| n.as_u64().and_then(|n| u32::try_from(n).ok())).collect())
                .unwrap_or_default()
        };
        let created = v["created"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|c| Some((c["title"].as_str()?.to_string(), u32::try_from(c["number"].as_u64()?).ok()?)))
                    .collect()
            })
            .unwrap_or_default();
        Ok(Marker { created, commented: numbers("commented"), closed: numbers("closed") })
    }

    fn save(&self, path: &Path) -> Result<(), EngineError> {
        let created: Vec<Value> = self.created.iter().map(|(t, n)| json!({ "title": t, "number": n })).collect();
        let v = json!({ "created": created, "commented": self.commented, "closed": self.closed });
        let text = serde_json::to_string_pretty(&v).map_err(|e| EngineError::Msg(e.to_string()))?;
        write_atomic(path, text.as_bytes())?;
        Ok(())
    }
}

fn list(numbers: &[u32]) -> String {
    numbers.iter().map(|n| format!("#{n}")).collect::<Vec<_>>().join(", ")
}

/// Files the follow-ups of a merged job, comments on its parent tickets and closes them.
/// With `dry_run` it makes only read calls and writes nothing. A `gh` failure is reported, never raised.
pub fn tickets_step(driver: &Driver, id: &str, dry_run: bool) -> Result<TicketsReport, EngineError> {
    let job = driver.job(id)?;
    let mut report = TicketsReport::default();
    if job.stage != Stage::Merged {
        return Ok(report.skip(format!("job {id} is not merged")));
    }
    if let Some(pos) = &job.series {
        if pos.index < pos.total {
            return Ok(report.skip(format!("plan {} of {} is not the last plan of the series", pos.index, pos.total)));
        }
    }
    let refs = parse_issue_refs(&job.feature);
    if refs.is_empty() {
        return Ok(report.skip("the feature text has no issue references".to_string()));
    }
    let tracker = driver.tracker();
    let repo = match tracker.repo_name() {
        Ok(name) => name,
        Err(e) => {
            report.fail(format!("repository name: {e}"));
            return Ok(finish(driver, &job, dry_run, report));
        }
    };
    for r in &refs {
        if let Some(other) = &r.repo {
            if !other.eq_ignore_ascii_case(&repo) {
                return Ok(report.skip(format!("issue #{} belongs to {other}, this repository is {repo}", r.number)));
            }
        }
    }
    let mut parents: Vec<u32> = refs.iter().map(|r| r.number).collect();
    parents.sort_unstable();
    parents.dedup();

    let dir = driver.job_dir(&job);
    let marker_path = dir.join(TICKETS_FILE);
    let mut marker = Marker::load(&marker_path)?;
    let spec = std::fs::read_to_string(dir.join("spec.md")).unwrap_or_default();
    let items = follow_ups(&spec);

    let mut labels: Vec<String> = Vec::new();
    for parent in &parents {
        match tracker.labels_of(*parent) {
            Ok(found) => {
                for label in found {
                    if label != "tracking" && !labels.contains(&label) {
                        labels.push(label);
                    }
                }
            }
            Err(e) => {
                report.fail(format!("labels of #{parent}: {e}"));
                return Ok(finish(driver, &job, dry_run, report));
            }
        }
    }

    for item in &items {
        if marker.created.iter().any(|(title, _)| *title == item.title) {
            continue;
        }
        if dry_run {
            report.lines.push(format!("would create: {} [{}]", item.title, labels.join(", ")));
            continue;
        }
        let body = format!(
            "{}\n\nFiled by factory job {} ({}) from the follow-ups of {}.",
            item.body,
            job.id,
            job.slug,
            list(&parents)
        );
        match tracker.create(&item.title, &body, &labels) {
            Ok(number) => {
                marker.created.push((item.title.clone(), number));
                marker.save(&marker_path)?;
                report.lines.push(format!("created #{number}: {}", item.title));
            }
            Err(e) => {
                report.fail(format!("create `{}`: {e}", item.title));
                return Ok(finish(driver, &job, dry_run, report));
            }
        }
    }

    let filed: Vec<u32> = items
        .iter()
        .filter_map(|item| marker.created.iter().find(|(title, _)| *title == item.title).map(|(_, n)| *n))
        .collect();
    let main_tip = driver.repo().tip("main").unwrap_or_default();
    let short = main_tip.get(..7).unwrap_or(&main_tip).to_string();
    let pushed = driver.repo().is_ancestor(&main_tip, "origin/main").unwrap_or(false);
    let note = if pushed { "" } else { " (not yet pushed to origin)" };
    let remaining = if items.is_empty() {
        "No remaining work was listed in the spec.".to_string()
    } else if filed.len() == items.len() {
        format!("Remaining work is filed as {}.", list(&filed))
    } else {
        format!("Remaining work: {}.", items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>().join("; "))
    };
    let comment = format!("Job {} ({}) is merged, main is at {short}{note}.\n\n{remaining}", job.id, job.slug);

    for parent in &parents {
        if !marker.commented.contains(parent) {
            if dry_run {
                report.lines.push(format!("would comment on #{parent}"));
            } else if let Err(e) = tracker.comment(*parent, &comment) {
                report.fail(format!("comment on #{parent}: {e}"));
                return Ok(finish(driver, &job, dry_run, report));
            } else {
                marker.commented.push(*parent);
                marker.save(&marker_path)?;
                report.lines.push(format!("commented on #{parent}"));
            }
        }
        if marker.closed.contains(parent) {
            continue;
        }
        match tracker.is_closed(*parent) {
            Ok(true) => {
                if dry_run {
                    report.lines.push(format!("#{parent} is already closed"));
                } else {
                    marker.closed.push(*parent);
                    marker.save(&marker_path)?;
                    report.lines.push(format!("#{parent} was already closed"));
                }
            }
            Ok(false) if dry_run => report.lines.push(format!("would close #{parent}")),
            Ok(false) => match tracker.close(*parent) {
                Ok(()) => {
                    marker.closed.push(*parent);
                    marker.save(&marker_path)?;
                    report.lines.push(format!("closed #{parent}"));
                }
                Err(e) => {
                    report.fail(format!("close #{parent}: {e}"));
                    return Ok(finish(driver, &job, dry_run, report));
                }
            },
            Err(e) => {
                report.fail(format!("state of #{parent}: {e}"));
                return Ok(finish(driver, &job, dry_run, report));
            }
        }
    }
    Ok(finish(driver, &job, dry_run, report))
}

/// Writes one ledger line for a real run and hands the report back.
fn finish(driver: &Driver, job: &factory_core::Job, dry_run: bool, report: TicketsReport) -> TicketsReport {
    if !dry_run {
        let state = if report.failed { "failed" } else { "done" };
        let text = format!("tickets {state}: {}", report.lines.join("; "));
        let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if let Err(e) = driver.ledger(job).ruling(&one_line) {
            eprintln!("tickets ledger error for job {}: {e}", job.id);
        }
    }
    report
}
```

Note on the comment-then-close order inside the loop: a parent that failed to comment must not be closed, which holds because the comment failure returns early. In the dry-run branch of the comment `if` chain, the `else if let Err(e)` arm is skipped, so check that the code compiles; if the borrow checker or the `if / else if let / else` chain complains, rewrite it as a `match` on `dry_run`.

Add `pub use tickets::{tickets_step, TicketsReport, TICKETS_FILE};` to `lib.rs`.

- [ ] **Step 5: Run the tests.** `cargo test -j4 -p factory-engine --test tickets`, then `cargo check -j4 -p factory-engine`. Expected: all pass. If `files_one_ticket_per_follow_up_comments_once_and_closes` fails on the title of the first ticket, check that the title is cut at the first `. ` of the bullet.

### Task 3: The tickets_on_merge key and the merge hook

**Files:**
- Modify: `tools/factory/crates/factory-core/src/config.rs` (`tickets_on_merge`, default and test)
- Modify: `tools/factory/crates/factory-engine/src/merge.rs` (the hook)
- Modify: `tools/factory/crates/factory-engine/tests/tickets.rs` (hook tests)

**Interfaces:**
- Consumes: `tickets_step` from Task 2.
- Produces: `Config.tickets_on_merge: bool`, default `false`.

- [ ] **Step 1: Write the failing tests.** In `config.rs` tests, next to the `commit_batch` test, add:

```rust
    #[test]
    fn tickets_on_merge_is_off_by_default_and_can_be_turned_on() {
        assert!(!Config::default().tickets_on_merge);
        assert!(Config::parse("tickets_on_merge = true").expect("parse").tickets_on_merge);
        assert!(Config::parse("tickets_on_merge = \"yes\"").is_err());
    }
```

Append to `tests/tickets.rs`:

```rust
use factory_core::Config;

/// Like `merged` but the merge itself runs the hook, so the fixture stops at the approved stage.
fn approved_with_hook(on: bool, spec: Option<&str>, tracker: &FakeTracker) -> MergeFixture {
    let config = Config { tickets_on_merge: on, ..Config::default() };
    let fx = with_tracker(common::approved_job_config(common::approving_runner(3), false, config), tracker.clone());
    set_feature(&fx, "dedup cleanups #76", spec);
    fx
}

#[test]
fn with_the_key_off_no_merge_calls_the_tracker() {
    let t = tracker();
    let fx = approved_with_hook(false, Some(SPEC_WITH_TWO), &t);
    assert!(matches!(merge_stage(&fx.driver, &fx.id).expect("merge"), MergeResult::Merged(_)));
    assert!(t.calls().is_empty(), "{:?}", t.calls());
}

#[test]
fn with_the_key_on_the_merge_files_tickets_and_closes_the_parent() {
    let t = tracker();
    let fx = approved_with_hook(true, Some(SPEC_WITH_TWO), &t);
    assert!(matches!(merge_stage(&fx.driver, &fx.id).expect("merge"), MergeResult::Merged(_)));
    let s = t.state.lock().expect("lock");
    assert_eq!(s.created.len(), 2);
    assert_eq!(s.closes, vec![76]);
}

#[test]
fn a_failing_tracker_never_changes_the_merge_result() {
    let t = tracker();
    t.fail_on("repo_name");
    let fx = approved_with_hook(true, Some(SPEC_WITH_TWO), &t);
    assert!(matches!(merge_stage(&fx.driver, &fx.id).expect("merge"), MergeResult::Merged(_)));
    let job = fx.driver.job(&fx.id).expect("job");
    assert_eq!(job.stage, factory_core::Stage::Merged);
    let ledger = std::fs::read_to_string(fx.driver.job_dir(&job).join("progress.md")).expect("ledger");
    assert!(ledger.contains("tickets failed"), "{ledger}");
}
```

If `approved_job_config` or `approving_runner` are not re-exported through `common::`, use the full path as written (they are `pub fn` in `common/mod.rs`).

- [ ] **Step 2: Run to see them fail.** `cargo test -j4 -p factory-core tickets_on_merge` fails to compile (no field). 

- [ ] **Step 3: Implement.** In `config.rs` add to `Config` after `builder_excerpt`:

```rust
    /// After a merge, file the follow-up tickets and close the parent tickets. Off by default.
    pub tickets_on_merge: bool,
```

and `tickets_on_merge: false,` in `Default`. In `merge.rs` `merge_stage`, between the kb refresh block and the costs refresh, add:

```rust
            // Ticket hygiene is best effort too: a failure is written to the ledger and never changes the merge.
            if driver.config().tickets_on_merge {
                match crate::tickets::tickets_step(driver, id, false) {
                    Ok(report) if report.failed => eprintln!("tickets error for job {id}: {}", report.lines.join("; ")),
                    Ok(_) => {}
                    Err(err) => eprintln!("tickets error for job {id}: {err}"),
                }
            }
```

- [ ] **Step 4: Run the tests.** `cargo test -j4 -p factory-core config`, `cargo test -j4 -p factory-engine --test tickets`, `cargo test -j4 -p factory-engine --test merge`, then `cargo check -j4 -p factory-core` and `cargo check -j4 -p factory-engine`. Expected: all pass. If `config.rs` has a place that lists every key (a test that parses a full sample file, or `.claude/factory/config.toml` lint), keep it consistent.

Next plan: docs/superpowers/plans/2026-10-17-factory-ticket-hygiene/plan-03-factory-ticket-hygiene.md
