# Factory driver commits, plan 05: merge and sync messages, docs, rollout

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The merge commit and the sync commit get their message from the committer agent in a read-only message-only mode, with the old fixed message as the fallback. The docs, the harness rule and the `/factory` command match the new flow.

**Architecture:** `merge_no_ff` and `merge_main_into` take the message before the commit exists, so the agent only drafts text. A new `message.rs` in `factory-engine` builds the prompt from facts the driver computes through `git2`, runs the agent with read-only tools, extracts the text between markers, checks it with `validate_message`, retries once by resuming, then falls back to `chore(factory): merge job <id> <slug>` or `chore(factory): sync main into job <id> <slug>`. A bad message never parks a job. `Driver::commit_message` is the entry point for `merge_stage` and `sync_stage`.

**Tech Stack:** Rust, `factory-core`, `factory-engine`, `factory-agent`, markdown docs.

**Spec:** `docs/superpowers/specs/2026-10-14-factory-driver-commits-design.md` (decisions 1 to 16, merge and sync parts). Plans 01 to 04 are merged. The spec's section 5 names a `Committer` port, which the series replaced with the existing `AgentRunner` port. Task 3 fixes that wording.

## Global Constraints

- Harness: `cargo ... -j4`, never `--workspace` or `--all`, never set `CARGO_TARGET_DIR`. Run from `tools/factory/`.
- Explicit `git add <paths>` and a pathspec commit. Every commit goes through `dev-workflow-skills:1-git-commit`.
- Board JSON stays compatible with `factory-server`. This plan adds no board fields.
- Message-only runs change nothing in git. Tools are `Read,Skill` (`--tools`) and the same two in `allowed_tools`, with `HARNESS_GATE=0` and `strict_mcp = true`.
- The usage role is `message`, not `committer`. The test fake commits the index for every `committer` run, and a message run must never match it.
- A message problem never parks a job. Only the existing merge and sync rules park.
- The commit-message validator is `factory_core::validate_message`, unchanged. The footer rules (`Fixes: #N`) are plan 06.

## Review Focus

- The reply has no markers, or two marker pairs: use the first pair, and with none retry once, then fall back.
- The reply message has a subject over 72 characters or ends in a period: treated like a missing message.
- The agent run is stopped or the runner fails: fall back, merge still happens.
- The agent output contains the string `---END MESSAGE---` inside the body: the first end marker after the begin marker wins.
- Sync and merge facts with zero commits above the base (a job with only a merge commit): the prompt says `(none)` and the run still works.

## File map

- Modify `crates/factory-core/src/commitmsg.rs`: add `extract_message`.
- Create `crates/factory-engine/src/message.rs`: `MessageKind`, `MessageRequest`, `message_prompt`, `message_spec`, `fixed_message`, `Engine::message_step`.
- Modify `crates/factory-engine/src/compat.rs`: `Driver::commit_message`.
- Modify `crates/factory-engine/src/lib.rs`: module and exports.
- Modify `crates/factory-engine/src/merge.rs` and `sync.rs`: use `commit_message`.
- Modify `.claude/agents/factory-committer.md` and `crates/factory-cli/tests/agent_lint.rs`: message-only wording and pins.
- Create `crates/factory-engine/tests/message.rs`; adjust `tests/merge.rs`.
- Docs: `.claude/software-factory.md`, `.claude/rules/harness.md`, `.claude/commands/factory.md`, the spec section 5.

---

### Task 1: The message-only step

**Files:**
- Modify: `crates/factory-core/src/commitmsg.rs`
- Create: `crates/factory-engine/src/message.rs`
- Modify: `crates/factory-engine/src/lib.rs`, `crates/factory-engine/src/compat.rs`
- Modify: `.claude/agents/factory-committer.md`, `crates/factory-cli/tests/agent_lint.rs`
- Test: unit tests in `commitmsg.rs` and `message.rs`

**Interfaces:**
- Consumes: `validate_message(&str) -> Result<(), String>`, `AgentSpec`, `Engine::run_agent`, `Engine::ruling`, `CommitInfo`, `Repo::commits_above(floor, tip)`, `Repo::tip`, `COMMITTER_AGENT`, `COMMITTER_MAX_TURNS`, `COMMIT_CONTEXT_FILE`.
- Produces:
  - `factory_core::extract_message(reply: &str) -> Option<String>`.
  - `MessageKind { Merge, Sync }` with `as_str`.
  - `MessageRequest<'a> { kind, job_id, slug, title, base, tip }` (all `&str` except `kind`). `base` is the commit the job started from, `tip` the commit being merged (merge: the reviewed head, sync: main's tip).
  - `fixed_message(kind, job_id, slug) -> String`.
  - `message_prompt(context, req, commits) -> String` and `message_spec(job_id, config, cwd, prompt) -> AgentSpec`.
  - `Engine::message_step(&self, cwd: &Path, req: &MessageRequest) -> Result<String, EngineError>`: always returns a valid message.
  - `Driver::commit_message(&self, id: &str, kind: MessageKind) -> Result<String, EngineError>`.

- [ ] **Step 1: Write the failing tests for `extract_message`**

In `crates/factory-core/src/commitmsg.rs` tests module:

```rust
#[test]
fn extract_message_takes_the_first_marker_pair() {
    let reply = "noise\n---BEGIN MESSAGE---\nfeat(x): add y\n\nbody line\n---END MESSAGE---\n---BEGIN MESSAGE---\nother\n---END MESSAGE---";
    assert_eq!(extract_message(reply).as_deref(), Some("feat(x): add y\n\nbody line"));
}

#[test]
fn extract_message_needs_both_markers_and_text() {
    assert_eq!(extract_message("feat(x): add y"), None);
    assert_eq!(extract_message("---BEGIN MESSAGE---\nfeat(x): y"), None);
    assert_eq!(extract_message("---BEGIN MESSAGE---\n  \n---END MESSAGE---"), None);
}
```

- [ ] **Step 2: Run, expect FAIL** (`extract_message` not defined)

Run: `cargo test -j4 -p factory-core extract_message`

- [ ] **Step 3: Implement**

```rust
pub const MESSAGE_BEGIN: &str = "---BEGIN MESSAGE---";
pub const MESSAGE_END: &str = "---END MESSAGE---";

/// The text between the first begin marker and the first end marker after it, trimmed.
/// `None` when a marker is missing or the text is empty.
pub fn extract_message(reply: &str) -> Option<String> {
    let start = reply.find(MESSAGE_BEGIN)? + MESSAGE_BEGIN.len();
    let rest = &reply[start..];
    let end = rest.find(MESSAGE_END)?;
    let text = rest[..end].trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}
```

Export `extract_message`, `MESSAGE_BEGIN`, `MESSAGE_END` from `factory-core/src/lib.rs` next to `validate_message`.

- [ ] **Step 4: Run, expect PASS**

- [ ] **Step 5: Write the failing tests for `message.rs`**

Unit tests in `message.rs` (a `#[cfg(test)]` module):

```rust
#[test]
fn fixed_messages_match_the_old_driver_text() {
    assert_eq!(fixed_message(MessageKind::Merge, "001", "hello"), "chore(factory): merge job 001 hello");
    assert_eq!(fixed_message(MessageKind::Sync, "001", "hello"), "chore(factory): sync main into job 001 hello");
}

#[test]
fn prompt_says_no_floor_and_no_commit_and_lists_the_commits() {
    let req = MessageRequest { kind: MessageKind::Merge, job_id: "001", slug: "hello", title: "hello feature", base: "abc1234", tip: "def5678" };
    let commits = vec![CommitInfo { sha: "aaaaaaa1111".into(), subject: "feat(x): one".into(), message: "feat(x): one".into(), files: vec!["a.txt".into()] }];
    let prompt = message_prompt("CONTEXT", &req, &commits);
    assert!(prompt.starts_with("CONTEXT"));
    for want in ["Kind: merge", "Job: 001 hello", "Title: hello feature", "aaaaaaa feat(x): one", "  a.txt", "no floor", "Do not run git commit", "---BEGIN MESSAGE---", "---END MESSAGE---"] {
        assert!(prompt.contains(want), "missing {want}");
    }
}

#[test]
fn prompt_with_no_commits_says_none() {
    let req = MessageRequest { kind: MessageKind::Sync, job_id: "001", slug: "hello", title: "t", base: "a", tip: "b" };
    assert!(message_prompt("c", &req, &[]).contains("(none)"));
}

#[test]
fn spec_is_read_only_and_capped_per_run() {
    let mut config = Config::default();
    config.commit_model = "opus".into();
    config.commit_budget_weighted = 777;
    let spec = message_spec("001", &config, Path::new("/wt"), "p");
    assert_eq!(spec.role, "message");
    assert_eq!(spec.agent.as_deref(), Some("factory-committer"));
    assert_eq!(spec.model.as_deref(), Some("opus"));
    assert_eq!(spec.budget_weighted, Some(777));
    assert_eq!(spec.used_before, 0);
    assert_eq!(spec.tools.as_deref(), Some("Read,Skill"));
    assert_eq!(spec.allowed_tools, vec!["Read".to_string(), "Skill".to_string()]);
    assert_eq!(spec.env, vec![("HARNESS_GATE".to_string(), "0".to_string())]);
    assert!(spec.strict_mcp);
}
```

- [ ] **Step 6: Run, expect FAIL** (module missing). Run: `cargo test -j4 -p factory-engine --lib message`

- [ ] **Step 7: Implement `message.rs`**

```rust
//! The message-only committer mode: the agent drafts the merge or sync message, the driver commits it.

use std::path::Path;
use std::time::Duration;

use factory_agent::AgentSpec;
use factory_core::{extract_message, validate_message, Config, MESSAGE_BEGIN, MESSAGE_END};
use factory_git::CommitInfo;

use crate::{Engine, EngineError, COMMITTER_AGENT, COMMITTER_MAX_TURNS};

/// The tools a message run may use. It reads files and runs the skill, nothing else.
pub const MESSAGE_TOOLS: &str = "Read,Skill";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Merge,
    Sync,
}

impl MessageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MessageKind::Merge => "merge",
            MessageKind::Sync => "sync",
        }
    }
}

/// What the driver tells the message run.
#[derive(Debug, Clone)]
pub struct MessageRequest<'a> {
    pub kind: MessageKind,
    pub job_id: &'a str,
    pub slug: &'a str,
    pub title: &'a str,
    /// The commit the job started from.
    pub base: &'a str,
    /// The commit that is merged: the reviewed head (merge) or main's tip (sync).
    pub tip: &'a str,
}

/// The message the driver used before the committer existed. It is the fallback.
pub fn fixed_message(kind: MessageKind, job_id: &str, slug: &str) -> String {
    match kind {
        MessageKind::Merge => format!("chore(factory): merge job {job_id} {slug}"),
        MessageKind::Sync => format!("chore(factory): sync main into job {job_id} {slug}"),
    }
}

/// The prompt of one message run. `commits` is the list above `base`, oldest first.
pub fn message_prompt(context: &str, req: &MessageRequest<'_>, commits: &[CommitInfo]) -> String {
    let mut listed = String::new();
    if commits.is_empty() {
        listed.push_str("(none)\n");
    }
    for commit in commits.iter().rev() {
        let short = commit.sha.get(..7).unwrap_or(&commit.sha);
        listed.push_str(&format!("{short} {}\n", commit.subject));
        for file in &commit.files {
            listed.push_str(&format!("  {file}\n"));
        }
    }
    let what = match req.kind {
        MessageKind::Merge => "the merge commit that brings the job branch into main",
        MessageKind::Sync => "the merge commit that brings main into the job branch",
    };
    format!(
        "{context}\n\n\
         ## This run\n\n\
         Kind: {kind}\n\
         Job: {id} {slug}\n\
         Title: {title}\n\
         Base: {base}\n\
         Tip: {tip}\n\n\
         Write the message for {what}. This is a message-only run: there is no floor, no staged change and no issue footer, the driver adds any footer itself. \
         Do not run git commit, git add or any other git command, and do not change any file. Use the commit skill's style rules, answer its questions with the standing answers, and follow the Conventional Commits style from the context.\n\n\
         ## Commits on the branch above the base, newest first\n\n{listed}\n\
         ## Reply\n\n\
         Reply with the message between {begin} and {end} and no other text.\n\n\
         {begin}\n<subject>\n\n<optional body>\n{end}\n",
        kind = req.kind.as_str(),
        id = req.job_id,
        slug = req.slug,
        title = req.title,
        base = req.base,
        tip = req.tip,
        begin = MESSAGE_BEGIN,
        end = MESSAGE_END,
    )
}

/// The run spec of a message run: read-only tools, a per-run cap, role `message`.
pub fn message_spec(job_id: &str, config: &Config, cwd: &Path, prompt: &str) -> AgentSpec {
    let mut spec = AgentSpec::new(job_id, "message", cwd, prompt);
    spec.agent = Some(COMMITTER_AGENT.to_string());
    spec.model = Some(config.commit_model.clone());
    spec.max_turns = Some(COMMITTER_MAX_TURNS);
    spec.budget_weighted = Some(config.commit_budget_weighted);
    spec.used_before = 0;
    spec.stall_after = Some(Duration::from_secs(u64::from(config.stall_minutes) * 60));
    spec.env = vec![("HARNESS_GATE".to_string(), "0".to_string())];
    spec.strict_mcp = true;
    spec.tools = Some(MESSAGE_TOOLS.to_string());
    spec.allowed_tools = MESSAGE_TOOLS.split(',').map(str::to_string).collect();
    spec
}

impl Engine<'_> {
    /// Asks the committer for a merge or sync message. A missing or invalid message gets one
    /// retry by resuming the session, then the fixed message. This never parks and never fails
    /// on a message problem, only on a board or git error.
    pub fn message_step(&self, cwd: &Path, req: &MessageRequest<'_>) -> Result<String, EngineError> {
        let fixed = fixed_message(req.kind, req.job_id, req.slug);
        let context = match std::fs::read_to_string(self.repo.root().join(crate::COMMIT_CONTEXT_FILE)) {
            Ok(text) => text,
            Err(e) => {
                self.ruling(&format!("{} message: commit context unreadable ({e}), fixed message used", req.kind.as_str()))?;
                return Ok(fixed);
            }
        };
        let commits = self.repo.commits_above(req.base, req.tip).map_err(crate::ports::git_err)?;
        let prompt = message_prompt(&context, req, &commits);

        let run = self.run_agent(message_spec(&self.job_id, &self.config, cwd, &prompt))?;
        let first = pick(&run.result_text);
        let problem = match first {
            Ok(message) => return Ok(message),
            Err(problem) => problem,
        };
        let retry = format!(
            "The previous reply cannot be used: {problem}\nReply again with only the message between {MESSAGE_BEGIN} and {MESSAGE_END}. The subject is type(scope): description, lowercase type, 72 characters or fewer, no trailing period."
        );
        let text = if run.session_id.is_some() { retry } else { format!("{prompt}\n\n{retry}") };
        let mut spec = message_spec(&self.job_id, &self.config, cwd, &text);
        spec.resume = run.session_id.clone();
        let run = self.run_agent(spec)?;
        match pick(&run.result_text) {
            Ok(message) => Ok(message),
            Err(problem) => {
                self.ruling(&format!("{} message failed twice ({problem}), fixed message used", req.kind.as_str()))?;
                Ok(fixed)
            }
        }
    }
}

/// The valid message in a reply, or the reason there is none.
fn pick(reply: &str) -> Result<String, String> {
    let message = extract_message(reply).ok_or_else(|| "no message between the markers".to_string())?;
    validate_message(&message)?;
    Ok(message)
}
```

Check `AgentSpec.allowed_tools` really is `Vec<String>` (it is, `spec.rs` line 34) and that `Repo::root()` exists (it does, `commit.rs` uses it). Add `mod message;` in `lib.rs` and export `fixed_message, message_prompt, message_spec, MessageKind, MessageRequest, MESSAGE_TOOLS`.

In `compat.rs` add:

```rust
/// Asks the committer for the message of the merge (`Merge`) or the sync (`Sync`) commit of a job.
/// Always returns a valid message, falling back to the fixed text.
pub fn commit_message(&self, id: &str, kind: crate::MessageKind) -> Result<String, EngineError> {
    let job = self.job(id)?;
    let base = job.base.clone().unwrap_or_default();
    let tip = match kind {
        crate::MessageKind::Merge => job.head.clone().unwrap_or_default(),
        crate::MessageKind::Sync => self.repo.tip("main")?,
    };
    let cwd = job.worktree.as_ref().map(PathBuf::from).unwrap_or_else(|| self.root.clone());
    self.with_engine(id, |e| {
        e.message_step(&cwd, &crate::MessageRequest {
            kind,
            job_id: &job.id,
            slug: &job.slug,
            title: &job.feature,
            base: &base,
            tip: &tip,
        })
    })
}
```

`Job` has no `title`, its one-line description is `feature`. For a sync the commits above the base list is `commits_above(base, main tip)`, which are the commits main gained; for a merge it is the job's own commits.

- [ ] **Step 8: Agent file and lint pins**

In `.claude/agents/factory-committer.md` replace the last paragraph with:

```
Merge and sync runs are message-only. The prompt says there is no floor, no staged change and no issue footer for them. Change nothing, run no git command, and reply with the message between `---BEGIN MESSAGE---` and `---END MESSAGE---` and no other text.
```

In `agent_lint.rs`, in the committer pins add `"message-only"` and `"---BEGIN MESSAGE---"` and keep every existing pin.

- [ ] **Step 9: Run**

Run: `cargo test -j4 -p factory-core`, `cargo test -j4 -p factory-engine --lib`, `cargo test -j4 -p factory-cli --test agent_lint`
Expected: PASS. `cargo clippy -j4 -p factory-core -- -D warnings` clean. `cargo check -j4` clean.

- [ ] **Step 10: Commit** with `dev-workflow-skills:1-git-commit` (`feat(factory): draft merge and sync messages with the committer`), explicit paths, pathspec.

---

### Task 2: Wire merge and sync

**Files:**
- Modify: `crates/factory-engine/src/merge.rs:81`, `crates/factory-engine/src/sync.rs:28`
- Create: `crates/factory-engine/tests/message.rs`
- Modify: `crates/factory-engine/tests/merge.rs` only if a fixture needs a reply script

**Interfaces:**
- Consumes: `Driver::commit_message`, `MessageKind`, `fixed_message`.
- Produces: merge and sync commits whose messages come from the committer.

- [ ] **Step 1: Write the failing tests** in `tests/message.rs`, using `common::approved_job()` (a driver, a runner, a job at approve). Read `common::approved_job` and `MergeFixture` first to see how the runner is built, and add a helper there if a scripted message reply needs a different runner (keep `approved_job` unchanged).

Tests:
1. `merge_uses_the_committer_message`: the runner replies `---BEGIN MESSAGE---\nfeat(hello): add the hello docs\n\nBody.\n---END MESSAGE---` to the first `message` run. Assert the merge commit's message starts with `feat(hello): add the hello docs`, the second parent is `fx.head`, and `runner.specs_for("message")` has exactly one spec with `tools == Some("Read,Skill")`.
2. `an_invalid_reply_retries_once_then_uses_the_fixed_message`: two replies without markers. Assert the merge subject is `chore(factory): merge job 001 hello`, two `message` specs, the second has `resume` set, and the ledger holds `message failed twice`.
3. `a_valid_retry_is_used`: first reply `no markers`, second a valid message. Assert that message is used and the ledger has no `failed twice`.
4. `a_failed_run_still_merges`: the runner is exhausted (use `scripted(vec![])`). Assert `MergeResult::Merged` and the fixed subject.
5. `sync_uses_the_committer_message`: build the main-moved case the way `tests/merge.rs` does for the sync path (read that file for the existing sync test). Script the message reply first, then the reviewer reply the sync needs. Assert the sync merge commit on the branch carries the message and that a `message` run was made with `Kind: sync` in its prompt.
6. `a_message_run_never_commits_the_index`: with `scripted_auto`, assert the branch tip is unchanged by a `message` run (the role is `message`, not `committer`).

- [ ] **Step 2: Run, expect FAIL.** `cargo test -j4 -p factory-engine --test message`

- [ ] **Step 3: Implement**

`merge.rs`, replace the `format!` line:

```rust
let message = driver.commit_message(id, MessageKind::Merge)?;
```

`merge_no_ff` is then called with `&message`. Import `MessageKind` from `crate`. In `sync.rs`:

```rust
let message = driver.commit_message(id, MessageKind::Sync)?;
```

A message error is a board or git error and goes through the existing `Err` path (`merge failed: ...` parks), as before.

- [ ] **Step 4: Run all engine and CLI tests that touch merge**

Run: `cargo test -j4 -p factory-engine`, `cargo test -j4 -p factory-cli --test e2e`, `cargo test -j4 -p factory-cli --test new_to_merged`.
Expected: PASS. The existing assertions on `chore(factory): merge job 001 hello` still hold through the fallback; if a CLI e2e fake runner replies something that is not a message to the `message` role, that is the fallback working, not a failure. If a CLI fake breaks on the unknown role, extend its role handling to return `done` for `message` and say so in the ledger.

- [ ] **Step 5: Commit** (`feat(factory): use the committer for merge and sync messages`).

---

### Task 3: Docs, rules and the spec wording

**Files:**
- Modify: `.claude/software-factory.md`, `.claude/rules/harness.md`, `.claude/commands/factory.md`
- Modify: `docs/superpowers/specs/2026-10-14-factory-driver-commits-design.md` (section 5)
- Modify: `crates/factory-cli/tests/e2e.rs` only to add the `message` role to the expected role sequence if the e2e asserts it

**Interfaces:** none. Docs only plus one test pin.

- [ ] **Step 1: `.claude/software-factory.md`.** Read the whole file. Rewrite the places that still say the builder commits (around lines 44, 80, 99 and 168 at the time of writing; search for `commit`, `1-git-commit`, `pathspec` and `made no commit`):
  - The builder never commits. The driver stages everything not ignored and runs the `factory-committer` agent (skill `dev-workflow-skills:1-git-commit`), once per task in a task plan, once per round otherwise.
  - Floor: the job base in round 1, the previous head in a fix round (after a red-check round the floor is the unreviewed head, which is stricter and safe). The committer may amend or fixup above the floor and never at or below it.
  - Checks after each run: floor is an ancestor, tree clean, at most 4 net-new commits, subject valid, HEAD on the job branch. One retry by resuming, then a `chore: <task title>` fallback commit. Rewriting the floor parks the job, a message problem never does.
  - Merge and sync messages come from the committer in message-only mode (role `message`, tools `Read,Skill`), with the fixed message as fallback.
  - Parks: `builder committed on its own (the branch was at <sha7> before the run)`, `builder made no changes`, `task N changed nothing`. Recovery for the first: in the job worktree run `git reset --soft <sha7>` (the sha is in the park reason), then `factory rework <id>`.
  - Tokens: the new roles `committer` and `message`, each capped per run by `commit_budget_weighted`.
  - Config keys: `commit_model`, `commit_subagents`, `commit_budget_weighted`.
- [ ] **Step 2: `.claude/rules/harness.md`.** Under the hooks list add one bullet: factory builders never commit; the driver's committer commits, amends only above the floor, and runs with `HARNESS_GATE=0` (the hook gate is skipped because the driver already ran the checks). Keep the file short.
- [ ] **Step 3: `.claude/commands/factory.md`.** Read the plan rules (lines near 43 to 46). Tell `writing-plans` that plans written for the factory carry no commit steps, because the driver commits, and that a plan that still has them is harmless but wasteful because the builder is told to skip every commit step.
- [ ] **Step 4: Spec section 5.** Replace the `Committer` port wording with: the engine reuses the `AgentRunner` port, tests use `ScriptedRunner`, and the message-only mode is `message.rs`. Do not change any decision text.
- [ ] **Step 5: Check.** Run `cargo test -j4 -p factory-cli --test agent_lint` and `grep -n "builder commits\|made no commit\|git commit" .claude/software-factory.md .claude/commands/factory.md` and confirm no stale claim is left. List any remaining match in the report with a reason.
- [ ] **Step 6: Commit** (`docs(factory): describe driver-made commits`), explicit paths, pathspec.

---

## Not a task: the real run

After the plan merges, one real factory run on a throwaway plan is the first test against a real `claude`. The user runs `factory new` and `factory run` and watches. Checks: the committer is found with `--agent factory-committer`, the skill loads (`Unknown skill` in a reply means it does not), `HARNESS_GATE=0` is honoured, commit messages pass the validator, merge messages come from the `message` role, `factory tokens` shows the two new roles. A failure there becomes a fix in this plan's branch or a note in plan 06.

Next plan: `plan-06-issue-refs-and-kb-read.md`, written against the signatures this plan produced.
