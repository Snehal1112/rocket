# Factory Spec Cards, Plan 02: Server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `/api/state` gets a `specs[]` list with one spec card per series: the series id, the spec title and goal, the stage the card sits in, and one row per plan with its state.

**Architecture:** The model reads the new `series.id` and the `cancelled` flag leniently. A new `specs.rs` module groups the already built `JobView`s by series id and turns each group into a `SpecCard`. `plan.rs` gets a small `load_spec` that reads `spec.md` of plan 1's job folder with the existing safe-folder check. `build_state` adds the cards to `StateView`.

**Tech Stack:** Rust (`tools/factory-server`), serde, `tempfile` in tests.

**Spec:** `docs/superpowers/specs/2026-10-13-factory-spec-and-plan-cards-design.md`, section "Server". Plan 01 must be done: the board carries `series.id`.

## Global Constraints

- Every cargo call passes `-j4`: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml`. Never use `--workspace` or `--all`.
- Production code never panics on a bad value: no unwrap and no expect outside tests. Tests use `Result` or `expect`.
- New optional board fields are read leniently: a bad value becomes none and never makes the board invalid.
- The server never writes any file. `spec.md` is read only through `safe_dir_name` folders under `.superpowers/sdd`.
- Commits: conventional commits. Use the `dev-workflow-skills:1-git-commit` skill for every commit, with a pathspec commit. Never `git add -A`, `--all` or `.`.
- Comments are short full sentences that end with a period.
- A spec card exists only for a series id shared by jobs whose `series.total` is 2 or more.

## Review Focus

- A `series.id` or `cancelled` of the wrong type does not drop the series or break the board (Task 1 test).
- A job folder name like `../x` cannot make the spec loader read outside the root (Task 2 test).
- Series ids group by first appearance, so `S10` and `S2` do not mix (Task 2 test).
- A series whose plan 1 job is missing still gets a card, with no title (Task 2 test).
- Old jobs without a series id produce no card and no error (Task 3 test).

---

### Task 1: Read the series id and the cancelled flag, and give a plan row its state

**Files:**
- Modify: `tools/factory-server/src/model.rs` (`Series.id`, `Job.cancelled`)
- Modify: `tools/factory-server/src/state.rs` (test helper `fn job`, add `cancelled: None`)
- Create: `tools/factory-server/src/specs.rs` (types, `plan_state`, `card_stage`, tests)
- Modify: `tools/factory-server/src/main.rs` (add `mod specs;`)

**Interfaces:**
- Consumes: nothing.
- Produces: `Series.id: Option<String>`, `Job.cancelled: Option<bool>`, and in `specs.rs`: `SpecPlanRow { job_id: String, index: u32, title: Option<String>, state: String }`, `SpecCard { series_id: String, title: Option<String>, goal: Option<String>, stage: String, plans: Vec<SpecPlanRow> }`, `plan_state(job: &Job) -> &'static str`, `card_stage(rows: &[SpecPlanRow]) -> &'static str`.

- [ ] **Step 1: Write the failing tests**

Create `src/specs.rs` with only the tests (the types and functions come in step 3):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn job_json(stage: &str, extra: serde_json::Value) -> serde_json::Value {
        let mut value = serde_json::json!({
            "id": "001", "dir": "d", "slug": "s", "created": "2026-10-13", "feature": "f",
            "branch": "b", "stage": stage, "round": 0, "stage_since": 0.0, "reviews": {},
        });
        if let (Some(base), Some(more)) = (value.as_object_mut(), extra.as_object()) {
            base.extend(more.clone());
        }
        value
    }

    fn job(stage: &str, cancelled: bool) -> Job {
        serde_json::from_value(job_json(stage, serde_json::json!({ "cancelled": cancelled }))).expect("job")
    }

    fn row(state: &str) -> SpecPlanRow {
        SpecPlanRow { job_id: "001".into(), index: 1, title: None, state: state.into() }
    }

    #[test]
    fn a_plan_row_state_follows_the_stage_and_the_cancelled_flag() {
        assert_eq!(plan_state(&job("spec", false)), "waiting");
        assert_eq!(plan_state(&job("build", false)), "building");
        assert_eq!(plan_state(&job("review", false)), "review");
        assert_eq!(plan_state(&job("approve", false)), "review");
        assert_eq!(plan_state(&job("needs-human", false)), "needs-human");
        assert_eq!(plan_state(&job("merged", false)), "merged");
        assert_eq!(plan_state(&job("needs-human", true)), "cancelled");
        assert_eq!(plan_state(&job("surprise", false)), "waiting");
    }

    #[test]
    fn the_card_stage_is_merged_when_all_merged_needs_human_when_stuck_and_spec_otherwise() {
        assert_eq!(card_stage(&[row("merged"), row("merged")]), "merged");
        assert_eq!(card_stage(&[row("merged"), row("building"), row("waiting")]), "spec");
        assert_eq!(card_stage(&[row("merged"), row("needs-human"), row("waiting")]), "needs-human");
        assert_eq!(card_stage(&[row("building"), row("cancelled")]), "needs-human");
        assert_eq!(card_stage(&[row("waiting"), row("waiting")]), "spec");
    }

    #[test]
    fn the_series_id_and_cancelled_flag_are_read() {
        let extra = serde_json::json!({ "series": { "index": 2, "total": 5, "id": "S2" }, "cancelled": true });
        let parsed: Job = serde_json::from_value(job_json("build", extra)).expect("job");
        assert_eq!(parsed.series.and_then(|s| s.id), Some("S2".to_string()));
        assert_eq!(parsed.cancelled, Some(true));
    }

    #[test]
    fn bad_series_id_and_cancelled_values_are_dropped_not_fatal() {
        let extra = serde_json::json!({ "series": { "index": 2, "total": 5, "id": 7 }, "cancelled": "yes" });
        let parsed: Job = serde_json::from_value(job_json("build", extra)).expect("job still parses");
        let series = parsed.series.expect("the series stays");
        assert_eq!((series.index, series.total, series.id), (2, 5, None));
        assert_eq!(parsed.cancelled, None);
    }

    #[test]
    fn an_old_job_has_no_series_id_and_no_cancelled_flag() {
        let extra = serde_json::json!({ "series": { "plan_file": "p.md", "index": 1, "total": 3 } });
        let parsed: Job = serde_json::from_value(job_json("merged", extra)).expect("job");
        assert_eq!(parsed.series.and_then(|s| s.id), None);
        assert_eq!(parsed.cancelled, None);
    }
}
```

In `src/main.rs`, add `mod specs;` next to the other `mod` lines.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml specs`
Expected: compile errors, `plan_state`, `card_stage`, `SpecPlanRow` not found and no field `cancelled`.

- [ ] **Step 3: Write the implementation**

In `src/model.rs`, in `struct Series`, replace the `id` field with:

```rust
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
```

In `struct Job`, add after `budget_weighted`:

```rust
    /// True once the user cancelled the job. The driver writes it, a bad value is dropped.
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub cancelled: Option<bool>,
```

In `src/state.rs`, in the test helper `fn job(...)`, add `cancelled: None,` after `budget_weighted: None,`.

At the top of `src/specs.rs` (above the tests module), add:

```rust
//! Spec cards: one card per series, built from the plan jobs that share a series id.

use serde::Serialize;

use crate::model::Job;

/// One plan of a series, as a row of the spec card.
#[derive(Debug, Serialize, PartialEq)]
pub struct SpecPlanRow {
    pub job_id: String,
    /// 1-based position in the series.
    pub index: u32,
    pub title: Option<String>,
    /// "waiting", "building", "review", "needs-human", "merged" or "cancelled".
    pub state: String,
}

/// The card of one spec and the plans that belong to it.
#[derive(Debug, Serialize, PartialEq)]
pub struct SpecCard {
    pub series_id: String,
    pub title: Option<String>,
    pub goal: Option<String>,
    /// The column the card sits in: "spec", "needs-human" or "merged".
    pub stage: String,
    pub plans: Vec<SpecPlanRow>,
}

/// The row state of one plan job. A cancelled flag wins over the stage.
pub fn plan_state(job: &Job) -> &'static str {
    if job.cancelled == Some(true) {
        return "cancelled";
    }
    match job.stage.as_str() {
        "merged" => "merged",
        "needs-human" => "needs-human",
        "review" | "approve" => "review",
        "build" => "building",
        _ => "waiting",
    }
}

/// Merged once every plan merged, needs-human when a plan is stuck or cancelled, spec otherwise.
pub fn card_stage(rows: &[SpecPlanRow]) -> &'static str {
    if !rows.is_empty() && rows.iter().all(|row| row.state == "merged") {
        "merged"
    } else if rows.iter().any(|row| row.state == "cancelled" || row.state == "needs-human") {
        "needs-human"
    } else {
        "spec"
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml`
Expected: PASS. If `cargo` reports an unused-code warning for the new types, that is fine until Task 2.

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Paths: `model.rs`, `state.rs`, `specs.rs`, `main.rs` under `tools/factory-server/src`. Suggested message: `feat(factory): read series ids and plan row states`.

---

### Task 2: Build the spec cards

**Files:**
- Modify: `tools/factory-server/src/plan.rs` (add `SpecInfo`, `load_spec`, a test)
- Modify: `tools/factory-server/src/specs.rs` (add `build_spec_cards`, tests)

**Interfaces:**
- Consumes: `JobView` from `state.rs` (fields `job: Job`, `plan: Option<PlanView>`), `plan_state`, `card_stage`, `SpecPlanRow`, `SpecCard` (Task 1).
- Produces: `plan::SpecInfo { title: Option<String>, goal: Option<String> }`, `plan::load_spec(root: &Path, dir: &str) -> Option<SpecInfo>`, and `specs::build_spec_cards(jobs: &[JobView], root: &Path) -> Vec<SpecCard>`.

- [ ] **Step 1: Write the failing tests**

In `plan.rs`, add inside `mod tests`:

```rust
    #[test]
    fn load_spec_reads_title_and_goal_and_refuses_unsafe_folders() -> TestResult {
        let root = tempfile::tempdir()?;
        let dir = root.path().join(".superpowers/sdd/2026-10-13-001-x");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("spec.md"), "# Spec: Demo spec\n\n## Goal\n\nShow the plans.\n")?;
        let info = load_spec(root.path(), "2026-10-13-001-x").ok_or("no spec")?;
        assert_eq!(info.title.as_deref(), Some("Demo spec"));
        assert_eq!(info.goal.as_deref(), Some("Show the plans."));
        assert!(load_spec(root.path(), "../x").is_none());
        assert!(load_spec(root.path(), "missing-folder").is_none());
        Ok(())
    }
```

In `specs.rs`, add these to the existing `mod tests` (they reuse `job_json`):

```rust
    use crate::plan::PlanView;
    use crate::state::JobView;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn view(id: &str, stage: &str, index: u32, total: u32, series_id: Option<&str>, title: Option<&str>) -> JobView {
        let extra = serde_json::json!({
            "id": id,
            "dir": format!("2026-10-13-{id}-x"),
            "series": { "index": index, "total": total, "id": series_id },
        });
        JobView {
            job: serde_json::from_value(job_json(stage, extra)).expect("job"),
            agents: Vec::new(),
            seconds_in_stage: 0.0,
            plan: title.map(|t| PlanView { title: Some(t.to_string()), goal: None, tasks: Vec::new() }),
            stage_durations: Vec::new(),
            token_view: None,
        }
    }

    #[test]
    fn one_card_per_series_with_plans_in_index_order() {
        let jobs = vec![
            view("003", "spec", 3, 3, Some("S2"), Some("Third")),
            view("001", "merged", 1, 3, Some("S2"), Some("First")),
            view("002", "build", 2, 3, Some("S2"), Some("Second")),
            view("004", "build", 1, 2, Some("S10"), None),
            view("005", "spec", 2, 2, Some("S10"), None),
        ];
        let cards = build_spec_cards(&jobs, std::path::Path::new("/nonexistent"));
        let ids: Vec<_> = cards.iter().map(|c| c.series_id.as_str()).collect();
        assert_eq!(ids, vec!["S2", "S10"], "groups keep first-appearance order");
        let rows: Vec<_> = cards[0].plans.iter().map(|r| (r.index, r.state.as_str(), r.title.as_deref())).collect();
        assert_eq!(rows, vec![(1, "merged", Some("First")), (2, "building", Some("Second")), (3, "waiting", Some("Third"))]);
        assert_eq!(cards[0].stage, "spec");
        assert_eq!(cards[1].plans.len(), 2);
    }

    #[test]
    fn jobs_without_a_series_id_or_in_a_one_plan_series_make_no_card() {
        let jobs = vec![
            view("001", "merged", 1, 3, None, None),
            view("002", "build", 1, 1, Some("S1"), None),
        ];
        assert!(build_spec_cards(&jobs, std::path::Path::new("/nonexistent")).is_empty());
    }

    #[test]
    fn the_title_and_goal_come_from_spec_md_of_plan_one() -> TestResult {
        let root = tempfile::tempdir()?;
        let dir = root.path().join(".superpowers/sdd/2026-10-13-001-x");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("spec.md"), "# Spec: Demo spec\n\n## Goal\n\nShow the plans.\n")?;
        let jobs = vec![view("001", "build", 1, 2, Some("S1"), None), view("002", "spec", 2, 2, Some("S1"), None)];
        let cards = build_spec_cards(&jobs, root.path());
        assert_eq!(cards[0].title.as_deref(), Some("Demo spec"));
        assert_eq!(cards[0].goal.as_deref(), Some("Show the plans."));
        Ok(())
    }

    #[test]
    fn a_series_without_its_plan_one_job_still_gets_a_card_without_a_title() {
        let jobs = vec![view("002", "build", 2, 3, Some("S1"), None), view("003", "spec", 3, 3, Some("S1"), None)];
        let cards = build_spec_cards(&jobs, std::path::Path::new("/nonexistent"));
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].title, None);
        assert_eq!(cards[0].plans.len(), 2);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml load_spec specs`
Expected: compile errors, `load_spec` and `build_spec_cards` not found.

- [ ] **Step 3: Write the implementation**

In `plan.rs`, add after `load_plan` (above `#[cfg(test)]`):

```rust
/// What a spec file says about itself.
#[derive(Debug, Clone, PartialEq)]
pub struct SpecInfo {
    pub title: Option<String>,
    pub goal: Option<String>,
}

/// Reads `spec.md` of one job folder. None when the folder name is unsafe or the file is missing.
pub fn load_spec(root: &Path, dir: &str) -> Option<SpecInfo> {
    if !safe_dir_name(dir) {
        return None;
    }
    let path = root.join(".superpowers").join("sdd").join(dir).join("spec.md");
    let text = std::fs::read_to_string(path).ok()?;
    Some(SpecInfo { title: first_heading(&text, "Spec:"), goal: goal(&text) })
}
```

In `specs.rs`, change the imports at the top to:

```rust
use std::path::Path;

use serde::Serialize;

use crate::model::Job;
use crate::plan::load_spec;
use crate::state::JobView;
```

and add after `card_stage`:

```rust
fn index_of(view: &JobView) -> u32 {
    view.job.series.as_ref().map_or(0, |series| series.index)
}

/// Builds one card per series id. Only a series of two or more plans gets a card, and jobs without a
/// series id (made before plans got series ids) are left out. The title and goal come from the spec
/// copy in plan 1's job folder.
pub fn build_spec_cards(jobs: &[JobView], root: &Path) -> Vec<SpecCard> {
    let mut groups: Vec<(String, Vec<&JobView>)> = Vec::new();
    for view in jobs {
        let Some(series) = view.job.series.as_ref() else { continue };
        let Some(id) = series.id.as_ref() else { continue };
        if series.total < 2 {
            continue;
        }
        match groups.iter_mut().find(|(group_id, _)| group_id == id) {
            Some((_, members)) => members.push(view),
            None => groups.push((id.clone(), vec![view])),
        }
    }
    groups
        .into_iter()
        .map(|(series_id, mut members)| {
            members.sort_by_key(|view| index_of(view));
            let spec = members
                .iter()
                .find(|view| index_of(view) == 1)
                .and_then(|view| load_spec(root, &view.job.dir));
            let plans: Vec<SpecPlanRow> = members
                .iter()
                .map(|view| SpecPlanRow {
                    job_id: view.job.id.clone(),
                    index: index_of(view),
                    title: view.plan.as_ref().and_then(|plan| plan.title.clone()),
                    state: plan_state(&view.job).to_string(),
                })
                .collect();
            let stage = card_stage(&plans).to_string();
            SpecCard {
                series_id,
                title: spec.as_ref().and_then(|info| info.title.clone()),
                goal: spec.and_then(|info| info.goal),
                stage,
                plans,
            }
        })
        .collect()
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml`
Expected: PASS.

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Paths: `plan.rs`, `specs.rs`. Suggested message: `feat(factory): build spec cards from the series jobs`.

---

### Task 3: Put the cards into `/api/state` and document them

**Files:**
- Modify: `tools/factory-server/src/state.rs` (`StateView`, `build_state`, tests)
- Modify: `tools/factory-server/README.md` (what it reads, endpoint table)
- Modify: `.claude/software-factory.md` (dashboard section, after the line that starts `- The card shows the job id`)

**Interfaces:**
- Consumes: `build_spec_cards(&[JobView], &Path) -> Vec<SpecCard>` (Task 2).
- Produces: `StateView.specs: Vec<SpecCard>`, serialized as `specs` in `/api/state`, each item `{series_id, title, goal, stage, plans[{job_id, index, title, state}]}`. Plan 03 reads exactly these names.

- [ ] **Step 1: Write the failing tests**

In `state.rs`, change the existing test `the_state_has_no_recent_activity_feed` by adding one line before `Ok(())`:

```rust
        assert_eq!(json.get("specs").and_then(|v| v.as_array()).map(Vec::len), Some(0), "specs is an empty list");
```

Add a new test in `state.rs` `mod tests`:

```rust
    fn series_job(id: &str, stage: &str, index: u32, series_id: Option<&str>) -> serde_json::Value {
        serde_json::json!({
            "id": id, "dir": format!("2026-10-13-{id}-x"), "slug": "x", "created": "2026-10-13",
            "feature": "f", "branch": format!("factory/{id}-x"), "stage": stage, "round": 0,
            "stage_since": 0.0, "reviews": {},
            "series": { "plan_file": "p.md", "index": index, "total": 2, "id": series_id },
        })
    }

    #[test]
    fn the_state_has_one_spec_card_per_series_and_none_for_old_jobs() -> TestResult {
        let root = tempfile::tempdir()?;
        let dir = root.path().join(".superpowers/sdd/2026-10-13-001-x");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("spec.md"), "# Spec: Demo spec\n\n## Goal\n\nShow the plans.\n")?;
        let board: Board = serde_json::from_value(serde_json::json!({ "jobs": [
            series_job("001", "build", 1, Some("S1")),
            series_job("002", "spec", 2, Some("S1")),
            series_job("003", "merged", 1, None),
        ]}))?;
        let view = build_state(&board, &Registry::new(), 10.0, None, root.path());
        assert_eq!(view.specs.len(), 1);
        assert_eq!(view.specs[0].series_id, "S1");
        assert_eq!(view.specs[0].title.as_deref(), Some("Demo spec"));
        let states: Vec<_> = view.specs[0].plans.iter().map(|r| r.state.as_str()).collect();
        assert_eq!(states, vec!["building", "waiting"]);
        let json = serde_json::to_value(&view)?;
        assert_eq!(json["specs"][0]["plans"][1]["job_id"], "002");
        assert_eq!(json["jobs"].as_array().map(Vec::len), Some(3), "plan jobs stay in jobs[]");
        Ok(())
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml state`
Expected: compile error, no field `specs` on `StateView`.

- [ ] **Step 3: Write the implementation**

In `state.rs`, add the import `use crate::specs::{build_spec_cards, SpecCard};`, add the field to `StateView` after `jobs`:

```rust
    pub specs: Vec<SpecCard>,
```

and in `build_state`, before the final `StateView { ... }`, add `let specs = build_spec_cards(&jobs, root);` and put `specs,` after `jobs,` in the struct literal.

In `README.md`, in the "What it reads" bullet for `board.json`, add `series.id` and `cancelled` to the optional fields. In the endpoint table row for `/api/state`, add `specs[]` (`series_id`, `title`, `goal`, `stage`, `plans[]`) after `jobs[]`.

In `.claude/software-factory.md`, after the line that starts `- The card shows the job id`, add:

```markdown
- A spec card is built for each series id shared by two or more plan jobs (`specs[]` in `/api/state`). It holds the spec title and goal from `spec.md` in plan 1's job folder, and one row per plan with its state (waiting, building, review, needs-human, merged, cancelled). The card sits in the spec column while any plan is open, in needs-human when a plan is stuck or cancelled, and in merged once every plan merged. Jobs without a series id get no card.
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory-server/Cargo.toml`
Expected: PASS.

Run: `cargo clippy -j4 --manifest-path tools/factory-server/Cargo.toml --tests`
Expected: no warnings.

Run: `cargo fmt --manifest-path tools/factory-server/Cargo.toml -- --check`
Expected: no diff. If it reports one, run `cargo fmt --manifest-path tools/factory-server/Cargo.toml` and review the diff.

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Paths: `state.rs`, `README.md`, `.claude/software-factory.md`. Suggested message: `feat(factory): serve spec cards in the dashboard state`.
