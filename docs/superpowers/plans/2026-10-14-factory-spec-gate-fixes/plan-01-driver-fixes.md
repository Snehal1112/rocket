# Factory spec gate: driver fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the three driver defects the 2026-10-14 factory audit found in the uncommitted `--spec` and `check_spec` work, so `/factory` can rely on a spec gate that never rejects a good spec and never silently skips a spec.

**Architecture:** All changes are in the `tools/factory` workspace. `create_job_with_spec` in `factory-engine/src/runloop.rs` gets a guard for `--spec` without `--plans`, a heading- and fence-aware `section_body`, and a single validated read of the spec that is written to every job folder. The CLI marks `--spec` as requiring `--plans`.

**Tech Stack:** Rust (workspace at `tools/factory`, its own `Cargo.toml`), clap, the existing integration tests in `factory-engine/tests/runloop.rs`.

**Spec:** The approved "Group A" list from the 2026-10-14 factory audit, reproduced as the Requirements below. This is a bounded fix to existing code, so no separate design doc exists. The related design is `docs/superpowers/specs/2026-10-13-software-factory-v3-rust-driver-design.md`.

## Requirements (from the approved Group A list)

1. `factory new … --spec <file>` without `--plans` must fail with a clear error. Today the job is created with no spec check and no spec copy, and the exit code is 0.
2. `section_body` must end a section only at a level-1 or level-2 heading outside code fences. Deeper headings (`### Why`), `# comment` lines inside fenced code and a line starting with `#123` must stay inside the section. Only an exact `## <title>` line names a section.
3. The spec text that passed `check_spec` must be the text written into each job folder. No second read of the file.

## Global Constraints

- Cargo for this workspace: `cargo <cmd> -j4 --manifest-path tools/factory/Cargo.toml`. Never `--workspace` or `--all`.
- No panicking shortcuts (unwrap-style calls) in non-test code. Tests use `.expect("...")`. A repo hook blocks any `.rs` or `.md` edit that contains the call-with-parentheses spelling, so do not write that spelling anywhere.
- Do not reformat files you do not change. The factory crates are not rustfmt-formatted, so do not run `cargo fmt`.
- Existing tests keep passing without edits, except where a step says to change one.
- Behavior of `create_job` (no `--spec`) must not change.

## Review Focus

- `### Why` under `## Goal` must not shorten the goal (pinned by a test in Task 2).
- A `# comment` line inside a fenced block under `## Acceptance criteria` must not hide a later bullet (Task 2).
- `### Goal`, `#### Goal`, `## Goal:` and `## Goals` must be rejected as "no Goal section" (Task 2).
- `--spec` with no plans must create no job and print an error (Task 1).
- The copied `spec.md` must equal the validated text byte for byte, also for a CRLF file (Task 3).

---

### Task 1: `--spec` requires `--plans`

**Files:**
- Modify: `tools/factory/crates/factory-cli/src/cli.rs`
- Modify: `tools/factory/crates/factory-engine/src/runloop.rs` (top of `create_job_with_spec`)
- Test: `tools/factory/crates/factory-engine/tests/runloop.rs`

**Interfaces:**
- Produces: `create_job_with_spec(driver, slug, feature, None, Some(spec), model)` returns `Err(EngineError::Msg("--spec needs --plans"))` and creates no job.

- [ ] **Step 1: Write the failing test**

Append to `tools/factory/crates/factory-engine/tests/runloop.rs`:

```rust
#[test]
fn a_spec_without_plans_is_an_error_and_creates_no_job() {
    let fx = common::SeriesFixture::new("x");
    let a = fx.plan_file("plan-01-a.md", 1, "none");
    let spec = a.parent().expect("plans folder").join("alone.md");
    std::fs::write(&spec, GOOD_SPEC).expect("spec");
    let result = create_job_with_spec(&fx.driver, "alone", "f", None, Some(&spec), None);
    assert!(result.is_err(), "a spec with no plans must not be ignored");
    assert!(fx.driver.store().load().expect("board").jobs.is_empty());
}
```

- [ ] **Step 2: Run it and see it fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test runloop a_spec_without_plans`
Expected: FAIL, the call returns `Ok` and a job exists.

- [ ] **Step 3: Add the engine guard**

In `create_job_with_spec` in `tools/factory/crates/factory-engine/src/runloop.rs`, add this as the first statement of the function body, before `let Some(start) = plan else {`:

```rust
    // A spec with no plans would be skipped without a word, so refuse it.
    if spec_path.is_some() && plan.is_none() {
        return Err(EngineError::Msg("--spec needs --plans".to_string()));
    }
```

- [ ] **Step 4: Require `--plans` in clap**

In `tools/factory/crates/factory-cli/src/cli.rs`, change the `spec` field of `Command::New`:

```rust
        /// The spec file, for example one committed under docs/superpowers/specs/. Without it a plans
        /// folder must hold spec.md. It needs --plans.
        #[arg(long, requires = "plans")]
        spec: Option<PathBuf>,
```

- [ ] **Step 5: Run the test and the CLI check**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test runloop a_spec_without_plans`
Expected: PASS.

Run: `cargo build -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli && tools/factory/target/debug/factory new x f --spec docs/superpowers/specs/2026-10-14-tree-walk-fewer-stat-calls-design.md; echo "exit=$?"`
Expected: clap prints `the following required arguments were not provided: --plans` and `exit=2`.

---

### Task 2: Heading- and fence-aware `section_body`

**Files:**
- Modify: `tools/factory/crates/factory-engine/src/runloop.rs` (`section_body`)
- Test: `tools/factory/crates/factory-engine/tests/runloop.rs`

**Interfaces:**
- Produces: `section_body(spec: &str, title: &str) -> Option<String>` keeps its signature. A new private `heading_level(line: &str) -> usize` returns the number of leading `#` when they are followed by a space, else 0.

- [ ] **Step 1: Write the failing tests**

Append to `tools/factory/crates/factory-engine/tests/runloop.rs`:

```rust
#[test]
fn a_subheading_or_a_code_comment_does_not_cut_a_section_short() {
    let fx = common::SeriesFixture::new("x");
    let a = fx.plan_file("plan-01-a.md", 1, "none");
    let folder = a.parent().expect("plans folder").to_path_buf();
    let spec = "# Spec: Demo\n\n## Goal\n\nShip it.\n\n### Why\n\nBuild the demo so that a user can finish the task in one step.\n\n\
                ## Acceptance criteria\n\n```bash\n# run the suite\ncargo test\n```\n\n- It is done.\n";
    std::fs::write(folder.join("spec.md"), spec).expect("spec");
    create_job(&fx.driver, "deep", "f", Some(&folder), None).expect("sub-headings and code comments stay in the section");
}

#[test]
fn only_an_exact_level_two_heading_names_a_section() {
    let body = "Build the demo so that a user can finish the task in one step.";
    for heading in ["### Goal", "#### Goal", "## Goal:", "## Goals", "##Goal"] {
        let fx = common::SeriesFixture::new("x");
        let a = fx.plan_file("plan-01-a.md", 1, "none");
        let folder = a.parent().expect("plans folder").to_path_buf();
        let spec = format!("# Spec: Demo\n\n{heading}\n\n{body}\n\n## Acceptance criteria\n\n- ok\n");
        std::fs::write(folder.join("spec.md"), spec).expect("spec");
        assert!(create_job(&fx.driver, "bad", "f", Some(&folder), None).is_err(), "{heading}");
    }
}
```

- [ ] **Step 2: Run them and see the failures**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test runloop a_subheading only_an_exact`
Expected: `a_subheading_or_a_code_comment_does_not_cut_a_section_short` FAILS (the goal is cut at `### Why`), and `only_an_exact_level_two_heading_names_a_section` FAILS for `### Goal` and `#### Goal`.

- [ ] **Step 3: Replace `section_body`**

In `tools/factory/crates/factory-engine/src/runloop.rs`, replace the whole `section_body` function, from its doc comment `/// The text of a section` through its closing brace, with:

```rust
/// The number of leading `#` of a heading line (`## Goal` is 2), or 0 for any other line.
fn heading_level(line: &str) -> usize {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if hashes > 0 && line[hashes..].starts_with(' ') {
        hashes
    } else {
        0
    }
}

/// The text of a section: the lines under the `## <title>` line up to the next heading of level 1 or 2.
/// Deeper headings and lines inside fenced code belong to the section. None when there is no such heading.
fn section_body(spec: &str, title: &str) -> Option<String> {
    let mut in_fence = false;
    let mut in_section = false;
    let mut found = false;
    let mut body: Vec<&str> = Vec::new();
    for line in spec.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            if in_section {
                body.push(line);
            }
            continue;
        }
        let level = if in_fence { 0 } else { heading_level(line) };
        if in_section {
            if level == 1 || level == 2 {
                break;
            }
            body.push(line);
        } else if level == 2 && line[2..].trim().eq_ignore_ascii_case(title) {
            in_section = true;
            found = true;
        }
    }
    found.then(|| body.join("\n"))
}
```

- [ ] **Step 4: Run all runloop tests**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test runloop`
Expected: all pass, including the earlier bad-spec cases (`a_folder_without_a_usable_spec_creates_no_jobs`).

- [ ] **Step 5: Clippy**

Run: `cargo clippy -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --all-targets -- -D warnings`
Expected: no warnings.

---

### Task 3: The validated text is the copied text

**Files:**
- Modify: `tools/factory/crates/factory-engine/src/runloop.rs` (`create_job_with_spec`)
- Test: `tools/factory/crates/factory-engine/tests/runloop.rs`

- [ ] **Step 1: Write the test**

Append to `tools/factory/crates/factory-engine/tests/runloop.rs`:

```rust
#[test]
fn the_copied_spec_is_the_checked_spec_with_a_crlf_file_too() {
    let fx = common::SeriesFixture::new("x");
    let a = fx.plan_file("plan-01-a.md", 1, "none");
    let folder = a.parent().expect("plans folder").to_path_buf();
    let crlf = GOOD_SPEC.replace('\n', "\r\n");
    std::fs::write(folder.join("spec.md"), &crlf).expect("spec");
    create_job(&fx.driver, "crlf", "f", Some(&folder), None).expect("a CRLF spec passes");
    let board = fx.driver.store().load().expect("board");
    let copy = std::fs::read_to_string(fx.driver.job_dir(&board.jobs[0]).join("spec.md")).expect("copy");
    assert_eq!(copy, crlf);
}
```

- [ ] **Step 2: Run it**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine --test runloop the_copied_spec`
Expected: PASS already (it pins today's behavior before the refactor).

- [ ] **Step 3: Keep the validated text and write it**

In `create_job_with_spec`, replace the `has_spec` block (from `let has_spec =` through the closing brace of `if has_spec { … }`) with:

```rust
    let has_spec = spec_path.is_some() || start.is_dir();
    // The text that passes the check is the text every job gets, so the file is read once.
    let mut spec_text: Option<String> = None;
    if has_spec {
        let text = std::fs::read_to_string(&spec).map_err(|_| {
            if spec_path.is_some() {
                EngineError::Msg(format!("the spec file {} cannot be read", spec.display()))
            } else {
                EngineError::Msg("the plans folder has no spec.md".to_string())
            }
        })?;
        check_spec(&text)?;
        spec_text = Some(text);
    }
```

Then replace the `copy_spec` closure with:

```rust
    let copy_spec = |job: &Job| -> Result<(), EngineError> {
        if let Some(text) = &spec_text {
            std::fs::write(driver.job_dir(job).join("spec.md"), text)?;
        }
        Ok(())
    };
```

- [ ] **Step 4: Run the whole factory suite and clippy**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-engine -p factory-cli`
Expected: all pass.

Run: `cargo clippy -j4 --manifest-path tools/factory/Cargo.toml --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

Stage only the five driver files and commit them through the `dev-workflow-skills:1-git-commit` skill, with a pathspec:

```bash
git add tools/factory/crates/factory-cli/src/cli.rs tools/factory/crates/factory-cli/src/commands.rs tools/factory/crates/factory-engine/src/lib.rs tools/factory/crates/factory-engine/src/runloop.rs tools/factory/crates/factory-engine/tests/runloop.rs
git diff --cached --name-only
```
Expected: exactly those five paths. Commit as a new commit, `feat(factory): add --spec and a stricter spec check`, unless the user chose to amend `62a792f2`.

Next plan: docs/superpowers/plans/2026-10-14-factory-spec-gate-fixes/plan-02-command-docs-lint.md
