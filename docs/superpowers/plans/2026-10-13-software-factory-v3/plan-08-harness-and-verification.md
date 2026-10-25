# Software Factory v3 Plan 08: Harness Fixes and End-to-End Verification

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task, executed by a factory-builder subagent. Skip that skill's final whole-branch review and its ledger scripts, and do not write a progress file. Steps use checkbox (`- [ ]`) syntax for tracking. Plans 01 to 07 of this series must be finished and merged first: this plan uses `factory-core`, `factory-git`, `factory-agent` (`FakeRunner`), `factory-checks`, `factory-engine`, the `factory` CLI (plan 05), the cut-over (plan 06, the Python ctl and hook are gone) and the dashboard demo seeder (plan 07). Read each file as it is on disk before editing and adapt the names below to it. Do not redo anything earlier plans did.

**Goal:** Make `/verify-rocket` obey the harness guard, prove the whole driver works end to end with a fake agent, add a token report with a baseline comparison, check the dashboard in a headless browser, and close the series with docs, a roadmap entry and a dry-run recipe for the user.

**Architecture:** Three independent tasks. Task 1 edits one command doc and one line of the root `CLAUDE.md`, and adds a lint to the existing harness test runner that pushes every cargo command found in those docs through the real `bash-guard`. Task 2 adds a scenario integration test and a `factory tokens` command in the factory CLI. Task 3 adds a small std-only Rust bin that screenshots the dashboard, then writes docs.

**Tech Stack:** Bash (harness test runner only), Rust (`tools/factory/`, std only for the screenshot bin), Markdown.

**Spec:** [docs/superpowers/specs/2026-10-13-software-factory-v3-rust-driver-design.md](../../specs/2026-10-13-software-factory-v3-rust-driver-design.md) (decisions 4, 6, 7, 9, 10, 11, success check). Earlier art: [v2 plan 06](../2026-10-13-software-factory-v2/plan-06-harness-and-cleanup.md) and gaps E10 to E12 in the [v2 spec](../../specs/2026-10-13-software-factory-v2-design.md). Design doc: [.claude/software-factory.md](../../../../.claude/software-factory.md).

## Global Constraints

- Never use `.unwrap()` anywhere in Rust, tests included (a harness hook blocks it). Use `?` with `Result<(), Box<dyn std::error::Error>>` tests, `.ok_or(...)`, `expect` is allowed only if the hook allows it, otherwise `match` or `assert!(matches!(..))`.
- Cargo commands always pass `-j4`, never `--workspace` or `--all`. The driver is `tools/factory/` (`--manifest-path tools/factory/Cargo.toml`), the server is `tools/factory-server/`. Test one crate at a time: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p <crate>`.
- Code comments are short full sentences ending in a period. No emojis.
- Commits use the skill `dev-workflow-skills:1-git-commit` (invoke it, but run its git commands inline, no helper agents) and a pathspec commit: `git add <paths> && git commit --only -m "..." -- <paths>`. Conventional message. Never `git add -A`, `--all` or `.`.
- Docs stay under `.claude/`. Root `CLAUDE.md` stays small: change existing lines, add no section.
- The driver never pushes. Tests use temp repos (`tempfile`) and `FakeRunner`, never the real `claude` binary and never the real Rocket repo.
- The dry run in Task 3 is written for the user. The builder does not run it.

## Review Focus

- A cargo command added to `verify-rocket.md` or `CLAUDE.md` later without `-j4`, or with `--workspace`, must fail a test. Task 1.
- `/verify-rocket` on a change that touches no crate must say "no Rust crate changed" and run no cargo command. Task 1 (command text, checked by a grep lint).
- A job that fails a check, then passes after exactly one builder resume, must record exactly one resume and merge. Task 2.
- A risky diff must park in `needs-human` and must not merge before `approve`. After `approve` it merges the reviewed sha. Task 2.
- A budget stop must park the job and leave main untouched. Task 2.
- `factory tokens` on a job with no usage records must print zeros, not fail or divide by zero. Task 2.
- No headless browser installed must exit with a BLOCKED message and manual steps, not skip silently. Task 3.

## File Structure

| File | Task | Change |
|---|---|---|
| `.claude/commands/verify-rocket.md` | 1 | Per-crate `-j4` checks, factory crate checks, no workspace command. |
| `CLAUDE.md` | 1 | Three cargo mentions get `-j4`. |
| `.claude/harness/tests/run.sh` | 1 | Lint: cargo commands in the two docs pass the real guard, no-crate wording present. |
| `tools/factory/crates/factory-cli/src/tokens.rs` | 2 | New: `TokenReport`, aggregation, rendering, baseline comparison. |
| `tools/factory/crates/factory-cli/src/main.rs` | 2 | Add the `tokens <id>` subcommand. |
| `tools/factory/crates/factory-cli/tests/support/mod.rs` | 2 | New: temp repo, series fixture, scripted `FakeRunner`. |
| `tools/factory/crates/factory-cli/tests/e2e.rs` | 2 | New: scenario tests. |
| `tools/factory/crates/factory-cli/src/bin/factory-shots.rs` | 3 | New: seed, serve, screenshot, exit 3 when no browser. |
| `.claude/software-factory.md` | 2, 3 | Token baseline section, ruling, open items, dry-run recipe. |
| `docs/roadmaps/roadmap.md` | 3 | New row (next free R-id) and a decisions line. |

## v1 minors triage (E12)

This table is the decision record. The three v1 ledgers are in `.superpowers/sdd/2026-10-13-software-factory-plan-0*/progress.md`. Most minors concerned the Python `factory-ctl`, the Python hook `log-event` and the shell tests, which plan 06 deleted.

| v1 minor | Verdict |
|---|---|
| log-event query strings, 0644 log, os.write short writes, python spawn per tool call, tool_target tests | dropped, the hook is deleted (the driver writes `events.jsonl`) |
| `new-job` half-created job, unvalidated `--date` and `--slug`, board.json mode, corrupt board traceback, hardcoded `main`, `needs-human` without `--reason`, merge message not via skill, builder id vs hook agent id | covered by plans 01, 02 and 05 (typed ids, atomic writes, guarded transitions, fixed merge message, events carry the job id) |
| `tick-plan` temp file and duplicate task numbers, `! is-merged` weak tests, `name3` unused, verdict-last lint, `maxTurns` lint, subcommand dispatch collision | dropped, Python ctl and its shell tests are deleted (verdict last-line is parsed by `parse_verdict` in plan 01) |
| `Head round` ledger wording, builder-fallback overwrites base, previous head lost on rework | covered by plans 01, 04, 05 (`prev_head`, reviewed sha) |
| merge step should use the main checkout, hardcoded `main` | covered by plan 02 (`preflight_main`, `merge_no_ff`) |
| spec-writer `Write` scope, lossy utf8 | dropped, platform limit or ASCII-only output |
| server test hygiene (stale test, `keep_unassigned`, socket thread, `expect`, port TOCTOU) | dropped, test hygiene with no user impact |
| log rotation, `skipped_lines`, security headers, CSP check, in-flight guard, `innerHTML` rewrite | covered by plan 07 (server and page work); if a case is missing, note it in the handoff |
| unknown-stage jobs not listed, `clock()` with missing ts, `renderBoard(null)`, merged tie-break, `datetime` attribute, escaping tests | fix here (Task 3 step 6, tests first) |
| muted text contrast, duplicated `.dot` rules | checked by eye in Task 3, fixed only if the screenshots show a real defect |
| smoke test polling, `server.log` on failure, dead `api()` helper, `seed-demo.sh` idempotence | covered by plan 07 if it kept those files; the Task 3 check reads them and records anything left in the open items |

---

### Task 1: Make `/verify-rocket` and the docs obey the harness guard

**Files:**
- Modify: `.claude/commands/verify-rocket.md`
- Modify: `CLAUDE.md` (the commands block and the Data Flow bullet; line numbers were 21, 22 and 55)
- Modify: `.claude/harness/tests/run.sh` (insert before the final `echo` and summary)

**Interfaces:**
- Consumes: the helpers `guard_case <allow|deny> <rule> <command>`, `pass`, `fail`, the variables `repo` and `harness`, all already in `.claude/harness/tests/run.sh`. The guard `.claude/harness/hooks/bash-guard` denies `cargo test|check|clippy|build` without `-j` and `cargo test` with `--workspace` or `--all`.
- Produces: a lint that every `cargo check|test|clippy|build` command written in `verify-rocket.md` or `CLAUDE.md` is allowed by the real guard.

- [ ] **Step 1: Write the failing lint**

Insert this block above the last `echo` of `.claude/harness/tests/run.sh` (find it with `grep -n '^echo$' .claude/harness/tests/run.sh | tail -1`):

```bash
# ---- cargo commands written in the docs pass the real guard ----
for doc in "$repo/.claude/commands/verify-rocket.md" "$repo/CLAUDE.md"; do
  found=0
  while IFS= read -r c; do
    found=$((found + 1))
    guard_case allow "" "$c"
  done < <(grep -oE 'cargo (check|test|clippy|build)[^`|]*' "$doc" | sed 's/[[:space:]]*$//')
  if [ "$found" -gt 0 ]; then pass "doc lint: $found cargo commands in $(basename "$doc")"; else fail "doc lint: no cargo commands in $(basename "$doc")"; fi
done
if grep -qE -- '--workspace|cargo [a-z]+ --all' "$repo/.claude/commands/verify-rocket.md"; then
  fail "doc lint: verify-rocket.md has no --workspace or --all"
else
  pass "doc lint: verify-rocket.md has no --workspace or --all"
fi
if grep -q 'no Rust crate changed' "$repo/.claude/commands/verify-rocket.md"; then
  pass "doc lint: verify-rocket.md says what to do when no crate changed"
else
  fail "doc lint: verify-rocket.md says what to do when no crate changed"
fi
if grep -q 'tools/factory' "$repo/.claude/commands/verify-rocket.md"; then
  pass "doc lint: verify-rocket.md covers the factory crates"
else
  fail "doc lint: verify-rocket.md covers the factory crates"
fi
```

- [ ] **Step 2: Run it and see it fail**

Run: `bash .claude/harness/tests/run.sh 2>&1 | grep -E '^FAIL'`
Expected: FAIL lines for the bare `cargo check`, `cargo test --workspace --no-run` and the bare `cargo test -p <crate>` found in the docs, and for the two wording lints.

- [ ] **Step 3: Rewrite `/verify-rocket`**

Replace everything from `3. For rust or all` to the end of the report table (step 5) in `.claude/commands/verify-rocket.md`. Keep the frontmatter, steps 1 and 2, steps 6 and 7, and the Conventions section.

````markdown
3. For rust or all, check only the packages that changed. From repo root:
- List changed paths: `git diff --name-only main -- crates src-tauri tools/factory tools/factory-server` plus `git status --porcelain -- crates src-tauri tools/factory tools/factory-server`.
- Map `crates/<name>/...` to package `<name>`, and `src-tauri/...` to package `rocket`. Drop duplicates.
- Map `tools/factory/crates/<name>/...` to factory package `<name>`. Map `tools/factory-server/...` to the factory server.
- If no crate changed, report "no Rust crate changed" and run no cargo command.
- For each changed Rocket package, one at a time, run:
  - cargo check -j4 -p <crate>
  - cargo test -j4 -p <crate> --no-run
- For each changed factory package, one at a time, run:
  - cargo check -j4 --manifest-path tools/factory/Cargo.toml -p <crate>
  - cargo test -j4 --manifest-path tools/factory/Cargo.toml -p <crate> --no-run
- If `tools/factory-server/` changed, run:
  - cargo check -j4 --manifest-path tools/factory-server/Cargo.toml
  - cargo test -j4 --manifest-path tools/factory-server/Cargo.toml --no-run
- Never run a workspace-wide build or test, and never leave out `-j4`. The harness guard denies both.

4. For frontend or all, run from repo root:
- yarn tsc --noEmit
- yarn check

5. Report output in a strict table, one row per command that ran:

| Check | Status | Notes |
|---|---|---|
| cargo check -j4 -p <crate> | pass/fail | error count or key failure line |
| cargo test -j4 -p <crate> --no-run | pass/fail | error count or key failure line |
| yarn tsc --noEmit | pass/fail | error count or key failure line |
| yarn check | pass/fail | error count or key failure line |
````

- [ ] **Step 4: Fix the three mentions in `CLAUDE.md`**

Make exactly these edits and nothing else:

```text
cargo check               # fast Rust validation   ->  cargo check -j4            # fast Rust validation
cargo test -p <crate> <test_name>                  ->  cargo test -j4 -p <crate> <test_name>
`cargo check` for fast validation.                 ->  `cargo check -j4` for fast validation.
```

- [ ] **Step 5: Run the whole harness suite**

Run: `bash .claude/harness/tests/run.sh 2>&1 | tail -6`
Expected: `ALL PASSED`, with `doc lint` PASS lines for both files.

- [ ] **Step 6: Prove the lint catches a regression**

Run, as one command:
`cp .claude/commands/verify-rocket.md "$TMPDIR/vr.bak" && printf '\n- cargo check\n' >> .claude/commands/verify-rocket.md; bash .claude/harness/tests/run.sh 2>&1 | grep -c '^FAIL'; cp "$TMPDIR/vr.bak" .claude/commands/verify-rocket.md`
Expected: a count of at least 1. Then `git diff --stat .claude/commands/verify-rocket.md` shows only the step 3 to 5 rewrite.

- [ ] **Step 7: Commit**

Invoke the skill `dev-workflow-skills:1-git-commit`, then run inline:

```bash
git add .claude/commands/verify-rocket.md CLAUDE.md .claude/harness/tests/run.sh
git commit --only -m "fix(harness): make verify-rocket obey the cargo guard" -- .claude/commands/verify-rocket.md CLAUDE.md .claude/harness/tests/run.sh
```

---

### Task 2: End-to-end scenario test and `factory tokens`

**Files:**
- Create: `tools/factory/crates/factory-cli/src/tokens.rs`
- Modify: `tools/factory/crates/factory-cli/src/main.rs` (add `mod tokens;` and the `tokens` subcommand)
- Create: `tools/factory/crates/factory-cli/tests/support/mod.rs`
- Create: `tools/factory/crates/factory-cli/tests/e2e.rs`
- Modify: `.claude/software-factory.md` (add `## Token baseline`)

**Interfaces:**
- Consumes (names from the spec interface table; read the real signatures first and adapt): `factory_core::{Board, Job, Stage, Config, weighted_tokens, BudgetState}`, `factory_git::Repo`, `factory_agent::{FakeRunner, AgentSpec, AgentRun, Usage, StopReason}`, `factory_checks::{plan_checks, run_checks}`, `factory_engine::{Engine, run_job, run_series}`, the CLI functions behind `approve` and `status` (plan 05).
- Produces: `factory_cli::tokens::{UsageRecord, TokenReport, aggregate, render, BASELINE}`, and the command `factory tokens <id>`.

**Read first (this step needs no code):** open the plan 03 to 05 tests that already use `FakeRunner` and temp repos, and the `usage` events that plan 03 writes to `events.jsonl`. Write down in a scratch note: (a) how `FakeRunner` is scripted (a queue of results per role?), (b) the exact JSON of a usage event (job id, role, token fields), (c) how `Config` can be built in a test, (d) how `approve` is invoked as a function. The code below uses these assumed names, collected in one place so you adapt only that place:

| Assumed | Meaning |
|---|---|
| `support::Scenario::new(plans)` | builds a temp main repo, the series files and a `Config` with a tiny `risk_paths` list and a check command that passes or fails on request |
| `Scenario::runner()` | a `FakeRunner` whose builder and reviewer answers come from a script |
| `Script::builder(edit)` and `Script::reviewer(verdict_line)` | queue one agent answer each, `edit` is a file path and content the fake builder writes and commits in its worktree |
| `Scenario::engine()` | an `Engine` over the temp repo |
| `scenario.board()` | reads `board.json` through `factory_core::Board` |

#### Part A: token report (pure, test first)

- [ ] **Step 1: Write the failing unit tests**

Create `tools/factory/crates/factory-cli/src/tokens.rs` with only the tests and empty signatures:

```rust
//! Token report: raw and weighted totals per agent role and per job.

use std::collections::BTreeMap;

/// Job 001 measured baseline from the spec: raw tokens, weighted tokens and orchestrator share.
pub const BASELINE: Baseline = Baseline { raw: 12_750_000, weighted: 2_170_000, orchestrator_percent: 53.6 };

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Baseline {
    pub raw: u64,
    pub weighted: u64,
    pub orchestrator_percent: f64,
}

/// One agent run as logged by the driver.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageRecord {
    pub job: String,
    pub role: String,
    pub input: u64,
    pub cache_creation: u64,
    pub output: u64,
    pub cache_read: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Totals {
    pub raw: u64,
    pub weighted: u64,
    pub runs: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TokenReport {
    pub by_role: BTreeMap<String, Totals>,
    pub total: Totals,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(role: &str, input: u64, cc: u64, out: u64, cr: u64) -> UsageRecord {
        UsageRecord { job: "001".into(), role: role.into(), input, cache_creation: cc, output: out, cache_read: cr }
    }

    #[test]
    fn weighted_counts_cache_reads_at_one_tenth() {
        let r = rec("builder", 100, 200, 300, 1_000);
        assert_eq!(weighted(&r), 100 + 200 + 300 + 100);
    }

    #[test]
    fn aggregate_groups_by_role_and_sums_totals() {
        let report = aggregate(&[rec("builder", 10, 0, 10, 0), rec("builder", 5, 0, 5, 0), rec("reviewer", 1, 1, 1, 10)]);
        assert_eq!(report.by_role["builder"].runs, 2);
        assert_eq!(report.by_role["builder"].raw, 30);
        assert_eq!(report.by_role["reviewer"].raw, 13);
        assert_eq!(report.by_role["reviewer"].weighted, 4);
        assert_eq!(report.total.raw, 43);
        assert_eq!(report.total.runs, 3);
    }

    #[test]
    fn aggregate_of_nothing_is_zero() {
        let report = aggregate(&[]);
        assert_eq!(report.total, Totals::default());
        assert!(report.by_role.is_empty());
    }

    #[test]
    fn render_shows_roles_totals_and_baseline_percent() {
        let report = aggregate(&[rec("builder", 217_000, 0, 0, 0)]);
        let text = render("001", &report);
        assert!(text.contains("job 001"));
        assert!(text.contains("builder"));
        assert!(text.contains("raw 217000"));
        assert!(text.contains("10.0% of job 001 baseline weighted"), "{text}");
    }

    #[test]
    fn render_of_an_empty_report_does_not_divide_by_zero() {
        let text = render("002", &aggregate(&[]));
        assert!(text.contains("raw 0"));
        assert!(text.contains("0.0% of job 001 baseline weighted"));
    }
}
```

- [ ] **Step 2: Run and see it fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli tokens`
Expected: compile error, `weighted`, `aggregate` and `render` not found.

- [ ] **Step 3: Implement**

Add above the `#[cfg(test)]` block. If plan 01's `weighted_tokens` takes a different argument shape, make `weighted` a one-line wrapper over it instead of the formula, so the weighting rule lives in one place.

```rust
/// Weighted tokens: input, cache creation and output count fully, cache reads count one tenth.
pub fn weighted(r: &UsageRecord) -> u64 {
    r.input + r.cache_creation + r.output + r.cache_read / 10
}

fn raw(r: &UsageRecord) -> u64 {
    r.input + r.cache_creation + r.output + r.cache_read
}

pub fn aggregate(records: &[UsageRecord]) -> TokenReport {
    let mut report = TokenReport::default();
    for r in records {
        let role = report.by_role.entry(r.role.clone()).or_default();
        role.raw += raw(r);
        role.weighted += weighted(r);
        role.runs += 1;
        report.total.raw += raw(r);
        report.total.weighted += weighted(r);
        report.total.runs += 1;
    }
    report
}

pub fn render(job: &str, report: &TokenReport) -> String {
    let mut out = format!("tokens for job {job}\n");
    for (role, t) in &report.by_role {
        out.push_str(&format!("  {role:<10} runs {:<3} raw {:<10} weighted {}\n", t.runs, t.raw, t.weighted));
    }
    out.push_str(&format!(
        "  total      runs {:<3} raw {} weighted {}\n",
        report.total.runs, report.total.raw, report.total.weighted
    ));
    let percent = report.total.weighted as f64 * 100.0 / BASELINE.weighted as f64;
    out.push_str(&format!(
        "  {percent:.1}% of job 001 baseline weighted ({} weighted, {} raw, orchestrator {}%)\n",
        BASELINE.weighted, BASELINE.raw, BASELINE.orchestrator_percent
    ));
    out
}
```

The test string `raw 217000` needs the total line to print `raw 217000` right after `runs 1   `; the format above prints `raw {}` for the total, so it matches. Role lines use `raw {:<10}`, which also contains `raw 217000`.

- [ ] **Step 4: Run and see it pass**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli tokens`
Expected: 5 passed.

#### Part B: the `tokens` subcommand

- [ ] **Step 5: Write the failing loader test**

Append to the `tests` module. Replace the JSON field names with the real usage event shape you wrote down in the read-first step (the test and the loader must use the same names):

```rust
    #[test]
    fn load_reads_only_usage_events_for_the_job() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let log = dir.path().join("events.jsonl");
        std::fs::write(
            &log,
            concat!(
                "{\"kind\":\"usage\",\"job\":\"001\",\"role\":\"builder\",\"input\":1,\"cache_creation\":2,\"output\":3,\"cache_read\":40}\n",
                "{\"kind\":\"usage\",\"job\":\"002\",\"role\":\"builder\",\"input\":9,\"cache_creation\":9,\"output\":9,\"cache_read\":9}\n",
                "{\"kind\":\"tool\",\"job\":\"001\",\"role\":\"builder\"}\n",
                "garbage line\n"
            ),
        )?;
        let records = load_usage(&log, "001")?;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].cache_read, 40);
        Ok(())
    }

    #[test]
    fn load_of_a_missing_log_is_empty() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        assert!(load_usage(&dir.path().join("none.jsonl"), "001")?.is_empty());
        Ok(())
    }
```

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli tokens` and expect a compile error for `load_usage`.

- [ ] **Step 6: Implement `load_usage` and the subcommand**

Add `serde_json` and `tempfile` (dev) to `factory-cli/Cargo.toml` if missing. In `tokens.rs`:

```rust
/// Reads usage events for one job. Unreadable lines and other event kinds are skipped.
pub fn load_usage(log: &std::path::Path, job: &str) -> Result<Vec<UsageRecord>, std::io::Error> {
    let text = match std::fs::read_to_string(log) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut out = Vec::new();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if v["kind"] != "usage" || v["job"] != job {
            continue;
        }
        let n = |k: &str| v[k].as_u64().unwrap_or(0);
        out.push(UsageRecord {
            job: job.to_string(),
            role: v["role"].as_str().unwrap_or("unknown").to_string(),
            input: n("input"),
            cache_creation: n("cache_creation"),
            output: n("output"),
            cache_read: n("cache_read"),
        });
    }
    Ok(out)
}
```

In `main.rs`, add `mod tokens;` (or `pub mod` in `lib.rs` if the crate has one, so the integration test can reach it), a clap variant `Tokens { id: String }`, and a handler that finds the events log through the same helper the other commands use (the path is `.superpowers/factory/events.jsonl` under the factory root):

```rust
Command::Tokens { id } => {
    let records = tokens::load_usage(&events_log_path(&root), &id)?;
    print!("{}", tokens::render(&id, &tokens::aggregate(&records)));
    Ok(())
}
```

Use the real names for the factory root and for the events log helper that plan 03 and 05 defined; do not add a second path rule. Note `unwrap_or(0)` is allowed (it is not `unwrap()`).

- [ ] **Step 7: Run and see it pass**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli tokens`
Expected: 7 passed.

- [ ] **Step 8: Commit part A and B**

Invoke the skill `dev-workflow-skills:1-git-commit`, then:

```bash
git add tools/factory/crates/factory-cli/src/tokens.rs tools/factory/crates/factory-cli/src/main.rs tools/factory/crates/factory-cli/Cargo.toml
git commit --only -m "feat(factory): add the tokens report with a job 001 baseline" -- tools/factory/crates/factory-cli/src/tokens.rs tools/factory/crates/factory-cli/src/main.rs tools/factory/crates/factory-cli/Cargo.toml
```

Add `tools/factory/crates/factory-cli/src/lib.rs` and `tools/factory/Cargo.lock` to both lists if you changed them.

#### Part C: the scenario test

The series is two plans, `plan-a.md` ending `Next plan: <path of plan-b.md>` and `plan-b.md` ending `Next plan: none`. The fake builder edits one file per plan in its worktree. Five scenarios cover the spec: a failing check that resumes the builder once (plan A), a CHANGES verdict fix round (plan A, round after), a risky diff (plan B), a main-moved sync (plan B), and a budget stop (separate job).

- [ ] **Step 9: Write the support module**

Create `tests/support/mod.rs`. The bodies call the real engine and fake runner; adapt only the lines marked `ADAPT` to the names written down in the read-first step.

```rust
//! Test fixtures: a temp main repo, a two-plan series and a scripted fake runner.

use std::path::{Path, PathBuf};
use std::process::Command;

pub type TestResult = Result<(), Box<dyn std::error::Error>>;

pub struct Scenario {
    pub dir: tempfile::TempDir,
    pub plan_a: PathBuf,
    pub plan_b: PathBuf,
}

/// Runs a git command in a temp repo. Tests may shell out to git, the driver itself may not.
pub fn git(dir: &Path, args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output()?;
    if !out.status.success() {
        return Err(format!("git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr)).into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

impl Scenario {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        git(root, &["init", "-q", "-b", "main"])?;
        git(root, &["config", "user.email", "t@t"])?;
        git(root, &["config", "user.name", "t"])?;
        std::fs::create_dir_all(root.join(".claude/factory"))?;
        std::fs::write(root.join("README.md"), "demo\n")?;
        // The config marks one path as risky and uses a check command we can flip with a file.
        std::fs::write(
            root.join(".claude/factory/config.toml"),
            "risk_paths = [\"secrets/\"]\ndiff_cap_lines = 4000\nbudget_weighted = 3000000\nfix_rounds = 2\nstall_minutes = 10\n\
             builder_model = \"sonnet\"\nreviewer_model = \"sonnet\"\n\n[[checks]]\nname = \"gate\"\npaths = [\"*\"]\n\
             command = \"test ! -f .fail-check\"\n", // ADAPT: the real [[checks]] keys from plan 01.
        )?;
        git(root, &["add", "README.md", ".claude/factory/config.toml"])?;
        git(root, &["commit", "-q", "-m", "init"])?;
        let plans = root.join("docs/plans");
        std::fs::create_dir_all(&plans)?;
        let plan_b = plans.join("plan-b.md");
        let plan_a = plans.join("plan-a.md");
        std::fs::write(&plan_b, "# B\n\n### Task 1: b\n\nNext plan: none\n")?;
        std::fs::write(&plan_a, format!("# A\n\n### Task 1: a\n\nNext plan: {}\n", plan_b.display()))?;
        git(root, &["add", "docs/plans"])?;
        git(root, &["commit", "-q", "-m", "plans"])?;
        Ok(Self { dir, plan_a, plan_b })
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    pub fn main_tip(&self) -> Result<String, Box<dyn std::error::Error>> {
        git(self.root(), &["rev-parse", "main"])
    }

    pub fn merges_on_main(&self) -> Result<usize, Box<dyn std::error::Error>> {
        Ok(git(self.root(), &["log", "--merges", "--format=%s", "main"])?.lines().count())
    }
}
```

Then a `Script` type that queues fake agent answers. Build it on the real `FakeRunner` API (`ADAPT`): a builder step carries the files the fake agent writes and commits inside its worktree plus the `Usage`, and a reviewer step carries the result text, whose last line is the verdict.

```rust
/// One scripted answer for the fake runner.
pub enum Step {
    /// Builder: write these files in the worktree, commit them, report this usage.
    Build { files: Vec<(&'static str, &'static str)>, weighted_hint: u64 },
    /// Reviewer: answer with this text. The last line is the verdict.
    Review { text: &'static str },
}

pub const APPROVE: &str = "Looks right.\nVERDICT: APPROVE";
pub const CHANGES: &str = "The helper is wrong.\nVERDICT: CHANGES";
```

- [ ] **Step 10: Write the failing scenario tests**

Create `tests/e2e.rs`. Each test builds a `Scenario`, queues steps in the order the engine asks for them (builder, review, then fix rounds), runs, and asserts on the board, git and `factory tokens`. Comments name the spec rule each test pins.

```rust
mod support;
use support::{Scenario, Step, TestResult, APPROVE, CHANGES};

#[test]
fn two_plans_go_from_new_to_merged_with_a_failed_check_resumed_once() -> TestResult {
    let sc = Scenario::new()?;
    let mut run = sc.series_run(vec![
        // Plan A, build: leaves the check failing by writing .fail-check.
        Step::Build { files: vec![("a.txt", "a\n"), (".fail-check", "x\n")], weighted_hint: 1_000 },
        // Resume after the failed check: the fake builder removes the flag file.
        Step::Build { files: vec![("a.txt", "a2\n")], weighted_hint: 500 },
        Step::Review { text: APPROVE },
        // Plan B: clean build and approve.
        Step::Build { files: vec![("b.txt", "b\n")], weighted_hint: 1_000 },
        Step::Review { text: APPROVE },
    ])?;
    run.start_series(&sc.plan_a)?;
    // Spec decision 4: a red check resumes the builder and spends no reviewer run.
    assert_eq!(run.resumes("001")?, 1);
    assert_eq!(run.review_runs("001")?, 1);
    // Spec decision 11: plan B starts by itself after plan A merges.
    assert_eq!(sc.merges_on_main()?, 2);
    assert_eq!(run.stage("001")?, "merged");
    assert_eq!(run.stage("002")?, "merged");
    Ok(())
}

#[test]
fn a_changes_verdict_runs_one_fix_round_then_merges() -> TestResult {
    let sc = Scenario::new()?;
    let mut run = sc.single_run(vec![
        Step::Build { files: vec![("a.txt", "a\n")], weighted_hint: 1_000 },
        Step::Review { text: CHANGES },
        Step::Build { files: vec![("a.txt", "fixed\n")], weighted_hint: 400 },
        Step::Review { text: APPROVE },
    ])?;
    run.start_job(&sc.plan_b)?;
    assert_eq!(run.fix_rounds("001")?, 1);
    assert_eq!(run.stage("001")?, "merged");
    // Spec decision 10: the merge commit has the fixed message.
    assert!(sc.last_merge_subject()?.starts_with("chore(factory): merge job 001"));
    Ok(())
}

#[test]
fn a_risky_diff_waits_for_approve_and_then_merges_the_reviewed_sha() -> TestResult {
    let sc = Scenario::new()?;
    let before = sc.main_tip()?;
    let mut run = sc.single_run(vec![
        Step::Build { files: vec![("secrets/token.txt", "x\n")], weighted_hint: 1_000 },
        Step::Review { text: APPROVE },
    ])?;
    run.start_job(&sc.plan_b)?;
    // Spec decision 6: risky paths park the job, main is untouched.
    assert_eq!(run.stage("001")?, "needs-human");
    assert_eq!(sc.main_tip()?, before);
    let reviewed = run.head_sha("001")?;
    run.approve("001")?;
    assert_eq!(run.stage("001")?, "merged");
    assert_eq!(sc.second_parent_of_last_merge()?, reviewed);
    Ok(())
}

#[test]
fn a_moved_main_is_synced_into_the_branch_and_still_merges() -> TestResult {
    let sc = Scenario::new()?;
    let mut run = sc.single_run(vec![
        Step::Build { files: vec![("b.txt", "b\n")], weighted_hint: 1_000 },
        Step::Review { text: APPROVE },
        // Review of the sync diff after main moved.
        Step::Review { text: APPROVE },
    ])?;
    // Spec decision 10: move main between the review and the merge.
    run.before_merge(|root| {
        std::fs::write(root.join("other.txt"), "o\n")?;
        support::git(root, &["add", "other.txt"])?;
        support::git(root, &["commit", "-q", "-m", "chore: move main"])?;
        Ok(())
    });
    run.start_job(&sc.plan_b)?;
    assert_eq!(run.stage("001")?, "merged");
    assert_eq!(run.sync_count("001")?, 1);
    assert!(sc.root().join("other.txt").exists() && sc.root().join("b.txt").exists());
    Ok(())
}

#[test]
fn a_budget_stop_parks_the_job_and_leaves_main_alone() -> TestResult {
    let sc = Scenario::new()?;
    let before = sc.main_tip()?;
    // The first builder run reports more weighted tokens than the budget.
    let mut run = sc.single_run(vec![Step::Build { files: vec![("a.txt", "a\n")], weighted_hint: 3_500_000 }])?;
    run.start_job(&sc.plan_b)?;
    assert_eq!(run.stage("001")?, "needs-human");
    assert_eq!(run.review_runs("001")?, 0);
    assert_eq!(sc.main_tip()?, before);
    Ok(())
}

#[test]
fn tokens_report_totals_match_the_scripted_usage() -> TestResult {
    let sc = Scenario::new()?;
    let mut run = sc.single_run(vec![
        Step::Build { files: vec![("a.txt", "a\n")], weighted_hint: 1_000 },
        Step::Review { text: APPROVE },
    ])?;
    run.start_job(&sc.plan_b)?;
    let text = run.tokens_report("001")?;
    assert!(text.contains("builder"));
    assert!(text.contains("reviewer"));
    assert!(text.contains("% of job 001 baseline weighted"));
    Ok(())
}
```

`sc.series_run`, `sc.single_run`, `run.*` and `sc.last_merge_subject` / `second_parent_of_last_merge` are thin helpers in `support/mod.rs`. Write them now with the same style: `series_run` and `single_run` build the real `Engine` over the temp repo and the scripted `FakeRunner` (`ADAPT`), the accessors read `board.json` through `factory_core::Board` (stage, resumes, review runs, fix rounds, sync count, head sha come from the job fields plan 01 and 05 defined; if a counter does not exist as a field, count the matching events in `events.jsonl` instead), `before_merge` registers a closure the engine's merge step calls through a test hook (if plan 05 gave no hook, run the closure inside the scripted runner when it answers the first reviewer step), and `tokens_report` calls `tokens::render` over `tokens::load_usage`. The git helpers use `support::git`:

```rust
impl Scenario {
    pub fn last_merge_subject(&self) -> Result<String, Box<dyn std::error::Error>> {
        git(self.root(), &["log", "--merges", "-1", "--format=%s", "main"])
    }
    pub fn second_parent_of_last_merge(&self) -> Result<String, Box<dyn std::error::Error>> {
        git(self.root(), &["rev-parse", "main^2"])
    }
}
```

- [ ] **Step 11: Run and see the tests fail, then make them pass**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli --test e2e`
Expected first: failures from missing helpers or an unwired script. Fix only the support module until all six tests pass. If a scenario reveals a real engine bug (for example a risky job merging, or a resume counted twice), do not paper over it in the test: fix it in the owning crate with its own failing unit test first, and list it in the handoff.

- [ ] **Step 12: Add the baseline section to `.claude/software-factory.md`**

Add a section `## Token baseline` after the dashboard section:

```markdown
## Token baseline

Measured on job 001 (v1, LLM orchestrator, full review set): about 12.75M raw tokens, about 2.17M weighted (input + cache creation + output + cache read / 10), orchestrator 53.6 percent of the total, commit helper agents about 7 percent, diff of 23 KB read by 5 reviewers.

`factory tokens <id>` prints raw and weighted totals per agent role and the percent of that weighted baseline. Compare like with like:

| Measure | Job 001 (v1) | Lean job target |
|---|---|---|
| Weighted total for the whole job | about 2.17M | well below, aim under 1.1M for a comparable small job |
| Orchestrator share | 53.6 percent | not counted: the driver is Rust, the interactive session only plans |
| Reviewer runs per clean round | 5 | 1 |
| Commit helper agents | 2 or more per commit | 0 |

The plan-time session (brainstorming and writing plans) is interactive and is not in `factory tokens`. Note it by hand from the session's usage line when comparing totals. The target is a guideline taken from the spec's success check ("well below 2.2M weighted"), not a gate.
```

- [ ] **Step 13: Run the crate tests and commit**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli` and `cargo check -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli`
Expected: all pass, no warnings that fail the build.

Invoke the skill `dev-workflow-skills:1-git-commit`, then:

```bash
git add tools/factory/crates/factory-cli/tests/support/mod.rs tools/factory/crates/factory-cli/tests/e2e.rs .claude/software-factory.md
git commit --only -m "test(factory): add the end-to-end scenarios and the token baseline" -- tools/factory/crates/factory-cli/tests/support/mod.rs tools/factory/crates/factory-cli/tests/e2e.rs .claude/software-factory.md
```

Add any engine or core file you had to fix to both lists, in a separate `fix(factory): ...` commit if it is a real bug fix.

---

### Task 3: Browser check, docs sync, roadmap and the dry-run recipe

**Files:**
- Create: `tools/factory/crates/factory-cli/src/bin/factory-shots.rs`
- Modify: `tools/factory-server/static/board.js` and `tools/factory-server/tests/board.test.js` (only for the minors in step 6 that still fail)
- Modify only if the screenshots show a real defect: `tools/factory-server/static/dashboard.css`
- Modify: `.claude/software-factory.md`, `docs/roadmaps/roadmap.md`

**Interfaces:**
- Consumes: the demo seeder from plan 07 and the built `factory-server` binary with `--root` and `--port`, plus `/healthz`. Run `ls tools/factory-server/tests/` and read the seeder's header comment first. The constants `SEED_PROGRAM`, `SEED_ARGS` and `SERVER_BIN` below are the only lines to adapt.
- Produces: PNG files under `.superpowers/factory/screenshots/` (not committed), exit code 0 on success and 3 with a BLOCKED message when no browser exists, a dated ruling and the dry-run recipe.

`google-chrome` and `firefox` are installed on this machine. The bin uses the browser binary directly with `--headless --screenshot`, std only, no npm install.

- [ ] **Step 1: Write the failing tests for the pure parts**

Create `factory-shots.rs` with the tests and signatures only:

```rust
//! Seeds the demo board, serves it on a free port and screenshots the page with a headless browser.
//! Exit 0 on success, 1 on a failure, 3 when no headless browser is installed.

pub const BROWSERS: [&str; 4] = ["google-chrome", "chromium", "chromium-browser", "firefox"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_browser_takes_the_first_installed_in_order() {
        assert_eq!(pick_browser(|n| n == "firefox" || n == "chromium"), Some("chromium"));
        assert_eq!(pick_browser(|_| false), None);
    }

    #[test]
    fn chrome_args_set_size_and_screenshot_path() {
        let args = shot_args("google-chrome", "/o/a.png", 400, 2600, "http://127.0.0.1:9/", &[]);
        assert!(args.contains(&"--headless=new".to_string()));
        assert!(args.contains(&"--window-size=400,2600".to_string()));
        assert!(args.contains(&"--screenshot=/o/a.png".to_string()));
        assert_eq!(args.last().map(String::as_str), Some("http://127.0.0.1:9/"));
    }

    #[test]
    fn firefox_args_use_its_own_flags() {
        let args = shot_args("firefox", "/o/a.png", 1440, 1500, "http://127.0.0.1:9/", &[]);
        assert_eq!(args[0], "--headless");
        assert!(args.contains(&"/o/a.png".to_string()));
        assert!(args.contains(&"1440,1500".to_string()));
    }

    #[test]
    fn blocked_message_names_the_browsers_and_the_manual_steps() {
        let text = blocked_message();
        assert!(text.starts_with("BLOCKED"));
        assert!(text.contains("google-chrome"));
        assert!(text.contains("400"));
    }

    #[test]
    fn a_free_port_can_be_bound_again_right_after() -> Result<(), Box<dyn std::error::Error>> {
        let port = free_port()?;
        let again = std::net::TcpListener::bind(("127.0.0.1", port));
        assert!(again.is_ok());
        Ok(())
    }
}
```

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli --bin factory-shots`
Expected: compile error, the functions are missing.

- [ ] **Step 2: Implement the bin**

Add above the tests. Adapt only the three constants to the real seeder and server.

```rust
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

// ADAPT: the plan 07 seeder. {root} and {port} are replaced before running.
const SEED_PROGRAM: &str = "bash";
const SEED_ARGS: [&str; 3] = ["tools/factory-server/tests/seed-demo.sh", "{root}", "{port}"];
const SERVER_BIN: &str = "tools/factory-server/target/debug/factory-server";

pub fn pick_browser(installed: impl Fn(&str) -> bool) -> Option<&'static str> {
    BROWSERS.iter().copied().find(|n| installed(n))
}

pub fn shot_args(browser: &str, file: &str, w: u32, h: u32, url: &str, extra: &[&str]) -> Vec<String> {
    if browser == "firefox" {
        return vec!["--headless".into(), "--screenshot".into(), file.into(), "--window-size".into(), format!("{w},{h}"), url.into()];
    }
    let mut args: Vec<String> = ["--headless=new", "--no-sandbox", "--disable-gpu", "--hide-scrollbars"].map(String::from).to_vec();
    args.extend(extra.iter().map(|s| s.to_string()));
    args.push("--virtual-time-budget=4000".into());
    args.push(format!("--window-size={w},{h}"));
    args.push(format!("--screenshot={file}"));
    args.push(url.into());
    args
}

pub fn blocked_message() -> String {
    "BLOCKED: no headless browser found (tried google-chrome, chromium, chromium-browser, firefox).\n\
     Manual steps: seed the demo board, start the dashboard, open it in a real window and check dark theme,\n\
     light theme and a 400 px wide window: six stage columns, summary strip, readable review chips,\n\
     needs-human card with its copy chips, no sideways page scroll."
        .to_string()
}

pub fn free_port() -> std::io::Result<u16> {
    Ok(TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port())
}

fn on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(name).is_file()))
        .unwrap_or(false)
}

fn healthy(port: u16) -> bool {
    let Ok(mut s) = TcpStream::connect(("127.0.0.1", port)) else { return false };
    if s.write_all(b"GET /healthz HTTP/1.0\r\n\r\n").is_err() {
        return false;
    }
    let mut buf = String::new();
    s.read_to_string(&mut buf).is_ok() && buf.starts_with("HTTP/1.") && buf.contains(" 200")
}

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let Some(browser) = pick_browser(on_path) else {
        eprintln!("{}", blocked_message());
        return Ok(3);
    };
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".superpowers/factory/screenshots".into()));
    std::fs::create_dir_all(&out)?;
    let tmp = tempfile::tempdir()?;
    let root = tmp.path().join("root");
    let port = free_port()?;

    let args: Vec<String> = SEED_ARGS.iter().map(|a| a.replace("{root}", &root.to_string_lossy()).replace("{port}", &port.to_string())).collect();
    if !Command::new(SEED_PROGRAM).args(&args).stdout(Stdio::null()).status()?.success() {
        eprintln!("FAIL: the demo seeder failed");
        return Ok(1);
    }
    if !Command::new("cargo").args(["build", "-j4", "-q", "--manifest-path", "tools/factory-server/Cargo.toml"]).status()?.success() {
        eprintln!("FAIL: the server did not build");
        return Ok(1);
    }
    let log = std::fs::File::create(tmp.path().join("server.log"))?;
    let _server = Server(
        Command::new(SERVER_BIN)
            .args(["--root", &root.to_string_lossy(), "--port", &port.to_string()])
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .spawn()?,
    );
    let start = Instant::now();
    while !healthy(port) {
        if start.elapsed() > Duration::from_secs(10) {
            eprintln!("FAIL: the server did not start, see {}", tmp.path().join("server.log").display());
            return Ok(1);
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let url = format!("http://127.0.0.1:{port}/");
    let shots: [(&str, u32, u32, &[&str]); 3] = [
        ("desktop-dark.png", 1440, 1500, &[]),
        ("phone-dark.png", 400, 2600, &[]),
        ("desktop-light.png", 1440, 1500, &["--blink-settings=preferredColorScheme=1"]),
    ];
    let mut failed = false;
    for (name, w, h, extra) in shots {
        let file = out.join(name);
        let _ = Command::new(browser).args(shot_args(browser, &file.to_string_lossy(), w, h, &url, extra)).stdout(Stdio::null()).stderr(Stdio::null()).status()?;
        if std::fs::metadata(&file).map(|m| m.len() > 0).unwrap_or(false) {
            println!("wrote {}", file.display());
        } else {
            eprintln!("FAIL: no screenshot {}", file.display());
            failed = true;
        }
    }
    let (dark, light) = (std::fs::read(out.join("desktop-dark.png")), std::fs::read(out.join("desktop-light.png")));
    if let (Ok(d), Ok(l)) = (dark, light) {
        if d == l {
            println!("NOTE: the light screenshot equals the dark one, so the light theme was not exercised.");
        }
    }
    Ok(if failed { 1 } else { 0 })
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("FAIL: {e}");
            std::process::exit(1);
        }
    }
}
```

Ensure `tempfile` is a normal (not dev) dependency of `factory-cli` and run `cargo fmt --manifest-path tools/factory/Cargo.toml -p factory-cli`.

- [ ] **Step 3: Run the unit tests**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli --bin factory-shots`
Expected: 5 passed.

- [ ] **Step 4: Run it against the real dashboard**

Run: `cargo run -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli --bin factory-shots; echo "exit $?"`
Expected: three `wrote ...png` lines and `exit 0`. If it prints `BLOCKED` and `exit 3`, do not invent screenshots: use the BLOCKED form of the ruling in step 7.

- [ ] **Step 5: Look at the screenshots**

Use the Read tool on each PNG under `.superpowers/factory/screenshots/`. Write one finding per line in the handoff:

- Stage columns and the summary strip are visible on the desktop shot, with jobs in several stages, and the tokens and series chip from plan 07 show.
- The needs-human card shows its reason and the copy chips for `factory approve`. Review chips show text, not color only.
- At 400 px: no horizontal page scroll, the board scrolls inside its own container, text is not clipped.
- Muted text and the dimmed stale board are readable.
- The light shot differs from the dark one and is readable, or the NOTE line says it could not be exercised.

Fix only a clear defect (overflow, clipped text, unreadable contrast) with the smallest change in `tools/factory-server/static/dashboard.css`, rerun step 4 and look again. A defect that needs a redesign becomes a line in the open items list (step 7), not a fix.

- [ ] **Step 6: Close the dashboard render minors that plan 07 left**

Append these tests to `tools/factory-server/tests/board.test.js`, using its existing `makeState` and `makeJob` helpers. Run `node --test tools/factory-server/tests/*.test.js 2>&1 | grep -E '^not ok|^# (pass|fail)'`. For each test that already passes, keep it as a regression test. For each failing one, apply the smallest fix in `tools/factory-server/static/board.js` (a missing-timestamp guard `if (!Number.isFinite(ts)) return "--:--:--";` in `clock`, `state = state || {};` as the first line of each render function, and an `Unknown stage` column for jobs whose stage is not in `STAGES`).

```js
test("clock shows dashes for a missing or invalid timestamp", () => {
  assert.equal(board.clock(undefined), "--:--:--");
  assert.equal(board.clock("soon"), "--:--:--");
});

test("render functions accept a missing state", () => {
  assert.doesNotThrow(() => board.renderBoard(null));
  assert.doesNotThrow(() => board.renderFeed(null));
  assert.doesNotThrow(() => board.renderSummary(null));
});

test("a job with an unknown stage is listed, not only counted", () => {
  const html = board.renderBoard(makeState([makeJob({ id: "007", stage: "limbo" })]));
  assert.match(html, /data-job="007"/);
});

test("renderCard escapes slug, branch and reason", () => {
  const evil = "<img src=x onerror=alert(1)>";
  const html = board.renderCard(makeJob({ slug: evil, branch: evil, stage: "needs-human", reason: evil }));
  assert.doesNotMatch(html, /<img/);
});
```

Run the same node command again. Expected: `# fail 0`. If plan 07 renamed a render function or helper, use the new name. If `smoke.sh` and `seed-demo.sh` still have fixed sleeps or a dead helper, note them in the open items (step 7) instead of reworking them here.

- [ ] **Step 7: Write the doc changes in `.claude/software-factory.md`**

Re-read the file as it is now. Then:

1. Replace the `## Open items` list with only what is still true after plans 01 to 08. Remove: the builder id match, "dashboard look has not been checked in a browser" (unless BLOCKED), "`/verify-rocket` conflicts with the harness guard", and anything about the Python ctl or hook. Add these lines, plus one line per follow-up from step 5 or 6:

```markdown
- Dry run of a two-plan feature through `/factory` has not been run yet. The user runs it, see "Dry run recipe" below.
- Real-agent behavior (the actual `claude -p` flags, resume, permission mode) is only proven by that dry run. The scenario tests use `FakeRunner`.
- Light theme in a headless browser counts as checked only if the screenshot differs from the dark one.
```

2. Add a dated ruling in the rulings part (create `## Rulings` if absent). Use exactly one form.

If screenshots were taken:

```markdown
- Ruling (2026-10-13): the dashboard was checked in a headless `<browser name and version>` with `factory-shots` at 1440 px and 400 px, dark theme and an attempted light theme. Findings: <the step 5 findings in one sentence>. A human look in a real window is welcome but blocks nothing. The v1 minors are triaged in plan 08 of the v3 series: the Python ctl and hook minors were dropped with the deleted code.
```

If no browser was found:

```markdown
- Ruling (2026-10-13): BLOCKED. No headless browser was installed on the builder machine, so the dashboard look is unchecked. Manual steps: seed the demo board, start the dashboard, open it in a real window and check dark, light and a 400 px wide window against the list printed by `factory-shots`.
```

3. Add this section at the end:

````markdown
## Dry run recipe (run by the user)

Run this once after plans 01 to 08 are merged. It uses real agents, so it costs real tokens. Pick a small feature that needs two plans, for example "Add a per-request timeout setting and show it in the request tab".

1. In a fresh Claude Code session on `main` with a clean tree, start the dashboard: `cargo run -j4 --manifest-path tools/factory-server/Cargo.toml -- --root "$(pwd)" --port 8765`, and open http://127.0.0.1:8765/.
2. Run `/factory <the feature>`. In the session, brainstorm and approve a spec, then a two-plan series (plans end with `Next plan:` lines). Note the session usage line, this is the interactive cost.
3. When the command starts `factory run` in the background, leave the session idle and watch the dashboard. Expect, in order:
   - A job card in build with its worktree and the token counter rising.
   - A `checks.md` in the job folder (`ls .superpowers/sdd/*/checks.md`) with every check passing, or the builder resumed once after a failure.
   - One reviewer run, then the stage moves to approve and merged. A diff that touches a risk path (auth, `src-tauri/`, `Cargo.toml` and so on) parks in needs-human instead. Run `factory approve <id>` after reading the diff.
   - The merge commit has the message `chore(factory): merge job NNN <slug>` and its second parent is the reviewed sha (`git log --merges -1 --format=%P`).
   - Plan 2 starts by itself as the next job and merges the same way.
4. Read the cost: `factory tokens 001` and `factory tokens 002`. Each prints runs, raw and weighted tokens per agent role, and the percent of the job 001 weighted baseline. Add the two jobs and the interactive session to compare with the 2.17M weighted of job 001. Check that the reviewer role shows one run per clean round, and that no commit helper agents appear.
5. Pass criteria (the spec's success check): the interactive session was the only place the user spent tokens by hand and sat idle while the driver worked, the driver built in a worktree, ran the checks, ran one reviewer, merged the reviewed sha and started plan 2, the dashboard showed live stages and per-job tokens, no stage needed manual repair, `git status` is clean in the main checkout, and the logged weighted total is well below 2.17M (aim under 1.1M for a comparable small feature).
6. Clean up with `factory cleanup <id>` for each job and confirm `git branch --list 'factory/*'` shows nothing left.

If any step needs a manual action, note the step and add a follow-up row to `docs/roadmaps/roadmap.md`.
````

- [ ] **Step 8: Roadmap entry**

Read the last `R-` id in `docs/roadmaps/roadmap.md` (R-59 when this plan was written; use the next free number, called `R-NN` below, and check `grep -n 'R-6' docs/roadmaps/roadmap.md` first). Add this row after the last row of the quality and hygiene table that holds R-57 to R-59:

```markdown
| R-NN | Software factory v3 series (plans 01 to 08): Rust driver with `git2`, headless agents, checks before review, risk gate, budget, series, dashboard tokens, harness and verification | built on branch `worktree-software-factory`; dry run pending (user) | spec `docs/superpowers/specs/2026-10-13-software-factory-v3-rust-driver-design.md`, plans `docs/superpowers/plans/2026-10-13-software-factory-v3/`, design doc `.claude/software-factory.md` | Run the dry run recipe in `.claude/software-factory.md`, compare `factory tokens` with the job 001 baseline, then merge. Open follow-ups are in its open items list. |
```

Set the status text to what is true when you finish. Add one line to the Decisions log at the end of the file:

```markdown
- Ruling (2026-10-13): the factory v3 series closes with plan 08. `/verify-rocket` checks changed crates with `-j4` and no workspace command, a harness test lints the cargo commands in its doc and in `CLAUDE.md` through the real guard, and an end-to-end scenario test with `FakeRunner` covers the merge, resume, fix round, risk, sync and budget paths.
```

- [ ] **Step 9: Verify everything**

Run, one per line, and put each result in the handoff:

- `bash .claude/harness/tests/run.sh 2>&1 | tail -3` expected `ALL PASSED`.
- `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli` expected all pass.
- `cargo check -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli` expected clean.
- `node --test tools/factory-server/tests/*.test.js 2>&1 | grep -E '^# (pass|fail)'` expected `# fail 0`.
- `git status --short` expected only the files this plan names, plus untracked screenshots under `.superpowers/` which must not be added.

- [ ] **Step 10: Commit**

Invoke the skill `dev-workflow-skills:1-git-commit`, then:

```bash
git add tools/factory/crates/factory-cli/src/bin/factory-shots.rs tools/factory/crates/factory-cli/Cargo.toml tools/factory-server/tests/board.test.js .claude/software-factory.md docs/roadmaps/roadmap.md
git commit --only -m "docs(factory): record the browser check, add the dry run recipe and roadmap entry" -- tools/factory/crates/factory-cli/src/bin/factory-shots.rs tools/factory/crates/factory-cli/Cargo.toml tools/factory-server/tests/board.test.js .claude/software-factory.md docs/roadmaps/roadmap.md
```

Add `tools/factory-server/static/board.js`, `tools/factory-server/static/dashboard.css` and `tools/factory/Cargo.lock` to both lists if you changed them.

- [ ] **Step 11: Handoff**

The reply must contain: the result of each step 9 command, the screenshot findings (or BLOCKED with the manual steps), any engine bug the scenario tests found and fixed, the commit shas, and a one-line reminder that the dry run in `.claude/software-factory.md` is for the user to run.

---

## Self-review notes

- Spec coverage: decision 4 (check failure resumes builder, no reviewer spent) is scenario 1; decision 6 is scenario 3; decision 7 and fix rounds are scenario 2; decisions 9 and 10 are scenarios 5 and 4; decision 11 is scenario 1; the success check and token comparison are the `tokens` command, the baseline section and the dry run recipe. E10 is Task 3 steps 1 to 5, E11 is Task 1, E12 is the triage table plus Task 3 step 6.
- Names used across tasks: `UsageRecord`, `TokenReport`, `aggregate`, `render`, `load_usage`, `BASELINE`, `Scenario`, `Step`, `shot_args`, `pick_browser`, `blocked_message`, `free_port` are defined where first used and used the same way later.
- Three tasks, every cargo command uses `-j4`, none uses `--workspace`, no `.unwrap()` in the Rust shown.

Next plan: none (series complete)
