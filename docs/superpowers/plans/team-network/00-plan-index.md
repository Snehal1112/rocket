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
impl PairingId { pub fn new() -> PairingId; }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct PairingCode(pub u32); // 0..=999_999
impl PairingCode { pub fn digits(&self) -> String; } // "048213"; Display -> "048 213"

pub fn commitment(responder_spki: &[u8], initiator_spki: &[u8], nb: &Nonce) -> [u8; 32];
// SHA-256("rocket-pair-commit-v1" || len-prefixed responder_spki || len-prefixed initiator_spki || nb)
pub fn pairing_code(initiator_spki: &[u8], responder_spki: &[u8], na: &Nonce, nb: &Nonce) -> PairingCode;
// SHA-256("rocket-pair-code-v1" || len-prefixed initiator_spki || len-prefixed responder_spki || na || nb),
// first 8 bytes big-endian u64 % 1_000_000. len-prefix = u32 big-endian byte length.

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
    pub fn new() -> PresenceTracker;                 // Unknown, attempt 0
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
// identity_store.rs (Plan 02)
pub struct KeychainDeviceIdentity { /* Arc<dyn SecretStore> */ }
impl KeychainDeviceIdentity { pub fn new(store: Arc<dyn SecretStore>) -> Self; }
// keychain service: KeyringSecretStore::new_device_identity() -> "com.rocketapi.device-identity"
// scope "device", key "ed25519-pkcs8", value base64 of the PKCS#8 DER. Reads use get_strict.
// tls_identity.rs (Plan 02)
pub struct TlsIdentity { pub cert_der: CertificateDer<'static>, pub key_der: PrivatePkcs8KeyDer<'static>, pub local: PeerIdentity }
pub fn build_tls_identity(key: &DeviceKeyMaterial) -> DomainResult<TlsIdentity>;   // rcgen self-signed, CN "rocket-node"
pub fn spki_from_cert(cert_der: &[u8]) -> DomainResult<Vec<u8>>;                 // rustls-webpki EndEntityCert
// repos.rs (Plan 02): FsPairedDeviceRepo, FsBlockedDeviceRepo, FsManualAddressRepo, FsNetworkSettingsRepo,
// each `new(path: PathBuf) -> Self`, files paired.yml, blocked.yml, addresses.yml, settings.yml
// tls_config.rs (Plan 02)
pub fn server_config(identity: &TlsIdentity) -> DomainResult<Arc<rustls::ServerConfig>>; // TLS 1.3 only, client cert required, any key accepted (signature still verified)
pub fn client_config(identity: &TlsIdentity, expected: Option<DeviceId>) -> DomainResult<Arc<rustls::ClientConfig>>; // TLS 1.3 only, presents our cert; expected=Some -> key must hash to it
pub fn peer_identity_from_certs(certs: Option<&[CertificateDer<'_>]>) -> DomainResult<PeerIdentity>;
// server.rs, client.rs, socket.rs, mdns.rs, transport.rs (Plan 03)
pub struct RustlsPeerServer;      // impl PeerServerControl
pub struct RustlsPeerConnector;   // impl PeerConnector
pub struct MdnsDiscovery;         // impl Discovery, `new() -> Self`
pub struct RustlsPeerTransportFactory; // impl PeerTransportFactory, `new() -> Self`
```

### App names (Plan 04, `crates/rocket-app/src/network_service.rs` and siblings)

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
// NetworkService also implements PeerHandler (through an Arc wrapper).
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
`pairingFinished`). Payload fields are snake_case.
