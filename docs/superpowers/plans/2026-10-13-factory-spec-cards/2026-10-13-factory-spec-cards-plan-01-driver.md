# Factory Spec Cards, Plan 01: Driver Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `factory new --plans` creates one job per plan up front, with plans 2 to N parked at stage `spec`, all sharing a series id `S<n>`, and copies `spec.md` into plan 1's job folder.

**Architecture:** `SeriesPos` gets an optional `id`. `create_job` is split: a private `add_job` adds one job to the board and makes its folder, and `create_job` calls it once per plan of the series. `run_series` already finds a job by plan file and index, and `run_job` already moves a `spec` job to `build`, so the series loop only keeps a fallback for old series.

**Tech Stack:** Rust (`tools/factory`, crates `factory-core`, `factory-engine`, `factory-cli`), cargo tests with the fake runner in `factory-engine/tests/common`.

**Spec:** `docs/superpowers/specs/2026-10-13-factory-spec-and-plan-cards-design.md`, section "Driver".

## Global Constraints

- Every cargo call passes `-j4`. Never use `--workspace` or `--all`. Use `--manifest-path tools/factory/Cargo.toml -p <crate>`.
- Production code never panics on a bad value: no unwrap and no expect outside tests. Tests may use `expect`.
- Serde: persistence structs get no `rename_all`. New board fields are optional with defaults, so old boards load.
- Commits: conventional commits. Use the `dev-workflow-skills:1-git-commit` skill for every commit, with a pathspec commit (`git add <paths>`, then commit only those paths). Never `git add -A`, `--all` or `.`.
- Comments are short full sentences that end with a period.
- A series with one plan gets no series id, no extra jobs and no spec copy.
- A job's stage after `create_job`: plan 1 `build`, plans 2 to N `spec`.

## Review Focus

- A chain with a missing plan file fails before any job is created, so no half series is left on the board (Task 2 test).
- A second `factory new` gets the next series id, `S2`, not `S1` again (Task 2 test).
- `--plans` given as a single file, not a folder, copies no `spec.md` and does not fail (Task 2 test).
- A series that has no `id` (made before this change) still gets its next job after the previous one merges (Task 3 test).
- A cancelled plan stops the series, and the plans after it stay at `spec` (Task 3 test).

---

### Task 1: Series id on the board

**Files:**
- Modify: `tools/factory/crates/factory-core/src/board.rs` (struct `SeriesPos`, test module)
- Modify: `tools/factory/crates/factory-core/src/ids.rs` (add `next_series_id`, test module)
- Modify: `tools/factory/crates/factory-core/src/lib.rs:18` (export)
- Modify: `tools/factory/crates/factory-engine/src/runloop.rs` (two `SeriesPos { ... }` literals)
- Modify: `tools/factory/crates/factory-cli/src/status.rs` (one `SeriesPos { ... }` literal in its test)

**Interfaces:**
- Consumes: nothing.
- Produces: `SeriesPos.id: Option<String>` (serde default, left out when `None`), and `factory_core::next_series_id(series_ids: &[String]) -> String`, which returns `S1` for an empty list and `S<max+1>` otherwise, ignoring entries that are not `S<digits>`.

- [ ] **Step 1: Write the failing tests**

Add inside `mod tests` of `ids.rs` (the `s` helper already exists there):

```rust
    #[test]
    fn series_ids_count_up_from_the_largest_seen() {
        assert_eq!(next_series_id(&[]), "S1");
        assert_eq!(next_series_id(&s(&["S1", "S3", "junk", "S2"])), "S4");
        assert_eq!(next_series_id(&s(&["S9", "S10"])), "S11");
    }
```

Add inside `mod tests` of `board.rs`:

```rust
    #[test]
    fn series_pos_id_is_optional_and_left_out_when_none() {
        let old: SeriesPos = serde_json::from_str(r#"{"plan_file":"p.md","index":1,"total":2}"#).expect("old shape");
        assert_eq!(old.id, None);
        let text = serde_json::to_string(&old).expect("json");
        assert!(!text.contains("\"id\""), "{text}");
        let with = SeriesPos { id: Some("S2".into()), ..old };
        let text = serde_json::to_string(&with).expect("json");
        assert!(text.contains(r#""id":"S2""#), "{text}");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-core series`
Expected: compile errors, `next_series_id` not found and no field `id` on `SeriesPos`.

- [ ] **Step 3: Write the implementation**

In `board.rs`, replace the `SeriesPos` struct with:

```rust
/// Where a job sits in a series of plans.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeriesPos {
    pub plan_file: String,
    pub index: u32,
    pub total: u32,
    /// The id of the series, `S1`, `S2` and so on. All jobs of one series share it. Absent in series
    /// made before the plans got their jobs up front.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}
```

In `ids.rs`, add above `#[cfg(test)]`:

```rust
/// The next series id is `S` plus one more than the largest series number seen. Other ids are ignored.
pub fn next_series_id(series_ids: &[String]) -> String {
    let max = series_ids
        .iter()
        .filter_map(|id| id.strip_prefix('S'))
        .filter_map(|n| n.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("S{}", max.saturating_add(1))
}
```

In `lib.rs` line 18, add `next_series_id` to the list:

```rust
pub use ids::{job_dir_name, next_job_id, next_series_id, normalize_id, slugify};
```

Add `id: None` to the three existing literals so the crates still compile:
- `runloop.rs`, in `create_job`: `Some(SeriesPos { plan_file: first.to_string_lossy().into_owned(), index: 1, total: series.plans.len() as u32, id: None })`
- `runloop.rs`, in `run_series`: `j.series = Some(SeriesPos { plan_file: next_name, index: pos.index + 1, total: pos.total, id: None });`
- `status.rs` test: `factory_core::SeriesPos { plan_file: "p".into(), index: 2, total: 5, id: None }`

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-core`
Expected: PASS, including the two new tests.

Run: `cargo check -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine -p factory-cli --tests`
Expected: no errors.

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Paths: the five files above. Suggested message: `feat(factory): add an optional series id to the board`.

---

### Task 2: Create every plan's job up front

**Files:**
- Modify: `tools/factory/crates/factory-engine/src/runloop.rs` (replace `create_job`, add `add_job`, change the fallback in `run_series`)
- Modify: `tools/factory/crates/factory-engine/tests/runloop.rs` (new tests)

**Interfaces:**
- Consumes: `factory_core::next_series_id`, `SeriesPos.id` (Task 1), `parse_series(root, path) -> Series` where `Series.plans[i].path` is the plan file.
- Produces: `create_job(driver, slug, feature, plan, model) -> Result<Job, EngineError>` with the same signature, now returning plan 1's job after it created the whole series. A private `add_job(driver, slug, feature, series: Option<SeriesPos>, stage: Stage, model) -> Result<Job, EngineError>`.

- [ ] **Step 1: Write the failing tests**

Append to `factory-engine/tests/runloop.rs`:

```rust
#[test]
fn create_job_parks_later_plans_at_spec_with_one_series_id() {
    let fx = common::SeriesFixture::new("x");
    let a = fx.plan_file("plan-01-a.md", 2, "plan-02-b.md");
    fx.plan_file("plan-02-b.md", 3, "plan-03-c.md");
    fx.plan_file("plan-03-c.md", 1, "none");
    let first = create_job(&fx.driver, "feat", "three plans", Some(&a), None).expect("create");
    assert_eq!(first.id, "001");
    assert_eq!(first.stage, factory_core::Stage::Build);
    let board = fx.driver.store().load().expect("board");
    assert_eq!(board.jobs.len(), 3);
    let stages: Vec<_> = board.jobs.iter().map(|j| j.stage).collect();
    assert_eq!(stages, vec![factory_core::Stage::Build, factory_core::Stage::Spec, factory_core::Stage::Spec]);
    let ids: Vec<_> = board.jobs.iter().map(|j| j.series.as_ref().and_then(|s| s.id.clone())).collect();
    assert_eq!(ids, vec![Some("S1".to_string()); 3]);
    let places: Vec<_> = board.jobs.iter().map(|j| j.series.as_ref().map(|s| (s.index, s.total))).collect();
    assert_eq!(places, vec![Some((1, 3)), Some((2, 3)), Some((3, 3))]);
    for job in &board.jobs {
        assert!(fx.driver.job_dir(job).join("plan.md").exists(), "plan.md for job {}", job.id);
    }
    assert_eq!(board.jobs[1].slug, "plan-02-b");
}

#[test]
fn a_second_series_gets_the_next_series_id() {
    let fx = common::SeriesFixture::new("x");
    let a = fx.plan_file("plan-01-a.md", 1, "plan-02-b.md");
    fx.plan_file("plan-02-b.md", 1, "none");
    create_job(&fx.driver, "one", "f", Some(&a), None).expect("first series");
    create_job(&fx.driver, "two", "f", Some(&a), None).expect("second series");
    let board = fx.driver.store().load().expect("board");
    let ids: Vec<_> = board.jobs.iter().filter_map(|j| j.series.as_ref().and_then(|s| s.id.clone())).collect();
    assert_eq!(ids, vec!["S1", "S1", "S2", "S2"]);
}

#[test]
fn a_single_plan_gets_no_series_id_and_no_extra_jobs() {
    let fx = common::SeriesFixture::new("x");
    let a = fx.plan_file("plan-01-a.md", 1, "none");
    create_job(&fx.driver, "solo", "f", Some(&a), None).expect("create");
    let board = fx.driver.store().load().expect("board");
    assert_eq!(board.jobs.len(), 1);
    assert_eq!(board.jobs[0].series.as_ref().and_then(|s| s.id.clone()), None);
}

#[test]
fn a_broken_chain_creates_no_jobs() {
    let fx = common::SeriesFixture::new("x");
    let a = fx.plan_file("plan-01-a.md", 1, "plan-02-missing.md");
    assert!(create_job(&fx.driver, "bad", "f", Some(&a), None).is_err());
    assert!(fx.driver.store().load().expect("board").jobs.is_empty());
}

#[test]
fn spec_md_is_copied_into_plan_one_only_and_only_for_a_folder() {
    let fx = common::SeriesFixture::new("x");
    let a = fx.plan_file("plan-01-a.md", 1, "plan-02-b.md");
    fx.plan_file("plan-02-b.md", 1, "none");
    let folder = a.parent().expect("plans folder").to_path_buf();
    std::fs::write(folder.join("spec.md"), "# Spec: Demo\n\n## Goal\n\nDo it.\n").expect("spec");
    // A single file as the argument has no folder, so nothing is copied and nothing fails.
    create_job(&fx.driver, "file", "f", Some(&a), None).expect("file argument");
    let board = fx.driver.store().load().expect("board");
    assert!(!fx.driver.job_dir(&board.jobs[0]).join("spec.md").exists());
    // A folder copies the spec into plan 1 and not into plan 2.
    create_job(&fx.driver, "folder", "f", Some(&folder), None).expect("folder argument");
    let board = fx.driver.store().load().expect("board");
    assert!(fx.driver.job_dir(&board.jobs[2]).join("spec.md").exists());
    assert!(!fx.driver.job_dir(&board.jobs[3]).join("spec.md").exists());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test runloop create_job`
Expected: FAIL, the board holds one job where the tests expect three.

- [ ] **Step 3: Write the implementation**

In `runloop.rs`, replace the whole `create_job` function (and its doc comment) with the two functions below. Keep the existing `use` lines.

```rust
/// Adds one job to the board and makes its folder, ledger and plan copy. A job that runs now starts at
/// stage build. A plan that waits for its turn starts at stage spec and moves to build when it runs.
fn add_job(
    driver: &Driver,
    slug: &str,
    feature: &str,
    series: Option<SeriesPos>,
    stage: Stage,
    model: Option<&str>,
) -> Result<Job, EngineError> {
    // Folder and branch names keep a new id from colliding with a leftover of an older job.
    let dirs: Vec<String> = std::fs::read_dir(driver.root().join(".superpowers/sdd"))
        .map(|rd| rd.filter_map(|e| e.ok()).filter_map(|e| e.file_name().into_string().ok()).collect())
        .unwrap_or_default();
    let branches = driver.repo().local_branch_names()?;
    let job = driver.store().update(|board| {
        let ids: Vec<String> = board.jobs.iter().map(|j| j.id.clone()).collect();
        let id = factory_core::next_job_id(&ids, &dirs, &branches);
        let slug = factory_core::slugify(slug);
        let date = today();
        let dir = factory_core::job_dir_name(&date, &id, &slug);
        let now = now_secs();
        let mut job = Job::new(&id, &dir, &slug, &date, feature, now);
        if stage != Stage::Spec {
            job.set_stage(stage, None, false, now)?;
        }
        job.series = series;
        job.model = model.map(str::to_string);
        job.budget_weighted = Some(driver.config().budget_weighted);
        board.jobs.push(job.clone());
        Ok(job)
    })?;
    let dir = driver.job_dir(&job);
    std::fs::create_dir_all(&dir)?;
    Ledger::new(dir.join("progress.md"))
        .create(&format!("# SDD ledger, plan: .superpowers/sdd/{}/plan.md", job.dir))?;
    if let Some(pos) = &job.series {
        std::fs::copy(&pos.plan_file, dir.join("plan.md"))?;
    }
    Ok(job)
}

/// Creates the jobs of a series and returns plan 1's job, which starts at stage build. Every later plan
/// gets its job now too, parked at stage spec, so the board shows the whole series from the start.
pub fn create_job(
    driver: &Driver,
    slug: &str,
    feature: &str,
    plan: Option<&Path>,
    model: Option<&str>,
) -> Result<Job, EngineError> {
    let Some(start) = plan else {
        return add_job(driver, slug, feature, None, Stage::Build, model);
    };
    // A broken chain fails here, before any job exists.
    let series = parse_series(driver.root(), start)?;
    let total = series.plans.len() as u32;
    // A single plan needs no series id. A longer chain shares one id between all its jobs.
    let series_id = if total > 1 {
        let used: Vec<String> = driver
            .store()
            .load()?
            .jobs
            .iter()
            .filter_map(|j| j.series.as_ref().and_then(|s| s.id.clone()))
            .collect();
        Some(factory_core::next_series_id(&used))
    } else {
        None
    };
    let pos = |index: usize| SeriesPos {
        plan_file: series.plans[index].path.to_string_lossy().into_owned(),
        index: index as u32 + 1,
        total,
        id: series_id.clone(),
    };
    // A folder starts at its first plan file.
    let first = add_job(driver, slug, feature, Some(pos(0)), Stage::Build, model)?;
    // The spec sits beside the plans. Only plan 1 gets a copy, so later builders are not pointed at it.
    let spec = start.join("spec.md");
    if start.is_dir() && spec.is_file() {
        std::fs::copy(&spec, driver.job_dir(&first).join("spec.md"))?;
    }
    for (index, later) in series.plans.iter().enumerate().skip(1) {
        let stem = later.path.file_stem().and_then(|s| s.to_str()).unwrap_or("plan").to_string();
        add_job(driver, &stem, feature, Some(pos(index)), Stage::Spec, model)?;
    }
    Ok(first)
}
```

In `run_series`, replace the `None => { ... }` arm of the `current = match existing` block with:

```rust
            None => {
                // A series made before plans got their jobs up front: create this plan's job now.
                let stem = next.file_stem().and_then(|s| s.to_str()).unwrap_or("plan").to_string();
                let next_pos =
                    SeriesPos { plan_file: next_name, index: pos.index + 1, total: pos.total, id: pos.id.clone() };
                add_job(driver, &stem, &job.feature, Some(next_pos), Stage::Build, job.model.as_deref())?.id
            }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test runloop`
Expected: the five new tests PASS. `run_series_stops_when_a_plan_needs_a_human` now FAILS on its job count of 1. That test is changed in Task 3, so leave it for now.

Run: `cargo check -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine -p factory-cli --tests`
Expected: no errors.

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Paths: `runloop.rs` and `tests/runloop.rs` under `factory-engine`. Suggested message: `feat(factory): create every plan job up front`. If the stale test blocks the commit gate, do Task 3 step 1 first and commit both together.

---

### Task 3: Run a series over waiting jobs, then docs

**Files:**
- Modify: `tools/factory/crates/factory-engine/tests/runloop.rs` (change one test, add three)
- Modify: `.claude/software-factory.md` (after the `- Job directory` line, near line 28)
- Modify: `.claude/commands/factory.md:49` (step 4 of the new-job flow)

**Interfaces:**
- Consumes: `create_job` and the waiting jobs from Task 2, `run_series(driver, id) -> Result<RunEnd, EngineError>`, `store().update_job`.
- Produces: tests that pin the series behavior, and the docs.

- [ ] **Step 1: Change the stale test and add the new ones**

In `run_series_stops_when_a_plan_needs_a_human`, replace the last line (`assert_eq!(fx.driver.store().load()...jobs.len(), 1);`) with:

```rust
    let board = fx.driver.store().load().expect("board");
    assert_eq!(board.jobs.len(), 2, "plan 2 has its job from the start");
    assert_eq!(board.jobs[1].stage, factory_core::Stage::Spec, "and it never started");
```

In `run_series_chains_plans_and_each_job_writes_its_own_job_id_into_events`, after the `assert_eq!(board.jobs[1].series...)` line, add:

```rust
    assert_eq!(
        board.jobs[1].series.as_ref().and_then(|s| s.id.clone()),
        board.jobs[0].series.as_ref().and_then(|s| s.id.clone()),
        "one series id"
    );
    let stages: Vec<_> = board.jobs[1].stage_history.iter().map(|e| e.stage).collect();
    assert_eq!(stages[..2], [factory_core::Stage::Spec, factory_core::Stage::Build], "parked, then built");
    assert_ne!(board.jobs[0].base, board.jobs[1].base, "plan 2 builds on top of plan 1's merge");
```

Append these tests:

```rust
#[test]
fn an_old_series_without_an_id_still_gets_its_next_job_after_the_merge() {
    let fx = common::SeriesFixture::new("fine\nVERDICT: APPROVE");
    let a = fx.plan_file("plan-01-a.md", 1, "plan-02-b.md");
    fx.plan_file("plan-02-b.md", 1, "none");
    let job = create_job(&fx.driver, "old", "two plans", Some(&a), None).expect("create");
    // Make the board look like one written before the plans got their jobs up front.
    fx.driver
        .store()
        .update(|board| {
            board.jobs.truncate(1);
            if let Some(series) = board.jobs[0].series.as_mut() {
                series.id = None;
            }
            Ok(())
        })
        .expect("rewrite board");
    assert_eq!(run_series(&fx.driver, &job.id).expect("series"), RunEnd::Merged);
    let board = fx.driver.store().load().expect("board");
    assert_eq!(board.jobs.len(), 2);
    assert!(board.jobs.iter().all(|j| j.stage == factory_core::Stage::Merged));
    assert_eq!(board.jobs[1].series.as_ref().and_then(|s| s.id.clone()), None);
}

#[test]
fn a_cancelled_plan_stops_the_series_and_later_plans_stay_at_spec() {
    let fx = common::SeriesFixture::new("fine\nVERDICT: APPROVE");
    let a = fx.plan_file("plan-01-a.md", 1, "plan-02-b.md");
    fx.plan_file("plan-02-b.md", 1, "plan-03-c.md");
    fx.plan_file("plan-03-c.md", 1, "none");
    let job = create_job(&fx.driver, "cancel", "three plans", Some(&a), None).expect("create");
    fx.driver
        .store()
        .update_job("002", |j| {
            j.set_stage(factory_core::Stage::NeedsHuman, Some("cancelled: test"), false, 1.0)?;
            j.cancelled = true;
            Ok(())
        })
        .expect("cancel plan 2");
    assert!(matches!(run_series(&fx.driver, &job.id).expect("series"), RunEnd::NeedsHuman(_)));
    let board = fx.driver.store().load().expect("board");
    assert_eq!(board.jobs[0].stage, factory_core::Stage::Merged);
    assert_eq!(board.jobs[2].stage, factory_core::Stage::Spec);
}

#[test]
fn run_job_moves_a_waiting_plan_to_build_by_itself() {
    let fx = common::SeriesFixture::new("fine\nVERDICT: APPROVE");
    let a = fx.plan_file("plan-01-a.md", 1, "plan-02-b.md");
    fx.plan_file("plan-02-b.md", 1, "none");
    create_job(&fx.driver, "waiting", "two plans", Some(&a), None).expect("create");
    assert_eq!(run_job(&fx.driver, "002").expect("run plan 2 alone"), RunEnd::Merged);
    assert_eq!(fx.driver.job("002").expect("job").stage, factory_core::Stage::Merged);
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test runloop`
Expected: all PASS. The old-series test leaves plan 2's job folder behind after the truncate, so the job made by the fallback gets id `003`. The test only checks the job count, the stages and the missing id, so that is fine.

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli`
Expected: PASS (the e2e two-plan test and `new_to_merged` keep working, because `new` still prints the same line and `run 1 --series` runs plan 1 then the waiting plan 2).

- [ ] **Step 3: Update the docs**

In `.claude/software-factory.md`, after the line that starts `- Job directory`, add:

```markdown
- `factory new --plans` makes one job per plan at once. Plan 1 starts at `build`. Plans 2 to N wait at `spec` and share a series id `S<n>` (`series.id` on the board). `spec.md` is copied into plan 1's job folder only, so builders of later plans are not pointed at it. `run --series` runs the next waiting job when the plan before it merges, and `run <id>` on a waiting job moves it to `build`. A series with one plan has no id. A series made before this change has no id either, and its next job is created after the previous one merges.
```

In `.claude/commands/factory.md`, step 4 of "`<feature>`: new job", replace the sentence ``Then copy `spec.md` to `.superpowers/sdd/<folder>/spec.md`.`` with:

```markdown
The driver copies `spec.md` from the staging folder into the first job's folder.
```

- [ ] **Step 4: Run the lint tests and the crate checks**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli`
Expected: PASS, including `agent_lint`.

Run: `cargo clippy -j4 --manifest-path tools/factory/Cargo.toml -p factory-core -p factory-engine -p factory-cli --tests`
Expected: no warnings.

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Paths: `factory-engine/tests/runloop.rs`, `.claude/software-factory.md`, `.claude/commands/factory.md`. Suggested message: `docs(factory): describe jobs created up front for a series`.
