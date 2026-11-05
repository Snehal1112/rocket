# DDD Decisions

Rulings for Phase 0 of [../roadmaps/ddd-roadmap.md](../roadmaps/ddd-roadmap.md). Evidence was gathered on 2026-11-04 at main `ef1a3c22`. Counts are approximate: a hit is a call site, not a unique operation.

## D-01 Which crates are mixed on purpose

**Evidence** (grep for `std::fs`, `tokio::fs`, `fs::`, `reqwest`, `std::process` in each crate's `src`, tests included):

| Crate | Sites | Top files | Port traits in the crate |
|---|---|---|---|
| `rocket-import` | 93 | `importer.rs` 51, `openapi/entry.rs` 18, `bru/zip_extractor.rs` 8, `wsdl/parser.rs` 7 | 1 |
| `rocket-http` | 14 | `spec_fetch.rs` 5, `aws_sig.rs` 3, `proxy.rs` 2, `token_client.rs` 1 | 7 |
| `rocket-git` | 220 | `git2_service/mod.rs` 107, `safety_contracts.rs` 77, `staging.rs` 11 | 2 |

Part of these hits is in test code (for example `importer.rs` from line 2394 on), so the production count is lower. The production count was not re-measured here; the roadmap's D-51 figure (about 28 sites in `importer.rs` and `zip_extractor.rs`) is the number to plan with. `rocket-http` has a real HTTP POST (`acquire_token`) and `spec_fetch.rs`. `rocket-git` is the libgit2 adapter by design.

**Options**
- (a) Accept the crates as mixed and document it (D-20 to D-22).
- (b) Move the I/O behind ports (D-50, D-51), so the importers are testable without a disk.

**Proposal:** `rocket-git` stays mixed (already documented). `rocket-import` gets a file port (D-51) and `rocket-http` gets the token POST moved out (D-50). Until those ship, D-20 to D-22 fix the docs.

Ruling (2026-11-04): `rocket-git` stays mixed. `rocket-import` gets a file port (D-51) and `rocket-http` loses the token POST (D-50). Why: it makes the importers testable without a disk and keeps `rocket-git` as the libgit2 adapter. Follow-ups: D-20 to D-22 (fix the docs now), D-50, D-51, D-55.## D-02 Size of `rocket-app`

**Evidence** (`crates/rocket-app/src`): 101 files, 99,040 lines including tests, 32,667 non-test lines (counted up to the first `#[cfg(test)]` in each file; the roadmap's "about 33k" is right). Lines by prefix, tests included: `flow_*` 25,230, `execution_*` 14,919, `openapi_*` 12,672, `collection*` 6,674, `acp_*` 3,663, `graphql_*` 3,286, `proposal*` 3,034, `contract*` 2,793, `conversation*` 1,234, `workspace*` 1,148.

**Options**
- (a) Keep one `rocket-app`.
- (b) Split by context over time: D-80 flow runtime, D-81 OpenAPI sync, D-82 contract, proposal and conversation.

**Proposal:** (b), but only after Phases 3 and 4 are done for the same area, and each split needs its own spec. No split starts before that.

Ruling (2026-11-04): split by context over time, not before Phases 3 and 4 are done for the same area, and each split needs its own spec. Why: the app layer is one crate with flow at 25k lines, but a split before the I/O cleanup would move the leaks. Follow-ups: D-80, D-81, D-82.## D-03 Events: one global `DomainEvent` or one enum per context

**Evidence:** `DomainEvent` has 81 variants in one 2,522-line file (`crates/rocket-shared/src/events.rs`). `TauriEventBus` maps each variant to a channel in one `match`. Contract tests in `events.rs` pin the wire keys. R-55 (events and IPC rework) is parked since 2026-10-11 with no spec (`docs/engineering/events-and-ipc-rework.md`). D-74 (split the enum) implements this ruling.

**Options**
- (a) Keep one global enum.
- (b) Per-context enums in the owning crates behind the same wire format, with the contract tests as the guard.

**Proposal:** (b) as the direction. Phases 3 to 6 do not need this decision to be executed.

Ruling (2026-11-05): (b), per-context enums behind the same wire format, implemented by the DomainEvent split (D-74, plans 0 to 7; spec docs/superpowers/specs/2026-11-05-domain-event-split-design.md). Status: done.

## D-04 Rich domain model

**Evidence** (callers that bypass rules today): `workspace_service.rs` mutates the registry directly at lines 112 and 289 (`registry.workspaces.push`) and 165 (`workspace.name = ..`). `flow_service.rs` calls `validate_report` before it saves, and has an in-crate helper that bypasses `validate`, so a validity check is easy to skip.

**Options**
- (a) Private fields and validating constructors everywhere.
- (b) Keep public fields plus validators.
- (c) Narrow: aggregate methods only where callers already bypass rules (`WorkspaceRegistry`, `Flow`). Newtypes later through D-67.

**Proposal:** (c). It is the gate for Phase 6 (D-60 to D-67).

Ruling (2026-11-04): narrow. Add aggregate methods only where callers already bypass rules (`WorkspaceRegistry`, `Flow`); newtypes later through D-67. Why: converting every struct costs far more than the bugs it prevents. Follow-ups: D-60, D-62, D-67.## D-05 Name of the assistant context

**Evidence:** `acp` appears in 63 files and `assistant` in 123 files across `crates`, `src` and `src-tauri/src` (case-insensitive). The crate is `rocket-acp`. In `events.rs` there are 13 distinct `Acp*` names and 3 distinct `Assistant*` names (the `Assistant*` ones are conversation events). Docs: `docs/features/acp-multi-agent-support.md` (protocol side) and `docs/features/assistant-*.md` (product side).

**Options**
- (a) Keep `rocket-acp` for the protocol client and "assistant" for the product features, with a written glossary. No rename.
- (b) Rename the crate and `Acp*` events to `assistant`.
- (c) Rename everything to `acp`.

**Proposal:** (a). ACP is the protocol and "assistant" is the feature. A rename (b) or (c) touches the 63 and 123 files above and changes event tags on the wire, which needs D-74.

Ruling (2026-11-04): keep `rocket-acp` for the protocol client and "assistant" for the product features, and write the glossary under D-87. No rename, so event tags stay as they are. Why: ACP is the protocol and assistant is the feature, and a rename touches 63 and 123 files and the event wire format. Follow-ups: D-83 shrinks to moving `AssistantMode` out of `mcp_tool_service.rs`; D-87 carries the glossary.