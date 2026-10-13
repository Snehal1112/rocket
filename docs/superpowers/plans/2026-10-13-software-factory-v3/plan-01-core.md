# Software Factory v3, Plan 01: Workspace and factory-core

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task, executed by a `factory-builder` subagent. Skip that skill's final whole-branch review, its ledger scripts and its progress file (the driver owns the ledger). Steps use checkbox (`- [ ]`) syntax. This is the first plan of the series, so no earlier plan must be finished first.

**Goal:** Create the `tools/factory/` Rust workspace with all six member crates and fill in `factory-core`: board and job model, stage guard, ledger, config, risk classification, verdict parsing, budget math, atomic writes and job ids.

**Architecture:** `factory-core` is a pure library with no git, process or network access. Everything is plain serde types and small pure functions, so later crates (`factory-git`, `factory-agent`, `factory-checks`, `factory-engine`, `factory-cli`) depend on it and nothing in it depends on them. The board JSON stays byte-compatible in shape with the v1 `board.json` that `tools/factory-server/` reads.

**Tech Stack:** Rust 2021, serde 1, serde_json 1, toml 0.8, thiserror 1, tempfile 3 (dev only). Workspace also declares clap 4, git2 0.19 and fs2 0.4 for later plans.

**Spec:** `docs/superpowers/specs/2026-10-13-software-factory-v3-rust-driver-design.md` (decisions 6, 7, 9, 12, 13 and the crate layout table). Python reference to port: `.claude/factory/factory-ctl`. Server model to stay compatible with: `tools/factory-server/src/model.rs`.

## Global Constraints

- Own workspace at `tools/factory/`, own target dir, NOT added to the root `Cargo.toml` workspace (like `tools/factory-server/`, which ends its manifest with an empty `[workspace]` table).
- Pinned versions: serde 1 (derive), serde_json 1, toml 0.8, thiserror 1, clap 4 (derive), git2 0.19, tempfile 3 (dev), fs2 0.4.
- Never `.unwrap()` anywhere (a harness hook blocks it). `.expect("...")` only inside tests. Production code returns `Result<_, CoreError>`.
- Commands: `cargo check -j4 --manifest-path tools/factory/Cargo.toml` and `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-core <test_name>`. Never `--workspace`, never omit `-j4`, never set `CARGO_TARGET_DIR`.
- Comments are short full sentences ending in a period. No emojis.
- `board.json` keeps the v1 stage names (`spec`, `build`, `review`, `approve`, `merged`, `needs-human`) and v1 field names. Existing fields serialize as `null` when empty (the real board has `"reason": null`); only new v3 fields may be skipped when empty.
- Ledger lines use the strict form `Task N: dispatched | implementer done | complete` plus `Ruling:` lines, append only.
- Commit steps: pathspec commit only, `git add <paths> && git commit --only -m "..." -- <paths>`, conventional message. Invoke skill `dev-workflow-skills:1-git-commit` for the message but run its git commands inline, no helper agents. Never `git add -A` or `git add .`. If `Cargo.lock` is git-ignored (check with `git check-ignore tools/factory/Cargo.lock`), do not add it.

## Review Focus

Failure modes the spec implies but a happy-path test would miss, most likely first:

1. A corrupt or truncated `board.json` must return an error from `Board::load` and never be treated as an empty board (the next save would erase every job). A missing file is the only case that yields an empty board. Pinned in Task 1.
2. A typo in a `config.toml` key, an invalid TOML file or a missing file must fail closed: every diff is risky. Pinned in Task 2.
3. Path spelling tricks: `./src-tauri/x`, `SRC-TAURI/x`, `Cargo.TOML`, and `docs/../src-tauri/x` must classify as risky, never doc-only. Pinned in Task 2.
4. Reviewer output spelling variants: CRLF line endings, trailing spaces, markdown bold, a verdict followed by prose, `VERDICT: approve` in lowercase. Only an exact last non-empty line counts. Pinned in Task 3.
5. Budget edge cases: a zero budget, `u64::MAX` token counts (no overflow panic), exactly 80 percent and exactly 100 percent. Also job ids rolling past 999 and stray directory or branch names that must not break `next_job_id`. Pinned in Tasks 1 and 3.

Known limit, not tested here: risk classification is path-only. Content triggers (`{{process.env}}`, `DomainEvent` use, CSP strings) are covered by the reviewer prompt in plan 06, not by this crate.

## File Structure

```
tools/factory/Cargo.toml                       workspace manifest
tools/factory/.gitignore                       target/
tools/factory/crates/factory-core/             Cargo.toml, src/{lib,error,stage,board,ids,ledger,fsutil,config,risk,verdict,budget}.rs
tools/factory/crates/factory-core/tests/       board_roundtrip.rs, fixtures/board.json
tools/factory/crates/factory-{git,agent,checks,engine}/   Cargo.toml + src/lib.rs (empty, filled by later plans)
tools/factory/crates/factory-cli/              Cargo.toml + src/main.rs (empty bin named `factory`)
.claude/factory/config.toml                    shipped default config
```

---

### Task 1: Workspace scaffold, Stage, Job, Board, Ledger, write_atomic, ids

**Files:**
- Create: `tools/factory/Cargo.toml`, `tools/factory/.gitignore`
- Create: `tools/factory/crates/factory-core/Cargo.toml`
- Create: `tools/factory/crates/factory-core/src/{lib,error,stage,board,ids,ledger,fsutil}.rs`
- Create: `tools/factory/crates/factory-core/tests/board_roundtrip.rs`, `tools/factory/crates/factory-core/tests/fixtures/board.json`
- Create: `tools/factory/crates/factory-{git,agent,checks,engine}/Cargo.toml` and `src/lib.rs`
- Create: `tools/factory/crates/factory-cli/Cargo.toml` and `src/main.rs`

**Interfaces:**
- Consumes: nothing.
- Produces (all re-exported from `factory_core`):
  - `enum CoreError` (thiserror): `Io { path: String, source: std::io::Error }`, `Json(serde_json::Error)`, `BadId(String)`, `NoJob(String)`, `StageMove { from: Stage, to: Stage }`, `BaseChange { have: String, want: String }`, `LedgerLine`, `Config(String)`.
  - `enum Stage { Spec, Build, Review, Approve, Merged, NeedsHuman }` (serde kebab-case, `Copy`, `Display`, `as_str()`).
  - `enum ReviewState { Pending, Running, Pass, Changes, Skipped }` (serde lowercase).
  - `struct Job` with public fields `id, dir, slug, created, feature, branch: String`, `worktree, builder_id, base, head, reason: Option<String>`, `stage: Stage`, `round: u32`, `stage_since: f64`, `reviews: BTreeMap<String, ReviewState>`, `model: Option<String>` (new in v3, skipped when `None`). `Job::new(id: &str, dir: &str, slug: &str, created: &str, feature: &str, now: f64) -> Job`, `Job::set_review(&mut self, role: &str, state: ReviewState)`, `Job::reset_reviews(&mut self)`.
  - `struct Board { pub jobs: Vec<Job> }` with `Board::load(path: &Path) -> Result<Board, CoreError>`, `Board::save(&self, path: &Path) -> Result<(), CoreError>`, `Board::find(&self, id: &str) -> Result<&Job, CoreError>`, `Board::find_mut(&mut self, id: &str) -> Result<&mut Job, CoreError>`.
  - `enum TaskState { Dispatched, ImplementerDone, Complete }`, `struct Ledger` with `Ledger::new(path: impl Into<PathBuf>) -> Ledger`, `Ledger::create(&self, header: &str) -> Result<(), CoreError>`, `Ledger::task(&self, n: u32, state: TaskState, detail: Option<&str>) -> Result<(), CoreError>`, `Ledger::ruling(&self, text: &str) -> Result<(), CoreError>`, `Ledger::tasks(&self) -> Result<Vec<(u32, TaskState)>, CoreError>`.
  - `fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), CoreError>`.
  - `fn normalize_id(raw: &str) -> Result<String, CoreError>`, `fn slugify(text: &str) -> String`, `fn job_dir_name(date: &str, id: &str, slug: &str) -> String`, `fn next_job_id(board_ids: &[String], dir_names: &[String], branch_names: &[String]) -> String`.

- [ ] **Step 1: Create the workspace manifest and the empty member crates**

`tools/factory/Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = [
    "crates/factory-core",
    "crates/factory-git",
    "crates/factory-agent",
    "crates/factory-checks",
    "crates/factory-engine",
    "crates/factory-cli",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
thiserror = "1"
clap = { version = "4", features = ["derive"] }
git2 = "0.19"
fs2 = "0.4"
tempfile = "3"
```

`tools/factory/.gitignore`: one line, `target/`.

For each of `factory-git`, `factory-agent`, `factory-checks`, `factory-engine` write `crates/<name>/Cargo.toml`:

```toml
[package]
name = "<name>"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
```

and `crates/<name>/src/lib.rs` containing one line: `//! Filled in by a later plan of the v3 series.`

`factory-cli`: same manifest plus

```toml
[[bin]]
name = "factory"
path = "src/main.rs"
```

and `src/main.rs`:

```rust
//! The `factory` command line. Filled in by plan 05.
fn main() {}
```

`factory-core/Cargo.toml`:

```toml
[package]
name = "factory-core"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
toml.workspace = true
thiserror.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

- [ ] **Step 2: Write the error type and the module skeleton**

`src/error.rs`:

```rust
use crate::stage::Stage;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("io error on {path}: {source}")]
    Io { path: String, source: std::io::Error },
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("job id must be digits, got {0:?}")]
    BadId(String),
    #[error("no job with id {0}")]
    NoJob(String),
    #[error("illegal stage move {from} -> {to}")]
    StageMove { from: Stage, to: Stage },
    #[error("job already has base {have}, refusing to change it to {want}")]
    BaseChange { have: String, want: String },
    #[error("ledger text must be one non-empty line")]
    LedgerLine,
    #[error("config error: {0}")]
    Config(String),
}

impl CoreError {
    pub fn io(path: &std::path::Path, source: std::io::Error) -> CoreError {
        CoreError::Io { path: path.display().to_string(), source }
    }
}
```

`src/lib.rs` (modules added in later tasks are added when their task starts):

```rust
//! Board model, ledger, config, risk, verdict and budget rules for the factory driver.
pub mod board;
pub mod error;
pub mod fsutil;
pub mod ids;
pub mod ledger;
pub mod stage;

pub use board::{Board, Job, ReviewState};
pub use error::CoreError;
pub use fsutil::write_atomic;
pub use ids::{job_dir_name, next_job_id, normalize_id, slugify};
pub use ledger::{Ledger, TaskState};
pub use stage::Stage;
```

`src/stage.rs` (the guard is added in Task 3):

```rust
use serde::{Deserialize, Serialize};
use std::fmt;

/// The job stage. Serialized names match the v1 `board.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    Spec,
    Build,
    Review,
    Approve,
    Merged,
    NeedsHuman,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Spec => "spec",
            Stage::Build => "build",
            Stage::Review => "review",
            Stage::Approve => "approve",
            Stage::Merged => "merged",
            Stage::NeedsHuman => "needs-human",
        }
    }
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
```

- [ ] **Step 3: Write the failing tests for `write_atomic`, ids and the board round trip**

Copy the real board into the fixture: `cp /home/numericlabs/data/rocket/rocket/.superpowers/factory/board.json tools/factory/crates/factory-core/tests/fixtures/board.json` (create the directory first). It holds jobs 001 and 002, both `merged`.

`tests/board_roundtrip.rs`:

```rust
use factory_core::{Board, ReviewState, Stage};
use std::path::Path;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/board.json");

#[test]
fn real_board_round_trips_without_losing_fields() {
    let text = std::fs::read_to_string(FIXTURE).expect("fixture readable");
    let original: serde_json::Value = serde_json::from_str(&text).expect("fixture is json");
    let board = Board::load(Path::new(FIXTURE)).expect("board loads");
    assert_eq!(board.jobs.len(), 2);
    assert_eq!(board.jobs[0].stage, Stage::Merged);
    assert_eq!(board.jobs[0].reviews["ux"], ReviewState::Skipped);
    let again = serde_json::to_value(&board).expect("board serializes");
    assert_eq!(again, original);
}

#[test]
fn save_then_load_is_identical() {
    let board = Board::load(Path::new(FIXTURE)).expect("board loads");
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("sub").join("board.json");
    board.save(&path).expect("save");
    let back = Board::load(&path).expect("reload");
    assert_eq!(back, board);
    assert!(std::fs::read_to_string(&path).expect("read").ends_with('\n'));
}

#[test]
fn missing_file_is_an_empty_board() {
    let dir = tempfile::tempdir().expect("tempdir");
    let board = Board::load(&dir.path().join("none.json")).expect("missing is ok");
    assert!(board.jobs.is_empty());
}

#[test]
fn corrupt_file_is_an_error_not_an_empty_board() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("board.json");
    std::fs::write(&path, "{\"jobs\": [{\"id\": ").expect("write");
    assert!(Board::load(&path).is_err());
    std::fs::write(&path, r#"{"jobs":[{"id":"001","stage":"flying"}]}"#).expect("write");
    assert!(Board::load(&path).is_err());
}
```

In `src/fsutil.rs` add a `#[cfg(test)] mod tests` with:

```rust
use super::*;

#[test]
fn creates_parents_replaces_content_and_leaves_no_temp_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("a").join("b.txt");
    write_atomic(&path, b"one").expect("first write");
    write_atomic(&path, b"two").expect("second write");
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "two");
    let names: Vec<_> = std::fs::read_dir(path.parent().expect("parent"))
        .expect("list")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["b.txt".to_string()]);
}
```

In `src/ids.rs` add tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|i| i.to_string()).collect()
    }

    #[test]
    fn normalize_pads_and_rejects_non_digits() {
        assert_eq!(normalize_id("7").expect("ok"), "007");
        assert_eq!(normalize_id("1000").expect("ok"), "1000");
        assert!(normalize_id("x1").is_err());
        assert!(normalize_id("").is_err());
        assert!(normalize_id("99999999999999999999").is_err());
    }

    #[test]
    fn slugify_matches_the_python_reference() {
        let feature = "GitHub issue #20 (https://github.com/Snehal1112/rocket/issues/20)";
        assert_eq!(slugify(feature), "github-issue-20-https-github-com-snehal1");
        assert_eq!(slugify("  !!  "), "job");
        assert_eq!(slugify("Ünï cödé"), "n-c-d");
    }

    #[test]
    fn dir_name_has_date_id_slug() {
        assert_eq!(job_dir_name("2026-10-13", "001", "x"), "2026-10-13-001-x");
    }

    #[test]
    fn next_id_takes_the_max_of_all_three_sources() {
        assert_eq!(next_job_id(&[], &[], &[]), "001");
        let next = next_job_id(
            &s(&["001", "002"]),
            &s(&["2026-10-13-004-foo", "notes", "2026-10-13-oops"]),
            &s(&["factory/003-bar", "main", "factory/x-y"]),
        );
        assert_eq!(next, "005");
        assert_eq!(next_job_id(&s(&["999"]), &[], &[]), "1000");
    }
}
```

In `src/ledger.rs` add tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn ledger() -> (tempfile::TempDir, Ledger) {
        let dir = tempfile::tempdir().expect("tempdir");
        let ledger = Ledger::new(dir.path().join("progress.md"));
        (dir, ledger)
    }

    #[test]
    fn writes_strict_lines_and_reads_them_back() {
        let (_dir, l) = ledger();
        l.create("# SDD ledger").expect("create");
        l.task(1, TaskState::Dispatched, None).expect("t1");
        l.task(1, TaskState::ImplementerDone, None).expect("t1b");
        l.task(1, TaskState::Complete, Some("commits a..b")).expect("t1c");
        l.ruling("keep the cap at 4000 lines").expect("ruling");
        let text = std::fs::read_to_string(&l.path).expect("read");
        assert!(text.contains("\nTask 1: dispatched\n"));
        assert!(text.contains("\nTask 1: implementer done\n"));
        assert!(text.contains("\nTask 1: complete (commits a..b)\n"));
        assert!(text.contains("\nRuling: keep the cap at 4000 lines\n"));
        let tasks = l.tasks().expect("parse");
        assert_eq!(tasks.last(), Some(&(1, TaskState::Complete)));
        assert_eq!(tasks.len(), 3);
    }

    #[test]
    fn create_never_truncates_and_appends_survive() {
        let (_dir, l) = ledger();
        l.create("# head").expect("create");
        l.task(2, TaskState::Complete, None).expect("t2");
        l.create("# other head").expect("second create is a no-op");
        let text = std::fs::read_to_string(&l.path).expect("read");
        assert!(text.starts_with("# head\n"));
        assert!(text.contains("Task 2: complete"));
    }

    #[test]
    fn rejects_multiline_and_empty_text() {
        let (_dir, l) = ledger();
        assert!(l.ruling("a\nTask 9: complete").is_err());
        assert!(l.ruling("   ").is_err());
        assert!(l.task(1, TaskState::Complete, Some("x\ny")).is_err());
    }

    #[test]
    fn missing_ledger_has_no_tasks_and_loose_lines_are_ignored() {
        let (_dir, l) = ledger();
        assert!(l.tasks().expect("missing ok").is_empty());
        std::fs::write(&l.path, "Task 3: complete\ntask 4: complete\nTask 5: completed\nTask x: complete\n").expect("w");
        assert_eq!(l.tasks().expect("parse"), vec![(3, TaskState::Complete)]);
    }
}
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-core`
Expected: compile errors (`Board`, `write_atomic`, `Ledger` and friends not defined yet).

- [ ] **Step 5: Implement `fsutil`, `ids`, `board` and `ledger`**

`src/fsutil.rs` (put above the test module):

```rust
use crate::error::CoreError;
use std::io::Write;
use std::path::Path;

/// Writes the file by creating a sibling temp file and renaming it over the target.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    std::fs::create_dir_all(dir).map_err(|e| CoreError::io(dir, e))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{}.{}.tmp", name, std::process::id()));
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(CoreError::io(path, e));
    }
    Ok(())
}
```

`src/ids.rs` (above the tests):

```rust
use crate::error::CoreError;

/// Pads a numeric job id to at least three digits.
pub fn normalize_id(raw: &str) -> Result<String, CoreError> {
    if raw.is_empty() || !raw.chars().all(|c| c.is_ascii_digit()) {
        return Err(CoreError::BadId(raw.to_string()));
    }
    let n: u32 = raw.parse().map_err(|_| CoreError::BadId(raw.to_string()))?;
    Ok(format!("{n:03}"))
}

/// Lowercases, joins alphanumeric runs with dashes and caps the result at 40 characters.
pub fn slugify(text: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in text.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            if gap && !out.is_empty() {
                out.push('-');
            }
            gap = false;
            out.push(c);
        } else {
            gap = true;
        }
    }
    let cut: String = out.chars().take(40).collect();
    let cut = cut.trim_matches('-').to_string();
    if cut.is_empty() {
        "job".to_string()
    } else {
        cut
    }
}

pub fn job_dir_name(date: &str, id: &str, slug: &str) -> String {
    format!("{date}-{id}-{slug}")
}

fn leading_number(text: &str) -> Option<u32> {
    let digits: String = text.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

/// Reads the id out of a `YYYY-MM-DD-NNN-slug` directory name.
fn id_from_dir(name: &str) -> Option<u32> {
    let mut parts = name.splitn(4, '-');
    let (y, m, d, rest) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    let date_ok = y.len() == 4 && m.len() == 2 && d.len() == 2;
    if !date_ok || !(y.chars().chain(m.chars()).chain(d.chars())).all(|c| c.is_ascii_digit()) {
        return None;
    }
    leading_number(rest)
}

/// Reads the id out of a `factory/NNN-slug` branch name.
fn id_from_branch(name: &str) -> Option<u32> {
    leading_number(name.strip_prefix("factory/")?)
}

/// The next id is one more than the largest id seen on the board, in job directories or in branches.
pub fn next_job_id(board_ids: &[String], dir_names: &[String], branch_names: &[String]) -> String {
    let from_board = board_ids.iter().filter_map(|i| leading_number(i));
    let from_dirs = dir_names.iter().filter_map(|d| id_from_dir(d));
    let from_branches = branch_names.iter().filter_map(|b| id_from_branch(b));
    let max = from_board.chain(from_dirs).chain(from_branches).max().unwrap_or(0);
    format!("{:03}", max.saturating_add(1))
}
```

`src/board.rs`:

```rust
use crate::error::CoreError;
use crate::fsutil::write_atomic;
use crate::stage::Stage;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewState {
    Pending,
    Running,
    Pass,
    Changes,
    Skipped,
}

/// One job in `board.json`. Field names match the v1 file so `factory-server` keeps working.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub head: Option<String>,
    pub stage: Stage,
    pub round: u32,
    #[serde(default)]
    pub reason: Option<String>,
    pub stage_since: f64,
    pub reviews: BTreeMap<String, ReviewState>,
    /// Model override for this job. New in v3, absent in v1 boards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl Job {
    /// A fresh job in the `spec` stage. v3 has one reviewer, so only the `code` role exists.
    pub fn new(id: &str, dir: &str, slug: &str, created: &str, feature: &str, now: f64) -> Job {
        let mut reviews = BTreeMap::new();
        reviews.insert("code".to_string(), ReviewState::Pending);
        Job {
            id: id.to_string(),
            dir: dir.to_string(),
            slug: slug.to_string(),
            created: created.to_string(),
            feature: feature.to_string(),
            branch: format!("factory/{id}-{slug}"),
            worktree: None,
            builder_id: None,
            base: None,
            head: None,
            stage: Stage::Spec,
            round: 0,
            reason: None,
            stage_since: now,
            reviews,
            model: None,
        }
    }

    pub fn set_review(&mut self, role: &str, state: ReviewState) {
        self.reviews.insert(role.to_string(), state);
    }

    pub fn reset_reviews(&mut self) {
        for state in self.reviews.values_mut() {
            *state = ReviewState::Pending;
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Board {
    #[serde(default)]
    pub jobs: Vec<Job>,
}

impl Board {
    /// A missing file is an empty board. Any other failure is an error, so a corrupt file is never overwritten by accident.
    pub fn load(path: &Path) -> Result<Board, CoreError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(serde_json::from_str(&text)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Board::default()),
            Err(e) => Err(CoreError::io(path, e)),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        write_atomic(path, text.as_bytes())
    }

    pub fn find(&self, id: &str) -> Result<&Job, CoreError> {
        let id = crate::ids::normalize_id(id)?;
        self.jobs.iter().find(|j| j.id == id).ok_or(CoreError::NoJob(id))
    }

    pub fn find_mut(&mut self, id: &str) -> Result<&mut Job, CoreError> {
        let id = crate::ids::normalize_id(id)?;
        self.jobs.iter_mut().find(|j| j.id == id).ok_or(CoreError::NoJob(id))
    }
}
```

`src/ledger.rs` (above the tests):

```rust
use crate::error::CoreError;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Dispatched,
    ImplementerDone,
    Complete,
}

impl TaskState {
    fn label(self) -> &'static str {
        match self {
            TaskState::Dispatched => "dispatched",
            TaskState::ImplementerDone => "implementer done",
            TaskState::Complete => "complete",
        }
    }
}

/// An append-only `progress.md`. Only the driver writes it.
#[derive(Debug, Clone)]
pub struct Ledger {
    pub path: PathBuf,
}

fn single_line(text: &str) -> Result<&str, CoreError> {
    let t = text.trim();
    if t.is_empty() || t.contains('\n') || t.contains('\r') {
        return Err(CoreError::LedgerLine);
    }
    Ok(t)
}

impl Ledger {
    pub fn new(path: impl Into<PathBuf>) -> Ledger {
        Ledger { path: path.into() }
    }

    /// Writes the header only when the file does not exist yet.
    pub fn create(&self, header: &str) -> Result<(), CoreError> {
        if self.path.exists() {
            return Ok(());
        }
        let header = single_line(header)?;
        self.append(&format!("{header}\n"))
    }

    pub fn task(&self, n: u32, state: TaskState, detail: Option<&str>) -> Result<(), CoreError> {
        let mut line = format!("Task {n}: {}", state.label());
        if let Some(d) = detail {
            line.push_str(&format!(" ({})", single_line(d)?));
        }
        self.append(&format!("{line}\n"))
    }

    pub fn ruling(&self, text: &str) -> Result<(), CoreError> {
        self.append(&format!("Ruling: {}\n", single_line(text)?))
    }

    /// Parses only the strict `Task N: <state>` lines, in file order. Other lines are ignored.
    pub fn tasks(&self) -> Result<Vec<(u32, TaskState)>, CoreError> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(CoreError::io(&self.path, e)),
        };
        Ok(text.lines().filter_map(parse_task_line).collect())
    }

    fn append(&self, text: &str) -> Result<(), CoreError> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| CoreError::io(dir, e))?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| CoreError::io(&self.path, e))?;
        file.write_all(text.as_bytes()).map_err(|e| CoreError::io(&self.path, e))
    }
}

fn parse_task_line(line: &str) -> Option<(u32, TaskState)> {
    let rest = line.strip_prefix("Task ")?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    let n: u32 = digits.parse().ok()?;
    let after = rest[digits.len()..].strip_prefix(": ")?;
    for state in [TaskState::ImplementerDone, TaskState::Dispatched, TaskState::Complete] {
        if let Some(tail) = after.strip_prefix(state.label()) {
            if tail.is_empty() || tail.starts_with(' ') {
                return Some((n, state));
            }
        }
    }
    None
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-core`
Expected: all tests in `board_roundtrip`, `fsutil`, `ids` and `ledger` pass. Then run `cargo check -j4 --manifest-path tools/factory/Cargo.toml` and expect the whole workspace (all six crates) to build with no warnings from `factory-core`.

- [ ] **Step 7: Commit**

Invoke `dev-workflow-skills:1-git-commit` for the message, run its git commands inline.

```bash
git add tools/factory/Cargo.toml tools/factory/.gitignore tools/factory/crates
git commit --only -m "feat(factory): add v3 workspace and core board, ledger and ids" -- tools/factory/Cargo.toml tools/factory/.gitignore tools/factory/crates
```

---

### Task 2: Config, classify_paths and the shipped config.toml

**Files:**
- Create: `tools/factory/crates/factory-core/src/{config,risk}.rs`
- Create: `.claude/factory/config.toml`
- Modify: `tools/factory/crates/factory-core/src/lib.rs` (add modules and re-exports)

**Interfaces:**
- Consumes: `CoreError` from Task 1.
- Produces:
  - `struct CheckRule { pub paths: Vec<String>, pub commands: Vec<String> }`. In `commands`, the literal `{crate}` is a placeholder that plan 04 replaces with the second path segment of a matching `crates/<name>/...` path. `CheckRule::matches(&self, path: &str) -> bool`.
  - `struct Config { pub risk_paths: Vec<String>, pub diff_cap_lines: u32, pub budget_weighted: u64, pub fix_rounds: u32, pub stall_minutes: u32, pub builder_model: String, pub reviewer_model: String, pub checks: Vec<CheckRule>, pub degraded: bool }`. `degraded` is never read from the file; it is true only for a fail-closed config.
  - `Config::default() -> Config` (diff cap 4000, budget 3_000_000, 2 fix rounds, 10 stall minutes, both models `"sonnet"`, the default risk list, no checks), `Config::fail_closed() -> Config`, `Config::parse(text: &str) -> Result<Config, CoreError>`, `Config::try_load(path: &Path) -> Result<Config, CoreError>`, `Config::load(path: &Path) -> Config` (never fails; a missing or invalid file gives `fail_closed()`).
  - `enum DiffClass { DocOnly, Normal, Risky }` (`Debug, Clone, Copy, PartialEq, Eq`), `fn classify_paths<S: AsRef<str>>(paths: &[S], config: &Config) -> DiffClass`, `fn glob_match(pattern: &str, path: &str) -> bool`.

- [ ] **Step 1: Write the failing tests**

`src/risk.rs` test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn class(paths: &[&str]) -> DiffClass {
        classify_paths(paths, &Config::default())
    }

    #[test]
    fn glob_basics() {
        assert!(glob_match("src-tauri/**", "src-tauri/src/lib.rs"));
        assert!(glob_match("**/Cargo.toml", "Cargo.toml"));
        assert!(glob_match("**/Cargo.toml", "crates/a/Cargo.toml"));
        assert!(glob_match("crates/*/**", "crates/rocket-http/src/lib.rs"));
        assert!(!glob_match("crates/*/**", "crates/lonely"));
        assert!(glob_match("tools/factory/**", "tools/factory/Cargo.toml"));
        assert!(!glob_match("tools/factory/**", "tools/factory-server/Cargo.toml"));
        assert!(glob_match("src/**/*.tsx", "src/a/b/C.tsx"));
        assert!(!glob_match("src/*.tsx", "src/a/C.tsx"));
        assert!(glob_match("a?c", "abc"));
    }

    #[test]
    fn doc_only_normal_and_risky() {
        assert_eq!(class(&["README.md", "docs/a.txt", ".claude/software-factory.md"]), DiffClass::DocOnly);
        assert_eq!(class(&["crates/rocket-http/src/executor.rs"]), DiffClass::Normal);
        assert_eq!(class(&["src/components/Foo.tsx", "docs/a.md"]), DiffClass::Normal);
        assert_eq!(class(&["src-tauri/src/lib.rs"]), DiffClass::Risky);
        assert_eq!(class(&["docs/a.md", "package.json"]), DiffClass::Risky);
        assert_eq!(class(&["tools/factory/crates/factory-core/Cargo.toml"]), DiffClass::Risky);
        assert_eq!(class(&[".claude/settings.json"]), DiffClass::Risky);
        assert_eq!(class(&[".claude/harness/hooks/no-unwrap.py"]), DiffClass::Risky);
        assert_eq!(class(&["src/lib/auth-utils.ts"]), DiffClass::Risky);
        assert_eq!(class(&["crates/rocket-infra/src/fs_collection.rs"]), DiffClass::Risky);
    }

    #[test]
    fn empty_diff_is_normal() {
        let none: [&str; 0] = [];
        assert_eq!(classify_paths(&none, &Config::default()), DiffClass::Normal);
    }

    #[test]
    fn spelling_tricks_are_still_risky() {
        assert_eq!(class(&["./src-tauri/src/lib.rs"]), DiffClass::Risky);
        assert_eq!(class(&["SRC-TAURI/src/lib.rs"]), DiffClass::Risky);
        assert_eq!(class(&["web/Cargo.TOML"]), DiffClass::Risky);
        assert_eq!(class(&["docs/../src-tauri/src/lib.rs"]), DiffClass::Risky);
        assert_eq!(class(&["docs/../outside.md"]), DiffClass::Risky);
    }

    #[test]
    fn a_degraded_config_makes_every_diff_risky() {
        let cfg = Config::fail_closed();
        assert_eq!(classify_paths(&["docs/a.md"], &cfg), DiffClass::Risky);
        assert_eq!(classify_paths(&["README.md"], &cfg), DiffClass::Risky);
    }
}
```

`src/config.rs` test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_gives_defaults_and_is_not_degraded() {
        let cfg = Config::parse("").expect("empty is valid");
        assert!(!cfg.degraded);
        assert_eq!(cfg.diff_cap_lines, 4000);
        assert_eq!(cfg.budget_weighted, 3_000_000);
        assert_eq!(cfg.fix_rounds, 2);
        assert_eq!(cfg.stall_minutes, 10);
        assert_eq!(cfg.builder_model, "sonnet");
        assert!(cfg.risk_paths.iter().any(|p| p == "src-tauri/**"));
    }

    #[test]
    fn keys_override_defaults_and_checks_parse() {
        let cfg = Config::parse(
            r#"
            diff_cap_lines = 100
            builder_model = "opus"
            risk_paths = ["x/**"]
            [[checks]]
            paths = ["src/**"]
            commands = ["yarn tsc --noEmit"]
            "#,
        )
        .expect("valid");
        assert_eq!(cfg.diff_cap_lines, 100);
        assert_eq!(cfg.builder_model, "opus");
        assert_eq!(cfg.risk_paths, vec!["x/**".to_string()]);
        assert_eq!(cfg.checks.len(), 1);
        assert!(cfg.checks[0].matches("src/a/b.ts"));
        assert!(!cfg.checks[0].matches("crates/a.rs"));
    }

    #[test]
    fn typos_and_bad_values_are_errors() {
        assert!(Config::parse("risk_path = [\"x\"]").is_err());
        assert!(Config::parse("diff_cap_lines = 0").is_err());
        assert!(Config::parse("budget_weighted = 0").is_err());
        assert!(Config::parse("builder_model = \"\"").is_err());
        assert!(Config::parse("this is = = not toml").is_err());
    }

    #[test]
    fn load_fails_closed_for_missing_and_invalid_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = Config::load(&dir.path().join("none.toml"));
        assert!(missing.degraded);
        let bad = dir.path().join("bad.toml");
        std::fs::write(&bad, "risk_path = 1").expect("write");
        assert!(Config::load(&bad).degraded);
        let good = dir.path().join("good.toml");
        std::fs::write(&good, "fix_rounds = 3").expect("write");
        let cfg = Config::load(&good);
        assert!(!cfg.degraded);
        assert_eq!(cfg.fix_rounds, 3);
    }

    #[test]
    fn shipped_config_loads_and_maps_the_expected_paths() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../.claude/factory/config.toml");
        let cfg = Config::try_load(std::path::Path::new(path)).expect("shipped config is valid");
        assert!(!cfg.degraded);
        let cmds = |p: &str| -> Vec<String> {
            cfg.checks.iter().filter(|c| c.matches(p)).flat_map(|c| c.commands.clone()).collect()
        };
        assert!(cmds("crates/rocket-http/src/lib.rs").contains(&"cargo check -j4 -p {crate}".to_string()));
        assert!(cmds("crates/rocket-http/src/lib.rs").contains(&"cargo test -j4 -p {crate}".to_string()));
        assert!(cmds("src/App.tsx").contains(&"yarn tsc --noEmit".to_string()));
        assert!(cmds("src/App.tsx").contains(&"yarn check".to_string()));
        assert!(cmds("src/App.tsx").contains(&"yarn test".to_string()));
        assert!(cmds("tools/factory/crates/factory-core/src/lib.rs")
            .contains(&"cargo check -j4 --manifest-path tools/factory/Cargo.toml".to_string()));
        assert!(cmds("tools/factory-server/src/main.rs")
            .contains(&"cargo check -j4 --manifest-path tools/factory-server/Cargo.toml".to_string()));
        assert!(cmds("docs/a.md").is_empty());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-core config`
Expected: compile error (`Config` not defined).

- [ ] **Step 3: Implement `risk.rs`**

Add `pub mod config; pub mod risk;` to `lib.rs` and re-export `Config, CheckRule` and `classify_paths, glob_match, DiffClass`.

```rust
use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffClass {
    DocOnly,
    Normal,
    Risky,
}

/// Matches a `/`-separated path against a glob. `*` and `?` stay inside one segment, `**` spans zero or more segments.
pub fn glob_match(pattern: &str, path: &str) -> bool {
    let p: Vec<&str> = pattern.split('/').collect();
    let s: Vec<&str> = path.split('/').collect();
    match_segments(&p, &s)
}

fn match_segments(p: &[&str], s: &[&str]) -> bool {
    match p.split_first() {
        None => s.is_empty(),
        Some((seg, rest)) if *seg == "**" => (0..=s.len()).any(|i| match_segments(rest, &s[i..])),
        Some((seg, rest)) => match s.split_first() {
            Some((first, tail)) => {
                let pc: Vec<char> = seg.chars().collect();
                let sc: Vec<char> = first.chars().collect();
                match_segment(&pc, &sc) && match_segments(rest, tail)
            }
            None => false,
        },
    }
}

fn match_segment(p: &[char], s: &[char]) -> bool {
    match p.split_first() {
        None => s.is_empty(),
        Some(('*', rest)) => (0..=s.len()).any(|i| match_segment(rest, &s[i..])),
        Some(('?', rest)) => !s.is_empty() && match_segment(rest, &s[1..]),
        Some((c, rest)) => s.first() == Some(c) && match_segment(rest, &s[1..]),
    }
}

fn normalize(path: &str) -> String {
    let mut p = path.trim();
    while let Some(rest) = p.strip_prefix("./") {
        p = rest;
    }
    p.to_string()
}

fn has_parent_segment(path: &str) -> bool {
    path.split('/').any(|seg| seg == "..")
}

fn is_doc_path(path: &str) -> bool {
    path.ends_with(".md") || path.starts_with("docs/") || path.starts_with(".claude/")
}

/// Risk is checked first and ignores case. A path with a `..` segment is treated as risky.
pub fn classify_paths<S: AsRef<str>>(paths: &[S], config: &Config) -> DiffClass {
    if paths.is_empty() {
        return DiffClass::Normal;
    }
    let patterns: Vec<String> = config.risk_paths.iter().map(|p| p.to_lowercase()).collect();
    let mut all_docs = true;
    for raw in paths {
        let path = normalize(raw.as_ref());
        let lower = path.to_lowercase();
        if has_parent_segment(&path) || patterns.iter().any(|p| glob_match(p, &lower)) {
            return DiffClass::Risky;
        }
        if !is_doc_path(&path) {
            all_docs = false;
        }
    }
    if all_docs {
        DiffClass::DocOnly
    } else {
        DiffClass::Normal
    }
}
```

- [ ] **Step 4: Implement `config.rs`**

```rust
use crate::error::CoreError;
use crate::risk::glob_match;
use serde::Deserialize;
use std::path::Path;

/// Path-based risk list from spec decision 6. Matching ignores case.
const DEFAULT_RISK: &[&str] = &[
    "src-tauri/**",
    "**/Cargo.toml",
    "**/Cargo.lock",
    "**/package.json",
    "**/tauri.conf.json",
    "**/*auth*",
    "**/*secret*",
    "**/*trust*",
    "**/*csp*",
    "crates/rocket-infra/**",
    "crates/rocket-shared/src/event*",
    "crates/rocket-shared/src/events/**",
    ".claude/settings.json",
    ".claude/settings.local.json",
    ".claude/harness/**",
    ".claude/factory/hooks/**",
    ".claude/factory/config.toml",
];

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckRule {
    pub paths: Vec<String>,
    pub commands: Vec<String>,
}

impl CheckRule {
    pub fn matches(&self, path: &str) -> bool {
        self.paths.iter().any(|p| glob_match(p, path))
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub risk_paths: Vec<String>,
    pub diff_cap_lines: u32,
    pub budget_weighted: u64,
    pub fix_rounds: u32,
    pub stall_minutes: u32,
    pub builder_model: String,
    pub reviewer_model: String,
    pub checks: Vec<CheckRule>,
    /// True only for a fail-closed config. Never read from the file.
    #[serde(skip)]
    pub degraded: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            risk_paths: DEFAULT_RISK.iter().map(|s| s.to_string()).collect(),
            diff_cap_lines: 4000,
            budget_weighted: 3_000_000,
            fix_rounds: 2,
            stall_minutes: 10,
            builder_model: "sonnet".to_string(),
            reviewer_model: "sonnet".to_string(),
            checks: Vec::new(),
            degraded: false,
        }
    }
}

impl Config {
    /// Every path matches `**`, so every diff is risky and nothing auto-merges.
    pub fn fail_closed() -> Config {
        Config { risk_paths: vec!["**".to_string()], degraded: true, ..Config::default() }
    }

    pub fn parse(text: &str) -> Result<Config, CoreError> {
        let cfg: Config = toml::from_str(text).map_err(|e| CoreError::Config(e.to_string()))?;
        if cfg.diff_cap_lines == 0 || cfg.budget_weighted == 0 {
            return Err(CoreError::Config("diff_cap_lines and budget_weighted must be above zero".into()));
        }
        if cfg.builder_model.trim().is_empty() || cfg.reviewer_model.trim().is_empty() {
            return Err(CoreError::Config("model names must not be empty".into()));
        }
        Ok(cfg)
    }

    pub fn try_load(path: &Path) -> Result<Config, CoreError> {
        let text = std::fs::read_to_string(path).map_err(|e| CoreError::io(path, e))?;
        Config::parse(&text)
    }

    /// Never fails. A missing or invalid file yields the fail-closed config.
    pub fn load(path: &Path) -> Config {
        match Config::try_load(path) {
            Ok(cfg) => cfg,
            Err(_) => Config::fail_closed(),
        }
    }
}
```

- [ ] **Step 5: Write the shipped config**

`.claude/factory/config.toml`:

```toml
# Factory driver config. A missing or invalid file makes every diff risky.
# Globs match the whole repo-relative path, ignoring case for risk_paths.
# In check commands, {crate} is replaced with the name from crates/<name>/.

diff_cap_lines = 4000
budget_weighted = 3000000
fix_rounds = 2
stall_minutes = 10
builder_model = "sonnet"
reviewer_model = "sonnet"

# Diffs touching these paths stop for the human after review.
risk_paths = [
  "src-tauri/**",
  "**/Cargo.toml",
  "**/Cargo.lock",
  "**/package.json",
  "**/tauri.conf.json",
  "**/*auth*",
  "**/*secret*",
  "**/*trust*",
  "**/*csp*",
  "crates/rocket-infra/**",
  "crates/rocket-shared/src/event*",
  "crates/rocket-shared/src/events/**",
  ".claude/settings.json",
  ".claude/settings.local.json",
  ".claude/harness/**",
  ".claude/factory/hooks/**",
  ".claude/factory/config.toml",
]

[[checks]]
paths = ["crates/*/**"]
commands = ["cargo check -j4 -p {crate}", "cargo test -j4 -p {crate}"]

[[checks]]
paths = ["src-tauri/**"]
commands = ["cargo check -j4"]

[[checks]]
paths = ["src/**"]
commands = ["yarn tsc --noEmit", "yarn check", "yarn test"]

[[checks]]
paths = ["tools/factory/**"]
commands = ["cargo check -j4 --manifest-path tools/factory/Cargo.toml"]

[[checks]]
paths = ["tools/factory-server/**"]
commands = ["cargo check -j4 --manifest-path tools/factory-server/Cargo.toml"]
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-core`
Expected: all tests in `risk` and `config` pass, plus Task 1 tests still pass.

- [ ] **Step 7: Commit**

```bash
git add tools/factory/crates/factory-core/src .claude/factory/config.toml
git commit --only -m "feat(factory): add config, risk classification and default config.toml" -- tools/factory/crates/factory-core/src .claude/factory/config.toml
```

---

### Task 3: Verdict, budget, stage transition guard and the final run

**Files:**
- Create: `tools/factory/crates/factory-core/src/{verdict,budget}.rs`
- Modify: `tools/factory/crates/factory-core/src/{lib,stage,board}.rs`

**Interfaces:**
- Consumes: `Stage`, `Job`, `CoreError` from Task 1.
- Produces:
  - `enum Verdict { Approve, Changes, Missing }`, `fn parse_verdict(text: &str) -> Verdict`.
  - `enum BudgetState { Ok, Warn, Stop }`, `fn weighted_tokens(input: u64, cache_creation: u64, output: u64, cache_read: u64) -> u64`, `fn budget_state(used: u64, budget: u64) -> BudgetState`.
  - `Stage::check_move(from: Stage, to: Stage, force: bool) -> Result<(), CoreError>`.
  - `Job::set_stage(&mut self, to: Stage, reason: Option<&str>, force: bool, now: f64) -> Result<(), CoreError>` and `Job::set_base(&mut self, sha: &str, force: bool) -> Result<(), CoreError>`.
  - Convention for plan 04: a fix round (`review` back to `build`) is the one legitimate backward move. The engine passes `force = true` for it and writes a `Ruling:` line.

- [ ] **Step 1: Write the failing tests**

`src/verdict.rs` tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_last_line_wins() {
        assert_eq!(parse_verdict("Looks fine.\nVERDICT: APPROVE"), Verdict::Approve);
        assert_eq!(parse_verdict("Issues below.\nVERDICT: CHANGES\n\n\n"), Verdict::Changes);
        assert_eq!(parse_verdict("VERDICT: APPROVE\r\n"), Verdict::Approve);
        assert_eq!(parse_verdict("text\r\nVERDICT: CHANGES   \r\n"), Verdict::Changes);
        assert_eq!(parse_verdict("VERDICT: APPROVE"), Verdict::Approve);
    }

    #[test]
    fn anything_else_is_missing() {
        assert_eq!(parse_verdict(""), Verdict::Missing);
        assert_eq!(parse_verdict("   \n \n"), Verdict::Missing);
        assert_eq!(parse_verdict("**VERDICT: APPROVE**"), Verdict::Missing);
        assert_eq!(parse_verdict("VERDICT: approve"), Verdict::Missing);
        assert_eq!(parse_verdict("VERDICT: APPROVE please"), Verdict::Missing);
        assert_eq!(parse_verdict("  VERDICT: APPROVE"), Verdict::Missing);
        assert_eq!(parse_verdict("VERDICT: APPROVE\nbut see the notes above"), Verdict::Missing);
        assert_eq!(parse_verdict("VERDICT: CHANGES\nVERDICT: MAYBE"), Verdict::Missing);
    }
}
```

`src/budget.rs` tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighted_formula() {
        assert_eq!(weighted_tokens(100, 200, 300, 1000), 100 + 200 + 300 + 100);
        assert_eq!(weighted_tokens(0, 0, 0, 9), 0);
        assert_eq!(weighted_tokens(0, 0, 0, 10), 1);
    }

    #[test]
    fn weighted_never_overflows() {
        assert_eq!(weighted_tokens(u64::MAX, u64::MAX, u64::MAX, u64::MAX), u64::MAX);
    }

    #[test]
    fn thresholds() {
        assert_eq!(budget_state(0, 1000), BudgetState::Ok);
        assert_eq!(budget_state(799, 1000), BudgetState::Ok);
        assert_eq!(budget_state(800, 1000), BudgetState::Warn);
        assert_eq!(budget_state(999, 1000), BudgetState::Warn);
        assert_eq!(budget_state(1000, 1000), BudgetState::Stop);
        assert_eq!(budget_state(5000, 1000), BudgetState::Stop);
    }

    #[test]
    fn zero_budget_and_huge_values() {
        assert_eq!(budget_state(0, 0), BudgetState::Stop);
        assert_eq!(budget_state(u64::MAX - 1, u64::MAX), BudgetState::Warn);
        assert_eq!(budget_state(u64::MAX, u64::MAX), BudgetState::Stop);
    }
}
```

Append to `src/stage.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use Stage::*;

    #[test]
    fn forward_and_same_moves_are_allowed() {
        for (from, to) in [(Spec, Build), (Build, Review), (Review, Approve), (Approve, Merged), (Spec, Merged), (Build, Build)] {
            assert!(Stage::check_move(from, to, false).is_ok(), "{from} -> {to}");
        }
    }

    #[test]
    fn backward_moves_are_refused_unless_forced() {
        assert!(Stage::check_move(Review, Build, false).is_err());
        assert!(Stage::check_move(Approve, Spec, false).is_err());
        assert!(Stage::check_move(Review, Build, true).is_ok());
    }

    #[test]
    fn merged_is_final_unless_forced() {
        for to in [Spec, Build, Review, Approve, NeedsHuman] {
            assert!(Stage::check_move(Merged, to, false).is_err(), "merged -> {to}");
        }
        assert!(Stage::check_move(Merged, Build, true).is_ok());
    }

    #[test]
    fn needs_human_is_a_parking_stage() {
        for from in [Spec, Build, Review, Approve] {
            assert!(Stage::check_move(from, NeedsHuman, false).is_ok());
        }
        for to in [Build, Review, Approve, Merged] {
            assert!(Stage::check_move(NeedsHuman, to, false).is_ok(), "needs-human -> {to}");
        }
        assert!(Stage::check_move(NeedsHuman, Spec, false).is_err());
    }

    #[test]
    fn serde_names_match_the_v1_board() {
        let names: Vec<String> = [Spec, Build, Review, Approve, Merged, NeedsHuman]
            .iter()
            .map(|s| serde_json::to_string(s).expect("serialize"))
            .collect();
        assert_eq!(names, vec!["\"spec\"", "\"build\"", "\"review\"", "\"approve\"", "\"merged\"", "\"needs-human\""]);
    }
}
```

Add to `src/board.rs` a `#[cfg(test)] mod tests`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn job() -> Job {
        Job::new("001", "2026-10-13-001-x", "x", "2026-10-13", "feature", 100.0)
    }

    #[test]
    fn set_stage_updates_since_only_on_change_and_manages_reason() {
        let mut j = job();
        j.set_stage(Stage::Build, None, false, 200.0).expect("spec to build");
        assert_eq!(j.stage_since, 200.0);
        j.set_stage(Stage::Build, None, false, 300.0).expect("same stage");
        assert_eq!(j.stage_since, 200.0);
        j.set_stage(Stage::NeedsHuman, Some("risky diff"), false, 400.0).expect("park");
        assert_eq!(j.reason.as_deref(), Some("risky diff"));
        j.set_stage(Stage::Review, Some("ignored"), false, 500.0).expect("resume");
        assert_eq!(j.reason, None);
    }

    #[test]
    fn refused_move_leaves_the_job_untouched() {
        let mut j = job();
        j.set_stage(Stage::Merged, None, false, 200.0).expect("forward");
        let before = j.clone();
        assert!(j.set_stage(Stage::Build, None, false, 300.0).is_err());
        assert_eq!(j, before);
    }

    #[test]
    fn base_is_set_once() {
        let mut j = job();
        j.set_base("aaa", false).expect("first");
        j.set_base("aaa", false).expect("same value is fine");
        assert!(j.set_base("bbb", false).is_err());
        j.set_base("bbb", true).expect("forced");
        assert_eq!(j.base.as_deref(), Some("bbb"));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-core stage`
Expected: compile error (`check_move`, `set_stage` not defined).

- [ ] **Step 3: Implement verdict and budget**

Add `pub mod budget; pub mod verdict;` to `lib.rs` plus re-exports `BudgetState, budget_state, weighted_tokens` and `Verdict, parse_verdict`.

`src/verdict.rs` (above tests):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Approve,
    Changes,
    Missing,
}

/// Only an exact last non-empty line counts. Trailing whitespace and CRLF are tolerated, nothing else.
pub fn parse_verdict(text: &str) -> Verdict {
    let last = text.lines().rev().map(|l| l.trim_end()).find(|l| !l.is_empty());
    match last {
        Some("VERDICT: APPROVE") => Verdict::Approve,
        Some("VERDICT: CHANGES") => Verdict::Changes,
        _ => Verdict::Missing,
    }
}
```

`src/budget.rs` (above tests):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetState {
    Ok,
    Warn,
    Stop,
}

/// Input plus cache creation plus output plus one tenth of cache reads. Saturates instead of overflowing.
pub fn weighted_tokens(input: u64, cache_creation: u64, output: u64, cache_read: u64) -> u64 {
    input
        .saturating_add(cache_creation)
        .saturating_add(output)
        .saturating_add(cache_read / 10)
}

/// Warn at 80 percent of the budget, stop at 100 percent. A zero budget is always stop.
pub fn budget_state(used: u64, budget: u64) -> BudgetState {
    if used >= budget {
        return BudgetState::Stop;
    }
    if u128::from(used) * 100 >= u128::from(budget) * 80 {
        return BudgetState::Warn;
    }
    BudgetState::Ok
}
```

- [ ] **Step 4: Implement the stage guard and the job setters**

In `src/stage.rs`, add above the tests:

```rust
use crate::error::CoreError;

impl Stage {
    fn rank(self) -> u8 {
        match self {
            Stage::Spec => 0,
            Stage::Build => 1,
            Stage::Review => 2,
            Stage::Approve => 3,
            Stage::Merged => 4,
            Stage::NeedsHuman => 5,
        }
    }

    /// No backward move and no move out of merged, unless forced.
    /// Any live stage may park in needs-human, and needs-human may resume at any stage after spec.
    pub fn check_move(from: Stage, to: Stage, force: bool) -> Result<(), CoreError> {
        let bad = CoreError::StageMove { from, to };
        if force || from == to {
            return Ok(());
        }
        let allowed = match (from, to) {
            (Stage::Merged, _) => false,
            (_, Stage::NeedsHuman) => true,
            (Stage::NeedsHuman, Stage::Spec) => false,
            (Stage::NeedsHuman, _) => true,
            _ => to.rank() >= from.rank(),
        };
        if allowed {
            Ok(())
        } else {
            Err(bad)
        }
    }
}
```

Move the `use crate::error::CoreError;` line to the top of the file with the other imports.

In `src/board.rs`, inside `impl Job`:

```rust
    /// Moves the job to a stage. The reason is kept only for needs-human. Nothing changes when the move is refused.
    pub fn set_stage(&mut self, to: Stage, reason: Option<&str>, force: bool, now: f64) -> Result<(), CoreError> {
        Stage::check_move(self.stage, to, force)?;
        if self.stage != to {
            self.stage = to;
            self.stage_since = now;
        }
        self.reason = if to == Stage::NeedsHuman { reason.map(str::to_string) } else { None };
        Ok(())
    }

    /// The base sha is recorded once. Changing it needs `force`.
    pub fn set_base(&mut self, sha: &str, force: bool) -> Result<(), CoreError> {
        match &self.base {
            Some(have) if have != sha && !force => {
                Err(CoreError::BaseChange { have: have.clone(), want: sha.to_string() })
            }
            _ => {
                self.base = Some(sha.to_string());
                Ok(())
            }
        }
    }
```

- [ ] **Step 5: Run the whole crate tests and the workspace check**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-core`
Expected: every test passes (`board_roundtrip`, `fsutil`, `ids`, `ledger`, `risk`, `config`, `verdict`, `budget`, `stage`, `board`).
Run: `cargo check -j4 --manifest-path tools/factory/Cargo.toml`
Expected: all six crates build with no errors. Fix any warning in `factory-core` before committing.
Run: `grep -rn "unwrap()" tools/factory/crates` and expect no output.

- [ ] **Step 6: Commit**

```bash
git add tools/factory/crates/factory-core/src
git commit --only -m "feat(factory): add verdict, budget math and stage guard" -- tools/factory/crates/factory-core/src
```

Next plan: docs/superpowers/plans/2026-10-13-software-factory-v3/plan-02-git.md
