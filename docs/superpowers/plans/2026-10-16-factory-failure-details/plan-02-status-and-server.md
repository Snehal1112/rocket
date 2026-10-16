# Factory Failure Details, Plan 02: factory status and the park endpoint

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `factory status` prints the reason, an excerpt and the next commands for parked jobs, and `factory-server` serves the full evidence at `GET /api/jobs/<id>/park` and tells the dashboard which jobs have details.

**Architecture:** The CLI reads `park.json` through `factory_core::ParkRecord` and prints a short block per parked job. The server is a separate workspace that does not depend on `factory-core`, so it reads the same JSON into its own sanitizing structs. The polled `/api/state` only gains a `has_park` flag per job, never the evidence text.

**Tech Stack:** Rust 2021, serde, serde_json, tiny_http, tempfile (tests). Workspaces `tools/factory/` and `tools/factory-server/`.

**Spec:** `docs/superpowers/specs/2026-10-16-factory-failure-details-design.md` (Design 1 for `factory status`, Design 2, Testing).

## Global Constraints

- Plan 01 is merged on this branch first: `factory_core::{ParkRecord, ParkKind, ParkPart}` and the `Job.park_kind` and `Job.park_at` board fields exist.
- Commands: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli <test>` for the CLI. For the server: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml <test>`. One crate per call, never `--workspace`, never omit `-j4`, never set `CARGO_TARGET_DIR`.
- No panicking unwrap calls in production code (a hook blocks that text in Rust edits; `unwrap_or`, `unwrap_or_default`, `map_or` are fine). `.expect("...")` only in tests.
- Comments are short full sentences ending in a period. No emojis.
- The server never writes `park.json`, the board or any other file. All replies already carry `Cache-Control: no-store` and the CSP from `serve()`.
- The park path comes from the board's `dir` field, never from the URL. It must stay inside `<root>/.superpowers/sdd/`.
- The reply text is capped: at most `MAX_REPLY_TEXT = 1024 * 1024` bytes of evidence text in total, keeping the end of each text.
- Board JSON stays compatible: the server's `Job` reads `park_kind` and `park_at` leniently, a bad value is dropped.
- Commit steps are for a human running the plan by hand: pathspec commit only, message through the skill `dev-workflow-skills:1-git-commit`, never `git add -A` or `git add .`.

## Review Focus

1. A job id or board `dir` with `..`, `/`, a backslash or a symlink that points outside the sdd folder must give 404 and never read a file outside it. Pinned in Task 3.
2. A `park.json` with unknown fields, wrong types or a 5 MiB part: unknown fields are dropped, wrong types give 422, and the reply stays under the cap with `truncated` set. Pinned in Task 3.
3. A parked job with no `park.json` (an older job) must show no button (`has_park` false) and the endpoint must answer 404. Pinned in Tasks 2 and 3.
4. `factory status` for a cancelled job or a job without a record must still print the reason and must not fail. Pinned in Task 1.
5. A POST or a foreign host on the new route is refused like the other routes. Pinned in Task 3.

## File Structure

```
tools/factory/crates/factory-cli/src/status.rs         render_parked, render_park_details
tools/factory/crates/factory-cli/src/commands.rs       Status prints the parked blocks
tools/factory/crates/factory-cli/src/lib.rs            re-export
tools/factory-server/src/model.rs                      Job.park_kind, Job.park_at
tools/factory-server/src/park.rs                       park_path, load_park, ParkView
tools/factory-server/src/main.rs                       mod park
tools/factory-server/src/state.rs                      JobView.has_park
tools/factory-server/src/specs.rs                      test helper JobView literal
tools/factory-server/src/server.rs                     route /api/jobs/<id>/park
```

---

### Task 1: factory status prints the park details

**Files:**
- Modify: `tools/factory/crates/factory-cli/src/status.rs`
- Modify: `tools/factory/crates/factory-cli/src/commands.rs`
- Modify: `tools/factory/crates/factory-cli/src/lib.rs`

**Interfaces:**
- Consumes: `factory_core::{Board, Job, ParkRecord, Stage}`.
- Produces: `pub fn render_parked(board: &Board, read: impl Fn(&Job) -> Option<ParkRecord>) -> String` and `pub fn render_park_details(job: &Job, record: Option<&ParkRecord>) -> String` in `status.rs`; `PARK_EXCERPT_LINES: usize = 12`.

- [ ] **Step 1: Write the failing tests**

Add to the tests module of `status.rs` (the module already imports `super::*`; add `use factory_core::{ParkKind, ParkPart, ParkRecord};` and `Stage` if not yet imported):

```rust
    fn parked_job(id: &str, reason: &str) -> factory_core::Job {
        let mut job = factory_core::Job::new(id, "d", "slug", "2026-10-16", "feature", 1.0);
        job.set_stage(Stage::Build, None, false, 2.0).expect("build");
        job.set_stage(Stage::NeedsHuman, Some(reason), false, 3.0).expect("park");
        job
    }

    fn record(id: &str, lines: usize) -> ParkRecord {
        let text = (1..=lines).map(|n| format!("line {n}")).collect::<Vec<_>>().join("\n");
        ParkRecord::new(
            ParkKind::ChecksFailed,
            "checks failed after 2 fix rounds",
            id,
            3,
            10,
            vec![ParkPart::new("Failing checks", "checks.md", &text)],
        )
    }

    #[test]
    fn a_parked_job_shows_reason_excerpt_and_next_commands() {
        let job = parked_job("012", "checks failed after 2 fix rounds");
        let text = render_park_details(&job, Some(&record("012", 3)));
        assert!(text.contains("012 needs a human: checks failed after 2 fix rounds"), "{text}");
        assert!(text.contains("Failing checks (checks.md)"), "{text}");
        assert!(text.contains("    line 1") && text.contains("    line 3"), "{text}");
        assert!(text.contains("  next: factory rework 012 <note>"), "{text}");
        assert!(!text.contains("more lines"), "{text}");
    }

    #[test]
    fn a_long_excerpt_is_cut_and_says_how_much_is_left() {
        let job = parked_job("012", "why");
        let text = render_park_details(&job, Some(&record("012", 20)));
        assert!(text.contains("    line 12"), "{text}");
        assert!(!text.contains("line 13"), "{text}");
        assert!(text.contains("8 more lines in park.json"), "{text}");
    }

    #[test]
    fn a_parked_job_without_a_record_still_shows_its_reason() {
        let job = parked_job("012", "cancelled: not needed");
        let text = render_park_details(&job, None);
        assert_eq!(text, "012 needs a human: cancelled: not needed\n");
    }

    #[test]
    fn render_parked_lists_only_parked_jobs_that_were_not_cancelled() {
        let mut board = factory_core::Board::default();
        board.jobs.push(parked_job("001", "first"));
        let mut cancelled = parked_job("002", "cancelled: x");
        cancelled.cancelled = true;
        board.jobs.push(cancelled);
        board.jobs.push(factory_core::Job::new("003", "d", "s", "2026-10-16", "f", 1.0));
        let text = render_parked(&board, |j| Some(record(&j.id, 1)));
        assert!(text.contains("001 needs a human: first"), "{text}");
        assert!(!text.contains("002") && !text.contains("003"), "{text}");
    }

    #[test]
    fn render_parked_of_a_board_with_nothing_parked_is_empty() {
        assert_eq!(render_parked(&factory_core::Board::default(), |_| None), "");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli status::tests`
Expected: compile errors, `render_park_details` and `render_parked` not found.

- [ ] **Step 3: Implement**

At the top of `status.rs` change the import to `use factory_core::{Board, Config, Job, ParkRecord, ReviewState, Stage};` and add:

```rust
/// How many lines of the first evidence part `factory status` prints.
pub const PARK_EXCERPT_LINES: usize = 12;

/// The block under the table for one parked job: reason, excerpt and next commands.
pub fn render_park_details(job: &Job, record: Option<&ParkRecord>) -> String {
    let reason = job.reason.clone().unwrap_or_else(|| "needs a human".to_string());
    let mut out = format!("{} needs a human: {}\n", job.id, reason);
    let Some(rec) = record else {
        return out;
    };
    if let Some(part) = rec.parts.first() {
        out.push_str(&format!("  {} ({})\n", part.title, part.source));
        for line in part.text.lines().take(PARK_EXCERPT_LINES) {
            out.push_str(&format!("    {line}\n"));
        }
        let total = part.text.lines().count();
        if total > PARK_EXCERPT_LINES {
            out.push_str(&format!("    ... {} more lines in park.json\n", total - PARK_EXCERPT_LINES));
        }
    }
    for command in &rec.next {
        out.push_str(&format!("  next: {command}\n"));
    }
    out
}

/// Park blocks for every parked job that was not cancelled, or an empty string when there are none.
pub fn render_parked(board: &Board, read: impl Fn(&Job) -> Option<ParkRecord>) -> String {
    let mut out = String::new();
    for job in board.jobs.iter().filter(|j| j.stage == Stage::NeedsHuman && !j.cancelled) {
        if out.is_empty() {
            out.push('\n');
        }
        out.push_str(&render_park_details(job, read(job).as_ref()));
    }
    out
}
```

In `lib.rs` change the re-export to `pub use status::{render_park_details, render_parked, render_status};`.

In `commands.rs` change the import `use crate::status::render_status;` to `use crate::status::{render_parked, render_status};`, add `ParkRecord` to the `factory_core` import there, and replace the `Command::Status` arm with:

```rust
        Command::Status => {
            let d = ctx.driver()?;
            let board = d.store().load()?;
            let mut out = render_status(&board, d.config());
            out.push_str(&render_parked(&board, |j| ParkRecord::read(&d.job_dir(j))));
            Ok(out)
        }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli`
Expected: all pass, including the existing `render_status` tests. Then `cargo check -j4 --manifest-path tools/factory/Cargo.toml`.

- [ ] **Step 5: Commit**

Commit `status.rs`, `commands.rs` and `lib.rs` with the skill message `feat(factory): show park details in factory status`.

---

### Task 2: has_park flag in /api/state

**Files:**
- Create: `tools/factory-server/src/park.rs`
- Modify: `tools/factory-server/src/main.rs` (add `mod park;` next to the other `mod` lines)
- Modify: `tools/factory-server/src/model.rs`
- Modify: `tools/factory-server/src/state.rs`
- Modify: `tools/factory-server/src/specs.rs` (test helper)

**Interfaces:**
- Produces:
  - `Job.park_kind: Option<String>`, `Job.park_at: Option<u64>` in `model.rs`.
  - `pub fn park_path(root: &Path, dir: &str) -> Option<PathBuf>` in `park.rs`: `None` when `dir` is empty, contains `/`, `\`, `..` or a NUL, else `<root>/.superpowers/sdd/<dir>/park.json`.
  - `JobView.has_park: bool`.

- [ ] **Step 1: Write the failing tests**

In the new `park.rs`, write the tests module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn park_path_joins_the_job_folder_under_the_sdd_root() {
        let p = park_path(Path::new("/r"), "2026-10-13-001-x");
        assert_eq!(p, Some(PathBuf::from("/r/.superpowers/sdd/2026-10-13-001-x/park.json")));
    }

    #[test]
    fn park_path_refuses_anything_that_could_leave_the_folder() {
        for bad in ["", "..", "../x", "a/b", "a\\b", "a/../b", "x\0y", "/abs"] {
            assert_eq!(park_path(Path::new("/r"), bad), None, "{bad:?}");
        }
    }
}
```

In `state.rs` tests add (the tests module has `job(id, stage, since)` helper; add `park_kind: None, park_at: None,` to its `Job { .. }` literal, and `has_park: false` to the `JobView { .. }` literal in `specs.rs` test helper `view`):

```rust
    #[test]
    fn has_park_needs_both_the_board_kind_and_the_file() -> TestResult {
        let root = tempfile::tempdir()?;
        let dir = root.path().join(".superpowers/sdd/2026-10-13-001-x");
        std::fs::create_dir_all(&dir)?;
        let mut parked = job("001", "needs-human", 0.0);
        parked.park_kind = Some("checks-failed".to_string());
        let board = Board { jobs: vec![parked.clone()] };
        // The kind is on the board but the file is missing.
        let view = build_state(&board, &Registry::new(), 10.0, None, root.path());
        assert!(!view.jobs[0].has_park);
        std::fs::write(dir.join("park.json"), "{}")?;
        let view = build_state(&board, &Registry::new(), 10.0, None, root.path());
        assert!(view.jobs[0].has_park);
        // The file exists but the board has no kind, as for an older job.
        let mut old = parked;
        old.park_kind = None;
        let view = build_state(&Board { jobs: vec![old] }, &Registry::new(), 10.0, None, root.path());
        assert!(!view.jobs[0].has_park);
        Ok(())
    }

    #[test]
    fn the_state_json_carries_has_park_but_no_evidence_text() -> TestResult {
        let root = tempfile::tempdir()?;
        let dir = root.path().join(".superpowers/sdd/2026-10-13-001-x");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("park.json"), r#"{"parts":[{"text":"SECRET-EVIDENCE"}]}"#)?;
        let mut parked = job("001", "needs-human", 0.0);
        parked.park_kind = Some("other".to_string());
        let view = build_state(&Board { jobs: vec![parked] }, &Registry::new(), 10.0, None, root.path());
        let json = serde_json::to_string(&view)?;
        assert!(json.contains("\"has_park\":true"), "{json}");
        assert!(json.contains("\"park_kind\":\"other\""), "{json}");
        assert!(!json.contains("SECRET-EVIDENCE"), "{json}");
        Ok(())
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml park`
Expected: compile errors (`park_path`, `park_kind`, `has_park` not found).

- [ ] **Step 3: Implement**

`park.rs` (above the tests):

```rust
//! Reads the driver's `park.json` for the dashboard. The server never writes it.

use std::path::{Path, PathBuf};

/// The file the driver writes into a job folder when it parks the job.
pub const PARK_FILE: &str = "park.json";

/// `<root>/.superpowers/sdd/<dir>/park.json`, or `None` when `dir` could point outside the sdd folder.
pub fn park_path(root: &Path, dir: &str) -> Option<PathBuf> {
    let bad = dir.is_empty() || dir.contains(['/', '\\', '\0']) || dir.contains("..");
    if bad {
        return None;
    }
    Some(root.join(".superpowers").join("sdd").join(dir).join(PARK_FILE))
}
```

`model.rs`: add to `Job` after `cancelled`:

```rust
    /// Why the job is parked, written by the driver. A bad value is dropped.
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub park_kind: Option<String>,
    /// Unix seconds the job parked. A bad value is dropped.
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub park_at: Option<u64>,
```

`state.rs`: add `pub has_park: bool,` to `JobView` (after `token_view`), set `has_park: has_park(root, job),` in the `JobView { .. }` literal inside `build_state`, and add:

```rust
/// True when the board names a park kind and the evidence file is in the job folder.
fn has_park(root: &Path, job: &Job) -> bool {
    job.park_kind.is_some() && crate::park::park_path(root, &job.dir).is_some_and(|p| p.is_file())
}
```

(add `use crate::model::Job;` if `Job` is not yet imported there; it is used by `JobView` already.)

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml`
Expected: all pass.

- [ ] **Step 5: Commit**

Commit `park.rs`, `main.rs`, `model.rs`, `state.rs` and `specs.rs` with the skill message `feat(factory-server): flag jobs that have park details`.

---

### Task 3: GET /api/jobs/<id>/park

**Files:**
- Modify: `tools/factory-server/src/park.rs`
- Modify: `tools/factory-server/src/server.rs`

**Interfaces:**
- Consumes: `park_path`, `load_board` (`state.rs`), `Board`.
- Produces:
  - `pub struct ParkView { kind: String, reason: String, at: u64, round: u32, next: Vec<String>, parts: Vec<PartView> }` and `pub struct PartView { title: String, source: String, text: String, truncated: bool }`, both `Serialize + Deserialize` with `#[serde(default)]` on every field, so unknown fields are dropped and missing ones are empty.
  - `pub enum ParkReply { Found(ParkView), NotFound, Broken }`.
  - `pub fn load_park(root: &Path, board: &Board, id: &str) -> ParkReply`.
  - `pub const MAX_REPLY_TEXT: usize = 1024 * 1024`.

- [ ] **Step 1: Write the failing tests**

Add to `park.rs` tests module:

```rust
    use crate::model::Board;

    fn board_with(dir: &str) -> Board {
        let text = format!(
            r#"{{"jobs":[{{"id":"001","dir":"{dir}","slug":"s","created":"c","feature":"f","branch":"b","stage":"needs-human","round":1,"stage_since":1.0,"reviews":{{}}}}]}}"#
        );
        serde_json::from_str(&text).expect("board")
    }

    fn write_park(root: &Path, dir: &str, json: &str) {
        let folder = root.join(".superpowers/sdd").join(dir);
        std::fs::create_dir_all(&folder).expect("dir");
        std::fs::write(folder.join(PARK_FILE), json).expect("park.json");
    }

    #[test]
    fn a_valid_file_is_returned_without_unknown_fields() {
        let root = tempfile::tempdir().expect("tempdir");
        write_park(
            root.path(),
            "d",
            r#"{"kind":"checks-failed","reason":"why","at":5,"round":2,"next":["factory rework 001 <note>"],
                "secret":"x","parts":[{"title":"T","source":"s","text":"body","truncated":false,"extra":1}]}"#,
        );
        match load_park(root.path(), &board_with("d"), "001") {
            ParkReply::Found(v) => {
                assert_eq!(v.kind, "checks-failed");
                assert_eq!(v.parts[0].text, "body");
                let json = serde_json::to_string(&v).expect("json");
                assert!(!json.contains("secret") && !json.contains("extra"), "{json}");
            }
            _ => panic!("expected Found"),
        }
    }

    #[test]
    fn missing_file_unknown_job_and_bad_ids_are_not_found() {
        let root = tempfile::tempdir().expect("tempdir");
        let board = board_with("d");
        assert!(matches!(load_park(root.path(), &board, "001"), ParkReply::NotFound));
        write_park(root.path(), "d", "{}");
        assert!(matches!(load_park(root.path(), &board, "002"), ParkReply::NotFound));
        for bad in ["", "1", "../1", "00a", "0000000"] {
            assert!(matches!(load_park(root.path(), &board, bad), ParkReply::NotFound), "{bad:?}");
        }
    }

    #[test]
    fn a_board_dir_that_leaves_the_sdd_folder_is_not_found() {
        let root = tempfile::tempdir().expect("tempdir");
        write_park(root.path(), "d", "{}");
        for dir in ["..", "../d", "d/..", "a/b"] {
            assert!(matches!(load_park(root.path(), &board_with(dir), "001"), ParkReply::NotFound), "{dir:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_job_folder_that_points_outside_is_not_found() {
        let root = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join(PARK_FILE), r#"{"kind":"other","reason":"leak"}"#).expect("file");
        let sdd = root.path().join(".superpowers/sdd");
        std::fs::create_dir_all(&sdd).expect("sdd");
        std::os::unix::fs::symlink(outside.path(), sdd.join("d")).expect("symlink");
        assert!(matches!(load_park(root.path(), &board_with("d"), "001"), ParkReply::NotFound));
    }

    #[test]
    fn a_broken_file_is_reported_as_broken() {
        let root = tempfile::tempdir().expect("tempdir");
        write_park(root.path(), "d", "not json");
        assert!(matches!(load_park(root.path(), &board_with("d"), "001"), ParkReply::Broken));
        write_park(root.path(), "d", r#"{"parts":"not a list"}"#);
        assert!(matches!(load_park(root.path(), &board_with("d"), "001"), ParkReply::Broken));
    }

    #[test]
    fn the_reply_text_is_capped_and_keeps_the_end() {
        let root = tempfile::tempdir().expect("tempdir");
        let big = format!("{}END", "x".repeat(MAX_REPLY_TEXT + 500));
        let json = serde_json::json!({
            "kind": "other", "reason": "r", "at": 1, "round": 1, "next": [],
            "parts": [{"title": "Big", "source": "s", "text": big, "truncated": false},
                      {"title": "Second", "source": "s", "text": "later", "truncated": false}]
        });
        write_park(root.path(), "d", &json.to_string());
        match load_park(root.path(), &board_with("d"), "001") {
            ParkReply::Found(v) => {
                let total: usize = v.parts.iter().map(|p| p.text.len()).sum();
                assert!(total <= MAX_REPLY_TEXT, "{total}");
                assert!(v.parts[0].truncated);
                assert!(v.parts[0].text.ends_with("END"));
                assert!(v.parts[1].truncated, "the second part had no room left");
            }
            _ => panic!("expected Found"),
        }
    }
```

Add to the tests module of `server.rs` (it already has `app_with_board`, `JOB` with job id `001` and dir `d`):

```rust
    #[test]
    fn the_park_route_answers_200_404_422_and_refuses_other_methods() -> TestResult {
        let (dir, app) = app_with_board(Some(JOB))?;
        let host = Some("127.0.0.1:1");
        // No file yet.
        assert_eq!(route(&app, "GET", "/api/jobs/001/park", host, 1, 5.0).status, 404);
        let folder = dir.path().join(".superpowers/sdd/d");
        std::fs::create_dir_all(&folder)?;
        std::fs::write(folder.join("park.json"), r#"{"kind":"checks-failed","reason":"why","at":5,"round":1,"next":[],"parts":[]}"#)?;
        let ok = route(&app, "GET", "/api/jobs/001/park?x=1", host, 1, 5.0);
        assert_eq!(ok.status, 200);
        assert_eq!(ok.content_type, "application/json");
        let json: serde_json::Value = serde_json::from_slice(&ok.body)?;
        assert_eq!(json["kind"], "checks-failed");
        std::fs::write(folder.join("park.json"), "not json")?;
        assert_eq!(route(&app, "GET", "/api/jobs/001/park", host, 1, 5.0).status, 422);
        assert_eq!(route(&app, "POST", "/api/jobs/001/park", host, 1, 5.0).status, 405);
        assert_eq!(route(&app, "GET", "/api/jobs/001/park", Some("evil.example"), 1, 5.0).status, 403);
        assert_eq!(route(&app, "GET", "/api/jobs/zzz/park", host, 1, 5.0).status, 404);
        assert_eq!(route(&app, "GET", "/api/jobs/001", host, 1, 5.0).status, 404);
        assert_eq!(route(&app, "GET", "/api/jobs/001/park/extra", host, 1, 5.0).status, 404);
        Ok(())
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml park`
Expected: compile errors (`load_park`, `ParkReply`, `MAX_REPLY_TEXT` not found).

- [ ] **Step 3: Implement**

Append to `park.rs` above the tests (keep `park_path` and `PARK_FILE`):

```rust
use serde::{Deserialize, Serialize};

use crate::model::Board;

/// Most evidence text, in bytes, one reply carries.
pub const MAX_REPLY_TEXT: usize = 1024 * 1024;

/// One block of evidence as the dashboard gets it. Unknown fields in the file are dropped.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PartView {
    pub title: String,
    pub source: String,
    pub text: String,
    pub truncated: bool,
}

/// The park record as the dashboard gets it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ParkView {
    pub kind: String,
    pub reason: String,
    pub at: u64,
    pub round: u32,
    pub next: Vec<String>,
    pub parts: Vec<PartView>,
}

pub enum ParkReply {
    Found(ParkView),
    NotFound,
    Broken,
}

fn valid_id(id: &str) -> bool {
    (3..=6).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit())
}

/// Keeps the end of `text`, at most `max` bytes, cut on a character boundary.
fn tail(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut start = text.len() - max;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_string()
}

/// Shares `MAX_REPLY_TEXT` over the parts in order. A part that does not fit is cut and marked truncated.
fn cap_parts(view: &mut ParkView) {
    let mut room = MAX_REPLY_TEXT;
    for part in &mut view.parts {
        if part.text.len() > room {
            part.text = tail(&part.text, room);
            part.truncated = true;
        }
        room -= part.text.len();
    }
}

/// Loads the park record of one job. The path comes from the board, never from the request.
pub fn load_park(root: &Path, board: &Board, id: &str) -> ParkReply {
    if !valid_id(id) {
        return ParkReply::NotFound;
    }
    let Some(job) = board.jobs.iter().find(|j| j.id == id) else {
        return ParkReply::NotFound;
    };
    let Some(path) = park_path(root, &job.dir) else {
        return ParkReply::NotFound;
    };
    // A symlinked job folder must not lead outside the sdd folder.
    let sdd = root.join(".superpowers").join("sdd");
    let (Ok(real), Ok(real_sdd)) = (path.canonicalize(), sdd.canonicalize()) else {
        return ParkReply::NotFound;
    };
    if !real.starts_with(&real_sdd) {
        return ParkReply::NotFound;
    }
    let Ok(text) = std::fs::read_to_string(&real) else {
        return ParkReply::NotFound;
    };
    match serde_json::from_str::<ParkView>(&text) {
        Ok(mut view) => {
            cap_parts(&mut view);
            ParkReply::Found(view)
        }
        Err(_) => ParkReply::Broken,
    }
}
```

(`Board` is in `crate::model`; if `build_state` imports it from `crate::model::Board`, copy that path.)

In `server.rs`: add `use crate::park::{load_park, ParkReply};` and a helper plus the route. Because the match in `route` is on exact paths, add the new branch before the `match path`:

```rust
    if let Some(id) = path.strip_prefix("/api/jobs/").and_then(|rest| rest.strip_suffix("/park")) {
        return api_park(app, id);
    }
```

and the function:

```rust
fn api_park(app: &App, id: &str) -> Reply {
    let board_path = app.root.join(".superpowers").join("factory").join("board.json");
    let (board, _) = load_board(&board_path);
    match load_park(&app.root, &board, id) {
        ParkReply::Found(view) => match serde_json::to_vec(&view) {
            Ok(body) => Reply { status: 200, content_type: "application/json", body },
            Err(_) => text(500, "could not encode park details"),
        },
        ParkReply::NotFound => text(404, "no park details for this job"),
        ParkReply::Broken => text(422, "park details are not readable"),
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml`
Expected: all pass (the existing route tests still pass: `/api/jobs/001` without `/park` falls to the 404 arm).

- [ ] **Step 5: Commit**

Commit `park.rs` and `server.rs` with the skill message `feat(factory-server): serve park details for a job`.

Next plan: docs/superpowers/plans/2026-10-16-factory-failure-details/plan-03-dashboard-popup.md
