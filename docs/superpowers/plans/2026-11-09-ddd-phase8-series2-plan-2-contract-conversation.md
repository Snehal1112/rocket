# Phase 8 series 2, plan 2: contract and conversation rules (D-82, part 1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The contract builders and pure helpers live in `rocket-collection::contract`; the conversation size rules live in `rocket-acp`; `ConversationMode` is gone and `AssistantMode` takes its place. No behavior, JSON or file-format change.

**Architecture:** Pure moves, no re-export at the old location. The `ConversationMode` merge is guarded by a pin test that loads a stored conversation in each of the three modes before and after.

**Tech Stack:** Rust: `rocket-collection`, `rocket-acp`, `rocket-infra`, `rocket-app`, `src-tauri` (package `rocket`).

**Spec:** `docs/superpowers/specs/2026-11-09-ddd-phase8-series2-domain-rules-design.md` (section "Plan 2"). Series index: `docs/superpowers/plans/2026-11-09-ddd-phase8-index.md`. Run after plan 1.

**Workspace:** the git worktree `ddd-phase8-series1` (branch `worktree-ddd-phase8-series1`). `AssistantMode` is already in `rocket-acp` (series 1).

## Global Constraints

- Serde shapes are unchanged; stored conversation files and contract files read as before. `AssistantMode` serializes as `"ask"`, `"edit"`, `"agent"`, default `Ask`.
- No re-export, alias or shim at an old location: `rocket-app/src/lib.rs` must stop re-exporting `NewContract` and `ContractEdit`, and `ConversationMode` must have no remaining reference.
- Search every target for importers: `rg -n '<old name>' crates src-tauri` over whole directories, so `src-tauri/tests`, `crates/*/tests`, benches and examples are included. After each task run `cargo check -j4 -p <pkg> --tests` for every package whose files changed.
- Format: `rustfmt --edition 2021 --check <file>` on every touched `.rs` file; hand-fix drift in place, never run write mode over a file with child modules.
- Production code has no `unwrap`. Every `cargo` command takes `-j4`, names a package with `-p`, and never uses `--workspace` or `--all`. `cargo tree` takes no `-j`. Long runs go to a log file under the session scratchpad.
- Commits: conventional commits, one per task, with the `dev-workflow-skills:1-git-commit` skill, explicit staged paths. Do not edit any `CLAUDE.md`, the glossary or the roadmap before task 3.
- Code comments are short, full sentences that end with a period.

## Review Focus

- A stored conversation file with `"mode":"agent"` (or `"edit"`, `"ask"`) no longer loads, or loads as a different mode (pin test, three modes).
- A missed importer in a test target (`rocket-infra`, `src-tauri/tests`) after `ConversationMode` is removed.
- `NewContract::into_contract` or `ContractEdit::apply_to` changes which fields survive an edit (scope, status, counters, creation data must be kept).
- `first_kept_index` changes which messages are kept at the size bound (the boundary: exactly `MAX_INPUT_TEXT_BYTES`, one over, and a newest message that alone is over).
- `rocket-collection` or `rocket-acp` gain a dependency they should not have (`cargo tree` check).

---

### Task 1: Move the contract builders and helpers to `rocket-collection`

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (this task touches `rocket-collection` and contract data).

**Files:**
- Create: `crates/rocket-collection/src/contract/builders.rs`
- Modify: `crates/rocket-collection/src/contract/snapshot.rs`, `crates/rocket-collection/src/contract/state_machine.rs`, `crates/rocket-collection/src/contract/mod.rs`, `crates/rocket-collection/src/lib.rs` (exports), `crates/rocket-app/src/contract_service.rs`, `crates/rocket-app/src/lib.rs` (line 121), `src-tauri/src/commands/contract.rs` (line 13)

**Interfaces:**
- Produces: in `rocket_collection::contract`: `NewContract` (`into_contract(self) -> Contract`), `ContractEdit` (`apply_to(self, existing: Contract) -> Contract`), `snapshot_path(requested_path: &str, saved_file_name: Option<&str>) -> String`, `walk_folder(folder: &Folder, prefix: &Path, out: &mut Vec<(PathBuf, &Request)>)` (made `pub`), `apply_transition(status: &ContractStatus, event: &StatusEvent) -> ContractResult<ContractStatus>` (made `pub`). Bodies unchanged.
- Consumes: `Contract`, `ContractParty`, `ContractScope`, `ContractPolicy`, `ContractStatus`, `ContractEnforcementMode`, `StatusEvent`, `transition_status`, `ContractError`, `ContractResult` (already in `rocket-collection::contract`).

- [ ] **Step 1: Pin the behavior at the old location.** In `contract_service.rs`'s tests (or a new small test module), add if absent: `new_contract_publish_immediately_is_active_else_draft`, `new_contract_starts_with_zero_counters_and_informational_mode`, `contract_edit_replaces_only_the_edited_fields` (scope, status, counters and creation data kept), `snapshot_path_uses_the_saved_file_name_in_the_same_folder`, `snapshot_path_without_a_file_name_is_the_requested_path`, `apply_transition_names_status_and_event_on_a_rejected_event`. Run them green.
- [ ] **Step 2: Move.** Create `builders.rs` with `NewContract` and `ContractEdit` (and their tests). Put `snapshot_path` and `walk_folder` into `snapshot.rs`, `apply_transition` into `state_machine.rs`, each with its tests and doc comment. Export the names from `contract/mod.rs` and the crate root. Remove them from `contract_service.rs`, which imports them from `rocket_collection`. Drop `NewContract` and `ContractEdit` from the `rocket-app/src/lib.rs` re-export; `src-tauri/src/commands/contract.rs` imports them from `rocket_collection`.
- [ ] **Step 3: Verify.** `rg -n 'NewContract|ContractEdit|snapshot_path|apply_transition' crates src-tauri` shows no `rocket_app::` path to the moved names. `cargo test -j4 -p rocket-collection contract` and `-p rocket-app contract_service` pass (same test count in total as the baseline). `cargo check -j4 --tests` passes for `rocket-collection`, `rocket-app`, `rocket-infra` and `rocket`. `cargo tree -p rocket-collection -e normal --depth 1` lists only `rocket-shared` among workspace crates.
- [ ] **Step 4: Format and commit.** `rustfmt --edition 2021 --check` on the touched files, then commit: `refactor(ddd): move contract builders to collection`.

---

### Task 2: Conversation size rules to `rocket-acp`, and merge `ConversationMode`

**Files:**
- Modify: `crates/rocket-acp/src/conversation.rs`, `crates/rocket-acp/src/lib.rs`, `crates/rocket-app/src/conversation_service.rs`, `crates/rocket-infra/src/fs_conversation_repo.rs`, `crates/rocket-infra/src/shared_path_conversation_repo.rs`, `src-tauri/src/commands/assistant_conversations.rs`, and any other file `rg -n 'ConversationMode|MAX_INPUT_TEXT_BYTES' crates src-tauri` lists

**Interfaces:**
- Produces: `rocket_acp::conversation::{MAX_INPUT_TEXT_BYTES: usize, first_kept_index(&[StoredMessage]) -> usize, text_bytes(&StoredMessage) -> usize}` (public); `ConversationRecord.mode: AssistantMode` (the field keeps its name and its serde key).
- Consumes: `rocket_acp::AssistantMode` (series 1, `mode.rs`).

- [ ] **Step 1: Pin the stored shape.** In `rocket-acp`'s `conversation.rs` tests add `stored_conversation_loads_each_mode` (deserialize a `ConversationRecord` JSON with `"mode":"ask"`, `"edit"` and `"agent"` and assert the three values, then serialize back and compare the string) and `conversation_without_a_mode_defaults_to_ask` if the field has a default. Run them green against `ConversationMode`, with the assertions written in terms of the variant names so only the type name changes later.
- [ ] **Step 2: Size rules.** Add boundary tests first (RED, the functions are not in `rocket-acp` yet): `first_kept_index_keeps_everything_under_the_bound`, `first_kept_index_drops_oldest_when_over`, `first_kept_index_is_len_when_newest_alone_is_over`, `text_bytes_counts_the_text_of_each_message_kind`. Then move `MAX_INPUT_TEXT_BYTES`, `first_kept_index` and `text_bytes` from `conversation_service.rs` into `rocket-acp/src/conversation.rs`; `conversation_service.rs` imports them. `collection_refs` and `MAX_SESSION_PINS` stay.
- [ ] **Step 3: Merge the mode.** Delete `ConversationMode` from `conversation.rs` and `lib.rs`; `ConversationRecord` uses `AssistantMode`. Update every user the `rg` lists (`rocket-infra` conversation repos, `conversation_service.rs`, `assistant_conversations.rs`, tests). Re-run the step 1 tests (now against `AssistantMode`).
- [ ] **Step 4: Verify.** `rg -n 'ConversationMode' crates src-tauri src` returns nothing outside `docs/`. `cargo test -j4 -p rocket-acp`, `-p rocket-infra conversation`, `-p rocket-app conversation_service` pass. `cargo check -j4 --tests` passes for `rocket-acp`, `rocket-infra`, `rocket-app` and `rocket`. Check the frontend does not name the type: `rg -n 'ConversationMode' src` (a TypeScript union of the three strings is fine).
- [ ] **Step 5: Format and commit.** `rustfmt --edition 2021 --check` on the touched files, then commit: `refactor(ddd): merge ConversationMode into AssistantMode`.

---

### Task 3: Docs

**Files:**
- Modify: `docs/engineering/glossary.md` (the AssistantMode and ConversationMode row: now one type), `crates/rocket-collection/CLAUDE.md`, `crates/rocket-acp/CLAUDE.md`, `crates/rocket-app/CLAUDE.md`, `docs/features/assistant-conversations.md` (only if it names `ConversationMode`), `docs/roadmaps/ddd-roadmap.md` (D-82 note for the contract and conversation parts; the row stays open until plan 3)

- [ ] **Step 1: Find stale statements.** `rg -n 'ConversationMode|NewContract|ContractEdit|first_kept_index|snapshot_path' .claude docs crates --glob '*.md' --glob '!docs/superpowers/**'` and judge each hit against the code.
- [ ] **Step 2: Update.** Module-map entries for `builders.rs` and the moved helpers in `rocket-collection/CLAUDE.md`, the conversation helpers in `rocket-acp/CLAUDE.md`, the removal of the moved items from `rocket-app/CLAUDE.md`, the glossary row, and a D-82 progress sentence in the roadmap ("contract and conversation parts done <date>; the proposal part is plan 3").
- [ ] **Step 3: Verify.** Every path named in the changed docs exists (`ls`), and the `rg` from step 1 shows no stale statement.
- [ ] **Step 4: Commit:** `docs(ddd): update guides for contract and conversation moves`.
