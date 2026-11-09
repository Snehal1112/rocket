# Phase 8 series 2: domain rules out of the app layer (D-86, D-82)

Status: approved 2026-11-09, implemented. Roadmap: `docs/roadmaps/ddd-roadmap.md` Phase 8, D-86 and D-82. Decisions: `docs/engineering/ddd-decisions.md` (D-02 split by context, D-04 narrow domain model). Follows series 1 (`2026-11-09-ddd-phase8-series1-small-moves-design.md`), which moved `AssistantMode` to `rocket-acp`.

## Goal

Rules that need no service state move to the crate whose language they belong to, with no behavior change. The services in `rocket-app` keep orchestration only: ports, locks, per-session state, events.

## Non-goals

- Any change to behavior, JSON shape, file format or IPC contract.
- The three GraphQL service files and `CollectionTrustService` (see the rulings).
- The content fingerprints and the masking rule of the proposal service (see the rulings).
- Series 3 and 4 (OpenAPI sync, flow runtime).

## Current state (checked 2026-11-09, branch on top of main `98f193c1`)

D-86 candidates in `crates/rocket-app/src`:
- `graphql_document.rs` (320 lines): `GraphQlOperationKind`, `GraphQlOperation`, `list_operations`, `select_operation`. Depends on `rocket-shared` only. Users: `graphql_request.rs`, `graphql_subscription.rs`, `graphql_schema.rs` (a test), `src-tauri/src/commands/execution.rs` (through `rocket_app::graphql_document`).
- `assertion_evaluator.rs` (687 lines): `evaluate_assertions`, `assertion_outcomes`. Depends on `rocket_http::HttpResponse`, `rocket_scripting::{AssertionOutcome, TestResult, TestStatus}`, `rocket_shared::assertion::Assertion`, `serde_json`. Users: two sites in `execution_service.rs`. `rocket-scripting` already depends on all of these.
- `collection_trust.rs` (986 lines): `effective_capabilities(repo: &dyn CollectionRepository, store: &dyn CollectionTrustStore, collection: &str) -> EffectiveCapabilities` is a free function over types that all live in `rocket-collection` (`CollectionRepository`, `CollectionTrustStore`, `resolve_effective`, `RequestedElevation`, `EffectiveCapabilities` in `trust.rs`). The rest is `CollectionTrustService` (locks, events). Users: `collection_trust.rs`, `execution_service.rs`, `mcp_tool_service.rs`, `collection_service.rs` (tests), `src-tauri/src/import_wiring.rs`, and the re-export in `rocket-app/src/lib.rs`.
- Not pure: `graphql_request.rs`, `graphql_schema.rs` and `graphql_subscription.rs` depend on `crate::execution_service`, `RequestExecutionService` and the websocket and event services.

D-82 candidates:
- `contract_service.rs` (3,322 lines): `snapshot_path`, `NewContract` (`into_contract`), `ContractEdit` (`apply_to`), `apply_transition`, `walk_folder` use only types from `rocket-collection::contract` (`Contract`, `ContractError`, `ContractResult`, `StatusEvent`, `transition_status`) and `Folder`. `src-tauri/src/commands/contract.rs` imports `ContractEdit` and `NewContract` through `rocket_app`.
- `conversation_service.rs` (1,025 lines): `first_kept_index` and `text_bytes` are pure over `StoredMessage` and the constant `MAX_INPUT_TEXT_BYTES`. `collection_refs` takes a `&dyn CollectionRepository`. `rocket-acp` has `ConversationMode`, an exact duplicate of `AssistantMode` (same variants, default `Ask`, serde `lowercase`).
- `proposal_service.rs` (3,035 lines): about 25 free functions after the service. `rocket-acp` and `rocket-collection` depend on `rocket-shared` only, so no crate sees both the proposal types and `Folder`/`Request`. Some functions call `rocket-app` helpers: `request_fingerprint` uses `flow_run_cache::saved_request_text`, `child_folder` and `folder_shape` used `runner_sequence::folder_dir_name` (a one-line `folder.dir_name.as_deref().unwrap_or(&folder.name)`), `refuse_masked_values` uses `redaction::REDACTED`, `env_var_fingerprint` and `validate_environment_name` use `rocket_environment`.

## Design

Three plans of at most three tasks each. Every move uses `git mv` (or a cut and paste of a function into a new module, where the source file stays), has no re-export at the old location, and carries its tests. A pin test for each moved type's JSON (where it has one) is green before the move.

### Plan 1: D-86, the pure parts

1. `graphql_document.rs` moves to `rocket-collection/src/graphql_document.rs` (`GraphQlRequest` already lives there). `rocket-collection/src/lib.rs` adds `pub mod graphql_document;`. Users switch to `rocket_collection::graphql_document`.
2. `assertion_evaluator.rs` moves to `rocket-scripting/src/assertion_evaluator.rs`, exported as `rocket_scripting::{evaluate_assertions, assertion_outcomes}`. `rocket-scripting` gains a dependency only if the compiler asks for one; report it.
3. `effective_capabilities` moves into `rocket-collection/src/trust.rs` next to `resolve_effective`, exported from the crate root. `CollectionTrustService::effective` calls it from there. The `rocket-app` re-export (`lib.rs:119`) drops `effective_capabilities` and `src-tauri/src/import_wiring.rs` imports it from `rocket_collection`. Docs and roadmap rows (D-86) are updated in this task.

### Plan 2: D-82, contract and conversation

1. Contract: `snapshot_path`, `NewContract`, `ContractEdit`, `apply_transition` (made `pub`) and `walk_folder` (made `pub`) move into `rocket-collection/src/contract/`: `NewContract` and `ContractEdit` in a new `builders.rs`, `snapshot_path` and `walk_folder` in the existing `snapshot.rs`, `apply_transition` in the existing `state_machine.rs`. `contract/mod.rs` re-exports the public names. `rocket-app/src/lib.rs` drops the `NewContract` and `ContractEdit` re-export; `src-tauri/src/commands/contract.rs` imports them from `rocket_collection`. `ContractService` stays and calls the moved functions.
2. Conversation: `first_kept_index` and `text_bytes` move into `rocket-acp/src/conversation.rs` with the constant `MAX_INPUT_TEXT_BYTES` (public). `collection_refs` stays in the service because it takes a repository port. Then `ConversationMode` is deleted and `AssistantMode` is used in its place in `ConversationRecord` and every user (`rocket-infra` conversation repos, `conversation_service.rs`, `src-tauri/src/commands/assistant_conversations.rs`). A pin test loads a stored conversation with `"mode":"agent"` before and after.
3. Docs: glossary entry (the pair is now one type), crate guides, roadmap (D-82 contract and conversation parts).

### Plan 3: D-82, the proposal rules

1. Add the dependency edge `rocket-acp -> rocket-collection` (no cycle: `rocket-collection` does not depend on `rocket-acp`). Move `folder_dir_name` to `rocket-collection` as a method on `Folder` (`Folder::dir_or_name(&self) -> &str`), because the path rules need it. Move the structural path rules into `rocket-acp/src/proposal_rules.rs`: `Target` and `locate`, `locate_with`, `same_name`, `child_folder`, `is_folder`, `is_free`, `last_segment`, `check_reserved_name`, `join_path`, `parent_of`, `move_destination`, `normalize_path`, `normalize_paths`, `paths_of`, `validate_relative_path`, `validate_item_name`.
2. Move the apply rules into the same crate: `apply_folder_patch`, `check_folder_variables`, `set_script`, `stored_body`, `build_request`, `apply_patch`, `sha256_hex`. They only need `rocket-acp`, `rocket-collection` and `rocket-shared` types.
3. Docs and roadmap (D-82 proposal part, and the closing status of D-82 and D-86).

What stays in `ProposalService` and `proposal_service.rs`: the service, `pending_mut`, `finish`, `prune_resolved`, `unchanged`, and the functions that need `rocket-app` helpers or `rocket-environment`: `request_fingerprint`, `folder_fingerprint`, `env_var_fingerprint`, `validate_environment_name`, `refuse_masked_values`. Their callers import the moved functions from `rocket_acp::proposal_rules`.

## Rulings

- Ruling (2026-11-09): D-86 moves the pure parts only. `graphql_request.rs`, `graphql_schema.rs`, `graphql_subscription.rs` and `CollectionTrustService` stay in `rocket-app` because they orchestrate `RequestExecutionService`, websockets, locks and events. Why: the roadmap premise ("pure logic with no I/O") only holds for three items. Cost if wrong: the graphql services stay in the app layer a while longer.
- Ruling (2026-11-09): the proposal rules go to `rocket-acp` with one new edge to `rocket-collection` (the user chose this over a new crate). Why: the assistant context consumes the collection context, which is the right direction. Cost if wrong: remove the edge and the module again; the functions are pure, so the move is mechanical.
- Ruling (2026-11-09): the content fingerprints and `refuse_masked_values` stay in `rocket-app`. Why: they need `saved_request_text` (flow run cache), `REDACTED` (redaction) or `rocket-environment`, and moving those helpers or adding a second edge (`rocket-acp -> rocket-environment`) is a bigger decision than D-82 needs. Cost if wrong: about 100 lines stay in the service; a later step can move them once the helpers have a home.
- Ruling (2026-11-09): `folder_shape` moved to `rocket-acp/src/proposal_rules.rs`. Why: it became pure once `Folder::dir_or_name` replaced the app helper, and `locate_with` calls it. `folder_fingerprint` stays in `rocket-app`.
- Ruling (2026-11-09): `collection_refs` stays in `ConversationService` (it takes a repository port).
- Ruling (2026-11-09): `ConversationMode` is merged into `AssistantMode` here (it was deferred from series 1, which keeps the merge with D-82 as planned). Why: the types are identical and the serde string is the same, so saved conversation files read as before.

## Testing and verification

- Per task, for every package touched: `cargo check -j4 -p <pkg>`, `cargo check -j4 -p <pkg> --tests` (test targets under `crates/*/tests` and `src-tauri/tests` are searched too, a lesson of series 1), `cargo clippy -j4 -p <pkg>`, and the covering tests (`cargo test -j4 -p <pkg> <filter>`). The Tauri package is `-p rocket`. Never `--workspace`.
- `rustfmt --edition 2021 --check` on every touched `.rs` file, hand-fixed in place, because the moves lengthen paths.
- Dependency direction after plan 3: `rocket-acp` depends on `rocket-shared` and `rocket-collection`; `rocket-collection` does not depend on `rocket-acp`; `rocket-scripting` gains nothing new beyond what the compiler requires. A `cargo tree -p <crate> -e normal --depth 1` check goes in each task that changes an edge.
- Moved tests stay with their code (count them against the source file). `bun run tsc --noEmit` is not needed unless an IPC DTO changes; the `ConversationMode` merge keeps the same JSON, and a pin test proves it.

## Risks and sequencing

- Plan 2 task 2 changes a persisted enum's Rust type. The pin test (a conversation file with each of the three modes) is the guard.
- Plan 3 widens `rocket-collection::Folder` with one method and adds a dependency edge; both are small but need the final review's attention.
- Execution order: plan 1, then plan 2, then plan 3. Each plan is one worktree session of up to three commits, reviewed per task. The series 1 branch already contains `AssistantMode` in `rocket-acp`, which plan 2 task 2 needs.
- Other work in `rocket-app` (the team-network files under `src/network/`) is not touched by these moves.

## Roadmap updates when it ships

- D-86: `done (date, pure parts only: graphql_document, assertion_evaluator, effective_capabilities; the graphql services and the trust service stay)`.
- D-82: `done (date, contract, conversation and proposal structural rules moved; fingerprints and masking stay in rocket-app)`.
