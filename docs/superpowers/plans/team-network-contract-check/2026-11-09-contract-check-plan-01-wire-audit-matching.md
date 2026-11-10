# Contract Check Plan 1: Wire, Audit, Matching Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the two contract-check frames, their limits and id, the event enum, the audit kinds, and the pure matching and verdict logic.

**Architecture:** Wire types and limits live in `rocket-network` (pure). Audit kinds go in `rocket-audit`. Matching and verdict logic goes in `rocket-app` as a pure module, because it reads `RequestSignatureSnapshot` from `rocket-collection`.

**Tech Stack:** Rust, serde, existing `hex_id!` macro, `is_sensitive_name` from `rocket-export`.

**Spec:** `docs/superpowers/specs/2026-11-09-team-network-contract-check-design.md` (sections 5, 6, 7). Index: `00-plan-index.md`.

## Global Constraints

- `deny_unknown_fields` on the frames; `MAX_FRAME_BYTES` 64 KiB (`limits.rs:26`).
- Caps: 200 changes; contract name 120 chars, no control characters; version 32; `path_template` 512; `field` 128; `old_value`/`new_value` 256 (cut if longer, on a char boundary); expiry 10 minutes; 1 pending per peer, 3 total.
- `PROTOCOL_VERSION` stays 3. No bump.
- Result frames never echo offer strings. Errors never echo frame text (`DomainError::InvalidInput("invalid peer message")`).
- Event fields snake_case, tag camelCase (existing rule). Audit kinds `network_contract_check_sent` / `network_contract_check_received`.

## Review Focus

- A change with a multi-byte character at the 256 cut: the cut must not split it (Task 1 test).
- `rows` with a duplicate or out-of-range index: refused by the provider-side check in Plan 2; here `ContractCheckResult` with 201 rows is refused at decode (Task 1 test).
- A secret value in `old_value`/`new_value` of `header:Authorization`, `auth`, `query:api_key`: becomes `[hidden]` (Task 3 test).
- Path template with `{{base}}/users/:id?x=1` against a request `https://h/users/42`: matches (Task 3 test).
- No local match for any change: every verdict `not_used` (Task 3 test).

---

### Task 1: Frames, id and limits

This task has no collection code, so the OpenCollection read is not needed.

**Files:**
- Modify: `crates/rocket-network/src/link.rs` (add `ContractCheckId` via `hex_id!`, next to `SyncId`, with the same tests)
- Modify: `crates/rocket-network/src/limits.rs` (new consts, one `assert_eq!` each in the test module)
- Modify: `crates/rocket-network/src/messages.rs` (enums, structs, two variants, validation in `decode`)
- Modify: `crates/rocket-network/src/lib.rs` (re-exports)

**Interfaces:**
- Produces: everything under "Plan 1 produces" in the index for `link.rs`, `messages.rs`, `limits.rs`.

- [ ] **Step 1: Write failing tests in `messages.rs`** (names; assertions use the spec's values):
  - `contract_check_frames_round_trip` (add both to `every_variant_round_trips` fixtures).
  - `contract_check_frames_have_the_expected_shape`: exact JSON, `rows` absent when empty, `type` is `contract_check_offer` / `contract_check_result`.
  - `contract_check_enums_use_snake_case_names`: `not_used`, `additive_ok`, `answered`.
  - `contract_check_frames_reject_bad_values`: unknown field; 0 changes; 201 changes; name of 121 chars; name with `\n`; `path_template` 513; `field` 129; bad `check_id` ("nope"); 201 rows; each must be `DomainError::InvalidInput(_)`.
  - `contract_check_values_are_cut_at_256_on_a_char_boundary`: build a change with `"é".repeat(200)` (400 bytes, 200 chars) and `"a".repeat(300)`; `ContractChange::cut_values(&mut self)` leaves at most 256 chars.
  - `contract_check_offer_fits_the_frame_limit_check`: `PeerMessage::fits_frame(&self) -> bool` is false for 200 maximal changes, true for 5 small ones.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-network contract_check` — expect FAIL (types missing).
- [ ] **Step 3: Implement** the types per the index (derives and serde attributes as the sync helpers: snake_case enums, `#[serde(default, skip_serializing_if = "Option::is_none")]` on the optional values, `#[serde(default, skip_serializing_if = "Vec::is_empty")]` on `rows`). Add `validate_contract_check(&PeerMessage) -> DomainResult<()>` called from `PeerMessage::decode` for the two variants (check_id parse, counts, lengths via `chars().count()`, control characters in `contract`). Add `ContractChange::cut_values` and `PeerMessage::fits_frame`.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-network` — expect PASS (existing 298 plus new).
- [ ] **Step 5: Commit** `feat(network): add contract check frames and limits` with pathspecs of the four files.

### Task 2: Event enum and audit kinds

**Files:**
- Modify: `crates/rocket-network/src/events.rs` (`ContractCheckEvent`, `impl PublishedEvent`, `ContractCheckRowDto` camelCase)
- Modify: `crates/rocket-audit/src/event.rs` (two kinds, `tag()`), `crates/rocket-audit/src/control.rs` (add `network_contract_check_*` to the CC6.7 entry)
- Modify: `src/lib/tauri-api.ts:2850-2857` (the `AuditEventKind` union mirror gets both kinds)

**Interfaces:**
- Produces: `ContractCheckEvent` (index), channel `"network-contract-check"`.

- [ ] **Step 1: Write failing tests**: in `events.rs` extend the pattern of `wire_contract_sync_events` (`wire_contract_contract_check_events`: sorted key sets `change_count, check_id, contract, device_id, type`; `ResultReceived` has `check_id, device_id, outcome, rows, type`), `contract_check_events_use_one_channel`, `wire_contract_contract_check_tags_are_camel_case` (`offerReceived`, `resultReceived`, `expired`), row DTO keys camelCase. In `event.rs`: `network_contract_check_kinds_have_stable_tags`. In `control.rs`: `the_contract_check_kinds_are_listed_in_the_catalog` and CC6.7 present, CC6.1 absent.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-network events` and `cargo test -j4 -p rocket-audit` — expect FAIL.
- [ ] **Step 3: Implement** the enum and kinds (`{ device_id: String, contract: String, outcome: String }`), the catalog entry, and the TS union.
- [ ] **Step 4: Run** the same commands plus `cargo check -j4` — expect PASS; `bun run tsc --noEmit` no errors.
- [ ] **Step 5: Commit** `feat(network): add contract check event and audit kinds`.

### Task 3: Matching, verdict and redaction (pure)

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-app/src/network/contract_check_match.rs` and `contract_check_match_tests.rs`
- Modify: `crates/rocket-app/src/network/mod.rs`

**Interfaces:**
- Consumes: `ContractChange`, `ContractPolicyWire`, `ContractVerdict` (Task 1); `RequestSignatureSnapshot` (`rocket-collection`, fields `method`, `url_pattern`, `headers`, `query_params`, `body_content`, `form_fields`, `auth_type`); `is_sensitive_name(&str) -> bool` (`rocket-export`).
- Produces:

```rust
pub struct LocalRequest { pub collection: String, pub name: String, pub signature: RequestSignatureSnapshot }
pub fn normalize_path(url: &str) -> Vec<String>;       // drops scheme, host, query; {{var}}, :id, {id} become "*"
pub fn paths_match(template: &str, url: &str) -> bool;  // segment by segment, case-sensitive, "*" matches any one segment
pub fn verdicts(changes: &[ContractChange], policy: ContractPolicyWire, locals: &[LocalRequest]) -> Vec<(ContractVerdict, Vec<usize>)>; // indexes into locals
pub fn redact_change(change: ContractChange) -> ContractChange; // [hidden] for sensitive field names or auth
```

- [ ] **Step 1: Write failing table tests** (`normalize_and_match_table`, `verdict_table`, `worst_verdict_wins`, `no_match_is_not_used`, `redaction_table`):
  - Match: template `/users/:id` vs `https://h.io/users/42?x=1` true; `{{base}}/users/{id}` vs `/users/7` true; `/users` vs `/users/7` false; `/Users` vs `/users` false; method must be equal ignoring case.
  - Verdict (policy Strict unless noted): added `header:X-Tenant` and breaking, request lacks it → `break`; request sends it → `pass`; removed `query:legacy`, request still sends → `break`, request does not → `pass`; changed `query:page` with new value `"2"` and breaking, request sends `"1"` → `break`, sends `"2"` → `pass`; same added change with `breaking: false` → `pass`; two matching requests, one break one pass → `break` and both indexes returned; no matching request → `not_used`; `auth` change breaking with request `auth_type` equal to new value → `pass`, different → `break`; `body` removed/changed breaking with request having a body → `break`, request without a body → `pass`.
  - Redaction: `header:Authorization`, `query:api_key`, `auth`, `form:password` → both values `Some("[hidden]")` when present; `header:Accept` untouched; `None` stays `None`.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-app contract_check_match` — expect FAIL.
- [ ] **Step 3: Implement** the four functions. Field lookup by prefix (`header:`, `query:`, `path:`, `form:`, `body`, `auth`); `path:` fields are evaluated against the matched path segments' variable names. Header names compare case-insensitively.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-app contract_check_match`, `cargo clippy -j4 -p rocket-app` — expect PASS, no new lints.
- [ ] **Step 5: Commit** `feat(network): add contract check matching and verdicts`.
