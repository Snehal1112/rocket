# Contract State Machine

> **Interactive diagram →** [`Contract State Machine.html`](./Contract%20State%20Machine.html)
> Open in a browser for a clickable, zoomable diagram with a flow simulator, tooltips, and a detail panel for every state and transition.

This document describes the lifecycle states a contract can occupy, the events that drive transitions between them, and the conditions under which each event fires.

---

## Visual Summary

```
                    ┌─────────────────────────────────────────────────────────--─┐
                    │              [ExpiryLapsed — any non-Archived]             │
                    │                                                            ▼
 Draft ──[Publish]──► Active ◄──────────────[Resign]────────────────---------- Drift ◄─-┐
           ▲          │  │                                                      │  │    │
           │          │  └──[DriftDetected]────────────────────────----------──►┘  │    │
           │    [Pause]│  └──[BreachDetected]──────────────────────────---------► Breach
           │          │                                    [BreachDetected]│  │
           │          ▼                                    [MarkBreaking]──┘  │
           │        Paused ◄────────────────[Pause from Drift / Breach]───────┘
           │          │
           │       [Resume]
           │          │
           │          ▼
           │        Active
           │
           │   [SendForReview — valid from Active, Drift, Breach, Paused]
           │          │
           │          ▼
           │       InReview ──[Approve]──► Active
           │          │
           │       [Reject]
           │          │
           └──────────┘

 Active ──[ExpiringSoon]──► ExpiringIn30Days ──┐
                                               │ (behaves like Active for drift/breach/pause)
                                               └──[ExpiryLapsed]──► Expired ──[Renew]──► Active
                                                                        │
                                                                    [Archive]
                                                                        │
                                                                        ▼
                                              Draft ◄──[Unarchive]── Archived
                                              (also reachable via: Paused ──[Archive]──► Archived)
```

---

## States

| State | Meaning | Drift-monitored |
|---|---|---|
| **Draft** | Created but not yet published. No baseline snapshot exists. | No |
| **Active** | Healthy — all covered endpoints match the signed baseline. | Yes |
| **Drift** | Non-breaking changes detected since signing. Consumer builds are safe but the contract has diverged from the baseline. | Yes |
| **Breach** | Breaking changes detected. Consumer builds are at risk. | Yes |
| **ExpiringIn30Days** | Expiry date is ≤ 30 days away. Behaves identically to Active for all monitoring purposes. | Yes |
| **InReview** | Sent to consumer for approval. Drift monitoring is **suspended** until Approved or Rejected. | No |
| **Paused** | Monitoring suspended by the provider. No drift or breach events are generated. | No |
| **Expired** | Past the expiry date. | No |
| **Archived** | Soft-deleted. Hidden from the main list; recoverable via Unarchive → Draft. | No |

> **Drift-monitored** means `recompute_drift_for_collection` evaluates the contract on every save.
> Draft, InReview, Paused, Expired, and Archived contracts are all skipped.

---

## Events

| Event | Triggered by | Description |
|---|---|---|
| `Publish` | `publish_contract` IPC | Moves Draft → Active and seals the baseline snapshot. |
| `DriftDetected` | `recompute_drift_for_collection` (automatic) | Non-breaking changes found vs the signed baseline. |
| `BreachDetected` | `recompute_drift_for_collection` (automatic) | Breaking changes found vs the signed baseline. |
| `Resign` | `accept_drift` or `publish_contract` called on Drift/Breach | Accepts current request shapes as the new baseline; returns to Active. |
| `MarkBreaking` | Manual (future UI action) | Escalates a Drift contract to Breach without waiting for automatic detection. |
| `Pause` | Context menu → Pause monitoring | Suspends drift evaluation. Valid from Active, ExpiringIn30Days, Drift, Breach, Paused. |
| `Resume` | Context menu → Resume | Resumes drift evaluation from Paused. |
| `SendForReview` | Context menu → Send for review | Routes to consumer for sign-off. Valid from Active, Drift, Breach, Paused. |
| `Approve` | Context menu → Approve | Consumer accepts; returns InReview → Active. |
| `Reject` | Context menu → Reject | Consumer rejects; returns InReview → Draft. |
| `ExpiringSoon` | Automatic date check | Fires when an Active contract's expiry date falls within 30 days. |
| `ExpiryLapsed` | Automatic date check | Fires when any non-Archived contract's expiry date has passed. |
| `Renew` | Context menu → Renew | Clears expiry and returns Expired → Active. |
| `Archive` | Context menu (Paused or Expired only) | Soft-deletes the contract. |
| `Unarchive` | Context menu → Unarchive | Restores Archived → Draft. |

---

## Valid Transitions

Rows = current state · Columns = event · Cell = destination state

| | Publish | DriftDetected | BreachDetected | Resign | MarkBreaking | Pause | Resume | SendForReview | Approve | Reject | ExpiringSoon | ExpiryLapsed | Renew | Archive | Unarchive |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| **Draft** | Active | — | — | — | — | — | — | — | — | — | — | — | — | — | — |
| **Active** | — | Drift | Breach | — | — | Paused | — | InReview | — | — | ExpiringIn30Days | Expired | — | — | — |
| **ExpiringIn30Days** | — | Drift | Breach | — | — | Paused | — | InReview | — | — | — | Expired | — | — | — |
| **Drift** | — | — | Breach | Active | Breach | Paused | — | InReview | — | — | — | Expired | — | — | — |
| **Breach** | — | — | — | Active | — | Paused | — | InReview | — | — | — | Expired | — | — | — |
| **InReview** | — | — | — | — | — | — | — | — | Active | Draft | — | Expired | — | — | — |
| **Paused** | — | — | — | — | — | — | Active | InReview | — | — | — | Expired | — | Archived | — |
| **Expired** | — | — | — | — | — | — | — | — | — | — | — | — | Active | Archived | — |
| **Archived** | — | — | — | — | — | — | — | — | — | — | — | — | — | — | Draft |

---

## How Drift and Breach Are Detected

`recompute_drift_for_collection` runs automatically 250 ms after any request save (triggered by the `collection-changed` Tauri event via `useContractDrift`). It walks the live collection, diffs every covered request against the signed baseline snapshot, counts breaking and non-breaking changes, then fires the appropriate event:

```
breach_count > 0              →  BreachDetected  →  Active/ExpiringIn30Days/Drift → Breach
drift_count > 0               →  DriftDetected   →  Active/ExpiringIn30Days → Drift
both zero, currently Drift    →  Resign          →  Drift → Active
both zero, currently Breach   →  Resign          →  Breach → Active
both zero, otherwise          →  no-op
```

### Breaking change policy

The contract's `policy.breakingChangePolicy` (default: `Lenient`) controls which changes are **Breach** vs **Drift**.

| Change | Strict | Lenient | AdditiveOk |
|---|---|---|---|
| Method changed | Breach | Breach | Breach |
| URL pattern changed | Breach | Breach | Breach |
| Auth type changed | Breach | Breach | Breach |
| Auth credential changed | Breach | Breach | Breach |
| Existing header value changed | Breach | Breach | Breach |
| Existing query param value changed | Breach | Breach | Breach |
| Body content changed or removed | Breach | Breach | Breach |
| Form field changed or removed | Breach | Breach | Breach |
| Request removed from collection | Breach | Breach | Breach |
| Query param / body field **removed** | Breach | Breach | Breach |
| Header **removed** | Breach | Breach | **Drift** |
| New query param **added** | Breach | **Drift** | **Drift** |
| New header **added** | Breach | **Drift** | **Drift** |
| Body **added** (None → Some) | Breach | **Drift** | **Drift** |

> `Lenient` does **not** mean "any change is drift". Method, URL, auth, and any
> existing value change are always Breach regardless of policy. Only *new additions*
> produce Drift under Lenient. `AdditiveOk` additionally treats header removals as
> non-breaking (Drift).

---

## Common Lifecycle Paths

### New contract — happy path
```
Draft ──[Publish]──► Active ──[DriftDetected]──► Drift ──[Resign/accept_drift]──► Active
```

### Breaking change, then re-sign
```
Active ──[BreachDetected]──► Breach ──[publish_contract]──► Active
```

### Drift escalates to breach
```
Active ──[DriftDetected]──► Drift ──[BreachDetected]──► Breach
                                    ──[MarkBreaking]──► Breach
```

### Consumer review
```
Active ──[SendForReview]──► InReview ──[Approve]──► Active
                                     └─[Reject]──► Draft
```

### Expiry cycle
```
Active ──[ExpiringSoon]──► ExpiringIn30Days ──[ExpiryLapsed]──► Expired ──[Renew]──► Active
```

### Archive and recovery
```
Paused ──[Archive]──► Archived ──[Unarchive]──► Draft
Expired ──[Archive]──► Archived ──[Unarchive]──► Draft
```

---

## Source Locations

| Concern | File |
|---|---|
| State enum and types | `crates/rocket-collection/src/contract/types.rs` |
| State machine (`transition`) | `crates/rocket-collection/src/contract/state_machine.rs` |
| Diff engine (breaking vs non-breaking) | `crates/rocket-collection/src/contract/diff.rs` |
| Snapshot capture (`from_request`) | `crates/rocket-collection/src/contract/snapshot.rs` |
| Drift recompute, accept drift, publish | `crates/rocket-app/src/contract_service.rs` |
| Tauri IPC commands | `src-tauri/src/commands/contract.rs` |
| File-watcher drift trigger (frontend) | `src/hooks/useContractDrift.ts` |
| Context menu actions (UI) | `src/components/contracts/ContractContextMenu.tsx` |
