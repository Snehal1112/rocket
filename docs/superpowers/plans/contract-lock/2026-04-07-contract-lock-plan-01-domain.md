# Contract Lock — Plan 01: Domain Types + Diff Logic

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the `contract` module to `rocket-collection` with all domain types, snapshot, changelog, and the pure `diff_signature` function.

**Architecture:** New `crates/rocket-collection/src/contract/` module with five focused files. All types derive `Serialize`/`Deserialize` for `.yml` persistence. `diff_signature` is a pure function with no I/O — it is the Model B extension seam.

**Tech Stack:** Rust, `serde`/`serde_yaml`, `ulid`, `chrono`

---

## File Map

| File | Action |
|---|---|
| `crates/rocket-collection/src/contract/mod.rs` | Create — module root, re-exports |
| `crates/rocket-collection/src/contract/types.rs` | Create — `Contract`, `ContractScope`, `ContractStatus`, `ContractEnforcementMode` |
| `crates/rocket-collection/src/contract/snapshot.rs` | Create — `RequestSignatureSnapshot`, `ContractSnapshot` |
| `crates/rocket-collection/src/contract/changelog.rs` | Create — `ChangelogEntry`, `ContractChangelog`, `ChangeType` |
| `crates/rocket-collection/src/contract/diff.rs` | Create — `SignatureChange`, `diff_signature()` |
| `crates/rocket-collection/src/lib.rs` | Modify — export `pub mod contract` |

---

## Task 1: Core contract types

**Files:**
- Create: `crates/rocket-collection/src/contract/types.rs`

- [ ] **Step 1: Create `types.rs`**

```rust
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ulid::Ulid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    pub id: Ulid,
    pub title: String,
    pub provider: String,
    pub consumer: String,
    pub project: String,
    pub version: String,
    pub effective_date: NaiveDate,
    pub expiry_date: Option<NaiveDate>,
    pub document_path: Option<PathBuf>,
    pub enforcement_mode: ContractEnforcementMode,
    pub scope: ContractScope,
}

impl Contract {
    pub fn status(&self) -> ContractStatus {
        let today = Utc::now().date_naive();
        match self.expiry_date {
            None => ContractStatus::Active,
            Some(exp) if exp < today => ContractStatus::Expired,
            Some(exp) if (exp - today).num_days() <= 30 => ContractStatus::ExpiringIn30Days,
            _ => ContractStatus::Active,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ContractStatus {
    Active,
    ExpiringIn30Days,
    Expired,
}

/// Model B extension seam.
/// Only `Informational` is reachable from UI in this sprint.
/// `Warn` and `Block` are defined now so Model B requires zero domain changes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContractEnforcementMode {
    #[default]
    Informational,
    Warn,
    Block,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContractScope {
    Collection,
    Folder { rel_path: PathBuf },
    Request { rel_path: PathBuf },
}
```

- [ ] **Step 2: Verify compile**

```bash
cargo check -p rocket-collection
```

Expected: compiles cleanly (module not exported yet — that's fine).

---

## Task 2: Snapshot + changelog types

**Files:**
- Create: `crates/rocket-collection/src/contract/snapshot.rs`
- Create: `crates/rocket-collection/src/contract/changelog.rs`

- [ ] **Step 1: Create `snapshot.rs`**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ulid::Ulid;

/// Shape of one request at the moment a contract is signed.
/// Rebuilt on every save and diffed against this baseline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RequestSignatureSnapshot {
    pub request_path: PathBuf,
    pub method: String,
    pub url_pattern: String,
    pub query_param_keys: Vec<String>,
    pub header_keys: Vec<String>,
    pub body_field_keys: Vec<String>,
    pub auth_type: String,
    pub captured_at: DateTime<Utc>,
}

/// All snapshots for one contract (one entry per covered request).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractSnapshot {
    pub contract_id: Ulid,
    pub entries: Vec<RequestSignatureSnapshot>,
}

impl ContractSnapshot {
    pub fn new(contract_id: Ulid) -> Self {
        Self { contract_id, entries: Vec::new() }
    }

    pub fn get(&self, request_path: &std::path::Path) -> Option<&RequestSignatureSnapshot> {
        self.entries.iter().find(|e| e.request_path == request_path)
    }

    pub fn upsert(&mut self, snap: RequestSignatureSnapshot) {
        if let Some(existing) = self.entries.iter_mut().find(|e| e.request_path == snap.request_path) {
            *existing = snap;
        } else {
            self.entries.push(snap);
        }
    }
}
```

- [ ] **Step 2: Create `changelog.rs`**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ulid::Ulid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    Changed,
    Added,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangelogEntry {
    pub timestamp: DateTime<Utc>,
    pub request_path: PathBuf,
    pub field: String,
    pub change_type: ChangeType,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

/// Append-only audit log for one contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractChangelog {
    pub contract_id: Ulid,
    pub entries: Vec<ChangelogEntry>,
}

impl ContractChangelog {
    pub fn new(contract_id: Ulid) -> Self {
        Self { contract_id, entries: Vec::new() }
    }

    /// Appends entries — never removes. Returns count added.
    pub fn append(&mut self, new_entries: Vec<ChangelogEntry>) -> usize {
        let count = new_entries.len();
        self.entries.extend(new_entries);
        count
    }
}
```

- [ ] **Step 3: Verify compile**

```bash
cargo check -p rocket-collection
```

Expected: clean.

---

## Task 3: Diff logic + module wiring

**Files:**
- Create: `crates/rocket-collection/src/contract/diff.rs`
- Create: `crates/rocket-collection/src/contract/mod.rs`
- Modify: `crates/rocket-collection/src/lib.rs`

- [ ] **Step 1: Create `diff.rs`**

```rust
use crate::contract::changelog::{ChangeType, ChangelogEntry};
use crate::contract::snapshot::RequestSignatureSnapshot;
use chrono::Utc;

/// Pure function — no I/O, no side effects.
/// Returns one `ChangelogEntry` per detected change.
///
/// This is the Model B extension seam:
/// the save hook calls this function and currently logs results silently.
/// Model B will act on the return value (warn / block) without changing this function.
pub fn diff_signature(
    old: &RequestSignatureSnapshot,
    new: &RequestSignatureSnapshot,
) -> Vec<ChangelogEntry> {
    let mut entries = Vec::new();
    let now = Utc::now();
    let path = new.request_path.clone();

    macro_rules! field_diff {
        ($field:expr, $old:expr, $new:expr) => {
            if $old != $new {
                entries.push(ChangelogEntry {
                    timestamp: now,
                    request_path: path.clone(),
                    field: $field.to_string(),
                    change_type: ChangeType::Changed,
                    old_value: Some($old.to_string()),
                    new_value: Some($new.to_string()),
                });
            }
        };
    }

    field_diff!("method", old.method, new.method);
    field_diff!("url_pattern", old.url_pattern, new.url_pattern);
    field_diff!("auth_type", old.auth_type, new.auth_type);

    diff_key_list(&path, "query_param", &old.query_param_keys, &new.query_param_keys, now, &mut entries);
    diff_key_list(&path, "header", &old.header_keys, &new.header_keys, now, &mut entries);
    diff_key_list(&path, "body_field", &old.body_field_keys, &new.body_field_keys, now, &mut entries);

    entries
}

fn diff_key_list(
    path: &std::path::Path,
    prefix: &str,
    old_keys: &[String],
    new_keys: &[String],
    now: chrono::DateTime<chrono::Utc>,
    out: &mut Vec<ChangelogEntry>,
) {
    for key in old_keys {
        if !new_keys.contains(key) {
            out.push(ChangelogEntry {
                timestamp: now,
                request_path: path.to_path_buf(),
                field: format!("{}.{}", prefix, key),
                change_type: ChangeType::Removed,
                old_value: Some(key.clone()),
                new_value: None,
            });
        }
    }
    for key in new_keys {
        if !old_keys.contains(key) {
            out.push(ChangelogEntry {
                timestamp: now,
                request_path: path.to_path_buf(),
                field: format!("{}.{}", prefix, key),
                change_type: ChangeType::Added,
                old_value: None,
                new_value: Some(key.clone()),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn base_snap() -> RequestSignatureSnapshot {
        RequestSignatureSnapshot {
            request_path: PathBuf::from("requests/payment.yml"),
            method: "POST".into(),
            url_pattern: "/payments".into(),
            query_param_keys: vec!["currency".into()],
            header_keys: vec!["Authorization".into()],
            body_field_keys: vec!["amount".into(), "currency".into()],
            auth_type: "bearer".into(),
            captured_at: Utc::now(),
        }
    }

    #[test]
    fn no_changes_returns_empty() {
        let snap = base_snap();
        assert!(diff_signature(&snap, &snap).is_empty());
    }

    #[test]
    fn method_change_detected() {
        let old = base_snap();
        let mut new = base_snap();
        new.method = "PUT".into();
        let changes = diff_signature(&old, &new);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "method");
        assert_eq!(changes[0].change_type, ChangeType::Changed);
    }

    #[test]
    fn removed_body_field_detected() {
        let old = base_snap();
        let mut new = base_snap();
        new.body_field_keys = vec!["currency".into()]; // "amount" removed
        let changes = diff_signature(&old, &new);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "body_field.amount");
        assert_eq!(changes[0].change_type, ChangeType::Removed);
    }

    #[test]
    fn added_query_param_detected() {
        let old = base_snap();
        let mut new = base_snap();
        new.query_param_keys.push("locale".into());
        let changes = diff_signature(&old, &new);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "query_param.locale");
        assert_eq!(changes[0].change_type, ChangeType::Added);
    }

    #[test]
    fn multiple_changes_all_detected() {
        let old = base_snap();
        let mut new = base_snap();
        new.method = "PUT".into();
        new.body_field_keys = vec!["total".into()]; // "amount"+"currency" removed, "total" added
        let changes = diff_signature(&old, &new);
        // method + amount removed + currency removed + total added = 4
        assert_eq!(changes.len(), 4);
    }
}
```

- [ ] **Step 2: Create `mod.rs`**

```rust
pub mod changelog;
pub mod diff;
pub mod snapshot;
pub mod types;

pub use changelog::{ChangeType, ChangelogEntry, ContractChangelog};
pub use diff::diff_signature;
pub use snapshot::{ContractSnapshot, RequestSignatureSnapshot};
pub use types::{Contract, ContractEnforcementMode, ContractScope, ContractStatus};
```

- [ ] **Step 3: Export from `crates/rocket-collection/src/lib.rs`**

Add this line alongside the other `pub mod` declarations:

```rust
pub mod contract;
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p rocket-collection contract
```

Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-collection/src/contract/
git add crates/rocket-collection/src/lib.rs
git commit -m "feat(contract): domain types, snapshot, changelog, diff_signature"
```
