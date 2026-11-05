# Team Network: Discovery and Pairing (MVP) Design

Status: approved design, MVP scope. No plan or code yet.
Date: 2026-11-05.
Roadmap: R-83, project CA-5 in `docs/roadmaps/contracts-ci-ai-roadmap.md`.

## 1. Goal

Rocket users on a team find each other the way AirDrop users do. A Network
panel with a search box lists nearby Rocket nodes. A user picks a node, pairs
once with a matching code, and from then on the two apps are trusted peers.

This spec covers discovery, presence and pairing only. Two later projects
build on it, each with its own spec:

- Project 3, send and receive: AirDrop-style sending of a collection, contract
  or pin as a sanitised snapshot, with an Accept dialog.
- Project 4, contract checks across nodes: "check my change against your
  contracts" and "who depends on this".

## 2. Decisions

Made by the user during brainstorming on 2026-11-05.

| Topic | Decision |
|---|---|
| Who sees a node | Everyone running Rocket on the same network. |
| What a stranger sees | Display name and initials only (plus the device id and protocol version needed to work). |
| When a node is visible | Always while Rocket runs, from first launch. A "Hide my node" switch exists in Settings. |
| Pairing check | Request, then the same 6-digit code on both screens, then both users accept. |
| Pairing lifetime | Until either side removes it. Paired nodes reconnect on their own. |
| Other networks | Same-network discovery, plus "Add by address" (host:port or a Tailscale name) with the same pairing. |
| Approach | mDNS (`mdns-sd`) plus Rocket's own HTTPS REST service with mutual TLS and device certificates. iroh and a typed-code PAKE were rejected. |
| Protocol style | Short HTTPS REST calls (JSON) for `/hello` and pairing. After pairing, one long-lived WebSocket over mutual TLS per paired pair, carrying JSON frames from one tagged Rust enum. Chosen over WebSocket-for-everything and gRPC streaming. |

## 3. Success criteria

1. Two teammates on the same Wi-Fi open Rocket and see each other in the
   search box within 5 seconds.
2. Pairing takes under a minute.
3. After both apps restart, the pair shows as paired and online with no user
   action.
4. A stranger on the same network can do nothing beyond `/hello` and sending a
   pairing request.
5. One node (the hub) can be paired with at least 30 teammates' nodes, and
   all of them show online at the same time.

## 3a. Many peers and the hub role

- Pairing is between two nodes, and a node can hold many pairings. Every
  node is equal in the protocol; there is no central server.
- A hub is a node that many teammates pair with. In the MVP it needs no
  special code: the hub user accepts one pairing per teammate, and the panel
  lists all of them under Paired.
- Teammates do not need to pair with each other to work with the hub.
- Limits sized for a team: up to 200 paired devices per node and 256
  concurrent connections. A hub with 30 teammates holds 30 open WebSockets,
  one per teammate.
- Several teammates may request pairing at the same time. Each gets its own
  session and its own notice; the hub's panel lists the open requests in
  order. The 5-per-minute pairing limit (section 8.3) still applies, so a
  larger onboarding takes a few minutes and extra requests see "busy, try
  again".
- What a hub does beyond being paired (holding contracts and pins, answering
  contract checks for everyone) belongs to projects 3 and 4. A hub setting
  ("This node is a team hub") is added there, not in the MVP.

## 4. Non-goals (MVP)

- Sending collections, contracts or pins (project 3).
- Contract checks between nodes (project 4).
- Visibility modes other than on and hidden.
- Connections across the internet, NAT traversal or relays.
- Moving a device identity to another machine.
- Groups or team-wide pairing.

## 5. Architecture

### 5.1 New domain crate `rocket-network`

Logic and traits only. No I/O.

- `DeviceId`: SHA-256 of the device's public key (SubjectPublicKeyInfo). The
  only thing a node is known by. Short form for display: the first 8 base32
  characters in two groups, for example `K7QF-2M9X`.
- `NodeProfile`: `display_name`, `initials`.
- `DiscoveredNode`: `device_id`, `profile`, `addresses`, `source` (Mdns or
  Manual), `last_seen`, `protocol_version`.
- `PairedDevice`: `device_id`, `name_at_pairing`, `paired_at`,
  `last_addresses`.
- `PairingSession`: state machine, see section 6.
- `Presence`: `Unknown`, `Online`, `Offline`, derived from the state of the
  peer WebSocket (section 7.3), with the reconnect attempt count and the next
  retry time.
- `PeerMessage`: the WebSocket frame type, one serde enum tagged by `type`.
  MVP variants: `Hello { version, profile }`, `Ping`, `Pong`, `Unpair`.
  Later projects add variants; unknown variants close the connection.
- `dialer(local_id, remote_id, reachability)`: pure rule that decides which
  side opens the WebSocket (section 7.3).
- Pure functions: `commitment(pk_b, pk_a, nb)`, `pairing_code(pk_a, pk_b, na,
  nb)`, `short_id(device_id)`.
- Ports (traits): `Discovery` (announce, browse), `PeerTransport` (serve,
  call), `DeviceIdentityStore` (load or create the key), `PairedDeviceRepository`,
  `BlockedDeviceRepository`, `ManualAddressRepository`.
- `NetworkEvent` enum: its own per-context event enum, following
  `.claude/rules/tauri-ipc-boundaries.md`, with `wire_contract_*` tests.
  Variants: `NodeDiscovered`, `NodeLost`, `PresenceChanged`,
  `PairingIncoming`, `PairingCodeReady`, `PairingFinished` (accepted, declined,
  expired, failed), `PairedRemoved`, `NetworkStatusChanged`.

Persistence structs keep snake_case. camelCase only on the IPC DTOs.

### 5.2 `rocket-infra`

- `MdnsDiscovery` on `mdns-sd`.
- `PeerServer` on axum (REST routes plus the `/peer` WebSocket upgrade through
  `axum::extract::ws`), rustls and tokio-rustls, with mutual TLS (section 8).
- `PeerClient`: HTTPS calls for `/hello` and pairing through the hyper client
  (`hyper-util`, already a dependency) over tokio-rustls, and
  `tokio-tungstenite` (already a dependency) with a rustls connector for the
  peer WebSocket. Both use the custom verifier from section 8.2.
- `PeerConnectionManager`: one task per paired device that dials or accepts
  the WebSocket, sends pings, reconnects with backoff and forwards
  `PeerMessage`s to `NetworkService`.
- `KeychainDeviceIdentity`: private key in the OS keychain, service
  `com.rocketapi.device-identity`. The self-signed Ed25519 certificate is made
  with `rcgen`, which moves from a dev-dependency to a normal dependency.
- `FsPairedDeviceRepo`, `FsBlockedDeviceRepo`, `FsManualAddressRepo`: YAML
  files under `~/.rocket-api/network/` (`paired.yml`, `blocked.yml`,
  `addresses.yml`). Machine-local. Never inside a workspace or collection,
  never committed.

### 5.3 `rocket-app`

`NetworkService` owns the lifecycle: start at launch, stop on exit, restart
when visibility or the port changes. It runs pairing sessions, updates
presence and publishes `NetworkEvent`s. It holds only `Box<dyn Trait>`.

### 5.4 `src-tauri`

Thin commands, each added to `generate_handler!` and `src-tauri/app-commands.txt`:

- `network_status`, `network_list_nodes`
- `network_set_visibility`, `network_set_profile`
- `network_add_address`, `network_remove_address`
- `network_request_pairing`, `network_respond_pairing`, `network_cancel_pairing`
- `network_remove_paired`, `network_unblock`

The peer server never reaches Tauri commands. It calls `NetworkService`
through its own route allow-list.

### 5.5 Frontend

- `src/components/network/`: `NetworkPanel` (search box, Paired and Nearby
  groups), `AddAddressDialog`, `PairingDialog` (code, Accept, Decline, Block),
  incoming-request notice, first-launch visibility card.
- A Network section on the Settings page: visibility, display name, port in
  use, blocked devices. Follow `docs/features/settings-page.md` for adding a
  section.
- A network icon in the status bar with the count of online paired nodes. It
  opens the panel.
- `src/stores/network-store.ts` (Zustand, narrow selectors), fed by the
  `NetworkEvent` listeners.
- shadcn/ui primitives and lucide-react icons only.

### 5.6 Data flow

```
mDNS or manual address -> MdnsDiscovery / probe -> NetworkService -> NetworkEvent -> Tauri bus -> network-store -> NetworkPanel
Stranger HTTPS call    -> PeerServer (route allow-list) -> NetworkService -> NetworkEvent (pairing prompt) -> UI
Paired WebSocket frame -> PeerServer /peer (mTLS paired check) -> PeerConnectionManager -> NetworkService -> NetworkEvent -> UI
UI action              -> Tauri command -> NetworkService -> PeerClient (HTTPS) or open WebSocket -> peer
```

## 6. Pairing protocol

A starts the pairing. B receives it.

### 6.1 Channel

Every connection is TLS 1.3. Both sides present their device certificate,
also on the open routes. A learns `PkB` and B learns `PkA` from the handshake,
bound to that connection. Nothing is trusted yet.

### 6.2 Messages

| Step | Call | Content |
|---|---|---|
| 1 | A -> B `POST /pair/request` | A's profile and protocol version. Reply: `pairing_id`, `Cb = SHA-256("rocket-pair-commit-v1" ‖ PkB ‖ PkA ‖ Nb)`. `Nb` is 32 random bytes kept by B. |
| 2 | A -> B `POST /pair/{id}/nonce` | `Na`, 32 random bytes. Reply: `Nb`. |
| 3 | A checks | `Cb` matches the revealed `Nb`. On mismatch the session is Failed. |
| 4 | Both compute | `code = SHA-256("rocket-pair-code-v1" ‖ PkA ‖ PkB ‖ Na ‖ Nb)`, first 8 bytes as a big-endian integer, mod 1,000,000, zero-padded to 6 digits, shown as `482 913`. |
| 5 | A -> B `POST /pair/{id}/decision` | `{ "accept": bool }`. |
| 6 | A -> B `GET /pair/{id}` | Polled every second. Reply: `pending`, `accepted`, `declined` or `expired`. |

B shows the incoming notice only after step 2 succeeds, with the name, short
device id, code and the buttons Accept, Decline and Block. B never calls A, so
pairing works when only A can reach B.

When both sides accepted, each saves the other as a `PairedDevice`, and the
peer WebSocket is opened (section 7.3).

### 6.3 States

`Requested -> Committed -> Revealed -> CodeShown -> (LocalAccepted |
RemoteAccepted) -> Paired`. Terminal: `Paired`, `Declined`, `Expired`,
`Cancelled`, `Failed`. Sessions live in memory only and expire 2 minutes after
`Requested`.

### 6.4 Why a node in the middle fails

It must use its own keys toward both sides, so the codes are computed from
different keys and differ. B commits to `Nb` before seeing `Na`, so the
attacker cannot search for values that make the codes match. One attempt has a
1 in 1,000,000 chance, and the rate limits in section 8.3 cap the attempts.

### 6.5 Removing a pairing

The remover deletes the entry and, if the WebSocket is open, sends
`PeerMessage::Unpair` and closes it. The peer deletes its entry too. An
offline peer gets `403 not-paired` on its next WebSocket upgrade, marks the
pairing as removed and tells its user.

### 6.6 Reinstall or lost key

A new key is a new device. The old entry shows offline until removed.
Teammates pair again. Rocket never links a new key to an old identity.

## 7. Discovery, presence and the panel

### 7.1 mDNS

- Service `_rocket._tcp.local.`. The TXT record holds `v` (protocol version),
  `id` (full device id, base32), `name` and `initials`. Nothing else.
- Default port 47821. If it is taken, the next free port in 47821 to 47830 is
  used and announced. Settings shows the port in use.
- Before a nearby node is listed, Rocket calls its `/hello` and checks that
  the certificate key hashes to the announced `id`. A node that fails is never
  listed.
- Entries are merged by device id across addresses. The own device id is
  filtered out.
- A goodbye packet removes a node at once. A silent node drops out at the
  mDNS record expiry (120 seconds).

### 7.2 Manual addresses

Up to 50 entries in `addresses.yml`. Each is probed with `/hello` every 60
seconds. A node that answers appears in Nearby, or in Paired when its key is
paired.

### 7.3 Presence

Each pair keeps one WebSocket at `wss://<host>:<port>/peer`, opened with
mutual TLS. Only paired, unblocked keys can upgrade.

- Who dials: when both sides can reach each other, the side with the lower
  device id dials. When only one side is reachable (for example, a manual
  address over VPN), the side that knows the address dials. If two
  connections still appear at once, the one dialled by the lower device id is
  kept and the other is closed, so there is only one connection per pair.
- First frame on a new connection is `Hello { version, profile }` from both
  sides. A version mismatch closes the connection and shows "Needs update".
- Online means the WebSocket is open. Each side sends `Ping` every 20
  seconds. Two missed pings or a closed socket means Offline.
- Reconnect after a drop with backoff: 1, 2, 4, 8 seconds and so on, up to 2
  minutes. A new mDNS sighting or a manual address change retries at once.
- Addresses tried: the mDNS addresses, the last address that worked, and the
  manual addresses. A new IP is accepted when the key matches and saved as the
  last address.

### 7.4 Hide my node

Hiding stops the mDNS announcement and refuses new pairing requests. Paired
nodes can still reach the node. Browsing still works.

### 7.5 Panel

- Status bar icon with the count of online paired nodes opens the panel.
- Search box at the top filters by name, initials or short device id.
- Groups: Paired (online first, offline greyed, menu: Remove) and Nearby
  (button: Pair).
- Each row: initials avatar, name, short device id, status chip (Online,
  Offline, Pairing…, Needs update).
- "Add by address" at the bottom.
- Open incoming requests are listed at the top until they expire.

## 8. Security

### 8.1 Route allow-list

| Route | Who |
|---|---|
| `GET /hello` | anyone |
| `POST /pair/request`, `POST /pair/{id}/nonce`, `POST /pair/{id}/decision`, `GET /pair/{id}` | anyone, bound to the TLS key that opened the session |
| `GET /peer` (WebSocket upgrade) | paired, unblocked keys only, else `403 not-paired` before the upgrade |
| anything else | `404` |

On the WebSocket, `PeerMessage` variants act as a second allow-list. Frames
are text JSON, at most 64 KiB. Binary frames, unknown variants or a frame
before `Hello` close the connection.

The peer server calls only `NetworkService`. In the MVP it never touches
collections, environments, scripts, history, RocketVault, the keychain
(except the device key) or `process.env`. Projects 3 and 4 add routes only
through their own spec and security review.

### 8.2 TLS and identity

- TLS 1.3 only, Ed25519 certificates, no certificate authority.
- A custom rustls verifier on both server and client side: on open routes any
  certificate is accepted and its key recorded; on paired routes the key must
  be paired and not blocked.
- The private key never leaves the keychain except into memory, and is never
  logged or written to a file.
- Fail closed: if the keychain is unavailable, networking is off and the
  panel and Settings say why. There is no key-file fallback. Any verification
  error counts as not paired.

### 8.3 Limits

- 256 concurrent connections, of which at most 32 may be unpaired at once.
  10 second header timeout, 16 KiB body limit on REST routes.
- One WebSocket per paired device. When a second one from the same key
  arrives, the rule in section 7.3 keeps one and closes the other.
- `/hello`: 30 requests a minute per IP.
- Pairing requests: 5 a minute across all strangers; extra requests get
  `503 busy` and no notice. One open session per remote device.
- Decline: 1 minute cooldown for that device. Block: refused until unblocked.
- Requests with an `Origin` header are refused (stops browser pages and DNS
  rebinding).

### 8.4 First launch

The default display name is the OS user name. A one-time card says "You're
visible on this network as '<name>'" with Edit name and Hide my node.

### 8.5 Logging and audit

Pairing outcomes, removals and blocks are logged with the short device id.
Nonces, codes, keys and certificates are never logged. The same three events
are recorded through `rocket-audit` (`SecurityAuditPublisher`), the
tamper-evident security log.

### 8.6 Accepted limits

- The device id is stable, so others on the network can see when a node is
  online. Answer: Hide my node.
- A paired device that turns hostile can only hold its WebSocket open and send
  `Hello`, `Ping`, `Pong` and `Unpair` in the MVP. Later projects must treat paired peers as possibly hostile.
- Network floods are out of scope beyond section 8.3.

### 8.7 Review

An Opus security review of the protocol and the peer server before merge,
using sections 6 and 8 as its checklist.

## 9. Error handling

| Case | What the user sees |
|---|---|
| Default port taken | Next free port, shown in Settings. |
| Listener cannot bind or firewall denied | Banner: "Others can't connect to you. Allow Rocket in your firewall settings." Outgoing connections still work. |
| mDNS unavailable | "Automatic discovery isn't available here." Manual addresses still work. |
| Pairing failed | "Pairing failed. Try again." No reason given. |
| Pairing expired | "The request expired." |
| Declined or blocked | "<name> declined." The two look the same to the sender. |
| Protocol version mismatch | Chip "Needs update", Pair disabled. |
| Keychain unavailable | Networking off, message in the panel and Settings. |
| Damaged `paired.yml`, `addresses.yml` | Kept as `.bak`, start empty. |
| Damaged `blocked.yml` | Kept as `.bak`, start empty, and show a warning. |
| Paired list full (200) | "You've reached the limit of 200 paired devices. Remove one to pair." |

## 10. Testing

- `rocket-network` unit tests: every state transition and refused transition,
  expiry, commitment check, code stability and formatting (leading zeros),
  key swap changes the code, device id and short id, the dialer rule, the
  reconnect backoff sequence, `PeerMessage` round trips and rejection of
  unknown variants, `wire_contract_*` tests for `NetworkEvent` and
  `PeerMessage`.
- `rocket-infra` tests with two in-process nodes on `127.0.0.1` and temp
  dirs: full pairing with matching codes and saved lists; a node in the middle
  gets different codes; a wrong `Nb` fails and saves nothing; a stranger's
  WebSocket upgrade on `/peer` gets 403; unknown path 404; `Origin` refused;
  6th pairing request busy; body over 16 KiB refused; frame over 64 KiB,
  binary frame and unknown variant close the socket; blocked device refused;
  both sides dialling at once leaves one connection; a dropped socket goes
  Offline after two missed pings and reconnects; unpair online and offline;
  one node paired with 30 in-process nodes shows all online and stays under
  the connection limit; the 201st pairing is refused with a clear
  message. mDNS: a spoofed TXT id is not listed; two addresses merge. If CI
  cannot run multicast, these use a fake `Discovery` and the real mDNS becomes
  a manual check. Keychain through an in-memory fake.
- `rocket-app` tests with fakes: start, stop, restart on visibility or port
  change; hidden node refuses pairing but serves paired presence; keychain
  failure turns networking off.
- Vitest: store reacts to each event; panel search, groups and chips; pairing
  dialog calls the right commands; first-launch card shows once.
- Manual checks in `docs/manual-tests/team-network.md`: two machines on one
  Wi-Fi; pairing; restart and reconnect; sleep and wake; network change; Add
  by address over VPN or Tailscale; declined firewall prompt; Wi-Fi with
  client isolation.

## 11. Observations and recommendations after the MVP

Ideas to revisit once the MVP runs with real users. Not part of this scope.

1. Measure locally first. Count discovery time, pairing success and failure
   reasons on the device only (no telemetry), and show them in a diagnostics
   view. Use the numbers to decide the next steps below.
2. "Paired only" visibility. Decision A (everyone sees me) is the simplest
   start. Corporate and public networks may push users toward a mode where
   only paired nodes see them. The device-key model supports it without
   protocol changes.
3. Tracking. Rotating the mDNS instance id and sending the stable id only
   inside TLS would stop passive tracking of who is online.
4. Binary frames for transfers. If project 3 sends large snapshots, move
   those frames to MessagePack or a separate HTTPS download, keeping JSON for
   control messages.
5. Per-peer permissions. When projects 3 and 4 add routes, give each paired
   device scopes (can send to me, can run contract checks) instead of all or
   nothing.
6. Admin policy. A machine-level policy file that turns networking off or
   locks it to "Add by address" only, for companies that ban discovery.
7. Moving to a new machine. An encrypted export of the device identity, or a
   "re-pair all" helper, so a laptop swap does not mean pairing with everyone
   again.
8. Remote teams. If teams need pairing across the internet, add iroh behind
   the same `DeviceId` and paired-list model, with self-hosted relays only.
9. Team invite for hubs. Pairing 30 teammates one by one means 30 code
   checks for the hub user. A "team invite" (the hub shows one code for a
   time window and many teammates join with it, each still confirmed in a
   list) would make hub setup faster. Needs its own security design.
10. Accessibility of the code. Offer reading the code as words and a larger
   display, since users compare it out loud.

## 12. Related

- `docs/roadmaps/contracts-ci-ai-roadmap.md` (CA-5)
- `docs/features/ipc-command-acl.md`
- `docs/features/collection-trust.md` (for project 3)
- `docs/features/settings-page.md`
- `src-tauri/src/mcp/tool_server.rs`, `src-tauri/src/mcp/auth.rs` (existing
  authenticated axum pattern)
- `crates/rocket-infra/src/callback_server.rs` (existing limits)
