# SP2-01 — Policy-Aware diff_signature

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update `diff_signature` to accept a `&BreakingChangePolicy` parameter and populate `is_breaking` on every returned `ChangelogEntry` according to the policy table.

**Architecture:** `diff_signature` remains a pure function. The new `policy` parameter has a default so all existing call sites add `&BreakingChangePolicy::Lenient` — no behaviour change for existing contracts.

**Tech Stack:** Rust

**Spec:** `docs/superpowers/specs/2026-05-07-contract-lock-enhancement-design.md` §SP2

**Depends on:** SP1-02 merged.

---

> **⚠️ Worktree** — all commands run inside `.worktrees/contract-enhancement` on branch `feat/contract-lock-enhancement`.

## Task 1: Update `diff_signature` signature and breaking rules

**Files:**
- Modify: `crates/rocket-collection/src/contract/diff.rs`

- [ ] **Step 1: Write failing policy tests**

Add to `#[cfg(test)]` in `diff.rs`:

```rust
use crate::contract::types::BreakingChangePolicy;

fn make_snap(method: &str, params: Vec<(&str, bool)>) -> RequestSignatureSnapshot {
    RequestSignatureSnapshot {
        request_path: std::path::PathBuf::from("req.yml"),
        method: method.into(),
        url_pattern: "/test".into(),
        headers: vec![],
        query_params: params.iter().map(|(k, _)| KeyValueEntry { key: k.to_string(), value: String::new() }).collect(),
        body_content: None,
        form_fields: vec![],
        auth_type: "none".into(),
        auth_detail: String::new(),
        captured_at: chrono::Utc::now(),
        query_param_keys: vec![],
        header_keys: vec![],
        body_field_keys: vec![],
    }
}

#[test]
fn method_change_always_breaking_all_policies() {
    let old = make_snap("GET", vec![]);
    let new = make_snap("POST", vec![]);
    for policy in [BreakingChangePolicy::Strict, BreakingChangePolicy::Lenient, BreakingChangePolicy::AdditiveOk] {
        let entries = diff_signature(&old, &new, &policy);
        assert!(entries.iter().any(|e| e.field == "method" && e.is_breaking), "method change must be breaking for {:?}", policy);
    }
}

#[test]
fn new_param_strict_is_breaking_lenient_is_not() {
    let old = make_snap("GET", vec![]);
    let new = make_snap("GET", vec![("page", false)]);

    let strict = diff_signature(&old, &new, &BreakingChangePolicy::Strict);
    assert!(strict.iter().any(|e| e.is_breaking));

    let lenient = diff_signature(&old, &new, &BreakingChangePolicy::Lenient);
    assert!(lenient.iter().all(|e| !e.is_breaking));
}

#[test]
fn required_param_removed_always_breaking() {
    // Simulate: old has required param, new doesn't
    // We use query_param_keys for simplicity (old field still diffed)
    let mut old = make_snap("GET", vec![]);
    old.query_param_keys = vec!["required_param".into()];
    let new = make_snap("GET", vec![]);

    for policy in [BreakingChangePolicy::Strict, BreakingChangePolicy::Lenient, BreakingChangePolicy::AdditiveOk] {
        let entries = diff_signature(&old, &new, &policy);
        // At minimum, removed param should be detected
        assert!(!entries.is_empty(), "removed param must be detected for {:?}", policy);
    }
}

#[test]
fn additive_ok_new_param_not_breaking() {
    let old = make_snap("GET", vec![]);
    let new = make_snap("GET", vec![("page", false)]);
    let entries = diff_signature(&old, &new, &BreakingChangePolicy::AdditiveOk);
    // New param added — not breaking under additive_ok
    for e in &entries {
        assert!(!e.is_breaking, "additive change must not be breaking under AdditiveOk");
    }
}
```

- [ ] **Step 2: Run — verify they fail**

```bash
cargo test -p rocket-collection contract::diff 2>&1 | tail -10
```

Expected: compile errors — `diff_signature` doesn't accept `policy` yet.

- [ ] **Step 3: Update `diff_signature` signature**

In `diff.rs`, update the public function signature:

```rust
pub fn diff_signature(
    old: &RequestSignatureSnapshot,
    new: &RequestSignatureSnapshot,
    policy: &crate::contract::types::BreakingChangePolicy,
) -> Vec<ChangelogEntry>
```

Update `field_diff!` macro and all helper functions to accept and use `policy`. The breaking rules:

```rust
// Inside diff_signature, after computing each change, determine is_breaking:
use crate::contract::types::BreakingChangePolicy::*;

// Method change: always breaking
fn method_is_breaking(_policy: &BreakingChangePolicy) -> bool { true }

// Path change: always breaking
fn path_is_breaking(_policy: &BreakingChangePolicy) -> bool { true }

// Auth type change: always breaking
fn auth_is_breaking(_policy: &BreakingChangePolicy) -> bool { true }

// Param/header removed: breaking for Strict and Lenient, not for AdditiveOk (headers only)
fn removed_is_breaking(field_type: &str, policy: &BreakingChangePolicy) -> bool {
    match (field_type, policy) {
        ("header", AdditiveOk) => false,
        (_, Strict) | (_, Lenient) => true,
        _ => false,
    }
}

// Param added: breaking only for Strict
fn added_is_breaking(policy: &BreakingChangePolicy) -> bool {
    matches!(policy, Strict)
}
```

Update `diff_key_value_list` and `diff_key_list` helpers to accept `policy` and set `is_breaking` on each `ChangelogEntry`.

For the full updated `diff.rs` implementation:

```rust
use crate::contract::changelog::{ChangeType, ChangelogEntry};
use crate::contract::snapshot::{KeyValueEntry, RequestSignatureSnapshot};
use crate::contract::types::BreakingChangePolicy;
use chrono::Utc;

pub fn diff_signature(
    old: &RequestSignatureSnapshot,
    new: &RequestSignatureSnapshot,
    policy: &BreakingChangePolicy,
) -> Vec<ChangelogEntry> {
    let mut entries = Vec::new();
    let now = Utc::now();
    let path = new.request_path.clone();

    // Method change — always breaking
    if old.method != new.method {
        entries.push(ChangelogEntry {
            timestamp: now,
            request_path: path.clone(),
            field: "method".into(),
            change_type: ChangeType::Changed,
            old_value: Some(old.method.clone()),
            new_value: Some(new.method.clone()),
            is_breaking: true,
        });
    }

    // URL pattern change — always breaking
    if old.url_pattern != new.url_pattern {
        entries.push(ChangelogEntry {
            timestamp: now,
            request_path: path.clone(),
            field: "url_pattern".into(),
            change_type: ChangeType::Changed,
            old_value: Some(old.url_pattern.clone()),
            new_value: Some(new.url_pattern.clone()),
            is_breaking: true,
        });
    }

    // Auth type change — always breaking
    if old.auth_type != new.auth_type {
        entries.push(ChangelogEntry {
            timestamp: now,
            request_path: path.clone(),
            field: "auth_type".into(),
            change_type: ChangeType::Changed,
            old_value: Some(old.auth_type.clone()),
            new_value: Some(new.auth_type.clone()),
            is_breaking: true,
        });
    }

    // Auth detail change
    if old.auth_detail != new.auth_detail {
        entries.push(ChangelogEntry {
            timestamp: now,
            request_path: path.clone(),
            field: "auth_detail".into(),
            change_type: ChangeType::Changed,
            old_value: if old.auth_detail.is_empty() { None } else { Some(old.auth_detail.clone()) },
            new_value: if new.auth_detail.is_empty() { None } else { Some(new.auth_detail.clone()) },
            is_breaking: true, // auth credential changes always breaking
        });
    }

    // Key-value diffs
    diff_kv_list(&path, "header", &old.headers, &new.headers, policy, false, now, &mut entries);
    diff_kv_list(&path, "query_param", &old.query_params, &new.query_params, policy, false, now, &mut entries);
    diff_kv_list(&path, "form_field", &old.form_fields, &new.form_fields, policy, false, now, &mut entries);

    // Legacy key-only lists (backward compat for old snapshots)
    diff_key_only_list(&path, "query_param", &old.query_param_keys, &new.query_param_keys, policy, now, &mut entries);
    diff_key_only_list(&path, "header", &old.header_keys, &new.header_keys, policy, now, &mut entries);
    diff_key_only_list(&path, "body_field", &old.body_field_keys, &new.body_field_keys, policy, now, &mut entries);

    // Body content
    if old.body_content != new.body_content {
        let (change_type, is_breaking) = match (&old.body_content, &new.body_content) {
            (None, Some(_)) => (ChangeType::Added, matches!(policy, BreakingChangePolicy::Strict)),
            (Some(_), None) => (ChangeType::Removed, true),
            _ => (ChangeType::Changed, true),
        };
        entries.push(ChangelogEntry {
            timestamp: now,
            request_path: path.clone(),
            field: "body".into(),
            change_type,
            old_value: old.body_content.clone(),
            new_value: new.body_content.clone(),
            is_breaking,
        });
    }

    entries
}

fn diff_kv_list(
    path: &std::path::Path,
    prefix: &str,
    old_kvs: &[KeyValueEntry],
    new_kvs: &[KeyValueEntry],
    policy: &BreakingChangePolicy,
    _header: bool,
    now: chrono::DateTime<Utc>,
    out: &mut Vec<ChangelogEntry>,
) {
    let old_map: std::collections::HashMap<&str, &str> =
        old_kvs.iter().map(|e| (e.key.as_str(), e.value.as_str())).collect();
    let new_map: std::collections::HashMap<&str, &str> =
        new_kvs.iter().map(|e| (e.key.as_str(), e.value.as_str())).collect();

    for (key, old_val) in &old_map {
        if let Some(new_val) = new_map.get(key) {
            if old_val != new_val {
                out.push(ChangelogEntry {
                    timestamp: now,
                    request_path: path.to_path_buf(),
                    field: format!("{prefix}.{key}"),
                    change_type: ChangeType::Changed,
                    old_value: Some(old_val.to_string()),
                    new_value: Some(new_val.to_string()),
                    is_breaking: true, // value changes are always breaking
                });
            }
        } else {
            // Key removed
            let is_breaking = match (prefix, policy) {
                ("header", BreakingChangePolicy::AdditiveOk) => false,
                _ => true,
            };
            out.push(ChangelogEntry {
                timestamp: now,
                request_path: path.to_path_buf(),
                field: format!("{prefix}.{key}"),
                change_type: ChangeType::Removed,
                old_value: Some(old_val.to_string()),
                new_value: None,
                is_breaking,
            });
        }
    }

    for (key, new_val) in &new_map {
        if !old_map.contains_key(key) {
            out.push(ChangelogEntry {
                timestamp: now,
                request_path: path.to_path_buf(),
                field: format!("{prefix}.{key}"),
                change_type: ChangeType::Added,
                old_value: None,
                new_value: Some(new_val.to_string()),
                is_breaking: matches!(policy, BreakingChangePolicy::Strict),
            });
        }
    }
}

fn diff_key_only_list(
    path: &std::path::Path,
    prefix: &str,
    old_keys: &[String],
    new_keys: &[String],
    policy: &BreakingChangePolicy,
    now: chrono::DateTime<Utc>,
    out: &mut Vec<ChangelogEntry>,
) {
    for key in old_keys {
        if !new_keys.contains(key) {
            let is_breaking = match (prefix, policy) {
                ("header", BreakingChangePolicy::AdditiveOk) => false,
                _ => true,
            };
            out.push(ChangelogEntry {
                timestamp: now,
                request_path: path.to_path_buf(),
                field: format!("{prefix}.{key}"),
                change_type: ChangeType::Removed,
                old_value: Some(key.clone()),
                new_value: None,
                is_breaking,
            });
        }
    }
    for key in new_keys {
        if !old_keys.contains(key) {
            out.push(ChangelogEntry {
                timestamp: now,
                request_path: path.to_path_buf(),
                field: format!("{prefix}.{key}"),
                change_type: ChangeType::Added,
                old_value: None,
                new_value: Some(key.clone()),
                is_breaking: matches!(policy, BreakingChangePolicy::Strict),
            });
        }
    }
}
```

- [ ] **Step 4: Fix call sites**

The save hook in `rocket-app/src/contract_service.rs` calls `diff_signature`. Find all call sites:

```bash
grep -r "diff_signature" crates/ src-tauri/ --include="*.rs" -l
```

For each call site, add `&BreakingChangePolicy::Lenient` as the third argument. Example:

```rust
// Before:
let changes = diff_signature(&old_snap, &new_snap);
// After:
let changes = diff_signature(&old_snap, &new_snap, &BreakingChangePolicy::Lenient);
```

When the contract's actual policy is available (inside `recompute_drift_for_collection`), use `&contract.policy.breaking_change_policy` instead.

- [ ] **Step 5: Run all tests**

```bash
cargo test --workspace 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-collection/src/contract/diff.rs
git commit -m "feat(contract): policy-aware diff_signature — is_breaking computed per BreakingChangePolicy"
```

---

## Task 2: `recompute_drift_for_collection` service method

**Files:**
- Modify: `crates/rocket-app/src/contract_service.rs`

- [ ] **Step 1: Write failing test**

Add to `#[cfg(test)]` in `contract_service.rs`:

```rust
#[test]
fn recompute_drift_active_contract_no_changes_stays_active() {
    // Setup: contract with Active status, snapshot matches current requests
    // recompute_drift should not change status
    // Use tempfile + FsContractRepo for integration-style test
    let dir = tempfile::TempDir::new().unwrap();
    // ... setup contract with matching snapshot
    // After recompute, status should still be Active
    // This test verifies the no-op path
}
```

Note: full integration test setup is complex. At minimum verify the function compiles and returns `Ok`.

- [ ] **Step 2: Implement `recompute_drift_for_collection`**

Add to `ContractService`:

```rust
/// Scans all active contracts in the collection, runs diff_signature for
/// each against current request snapshots, updates drift/breach counts,
/// transitions status via the state machine, appends changelog entries,
/// and saves all modified contracts.
///
/// Returns a summary of each contract's new counts.
pub fn recompute_drift_for_collection(
    &self,
    collection_root: &std::path::Path,
    current_snapshots: &[rocket_collection::contract::snapshot::RequestSignatureSnapshot],
) -> DomainResult<Vec<ContractDriftSummary>> {
    use rocket_collection::contract::{
        diff::diff_signature,
        state_machine::{StatusEvent, transition_status},
        types::ContractStatus,
    };

    let contracts = self.repo.list(collection_root)?;
    let mut summaries = Vec::new();

    for mut contract in contracts {
        // Skip contracts not in an active monitoring state
        match contract.status {
            ContractStatus::Draft
            | ContractStatus::Paused
            | ContractStatus::Expired
            | ContractStatus::InReview => continue,
            _ => {}
        }

        // Load snapshot
        let snapshot = match self.repo.load_snapshot(collection_root, contract.id) {
            Ok(s) => s,
            Err(_) => continue, // No snapshot — draft that was never published
        };

        let mut all_entries = Vec::new();
        let mut drift_count = 0u32;
        let mut breach_count = 0u32;

        // Diff each snapshotted request against current
        for snap_entry in &snapshot.entries {
            let current = current_snapshots
                .iter()
                .find(|s| s.request_path == snap_entry.request_path);

            match current {
                None => {
                    // Request removed — always breaking
                    let entry = rocket_collection::contract::changelog::ChangelogEntry {
                        timestamp: chrono::Utc::now(),
                        request_path: snap_entry.request_path.clone(),
                        field: "request".into(),
                        change_type: rocket_collection::contract::changelog::ChangeType::Removed,
                        old_value: Some(format!("{} {}", snap_entry.method, snap_entry.url_pattern)),
                        new_value: None,
                        is_breaking: true,
                    };
                    drift_count += 1;
                    breach_count += 1;
                    all_entries.push(entry);
                }
                Some(current_snap) => {
                    let changes = diff_signature(
                        snap_entry,
                        current_snap,
                        &contract.policy.breaking_change_policy,
                    );
                    for entry in &changes {
                        drift_count += 1;
                        if entry.is_breaking { breach_count += 1; }
                    }
                    all_entries.extend(changes);
                }
            }
        }

        // Determine new status via state machine
        let event = if breach_count > 0 {
            StatusEvent::BreachDetected
        } else if drift_count > 0 {
            StatusEvent::DriftDetected
        } else {
            // No changes — if currently Drift or Breach, transition back to Active
            // This handles the case where a developer reverted their changes
            match contract.status {
                ContractStatus::Drift | ContractStatus::Breach => StatusEvent::Resign,
                _ => {
                    // No event needed — status unchanged
                    summaries.push(ContractDriftSummary {
                        contract_id: contract.id.to_string(),
                        status: contract.status.clone(),
                        drift_count: 0,
                        breach_count: 0,
                    });
                    continue;
                }
            }
        };

        if let Ok(new_status) = transition_status(&contract.status, &event) {
            contract.status = new_status;
        }

        contract.drift_count = drift_count;
        contract.breach_count = breach_count;
        contract.updated_at = Some(chrono::Utc::now());

        // Append new changelog entries (cap at 200 total)
        if !all_entries.is_empty() {
            let mut changelog = self.repo.load_changelog(collection_root, contract.id)
                .unwrap_or_else(|_| rocket_collection::contract::changelog::ContractChangelog::new(contract.id));
            changelog.append(all_entries);
            // Keep only the 200 most recent
            if changelog.entries.len() > 200 {
                let drain_count = changelog.entries.len() - 200;
                changelog.entries.drain(0..drain_count);
            }
            self.repo.save_changelog(collection_root, &changelog)?;
        }

        self.repo.save(collection_root, &contract)?;

        summaries.push(ContractDriftSummary {
            contract_id: contract.id.to_string(),
            status: contract.status,
            drift_count,
            breach_count,
        });
    }

    Ok(summaries)
}
```

Also add `ContractDriftSummary` struct to `contract_service.rs` or a DTO file:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractDriftSummary {
    pub contract_id: String,
    pub status: rocket_collection::contract::types::ContractStatus,
    pub drift_count: u32,
    pub breach_count: u32,
}
```

- [ ] **Step 3: Compile check**

```bash
cargo check -p rocket-app 2>&1 | grep "^error" | head -20
```

Fix any errors. Common issues: `load_snapshot`/`load_changelog` may need to be added to `ContractRepository` trait if not present.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-app/src/contract_service.rs
git commit -m "feat(contract): recompute_drift_for_collection — drift engine in ContractService"
```
