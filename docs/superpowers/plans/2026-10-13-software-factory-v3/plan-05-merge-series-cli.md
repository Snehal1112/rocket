# Software Factory v3, Plan 05: Merge, Series, Lock, Lease and the CLI

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. It is executed by a `factory-builder` subagent, which skips that skill's final whole-branch review, its ledger scripts and its progress file. Steps use checkbox (`- [ ]`) syntax. Plans 01, 02, 03 and 04 of this series must be finished and merged first (`factory-core`, `factory-git`, `factory-agent`, `factory-checks`, and the first half of `factory-engine`).

**Goal:** Finish `factory-engine` (merge, sync, series, run loop) and ship the `factory` binary with lock, lease and all subcommands, proven by a CLI test that takes a job from `new` to `merged`.

**Architecture:** `merge_stage` and `sync_stage` do all git work through `factory_git` and never leave main half-changed: every failure parks the job with the real error. `run_job` is a loop over the board stage, so a crash anywhere resumes by re-reading the board. `BoardStore` wraps every board write in an `fs2` file lock and a lease stops two drivers on one job. `factory-cli` is a lib plus a thin `main`, so the integration test drives it with a `FakeRunner`.

**Tech Stack:** Rust 2021, `git2` 0.19, `fs2` 0.4, `clap` 4 (derive), `serde`/`serde_json` 1, `toml` 0.8, `thiserror` 1, `tempfile` 3 (dev).

**Spec:** `docs/superpowers/specs/2026-10-13-software-factory-v3-rust-driver-design.md` (decisions 7, 10, 11, 12, 13 and the plan 05 row). Behavior ported from `.claude/commands/factory.md` (merge step, approve, rework), `.claude/factory/factory-ctl` (`status`, `tick-plan`, `new-job`, `set-stage`), and v2 plans 03 and 04 in `docs/superpowers/plans/2026-10-13-software-factory-v2/`.

## Global Constraints

- Rust workspace at `tools/factory/`, own workspace, own target dir, not in the root workspace. Pinned crates: serde 1, serde_json 1, toml 0.8, thiserror 1, clap 4 derive, git2 0.19, tempfile 3 (dev), fs2 0.4.
- Never `.unwrap()` anywhere (a harness hook blocks it in any Rust write). `.expect("...")` only in tests. Production code returns `Result` with thiserror error types.
- Commands: `cargo check -j4 --manifest-path tools/factory/Cargo.toml` and `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p <crate> <test_name>`. Never `--workspace`, never omit `-j4`, never set `CARGO_TARGET_DIR`.
- Comments are short full sentences ending in a period. No emojis.
- Merge message is fixed: `chore(factory): merge job NNN <slug>`. Merge the recorded reviewed head sha, never the branch name. Never push. Never rewrite history.
- Board stage names stay `spec`, `build`, `review`, `approve`, `merged`, `needs-human`. Agents never write the ledger or board; only the driver does.
- At most 2 syncs per job, at most 6 plans per series, 1 to 3 tasks per plan, at most 2 fix rounds (config `fix_rounds`).
- Commit steps: invoke skill `dev-workflow-skills:1-git-commit` for the message, but run its git commands inline with no helper agents. Pathspec commit only: `git add <paths> && git commit --only -m "..." -- <paths>`. Never `git add -A` or `git add .`.

## Review Focus

1. Main has a tracked change or an untracked file that blocks the merge: the job parks in `needs-human` with the real error and main's tip is unchanged. Tested in Task 1.
2. Driver crashed after the merge commit but before the board said `merged`: `run_job` must finish the bookkeeping without a second merge. Tested in Task 2.
3. A plan chain with a cycle, a 4-task plan, a missing file, a missing `Next plan:` last line, more than 6 plans, or an orphan plan file in the folder: clear error, no loop. Tested in Task 2.
4. Two `factory run` processes on one job, or two writers racing on the board: the second is refused or waits a bounded time, and ids never collide. Tested in Task 3.
5. `rework` with a multi-line or over-200-character note, or on a job that is not `needs-human`; `cleanup` on a job that is neither merged nor cancelled: refused with a clear message and no change. Tested in Task 3.

## Step 0 (read before Task 1): verify the interfaces from plans 01 to 04

Plans 01 to 04 are written, and the shapes below are copied from them, so the code in this plan calls the real signatures. This is a verification step. Run once and confirm; if the merged source differs from a plan, fix the call sites and say so in the commit message:

```bash
grep -rn "pub fn\|pub struct\|pub enum\|pub trait" tools/factory/crates/factory-core/src tools/factory/crates/factory-git/src tools/factory/crates/factory-agent/src tools/factory/crates/factory-engine
```

Real shapes (this plan uses these):

- `factory_core` (plan 01): `Board { pub jobs: Vec<Job> }` with `Board::load(&Path)`, `save(&self, &Path)`, `find(&self, id) -> Result<&Job, CoreError>`, `find_mut(&mut self, id) -> Result<&mut Job, CoreError>`; `Job::new(id, dir, slug, created, feature, now: f64) -> Job` and the fields `id, dir, slug, created, feature, branch, worktree, builder_id, base, head, stage, round, reason, stage_since, reviews, model` plus plan 04 Step 0b `prev_head: Option<String>` and `tokens: JobTokens` (`tokens.weighted() -> u64`); `Job::set_stage(&mut self, to: Stage, reason: Option<&str>, force: bool, now: f64) -> Result<(), CoreError>` (only a backward move needs `force = true`, and every forced move gets a `Ruling:` ledger line); `Job::set_base(&mut self, sha: &str, force: bool)`; `Stage::check_move` (any stage may park in `needs-human`, and `needs-human` may resume at build, review or approve without force); `Ledger::new(path)` with `create(header)`, `task(n, TaskState, Option<&str>)`, `ruling(&str)` (one line only), `tasks() -> Result<Vec<(u32, TaskState)>, CoreError>` and the public `path` field; `TaskState::{Dispatched, ImplementerDone, Complete}`; `Config` with `diff_cap_lines: u32, budget_weighted: u64, fix_rounds: u32, stall_minutes: u32`; `write_atomic(path, bytes)`; `next_job_id(board_ids: &[String], dir_names: &[String], branch_names: &[String]) -> String`; `job_dir_name(date, id, slug) -> String`; `slugify(&str) -> String`; `normalize_id(&str) -> Result<String, CoreError>`; `weighted_tokens(input, cache_creation, output, cache_read: u64) -> u64`; `budget_state(used, budget) -> BudgetState`. Plan 01 has no `now_secs()`, no `today()`, no `Job::transition`, and no `syncs`, `series`, `lease`, `cancelled` or `cleaned` fields. This plan adds those fields in `factory-core` (Tasks 1 to 3) and the two clock functions in `factory-engine` (Task 1 Step 5). Every new `Job` field uses `#[serde(default, skip_serializing_if = ...)]`, so a v1 board still round-trips and plan 01's tests stay green.
- `factory_git` (plan 02): `Repo::open(&Path)`, `root()`, `tip(&self, branch) -> Result<String>`, `is_ancestor(&self, older, newer) -> Result<bool>`, `is_merged(&self, base: &str, head: &str) -> Result<Option<String>>` (the main tip when merged, an error when the job has no commits), `preflight_main(&self) -> Result<Preflight>` with `Preflight::{Ready { main_sha }, Busy { reason }}`, `base_state(&self, base: &str, head: &str) -> Result<BaseCheck>` with `BaseCheck { state: BaseState, main_sha: String }` and `BaseState::{Current, MainMoved, BaseNotMain, Merged}`, `merge_no_ff(&self, branch: &str, head_sha: &str, message: &str) -> Result<MergeOutcome>` (it does not check that main has not moved, so callers run `base_state` first and merge only on `Current`), `merge_main_into(&self, wt: &Worktree, message: &str) -> Result<MergeOutcome>`, `remove_worktree(&self, wt: &Worktree, force: bool) -> Result<()>` (it also deletes the job branch, and refuses a dirty worktree or unmerged commits unless `force`), `MergeOutcome::{Merged { merge_sha }, UpToDate, Conflict { paths }, Refused { reason }}` (a conflict leaves everything unchanged), `JobRef { id, slug }` with `dir_name()`, and `Worktree { name, path, branch, base }`. `GitError` has a `Refused(String)` variant.
- `factory_agent` (plan 03): `AgentRunner::run(&self, &AgentSpec) -> AgentRun`, `ClaudeRunner::new(program, Caps, EventSink)`, `Caps::default()`, `EventSink::new(path, job_id: &str)`, `FakeRunner::new(Vec<AgentRun>)` (it replays fixed runs and cannot run code, so tests that need a commit use the `ScriptedRunner` of plan 04's `tests/common/mod.rs`, and `factory-cli/tests/common/mod.rs` gets a copy of it).
- `factory_engine` (plan 04): `Engine<'a>` is built for one job (`Engine::new(board_path, job_dir, job_id, plan_file, repo, runner, config, events)`) and its stage methods are `build_stage(&self)` and `checks_stage(&self)` (both `-> Result<StageOutcome, EngineError>`, with `StageOutcome::{Advanced(Stage), NeedsHuman(String)}`), `review_stage(&self) -> Result<ReviewOutcome, EngineError>` (`ReviewOutcome { verdict, review_path, decision: Decision }`, `Decision::{AutoMerge, FixRound, NeedsHuman(String)}`) and `apply_decision(&self, &ReviewOutcome) -> Result<StageOutcome, EngineError>` (it approves after the final checks, starts a fix round as a forced review-to-build move with a ledger ruling, or parks). `EngineError` carries strings and has no `From` impls. This plan writes the same four as free functions over `(engine, id)` (`build_stage(engine, id)`, `checks_stage(engine, id)`, `review_stage(engine, id)`, `apply_decision(engine, id, &outcome)`) and uses the accessors `root() -> &Path`, `config() -> &Config`, `repo() -> &Repo`, `store() -> &BoardStore`, `job(id) -> Result<Job, EngineError>`, `job_dir(&Job) -> PathBuf`, `ledger(&Job) -> Ledger`. Plan 04's `Engine::job()` takes no argument, so the accessors and free functions live in `crates/factory-engine/src/compat.rs` as thin wrappers that build a per-job plan 04 `Engine` for `id`, and `compat.rs` also adds `From<CoreError>`, `From<GitError>`, `From<std::io::Error>`, `From<StoreError>` and `From<SeriesError>` for `EngineError`. Use them so the rest of the plan reads unchanged.
- `build_stage` creates the worktree when the job has none (through `Repo::create_worktree(&JobRef)`). The `rework-note.md` handling is added in Task 3 Step 7.
- Engine tests extend plan 04's `tests/common/mod.rs` (`Fixture`, `ScriptedRunner`, `commit_file`). The merge fixture below is named `MergeFixture` so it does not clash.

## File structure (all under `tools/factory/crates/`)

- `factory-engine/src/tick.rs` (create): tick plan checkboxes and write `Task N: complete (merge <sha>)` lines.
- `factory-engine/src/merge.rs` (create): `merge_stage`, `park`, `MergeResult`.
- `factory-engine/src/sync.rs` (create): `sync_stage`, `SyncOutcome`.
- `factory-engine/src/series.rs` (create): `parse_series`, `Series`, `SeriesError`.
- `factory-engine/src/runloop.rs` (create): `run_job`, `run_series`, `create_job`.
- `factory-engine/src/store.rs`, `lock.rs`, `lease.rs`, `clock.rs`, `compat.rs` (create): board store, file lock, lease, `now_secs`/`today`, plan 04 bridge.
- `factory-engine/tests/` (create): `merge.rs`, `series.rs`, `runloop.rs`, `lock_lease.rs`; `common/mod.rs` is plan 04's file, extended here.
- `factory-cli/src/{lib.rs,main.rs,cli.rs,commands.rs,status.rs}` and `factory-cli/tests/new_to_merged.rs` (create).

---

### Task 1: `merge_stage`, `sync_stage` and plan ticking

**Files:**
- Create: `crates/factory-engine/src/tick.rs`, `merge.rs`, `sync.rs`, `store.rs`, `clock.rs`, `compat.rs`, `tests/merge.rs`
- Modify: `crates/factory-engine/tests/common/mod.rs` (plan 04's file, add `MergeFixture`), `crates/factory-engine/src/lib.rs` (declare modules, re-export), `crates/factory-engine/Cargo.toml` (dev-deps `tempfile`, `git2` are already there from plan 04), `crates/factory-core/src/board.rs` (`Job.syncs`)

**Interfaces:**
- Consumes: Step 0 shapes.
- Produces:
  - `pub fn tick_plan(plan_path: &Path, ledger: &Ledger, merge_sha: &str) -> Result<Vec<u32>, EngineError>` returns the task numbers newly recorded.
  - `pub struct BoardStore { .. }` with `BoardStore::new(root: &Path) -> BoardStore`, `load(&self) -> Result<Board, StoreError>`, `update<R>(&self, f: impl FnOnce(&mut Board) -> Result<R, StoreError>) -> Result<R, StoreError>` (no lock yet, Task 3 adds it), `update_job<R>(&self, id: &str, f: impl FnOnce(&mut Job) -> Result<R, CoreError>) -> Result<R, StoreError>` (the closure returns a `Result` so it can call `Job::set_stage`).
  - `pub fn next_id(board: &Board) -> String` (board ids only, `next_job_id` with no folder or branch names), `pub fn now_secs() -> f64`, `pub fn today() -> String` (UTC `YYYY-MM-DD`).
  - `pub enum MergeResult { Merged(String), Parked(String) }`
  - `pub fn merge_stage(engine: &Engine, id: &str) -> Result<MergeResult, EngineError>`
  - `pub enum SyncOutcome { Synced, Parked(String) }`
  - `pub fn sync_stage(engine: &Engine, id: &str) -> Result<SyncOutcome, EngineError>`
  - `pub fn park(engine: &Engine, id: &str, reason: &str) -> Result<(), EngineError>`

- [ ] **Step 1: Write the failing tick test**

In `src/tick.rs` start with the test module only (the function comes in step 3):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use factory_core::Ledger;

    #[test]
    fn ticks_checkboxes_and_writes_ledger_lines_once() {
        let dir = tempfile::tempdir().expect("tempdir");
        let plan = dir.path().join("plan.md");
        std::fs::write(&plan, "# P\n- [ ] Task 1: a\n- [ ] Task 2: b\n### Task 3: c\n")
            .expect("write plan");
        let ledger = Ledger::new(dir.path().join("progress.md"));
        let done = tick_plan(&plan, &ledger, "abc1234").expect("tick");
        assert_eq!(done, vec![1, 2, 3]);
        let text = std::fs::read_to_string(&plan).expect("read");
        assert!(text.contains("- [x] Task 1: a") && text.contains("- [x] Task 2: b"));
        let led = std::fs::read_to_string(dir.path().join("progress.md")).expect("ledger");
        assert!(led.contains("Task 2: complete (merge abc1234)"));
        assert!(tick_plan(&plan, &ledger, "abc1234").expect("again").is_empty());
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine ticks_checkboxes`
Expected: FAIL to compile, `tick_plan` not found.

- [ ] **Step 3: Implement `tick_plan`**

Plans use either `- [ ] Task N:` lines or `### Task N:` headings, so task numbers come from both.

```rust
use std::collections::BTreeSet;
use std::path::Path;

use factory_core::{Ledger, TaskState};

use crate::EngineError;

/// Ticks `- [ ] Task N:` lines and records `Task N: complete (merge <sha>)` once per task.
pub fn tick_plan(plan_path: &Path, ledger: &Ledger, merge_sha: &str) -> Result<Vec<u32>, EngineError> {
    let text = std::fs::read_to_string(plan_path)?;
    let already: BTreeSet<u32> = ledger
        .tasks()?
        .into_iter()
        .filter(|(_, state)| *state == TaskState::Complete)
        .map(|(n, _)| n)
        .collect();
    let mut numbers = BTreeSet::new();
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        if let Some(n) = task_number(line.trim_start_matches("- [ ] ").trim_start_matches("- [x] "))
            .or_else(|| task_number(line.trim_start_matches("### ")))
        {
            numbers.insert(n);
        }
        match line.strip_prefix("- [ ] Task ") {
            Some(rest) => out.push_str(&format!("- [x] Task {rest}")),
            None => out.push_str(line),
        }
        out.push('\n');
    }
    if out != text {
        factory_core::write_atomic(plan_path, out.as_bytes())?;
    }
    let mut recorded = Vec::new();
    for n in numbers {
        if !already.contains(&n) {
            ledger.task(n, TaskState::Complete, Some(&format!("merge {merge_sha}")))?;
            recorded.push(n);
        }
    }
    Ok(recorded)
}

fn task_number(s: &str) -> Option<u32> {
    let rest = s.strip_prefix("Task ")?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || !rest[digits.len()..].starts_with(':') {
        return None;
    }
    digits.parse().ok()
}
```

The ledger lines come from `Ledger::task`, so they match the strict `Task N: complete (...)` form that `Ledger::tasks` parses.

- [ ] **Step 4: Run it to verify it passes**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine ticks_checkboxes`
Expected: PASS.

- [ ] **Step 5: Write `store.rs` and the shared test fixtures**

`store.rs` is the only place that touches `board.json` from the engine and CLI. Task 3 adds the lock inside `update`.

```rust
use std::path::{Path, PathBuf};

use factory_core::{Board, CoreError, Job};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("board.json is not valid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Core(#[from] CoreError),
    #[error("the board is locked by another factory process, try again")]
    Locked,
    #[error("{0}")]
    Other(String),
}

pub struct BoardStore {
    board_path: PathBuf,
}

impl BoardStore {
    pub fn new(root: &Path) -> BoardStore {
        BoardStore { board_path: root.join(".superpowers/factory/board.json") }
    }
    pub fn board_path(&self) -> &Path {
        &self.board_path
    }
    pub fn load(&self) -> Result<Board, StoreError> {
        match std::fs::read_to_string(&self.board_path) {
            Ok(text) => Ok(serde_json::from_str(&text)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Board::default()),
            Err(e) => Err(e.into()),
        }
    }
    pub fn update<R>(&self, f: impl FnOnce(&mut Board) -> Result<R, StoreError>) -> Result<R, StoreError> {
        let mut board = self.load()?;
        let out = f(&mut board)?;
        let mut text = serde_json::to_string_pretty(&board)?;
        text.push('\n');
        if let Some(parent) = self.board_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        factory_core::write_atomic(&self.board_path, text.as_bytes())
            .map_err(|e| StoreError::Other(e.to_string()))?;
        Ok(out)
    }
    pub fn update_job<R>(&self, id: &str, f: impl FnOnce(&mut Job) -> Result<R, CoreError>) -> Result<R, StoreError> {
        self.update(|board| Ok(f(board.find_mut(id)?)?))
    }
}

/// The next free job id, looking at the board only. `create_job` also passes folder names.
pub fn next_id(board: &Board) -> String {
    let ids: Vec<String> = board.jobs.iter().map(|j| j.id.clone()).collect();
    factory_core::next_job_id(&ids, &[], &[])
}
```

`StoreError` and `SeriesError` are mapped into `EngineError::Msg` by the `From` impls of `compat.rs` (Step 0), so `?` works on them. `engine.store()` is a one-line accessor `pub fn store(&self) -> &BoardStore` in `compat.rs`.

`src/clock.rs`, the two clock functions plan 01 does not have (re-export both from `lib.rs`):

```rust
use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since the Unix epoch, the unit of `Job::stage_since`.
pub fn now_secs() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// The UTC date `YYYY-MM-DD` for a count of days since 1970-01-01.
pub fn date_from_days(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Today's UTC date as `YYYY-MM-DD`.
pub fn today() -> String {
    date_from_days((now_secs() / 86_400.0) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_from_days() {
        assert_eq!(date_from_days(0), "1970-01-01");
        assert_eq!(date_from_days(11_016), "2000-02-29");
        assert_eq!(date_from_days(18_993), "2022-01-01");
    }
}
```

Extend plan 04's `tests/common/mod.rs` (it already has `Fixture`, `ScriptedRunner`, `scripted` and `commit_file`; keep them) with a merge fixture: a temp repo with an initial commit on `main`, a job on the board in stage `approve` with a committed change on its branch, and a worktree made with `Repo::create_worktree`. Functions (full bodies use `git2`; they are test helpers, keep them short):

```rust
pub struct MergeFixture { pub dir: tempfile::TempDir, pub engine: factory_engine::Engine, pub id: String, pub head: String }

/// Creates a repo, a job branch with one commit adding `docs/hello.txt`, a plan file and a board entry at stage `approve`.
pub fn approved_job() -> MergeFixture { /* init repo, commit README on main, create branch factory/001-hello at a worktree, commit file, write plan.md and progress.md, add a `Job::new("001", "2026-10-13-001-hello", "hello", "2026-10-13", "say hello", 0.0)` with `stage`, `base`, `head` set (use `Job::set_stage`) to the board */ }

pub fn commit_file(repo_dir: &std::path::Path, rel: &str, body: &str, msg: &str) -> String { /* write, stage the path, commit on the checked-out branch, return the sha */ }

/// The job worktree path from the board.
pub fn worktree_of(fx: &MergeFixture) -> std::path::PathBuf { /* Job.worktree is an Option<String> */ }
pub fn main_tip(dir: &std::path::Path) -> String { /* git2 revparse "main" */ }
```

- [ ] **Step 6: Write the failing merge tests**

`tests/merge.rs`:

```rust
mod common;
use factory_engine::{merge_stage, MergeResult};

#[test]
fn merges_the_reviewed_head_with_the_fixed_message() {
    let fx = common::approved_job();
    let before = common::main_tip(fx.dir.path());
    let MergeResult::Merged(sha) = merge_stage(&fx.engine, &fx.id).expect("merge") else {
        panic!("expected a merge");
    };
    assert_ne!(sha, before);
    let repo = git2::Repository::open(fx.dir.path()).expect("open");
    let commit = repo.find_commit(git2::Oid::from_str(&sha).expect("oid")).expect("commit");
    assert_eq!(commit.message().expect("msg").lines().next(), Some("chore(factory): merge job 001 hello"));
    assert_eq!(commit.parent_id(1).expect("second parent").to_string(), fx.head);
    let job = fx.engine.job(&fx.id).expect("job");
    assert_eq!(job.stage, factory_core::Stage::Merged);
    let ledger = std::fs::read_to_string(fx.engine.job_dir(&job).join("progress.md")).expect("ledger");
    assert!(ledger.contains(&format!("Task 1: complete (merge {sha})")));
}

#[test]
fn a_blocking_untracked_file_parks_the_job_and_leaves_main_alone() {
    let fx = common::approved_job();
    // The reviewed change adds docs/hello.txt, so an untracked file at that path blocks the merge.
    std::fs::create_dir_all(fx.dir.path().join("docs")).expect("mkdir");
    std::fs::write(fx.dir.path().join("docs/hello.txt"), "mine").expect("write");
    let before = common::main_tip(fx.dir.path());
    let MergeResult::Parked(reason) = merge_stage(&fx.engine, &fx.id).expect("stage") else {
        panic!("expected a park");
    };
    assert!(!reason.is_empty());
    assert_eq!(common::main_tip(fx.dir.path()), before);
    let job = fx.engine.job(&fx.id).expect("job");
    assert_eq!(job.stage, factory_core::Stage::NeedsHuman);
    assert_eq!(job.reason.as_deref(), Some(reason.as_str()));
}

#[test]
fn a_moved_branch_tip_parks_without_merging() {
    let fx = common::approved_job();
    common::commit_file(&common::worktree_of(&fx), "late.txt", "x", "late");
    let before = common::main_tip(fx.dir.path());
    assert!(matches!(merge_stage(&fx.engine, &fx.id).expect("stage"), factory_engine::MergeResult::Parked(_)));
    assert_eq!(common::main_tip(fx.dir.path()), before);
}

#[test]
fn a_dirty_main_parks_as_main_busy() {
    let fx = common::approved_job();
    std::fs::write(fx.dir.path().join("README.md"), "dirty").expect("write");
    let r = merge_stage(&fx.engine, &fx.id).expect("stage");
    assert!(matches!(&r, factory_engine::MergeResult::Parked(m) if m.starts_with("main busy")));
}

#[test]
fn main_moved_triggers_a_sync_then_merges() {
    let fx = common::approved_job();
    common::commit_file(fx.dir.path(), "other.txt", "o", "main moves");
    let r = merge_stage(&fx.engine, &fx.id).expect("stage");
    assert!(matches!(r, factory_engine::MergeResult::Merged(_)), "got {r:?}");
    assert_eq!(fx.engine.job(&fx.id).expect("job").syncs, 1);
}

#[test]
fn a_sync_conflict_parks_and_the_third_sync_is_refused() {
    let fx = common::approved_job();
    let wt = common::worktree_of(&fx);
    common::commit_file(&wt, "same.txt", "branch", "branch side");
    // Re-review is out of scope here: record the new head as the reviewed head.
    common::record_head(&fx, &common::branch_tip(&fx));
    common::commit_file(fx.dir.path(), "same.txt", "main", "main side");
    let r = merge_stage(&fx.engine, &fx.id).expect("stage");
    assert!(matches!(&r, factory_engine::MergeResult::Parked(m) if m.contains("conflict")), "got {r:?}");
    assert_eq!(fx.engine.job(&fx.id).expect("job").stage, factory_core::Stage::NeedsHuman);
}
```

Add `record_head`, `branch_tip` helpers to `common` (`record_head` sets `Job.head` with `update_job`). In the fixture, the runner for the sync review replies `VERDICT: APPROVE`: the fixture's `Engine` is built with plan 04's `scripted(...)` `ScriptedRunner`, one reviewer effect per expected review.

- [ ] **Step 7: Run to verify failure**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test merge`
Expected: FAIL to compile (`merge_stage` missing).

- [ ] **Step 8: Implement `merge.rs`**

```rust
use factory_core::Stage;
use factory_git::{BaseState, MergeOutcome, Preflight};

use crate::{now_secs, sync_stage, tick_plan, Engine, EngineError, SyncOutcome};

pub const MAX_SYNCS: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeResult {
    Merged(String),
    Parked(String),
}

enum Step {
    Merged(String),
    Park(String),
}

/// Parks the job in needs-human with the real reason. Main is never touched here.
pub fn park(engine: &Engine, id: &str, reason: &str) -> Result<(), EngineError> {
    let now = now_secs();
    let job = engine.store().update_job(id, |job| {
        job.set_stage(Stage::NeedsHuman, Some(reason), false, now)?;
        Ok(job.clone())
    })?;
    // The ledger takes one line, so the reason is folded into a single line.
    let one_line = reason.split_whitespace().collect::<Vec<_>>().join(" ");
    engine.ledger(&job).ruling(&format!("parked in needs-human: {one_line}"))?;
    Ok(())
}

/// Merges the reviewed head into main, syncing first when main moved. Any failure parks the job.
pub fn merge_stage(engine: &Engine, id: &str) -> Result<MergeResult, EngineError> {
    match try_merge(engine, id) {
        Ok(Step::Merged(sha)) => {
            finish_merge(engine, id, &sha)?;
            Ok(MergeResult::Merged(sha))
        }
        Ok(Step::Park(reason)) => {
            park(engine, id, &reason)?;
            Ok(MergeResult::Parked(reason))
        }
        Err(err) => {
            let reason = format!("merge failed: {err}");
            park(engine, id, &reason)?;
            Ok(MergeResult::Parked(reason))
        }
    }
}

fn try_merge(engine: &Engine, id: &str) -> Result<Step, EngineError> {
    let repo = engine.repo();
    for _ in 0..=MAX_SYNCS {
        let job = engine.job(id)?;
        let head = job.head.clone().ok_or_else(|| EngineError::Msg("job has no recorded head".into()))?;
        let base = job.base.clone().ok_or_else(|| EngineError::Msg("job has no recorded base".into()))?;
        // merge_no_ff does not check that main has not moved, so base_state decides first.
        let check = repo.base_state(&base, &head)?;
        match check.state {
            // A crash after the merge commit but before the board update lands here.
            BaseState::Merged => return Ok(Step::Merged(check.main_sha)),
            BaseState::BaseNotMain => {
                return Ok(Step::Park("the job base is not on main, a human must look at the branch".into()));
            }
            BaseState::Current | BaseState::MainMoved => {}
        }
        let tip = repo.tip(&job.branch)?;
        if tip != head {
            return Ok(Step::Park(format!("branch tip {tip} differs from the reviewed head {head}")));
        }
        if let Preflight::Busy { reason } = repo.preflight_main()? {
            return Ok(Step::Park(format!("main busy: {reason}")));
        }
        if check.state == BaseState::MainMoved {
            match sync_stage(engine, id)? {
                SyncOutcome::Synced => continue,
                SyncOutcome::Parked(reason) => return Ok(Step::Park(reason)),
            }
        }
        let message = format!("chore(factory): merge job {} {}", job.id, job.slug);
        // plan 02's commit_merge already refuses a merge commit whose parents are not main and the head.
        return match repo.merge_no_ff(&job.branch, &head, &message)? {
            MergeOutcome::Merged { merge_sha } => Ok(Step::Merged(merge_sha)),
            MergeOutcome::Conflict { paths } => Ok(Step::Park(format!("merge conflict in {}", paths.join(", ")))),
            MergeOutcome::UpToDate => Ok(Step::Park("nothing to merge, head already in main".into())),
            MergeOutcome::Refused { reason } => Ok(Step::Park(format!("merge refused: {reason}"))),
        };
    }
    Ok(Step::Park("main moved again after the maximum number of syncs".into()))
}

fn finish_merge(engine: &Engine, id: &str, sha: &str) -> Result<(), EngineError> {
    let job = engine.job(id)?;
    let plan = engine.job_dir(&job).join("plan.md");
    if plan.exists() {
        tick_plan(&plan, &engine.ledger(&job), sha)?;
    }
    let now = now_secs();
    engine.store().update_job(id, |job| job.set_stage(Stage::Merged, None, false, now))?;
    Ok(())
}
```

Add `#[serde(default, skip_serializing_if = "is_zero")] pub syncs: u32` to `Job` in `factory-core` (with a private `fn is_zero(n: &u32) -> bool`, and `syncs: 0` in `Job::new`). The second-parent check needs no `Repo::parents`: the merge test asserts it with `git2`, and plan 02's `commit_merge` refuses a merge commit with the wrong parents.

- [ ] **Step 9: Implement `sync.rs`**

```rust
use std::path::PathBuf;

use factory_core::Stage;
use factory_git::{JobRef, MergeOutcome, Worktree};

use crate::{apply_decision, checks_stage, now_secs, review_stage, Decision, Engine, EngineError, StageOutcome, MAX_SYNCS};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncOutcome {
    Synced,
    Parked(String),
}

/// Merges main into the job worktree, reruns checks, and runs one reviewer pass on the sync diff.
pub fn sync_stage(engine: &Engine, id: &str) -> Result<SyncOutcome, EngineError> {
    let job = engine.job(id)?;
    if job.syncs >= MAX_SYNCS {
        return Ok(SyncOutcome::Parked(format!("main moved again after {} syncs", job.syncs)));
    }
    let path = job
        .worktree
        .clone()
        .ok_or_else(|| EngineError::Msg("job has no worktree to sync".into()))?;
    let old_head = job.head.clone().unwrap_or_default();
    let wt = Worktree {
        name: JobRef { id: job.id.clone(), slug: job.slug.clone() }.dir_name(),
        path: PathBuf::from(path),
        branch: job.branch.clone(),
        base: job.base.clone().unwrap_or_default(),
    };
    let message = format!("chore(factory): sync main into job {} {}", job.id, job.slug);
    let new_head = match engine.repo().merge_main_into(&wt, &message)? {
        MergeOutcome::Conflict { paths } => {
            return Ok(SyncOutcome::Parked(format!("sync conflict in {}", paths.join(", "))));
        }
        MergeOutcome::Refused { reason } => return Ok(SyncOutcome::Parked(format!("sync refused: {reason}"))),
        MergeOutcome::UpToDate => return Ok(SyncOutcome::Synced),
        MergeOutcome::Merged { merge_sha } => merge_sha,
    };
    let main7: String = engine.repo().tip("main")?.chars().take(7).collect();
    let now = now_secs();
    engine.store().update_job(id, |j| {
        j.prev_head = Some(old_head.clone());
        j.head = Some(new_head.clone());
        j.syncs += 1;
        // Approve back to review is a backward move, so it is forced and logged as a ruling below.
        j.set_stage(Stage::Review, None, true, now)
    })?;
    engine.ledger(&job).ruling(&format!("head sync {main7} is {new_head}, job back to review"))?;
    // The checks and one reviewer pass run on the sync diff (prev_head..head).
    match checks_stage(engine, id)? {
        StageOutcome::Advanced(Stage::Review) => {}
        StageOutcome::Advanced(_) => return Ok(SyncOutcome::Parked("checks failed after sync".into())),
        StageOutcome::NeedsHuman(why) => return Ok(SyncOutcome::Parked(format!("after sync: {why}"))),
    }
    let outcome = review_stage(engine, id)?;
    match &outcome.decision {
        Decision::AutoMerge => match apply_decision(engine, id, &outcome)? {
            StageOutcome::Advanced(Stage::Approve) => Ok(SyncOutcome::Synced),
            _ => Ok(SyncOutcome::Parked("final checks failed after sync".into())),
        },
        Decision::FixRound => Ok(SyncOutcome::Parked("sync review asked for changes".into())),
        Decision::NeedsHuman(why) => Ok(SyncOutcome::Parked(format!("after sync: {why}"))),
    }
}
```

`checks_stage` and `apply_decision` return plan 04's `StageOutcome` (see Step 0). `MAX_SYNCS` is re-exported from `merge.rs` in `lib.rs`.

- [ ] **Step 10: Run the tests**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test merge`
Expected: all 6 PASS. If the untracked-file test merges instead of parking, the git layer's `merge_no_ff` is not using a safe checkout: fix it in `factory-git` (a forced checkout would overwrite the file) and keep this test.

- [ ] **Step 11: Commit**

Invoke skill `dev-workflow-skills:1-git-commit` for the message (for example `feat(factory): merge and sync stages`), then:

```bash
git add tools/factory/crates/factory-engine tools/factory/crates/factory-core tools/factory/crates/factory-git && git commit --only -m "<message>" -- tools/factory/crates/factory-engine tools/factory/crates/factory-core tools/factory/crates/factory-git
```

---

### Task 2: Series parsing and the run loop

**Files:**
- Create: `crates/factory-engine/src/series.rs`, `runloop.rs`, `tests/series.rs`, `tests/runloop.rs`
- Modify: `crates/factory-engine/src/lib.rs`, `crates/factory-core` (`Job.series`)

**Interfaces:**
- Consumes: Task 1 (`merge_stage`, `park`, `BoardStore`), plan 04 stage fns and `decide`.
- Produces:
  - `pub struct SeriesPlan { pub path: PathBuf, pub tasks: Vec<u32>, pub next: Option<PathBuf> }`
  - `pub struct Series { pub plans: Vec<SeriesPlan> }`
  - `pub enum SeriesError` (thiserror, messages below)
  - `pub fn parse_series(root: &Path, start: &Path) -> Result<Series, SeriesError>`: `start` is a plan file, or a folder (starts at the lowest `plan-*.md` and also rejects orphan plan files).
  - In `factory-core`: `#[serde(default)] pub series: Option<SeriesPos>` on `Job`, `pub struct SeriesPos { pub plan_file: String, pub index: u32, pub total: u32 }`.
  - `pub fn create_job(engine: &Engine, slug: &str, feature: &str, plan: Option<&Path>, model: Option<&str>) -> Result<Job, EngineError>`
  - `pub enum RunEnd { Merged, NeedsHuman(String) }`
  - `pub fn run_job(engine: &Engine, id: &str) -> Result<RunEnd, EngineError>`
  - `pub fn run_series(engine: &Engine, id: &str) -> Result<RunEnd, EngineError>`

- [ ] **Step 1: Write the failing series tests**

`tests/series.rs` builds plan files in a temp dir with a helper `plan(dir, name, tasks, next)` that writes `### Task N: t` headings and a last line `Next plan: <next>`:

```rust
use factory_engine::{parse_series, SeriesError};
use std::path::Path;

fn plan(dir: &Path, name: &str, tasks: u32, next: &str) {
    let mut s = String::from("# Plan\n\n");
    for n in 1..=tasks {
        s.push_str(&format!("### Task {n}: t\n\n"));
    }
    s.push_str(&format!("Next plan: {next}\n"));
    std::fs::write(dir.join(name), s).expect("write plan");
}

#[test]
fn a_valid_chain_parses_in_order() {
    let d = tempfile::tempdir().expect("tmp");
    plan(d.path(), "plan-01-a.md", 2, "plan-02-b.md");
    plan(d.path(), "plan-02-b.md", 3, "none");
    let s = parse_series(d.path(), d.path()).expect("series");
    assert_eq!(s.plans.len(), 2);
    assert_eq!(s.plans[0].tasks, vec![1, 2]);
    assert!(s.plans[1].next.is_none());
}

#[test]
fn a_cycle_is_rejected() {
    let d = tempfile::tempdir().expect("tmp");
    plan(d.path(), "plan-01-a.md", 1, "plan-02-b.md");
    plan(d.path(), "plan-02-b.md", 1, "plan-01-a.md");
    let e = parse_series(d.path(), d.path()).expect_err("cycle");
    assert!(matches!(e, SeriesError::Cycle(_)), "{e}");
}

#[test]
fn bad_plans_are_rejected_with_clear_errors() {
    let d = tempfile::tempdir().expect("tmp");
    plan(d.path(), "plan-01-a.md", 4, "none");
    assert!(matches!(parse_series(d.path(), d.path()), Err(SeriesError::TaskCount { .. })));
    plan(d.path(), "plan-01-a.md", 0, "none");
    assert!(matches!(parse_series(d.path(), d.path()), Err(SeriesError::TaskCount { .. })));
    plan(d.path(), "plan-01-a.md", 1, "plan-09-missing.md");
    assert!(matches!(parse_series(d.path(), d.path()), Err(SeriesError::Missing(_))));
    std::fs::write(d.path().join("plan-01-a.md"), "### Task 1: t\n\nno pointer here\n").expect("w");
    assert!(matches!(parse_series(d.path(), d.path()), Err(SeriesError::NoNextLine(_))));
}

#[test]
fn an_orphan_plan_file_and_a_seventh_plan_are_rejected() {
    let d = tempfile::tempdir().expect("tmp");
    plan(d.path(), "plan-01-a.md", 1, "none");
    plan(d.path(), "plan-02-orphan.md", 1, "none");
    assert!(matches!(parse_series(d.path(), d.path()), Err(SeriesError::Orphan(_))));
    let e = tempfile::tempdir().expect("tmp");
    for i in 1..=7 {
        let next = if i == 7 { "none".to_string() } else { format!("plan-0{}-x.md", i + 1) };
        plan(e.path(), &format!("plan-0{i}-x.md"), 1, &next);
    }
    assert!(matches!(parse_series(e.path(), e.path()), Err(SeriesError::TooMany(_))));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test series`
Expected: FAIL to compile.

- [ ] **Step 3: Implement `series.rs`**

`Next plan:` paths are resolved against the repo root first, then against the folder of the plan file (so temp folders and `docs/...` paths both work).

```rust
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const MAX_PLANS: usize = 6;

#[derive(Debug, thiserror::Error)]
pub enum SeriesError {
    #[error("plan file not found: {0}")]
    Missing(PathBuf),
    #[error("{0}: the last line must be 'Next plan: <path|none>'")]
    NoNextLine(PathBuf),
    #[error("{path}: a plan needs 1 to 3 tasks, found {found}")]
    TaskCount { path: PathBuf, found: usize },
    #[error("cycle in the Next plan chain at {0}")]
    Cycle(PathBuf),
    #[error("the chain has more than {MAX_PLANS} plans, stopped at {0}")]
    TooMany(PathBuf),
    #[error("plan file is not in the chain: {0}")]
    Orphan(PathBuf),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct SeriesPlan {
    pub path: PathBuf,
    pub tasks: Vec<u32>,
    pub next: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Series {
    pub plans: Vec<SeriesPlan>,
}

fn resolve(root: &Path, from: &Path, rel: &str) -> PathBuf {
    let a = root.join(rel);
    if a.exists() {
        return a;
    }
    from.parent().map(|p| p.join(rel)).filter(|p| p.exists()).unwrap_or(a)
}

fn read_plan(root: &Path, path: &Path) -> Result<SeriesPlan, SeriesError> {
    let text = std::fs::read_to_string(path).map_err(|_| SeriesError::Missing(path.to_path_buf()))?;
    let last = text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
    let target = last
        .strip_prefix("Next plan:")
        .map(str::trim)
        .filter(|t| !t.is_empty() && !t.contains(char::is_whitespace))
        .ok_or_else(|| SeriesError::NoNextLine(path.to_path_buf()))?;
    let mut tasks: Vec<u32> = Vec::new();
    for line in text.lines() {
        let t = line.trim_start_matches("- [ ] ").trim_start_matches("- [x] ").trim_start_matches("### ");
        if let Some(rest) = t.strip_prefix("Task ") {
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = digits.parse::<u32>() {
                if rest[digits.len()..].starts_with(':') && !tasks.contains(&n) {
                    tasks.push(n);
                }
            }
        }
    }
    if tasks.is_empty() || tasks.len() > 3 {
        return Err(SeriesError::TaskCount { path: path.to_path_buf(), found: tasks.len() });
    }
    let next = (target != "none").then(|| resolve(root, path, target));
    Ok(SeriesPlan { path: path.to_path_buf(), tasks, next })
}

/// Validates the `Next plan:` chain from `start` (a plan file, or a folder of `plan-*.md` files).
pub fn parse_series(root: &Path, start: &Path) -> Result<Series, SeriesError> {
    let (first, folder) = if start.is_dir() {
        let mut names: Vec<PathBuf> = std::fs::read_dir(start)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("plan-") && n.ends_with(".md"))
            })
            .collect();
        names.sort();
        let first = names.first().cloned().ok_or_else(|| SeriesError::Missing(start.join("plan-01-*.md")))?;
        (first, Some((start.to_path_buf(), names)))
    } else {
        (start.to_path_buf(), None)
    };
    let mut plans = Vec::new();
    let mut seen = HashSet::new();
    let mut current = Some(first);
    while let Some(path) = current {
        let key = path.canonicalize().unwrap_or_else(|_| path.clone());
        if !seen.insert(key) {
            return Err(SeriesError::Cycle(path));
        }
        if plans.len() == MAX_PLANS {
            return Err(SeriesError::TooMany(path));
        }
        let plan = read_plan(root, &path)?;
        current = plan.next.clone();
        plans.push(plan);
    }
    if let Some((_, names)) = folder {
        for name in names {
            let key = name.canonicalize().unwrap_or_else(|_| name.clone());
            if !plans.iter().any(|p| p.path.canonicalize().unwrap_or_else(|_| p.path.clone()) == key) {
                return Err(SeriesError::Orphan(name));
            }
        }
    }
    Ok(Series { plans })
}
```

Add `#[error("{0}")] Series(#[from] SeriesError)` to `EngineError`.

- [ ] **Step 4: Run to verify the series tests pass**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test series`
Expected: 4 PASS.

- [ ] **Step 5: Write the failing run loop tests**

`tests/runloop.rs`, using the Task 1 fixture style. Needed fixtures: `common::new_job_engine()` builds a repo plus an `Engine` whose `ScriptedRunner` (plan 04's `tests/common/mod.rs`) builder effect commits `docs/<n>.txt` in the worktree and whose reviewer effect replies `VERDICT: APPROVE`, with an empty `[[checks]]` config.

```rust
mod common;
use factory_engine::{create_job, run_job, run_series, RunEnd};

#[test]
fn run_job_goes_from_build_to_merged_in_one_call() {
    let fx = common::new_job_engine();
    let job = create_job(&fx.engine, "hello", "say hello", Some(&fx.plan_file("plan-01-a.md", 1, "none")), None).expect("create");
    assert_eq!(run_job(&fx.engine, &job.id).expect("run"), RunEnd::Merged);
    assert_eq!(fx.engine.job(&job.id).expect("job").stage, factory_core::Stage::Merged);
}

#[test]
fn a_crash_after_the_merge_commit_resumes_without_a_second_merge() {
    let fx = common::approved_job_fixture();
    // Simulate the crash: merge by hand with the real message, leave the board at approve.
    let sha = common::merge_by_hand(&fx);
    let before = common::main_tip(fx.dir.path());
    assert_eq!(before, sha);
    assert_eq!(run_job(&fx.engine, &fx.id).expect("run"), RunEnd::Merged);
    assert_eq!(common::main_tip(fx.dir.path()), sha, "no second merge commit");
    let ledger = std::fs::read_to_string(fx.engine.job_dir(&fx.engine.job(&fx.id).expect("job")).join("progress.md")).expect("l");
    assert!(ledger.contains(&format!("(merge {sha})")));
}

#[test]
fn run_job_stops_on_needs_human_and_run_series_chains_plans() {
    let fx = common::new_job_engine();
    let a = fx.plan_file("plan-01-a.md", 1, "plan-02-b.md");
    fx.plan_file("plan-02-b.md", 1, "none");
    let job = create_job(&fx.engine, "series", "two plans", Some(&a), None).expect("create");
    assert_eq!(run_series(&fx.engine, &job.id).expect("series"), RunEnd::Merged);
    let board = fx.engine.store().load().expect("board");
    assert_eq!(board.jobs.len(), 2);
    assert!(board.jobs.iter().all(|j| j.stage == factory_core::Stage::Merged));
    assert_eq!(board.jobs[1].series.as_ref().map(|s| (s.index, s.total)), Some((2, 2)));
}

#[test]
fn run_series_stops_when_a_plan_needs_a_human() {
    let fx = common::new_job_engine_with_reviewer("VERDICT: CHANGES\nno");
    let a = fx.plan_file("plan-01-a.md", 1, "plan-02-b.md");
    fx.plan_file("plan-02-b.md", 1, "none");
    let job = create_job(&fx.engine, "stuck", "x", Some(&a), None).expect("create");
    assert!(matches!(run_series(&fx.engine, &job.id).expect("series"), RunEnd::NeedsHuman(_)));
    assert_eq!(fx.engine.store().load().expect("board").jobs.len(), 1);
}
```

`plan_file(name, tasks, next)` writes a valid plan file into a `plans/` folder in the temp repo and returns its path.

- [ ] **Step 6: Run to verify failure**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test runloop`
Expected: FAIL to compile.

- [ ] **Step 7: Implement `runloop.rs`**

New jobs start at `build`: the plan was written interactively, so there is no `spec` stage in v3 (the stage name stays valid for old boards).

```rust
use std::path::Path;

use factory_core::{Job, Ledger, SeriesPos, Stage};

use crate::{apply_decision, build_stage, checks_stage, merge_stage, now_secs, park, parse_series, review_stage, today};
use crate::{Engine, EngineError, MergeResult, StageOutcome};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunEnd {
    Merged,
    NeedsHuman(String),
}

/// Creates the job folder, ledger, plan copy and board entry. The job starts at stage build.
pub fn create_job(
    engine: &Engine,
    slug: &str,
    feature: &str,
    plan: Option<&Path>,
    model: Option<&str>,
) -> Result<Job, EngineError> {
    let series_pos = match plan {
        Some(p) => {
            let series = parse_series(engine.root(), p)?;
            Some(SeriesPos { plan_file: p.to_string_lossy().into_owned(), index: 1, total: series.plans.len() as u32 })
        }
        None => None,
    };
    // Folder names keep a new id from colliding with a leftover. Plan 02 has no branch listing,
    // so `create_worktree` refuses an existing branch with a clear error instead.
    let dirs: Vec<String> = std::fs::read_dir(engine.root().join(".superpowers/sdd"))
        .map(|rd| rd.filter_map(|e| e.ok()).filter_map(|e| e.file_name().into_string().ok()).collect())
        .unwrap_or_default();
    let job = engine.store().update(|board| {
        let ids: Vec<String> = board.jobs.iter().map(|j| j.id.clone()).collect();
        let id = factory_core::next_job_id(&ids, &dirs, &[]);
        let slug = factory_core::slugify(slug);
        let date = today();
        let dir = factory_core::job_dir_name(&date, &id, &slug);
        let now = now_secs();
        let mut job = Job::new(&id, &dir, &slug, &date, feature, now);
        job.set_stage(Stage::Build, None, false, now)?;
        job.series = series_pos;
        job.model = model.map(str::to_string);
        board.jobs.push(job.clone());
        Ok(job)
    })?;
    let dir = engine.job_dir(&job);
    std::fs::create_dir_all(&dir)?;
    Ledger::new(dir.join("progress.md")).create(&format!("# SDD ledger, plan: .superpowers/sdd/{}/plan.md", job.dir))?;
    if let Some(p) = plan {
        std::fs::copy(p, dir.join("plan.md"))?;
    }
    Ok(job)
}

/// Drives one job to merged or needs-human. The board stage is the only state, so a crash resumes here.
pub fn run_job(engine: &Engine, id: &str) -> Result<RunEnd, EngineError> {
    let limit = engine.config().fix_rounds as usize * 4 + 12;
    for _ in 0..limit {
        let job = engine.job(id)?;
        match job.stage {
            Stage::Merged => return Ok(RunEnd::Merged),
            Stage::NeedsHuman => return Ok(RunEnd::NeedsHuman(job.reason.unwrap_or_default())),
            Stage::Spec => {
                if !engine.job_dir(&job).join("plan.md").exists() {
                    park(engine, id, "no plan.md in the job folder")?;
                } else {
                    let now = now_secs();
                    engine.store().update_job(id, |j| j.set_stage(Stage::Build, None, false, now))?;
                }
            }
            Stage::Build => {
                build_stage(engine, id)?;
            }
            Stage::Review => {
                // checks_stage starts the fix round itself (a forced review-to-build move with a ruling)
                // and parks the job when the rounds are used up, so anything but a pass loops back to the board.
                if checks_stage(engine, id)? != StageOutcome::Advanced(Stage::Review) {
                    continue;
                }
                let outcome = review_stage(engine, id)?;
                // apply_decision approves after the final checks, starts a fix round, or parks.
                apply_decision(engine, id, &outcome)?;
            }
            Stage::Approve => {
                if let MergeResult::Parked(why) = merge_stage(engine, id)? {
                    return Ok(RunEnd::NeedsHuman(why));
                }
            }
        }
    }
    park(engine, id, "run loop did not settle, see the ledger")?;
    Ok(RunEnd::NeedsHuman("run loop did not settle".into()))
}

/// Runs plan k, then creates and runs plan k+1, one at a time. Stops on needs-human.
pub fn run_series(engine: &Engine, id: &str) -> Result<RunEnd, EngineError> {
    let mut current = id.to_string();
    loop {
        let end = run_job(engine, &current)?;
        if end != RunEnd::Merged {
            return Ok(end);
        }
        let job = engine.job(&current)?;
        let Some(pos) = job.series.clone() else { return Ok(RunEnd::Merged) };
        let series = parse_series(engine.root(), Path::new(&pos.plan_file))?;
        let Some(next) = series.plans[0].next.clone() else { return Ok(RunEnd::Merged) };
        // Resume: the next job may already exist from an earlier run.
        let board = engine.store().load()?;
        let existing = board.jobs.iter().find(|j| {
            j.series.as_ref().is_some_and(|s| s.plan_file == next.to_string_lossy() && s.index == pos.index + 1)
        });
        current = match existing {
            Some(j) => j.id.clone(),
            None => {
                let stem = next.file_stem().and_then(|s| s.to_str()).unwrap_or("plan").to_string();
                let created = create_job(engine, &stem, &job.feature, Some(&next), job.model.as_deref())?;
                engine.store().update_job(&created.id, |j| {
                    j.series = Some(SeriesPos { plan_file: next.to_string_lossy().into_owned(), index: pos.index + 1, total: pos.total });
                    Ok(())
                })?;
                created.id
            }
        };
    }
}
```

Core additions (plan 05 adds them): in `factory-core`, `pub struct SeriesPos { pub plan_file: String, pub index: u32, pub total: u32 }` (Debug, Clone, PartialEq, Serialize, Deserialize, exported from `lib.rs`) and `#[serde(default, skip_serializing_if = "Option::is_none")] pub series: Option<SeriesPos>` on `Job` (`series: None` in `Job::new`). Plan 01 already has `Job.model`, `Job::new`, `slugify`, `next_job_id` and `job_dir_name`, and `today()` and `now_secs()` come from `factory-engine` (Task 1 Step 5). `run_series` overwrites the `index: 1` that `create_job` sets, as shown.

- [ ] **Step 8: Run to verify**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test runloop`
Expected: 3 PASS (the series test runs two builds in two worktrees; if the second job's branch name collides, `create_job` must use the new id, which `next_job_id` guarantees).

- [ ] **Step 9: Commit**

Invoke skill `dev-workflow-skills:1-git-commit`, then:

```bash
git add tools/factory/crates/factory-engine tools/factory/crates/factory-core && git commit --only -m "<message>" -- tools/factory/crates/factory-engine tools/factory/crates/factory-core
```

---

### Task 3: Lock, lease and the `factory` CLI

**Files:**
- Create: `crates/factory-engine/src/lock.rs`, `lease.rs`, `tests/lock_lease.rs`; `crates/factory-cli/src/{lib.rs,main.rs,cli.rs,commands.rs,status.rs}`, `crates/factory-cli/tests/new_to_merged.rs`
- Modify: `crates/factory-engine/src/store.rs` (lock inside `update`), plan 04's `crates/factory-engine/src/ports.rs` (`update` goes through `BoardStore`), `crates/factory-core/src/board.rs` (`Lease`, `Job.lease`, `Job.cancelled`, `Job.cleaned`), `crates/factory-cli/Cargo.toml` (`clap`, `git2`, deps on engine, git, agent, core)

**Interfaces:**
- Consumes: Tasks 1 and 2, `ClaudeRunner` (`ClaudeRunner::new(program, Caps, EventSink)`) and a `ScriptedRunner` test runner.
- Produces:
  - `pub fn lock_board(path: &Path, wait: Duration) -> Result<BoardLock, StoreError>` (`BoardLock` unlocks on drop). `BoardStore::update` now takes it; `BoardStore::with_wait(self, Duration) -> BoardStore`. Default wait is `FACTORY_LOCK_WAIT` seconds, else 15.
  - `pub struct Lease { pub owner: String, pub expires: f64 }` (`Job.lease: Option<Lease>`, serde default). `pub fn acquire_lease(store: &BoardStore, id: &str, owner: &str, ttl_secs: f64, now: f64) -> Result<(), LeaseError>`, `pub fn release_lease(store: &BoardStore, id: &str, owner: &str) -> Result<(), LeaseError>`, `pub fn lease_is_live(job: &Job, now: f64) -> bool`. `LeaseError::Held { owner, secs_left }`.
  - `pub fn validate_rework_note(note: &str) -> Result<(), CliError>`
  - `pub fn render_status(board: &Board, cfg: &Config) -> String`
  - `pub struct CliCtx { pub root: PathBuf, pub runner: Option<Arc<dyn AgentRunner>>, pub owner: String }` with `CliCtx::new(root, runner: Arc<dyn AgentRunner>, owner)` (tests) and `CliCtx::runner_for(&self, job_id: &str) -> Arc<dyn AgentRunner>`
  - `pub fn dispatch(cli: Cli, ctx: &CliCtx) -> Result<String, CliError>` and the `Cli` clap type; `main` builds a `ClaudeRunner` and prints the string.

- [ ] **Step 1: Write the failing lock and lease tests**

`tests/lock_lease.rs`:

```rust
use factory_engine::{acquire_lease, lease_is_live, lock_board, release_lease, BoardStore, LeaseError, StoreError};
use std::time::Duration;

fn store_with_job() -> (tempfile::TempDir, BoardStore) {
    let d = tempfile::tempdir().expect("tmp");
    let store = BoardStore::new(d.path()).with_wait(Duration::from_millis(200));
    store
        .update(|b| {
            b.jobs.push(factory_core::Job::new("001", "d-001-x", "x", "2026-10-13", "f", 0.0));
            Ok(())
        })
        .expect("seed");
    (d, store)
}

#[test]
fn a_held_lock_fails_a_write_after_a_bounded_wait_but_not_a_read() {
    let (d, store) = store_with_job();
    let _held = lock_board(&d.path().join(".superpowers/factory/board.json.lock"), Duration::from_millis(50)).expect("lock");
    let err = store.update(|_| Ok(())).expect_err("locked");
    assert!(matches!(err, StoreError::Locked));
    assert_eq!(store.load().expect("read").jobs.len(), 1);
}

#[test]
fn concurrent_creators_never_collide_on_ids() {
    let d = tempfile::tempdir().expect("tmp");
    let root = d.path().to_path_buf();
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let root = root.clone();
            std::thread::spawn(move || {
                let store = BoardStore::new(&root).with_wait(Duration::from_secs(10));
                for _ in 0..5 {
                    store
                        .update(|b| {
                            let id = factory_engine::next_id(b);
                            b.jobs.push(factory_core::Job::new(&id, &format!("d-{id}"), "x", "2026-10-13", "f", 0.0));
                            Ok(())
                        })
                        .expect("update");
                }
            })
        })
        .collect();
    for h in handles {
        h.join().expect("join");
    }
    let mut ids: Vec<String> = BoardStore::new(&root).load().expect("load").jobs.into_iter().map(|j| j.id).collect();
    assert_eq!(ids.len(), 20);
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 20);
}

#[test]
fn a_live_lease_blocks_another_owner_until_it_expires() {
    let (_d, store) = store_with_job();
    acquire_lease(&store, "001", "alice", 60.0, 100.0).expect("alice");
    assert!(matches!(acquire_lease(&store, "001", "bob", 60.0, 110.0), Err(LeaseError::Held { .. })));
    acquire_lease(&store, "001", "alice", 60.0, 120.0).expect("renew");
    assert!(matches!(release_lease(&store, "001", "bob"), Err(LeaseError::Held { .. })));
    assert!(lease_is_live(&store.load().expect("b").jobs[0], 130.0));
    acquire_lease(&store, "001", "bob", 60.0, 500.0).expect("takeover after expiry");
    release_lease(&store, "001", "bob").expect("release");
    assert!(store.load().expect("b").jobs[0].lease.is_none());
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test lock_lease`
Expected: FAIL to compile.

- [ ] **Step 3: Implement `lock.rs`, `lease.rs` and wire the lock into `update`**

```rust
// lock.rs
use std::fs::File;
use std::path::Path;
use std::time::{Duration, Instant};

use fs2::FileExt;

use crate::StoreError;

pub struct BoardLock {
    file: File,
}

impl Drop for BoardLock {
    fn drop(&mut self) {
        // Closing the file also releases the lock, so an unlock error is harmless.
        let _ = FileExt::unlock(&self.file);
    }
}

/// Takes an exclusive advisory lock, polling until `wait` has passed.
pub fn lock_board(path: &Path, wait: Duration) -> Result<BoardLock, StoreError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::OpenOptions::new().create(true).write(true).truncate(false).open(path)?;
    let start = Instant::now();
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(BoardLock { file }),
            Err(_) if start.elapsed() < wait => std::thread::sleep(Duration::from_millis(25)),
            Err(_) => return Err(StoreError::Locked),
        }
    }
}
```

In `store.rs` add a `wait: Duration` field (default from `FACTORY_LOCK_WAIT`, else 15 s), `with_wait`, and at the top of `update`: `let _guard = lock_board(&self.board_path.with_extension("json.lock"), self.wait)?;`. The lock path is `board.json.lock`, which `with_extension("json.lock")` gives. The guard lives until the board is saved.

```rust
// lease.rs
use factory_core::Job;

use crate::{BoardStore, StoreError};

pub use factory_core::Lease;

#[derive(Debug, thiserror::Error)]
pub enum LeaseError {
    #[error("job is leased by {owner} for another {secs_left} seconds")]
    Held { owner: String, secs_left: u64 },
    #[error("{0}")]
    Store(#[from] StoreError),
}

pub fn lease_is_live(job: &Job, now: f64) -> bool {
    job.lease.as_ref().is_some_and(|l| l.expires > now)
}

fn check_free(job: &Job, owner: &str, now: f64) -> Result<(), LeaseError> {
    match &job.lease {
        Some(l) if l.expires > now && l.owner != owner => {
            Err(LeaseError::Held { owner: l.owner.clone(), secs_left: (l.expires - now) as u64 })
        }
        _ => Ok(()),
    }
}

/// Takes or renews the lease. An expired lease can be taken over by anyone.
pub fn acquire_lease(store: &BoardStore, id: &str, owner: &str, ttl: f64, now: f64) -> Result<(), LeaseError> {
    // The closure's own Result carries the lease verdict, so a refused lease still saves an unchanged board.
    let verdict = store.update_job(id, |job| {
        Ok(check_free(job, owner, now).map(|()| {
            job.lease = Some(Lease { owner: owner.to_string(), expires: now + ttl });
        }))
    })?;
    verdict
}

pub fn release_lease(store: &BoardStore, id: &str, owner: &str) -> Result<(), LeaseError> {
    let now = crate::now_secs();
    let verdict = store.update_job(id, |job| {
        Ok(check_free(job, owner, now).map(|()| {
            job.lease = None;
        }))
    })?;
    verdict
}
```

In `factory-core` add `pub struct Lease { pub owner: String, pub expires: f64 }` (Debug, Clone, PartialEq, Serialize, Deserialize, exported from `lib.rs`) and on `Job`: `#[serde(default, skip_serializing_if = "Option::is_none")] pub lease: Option<Lease>`, `#[serde(default, skip_serializing_if = "is_false")] pub cancelled: bool` and `#[serde(default, skip_serializing_if = "is_false")] pub cleaned: bool` (a private `fn is_false(b: &bool) -> bool`; `Job::new` sets `None`, `false`, `false`). The board still deserializes the real job 001 file thanks to the defaults, and a job without them serializes as before.

Plan 04's `ports.rs::update` must write through `BoardStore::update_job` (it can build the store with `BoardStore::new` over the board path's root) instead of `Board::load` and `save`, so every stage write takes the lock.

`run_job` renews the lease at each loop turn: in the CLI `run` command, call `acquire_lease(.., 1800.0, now)` before `run_job` and `release_lease` after, and a guard struct releases it on every exit path:

```rust
struct LeaseGuard<'a> { store: &'a factory_engine::BoardStore, id: String, owner: String }
impl Drop for LeaseGuard<'_> {
    fn drop(&mut self) {
        // A crashed driver's lease expires by itself, so a failed release is not fatal.
        let _ = factory_engine::release_lease(self.store, &self.id, &self.owner);
    }
}
```

- [ ] **Step 4: Run to verify the lock and lease tests pass**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test lock_lease`
Expected: 3 PASS. If the concurrency test is flaky the lock is wrong: do not add retries to the test.

- [ ] **Step 5: Write the failing CLI unit tests**

In `crates/factory-cli/src/commands.rs` and `status.rs` test modules:

```rust
#[test]
fn rework_notes_are_one_short_line() {
    assert!(validate_rework_note("all green").is_ok());
    assert!(validate_rework_note("").is_err());
    assert!(validate_rework_note("two\nlines").is_err());
    assert!(validate_rework_note(&"x".repeat(201)).is_err());
    assert!(validate_rework_note(&"x".repeat(200)).is_ok());
}

#[test]
fn status_shows_stage_round_reviews_tokens_and_series_position() {
    let mut board = factory_core::Board::default();
    let mut job = factory_core::Job::new("002", "d-002-x", "pagination", "2026-10-13", "f", 0.0);
    job.set_stage(factory_core::Stage::Review, None, false, 1.0).expect("stage");
    job.round = 1;
    job.series = Some(factory_core::SeriesPos { plan_file: "p".into(), index: 2, total: 5 });
    board.jobs.push(job);
    let out = render_status(&board, &factory_core::Config::default());
    let line = out.lines().nth(1).expect("row");
    for needle in ["002", "review", "1", "2/5", "pagination"] {
        assert!(line.contains(needle), "missing {needle} in {line}");
    }
    assert_eq!(render_status(&factory_core::Board::default(), &factory_core::Config::default()), "no jobs\n");
}
```

- [ ] **Step 6: Implement the CLI**

`cli.rs` (clap derive):

```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "factory", about = "Rocket software factory driver")]
pub struct Cli {
    /// Main checkout root. Defaults to the repository containing the current directory.
    #[arg(long, global = true)]
    pub root: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Create a job at stage build.
    New {
        slug: String,
        feature: String,
        /// A plan file or a folder of plan-NN files.
        #[arg(long)]
        plans: Option<PathBuf>,
        #[arg(long)]
        model: Option<String>,
    },
    /// Drive a job to merged or needs-human.
    Run {
        id: String,
        /// Continue with the next plan of the series after each merge.
        #[arg(long)]
        series: bool,
    },
    Status,
    Show { id: String },
    /// Merge a job in needs-human or approve.
    Approve { id: String },
    /// Send a one-line note to the builder and reset the round counter.
    Rework { id: String, note: String },
    Cancel {
        id: String,
        #[arg(long, default_value = "cancelled by user")]
        reason: String,
        #[arg(long)]
        force: bool,
    },
    /// Remove the worktree and branch of a merged or cancelled job.
    Cleanup {
        id: String,
        #[arg(long)]
        discard_commits: bool,
    },
    Tokens { id: String },
}
```

`commands.rs` core, one function per command, all taking `&CliCtx` and building an `Engine` with `ctx.runner`. Job ids are normalized with `format!("{:03}", n)` from digits. Key logic:

```rust
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("{0}")]
    Refused(String),
    #[error("{0}")]
    Engine(#[from] factory_engine::EngineError),
    #[error("{0}")]
    Store(#[from] factory_engine::StoreError),
    #[error("{0}")]
    Lease(#[from] factory_engine::LeaseError),
    #[error("{0}")]
    Git(#[from] factory_git::GitError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub fn validate_rework_note(note: &str) -> Result<(), CliError> {
    if note.trim().is_empty() {
        return Err(CliError::Refused("the rework note is empty".into()));
    }
    if note.contains('\n') || note.contains('\r') {
        return Err(CliError::Refused("the rework note must be one line".into()));
    }
    if note.chars().count() > 200 {
        return Err(CliError::Refused("the rework note is longer than 200 characters".into()));
    }
    Ok(())
}

fn rework(ctx: &CliCtx, id: &str, note: &str) -> Result<String, CliError> {
    validate_rework_note(note)?;
    let engine = ctx.engine()?;
    let job = engine.job(id)?;
    if job.stage != Stage::NeedsHuman {
        return Err(CliError::Refused(format!("job {id} is in stage {:?}, rework needs needs-human", job.stage)));
    }
    // The note is stored verbatim. The builder gets exactly this text.
    std::fs::write(engine.job_dir(&job).join("rework-note.md"), note)?;
    engine.ledger(&job).ruling(&format!("rework note: {note}"))?;
    let now = factory_engine::now_secs();
    engine.store().update_job(id, |j| {
        // Needs-human back to build is allowed by the stage guard, so no force. The note was checked to be one line.
        j.set_stage(Stage::Build, None, false, now)?;
        j.round = 0;
        Ok(())
    })?;
    Ok(format!("job {id} back to build, round reset to 0"))
}

fn cancel(ctx: &CliCtx, id: &str, reason: &str, force: bool) -> Result<String, CliError> {
    let engine = ctx.engine()?;
    let job = engine.job(id)?;
    if job.stage == Stage::Merged {
        return Err(CliError::Refused(format!("job {id} is already merged")));
    }
    if factory_engine::lease_is_live(&job, factory_engine::now_secs()) && !force {
        return Err(CliError::Refused("job has a live lease, use --force to cancel anyway".into()));
    }
    let text = format!("cancelled: {reason}");
    let now = factory_engine::now_secs();
    engine.store().update_job(id, |j| {
        j.set_stage(Stage::NeedsHuman, Some(&text), false, now)?;
        j.cancelled = true;
        j.lease = None;
        Ok(())
    })?;
    engine.ledger(&job).ruling(&text)?;
    Ok(format!("job {id} cancelled"))
}

fn cleanup(ctx: &CliCtx, id: &str, discard: bool) -> Result<String, CliError> {
    let engine = ctx.engine()?;
    let job = engine.job(id)?;
    if job.stage != Stage::Merged && !job.cancelled {
        return Err(CliError::Refused("cleanup only works on a merged or cancelled job".into()));
    }
    let repo = engine.repo();
    let jr = factory_git::JobRef { id: job.id.clone(), slug: job.slug.clone() };
    let wt = factory_git::Worktree {
        name: jr.dir_name(),
        path: job
            .worktree
            .as_deref()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| engine.root().join(".claude/worktrees").join(jr.dir_name())),
        branch: job.branch.clone(),
        base: job.base.clone().unwrap_or_default(),
    };
    // A head that is gone or equals the base has no commits to protect, so an error counts as merged.
    let unmerged = match (job.base.as_deref(), job.head.as_deref()) {
        (Some(b), Some(h)) => repo.is_merged(b, h).map(|m| m.is_none()).unwrap_or(false),
        _ => false,
    };
    if unmerged && !discard {
        return Err(CliError::Refused("the branch has commits that are not in main, use --discard-commits".into()));
    }
    // remove_worktree also deletes the job branch. It refuses a dirty worktree and unmerged commits unless forced.
    repo.remove_worktree(&wt, discard)
        .map_err(|e| CliError::Refused(format!("{e}, use --discard-commits to drop it")))?;
    engine.store().update_job(id, |j| j.cleaned = true)?;
    Ok(format!("removed worktree and branch of job {id}"))
}
```

`approve`: job must be `needs-human` or `approve` (else `Refused`); acquire lease; set stage `approve` with `Job::set_stage(Stage::Approve, None, false, now)` (clearing the reason); `merge_stage`; print `merged <sha>` or `parked: <reason>`. It also works after the user merged by hand, because `merge_stage` sees `is_merged`. `run`: acquire lease (owner `ctx.owner`, ttl 1800), then `run_job` or `run_series`, print `merged` or `needs-human: <reason>`. `new`: `create_job`, print the folder name. `show`: pretty JSON of the job. `tokens`: print the four `job.tokens` categories, `job.tokens.weighted()` and the percent of `budget_weighted`. `status` uses `render_status`:

```rust
pub fn render_status(board: &Board, cfg: &Config) -> String {
    if board.jobs.is_empty() {
        return "no jobs\n".to_string();
    }
    let mut out = String::from("ID   STAGE        ROUND  REVIEWS         TOKENS    SERIES  SLUG\n");
    for job in &board.jobs {
        let series = job.series.as_ref().map_or("-".to_string(), |s| format!("{}/{}", s.index, s.total));
        let weighted = job.tokens.weighted();
        let pct = if cfg.budget_weighted > 0 { weighted * 100 / cfg.budget_weighted } else { 0 };
        out.push_str(&format!(
            "{:<4} {:<12} {:<6} {:<15} {:<9} {:<7} {}\n",
            job.id,
            stage_name(job.stage),
            job.round,
            reviews_text(job),
            format!("{} ({pct}%)", human(weighted)),
            series,
            job.slug,
        ));
    }
    out
}
```

`human(n)` prints `850`, `12.3K`, `1.2M`. `reviews_text` prints the reviewer states from `job.reviews` as `code:pass` pairs, joined by spaces. `stage_name` uses the serde name (`needs-human`). `lib.rs` exports `Cli`, `CliCtx`, `CliError`, `dispatch`, `validate_rework_note`, `render_status`. `main.rs`:

```rust
use clap::Parser;
use factory_cli::{dispatch, Cli, CliCtx};

fn main() {
    let cli = Cli::parse();
    let ctx = match CliCtx::from_env(cli.root.clone()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("factory: {e}");
            std::process::exit(2);
        }
    };
    match dispatch(cli, &ctx) {
        Ok(text) => println!("{text}"),
        Err(e) => {
            eprintln!("factory: {e}");
            std::process::exit(1);
        }
    }
}
```

`CliCtx::from_env` finds the main checkout with `git2::Repository::discover(cwd)` then the parent of `commondir()`, leaves `runner` as `None`, and sets `owner = format!("driver-{}-{}", std::process::id(), factory_engine::now_secs() as u64)`. `CliCtx::runner_for(job_id)` returns the injected runner when there is one, else `Arc::new(ClaudeRunner::new("claude", Caps::default(), EventSink::new(root.join(".superpowers/factory/events.jsonl"), job_id)))`, because the sink stamps its job id on every event (the `agent_id` of each event still carries the right job prefix in a series). `CliCtx::new(root, runner, owner)` is used by tests.

- [ ] **Step 7: Core, git and engine adjustments**

- `factory-core` stage guard: nothing to add. Plan 01's `Stage::check_move` already lets any stage park in `needs-human` and lets `needs-human` resume at build, review or approve without force. The CLI always goes through `Job::set_stage`; only the sync step's approve-to-review move and a fix round pass `force = true`, and each logs a `Ruling:` line.
- `factory-git`: nothing to add. `Repo::remove_worktree(&wt, force)` already deletes the branch and refuses a dirty worktree or unmerged commits unless `force`, which is what `cleanup` needs.
- Plan 04's `build_stage` does not read `rework-note.md`, and after a rework the round counter is reset, so it would look for a missing `changes-r<round>.md`. Add it now in `build.rs`: on a resume, when `<job dir>/rework-note.md` exists, pass that file as the `findings` argument of `builder_resume_spec` (so the builder is told to fix exactly the note), and remove the file after the builder reply is handled. Test with plan 04's `ScriptedRunner` and its `last_spec()`, asserting the prompt names `rework-note.md`.

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-core` and `-p factory-engine`. Expected: PASS.

- [ ] **Step 8: Write the CLI integration test**

`crates/factory-cli/tests/new_to_merged.rs` builds a temp repo (initial commit on `main`, `.claude/factory/config.toml` with no `[[checks]]` and no matching risk path for `docs/`), a `ScriptedRunner` (a copy of plan 04's, kept in `factory-cli/tests/common/mod.rs`, because `FakeRunner` only replays fixed runs and cannot commit) whose builder effect commits `docs/hello.txt` in the worktree and whose reviewer effect replies with a last line `VERDICT: APPROVE`, and a `CliCtx::new(...)`. A helper `run(ctx, args: &[&str]) -> Result<String, CliError>` calls `Cli::parse_from(["factory", args..])` and `dispatch`.

```rust
#[test]
fn a_job_goes_from_new_to_merged_through_the_cli() {
    let fx = Fixture::new();
    let plans = fx.write_plan("plan-01-hello.md", 1, "none");
    fx.run(&["new", "hello", "say hello", "--plans", plans.to_str().expect("utf8")]).expect("new");
    let before = fx.main_tip();
    assert_eq!(fx.run(&["run", "1"]).expect("run"), "merged");
    assert_ne!(fx.main_tip(), before);
    assert!(fx.main_message().starts_with("chore(factory): merge job 001 hello"));
    assert!(fx.main_file("docs/hello.txt").exists());
    let status = fx.run(&["status"]).expect("status");
    assert!(status.contains("merged") && status.contains("1/1"));
    let show = fx.run(&["show", "1"]).expect("show");
    assert!(show.contains("\"stage\": \"merged\""));
    assert!(fx.run(&["tokens", "1"]).expect("tokens").contains("weighted"));
    fx.run(&["cleanup", "1"]).expect("cleanup");
    assert!(!fx.worktree_path(1).exists());
}

#[test]
fn approve_merges_a_risky_job_that_run_parked() {
    let fx = Fixture::with_risk_path("docs/");
    let plans = fx.write_plan("plan-01-hello.md", 1, "none");
    fx.run(&["new", "risky", "touch risky path", "--plans", plans.to_str().expect("utf8")]).expect("new");
    assert!(fx.run(&["run", "1"]).expect("run").starts_with("needs-human"));
    let before = fx.main_tip();
    assert_eq!(fx.main_tip(), before, "a parked job leaves main untouched");
    assert!(fx.run(&["approve", "1"]).expect("approve").starts_with("merged"));
}

#[test]
fn guard_rails_refuse_with_clear_messages() {
    let fx = Fixture::new();
    let plans = fx.write_plan("plan-01-hello.md", 1, "none");
    fx.run(&["new", "hello", "x", "--plans", plans.to_str().expect("utf8")]).expect("new");
    let e = fx.run(&["rework", "1", "note"]).expect_err("not needs-human");
    assert!(e.to_string().contains("needs-human"));
    let e = fx.run(&["cleanup", "1"]).expect_err("not merged");
    assert!(e.to_string().contains("merged or cancelled"));
    fx.run(&["cancel", "1"]).expect("cancel");
    let e = fx.run(&["rework", "1", "a\nb"]).expect_err("multi-line");
    assert!(e.to_string().contains("one line"));
    // A second driver cannot take a live lease.
    fx.hold_lease("001", "someone-else");
    let e = fx.run(&["run", "1"]).expect_err("leased");
    assert!(e.to_string().contains("leased by someone-else"));
}
```

`hold_lease` calls `acquire_lease` directly with another owner. In the last test the job is `needs-human` and cancelled, so `run` must still check the lease first; keep the lease check as the first thing `run` does.

- [ ] **Step 9: Run the CLI tests**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli`
Expected: unit tests and the 3 integration tests PASS. A `--plans` file whose `Next plan:` is `none` gives status series `1/1`.

- [ ] **Step 10: Final check and test run**

Run each, one at a time, never `--workspace`:

```bash
cargo check -j4 --manifest-path tools/factory/Cargo.toml
cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-core
cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-git
cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine
cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli
grep -rn "\.unwrap()" tools/factory/crates/*/src tools/factory/crates/factory-cli/src && echo "FOUND unwrap" || echo "no unwrap"
```

Expected: check clean, every test run green, and `no unwrap`. Also run `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-agent` and `-p factory-checks` once, because Task 1 and 3 touched shared core types.

- [ ] **Step 11: Commit**

Invoke skill `dev-workflow-skills:1-git-commit`, then:

```bash
git add tools/factory/crates/factory-cli tools/factory/crates/factory-engine tools/factory/crates/factory-core tools/factory/crates/factory-git tools/factory/Cargo.toml tools/factory/Cargo.lock && git commit --only -m "<message>" -- tools/factory/crates/factory-cli tools/factory/crates/factory-engine tools/factory/crates/factory-core tools/factory/crates/factory-git tools/factory/Cargo.toml tools/factory/Cargo.lock
```

---

## Self-review notes

- Spec plan 05 row: merge and sync (T1), series and run loop (T2), lock, lease, cancel, cleanup, tokens and the CLI (T3). Decision 10 (reviewed sha, preflight, fixed message, second-parent check, at most 2 syncs, conflict to needs-human) is in `merge_stage` and `sync_stage`. Decision 11 (one at a time, stop on needs-human) is in `run_series`. Decision 13 (only the driver writes ledger and board) holds: tick and ledger lines are written by `tick_plan` and `park`.
- Spec table names a `budget` command, the task for this plan says `tokens`, and this plan follows the task.
- Series parsing takes `(root, start)` instead of only a folder so `Next plan:` paths resolve against the repo root.
- Names used across tasks: `BoardStore::{new, load, update, update_job, with_wait}`, `merge_stage`, `sync_stage`, `park`, `MergeResult`, `SyncOutcome`, `parse_series`, `create_job`, `run_job`, `run_series`, `RunEnd`, `acquire_lease`, `release_lease`, `lease_is_live`, `validate_rework_note`, `render_status`, `CliCtx`, `dispatch`.

Next plan: docs/superpowers/plans/2026-10-13-software-factory-v3/plan-06-agents-command-cutover.md
