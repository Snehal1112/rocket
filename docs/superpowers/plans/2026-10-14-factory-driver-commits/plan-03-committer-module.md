# Factory driver commits, plan 03: committer module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the engine step that turns a builder's uncommitted work into commits: stage the worktree, run the committer agent with a prepared prompt, verify what it did against the rules in the spec, retry once, and fall back to a plain `git2` commit.

**Architecture:** Three layers. `factory-core` gets pure rules (message validator, fallback subject, reply parser). `factory-engine` gets a new `commit.rs` with two pure builders (the prompt and the run spec) and one `Engine::commit_step` that uses the git layer from plan 01 and the agent file and config from plan 02. The spec names a separate `Committer` port, but the engine already tests agent steps through the `AgentRunner` port and `ScriptedRunner`, so `commit_step` runs through `self.runner` and the tests script it. The step returns an outcome and does not park the job: plan 04 calls it and decides.

**Tech Stack:** Rust 2021, `git2` through `factory-git`, `ScriptedRunner` and `Fixture` from `factory-engine/tests/common`. Run from `tools/factory/`.

**Spec:** `docs/superpowers/specs/2026-10-14-factory-driver-commits-design.md`, decisions 5 (floor), 7 (failure policy), 8 (gate env), 11 (config keys), 12 (precomputed analysis), 13 (trimmed prefix), design sections 2 (flow), 3 (verification and subject validator), 5 (module).

## Global Constraints

- Always `cargo ... -j4`. Never `--workspace` or `--all`. Never set `CARGO_TARGET_DIR`. Run cargo from `tools/factory/`.
- No `unwrap()` in production code. Tests may use `expect("...")`.
- Subject validator, from the spec: a Conventional Commits type with an optional scope, a description, at most 72 characters, no trailing period, and a blank line after the subject when a body exists.
- The floor rule, from the spec: a commit at or below the floor is never rewritten. A rewrite parks the job and is never retried. The driver does not rely on the prompt for this.
- Retry once by resuming the committer session. After that the driver makes a plain `git2` commit `chore: <task title>`. A message problem never parks a job.
- The committer spec sets `HARNESS_GATE=0` in `env`, `strict_mcp = true`, `tools = "Bash,Read,Skill"`, and a per-run cap `commit_budget_weighted` with `used_before = 0`. Do not reuse `with_limits`, which sets the job cap and the job's spent tokens.
- Existing engine helpers: `Engine::run_agent` (runs a spec and tallies usage), `Engine::ruling` (one-line `Ruling:` in the ledger), `ports::git_err`. Read them in `crates/factory-engine/src/ports.rs`.
- Commit each task through the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths and commit with a pathspec.

## Review Focus

- A 72 character subject passes and a 73 character one fails; a trailing period, an uppercase type, a missing space after the colon, and no blank line before a body all fail; `feat(x)!: ...` passes.
- The committer run is capped per run (`used_before` is 0), not by the job tally.
- The prompt says "inline, no subagents" only when `commit_subagents` is false; a huge or multibyte diff is cut at a character boundary.
- Rewriting the floor commit parks at once with no retry; a valid commit with a clean tree is accepted even when the reply says BLOCKED or the run stopped early.
- Nothing staged makes no agent call; a missing `commit-context.md` makes no agent call and uses the fallback commit.
- More than 4 commits, or an invalid message after the retry, keeps the commits and writes a `Ruling:` line.

---

### Task 1: Message validator, fallback subject and reply parser

**Files:**
- Create: `tools/factory/crates/factory-core/src/commitmsg.rs`
- Modify: `tools/factory/crates/factory-core/src/lib.rs`
- Test: the `tests` module at the bottom of `commitmsg.rs`

**Interfaces:**
- Consumes: nothing.
- Produces, exported from the crate root:
  - `pub const MAX_SUBJECT_CHARS: usize = 72;`
  - `pub const MAX_COMMITS_PER_RUN: usize = 4;`
  - `pub fn validate_message(message: &str) -> Result<(), String>`: the subject validator, the error is a short reason.
  - `pub fn fallback_subject(task_title: &str) -> String`: `chore: <title>`, whitespace folded, trailing period removed, cut to 72 characters.
  - `pub fn blocked_reason(reply: &str) -> Option<String>`: the reason when the first line of a committer reply starts with `BLOCKED:`.

- [ ] **Step 1: Write the failing tests**

Create `tools/factory/crates/factory-core/src/commitmsg.rs` with only the tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_scoped_and_breaking_subjects() {
        assert!(validate_message("feat: add a thing").is_ok());
        assert!(validate_message("fix(infra): stop the leak").is_ok());
        assert!(validate_message("feat(x)!: drop the old flag").is_ok());
        assert!(validate_message("chore: tidy\n\nBody paragraph.\nMore body.").is_ok());
    }

    #[test]
    fn subject_length_limit_is_72_characters() {
        let ok = format!("feat: {}", "a".repeat(72 - 6));
        assert_eq!(ok.chars().count(), 72);
        assert!(validate_message(&ok).is_ok());
        let long = format!("feat: {}", "a".repeat(72 - 5));
        assert_eq!(long.chars().count(), 73);
        assert!(validate_message(&long).is_err());
    }

    #[test]
    fn rejects_bad_subjects() {
        for bad in [
            "",
            "Added a thing",
            "feat add a thing",
            "feat:add a thing",
            "Feat: add a thing",
            "wip: add a thing",
            "feat(): add a thing",
            "feat(two words): add a thing",
            "feat(x: add a thing",
            "feat: ",
            "feat: add a thing.",
        ] {
            assert!(validate_message(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn a_body_needs_a_blank_line_after_the_subject() {
        assert!(validate_message("feat: add a thing\nbody right away").is_err());
        assert!(validate_message("feat: add a thing\n\nbody after a blank line").is_ok());
    }

    #[test]
    fn counts_characters_and_not_bytes() {
        let subject = format!("feat: {}", "é".repeat(66));
        assert_eq!(subject.chars().count(), 72);
        assert!(validate_message(&subject).is_ok());
    }

    #[test]
    fn fallback_subject_is_a_valid_chore_subject() {
        assert_eq!(fallback_subject("Add the thing."), "chore: Add the thing");
        assert_eq!(fallback_subject("  Add   the\nthing  "), "chore: Add the thing");
        assert_eq!(fallback_subject(""), "chore: commit factory changes");
        let long = fallback_subject(&"word ".repeat(40));
        assert!(long.chars().count() <= MAX_SUBJECT_CHARS);
        assert!(validate_message(&long).is_ok());
        let multibyte = fallback_subject(&"é".repeat(100));
        assert!(multibyte.chars().count() <= MAX_SUBJECT_CHARS);
        assert!(validate_message(&multibyte).is_ok());
    }

    #[test]
    fn blocked_reason_reads_only_the_first_line() {
        assert_eq!(blocked_reason("BLOCKED: no skill\nmore"), Some("no skill".to_string()));
        assert_eq!(blocked_reason("  BLOCKED:   spaced  "), Some("spaced".to_string()));
        assert_eq!(blocked_reason("abc1234 feat: x\nBLOCKED: later line"), None);
        assert_eq!(blocked_reason(""), None);
    }
}
```

In `tools/factory/crates/factory-core/src/lib.rs`, add `pub mod commitmsg;` after `pub mod budget;` and add this line after the `pub use budget::...` line:

```rust
pub use commitmsg::{blocked_reason, fallback_subject, validate_message, MAX_COMMITS_PER_RUN, MAX_SUBJECT_CHARS};
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd tools/factory && cargo test -j4 -p factory-core commitmsg`
Expected: FAIL to compile, `cannot find function validate_message`.

- [ ] **Step 3: Write the implementation**

Put this at the top of `commitmsg.rs`, above the `tests` module:

```rust
//! Rules for commit messages written by the committer agent, checked by the driver.

/// Longest subject the driver accepts. The commit skill asks for 50, and the driver
/// allows slack so a 51 character subject does not fail a run.
pub const MAX_SUBJECT_CHARS: usize = 72;

/// Most commits one committer run may make.
pub const MAX_COMMITS_PER_RUN: usize = 4;

const TYPES: &[&str] = &["feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore", "revert"];

/// Checks a full commit message: a Conventional Commits subject of at most 72 characters
/// with no trailing period, and a blank line before any body. The error is a short reason.
pub fn validate_message(message: &str) -> Result<(), String> {
    let mut lines = message.lines();
    let subject = lines.next().unwrap_or("");
    if subject.trim().is_empty() {
        return Err("the subject is empty".to_string());
    }
    let chars = subject.chars().count();
    if chars > MAX_SUBJECT_CHARS {
        return Err(format!("the subject is {chars} characters, the limit is {MAX_SUBJECT_CHARS}"));
    }
    if subject.ends_with('.') {
        return Err("the subject ends with a period".to_string());
    }
    let (head, description) =
        subject.split_once(": ").ok_or_else(|| "the subject must look like `type(scope): description`".to_string())?;
    let head = head.strip_suffix('!').unwrap_or(head);
    let (kind, scope) = match head.split_once('(') {
        Some((kind, rest)) => {
            let scope = rest.strip_suffix(')').ok_or_else(|| "the scope has no closing parenthesis".to_string())?;
            (kind, Some(scope))
        }
        None => (head, None),
    };
    if !TYPES.contains(&kind) {
        return Err(format!("`{kind}` is not a lowercase Conventional Commits type"));
    }
    if let Some(scope) = scope {
        if scope.is_empty() || scope.chars().any(char::is_whitespace) {
            return Err("the scope must be one word".to_string());
        }
    }
    if description.trim().is_empty() {
        return Err("the description is empty".to_string());
    }
    if let Some(second) = lines.next() {
        if !second.trim().is_empty() {
            return Err("a blank line must follow the subject".to_string());
        }
    }
    Ok(())
}

/// The subject of the plain fallback commit: `chore: <task title>`, with whitespace folded,
/// a trailing period removed and the whole subject cut to 72 characters.
pub fn fallback_subject(task_title: &str) -> String {
    let folded = task_title.split_whitespace().collect::<Vec<_>>().join(" ");
    let title = folded.trim_end_matches('.').trim();
    let title = if title.is_empty() { "commit factory changes" } else { title };
    let prefix = "chore: ";
    let room = MAX_SUBJECT_CHARS - prefix.chars().count();
    let cut: String = title.chars().take(room).collect();
    format!("{prefix}{}", cut.trim_end().trim_end_matches('.'))
}

/// The reason after `BLOCKED:` when the first line of a committer reply starts with it.
pub fn blocked_reason(reply: &str) -> Option<String> {
    let first = reply.lines().next()?.trim();
    first.strip_prefix("BLOCKED:").map(|reason| reason.trim().to_string())
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd tools/factory && cargo test -j4 -p factory-core`
Expected: PASS, including the seven new tests.

- [ ] **Step 5: Check warnings**

Run: `cd tools/factory && cargo clippy -j4 -p factory-core -- -D warnings`
Expected: no output.

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill, with these paths only:

```bash
git add tools/factory/crates/factory-core/src/commitmsg.rs tools/factory/crates/factory-core/src/lib.rs
```

Expected subject: `feat(factory): validate commit messages and parse committer replies`.

---

### Task 2: The committer prompt, run spec and context file

**Files:**
- Create: `.claude/factory/commit-context.md`
- Create: `tools/factory/crates/factory-engine/src/commit.rs`
- Modify: `tools/factory/crates/factory-engine/src/lib.rs`
- Test: the `tests` module at the bottom of `commit.rs`

**Interfaces:**
- Consumes: `factory_core::Config` (plan 02 keys `commit_model`, `commit_budget_weighted`, `commit_subagents`, `stall_minutes`), `factory_agent::AgentSpec` (plan 02 fields `env`, `strict_mcp`, `tools`), `factory_git::{CommitInfo, StagedReport}` (plan 01).
- Produces, all `pub` and re-exported from the engine crate root:
  - `CommitKind { Task, Legacy, Fix }` with `as_str(self) -> &'static str` (`"task"`, `"legacy"`, `"fix"`).
  - `CommitRequest<'a> { kind: CommitKind, plan_title: &'a str, task_title: &'a str, goal: &'a str, floor: &'a str }`.
  - `COMMITTER_AGENT`, `COMMITTER_MAX_TURNS`, `COMMITTER_TOOLS`, `COMMIT_CONTEXT_FILE` constants.
  - `committer_prompt(context: &str, req: &CommitRequest<'_>, report: &StagedReport, above: &[CommitInfo], subagents: bool) -> String`.
  - `committer_spec(job_id: &str, config: &Config, cwd: &Path, prompt: &str) -> AgentSpec`.

Note for the implementer: `CommitInfo` gains a `message` field only in task 3. In this task's tests, build `CommitInfo` through the helper in the test module below, which uses only the three fields that exist now. Task 3 adds `message` to that helper.

- [ ] **Step 1: Create the context file**

Create `.claude/factory/commit-context.md` with exactly this content:

````markdown
# Factory committer context

You are committing the work of a job in Rocket's software factory. The driver prepared everything below and checks the result again when you finish. Nobody answers questions during a run.

## Commit style

The project uses Conventional Commits with a lowercase type: `type(scope): description`, as in `CLAUDE.md`. Allowed types are feat, fix, docs, style, refactor, perf, test, build, ci, chore and revert. The scope is the crate or area, for example `infra`, `http`, `flow`, `factory` or `frontend`. Keep the subject at 72 characters or fewer, with no trailing period, and put a blank line between the subject and any body. Use the body to say why, in short sentences.

## Standing answers

The commit skill asks questions. Answer them like this:

| The skill asks | Answer |
|---|---|
| Step 1 verdict is `split` and asks to confirm the split | Confirm. Commit each group, at most 4 commits per run. If there are more groups, merge the smallest. |
| One amend candidate matches | Amend it only if it is above the floor and not pushed. Otherwise make a new commit. |
| Two or more candidates match | Take the most recent one above the floor. If none is above the floor, make a new commit. |
| Amend target is HEAD | `git commit --amend`. |
| Amend target is not HEAD | `git commit --fixup` plus `GIT_SEQUENCE_EDITOR=true git rebase -i --autosquash <target>~1`. On a conflict: `git rebase --abort` and make a new commit. |
| Force-push after amending a pushed commit | Never. A pushed commit is not amended in the first place. |
| Anything else | Pick the option that makes a new commit and changes no existing commit. |

## Rules that do not change

- The floor is the commit named in "This run". Never rewrite it or any commit below it.
- Never push, merge, stash, or use `git add -A`, `--all` or `.`.
- Stage only paths of the staged change you were given. The driver already staged everything that belongs to this task.
````

- [ ] **Step 2: Write the failing tests**

In `tools/factory/crates/factory-engine/src/lib.rs`, add `mod commit;` after `mod clock;` and add this line after the `pub use clock::...` line:

```rust
pub use commit::{
    committer_prompt, committer_spec, CommitKind, CommitRequest, COMMITTER_AGENT, COMMITTER_MAX_TURNS, COMMITTER_TOOLS,
    COMMIT_CONTEXT_FILE,
};
```

Create `tools/factory/crates/factory-engine/src/commit.rs` with only the tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn request<'a>() -> CommitRequest<'a> {
        CommitRequest {
            kind: CommitKind::Task,
            plan_title: "Demo plan",
            task_title: "Add the thing",
            goal: "add the thing",
            floor: "abc1234def",
        }
    }

    fn report(diff: &str) -> StagedReport {
        StagedReport {
            status: "On branch factory/001-demo\nChanges to be committed:\n\tnew file:   a.txt\n".to_string(),
            stat: " a.txt | 1 +\n".to_string(),
            diff: diff.to_string(),
            log: vec!["1111111 feat(x): earlier".to_string()],
        }
    }

    fn info(sha: &str, subject: &str, files: &[&str]) -> CommitInfo {
        CommitInfo {
            sha: sha.to_string(),
            subject: subject.to_string(),
            files: files.iter().map(|f| f.to_string()).collect(),
        }
    }

    #[test]
    fn prompt_carries_the_context_the_run_and_the_precomputed_analysis() {
        let above = vec![info("aaaaaaa1111", "feat(x): first", &["one.txt"]), info("bbbbbbb2222", "feat(x): second", &["two.txt"])];
        let prompt = committer_prompt("CONTEXT TEXT", &request(), &report("+++ b/a.txt\n+hi\n"), &above, false);
        assert!(prompt.starts_with("CONTEXT TEXT"));
        for want in ["Kind: task", "Plan: Demo plan", "Task: Add the thing", "Goal: add the thing", "Floor: abc1234def"] {
            assert!(prompt.contains(want), "missing {want}");
        }
        for want in ["### git status", "new file:   a.txt", "### git diff --staged --stat", "### git diff --staged", "+hi", "### git log --oneline -5", "1111111 feat(x): earlier"] {
            assert!(prompt.contains(want), "missing {want}");
        }
        // Newest first, with the files of each candidate.
        let second = prompt.find("bbbbbbb feat(x): second").expect("second listed");
        let first = prompt.find("aaaaaaa feat(x): first").expect("first listed");
        assert!(second < first);
        assert!(prompt.contains("  two.txt"));
    }

    #[test]
    fn prompt_says_inline_only_when_subagents_are_off() {
        let off = committer_prompt("c", &request(), &report(""), &[], false);
        let on = committer_prompt("c", &request(), &report(""), &[], true);
        assert!(off.contains("inline, no subagents"));
        assert!(!on.contains("inline, no subagents"));
        assert!(on.contains("through the skill's subagents"));
    }

    #[test]
    fn prompt_lists_no_candidates_when_nothing_is_above_the_floor() {
        let prompt = committer_prompt("c", &request(), &report(""), &[], false);
        assert!(prompt.contains("(none)"));
    }

    #[test]
    fn a_huge_diff_is_cut_at_a_character_boundary() {
        let huge = "é".repeat(40_000);
        assert!(huge.len() > MAX_DIFF_BYTES);
        let prompt = committer_prompt("c", &request(), &report(&huge), &[], false);
        assert!(prompt.contains("diff cut at 60000 bytes"));
        assert!(prompt.len() < huge.len());
    }

    #[test]
    fn spec_is_trimmed_gated_and_capped_per_run() {
        let mut config = Config::default();
        config.commit_model = "opus".to_string();
        config.commit_budget_weighted = 12_345;
        config.stall_minutes = 7;
        let spec = committer_spec("001", &config, Path::new("/wt"), "the prompt");
        assert_eq!(spec.role, "committer");
        assert_eq!(spec.agent.as_deref(), Some("factory-committer"));
        assert_eq!(spec.model.as_deref(), Some("opus"));
        assert_eq!(spec.budget_weighted, Some(12_345));
        assert_eq!(spec.used_before, 0);
        assert_eq!(spec.max_turns, Some(40));
        assert_eq!(spec.stall_after, Some(Duration::from_secs(420)));
        assert_eq!(spec.env, vec![("HARNESS_GATE".to_string(), "0".to_string())]);
        assert!(spec.strict_mcp);
        assert_eq!(spec.tools.as_deref(), Some("Bash,Read,Skill"));
        assert_eq!(spec.prompt, "the prompt");
    }

    #[test]
    fn shipped_context_file_has_the_style_and_the_standing_answers() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../.claude/factory/commit-context.md");
        let text = std::fs::read_to_string(path).expect("shipped context file");
        for want in [
            "Conventional Commits",
            "lowercase type",
            "## Standing answers",
            "at most 4 commits per run",
            "above the floor and not pushed",
            "GIT_SEQUENCE_EDITOR=true git rebase -i --autosquash",
            "Never push",
        ] {
            assert!(text.contains(want), "context file is missing {want}");
        }
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd tools/factory && cargo test -j4 -p factory-engine --lib commit`
Expected: FAIL to compile, `cannot find type CommitRequest`.

- [ ] **Step 4: Write the implementation**

Put this at the top of `commit.rs`, above the `tests` module:

```rust
//! The committer step: stage a job worktree, run the committer agent, check what it did.

use std::path::Path;
use std::time::Duration;

use factory_agent::AgentSpec;
use factory_core::Config;
use factory_git::{CommitInfo, StagedReport};

/// The agent file the driver passes with `--agent`.
pub const COMMITTER_AGENT: &str = "factory-committer";
pub const COMMITTER_MAX_TURNS: u32 = 40;
/// The tools a committer run may use. It never edits files, so there is no Edit or Write.
pub const COMMITTER_TOOLS: &str = "Bash,Read,Skill";
/// Repo relative path of the prompt text, read from the main checkout like `config.toml`.
pub const COMMIT_CONTEXT_FILE: &str = ".claude/factory/commit-context.md";
/// The staged diff in the prompt is cut at this many bytes.
const MAX_DIFF_BYTES: usize = 60_000;

/// What kind of work the commit is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitKind {
    Task,
    Legacy,
    Fix,
}

impl CommitKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CommitKind::Task => "task",
            CommitKind::Legacy => "legacy",
            CommitKind::Fix => "fix",
        }
    }
}

/// What the driver tells the committer about one run.
#[derive(Debug, Clone)]
pub struct CommitRequest<'a> {
    pub kind: CommitKind,
    pub plan_title: &'a str,
    pub task_title: &'a str,
    /// The goal line of the spec.
    pub goal: &'a str,
    /// The commit at or below which history is never rewritten.
    pub floor: &'a str,
}

/// Cuts `text` to at most `max` bytes without splitting a character.
fn truncate_utf8(text: &str, max: usize) -> (&str, bool) {
    if text.len() <= max {
        return (text, false);
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], true)
}

/// The prompt for one committer run: the context file, the run facts, and the output of the
/// skill's step 1 and step 1b commands, computed by the driver through `git2`.
/// `above` is the list of commits above the floor, oldest first, as `commits_above` returns it.
pub fn committer_prompt(
    context: &str,
    req: &CommitRequest<'_>,
    report: &StagedReport,
    above: &[CommitInfo],
    subagents: bool,
) -> String {
    let analysis = if subagents {
        "Analysis: through the skill's subagents."
    } else {
        "Analysis: inline, no subagents. Run the skill's steps 1, 1b and 1c with your own tool calls."
    };
    let (diff, cut) = truncate_utf8(&report.diff, MAX_DIFF_BYTES);
    let cut_note = if cut {
        format!("\n[diff cut at {MAX_DIFF_BYTES} bytes, run `git diff --staged` for the rest]\n")
    } else {
        String::new()
    };
    let mut candidates = String::new();
    if above.is_empty() {
        candidates.push_str("(none)\n");
    }
    for commit in above.iter().rev() {
        let short = commit.sha.get(..7).unwrap_or(&commit.sha);
        candidates.push_str(&format!("{short} {}\n", commit.subject));
        for file in &commit.files {
            candidates.push_str(&format!("  {file}\n"));
        }
    }
    format!(
        "{context}\n\n\
         ## This run\n\n\
         Kind: {kind}\n\
         Plan: {plan}\n\
         Task: {task}\n\
         Goal: {goal}\n\
         Floor: {floor}\n\
         {analysis}\n\n\
         ## Output of the skill's step 1 commands\n\n\
         Treat this as the result of running them. Do not run them again.\n\n\
         ### git status\n{status}\n\
         ### git diff --staged --stat\n{stat}\n\
         ### git diff --staged\n{diff}{cut_note}\n\
         ### git log --oneline -5\n{log}\n\n\
         ## Output of the skill's step 1b commands\n\n\
         Commits above the floor, newest first, all local only, with their files:\n\n{candidates}",
        kind = req.kind.as_str(),
        plan = req.plan_title,
        task = req.task_title,
        goal = req.goal,
        floor = req.floor,
        status = report.status,
        stat = report.stat,
        log = report.log.join("\n"),
    )
}

/// The run spec for the committer. It does not go through `Engine::with_limits`: the cap is
/// per run (`commit_budget_weighted`, nothing spent before), not the job budget.
pub fn committer_spec(job_id: &str, config: &Config, cwd: &Path, prompt: &str) -> AgentSpec {
    let mut spec = AgentSpec::new(job_id, "committer", cwd, prompt);
    spec.agent = Some(COMMITTER_AGENT.to_string());
    spec.model = Some(config.commit_model.clone());
    spec.max_turns = Some(COMMITTER_MAX_TURNS);
    spec.budget_weighted = Some(config.commit_budget_weighted);
    spec.used_before = 0;
    spec.stall_after = Some(Duration::from_secs(u64::from(config.stall_minutes) * 60));
    spec.env = vec![("HARNESS_GATE".to_string(), "0".to_string())];
    spec.strict_mcp = true;
    spec.tools = Some(COMMITTER_TOOLS.to_string());
    spec
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd tools/factory && cargo test -j4 -p factory-engine --lib commit`
Expected: PASS, 6 tests.

- [ ] **Step 6: Run the crate tests and check warnings**

Run: `cd tools/factory && cargo test -j4 -p factory-engine && cargo clippy -j4 -p factory-engine --lib -- -D warnings`
Expected: all tests pass. Clippy on `factory-engine` may report old lints from its dependency `factory-agent` (three known `map_or` lints in `claude.rs`, `runner.rs`, `stream.rs`); that is not caused by this task. If clippy stops there, run `cargo check -j4 -p factory-engine` and state the clippy result in the report.

- [ ] **Step 7: Commit**

Use the `dev-workflow-skills:1-git-commit` skill, with these paths only:

```bash
git add .claude/factory/commit-context.md tools/factory/crates/factory-engine/src/commit.rs tools/factory/crates/factory-engine/src/lib.rs
```

Expected subject: `feat(factory): build the committer prompt and run spec`.

---

### Task 3: Run the committer, verify it, retry and fall back

**Files:**
- Modify: `tools/factory/crates/factory-git/src/commits.rs` (add `message` to `CommitInfo`)
- Modify: `tools/factory/crates/factory-git/tests/commits.rs`
- Modify: `tools/factory/crates/factory-engine/src/commit.rs`
- Modify: `tools/factory/crates/factory-engine/src/lib.rs` (exports)
- Test: `tools/factory/crates/factory-engine/tests/commit.rs` (create)

**Interfaces:**
- Consumes: from plan 01 `Repo::stage_all`, `worktree_is_clean`, `commits_above`, `commit_staged`, `staged_report`, `tip`, `is_ancestor`; from task 1 `validate_message`, `fallback_subject`, `blocked_reason`, `MAX_COMMITS_PER_RUN`; from task 2 `committer_prompt`, `committer_spec`, `CommitRequest`, `COMMIT_CONTEXT_FILE`; `Engine::run_agent`, `Engine::ruling`.
- Produces:
  - `CommitInfo.message: String` (the full message), in `factory-git`.
  - `pub enum CommitOutcome { Committed { tip: String, fallback: bool }, NothingStaged, Park(String) }` (`Debug`, `PartialEq`).
  - `Engine::commit_step(&self, wt: &factory_git::Worktree, req: &CommitRequest<'_>) -> Result<CommitOutcome, EngineError>`.

- [ ] **Step 1: Add the message field to CommitInfo (git layer)**

In `tools/factory/crates/factory-git/tests/commits.rs`, add this test:

```rust
#[test]
fn commits_above_carries_the_full_message() {
    let fx = Fixture::new();
    let (repo, wt) = job_worktree(&fx);
    commit_files(&inside(&wt), &[("a.txt", "a\n")], "feat(x): add a\n\nWhy a exists.");
    let tip = repo.tip(&wt.branch).expect("tip");
    let commits = repo.commits_above(&wt.base, &tip).expect("commits");
    assert_eq!(commits[0].subject, "feat(x): add a");
    assert_eq!(commits[0].message, "feat(x): add a\n\nWhy a exists.");
}
```

Run: `cd tools/factory && cargo test -j4 -p factory-git --test commits commits_above_carries_the_full_message`
Expected: FAIL to compile, `no field message on type CommitInfo`.

In `tools/factory/crates/factory-git/src/commits.rs`, add to `pub struct CommitInfo` after `subject`:

```rust
    /// The full commit message, subject and body.
    pub message: String,
```

and in `commits_above`, in the `CommitInfo { ... }` literal after the `subject:` line:

```rust
                message: commit.message().unwrap_or("").to_string(),
```

Run: `cd tools/factory && cargo test -j4 -p factory-git`
Expected: PASS.

In the engine, update the `info` helper in the tests module of `tools/factory/crates/factory-engine/src/commit.rs` (from task 2) so it compiles: add `message: subject.to_string(),` after the `subject:` line of the `CommitInfo` literal.

- [ ] **Step 2: Write the failing engine tests**

Create `tools/factory/crates/factory-engine/tests/commit.rs`:

```rust
mod common;

use std::path::Path;

use common::*;
use factory_agent::AgentSpec;
use factory_engine::{CommitKind, CommitOutcome, CommitRequest, COMMIT_CONTEXT_FILE};
use factory_git::{JobRef, Worktree};
use git2::{Repository, Signature};

const GOOD: &str = "feat(x): add the thing\n\nThe thing was missing.";

/// Writes the prompt context file into the main checkout, where the engine reads it.
fn context(fx: &Fixture) {
    let path = fx.main.join(COMMIT_CONTEXT_FILE);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, "TEST CONTEXT\n").expect("context");
}

fn worktree(fx: &Fixture) -> Worktree {
    fx.repo.create_worktree(&JobRef { id: "001".into(), slug: "demo".into() }).expect("worktree")
}

fn request(floor: &str) -> CommitRequest<'_> {
    CommitRequest {
        kind: CommitKind::Task,
        plan_title: "demo plan",
        task_title: "Add the thing.",
        goal: "add the thing",
        floor,
    }
}

fn write_change(wt: &Worktree, name: &str) {
    std::fs::write(wt.path.join(name), "content\n").expect("write change");
}

/// Stages the given paths and commits them on HEAD, like a committer would.
fn commit_paths(dir: &Path, paths: &[&str], message: &str) -> String {
    let repo = Repository::open(dir).expect("open");
    let mut index = repo.index().expect("index");
    for p in paths {
        index.add_path(Path::new(p)).expect("add");
    }
    index.write().expect("write index");
    let tree = repo.find_tree(index.write_tree().expect("tree")).expect("find tree");
    let sig = Signature::now("t", "t@example.com").expect("sig");
    let parent = repo.head().expect("head").peel_to_commit().expect("commit");
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent]).expect("commit").to_string()
}

/// Stages the given paths and amends HEAD with them, like `git commit --amend`.
fn amend_with(dir: &Path, paths: &[&str], message: &str) -> String {
    let repo = Repository::open(dir).expect("open");
    let mut index = repo.index().expect("index");
    for p in paths {
        index.add_path(Path::new(p)).expect("add");
    }
    index.write().expect("write index");
    let tree = repo.find_tree(index.write_tree().expect("tree")).expect("find tree");
    let head = repo.head().expect("head").peel_to_commit().expect("commit");
    head.amend(Some("HEAD"), None, None, None, Some(message), Some(&tree)).expect("amend").to_string()
}

fn effect(f: impl Fn(&Path) -> String + Send + Sync + 'static) -> Effect {
    Box::new(move |spec: &AgentSpec| f(&spec.cwd))
}

fn nothing() -> Effect {
    effect(|_| "did nothing".to_string())
}

fn ledger(fx: &Fixture) -> String {
    std::fs::read_to_string(fx.job_dir.join("progress.md")).unwrap_or_default()
}

fn subjects(fx: &Fixture, wt: &Worktree, floor: &str) -> Vec<String> {
    let tip = fx.repo.tip(&wt.branch).expect("tip");
    fx.repo.commits_above(floor, &tip).expect("commits").into_iter().map(|c| c.subject).collect()
}

#[test]
fn one_valid_commit_is_accepted_and_the_run_is_trimmed_and_capped() {
    let fx = Fixture::new();
    context(&fx);
    let wt = worktree(&fx);
    write_change(&wt, "a.txt");
    let runner = scripted(vec![effect(|dir| {
        commit_paths(dir, &["a.txt"], GOOD);
        "done".to_string()
    })]);
    let out = fx.engine(&runner).commit_step(&wt, &request(&wt.base)).expect("step");
    match out {
        CommitOutcome::Committed { tip, fallback } => {
            assert!(!fallback);
            assert_eq!(tip, fx.repo.tip(&wt.branch).expect("tip"));
        }
        other => panic!("unexpected outcome {other:?}"),
    }
    assert_eq!(subjects(&fx, &wt, &wt.base), vec!["feat(x): add the thing"]);
    let spec = runner.last_spec().expect("spec");
    assert_eq!(spec.role, "committer");
    assert_eq!(spec.agent.as_deref(), Some("factory-committer"));
    assert_eq!(spec.budget_weighted, Some(60_000));
    assert_eq!(spec.used_before, 0);
    assert_eq!(spec.env, vec![("HARNESS_GATE".to_string(), "0".to_string())]);
    assert!(spec.strict_mcp);
    assert_eq!(spec.tools.as_deref(), Some("Bash,Read,Skill"));
    assert!(spec.prompt.starts_with("TEST CONTEXT"));
    assert!(spec.prompt.contains(&format!("Floor: {}", wt.base)));
    assert!(spec.prompt.contains("new file:   a.txt"));
}

#[test]
fn a_split_into_two_commits_is_accepted() {
    let fx = Fixture::new();
    context(&fx);
    let wt = worktree(&fx);
    write_change(&wt, "a.txt");
    write_change(&wt, "b.txt");
    let runner = scripted(vec![effect(|dir| {
        commit_paths(dir, &["a.txt"], "feat(x): add a");
        commit_paths(dir, &["b.txt"], "feat(x): add b");
        "done".to_string()
    })]);
    let out = fx.engine(&runner).commit_step(&wt, &request(&wt.base)).expect("step");
    assert!(matches!(out, CommitOutcome::Committed { fallback: false, .. }), "{out:?}");
    assert_eq!(subjects(&fx, &wt, &wt.base), vec!["feat(x): add a", "feat(x): add b"]);
}

#[test]
fn amending_a_commit_above_the_floor_is_accepted() {
    let fx = Fixture::new();
    context(&fx);
    let wt = worktree(&fx);
    write_change(&wt, "a.txt");
    commit_paths(&wt.path, &["a.txt"], GOOD);
    write_change(&wt, "b.txt");
    let runner = scripted(vec![effect(|dir| {
        amend_with(dir, &["b.txt"], "feat(x): add the thing and b");
        "done".to_string()
    })]);
    let out = fx.engine(&runner).commit_step(&wt, &request(&wt.base)).expect("step");
    assert!(matches!(out, CommitOutcome::Committed { fallback: false, .. }), "{out:?}");
    assert_eq!(subjects(&fx, &wt, &wt.base), vec!["feat(x): add the thing and b"]);
}

#[test]
fn rewriting_the_floor_commit_parks_at_once() {
    let fx = Fixture::new();
    context(&fx);
    let wt = worktree(&fx);
    write_change(&wt, "a.txt");
    let floor = commit_paths(&wt.path, &["a.txt"], GOOD);
    write_change(&wt, "b.txt");
    // Only one effect is scripted: a retry would find the script exhausted and fail the test.
    let runner = scripted(vec![effect(|dir| {
        amend_with(dir, &["b.txt"], GOOD);
        "done".to_string()
    })]);
    let out = fx.engine(&runner).commit_step(&wt, &request(&floor)).expect("step");
    match out {
        CommitOutcome::Park(reason) => assert!(reason.contains("at or below the floor"), "{reason}"),
        other => panic!("unexpected outcome {other:?}"),
    }
}

#[test]
fn an_uncommitted_tree_is_retried_once_by_resuming_the_session() {
    let fx = Fixture::new();
    context(&fx);
    let wt = worktree(&fx);
    write_change(&wt, "a.txt");
    let runner = scripted(vec![
        nothing(),
        effect(|dir| {
            commit_paths(dir, &["a.txt"], GOOD);
            "done".to_string()
        }),
    ]);
    let out = fx.engine(&runner).commit_step(&wt, &request(&wt.base)).expect("step");
    assert!(matches!(out, CommitOutcome::Committed { fallback: false, .. }), "{out:?}");
    let retry = runner.last_spec().expect("retry spec");
    assert_eq!(retry.resume.as_deref(), Some("sess-1"));
    assert!(retry.prompt.contains("uncommitted"), "retry prompt: {}", retry.prompt);
}

#[test]
fn two_failed_runs_fall_back_to_a_plain_commit_and_write_a_ruling() {
    let fx = Fixture::new();
    context(&fx);
    let wt = worktree(&fx);
    write_change(&wt, "a.txt");
    let runner = scripted(vec![nothing(), nothing()]);
    let out = fx.engine(&runner).commit_step(&wt, &request(&wt.base)).expect("step");
    assert!(matches!(out, CommitOutcome::Committed { fallback: true, .. }), "{out:?}");
    assert_eq!(subjects(&fx, &wt, &wt.base), vec!["chore: Add the thing"]);
    assert!(ledger(&fx).contains("fallback commit"), "ledger: {}", ledger(&fx));
}

#[test]
fn a_blocked_reply_twice_falls_back_and_names_the_reason() {
    let fx = Fixture::new();
    context(&fx);
    let wt = worktree(&fx);
    write_change(&wt, "a.txt");
    let runner = scripted(vec![effect(|_| "BLOCKED: no skill".to_string()), effect(|_| "BLOCKED: no skill".to_string())]);
    let out = fx.engine(&runner).commit_step(&wt, &request(&wt.base)).expect("step");
    assert!(matches!(out, CommitOutcome::Committed { fallback: true, .. }), "{out:?}");
    assert!(ledger(&fx).contains("BLOCKED: no skill"), "ledger: {}", ledger(&fx));
}

#[test]
fn an_invalid_subject_after_the_retry_keeps_the_commit_and_writes_a_ruling() {
    let fx = Fixture::new();
    context(&fx);
    let wt = worktree(&fx);
    write_change(&wt, "a.txt");
    let runner = scripted(vec![
        effect(|dir| {
            commit_paths(dir, &["a.txt"], "Added the thing.");
            "done".to_string()
        }),
        nothing(),
    ]);
    let out = fx.engine(&runner).commit_step(&wt, &request(&wt.base)).expect("step");
    assert!(matches!(out, CommitOutcome::Committed { fallback: false, .. }), "{out:?}");
    assert_eq!(subjects(&fx, &wt, &wt.base), vec!["Added the thing."]);
    assert!(ledger(&fx).contains("commit message check failed"), "ledger: {}", ledger(&fx));
}

#[test]
fn more_than_four_commits_are_kept_with_a_ruling() {
    let fx = Fixture::new();
    context(&fx);
    let wt = worktree(&fx);
    for name in ["a.txt", "b.txt", "c.txt", "d.txt", "e.txt"] {
        write_change(&wt, name);
    }
    let runner = scripted(vec![
        effect(|dir| {
            for name in ["a.txt", "b.txt", "c.txt", "d.txt", "e.txt"] {
                commit_paths(dir, &[name], &format!("feat(x): add {name}"));
            }
            "done".to_string()
        }),
        nothing(),
    ]);
    let out = fx.engine(&runner).commit_step(&wt, &request(&wt.base)).expect("step");
    assert!(matches!(out, CommitOutcome::Committed { fallback: false, .. }), "{out:?}");
    assert_eq!(subjects(&fx, &wt, &wt.base).len(), 5);
    assert!(ledger(&fx).contains("5 commits were made"), "ledger: {}", ledger(&fx));
}

#[test]
fn nothing_staged_makes_no_agent_call() {
    let fx = Fixture::new();
    context(&fx);
    let wt = worktree(&fx);
    let runner = scripted(vec![]);
    let out = fx.engine(&runner).commit_step(&wt, &request(&wt.base)).expect("step");
    assert_eq!(out, CommitOutcome::NothingStaged);
    assert!(runner.last_spec().is_none());
}

#[test]
fn a_missing_context_file_uses_the_fallback_without_an_agent_call() {
    let fx = Fixture::new();
    let wt = worktree(&fx);
    write_change(&wt, "a.txt");
    let runner = scripted(vec![]);
    let out = fx.engine(&runner).commit_step(&wt, &request(&wt.base)).expect("step");
    assert!(matches!(out, CommitOutcome::Committed { fallback: true, .. }), "{out:?}");
    assert!(runner.last_spec().is_none());
    assert!(ledger(&fx).contains("commit context"), "ledger: {}", ledger(&fx));
}

#[test]
fn a_valid_commit_with_a_clean_tree_is_accepted_even_if_the_run_failed() {
    let fx = Fixture::new();
    context(&fx);
    let wt = worktree(&fx);
    write_change(&wt, "a.txt");
    let runner = scripted(vec![effect(|dir| {
        commit_paths(dir, &["a.txt"], GOOD);
        "BLOCKED: gave up after committing".to_string()
    })]);
    let out = fx.engine(&runner).commit_step(&wt, &request(&wt.base)).expect("step");
    assert!(matches!(out, CommitOutcome::Committed { fallback: false, .. }), "{out:?}");
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd tools/factory && cargo test -j4 -p factory-engine --test commit`
Expected: FAIL to compile, `no method named commit_step found for struct Engine`.

- [ ] **Step 4: Write the implementation**

In `tools/factory/crates/factory-engine/src/commit.rs`, replace the `use` block at the top (the four lines `use std::path::Path;` to `use factory_git::{CommitInfo, StagedReport};`) with:

```rust
use std::path::Path;
use std::time::Duration;

use factory_agent::{AgentRun, AgentSpec, StopReason};
use factory_core::{blocked_reason, fallback_subject, validate_message, Config, MAX_COMMITS_PER_RUN};
use factory_git::{CommitInfo, StagedReport, Worktree};

use crate::ports::git_err;
use crate::{Engine, EngineError};
```

Add this below `committer_spec` and above the `tests` module:

```rust
/// What a committer step did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitOutcome {
    /// The new tip, and whether the plain fallback commit was used.
    Committed { tip: String, fallback: bool },
    /// Nothing was staged, so no committer run was started.
    NothingStaged,
    /// The run broke a rule that stops the job. The caller parks it.
    Park(String),
}

/// The result of checking the worktree after one committer run.
enum Check {
    Good(String),
    Park(String),
    RunFault(String),
    MessageFault(Vec<String>),
}

impl Check {
    /// The findings text for a retry, when the check failed in a way a retry can fix.
    fn findings(&self) -> Option<String> {
        match self {
            Check::RunFault(reason) => Some(reason.clone()),
            Check::MessageFault(problems) => Some(problems.join("\n")),
            Check::Good(_) | Check::Park(_) => None,
        }
    }
}

/// A short note on why the run itself looks wrong, or an empty string.
fn run_note(run: &AgentRun) -> String {
    if run.stop_reason != StopReason::Done {
        let detail = run.error.as_deref().map(|e| format!(": {e}")).unwrap_or_default();
        return format!("the run ended with {:?}{detail}", run.stop_reason);
    }
    if let Some(reason) = blocked_reason(&run.result_text) {
        return format!("the committer replied BLOCKED: {reason}");
    }
    if run.result_text.contains("Unknown skill") {
        return "the commit skill is not available in the run".to_string();
    }
    String::new()
}

fn retry_prompt(findings: &str) -> String {
    format!(
        "The previous committer run did not leave the worktree in the expected state.\n\
         Findings:\n{findings}\n\
         Fix this with the commit skill, using the same standing answers. Never rewrite the floor."
    )
}

impl Engine<'_> {
    /// Stages the job worktree, runs the committer agent, and checks the result against the
    /// rules in the spec. A failed run gets one retry by resuming the session, then a plain
    /// `git2` fallback commit. Rewriting the floor is returned as `Park`, never retried.
    /// The step does not change the job stage: the caller parks on `Park`.
    pub fn commit_step(&self, wt: &Worktree, req: &CommitRequest<'_>) -> Result<CommitOutcome, EngineError> {
        let staged = self.repo.stage_all(wt).map_err(git_err)?;
        if staged.is_empty() {
            return Ok(CommitOutcome::NothingStaged);
        }
        let tip = self.repo.tip(&wt.branch).map_err(git_err)?;
        let above = self.repo.commits_above(req.floor, &tip).map_err(git_err)?;
        let before: Vec<String> = above.iter().map(|c| c.sha.clone()).collect();
        let context = match std::fs::read_to_string(self.repo.root().join(COMMIT_CONTEXT_FILE)) {
            Ok(text) => text,
            Err(e) => return self.fallback_commit(wt, req, &format!("commit context unreadable: {e}")),
        };
        let report = self.repo.staged_report(wt, 5).map_err(git_err)?;
        let prompt = committer_prompt(&context, req, &report, &above, self.config.commit_subagents);

        let mut run = self.run_agent(committer_spec(&self.job_id, &self.config, &wt.path, &prompt))?;
        let mut check = self.verify_commit_run(wt, req, &before, &run)?;
        if let Some(findings) = check.findings() {
            let retry = retry_prompt(&findings);
            // Without a session to resume, the retry has to carry the whole prompt.
            let text = if run.session_id.is_some() { retry } else { format!("{prompt}\n\n{retry}") };
            let mut spec = committer_spec(&self.job_id, &self.config, &wt.path, &text);
            spec.resume = run.session_id.clone();
            run = self.run_agent(spec)?;
            check = self.verify_commit_run(wt, req, &before, &run)?;
        }
        match check {
            Check::Good(tip) => Ok(CommitOutcome::Committed { tip, fallback: false }),
            Check::Park(reason) => Ok(CommitOutcome::Park(reason)),
            Check::RunFault(reason) => self.fallback_commit(wt, req, &reason),
            Check::MessageFault(problems) => {
                for problem in &problems {
                    self.ruling(&format!("commit message check failed, {problem}"))?;
                }
                let tip = self.repo.tip(&wt.branch).map_err(git_err)?;
                Ok(CommitOutcome::Committed { tip, fallback: false })
            }
        }
    }

    /// Checks the worktree after a run. Git state decides: a valid commit with a clean tree is
    /// accepted even when the reply says BLOCKED. The driver never trusts the prompt for the floor.
    fn verify_commit_run(
        &self,
        wt: &Worktree,
        req: &CommitRequest<'_>,
        before: &[String],
        run: &AgentRun,
    ) -> Result<Check, EngineError> {
        let tip = self.repo.tip(&wt.branch).map_err(git_err)?;
        if !self.repo.is_ancestor(req.floor, &tip).map_err(git_err)? {
            return Ok(Check::Park("committer rewrote history at or below the floor".to_string()));
        }
        let dirty = !self.repo.worktree_is_clean(wt).map_err(git_err)?;
        let now = self.repo.commits_above(req.floor, &tip).map_err(git_err)?;
        let changed: Vec<&CommitInfo> = now.iter().filter(|c| !before.contains(&c.sha)).collect();
        if dirty || changed.is_empty() {
            let mut notes = Vec::new();
            let note = run_note(run);
            if !note.is_empty() {
                notes.push(note);
            }
            if dirty {
                notes.push("changes are still uncommitted in the worktree".to_string());
            }
            if changed.is_empty() {
                notes.push("no commit was made".to_string());
            }
            return Ok(Check::RunFault(notes.join("; ")));
        }
        let mut problems = Vec::new();
        if changed.len() > MAX_COMMITS_PER_RUN {
            problems.push(format!("{} commits were made, the limit is {MAX_COMMITS_PER_RUN}", changed.len()));
        }
        for commit in &changed {
            if let Err(reason) = validate_message(&commit.message) {
                let short = commit.sha.get(..7).unwrap_or(&commit.sha);
                problems.push(format!("commit {short}: {reason}"));
            }
        }
        if problems.is_empty() {
            Ok(Check::Good(tip))
        } else {
            Ok(Check::MessageFault(problems))
        }
    }

    /// The plain `git2` commit used when the committer failed twice: `chore: <task title>`.
    /// Parks when there is nothing left to commit, because then the run lost the changes.
    fn fallback_commit(&self, wt: &Worktree, req: &CommitRequest<'_>, why: &str) -> Result<CommitOutcome, EngineError> {
        let staged = self.repo.stage_all(wt).map_err(git_err)?;
        if staged.is_empty() {
            return Ok(CommitOutcome::Park(format!("the committer left nothing to commit: {why}")));
        }
        let sha = self.repo.commit_staged(wt, &fallback_subject(req.task_title)).map_err(git_err)?;
        let short = sha.get(..7).unwrap_or(&sha);
        self.ruling(&format!("committer failed ({why}), fallback commit {short}"))?;
        Ok(CommitOutcome::Committed { tip: sha, fallback: true })
    }
}
```

In `tools/factory/crates/factory-engine/src/lib.rs`, add `CommitOutcome` to the `pub use commit::{...}` line from task 2.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd tools/factory && cargo test -j4 -p factory-engine --test commit && cargo test -j4 -p factory-engine --lib commit`
Expected: PASS, 11 integration tests and the 6 unit tests from task 2.

- [ ] **Step 6: Run the affected crates and check warnings**

Run: `cd tools/factory && cargo test -j4 -p factory-git && cargo test -j4 -p factory-engine && cargo check -j4`
Expected: all tests pass and the workspace checks. Run `cargo clippy -j4 -p factory-git -- -D warnings`; expected no output. For `factory-engine`, clippy may stop on the three known old lints in `factory-agent`; say so in the report if it does.

- [ ] **Step 7: Commit**

Use the `dev-workflow-skills:1-git-commit` skill, with these paths only:

```bash
git add tools/factory/crates/factory-git/src/commits.rs tools/factory/crates/factory-git/tests/commits.rs tools/factory/crates/factory-engine/src/commit.rs tools/factory/crates/factory-engine/src/lib.rs tools/factory/crates/factory-engine/tests/commit.rs
```

Expected subject: `feat(factory): run the committer, verify it and fall back`.

Next plan: `plan-04-build-flow.md`, written after this plan is merged.
