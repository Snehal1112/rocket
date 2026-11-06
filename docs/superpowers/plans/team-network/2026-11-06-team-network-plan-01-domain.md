# Team Network Plan 1: Domain Crate `rocket-network`

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 1 of 5.** Previous plan: none (this is the first). Index: `docs/superpowers/plans/team-network/00-plan-index.md`.
**Next plan to run: `docs/superpowers/plans/team-network/2026-11-06-team-network-plan-02-identity-storage-tls.md`**
**Recommended model: Sonnet.**

**Goal:** A new pure domain crate `crates/rocket-network` holds every type, rule and port the Team Network MVP needs: device ids, node profile, persistence records, limits, the pairing commitment and code functions, the pairing state machine, presence and backoff rules, the dialer rule, the peer wire types, `PeerMessage`, `NetworkEvent` and all ports. It has no I/O.

**Architecture:** `rocket-network` depends only on `rocket-shared` (for `DomainError`, `DomainResult` and `PublishedEvent`) and small pure crates (`sha2`, `base64`, `data-encoding`, `rand`, `zeroize`). `PairingSession` is a plain state machine that takes `Instant` values as arguments, so tests need no clock. Ports are traits (`DeviceIdentityStore`, the four repositories, `Discovery`, `PeerHandler`, `PeerConnector`, `PeerServerControl`, `PeerTransportFactory`) that Plans 2 to 4 implement or consume. Nothing in this plan touches `rocket-infra`, `rocket-app` or `src-tauri`.

**Tech Stack:** Rust 2021, serde, serde_json, sha2, base64 0.22, data-encoding 2, rand 0.8, zeroize 1.8, chrono, uuid, async-trait, tokio (mpsc types only).

**Spec:** `docs/superpowers/specs/2026-11-05-team-network-discovery-pairing-design.md`, sections 5.1, 6, 7.3, 8.3, 10 (see Self-Review for the mapping).

## Global Constraints

- Always pass `-j4` to cargo. Never run `cargo test --workspace` or `--all`. Use `cargo test -j4 -p rocket-network <filter>`.
- No unwrap or expect calls in production code. Tests use `expect("reason")`. A repo hook rejects any Write or Edit whose Rust text contains a bare unwrap call, tests included.
- Mutex locks use `.lock().unwrap_or_else(|e| e.into_inner())` (only the test fakes in Task 3 lock a mutex).
- Persistence structs (`PairedDevice`, `BlockedDevice`, `ManualAddress`, `NetworkSettings`) keep snake_case serde. No camelCase rename in this crate. Peer wire types (`wire.rs`, `messages.rs`) are snake_case JSON, defined once here.
- `NetworkEvent`: tag camelCase, variant fields snake_case. `wire_contract_*` tests pin the keys, and a reserved-key test keeps `seq`, `tsMs`, `ts_ms`, `workspaceId` free for the bus envelope.
- Code comments are short full sentences ending with a period.
- Domain crate rule: logic and traits only. No file, network or keychain access.
- Persistence stays backward compatible: every new optional field has `#[serde(default)]`.
- No commits during this plan. At the end, stop and show the user the whole change. After the user's OK, commit through `dev-workflow-skills:1-git-commit`, split into logical commits (scaffold and records, pairing, presence and ports), staging explicit paths only (never `git add -A` or `.`). Subjects use `feat(network): ...` and `test(network): ...`.
- Line numbers drift. Anchor every edit on quoted text, not on a number.
- This plan does not write `.yml` files and does not touch collection, environment or auth code, so the OpenCollection spec read is not needed here. Plan 02 Task 2 needs it.

## Review Focus

1. **Pairing code leading zeros** (Task 2, `pairing_code_pads_leading_zeros`): inputs that give a code below 10 000 must still show six digits, for example `005 263`. A `PairingCode(4821)` shows `004821`.
2. **Decision after expiry** (Task 2, `decision_after_expiry_is_conflict`): once `expire_if_due` has moved the session to `Expired`, an accept must return `Err(Conflict)` and must not flip the state to `Paired`.
3. **Oversize frame** (Task 3, `decode_rejects_oversize_frame_and_accepts_the_limit`): a text of `MAX_FRAME_BYTES + 1` bytes is refused before any JSON parsing. A text of exactly `MAX_FRAME_BYTES` bytes is accepted.
4. **Unknown PeerMessage type** (Task 3, `decode_rejects_unknown_type_without_echoing_it`): `{"type":"send_collection"}` is `InvalidInput`, and the error text never echoes frame content.
5. **Base32 parse of wrong length** (Task 1, `parse_base32_rejects_bad_input`): 51 chars, 53 chars, lowercase, characters outside the alphabet and a non-canonical last character are all `InvalidInput`.
6. **Wrong revealed nonce** (Task 2, `wrong_revealed_nonce_fails_the_session`): the initiator session ends `Failed`, shows no code, and a later correct reveal is `Err(Conflict)`.
7. **Forged profile on the wire** (Task 3, `decode_normalizes_hello_profile`): a peer that sends its own `initials` or a name with control characters must not reach the UI as sent. `decode` rebuilds initials and rejects bad names.

## File Structure

| File | Change |
|---|---|
| `Cargo.toml` (root) | Add `crates/rocket-network` to `members` and a path entry to `[workspace.dependencies]` |
| `crates/rocket-network/Cargo.toml` | **Create** |
| `crates/rocket-network/CLAUDE.md` | **Create**, rocket-history style |
| `CLAUDE.md` (root) | Add a `rocket-network` row to the crate table |
| `crates/rocket-network/src/lib.rs` | **Create**, modules and re-exports (rewritten in each task) |
| `crates/rocket-network/src/limits.rs` | **Create**, constants |
| `crates/rocket-network/src/device.rs` | **Create**, `DeviceId` |
| `crates/rocket-network/src/profile.rs` | **Create**, `NodeProfile`, `initials_for`, `default_display_name` |
| `crates/rocket-network/src/identity.rs` | **Create**, `PeerIdentity`, `DeviceKeyMaterial` |
| `crates/rocket-network/src/records.rs` | **Create**, persistence records and `DiscoveredNode` |
| `crates/rocket-network/src/pairing.rs` | **Create**, nonce, code functions, `PairingSession` |
| `crates/rocket-network/src/presence.rs` | **Create**, `Presence`, `PresenceTracker`, `backoff` |
| `crates/rocket-network/src/dialer.rs` | **Create**, `DialHint`, `should_dial`, `keep_dialed_by` |
| `crates/rocket-network/src/rate.rs` | **Create**, `SlidingWindow` (request limit, pure) |
| `crates/rocket-network/src/wire.rs` | **Create**, REST body types |
| `crates/rocket-network/src/messages.rs` | **Create**, `PeerMessage` |
| `crates/rocket-network/src/events.rs` | **Create**, `NetworkEvent` |
| `crates/rocket-network/src/ports.rs` | **Create**, every trait and port type |

## Dependency decisions

- Workspace deps (used as `x.workspace = true`): `rocket-shared`, `serde`, `serde_json`, `sha2` (0.10), `base64` (0.22), `uuid` (1, v4), `chrono` (serde), `async-trait`, `tokio`.
- New per-crate deps: `data-encoding = "2.11"` (Cargo.lock already has 2.11.0 transitively), `rand = "0.8"` (same version `rocket-http`, `rocket-environment` and `rocket-infra` use; lock has 0.8.5), `zeroize = "1.8"` (lock has 1.8.2).
- API facts checked in `~/.cargo/registry/src`: `data_encoding::BASE32_NOPAD.encode(&[u8]) -> String` and `.decode(&[u8]) -> Result<Vec<u8>, DecodeError>`; base64 0.22 needs `use base64::{engine::general_purpose::STANDARD, Engine as _}`; `rand 0.8` has `rand::rngs::OsRng` and `rand::RngCore::fill_bytes`.
- `thiserror` is not needed: errors are `DomainError`.

---

### Task 1: Crate scaffold, limits, device id, profile, identity, records

**Files:**
- Modify: `Cargo.toml` (root)
- Modify: `CLAUDE.md` (root)
- Create: `crates/rocket-network/Cargo.toml`
- Create: `crates/rocket-network/CLAUDE.md`
- Create: `crates/rocket-network/src/lib.rs`
- Create: `crates/rocket-network/src/limits.rs`
- Create: `crates/rocket-network/src/device.rs`
- Create: `crates/rocket-network/src/profile.rs`
- Create: `crates/rocket-network/src/identity.rs`
- Create: `crates/rocket-network/src/records.rs`

**Interfaces:**
- Consumes: `rocket_shared::error::{DomainError, DomainResult}`.
- Produces (used by every later task and plan):

```rust
// limits.rs: every constant from the plan index, unchanged.
pub const PROTOCOL_VERSION: u32 = 1;
pub const MDNS_SERVICE_TYPE: &str = "_rocket._tcp.local.";
pub const DEFAULT_PORT: u16 = 47821;
pub const PORT_RANGE_END: u16 = 47830;
pub const MAX_PAIRED_DEVICES: usize = 200;
pub const MAX_MANUAL_ADDRESSES: usize = 50;
pub const MAX_CONNECTIONS: usize = 256;
pub const MAX_UNPAIRED_CONNECTIONS: usize = 32;
pub const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(10);
pub const MAX_REST_BODY_BYTES: usize = 16 * 1024;
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
pub const HELLO_PER_IP_PER_MINUTE: u32 = 30;
pub const PAIR_REQUESTS_PER_MINUTE: u32 = 5;
pub const PAIRING_TTL: Duration = Duration::from_secs(120);
pub const DECLINE_COOLDOWN: Duration = Duration::from_secs(60);
pub const PING_INTERVAL: Duration = Duration::from_secs(20);
pub const MISSED_PINGS_OFFLINE: u32 = 2;
pub const BACKOFF_MAX: Duration = Duration::from_secs(120);
pub const DIAL_GRACE: Duration = Duration::from_secs(10);
pub const MANUAL_PROBE_INTERVAL: Duration = Duration::from_secs(60);
pub const MDNS_RECORD_TTL: Duration = Duration::from_secs(120);
pub const DISPLAY_NAME_MAX_CHARS: usize = 40;

// device.rs
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceId([u8; 32]);
impl DeviceId {
    pub fn from_spki(spki_der: &[u8]) -> DeviceId;
    pub fn from_bytes(bytes: [u8; 32]) -> DeviceId;
    pub fn as_bytes(&self) -> &[u8; 32];
    pub fn to_base32(&self) -> String;
    pub fn parse_base32(s: &str) -> DomainResult<DeviceId>;
    pub fn short(&self) -> String;
}
// Serialize/Deserialize as the base32 string. Debug and Display print short().

// profile.rs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeProfile { pub display_name: String, pub initials: String }
impl NodeProfile {
    pub fn new(display_name: &str) -> DomainResult<NodeProfile>;
    pub fn validated(&self) -> DomainResult<NodeProfile>; // addition: for profiles that came from a peer
}
pub fn initials_for(display_name: &str) -> String;
pub fn default_display_name(user: Option<&str>) -> String;

// identity.rs
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerIdentity { pub device_id: DeviceId, pub spki_der: Vec<u8> }
impl PeerIdentity { pub fn from_spki(spki_der: Vec<u8>) -> PeerIdentity; }
pub struct DeviceKeyMaterial { pub pkcs8_der: Vec<u8> } // no Debug, no Clone; Drop zeroes the bytes

// records.rs
pub struct PairedDevice { pub device_id: DeviceId, pub name_at_pairing: String, pub paired_at: DateTime<Utc>, pub last_addresses: Vec<SocketAddr> }
pub struct BlockedDevice { pub device_id: DeviceId, pub name: String, pub blocked_at: DateTime<Utc> }
pub struct ManualAddress { pub host: String, pub port: u16, pub added_at: DateTime<Utc> }
pub struct NetworkSettings { pub hidden: bool, pub display_name: Option<String>, pub intro_seen: bool }
pub enum DiscoverySource { Mdns, Manual }
pub struct DiscoveredNode { pub device_id: DeviceId, pub profile: NodeProfile, pub addresses: Vec<SocketAddr>, pub source: DiscoverySource, pub last_seen: DateTime<Utc>, pub protocol_version: u32 }
```

Task 1 adds one public helper beyond the index: `NodeProfile::validated`. Plan 03 and Plan 04 call it on every profile that arrives from a peer, so a peer cannot choose its own initials or send control characters.

- [ ] **Step 1: Register the crate in the workspace**

In the root `Cargo.toml`, replace

```toml
    "crates/rocket-grpc",
    "src-tauri",
]
```

with

```toml
    "crates/rocket-grpc",
    "crates/rocket-network",
    "src-tauri",
]
```

Then in `[workspace.dependencies]`, below the line `rocket-grpc = { path = "crates/rocket-grpc" }`, add:

```toml
rocket-network = { path = "crates/rocket-network" }
```

- [ ] **Step 2: Create the crate manifest**

Create `crates/rocket-network/Cargo.toml`:

```toml
[package]
name = "rocket-network"
version.workspace = true
edition.workspace = true

[dependencies]
rocket-shared.workspace = true
serde.workspace = true
serde_json.workspace = true
sha2.workspace = true
base64.workspace = true
uuid.workspace = true
chrono.workspace = true
async-trait.workspace = true
tokio = { workspace = true, features = ["sync"] }
data-encoding = "2.11"
rand = "0.8"
zeroize = "1.8"
```

- [ ] **Step 3: Create the crate guide**

Create `crates/rocket-network/CLAUDE.md`:

````markdown
# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# rocket-network

Domain crate for the Team Network: device identity, node profile, pairing, presence, the dialer rule, the peer wire types and the ports that infra implements. It defines types, pure functions and traits only; no I/O lives here.

Spec: `docs/superpowers/specs/2026-11-05-team-network-discovery-pairing-design.md`. Plans: `docs/superpowers/plans/team-network/`.

## Commands

```bash
# Run all tests in this crate
cargo test -j4 -p rocket-network

# Run a single test
cargo test -j4 -p rocket-network pairing::tests::end_to_end_codes_match

# Fast validation
cargo check -j4 -p rocket-network
```

## Public Types

| Type | Description |
|---|---|
| `limits` | Every protocol constant: ports, limits, timeouts, TTLs. |
| `DeviceId` | SHA-256 of the device public key (SPKI DER). Serializes as base32. `short()` gives `XXXX-XXXX`. |
| `NodeProfile` | Display name and initials. `new` validates, `validated` re-checks a profile that came from a peer. |
| `PeerIdentity` | A device id plus the SPKI DER bytes seen in a TLS handshake. |
| `DeviceKeyMaterial` | The PKCS#8 private key bytes. No `Debug`, no `Clone`, zeroed on drop. |
| `PairedDevice`, `BlockedDevice`, `ManualAddress`, `NetworkSettings` | Persistence records. snake_case serde, `#[serde(default)]` on optional fields. |
| `DiscoveredNode`, `DiscoverySource` | A node seen through mDNS or a manual address. |
| `Nonce`, `PairingId`, `PairingCode` | Pairing values. `PairingCode::digits()` is always six digits. |
| `commitment`, `pairing_code` | The two pairing hashes, with length-prefixed inputs. |
| `PairingSession` | Pairing state machine for both roles. Time is passed in as `Instant`. |
| `Presence`, `PresenceTracker`, `backoff` | Online state from ping bookkeeping, and the reconnect delay. |
| `DialHint`, `should_dial`, `keep_dialed_by` | Which side opens the WebSocket. |
| `SlidingWindow` | Request limit: at most `cap` hits in a `window`. Time is passed in as `Instant`. |
| `HelloResponse`, `PairRequest`, `PairRequestResponse`, `NonceMessage`, `DecisionRequest`, `PairStatusResponse` | REST bodies, snake_case JSON. |
| `PeerMessage` | WebSocket frame enum. `decode` enforces the 64 KiB limit and rejects unknown types. |
| `NetworkEvent` | Events for the frontend. Implements `PublishedEvent`. Channels `network-changed` and `network-pairing`. |
| Ports | `DeviceIdentityStore`, `PairedDeviceRepository`, `BlockedDeviceRepository`, `ManualAddressRepository`, `NetworkSettingsRepository`, `QuarantineReport`, `Discovery`, `DiscoverySink`, `PeerHandler`, `PeerConnector`, `PeerServerControl`, `PeerTransportFactory`, plus `PeerSocket`, `PeerTransport`, `PeerError`, `PeerCallError`. |

## Key Patterns

- All repository traits are synchronous and `Send + Sync`. The peer traits are async (`async_trait`).
- All fallible methods return `DomainResult<T>` from `rocket-shared`.
- Persistence structs keep snake_case serde. Peer wire types are snake_case JSON. Only IPC DTOs in `src-tauri` use camelCase.
- `NetworkEvent` tag is camelCase and variant fields are snake_case on purpose. `wire_contract_*` tests pin the keys.
- Pairing logic never reads the clock. Callers pass `Instant` to `PairingSession::new_*` and `expire_if_due`.
- Never log nonces, codes, keys or certificates. `Nonce` and `DeviceKeyMaterial` have no `Debug` on purpose.

## Workspace Relationships

- Depends on `rocket-shared` and small pure crates (`serde`, `sha2`, `base64`, `data-encoding`, `rand`, `zeroize`, `uuid`, `chrono`, `async-trait`). It uses `tokio::sync::mpsc` channel types only, for `PeerSocket`, declared with the `sync` feature like `rocket-http` does. It never starts a runtime, opens a socket or touches the filesystem. Do not add `rustls`, `axum`, `hyper`, `mdns-sd`, `keyring` or `tauri` here.
- Concrete implementations of the ports live in `rocket-infra` (`src/network/`).
- `rocket-app` holds the ports as `Arc<dyn Trait>` in `NetworkService`.
- The Tauri shell in `src-tauri` wires infra to the service at startup.
````

- [ ] **Step 4: Add the crate row to the root `CLAUDE.md`**

In the root `CLAUDE.md`, find the crate table row

```
| `rocket-git` | `GitService` trait + `Git2Service` (libgit2) |
```

and add this row directly after it:

```
| `rocket-network` | Team network domain: device id, pairing, presence, peer messages, ports (pure, no I/O) |
```

- [ ] **Step 5: Create `lib.rs` for Task 1 and test-only module files**

Create `crates/rocket-network/src/lib.rs`:

```rust
pub mod device;
pub mod identity;
pub mod limits;
pub mod profile;
pub mod records;

pub use device::DeviceId;
pub use identity::{DeviceKeyMaterial, PeerIdentity};
pub use profile::{default_display_name, initials_for, NodeProfile};
pub use records::{
    BlockedDevice, DiscoveredNode, DiscoverySource, ManualAddress, NetworkSettings, PairedDevice,
};
```

Create `crates/rocket-network/src/limits.rs` with the tests only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn spec_values_are_pinned() {
        assert_eq!(PROTOCOL_VERSION, 1);
        assert_eq!(MDNS_SERVICE_TYPE, "_rocket._tcp.local.");
        assert_eq!(MAX_PAIRED_DEVICES, 200);
        assert_eq!(MAX_MANUAL_ADDRESSES, 50);
        assert_eq!(MAX_CONNECTIONS, 256);
        assert_eq!(MAX_UNPAIRED_CONNECTIONS, 32);
        assert_eq!(MAX_REST_BODY_BYTES, 16 * 1024);
        assert_eq!(MAX_FRAME_BYTES, 64 * 1024);
        assert_eq!(PAIR_REQUESTS_PER_MINUTE, 5);
        assert_eq!(HELLO_PER_IP_PER_MINUTE, 30);
        assert_eq!(PAIRING_TTL, Duration::from_secs(120));
        assert_eq!(DECLINE_COOLDOWN, Duration::from_secs(60));
        assert_eq!(PING_INTERVAL, Duration::from_secs(20));
        assert_eq!(MISSED_PINGS_OFFLINE, 2);
        assert_eq!(BACKOFF_MAX, Duration::from_secs(120));
        assert_eq!(DIAL_GRACE, Duration::from_secs(10));
        assert_eq!(MDNS_RECORD_TTL, Duration::from_secs(120));
        assert_eq!(DISPLAY_NAME_MAX_CHARS, 40);
    }

    #[test]
    fn port_range_holds_ten_ports() {
        let (start, end) = (DEFAULT_PORT, PORT_RANGE_END);
        assert_eq!(start, 47821);
        assert_eq!(end - start + 1, 10);
    }

    #[test]
    fn unpaired_connections_stay_below_the_total() {
        let (unpaired, total) = (MAX_UNPAIRED_CONNECTIONS, MAX_CONNECTIONS);
        assert!(unpaired < total);
        let (rest, frame) = (MAX_REST_BODY_BYTES, MAX_FRAME_BYTES);
        assert!(rest < frame);
    }
}
```

Create `crates/rocket-network/src/device.rs` with the tests only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::error::DomainError;

    fn spki() -> Vec<u8> {
        vec![0xA1; 44]
    }

    #[test]
    fn from_spki_is_sha256_in_base32() {
        let id = DeviceId::from_spki(&spki());
        // SHA-256 of 44 bytes of 0xA1, computed outside Rust.
        assert_eq!(
            id.to_base32(),
            "PYE7HWE72X5NAUV5ZHYJISTMRAGMVJ5IYY2BHXJW4C2F5WJ5NCMQ"
        );
        assert_eq!(id.to_base32().len(), 52);
    }

    #[test]
    fn short_is_two_groups_of_four() {
        let id = DeviceId::from_spki(&spki());
        assert_eq!(id.short(), "PYE7-HWE7");
    }

    #[test]
    fn debug_and_display_print_the_short_form_only() {
        let id = DeviceId::from_spki(&spki());
        assert_eq!(format!("{id}"), "PYE7-HWE7");
        assert_eq!(format!("{id:?}"), "PYE7-HWE7");
    }

    #[test]
    fn base32_round_trips() {
        let id = DeviceId::from_bytes([7; 32]);
        let parsed = DeviceId::parse_base32(&id.to_base32()).expect("round trip");
        assert_eq!(parsed, id);
        assert_eq!(parsed.as_bytes(), &[7; 32]);
    }

    #[test]
    fn parse_base32_rejects_bad_input() {
        let good = DeviceId::from_spki(&spki()).to_base32();
        let too_short = &good[..51];
        let too_long = format!("{good}A");
        let lowercase = good.to_lowercase();
        let bad_char = format!("{}1", &good[..51]);
        // The last character carries one data bit, so only A and Q are canonical.
        let non_canonical = format!("{}B", "A".repeat(51));
        let multibyte = "é".repeat(26);
        for bad in [
            "",
            too_short,
            too_long.as_str(),
            lowercase.as_str(),
            bad_char.as_str(),
            non_canonical.as_str(),
            multibyte.as_str(),
        ] {
            let err = DeviceId::parse_base32(bad).expect_err("rejected");
            assert!(matches!(err, DomainError::InvalidInput(_)), "input {bad:?}");
        }
    }

    #[test]
    fn canonical_all_zero_id_parses() {
        let id = DeviceId::parse_base32(&"A".repeat(52)).expect("all zero is canonical");
        assert_eq!(id.as_bytes(), &[0; 32]);
    }

    #[test]
    fn serde_uses_the_base32_string() {
        let id = DeviceId::from_bytes([3; 32]);
        let json = serde_json::to_string(&id).expect("serializes");
        assert_eq!(json, format!("\"{}\"", id.to_base32()));
        let back: DeviceId = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(back, id);
    }

    #[test]
    fn serde_rejects_a_bad_string() {
        assert!(serde_json::from_str::<DeviceId>("\"nope\"").is_err());
        assert!(serde_json::from_str::<DeviceId>("42").is_err());
    }

    #[test]
    fn ids_order_by_bytes() {
        let low = DeviceId::from_bytes([1; 32]);
        let high = DeviceId::from_bytes([2; 32]);
        assert!(low < high);
        assert_eq!(std::cmp::min(low, high), low);
    }
}
```

Create `crates/rocket-network/src/profile.rs` with the tests only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::error::DomainError;

    #[test]
    fn new_trims_and_builds_initials() {
        let profile = NodeProfile::new("  Ada Lovelace  ").expect("valid");
        assert_eq!(profile.display_name, "Ada Lovelace");
        assert_eq!(profile.initials, "AL");
    }

    #[test]
    fn new_rejects_empty_long_and_control_names() {
        for bad in ["", "   ", "Ada\u{0}", "Ada\nLovelace", "tab\there"] {
            let err = NodeProfile::new(bad).expect_err("rejected");
            assert!(matches!(err, DomainError::InvalidInput(_)), "input {bad:?}");
        }
        let too_long = "a".repeat(DISPLAY_NAME_MAX_CHARS + 1);
        assert!(NodeProfile::new(&too_long).is_err());
        let at_limit = "a".repeat(DISPLAY_NAME_MAX_CHARS);
        assert!(NodeProfile::new(&at_limit).is_ok());
    }

    #[test]
    fn length_counts_characters_not_bytes() {
        let name = "é".repeat(DISPLAY_NAME_MAX_CHARS);
        assert!(NodeProfile::new(&name).is_ok());
    }

    #[test]
    fn initials_use_the_first_two_words() {
        assert_eq!(initials_for("ada lovelace"), "AL");
        assert_eq!(initials_for("Ada"), "A");
        assert_eq!(initials_for("Ada Byron King"), "AB");
        assert_eq!(initials_for("   "), "");
        assert_eq!(initials_for("éva  östen"), "ÉÖ");
    }

    #[test]
    fn default_name_uses_the_os_user_or_falls_back() {
        assert_eq!(default_display_name(Some(" sneha ")), "sneha");
        assert_eq!(default_display_name(None), "Rocket user");
        assert_eq!(default_display_name(Some("")), "Rocket user");
        assert_eq!(default_display_name(Some("  ")), "Rocket user");
        assert_eq!(default_display_name(Some("bad\u{7}name")), "Rocket user");
    }

    #[test]
    fn default_name_is_cut_to_the_limit() {
        let long = "b".repeat(60);
        let name = default_display_name(Some(&long));
        assert_eq!(name.chars().count(), DISPLAY_NAME_MAX_CHARS);
        assert!(NodeProfile::new(&name).is_ok());
    }

    #[test]
    fn validated_rebuilds_initials_and_rejects_bad_names() {
        let forged = NodeProfile {
            display_name: "Ada Lovelace".to_string(),
            initials: "ZZZZ".to_string(),
        };
        let clean = forged.validated().expect("valid name");
        assert_eq!(clean.initials, "AL");
        let bad = NodeProfile {
            display_name: "x\u{1b}[31m".to_string(),
            initials: "X".to_string(),
        };
        assert!(bad.validated().is_err());
    }

    #[test]
    fn profile_json_is_snake_case() {
        let profile = NodeProfile::new("Ada Lovelace").expect("valid");
        let value = serde_json::to_value(&profile).expect("serializes");
        assert_eq!(
            value,
            serde_json::json!({"display_name": "Ada Lovelace", "initials": "AL"})
        );
    }
}
```

Create `crates/rocket-network/src/identity.rs` with the tests only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_identity_derives_the_device_id_from_the_spki() {
        let spki = vec![0xA1; 44];
        let peer = PeerIdentity::from_spki(spki.clone());
        assert_eq!(peer.device_id, DeviceId::from_spki(&spki));
        assert_eq!(peer.spki_der, spki);
    }

    #[test]
    fn same_spki_gives_equal_identities() {
        let a = PeerIdentity::from_spki(vec![1; 44]);
        let b = PeerIdentity::from_spki(vec![1; 44]);
        let c = PeerIdentity::from_spki(vec![2; 44]);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn wipe_zeroes_every_byte_and_keeps_the_length() {
        let mut key = DeviceKeyMaterial {
            pkcs8_der: vec![0x5A; 48],
        };
        key.wipe();
        assert_eq!(key.pkcs8_der.len(), 48);
        assert!(key.pkcs8_der.iter().all(|b| *b == 0));
    }
}
```

Create `crates/rocket-network/src/records.rs` with the tests only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    fn when() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 6, 10, 30, 0)
            .single()
            .expect("valid date")
    }

    #[test]
    fn paired_device_json_is_snake_case_and_round_trips() {
        let device = PairedDevice {
            device_id: DeviceId::from_bytes([4; 32]),
            name_at_pairing: "Ada".to_string(),
            paired_at: when(),
            last_addresses: vec!["192.168.1.20:47821".parse().expect("addr")],
        };
        let value = serde_json::to_value(&device).expect("serializes");
        let object = value.as_object().expect("object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["device_id", "last_addresses", "name_at_pairing", "paired_at"]
        );
        assert_eq!(object["last_addresses"], json!(["192.168.1.20:47821"]));
        let back: PairedDevice = serde_json::from_value(value).expect("deserializes");
        assert_eq!(back, device);
    }

    #[test]
    fn paired_device_without_addresses_still_loads() {
        let id = DeviceId::from_bytes([4; 32]).to_base32();
        let text = format!(
            r#"{{"device_id":"{id}","name_at_pairing":"Ada","paired_at":"2026-11-06T10:30:00Z"}}"#
        );
        let device: PairedDevice = serde_json::from_str(&text).expect("old file loads");
        assert!(device.last_addresses.is_empty());
    }

    #[test]
    fn blocked_device_and_manual_address_round_trip() {
        let blocked = BlockedDevice {
            device_id: DeviceId::from_bytes([5; 32]),
            name: "Mallory".to_string(),
            blocked_at: when(),
        };
        let text = serde_json::to_string(&blocked).expect("serializes");
        let back: BlockedDevice = serde_json::from_str(&text).expect("deserializes");
        assert_eq!(back, blocked);

        let address = ManualAddress {
            host: "laptop.tail1234.ts.net".to_string(),
            port: 47821,
            added_at: when(),
        };
        let text = serde_json::to_string(&address).expect("serializes");
        let back: ManualAddress = serde_json::from_str(&text).expect("deserializes");
        assert_eq!(back, address);
    }

    #[test]
    fn settings_default_to_visible_with_no_name() {
        let settings = NetworkSettings::default();
        assert!(!settings.hidden);
        assert_eq!(settings.display_name, None);
        assert!(!settings.intro_seen);
    }

    #[test]
    fn settings_fill_missing_fields_with_defaults() {
        let settings: NetworkSettings = serde_json::from_str("{}").expect("empty loads");
        assert_eq!(settings, NetworkSettings::default());
        let settings: NetworkSettings =
            serde_json::from_str(r#"{"hidden":true}"#).expect("partial loads");
        assert!(settings.hidden);
        assert_eq!(settings.display_name, None);
    }

    #[test]
    fn discovery_source_names_are_stable() {
        assert_eq!(
            serde_json::to_string(&DiscoverySource::Mdns).expect("serializes"),
            "\"Mdns\""
        );
        assert_eq!(
            serde_json::to_string(&DiscoverySource::Manual).expect("serializes"),
            "\"Manual\""
        );
    }

    #[test]
    fn discovered_node_holds_what_the_panel_needs() {
        let node = DiscoveredNode {
            device_id: DeviceId::from_bytes([6; 32]),
            profile: NodeProfile::new("Grace Hopper").expect("valid"),
            addresses: vec!["10.0.0.5:47822".parse().expect("addr")],
            source: DiscoverySource::Mdns,
            last_seen: when(),
            protocol_version: 1,
        };
        assert_eq!(node.clone(), node);
        assert_eq!(node.profile.initials, "GH");
    }
}
```

- [ ] **Step 6: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket-network`
Expected: FAIL to compile. Typical errors: `cannot find value PROTOCOL_VERSION in this scope`, `cannot find type DeviceId in this scope`, `cannot find struct NodeProfile`, `cannot find struct PeerIdentity`, `cannot find type PairedDevice`. (`lib.rs` also reports unresolved imports for the re-exports.)

- [ ] **Step 7: Implement `limits.rs`**

Add this above the `#[cfg(test)]` line in `crates/rocket-network/src/limits.rs`:

```rust
//! Protocol constants for the Team Network.

use std::time::Duration;

/// The peer protocol version this build speaks.
pub const PROTOCOL_VERSION: u32 = 1;
/// The mDNS service type.
pub const MDNS_SERVICE_TYPE: &str = "_rocket._tcp.local.";
/// The first port the peer server tries.
pub const DEFAULT_PORT: u16 = 47821;
/// The last port the peer server tries.
pub const PORT_RANGE_END: u16 = 47830;
/// The most paired devices one node keeps.
pub const MAX_PAIRED_DEVICES: usize = 200;
/// The most manual addresses one node keeps.
pub const MAX_MANUAL_ADDRESSES: usize = 50;
/// The most concurrent connections the peer server accepts.
pub const MAX_CONNECTIONS: usize = 256;
/// The most concurrent connections from unpaired keys.
pub const MAX_UNPAIRED_CONNECTIONS: usize = 32;
/// How long a client may take to send its request headers.
pub const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(10);
/// The largest REST request body.
pub const MAX_REST_BODY_BYTES: usize = 16 * 1024;
/// The largest WebSocket text frame.
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
/// `/hello` requests allowed per IP per minute.
pub const HELLO_PER_IP_PER_MINUTE: u32 = 30;
/// Pairing requests allowed per minute across all strangers.
pub const PAIR_REQUESTS_PER_MINUTE: u32 = 5;
/// How long a pairing session lives, counted from its creation.
pub const PAIRING_TTL: Duration = Duration::from_secs(120);
/// How long a declined device must wait before it may ask again.
pub const DECLINE_COOLDOWN: Duration = Duration::from_secs(60);
/// How often each side sends a ping.
pub const PING_INTERVAL: Duration = Duration::from_secs(20);
/// Unanswered pings that are tolerated before a peer counts as offline.
pub const MISSED_PINGS_OFFLINE: u32 = 2;
/// The longest reconnect delay.
pub const BACKOFF_MAX: Duration = Duration::from_secs(120);
/// How long the higher device id waits before it dials too.
pub const DIAL_GRACE: Duration = Duration::from_secs(10);
/// How often a manual address is probed.
pub const MANUAL_PROBE_INTERVAL: Duration = Duration::from_secs(60);
/// How long an mDNS record lives without a refresh.
pub const MDNS_RECORD_TTL: Duration = Duration::from_secs(120);
/// The longest display name, in characters.
pub const DISPLAY_NAME_MAX_CHARS: usize = 40;
```

- [ ] **Step 8: Implement `device.rs`**

Add this above the `#[cfg(test)]` line in `crates/rocket-network/src/device.rs`:

```rust
//! The device id: SHA-256 of the device public key.

use std::fmt;

use data_encoding::BASE32_NOPAD;
use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

/// Length of a base32 device id without padding.
const BASE32_LEN: usize = 52;

/// The only thing a node is known by.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceId([u8; 32]);

impl DeviceId {
    /// Hashes a SubjectPublicKeyInfo DER blob into a device id.
    pub fn from_spki(spki_der: &[u8]) -> DeviceId {
        DeviceId(Sha256::digest(spki_der).into())
    }

    pub fn from_bytes(bytes: [u8; 32]) -> DeviceId {
        DeviceId(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The full id as 52 uppercase base32 characters.
    pub fn to_base32(&self) -> String {
        BASE32_NOPAD.encode(&self.0)
    }

    /// Parses the canonical base32 form. Anything else is `InvalidInput`.
    pub fn parse_base32(s: &str) -> DomainResult<DeviceId> {
        let invalid = || DomainError::InvalidInput("invalid device id".to_string());
        if s.len() != BASE32_LEN {
            return Err(invalid());
        }
        let bytes = BASE32_NOPAD.decode(s.as_bytes()).map_err(|_| invalid())?;
        let array: [u8; 32] = bytes.try_into().map_err(|_| invalid())?;
        let id = DeviceId(array);
        // Re-encoding rejects non-canonical trailing bits.
        if id.to_base32() != s {
            return Err(invalid());
        }
        Ok(id)
    }

    /// The first 8 base32 characters as `XXXX-XXXX`, for display.
    pub fn short(&self) -> String {
        let full = self.to_base32();
        format!("{}-{}", &full[..4], &full[4..8])
    }
}

impl fmt::Debug for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.short())
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.short())
    }
}

impl Serialize for DeviceId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_base32())
    }
}

impl<'de> Deserialize<'de> for DeviceId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<DeviceId, D::Error> {
        let text = String::deserialize(deserializer)?;
        DeviceId::parse_base32(&text).map_err(serde::de::Error::custom)
    }
}
```

- [ ] **Step 9: Implement `profile.rs`**

Add this above the `#[cfg(test)]` line in `crates/rocket-network/src/profile.rs`:

```rust
//! What a stranger sees of a node: a display name and initials.

use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

use crate::limits::DISPLAY_NAME_MAX_CHARS;

const FALLBACK_NAME: &str = "Rocket user";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeProfile {
    pub display_name: String,
    pub initials: String,
}

impl NodeProfile {
    /// Trims the name, checks it and builds the initials.
    pub fn new(display_name: &str) -> DomainResult<NodeProfile> {
        let name = display_name.trim();
        let count = name.chars().count();
        if count == 0 || count > DISPLAY_NAME_MAX_CHARS {
            return Err(DomainError::InvalidInput(format!(
                "display name must be 1 to {DISPLAY_NAME_MAX_CHARS} characters"
            )));
        }
        if name.chars().any(char::is_control) {
            return Err(DomainError::InvalidInput(
                "display name has control characters".to_string(),
            ));
        }
        Ok(NodeProfile {
            display_name: name.to_string(),
            initials: initials_for(name),
        })
    }

    /// Re-checks a profile that came from a peer. The initials are rebuilt from
    /// the name, so a peer cannot choose them.
    pub fn validated(&self) -> DomainResult<NodeProfile> {
        NodeProfile::new(&self.display_name)
    }
}

/// The first character of the first two words, uppercased.
pub fn initials_for(display_name: &str) -> String {
    display_name
        .split_whitespace()
        .take(2)
        .filter_map(|word| word.chars().next())
        .flat_map(|c| c.to_uppercase())
        .collect()
}

/// The OS user name, or "Rocket user" when it is missing or unusable.
pub fn default_display_name(user: Option<&str>) -> String {
    let candidate = user.map(str::trim).unwrap_or("");
    if candidate.is_empty() || candidate.chars().any(char::is_control) {
        return FALLBACK_NAME.to_string();
    }
    let cut: String = candidate.chars().take(DISPLAY_NAME_MAX_CHARS).collect();
    cut.trim_end().to_string()
}
```

- [ ] **Step 10: Implement `identity.rs`**

Add this above the `#[cfg(test)]` line in `crates/rocket-network/src/identity.rs`:

```rust
//! A peer's key identity and the local private key bytes.

use zeroize::Zeroize;

use crate::device::DeviceId;

/// A device id plus the SPKI DER bytes the TLS handshake showed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerIdentity {
    pub device_id: DeviceId,
    pub spki_der: Vec<u8>,
}

impl PeerIdentity {
    pub fn from_spki(spki_der: Vec<u8>) -> PeerIdentity {
        PeerIdentity {
            device_id: DeviceId::from_spki(&spki_der),
            spki_der,
        }
    }
}

/// The PKCS#8 private key bytes. No `Debug` and no `Clone`, so the key cannot
/// be logged or copied by accident. The bytes are zeroed on drop.
pub struct DeviceKeyMaterial {
    pub pkcs8_der: Vec<u8>,
}

impl DeviceKeyMaterial {
    /// Overwrites every key byte with zero. The length stays the same.
    fn wipe(&mut self) {
        self.pkcs8_der.as_mut_slice().zeroize();
    }
}

impl Drop for DeviceKeyMaterial {
    fn drop(&mut self) {
        self.wipe();
    }
}
```

- [ ] **Step 11: Implement `records.rs`**

Add this above the `#[cfg(test)]` line in `crates/rocket-network/src/records.rs`:

```rust
//! Persistence records and the discovered-node view. Persistence is snake_case.

use std::net::SocketAddr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::device::DeviceId;
use crate::profile::NodeProfile;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedDevice {
    pub device_id: DeviceId,
    pub name_at_pairing: String,
    pub paired_at: DateTime<Utc>,
    #[serde(default)]
    pub last_addresses: Vec<SocketAddr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockedDevice {
    pub device_id: DeviceId,
    pub name: String,
    pub blocked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualAddress {
    pub host: String,
    pub port: u16,
    pub added_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NetworkSettings {
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub intro_seen: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscoverySource {
    Mdns,
    Manual,
}

/// A node seen on the network. Held in memory only, never persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredNode {
    pub device_id: DeviceId,
    pub profile: NodeProfile,
    pub addresses: Vec<SocketAddr>,
    pub source: DiscoverySource,
    pub last_seen: DateTime<Utc>,
    pub protocol_version: u32,
}
```

- [ ] **Step 12: Run the tests and see them pass**

Run: `cargo test -j4 -p rocket-network`
Expected: PASS. All tests in `limits`, `device`, `profile`, `identity` and `records` pass, 0 failed.

- [ ] **Step 13: Check and lint**

Run: `cargo check -j4 -p rocket-network`
Expected: no errors, no warnings.

Run: `cargo clippy -j4 -p rocket-network -- -D warnings`
Expected: no findings. (CI runs clippy report-only because of an old backlog elsewhere. If clippy reports a finding in another crate, ignore it. Fix anything in `rocket-network`.)

Run: `cargo check -j4 --workspace`
Expected: PASS. This proves the new member and the workspace dependency entry are valid.

- [ ] **Step 14: Stop**

No commit. Changes are reviewed at the plan end.

---

### Task 2: Pairing crypto and the `PairingSession` state machine

**Files:**
- Modify: `crates/rocket-network/src/lib.rs`
- Create: `crates/rocket-network/src/pairing.rs`

**Interfaces:**
- Consumes: `PeerIdentity`, `NodeProfile`, `PAIRING_TTL` (Task 1).
- Produces (used by Plan 03 wire code, Plan 04 `NetworkService`):

```rust
#[derive(Clone, PartialEq, Eq)] pub struct Nonce(pub [u8; 32]);
impl Nonce { pub fn random() -> Nonce; pub fn to_base64(&self) -> String; pub fn parse_base64(s: &str) -> DomainResult<Nonce>; }
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PairingId(pub String);
impl PairingId { pub fn new() -> PairingId; } // plus Default
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct PairingCode(pub u32);
impl PairingCode { pub fn digits(&self) -> String; } // Display: "048 213"

pub fn commitment(responder_spki: &[u8], initiator_spki: &[u8], nb: &Nonce) -> [u8; 32];
pub fn pairing_code(initiator_spki: &[u8], responder_spki: &[u8], na: &Nonce, nb: &Nonce) -> PairingCode;
// Additions, so the REST layer needs no base64 code of its own:
pub fn commitment_to_base64(commitment: &[u8; 32]) -> String;
pub fn commitment_from_base64(s: &str) -> DomainResult<[u8; 32]>;

pub enum PairingRole { Initiator, Responder }
pub enum PairingState { Committed, CodeShown, Paired, Declined, Expired, Cancelled, Failed }
pub enum PairingStatus { Pending, Accepted, Declined, Expired }   // serde snake_case
pub enum PairingOutcome { Accepted, Declined, Expired, Cancelled, Failed }
impl PairingOutcome { pub fn as_str(&self) -> &'static str; }

pub struct PairingSession { /* private */ }
impl PairingSession {
    pub fn new_responder(id: PairingId, peer: PeerIdentity, peer_profile: NodeProfile,
        local_spki: Vec<u8>, nb: Nonce, now: Instant) -> (PairingSession, [u8; 32]);
    pub fn new_initiator(id: PairingId, peer: PeerIdentity, peer_profile: NodeProfile,
        local_spki: Vec<u8>, commitment: [u8; 32], na: Nonce, now: Instant) -> PairingSession;
    pub fn receive_nonce(&mut self, na: Nonce) -> DomainResult<(Nonce, PairingCode)>;
    pub fn receive_reveal(&mut self, nb: Nonce) -> DomainResult<PairingCode>;
    pub fn decide_local(&mut self, accept: bool) -> DomainResult<()>;
    pub fn decide_remote(&mut self, accept: bool) -> DomainResult<()>;
    pub fn cancel(&mut self);
    pub fn fail(&mut self);
    pub fn expire_if_due(&mut self, now: Instant) -> bool;
    pub fn id(&self) -> &PairingId; pub fn role(&self) -> PairingRole; pub fn state(&self) -> PairingState;
    pub fn peer(&self) -> &PeerIdentity; pub fn peer_profile(&self) -> &NodeProfile;
    pub fn code(&self) -> Option<PairingCode>;
    pub fn local_nonce_b64(&self) -> String;
    pub fn status(&self) -> PairingStatus;
    pub fn outcome(&self) -> Option<PairingOutcome>;
    pub fn is_terminal(&self) -> bool;
}
```

Rules this task implements and tests:

- Initiator A holds `na`. Responder B holds `nb` and the commitment `Cb = SHA-256("rocket-pair-commit-v1" || len(PkB) || PkB || len(PkA) || PkA || Nb)`.
- The code is `SHA-256("rocket-pair-code-v1" || len(PkA) || PkA || len(PkB) || PkB || Na || Nb)`, first 8 bytes as a big-endian u64, modulo 1 000 000. Length prefix is a u32 big-endian byte count.
- `receive_nonce` (responder, state `Committed`) moves to `CodeShown` and returns `nb` and the code. `receive_reveal` (initiator, state `Committed`) checks the commitment; a mismatch ends `Failed` and returns `Err(InvalidInput)`.
- Both decisions `true` give `Paired`. Any `false` gives `Declined`. A decision in any state but `CodeShown` is `Err(Conflict)`, and so is a second decision from the same side.
- `expire_if_due(now)` expires a non-terminal session when `now - created >= PAIRING_TTL`. The caller (Plan 04) calls it before every decision. A session that is already terminal never changes state again.
- `cancel` ends a non-terminal session as `Cancelled`.

Expected values used in the tests were computed outside Rust (Python `hashlib`) for responder key `[0xB2; 44]`, initiator key `[0xA1; 44]`, `na = [1; 32]`, `nb = [2; 32]`: commitment `b57ff7f7441d6019f770d430fd2e8e4ede27c8fd7835315a0131943b7f3d527a`, code `162483`, code with the two keys swapped `595548`. With `na = [60; 32]` the code is `5263`, which shows as `005263`.

- [ ] **Step 1: Declare the module in `lib.rs` and create the test-only file**

Replace `crates/rocket-network/src/lib.rs` with:

```rust
pub mod device;
pub mod identity;
pub mod limits;
pub mod pairing;
pub mod profile;
pub mod records;

pub use device::DeviceId;
pub use identity::{DeviceKeyMaterial, PeerIdentity};
pub use pairing::{
    commitment, commitment_from_base64, commitment_to_base64, pairing_code, Nonce, PairingCode,
    PairingId, PairingOutcome, PairingRole, PairingSession, PairingState, PairingStatus,
};
pub use profile::{default_display_name, initials_for, NodeProfile};
pub use records::{
    BlockedDevice, DiscoveredNode, DiscoverySource, ManualAddress, NetworkSettings, PairedDevice,
};
```

Create `crates/rocket-network/src/pairing.rs` with the tests only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::DeviceId;
    use rocket_shared::error::DomainError;
    use std::time::Duration;

    fn spki(byte: u8) -> Vec<u8> {
        vec![byte; 44]
    }

    fn nonce(byte: u8) -> Nonce {
        Nonce([byte; 32])
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn profile() -> NodeProfile {
        NodeProfile::new("Ada Lovelace").expect("valid name")
    }

    /// Responder B (key 0xB2) and initiator A (key 0xA1) after step 1.
    fn pair_at(now: Instant, na: Nonce, nb: Nonce) -> (PairingSession, PairingSession) {
        let (responder, c) = PairingSession::new_responder(
            PairingId::new(),
            PeerIdentity::from_spki(spki(0xA1)),
            profile(),
            spki(0xB2),
            nb,
            now,
        );
        let initiator = PairingSession::new_initiator(
            responder.id().clone(),
            PeerIdentity::from_spki(spki(0xB2)),
            profile(),
            spki(0xA1),
            c,
            na,
            now,
        );
        (responder, initiator)
    }

    /// Both sessions after steps 2 and 3, with the code shown.
    fn shown(now: Instant) -> (PairingSession, PairingSession) {
        let (mut responder, mut initiator) = pair_at(now, nonce(1), nonce(2));
        let (nb, _) = responder.receive_nonce(nonce(1)).expect("responder takes na");
        initiator.receive_reveal(nb).expect("commitment matches");
        (responder, initiator)
    }

    // Nonce

    #[test]
    fn nonce_base64_round_trips() {
        let original = nonce(9);
        let text = original.to_base64();
        assert_eq!(text.len(), 44);
        let parsed = Nonce::parse_base64(&text).expect("round trip");
        assert!(parsed == original);
    }

    #[test]
    fn nonce_parse_rejects_wrong_length_and_garbage() {
        let short = STANDARD.encode([1u8; 31]);
        let long = STANDARD.encode([1u8; 33]);
        for bad in ["", "!!!!", short.as_str(), long.as_str(), "AAAA"] {
            let err = Nonce::parse_base64(bad).err().expect("rejected");
            assert!(matches!(err, DomainError::InvalidInput(_)), "input {bad:?}");
        }
    }

    #[test]
    fn random_nonces_differ() {
        assert!(Nonce::random() != Nonce::random());
    }

    // Pairing id and code formatting

    #[test]
    fn pairing_id_is_a_unique_uuid_string() {
        let a = PairingId::new();
        let b = PairingId::default();
        assert_ne!(a, b);
        assert_eq!(a.0.len(), 36);
        assert!(uuid::Uuid::parse_str(&a.0).is_ok());
    }

    #[test]
    fn code_digits_pad_with_zeros() {
        assert_eq!(PairingCode(4821).digits(), "004821");
        assert_eq!(PairingCode(0).digits(), "000000");
        assert_eq!(PairingCode(999_999).digits(), "999999");
        assert_eq!(PairingCode(48_213).digits(), "048213");
    }

    #[test]
    fn code_display_groups_three_and_three() {
        assert_eq!(PairingCode(48_213).to_string(), "048 213");
        assert_eq!(PairingCode(4821).to_string(), "004 821");
        assert_eq!(PairingCode(999_999).to_string(), "999 999");
    }

    // Commitment and code functions

    #[test]
    fn commitment_matches_the_known_vector() {
        let c = commitment(&spki(0xB2), &spki(0xA1), &nonce(2));
        assert_eq!(
            hex(&c),
            "b57ff7f7441d6019f770d430fd2e8e4ede27c8fd7835315a0131943b7f3d527a"
        );
    }

    #[test]
    fn commitment_depends_on_every_input() {
        let base = commitment(&spki(0xB2), &spki(0xA1), &nonce(2));
        assert_ne!(base, commitment(&spki(0xB3), &spki(0xA1), &nonce(2)));
        assert_ne!(base, commitment(&spki(0xB2), &spki(0xA2), &nonce(2)));
        assert_ne!(base, commitment(&spki(0xB2), &spki(0xA1), &nonce(3)));
        // Swapping the two keys is a different commitment.
        assert_ne!(base, commitment(&spki(0xA1), &spki(0xB2), &nonce(2)));
    }

    #[test]
    fn length_prefix_stops_boundary_shifting() {
        // Without a length prefix these two inputs would hash the same bytes.
        let a = commitment(&[1, 2, 3], &[4, 5], &nonce(2));
        let b = commitment(&[1, 2], &[3, 4, 5], &nonce(2));
        assert_ne!(a, b);
    }

    #[test]
    fn pairing_code_matches_the_known_vector() {
        let code = pairing_code(&spki(0xA1), &spki(0xB2), &nonce(1), &nonce(2));
        assert_eq!(code, PairingCode(162_483));
    }

    #[test]
    fn pairing_code_pads_leading_zeros() {
        let code = pairing_code(&spki(0xA1), &spki(0xB2), &nonce(60), &nonce(2));
        assert_eq!(code, PairingCode(5_263));
        assert_eq!(code.digits(), "005263");
        assert_eq!(code.to_string(), "005 263");
    }

    #[test]
    fn swapping_either_spki_changes_the_code() {
        let base = pairing_code(&spki(0xA1), &spki(0xB2), &nonce(1), &nonce(2));
        let swapped = pairing_code(&spki(0xB2), &spki(0xA1), &nonce(1), &nonce(2));
        assert_eq!(swapped, PairingCode(595_548));
        assert_ne!(base, swapped);
        let other_initiator = pairing_code(&spki(0xC3), &spki(0xB2), &nonce(1), &nonce(2));
        let other_responder = pairing_code(&spki(0xA1), &spki(0xC3), &nonce(1), &nonce(2));
        assert_ne!(base, other_initiator);
        assert_ne!(base, other_responder);
    }

    #[test]
    fn changing_either_nonce_changes_the_code() {
        let base = pairing_code(&spki(0xA1), &spki(0xB2), &nonce(1), &nonce(2));
        assert_ne!(base, pairing_code(&spki(0xA1), &spki(0xB2), &nonce(5), &nonce(2)));
        assert_ne!(base, pairing_code(&spki(0xA1), &spki(0xB2), &nonce(1), &nonce(6)));
    }

    #[test]
    fn code_is_always_below_one_million() {
        for byte in 0..=255u8 {
            let code = pairing_code(&spki(0xA1), &spki(0xB2), &nonce(byte), &nonce(2));
            assert!(code.0 < 1_000_000);
            assert_eq!(code.digits().len(), 6);
            assert!(code.digits().chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn commitment_base64_round_trips_and_checks_length() {
        let c = commitment(&spki(0xB2), &spki(0xA1), &nonce(2));
        let text = commitment_to_base64(&c);
        assert_eq!(commitment_from_base64(&text).expect("round trip"), c);
        let short = STANDARD.encode([1u8; 31]);
        for bad in ["", "!!!!", short.as_str()] {
            let err = commitment_from_base64(bad).expect_err("rejected");
            assert!(matches!(err, DomainError::InvalidInput(_)), "input {bad:?}");
        }
    }

    // Session: the happy path

    #[test]
    fn responder_returns_the_commitment_and_starts_committed() {
        let now = Instant::now();
        let (responder, initiator) = pair_at(now, nonce(1), nonce(2));
        assert_eq!(responder.state(), PairingState::Committed);
        assert_eq!(initiator.state(), PairingState::Committed);
        assert_eq!(responder.role(), PairingRole::Responder);
        assert_eq!(initiator.role(), PairingRole::Initiator);
        assert_eq!(responder.status(), PairingStatus::Pending);
        assert!(responder.code().is_none());
        assert!(responder.outcome().is_none());
        assert!(!responder.is_terminal());
        assert_eq!(responder.id(), initiator.id());
        assert_eq!(responder.peer().device_id, DeviceId::from_spki(&spki(0xA1)));
        assert_eq!(responder.peer_profile().display_name, "Ada Lovelace");
    }

    #[test]
    fn new_responder_commitment_matches_the_function() {
        let (_, c) = PairingSession::new_responder(
            PairingId::new(),
            PeerIdentity::from_spki(spki(0xA1)),
            profile(),
            spki(0xB2),
            nonce(2),
            Instant::now(),
        );
        assert_eq!(c, commitment(&spki(0xB2), &spki(0xA1), &nonce(2)));
    }

    #[test]
    fn end_to_end_codes_match() {
        let now = Instant::now();
        let (mut responder, mut initiator) = pair_at(now, nonce(1), nonce(2));
        let (nb, responder_code) = responder.receive_nonce(nonce(1)).expect("step 2");
        assert!(nb == nonce(2));
        assert_eq!(responder.state(), PairingState::CodeShown);
        let initiator_code = initiator.receive_reveal(nb).expect("step 3");
        assert_eq!(initiator.state(), PairingState::CodeShown);
        assert_eq!(responder_code, initiator_code);
        assert_eq!(responder_code, PairingCode(162_483));
        assert_eq!(responder.code(), Some(responder_code));
        assert_eq!(initiator.code(), Some(initiator_code));
    }

    #[test]
    fn end_to_end_codes_match_with_random_nonces() {
        let now = Instant::now();
        let (mut responder, mut initiator) = pair_at(now, Nonce::random(), Nonce::random());
        let na = Nonce::parse_base64(&initiator.local_nonce_b64()).expect("na on the wire");
        let (nb, responder_code) = responder.receive_nonce(na).expect("step 2");
        assert_eq!(nb.to_base64(), responder.local_nonce_b64());
        let initiator_code = initiator.receive_reveal(nb).expect("step 3");
        assert_eq!(responder_code, initiator_code);
    }

    #[test]
    fn a_man_in_the_middle_sees_different_codes() {
        let now = Instant::now();
        // A talks to Mallory (key 0xC3), who talks to B (key 0xB2).
        let (mut mallory_for_a, mut a) = {
            let (r, c) = PairingSession::new_responder(
                PairingId::new(),
                PeerIdentity::from_spki(spki(0xA1)),
                profile(),
                spki(0xC3),
                nonce(7),
                now,
            );
            let a = PairingSession::new_initiator(
                r.id().clone(),
                PeerIdentity::from_spki(spki(0xC3)),
                profile(),
                spki(0xA1),
                c,
                nonce(1),
                now,
            );
            (r, a)
        };
        let (mut b, mut mallory_for_b) = {
            let (r, c) = PairingSession::new_responder(
                PairingId::new(),
                PeerIdentity::from_spki(spki(0xC3)),
                profile(),
                spki(0xB2),
                nonce(2),
                now,
            );
            let m = PairingSession::new_initiator(
                r.id().clone(),
                PeerIdentity::from_spki(spki(0xB2)),
                profile(),
                spki(0xC3),
                c,
                nonce(8),
                now,
            );
            (r, m)
        };
        let (nb_m, code_a_side) = mallory_for_a.receive_nonce(nonce(1)).expect("a side");
        a.receive_reveal(nb_m).expect("a checks");
        let (nb, code_b_side) = b.receive_nonce(nonce(8)).expect("b side");
        mallory_for_b.receive_reveal(nb).expect("m checks");
        assert_ne!(a.code(), b.code());
        assert_ne!(code_a_side, code_b_side);
    }

    #[test]
    fn wrong_revealed_nonce_fails_the_session() {
        let now = Instant::now();
        let (mut responder, mut initiator) = pair_at(now, nonce(1), nonce(2));
        responder.receive_nonce(nonce(1)).expect("step 2");
        let err = initiator.receive_reveal(nonce(9)).expect_err("commitment mismatch");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        assert_eq!(initiator.state(), PairingState::Failed);
        assert!(initiator.is_terminal());
        assert!(initiator.code().is_none());
        assert_eq!(initiator.outcome(), Some(PairingOutcome::Failed));
        assert_eq!(initiator.status(), PairingStatus::Declined);
        // A later correct reveal cannot revive the session.
        let err = initiator.receive_reveal(nonce(2)).expect_err("already failed");
        assert!(matches!(err, DomainError::Conflict(_)));
        assert_eq!(initiator.state(), PairingState::Failed);
    }

    #[test]
    fn roles_cannot_use_the_other_roles_steps() {
        let now = Instant::now();
        let (mut responder, mut initiator) = pair_at(now, nonce(1), nonce(2));
        let err = initiator.receive_nonce(nonce(1)).err().expect("initiator");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        let err = responder.receive_reveal(nonce(2)).expect_err("responder");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        assert_eq!(responder.state(), PairingState::Committed);
        assert_eq!(initiator.state(), PairingState::Committed);
    }

    #[test]
    fn receive_nonce_twice_is_a_conflict() {
        let now = Instant::now();
        let (mut responder, _) = pair_at(now, nonce(1), nonce(2));
        responder.receive_nonce(nonce(1)).expect("first");
        let err = responder.receive_nonce(nonce(4)).err().expect("second");
        assert!(matches!(err, DomainError::Conflict(_)));
        assert_eq!(responder.code(), Some(PairingCode(162_483)));
    }

    // Session: decisions

    #[test]
    fn accept_and_accept_pairs_in_either_order() {
        let (mut responder, _) = shown(Instant::now());
        responder.decide_local(true).expect("local");
        assert_eq!(responder.state(), PairingState::CodeShown);
        assert_eq!(responder.status(), PairingStatus::Pending);
        responder.decide_remote(true).expect("remote");
        assert_eq!(responder.state(), PairingState::Paired);
        assert_eq!(responder.status(), PairingStatus::Accepted);
        assert_eq!(responder.outcome(), Some(PairingOutcome::Accepted));
        assert!(responder.is_terminal());

        let (_, mut initiator) = shown(Instant::now());
        initiator.decide_remote(true).expect("remote first");
        initiator.decide_local(true).expect("local second");
        assert_eq!(initiator.state(), PairingState::Paired);
    }

    #[test]
    fn any_decline_declines_the_session() {
        let (mut a, _) = shown(Instant::now());
        a.decide_local(false).expect("local decline");
        assert_eq!(a.state(), PairingState::Declined);
        assert_eq!(a.status(), PairingStatus::Declined);
        assert_eq!(a.outcome(), Some(PairingOutcome::Declined));

        let (mut b, _) = shown(Instant::now());
        b.decide_remote(false).expect("remote decline");
        assert_eq!(b.state(), PairingState::Declined);

        // A decline after the other side accepted still declines.
        let (mut c, _) = shown(Instant::now());
        c.decide_local(true).expect("local accept");
        c.decide_remote(false).expect("remote decline");
        assert_eq!(c.state(), PairingState::Declined);
        assert_eq!(c.status(), PairingStatus::Declined);
    }

    #[test]
    fn decision_before_the_code_is_shown_is_an_error() {
        let (mut responder, mut initiator) = pair_at(Instant::now(), nonce(1), nonce(2));
        assert!(responder.decide_local(true).is_err());
        assert!(responder.decide_remote(true).is_err());
        assert!(initiator.decide_local(true).is_err());
        assert!(initiator.decide_remote(false).is_err());
        assert_eq!(responder.state(), PairingState::Committed);
        assert_eq!(initiator.state(), PairingState::Committed);
    }

    #[test]
    fn decision_after_a_terminal_state_is_a_conflict() {
        let (mut session, _) = shown(Instant::now());
        session.decide_local(true).expect("local");
        session.decide_remote(true).expect("remote");
        for result in [
            session.decide_local(true),
            session.decide_local(false),
            session.decide_remote(true),
            session.decide_remote(false),
        ] {
            assert!(matches!(result, Err(DomainError::Conflict(_))));
        }
        assert_eq!(session.state(), PairingState::Paired);

        let (mut declined, _) = shown(Instant::now());
        declined.decide_remote(false).expect("decline");
        assert!(matches!(
            declined.decide_local(true),
            Err(DomainError::Conflict(_))
        ));
        assert_eq!(declined.state(), PairingState::Declined);
    }

    #[test]
    fn the_same_side_cannot_decide_twice() {
        let (mut session, _) = shown(Instant::now());
        session.decide_local(true).expect("first");
        assert!(matches!(
            session.decide_local(false),
            Err(DomainError::Conflict(_))
        ));
        assert_eq!(session.state(), PairingState::CodeShown);
    }

    // Session: expiry and cancel

    #[test]
    fn session_expires_exactly_at_the_ttl() {
        let start = Instant::now();
        let (mut session, _) = shown(start);
        assert!(!session.expire_if_due(start));
        assert!(!session.expire_if_due(start + PAIRING_TTL - Duration::from_secs(1)));
        assert_eq!(session.state(), PairingState::CodeShown);
        assert!(session.expire_if_due(start + PAIRING_TTL));
        assert_eq!(session.state(), PairingState::Expired);
        assert_eq!(session.status(), PairingStatus::Expired);
        assert_eq!(session.outcome(), Some(PairingOutcome::Expired));
        assert!(session.is_terminal());
    }

    #[test]
    fn expiry_counts_from_creation_not_from_the_code() {
        let start = Instant::now();
        let (mut session, _) = pair_at(start, nonce(1), nonce(2));
        assert!(session.expire_if_due(start + PAIRING_TTL + Duration::from_secs(5)));
        assert_eq!(session.state(), PairingState::Expired);
    }

    #[test]
    fn a_clock_before_creation_never_expires() {
        let later = Instant::now() + Duration::from_secs(30);
        let (mut session, _) = pair_at(later, nonce(1), nonce(2));
        assert!(!session.expire_if_due(Instant::now()));
        assert_eq!(session.state(), PairingState::Committed);
    }

    #[test]
    fn expiry_does_not_touch_a_terminal_session() {
        let start = Instant::now();
        let (mut session, _) = shown(start);
        session.decide_local(true).expect("local");
        session.decide_remote(true).expect("remote");
        assert!(!session.expire_if_due(start + PAIRING_TTL * 2));
        assert_eq!(session.state(), PairingState::Paired);
        assert_eq!(session.outcome(), Some(PairingOutcome::Accepted));
    }

    #[test]
    fn decision_after_expiry_is_conflict() {
        let start = Instant::now();
        let (mut session, _) = shown(start);
        session.decide_local(true).expect("local accept before expiry");
        assert!(session.expire_if_due(start + PAIRING_TTL));
        let err = session.decide_remote(true).expect_err("expired");
        assert!(matches!(err, DomainError::Conflict(_)));
        assert_eq!(session.state(), PairingState::Expired);
        assert_ne!(session.status(), PairingStatus::Accepted);
    }

    #[test]
    fn steps_after_expiry_are_conflicts() {
        let start = Instant::now();
        let (mut responder, _) = pair_at(start, nonce(1), nonce(2));
        assert!(responder.expire_if_due(start + PAIRING_TTL));
        assert!(matches!(
            responder.receive_nonce(nonce(1)),
            Err(DomainError::Conflict(_))
        ));
        assert!(responder.code().is_none());
    }

    #[test]
    fn cancel_ends_a_live_session() {
        let (mut session, _) = shown(Instant::now());
        session.cancel();
        assert_eq!(session.state(), PairingState::Cancelled);
        assert_eq!(session.status(), PairingStatus::Declined);
        assert_eq!(session.outcome(), Some(PairingOutcome::Cancelled));
        assert!(session.is_terminal());
        assert!(matches!(
            session.decide_local(true),
            Err(DomainError::Conflict(_))
        ));
    }

    #[test]
    fn cancel_does_not_change_a_finished_session() {
        let (mut session, _) = shown(Instant::now());
        session.decide_local(true).expect("local");
        session.decide_remote(true).expect("remote");
        session.cancel();
        assert_eq!(session.state(), PairingState::Paired);
        assert_eq!(session.outcome(), Some(PairingOutcome::Accepted));
    }

    #[test]
    fn fail_ends_a_live_session_as_failed() {
        let (mut session, _) = shown(Instant::now());
        session.fail();
        assert_eq!(session.state(), PairingState::Failed);
        assert_eq!(session.status(), PairingStatus::Declined);
        assert_eq!(session.outcome(), Some(PairingOutcome::Failed));
        assert!(session.is_terminal());
    }

    #[test]
    fn fail_does_not_change_a_finished_session() {
        let (mut session, _) = shown(Instant::now());
        session.decide_local(false).expect("local");
        session.fail();
        assert_eq!(session.outcome(), Some(PairingOutcome::Declined));
    }

    #[test]
    fn outcome_names_are_stable() {
        assert_eq!(PairingOutcome::Accepted.as_str(), "accepted");
        assert_eq!(PairingOutcome::Declined.as_str(), "declined");
        assert_eq!(PairingOutcome::Expired.as_str(), "expired");
        assert_eq!(PairingOutcome::Cancelled.as_str(), "cancelled");
        assert_eq!(PairingOutcome::Failed.as_str(), "failed");
    }

    #[test]
    fn status_serializes_in_snake_case() {
        for (status, text) in [
            (PairingStatus::Pending, "\"pending\""),
            (PairingStatus::Accepted, "\"accepted\""),
            (PairingStatus::Declined, "\"declined\""),
            (PairingStatus::Expired, "\"expired\""),
        ] {
            assert_eq!(serde_json::to_string(&status).expect("serializes"), text);
        }
    }
}
```

- [ ] **Step 2: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket-network pairing`
Expected: FAIL to compile. Typical errors: `cannot find type Nonce in this scope`, `cannot find function commitment`, `cannot find type PairingSession`, and `unresolved imports crate::pairing::...` in `lib.rs`.

- [ ] **Step 3: Implement `pairing.rs`**

Add this above the `#[cfg(test)]` line in `crates/rocket-network/src/pairing.rs`:

```rust
//! Pairing: the commitment and code functions and the session state machine.
//!
//! No clock is read here. The caller passes `Instant` values in.

use std::fmt;
use std::time::Instant;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::{rngs::OsRng, RngCore};
use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::identity::PeerIdentity;
use crate::limits::PAIRING_TTL;
use crate::profile::NodeProfile;

const COMMIT_LABEL: &[u8] = b"rocket-pair-commit-v1";
const CODE_LABEL: &[u8] = b"rocket-pair-code-v1";
/// Base64 length of 32 bytes with padding.
const BASE64_LEN_32: usize = 44;

/// Feeds a byte string into the hash with a big-endian u32 length in front.
fn update_prefixed(hasher: &mut Sha256, data: &[u8]) {
    let len = u32::try_from(data.len()).unwrap_or(u32::MAX);
    hasher.update(len.to_be_bytes());
    hasher.update(data);
}

/// Compares two 32-byte values without an early exit.
fn constant_time_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn decode_32(s: &str, what: &str) -> DomainResult<[u8; 32]> {
    let invalid = || DomainError::InvalidInput(format!("invalid {what}"));
    if s.len() != BASE64_LEN_32 {
        return Err(invalid());
    }
    let bytes = STANDARD.decode(s).map_err(|_| invalid())?;
    bytes.try_into().map_err(|_| invalid())
}

/// 32 random bytes. No `Debug`, so a nonce cannot be logged by accident.
#[derive(Clone, PartialEq, Eq)]
pub struct Nonce(pub [u8; 32]);

impl Nonce {
    pub fn random() -> Nonce {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        Nonce(bytes)
    }

    pub fn to_base64(&self) -> String {
        STANDARD.encode(self.0)
    }

    /// Accepts exactly 32 bytes in standard padded base64.
    pub fn parse_base64(s: &str) -> DomainResult<Nonce> {
        decode_32(s, "nonce").map(Nonce)
    }
}

/// Encodes a commitment for the REST body.
pub fn commitment_to_base64(commitment: &[u8; 32]) -> String {
    STANDARD.encode(commitment)
}

/// Decodes a commitment from the REST body.
pub fn commitment_from_base64(s: &str) -> DomainResult<[u8; 32]> {
    decode_32(s, "commitment")
}

/// The id of one pairing session. A UUID v4 string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PairingId(pub String);

impl PairingId {
    pub fn new() -> PairingId {
        PairingId(uuid::Uuid::new_v4().to_string())
    }
}

impl Default for PairingId {
    fn default() -> Self {
        PairingId::new()
    }
}

/// The 6-digit code both users compare. The value is 0 to 999 999.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PairingCode(pub u32);

impl PairingCode {
    /// Six digits with leading zeros, for example `048213`.
    pub fn digits(&self) -> String {
        format!("{:06}", self.0)
    }
}

impl fmt::Display for PairingCode {
    /// Two groups of three, for example `048 213`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let digits = self.digits();
        write!(f, "{} {}", &digits[..3], &digits[3..])
    }
}

/// `Cb = SHA-256("rocket-pair-commit-v1" || PkB || PkA || Nb)`, keys length-prefixed.
pub fn commitment(responder_spki: &[u8], initiator_spki: &[u8], nb: &Nonce) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(COMMIT_LABEL);
    update_prefixed(&mut hasher, responder_spki);
    update_prefixed(&mut hasher, initiator_spki);
    hasher.update(nb.0);
    hasher.finalize().into()
}

/// The pairing code from both keys and both nonces.
pub fn pairing_code(
    initiator_spki: &[u8],
    responder_spki: &[u8],
    na: &Nonce,
    nb: &Nonce,
) -> PairingCode {
    let mut hasher = Sha256::new();
    hasher.update(CODE_LABEL);
    update_prefixed(&mut hasher, initiator_spki);
    update_prefixed(&mut hasher, responder_spki);
    hasher.update(na.0);
    hasher.update(nb.0);
    let digest: [u8; 32] = hasher.finalize().into();
    let mut head = [0u8; 8];
    head.copy_from_slice(&digest[..8]);
    let value = u64::from_be_bytes(head) % 1_000_000;
    PairingCode(u32::try_from(value).unwrap_or(0))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingRole {
    Initiator,
    Responder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingState {
    Committed,
    CodeShown,
    Paired,
    Declined,
    Expired,
    Cancelled,
    Failed,
}

/// What the initiator sees when it polls `GET /pair/{id}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairingStatus {
    Pending,
    Accepted,
    Declined,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingOutcome {
    Accepted,
    Declined,
    Expired,
    Cancelled,
    Failed,
}

impl PairingOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            PairingOutcome::Accepted => "accepted",
            PairingOutcome::Declined => "declined",
            PairingOutcome::Expired => "expired",
            PairingOutcome::Cancelled => "cancelled",
            PairingOutcome::Failed => "failed",
        }
    }
}

/// One pairing, seen from one side. Lives in memory only.
pub struct PairingSession {
    id: PairingId,
    role: PairingRole,
    peer: PeerIdentity,
    peer_profile: NodeProfile,
    local_spki: Vec<u8>,
    state: PairingState,
    created: Instant,
    /// `nb` for a responder, `na` for an initiator.
    local_nonce: Nonce,
    /// The commitment the initiator was given. `None` for a responder.
    expected_commitment: Option<[u8; 32]>,
    code: Option<PairingCode>,
    local_decision: Option<bool>,
    remote_decision: Option<bool>,
    outcome: Option<PairingOutcome>,
}

impl PairingSession {
    /// Starts a session for the side that received `POST /pair/request`.
    /// Returns the session and the commitment to send back.
    pub fn new_responder(
        id: PairingId,
        peer: PeerIdentity,
        peer_profile: NodeProfile,
        local_spki: Vec<u8>,
        nb: Nonce,
        now: Instant,
    ) -> (PairingSession, [u8; 32]) {
        let commit = commitment(&local_spki, &peer.spki_der, &nb);
        let session = PairingSession {
            id,
            role: PairingRole::Responder,
            peer,
            peer_profile,
            local_spki,
            state: PairingState::Committed,
            created: now,
            local_nonce: nb,
            expected_commitment: None,
            code: None,
            local_decision: None,
            remote_decision: None,
            outcome: None,
        };
        (session, commit)
    }

    /// Starts a session for the side that sent `POST /pair/request` and got a commitment.
    pub fn new_initiator(
        id: PairingId,
        peer: PeerIdentity,
        peer_profile: NodeProfile,
        local_spki: Vec<u8>,
        commitment: [u8; 32],
        na: Nonce,
        now: Instant,
    ) -> PairingSession {
        PairingSession {
            id,
            role: PairingRole::Initiator,
            peer,
            peer_profile,
            local_spki,
            state: PairingState::Committed,
            created: now,
            local_nonce: na,
            expected_commitment: Some(commitment),
            code: None,
            local_decision: None,
            remote_decision: None,
            outcome: None,
        }
    }

    /// Responder, step 2: takes `na`, shows the code, returns `nb` to reveal.
    pub fn receive_nonce(&mut self, na: Nonce) -> DomainResult<(Nonce, PairingCode)> {
        if self.role != PairingRole::Responder {
            return Err(DomainError::InvalidInput(
                "only the responder receives the initiator nonce".to_string(),
            ));
        }
        self.require_committed()?;
        let code = pairing_code(&self.peer.spki_der, &self.local_spki, &na, &self.local_nonce);
        self.code = Some(code);
        self.state = PairingState::CodeShown;
        Ok((self.local_nonce.clone(), code))
    }

    /// Initiator, step 3: checks the revealed `nb` against the commitment.
    /// A mismatch ends the session as `Failed`.
    pub fn receive_reveal(&mut self, nb: Nonce) -> DomainResult<PairingCode> {
        if self.role != PairingRole::Initiator {
            return Err(DomainError::InvalidInput(
                "only the initiator receives the revealed nonce".to_string(),
            ));
        }
        self.require_committed()?;
        let expected = self
            .expected_commitment
            .ok_or_else(|| DomainError::Internal("initiator has no commitment".to_string()))?;
        let actual = commitment(&self.peer.spki_der, &self.local_spki, &nb);
        if !constant_time_eq(&expected, &actual) {
            self.finish(PairingState::Failed, PairingOutcome::Failed);
            return Err(DomainError::InvalidInput("pairing failed".to_string()));
        }
        let code = pairing_code(&self.local_spki, &self.peer.spki_der, &self.local_nonce, &nb);
        self.code = Some(code);
        self.state = PairingState::CodeShown;
        Ok(code)
    }

    /// The local user decided.
    pub fn decide_local(&mut self, accept: bool) -> DomainResult<()> {
        self.decide(true, accept)
    }

    /// The peer's decision arrived.
    pub fn decide_remote(&mut self, accept: bool) -> DomainResult<()> {
        self.decide(false, accept)
    }

    /// Ends a live session. A finished session stays as it is.
    pub fn cancel(&mut self) {
        if !self.is_terminal() {
            self.finish(PairingState::Cancelled, PairingOutcome::Cancelled);
        }
    }

    /// Ends a live session as failed, for example when the peer cannot be
    /// reached partway through. A finished session stays as it is.
    pub fn fail(&mut self) {
        if !self.is_terminal() {
            self.finish(PairingState::Failed, PairingOutcome::Failed);
        }
    }

    /// Expires a live session once `PAIRING_TTL` has passed since creation.
    /// Returns true when this call expired it. Call it before every decision.
    pub fn expire_if_due(&mut self, now: Instant) -> bool {
        if self.is_terminal() {
            return false;
        }
        if now.saturating_duration_since(self.created) >= PAIRING_TTL {
            self.finish(PairingState::Expired, PairingOutcome::Expired);
            return true;
        }
        false
    }

    pub fn id(&self) -> &PairingId {
        &self.id
    }

    pub fn role(&self) -> PairingRole {
        self.role
    }

    pub fn state(&self) -> PairingState {
        self.state
    }

    pub fn peer(&self) -> &PeerIdentity {
        &self.peer
    }

    pub fn peer_profile(&self) -> &NodeProfile {
        &self.peer_profile
    }

    pub fn code(&self) -> Option<PairingCode> {
        self.code
    }

    /// The local nonce in base64, for the wire.
    pub fn local_nonce_b64(&self) -> String {
        self.local_nonce.to_base64()
    }

    pub fn status(&self) -> PairingStatus {
        match self.state {
            PairingState::Paired => PairingStatus::Accepted,
            PairingState::Declined | PairingState::Cancelled | PairingState::Failed => {
                PairingStatus::Declined
            }
            PairingState::Expired => PairingStatus::Expired,
            PairingState::Committed | PairingState::CodeShown => PairingStatus::Pending,
        }
    }

    /// Set once, when the session becomes terminal.
    pub fn outcome(&self) -> Option<PairingOutcome> {
        self.outcome
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            PairingState::Paired
                | PairingState::Declined
                | PairingState::Expired
                | PairingState::Cancelled
                | PairingState::Failed
        )
    }

    fn finish(&mut self, state: PairingState, outcome: PairingOutcome) {
        self.state = state;
        self.outcome = Some(outcome);
    }

    fn require_committed(&self) -> DomainResult<()> {
        match self.state {
            PairingState::Committed => Ok(()),
            _ => Err(DomainError::Conflict(
                "pairing is not waiting for this step".to_string(),
            )),
        }
    }

    fn decide(&mut self, from_local: bool, accept: bool) -> DomainResult<()> {
        if self.is_terminal() {
            return Err(DomainError::Conflict("pairing is already finished".to_string()));
        }
        if self.state != PairingState::CodeShown {
            return Err(DomainError::Conflict("pairing has no code yet".to_string()));
        }
        let slot = if from_local {
            &mut self.local_decision
        } else {
            &mut self.remote_decision
        };
        if slot.is_some() {
            return Err(DomainError::Conflict("decision already recorded".to_string()));
        }
        *slot = Some(accept);
        if !accept {
            self.finish(PairingState::Declined, PairingOutcome::Declined);
        } else if self.local_decision == Some(true) && self.remote_decision == Some(true) {
            self.finish(PairingState::Paired, PairingOutcome::Accepted);
        }
        Ok(())
    }
}
```

- [ ] **Step 4: Run the tests and see them pass**

Run: `cargo test -j4 -p rocket-network pairing`
Expected: PASS. All `pairing::tests` pass, 0 failed. If `pairing_code_matches_the_known_vector` fails, the hash input order is wrong (initiator key first, then responder key, then `na`, then `nb`, each key length-prefixed).

- [ ] **Step 5: Check and lint**

Run: `cargo check -j4 -p rocket-network`
Expected: no errors, no warnings.

Run: `cargo clippy -j4 -p rocket-network -- -D warnings`
Expected: no findings in `rocket-network`.

- [ ] **Step 6: Stop**

No commit. Changes are reviewed at the plan end.

---

### Task 3: Presence, dialer, wire types, `PeerMessage`, `NetworkEvent`, ports

**Files:**
- Modify: `crates/rocket-network/src/lib.rs`
- Create: `crates/rocket-network/src/presence.rs`
- Create: `crates/rocket-network/src/dialer.rs`
- Create: `crates/rocket-network/src/rate.rs`
- Create: `crates/rocket-network/src/wire.rs`
- Create: `crates/rocket-network/src/messages.rs`
- Create: `crates/rocket-network/src/events.rs`
- Create: `crates/rocket-network/src/ports.rs`

**Interfaces:**
- Consumes: everything from Tasks 1 and 2, `rocket_shared::published_event::PublishedEvent`.
- Produces (used by Plans 02 to 05, exactly as in the index):

```rust
// presence.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Presence { Unknown, Online, Offline }
pub struct PresenceTracker { /* private */ }
impl PresenceTracker {
    pub fn new() -> PresenceTracker;
    pub fn state(&self) -> Presence;
    pub fn on_connected(&mut self);
    pub fn on_pong(&mut self);
    pub fn on_ping_sent(&mut self) -> Presence;
    pub fn on_disconnected(&mut self);
    pub fn next_retry_delay(&self) -> Duration;
}
pub fn backoff(attempt: u32) -> Duration;

// dialer.rs
pub enum DialHint { Mdns, ManualAddress }
pub fn should_dial(local: &DeviceId, remote: &DeviceId, hint: DialHint, waited: Duration) -> bool;
pub fn keep_dialed_by(local: &DeviceId, remote: &DeviceId) -> DeviceId;

// rate.rs
#[derive(Debug, Clone)]
pub struct SlidingWindow { /* private */ }
impl SlidingWindow {
    pub fn new(cap: usize, window: Duration) -> SlidingWindow;
    pub fn try_hit(&mut self, now: Instant) -> bool;
    pub fn clear(&mut self);
}

// wire.rs: HelloResponse, PairRequest, PairRequestResponse, NonceMessage, DecisionRequest, PairStatusResponse

// messages.rs
pub enum PeerMessage { Hello { version: u32, profile: NodeProfile }, Ping, Pong, Unpair }
impl PeerMessage { pub fn encode(&self) -> String; pub fn decode(text: &str) -> DomainResult<PeerMessage>; }

// events.rs
pub enum NetworkEvent { NodesChanged, StatusChanged, PairedRemoved { .. }, PairingIncoming { .. }, PairingFinished { .. } }

// ports.rs: DeviceIdentityStore, PairedDeviceRepository, BlockedDeviceRepository,
// ManualAddressRepository, NetworkSettingsRepository, QuarantineReport, Announcement,
// DiscoveryEvent, DiscoverySink, Discovery, PeerCallError, PeerError, PeerSocket,
// PeerHandler, PeerConnector, PeerServerControl, PeerTransport, PeerTransportFactory
```

Rules this task implements and tests:

- `PresenceTracker` starts `Unknown`. `on_connected` makes it `Online` with attempt 0 and missed 0. `on_ping_sent` adds one to the missed count and turns an `Online` tracker `Offline` when the count is above `MISSED_PINGS_OFFLINE` (the third unanswered ping). `on_disconnected` makes it `Offline` and adds one to the attempt. `backoff` gives 1, 1, 2, 4, 8, 16, 32, 64, 120, 120 seconds for attempts 0 to 9.
- `SlidingWindow` is the request limit Plan 04 uses for "5 pairing requests per minute". `try_hit(now)` first drops hits that are at least `window` old (a hit exactly `window` old is dropped), then refuses when `cap` hits remain, else records `now` and returns true. A refused hit is not recorded. Cap 0 refuses everything. `clear` forgets all hits. It never reads the clock; the caller passes `Instant`.
- `should_dial`: a manual address always dials. An mDNS sighting dials when `local < remote` or the wait reached `DIAL_GRACE`. `keep_dialed_by` returns the lower id.
- `PeerMessage::decode` refuses a text longer than `MAX_FRAME_BYTES` before parsing, refuses unknown types, never echoes frame content in the error, and rebuilds the initials of a `Hello` profile (rejecting bad names).
- `NetworkEvent` channels: `NodesChanged`, `StatusChanged`, `PairedRemoved` on `network-changed`; `PairingIncoming`, `PairingFinished` on `network-pairing`.
- One addition to the index: `PeerError::status_code()` returns 404, 403, 503, 403, 400, 500 in the order of the variants, so Plan 03 maps errors to HTTP in one place.

- [ ] **Step 1: Update `lib.rs` and create the test-only files**

Replace `crates/rocket-network/src/lib.rs` with:

```rust
pub mod device;
pub mod dialer;
pub mod events;
pub mod identity;
pub mod limits;
pub mod messages;
pub mod pairing;
pub mod ports;
pub mod presence;
pub mod profile;
pub mod rate;
pub mod records;
pub mod wire;

pub use device::DeviceId;
pub use dialer::{keep_dialed_by, should_dial, DialHint};
pub use events::NetworkEvent;
pub use identity::{DeviceKeyMaterial, PeerIdentity};
pub use messages::PeerMessage;
pub use pairing::{
    commitment, commitment_from_base64, commitment_to_base64, pairing_code, Nonce, PairingCode,
    PairingId, PairingOutcome, PairingRole, PairingSession, PairingState, PairingStatus,
};
pub use ports::{
    Announcement, BlockedDeviceRepository, DeviceIdentityStore, Discovery, DiscoveryEvent,
    DiscoverySink, ManualAddressRepository, NetworkSettingsRepository, PairedDeviceRepository,
    PeerCallError, PeerConnector, PeerError, PeerHandler, PeerServerControl, PeerSocket,
    PeerTransport, PeerTransportFactory, QuarantineReport,
};
pub use presence::{backoff, Presence, PresenceTracker};
pub use profile::{default_display_name, initials_for, NodeProfile};
pub use rate::SlidingWindow;
pub use records::{
    BlockedDevice, DiscoveredNode, DiscoverySource, ManualAddress, NetworkSettings, PairedDevice,
};
pub use wire::{
    DecisionRequest, HelloResponse, NonceMessage, PairRequest, PairRequestResponse,
    PairStatusResponse,
};
```

Create `crates/rocket-network/src/presence.rs` with the tests only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_tracker_is_unknown_with_a_one_second_retry() {
        let tracker = PresenceTracker::new();
        assert_eq!(tracker.state(), Presence::Unknown);
        assert_eq!(tracker.next_retry_delay(), Duration::from_secs(1));
        assert_eq!(PresenceTracker::default().state(), Presence::Unknown);
    }

    #[test]
    fn connecting_goes_online_and_resets_the_counters() {
        let mut tracker = PresenceTracker::new();
        tracker.on_disconnected();
        tracker.on_disconnected();
        tracker.on_disconnected();
        assert_eq!(tracker.next_retry_delay(), Duration::from_secs(4));
        tracker.on_connected();
        assert_eq!(tracker.state(), Presence::Online);
        assert_eq!(tracker.next_retry_delay(), Duration::from_secs(1));
    }

    #[test]
    fn the_third_unanswered_ping_goes_offline() {
        let mut tracker = PresenceTracker::new();
        tracker.on_connected();
        assert_eq!(tracker.on_ping_sent(), Presence::Online);
        assert_eq!(tracker.on_ping_sent(), Presence::Online);
        assert_eq!(tracker.on_ping_sent(), Presence::Offline);
        assert_eq!(tracker.state(), Presence::Offline);
    }

    #[test]
    fn a_pong_resets_the_missed_count() {
        let mut tracker = PresenceTracker::new();
        tracker.on_connected();
        tracker.on_ping_sent();
        tracker.on_ping_sent();
        tracker.on_pong();
        assert_eq!(tracker.on_ping_sent(), Presence::Online);
        assert_eq!(tracker.on_ping_sent(), Presence::Online);
        assert_eq!(tracker.on_ping_sent(), Presence::Offline);
    }

    #[test]
    fn pings_do_not_change_an_unknown_tracker() {
        let mut tracker = PresenceTracker::new();
        for _ in 0..5 {
            assert_eq!(tracker.on_ping_sent(), Presence::Unknown);
        }
    }

    #[test]
    fn disconnecting_goes_offline_and_grows_the_backoff() {
        let mut tracker = PresenceTracker::new();
        tracker.on_connected();
        tracker.on_disconnected();
        assert_eq!(tracker.state(), Presence::Offline);
        assert_eq!(tracker.next_retry_delay(), Duration::from_secs(1));
        tracker.on_disconnected();
        assert_eq!(tracker.next_retry_delay(), Duration::from_secs(2));
        tracker.on_disconnected();
        assert_eq!(tracker.next_retry_delay(), Duration::from_secs(4));
    }

    #[test]
    fn backoff_sequence_doubles_and_caps() {
        let seconds: Vec<u64> = (0..10).map(|n| backoff(n).as_secs()).collect();
        assert_eq!(seconds, [1, 1, 2, 4, 8, 16, 32, 64, 120, 120]);
        assert_eq!(backoff(u32::MAX), BACKOFF_MAX);
    }

    #[test]
    fn presence_serializes_in_snake_case() {
        assert_eq!(
            serde_json::to_string(&Presence::Online).expect("serializes"),
            "\"online\""
        );
        assert_eq!(
            serde_json::to_string(&Presence::Unknown).expect("serializes"),
            "\"unknown\""
        );
    }
}
```

Create `crates/rocket-network/src/dialer.rs` with the tests only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn low() -> DeviceId {
        DeviceId::from_bytes([1; 32])
    }

    fn high() -> DeviceId {
        DeviceId::from_bytes([2; 32])
    }

    #[test]
    fn the_lower_id_dials_an_mdns_peer_at_once() {
        assert!(should_dial(&low(), &high(), DialHint::Mdns, Duration::ZERO));
    }

    #[test]
    fn the_higher_id_waits_for_the_grace_period() {
        assert!(!should_dial(&high(), &low(), DialHint::Mdns, Duration::ZERO));
        assert!(!should_dial(
            &high(),
            &low(),
            DialHint::Mdns,
            DIAL_GRACE - Duration::from_millis(1)
        ));
        assert!(should_dial(&high(), &low(), DialHint::Mdns, DIAL_GRACE));
        assert!(should_dial(
            &high(),
            &low(),
            DialHint::Mdns,
            DIAL_GRACE + Duration::from_secs(5)
        ));
    }

    #[test]
    fn a_manual_address_always_dials() {
        assert!(should_dial(&high(), &low(), DialHint::ManualAddress, Duration::ZERO));
        assert!(should_dial(&low(), &high(), DialHint::ManualAddress, Duration::ZERO));
    }

    #[test]
    fn the_same_id_on_both_sides_waits_for_the_grace_period() {
        assert!(!should_dial(&low(), &low(), DialHint::Mdns, Duration::ZERO));
    }

    #[test]
    fn keep_dialed_by_picks_the_lower_id_from_either_side() {
        assert_eq!(keep_dialed_by(&low(), &high()), low());
        assert_eq!(keep_dialed_by(&high(), &low()), low());
        assert_eq!(keep_dialed_by(&low(), &low()), low());
    }
}
```

Create `crates/rocket-network/src/rate.rs` with the tests only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn hits_up_to_the_cap_pass_and_the_next_one_is_refused() {
        let t0 = Instant::now();
        let mut window = SlidingWindow::new(3, secs(60));
        assert!(window.try_hit(t0));
        assert!(window.try_hit(t0));
        assert!(window.try_hit(t0 + secs(1)));
        assert!(!window.try_hit(t0 + secs(2)), "the fourth hit in the window");
    }

    #[test]
    fn the_window_slides_as_old_hits_age_out() {
        let t0 = Instant::now();
        let mut window = SlidingWindow::new(2, secs(60));
        assert!(window.try_hit(t0));
        assert!(window.try_hit(t0 + secs(30)));
        assert!(!window.try_hit(t0 + secs(59)));
        assert!(window.try_hit(t0 + secs(60)), "the first hit has aged out");
        assert!(!window.try_hit(t0 + secs(61)), "the hits at 30 s and 60 s remain");
        assert!(window.try_hit(t0 + secs(90)), "the hit at 30 s has aged out");
    }

    #[test]
    fn a_hit_exactly_one_window_old_is_dropped() {
        let t0 = Instant::now();
        let mut window = SlidingWindow::new(1, secs(60));
        assert!(window.try_hit(t0));
        assert!(!window.try_hit(t0 + secs(60) - Duration::from_millis(1)));
        assert!(window.try_hit(t0 + secs(60)));
    }

    #[test]
    fn a_refused_hit_is_not_recorded() {
        let t0 = Instant::now();
        let mut window = SlidingWindow::new(1, secs(60));
        assert!(window.try_hit(t0));
        assert!(!window.try_hit(t0 + secs(30)));
        assert!(window.try_hit(t0 + secs(60)), "only the first hit counted");
    }

    #[test]
    fn a_cap_of_zero_refuses_everything() {
        let mut window = SlidingWindow::new(0, secs(60));
        assert!(!window.try_hit(Instant::now()));
    }

    #[test]
    fn a_clock_that_goes_back_keeps_the_hit_in_the_window() {
        let t0 = Instant::now();
        let mut window = SlidingWindow::new(1, secs(60));
        assert!(window.try_hit(t0 + secs(10)));
        assert!(!window.try_hit(t0), "an earlier `now` does not panic or free the slot");
    }

    #[test]
    fn clear_forgets_every_hit() {
        let t0 = Instant::now();
        let mut window = SlidingWindow::new(1, secs(60));
        assert!(window.try_hit(t0));
        window.clear();
        assert!(window.try_hit(t0));
    }
}
```

Create `crates/rocket-network/src/wire.rs` with the tests only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn profile() -> NodeProfile {
        NodeProfile::new("Ada Lovelace").expect("valid name")
    }

    #[test]
    fn hello_response_is_snake_case_json() {
        let hello = HelloResponse {
            device_id: DeviceId::from_bytes([1; 32]),
            version: 1,
            profile: profile(),
        };
        let value = serde_json::to_value(&hello).expect("serializes");
        assert_eq!(
            value,
            json!({
                "device_id": DeviceId::from_bytes([1; 32]).to_base32(),
                "version": 1,
                "profile": {"display_name": "Ada Lovelace", "initials": "AL"}
            })
        );
        let back: HelloResponse = serde_json::from_value(value).expect("deserializes");
        assert_eq!(back, hello);
    }

    #[test]
    fn pair_request_round_trips() {
        let request = PairRequest {
            version: 1,
            profile: profile(),
        };
        let text = serde_json::to_string(&request).expect("serializes");
        let back: PairRequest = serde_json::from_str(&text).expect("deserializes");
        assert_eq!(back, request);
    }

    #[test]
    fn pair_request_response_keys_are_stable() {
        let response = PairRequestResponse {
            pairing_id: PairingId("8d2b0c5e-0000-4000-8000-000000000001".to_string()),
            commitment: "AAAA".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&response).expect("serializes"),
            json!({"pairing_id": "8d2b0c5e-0000-4000-8000-000000000001", "commitment": "AAAA"})
        );
    }

    #[test]
    fn nonce_message_and_decision_keys_are_stable() {
        assert_eq!(
            serde_json::to_value(NonceMessage {
                nonce: "abc".to_string()
            })
            .expect("serializes"),
            json!({"nonce": "abc"})
        );
        assert_eq!(
            serde_json::to_value(DecisionRequest { accept: true }).expect("serializes"),
            json!({"accept": true})
        );
    }

    #[test]
    fn pair_status_response_uses_snake_case_status() {
        let response = PairStatusResponse {
            status: PairingStatus::Declined,
        };
        assert_eq!(
            serde_json::to_value(&response).expect("serializes"),
            json!({"status": "declined"})
        );
        let back: PairStatusResponse =
            serde_json::from_str(r#"{"status":"pending"}"#).expect("deserializes");
        assert_eq!(back.status, PairingStatus::Pending);
    }

    #[test]
    fn a_wrong_type_is_rejected() {
        assert!(serde_json::from_str::<DecisionRequest>(r#"{"accept":"yes"}"#).is_err());
        assert!(serde_json::from_str::<PairRequest>(r#"{"version":"1"}"#).is_err());
    }
}
```

Create `crates/rocket-network/src/messages.rs` with the tests only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::DISPLAY_NAME_MAX_CHARS;
    use serde_json::json;

    fn hello() -> PeerMessage {
        PeerMessage::Hello {
            version: 1,
            profile: NodeProfile::new("Ada Lovelace").expect("valid name"),
        }
    }

    #[test]
    fn every_variant_round_trips() {
        for message in [hello(), PeerMessage::Ping, PeerMessage::Pong, PeerMessage::Unpair] {
            let text = message.encode();
            assert_eq!(PeerMessage::decode(&text).expect("decodes"), message);
        }
    }

    #[test]
    fn encoded_frames_have_the_expected_shape() {
        assert_eq!(PeerMessage::Ping.encode(), r#"{"type":"ping"}"#);
        assert_eq!(PeerMessage::Pong.encode(), r#"{"type":"pong"}"#);
        assert_eq!(PeerMessage::Unpair.encode(), r#"{"type":"unpair"}"#);
        let value: serde_json::Value = serde_json::from_str(&hello().encode()).expect("json");
        assert_eq!(
            value,
            json!({
                "type": "hello",
                "version": 1,
                "profile": {"display_name": "Ada Lovelace", "initials": "AL"}
            })
        );
    }

    #[test]
    fn decode_rejects_unknown_type_without_echoing_it() {
        for text in [
            r#"{"type":"send_collection"}"#,
            r#"{"type":"secret-token-123"}"#,
            r#"{"type":""}"#,
        ] {
            let err = PeerMessage::decode(text).expect_err("unknown type");
            match err {
                DomainError::InvalidInput(message) => {
                    assert!(!message.contains("secret-token"));
                    assert!(!message.contains("send_collection"));
                }
                other => panic!("expected InvalidInput, got {other:?}"),
            }
        }
    }

    #[test]
    fn decode_rejects_malformed_frames() {
        for text in [
            "",
            "not json",
            "[]",
            "42",
            "null",
            "{}",
            r#"{"kind":"ping"}"#,
            r#"{"type":"hello"}"#,
            r#"{"type":"hello","version":"1","profile":{"display_name":"A","initials":"A"}}"#,
        ] {
            let err = PeerMessage::decode(text).expect_err("rejected");
            assert!(matches!(err, DomainError::InvalidInput(_)), "input {text:?}");
        }
    }

    #[test]
    fn decode_rejects_oversize_frame_and_accepts_the_limit() {
        let base = r#"{"type":"ping"}"#;
        let at_limit = format!("{base}{}", " ".repeat(MAX_FRAME_BYTES - base.len()));
        assert_eq!(at_limit.len(), MAX_FRAME_BYTES);
        assert_eq!(
            PeerMessage::decode(&at_limit).expect("limit is allowed"),
            PeerMessage::Ping
        );

        let over = format!("{at_limit} ");
        assert_eq!(over.len(), MAX_FRAME_BYTES + 1);
        let err = PeerMessage::decode(&over).expect_err("too large");
        assert!(matches!(err, DomainError::InvalidInput(_)));

        let huge = "x".repeat(MAX_FRAME_BYTES * 4);
        assert!(PeerMessage::decode(&huge).is_err());
    }

    #[test]
    fn decode_normalizes_hello_profile() {
        let forged = r#"{"type":"hello","version":1,"profile":{"display_name":"Ada Lovelace","initials":"ZZZZZZ"}}"#;
        match PeerMessage::decode(forged).expect("decodes") {
            PeerMessage::Hello { profile, .. } => assert_eq!(profile.initials, "AL"),
            other => panic!("expected Hello, got {other:?}"),
        }
    }

    #[test]
    fn decode_rejects_a_hello_with_a_bad_name() {
        let control = r#"{"type":"hello","version":1,"profile":{"display_name":"Ada\u001b[31m","initials":"A"}}"#;
        assert!(PeerMessage::decode(control).is_err());
        let empty = r#"{"type":"hello","version":1,"profile":{"display_name":"  ","initials":""}}"#;
        assert!(PeerMessage::decode(empty).is_err());
        let long = format!(
            r#"{{"type":"hello","version":1,"profile":{{"display_name":"{}","initials":"A"}}}}"#,
            "a".repeat(DISPLAY_NAME_MAX_CHARS + 1)
        );
        assert!(PeerMessage::decode(&long).is_err());
    }

    #[test]
    fn hello_with_an_unknown_field_is_rejected() {
        let text = r#"{"type":"hello","version":1,"profile":{"display_name":"Ada","initials":"A"},"extra":1}"#;
        assert!(PeerMessage::decode(text).is_err());
    }
}
```

Create `crates/rocket-network/src/events.rs` with the tests only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cases() -> Vec<(NetworkEvent, &'static str, &'static [&'static str])> {
        vec![
            (NetworkEvent::NodesChanged, "network-changed", &["type"][..]),
            (NetworkEvent::StatusChanged, "network-changed", &["type"][..]),
            (
                NetworkEvent::PairedRemoved {
                    device_id: "ID".into(),
                    display_name: "Ada".into(),
                    by_peer: true,
                },
                "network-changed",
                &["type", "device_id", "display_name", "by_peer"][..],
            ),
            (
                NetworkEvent::PairingIncoming {
                    pairing_id: "P".into(),
                    device_id: "ID".into(),
                    short_id: "ABCD-EFGH".into(),
                    display_name: "Ada".into(),
                    code: "048213".into(),
                },
                "network-pairing",
                &["type", "pairing_id", "device_id", "short_id", "display_name", "code"][..],
            ),
            (
                NetworkEvent::PairingFinished {
                    pairing_id: "P".into(),
                    outcome: "accepted".into(),
                    display_name: "Ada".into(),
                },
                "network-pairing",
                &["type", "pairing_id", "outcome", "display_name"][..],
            ),
        ]
    }

    fn sorted_keys(event: &NetworkEvent) -> Vec<String> {
        let value = event.to_json().expect("serializes");
        let mut keys: Vec<String> = value
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    #[test]
    fn channels_are_pinned() {
        for (event, channel, _) in cases() {
            assert_eq!(event.channel(), channel, "event {event:?}");
        }
    }

    // Wire contract: tag is camelCase, fields stay snake_case. A rename here
    // breaks the frontend listeners and needs its own decision.
    #[test]
    fn wire_contract_network_events() {
        for (event, _, expected) in cases() {
            let mut want: Vec<String> = expected.iter().map(|k| k.to_string()).collect();
            want.sort();
            assert_eq!(sorted_keys(&event), want, "event {event:?}");
        }
    }

    #[test]
    fn wire_contract_tags_are_camel_case() {
        let tags: Vec<String> = cases()
            .iter()
            .map(|(event, _, _)| {
                event.to_json().expect("serializes")["type"]
                    .as_str()
                    .expect("tag")
                    .to_string()
            })
            .collect();
        assert_eq!(
            tags,
            [
                "nodesChanged",
                "statusChanged",
                "pairedRemoved",
                "pairingIncoming",
                "pairingFinished"
            ]
        );
    }

    // The envelope keys the bus adds must not collide with a top-level field.
    #[test]
    fn wire_contract_envelope_keys_are_free() {
        for (event, _, _) in cases() {
            let keys = sorted_keys(&event);
            for reserved in ["seq", "tsMs", "ts_ms", "workspaceId"] {
                assert!(
                    !keys.iter().any(|k| k == reserved),
                    "event {event:?} uses {reserved}"
                );
            }
        }
    }

    #[test]
    fn pairing_incoming_json_carries_the_values() {
        let event = NetworkEvent::PairingIncoming {
            pairing_id: "P1".into(),
            device_id: "DEV".into(),
            short_id: "ABCD-EFGH".into(),
            display_name: "Ada".into(),
            code: "048213".into(),
        };
        assert_eq!(
            event.to_json().expect("serializes"),
            json!({
                "type": "pairingIncoming",
                "pairing_id": "P1",
                "device_id": "DEV",
                "short_id": "ABCD-EFGH",
                "display_name": "Ada",
                "code": "048213"
            })
        );
    }

    #[test]
    fn events_have_no_extra_channels_or_written_collections() {
        for (event, _, _) in cases() {
            assert!(event.also_emit_on().is_empty());
            assert!(event.written_collections().is_empty());
        }
    }

    #[test]
    fn events_can_be_downcast_from_the_trait_object() {
        let event: Box<dyn PublishedEvent> = Box::new(NetworkEvent::NodesChanged);
        assert_eq!(
            event.as_any().downcast_ref::<NetworkEvent>(),
            Some(&NetworkEvent::NodesChanged)
        );
    }
}
```

Create `crates/rocket-network/src/ports.rs` with the tests only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn assert_dyn<T: ?Sized>() {}

    #[test]
    fn every_port_is_object_safe() {
        assert_dyn::<dyn DeviceIdentityStore>();
        assert_dyn::<dyn PairedDeviceRepository>();
        assert_dyn::<dyn BlockedDeviceRepository>();
        assert_dyn::<dyn ManualAddressRepository>();
        assert_dyn::<dyn NetworkSettingsRepository>();
        assert_dyn::<dyn QuarantineReport>();
        assert_dyn::<dyn DiscoverySink>();
        assert_dyn::<dyn Discovery>();
        assert_dyn::<dyn PeerHandler>();
        assert_dyn::<dyn PeerConnector>();
        assert_dyn::<dyn PeerServerControl>();
        assert_dyn::<dyn PeerTransportFactory>();
    }

    #[test]
    fn peer_errors_map_to_http_status_codes() {
        assert_eq!(PeerError::NotFound.status_code(), 404);
        assert_eq!(PeerError::NotPaired.status_code(), 403);
        assert_eq!(PeerError::Busy.status_code(), 503);
        assert_eq!(PeerError::Forbidden.status_code(), 403);
        assert_eq!(PeerError::BadRequest.status_code(), 400);
        assert_eq!(PeerError::Internal.status_code(), 500);
    }

    struct Collecting(Mutex<Vec<DiscoveryEvent>>);

    impl DiscoverySink for Collecting {
        fn on_event(&self, event: DiscoveryEvent) {
            self.0.lock().unwrap_or_else(|e| e.into_inner()).push(event);
        }
    }

    #[test]
    fn a_sink_receives_events_through_the_trait_object() {
        let sink = Arc::new(Collecting(Mutex::new(Vec::new())));
        let dynamic: Arc<dyn DiscoverySink> = sink.clone();
        let id = DeviceId::from_bytes([1; 32]);
        dynamic.on_event(DiscoveryEvent::Gone { claimed_id: id });
        dynamic.on_event(DiscoveryEvent::Seen {
            claimed_id: id,
            profile: NodeProfile::new("Ada").expect("valid"),
            addresses: vec!["127.0.0.1:47821".parse().expect("addr")],
            version: 1,
        });
        let seen = sink.0.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0], DiscoveryEvent::Gone { claimed_id: id });
    }

    #[test]
    fn announcements_compare_by_value() {
        let a = Announcement {
            device_id: DeviceId::from_bytes([1; 32]),
            profile: NodeProfile::new("Ada").expect("valid"),
            port: 47821,
            version: 1,
        };
        assert_eq!(a.clone(), a);
    }

    #[tokio::test]
    async fn peer_socket_channels_carry_messages_both_ways() {
        let (out_tx, mut out_rx) = mpsc::channel::<PeerMessage>(4);
        let (in_tx, in_rx) = mpsc::channel::<PeerMessage>(4);
        let mut socket = PeerSocket {
            peer: PeerIdentity::from_spki(vec![1; 44]),
            dialed_by: DeviceId::from_bytes([1; 32]),
            remote_hello: PeerMessage::Hello {
                version: 1,
                profile: NodeProfile::new("Ada").expect("valid"),
            },
            outbound: out_tx,
            inbound: in_rx,
        };

        socket
            .outbound
            .send(PeerMessage::Ping)
            .await
            .expect("outbound open");
        assert_eq!(out_rx.recv().await, Some(PeerMessage::Ping));

        in_tx.send(PeerMessage::Pong).await.expect("inbound open");
        assert_eq!(socket.inbound.recv().await, Some(PeerMessage::Pong));

        drop(in_tx);
        assert_eq!(socket.inbound.recv().await, None);
    }

    #[test]
    fn peer_call_errors_compare_by_value() {
        assert_eq!(PeerCallError::Rejected(403), PeerCallError::Rejected(403));
        assert_ne!(PeerCallError::NotPaired, PeerCallError::Busy);
        assert_ne!(
            PeerCallError::Unreachable("a".to_string()),
            PeerCallError::Unreachable("b".to_string())
        );
    }
}
```

- [ ] **Step 2: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket-network`
Expected: FAIL to compile. Typical errors: `cannot find type PresenceTracker`, `cannot find function should_dial`, `cannot find type SlidingWindow`, `cannot find type HelloResponse`, `cannot find type PeerMessage`, `cannot find type NetworkEvent`, `cannot find trait DiscoverySink`, and unresolved imports in `lib.rs`.

- [ ] **Step 3: Implement `presence.rs`**

Add this above the `#[cfg(test)]` line in `crates/rocket-network/src/presence.rs`:

```rust
//! Presence bookkeeping and the reconnect backoff. Pure; the caller owns the timers.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::limits::{BACKOFF_MAX, MISSED_PINGS_OFFLINE};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Presence {
    Unknown,
    Online,
    Offline,
}

/// Tracks one peer WebSocket: the state, the missed pings and the reconnect attempt.
#[derive(Debug, Clone)]
pub struct PresenceTracker {
    state: Presence,
    attempt: u32,
    missed: u32,
}

impl PresenceTracker {
    pub fn new() -> PresenceTracker {
        PresenceTracker {
            state: Presence::Unknown,
            attempt: 0,
            missed: 0,
        }
    }

    pub fn state(&self) -> Presence {
        self.state
    }

    /// The socket is open.
    pub fn on_connected(&mut self) {
        self.state = Presence::Online;
        self.attempt = 0;
        self.missed = 0;
    }

    /// A pong (or any frame the caller counts as alive) arrived.
    pub fn on_pong(&mut self) {
        self.missed = 0;
    }

    /// A ping was sent. More unanswered pings than `MISSED_PINGS_OFFLINE` means offline.
    pub fn on_ping_sent(&mut self) -> Presence {
        self.missed = self.missed.saturating_add(1);
        if self.state == Presence::Online && self.missed > MISSED_PINGS_OFFLINE {
            self.state = Presence::Offline;
        }
        self.state
    }

    /// The socket closed or a dial failed.
    pub fn on_disconnected(&mut self) {
        self.state = Presence::Offline;
        self.attempt = self.attempt.saturating_add(1);
        self.missed = 0;
    }

    pub fn next_retry_delay(&self) -> Duration {
        backoff(self.attempt)
    }
}

impl Default for PresenceTracker {
    fn default() -> Self {
        PresenceTracker::new()
    }
}

/// Reconnect delay: attempts 0 and 1 wait 1 s, then it doubles up to `BACKOFF_MAX`.
pub fn backoff(attempt: u32) -> Duration {
    let exponent = attempt.saturating_sub(1).min(10);
    Duration::from_secs(1u64 << exponent).min(BACKOFF_MAX)
}
```

- [ ] **Step 4: Implement `dialer.rs` and `rate.rs`**

Add this above the `#[cfg(test)]` line in `crates/rocket-network/src/dialer.rs`:

```rust
//! Which side opens the peer WebSocket.

use std::time::Duration;

use crate::device::DeviceId;
use crate::limits::DIAL_GRACE;

/// How the local node learned about the remote one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialHint {
    Mdns,
    ManualAddress,
}

/// Decides whether the local node should dial now.
///
/// A manual address always dials, because only this side knows the address.
/// For mDNS the lower device id dials. The higher id dials too once `waited`
/// reaches `DIAL_GRACE`, which covers a lower side that cannot dial out.
pub fn should_dial(local: &DeviceId, remote: &DeviceId, hint: DialHint, waited: Duration) -> bool {
    match hint {
        DialHint::ManualAddress => true,
        DialHint::Mdns => local < remote || waited >= DIAL_GRACE,
    }
}

/// When two connections exist for one pair, keep the one dialled by this id.
pub fn keep_dialed_by(local: &DeviceId, remote: &DeviceId) -> DeviceId {
    *std::cmp::min(local, remote)
}
```

Add this above the `#[cfg(test)]` line in `crates/rocket-network/src/rate.rs`:

```rust
//! A sliding-window counter for request limits. Pure; the caller passes the time.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Allows at most `cap` hits within any `window`.
#[derive(Debug, Clone)]
pub struct SlidingWindow {
    cap: usize,
    window: Duration,
    hits: VecDeque<Instant>,
}

impl SlidingWindow {
    pub fn new(cap: usize, window: Duration) -> SlidingWindow {
        SlidingWindow {
            cap,
            window,
            hits: VecDeque::new(),
        }
    }

    /// Records a hit at `now` and returns true, or returns false when `cap` hits are already
    /// inside the window. A refused hit is not recorded.
    pub fn try_hit(&mut self, now: Instant) -> bool {
        while self
            .hits
            .front()
            .is_some_and(|t| now.saturating_duration_since(*t) >= self.window)
        {
            self.hits.pop_front();
        }
        if self.hits.len() >= self.cap {
            return false;
        }
        self.hits.push_back(now);
        true
    }

    /// Forgets every hit.
    pub fn clear(&mut self) {
        self.hits.clear();
    }
}
```

- [ ] **Step 5: Implement `wire.rs`**

Add this above the `#[cfg(test)]` line in `crates/rocket-network/src/wire.rs`:

```rust
//! REST bodies between peers. snake_case JSON. Not IPC DTOs.

use serde::{Deserialize, Serialize};

use crate::device::DeviceId;
use crate::pairing::{PairingId, PairingStatus};
use crate::profile::NodeProfile;

/// Reply of `GET /hello`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloResponse {
    pub device_id: DeviceId,
    pub version: u32,
    pub profile: NodeProfile,
}

/// Body of `POST /pair/request`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairRequest {
    pub version: u32,
    pub profile: NodeProfile,
}

/// Reply of `POST /pair/request`. `commitment` is base64.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairRequestResponse {
    pub pairing_id: PairingId,
    pub commitment: String,
}

/// Body and reply of `POST /pair/{id}/nonce`. `nonce` is base64.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonceMessage {
    pub nonce: String,
}

/// Body of `POST /pair/{id}/decision`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionRequest {
    pub accept: bool,
}

/// Reply of `GET /pair/{id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairStatusResponse {
    pub status: PairingStatus,
}
```

- [ ] **Step 6: Implement `messages.rs`**

Add this above the `#[cfg(test)]` line in `crates/rocket-network/src/messages.rs`:

```rust
//! The WebSocket frame type. One serde enum tagged by `type`.

use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

use crate::limits::MAX_FRAME_BYTES;
use crate::profile::NodeProfile;

/// A text frame on the peer WebSocket. Later projects add variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PeerMessage {
    Hello { version: u32, profile: NodeProfile },
    Ping,
    Pong,
    Unpair,
}

impl PeerMessage {
    /// The frame as JSON text.
    pub fn encode(&self) -> String {
        // Serializing this enum cannot fail. The fallback is a frame the peer rejects.
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Parses a text frame.
    ///
    /// A text above `MAX_FRAME_BYTES` is refused before parsing. Unknown types
    /// and malformed JSON are `InvalidInput`, and the error never repeats the
    /// frame. A `Hello` profile is re-validated and its initials rebuilt.
    pub fn decode(text: &str) -> DomainResult<PeerMessage> {
        if text.len() > MAX_FRAME_BYTES {
            return Err(DomainError::InvalidInput("frame too large".to_string()));
        }
        let message: PeerMessage = serde_json::from_str(text)
            .map_err(|_| DomainError::InvalidInput("invalid peer message".to_string()))?;
        match message {
            PeerMessage::Hello { version, profile } => Ok(PeerMessage::Hello {
                version,
                profile: profile.validated()?,
            }),
            other => Ok(other),
        }
    }
}
```

- [ ] **Step 7: Implement `events.rs`**

Add this above the `#[cfg(test)]` line in `crates/rocket-network/src/events.rs`:

```rust
//! Events the network context publishes.
//!
//! The tag is camelCase and the variant fields stay snake_case on purpose.
//! Do not rename them: the frontend listeners read these keys.

use rocket_shared::published_event::PublishedEvent;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum NetworkEvent {
    /// The node list changed. The frontend refetches it.
    NodesChanged,
    /// The network status changed. The frontend refetches it.
    StatusChanged,
    /// A pairing was removed here or by the peer.
    PairedRemoved {
        device_id: String,
        display_name: String,
        by_peer: bool,
    },
    /// A stranger asked to pair and the code is ready. `code` is six digits.
    PairingIncoming {
        pairing_id: String,
        device_id: String,
        short_id: String,
        display_name: String,
        code: String,
    },
    /// A pairing ended. `outcome` is `PairingOutcome::as_str()`.
    PairingFinished {
        pairing_id: String,
        outcome: String,
        display_name: String,
    },
}

impl PublishedEvent for NetworkEvent {
    fn channel(&self) -> &'static str {
        match self {
            NetworkEvent::NodesChanged
            | NetworkEvent::StatusChanged
            | NetworkEvent::PairedRemoved { .. } => "network-changed",
            NetworkEvent::PairingIncoming { .. } | NetworkEvent::PairingFinished { .. } => {
                "network-pairing"
            }
        }
    }

    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
```

- [ ] **Step 8: Implement `ports.rs`**

Add this above the `#[cfg(test)]` line in `crates/rocket-network/src/ports.rs`:

```rust
//! Ports: the traits that infra implements and the service consumes.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use rocket_shared::error::DomainResult;
use tokio::sync::mpsc;

use crate::device::DeviceId;
use crate::identity::{DeviceKeyMaterial, PeerIdentity};
use crate::messages::PeerMessage;
use crate::pairing::PairingId;
use crate::profile::NodeProfile;
use crate::records::{BlockedDevice, ManualAddress, NetworkSettings, PairedDevice};
use crate::wire::{
    DecisionRequest, HelloResponse, NonceMessage, PairRequest, PairRequestResponse,
    PairStatusResponse,
};

pub trait DeviceIdentityStore: Send + Sync {
    /// Loads the device key or creates it. Errors when the keychain is
    /// unavailable. It never falls back to a file.
    fn load_or_create(&self) -> DomainResult<DeviceKeyMaterial>;
}

pub trait PairedDeviceRepository: Send + Sync {
    fn list(&self) -> DomainResult<Vec<PairedDevice>>;
    fn get(&self, id: &DeviceId) -> DomainResult<Option<PairedDevice>>;
    /// `InvalidInput` when a new entry would exceed `MAX_PAIRED_DEVICES`.
    fn upsert(&self, device: PairedDevice) -> DomainResult<()>;
    fn remove(&self, id: &DeviceId) -> DomainResult<bool>;
}

pub trait BlockedDeviceRepository: Send + Sync {
    fn list(&self) -> DomainResult<Vec<BlockedDevice>>;
    fn contains(&self, id: &DeviceId) -> DomainResult<bool>;
    fn add(&self, device: BlockedDevice) -> DomainResult<()>;
    fn remove(&self, id: &DeviceId) -> DomainResult<bool>;
}

pub trait ManualAddressRepository: Send + Sync {
    fn list(&self) -> DomainResult<Vec<ManualAddress>>;
    /// Dedupes on (host lowercased, port). `InvalidInput` over `MAX_MANUAL_ADDRESSES`.
    fn add(&self, address: ManualAddress) -> DomainResult<()>;
    fn remove(&self, host: &str, port: u16) -> DomainResult<bool>;
}

pub trait NetworkSettingsRepository: Send + Sync {
    fn load(&self) -> DomainResult<NetworkSettings>;
    fn save(&self, settings: &NetworkSettings) -> DomainResult<()>;
}

/// Each Fs repo also implements this, so the service can warn about a quarantined file once.
pub trait QuarantineReport: Send + Sync {
    /// The name of a file that was set aside, once. `None` afterwards.
    fn take_quarantined(&self) -> Option<String>;
}

/// What the local node announces on the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announcement {
    pub device_id: DeviceId,
    pub profile: NodeProfile,
    pub port: u16,
    pub version: u32,
}

/// What discovery reports. The id is only claimed until `/hello` confirms it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryEvent {
    Seen {
        claimed_id: DeviceId,
        profile: NodeProfile,
        addresses: Vec<SocketAddr>,
        version: u32,
    },
    Gone {
        claimed_id: DeviceId,
    },
}

pub trait DiscoverySink: Send + Sync {
    fn on_event(&self, event: DiscoveryEvent);
}

pub trait Discovery: Send + Sync {
    /// Starts browsing. An error means "discovery unavailable".
    fn start(&self, sink: Arc<dyn DiscoverySink>) -> DomainResult<()>;
    fn announce(&self, announcement: &Announcement) -> DomainResult<()>;
    fn withdraw(&self) -> DomainResult<()>;
    fn stop(&self);
}

/// Why an outgoing call to a peer failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerCallError {
    Unreachable(String),
    NotPaired,
    Busy,
    Rejected(u16),
    IdentityMismatch,
    Protocol(String),
}

/// Why the local handler refuses a peer's request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerError {
    NotFound,
    NotPaired,
    Busy,
    Forbidden,
    BadRequest,
    Internal,
}

impl PeerError {
    /// The HTTP status the server sends: 404, 403, 503, 403, 400, 500.
    pub fn status_code(&self) -> u16 {
        match self {
            PeerError::NotFound => 404,
            PeerError::NotPaired => 403,
            PeerError::Busy => 503,
            PeerError::Forbidden => 403,
            PeerError::BadRequest => 400,
            PeerError::Internal => 500,
        }
    }
}

/// An open peer WebSocket, after both Hello frames were exchanged.
pub struct PeerSocket {
    pub peer: PeerIdentity,
    pub dialed_by: DeviceId,
    /// The peer's Hello frame.
    pub remote_hello: PeerMessage,
    pub outbound: mpsc::Sender<PeerMessage>,
    /// Closes when the socket closes.
    pub inbound: mpsc::Receiver<PeerMessage>,
}

/// What the peer server calls. Implemented by the service.
#[async_trait]
pub trait PeerHandler: Send + Sync {
    async fn hello(&self) -> HelloResponse;
    async fn pair_request(
        &self,
        peer: PeerIdentity,
        remote_ip: IpAddr,
        req: PairRequest,
    ) -> Result<PairRequestResponse, PeerError>;
    async fn pair_nonce(
        &self,
        peer: PeerIdentity,
        id: PairingId,
        msg: NonceMessage,
    ) -> Result<NonceMessage, PeerError>;
    async fn pair_decision(
        &self,
        peer: PeerIdentity,
        id: PairingId,
        req: DecisionRequest,
    ) -> Result<(), PeerError>;
    async fn pair_status(
        &self,
        peer: PeerIdentity,
        id: PairingId,
    ) -> Result<PairStatusResponse, PeerError>;
    /// True when the device is paired and not blocked.
    async fn is_paired(&self, device: &DeviceId) -> bool;
    /// Takes ownership of an open socket, after the Hello frames were exchanged.
    async fn on_peer_socket(&self, socket: PeerSocket);
    /// The `PeerMessage::Hello` for the first frame.
    fn local_hello(&self) -> PeerMessage;
}

/// What the service calls to reach a peer. Implemented by infra.
#[async_trait]
pub trait PeerConnector: Send + Sync {
    async fn hello(
        &self,
        addr: SocketAddr,
    ) -> Result<(PeerIdentity, HelloResponse), PeerCallError>;
    async fn pair_request(
        &self,
        addr: SocketAddr,
        expected: &DeviceId,
        req: &PairRequest,
    ) -> Result<(PeerIdentity, PairRequestResponse), PeerCallError>;
    async fn pair_nonce(
        &self,
        addr: SocketAddr,
        expected: &DeviceId,
        id: &PairingId,
        msg: &NonceMessage,
    ) -> Result<NonceMessage, PeerCallError>;
    async fn pair_decision(
        &self,
        addr: SocketAddr,
        expected: &DeviceId,
        id: &PairingId,
        req: &DecisionRequest,
    ) -> Result<(), PeerCallError>;
    async fn pair_status(
        &self,
        addr: SocketAddr,
        expected: &DeviceId,
        id: &PairingId,
    ) -> Result<PairStatusResponse, PeerCallError>;
    async fn open_socket(
        &self,
        addr: SocketAddr,
        expected: &DeviceId,
        hello: PeerMessage,
    ) -> Result<PeerSocket, PeerCallError>;
}

#[async_trait]
pub trait PeerServerControl: Send + Sync {
    /// Binds `0.0.0.0` on `DEFAULT_PORT..=PORT_RANGE_END` and returns the port.
    /// Errors when no port in the range is free.
    async fn start(&self, handler: Arc<dyn PeerHandler>) -> DomainResult<u16>;
    fn stop(&self);
}

/// The pieces the service needs once the device key is known.
pub struct PeerTransport {
    pub local: PeerIdentity,
    pub connector: Arc<dyn PeerConnector>,
    pub server: Arc<dyn PeerServerControl>,
}

pub trait PeerTransportFactory: Send + Sync {
    fn build(&self, key: &DeviceKeyMaterial) -> DomainResult<PeerTransport>;
}
```

- [ ] **Step 9: Run the tests and see them pass**

Run: `cargo test -j4 -p rocket-network`
Expected: PASS. All tests in all modules pass, 0 failed.

If `hello_with_an_unknown_field_is_rejected` fails, serde did not apply `deny_unknown_fields` to the `Hello` struct variant. Do not drop `deny_unknown_fields` from the enum (the index requires it). Instead, in `decode`, parse into `serde_json::Value` first, reject a `Hello` object that has keys other than `type`, `version` and `profile`, then call `serde_json::from_value`. Keep every other test unchanged.

- [ ] **Step 10: Check and lint**

Run: `cargo check -j4 -p rocket-network`
Expected: no errors, no warnings.

Run: `cargo clippy -j4 -p rocket-network -- -D warnings`
Expected: no findings in `rocket-network`.

Run: `cargo check -j4 --workspace`
Expected: PASS. Nothing else uses the new crate yet, so nothing else changes.

- [ ] **Step 11: Stop**

No commit. Changes are reviewed at the plan end.

---

**End of Plan 1.** Verify:

```bash
cargo test -j4 -p rocket-network
cargo check -j4 -p rocket-network
cargo clippy -j4 -p rocket-network -- -D warnings
cargo check -j4 --workspace
```

Then stop and show the user the whole change: root `Cargo.toml`, root `CLAUDE.md`, and `crates/rocket-network/`. After the user's OK, commit through `dev-workflow-skills:1-git-commit`, for example in three commits: scaffold with limits, device, profile, identity and records; pairing; presence, dialer, wire, messages, events and ports. Stage explicit paths only.

**Next plan to run: `docs/superpowers/plans/team-network/2026-11-06-team-network-plan-02-identity-storage-tls.md`**

## Plan 01 Self-Review

**Spec coverage**

| Spec section | Where in this plan |
|---|---|
| 5.1 `DeviceId`, short id | Task 1 `device.rs` |
| 5.1 `NodeProfile` | Task 1 `profile.rs` |
| 5.1 `DiscoveredNode`, `PairedDevice` | Task 1 `records.rs` |
| 5.1 `PairingSession`, commitment, pairing code | Task 2 `pairing.rs` |
| 5.1 `Presence`, `PeerMessage`, `dialer` | Task 3 `presence.rs`, `messages.rs`, `dialer.rs` |
| 5.1 Ports | Task 3 `ports.rs` |
| 5.1 `NetworkEvent` with `wire_contract_*` tests | Task 3 `events.rs` |
| 6.2 steps 1 to 6, message shapes | Task 2 (crypto and state), Task 3 `wire.rs` (bodies) |
| 6.3 states, 2 minute expiry | Task 2 (`PAIRING_TTL`, `expire_if_due`). The spec's `Requested`, `Revealed`, `LocalAccepted` and `RemoteAccepted` are folded into `Committed` and `CodeShown` plus two decision flags, as the index defines. |
| 6.4 node in the middle | Task 2 `a_man_in_the_middle_sees_different_codes` |
| 7.3 who dials, backoff, ping | Task 3 `dialer.rs`, `presence.rs`, `limits.rs` |
| 8.3 pairing requests 5 per minute (the counter) | Task 3 `rate.rs` (`SlidingWindow`), used by Plan 04 |
| 8.1 frame limit and variants as allow-list | Task 3 `PeerMessage::decode` |
| 8.3 limits | Task 1 `limits.rs` (constants only; Plan 03 enforces them) |
| 9 paired list full (200) | Task 1 `MAX_PAIRED_DEVICES`, Task 3 `PairedDeviceRepository::upsert` doc |
| 10 `rocket-network` unit tests | All three tasks. Leading zeros, key swap, commitment check, backoff sequence, round trips, unknown variants, `wire_contract_*`. |

Spec section 8.2 (TLS), 8.4 to 8.5 (first launch, audit), 7.1, 7.2, 7.4, 7.5 (mDNS, manual probes, hide, panel) are not in this plan. Plans 02 to 05 own them.

**Names later plans use (all defined here)**

- `rocket_network::limits::*` (every constant in the index).
- `DeviceId`, `NodeProfile`, `initials_for`, `default_display_name`, `PeerIdentity`, `DeviceKeyMaterial`.
- `PairedDevice`, `BlockedDevice`, `ManualAddress`, `NetworkSettings`, `DiscoverySource`, `DiscoveredNode`.
- `Nonce`, `PairingId`, `PairingCode`, `commitment`, `pairing_code`, `PairingRole`, `PairingState`, `PairingStatus`, `PairingOutcome`, `PairingSession`.
- `Presence`, `PresenceTracker`, `backoff`, `DialHint`, `should_dial`, `keep_dialed_by`.
- `HelloResponse`, `PairRequest`, `PairRequestResponse`, `NonceMessage`, `DecisionRequest`, `PairStatusResponse`, `PeerMessage`, `NetworkEvent`.
- Ports: `DeviceIdentityStore`, `PairedDeviceRepository`, `BlockedDeviceRepository`, `ManualAddressRepository`, `NetworkSettingsRepository`, `QuarantineReport`, `Announcement`, `DiscoveryEvent`, `DiscoverySink`, `Discovery`, `PeerCallError`, `PeerError`, `PeerSocket`, `PeerHandler`, `PeerConnector`, `PeerServerControl`, `PeerTransport`, `PeerTransportFactory`.

**Additions beyond the index (all public, additive, no rename)**

- `NodeProfile::validated(&self)`: Plan 03 and Plan 04 call it on every profile that arrives in a REST body.
- `commitment_to_base64` and `commitment_from_base64`: Plan 03 uses them for `PairRequestResponse.commitment`.
- `PeerError::status_code()`: Plan 03 maps handler errors to HTTP with it.
- `Default` for `PairingId` and `PresenceTracker` (clippy `new_without_default`).
- New crate dependency `zeroize = "1.8"` for the zeroing `Drop` of `DeviceKeyMaterial`.

**Placeholder scan:** no TBD, no "similar to", every step has full code and a command with an expected result.

**Type consistency:** `PairingStatus` lives in `pairing.rs` and `wire.rs` imports it. `PeerMessage` is in `messages.rs` and `ports.rs` imports it. `PeerSocket.remote_hello` is a `PeerMessage`. `PairingSession::new_responder` returns `(PairingSession, [u8; 32])` and `new_initiator` takes that same array.
