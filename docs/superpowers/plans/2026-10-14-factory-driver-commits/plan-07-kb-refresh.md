# Factory driver commits, plan 07: KB refresh after merge

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** After a job merges, a headless `factory-kb-refresher` agent runs the `kb-refresh` skill for the Rocket KB. It never blocks or parks anything, it must not touch Rocket itself, and `factory kb-refresh` runs it by hand.

**Architecture:** A new `kb.rs` in `factory-engine` (prompt, spec, `Engine::kb_refresh_step`) reuses the `AgentRunner` port, so tests use `ScriptedRunner` (the series ruling that replaced the spec's `KbRefresher` port). `factory-git` gets `Repo::checkout_fingerprint` so the driver can prove the main checkout is unchanged by the run. `Driver::kb_refresh` wraps the step, a stop marker file and the ledger rulings. `merge_stage` calls it after a merge (best effort), and `factory kb-refresh` calls it directly.

**Tech Stack:** Rust, `factory-git` (`git2`), `factory-engine`, `factory-cli`, markdown agent file and docs.

**Spec:** `docs/superpowers/specs/2026-10-14-factory-driver-commits-design.md` decision 16 and the Cost and risks entries on the KB. Plans 01 to 06 are merged.

## Global Constraints

- Harness: `cargo ... -j4`, never `--workspace` or `--all`, never set `CARGO_TARGET_DIR`. Run from `tools/factory/`. Explicit `git add <paths>` and a pathspec commit through `dev-workflow-skills:1-git-commit`.
- `kb_path` unset: the refresh does nothing, no agent run, no ledger line, no marker.
- The refresh never parks and never changes the job stage. Every failure becomes a `Ruling:` line in the job ledger: `kb refresh failed, <reason>`.
- The agent runs with `cwd` at the main checkout, `--add-dir <kb_dir>`, role `kb`, model `kb_model`, cap `kb_budget_weighted` per run (`used_before = 0`). It may use the skill's own subagents through the `Agent` tool. The `Workflow` tool is never used.
- The refresher edits only files under `kb_path`. Its edits stay uncommitted (`kb_auto_commit` is parsed and documented, auto commit itself is not built in this series, see the ledger ruling below).
- After every run the driver compares `checkout_fingerprint()` before and after. A difference writes `Ruling: kb refresh changed the main checkout ...` and creates the stop marker `.superpowers/factory/kb-stopped` (a file with the reason). While the marker exists the post-merge refresh is skipped with one ruling line. `factory kb-refresh` ignores the marker, and a manual run whose guard passes deletes it.
- A reply starting `BLOCKED:` or a stopped or over-budget run is a failed run.

## Review Focus

- The main checkout already has the user's own uncommitted edits (this repo usually does): the guard compares before and after, never "clean".
- A run that edits a tracked file that was already dirty (same status, different content): the fingerprint hashes content, so it is caught.
- A refresher that wrote outside `kb_path` inside Rocket (a stray file, an ignored file): untracked files show in the fingerprint, ignored files do not, which the docs say.
- The runner fails or is exhausted: ruling, merge result untouched.
- `kb_path` points to a folder that does not exist: a failed run with a clear ruling, no agent started.

## File map

- Modify `crates/factory-git/src/*` (new `fingerprint.rs`), `lib.rs` exports.
- Create `.claude/agents/factory-kb-refresher.md`; modify `crates/factory-cli/tests/agent_lint.rs` (agent list and pins).
- Create `crates/factory-engine/src/kb.rs`; modify `lib.rs`, `compat.rs` (`Driver::kb_refresh`), `merge.rs` (post-merge call).
- Modify `crates/factory-cli/src/cli.rs`, `commands.rs` (`kb-refresh`).
- Tests: `crates/factory-git/tests/`, unit tests in `kb.rs`, `crates/factory-engine/tests/kb.rs`, `crates/factory-cli/tests/`.
- Docs: `.claude/software-factory.md`, spec status note.

---

### Task 1: Fingerprint, agent file and the refresh step

**Files:**
- Create: `crates/factory-git/src/fingerprint.rs`, `.claude/agents/factory-kb-refresher.md`, `crates/factory-engine/src/kb.rs`
- Modify: `crates/factory-git/src/lib.rs`, `crates/factory-cli/tests/agent_lint.rs`, `crates/factory-engine/src/lib.rs`
- Test: `crates/factory-git/tests/fingerprint.rs`, unit tests in `kb.rs`, lint pins

**Interfaces:**
- Consumes: `Repo`, `AgentSpec`, `Engine::run_agent`, `Engine::ruling`, `Config::kb_dir`, `blocked_reason`, `AgentRun`, `StopReason`.
- Produces:
  - `Repo::checkout_fingerprint(&self) -> Result<String>`: hashes the tip sha of the checked-out branch, then every non-ignored status entry (path, status flags) sorted by path, then the content of each such file that exists and is at most 4 MiB (larger files hash their length and mtime). Use `std::hash::Hasher` with `DefaultHasher` and return hex. It is only compared inside one process.
  - `KB_AGENT = "factory-kb-refresher"`, `KB_MAX_TURNS = 80`, `KB_TOOLS = "Read,Grep,Glob,Edit,Write,Skill,Agent,Bash"`, `KB_ALLOWED_TOOLS = ["Read","Grep","Glob","Edit","Write","Skill","Agent","Bash(git log:*)","Bash(git diff:*)","Bash(git show:*)","Bash(git rev-parse:*)","Bash(git merge-base:*)","Bash(git status:*)","Bash(ls:*)"]`.
  - `kb_prompt(kb_dir: &Path, repo_root: &Path, tip: &str, job: Option<(&str, &str)>) -> String`: names the project (`rocketapi`, the folder name of `kb_dir`), the repo path, the KB path, the current main tip, and the job id and slug when the run follows a merge. It says: run the `kb-refresh` skill for this project, use the skill's own subagents through the Agent tool, never the Workflow tool, edit only files under the KB path, change nothing in the repo, never run git commands that write, do not commit, and reply with the pages changed one per line or `No relevant changes`.
  - `kb_spec(job_id: &str, config: &Config, repo_root: &Path, kb_dir: &Path, prompt: &str) -> AgentSpec`: role `kb`, agent `factory-kb-refresher`, model `kb_model`, `budget_weighted = Some(kb_budget_weighted)`, `used_before = 0`, `max_turns = Some(KB_MAX_TURNS)`, `stall_after` from `stall_minutes`, `strict_mcp = true`, `tools = Some(KB_TOOLS)`, `allowed_tools = KB_ALLOWED_TOOLS`, `add_dirs = vec![kb_dir]`, `cwd = repo_root`, `env` empty.
  - `KbOutcome { Refreshed { pages: Vec<String> }, UpToDate, Failed(String), Tampered(String) }`.
  - `Engine::kb_refresh_step(&self, job: Option<(&str, &str)>) -> Result<KbOutcome, EngineError>`: returns `Failed("kb_path is not set")` is NOT used, the caller checks `kb_dir()` first. Steps: check the KB folder exists (else `Failed`), take the fingerprint, run the agent, take the fingerprint again; a difference gives `Tampered`; a stop reason other than `Done`, an error, or `BLOCKED:` gives `Failed(reason)`; a reply of `No relevant changes` (case-insensitive, first line) gives `UpToDate`; otherwise `Refreshed` with the non-empty reply lines. The step writes no ruling itself except through `run_agent`'s usage log: the caller writes rulings.

- [ ] **Step 1: Write the failing git test** (`crates/factory-git/tests/fingerprint.rs`, read `tests/common` for the fixture helpers): equal fingerprints on an unchanged checkout; a new untracked file changes it; editing a tracked file changes it; editing a file that was already dirty (different content, same status) changes it; a file listed in `.gitignore` does not change it; a new commit on the branch changes it.
- [ ] **Step 2: Run, expect FAIL.** `cargo test -j4 -p factory-git --test fingerprint`
- [ ] **Step 3: Implement `fingerprint.rs`** with `git2` status options `include_untracked(true).recurse_untracked_dirs(true).include_ignored(false)`. Export from `lib.rs`.
- [ ] **Step 4: Run, expect PASS.**
- [ ] **Step 5: Agent file.** `.claude/agents/factory-kb-refresher.md` with frontmatter `name: factory-kb-refresher`, a `description`, and `maxTurns: 80` only (copy the shape of `factory-committer.md`; the lint bans permission or tool keys that the committer file also bans). Body: you are the KB refresher of Rocket's software factory, started headless after a merge; invoke the `kb-refresh` skill with the Skill tool for the project and repo named in the prompt; fan out with the Agent tool when the skill says to (1 to 3 topics in parallel tool calls), never the Workflow tool; edit only files under the KB path from the prompt, never anything in the repository, never write git state (no add, commit, reset, checkout, stash, push); never ask the user anything; reply with one changed page per line, or `No relevant changes`; if you cannot finish make `BLOCKED: <reason>` the first line. Add the file to the agent list in `agent_lint.rs` and add pins: `kb-refresh`, `Workflow`, `only files under the KB path`, `BLOCKED:`, `No relevant changes`; forbid `SendMessage` and `--force-with-lease`. Read how the committer pins are written and mirror them.
- [ ] **Step 6: Write the failing unit tests in `kb.rs`:** prompt contents (project `rocketapi`, both paths, the tip, the job line when given and absent when `None`, the words `Workflow`, `Agent`, `No relevant changes`); `kb_spec` fields from the list above; reply parsing (a helper `parse_kb_reply(&str) -> Result<Vec<String>, String>` returning an empty vec for `No relevant changes`, an error for `BLOCKED: x`, trimmed non-empty lines otherwise).
- [ ] **Step 7: Implement `kb.rs`**, export `KbOutcome`, `kb_prompt`, `kb_spec`, `KB_AGENT` from `lib.rs`.
- [ ] **Step 8: Engine tests** (`crates/factory-engine/tests/kb.rs`, mirror `tests/commit.rs` fixtures; add a helper in `tests/common` only if needed): with `scripted(...)` the step returns `Refreshed` with the listed pages and the spec has role `kb`, `add_dirs` equal to the KB folder and `cwd` equal to the main checkout; `No relevant changes` gives `UpToDate`; an effect that writes a file into the main checkout gives `Tampered`; an effect that edits an already-dirty file gives `Tampered`; `BLOCKED: x` gives `Failed`; an exhausted runner gives `Failed`; a missing KB folder gives `Failed` and the runner is never called.
- [ ] **Step 9: Run.** `cargo test -j4 -p factory-git`, `cargo test -j4 -p factory-engine --test kb`, `cargo test -j4 -p factory-cli --test agent_lint`, `cargo check -j4`, `cargo clippy -j4 -p factory-git -- -D warnings`.
- [ ] **Step 10: Commit** (`feat(factory): add the KB refresher step`).

---

### Task 2: The post-merge call, the stop marker and `factory kb-refresh`

**Files:**
- Modify: `crates/factory-engine/src/compat.rs`, `merge.rs`, `lib.rs`
- Modify: `crates/factory-cli/src/cli.rs`, `crates/factory-cli/src/commands.rs`
- Test: `crates/factory-engine/tests/kb.rs`, a CLI test beside the existing command tests

**Interfaces:**
- Consumes: `Engine::kb_refresh_step`, `KbOutcome`, `Config::kb_dir`.
- Produces:
  - `const KB_STOP_FILE: &str = ".superpowers/factory/kb-stopped"` (relative to the repo root).
  - `Driver::kb_refresh(&self, id: Option<&str>, manual: bool) -> Result<String, EngineError>`: returns a one-line human summary. `None` job id is only used by the CLI when no job is named; the run then uses job id `kb` for the usage log and the event sink and writes rulings nowhere (it prints instead). With a job id it writes the rulings to that job's ledger. Behaviour:
    1. `kb_dir()` is `None`: return `"kb_path is not set"` and do nothing (no ledger line).
    2. Stop marker exists and not `manual`: ledger ruling `kb refresh skipped, stopped by an earlier change to the main checkout (see .superpowers/factory/kb-stopped)`, return.
    3. Run `kb_refresh_step`. `Refreshed` ruling `kb refreshed, pages: a, b`; `UpToDate` ruling `kb up to date`; `Failed(r)` ruling `kb refresh failed, <r>`; `Tampered(r)` ruling `kb refresh changed the main checkout, further refreshes stopped: <r>` plus the marker written with the reason. On a manual run whose outcome is not `Tampered`, delete the marker.
    4. Any `Err` from the step becomes a `Failed` ruling. The method itself only returns `Err` when even the ledger write fails.
  - `merge_stage` calls `driver.kb_refresh(Some(id), false)` after `finish_merge` succeeded and before returning `Merged`. The result is ignored except that an `Err` is swallowed (the merge is already done). A test pins that a failing refresh does not change the `Merged` result or the job stage.
  - CLI: `factory kb-refresh [--job <id>]` prints the summary. Without `--job` the usage and events go under job id `kb` and the rulings print. Add the command to `cli.rs` and dispatch in `commands.rs`. It needs no lease.

- [ ] **Step 1: Write the failing tests** (engine, using `MergeFixture`/`approved_job` and a `kb_path` pointing at a temp folder; set `config.kb_path` through the fixture helper, read how `tests/message.rs` builds its driver and extend with a `kb` option instead of editing `approved_job`):
  1. A merge with `kb_path` unset: no `kb` run (`runner.specs_for("kb")` is empty), same behaviour as before.
  2. A merge with `kb_path` set and the runner replying `docs/a.md`: one `kb` spec, the ledger holds `kb refreshed, pages: docs/a.md`, the job is `Merged`.
  3. The `kb` run fails (exhausted runner): the merge still returns `Merged`, the ledger holds `kb refresh failed`.
  4. The run edits the main checkout: ledger `changed the main checkout`, the stop file exists with the reason; a second merge (use a second job fixture or call `driver.kb_refresh` directly) writes `kb refresh skipped` and starts no `kb` run.
  5. `driver.kb_refresh(Some(id), true)` with the marker present runs, and a passing run deletes the marker.
  6. `UpToDate` writes `kb up to date`.
- [ ] **Step 2: Run, expect FAIL.**
- [ ] **Step 3: Implement.** Keep `Driver::kb_refresh` in `compat.rs` next to `commit_message` (use `with_engine` for the engine, and for the no-job case build the engine against a job-less path: if that is not possible through `with_engine`, the CLI path may require `--job` and say so, the ledger rulings need a job anyway. Prefer the simpler rule: `factory kb-refresh --job <id>` is required in this plan, and the plan 07 docs say so.) Write the marker with `std::fs::write` after creating the parent folder.
- [ ] **Step 4: CLI test.** Mirror an existing command test: `kb-refresh --job 001` with `kb_path` unset prints `kb_path is not set` and exits 0; with a fake runner the summary lists the pages.
- [ ] **Step 5: Run.** `cargo test -j4 -p factory-engine`, `cargo test -j4 -p factory-cli --test e2e --test agent_lint` and the new CLI test, `cargo check -j4`.
- [ ] **Step 6: Commit** (`feat(factory): refresh the KB after each merge`).

---

### Task 3: Docs, `factory tokens` and the end to end check

**Files:**
- Modify: `.claude/software-factory.md`, `.claude/factory/config.toml` (comment only), the spec (status note)
- Modify: `crates/factory-cli/src/tokens.rs` only if the role column cannot show `kb`; add a unit test either way
- Test: `crates/factory-cli/tests/e2e.rs` (one added scenario) and a `tokens` test

- [ ] **Step 1: e2e.** Add one scenario to `e2e.rs` (read it for the helper style): a full job to merged with `kb_path` set to a temp KB folder and a fake `claude` that answers the `kb` role (see how the fake handles roles in `tests/support/mod.rs`) with one page. Assert the merged result, the ledger line `kb refreshed`, and that `factory tokens` lists a `kb` row.
- [ ] **Step 2: `factory tokens`.** Confirm the report groups by the role string, so `kb` and `message` show without code changes. If a fixed role list exists, add `kb` and `message`. Add a unit test with `kb` and `message` rows.
- [ ] **Step 3: Docs.** `.claude/software-factory.md`: a KB section covering `kb_path` (absolute or `~/`), `kb_model`, `kb_budget_weighted`, what the refresher does and may touch, the guard (fingerprint of tip, status entries and file contents, ignored files not covered), the stop marker and how to clear it (`factory kb-refresh --job <id>` after you looked, or delete the file), that failures never park, the `kb` token role, `factory kb-refresh --job <id>`, and that the KB edits stay uncommitted (`kb_auto_commit` is parsed and reserved, automatic KB commits are not built). `.claude/factory/config.toml`: the commented `kb_path` sample gets a one-line note to turn it on only after one manual `factory kb-refresh`. Spec decision 16: add a "Plan 07 status" note: `KbRefresher` port replaced by `AgentRunner`, `kb_auto_commit` not built, `--job` required for the manual command, guard covers non-ignored files only.
- [ ] **Step 4: Run.** `cargo test -j4 -p factory-cli`, `cargo test -j4 -p factory-engine`, `cargo check -j4`.
- [ ] **Step 5: Commit** (`docs(factory): describe the KB refresh`).

---

## Not a task: the real run

After merge, the user turns on `kb_path = "~/data/rocket/Nl-knowledge-base/rocketapi"` and runs `factory kb-refresh --job <a merged job id>` once. Checks: the `kb-refresh` skill loads inside the headless run, the agent can read and write the KB through `--add-dir`, the Agent tool fan-out works, the fingerprint guard stays quiet, and `factory tokens` shows the weighted cost of the `kb` role. Record the number in `.claude/software-factory.md`, then adjust `kb_budget_weighted`.

This is the last plan of the series.
