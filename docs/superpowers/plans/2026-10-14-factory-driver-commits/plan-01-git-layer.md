# Factory driver commits, plan 01: git layer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give `factory-git` the functions the driver needs to hand a worktree to the committer agent and to check what it did: stage everything not ignored, check the tree is clean, list commits above a floor, make a plain fallback commit, and report the staged analysis.

**Architecture:** All functions are methods on `Repo` that take a `&Worktree` (like `merge_main_into`) or a revision (like `log_oneline`), and use `git2` only. Three new source files: `stage.rs`, `commits.rs` and `report.rs`. Tests use the existing `tests/common` fixture with real temp repos.

**Tech Stack:** Rust 2021, `git2` 0.19, `tempfile`. Run from `tools/factory/`.

**Spec:** `docs/superpowers/specs/2026-10-14-factory-driver-commits-design.md`, decision 3 (staging), decision 7 (fallback commit), decision 12 (precomputed analysis), design section 3 (verification) and section 5 (git layer).

## Global Constraints

- Never shell out to `git`: `git2` only (`tools/factory/CLAUDE.md`).
- Always `cargo ... -j4`. Never `--workspace` or `--all`. Never set `CARGO_TARGET_DIR`.
- No `unwrap()` in production code. Tests may use `expect("...")`, as the existing tests do.
- Errors use `GitError` and `Result` from `crates/factory-git/src/error.rs`.
- Follow the existing style: doc comment on every public item, short sentences, tests in `crates/factory-git/tests/*.rs` using `tests/common/mod.rs`.
- Commit each task through the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths, and commit with a pathspec.

## Review Focus

- A deleted tracked file must be staged as a deletion and listed.
- A gitignored file (`*.log`) must never be staged.
- A path with spaces and non-ASCII characters must be staged and listed intact.
- Staging an already clean worktree must return an empty list and change nothing.
- `commit_staged` with nothing staged must be refused, not make an empty commit.
- `commits_above` with `floor == tip` must return an empty list.

---

### Task 1: Stage everything and check for a clean tree

**Files:**
- Create: `tools/factory/crates/factory-git/src/stage.rs`
- Modify: `tools/factory/crates/factory-git/src/lib.rs`
- Test: `tools/factory/crates/factory-git/tests/stage.rs`

**Interfaces:**
- Consumes: `Repo`, `Worktree` (`path: PathBuf`), `Result`.
- Produces:
  - `Repo::stage_all(&self, wt: &Worktree) -> Result<Vec<String>>`: stages new, modified and deleted files that are not ignored, returns the sorted staged paths.
  - `Repo::worktree_is_clean(&self, wt: &Worktree) -> Result<bool>`: true when there are no staged, unstaged or untracked-not-ignored changes.
  - `pub(crate) fn staged_entries(inside: &Repository) -> Result<Vec<(&'static str, String)>>`: label (`new file:`, `modified:`, `deleted:`, `renamed:`, `typechange:`) and path of every staged change, sorted by path. Task 3 uses it.

- [ ] **Step 1: Write the failing tests**

Create `tools/factory/crates/factory-git/tests/stage.rs`:

```rust
mod common;

use common::*;
use factory_git::{JobRef, Repo, Worktree};
use git2::Repository;

fn job_worktree(fx: &Fixture) -> (Repo, Worktree) {
    let repo = fx.repo();
    let wt = repo
        .create_worktree(&JobRef { id: "001".into(), slug: "demo".into() })
        .expect("create worktree");
    (repo, wt)
}

fn inside(wt: &Worktree) -> Repository {
    Repository::open(&wt.path).expect("open worktree")
}

#[test]
fn stage_all_stages_new_modified_and_deleted_files() {
    let fx = Fixture::new();
    let (repo, wt) = job_worktree(&fx);
    commit_files(&inside(&wt), &[("keep.txt", "one\n")], "add keep");
    std::fs::write(wt.path.join("keep.txt"), "one\ntwo\n").expect("modify");
    std::fs::write(wt.path.join("new.txt"), "new\n").expect("new");
    std::fs::remove_file(wt.path.join("base.txt")).expect("delete");

    let staged = repo.stage_all(&wt).expect("stage");
    assert_eq!(staged, vec!["base.txt", "keep.txt", "new.txt"]);
    assert!(!repo.worktree_is_clean(&wt).expect("clean check"));
}

#[test]
fn stage_all_skips_gitignored_files() {
    let fx = Fixture::new();
    let (repo, wt) = job_worktree(&fx);
    std::fs::write(wt.path.join(".gitignore"), "*.log\n").expect("gitignore");
    std::fs::write(wt.path.join("scratch.log"), "noise\n").expect("log");
    std::fs::write(wt.path.join("kept.txt"), "keep\n").expect("kept");

    let staged = repo.stage_all(&wt).expect("stage");
    assert_eq!(staged, vec![".gitignore", "kept.txt"]);
}

#[test]
fn stage_all_keeps_spaces_and_unicode_in_paths() {
    let fx = Fixture::new();
    let (repo, wt) = job_worktree(&fx);
    std::fs::create_dir_all(wt.path.join("dir with space")).expect("mkdir");
    std::fs::write(wt.path.join("dir with space").join("ünï.txt"), "x\n").expect("write");

    let staged = repo.stage_all(&wt).expect("stage");
    assert_eq!(staged, vec!["dir with space/ünï.txt"]);
}

#[test]
fn stage_all_on_a_clean_worktree_returns_nothing() {
    let fx = Fixture::new();
    let (repo, wt) = job_worktree(&fx);
    assert!(repo.stage_all(&wt).expect("stage").is_empty());
    assert!(repo.worktree_is_clean(&wt).expect("clean check"));
}

#[test]
fn worktree_is_clean_ignores_ignored_files_and_sees_untracked_ones() {
    let fx = Fixture::new();
    let (repo, wt) = job_worktree(&fx);
    commit_files(&inside(&wt), &[(".gitignore", "*.log\n")], "ignore logs");
    std::fs::write(wt.path.join("noise.log"), "x\n").expect("log");
    assert!(repo.worktree_is_clean(&wt).expect("ignored only"));

    std::fs::write(wt.path.join("stray.txt"), "x\n").expect("stray");
    assert!(!repo.worktree_is_clean(&wt).expect("untracked"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd tools/factory && cargo test -j4 -p factory-git --test stage`
Expected: FAIL to compile, `no method named stage_all found for struct Repo`.

- [ ] **Step 3: Write the implementation**

Create `tools/factory/crates/factory-git/src/stage.rs`:

```rust
use crate::error::Result;
use crate::repo::Repo;
use crate::worktree::Worktree;
use git2::{IndexAddOption, Repository, Status, StatusOptions};

/// Options that see untracked files (but not ignored ones), like `git status`.
fn status_options() -> StatusOptions {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true).include_ignored(false);
    opts
}

/// Label and path of every staged change, sorted by path.
pub(crate) fn staged_entries(inside: &Repository) -> Result<Vec<(&'static str, String)>> {
    let mut entries = Vec::new();
    for entry in inside.statuses(Some(&mut status_options()))?.iter() {
        let status = entry.status();
        let label = if status.contains(Status::INDEX_NEW) {
            "new file:"
        } else if status.contains(Status::INDEX_DELETED) {
            "deleted:"
        } else if status.contains(Status::INDEX_RENAMED) {
            "renamed:"
        } else if status.contains(Status::INDEX_TYPECHANGE) {
            "typechange:"
        } else if status.contains(Status::INDEX_MODIFIED) {
            "modified:"
        } else {
            continue;
        };
        if let Some(path) = entry.path() {
            entries.push((label, path.to_string()));
        }
    }
    entries.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(entries)
}

impl Repo {
    /// Stages every change in the job worktree that is not ignored: new, modified
    /// and deleted files. Returns the sorted staged paths. The worktree belongs to
    /// one job and one implementer, so every change belongs to the current task.
    pub fn stage_all(&self, wt: &Worktree) -> Result<Vec<String>> {
        let inside = Repository::open(&wt.path)?;
        let mut index = inside.index()?;
        index.add_all(["*"], IndexAddOption::DEFAULT, None)?;
        index.update_all(["*"], None)?;
        index.write()?;
        Ok(staged_entries(&inside)?.into_iter().map(|(_, path)| path).collect())
    }

    /// True when the worktree has no staged, unstaged or untracked (not ignored)
    /// changes.
    pub fn worktree_is_clean(&self, wt: &Worktree) -> Result<bool> {
        let inside = Repository::open(&wt.path)?;
        let clean = inside.statuses(Some(&mut status_options()))?.is_empty();
        Ok(clean)
    }
}
```

In `tools/factory/crates/factory-git/src/lib.rs`, add `mod stage;` after `mod repo;` (keep the list sorted: `diff`, `error`, `merge`, `repo`, `stage`, `state`, `worktree`).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd tools/factory && cargo test -j4 -p factory-git --test stage`
Expected: PASS, 5 tests.

- [ ] **Step 5: Run the whole crate and check for warnings**

Run: `cd tools/factory && cargo test -j4 -p factory-git && cargo clippy -j4 -p factory-git -- -D warnings`
Expected: all tests pass, no clippy output.

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill, with these paths only:

```bash
git add tools/factory/crates/factory-git/src/stage.rs tools/factory/crates/factory-git/src/lib.rs tools/factory/crates/factory-git/tests/stage.rs
```

Expected subject: `feat(factory): stage a worktree and check it is clean`.

---

### Task 2: Commits above a floor and a fallback commit

**Files:**
- Create: `tools/factory/crates/factory-git/src/commits.rs`
- Modify: `tools/factory/crates/factory-git/src/lib.rs`, `tools/factory/crates/factory-git/src/merge.rs:22` (make `signature` `pub(crate)`)
- Test: `tools/factory/crates/factory-git/tests/commits.rs`

**Interfaces:**
- Consumes: `Repo::commit_of`, `repo::paths_of`, `merge::signature` (made `pub(crate)` here), `Repo::stage_all` from Task 1.
- Produces:
  - `pub struct CommitInfo { pub sha: String, pub subject: String, pub files: Vec<String> }`, exported from the crate root.
  - `Repo::commits_above(&self, floor: &str, tip: &str) -> Result<Vec<CommitInfo>>`: the commits in `floor..tip`, oldest first, each with the files it changed (against its first parent).
  - `Repo::commit_staged(&self, wt: &Worktree, message: &str) -> Result<String>`: commits the index of the worktree on its current branch and returns the new sha. Refused when nothing is staged.

- [ ] **Step 1: Write the failing tests**

Create `tools/factory/crates/factory-git/tests/commits.rs`:

```rust
mod common;

use common::*;
use factory_git::{GitError, JobRef, Repo, Worktree};
use git2::Repository;

fn job_worktree(fx: &Fixture) -> (Repo, Worktree) {
    let repo = fx.repo();
    let wt = repo
        .create_worktree(&JobRef { id: "001".into(), slug: "demo".into() })
        .expect("create worktree");
    (repo, wt)
}

fn inside(wt: &Worktree) -> Repository {
    Repository::open(&wt.path).expect("open worktree")
}

#[test]
fn commits_above_lists_commits_oldest_first_with_their_files() {
    let fx = Fixture::new();
    let (repo, wt) = job_worktree(&fx);
    commit_files(&inside(&wt), &[("a.txt", "a\n")], "feat(x): add a");
    commit_files(&inside(&wt), &[("b.txt", "b\n"), ("c.txt", "c\n")], "feat(x): add b and c");
    let tip = repo.tip(&wt.branch).expect("tip");

    let commits = repo.commits_above(&wt.base, &tip).expect("commits");
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].subject, "feat(x): add a");
    assert_eq!(commits[0].files, vec!["a.txt"]);
    assert_eq!(commits[1].subject, "feat(x): add b and c");
    assert_eq!(commits[1].files, vec!["b.txt", "c.txt"]);
    assert_eq!(commits[1].sha, tip);
}

#[test]
fn commits_above_is_empty_when_floor_equals_tip() {
    let fx = Fixture::new();
    let (repo, wt) = job_worktree(&fx);
    assert!(repo.commits_above(&wt.base, &wt.base).expect("commits").is_empty());
}

#[test]
fn commit_staged_commits_the_index_and_returns_the_new_tip() {
    let fx = Fixture::new();
    let (repo, wt) = job_worktree(&fx);
    std::fs::write(wt.path.join("new.txt"), "new\n").expect("write");
    repo.stage_all(&wt).expect("stage");

    let sha = repo.commit_staged(&wt, "chore: add new\n\nbody line").expect("commit");
    assert_eq!(sha, repo.tip(&wt.branch).expect("tip"));
    let commits = repo.commits_above(&wt.base, &sha).expect("commits");
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].files, vec!["new.txt"]);
    let message = inside(&wt).head().expect("head").peel_to_commit().expect("commit").message().map(str::to_string);
    assert_eq!(message.as_deref(), Some("chore: add new\n\nbody line"));
    assert!(repo.worktree_is_clean(&wt).expect("clean"));
}

#[test]
fn commit_staged_with_nothing_staged_is_refused() {
    let fx = Fixture::new();
    let (repo, wt) = job_worktree(&fx);
    let before = repo.tip(&wt.branch).expect("tip");
    assert!(matches!(repo.commit_staged(&wt, "chore: nothing"), Err(GitError::Refused(_))));
    assert_eq!(repo.tip(&wt.branch).expect("tip"), before);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd tools/factory && cargo test -j4 -p factory-git --test commits`
Expected: FAIL to compile, `no method named commits_above found for struct Repo`.

- [ ] **Step 3: Write the implementation**

In `tools/factory/crates/factory-git/src/merge.rs` change line 22 from `fn signature(` to `pub(crate) fn signature(`.

Create `tools/factory/crates/factory-git/src/commits.rs`:

```rust
use crate::error::{GitError, Result};
use crate::merge::signature;
use crate::repo::{paths_of, Repo};
use crate::worktree::Worktree;
use git2::{Repository, Sort};

/// One commit above a floor: its sha, subject line and changed files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInfo {
    pub sha: String,
    pub subject: String,
    pub files: Vec<String>,
}

impl Repo {
    /// The commits in `floor..tip`, oldest first, each with the files it changed
    /// against its first parent. Empty when `floor` is `tip` or one of its
    /// descendants.
    pub fn commits_above(&self, floor: &str, tip: &str) -> Result<Vec<CommitInfo>> {
        let mut walk = self.inner.revwalk()?;
        walk.push(self.commit_of(tip)?.id())?;
        walk.hide(self.commit_of(floor)?.id())?;
        walk.set_sorting(Sort::TOPOLOGICAL | Sort::REVERSE)?;
        let mut commits = Vec::new();
        for oid in walk {
            let commit = self.inner.find_commit(oid?)?;
            let tree = commit.tree()?;
            let parent_tree = match commit.parent(0) {
                Ok(parent) => Some(parent.tree()?),
                Err(_) => None,
            };
            let diff = self.inner.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None)?;
            commits.push(CommitInfo {
                sha: commit.id().to_string(),
                subject: commit.summary().unwrap_or("").to_string(),
                files: paths_of(&diff),
            });
        }
        Ok(commits)
    }

    /// Commits the index of the job worktree on its current branch and returns the
    /// new sha. This is the plain `git2` fallback commit, used when the committer
    /// agent fails. Refused when nothing is staged.
    pub fn commit_staged(&self, wt: &Worktree, message: &str) -> Result<String> {
        let inside = Repository::open(&wt.path)?;
        let mut index = inside.index()?;
        let tree = inside.find_tree(index.write_tree()?)?;
        let parent = inside.head()?.peel_to_commit()?;
        if parent.tree_id() == tree.id() {
            return Err(GitError::Refused("nothing is staged to commit".into()));
        }
        let sig = signature(&inside)?;
        let id = inside.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])?;
        Ok(id.to_string())
    }
}
```

In `lib.rs` add `mod commits;` (after `mod error;`... keep sorted: `commits`, `diff`, ...) and `pub use commits::CommitInfo;`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd tools/factory && cargo test -j4 -p factory-git --test commits`
Expected: PASS, 4 tests.

- [ ] **Step 5: Run the whole crate and check for warnings**

Run: `cd tools/factory && cargo test -j4 -p factory-git && cargo clippy -j4 -p factory-git -- -D warnings`
Expected: all tests pass, no clippy output.

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill, with these paths only:

```bash
git add tools/factory/crates/factory-git/src/commits.rs tools/factory/crates/factory-git/src/lib.rs tools/factory/crates/factory-git/src/merge.rs tools/factory/crates/factory-git/tests/commits.rs
```

Expected subject: `feat(factory): list commits above a floor and commit staged`.

---

### Task 3: The staged analysis report

**Files:**
- Create: `tools/factory/crates/factory-git/src/report.rs`
- Modify: `tools/factory/crates/factory-git/src/lib.rs`
- Test: `tools/factory/crates/factory-git/tests/report.rs`

**Interfaces:**
- Consumes: `stage::staged_entries` from Task 1, `repo::short`.
- Produces:
  - `pub struct StagedReport { pub status: String, pub stat: String, pub diff: String, pub log: Vec<String> }`, exported from the crate root.
  - `Repo::staged_report(&self, wt: &Worktree, log_count: usize) -> Result<StagedReport>`: the output of the skill's step 1 commands, computed through `git2`. `status` mimics `git status` (branch line and the `Changes to be committed:` list), `stat` is `git diff --staged --stat`, `diff` is `git diff --staged`, `log` is `git log --oneline -<log_count>` as `<short sha> <subject>` lines, newest first. The driver puts these in the committer prompt (spec decision 12). A cap on the diff size is the committer module's job (plan 03), not this function's.

- [ ] **Step 1: Write the failing tests**

Create `tools/factory/crates/factory-git/tests/report.rs`:

```rust
mod common;

use common::*;
use factory_git::{JobRef, Repo, Worktree};
use git2::Repository;

fn job_worktree(fx: &Fixture) -> (Repo, Worktree) {
    let repo = fx.repo();
    let wt = repo
        .create_worktree(&JobRef { id: "001".into(), slug: "demo".into() })
        .expect("create worktree");
    (repo, wt)
}

fn inside(wt: &Worktree) -> Repository {
    Repository::open(&wt.path).expect("open worktree")
}

#[test]
fn staged_report_describes_the_staged_change() {
    let fx = Fixture::new();
    let (repo, wt) = job_worktree(&fx);
    commit_files(&inside(&wt), &[("keep.txt", "one\n")], "feat(x): add keep");
    std::fs::write(wt.path.join("keep.txt"), "one\ntwo\n").expect("modify");
    std::fs::write(wt.path.join("new.txt"), "new\n").expect("new");
    repo.stage_all(&wt).expect("stage");

    let report = repo.staged_report(&wt, 5).expect("report");
    assert!(report.status.starts_with("On branch factory/001-demo\n"), "status: {}", report.status);
    assert!(report.status.contains("Changes to be committed:"), "status: {}", report.status);
    assert!(report.status.contains("\tmodified:   keep.txt\n"), "status: {}", report.status);
    assert!(report.status.contains("\tnew file:   new.txt\n"), "status: {}", report.status);
    assert!(report.stat.contains("keep.txt"), "stat: {}", report.stat);
    assert!(report.diff.contains("+++ b/keep.txt"), "diff: {}", report.diff);
    assert!(report.diff.contains("+two\n"), "diff: {}", report.diff);
    assert!(report.log[0].ends_with("feat(x): add keep"), "log: {:?}", report.log);
}

#[test]
fn staged_report_log_is_newest_first_and_capped() {
    let fx = Fixture::new();
    let (repo, wt) = job_worktree(&fx);
    commit_files(&inside(&wt), &[("a.txt", "a\n")], "feat(x): one");
    commit_files(&inside(&wt), &[("b.txt", "b\n")], "feat(x): two");

    let report = repo.staged_report(&wt, 2).expect("report");
    assert_eq!(report.log.len(), 2);
    assert!(report.log[0].ends_with("feat(x): two"));
    assert!(report.log[1].ends_with("feat(x): one"));
}

#[test]
fn staged_report_with_nothing_staged_has_an_empty_diff() {
    let fx = Fixture::new();
    let (repo, wt) = job_worktree(&fx);
    let report = repo.staged_report(&wt, 5).expect("report");
    assert!(report.status.contains("nothing to commit"), "status: {}", report.status);
    assert!(report.diff.is_empty());
    assert!(report.stat.is_empty());
}

#[test]
fn staged_report_shows_a_deletion() {
    let fx = Fixture::new();
    let (repo, wt) = job_worktree(&fx);
    std::fs::remove_file(wt.path.join("base.txt")).expect("delete");
    repo.stage_all(&wt).expect("stage");

    let report = repo.staged_report(&wt, 5).expect("report");
    assert!(report.status.contains("\tdeleted:    base.txt\n"), "status: {}", report.status);
    assert!(report.diff.contains("-base\n"), "diff: {}", report.diff);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd tools/factory && cargo test -j4 -p factory-git --test report`
Expected: FAIL to compile, `no method named staged_report found for struct Repo`.

- [ ] **Step 3: Write the implementation**

Create `tools/factory/crates/factory-git/src/report.rs`:

```rust
use crate::error::Result;
use crate::repo::{short, Repo};
use crate::stage::staged_entries;
use crate::worktree::Worktree;
use git2::{Diff, DiffFormat, DiffStatsFormat, Repository, Sort};

/// The output of the commit skill's step 1 commands for a worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedReport {
    /// What `git status` would print for the staged changes.
    pub status: String,
    /// `git diff --staged --stat`.
    pub stat: String,
    /// `git diff --staged`.
    pub diff: String,
    /// `git log --oneline -N`, newest first, as `<short sha> <subject>`.
    pub log: Vec<String>,
}

/// Renders a diff as patch text, with the `+`, `-` and space prefixes git prints.
fn render_patch(diff: &Diff<'_>) -> Result<String> {
    let mut out: Vec<u8> = Vec::new();
    diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
        if matches!(line.origin(), '+' | '-' | ' ') {
            out.push(line.origin() as u8);
        }
        out.extend_from_slice(line.content());
        true
    })?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}

fn recent_log(inside: &Repository, count: usize) -> Result<Vec<String>> {
    let mut walk = inside.revwalk()?;
    walk.push_head()?;
    walk.set_sorting(Sort::TOPOLOGICAL)?;
    let mut lines = Vec::new();
    for oid in walk.take(count) {
        let commit = inside.find_commit(oid?)?;
        let id = commit.id().to_string();
        lines.push(format!("{} {}", short(&id), commit.summary().unwrap_or("")));
    }
    Ok(lines)
}

impl Repo {
    /// Computes, through `git2`, what the skill's step 1 commands print for the
    /// staged changes of a job worktree, so the driver can put them in the
    /// committer prompt.
    pub fn staged_report(&self, wt: &Worktree, log_count: usize) -> Result<StagedReport> {
        let inside = Repository::open(&wt.path)?;
        let head = inside.head()?;
        let branch = head.shorthand().unwrap_or("HEAD").to_string();
        let mut status = format!("On branch {branch}\n");
        let entries = staged_entries(&inside)?;
        if entries.is_empty() {
            status.push_str("nothing to commit\n");
        } else {
            status.push_str("Changes to be committed:\n");
            for (label, path) in &entries {
                status.push_str(&format!("\t{label:<12}{path}\n"));
            }
        }
        let head_tree = head.peel_to_commit()?.tree()?;
        let index = inside.index()?;
        let diff = inside.diff_tree_to_index(Some(&head_tree), Some(&index), None)?;
        let stat = if entries.is_empty() {
            String::new()
        } else {
            let buf = diff.stats()?.to_buf(DiffStatsFormat::FULL, 80)?;
            String::from_utf8_lossy(&buf[..]).into_owned()
        };
        Ok(StagedReport { status, stat, diff: render_patch(&diff)?, log: recent_log(&inside, log_count)? })
    }
}
```

In `lib.rs` add `mod report;` (after `mod repo;`) and `pub use report::StagedReport;`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd tools/factory && cargo test -j4 -p factory-git --test report`
Expected: PASS, 4 tests.

- [ ] **Step 5: Run the whole crate and check for warnings**

Run: `cd tools/factory && cargo test -j4 -p factory-git && cargo clippy -j4 -p factory-git -- -D warnings`
Expected: all tests pass, no clippy output. If clippy flags `format!` appended to a `String`, switch those lines to `use std::fmt::Write` and `writeln!`, and run the tests again.

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill, with these paths only:

```bash
git add tools/factory/crates/factory-git/src/report.rs tools/factory/crates/factory-git/src/lib.rs tools/factory/crates/factory-git/tests/report.rs
```

Expected subject: `feat(factory): report the staged analysis through git2`.

Next plan: `plan-02-runner-config-agent.md`, written after this plan is merged.
