# Team Network Plan 04: NetworkService, Audit and Tauri Commands

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 4 of 5.** Previous: `2026-11-06-team-network-plan-03-transport-discovery.md`. Next: `2026-11-06-team-network-plan-04b-end-to-end.md` (then Plan 05). Recommended model: Sonnet. Index (binding names): `docs/superpowers/plans/team-network/00-plan-index.md`.

**Goal:** Rocket starts a network node at launch: it loads its identity, serves the peer protocol, announces itself, lists nearby and paired nodes, pairs with a matching code, keeps one presence connection per paired device, and stops cleanly on exit. Fourteen Tauri commands expose this to the UI. Pairing outcomes, removals and blocks reach the tamper-evident security log.

**Architecture:** `rocket-audit` gets three event kinds under SOC 2 CC6.1. `rocket-app` gets a `network/` module with `NetworkService`. It holds only ports from `rocket-network`, as `Arc<dyn Trait>` (the index requires `Arc`, because the sink and the handler are shared with other threads): identity, four repositories, discovery, transport factory, event publisher, audit publisher. It implements `DiscoverySink` (through a small sink wrapper) and `PeerHandler` (so the peer server calls the service directly). One in-memory state behind one `std::sync::Mutex` holds the node table, open pairing sessions, cooldowns and per-device presence. Tasks are spawned on the tokio runtime and end through a `watch` stop signal, like `openapi_sync_scheduler.rs`. `src-tauri` adds thin commands with camelCase DTOs, wires the real adapters from Plans 02 and 03, and stores the `NetworkHandle` in a slot that is stopped at exit.

**Tech Stack:** Rust, tokio (`test-util` paused time in tests), async-trait, chrono, `rocket-network`, `rocket-audit`, Tauri 2 commands.

**Spec:** `docs/superpowers/specs/2026-11-05-team-network-discovery-pairing-design.md`, sections 5.3, 5.4, 6, 7.1 to 7.4, 8.3, 8.5, 9, 10 (rocket-app tests with fakes).

Prerequisites: Plans 01 to 03 are applied in the working tree. This plan uses only names from the index and from Plan 01's additions: `NodeProfile::validated`, `commitment_to_base64`, `commitment_from_base64`, `PairingId::default`, `PresenceTracker::default`, `PeerError::status_code`. Everything is imported from the `rocket_network` crate root (Plan 01 re-exports all of it) except constants, which come from `rocket_network::limits`.

## Global Constraints

- `-j4` on every cargo command. Never `cargo test --workspace` or `--all`. Use `cargo test -j4 -p rocket-app network` and `cargo check -j4 -p rocket-app`.
- No `unwrap()` or `expect()` in production code. Tests use `expect("reason")`. A repo hook rejects Rust text that contains a bare unwrap call, tests included, so write `expect("...")` even in test helpers.
- Locks: `self.state.lock().unwrap_or_else(|e| e.into_inner())`. Never hold a `std::sync::Mutex` guard across an `.await`. Take data out, drop the guard, then await.
- `rocket-app` holds only `Arc<dyn Trait>` ports. It never names an infra type. The infra types appear only in `src-tauri/src/lib.rs`.
- Persistence structs stay snake_case. camelCase goes on IPC DTOs in `src-tauri/src/commands/network.rs` only. Event payload fields stay snake_case (Plan 01 pins them).
- Code comments are short, full sentences that end with a period. No emojis.
- A new Tauri command goes into `generate_handler!` in `src-tauri/src/lib.rs` and into `src-tauri/app-commands.txt` (sorted), or it is denied at runtime.
- Every user-visible error text is a `DomainError::InvalidInput(...)` with the exact spec wording, because the frontend strips the `Invalid input: ` prefix (Plan 05 `errorText`).
- Logs carry codes and short device ids only. Never log nonces, codes, keys, certificates or key material.
- No commit during the plan. The plan ends with "reviewed at the plan end": the user reviews the whole change, then it is committed through `dev-workflow-skills:1-git-commit` with explicit paths. Task 3 also needs the `rocket-network` workspace dependency from Plan 01; Plan 01 adds it below `rocket-grpc = { path = "crates/rocket-grpc" }`. Only if `grep -n rocket-network Cargo.toml` shows no `[workspace.dependencies]` entry, add `rocket-network = { path = "crates/rocket-network" }` there.

## Review Focus

1. **A stranger spoofs an mDNS id.** A node announces the id of a paired teammate from another key. `handle_discovery` calls `/hello`, sees the certificate key hash to a different id, and never lists it or dials it. Also the case where the certificate matches but the `HelloResponse.device_id` lies. Tests: `a_stranger_spoofing_an_mdns_id_is_never_listed`, `a_lying_hello_body_is_never_listed`, `a_spoofed_id_of_a_paired_device_is_never_dialled` (Task 1, Task 2 for the dial part).
2. **A decision arrives after the request expired.** The responder clicks Accept at 121 s, or the initiator's decision reaches the responder after expiry. Nothing is saved, the user sees "The request expired.", and `PairingFinished expired` is published once. Tests: `a_decision_after_expiry_saves_nothing_and_says_expired`, `a_remote_decision_after_expiry_is_ignored` (Task 2).
3. **A peer unpairs while this node is offline.** On the next dial the peer answers `NotPaired`; this node removes the pairing, publishes `PairedRemoved { by_peer: true }` and audits it. An impostor at the saved address (identity mismatch) must not remove anything. Tests: `a_peer_that_unpaired_us_while_offline_is_removed_on_the_next_dial`, `an_impostor_at_a_saved_address_does_not_remove_the_pairing` (Task 2).
4. **The keychain is unavailable.** `start` returns a handle, the state is `Off { reason: "The keychain is not available, so networking is off." }`, the transport factory is never built, no listener starts, nothing is announced, and pairing is refused. Tests: `keychain_unavailable_turns_networking_off_without_a_listener` (Task 1), `pairing_is_refused_while_networking_is_off` (Task 2).
5. **The 201st pairing.** At 200 paired devices a new incoming request gets `Forbidden` (no notice), and a new outgoing request fails with exactly "You've reached the limit of 200 paired devices. Remove one to pair." An already paired device can still re-pair. Tests: `the_201st_incoming_request_is_forbidden`, `the_201st_outgoing_request_shows_the_limit_message` (Task 2).
6. **Two sockets for one pair.** Both sides dial at once. The socket dialled by the lower device id wins; the other is dropped and presence stays Online. The higher side waits `DIAL_GRACE` before dialling at all. Tests: `the_socket_dialled_by_the_lower_id_wins`, `the_higher_id_waits_for_the_grace_period`, `paired_nodes_open_exactly_one_connection` (Task 2).
7. **Blocked or hidden.** A blocked device gets `Forbidden`, a hidden node refuses new requests but still serves paired sockets, a decline starts a one minute cooldown that ends by itself. Blocking a paired device also unpairs it and closes its live socket, and its later frames get no answer. Tests in Task 2, including `blocking_a_paired_device_unpairs_it_and_closes_its_socket` and `frames_from_a_blocked_device_get_no_answer_and_close_the_socket`.

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `crates/rocket-audit/src/event.rs` | modify | Three kinds and their `tag()` arms, tests |
| `crates/rocket-audit/src/control.rs` | modify | Add the three tags to the CC6.1 entry, test |
| `crates/rocket-app/Cargo.toml` | modify | `rocket-network` dependency |
| `crates/rocket-app/src/lib.rs` | modify | `pub mod network;`, re-exports and `pub(crate) mod stop_signal;` |
| `crates/rocket-app/src/stop_signal.rs` | create | `sleep_or_stop` (moved out of `openapi_sync_scheduler.rs`) and `run_every`, with tests; shared by the sync scheduler and the network loops |
| `crates/rocket-app/src/openapi_sync_scheduler.rs` | modify | Delete the private `sleep_or_stop`, import it from `stop_signal` |
| `crates/rocket-app/src/network/mod.rs` | create | Module list, re-exports, test module paths |
| `crates/rocket-app/src/network/service.rs` | create | `NetworkDeps`, `NetworkService`, `NetworkHandle`, start, settings, addresses, shared helpers |
| `crates/rocket-app/src/network/status.rs` | create | `NetworkStatus`, `NetworkState`, damaged-file warnings |
| `crates/rocket-app/src/network/nodes.rs` | create | Node table, `NodeView`, `list_nodes`, discovery sink, manual probes |
| `crates/rocket-app/src/network/pairing.rs` | create (Task 1 interim, Task 2 final) | Session entries, request, respond, cancel, sweep |
| `crates/rocket-app/src/network/handler.rs` | create (Task 1 interim, Task 2 final) | `PeerHandler` implementation |
| `crates/rocket-app/src/network/connections.rs` | create (Task 1 interim, Task 2 final) | Per-device presence tasks, unpair |
| `crates/rocket-app/src/network/test_support.rs` | create | Fakes for every port, two-node harness |
| `crates/rocket-app/src/network/service_tests.rs` | create | Task 1 tests |
| `crates/rocket-app/src/network/pairing_tests.rs` | create | Task 2 pairing and handler tests |
| `crates/rocket-app/src/network/connections_tests.rs` | create | Task 2 presence tests |
| `src-tauri/Cargo.toml` | modify | `rocket-network` dependency |
| `src-tauri/src/commands/network.rs` | create | 14 commands, DTOs, `NetworkSlot` (an alias of `Slot`), `start_network` |
| `src-tauri/src/commands/slot.rs` | create | `Stoppable` and the generic `Slot<H>` (moved out of `import.rs`), with the slot tests |
| `src-tauri/src/commands/import.rs` | modify | `SyncSchedulerSlot` becomes an alias of `Slot`; its two tests move to `slot.rs` |
| `src-tauri/src/commands/mod.rs` | modify | `pub mod network;` and `pub mod slot;` |
| `src-tauri/src/lib.rs` | modify | Wiring, `generate_handler!`, start, shutdown |
| `src-tauri/app-commands.txt` | modify | 14 sorted names |

Per-task checks: `cargo check -j4 -p rocket-app`, `cargo test -j4 -p rocket-app network`, and for Task 3 `cargo test -j4 -p rocket ipc_acl`.

---

### Task 1: Audit kinds and the NetworkService core

**Files:**
- Modify: `crates/rocket-audit/src/event.rs`, `crates/rocket-audit/src/control.rs`, `crates/rocket-app/Cargo.toml`, `crates/rocket-app/src/lib.rs`, `crates/rocket-app/src/openapi_sync_scheduler.rs`
- Create: `crates/rocket-app/src/stop_signal.rs`
- Create: `crates/rocket-app/src/network/{mod.rs, service.rs, status.rs, nodes.rs, pairing.rs, handler.rs, connections.rs, test_support.rs, service_tests.rs}`
- Create (empty for now, filled in Task 2): `crates/rocket-app/src/network/{pairing_tests.rs, connections_tests.rs}`

**Interfaces:**

```rust
// rocket-audit
AuditEventKind::NetworkDevicePaired { device_id: String }
AuditEventKind::NetworkDeviceUnpaired { device_id: String, by_peer: bool }
AuditEventKind::NetworkDeviceBlocked { device_id: String }
// tags: "network_device_paired", "network_device_unpaired", "network_device_blocked"

// rocket-app network module (index names)
pub struct NetworkDeps { /* see index */ }
pub struct NetworkService { /* private */ }
pub struct NetworkHandle { /* private */ }
impl NetworkHandle { pub fn stop(&self); }            // idempotent, Drop calls it
pub enum NetworkState { Running, Off { reason: String } }
pub struct NetworkStatus { /* see index */ }
pub enum NodeGroup { Paired, Nearby }
pub enum NodeStatus { Online, Offline, Pairing, NeedsUpdate, Available }
pub struct NodeView { /* see index */ }
pub fn paired_limit_message() -> String;              // the spec's 200-device text
impl NetworkService {
    pub fn new(deps: NetworkDeps) -> Arc<NetworkService>;
    pub async fn start(self: &Arc<Self>) -> NetworkHandle;
    pub fn status(&self) -> NetworkStatus;
    pub fn list_nodes(&self) -> Vec<NodeView>;
    pub async fn set_hidden(self: &Arc<Self>, hidden: bool) -> DomainResult<()>;
    pub async fn set_display_name(self: &Arc<Self>, name: &str) -> DomainResult<()>;
    pub fn mark_intro_seen(&self) -> DomainResult<()>;
    pub fn list_addresses(&self) -> DomainResult<Vec<ManualAddress>>;
    pub async fn add_address(self: &Arc<Self>, host: &str, port: u16) -> DomainResult<()>;
    pub fn remove_address(&self, host: &str, port: u16) -> DomainResult<()>;
    pub fn list_blocked(&self) -> DomainResult<Vec<BlockedDevice>>;
    pub fn unblock(&self, device_id: &DeviceId) -> DomainResult<()>;
}
```

Design notes for the task:

- Time uses `tokio::time::Instant::now().into_std()` (helper `NetworkService::now()`), because `PairingSession` takes `std::time::Instant` and tokio's paused clock only moves tokio instants. Tests then control every timer with `tokio::time::advance`.
- `start` never fails. A failure lands in `status()`: keychain error gives `Off` with the spec reason; a bind failure gives `listener_available = false`; a discovery failure gives `discovery_available = false`.
- The discovery sink runs on the mDNS thread, not on the runtime, so the sink keeps a `tokio::runtime::Handle` and spawns the verification on it.
- A discovered node is listed only after `/hello` proves it: the certificate key hashes to the announced id and the `HelloResponse.device_id` says the same. The first address that answers decides; a wrong answer stops the search.
- `seen` keeps mDNS and manual addresses apart so a manual probe round can replace its own addresses without touching mDNS ones.

- [ ] **Step 1: Write the failing audit tests**

In `crates/rocket-audit/src/event.rs`, inside `mod tests`, after the test `serializes_tagged_event`, add (anchor: the last test ends with `assert!(json.contains("\"kind\":\"collection_deleted\""));` then `}` then the closing `}` of the module; insert before that final closing brace):

```rust
    #[test]
    fn network_kinds_have_stable_tags() {
        let paired = AuditEventKind::NetworkDevicePaired {
            device_id: "ABCD".into(),
        };
        let unpaired = AuditEventKind::NetworkDeviceUnpaired {
            device_id: "ABCD".into(),
            by_peer: true,
        };
        let blocked = AuditEventKind::NetworkDeviceBlocked {
            device_id: "ABCD".into(),
        };
        assert_eq!(paired.tag(), "network_device_paired");
        assert_eq!(unpaired.tag(), "network_device_unpaired");
        assert_eq!(blocked.tag(), "network_device_blocked");
    }

    #[test]
    fn network_kinds_map_to_soc2_cc6_1() {
        let kinds = [
            AuditEventKind::NetworkDevicePaired {
                device_id: "ABCD".into(),
            },
            AuditEventKind::NetworkDeviceUnpaired {
                device_id: "ABCD".into(),
                by_peer: false,
            },
            AuditEventKind::NetworkDeviceBlocked {
                device_id: "ABCD".into(),
            },
        ];
        for kind in kinds {
            let tag = kind.tag();
            let ev = SecurityAuditEvent::new("user", None, kind, "");
            assert!(
                ev.controls.iter().any(|c| c.code == "CC6.1"),
                "{tag} is not under CC6.1"
            );
        }
    }

    #[test]
    fn network_unpaired_serializes_with_snake_case_fields() {
        let kind = AuditEventKind::NetworkDeviceUnpaired {
            device_id: "ABCD".into(),
            by_peer: true,
        };
        let value = serde_json::to_value(&kind).expect("kind serializes");
        assert_eq!(value["kind"], "network_device_unpaired");
        assert_eq!(value["device_id"], "ABCD");
        assert_eq!(value["by_peer"], true);
        let back: AuditEventKind = serde_json::from_value(value).expect("kind round trips");
        assert_eq!(back, kind);
    }
```

In `crates/rocket-audit/src/control.rs`, inside `mod tests`, after `framework_serializes_snake_case`, add:

```rust
    #[test]
    fn cc6_1_lists_the_network_kinds() {
        for tag in [
            "network_device_paired",
            "network_device_unpaired",
            "network_device_blocked",
        ] {
            let controls = controls_for_kind(tag);
            assert!(
                controls
                    .iter()
                    .any(|c| c.framework == Framework::Soc2 && c.code == "CC6.1"),
                "{tag} missing from CC6.1"
            );
        }
    }
```

- [ ] **Step 2: Run the audit tests and see them fail**

Run: `cargo test -j4 -p rocket-audit network`
Expected: FAIL to compile, `error[E0599]: no variant or associated item named `NetworkDevicePaired` found for enum `AuditEventKind``.

- [ ] **Step 3: Add the kinds, tags and catalog entries**

In `crates/rocket-audit/src/event.rs`, in the enum, after the `AuditChainBroken { ... },` variant (anchor on `        actual_hash: String,\n    },\n}`), add three variants:

```rust
    NetworkDevicePaired {
        device_id: String,
    },
    NetworkDeviceUnpaired {
        device_id: String,
        by_peer: bool,
    },
    NetworkDeviceBlocked {
        device_id: String,
    },
```

In `tag()`, after the arm `Self::AuditChainBroken { .. } => "audit_chain_broken",` add:

```rust
            Self::NetworkDevicePaired { .. } => "network_device_paired",
            Self::NetworkDeviceUnpaired { .. } => "network_device_unpaired",
            Self::NetworkDeviceBlocked { .. } => "network_device_blocked",
```

In `crates/rocket-audit/src/control.rs`, replace the CC6.1 `kinds` line:

```rust
        kinds: &["sensitive_auth_used", "secret_variable_written"],
    },
    ControlEntry {
        framework: Framework::Soc2,
        code: "CC6.7",
```

with:

```rust
        kinds: &[
            "sensitive_auth_used",
            "secret_variable_written",
            "network_device_paired",
            "network_device_unpaired",
            "network_device_blocked",
        ],
    },
    ControlEntry {
        framework: Framework::Soc2,
        code: "CC6.7",
```

(The first line appears twice in the file, once for SOC 2 CC6.1 and once for ISO A.5.15; the three following lines make the quoted block unique.)

- [ ] **Step 4: Run the audit tests and see them pass**

Run: `cargo test -j4 -p rocket-audit`
Expected: PASS, all `rocket-audit` tests green, 0 failed. Then run `cargo check -j4 --workspace` and fix any non-exhaustive `match` on `AuditEventKind` the compiler reports (the earlier survey found none outside `event.rs`).

- [ ] **Step 5: Wire the dependency and the module, then add the test support (fakes)**

In `crates/rocket-app/Cargo.toml`, after the line `rocket-audit = { path = "../rocket-audit" }` add:

```toml
rocket-network.workspace = true
```

In `crates/rocket-app/src/lib.rs`, after `pub mod mcp_tool_service;` add `pub mod network;` (alphabetical: it goes before `pub mod oauth2_service;`). In the `pub use` block, put the re-export in alphabetical position, before the line `pub use openapi_sync_apply::{ApplyDeps, RendererFactory};`:

```rust
pub use network::{
    paired_limit_message, NetworkDeps, NetworkHandle, NetworkService, NetworkState, NetworkStatus,
    NodeGroup, NodeStatus, NodeView, PairingDecision, PairingStarted,
};
```

Share the stop-signal sleep instead of copying it. In `crates/rocket-app/src/lib.rs`, after the line `mod session_plan;` add `pub(crate) mod stop_signal;` (alphabetical; it goes before `pub mod template_service;`). Create `crates/rocket-app/src/stop_signal.rs`. `sleep_or_stop` is the private function of `openapi_sync_scheduler.rs`, moved here unchanged; `run_every` is the shape of the two simple periodic network loops:

```rust
//! Helpers for background loops that end on a stop signal.

use std::future::Future;
use std::time::Duration;

use tokio::sync::watch;

/// Sleeps for `wait`. Returns false when a stop came first or the handle is gone.
pub(crate) async fn sleep_or_stop(stop: &mut watch::Receiver<bool>, wait: Duration) -> bool {
    if *stop.borrow() {
        return false;
    }
    tokio::select! {
        _ = tokio::time::sleep(wait) => !*stop.borrow(),
        _ = stop.changed() => false,
    }
}

/// Waits `interval`, then runs `tick`, again and again, until a stop comes. The first tick runs
/// after one interval, not at once. A tick that is running is not cancelled by a stop.
pub(crate) async fn run_every<F, Fut>(
    stop: &mut watch::Receiver<bool>,
    interval: Duration,
    mut tick: F,
) where
    F: FnMut() -> Fut,
    Fut: Future<Output = ()>,
{
    while sleep_or_stop(stop, interval).await {
        tick().await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use super::*;

    #[tokio::test(start_paused = true)]
    async fn sleep_or_stop_returns_true_after_the_wait() {
        let (_tx, mut rx) = watch::channel(false);
        assert!(sleep_or_stop(&mut rx, Duration::from_secs(5)).await);
    }

    #[tokio::test(start_paused = true)]
    async fn sleep_or_stop_returns_false_when_already_stopped() {
        let (tx, mut rx) = watch::channel(false);
        tx.send_replace(true);
        assert!(!sleep_or_stop(&mut rx, Duration::from_secs(5)).await);
    }

    #[tokio::test(start_paused = true)]
    async fn sleep_or_stop_returns_false_when_a_stop_comes_during_the_wait() {
        let (tx, mut rx) = watch::channel(false);
        let task = tokio::spawn(async move { sleep_or_stop(&mut rx, Duration::from_secs(60)).await });
        tokio::time::advance(Duration::from_secs(1)).await;
        tx.send_replace(true);
        assert!(!task.await.expect("the task finishes"));
    }

    #[tokio::test(start_paused = true)]
    async fn sleep_or_stop_returns_false_when_the_sender_is_gone() {
        let (tx, mut rx) = watch::channel(false);
        drop(tx);
        assert!(!sleep_or_stop(&mut rx, Duration::from_secs(5)).await);
    }

    #[tokio::test(start_paused = true)]
    async fn run_every_ticks_once_per_interval_and_not_at_time_zero() {
        let (tx, mut rx) = watch::channel(false);
        let ticks = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&ticks);
        let tick = move || {
            let counter = Arc::clone(&counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        };
        let stopper = async {
            tokio::time::sleep(Duration::from_secs(25)).await;
            tx.send_replace(true);
        };
        tokio::join!(run_every(&mut rx, Duration::from_secs(10), tick), stopper);
        assert_eq!(ticks.load(Ordering::SeqCst), 2, "ticks at 10 s and 20 s");
    }

    #[tokio::test(start_paused = true)]
    async fn run_every_does_not_tick_when_already_stopped() {
        let (tx, mut rx) = watch::channel(false);
        tx.send_replace(true);
        let ticks = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&ticks);
        run_every(&mut rx, Duration::from_secs(10), move || {
            let counter = Arc::clone(&counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        })
        .await;
        assert_eq!(ticks.load(Ordering::SeqCst), 0);
    }
}
```

In `crates/rocket-app/src/openapi_sync_scheduler.rs`, delete the private `sleep_or_stop` function (the doc comment `/// Sleeps for `wait`. ...` and the function below `error_backoff_secs`), and add `use crate::stop_signal::sleep_or_stop;` next to the other `use crate::...` lines at the top. Keep `use tokio::sync::watch;` and `use std::time::Duration;`, the file still uses both. Its two calls (`launch_delay` and `tick`) stay as they are.

Create `crates/rocket-app/src/network/mod.rs`:

```rust
//! Team network: discovery, pairing and presence for Rocket nodes.
//!
//! `NetworkService` owns the lifecycle. It talks to the outside only through the ports in
//! `rocket-network`, so tests run with fakes and no sockets.

mod connections;
mod handler;
mod nodes;
mod pairing;
mod service;
mod status;
#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
#[path = "connections_tests.rs"]
mod connections_tests;
#[cfg(test)]
#[path = "pairing_tests.rs"]
mod pairing_tests;
#[cfg(test)]
#[path = "service_tests.rs"]
mod service_tests;

pub use nodes::{NodeGroup, NodeStatus, NodeView};
pub use pairing::{PairingDecision, PairingStarted};
pub use service::{paired_limit_message, NetworkDeps, NetworkHandle, NetworkService};
pub use status::{NetworkState, NetworkStatus};
```

Create two empty files so the module compiles: `crates/rocket-app/src/network/pairing_tests.rs` and `crates/rocket-app/src/network/connections_tests.rs`, each containing only this line:

```rust
// Filled in by Task 2.
```

Create `crates/rocket-app/src/network/test_support.rs`:

```rust
//! Fakes for every port, and a harness that links two services in memory.
//!
//! `FakeConnector` can route a call to another service's `PeerHandler`, so two real
//! `NetworkService`s pair and connect without sockets. A route without a handler answers
//! `/hello` from fixed data and hands out scripted sockets, which tests use to play a peer.

use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use rocket_audit::publisher::RecordingSecurityAuditPublisher;
use rocket_network::{
    Announcement, BlockedDevice, BlockedDeviceRepository, DecisionRequest, DeviceId,
    DeviceIdentityStore, DeviceKeyMaterial, Discovery, DiscoveryEvent, DiscoverySink, HelloResponse,
    ManualAddress, ManualAddressRepository, NetworkEvent, NetworkSettings,
    NetworkSettingsRepository, NodeProfile, NonceMessage, PairRequest, PairRequestResponse,
    PairStatusResponse, PairedDevice, PairedDeviceRepository, PairingId, PeerCallError,
    PeerConnector, PeerError, PeerHandler, PeerIdentity, PeerMessage, PeerServerControl, PeerSocket,
    PeerTransport, PeerTransportFactory, QuarantineReport,
};
use rocket_network::limits::{MAX_MANUAL_ADDRESSES, MAX_PAIRED_DEVICES, PROTOCOL_VERSION};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{EventPublisher, PublishedEvent};
use tokio::sync::mpsc;

use super::service::paired_limit_message;
use super::{NetworkDeps, NetworkService, PairingDecision};

/// The address of the node with this seed.
pub(crate) fn addr(seed: u8) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 47000 + u16::from(seed)))
}

/// A peer identity whose key is eight bytes of `seed`.
pub(crate) fn identity(seed: u8) -> PeerIdentity {
    PeerIdentity::from_spki(vec![seed; 8])
}

pub(crate) fn profile(name: &str) -> NodeProfile {
    NodeProfile::new(name).expect("valid test profile")
}

/// Lets every ready task run. With paused time this also moves the clock 1 ms.
pub(crate) async fn settle() {
    tokio::time::sleep(Duration::from_millis(1)).await;
}

pub(crate) fn ip() -> IpAddr {
    IpAddr::from([127, 0, 0, 1])
}

pub(crate) fn pair_request(name: &str) -> PairRequest {
    PairRequest {
        version: PROTOCOL_VERSION,
        profile: profile(name),
    }
}

// ----- identity, discovery, server, transport -----

pub(crate) struct FakeIdentity {
    pub fail: AtomicBool,
}

impl DeviceIdentityStore for FakeIdentity {
    fn load_or_create(&self) -> DomainResult<DeviceKeyMaterial> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(DomainError::Internal("the keychain could not be read".into()));
        }
        Ok(DeviceKeyMaterial {
            pkcs8_der: vec![7; 32],
        })
    }
}

#[derive(Default)]
pub(crate) struct FakeDiscovery {
    pub fail_start: AtomicBool,
    pub starts: AtomicUsize,
    pub stops: AtomicUsize,
    pub withdraws: AtomicUsize,
    pub announced: Mutex<Vec<Announcement>>,
    sink: Mutex<Option<Arc<dyn DiscoverySink>>>,
}

impl FakeDiscovery {
    /// Plays an mDNS event into the service, as the mDNS thread would. Not `emit`, which is the
    /// service's own event publisher.
    pub fn play(&self, event: DiscoveryEvent) {
        let sink = self.sink.lock().expect("sink lock").clone();
        if let Some(sink) = sink {
            sink.on_event(event);
        }
    }

    pub fn announcements(&self) -> Vec<Announcement> {
        self.announced.lock().expect("announced lock").clone()
    }
}

impl Discovery for FakeDiscovery {
    fn start(&self, sink: Arc<dyn DiscoverySink>) -> DomainResult<()> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        if self.fail_start.load(Ordering::SeqCst) {
            return Err(DomainError::Io("multicast is not available".into()));
        }
        *self.sink.lock().expect("sink lock") = Some(sink);
        Ok(())
    }

    fn announce(&self, announcement: &Announcement) -> DomainResult<()> {
        self.announced
            .lock()
            .expect("announced lock")
            .push(announcement.clone());
        Ok(())
    }

    fn withdraw(&self) -> DomainResult<()> {
        self.withdraws.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn stop(&self) {
        self.stops.fetch_add(1, Ordering::SeqCst);
    }
}

pub(crate) struct FakeServer {
    pub port: Mutex<Result<u16, String>>,
    pub starts: AtomicUsize,
    pub stops: AtomicUsize,
}

impl FakeServer {
    pub fn new() -> Arc<FakeServer> {
        Arc::new(FakeServer {
            port: Mutex::new(Ok(47821)),
            starts: AtomicUsize::new(0),
            stops: AtomicUsize::new(0),
        })
    }
}

#[async_trait]
impl PeerServerControl for FakeServer {
    async fn start(&self, _handler: Arc<dyn PeerHandler>) -> DomainResult<u16> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        let outcome = self.port.lock().expect("port lock").clone();
        outcome.map_err(DomainError::Io)
    }

    fn stop(&self) {
        self.stops.fetch_add(1, Ordering::SeqCst);
    }
}

/// One address the fake connector can reach.
#[derive(Clone)]
pub(crate) struct Route {
    /// The key the certificate on that address carries.
    pub identity: PeerIdentity,
    pub profile: NodeProfile,
    pub version: u32,
    /// When set, calls reach this handler. When not, `/hello` is answered from fixed data.
    pub handler: Option<Arc<dyn PeerHandler>>,
    /// What the `/hello` body claims as its device id, when it should differ from the key.
    pub claims: Option<DeviceId>,
}

impl Route {
    pub fn fixed(identity: PeerIdentity, name: &str) -> Route {
        Route {
            identity,
            profile: profile(name),
            version: PROTOCOL_VERSION,
            handler: None,
            claims: None,
        }
    }
}

pub(crate) struct FakeConnector {
    pub caller: PeerIdentity,
    routes: Mutex<HashMap<SocketAddr, Route>>,
    scripted: Mutex<VecDeque<PeerSocket>>,
    pub hello_calls: AtomicUsize,
    pub open_calls: AtomicUsize,
    pub pair_request_calls: AtomicUsize,
}

impl FakeConnector {
    pub fn new(caller: PeerIdentity) -> Arc<FakeConnector> {
        Arc::new(FakeConnector {
            caller,
            routes: Mutex::new(HashMap::new()),
            scripted: Mutex::new(VecDeque::new()),
            hello_calls: AtomicUsize::new(0),
            open_calls: AtomicUsize::new(0),
            pair_request_calls: AtomicUsize::new(0),
        })
    }

    pub fn add_route(&self, address: SocketAddr, route: Route) {
        self.routes.lock().expect("routes lock").insert(address, route);
    }

    pub fn remove_route(&self, address: SocketAddr) {
        self.routes.lock().expect("routes lock").remove(&address);
    }

    /// Queues a socket that `open_socket` returns for a route without a handler.
    pub fn push_socket(&self, socket: PeerSocket) {
        self.scripted.lock().expect("scripted lock").push_back(socket);
    }

    fn route(&self, address: SocketAddr) -> Result<Route, PeerCallError> {
        self.routes
            .lock()
            .expect("routes lock")
            .get(&address)
            .cloned()
            .ok_or_else(|| PeerCallError::Unreachable("no route".into()))
    }

    /// The route for `address`, checked against the key the caller expects there.
    fn checked(
        &self,
        address: SocketAddr,
        expected: &DeviceId,
    ) -> Result<(Route, Arc<dyn PeerHandler>), PeerCallError> {
        let route = self.route(address)?;
        if route.identity.device_id != *expected {
            return Err(PeerCallError::IdentityMismatch);
        }
        let handler = route
            .handler
            .clone()
            .ok_or_else(|| PeerCallError::Unreachable("no handler".into()))?;
        Ok((route, handler))
    }
}

fn map_peer_error(error: PeerError) -> PeerCallError {
    match error {
        PeerError::Busy => PeerCallError::Busy,
        PeerError::NotPaired => PeerCallError::NotPaired,
        other => PeerCallError::Rejected(other.status_code()),
    }
}

#[async_trait]
impl PeerConnector for FakeConnector {
    async fn hello(
        &self,
        address: SocketAddr,
    ) -> Result<(PeerIdentity, HelloResponse), PeerCallError> {
        self.hello_calls.fetch_add(1, Ordering::SeqCst);
        let route = self.route(address)?;
        let response = match &route.handler {
            Some(handler) => handler.hello().await,
            None => HelloResponse {
                device_id: route.claims.unwrap_or(route.identity.device_id),
                version: route.version,
                profile: route.profile.clone(),
            },
        };
        Ok((route.identity, response))
    }

    async fn pair_request(
        &self,
        address: SocketAddr,
        expected: &DeviceId,
        req: &PairRequest,
    ) -> Result<(PeerIdentity, PairRequestResponse), PeerCallError> {
        self.pair_request_calls.fetch_add(1, Ordering::SeqCst);
        let (route, handler) = self.checked(address, expected)?;
        let reply = handler
            .pair_request(self.caller.clone(), ip(), req.clone())
            .await
            .map_err(map_peer_error)?;
        Ok((route.identity, reply))
    }

    async fn pair_nonce(
        &self,
        address: SocketAddr,
        expected: &DeviceId,
        id: &PairingId,
        msg: &NonceMessage,
    ) -> Result<NonceMessage, PeerCallError> {
        let (_, handler) = self.checked(address, expected)?;
        handler
            .pair_nonce(self.caller.clone(), id.clone(), msg.clone())
            .await
            .map_err(map_peer_error)
    }

    async fn pair_decision(
        &self,
        address: SocketAddr,
        expected: &DeviceId,
        id: &PairingId,
        req: &DecisionRequest,
    ) -> Result<(), PeerCallError> {
        let (_, handler) = self.checked(address, expected)?;
        handler
            .pair_decision(self.caller.clone(), id.clone(), req.clone())
            .await
            .map_err(map_peer_error)
    }

    async fn pair_status(
        &self,
        address: SocketAddr,
        expected: &DeviceId,
        id: &PairingId,
    ) -> Result<PairStatusResponse, PeerCallError> {
        let (_, handler) = self.checked(address, expected)?;
        handler
            .pair_status(self.caller.clone(), id.clone())
            .await
            .map_err(map_peer_error)
    }

    async fn open_socket(
        &self,
        address: SocketAddr,
        expected: &DeviceId,
        hello: PeerMessage,
    ) -> Result<PeerSocket, PeerCallError> {
        self.open_calls.fetch_add(1, Ordering::SeqCst);
        let route = self.route(address)?;
        if route.identity.device_id != *expected {
            return Err(PeerCallError::IdentityMismatch);
        }
        let Some(handler) = route.handler.clone() else {
            return self
                .scripted
                .lock()
                .expect("scripted lock")
                .pop_front()
                .ok_or_else(|| PeerCallError::Unreachable("no scripted socket".into()));
        };
        if !handler.is_paired(&self.caller.device_id).await {
            return Err(PeerCallError::NotPaired);
        }
        let (to_them, their_inbound) = mpsc::channel(16);
        let (to_us, our_inbound) = mpsc::channel(16);
        let theirs = PeerSocket {
            peer: self.caller.clone(),
            dialed_by: self.caller.device_id,
            remote_hello: hello,
            outbound: to_us,
            inbound: their_inbound,
        };
        let ours = PeerSocket {
            peer: route.identity,
            dialed_by: self.caller.device_id,
            remote_hello: handler.local_hello(),
            outbound: to_them,
            inbound: our_inbound,
        };
        handler.on_peer_socket(theirs).await;
        Ok(ours)
    }
}

/// The test side of a scripted socket.
pub(crate) struct Remote {
    pub to_service: mpsc::Sender<PeerMessage>,
    pub from_service: mpsc::Receiver<PeerMessage>,
}

/// A socket the service sees as coming from `peer`, plus the handles to play the peer.
pub(crate) fn scripted_socket(
    peer: PeerIdentity,
    dialed_by: DeviceId,
    version: u32,
) -> (PeerSocket, Remote) {
    let (to_service, inbound) = mpsc::channel(16);
    let (outbound, from_service) = mpsc::channel(16);
    let socket = PeerSocket {
        peer,
        dialed_by,
        remote_hello: PeerMessage::Hello {
            version,
            profile: profile("Remote"),
        },
        outbound,
        inbound,
    };
    (
        socket,
        Remote {
            to_service,
            from_service,
        },
    )
}

pub(crate) struct FakeTransportFactory {
    local: PeerIdentity,
    connector: Arc<FakeConnector>,
    server: Arc<FakeServer>,
    pub builds: AtomicUsize,
    pub fail: AtomicBool,
}

impl PeerTransportFactory for FakeTransportFactory {
    fn build(&self, _key: &DeviceKeyMaterial) -> DomainResult<PeerTransport> {
        self.builds.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) {
            return Err(DomainError::Internal("tls setup failed".into()));
        }
        Ok(PeerTransport {
            local: self.local.clone(),
            connector: self.connector.clone(),
            server: self.server.clone(),
        })
    }
}

// ----- repositories -----

#[derive(Default)]
pub(crate) struct MemPaired(Mutex<Vec<PairedDevice>>);

impl MemPaired {
    pub fn seed(&self, id: DeviceId, name: &str) {
        self.seed_with(id, name, Vec::new());
    }

    pub fn seed_with(&self, id: DeviceId, name: &str, last_addresses: Vec<SocketAddr>) {
        self.0.lock().expect("paired lock").push(PairedDevice {
            device_id: id,
            name_at_pairing: name.to_string(),
            paired_at: Utc::now(),
            last_addresses,
        });
    }

    /// Adds `count` unrelated paired devices.
    pub fn fill(&self, count: usize) {
        for index in 0..count {
            let mut bytes = [0u8; 32];
            bytes[0] = (index >> 8) as u8;
            bytes[1] = index as u8;
            bytes[31] = 0xEE;
            self.seed(DeviceId::from_bytes(bytes), "Filler");
        }
    }

    pub fn contains(&self, id: &DeviceId) -> bool {
        self.0
            .lock()
            .expect("paired lock")
            .iter()
            .any(|p| p.device_id == *id)
    }

    pub fn count(&self) -> usize {
        self.0.lock().expect("paired lock").len()
    }

    pub fn last_addresses(&self, id: &DeviceId) -> Vec<SocketAddr> {
        self.0
            .lock()
            .expect("paired lock")
            .iter()
            .find(|p| p.device_id == *id)
            .map(|p| p.last_addresses.clone())
            .unwrap_or_default()
    }
}

impl PairedDeviceRepository for MemPaired {
    fn list(&self) -> DomainResult<Vec<PairedDevice>> {
        Ok(self.0.lock().expect("paired lock").clone())
    }

    fn get(&self, id: &DeviceId) -> DomainResult<Option<PairedDevice>> {
        Ok(self
            .0
            .lock()
            .expect("paired lock")
            .iter()
            .find(|p| p.device_id == *id)
            .cloned())
    }

    fn upsert(&self, device: PairedDevice) -> DomainResult<()> {
        let mut list = self.0.lock().expect("paired lock");
        if let Some(existing) = list.iter_mut().find(|p| p.device_id == device.device_id) {
            *existing = device;
            return Ok(());
        }
        if list.len() >= MAX_PAIRED_DEVICES {
            return Err(DomainError::InvalidInput(paired_limit_message()));
        }
        list.push(device);
        Ok(())
    }

    fn remove(&self, id: &DeviceId) -> DomainResult<bool> {
        let mut list = self.0.lock().expect("paired lock");
        let before = list.len();
        list.retain(|p| p.device_id != *id);
        Ok(list.len() != before)
    }
}

#[derive(Default)]
pub(crate) struct MemBlocked(Mutex<Vec<BlockedDevice>>);

impl MemBlocked {
    pub fn put(&self, id: DeviceId, name: &str) {
        self.0.lock().expect("blocked lock").push(BlockedDevice {
            device_id: id,
            name: name.to_string(),
            blocked_at: Utc::now(),
        });
    }

    pub fn has(&self, id: &DeviceId) -> bool {
        self.0
            .lock()
            .expect("blocked lock")
            .iter()
            .any(|b| b.device_id == *id)
    }
}

impl BlockedDeviceRepository for MemBlocked {
    fn list(&self) -> DomainResult<Vec<BlockedDevice>> {
        Ok(self.0.lock().expect("blocked lock").clone())
    }

    fn contains(&self, id: &DeviceId) -> DomainResult<bool> {
        Ok(self
            .0
            .lock()
            .expect("blocked lock")
            .iter()
            .any(|b| b.device_id == *id))
    }

    fn add(&self, device: BlockedDevice) -> DomainResult<()> {
        let mut list = self.0.lock().expect("blocked lock");
        if !list.iter().any(|b| b.device_id == device.device_id) {
            list.push(device);
        }
        Ok(())
    }

    fn remove(&self, id: &DeviceId) -> DomainResult<bool> {
        let mut list = self.0.lock().expect("blocked lock");
        let before = list.len();
        list.retain(|b| b.device_id != *id);
        Ok(list.len() != before)
    }
}

#[derive(Default)]
pub(crate) struct MemAddresses(Mutex<Vec<ManualAddress>>);

impl ManualAddressRepository for MemAddresses {
    fn list(&self) -> DomainResult<Vec<ManualAddress>> {
        Ok(self.0.lock().expect("addresses lock").clone())
    }

    fn add(&self, address: ManualAddress) -> DomainResult<()> {
        let mut list = self.0.lock().expect("addresses lock");
        let host = address.host.to_lowercase();
        if list
            .iter()
            .any(|a| a.host.to_lowercase() == host && a.port == address.port)
        {
            return Ok(());
        }
        if list.len() >= MAX_MANUAL_ADDRESSES {
            return Err(DomainError::InvalidInput(
                "You already saved 50 addresses.".into(),
            ));
        }
        list.push(address);
        Ok(())
    }

    fn remove(&self, host: &str, port: u16) -> DomainResult<bool> {
        let mut list = self.0.lock().expect("addresses lock");
        let before = list.len();
        let host = host.to_lowercase();
        list.retain(|a| !(a.host.to_lowercase() == host && a.port == port));
        Ok(list.len() != before)
    }
}

#[derive(Default)]
pub(crate) struct MemSettings(Mutex<NetworkSettings>);

impl MemSettings {
    pub fn get(&self) -> NetworkSettings {
        self.0.lock().expect("settings lock").clone()
    }

    pub fn set_hidden(&self, hidden: bool) {
        self.0.lock().expect("settings lock").hidden = hidden;
    }

    pub fn set_name(&self, name: &str) {
        self.0.lock().expect("settings lock").display_name = Some(name.to_string());
    }
}

impl NetworkSettingsRepository for MemSettings {
    fn load(&self) -> DomainResult<NetworkSettings> {
        Ok(self.get())
    }

    fn save(&self, settings: &NetworkSettings) -> DomainResult<()> {
        *self.0.lock().expect("settings lock") = settings.clone();
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct FakeQuarantine(Mutex<Option<String>>);

impl FakeQuarantine {
    pub fn set(&self, file: &str) {
        *self.0.lock().expect("quarantine lock") = Some(file.to_string());
    }
}

impl QuarantineReport for FakeQuarantine {
    fn take_quarantined(&self) -> Option<String> {
        self.0.lock().expect("quarantine lock").take()
    }
}

// ----- events -----

/// Kept concrete on purpose: its query helpers are specific to `NetworkEvent`, and the recorders
/// in other crates are private test doubles.
#[derive(Default)]
pub(crate) struct RecordingEvents(Mutex<Vec<NetworkEvent>>);

impl RecordingEvents {
    pub fn all(&self) -> Vec<NetworkEvent> {
        self.0.lock().expect("events lock").clone()
    }

    pub fn incoming(&self) -> Vec<NetworkEvent> {
        self.all()
            .into_iter()
            .filter(|e| matches!(e, NetworkEvent::PairingIncoming { .. }))
            .collect()
    }

    /// The `(pairing_id, code)` of the first incoming notice, if any.
    pub fn first_incoming(&self) -> Option<(String, String)> {
        self.all().into_iter().find_map(|e| match e {
            NetworkEvent::PairingIncoming {
                pairing_id, code, ..
            } => Some((pairing_id, code)),
            _ => None,
        })
    }

    /// The outcome text of every `PairingFinished`, in order.
    pub fn finished(&self) -> Vec<String> {
        self.all()
            .into_iter()
            .filter_map(|e| match e {
                NetworkEvent::PairingFinished { outcome, .. } => Some(outcome),
                _ => None,
            })
            .collect()
    }

    /// The `(display_name, by_peer)` of every `PairedRemoved`, in order.
    pub fn removed(&self) -> Vec<(String, bool)> {
        self.all()
            .into_iter()
            .filter_map(|e| match e {
                NetworkEvent::PairedRemoved {
                    display_name,
                    by_peer,
                    ..
                } => Some((display_name, by_peer)),
                _ => None,
            })
            .collect()
    }
}

impl EventPublisher for RecordingEvents {
    fn publish(&self, event: &dyn PublishedEvent) {
        if let Some(event) = event.downcast_ref::<NetworkEvent>() {
            self.0.lock().expect("events lock").push(event.clone());
        }
    }
}

// ----- the harness -----

/// One service with all its fakes.
pub(crate) struct Node {
    pub seed: u8,
    pub name: String,
    pub svc: Arc<NetworkService>,
    pub identity: PeerIdentity,
    pub connector: Arc<FakeConnector>,
    pub server: Arc<FakeServer>,
    pub discovery: Arc<FakeDiscovery>,
    pub factory: Arc<FakeTransportFactory>,
    pub id_store: Arc<FakeIdentity>,
    pub paired: Arc<MemPaired>,
    pub blocked: Arc<MemBlocked>,
    pub addresses: Arc<MemAddresses>,
    pub settings: Arc<MemSettings>,
    pub quarantine: Arc<FakeQuarantine>,
    pub events: Arc<RecordingEvents>,
    pub audit: Arc<RecordingSecurityAuditPublisher>,
}

impl Node {
    pub fn id(&self) -> DeviceId {
        self.identity.device_id
    }

    pub fn addr(&self) -> SocketAddr {
        addr(self.seed)
    }

    pub fn profile(&self) -> NodeProfile {
        profile(&self.name)
    }

    pub fn handler(&self) -> Arc<dyn PeerHandler> {
        self.svc.clone()
    }
}

/// Builds a node. Its OS user name is `name`, so that is its default display name.
pub(crate) fn node(seed: u8, name: &str) -> Node {
    let identity = identity(seed);
    let connector = FakeConnector::new(identity.clone());
    let server = FakeServer::new();
    let discovery = Arc::new(FakeDiscovery::default());
    let factory = Arc::new(FakeTransportFactory {
        local: identity.clone(),
        connector: connector.clone(),
        server: server.clone(),
        builds: AtomicUsize::new(0),
        fail: AtomicBool::new(false),
    });
    let id_store = Arc::new(FakeIdentity {
        fail: AtomicBool::new(false),
    });
    let paired = Arc::new(MemPaired::default());
    let blocked = Arc::new(MemBlocked::default());
    let addresses = Arc::new(MemAddresses::default());
    let settings = Arc::new(MemSettings::default());
    let quarantine = Arc::new(FakeQuarantine::default());
    let events = Arc::new(RecordingEvents::default());
    let audit = Arc::new(RecordingSecurityAuditPublisher::new());
    let svc = NetworkService::new(NetworkDeps {
        identity: id_store.clone(),
        paired: paired.clone(),
        blocked: blocked.clone(),
        addresses: addresses.clone(),
        settings: settings.clone(),
        quarantine: vec![quarantine.clone() as Arc<dyn QuarantineReport>],
        discovery: discovery.clone(),
        transport: factory.clone(),
        events: events.clone(),
        audit: audit.clone(),
        os_user_name: Some(name.to_string()),
    });
    Node {
        seed,
        name: name.to_string(),
        svc,
        identity,
        connector,
        server,
        discovery,
        factory,
        id_store,
        paired,
        blocked,
        addresses,
        settings,
        quarantine,
        events,
        audit,
    }
}

/// Lets each node reach the other's handler at the other's address.
pub(crate) fn link(a: &Node, b: &Node) {
    a.connector.add_route(
        b.addr(),
        Route {
            identity: b.identity.clone(),
            profile: b.profile(),
            version: PROTOCOL_VERSION,
            handler: Some(b.handler()),
            claims: None,
        },
    );
    b.connector.add_route(
        a.addr(),
        Route {
            identity: a.identity.clone(),
            profile: a.profile(),
            version: PROTOCOL_VERSION,
            handler: Some(a.handler()),
            claims: None,
        },
    );
}

/// Plays an mDNS sighting of `other` into `node`.
pub(crate) fn see(node: &Node, other: &Node) {
    node.discovery.play(DiscoveryEvent::Seen {
        claimed_id: other.id(),
        profile: other.profile(),
        addresses: vec![other.addr()],
        version: PROTOCOL_VERSION,
    });
}

/// The node with the lower device id first.
pub(crate) fn ordered<'a>(x: &'a Node, y: &'a Node) -> (&'a Node, &'a Node) {
    if x.id() < y.id() {
        (x, y)
    } else {
        (y, x)
    }
}

/// Both users accept: the responder first, then the initiator. It does not advance time, so the
/// caller waits for the save in its own way (`settle` here, `wait_until` in Plan 04b).
pub(crate) async fn accept_both(
    responder: &Arc<NetworkService>,
    initiator: &Arc<NetworkService>,
    id: &PairingId,
) {
    responder
        .respond_pairing(id, PairingDecision::Accept)
        .await
        .expect("responder accepts");
    initiator
        .respond_pairing(id, PairingDecision::Accept)
        .await
        .expect("initiator accepts");
}

/// The status of the row for `id`, if any.
pub(crate) fn row_status(node: &Node, id: &DeviceId) -> Option<super::NodeStatus> {
    node.svc
        .list_nodes()
        .into_iter()
        .find(|row| row.device_id == id.to_base32())
        .map(|row| row.status)
}
```

- [ ] **Step 6: Write the failing service tests**

Create `crates/rocket-app/src/network/service_tests.rs`:

```rust
//! Tests for start, status, node listing, settings and addresses. All ports are fakes.

use std::sync::atomic::Ordering;
use std::time::Duration;

use rocket_network::limits::{MANUAL_PROBE_INTERVAL, PROTOCOL_VERSION};
use rocket_network::{DeviceId, DiscoveryEvent, DiscoverySource, NetworkEvent};
use rocket_shared::error::DomainError;

use super::test_support::*;
use super::{NetworkState, NodeGroup, NodeStatus};

#[tokio::test(start_paused = true)]
async fn start_runs_and_announces_with_the_port_in_use() {
    let n = node(1, "sam");
    let _handle = n.svc.start().await;

    let status = n.svc.status();
    assert_eq!(status.state, NetworkState::Running);
    assert_eq!(status.port, Some(47821));
    assert!(status.listener_available);
    assert!(status.discovery_available);
    assert_eq!(status.profile.display_name, "sam");
    assert_eq!(status.device_short_id, Some(n.id().short()));
    assert!(!status.hidden);
    assert!(!status.intro_seen);

    let announced = n.discovery.announcements();
    assert_eq!(announced.len(), 1);
    assert_eq!(announced[0].port, 47821);
    assert_eq!(announced[0].device_id, n.id());
    assert_eq!(announced[0].version, PROTOCOL_VERSION);
    assert_eq!(announced[0].profile.display_name, "sam");
    assert!(n.events.all().contains(&NetworkEvent::StatusChanged));
}

#[tokio::test(start_paused = true)]
async fn keychain_unavailable_turns_networking_off_without_a_listener() {
    let n = node(1, "sam");
    n.id_store.fail.store(true, Ordering::SeqCst);
    let _handle = n.svc.start().await;

    assert_eq!(
        n.svc.status().state,
        NetworkState::Off {
            reason: "The keychain is not available, so networking is off.".to_string()
        }
    );
    assert_eq!(n.factory.builds.load(Ordering::SeqCst), 0, "no TLS setup");
    assert_eq!(n.server.starts.load(Ordering::SeqCst), 0, "no listener");
    assert_eq!(n.discovery.starts.load(Ordering::SeqCst), 0, "no discovery");
    assert!(n.discovery.announcements().is_empty(), "nothing announced");
    assert!(n.svc.list_nodes().is_empty());
    assert!(n.events.all().contains(&NetworkEvent::StatusChanged));
}

#[tokio::test(start_paused = true)]
async fn a_transport_setup_failure_also_turns_networking_off() {
    let n = node(1, "sam");
    n.factory.fail.store(true, Ordering::SeqCst);
    let _handle = n.svc.start().await;

    assert!(matches!(n.svc.status().state, NetworkState::Off { .. }));
    assert_eq!(n.server.starts.load(Ordering::SeqCst), 0);
    assert!(n.discovery.announcements().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_bind_failure_keeps_running_but_reports_no_listener() {
    let n = node(1, "sam");
    *n.server.port.lock().expect("port lock") = Err("address in use".to_string());
    let _handle = n.svc.start().await;

    let status = n.svc.status();
    assert_eq!(status.state, NetworkState::Running);
    assert!(!status.listener_available);
    assert_eq!(status.port, None);
    assert!(n.discovery.announcements().is_empty(), "no port to announce");
}

#[tokio::test(start_paused = true)]
async fn a_discovery_failure_reports_it_and_manual_addresses_still_work() {
    let n = node(1, "sam");
    n.discovery.fail_start.store(true, Ordering::SeqCst);
    n.connector
        .add_route(addr(5), Route::fixed(identity(5), "Pat"));
    let _handle = n.svc.start().await;

    let status = n.svc.status();
    assert_eq!(status.state, NetworkState::Running);
    assert!(!status.discovery_available);
    assert!(n.discovery.announcements().is_empty());

    n.svc.add_address("127.0.0.1", 47005).await.expect("add");
    assert_eq!(n.svc.list_nodes().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_hidden_node_starts_without_announcing() {
    let n = node(1, "sam");
    n.settings.set_hidden(true);
    let _handle = n.svc.start().await;

    assert!(n.svc.status().hidden);
    assert!(n.discovery.announcements().is_empty());
}

#[tokio::test(start_paused = true)]
async fn hiding_withdraws_and_showing_announces_again() {
    let n = node(1, "sam");
    let _handle = n.svc.start().await;
    assert_eq!(n.discovery.announcements().len(), 1);

    n.svc.set_hidden(true).await.expect("hide");
    assert_eq!(n.discovery.withdraws.load(Ordering::SeqCst), 1);
    assert!(n.settings.get().hidden);
    assert!(n.svc.status().hidden);

    n.svc.set_hidden(false).await.expect("show");
    assert_eq!(n.discovery.announcements().len(), 2);
    assert!(!n.settings.get().hidden);
}

#[tokio::test(start_paused = true)]
async fn the_display_name_is_validated_saved_and_announced() {
    let n = node(1, "sam");
    let _handle = n.svc.start().await;

    let err = n.svc.set_display_name("   ").await.expect_err("empty name");
    assert!(matches!(err, DomainError::InvalidInput(_)));
    let err = n
        .svc
        .set_display_name(&"x".repeat(41))
        .await
        .expect_err("name too long");
    assert!(matches!(err, DomainError::InvalidInput(_)));

    n.svc
        .set_display_name("  Dana Reyes ")
        .await
        .expect("valid name");
    assert_eq!(n.settings.get().display_name.as_deref(), Some("Dana Reyes"));
    let status = n.svc.status();
    assert_eq!(status.profile.display_name, "Dana Reyes");
    assert_eq!(status.profile.initials, "DR");
    let announced = n.discovery.announcements();
    assert_eq!(announced.len(), 2);
    assert_eq!(announced[1].profile.display_name, "Dana Reyes");
}

#[tokio::test(start_paused = true)]
async fn the_saved_display_name_wins_over_the_os_user_name() {
    let n = node(1, "sam");
    n.settings.set_name("Chosen Name");
    let _handle = n.svc.start().await;
    assert_eq!(n.svc.status().profile.display_name, "Chosen Name");
}

#[tokio::test(start_paused = true)]
async fn the_intro_card_is_marked_seen_and_saved() {
    let n = node(1, "sam");
    let _handle = n.svc.start().await;
    assert!(!n.svc.status().intro_seen);

    n.svc.mark_intro_seen().expect("mark");
    assert!(n.svc.status().intro_seen);
    assert!(n.settings.get().intro_seen);
}

#[tokio::test(start_paused = true)]
async fn damaged_files_become_warnings_and_blocked_yml_has_its_own_text() {
    let n = node(1, "sam");
    n.quarantine.set("blocked.yml");
    let _handle = n.svc.start().await;
    assert_eq!(
        n.svc.status().warnings,
        vec!["Your blocked devices list was damaged and has been reset.".to_string()]
    );

    let m = node(2, "mia");
    m.quarantine.set("paired.yml.corrupt-1730000000");
    let _handle = m.svc.start().await;
    assert_eq!(
        m.svc.status().warnings,
        vec!["paired.yml was damaged and has been reset.".to_string()]
    );
}

// ----- discovery and the node list -----

#[tokio::test(start_paused = true)]
async fn a_verified_nearby_node_is_listed_with_its_hello_profile() {
    let a = node(1, "ann");
    a.connector
        .add_route(addr(2), Route::fixed(identity(2), "Ben Ortiz"));
    let _handle = a.svc.start().await;

    a.discovery.play(DiscoveryEvent::Seen {
        claimed_id: identity(2).device_id,
        profile: profile("A name from the TXT record"),
        addresses: vec![addr(2)],
        version: PROTOCOL_VERSION,
    });
    settle().await;

    let rows = a.svc.list_nodes();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].display_name, "Ben Ortiz", "the verified hello wins");
    assert_eq!(rows[0].initials, "BO");
    assert_eq!(rows[0].group, NodeGroup::Nearby);
    assert_eq!(rows[0].status, NodeStatus::Available);
    assert_eq!(rows[0].source, Some(DiscoverySource::Mdns));
    assert_eq!(rows[0].short_id, identity(2).device_id.short());
    assert!(a.events.all().contains(&NetworkEvent::NodesChanged));
}

#[tokio::test(start_paused = true)]
async fn a_stranger_spoofing_an_mdns_id_is_never_listed() {
    let a = node(1, "ann");
    let victim = identity(9);
    // The address answers with the stranger's own key, not the victim's.
    a.connector
        .add_route(addr(8), Route::fixed(identity(8), "Mallory"));
    let _handle = a.svc.start().await;

    a.discovery.play(DiscoveryEvent::Seen {
        claimed_id: victim.device_id,
        profile: profile("Victim"),
        addresses: vec![addr(8)],
        version: PROTOCOL_VERSION,
    });
    settle().await;

    assert!(a.svc.list_nodes().is_empty());
    assert_eq!(a.connector.hello_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn a_lying_hello_body_is_never_listed() {
    let a = node(1, "ann");
    let victim = identity(9);
    // The certificate key is the victim's, but the body names someone else.
    let mut route = Route::fixed(victim.clone(), "Victim");
    route.claims = Some(identity(8).device_id);
    a.connector.add_route(addr(9), route);
    let _handle = a.svc.start().await;

    a.discovery.play(DiscoveryEvent::Seen {
        claimed_id: victim.device_id,
        profile: profile("Victim"),
        addresses: vec![addr(9)],
        version: PROTOCOL_VERSION,
    });
    settle().await;

    assert!(a.svc.list_nodes().is_empty());
}

#[tokio::test(start_paused = true)]
async fn the_own_device_id_is_filtered_out() {
    let a = node(1, "ann");
    a.connector
        .add_route(a.addr(), Route::fixed(a.identity.clone(), "ann"));
    let _handle = a.svc.start().await;

    a.discovery.play(DiscoveryEvent::Seen {
        claimed_id: a.id(),
        profile: a.profile(),
        addresses: vec![a.addr()],
        version: PROTOCOL_VERSION,
    });
    settle().await;

    assert!(a.svc.list_nodes().is_empty());
    assert_eq!(a.connector.hello_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn sightings_on_two_addresses_merge_into_one_row() {
    let a = node(1, "ann");
    // The first address does not answer. The second and third carry the same key.
    a.connector
        .add_route(addr(21), Route::fixed(identity(2), "Ben"));
    a.connector
        .add_route(addr(22), Route::fixed(identity(2), "Ben"));
    let _handle = a.svc.start().await;
    let ben = identity(2).device_id;

    a.discovery.play(DiscoveryEvent::Seen {
        claimed_id: ben,
        profile: profile("Ben"),
        addresses: vec![addr(20), addr(21)],
        version: PROTOCOL_VERSION,
    });
    settle().await;
    a.discovery.play(DiscoveryEvent::Seen {
        claimed_id: ben,
        profile: profile("Ben"),
        addresses: vec![addr(22)],
        version: PROTOCOL_VERSION,
    });
    settle().await;

    assert_eq!(a.svc.list_nodes().len(), 1);
    let addresses = a.svc.mdns_addresses(&ben);
    assert!(addresses.contains(&addr(21)) && addresses.contains(&addr(22)));
    assert!(!addresses.contains(&addr(20)), "the dead address is not kept");
}

#[tokio::test(start_paused = true)]
async fn a_goodbye_removes_the_node_at_once() {
    let a = node(1, "ann");
    a.connector
        .add_route(addr(2), Route::fixed(identity(2), "Ben"));
    let _handle = a.svc.start().await;
    let ben = identity(2).device_id;
    a.discovery.play(DiscoveryEvent::Seen {
        claimed_id: ben,
        profile: profile("Ben"),
        addresses: vec![addr(2)],
        version: PROTOCOL_VERSION,
    });
    settle().await;
    assert_eq!(a.svc.list_nodes().len(), 1);

    a.discovery.play(DiscoveryEvent::Gone { claimed_id: ben });
    settle().await;
    assert!(a.svc.list_nodes().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_node_with_another_protocol_version_needs_an_update() {
    let a = node(1, "ann");
    let mut route = Route::fixed(identity(2), "Ben");
    route.version = PROTOCOL_VERSION + 1;
    a.connector.add_route(addr(2), route);
    let _handle = a.svc.start().await;

    a.discovery.play(DiscoveryEvent::Seen {
        claimed_id: identity(2).device_id,
        profile: profile("Ben"),
        addresses: vec![addr(2)],
        version: PROTOCOL_VERSION + 1,
    });
    settle().await;

    let rows = a.svc.list_nodes();
    assert_eq!(rows[0].status, NodeStatus::NeedsUpdate);
}

#[tokio::test(start_paused = true)]
async fn a_paired_device_is_listed_under_paired_and_a_blocked_one_is_hidden() {
    let a = node(1, "ann");
    a.paired.seed(identity(2).device_id, "Old Name");
    a.connector
        .add_route(addr(2), Route::fixed(identity(2), "New Name"));
    a.connector
        .add_route(addr(3), Route::fixed(identity(3), "Cy"));
    a.blocked.put(identity(3).device_id, "Cy");
    let _handle = a.svc.start().await;

    // Before it is seen, the paired row uses the name saved at pairing time.
    let rows = a.svc.list_nodes();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].group, NodeGroup::Paired);
    assert_eq!(rows[0].display_name, "Old Name");
    assert_eq!(rows[0].status, NodeStatus::Offline);

    for (seed, name) in [(2u8, "New Name"), (3u8, "Cy")] {
        a.discovery.play(DiscoveryEvent::Seen {
            claimed_id: identity(seed).device_id,
            profile: profile(name),
            addresses: vec![addr(seed)],
            version: PROTOCOL_VERSION,
        });
    }
    settle().await;

    let rows = a.svc.list_nodes();
    assert_eq!(rows.len(), 1, "the blocked device is not offered");
    assert_eq!(rows[0].display_name, "New Name", "the live name is preferred");
    assert_eq!(rows[0].group, NodeGroup::Paired);
}

// ----- manual addresses -----

#[tokio::test(start_paused = true)]
async fn adding_an_address_probes_it_at_once() {
    let a = node(1, "ann");
    a.connector
        .add_route(addr(5), Route::fixed(identity(5), "Pat"));
    let _handle = a.svc.start().await;

    a.svc.add_address("127.0.0.1", 47005).await.expect("add");

    let rows = a.svc.list_nodes();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].source, Some(DiscoverySource::Manual));
    assert_eq!(rows[0].status, NodeStatus::Available);
    assert_eq!(a.svc.list_addresses().expect("list").len(), 1);
}

#[tokio::test(start_paused = true)]
async fn an_unreachable_address_is_probed_again_every_interval() {
    let a = node(1, "ann");
    let _handle = a.svc.start().await;
    a.svc.add_address("127.0.0.1", 47005).await.expect("add");
    assert!(a.svc.list_nodes().is_empty());

    a.connector
        .add_route(addr(5), Route::fixed(identity(5), "Pat"));
    tokio::time::advance(MANUAL_PROBE_INTERVAL + Duration::from_secs(1)).await;
    settle().await;

    assert_eq!(a.svc.list_nodes().len(), 1);

    // When it goes away, the next round removes it.
    a.connector.remove_route(addr(5));
    tokio::time::advance(MANUAL_PROBE_INTERVAL + Duration::from_secs(1)).await;
    settle().await;
    assert!(a.svc.list_nodes().is_empty());
}

#[tokio::test(start_paused = true)]
async fn bad_addresses_are_refused_and_removing_one_forgets_it() {
    let a = node(1, "ann");
    let _handle = a.svc.start().await;

    for (host, port) in [("", 47821u16), ("   ", 47821), ("a b", 47821), ("host/x", 47821), ("h", 0)] {
        let err = a
            .svc
            .add_address(host, port)
            .await
            .expect_err("bad address");
        assert!(matches!(err, DomainError::InvalidInput(_)), "{host:?}");
    }
    assert!(a.svc.list_addresses().expect("list").is_empty());

    a.svc.add_address("Team.Example", 47821).await.expect("add");
    a.svc.remove_address("team.example", 47821).expect("remove");
    assert!(a.svc.list_addresses().expect("list").is_empty());
}

// ----- stopping -----

#[tokio::test(start_paused = true)]
async fn stopping_the_handle_stops_the_server_and_discovery_once() {
    let a = node(1, "ann");
    let handle = a.svc.start().await;

    handle.stop();
    handle.stop();
    assert_eq!(a.server.stops.load(Ordering::SeqCst), 1);
    assert_eq!(a.discovery.stops.load(Ordering::SeqCst), 1);

    drop(handle);
    assert_eq!(a.server.stops.load(Ordering::SeqCst), 1, "drop does not stop twice");
}

#[tokio::test(start_paused = true)]
async fn dropping_the_handle_stops_everything() {
    let a = node(1, "ann");
    let handle = a.svc.start().await;
    drop(handle);
    assert_eq!(a.server.stops.load(Ordering::SeqCst), 1);
    assert_eq!(a.discovery.stops.load(Ordering::SeqCst), 1);
}

#[test]
fn the_limit_message_is_the_spec_text() {
    assert_eq!(
        super::paired_limit_message(),
        "You've reached the limit of 200 paired devices. Remove one to pair."
    );
}

#[tokio::test(start_paused = true)]
async fn blocked_devices_can_be_listed_and_unblocked() {
    let a = node(1, "ann");
    let _handle = a.svc.start().await;
    let id = DeviceId::from_bytes([4; 32]);
    a.blocked.put(id, "Dee");

    assert_eq!(a.svc.list_blocked().expect("list").len(), 1);
    let before = a.events.all().len();
    a.svc.unblock(&id).expect("unblock");
    assert!(a.svc.list_blocked().expect("list").is_empty());
    let after = a.events.all()[before..].to_vec();
    assert!(after.contains(&NetworkEvent::NodesChanged), "the panel refetches");
}

- [ ] **Step 7: Run the service tests and see them fail**

Run: `cargo test -j4 -p rocket-app network`
Expected: FAIL to compile, `error[E0583]: file not found for module `connections`` (and the same for `handler`, `nodes`, `pairing`, `service`, `status`), because `network/mod.rs` names files that do not exist yet.

- [ ] **Step 8: Implement status, nodes and the service core, with interim pairing, handler and connections files**

Create `crates/rocket-app/src/network/status.rs`:

```rust
//! The status shown in the Network panel and in Settings.

use rocket_network::NodeProfile;

use super::service::NetworkService;

/// Shown when the device key cannot be read. There is no key-file fallback.
pub(super) const KEYCHAIN_OFF_REASON: &str = "The keychain is not available, so networking is off.";
/// Shown when the TLS setup fails for another reason.
pub(super) const TRANSPORT_OFF_REASON: &str = "Networking could not start on this computer.";
/// Shown before `start` has run.
pub(super) const NOT_STARTED_REASON: &str = "Networking is starting.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkState {
    Running,
    Off { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkStatus {
    pub state: NetworkState,
    pub port: Option<u16>,
    pub listener_available: bool,
    pub discovery_available: bool,
    pub hidden: bool,
    pub profile: NodeProfile,
    pub device_short_id: Option<String>,
    pub intro_seen: bool,
    pub warnings: Vec<String>,
}

/// The user text for a file that was set aside as damaged. `file` is the file name the
/// repository reports, possibly with the `.corrupt-<secs>` suffix.
pub(super) fn warning_for(file: &str) -> String {
    let name = std::path::Path::new(file)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(file);
    let name = name.split(".corrupt-").next().unwrap_or(name);
    if name == "blocked.yml" {
        "Your blocked devices list was damaged and has been reset.".to_string()
    } else {
        format!("{name} was damaged and has been reset.")
    }
}

impl NetworkService {
    pub fn status(&self) -> NetworkStatus {
        let st = self.lock();
        NetworkStatus {
            state: st.net.clone(),
            port: st.port,
            listener_available: st.listener_available,
            discovery_available: st.discovery_available,
            hidden: st.settings.hidden,
            profile: st.profile.clone(),
            device_short_id: st.runtime.as_ref().map(|r| r.local.device_id.short()),
            intro_seen: st.settings.intro_seen,
            warnings: st.warnings.clone(),
        }
    }
}
```

Create `crates/rocket-app/src/network/nodes.rs`:

```rust
//! The node table, the node list for the panel, the discovery sink and manual probes.
//!
//! A node is listed only after `/hello` proves it: the certificate key hashes to the id that
//! was announced, and the `HelloResponse` says the same id. The first address that answers
//! decides. A wrong answer ends the search, so a stranger cannot borrow another node's id.

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Weak};
use std::time::Duration;

use rocket_network::limits::PROTOCOL_VERSION;
use rocket_network::{
    initials_for, DeviceId, DiscoveryEvent, DiscoverySink, DiscoverySource, HelloResponse,
    NetworkEvent, NodeProfile, PeerConnector, Presence,
};
use tokio::runtime::Handle;

use super::service::NetworkService;
use super::status::NetworkState;

const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_ADDRESSES_PER_NODE: usize = 8;

/// What the node table knows about one verified node.
pub(super) struct Seen {
    pub profile: NodeProfile,
    pub version: u32,
    /// Addresses from mDNS, the one that answered `/hello` first.
    pub mdns: Vec<SocketAddr>,
    /// Addresses from the saved manual list that answered `/hello`.
    pub manual: Vec<SocketAddr>,
}

impl Seen {
    pub(super) fn source(&self) -> DiscoverySource {
        if self.mdns.is_empty() {
            DiscoverySource::Manual
        } else {
            DiscoverySource::Mdns
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeGroup {
    Paired,
    Nearby,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeStatus {
    Online,
    Offline,
    Pairing,
    NeedsUpdate,
    Available,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeView {
    pub device_id: String,
    pub short_id: String,
    pub display_name: String,
    pub initials: String,
    pub group: NodeGroup,
    pub status: NodeStatus,
    pub source: Option<DiscoverySource>,
}

/// Receives mDNS events on the mDNS thread and hands them to the runtime.
pub(super) struct ServiceSink {
    svc: Weak<NetworkService>,
    rt: Handle,
}

impl ServiceSink {
    pub(super) fn new(svc: Weak<NetworkService>, rt: Handle) -> ServiceSink {
        ServiceSink { svc, rt }
    }
}

impl DiscoverySink for ServiceSink {
    fn on_event(&self, event: DiscoveryEvent) {
        let Some(svc) = self.svc.upgrade() else {
            return;
        };
        let _ = self.rt.spawn(async move { svc.handle_discovery(event).await });
    }
}

/// A stable text of the table, to tell whether a change is worth an event.
fn signature(seen: &HashMap<DeviceId, Seen>) -> Vec<String> {
    let mut rows: Vec<String> = seen
        .iter()
        .map(|(id, s)| {
            format!(
                "{}|{}|{}|{:?}|{:?}",
                id.to_base32(),
                s.profile.display_name,
                s.version,
                s.mdns,
                s.manual
            )
        })
        .collect();
    rows.sort();
    rows
}

/// An IP literal needs no lookup. A name (for example a Tailscale name) is resolved.
async fn resolve(host: &str, port: u16) -> Option<SocketAddr> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Some(SocketAddr::new(ip, port));
    }
    let mut found = tokio::net::lookup_host((host, port)).await.ok()?;
    found.next()
}

fn group_rank(group: NodeGroup) -> u8 {
    match group {
        NodeGroup::Paired => 0,
        NodeGroup::Nearby => 1,
    }
}

fn status_rank(status: NodeStatus) -> u8 {
    match status {
        NodeStatus::Online => 0,
        NodeStatus::Pairing => 1,
        NodeStatus::NeedsUpdate => 2,
        NodeStatus::Available => 3,
        NodeStatus::Offline => 4,
    }
}

impl NetworkService {
    /// Calls `/hello` on the addresses in order. The first one that answers decides: it must
    /// be consistent (key and body agree, and the claimed id matches when there is one), or
    /// the result is `None`. The returned profile is re-validated.
    pub(super) async fn verify_hello(
        &self,
        connector: &Arc<dyn PeerConnector>,
        addresses: &[SocketAddr],
        claimed: Option<&DeviceId>,
    ) -> Option<(SocketAddr, HelloResponse)> {
        for address in addresses {
            let Ok(Ok((identity, hello))) =
                tokio::time::timeout(HELLO_TIMEOUT, connector.hello(*address)).await
            else {
                continue;
            };
            let consistent = identity.device_id == hello.device_id
                && claimed.map_or(true, |c| *c == hello.device_id);
            if !consistent {
                return None;
            }
            let profile = hello.profile.validated().ok()?;
            return Some((*address, HelloResponse { profile, ..hello }));
        }
        None
    }

    pub(super) async fn handle_discovery(&self, event: DiscoveryEvent) {
        match event {
            DiscoveryEvent::Seen {
                claimed_id,
                addresses,
                ..
            } => self.on_seen(claimed_id, addresses).await,
            DiscoveryEvent::Gone { claimed_id } => self.on_gone(claimed_id),
        }
    }

    async fn on_seen(&self, claimed: DeviceId, addresses: Vec<SocketAddr>) {
        if self.local_id() == Some(claimed) {
            return;
        }
        let Some(connector) = self.connector() else {
            return;
        };
        let Some((verified, hello)) = self
            .verify_hello(&connector, &addresses, Some(&claimed))
            .await
        else {
            tracing::debug!(
                code = "network_sighting_unverified",
                id = %claimed.short(),
                "a sighting did not verify"
            );
            return;
        };
        let changed = {
            let mut st = self.lock();
            let before = signature(&st.seen);
            let entry = st.seen.entry(claimed).or_insert_with(|| Seen {
                profile: hello.profile.clone(),
                version: hello.version,
                mdns: Vec::new(),
                manual: Vec::new(),
            });
            entry.profile = hello.profile.clone();
            entry.version = hello.version;
            let mut merged = vec![verified];
            for known in &entry.mdns {
                if !merged.contains(known) {
                    merged.push(*known);
                }
            }
            merged.truncate(MAX_ADDRESSES_PER_NODE);
            entry.mdns = merged;
            before != signature(&st.seen)
        };
        // A new sighting means "retry at once" for a paired device.
        self.wake_device(&claimed);
        if changed {
            self.emit(NetworkEvent::NodesChanged);
        }
    }

    fn on_gone(&self, claimed: DeviceId) {
        let changed = {
            let mut st = self.lock();
            let drop_row = match st.seen.get_mut(&claimed) {
                Some(seen) => {
                    seen.mdns.clear();
                    Some(seen.manual.is_empty())
                }
                None => None,
            };
            match drop_row {
                Some(true) => {
                    st.seen.remove(&claimed);
                    true
                }
                Some(false) => true,
                None => false,
            }
        };
        if changed {
            self.emit(NetworkEvent::NodesChanged);
        }
    }

    /// Probes every saved address and replaces the manual addresses in the table.
    pub(super) async fn probe_manual(&self) {
        let Some(connector) = self.connector() else {
            return;
        };
        let entries = match self.deps.addresses.list() {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!(code = "network_addresses_unreadable", error = %e, "probe skipped");
                return;
            }
        };
        let mut found: Vec<(SocketAddr, HelloResponse)> = Vec::new();
        for entry in entries {
            let Some(address) = resolve(&entry.host, entry.port).await else {
                continue;
            };
            if let Some(hit) = self.verify_hello(&connector, &[address], None).await {
                found.push(hit);
            }
        }
        let local = self.local_id();
        let (changed, woken) = {
            let mut st = self.lock();
            let before = signature(&st.seen);
            for seen in st.seen.values_mut() {
                seen.manual.clear();
            }
            let mut woken = Vec::new();
            for (address, hello) in found {
                if Some(hello.device_id) == local {
                    continue;
                }
                let entry = st.seen.entry(hello.device_id).or_insert_with(|| Seen {
                    profile: hello.profile.clone(),
                    version: hello.version,
                    mdns: Vec::new(),
                    manual: Vec::new(),
                });
                entry.profile = hello.profile.clone();
                entry.version = hello.version;
                if !entry.manual.contains(&address) {
                    entry.manual.push(address);
                }
                woken.push(hello.device_id);
            }
            st.seen
                .retain(|_, seen| !(seen.mdns.is_empty() && seen.manual.is_empty()));
            (before != signature(&st.seen), woken)
        };
        for id in woken {
            self.wake_device(&id);
        }
        if changed {
            self.emit(NetworkEvent::NodesChanged);
        }
    }

    /// The name to show for a device: the live name, else the one saved at pairing.
    pub(super) fn known_name(&self, id: &DeviceId) -> String {
        let live = self.lock().seen.get(id).map(|s| s.profile.display_name.clone());
        if let Some(name) = live {
            return name;
        }
        match self.deps.paired.get(id) {
            Ok(Some(device)) => device.name_at_pairing,
            _ => "Unknown device".to_string(),
        }
    }

    #[cfg(test)]
    pub(crate) fn mdns_addresses(&self, id: &DeviceId) -> Vec<SocketAddr> {
        self.lock()
            .seen
            .get(id)
            .map(|s| s.mdns.clone())
            .unwrap_or_default()
    }

    pub fn list_nodes(&self) -> Vec<NodeView> {
        let paired = self.deps.paired.list().unwrap_or_default();
        let blocked: HashSet<DeviceId> = self
            .deps
            .blocked
            .list()
            .unwrap_or_default()
            .into_iter()
            .map(|b| b.device_id)
            .collect();
        let st = self.lock();
        if !matches!(st.net, NetworkState::Running) {
            return Vec::new();
        }
        let local = st.runtime.as_ref().map(|r| r.local.device_id);
        let pairing: HashSet<DeviceId> = st
            .sessions
            .values()
            .filter(|e| !e.session.is_terminal())
            .map(|e| e.session.peer().device_id)
            .collect();
        let mut rows = Vec::new();
        let mut paired_ids = HashSet::new();
        for device in &paired {
            paired_ids.insert(device.device_id);
            if Some(device.device_id) == local {
                continue;
            }
            let seen = st.seen.get(&device.device_id);
            let conn = st.conns.get(&device.device_id);
            let name = seen
                .map(|s| s.profile.display_name.clone())
                .unwrap_or_else(|| device.name_at_pairing.clone());
            let status = if pairing.contains(&device.device_id) {
                NodeStatus::Pairing
            } else if seen.is_some_and(|s| s.version != PROTOCOL_VERSION)
                || conn.is_some_and(|c| c.needs_update)
            {
                NodeStatus::NeedsUpdate
            } else if conn.is_some_and(|c| c.presence.state() == Presence::Online) {
                NodeStatus::Online
            } else {
                NodeStatus::Offline
            };
            rows.push(NodeView {
                device_id: device.device_id.to_base32(),
                short_id: device.device_id.short(),
                initials: initials_for(&name),
                display_name: name,
                group: NodeGroup::Paired,
                status,
                source: seen.map(Seen::source),
            });
        }
        for (id, seen) in &st.seen {
            if paired_ids.contains(id) || blocked.contains(id) || Some(*id) == local {
                continue;
            }
            let status = if pairing.contains(id) {
                NodeStatus::Pairing
            } else if seen.version != PROTOCOL_VERSION {
                NodeStatus::NeedsUpdate
            } else {
                NodeStatus::Available
            };
            rows.push(NodeView {
                device_id: id.to_base32(),
                short_id: id.short(),
                display_name: seen.profile.display_name.clone(),
                initials: seen.profile.initials.clone(),
                group: NodeGroup::Nearby,
                status,
                source: Some(seen.source()),
            });
        }
        rows.sort_by(|a, b| {
            group_rank(a.group)
                .cmp(&group_rank(b.group))
                .then(status_rank(a.status).cmp(&status_rank(b.status)))
                .then_with(|| {
                    a.display_name
                        .to_lowercase()
                        .cmp(&b.display_name.to_lowercase())
                })
                .then_with(|| a.device_id.cmp(&b.device_id))
        });
        rows
    }
}
```

Create `crates/rocket-app/src/network/service.rs`:

```rust
//! The network service: lifecycle, settings, addresses and the shared state of one node.
//!
//! One `Mutex<State>` guards the node table, the pairing sessions and the presence of each
//! paired device. A guard is never held across an `.await`.

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use chrono::Utc;
use rocket_audit::{AuditActor, AuditEventKind, SecurityAuditPublisher};
use rocket_network::limits::{
    MANUAL_PROBE_INTERVAL, MAX_PAIRED_DEVICES, PAIR_REQUESTS_PER_MINUTE, PROTOCOL_VERSION,
};
use rocket_network::{
    default_display_name, initials_for, Announcement, BlockedDevice, BlockedDeviceRepository,
    DeviceId, DeviceIdentityStore, Discovery, DiscoverySink, HelloResponse, ManualAddress,
    ManualAddressRepository, NetworkEvent, NetworkSettings, NetworkSettingsRepository, NodeProfile,
    PairedDeviceRepository, PairingId, PeerConnector, PeerHandler, PeerIdentity, PeerMessage,
    PeerServerControl, PeerTransport, PeerTransportFactory, QuarantineReport, SlidingWindow,
};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::EventPublisher;
use tokio::sync::watch;

use crate::stop_signal::run_every;

use super::connections::ConnEntry;
use super::nodes::{Seen, ServiceSink};
use super::pairing::SessionEntry;
use super::status::{
    warning_for, NetworkState, KEYCHAIN_OFF_REASON, NOT_STARTED_REASON, TRANSPORT_OFF_REASON,
};

/// The text shown when the paired list is full.
pub fn paired_limit_message() -> String {
    format!("You've reached the limit of {MAX_PAIRED_DEVICES} paired devices. Remove one to pair.")
}

/// Everything the service needs from outside. All of it is a port.
pub struct NetworkDeps {
    pub identity: Arc<dyn DeviceIdentityStore>,
    pub paired: Arc<dyn PairedDeviceRepository>,
    pub blocked: Arc<dyn BlockedDeviceRepository>,
    pub addresses: Arc<dyn ManualAddressRepository>,
    pub settings: Arc<dyn NetworkSettingsRepository>,
    pub quarantine: Vec<Arc<dyn QuarantineReport>>,
    pub discovery: Arc<dyn Discovery>,
    pub transport: Arc<dyn PeerTransportFactory>,
    pub events: Arc<dyn EventPublisher>,
    pub audit: Arc<dyn SecurityAuditPublisher>,
    pub os_user_name: Option<String>,
}

/// What exists only while the node runs.
pub(super) struct Runtime {
    pub local: PeerIdentity,
    pub connector: Arc<dyn PeerConnector>,
    pub stop: watch::Receiver<bool>,
    pub rt: tokio::runtime::Handle,
}

pub(super) struct State {
    pub net: NetworkState,
    pub port: Option<u16>,
    pub listener_available: bool,
    pub discovery_available: bool,
    pub settings: NetworkSettings,
    pub profile: NodeProfile,
    pub warnings: Vec<String>,
    pub runtime: Option<Runtime>,
    pub seen: HashMap<DeviceId, Seen>,
    pub sessions: HashMap<PairingId, SessionEntry>,
    pub cooldowns: HashMap<DeviceId, Instant>,
    /// The service-level limit on incoming pairing requests.
    pub pair_window: SlidingWindow,
    pub conns: HashMap<DeviceId, ConnEntry>,
}

pub struct NetworkService {
    me: Weak<NetworkService>,
    pub(super) deps: NetworkDeps,
    state: Mutex<State>,
}

/// Keeps the node running. Dropping it, or calling `stop`, ends every task and closes the
/// listener and discovery.
pub struct NetworkHandle {
    stop: watch::Sender<bool>,
    server: Option<Arc<dyn PeerServerControl>>,
    discovery: Option<Arc<dyn Discovery>>,
    stopped: AtomicBool,
}

impl NetworkHandle {
    /// A handle for a node that is off. It owns nothing but the stop signal.
    fn inert(stop: watch::Sender<bool>) -> NetworkHandle {
        NetworkHandle {
            stop,
            server: None,
            discovery: None,
            stopped: AtomicBool::new(false),
        }
    }

    /// Ends the node. Calling it again does nothing.
    pub fn stop(&self) {
        if self.stopped.swap(true, Ordering::SeqCst) {
            return;
        }
        let _ = self.stop.send(true);
        if let Some(server) = &self.server {
            server.stop();
        }
        if let Some(discovery) = &self.discovery {
            if let Err(e) = discovery.withdraw() {
                tracing::debug!(code = "network_withdraw_failed", error = %e, "withdraw failed");
            }
            discovery.stop();
        }
    }
}

impl Drop for NetworkHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The profile for a saved name, else the OS user name. A name that does not validate falls
/// back to "Rocket user", so the node always has a valid profile.
fn profile_for(saved: Option<&str>, os_user: Option<&str>) -> NodeProfile {
    let name = match saved {
        Some(name) => name.to_string(),
        None => default_display_name(os_user),
    };
    NodeProfile::new(&name).unwrap_or_else(|_| {
        let fallback = default_display_name(None);
        NodeProfile {
            initials: initials_for(&fallback),
            display_name: fallback,
        }
    })
}

/// The window of the service-level pairing limit.
pub(super) const PAIR_REQUEST_WINDOW: Duration = Duration::from_secs(60);

impl NetworkService {
    pub fn new(deps: NetworkDeps) -> Arc<NetworkService> {
        let profile = profile_for(None, deps.os_user_name.as_deref());
        let state = State {
            net: NetworkState::Off {
                reason: NOT_STARTED_REASON.to_string(),
            },
            port: None,
            listener_available: true,
            discovery_available: true,
            settings: NetworkSettings::default(),
            profile,
            warnings: Vec::new(),
            runtime: None,
            seen: HashMap::new(),
            sessions: HashMap::new(),
            cooldowns: HashMap::new(),
            pair_window: SlidingWindow::new(PAIR_REQUESTS_PER_MINUTE as usize, PAIR_REQUEST_WINDOW),
            conns: HashMap::new(),
        };
        Arc::new_cyclic(|me| NetworkService {
            me: me.clone(),
            deps,
            state: Mutex::new(state),
        })
    }

    pub(super) fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The service as an `Arc`, for tasks that outlive one call.
    pub(super) fn arc(&self) -> Option<Arc<NetworkService>> {
        self.me.upgrade()
    }

    /// The tokio clock as a std instant, so paused time works in tests.
    pub(super) fn now() -> Instant {
        tokio::time::Instant::now().into_std()
    }

    pub(super) fn emit(&self, event: NetworkEvent) {
        self.deps.events.publish(&event);
    }

    pub(super) fn audit(&self, actor: AuditActor, kind: AuditEventKind) {
        self.deps.audit.publish(actor.into(), None, kind);
    }

    /// Runs a task on the node's runtime, or on the current one before `start`.
    pub(super) fn spawn<F>(&self, task: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let handle = self.lock().runtime.as_ref().map(|r| r.rt.clone());
        let handle = handle.or_else(|| tokio::runtime::Handle::try_current().ok());
        if let Some(handle) = handle {
            let _ = handle.spawn(task);
        }
    }

    pub(super) fn connector(&self) -> Option<Arc<dyn PeerConnector>> {
        self.lock().runtime.as_ref().map(|r| Arc::clone(&r.connector))
    }

    pub(super) fn local_id(&self) -> Option<DeviceId> {
        self.lock().runtime.as_ref().map(|r| r.local.device_id)
    }

    pub(super) fn hello_response(&self) -> HelloResponse {
        let st = self.lock();
        HelloResponse {
            device_id: st
                .runtime
                .as_ref()
                .map(|r| r.local.device_id)
                .unwrap_or_else(|| DeviceId::from_bytes([0; 32])),
            version: PROTOCOL_VERSION,
            profile: st.profile.clone(),
        }
    }

    pub(super) fn local_hello_message(&self) -> PeerMessage {
        PeerMessage::Hello {
            version: PROTOCOL_VERSION,
            profile: self.lock().profile.clone(),
        }
    }

    /// Paired and not blocked. Any read error counts as not paired.
    pub(super) fn paired_and_not_blocked(&self, device: &DeviceId) -> bool {
        let paired = matches!(self.deps.paired.get(device), Ok(Some(_)));
        paired && matches!(self.deps.blocked.contains(device), Ok(false))
    }

    // ----- lifecycle -----

    /// Loads settings and damaged-file warnings, and clears what a previous run left.
    fn prepare(&self) {
        let settings = match self.deps.settings.load() {
            Ok(settings) => settings,
            Err(e) => {
                tracing::warn!(code = "network_settings_unreadable", error = %e, "using defaults");
                NetworkSettings::default()
            }
        };
        let warnings: Vec<String> = self
            .deps
            .quarantine
            .iter()
            .filter_map(|report| report.take_quarantined())
            .map(|file| warning_for(&file))
            .collect();
        let profile = profile_for(
            settings.display_name.as_deref(),
            self.deps.os_user_name.as_deref(),
        );
        let mut st = self.lock();
        st.settings = settings;
        st.profile = profile;
        st.warnings = warnings;
        st.runtime = None;
        st.port = None;
        st.listener_available = true;
        st.discovery_available = true;
        st.seen.clear();
        st.sessions.clear();
        st.cooldowns.clear();
        st.pair_window.clear();
        st.conns.clear();
    }

    fn set_off(&self, reason: &str) {
        self.lock().net = NetworkState::Off {
            reason: reason.to_string(),
        };
        self.emit(NetworkEvent::StatusChanged);
    }

    /// Starts the node. It never fails: a problem lands in `status()`.
    pub async fn start(self: &Arc<Self>) -> NetworkHandle {
        let (stop_tx, stop_rx) = watch::channel(false);
        let rt = tokio::runtime::Handle::current();
        self.prepare();

        let key = match self.deps.identity.load_or_create() {
            Ok(key) => key,
            Err(e) => {
                tracing::warn!(code = "network_keychain_unavailable", error = %e, "networking is off");
                self.set_off(KEYCHAIN_OFF_REASON);
                return NetworkHandle::inert(stop_tx);
            }
        };
        let built = self.deps.transport.build(&key);
        drop(key);
        let PeerTransport {
            local,
            connector,
            server,
        } = match built {
            Ok(transport) => transport,
            Err(e) => {
                tracing::warn!(code = "network_transport_failed", error = %e, "networking is off");
                self.set_off(TRANSPORT_OFF_REASON);
                return NetworkHandle::inert(stop_tx);
            }
        };
        {
            let mut st = self.lock();
            st.runtime = Some(Runtime {
                local,
                connector,
                stop: stop_rx.clone(),
                rt: rt.clone(),
            });
            st.net = NetworkState::Running;
        }

        let handler: Arc<dyn PeerHandler> = self.clone();
        match server.start(handler).await {
            Ok(port) => {
                let mut st = self.lock();
                st.port = Some(port);
                st.listener_available = true;
            }
            Err(e) => {
                tracing::warn!(code = "network_listener_failed", error = %e, "others cannot connect");
                let mut st = self.lock();
                st.port = None;
                st.listener_available = false;
            }
        }

        let sink: Arc<dyn DiscoverySink> = Arc::new(ServiceSink::new(Arc::downgrade(self), rt));
        let discovery_ok = match self.deps.discovery.start(sink) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(code = "network_discovery_unavailable", error = %e, "no mDNS");
                false
            }
        };
        self.lock().discovery_available = discovery_ok;
        self.announce();

        // Background work and the final events.
        let me = Arc::clone(self);
        let probe_stop = stop_rx.clone();
        self.spawn(async move { me.manual_probe_loop(probe_stop).await });
        self.emit(NetworkEvent::StatusChanged);
        self.emit(NetworkEvent::NodesChanged);

        NetworkHandle {
            stop: stop_tx,
            server: Some(server),
            discovery: Some(Arc::clone(&self.deps.discovery)),
            stopped: AtomicBool::new(false),
        }
    }

    /// Probes once at start, then again after every `MANUAL_PROBE_INTERVAL`, until stopped.
    async fn manual_probe_loop(&self, mut stop: watch::Receiver<bool>) {
        self.probe_manual().await;
        run_every(&mut stop, MANUAL_PROBE_INTERVAL, || self.probe_manual()).await;
    }

    /// Announces this node on mDNS, unless it is hidden, off, without a port or without mDNS.
    pub(super) fn announce(&self) {
        let announcement = {
            let st = self.lock();
            let running = matches!(st.net, NetworkState::Running);
            if !running || st.settings.hidden || !st.discovery_available {
                return;
            }
            let (Some(runtime), Some(port)) = (st.runtime.as_ref(), st.port) else {
                return;
            };
            Announcement {
                device_id: runtime.local.device_id,
                profile: st.profile.clone(),
                port,
                version: PROTOCOL_VERSION,
            }
        };
        if let Err(e) = self.deps.discovery.announce(&announcement) {
            tracing::warn!(code = "network_announce_failed", error = %e, "announce failed");
        }
    }

    fn is_running(&self) -> bool {
        matches!(self.lock().net, NetworkState::Running)
    }

    // ----- settings -----

    /// Applies `change` to a copy of the settings, saves it, then keeps it in memory.
    fn update_settings(
        &self,
        change: impl FnOnce(&mut NetworkSettings),
    ) -> DomainResult<NetworkSettings> {
        let mut settings = self.lock().settings.clone();
        change(&mut settings);
        self.deps.settings.save(&settings)?;
        self.lock().settings = settings.clone();
        Ok(settings)
    }

    pub async fn set_hidden(self: &Arc<Self>, hidden: bool) -> DomainResult<()> {
        self.update_settings(|s| s.hidden = hidden)?;
        if self.is_running() {
            if hidden {
                if let Err(e) = self.deps.discovery.withdraw() {
                    tracing::warn!(code = "network_withdraw_failed", error = %e, "withdraw failed");
                }
            } else {
                self.announce();
            }
        }
        self.emit(NetworkEvent::StatusChanged);
        Ok(())
    }

    pub async fn set_display_name(self: &Arc<Self>, name: &str) -> DomainResult<()> {
        let profile = NodeProfile::new(name)?;
        let saved = profile.display_name.clone();
        self.update_settings(|s| s.display_name = Some(saved))?;
        self.lock().profile = profile;
        self.announce();
        self.emit(NetworkEvent::StatusChanged);
        Ok(())
    }

    pub fn mark_intro_seen(&self) -> DomainResult<()> {
        self.update_settings(|s| s.intro_seen = true)?;
        self.emit(NetworkEvent::StatusChanged);
        Ok(())
    }

    // ----- manual addresses and the block list -----

    pub fn list_addresses(&self) -> DomainResult<Vec<ManualAddress>> {
        self.deps.addresses.list()
    }

    pub async fn add_address(self: &Arc<Self>, host: &str, port: u16) -> DomainResult<()> {
        let host = host.trim();
        let bad_host = host.is_empty()
            || host.len() > 253
            || host
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || c == '/' || c == '@');
        if bad_host || port == 0 {
            return Err(DomainError::InvalidInput(
                "Enter a host name or IP address and a port.".to_string(),
            ));
        }
        self.deps.addresses.add(ManualAddress {
            host: host.to_string(),
            port,
            added_at: Utc::now(),
        })?;
        self.probe_manual().await;
        Ok(())
    }

    pub fn remove_address(&self, host: &str, port: u16) -> DomainResult<()> {
        self.deps.addresses.remove(host.trim(), port)?;
        if let Some(me) = self.arc() {
            self.spawn(async move { me.probe_manual().await });
        }
        Ok(())
    }

    pub fn list_blocked(&self) -> DomainResult<Vec<BlockedDevice>> {
        self.deps.blocked.list()
    }

    pub fn unblock(&self, device_id: &DeviceId) -> DomainResult<()> {
        self.deps.blocked.remove(device_id)?;
        // The Nearby list may show this device again, and Settings lists the blocked ones.
        self.emit(NetworkEvent::NodesChanged);
        self.emit(NetworkEvent::StatusChanged);
        Ok(())
    }
}
```

Create the interim files. Task 2 replaces all three completely.

`crates/rocket-app/src/network/pairing.rs`:

```rust
//! Pairing sessions. Interim version from Task 1: the public enums and the session entry.
//! Task 2 replaces this file with the full flow.

use std::net::SocketAddr;
use std::time::Instant;

use rocket_network::{PairingCode, PairingId, PairingSession};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingDecision {
    Accept,
    Decline,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingStarted {
    pub pairing_id: PairingId,
    pub code: PairingCode,
}

/// One open or finished pairing, seen from this node.
pub(super) struct SessionEntry {
    pub session: PairingSession,
    /// The address the initiator reached the peer on. `None` for a responder.
    pub addr: Option<SocketAddr>,
    pub created: Instant,
    /// `PairingFinished` was published.
    pub notified: bool,
    /// The paired device was saved.
    pub saved: bool,
    /// Saving has started, so it runs once.
    pub claimed: bool,
}
```

`crates/rocket-app/src/network/connections.rs`:

```rust
//! Presence connections. Interim version from Task 1: the entry type and the wake helper.
//! Task 2 replaces this file with the per-device tasks.

use std::sync::Arc;

use rocket_network::{DeviceId, PeerMessage, PeerSocket, PresenceTracker};
use tokio::sync::{mpsc, Notify};

use super::service::NetworkService;

/// The live state of one paired device's connection.
pub(super) struct ConnEntry {
    pub presence: PresenceTracker,
    /// Sockets the peer opened to us arrive here.
    pub inbox: mpsc::Sender<PeerSocket>,
    /// Pinged when a new sighting says "retry at once".
    pub wake: Arc<Notify>,
    pub outbound: Option<mpsc::Sender<PeerMessage>>,
    pub dialed_by: Option<DeviceId>,
    pub needs_update: bool,
}

impl NetworkService {
    /// Asks the task of a paired device to retry now.
    pub(super) fn wake_device(&self, id: &DeviceId) {
        if let Some(entry) = self.lock().conns.get(id) {
            entry.wake.notify_one();
        }
    }
}
```

`crates/rocket-app/src/network/handler.rs`:

```rust
//! The peer handler. Interim version from Task 1: hello and the paired check.
//! The pairing calls refuse everything until Task 2 replaces this file.

use std::net::IpAddr;

use async_trait::async_trait;
use rocket_network::{
    DecisionRequest, DeviceId, HelloResponse, NonceMessage, PairRequest, PairRequestResponse,
    PairStatusResponse, PairingId, PeerError, PeerHandler, PeerIdentity, PeerMessage, PeerSocket,
};

use super::service::NetworkService;

#[async_trait]
impl PeerHandler for NetworkService {
    async fn hello(&self) -> HelloResponse {
        self.hello_response()
    }

    async fn pair_request(
        &self,
        _peer: PeerIdentity,
        _remote_ip: IpAddr,
        _req: PairRequest,
    ) -> Result<PairRequestResponse, PeerError> {
        Err(PeerError::Forbidden)
    }

    async fn pair_nonce(
        &self,
        _peer: PeerIdentity,
        _id: PairingId,
        _msg: NonceMessage,
    ) -> Result<NonceMessage, PeerError> {
        Err(PeerError::Forbidden)
    }

    async fn pair_decision(
        &self,
        _peer: PeerIdentity,
        _id: PairingId,
        _req: DecisionRequest,
    ) -> Result<(), PeerError> {
        Err(PeerError::Forbidden)
    }

    async fn pair_status(
        &self,
        _peer: PeerIdentity,
        _id: PairingId,
    ) -> Result<PairStatusResponse, PeerError> {
        Err(PeerError::Forbidden)
    }

    async fn is_paired(&self, device: &DeviceId) -> bool {
        self.paired_and_not_blocked(device)
    }

    async fn on_peer_socket(&self, socket: PeerSocket) {
        drop(socket);
    }

    fn local_hello(&self) -> PeerMessage {
        self.local_hello_message()
    }
}
```

- [ ] **Step 9: Run the service tests and see them pass**

Run: `cargo test -j4 -p rocket-app network`
Expected: PASS, the 26 tests in `network::service_tests` pass, 0 failed. Warnings about unused fields (`created`, `notified`, `saved`, `claimed`, `inbox`, `outbound`, `dialed_by`) and unused imports are expected until Task 2 uses them; fix any real error. If `a_stranger_spoofing_an_mdns_id_is_never_listed` fails with 0 hello calls, the sink ran before `start` stored the runtime. If a paused-time test hangs, a guard is held across an `.await`.

- [ ] **Step 10: Check the crate and the workspace**

Run: `cargo check -j4 -p rocket-app`
Expected: Finished, no errors.

Run: `cargo test -j4 -p rocket-app stop_signal` and `cargo test -j4 -p rocket-app openapi_sync_scheduler`
Expected: PASS. The 6 `stop_signal` tests pass, and the existing scheduler tests still pass with the shared `sleep_or_stop`.

Run: `cargo test -j4 -p rocket-audit`
Expected: PASS.

No commit; reviewed at the plan end.

---

### Task 2: Pairing, the peer handler and presence connections

**Files:**
- Modify: `crates/rocket-app/src/network/service.rs` (three small edits)
- Replace: `crates/rocket-app/src/network/pairing.rs`, `handler.rs`, `connections.rs` (full content below)
- Fill: `crates/rocket-app/src/network/pairing_tests.rs`, `connections_tests.rs`

**Interfaces:**

```rust
pub enum PairingDecision { Accept, Decline, Block }
pub struct PairingStarted { pub pairing_id: PairingId, pub code: PairingCode }
impl NetworkService {
    pub async fn request_pairing(self: &Arc<Self>, device_id: &DeviceId) -> DomainResult<PairingStarted>;
    pub async fn respond_pairing(self: &Arc<Self>, id: &PairingId, decision: PairingDecision) -> DomainResult<()>;
    pub fn cancel_pairing(&self, id: &PairingId) -> DomainResult<()>;
    pub async fn remove_paired(self: &Arc<Self>, device_id: &DeviceId) -> DomainResult<()>;
}
// impl PeerHandler for NetworkService (hello, pair_request, pair_nonce, pair_decision,
// pair_status, is_paired, on_peer_socket, local_hello)
```

Rules the code and tests pin:

- **Initiator** (`request_pairing`): checks networking is on, the device is not already paired, the paired list is not full, the device is not blocked, no session is open with it, and it is in the node table. It calls `/hello` on the addresses (deviation 8) and requires key and body to match the device id being paired. Then `pair_request`, then the commitment check through `PairingSession`, `pair_nonce`, `receive_reveal`. A mismatch ends the session as `failed`, tells the peer to drop its session, saves nothing and shows "Pairing failed. Try again." A `Busy` answer shows "The other device is busy. Try again in a minute."
- **Both roles** use `respond_pairing`. The initiator sends its decision, then a background task polls `pair_status` every second until the answer is final or the session expires. The responder decides locally; when the initiator already accepted, the pair completes at once.
- **Every terminal outcome publishes `PairingFinished` exactly once on each node**, including on the node whose user pressed Decline or Block (`declined`) or Cancel (`cancelled`). A peer that stops answering partway (the decision call fails, or ten polls in a row fail) ends the session with `PairingSession::fail()` and publishes `failed`, never `cancelled`. The sweeper (every 5 s) and every session access expire due sessions, so an expired request is announced even when nobody acts.
- **Saving a pairing**, an unpair, a block and an unblock each publish `NodesChanged` so the panel refetches.
- **Responder checks** in `pair_request`, in this order: version, profile, blocked, hidden or off, self, 200 limit (a known device may re-pair), decline cooldown, one open session per device, 5 per minute across strangers. No notice is shown until `pair_nonce` succeeds.
- **A late remote decision** on a finished session is acknowledged and ignored. `pair_status` still answers, so the initiator learns the outcome.
- **Connections**: one task per paired device. It dials when `should_dial` says so (or immediately for a manual address), waits `retry_delay` after a drop, pings every `PING_INTERVAL`, answers pings, removes the pairing on `Unpair` or on `NotPaired`, and keeps the socket dialled by the lower id when two exist. An impostor at a saved address (`IdentityMismatch`) removes nothing.
- A small `paired_lock` serialises writes that must not undo a removal (remembering the last good address versus removing the pairing).
- **Block on a paired device** (decided by the user on 2026-11-06): `block_peer` adds the block, then, when the device is paired, runs the same unpair as Remove (`unpair_by_user`): it sends `Unpair` on the live socket, removes the `PairedDevice`, drops the connection entry (which closes the socket), publishes `PairedRemoved { by_peer: false }` and `NodesChanged`, and records `NetworkDeviceBlocked` and `NetworkDeviceUnpaired { by_peer: false }`. `run_socket` checks the block list before it handles any frame, so a frame from a blocked id gets no answer and closes the socket (fail closed when the list cannot be read). Tests: `blocking_a_paired_device_unpairs_it_and_closes_its_socket`, `frames_from_a_blocked_device_get_no_answer_and_close_the_socket`.

- [ ] **Step 1: Write the failing pairing tests**

Replace `crates/rocket-app/src/network/pairing_tests.rs`:

```rust
//! Pairing flow and peer-handler tests. Two real services talk through in-memory fakes.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use rocket_audit::AuditEventKind;
use rocket_network::limits::{DECLINE_COOLDOWN, PAIRING_TTL, PROTOCOL_VERSION};
use rocket_network::{
    commitment, commitment_to_base64, pairing_code, DecisionRequest, DiscoveryEvent, HelloResponse,
    NetworkEvent, NodeProfile, Nonce, NonceMessage, PairRequest, PairRequestResponse,
    PairStatusResponse, PairingId, PairingStatus, PeerError, PeerHandler, PeerIdentity,
    PeerMessage, PeerSocket,
};
use rocket_shared::error::DomainError;
use std::net::IpAddr;
use std::sync::Arc;

use super::test_support::*;
use super::{paired_limit_message, NetworkHandle, NodeGroup, PairingDecision};

/// Two started nodes that see each other.
async fn ready() -> (Node, Node, NetworkHandle, NetworkHandle) {
    let a = node(1, "ann");
    let b = node(2, "ben");
    link(&a, &b);
    let ha = a.svc.start().await;
    let hb = b.svc.start().await;
    see(&a, &b);
    see(&b, &a);
    settle().await;
    (a, b, ha, hb)
}

fn audited(n: &Node) -> Vec<AuditEventKind> {
    n.audit.records().into_iter().map(|r| r.kind).collect()
}

fn nodes_changed_since(n: &Node, mark: usize) -> bool {
    n.events.all()[mark..].contains(&NetworkEvent::NodesChanged)
}

// ----- the happy paths -----

#[tokio::test(start_paused = true)]
async fn both_accept_and_each_side_saves_the_other() {
    let (a, b, _ha, _hb) = ready().await;
    let started = a.svc.request_pairing(&b.id()).await.expect("request");
    settle().await;

    let incoming = b.events.incoming();
    assert_eq!(incoming.len(), 1, "one notice on the responder");
    let NetworkEvent::PairingIncoming {
        pairing_id,
        device_id,
        short_id,
        display_name,
        code,
    } = &incoming[0]
    else {
        panic!("not an incoming notice");
    };
    assert_eq!(pairing_id, &started.pairing_id.0);
    assert_eq!(device_id, &a.id().to_base32());
    assert_eq!(short_id, &a.id().short());
    assert_eq!(display_name, "ann");
    assert_eq!(code, &started.code.digits(), "both screens show the same code");
    assert_eq!(code.len(), 6);

    // The responder accepts first. Nothing is saved until the initiator accepts too.
    let mark_b = b.events.all().len();
    b.svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect("responder accepts");
    assert!(!b.paired.contains(&a.id()));
    assert!(b.events.finished().is_empty());

    let mark_a = a.events.all().len();
    a.svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect("initiator accepts");
    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;

    assert!(a.paired.contains(&b.id()));
    assert!(b.paired.contains(&a.id()));
    assert_eq!(a.events.finished(), vec!["accepted"]);
    assert_eq!(b.events.finished(), vec!["accepted"]);
    assert!(nodes_changed_since(&a, mark_a), "the panel refetches on the initiator");
    assert!(nodes_changed_since(&b, mark_b), "the panel refetches on the responder");
    assert!(audited(&a).contains(&AuditEventKind::NetworkDevicePaired {
        device_id: b.id().to_base32()
    }));
    assert!(audited(&b).contains(&AuditEventKind::NetworkDevicePaired {
        device_id: a.id().to_base32()
    }));
    let rows = a.svc.list_nodes();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].group, NodeGroup::Paired);
    assert_eq!(rows[0].display_name, "ben");

    // The pair then connects on its own.
    tokio::time::advance(Duration::from_secs(15)).await;
    settle().await;
    assert_eq!(row_status(&a, &b.id()), Some(super::NodeStatus::Online));
    assert_eq!(row_status(&b, &a.id()), Some(super::NodeStatus::Online));
}

#[tokio::test(start_paused = true)]
async fn the_responder_keeps_the_pairing_while_the_initiator_is_still_saving() {
    let x = node(1, "ann");
    let y = node(2, "ben");
    link(&x, &y);
    let (low, high) = ordered(&x, &y);
    let _hl = low.svc.start().await;
    let _hh = high.svc.start().await;
    see(low, high);
    see(high, low);
    settle().await;

    // The higher node starts, so the lower node is the responder and dials at once.
    let started = high.svc.request_pairing(&low.id()).await.expect("request");
    settle().await;
    accept_both(&low.svc, &high.svc, &started.pairing_id).await;
    settle().await;

    // The initiator has not saved yet, so it answers "not paired". That is not a removal.
    assert!(low.paired.contains(&high.id()));
    assert!(low.events.removed().is_empty());

    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;
    assert!(high.paired.contains(&low.id()), "the initiator's poll saved it");
    tokio::time::advance(Duration::from_secs(15)).await;
    settle().await;
    assert_eq!(row_status(low, &high.id()), Some(super::NodeStatus::Online));
    assert_eq!(row_status(high, &low.id()), Some(super::NodeStatus::Online));
}

#[tokio::test(start_paused = true)]
async fn the_initiator_can_accept_before_the_responder() {
    let (a, b, _ha, _hb) = ready().await;
    let started = a.svc.request_pairing(&b.id()).await.expect("request");
    settle().await;

    a.svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect("initiator accepts");
    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;
    assert!(!a.paired.contains(&b.id()), "still waiting for the other side");
    assert_eq!(
        row_status(&a, &b.id()),
        Some(super::NodeStatus::Pairing),
        "the row shows Pairing while the session is open"
    );

    b.svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect("responder accepts");
    assert!(b.paired.contains(&a.id()), "the responder saves at once");
    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;

    assert!(a.paired.contains(&b.id()));
    assert_eq!(a.events.finished(), vec!["accepted"]);
    assert_eq!(b.events.finished(), vec!["accepted"]);
}

#[tokio::test(start_paused = true)]
async fn a_decline_ends_both_sides_and_starts_a_cooldown() {
    let (a, b, _ha, _hb) = ready().await;
    let started = a.svc.request_pairing(&b.id()).await.expect("request");
    settle().await;
    a.svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect("initiator accepts");

    b.svc
        .respond_pairing(&started.pairing_id, PairingDecision::Decline)
        .await
        .expect("responder declines");
    assert_eq!(b.events.finished(), vec!["declined"], "the decliner is told too");

    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;
    assert_eq!(a.events.finished(), vec!["declined"]);
    assert!(!a.paired.contains(&b.id()));
    assert!(!b.paired.contains(&a.id()));

    // Inside the cooldown the responder says busy.
    let err = a
        .svc
        .request_pairing(&b.id())
        .await
        .expect_err("cooldown");
    assert_eq!(
        err,
        DomainError::InvalidInput("The other device is busy. Try again in a minute.".into())
    );

    // After the cooldown a new request works.
    tokio::time::advance(DECLINE_COOLDOWN + Duration::from_secs(1)).await;
    settle().await;
    a.svc
        .request_pairing(&b.id())
        .await
        .expect("allowed after the cooldown");
}

#[tokio::test(start_paused = true)]
async fn blocking_refuses_the_device_until_it_is_unblocked() {
    let (a, b, _ha, _hb) = ready().await;
    let started = a.svc.request_pairing(&b.id()).await.expect("request");
    settle().await;
    a.svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect("initiator accepts");

    let mark = b.events.all().len();
    b.svc
        .respond_pairing(&started.pairing_id, PairingDecision::Block)
        .await
        .expect("responder blocks");
    assert_eq!(b.events.finished(), vec!["declined"]);
    assert!(nodes_changed_since(&b, mark));
    assert!(b.blocked.has(&a.id()));
    assert!(audited(&b).contains(&AuditEventKind::NetworkDeviceBlocked {
        device_id: a.id().to_base32()
    }));

    // The sender sees the same "declined" as for a plain decline.
    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;
    assert_eq!(a.events.finished(), vec!["declined"]);

    // A new request from the blocked device is refused outright.
    let refused = b
        .svc
        .pair_request(a.identity.clone(), ip(), pair_request("ann"))
        .await
        .expect_err("blocked");
    assert_eq!(refused, PeerError::Forbidden);

    // Unblocking lifts it (after the cooldown, which is separate).
    b.svc.unblock(&a.id()).expect("unblock");
    tokio::time::advance(DECLINE_COOLDOWN + Duration::from_secs(1)).await;
    settle().await;
    b.svc
        .pair_request(a.identity.clone(), ip(), pair_request("ann"))
        .await
        .expect("allowed again");
}

#[tokio::test(start_paused = true)]
async fn cancelling_as_the_initiator_tells_the_peer() {
    let (a, b, _ha, _hb) = ready().await;
    let started = a.svc.request_pairing(&b.id()).await.expect("request");
    settle().await;
    assert_eq!(b.events.incoming().len(), 1);

    a.svc.cancel_pairing(&started.pairing_id).expect("cancel");
    settle().await;

    assert_eq!(a.events.finished(), vec!["cancelled"]);
    assert_eq!(b.events.finished(), vec!["declined"], "the peer's notice closes");
    let err = a
        .svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect_err("session is over");
    assert_eq!(
        err,
        DomainError::InvalidInput("That pairing request is no longer open.".into())
    );
    a.svc
        .cancel_pairing(&started.pairing_id)
        .expect("cancelling twice is fine");
    assert_eq!(a.events.finished(), vec!["cancelled"], "published once");

    let unknown = a
        .svc
        .cancel_pairing(&PairingId::new())
        .expect_err("unknown session");
    assert!(matches!(unknown, DomainError::InvalidInput(_)));
}

#[tokio::test(start_paused = true)]
async fn the_responder_can_cancel_too() {
    let (a, b, _ha, _hb) = ready().await;
    let started = a.svc.request_pairing(&b.id()).await.expect("request");
    settle().await;
    b.svc.cancel_pairing(&started.pairing_id).expect("cancel");
    assert_eq!(b.events.finished(), vec!["cancelled"]);
}

// ----- failures partway end as failed, not cancelled -----

#[tokio::test(start_paused = true)]
async fn a_decision_that_cannot_be_sent_ends_the_pairing_as_failed() {
    let (a, b, _ha, _hb) = ready().await;
    let started = a.svc.request_pairing(&b.id()).await.expect("request");
    settle().await;

    // The responder drops off the network before the initiator accepts.
    a.connector.remove_route(b.addr());
    let err = a
        .svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect_err("the decision cannot reach the peer");
    assert_eq!(
        err,
        DomainError::InvalidInput("Pairing failed. Try again.".into())
    );
    settle().await;

    assert_eq!(a.events.finished(), vec!["failed"], "a failure, not a cancel");
    assert!(!a.paired.contains(&b.id()), "nothing is saved");
}

#[tokio::test(start_paused = true)]
async fn a_responder_that_stops_answering_while_polled_ends_the_pairing_as_failed() {
    let (a, b, _ha, _hb) = ready().await;
    let started = a.svc.request_pairing(&b.id()).await.expect("request");
    settle().await;
    a.svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect("initiator accepts");
    settle().await;

    // The responder vanishes while the initiator polls for its answer.
    a.connector.remove_route(b.addr());
    for _ in 0..12 {
        tokio::time::advance(Duration::from_secs(1)).await;
        settle().await;
    }

    assert_eq!(a.events.finished(), vec!["failed"], "a failure, not a cancel");
    assert!(!a.paired.contains(&b.id()), "nothing is saved");
}

// ----- refusals on the initiator side -----

#[tokio::test(start_paused = true)]
async fn pairing_is_refused_while_networking_is_off() {
    let a = node(1, "ann");
    a.id_store.fail.store(true, Ordering::SeqCst);
    let _ha = a.svc.start().await;

    let err = a
        .svc
        .request_pairing(&identity(2).device_id)
        .await
        .expect_err("off");
    assert_eq!(err, DomainError::InvalidInput("Networking is off.".into()));
    let refused = a
        .svc
        .pair_request(identity(2), ip(), pair_request("Ben"))
        .await
        .expect_err("off");
    assert_eq!(refused, PeerError::Forbidden);
}

#[tokio::test(start_paused = true)]
async fn a_device_that_is_not_nearby_cannot_be_paired() {
    let a = node(1, "ann");
    let _ha = a.svc.start().await;
    let err = a
        .svc
        .request_pairing(&identity(2).device_id)
        .await
        .expect_err("unknown");
    assert_eq!(
        err,
        DomainError::InvalidInput("That device is no longer nearby.".into())
    );
}

#[tokio::test(start_paused = true)]
async fn a_paired_or_blocked_device_cannot_be_requested_again() {
    let (a, b, _ha, _hb) = ready().await;
    a.paired.seed(b.id(), "ben");
    let err = a.svc.request_pairing(&b.id()).await.expect_err("paired");
    assert_eq!(
        err,
        DomainError::InvalidInput("This device is already paired.".into())
    );

    let (c, d, _hc, _hd) = {
        let c = node(3, "cat");
        let d = node(4, "dan");
        link(&c, &d);
        let hc = c.svc.start().await;
        let hd = d.svc.start().await;
        see(&c, &d);
        settle().await;
        (c, d, hc, hd)
    };
    c.blocked.put(d.id(), "dan");
    let err = c.svc.request_pairing(&d.id()).await.expect_err("blocked");
    assert_eq!(
        err,
        DomainError::InvalidInput("This device is blocked. Unblock it in Settings to pair.".into())
    );
}

#[tokio::test(start_paused = true)]
async fn the_201st_outgoing_request_shows_the_limit_message() {
    let (a, b, _ha, _hb) = ready().await;
    a.paired.fill(200);

    let err = a.svc.request_pairing(&b.id()).await.expect_err("full");
    assert_eq!(err, DomainError::InvalidInput(paired_limit_message()));
    assert_eq!(
        paired_limit_message(),
        "You've reached the limit of 200 paired devices. Remove one to pair."
    );
    assert_eq!(a.connector.pair_request_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn a_hidden_responder_refuses_new_requests_but_still_answers_hello() {
    let (a, b, _ha, _hb) = ready().await;
    b.svc.set_hidden(true).await.expect("hide");

    let err = a.svc.request_pairing(&b.id()).await.expect_err("hidden");
    assert_eq!(
        err,
        DomainError::InvalidInput("Pairing failed. Try again.".into())
    );
    assert!(b.events.incoming().is_empty());
    assert_eq!(b.svc.hello().await.device_id, b.id());
}

#[tokio::test(start_paused = true)]
async fn a_responder_that_lies_about_its_nonce_fails_the_pairing() {
    // A responder that commits to one value and reveals another.
    struct Liar {
        identity: PeerIdentity,
        declines: AtomicUsize,
    }

    #[async_trait]
    impl PeerHandler for Liar {
        async fn hello(&self) -> HelloResponse {
            HelloResponse {
                device_id: self.identity.device_id,
                version: PROTOCOL_VERSION,
                profile: profile("Liar"),
            }
        }
        async fn pair_request(
            &self,
            _peer: PeerIdentity,
            _ip: IpAddr,
            _req: PairRequest,
        ) -> Result<PairRequestResponse, PeerError> {
            Ok(PairRequestResponse {
                pairing_id: PairingId::new(),
                commitment: commitment_to_base64(&[1u8; 32]),
            })
        }
        async fn pair_nonce(
            &self,
            _peer: PeerIdentity,
            _id: PairingId,
            _msg: NonceMessage,
        ) -> Result<NonceMessage, PeerError> {
            Ok(NonceMessage {
                nonce: Nonce::random().to_base64(),
            })
        }
        async fn pair_decision(
            &self,
            _peer: PeerIdentity,
            _id: PairingId,
            req: DecisionRequest,
        ) -> Result<(), PeerError> {
            assert!(!req.accept, "the initiator must decline");
            self.declines.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn pair_status(
            &self,
            _peer: PeerIdentity,
            _id: PairingId,
        ) -> Result<PairStatusResponse, PeerError> {
            Ok(PairStatusResponse {
                status: PairingStatus::Declined,
            })
        }
        async fn is_paired(&self, _device: &rocket_network::DeviceId) -> bool {
            false
        }
        async fn on_peer_socket(&self, _socket: PeerSocket) {}
        fn local_hello(&self) -> PeerMessage {
            PeerMessage::Hello {
                version: PROTOCOL_VERSION,
                profile: profile("Liar"),
            }
        }
    }

    let a = node(1, "ann");
    let liar = Arc::new(Liar {
        identity: identity(7),
        declines: AtomicUsize::new(0),
    });
    a.connector.add_route(
        addr(7),
        Route {
            identity: liar.identity.clone(),
            profile: profile("Liar"),
            version: PROTOCOL_VERSION,
            handler: Some(liar.clone()),
            claims: None,
        },
    );
    let _ha = a.svc.start().await;
    a.discovery.play(DiscoveryEvent::Seen {
        claimed_id: liar.identity.device_id,
        profile: profile("Liar"),
        addresses: vec![addr(7)],
        version: PROTOCOL_VERSION,
    });
    settle().await;

    let err = a
        .svc
        .request_pairing(&liar.identity.device_id)
        .await
        .expect_err("commitment mismatch");
    assert_eq!(
        err,
        DomainError::InvalidInput("Pairing failed. Try again.".into())
    );
    assert_eq!(a.events.finished(), vec!["failed"]);
    assert_eq!(a.paired.count(), 0, "nothing is saved");
    assert_eq!(liar.declines.load(Ordering::SeqCst), 1, "the peer is told to drop it");
}

// ----- the handler, called as the peer server would call it -----

#[tokio::test(start_paused = true)]
async fn the_notice_appears_only_after_the_nonce_and_shows_the_right_code() {
    let b = node(2, "ben");
    let _hb = b.svc.start().await;
    let stranger = identity(10);

    let reply = b
        .svc
        .pair_request(stranger.clone(), ip(), pair_request("Mallory"))
        .await
        .expect("request");
    assert!(b.events.incoming().is_empty(), "no notice after the request alone");

    let na = Nonce::random();
    let revealed = b
        .svc
        .pair_nonce(
            stranger.clone(),
            reply.pairing_id.clone(),
            NonceMessage {
                nonce: na.to_base64(),
            },
        )
        .await
        .expect("nonce");
    let nb = Nonce::parse_base64(&revealed.nonce).expect("nb");

    // The commitment from the first reply matches the revealed nonce.
    let expected_commitment = commitment(&b.identity.spki_der, &stranger.spki_der, &nb);
    assert_eq!(commitment_to_base64(&expected_commitment), reply.commitment);

    let incoming = b.events.incoming();
    assert_eq!(incoming.len(), 1);
    let NetworkEvent::PairingIncoming {
        code, display_name, ..
    } = &incoming[0]
    else {
        panic!("not an incoming notice");
    };
    let expected = pairing_code(&stranger.spki_der, &b.identity.spki_der, &na, &nb);
    assert_eq!(code, &expected.digits(), "initiator key first, responder key second");
    assert_eq!(display_name, "Mallory");
}

#[tokio::test(start_paused = true)]
async fn only_the_key_that_opened_a_session_can_use_it() {
    let (a, b, _ha, _hb) = ready().await;
    let started = a.svc.request_pairing(&b.id()).await.expect("request");
    settle().await;
    let intruder = identity(30);
    let id = started.pairing_id.clone();

    let nonce = b
        .svc
        .pair_nonce(
            intruder.clone(),
            id.clone(),
            NonceMessage {
                nonce: Nonce::random().to_base64(),
            },
        )
        .await
        .expect_err("wrong key");
    assert_eq!(nonce, PeerError::Forbidden);
    let decision = b
        .svc
        .pair_decision(intruder.clone(), id.clone(), DecisionRequest { accept: true })
        .await
        .expect_err("wrong key");
    assert_eq!(decision, PeerError::Forbidden);
    let status = b
        .svc
        .pair_status(intruder, id.clone())
        .await
        .expect_err("wrong key");
    assert_eq!(status, PeerError::Forbidden);

    let unknown = b
        .svc
        .pair_status(a.identity.clone(), PairingId::new())
        .await
        .expect_err("unknown id");
    assert_eq!(unknown, PeerError::NotFound);
}

#[tokio::test(start_paused = true)]
async fn the_201st_incoming_request_is_forbidden() {
    let b = node(2, "ben");
    let _hb = b.svc.start().await;
    b.paired.fill(199);
    b.paired.seed(identity(11).device_id, "Known");
    assert_eq!(b.paired.count(), 200);

    let refused = b
        .svc
        .pair_request(identity(10), ip(), pair_request("Newcomer"))
        .await
        .expect_err("list is full");
    assert_eq!(refused, PeerError::Forbidden);
    assert!(b.events.incoming().is_empty());

    // A device that is already paired may pair again.
    b.svc
        .pair_request(identity(11), ip(), pair_request("Known"))
        .await
        .expect("a known device is not a 201st");
}

#[tokio::test(start_paused = true)]
async fn the_sixth_request_in_a_minute_is_busy_and_the_window_slides() {
    let b = node(2, "ben");
    let _hb = b.svc.start().await;
    for seed in 10..15u8 {
        b.svc
            .pair_request(identity(seed), ip(), pair_request("Stranger"))
            .await
            .expect("within the limit");
    }
    let busy = b
        .svc
        .pair_request(identity(15), ip(), pair_request("Stranger"))
        .await
        .expect_err("sixth");
    assert_eq!(busy, PeerError::Busy);
    assert!(b.events.incoming().is_empty(), "a refused request shows no notice");

    tokio::time::advance(Duration::from_secs(61)).await;
    settle().await;
    b.svc
        .pair_request(identity(15), ip(), pair_request("Stranger"))
        .await
        .expect("a minute later");
}

#[tokio::test(start_paused = true)]
async fn one_open_session_per_remote_device() {
    let b = node(2, "ben");
    let _hb = b.svc.start().await;
    b.svc
        .pair_request(identity(10), ip(), pair_request("Stranger"))
        .await
        .expect("first");
    let second = b
        .svc
        .pair_request(identity(10), ip(), pair_request("Stranger"))
        .await
        .expect_err("second");
    assert_eq!(second, PeerError::Busy);
}

#[tokio::test(start_paused = true)]
async fn a_bad_version_or_profile_is_a_bad_request() {
    let b = node(2, "ben");
    let _hb = b.svc.start().await;

    let version = b
        .svc
        .pair_request(
            identity(10),
            ip(),
            PairRequest {
                version: PROTOCOL_VERSION + 1,
                profile: profile("Old"),
            },
        )
        .await
        .expect_err("version");
    assert_eq!(version, PeerError::BadRequest);

    for name in ["", "bad\u{7}name", &"x".repeat(41)] {
        let bad = b
            .svc
            .pair_request(
                identity(11),
                ip(),
                PairRequest {
                    version: PROTOCOL_VERSION,
                    profile: NodeProfile {
                        display_name: name.to_string(),
                        initials: "ZZZ".to_string(),
                    },
                },
            )
            .await
            .expect_err("profile");
        assert_eq!(bad, PeerError::BadRequest, "{name:?}");
    }
}

// ----- expiry -----

#[tokio::test(start_paused = true)]
async fn a_decision_after_expiry_saves_nothing_and_says_expired() {
    let (a, b, _ha, _hb) = ready().await;
    let started = a.svc.request_pairing(&b.id()).await.expect("request");
    settle().await;

    tokio::time::advance(PAIRING_TTL + Duration::from_secs(1)).await;
    settle().await;

    let err = b
        .svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect_err("expired");
    assert_eq!(err, DomainError::InvalidInput("The request expired.".into()));
    assert!(!b.paired.contains(&a.id()));
    assert_eq!(b.events.finished(), vec!["expired"], "announced once");

    let err = a
        .svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect_err("expired");
    assert_eq!(err, DomainError::InvalidInput("The request expired.".into()));
    assert!(!a.paired.contains(&b.id()));
    assert_eq!(a.events.finished(), vec!["expired"]);
}

#[tokio::test(start_paused = true)]
async fn a_remote_decision_after_expiry_is_ignored() {
    let b = node(2, "ben");
    let _hb = b.svc.start().await;
    let stranger = identity(10);
    let reply = b
        .svc
        .pair_request(stranger.clone(), ip(), pair_request("Mallory"))
        .await
        .expect("request");
    b.svc
        .pair_nonce(
            stranger.clone(),
            reply.pairing_id.clone(),
            NonceMessage {
                nonce: Nonce::random().to_base64(),
            },
        )
        .await
        .expect("nonce");

    tokio::time::advance(PAIRING_TTL + Duration::from_secs(1)).await;
    settle().await;

    b.svc
        .pair_decision(
            stranger.clone(),
            reply.pairing_id.clone(),
            DecisionRequest { accept: true },
        )
        .await
        .expect("a late decision is acknowledged and ignored");
    let status = b
        .svc
        .pair_status(stranger.clone(), reply.pairing_id.clone())
        .await
        .expect("status");
    assert_eq!(status.status, PairingStatus::Expired);
    assert!(!b.paired.contains(&stranger.device_id));
    assert_eq!(b.events.finished(), vec!["expired"]);
}

#[tokio::test(start_paused = true)]
async fn old_finished_sessions_are_purged() {
    let b = node(2, "ben");
    let _hb = b.svc.start().await;
    let stranger = identity(10);
    let reply = b
        .svc
        .pair_request(stranger.clone(), ip(), pair_request("Mallory"))
        .await
        .expect("request");

    tokio::time::advance(PAIRING_TTL * 3).await;
    settle().await;

    let gone = b
        .svc
        .pair_status(stranger, reply.pairing_id)
        .await
        .expect_err("purged");
    assert_eq!(gone, PeerError::NotFound);
}

- [ ] **Step 2: Write the failing presence tests**

Replace `crates/rocket-app/src/network/connections_tests.rs`:

```rust
//! Presence tests: dialing, pings, reconnects, unpairing and duplicate sockets.
//!
//! Single-node tests play the peer through a scripted socket. A manual address makes the node
//! dial at once (`DialHint::ManualAddress`), so those tests do not depend on id order.

use std::sync::atomic::Ordering;
use std::time::Duration;

use rocket_audit::AuditEventKind;
use rocket_network::limits::{DIAL_GRACE, PING_INTERVAL, PROTOCOL_VERSION};
use rocket_network::{
    DeviceId, DiscoveryEvent, NetworkEvent, Nonce, NonceMessage, PeerHandler, PeerMessage,
};
use rocket_shared::error::DomainError;

use super::test_support::*;
use super::{NetworkHandle, NodeGroup, NodeStatus, PairingDecision};

/// A node whose only paired device is `peer_seed`, reachable at its fixed route.
async fn solo(peer_seed: u8) -> (Node, NetworkHandle) {
    let a = node(1, "ann");
    a.paired.seed(identity(peer_seed).device_id, "Peer");
    a.connector
        .add_route(addr(peer_seed), Route::fixed(identity(peer_seed), "Peer"));
    let handle = a.svc.start().await;
    (a, handle)
}

/// Queues a scripted socket and adds the manual address, so the node dials and gets it.
async fn go_online(a: &Node, peer_seed: u8, dialed_by: DeviceId) -> Remote {
    let (socket, remote) = scripted_socket(identity(peer_seed), dialed_by, PROTOCOL_VERSION);
    a.connector.push_socket(socket);
    a.svc
        .add_address("127.0.0.1", addr(peer_seed).port())
        .await
        .expect("add address");
    settle().await;
    remote
}

fn status_of(n: &Node, seed: u8) -> Option<NodeStatus> {
    row_status(n, &identity(seed).device_id)
}

fn audited(n: &Node) -> Vec<AuditEventKind> {
    n.audit.records().into_iter().map(|r| r.kind).collect()
}

// ----- one node against a scripted peer -----

#[tokio::test(start_paused = true)]
async fn a_hub_shows_thirty_paired_peers_online_at_once() {
    let a = node(1, "hub");
    for seed in 2..32u8 {
        a.paired.seed(identity(seed).device_id, "Peer");
        a.connector
            .add_route(addr(seed), Route::fixed(identity(seed), "Peer"));
    }
    let _h = a.svc.start().await;

    // Keep every remote end alive, or its socket closes.
    let mut remotes = Vec::new();
    for seed in 2..32u8 {
        remotes.push(go_online(&a, seed, a.id()).await);
    }

    let online = (2..32u8)
        .filter(|seed| status_of(&a, *seed) == Some(NodeStatus::Online))
        .count();
    assert_eq!(online, 30, "spec 3a: a hub with 30 teammates shows all of them online");
    assert_eq!(remotes.len(), 30);
}

#[tokio::test(start_paused = true)]
async fn a_scripted_peer_goes_online_and_receives_pings() {
    let (a, _h) = solo(2).await;
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Offline));

    let mut remote = go_online(&a, 2, a.id()).await;
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Online));

    tokio::time::advance(PING_INTERVAL + Duration::from_secs(1)).await;
    settle().await;
    assert_eq!(remote.from_service.try_recv(), Ok(PeerMessage::Ping));
}

#[tokio::test(start_paused = true)]
async fn answering_pings_keeps_the_peer_online() {
    let (a, _h) = solo(2).await;
    let mut remote = go_online(&a, 2, a.id()).await;

    for _ in 0..6 {
        tokio::time::advance(PING_INTERVAL).await;
        settle().await;
        assert_eq!(remote.from_service.try_recv(), Ok(PeerMessage::Ping));
        remote
            .to_service
            .send(PeerMessage::Pong)
            .await
            .expect("pong");
        settle().await;
    }
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Online));
}

#[tokio::test(start_paused = true)]
async fn the_third_unanswered_ping_goes_offline() {
    let (a, _h) = solo(2).await;
    let _remote = go_online(&a, 2, a.id()).await;

    for _ in 0..2 {
        tokio::time::advance(PING_INTERVAL).await;
        settle().await;
        assert_eq!(status_of(&a, 2), Some(NodeStatus::Online));
    }
    tokio::time::advance(PING_INTERVAL).await;
    settle().await;
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Offline));
}

#[tokio::test(start_paused = true)]
async fn a_ping_from_the_peer_is_answered_with_a_pong() {
    let (a, _h) = solo(2).await;
    let mut remote = go_online(&a, 2, a.id()).await;

    remote
        .to_service
        .send(PeerMessage::Ping)
        .await
        .expect("ping");
    settle().await;
    assert_eq!(remote.from_service.try_recv(), Ok(PeerMessage::Pong));
}

#[tokio::test(start_paused = true)]
async fn an_unpair_message_removes_the_pairing_and_tells_the_user() {
    let (a, _h) = solo(2).await;
    let remote = go_online(&a, 2, a.id()).await;
    let mark = a.events.all().len();

    remote
        .to_service
        .send(PeerMessage::Unpair)
        .await
        .expect("unpair");
    settle().await;

    assert!(!a.paired.contains(&identity(2).device_id));
    assert_eq!(a.events.removed(), vec![("Peer".to_string(), true)]);
    assert!(a.events.all()[mark..].contains(&NetworkEvent::NodesChanged));
    assert!(audited(&a).contains(&AuditEventKind::NetworkDeviceUnpaired {
        device_id: identity(2).device_id.to_base32(),
        by_peer: true
    }));
    // The device is still on the network, so it is offered under Nearby.
    let rows = a.svc.list_nodes();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].group, NodeGroup::Nearby);
}

#[tokio::test(start_paused = true)]
async fn it_reconnects_after_a_drop_with_growing_delays() {
    let (a, _h) = solo(2).await;
    let remote = go_online(&a, 2, a.id()).await;
    let base = a.connector.open_calls.load(Ordering::SeqCst);
    assert_eq!(base, 1);

    drop(remote);
    settle().await;
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Offline));
    assert_eq!(a.connector.open_calls.load(Ordering::SeqCst), base, "waits first");

    // Retries come after 1 s, 2 s and 4 s.
    tokio::time::advance(Duration::from_secs(1)).await;
    settle().await;
    assert_eq!(a.connector.open_calls.load(Ordering::SeqCst), base + 1);
    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;
    assert_eq!(a.connector.open_calls.load(Ordering::SeqCst), base + 2);
    tokio::time::advance(Duration::from_secs(3)).await;
    settle().await;
    assert_eq!(a.connector.open_calls.load(Ordering::SeqCst), base + 2, "not yet");
    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;
    assert_eq!(a.connector.open_calls.load(Ordering::SeqCst), base + 3);
}

#[tokio::test(start_paused = true)]
async fn it_goes_online_again_when_a_socket_is_available() {
    let (a, _h) = solo(2).await;
    let remote = go_online(&a, 2, a.id()).await;
    drop(remote);
    settle().await;
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Offline));

    let (socket, _second) = scripted_socket(identity(2), a.id(), PROTOCOL_VERSION);
    a.connector.push_socket(socket);
    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Online));
}

#[tokio::test(start_paused = true)]
async fn a_new_sighting_retries_at_once() {
    let (a, _h) = solo(2).await;
    let remote = go_online(&a, 2, a.id()).await;
    drop(remote);
    settle().await;
    // Back off to a long delay first.
    for seconds in [1u64, 2, 4] {
        tokio::time::advance(Duration::from_secs(seconds)).await;
        settle().await;
    }
    let (socket, _second) = scripted_socket(identity(2), a.id(), PROTOCOL_VERSION);
    a.connector.push_socket(socket);

    a.discovery.play(DiscoveryEvent::Seen {
        claimed_id: identity(2).device_id,
        profile: profile("Peer"),
        addresses: vec![addr(2)],
        version: PROTOCOL_VERSION,
    });
    settle().await;
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Online), "no waiting for the backoff");
}

#[tokio::test(start_paused = true)]
async fn a_socket_with_another_protocol_version_needs_an_update() {
    let (a, _h) = solo(2).await;
    let (socket, remote) = scripted_socket(identity(2), a.id(), PROTOCOL_VERSION + 1);
    a.connector.push_socket(socket);
    a.svc
        .add_address("127.0.0.1", addr(2).port())
        .await
        .expect("add address");
    settle().await;

    assert_eq!(status_of(&a, 2), Some(NodeStatus::NeedsUpdate));
    assert!(remote.to_service.is_closed(), "the socket was dropped");
}

// ----- duplicate sockets -----

#[tokio::test(start_paused = true)]
async fn the_socket_dialled_by_the_lower_id_wins() {
    let (a, _h) = solo(2).await;
    let keep = a.id().min(identity(2).device_id);
    let other = a.id().max(identity(2).device_id);
    let first = go_online(&a, 2, keep).await;

    let (second_socket, second) = scripted_socket(identity(2), other, PROTOCOL_VERSION);
    a.svc.on_peer_socket(second_socket).await;
    settle().await;

    assert!(second.to_service.is_closed(), "the other socket is closed");
    assert!(!first.to_service.is_closed(), "the kept socket stays");
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Online));
}

#[tokio::test(start_paused = true)]
async fn a_socket_dialled_by_the_lower_id_replaces_the_other() {
    let (a, _h) = solo(2).await;
    let keep = a.id().min(identity(2).device_id);
    let other = a.id().max(identity(2).device_id);
    let first = go_online(&a, 2, other).await;

    let (second_socket, second) = scripted_socket(identity(2), keep, PROTOCOL_VERSION);
    a.svc.on_peer_socket(second_socket).await;
    settle().await;

    assert!(first.to_service.is_closed(), "the old socket is closed");
    assert!(!second.to_service.is_closed());
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Online));
}

#[tokio::test(start_paused = true)]
async fn a_socket_from_an_unpaired_or_blocked_device_is_refused() {
    let (a, _h) = solo(2).await;

    let (stranger, stranger_remote) = scripted_socket(identity(40), identity(40).device_id, PROTOCOL_VERSION);
    a.svc.on_peer_socket(stranger).await;
    settle().await;
    assert!(stranger_remote.to_service.is_closed());

    a.blocked.put(identity(2).device_id, "Peer");
    let (blocked, blocked_remote) = scripted_socket(identity(2), identity(2).device_id, PROTOCOL_VERSION);
    a.svc.on_peer_socket(blocked).await;
    settle().await;
    assert!(blocked_remote.to_service.is_closed());
    assert!(!a.svc.is_paired(&identity(2).device_id).await, "blocked counts as not paired");
}

// ----- the dialer rule -----

#[tokio::test(start_paused = true)]
async fn paired_nodes_open_exactly_one_connection() {
    let a = node(1, "ann");
    let b = node(2, "ben");
    link(&a, &b);
    a.paired.seed(b.id(), "ben");
    b.paired.seed(a.id(), "ann");
    let _ha = a.svc.start().await;
    let _hb = b.svc.start().await;
    see(&a, &b);
    see(&b, &a);
    settle().await;
    tokio::time::advance(DIAL_GRACE + Duration::from_secs(1)).await;
    settle().await;

    assert_eq!(row_status(&a, &b.id()), Some(NodeStatus::Online));
    assert_eq!(row_status(&b, &a.id()), Some(NodeStatus::Online));
    let opens = a.connector.open_calls.load(Ordering::SeqCst)
        + b.connector.open_calls.load(Ordering::SeqCst);
    assert_eq!(opens, 1, "only the lower id dialled");
}

#[tokio::test(start_paused = true)]
async fn the_higher_id_waits_for_the_grace_period() {
    let a = node(1, "ann");
    let b = node(2, "ben");
    link(&a, &b);
    a.paired.seed(b.id(), "ben");
    b.paired.seed(a.id(), "ann");
    let (low, high) = ordered(&a, &b);
    let _hl = low.svc.start().await;
    let _hh = high.svc.start().await;

    // Only the higher side knows where the lower one is.
    see(high, low);
    settle().await;
    tokio::time::advance(DIAL_GRACE - Duration::from_secs(3)).await;
    settle().await;
    assert_eq!(high.connector.open_calls.load(Ordering::SeqCst), 0, "still waiting");
    assert_eq!(row_status(high, &low.id()), Some(NodeStatus::Offline));

    tokio::time::advance(Duration::from_secs(4)).await;
    settle().await;
    assert_eq!(high.connector.open_calls.load(Ordering::SeqCst), 1, "the grace period ended");
    assert_eq!(row_status(high, &low.id()), Some(NodeStatus::Online));
    assert_eq!(row_status(low, &high.id()), Some(NodeStatus::Online));
}

#[tokio::test(start_paused = true)]
async fn a_hidden_node_still_serves_paired_presence() {
    let a = node(1, "ann");
    let b = node(2, "ben");
    link(&a, &b);
    a.paired.seed(b.id(), "ben");
    b.paired.seed(a.id(), "ann");
    let _ha = a.svc.start().await;
    let _hb = b.svc.start().await;
    b.svc.set_hidden(true).await.expect("hide");
    see(&a, &b);
    see(&b, &a);
    settle().await;
    tokio::time::advance(DIAL_GRACE + Duration::from_secs(1)).await;
    settle().await;

    assert_eq!(row_status(&a, &b.id()), Some(NodeStatus::Online));
    assert_eq!(row_status(&b, &a.id()), Some(NodeStatus::Online));

    // Browsing still works while hidden (spec 7.4).
    b.connector
        .add_route(addr(3), Route::fixed(identity(3), "Cat Lee"));
    b.discovery.play(DiscoveryEvent::Seen {
        claimed_id: identity(3).device_id,
        profile: profile("Cat Lee"),
        addresses: vec![addr(3)],
        version: PROTOCOL_VERSION,
    });
    settle().await;
    assert_eq!(
        row_status(&b, &identity(3).device_id),
        Some(NodeStatus::Available),
        "a hidden node still lists nearby nodes"
    );
}

// ----- spoofing and impostors -----

#[tokio::test(start_paused = true)]
async fn a_spoofed_id_of_a_paired_device_is_never_dialled() {
    let a = node(1, "ann");
    a.paired.seed(identity(2).device_id, "Peer");
    // A stranger answers at addr(8) with its own key.
    a.connector
        .add_route(addr(8), Route::fixed(identity(8), "Mallory"));
    let _ha = a.svc.start().await;

    a.discovery.play(DiscoveryEvent::Seen {
        claimed_id: identity(2).device_id,
        profile: profile("Peer"),
        addresses: vec![addr(8)],
        version: PROTOCOL_VERSION,
    });
    settle().await;
    tokio::time::advance(DIAL_GRACE * 3).await;
    settle().await;

    assert_eq!(a.connector.open_calls.load(Ordering::SeqCst), 0);
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Offline));
}

#[tokio::test(start_paused = true)]
async fn an_impostor_at_a_saved_address_does_not_remove_the_pairing() {
    let a = node(1, "ann");
    a.paired
        .seed_with(identity(2).device_id, "Peer", vec![addr(2)]);
    // The address now answers with another key. The connector reports an identity mismatch.
    a.connector
        .add_route(addr(2), Route::fixed(identity(77), "Impostor"));
    let _ha = a.svc.start().await;
    tokio::time::advance(DIAL_GRACE + Duration::from_secs(1)).await;
    settle().await;

    assert!(a.connector.open_calls.load(Ordering::SeqCst) >= 1, "it did try");
    assert!(a.paired.contains(&identity(2).device_id));
    assert!(a.events.removed().is_empty());
}

// ----- unpairing between two real nodes -----

#[tokio::test(start_paused = true)]
async fn a_peer_that_unpaired_us_while_offline_is_removed_on_the_next_dial() {
    let a = node(1, "ann");
    let b = node(2, "ben");
    link(&a, &b);
    a.paired.seed(b.id(), "ben");
    // B no longer has A.
    let _ha = a.svc.start().await;
    let _hb = b.svc.start().await;
    let mark = a.events.all().len();

    // A manual address makes A dial straight away. B answers "not paired".
    a.svc
        .add_address("127.0.0.1", b.addr().port())
        .await
        .expect("add address");
    settle().await;

    assert!(!a.paired.contains(&b.id()));
    assert_eq!(a.events.removed(), vec![("ben".to_string(), true)]);
    assert!(a.events.all()[mark..].contains(&NetworkEvent::NodesChanged));
    assert!(audited(&a).contains(&AuditEventKind::NetworkDeviceUnpaired {
        device_id: b.id().to_base32(),
        by_peer: true
    }));
}

#[tokio::test(start_paused = true)]
async fn removing_a_paired_device_online_tells_the_peer() {
    let a = node(1, "ann");
    let b = node(2, "ben");
    link(&a, &b);
    a.paired.seed(b.id(), "ben");
    b.paired.seed(a.id(), "ann");
    let _ha = a.svc.start().await;
    let _hb = b.svc.start().await;
    see(&a, &b);
    see(&b, &a);
    settle().await;
    tokio::time::advance(DIAL_GRACE + Duration::from_secs(1)).await;
    settle().await;
    assert_eq!(row_status(&a, &b.id()), Some(NodeStatus::Online));

    let mark_a = a.events.all().len();
    a.svc.remove_paired(&b.id()).await.expect("remove");
    settle().await;

    assert!(!a.paired.contains(&b.id()));
    assert!(!b.paired.contains(&a.id()), "the peer removed us too");
    assert_eq!(a.events.removed(), vec![("ben".to_string(), false)]);
    assert_eq!(b.events.removed(), vec![("ann".to_string(), true)]);
    assert!(a.events.all()[mark_a..].contains(&NetworkEvent::NodesChanged));
    assert!(audited(&a).contains(&AuditEventKind::NetworkDeviceUnpaired {
        device_id: b.id().to_base32(),
        by_peer: false
    }));
    assert!(audited(&b).contains(&AuditEventKind::NetworkDeviceUnpaired {
        device_id: a.id().to_base32(),
        by_peer: true
    }));
    assert!(a
        .svc
        .list_nodes()
        .iter()
        .all(|row| row.group != NodeGroup::Paired));
}

#[tokio::test(start_paused = true)]
async fn blocking_a_paired_device_unpairs_it_and_closes_its_socket() {
    let a = node(1, "ann");
    let b = node(2, "ben");
    link(&a, &b);
    a.paired.seed(b.id(), "ben");
    b.paired.seed(a.id(), "ann");
    let _ha = a.svc.start().await;
    let _hb = b.svc.start().await;
    see(&a, &b);
    see(&b, &a);
    settle().await;
    tokio::time::advance(DIAL_GRACE + Duration::from_secs(1)).await;
    settle().await;
    assert_eq!(row_status(&a, &b.id()), Some(NodeStatus::Online));

    // The paired device asks to pair again, and the user answers Block.
    let reply = a
        .svc
        .pair_request(b.identity.clone(), ip(), pair_request("ben"))
        .await
        .expect("a paired device may ask again");
    a.svc
        .pair_nonce(
            b.identity.clone(),
            reply.pairing_id.clone(),
            NonceMessage {
                nonce: Nonce::random().to_base64(),
            },
        )
        .await
        .expect("nonce");
    let mark_a = a.events.all().len();
    a.svc
        .respond_pairing(&reply.pairing_id, PairingDecision::Block)
        .await
        .expect("block");
    settle().await;

    assert!(a.blocked.has(&b.id()));
    assert!(!a.paired.contains(&b.id()), "blocking also removes the pairing");
    assert!(!b.paired.contains(&a.id()), "the peer got Unpair and removed us");
    assert_eq!(a.events.removed(), vec![("ben".to_string(), false)]);
    assert_eq!(b.events.removed(), vec![("ann".to_string(), true)]);
    assert!(a.events.all()[mark_a..].contains(&NetworkEvent::NodesChanged));
    assert!(audited(&a).contains(&AuditEventKind::NetworkDeviceBlocked {
        device_id: b.id().to_base32()
    }));
    assert!(audited(&a).contains(&AuditEventKind::NetworkDeviceUnpaired {
        device_id: b.id().to_base32(),
        by_peer: false
    }));
    assert_ne!(row_status(&a, &b.id()), Some(NodeStatus::Online), "the socket is closed");

    // The blocked device cannot open a new socket.
    tokio::time::advance(DIAL_GRACE + Duration::from_secs(1)).await;
    settle().await;
    assert!(!a.svc.is_paired(&b.id()).await);
    assert_ne!(row_status(&b, &a.id()), Some(NodeStatus::Online));
}

#[tokio::test(start_paused = true)]
async fn frames_from_a_blocked_device_get_no_answer_and_close_the_socket() {
    let (a, _h) = solo(2).await;
    let mut remote = go_online(&a, 2, a.id()).await;
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Online));

    // The block lands while the socket is open.
    a.blocked.put(identity(2).device_id, "Peer");
    remote
        .to_service
        .send(PeerMessage::Ping)
        .await
        .expect("send ping");
    settle().await;

    let next = tokio::time::timeout(Duration::from_secs(1), remote.from_service.recv())
        .await
        .expect("the socket ends at once");
    assert_eq!(next, None, "no Pong, and the service closed the socket");
}

#[tokio::test(start_paused = true)]
async fn removing_a_device_that_is_not_paired_is_an_error() {
    let a = node(1, "ann");
    let _ha = a.svc.start().await;
    let err = a
        .svc
        .remove_paired(&identity(2).device_id)
        .await
        .expect_err("not paired");
    assert_eq!(
        err,
        DomainError::InvalidInput("That device is not paired.".into())
    );
}

#[tokio::test(start_paused = true)]
async fn the_last_good_address_is_remembered() {
    let (a, _h) = solo(2).await;
    let _remote = go_online(&a, 2, a.id()).await;
    assert_eq!(a.paired.last_addresses(&identity(2).device_id), vec![addr(2)]);
}
```

- [ ] **Step 3: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket-app network`
Expected: FAIL to compile, `error[E0599]: no method named `request_pairing` found for struct `Arc<NetworkService>`` (also `respond_pairing`, `cancel_pairing`, `remove_paired`). The Task 1 tests still compile.

- [ ] **Step 4: Edit `service.rs` for the lock, the sweeper and the connections**

In `crates/rocket-app/src/network/service.rs`:

(a) In `pub struct NetworkService`, replace

```rust
    state: Mutex<State>,
}
```

with

```rust
    state: Mutex<State>,
    /// Serialises writes to the paired list that must not undo a removal.
    paired_lock: Mutex<()>,
}
```

(b) In `NetworkService::new`, replace

```rust
            state: Mutex::new(state),
        })
```

with

```rust
            state: Mutex::new(state),
            paired_lock: Mutex::new(()),
        })
```

(c) After the `lock()` method, before the doc comment `/// The service as an `Arc`, for tasks that outlive one call.`, add:

```rust
    /// Held while the paired list is changed together with a decision about it.
    pub(super) fn paired_guard(&self) -> MutexGuard<'_, ()> {
        self.paired_lock.lock().unwrap_or_else(|e| e.into_inner())
    }

```

(d) In `start`, replace the line `        // Background work and the final events.` with:

```rust
        // Background work and the final events.
        let sweeper = Arc::clone(self);
        let sweep_stop = stop_rx.clone();
        self.spawn(async move { sweeper.sweep_loop(sweep_stop).await });
        for device in self.deps.paired.list().unwrap_or_default() {
            self.ensure_connection(&device.device_id, false);
        }
```

- [ ] **Step 5: Implement pairing**

Replace `crates/rocket-app/src/network/pairing.rs`:

```rust
//! Pairing: sessions, the local user's requests and decisions, cancel and the sweeper.
//!
//! The initiator is the node whose user picked the other one. The responder is the node that
//! received the request. Both keep a `PairingSession` in memory only. `PairingFinished` is
//! published once per session, from `take_finished`, whatever ended it.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use rocket_audit::{AuditActor, AuditEventKind};
use rocket_network::limits::{
    DECLINE_COOLDOWN, MAX_PAIRED_DEVICES, PAIRING_TTL, PROTOCOL_VERSION,
};
use rocket_network::{
    commitment_from_base64, BlockedDevice, DecisionRequest, DeviceId, HelloResponse, NetworkEvent,
    Nonce, NonceMessage, PairRequest, PairedDevice, PairingCode, PairingId, PairingRole,
    PairingSession, PairingState, PairingStatus, PeerCallError, PeerConnector, PeerIdentity,
};
use rocket_shared::error::{DomainError, DomainResult};
use tokio::sync::watch;

use crate::stop_signal::{run_every, sleep_or_stop};

use super::service::{paired_limit_message, NetworkService};
use super::status::NetworkState;

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const SWEEP_INTERVAL: Duration = Duration::from_secs(5);
const CALL_TIMEOUT: Duration = Duration::from_secs(10);
/// Polls that may fail one after the other before the initiator gives up.
const MAX_POLL_FAILURES: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingDecision {
    Accept,
    Decline,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingStarted {
    pub pairing_id: PairingId,
    pub code: PairingCode,
}

/// One open or finished pairing, seen from this node.
pub(super) struct SessionEntry {
    pub session: PairingSession,
    /// The address the initiator reached the peer on. `None` for a responder.
    pub addr: Option<SocketAddr>,
    pub created: Instant,
    /// `PairingFinished` was published.
    pub notified: bool,
    /// The paired device was saved.
    pub saved: bool,
    /// Saving has started, so it runs once.
    pub claimed: bool,
}

impl SessionEntry {
    pub(super) fn new(session: PairingSession, addr: Option<SocketAddr>, created: Instant) -> Self {
        SessionEntry {
            session,
            addr,
            created,
            notified: false,
            saved: false,
            claimed: false,
        }
    }
}

fn user_error(text: &str) -> DomainError {
    DomainError::InvalidInput(text.to_string())
}

fn pairing_failed() -> DomainError {
    user_error("Pairing failed. Try again.")
}

fn busy() -> DomainError {
    user_error("The other device is busy. Try again in a minute.")
}

fn not_open() -> DomainError {
    user_error("That pairing request is no longer open.")
}

fn call_error(error: PeerCallError) -> DomainError {
    match error {
        PeerCallError::Busy => busy(),
        _ => pairing_failed(),
    }
}

/// The `PairingFinished` event for a session that just ended, once. A session that reached
/// `Paired` is announced only after the device was saved.
fn take_finished(entry: &mut SessionEntry) -> Option<NetworkEvent> {
    if entry.notified || !entry.session.is_terminal() {
        return None;
    }
    if entry.session.state() == PairingState::Paired && !entry.saved {
        return None;
    }
    let outcome = entry.session.outcome()?;
    entry.notified = true;
    Some(NetworkEvent::PairingFinished {
        pairing_id: entry.session.id().0.clone(),
        outcome: outcome.as_str().to_string(),
        display_name: entry.session.peer_profile().display_name.clone(),
    })
}

impl NetworkService {
    /// Runs `act` on a session after expiring it if due, then publishes what ended. Returns
    /// `None` when the session does not exist.
    pub(super) fn with_session<R>(
        &self,
        id: &PairingId,
        act: impl FnOnce(&mut SessionEntry) -> R,
    ) -> Option<R> {
        let (result, finished) = {
            let mut st = self.lock();
            let entry = st.sessions.get_mut(id)?;
            entry.session.expire_if_due(Self::now());
            let result = act(entry);
            (result, take_finished(entry))
        };
        if let Some(event) = finished {
            self.emit(event);
            self.emit(NetworkEvent::NodesChanged);
        }
        Some(result)
    }

    // ----- starting a pairing -----

    pub async fn request_pairing(
        self: &Arc<Self>,
        device_id: &DeviceId,
    ) -> DomainResult<PairingStarted> {
        let (local, connector) = {
            let st = self.lock();
            match st.runtime.as_ref() {
                Some(rt) if matches!(st.net, NetworkState::Running) => {
                    (rt.local.clone(), Arc::clone(&rt.connector))
                }
                _ => return Err(user_error("Networking is off.")),
            }
        };
        if *device_id == local.device_id {
            return Err(user_error("You cannot pair with this device."));
        }
        if self.deps.paired.get(device_id)?.is_some() {
            return Err(user_error("This device is already paired."));
        }
        if self.deps.paired.list()?.len() >= MAX_PAIRED_DEVICES {
            return Err(user_error(&paired_limit_message()));
        }
        if self.deps.blocked.contains(device_id)? {
            return Err(user_error(
                "This device is blocked. Unblock it in Settings to pair.",
            ));
        }
        let addresses = {
            let st = self.lock();
            if st.sessions.values().any(|e| {
                !e.session.is_terminal() && e.session.peer().device_id == *device_id
            }) {
                return Err(user_error("A pairing with this device is already open."));
            }
            match st.seen.get(device_id) {
                Some(seen) => seen
                    .mdns
                    .iter()
                    .chain(seen.manual.iter())
                    .copied()
                    .collect::<Vec<_>>(),
                None => return Err(user_error("That device is no longer nearby.")),
            }
        };

        // Deviation 8: a hello first, so the key is checked before any pairing message.
        let Some((address, hello)) = self
            .verify_hello(&connector, &addresses, Some(device_id))
            .await
        else {
            return Err(pairing_failed());
        };
        self.initiate(&local, &connector, address, hello, device_id)
            .await
    }

    /// The pairing messages, after `/hello` proved the key.
    async fn initiate(
        self: &Arc<Self>,
        local: &PeerIdentity,
        connector: &Arc<dyn PeerConnector>,
        address: SocketAddr,
        hello: HelloResponse,
        device_id: &DeviceId,
    ) -> DomainResult<PairingStarted> {
        if hello.version != PROTOCOL_VERSION {
            return Err(user_error("This device needs an update before it can pair."));
        }
        let request = PairRequest {
            version: PROTOCOL_VERSION,
            profile: self.lock().profile.clone(),
        };
        let (peer_identity, reply) =
            match tokio::time::timeout(CALL_TIMEOUT, connector.pair_request(address, device_id, &request))
                .await
            {
                Ok(Ok(reply)) => reply,
                Ok(Err(e)) => return Err(call_error(e)),
                Err(_) => return Err(pairing_failed()),
            };
        if peer_identity.device_id != *device_id {
            return Err(pairing_failed());
        }
        let pairing_id = reply.pairing_id.clone();
        let Ok(commitment) = commitment_from_base64(&reply.commitment) else {
            self.abort_remote(address, *device_id, &pairing_id).await;
            return Err(pairing_failed());
        };

        let na = Nonce::random();
        let mut session = PairingSession::new_initiator(
            pairing_id.clone(),
            peer_identity,
            hello.profile.clone(),
            local.spki_der.clone(),
            commitment,
            na.clone(),
            Self::now(),
        );
        let message = NonceMessage {
            nonce: na.to_base64(),
        };
        let revealed = match tokio::time::timeout(
            CALL_TIMEOUT,
            connector.pair_nonce(address, device_id, &pairing_id, &message),
        )
        .await
        {
            Ok(Ok(revealed)) => revealed,
            Ok(Err(e)) => {
                self.abort_remote(address, *device_id, &pairing_id).await;
                return Err(call_error(e));
            }
            Err(_) => {
                self.abort_remote(address, *device_id, &pairing_id).await;
                return Err(pairing_failed());
            }
        };
        let code = Nonce::parse_base64(&revealed.nonce)
            .and_then(|nb| session.receive_reveal(nb));
        let code = match code {
            Ok(code) => code,
            Err(_) => {
                // A wrong nonce: the commitment check failed. Save nothing and drop the peer's side.
                tracing::info!(code = "network_pairing_failed", id = %device_id.short(), "commitment mismatch");
                self.abort_remote(address, *device_id, &pairing_id).await;
                self.emit(NetworkEvent::PairingFinished {
                    pairing_id: pairing_id.0.clone(),
                    outcome: "failed".to_string(),
                    display_name: hello.profile.display_name.clone(),
                });
                return Err(pairing_failed());
            }
        };

        self.lock().sessions.insert(
            pairing_id.clone(),
            SessionEntry::new(session, Some(address), Self::now()),
        );
        self.emit(NetworkEvent::NodesChanged);
        Ok(PairingStarted { pairing_id, code })
    }

    /// Tells the peer to drop its session. Best effort: errors are ignored.
    async fn abort_remote(&self, address: SocketAddr, expected: DeviceId, id: &PairingId) {
        if let Some(connector) = self.connector() {
            let _ = tokio::time::timeout(
                CALL_TIMEOUT,
                connector.pair_decision(address, &expected, id, &DecisionRequest { accept: false }),
            )
            .await;
        }
    }

    // ----- answering a pairing -----

    pub async fn respond_pairing(
        self: &Arc<Self>,
        id: &PairingId,
        decision: PairingDecision,
    ) -> DomainResult<()> {
        let accept = decision == PairingDecision::Accept;
        let (applied, role, address, peer, name, state) = self
            .with_session(id, |entry| {
                let applied = entry.session.decide_local(accept);
                (
                    applied,
                    entry.session.role(),
                    entry.addr,
                    entry.session.peer().clone(),
                    entry.session.peer_profile().display_name.clone(),
                    entry.session.state(),
                )
            })
            .ok_or_else(not_open)?;
        if applied.is_err() {
            return Err(if state == PairingState::Expired {
                user_error("The request expired.")
            } else {
                not_open()
            });
        }
        if decision == PairingDecision::Block {
            // A paired device that is blocked is also unpaired and disconnected.
            self.block_peer(&peer, &name).await;
        }
        if !accept && role == PairingRole::Responder {
            self.start_cooldown(&peer.device_id);
        }
        match role {
            PairingRole::Responder => {
                if accept {
                    self.complete_pairing(id);
                }
                Ok(())
            }
            PairingRole::Initiator => {
                self.finish_as_initiator(id, address, peer.device_id, accept)
                    .await
            }
        }
    }

    /// Sends the initiator's decision. On accept, a background task polls for the answer.
    async fn finish_as_initiator(
        self: &Arc<Self>,
        id: &PairingId,
        address: Option<SocketAddr>,
        expected: DeviceId,
        accept: bool,
    ) -> DomainResult<()> {
        let (Some(address), Some(connector)) = (address, self.connector()) else {
            return Err(pairing_failed());
        };
        let sent = tokio::time::timeout(
            CALL_TIMEOUT,
            connector.pair_decision(address, &expected, id, &DecisionRequest { accept }),
        )
        .await;
        if !accept {
            // A decline is best effort. The session already ended locally.
            return Ok(());
        }
        if !matches!(sent, Ok(Ok(()))) {
            // The peer could not take the decision. This is a failure, not a user cancel.
            let _ = self.with_session(id, |entry| entry.session.fail());
            return Err(pairing_failed());
        }
        let Some(stop) = self.lock().runtime.as_ref().map(|r| r.stop.clone()) else {
            return Err(user_error("Networking is off."));
        };
        let me = Arc::clone(self);
        let id = id.clone();
        self.spawn(async move { me.poll_initiator(id, address, expected, stop).await });
        Ok(())
    }

    /// Polls `pair_status` once a second until the answer is final or the session expires.
    /// It keeps its own loop instead of `run_every`: it carries a failure counter and returns from
    /// several places, which a tick closure would only hide.
    async fn poll_initiator(
        &self,
        id: PairingId,
        address: SocketAddr,
        expected: DeviceId,
        mut stop: watch::Receiver<bool>,
    ) {
        let Some(connector) = self.connector() else {
            return;
        };
        let mut failures = 0u32;
        loop {
            if !sleep_or_stop(&mut stop, POLL_INTERVAL).await {
                return;
            }
            match self.with_session(&id, |entry| entry.session.is_terminal()) {
                None => return,
                Some(true) => {
                    self.complete_pairing(&id);
                    return;
                }
                Some(false) => {}
            }
            let reply =
                tokio::time::timeout(CALL_TIMEOUT, connector.pair_status(address, &expected, &id))
                    .await;
            match reply {
                Ok(Ok(answer)) => {
                    failures = 0;
                    match answer.status {
                        // The local timer expires the session; the peer's "expired" needs no action.
                        PairingStatus::Pending | PairingStatus::Expired => {}
                        PairingStatus::Accepted => {
                            let _ = self.with_session(&id, |entry| {
                                let _ = entry.session.decide_remote(true);
                            });
                            self.complete_pairing(&id);
                            return;
                        }
                        PairingStatus::Declined => {
                            let _ = self.with_session(&id, |entry| {
                                let _ = entry.session.decide_remote(false);
                            });
                            return;
                        }
                    }
                }
                _ => {
                    failures += 1;
                    if failures >= MAX_POLL_FAILURES {
                        // The peer stopped answering partway. The user sees "Pairing failed".
                        let _ = self.with_session(&id, |entry| entry.session.fail());
                        return;
                    }
                }
            }
        }
    }

    pub fn cancel_pairing(&self, id: &PairingId) -> DomainResult<()> {
        let (was_live, address, peer) = self
            .with_session(id, |entry| {
                let was_live = !entry.session.is_terminal();
                entry.session.cancel();
                (was_live, entry.addr, entry.session.peer().device_id)
            })
            .ok_or_else(not_open)?;
        if let (true, Some(address), Some(me)) = (was_live, address, self.arc()) {
            let id = id.clone();
            self.spawn(async move { me.abort_remote(address, peer, &id).await });
        }
        Ok(())
    }

    // ----- finishing -----

    /// Saves the paired device once a session is `Paired`, in either role. Runs once.
    pub(super) fn complete_pairing(&self, id: &PairingId) {
        let job = {
            let mut st = self.lock();
            let Some(entry) = st.sessions.get_mut(id) else {
                return;
            };
            if entry.claimed || entry.session.state() != PairingState::Paired {
                return;
            }
            entry.claimed = true;
            (
                entry.session.peer().device_id,
                entry.session.peer_profile().display_name.clone(),
                entry.addr,
            )
        };
        let (device_id, name, address) = job;
        let result = {
            let _guard = self.paired_guard();
            self.deps.paired.upsert(PairedDevice {
                device_id,
                name_at_pairing: name,
                paired_at: Utc::now(),
                last_addresses: address.into_iter().collect(),
            })
        };
        let finished = {
            let mut st = self.lock();
            st.sessions.get_mut(id).and_then(|entry| match &result {
                Ok(()) => {
                    entry.saved = true;
                    take_finished(entry)
                }
                Err(_) => {
                    entry.notified = true;
                    Some(NetworkEvent::PairingFinished {
                        pairing_id: id.0.clone(),
                        outcome: "failed".to_string(),
                        display_name: entry.session.peer_profile().display_name.clone(),
                    })
                }
            })
        };
        match result {
            Ok(()) => {
                tracing::info!(code = "network_device_paired", id = %device_id.short(), "paired");
                self.audit(
                    AuditActor::User,
                    AuditEventKind::NetworkDevicePaired {
                        device_id: device_id.to_base32(),
                    },
                );
                self.ensure_connection(&device_id, true);
            }
            Err(e) => {
                tracing::warn!(code = "network_pairing_save_failed", error = %e, "pairing not saved");
            }
        }
        if let Some(event) = finished {
            self.emit(event);
        }
        self.emit(NetworkEvent::NodesChanged);
    }

    pub(super) fn start_cooldown(&self, device_id: &DeviceId) {
        let until = Self::now() + DECLINE_COOLDOWN;
        self.lock().cooldowns.insert(*device_id, until);
    }

    /// Adds the peer to the block list. The sender sees the same "declined" as for a decline.
    /// A paired device is also unpaired: it gets `Unpair` on a live socket, the socket closes,
    /// and `PairedRemoved { by_peer: false }` and an unpair audit record follow.
    pub(super) async fn block_peer(&self, peer: &PeerIdentity, name: &str) {
        let device = BlockedDevice {
            device_id: peer.device_id,
            name: name.to_string(),
            blocked_at: Utc::now(),
        };
        match self.deps.blocked.add(device) {
            Ok(()) => {
                tracing::info!(code = "network_device_blocked", id = %peer.device_id.short(), "blocked");
                self.audit(
                    AuditActor::User,
                    AuditEventKind::NetworkDeviceBlocked {
                        device_id: peer.device_id.to_base32(),
                    },
                );
                self.start_cooldown(&peer.device_id);
            }
            Err(e) => {
                tracing::warn!(code = "network_block_failed", error = %e, "block not saved");
                return;
            }
        }
        if matches!(self.deps.paired.get(&peer.device_id), Ok(Some(_))) {
            // The pairing goes too, so a blocked device never stays paired.
            if let Err(e) = self.unpair_by_user(&peer.device_id).await {
                tracing::warn!(code = "network_unpair_failed", error = %e, "pairing not removed");
            }
        }
        self.emit(NetworkEvent::NodesChanged);
    }

    // ----- the sweeper -----

    pub(super) async fn sweep_loop(&self, mut stop: watch::Receiver<bool>) {
        run_every(&mut stop, SWEEP_INTERVAL, || async move {
            self.sweep_sessions();
        })
        .await;
    }

    /// Expires due sessions (and publishes them), then drops old finished ones.
    pub(super) fn sweep_sessions(&self) {
        let now = Self::now();
        let finished: Vec<NetworkEvent> = {
            let mut st = self.lock();
            let mut events = Vec::new();
            for entry in st.sessions.values_mut() {
                entry.session.expire_if_due(now);
                if let Some(event) = take_finished(entry) {
                    events.push(event);
                }
            }
            st.sessions.retain(|_, entry| {
                !(entry.session.is_terminal()
                    && now.saturating_duration_since(entry.created) > PAIRING_TTL * 2)
            });
            st.cooldowns.retain(|_, until| *until > now);
            events
        };
        if !finished.is_empty() {
            for event in finished {
                self.emit(event);
            }
            self.emit(NetworkEvent::NodesChanged);
        }
    }
}
```

- [ ] **Step 6: Implement the peer handler**

Replace `crates/rocket-app/src/network/handler.rs`:

```rust
//! The peer handler: what the peer server calls. Strangers can reach only `/hello` and the
//! pairing calls, and every pairing call is bound to the key that opened the session.

use std::net::IpAddr;

use async_trait::async_trait;
use rocket_network::limits::{MAX_PAIRED_DEVICES, PROTOCOL_VERSION};
use rocket_network::{
    commitment_to_base64, DecisionRequest, DeviceId, HelloResponse, NetworkEvent, Nonce,
    NonceMessage, PairRequest, PairRequestResponse, PairStatusResponse, PairingCode, PairingId,
    PairingRole, PairingSession, PairingState, PairingStatus, PeerError, PeerHandler, PeerIdentity,
    PeerMessage, PeerSocket,
};

use super::pairing::SessionEntry;
use super::service::NetworkService;
use super::status::NetworkState;

/// The session must belong to this key and play the responder.
fn check_session(entry: &SessionEntry, peer: &PeerIdentity) -> Result<(), PeerError> {
    if entry.session.role() != PairingRole::Responder
        || entry.session.peer().device_id != peer.device_id
    {
        return Err(PeerError::Forbidden);
    }
    Ok(())
}

#[async_trait]
impl PeerHandler for NetworkService {
    async fn hello(&self) -> HelloResponse {
        self.hello_response()
    }

    async fn pair_request(
        &self,
        peer: PeerIdentity,
        _remote_ip: IpAddr,
        req: PairRequest,
    ) -> Result<PairRequestResponse, PeerError> {
        if req.version != PROTOCOL_VERSION {
            return Err(PeerError::BadRequest);
        }
        let profile = req.profile.validated().map_err(|_| PeerError::BadRequest)?;
        // Any read error counts as blocked.
        if self.deps.blocked.contains(&peer.device_id).unwrap_or(true) {
            tracing::info!(code = "network_request_blocked", id = %peer.device_id.short(), "refused");
            return Err(PeerError::Forbidden);
        }
        let already_paired = self
            .deps
            .paired
            .get(&peer.device_id)
            .map_err(|_| PeerError::Internal)?
            .is_some();
        let paired_count = self
            .deps
            .paired
            .list()
            .map_err(|_| PeerError::Internal)?
            .len();

        let now = Self::now();
        let id = PairingId::new();
        let commit = {
            let mut st = self.lock();
            let Some(local) = st.runtime.as_ref().map(|r| r.local.clone()) else {
                return Err(PeerError::Forbidden);
            };
            if !matches!(st.net, NetworkState::Running) || st.settings.hidden {
                return Err(PeerError::Forbidden);
            }
            if peer.device_id == local.device_id {
                return Err(PeerError::BadRequest);
            }
            if !already_paired && paired_count >= MAX_PAIRED_DEVICES {
                return Err(PeerError::Forbidden);
            }
            if st
                .cooldowns
                .get(&peer.device_id)
                .is_some_and(|until| *until > now)
            {
                return Err(PeerError::Busy);
            }
            if st.sessions.values().any(|e| {
                !e.session.is_terminal() && e.session.peer().device_id == peer.device_id
            }) {
                return Err(PeerError::Busy);
            }
            // The service-level limit; `SlidingWindow` is tested in Plan 01.
            if !st.pair_window.try_hit(now) {
                return Err(PeerError::Busy);
            }
            let (session, commit) = PairingSession::new_responder(
                id.clone(),
                peer.clone(),
                profile,
                local.spki_der.clone(),
                Nonce::random(),
                now,
            );
            st.sessions
                .insert(id.clone(), SessionEntry::new(session, None, now));
            commit
        };
        Ok(PairRequestResponse {
            pairing_id: id,
            commitment: commitment_to_base64(&commit),
        })
    }

    async fn pair_nonce(
        &self,
        peer: PeerIdentity,
        id: PairingId,
        msg: NonceMessage,
    ) -> Result<NonceMessage, PeerError> {
        let na = Nonce::parse_base64(&msg.nonce).map_err(|_| PeerError::BadRequest)?;
        let (nb, code, name) = self
            .with_session(&id, |entry| -> Result<(Nonce, PairingCode, String), PeerError> {
                check_session(entry, &peer)?;
                if entry.session.is_terminal() {
                    return Err(PeerError::NotFound);
                }
                let (nb, code) = entry
                    .session
                    .receive_nonce(na)
                    .map_err(|_| PeerError::BadRequest)?;
                Ok((nb, code, entry.session.peer_profile().display_name.clone()))
            })
            .ok_or(PeerError::NotFound)??;
        // The notice appears only now, after the commitment was revealed.
        self.emit(NetworkEvent::PairingIncoming {
            pairing_id: id.0.clone(),
            device_id: peer.device_id.to_base32(),
            short_id: peer.device_id.short(),
            display_name: name,
            code: code.digits(),
        });
        self.emit(NetworkEvent::NodesChanged);
        Ok(NonceMessage {
            nonce: nb.to_base64(),
        })
    }

    async fn pair_decision(
        &self,
        peer: PeerIdentity,
        id: PairingId,
        req: DecisionRequest,
    ) -> Result<(), PeerError> {
        let state = self
            .with_session(&id, |entry| -> Result<PairingState, PeerError> {
                check_session(entry, &peer)?;
                // A decision for a finished session is acknowledged and ignored.
                if entry.session.is_terminal() {
                    return Ok(entry.session.state());
                }
                entry
                    .session
                    .decide_remote(req.accept)
                    .map_err(|_| PeerError::BadRequest)?;
                Ok(entry.session.state())
            })
            .ok_or(PeerError::NotFound)??;
        if state == PairingState::Paired {
            self.complete_pairing(&id);
        }
        Ok(())
    }

    async fn pair_status(
        &self,
        peer: PeerIdentity,
        id: PairingId,
    ) -> Result<PairStatusResponse, PeerError> {
        let (status, state) = self
            .with_session(&id, |entry| -> Result<(PairingStatus, PairingState), PeerError> {
                check_session(entry, &peer)?;
                Ok((entry.session.status(), entry.session.state()))
            })
            .ok_or(PeerError::NotFound)??;
        if state == PairingState::Paired {
            self.complete_pairing(&id);
        }
        Ok(PairStatusResponse { status })
    }

    async fn is_paired(&self, device: &DeviceId) -> bool {
        self.paired_and_not_blocked(device)
    }

    async fn on_peer_socket(&self, socket: PeerSocket) {
        self.accept_socket(socket).await;
    }

    fn local_hello(&self) -> PeerMessage {
        self.local_hello_message()
    }
}
```

- [ ] **Step 7: Implement the presence connections**

Replace `crates/rocket-app/src/network/connections.rs`:

```rust
//! Presence: one task per paired device keeps a WebSocket open when it can.
//!
//! The task dials when the dialer rule allows it, accepts sockets the peer opened (they arrive
//! through the entry's inbox), pings, answers pings, and ends when the pairing is removed. A
//! `NotPaired` answer to a dial means the peer removed us: we remove it too.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rocket_audit::{AuditActor, AuditEventKind};
use rocket_network::limits::{DIAL_GRACE, PING_INTERVAL, PROTOCOL_VERSION};
use rocket_network::{
    keep_dialed_by, should_dial, DeviceId, DialHint, NetworkEvent, PeerCallError, PeerMessage,
    PeerSocket, Presence, PresenceTracker,
};
use rocket_shared::error::{DomainError, DomainResult};
use tokio::sync::{mpsc, watch, Notify};
use tokio::time::MissedTickBehavior;

use super::service::NetworkService;

/// How long to wait for the peer's socket when there is no address to dial.
const NO_ADDRESS_WAIT: Duration = Duration::from_secs(30);
const DIAL_TIMEOUT: Duration = Duration::from_secs(10);
const UNPAIR_SEND_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_REMEMBERED_ADDRESSES: usize = 5;
/// After a pairing is saved here, a "not paired" answer is not trusted for this long. The
/// other side may still be saving its half of the pairing.
const NOT_PAIRED_GRACE: Duration = Duration::from_secs(30);
/// Sockets the peer opened that wait for the task to take them.
const INBOX_SIZE: usize = 4;

/// The live state of one paired device's connection.
pub(super) struct ConnEntry {
    pub presence: PresenceTracker,
    /// Sockets the peer opened to us arrive here.
    pub inbox: mpsc::Sender<PeerSocket>,
    /// Pinged when a new sighting says "retry at once".
    pub wake: Arc<Notify>,
    pub outbound: Option<mpsc::Sender<PeerMessage>>,
    pub dialed_by: Option<DeviceId>,
    pub needs_update: bool,
    /// Set for a pairing saved a moment ago. Until then `NotPaired` is not a removal.
    pub young_until: Option<Instant>,
}

enum DialResult {
    Socket(PeerSocket),
    NotPaired,
    Failed,
}

enum SocketEnd {
    /// The socket closed or went silent. Retry after the backoff.
    Closed,
    /// The peer sent `Unpair`. The pairing is gone.
    Unpaired,
    /// The node is stopping.
    Stopped,
    /// The pairing was removed here.
    Removed,
}

/// Whether a Hello frame carries the protocol version we speak.
fn version_matches(hello: &PeerMessage) -> bool {
    matches!(hello, PeerMessage::Hello { version, .. } if *version == PROTOCOL_VERSION)
}

impl NetworkService {
    /// Asks the task of a paired device to retry now.
    pub(super) fn wake_device(&self, id: &DeviceId) {
        if let Some(entry) = self.lock().conns.get(id) {
            entry.wake.notify_one();
        }
    }

    fn with_conn<R>(&self, id: &DeviceId, act: impl FnOnce(&mut ConnEntry) -> R) -> Option<R> {
        self.lock().conns.get_mut(id).map(act)
    }

    /// Starts the task for a paired device, once. `young` is true for a pairing saved just now.
    pub(super) fn ensure_connection(&self, id: &DeviceId, young: bool) {
        let Some(me) = self.arc() else {
            return;
        };
        let started = {
            let mut st = self.lock();
            let Some(stop) = st.runtime.as_ref().map(|r| r.stop.clone()) else {
                return;
            };
            if st.conns.contains_key(id) {
                return;
            }
            let (inbox_tx, inbox_rx) = mpsc::channel(INBOX_SIZE);
            let wake = Arc::new(Notify::new());
            st.conns.insert(
                *id,
                ConnEntry {
                    presence: PresenceTracker::new(),
                    inbox: inbox_tx,
                    wake: Arc::clone(&wake),
                    outbound: None,
                    dialed_by: None,
                    needs_update: false,
                    young_until: young.then(|| Self::now() + NOT_PAIRED_GRACE),
                },
            );
            (inbox_rx, wake, stop)
        };
        let (inbox, wake, stop) = started;
        let id = *id;
        self.spawn(async move { me.run_device(id, inbox, wake, stop).await });
    }

    /// Takes a socket the peer opened. Only a paired, unblocked device gets one.
    pub(super) async fn accept_socket(&self, socket: PeerSocket) {
        let id = socket.peer.device_id;
        if !self.paired_and_not_blocked(&id) {
            return;
        }
        self.ensure_connection(&id, false);
        let inbox = self.lock().conns.get(&id).map(|c| c.inbox.clone());
        if let Some(inbox) = inbox {
            // A full or closed inbox drops the socket, which closes it.
            let _ = inbox.try_send(socket);
        }
    }

    // ----- the task -----

    async fn run_device(
        &self,
        id: DeviceId,
        mut inbox: mpsc::Receiver<PeerSocket>,
        wake: Arc<Notify>,
        mut stop: watch::Receiver<bool>,
    ) {
        let Some(local) = self.local_id() else {
            return;
        };
        let mut idle_since = Self::now();
        let mut skip_dial = false;
        loop {
            if *stop.borrow() {
                return;
            }
            if !matches!(self.deps.paired.get(&id), Ok(Some(_))) {
                self.lock().conns.remove(&id);
                return;
            }
            let (candidates, hint) = self.dial_candidates(&id);
            let waited = Self::now().saturating_duration_since(idle_since);
            let allowed = should_dial(&local, &id, hint, waited);
            let mut socket = None;
            if !skip_dial && !candidates.is_empty() && allowed {
                match self.dial(&id, &candidates).await {
                    DialResult::Socket(found) => socket = Some(found),
                    DialResult::NotPaired if self.is_young(&id) => {
                        // The other side may still be saving its half of a new pairing.
                        self.mark_disconnected(&id);
                    }
                    DialResult::NotPaired => {
                        // The peer removed us while we were away.
                        self.remove_local(&id, true);
                        return;
                    }
                    DialResult::Failed => self.mark_disconnected(&id),
                }
            }
            skip_dial = false;

            if socket.is_none() {
                let delay = if candidates.is_empty() {
                    NO_ADDRESS_WAIT
                } else if !allowed {
                    DIAL_GRACE
                        .saturating_sub(waited)
                        .max(Duration::from_secs(1))
                } else {
                    self.retry_delay(&id)
                };
                tokio::select! {
                    next = inbox.recv() => match next {
                        Some(next) => socket = Some(next),
                        None => return,
                    },
                    _ = wake.notified() => {}
                    _ = tokio::time::sleep(delay) => {}
                    _ = stop.changed() => return,
                }
            }

            if let Some(socket) = socket {
                match self.run_socket(&id, socket, &mut inbox, &mut stop).await {
                    SocketEnd::Closed => {
                        self.mark_disconnected(&id);
                        idle_since = Self::now();
                        skip_dial = true;
                    }
                    SocketEnd::Unpaired | SocketEnd::Stopped | SocketEnd::Removed => return,
                }
            }
        }
    }

    /// Addresses to try, and whether the dialer rule reads them as mDNS or manual.
    fn dial_candidates(&self, id: &DeviceId) -> (Vec<SocketAddr>, DialHint) {
        let last = self
            .deps
            .paired
            .get(id)
            .ok()
            .flatten()
            .map(|p| p.last_addresses)
            .unwrap_or_default();
        let (mdns, manual) = self
            .lock()
            .seen
            .get(id)
            .map(|s| (s.mdns.clone(), s.manual.clone()))
            .unwrap_or_default();
        let hint = if mdns.is_empty() && !manual.is_empty() {
            DialHint::ManualAddress
        } else {
            DialHint::Mdns
        };
        let mut out: Vec<SocketAddr> = Vec::new();
        for address in mdns.into_iter().chain(manual).chain(last) {
            if !out.contains(&address) {
                out.push(address);
            }
        }
        (out, hint)
    }

    /// Blocked, or the block list cannot be read. Fails closed.
    fn is_blocked(&self, id: &DeviceId) -> bool {
        self.deps.blocked.contains(id).unwrap_or(true)
    }

    fn is_young(&self, id: &DeviceId) -> bool {
        let now = Self::now();
        self.with_conn(id, |c| c.young_until.is_some_and(|until| until > now))
            .unwrap_or(false)
    }

    fn retry_delay(&self, id: &DeviceId) -> Duration {
        self.with_conn(id, |c| c.presence.next_retry_delay())
            .unwrap_or(Duration::from_secs(1))
    }

    async fn dial(&self, id: &DeviceId, candidates: &[SocketAddr]) -> DialResult {
        let Some(connector) = self.connector() else {
            return DialResult::Failed;
        };
        for address in candidates {
            let hello = self.local_hello_message();
            let attempt =
                tokio::time::timeout(DIAL_TIMEOUT, connector.open_socket(*address, id, hello)).await;
            match attempt {
                Ok(Ok(socket)) => {
                    self.remember_address(id, *address);
                    return DialResult::Socket(socket);
                }
                // Only the genuine device (its key matched) can say "not paired".
                Ok(Err(PeerCallError::NotPaired)) => return DialResult::NotPaired,
                _ => continue,
            }
        }
        DialResult::Failed
    }

    /// Saves the address that worked first in the device's list.
    fn remember_address(&self, id: &DeviceId, address: SocketAddr) {
        let _guard = self.paired_guard();
        let Ok(Some(mut device)) = self.deps.paired.get(id) else {
            return;
        };
        if device.last_addresses.first() == Some(&address) {
            return;
        }
        device.last_addresses.retain(|a| *a != address);
        device.last_addresses.insert(0, address);
        device.last_addresses.truncate(MAX_REMEMBERED_ADDRESSES);
        if let Err(e) = self.deps.paired.upsert(device) {
            tracing::debug!(code = "network_address_not_saved", error = %e, "address not saved");
        }
    }

    fn set_connected(&self, id: &DeviceId, socket: &PeerSocket) {
        let changed = self
            .with_conn(id, |c| {
                let was = c.presence.state();
                c.presence.on_connected();
                c.outbound = Some(socket.outbound.clone());
                c.dialed_by = Some(socket.dialed_by);
                c.needs_update = false;
                was != Presence::Online
            })
            .unwrap_or(false);
        if changed {
            self.emit(NetworkEvent::NodesChanged);
        }
    }

    fn mark_disconnected(&self, id: &DeviceId) {
        let changed = self
            .with_conn(id, |c| {
                let was = c.presence.state();
                c.presence.on_disconnected();
                c.outbound = None;
                c.dialed_by = None;
                was != Presence::Offline
            })
            .unwrap_or(false);
        if changed {
            self.emit(NetworkEvent::NodesChanged);
        }
    }

    /// Runs one socket until it ends. A new socket from the inbox replaces it when it wins the
    /// duplicate rule (the one dialled by the lower id is kept).
    async fn run_socket(
        &self,
        id: &DeviceId,
        first: PeerSocket,
        inbox: &mut mpsc::Receiver<PeerSocket>,
        stop: &mut watch::Receiver<bool>,
    ) -> SocketEnd {
        let Some(local) = self.local_id() else {
            return SocketEnd::Stopped;
        };
        let keep = keep_dialed_by(&local, id);
        let mut socket = first;
        if !version_matches(&socket.remote_hello) {
            self.with_conn(id, |c| c.needs_update = true);
            return SocketEnd::Closed;
        }
        self.set_connected(id, &socket);
        let mut ticker =
            tokio::time::interval_at(tokio::time::Instant::now() + PING_INTERVAL, PING_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = stop.changed() => return SocketEnd::Stopped,
                frame = socket.inbound.recv() => match frame {
                    None => return SocketEnd::Closed,
                    // A blocked device gets no answer to any frame. Its entry goes, so the
                    // socket closes when this function returns.
                    Some(_) if self.is_blocked(id) => {
                        self.lock().conns.remove(id);
                        return SocketEnd::Removed;
                    }
                    Some(PeerMessage::Ping) => {
                        let _ = socket.outbound.try_send(PeerMessage::Pong);
                    }
                    Some(PeerMessage::Pong) => {
                        self.with_conn(id, |c| c.presence.on_pong());
                    }
                    Some(PeerMessage::Unpair) => {
                        self.remove_local(id, true);
                        return SocketEnd::Unpaired;
                    }
                    // A second Hello is a protocol error.
                    Some(PeerMessage::Hello { .. }) => return SocketEnd::Closed,
                },
                _ = ticker.tick() => {
                    let state = self.with_conn(id, |c| c.presence.on_ping_sent());
                    if state != Some(Presence::Online) {
                        return SocketEnd::Closed;
                    }
                    let _ = socket.outbound.try_send(PeerMessage::Ping);
                }
                next = inbox.recv() => match next {
                    None => return SocketEnd::Removed,
                    Some(new) => {
                        let wins = new.dialed_by == keep || new.dialed_by == socket.dialed_by;
                        if wins && version_matches(&new.remote_hello) {
                            socket = new;
                            self.set_connected(id, &socket);
                        }
                    }
                },
            }
        }
    }

    // ----- removing a pairing -----

    /// Removes the pairing here and tells the user. `by_peer` is true when the peer did it.
    pub(super) fn remove_local(&self, id: &DeviceId, by_peer: bool) -> bool {
        let name = self.known_name(id);
        let removed = {
            let _guard = self.paired_guard();
            self.deps.paired.remove(id)
        };
        let removed = match removed {
            Ok(removed) => removed,
            Err(e) => {
                tracing::warn!(code = "network_unpair_failed", error = %e, "pairing not removed");
                return false;
            }
        };
        let entry = self.lock().conns.remove(id);
        drop(entry);
        if removed {
            tracing::info!(code = "network_device_unpaired", id = %id.short(), by_peer, "unpaired");
            let actor = if by_peer {
                AuditActor::System
            } else {
                AuditActor::User
            };
            self.audit(
                actor,
                AuditEventKind::NetworkDeviceUnpaired {
                    device_id: id.to_base32(),
                    by_peer,
                },
            );
            self.emit(NetworkEvent::PairedRemoved {
                device_id: id.to_base32(),
                display_name: name,
                by_peer,
            });
            self.emit(NetworkEvent::NodesChanged);
        }
        removed
    }

    /// Removes a paired device. When a socket is open, the peer is told first.
    pub async fn remove_paired(self: &Arc<Self>, device_id: &DeviceId) -> DomainResult<()> {
        if !self.unpair_by_user(device_id).await? {
            return Err(DomainError::InvalidInput(
                "That device is not paired.".to_string(),
            ));
        }
        Ok(())
    }

    /// The local user's unpair, shared by Remove and Block. Sends `Unpair` on a live socket,
    /// drops the connection entry (which closes the socket), audits and tells the UI. Returns
    /// false when the device was not paired.
    pub(super) async fn unpair_by_user(&self, device_id: &DeviceId) -> DomainResult<bool> {
        let outbound = self
            .lock()
            .conns
            .get(device_id)
            .and_then(|c| c.outbound.clone());
        let name = self.known_name(device_id);
        let removed = {
            let _guard = self.paired_guard();
            self.deps.paired.remove(device_id)?
        };
        if !removed {
            return Ok(false);
        }
        if let Some(outbound) = outbound {
            let _ =
                tokio::time::timeout(UNPAIR_SEND_TIMEOUT, outbound.send(PeerMessage::Unpair)).await;
        }
        let entry = self.lock().conns.remove(device_id);
        drop(entry);
        tracing::info!(code = "network_device_unpaired", id = %device_id.short(), by_peer = false, "unpaired");
        self.audit(
            AuditActor::User,
            AuditEventKind::NetworkDeviceUnpaired {
                device_id: device_id.to_base32(),
                by_peer: false,
            },
        );
        self.emit(NetworkEvent::PairedRemoved {
            device_id: device_id.to_base32(),
            display_name: name,
            by_peer: false,
        });
        self.emit(NetworkEvent::NodesChanged);
        Ok(true)
    }
}
```

- [ ] **Step 8: Run the tests and see them pass**

Run: `cargo test -j4 -p rocket-app network`
Expected: PASS. All `network::service_tests`, `network::pairing_tests` and `network::connections_tests` pass, 0 failed. If a paused-time test hangs, look for a `MutexGuard` held across an `.await` in the new files. If `it_reconnects_after_a_drop_with_growing_delays` is off by one, check that `run_device` sets `skip_dial` after `SocketEnd::Closed` so the first retry waits `retry_delay`. If `paired_nodes_open_exactly_one_connection` counts two opens, `should_dial` is not being given the `waited` time since the task started.

- [ ] **Step 9: Check the crate**

Run: `cargo check -j4 -p rocket-app`
Expected: Finished, no errors and no unused-item warnings from the `network` module (fix any that remain, for example an unused import).

No commit; reviewed at the plan end.

---

### Task 3: Tauri commands, DTOs, wiring, start at launch and stop on exit

**Files:**
- Create: `src-tauri/src/commands/network.rs`, `src-tauri/src/commands/slot.rs`
- Modify: `src-tauri/Cargo.toml`, `src-tauri/src/commands/mod.rs`, `src-tauri/src/commands/import.rs`, `src-tauri/src/lib.rs`, `src-tauri/app-commands.txt`

**Interfaces:**

```rust
// src-tauri/src/commands/network.rs (14 commands, names and args from the index table)
network_status() -> NetworkStatusDto
network_list_nodes() -> Vec<NodeViewDto>
network_set_visibility(hidden: bool) -> Result<(), DomainError>
network_set_profile(display_name: String) -> Result<(), DomainError>
network_mark_intro_seen() -> Result<(), DomainError>
network_list_addresses() -> Result<Vec<ManualAddressDto>, DomainError>
network_add_address(host: String, port: u16) -> Result<(), DomainError>
network_remove_address(host: String, port: u16) -> Result<(), DomainError>
network_request_pairing(device_id: String) -> Result<PairingStartedDto, DomainError>
network_respond_pairing(pairing_id: String, decision: String) -> Result<(), DomainError>
network_cancel_pairing(pairing_id: String) -> Result<(), DomainError>
network_remove_paired(device_id: String) -> Result<(), DomainError>
network_list_blocked() -> Result<Vec<BlockedDeviceDto>, DomainError>
network_unblock(device_id: String) -> Result<(), DomainError>

pub(crate) type NetworkSlot<H = NetworkHandle> = Slot<H>;
pub(crate) fn start_network(app: &AppHandle);

// src-tauri/src/commands/slot.rs (moved out of import.rs, shared by the sync scheduler and the node)
pub(crate) trait Stoppable { fn stop(&self); }
pub(crate) struct Slot<H: Stoppable>;
impl<H: Stoppable> Slot<H> {
    pub(crate) fn replace(&self, handle: H);   // stops the old handle; after stop_for_good, stops the new one
    pub(crate) fn stop(&self);                 // stops and drops the handle; does not latch
    pub(crate) fn stop_for_good(&self);        // stops, and latches so a late replace stops its handle
}

// src-tauri/src/commands/import.rs
pub(crate) type SyncSchedulerSlot<H = rocket_app::SchedulerHandle> = Slot<H>;
```

Rules for the task:

- Commands stay thin: parse text, call `NetworkService`, map the result. All state rules live in `rocket-app`.
- The service is managed as `Arc<NetworkService>`, so commands take `State<'_, Arc<NetworkService>>`.
- `decision` is `"accept"`, `"decline"` or `"block"`; anything else is `InvalidInput`. A device id that is not valid base32 is `InvalidInput` with a stable text.
- DTO shapes are exactly the index: `NetworkStatusDto { state, offReason, port, listenerAvailable, discoveryAvailable, hidden, displayName, initials, deviceShortId, introSeen, warnings }`, `NodeViewDto { deviceId, shortId, displayName, initials, group, status, source }` (`status` is `online`, `offline`, `pairing`, `needs_update` or `available`), `ManualAddressDto { host, port, addedAt }`, `BlockedDeviceDto { deviceId, shortId, name, blockedAt }`, `PairingStartedDto { pairingId, code }` (code is the six digits without a space).
- `start_network` runs `NetworkService::start` on the Tauri async runtime (it is async and needs the runtime), then stores the handle in `NetworkSlot`. The exit paths call `stop_for_good()` on the slot, which latches, so a stop that arrives before `start` finishes still stops the late handle.
- `NetworkSlot` and `SyncSchedulerSlot` are two aliases of one generic `Slot<H>` in `commands/slot.rs`, together with the `Stoppable` trait (moved out of `import.rs`). The sync scheduler is replaced on every workspace switch, so its plain `stop()` must not latch: only `stop_for_good()` does, and only the node's exit paths use it. The generic slot tests are written once, in `slot.rs`.
- The service reads the OS user name from `USER`, then `USERNAME`. The service falls back to "Rocket user".
- Exit paths: `shutdown_services` (normal exit) and `spawn_exit_signal_listener` (SIGINT, SIGTERM, SIGHUP) both call `stop_for_good()` on the node slot.

- [ ] **Step 1: Write the failing DTO, parser and slot tests**

First create `src-tauri/src/commands/slot.rs` with the slot tests only (the implementation comes in Step 3). They are the generic tests, written once for both aliases:

```rust
#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use super::*;

    struct Counting(Arc<AtomicUsize>);

    impl Stoppable for Counting {
        fn stop(&self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn counter() -> Arc<AtomicUsize> {
        Arc::new(AtomicUsize::new(0))
    }

    #[test]
    fn replacing_the_handle_stops_the_old_one() {
        let (first, second) = (counter(), counter());
        let slot = Slot::<Counting>::default();
        slot.replace(Counting(first.clone()));
        slot.replace(Counting(second.clone()));
        assert_eq!(first.load(Ordering::SeqCst), 1);
        assert_eq!(second.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn stopping_twice_stops_the_handle_once() {
        let count = counter();
        let slot = Slot::<Counting>::default();
        slot.replace(Counting(count.clone()));
        slot.stop();
        slot.stop();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_plain_stop_does_not_latch() {
        let (first, second) = (counter(), counter());
        let slot = Slot::<Counting>::default();
        slot.replace(Counting(first.clone()));
        slot.stop();
        slot.replace(Counting(second.clone()));
        assert_eq!(first.load(Ordering::SeqCst), 1);
        assert_eq!(second.load(Ordering::SeqCst), 0, "a workspace switch keeps the new handle");
        slot.stop();
        assert_eq!(second.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn stopping_for_good_stops_the_kept_handle_once() {
        let count = counter();
        let slot = Slot::<Counting>::default();
        slot.replace(Counting(count.clone()));
        slot.stop_for_good();
        slot.stop_for_good();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_handle_that_arrives_after_stopping_for_good_is_stopped_at_once() {
        let count = counter();
        let slot = Slot::<Counting>::default();
        slot.stop_for_good();
        slot.replace(Counting(count.clone()));
        assert_eq!(count.load(Ordering::SeqCst), 1, "exit raced the start");
    }
}
```

Then create `src-tauri/src/commands/network.rs` with the tests only. The test module needs these names to exist, so the file at this point is just:

```rust
#[cfg(test)]
mod tests {
    use rocket_app::{
        NetworkState, NetworkStatus, NodeGroup, NodeStatus, NodeView, PairingStarted,
    };
    use rocket_network::{
        BlockedDevice, DeviceId, DiscoverySource, ManualAddress, NodeProfile, PairingCode,
        PairingId,
    };
    use rocket_shared::error::DomainError;

    use super::*;

    fn keys(value: &serde_json::Value) -> Vec<String> {
        let mut keys: Vec<String> = value
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    fn sample_status(state: NetworkState) -> NetworkStatus {
        NetworkStatus {
            state,
            port: Some(47821),
            listener_available: true,
            discovery_available: false,
            hidden: false,
            profile: NodeProfile::new("Sam Lee").expect("profile"),
            device_short_id: Some("K7QF-2M9X".to_string()),
            intro_seen: true,
            warnings: vec!["paired.yml was damaged and has been reset.".to_string()],
        }
    }

    #[test]
    fn the_status_dto_uses_camel_case_keys() {
        let dto = NetworkStatusDto::from(sample_status(NetworkState::Running));
        let value = serde_json::to_value(&dto).expect("json");
        assert_eq!(
            keys(&value),
            vec![
                "deviceShortId",
                "discoveryAvailable",
                "displayName",
                "hidden",
                "initials",
                "introSeen",
                "listenerAvailable",
                "offReason",
                "port",
                "state",
                "warnings"
            ]
        );
        assert_eq!(value["state"], "running");
        assert_eq!(value["offReason"], serde_json::Value::Null);
        assert_eq!(value["displayName"], "Sam Lee");
        assert_eq!(value["initials"], "SL");
        assert_eq!(value["port"], 47821);
        assert_eq!(value["discoveryAvailable"], false);
    }

    #[test]
    fn an_off_status_carries_its_reason() {
        let reason = "The keychain is not available, so networking is off.".to_string();
        let dto = NetworkStatusDto::from(sample_status(NetworkState::Off {
            reason: reason.clone(),
        }));
        let value = serde_json::to_value(&dto).expect("json");
        assert_eq!(value["state"], "off");
        assert_eq!(value["offReason"], reason);
    }

    #[test]
    fn the_node_dto_uses_camel_case_keys_and_snake_case_words() {
        let view = NodeView {
            device_id: "ABC".to_string(),
            short_id: "K7QF-2M9X".to_string(),
            display_name: "Ben Ortiz".to_string(),
            initials: "BO".to_string(),
            group: NodeGroup::Nearby,
            status: NodeStatus::NeedsUpdate,
            source: Some(DiscoverySource::Mdns),
        };
        let value = serde_json::to_value(NodeViewDto::from(view)).expect("json");
        assert_eq!(
            keys(&value),
            vec![
                "deviceId",
                "displayName",
                "group",
                "initials",
                "shortId",
                "source",
                "status"
            ]
        );
        assert_eq!(value["group"], "nearby");
        assert_eq!(value["status"], "needs_update");
        assert_eq!(value["source"], "mdns");
    }

    #[test]
    fn every_node_word_matches_the_contract() {
        let words = [
            (NodeStatus::Online, "online"),
            (NodeStatus::Offline, "offline"),
            (NodeStatus::Pairing, "pairing"),
            (NodeStatus::NeedsUpdate, "needs_update"),
            (NodeStatus::Available, "available"),
        ];
        for (status, word) in words {
            let view = NodeView {
                device_id: String::new(),
                short_id: String::new(),
                display_name: String::new(),
                initials: String::new(),
                group: NodeGroup::Paired,
                status,
                source: Some(DiscoverySource::Manual),
            };
            let value = serde_json::to_value(NodeViewDto::from(view)).expect("json");
            assert_eq!(value["status"], word);
            assert_eq!(value["group"], "paired");
            assert_eq!(value["source"], "manual");
        }
    }

    #[test]
    fn the_small_dtos_use_camel_case_keys() {
        let address = ManualAddress {
            host: "team.example".to_string(),
            port: 47821,
            added_at: chrono::Utc::now(),
        };
        let value = serde_json::to_value(ManualAddressDto::from(address)).expect("json");
        assert_eq!(keys(&value), vec!["addedAt", "host", "port"]);
        assert!(value["addedAt"].is_string());

        let blocked = BlockedDevice {
            device_id: DeviceId::from_bytes([3; 32]),
            name: "Dee".to_string(),
            blocked_at: chrono::Utc::now(),
        };
        let value = serde_json::to_value(BlockedDeviceDto::from(blocked)).expect("json");
        assert_eq!(keys(&value), vec!["blockedAt", "deviceId", "name", "shortId"]);
        assert_eq!(value["deviceId"], DeviceId::from_bytes([3; 32]).to_base32());

        let started = PairingStarted {
            pairing_id: PairingId("abc".to_string()),
            code: PairingCode(4_213),
        };
        let value = serde_json::to_value(PairingStartedDto::from(started)).expect("json");
        assert_eq!(keys(&value), vec!["code", "pairingId"]);
        assert_eq!(value["code"], "004213", "six digits, no space");
        assert_eq!(value["pairingId"], "abc");
    }

    #[test]
    fn decisions_parse_and_other_words_are_rejected() {
        assert_eq!(parse_decision("accept"), Ok(PairingDecision::Accept));
        assert_eq!(parse_decision("decline"), Ok(PairingDecision::Decline));
        assert_eq!(parse_decision("block"), Ok(PairingDecision::Block));
        for bad in ["", "Accept", "yes", "accept "] {
            assert!(
                matches!(parse_decision(bad), Err(DomainError::InvalidInput(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn device_ids_parse_or_are_invalid_input() {
        let id = DeviceId::from_bytes([9; 32]);
        assert_eq!(parse_device_id(&id.to_base32()), Ok(id));
        for bad in ["", "nope", "K7QF-2M9X", &"A".repeat(51)] {
            assert_eq!(
                parse_device_id(bad),
                Err(DomainError::InvalidInput(
                    "That device id is not valid.".to_string()
                )),
                "{bad:?}"
            );
        }
    }
}
```

Register the modules so the files are compiled: in `src-tauri/src/commands/mod.rs`, add `pub mod network;` after `pub mod load_test;` (alphabetical; it goes before `pub mod oauth2;`) and `pub mod slot;` after `pub mod secret_managers;` (before `pub mod templates;`).

- [ ] **Step 2: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket network`
Expected: FAIL to compile, `error[E0425]: cannot find function `parse_decision` in this scope` (and `NetworkStatusDto`, `NodeViewDto`, `Slot`, `Stoppable`, ...). This needs the Cargo dependency first, so do the next step, then rerun to see this error.

In `src-tauri/Cargo.toml`, after the line `rocket-audit.workspace = true` add:

```toml
rocket-network.workspace = true
```

Run `grep -n "rocket-network" Cargo.toml` at the repo root. If there is no `[workspace.dependencies]` entry from Plan 01 (it should be below `rocket-grpc = { path = "crates/rocket-grpc" }`), add `rocket-network = { path = "crates/rocket-network" }` there. Then rerun `cargo test -j4 -p rocket network` and see the error above.

- [ ] **Step 3: Implement the commands, DTOs and the slot**

(a) Put the slot above the test module in `src-tauri/src/commands/slot.rs`. It is the code of `SyncSchedulerSlot` and `Stoppable` that lived in `import.rs`, made opt-in latching: the sync scheduler is replaced on every workspace switch, so its `stop()` must keep accepting a later `replace`.

```rust
//! A slot that holds the handle of one running background task, so a replace or the app exit can
//! stop it.

use std::sync::Mutex;

/// A running background loop that can be told to stop.
pub(crate) trait Stoppable {
    fn stop(&self);
}

struct SlotState<H> {
    handle: Option<H>,
    stopped_for_good: bool,
}

/// Holds one handle. Generic over `Stoppable` so tests can use a counting stand-in.
pub(crate) struct Slot<H: Stoppable>(Mutex<SlotState<H>>);

impl<H: Stoppable> Default for Slot<H> {
    fn default() -> Self {
        Self(Mutex::new(SlotState {
            handle: None,
            stopped_for_good: false,
        }))
    }
}

impl<H: Stoppable> Slot<H> {
    /// Keeps the new handle and stops the old one. After `stop_for_good`, the new handle is
    /// stopped at once instead.
    pub(crate) fn replace(&self, handle: H) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.stopped_for_good {
            handle.stop();
            return;
        }
        if let Some(old) = state.handle.take() {
            old.stop();
        }
        state.handle = Some(handle);
    }

    /// Stops and drops the kept handle. Calling it again does nothing, and a later `replace`
    /// still keeps its handle.
    pub(crate) fn stop(&self) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(old) = state.handle.take() {
            old.stop();
        }
    }

    /// Like `stop`, and also remembers it, so a handle that arrives later (a start that raced the
    /// exit) is stopped at once. Used on the app exit paths only.
    pub(crate) fn stop_for_good(&self) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.stopped_for_good = true;
        if let Some(old) = state.handle.take() {
            old.stop();
        }
    }
}
```

(b) In `src-tauri/src/commands/import.rs`, replace the block from `/// A running background loop that can be told to stop.` through the end of `impl<H: Stoppable> SyncSchedulerSlot<H> { ... }` (the `Stoppable` trait, its `SchedulerHandle` impl, the `SyncSchedulerSlot` struct, its `Default` and its `replace` and `stop`) with:

```rust
impl Stoppable for rocket_app::SchedulerHandle {
    fn stop(&self) {
        rocket_app::SchedulerHandle::stop(self);
    }
}

/// Holds the handle of the running sync supervisor, so a workspace switch or the app exit can
/// stop it. Its `stop()` does not latch, so the next workspace switch can still `replace` it.
pub(crate) type SyncSchedulerSlot<H = rocket_app::SchedulerHandle> = Slot<H>;
```

and add `use super::slot::{Slot, Stoppable};` to the imports at the top of `import.rs`. The two `Slot` types differ by `H`, so Tauri manages `SyncSchedulerSlot` and `NetworkSlot` as separate states. Replace the test module `sync_scheduler_slot` at the bottom of `import.rs` (its two tests move to `slot.rs` as generic tests) with one test that pins the unchanged scheduler behaviour:

```rust
#[cfg(test)]
mod sync_scheduler_slot {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Counting(Arc<AtomicUsize>);
    impl Stoppable for Counting {
        fn stop(&self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn a_workspace_switch_after_a_stop_keeps_the_new_scheduler() {
        let slot = SyncSchedulerSlot::<Counting>::default();
        let (first, second) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        slot.replace(Counting(Arc::clone(&first)));
        slot.stop();
        slot.replace(Counting(Arc::clone(&second)));
        assert_eq!(first.load(Ordering::SeqCst), 1);
        assert_eq!(second.load(Ordering::SeqCst), 0, "stop() must not latch");
    }
}
```

(c) Replace `src-tauri/src/commands/network.rs`, keeping the test module from Step 1 at the bottom unchanged. Put this above it:

```rust
//! Team network commands. They stay thin: parse text, call `NetworkService`, map the result.
//! DTOs are camelCase and live here only; the domain types are not renamed.

use std::sync::Arc;

use rocket_app::{
    NetworkHandle, NetworkService, NetworkState, NetworkStatus, NodeGroup, NodeStatus, NodeView,
    PairingDecision, PairingStarted,
};
use rocket_network::{BlockedDevice, DeviceId, DiscoverySource, ManualAddress, PairingId};
use rocket_shared::error::DomainError;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use super::slot::{Slot, Stoppable};

// ----- DTOs -----

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStatusDto {
    /// `running` or `off`.
    pub state: &'static str,
    pub off_reason: Option<String>,
    pub port: Option<u16>,
    pub listener_available: bool,
    pub discovery_available: bool,
    pub hidden: bool,
    pub display_name: String,
    pub initials: String,
    pub device_short_id: Option<String>,
    pub intro_seen: bool,
    pub warnings: Vec<String>,
}

impl From<NetworkStatus> for NetworkStatusDto {
    fn from(status: NetworkStatus) -> Self {
        let (state, off_reason) = match status.state {
            NetworkState::Running => ("running", None),
            NetworkState::Off { reason } => ("off", Some(reason)),
        };
        Self {
            state,
            off_reason,
            port: status.port,
            listener_available: status.listener_available,
            discovery_available: status.discovery_available,
            hidden: status.hidden,
            display_name: status.profile.display_name,
            initials: status.profile.initials,
            device_short_id: status.device_short_id,
            intro_seen: status.intro_seen,
            warnings: status.warnings,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeViewDto {
    pub device_id: String,
    pub short_id: String,
    pub display_name: String,
    pub initials: String,
    /// `paired` or `nearby`.
    pub group: &'static str,
    /// `online`, `offline`, `pairing`, `needs_update` or `available`.
    pub status: &'static str,
    /// `mdns`, `manual` or none.
    pub source: Option<&'static str>,
}

impl From<NodeView> for NodeViewDto {
    fn from(view: NodeView) -> Self {
        Self {
            device_id: view.device_id,
            short_id: view.short_id,
            display_name: view.display_name,
            initials: view.initials,
            group: match view.group {
                NodeGroup::Paired => "paired",
                NodeGroup::Nearby => "nearby",
            },
            status: match view.status {
                NodeStatus::Online => "online",
                NodeStatus::Offline => "offline",
                NodeStatus::Pairing => "pairing",
                NodeStatus::NeedsUpdate => "needs_update",
                NodeStatus::Available => "available",
            },
            source: view.source.map(|source| match source {
                DiscoverySource::Mdns => "mdns",
                DiscoverySource::Manual => "manual",
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualAddressDto {
    pub host: String,
    pub port: u16,
    /// RFC 3339.
    pub added_at: String,
}

impl From<ManualAddress> for ManualAddressDto {
    fn from(address: ManualAddress) -> Self {
        Self {
            host: address.host,
            port: address.port,
            added_at: address.added_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockedDeviceDto {
    pub device_id: String,
    pub short_id: String,
    pub name: String,
    /// RFC 3339.
    pub blocked_at: String,
}

impl From<BlockedDevice> for BlockedDeviceDto {
    fn from(device: BlockedDevice) -> Self {
        Self {
            device_id: device.device_id.to_base32(),
            short_id: device.device_id.short(),
            name: device.name,
            blocked_at: device.blocked_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingStartedDto {
    pub pairing_id: String,
    /// Six digits without a space. The UI groups them as `482 913`.
    pub code: String,
}

impl From<PairingStarted> for PairingStartedDto {
    fn from(started: PairingStarted) -> Self {
        Self {
            pairing_id: started.pairing_id.0,
            code: started.code.digits(),
        }
    }
}

// ----- parsing -----

fn parse_device_id(raw: &str) -> Result<DeviceId, DomainError> {
    DeviceId::parse_base32(raw)
        .map_err(|_| DomainError::InvalidInput("That device id is not valid.".to_string()))
}

fn parse_decision(raw: &str) -> Result<PairingDecision, DomainError> {
    match raw {
        "accept" => Ok(PairingDecision::Accept),
        "decline" => Ok(PairingDecision::Decline),
        "block" => Ok(PairingDecision::Block),
        _ => Err(DomainError::InvalidInput(
            "The decision must be accept, decline or block.".to_string(),
        )),
    }
}

// ----- commands -----

#[tauri::command]
pub fn network_status(svc: State<'_, Arc<NetworkService>>) -> NetworkStatusDto {
    svc.status().into()
}

#[tauri::command]
pub fn network_list_nodes(svc: State<'_, Arc<NetworkService>>) -> Vec<NodeViewDto> {
    svc.list_nodes().into_iter().map(Into::into).collect()
}

#[tauri::command]
pub async fn network_set_visibility(
    hidden: bool,
    svc: State<'_, Arc<NetworkService>>,
) -> Result<(), DomainError> {
    svc.set_hidden(hidden).await
}

#[tauri::command]
pub async fn network_set_profile(
    display_name: String,
    svc: State<'_, Arc<NetworkService>>,
) -> Result<(), DomainError> {
    svc.set_display_name(&display_name).await
}

#[tauri::command]
pub fn network_mark_intro_seen(svc: State<'_, Arc<NetworkService>>) -> Result<(), DomainError> {
    svc.mark_intro_seen()
}

#[tauri::command]
pub fn network_list_addresses(
    svc: State<'_, Arc<NetworkService>>,
) -> Result<Vec<ManualAddressDto>, DomainError> {
    Ok(svc
        .list_addresses()?
        .into_iter()
        .map(Into::into)
        .collect())
}

#[tauri::command]
pub async fn network_add_address(
    host: String,
    port: u16,
    svc: State<'_, Arc<NetworkService>>,
) -> Result<(), DomainError> {
    svc.add_address(&host, port).await
}

#[tauri::command]
pub fn network_remove_address(
    host: String,
    port: u16,
    svc: State<'_, Arc<NetworkService>>,
) -> Result<(), DomainError> {
    svc.remove_address(&host, port)
}

#[tauri::command]
pub async fn network_request_pairing(
    device_id: String,
    svc: State<'_, Arc<NetworkService>>,
) -> Result<PairingStartedDto, DomainError> {
    let id = parse_device_id(&device_id)?;
    svc.request_pairing(&id).await.map(Into::into)
}

#[tauri::command]
pub async fn network_respond_pairing(
    pairing_id: String,
    decision: String,
    svc: State<'_, Arc<NetworkService>>,
) -> Result<(), DomainError> {
    let decision = parse_decision(&decision)?;
    svc.respond_pairing(&PairingId(pairing_id), decision).await
}

#[tauri::command]
pub fn network_cancel_pairing(
    pairing_id: String,
    svc: State<'_, Arc<NetworkService>>,
) -> Result<(), DomainError> {
    svc.cancel_pairing(&PairingId(pairing_id))
}

#[tauri::command]
pub async fn network_remove_paired(
    device_id: String,
    svc: State<'_, Arc<NetworkService>>,
) -> Result<(), DomainError> {
    let id = parse_device_id(&device_id)?;
    svc.remove_paired(&id).await
}

#[tauri::command]
pub fn network_list_blocked(
    svc: State<'_, Arc<NetworkService>>,
) -> Result<Vec<BlockedDeviceDto>, DomainError> {
    Ok(svc.list_blocked()?.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub fn network_unblock(
    device_id: String,
    svc: State<'_, Arc<NetworkService>>,
) -> Result<(), DomainError> {
    let id = parse_device_id(&device_id)?;
    svc.unblock(&id)
}

// ----- the running node -----

impl Stoppable for NetworkHandle {
    fn stop(&self) {
        NetworkHandle::stop(self);
    }
}

/// The kept node handle. The exit paths call `stop_for_good()`, so a node that finishes starting
/// after the exit began is stopped at once.
pub(crate) type NetworkSlot<H = NetworkHandle> = Slot<H>;

/// Starts the node on the app runtime and keeps its handle in the slot.
pub(crate) fn start_network(app: &AppHandle) {
    let Some(svc) = app.try_state::<Arc<NetworkService>>() else {
        tracing::error!(code = "network_service_missing", "networking not started");
        return;
    };
    let svc = Arc::clone(svc.inner());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let handle = svc.start().await;
        match app.try_state::<NetworkSlot>() {
            Some(slot) => slot.replace(handle),
            None => handle.stop(),
        }
    });
}
```

- [ ] **Step 4: Register the commands and wire the service**

In `src-tauri/app-commands.txt`, between the lines `move_item` and `oauth2_decode_jwt`, add these 14 lines in this order (already sorted):

```
network_add_address
network_cancel_pairing
network_list_addresses
network_list_blocked
network_list_nodes
network_mark_intro_seen
network_remove_address
network_remove_paired
network_request_pairing
network_respond_pairing
network_set_profile
network_set_visibility
network_status
network_unblock
```

In `src-tauri/src/lib.rs`, in `generate_handler!`, after the line `            commands::history::clear_history,` add:

```rust
            commands::network::network_status,
            commands::network::network_list_nodes,
            commands::network::network_set_visibility,
            commands::network::network_set_profile,
            commands::network::network_mark_intro_seen,
            commands::network::network_list_addresses,
            commands::network::network_add_address,
            commands::network::network_remove_address,
            commands::network::network_request_pairing,
            commands::network::network_respond_pairing,
            commands::network::network_cancel_pairing,
            commands::network::network_remove_paired,
            commands::network::network_list_blocked,
            commands::network::network_unblock,
```

In the `setup` closure, after the line `            app.manage(audit_publisher.clone());` add the service and the slot:

```rust

            // Team network: discovery, pairing and presence. Files live in ~/.rocket-api/network/
            // and never inside a workspace.
            let network_dir = data_dir.join("network");
            let network_paired = Arc::new(rocket_infra::network::FsPairedDeviceRepo::new(
                network_dir.join("paired.yml"),
            ));
            let network_blocked = Arc::new(rocket_infra::network::FsBlockedDeviceRepo::new(
                network_dir.join("blocked.yml"),
            ));
            let network_addresses = Arc::new(rocket_infra::network::FsManualAddressRepo::new(
                network_dir.join("addresses.yml"),
            ));
            let network_settings = Arc::new(rocket_infra::network::FsNetworkSettingsRepo::new(
                network_dir.join("settings.yml"),
            ));
            let network_svc = rocket_app::NetworkService::new(rocket_app::NetworkDeps {
                identity: Arc::new(rocket_infra::network::KeychainDeviceIdentity::new(
                    Arc::new(KeyringSecretStore::new_device_identity()),
                )),
                paired: network_paired.clone(),
                blocked: network_blocked.clone(),
                addresses: network_addresses.clone(),
                settings: network_settings.clone(),
                quarantine: vec![
                    network_paired as Arc<dyn rocket_network::QuarantineReport>,
                    network_blocked as Arc<dyn rocket_network::QuarantineReport>,
                    network_addresses as Arc<dyn rocket_network::QuarantineReport>,
                    network_settings as Arc<dyn rocket_network::QuarantineReport>,
                ],
                discovery: Arc::new(rocket_infra::network::MdnsDiscovery::new()),
                transport: Arc::new(rocket_infra::network::RustlsPeerTransportFactory::new()),
                events: Arc::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
                audit: audit_publisher.clone(),
                os_user_name: std::env::var("USER")
                    .ok()
                    .or_else(|| std::env::var("USERNAME").ok()),
            });
            app.manage(network_svc);
            app.manage(commands::network::NetworkSlot::<rocket_app::NetworkHandle>::default());
```

After the line `            commands::updates::start_update_checker(app.handle());` add:

```rust

            // Start the network node on the app runtime. The slot stops it at exit.
            commands::network::start_network(app.handle());
```

In `shutdown_services`, replace the first lines

```rust
pub(crate) fn shutdown_services(app_handle: &tauri::AppHandle) {
    if let Some(slot) = app_handle.try_state::<commands::import::SyncSchedulerSlot>() {
        slot.stop();
    }
```

with

```rust
pub(crate) fn shutdown_services(app_handle: &tauri::AppHandle) {
    if let Some(slot) = app_handle.try_state::<commands::import::SyncSchedulerSlot>() {
        slot.stop();
    }
    if let Some(slot) = app_handle.try_state::<commands::network::NetworkSlot>() {
        // Closes the listener and withdraws the mDNS announcement.
        slot.stop_for_good();
    }
```

In `spawn_exit_signal_listener`, after the block

```rust
            if attempt > 0 {
                std::process::exit(1);
            }
```

add:

```rust
            if let Some(slot) = app_handle.try_state::<commands::network::NetworkSlot>() {
                slot.stop_for_good();
            }
```

- [ ] **Step 5: Run the command tests and the ACL tests**

Run: `cargo test -j4 -p rocket network`
Expected: PASS. The 7 tests in `commands::network::tests` pass, 0 failed.

Run: `cargo test -j4 -p rocket slot`
Expected: PASS. The 5 tests in `commands::slot::tests` and `commands::import::sync_scheduler_slot::a_workspace_switch_after_a_stop_keeps_the_new_scheduler` pass.

Run: `cargo test -j4 -p rocket ipc_acl`
Expected: PASS, including `app_commands_file_matches_generate_handler` and `app_commands_file_is_sorted_and_unique`. If the first fails, the printed set difference names the command that is missing from `app-commands.txt` or from `generate_handler!`. If the second fails, the 14 names were inserted out of order.

- [ ] **Step 6: Check the whole app**

Run: `cargo check -j4 -p rocket`
Expected: Finished with no errors. A missing-import error on `rocket_infra::network::...` means a Plan 02 or 03 name differs from the index; fix the path in `lib.rs`, not the service.

Run: `cargo check -j4 --workspace`
Expected: Finished, no errors.

Run: `cargo test -j4 -p rocket-app network` and `cargo test -j4 -p rocket-audit`
Expected: PASS.

Optional manual check (needs a desktop and a keychain): `bun run tauri dev`, then look for the log line `network_listener_failed` (should be absent) and run `ss -ltn | grep 4782` to see the listener on port 47821.

- [ ] **Step 7: Update the crate guide**

In `crates/rocket-app/CLAUDE.md`, add rows to the Public Types table, in the table's own format, for `NetworkService`, `NetworkHandle` and `NetworkDeps`. Add one line to its notes: the Team Network service lives in `src/network/` inside `rocket-app` on purpose (ruling D-02 in `docs/engineering/ddd-decisions.md`: one `rocket-app`, split by context only later with its own spec). It holds `Arc<dyn Trait>` ports from `rocket-network` and spawns tokio tasks like `openapi_sync_scheduler.rs`; it never imports `rocket-infra`.

No commit; reviewed at the plan end.

---

End of Plan 4. Verify:

```bash
cargo test -j4 -p rocket-audit
cargo test -j4 -p rocket-app network
cargo test -j4 -p rocket network
cargo test -j4 -p rocket slot
cargo test -j4 -p rocket ipc_acl
cargo check -j4 --workspace
```

All pass. `git status` shows only the files in the File Structure table plus `crates/rocket-app/CLAUDE.md` (Step 7). Then stop and show the user the whole plan's change; commit only after their OK, through `dev-workflow-skills:1-git-commit`, staging explicit paths and splitting into `feat(audit): ...`, `feat(network): add NetworkService ...` and `feat(network): add network commands ...`.

Next: `docs/superpowers/plans/team-network/2026-11-06-team-network-plan-04b-end-to-end.md` (end-to-end tests over the real transport), then `2026-11-06-team-network-plan-05-frontend.md`. Plan 05 builds on the 14 commands, the DTO shapes, the `network-changed` and `network-pairing` event channels, and the user texts listed in this plan: "Pairing failed. Try again.", "The request expired.", "The other device is busy. Try again in a minute.", "That device is no longer nearby.", "Networking is off." and the 200-device limit message.

## Self-Review

**Spec coverage**

| Spec item | Where |
|---|---|
| 5.3 service owns lifecycle: start at launch, stop on exit | Task 1 `start`, `NetworkHandle`; Task 3 `start_network`, `NetworkSlot` (an alias of the shared `Slot`), `stop_for_good` on the exit paths |
| 5.4 commands and ACL | Task 3 (14 commands, `app-commands.txt`, `generate_handler!`) |
| 6.2 messages, 6.3 states, 6.4 commitment | Task 2 `initiate`, handler `pair_request`, `pair_nonce`, `pair_decision`, `pair_status`; tests `the_notice_appears_only_after_the_nonce_and_shows_the_right_code`, `a_responder_that_lies_about_its_nonce_fails_the_pairing` |
| 6.5 removing a pairing, offline peer gets `not-paired` | Task 2 `remove_paired`, `remove_local`, `NotPaired` branch, tests for both |
| 7.1 verify `/hello` before listing, merge by id, filter own id, goodbye | Task 1 `verify_hello`, `on_seen`, `on_gone`, tests |
| 7.2 manual addresses, 60 s probe | Task 1 `probe_manual`, `manual_probe_loop`, tests |
| 7.3 presence, who dials, pings, backoff, duplicates | Task 2 `run_device`, `run_socket`, tests |
| 7.4 hide my node | Task 1 `set_hidden`; Task 2 hidden tests |
| 8.3 limits: pairing 5 per minute, one session per device, decline cooldown, block | Task 2 `pair_request` and tests |
| 8.5 audit of pairing, removal, block | Task 1 audit kinds; Task 2 `complete_pairing`, `remove_local`, `remove_paired`, `block_peer` |
| 9 errors: keychain, port taken, mDNS off, failed, expired, declined/blocked look alike, version, damaged files, 200 limit | Task 1 status tests; Task 2 pairing tests |
| 10 `rocket-app` tests with fakes: start, stop, hidden, keychain failure | Task 1 and Task 2 tests |

**Deviations from the index (decided while planning)**

1. The module is `crates/rocket-app/src/network/` (a folder with `mod.rs`), not one `network_service.rs`, because the service is large. Public names are unchanged and re-exported from `rocket_app`.
2. `PairingDecision` and `PairingStarted` derive `Debug`, `Clone`, `PartialEq`, `Eq` (the index lists only the fields).
3. `NetworkHandle::stop` also stops the server and discovery, not only the tasks, because the handle owns those two `Arc`s.
4. `NodesChanged` is also published after a saved pairing, an unpair, a block, an unblock and every finished session, so the panel refetches (Plan 05 relies on it).
5. A "not paired" answer to a dial within 30 s of saving a pairing is not a removal (`NOT_PAIRED_GRACE`), because the responder saves before the initiator's poll does.
6. Every user-visible error is `DomainError::InvalidInput` (Plan 05 strips that prefix). Genuine failures end the session through `PairingSession::fail()` (Plan 01) and publish `failed`: ten poll failures in a row, a decision call that does not reach the peer, a commitment mismatch and a failed save. Only a user's Cancel publishes `cancelled`. Tests: `a_decision_that_cannot_be_sent_ends_the_pairing_as_failed`, `a_responder_that_stops_answering_while_polled_ends_the_pairing_as_failed`. Failures before the session exists locally (hello, `pair_request` or `pair_nonce` unreachable) publish nothing and only return "Pairing failed. Try again." from the command.
7. `start_network` uses `tauri::async_runtime::spawn` instead of the `enter()` guard, because `NetworkService::start` is async.
8. Block on a paired device also unpairs it and closes its connection (the user decided this on 2026-11-06). `block_peer` is async and shares `unpair_by_user` with `remove_paired`. `run_socket` refuses every frame from a blocked id.
9. Plan 04b (end-to-end tests over the real transport) runs after this plan. It adds only test code and the small `test-support` seams named in Plan 04b.
10. Reuse over copies (reuse review 2026-11-06): `sleep_or_stop` moves to `rocket-app/src/stop_signal.rs` and the OpenAPI sync scheduler imports it, with `run_every` for the two plain periodic loops (`sweep_loop`, `manual_probe_loop`; `poll_initiator` keeps its own loop because it carries a failure counter and returns from several places). `NetworkSlot` and `SyncSchedulerSlot` become aliases of one generic `Slot<H>` in `src-tauri/src/commands/slot.rs`, with `stop_for_good()` as the opt-in latch the sync scheduler must not get. The pairing-request window is `rocket_network::SlidingWindow` (Plan 01), not a hand-written `VecDeque` loop. `RecordingEvents` stays concrete (its helpers are `NetworkEvent`-specific). `FakeDiscovery::emit` is now `play`, so it is not confused with `NetworkService::emit`. `accept_both` and `RecordingEvents::first_incoming` in `test_support` are shared with Plan 04b.

**Type and name consistency**

- `NetworkDeps`, `NetworkService`, `NetworkHandle`, `NetworkState`, `NetworkStatus`, `NodeGroup`, `NodeStatus`, `NodeView`, `PairingDecision`, `PairingStarted` match the index. The command table in Task 3 matches the index table row by row.
- `SessionEntry` and `ConnEntry` are defined in Task 1 (interim) and finished in Task 2 with the same field names; `State` in `service.rs` does not change between tasks.
- Constants come from `rocket_network::limits` only; no duplicates in this plan except `POLL_INTERVAL`, `SWEEP_INTERVAL`, `CALL_TIMEOUT`, `HELLO_TIMEOUT`, `DIAL_TIMEOUT`, `NOT_PAIRED_GRACE`, `PAIR_REQUEST_WINDOW`, which are service-internal timeouts.

**Placeholders:** none. The interim `pairing.rs`, `handler.rs` and `connections.rs` in Task 1 are complete compiling files that Task 2 replaces in full; they are not stubs left in the final tree.
