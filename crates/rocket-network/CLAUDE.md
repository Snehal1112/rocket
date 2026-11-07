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
| `PeerMessage` | WebSocket frame enum: Hello, Ping, Pong, Unpair and the six transfer frames (offer, answer, chunk, abort, chunk ack, result). `decode` enforces the 64 KiB limit and rejects unknown types. |
| `AbortReason`, `ResultOutcome` | Why a transfer stopped (`cancelled`, `failed`, `too_big`, `busy`, `expired`) and what the receiver reports at the end (`completed`, `damaged`, `rejected`, `failed`). |
| `TransferId`, `chunk_count_for`, `offer_name_is_valid`, `Direction`, `Outcome` | Transfer id (32 lowercase hex), chunk math, the offer name rule, direction and result names (`unconfirmed` when the sender got no result after the last chunk). |
| `OutgoingTransfer`, `IncomingTransfer`, `OfferError`, `ChunkStep` | Sender and receiver state machines. Time is passed in as `Instant`. They hold no bytes. The sender keeps an ack window of 32 chunks (`window_open`, `on_ack`) and stays `Done` until `on_result` brings the receiver's outcome, or ends `Unconfirmed` when the peer goes or the result times out. |
| `TransferLimits`, `Admission` | Pending, per-peer and rate limits for offers, plus duplicate-id detection. |
| `TransferSink` | Port with `on_frame` (the six transfer frames from `NetworkService`) and `on_peer_gone` (peer socket ended or pairing removed). |
| `NetworkEvent`, `TransferEvent` | Events for the frontend. Implement `PublishedEvent`. Channels `network-changed`, `network-pairing` and `network-transfer`. |
| Ports | `DeviceIdentityStore`, `PairedDeviceRepository`, `BlockedDeviceRepository`, `ManualAddressRepository`, `NetworkSettingsRepository`, `QuarantineReport`, `Discovery`, `DiscoverySink`, `PeerHandler`, `PeerConnector`, `PeerServerControl`, `PeerTransportFactory`, plus `PeerSocket`, `PeerTransport`, `PeerError`, `PeerCallError`. |

## Key Patterns

- All repository traits are synchronous and `Send + Sync`. The peer traits are async (`async_trait`).
- All fallible methods return `DomainResult<T>` from `rocket-shared`.
- Persistence structs keep snake_case serde. Peer wire types are snake_case JSON. Only IPC DTOs in `src-tauri` use camelCase.
- `NetworkEvent` tag is camelCase and variant fields are snake_case on purpose. `wire_contract_*` tests pin the keys.
- Pairing logic never reads the clock. Callers pass `Instant` to `PairingSession::new_*` and `expire_if_due`.
- Transfer machines never read the clock and never touch bytes on disk. They return decoded chunk bytes to the caller.
- Never log nonces, codes, keys or certificates. `Nonce` and `DeviceKeyMaterial` have no `Debug` on purpose.

## Workspace Relationships

- Depends on `rocket-shared` and small pure crates (`serde`, `sha2`, `base64`, `data-encoding`, `rand`, `zeroize`, `uuid`, `chrono`, `async-trait`). It uses `tokio::sync::mpsc` channel types only, for `PeerSocket`, declared with the `sync` feature like `rocket-http` does. It never starts a runtime, opens a socket or touches the filesystem. Do not add `rustls`, `axum`, `hyper`, `mdns-sd`, `keyring` or `tauri` here.
- Concrete implementations of the ports live in `rocket-infra` (`src/network/`).
- `rocket-app` holds the ports as `Arc<dyn Trait>` in `NetworkService`.
- The Tauri shell in `src-tauri` wires infra to the service at startup.
