# Factory Ticket Hygiene, Plan 3: The `factory tickets` Command and the Docs

> **For agentic workers:** This plan is executed by a `factory-builder` subagent through `superpowers:executing-plans`, skipping that skill's final whole-branch review, ledger scripts and progress file. Steps use checkbox (`- [ ]`) syntax. The driver commits, so there are no commit steps.

**Goal:** Add the `factory tickets <id> [--dry-run]` command, and document the `## Follow-ups` section, the command, the `tickets_on_merge` key and the footer rule.

**Architecture:** `factory-cli` gets a `Tickets` subcommand that calls `factory_engine::tickets_step` and prints its lines; a failed step is an error so the exit code is non-zero. `CliCtx` can carry an injected tracker for tests. The documentation lives in `.claude/software-factory.md`, `.claude/commands/factory.md` and `tools/factory/CLAUDE.md`.

**Tech Stack:** Rust (`clap`, the `tools/factory` workspace) and Markdown. Cargo commands run from `tools/factory/`.

**Spec:** `docs/superpowers/specs/2026-10-17-factory-ticket-hygiene-design.md`

## Global Constraints

- Run only `cargo check -j4 -p <crate>` and `cargo test -j4 -p <crate> <filter>` from `tools/factory/`. Never a bare `cargo check -j4`, never `--workspace`, never `--all`.
- No unwrap calls in production code (tests may use `expect`). Comments are short full sentences ending with a period. No emojis.
- Tests never call the real `gh`.
- Docs go in `.claude/` and stay short. Do not edit the root `CLAUDE.md`.
- Ruling: the spec's docs section names the spec rule and the `#N` request for `/factory`. This plan also adds `tickets <id>` to the `/factory` subcommand list, because the user wants to run the step from the same front end. It is one call and a one-line report, like the other subcommands.

## Review Focus

- `factory tickets <id>` on a job that is not merged exits zero with a `skipped:` line and makes no tracker call. Pinned by `tickets_on_an_unmerged_job_skips_without_calls` in `factory-cli/tests/tickets.rs`.
- A failed step exits non-zero and the message holds every line printed so far. Pinned by `a_failed_report_is_an_error_with_all_lines`.
- `--dry-run` is parsed and `tickets` needs an id. Pinned by `tickets_parses_dry_run` and `tickets_needs_an_id`.
- The docs name the section, the command, the key and the footer rule. Pinned by the `grep` checks in Task 2 step 4.

---

### Task 1: The tickets subcommand

**Files:**
- Modify: `tools/factory/crates/factory-cli/src/cli.rs` (the `Tickets` command and parse tests)
- Modify: `tools/factory/crates/factory-cli/src/commands.rs` (`CliCtx.tracker`, `with_tracker`, `driver()`, `dispatch`, `tickets`, `render_tickets`)
- Create: `tools/factory/crates/factory-cli/tests/tickets.rs`

**Interfaces:**
- Consumes: `factory_engine::{tickets_step, TicketsReport, Tracker}`.
- Produces: `Command::Tickets { id: String, dry_run: bool }`, `CliCtx::with_tracker(self, Arc<dyn Tracker>) -> CliCtx`, and `fn render_tickets(report: TicketsReport, dry_run: bool) -> Result<String, CliError>` (private to `commands.rs`, tested inside the module).

- [ ] **Step 1: Write the failing tests.** In the `tests` module of `cli.rs` add:

```rust
    #[test]
    fn tickets_parses_dry_run() {
        let cli = Cli::try_parse_from(["factory", "tickets", "012", "--dry-run"]).expect("parses");
        assert!(matches!(cli.command, Command::Tickets { ref id, dry_run: true } if id == "012"));
        let cli = Cli::try_parse_from(["factory", "tickets", "012"]).expect("parses");
        assert!(matches!(cli.command, Command::Tickets { dry_run: false, .. }));
    }

    #[test]
    fn tickets_needs_an_id() {
        let err = Cli::try_parse_from(["factory", "tickets"]).expect_err("needs an id");
        assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
    }
```

At the bottom of `commands.rs` add (or extend) a `#[cfg(test)] mod tests`:

```rust
#[cfg(test)]
mod tickets_tests {
    use super::*;
    use factory_engine::TicketsReport;

    fn report(lines: &[&str], failed: bool, skipped: bool) -> TicketsReport {
        TicketsReport { lines: lines.iter().map(|s| s.to_string()).collect(), failed, skipped }
    }

    #[test]
    fn a_good_report_prints_its_lines() {
        let text = render_tickets(report(&["created #201: A", "closed #76"], false, false), false).expect("ok");
        assert_eq!(text, "created #201: A\nclosed #76");
    }

    #[test]
    fn a_dry_run_says_so() {
        let text = render_tickets(report(&["would close #76"], false, false), true).expect("ok");
        assert!(text.starts_with("dry run, nothing was changed"), "{text}");
        assert!(text.contains("would close #76"));
    }

    #[test]
    fn a_skipped_report_is_not_an_error() {
        let text = render_tickets(report(&["skipped: job 001 is not merged"], false, true), false).expect("ok");
        assert!(text.contains("skipped"));
    }

    #[test]
    fn a_failed_report_is_an_error_with_all_lines() {
        let err = render_tickets(report(&["created #201: A", "failed: comment on #76: boom"], true, false), false)
            .expect_err("failed");
        let message = err.to_string();
        assert!(message.contains("created #201: A") && message.contains("comment on #76: boom"), "{message}");
    }
}
```

Create `tests/tickets.rs` in `factory-cli`. First read `tests/new_to_merged.rs` to see how a job is created through `dispatch` (the `CliCtx::new(root, runner, owner)` helper and the repo setup in `tests/common/mod.rs`), and use the same setup:

```rust
mod common;
use std::sync::Arc;

use factory_engine::Tracker;

/// A tracker that fails the test when any method is called.
struct NoCalls;

impl Tracker for NoCalls {
    fn repo_name(&self) -> Result<String, String> {
        panic!("repo_name was called")
    }
    fn labels_of(&self, _: u32) -> Result<Vec<String>, String> {
        panic!("labels_of was called")
    }
    fn is_closed(&self, _: u32) -> Result<bool, String> {
        panic!("is_closed was called")
    }
    fn create(&self, _: &str, _: &str, _: &[String]) -> Result<u32, String> {
        panic!("create was called")
    }
    fn comment(&self, _: u32, _: &str) -> Result<(), String> {
        panic!("comment was called")
    }
    fn close(&self, _: u32) -> Result<(), String> {
        panic!("close was called")
    }
}

#[test]
fn tickets_on_an_unmerged_job_skips_without_calls() {
    // Create a job the way new_to_merged.rs does, but do not run it.
    let (ctx, id) = common::new_job_ctx("tickets-skip", "work on #76");
    let ctx = ctx.with_tracker(Arc::new(NoCalls));
    let out = common::run(&ctx, &["factory", "tickets", &id]).expect("tickets");
    assert!(out.contains("skipped") && out.contains("not merged"), "{out}");
}
```

`common::new_job_ctx` and `common::run` are helper names this step introduces. Look at what `tests/common/mod.rs` already offers (a repo fixture, a way to build a `CliCtx`, a way to parse args with `Cli::try_parse_from` and call `dispatch`) and reuse those. If a helper like `run` already exists under another name, use it. If creating a job needs plan files, copy the minimal plan and spec fixture that `new_to_merged.rs` uses. Add only the helper that is missing, to `tests/common/mod.rs`.

- [ ] **Step 2: Run to see them fail.** `cargo test -j4 -p factory-cli tickets`. Expected: compile errors, `Command::Tickets` and `render_tickets` do not exist.

- [ ] **Step 3: Implement.**
  - `cli.rs`: add to `Command`, after `Costs`:

```rust
    /// File the follow-up tickets of a merged job and close its parent tickets.
    Tickets {
        id: String,
        /// Print what would happen and change nothing.
        #[arg(long)]
        dry_run: bool,
    },
```

  - `commands.rs`: add `tracker: Option<Arc<dyn factory_engine::Tracker>>` to `CliCtx`, set it to `None` in `new` and `from_env`, and add

```rust
    /// A context whose driver uses this tracker instead of `gh`. Tests use it.
    pub fn with_tracker(mut self, tracker: Arc<dyn factory_engine::Tracker>) -> CliCtx {
        self.tracker = Some(tracker);
        self
    }
```

    In `CliCtx::driver()`, after `with_lease_owner`, keep the driver in a variable and apply the tracker: `let driver = driver.with_lease_owner(&self.owner); let driver = match &self.tracker { Some(t) => driver.with_tracker(Box::new(t.clone())), None => driver }; Ok(driver)`.
  - Add the dispatch arm `Command::Tickets { id, dry_run } => tickets(ctx, &norm(&id)?, dry_run),` and these functions (next to `kb_refresh`):

```rust
fn tickets(ctx: &CliCtx, id: &str, dry_run: bool) -> Result<String, CliError> {
    let d = ctx.driver()?;
    let job = d.job(id)?;
    if let Some(lease) = job.lease.as_ref().filter(|_| lease_is_live(&job, now_secs())) {
        if lease.owner != ctx.owner {
            return Err(CliError::Refused(format!("job {id} is leased by {}", lease.owner)));
        }
    }
    render_tickets(factory_engine::tickets_step(&d, id, dry_run)?, dry_run)
}

/// The text of a tickets report. A failed report is an error, so the exit code is non-zero.
fn render_tickets(report: factory_engine::TicketsReport, dry_run: bool) -> Result<String, CliError> {
    let body = report.lines.join("\n");
    if report.failed {
        return Err(CliError::Refused(body));
    }
    if dry_run && !report.skipped {
        return Ok(format!("dry run, nothing was changed\n{body}"));
    }
    Ok(body)
}
```

  Check where else `CliCtx { ... }` is built with a struct literal (grep in `crates/factory-cli`) and add `tracker: None` there.

- [ ] **Step 4: Run the tests.** `cargo test -j4 -p factory-cli tickets` and `cargo check -j4 -p factory-cli`. Then run `cargo test -j4 -p factory-cli --test e2e` and `cargo test -j4 -p factory-cli --test new_to_merged` to confirm the older flows still pass. Expected: all pass.

### Task 2: Documentation

**Files:**
- Modify: `.claude/software-factory.md`
- Modify: `.claude/commands/factory.md`
- Modify: `tools/factory/CLAUDE.md`

**Interfaces:**
- Consumes: the behavior built in Plans 1 and 2 and Task 1.
- Produces: documentation only.

- [ ] **Step 1: Read the current text.** Read `.claude/software-factory.md` (find the config key list near the line that starts `- Config:` and the section that lists the commands and the merge footers) and `.claude/commands/factory.md`.

- [ ] **Step 2: Update `.claude/software-factory.md`.**
  - In the `- Config:` key list add `tickets_on_merge` after `builder_excerpt`.
  - Add a new section `## Ticket hygiene` after the cost or KB sections (before the first-run checklist), with exactly this content:

```markdown
## Ticket hygiene

The driver can file the leftover work of a ticket and close the ticket after the last plan of a job merges. It reaches GitHub only through `gh`, behind the `Tracker` port in `factory-engine/src/tracker.rs`. The command is `factory tickets <id> [--dry-run]`, and the config key `tickets_on_merge` (default `false`) runs the same step at the end of `merge_stage`.

- The parent tickets come from the feature text given to `factory new`: `#N` and `https://github.com/<owner>/<repo>/issues/N` both count. A URL for a repository other than `origin` makes the step skip.
- The remaining work comes from the `## Follow-ups` section of the job's `spec.md`. One bullet is one ticket: the title is the first sentence (at most 100 characters), the body is the whole bullet. The new tickets get the parent's labels except `tracking`. A missing or empty section files nothing.
- The step needs a merged job that is the last plan of its series. It files the tickets, comments once on each parent (job id, `main` sha, "not yet pushed" when `origin/main` lacks it, links to the new tickets) and closes each parent that is still open.
- `tickets.json` in the job folder records every created ticket, comment and close, saved after each `gh` call. A re-run, also after a failure, never repeats a call. `--dry-run` makes only read calls and prints each action.
- A `gh` failure is written to the ledger and printed. It never changes the merge result, and `factory tickets` exits non-zero.
- Merge footer: `Relates to: #N` when the spec has follow-ups (the ticket is not finished), `Fixes: #N` otherwise. Commit and sync footers stay `Relates to`.
```

  - In the list of commands (search for `factory kb-refresh` or `factory costs`), add `factory tickets <id> [--dry-run]` with the words "file follow-up tickets and close the parent tickets of a merged job".

- [ ] **Step 3: Update `.claude/commands/factory.md` and `tools/factory/CLAUDE.md`.**
  - `factory.md`: add `tickets <id>` to the `argument-hint` (`... | cleanup <id> | tickets <id>`), add `- tickets <digits> (nothing after the digits)` to the exact-shape list and `- tickets <id>: <factory> tickets <id>` to the one-call list. In step 1 (the spec rules) add a bullet after the `## Decisions` and `## Non-goals` sentence: "The spec also has a `## Follow-ups` section that lists, one bullet per piece, the work of the ticket that this spec does not build. Leave the section out when the ticket is fully done. The factory files one ticket per bullet and closes the ticket after the last plan merges (see `.claude/software-factory.md`, Ticket hygiene)." In step 4 add: "When the work belongs to a GitHub ticket, put it in the feature text as `#N` or as the issue URL, so the driver can write the footer and file the follow-ups."
  - `tools/factory/CLAUDE.md`: in the crate layering line for `factory-cli` add `tickets` to the command list, and in the `factory-engine` bullet list add one sentence: "`tickets.rs` and `tracker.rs` are the ticket hygiene step and its `gh` port."

- [ ] **Step 4: Verify the docs.** Run these checks from the repository root; each must print at least one match:
  - `grep -n "tickets_on_merge" .claude/software-factory.md`
  - `grep -n "Follow-ups" .claude/software-factory.md .claude/commands/factory.md`
  - `grep -n "factory tickets" .claude/software-factory.md tools/factory/CLAUDE.md`
  - `grep -n "Relates to" .claude/software-factory.md`
  Then run `cargo test -j4 -p factory-cli --test agent_lint` to confirm the agent lint still passes.

Next plan: none
