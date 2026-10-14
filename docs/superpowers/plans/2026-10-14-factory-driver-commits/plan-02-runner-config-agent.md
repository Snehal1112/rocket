# Factory driver commits, plan 02: runner, config and agent file Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the factory driver what a committer run needs before any engine code uses it: per-run environment and trimmed `claude` flags in `AgentSpec`, the commit and KB config keys, and the `factory-committer` agent file with its lint pins.

**Architecture:** Three independent changes in three crates. `factory-agent` gets three optional `AgentSpec` fields that `args()` and the process spawn honor. `factory-core` gets seven `Config` keys with serde defaults, so older config files still load. `.claude/agents/factory-committer.md` is the agent text, pinned by `factory-cli/tests/agent_lint.rs`. Nothing in `factory-engine` changes here: plan 03 uses these pieces.

**Tech Stack:** Rust 2021, serde, toml, the fake `claude` binary in `factory-agent` for runner tests. Run from `tools/factory/`.

**Spec:** `docs/superpowers/specs/2026-10-14-factory-driver-commits-design.md`, decisions 8 (gate env), 11 (config keys), 13 (trimmed prefix), 15 and 16 (KB keys), design sections 1 (agent file) and 5 (config, runner).

## Global Constraints

- Always `cargo ... -j4`. Never `--workspace` or `--all`. Never set `CARGO_TARGET_DIR`. Run cargo from `tools/factory/`.
- No `unwrap()` in production code. Tests may use `expect("...")`.
- Config stays `#[serde(default, deny_unknown_fields)]`: every new key has a default, and a typo is still an error.
- The agent file frontmatter has only `name`, `description` and `maxTurns`. The lint bans `tools`, `model` and `isolation`, and a description must not contain a colon followed by a space.
- `AgentSpec.env` is the only way to set a child environment variable, and no existing agent sets one. The committer sets `HARNESS_GATE=0` in plan 03, not here.
- `.claude/factory/config.toml` is a risk path. Edit it only in task 2 and only the keys named there.
- Commit each task through the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths and commit with a pathspec.

## Review Focus

- The child sees a variable only when the spec sets it: a run with an empty `env` must not see it.
- `--tools` takes one argument with commas inside (`Bash,Read,Skill`), not three arguments.
- An old config file with none of the new keys must load with the documented defaults and not be degraded.
- A typo in a new key (`kb_pth`) must still be a config error.
- Zero budgets, empty model names and an empty `kb_path` string must be rejected.
- The agent lint must fail when `factory-committer.md` is missing, and when it sets `tools`, `model` or `isolation`.

---

### Task 1: AgentSpec env, strict MCP and tools

**Files:**
- Modify: `tools/factory/crates/factory-agent/src/spec.rs`
- Modify: `tools/factory/crates/factory-agent/src/claude.rs` (the `Command::new` chain in `run_inner`)
- Modify: `tools/factory/crates/factory-agent/src/bin/fake-claude.rs` (new `env` mode)
- Test: `tools/factory/crates/factory-agent/tests/stream.rs`, `tools/factory/crates/factory-agent/tests/runner.rs`

**Interfaces:**
- Consumes: `AgentSpec`, `Caps`, `ClaudeRunner`.
- Produces on `AgentSpec` (all public, set after `AgentSpec::new`):
  - `env: Vec<(String, String)>`: variables added to the child process environment. Empty by default.
  - `strict_mcp: bool`: adds `--strict-mcp-config` to the arguments. `false` by default.
  - `tools: Option<String>`: adds `--tools <value>` as one argument, for example `Some("Bash,Read,Skill".into())`. `None` by default.

- [ ] **Step 1: Write the failing tests**

Append to `tools/factory/crates/factory-agent/tests/stream.rs`, after the `args_omit_empty_options` test:

```rust
#[test]
fn args_carry_strict_mcp_and_tools_as_single_arguments() {
    let mut spec = AgentSpec::new("001", "committer", "/w", "commit");
    spec.strict_mcp = true;
    spec.tools = Some("Bash,Read,Skill".into());
    let args = spec.args(&caps(true, true, true));
    assert!(args.iter().any(|a| a == "--strict-mcp-config"), "args: {args:?}");
    let at = args.iter().position(|a| a == "--tools").expect("--tools present");
    assert_eq!(args[at + 1], "Bash,Read,Skill");
}

#[test]
fn args_omit_strict_mcp_and_tools_by_default() {
    let spec = AgentSpec::new("001", "builder", "/w", "build");
    let joined = spec.args(&caps(true, true, true)).join(" ");
    assert!(!joined.contains("--strict-mcp-config"));
    assert!(!joined.contains("--tools"));
    assert!(spec.env.is_empty());
}
```

Append to `tools/factory/crates/factory-agent/tests/runner.rs`:

```rust
#[test]
fn env_reaches_the_child() {
    let tmp = tempfile::tempdir().expect("tmp");
    let mut s = spec("env\nFACTORY_TEST_ENV", tmp.path());
    s.env = vec![("FACTORY_TEST_ENV".into(), "from-spec".into())];
    let run = runner(EventSink::null("001")).run(&s);
    assert_eq!(run.stop_reason, StopReason::Done);
    assert_eq!(run.result_text, "from-spec");
}

#[test]
fn env_is_unset_when_the_spec_does_not_set_it() {
    let tmp = tempfile::tempdir().expect("tmp");
    let run = runner(EventSink::null("001")).run(&spec("env\nFACTORY_TEST_ENV", tmp.path()));
    assert_eq!(run.result_text, "<unset>");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd tools/factory && cargo test -j4 -p factory-agent --test stream --test runner`
Expected: FAIL to compile, `no field strict_mcp on type AgentSpec`.

- [ ] **Step 3: Write the implementation**

In `tools/factory/crates/factory-agent/src/spec.rs`, add three fields at the end of `pub struct AgentSpec` (after `stall_after`):

```rust
    /// Extra environment variables for the child process. Empty for every agent
    /// except the committer, which sets `HARNESS_GATE=0`.
    pub env: Vec<(String, String)>,
    /// Adds `--strict-mcp-config`, so the run loads no MCP servers from settings.
    pub strict_mcp: bool,
    /// Value for `--tools`, a comma separated tool list such as `Bash,Read,Skill`.
    pub tools: Option<String>,
```

In `AgentSpec::new`, add after `stall_after: None,`:

```rust
            env: Vec::new(),
            strict_mcp: false,
            tools: None,
```

In `args()`, add the flag before the `let mut push = ...` closure (a flag without a value cannot go through the closure):

```rust
        if self.strict_mcp {
            a.push("--strict-mcp-config".to_string());
        }
```

and add this block right after the `allowed_tools` block:

```rust
        if let Some(t) = &self.tools {
            push("--tools", t);
        }
```

In `tools/factory/crates/factory-agent/src/claude.rs`, in `run_inner`, add one line after `.current_dir(&spec.cwd)`:

```rust
            .envs(spec.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
```

In `tools/factory/crates/factory-agent/src/bin/fake-claude.rs`, add a new arm to the `match mode.as_str()` right after the `"args"` arm:

```rust
        "env" => {
            // The second prompt line names the variable to report.
            let name = prompt.lines().nth(1).map_or("", str::trim).to_string();
            init("fake-1");
            result(&std::env::var(&name).unwrap_or_else(|_| "<unset>".to_string()), false, "success");
        }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd tools/factory && cargo test -j4 -p factory-agent --test stream --test runner`
Expected: PASS, including the four new tests.

- [ ] **Step 5: Run the whole workspace crate set that uses the runner and check for warnings**

Run: `cd tools/factory && cargo test -j4 -p factory-agent && cargo clippy -j4 -p factory-agent -- -D warnings && cargo check -j4`
Expected: all tests pass, no clippy output, the whole factory workspace still checks (no other crate builds an `AgentSpec` literal).

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill, with these paths only:

```bash
git add tools/factory/crates/factory-agent/src/spec.rs tools/factory/crates/factory-agent/src/claude.rs tools/factory/crates/factory-agent/src/bin/fake-claude.rs tools/factory/crates/factory-agent/tests/stream.rs tools/factory/crates/factory-agent/tests/runner.rs
```

Expected subject: `feat(factory): pass env and trimmed flags to agent runs`.

---

### Task 2: Commit and KB config keys

**Files:**
- Modify: `tools/factory/crates/factory-core/src/config.rs`
- Modify: `.claude/factory/config.toml`
- Test: the `tests` module at the bottom of `tools/factory/crates/factory-core/src/config.rs`

**Interfaces:**
- Consumes: `Config`, `CoreError::Config`.
- Produces on `Config` (all with serde defaults):
  - `commit_model: String`, default `"sonnet"`.
  - `commit_subagents: bool`, default `false`.
  - `commit_budget_weighted: u64`, default `60_000`, must be above zero.
  - `kb_path: Option<String>`, default `None`. `None` turns the KB read side and the refresh off. When set it must not be empty.
  - `kb_model: String`, default `"sonnet"`.
  - `kb_budget_weighted: u64`, default `150_000`, must be above zero. A first guess, replaced after the first real run.
  - `kb_auto_commit: bool`, default `false`. Plan 07 reads it.

- [ ] **Step 1: Write the failing tests**

In the `tests` module of `config.rs`, extend `empty_file_gives_defaults_and_is_not_degraded` with these assertions (before its closing brace):

```rust
        assert_eq!(cfg.commit_model, "sonnet");
        assert!(!cfg.commit_subagents);
        assert_eq!(cfg.commit_budget_weighted, 60_000);
        assert_eq!(cfg.kb_path, None);
        assert_eq!(cfg.kb_model, "sonnet");
        assert_eq!(cfg.kb_budget_weighted, 150_000);
        assert!(!cfg.kb_auto_commit);
```

Add these tests to the same module:

```rust
    #[test]
    fn commit_and_kb_keys_override_defaults() {
        let cfg = Config::parse(
            r#"
            commit_model = "opus"
            commit_subagents = true
            commit_budget_weighted = 90000
            kb_path = "/kb/rocketapi"
            kb_model = "haiku"
            kb_budget_weighted = 40000
            kb_auto_commit = true
            "#,
        )
        .expect("valid");
        assert_eq!(cfg.commit_model, "opus");
        assert!(cfg.commit_subagents);
        assert_eq!(cfg.commit_budget_weighted, 90_000);
        assert_eq!(cfg.kb_path.as_deref(), Some("/kb/rocketapi"));
        assert_eq!(cfg.kb_model, "haiku");
        assert_eq!(cfg.kb_budget_weighted, 40_000);
        assert!(cfg.kb_auto_commit);
    }

    #[test]
    fn bad_commit_and_kb_values_are_errors() {
        assert!(Config::parse("commit_budget_weighted = 0").is_err());
        assert!(Config::parse("kb_budget_weighted = 0").is_err());
        assert!(Config::parse("commit_model = \"\"").is_err());
        assert!(Config::parse("kb_model = \"  \"").is_err());
        assert!(Config::parse("kb_path = \"\"").is_err());
        assert!(Config::parse("kb_pth = \"/x\"").is_err());
    }
```

Extend `shipped_config_loads_and_maps_the_expected_paths` with these assertions (before its closing brace):

```rust
        assert_eq!(cfg.commit_model, "sonnet");
        assert!(!cfg.commit_subagents);
        assert_eq!(cfg.commit_budget_weighted, 60_000);
        assert_eq!(cfg.kb_path, None);
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd tools/factory && cargo test -j4 -p factory-core config`
Expected: FAIL to compile, `no field commit_model on type Config`.

- [ ] **Step 3: Write the implementation**

In `config.rs`, add the fields to `pub struct Config` after `reviewer_model`:

```rust
    pub commit_model: String,
    pub commit_subagents: bool,
    /// Per-run cap on weighted tokens for one committer run.
    pub commit_budget_weighted: u64,
    /// The Rocket knowledge base folder. `None` turns the KB read side and refresh off.
    pub kb_path: Option<String>,
    pub kb_model: String,
    /// Per-run cap on weighted tokens for one KB refresh run.
    pub kb_budget_weighted: u64,
    /// Whether the KB refresh commits its edits in the KB folder.
    pub kb_auto_commit: bool,
```

In `impl Default for Config`, add after `reviewer_model: "sonnet".to_string(),`:

```rust
            commit_model: "sonnet".to_string(),
            commit_subagents: false,
            commit_budget_weighted: 60_000,
            kb_path: None,
            kb_model: "sonnet".to_string(),
            kb_budget_weighted: 150_000,
            kb_auto_commit: false,
```

In `Config::parse`, replace the two `if` checks with these four (the first two stay as they are, the model check grows, two are new):

```rust
        if cfg.diff_cap_lines == 0 || cfg.budget_weighted == 0 {
            return Err(CoreError::Config("diff_cap_lines and budget_weighted must be above zero".into()));
        }
        if cfg.commit_budget_weighted == 0 || cfg.kb_budget_weighted == 0 {
            return Err(CoreError::Config("commit_budget_weighted and kb_budget_weighted must be above zero".into()));
        }
        let models = [&cfg.builder_model, &cfg.reviewer_model, &cfg.commit_model, &cfg.kb_model];
        if models.iter().any(|m| m.trim().is_empty()) {
            return Err(CoreError::Config("model names must not be empty".into()));
        }
        if cfg.kb_path.as_deref().is_some_and(|p| p.trim().is_empty()) {
            return Err(CoreError::Config("kb_path must not be empty when it is set".into()));
        }
```

In `.claude/factory/config.toml`, add after the `reviewer_model = "sonnet"` line:

```toml
commit_model = "sonnet"
commit_subagents = false
commit_budget_weighted = 60000
# kb_path = "/home/numericlabs/data/rocket/Nl-knowledge-base/rocketapi"
# kb_model = "sonnet"
# kb_budget_weighted = 150000
# kb_auto_commit = false
```

(`kb_path` stays commented, so the KB features are off until plans 06 and 07 ship and the user turns them on.)

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd tools/factory && cargo test -j4 -p factory-core`
Expected: PASS, all config tests including the three changed or new ones.

- [ ] **Step 5: Check the rest of the workspace and warnings**

Run: `cd tools/factory && cargo clippy -j4 -p factory-core -- -D warnings && cargo check -j4`
Expected: no clippy output, the workspace checks (nothing else builds a `Config` literal).

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill, with these paths only:

```bash
git add tools/factory/crates/factory-core/src/config.rs .claude/factory/config.toml
```

Expected subject: `feat(factory): add commit and KB config keys`.

---

### Task 3: The committer agent file and its lint pins

**Files:**
- Create: `.claude/agents/factory-committer.md`
- Modify: `tools/factory/crates/factory-cli/tests/agent_lint.rs` (the agent-list test at line 89 and three new tests)

**Interfaces:**
- Consumes: the lint helpers already in `agent_lint.rs`: `read`, `frontmatter`, `fm_get`, `require_all`, `forbid_all`, `check_description`, `check_max_turns`.
- Produces: an agent named `factory-committer` that plan 03 starts with `AgentSpec.agent = Some("factory-committer".into())`, and lint pins that fix its rules.

- [ ] **Step 1: Write the failing lint tests**

In `agent_lint.rs`, replace the test `only_builder_and_code_reviewer_remain` (lines 89 to 107) with:

```rust
#[test]
fn only_the_three_factory_agents_remain() -> TestResult {
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
    let want = vec![
        "factory-builder.md".to_string(),
        "factory-code-reviewer.md".to_string(),
        "factory-committer.md".to_string(),
    ];
    if names == want {
        Ok(())
    } else {
        Err(format!("factory agents are {names:?}, want {want:?}"))
    }
}
```

Add these two tests right after the `reviewer_body_pins_the_rules` test:

```rust
#[test]
fn committer_frontmatter_is_headless() -> TestResult {
    let text = read(".claude/agents/factory-committer.md")?;
    let fm = frontmatter(&text)?;
    if fm_get(&fm, "name") != Some("factory-committer") {
        return Err("committer name is wrong".to_string());
    }
    for banned in ["isolation", "model", "tools"] {
        if fm_get(&fm, banned).is_some() {
            return Err(format!("committer frontmatter must not set {banned}"));
        }
    }
    check_description(&fm, "committer")?;
    check_max_turns(&fm, "committer")
}

#[test]
fn committer_body_pins_the_rules() -> TestResult {
    let text = read(".claude/agents/factory-committer.md")?;
    require_all(
        &text,
        "committer",
        &[
            "dev-workflow-skills:1-git-commit",
            "standing answers",
            "inline",
            "floor",
            "Never push",
            "Never ask the user",
            "GIT_SEQUENCE_EDITOR=true",
            "git add -A",
            "at most 4 commits",
            "git rebase --abort",
            "HARNESS_GATE",
            "BLOCKED:",
            "---BEGIN MESSAGE---",
            "---END MESSAGE---",
        ],
    )?;
    forbid_all(&text, "committer", &["SendMessage", "--force-with-lease"])
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd tools/factory && cargo test -j4 -p factory-cli --test agent_lint`
Expected: FAIL: `only_the_three_factory_agents_remain` reports the committer missing, and the two committer tests report `factory-committer.md` cannot be read.

- [ ] **Step 3: Create the agent file**

Create `.claude/agents/factory-committer.md` with exactly this content:

```markdown
---
name: factory-committer
description: Writes the commits of a factory job with the 1-git-commit skill, and the merge and sync messages. Run headless by the factory driver.
maxTurns: 40
---
You are the committer in Rocket's software factory. The factory driver started you headless, inside a job worktree, after a builder finished a step. The builder never commits, so your job is to turn the staged changes into well written commits. The driver owns the board, the ledger, the checks, the review and the merge.

Input: your prompt names the kind of run (task, legacy, fix, merge or sync), the plan title, the task title, the spec goal line, the issue footer to use, the floor sha, the standing answers, and whether the skill's analysis steps run inline or through subagents.

## How to commit

1. Invoke the `dev-workflow-skills:1-git-commit` skill with the Skill tool and follow it.
2. The prompt says whether steps 1, 1b and 1c run inline or through the skill's subagents. Inline is the default: do them with your own tool calls and dispatch no subagent. When the prompt already holds the output of the step 1 and 1b commands, treat it as the result of running them and do not run them again.
3. Use the standing answers from the prompt for every question the skill asks. Never ask the user anything, because nobody answers during a run.
4. Follow the commit style the prompt names: Conventional Commits, lowercase type, `type(scope): description`.
5. When the prompt names an issue footer, put it in the footer of every commit message you write.

## Safety rules

The driver checks these again after you finish.

- The floor is a commit sha named in the prompt. Never rewrite the floor or any commit at or below it. Amend or fixup only commits above the floor that are not pushed.
- Allowed git: `status`, `diff`, `log`, `show`, `merge-base`, `add <explicit paths>`, `reset HEAD`, `commit`, `commit --amend`, `commit --fixup`, and `rebase -i --autosquash` with `GIT_SEQUENCE_EDITOR=true` exactly as the skill does it.
- Never push. Never `merge` or `stash`. Never `git add -A`, `--all` or `.`. Never rebase onto anything except the autosquash form. Never touch the ledger, the board or the job directory.
- Make at most 4 commits per run. If the skill proposes more groups, merge the smallest ones.
- On a rebase conflict run `git rebase --abort` and make a new commit instead.
- The driver sets `HARNESS_GATE` in your environment. Leave it as it is.

## Reply

Reply with the new commit shas, one per line, each followed by its subject. If you cannot finish, make `BLOCKED: <reason>` the first line of the reply.

Merge and sync runs change nothing. They reply with the message between `---BEGIN MESSAGE---` and `---END MESSAGE---` and no other text.
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd tools/factory && cargo test -j4 -p factory-cli --test agent_lint`
Expected: PASS, all agent lint tests including the three changed or new ones.

- [ ] **Step 5: Run the CLI crate tests and check for warnings**

Run: `cd tools/factory && cargo test -j4 -p factory-cli && cargo clippy -j4 -p factory-cli -- -D warnings`
Expected: all tests pass, no clippy output.

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill, with these paths only:

```bash
git add .claude/agents/factory-committer.md tools/factory/crates/factory-cli/tests/agent_lint.rs
```

Expected subject: `feat(factory): add the committer agent file and lint pins`.

Next plan: `plan-03-committer-module.md`, written after this plan is merged.
