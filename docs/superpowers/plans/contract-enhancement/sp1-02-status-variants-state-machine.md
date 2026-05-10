# SP1-02 — Domain: New ContractStatus Variants + State Machine

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `Draft`, `Drift`, `Breach`, `InReview`, `Paused` variants to `ContractStatus`; add `is_breaking: bool` to `ChangelogEntry`; create the `state_machine.rs` pure transition function.

**Architecture:** Status is stored in YAML (not computed). `state_machine.rs` is a pure function — no I/O. `ChangelogEntry.is_breaking` defaults to `false` for backward compat. Existing `Active`/`Expired` YAML values are unchanged.

**Tech Stack:** Rust, serde

**Spec:** `docs/superpowers/specs/2026-05-07-contract-lock-enhancement-design.md` §SP1, §SP2

**Depends on:** SP1-01 merged.

---

> **⚠️ Worktree** — all commands run inside `.worktrees/contract-enhancement` on branch `feat/contract-lock-enhancement`.

## Task 1: Add new `ContractStatus` variants + `is_breaking` on `ChangelogEntry`

**Files:**
- Modify: `crates/rocket-collection/src/contract/types.rs`
- Modify: `crates/rocket-collection/src/contract/changelog.rs`

- [ ] **Step 1: Write failing tests**

Add to `#[cfg(test)]` in `types.rs`:

```rust
#[test]
fn new_status_variants_roundtrip() {
    for status in [
        ContractStatus::Draft,
        ContractStatus::Drift,
        ContractStatus::Breach,
        ContractStatus::InReview,
        ContractStatus::Paused,
    ] {
        let yaml = serde_yaml::to_string(&status).unwrap();
        let back: ContractStatus = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(status, back);
    }
}

#[test]
fn existing_status_values_unchanged() {
    // Regression: active and expired must keep their exact YAML values
    let active = serde_yaml::to_string(&ContractStatus::Active).unwrap();
    let expired = serde_yaml::to_string(&ContractStatus::Expired).unwrap();
    assert!(active.trim() == "active");
    assert!(expired.trim() == "expired");
}
```

Add to `#[cfg(test)]` in `changelog.rs`:

```rust
#[test]
fn changelog_entry_is_breaking_defaults_false() {
    let yaml = r#"
timestamp: "2026-05-07T10:00:00Z"
requestPath: requests/payments.yml
field: method
changeType: changed
oldValue: GET
newValue: POST
"#;
    let entry: ChangelogEntry = serde_yaml::from_str(yaml).unwrap();
    assert!(!entry.is_breaking, "is_breaking must default to false for old entries");
}
```

- [ ] **Step 2: Run — verify they fail**

```bash
cargo test -p rocket-collection contract 2>&1 | grep -E "FAILED|error" | head -10
```

Expected: compile errors for missing variants.

- [ ] **Step 3: Add new `ContractStatus` variants**

In `types.rs`, replace the `ContractStatus` enum with:

```rust
/// Lifecycle status of a contract. Stored explicitly in YAML.
///
/// Backward compat: `active` and `expired` retain their serialised values.
/// `expiring_in_30_days` is stored but also recomputed on load if expiry
/// is approaching, so the stored value may lag by one session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContractStatus {
    /// Not yet published — snapshot not taken.
    Draft,
    /// Healthy, in compliance.
    #[default]
    Active,
    /// Non-breaking changes detected since signing.
    Drift,
    /// Breaking changes detected — consumer build at risk.
    Breach,
    /// Sent for consumer sign-off (not yet approved).
    InReview,
    /// Monitoring suspended by the provider.
    Paused,
    /// Expiry date is within 30 days.
    ExpiringIn30Days,
    /// Past expiry date.
    Expired,
}
```

- [ ] **Step 4: Add `is_breaking` to `ChangelogEntry`**

In `changelog.rs`, add the field to `ChangelogEntry`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangelogEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub request_path: std::path::PathBuf,
    pub field: String,
    pub change_type: ChangeType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_value: Option<String>,
    /// True if this change violates the contract's breaking-change policy.
    /// Defaults to false so old changelog entries deserialise correctly.
    #[serde(default)]
    pub is_breaking: bool,
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p rocket-collection contract 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-collection/src/contract/types.rs
git add crates/rocket-collection/src/contract/changelog.rs
git commit -m "feat(contract): add Draft/Drift/Breach/InReview/Paused status variants; is_breaking on ChangelogEntry"
```

---

## Task 2: Create `state_machine.rs` — pure status transition function

**Files:**
- Create: `crates/rocket-collection/src/contract/state_machine.rs`
- Modify: `crates/rocket-collection/src/contract/mod.rs`

- [ ] **Step 1: Write failing tests first**

Create `crates/rocket-collection/src/contract/state_machine.rs` with only the test module initially:

```rust
use super::types::ContractStatus;

pub enum StatusEvent {
    Publish,
    DriftDetected,
    BreachDetected,
    Resign,
    MarkBreaking,
    Pause,
    Resume,
    SendForReview,
    Approve,
    Reject,
    Renew,
    ExpiryLapsed,
    ExpiringSoon,
}

#[derive(Debug, PartialEq)]
pub struct InvalidTransition {
    pub from: ContractStatus,
    pub event: String,
}

pub fn transition(
    current: &ContractStatus,
    event: &StatusEvent,
) -> Result<ContractStatus, InvalidTransition> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_publish_to_active() {
        let result = transition(&ContractStatus::Draft, &StatusEvent::Publish).unwrap();
        assert_eq!(result, ContractStatus::Active);
    }

    #[test]
    fn active_drift_detected() {
        let result = transition(&ContractStatus::Active, &StatusEvent::DriftDetected).unwrap();
        assert_eq!(result, ContractStatus::Drift);
    }

    #[test]
    fn active_breach_detected() {
        let result = transition(&ContractStatus::Active, &StatusEvent::BreachDetected).unwrap();
        assert_eq!(result, ContractStatus::Breach);
    }

    #[test]
    fn drift_resign_to_active() {
        let result = transition(&ContractStatus::Drift, &StatusEvent::Resign).unwrap();
        assert_eq!(result, ContractStatus::Active);
    }

    #[test]
    fn drift_mark_breaking_to_breach() {
        let result = transition(&ContractStatus::Drift, &StatusEvent::MarkBreaking).unwrap();
        assert_eq!(result, ContractStatus::Breach);
    }

    #[test]
    fn breach_resign_to_active() {
        let result = transition(&ContractStatus::Breach, &StatusEvent::Resign).unwrap();
        assert_eq!(result, ContractStatus::Active);
    }

    #[test]
    fn paused_resume_to_active() {
        let result = transition(&ContractStatus::Paused, &StatusEvent::Resume).unwrap();
        assert_eq!(result, ContractStatus::Active);
    }

    #[test]
    fn expired_renew_to_active() {
        let result = transition(&ContractStatus::Expired, &StatusEvent::Renew).unwrap();
        assert_eq!(result, ContractStatus::Active);
    }

    #[test]
    fn in_review_approve_to_active() {
        let result = transition(&ContractStatus::InReview, &StatusEvent::Approve).unwrap();
        assert_eq!(result, ContractStatus::Active);
    }

    #[test]
    fn in_review_reject_to_draft() {
        let result = transition(&ContractStatus::InReview, &StatusEvent::Reject).unwrap();
        assert_eq!(result, ContractStatus::Draft);
    }

    #[test]
    fn any_status_send_for_review() {
        for status in [
            ContractStatus::Active,
            ContractStatus::Drift,
            ContractStatus::Breach,
            ContractStatus::Paused,
        ] {
            let result = transition(&status, &StatusEvent::SendForReview).unwrap();
            assert_eq!(result, ContractStatus::InReview);
        }
    }

    #[test]
    fn invalid_transition_returns_err() {
        let result = transition(&ContractStatus::Draft, &StatusEvent::DriftDetected);
        assert!(result.is_err());
    }

    #[test]
    fn paused_cannot_drift() {
        let result = transition(&ContractStatus::Paused, &StatusEvent::DriftDetected);
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Run — verify tests fail**

```bash
cargo test -p rocket-collection contract::state_machine 2>&1 | tail -10
```

Expected: all tests fail with `todo!()` panic.

- [ ] **Step 3: Implement `transition()`**

Replace the `todo!()` in `transition()`:

```rust
pub fn transition(
    current: &ContractStatus,
    event: &StatusEvent,
) -> Result<ContractStatus, InvalidTransition> {
    use ContractStatus::*;
    use StatusEvent::*;

    let next = match (current, event) {
        // Draft lifecycle
        (Draft, Publish)        => Active,

        // Active transitions
        (Active, DriftDetected)  => Drift,
        (Active, BreachDetected) => Breach,
        (Active, Pause)          => Paused,
        (Active, ExpiryLapsed)   => Expired,
        (Active, ExpiringSoon)   => ExpiringIn30Days,

        // ExpiringIn30Days — same as Active for most events
        (ExpiringIn30Days, DriftDetected)  => Drift,
        (ExpiringIn30Days, BreachDetected) => Breach,
        (ExpiringIn30Days, Pause)          => Paused,
        (ExpiringIn30Days, ExpiryLapsed)   => Expired,

        // Drift transitions
        (Drift, Resign)        => Active,
        (Drift, MarkBreaking)  => Breach,
        (Drift, Pause)         => Paused,
        (Drift, BreachDetected)=> Breach,

        // Breach transitions
        (Breach, Resign)       => Active,
        (Breach, Pause)        => Paused,

        // Paused transitions
        (Paused, Resume)       => Active,

        // Expired transitions
        (Expired, Renew)       => Active,

        // InReview transitions
        (InReview, Approve)    => Active,
        (InReview, Reject)     => Draft,

        // SendForReview: valid from Active, Drift, Breach, Paused
        (Active | Drift | Breach | Paused, SendForReview) => InReview,

        // Any status can lapse into Expired
        (_, ExpiryLapsed) => Expired,

        // All other combinations are invalid
        _ => {
            return Err(InvalidTransition {
                from: current.clone(),
                event: format!("{:?}", std::mem::discriminant(event)),
            });
        }
    };

    Ok(next)
}
```

- [ ] **Step 4: Run tests — verify they pass**

```bash
cargo test -p rocket-collection contract::state_machine 2>&1 | tail -20
```

Expected: all 13 tests pass.

- [ ] **Step 5: Export from `mod.rs`**

Add to `crates/rocket-collection/src/contract/mod.rs`:

```rust
pub mod state_machine;
pub use state_machine::{StatusEvent, InvalidTransition, transition as transition_status};
```

- [ ] **Step 6: Workspace compile + tests**

```bash
cargo test --workspace 2>&1 | tail -10
```

Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-collection/src/contract/state_machine.rs
git add crates/rocket-collection/src/contract/mod.rs
git commit -m "feat(contract): state machine — pure transition() function with 13 test cases"
```

---

## Task 3: Update CLAUDE.md for `rocket-collection`

**Files:**
- Modify: `crates/rocket-collection/CLAUDE.md`

- [ ] **Step 1: Update CLAUDE.md**

Add the following section to `crates/rocket-collection/CLAUDE.md` (create the file if it does not exist):

```markdown
## Contract Module (`src/contract/`)

### New public types (SP1)
- `ContractParty` — replaces bare `String` for provider/consumer. Custom `Deserialize` accepts both plain strings (old format) and objects (new format).
- `PartyKind` — `Team | Company | Service`
- `ContractPolicy` — `breaking_change_policy`, `notice_days`, `uptime_sla`
- `BreakingChangePolicy` — `Strict | Lenient | AdditiveOk`
- `ContractStatus` — now has 8 variants including `Draft`, `Drift`, `Breach`, `InReview`, `Paused`. Status is **stored** in YAML, not computed at runtime.
- `ChangelogEntry.is_breaking: bool` — defaults `false` for backward compat.

### State machine (`state_machine.rs`)
- `transition(current: &ContractStatus, event: &StatusEvent) -> Result<ContractStatus, InvalidTransition>`
- Pure function — no I/O. Call from `ContractService` (in `rocket-app`) when handling lifecycle commands.

### Backward compatibility rules
- Old YAML `provider: "string"` → `ContractParty::from_name(string)`
- Old YAML `consumer: "string"` → `consumers: vec![ContractParty::from_name(string)]`
- Old YAML with no `status` field → defaults to `ContractStatus::Active`
- Old YAML `ChangelogEntry` with no `isBreaking` → defaults to `false`
```

- [ ] **Step 2: Commit**

```bash
git add crates/rocket-collection/CLAUDE.md
git commit -m "docs(rocket-collection): update CLAUDE.md with SP1 contract module changes"
```
