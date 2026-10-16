# Factory Cost Report, Plan 01: the cost model and the history gatherer

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pure cost types and statistics in `factory-core`, a numstat helper in `factory-git`, and a `gather_costs` function in `factory-engine` that builds one cost record per board job from the job folder and git.

**Architecture:** `factory-core` gets a `cost` module (size groups, a `usage.jsonl` summarizer, group statistics, the estimate and the `costs.json` shape) and two config keys. `factory-git` gets `diff_counts`. `factory-engine` gets a `costs` module whose `gather_costs(driver)` is the one place that reads the board, `plan.md`, `usage.jsonl` and git. Plans 02 and 03 use these from the CLI, the merge step, the server and the dashboard.

**Tech Stack:** Rust 2021, serde, serde_json, git2 (through `factory-git`), tempfile (tests). Workspace at `tools/factory/`.

**Spec:** `docs/superpowers/specs/2026-10-16-factory-cost-report-design.md` (Decisions, Design 1 and 2, Testing).

## Global Constraints

- Commands run from the repo root as `cargo check -j4 --manifest-path tools/factory/Cargo.toml` and `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p <crate> <test_name>`. One crate per call. Never `--workspace`, never omit `-j4`, never set `CARGO_TARGET_DIR`.
- No panicking unwrap calls in production code (a hook blocks that text in Rust edits; `unwrap_or`, `unwrap_or_default` and `map_or` are fine). `.expect("...")` only inside tests.
- Comments are short full sentences ending in a period. No emojis.
- The measure is weighted tokens (`factory_core::weighted_tokens`). No dollar amounts. No LLM calls.
- Size groups: `small` is `plan_bytes < size_small_max_bytes`, `medium` is `size_small_max_bytes <= plan_bytes <= size_medium_max_bytes`, `complex` is above. Defaults 12288 and 25600.
- Only merged jobs with data (`runs > 0` and a plan) feed the group stats. Median of an even count is the mean of the two middle values (integer division). A group with fewer than 3 jobs is "few jobs".
- Board JSON stays compatible with `tools/factory-server`: the new `Job.estimate` is optional and skipped when empty.
- Commit steps are for a human running the plan by hand: pathspec commit only (`git add <paths> && git commit --only -m "..." -- <paths>`), message through the skill `dev-workflow-skills:1-git-commit`, never `git add -A` or `git add .`.

## Review Focus

1. Threshold edges: a plan of exactly `size_small_max_bytes - 1`, `size_small_max_bytes`, `size_medium_max_bytes` and `size_medium_max_bytes + 1` bytes land in the right group. Pinned in Task 1.
2. A `usage.jsonl` with garbage lines, lines of other jobs, old lines without `usage_v` and new lines with it: only this job's lines count, old lines set `approx`. Pinned in Task 1.
3. Empty and one-job groups: no division by zero, `few jobs` is derivable from `n`, and `corrected` is absent when every job is approximate. Pinned in Task 2.
4. A job with a missing `plan.md`, a missing `usage.jsonl` or commits that no longer exist must give a record (with `group`, `lines` or `runs` empty), never an error. Pinned in Task 3.
5. Even-count median, huge token counts (no overflow), and role shares that add to 100 within rounding. Pinned in Task 2.

## File Structure

```
tools/factory/crates/factory-core/src/config.rs     size_small_max_bytes, size_medium_max_bytes
tools/factory/crates/factory-core/src/cost.rs       SizeGroup, usage_summary, JobCost, GroupStats, Estimate, stats, estimate_for, CostsFile
tools/factory/crates/factory-core/src/board.rs      Job.estimate
tools/factory/crates/factory-core/src/lib.rs        module and re-exports
tools/factory/crates/factory-git/src/diff.rs        DiffCounts, Repo::diff_counts
tools/factory/crates/factory-git/src/lib.rs         re-export
tools/factory/crates/factory-engine/src/costs.rs    gather_costs
tools/factory/crates/factory-engine/src/lib.rs      mod costs and re-export
```

---

### Task 1: Config keys, size groups, usage summary and the job estimate field

**Files:**
- Modify: `tools/factory/crates/factory-core/src/config.rs`
- Create: `tools/factory/crates/factory-core/src/cost.rs`
- Modify: `tools/factory/crates/factory-core/src/board.rs`
- Modify: `tools/factory/crates/factory-core/src/lib.rs`

**Interfaces:**
- Produces:
  - `Config.size_small_max_bytes: u64` (12288) and `Config.size_medium_max_bytes: u64` (25600). `Config::parse` errors when either is 0 or `small >= medium`.
  - `pub enum SizeGroup { Small, Medium, Complex }` with `as_str(self) -> &'static str` (`small`, `medium`, `complex`) and `pub const ALL: [SizeGroup; 3]`.
  - `pub fn group_of(plan_bytes: u64, cfg: &Config) -> SizeGroup`.
  - `pub fn count_tasks(plan: &str) -> u32`: lines starting with `### Task ` followed by a digit, outside fenced code.
  - `pub struct UsageSummary { pub weighted: u64, pub raw: u64, pub runs: u32, pub approx: bool, pub roles: BTreeMap<String, u64> }` (derives `Default`) and `pub fn usage_summary(text: &str, job: &str) -> UsageSummary`.
  - `pub struct Estimate { pub group: String, pub median: u64, pub low: u64, pub high: u64, pub n: u32, pub n_approx: u32 }` (serde, `Clone`, `PartialEq`, `Debug`).
  - `Job.estimate: Option<Estimate>`.

- [ ] **Step 1: Write the failing tests**

Create `tools/factory/crates/factory-core/src/cost.rs` containing only this tests module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn group_edges_follow_the_thresholds() {
        let cfg = Config::default();
        assert_eq!(group_of(0, &cfg), SizeGroup::Small);
        assert_eq!(group_of(12_287, &cfg), SizeGroup::Small);
        assert_eq!(group_of(12_288, &cfg), SizeGroup::Medium);
        assert_eq!(group_of(25_600, &cfg), SizeGroup::Medium);
        assert_eq!(group_of(25_601, &cfg), SizeGroup::Complex);
        assert_eq!(group_of(u64::MAX, &cfg), SizeGroup::Complex);
    }

    #[test]
    fn group_names_are_stable() {
        let names: Vec<&str> = SizeGroup::ALL.iter().map(|g| g.as_str()).collect();
        assert_eq!(names, vec!["small", "medium", "complex"]);
    }

    #[test]
    fn count_tasks_counts_headings_outside_code_fences() {
        let plan = "# Plan\n\n### Task 1: One\n\ntext\n\n```markdown\n### Task 9: quoted\n```\n\n### Task 2: Two\n### Task Z: not a task\n";
        assert_eq!(count_tasks(plan), 2);
        assert_eq!(count_tasks(""), 0);
    }

    #[test]
    fn usage_summary_sums_this_jobs_lines_and_weights_cache_reads() {
        let text = concat!(
            "{\"job\":\"001\",\"role\":\"builder\",\"input\":100,\"cache_creation\":200,\"output\":300,\"cache_read\":1000,\"usage_v\":2}\n",
            "{\"job\":\"002\",\"role\":\"builder\",\"input\":9,\"cache_creation\":9,\"output\":9,\"cache_read\":9,\"usage_v\":2}\n",
            "garbage line\n",
            "{\"job\":\"001\",\"role\":\"reviewer\",\"input\":10,\"cache_creation\":0,\"output\":0,\"cache_read\":0,\"usage_v\":2}\n",
        );
        let s = usage_summary(text, "001");
        assert_eq!(s.runs, 2);
        assert_eq!(s.weighted, (100 + 200 + 300 + 100) + 10);
        assert_eq!(s.raw, (100 + 200 + 300 + 1000) + 10);
        assert!(!s.approx);
        assert_eq!(s.roles.get("builder"), Some(&700));
        assert_eq!(s.roles.get("reviewer"), Some(&10));
    }

    #[test]
    fn a_line_without_usage_v_makes_the_summary_approximate() {
        let text = "{\"job\":\"001\",\"role\":\"builder\",\"input\":1,\"cache_creation\":1,\"output\":1,\"cache_read\":10}\n\
                    {\"job\":\"001\",\"role\":\"builder\",\"input\":1,\"cache_creation\":1,\"output\":1,\"cache_read\":10,\"usage_v\":2}\n";
        assert!(usage_summary(text, "001").approx);
    }

    #[test]
    fn usage_summary_of_nothing_is_empty_and_not_approximate() {
        let s = usage_summary("", "001");
        assert_eq!(s.runs, 0);
        assert_eq!(s.weighted, 0);
        assert!(!s.approx);
    }

    #[test]
    fn huge_counts_do_not_overflow() {
        let text = format!(
            "{{\"job\":\"001\",\"role\":\"builder\",\"input\":{m},\"cache_creation\":{m},\"output\":{m},\"cache_read\":{m},\"usage_v\":2}}\n",
            m = u64::MAX
        );
        let s = usage_summary(&text, "001");
        assert_eq!(s.raw, u64::MAX);
        assert_eq!(s.weighted, u64::MAX);
    }
}
```

In `config.rs`, find the tests module (`#[cfg(test)] mod tests`) and add:

```rust
    #[test]
    fn size_thresholds_default_and_validate() {
        let cfg = Config::default();
        assert_eq!(cfg.size_small_max_bytes, 12_288);
        assert_eq!(cfg.size_medium_max_bytes, 25_600);
        assert!(Config::parse("size_small_max_bytes = 100\nsize_medium_max_bytes = 200\n").is_ok());
        assert!(Config::parse("size_small_max_bytes = 200\nsize_medium_max_bytes = 200\n").is_err());
        assert!(Config::parse("size_small_max_bytes = 300\nsize_medium_max_bytes = 200\n").is_err());
        assert!(Config::parse("size_small_max_bytes = 0\n").is_err());
        assert!(Config::parse("size_smal_max_bytes = 1\n").is_err());
    }
```

In `board.rs` tests module add:

```rust
    #[test]
    fn the_estimate_field_is_optional_and_round_trips() {
        let mut job = Job::new("001", "d", "s", "2026-10-16", "f", 1.0);
        let json = serde_json::to_string(&job).expect("json");
        assert!(!json.contains("estimate"), "{json}");
        job.estimate = Some(crate::cost::Estimate { group: "small".into(), median: 250_000, low: 220_000, high: 310_000, n: 4, n_approx: 4 });
        let back: Job = serde_json::from_str(&serde_json::to_string(&job).expect("json")).expect("load");
        assert_eq!(back.estimate.as_ref().map(|e| e.median), Some(250_000));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-core cost`
Expected: compile errors (`SizeGroup`, `group_of`, `usage_summary` and the config fields not found).

- [ ] **Step 3: Implement**

`config.rs`: add to the `Config` struct (after `kb_auto_commit`):

```rust
    /// A plan under this many bytes is a small job.
    pub size_small_max_bytes: u64,
    /// A plan up to this many bytes is a medium job, a bigger one is complex.
    pub size_medium_max_bytes: u64,
```

in `Default` (after `kb_auto_commit: false,`): `size_small_max_bytes: 12_288, size_medium_max_bytes: 25_600,`, and in `Config::parse` after the model-name check:

```rust
        if cfg.size_small_max_bytes == 0 || cfg.size_small_max_bytes >= cfg.size_medium_max_bytes {
            return Err(CoreError::Config("size_small_max_bytes must be above zero and below size_medium_max_bytes".into()));
        }
```

`cost.rs` (above the tests):

```rust
//! Job cost records, group statistics and the estimate for a new job.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::budget::weighted_tokens;
use crate::config::Config;

/// How big a job is, by the size of its plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SizeGroup {
    Small,
    Medium,
    Complex,
}

impl SizeGroup {
    pub const ALL: [SizeGroup; 3] = [SizeGroup::Small, SizeGroup::Medium, SizeGroup::Complex];

    pub fn as_str(self) -> &'static str {
        match self {
            SizeGroup::Small => "small",
            SizeGroup::Medium => "medium",
            SizeGroup::Complex => "complex",
        }
    }
}

/// The group of a plan of `plan_bytes` bytes.
pub fn group_of(plan_bytes: u64, cfg: &Config) -> SizeGroup {
    if plan_bytes < cfg.size_small_max_bytes {
        SizeGroup::Small
    } else if plan_bytes <= cfg.size_medium_max_bytes {
        SizeGroup::Medium
    } else {
        SizeGroup::Complex
    }
}

/// The number of `### Task N` headings outside fenced code.
pub fn count_tasks(plan: &str) -> u32 {
    let mut in_fence = false;
    let mut count = 0u32;
    for line in plan.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(rest) = line.strip_prefix("### Task ") {
            if rest.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                count = count.saturating_add(1);
            }
        }
    }
    count
}

/// What `usage.jsonl` says about one job.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageSummary {
    pub weighted: u64,
    pub raw: u64,
    pub runs: u32,
    /// True when any line predates the corrected output count (`usage_v` below 2).
    pub approx: bool,
    /// Weighted tokens per agent role.
    pub roles: BTreeMap<String, u64>,
}

/// Sums the lines of `job`. Lines of other jobs and lines that are not JSON are skipped.
pub fn usage_summary(text: &str, job: &str) -> UsageSummary {
    let mut s = UsageSummary::default();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if v["job"] != job {
            continue;
        }
        let n = |k: &str| v[k].as_u64().unwrap_or(0);
        let (input, cc, out, cr) = (n("input"), n("cache_creation"), n("output"), n("cache_read"));
        let weighted = weighted_tokens(input, cc, out, cr);
        let raw = input.saturating_add(cc).saturating_add(out).saturating_add(cr);
        s.weighted = s.weighted.saturating_add(weighted);
        s.raw = s.raw.saturating_add(raw);
        s.runs = s.runs.saturating_add(1);
        if n("usage_v") < 2 {
            s.approx = true;
        }
        let role = v["role"].as_str().unwrap_or("unknown").to_string();
        let slot = s.roles.entry(role).or_insert(0);
        *slot = slot.saturating_add(weighted);
    }
    s
}

/// The estimated cost of a new job, from the history at the time it was made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Estimate {
    /// `small`, `medium`, `complex`, or `all` when the group had no jobs.
    pub group: String,
    pub median: u64,
    pub low: u64,
    pub high: u64,
    pub n: u32,
    pub n_approx: u32,
}
```

(`weighted_tokens` must saturate; if it does not, wrap the arguments with `saturating_add` first. Open `budget.rs` to check: `weighted_tokens(input, cc, out, cr)` is documented as saturating in `factory-core/src/board.rs:33` and `budget.rs:9`.)

`board.rs`: add to `Job` after `park_at`:

```rust
    /// The cost estimate made when the job was created. New in the cost report series.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimate: Option<crate::cost::Estimate>,
```

and `estimate: None,` in `Job::new`.

`lib.rs`: add `pub mod cost;` (after `pub mod config;`) and `pub use cost::{count_tasks, group_of, usage_summary, Estimate, SizeGroup, UsageSummary};`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-core`
Expected: all pass. Then `cargo check -j4 --manifest-path tools/factory/Cargo.toml` (the other crates build `Job` only through `Job::new`, so they must still compile).

- [ ] **Step 5: Commit**

Commit `config.rs`, `cost.rs`, `board.rs` and `lib.rs` with the skill message `feat(factory): add size groups and the usage summary`.

---

### Task 2: Group statistics, the estimate and the costs.json shape

**Files:**
- Modify: `tools/factory/crates/factory-core/src/cost.rs`
- Modify: `tools/factory/crates/factory-core/src/lib.rs`

**Interfaces:**
- Consumes: Task 1 types.
- Produces:
  - `pub struct JobCost { pub id: String, pub slug: String, pub stage: String, pub group: Option<SizeGroup>, pub plan_bytes: u64, pub tasks: u32, pub lines: Option<u64>, pub files: Option<u64>, pub weighted: u64, pub raw: u64, pub runs: u32, pub approx: bool, pub roles: BTreeMap<String, u64> }` (`Debug, Clone, PartialEq`) with `has_data(&self) -> bool` (`runs > 0 && group.is_some()`).
  - `pub struct Spread { pub n: u32, pub median: u64, pub low: u64, pub high: u64 }` and `pub struct GroupStats { pub n: u32, pub n_approx: u32, pub median: u64, pub low: u64, pub high: u64, pub roles: BTreeMap<String, u32>, pub corrected: Option<Spread> }` (both serde).
  - `pub fn stats(jobs: &[JobCost]) -> BTreeMap<String, GroupStats>`: keys `small`, `medium`, `complex` (only groups with jobs) and `all` (when any job).
  - `pub fn estimate_for(plan_bytes: u64, stats: &BTreeMap<String, GroupStats>, cfg: &Config) -> Option<Estimate>`.
  - `pub struct CostsFile { pub generated_at: f64, pub thresholds: Thresholds, pub groups: BTreeMap<String, GroupStats>, pub roles: BTreeMap<String, u32> }`, `pub struct Thresholds { pub small_max_bytes: u64, pub medium_max_bytes: u64 }`, `CostsFile::build(jobs: &[JobCost], cfg: &Config, now: f64) -> CostsFile`, `CostsFile::write(&self, path: &Path) -> Result<(), CoreError>`.

- [ ] **Step 1: Write the failing tests**

Add to the tests module in `cost.rs`:

```rust
    fn job(id: &str, group: SizeGroup, stage: &str, weighted: u64, approx: bool) -> JobCost {
        let mut roles = BTreeMap::new();
        roles.insert("builder".to_string(), weighted * 7 / 10);
        roles.insert("reviewer".to_string(), weighted - weighted * 7 / 10);
        JobCost {
            id: id.into(),
            slug: "s".into(),
            stage: stage.into(),
            group: Some(group),
            plan_bytes: 1000,
            tasks: 2,
            lines: Some(100),
            files: Some(3),
            weighted,
            raw: weighted * 4,
            runs: 3,
            approx,
            roles,
        }
    }

    #[test]
    fn stats_median_low_high_for_odd_and_even_counts() {
        let odd = [
            job("1", SizeGroup::Small, "merged", 300, false),
            job("2", SizeGroup::Small, "merged", 100, false),
            job("3", SizeGroup::Small, "merged", 200, false),
        ];
        let s = stats(&odd);
        let small = &s["small"];
        assert_eq!((small.n, small.median, small.low, small.high), (3, 200, 100, 300));
        let even = [job("1", SizeGroup::Medium, "merged", 100, false), job("2", SizeGroup::Medium, "merged", 201, false)];
        let m = &stats(&even)["medium"];
        assert_eq!((m.n, m.median, m.low, m.high), (2, 150, 100, 201));
    }

    #[test]
    fn only_merged_jobs_with_data_feed_the_stats() {
        let mut no_data = job("3", SizeGroup::Small, "merged", 999, false);
        no_data.runs = 0;
        let mut no_group = job("4", SizeGroup::Small, "merged", 999, false);
        no_group.group = None;
        let jobs = [
            job("1", SizeGroup::Small, "merged", 100, false),
            job("2", SizeGroup::Small, "needs-human", 900, false),
            no_data,
            no_group,
        ];
        let s = stats(&jobs);
        assert_eq!(s["small"].n, 1);
        assert_eq!(s["all"].n, 1);
        assert!(!s.contains_key("medium"));
    }

    #[test]
    fn approximate_jobs_are_counted_and_corrected_figures_exclude_them() {
        let jobs = [
            job("1", SizeGroup::Small, "merged", 100, true),
            job("2", SizeGroup::Small, "merged", 300, false),
            job("3", SizeGroup::Small, "merged", 500, false),
        ];
        let small = &stats(&jobs)["small"];
        assert_eq!((small.n, small.n_approx), (3, 1));
        let c = small.corrected.as_ref().expect("corrected");
        assert_eq!((c.n, c.median, c.low, c.high), (2, 400, 300, 500));
        let only_old = [job("1", SizeGroup::Small, "merged", 100, true)];
        assert!(stats(&only_old)["small"].corrected.is_none());
    }

    #[test]
    fn role_shares_add_up_to_one_hundred_within_rounding() {
        let jobs = [job("1", SizeGroup::Small, "merged", 1000, false), job("2", SizeGroup::Small, "merged", 1000, false)];
        let roles = &stats(&jobs)["small"].roles;
        assert_eq!(roles["builder"], 70);
        assert_eq!(roles["reviewer"], 30);
        let total: u32 = roles.values().sum();
        assert!((99..=101).contains(&total), "{total}");
    }

    #[test]
    fn empty_input_gives_no_stats_and_no_estimate() {
        let s = stats(&[]);
        assert!(s.is_empty());
        assert_eq!(estimate_for(5000, &s, &Config::default()), None);
    }

    #[test]
    fn huge_weighted_values_do_not_overflow_the_median() {
        let jobs = [
            job("1", SizeGroup::Complex, "merged", u64::MAX, false),
            job("2", SizeGroup::Complex, "merged", u64::MAX, false),
        ];
        assert_eq!(stats(&jobs)["complex"].median, u64::MAX);
    }

    #[test]
    fn the_estimate_uses_the_group_of_the_plan_and_falls_back_to_all() {
        let jobs = [job("1", SizeGroup::Small, "merged", 200, false), job("2", SizeGroup::Complex, "merged", 400, true)];
        let s = stats(&jobs);
        let cfg = Config::default();
        let small = estimate_for(5_000, &s, &cfg).expect("small");
        assert_eq!((small.group.as_str(), small.median, small.n, small.n_approx), ("small", 200, 1, 0));
        let complex = estimate_for(40_000, &s, &cfg).expect("complex");
        assert_eq!((complex.group.as_str(), complex.n_approx), ("complex", 1));
        // No medium jobs, so the overall figures are used.
        let medium = estimate_for(15_000, &s, &cfg).expect("fallback");
        assert_eq!((medium.group.as_str(), medium.n, medium.low, medium.high), ("all", 2, 200, 400));
    }

    #[test]
    fn the_costs_file_has_groups_roles_and_thresholds_and_writes_atomically() -> Result<(), Box<dyn std::error::Error>> {
        let jobs = [job("1", SizeGroup::Small, "merged", 1000, false)];
        let file = CostsFile::build(&jobs, &Config::default(), 42.0);
        assert_eq!(file.thresholds.small_max_bytes, 12_288);
        assert_eq!(file.roles["builder"], 70);
        assert!(file.groups.contains_key("small") && file.groups.contains_key("all"));
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("sub").join("costs.json");
        file.write(&path)?;
        let raw = std::fs::read_to_string(&path)?;
        assert!(raw.contains("\"generated_at\": 42.0"), "{raw}");
        assert!(!raw.contains("\"jobs\""), "the per-job list is not written");
        Ok(())
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-core cost`
Expected: compile errors (`JobCost`, `stats`, `estimate_for`, `CostsFile` not found).

- [ ] **Step 3: Implement**

Add to `cost.rs` above the tests module (add `use std::path::Path;` and `use crate::error::CoreError;` and `use crate::fsutil::write_atomic;` at the top):

```rust
/// The cost of one board job, derived from its folder and git.
#[derive(Debug, Clone, PartialEq)]
pub struct JobCost {
    pub id: String,
    pub slug: String,
    pub stage: String,
    pub group: Option<SizeGroup>,
    pub plan_bytes: u64,
    pub tasks: u32,
    pub lines: Option<u64>,
    pub files: Option<u64>,
    pub weighted: u64,
    pub raw: u64,
    pub runs: u32,
    pub approx: bool,
    pub roles: BTreeMap<String, u64>,
}

impl JobCost {
    /// True when the job ran at least once and has a plan to size it by.
    pub fn has_data(&self) -> bool {
        self.runs > 0 && self.group.is_some()
    }
}

/// Median and range over a set of jobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spread {
    pub n: u32,
    pub median: u64,
    pub low: u64,
    pub high: u64,
}

/// The figures of one size group, or of all groups together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupStats {
    pub n: u32,
    pub n_approx: u32,
    pub median: u64,
    pub low: u64,
    pub high: u64,
    /// Share of the weighted tokens per role, in whole percent.
    pub roles: BTreeMap<String, u32>,
    /// The same figures over jobs that are not approximate, or `None` when there are none.
    pub corrected: Option<Spread>,
}

fn median_of(sorted: &[u64]) -> u64 {
    let n = sorted.len();
    if n == 0 {
        return 0;
    }
    if n % 2 == 1 {
        return sorted[n / 2];
    }
    let (a, b) = (sorted[n / 2 - 1], sorted[n / 2]);
    // Half of each first keeps the sum from overflowing.
    a / 2 + b / 2 + (a % 2 + b % 2) / 2
}

fn spread(values: &mut [u64]) -> Spread {
    values.sort_unstable();
    Spread {
        n: values.len() as u32,
        median: median_of(values),
        low: values.first().copied().unwrap_or(0),
        high: values.last().copied().unwrap_or(0),
    }
}

fn group_stats(jobs: &[&JobCost]) -> GroupStats {
    let mut all: Vec<u64> = jobs.iter().map(|j| j.weighted).collect();
    let mut fixed: Vec<u64> = jobs.iter().filter(|j| !j.approx).map(|j| j.weighted).collect();
    let s = spread(&mut all);
    let corrected = if fixed.is_empty() { None } else { Some(spread(&mut fixed)) };
    let mut role_totals: BTreeMap<String, u128> = BTreeMap::new();
    let mut total: u128 = 0;
    for j in jobs {
        for (role, w) in &j.roles {
            *role_totals.entry(role.clone()).or_insert(0) += u128::from(*w);
            total += u128::from(*w);
        }
    }
    let roles = role_totals
        .into_iter()
        .map(|(role, w)| (role, if total == 0 { 0 } else { ((w * 100 + total / 2) / total) as u32 }))
        .collect();
    GroupStats {
        n: s.n,
        n_approx: jobs.iter().filter(|j| j.approx).count() as u32,
        median: s.median,
        low: s.low,
        high: s.high,
        roles,
        corrected,
    }
}

/// Statistics per size group over merged jobs with data, plus an `all` entry.
pub fn stats(jobs: &[JobCost]) -> BTreeMap<String, GroupStats> {
    let feeding: Vec<&JobCost> = jobs.iter().filter(|j| j.stage == "merged" && j.has_data()).collect();
    let mut out = BTreeMap::new();
    if feeding.is_empty() {
        return out;
    }
    for group in SizeGroup::ALL {
        let members: Vec<&JobCost> = feeding.iter().copied().filter(|j| j.group == Some(group)).collect();
        if !members.is_empty() {
            out.insert(group.as_str().to_string(), group_stats(&members));
        }
    }
    out.insert("all".to_string(), group_stats(&feeding));
    out
}

/// The estimate for a plan of `plan_bytes` bytes. It uses the plan's group and falls back to all sizes.
pub fn estimate_for(plan_bytes: u64, stats: &BTreeMap<String, GroupStats>, cfg: &Config) -> Option<Estimate> {
    let group = group_of(plan_bytes, cfg).as_str();
    let (name, g) = match stats.get(group) {
        Some(g) => (group, g),
        None => ("all", stats.get("all")?),
    };
    Some(Estimate { group: name.to_string(), median: g.median, low: g.low, high: g.high, n: g.n, n_approx: g.n_approx })
}

/// The size thresholds the figures were made with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thresholds {
    pub small_max_bytes: u64,
    pub medium_max_bytes: u64,
}

/// The content of `costs.json`: the group figures and the role split, never the per-job list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostsFile {
    pub generated_at: f64,
    pub thresholds: Thresholds,
    pub groups: BTreeMap<String, GroupStats>,
    pub roles: BTreeMap<String, u32>,
}

impl CostsFile {
    pub fn build(jobs: &[JobCost], cfg: &Config, now: f64) -> CostsFile {
        let groups = stats(jobs);
        let roles = groups.get("all").map(|g| g.roles.clone()).unwrap_or_default();
        CostsFile {
            generated_at: now,
            thresholds: Thresholds { small_max_bytes: cfg.size_small_max_bytes, medium_max_bytes: cfg.size_medium_max_bytes },
            groups,
            roles,
        }
    }

    pub fn write(&self, path: &Path) -> Result<(), CoreError> {
        let bytes = serde_json::to_vec_pretty(self)?;
        write_atomic(path, &bytes)
    }
}
```

In `lib.rs` extend the re-export: `pub use cost::{count_tasks, estimate_for, group_of, stats, usage_summary, CostsFile, Estimate, GroupStats, JobCost, SizeGroup, Spread, Thresholds, UsageSummary};` (replacing the Task 1 line).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-core`
Expected: all pass.

- [ ] **Step 5: Commit**

Commit `cost.rs` and `lib.rs` with the skill message `feat(factory): add group statistics and the cost estimate`.

---

### Task 3: Diff counts in factory-git and gather_costs in factory-engine

**Files:**
- Modify: `tools/factory/crates/factory-git/src/diff.rs`
- Modify: `tools/factory/crates/factory-git/src/lib.rs`
- Create: `tools/factory/crates/factory-engine/src/costs.rs`
- Modify: `tools/factory/crates/factory-engine/src/lib.rs`

**Interfaces:**
- Consumes: Task 1 and 2 types, `Driver::{store, job_dir, repo, config}` (in `compat.rs`), `Repo::tree_diff` (private to `factory-git`).
- Produces:
  - `pub struct DiffCounts { pub files: usize, pub insertions: usize, pub deletions: usize }` and `Repo::diff_counts(&self, base: &str, head: &str) -> Result<DiffCounts>`.
  - `pub fn gather_costs(driver: &Driver) -> Result<Vec<JobCost>, EngineError>`.

- [ ] **Step 1: Write the failing tests**

Add to `tools/factory/crates/factory-git/tests/repo_basics.rs`, after `diff_to_files_writes_diff_stat_and_names_files` (it uses the existing `Fixture` from `tests/common`):

```rust
#[test]
fn diff_counts_reports_files_and_changed_lines_without_writing_files() {
    let fx = Fixture::new();
    let (base, head) = fx.job_branch("factory/001-x", &[("b.txt", "new\n"), ("base.txt", "base\nmore\n")]);
    let counts = fx.repo().diff_counts(&base, &head).expect("counts");
    assert_eq!((counts.files, counts.insertions, counts.deletions), (2, 2, 0));
    let out = fx.dir.path().join("out");
    assert!(!out.exists(), "diff_counts must not write anything");
}

#[test]
fn diff_counts_of_identical_commits_is_zero_and_of_an_unknown_commit_is_an_error() {
    let fx = Fixture::new();
    let tip = fx.main_sha();
    let counts = fx.repo().diff_counts(&tip, &tip).expect("counts");
    assert_eq!((counts.files, counts.insertions, counts.deletions), (0, 0, 0));
    assert!(fx.repo().diff_counts("0000000000000000000000000000000000000000", &tip).is_err());
}
```

Create `tools/factory/crates/factory-engine/src/costs.rs` with this tests module first (the function comes in Step 3). The engine tests use the `tests/common` fixtures (`SeriesFixture`) for a driver with a real repo; write these as an integration test file `tools/factory/crates/factory-engine/tests/costs.rs` instead:

```rust
mod common;

use factory_core::{Job, SizeGroup, Stage};
use factory_engine::gather_costs;

fn add_finished_job(fx: &common::SeriesFixture, id: &str, plan_text: &str, usage: Option<&str>) -> Job {
    let mut job = Job::new(id, &format!("2026-10-16-{id}-x"), "x", "2026-10-16", "feature", 1.0);
    job.set_stage(Stage::Build, None, false, 2.0).expect("build");
    let dir = fx.driver.job_dir(&job);
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("plan.md"), plan_text).expect("plan");
    if let Some(text) = usage {
        std::fs::write(dir.join("usage.jsonl"), text).expect("usage");
    }
    let added = job.clone();
    fx.driver.store().update(|board| { board.jobs.push(added); Ok(()) }).expect("add job");
    job
}

#[test]
fn gather_costs_builds_one_record_per_job_with_group_tasks_and_usage() {
    let fx = common::SeriesFixture::new("x");
    let plan = format!("# P\n### Task 1: a\n### Task 2: b\n{}", "x".repeat(13_000));
    let usage = "{\"job\":\"101\",\"role\":\"builder\",\"input\":100,\"cache_creation\":0,\"output\":0,\"cache_read\":1000,\"usage_v\":2}\n";
    add_finished_job(&fx, "101", &plan, Some(usage));
    let jobs = gather_costs(&fx.driver).expect("gather");
    let j = jobs.iter().find(|j| j.id == "101").expect("job 101");
    assert_eq!(j.group, Some(SizeGroup::Medium));
    assert_eq!(j.tasks, 2);
    assert_eq!(j.weighted, 200);
    assert_eq!(j.runs, 1);
    assert!(!j.approx);
    assert_eq!(j.lines, None, "no base and head on this job");
}

#[test]
fn a_job_without_a_plan_or_usage_is_listed_without_data() {
    let fx = common::SeriesFixture::new("x");
    let mut job = Job::new("102", "2026-10-16-102-x", "x", "2026-10-16", "f", 1.0);
    job.set_stage(Stage::Build, None, false, 2.0).expect("build");
    fx.driver.store().update(|board| { board.jobs.push(job); Ok(()) }).expect("add job");
    let jobs = gather_costs(&fx.driver).expect("gather");
    let j = jobs.iter().find(|j| j.id == "102").expect("job 102");
    assert_eq!(j.group, None);
    assert_eq!(j.runs, 0);
    assert!(!j.has_data());
}

#[test]
fn a_job_with_commits_that_no_longer_exist_still_gets_a_record() {
    let fx = common::SeriesFixture::new("x");
    let job = add_finished_job(&fx, "103", "# P\n", None);
    fx.driver
        .store()
        .update_job(&job.id, |j| {
            j.base = Some("1111111111111111111111111111111111111111".to_string());
            j.head = Some("2222222222222222222222222222222222222222".to_string());
            Ok(())
        })
        .expect("shas");
    let jobs = gather_costs(&fx.driver).expect("gather");
    let j = jobs.iter().find(|j| j.id == "103").expect("job 103");
    assert_eq!((j.lines, j.files), (None, None));
}

#[test]
fn old_usage_lines_make_a_job_approximate() {
    let fx = common::SeriesFixture::new("x");
    let usage = "{\"job\":\"104\",\"role\":\"builder\",\"input\":1,\"cache_creation\":0,\"output\":0,\"cache_read\":0}\n";
    add_finished_job(&fx, "104", "# P\n", Some(usage));
    let jobs = gather_costs(&fx.driver).expect("gather");
    assert!(jobs.iter().find(|j| j.id == "104").expect("job").approx);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-git diff_counts` and `-p factory-engine --test costs`
Expected: compile errors (`diff_counts` and `gather_costs` not found).

- [ ] **Step 3: Implement**

`factory-git/src/diff.rs`: add above `impl Repo`:

```rust
/// Counts of a diff, without writing any file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffCounts {
    pub files: usize,
    pub insertions: usize,
    pub deletions: usize,
}
```

and inside the `impl Repo` block:

```rust
    /// File and line counts between two commits. Nothing is written.
    pub fn diff_counts(&self, base: &str, head: &str) -> Result<DiffCounts> {
        let diff = self.tree_diff(base, head)?;
        let stats = diff.stats()?;
        Ok(DiffCounts { files: stats.files_changed(), insertions: stats.insertions(), deletions: stats.deletions() })
    }
```

Re-export `DiffCounts` from `factory-git/src/lib.rs` next to `DiffFiles`.

`factory-engine/src/costs.rs`:

```rust
//! Builds the cost record of every board job from its folder and git.

use factory_core::{count_tasks, group_of, usage_summary, JobCost};

use crate::{Driver, EngineError};

/// One record per board job. A missing plan, usage file or commit gives an empty field, never an error.
pub fn gather_costs(driver: &Driver) -> Result<Vec<JobCost>, EngineError> {
    let board = driver.store().load()?;
    let cfg = driver.config();
    let mut out = Vec::new();
    for job in &board.jobs {
        let dir = driver.job_dir(job);
        let plan = std::fs::read_to_string(dir.join("plan.md")).ok();
        let plan_bytes = plan.as_ref().map_or(0, |t| t.len() as u64);
        let tasks = plan.as_deref().map_or(0, count_tasks);
        let group = plan.as_ref().map(|_| group_of(plan_bytes, cfg));
        let usage = std::fs::read_to_string(dir.join("usage.jsonl")).map(|t| usage_summary(&t, &job.id)).unwrap_or_default();
        let counts = match (&job.base, &job.head) {
            (Some(base), Some(head)) => driver.repo().diff_counts(base, head).ok(),
            _ => None,
        };
        out.push(JobCost {
            id: job.id.clone(),
            slug: job.slug.clone(),
            stage: job.stage.as_str().to_string(),
            group,
            plan_bytes,
            tasks,
            lines: counts.map(|c| (c.insertions + c.deletions) as u64),
            files: counts.map(|c| c.files as u64),
            weighted: usage.weighted,
            raw: usage.raw,
            runs: usage.runs,
            approx: usage.approx,
            roles: usage.roles,
        });
    }
    Ok(out)
}
```

In `factory-engine/src/lib.rs` add `mod costs;` and `pub use costs::gather_costs;` next to the other `pub use` lines (follow how `merge_stage` is re-exported).

- [ ] **Step 4: Run the tests to verify they pass**

Run, one per call: `-p factory-git`, then `-p factory-engine --test costs`, then `-p factory-engine`, then `cargo check -j4 --manifest-path tools/factory/Cargo.toml`.
Expected: all pass.

- [ ] **Step 5: Commit**

Commit the files of this task with the skill message `feat(factory): gather the cost record of every job`.

Next plan: docs/superpowers/plans/2026-10-16-factory-cost-report/plan-02-commands-and-estimate.md
