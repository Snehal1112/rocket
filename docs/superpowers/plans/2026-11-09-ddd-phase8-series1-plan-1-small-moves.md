# Phase 8 series 1: small moves and the glossary (D-83, D-84, D-87, D-88)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `AssistantMode` lives in `rocket-acp`, `Template` and `TemplateRepository` in `rocket-collection`, the script cookie API in `rocket-scripting`, and a written glossary exists. No behavior, JSON or file-format change.

**Architecture:** Pure moves with `git mv`. No re-exports at the old locations, so a missed import fails to compile. A JSON pin test is written and run green at the old location before each move, then moves with the code.

**Tech Stack:** Rust: `rocket-acp`, `rocket-collection`, `rocket-history`, `rocket-http`, `rocket-scripting`, `rocket-infra`, `rocket-app`, `src-tauri` (package `rocket`).

**Spec:** `docs/superpowers/specs/2026-11-09-ddd-phase8-series1-small-moves-design.md`. Series index: `docs/superpowers/plans/2026-11-09-ddd-phase8-index.md`.

**Workspace:** git worktree `ddd-phase8-series1` (branch `worktree-ddd-phase8-series1`). Before task 1 run `git merge main` so the branch includes the team-network work; resolve nothing by hand without asking. Do not touch other worktrees.

## Global Constraints

- Serde shapes are unchanged: `AssistantMode` serializes as `"ask"`, `"edit"`, `"agent"`; `Template`, `CookieEntry` and `CookieInput` keep `camelCase`. IPC DTOs, command names (`src-tauri/app-commands.txt`) and the on-disk template layout do not change.
- No re-export, alias or shim at an old location.
- New dependency edges: `rocket-scripting` gains `chrono` (workspace) and `url = "2"`. No new crate-to-crate edge. After the series `rocket-acp` and `rocket-collection` depend only on `rocket-shared` (plus the third-party crates they use now), and `rocket-http` does not depend on `rocket-scripting`.
- Production code has no `unwrap`. Every `cargo` command takes `-j4`, names a package with `-p`, and never uses `--workspace` or `--all`. The Tauri package is `rocket`. Long runs go to a log file or the background.
- Commits: conventional commits, one per task, made with the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths; never `git add -A`, `--all` or `.`.
- Code comments are short, full sentences that end with a period.

## Review Focus

- A stale or missed import still compiles through a leftover `pub use` (the check: `rg` for the old paths returns nothing).
- The JSON of `AssistantMode`, `Template`, `CookieEntry` or `CookieInput` changes by accident (pin tests, run green before and after the move).
- A saved template file written before the move no longer loads (`FsTemplateRepo` round trip still passes).
- A script that reads or writes cookies stops working end to end (existing `script_host` and `script_cookies` tests in `rocket-app`).
- A crate gains a dependency it should not have (`cargo tree` checks in each task).

---

### Task 1: Move `AssistantMode` (D-83) and `Template` (D-88)

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (this task touches `rocket-collection` and `rocket-infra`).

**Files:**
- Create: `crates/rocket-acp/src/mode.rs`
- Modify: `crates/rocket-acp/src/lib.rs`, `crates/rocket-app/src/mcp_tool_service.rs` (remove the enum at about line 77, import it), `crates/rocket-app/src/lib.rs` (drop `AssistantMode` from the `pub use` at about line 162), `src-tauri/src/commands/acp_sessions.rs`, `src-tauri/src/mcp/tool_server.rs`
- Move: `crates/rocket-history/src/template.rs` and `template_repository.rs` to `crates/rocket-collection/src/`
- Modify: `crates/rocket-collection/src/lib.rs`, `crates/rocket-history/src/lib.rs`, `crates/rocket-infra/src/fs_template_repo.rs`, `crates/rocket-infra/src/shared_path_template_repo.rs`, `crates/rocket-app/src/template_service.rs`, `src-tauri/src/commands/templates.rs`, `crates/rocket-acp/CLAUDE.md`, `crates/rocket-collection/CLAUDE.md`, `crates/rocket-history/CLAUDE.md`

**Interfaces:**
- Produces: `rocket_acp::AssistantMode` (variants `Ask`, `Edit`, `Agent`; `label(self) -> &'static str`; `summary(self) -> &'static str`; derives and serde as today). `rocket_collection::{Template, TemplateRepository}` with unchanged signatures.
- Consumes: nothing from other tasks.

- [ ] **Step 1: Write the mode tests.** In a new `crates/rocket-acp/src/mode.rs`, `#[cfg(test)]` tests: `assistant_mode_order_is_ask_edit_agent` (`Ask < Edit < Agent`), `assistant_mode_default_is_ask`, `assistant_mode_json_strings` (`serde_json::to_string` gives `"\"ask\""`, `"\"edit\""`, `"\"agent\""`, and each parses back), `assistant_mode_label_and_summary_are_not_empty` (all three variants).
- [ ] **Step 2: Run to see it fail.** `cargo test -j4 -p rocket-acp mode` fails to compile because `AssistantMode` is not defined.
- [ ] **Step 3: Move the enum.** Cut `AssistantMode` and its `impl` from `mcp_tool_service.rs` into `mode.rs` unchanged (derives, doc comments, `label`, `summary`). Add `mod mode;` and `pub use mode::AssistantMode;` to `rocket-acp/src/lib.rs`. In `mcp_tool_service.rs` add `use rocket_acp::AssistantMode;`; remove it from the `rocket-app/src/lib.rs` re-export. Point `acp_sessions.rs` and `tool_server.rs` at `rocket_acp::AssistantMode`. Enumerate leftovers with `rg 'rocket_app::.*AssistantMode|AssistantMode' crates src-tauri/src --glob '*.rs'`.
- [ ] **Step 4: Verify D-83.** `cargo test -j4 -p rocket-acp mode` passes; `cargo test -j4 -p rocket-app mcp_tool_service` passes (the `check_mode` and `open_session` tests); `cargo check -j4 -p rocket` passes.
- [ ] **Step 5: Pin `Template` at the old location.** In `rocket-history/src/template.rs` add `template_json_shape_is_unchanged`: serialize `Template::new("t", HttpMethod::Get, "https://x")` and assert the object has exactly the keys `name`, `method`, `url`, `headers`, `body`, with the values the current code produces (run it once to read them; do not guess `method`'s casing). It must pass before the move.
- [ ] **Step 6: Move `Template`.** `git mv` both files to `rocket-collection/src/`. Add the two `mod` lines and `pub use template::Template; pub use template_repository::TemplateRepository;` to `rocket-collection/src/lib.rs`; delete the matching lines (12 and 13) in `rocket-history/src/lib.rs`. Fix `template_repository.rs`'s own `use` line if it names `crate::template`. Switch `fs_template_repo.rs`, `shared_path_template_repo.rs`, `template_service.rs` (two places) and `templates.rs` to `rocket_collection::{Template, TemplateRepository}`.
- [ ] **Step 7: Verify D-88.** `rg 'rocket_history::.*Template|history::template' crates src-tauri/src` returns nothing. `cargo test -j4 -p rocket-collection template`, `-p rocket-history`, `-p rocket-infra template` and `-p rocket-app template_service` pass. `cargo clippy -j4 -p rocket-acp -p rocket-collection -p rocket-history` is clean. `cargo tree -j4 -p rocket-acp -e normal --depth 1` and the same for `rocket-collection` show `rocket-shared` as the only workspace dependency.
- [ ] **Step 8: Commit** with the git-commit skill, paths staged explicitly. Suggested subject: `refactor(ddd): move AssistantMode and Template`.

---

### Task 2: Move the script cookie API to `rocket-scripting` (D-84)

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (this task touches `rocket-http` and `rocket-infra`).

**Files:**
- Move: `crates/rocket-http/src/script_cookie.rs` to `crates/rocket-scripting/src/script_cookie.rs`
- Modify: `crates/rocket-http/src/cookie.rs` (lines 88 and 91), `crates/rocket-http/src/lib.rs` (the `pub mod script_cookie` line and the `pub use script_cookie::{...}` block at about line 52), `crates/rocket-scripting/Cargo.toml`, `crates/rocket-scripting/src/lib.rs`, `crates/rocket-scripting/src/host.rs`, `crates/rocket-scripting/src/context.rs`, `crates/rocket-infra/src/scripting/state.rs`, `crates/rocket-infra/src/scripting/engine.rs` (about line 4284), `crates/rocket-app/src/execution_service.rs` (about lines 2417 and 2424), `crates/rocket-app/src/execution_service/script_host.rs`, `.../script_cookies.rs`, `.../script_host_tests.rs`, `crates/rocket-scripting/CLAUDE.md`, `crates/rocket-http/CLAUDE.md`

**Interfaces:**
- Produces: `rocket_scripting::{UrlScope, CookieEntry, CookieInput, cookie_id, to_entry, build_cookie, matching_cookies}` with unchanged signatures. `rocket_http::cookie::{normalize_same_site, MAX_COOKIE_AGE_SECS}` become `pub`.
- Consumes: nothing from task 1.

- [ ] **Step 1: Pin the JSON at the old location.** If `script_cookie.rs` has no test that asserts the JSON keys of `CookieEntry` and `CookieInput`, add `cookie_entry_json_keys_are_camel_case` and `cookie_input_json_keys_are_camel_case` (keys such as `httpOnly`, `maxAge`, `sameSite`). Run `cargo test -j4 -p rocket-http script_cookie`; it passes before the move.
- [ ] **Step 2: Widen two items.** Make `normalize_same_site` and `MAX_COOKIE_AGE_SECS` in `rocket-http/src/cookie.rs` `pub`. Do not re-export them from the crate root. `cargo check -j4 -p rocket-http` passes.
- [ ] **Step 3: Move the module.** `git mv` the file. Replace its `use crate::cookie::{...}` with `rocket_http::cookie::{...}` and `rocket_http::{Cookie, CookieJar}`. Add `chrono.workspace = true` and `url = "2"` to `rocket-scripting/Cargo.toml`. Add `pub mod script_cookie;` and a `pub use script_cookie::{...}` of the seven names to `rocket-scripting/src/lib.rs`. Remove the module line and the re-export block from `rocket-http/src/lib.rs`.
- [ ] **Step 4: Fix every importer.** Enumerate with `rg 'CookieEntry|CookieInput|UrlScope|to_entry|build_cookie|matching_cookies|cookie_id' crates src-tauri/src --glob '*.rs'`. In `rocket-scripting` use `crate::script_cookie::...`; in `rocket-infra`, `rocket-app` and `src-tauri` use `rocket_scripting::...` (fully qualified `rocket_http::to_entry` and `rocket_http::UrlScope` in `execution_service.rs` become `rocket_scripting::...`). Keep `HttpRequest`, `HttpResponse`, `RequestOptions` on `rocket_http`.
- [ ] **Step 5: Verify.** `rg 'rocket_http::.*(CookieEntry|CookieInput|UrlScope|to_entry|build_cookie|matching_cookies)'` returns nothing. These pass: `cargo test -j4 -p rocket-scripting`, `-p rocket-http`, `-p rocket-infra scripting`, `-p rocket-app script_host`, `-p rocket-app script_cookies`; `cargo check -j4 -p rocket`; `cargo clippy -j4 -p rocket-http -p rocket-scripting`. `cargo tree -j4 -p rocket-http -e normal --depth 1` does not list `rocket-scripting`.
- [ ] **Step 6: Commit** with the git-commit skill. Suggested subject: `refactor(ddd): move script cookie API to scripting`.

---

### Task 3: The glossary (D-87) and roadmap updates

**Files:**
- Create: `docs/engineering/glossary.md`
- Modify: `.claude/docs-index.md`, `docs/roadmaps/ddd-roadmap.md` (rows D-83, D-84, D-87, D-88 and the decisions log), `crates/rocket-acp/CLAUDE.md`, `crates/rocket-collection/CLAUDE.md`, `crates/rocket-history/CLAUDE.md`, `crates/rocket-scripting/CLAUDE.md`, `crates/rocket-http/CLAUDE.md`

**Interfaces:**
- Consumes: the moves from tasks 1 and 2 (the glossary names the new crates).
- Produces: nothing later work depends on in code.

- [ ] **Step 1: Verify each entry before writing it.** For every type in the table below, open its definition and read its doc comment and fields; the glossary states what it is, not what its name suggests. Locations today: `RunSummary` (`rocket-app/src/collection_runner_service.rs:158`), `FlowRunSummary` (`rocket-app/src/flow_execution_service.rs:821`), `Variable` (`rocket-environment/src/variable.rs`), `CollectionVariable` (`rocket-collection/src/settings.rs`), `VariableValue` (`rocket-shared/src/variable_value.rs`), `EnvVarWrite` (`rocket-scripting/src/result.rs`), `Header` (`rocket-shared/src/types.rs`), `KeyValueEntry` (`rocket-collection/src/contract/snapshot.rs`), `GrpcMetadataPair` (`rocket-grpc/src/events.rs`), `ConversationMode` (`rocket-acp/src/conversation.rs`), `AssistantMode` (`rocket-acp/src/mode.rs`), `Template` (`rocket-collection`).
- [ ] **Step 2: Write `docs/engineering/glossary.md`.** One row per concept: preferred term, other names with crate and file, one-line reason, and "rename: no" unless a later decision says otherwise. Rulings to record (change one only if step 1 shows it is wrong, and say so in the file):
  - Run summaries: keep both names; a summary type is prefixed with its context unless it is that context's only one.
  - Variables: "variable" means a named value in a scope; type names carry the scope (`Variable` in an environment, `CollectionVariable` in collection settings); `VariableValue` is the shared value enum; `EnvVarWrite` is a script's request to write one.
  - Key-value rows: use `Header` for HTTP headers; do not add another pair type, reuse an existing one; the three existing types stay.
  - acp is the protocol and its crate (`rocket-acp`); assistant is the user-facing feature.
  - `AssistantMode` is the live session mode; `ConversationMode` is the mode saved in a conversation file; series 2 (D-82) merges them.
  - A template is a saved request blueprint (`Template`, keyed by name); a request lives in a collection.
- [ ] **Step 3: Update the crate guides to match tasks 1 and 2.** `rocket-acp/CLAUDE.md` module map gains `mode.rs`. `rocket-collection/CLAUDE.md` gains the `Template` and `TemplateRepository` rows and the keyed-by-name note. `rocket-history/CLAUDE.md` loses them, and its first sentence says it covers request execution history only. `rocket-scripting/CLAUDE.md` module map gains `script_cookie.rs` and says why `rocket-http` stays a dependency (requests, responses, the cookie jar). `rocket-http/CLAUDE.md` drops the module and says it keeps the cookie jar only.
- [ ] **Step 3b: Link the glossary.** Add a line for `docs/engineering/glossary.md` to `.claude/docs-index.md` beside the `ddd-decisions.md` line, and put the path in the D-87 row of the roadmap.
- [ ] **Step 4: Update the roadmap.** Set D-83, D-88 `done (<date>)`; D-84 `done (<date>, re-scoped: the whole module moved; the dependency on rocket-http stays)`; D-87 `done (<date>, glossary only, no code renames)`. Add a decisions-log line for the D-84 re-scope (Ruling 2026-11-09).
- [ ] **Step 5: Verify.** `rg 'glossary.md' .claude/docs-index.md docs/roadmaps/ddd-roadmap.md` shows both links; every path named in the glossary exists (`ls` them).
- [ ] **Step 6: Commit** with the git-commit skill. Suggested subject: `docs(ddd): add glossary and close series 1`.
