# Factory Failure Details, Plan 01: park.json written by the driver

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every time the driver parks a job in `needs-human`, it records the kind, reason, next commands and evidence in `park.json` in the job folder, and puts `park_kind` and `park_at` on the board.

**Architecture:** `factory-core` gets a `park` module with the record types and the file writer, plus two optional board fields. `factory-engine` gets one `park_with` entry point that writes the record and then parks, and every park site calls it with a kind and evidence parts. The park of the merge stage and the run loop (`merge::park`) writes a record of kind `other` with no parts.

**Tech Stack:** Rust 2021, serde, serde_json, tempfile (tests). Workspace at `tools/factory/`.

**Spec:** `docs/superpowers/specs/2026-10-16-factory-failure-details-design.md` (sections Decisions, Design 1 and Testing).

## Global Constraints

- Commands run from the repo root as `cargo check -j4 --manifest-path tools/factory/Cargo.toml` and `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p <crate> <test_name>`. One crate per call. Never `--workspace`, never omit `-j4`, never set `CARGO_TARGET_DIR`.
- No panicking unwrap calls in production code (a hook blocks that text in Rust edits; `unwrap_or`, `unwrap_or_default` and `map_or` are fine). `.expect("...")` only inside tests.
- Comments are short full sentences ending in a period. No emojis.
- The board JSON stays compatible with `tools/factory-server`: new board fields are optional and skipped when empty. Writing `park.json` is best effort and never stops a park.
- Each evidence part is capped at 200 KB (`PART_CAP = 200 * 1024` bytes). The cap keeps the END of the text, because the failure is at the end of a log, and sets `truncated`.
- No dollar amounts, no LLM calls added, no dashboard or server changes in this plan.
- Commit steps are for a human running the plan by hand: pathspec commit only (`git add <paths> && git commit --only -m "..." -- <paths>`), message through the skill `dev-workflow-skills:1-git-commit`, never `git add -A` or `git add .`.

## Review Focus

1. A park with no evidence available (missing `checks.md`, no review file, no worktree) must still write a record with the reason and `next`, and still park. Pinned in Task 2.
2. A write failure for `park.json` (read-only job folder) must not stop the park. Pinned in Task 1 and Task 2.
3. A text over the cap keeps its end, is cut on a character boundary (multi-byte text must not panic), and sets `truncated`. Pinned in Task 1.
4. An old board without `park_kind` and `park_at` loads, and leaving `needs-human` clears both fields. Pinned in Task 1.
5. The decision and sync code that matches on `Decision::NeedsHuman` still compiles and still prefixes the sync reason. Pinned in Task 2.

## File Structure

```
tools/factory/crates/factory-core/src/park.rs         ParkKind, ParkPart, ParkRecord, write and read
tools/factory/crates/factory-core/src/board.rs        Job.park_kind, Job.park_at, set_stage clears them
tools/factory/crates/factory-core/src/lib.rs          module and re-exports
tools/factory/crates/factory-engine/src/park.rs       park_with and the evidence helpers
tools/factory/crates/factory-engine/src/lib.rs        mod park
tools/factory/crates/factory-engine/src/decide.rs     Decision::NeedsHuman { kind, reason }
tools/factory/crates/factory-engine/src/review.rs     apply_decision parks with evidence
tools/factory/crates/factory-engine/src/checks_stage.rs  fix_or_park takes a kind
tools/factory/crates/factory-engine/src/sync.rs       matches the new variant
tools/factory/crates/factory-engine/src/merge.rs      merge::park writes kind other
tools/factory/crates/factory-engine/src/build.rs      builder, commit and diff-cap parks
tools/factory/crates/factory-engine/tests/review_decide.rs  pattern updates
tools/factory/crates/factory-engine/tests/runloop.rs  park.json integration test
docs/superpowers/specs/2026-10-16-factory-failure-details-design.md  kind other, tail cap
```

---

### Task 1: Park record types and board fields in factory-core

**Files:**
- Create: `tools/factory/crates/factory-core/src/park.rs`
- Modify: `tools/factory/crates/factory-core/src/lib.rs`
- Modify: `tools/factory/crates/factory-core/src/board.rs`
- Modify: `docs/superpowers/specs/2026-10-16-factory-failure-details-design.md`

**Interfaces:**
- Produces:
  - `pub const PART_CAP: usize`
  - `pub enum ParkKind { ChecksFailed, ChangesRemain, RiskyDiff, DiffTooLarge, BudgetStopped, BuilderStopped, BuilderCommitted, NothingChanged, CommitFailed, Other }` with `as_str(self) -> &'static str` (kebab-case names, `Other` is `other`) and `next(self, id: &str) -> Vec<String>`.
  - `pub struct ParkPart { pub title: String, pub source: String, pub text: String, pub truncated: bool }` with `ParkPart::new(title: &str, source: &str, text: &str) -> ParkPart`.
  - `pub struct ParkRecord { pub kind: ParkKind, pub reason: String, pub at: u64, pub round: u32, pub next: Vec<String>, pub parts: Vec<ParkPart> }` with `ParkRecord::new(kind, reason: &str, id: &str, round: u32, at: u64, parts: Vec<ParkPart>) -> ParkRecord`, `write(&self, dir: &Path) -> Result<(), CoreError>` and `ParkRecord::read(dir: &Path) -> Option<ParkRecord>`.
  - `Job.park_kind: Option<String>` and `Job.park_at: Option<u64>`.
- Consumes: `write_atomic` from `fsutil.rs`, `CoreError`.

- [ ] **Step 1: Write the failing tests**

Create `tools/factory/crates/factory-core/src/park.rs` with only the tests first (the types come in Step 3):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_names_are_stable_kebab_case() {
        assert_eq!(ParkKind::ChecksFailed.as_str(), "checks-failed");
        assert_eq!(ParkKind::ChangesRemain.as_str(), "changes-remain");
        assert_eq!(ParkKind::RiskyDiff.as_str(), "risky-diff");
        assert_eq!(ParkKind::DiffTooLarge.as_str(), "diff-too-large");
        assert_eq!(ParkKind::BudgetStopped.as_str(), "budget-stopped");
        assert_eq!(ParkKind::BuilderStopped.as_str(), "builder-stopped");
        assert_eq!(ParkKind::BuilderCommitted.as_str(), "builder-committed");
        assert_eq!(ParkKind::NothingChanged.as_str(), "nothing-changed");
        assert_eq!(ParkKind::CommitFailed.as_str(), "commit-failed");
        assert_eq!(ParkKind::Other.as_str(), "other");
    }

    #[test]
    fn next_commands_carry_the_job_id() {
        assert_eq!(ParkKind::ChecksFailed.next("012"), vec!["factory rework 012 <note>"]);
        assert_eq!(
            ParkKind::RiskyDiff.next("012"),
            vec!["factory approve 012", "factory rework 012 <note>"]
        );
        assert_eq!(
            ParkKind::BudgetStopped.next("012"),
            vec!["factory approve 012", "factory cancel 012"]
        );
        assert_eq!(
            ParkKind::NothingChanged.next("012"),
            vec!["factory rework 012 <note>", "factory cancel 012"]
        );
        assert_eq!(
            ParkKind::Other.next("012"),
            vec!["factory approve 012", "factory rework 012 <note>", "factory cancel 012"]
        );
    }

    #[test]
    fn a_short_part_is_kept_whole() {
        let p = ParkPart::new("Checks", "checks.md", "FAIL cargo check");
        assert_eq!(p.text, "FAIL cargo check");
        assert!(!p.truncated);
    }

    #[test]
    fn a_long_part_keeps_its_end_and_is_marked_truncated() {
        let text = format!("{}END", "x".repeat(PART_CAP + 10));
        let p = ParkPart::new("Log", "log", &text);
        assert!(p.truncated);
        assert!(p.text.len() <= PART_CAP);
        assert!(p.text.ends_with("END"));
    }

    #[test]
    fn the_cap_cuts_on_a_character_boundary() {
        // Each char is 3 bytes, so a byte cut in the middle of one must move forward.
        let text = "€".repeat(PART_CAP / 3 + 5);
        let p = ParkPart::new("Log", "log", &text);
        assert!(p.truncated);
        assert!(p.text.chars().all(|c| c == '€'));
    }

    #[test]
    fn a_record_round_trips_through_park_json() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let rec = ParkRecord::new(
            ParkKind::ChecksFailed,
            "checks failed after 2 fix rounds",
            "012",
            3,
            1_760_000_000,
            vec![ParkPart::new("Failing checks", "checks.md", "FAIL cargo check")],
        );
        rec.write(dir.path())?;
        let back = ParkRecord::read(dir.path());
        assert_eq!(back, Some(rec));
        let raw = std::fs::read_to_string(dir.path().join("park.json"))?;
        assert!(raw.contains("\"kind\": \"checks-failed\""), "{raw}");
        Ok(())
    }

    #[test]
    fn read_of_a_missing_or_broken_file_is_none() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        assert_eq!(ParkRecord::read(dir.path()), None);
        std::fs::write(dir.path().join("park.json"), "not json")?;
        assert_eq!(ParkRecord::read(dir.path()), None);
        Ok(())
    }

    #[test]
    fn write_into_a_missing_unwritable_place_is_an_error_not_a_panic() {
        let rec = ParkRecord::new(ParkKind::Other, "x", "001", 1, 1, Vec::new());
        // A path under a regular file cannot be created.
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("f");
        std::fs::write(&file, "x").expect("file");
        assert!(rec.write(&file.join("sub")).is_err());
    }
}
```

In `board.rs` tests module (find the existing `#[cfg(test)] mod tests`), add:

```rust
    #[test]
    fn park_fields_are_set_kept_and_cleared_by_set_stage() {
        let mut job = Job::new("001", "d", "s", "2026-10-16", "f", 1.0);
        job.set_stage(Stage::Build, None, false, 2.0).expect("build");
        job.park_kind = Some("checks-failed".to_string());
        job.park_at = Some(5);
        job.set_stage(Stage::NeedsHuman, Some("why"), false, 3.0).expect("park");
        assert_eq!(job.park_kind.as_deref(), Some("checks-failed"));
        assert_eq!(job.park_at, Some(5));
        job.set_stage(Stage::Build, None, true, 4.0).expect("back");
        assert_eq!(job.park_kind, None);
        assert_eq!(job.park_at, None);
    }

    #[test]
    fn an_old_board_without_park_fields_loads_and_new_fields_are_skipped_when_empty() {
        let job = Job::new("001", "d", "s", "2026-10-16", "f", 1.0);
        let json = serde_json::to_string(&job).expect("json");
        assert!(!json.contains("park_kind") && !json.contains("park_at"), "{json}");
        let back: Job = serde_json::from_str(&json).expect("load");
        assert_eq!(back.park_kind, None);
    }
```

(`Stage::Build` to `Stage::NeedsHuman` and back with `force: true` is how the engine already moves jobs; check `Stage::check_move` allows spec -> build -> needs-human. If the existing tests use a different helper to reach `NeedsHuman`, copy it.)

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-core park`
Expected: compile errors (`ParkKind` and the others not found, `park_kind` not a field).

- [ ] **Step 3: Implement**

Put this above the tests module in `park.rs`:

```rust
//! The park record: why a job stopped in `needs-human`, the evidence and the next commands.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::fsutil::write_atomic;

/// Largest evidence text kept per part, in bytes.
pub const PART_CAP: usize = 200 * 1024;

/// The file name inside the job folder.
pub const PARK_FILE: &str = "park.json";

/// Why a job parked. The names are stable, the dashboard reads them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ParkKind {
    ChecksFailed,
    ChangesRemain,
    RiskyDiff,
    DiffTooLarge,
    BudgetStopped,
    BuilderStopped,
    BuilderCommitted,
    NothingChanged,
    CommitFailed,
    Other,
}

impl ParkKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ParkKind::ChecksFailed => "checks-failed",
            ParkKind::ChangesRemain => "changes-remain",
            ParkKind::RiskyDiff => "risky-diff",
            ParkKind::DiffTooLarge => "diff-too-large",
            ParkKind::BudgetStopped => "budget-stopped",
            ParkKind::BuilderStopped => "builder-stopped",
            ParkKind::BuilderCommitted => "builder-committed",
            ParkKind::NothingChanged => "nothing-changed",
            ParkKind::CommitFailed => "commit-failed",
            ParkKind::Other => "other",
        }
    }

    /// The commands worth running next, with the job id filled in.
    pub fn next(self, id: &str) -> Vec<String> {
        let approve = format!("factory approve {id}");
        let rework = format!("factory rework {id} <note>");
        let cancel = format!("factory cancel {id}");
        match self {
            ParkKind::ChecksFailed | ParkKind::BuilderStopped | ParkKind::BuilderCommitted | ParkKind::CommitFailed => {
                vec![rework]
            }
            ParkKind::ChangesRemain | ParkKind::RiskyDiff | ParkKind::DiffTooLarge => vec![approve, rework],
            ParkKind::BudgetStopped => vec![approve, cancel],
            ParkKind::NothingChanged => vec![rework, cancel],
            ParkKind::Other => vec![approve, rework, cancel],
        }
    }
}

/// One block of evidence, such as the failing check output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParkPart {
    pub title: String,
    pub source: String,
    pub text: String,
    pub truncated: bool,
}

impl ParkPart {
    /// Keeps at most `PART_CAP` bytes of `text`, the end of it, cut on a character boundary.
    pub fn new(title: &str, source: &str, text: &str) -> ParkPart {
        let (text, truncated) = if text.len() <= PART_CAP {
            (text.to_string(), false)
        } else {
            let mut start = text.len() - PART_CAP;
            while !text.is_char_boundary(start) {
                start += 1;
            }
            (text[start..].to_string(), true)
        };
        ParkPart { title: title.to_string(), source: source.to_string(), text, truncated }
    }
}

/// The content of `park.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParkRecord {
    pub kind: ParkKind,
    pub reason: String,
    /// Unix seconds.
    pub at: u64,
    pub round: u32,
    pub next: Vec<String>,
    pub parts: Vec<ParkPart>,
}

impl ParkRecord {
    pub fn new(kind: ParkKind, reason: &str, id: &str, round: u32, at: u64, parts: Vec<ParkPart>) -> ParkRecord {
        ParkRecord { kind, reason: reason.to_string(), at, round, next: kind.next(id), parts }
    }

    pub fn write(&self, dir: &Path) -> Result<(), CoreError> {
        // `CoreError::Json` converts from a serde_json error, so `?` is enough.
        let bytes = serde_json::to_vec_pretty(self)?;
        write_atomic(&dir.join(PARK_FILE), &bytes)
    }

    /// A missing or unreadable file is `None`, because old jobs have no record.
    pub fn read(dir: &Path) -> Option<ParkRecord> {
        let text = std::fs::read_to_string(dir.join(PARK_FILE)).ok()?;
        serde_json::from_str(&text).ok()
    }
}
```


In `lib.rs` add `pub mod park;` after `pub mod ledger;` and `pub use park::{ParkKind, ParkPart, ParkRecord, PARK_FILE, PART_CAP};` after the ledger re-export.

In `board.rs` add to `Job` (after `task_index`):

```rust
    /// Why the job is parked, see `ParkKind::as_str`. New in the failure details series.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub park_kind: Option<String>,
    /// Unix seconds the job parked. New in the failure details series.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub park_at: Option<u64>,
```

add `park_kind: None, park_at: None,` to `Job::new`, and in `set_stage` after the `self.reason = ...` line:

```rust
        if to != Stage::NeedsHuman {
            self.park_kind = None;
            self.park_at = None;
        }
```

In the spec file, add `other` to the kind list (`any other park: merge conflicts, main moved, a run loop that did not settle`, no evidence parts, `next` is approve, rework and cancel), add one row to the Design 1 table for it, and change the sentence "Each evidence part is capped at 200 KB when written, and marked `truncated`." to "Each evidence part is capped at 200 KB when written. The cap keeps the end of the text and marks the part `truncated`."

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-core`
Expected: all pass. Then `cargo check -j4 --manifest-path tools/factory/Cargo.toml` (the other crates build `Job { .. }` only through `Job::new`, so they must still compile).

- [ ] **Step 5: Commit**

Commit `park.rs`, `lib.rs`, `board.rs` and the spec file with the skill message `feat(factory): add the park record and board fields`.

---

### Task 2: park_with and the decision, review, checks and merge sites

**Files:**
- Create: `tools/factory/crates/factory-engine/src/park.rs`
- Modify: `tools/factory/crates/factory-engine/src/lib.rs` (add `mod park;`)
- Modify: `tools/factory/crates/factory-engine/src/decide.rs`
- Modify: `tools/factory/crates/factory-engine/src/review.rs`
- Modify: `tools/factory/crates/factory-engine/src/checks_stage.rs`
- Modify: `tools/factory/crates/factory-engine/src/sync.rs`
- Modify: `tools/factory/crates/factory-engine/src/merge.rs`
- Modify: `tools/factory/crates/factory-engine/tests/review_decide.rs`
- Modify: `tools/factory/crates/factory-engine/tests/runloop.rs`

**Interfaces:**
- Consumes: Task 1 types, `Engine::park(reason: String) -> Result<StageOutcome, EngineError>` in `ports.rs`, `Engine::update`, `now()` in `ports.rs`.
- Produces:
  - `Decision::NeedsHuman { kind: ParkKind, reason: String }`.
  - `Engine::park_with(&self, kind: ParkKind, reason: String, parts: Vec<ParkPart>) -> Result<StageOutcome, EngineError>` (`pub(crate)`).
  - `pub(crate) fn blocker_lines(review_text: &str) -> String`.
  - `Engine::fix_or_park(&self, findings: &str, reason: &str, kind: ParkKind, source: &str)`.

- [ ] **Step 1: Write the failing tests**

Create the tests module at the end of the new `tools/factory/crates/factory-engine/src/park.rs` (the code comes in Step 3):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocker_lines_keeps_only_the_blockers() {
        let text = "Summary line\n[blocker] src/a.rs:1 wrong\n[minor] nit\n[blocker] src/b.rs:2 bad\nVERDICT: CHANGES";
        assert_eq!(blocker_lines(text), "[blocker] src/a.rs:1 wrong\n[blocker] src/b.rs:2 bad");
    }

    #[test]
    fn blocker_lines_falls_back_to_the_whole_reply() {
        let text = "Please rework this\nVERDICT: CHANGES";
        assert_eq!(blocker_lines(text), text);
    }

    #[test]
    fn part_from_file_reads_the_file_and_skips_a_missing_one() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("checks.md"), "FAIL cargo check")?;
        let p = part_from_file(dir.path(), "checks.md", "Checks");
        assert_eq!(p.map(|p| p.text), Some("FAIL cargo check".to_string()));
        assert!(part_from_file(dir.path(), "missing.md", "Nope").is_none());
        Ok(())
    }
}
```

In `tests/review_decide.rs` update the existing patterns to the new variant (these are the failing tests of this step; they fail to compile until Step 3):

- line ~41: `Decision::NeedsHuman(r) => assert!(...)` becomes `Decision::NeedsHuman { reason: r, kind } => { assert!(r.contains("src-tauri/src/lib.rs"), "{r}"); assert_eq!(kind, factory_core::ParkKind::RiskyDiff); }`
- line ~51: `Decision::NeedsHuman(r) if r.contains("budget")` becomes `Decision::NeedsHuman { reason: r, kind } if r.contains("budget") && kind == factory_core::ParkKind::BudgetStopped`
- line ~67: `if r.contains("cap")` becomes `Decision::NeedsHuman { reason: r, kind } if r.contains("cap") && kind == factory_core::ParkKind::DiffTooLarge`
- line ~77: `if r.contains("fix rounds")` becomes `Decision::NeedsHuman { reason: r, kind } if r.contains("fix rounds") && kind == factory_core::ParkKind::ChecksFailed`
- line ~86 (the changes and missing verdict loop): `Decision::NeedsHuman(_)` becomes `Decision::NeedsHuman { kind: factory_core::ParkKind::ChangesRemain, .. }`
- line ~225: same shape as line ~41.

Add to `tests/runloop.rs`, after `run_series_stops_when_a_plan_needs_a_human`:

```rust
#[test]
fn a_parked_job_gets_a_park_record_with_evidence() {
    let fx = common::SeriesFixture::new("[blocker] src/a.rs wrong\nVERDICT: CHANGES");
    let a = fx.plan_file("plan-01-a.md", 1, "none");
    let job = create_job(&fx.driver, "stuck", "x", Some(&a), None).expect("create");
    assert!(matches!(run_job(&fx.driver, &job.id).expect("run"), RunEnd::NeedsHuman(_)));
    let board_job = fx.driver.job(&job.id).expect("job");
    assert_eq!(board_job.park_kind.as_deref(), Some("changes-remain"));
    assert!(board_job.park_at.unwrap_or(0) > 0);
    let rec = factory_core::ParkRecord::read(&fx.driver.job_dir(&board_job)).expect("park.json");
    assert_eq!(rec.kind, factory_core::ParkKind::ChangesRemain);
    assert!(rec.reason.contains("changes remain"), "{}", rec.reason);
    assert_eq!(rec.next, vec![format!("factory approve {}", job.id), format!("factory rework {} <note>", job.id)]);
    assert!(rec.parts.iter().any(|p| p.text.contains("[blocker] src/a.rs wrong")), "{:?}", rec.parts);
}

#[test]
fn leaving_needs_human_clears_the_board_park_fields() {
    let fx = common::SeriesFixture::new("[blocker] x\nVERDICT: CHANGES");
    let a = fx.plan_file("plan-01-a.md", 1, "none");
    let job = create_job(&fx.driver, "stuck", "x", Some(&a), None).expect("create");
    let _ = run_job(&fx.driver, &job.id).expect("run");
    fx.driver
        .store()
        .update_job(&job.id, |j| j.set_stage(factory_core::Stage::Build, None, true, 9.0))
        .expect("rework");
    let after = fx.driver.job(&job.id).expect("job");
    assert_eq!(after.park_kind, None);
    assert_eq!(after.park_at, None);
}
```

(If `fx.driver.job(..)` returns a different type than `Job`, follow how the neighbouring tests read the board, `fx.driver.store().load()`.)

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test review_decide`
Expected: compile errors, `Decision::NeedsHuman` has no field `reason`.

- [ ] **Step 3: Implement**

`decide.rs`: change the variant and the five returns.

```rust
use factory_core::{classify_paths, BudgetState, Config, DiffClass, ParkKind, Verdict};

    NeedsHuman { kind: ParkKind, reason: String },
```

```rust
        return Decision::NeedsHuman { kind: ParkKind::RiskyDiff, reason: format!("risky diff: {}", i.risky_summary) };
    // budget:
        return Decision::NeedsHuman { kind: ParkKind::BudgetStopped, reason: "budget is stopped".to_string() };
    // cap:
        return Decision::NeedsHuman {
            kind: ParkKind::DiffTooLarge,
            reason: format!("diff has {} changed lines, the cap is {}", i.diff_lines, i.diff_cap),
        };
    // checks:
            Decision::NeedsHuman { kind: ParkKind::ChecksFailed, reason: format!("checks failed after {used} fix rounds") }
    // changes:
                Decision::NeedsHuman { kind: ParkKind::ChangesRemain, reason: format!("changes remain after {used} fix rounds") }
```

`sync.rs` line ~60: `Decision::NeedsHuman { reason: why, .. } => Ok(SyncOutcome::Parked(format!("after sync: {why}"))),`

New `factory-engine/src/park.rs` (add `mod park;` in `lib.rs`, next to the other `mod` lines):

```rust
//! Parks a job and records why, with the evidence, in `park.json`.

use std::path::Path;

use factory_core::{ParkKind, ParkPart, ParkRecord};

use crate::ports::now;
use crate::{Engine, EngineError, StageOutcome};

/// The reviewer's `[blocker]` lines, or the whole reply when it has none.
pub(crate) fn blocker_lines(review_text: &str) -> String {
    let blockers: Vec<&str> = review_text.lines().filter(|l| l.contains("[blocker]")).collect();
    if blockers.is_empty() {
        review_text.to_string()
    } else {
        blockers.join("\n")
    }
}

/// An evidence part read from a file in the job folder. A missing file gives no part.
pub(crate) fn part_from_file(dir: &Path, file: &str, title: &str) -> Option<ParkPart> {
    let text = std::fs::read_to_string(dir.join(file)).ok()?;
    Some(ParkPart::new(title, file, &text))
}

impl Engine<'_> {
    /// Records why the job parks, then parks it. Writing `park.json` is best effort.
    pub(crate) fn park_with(&self, kind: ParkKind, reason: String, parts: Vec<ParkPart>) -> Result<StageOutcome, EngineError> {
        let round = self.job().map_or(1, |j| j.round);
        let at = now().max(0.0) as u64;
        let record = ParkRecord::new(kind, &reason, &self.job_id, round, at, parts);
        // A job folder that cannot be written must not stop the park.
        let _ = record.write(&self.job_dir);
        self.update(|j| {
            j.park_kind = Some(kind.as_str().to_string());
            j.park_at = Some(at);
            Ok(())
        })?;
        self.park(reason)
    }

    /// Evidence for a park the decision step chose. Anything that cannot be read is left out.
    pub(crate) fn decision_parts(&self, kind: ParkKind, reason: &str, round: u32) -> Vec<ParkPart> {
        match kind {
            ParkKind::ChecksFailed => part_from_file(&self.job_dir, "checks.md", "Checks").into_iter().collect(),
            ParkKind::ChangesRemain => {
                let file = format!("review-code-r{round}.md");
                match std::fs::read_to_string(self.job_dir.join(&file)) {
                    Ok(text) => vec![ParkPart::new("Reviewer findings", &file, &blocker_lines(&text))],
                    Err(_) => Vec::new(),
                }
            }
            ParkKind::RiskyDiff | ParkKind::DiffTooLarge => {
                let mut parts = vec![ParkPart::new("Why it stopped", "driver", reason)];
                if let Some(stat) = self.diff_stat_text() {
                    parts.push(ParkPart::new("Changed files", "diff stat", &stat));
                }
                parts
            }
            ParkKind::BudgetStopped => {
                let text = format!(
                    "{} weighted tokens used against a budget of {}",
                    self.job_weighted_tokens(),
                    self.config.budget_weighted
                );
                vec![ParkPart::new("Budget", "board", &text)]
            }
            _ => Vec::new(),
        }
    }

    /// The `--stat` text of the whole job diff, or `None` when the diff cannot be made.
    fn diff_stat_text(&self) -> Option<String> {
        let job = self.job().ok()?;
        let (base, head) = (job.base?, job.head?);
        let files = self.repo.diff_to_files(&base, &head, &self.job_dir).ok()?;
        std::fs::read_to_string(files.stat).ok()
    }
}
```

`review.rs` `apply_decision`: replace the `NeedsHuman` arm with

```rust
            Decision::NeedsHuman { kind, reason } => {
                let round = self.job()?.round;
                let parts = self.decision_parts(*kind, reason, round);
                self.park_with(*kind, reason.clone(), parts)
            }
```

and update its two `fix_or_park` calls (see below): the `FixRound` one becomes `self.fix_or_park(&findings, "review asked for changes", ParkKind::ChangesRemain, "review reply")`, the final-checks one becomes `self.fix_or_park(&findings, "final checks failed", ParkKind::ChecksFailed, "checks")`. Add `ParkKind` to the `factory_core` import.

`checks_stage.rs`: change the signature and the park line, and the one other call.

```rust
    pub(crate) fn fix_or_park(&self, findings: &str, reason: &str, kind: ParkKind, source: &str) -> Result<StageOutcome, EngineError> {
        let job = self.job()?;
        let used = job.round.saturating_sub(1);
        if used >= self.config.fix_rounds {
            let title = if kind == ParkKind::ChecksFailed { "Failing checks" } else { "Reviewer findings" };
            let part = ParkPart::new(title, source, findings);
            return self.park_with(kind, format!("{reason} after {used} fix rounds"), vec![part]);
        }
```

and in `checks_stage` the call becomes `self.fix_or_park(&findings, "checks failed", ParkKind::ChecksFailed, "checks")`. Import `ParkKind, ParkPart` from `factory_core`.

`merge.rs` `park()`: write a record of kind `other` and set the board fields inside the same update.

```rust
pub fn park(driver: &Driver, id: &str, reason: &str) -> Result<(), EngineError> {
    let now = now_secs();
    let at = now.max(0.0) as u64;
    let job = driver.store().update_job(id, |job| {
        job.park_kind = Some(ParkKind::Other.as_str().to_string());
        job.park_at = Some(at);
        job.set_stage(Stage::NeedsHuman, Some(reason), false, now)?;
        Ok(job.clone())
    })?;
    // Writing the record is best effort, the park already happened.
    let record = ParkRecord::new(ParkKind::Other, reason, id, job.round, at, Vec::new());
    let _ = record.write(&driver.job_dir(&job));
```

(keep the existing ledger lines after it) and add `ParkKind, ParkRecord` to the `factory_core` import there.

- [ ] **Step 4: Run the tests to verify they pass**

Run, one per call: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test review_decide`, then `-p factory-engine --test runloop`, then `-p factory-engine` (all), and `-p factory-cli` (it uses the engine). Expected: all pass, including `a_parked_job_gets_a_park_record_with_evidence`. If an existing test asserted the exact text of a reason, it must still pass because the reason strings are unchanged.

- [ ] **Step 5: Commit**

Commit the files of this task with the skill message `feat(factory): park with evidence from the decision and review steps`.

---

### Task 3: Builder, commit and diff-cap park sites

**Files:**
- Modify: `tools/factory/crates/factory-engine/src/build.rs`
- Modify: `tools/factory/crates/factory-engine/src/park.rs` (one helper)
- Modify: `tools/factory/crates/factory-engine/tests/build_checks.rs`

**Interfaces:**
- Consumes: `park_with`, `part_from_file`, `ParkKind` from Tasks 1 and 2, `AgentRun.result_text` and `.stop_reason` from `factory-agent`.
- Produces: `fn builder_stop_parts(run: &AgentRun) -> Vec<ParkPart>` in `park.rs` (`pub(crate)`).

- [ ] **Step 1: Write the failing tests**

Add to the tests module in `park.rs`:

```rust
    #[test]
    fn builder_stop_parts_carry_the_stop_reason_and_the_last_message() {
        let mut run = factory_agent::AgentRun::failed("boom");
        run.result_text = "I could not find the file.".to_string();
        run.stop_reason = factory_agent::StopReason::TurnLimit;
        let parts = builder_stop_parts(&run);
        assert_eq!(parts.len(), 1);
        assert!(parts[0].text.contains("turn-limit"), "{}", parts[0].text);
        assert!(parts[0].text.contains("I could not find the file."), "{}", parts[0].text);
    }

    #[test]
    fn builder_stop_parts_without_a_message_still_name_the_reason() {
        let run = factory_agent::AgentRun::failed("boom");
        let parts = builder_stop_parts(&run);
        assert!(parts[0].text.contains("failed"), "{}", parts[0].text);
        assert!(parts[0].text.contains("boom"), "{}", parts[0].text);
    }
```

Then extend two existing tests in `tools/factory/crates/factory-engine/tests/build_checks.rs` (they already drive these parks):

- In `a_task_that_changes_nothing_parks_the_job_with_its_number`, after the `match` block, add:

```rust
    let rec = factory_core::ParkRecord::read(&fx.job_dir).expect("park.json");
    assert_eq!(rec.kind, factory_core::ParkKind::NothingChanged);
    assert!(rec.reason.contains("task 2 changed nothing"), "{}", rec.reason);
    assert_eq!(fx.job().park_kind.as_deref(), Some("nothing-changed"));
    assert!(!rec.next.is_empty());
```

- In `a_stopped_first_task_parks_and_a_rework_resumes_at_that_task_with_the_note`, right after the first `match` block (before `assert_eq!(fx.job().task_index, 0);`), add:

```rust
    let rec = factory_core::ParkRecord::read(&fx.job_dir).expect("park.json");
    assert_eq!(rec.kind, factory_core::ParkKind::BuilderStopped);
    assert!(rec.parts[0].text.contains("Stop reason:"), "{}", rec.parts[0].text);
    assert_eq!(rec.next, vec!["factory rework 001 <note>".to_string()]);
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine builder_stop_parts`
Expected: compile error, `builder_stop_parts` not found.

- [ ] **Step 3: Implement**

In `park.rs` add:

```rust
/// Evidence for a builder that stopped: the stop reason, the error if any, and its last message.
pub(crate) fn builder_stop_parts(run: &factory_agent::AgentRun) -> Vec<ParkPart> {
    let mut text = format!("Stop reason: {}", run.stop_reason.as_str());
    if let Some(error) = &run.error {
        text.push_str(&format!("\nError: {error}"));
    }
    if !run.result_text.trim().is_empty() {
        text.push_str(&format!("\n\nLast message from the builder:\n{}", run.result_text));
    }
    vec![ParkPart::new("Builder stopped", "builder", &text)]
}
```

In `build.rs` replace each `self.park(...)` with `park_with` (keep every reason string exactly as it is now):

- `builder stopped: ...` (two places, whole plan and task): `return self.park_with(ParkKind::BuilderStopped, <the same format!(..)>, builder_stop_parts(&run));`
- `builder committed on its own ...` (two places): `return self.park_with(ParkKind::BuilderCommitted, <the same format!(..)>, Vec::new());`
- `builder made no changes` and `... on resume`: `return self.park_with(ParkKind::NothingChanged, why.to_string(), Vec::new());`
- `task N changed nothing`: `return self.park_with(ParkKind::NothingChanged, format!("task {} changed nothing", task.n), Vec::new());`
- `CommitOutcome::Park(reason)` (two places): `return self.park_with(ParkKind::CommitFailed, reason, Vec::new());`
- the diff cap in `finish_build`: `return self.park_with(ParkKind::DiffTooLarge, format!("diff has {} changed lines, the cap is {cap}", info.changed_lines()), self.decision_parts(ParkKind::DiffTooLarge, &reason_text, job.round));` where `reason_text` is the same formatted string bound to a variable first.

Import `ParkKind` from `factory_core` and `builder_stop_parts` from `crate::park` at the top of `build.rs`.

For `builder-committed` the reason already holds the `<sha7>` that `git reset --soft` needs, so no extra part is added.

- [ ] **Step 4: Run the tests to verify they pass**

Run, one per call: `-p factory-engine` (all), `-p factory-cli`, then `cargo check -j4 --manifest-path tools/factory/Cargo.toml`. Expected: all pass, no new warnings.

- [ ] **Step 5: Commit**

Commit the files of this task with the skill message `feat(factory): park the builder and commit failures with evidence`.

Next plan: docs/superpowers/plans/2026-10-16-factory-failure-details/plan-02-status-and-server.md
