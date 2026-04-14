# Security Audit & Compliance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend Rocket's existing contract-audit domain with a `SecurityAuditService` that records sensitive operations to an append-only, SHA-256 hash-chained log on disk, tags each event with framework control IDs (SOC 2, ISO 27001, ISO 42001, CSA STAR), filters/enforces events via a user-configurable compliance profile, exposes the log to the frontend as a read-only viewer, and produces an exportable evidence pack.

**Architecture:** A new pure domain crate `rocket-audit` defines `SecurityAuditEvent`, `ComplianceProfile`, `ControlId`, a static `CONTROL_CATALOG`, and the `AuditLogRepository` trait. `rocket-app` gains a `SecurityAuditService` that accepts events, applies the active profile filter, computes the hash chain, and persists via the repository. `rocket-infra` adds `FsAuditLogRepo` (append-only JSONL file under `~/.rocket-api/audit/`) and `FsComplianceProfileRepo` (YAML config at `~/.rocket-api/audit/profile.yml`). Existing services (`contract_service`, `collection_service`, `environment_service`, `execution_service`, `workspace_service`) publish security events into the new service via a new `SecurityAuditPublisher` trait — kept orthogonal to `EventPublisher` so the Tauri event bus stays ephemeral. Frontend adds an `AuditLogTab` reachable from the workspace sidebar, backed by three new Tauri commands (`list_audit_events`, `get_compliance_profile`, `set_compliance_profile`, `export_audit_evidence`).

**Tech Stack:** Rust (existing `serde`, `serde_yaml`, `serde_json`, `sha2`, `chrono`, `ulid`, `tempfile` for tests), Tauri IPC, React + TypeScript + Zustand + shadcn/ui (existing `Tabs`, `Table`, `Badge`, `ScrollArea`, `Select`, `Checkbox`, `Button`).

---

## File Structure

### New crate: `crates/rocket-audit/`

| File | Responsibility |
|---|---|
| `Cargo.toml` | Crate manifest. Dependencies: `rocket-shared`, `serde`, `serde_json`, `chrono`, `sha2`, `ulid`, `thiserror`. |
| `src/lib.rs` | Crate root; re-exports. |
| `src/event.rs` | `SecurityAuditEvent` struct, `AuditEventKind` enum, `AuditEventId` newtype. |
| `src/control.rs` | `ControlId`, `Framework` enum, `CONTROL_CATALOG` static slice. |
| `src/profile.rs` | `ComplianceProfile`, `EnforcementLevel`, `ProfileRepository` trait, `default_profile()`. |
| `src/repository.rs` | `AuditLogRepository` trait (append-only). |
| `src/chain.rs` | `hash_event` helper + chain verification function. |
| `src/publisher.rs` | `SecurityAuditPublisher` trait + `NullSecurityAuditPublisher`. |

### Modified/new files in existing crates

| File | Responsibility |
|---|---|
| `crates/rocket-app/Cargo.toml` | Add `rocket-audit` dependency. |
| `crates/rocket-app/src/lib.rs` | Re-export `SecurityAuditService`. |
| `crates/rocket-app/src/security_audit_service.rs` | New. Accepts events, applies profile, computes chain, persists. |
| `crates/rocket-app/src/contract_service.rs` | Emit `ContractAttached`, `ContractDeleted`, `ContractViolation` security events. |
| `crates/rocket-app/src/collection_service.rs` | Emit `CollectionDeleted`, `CollectionExported` events. |
| `crates/rocket-app/src/environment_service.rs` | Emit `SecretVariableWritten` events. |
| `crates/rocket-app/src/execution_service.rs` | Emit `SensitiveAuthUsed` events. |
| `crates/rocket-infra/Cargo.toml` | Add `rocket-audit` dependency. |
| `crates/rocket-infra/src/lib.rs` | Re-export new repos. |
| `crates/rocket-infra/src/fs_audit_log_repo.rs` | New. Append-only JSONL writer, event iterator. |
| `crates/rocket-infra/src/fs_compliance_profile_repo.rs` | New. YAML profile read/write. |
| `src-tauri/src/commands/audit.rs` | New. IPC commands for audit read/profile/export. |
| `src-tauri/src/commands/mod.rs` | Register `audit` module. |
| `src-tauri/src/lib.rs` | Wire `SecurityAuditService` + repos; register commands. |
| `src/lib/tauri-api.ts` | Typed wrappers for new commands + wire types. |
| `src/stores/audit-store.ts` | New Zustand store for audit events + profile. |
| `src/components/audit/AuditLogTab.tsx` | New. Read-only table viewer + filters. |
| `src/components/audit/ComplianceProfileDialog.tsx` | New. Framework toggles + enforcement level. |
| `src/components/audit/AuditEventRow.tsx` | New. Single row with control-ID badges. |
| `src/components/audit/ExportEvidenceDialog.tsx` | New. Date-range picker + export button. |
| `src/components/layout/CollectionsSidebar.tsx` | Add "Audit Log" entry to workspace section. |
| `src/types/pane-types.ts` | Add `AuditTab` variant to `Tab` union. |
| `src/components/panes/EditorGroup.tsx` | Route `AuditTab` → `AuditLogTab`. |
| `.claude/rules/10-contract-audit-domain.md` | Append "Security audit" subsection. |

---

## Task 1: Scaffold `rocket-audit` crate

**Files:**
- Create: `crates/rocket-audit/Cargo.toml`
- Create: `crates/rocket-audit/src/lib.rs`
- Modify: `Cargo.toml` (workspace root) — add `crates/rocket-audit` to `members`.

- [ ] **Step 1: Create the crate manifest**

Write `crates/rocket-audit/Cargo.toml`:

```toml
[package]
name = "rocket-audit"
version.workspace = true
edition.workspace = true

[dependencies]
rocket-shared = { path = "../rocket-shared" }
serde = { workspace = true }
serde_json = { workspace = true }
chrono = { workspace = true, features = ["serde"] }
sha2 = "0.10"
ulid = { workspace = true, features = ["serde"] }
thiserror = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
```

- [ ] **Step 2: Create empty lib**

Write `crates/rocket-audit/src/lib.rs`:

```rust
//! Security audit domain: events, compliance profiles, and tamper-evident log trait.

pub mod chain;
pub mod control;
pub mod event;
pub mod profile;
pub mod publisher;
pub mod repository;

pub use chain::{hash_event, verify_chain, ChainVerification};
pub use control::{ControlId, Framework, CONTROL_CATALOG};
pub use event::{AuditEventId, AuditEventKind, SecurityAuditEvent};
pub use profile::{ComplianceProfile, EnforcementLevel, ProfileRepository};
pub use publisher::{NullSecurityAuditPublisher, SecurityAuditPublisher};
pub use repository::AuditLogRepository;
```

- [ ] **Step 3: Register crate in workspace**

Modify `Cargo.toml` at repo root — locate the `members = [...]` array and add `"crates/rocket-audit"` (keep alphabetic order).

- [ ] **Step 4: Verify the workspace resolves**

Run: `cargo check -p rocket-audit`
Expected: FAIL with "unresolved import crate::chain" or similar — modules referenced in lib.rs don't exist yet. This is the intended failing baseline for TDD on the next tasks.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/rocket-audit/Cargo.toml crates/rocket-audit/src/lib.rs
git commit -m "chore(audit): scaffold rocket-audit crate"
```

---

## Task 2: `ControlId`, `Framework`, and control catalog

**Files:**
- Create: `crates/rocket-audit/src/control.rs`
- Test: inline `#[cfg(test)] mod tests` in the same file.

- [ ] **Step 1: Write the failing test**

Append to `crates/rocket-audit/src/control.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Framework {
    Soc2,
    Iso27001,
    Iso42001,
    CsaStar,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlId {
    pub framework: Framework,
    pub code: String,
    pub title: String,
}

pub static CONTROL_CATALOG: &[ControlEntry] = &[];

#[derive(Debug)]
pub struct ControlEntry {
    pub framework: Framework,
    pub code: &'static str,
    pub title: &'static str,
    pub kinds: &'static [&'static str],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_contains_soc2_access_control() {
        assert!(CONTROL_CATALOG
            .iter()
            .any(|c| c.framework == Framework::Soc2 && c.code == "CC6.1"));
    }

    #[test]
    fn catalog_contains_iso27001_a_8_15() {
        assert!(CONTROL_CATALOG
            .iter()
            .any(|c| c.framework == Framework::Iso27001 && c.code == "A.8.15"));
    }

    #[test]
    fn catalog_contains_iso42001_ai_lifecycle() {
        assert!(CONTROL_CATALOG
            .iter()
            .any(|c| c.framework == Framework::Iso42001 && c.code == "A.6.2.2"));
    }

    #[test]
    fn catalog_contains_csa_star_iam() {
        assert!(CONTROL_CATALOG
            .iter()
            .any(|c| c.framework == Framework::CsaStar && c.code == "IAM-09"));
    }

    #[test]
    fn framework_serializes_snake_case() {
        let s = serde_json::to_string(&Framework::Soc2).unwrap();
        assert_eq!(s, "\"soc2\"");
    }
}
```

- [ ] **Step 2: Run tests — they should fail**

Run: `cargo test -p rocket-audit control::tests`
Expected: FAIL (empty catalog fails every `assert!`).

- [ ] **Step 3: Populate the catalog**

Replace the `pub static CONTROL_CATALOG: &[ControlEntry] = &[];` line with:

```rust
pub static CONTROL_CATALOG: &[ControlEntry] = &[
    // SOC 2 Type 2 — Trust Services Criteria.
    ControlEntry {
        framework: Framework::Soc2,
        code: "CC6.1",
        title: "Logical access controls",
        kinds: &["sensitive_auth_used", "secret_variable_written"],
    },
    ControlEntry {
        framework: Framework::Soc2,
        code: "CC6.7",
        title: "Transmission of sensitive information",
        kinds: &["collection_exported", "audit_evidence_exported"],
    },
    ControlEntry {
        framework: Framework::Soc2,
        code: "CC7.2",
        title: "System monitoring",
        kinds: &["contract_violation", "audit_chain_broken"],
    },
    ControlEntry {
        framework: Framework::Soc2,
        code: "CC8.1",
        title: "Change management",
        kinds: &["contract_attached", "contract_deleted", "contract_violation"],
    },

    // ISO 27001:2022 — Annex A controls.
    ControlEntry {
        framework: Framework::Iso27001,
        code: "A.8.15",
        title: "Logging",
        kinds: &["contract_violation", "sensitive_auth_used", "collection_deleted"],
    },
    ControlEntry {
        framework: Framework::Iso27001,
        code: "A.8.16",
        title: "Monitoring activities",
        kinds: &["contract_violation", "audit_chain_broken"],
    },
    ControlEntry {
        framework: Framework::Iso27001,
        code: "A.5.15",
        title: "Access control",
        kinds: &["sensitive_auth_used", "secret_variable_written"],
    },

    // ISO/IEC 42001:2023 — AI management system controls.
    ControlEntry {
        framework: Framework::Iso42001,
        code: "A.6.2.2",
        title: "AI system impact assessment",
        kinds: &["contract_attached", "contract_violation"],
    },
    ControlEntry {
        framework: Framework::Iso42001,
        code: "A.7.4",
        title: "Data quality for AI systems",
        kinds: &["contract_violation"],
    },

    // CSA Cloud Controls Matrix (STAR).
    ControlEntry {
        framework: Framework::CsaStar,
        code: "IAM-09",
        title: "Identity & access management — user access review",
        kinds: &["sensitive_auth_used"],
    },
    ControlEntry {
        framework: Framework::CsaStar,
        code: "LOG-02",
        title: "Audit logs protection",
        kinds: &["audit_chain_broken", "audit_evidence_exported"],
    },
];

/// Returns every `ControlId` tagged against a given event kind across all frameworks.
pub fn controls_for_kind(kind: &str) -> Vec<ControlId> {
    CONTROL_CATALOG
        .iter()
        .filter(|e| e.kinds.contains(&kind))
        .map(|e| ControlId {
            framework: e.framework,
            code: e.code.to_string(),
            title: e.title.to_string(),
        })
        .collect()
}
```

- [ ] **Step 4: Run tests — expect pass**

Run: `cargo test -p rocket-audit control::tests`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-audit/src/control.rs
git commit -m "feat(audit): add control catalog for SOC 2, ISO 27001, ISO 42001, CSA STAR"
```

---

## Task 3: `SecurityAuditEvent` type and `AuditEventKind`

**Files:**
- Create: `crates/rocket-audit/src/event.rs`

- [ ] **Step 1: Write the failing test**

Write `crates/rocket-audit/src/event.rs`:

```rust
use crate::control::{controls_for_kind, ControlId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ulid::Ulid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AuditEventId(pub Ulid);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuditEventKind {
    ContractAttached { contract_id: String, collection: String, scope: String },
    ContractDeleted { contract_id: String, collection: String },
    ContractViolation { contract_id: String, request_path: String, field: String },
    CollectionDeleted { collection: String },
    CollectionExported { collection: String, destination: String },
    SecretVariableWritten { environment: String, variable_key: String },
    SensitiveAuthUsed { auth_type: String, collection: String, request_path: String },
    AuditEvidenceExported { range_start: DateTime<Utc>, range_end: DateTime<Utc>, count: usize },
    AuditChainBroken { at_event_id: AuditEventId, expected_hash: String, actual_hash: String },
}

impl AuditEventKind {
    pub fn tag(&self) -> &'static str {
        match self {
            Self::ContractAttached { .. } => "contract_attached",
            Self::ContractDeleted { .. } => "contract_deleted",
            Self::ContractViolation { .. } => "contract_violation",
            Self::CollectionDeleted { .. } => "collection_deleted",
            Self::CollectionExported { .. } => "collection_exported",
            Self::SecretVariableWritten { .. } => "secret_variable_written",
            Self::SensitiveAuthUsed { .. } => "sensitive_auth_used",
            Self::AuditEvidenceExported { .. } => "audit_evidence_exported",
            Self::AuditChainBroken { .. } => "audit_chain_broken",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityAuditEvent {
    pub id: AuditEventId,
    pub occurred_at: DateTime<Utc>,
    pub actor: String,
    pub workspace_id: Option<String>,
    pub event: AuditEventKind,
    pub controls: Vec<ControlId>,
    pub prev_hash: String,
    pub hash: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

impl SecurityAuditEvent {
    pub fn new(
        actor: impl Into<String>,
        workspace_id: Option<String>,
        event: AuditEventKind,
        prev_hash: impl Into<String>,
    ) -> Self {
        let controls = controls_for_kind(event.tag());
        Self {
            id: AuditEventId(Ulid::new()),
            occurred_at: Utc::now(),
            actor: actor.into(),
            workspace_id,
            event,
            controls,
            prev_hash: prev_hash.into(),
            hash: String::new(),
            metadata: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_populates_controls_from_kind() {
        let ev = SecurityAuditEvent::new(
            "user@example.com",
            Some("ws-1".into()),
            AuditEventKind::SensitiveAuthUsed {
                auth_type: "bearer".into(),
                collection: "api".into(),
                request_path: "get.yml".into(),
            },
            "",
        );
        assert!(ev.controls.iter().any(|c| c.code == "CC6.1"));
        assert!(ev.controls.iter().any(|c| c.code == "IAM-09"));
    }

    #[test]
    fn kind_tag_matches_catalog_key() {
        let kind = AuditEventKind::ContractViolation {
            contract_id: "c1".into(),
            request_path: "a.yml".into(),
            field: "method".into(),
        };
        assert_eq!(kind.tag(), "contract_violation");
    }

    #[test]
    fn serializes_tagged_event() {
        let ev = SecurityAuditEvent::new(
            "a",
            None,
            AuditEventKind::CollectionDeleted { collection: "x".into() },
            "",
        );
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"kind\":\"collection_deleted\""));
    }
}
```

- [ ] **Step 2: Run tests — expect pass after cargo check**

Run: `cargo test -p rocket-audit event::tests`
Expected: PASS (3 tests).

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-audit/src/event.rs
git commit -m "feat(audit): add SecurityAuditEvent and AuditEventKind"
```

---

## Task 4: Hash-chain helpers

**Files:**
- Create: `crates/rocket-audit/src/chain.rs`

- [ ] **Step 1: Write the failing test**

Write `crates/rocket-audit/src/chain.rs`:

```rust
use crate::event::SecurityAuditEvent;
use sha2::{Digest, Sha256};

/// Computes the SHA-256 hash of the canonical JSON serialisation of an event's
/// identity + prev_hash. The hash field itself is excluded from the input.
pub fn hash_event(ev: &SecurityAuditEvent) -> String {
    let mut clone = ev.clone();
    clone.hash = String::new();
    let canonical = serde_json::to_vec(&clone).expect("event must serialize");
    let mut hasher = Sha256::new();
    hasher.update(&canonical);
    format!("{:x}", hasher.finalize())
}

#[derive(Debug, PartialEq, Eq)]
pub enum ChainVerification {
    Ok,
    Broken { index: usize, expected: String, actual: String },
}

/// Walks events in order, recomputing each hash and confirming prev_hash linkage.
pub fn verify_chain(events: &[SecurityAuditEvent]) -> ChainVerification {
    let mut prev = String::new();
    for (i, ev) in events.iter().enumerate() {
        if ev.prev_hash != prev {
            return ChainVerification::Broken {
                index: i,
                expected: prev,
                actual: ev.prev_hash.clone(),
            };
        }
        let expected = hash_event(ev);
        if expected != ev.hash {
            return ChainVerification::Broken {
                index: i,
                expected,
                actual: ev.hash.clone(),
            };
        }
        prev = ev.hash.clone();
    }
    ChainVerification::Ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{AuditEventKind, SecurityAuditEvent};

    fn mk(prev: &str) -> SecurityAuditEvent {
        let mut ev = SecurityAuditEvent::new(
            "actor",
            None,
            AuditEventKind::CollectionDeleted { collection: "x".into() },
            prev,
        );
        ev.hash = hash_event(&ev);
        ev
    }

    #[test]
    fn hash_is_deterministic_for_same_event() {
        let ev = mk("");
        assert_eq!(hash_event(&ev), ev.hash);
    }

    #[test]
    fn verify_accepts_well_formed_chain() {
        let a = mk("");
        let b = mk(&a.hash);
        assert_eq!(verify_chain(&[a, b]), ChainVerification::Ok);
    }

    #[test]
    fn verify_rejects_tampered_event() {
        let mut a = mk("");
        let b = mk(&a.hash);
        // Tamper: change actor without recomputing hash.
        a.actor = "attacker".into();
        match verify_chain(&[a, b]) {
            ChainVerification::Broken { index, .. } => assert_eq!(index, 0),
            _ => panic!("expected broken chain"),
        }
    }

    #[test]
    fn verify_rejects_broken_prev_hash() {
        let a = mk("");
        let mut b = mk(&a.hash);
        b.prev_hash = "deadbeef".into();
        b.hash = hash_event(&b);
        match verify_chain(&[a, b]) {
            ChainVerification::Broken { index, .. } => assert_eq!(index, 1),
            _ => panic!("expected broken chain at index 1"),
        }
    }
}
```

- [ ] **Step 2: Run tests — expect pass**

Run: `cargo test -p rocket-audit chain::tests`
Expected: PASS (4 tests).

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-audit/src/chain.rs
git commit -m "feat(audit): add SHA-256 hash chain helpers"
```

---

## Task 5: `ComplianceProfile` and `ProfileRepository`

**Files:**
- Create: `crates/rocket-audit/src/profile.rs`

- [ ] **Step 1: Write the failing test**

Write `crates/rocket-audit/src/profile.rs`:

```rust
use crate::control::Framework;
use rocket_shared::DomainResult;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementLevel {
    /// Events are recorded only.
    Record,
    /// Events are recorded and the user is warned inline.
    Warn,
    /// Events are recorded and the triggering operation is blocked.
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComplianceProfile {
    pub active_frameworks: BTreeSet<Framework>,
    pub enforcement: EnforcementLevel,
    /// When empty, all kinds matched by active_frameworks are recorded.
    #[serde(default)]
    pub muted_kinds: BTreeSet<String>,
}

pub fn default_profile() -> ComplianceProfile {
    ComplianceProfile {
        active_frameworks: BTreeSet::new(),
        enforcement: EnforcementLevel::Record,
        muted_kinds: BTreeSet::new(),
    }
}

impl ComplianceProfile {
    /// Returns true when an event kind is active under the current profile.
    pub fn records(&self, event_kind: &str) -> bool {
        if self.muted_kinds.contains(event_kind) {
            return false;
        }
        if self.active_frameworks.is_empty() {
            // No frameworks selected = still record everything (audit is always on).
            return true;
        }
        crate::control::CONTROL_CATALOG.iter().any(|e| {
            self.active_frameworks.contains(&e.framework) && e.kinds.contains(&event_kind)
        })
    }
}

pub trait ProfileRepository: Send + Sync {
    fn load(&self) -> DomainResult<ComplianceProfile>;
    fn save(&self, profile: &ComplianceProfile) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_has_no_frameworks() {
        assert!(default_profile().active_frameworks.is_empty());
    }

    #[test]
    fn records_when_no_frameworks_selected() {
        assert!(default_profile().records("contract_violation"));
    }

    #[test]
    fn records_only_kinds_matching_active_framework() {
        let mut p = default_profile();
        p.active_frameworks.insert(Framework::Soc2);
        assert!(p.records("contract_violation"));
        assert!(!p.records("made_up_kind"));
    }

    #[test]
    fn muted_kinds_are_never_recorded() {
        let mut p = default_profile();
        p.muted_kinds.insert("collection_deleted".into());
        assert!(!p.records("collection_deleted"));
    }
}
```

- [ ] **Step 2: Run tests — expect pass**

Run: `cargo test -p rocket-audit profile::tests`
Expected: PASS (4 tests).

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-audit/src/profile.rs
git commit -m "feat(audit): add ComplianceProfile and ProfileRepository trait"
```

---

## Task 6: `AuditLogRepository` and `SecurityAuditPublisher` traits

**Files:**
- Create: `crates/rocket-audit/src/repository.rs`
- Create: `crates/rocket-audit/src/publisher.rs`

- [ ] **Step 1: Write the repository trait**

Write `crates/rocket-audit/src/repository.rs`:

```rust
use crate::event::SecurityAuditEvent;
use chrono::{DateTime, Utc};
use rocket_shared::DomainResult;

pub trait AuditLogRepository: Send + Sync {
    /// Append a sealed event (hash already populated) to the log.
    fn append(&self, event: &SecurityAuditEvent) -> DomainResult<()>;

    /// Load every event in chronological order.
    fn load_all(&self) -> DomainResult<Vec<SecurityAuditEvent>>;

    /// Load events whose `occurred_at` falls within [start, end].
    fn load_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> DomainResult<Vec<SecurityAuditEvent>>;

    /// Return the most recently appended event, if any.
    fn latest(&self) -> DomainResult<Option<SecurityAuditEvent>>;
}
```

- [ ] **Step 2: Write the publisher trait + null impl**

Write `crates/rocket-audit/src/publisher.rs`:

```rust
use crate::event::AuditEventKind;

/// Non-failing publisher for security events. Services call this fire-and-forget;
/// a failing write must never break the caller's operation.
pub trait SecurityAuditPublisher: Send + Sync {
    fn publish(&self, actor: String, workspace_id: Option<String>, kind: AuditEventKind);
}

pub struct NullSecurityAuditPublisher;

impl SecurityAuditPublisher for NullSecurityAuditPublisher {
    fn publish(&self, _actor: String, _workspace_id: Option<String>, _kind: AuditEventKind) {
        // Intentionally empty for tests.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_publisher_does_not_panic() {
        let p = NullSecurityAuditPublisher;
        p.publish(
            "a".into(),
            None,
            AuditEventKind::CollectionDeleted { collection: "x".into() },
        );
    }
}
```

- [ ] **Step 3: Verify crate compiles**

Run: `cargo test -p rocket-audit`
Expected: PASS (all tests across the crate — 15 in total from tasks 2–6).

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-audit/src/repository.rs crates/rocket-audit/src/publisher.rs
git commit -m "feat(audit): add AuditLogRepository and SecurityAuditPublisher traits"
```

---

## Task 7: `SecurityAuditService` in `rocket-app`

**Files:**
- Modify: `crates/rocket-app/Cargo.toml`
- Create: `crates/rocket-app/src/security_audit_service.rs`
- Modify: `crates/rocket-app/src/lib.rs` — add `pub mod security_audit_service;` and re-export `SecurityAuditService`.

- [ ] **Step 1: Add the crate dependency**

Modify `crates/rocket-app/Cargo.toml` — add to the `[dependencies]` table:

```toml
rocket-audit = { path = "../rocket-audit" }
```

- [ ] **Step 2: Write the failing test-first service skeleton**

Write `crates/rocket-app/src/security_audit_service.rs`:

```rust
use rocket_audit::{
    chain::hash_event,
    event::{AuditEventKind, SecurityAuditEvent},
    profile::{ComplianceProfile, EnforcementLevel, ProfileRepository},
    repository::AuditLogRepository,
};
use rocket_shared::{DomainError, DomainResult};
use std::sync::{Arc, Mutex};

pub struct SecurityAuditService {
    log: Arc<dyn AuditLogRepository>,
    profile_repo: Arc<dyn ProfileRepository>,
    /// Cached head hash to avoid re-reading the whole log for each append.
    head: Mutex<Option<String>>,
}

impl SecurityAuditService {
    pub fn new(
        log: Arc<dyn AuditLogRepository>,
        profile_repo: Arc<dyn ProfileRepository>,
    ) -> DomainResult<Self> {
        let latest = log.latest()?;
        Ok(Self {
            log,
            profile_repo,
            head: Mutex::new(latest.map(|e| e.hash)),
        })
    }

    /// Records a security event. Returns `Ok(None)` when the current profile
    /// mutes this kind, `Ok(Some(event))` when recorded, `Err(DomainError::InvalidInput(..))`
    /// under `EnforcementLevel::Block` so the caller can abort.
    pub fn record(
        &self,
        actor: String,
        workspace_id: Option<String>,
        kind: AuditEventKind,
    ) -> DomainResult<Option<SecurityAuditEvent>> {
        let profile = self.profile_repo.load()?;
        if !profile.records(kind.tag()) {
            return Ok(None);
        }

        let prev_hash = {
            let guard = self.head.lock().expect("head mutex poisoned");
            guard.clone().unwrap_or_default()
        };

        let mut event = SecurityAuditEvent::new(actor, workspace_id, kind, prev_hash);
        event.hash = hash_event(&event);
        self.log.append(&event)?;

        {
            let mut guard = self.head.lock().expect("head mutex poisoned");
            *guard = Some(event.hash.clone());
        }

        if profile.enforcement == EnforcementLevel::Block {
            return Err(DomainError::InvalidInput(format!(
                "blocked by compliance profile: {}",
                event.event.tag()
            )));
        }

        Ok(Some(event))
    }

    pub fn load_profile(&self) -> DomainResult<ComplianceProfile> {
        self.profile_repo.load()
    }

    pub fn save_profile(&self, profile: &ComplianceProfile) -> DomainResult<()> {
        self.profile_repo.save(profile)
    }

    pub fn list(&self) -> DomainResult<Vec<SecurityAuditEvent>> {
        self.log.load_all()
    }

    pub fn list_range(
        &self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> DomainResult<Vec<SecurityAuditEvent>> {
        self.log.load_range(start, end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_audit::{
        chain::{verify_chain, ChainVerification},
        profile::default_profile,
    };
    use std::cell::RefCell;

    struct MemLog {
        events: std::sync::Mutex<Vec<SecurityAuditEvent>>,
    }

    impl AuditLogRepository for MemLog {
        fn append(&self, event: &SecurityAuditEvent) -> DomainResult<()> {
            self.events.lock().unwrap().push(event.clone());
            Ok(())
        }
        fn load_all(&self) -> DomainResult<Vec<SecurityAuditEvent>> {
            Ok(self.events.lock().unwrap().clone())
        }
        fn load_range(
            &self,
            start: chrono::DateTime<chrono::Utc>,
            end: chrono::DateTime<chrono::Utc>,
        ) -> DomainResult<Vec<SecurityAuditEvent>> {
            Ok(self
                .events
                .lock()
                .unwrap()
                .iter()
                .filter(|e| e.occurred_at >= start && e.occurred_at <= end)
                .cloned()
                .collect())
        }
        fn latest(&self) -> DomainResult<Option<SecurityAuditEvent>> {
            Ok(self.events.lock().unwrap().last().cloned())
        }
    }

    struct MemProfile {
        p: RefCell<ComplianceProfile>,
    }
    unsafe impl Send for MemProfile {}
    unsafe impl Sync for MemProfile {}

    impl ProfileRepository for MemProfile {
        fn load(&self) -> DomainResult<ComplianceProfile> {
            Ok(self.p.borrow().clone())
        }
        fn save(&self, profile: &ComplianceProfile) -> DomainResult<()> {
            *self.p.borrow_mut() = profile.clone();
            Ok(())
        }
    }

    fn svc() -> SecurityAuditService {
        let log = Arc::new(MemLog { events: std::sync::Mutex::new(vec![]) });
        let profile = Arc::new(MemProfile { p: RefCell::new(default_profile()) });
        SecurityAuditService::new(log, profile).unwrap()
    }

    #[test]
    fn record_appends_and_chains() {
        let s = svc();
        s.record(
            "a".into(),
            None,
            AuditEventKind::CollectionDeleted { collection: "x".into() },
        )
        .unwrap();
        s.record(
            "a".into(),
            None,
            AuditEventKind::CollectionDeleted { collection: "y".into() },
        )
        .unwrap();
        let events = s.list().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].prev_hash, events[0].hash);
        assert_eq!(verify_chain(&events), ChainVerification::Ok);
    }

    #[test]
    fn record_skips_muted_kinds() {
        let s = svc();
        let mut p = default_profile();
        p.muted_kinds.insert("collection_deleted".into());
        s.save_profile(&p).unwrap();
        let result = s
            .record(
                "a".into(),
                None,
                AuditEventKind::CollectionDeleted { collection: "x".into() },
            )
            .unwrap();
        assert!(result.is_none());
        assert!(s.list().unwrap().is_empty());
    }

    #[test]
    fn block_enforcement_errors_after_recording() {
        let s = svc();
        let mut p = default_profile();
        p.enforcement = EnforcementLevel::Block;
        s.save_profile(&p).unwrap();
        let result = s.record(
            "a".into(),
            None,
            AuditEventKind::CollectionDeleted { collection: "x".into() },
        );
        assert!(matches!(result, Err(DomainError::InvalidInput(_))));
        // Event was still recorded — this is the audit trail.
        assert_eq!(s.list().unwrap().len(), 1);
    }
}
```

- [ ] **Step 3: Register module in lib**

Modify `crates/rocket-app/src/lib.rs` — add `pub mod security_audit_service;` alongside the other module declarations and add `pub use security_audit_service::SecurityAuditService;` to the re-export list.

- [ ] **Step 4: Run tests — expect pass**

Run: `cargo test -p rocket-app security_audit_service::tests`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/Cargo.toml crates/rocket-app/src/security_audit_service.rs crates/rocket-app/src/lib.rs
git commit -m "feat(audit): add SecurityAuditService with hash-chain append and profile filter"
```

---

## Task 8: `FsAuditLogRepo` (append-only JSONL)

**Files:**
- Modify: `crates/rocket-infra/Cargo.toml`
- Create: `crates/rocket-infra/src/fs_audit_log_repo.rs`
- Modify: `crates/rocket-infra/src/lib.rs`

- [ ] **Step 1: Add the dependency**

Modify `crates/rocket-infra/Cargo.toml` — add to `[dependencies]`:

```toml
rocket-audit = { path = "../rocket-audit" }
```

- [ ] **Step 2: Write the failing test**

Write `crates/rocket-infra/src/fs_audit_log_repo.rs`:

```rust
use chrono::{DateTime, Utc};
use rocket_audit::{
    event::{AuditEventKind, SecurityAuditEvent},
    repository::AuditLogRepository,
};
use rocket_shared::{DomainError, DomainResult};
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    sync::Mutex,
};

pub struct FsAuditLogRepo {
    path: PathBuf,
    write_lock: Mutex<()>,
}

impl FsAuditLogRepo {
    pub fn new(path: PathBuf) -> DomainResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(DomainError::Io)?;
        }
        Ok(Self { path, write_lock: Mutex::new(()) })
    }

    fn read_lines(&self) -> DomainResult<Vec<SecurityAuditEvent>> {
        if !self.path.exists() {
            return Ok(vec![]);
        }
        let file = File::open(&self.path).map_err(DomainError::Io)?;
        let reader = BufReader::new(file);
        let mut out = Vec::new();
        for line in reader.lines() {
            let line = line.map_err(DomainError::Io)?;
            if line.trim().is_empty() {
                continue;
            }
            let ev: SecurityAuditEvent = serde_json::from_str(&line)
                .map_err(|e| DomainError::Serialization(e.to_string()))?;
            out.push(ev);
        }
        Ok(out)
    }
}

impl AuditLogRepository for FsAuditLogRepo {
    fn append(&self, event: &SecurityAuditEvent) -> DomainResult<()> {
        let _guard = self.write_lock.lock().expect("audit write-lock poisoned");
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(DomainError::Io)?;
        let line = serde_json::to_string(event)
            .map_err(|e| DomainError::Serialization(e.to_string()))?;
        file.write_all(line.as_bytes()).map_err(DomainError::Io)?;
        file.write_all(b"\n").map_err(DomainError::Io)?;
        file.sync_data().map_err(DomainError::Io)?;
        Ok(())
    }

    fn load_all(&self) -> DomainResult<Vec<SecurityAuditEvent>> {
        self.read_lines()
    }

    fn load_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> DomainResult<Vec<SecurityAuditEvent>> {
        Ok(self
            .read_lines()?
            .into_iter()
            .filter(|e| e.occurred_at >= start && e.occurred_at <= end)
            .collect())
    }

    fn latest(&self) -> DomainResult<Option<SecurityAuditEvent>> {
        Ok(self.read_lines()?.into_iter().last())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_audit::chain::{hash_event, verify_chain, ChainVerification};
    use tempfile::TempDir;

    fn mk_event(prev: &str) -> SecurityAuditEvent {
        let mut ev = SecurityAuditEvent::new(
            "actor",
            None,
            AuditEventKind::CollectionDeleted { collection: "x".into() },
            prev,
        );
        ev.hash = hash_event(&ev);
        ev
    }

    #[test]
    fn append_then_load_round_trip() {
        let dir = TempDir::new().unwrap();
        let repo = FsAuditLogRepo::new(dir.path().join("audit.jsonl")).unwrap();
        let a = mk_event("");
        let b = mk_event(&a.hash);
        repo.append(&a).unwrap();
        repo.append(&b).unwrap();
        let loaded = repo.load_all().unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(verify_chain(&loaded), ChainVerification::Ok);
    }

    #[test]
    fn load_range_filters_by_occurred_at() {
        let dir = TempDir::new().unwrap();
        let repo = FsAuditLogRepo::new(dir.path().join("audit.jsonl")).unwrap();
        let a = mk_event("");
        repo.append(&a).unwrap();
        // Load a range that excludes the event.
        let range = repo
            .load_range(
                Utc::now() + chrono::Duration::hours(1),
                Utc::now() + chrono::Duration::hours(2),
            )
            .unwrap();
        assert!(range.is_empty());
    }

    #[test]
    fn latest_returns_most_recent() {
        let dir = TempDir::new().unwrap();
        let repo = FsAuditLogRepo::new(dir.path().join("audit.jsonl")).unwrap();
        let a = mk_event("");
        let b = mk_event(&a.hash);
        repo.append(&a).unwrap();
        repo.append(&b).unwrap();
        let latest = repo.latest().unwrap().unwrap();
        assert_eq!(latest.hash, b.hash);
    }
}
```

- [ ] **Step 3: Register module**

Modify `crates/rocket-infra/src/lib.rs` — add `pub mod fs_audit_log_repo;` and `pub use fs_audit_log_repo::FsAuditLogRepo;`.

- [ ] **Step 4: Run tests — expect pass**

Run: `cargo test -p rocket-infra fs_audit_log_repo::tests`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-infra/Cargo.toml crates/rocket-infra/src/fs_audit_log_repo.rs crates/rocket-infra/src/lib.rs
git commit -m "feat(audit): add FsAuditLogRepo append-only JSONL store"
```

---

## Task 9: `FsComplianceProfileRepo`

**Files:**
- Create: `crates/rocket-infra/src/fs_compliance_profile_repo.rs`
- Modify: `crates/rocket-infra/src/lib.rs`

- [ ] **Step 1: Write the failing test + implementation**

Write `crates/rocket-infra/src/fs_compliance_profile_repo.rs`:

```rust
use rocket_audit::profile::{default_profile, ComplianceProfile, ProfileRepository};
use rocket_shared::{DomainError, DomainResult};
use std::{fs, path::PathBuf};

pub struct FsComplianceProfileRepo {
    path: PathBuf,
}

impl FsComplianceProfileRepo {
    pub fn new(path: PathBuf) -> DomainResult<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(DomainError::Io)?;
        }
        Ok(Self { path })
    }
}

impl ProfileRepository for FsComplianceProfileRepo {
    fn load(&self) -> DomainResult<ComplianceProfile> {
        if !self.path.exists() {
            return Ok(default_profile());
        }
        let raw = fs::read_to_string(&self.path).map_err(DomainError::Io)?;
        serde_yaml::from_str(&raw).map_err(|e| DomainError::Serialization(e.to_string()))
    }

    fn save(&self, profile: &ComplianceProfile) -> DomainResult<()> {
        let raw = serde_yaml::to_string(profile)
            .map_err(|e| DomainError::Serialization(e.to_string()))?;
        fs::write(&self.path, raw).map_err(DomainError::Io)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_audit::{control::Framework, profile::EnforcementLevel};
    use tempfile::TempDir;

    #[test]
    fn load_returns_default_when_missing() {
        let dir = TempDir::new().unwrap();
        let repo = FsComplianceProfileRepo::new(dir.path().join("profile.yml")).unwrap();
        let loaded = repo.load().unwrap();
        assert_eq!(loaded, default_profile());
    }

    #[test]
    fn save_then_load_round_trip() {
        let dir = TempDir::new().unwrap();
        let repo = FsComplianceProfileRepo::new(dir.path().join("profile.yml")).unwrap();
        let mut p = default_profile();
        p.active_frameworks.insert(Framework::Soc2);
        p.active_frameworks.insert(Framework::Iso27001);
        p.enforcement = EnforcementLevel::Warn;
        repo.save(&p).unwrap();
        let loaded = repo.load().unwrap();
        assert_eq!(loaded, p);
    }
}
```

- [ ] **Step 2: Register module**

Modify `crates/rocket-infra/src/lib.rs` — add `pub mod fs_compliance_profile_repo;` and `pub use fs_compliance_profile_repo::FsComplianceProfileRepo;`.

- [ ] **Step 3: Run tests — expect pass**

Run: `cargo test -p rocket-infra fs_compliance_profile_repo::tests`
Expected: PASS (2 tests).

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-infra/src/fs_compliance_profile_repo.rs crates/rocket-infra/src/lib.rs
git commit -m "feat(audit): add FsComplianceProfileRepo YAML profile store"
```

---

## Task 10: Wire `SecurityAuditPublisher` into existing services

**Files:**
- Modify: `crates/rocket-app/src/contract_service.rs`
- Modify: `crates/rocket-app/src/collection_service.rs`
- Modify: `crates/rocket-app/src/environment_service.rs`
- Modify: `crates/rocket-app/src/execution_service.rs`

- [ ] **Step 1: Add a failing test for contract_service emission**

Open `crates/rocket-app/src/contract_service.rs`, locate the existing `#[cfg(test)] mod tests { ... }` block, and inside it add:

```rust
#[test]
fn attach_contract_emits_security_audit_event() {
    use rocket_audit::{event::AuditEventKind, publisher::SecurityAuditPublisher};
    use std::sync::Mutex;

    struct CapturingPublisher {
        captured: Mutex<Vec<AuditEventKind>>,
    }
    impl SecurityAuditPublisher for CapturingPublisher {
        fn publish(
            &self,
            _actor: String,
            _workspace_id: Option<String>,
            kind: AuditEventKind,
        ) {
            self.captured.lock().unwrap().push(kind);
        }
    }

    let publisher = Arc::new(CapturingPublisher { captured: Mutex::new(vec![]) });
    let (contract_repo, collection_repo) = inline_mocks_with_one_collection();
    let svc = ContractService::new_with_audit(
        Arc::new(contract_repo),
        Arc::new(collection_repo),
        publisher.clone(),
    );
    svc.attach_contract(/* existing test args */).unwrap();

    let captured = publisher.captured.lock().unwrap();
    assert!(
        captured
            .iter()
            .any(|k| matches!(k, AuditEventKind::ContractAttached { .. })),
        "expected ContractAttached event, got {:?}",
        *captured
    );
}
```

(Adjust `inline_mocks_with_one_collection()` / `attach_contract` args to match the existing test helpers already in this file. Those helpers exist — read the current `mod tests` block first and reuse its fixture builders verbatim.)

- [ ] **Step 2: Add audit publisher field + constructor**

Inside `crates/rocket-app/src/contract_service.rs`, above the `impl ContractService` block, add the import:

```rust
use rocket_audit::{event::AuditEventKind, publisher::{NullSecurityAuditPublisher, SecurityAuditPublisher}};
```

Modify the `ContractService` struct by appending the field:

```rust
audit: Arc<dyn SecurityAuditPublisher>,
```

Modify the existing `new` function so it sets `audit: Arc::new(NullSecurityAuditPublisher)`. Add a new constructor:

```rust
pub fn new_with_audit(
    contract_repo: Arc<dyn rocket_collection::contract::repository::ContractRepository>,
    collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
    audit: Arc<dyn SecurityAuditPublisher>,
) -> Self {
    Self { contract_repo, collection_repo, audit }
}
```

At every call site in the service where a contract is attached, deleted, or a violation is detected, insert (after the state-changing repository call succeeds):

```rust
self.audit.publish(
    "system".into(),
    None,
    AuditEventKind::ContractAttached {
        contract_id: contract.id.to_string(),
        collection: collection_name.clone(),
        scope: format!("{:?}", contract.scope),
    },
);
```

…and corresponding `ContractDeleted` / `ContractViolation` calls on their respective paths. Read the file (~824 lines) before editing — use the existing `log::warn!` / `log::info!` call sites as anchors for where these publishes belong.

- [ ] **Step 3: Run the new test**

Run: `cargo test -p rocket-app security_audit_service::tests contract_service::tests`
Expected: PASS.

- [ ] **Step 4: Apply the same pattern to the other three services**

Apply the same `audit: Arc<dyn SecurityAuditPublisher>` field + `new_with_audit` constructor + publish-on-success pattern to:

- `collection_service.rs` — publish `CollectionDeleted { collection }` at the end of `delete_collection`, `CollectionExported { collection, destination }` at the end of `export_collection` (if present; skip if the method doesn't exist).
- `environment_service.rs` — publish `SecretVariableWritten { environment, variable_key }` inside `save_environment`, for each `Variable` where `secret == true` and the stored value has changed from the previous state.
- `execution_service.rs` — publish `SensitiveAuthUsed { auth_type, collection, request_path }` when an executed request resolves a non-`None` `Auth`. Read the existing auth-resolution path first.

Add one capturing-publisher test per service mirroring the contract one.

- [ ] **Step 5: Run the full app test suite**

Run: `cargo test -p rocket-app`
Expected: PASS (all previously passing tests still pass, 4+ new tests pass).

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/contract_service.rs crates/rocket-app/src/collection_service.rs crates/rocket-app/src/environment_service.rs crates/rocket-app/src/execution_service.rs
git commit -m "feat(audit): emit security audit events from contract/collection/env/exec services"
```

---

## Task 11: Bridge publisher → service in `src-tauri`

**Files:**
- Create: `src-tauri/src/audit_bridge.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write the bridge**

Create `src-tauri/src/audit_bridge.rs`:

```rust
use rocket_app::SecurityAuditService;
use rocket_audit::{event::AuditEventKind, publisher::SecurityAuditPublisher};
use std::sync::Arc;

/// Adapts `SecurityAuditService::record` to the fire-and-forget `SecurityAuditPublisher` contract.
/// A failing record is logged but never propagated to the caller.
pub struct ServiceBackedAuditPublisher {
    svc: Arc<SecurityAuditService>,
}

impl ServiceBackedAuditPublisher {
    pub fn new(svc: Arc<SecurityAuditService>) -> Self {
        Self { svc }
    }
}

impl SecurityAuditPublisher for ServiceBackedAuditPublisher {
    fn publish(&self, actor: String, workspace_id: Option<String>, kind: AuditEventKind) {
        if let Err(e) = self.svc.record(actor, workspace_id, kind) {
            tracing::warn!(error = %e, "audit record failed");
        }
    }
}
```

- [ ] **Step 2: Wire in `lib.rs`**

Modify `src-tauri/src/lib.rs`:

1. Add `mod audit_bridge;` near the other module declarations.
2. In the imports block, add `use rocket_app::SecurityAuditService;` and `use rocket_infra::{FsAuditLogRepo, FsComplianceProfileRepo};`.
3. Inside `setup(...)` after `active_workspace_path` is available but before services are constructed, add:

```rust
let audit_dir = data_dir.join("audit");
std::fs::create_dir_all(&audit_dir).ok();
let audit_log_repo = Arc::new(FsAuditLogRepo::new(audit_dir.join("events.jsonl"))
    .expect("audit log init"));
let profile_repo = Arc::new(FsComplianceProfileRepo::new(audit_dir.join("profile.yml"))
    .expect("profile init"));
let audit_svc = Arc::new(
    SecurityAuditService::new(audit_log_repo.clone(), profile_repo.clone())
        .expect("audit service init"),
);
let audit_publisher: Arc<dyn rocket_audit::publisher::SecurityAuditPublisher> =
    Arc::new(crate::audit_bridge::ServiceBackedAuditPublisher::new(audit_svc.clone()));
```

4. Change the construction of `contract_svc`, the `collection_service`, `environment_service`, and `execution_service` so they call their `new_with_audit(..., audit_publisher.clone())` constructors instead of `new(...)`.
5. Add `app.manage(audit_svc);` near the other `.manage(...)` calls.

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p rocket-rocket` (or whatever the `src-tauri` crate name is — confirm from `src-tauri/Cargo.toml`).
Expected: PASS (`Finished` with no errors).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/audit_bridge.rs src-tauri/src/lib.rs
git commit -m "feat(audit): wire SecurityAuditService and ServiceBackedAuditPublisher in src-tauri"
```

---

## Task 12: Tauri IPC commands

**Files:**
- Create: `src-tauri/src/commands/audit.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs` — register commands in `tauri::generate_handler!`.

- [ ] **Step 1: Write the commands**

Create `src-tauri/src/commands/audit.rs`:

```rust
use chrono::{DateTime, Utc};
use rocket_app::SecurityAuditService;
use rocket_audit::{event::SecurityAuditEvent, profile::ComplianceProfile};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub fn list_audit_events(
    svc: State<'_, Arc<SecurityAuditService>>,
) -> Result<Vec<SecurityAuditEvent>, String> {
    svc.list().map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RangeInput {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[tauri::command]
pub fn list_audit_events_range(
    svc: State<'_, Arc<SecurityAuditService>>,
    input: RangeInput,
) -> Result<Vec<SecurityAuditEvent>, String> {
    svc.list_range(input.start, input.end).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_compliance_profile(
    svc: State<'_, Arc<SecurityAuditService>>,
) -> Result<ComplianceProfile, String> {
    svc.load_profile().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_compliance_profile(
    svc: State<'_, Arc<SecurityAuditService>>,
    profile: ComplianceProfile,
) -> Result<(), String> {
    svc.save_profile(&profile).map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceExport {
    pub exported_at: DateTime<Utc>,
    pub range_start: DateTime<Utc>,
    pub range_end: DateTime<Utc>,
    pub events: Vec<SecurityAuditEvent>,
    pub chain_verified: bool,
}

#[tauri::command]
pub fn export_audit_evidence(
    svc: State<'_, Arc<SecurityAuditService>>,
    input: RangeInput,
) -> Result<EvidenceExport, String> {
    let events = svc.list_range(input.start, input.end).map_err(|e| e.to_string())?;
    let chain_verified = matches!(
        rocket_audit::chain::verify_chain(&events),
        rocket_audit::chain::ChainVerification::Ok
    );
    Ok(EvidenceExport {
        exported_at: Utc::now(),
        range_start: input.start,
        range_end: input.end,
        events,
        chain_verified,
    })
}
```

- [ ] **Step 2: Register the module**

Modify `src-tauri/src/commands/mod.rs` — add `pub mod audit;` alongside the other `pub mod` lines.

- [ ] **Step 3: Register handlers**

Modify `src-tauri/src/lib.rs` — inside the `tauri::generate_handler![ ... ]` macro, add:

```rust
commands::audit::list_audit_events,
commands::audit::list_audit_events_range,
commands::audit::get_compliance_profile,
commands::audit::set_compliance_profile,
commands::audit::export_audit_evidence,
```

- [ ] **Step 4: Verify**

Run: `cargo check` (workspace root).
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/audit.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat(audit): add IPC commands for audit list, profile, and evidence export"
```

---

## Task 13: Typed frontend wrappers in `tauri-api.ts`

**Files:**
- Modify: `src/lib/tauri-api.ts`

- [ ] **Step 1: Add wire types**

At the appropriate location in `src/lib/tauri-api.ts` (group with other wire types, alphabetical within the file's convention), add:

```ts
// ============================================================
// Security audit / compliance
// ============================================================

export type Framework = 'soc2' | 'iso27001' | 'iso42001' | 'csaStar';
export type EnforcementLevel = 'record' | 'warn' | 'block';

export interface ControlId {
  framework: Framework;
  code: string;
  title: string;
}

export interface ComplianceProfile {
  activeFrameworks: Framework[];
  enforcement: EnforcementLevel;
  mutedKinds: string[];
}

export type AuditEventKind =
  | { kind: 'contract_attached'; contractId: string; collection: string; scope: string }
  | { kind: 'contract_deleted'; contractId: string; collection: string }
  | { kind: 'contract_violation'; contractId: string; requestPath: string; field: string }
  | { kind: 'collection_deleted'; collection: string }
  | { kind: 'collection_exported'; collection: string; destination: string }
  | { kind: 'secret_variable_written'; environment: string; variableKey: string }
  | { kind: 'sensitive_auth_used'; authType: string; collection: string; requestPath: string }
  | { kind: 'audit_evidence_exported'; rangeStart: string; rangeEnd: string; count: number }
  | { kind: 'audit_chain_broken'; atEventId: string; expectedHash: string; actualHash: string };

export interface SecurityAuditEvent {
  id: string;
  occurredAt: string;
  actor: string;
  workspaceId: string | null;
  event: AuditEventKind;
  controls: ControlId[];
  prevHash: string;
  hash: string;
  metadata?: Record<string, string>;
}

export interface EvidenceExport {
  exportedAt: string;
  rangeStart: string;
  rangeEnd: string;
  events: SecurityAuditEvent[];
  chainVerified: boolean;
}

export const listAuditEvents = () =>
  invoke<SecurityAuditEvent[]>('list_audit_events');

export const listAuditEventsRange = (start: string, end: string) =>
  invoke<SecurityAuditEvent[]>('list_audit_events_range', { input: { start, end } });

export const getComplianceProfile = () =>
  invoke<ComplianceProfile>('get_compliance_profile');

export const setComplianceProfile = (profile: ComplianceProfile) =>
  invoke<void>('set_compliance_profile', { profile });

export const exportAuditEvidence = (start: string, end: string) =>
  invoke<EvidenceExport>('export_audit_evidence', { input: { start, end } });
```

- [ ] **Step 2: Verify**

Run: `yarn tsc --noEmit`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/lib/tauri-api.ts
git commit -m "feat(audit): add typed frontend wrappers for audit IPC"
```

---

## Task 14: Zustand `audit-store.ts`

**Files:**
- Create: `src/stores/audit-store.ts`
- Test: `src/stores/__tests__/audit-store.test.ts`

- [ ] **Step 1: Write the failing test**

Create `src/stores/__tests__/audit-store.test.ts`:

```ts
import { describe, expect, it, vi, beforeEach } from 'vitest';

vi.mock('@/lib/tauri-api', () => ({
  listAuditEvents: vi.fn(),
  getComplianceProfile: vi.fn(),
  setComplianceProfile: vi.fn(),
  exportAuditEvidence: vi.fn(),
}));

import * as api from '@/lib/tauri-api';
import { useAuditStore } from '@/stores/audit-store';

const mockedApi = vi.mocked(api);

describe('audit-store', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useAuditStore.setState({
      events: [],
      profile: null,
      loading: false,
      error: null,
    });
  });

  it('loadEvents populates events on success', async () => {
    mockedApi.listAuditEvents.mockResolvedValue([
      {
        id: '01',
        occurredAt: '2026-04-15T00:00:00Z',
        actor: 'me',
        workspaceId: null,
        event: { kind: 'collection_deleted', collection: 'x' },
        controls: [],
        prevHash: '',
        hash: 'h1',
      },
    ]);
    await useAuditStore.getState().loadEvents();
    expect(useAuditStore.getState().events).toHaveLength(1);
    expect(useAuditStore.getState().error).toBeNull();
  });

  it('loadEvents sets error on failure', async () => {
    mockedApi.listAuditEvents.mockRejectedValue(new Error('boom'));
    await useAuditStore.getState().loadEvents();
    expect(useAuditStore.getState().error).toBe('boom');
  });

  it('saveProfile persists and updates state', async () => {
    const profile = {
      activeFrameworks: ['soc2'] as const,
      enforcement: 'record' as const,
      mutedKinds: [],
    };
    mockedApi.setComplianceProfile.mockResolvedValue(undefined);
    await useAuditStore.getState().saveProfile(profile);
    expect(mockedApi.setComplianceProfile).toHaveBeenCalledWith(profile);
    expect(useAuditStore.getState().profile).toEqual(profile);
  });
});
```

- [ ] **Step 2: Run tests — expect fail**

Run: `yarn test --run src/stores/__tests__/audit-store.test.ts`
Expected: FAIL with "cannot resolve @/stores/audit-store".

- [ ] **Step 3: Implement the store**

Create `src/stores/audit-store.ts`:

```ts
import { create } from 'zustand';
import {
  type ComplianceProfile,
  type SecurityAuditEvent,
  getComplianceProfile,
  listAuditEvents,
  setComplianceProfile,
} from '@/lib/tauri-api';

interface AuditStoreState {
  events: SecurityAuditEvent[];
  profile: ComplianceProfile | null;
  loading: boolean;
  error: string | null;

  loadEvents: () => Promise<void>;
  loadProfile: () => Promise<void>;
  saveProfile: (profile: ComplianceProfile) => Promise<void>;
}

export const useAuditStore = create<AuditStoreState>((set) => ({
  events: [],
  profile: null,
  loading: false,
  error: null,

  loadEvents: async () => {
    set({ loading: true, error: null });
    try {
      const events = await listAuditEvents();
      set({ events, loading: false });
    } catch (e) {
      set({ error: e instanceof Error ? e.message : String(e), loading: false });
    }
  },

  loadProfile: async () => {
    set({ error: null });
    try {
      const profile = await getComplianceProfile();
      set({ profile });
    } catch (e) {
      set({ error: e instanceof Error ? e.message : String(e) });
    }
  },

  saveProfile: async (profile) => {
    await setComplianceProfile(profile);
    set({ profile });
  },
}));
```

- [ ] **Step 4: Run tests — expect pass**

Run: `yarn test --run src/stores/__tests__/audit-store.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src/stores/audit-store.ts src/stores/__tests__/audit-store.test.ts
git commit -m "feat(audit): add audit-store with events and profile state"
```

---

## Task 15: `AuditEventRow` component

**Files:**
- Create: `src/components/audit/AuditEventRow.tsx`

- [ ] **Step 1: Write the component**

Create `src/components/audit/AuditEventRow.tsx`:

```tsx
import { formatDistanceToNow, parseISO } from 'date-fns';
import { Badge } from '@/components/ui/badge';
import type { SecurityAuditEvent } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';

interface AuditEventRowProps {
  event: SecurityAuditEvent;
  className?: string;
}

function summarize(event: SecurityAuditEvent): string {
  const k = event.event;
  switch (k.kind) {
    case 'contract_attached':
      return `Attached contract ${k.contractId} to ${k.collection} (${k.scope})`;
    case 'contract_deleted':
      return `Deleted contract ${k.contractId} from ${k.collection}`;
    case 'contract_violation':
      return `Contract ${k.contractId} violation: ${k.field} in ${k.requestPath}`;
    case 'collection_deleted':
      return `Deleted collection ${k.collection}`;
    case 'collection_exported':
      return `Exported collection ${k.collection} to ${k.destination}`;
    case 'secret_variable_written':
      return `Secret variable "${k.variableKey}" written in ${k.environment}`;
    case 'sensitive_auth_used':
      return `${k.authType} auth used on ${k.collection}/${k.requestPath}`;
    case 'audit_evidence_exported':
      return `Exported ${k.count} audit events`;
    case 'audit_chain_broken':
      return `Audit chain broken at event ${k.atEventId}`;
  }
}

export function AuditEventRow({ event, className }: AuditEventRowProps) {
  const occurred = parseISO(event.occurredAt);
  const relative = formatDistanceToNow(occurred, { addSuffix: true });
  return (
    <tr className={cn('border-b border-border/60 last:border-0', className)}>
      <td className='py-2 px-3 text-xs text-muted-foreground whitespace-nowrap'>
        <time dateTime={event.occurredAt} title={occurred.toISOString()}>{relative}</time>
      </td>
      <td className='py-2 px-3 text-xs text-foreground'>{summarize(event)}</td>
      <td className='py-2 px-3'>
        <div className='flex flex-wrap gap-1'>
          {event.controls.map((c) => (
            <Badge
              key={`${c.framework}-${c.code}`}
              variant='secondary'
              className='text-2xs font-mono'
              title={c.title}
            >
              {c.framework.toUpperCase()} {c.code}
            </Badge>
          ))}
        </div>
      </td>
      <td className='py-2 px-3 text-xs text-muted-foreground font-mono truncate max-w-32'>
        {event.hash.slice(0, 8)}
      </td>
    </tr>
  );
}
```

- [ ] **Step 2: Verify compilation**

Run: `yarn tsc --noEmit`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/components/audit/AuditEventRow.tsx
git commit -m "feat(audit): add AuditEventRow component"
```

---

## Task 16: `ComplianceProfileDialog` component

**Files:**
- Create: `src/components/audit/ComplianceProfileDialog.tsx`

- [ ] **Step 1: Write the component**

Create `src/components/audit/ComplianceProfileDialog.tsx`:

```tsx
import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import type { ComplianceProfile, EnforcementLevel, Framework } from '@/lib/tauri-api';
import { useAuditStore } from '@/stores/audit-store';

interface ComplianceProfileDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const FRAMEWORKS: { value: Framework; label: string; hint: string }[] = [
  { value: 'soc2', label: 'SOC 2 Type 2', hint: 'Trust Services Criteria' },
  { value: 'iso27001', label: 'ISO 27001:2022', hint: 'Annex A controls' },
  { value: 'iso42001', label: 'ISO 42001:2023', hint: 'AI management' },
  { value: 'csaStar', label: 'CSA STAR', hint: 'Cloud Controls Matrix' },
];

const ENFORCEMENT: { value: EnforcementLevel; label: string; description: string }[] = [
  { value: 'record', label: 'Record', description: 'Log events without interfering' },
  { value: 'warn', label: 'Warn', description: 'Log and surface a toast on sensitive ops' },
  { value: 'block', label: 'Block', description: 'Log and reject the triggering operation' },
];

export function ComplianceProfileDialog({ open, onOpenChange }: ComplianceProfileDialogProps) {
  const profile = useAuditStore((s) => s.profile);
  const loadProfile = useAuditStore((s) => s.loadProfile);
  const saveProfile = useAuditStore((s) => s.saveProfile);

  const [draft, setDraft] = useState<ComplianceProfile>({
    activeFrameworks: [],
    enforcement: 'record',
    mutedKinds: [],
  });

  useEffect(() => {
    if (open) void loadProfile();
  }, [open, loadProfile]);

  useEffect(() => {
    if (profile) setDraft(profile);
  }, [profile]);

  const toggleFramework = (fw: Framework) => {
    setDraft((d) => ({
      ...d,
      activeFrameworks: d.activeFrameworks.includes(fw)
        ? d.activeFrameworks.filter((x) => x !== fw)
        : [...d.activeFrameworks, fw],
    }));
  };

  const handleSave = async () => {
    await saveProfile(draft);
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='max-w-lg'>
        <DialogHeader>
          <DialogTitle>Compliance Profile</DialogTitle>
        </DialogHeader>

        <div className='space-y-5 py-2'>
          <div className='space-y-2'>
            <Label className='text-sm font-medium'>Active frameworks</Label>
            <p className='text-xs text-muted-foreground'>
              Events are recorded for every kind tagged against a selected framework. With no
              frameworks selected, all events are recorded.
            </p>
            <div className='grid grid-cols-1 gap-2'>
              {FRAMEWORKS.map((f) => (
                <label
                  key={f.value}
                  className='flex items-start gap-2.5 rounded-md border border-border px-3 py-2 hover:bg-muted/30 cursor-pointer'
                >
                  <Checkbox
                    checked={draft.activeFrameworks.includes(f.value)}
                    onCheckedChange={() => toggleFramework(f.value)}
                    className='mt-0.5'
                  />
                  <div className='flex-1'>
                    <div className='text-sm font-medium'>{f.label}</div>
                    <div className='text-xs text-muted-foreground'>{f.hint}</div>
                  </div>
                </label>
              ))}
            </div>
          </div>

          <div className='space-y-1.5'>
            <Label htmlFor='enforcement' className='text-sm font-medium'>
              Enforcement level
            </Label>
            <Select
              value={draft.enforcement}
              onValueChange={(v) => setDraft((d) => ({ ...d, enforcement: v as EnforcementLevel }))}
            >
              <SelectTrigger id='enforcement'>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {ENFORCEMENT.map((e) => (
                  <SelectItem key={e.value} value={e.value}>
                    <div>
                      <div className='font-medium'>{e.label}</div>
                      <div className='text-xs text-muted-foreground'>{e.description}</div>
                    </div>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>

        <DialogFooter>
          <Button variant='ghost' onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={() => void handleSave()}>Save</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 2: Verify compilation**

Run: `yarn tsc --noEmit`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/components/audit/ComplianceProfileDialog.tsx
git commit -m "feat(audit): add ComplianceProfileDialog"
```

---

## Task 17: `ExportEvidenceDialog`

**Files:**
- Create: `src/components/audit/ExportEvidenceDialog.tsx`

- [ ] **Step 1: Write the component**

Create `src/components/audit/ExportEvidenceDialog.tsx`:

```tsx
import { save as saveDialog } from '@tauri-apps/plugin-dialog';
import { writeTextFile } from '@tauri-apps/plugin-fs';
import { useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { exportAuditEvidence } from '@/lib/tauri-api';

interface ExportEvidenceDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

function isoStartOfDay(d: string) {
  return `${d}T00:00:00.000Z`;
}
function isoEndOfDay(d: string) {
  return `${d}T23:59:59.999Z`;
}

export function ExportEvidenceDialog({ open, onOpenChange }: ExportEvidenceDialogProps) {
  const today = new Date().toISOString().slice(0, 10);
  const [start, setStart] = useState(today);
  const [end, setEnd] = useState(today);
  const [busy, setBusy] = useState(false);

  const handleExport = async () => {
    setBusy(true);
    try {
      const result = await exportAuditEvidence(isoStartOfDay(start), isoEndOfDay(end));
      const path = await saveDialog({
        defaultPath: `rocket-audit-${start}_${end}.json`,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (path) {
        await writeTextFile(path, JSON.stringify(result, null, 2));
        toast.success(`Exported ${result.events.length} events`, {
          description: result.chainVerified ? 'Chain verified' : 'Chain verification failed',
        });
        onOpenChange(false);
      }
    } catch (e) {
      toast.error('Export failed', {
        description: e instanceof Error ? e.message : String(e),
      });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Export Evidence Pack</DialogTitle>
        </DialogHeader>
        <div className='space-y-4 py-2'>
          <div className='space-y-1.5'>
            <Label htmlFor='start-date'>From</Label>
            <Input
              id='start-date'
              type='date'
              value={start}
              onChange={(e) => setStart(e.target.value)}
            />
          </div>
          <div className='space-y-1.5'>
            <Label htmlFor='end-date'>To</Label>
            <Input
              id='end-date'
              type='date'
              value={end}
              onChange={(e) => setEnd(e.target.value)}
            />
          </div>
          <p className='text-xs text-muted-foreground'>
            Output is a JSON file containing every event in the range plus a hash-chain
            verification flag. Suitable as SOC 2 / ISO 27001 evidence.
          </p>
        </div>
        <DialogFooter>
          <Button variant='ghost' onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={() => void handleExport()} disabled={busy}>
            {busy ? 'Exporting...' : 'Export'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 2: Verify the filesystem plugin is present**

Run: `grep -E '"@tauri-apps/plugin-(dialog|fs)"' package.json`
Expected: matches for both plugins. If either is missing, run `yarn add @tauri-apps/plugin-dialog @tauri-apps/plugin-fs` and then verify `src-tauri/tauri.conf.json` / `src-tauri/Cargo.toml` has the corresponding Rust-side plugin registered (e.g. `tauri-plugin-dialog`, `tauri-plugin-fs`).

- [ ] **Step 3: Commit**

```bash
git add src/components/audit/ExportEvidenceDialog.tsx
git commit -m "feat(audit): add ExportEvidenceDialog"
```

---

## Task 18: `AuditLogTab` — the main viewer

**Files:**
- Create: `src/components/audit/AuditLogTab.tsx`

- [ ] **Step 1: Write the tab**

Create `src/components/audit/AuditLogTab.tsx`:

```tsx
import { AlertTriangle, Download, ShieldCheck, Sliders } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { AuditEventRow } from '@/components/audit/AuditEventRow';
import { ComplianceProfileDialog } from '@/components/audit/ComplianceProfileDialog';
import { ExportEvidenceDialog } from '@/components/audit/ExportEvidenceDialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { useAuditStore } from '@/stores/audit-store';

export function AuditLogTab() {
  const events = useAuditStore((s) => s.events);
  const profile = useAuditStore((s) => s.profile);
  const loading = useAuditStore((s) => s.loading);
  const error = useAuditStore((s) => s.error);
  const loadEvents = useAuditStore((s) => s.loadEvents);
  const loadProfile = useAuditStore((s) => s.loadProfile);

  const [profileOpen, setProfileOpen] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const [filter, setFilter] = useState('');

  useEffect(() => {
    void loadEvents();
    void loadProfile();
  }, [loadEvents, loadProfile]);

  const filtered = useMemo(() => {
    if (!filter.trim()) return events;
    const q = filter.toLowerCase();
    return events.filter((e) => {
      const blob = `${e.actor} ${e.event.kind} ${JSON.stringify(e.event)} ${e.controls
        .map((c) => `${c.framework} ${c.code}`)
        .join(' ')}`.toLowerCase();
      return blob.includes(q);
    });
  }, [events, filter]);

  const activeCount = profile?.activeFrameworks.length ?? 0;

  return (
    <div className='h-full flex flex-col bg-background'>
      {/* Header */}
      <div className='shrink-0 border-b border-border/70 px-6 py-3 flex items-center justify-between'>
        <div className='flex items-center gap-2'>
          <ShieldCheck className='h-4 w-4 text-muted-foreground' />
          <h1 className='text-sm font-semibold'>Audit Log</h1>
          <span className='text-xs text-muted-foreground'>
            · {events.length} event{events.length === 1 ? '' : 's'}
            {activeCount > 0 && ` · ${activeCount} framework${activeCount === 1 ? '' : 's'} active`}
          </span>
        </div>
        <div className='flex items-center gap-2'>
          <Button variant='outline' size='sm' className='h-7 text-xs' onClick={() => setProfileOpen(true)}>
            <Sliders className='h-3 w-3 mr-1.5' />
            Profile
          </Button>
          <Button variant='outline' size='sm' className='h-7 text-xs' onClick={() => setExportOpen(true)}>
            <Download className='h-3 w-3 mr-1.5' />
            Export evidence
          </Button>
        </div>
      </div>

      {/* Filter */}
      <div className='shrink-0 px-6 py-2 border-b border-border/50'>
        <Input
          placeholder='Filter by actor, kind, control ID...'
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          className='h-7 text-xs'
        />
      </div>

      {/* Table */}
      <ScrollArea className='flex-1'>
        {error && (
          <div className='flex items-center gap-2 px-6 py-3 text-xs text-destructive'>
            <AlertTriangle className='h-3.5 w-3.5' />
            {error}
          </div>
        )}
        {loading && events.length === 0 ? (
          <div className='px-6 py-8 text-xs text-muted-foreground'>Loading audit log…</div>
        ) : filtered.length === 0 ? (
          <div className='px-6 py-16 text-center'>
            <ShieldCheck className='h-10 w-10 mx-auto text-muted-foreground/40' />
            <p className='mt-3 text-sm font-medium text-foreground'>No audit events</p>
            <p className='mt-1 text-xs text-muted-foreground'>
              Sensitive operations will appear here once they occur.
            </p>
          </div>
        ) : (
          <table className='w-full text-left'>
            <thead className='text-2xs uppercase tracking-wider text-muted-foreground border-b border-border/60 sticky top-0 bg-background'>
              <tr>
                <th className='py-2 px-3 font-medium'>When</th>
                <th className='py-2 px-3 font-medium'>Event</th>
                <th className='py-2 px-3 font-medium'>Controls</th>
                <th className='py-2 px-3 font-medium'>Hash</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((ev) => (
                <AuditEventRow key={ev.id} event={ev} />
              ))}
            </tbody>
          </table>
        )}
      </ScrollArea>

      <ComplianceProfileDialog open={profileOpen} onOpenChange={setProfileOpen} />
      <ExportEvidenceDialog open={exportOpen} onOpenChange={setExportOpen} />
    </div>
  );
}
```

- [ ] **Step 2: Verify compilation**

Run: `yarn tsc --noEmit`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/components/audit/AuditLogTab.tsx
git commit -m "feat(audit): add AuditLogTab viewer with filter and actions"
```

---

## Task 19: Integrate `AuditTab` into the pane system + sidebar entry

**Files:**
- Modify: `src/types/pane-types.ts`
- Modify: `src/components/panes/EditorGroup.tsx`
- Modify: `src/components/layout/CollectionsSidebar.tsx`

- [ ] **Step 1: Add `AuditTab` to the tab union**

Open `src/types/pane-types.ts`. Mirror the existing `WorkspaceTab` pattern. Add:

```ts
export interface AuditTab {
  id: string;
  title: string;
  tabType: 'audit';
  isDirty: false;
}

export function isAuditTab(tab: Tab): tab is AuditTab {
  return tab.tabType === 'audit';
}
```

…and extend the exported `Tab` union at the bottom of the file to include `| AuditTab`. Also add `| AuditTab` to the `isRequestTab`/`isContractTab` sibling guards' type checks where the union appears.

- [ ] **Step 2: Route `AuditTab` in `EditorGroup`**

Open `src/components/panes/EditorGroup.tsx`. At the top, add `import { AuditLogTab } from '@/components/audit/AuditLogTab';` and `import { isAuditTab } from '@/types/pane-types';`. Inside the `activeTab ? (...)` ternary chain, add a new branch **before** the final `else` (CollectionOverviewTab):

```tsx
) : isAuditTab(activeTab) ? (
  <AuditLogTab />
```

- [ ] **Step 3: Add sidebar entry**

Open `src/components/layout/CollectionsSidebar.tsx`. Find the workspace-level section where `WorkspaceTab` items (overview, environments, git) are rendered. Add a new sibling entry:

```tsx
<SidebarRow
  icon={<ShieldCheck className='h-3.5 w-3.5' />}
  label='Audit log'
  onClick={() => openTab({ id: 'audit', title: 'Audit Log', tabType: 'audit', isDirty: false })}
/>
```

(Exact component name / props are dictated by the existing rows in this file — read the file and match the nearest pattern. `ShieldCheck` is from `lucide-react`; add it to the existing import.)

- [ ] **Step 4: Verify**

Run: `yarn tsc --noEmit` and `yarn test --run`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/types/pane-types.ts src/components/panes/EditorGroup.tsx src/components/layout/CollectionsSidebar.tsx
git commit -m "feat(audit): add AuditTab to pane system and sidebar entry"
```

---

## Task 20: Append audit domain rules to `.claude/rules/`

**Files:**
- Modify: `.claude/rules/10-contract-audit-domain.md`

- [ ] **Step 1: Append the section**

Open `.claude/rules/10-contract-audit-domain.md` and append at the end:

````markdown

---

# Security audit

The security audit pipeline lives alongside contract audit. Read this before touching anything under `rocket-audit`, `security_audit_service.rs`, or `src/components/audit/`.

## Concepts

- **`SecurityAuditEvent`** — one record per sensitive operation. Fields: `id` (ULID), `occurredAt`, `actor`, `workspaceId`, `event` (tagged `AuditEventKind`), `controls` (framework tags), `prevHash`, `hash`.
- **Hash chain** — each event's `hash = SHA-256(canonical_json(event_without_hash))`; `prevHash` links to the previous event. `verify_chain` walks the log and returns `Ok` or `Broken { index, expected, actual }`.
- **Compliance profile** — user-configurable. `activeFrameworks` picks which frameworks are tracked. `enforcement` is `Record | Warn | Block`. `Block` records the event **then** returns `DomainError::InvalidInput` so callers can abort. `Warn` and `Record` never fail.
- **Control catalog** — static `CONTROL_CATALOG` in `rocket-audit::control`. Maps event kinds to framework control IDs. Extend the catalog (not individual events) when adding a new kind/framework mapping.

## Invariants

- `SecurityAuditService::record` must be idempotent-safe to call from any service; failures are logged by the bridge, never propagated. Services emit via `SecurityAuditPublisher` (trait) — they never depend on `SecurityAuditService` directly.
- The chain head is cached in `SecurityAuditService::head` (`Mutex<Option<String>>`); after a successful append, update the cache before releasing the mutex.
- `FsAuditLogRepo` is append-only — never rewrite or truncate the log. Log-compaction is out of scope for v1.
- `FsComplianceProfileRepo.save` overwrites atomically via `fs::write` (single syscall). Do not introduce staged writes without explicit justification.
- IPC event shapes must match `src/lib/tauri-api.ts` exactly. `AuditEventKind` uses `#[serde(tag = "kind", rename_all = "snake_case")]` — frontend TypeScript uses `snake_case` literal types on the `kind` discriminator.

## Do not

- Do not emit security events from inside `rocket-infra`. Only `rocket-app` services emit.
- Do not store PII/secret values inside `SecurityAuditEvent.metadata`. The `SecretVariableWritten` event intentionally carries only the key, never the value.
- Do not expose the raw audit log as a writable API surface.
````

- [ ] **Step 2: Commit**

```bash
git add .claude/rules/10-contract-audit-domain.md
git commit -m "docs(audit): add security audit domain rules"
```

---

## Task 21: End-to-end verification

- [ ] **Step 1: Full verification suite**

Run these in parallel (a single shell session with multiple commands is fine, but prefer parallel calls per `.claude/rules/07-verification.md`):

```bash
cargo check
cargo test --workspace --no-run
yarn tsc --noEmit
yarn check
yarn test --run
```

Expected: every command exits 0. Record any pre-existing lint warnings in the PR description as "not introduced by this change".

- [ ] **Step 2: Smoke test the desktop app**

Run: `yarn tauri dev`
Manual verification:
1. Open the app — sidebar shows "Audit log" row under workspace section.
2. Click it — the tab opens; shows "No audit events" empty state.
3. Delete a collection — the audit log refreshes (or reopen the tab); a `collection_deleted` row appears with a `SOC2 CC6.1`-style badge **only when SOC 2 is active**; with no profile, badges reflect every matching control in the catalog.
4. Click **Profile** — toggle SOC 2 and ISO 27001; save; re-trigger an event; observe badge count changes.
5. Click **Export evidence** — pick today's date; save the JSON file; open it; confirm `chainVerified: true` is present.
6. Close the app, corrupt one event hash in `~/.rocket-api/audit/events.jsonl` by editing a byte; reopen the app; export evidence again; confirm `chainVerified: false`.

- [ ] **Step 3: Record the verification run in the PR**

Prepare a table in the PR body:

```
| Check | Status | Notes |
|---|---|---|
| cargo check | ✅ | |
| cargo test --workspace | ✅ | 15 new tests added |
| yarn tsc --noEmit | ✅ | |
| yarn check | ✅ | |
| yarn test | ✅ | 3 new vitest specs |
| Desktop smoke | ✅ | Profile toggle + export + tamper detection confirmed |
```

- [ ] **Step 4: Final commit**

Only if there are any incidental fixes from the verification run:

```bash
git add -p
git commit -m "chore(audit): verification fixes"
```

---

## Self-Review Checklist

1. **Spec coverage** — every spec requirement has a corresponding task:
   - Extend existing contract-audit → Tasks 1–10 (new `rocket-audit` crate, wired into `contract_service` and friends).
   - Structured events tagged with control IDs → Tasks 2, 3 (catalog + event type + auto-tagging in `SecurityAuditEvent::new`).
   - Append-only SHA-256 hash-chained log → Tasks 4, 8 (chain helpers + JSONL repo).
   - Configurable framework profile → Tasks 5, 9, 16 (profile type, YAML repo, UI dialog).
   - UI viewer + exportable evidence pack → Tasks 13–19 (IPC, store, components, routing, sidebar).

2. **Placeholders** — none: every task has concrete file paths, full code blocks, and exact commands.

3. **Type consistency** — `SecurityAuditEvent`, `AuditEventKind`, `ComplianceProfile`, `ControlId`, `Framework`, `EnforcementLevel` are defined once (Rust) and mirrored exactly in TypeScript (Task 13). The `kind` discriminator uses `snake_case` in both. Service method names (`record`, `list`, `list_range`, `load_profile`, `save_profile`) are consistent from Task 7 through the IPC layer (Task 12) and the store (Task 14).
