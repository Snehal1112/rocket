# Contract Types: Domain / Persistence / IPC Split — Design

**Date:** 2026-05-10
**Status:** Approved (brainstorming complete; ready for implementation plan)
**Branch:** `feat/contract-lock-enhancement` (follow-up to the contract-new feature)

---

## Background

The contract-new feature shipped 9 types that double as domain models, persistence records, and IPC DTOs:

- `Contract`, `ContractParty`, `ContractPolicy`, `ContractScope` (`rocket-collection/src/contract/types.rs`)
- `ChangelogEntry`, `ContractChangelog` (`rocket-collection/src/contract/changelog.rs`)
- `KeyValueEntry`, `RequestSignatureSnapshot` (`rocket-collection/src/contract/snapshot.rs`)
- `ContractSummary`, `ContractDriftSummary` (`rocket-app/src/contract_service.rs`)

All carry `#[serde(rename_all = "camelCase")]` so a single struct serves YAML on disk and JSON on the Tauri IPC wire. The CLAUDE.md hard rule states `camelCase` is for IPC DTOs only, never persistence structs. Today's code violates that rule and conflates three concerns in one type: domain semantics, on-disk schema (with legacy back-compat), and wire format.

The DDD layout in this repo already separates domain (`rocket-collection`), persistence (`rocket-infra`), and IPC (`src-tauri`). The refactor below restores that layering for contract types.

---

## Decisions

1. **YAML on disk stays camelCase forever.** No file migration. The persistence record type owns the serde rename.
2. **Two separate adapter type families** — `*Record` types in `rocket-infra` for YAML, `*Dto` types in `src-tauri` for IPC. Persistence and transport can evolve independently.
3. **Domain types carry no serde derives at all.** Only `Debug`, `Clone`, `PartialEq`. All serialization happens at adapter boundaries.

---

## Architecture

```
rocket-collection (domain — pure model, no serde)
  contract/types.rs        Contract, ContractParty, ContractPolicy, ContractScope
                           + enums: ContractStatus, PartyKind, BreakingChangePolicy
  contract/changelog.rs    ChangelogEntry, ContractChangelog
  contract/snapshot.rs     KeyValueEntry, RequestSignatureSnapshot

rocket-app (domain — service-layer summaries, no serde)
  contract_service.rs      ContractSummary, ContractDriftSummary

rocket-infra (persistence adapter — YAML, camelCase, owns back-compat)
  fs_contract_repo.rs (or new submodule)
    ContractRecord, ContractPartyRecord, ContractPolicyRecord, ContractScopeRecord
    ChangelogEntryRecord, ContractChangelogRecord
    KeyValueEntryRecord, RequestSignatureSnapshotRecord
    impl From<&Contract>            for ContractRecord            // write
    impl TryFrom<ContractRecord>    for Contract                  // read
    (similar for every Record/Domain pair)

src-tauri (IPC adapter — JSON, camelCase except rel_path)
  commands/contract.rs (or new submodule)
    ContractDto + sibling Dto types for every IPC-facing domain type,
    plus ContractSummaryDto, ContractDriftSummaryDto
    impl From<Contract>             for ContractDto               // response
    impl From<ContractDto>          for Contract                  // input  (or TryFrom for validating inputs)
```

**Round-trip:** disk → Record → Domain → service → Domain → Dto → frontend. Two conversions per request lifecycle.

---

## Conversion Strategy

- `From` for infallible same-shape mappings (the common case — Record/Dto fields match Domain fields 1:1).
- `TryFrom` for fallible conversions where parse/validation can fail (e.g., version-string parsing on input DTOs, semantic validation on `attach_contract` input).
- Conversion impls live next to the adapter type they target (`impl From<&Contract> for ContractRecord` in `fs_contract_repo.rs`).

---

## Back-Compat Ownership

All the legacy-format hacks currently polluting domain types move down to the Record layer:

| Hack (currently in domain) | New home |
|---|---|
| Custom `Deserialize` for old `provider`/`consumer` plain-string format | `ContractRecord` |
| `serde(default)` on `headers`, `query_params`, `auth_detail`, `body_content`, `form_fields` | `RequestSignatureSnapshotRecord` |
| `Option<NaiveDate>` for nullable `expiryDate` plus nullable `createdBy`/`createdAt`/`updatedAt` | `ContractRecord` |
| `serde(default, skip_serializing_if = "String::is_empty")` for the dead `project` field | `ContractRecord` |

Future YAML schema changes (new fields, renames, deletions) only touch Record types and their conversion impls. Domain stays untouched.

---

## IPC Special Case: `ContractScopeDto.rel_path`

`ContractScope` has a snake_case wire field `rel_path` (preserved by fix commit `52da6b0`, asserted by frontend `src/types/contracts.ts:21-24`). On `ContractScopeDto`:

```rust
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ContractScopeDto {
    Collection,
    Folder { #[serde(rename = "rel_path")] rel_path: String },
    Request { #[serde(rename = "rel_path")] rel_path: String },
}
```

`ContractScopeRecord` uses the same approach for YAML (already camelCase except `rel_path`).

---

## Migration Sequence

Each step must compile and pass tests before the next. No long-lived broken state.

1. **Add Record types** in `rocket-infra/src/fs_contract_repo.rs` (or new `rocket-infra/src/contract_records.rs`). Copy current camelCase serde derives + back-compat impls from domain. No call-site changes yet.
2. **Add conversion impls** between `Contract` and `ContractRecord` (and siblings). Move legacy custom `Deserialize` from domain into the Record types.
3. **Switch `fs_contract_repo`** to convert through Records on every read/write:
   - `save_contract`: `Contract` → `ContractRecord` → `serde_yaml::to_string`
   - `read_contract`: `serde_yaml::from_str` → `ContractRecord` → `Contract`
   - Same for `save_snapshot`, `append_changelog`, `read_changelog`, `list_contracts`.
   Domain still has serde at this point — both code paths compile.
4. **Add Dto types + conversion impls** in `src-tauri/src/commands/contract.rs` (or new `src-tauri/src/commands/contract_dtos.rs`). Service still returns/accepts domain types; commands convert at the boundary.
5. **Switch all 17 `#[tauri::command]` signatures** to use `*Dto` types. Update each command to convert input Dto → Domain before calling service, and convert service result Domain → Dto before returning.
6. **Strip serde from domain types** in `rocket-collection` and the two `rocket-app` summary types. Remove:
   - `#[derive(Serialize, Deserialize)]`
   - `#[serde(rename_all = "camelCase")]`
   - `#[serde(default)]`, `#[serde(rename = ...)]`, `#[serde(skip_serializing_if = ...)]`
   - Custom `impl Deserialize` blocks
   - Now-unused `use serde::*` imports.
   Compilation will fail wherever a non-adapter call site still serializes domain types directly — fix each one (use `Debug` formatting for logs, or convert through a Record/Dto).
7. **Full test matrix:**
   - `cargo check --workspace`
   - `cargo test -p rocket-collection`
   - `cargo test -p rocket-infra`
   - `cargo test -p rocket-app`
   - `cargo test -p rocket_lib` (the `src-tauri` library crate hosting Tauri command tests)
   - `yarn tsc --noEmit`
   - `yarn test`
   - `yarn playwright test e2e/contracts.spec.ts`

---

## Test Strategy

| Layer | Test |
|---|---|
| Domain | Existing state_machine, diff, drift, snapshot, changelog tests — unchanged. They use domain types. |
| Persistence (new) | Roundtrip `domain → Record → YAML string → Record → domain` for `Contract`, `ContractChangelog`, `RequestSignatureSnapshot`. |
| Persistence back-compat (new) | Fixture-driven tests reading legacy YAML strings: old plain-string `provider`/`consumer`; missing `headers`/`query_params`/`auth_detail`; null `expiryDate`/`createdAt`/`createdBy`/`updatedAt`. Each must deserialize successfully and produce the expected domain shape. |
| IPC (new) | Roundtrip `domain → Dto → JSON → Dto → domain` for every IPC-facing type, including `ContractScope` (`rel_path` snake-case preservation), `ContractSummary`, `ContractDriftSummary`. |
| E2E | `e2e/contracts.spec.ts` — unchanged. Verifies wire-format compatibility end-to-end via existing 5 scenarios. |

---

## Out of Scope

- No on-disk YAML migration (camelCase preserved per decision).
- No IPC wire-format change (Dtos preserve current shape including snake-case `rel_path`).
- No frontend type changes — `src/types/contracts.ts` is unaffected.
- No changes to the contract-new feature behavior, business logic, or state machine.
- No deprecation of `Contract.project` (left for a separate cleanup).

---

## Risk & Rollback

- **Risk:** step 6 (strip serde from domain) will surface every implicit serde dependency in non-adapter code (logging snippets, dev-only debug, tests that JSON-serialize domain types). Each is a localized fix. The compiler enumerates them.
- **Rollback:** the migration is git-revertable per step. If the refactor proves too costly, revert from step 6 backward; the codebase still works at any earlier step (domain still has serde, adapter types are dead code but harmless).
- **Validation:** Playwright e2e is the strongest signal that wire format is preserved. Run it after step 5 and step 6.

---

## Files Touched (estimated)

| File | Change |
|---|---|
| `crates/rocket-collection/src/contract/types.rs` | strip serde, remove custom Deserialize |
| `crates/rocket-collection/src/contract/changelog.rs` | strip serde |
| `crates/rocket-collection/src/contract/snapshot.rs` | strip serde, remove `serde(default)` |
| `crates/rocket-app/src/contract_service.rs` | strip serde from `ContractSummary`, `ContractDriftSummary` |
| `crates/rocket-infra/src/fs_contract_repo.rs` | add Record types, From/TryFrom impls, switch read/write to convert through Records |
| `crates/rocket-infra/src/contract_records.rs` (new, optional) | Record types if too large for fs_contract_repo.rs |
| `src-tauri/src/commands/contract.rs` | add Dto types, From impls, switch all 17 command signatures |
| `src-tauri/src/commands/contract_dtos.rs` (new, optional) | Dto types if too large for contract.rs |
| Test files | new persistence + IPC roundtrip tests; existing domain tests unchanged |

Estimated diff: ~1500–2000 lines added (Record + Dto + From impls + tests), ~300 lines removed (serde derives + custom Deserialize from domain). Net new ≈ +1500.

---

## Acceptance Criteria

- `crates/rocket-collection/src/contract/**` and `crates/rocket-app/src/contract_service.rs` contain zero `serde` references for the affected types.
- `rocket-infra` owns all YAML serialization of contract data; legacy formats deserialize via `ContractRecord`.
- `src-tauri/src/commands/contract.rs` exposes only Dto types in `#[tauri::command]` signatures.
- All existing tests pass.
- `e2e/contracts.spec.ts` 5/5 scenarios pass.
- On-disk YAML format is byte-equivalent to before the refactor for any roundtripped contract.
