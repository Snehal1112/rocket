# Factory driver commits, plan 06: issue references and the KB read side

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every commit of a job that fixes GitHub issues references them (`Relates to: #N` on task, legacy, fix and sync commits, `Fixes: #N` on the merge commit), and builder and reviewer prompts point at the Rocket knowledge base when `kb_path` is set.

**Architecture:** The board job gets an optional `issues: Vec<u32>`, filled by `factory new` from the feature text. Pure helpers in `factory-core::commitmsg` parse issue numbers, build and check footers. The committer prompt carries the footer, `verify_commit_run` checks it on new commits, and the driver appends the footers on merge and sync messages (and on the plain fallback commit). `Config::kb_dir()` expands `~` and the KB line is added to the four prompts.

**Tech Stack:** Rust, `factory-core`, `factory-engine`, `factory-cli`, markdown docs.

**Spec:** `docs/superpowers/specs/2026-10-14-factory-driver-commits-design.md` decisions 14 and 15 and the sections they touch. Plans 01 to 05 are merged.

## Global Constraints

- Harness: `cargo ... -j4`, never `--workspace` or `--all`, never set `CARGO_TARGET_DIR`. Run from `tools/factory/`. Explicit `git add <paths>` and a pathspec commit through `dev-workflow-skills:1-git-commit`.
- Board JSON stays compatible with `factory-server`: `issues` is `#[serde(default, skip_serializing_if = "Vec::is_empty")]`. A job with no issues serializes exactly as before.
- Footer lines: `Relates to: #20, #21` and `Fixes: #20, #21` (numbers ascending, one line, `, ` separator). Merge and sync messages also end with `Factory-Job: NNN` (the job id as shown on the board).
- A job without issues adds no issue footer anywhere. `Factory-Job` is added on merge and sync messages only.
- A footer problem on a task, fix or legacy commit follows the existing message rule: one retry, then keep the commits and write a ruling. It never parks.
- `kb_path` unset means no KB line anywhere and no behaviour change.

## Review Focus

- `#` followed by digits inside a word, a URL fragment or a code sample (`issues/20`, `#20`, `C#`, `#abc`, `#0`): only `#N` with N from 1 and a non-word character or end before it counts; `#0` and numbers above `u32` are ignored.
- The same issue named twice, or in any order: the list is sorted and de-duplicated.
- A fallback commit or a fallback merge message for a job with issues still carries the footer.
- The agent already wrote a `Relates to:` line (or `Fixes:` on a merge): the driver does not append a second one.
- `kb_path = "~/x"` with no `HOME`: no KB line, no panic.

## File map

- Modify `crates/factory-core/src/commitmsg.rs`: `parse_issues`, `issue_footer`, `has_footer`, `append_footers`.
- Modify `crates/factory-core/src/board.rs`: `Job.issues`. Modify `crates/factory-core/src/config.rs`: `Config::kb_dir`.
- Modify `crates/factory-engine/src/runloop.rs` (`add_job`), `ports.rs` (`JobView.issues`), `commit.rs`, `build.rs`, `message.rs`, `compat.rs`, `merge.rs` or `sync.rs` only if the footer plumbing needs it.
- Modify `crates/factory-engine/src/specs.rs`: the KB line.
- Tests next to each change and in `crates/factory-engine/tests/`.
- Docs: `.claude/software-factory.md`, `.claude/factory/commit-context.md`.

---

### Task 1: Issues on the board, parsing and footer helpers

**Files:**
- Modify: `crates/factory-core/src/commitmsg.rs`, `crates/factory-core/src/lib.rs`, `crates/factory-core/src/board.rs`
- Modify: `crates/factory-engine/src/runloop.rs` (`add_job`)
- Test: unit tests in both core files, plus one test in `crates/factory-engine/tests/runloop.rs`

**Interfaces:**
- Produces (all in `factory_core`, exported next to `validate_message`):
  - `parse_issues(text: &str) -> Vec<u32>`: every `#N` (N >= 1, fits `u32`) where the `#` is at the start or follows a character that is not alphanumeric or `_`, sorted ascending, de-duplicated.
  - `issue_footer(key: &str, issues: &[u32]) -> Option<String>`: `None` when `issues` is empty, else `"<key>: #20, #21"`.
  - `has_footer(message: &str, key: &str, issues: &[u32]) -> bool`: true when `issues` is empty, or a line of the message equals the footer line exactly.
  - `append_footers(message: &str, lines: &[String]) -> String`: trims trailing whitespace, then appends a blank line and the lines, skipping any line the message already contains as a whole line; with no lines left it returns the trimmed message.
- `Job.issues: Vec<u32>`.

- [ ] **Step 1: Write the failing tests** in `commitmsg.rs`:

```rust
#[test]
fn parse_issues_reads_hash_numbers_only() {
    assert_eq!(parse_issues("GitHub issue #20 (https://github.com/o/r/issues/20)"), vec![20]);
    assert_eq!(parse_issues("fix #21, #20 and #21"), vec![20, 21]);
    assert_eq!(parse_issues("C# and a#1 and #abc and #0 and #99999999999"), Vec::<u32>::new());
    assert_eq!(parse_issues("(#7)"), vec![7]);
    assert_eq!(parse_issues("#3\n#4"), vec![3, 4]);
}

#[test]
fn footers_are_built_and_checked() {
    assert_eq!(issue_footer("Relates to", &[]), None);
    assert_eq!(issue_footer("Fixes", &[20, 21]).as_deref(), Some("Fixes: #20, #21"));
    assert!(has_footer("feat: x", "Relates to", &[]));
    assert!(has_footer("feat: x\n\nRelates to: #20", "Relates to", &[20]));
    assert!(!has_footer("feat: x\n\nRelates to: #20", "Relates to", &[20, 21]));
    assert!(!has_footer("feat: x", "Relates to", &[20]));
}

#[test]
fn append_footers_adds_each_line_once() {
    let lines = vec!["Fixes: #20".to_string(), "Factory-Job: 001".to_string()];
    assert_eq!(append_footers("feat: x\n", &lines), "feat: x\n\nFixes: #20\nFactory-Job: 001");
    assert_eq!(append_footers("feat: x\n\nFixes: #20", &lines), "feat: x\n\nFixes: #20\nFactory-Job: 001");
    assert_eq!(append_footers("feat: x", &[]), "feat: x");
}
```

In `board.rs` tests: a job serialized with no issues has no `issues` key; a job JSON without the key loads with an empty list; a job with `[20, 21]` round trips.

- [ ] **Step 2: Run, expect FAIL.** `cargo test -j4 -p factory-core commitmsg board`

- [ ] **Step 3: Implement.** The footer-last-paragraph rule: `append_footers` keeps the existing body and puts the new lines in one block after one blank line. When the message already ends with a footer paragraph (a last paragraph whose every line looks like `Key: value`), add the new lines to that block instead of a new paragraph. Write `parse_issues` with a char walk (no regex dependency): on `#`, check the previous char, read the digit run, parse as `u32`, reject 0.

`Job` gets:

```rust
/// GitHub issues this job works on, as numbers. New in this series, absent in older boards.
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub issues: Vec<u32>,
```

Set `issues: Vec::new()` in `Job::new`. Fix every struct literal of `Job` the compiler names.

In `add_job` (`runloop.rs`), after `Job::new`, set `job.issues = factory_core::parse_issues(feature);`. In `create_job`, when the spec file is copied (`spec.md`), also parse its text and union the numbers into the first job's `issues` (read the file, `parse_issues`, merge, sort, dedup, save through `driver.store().update_job`). A later series job gets the same union as `feature` yields plus the spec numbers: do this in a small helper `series_issues(feature, spec_text)` used for every job of the series.

- [ ] **Step 4: Engine test** in `tests/runloop.rs` (read the existing `create_job` tests for the fixture): `create_job(..., "GitHub issue #20 (https://github.com/o/r/issues/20)", None, None)` gives `job.issues == vec![20]` on the board; a feature without `#` gives an empty list and the board JSON has no `issues` key.

- [ ] **Step 5: Run.** `cargo test -j4 -p factory-core`, `cargo test -j4 -p factory-engine --test runloop`, `cargo clippy -j4 -p factory-core -- -D warnings`, `cargo check -j4`. Expected: PASS and clean.

- [ ] **Step 6: Commit** (`feat(factory): record GitHub issues on the board`).

---

### Task 2: The footer on every commit

**Files:**
- Modify: `crates/factory-engine/src/ports.rs` (`JobView.issues`), `commit.rs`, `build.rs`, `message.rs`, `compat.rs`
- Modify: `.claude/factory/commit-context.md`, `.claude/agents/factory-committer.md` only if a pin needs the footer text
- Test: `crates/factory-engine/tests/commit.rs`, `tests/message.rs`, unit tests in `commit.rs` and `message.rs`

**Interfaces:**
- Consumes: `issue_footer`, `has_footer`, `append_footers`, `Job.issues`.
- Produces:
  - `CommitRequest<'a>` gets `pub issues: &'a [u32]`.
  - `JobView` gets `pub issues: Vec<u32>`.
  - `committer_prompt` adds a line `Issue footer: Relates to: #20, #21` after `Floor:` (or `Issue footer: none`), and says to put it in the footer of every commit of the run.
  - `verify_commit_run` adds the problem `commit <sha7>: missing footer \`Relates to: #20\`` for every changed commit whose message lacks the footer (same set that gets `validate_message`).
  - `fallback_commit` builds `append_footers(&fallback_subject(title), &[footer])` when the job has issues. `commit_staged` takes that full message (the subject-only form is unchanged for jobs without issues).
  - `MessageRequest<'a>` gets `pub issues: &'a [u32]`. `message_step` returns the validated message with the driver footers appended: merge `Fixes: ...` then `Factory-Job: <id>`, sync `Relates to: ...` then `Factory-Job: <id>`. The fallback messages get the same footers. The prompt for a message run says the driver appends the issue footer and the `Factory-Job` line, so the agent must leave them out (replace the sentence "the driver adds any footer itself" with the exact lines the driver adds).

- [ ] **Step 1: Write the failing tests.**
  - `commit.rs` unit: `committer_prompt` with issues `[20, 21]` contains `Issue footer: Relates to: #20, #21`; with none it contains `Issue footer: none`.
  - `tests/commit.rs` (read it for the fixtures): (a) job with issues, committer fake commits `feat(test): build step` without a footer on both runs: the commits are kept, the ledger holds `missing footer`, no fallback commit, no park; (b) fake commits `feat(test): build step\n\nRelates to: #20`: accepted with no ruling; (c) a failed run twice gives the fallback commit whose message is `chore: <title>\n\nRelates to: #20`; (d) no issues: nothing changes (existing tests stay green).
  - `tests/message.rs`: merge for a job with issues `[20]` and job id `001`: the commit message on main ends with `Fixes: #20\nFactory-Job: 001`; the fallback path gives `chore(factory): merge job 001 hello\n\nFixes: #20\nFactory-Job: 001`; sync gives `Relates to: #20` then `Factory-Job: 001`; a job with no issues gets only `Factory-Job: 001`; an agent reply that already contains `Fixes: #20` gets no duplicate.
- [ ] **Step 2: Run, expect FAIL.**
- [ ] **Step 3: Implement.** Pass `&job.issues` from `JobView` into `CommitRequest` at the one construction site in `build.rs` (`commit_work`). In `compat.rs::commit_message`, read `job.issues` and pass it to `MessageRequest`; do the footer append inside `message_step` after `pick` succeeds and on the fallback path. Merge's `Fixes` key vs sync's `Relates to` key comes from `MessageKind`. Keep `validate_message` unchanged: the footer is checked with `has_footer`, not inside it.
- [ ] **Step 3b: Context file.** `.claude/factory/commit-context.md`: one line under the standing answers: when the prompt names an issue footer, add it as the last paragraph of every commit message, exactly as given. Check `shipped_context_file_has_the_style_and_the_standing_answers` still passes and add the new phrase to its pins.
- [ ] **Step 4: Run.** `cargo test -j4 -p factory-engine`, `cargo test -j4 -p factory-cli --test agent_lint`, `cargo test -j4 -p factory-cli --test e2e`, `cargo check -j4`.
- [ ] **Step 5: Commit** (`feat(factory): reference GitHub issues in commit messages`).

---

### Task 3: The KB read side and docs

**Files:**
- Modify: `crates/factory-core/src/config.rs` (`Config::kb_dir`), `crates/factory-engine/src/specs.rs`
- Modify: `.claude/software-factory.md`
- Test: unit tests in `config.rs` and `specs.rs` (or `tests/build_checks.rs` where the prompts are already asserted)

**Interfaces:**
- Produces: `Config::kb_dir(&self) -> Option<PathBuf>`: `None` when `kb_path` is unset; expands a leading `~/` (or a lone `~`) with `HOME`; `None` when `~` is used and `HOME` is unset or empty. Does not check that the directory exists (an unreadable path is the agent's problem and the line says to skip it when missing).
- `Engine::kb_line(&self) -> String` (private, `specs.rs`): empty when `kb_dir()` is `None`, else:
  `"Knowledge base: {dir}. Read {dir}/README.md first, then only the topic pages that apply to your task. It is read-only context, never write to it. Source code and the spec win on any conflict, and say in your reply when a page is stale. If the folder does not exist, skip this.\n"`
- The line is added to `builder_first_spec`, `builder_task_spec`, `builder_resume_spec` and `reviewer_spec` prompts. The reviewer's tools stay `Read,Grep,Glob` (the KB path is read-only anyway). Do not touch the committer prompt.

- [ ] **Step 1: Write the failing tests.**
  - `config.rs`: `kb_dir` unset gives `None`; `/kb/rocketapi` stays as is; `~/kb` with `HOME=/home/u` gives `/home/u/kb` (set `HOME` inside one test with a lock or compute through an inner function `expand_home(path, home: Option<&str>)` and test that, which avoids touching the environment); `~/kb` with no home gives `None`; a path with `~` inside (`/a/~b`) is untouched.
  - `specs`: with `config.kb_path = Some("/kb/rocketapi")` all four prompts contain `Knowledge base: /kb/rocketapi` and `README.md`; with `kb_path = None` none contains `Knowledge base`. Reuse the existing spec-building tests' fixtures.
- [ ] **Step 2: Run, expect FAIL.**
- [ ] **Step 3: Implement.** Keep `expand_home` a free function in `config.rs`. In `specs.rs` insert `{kb}` into each of the four `format!` prompts right before the line that tells the agent to stop when blocked (or, for the reviewer, before `Reply in at most 60 lines`).
- [ ] **Step 4: Docs.** In `.claude/software-factory.md` add: the board `issues` field and where it comes from (`factory new` feature text, plus `spec.md` for a series); the footer table (task, legacy, fix and sync `Relates to`, merge `Fixes`, merge and sync `Factory-Job`); that `Fixes` closes the issue when the user pushes the merge, and a job that only helps an issue should be created without the number; the `kb_path` key (absolute or `~/` path, unset turns the KB off) and what the prompts say. The `config.toml` sample in `.claude/factory/config.toml` already has `kb_path` commented, check the comment says `~/` works.
- [ ] **Step 5: Run.** `cargo test -j4 -p factory-core`, `cargo test -j4 -p factory-engine`, `cargo test -j4 -p factory-cli --test agent_lint`, `cargo clippy -j4 -p factory-core -- -D warnings`, `cargo check -j4`.
- [ ] **Step 6: Commit** (`feat(factory): point builders and reviewers at the KB`).

---

Next plan: `plan-07-kb-refresh.md`, written against the signatures this plan produced.
