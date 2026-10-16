# Factory Batch Committer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run one committer agent per job after the last task, instead of one per task, behind the config switch `commit_batch` (default on).

**Architecture:** After each builder task the driver makes a plain `wip: task N <title>` snapshot commit through git2 (no agent). After the last task it soft-resets the branch to `base`, so all work is staged, and runs one committer (`CommitKind::Batch`, floor `base`). The snapshot tip is kept in the optional board field `batch_snapshot` so a parked batch can resume and so the fallback can rebuild one `chore:` commit per snapshot.

**Tech Stack:** Rust (git2, serde, toml), crates `factory-core`, `factory-git`, `factory-engine`. Work in `tools/factory/`.

**Spec:** `docs/superpowers/specs/2026-10-17-factory-cost-reduction-design.md` (part 1 and 3). Plan 2 (`plan-02-builder-excerpt.md`) covers part 2.

## Global Constraints

- Always `cargo ... -j4`, never `--workspace` or `--all`, never set `CARGO_TARGET_DIR`. Run from `tools/factory/`.
- No panicking unwrap or expect calls in production Rust (tests may use `expect`). No git CLI in Rust code: use git2.
- Board JSON must stay compatible with `factory-server`: new board fields are optional with `#[serde(default, skip_serializing_if = "Option::is_none")]`.
- A config typo or a non-boolean value must fail (the struct has `deny_unknown_fields`). A missing config must never become permissive.
- Comments are short full sentences ending with a period. No emojis. Conventional commit messages. Commit through the `dev-workflow-skills:1-git-commit` skill, with explicit paths (no `git add -A` or `.`).
- The `dev-workflow-skills:1-git-commit` skill still writes every final commit message: the committer agent is not replaced, only run less often.
- Fix rounds are unchanged: each keeps its own `CommitKind::Fix` committer run with the previous head as floor.
- `commit_batch = false` must restore today's behavior exactly (one committer per task, `CommitKind::Task`).
- Ruling added by this plan: a batch committer run sees the whole job diff, so its per-run cap is twice `commit_budget_weighted`. Update the spec line in Task 3.

## Review Focus

- A parked batch (committer moved HEAD, or the process died after the soft reset) must resume on `rework`/`approve` without redoing builders and without losing the snapshots.
- An empty task (nothing staged) must still park with `task N changed nothing`, also when the batch flag is on.
- A task whose snapshot tree equals the previous one must not break the `chore:` fallback (skip the refused empty commit).
- A builder that commits on its own must still trip the `builder committed on its own` guard.
- Old boards without `batch_snapshot` must load and re-save without the field.

---

### Task 1: Config switches, board field, git primitives

**Files:**
- Modify: `tools/factory/crates/factory-core/src/config.rs`
- Modify: `tools/factory/crates/factory-core/src/board.rs`
- Create: `tools/factory/crates/factory-git/src/reset.rs`
- Modify: `tools/factory/crates/factory-git/src/lib.rs`
- Create: `tools/factory/crates/factory-git/tests/reset.rs`
- Modify: `tools/factory/crates/factory-engine/src/ports.rs`

**Interfaces:**
- Produces: `Config.commit_batch: bool` and `Config.builder_excerpt: bool` (both default `true`); `Job.batch_snapshot: Option<String>`; `JobView.batch_snapshot: Option<String>`; `Repo::reset_soft(&self, wt: &Worktree, rev: &str) -> Result<()>`, `Repo::reset_hard(&self, wt: &Worktree, rev: &str) -> Result<()>`, `Repo::restore_tree(&self, wt: &Worktree, rev: &str) -> Result<()>`.

- [ ] **Step 1: Write the failing config test**

Add to the `tests` module of `config.rs`:

```rust
    #[test]
    fn cost_switches_default_on_and_can_be_turned_off() {
        let cfg = Config::default();
        assert!(cfg.commit_batch && cfg.builder_excerpt);
        let off = Config::parse("commit_batch = false\nbuilder_excerpt = false").expect("parse");
        assert!(!off.commit_batch && !off.builder_excerpt);
        assert!(Config::parse("commit_batch = \"yes\"").is_err());
        assert!(Config::parse("commit_bach = true").is_err());
    }
```

- [ ] **Step 2: Run it to see it fail**

Run: `cd tools/factory && cargo test -j4 -p factory-core cost_switches`
Expected: compile error, no field `commit_batch`.

- [ ] **Step 3: Add the fields**

In `Config` add after `commit_budget_weighted`:

```rust
    /// Run one committer per job after the last task instead of one per task.
    pub commit_batch: bool,
    /// Give each task builder only its own task instead of the whole plan and spec.
    pub builder_excerpt: bool,
```

In `Default`, after `commit_budget_weighted: 60_000,` add `commit_batch: true,` and `builder_excerpt: true,`.

- [ ] **Step 4: Write the failing board test**

Find the existing board test that proves `park_kind` or `task_index` is omitted when unset and that an old board loads (search `board.rs` tests for `skip` or `park_kind`). Add a sibling test in the same style:

```rust
    #[test]
    fn batch_snapshot_is_optional_and_omitted_when_unset() {
        let job = Job::new("001", "2026-10-17-001-demo", "demo", "2026-10-17", "demo", 0.0);
        assert!(job.batch_snapshot.is_none());
        let text = serde_json::to_string(&job).expect("json");
        assert!(!text.contains("batch_snapshot"), "{text}");
        let mut with = job.clone();
        with.batch_snapshot = Some("abc1234".to_string());
        let back: Job = serde_json::from_str(&serde_json::to_string(&with).expect("json")).expect("parse");
        assert_eq!(back.batch_snapshot.as_deref(), Some("abc1234"));
    }
```

- [ ] **Step 5: Add the board field**

In `Job` (next to `park_at`) add:

```rust
    /// The tip of the snapshot commits while the batch commit of the job is pending. New in the cost reduction series.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_snapshot: Option<String>,
```

In `Job::new` add `batch_snapshot: None,` next to `park_at: None,`. Search the crate for any other struct literal of `Job` (`rg "park_at: None"`) and add the field there too.

- [ ] **Step 6: Mirror it in `JobView`**

In `ports.rs` add `pub batch_snapshot: Option<String>,` to `JobView` and `batch_snapshot: job.batch_snapshot.clone(),` in `Engine::job`.

- [ ] **Step 7: Write the failing git tests**

Create `tools/factory/crates/factory-git/tests/reset.rs`:

```rust
mod common;

use common::*;
use factory_git::{JobRef, Repo, Worktree};
use git2::Repository;

fn job_worktree(fx: &Fixture) -> (Repo, Worktree) {
    let repo = fx.repo();
    let wt = repo.create_worktree(&JobRef { id: "001".into(), slug: "demo".into() }).expect("create worktree");
    (repo, wt)
}

fn inside(wt: &Worktree) -> Repository {
    Repository::open(&wt.path).expect("open worktree")
}

#[test]
fn reset_soft_moves_the_branch_and_keeps_the_work_staged() {
    let fx = Fixture::new();
    let (repo, wt) = job_worktree(&fx);
    commit_files(&inside(&wt), &[("a.txt", "a\n")], "wip: task 1 a");
    commit_files(&inside(&wt), &[("b.txt", "b\n")], "wip: task 2 b");
    repo.reset_soft(&wt, &wt.base).expect("reset");
    assert_eq!(repo.tip(&wt.branch).expect("tip"), wt.base);
    assert_eq!(repo.stage_all(&wt).expect("stage"), vec!["a.txt".to_string(), "b.txt".to_string()]);
    assert!(wt.path.join("b.txt").exists());
}

#[test]
fn reset_hard_drops_the_work() {
    let fx = Fixture::new();
    let (repo, wt) = job_worktree(&fx);
    commit_files(&inside(&wt), &[("a.txt", "a\n")], "wip: task 1 a");
    repo.reset_hard(&wt, &wt.base).expect("reset");
    assert_eq!(repo.tip(&wt.branch).expect("tip"), wt.base);
    assert!(!wt.path.join("a.txt").exists());
    assert!(repo.worktree_is_clean(&wt).expect("clean"));
}

#[test]
fn restore_tree_makes_the_files_and_index_match_a_commit_and_keeps_the_branch() {
    let fx = Fixture::new();
    let (repo, wt) = job_worktree(&fx);
    commit_files(&inside(&wt), &[("a.txt", "a\n")], "wip: task 1 a");
    let first = repo.tip(&wt.branch).expect("tip");
    commit_files(&inside(&wt), &[("b.txt", "b\n")], "wip: task 2 b");
    repo.reset_hard(&wt, &wt.base).expect("reset");
    repo.restore_tree(&wt, &first).expect("restore");
    assert_eq!(repo.tip(&wt.branch).expect("tip"), wt.base);
    assert!(wt.path.join("a.txt").exists());
    assert!(!wt.path.join("b.txt").exists());
    assert_eq!(repo.stage_all(&wt).expect("stage"), vec!["a.txt".to_string()]);
}
```

If `commit_files` or `Fixture::repo` have different signatures, read `factory-git/tests/commits.rs` (the first 20 lines) and `tests/common/mod.rs` and match them.

- [ ] **Step 8: Implement the git primitives**

Create `factory-git/src/reset.rs`:

```rust
use crate::error::Result;
use crate::repo::Repo;
use crate::worktree::Worktree;
use git2::build::CheckoutBuilder;
use git2::{Repository, ResetType};

impl Repo {
    /// Moves the job branch to `rev` and keeps the index and the files, so the work above `rev` stays staged.
    pub fn reset_soft(&self, wt: &Worktree, rev: &str) -> Result<()> {
        let inside = Repository::open(&wt.path)?;
        let target = inside.revparse_single(rev)?;
        inside.reset(&target, ResetType::Soft, None)?;
        Ok(())
    }

    /// Moves the job branch to `rev` and makes the index and the files match it.
    pub fn reset_hard(&self, wt: &Worktree, rev: &str) -> Result<()> {
        let inside = Repository::open(&wt.path)?;
        let target = inside.revparse_single(rev)?;
        inside.reset(&target, ResetType::Hard, None)?;
        Ok(())
    }

    /// Makes the index and the files match the tree of `rev` and leaves the branch where it is.
    pub fn restore_tree(&self, wt: &Worktree, rev: &str) -> Result<()> {
        let inside = Repository::open(&wt.path)?;
        let commit = inside.revparse_single(rev)?.peel_to_commit()?;
        let mut opts = CheckoutBuilder::new();
        opts.force();
        inside.checkout_tree(commit.tree()?.as_object(), Some(&mut opts))?;
        Ok(())
    }
}
```

Add `mod reset;` to `factory-git/src/lib.rs` next to the other `mod` lines. If `Result` or the error conversion for `git2::Error` is named differently, copy the `use` lines from `stage.rs`.

- [ ] **Step 9: Run the tests**

Run: `cd tools/factory && cargo test -j4 -p factory-core && cargo test -j4 -p factory-git && cargo check -j4`
Expected: all pass, check clean.

- [ ] **Step 10: Commit**

Invoke `dev-workflow-skills:1-git-commit`, staging only the six files above. Suggested message: `feat(factory): add batch commit switches and git resets`.

---

### Task 2: Batch committer kind, prompt, budget and fallback

**Files:**
- Modify: `tools/factory/crates/factory-engine/src/commit.rs`
- Modify: `tools/factory/crates/factory-engine/src/lib.rs` (re-export `SnapshotRef` next to `CommitRequest`)
- Modify: `tools/factory/crates/factory-engine/src/build.rs` (only the `CommitRequest { .. }` literal gets `snapshots: &[]`)
- Modify: `tools/factory/crates/factory-engine/tests/commit.rs`
- Modify: `.claude/factory/commit-context.md`

**Interfaces:**
- Consumes: `Repo::reset_hard`, `Repo::restore_tree`, `Repo::commit_staged` from Task 1.
- Produces: `CommitKind::Batch` (`as_str` gives `"batch"`); `pub struct SnapshotRef { pub n: u32, pub sha: String, pub title: String, pub files: Vec<String> }`; `CommitRequest.snapshots: &'a [SnapshotRef]`; a batch run capped at `2 * commit_budget_weighted`; `fallback_commit` for a batch request rebuilds one `chore: <title>` commit per snapshot.

- [ ] **Step 1: Write the failing prompt unit tests**

In the `tests` module of `commit.rs`, add `snapshots: &[],` to the `request()` literal, then add:

```rust
    fn snap(n: u32, title: &str, files: &[&str]) -> SnapshotRef {
        SnapshotRef { n, sha: format!("{n}{n}{n}{n}{n}{n}{n}"), title: title.to_string(), files: files.iter().map(|f| f.to_string()).collect() }
    }

    #[test]
    fn a_batch_prompt_lists_the_tasks_with_their_files() {
        let snaps = vec![snap(1, "first thing", &["a.txt"]), snap(2, "second thing", &["b.txt", "c.txt"])];
        let req = CommitRequest { kind: CommitKind::Batch, snapshots: &snaps, ..request() };
        let prompt = committer_prompt("CONTEXT", &req, &report("+x\n"), &[], false);
        assert!(prompt.contains("Kind: batch"), "{prompt}");
        assert!(prompt.contains("## Tasks in this change"), "{prompt}");
        assert!(prompt.contains("Task 1: first thing\n  a.txt\n"), "{prompt}");
        assert!(prompt.contains("Task 2: second thing\n  b.txt\n  c.txt\n"), "{prompt}");
        let tasks = prompt.find("## Tasks in this change").expect("tasks");
        let step1 = prompt.find("## Output of the skill's step 1 commands").expect("step 1");
        assert!(tasks < step1);
    }

    #[test]
    fn a_prompt_without_snapshots_has_no_tasks_section() {
        let prompt = committer_prompt("c", &request(), &report(""), &[], false);
        assert!(!prompt.contains("## Tasks in this change"));
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cd tools/factory && cargo test -j4 -p factory-engine --lib commit::tests`
Expected: compile error (no `Batch`, no `SnapshotRef`, no `snapshots`).

- [ ] **Step 3: Add the kind, the type and the prompt section**

In `commit.rs`:

1. Add `Batch,` to `CommitKind` and `CommitKind::Batch => "batch",` to `as_str`.
2. Add above `CommitRequest`:

```rust
/// One snapshot commit of a batch: the work of one task, made by the driver without an agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotRef {
    pub n: u32,
    pub sha: String,
    pub title: String,
    pub files: Vec<String>,
}
```

3. Add to `CommitRequest`: `/// The snapshots of a batch run, oldest first. Empty for every other kind.` and `pub snapshots: &'a [SnapshotRef],`.
4. In `committer_prompt`, before the final `format!`, build the section:

```rust
    let mut tasks = String::new();
    if !req.snapshots.is_empty() {
        tasks.push_str(
            "\n## Tasks in this change\n\nThe staged change holds these tasks, oldest first. \
             Make focused commits, normally one per task, in this order.\n\n",
        );
        for snap in req.snapshots {
            tasks.push_str(&format!("Task {}: {}\n", snap.n, snap.title));
            for file in &snap.files {
                tasks.push_str(&format!("  {file}\n"));
            }
        }
    }
```

and in the format string change `{analysis}\n\n\` to `{analysis}\n{tasks}\n\` (the empty case gives the same text as before). Add `tasks = tasks,` is not needed if the variable is named `tasks` (inline capture); keep the existing named arguments.
5. Re-export `SnapshotRef` in `lib.rs` where `CommitRequest` is exported. Add `snapshots: &[],` to the literal in `build.rs` (`commit_work`) and to `request()` in `tests/commit.rs`.

- [ ] **Step 4: Run the prompt tests**

Run: `cd tools/factory && cargo test -j4 -p factory-engine --lib commit::tests`
Expected: pass.

- [ ] **Step 5: Write the failing step tests**

Append to `tests/commit.rs`:

```rust
fn batch_request<'a>(floor: &'a str, snaps: &'a [factory_engine::SnapshotRef]) -> CommitRequest<'a> {
    CommitRequest { kind: CommitKind::Batch, task_title: "demo plan", snapshots: snaps, ..request(floor) }
}

/// Makes two snapshot commits on the job branch, then soft-resets to the base like the build stage does.
fn two_snapshots(wt: &Worktree) -> Vec<factory_engine::SnapshotRef> {
    write_change(wt, "a.txt");
    let first = commit_paths(&wt.path, &["a.txt"], "wip: task 1 first thing");
    write_change(wt, "b.txt");
    let second = commit_paths(&wt.path, &["b.txt"], "wip: task 2 second thing");
    reset_mixed(&wt.path, &wt.base);
    vec![
        factory_engine::SnapshotRef { n: 1, sha: first, title: "first thing".into(), files: vec!["a.txt".into()] },
        factory_engine::SnapshotRef { n: 2, sha: second, title: "second thing".into(), files: vec!["b.txt".into()] },
    ]
}

#[test]
fn a_batch_run_is_capped_at_twice_the_commit_budget_and_names_the_tasks() {
    let fx = Fixture::new();
    context(&fx);
    let wt = worktree(&fx);
    let snaps = two_snapshots(&wt);
    let runner = scripted(vec![effect(|dir| {
        commit_paths(dir, &["a.txt"], "feat(x): add a");
        commit_paths(dir, &["b.txt"], "feat(x): add b");
        "done".to_string()
    })]);
    let out = fx.engine(&runner).commit_step(&wt, &batch_request(&wt.base, &snaps)).expect("step");
    assert!(matches!(out, CommitOutcome::Committed { fallback: false, .. }), "{out:?}");
    assert_eq!(subjects(&fx, &wt, &wt.base), vec!["feat(x): add a", "feat(x): add b"]);
    let spec = runner.last_spec().expect("spec");
    assert_eq!(spec.budget_weighted, Some(120_000));
    assert!(spec.prompt.contains("Kind: batch") && spec.prompt.contains("Task 2: second thing"), "{}", spec.prompt);
}

#[test]
fn a_failed_batch_falls_back_to_one_chore_commit_per_snapshot() {
    let fx = Fixture::new();
    context(&fx);
    let wt = worktree(&fx);
    let snaps = two_snapshots(&wt);
    let runner = scripted(vec![nothing(), nothing()]);
    let out = fx.engine(&runner).commit_step(&wt, &batch_request(&wt.base, &snaps)).expect("step");
    assert!(matches!(out, CommitOutcome::Committed { fallback: true, .. }), "{out:?}");
    assert_eq!(subjects(&fx, &wt, &wt.base), vec!["chore: first thing", "chore: second thing"]);
    assert!(wt.path.join("a.txt").exists() && wt.path.join("b.txt").exists());
    assert!(fx.repo.worktree_is_clean(&wt).expect("clean"));
    assert!(ledger(&fx).contains("fallback commit"), "ledger: {}", ledger(&fx));
}

#[test]
fn a_batch_fallback_skips_a_snapshot_that_changed_nothing() {
    let fx = Fixture::new();
    context(&fx);
    let wt = worktree(&fx);
    write_change(&wt, "a.txt");
    let first = commit_paths(&wt.path, &["a.txt"], "wip: task 1 first thing");
    // The second snapshot has the same tree as the first.
    let second = {
        let repo = Repository::open(&wt.path).expect("open");
        let head = repo.head().expect("head").peel_to_commit().expect("commit");
        let sig = Signature::now("t", "t@example.com").expect("sig");
        repo.commit(Some("HEAD"), &sig, &sig, "wip: task 2 empty", &head.tree().expect("tree"), &[&head])
            .expect("commit")
            .to_string()
    };
    reset_mixed(&wt.path, &wt.base);
    let snaps = vec![
        factory_engine::SnapshotRef { n: 1, sha: first, title: "first thing".into(), files: vec!["a.txt".into()] },
        factory_engine::SnapshotRef { n: 2, sha: second, title: "empty thing".into(), files: vec![] },
    ];
    let runner = scripted(vec![nothing(), nothing()]);
    let out = fx.engine(&runner).commit_step(&wt, &batch_request(&wt.base, &snaps)).expect("step");
    assert!(matches!(out, CommitOutcome::Committed { fallback: true, .. }), "{out:?}");
    assert_eq!(subjects(&fx, &wt, &wt.base), vec!["chore: first thing"]);
}
```

- [ ] **Step 6: Run them to see them fail**

Run: `cd tools/factory && cargo test -j4 -p factory-engine --test commit batch`
Expected: the cap test fails on `60_000` and the fallback tests fail on subjects.

- [ ] **Step 7: Implement the cap and the batch fallback**

In `commit.rs`:

1. Add a helper on `Engine`:

```rust
    /// The run spec for one committer run. A batch run sees the whole job diff, so its cap is doubled.
    fn committer_run_spec(&self, wt: &Worktree, req: &CommitRequest<'_>, text: &str) -> AgentSpec {
        let mut spec = committer_spec(&self.job_id, &self.config, &wt.path, text);
        if req.kind == CommitKind::Batch {
            spec.budget_weighted = spec.budget_weighted.map(|b| b.saturating_mul(2));
        }
        spec
    }
```

and use it for both runs in `commit_step` (`self.run_agent(self.committer_run_spec(wt, req, &prompt))?`, and for the retry `let mut spec = self.committer_run_spec(wt, req, &text);`).

2. At the top of `fallback_commit` add:

```rust
        if req.kind == CommitKind::Batch && !req.snapshots.is_empty() {
            return self.fallback_batch(wt, req, why);
        }
```

3. Add:

```rust
    /// The fallback of a batch run: one `chore: <title>` commit per snapshot, rebuilt from the snapshot trees.
    fn fallback_batch(&self, wt: &Worktree, req: &CommitRequest<'_>, why: &str) -> Result<CommitOutcome, EngineError> {
        self.repo.reset_hard(wt, req.floor).map_err(git_err)?;
        let mut tip: Option<String> = None;
        for snap in req.snapshots {
            self.repo.restore_tree(wt, &snap.sha).map_err(git_err)?;
            let mut message = fallback_subject(&snap.title);
            if let Some(line) = issue_footer(ISSUE_KEY, req.issues) {
                message = append_footers(&message, &[line]);
            }
            match self.repo.commit_staged(wt, &message) {
                Ok(sha) => tip = Some(sha),
                // A task whose tree equals the one before it has nothing to commit.
                Err(GitError::Refused(_)) => {}
                Err(e) => return Err(git_err(e)),
            }
        }
        let Some(sha) = tip else {
            return Ok(CommitOutcome::Park(format!("the batch fallback made no commit: {why}")));
        };
        let short = sha.get(..7).unwrap_or(&sha);
        self.ruling(&format!("committer failed ({why}), batch fallback commits up to {short}"))?;
        Ok(CommitOutcome::Committed { tip: sha, fallback: true })
    }
```

The batch fallback ruling must contain the words `fallback commit` for the test above: write it as `"committer failed ({why}), batch fallback commits up to {short}"` only if that contains the exact substring `fallback commit` (it does: "fallback commits"). Keep it.

- [ ] **Step 8: Teach the committer about the new kind**

Read `.claude/factory/commit-context.md`. Where it explains the `Kind:` values (task, legacy, fix), add a short paragraph: for `Kind: batch` the staged change holds several tasks listed under "Tasks in this change"; make focused commits, normally one per task in that order, never more than 4; the floor is the base, so there is nothing above it to amend. Keep every phrase the test `shipped_context_file_has_the_style_and_the_standing_answers` checks. If `.claude/agents/factory-committer.md` lists the kinds, add `batch` there too, then run `cargo test -j4 -p factory-cli --test agent_lint`.

- [ ] **Step 9: Run the crate tests**

Run: `cd tools/factory && cargo test -j4 -p factory-engine && cargo test -j4 -p factory-cli --test agent_lint`
Expected: all pass.

- [ ] **Step 10: Commit**

Invoke `dev-workflow-skills:1-git-commit` with explicit paths (the five files above plus `.claude/agents/factory-committer.md` only if changed). Suggested message: `feat(factory): add the batch committer kind`.

---

### Task 3: Snapshot commits, the batch step, resume and docs

**Files:**
- Modify: `tools/factory/crates/factory-engine/src/build.rs`
- Modify: `tools/factory/crates/factory-engine/tests/build_checks.rs`
- Modify: `.claude/software-factory.md`
- Modify: `docs/superpowers/specs/2026-10-17-factory-cost-reduction-design.md` (one line, the cap ruling)

**Interfaces:**
- Consumes: Task 1 (`commit_batch`, `batch_snapshot`, `reset_soft`) and Task 2 (`CommitKind::Batch`, `SnapshotRef`).
- Produces: with `commit_batch` on, `build_task` makes a `wip: task N <title>` snapshot per task and one batch committer after the last; `build_stage` resumes a pending batch from `batch_snapshot`.

- [ ] **Step 1: Write the failing end-to-end tests**

Append to `tests/build_checks.rs` (it already has `two_task_fixture`, `ledger_of`, `change_effect`, `silent_effect`, `scripted_auto`):

```rust
const THREE_TASKS: &str = "# P\n\n### Task 1: first thing\n\nx\n\n### Task 2: second thing\n\ny\n\n### Task 3: third thing\n\nz\n";

fn three_task_fixture() -> Fixture {
    let fx = committing_fixture();
    std::fs::write(&fx.plan_file, THREE_TASKS).expect("plan");
    let mut board = factory_core::Board::load(&fx.board_path).expect("load");
    board.find_mut("001").expect("job").set_stage(Stage::Build, None, false, 1.0).expect("stage");
    board.save(&fx.board_path).expect("save");
    fx
}

#[test]
fn a_three_task_plan_runs_one_committer_and_leaves_no_snapshots() {
    let fx = three_task_fixture();
    let runner = scripted_auto(vec![change_effect("a.txt", 3), change_effect("b.txt", 2), change_effect("c.txt", 4)]);
    let e = fx.engine(&runner);
    assert_eq!(e.build_stage().expect("task 1"), StageOutcome::Advanced(Stage::Build));
    assert_eq!(e.build_stage().expect("task 2"), StageOutcome::Advanced(Stage::Build));
    let prompt = runner.last_spec_for("builder").expect("spec").prompt;
    assert!(prompt.contains("wip: task 1 first thing"), "{prompt}");
    assert_eq!(runner.specs_for("committer").len(), 0);
    assert_eq!(e.build_stage().expect("task 3"), StageOutcome::Advanced(Stage::Review));
    assert_eq!(runner.specs_for("committer").len(), 1);
    let batch = runner.last_spec_for("committer").expect("committer");
    assert!(batch.prompt.contains("Kind: batch"), "{}", batch.prompt);
    for want in ["Task 1: first thing", "Task 2: second thing", "Task 3: third thing"] {
        assert!(batch.prompt.contains(want), "missing {want}");
    }
    let job = fx.job();
    assert_eq!(job.stage, Stage::Review);
    assert_eq!(job.task_index, 3);
    assert!(job.batch_snapshot.is_none());
    let head = job.head.clone().expect("head");
    let base = job.base.clone().expect("base");
    let commits = fx.repo.commits_above(&base, &head).expect("commits");
    assert!(!commits.is_empty());
    assert!(commits.iter().all(|c| !c.subject.starts_with("wip:")), "{commits:?}");
    assert_eq!(fx.repo.tip(&job.branch).expect("tip"), head);
}

#[test]
fn an_empty_task_still_parks_with_the_batch_flag_on() {
    let fx = three_task_fixture();
    let runner = scripted_auto(vec![change_effect("a.txt", 3), silent_effect()]);
    let e = fx.engine(&runner);
    e.build_stage().expect("task 1");
    match e.build_stage().expect("task 2") {
        StageOutcome::NeedsHuman(r) => assert!(r.contains("task 2 changed nothing"), "{r}"),
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(fx.job().task_index, 1);
    assert_eq!(runner.specs_for("committer").len(), 0);
}

#[test]
fn the_flag_off_keeps_one_committer_per_task() {
    let mut fx = two_task_fixture();
    fx.config.commit_batch = false;
    let runner = scripted_auto(vec![change_effect("a.txt", 3), change_effect("b.txt", 2)]);
    let e = fx.engine(&runner);
    e.build_stage().expect("task 1");
    e.build_stage().expect("task 2");
    assert_eq!(runner.specs_for("committer").len(), 2);
    assert!(fx.job().batch_snapshot.is_none());
}
```

Then add this test, which exercises the park with a plain `scripted` runner (every effect is spelled out, including the builders and the batch committer):

```rust
#[test]
fn a_parked_batch_resumes_on_the_next_build_call_without_new_builders() {
    let fx = two_task_fixture();
    let detach: Effect = Box::new(|spec| {
        let repo = git2::Repository::open(&spec.cwd).expect("open");
        let id = repo.head().expect("head").target().expect("target");
        repo.set_head_detached(id).expect("detach");
        "done".to_string()
    });
    let runner = scripted(vec![change_effect("a.txt", 3), change_effect("b.txt", 2), detach]);
    let e = fx.engine(&runner);
    e.build_stage().expect("task 1");
    match e.build_stage().expect("task 2") {
        StageOutcome::NeedsHuman(r) => assert!(r.contains("HEAD"), "{r}"),
        other => panic!("unexpected {other:?}"),
    }
    let parked = fx.job();
    assert!(parked.batch_snapshot.is_some());
    assert_eq!(parked.task_index, 2);
    // The person fixes HEAD and the next call finishes the batch.
    let wt = parked.worktree.clone().expect("worktree");
    let repo = git2::Repository::open(&wt).expect("open");
    repo.set_head(&format!("refs/heads/{}", parked.branch)).expect("reattach");
    let runner2 = scripted_auto(vec![]);
    let out = fx.engine(&runner2).build_stage().expect("resume");
    assert_eq!(out, StageOutcome::Advanced(Stage::Review));
    assert_eq!(runner2.specs_for("builder").len(), 0);
    let job = fx.job();
    assert!(job.batch_snapshot.is_none() && job.head.is_some());
}
```

If `change_effect` or `silent_effect` do not exist under those names, use the helpers already at the top of `build_checks.rs`. The assertion `r.contains("HEAD")` must match the real park reason text from `verify_commit_run` ("committer moved HEAD off the job branch"); adjust only if that text differs.

- [ ] **Step 2: Run them to see them fail**

Run: `cd tools/factory && cargo test -j4 -p factory-engine --test build_checks`
Expected: the new tests fail (the committer still runs per task); existing per-task tests may fail on the new default, which Step 6 handles.

- [ ] **Step 3: Implement the snapshot step and the batch step**

In `build.rs`:

1. Add a free function and imports (`SnapshotRef` from `crate::commit`, `fallback_subject` is not needed here):

```rust
/// The subject of a task snapshot commit. The batch committer replaces it.
fn snapshot_subject(n: u32, title: &str) -> String {
    format!("wip: task {n} {title}")
}
```

2. In `build_stage`, right after `let tasks = parse_tasks(...)`, resume a pending batch:

```rust
        if job.batch_snapshot.is_some() {
            return self.resume_batch(&job, &tasks);
        }
```

3. In `build_task`, replace the `let before = ...` through the `new_tip` match and the `note_rewrites` call with a branch on the flag:

```rust
        let wt = self.job_worktree(job, &cwd, &base, &branch);
        let new_tip = if self.config.commit_batch {
            let staged = self.repo.stage_all(&wt).map_err(git_err)?;
            if staged.is_empty() {
                return self.park_with(ParkKind::NothingChanged, format!("task {} changed nothing", task.n), Vec::new());
            }
            self.repo.commit_staged(&wt, &snapshot_subject(task.n, &task.title)).map_err(git_err)?
        } else {
            let before = self.repo.commits_above(&base, &tip).map_err(git_err)?;
            let new_tip = match self.commit_work(&wt, CommitKind::Task, &task.title, &base, &job.issues)? {
                CommitOutcome::Committed { tip, .. } => tip,
                CommitOutcome::NothingStaged => return self.park_with(ParkKind::NothingChanged, format!("task {} changed nothing", task.n), Vec::new()),
                CommitOutcome::Park(reason) => return self.park_with(ParkKind::CommitFailed, reason, Vec::new()),
            };
            self.note_rewrites(&format!("task {}", task.n), &base, &before, &new_tip)?;
            new_tip
        };
```

(Remove the old `let wt`, `let before` and match lines it replaces. `wt` is built once, before the branch.)

4. Replace the end of `build_task` (from `let session = ...` on) with:

```rust
        let session = run.session_id.clone().or(job.session_id.clone());
        let next = job.task_index + 1;
        let last = next as usize >= tasks.len();
        let snapshot = (self.config.commit_batch && last).then(|| new_tip.clone());
        self.update(|j| {
            j.builder_id = session;
            j.task_index = next;
            j.batch_snapshot = snapshot;
            Ok(())
        })?;
        if !last {
            return Ok(StageOutcome::Advanced(Stage::Build));
        }
        if self.config.commit_batch {
            let job = self.job()?;
            return self.resume_batch(&job, tasks);
        }
        self.finish_build(job, &base, new_tip, job.head.clone())
```

5. Add the batch step:

```rust
    /// Runs the one committer of a job after its last task. It is safe to call again after a park:
    /// the branch is either still at the snapshot tip or already reset to the base.
    fn resume_batch(&self, job: &JobView, tasks: &[PlanTask]) -> Result<StageOutcome, EngineError> {
        let snapshot = job.batch_snapshot.clone().ok_or_else(|| EngineError::Msg("no pending batch commit".into()))?;
        let (cwd, base) = self.existing_worktree(job)?;
        let branch = job.branch.clone();
        let wt = self.job_worktree(job, &cwd, &base, &branch);
        let tip = self.repo.tip(&branch).map_err(git_err)?;
        if tip == snapshot {
            self.repo.reset_soft(&wt, &base).map_err(git_err)?;
        } else if tip != base {
            return self.park_with(ParkKind::CommitFailed, "the branch moved during the batch commit".to_string(), Vec::new());
        }
        let snaps = self.snapshot_refs(&base, &snapshot, tasks)?;
        let plan = std::fs::read_to_string(&self.plan_file).unwrap_or_default();
        let spec = std::fs::read_to_string(self.job_dir.join("spec.md")).unwrap_or_default();
        let (title, goal) = (plan_title(&plan), spec_goal(&spec));
        let req = CommitRequest {
            kind: CommitKind::Batch,
            plan_title: &title,
            task_title: &title,
            goal: &goal,
            floor: &base,
            issues: &job.issues,
            snapshots: &snaps,
        };
        let head = match self.commit_step(&wt, &req)? {
            CommitOutcome::Committed { tip, .. } => tip,
            CommitOutcome::NothingStaged => {
                return self.park_with(ParkKind::NothingChanged, "the batch commit found nothing staged".to_string(), Vec::new())
            }
            CommitOutcome::Park(reason) => return self.park_with(ParkKind::CommitFailed, reason, Vec::new()),
        };
        let short = head.get(..7).unwrap_or(&head).to_string();
        self.ruling(&format!("batch commit replaced {} snapshots, the branch is at {short}", snaps.len()))?;
        self.update(|j| {
            j.batch_snapshot = None;
            Ok(())
        })?;
        self.finish_build(job, &base, head, job.head.clone())
    }

    /// The snapshot commits above the base, oldest first, matched to the plan tasks by position.
    fn snapshot_refs(&self, base: &str, snapshot: &str, tasks: &[PlanTask]) -> Result<Vec<SnapshotRef>, EngineError> {
        let commits = self.repo.commits_above(base, snapshot).map_err(git_err)?;
        Ok(commits
            .into_iter()
            .enumerate()
            .map(|(i, c)| SnapshotRef {
                n: tasks.get(i).map_or(i as u32 + 1, |t| t.n),
                sha: c.sha,
                title: tasks.get(i).map_or_else(|| c.subject.clone(), |t| t.title.clone()),
                files: c.files,
            })
            .collect())
    }
```

`JobView` must derive what `finish_build` needs (`job.head`). If `JobView` is passed by reference as in the existing code, no change. `commit_work`'s `CommitRequest` literal already has `snapshots: &[]` from Task 2.

- [ ] **Step 4: Run the new tests**

Run: `cd tools/factory && cargo test -j4 -p factory-engine --test build_checks`
Expected: the four new tests pass.

- [ ] **Step 5: Fix the existing tests that assumed one committer per task**

Run the whole engine suite: `cargo test -j4 -p factory-engine`. For every failing test that asserts per-task committer behavior (for example `the_last_task_call_records_head_and_moves_to_review`, which checks the prompt for the task 1 commit subject "build step"), set `fx.config.commit_batch = false;` at the top of the test unless the test is about something the batch flow changes on purpose; in that case update the assertion to the batch behavior. Do not weaken an assertion to make it pass. List every changed test in the commit message body.

- [ ] **Step 6: Run the other crates that drive the engine**

Run: `cd tools/factory && cargo test -j4 -p factory-cli && cargo test -j4 -p factory-core && cargo test -j4 -p factory-git && cargo check -j4`
Expected: pass. Fix any CLI e2e test that counted committer runs the same way as Step 5. Then `cd ../factory-server && cargo test -j4` must still pass (the new board field is optional).

- [ ] **Step 7: Update the docs**

In `.claude/software-factory.md`: in the committer section say that with `commit_batch` on (default) the driver makes a `wip: task N <title>` snapshot after each task and runs one committer (`Kind: batch`, floor `base`, cap `2 * commit_budget_weighted`) after the last task, that `batch_snapshot` on the board marks a pending batch and `rework` or `approve` resume it, that the fallback makes one `chore:` commit per snapshot, and that `commit_batch = false` restores one committer per task. Fix the "one committer per task" sentence in the "Per-task builders" section accordingly. In the spec file add to Design 1: "A batch run is capped at twice `commit_budget_weighted`, because it sees the whole job diff."

- [ ] **Step 8: Commit**

Invoke `dev-workflow-skills:1-git-commit` with explicit paths (the five files above plus any test files changed in Steps 5 and 6). Suggested message: `feat(factory): run one committer per job`.
