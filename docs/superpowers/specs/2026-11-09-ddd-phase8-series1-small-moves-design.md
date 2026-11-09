# Phase 8 series 1: small moves and the glossary (D-83, D-84, D-87, D-88)

Status: draft for review. Roadmap: `docs/roadmaps/ddd-roadmap.md` Phase 8. Decisions: `docs/engineering/ddd-decisions.md` (D-02 split by context, D-05 keep `rocket-acp`). Related: series 2 to 4 of Phase 8 (D-80, D-81, D-82, D-86, D-85) have their own specs.

## Goal

Four small boundary fixes, with no behavior change:

- Each type lives in the crate whose language it belongs to (`AssistantMode` in `rocket-acp`, `Template` in `rocket-collection`, the script cookie API in `rocket-scripting`).
- One written glossary says which name is the right one for a concept that has several names.

## Non-goals

- Any change to behavior, JSON shape, file format or IPC contract.
- Renaming types in code (D-87 is a document only).
- Removing `ConversationMode` (see the rulings below).
- Anything from series 2 to 4.

## Current state (checked 2026-11-09)

- `AssistantMode` (`Ask`, `Edit`, `Agent`) is defined in `crates/rocket-app/src/mcp_tool_service.rs:77`. It is plain data: derives, serde `lowercase`, `label()` and `summary()`. It is re-exported from `rocket-app/src/lib.rs` and used by `mcp_tool_service.rs`, `src-tauri/src/commands/acp_sessions.rs` and `src-tauri/src/mcp/tool_server.rs`.
- `rocket-acp` already has `ConversationMode` (`conversation.rs:26`), with the same three variants and the same serde shape. It exists because `rocket-acp` could not reach `AssistantMode` in `rocket-app`.
- `crates/rocket-http/src/script_cookie.rs` (503 lines, two test modules) holds `UrlScope`, `CookieEntry`, `CookieInput`, `cookie_id`, `to_entry`, `build_cookie` and `matching_cookies`. It builds on `Cookie`, `CookieJar` and helpers from `rocket-http::cookie`. It is re-exported in `rocket-http/src/lib.rs:52`. Users: `rocket-scripting/src/host.rs` and `context.rs`, `rocket-infra/src/scripting/state.rs` and `engine.rs`, and `rocket-app` (`execution_service.rs`, `execution_service/script_cookies.rs`, `script_host.rs`, `script_host_tests.rs`).
- `rocket-scripting` depends on `rocket-http` for `HttpRequest` and `HttpResponse` as well (`context.rs:3`), so the dependency stays after any cookie move.
- `Template` and `TemplateRepository` live in `rocket-history` (`template.rs`, `template_repository.rs`). `Template` is a saved request blueprint (name, `HttpMethod`, URL, headers, optional body), keyed by name, not a history record. Users: `rocket-infra` (`fs_template_repo.rs`, `shared_path_template_repo.rs`), `rocket-app/src/template_service.rs`, `src-tauri/src/commands/templates.rs`.
- `docs/engineering/glossary.md` does not exist.

## Design

### 1. D-83: `AssistantMode` moves to `rocket-acp`

- New module `crates/rocket-acp/src/mode.rs` with `AssistantMode` and its impl, copied unchanged (derives, doc comments, `label()`, `summary()`). `lib.rs` adds `mod mode;` and `pub use mode::AssistantMode;`.
- `rocket-app` imports it with `use rocket_acp::AssistantMode;`. The `pub use` in `rocket-app/src/lib.rs` is removed, so a missed import fails to compile. `acp_sessions.rs` and `tool_server.rs` import from `rocket_acp`.
- The tests that exercise the mode rules (`check_mode` and `open_session`) stay in `mcp_tool_service.rs`. A new test module in `mode.rs` pins the order (`Ask < Edit < Agent`), the `Default`, the JSON strings `"ask"`, `"edit"` and `"agent"`, and a non-empty `label()` and `summary()` for each variant.
- Wire format: no change. The type serializes as before.

### 2. D-84: the script cookie API moves to `rocket-scripting`

Re-scoped from the roadmap. The roadmap goal "rocket-scripting no longer depends on rocket-http" cannot be met, because scripts also use `HttpRequest` and `HttpResponse`. The move is still right: the types are what `rok` cookie calls see, which is the scripting context's language, and the lower layer (`rocket-http`) should not hold types for the layer above it. The mapping between a jar cookie and a script cookie (`to_entry`, `build_cookie`) is the translation at the edge of the two contexts, and it belongs in the consuming context.

- `crates/rocket-http/src/script_cookie.rs` moves with its tests to `crates/rocket-scripting/src/script_cookie.rs` (history kept with `git mv`). It reads `Cookie` and `CookieJar` (re-exported at the crate root), and `cookies_for_request`, `default_cookie_path` and `domain_matches` (already `pub` in `rocket_http::cookie`). Two items are `pub(crate)` today: `normalize_same_site` and `MAX_COOKIE_AGE_SECS` (`cookie.rs:88,91`). They become `pub` in `rocket_http::cookie`; they are not re-exported at the crate root and not copied.
- `rocket-scripting/Cargo.toml` adds `chrono` (workspace) and `url = "2"` (declared per crate, as `rocket-http` does). `rocket-http` keeps both, because its cookie and request code still use them.
- `rocket-http` drops `pub mod script_cookie` and the re-export at `lib.rs:52`. `rocket-scripting/src/lib.rs` exposes the same names.
- `rocket-scripting`'s `host.rs` and `context.rs` switch from `rocket_http::{CookieEntry, CookieInput}` to `crate::script_cookie::...`. `rocket-infra` and `rocket-app` import from `rocket_scripting`.
- `rocket-scripting/CLAUDE.md` and `rocket-http/CLAUDE.md` are updated in the same task (which module owns what).
- Wire format: `CookieEntry` and `CookieInput` keep their `camelCase` serde shape. The tests that move with the code pin it; one test pins the JSON of each type explicitly if none does.

### 3. D-88: `Template` and `TemplateRepository` move to `rocket-collection`

- `crates/rocket-history/src/template.rs` and `template_repository.rs` move to `crates/rocket-collection/src/` with their tests (`git mv`). `rocket-collection` depends only on `rocket-shared`, which already provides `HttpMethod`, `Header` and `Body`, so no new dependency is needed.
- `rocket-collection/src/lib.rs` adds the modules and `pub use`. `rocket-history/src/lib.rs` drops its two `pub use` lines.
- `rocket-infra` (`fs_template_repo.rs`, `shared_path_template_repo.rs`), `rocket-app/src/template_service.rs` and `src-tauri/src/commands/templates.rs` import from `rocket_collection`. `rocket-history` stays a dependency of these crates only where history types are still used.
- The on-disk layout, the repository's keying by name, the IPC DTO and the command names (`src-tauri/app-commands.txt`) do not change.
- `rocket-history/CLAUDE.md` loses the two table rows and the template notes. `rocket-collection/CLAUDE.md` gains them. The sentence "Domain crate for request execution history and reusable request templates" becomes "request execution history".

### 4. D-87: a glossary, docs only

- New `docs/engineering/glossary.md`, linked from `.claude/docs-index.md` and from the D-87 row in the roadmap.
- One entry per concept with several names. For each entry: the chosen term, the other names in use with their crate and file, and a one-line reason. First entries:
  - Run summaries: `RunSummary` and `FlowRunSummary`.
  - Variables: `Variable`, `CollectionVariable`, `VariableValue`, `EnvVarWrite`.
  - Key-value rows: `Header`, `KeyValueEntry`, `GrpcMetadataPair`.
  - acp (the protocol and its crate) versus assistant (the user-facing feature), from D-05.
  - Assistant mode: `AssistantMode` is the live mode; `ConversationMode` is the mode saved in a conversation file.
  - Saved request blueprint: "template" (`Template`) versus a request in a collection.
- The glossary names a preferred term but renames nothing. A later rename needs its own decision because it touches wire formats and many files.

## Rulings

- Ruling (2026-11-09): D-84 is re-scoped. The whole `script_cookie.rs` moves into `rocket-scripting`. Why: the types are the script API's language and the dependency direction stays one way (scripting to http). Cost if wrong: one mechanical move of about 10 files back.
- Ruling (2026-11-09): `ConversationMode` is not removed in this series. It duplicates `AssistantMode` only because `rocket-acp` could not see it, and it is the stored snapshot in a conversation file, so merging it touches `conversation_service.rs`. That belongs to D-82 (series 2), which owns the conversation service. The glossary records the pair. Cost if wrong: one duplicate enum stays for a while.
- Ruling (2026-11-09): no re-exports at the old locations. A missed import must fail to compile. Cost if wrong: a few import fixes at execution.

## Testing and verification

- Per task, for the crates it touches: `cargo check -j4 -p <crate>`, `cargo clippy -j4 -p <crate>`, `cargo test -j4 -p <crate>`. The Tauri crate is `-p rocket`. Never `--workspace`.
- Moved tests pass unchanged (only `use` lines differ).
- Dependency direction after the series: `rocket-acp` depends on `rocket-shared` only; `rocket-collection` depends on `rocket-shared` only; `rocket-http` has no dependency on `rocket-scripting`; `rocket-history` has no `Template` left. A `cargo tree -p <crate> -e normal --depth 1` check goes in each task.
- `bun run tsc --noEmit` is not needed: no IPC type changes. If a command's output shape changes by accident, the JSON pin tests fail first.

## Risks and sequencing

- Task 1 and task 3 edit `rocket-app` (`lib.rs`, `mcp_tool_service.rs`, `template_service.rs`), where other work is in flight (team-network sync). Execute after that work is committed, or rebase on it first.
- Widening `normalize_same_site` and `MAX_COOKIE_AGE_SECS` to `pub` exposes two small cookie rules from `rocket-http`. That is intended: they stay single-sourced and the scripting module does not copy them.
- Order of tasks: 1 (D-83 and D-88), 2 (D-84), 3 (D-87 and the roadmap and `CLAUDE.md` updates). Each task leaves the build green and is one commit.

## Roadmap updates when it ships

- D-83, D-88: `done (date)`.
- D-84: `done (date, re-scoped: whole module moved; dependency on rocket-http stays)`.
- D-87: `done (date, glossary only, no code renames)`.
