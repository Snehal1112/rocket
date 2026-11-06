# Team Network MVP Plan Index

Spec: `docs/superpowers/specs/2026-11-05-team-network-discovery-pairing-design.md`
Roadmap: R-83, project CA-5 in `docs/roadmaps/contracts-ci-ai-roadmap.md`.

Plans run one at a time, in order. Each plan has at most three tasks and ends
with working, checked software. Each later plan builds only on names defined
in this index or in an earlier plan.

| # | Plan | Delivers | Model |
|---|---|---|---|
| 01 | `2026-11-06-team-network-plan-01-domain.md` | New crate `rocket-network`: ids, profile, records, limits, pairing crypto and state machine, presence and dialer rules, peer wire types, `NetworkEvent`, all ports. Pure, no I/O. | Sonnet |
| 02 | `2026-11-06-team-network-plan-02-identity-storage-tls.md` | `rocket-infra`: device key in the keychain, self-signed certificate, the four YAML repos under `~/.rocket-api/network/`, rustls server and client configs with the custom certificate checks. | Sonnet |
| 03 | `2026-11-06-team-network-plan-03-transport-discovery.md` | `rocket-infra`: TLS peer server (REST routes, limits, `/peer` WebSocket), peer connector (REST client, WebSocket client), `PeerSocket` pump, mDNS discovery, transport factory. | Opus |
| 04 | `2026-11-06-team-network-plan-04-service-tauri.md` | `rocket-app` `NetworkService` (state, nodes, pairing, presence, peer handler, audit), audit kinds, Tauri commands, DTOs, wiring, start at launch, stop on exit. | Sonnet |
| 04b | `2026-11-06-team-network-plan-04b-end-to-end.md` | End-to-end tests in `rocket-app`: two real `NetworkService`s over the real TLS transport on `127.0.0.1` (pairing, presence socket, unpair online and offline, Block on a paired device, a node in the middle, codes from the handshake keys). Test code only, on top of the Plan 03 `test-support` seams. | Sonnet |
| 05 | `2026-11-06-team-network-plan-05-frontend.md` | Store and event bridge, status bar item, Network panel, dialogs, incoming notice, intro card, Settings section, docs and manual checks. | Sonnet |

## Cross-plan facts

- Always pass `-j4` to cargo. Never run `cargo test --workspace` or `--all`.
  Use `cargo test -j4 -p <crate> <filter>` and `cargo check -j4 --workspace`.
- Frontend: Bun only. `bun run tsc --noEmit`, `bun run check` (fix with
  `bun run lint` and `bun run format`), `bunx vitest run <path>`.
- No commits during a plan. At the end of each plan, stop and show the user
  the whole plan's change. After the user's OK, commit through the
  `dev-workflow-skills:1-git-commit` skill, split into logical commits where
  it fits, staging explicit paths only (never `git add -A` or `.`). Subjects
  use conventional commits (`feat(network): ...`, `test(network): ...`,
  `docs(network): ...`).
- No unwrap or expect calls in production paths. Tests use
  `expect("reason")`. A repo hook rejects any Write or Edit whose Rust text
  contains a bare unwrap call, tests included.
- Mutex locks use `.lock().unwrap_or_else(|e| e.into_inner())`.
- Persistence structs keep snake_case serde. camelCase only on IPC DTOs in
  `src-tauri/src/commands/network.rs`. Peer wire types (REST bodies and
  `PeerMessage`) are a third category: snake_case JSON, defined once in
  `rocket-network/src/wire.rs` and `messages.rs`.
- Event enums: tag camelCase, variant fields snake_case, nested helper DTOs
  camelCase. `wire_contract_*` tests pin keys.
- A new Tauri command goes into `generate_handler!` in `src-tauri/src/lib.rs`
  and into `src-tauri/app-commands.txt` (sorted), or it is denied at runtime.
- Plan 02 Task 2 writes `.yml` files and starts with: "Before starting, read
  `docs/superpowers/specs/opencollection-spec-reference.md`." The network
  files are not collection files, but the repo rule applies to every task
  that writes `.yml`.
- Line numbers drift. Anchor every edit on quoted text, not on the number.

## Deviations from the spec (decided while planning)

1. **Damaged files are quarantined, not kept as `.bak`.** The repo already
   has a precedent (`fs_trust_store.rs`): rename to
   `<name>.corrupt-<unix-secs>` and start with an empty list. Same user
   message as the spec. `blocked.yml` still shows a warning (via
   `NetworkStatus.warnings`).
2. **Events are coarser than the spec's list.** `NodeDiscovered`, `NodeLost`
   and `PresenceChanged` become one `NodesChanged` event; the frontend
   refetches `network_list_nodes`. `PairingCodeReady` is dropped because
   `network_request_pairing` returns the code directly. This avoids two
   sources of truth for the node list.
3. **A fourth repo for settings.** `~/.rocket-api/network/settings.yml`
   holds `hidden`, `display_name` and `intro_seen`. The spec implied these
   settings but named no file.
4. **The dialer rule has a grace period.** When both sides see each other
   through mDNS, the lower device id dials. If no connection exists after
   `DIAL_GRACE` (10 s), the higher side dials too. This covers a lower side
   whose firewall blocks only outgoing connections. Duplicate connections are
   still resolved by keeping the one dialled by the lower id.
5. **Peer HTTPS client is hyper's low-level client** (`hyper::client::conn::http1`
   over a `tokio_rustls` stream), not the `hyper-util` legacy client and not
   reqwest. reqwest in this workspace is built with native-tls, which cannot
   run a custom certificate check.
6. **WebSocket client uses `tokio_tungstenite::client_async`** over our own
   `tokio_rustls` stream, so no new TLS feature is needed on
   `tokio-tungstenite`.
7. **The device public key is read from certificates with `rustls-webpki`** (`EndEntityCert::subject_public_key_info()`, already in the tree through rustls).
   `DeviceId = SHA-256(SubjectPublicKeyInfo DER)`. Pairing hashes also use
   the SPKI DER bytes as `PkA` and `PkB`.
8. **A pairing request costs a hello first.** `network_request_pairing` calls
   `/hello` on the address, checks the certificate key matches the device id
   being paired, and only then sends `/pair/request`.
9. **The default display name** is the OS user name from `USER` or
   `USERNAME`, falling back to `Rocket user`.

## Decisions made while writing the plans

Each plan's authors made these choices. Later plans already follow them.
The signature blocks below include the additions.

Plan 01 (domain):
- `NodeProfile::validated`, `commitment_to_base64`, `commitment_from_base64`, `PeerError::status_code` and `Default` for `PairingId` and `PresenceTracker` are added. Plans 03 and 04 then validate profiles and map errors to HTTP in one place.
- `PairingSession::fail()` is added (live to `Failed`, outcome `failed`). Without it the service could not tell a peer that dropped partway from a user's Cancel.
- The spec states `Requested`, `Revealed`, `LocalAccepted` and `RemoteAccepted` are folded into `Committed` and `CodeShown` plus two decision flags. The observable behaviour is the same and the state machine is smaller.
- `PeerMessage::decode` rebuilds the initials of a `Hello` profile and rejects bad names. A peer cannot choose its own initials or send control characters to the UI.
- Presence goes `Offline` when the third ping is sent while two are unanswered, so 40 to 60 s after the peer goes silent. This is how "two missed pings" reads with a 20 s interval.
- The new crate dependency `zeroize` zeroes `DeviceKeyMaterial` on drop.

Plan 02 (identity, storage, TLS):
- Certificates are read with `rustls-webpki` (`EndEntityCert`), not `x509-parser`. `x509-parser` is not in the local registry, and webpki is already in the tree through rustls.
- `rcgen` moves from a dev-dependency to a normal one. `tokio-rustls` becomes a normal dependency with `ring` only, and the dev-dependency keeps `tls12` for the TLS 1.2 refusal test.
- `rocket-infra` uses `rocket-network.workspace = true`, the entry Plan 01 adds to the root `Cargo.toml`. This was changed in the cross-plan review for consistency.
- `server_config`, `client_config` and `peer_identity_from_certs` live only at `rocket_infra::network::*`, because the names are too generic for the crate root.
- The repos take a file path. Plan 04 picks `~/.rocket-api/network/`.
- The verifiers only pin the key when an id is expected. The paired-and-not-blocked check runs in the routes through `PeerHandler::is_paired`. Session resumption is off, so every connection runs the full check.

Plan 03 (transport, discovery):
- `RustlsPeerServer::new(Arc<TlsIdentity>)` and `RustlsPeerConnector::new(Arc<TlsIdentity>)` are added, so the factory shares one identity. `ServerOptions` and `with_options` are a crate-private test seam.
- `axum` 0.8 with `ws` is added, which brings a second `tungstenite` (0.29) next to 0.30. This reuses axum's tested upgrade code instead of writing new handshake code.
- `mdns-sd` is pinned to `=0.21.6`, because 0.21.7 is less than two weeks old. `mio` moves to 1.2.x in the lock.
- `hyper` gains `client` and `hyper-util` gains `service`.
- The status codes `429` (hello limit), `413` (body), `408` (timeout), `204` (decision) and `405` (wrong method) are fixed. See "Server status codes" below.
- The transport hands every Hello-checked socket to `on_peer_socket`, including duplicates and version mismatches. Plan 04 applies the protocol version check and `keep_dialed_by`.
- A silent mDNS node drops out when mdns-sd's SRV record (120 s host TTL) expires. mdns-sd keeps its TTL setters crate-private, so the plan does not set a TTL.
- The server binds `0.0.0.0` only, and IPv6 link-local mDNS addresses are skipped. Rate limits are per server instance, and the hello limit is per source IP, so teammates behind one NAT share 30 a minute.
- The server limits pairing requests to 5 a minute before the handler runs. Plan 04's handler counts again as a second gate, and it can never refuse more than the server does.

Plan 04 (service, Tauri):
- The module is the folder `crates/rocket-app/src/network/`, not one `network_service.rs`, because the service is large. Its names are re-exported from `rocket_app`, including `paired_limit_message`.
- `PairingDecision` and `PairingStarted` derive `Debug`, `Clone`, `PartialEq` and `Eq`.
- `NetworkHandle::stop` also stops the server and discovery.
- `NodesChanged` is also published after a saved pairing, an unpair, a block, an unblock and every finished session, so the panel refetches.
- A `NotPaired` answer within 30 s of saving a pairing is not a removal (`NOT_PAIRED_GRACE`), because the responder saves before the initiator's poll does.
- Genuine failures (ten failed polls in a row, a decision call that does not reach the peer, a commitment mismatch, a failed save) call `PairingSession::fail()` and publish `failed`. Only a user's Cancel publishes `cancelled`. This was changed in the cross-plan review.
- `start_network` uses `tauri::async_runtime::spawn`, because `NetworkService::start` is async.
- No restart on a visibility or port change. The port is picked once at launch and does not change while the app runs. Hiding withdraws the mDNS record and showing announces it again, with no restart. The user confirmed this on 2026-11-06, and spec 5.3 and 10 now say the same.
- Block on a paired device also unpairs it and closes its connection (the user decided this on 2026-11-06).
  - `block_peer` adds the block, then runs the same unpair as Remove (`unpair_by_user`): it sends `Unpair` on a live socket, removes the `PairedDevice` and drops the connection entry, which closes the socket.
  - It publishes `PairedRemoved { by_peer: false }` and `NodesChanged`, and records both `NetworkDeviceBlocked` and `NetworkDeviceUnpaired { by_peer: false }`.
  - `run_socket` checks the block list before it handles any frame. A blocked id gets no answer and its socket closes, and a block list that cannot be read counts as blocked.
  - Spec 6.5 and 8.3 now say this.

Plan 04b (end-to-end):
- The tests are a `#[cfg(test)]` module of `rocket-app` (`network/e2e_tests.rs`), so they reuse Plan 04's fakes. They reach the real transport through `rocket-app`'s existing dev-dependency `rocket-infra = { workspace = true, features = ["test-support"] }`. There is no cycle, because `rocket-infra` does not depend on `rocket-app`. `src-tauri/tests` was rejected because it cannot see the fakes and would build the whole app.
- Nodes meet through manual addresses on loopback, and mDNS is `FakeDiscovery`. Plan 03 Task 3 Step 5b opens the seams, compiled only with `test` or `test-support`:
  - `RustlsPeerTransportFactory::local_test()`;
  - `RustlsPeerServer::local_test(Arc<TlsIdentity>)`;
  - `ServerOptions::local_test()`;
  - a public `rocket_infra::network::tls_identity::test_support::{key_material, identity}`.
- A relay test handler with its own key stands in for a node in the middle. It records the nonces and the handshake keys, so the tests can recompute both codes from the TLS keys.

Plan 05 (frontend):
- `ActivePairing` and the incoming request carry extra fields (`deviceId`, `shortId`, `accepted`) for the dialog. The store also owns `panelOpen` and `dialogPairingId`.
- The incoming toast comes from a store subscriber hook, so a duplicate event cannot make a second toast.
- There are two payload unions, one per event channel.
- Buttons that only hide a dialog are named "Done", because the X button is already named "Close".
- The "declined" toast shows only on the node that started the pairing, and `cancelled` is silent.

## Carry-over for implementers

- Plan 04 `block_peer`: send `Unpair` on the live socket (through
  `unpair_by_user`) before the block reaches the block list that `run_socket`
  checks. Otherwise an incoming frame can close the socket first and the peer
  learns of the removal only from a 403 on its next dial. Add an assertion
  that the peer received `Unpair` in
  `blocking_a_paired_device_unpairs_it_and_closes_its_socket`.
- Nothing in this series has been compiled. Expect small compile fixes
  (imports, inference, `#[cfg]` placement) on the first run of each task. Fix
  the code, never weaken a test; stop and report if a test's intent cannot
  hold.
- Plan 04b tests run on real time with 20 s waits. Run the suite ten times as
  the plan says before calling it done.

## Shared names and signatures

Every later plan uses these names exactly. Plan 01 defines everything in
`crates/rocket-network`. `DomainResult` and `DomainError` are
`rocket_shared::error::{DomainResult, DomainError}`.

### Constants (`rocket_network::limits`)

```rust
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
```

### Identity and profile

```rust
// device.rs
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceId([u8; 32]);
impl DeviceId {
    pub fn from_spki(spki_der: &[u8]) -> DeviceId;          // SHA-256
    pub fn from_bytes(bytes: [u8; 32]) -> DeviceId;
    pub fn as_bytes(&self) -> &[u8; 32];
    pub fn to_base32(&self) -> String;                     // data_encoding::BASE32_NOPAD, 52 chars
    pub fn parse_base32(s: &str) -> DomainResult<DeviceId>; // InvalidInput on bad input
    pub fn short(&self) -> String;                         // first 8 base32 chars as "XXXX-XXXX"
}
// Serialize/Deserialize as the base32 string. Debug and Display print short().

// profile.rs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeProfile { pub display_name: String, pub initials: String }
impl NodeProfile {
    pub fn new(display_name: &str) -> DomainResult<NodeProfile>; // trims; 1..=40 chars; no control chars
    pub fn validated(&self) -> DomainResult<NodeProfile>;        // Plan 01 addition: re-checks the name, rebuilds initials (every profile from a peer)
}
pub fn initials_for(display_name: &str) -> String; // first char of the first two words, uppercased; one word -> first char
pub fn default_display_name(user: Option<&str>) -> String; // trimmed user or "Rocket user"

// identity.rs
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerIdentity { pub device_id: DeviceId, pub spki_der: Vec<u8> }
impl PeerIdentity { pub fn from_spki(spki_der: Vec<u8>) -> PeerIdentity; }
pub struct DeviceKeyMaterial { pub pkcs8_der: Vec<u8> } // no Debug, no Clone; Drop zeroes the bytes
```

### Records (persistence, snake_case)

```rust
// records.rs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedDevice {
    pub device_id: DeviceId,
    pub name_at_pairing: String,
    pub paired_at: DateTime<Utc>,
    #[serde(default)] pub last_addresses: Vec<SocketAddr>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockedDevice { pub device_id: DeviceId, pub name: String, pub blocked_at: DateTime<Utc> }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualAddress { pub host: String, pub port: u16, pub added_at: DateTime<Utc> }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NetworkSettings {
    #[serde(default)] pub hidden: bool,
    #[serde(default)] pub display_name: Option<String>,
    #[serde(default)] pub intro_seen: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscoverySource { Mdns, Manual }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredNode {
    pub device_id: DeviceId, pub profile: NodeProfile, pub addresses: Vec<SocketAddr>,
    pub source: DiscoverySource, pub last_seen: DateTime<Utc>, pub protocol_version: u32,
}
```

### Pairing (`pairing.rs`)

```rust
#[derive(Clone, PartialEq, Eq)] pub struct Nonce(pub [u8; 32]);
impl Nonce { pub fn random() -> Nonce; pub fn to_base64(&self) -> String; pub fn parse_base64(s: &str) -> DomainResult<Nonce>; }
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PairingId(pub String);  // uuid v4 string
impl PairingId { pub fn new() -> PairingId; } // also Default
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct PairingCode(pub u32); // 0..=999_999
impl PairingCode { pub fn digits(&self) -> String; } // "048213"; Display -> "048 213"

pub fn commitment(responder_spki: &[u8], initiator_spki: &[u8], nb: &Nonce) -> [u8; 32];
// SHA-256("rocket-pair-commit-v1" || len-prefixed responder_spki || len-prefixed initiator_spki || nb)
pub fn pairing_code(initiator_spki: &[u8], responder_spki: &[u8], na: &Nonce, nb: &Nonce) -> PairingCode;
// SHA-256("rocket-pair-code-v1" || len-prefixed initiator_spki || len-prefixed responder_spki || na || nb),
// first 8 bytes big-endian u64 % 1_000_000. len-prefix = u32 big-endian byte length.
pub fn commitment_to_base64(commitment: &[u8; 32]) -> String;        // Plan 01 addition, for PairRequestResponse.commitment
pub fn commitment_from_base64(s: &str) -> DomainResult<[u8; 32]>;   // Plan 01 addition

#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum PairingRole { Initiator, Responder }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingState { Committed, CodeShown, Paired, Declined, Expired, Cancelled, Failed }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairingStatus { Pending, Accepted, Declined, Expired }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingOutcome { Accepted, Declined, Expired, Cancelled, Failed }
impl PairingOutcome { pub fn as_str(&self) -> &'static str; } // "accepted", ...

pub struct PairingSession { /* private fields */ }
impl PairingSession {
    pub fn new_responder(id: PairingId, peer: PeerIdentity, peer_profile: NodeProfile,
        local_spki: Vec<u8>, nb: Nonce, now: Instant) -> (PairingSession, [u8; 32]);         // state Committed
    pub fn new_initiator(id: PairingId, peer: PeerIdentity, peer_profile: NodeProfile,
        local_spki: Vec<u8>, commitment: [u8; 32], na: Nonce, now: Instant) -> PairingSession; // state Committed
    pub fn receive_nonce(&mut self, na: Nonce) -> DomainResult<(Nonce, PairingCode)>; // responder: Committed -> CodeShown, returns nb
    pub fn receive_reveal(&mut self, nb: Nonce) -> DomainResult<PairingCode>;         // initiator: commitment check; mismatch -> Failed + Err
    pub fn decide_local(&mut self, accept: bool) -> DomainResult<()>;                 // only in CodeShown
    pub fn decide_remote(&mut self, accept: bool) -> DomainResult<()>;                // only in CodeShown
    pub fn cancel(&mut self);
    pub fn fail(&mut self);                                                            // live -> Failed; used when the peer fails partway
    pub fn expire_if_due(&mut self, now: Instant) -> bool;                            // PAIRING_TTL from creation
    pub fn id(&self) -> &PairingId; pub fn role(&self) -> PairingRole; pub fn state(&self) -> PairingState;
    pub fn peer(&self) -> &PeerIdentity; pub fn peer_profile(&self) -> &NodeProfile;
    pub fn code(&self) -> Option<PairingCode>;
    pub fn local_nonce_b64(&self) -> String;                                          // for the wire
    pub fn status(&self) -> PairingStatus;     // Paired->Accepted, Declined|Cancelled|Failed->Declined, Expired->Expired, else Pending
    pub fn outcome(&self) -> Option<PairingOutcome>;
    pub fn is_terminal(&self) -> bool;
}
// Rules: both decisions true -> Paired. Any false -> Declined. A decision after a terminal state is Err(Conflict).
```

### Presence and dialing (`presence.rs`, `dialer.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Presence { Unknown, Online, Offline }
pub struct PresenceTracker { /* private */ }
impl PresenceTracker {
    pub fn new() -> PresenceTracker;                 // Unknown, attempt 0; also Default
    pub fn state(&self) -> Presence;
    pub fn on_connected(&mut self);                  // Online, attempt 0, missed 0
    pub fn on_pong(&mut self);                       // missed 0
    pub fn on_ping_sent(&mut self) -> Presence;      // missed += 1; missed > MISSED_PINGS_OFFLINE -> Offline
    pub fn on_disconnected(&mut self);               // Offline, attempt += 1
    pub fn next_retry_delay(&self) -> Duration;      // backoff(attempt)
}
pub fn backoff(attempt: u32) -> Duration;            // 0->1s,1->1s,2->2s,3->4s,... capped at BACKOFF_MAX

#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum DialHint { Mdns, ManualAddress }
pub fn should_dial(local: &DeviceId, remote: &DeviceId, hint: DialHint, waited: Duration) -> bool;
// ManualAddress -> true; Mdns -> local < remote || waited >= DIAL_GRACE
pub fn keep_dialed_by(local: &DeviceId, remote: &DeviceId) -> DeviceId; // min(local, remote)
```

### Peer wire types (`wire.rs`, `messages.rs`), snake_case JSON

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloResponse { pub device_id: DeviceId, pub version: u32, pub profile: NodeProfile }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairRequest { pub version: u32, pub profile: NodeProfile }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairRequestResponse { pub pairing_id: PairingId, pub commitment: String } // base64
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonceMessage { pub nonce: String }                                       // base64, used both ways
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionRequest { pub accept: bool }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairStatusResponse { pub status: PairingStatus }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PeerMessage { Hello { version: u32, profile: NodeProfile }, Ping, Pong, Unpair }
impl PeerMessage {
    pub fn encode(&self) -> String;
    pub fn decode(text: &str) -> DomainResult<PeerMessage>; // len > MAX_FRAME_BYTES or unknown type -> InvalidInput
}
```

Routes (Plan 03): `GET /hello`, `POST /pair/request`, `POST /pair/{id}/nonce`,
`POST /pair/{id}/decision`, `GET /pair/{id}`, `GET /peer` (WebSocket).

Server status codes (Plan 03, the contract Plan 04 relies on): any request
with an `Origin` header `403 forbidden` (before any route, `/peer` included);
`/hello` over 30 a minute from one IP `429 slow-down`; `/pair/request` over 5 a
minute across all callers `503 busy` (no handler call); invalid profile or
pairing id `400`; decision success `204`; handler `Err(PeerError)` uses
`status_code()` with bodies `not-found`, `not-paired`, `busy`, `forbidden`,
`bad-request`, `internal`; body over 16 KiB `413`; unknown path `404
not-found`; a request not answered within 10 s `408 timeout`; `/peer` from a
key that is not paired `403 not-paired` before the upgrade. Client mapping
(`RustlsPeerConnector`): I/O failure or timeout `Unreachable`; certificate id
mismatch or a hello body id that differs from the certificate
`IdentityMismatch`; other TLS errors or malformed replies `Protocol`; `403
not-paired` `NotPaired` (any `403` on `/peer`); `503` `Busy`; other non-2xx
`Rejected(status)`. The transport does not check the protocol version and
does not resolve duplicate sockets: it hands every Hello-checked socket to
`on_peer_socket` and Plan 04 applies the version check and `keep_dialed_by`.
Dropping `PeerSocket.outbound` closes the socket; `inbound` yields `None` when
the socket ends.

### Events (`events.rs`)

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum NetworkEvent {
    NodesChanged,                                                    // channel "network-changed"
    StatusChanged,                                                   // channel "network-changed"
    PairedRemoved { device_id: String, display_name: String, by_peer: bool }, // "network-changed"
    PairingIncoming { pairing_id: String, device_id: String, short_id: String,
                      display_name: String, code: String },          // "network-pairing"; code = digits()
    PairingFinished { pairing_id: String, outcome: String,
                      display_name: String },                        // "network-pairing"; outcome = PairingOutcome::as_str()
}
```

### Ports (`ports.rs`)

```rust
pub trait DeviceIdentityStore: Send + Sync {
    fn load_or_create(&self) -> DomainResult<DeviceKeyMaterial>; // Err if the keychain is unavailable; never falls back to a file
}
pub trait PairedDeviceRepository: Send + Sync {
    fn list(&self) -> DomainResult<Vec<PairedDevice>>;
    fn get(&self, id: &DeviceId) -> DomainResult<Option<PairedDevice>>;
    fn upsert(&self, device: PairedDevice) -> DomainResult<()>;  // InvalidInput when a new entry would exceed MAX_PAIRED_DEVICES
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
    fn add(&self, address: ManualAddress) -> DomainResult<()>;   // dedupe on (host lowercased, port); InvalidInput over MAX_MANUAL_ADDRESSES
    fn remove(&self, host: &str, port: u16) -> DomainResult<bool>;
}
pub trait NetworkSettingsRepository: Send + Sync {
    fn load(&self) -> DomainResult<NetworkSettings>;
    fn save(&self, settings: &NetworkSettings) -> DomainResult<()>;
}
/// Each Fs repo also implements this, so the service can warn about a quarantined file once.
pub trait QuarantineReport: Send + Sync { fn take_quarantined(&self) -> Option<String>; } // file name

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announcement { pub device_id: DeviceId, pub profile: NodeProfile, pub port: u16, pub version: u32 }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryEvent {
    Seen { claimed_id: DeviceId, profile: NodeProfile, addresses: Vec<SocketAddr>, version: u32 },
    Gone { claimed_id: DeviceId },
}
pub trait DiscoverySink: Send + Sync { fn on_event(&self, event: DiscoveryEvent); }
pub trait Discovery: Send + Sync {
    fn start(&self, sink: Arc<dyn DiscoverySink>) -> DomainResult<()>; // Err -> "discovery unavailable"
    fn announce(&self, announcement: &Announcement) -> DomainResult<()>;
    fn withdraw(&self) -> DomainResult<()>;
    fn stop(&self);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerCallError { Unreachable(String), NotPaired, Busy, Rejected(u16), IdentityMismatch, Protocol(String) }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerError { NotFound, NotPaired, Busy, Forbidden, BadRequest, Internal } // HTTP 404, 403, 503, 403, 400, 500
impl PeerError { pub fn status_code(&self) -> u16; }   // Plan 01 addition; Plan 03 maps handler errors with it

pub struct PeerSocket {
    pub peer: PeerIdentity,
    pub dialed_by: DeviceId,
    pub remote_hello: PeerMessage,                          // the peer's Hello frame
    pub outbound: tokio::sync::mpsc::Sender<PeerMessage>,
    pub inbound: tokio::sync::mpsc::Receiver<PeerMessage>, // closes when the socket closes
}

#[async_trait]
pub trait PeerHandler: Send + Sync {
    async fn hello(&self) -> HelloResponse;
    async fn pair_request(&self, peer: PeerIdentity, remote_ip: IpAddr, req: PairRequest) -> Result<PairRequestResponse, PeerError>;
    async fn pair_nonce(&self, peer: PeerIdentity, id: PairingId, msg: NonceMessage) -> Result<NonceMessage, PeerError>;
    async fn pair_decision(&self, peer: PeerIdentity, id: PairingId, req: DecisionRequest) -> Result<(), PeerError>;
    async fn pair_status(&self, peer: PeerIdentity, id: PairingId) -> Result<PairStatusResponse, PeerError>;
    async fn is_paired(&self, device: &DeviceId) -> bool;          // paired and not blocked
    async fn on_peer_socket(&self, socket: PeerSocket);            // takes ownership; called after Hello frames were exchanged
    fn local_hello(&self) -> PeerMessage;                           // PeerMessage::Hello for the first frame
}

#[async_trait]
pub trait PeerConnector: Send + Sync {
    async fn hello(&self, addr: SocketAddr) -> Result<(PeerIdentity, HelloResponse), PeerCallError>;
    async fn pair_request(&self, addr: SocketAddr, expected: &DeviceId, req: &PairRequest) -> Result<(PeerIdentity, PairRequestResponse), PeerCallError>;
    async fn pair_nonce(&self, addr: SocketAddr, expected: &DeviceId, id: &PairingId, msg: &NonceMessage) -> Result<NonceMessage, PeerCallError>;
    async fn pair_decision(&self, addr: SocketAddr, expected: &DeviceId, id: &PairingId, req: &DecisionRequest) -> Result<(), PeerCallError>;
    async fn pair_status(&self, addr: SocketAddr, expected: &DeviceId, id: &PairingId) -> Result<PairStatusResponse, PeerCallError>;
    async fn open_socket(&self, addr: SocketAddr, expected: &DeviceId, hello: PeerMessage) -> Result<PeerSocket, PeerCallError>;
}

#[async_trait]
pub trait PeerServerControl: Send + Sync {
    async fn start(&self, handler: Arc<dyn PeerHandler>) -> DomainResult<u16>; // binds 0.0.0.0, DEFAULT_PORT..=PORT_RANGE_END; Err when none is free
    fn stop(&self);
}

pub struct PeerTransport {
    pub local: PeerIdentity,
    pub connector: Arc<dyn PeerConnector>,
    pub server: Arc<dyn PeerServerControl>,
}
pub trait PeerTransportFactory: Send + Sync {
    fn build(&self, key: &DeviceKeyMaterial) -> DomainResult<PeerTransport>;
}
```

### Infra names (Plans 02 and 03, `crates/rocket-infra/src/network/`)

```rust
// Exported at `rocket_infra::network::*`. The crate root re-exports every type and
// `build_tls_identity`, `spki_from_cert`, but not the TLS config functions (too generic).
// identity_store.rs (Plan 02)
pub struct KeychainDeviceIdentity { /* Arc<dyn SecretStore> */ }
impl KeychainDeviceIdentity { pub fn new(store: Arc<dyn SecretStore>) -> Self; }
// keychain service: KeyringSecretStore::new_device_identity() -> "com.rocketapi.device-identity"
// scope "device", key "ed25519-pkcs8", value base64 of the PKCS#8 DER. Reads use get_strict.
// tls_identity.rs (Plan 02)
pub struct TlsIdentity { pub cert_der: CertificateDer<'static>, pub key_der: PrivatePkcs8KeyDer<'static>, pub local: PeerIdentity }
pub fn build_tls_identity(key: &DeviceKeyMaterial) -> DomainResult<TlsIdentity>;   // rcgen self-signed, CN "rocket-node"
pub fn spki_from_cert(cert_der: &[u8]) -> DomainResult<Vec<u8>>;                 // rustls-webpki EndEntityCert
// Test builds only: crate::network::tls_identity::test_support::{identity() -> TlsIdentity, key_material() -> DeviceKeyMaterial}.
// repos.rs (Plan 02): FsPairedDeviceRepo, FsBlockedDeviceRepo, FsManualAddressRepo, FsNetworkSettingsRepo,
// each `new(path: PathBuf) -> Self` (the caller picks the file), each also `impl QuarantineReport`,
// files paired.yml, blocked.yml, addresses.yml, settings.yml
// tls_config.rs (Plan 02)
pub fn server_config(identity: &TlsIdentity) -> DomainResult<Arc<rustls::ServerConfig>>; // TLS 1.3 only, client cert required, any key accepted (signature still verified)
pub fn client_config(identity: &TlsIdentity, expected: Option<DeviceId>) -> DomainResult<Arc<rustls::ClientConfig>>; // TLS 1.3 only, presents our cert; expected=Some -> key must hash to it
pub fn peer_identity_from_certs(certs: Option<&[CertificateDer<'_>]>) -> DomainResult<PeerIdentity>;
// server.rs, client.rs, socket.rs, mdns.rs, transport.rs (Plan 03)
pub struct RustlsPeerServer;      // impl PeerServerControl, `new(identity: Arc<TlsIdentity>) -> Self`
pub struct RustlsPeerConnector;   // impl PeerConnector, `new(identity: Arc<TlsIdentity>) -> Self`
pub struct MdnsDiscovery;         // impl Discovery, `new() -> Self`, also Default
pub struct RustlsPeerTransportFactory; // impl PeerTransportFactory, `new() -> Self`, also Default; one Arc<TlsIdentity> for server and connector
// Test seams (Plan 03 Task 3 Step 5b), only with `test` or the `test-support` feature, used by Plan 04b:
// RustlsPeerTransportFactory::local_test() -> Self; RustlsPeerServer::local_test(Arc<TlsIdentity>) -> Self
// (both bind 127.0.0.1 on an OS port); rocket_infra::network::tls_identity::test_support::{key_material, identity} are pub.
```

### App names (Plan 04, module folder `crates/rocket-app/src/network/`, re-exported from `rocket_app`)

```rust
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
pub struct NetworkService { /* private */ }
pub struct NetworkHandle { /* watch::Sender<bool> */ }   // stop(), Drop stops
impl NetworkService {
    pub fn new(deps: NetworkDeps) -> Arc<NetworkService>;
    pub async fn start(self: &Arc<Self>) -> NetworkHandle;          // never fails; failures land in status()
    pub fn status(&self) -> NetworkStatus;
    pub fn list_nodes(&self) -> Vec<NodeView>;
    pub async fn set_hidden(self: &Arc<Self>, hidden: bool) -> DomainResult<()>;
    pub async fn set_display_name(self: &Arc<Self>, name: &str) -> DomainResult<()>;
    pub fn mark_intro_seen(&self) -> DomainResult<()>;
    pub fn list_addresses(&self) -> DomainResult<Vec<ManualAddress>>;
    pub async fn add_address(self: &Arc<Self>, host: &str, port: u16) -> DomainResult<()>;
    pub fn remove_address(&self, host: &str, port: u16) -> DomainResult<()>;
    pub async fn request_pairing(self: &Arc<Self>, device_id: &DeviceId) -> DomainResult<PairingStarted>;
    pub async fn respond_pairing(self: &Arc<Self>, id: &PairingId, decision: PairingDecision) -> DomainResult<()>;
    pub fn cancel_pairing(&self, id: &PairingId) -> DomainResult<()>;
    pub async fn remove_paired(self: &Arc<Self>, device_id: &DeviceId) -> DomainResult<()>;
    pub fn list_blocked(&self) -> DomainResult<Vec<BlockedDevice>>;
    pub fn unblock(&self, device_id: &DeviceId) -> DomainResult<()>;
}
// NetworkService also implements PeerHandler directly; Arc<NetworkService> is passed as Arc<dyn PeerHandler>.
pub fn paired_limit_message() -> String; // Plan 04 addition: "You've reached the limit of 200 paired devices. Remove one to pair."
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum PairingDecision { Accept, Decline, Block }
pub struct PairingStarted { pub pairing_id: PairingId, pub code: PairingCode }
pub enum NetworkState { Running, Off { reason: String } }
pub struct NetworkStatus {
    pub state: NetworkState, pub port: Option<u16>, pub listener_available: bool,
    pub discovery_available: bool, pub hidden: bool, pub profile: NodeProfile,
    pub device_short_id: Option<String>, pub intro_seen: bool, pub warnings: Vec<String>,
}
pub enum NodeGroup { Paired, Nearby }
pub enum NodeStatus { Online, Offline, Pairing, NeedsUpdate, Available }
pub struct NodeView {
    pub device_id: String, pub short_id: String, pub display_name: String, pub initials: String,
    pub group: NodeGroup, pub status: NodeStatus, pub source: Option<DiscoverySource>,
}
```

New audit kinds (Plan 04, `rocket-audit`): `NetworkDevicePaired { device_id }`,
`NetworkDeviceUnpaired { device_id, by_peer: bool }`,
`NetworkDeviceBlocked { device_id }`, tags `network_device_paired`,
`network_device_unpaired`, `network_device_blocked`, mapped to SOC 2 CC6.1.

### Tauri commands (Plan 04) and TS wrappers (Plan 05)

| Rust command | Args (camelCase over IPC) | Returns | TS wrapper |
|---|---|---|---|
| `network_status` | none | `NetworkStatusDto` | `getNetworkStatus()` |
| `network_list_nodes` | none | `NodeViewDto[]` | `listNetworkNodes()` |
| `network_set_visibility` | `hidden: bool` | `()` | `setNetworkVisibility(hidden)` |
| `network_set_profile` | `displayName: String` | `()` | `setNetworkProfile(displayName)` |
| `network_mark_intro_seen` | none | `()` | `markNetworkIntroSeen()` |
| `network_list_addresses` | none | `ManualAddressDto[]` | `listNetworkAddresses()` |
| `network_add_address` | `host: String, port: u16` | `()` | `addNetworkAddress(host, port)` |
| `network_remove_address` | `host: String, port: u16` | `()` | `removeNetworkAddress(host, port)` |
| `network_request_pairing` | `deviceId: String` | `PairingStartedDto { pairingId, code }` | `requestNetworkPairing(deviceId)` |
| `network_respond_pairing` | `pairingId: String, decision: "accept" \| "decline" \| "block"` | `()` | `respondNetworkPairing(pairingId, decision)` |
| `network_cancel_pairing` | `pairingId: String` | `()` | `cancelNetworkPairing(pairingId)` |
| `network_remove_paired` | `deviceId: String` | `()` | `removeNetworkPaired(deviceId)` |
| `network_list_blocked` | none | `BlockedDeviceDto[]` | `listNetworkBlocked()` |
| `network_unblock` | `deviceId: String` | `()` | `unblockNetworkDevice(deviceId)` |

DTO fields (camelCase): `NetworkStatusDto { state: "running" | "off", offReason: string | null, port: number | null, listenerAvailable, discoveryAvailable, hidden, displayName, initials, deviceShortId: string | null, introSeen, warnings: string[] }`;
`NodeViewDto { deviceId, shortId, displayName, initials, group: "paired" | "nearby", status: "online" | "offline" | "pairing" | "needs_update" | "available", source: "mdns" | "manual" | null }`;
`ManualAddressDto { host, port, addedAt }`; `BlockedDeviceDto { deviceId, shortId, name, blockedAt }`.
`code` in `PairingStartedDto` is the 6 digits without a space; the UI shows `482 913`.

Event channels for the frontend: `network-changed` (`nodesChanged`,
`statusChanged`, `pairedRemoved`) and `network-pairing` (`pairingIncoming`,
`pairingFinished`). The `type` tag values are camelCase; the other payload
fields are snake_case. `TauriEventBus` adds `seq` and `tsMs`, which listeners
ignore. `pairingFinished.outcome` is `accepted`, `declined`, `expired`,
`cancelled` (a user's Cancel only, shown silently) or `failed` (commitment
mismatch, the peer stopped answering partway, a failed save; shown as
"Pairing failed. Try again.").

## Shared code added for reuse (2026-11-06 review)

A reuse review found code the plans wrote again that Rocket already has. These
shared pieces replace the copies. Each plan names them in its own steps.

| Piece | Where | Replaces | Plan |
|---|---|---|---|
| `yaml_io::quarantine_corrupt(&Path)` | `rocket-infra/src/yaml_io.rs` | private quarantine in `fs_trust_store.rs` and `YamlFile::quarantine` | 02 |
| `YamlListRepo`, `Keyed`, `ListFile`, `quarantine_report!` | `rocket-infra/src/network/repos.rs` (private) | repeated list and quarantine code in three repos | 02 |
| `network::lock::lock_ignore_poison` | `rocket-infra/src/network/lock.rs` (crate-private) | repeated poison-tolerant lock calls | 02, 03 |
| `callback_server::{ACCEPT_ERROR_BACKOFF, HEADER_READ_TIMEOUT}` now `pub(crate)` | `rocket-infra/src/callback_server.rs` | constants declared again in the network server | 03 |
| `client::REQUEST_TIMEOUT` declared once | `rocket-infra/src/network/client.rs` | a second declaration in `server.rs` | 03 |
| `SlidingWindow::{new, try_hit, clear}` | `rocket-network/src/rate.rs`, re-exported as `rocket_network::SlidingWindow` | inline request-window loop | 01, 04 |
| `stop_signal::{sleep_or_stop, run_every}` | `rocket-app/src/stop_signal.rs` (crate-private) | `sleep_or_stop` in `openapi_sync_scheduler.rs` and a second copy | 04 |
| `commands::slot::{Stoppable, Slot<H>}` (`replace`, `stop`, `stop_for_good`) | `src-tauri/src/commands/slot.rs` | `SyncSchedulerSlot` and `NetworkSlot` bodies. `commands::import::Stoppable` no longer exists | 04 |
| `FakeDiscovery::play` (was `emit`), `accept_both`, `RecordingEvents::first_incoming` | `rocket-app/src/network/test_support.rs` | name clash with `NetworkService::emit`, repeated accept steps | 04, 04b |
| `AuditEventKind` network kinds in TS, `AuditEventRow` summaries | `src/lib/tauri-api.ts`, `src/components/audit/AuditEventRow.tsx` | missing audit screen support | 05 |
| `createListenerScope` | `src/lib/listen-scope.ts` | listener teardown code in `useUpdateNotice.tsx` (migrated) | 05 |
| `errorText(err, fallback)`, `toastError` | `src/lib/error-text.ts` (moved from `network-utils.ts`) | error text helpers | 05 |
| `useAsyncAction` | `src/hooks/useAsyncAction.ts` | repeated try, catch, busy | 05 |
| `StatusBarItem`, `StatusBarCount` | `src/components/status-bar/` | copied class strings (`ContractsStatusItem` migrated) | 05 |
| `SearchInput` | `src/components/ui/search-input.tsx` | hand-written search box markup | 05 |
| `InitialsAvatar` | `src/components/ui/initials-avatar.tsx` | inline avatar circle | 05 |
| `Badge` `success` variant | `src/components/ui/badge.tsx` | hand-written green classes | 05 |
| `ListRow`, `NetworkIdentity`, `SectionLabel` | `src/components/network/` | repeated row and heading markup | 05 |
| Store actions `removePaired`, `addAddress`, `removeAddress`, `unblock`, `setHidden`, `saveName`, `markIntroSeen`; fixture `makeIncoming` | `src/stores/network-store.ts`, `src/test/network-fixtures.ts` | refresh calls and repeated test literals in components | 05 |

Kept as written on purpose: the TLS verifiers (no equivalent exists), the
fixed-window `/hello` limiter, the keychain test fake, `NetworkService::now`,
`spawn`, the TTL sweeps, `wait_until`, the DTO `From` impls, the end-to-end relay.

Not done (repo-wide, outside this feature): a generic `ConfirmDialog`,
`SettingsBlock` and `SettingsSwitchRow`, a shared generic event recorder for
test doubles, migrating the other search boxes and error-text copies.

Behaviour changes from the reuse work: a failed event-listener registration is
swallowed (as in `useUpdateNotice`), `unblock` also reloads the nodes, request
rows get a coloured avatar, blocked rows show name above short id.
