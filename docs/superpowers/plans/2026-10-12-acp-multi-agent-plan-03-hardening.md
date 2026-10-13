# ACP Multi-Agent Support, Plan 3: Hardening and Docs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the small gaps the investigation found around the new agent kinds: the stdio MCP bridge advertises no tools, the `propose_changes` description never mentions request docs, and the docs still describe a Claude-only isolation layer.

**Architecture:** Two independent, small changes. The bridge gets a `get_info` that reports the tools capability, so an agent without HTTP MCP support sees the tools. The tool description names request docs, so any model reading it knows `update_request` can change them. Docs are updated to match Plans 1 and 2.

**Tech Stack:** Rust (src-tauri), Markdown.

**Spec:** `.claude/acp-multi-agent-support.md`. Depends on Plans 1 and 2 being merged (the docs describe them).

## Global Constraints

- Run cargo with `-j4`. Never `cargo test --workspace` or `--all`.
- Never call unwrap in production paths (a project hook blocks it in Rust edits). `expect("...")` is fine in tests.
- Never `git add -A`, `git add .` or `--all`. Explicit paths. Commit through the `dev-workflow-skills:1-git-commit` skill (conventional commits).
- Own documentation goes in `.claude/`. Keep the project `CLAUDE.md` small: one pointer line only.
- No `OpenCollection` read needed: this plan does not touch collection or environment YAML.

## Review Focus

- The bridge must report a tools capability, because Gemini-family clients return no tools for a server without one (`discoverTools` skips a server whose capabilities have no `tools`).
- The bridge must keep forwarding calls unchanged: `get_info` must not change `list_tools` or `call_tool`.
- The new description must not weaken the tool's own limits: it must still say that nothing is written until the user accepts, that auth is never changed and that Ask mode refuses.

---

### Task 7: The stdio bridge reports the tools capability

**Files:**
- Modify: `src-tauri/src/mcp/stdio_bridge.rs` (imports at line 25-33, `ForwardingHandler` at line 38-65, tests module from line 118)
- Test: `src-tauri/src/mcp/stdio_bridge.rs`

**Interfaces:**
- Consumes: nothing from earlier plans.
- Produces: `fn bridge_server_info() -> ServerConfig` (module-private) used by `ForwardingHandler::get_info`.

- [ ] **Step 1: Write the failing test**

Add inside the `tests` module of `src-tauri/src/mcp/stdio_bridge.rs`:

```rust
    #[test]
    fn the_bridge_reports_the_tools_capability_and_its_own_identity() {
        let info = bridge_server_info();
        assert!(
            info.capabilities.tools.is_some(),
            "a client that sees no tools capability asks for no tools"
        );
        assert_eq!(info.server_info.name, "rocket-mcp-stdio-bridge");
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p rocket -j4 --lib the_bridge_reports_the_tools_capability`
Expected: FAIL to compile (`cannot find function bridge_server_info`).

- [ ] **Step 3: Implement**

In `stdio_bridge.rs` change the `rmcp::model` import to:

```rust
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, ErrorData, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig,
};
```

Add above `struct ForwardingHandler`:

```rust
/// What the bridge tells an MCP client when it connects. The tools capability
/// matters: a client that finds none (for example Gemini CLI) lists no tools
/// from this server, so the agent would see an empty tool set.
fn bridge_server_info() -> ServerConfig {
    ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_server_info(
        Implementation::new("rocket-mcp-stdio-bridge", env!("CARGO_PKG_VERSION")),
    )
}
```

Add inside `impl ServerHandler for ForwardingHandler`, before `list_tools`:

```rust
    fn get_info(&self) -> ServerConfig {
        bridge_server_info()
    }
```

- [ ] **Step 4: Run the bridge tests and the tool server tests**

Run: `cargo test -p rocket -j4 --lib stdio_bridge && cargo test -p rocket -j4 --lib mcp::tool_server`
Expected: PASS (the new test, the three env-var tests and every tool server test).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/mcp/stdio_bridge.rs
```

Then invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `fix(acp): report the tools capability from the stdio bridge`.

---

### Task 8: Name request docs in the tool description, and update the docs

**Files:**
- Modify: `src-tauri/src/mcp/tool_server.rs` (the `propose_changes` tool attribute at line 1026-1029, tests module)
- Modify: `crates/rocket-app/CLAUDE.md` (the `AcpSessionService` row)
- Modify: `crates/rocket-acp/CLAUDE.md` (module map and key design points)
- Modify: `.claude/assistant-followups.md` (open follow-ups)
- Modify: `CLAUDE.md` (one pointer line)
- Test: `src-tauri/src/mcp/tool_server.rs`

**Interfaces:**
- Consumes: the finished Plans 1 and 2 (the docs describe `AgentKind`, `session_plan`, the Gemini preset and the verification log).
- Produces: the new `propose_changes` description text.

- [ ] **Step 1: Write the failing test**

Add inside the `tests` module of `src-tauri/src/mcp/tool_server.rs` next to `get_info_reports_the_rocket_tool_server_identity`:

```rust
    #[test]
    fn the_propose_changes_description_names_request_docs_and_keeps_its_limits() {
        let server: RocketMcpToolServer<tauri::test::MockRuntime> =
            RocketMcpToolServer::new(test_app_handle(), "session-1".to_string());
        let tool = server
            .tool_router
            .list_all()
            .into_iter()
            .find(|tool| tool.name == "propose_changes")
            .expect("propose_changes is registered");
        let description = tool.description.as_deref().unwrap_or_default();
        assert!(description.contains("the request's docs"), "{description}");
        assert!(description.contains("Nothing is written until the user accepts"));
        assert!(description.contains("never auth"));
        assert!(description.contains("Not available in Ask mode"));
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p rocket -j4 --lib the_propose_changes_description`
Expected: FAIL (`the request's docs` is missing).

- [ ] **Step 3: Update the description**

In the `#[tool(...)]` attribute of `propose_changes` (line 1026-1029) replace the description string with:

```rust
        description = "Propose workspace changes for the user to review: create_folder, create_request, update_request (method, url, headers, query params, body and the request's docs), edit_script, move_item, rename_item, set_env_var (non-secret only) or update_folder_settings (a folder's default headers, variables, scripts and docs; never auth). Nothing is written until the user accepts each proposal. Returns the new proposal ids. Not available in Ask mode."
```

Keep every other attribute argument of that `#[tool(...)]` unchanged.

- [ ] **Step 4: Run the tool server tests**

Run: `cargo test -p rocket -j4 --lib mcp::tool_server`
Expected: PASS.

- [ ] **Step 5: Update `crates/rocket-app/CLAUDE.md`**

In the `AcpSessionService` row replace the sentence "start_workspace_session always attaches the tool server and takes a SessionIsolation (agent_isolation.rs: _meta options, CLAUDE_CONFIG_DIR, Rocket system prompt)." with: "start_workspace_session always attaches the tool server and takes a `SessionPlan` (`session_plan.rs`, built per `AgentKind`: Claude gets `_meta` options and `CLAUDE_CONFIG_DIR` from `agent_isolation.rs`, Gemini gets a private `GEMINI_CLI_HOME` with a lockdown `settings.json` and a `GEMINI.md`, Generic gets only a first-prompt prefix). `ROCKET_MCP_TOOL_NAMES` must match the registered tools (a `src-tauri` test checks it)."

- [ ] **Step 6: Update `crates/rocket-acp/CLAUDE.md`**

In the module map row for `agent_config.rs` write: "`AgentConfig` struct + `AgentKind` (Claude, Gemini, Generic; per-kind reserved env names and `validate_kind_env`) + `AgentConfigRepository` trait". Under "Key Design Points" add the bullet: "`AgentKind` is persisted on `AgentConfig` with `#[serde(default)]` (old files load as Claude). It decides how `rocket-app`'s `session_plan` isolates and instructs a session. It carries no behavior itself."

- [ ] **Step 7: Update `.claude/assistant-followups.md`**

Append this section at the end of the file:

```markdown
## Multi-agent support (2026-10-12)

Design and verified Gemini facts: `.claude/acp-multi-agent-support.md`. Plans: `docs/superpowers/plans/2026-10-12-acp-multi-agent-index.md`.

Open follow-ups:

- Gemini Google login (OAuth): needs `oauth_creds.json` copied into the private home with mode 0600. Not built; needs a security review first.
- If Google's API rejects the `propose_changes` schema: add a Gemini-only flattened schema (inline `$ref`, `enum` for the `op` tag, no `$schema`). Decided only after the Plan 2 Task 6 run.
- `GEMINI.md` and the first-prompt reminder are ordinary history in Gemini 0.63 and may be summarised in a very long session. Only `GEMINI_SYSTEM_MD` is compaction-proof, but it replaces Gemini's whole system prompt.
- The permission card plan (`docs/superpowers/plans/2026-10-12-acp-permission-card.md`) is still needed for `Generic` agents, whose permission requests Rocket denies today.
- Other agents, one plan each: Qwen, Codex (no single no-built-ins switch, no SSE), OpenCode (merges global config), Goose (developer built-in on by default), Copilot, Cursor (cannot remove built-ins).
- The model option id `model` may not exist for Gemini. `model_to_apply` skips it silently.
```

- [ ] **Step 8: Add the pointer to `CLAUDE.md`**

In the list of `See .claude/...` lines in the project `CLAUDE.md` (after the `.claude/assistant-followups.md` line) add:

```markdown
See `.claude/acp-multi-agent-support.md` for ACP agent kinds (Claude, Gemini, Generic), the session plan and the verified Gemini facts.
```

- [ ] **Step 9: Run the checks**

Run: `cargo test -p rocket -j4 --lib mcp::tool_server && cargo check -j4 && yarn check`
Expected: PASS. (`yarn check` covers the Markdown-free TS tree only; it must stay green.)

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/mcp/tool_server.rs crates/rocket-app/CLAUDE.md crates/rocket-acp/CLAUDE.md \
  .claude/assistant-followups.md CLAUDE.md
```

Then invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `docs(acp): describe agent kinds and name request docs in propose_changes`.

---

## Next plan

**Next: Plan 4, reasoning visibility and liveness** (`docs/superpowers/plans/2026-10-12-acp-multi-agent-plan-04-reasoning-visibility.md`).

When Tasks 7 and 8 are committed with green checks, my Claude executes Plan 4 next, in the same worktree, without asking again. One plan at a time. Reasoning matters most for guiding and correcting the user, so Plans 4 to 6 run before the permission card plan. Ledger: `.superpowers/sdd/2026-10-12-acp-multi-agent-plan-03/progress.md`.

The chain after that is Plan 5 (reasoning level), Plan 6 (guidance quality), then the existing permission card plan (`docs/superpowers/plans/2026-10-12-acp-permission-card.md`), then one small plan per further agent. See the index.
