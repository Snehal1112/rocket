# SP1-01 — Domain: ContractParty, PartyKind, ContractPolicy

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.


> **⚠️ Worktree required** — run SP0 first. Verify before any command:
> ```bash
> pwd                        # must end with .worktrees/contract-enhancement
> git branch --show-current  # must print feat/contract-lock-enhancement
> ```

**Goal:** Add `ContractParty`, `PartyKind`, `ContractPolicy`, and `BreakingChangePolicy` types to `rocket-collection`, with backward-compat serde that reads old plain-string provider/consumer YAML.

**Architecture:** All new types live in `crates/rocket-collection/src/contract/types.rs` alongside the existing `Contract`. Custom `Deserialize` impls handle the old `provider: "string"` format. All new fields on `Contract` use `#[serde(default)]` so existing YAML files deserialise without errors.

**Tech Stack:** Rust, serde, serde_yaml, chrono

**Spec:** `docs/superpowers/specs/2026-05-07-contract-lock-enhancement-design.md` §SP1

---

## Task 1: Add `ContractParty`, `PartyKind`, `ContractPolicy`, `BreakingChangePolicy`

**Files:**
- Modify: `crates/rocket-collection/src/contract/types.rs`

- [ ] **Step 1: Write failing serde roundtrip tests**

Add to `#[cfg(test)]` block at the bottom of `types.rs`:

```rust
#[test]
fn contract_party_roundtrip() {
    let party = ContractParty {
        id: "billing-team".into(),
        name: "Billing Team".into(),
        kind: PartyKind::Team,
        avatar_seed: None,
        avatar_color: Some("#3B82F6".into()),
    };
    let yaml = serde_yaml::to_string(&party).unwrap();
    let back: ContractParty = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(party, back);
}

#[test]
fn contract_policy_defaults() {
    let policy: ContractPolicy = serde_yaml::from_str("{}").unwrap();
    assert_eq!(policy.breaking_change_policy, BreakingChangePolicy::Lenient);
    assert_eq!(policy.notice_days, 30);
    assert!(policy.uptime_sla.is_none());
}

#[test]
fn party_kind_snake_case() {
    let y = serde_yaml::to_string(&PartyKind::AdditiveOk).unwrap();
    assert!(y.contains("additive_ok"));
}
```

- [ ] **Step 2: Run tests — verify they fail**

```bash
cargo test -p rocket-collection contract::types 2>&1 | tail -20
```

Expected: compile errors — `ContractParty`, `PartyKind`, etc. not found yet.

- [ ] **Step 3: Add the new types**

In `crates/rocket-collection/src/contract/types.rs`, add after the existing imports:

```rust
/// A party (provider or consumer) in a contract.
/// Backward-compat: deserialises from a plain YAML string (old format)
/// or a full object (new format).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractParty {
    pub id: String,
    pub name: String,
    pub kind: PartyKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_seed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_color: Option<String>,
}

impl ContractParty {
    /// Create from a plain string (backward compat).
    pub fn from_name(name: &str) -> Self {
        Self {
            id: name.to_lowercase().replace(' ', "-"),
            name: name.to_string(),
            kind: PartyKind::Team,
            avatar_seed: None,
            avatar_color: None,
        }
    }
}

/// Custom Deserialize: accepts both a plain string ("Billing Team") and
/// a full object ({id: ..., name: ..., kind: ...}).
impl<'de> serde::Deserialize<'de> for ContractParty {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::{self, Visitor, MapAccess};
        use std::fmt;

        struct ContractPartyVisitor;

        impl<'de> Visitor<'de> for ContractPartyVisitor {
            type Value = ContractParty;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a string or a ContractParty object")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<ContractParty, E> {
                Ok(ContractParty::from_name(v))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<ContractParty, E> {
                Ok(ContractParty::from_name(&v))
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<ContractParty, A::Error> {
                // Delegate to the derived impl via a helper struct
                #[derive(serde::Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct ContractPartyHelper {
                    id: String,
                    name: String,
                    #[serde(default)]
                    kind: PartyKind,
                    #[serde(default)]
                    avatar_seed: Option<String>,
                    #[serde(default)]
                    avatar_color: Option<String>,
                }
                let h = ContractPartyHelper::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )?;
                Ok(ContractParty { id: h.id, name: h.name, kind: h.kind, avatar_seed: h.avatar_seed, avatar_color: h.avatar_color })
            }
        }

        d.deserialize_any(ContractPartyVisitor)
    }
}

/// What kind of entity a party represents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PartyKind {
    #[default]
    Team,
    Company,
    Service,
}

/// Policy governing how contract drift is evaluated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractPolicy {
    #[serde(default)]
    pub breaking_change_policy: BreakingChangePolicy,
    #[serde(default = "default_notice_days")]
    pub notice_days: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime_sla: Option<f32>,
}

impl Default for ContractPolicy {
    fn default() -> Self {
        Self { breaking_change_policy: BreakingChangePolicy::Lenient, notice_days: 30, uptime_sla: None }
    }
}

fn default_notice_days() -> u32 { 30 }

/// How strictly drift is classified as breaking.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BreakingChangePolicy {
    /// Any change to the API shape is breaking.
    Strict,
    /// Only changes that remove or modify existing fields are breaking.
    #[default]
    Lenient,
    /// Additive changes (new params, new endpoints) are not breaking.
    AdditiveOk,
}
```

- [ ] **Step 4: Run tests — verify they pass**

```bash
cargo test -p rocket-collection contract::types 2>&1 | tail -20
```

Expected: all 3 tests pass.

- [ ] **Step 5: Compile check**

```bash
cargo check -p rocket-collection
```

Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-collection/src/contract/types.rs
git commit -m "feat(contract): add ContractParty, PartyKind, ContractPolicy, BreakingChangePolicy"
```

---

## Task 2: Update `Contract` struct with new fields + backward-compat consumers

**Files:**
- Modify: `crates/rocket-collection/src/contract/types.rs`

- [ ] **Step 1: Write backward-compat tests**

Add to `#[cfg(test)]` block:

```rust
#[test]
fn old_yaml_provider_string_deserialises() {
    let yaml = r#"
id: 01HWXYZ
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
    let c: Contract = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(c.provider.name, "Billing Team");
    assert_eq!(c.provider.id, "billing-team");
    assert_eq!(c.consumers.len(), 1);
    assert_eq!(c.consumers[0].name, "Platform Team");
    assert_eq!(c.status, ContractStatus::Active);
    assert_eq!(c.version, "1.0.0");
}

#[test]
fn new_yaml_consumers_vec_deserialises() {
    let yaml = r#"
id: 01HWXYZ
title: Payments API
provider:
  id: billing-team
  name: Billing Team
  kind: team
consumers:
  - id: platform-team
    name: Platform Team
    kind: team
  - id: mobile-team
    name: Mobile Team
    kind: team
project: Checkout
version: "2.0.0"
status: drift
effectiveDate: "2026-01-15"
enforcementMode: informational
scope:
  type: collection
policy:
  breakingChangePolicy: strict
  noticeDays: 14
driftCount: 3
breachCount: 1
"#;
    let c: Contract = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(c.consumers.len(), 2);
    assert_eq!(c.status, ContractStatus::Drift);
    assert_eq!(c.drift_count, 3);
    assert_eq!(c.policy.breaking_change_policy, BreakingChangePolicy::Strict);
}
```

- [ ] **Step 2: Run — verify they fail**

```bash
cargo test -p rocket-collection contract::types 2>&1 | tail -20
```

Expected: compile errors — `consumers`, `drift_count`, etc. not on `Contract` yet.

- [ ] **Step 3: Update `Contract` struct**

Replace the existing `Contract` struct definition in `types.rs` with:

```rust
/// A contract between a provider and one or more consumers.
///
/// Backward compat: `provider` accepts both a plain string and an object.
/// `consumer` (singular, old format) is also accepted and mapped to `consumers`.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    pub id: Ulid,
    pub title: String,

    // Upgraded — backward-compat via custom Deserialize below
    pub provider: ContractParty,
    pub consumers: Vec<ContractParty>,

    pub project: String,

    #[serde(default = "default_version")]
    pub version: String,

    // Status is now stored, not computed
    #[serde(default)]
    pub status: ContractStatus,

    pub effective_date: NaiveDate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiry_date: Option<NaiveDate>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub document_paths: Vec<std::path::PathBuf>,

    pub enforcement_mode: ContractEnforcementMode,
    pub scope: ContractScope,

    #[serde(default)]
    pub policy: ContractPolicy,

    #[serde(default)]
    pub drift_count: u32,
    #[serde(default)]
    pub breach_count: u32,
    #[serde(default)]
    pub endpoint_count: u32,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn default_version() -> String { "1.0.0".to_string() }

/// Custom Deserialize for `Contract` handles the old `consumer: String`
/// field (singular) and maps it to `consumers: Vec<ContractParty>`.
impl<'de> serde::Deserialize<'de> for Contract {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::{MapAccess, Visitor};
        use std::fmt;

        struct ContractVisitor;

        impl<'de> Visitor<'de> for ContractVisitor {
            type Value = Contract;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a Contract object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Contract, A::Error> {
                use serde::de::Error;

                let mut id: Option<Ulid> = None;
                let mut title: Option<String> = None;
                let mut provider: Option<ContractParty> = None;
                let mut consumers: Option<Vec<ContractParty>> = None;
                let mut consumer_singular: Option<ContractParty> = None; // old format
                let mut project: Option<String> = None;
                let mut version: Option<String> = None;
                let mut status: Option<ContractStatus> = None;
                let mut effective_date: Option<NaiveDate> = None;
                let mut expiry_date: Option<NaiveDate> = None;
                let mut document_paths: Option<Vec<std::path::PathBuf>> = None;
                let mut enforcement_mode: Option<ContractEnforcementMode> = None;
                let mut scope: Option<ContractScope> = None;
                let mut policy: Option<ContractPolicy> = None;
                let mut drift_count: Option<u32> = None;
                let mut breach_count: Option<u32> = None;
                let mut endpoint_count: Option<u32> = None;
                let mut created_by: Option<String> = None;
                let mut created_at: Option<chrono::DateTime<chrono::Utc>> = None;
                let mut updated_at: Option<chrono::DateTime<chrono::Utc>> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "id" => id = Some(map.next_value()?),
                        "title" => title = Some(map.next_value()?),
                        "provider" => provider = Some(map.next_value()?),
                        "consumers" => consumers = Some(map.next_value()?),
                        // Old singular field
                        "consumer" => consumer_singular = Some(map.next_value()?),
                        "project" => project = Some(map.next_value()?),
                        "version" => version = Some(map.next_value()?),
                        "status" => status = Some(map.next_value()?),
                        "effectiveDate" => effective_date = Some(map.next_value()?),
                        "expiryDate" => expiry_date = Some(map.next_value()?),
                        "documentPaths" => document_paths = Some(map.next_value()?),
                        "enforcementMode" => enforcement_mode = Some(map.next_value()?),
                        "scope" => scope = Some(map.next_value()?),
                        "policy" => policy = Some(map.next_value()?),
                        "driftCount" => drift_count = Some(map.next_value()?),
                        "breachCount" => breach_count = Some(map.next_value()?),
                        "endpointCount" => endpoint_count = Some(map.next_value()?),
                        "createdBy" => created_by = Some(map.next_value()?),
                        "createdAt" => created_at = Some(map.next_value()?),
                        "updatedAt" => updated_at = Some(map.next_value()?),
                        _ => { let _ = map.next_value::<serde::de::IgnoredAny>()?; }
                    }
                }

                // Resolve consumers: prefer explicit Vec, fall back to singular old field
                let resolved_consumers = consumers
                    .or_else(|| consumer_singular.map(|c| vec![c]))
                    .unwrap_or_default();

                Ok(Contract {
                    id: id.ok_or_else(|| A::Error::missing_field("id"))?,
                    title: title.ok_or_else(|| A::Error::missing_field("title"))?,
                    provider: provider.ok_or_else(|| A::Error::missing_field("provider"))?,
                    consumers: resolved_consumers,
                    project: project.unwrap_or_default(),
                    version: version.unwrap_or_else(default_version),
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

        d.deserialize_map(ContractVisitor)
    }
}
```

- [ ] **Step 4: Run tests — verify they pass**

```bash
cargo test -p rocket-collection contract::types 2>&1 | tail -20
```

Expected: all 5 tests pass.

- [ ] **Step 5: Compile workspace**

```bash
cargo check --workspace 2>&1 | grep "^error" | head -20
```

Fix any field-name mismatches from callers of the old `provider: String` or `consumer: String` fields. They will be in `rocket-app/src/contract_service.rs` and `src-tauri/src/commands/contract.rs`. For each, update references:
- `contract.provider` (was `String`) → `contract.provider.name` or `contract.provider.id`
- `contract.consumer` (was `String`) → `contract.consumers[0].name`

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-collection/src/contract/types.rs
git commit -m "feat(contract): update Contract struct — ContractParty, Vec<consumers>, stored status, policy, drift counts"
```

---

## Task 3: Export new types from `mod.rs`

**Files:**
- Modify: `crates/rocket-collection/src/contract/mod.rs`

- [ ] **Step 1: Update re-exports**

Open `crates/rocket-collection/src/contract/mod.rs`. The current `pub use types::...` line exports only the old types. Replace it with:

```rust
pub use types::{
    BreakingChangePolicy,
    Contract,
    ContractEnforcementMode,
    ContractParty,
    ContractPolicy,
    ContractScope,
    ContractStatus,
    PartyKind,
};
```

- [ ] **Step 2: Full workspace compile + tests**

```bash
cargo test --workspace 2>&1 | tail -30
```

Expected: all existing tests pass. Fix any remaining callers of removed fields.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-collection/src/contract/mod.rs
git commit -m "feat(contract): re-export new domain types from contract module"
```
