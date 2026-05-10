# Contract DTO / Persistence / Domain Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore the DDD layering for contract types — strip serde from domain types in `rocket-collection`/`rocket-app`, move YAML serialization to new `*Record` types in `rocket-infra`, and move IPC serialization to new `*Dto` types in `src-tauri`.

**Architecture:** Three layers, three type families. Domain types carry only `Debug/Clone/PartialEq`. `*Record` types in `rocket-infra` own YAML serialization (camelCase preserved on disk, plus all back-compat deserializers). `*Dto` types in `src-tauri` own IPC JSON. `From`/`TryFrom` impls bridge each boundary. On-disk YAML format is unchanged byte-for-byte.

**Tech Stack:** Rust (workspace crates), `serde_yaml`, `chrono`, `ulid`. Frontend types untouched.

**Spec:** `docs/superpowers/specs/2026-05-10-contract-dto-persistence-split-design.md`

**Working directory for ALL Rust work:** `/Users/snehaldangroshiya/data/rocket/.worktrees/contract-enhancement/` on branch `feat/contract-lock-enhancement` (or a fix branch off it — see Task 1).

---

## File Structure

**New files:**

| File | Responsibility |
|---|---|
| `crates/rocket-infra/src/contract_records/mod.rs` | Module root, re-exports |
| `crates/rocket-infra/src/contract_records/types.rs` | `ContractRecord`, `ContractPartyRecord`, `ContractPolicyRecord`, `ContractScopeRecord` + `PartyKindRecord`, `BreakingChangePolicyRecord`, `ContractStatusRecord`, `ContractEnforcementModeRecord` + From/TryFrom impls. Owns custom `Deserialize` for old `provider`/`consumer` strings + nullable date fields. |
| `crates/rocket-infra/src/contract_records/changelog.rs` | `ChangelogEntryRecord`, `ContractChangelogRecord`, `ChangeTypeRecord` + From impls |
| `crates/rocket-infra/src/contract_records/snapshot.rs` | `KeyValueEntryRecord`, `RequestSignatureSnapshotRecord`, `ContractSnapshotRecord` + From impls. Owns `serde(default)` on legacy fields. |
| `src-tauri/src/commands/contract_dtos/mod.rs` | Module root, re-exports |
| `src-tauri/src/commands/contract_dtos/types.rs` | `ContractDto`, `ContractPartyDto`, `ContractPolicyDto`, `ContractScopeDto` + sibling enum DTOs + From impls |
| `src-tauri/src/commands/contract_dtos/changelog.rs` | `ChangelogEntryDto`, `ContractChangelogDto`, `ChangeTypeDto` + From impls |
| `src-tauri/src/commands/contract_dtos/snapshot.rs` | `KeyValueEntryDto`, `RequestSignatureSnapshotDto` + From impls |
| `src-tauri/src/commands/contract_dtos/summary.rs` | `ContractSummaryDto`, `ContractDriftSummaryDto` + From impls |

**Modified files:**

| File | Change |
|---|---|
| `crates/rocket-infra/src/lib.rs` | `pub mod contract_records;` |
| `crates/rocket-infra/src/fs_contract_repo.rs` | Read/write through `*Record` types instead of domain types |
| `src-tauri/src/commands/mod.rs` | `pub mod contract_dtos;` (or add to existing module list) |
| `src-tauri/src/commands/contract.rs` | All 17 `#[tauri::command]` signatures use `*Dto` types |
| `crates/rocket-collection/src/contract/types.rs` | Strip serde derives + custom Deserialize (last) |
| `crates/rocket-collection/src/contract/changelog.rs` | Strip serde derives (last) |
| `crates/rocket-collection/src/contract/snapshot.rs` | Strip serde derives (last) |
| `crates/rocket-app/src/contract_service.rs` | Strip serde derives from `ContractSummary` and `ContractDriftSummary` (last) |

---

## Tasks

### Task 1: Create fix branch off the feature branch

**Files:**
- Workspace: `/Users/snehaldangroshiya/data/rocket/.worktrees/contract-enhancement/`

- [ ] **Step 1: Confirm worktree state**

```bash
cd /Users/snehaldangroshiya/data/rocket/.worktrees/contract-enhancement
git status
git rev-parse --abbrev-ref HEAD
```

Expected: clean working tree, branch `feat/contract-lock-enhancement` (or a quick-fix branch already created off it — that's also fine; treat its tip as the new BASE).

- [ ] **Step 2: Create the refactor branch**

```bash
git checkout -b refactor/contract-dto-persistence-split
```

Expected: `Switched to a new branch 'refactor/contract-dto-persistence-split'`.

- [ ] **Step 3: Confirm baseline tests pass**

```bash
cargo check --workspace
cargo test -p rocket-collection contract::
cargo test -p rocket-app contract_service
```

Expected: cargo check succeeds; both test invocations show all tests passing (60 + 21 = 81 total at baseline).

---

### Task 2: Scaffold `contract_records` module in rocket-infra

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-infra/src/contract_records/mod.rs`
- Create: `crates/rocket-infra/src/contract_records/types.rs` (empty placeholder)
- Create: `crates/rocket-infra/src/contract_records/changelog.rs` (empty placeholder)
- Create: `crates/rocket-infra/src/contract_records/snapshot.rs` (empty placeholder)
- Modify: `crates/rocket-infra/src/lib.rs` (add `pub mod contract_records;`)

- [ ] **Step 1: Create empty submodule files**

Each new file gets a one-line doc comment and nothing else for now. This keeps tasks isolated.

`crates/rocket-infra/src/contract_records/mod.rs`:
```rust
//! YAML persistence records for contract types.
//!
//! Records live here (the adapter layer) so domain types in `rocket-collection`
//! stay free of serde wire-format concerns. Records use camelCase YAML for
//! on-disk compatibility with files written by previous versions; back-compat
//! custom Deserialize impls live with the Record types they target.

pub mod changelog;
pub mod snapshot;
pub mod types;
```

`crates/rocket-infra/src/contract_records/types.rs`:
```rust
//! Persistence records for `Contract` and its sub-types (`ContractParty`,
//! `ContractPolicy`, `ContractScope`, plus the four enums).
```

`crates/rocket-infra/src/contract_records/changelog.rs`:
```rust
//! Persistence records for `ChangelogEntry` and `ContractChangelog`.
```

`crates/rocket-infra/src/contract_records/snapshot.rs`:
```rust
//! Persistence records for `KeyValueEntry`, `RequestSignatureSnapshot`,
//! and `ContractSnapshot`.
```

- [ ] **Step 2: Register the module in `lib.rs`**

Open `crates/rocket-infra/src/lib.rs`, find the `pub mod` lines (near the top of the file), add:

```rust
pub mod contract_records;
```

Place it alphabetically with the other `pub mod` declarations.

- [ ] **Step 3: Verify**

```bash
cargo check -p rocket-infra
```

Expected: success, no warnings about empty modules (the doc comments suppress them).

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-infra/src/contract_records/ crates/rocket-infra/src/lib.rs
git commit -m "refactor(contract): scaffold contract_records module in rocket-infra"
```

---

### Task 3: `ContractPartyRecord` + `PartyKindRecord` with back-compat string Deserialize

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/src/contract_records/types.rs`

- [ ] **Step 1: Write the failing roundtrip + back-compat tests**

Append to `crates/rocket-infra/src/contract_records/types.rs`:

```rust
use rocket_collection::contract::types::{ContractParty, PartyKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractPartyRecord {
    pub id: String,
    pub name: String,
    pub kind: PartyKindRecord,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_seed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PartyKindRecord {
    #[default]
    Team,
    Company,
    Service,
}

impl<'de> serde::Deserialize<'de> for ContractPartyRecord {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::{self, MapAccess, Visitor};
        use std::fmt;

        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = ContractPartyRecord;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a string or a ContractPartyRecord object")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<ContractPartyRecord, E> {
                Ok(ContractPartyRecord {
                    id: v.to_lowercase().replace(' ', "-"),
                    name: v.to_string(),
                    kind: PartyKindRecord::Team,
                    avatar_seed: None,
                    avatar_color: None,
                })
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<ContractPartyRecord, E> {
                self.visit_str(&v)
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<ContractPartyRecord, A::Error> {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Helper {
                    id: String,
                    name: String,
                    #[serde(default)]
                    kind: PartyKindRecord,
                    #[serde(default)]
                    avatar_seed: Option<String>,
                    #[serde(default)]
                    avatar_color: Option<String>,
                }
                let h = Helper::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                Ok(ContractPartyRecord {
                    id: h.id,
                    name: h.name,
                    kind: h.kind,
                    avatar_seed: h.avatar_seed,
                    avatar_color: h.avatar_color,
                })
            }
        }

        d.deserialize_any(V)
    }
}

// Conversions ---------------------------------------------------------------

impl From<&PartyKind> for PartyKindRecord {
    fn from(k: &PartyKind) -> Self {
        match k {
            PartyKind::Team => PartyKindRecord::Team,
            PartyKind::Company => PartyKindRecord::Company,
            PartyKind::Service => PartyKindRecord::Service,
        }
    }
}

impl From<PartyKindRecord> for PartyKind {
    fn from(r: PartyKindRecord) -> Self {
        match r {
            PartyKindRecord::Team => PartyKind::Team,
            PartyKindRecord::Company => PartyKind::Company,
            PartyKindRecord::Service => PartyKind::Service,
        }
    }
}

impl From<&ContractParty> for ContractPartyRecord {
    fn from(p: &ContractParty) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            kind: (&p.kind).into(),
            avatar_seed: p.avatar_seed.clone(),
            avatar_color: p.avatar_color.clone(),
        }
    }
}

impl From<ContractPartyRecord> for ContractParty {
    fn from(r: ContractPartyRecord) -> Self {
        Self {
            id: r.id,
            name: r.name,
            kind: r.kind.into(),
            avatar_seed: r.avatar_seed,
            avatar_color: r.avatar_color,
        }
    }
}

#[cfg(test)]
mod party_tests {
    use super::*;

    #[test]
    fn party_record_roundtrip_yaml() {
        let r = ContractPartyRecord {
            id: "billing-team".into(),
            name: "Billing Team".into(),
            kind: PartyKindRecord::Team,
            avatar_seed: None,
            avatar_color: Some("#3B82F6".into()),
        };
        let yaml = serde_yaml::to_string(&r).unwrap();
        let back: ContractPartyRecord = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn party_record_deserialises_plain_string() {
        let r: ContractPartyRecord = serde_yaml::from_str("\"Billing Team\"").unwrap();
        assert_eq!(r.name, "Billing Team");
        assert_eq!(r.id, "billing-team");
        assert_eq!(r.kind, PartyKindRecord::Team);
    }

    #[test]
    fn party_record_yaml_uses_camel_case_field_names() {
        let r = ContractPartyRecord {
            id: "x".into(),
            name: "X".into(),
            kind: PartyKindRecord::Team,
            avatar_seed: Some("seed-v".into()),
            avatar_color: None,
        };
        let yaml = serde_yaml::to_string(&r).unwrap();
        assert!(yaml.contains("avatarSeed"), "expected camelCase field, got:\n{yaml}");
        assert!(!yaml.contains("avatar_seed"), "snake_case must not leak into YAML, got:\n{yaml}");
    }

    #[test]
    fn domain_to_record_roundtrip() {
        let domain = ContractParty::from_name("Platform Team");
        let record: ContractPartyRecord = (&domain).into();
        let back: ContractParty = record.into();
        assert_eq!(domain, back);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p rocket-infra contract_records::types::party_tests
```

Expected: tests don't exist yet (or fail to compile if you ran step 1 in a single edit). Address compile errors if any (likely the `rocket-collection` dependency — already in `rocket-infra/Cargo.toml`, should compile).

- [ ] **Step 3: Run tests to verify they pass**

```bash
cargo test -p rocket-infra contract_records::types::party_tests
```

Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-infra/src/contract_records/types.rs
git commit -m "refactor(contract): add ContractPartyRecord with back-compat string deserialize"
```

---

### Task 4: `ContractPolicyRecord` + `BreakingChangePolicyRecord`

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/src/contract_records/types.rs`

- [ ] **Step 1: Append types and conversions**

Append to `crates/rocket-infra/src/contract_records/types.rs`:

```rust
use rocket_collection::contract::types::{BreakingChangePolicy, ContractPolicy};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractPolicyRecord {
    #[serde(default)]
    pub breaking_change_policy: BreakingChangePolicyRecord,
    #[serde(default = "default_notice_days_record")]
    pub notice_days: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime_sla: Option<f32>,
}

impl Default for ContractPolicyRecord {
    fn default() -> Self {
        Self {
            breaking_change_policy: BreakingChangePolicyRecord::Lenient,
            notice_days: 30,
            uptime_sla: None,
        }
    }
}

fn default_notice_days_record() -> u32 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BreakingChangePolicyRecord {
    Strict,
    #[default]
    Lenient,
    AdditiveOk,
}

impl From<&BreakingChangePolicy> for BreakingChangePolicyRecord {
    fn from(p: &BreakingChangePolicy) -> Self {
        match p {
            BreakingChangePolicy::Strict => BreakingChangePolicyRecord::Strict,
            BreakingChangePolicy::Lenient => BreakingChangePolicyRecord::Lenient,
            BreakingChangePolicy::AdditiveOk => BreakingChangePolicyRecord::AdditiveOk,
        }
    }
}

impl From<BreakingChangePolicyRecord> for BreakingChangePolicy {
    fn from(r: BreakingChangePolicyRecord) -> Self {
        match r {
            BreakingChangePolicyRecord::Strict => BreakingChangePolicy::Strict,
            BreakingChangePolicyRecord::Lenient => BreakingChangePolicy::Lenient,
            BreakingChangePolicyRecord::AdditiveOk => BreakingChangePolicy::AdditiveOk,
        }
    }
}

impl From<&ContractPolicy> for ContractPolicyRecord {
    fn from(p: &ContractPolicy) -> Self {
        Self {
            breaking_change_policy: (&p.breaking_change_policy).into(),
            notice_days: p.notice_days,
            uptime_sla: p.uptime_sla,
        }
    }
}

impl From<ContractPolicyRecord> for ContractPolicy {
    fn from(r: ContractPolicyRecord) -> Self {
        Self {
            breaking_change_policy: r.breaking_change_policy.into(),
            notice_days: r.notice_days,
            uptime_sla: r.uptime_sla,
        }
    }
}

#[cfg(test)]
mod policy_tests {
    use super::*;

    #[test]
    fn policy_record_defaults_from_empty_yaml() {
        let p: ContractPolicyRecord = serde_yaml::from_str("{}").unwrap();
        assert_eq!(p.breaking_change_policy, BreakingChangePolicyRecord::Lenient);
        assert_eq!(p.notice_days, 30);
        assert!(p.uptime_sla.is_none());
    }

    #[test]
    fn policy_record_roundtrip() {
        let p = ContractPolicyRecord {
            breaking_change_policy: BreakingChangePolicyRecord::Strict,
            notice_days: 14,
            uptime_sla: Some(99.9),
        };
        let yaml = serde_yaml::to_string(&p).unwrap();
        assert!(yaml.contains("breakingChangePolicy"), "got:\n{yaml}");
        let back: ContractPolicyRecord = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn breaking_policy_record_uses_snake_case() {
        let y = serde_yaml::to_string(&BreakingChangePolicyRecord::AdditiveOk).unwrap();
        assert!(y.contains("additive_ok"), "got: {y}");
    }

    #[test]
    fn domain_policy_record_roundtrip() {
        let domain = ContractPolicy {
            breaking_change_policy: BreakingChangePolicy::AdditiveOk,
            notice_days: 7,
            uptime_sla: Some(95.0),
        };
        let r: ContractPolicyRecord = (&domain).into();
        let back: ContractPolicy = r.into();
        assert_eq!(domain, back);
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p rocket-infra contract_records::types::policy_tests
```

Expected: 4 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-infra/src/contract_records/types.rs
git commit -m "refactor(contract): add ContractPolicyRecord with snake_case enum YAML"
```

---

### Task 5: `ContractScopeRecord` (preserve snake_case `rel_path`)

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/src/contract_records/types.rs`

- [ ] **Step 1: Append**

```rust
use rocket_collection::contract::types::ContractScope;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContractScopeRecord {
    Collection,
    Folder { rel_path: PathBuf },
    Request { rel_path: PathBuf },
}

impl From<&ContractScope> for ContractScopeRecord {
    fn from(s: &ContractScope) -> Self {
        match s {
            ContractScope::Collection => ContractScopeRecord::Collection,
            ContractScope::Folder { rel_path } => ContractScopeRecord::Folder { rel_path: rel_path.clone() },
            ContractScope::Request { rel_path } => ContractScopeRecord::Request { rel_path: rel_path.clone() },
        }
    }
}

impl From<ContractScopeRecord> for ContractScope {
    fn from(r: ContractScopeRecord) -> Self {
        match r {
            ContractScopeRecord::Collection => ContractScope::Collection,
            ContractScopeRecord::Folder { rel_path } => ContractScope::Folder { rel_path },
            ContractScopeRecord::Request { rel_path } => ContractScope::Request { rel_path },
        }
    }
}

#[cfg(test)]
mod scope_tests {
    use super::*;

    #[test]
    fn scope_record_folder_yaml_uses_snake_case_rel_path() {
        let s = ContractScopeRecord::Folder { rel_path: PathBuf::from("auth/login.yml") };
        let yaml = serde_yaml::to_string(&s).unwrap();
        assert!(yaml.contains("rel_path:"), "expected rel_path in:\n{yaml}");
        assert!(!yaml.contains("relPath:"), "camelCase relPath must NOT appear in:\n{yaml}");
    }

    #[test]
    fn scope_record_request_yaml_uses_snake_case_rel_path() {
        let s = ContractScopeRecord::Request { rel_path: PathBuf::from("users/get.yml") };
        let yaml = serde_yaml::to_string(&s).unwrap();
        assert!(yaml.contains("rel_path:"));
    }

    #[test]
    fn scope_record_collection_roundtrip() {
        let s = ContractScopeRecord::Collection;
        let yaml = serde_yaml::to_string(&s).unwrap();
        let back: ContractScopeRecord = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn domain_scope_record_roundtrip() {
        let domain = ContractScope::Folder { rel_path: PathBuf::from("a/b.yml") };
        let r: ContractScopeRecord = (&domain).into();
        let back: ContractScope = r.into();
        assert_eq!(domain, back);
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p rocket-infra contract_records::types::scope_tests
```

Expected: 4 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-infra/src/contract_records/types.rs
git commit -m "refactor(contract): add ContractScopeRecord preserving snake_case rel_path"
```

---

### Task 6: `ContractStatusRecord` + `ContractEnforcementModeRecord`

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/src/contract_records/types.rs`

- [ ] **Step 1: Append**

```rust
use rocket_collection::contract::types::{ContractEnforcementMode, ContractStatus};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContractStatusRecord {
    Draft,
    #[default]
    Active,
    Drift,
    Breach,
    InReview,
    Paused,
    ExpiringIn30Days,
    Expired,
}

impl From<&ContractStatus> for ContractStatusRecord {
    fn from(s: &ContractStatus) -> Self {
        match s {
            ContractStatus::Draft => Self::Draft,
            ContractStatus::Active => Self::Active,
            ContractStatus::Drift => Self::Drift,
            ContractStatus::Breach => Self::Breach,
            ContractStatus::InReview => Self::InReview,
            ContractStatus::Paused => Self::Paused,
            ContractStatus::ExpiringIn30Days => Self::ExpiringIn30Days,
            ContractStatus::Expired => Self::Expired,
        }
    }
}

impl From<ContractStatusRecord> for ContractStatus {
    fn from(r: ContractStatusRecord) -> Self {
        match r {
            ContractStatusRecord::Draft => Self::Draft,
            ContractStatusRecord::Active => Self::Active,
            ContractStatusRecord::Drift => Self::Drift,
            ContractStatusRecord::Breach => Self::Breach,
            ContractStatusRecord::InReview => Self::InReview,
            ContractStatusRecord::Paused => Self::Paused,
            ContractStatusRecord::ExpiringIn30Days => Self::ExpiringIn30Days,
            ContractStatusRecord::Expired => Self::Expired,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContractEnforcementModeRecord {
    #[default]
    Informational,
    Warn,
    Block,
}

impl From<&ContractEnforcementMode> for ContractEnforcementModeRecord {
    fn from(e: &ContractEnforcementMode) -> Self {
        match e {
            ContractEnforcementMode::Informational => Self::Informational,
            ContractEnforcementMode::Warn => Self::Warn,
            ContractEnforcementMode::Block => Self::Block,
        }
    }
}

impl From<ContractEnforcementModeRecord> for ContractEnforcementMode {
    fn from(r: ContractEnforcementModeRecord) -> Self {
        match r {
            ContractEnforcementModeRecord::Informational => Self::Informational,
            ContractEnforcementModeRecord::Warn => Self::Warn,
            ContractEnforcementModeRecord::Block => Self::Block,
        }
    }
}

#[cfg(test)]
mod enum_tests {
    use super::*;

    #[test]
    fn status_record_active_serialises_unchanged() {
        let y = serde_yaml::to_string(&ContractStatusRecord::Active).unwrap();
        assert_eq!(y.trim(), "active");
    }

    #[test]
    fn status_record_in_review_uses_snake_case() {
        let y = serde_yaml::to_string(&ContractStatusRecord::InReview).unwrap();
        assert_eq!(y.trim(), "in_review");
    }

    #[test]
    fn status_record_expiring_in_30_days_uses_snake_case() {
        let y = serde_yaml::to_string(&ContractStatusRecord::ExpiringIn30Days).unwrap();
        assert_eq!(y.trim(), "expiring_in_30_days");
    }

    #[test]
    fn enforcement_mode_record_roundtrip() {
        for m in [ContractEnforcementMode::Informational, ContractEnforcementMode::Warn, ContractEnforcementMode::Block] {
            let r: ContractEnforcementModeRecord = (&m).into();
            let back: ContractEnforcementMode = r.into();
            assert_eq!(m, back);
        }
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p rocket-infra contract_records::types::enum_tests
```

Expected: 4 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-infra/src/contract_records/types.rs
git commit -m "refactor(contract): add ContractStatusRecord and ContractEnforcementModeRecord"
```

---

### Task 7: `ContractRecord` with custom Deserialize for `consumer`-singular and nullable dates

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/src/contract_records/types.rs`

- [ ] **Step 1: Append `ContractRecord` and its custom Deserialize**

```rust
use chrono::{DateTime, NaiveDate, Utc};
use rocket_collection::contract::types::Contract;
use ulid::Ulid;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractRecord {
    pub id: Ulid,
    pub title: String,

    pub provider: ContractPartyRecord,
    pub consumers: Vec<ContractPartyRecord>,

    pub project: String,

    #[serde(default = "default_version_record")]
    pub version: String,

    #[serde(default)]
    pub status: ContractStatusRecord,

    pub effective_date: NaiveDate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiry_date: Option<NaiveDate>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub document_paths: Vec<PathBuf>,

    pub enforcement_mode: ContractEnforcementModeRecord,
    pub scope: ContractScopeRecord,

    #[serde(default)]
    pub policy: ContractPolicyRecord,

    #[serde(default)]
    pub drift_count: u32,
    #[serde(default)]
    pub breach_count: u32,
    #[serde(default)]
    pub endpoint_count: u32,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

fn default_version_record() -> String {
    "1.0.0".to_string()
}

impl<'de> serde::Deserialize<'de> for ContractRecord {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::{Error, MapAccess, Visitor};
        use std::fmt;

        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = ContractRecord;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a ContractRecord object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<ContractRecord, A::Error> {
                let mut id = None;
                let mut title = None;
                let mut provider = None;
                let mut consumers: Option<Vec<ContractPartyRecord>> = None;
                let mut consumer_singular: Option<ContractPartyRecord> = None;
                let mut project = None;
                let mut version = None;
                let mut status = None;
                let mut effective_date = None;
                let mut expiry_date = None;
                let mut document_paths = None;
                let mut enforcement_mode = None;
                let mut scope = None;
                let mut policy = None;
                let mut drift_count = None;
                let mut breach_count = None;
                let mut endpoint_count = None;
                let mut created_by = None;
                let mut created_at = None;
                let mut updated_at = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "id" => id = Some(map.next_value()?),
                        "title" => title = Some(map.next_value()?),
                        "provider" => provider = Some(map.next_value()?),
                        "consumers" => consumers = Some(map.next_value()?),
                        "consumer" => consumer_singular = Some(map.next_value()?),
                        "project" => project = Some(map.next_value()?),
                        "version" => version = Some(map.next_value()?),
                        "status" => status = Some(map.next_value()?),
                        "effectiveDate" => effective_date = Some(map.next_value()?),
                        "expiryDate" => expiry_date = map.next_value()?,
                        "documentPaths" => document_paths = Some(map.next_value()?),
                        "enforcementMode" => enforcement_mode = Some(map.next_value()?),
                        "scope" => scope = Some(map.next_value()?),
                        "policy" => policy = Some(map.next_value()?),
                        "driftCount" => drift_count = Some(map.next_value()?),
                        "breachCount" => breach_count = Some(map.next_value()?),
                        "endpointCount" => endpoint_count = Some(map.next_value()?),
                        "createdBy" => created_by = map.next_value()?,
                        "createdAt" => created_at = map.next_value()?,
                        "updatedAt" => updated_at = map.next_value()?,
                        _ => { let _ = map.next_value::<serde::de::IgnoredAny>()?; }
                    }
                }

                let resolved_consumers = consumers
                    .or_else(|| consumer_singular.map(|c| vec![c]))
                    .unwrap_or_default();

                Ok(ContractRecord {
                    id: id.ok_or_else(|| A::Error::missing_field("id"))?,
                    title: title.ok_or_else(|| A::Error::missing_field("title"))?,
                    provider: provider.ok_or_else(|| A::Error::missing_field("provider"))?,
                    consumers: resolved_consumers,
                    project: project.unwrap_or_default(),
                    version: version.unwrap_or_else(default_version_record),
                    status: status.unwrap_or_default(),
                    effective_date: effective_date.ok_or_else(|| A::Error::missing_field("effectiveDate"))?,
                    expiry_date,
                    document_paths: document_paths.unwrap_or_default(),
                    enforcement_mode: enforcement_mode.unwrap_or_default(),
                    scope: scope.ok_or_else(|| A::Error::missing_field("scope"))?,
                    policy: policy.unwrap_or_default(),
                    drift_count: drift_count.unwrap_or(0),
                    breach_count: breach_count.unwrap_or(0),
                    endpoint_count: endpoint_count.unwrap_or(0),
                    created_by,
                    created_at,
                    updated_at,
                })
            }
        }

        d.deserialize_map(V)
    }
}

impl From<&Contract> for ContractRecord {
    fn from(c: &Contract) -> Self {
        Self {
            id: c.id,
            title: c.title.clone(),
            provider: (&c.provider).into(),
            consumers: c.consumers.iter().map(Into::into).collect(),
            project: c.project.clone(),
            version: c.version.clone(),
            status: (&c.status).into(),
            effective_date: c.effective_date,
            expiry_date: c.expiry_date,
            document_paths: c.document_paths.clone(),
            enforcement_mode: (&c.enforcement_mode).into(),
            scope: (&c.scope).into(),
            policy: (&c.policy).into(),
            drift_count: c.drift_count,
            breach_count: c.breach_count,
            endpoint_count: c.endpoint_count,
            created_by: c.created_by.clone(),
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

impl From<ContractRecord> for Contract {
    fn from(r: ContractRecord) -> Self {
        Self {
            id: r.id,
            title: r.title,
            provider: r.provider.into(),
            consumers: r.consumers.into_iter().map(Into::into).collect(),
            project: r.project,
            version: r.version,
            status: r.status.into(),
            effective_date: r.effective_date,
            expiry_date: r.expiry_date,
            document_paths: r.document_paths,
            enforcement_mode: r.enforcement_mode.into(),
            scope: r.scope.into(),
            policy: r.policy.into(),
            drift_count: r.drift_count,
            breach_count: r.breach_count,
            endpoint_count: r.endpoint_count,
            created_by: r.created_by,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}
```

- [ ] **Step 2: Add tests**

Append:

```rust
#[cfg(test)]
mod contract_record_tests {
    use super::*;

    #[test]
    fn old_yaml_provider_string_deserialises_via_record() {
        let yaml = r#"
id: 01ARZ3NDEKTSV4RRFFQ69G5FAV
title: Payments API
provider: "Billing Team"
consumer: "Platform Team"
project: Checkout
version: "1.0.0"
effectiveDate: "2026-01-15"
enforcementMode: informational
scope:
  type: collection
"#;
        let r: ContractRecord = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(r.provider.name, "Billing Team");
        assert_eq!(r.provider.id, "billing-team");
        assert_eq!(r.consumers.len(), 1);
        assert_eq!(r.consumers[0].name, "Platform Team");
        assert_eq!(r.status, ContractStatusRecord::Active);
    }

    #[test]
    fn old_yaml_expiry_date_null_deserialises_via_record() {
        let yaml = r#"
id: 01KR4KAPFKR5YM1T1WSD2YYGC0
title: Jjjjj
provider: Jhggg
consumer: Yyyy
project: Uuuu
version: V222
effectiveDate: 2026-05-08
expiryDate: null
documentPaths: []
enforcementMode: informational
scope:
  type: collection
"#;
        let r: ContractRecord = serde_yaml::from_str(yaml).unwrap();
        assert!(r.expiry_date.is_none());
    }

    #[test]
    fn old_yaml_created_at_null_deserialises_via_record() {
        let yaml = r#"
id: 01ARZ3NDEKTSV4RRFFQ69G5FAV
title: Test
provider: Provider
consumer: Consumer
project: ''
version: "1.0.0"
effectiveDate: "2026-01-15"
enforcementMode: informational
scope:
  type: collection
createdBy: null
createdAt: null
updatedAt: null
"#;
        let r: ContractRecord = serde_yaml::from_str(yaml).unwrap();
        assert!(r.created_by.is_none());
        assert!(r.created_at.is_none());
        assert!(r.updated_at.is_none());
    }

    #[test]
    fn domain_to_record_to_domain_roundtrip() {
        use rocket_collection::contract::types::{
            BreakingChangePolicy, ContractEnforcementMode, ContractPolicy, ContractScope, ContractStatus,
        };
        let domain = Contract {
            id: Ulid::new(),
            title: "X".into(),
            provider: ContractParty::from_name("Prov"),
            consumers: vec![ContractParty::from_name("Cons")],
            project: "P".into(),
            version: "1.2.3".into(),
            status: ContractStatus::Drift,
            effective_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            expiry_date: None,
            document_paths: vec![],
            enforcement_mode: ContractEnforcementMode::Informational,
            scope: ContractScope::Collection,
            policy: ContractPolicy {
                breaking_change_policy: BreakingChangePolicy::Strict,
                notice_days: 14,
                uptime_sla: Some(99.5),
            },
            drift_count: 2,
            breach_count: 0,
            endpoint_count: 5,
            created_by: Some("alice".into()),
            created_at: None,
            updated_at: None,
        };
        let r: ContractRecord = (&domain).into();
        let back: Contract = r.into();
        assert_eq!(domain, back);
    }

    #[test]
    fn yaml_roundtrip_preserves_camel_case_field_names() {
        use rocket_collection::contract::types::{ContractEnforcementMode, ContractScope, ContractStatus};
        let domain = Contract {
            id: Ulid::new(),
            title: "Y".into(),
            provider: ContractParty::from_name("Prov"),
            consumers: vec![],
            project: String::new(),
            version: "1.0.0".into(),
            status: ContractStatus::Active,
            effective_date: NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
            expiry_date: Some(NaiveDate::from_ymd_opt(2026, 12, 31).unwrap()),
            document_paths: vec![],
            enforcement_mode: ContractEnforcementMode::Informational,
            scope: ContractScope::Folder { rel_path: PathBuf::from("a.yml") },
            policy: ContractPolicy::default(),
            drift_count: 0,
            breach_count: 0,
            endpoint_count: 0,
            created_by: None,
            created_at: None,
            updated_at: None,
        };
        let r: ContractRecord = (&domain).into();
        let yaml = serde_yaml::to_string(&r).unwrap();
        assert!(yaml.contains("effectiveDate:"), "expected camelCase in:\n{yaml}");
        assert!(yaml.contains("expiryDate:"));
        assert!(yaml.contains("enforcementMode:"));
        assert!(yaml.contains("rel_path:"), "scope rel_path must remain snake_case in:\n{yaml}");
        let back: ContractRecord = serde_yaml::from_str(&yaml).unwrap();
        let back_domain: Contract = back.into();
        assert_eq!(domain, back_domain);
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p rocket-infra contract_records::types::contract_record_tests
```

Expected: 5 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-infra/src/contract_records/types.rs
git commit -m "refactor(contract): add ContractRecord with full back-compat YAML deserializer"
```

---

### Task 8: Changelog records (`ChangeTypeRecord`, `ChangelogEntryRecord`, `ContractChangelogRecord`)

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/src/contract_records/changelog.rs`

- [ ] **Step 1: Write the module contents**

Replace the placeholder doc-comment in `crates/rocket-infra/src/contract_records/changelog.rs` with:

```rust
//! Persistence records for `ChangelogEntry` and `ContractChangelog`.

use chrono::{DateTime, Utc};
use rocket_collection::contract::changelog::{ChangeType, ChangelogEntry, ContractChangelog};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ulid::Ulid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeTypeRecord {
    Changed,
    Added,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChangelogEntryRecord {
    pub timestamp: DateTime<Utc>,
    pub request_path: PathBuf,
    pub field: String,
    pub change_type: ChangeTypeRecord,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_value: Option<String>,
    #[serde(default)]
    pub is_breaking: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractChangelogRecord {
    pub contract_id: Ulid,
    pub entries: Vec<ChangelogEntryRecord>,
}

impl From<&ChangeType> for ChangeTypeRecord {
    fn from(c: &ChangeType) -> Self {
        match c {
            ChangeType::Changed => Self::Changed,
            ChangeType::Added => Self::Added,
            ChangeType::Removed => Self::Removed,
        }
    }
}

impl From<ChangeTypeRecord> for ChangeType {
    fn from(r: ChangeTypeRecord) -> Self {
        match r {
            ChangeTypeRecord::Changed => Self::Changed,
            ChangeTypeRecord::Added => Self::Added,
            ChangeTypeRecord::Removed => Self::Removed,
        }
    }
}

impl From<&ChangelogEntry> for ChangelogEntryRecord {
    fn from(e: &ChangelogEntry) -> Self {
        Self {
            timestamp: e.timestamp,
            request_path: e.request_path.clone(),
            field: e.field.clone(),
            change_type: (&e.change_type).into(),
            old_value: e.old_value.clone(),
            new_value: e.new_value.clone(),
            is_breaking: e.is_breaking,
        }
    }
}

impl From<ChangelogEntryRecord> for ChangelogEntry {
    fn from(r: ChangelogEntryRecord) -> Self {
        Self {
            timestamp: r.timestamp,
            request_path: r.request_path,
            field: r.field,
            change_type: r.change_type.into(),
            old_value: r.old_value,
            new_value: r.new_value,
            is_breaking: r.is_breaking,
        }
    }
}

impl From<&ContractChangelog> for ContractChangelogRecord {
    fn from(c: &ContractChangelog) -> Self {
        Self {
            contract_id: c.contract_id,
            entries: c.entries.iter().map(Into::into).collect(),
        }
    }
}

impl From<ContractChangelogRecord> for ContractChangelog {
    fn from(r: ContractChangelogRecord) -> Self {
        Self {
            contract_id: r.contract_id,
            entries: r.entries.into_iter().map(Into::into).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_changelog_entry_without_is_breaking_defaults_false_via_record() {
        let yaml = r#"
timestamp: "2026-05-07T10:00:00Z"
requestPath: requests/payments.yml
field: method
changeType: changed
oldValue: GET
newValue: POST
"#;
        let r: ChangelogEntryRecord = serde_yaml::from_str(yaml).unwrap();
        assert!(!r.is_breaking);
    }

    #[test]
    fn changelog_entry_record_yaml_roundtrip_camel_case() {
        let r = ChangelogEntryRecord {
            timestamp: "2026-05-07T10:00:00Z".parse().unwrap(),
            request_path: PathBuf::from("a.yml"),
            field: "method".into(),
            change_type: ChangeTypeRecord::Changed,
            old_value: Some("GET".into()),
            new_value: Some("POST".into()),
            is_breaking: true,
        };
        let yaml = serde_yaml::to_string(&r).unwrap();
        assert!(yaml.contains("requestPath:"));
        assert!(yaml.contains("isBreaking:"));
        let back: ChangelogEntryRecord = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn domain_changelog_record_roundtrip() {
        let domain = ContractChangelog::new(Ulid::new());
        let r: ContractChangelogRecord = (&domain).into();
        let back: ContractChangelog = r.into();
        // ContractChangelog has no PartialEq; compare field-by-field.
        assert_eq!(domain.contract_id, back.contract_id);
        assert_eq!(domain.entries.len(), back.entries.len());
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p rocket-infra contract_records::changelog::tests
```

Expected: 3 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-infra/src/contract_records/changelog.rs
git commit -m "refactor(contract): add ContractChangelogRecord and entry record"
```

---

### Task 9: Snapshot records (`KeyValueEntryRecord`, `RequestSignatureSnapshotRecord`, `ContractSnapshotRecord`)

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/src/contract_records/snapshot.rs`

- [ ] **Step 1: Write the module**

Replace the placeholder doc-comment with:

```rust
//! Persistence records for `KeyValueEntry`, `RequestSignatureSnapshot`,
//! and `ContractSnapshot`. Owns `serde(default)` for legacy v0.6.x fields
//! (`headers`, `query_params`, `auth_detail`, etc.) so old on-disk snapshots
//! deserialise without error.

use chrono::{DateTime, Utc};
use rocket_collection::contract::snapshot::{
    ContractSnapshot, KeyValueEntry, RequestSignatureSnapshot,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ulid::Ulid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyValueEntryRecord {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RequestSignatureSnapshotRecord {
    pub request_path: PathBuf,
    pub method: String,
    pub url_pattern: String,
    #[serde(default)]
    pub headers: Vec<KeyValueEntryRecord>,
    #[serde(default)]
    pub query_params: Vec<KeyValueEntryRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_content: Option<String>,
    #[serde(default)]
    pub form_fields: Vec<KeyValueEntryRecord>,
    pub auth_type: String,
    #[serde(default)]
    pub auth_detail: String,
    pub captured_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub query_param_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub header_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body_field_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractSnapshotRecord {
    pub contract_id: Ulid,
    pub entries: Vec<RequestSignatureSnapshotRecord>,
}

impl From<&KeyValueEntry> for KeyValueEntryRecord {
    fn from(k: &KeyValueEntry) -> Self {
        Self { key: k.key.clone(), value: k.value.clone() }
    }
}

impl From<KeyValueEntryRecord> for KeyValueEntry {
    fn from(r: KeyValueEntryRecord) -> Self {
        Self { key: r.key, value: r.value }
    }
}

impl From<&RequestSignatureSnapshot> for RequestSignatureSnapshotRecord {
    fn from(s: &RequestSignatureSnapshot) -> Self {
        Self {
            request_path: s.request_path.clone(),
            method: s.method.clone(),
            url_pattern: s.url_pattern.clone(),
            headers: s.headers.iter().map(Into::into).collect(),
            query_params: s.query_params.iter().map(Into::into).collect(),
            body_content: s.body_content.clone(),
            form_fields: s.form_fields.iter().map(Into::into).collect(),
            auth_type: s.auth_type.clone(),
            auth_detail: s.auth_detail.clone(),
            captured_at: s.captured_at,
            query_param_keys: s.query_param_keys.clone(),
            header_keys: s.header_keys.clone(),
            body_field_keys: s.body_field_keys.clone(),
        }
    }
}

impl From<RequestSignatureSnapshotRecord> for RequestSignatureSnapshot {
    fn from(r: RequestSignatureSnapshotRecord) -> Self {
        Self {
            request_path: r.request_path,
            method: r.method,
            url_pattern: r.url_pattern,
            headers: r.headers.into_iter().map(Into::into).collect(),
            query_params: r.query_params.into_iter().map(Into::into).collect(),
            body_content: r.body_content,
            form_fields: r.form_fields.into_iter().map(Into::into).collect(),
            auth_type: r.auth_type,
            auth_detail: r.auth_detail,
            captured_at: r.captured_at,
            query_param_keys: r.query_param_keys,
            header_keys: r.header_keys,
            body_field_keys: r.body_field_keys,
        }
    }
}

impl From<&ContractSnapshot> for ContractSnapshotRecord {
    fn from(c: &ContractSnapshot) -> Self {
        Self {
            contract_id: c.contract_id,
            entries: c.entries.iter().map(Into::into).collect(),
        }
    }
}

impl From<ContractSnapshotRecord> for ContractSnapshot {
    fn from(r: ContractSnapshotRecord) -> Self {
        Self {
            contract_id: r.contract_id,
            entries: r.entries.into_iter().map(Into::into).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_format_snapshot_deserialises_via_record() {
        let yaml = r#"contractId: 01KQRNJTCG9AA2FR6AV9N1H3QA
entries:
- requestPath: get-users.yml
  method: GET
  urlPattern: https://api.example.com/users
  queryParamKeys: []
  headerKeys:
  - Authorization
  bodyFieldKeys: []
  authType: none
  capturedAt: 2026-05-04T04:57:42.033432603Z
"#;
        let r: ContractSnapshotRecord = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(r.entries.len(), 1);
        let e = &r.entries[0];
        assert!(e.headers.is_empty());
        assert!(e.query_params.is_empty());
        assert!(e.auth_detail.is_empty());
        assert_eq!(e.header_keys, vec!["Authorization".to_string()]);
    }

    #[test]
    fn snapshot_record_roundtrip_camel_case_yaml() {
        let r = RequestSignatureSnapshotRecord {
            request_path: PathBuf::from("a.yml"),
            method: "GET".into(),
            url_pattern: "/x".into(),
            headers: vec![KeyValueEntryRecord { key: "K".into(), value: "V".into() }],
            query_params: vec![],
            body_content: None,
            form_fields: vec![],
            auth_type: "none".into(),
            auth_detail: String::new(),
            captured_at: "2026-05-08T00:00:00Z".parse().unwrap(),
            query_param_keys: vec![],
            header_keys: vec![],
            body_field_keys: vec![],
        };
        let yaml = serde_yaml::to_string(&r).unwrap();
        assert!(yaml.contains("requestPath:"));
        assert!(yaml.contains("urlPattern:"));
        assert!(yaml.contains("authType:"));
        let back: RequestSignatureSnapshotRecord = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(r, back);
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p rocket-infra contract_records::snapshot::tests
```

Expected: 2 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-infra/src/contract_records/snapshot.rs
git commit -m "refactor(contract): add snapshot records with legacy v0.6.x serde defaults"
```

---

### Task 10: Switch `FsContractRepo` to read/write through Records

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/src/fs_contract_repo.rs`

- [ ] **Step 1: Replace the file body**

Open `crates/rocket-infra/src/fs_contract_repo.rs` and replace the existing `impl ContractRepository for FsContractRepo { ... }` block. The full file becomes:

```rust
use rocket_collection::contract::{
    changelog::ContractChangelog,
    repository::{ContractError, ContractRepository, ContractResult},
    snapshot::ContractSnapshot,
    types::Contract,
};
use std::path::Path;
use ulid::Ulid;

use crate::atomic_write;
use crate::contract_records::{
    changelog::ContractChangelogRecord,
    snapshot::ContractSnapshotRecord,
    types::ContractRecord,
};

pub struct FsContractRepo;

impl FsContractRepo {
    fn contracts_dir(collection_root: &Path) -> std::path::PathBuf {
        collection_root.join(".rocket").join("contracts")
    }

    fn contract_path(collection_root: &Path, id: Ulid) -> std::path::PathBuf {
        Self::contracts_dir(collection_root).join(format!("{}.yml", id))
    }

    fn snapshot_path(collection_root: &Path, id: Ulid) -> std::path::PathBuf {
        Self::contracts_dir(collection_root).join(format!("{}-snapshot.yml", id))
    }

    fn changelog_path(collection_root: &Path, id: Ulid) -> std::path::PathBuf {
        Self::contracts_dir(collection_root).join(format!("{}-changelog.yml", id))
    }

    pub fn attachments_dir(collection_root: &Path, id: Ulid) -> std::path::PathBuf {
        Self::contracts_dir(collection_root).join("attachments").join(id.to_string())
    }

    fn ensure_dir(collection_root: &Path) -> ContractResult<()> {
        std::fs::create_dir_all(Self::contracts_dir(collection_root))?;
        Ok(())
    }
}

impl ContractRepository for FsContractRepo {
    fn save_contract(&self, collection_root: &Path, contract: &Contract) -> ContractResult<()> {
        Self::ensure_dir(collection_root)?;
        let path = Self::contract_path(collection_root, contract.id);
        let record: ContractRecord = contract.into();
        let yaml = serde_yaml::to_string(&record)?;
        atomic_write(&path, yaml.as_bytes())?;
        Ok(())
    }

    fn load_contract(&self, collection_root: &Path, id: Ulid) -> ContractResult<Contract> {
        let path = Self::contract_path(collection_root, id);
        let yaml = std::fs::read_to_string(&path).map_err(|_| ContractError::NotFound(id))?;
        let record: ContractRecord = serde_yaml::from_str(&yaml)?;
        Ok(record.into())
    }

    fn list_contracts(&self, collection_root: &Path) -> ContractResult<Vec<Contract>> {
        let dir = Self::contracts_dir(collection_root);
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut contracts = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name.ends_with(".yml")
                && !name.contains("-snapshot")
                && !name.contains("-changelog")
            {
                let yaml = std::fs::read_to_string(&path)?;
                match serde_yaml::from_str::<ContractRecord>(&yaml) {
                    Ok(r) => contracts.push(r.into()),
                    Err(e) => tracing::warn!(
                        path = %path.display(), error = %e,
                        "skipping malformed contract YAML"
                    ),
                }
            }
        }
        Ok(contracts)
    }

    fn delete_contract(&self, collection_root: &Path, id: Ulid) -> ContractResult<()> {
        let _ = std::fs::remove_file(Self::contract_path(collection_root, id));
        let _ = std::fs::remove_file(Self::snapshot_path(collection_root, id));
        let _ = std::fs::remove_file(Self::changelog_path(collection_root, id));
        let attachments = Self::attachments_dir(collection_root, id);
        if attachments.exists() {
            std::fs::remove_dir_all(attachments)?;
        }
        Ok(())
    }

    fn save_snapshot(&self, collection_root: &Path, snapshot: &ContractSnapshot) -> ContractResult<()> {
        Self::ensure_dir(collection_root)?;
        let path = Self::snapshot_path(collection_root, snapshot.contract_id);
        let record: ContractSnapshotRecord = snapshot.into();
        let yaml = serde_yaml::to_string(&record)?;
        atomic_write(&path, yaml.as_bytes())?;
        Ok(())
    }

    fn load_snapshot(&self, collection_root: &Path, contract_id: Ulid) -> ContractResult<ContractSnapshot> {
        let path = Self::snapshot_path(collection_root, contract_id);
        if !path.exists() {
            return Ok(ContractSnapshot::new(contract_id));
        }
        let yaml = std::fs::read_to_string(path)?;
        let record: ContractSnapshotRecord = serde_yaml::from_str(&yaml)?;
        Ok(record.into())
    }

    fn append_changelog(&self, collection_root: &Path, incoming: &ContractChangelog) -> ContractResult<()> {
        Self::ensure_dir(collection_root)?;
        let path = Self::changelog_path(collection_root, incoming.contract_id);
        let mut existing: ContractChangelog = if path.exists() {
            let yaml = std::fs::read_to_string(&path)?;
            let record: ContractChangelogRecord = serde_yaml::from_str(&yaml)?;
            record.into()
        } else {
            ContractChangelog::new(incoming.contract_id)
        };
        existing.append(incoming.entries.clone());
        let record: ContractChangelogRecord = (&existing).into();
        let yaml = serde_yaml::to_string(&record)?;
        atomic_write(&path, yaml.as_bytes())?;
        Ok(())
    }

    fn load_changelog(&self, collection_root: &Path, contract_id: Ulid) -> ContractResult<ContractChangelog> {
        let path = Self::changelog_path(collection_root, contract_id);
        if !path.exists() {
            return Ok(ContractChangelog::new(contract_id));
        }
        let yaml = std::fs::read_to_string(path)?;
        let record: ContractChangelogRecord = serde_yaml::from_str(&yaml)?;
        Ok(record.into())
    }
}
```

- [ ] **Step 2: Run all the tests that touch this layer**

```bash
cargo test -p rocket-infra
cargo test -p rocket-app contract_service
```

Expected: all pass. The `rocket-collection` domain tests still work because domain types still have serde at this point (they'll be stripped in Task 14).

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-infra/src/fs_contract_repo.rs
git commit -m "refactor(contract): switch FsContractRepo to convert through Record types"
```

---

### Task 11: Scaffold `contract_dtos` module in src-tauri

**Files:**
- Create: `src-tauri/src/commands/contract_dtos/mod.rs`
- Create: `src-tauri/src/commands/contract_dtos/types.rs`
- Create: `src-tauri/src/commands/contract_dtos/changelog.rs`
- Create: `src-tauri/src/commands/contract_dtos/snapshot.rs`
- Create: `src-tauri/src/commands/contract_dtos/summary.rs`
- Modify: `src-tauri/src/commands/mod.rs` (add `pub mod contract_dtos;`)

- [ ] **Step 1: Inspect the existing module layout**

```bash
sed -n '1,40p' src-tauri/src/commands/mod.rs
```

Note where other `pub mod` lines live so the new declaration matches the existing style.

- [ ] **Step 2: Create empty submodule files with one-line doc comments**

`src-tauri/src/commands/contract_dtos/mod.rs`:
```rust
//! IPC DTOs for contract commands.
//!
//! DTOs sit at the IPC adapter layer. Domain types in `rocket-collection` and
//! `rocket-app` carry no serde — all wire-format concerns live here. Each DTO
//! has `From` impls bridging it to its domain counterpart.

pub mod changelog;
pub mod snapshot;
pub mod summary;
pub mod types;
```

`src-tauri/src/commands/contract_dtos/types.rs`:
```rust
//! IPC DTOs for `Contract` and its sub-types.
```

`src-tauri/src/commands/contract_dtos/changelog.rs`:
```rust
//! IPC DTOs for `ChangelogEntry` and `ContractChangelog`.
```

`src-tauri/src/commands/contract_dtos/snapshot.rs`:
```rust
//! IPC DTOs for `KeyValueEntry` and `RequestSignatureSnapshot`.
```

`src-tauri/src/commands/contract_dtos/summary.rs`:
```rust
//! IPC DTOs for service-layer summary types.
```

- [ ] **Step 3: Register the module**

In `src-tauri/src/commands/mod.rs`, add the line `pub mod contract_dtos;` adjacent to the other `pub mod` declarations (alphabetical insertion).

- [ ] **Step 4: Verify**

```bash
cargo check -p rocket_lib
```

Expected: success.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/contract_dtos/ src-tauri/src/commands/mod.rs
git commit -m "refactor(contract): scaffold contract_dtos module in src-tauri"
```

---

### Task 12: `ContractPartyDto`, `ContractPolicyDto`, `ContractScopeDto`, `ContractStatusDto`, `ContractEnforcementModeDto` + From impls

**Files:**
- Modify: `src-tauri/src/commands/contract_dtos/types.rs`

- [ ] **Step 1: Write the DTOs**

Append to `src-tauri/src/commands/contract_dtos/types.rs`:

```rust
use rocket_collection::contract::types::{
    BreakingChangePolicy, Contract, ContractEnforcementMode, ContractParty,
    ContractPolicy, ContractScope, ContractStatus, PartyKind,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ulid::Ulid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractPartyDto {
    pub id: String,
    pub name: String,
    pub kind: PartyKindDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_seed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PartyKindDto {
    #[default]
    Team,
    Company,
    Service,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractPolicyDto {
    #[serde(default)]
    pub breaking_change_policy: BreakingChangePolicyDto,
    #[serde(default = "default_notice_days_dto")]
    pub notice_days: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime_sla: Option<f32>,
}

impl Default for ContractPolicyDto {
    fn default() -> Self {
        Self {
            breaking_change_policy: BreakingChangePolicyDto::Lenient,
            notice_days: 30,
            uptime_sla: None,
        }
    }
}

fn default_notice_days_dto() -> u32 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BreakingChangePolicyDto {
    Strict,
    #[default]
    Lenient,
    AdditiveOk,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContractScopeDto {
    Collection,
    Folder { rel_path: PathBuf },
    Request { rel_path: PathBuf },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContractStatusDto {
    Draft,
    #[default]
    Active,
    Drift,
    Breach,
    InReview,
    Paused,
    ExpiringIn30Days,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContractEnforcementModeDto {
    #[default]
    Informational,
    Warn,
    Block,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractDto {
    pub id: Ulid,
    pub title: String,
    pub provider: ContractPartyDto,
    pub consumers: Vec<ContractPartyDto>,
    pub project: String,
    pub version: String,
    pub status: ContractStatusDto,
    pub effective_date: chrono::NaiveDate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiry_date: Option<chrono::NaiveDate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub document_paths: Vec<PathBuf>,
    pub enforcement_mode: ContractEnforcementModeDto,
    pub scope: ContractScopeDto,
    pub policy: ContractPolicyDto,
    pub drift_count: u32,
    pub breach_count: u32,
    pub endpoint_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

// From impls ---------------------------------------------------------------

impl From<&PartyKind> for PartyKindDto {
    fn from(k: &PartyKind) -> Self {
        match k {
            PartyKind::Team => Self::Team,
            PartyKind::Company => Self::Company,
            PartyKind::Service => Self::Service,
        }
    }
}
impl From<PartyKindDto> for PartyKind {
    fn from(d: PartyKindDto) -> Self {
        match d {
            PartyKindDto::Team => Self::Team,
            PartyKindDto::Company => Self::Company,
            PartyKindDto::Service => Self::Service,
        }
    }
}

impl From<&ContractParty> for ContractPartyDto {
    fn from(p: &ContractParty) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            kind: (&p.kind).into(),
            avatar_seed: p.avatar_seed.clone(),
            avatar_color: p.avatar_color.clone(),
        }
    }
}
impl From<ContractPartyDto> for ContractParty {
    fn from(d: ContractPartyDto) -> Self {
        Self {
            id: d.id,
            name: d.name,
            kind: d.kind.into(),
            avatar_seed: d.avatar_seed,
            avatar_color: d.avatar_color,
        }
    }
}

impl From<&BreakingChangePolicy> for BreakingChangePolicyDto {
    fn from(p: &BreakingChangePolicy) -> Self {
        match p {
            BreakingChangePolicy::Strict => Self::Strict,
            BreakingChangePolicy::Lenient => Self::Lenient,
            BreakingChangePolicy::AdditiveOk => Self::AdditiveOk,
        }
    }
}
impl From<BreakingChangePolicyDto> for BreakingChangePolicy {
    fn from(d: BreakingChangePolicyDto) -> Self {
        match d {
            BreakingChangePolicyDto::Strict => Self::Strict,
            BreakingChangePolicyDto::Lenient => Self::Lenient,
            BreakingChangePolicyDto::AdditiveOk => Self::AdditiveOk,
        }
    }
}

impl From<&ContractPolicy> for ContractPolicyDto {
    fn from(p: &ContractPolicy) -> Self {
        Self {
            breaking_change_policy: (&p.breaking_change_policy).into(),
            notice_days: p.notice_days,
            uptime_sla: p.uptime_sla,
        }
    }
}
impl From<ContractPolicyDto> for ContractPolicy {
    fn from(d: ContractPolicyDto) -> Self {
        Self {
            breaking_change_policy: d.breaking_change_policy.into(),
            notice_days: d.notice_days,
            uptime_sla: d.uptime_sla,
        }
    }
}

impl From<&ContractScope> for ContractScopeDto {
    fn from(s: &ContractScope) -> Self {
        match s {
            ContractScope::Collection => Self::Collection,
            ContractScope::Folder { rel_path } => Self::Folder { rel_path: rel_path.clone() },
            ContractScope::Request { rel_path } => Self::Request { rel_path: rel_path.clone() },
        }
    }
}
impl From<ContractScopeDto> for ContractScope {
    fn from(d: ContractScopeDto) -> Self {
        match d {
            ContractScopeDto::Collection => Self::Collection,
            ContractScopeDto::Folder { rel_path } => Self::Folder { rel_path },
            ContractScopeDto::Request { rel_path } => Self::Request { rel_path },
        }
    }
}

impl From<&ContractStatus> for ContractStatusDto {
    fn from(s: &ContractStatus) -> Self {
        match s {
            ContractStatus::Draft => Self::Draft,
            ContractStatus::Active => Self::Active,
            ContractStatus::Drift => Self::Drift,
            ContractStatus::Breach => Self::Breach,
            ContractStatus::InReview => Self::InReview,
            ContractStatus::Paused => Self::Paused,
            ContractStatus::ExpiringIn30Days => Self::ExpiringIn30Days,
            ContractStatus::Expired => Self::Expired,
        }
    }
}
impl From<ContractStatusDto> for ContractStatus {
    fn from(d: ContractStatusDto) -> Self {
        match d {
            ContractStatusDto::Draft => Self::Draft,
            ContractStatusDto::Active => Self::Active,
            ContractStatusDto::Drift => Self::Drift,
            ContractStatusDto::Breach => Self::Breach,
            ContractStatusDto::InReview => Self::InReview,
            ContractStatusDto::Paused => Self::Paused,
            ContractStatusDto::ExpiringIn30Days => Self::ExpiringIn30Days,
            ContractStatusDto::Expired => Self::Expired,
        }
    }
}

impl From<&ContractEnforcementMode> for ContractEnforcementModeDto {
    fn from(e: &ContractEnforcementMode) -> Self {
        match e {
            ContractEnforcementMode::Informational => Self::Informational,
            ContractEnforcementMode::Warn => Self::Warn,
            ContractEnforcementMode::Block => Self::Block,
        }
    }
}
impl From<ContractEnforcementModeDto> for ContractEnforcementMode {
    fn from(d: ContractEnforcementModeDto) -> Self {
        match d {
            ContractEnforcementModeDto::Informational => Self::Informational,
            ContractEnforcementModeDto::Warn => Self::Warn,
            ContractEnforcementModeDto::Block => Self::Block,
        }
    }
}

impl From<&Contract> for ContractDto {
    fn from(c: &Contract) -> Self {
        Self {
            id: c.id,
            title: c.title.clone(),
            provider: (&c.provider).into(),
            consumers: c.consumers.iter().map(Into::into).collect(),
            project: c.project.clone(),
            version: c.version.clone(),
            status: (&c.status).into(),
            effective_date: c.effective_date,
            expiry_date: c.expiry_date,
            document_paths: c.document_paths.clone(),
            enforcement_mode: (&c.enforcement_mode).into(),
            scope: (&c.scope).into(),
            policy: (&c.policy).into(),
            drift_count: c.drift_count,
            breach_count: c.breach_count,
            endpoint_count: c.endpoint_count,
            created_by: c.created_by.clone(),
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

impl From<ContractDto> for Contract {
    fn from(d: ContractDto) -> Self {
        Self {
            id: d.id,
            title: d.title,
            provider: d.provider.into(),
            consumers: d.consumers.into_iter().map(Into::into).collect(),
            project: d.project,
            version: d.version,
            status: d.status.into(),
            effective_date: d.effective_date,
            expiry_date: d.expiry_date,
            document_paths: d.document_paths,
            enforcement_mode: d.enforcement_mode.into(),
            scope: d.scope.into(),
            policy: d.policy.into(),
            drift_count: d.drift_count,
            breach_count: d.breach_count,
            endpoint_count: d.endpoint_count,
            created_by: d.created_by,
            created_at: d.created_at,
            updated_at: d.updated_at,
        }
    }
}
```

- [ ] **Step 2: Add roundtrip tests**

Append:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn party_dto_json_roundtrip_camel_case() {
        let d = ContractPartyDto {
            id: "billing-team".into(),
            name: "Billing Team".into(),
            kind: PartyKindDto::Team,
            avatar_seed: Some("seed".into()),
            avatar_color: None,
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"avatarSeed\":"), "got {json}");
        let back: ContractPartyDto = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn scope_dto_json_keeps_rel_path_snake_case() {
        let d = ContractScopeDto::Folder { rel_path: PathBuf::from("a/b.yml") };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"rel_path\":"), "rel_path must stay snake_case in JSON: {json}");
    }

    #[test]
    fn contract_dto_domain_roundtrip() {
        let domain = Contract {
            id: Ulid::new(),
            title: "T".into(),
            provider: ContractParty::from_name("Prov"),
            consumers: vec![ContractParty::from_name("Cons")],
            project: "P".into(),
            version: "1.0.0".into(),
            status: ContractStatus::Active,
            effective_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            expiry_date: None,
            document_paths: vec![],
            enforcement_mode: ContractEnforcementMode::Informational,
            scope: ContractScope::Collection,
            policy: ContractPolicy::default(),
            drift_count: 0,
            breach_count: 0,
            endpoint_count: 0,
            created_by: None,
            created_at: None,
            updated_at: None,
        };
        let dto: ContractDto = (&domain).into();
        let back: Contract = dto.into();
        assert_eq!(domain, back);
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p rocket_lib commands::contract_dtos::types::tests
```

Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/contract_dtos/types.rs
git commit -m "refactor(contract): add ContractDto and sub-type DTOs with From impls"
```

---

### Task 13: Changelog DTOs (`ChangeTypeDto`, `ChangelogEntryDto`, `ContractChangelogDto`)

**Files:**
- Modify: `src-tauri/src/commands/contract_dtos/changelog.rs`

- [ ] **Step 1: Write the module**

Replace placeholder with:

```rust
//! IPC DTOs for `ChangelogEntry` and `ContractChangelog`.

use chrono::{DateTime, Utc};
use rocket_collection::contract::changelog::{ChangeType, ChangelogEntry, ContractChangelog};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ulid::Ulid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeTypeDto {
    Changed,
    Added,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChangelogEntryDto {
    pub timestamp: DateTime<Utc>,
    pub request_path: PathBuf,
    pub field: String,
    pub change_type: ChangeTypeDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_value: Option<String>,
    #[serde(default)]
    pub is_breaking: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractChangelogDto {
    pub contract_id: Ulid,
    pub entries: Vec<ChangelogEntryDto>,
}

impl From<&ChangeType> for ChangeTypeDto {
    fn from(c: &ChangeType) -> Self {
        match c {
            ChangeType::Changed => Self::Changed,
            ChangeType::Added => Self::Added,
            ChangeType::Removed => Self::Removed,
        }
    }
}
impl From<ChangeTypeDto> for ChangeType {
    fn from(d: ChangeTypeDto) -> Self {
        match d {
            ChangeTypeDto::Changed => Self::Changed,
            ChangeTypeDto::Added => Self::Added,
            ChangeTypeDto::Removed => Self::Removed,
        }
    }
}

impl From<&ChangelogEntry> for ChangelogEntryDto {
    fn from(e: &ChangelogEntry) -> Self {
        Self {
            timestamp: e.timestamp,
            request_path: e.request_path.clone(),
            field: e.field.clone(),
            change_type: (&e.change_type).into(),
            old_value: e.old_value.clone(),
            new_value: e.new_value.clone(),
            is_breaking: e.is_breaking,
        }
    }
}
impl From<ChangelogEntryDto> for ChangelogEntry {
    fn from(d: ChangelogEntryDto) -> Self {
        Self {
            timestamp: d.timestamp,
            request_path: d.request_path,
            field: d.field,
            change_type: d.change_type.into(),
            old_value: d.old_value,
            new_value: d.new_value,
            is_breaking: d.is_breaking,
        }
    }
}

impl From<&ContractChangelog> for ContractChangelogDto {
    fn from(c: &ContractChangelog) -> Self {
        Self {
            contract_id: c.contract_id,
            entries: c.entries.iter().map(Into::into).collect(),
        }
    }
}
impl From<ContractChangelogDto> for ContractChangelog {
    fn from(d: ContractChangelogDto) -> Self {
        Self {
            contract_id: d.contract_id,
            entries: d.entries.into_iter().map(Into::into).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changelog_entry_dto_json_camel_case() {
        let d = ChangelogEntryDto {
            timestamp: "2026-05-07T10:00:00Z".parse().unwrap(),
            request_path: PathBuf::from("a.yml"),
            field: "method".into(),
            change_type: ChangeTypeDto::Changed,
            old_value: Some("GET".into()),
            new_value: Some("POST".into()),
            is_breaking: false,
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"requestPath\":"));
        assert!(json.contains("\"isBreaking\":"));
        let back: ChangelogEntryDto = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn changelog_dto_domain_roundtrip() {
        let domain = ContractChangelog::new(Ulid::new());
        let dto: ContractChangelogDto = (&domain).into();
        let back: ContractChangelog = dto.into();
        assert_eq!(domain.contract_id, back.contract_id);
    }
}
```

- [ ] **Step 2: Run**

```bash
cargo test -p rocket_lib commands::contract_dtos::changelog::tests
```

Expected: 2 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/contract_dtos/changelog.rs
git commit -m "refactor(contract): add ContractChangelogDto and entry DTO with From impls"
```

---

### Task 14: Snapshot DTOs (`KeyValueEntryDto`, `RequestSignatureSnapshotDto`)

**Files:**
- Modify: `src-tauri/src/commands/contract_dtos/snapshot.rs`

- [ ] **Step 1: Write the module**

Replace placeholder with:

```rust
//! IPC DTOs for `KeyValueEntry` and `RequestSignatureSnapshot`.

use chrono::{DateTime, Utc};
use rocket_collection::contract::snapshot::{KeyValueEntry, RequestSignatureSnapshot};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyValueEntryDto {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RequestSignatureSnapshotDto {
    pub request_path: PathBuf,
    pub method: String,
    pub url_pattern: String,
    #[serde(default)]
    pub headers: Vec<KeyValueEntryDto>,
    #[serde(default)]
    pub query_params: Vec<KeyValueEntryDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_content: Option<String>,
    #[serde(default)]
    pub form_fields: Vec<KeyValueEntryDto>,
    pub auth_type: String,
    #[serde(default)]
    pub auth_detail: String,
    pub captured_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub query_param_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub header_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body_field_keys: Vec<String>,
}

impl From<&KeyValueEntry> for KeyValueEntryDto {
    fn from(k: &KeyValueEntry) -> Self {
        Self { key: k.key.clone(), value: k.value.clone() }
    }
}
impl From<KeyValueEntryDto> for KeyValueEntry {
    fn from(d: KeyValueEntryDto) -> Self {
        Self { key: d.key, value: d.value }
    }
}

impl From<&RequestSignatureSnapshot> for RequestSignatureSnapshotDto {
    fn from(s: &RequestSignatureSnapshot) -> Self {
        Self {
            request_path: s.request_path.clone(),
            method: s.method.clone(),
            url_pattern: s.url_pattern.clone(),
            headers: s.headers.iter().map(Into::into).collect(),
            query_params: s.query_params.iter().map(Into::into).collect(),
            body_content: s.body_content.clone(),
            form_fields: s.form_fields.iter().map(Into::into).collect(),
            auth_type: s.auth_type.clone(),
            auth_detail: s.auth_detail.clone(),
            captured_at: s.captured_at,
            query_param_keys: s.query_param_keys.clone(),
            header_keys: s.header_keys.clone(),
            body_field_keys: s.body_field_keys.clone(),
        }
    }
}
impl From<RequestSignatureSnapshotDto> for RequestSignatureSnapshot {
    fn from(d: RequestSignatureSnapshotDto) -> Self {
        Self {
            request_path: d.request_path,
            method: d.method,
            url_pattern: d.url_pattern,
            headers: d.headers.into_iter().map(Into::into).collect(),
            query_params: d.query_params.into_iter().map(Into::into).collect(),
            body_content: d.body_content,
            form_fields: d.form_fields.into_iter().map(Into::into).collect(),
            auth_type: d.auth_type,
            auth_detail: d.auth_detail,
            captured_at: d.captured_at,
            query_param_keys: d.query_param_keys,
            header_keys: d.header_keys,
            body_field_keys: d.body_field_keys,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_dto_json_uses_camel_case_field_names() {
        let d = RequestSignatureSnapshotDto {
            request_path: PathBuf::from("a.yml"),
            method: "GET".into(),
            url_pattern: "/x".into(),
            headers: vec![],
            query_params: vec![],
            body_content: None,
            form_fields: vec![],
            auth_type: "none".into(),
            auth_detail: String::new(),
            captured_at: "2026-05-08T00:00:00Z".parse().unwrap(),
            query_param_keys: vec![],
            header_keys: vec![],
            body_field_keys: vec![],
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"requestPath\":"));
        assert!(json.contains("\"urlPattern\":"));
        assert!(json.contains("\"authType\":"));
        let back: RequestSignatureSnapshotDto = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
```

- [ ] **Step 2: Run**

```bash
cargo test -p rocket_lib commands::contract_dtos::snapshot::tests
```

Expected: 1 test passes.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/contract_dtos/snapshot.rs
git commit -m "refactor(contract): add RequestSignatureSnapshotDto and KeyValueEntryDto"
```

---

### Task 15: Summary DTOs (`ContractSummaryDto`, `ContractDriftSummaryDto`)

**Files:**
- Modify: `src-tauri/src/commands/contract_dtos/summary.rs`

- [ ] **Step 1: Write the module**

Replace placeholder with:

```rust
//! IPC DTOs for service-layer summary types.

use rocket_app::{ContractDriftSummary, ContractSummary};
use serde::{Deserialize, Serialize};

use super::types::ContractStatusDto;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractDriftSummaryDto {
    pub contract_id: String,
    pub status: ContractStatusDto,
    pub drift_count: u32,
    pub breach_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractSummaryDto {
    pub id: String,
    pub title: String,
    pub status: ContractStatusDto,
    pub drift_count: u32,
    pub breach_count: u32,
    pub endpoint_count: u32,
}

impl From<ContractDriftSummary> for ContractDriftSummaryDto {
    fn from(s: ContractDriftSummary) -> Self {
        Self {
            contract_id: s.contract_id,
            status: (&s.status).into(),
            drift_count: s.drift_count,
            breach_count: s.breach_count,
        }
    }
}

impl From<ContractSummary> for ContractSummaryDto {
    fn from(s: ContractSummary) -> Self {
        Self {
            id: s.id,
            title: s.title,
            status: (&s.status).into(),
            drift_count: s.drift_count,
            breach_count: s.breach_count,
            endpoint_count: s.endpoint_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::contract::types::ContractStatus;

    #[test]
    fn drift_summary_dto_camel_case_json() {
        let s = ContractDriftSummary {
            contract_id: "01ABC".into(),
            status: ContractStatus::Drift,
            drift_count: 3,
            breach_count: 1,
        };
        let dto: ContractDriftSummaryDto = s.into();
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("\"contractId\":"));
        assert!(json.contains("\"driftCount\":"));
    }

    #[test]
    fn summary_dto_camel_case_json() {
        let s = ContractSummary {
            id: "x".into(),
            title: "T".into(),
            status: ContractStatus::Active,
            drift_count: 0,
            breach_count: 0,
            endpoint_count: 5,
        };
        let dto: ContractSummaryDto = s.into();
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("\"endpointCount\":"));
    }
}
```

- [ ] **Step 2: Run**

```bash
cargo test -p rocket_lib commands::contract_dtos::summary::tests
```

Expected: 2 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/contract_dtos/summary.rs
git commit -m "refactor(contract): add ContractSummaryDto and ContractDriftSummaryDto"
```

---

### Task 16: Switch all 17 `#[tauri::command]` signatures to use Dtos

**Files:**
- Modify: `src-tauri/src/commands/contract.rs`

This task changes signatures so each command accepts/returns DTOs and converts at the boundary. Domain types still exist; the service layer is unchanged.

- [ ] **Step 1: Read the current command list**

```bash
grep -n "^pub fn\|^#\[tauri::command\]" src-tauri/src/commands/contract.rs
```

Expected output: 17 commands at the line numbers listed in this plan's introduction. Confirm the count is still 17.

- [ ] **Step 2: Update each command — pattern**

Apply this pattern to every command that mentions `Contract`, `ContractParty`, `ContractPolicy`, `ContractScope`, `ContractStatus`, `ContractEnforcementMode`, `ContractChangelog`, `ChangelogEntry`, `RequestSignatureSnapshot`, `ContractSummary`, or `ContractDriftSummary` in its signature:

- Replace each domain type in the signature with its DTO equivalent (`Contract` → `ContractDto`, `ContractParty` → `ContractPartyDto`, etc.).
- Inside the body, convert input DTOs to domain (`let domain: Contract = input.into();`) before calling the service, and convert the service result to DTO before returning (`Ok(result.into())`).

**Example — `attach_contract` rewrite:**

```rust
use crate::commands::contract_dtos::{
    snapshot::RequestSignatureSnapshotDto,
    types::{ContractDto, ContractPartyDto, ContractPolicyDto, ContractScopeDto},
};

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachContractInput {
    pub title: String,
    pub provider: ContractPartyDto,
    pub consumers: Vec<ContractPartyDto>,
    pub version: String,
    pub effective_date: String,
    pub expiry_date: Option<String>,
    pub document_paths: Vec<PathBuf>,
    pub scope: ContractScopeDto,
    pub policy: ContractPolicyDto,
    pub initial_snapshots: Vec<RequestSignatureSnapshotDto>,
    pub publish_immediately: bool,
}

#[tauri::command]
pub fn attach_contract(
    collection_root: String,
    input: AttachContractInput,
    svc: State<'_, ContractService>,
) -> Result<ContractDto, String> {
    use chrono::NaiveDate;

    let root = PathBuf::from(&collection_root);
    let effective_date = NaiveDate::parse_from_str(&input.effective_date, "%Y-%m-%d")
        .map_err(|e| format!("invalid effectiveDate: {e}"))?;
    let expiry_date = input.expiry_date.as_deref()
        .map(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d"))
        .transpose()
        .map_err(|e| format!("invalid expiryDate: {e}"))?;

    let status = if input.publish_immediately {
        ContractStatus::Active
    } else {
        ContractStatus::Draft
    };

    let contract = Contract {
        id: Ulid::new(),
        title: input.title,
        provider: input.provider.into(),
        consumers: input.consumers.into_iter().map(Into::into).collect(),
        project: String::new(),
        version: input.version,
        status,
        effective_date,
        expiry_date,
        document_paths: vec![],
        enforcement_mode: ContractEnforcementMode::Informational,
        scope: input.scope.into(),
        policy: input.policy.into(),
        drift_count: 0,
        breach_count: 0,
        endpoint_count: 0,
        created_by: None,
        created_at: None,
        updated_at: None,
    };

    let snapshots: Vec<RequestSignatureSnapshot> = if input.publish_immediately {
        input.initial_snapshots.into_iter().map(Into::into).collect()
    } else {
        vec![]
    };

    svc.attach_contract(&root, contract, snapshots, input.document_paths)
        .map(|c| (&c).into())
        .map_err(|e| e.to_string())
}
```

Apply the equivalent transformation to:

- `update_contract` — `UpdateContractInput` becomes DTO-typed; returns `ContractDto`.
- `list_contracts` — returns `Vec<ContractDto>`.
- `get_contract` — returns `ContractDto`.
- `delete_contract` — no DTO change (`Result<(), String>`).
- `get_contract_changelog` — returns `ContractChangelogDto`.
- `publish_contract`, `pause_contract`, `resume_contract`, `renew_contract`, `send_for_review`, `approve_contract`, `reject_contract`, `duplicate_contract` — return `ContractDto`.
- `recompute_drift` — returns `Vec<ContractDriftSummaryDto>`.
- `get_contract_summary` — returns `ContractSummaryDto`.
- `export_contract_openapi` — no DTO change (returns string YAML).

For each rewritten command, the body pattern is the same: convert input DTOs to domain at the top, call the service, convert the result with `.into()` or `.map(|c| (&c).into())` for `Result<Contract, _>` returns.

- [ ] **Step 3: Verify the file compiles**

```bash
cargo check -p rocket_lib
```

Expected: success. If errors mention `Contract` field mismatches, the conversion via `into()` is missing somewhere — fix and re-run.

- [ ] **Step 4: Run all tauri command tests**

```bash
cargo test -p rocket_lib
```

Expected: pre-existing tests still pass; the new `contract_dtos` tests still pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/contract.rs
git commit -m "refactor(contract): switch all 17 Tauri commands to use DTO types"
```

---

### Task 17: Strip serde from `rocket-collection` contract domain types

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-collection/src/contract/types.rs`
- Modify: `crates/rocket-collection/src/contract/changelog.rs`
- Modify: `crates/rocket-collection/src/contract/snapshot.rs`

This is the danger-zone task. Compilation will fail at any non-adapter call site that still serializes domain types directly. Each failure is a localized fix.

- [ ] **Step 1: Update `types.rs`**

In `crates/rocket-collection/src/contract/types.rs`:

- Remove `use serde::{Deserialize, Serialize};` from line 2.
- For every type below, remove the `Serialize` and `Deserialize` from its `#[derive(...)]` and remove all `#[serde(...)]` attributes:
  - `ContractParty`
  - `PartyKind`
  - `ContractPolicy`
  - `BreakingChangePolicy`
  - `Contract`
  - `ContractStatus`
  - `ContractEnforcementMode`
  - `ContractScope`
- Delete the entire `impl<'de> serde::Deserialize<'de> for ContractParty { ... }` block.
- Delete the entire `impl<'de> serde::Deserialize<'de> for Contract { ... }` block.
- Delete the now-unused `default_version` and `default_notice_days` private functions (the constants are inlined into Record types now).
- Remove the `#[cfg(test)] mod tests { ... }` module entirely from this file. Its roundtrip and back-compat tests have been ported to `contract_records::types`. (Domain-only logic tests, if any existed, would already be in `state_machine`/`diff` modules — types.rs has only serde tests.)
- The `impl ContractParty { pub fn from_name(...) -> Self }` block stays.

**Final shape of each derive line should look like:**
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ContractParty { /* fields without #[serde(...)] */ }

#[derive(Debug, Clone, PartialEq, Default)]
pub enum PartyKind { /* variants without #[serde(...)] */ }
```

- [ ] **Step 2: Update `changelog.rs`**

In `crates/rocket-collection/src/contract/changelog.rs`:

- Remove `use serde::{Deserialize, Serialize};`.
- Strip `Serialize`, `Deserialize`, and all `#[serde(...)]` attributes from `ChangeType`, `ChangelogEntry`, and `ContractChangelog`.
- Add `#[derive(PartialEq)]` to `ChangelogEntry` and `ContractChangelog` (they didn't have it before because serde provided struct equality via roundtrip — domain code doesn't need it but tests might; keep it minimal and only add PartialEq if a test imports it). For now, add `PartialEq` to both — it's cheap and useful.
- Delete the `#[cfg(test)] mod tests` module.

**Final shape:**
```rust
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use ulid::Ulid;

#[derive(Debug, Clone, PartialEq)]
pub enum ChangeType { Changed, Added, Removed }

#[derive(Debug, Clone, PartialEq)]
pub struct ChangelogEntry { /* … */ }

#[derive(Debug, Clone, PartialEq)]
pub struct ContractChangelog { /* … */ }

impl ContractChangelog {
    pub fn new(contract_id: Ulid) -> Self { /* unchanged */ }
    pub fn append(&mut self, new_entries: Vec<ChangelogEntry>) -> usize { /* unchanged */ }
}
```

- [ ] **Step 3: Update `snapshot.rs`**

In `crates/rocket-collection/src/contract/snapshot.rs`:

- Remove `use serde::{Deserialize, Serialize};`.
- Strip `Serialize`, `Deserialize`, and all `#[serde(...)]` attributes from `KeyValueEntry`, `RequestSignatureSnapshot`, and `ContractSnapshot`.
- Delete the `#[cfg(test)] mod tests` module entirely. (Snapshot construction tests via `from_request` move to `rocket-app` if they aren't already there; the legacy YAML test moves to `contract_records::snapshot` — already done in Task 9.)
  - **Important:** scan the deleted tests for any unique behaviour test that isn't covered by Records. The test `from_request_*` family is genuine domain logic — those should be **kept** as `#[cfg(test)] mod from_request_tests` without changes (no serde involved). Cut only the YAML-roundtrip test (`old_format_snapshot_deserialises_without_error`).

**Action:** in `snapshot.rs`, delete only:
- the `old_format_snapshot_deserialises_without_error` test
- any test that calls `serde_yaml::from_str` or `serde_yaml::to_string` on snapshot types

Keep all `from_request_*` tests — they exercise pure domain logic on `Request`.

- [ ] **Step 4: Build the workspace and fix the cascade**

```bash
cargo check --workspace
```

Expected: errors at every site that still calls `serde_yaml::from_str::<Contract>` or similar. Walk through each error:

- If the call site is in `rocket-infra/src/fs_contract_repo.rs` — should already be using Records (Task 10 fixed this). If errors appear here, Task 10 was incomplete.
- If the call site is in `src-tauri/src/commands/contract.rs` — should already be using DTOs (Task 16). If errors appear, Task 16 was incomplete.
- If the call site is in test code — port the test to use Record/DTO, or delete it if duplicated.
- If the call site is something unexpected (logging, metrics, dev-only debug snippets) — replace `serde_yaml::to_string(&contract)` with `format!("{contract:?}")`, or convert to a Record/DTO first.

Common errors and fixes:

| Compiler error | Fix |
|---|---|
| `the trait Serialize is not implemented for Contract` | Convert via Record or DTO before serializing. |
| `the trait Deserialize<'_> is not implemented for Contract` | Deserialize as Record/DTO, then `.into()`. |
| `unused import: serde::...` in domain crate | Remove the import. |

- [ ] **Step 5: Run all tests across affected crates**

```bash
cargo test -p rocket-collection
cargo test -p rocket-infra
cargo test -p rocket-app
cargo test -p rocket_lib
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-collection/src/contract/
git commit -m "refactor(contract): strip serde from rocket-collection contract domain types"
```

---

### Task 18: Strip serde from `rocket-app` summary types

**Files:**
- Modify: `crates/rocket-app/src/contract_service.rs`

- [ ] **Step 1: Update**

In `crates/rocket-app/src/contract_service.rs` find the two structs (around lines 106–125):

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractDriftSummary { /* … */ }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractSummary { /* … */ }
```

Replace each with:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ContractDriftSummary { /* unchanged fields */ }

#[derive(Debug, Clone, PartialEq)]
pub struct ContractSummary { /* unchanged fields */ }
```

- [ ] **Step 2: Build and fix**

```bash
cargo check --workspace
```

If `src-tauri` complains, Task 15's `From` impls + Task 16's command rewrites should already cover it.

- [ ] **Step 3: Test**

```bash
cargo test -p rocket-app
cargo test -p rocket_lib
```

Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-app/src/contract_service.rs
git commit -m "refactor(contract): strip serde from ContractSummary and ContractDriftSummary"
```

---

### Task 19: Full verification matrix

**Files:** none modified — verification only.

- [ ] **Step 1: Rust full workspace**

```bash
cargo check --workspace
cargo test --workspace
```

Expected: success. Note any non-contract tests that fail — they're unrelated and shouldn't be touched here.

- [ ] **Step 2: Frontend type check + lint + tests**

```bash
yarn tsc --noEmit
yarn check
yarn test
```

Expected: all pass. The frontend types in `src/types/contracts.ts` were intentionally not changed; the on-wire JSON shape from DTOs matches what frontend already expects (camelCase + snake-case `rel_path`).

- [ ] **Step 3: End-to-end Playwright**

```bash
yarn playwright test e2e/contracts.spec.ts --reporter=line
```

Expected: `5 passed`.

- [ ] **Step 4: Sanity check — wire format unchanged**

```bash
grep -rn "rename_all = \"camelCase\"" crates/rocket-collection/src/contract/ crates/rocket-app/src/contract_service.rs
```

Expected: zero matches in `crates/rocket-collection/src/contract/`. The only `rename_all = "camelCase"` mentions in `contract_service.rs` should be inside conversion code if any (none expected).

- [ ] **Step 5: Confirm zero serde in domain**

```bash
grep -rn "serde::" crates/rocket-collection/src/contract/
grep -rn "use serde" crates/rocket-collection/src/contract/
```

Expected: zero matches.

- [ ] **Step 6: Final commit (if any cleanup happened)**

If verification surfaced anything to fix, commit it now:

```bash
git status
# If clean, skip. If dirty, commit with a fix message.
```

---

## Summary of Branch Layout

After all tasks:

- `crates/rocket-collection/src/contract/` — domain types only, no serde, no back-compat.
- `crates/rocket-app/src/contract_service.rs` — `ContractSummary`, `ContractDriftSummary` are pure domain.
- `crates/rocket-infra/src/contract_records/` — YAML records, all camelCase, owns back-compat deserializers, owns `serde(default)` legacy fields.
- `src-tauri/src/commands/contract_dtos/` — IPC DTOs, camelCase JSON (snake-case `rel_path` preserved on `ContractScopeDto`).
- `src-tauri/src/commands/contract.rs` — 17 commands signatures use DTOs only.

YAML files on disk are unchanged. Frontend `src/types/contracts.ts` is unchanged. E2E tests pass.
