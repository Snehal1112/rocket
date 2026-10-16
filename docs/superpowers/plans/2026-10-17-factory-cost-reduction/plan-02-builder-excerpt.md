# Factory Task-Only Builder Prompts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give each per-task builder its own task text, the plan header, and the spec goal and acceptance criteria in its prompt, instead of making it read the whole `plan.md` and `spec.md`, behind the config switch `builder_excerpt` (default on, added by plan 1).

**Architecture:** Pure functions in `factory-engine/src/tasks.rs` cut the plan header, one `### Task N:` section and the spec excerpt out of the files. `builder_task_spec` in `specs.rs` embeds them in the prompt when `config.builder_excerpt` is on and the plan parses; otherwise it builds today's prompt. The builder agent file tells the agent to use the excerpt.

**Tech Stack:** Rust, crate `factory-engine`, the agent file `.claude/agents/factory-builder.md`. Work in `tools/factory/`.

**Spec:** `docs/superpowers/specs/2026-10-17-factory-cost-reduction-design.md` (part 2). Plan 1 (`plan-01-commit-batch.md`) must be merged first: it adds `Config.builder_excerpt`.

## Global Constraints

- Always `cargo ... -j4`, never `--workspace` or `--all`, never set `CARGO_TARGET_DIR`. Run from `tools/factory/`.
- No panicking unwrap or expect calls in production Rust (tests may use `expect`).
- Comments are short full sentences ending with a period. No emojis. Conventional commit messages. Commit through the `dev-workflow-skills:1-git-commit` skill, with explicit paths.
- Only the single builder "Start" step of `.claude/agents/factory-builder.md` changes (plus the matching "Work" line). The system prompt, tools and other agent instructions are not slimmed.
- `builder_excerpt = false` must give today's prompt byte for byte. Whole-plan builders (no `### Task N:` headings), fix resumes and rework resumes are unchanged.
- The full `plan.md` and `spec.md` paths stay in the prompt, so the builder can open them when the task refers to something outside the excerpt.

## Review Focus

- A plan whose tasks are `- [ ] Task N:` checkbox lines (no `### Task` headings) must fall back to the full prompt, not produce an empty excerpt.
- Task 1 of a plan must not contain the text of Task 2, and the last task must not include the `Next plan:` pointer line.
- A spec without an acceptance section must still give the goal; an empty spec gives an empty excerpt, and the prompt then falls back to naming the files only.
- A task body that contains a line starting with `### ` that is not a task heading (for example `### Step`) must stay inside its task.
- Task numbers that are not consecutive (Task 3 then Task 5) must still find the right section.

---

### Task 1: Excerpt functions

**Files:**
- Modify: `tools/factory/crates/factory-engine/src/tasks.rs`

**Interfaces:**
- Consumes: the existing `parse_tasks`, `spec_goal` in the same file.
- Produces:
  - `pub fn plan_header(plan: &str) -> String`: the text before the first `### Task` heading, trimmed.
  - `pub fn task_section(plan: &str, n: u32) -> Option<String>`: the `### Task N: ...` heading line and its body up to the next `### Task` heading, a line starting with `Next plan:`, or the end; `None` when no `### Task N:` heading exists.
  - `pub fn spec_excerpt(spec: &str) -> String`: the goal line plus the acceptance criteria section.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module of `tasks.rs`:

```rust
    const PLAN: &str = "# Demo Plan\n\n**Goal:** make it.\n\n## Global Constraints\n\n- Use -j4.\n\n---\n\n\
### Task 1: first thing\n\nbody one\n\n### Step 2: not a task\n\nstill one\n\n\
### Task 2: second thing\n\nbody two\n\nNext plan: none\n";

    #[test]
    fn the_header_is_everything_before_the_first_task() {
        let header = plan_header(PLAN);
        assert!(header.starts_with("# Demo Plan"), "{header}");
        assert!(header.contains("Global Constraints") && header.contains("Use -j4."));
        assert!(!header.contains("Task 1"));
    }

    #[test]
    fn a_task_section_stops_at_the_next_task_and_keeps_inner_headings() {
        let one = task_section(PLAN, 1).expect("task 1");
        assert!(one.starts_with("### Task 1: first thing"), "{one}");
        assert!(one.contains("body one") && one.contains("### Step 2: not a task") && one.contains("still one"));
        assert!(!one.contains("body two") && !one.contains("Task 2"));
    }

    #[test]
    fn the_last_task_section_drops_the_next_plan_pointer() {
        let two = task_section(PLAN, 2).expect("task 2");
        assert!(two.contains("body two") && !two.contains("Next plan"), "{two}");
    }

    #[test]
    fn task_numbers_need_not_be_consecutive_and_missing_ones_give_none() {
        let plan = "# P\n\n### Task 3: three\n\nc\n\n### Task 5: five\n\ne\n";
        assert!(task_section(plan, 5).expect("five").contains('e'));
        assert!(!task_section(plan, 3).expect("three").contains("five"));
        assert!(task_section(plan, 4).is_none());
        assert!(task_section("- [ ] Task 1: checkbox only\n", 1).is_none());
    }

    #[test]
    fn the_spec_excerpt_has_the_goal_and_the_acceptance_section_only() {
        let spec = "# Spec\n\nGoal: ship the thing quickly.\n\n## Background\n\nlong text\n\n\
## Acceptance criteria\n\n- it works\n- it is fast\n\n## Testing\n\nmore text\n";
        let ex = spec_excerpt(spec);
        assert!(ex.contains("Goal: ship the thing quickly."), "{ex}");
        assert!(ex.contains("Acceptance criteria") && ex.contains("- it is fast"));
        assert!(!ex.contains("long text") && !ex.contains("more text"));
    }

    #[test]
    fn a_spec_without_acceptance_criteria_gives_the_goal_and_an_empty_spec_gives_nothing() {
        assert_eq!(spec_excerpt("Goal: only this goal here.\n"), "Goal: only this goal here.");
        assert_eq!(spec_excerpt(""), "");
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cd tools/factory && cargo test -j4 -p factory-engine --lib tasks::tests`
Expected: compile errors, the three functions do not exist.

- [ ] **Step 3: Implement**

Add above the `tests` module in `tasks.rs`:

```rust
/// True for a `### Task N:` heading line.
fn is_task_heading(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("### Task ") else { return false };
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    !digits.is_empty() && rest[digits.len()..].starts_with(':')
}

/// The text of a plan before its first `### Task` heading, trimmed. The whole plan when it has no such heading.
pub fn plan_header(plan: &str) -> String {
    let end = plan.lines().position(is_task_heading).map_or(plan.lines().count(), |i| i);
    plan.lines().take(end).collect::<Vec<_>>().join("\n").trim().to_string()
}

/// The heading and body of `### Task N:`, up to the next task heading, a `Next plan:` line or the end.
/// `None` when the plan has no such heading. Inner headings such as `### Step` stay in the section.
pub fn task_section(plan: &str, n: u32) -> Option<String> {
    let mut lines = plan.lines().skip_while(|line| {
        !(is_task_heading(line) && line.strip_prefix("### Task ").is_some_and(|r| r.starts_with(&format!("{n}:"))))
    });
    let heading = lines.next()?;
    let mut out = vec![heading];
    for line in lines {
        if is_task_heading(line) || line.trim_start().starts_with("Next plan:") {
            break;
        }
        out.push(line);
    }
    Some(out.join("\n").trim().to_string())
}

/// The goal of a spec plus its `Acceptance criteria` section (a `## ` heading containing those words,
/// up to the next `## ` heading). Empty for an empty spec.
pub fn spec_excerpt(spec: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    let goal = spec_goal(spec);
    let goal_line = spec.lines().map(str::trim).find(|l| l.len() >= 5 && l.is_char_boundary(5) && l[..5].eq_ignore_ascii_case("goal:"));
    if let Some(line) = goal_line {
        parts.push(line.to_string());
    } else if !goal.is_empty() {
        parts.push(format!("Goal: {goal}"));
    }
    let mut lines = spec
        .lines()
        .skip_while(|l| !(l.starts_with("## ") && l.to_ascii_lowercase().contains("acceptance criteria")));
    if let Some(heading) = lines.next() {
        let mut section = vec![heading];
        for line in lines {
            if line.starts_with("## ") {
                break;
            }
            section.push(line);
        }
        parts.push(section.join("\n").trim().to_string());
    }
    parts.join("\n\n")
}
```

The first test expects `spec_excerpt("Goal: only this goal here.\n")` to equal the same line without the newline. In that input `goal_line` finds `Goal: only this goal here.` (trimmed), so it passes. For a spec whose first plain line is not a `Goal:` line, `spec_goal` supplies the text and the excerpt prefixes `Goal: `.

- [ ] **Step 4: Run the tests**

Run: `cd tools/factory && cargo test -j4 -p factory-engine --lib tasks::tests`
Expected: pass. If `the_header_is_everything_before_the_first_task` fails because `plan_header` returns `""` for a plan that has no task heading, that is wrong: the `map_or` above returns the full line count, so it returns the whole plan. Fix the code, not the test.

- [ ] **Step 5: Commit**

Invoke `dev-workflow-skills:1-git-commit` with the path `tools/factory/crates/factory-engine/src/tasks.rs`. Suggested message: `feat(factory): cut a task excerpt from plan and spec`.

---

### Task 2: Excerpt prompt, agent file, docs

**Files:**
- Modify: `tools/factory/crates/factory-engine/src/specs.rs`
- Modify: `tools/factory/crates/factory-engine/tests/build_checks.rs`
- Modify: `.claude/agents/factory-builder.md`
- Modify: `.claude/software-factory.md`

**Interfaces:**
- Consumes: Task 1 functions, `Config.builder_excerpt` (plan 1).
- Produces: `builder_task_spec` embeds the excerpt when `config.builder_excerpt` is on and `task_section` finds the task; otherwise today's prompt.

- [ ] **Step 1: Write the failing engine tests**

Append to `tests/build_checks.rs`:

```rust
const EXCERPT_PLAN: &str = "# Demo Plan\n\n**Goal:** make it.\n\n## Global Constraints\n\n- Use -j4.\n\n---\n\n\
### Task 1: first thing\n\nBODY-ONE-MARKER\n\n### Task 2: second thing\n\nBODY-TWO-MARKER\n\nNext plan: none\n";
const EXCERPT_SPEC: &str = "# Spec\n\nGoal: ship the thing quickly.\n\n## Background\n\nBACKGROUND-MARKER\n\n\
## Acceptance criteria\n\n- ACCEPT-MARKER\n\n## Testing\n\nTESTING-MARKER\n";

fn excerpt_fixture() -> Fixture {
    let fx = committing_fixture();
    std::fs::write(&fx.plan_file, EXCERPT_PLAN).expect("plan");
    std::fs::write(fx.job_dir.join("spec.md"), EXCERPT_SPEC).expect("spec");
    let mut board = factory_core::Board::load(&fx.board_path).expect("load");
    board.find_mut("001").expect("job").set_stage(Stage::Build, None, false, 1.0).expect("stage");
    board.save(&fx.board_path).expect("save");
    fx
}

#[test]
fn a_task_builder_gets_its_own_task_the_header_and_the_spec_excerpt() {
    let fx = excerpt_fixture();
    let runner = scripted_auto(vec![change_effect("a.txt", 3)]);
    fx.engine(&runner).build_stage().expect("task 1");
    let prompt = runner.last_spec_for("builder").expect("spec").prompt;
    for want in ["Use -j4.", "BODY-ONE-MARKER", "Goal: ship the thing quickly.", "ACCEPT-MARKER", "Do only Task 1"] {
        assert!(prompt.contains(want), "missing {want}: {prompt}");
    }
    for gone in ["BODY-TWO-MARKER", "BACKGROUND-MARKER", "TESTING-MARKER"] {
        assert!(!prompt.contains(gone), "unexpected {gone}: {prompt}");
    }
    assert!(prompt.contains(&fx.plan_file.display().to_string()), "{prompt}");
    assert!(prompt.contains("spec.md"), "{prompt}");
}

#[test]
fn the_flag_off_gives_the_old_prompt_without_the_excerpt() {
    let mut fx = excerpt_fixture();
    fx.config.builder_excerpt = false;
    let runner = scripted_auto(vec![change_effect("a.txt", 3)]);
    fx.engine(&runner).build_stage().expect("task 1");
    let prompt = runner.last_spec_for("builder").expect("spec").prompt;
    assert!(!prompt.contains("BODY-ONE-MARKER") && !prompt.contains("ACCEPT-MARKER"), "{prompt}");
    assert!(prompt.contains("Do only Task 1"));
}

#[test]
fn a_plan_with_checkbox_tasks_falls_back_to_the_old_prompt() {
    let fx = committing_fixture();
    std::fs::write(&fx.plan_file, "# P\n\n- [ ] Task 1: first\n\nBODY-ONE-MARKER\n- [ ] Task 2: second\n").expect("plan");
    let mut board = factory_core::Board::load(&fx.board_path).expect("load");
    board.find_mut("001").expect("job").set_stage(Stage::Build, None, false, 1.0).expect("stage");
    board.save(&fx.board_path).expect("save");
    let runner = scripted_auto(vec![change_effect("a.txt", 3)]);
    fx.engine(&runner).build_stage().expect("task 1");
    let prompt = runner.last_spec_for("builder").expect("spec").prompt;
    assert!(!prompt.contains("BODY-ONE-MARKER"), "{prompt}");
    assert!(prompt.contains("Do only Task 1"));
}

#[test]
fn the_excerpt_prompt_is_smaller_than_the_plan_and_spec_it_replaces() {
    let big_task = format!("### Task 2: second thing\n\n{}\n", "filler line\n".repeat(400));
    let plan = format!("# Demo Plan\n\n- Use -j4.\n\n### Task 1: first thing\n\nbody\n\n{big_task}");
    let fx = excerpt_fixture();
    std::fs::write(&fx.plan_file, &plan).expect("plan");
    let runner = scripted_auto(vec![change_effect("a.txt", 3)]);
    fx.engine(&runner).build_stage().expect("task 1");
    let prompt = runner.last_spec_for("builder").expect("spec").prompt;
    // The old prompt only named the files and the builder read them in full, about plan + spec bytes.
    let replaced = plan.len() + EXCERPT_SPEC.len();
    assert!(prompt.len() < replaced / 2, "prompt {} bytes, plan and spec {replaced} bytes", prompt.len());
    assert!(!prompt.contains("filler line"));
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cd tools/factory && cargo test -j4 -p factory-engine --test build_checks excerpt`
Expected: the first test fails (no markers in the prompt); the others may already pass.

- [ ] **Step 3: Implement the prompt**

In `specs.rs` import the helpers: `use crate::tasks::{plan_header, spec_excerpt, task_section, PlanTask};`. In `builder_task_spec`, build an `excerpt` string before the `format!` and add it to the prompt:

```rust
        let excerpt = self.task_excerpt(task);
```

Change the second and third line of the prompt text (the `The plan is at ...` line) so the excerpt follows it. Replace

```
             The plan is at {plan}. Never edit the plan.\n{spec_line}\
```

with

```
             The plan is at {plan}. Never edit the plan.\n{spec_line}{excerpt}\
```

Add the method (inside the first `impl Engine<'_>` block):

```rust
    /// The task excerpt for a builder prompt, or an empty string when the switch is off or the plan has no
    /// `### Task N:` section for the task. It replaces reading `plan.md` and `spec.md` in full.
    fn task_excerpt(&self, task: &PlanTask) -> String {
        if !self.config.builder_excerpt {
            return String::new();
        }
        let plan = std::fs::read_to_string(&self.plan_file).unwrap_or_default();
        let Some(section) = task_section(&plan, task.n) else { return String::new() };
        let header = plan_header(&plan);
        let spec = std::fs::read_to_string(self.job_dir.join("spec.md")).unwrap_or_default();
        let spec_part = spec_excerpt(&spec);
        let mut out = String::from(
            "Task excerpt: the plan header, your task, and the spec goal and acceptance criteria follow. \
             They replace reading plan.md and spec.md in full. Open those files only for something this excerpt refers to outside itself.\n\
             --- plan header ---\n",
        );
        out.push_str(&header);
        out.push_str("\n--- your task ---\n");
        out.push_str(&section);
        if !spec_part.is_empty() {
            out.push_str("\n--- spec excerpt ---\n");
            out.push_str(&spec_part);
        }
        out.push_str("\n--- end of excerpt ---\n");
        out
    }
```

- [ ] **Step 4: Run the new tests**

Run: `cd tools/factory && cargo test -j4 -p factory-engine --test build_checks`
Expected: pass. Re-run the whole `factory-engine` suite; existing tests that assert exact builder prompt text keep passing because a plan like `TWO_TASKS` now produces an excerpt, which only adds text. If one asserts the absence of text that the excerpt now adds, set `fx.config.builder_excerpt = false` in that test and say so in the commit body.

- [ ] **Step 5: Edit the builder agent file**

In `.claude/agents/factory-builder.md`, replace Start step 3, `Read the plan and the spec once. Never edit them.`, with:

```
3. If your prompt has a "Task excerpt" section, it is your plan and spec: do not read `plan.md` or `spec.md` in full, and open them only for something the excerpt refers to outside itself. Otherwise read the plan and the spec once. Never edit them.
```

In the Work section, change the first bullet's opening so it reads `Invoke \`superpowers:executing-plans\` and follow it, treating the task excerpt in your prompt as the plan when there is one.` and keep the rest of the bullet. Then run `cd tools/factory && cargo test -j4 -p factory-cli --test agent_lint`. Expected: pass. If the lint rejects a phrase, reword without changing the meaning.

- [ ] **Step 6: Update the docs**

In `.claude/software-factory.md`, section "Per-task builders": add a bullet that with `builder_excerpt` on (default) the task prompt carries the plan header, the task's own `### Task N:` section and the spec goal and acceptance criteria, names the full files for reference, falls back to the full prompt when the plan has no `### Task N:` heading for the task, and that `builder_excerpt = false` restores the old prompt. Also list both switches (`commit_batch`, `builder_excerpt`) in the config key list of that file if it has one.

- [ ] **Step 7: Run everything once**

Run: `cd tools/factory && cargo test -j4 -p factory-core && cargo test -j4 -p factory-git && cargo test -j4 -p factory-engine && cargo test -j4 -p factory-cli && cargo check -j4`
Expected: all pass.

- [ ] **Step 8: Commit**

Invoke `dev-workflow-skills:1-git-commit` with the explicit paths of the files above. Suggested message: `feat(factory): give task builders only their task`.
