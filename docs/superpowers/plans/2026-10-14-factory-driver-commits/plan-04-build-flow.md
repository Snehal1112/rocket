# Factory driver commits, plan 04: build flow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the factory driver commit the builder's work through the committer agent. The builder stops committing. After each builder run the driver stages the worktree and calls `Engine::commit_step` (plan 03) for task, legacy and fix runs, with guards for a builder that commits on its own or changes nothing.

**Architecture:** Two pure helpers (plan title, spec goal) feed the committer prompt. `build.rs` calls `commit_step` after every builder run and maps its outcome to the existing parks and ledger lines. The three builder prompts and `factory-builder.md` tell the builder to leave its changes in the worktree. Test fakes change at the same time, because the engine would otherwise park every old fake that commits: fake builders write files and the fake committer commits the index.

**Tech Stack:** Rust 2021, `git2`, `ScriptedRunner` and `Fixture` from the engine tests, the fake runners of `factory-cli` tests. Run from `tools/factory/`.

**Spec:** `docs/superpowers/specs/2026-10-14-factory-driver-commits-design.md`, decisions 1 and 3 (scope, staging), 5 (floor), design section 2 (driver flow), section 6 (rules, tests).

## Global Constraints

- Always `cargo ... -j4`. Never `--workspace` or `--all`. Never set `CARGO_TARGET_DIR`. Run cargo from `tools/factory/`.
- No `unwrap()` in production code. Tests may use `expect("...")`.
- The floor is the base commit in the first build round and the last reviewed head (`job.head`) in a fix round (spec decision 5). A park from `commit_step` goes to `Engine::park`.
- The guard texts from the spec: `builder committed on its own`, `task N changed nothing`, `builder made no changes`, `builder made no changes on resume`. These replace the old "no commit" parks.
- The builder never runs `git add` or `git commit`. The committer is the only agent that commits in a factory job.
- Do not weaken a test assertion to make it pass. A test that fails because a fake builder commits must be converted with the rule in task 2, step 2.
- Commit each task through the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths and commit with a pathspec.

## Review Focus

- A builder that commits on its own parks at once with no committer run, in the task path and in the whole-plan path.
- A builder that changes nothing parks with the right text in each path (task, whole plan, fix round) and never makes an empty commit.
- The fix-round committer prompt carries the reviewed head as its floor and `Kind: fix`; a committer that rewrites the reviewed head parks.
- A committer that rewrites an earlier task's commit leaves a `Ruling:` line and does not fail the job.
- The builder prompts and agent file no longer tell the builder to commit or to end with a `HEAD:` line.
- The end-to-end CLI scenarios still reach `merged`, and their role sequences show one committer run after each builder run.

---

### Task 1: Plan title and spec goal helpers

**Files:**
- Modify: `tools/factory/crates/factory-engine/src/tasks.rs`
- Modify: `tools/factory/crates/factory-engine/src/lib.rs` (export)
- Test: the `tests` module at the bottom of `tasks.rs`

**Interfaces:**
- Consumes: nothing.
- Produces, exported from the engine crate root:
  - `pub fn plan_title(plan: &str) -> String`: the text of the first `# ` heading, or `plan` when there is none.
  - `pub fn spec_goal(spec: &str) -> String`: a `Goal:` line (a leading `-`, `*` or spaces ignored, any case) or else the first line that is not blank and not a heading, cut to 200 characters; empty when the spec has no such line.

- [ ] **Step 1: Write the failing tests**

Add these tests inside the existing `tests` module of `tasks.rs`:

```rust
    #[test]
    fn plan_title_is_the_first_top_level_heading() {
        assert_eq!(plan_title("# Plan A\n\n### Task 1: x\n"), "Plan A");
        assert_eq!(plan_title("intro\n# Late Title\n"), "Late Title");
        assert_eq!(plan_title("## Not top level\n"), "plan");
        assert_eq!(plan_title("#NoSpace\n"), "plan");
        assert_eq!(plan_title(""), "plan");
        assert_eq!(plan_title("#  \n"), "plan");
    }

    #[test]
    fn spec_goal_prefers_a_goal_line() {
        assert_eq!(spec_goal("# T\n\nGoal: add the thing\n\nmore"), "add the thing");
        assert_eq!(spec_goal("**Goal:** add it\n"), "add it");
        assert_eq!(spec_goal("- goal: fix it\n"), "fix it");
        assert_eq!(spec_goal("# T\n\nFirst line.\nGoal: later goal\n"), "later goal");
    }

    #[test]
    fn spec_goal_falls_back_to_the_first_plain_line() {
        assert_eq!(spec_goal("# Title\n\nFirst paragraph line.\nSecond.\n"), "First paragraph line.");
        assert_eq!(spec_goal(""), "");
        assert_eq!(spec_goal("# Only a heading\n\n"), "");
        assert_eq!(spec_goal("Goal:\nnext line\n"), "Goal:");
        let long = "x".repeat(300);
        assert_eq!(spec_goal(&long).chars().count(), 200);
    }
```

In `lib.rs` change `pub use tasks::{parse_tasks, PlanTask};` to `pub use tasks::{parse_tasks, plan_title, spec_goal, PlanTask};`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd tools/factory && cargo test -j4 -p factory-engine --lib tasks`
Expected: FAIL to compile, `cannot find function plan_title`.

- [ ] **Step 3: Write the implementation**

Add above the `tests` module of `tasks.rs`:

```rust
/// The text of the first `# ` heading of a plan, or `plan` when it has none.
pub fn plan_title(plan: &str) -> String {
    plan.lines()
        .find_map(|line| line.strip_prefix("# "))
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "plan".to_string())
}

/// The goal of a spec for the committer prompt: a `Goal:` line, or else the first line that is
/// neither blank nor a heading, cut to 200 characters. Empty when there is none.
pub fn spec_goal(spec: &str) -> String {
    let cut = |text: &str| text.chars().take(200).collect::<String>();
    let mut first_plain: Option<String> = None;
    for line in spec.lines() {
        let t = line.trim().trim_start_matches(['-', '*', ' ']);
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if t.len() >= 5 && t.is_char_boundary(5) && t[..5].eq_ignore_ascii_case("goal:") {
            let rest = t[5..].trim_start_matches(['*', ' ']).trim();
            if !rest.is_empty() {
                return cut(rest);
            }
        }
        if first_plain.is_none() {
            first_plain = Some(cut(t));
        }
    }
    first_plain.unwrap_or_default()
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd tools/factory && cargo test -j4 -p factory-engine --lib tasks`
Expected: PASS.

- [ ] **Step 5: Check the crate**

Run: `cd tools/factory && cargo test -j4 -p factory-engine && cargo check -j4`
Expected: all pass. (`cargo clippy` on `factory-engine` stops on old lints in other crates; say so in the report.)

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill, with these paths only:

```bash
git add tools/factory/crates/factory-engine/src/tasks.rs tools/factory/crates/factory-engine/src/lib.rs
```

Expected subject: `feat(factory): read the plan title and spec goal for commits`.

---

### Task 2: Commit through the committer after every builder run

**Files:**
- Modify: `tools/factory/crates/factory-engine/src/build.rs`
- Modify: `tools/factory/crates/factory-engine/src/specs.rs` (three builder prompts)
- Modify (tests and fakes): `tools/factory/crates/factory-engine/tests/common/mod.rs`, `tools/factory/crates/factory-engine/tests/build_checks.rs`, `tools/factory/crates/factory-engine/tests/review_decide.rs`, `tools/factory/crates/factory-cli/tests/common/mod.rs`, `tools/factory/crates/factory-cli/tests/support/mod.rs`, `tools/factory/crates/factory-cli/tests/e2e.rs`
- Test: `tools/factory/crates/factory-engine/tests/build_checks.rs` (new guard tests), plus the converted tests

**Interfaces:**
- Consumes: from plan 03 `Engine::commit_step`, `CommitKind`, `CommitRequest`, `CommitOutcome`; from task 1 `plan_title`, `spec_goal`; `Repo::commits_above`, `Engine::park`, `Engine::ruling`.
- Produces: `build_stage` and `build_task` that commit through the committer. No new public API.

#### Part A: the engine code

- [ ] **Step 1: Write the new support code and failing tests for the guards**

The existing tests will fail until the fakes change, so write the fakes first.

In `tools/factory/crates/factory-engine/tests/common/mod.rs`:

1. Add these helpers after `commit_file`:

```rust
/// The subject the fake committer gives its commits.
pub const AUTO_COMMIT_MESSAGE: &str = "feat(test): build step";

/// Commits whatever is staged in `dir`, like a committer agent would. Returns the new sha.
pub fn commit_index(dir: &Path, message: &str) -> String {
    let repo = Repository::open(dir).expect("open repo");
    let mut index = repo.index().expect("index");
    let tree = repo.find_tree(index.write_tree().expect("tree")).expect("find tree");
    let sig = Signature::now("t", "t@example.com").expect("sig");
    let parent = repo.head().expect("head").peel_to_commit().expect("commit");
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent]).expect("commit").to_string()
}

/// Amends HEAD with whatever is staged in `dir` and a new message, like `git commit --amend`.
pub fn amend_head(dir: &Path, message: &str) -> String {
    let repo = Repository::open(dir).expect("open repo");
    let mut index = repo.index().expect("index");
    let tree = repo.find_tree(index.write_tree().expect("tree")).expect("find tree");
    let head = repo.head().expect("head").peel_to_commit().expect("commit");
    head.amend(Some("HEAD"), None, None, None, Some(message), Some(&tree)).expect("amend").to_string()
}

/// Writes the committer prompt context into the main checkout, where the engine reads it.
pub fn write_commit_context(main: &Path) {
    let path = main.join(".claude/factory/commit-context.md");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, "TEST CONTEXT\n").expect("context");
}

/// A fixture whose main checkout has the committer context file, so the engine runs the committer.
pub fn committing_fixture() -> Fixture {
    let fx = Fixture::new();
    write_commit_context(&fx.main);
    fx
}
```

2. In `ScriptedRunner`, add a field `auto_committer: bool`, and extend the impl blocks. The struct becomes:

```rust
pub struct ScriptedRunner {
    effects: Mutex<VecDeque<Effect>>,
    calls: Mutex<Vec<AgentSpec>>,
    /// When true, every `committer` run commits the staged index and does not use up an effect.
    auto_committer: bool,
}
```

Add to `impl ScriptedRunner`:

```rust
    /// Every spec the runner was called with for one role, in order.
    pub fn specs_for(&self, role: &str) -> Vec<AgentSpec> {
        self.calls.lock().expect("calls").iter().filter(|s| s.role == role).cloned().collect()
    }

    pub fn last_spec_for(&self, role: &str) -> Option<AgentSpec> {
        self.specs_for(role).pop()
    }
```

Change `AgentRunner for ScriptedRunner::run` so that, right after the call is recorded, it handles the auto committer:

```rust
        if self.auto_committer && spec.role == "committer" {
            commit_index(&spec.cwd, AUTO_COMMIT_MESSAGE);
            return AgentRun { session_id: Some("sess-c".to_string()), ..AgentRun::done("done") };
        }
```

Change `scripted` and add `scripted_auto`:

```rust
pub fn scripted(effects: Vec<Effect>) -> ScriptedRunner {
    ScriptedRunner { effects: Mutex::new(effects.into()), calls: Mutex::new(Vec::new()), auto_committer: false }
}

/// Like `scripted`, but committer runs are answered by committing the staged index.
pub fn scripted_auto(effects: Vec<Effect>) -> ScriptedRunner {
    ScriptedRunner { effects: Mutex::new(effects.into()), calls: Mutex::new(Vec::new()), auto_committer: true }
}
```

3. In `RoleRunner::run`, replace the builder branch so the builder writes a file and does not commit, and add a committer branch:

```rust
        let text = if spec.role == "builder" {
            let n = self.counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let path = spec.cwd.join(format!("docs/{}-{n}.txt", spec.job_id));
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            std::fs::write(path, "built\n").expect("write");
            "done".to_string()
        } else if spec.role == "committer" {
            commit_index(&spec.cwd, AUTO_COMMIT_MESSAGE);
            "done".to_string()
        } else {
            self.verdict.clone()
        };
```

4. In `SeriesFixture::new`, call `write_commit_context(&main);` right after `init_repo(&main);`.

- [ ] **Step 2: Convert the existing engine tests**

Rule: a fake builder writes files and does not commit; the fake committer (`scripted_auto`) commits the index; the engine tests build their fixture with `committing_fixture()`.

In `tests/build_checks.rs`:

1. Replace the imports and the `commit_effect` helper (the top of the file down to `silent_effect`) with:

```rust
mod common;

use common::{
    amend_head, commit_file, committing_fixture, scripted, scripted_auto, Effect, Fixture, ScriptedRunner,
};
use factory_core::Stage;
use factory_engine::StageOutcome;

/// A builder that writes a file and leaves it uncommitted, as the real builder now does.
fn change_effect(file: &'static str, lines: usize) -> Effect {
    Box::new(move |spec| {
        let body: String = (0..lines).map(|n| format!("line {n}\n")).collect();
        std::fs::write(spec.cwd.join(file), body).expect("write change");
        "done".to_string()
    })
}

/// A committer that amends HEAD with the staged index instead of making a new commit.
fn amend_effect(message: &'static str) -> Effect {
    Box::new(move |spec| {
        amend_head(&spec.cwd, message);
        "done".to_string()
    })
}

/// A builder that commits on its own, which the driver must refuse.
fn commits_on_its_own_effect(file: &'static str) -> Effect {
    Box::new(move |spec| {
        commit_file(&spec.cwd, file, "x\n");
        "done".to_string()
    })
}
```

Keep `silent_effect` as it is.

2. In the whole file: replace every `commit_effect(` with `change_effect(`; replace `Fixture::new()` with `committing_fixture()` (including inside `two_task_fixture`); replace `scripted(vec![` with `scripted_auto(vec![` except in `bad_config_is_rejected_when_the_engine_is_built` and in the `dead` runner of `a_stopped_first_task_parks_and_a_rework_resumes_at_that_task_with_the_note` (both stay `scripted(vec![])`).

3. Replace `runner.last_spec()` with `runner.last_spec_for("builder")` in these tests: `the_first_task_call_stays_in_build_and_names_only_task_one`, `the_last_task_call_records_head_and_moves_to_review`, `a_fix_round_resumes_the_session_and_does_not_restart_the_task_loop`, `a_rework_note_is_the_findings_of_the_resume_and_is_removed_after`, `a_stopped_first_task_parks_and_a_rework_resumes_at_that_task_with_the_note`.

4. These assertions change:
   - `first_build_with_no_commit_parks_the_job`: rename to `first_build_with_no_changes_parks_the_job`; `r.contains("no commits")` becomes `r.contains("no changes")`.
   - `resume_with_no_new_commit_parks_the_job`: rename to `resume_with_no_changes_parks_the_job`; `r.contains("no new commit")` becomes `r.contains("no changes on resume")`.
   - `the_last_task_call_records_head_and_moves_to_review`: `prompt.contains("test commit")` becomes `prompt.contains("build step")`.
   - `a_task_without_a_commit_parks_the_job_with_its_number`: rename to `a_task_that_changes_nothing_parks_the_job_with_its_number`; keep `r.contains("task 2")` and add `assert!(r.contains("changed nothing"), "{r}");`.
   - `the_first_task_call_stays_in_build_and_names_only_task_one`: the existing `assert_eq!(spec.max_turns, Some(120));` stays (the spec is now the builder spec).

In `tests/review_decide.rs`:
   - change the import to `use common::{committing_fixture, scripted, scripted_auto, Effect, Fixture, ScriptedRunner};` (drop `commit_file` if it is no longer used);
   - replace `Fixture::new()` with `committing_fixture()` in every test that calls `build_and_pass_checks` (leave the others);
   - replace the body of `build_and_pass_checks` with:

```rust
fn build_and_pass_checks(fx: &Fixture, files: &[&'static str]) -> ScriptedRunner {
    let effects: Vec<Effect> = vec![Box::new({
        let files: Vec<&'static str> = files.to_vec();
        move |spec| {
            for f in &files {
                if let Some(parent) = std::path::Path::new(f).parent() {
                    std::fs::create_dir_all(spec.cwd.join(parent)).expect("mkdir");
                }
                std::fs::write(spec.cwd.join(f), "x\n").expect("write");
            }
            "done".to_string()
        }
    })];
    let runner = scripted_auto(effects);
    fx.engine(&runner).build_stage().expect("build");
    // The review stage reads the checks.md that the checks stage wrote.
    fx.engine(&runner).checks_stage().expect("checks");
    runner
}
```

In `tests/runloop.rs`, `series.rs` and the other engine test files, `SeriesFixture` now handles the fake committer. Run `cargo test -j4 -p factory-engine`; if a test there fails because it counts builder events or agent runs (the committer adds one run and one event after every builder run), update the count by role (`agent_type` in events), not by weakening the assertion.

- [ ] **Step 3: Write the new guard tests**

Add to `tests/build_checks.rs`:

```rust
fn start_fix_round(fx: &Fixture) {
    std::fs::write(fx.job_dir.join("changes-r2.md"), "fix it").expect("findings");
    let mut board = factory_core::Board::load(&fx.board_path).expect("load");
    let job = board.find_mut("001").expect("job");
    job.round = 2;
    job.set_stage(Stage::Build, None, true, 1.0).expect("stage");
    board.save(&fx.board_path).expect("save");
}

#[test]
fn a_whole_plan_builder_that_commits_on_its_own_parks_before_any_committer_run() {
    let fx = committing_fixture();
    let runner = scripted_auto(vec![commits_on_its_own_effect("a.txt")]);
    match fx.engine(&runner).build_stage().expect("build") {
        StageOutcome::NeedsHuman(r) => assert!(r.contains("committed on its own"), "{r}"),
        other => panic!("unexpected {other:?}"),
    }
    assert!(runner.specs_for("committer").is_empty());
}

#[test]
fn a_task_builder_that_commits_on_its_own_parks_with_its_number() {
    let fx = two_task_fixture();
    let runner = scripted_auto(vec![commits_on_its_own_effect("a.txt")]);
    match fx.engine(&runner).build_stage().expect("task 1") {
        StageOutcome::NeedsHuman(r) => assert!(r.contains("committed on its own in task 1"), "{r}"),
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(fx.job().task_index, 0);
    assert!(runner.specs_for("committer").is_empty());
}

#[test]
fn the_committer_prompt_names_the_kind_the_task_the_plan_and_the_base_as_floor() {
    let fx = two_task_fixture();
    let runner = scripted_auto(vec![change_effect("a.txt", 3)]);
    fx.engine(&runner).build_stage().expect("task 1");
    let prompt = runner.last_spec_for("committer").expect("committer spec").prompt;
    assert!(prompt.contains("Kind: task"), "{prompt}");
    assert!(prompt.contains("Task: first thing"), "{prompt}");
    assert!(prompt.contains("Plan: P"), "{prompt}");
    assert!(prompt.contains(&format!("Floor: {}", fx.job().base.expect("base"))), "{prompt}");
    assert!(prompt.contains("new file:   a.txt"), "{prompt}");
}

#[test]
fn a_whole_plan_build_commits_as_legacy() {
    let fx = committing_fixture();
    let runner = scripted_auto(vec![change_effect("a.txt", 3)]);
    fx.engine(&runner).build_stage().expect("build");
    let prompt = runner.last_spec_for("committer").expect("committer spec").prompt;
    assert!(prompt.contains("Kind: legacy"), "{prompt}");
}

#[test]
fn a_fix_round_commits_above_the_reviewed_head() {
    let fx = committing_fixture();
    let runner = scripted_auto(vec![change_effect("a.txt", 3), change_effect("b.txt", 2)]);
    let e = fx.engine(&runner);
    e.build_stage().expect("first");
    let reviewed = fx.job().head.clone().expect("reviewed head");
    start_fix_round(&fx);
    e.build_stage().expect("fix");
    let prompt = runner.last_spec_for("committer").expect("committer spec").prompt;
    assert!(prompt.contains("Kind: fix"), "{prompt}");
    assert!(prompt.contains(&format!("Floor: {reviewed}")), "{prompt}");
}

#[test]
fn a_fix_round_that_changes_nothing_parks_without_a_committer_run() {
    let fx = committing_fixture();
    let first = scripted_auto(vec![change_effect("a.txt", 3)]);
    fx.engine(&first).build_stage().expect("first");
    start_fix_round(&fx);
    let second = scripted_auto(vec![silent_effect()]);
    match fx.engine(&second).build_stage().expect("fix") {
        StageOutcome::NeedsHuman(r) => assert!(r.contains("no changes on resume"), "{r}"),
        other => panic!("unexpected {other:?}"),
    }
    assert!(second.specs_for("committer").is_empty());
}

#[test]
fn a_committer_that_rewrites_the_reviewed_head_parks_the_fix_round() {
    let fx = committing_fixture();
    let first = scripted_auto(vec![change_effect("a.txt", 3)]);
    fx.engine(&first).build_stage().expect("first");
    start_fix_round(&fx);
    let second = scripted(vec![change_effect("b.txt", 2), amend_effect("feat(test): reword the reviewed commit")]);
    match fx.engine(&second).build_stage().expect("fix") {
        StageOutcome::NeedsHuman(r) => assert!(r.contains("at or below the floor"), "{r}"),
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(fx.job().stage, Stage::NeedsHuman);
}

#[test]
fn a_committer_that_rewrites_an_earlier_task_commit_leaves_a_ruling() {
    let fx = two_task_fixture();
    let first = scripted_auto(vec![change_effect("a.txt", 3)]);
    fx.engine(&first).build_stage().expect("task 1");
    let second = scripted(vec![change_effect("b.txt", 2), amend_effect("feat(test): tasks one and two")]);
    let out = fx.engine(&second).build_stage().expect("task 2");
    assert_eq!(out, StageOutcome::Advanced(Stage::Review));
    let ledger = ledger_of(&fx);
    assert!(ledger.contains("task 2 commit rewrote earlier commits"), "{ledger}");
}

#[test]
fn the_builder_prompts_tell_it_not_to_commit() {
    let fx = two_task_fixture();
    let runner = scripted_auto(vec![change_effect("a.txt", 3), change_effect("b.txt", 2), change_effect("c.txt", 1)]);
    let e = fx.engine(&runner);
    e.build_stage().expect("task 1");
    e.build_stage().expect("task 2");
    start_fix_round(&fx);
    e.build_stage().expect("fix");
    let specs = runner.specs_for("builder");
    assert_eq!(specs.len(), 3);
    for spec in &specs {
        assert!(spec.prompt.contains("Do not run `git add` or `git commit`"), "{}", spec.prompt);
        assert!(!spec.prompt.contains("1-git-commit"), "{}", spec.prompt);
        assert!(!spec.prompt.contains("HEAD: <sha"), "{}", spec.prompt);
    }
    let whole = committing_fixture();
    let whole_runner = scripted_auto(vec![change_effect("a.txt", 3)]);
    whole.engine(&whole_runner).build_stage().expect("whole plan");
    let prompt = whole_runner.last_spec_for("builder").expect("builder spec").prompt;
    assert!(prompt.contains("Do not run `git add` or `git commit`"), "{prompt}");
    assert!(!prompt.contains("1-git-commit"), "{prompt}");
}
```

- [ ] **Step 4: Run the engine tests to see the failures**

Run: `cd tools/factory && cargo test -j4 -p factory-engine --test build_checks`
Expected: FAIL. Old behavior still runs (the builder is not told to leave changes and nothing calls the committer), so the new tests fail, and the tests that expect "no changes" parks fail.

- [ ] **Step 5: Write the engine implementation**

In `tools/factory/crates/factory-engine/src/specs.rs`, make three prompt edits.

In `builder_first_spec`, replace the two lines

```
             Commit each task with the dev-workflow-skills:1-git-commit skill and a pathspec commit (`git add <paths> && git commit --only -m \"...\" -- <paths>`). \
             Never use `git add -A`, `--all` or `.`. Never push, never merge, never touch the board or the ledger.\n\
```

with

```
             Do not run `git add` or `git commit`. Leave your changes in the worktree: the driver commits them for you after you finish. \
             Never push, never merge, never touch the board or the ledger.\n\
```

and replace the last prompt line `If you are blocked, say why in your final reply and stop. Your last line must be `HEAD: <sha of your last commit>`.` with `If you are blocked, say why in your final reply and stop.`

In `builder_task_spec`, replace the three lines

```
             Make exactly one commit, for this task, with the dev-workflow-skills:1-git-commit skill and a pathspec commit \
             (`git add <paths> && git commit --only -m \"...\" -- <paths>`). Never use `git add -A`, `--all` or `.`. \
             Never push, never merge, never touch the board or the ledger.\n\
```

with

```
             Do not run `git add` or `git commit`. Leave this task's changes in the worktree: the driver commits them for you after you finish. \
             Never push, never merge, never touch the board or the ledger.\n\
```

and replace `Reply with one line for Task {n}, and end your reply with the line `HEAD: <sha>`.` with `Reply with one line for Task {n}.`

In `builder_resume_spec`, replace the whole `format!` text with:

```rust
            "Fix only the problems listed in {findings}. You are on branch `{branch}`. \
             Do not run `git add` or `git commit`: leave the fixes in the worktree and the driver commits them. \
             Do not amend or rewrite earlier commits. \
             Run the relevant checks (`cargo check -j4`, targeted `cargo test -j4 -p <crate> <test_name>`, yarn checks) before you finish. \
             Never push, never merge.",
```

In `tools/factory/crates/factory-engine/src/build.rs`:

1. Replace the `use` block at the top with:

```rust
use std::path::{Path, PathBuf};

use factory_agent::StopReason;
use factory_core::{Stage, TaskState};
use factory_git::{CommitInfo, JobRef, Worktree};

use crate::commit::{CommitKind, CommitOutcome, CommitRequest};
use crate::ports::{git_err, now, JobView};
use crate::tasks::{parse_tasks, plan_title, spec_goal, PlanTask};
use crate::{Engine, EngineError, StageOutcome};
```

2. In `build_stage`, record the tip before the run. Right after the `let spec = match &job.session_id { ... };` block and before `let run = self.run_agent(spec)?;` insert:

```rust
        let tip_before = self.repo.tip(&branch).map_err(git_err)?;
```

3. Replace everything in `build_stage` from `let head = self.repo.tip(&branch).map_err(git_err)?;` (after the stop-reason park) through `self.finish_build(&job, &base, head, previous_head)` with:

```rust
        if self.repo.tip(&branch).map_err(git_err)? != tip_before {
            return self.park("builder committed on its own".to_string());
        }
        // The floor is the base in the first round and the last reviewed head in a fix round.
        let floor = previous_head.clone().unwrap_or_else(|| base.clone());
        let before = self.repo.commits_above(&floor, &tip_before).map_err(git_err)?;
        let wt = self.job_worktree(&job, &cwd, &base, &branch);
        let (kind, title) = if new_work {
            (CommitKind::Legacy, "build the plan")
        } else {
            (CommitKind::Fix, "address review findings")
        };
        let head = match self.commit_work(&wt, kind, title, &floor)? {
            CommitOutcome::Committed { tip, .. } => tip,
            CommitOutcome::NothingStaged => {
                let why = if new_work { "builder made no changes" } else { "builder made no changes on resume" };
                return self.park(why.to_string());
            }
            CommitOutcome::Park(reason) => return self.park(reason),
        };
        self.note_rewrites("build", &floor, &before, &head)?;
        self.finish_build(&job, &base, head, previous_head)
```

4. In `build_task`, replace everything from `let after = self.repo.tip(&branch).map_err(git_err)?;` through the end of the function with:

```rust
        if self.repo.tip(&branch).map_err(git_err)? != tip {
            return self.park(format!("builder committed on its own in task {}", task.n));
        }
        let before = self.repo.commits_above(&base, &tip).map_err(git_err)?;
        let wt = self.job_worktree(job, &cwd, &base, &branch);
        let new_tip = match self.commit_work(&wt, CommitKind::Task, &task.title, &base)? {
            CommitOutcome::Committed { tip, .. } => tip,
            CommitOutcome::NothingStaged => return self.park(format!("task {} changed nothing", task.n)),
            CommitOutcome::Park(reason) => return self.park(reason),
        };
        self.note_rewrites(&format!("task {}", task.n), &base, &before, &new_tip)?;
        let short = new_tip.get(..7).unwrap_or(&new_tip).to_string();
        self.ledger_task(task.n, TaskState::ImplementerDone, &short)?;
        let session = run.session_id.clone().or(job.session_id.clone());
        let next = job.task_index + 1;
        self.update(|j| {
            j.builder_id = session;
            j.task_index = next;
            Ok(())
        })?;
        if (next as usize) < tasks.len() {
            return Ok(StageOutcome::Advanced(Stage::Build));
        }
        self.finish_build(job, &base, new_tip, job.head.clone())
    }
```

(Keep the `if run.stop_reason != StopReason::Done` park that comes before it.)

5. Add these methods to the `impl Engine<'_>` block, below `existing_worktree`:

```rust
    /// The worktree handle the git layer needs, built from the job record.
    fn job_worktree(&self, job: &JobView, cwd: &Path, base: &str, branch: &str) -> Worktree {
        Worktree {
            name: JobRef { id: self.job_id.clone(), slug: job.slug.clone() }.dir_name(),
            path: cwd.to_path_buf(),
            branch: branch.to_string(),
            base: base.to_string(),
        }
    }

    /// Runs the committer step for the builder's uncommitted work.
    fn commit_work(
        &self,
        wt: &Worktree,
        kind: CommitKind,
        task_title: &str,
        floor: &str,
    ) -> Result<CommitOutcome, EngineError> {
        let plan = std::fs::read_to_string(&self.plan_file).unwrap_or_default();
        let spec = std::fs::read_to_string(self.job_dir.join("spec.md")).unwrap_or_default();
        let (plan_title, goal) = (plan_title(&plan), spec_goal(&spec));
        let req = CommitRequest { kind, plan_title: &plan_title, task_title, goal: &goal, floor };
        self.commit_step(wt, &req)
    }

    /// Writes a ruling when the committer rewrote commits that were above the floor before the run.
    fn note_rewrites(&self, label: &str, floor: &str, before: &[CommitInfo], tip: &str) -> Result<(), EngineError> {
        let after = self.repo.commits_above(floor, tip).map_err(git_err)?;
        let gone: Vec<&str> = before
            .iter()
            .filter(|c| !after.iter().any(|a| a.sha == c.sha))
            .map(|c| c.sha.get(..7).unwrap_or(&c.sha))
            .collect();
        if gone.is_empty() {
            return Ok(());
        }
        let short = tip.get(..7).unwrap_or(tip);
        self.ruling(&format!("{label} commit rewrote earlier commits {}, the branch is now at {short}", gone.join(", ")))
    }
```

- [ ] **Step 6: Run the engine tests to verify they pass**

Run: `cd tools/factory && cargo test -j4 -p factory-engine`
Expected: PASS, including the ten new tests in `build_checks.rs`. If an old test outside the files listed above still fails because its fake builder commits, convert it with the rule in step 2 and mention it in the report.

#### Part B: the CLI tests

- [ ] **Step 7: Convert the CLI fakes and scenarios**

In `tools/factory/crates/factory-cli/tests/common/mod.rs`:

1. Add after `commit_file`:

```rust
/// Commits whatever is staged in `dir`, like a committer agent would.
pub fn commit_index(dir: &Path, message: &str) -> String {
    let repo = Repository::open(dir).expect("open repo");
    let mut index = repo.index().expect("index");
    let tree = repo.find_tree(index.write_tree().expect("tree")).expect("find tree");
    let sig = Signature::now("t", "t@example.com").expect("sig");
    let parent = repo.head().expect("head").peel_to_commit().expect("commit");
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent]).expect("commit").to_string()
}
```

2. Replace the builder branch of `RoleRunner::run` with a file write and add a committer branch:

```rust
        let text = if spec.role == "builder" {
            let n = self.counter.fetch_add(1, Ordering::SeqCst);
            let path = spec.cwd.join(format!("docs/{}-{n}.txt", spec.job_id));
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            std::fs::write(path, "built\n").expect("write");
            "done".to_string()
        } else if spec.role == "committer" {
            commit_index(&spec.cwd, "feat(test): build step");
            "done".to_string()
        } else {
            "fine\nVERDICT: APPROVE".to_string()
        };
```

3. In `Fixture::with_risk_path`, after the `config.toml` write, add:

```rust
        std::fs::write(root.join(".claude/factory/commit-context.md"), "TEST CONTEXT\n").expect("context");
```

In `tools/factory/crates/factory-cli/tests/support/mod.rs`:

1. Add the same `commit_index` helper (copy it from above) after `commit_files`.
2. In `ScriptRunner::run`, right after the `roles.push` block and before the `let step = ...` line, add:

```rust
        if spec.role == "committer" {
            commit_index(&spec.cwd, "feat(test): build step");
            return AgentRun { session_id: Some("sess-c".to_string()), ..AgentRun::done("done") };
        }
```

3. Replace the `Step::Build` arm so the builder writes its files and does not commit:

```rust
            Some(Step::Build { files, input_tokens }) if spec.role == "builder" => {
                for (name, content) in &files {
                    // An empty content deletes the file, which the driver stages as a deletion.
                    if content.is_empty() {
                        let _ = std::fs::remove_file(spec.cwd.join(name));
                        continue;
                    }
                    if let Some(parent) = spec.cwd.join(name).parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Err(e) = std::fs::write(spec.cwd.join(name), content) {
                        return AgentRun::failed(&format!("fake builder failed: {e}"));
                    }
                }
                ("done".to_string(), input_tokens)
            }
```

4. In `Scenario::new`, after the `config.toml` write, add:

```rust
        std::fs::write(root.join(".claude/factory/commit-context.md"), "TEST CONTEXT\n")?;
```

In `tools/factory/crates/factory-cli/tests/e2e.rs` update the three role assertions:
- line 22: `["builder", "committer", "builder", "committer", "reviewer", "builder", "committer", "reviewer"]`
- line 42: `["builder", "committer", "reviewer", "builder", "committer", "reviewer"]`
- line 55: `["builder", "committer", "reviewer"]`

and update the comment above the first one to say a red check resumes the builder, the committer commits the fix, and no reviewer run is spent.

- [ ] **Step 8: Run all affected tests**

Run:

```bash
cd tools/factory && cargo test -j4 -p factory-engine && cargo test -j4 -p factory-cli --test e2e --test new_to_merged && cargo test -j4 -p factory-git && cargo check -j4
```

Expected: all pass. If a CLI test outside `e2e.rs` and `new_to_merged.rs` fails because its fake builder commits, convert it with the same rule and mention it in the report.

- [ ] **Step 9: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. The engine change and every converted test must land in one commit, or the tree is red between commits. Stage these paths (and any other test file you had to convert):

```bash
git add tools/factory/crates/factory-engine/src/build.rs tools/factory/crates/factory-engine/src/specs.rs
git add tools/factory/crates/factory-engine/tests/common/mod.rs tools/factory/crates/factory-engine/tests/build_checks.rs tools/factory/crates/factory-engine/tests/review_decide.rs
git add tools/factory/crates/factory-cli/tests/common/mod.rs tools/factory/crates/factory-cli/tests/support/mod.rs tools/factory/crates/factory-cli/tests/e2e.rs
```

Expected subject: `feat(factory): commit builder work through the committer`.

---

### Task 3: The builder agent file and its lint pins

**Files:**
- Modify: `.claude/agents/factory-builder.md`
- Modify: `tools/factory/crates/factory-cli/tests/agent_lint.rs` (the builder body pins, the test at line 125)

**Interfaces:**
- Consumes: the lint helpers `read`, `require_all`, `forbid_all` in `agent_lint.rs`.
- Produces: a builder agent text that matches the prompts of task 2.

- [ ] **Step 1: Write the failing lint change**

In `agent_lint.rs`, in `builder_body_pins_the_rules`:

1. Remove these five phrases from the `require_all` list: `"dev-workflow-skills:1-git-commit"`, `"run its git commands yourself"`, `"One commit per task"`, `"Never use a helper agent for commits"`, `"pathspec"`.
2. Add these phrases to the same list (keep `"Never amend"` and `"Use at most 2 helper agents per task"`): `"Never run `git commit`"`, `"driver commits them"`.
3. Replace the `forbid_all` list at the end of the test with:

```rust
        &[
            "isolation",
            "WORKTREE:",
            "BASE:",
            "HEAD:",
            "git switch -c",
            "SendMessage",
            "dev-workflow-skills:1-git-commit",
            "git commit --only",
            "Never use a helper agent for commits",
        ],
```

- [ ] **Step 2: Run the lint to verify it fails**

Run: `cd tools/factory && cargo test -j4 -p factory-cli --test agent_lint builder_body_pins_the_rules`
Expected: FAIL: the builder body is missing the new phrases and still contains the forbidden ones.

- [ ] **Step 3: Edit the agent file**

In `.claude/agents/factory-builder.md` make these edits:

1. In the Work section replace the bullet `- Use at most 2 helper agents per task. Never use a helper agent for commits.` with `- Use at most 2 helper agents per task.`
2. In the Rocket hard rules section replace the bullet `- Run `cargo check -j4` yourself before the first commit, so the commit gate starts from a warm target.` with `- Run `cargo check -j4` yourself before you finish, so the driver's checks start from a warm target.`
3. Replace the whole Commits section (from the `## Commits` heading down to the line before `## Sandbox`) with:

```markdown
## Commits

- Never run `git commit`, `git add`, `git commit --amend`, `git rebase`, `git reset` or `git push`. Leave your changes in the worktree. The driver commits them for you after you finish, with a committer agent that writes the message.
- Do not invoke a commit skill and do not start helper agents for commits.
- Never amend, never rebase, never run a bare `git stash`, never push, never merge.
```

4. In the Blockers and resume section replace `- On a resume, fix only the listed failures or findings, with new commits. Do not rewrite earlier commits.` with `- On a resume, fix only the listed failures or findings and leave the fixes uncommitted. Do not rewrite earlier commits.`

- [ ] **Step 4: Run the lint to verify it passes**

Run: `cd tools/factory && cargo test -j4 -p factory-cli --test agent_lint`
Expected: PASS, all agent lint tests.

- [ ] **Step 5: Check the CLI crate**

Run: `cd tools/factory && cargo test -j4 -p factory-cli`
Expected: all pass.

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill, with these paths only:

```bash
git add .claude/agents/factory-builder.md tools/factory/crates/factory-cli/tests/agent_lint.rs
```

Expected subject: `feat(factory): tell the builder to leave commits to the driver`.

Next plan: `plan-05-merge-sync-rollout.md`, written after this plan is merged.
