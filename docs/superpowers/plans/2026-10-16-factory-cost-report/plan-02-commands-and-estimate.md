# Factory Cost Report, Plan 02: costs.json, the estimate, `factory costs` and the server

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The driver writes `costs.json` after each merge and puts an `estimate` on every job it creates from a plan. `factory costs` prints the report, `factory new` prints the estimates, and `factory-server` serves the group summary in `/api/state`.

**Architecture:** `factory-engine` gets `refresh_costs` (gather, build `CostsFile`, write) called best effort after a merge, and an estimate step in `add_job`. `factory-cli` gets a `costs` command and an estimate line in `new`. `factory-server` (its own workspace) reads `costs.json` leniently and adds `costs` and the job `estimate` to the state, never the per-job list.

**Tech Stack:** Rust 2021, serde, serde_json, clap, tiny_http, tempfile (tests). Workspaces `tools/factory/` and `tools/factory-server/`.

**Spec:** `docs/superpowers/specs/2026-10-16-factory-cost-report-design.md` (Design 3, 4 and Testing). Plan 01 is merged on this branch first: `factory_core::{CostsFile, JobCost, GroupStats, Estimate, stats, estimate_for}` and `factory_engine::gather_costs` exist, and `Job.estimate` is on the board.

## Global Constraints

- Commands: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p <crate> <test>` for the driver crates, `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml` for the server. One crate per call. Never `--workspace`, never omit `-j4`, never set `CARGO_TARGET_DIR`.
- No panicking unwrap calls in production code (a hook blocks that text in Rust edits; `unwrap_or`, `unwrap_or_default` and `map_or` are fine). `.expect("...")` only inside tests.
- Comments are short full sentences ending in a period. No emojis. No dollar amounts. No LLM calls.
- A failed `costs.json` refresh must never change the result of a merge or of job creation. The server never writes any file.
- The server reads `costs.json` leniently: a missing or unreadable file gives `costs: null`, a bad `estimate` on a board job is dropped.
- `/api/state` carries only the group figures and the role split, never the per-job list.
- Token figures are printed with the existing `human()` helper in `factory-cli/src/status.rs` (`850`, `12.3K`, `1.2M`).
- Commit steps are for a human running the plan by hand: pathspec commit only, message through the skill `dev-workflow-skills:1-git-commit`, never `git add -A` or `git add .`.

## Review Focus

1. A refresh that cannot write (a directory sits where `costs.json` goes) must leave the merge result `Merged`. Pinned in Task 1.
2. The estimate must come from history only: creating a job never changes the stats, and with no history `estimate` is `None` and `factory new` says so. Pinned in Tasks 1 and 2.
3. A job with no data, an approximate job and a group of fewer than 3 jobs must show `no data`, `~` and `few jobs` in `factory costs`. Pinned in Task 2.
4. A broken or partial `costs.json` and a malformed `estimate` must not turn the server board into an error. Pinned in Task 3.
5. The state JSON must not contain the per-job list or any job's `usage` lines. Pinned in Task 3.

## File Structure

```
tools/factory/crates/factory-engine/src/costs.rs     costs_path, refresh_costs, estimate_for_plan
tools/factory/crates/factory-engine/src/merge.rs     best-effort refresh after a merge
tools/factory/crates/factory-engine/src/runloop.rs   add_job stores the estimate
tools/factory/crates/factory-engine/src/lib.rs       re-exports
tools/factory/crates/factory-engine/tests/costs.rs   refresh and estimate tests
tools/factory/crates/factory-cli/src/costs.rs        render_costs, estimate lines
tools/factory/crates/factory-cli/src/cli.rs          Command::Costs
tools/factory/crates/factory-cli/src/commands.rs     costs command, estimate in new
tools/factory/crates/factory-cli/src/status.rs       human() made pub(crate)
tools/factory/crates/factory-cli/src/lib.rs          mod costs
tools/factory/crates/factory-cli/tests/e2e.rs        costs and new output
tools/factory-server/src/model.rs                    Estimate, Job.estimate
tools/factory-server/src/costs.rs                    CostsView, load_costs
tools/factory-server/src/main.rs                     mod costs
tools/factory-server/src/state.rs                    StateView.costs
tools/factory-server/src/server.rs                   state test
```

---

### Task 1: refresh_costs, the merge hook and the estimate on new jobs (factory-engine)

**Files:**
- Modify: `tools/factory/crates/factory-engine/src/costs.rs`
- Modify: `tools/factory/crates/factory-engine/src/merge.rs`
- Modify: `tools/factory/crates/factory-engine/src/runloop.rs`
- Modify: `tools/factory/crates/factory-engine/src/lib.rs`
- Modify: `tools/factory/crates/factory-engine/tests/costs.rs`

**Interfaces:**
- Consumes: `gather_costs`, `factory_core::{CostsFile, stats, estimate_for, Estimate}`, `Driver::{root, config, store, job_dir}`, `now_secs()`.
- Produces: `pub fn costs_path(root: &Path) -> PathBuf` (`<root>/.superpowers/factory/costs.json`), `pub fn refresh_costs(driver: &Driver) -> Result<(), EngineError>`, `pub fn estimate_for_plan(driver: &Driver, plan_bytes: u64) -> Option<Estimate>`, and `Job.estimate` set by `create_job_with_spec` for jobs made from a plan.

- [ ] **Step 1: Write the failing tests**

Add to `tools/factory/crates/factory-engine/tests/costs.rs` (it already has `add_finished_job`; add `use factory_engine::{costs_path, create_job, estimate_for_plan, refresh_costs, run_job, RunEnd};`):

```rust
fn make_merged(fx: &common::SeriesFixture, id: &str, plan_text: &str, weighted_input: u64) {
    let usage = format!(
        "{{\"job\":\"{id}\",\"role\":\"builder\",\"input\":{weighted_input},\"cache_creation\":0,\"output\":0,\"cache_read\":0,\"usage_v\":2}}\n"
    );
    let job = add_finished_job(fx, id, plan_text, Some(&usage));
    fx.driver
        .store()
        .update_job(&job.id, |j| j.set_stage(Stage::Merged, None, true, 3.0))
        .expect("merged");
}

#[test]
fn refresh_costs_writes_groups_and_roles_but_no_job_list() {
    let fx = common::SeriesFixture::new("x");
    make_merged(&fx, "101", "# tiny\n", 200);
    refresh_costs(&fx.driver).expect("refresh");
    let raw = std::fs::read_to_string(costs_path(fx.driver.root())).expect("costs.json");
    let json: serde_json::Value = serde_json::from_str(&raw).expect("json");
    assert_eq!(json["groups"]["small"]["n"], 1);
    assert_eq!(json["groups"]["small"]["median"], 200);
    assert_eq!(json["roles"]["builder"], 100);
    assert!(json.get("jobs").is_none(), "{raw}");
}

#[test]
fn estimate_for_plan_uses_history_and_is_none_without_it() {
    let fx = common::SeriesFixture::new("x");
    assert_eq!(estimate_for_plan(&fx.driver, 100), None);
    make_merged(&fx, "101", "# tiny\n", 200);
    make_merged(&fx, "102", "# tiny\n", 400);
    let e = estimate_for_plan(&fx.driver, 100).expect("estimate");
    assert_eq!((e.group.as_str(), e.median, e.low, e.high, e.n), ("small", 300, 200, 400, 2));
    // A complex plan has no complex history, so the overall figures are used.
    let all = estimate_for_plan(&fx.driver, 40_000).expect("fallback");
    assert_eq!((all.group.as_str(), all.n), ("all", 2));
}

#[test]
fn a_new_job_from_a_plan_gets_an_estimate_and_a_new_job_does_not_change_the_history() {
    let fx = common::SeriesFixture::new("fine\nVERDICT: APPROVE");
    make_merged(&fx, "101", "# tiny\n", 200);
    let plan = fx.plan_file("plan-01-a.md", 1, "none");
    let job = create_job(&fx.driver, "feat", "a feature", Some(&plan), None).expect("create");
    let est = job.estimate.as_ref().expect("estimate on the returned job");
    assert_eq!((est.group.as_str(), est.median, est.n), ("small", 200, 1));
    let on_board = fx.driver.job(&job.id).expect("job");
    assert_eq!(on_board.estimate, job.estimate);
    // The new job has no usage and is not merged, so the history is unchanged.
    let again = estimate_for_plan(&fx.driver, 100).expect("estimate");
    assert_eq!(again.n, 1);
}

#[test]
fn a_job_made_without_a_plan_has_no_estimate() {
    let fx = common::SeriesFixture::new("x");
    make_merged(&fx, "101", "# tiny\n", 200);
    let job = create_job(&fx.driver, "bare", "no plan", None, None).expect("create");
    assert_eq!(job.estimate, None);
}

#[test]
fn a_merge_refreshes_costs_json_and_a_failed_refresh_does_not_fail_the_merge() {
    let fx = common::SeriesFixture::new("fine\nVERDICT: APPROVE");
    let plan = fx.plan_file("plan-01-a.md", 1, "none");
    let job = create_job(&fx.driver, "ok", "a feature", Some(&plan), None).expect("create");
    assert_eq!(run_job(&fx.driver, &job.id).expect("run"), RunEnd::Merged);
    let raw = std::fs::read_to_string(costs_path(fx.driver.root())).expect("costs.json after the merge");
    assert!(raw.contains("\"groups\""), "{raw}");

    // A directory where the file goes makes the write fail.
    let fx2 = common::SeriesFixture::new("fine\nVERDICT: APPROVE");
    std::fs::create_dir_all(costs_path(fx2.driver.root())).expect("blocker dir");
    let plan2 = fx2.plan_file("plan-01-a.md", 1, "none");
    let job2 = create_job(&fx2.driver, "blocked", "a feature", Some(&plan2), None).expect("create");
    assert_eq!(run_job(&fx2.driver, &job2.id).expect("run"), RunEnd::Merged);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test costs`
Expected: compile errors (`costs_path`, `refresh_costs`, `estimate_for_plan` not found, `Job.estimate` not set).

- [ ] **Step 3: Implement**

Append to `factory-engine/src/costs.rs` (add the imports `use std::path::{Path, PathBuf};` and extend the core import to `use factory_core::{count_tasks, estimate_for, group_of, stats, usage_summary, CostsFile, Estimate, JobCost};`, and `use crate::clock::now_secs;`):

```rust
/// Where the dashboard reads the group figures.
pub fn costs_path(root: &Path) -> PathBuf {
    root.join(".superpowers/factory/costs.json")
}

/// Rebuilds `costs.json` from the board, the job folders and git.
pub fn refresh_costs(driver: &Driver) -> Result<(), EngineError> {
    let jobs = gather_costs(driver)?;
    let file = CostsFile::build(&jobs, driver.config(), now_secs());
    file.write(&costs_path(driver.root())).map_err(|e| EngineError::Core(e.to_string()))
}

/// The estimate for a plan of `plan_bytes` bytes, from the history now. `None` without history.
pub fn estimate_for_plan(driver: &Driver, plan_bytes: u64) -> Option<Estimate> {
    let jobs = gather_costs(driver).ok()?;
    estimate_for(plan_bytes, &stats(&jobs), driver.config())
}
```

In `lib.rs` change the Plan 01 line to `pub use costs::{costs_path, estimate_for_plan, gather_costs, refresh_costs};`.

`merge.rs`, in `merge_stage`, replace the `Ok(Step::Merged(sha))` arm body so the refresh runs after `finish_merge` and before the KB refresh:

```rust
        Ok(Step::Merged(sha)) => {
            finish_merge(driver, id, &sha)?;
            // The merge is done, so a failed refresh must not change the result.
            if let Err(err) = crate::costs::refresh_costs(driver) {
                eprintln!("costs refresh error for job {id}: {err}");
            }
            if let Err(err) = driver.kb_refresh(id, false) {
                eprintln!("kb refresh error for job {id}: {err}");
            }
            Ok(MergeResult::Merged(sha))
        }
```

`runloop.rs` `add_job`: replace the tail (from `let dir = driver.job_dir(&job);` to the end) with:

```rust
    let mut job = job;
    let dir = driver.job_dir(&job);
    std::fs::create_dir_all(&dir)?;
    Ledger::new(dir.join("progress.md"))
        .create(&format!("# SDD ledger, plan: .superpowers/sdd/{}/plan.md", job.dir))?;
    if let Some(pos) = &job.series {
        std::fs::copy(&pos.plan_file, dir.join("plan.md"))?;
        // The estimate is best effort: no history or a failed read gives no estimate, never an error.
        let bytes = std::fs::metadata(dir.join("plan.md")).map(|m| m.len()).unwrap_or(0);
        if let Some(estimate) = crate::costs::estimate_for_plan(driver, bytes) {
            let stored = estimate.clone();
            driver.store().update_job(&job.id, |j| {
                j.estimate = Some(stored);
                Ok(())
            })?;
            job.estimate = Some(estimate);
        }
    }
    Ok(job)
```

- [ ] **Step 4: Run the tests to verify they pass**

Run, one per call: `-p factory-engine --test costs`, then `-p factory-engine` (all), then `-p factory-cli`, then `cargo check -j4 --manifest-path tools/factory/Cargo.toml`.
Expected: all pass. Existing tests that compare whole `Job` values from `create_job` keep passing because the estimate is `None` without history.

- [ ] **Step 5: Commit**

Commit the files of this task with the skill message `feat(factory): write costs.json after a merge and estimate new jobs`.

---

### Task 2: `factory costs` and the estimate in `factory new` (factory-cli)

**Files:**
- Create: `tools/factory/crates/factory-cli/src/costs.rs`
- Modify: `tools/factory/crates/factory-cli/src/cli.rs`
- Modify: `tools/factory/crates/factory-cli/src/commands.rs`
- Modify: `tools/factory/crates/factory-cli/src/status.rs` (make `human` `pub(crate)`)
- Modify: `tools/factory/crates/factory-cli/src/lib.rs` (add `mod costs;`)
- Modify: `tools/factory/crates/factory-cli/tests/e2e.rs`

**Interfaces:**
- Consumes: `gather_costs`, `costs_path`, `factory_core::{CostsFile, JobCost, GroupStats, Estimate, stats}`, `now_secs()` (already imported in `commands.rs`), `human()`.
- Produces: `pub fn render_costs(jobs: &[JobCost], groups: &BTreeMap<String, GroupStats>) -> String`, `pub fn format_estimate(e: &Estimate) -> String`, `pub fn estimate_lines(board: &Board, first: &Job) -> String`, and the `Command::Costs` variant.

- [ ] **Step 1: Write the failing tests**

Create `tools/factory/crates/factory-cli/src/costs.rs` with the tests module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use factory_core::{stats, JobCost, SizeGroup};
    use std::collections::BTreeMap;

    fn cost(id: &str, group: Option<SizeGroup>, stage: &str, weighted: u64, approx: bool, runs: u32) -> JobCost {
        let mut roles = BTreeMap::new();
        roles.insert("builder".to_string(), weighted * 6 / 10);
        roles.insert("reviewer".to_string(), weighted - weighted * 6 / 10);
        JobCost {
            id: id.into(),
            slug: "s".into(),
            stage: stage.into(),
            group,
            plan_bytes: 5000,
            tasks: 2,
            lines: Some(300),
            files: Some(4),
            weighted,
            raw: weighted * 4,
            runs,
            approx,
            roles,
        }
    }

    #[test]
    fn the_per_job_table_marks_approximate_and_no_data_jobs() {
        let jobs = vec![
            cost("003", Some(SizeGroup::Small), "merged", 300_000, true, 2),
            cost("004", Some(SizeGroup::Small), "merged", 250_000, false, 3),
            cost("005", None, "build", 0, false, 0),
        ];
        let text = render_costs(&jobs, &stats(&jobs));
        let line = |id: &str| text.lines().find(|l| l.starts_with(id)).unwrap_or("").to_string();
        assert!(line("003").contains("300.0K ~"), "{text}");
        assert!(!line("004").contains('~'), "{text}");
        assert!(line("005").contains("no data"), "{text}");
        assert!(line("003").contains("small") && line("003").contains("300"), "{text}");
    }

    #[test]
    fn a_group_table_shows_jobs_median_range_and_notes() {
        let jobs = vec![
            cost("1", Some(SizeGroup::Small), "merged", 200_000, true, 1),
            cost("2", Some(SizeGroup::Small), "merged", 400_000, false, 1),
        ];
        let text = render_costs(&jobs, &stats(&jobs));
        let small = text.lines().find(|l| l.trim_start().starts_with("small")).expect("small row");
        assert!(small.contains("300.0K"), "{small}");
        assert!(small.contains("200.0K-400.0K"), "{small}");
        assert!(small.contains("few jobs"), "{small}");
        assert!(small.contains("1 approx"), "{small}");
        assert!(text.contains("corrected: 1 job, median 400.0K"), "{text}");
    }

    #[test]
    fn the_role_split_is_printed() {
        let jobs = vec![cost("1", Some(SizeGroup::Small), "merged", 1_000_000, false, 1)];
        let text = render_costs(&jobs, &stats(&jobs));
        assert!(text.contains("builder 60%") && text.contains("reviewer 40%"), "{text}");
    }

    #[test]
    fn no_history_says_so() {
        let jobs = vec![cost("1", Some(SizeGroup::Small), "build", 0, false, 0)];
        let text = render_costs(&jobs, &stats(&jobs));
        assert!(text.contains("no merged jobs with data yet"), "{text}");
    }

    #[test]
    fn estimates_read_as_one_line() {
        let e = factory_core::Estimate { group: "small".into(), median: 280_000, low: 220_000, high: 370_000, n: 4, n_approx: 4 };
        assert_eq!(format_estimate(&e), "estimate 220.0K to 370.0K weighted, median 280.0K (small, 4 jobs, 4 approximate)");
        let few = factory_core::Estimate { group: "all".into(), median: 10, low: 10, high: 10, n: 1, n_approx: 0 };
        assert_eq!(format_estimate(&few), "estimate 10 to 10 weighted, median 10 (all sizes, 1 job, few jobs)");
    }
}
```

Add to `tools/factory/crates/factory-cli/tests/e2e.rs`:

```rust
#[test]
fn factory_costs_lists_jobs_writes_costs_json_and_new_prints_estimates() -> TestResult {
    let sc = Scenario::new("", vec![build(vec![("a.txt", "a\n")], 1_000), review(APPROVE)])?;
    let plan = sc.plan_a.to_string_lossy().to_string();
    // No history yet, so neither of the two jobs of the series has an estimate.
    let created = sc.run(&["new", "hello", "say hello", "--plans", &plan])?;
    assert!(created.contains("job 001: no estimate yet"), "{created}");
    assert!(created.contains("job 002: no estimate yet"), "{created}");
    let end = sc.run(&["run", "1"])?;
    assert_eq!(end, "merged", "{end}");

    let out = sc.run(&["costs"])?;
    let row = out.lines().find(|l| l.starts_with("001")).expect("job 001 row");
    assert!(row.contains("merged") && row.contains("small"), "{out}");
    assert!(out.contains("ROLES"), "{out}");
    assert!(out.contains("few jobs"), "{out}");
    assert!(sc.root.join(".superpowers/factory/costs.json").exists());

    // Job 001 is now history, so the next plan gets an estimate.
    let again = sc.run(&["new", "second", "another one", "--plans", &plan])?;
    assert!(again.contains("job 003: estimate "), "{again}");
    assert!(again.contains("(small, 1 job, few jobs"), "{again}");
    Ok(())
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli costs`
Expected: compile errors (`render_costs`, `format_estimate` not found, no `costs` command).

- [ ] **Step 3: Implement**

`status.rs`: change `fn human(` to `pub(crate) fn human(`. `lib.rs`: add `mod costs;` and `pub use costs::{estimate_lines, format_estimate, render_costs};`.

`costs.rs` (above the tests):

```rust
//! Text for `factory costs` and the estimate lines of `factory new`.

use std::collections::BTreeMap;

use factory_core::{Board, Estimate, GroupStats, Job, JobCost, SizeGroup};

use crate::status::human;

const FEW_JOBS: u32 = 3;

fn opt(n: Option<u64>) -> String {
    n.map_or("-".to_string(), |v| v.to_string())
}

/// The per-job table, the per-group table, the corrected figures and the role split.
pub fn render_costs(jobs: &[JobCost], groups: &BTreeMap<String, GroupStats>) -> String {
    if jobs.is_empty() {
        return "no jobs\n".to_string();
    }
    let mut out = String::from("JOB  STAGE        GROUP    TASKS  LINES  FILES  WEIGHTED     RAW       RUNS\n");
    for j in jobs {
        let group = j.group.map_or("-", SizeGroup::as_str);
        let weighted = if j.runs == 0 {
            "no data".to_string()
        } else if j.approx {
            format!("{} ~", human(j.weighted))
        } else {
            human(j.weighted)
        };
        out.push_str(&format!(
            "{:<4} {:<12} {:<8} {:<6} {:<6} {:<6} {:<12} {:<9} {}\n",
            j.id,
            j.stage,
            group,
            j.tasks,
            opt(j.lines),
            opt(j.files),
            weighted,
            human(j.raw),
            j.runs
        ));
    }
    out.push('\n');
    if groups.is_empty() {
        out.push_str("no merged jobs with data yet\n");
        return out;
    }
    out.push_str("GROUP    JOBS  MEDIAN     RANGE                NOTES\n");
    let mut corrected_lines = Vec::new();
    let order = ["small", "medium", "complex", "all"];
    for name in order {
        let Some(g) = groups.get(name) else { continue };
        let mut notes = Vec::new();
        if g.n < FEW_JOBS {
            notes.push("few jobs".to_string());
        }
        if g.n_approx > 0 {
            notes.push(format!("{} approx", g.n_approx));
        }
        out.push_str(&format!(
            "{:<8} {:<5} {:<10} {:<20} {}\n",
            name,
            g.n,
            human(g.median),
            format!("{}-{}", human(g.low), human(g.high)),
            notes.join(", ")
        ));
        if let Some(c) = &g.corrected {
            let plural = if c.n == 1 { "job" } else { "jobs" };
            corrected_lines.push(format!("  {name} corrected: {} {plural}, median {} ({}-{})", c.n, human(c.median), human(c.low), human(c.high)));
        }
    }
    if !corrected_lines.is_empty() {
        out.push_str("\nCORRECTED (jobs with exact output counts only)\n");
        for l in corrected_lines {
            out.push_str(&l);
            out.push('\n');
        }
    }
    if let Some(all) = groups.get("all") {
        let roles: Vec<String> = all.roles.iter().map(|(r, p)| format!("{r} {p}%")).collect();
        out.push_str(&format!("\nROLES (share of weighted tokens)\n  {}\n", roles.join("  ")));
    }
    out
}

/// One line for an estimate.
pub fn format_estimate(e: &Estimate) -> String {
    let group = if e.group == "all" { "all sizes".to_string() } else { e.group.clone() };
    let jobs = if e.n == 1 { "1 job".to_string() } else { format!("{} jobs", e.n) };
    let mut notes = vec![group, jobs];
    if e.n < FEW_JOBS {
        notes.push("few jobs".to_string());
    }
    if e.n_approx > 0 {
        notes.push(format!("{} approximate", e.n_approx));
    }
    format!("estimate {} to {} weighted, median {} ({})", human(e.low), human(e.high), human(e.median), notes.join(", "))
}

/// The estimate line for the first job and for every later job of its series.
pub fn estimate_lines(board: &Board, first: &Job) -> String {
    let series_id = first.series.as_ref().and_then(|s| s.id.clone());
    let mut out = String::new();
    for j in &board.jobs {
        let same = j.id == first.id || (series_id.is_some() && j.series.as_ref().and_then(|s| s.id.clone()) == series_id);
        if !same {
            continue;
        }
        match &j.estimate {
            Some(e) => out.push_str(&format!("\njob {}: {}", j.id, format_estimate(e))),
            None => out.push_str(&format!("\njob {}: no estimate yet (no merged jobs with data)", j.id)),
        }
    }
    out
}
```

`cli.rs`: add to `Command` (after `Tokens`):

```rust
    /// Print the cost of every job and the figures per size group, and write costs.json.
    Costs,
```

`commands.rs`: add imports `use factory_core::{CostsFile, ...}` (extend the existing `factory_core` import) and `use factory_engine::{costs_path, gather_costs, ...}` (extend the existing engine import), `use crate::costs::{estimate_lines, render_costs};`; in `dispatch` add `Command::Costs => costs(ctx),`; change `new_job` and add `costs`:

```rust
    let d = ctx.driver()?;
    let job = create_job_with_spec(&d, slug, feature, plans.as_deref(), spec.as_deref(), model.as_deref())?;
    let board = d.store().load()?;
    Ok(format!("created job {} in {}{}", job.id, job.dir, estimate_lines(&board, &job)))
```

```rust
fn costs(ctx: &CliCtx) -> Result<String, CliError> {
    let d = ctx.driver()?;
    let jobs = gather_costs(&d)?;
    let file = CostsFile::build(&jobs, d.config(), now_secs());
    let mut out = render_costs(&jobs, &file.groups);
    if let Err(e) = file.write(&costs_path(d.root())) {
        out.push_str(&format!("\nwarning: could not write costs.json: {e}\n"));
    }
    Ok(out)
}
```

(`new_job` for a job created without `--plans` prints the line `no estimate yet`, which is accurate because such a job has no plan to size.)

- [ ] **Step 4: Run the tests to verify they pass**

Run, one per call: `-p factory-cli` (all), `cargo check -j4 --manifest-path tools/factory/Cargo.toml`.
Expected: all pass. Then run `cargo build -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli` and, from the repo root, `tools/factory/target/debug/factory --root /home/numericlabs/data/rocket/rocket costs` against the real main checkout (it only reads, except it writes `.superpowers/factory/costs.json` which is git-ignored). Expected: the 11 jobs, the group table with `few jobs` and `~` markers, and the role split. Paste the output into the report.

- [ ] **Step 5: Commit**

Commit the files of this task with the skill message `feat(factory): add factory costs and estimate lines in factory new`.

---

### Task 3: costs and estimate in the server state (factory-server)

**Files:**
- Create: `tools/factory-server/src/costs.rs`
- Modify: `tools/factory-server/src/main.rs` (add `mod costs;` next to `mod park;`)
- Modify: `tools/factory-server/src/model.rs`
- Modify: `tools/factory-server/src/state.rs`
- Modify: `tools/factory-server/src/specs.rs` (test helper literals, if they construct `Job` or `StateView`)
- Modify: `tools/factory-server/src/server.rs` (state test)

**Interfaces:**
- Produces:
  - `pub struct Estimate { pub group: String, pub median: u64, pub low: u64, pub high: u64, pub n: u32, pub n_approx: u32 }` (`Serialize, Deserialize, Clone, Debug, PartialEq`) and `Job.estimate: Option<Estimate>` (lenient, skipped when `None`).
  - `pub struct CostsView { pub generated_at: f64, pub groups: BTreeMap<String, GroupView>, pub roles: BTreeMap<String, u32> }`, `pub struct GroupView { pub n: u32, pub n_approx: u32, pub median: u64, pub low: u64, pub high: u64, pub roles: BTreeMap<String, u32>, pub corrected: Option<SpreadView> }`, `pub struct SpreadView { pub n: u32, pub median: u64, pub low: u64, pub high: u64 }` (all `#[serde(default)]`), and `pub fn load_costs(root: &Path) -> Option<CostsView>`.
  - `StateView.costs: Option<CostsView>`.

- [ ] **Step 1: Write the failing tests**

Create `tools/factory-server/src/costs.rs` with its tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, text: &str) {
        let dir = root.join(".superpowers").join("factory");
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("costs.json"), text).expect("costs.json");
    }

    #[test]
    fn a_valid_file_gives_groups_and_roles_and_drops_unknown_fields() {
        let root = tempfile::tempdir().expect("tempdir");
        write(
            root.path(),
            r#"{"generated_at":5.0,"thresholds":{"small_max_bytes":1,"medium_max_bytes":2},"secret":"x","jobs":[{"id":"001"}],
                "groups":{"small":{"n":4,"n_approx":4,"median":280000,"low":220000,"high":370000,"roles":{"builder":62},"corrected":null,"extra":1}},
                "roles":{"builder":62,"reviewer":38}}"#,
        );
        let v = load_costs(root.path()).expect("costs");
        assert_eq!(v.groups["small"].median, 280_000);
        assert_eq!(v.roles["reviewer"], 38);
        let json = serde_json::to_string(&v).expect("json");
        assert!(!json.contains("secret") && !json.contains("\"jobs\"") && !json.contains("extra"), "{json}");
    }

    #[test]
    fn a_missing_or_broken_file_gives_none() {
        let root = tempfile::tempdir().expect("tempdir");
        assert!(load_costs(root.path()).is_none());
        write(root.path(), "not json");
        assert!(load_costs(root.path()).is_none());
        write(root.path(), r#"{"groups":"nope"}"#);
        assert!(load_costs(root.path()).is_none());
    }

    #[test]
    fn corrected_figures_are_kept_when_present() {
        let root = tempfile::tempdir().expect("tempdir");
        write(
            root.path(),
            r#"{"groups":{"small":{"n":3,"n_approx":1,"median":3,"low":1,"high":5,"roles":{},"corrected":{"n":2,"median":4,"low":3,"high":5}}},"roles":{}}"#,
        );
        let v = load_costs(root.path()).expect("costs");
        assert_eq!(v.groups["small"].corrected.as_ref().map(|c| c.median), Some(4));
    }
}
```

Add to the tests module of `state.rs`:

```rust
    #[test]
    fn the_state_carries_the_group_summary_but_never_a_per_job_list() -> TestResult {
        let root = tempfile::tempdir()?;
        let dir = root.path().join(".superpowers/factory");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            dir.join("costs.json"),
            r#"{"generated_at":1.0,"groups":{"small":{"n":1,"n_approx":0,"median":9,"low":9,"high":9,"roles":{},"corrected":null}},"roles":{"builder":100},"jobs":[{"id":"SECRET-JOB"}]}"#,
        )?;
        let view = build_state(&Board::default(), &Registry::new(), 10.0, None, root.path());
        let json = serde_json::to_string(&view)?;
        assert!(json.contains("\"costs\":{"), "{json}");
        assert!(json.contains("\"median\":9"), "{json}");
        assert!(!json.contains("SECRET-JOB"), "{json}");
        let none = build_state(&Board::default(), &Registry::new(), 10.0, None, Path::new("/nonexistent"));
        assert!(serde_json::to_value(&none)?["costs"].is_null());
        Ok(())
    }

    #[test]
    fn a_job_estimate_passes_through_and_a_malformed_one_is_dropped() -> TestResult {
        let good: Board = serde_json::from_str(
            r#"{"jobs":[{"id":"001","dir":"d","slug":"s","created":"c","feature":"f","branch":"b","stage":"build","round":0,"stage_since":1.0,"reviews":{},
                "estimate":{"group":"small","median":250000,"low":220000,"high":310000,"n":4,"n_approx":2}}]}"#,
        )?;
        let view = build_state(&good, &Registry::new(), 10.0, None, Path::new("/nonexistent"));
        let json = serde_json::to_value(&view)?;
        assert_eq!(json["jobs"][0]["estimate"]["median"], 250_000);
        let bad: Board = serde_json::from_str(
            r#"{"jobs":[{"id":"001","dir":"d","slug":"s","created":"c","feature":"f","branch":"b","stage":"build","round":0,"stage_since":1.0,"reviews":{},"estimate":"lots"}]}"#,
        )?;
        assert!(bad.jobs[0].estimate.is_none());
        Ok(())
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml costs`
Expected: compile errors (`load_costs`, `estimate`, `costs` not found).

- [ ] **Step 3: Implement**

`costs.rs` (above the tests):

```rust
//! Reads the driver's `costs.json` for the dashboard. The server never writes it.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Median and range over the jobs with exact output counts.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SpreadView {
    pub n: u32,
    pub median: u64,
    pub low: u64,
    pub high: u64,
}

/// The figures of one size group.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct GroupView {
    pub n: u32,
    pub n_approx: u32,
    pub median: u64,
    pub low: u64,
    pub high: u64,
    pub roles: BTreeMap<String, u32>,
    pub corrected: Option<SpreadView>,
}

/// What the state carries: the group figures and the role split, never the per-job list.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CostsView {
    pub generated_at: f64,
    pub groups: BTreeMap<String, GroupView>,
    pub roles: BTreeMap<String, u32>,
}

/// `None` when the file is missing or unreadable.
pub fn load_costs(root: &Path) -> Option<CostsView> {
    let path = root.join(".superpowers").join("factory").join("costs.json");
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}
```

`main.rs`: add `mod costs;`. `model.rs`: add

```rust
/// The cost estimate the driver stored on a job when it created it.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Estimate {
    pub group: String,
    pub median: u64,
    pub low: u64,
    pub high: u64,
    pub n: u32,
    pub n_approx: u32,
}
```

and to `Job` after `park_at`:

```rust
    /// The cost estimate made when the job was created. A bad value is dropped.
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub estimate: Option<Estimate>,
```

`state.rs`: add `pub costs: Option<crate::costs::CostsView>,` to `StateView` and `costs: crate::costs::load_costs(root),` in the `StateView { .. }` literal in `build_state`. Add `estimate: None,` to the test helper `Job { .. }` literals (`state.rs` `fn job`, and wherever `specs.rs` constructs a `Job`).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml`
Expected: all pass.

- [ ] **Step 5: Commit**

Commit `costs.rs`, `main.rs`, `model.rs`, `state.rs` and `specs.rs` with the skill message `feat(factory-server): serve the cost summary and job estimates`.

Next plan: docs/superpowers/plans/2026-10-16-factory-cost-report/plan-03-dashboard-panel.md
