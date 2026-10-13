# Software Factory v3, Plan 02: The git2 Layer (`factory-git`)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. It is executed by a `factory-builder` subagent. Skip that skill's final whole-branch review, its ledger scripts and its progress file: the driver owns the ledger. Steps use checkbox (`- [ ]`) syntax. **Plan 01 must be finished and merged first.** It created the `tools/factory/` workspace. This plan needs only that workspace, not any `factory-core` type. If `tools/factory/Cargo.toml` does not exist, stop and report it.

**Goal:** Build the `factory-git` crate: every git operation the driver needs (diff files, ancestry, merged check, preflight, base-state, reviewed-sha no-ff merge, worktree create and remove, sync of main into a job branch), on `git2`, with no git CLI.

**Architecture:** One `Repo` struct wraps a `git2::Repository` opened on the main checkout. Merges are computed in memory (`merge_commits`), the two-parent commit is created without touching any ref, parents are verified, then a safe checkout updates the working tree and only then the branch ref moves. Any refusal, conflict or failure returns before the ref moves, so main stays untouched. Conflicts and refusals are returned as `MergeOutcome` values, not errors. Diffs are written straight to files and never returned as text.

**Tech Stack:** Rust, `git2` 0.19 (no default features, so no OpenSSL), `thiserror` 1, `tempfile` 3 (dev).

**Spec:** [docs/superpowers/specs/2026-10-13-software-factory-v3-rust-driver-design.md](../../specs/2026-10-13-software-factory-v3-rust-driver-design.md) (crate layout, plan 02 row of the interface table, decision 10). Behavior ported from `.claude/commands/factory.md` (merge step), `.claude/factory/factory-ctl` (`is-merged`, `root`), v2 plan 03 (preflight, base-state, sync, cleanup rules) and v2 spec section C (gaps C1, C2, C4, C5).

## Global Constraints

- Own workspace at `tools/factory/` (not in the root workspace, own target dir). Pinned crates: `git2` 0.19, `thiserror` 1, `tempfile` 3 (dev). No `fs2` here.
- Never shell out to git. Never `.unwrap()` anywhere (a harness hook blocks it in any Rust write). `.expect("...")` only in tests. Production code returns `Result<_, GitError>`.
- Commands: `cargo check -j4 --manifest-path tools/factory/Cargo.toml` and `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-git <test_name>`. Never `--workspace`, never omit `-j4`, never set `CARGO_TARGET_DIR`.
- All tests use temp repos built with git2 in `tempfile` dirs. Never touch the real repo, its branches or its worktrees.
- The factory never pushes, never rebases, never rewrites history.
- Comments are short full sentences that end with a period. No emojis.
- Commit steps: invoke the skill `dev-workflow-skills:1-git-commit` for the message, but run its git commands inline with no helper agents. Use a pathspec commit: `git add <paths> && git commit --only -m "<message>" -- <paths>`. Never `git add -A` or `.`. Never set `HARNESS_GATE=0`. The commit gate runs `cargo check -j4` for staged `.rs` files.

## Review Focus

- The job branch moves after review (the builder or a person commits again): the merge must refuse and leave main untouched. Pinned in Task 2.
- An untracked file in the main checkout that the merge would overwrite: the merge must return a refusal value, keep that file and leave main at the same sha with no merge in progress. Pinned in Task 2.
- A job base that is not on main (unrelated commits baked in): `base_state` must say `BaseNotMain`, never `Current` or `MainMoved`, so it can never be synced into main. Pinned in Task 2.
- A worktree with untracked work or unmerged commits, or one named after main or the main checkout: `remove_worktree` must refuse and leave everything in place. Pinned in Task 3.
- A hostile or sloppy slug (`../x`, spaces, upper case): `create_worktree` must reject it before touching the disk. Pinned in Task 3.

## File structure

All paths are under `tools/factory/crates/factory-git/` (this is the layout plan 01 used; if its workspace `members` list a different path for factory-git, use that path and keep the relative names).

- `Cargo.toml`: crate manifest.
- `src/lib.rs`: module list and re-exports.
- `src/error.rs`: `GitError`, `Result`.
- `src/repo.rs`: `Repo`, `open`, `tip`, `is_ancestor`, `is_merged`, `changed_paths`, shared helpers.
- `src/diff.rs`: `DiffFiles` and `diff_to_files`.
- `src/state.rs`: `Preflight`, `BaseState`, `BaseCheck`, `preflight_main`, `base_state`.
- `src/merge.rs`: `MergeOutcome`, the shared `commit_merge`, `merge_no_ff`.
- `src/worktree.rs`: `JobRef`, `Worktree`, `create_worktree`, `remove_worktree`, `merge_main_into`.
- `tests/common/mod.rs`: temp repo fixture helpers.
- `tests/repo_basics.rs` (Task 1), `tests/merge.rs` (Task 2), `tests/worktree.rs` (Task 3).

---

### Task 1: `Repo`, `tip`, `changed_paths`, `diff_to_files`, `is_ancestor`, `is_merged`

**Files:**
- Create or fill: `tools/factory/crates/factory-git/Cargo.toml`, `src/lib.rs`, `src/error.rs`, `src/repo.rs`, `src/diff.rs`, `tests/common/mod.rs`, `tests/repo_basics.rs`
- Modify (only if `members` is not a `crates/*` glob): `tools/factory/Cargo.toml`

**Interfaces:**
- Consumes: nothing from other plans.
- Produces (exact signatures):
  - `pub enum GitError { Git(git2::Error), Io(std::io::Error), NotFound(String), Refused(String), BadInput(String) }`, `pub type Result<T> = std::result::Result<T, GitError>`.
  - `pub const MAIN_BRANCH: &str = "main";`
  - `Repo::open(path: &Path) -> Result<Repo>` (finds the main checkout even when `path` is inside a linked worktree), `Repo::root(&self) -> &Path`.
  - `Repo::tip(&self, branch: &str) -> Result<String>` (full sha of `refs/heads/<branch>`, `NotFound` if missing).
  - `Repo::changed_paths(&self, base: &str, head: &str) -> Result<Vec<String>>` (sorted, deduplicated, both sides of a rename).
  - `Repo::diff_to_files(&self, base: &str, head: &str, dir: &Path) -> Result<DiffFiles>`.
  - `pub struct DiffFiles { pub diff: PathBuf, pub stat: PathBuf, pub names: PathBuf, pub files: usize, pub insertions: usize, pub deletions: usize }` with `fn changed_lines(&self) -> usize`.
  - `Repo::is_ancestor(&self, older: &str, newer: &str) -> Result<bool>` (a commit is its own ancestor).
  - `Repo::is_merged(&self, base: &str, head: &str) -> Result<Option<String>>` (`Some(main sha)` when merged, `None` when not, `Err(Refused)` for an empty head or a job with no commits).

- [ ] **Step 1: Create the manifest and a stub that compiles**

Open `tools/factory/Cargo.toml` and check `members`. If it lists `crates/*` or already lists `factory-git`, leave it. Otherwise add `"crates/factory-git"`.

`tools/factory/crates/factory-git/Cargo.toml` (if plan 01 left a stub, replace its contents):

```toml
[package]
name = "factory-git"
version = "0.1.0"
edition = "2021"

[dependencies]
git2 = { version = "0.19", default-features = false }
thiserror = "1"

[dev-dependencies]
tempfile = "3"
```

`src/lib.rs`:

```rust
//! Git layer for the factory driver. Built on git2, never shells out to git.

mod diff;
mod error;
mod repo;

pub use diff::DiffFiles;
pub use error::{GitError, Result};
pub use repo::{Repo, MAIN_BRANCH};
```

`src/error.rs`:

```rust
/// Errors from the git layer.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git error: {0}")]
    Git(#[from] git2::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("refused: {0}")]
    Refused(String),
    #[error("bad input: {0}")]
    BadInput(String),
}

pub type Result<T> = std::result::Result<T, GitError>;
```

- [ ] **Step 2: Write the test fixture helpers**

`tests/common/mod.rs`:

```rust
#![allow(dead_code)]

use git2::{BranchType, Commit, Oid, Repository, RepositoryInitOptions, Signature};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A temp repo with one commit on `main` and a file `base.txt`.
pub struct Fixture {
    pub dir: TempDir,
    pub path: PathBuf,
}

impl Fixture {
    pub fn new() -> Fixture {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("repo");
        std::fs::create_dir_all(&path).expect("mkdir");
        let mut opts = RepositoryInitOptions::new();
        opts.initial_head("main");
        let repo = Repository::init_opts(&path, &opts).expect("init");
        commit_files(&repo, &[("base.txt", "base\n")], "init");
        Fixture { dir, path }
    }

    pub fn git(&self) -> Repository {
        Repository::open(&self.path).expect("open git")
    }

    pub fn repo(&self) -> factory_git::Repo {
        factory_git::Repo::open(&self.path).expect("open repo")
    }

    pub fn main_sha(&self) -> String {
        branch_sha(&self.git(), "main")
    }

    /// Creates `branch` from main and commits `files` onto it. Returns (base, head).
    pub fn job_branch(&self, branch: &str, files: &[(&str, &str)]) -> (String, String) {
        let git = self.git();
        let base = self.main_sha();
        let main = git.find_commit(Oid::from_str(&base).expect("oid")).expect("commit");
        git.branch(branch, &main, false).expect("branch");
        let head = commit_to_branch(&git, branch, files, "job work");
        (base, head.to_string())
    }
}

pub fn sig() -> Signature<'static> {
    Signature::now("t", "t@t").expect("sig")
}

pub fn branch_sha(repo: &Repository, branch: &str) -> String {
    let b = repo.find_branch(branch, BranchType::Local).expect("branch");
    b.get().peel_to_commit().expect("commit").id().to_string()
}

/// Writes files into the workdir, stages them and commits on HEAD.
pub fn commit_files(repo: &Repository, files: &[(&str, &str)], msg: &str) -> Oid {
    let workdir = repo.workdir().expect("workdir");
    let mut index = repo.index().expect("index");
    for (p, c) in files {
        std::fs::write(workdir.join(p), c).expect("write");
        index.add_path(Path::new(p)).expect("add");
    }
    index.write().expect("write index");
    let tree = repo.find_tree(index.write_tree().expect("tree")).expect("find tree");
    let parents: Vec<Commit> = match repo.head() {
        Ok(h) => vec![h.peel_to_commit().expect("head commit")],
        Err(_) => vec![],
    };
    let refs: Vec<&Commit> = parents.iter().collect();
    repo.commit(Some("HEAD"), &sig(), &sig(), msg, &tree, &refs).expect("commit")
}

/// Commits flat files onto a branch without touching any working tree.
pub fn commit_to_branch(repo: &Repository, branch: &str, files: &[(&str, &str)], msg: &str) -> Oid {
    let b = repo.find_branch(branch, BranchType::Local).expect("branch");
    let parent = b.get().peel_to_commit().expect("parent");
    let mut tb = repo.treebuilder(Some(&parent.tree().expect("tree"))).expect("treebuilder");
    for (p, c) in files {
        let blob = repo.blob(c.as_bytes()).expect("blob");
        tb.insert(p, blob, 0o100644).expect("insert");
    }
    let tree = repo.find_tree(tb.write().expect("write tree")).expect("find tree");
    let name = format!("refs/heads/{branch}");
    repo.commit(Some(&name), &sig(), &sig(), msg, &tree, &[&parent]).expect("commit")
}

/// Moves a branch ref to a commit (a stand-in for a manual fast-forward merge).
pub fn point_branch_at(repo: &Repository, branch: &str, sha: &str) {
    let name = format!("refs/heads/{branch}");
    repo.reference(&name, Oid::from_str(sha).expect("oid"), true, "test").expect("reference");
}
```

- [ ] **Step 3: Write the failing tests**

`tests/repo_basics.rs`:

```rust
mod common;

use common::*;
use factory_git::{GitError, Repo};

#[test]
fn open_reports_the_checkout_root_from_a_subdirectory() {
    let fx = Fixture::new();
    let sub = fx.path.join("sub");
    std::fs::create_dir_all(&sub).expect("mkdir");
    let repo = Repo::open(&sub).expect("open");
    let want = std::fs::canonicalize(&fx.path).expect("canon");
    let got = std::fs::canonicalize(repo.root()).expect("canon");
    assert_eq!(got, want);
}

#[test]
fn tip_returns_the_branch_sha_and_not_found_for_a_missing_branch() {
    let fx = Fixture::new();
    let repo = fx.repo();
    assert_eq!(repo.tip("main").expect("tip"), fx.main_sha());
    assert!(matches!(repo.tip("nope"), Err(GitError::NotFound(_))));
}

#[test]
fn changed_paths_lists_added_and_modified_files_sorted() {
    let fx = Fixture::new();
    let (base, head) = fx.job_branch("factory/001-x", &[("b.txt", "new\n"), ("base.txt", "base\nmore\n")]);
    let paths = fx.repo().changed_paths(&base, &head).expect("paths");
    assert_eq!(paths, vec!["b.txt".to_string(), "base.txt".to_string()]);
}

#[test]
fn diff_to_files_writes_diff_stat_and_names_files() {
    let fx = Fixture::new();
    let (base, head) = fx.job_branch("factory/001-x", &[("b.txt", "new\n"), ("base.txt", "base\nmore\n")]);
    let out = fx.dir.path().join("out");
    let files = fx.repo().diff_to_files(&base, &head, &out).expect("diff");
    let diff = std::fs::read_to_string(&files.diff).expect("diff file");
    assert!(diff.contains("+++ b/b.txt"), "diff was: {diff}");
    assert!(diff.contains("+new"), "diff was: {diff}");
    assert!(diff.contains("+more"), "diff was: {diff}");
    let stat = std::fs::read_to_string(&files.stat).expect("stat file");
    assert!(stat.contains("2 files changed"), "stat was: {stat}");
    let names = std::fs::read_to_string(&files.names).expect("names file");
    assert_eq!(names, "b.txt\nbase.txt\n");
    assert_eq!((files.files, files.insertions, files.deletions), (2, 2, 0));
    assert_eq!(files.changed_lines(), 2);
    let file_name = files.diff.file_name().expect("name").to_string_lossy().into_owned();
    assert!(file_name.starts_with("review-") && file_name.ends_with(".diff"), "name was {file_name}");
}

#[test]
fn diff_to_files_of_identical_commits_writes_empty_files() {
    let fx = Fixture::new();
    let sha = fx.main_sha();
    let out = fx.dir.path().join("out");
    let files = fx.repo().diff_to_files(&sha, &sha, &out).expect("diff");
    assert_eq!(files.files, 0);
    assert_eq!(std::fs::read_to_string(&files.names).expect("names"), "");
    assert_eq!(std::fs::read_to_string(&files.diff).expect("diff"), "");
}

#[test]
fn is_ancestor_handles_ancestor_equal_and_sibling() {
    let fx = Fixture::new();
    let (base, head) = fx.job_branch("factory/001-x", &[("a.txt", "a\n")]);
    let (_, other) = fx.job_branch("factory/002-y", &[("c.txt", "c\n")]);
    let repo = fx.repo();
    assert!(repo.is_ancestor(&base, &head).expect("a"));
    assert!(repo.is_ancestor(&head, &head).expect("eq"));
    assert!(!repo.is_ancestor(&head, &base).expect("rev"));
    assert!(!repo.is_ancestor(&head, &other).expect("sibling"));
}

#[test]
fn is_merged_refuses_a_job_with_no_commits_or_no_head() {
    let fx = Fixture::new();
    let sha = fx.main_sha();
    let repo = fx.repo();
    assert!(matches!(repo.is_merged(&sha, &sha), Err(GitError::Refused(_))));
    assert!(matches!(repo.is_merged(&sha, " "), Err(GitError::Refused(_))));
}

#[test]
fn is_merged_follows_main_and_survives_a_deleted_branch() {
    let fx = Fixture::new();
    let (base, head) = fx.job_branch("factory/001-x", &[("a.txt", "a\n")]);
    let repo = fx.repo();
    assert_eq!(repo.is_merged(&base, &head).expect("before"), None);
    // A manual merge: main now points at the job head.
    point_branch_at(&fx.git(), "main", &head);
    assert_eq!(repo.is_merged(&base, &head).expect("after"), Some(head.clone()));
    // Deleting the branch changes nothing, because the check uses shas.
    fx.git()
        .find_branch("factory/001-x", git2::BranchType::Local)
        .expect("branch")
        .delete()
        .expect("delete");
    assert_eq!(repo.is_merged(&base, &head).expect("deleted"), Some(head));
}
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-git --test repo_basics`
Expected: compile error, `Repo` has no method `open` (and the others). The stubs from step 1 only define the error type.

- [ ] **Step 5: Implement `repo.rs`**

`src/repo.rs`:

```rust
use crate::error::{GitError, Result};
use git2::{BranchType, Commit, Diff, ErrorCode, Repository};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The branch the factory merges into.
pub const MAIN_BRANCH: &str = "main";

/// Handle on the main checkout of a repository.
pub struct Repo {
    pub(crate) inner: Repository,
    pub(crate) root: PathBuf,
}

/// First 7 characters of a sha, or the whole string when it is shorter.
pub(crate) fn short(sha: &str) -> &str {
    sha.get(..7).map_or(sha, |s| s)
}

impl Repo {
    /// Opens the repository that contains `path`. From inside a linked worktree it
    /// resolves to the main checkout, like `git rev-parse --git-common-dir` did.
    pub fn open(path: &Path) -> Result<Repo> {
        let found = Repository::discover(path)?;
        let repo = if found.is_worktree() {
            let common = found.commondir().to_path_buf();
            let top = common
                .parent()
                .ok_or_else(|| GitError::NotFound("main checkout of the worktree".into()))?;
            Repository::open(top)?
        } else {
            found
        };
        let root = repo
            .workdir()
            .ok_or_else(|| GitError::Refused("bare repositories are not supported".into()))?
            .to_path_buf();
        Ok(Repo { inner: repo, root })
    }

    /// The working directory of the main checkout.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolves a sha or other revision to a commit.
    pub(crate) fn commit_of(&self, rev: &str) -> Result<Commit<'_>> {
        let obj = self.inner.revparse_single(rev).map_err(|e| {
            if e.code() == ErrorCode::NotFound {
                GitError::NotFound(format!("revision {rev}"))
            } else {
                GitError::Git(e)
            }
        })?;
        Ok(obj.peel_to_commit()?)
    }

    /// The commit a local branch points at.
    pub(crate) fn branch_commit(&self, branch: &str) -> Result<Commit<'_>> {
        let found = self.inner.find_branch(branch, BranchType::Local).map_err(|e| {
            if e.code() == ErrorCode::NotFound {
                GitError::NotFound(format!("branch {branch}"))
            } else {
                GitError::Git(e)
            }
        })?;
        Ok(found.get().peel_to_commit()?)
    }

    /// Full sha of a local branch tip.
    pub fn tip(&self, branch: &str) -> Result<String> {
        Ok(self.branch_commit(branch)?.id().to_string())
    }

    /// Tree-to-tree diff, the same as `git diff base..head`.
    pub(crate) fn tree_diff(&self, base: &str, head: &str) -> Result<Diff<'_>> {
        let b = self.commit_of(base)?.tree()?;
        let h = self.commit_of(head)?.tree()?;
        Ok(self.inner.diff_tree_to_tree(Some(&b), Some(&h), None)?)
    }

    /// Paths changed between two commits. Renames list both paths on purpose,
    /// so risk classification sees the old location too.
    pub fn changed_paths(&self, base: &str, head: &str) -> Result<Vec<String>> {
        Ok(paths_of(&self.tree_diff(base, head)?))
    }

    /// True when `older` is `newer` or one of its ancestors.
    pub fn is_ancestor(&self, older: &str, newer: &str) -> Result<bool> {
        let a = self.commit_of(older)?.id();
        let b = self.commit_of(newer)?.id();
        if a == b {
            return Ok(true);
        }
        Ok(self.inner.graph_descendant_of(b, a)?)
    }

    /// `Some(main sha)` when `head` is already on main, `None` when it is not.
    /// Works after a manual merge and after the branch was deleted, because it
    /// takes shas. A job with no commits is refused, as `ctl is-merged` did.
    pub fn is_merged(&self, base: &str, head: &str) -> Result<Option<String>> {
        if head.trim().is_empty() {
            return Err(GitError::Refused("job has no recorded head".into()));
        }
        let head_id = self.commit_of(head)?.id();
        if head_id == self.commit_of(base)?.id() {
            return Err(GitError::Refused("job has no commits, head equals base".into()));
        }
        let main = self.tip(MAIN_BRANCH)?;
        if self.is_ancestor(head, &main)? {
            Ok(Some(main))
        } else {
            Ok(None)
        }
    }
}

/// Sorted, deduplicated paths of a diff.
pub(crate) fn paths_of(diff: &Diff<'_>) -> Vec<String> {
    let mut set = BTreeSet::new();
    for delta in diff.deltas() {
        for file in [delta.old_file(), delta.new_file()] {
            if let Some(p) = file.path() {
                set.insert(p.to_string_lossy().into_owned());
            }
        }
    }
    set.into_iter().collect()
}
```

- [ ] **Step 6: Implement `diff.rs`**

`src/diff.rs`:

```rust
use crate::error::Result;
use crate::repo::{paths_of, short, Repo};
use git2::{DiffFormat, DiffLine, DiffStatsFormat};
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

/// Paths and counts of the files `diff_to_files` wrote. The diff text itself
/// is never returned, so it never enters an LLM context by accident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffFiles {
    pub diff: PathBuf,
    pub stat: PathBuf,
    pub names: PathBuf,
    pub files: usize,
    pub insertions: usize,
    pub deletions: usize,
}

impl DiffFiles {
    /// Changed lines, the number the diff size cap is checked against.
    pub fn changed_lines(&self) -> usize {
        self.insertions + self.deletions
    }
}

fn write_line(out: &mut impl Write, line: &DiffLine<'_>) -> io::Result<()> {
    if matches!(line.origin(), '+' | '-' | ' ') {
        out.write_all(&[line.origin() as u8])?;
    }
    out.write_all(line.content())
}

impl Repo {
    /// Writes `review-<base7>..<head7>.diff`, `.stat` and `.names` into `dir`.
    pub fn diff_to_files(&self, base: &str, head: &str, dir: &Path) -> Result<DiffFiles> {
        let base_id = self.commit_of(base)?.id().to_string();
        let head_id = self.commit_of(head)?.id().to_string();
        let diff = self.tree_diff(base, head)?;
        fs::create_dir_all(dir)?;
        let stem = format!("review-{}..{}", short(&base_id), short(&head_id));
        let diff_path = dir.join(format!("{stem}.diff"));
        let stat_path = dir.join(format!("{stem}.stat"));
        let names_path = dir.join(format!("{stem}.names"));

        let mut out = BufWriter::new(fs::File::create(&diff_path)?);
        let mut failure: Option<io::Error> = None;
        let printed = diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
            match write_line(&mut out, &line) {
                Ok(()) => true,
                Err(e) => {
                    failure = Some(e);
                    false
                }
            }
        });
        if let Some(e) = failure {
            return Err(e.into());
        }
        printed?;
        out.flush()?;

        let stats = diff.stats()?;
        let stat_buf = stats.to_buf(DiffStatsFormat::FULL, 80)?;
        fs::write(&stat_path, &stat_buf[..])?;

        let mut names = String::new();
        for path in paths_of(&diff) {
            names.push_str(&path);
            names.push('\n');
        }
        fs::write(&names_path, names)?;

        Ok(DiffFiles {
            diff: diff_path,
            stat: stat_path,
            names: names_path,
            files: stats.files_changed(),
            insertions: stats.insertions(),
            deletions: stats.deletions(),
        })
    }
}
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-git --test repo_basics`
Expected: 8 passed. If `diff_to_files_of_identical_commits_writes_empty_files` fails because `diff.stats()` errors on an empty diff, report it instead of weakening the test.

Run: `cargo check -j4 --manifest-path tools/factory/Cargo.toml`
Expected: no errors.

- [ ] **Step 8: Commit**

Invoke the skill `dev-workflow-skills:1-git-commit` for the message (for example `feat(factory): add the git2 repo, diff and merged checks`), run its git commands inline, then:

```bash
git add tools/factory/Cargo.toml tools/factory/crates/factory-git && git commit --only -m "<message>" -- tools/factory/Cargo.toml tools/factory/crates/factory-git
```

If `tools/factory/Cargo.toml` is unchanged, drop it from both path lists. Also add `tools/factory/Cargo.lock` if it exists and changed.

---

### Task 2: `preflight_main`, `base_state`, `merge_no_ff`

**Files:**
- Create: `src/state.rs`, `src/merge.rs`, `tests/merge.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes (Task 1): `Repo` fields `inner` and `root`, `Repo::tip`, `Repo::is_ancestor`, `Repo::commit_of`, `Repo::branch_commit`, `short`, `MAIN_BRANCH`, `GitError`, and the test helpers in `tests/common/mod.rs`.
- Produces (exact signatures):
  - `pub enum Preflight { Ready { main_sha: String }, Busy { reason: String } }`
  - `Repo::preflight_main(&self) -> Result<Preflight>` (on `main`, no tracked changes, no merge or other operation in progress; untracked files are fine).
  - `pub enum BaseState { Current, MainMoved, BaseNotMain, Merged }` with `fn as_token(&self) -> &'static str` returning `ok`, `main-moved`, `base-not-main`, `merged`.
  - `pub struct BaseCheck { pub state: BaseState, pub main_sha: String }`
  - `Repo::base_state(&self, base: &str, head: &str) -> Result<BaseCheck>` (`Err(Refused)` when head equals base). `Current` means base is on main, head contains base, and main is an ancestor of head. After a sync the caller records the main sha as the new base (plan 05), but a stale base also still reads `Current`.
  - `pub enum MergeOutcome { Merged { merge_sha: String }, UpToDate, Conflict { paths: Vec<String> }, Refused { reason: String } }`
  - `Repo::merge_no_ff(&self, branch: &str, head_sha: &str, message: &str) -> Result<MergeOutcome>`. Callers call `base_state` first and only merge on `Current`. Main is left untouched on `Conflict`, `Refused` and `Err`.
  - `pub(crate) fn commit_merge(repo: &git2::Repository, branch_ref: &str, ours: &Commit, theirs: &Commit, message: &str) -> Result<MergeOutcome>` (used again in Task 3).

- [ ] **Step 1: Write the failing tests**

`tests/merge.rs`:

```rust
mod common;

use common::*;
use factory_git::{BaseState, GitError, MergeOutcome, Preflight};

const MSG: &str = "chore(factory): merge job 001 x";

#[test]
fn preflight_is_ready_on_a_clean_main_and_ignores_untracked_files() {
    let fx = Fixture::new();
    std::fs::write(fx.path.join("scratch.txt"), "x").expect("write");
    match fx.repo().preflight_main().expect("preflight") {
        Preflight::Ready { main_sha } => assert_eq!(main_sha, fx.main_sha()),
        other => panic!("expected Ready, got {other:?}"),
    }
}

fn busy_reason(fx: &Fixture) -> String {
    match fx.repo().preflight_main().expect("preflight") {
        Preflight::Busy { reason } => reason,
        other => panic!("expected Busy, got {other:?}"),
    }
}

#[test]
fn preflight_is_busy_for_tracked_changes() {
    let fx = Fixture::new();
    std::fs::write(fx.path.join("base.txt"), "changed\n").expect("write");
    assert!(busy_reason(&fx).contains("tracked changes"));
}

#[test]
fn preflight_is_busy_when_main_is_not_checked_out() {
    let fx = Fixture::new();
    let git = fx.git();
    let main = git.find_commit(git2::Oid::from_str(&fx.main_sha()).expect("oid")).expect("c");
    git.branch("other", &main, false).expect("branch");
    git.set_head("refs/heads/other").expect("set head");
    assert!(busy_reason(&fx).contains("not on main"));
}

#[test]
fn preflight_is_busy_in_the_middle_of_a_merge() {
    let fx = Fixture::new();
    let merge_head = fx.path.join(".git").join("MERGE_HEAD");
    std::fs::write(merge_head, format!("{}\n", fx.main_sha())).expect("write");
    assert!(busy_reason(&fx).contains("in progress"));
}

#[test]
fn base_state_reports_current_main_moved_merged_and_tokens() {
    let fx = Fixture::new();
    let (base, head) = fx.job_branch("factory/001-x", &[("job.txt", "j\n")]);
    let repo = fx.repo();
    let check = repo.base_state(&base, &head).expect("state");
    assert_eq!(check.state, BaseState::Current);
    assert_eq!(check.main_sha, base);
    assert_eq!(check.state.as_token(), "ok");

    commit_files(&fx.git(), &[("other.txt", "o\n")], "main moves");
    let moved = repo.base_state(&base, &head).expect("moved");
    assert_eq!(moved.state, BaseState::MainMoved);
    assert_eq!(moved.state.as_token(), "main-moved");

    point_branch_at(&fx.git(), "main", &head);
    let merged = repo.base_state(&base, &head).expect("merged");
    assert_eq!(merged.state, BaseState::Merged);
    assert_eq!(merged.state.as_token(), "merged");
}

#[test]
fn base_state_says_base_not_main_when_the_base_has_unrelated_commits() {
    let fx = Fixture::new();
    let (_, unrelated) = fx.job_branch("factory/001-x", &[("unrelated.txt", "u\n")]);
    // The job base is the unrelated commit, which is not on main.
    let head = commit_to_branch(&fx.git(), "factory/001-x", &[("mine.txt", "m\n")], "mine").to_string();
    let check = fx.repo().base_state(&unrelated, &head).expect("state");
    assert_eq!(check.state, BaseState::BaseNotMain);
    assert_eq!(check.state.as_token(), "base-not-main");
}

#[test]
fn base_state_refuses_a_job_with_no_commits() {
    let fx = Fixture::new();
    let sha = fx.main_sha();
    assert!(matches!(fx.repo().base_state(&sha, &sha), Err(GitError::Refused(_))));
}

#[test]
fn merge_no_ff_builds_a_two_parent_commit_with_the_fixed_message() {
    let fx = Fixture::new();
    let (base, head) = fx.job_branch("factory/001-x", &[("job.txt", "j\n")]);
    let repo = fx.repo();
    let merge_sha = match repo.merge_no_ff("factory/001-x", &head, MSG).expect("merge") {
        MergeOutcome::Merged { merge_sha } => merge_sha,
        other => panic!("expected Merged, got {other:?}"),
    };
    let git = fx.git();
    assert_eq!(fx.main_sha(), merge_sha);
    let commit = git.find_commit(git2::Oid::from_str(&merge_sha).expect("oid")).expect("commit");
    assert_eq!(commit.parent_count(), 2);
    assert_eq!(commit.parent_id(0).expect("p0").to_string(), base);
    assert_eq!(commit.parent_id(1).expect("p1").to_string(), head);
    assert_eq!(commit.message().expect("msg").trim_end(), MSG);
    assert!(fx.path.join("job.txt").is_file(), "the working tree must have the job file");
    assert!(matches!(repo.preflight_main().expect("pf"), Preflight::Ready { .. }));
    // A merged job stays merged after its branch is deleted.
    git.find_branch("factory/001-x", git2::BranchType::Local).expect("b").delete().expect("del");
    assert_eq!(repo.is_merged(&base, &head).expect("merged"), Some(merge_sha));
}

#[test]
fn merge_no_ff_refuses_when_the_branch_tip_is_not_the_reviewed_head() {
    let fx = Fixture::new();
    let (_, head) = fx.job_branch("factory/001-x", &[("job.txt", "j\n")]);
    commit_to_branch(&fx.git(), "factory/001-x", &[("late.txt", "l\n")], "unreviewed");
    let before = fx.main_sha();
    match fx.repo().merge_no_ff("factory/001-x", &head, MSG).expect("outcome") {
        MergeOutcome::Refused { reason } => assert!(reason.contains("reviewed head"), "reason: {reason}"),
        other => panic!("expected Refused, got {other:?}"),
    }
    assert_eq!(fx.main_sha(), before);
    assert!(!fx.path.join("late.txt").exists());
}

#[test]
fn merge_no_ff_reports_a_conflict_and_leaves_main_untouched() {
    let fx = Fixture::new();
    let (_, head) = fx.job_branch("factory/001-x", &[("base.txt", "job\n")]);
    commit_files(&fx.git(), &[("base.txt", "main\n")], "main edits the same file");
    let before = fx.main_sha();
    match fx.repo().merge_no_ff("factory/001-x", &head, MSG).expect("outcome") {
        MergeOutcome::Conflict { paths } => assert_eq!(paths, vec!["base.txt".to_string()]),
        other => panic!("expected Conflict, got {other:?}"),
    }
    assert_eq!(fx.main_sha(), before);
    assert_eq!(std::fs::read_to_string(fx.path.join("base.txt")).expect("read"), "main\n");
    assert!(matches!(fx.repo().preflight_main().expect("pf"), Preflight::Ready { .. }));
}

#[test]
fn merge_no_ff_refuses_when_an_untracked_file_would_be_overwritten() {
    let fx = Fixture::new();
    let (_, head) = fx.job_branch("factory/001-x", &[("clash.txt", "from job\n")]);
    std::fs::write(fx.path.join("clash.txt"), "mine\n").expect("write");
    let before = fx.main_sha();
    match fx.repo().merge_no_ff("factory/001-x", &head, MSG).expect("outcome") {
        MergeOutcome::Refused { .. } => {}
        other => panic!("expected Refused, got {other:?}"),
    }
    assert_eq!(fx.main_sha(), before);
    assert_eq!(std::fs::read_to_string(fx.path.join("clash.txt")).expect("read"), "mine\n");
    assert!(matches!(fx.repo().preflight_main().expect("pf"), Preflight::Ready { .. }));
}

#[test]
fn merge_no_ff_refuses_a_busy_main_and_keeps_the_local_edit() {
    let fx = Fixture::new();
    let (_, head) = fx.job_branch("factory/001-x", &[("job.txt", "j\n")]);
    std::fs::write(fx.path.join("base.txt"), "local edit\n").expect("write");
    let before = fx.main_sha();
    match fx.repo().merge_no_ff("factory/001-x", &head, MSG).expect("outcome") {
        MergeOutcome::Refused { reason } => assert!(reason.contains("tracked changes"), "reason: {reason}"),
        other => panic!("expected Refused, got {other:?}"),
    }
    assert_eq!(fx.main_sha(), before);
    assert_eq!(std::fs::read_to_string(fx.path.join("base.txt")).expect("read"), "local edit\n");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-git --test merge`
Expected: compile error, `BaseState`, `MergeOutcome`, `Preflight` are not exported.

- [ ] **Step 3: Implement `state.rs`**

`src/state.rs`:

```rust
use crate::error::{GitError, Result};
use crate::repo::{Repo, MAIN_BRANCH};
use git2::{RepositoryState, StatusOptions};

/// Result of the pre-merge check on the main checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Preflight {
    Ready { main_sha: String },
    Busy { reason: String },
}

/// Where a job branch stands relative to main.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseState {
    /// Base is on main, head contains it, and head contains all of main.
    Current,
    /// Main has commits the job branch does not have yet.
    MainMoved,
    /// The job base is not on main, or the head does not contain it. Never sync this.
    BaseNotMain,
    /// The head is already on main.
    Merged,
}

impl BaseState {
    /// The token used on the board and in the CLI.
    pub fn as_token(&self) -> &'static str {
        match self {
            BaseState::Current => "ok",
            BaseState::MainMoved => "main-moved",
            BaseState::BaseNotMain => "base-not-main",
            BaseState::Merged => "merged",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseCheck {
    pub state: BaseState,
    pub main_sha: String,
}

impl Repo {
    /// Checks that main can take a merge: it is checked out, has no tracked
    /// changes (untracked files are fine) and no merge or other operation is
    /// in progress. Reports the main sha so the caller can detect a race.
    pub fn preflight_main(&self) -> Result<Preflight> {
        let main_sha = self.tip(MAIN_BRANCH)?;
        let busy = |reason: &str| Ok(Preflight::Busy { reason: reason.to_string() });
        let head = self.inner.head()?;
        if !head.is_branch() || head.shorthand() != Some(MAIN_BRANCH) {
            return busy("the main checkout is not on main");
        }
        if self.inner.state() != RepositoryState::Clean {
            return busy("a merge or another operation is in progress on main");
        }
        let mut opts = StatusOptions::new();
        opts.include_untracked(false).include_ignored(false);
        if !self.inner.statuses(Some(&mut opts))?.is_empty() {
            return busy("the main checkout has tracked changes");
        }
        Ok(Preflight::Ready { main_sha })
    }

    /// Classifies a job branch against main. A job with no commits is refused.
    pub fn base_state(&self, base: &str, head: &str) -> Result<BaseCheck> {
        if self.commit_of(base)?.id() == self.commit_of(head)?.id() {
            return Err(GitError::Refused("job has no commits, head equals base".into()));
        }
        let main_sha = self.tip(MAIN_BRANCH)?;
        let state = if self.is_ancestor(head, &main_sha)? {
            BaseState::Merged
        } else if !self.is_ancestor(base, &main_sha)? || !self.is_ancestor(base, head)? {
            BaseState::BaseNotMain
        } else if self.is_ancestor(&main_sha, head)? {
            BaseState::Current
        } else {
            BaseState::MainMoved
        };
        Ok(BaseCheck { state, main_sha })
    }
}
```

- [ ] **Step 4: Implement `merge.rs`**

`src/merge.rs`:

```rust
use crate::error::Result;
use crate::repo::{short, Repo, MAIN_BRANCH};
use crate::state::Preflight;
use git2::build::CheckoutBuilder;
use git2::{Commit, ErrorClass, ErrorCode, Repository, Signature};
use std::collections::BTreeSet;

/// What a merge did. Conflicts and refusals are values, not errors, so the
/// driver can park the job with a reason instead of crashing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    /// A new two-parent merge commit. For a sync this is the new job head.
    Merged { merge_sha: String },
    /// Nothing to merge, the target already contains the other side.
    UpToDate,
    /// The merge conflicts. Nothing was changed on disk or in any ref.
    Conflict { paths: Vec<String> },
    /// A safety check failed. Nothing was changed.
    Refused { reason: String },
}

fn signature(repo: &Repository) -> Result<Signature<'static>> {
    match repo.signature() {
        Ok(sig) => Ok(sig),
        Err(_) => Ok(Signature::now("factory", "factory@localhost")?),
    }
}

/// Puts the working tree and index back on `ours` after a failed checkout.
fn restore(repo: &Repository, ours: &Commit<'_>) {
    let mut cb = CheckoutBuilder::new();
    cb.force();
    // Best effort. The original error is the one the caller sees.
    let _ = repo.checkout_tree(ours.as_object(), Some(&mut cb));
}

/// Merges `theirs` into `ours` and moves `branch_ref` to the result.
///
/// The merge is computed in memory and the commit is created without touching
/// any ref. Parents are verified, then a safe checkout updates the working
/// tree (it refuses to overwrite untracked or modified files), and only then
/// the branch ref moves. So every failure before the last step leaves the
/// branch where it was. `repo` must have `branch_ref` checked out.
pub(crate) fn commit_merge(
    repo: &Repository,
    branch_ref: &str,
    ours: &Commit<'_>,
    theirs: &Commit<'_>,
    message: &str,
) -> Result<MergeOutcome> {
    let mut index = repo.merge_commits(ours, theirs, None)?;
    if index.has_conflicts() {
        let mut paths = BTreeSet::new();
        for conflict in index.conflicts()? {
            let conflict = conflict?;
            for entry in [conflict.our, conflict.their, conflict.ancestor].into_iter().flatten() {
                paths.insert(String::from_utf8_lossy(&entry.path).into_owned());
            }
        }
        return Ok(MergeOutcome::Conflict { paths: paths.into_iter().collect() });
    }
    let tree = repo.find_tree(index.write_tree_to(repo)?)?;
    let sig = signature(repo)?;
    let new_id = repo.commit(None, &sig, &sig, message, &tree, &[ours, theirs])?;
    let created = repo.find_commit(new_id)?;
    let parents_ok = created.parent_count() == 2
        && created.parent_id(0)? == ours.id()
        && created.parent_id(1)? == theirs.id();
    if !parents_ok {
        return Ok(MergeOutcome::Refused {
            reason: "the merge commit has the wrong parents, nothing was changed".into(),
        });
    }

    let mut cb = CheckoutBuilder::new();
    cb.safe();
    if let Err(e) = repo.checkout_tree(created.as_object(), Some(&mut cb)) {
        if e.code() == ErrorCode::Conflict || e.class() == ErrorClass::Checkout {
            return Ok(MergeOutcome::Refused {
                reason: format!("local files would be overwritten: {}", e.message()),
            });
        }
        restore(repo, ours);
        return Err(e.into());
    }
    let moved = repo
        .find_reference(branch_ref)
        .and_then(|mut r| r.set_target(new_id, "factory: merge").map(|_| ()));
    if let Err(e) = moved {
        restore(repo, ours);
        return Err(e.into());
    }
    Ok(MergeOutcome::Merged { merge_sha: new_id.to_string() })
}

impl Repo {
    /// Merges the reviewed head of `branch` into main with a no-ff merge commit.
    ///
    /// Refuses when the branch tip is not exactly `head_sha`, when main is not
    /// ready (see `preflight_main`) or when the head is already on main. Call
    /// `base_state` first and merge only on `Current`.
    pub fn merge_no_ff(&self, branch: &str, head_sha: &str, message: &str) -> Result<MergeOutcome> {
        let tip = self.tip(branch)?;
        let head = self.commit_of(head_sha)?;
        let head_id = head.id().to_string();
        if tip != head_id {
            return Ok(MergeOutcome::Refused {
                reason: format!(
                    "branch {branch} is at {} but the reviewed head is {}",
                    short(&tip),
                    short(&head_id)
                ),
            });
        }
        if let Preflight::Busy { reason } = self.preflight_main()? {
            return Ok(MergeOutcome::Refused { reason });
        }
        let main = self.branch_commit(MAIN_BRANCH)?;
        if self.is_ancestor(&head_id, &main.id().to_string())? {
            return Ok(MergeOutcome::Refused { reason: "the head is already on main".into() });
        }
        let branch_ref = format!("refs/heads/{MAIN_BRANCH}");
        commit_merge(&self.inner, &branch_ref, &main, &head, message)
    }
}
```

Update `src/lib.rs` to:

```rust
//! Git layer for the factory driver. Built on git2, never shells out to git.

mod diff;
mod error;
mod merge;
mod repo;
mod state;

pub use diff::DiffFiles;
pub use error::{GitError, Result};
pub use merge::MergeOutcome;
pub use repo::{Repo, MAIN_BRANCH};
pub use state::{BaseCheck, BaseState, Preflight};
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-git --test merge`
Expected: 12 passed. If the untracked-collision test returns an `Err` instead of `Refused`, print `e.code()` and `e.class()` in `commit_merge` and extend the match, do not change the test. If `preflight_is_busy_in_the_middle_of_a_merge` does not see the `MERGE_HEAD` file, set `.git/MERGE_MODE` as well, but only in the test.

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-git --test repo_basics`
Expected: still 8 passed.

- [ ] **Step 6: Commit**

Invoke the skill `dev-workflow-skills:1-git-commit` (for example `feat(factory): add preflight, base state and the reviewed-sha merge`), then:

```bash
git add tools/factory/crates/factory-git && git commit --only -m "<message>" -- tools/factory/crates/factory-git
```

---

### Task 3: `create_worktree`, `remove_worktree`, `merge_main_into`, final verification

**Files:**
- Create: `src/worktree.rs`, `tests/worktree.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes (Tasks 1 and 2): `Repo` (`inner`, `root`, `commit_of`, `branch_commit`, `tip`, `is_ancestor`), `commit_merge`, `MergeOutcome`, `MAIN_BRANCH`, `GitError`, `short`, the test helpers.
- Produces (exact signatures):
  - `pub struct JobRef { pub id: String, pub slug: String }` with `fn branch_name(&self) -> String` (`factory/NNN-<slug>`) and `fn dir_name(&self) -> String` (`factory-NNN-<slug>`). `id` is digits, `slug` is `[a-z0-9-]`, both non-empty, otherwise `GitError::BadInput`.
  - `pub struct Worktree { pub name: String, pub path: PathBuf, pub branch: String, pub base: String }` (`base` is the main sha the branch was created from).
  - `Repo::create_worktree(&self, job: &JobRef) -> Result<Worktree>` (path `<root>/.claude/worktrees/factory-NNN-<slug>`, branch from main, refuses an existing path or branch; its own cargo target dir is left to the builder).
  - `Repo::remove_worktree(&self, wt: &Worktree, force: bool) -> Result<()>` (removes the directory and the branch; `Err(Refused)` for a dirty worktree or unmerged commits unless `force`; always refuses a branch not under `factory/`, the main branch, the main checkout path, and the branch the main checkout is on).
  - `Repo::merge_main_into(&self, wt: &Worktree, message: &str) -> Result<MergeOutcome>` (merge main into the job branch inside the worktree, no rebase; `Merged { merge_sha }` is the new job head, `UpToDate` when main is already contained, `Conflict` leaves the worktree untouched, `Refused` for a dirty or wrong-branch worktree).

- [ ] **Step 1: Write the failing tests**

`tests/worktree.rs`:

```rust
mod common;

use common::*;
use factory_git::{GitError, JobRef, MergeOutcome, Repo, Worktree};
use git2::Repository;

fn job() -> JobRef {
    JobRef { id: "001".into(), slug: "demo".into() }
}

fn wt_repo(wt: &Worktree) -> Repository {
    Repository::open(&wt.path).expect("open worktree")
}

#[test]
fn create_worktree_makes_the_branch_and_the_directory() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let wt = repo.create_worktree(&job()).expect("create");
    assert_eq!(wt.branch, "factory/001-demo");
    assert_eq!(wt.name, "factory-001-demo");
    assert_eq!(wt.base, fx.main_sha());
    assert!(wt.path.ends_with(".claude/worktrees/factory-001-demo"), "path: {:?}", wt.path);
    assert!(wt.path.join(".git").exists());
    assert!(wt.path.join("base.txt").is_file());
    assert_eq!(repo.tip("factory/001-demo").expect("tip"), fx.main_sha());
    // The worktree resolves back to the main checkout.
    let from_wt = Repo::open(&wt.path).expect("open from worktree");
    let want = std::fs::canonicalize(&fx.path).expect("canon");
    assert_eq!(std::fs::canonicalize(from_wt.root()).expect("canon"), want);
    // Creating it twice is refused.
    assert!(matches!(repo.create_worktree(&job()), Err(GitError::Refused(_))));
}

#[test]
fn create_worktree_rejects_hostile_slugs_before_touching_the_disk() {
    let fx = Fixture::new();
    let repo = fx.repo();
    for slug in ["../evil", "Bad Slug", "a/b", "", "x\n"] {
        let bad = JobRef { id: "001".into(), slug: slug.into() };
        assert!(matches!(repo.create_worktree(&bad), Err(GitError::BadInput(_))), "slug {slug:?}");
    }
    let bad_id = JobRef { id: "1x".into(), slug: "ok".into() };
    assert!(matches!(repo.create_worktree(&bad_id), Err(GitError::BadInput(_))));
    assert!(!fx.path.join(".claude").exists(), "nothing may be created for a rejected job");
}

#[test]
fn remove_worktree_removes_a_clean_worktree_and_its_branch() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let wt = repo.create_worktree(&job()).expect("create");
    repo.remove_worktree(&wt, false).expect("remove");
    assert!(!wt.path.exists());
    assert!(matches!(repo.tip("factory/001-demo"), Err(GitError::NotFound(_))));
}

#[test]
fn remove_worktree_refuses_untracked_work_unless_forced() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let wt = repo.create_worktree(&job()).expect("create");
    std::fs::write(wt.path.join("scratch.txt"), "work in progress").expect("write");
    assert!(matches!(repo.remove_worktree(&wt, false), Err(GitError::Refused(_))));
    assert!(wt.path.join("scratch.txt").is_file());
    assert!(repo.tip("factory/001-demo").is_ok());
    repo.remove_worktree(&wt, true).expect("forced remove");
    assert!(!wt.path.exists());
}

#[test]
fn remove_worktree_refuses_unmerged_commits_unless_forced() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let wt = repo.create_worktree(&job()).expect("create");
    commit_files(&wt_repo(&wt), &[("job.txt", "j\n")], "job work");
    assert!(matches!(repo.remove_worktree(&wt, false), Err(GitError::Refused(_))));
    assert!(wt.path.is_dir());
    assert!(repo.tip("factory/001-demo").is_ok());
    repo.remove_worktree(&wt, true).expect("forced remove");
    assert!(matches!(repo.tip("factory/001-demo"), Err(GitError::NotFound(_))));
}

#[test]
fn remove_worktree_allows_a_merged_job() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let wt = repo.create_worktree(&job()).expect("create");
    let head = commit_files(&wt_repo(&wt), &[("job.txt", "j\n")], "job work").to_string();
    point_branch_at(&fx.git(), "main", &head);
    repo.remove_worktree(&wt, false).expect("remove merged");
    assert!(!wt.path.exists());
}

#[test]
fn remove_worktree_never_touches_main_or_the_main_checkout() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let on_main = Worktree {
        name: "x".into(),
        path: fx.path.join(".claude/worktrees/x"),
        branch: "main".into(),
        base: fx.main_sha(),
    };
    assert!(matches!(repo.remove_worktree(&on_main, true), Err(GitError::Refused(_))));
    let at_root = Worktree {
        name: "factory-001-demo".into(),
        path: fx.path.clone(),
        branch: "factory/001-demo".into(),
        base: fx.main_sha(),
    };
    assert!(matches!(repo.remove_worktree(&at_root, true), Err(GitError::Refused(_))));
    assert!(fx.path.join("base.txt").is_file());
    assert!(repo.tip("main").is_ok());
}

#[test]
fn merge_main_into_adds_a_merge_commit_without_rewriting_history() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let wt = repo.create_worktree(&job()).expect("create");
    let old_head = commit_files(&wt_repo(&wt), &[("job.txt", "j\n")], "job work").to_string();
    commit_files(&fx.git(), &[("other.txt", "o\n")], "main moves");
    let main = fx.main_sha();
    let msg = "chore(factory): sync job 001 demo with main";
    let new_head = match repo.merge_main_into(&wt, msg).expect("sync") {
        MergeOutcome::Merged { merge_sha } => merge_sha,
        other => panic!("expected Merged, got {other:?}"),
    };
    let git = fx.git();
    let commit = git.find_commit(git2::Oid::from_str(&new_head).expect("oid")).expect("commit");
    assert_eq!(commit.parent_id(0).expect("p0").to_string(), old_head);
    assert_eq!(commit.parent_id(1).expect("p1").to_string(), main);
    assert_eq!(repo.tip("factory/001-demo").expect("tip"), new_head);
    assert!(repo.is_ancestor(&old_head, &new_head).expect("ancestor"));
    assert_eq!(fx.main_sha(), main, "main must not move");
    assert!(wt.path.join("job.txt").is_file() && wt.path.join("other.txt").is_file());
    // The stale base still reads as current after the sync.
    let check = repo.base_state(&wt.base, &new_head).expect("state");
    assert_eq!(check.state, factory_git::BaseState::Current);
}

#[test]
fn merge_main_into_is_up_to_date_when_main_has_not_moved() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let wt = repo.create_worktree(&job()).expect("create");
    commit_files(&wt_repo(&wt), &[("job.txt", "j\n")], "job work");
    let outcome = repo.merge_main_into(&wt, "sync").expect("sync");
    assert_eq!(outcome, MergeOutcome::UpToDate);
}

#[test]
fn merge_main_into_reports_a_conflict_and_leaves_the_worktree_untouched() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let wt = repo.create_worktree(&job()).expect("create");
    let old_head = commit_files(&wt_repo(&wt), &[("base.txt", "job\n")], "job edit").to_string();
    commit_files(&fx.git(), &[("base.txt", "main\n")], "main edit");
    match repo.merge_main_into(&wt, "sync").expect("sync") {
        MergeOutcome::Conflict { paths } => assert_eq!(paths, vec!["base.txt".to_string()]),
        other => panic!("expected Conflict, got {other:?}"),
    }
    assert_eq!(repo.tip("factory/001-demo").expect("tip"), old_head);
    assert_eq!(std::fs::read_to_string(wt.path.join("base.txt")).expect("read"), "job\n");
}

#[test]
fn merge_main_into_refuses_a_worktree_with_tracked_changes() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let wt = repo.create_worktree(&job()).expect("create");
    commit_files(&fx.git(), &[("other.txt", "o\n")], "main moves");
    std::fs::write(wt.path.join("base.txt"), "dirty\n").expect("write");
    match repo.merge_main_into(&wt, "sync").expect("sync") {
        MergeOutcome::Refused { reason } => assert!(reason.contains("tracked changes"), "reason: {reason}"),
        other => panic!("expected Refused, got {other:?}"),
    }
    assert_eq!(std::fs::read_to_string(wt.path.join("base.txt")).expect("read"), "dirty\n");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-git --test worktree`
Expected: compile error, `JobRef` and `Worktree` are not exported.

- [ ] **Step 3: Implement `worktree.rs`**

`src/worktree.rs`:

```rust
use crate::error::{GitError, Result};
use crate::merge::{commit_merge, MergeOutcome};
use crate::repo::{Repo, MAIN_BRANCH};
use git2::{BranchType, Repository, StatusOptions, WorktreeAddOptions, WorktreePruneOptions};
use std::fs;
use std::path::{Path, PathBuf};

/// The two fields of a job that name its branch and worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRef {
    pub id: String,
    pub slug: String,
}

impl JobRef {
    /// `factory/NNN-<slug>`.
    pub fn branch_name(&self) -> String {
        format!("factory/{}-{}", self.id, self.slug)
    }

    /// `factory-NNN-<slug>`, the worktree directory and git worktree name.
    pub fn dir_name(&self) -> String {
        format!("factory-{}-{}", self.id, self.slug)
    }

    fn validate(&self) -> Result<()> {
        let id_ok = !self.id.is_empty() && self.id.chars().all(|c| c.is_ascii_digit());
        let slug_ok = !self.slug.is_empty()
            && self.slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if id_ok && slug_ok {
            Ok(())
        } else {
            Err(GitError::BadInput(format!(
                "job id {:?} must be digits and slug {:?} must be lower case letters, digits and dashes",
                self.id, self.slug
            )))
        }
    }
}

/// A linked worktree for one job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub name: String,
    pub path: PathBuf,
    pub branch: String,
    /// The main sha the branch was created from.
    pub base: String,
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

impl Repo {
    /// Creates branch `factory/NNN-<slug>` from main and a worktree for it at
    /// `.claude/worktrees/factory-NNN-<slug>`. The builder owns its cargo target dir.
    pub fn create_worktree(&self, job: &JobRef) -> Result<Worktree> {
        job.validate()?;
        let branch = job.branch_name();
        let name = job.dir_name();
        let parent = self.root.join(".claude").join("worktrees");
        let path = parent.join(&name);
        if path.exists() {
            return Err(GitError::Refused(format!("{} already exists", path.display())));
        }
        if self.inner.find_branch(&branch, BranchType::Local).is_ok() {
            return Err(GitError::Refused(format!("branch {branch} already exists")));
        }
        let main = self.branch_commit(MAIN_BRANCH)?;
        fs::create_dir_all(&parent)?;
        let mut created = self.inner.branch(&branch, &main, false)?;
        let added = {
            let mut opts = WorktreeAddOptions::new();
            opts.reference(Some(created.get()));
            self.inner.worktree(&name, &path, Some(&opts)).map(|_| ())
        };
        if let Err(e) = added {
            // Do not leave a stray branch behind.
            let _ = created.delete();
            return Err(e.into());
        }
        Ok(Worktree { name, path, branch, base: main.id().to_string() })
    }

    /// Removes a job worktree and its branch. Refuses a dirty worktree and
    /// unmerged commits unless `force`. Never removes main, a branch outside
    /// `factory/`, the main checkout, or the branch the main checkout is on.
    pub fn remove_worktree(&self, wt: &Worktree, force: bool) -> Result<()> {
        if wt.branch == MAIN_BRANCH || !wt.branch.starts_with("factory/") {
            return Err(GitError::Refused(format!("{} is not a factory job branch", wt.branch)));
        }
        if same_dir(&wt.path, &self.root) {
            return Err(GitError::Refused("the worktree is the main checkout".into()));
        }
        let head = self.inner.head()?;
        if head.shorthand() == Some(wt.branch.as_str()) {
            return Err(GitError::Refused("the main checkout is on the job branch".into()));
        }
        if !force {
            if wt.path.is_dir() {
                let inside = Repository::open(&wt.path)?;
                let mut opts = StatusOptions::new();
                opts.include_untracked(true).recurse_untracked_dirs(true).include_ignored(false);
                if !inside.statuses(Some(&mut opts))?.is_empty() {
                    return Err(GitError::Refused(format!(
                        "worktree {} has uncommitted or untracked work",
                        wt.path.display()
                    )));
                }
            }
            if let Ok(tip) = self.tip(&wt.branch) {
                let main = self.tip(MAIN_BRANCH)?;
                if !self.is_ancestor(&tip, &main)? {
                    return Err(GitError::Refused(format!(
                        "branch {} has commits that are not on main",
                        wt.branch
                    )));
                }
            }
        }
        if let Ok(found) = self.inner.find_worktree(&wt.name) {
            let mut opts = WorktreePruneOptions::new();
            opts.valid(true).working_tree(true);
            found.prune(Some(&mut opts))?;
        }
        if let Ok(mut branch) = self.inner.find_branch(&wt.branch, BranchType::Local) {
            branch.delete()?;
        }
        Ok(())
    }

    /// Merges main into the job branch inside its worktree. No rebase and no
    /// history rewrite, the branch only gains a merge commit. A conflict is
    /// returned as a value and the worktree is left as it was.
    pub fn merge_main_into(&self, wt: &Worktree, message: &str) -> Result<MergeOutcome> {
        let refuse = |reason: &str| Ok(MergeOutcome::Refused { reason: reason.to_string() });
        let inside = Repository::open(&wt.path)?;
        let head = inside.head()?;
        if wt.branch == MAIN_BRANCH || !head.is_branch() || head.shorthand() != Some(wt.branch.as_str()) {
            return refuse("the worktree is not on the job branch");
        }
        if inside.state() != git2::RepositoryState::Clean {
            return refuse("an operation is already in progress in the worktree");
        }
        let mut opts = StatusOptions::new();
        opts.include_untracked(false).include_ignored(false);
        if !inside.statuses(Some(&mut opts))?.is_empty() {
            return refuse("the worktree has tracked changes");
        }
        let ours = head.peel_to_commit()?;
        let theirs = inside.find_commit(self.branch_commit(MAIN_BRANCH)?.id())?;
        if ours.id() == theirs.id() || inside.graph_descendant_of(ours.id(), theirs.id())? {
            return Ok(MergeOutcome::UpToDate);
        }
        let branch_ref = format!("refs/heads/{}", wt.branch);
        commit_merge(&inside, &branch_ref, &ours, &theirs, message)
    }
}
```

Update `src/lib.rs`:

```rust
//! Git layer for the factory driver. Built on git2, never shells out to git.

mod diff;
mod error;
mod merge;
mod repo;
mod state;
mod worktree;

pub use diff::DiffFiles;
pub use error::{GitError, Result};
pub use merge::MergeOutcome;
pub use repo::{Repo, MAIN_BRANCH};
pub use state::{BaseCheck, BaseState, Preflight};
pub use worktree::{JobRef, Worktree};
```

- [ ] **Step 4: Run the worktree tests**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-git --test worktree`
Expected: 10 passed. If `create_worktree` fails because libgit2 cannot create the nested path, keep the `create_dir_all(&parent)` call and report the error text. If the `remove_worktree` tests report the branch still exists, check that `prune` ran (it needs `valid(true)` for a healthy worktree).

- [ ] **Step 5: Final verification of the whole crate**

Run: `cargo check -j4 --manifest-path tools/factory/Cargo.toml`
Expected: no errors and no warnings from `factory-git`.

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-git`
Expected: 30 passed (8 in `repo_basics`, 12 in `merge`, 10 in `worktree`), 0 failed.

Run: `grep -rn "unwrap()" tools/factory/crates/factory-git/src`
Expected: no output. Also run `git status --short` and confirm only `tools/factory/` paths changed (plus the untracked spec file, which stays untouched), and that `git branch --list 'factory/*'` is the same as before the plan (all fixtures used temp repos).

- [ ] **Step 6: Commit**

Invoke the skill `dev-workflow-skills:1-git-commit` (for example `feat(factory): add worktree create, remove and sync of main`), then:

```bash
git add tools/factory/crates/factory-git && git commit --only -m "<message>" -- tools/factory/crates/factory-git
```

Add `tools/factory/Cargo.lock` to both lists if it changed.

---

## Self-review (done by the plan author)

- Spec coverage for the plan 02 row: `Repo::open` (T1), `tip` (T1), `create_worktree` with path `.claude/worktrees/factory-NNN-<slug>` and branch `factory/NNN-<slug>` from main (T3), `remove_worktree` with dirty, unmerged, forced and main-branch rules (T3), `diff_to_files` writing `.diff`, `.stat`, `.names` and returning only paths and counts (T1), `changed_paths` (T1), `is_ancestor` (T1), `is_merged` after a manual merge or a deleted branch and refusing a job with no commits (T1, T2), `preflight_main` (T2), `merge_no_ff` with tip check, two-parent commit, parent verification and clean abort (T2), `merge_main_into` with the conflict as a value (T3), base-state `ok`, `main-moved`, `base-not-main`, `merged` (T2). v2 gaps C1 (sync), C2 (`BaseNotMain` never synced), C4 (non-conflict failure leaves main untouched), C5 (cleanup rules) are covered.
- Deliberate differences from the spec's shorthand signatures: `is_merged` takes `base` as well (needed to detect a job with no commits), `merge_no_ff` takes the branch name (needed for the tip check), and `create_worktree` takes a small `JobRef` so `factory-git` does not depend on `factory-core`. Plan 05 maps its `Job` onto these.
- Type consistency: `MergeOutcome`, `Preflight`, `BaseState`, `BaseCheck`, `Worktree` and `JobRef` use one set of field names across tasks and tests. `commit_merge` has one signature, used by `merge_no_ff` (T2) and `merge_main_into` (T3). The test helpers (`Fixture`, `commit_files`, `commit_to_branch`, `point_branch_at`, `job_branch`) are defined in Task 1 and used unchanged later.
- No placeholders: every step has code or an exact command with expected output. Conditional notes in Steps name a concrete fallback.

Next plan: docs/superpowers/plans/2026-10-13-software-factory-v3/plan-03-agent-runner.md
