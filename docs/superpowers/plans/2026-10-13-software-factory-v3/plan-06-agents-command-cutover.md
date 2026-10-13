# Software Factory v3, Plan 06: Agents, thin command, cutover Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task, executed by a `factory-builder` subagent. Skip that skill's final whole-branch review, its ledger scripts and its progress file. Steps use checkbox (`- [ ]`) syntax for tracking. Plans 01 to 05 must be finished and merged first, because this plan uses the built `factory` binary and the `factory-cli` crate.

**Goal:** Cut the factory over from the Python and LLM-orchestrator design to the Rust driver: headless-ready agent files, a thin `/factory` command, the hook and the Python helper removed, docs rewritten.

**Architecture:** The driver (`tools/factory/`) now owns state, checks, merge, budget and events. The two remaining agents (`factory-builder`, `factory-code-reviewer`) are plain prompts the driver starts with `claude -p`. The `/factory` command only brainstorms and plans with the user, then runs `factory run <id> --series` in the background. Lint tests written in Rust pin the agent files, the command, and the cutover so they cannot drift.

**Tech Stack:** Markdown agent and command files, JSON settings, Rust integration tests in `factory-cli` (std only, no new crates).

**Spec:** `docs/superpowers/specs/2026-10-13-software-factory-v3-rust-driver-design.md` (read it fully first, especially decisions 2, 4, 5, 6, 8, 9, 12 and 13).

## Global Constraints

- Rust workspace at `tools/factory/` (own workspace, own target dir, not in the root workspace). No new crates are needed. Use `--manifest-path tools/factory/Cargo.toml`.
- Never call unwrap anywhere in Rust (a harness hook blocks it). Tests return `Result<(), String>` and use `?` and `map_err`.
- Cargo commands always pass `-j4`. Never `--workspace`, never `--all`, never set `CARGO_TARGET_DIR`.
- Comments: short full sentences ending in a period. No emojis.
- Commit steps are pathspec commits: `git add <paths> && git commit --only -m "..." -- <paths>`, conventional message. Invoke the skill `dev-workflow-skills:1-git-commit` for the message, but run its git commands inline, with no helper agents. Never `git add -A` or `git add .`. Never amend.
- Agent file frontmatter descriptions must not contain a colon followed by a space.
- Only the driver writes the ledger, board and events. Agents and the command never do.
- Do NOT edit the memory note `/home/numericlabs/.claude/projects/-home-numericlabs-data-rocket-rocket/memory/project_software_factory.md`. The caller updates it after the merge.
- Docs go under `.claude/`, root `CLAUDE.md` only gets a one-line pointer.

## Review Focus

Spec-implied failure modes that no task's main tests would otherwise exercise, most likely first:

1. A feature text that starts with a subcommand word, for example `status page for jobs`, `approve button for forms`, `cancel dialog 12 layout`, `next button for pagination`. Expected: it stays a feature. Pinned in T2 by the lint test on the exact-shape rule text.
2. A feature text with quotes, `$`, backticks or newlines. Expected: the command passes it to `factory new` safely escaped, never as raw shell. Pinned in T2.
3. `rework <id>` with no note. Expected: usage line, no `factory` call. Pinned in T2.
4. The `factory` binary is not built yet. Expected: the command builds it once with `cargo build -j4` and then continues. Pinned in T2 and in the settings allow list.
5. A plan with more than 3 tasks, or without the final `Next plan:` line, reaching `factory run`. Expected: the command validates before running. Pinned in T2.
6. An old ctl capability with no Rust test that covers it. Expected: nothing is deleted and the builder reports BLOCKED. Enforced by the parity step in T3.

---

## File Structure

- Rewrite: `.claude/agents/factory-builder.md`, `.claude/agents/factory-code-reviewer.md`
- Delete: `.claude/agents/factory-spec-writer.md`, `factory-security-reviewer.md`, `factory-ux-reviewer.md`, `factory-ui-reviewer.md`, `factory-approver.md`
- Rewrite: `.claude/commands/factory.md`
- Modify: `.claude/settings.json` (add allow list in T2, remove three hook entries in T3)
- Create: `tools/factory/crates/factory-cli/tests/agent_lint.rs` (confirm the crate directory with `ls tools/factory`; if plan 01 used a different layout, put the file under that crate's `tests/`)
- Delete (T3, after parity): `.claude/factory/factory-ctl`, `.claude/factory/hooks/log-event`, `.claude/factory/tests/run.sh`
- Rewrite: `.claude/software-factory.md`
- Modify: `CLAUDE.md` (one line), `tools/factory-server/README.md` (run line and source list), `docs/superpowers/specs/2026-10-13-software-factory-v2-design.md` (status line), `docs/superpowers/plans/2026-10-13-software-factory-v2/00-plan-index.md` (note at top)

---

### Task 1: Headless agent files and their lint test

**Files:**
- Create: `tools/factory/crates/factory-cli/tests/agent_lint.rs`
- Rewrite: `.claude/agents/factory-builder.md`, `.claude/agents/factory-code-reviewer.md`
- Delete: the five other `.claude/agents/factory-*.md` files

**Interfaces:**
- Consumes: nothing from code. The driver (plans 03 to 05) starts these agents headless inside the job worktree and passes the model from config, so neither file has a `model:` line.
- Produces: the two agent files, and `tests/agent_lint.rs` with helpers `repo_root`, `read`, `frontmatter`, `fm_get`, `require_all`, `forbid_all`, `has_line` that T2 and T3 extend.

- [ ] **Step 1: Write the failing lint test**

Create `tools/factory/crates/factory-cli/tests/agent_lint.rs`:

```rust
//! Lint tests for the factory agent files, the /factory command and the cutover.
//! They read the repo files and pin the rules the driver relies on.

use std::fs;
use std::path::PathBuf;

type TestResult = Result<(), String>;

/// Walks up from the crate until a folder with `.claude/agents` is found.
fn repo_root() -> Result<PathBuf, String> {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join(".claude").join("agents").is_dir() {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err("repo root with .claude/agents not found".to_string());
        }
    }
}

fn read(rel: &str) -> Result<String, String> {
    let path = repo_root()?.join(rel);
    fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Parses the key and value lines between the two `---` markers.
fn frontmatter(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut lines = text.lines();
    if lines.next() != Some("---") {
        return Err("missing frontmatter".to_string());
    }
    let mut out = Vec::new();
    for line in lines {
        if line == "---" {
            return Ok(out);
        }
        if let Some((key, value)) = line.split_once(':') {
            out.push((key.trim().to_string(), value.trim().to_string()));
        }
    }
    Err("unterminated frontmatter".to_string())
}

fn fm_get<'a>(fm: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fm.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn require_all(text: &str, label: &str, phrases: &[&str]) -> TestResult {
    let missing: Vec<&&str> = phrases.iter().filter(|p| !text.contains(**p)).collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("{label} is missing: {missing:?}"))
    }
}

fn forbid_all(text: &str, label: &str, phrases: &[&str]) -> TestResult {
    let found: Vec<&&str> = phrases.iter().filter(|p| text.contains(**p)).collect();
    if found.is_empty() {
        Ok(())
    } else {
        Err(format!("{label} must not contain: {found:?}"))
    }
}

fn has_line(text: &str, line: &str) -> bool {
    text.lines().any(|l| l == line)
}

fn check_max_turns(fm: &[(String, String)], label: &str) -> TestResult {
    let raw = fm_get(fm, "maxTurns").ok_or(format!("{label}: maxTurns missing"))?;
    let turns: u32 = raw.parse().map_err(|_| format!("{label}: maxTurns not a number"))?;
    if (1..=400).contains(&turns) {
        Ok(())
    } else {
        Err(format!("{label}: maxTurns {turns} out of range"))
    }
}

fn check_description(fm: &[(String, String)], label: &str) -> TestResult {
    let desc = fm_get(fm, "description").ok_or(format!("{label}: description missing"))?;
    if desc.is_empty() || desc.contains(": ") {
        return Err(format!("{label}: description empty or has a colon and space"));
    }
    Ok(())
}

#[test]
fn only_builder_and_code_reviewer_remain() -> TestResult {
    let dir = repo_root()?.join(".claude").join("agents");
    let entries = fs::read_dir(&dir).map_err(|e| e.to_string())?;
    let mut names: Vec<String> = Vec::new();
    for entry in entries {
        let name = entry.map_err(|e| e.to_string())?.file_name().to_string_lossy().to_string();
        if name.starts_with("factory-") {
            names.push(name);
        }
    }
    names.sort();
    let want = vec!["factory-builder.md".to_string(), "factory-code-reviewer.md".to_string()];
    if names == want {
        Ok(())
    } else {
        Err(format!("factory agents are {names:?}, want {want:?}"))
    }
}

#[test]
fn builder_frontmatter_is_headless() -> TestResult {
    let text = read(".claude/agents/factory-builder.md")?;
    let fm = frontmatter(&text)?;
    if fm_get(&fm, "name") != Some("factory-builder") {
        return Err("builder name is wrong".to_string());
    }
    for banned in ["isolation", "model", "tools"] {
        if fm_get(&fm, banned).is_some() {
            return Err(format!("builder frontmatter must not set {banned}"));
        }
    }
    check_description(&fm, "builder")?;
    check_max_turns(&fm, "builder")
}

#[test]
fn builder_body_pins_the_rules() -> TestResult {
    let text = read(".claude/agents/factory-builder.md")?;
    require_all(
        &text,
        "builder",
        &[
            "superpowers:executing-plans",
            "inline",
            "final whole-branch review",
            "ledger scripts",
            "progress file",
            "superpowers:test-driven-development",
            "superpowers:systematic-debugging",
            "superpowers:verification-before-completion",
            "dev-workflow-skills:1-git-commit",
            "run its git commands yourself",
            "Never amend",
            "One commit per task",
            "Use at most 2 helper agents per task",
            "Never use a helper agent for commits",
            "Never sleep",
            "never poll",
            "Never `cd` out of the worktree",
            "heredoc",
            "Use Edit and Write",
            "Never write the ledger",
            "board",
            "job directory",
            "Do not re-read `CLAUDE.md`",
            "crate `CLAUDE.md`",
            "BLOCKED:",
            "clippy",
            "release build",
            "-j4",
            "CARGO_TARGET_DIR",
            "shadcn",
            "lucide-react",
            "SingleLineEditor",
            "Monaco",
            "Zustand",
            "unwrap",
            "pathspec",
            "No handoff lines",
        ],
    )?;
    forbid_all(
        &text,
        "builder",
        &["isolation", "WORKTREE:", "BASE:", "HEAD:", "git switch -c", "SendMessage"],
    )
}

#[test]
fn reviewer_frontmatter_is_read_only() -> TestResult {
    let text = read(".claude/agents/factory-code-reviewer.md")?;
    let fm = frontmatter(&text)?;
    if fm_get(&fm, "name") != Some("factory-code-reviewer") {
        return Err("reviewer name is wrong".to_string());
    }
    if fm_get(&fm, "tools") != Some("Read, Grep, Glob") {
        return Err("reviewer tools must be exactly Read, Grep, Glob".to_string());
    }
    for banned in ["model", "isolation"] {
        if fm_get(&fm, banned).is_some() {
            return Err(format!("reviewer frontmatter must not set {banned}"));
        }
    }
    check_description(&fm, "reviewer")?;
    check_max_turns(&fm, "reviewer")
}

#[test]
fn reviewer_body_pins_the_rules() -> TestResult {
    let text = read(".claude/agents/factory-code-reviewer.md")?;
    require_all(
        &text,
        "reviewer",
        &[
            "correctness",
            "DDD placement",
            "serde",
            "error handling",
            "persistence",
            "tests",
            "shadcn",
            "lucide",
            "SingleLineEditor",
            "Monaco",
            "Zustand",
            "unwrap",
            "git CLI",
            "offset",
            "limit 1500",
            "at most 60 lines",
            "[blocker]",
            "[minor]",
            "Do not re-read `CLAUDE.md`",
            "crate `CLAUDE.md`",
        ],
    )?;
    if !has_line(&text, "VERDICT: APPROVE") || !has_line(&text, "VERDICT: CHANGES") {
        return Err("reviewer must list both VERDICT lines on their own line".to_string());
    }
    forbid_all(&text, "reviewer", &["Bash", "Write", "Edit"])
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli --test agent_lint`
Expected: FAIL. Five tests fail (old builder has `isolation` and `model`, the other agent files still exist, the reviewer lacks `maxTurns` rules text).

- [ ] **Step 3: Rewrite the builder agent**

Write `.claude/agents/factory-builder.md` with exactly this content:

````markdown
---
name: factory-builder
description: Builds one Rocket factory plan task by task in the worktree it was started in, and fixes failing checks or review findings when resumed. Run headless by the factory driver.
maxTurns: 150
---
You are the builder in Rocket's software factory. The factory driver started you headless, inside the job worktree, on the job branch. The driver chose your model and owns the board, the ledger, the checks, the review, the merge and the budget.

Input: your prompt names the plan file and the spec file (absolute paths in the job directory, outside your worktree). On a resume it also holds the failing check output or the review findings to fix.

## Start

1. Stay in the worktree. Never `cd` out of the worktree and never touch another checkout.
2. Do not re-read `CLAUDE.md` or the files in `.claude/rules/`. They are preloaded. Read the crate `CLAUDE.md` (`crates/<name>/CLAUDE.md`) of each crate you touch.
3. Read the plan and the spec once. Never edit them.
4. If the plan touches the frontend and `node_modules` is missing, run `yarn install --frozen-lockfile` once before the first `yarn tsc` or `yarn check`.

## Work

- Invoke `superpowers:executing-plans` and follow it. If the Skill tool cannot load it, work inline: do the tasks in order. Skip its final whole-branch review, its ledger scripts and its progress file. The driver owns all three.
- Invoke `superpowers:test-driven-development` for each task. Write the failing test first, then the code.
- When a check or test fails, invoke `superpowers:systematic-debugging` before you change code.
- Before you finish, invoke `superpowers:verification-before-completion` and run the checks the plan names. After they pass, stop. Do not run clippy and do not run a release build.
- Use at most 2 helper agents per task. Never use a helper agent for commits.
- Never sleep and never poll. Run each command once, in the foreground.

## Rocket hard rules

- Run `cargo check -j4` and `cargo test -j4 -p <crate> <test_name>`. Never use `--workspace` or `--all`. Run `yarn tsc --noEmit` and `yarn check` for frontend changes.
- Run `cargo check -j4` yourself before the first commit, so the commit gate starts from a warm target.
- Never set `CARGO_TARGET_DIR`. Your worktree uses its own target directory.
- shadcn/ui primitives only, lucide-react icons only, `SingleLineEditor` for single-line variable-aware fields, Monaco for multi-line editors, narrow Zustand selectors.
- Rust: no unwrap in production paths, no git CLI in app code, `#[serde(rename_all = "camelCase")]` on IPC DTOs only.

## Commits

- One commit per task, made after that task's checks pass.
- Invoke the `dev-workflow-skills:1-git-commit` skill for the message, but run its git commands yourself, inline. Do not start helper agents for it.
- Use a pathspec commit: `git add <paths> && git commit --only -m "..." -- <paths>`. Never `git add -A`, `--all` or `.`.
- Never amend, never rebase, never set `HARNESS_GATE=0`, never run a bare `git stash`, never push, never merge.

## Sandbox

- Use Edit and Write for every file change. Never append with a heredoc or `echo >>`.
- Never write the ledger (`progress.md`), the board (`board.json`), the events log or anything else in the job directory. Only the driver does.

## Blockers and resume

- If the plan cannot be done (a task is ambiguous, a dependency is missing, a check still fails after systematic debugging), stop. Start your result text with `BLOCKED:` and one reason line, then stop. Nobody can answer questions during a run.
- On a resume, fix only the listed failures or findings, with new commits. Do not rewrite earlier commits.

## Reply

End with a short result text: one line per task, plus any blocker. No handoff lines. The driver reads the git state itself.
````

- [ ] **Step 4: Rewrite the reviewer agent**

Write `.claude/agents/factory-code-reviewer.md` with exactly this content:

````markdown
---
name: factory-code-reviewer
description: Reviews a factory diff for Rocket in one pass covering correctness, DDD placement, serde rules, tests and the hard rules. Read-only, run headless by the factory driver.
tools: Read, Grep, Glob
maxTurns: 40
---
You are the single reviewer in Rocket's software factory. You are read-only. The driver already ran the build and test checks and they passed, so do not judge whether the code compiles.

Input: your prompt names a diff file, the job's `spec.md` and the worktree path.

## How to read

- Do not re-read `CLAUDE.md` or the files in `.claude/rules/`. They are preloaded. Read the crate `CLAUDE.md` of each crate the diff touches.
- Page the diff with the Read tool: offset 1 and limit 1500, then the next 1500 lines, until the end. Never read a huge diff in one call.
- Open source files in the worktree only to confirm a finding.

## Check

- correctness: the diff does what `spec.md` asks and nothing outside it, edge cases, no leftover debug code
- error handling: no unwrap in production paths, failures mapped to stable errors
- DDD placement: domain crates hold logic and traits only, `rocket-infra` holds I/O, `rocket-app` is trait-first, Tauri commands stay thin
- serde: `camelCase` renames on IPC DTOs only, never on persistence structs
- persistence compatibility: new fields are optional with defaults, old files still load
- tests: new behavior has tests that would fail without the change
- hard rules: shadcn/ui primitives only (no raw `button`, `input`, `dialog`, `select`, `form`), lucide-react icons only, `SingleLineEditor` for single-line variable-aware fields and Monaco for multi-line editors, narrow Zustand selectors (no full destructuring of store state), no git CLI shell-outs

## Report

Reply with at most 60 lines. Number the findings: `N. [blocker] file:line - problem - concrete fix` or `N. [minor] file:line - problem - concrete fix`. Any `[blocker]` means CHANGES. If there are no findings, say so in one line.

Your last line, alone, must be exactly one of:

VERDICT: APPROVE
VERDICT: CHANGES
````

The reviewer text must not contain the words `Bash`, `Write` or `Edit` (the lint forbids them). If you reword it, keep it free of those words.

- [ ] **Step 5: Delete the other five agent files**

Run: `git rm .claude/agents/factory-spec-writer.md .claude/agents/factory-security-reviewer.md .claude/agents/factory-ux-reviewer.md .claude/agents/factory-ui-reviewer.md .claude/agents/factory-approver.md`
Expected: five `rm` lines.

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli --test agent_lint`
Expected: PASS, 5 tests. If a phrase check fails, fix the agent text, never loosen the test.

- [ ] **Step 7: Commit**

Invoke `dev-workflow-skills:1-git-commit` for the message (run its git commands inline, no helper agents), then:

```bash
git add tools/factory/crates/factory-cli/tests/agent_lint.rs .claude/agents/factory-builder.md .claude/agents/factory-code-reviewer.md
git commit --only -m "feat(factory): headless builder and single reviewer agents" -- tools/factory/crates/factory-cli/tests/agent_lint.rs .claude/agents/factory-builder.md .claude/agents/factory-code-reviewer.md .claude/agents/factory-spec-writer.md .claude/agents/factory-security-reviewer.md .claude/agents/factory-ux-reviewer.md .claude/agents/factory-ui-reviewer.md .claude/agents/factory-approver.md
```

---

### Task 2: Thin `/factory` command and permissions

**Files:**
- Rewrite: `.claude/commands/factory.md`
- Modify: `.claude/settings.json` (add an allow list)
- Modify: `tools/factory/crates/factory-cli/tests/agent_lint.rs` (append tests)

**Interfaces:**
- Consumes: the `factory` binary from plan 05 with subcommands `new`, `run`, `status`, `approve`, `rework`, `cancel`, `cleanup`. `<factory>` below means `tools/factory/target/debug/factory`. `factory new "<feature>" [--model opus]` prints the job id and the absolute job directory (the last line is the directory). `factory run <id> --series` runs the plans of the job one after another.
- Produces: the command file and the settings allow list. T3 relies on both.

- [ ] **Step 1: Confirm the real CLI shape**

Run (build first if needed): `cargo build -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli` then `tools/factory/target/debug/factory --help`, `tools/factory/target/debug/factory new --help`, `tools/factory/target/debug/factory run --help`.
Expected: the subcommands and flags listed above. If a name or flag differs (for example how `new` prints the directory, the model flag, or how the plan files must be named), use the real shape in the command file below and in its lint phrases, and mention the difference in your result text. Do not change the Rust CLI in this plan.

- [ ] **Step 2: Append the failing command and settings tests**

Append to `agent_lint.rs`:

```rust
const COMMAND: &str = ".claude/commands/factory.md";

#[test]
fn command_is_thin_and_pins_the_shapes() -> TestResult {
    let text = read(COMMAND)?;
    require_all(
        &text,
        "command",
        &[
            "<factory> new",
            "<factory> run",
            "--series",
            "--opus",
            "<factory> status",
            "<factory> approve",
            "<factory> rework",
            "<factory> cancel",
            "<factory> cleanup",
            "exact shape",
            "next button for pagination",
            "status page for jobs",
            "approve button for forms",
            "superpowers:brainstorming",
            "superpowers:writing-plans",
            "run_in_background",
            "Do not poll",
            "at most 3 tasks",
            "Next plan:",
            "Next plan: none",
            "validate",
            "rework needs a note",
            "cargo build -j4",
            "escape",
            "Never edit the board",
        ],
    )?;
    forbid_all(
        &text,
        "command",
        &["factory-ctl", "python3", "factory-spec-writer", "factory-approver", "set-stage", "SendMessage"],
    )
}

#[test]
fn settings_allow_the_factory_binary() -> TestResult {
    let text = read(".claude/settings.json")?;
    require_all(
        &text,
        "settings.json",
        &[
            "\"allow\"",
            "Bash(tools/factory/target/debug/factory:*)",
            "Bash(cargo build -j4 --manifest-path tools/factory/Cargo.toml:*)",
        ],
    )
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli --test agent_lint command_is_thin_and_pins_the_shapes`, then the same with `settings_allow_the_factory_binary` (cargo takes one name filter per command here).
Expected: FAIL for both.

- [ ] **Step 4: Rewrite the command**

Write `.claude/commands/factory.md` with exactly this content:

````markdown
---
description: Run Rocket's software factory (brainstorm, plan, then the Rust driver builds, checks, reviews and merges)
argument-hint: "<feature> [--opus] | status | approve <id> | rework <id> <note> | cancel <id> | cleanup <id>"
---

# Factory

Arguments: $ARGUMENTS

You are a thin front end. The design is in `.claude/software-factory.md`. The Rust driver does all mechanical work. Never edit the board, the ledger, the events log or any plan checkbox, and do not do the builder's or reviewer's work.

`<factory>` means `tools/factory/target/debug/factory`, run from the current checkout. If that file does not exist, run `cargo build -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli` once, then continue.

## Pick the subcommand

Match by exact shape only, on the whole argument string:

- `status` alone
- `approve <digits>` (nothing after the digits)
- `cancel <digits>` (nothing after the digits)
- `cleanup <digits>` (nothing after the digits)
- `rework <digits> <note>` with a non-empty note

Anything else is a feature text, for example `next button for pagination`, `status page for jobs`, `approve button for forms` or `cancel dialog 12 layout`. A feature may start or end with the single token `--opus`. Remove that token from the text and pass `--model opus` to `<factory> new`. If the argument is `rework <digits>` with no note, print `rework needs a note: /factory rework <id> <note>` and stop without calling `<factory>`. If the argument is empty, print the argument hint and stop.

## Other subcommands

Each is one call and a one-line report of the output, nothing more:

- `status`: `<factory> status`
- `approve <id>`: `<factory> approve <id>`
- `rework <id> <note>`: `<factory> rework <id> '<note>'`
- `cancel <id>`: `<factory> cancel <id>`
- `cleanup <id>`: `<factory> cleanup <id>`

Always escape the note when you put it in the shell: wrap text in single quotes and write an embedded single quote as `'\''`.

## `<feature>`: new job

1. Run `<factory> new '<feature>'`, with `--model opus` when `--opus` was given. Escape the feature text as above, so quotes, `$`, backticks and newlines stay literal. Read the job id and the absolute job directory from its output.
2. Invoke `superpowers:brainstorming` yourself, with the user, in this session. You may ask the user questions. Save the resulting spec as `spec.md` in the job directory. Use no subagents.
3. Invoke `superpowers:writing-plans` yourself. Save every plan in the job directory as `plan-01.md`, `plan-02.md` and so on, not under `docs/`. Rules for each plan:
   - at most 3 tasks, each ending with an independently testable deliverable
   - the header says the plan is executed by a `factory-builder` subagent through `superpowers:executing-plans`, skipping that skill's final whole-branch review, ledger scripts and progress file
   - every task that touches yml, collection or environment data, auth, variable resolution or the crates named in `CLAUDE.md` starts with the line to read `docs/superpowers/specs/opencollection-spec-reference.md`
   - the last line of the file is `Next plan: <absolute path of the next plan file>`, or exactly `Next plan: none` in the last plan
4. Always validate before running: every plan has at most 3 tasks (count the `### Task` headings), each plan's last line starts with `Next plan:`, the chain of paths leads from `plan-01.md` to the final `Next plan: none`, and `spec.md` exists. If something is wrong, fix the plan with the user. Do not run an invalid series.
5. Run `<factory> run <id> --series` as a Bash command with `run_in_background` set to true. Then tell the user in one line that the factory is running and stop. Do not poll, do not sleep, do not read logs. The harness notifies you when the command exits.
6. When the notification arrives, run `<factory> status` once and report the line for this job. If it is `needs-human`, add that the user can run `/factory approve <id>` or `/factory rework <id> <note>`.

The driver never asks questions during a run, so the plans must be complete before step 5. A blocker found by the builder moves the job to `needs-human`.
````

- [ ] **Step 5: Add the allow list to settings**

Edit `.claude/settings.json`. The file has no `allow` list today, so the background `factory` call and the one-time build would prompt the user. Change the `permissions` block from:

```json
	"permissions": {
		"deny": [
```

to:

```json
	"permissions": {
		"allow": [
			"Bash(tools/factory/target/debug/factory:*)",
			"Bash(cargo build -j4 --manifest-path tools/factory/Cargo.toml:*)"
		],
		"deny": [
```

Nothing else changes in this task (hooks stay until T3). Keep the file's tab indentation.

- [ ] **Step 6: Run the tests and validate the JSON**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli --test agent_lint` and `jq . .claude/settings.json > /dev/null && echo json-ok`
Expected: 7 tests PASS, then `json-ok`.

- [ ] **Step 7: Commit**

Invoke `dev-workflow-skills:1-git-commit` (git commands inline), then:

```bash
git add .claude/commands/factory.md .claude/settings.json tools/factory/crates/factory-cli/tests/agent_lint.rs
git commit --only -m "feat(factory): thin /factory command over the Rust driver" -- .claude/commands/factory.md .claude/settings.json tools/factory/crates/factory-cli/tests/agent_lint.rs
```

---

### Task 3: Cutover, parity check and docs

**Files:**
- Modify: `.claude/settings.json` (remove the three hook entries)
- Delete: `.claude/factory/factory-ctl`, `.claude/factory/hooks/log-event`, `.claude/factory/tests/run.sh`
- Rewrite: `.claude/software-factory.md`
- Modify: `CLAUDE.md`, `tools/factory-server/README.md`, the v2 spec status line, the v2 plan index
- Modify: `tools/factory/crates/factory-cli/tests/agent_lint.rs` (append tests)

**Interfaces:**
- Consumes: the Rust tests of plans 01 to 05 (parity), `agent_lint.rs` helpers from T1.
- Produces: the final repo state: no Python helper, no log-event hook, docs for the new design.

- [ ] **Step 1: Parity check, before any deletion**

For each old capability the table names the Rust coverage and a name pattern. List the test names of each crate once, then confirm every pattern matches at least one test. Run:

```bash
for c in factory-core factory-git factory-agent factory-checks factory-engine factory-cli; do
  echo "== $c"; cargo test -j4 --manifest-path tools/factory/Cargo.toml -p $c -- --list 2>/dev/null | grep ': test' | sed 's/: test//'
done
```

| Old capability (`factory-ctl` and `run.sh`) | Covered by | Pattern to find in that crate's test list |
|---|---|---|
| `new-job`: next id, date, slug, folder, ledger file | `factory-core` ids and slugs (plan 01) | `next_job_id\|job_dir_name\|slug` |
| `set-stage`: stage, round, review states, ids | `factory-core` stage machine (plan 01) | `stage\|transition` |
| `set-stage`: unknown stage or review state rejected | `factory-core` serde and stage tests | `invalid\|unknown\|reject` |
| failed call leaves the board unchanged | `factory-core` atomic write (plan 01) | `atomic\|unchanged` |
| `--reset-reviews` | `factory-core` review state reset | `review` |
| base is never silently changed (`--force-base`) | `factory-core` job model | `base` |
| `ledger` append | `factory-core` `Ledger` | `ledger` |
| `tick-plan`: `Task N: complete` ledger lines at merge, no duplicates | `factory-engine` merge stage and `factory-core` ledger | `complete\|merge` |
| `tick-plan`: plan checkbox ticking | no equivalent expected, see below | `tick` |
| `is-merged`: merged, not merged, no commits, no head, branch deleted | `factory-git` `is_merged` (plan 02) | `is_merged\|merged` |
| `root`: main checkout from a worktree | `factory-git` `Repo::open` (plan 02) | `open\|common\|main_root` |
| `status` lists every job | `factory-cli` status command (plan 05) | `status` |
| hook: appends one line per event, keeps agent ids | `factory-agent` `EventSink` (plan 03) | `event` |
| hook: trims long commands, never stores tool output, strips url query | `factory-agent` `EventSink` | `trim\|redact\|query\|command` |
| hook: invalid input or unwritable log never blocks | `factory-agent` `EventSink` | `unwritable\|invalid\|ignore\|best_effort` |
| hook: concurrent writers give valid lines | `factory-agent` `EventSink` | `concurrent\|parallel` |
| hook: event attributed to the right job and main root | `factory-agent` events carry the job id (spec decision 12) | `job_id\|attribution` |
| agent files lint | `agent_lint.rs` (this plan, T1) | not applicable |
| `/factory` command words lint | `agent_lint.rs` (this plan, T2) | not applicable |
| settings: hooks registered and async | removed on purpose, pinned by the T3 test below | not applicable |

Rules:
- Every row with a pattern must match at least one test in the named crate. If a row does not match, look for the real test name by hand (the pattern is a hint, not the contract). Only if no test covers the capability, stop: do not delete anything, and start your result text with `BLOCKED:` and the list of uncovered rows.
- The `tick-plan` checkbox row is the one allowed gap: the driver records `Task N: complete` in the ledger, and job plans live in the gitignored job directory. If no test matches `tick`, keep going and write the sentence "The driver does not tick plan checkboxes, the ledger is the record." in the docs (step 6) and in your result text.

- [ ] **Step 2: Append the failing cutover tests**

Append to `agent_lint.rs`:

```rust
fn path_exists(rel: &str) -> Result<bool, String> {
    Ok(repo_root()?.join(rel).exists())
}

#[test]
fn old_python_helper_and_hook_are_gone() -> TestResult {
    for rel in [
        ".claude/factory/factory-ctl",
        ".claude/factory/hooks/log-event",
        ".claude/factory/tests/run.sh",
    ] {
        if path_exists(rel)? {
            return Err(format!("{rel} must be deleted"));
        }
    }
    Ok(())
}

#[test]
fn settings_have_no_factory_hooks() -> TestResult {
    let text = read(".claude/settings.json")?;
    forbid_all(
        &text,
        "settings.json",
        &["log-event", "factory/hooks", "SubagentStart", "SubagentStop", "PostToolUse"],
    )?;
    require_all(&text, "settings.json", &["rust-safety-pretool", "bash-guard", "commit-gate"])
}

#[test]
fn docs_describe_the_new_design() -> TestResult {
    let doc = read(".claude/software-factory.md")?;
    require_all(
        &doc,
        "software-factory.md",
        &[
            "Rust driver",
            "tools/factory/",
            "measured baseline",
            "estimate",
            "12.75M",
            "2.2M weighted",
            "needs-human",
            "VERDICT: APPROVE",
            "factory run",
            "tools/factory-server/",
            "risk_paths",
            "reviewed sha",
        ],
    )?;
    forbid_all(&doc, "software-factory.md", &["factory-ctl", "factory-approver", "factory-spec-writer", "log-event"])?;
    let root_doc = read("CLAUDE.md")?;
    require_all(&root_doc, "CLAUDE.md", &[".claude/software-factory.md", "tools/factory/"])?;
    forbid_all(&root_doc, "CLAUDE.md", &["factory-ctl"])
}

#[test]
fn v2_is_marked_superseded() -> TestResult {
    let spec = read("docs/superpowers/specs/2026-10-13-software-factory-v2-design.md")?;
    let index = read("docs/superpowers/plans/2026-10-13-software-factory-v2/00-plan-index.md")?;
    require_all(&spec, "v2 spec", &["Status: superseded"])?;
    require_all(&index, "v2 plan index", &["Superseded", "software-factory-v3"])
}
```

- [ ] **Step 3: Run the new tests to verify they fail**

Run: `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli --test agent_lint`
Expected: FAIL for `old_python_helper_and_hook_are_gone`, `settings_have_no_factory_hooks`, `docs_describe_the_new_design`, `v2_is_marked_superseded`.

- [ ] **Step 4: Remove the hook entries from settings**

Edit `.claude/settings.json`. Delete the whole `"SubagentStart"`, `"SubagentStop"` and `"PostToolUse"` keys under `hooks`. Each of them held only the `log-event` command. Keep `PreToolUse` (rust-safety-pretool, bash-guard, commit-gate) exactly as is, and keep the `permissions.allow` list from T2. After the edit, `"hooks"` contains only `"PreToolUse"`, and the closing `},` of `PreToolUse` becomes `}` with no trailing comma before the end of `"hooks"`.

Run: `jq '.hooks | keys' .claude/settings.json`
Expected: `["PreToolUse"]`.

- [ ] **Step 5: Delete the Python helper, the hook and the old tests**

Run: `git rm .claude/factory/factory-ctl .claude/factory/hooks/log-event .claude/factory/tests/run.sh`
Expected: three `rm` lines. Then run `ls .claude/factory` and confirm that `config.toml` (from plan 01) is still there and nothing else of the old files is left. Remove empty `hooks` and `tests` directories if they remain.

- [ ] **Step 6: Rewrite `.claude/software-factory.md`**

Write the file with this content (keep sentences short, keep the exact phrases the lint test pins):

````markdown
# Software Factory for Rocket

Status: v3, a Rust driver. Design: `docs/superpowers/specs/2026-10-13-software-factory-v3-rust-driver-design.md`. Plans: `docs/superpowers/plans/2026-10-13-software-factory-v3/`. v1 (LLM orchestrator, Python helper) and the v2 plan series are superseded.

The Rust driver does everything mechanical: state, checks, diffs, merge, sync, series, budget and events. The LLM is used only where judgment is needed: design and planning with the user, one builder per plan, one reviewer per diff.

## Pipeline

```
/factory <feature>  (interactive, with the user)
  superpowers:brainstorming -> spec.md
  superpowers:writing-plans -> plan-01.md ... plan-NN.md (at most 3 tasks each, Next plan: line)
  factory new, then factory run <id> --series  (background Bash, session idle)

factory run (no LLM tokens):
  worktree -> builder (claude -p, Sonnet) -> checks -> reviewer (claude -p, Sonnet)
                 ^                              |  red: resume builder with trimmed output
                 |  CHANGES, at most 2 rounds   |
                 +------------------------------+
  APPROVE + no risk + under size cap + budget ok -> merge reviewed sha to local main -> next plan
  otherwise                                      -> needs-human (factory approve | factory rework | factory cancel)
```

## Job model

- One job is one feature. It holds a series of plans, each of at most 3 tasks, run one after another, never in parallel.
- Job id `NNN`, branch `factory/NNN-<slug>`, worktree `.claude/worktrees/factory-NNN-<slug>`. The driver creates the worktree with `git2` and starts the builder inside it.
- Job directory `.superpowers/sdd/YYYY-MM-DD-NNN-<slug>/` (gitignored): `spec.md`, `plan-NN.md`, `progress.md` (the ledger), `checks.md`, `review-*.diff`, review replies.
- `.superpowers/factory/board.json`: stage per job. Stages `spec`, `build`, `review`, `approve`, `merged`, `needs-human`. Same names and fields as v1, so the dashboard keeps working.
- `.superpowers/factory/events.jsonl`: events written by the driver, each with the job id.
- Only the driver writes the ledger, board and events. Agents and the `/factory` command never do.
- Config: `.claude/factory/config.toml` (`risk_paths`, `diff_cap_lines`, `budget_weighted`, `fix_rounds`, `stall_minutes`, `builder_model`, `reviewer_model`, `[[checks]]`).

## Modes

- Default: builder and reviewer run on Sonnet.
- `/factory <feature> --opus`: this job uses Opus (`factory new --model opus`). The model is never in the agent files, the driver passes it.

## Agents (`.claude/agents/`)

| Agent | Tools | Duties |
|---|---|---|
| `factory-builder` | all | Runs headless in the worktree. `superpowers:executing-plans` with an inline fallback (no final review, ledger scripts or progress file), TDD, systematic debugging on a failing check, verification before finishing. One commit per task through `dev-workflow-skills:1-git-commit`, git commands inline, no amend. At most 2 helper agents per task, none for commits. Reports a blocker as `BLOCKED:` in the result text. |
| `factory-code-reviewer` | Read, Grep, Glob | One combined review: correctness, DDD placement, serde, error handling, persistence compatibility, tests, and the hard rules (shadcn, lucide, SingleLineEditor vs Monaco, Zustand selectors, no unwrap, no git CLI). At most 60 lines, findings tagged `[blocker]` or `[minor]`, last line `VERDICT: APPROVE` or `VERDICT: CHANGES`. |

There are no spec-writer, security, ux, ui or approver agents. Risky diffs go to the human.

## Checks

The driver runs the checks itself, chosen from the changed paths by `config.toml`: `cargo check -j4`, targeted `cargo test -j4 -p <crate>`, `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern>`, and the factory crate checks. A failure resumes the builder with trimmed output. The reviewer never sees a red build. Results go to `checks.md`.

## Budget

Weighted tokens per job: input + cache creation + output + cache read / 10. Defaults: 3,000,000 per job, warn at 80 percent, kill the agent and park the job at 100 percent. Diff cap 4,000 changed lines, 2 fix rounds, a stall limit without events.

## Merge rules

Auto-merge needs all of: no check failure, the latest reviewer verdict is exactly `VERDICT: APPROVE`, the diff is not risky, the diff is within the size cap, the budget is not at stop. Otherwise a fix round (at most 2) or `needs-human`.

Risky means the diff touches any `risk_paths` entry: the collection trust gate, auth, secrets or `{{process.env}}`, persistence formats, `src-tauri/`, `DomainEvent`, `Cargo.toml`, `Cargo.lock`, `package.json`, `tauri.conf.json`, CSP or the harness hooks. A missing or broken config makes every diff risky.

The driver merges the reviewed sha only: it checks the branch tip equals the recorded head, preflights main (on `main`, clean, not mid-merge), makes a no-ff merge commit `chore(factory): merge job NNN <slug>`, and verifies the second parent. If main moved it merges main into the job branch inside the worktree, re-runs checks and the reviewer on the sync diff (at most 2 syncs). A conflict goes to `needs-human`. It never pushes and never rewrites history.

## How to run

Build once: `cargo build -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli`. Binary: `tools/factory/target/debug/factory`.

| Command | Does |
|---|---|
| `/factory <feature> [--opus]` | Brainstorm and plan with the user, then `factory run <id> --series` in the background. |
| `/factory status` | `factory status`. |
| `/factory approve <id>` | Release a `needs-human` job to the merge step. |
| `/factory rework <id> <note>` | Send a `needs-human` job back to the builder with the note. |
| `/factory cancel <id>` | Stop and park a job. |
| `/factory cleanup <id>` | Remove the worktree of a finished job. |

The command matches subcommands by exact shape, so feature text such as `next button for pagination` stays a feature. The session does not poll while the driver runs. The harness notifies it on exit.

The driver cannot ask questions during a run, so plans must be complete. Harness rules the driver keeps: every cargo call has `-j4`, no `--workspace`, no `CARGO_TARGET_DIR`, pathspec commits only, one implementer per worktree.

## Dashboard

`tools/factory-server/` (own workspace, not shipped) reads `board.json` and `events.jsonl` and serves a local page. It never writes them, and the factory works the same when it is not running. Run it with `cargo run -j4 --manifest-path tools/factory-server/Cargo.toml -- --root <main checkout>`. Token display, stage history and series chips come with plan 07 of the v3 series.

## Token comparison

| | Raw tokens | Weighted tokens | Status |
|---|---|---|---|
| Job 001 (v1, Opus builder, orchestrator session, 6 agents) | 12.75M | 2.2M weighted | measured baseline |
| New design, comparable job | not stated | about 0.9M weighted | estimate |

The baseline: the orchestrator was 53.6 percent of the raw tokens and commit helper agents about 7 percent. The estimate is arithmetic, not a measurement: it removes those two shares and the five extra reviewers (about 39 percent of the baseline remains, 2.2M x 0.39), and it ignores the interactive brainstorming and planning tokens, which the user spends in their own session. Plan 08 measures a real job, and that number replaces the estimate.

## Open items

- The new driver has only run against a fake `claude` in tests. The first real run is the plan 08 dry run.
- The driver does not tick plan checkboxes, the ledger is the record.
- `/verify-rocket` still conflicts with the harness guard. Plan 08 fixes it.
- The dashboard look was never checked in a browser. Plan 08 checks it.
````

If step 1 found a test that does tick checkboxes, delete the second open item.

- [ ] **Step 7: Update the pointer, README and supersede notes**

1. `CLAUDE.md`: replace the line starting `See `.claude/software-factory.md`` with exactly:
   `See `.claude/software-factory.md` for the software factory (Rust driver in `tools/factory/`, thin `/factory` command, dashboard in `tools/factory-server/`).`
2. `tools/factory-server/README.md`: in the Run block replace `"$(python3 .claude/factory/factory-ctl root)"` with `"$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")"`. In "What it reads", say the board and the events file are written by the `factory` driver (`tools/factory/`), not by a Python helper or a hook. Plan 07 owns the server code, so do not edit `tools/factory-server/src/` or its tests here.
3. `docs/superpowers/specs/2026-10-13-software-factory-v2-design.md`: change the status line `Status: draft for review, 2026-10-13. Builds on` to `Status: superseded by docs/superpowers/specs/2026-10-13-software-factory-v3-rust-driver-design.md on 2026-10-13. Builds on` (keep the rest of the line).
4. `docs/superpowers/plans/2026-10-13-software-factory-v2/00-plan-index.md`: add as the first line, then a blank line:
   `> **Superseded** by the v3 Rust driver series in docs/superpowers/plans/2026-10-13-software-factory-v3/. Do not run these plans.`

- [ ] **Step 8: Find leftover references**

Run: `grep -rln "factory-ctl\|factory/hooks\|log-event" --exclude-dir=node_modules --exclude-dir=target --exclude-dir=.git --exclude-dir=docs .`
Expected: only `tools/factory-server/` files (`tests/smoke.sh`, `tests/seed-demo.sh`, `src/model.rs`) and no `.claude/` files. Do not edit those server files. List them in your result text so plan 07 picks them up.

- [ ] **Step 9: Run the final checks**

Run each, expect success:
- `cargo test -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli --test agent_lint` (all 11 tests PASS)
- `cargo check -j4 --manifest-path tools/factory/Cargo.toml -p factory-cli`
- `jq . .claude/settings.json > /dev/null && echo json-ok`
- `ls .claude/agents | grep factory-` lists exactly `factory-builder.md` and `factory-code-reviewer.md`

- [ ] **Step 10: Commit**

Invoke `dev-workflow-skills:1-git-commit` (git commands inline), then one pathspec commit:

```bash
git add .claude/settings.json .claude/software-factory.md CLAUDE.md tools/factory-server/README.md docs/superpowers/specs/2026-10-13-software-factory-v2-design.md docs/superpowers/plans/2026-10-13-software-factory-v2/00-plan-index.md tools/factory/crates/factory-cli/tests/agent_lint.rs
git commit --only -m "chore(factory): cut over to the Rust driver and remove the Python helper" -- .claude/settings.json .claude/software-factory.md CLAUDE.md tools/factory-server/README.md docs/superpowers/specs/2026-10-13-software-factory-v2-design.md docs/superpowers/plans/2026-10-13-software-factory-v2/00-plan-index.md tools/factory/crates/factory-cli/tests/agent_lint.rs .claude/factory/factory-ctl .claude/factory/hooks/log-event .claude/factory/tests/run.sh
```

---

## Self-Review

- Spec coverage: decision 2 (thin command, background run, no polling) in T2. Decisions 5 and 6 (one reviewer, risky diffs to the human, other agents deleted) in T1 and docs in T3. Decision 8 (models from driver and `--opus`) in T1 frontmatter lint and T2. Decisions 12 and 13 (no Python hook, driver writes events and ledger) in T3 and the builder and command rules. Plan table row 06 (agent files, thin command, hook removed, Python and tests deleted after parity) in T1 to T3. The docs ask (diagram, job model, modes, agents, checks, budget, merge rules, how to run, dashboard, token comparison labelled measured versus estimate) is in T3 step 6. The memory note is explicitly left alone.
- Placeholder scan: none. The one soft spot is the plan 05 CLI shape and crate directory, which T2 step 1 and the File Structure note tell the builder to confirm and adapt, with the lint phrases following the real shape.
- Type consistency: helper names `repo_root`, `read`, `frontmatter`, `fm_get`, `require_all`, `forbid_all`, `has_line` are defined in T1 and reused unchanged in T2 and T3. `<factory>` means the same binary path in the command, the docs and the settings allow list.

Next plan: docs/superpowers/plans/2026-10-13-software-factory-v3/plan-07-dashboard.md
