# Team network: send a collection to a paired node

Project 3 of the team network (CA-5, roadmap R-83). Builds on `docs/superpowers/specs/2026-11-05-team-network-discovery-pairing-design.md` (the MVP: discovery, pairing, presence). Status: design, not built.

## 1. Goal

A user sends one collection to a paired node, like AirDrop. It is a one-time copy with no ongoing link. The receiver is asked every time and nothing is written until they press Accept.

## 2. Decisions

| # | Decision | Why |
|---|---|---|
| 1 | One-time push only. No request-a-collection, no live sync, no merge. | Smallest useful feature. Sync needs change tracking and conflict handling that deserve their own project. |
| 2 | The payload is the existing Bruno export ZIP (`CollectionExportService::export_bruno`). Credentials and secret-named values are blanked by the existing rules. Environments are included only if the sender ticks "Include environments" (off by default). | No new scrubbing code. Nothing sensitive leaves the machine. |
| 3 | The receiver always gets a new collection in the active workspace. Nothing is overwritten. A name clash gets a suffix, for example `Orders (from Ana)`. | An import can never destroy data. |
| 4 | The receiver is asked for every offer (sender, name, size). An offer expires after 2 minutes. No per-peer auto-accept. | Pairing proves who the peer is, not that the user wants their data. Per-peer permissions stay a later idea (MVP spec, section 11). |
| 5 | Transport: chunks over the existing peer WebSocket (`/peer`) with new `PeerMessage` variants. No new HTTP route. | Reuses the paired-keys-only rule and the frame allow-list. Works whichever side dialled. |
| 6 | Received scripts stay untrusted. The imported collection goes through the existing collection trust gate. | A pushed collection cannot run code until the receiver trusts it. |

## 3. Success criteria

- A Linux node sends a collection to a paired Mac node (and the reverse). The receiver accepts and finds a new collection with the same requests and folders.
- Declining, ignoring for 2 minutes, a dropped connection, a damaged payload and an unsafe ZIP each end with a clear message and no collection created.
- A secret value in the source collection is not in the received files.
- A pushed script does not run before the receiver trusts the collection.
- The size limit is exact: 10 MiB passes, 10 MiB + 1 byte is refused before any offer.

## 4. Non-goals

- Requesting a collection from another node, live sync, merging into an existing collection, per-peer auto-accept.
- Sending flows, gRPC or WebSocket requests, client certificates or proxy settings. The existing export drops these and reports them. The send report says so.
- Sending anything other than a collection (environments alone, history, secrets).
- Binary frames or HTTPS download. Revisit if bundles outgrow the cap (MVP spec, section 11, item 4).

## 5. Flow

1. The sender picks **Send to...** in the collection context menu. The item is shown only when at least one paired node is online. A dialog lists online paired nodes and has the checkbox "Include environments".
2. The sender builds the ZIP with `export_bruno` and computes its SHA-256. If the ZIP is over 10 MiB the send stops with "This collection is too big to send (X MiB, limit 10)." and no offer is made.
3. The sender sends `CollectionOffer`: transfer id, collection name, size in bytes, chunk count, SHA-256.
4. The receiver shows a toast and a dialog with sender name, short device id, collection name and size, with Accept and Decline.
5. On Accept the receiver sends `OfferAnswer { accept: true }`. The sender streams `Chunk` frames. The receiver stages them in a temp file.
6. After the last chunk the receiver checks size and SHA-256, unzips into a temp folder with the safety checks in section 8, imports with the Bruno importer, and moves the result into place as a new collection. Both sides get a finished event.

A transfer id is a random 128-bit value created by the sender. It is not secret. It only ties frames together, and every frame is already inside the authenticated, paired TLS connection.

## 6. Protocol

New `PeerMessage` variants. The protocol version goes from 1 to 2. A version 1 node closes the connection on an unknown variant, as it does today, so both nodes must run version 2 to transfer:

```
CollectionOffer { transfer_id, name, size_bytes, chunk_count, sha256 }
OfferAnswer     { transfer_id, accept }
Chunk           { transfer_id, seq, data }        // data is base64
TransferAbort   { transfer_id, reason }           // reason: cancelled | failed | too_big | busy | expired
```

- Each frame stays under `MAX_FRAME_BYTES` (64 KiB). `CHUNK_BYTES` is 32 KiB of raw data, about 43 KiB after base64.
- A frame for an unknown, finished or expired transfer id is ignored.
- A `Chunk` before `OfferAnswer { accept: true }`, a `seq` out of order or past `chunk_count`, or data beyond `size_bytes` aborts the transfer with `failed`.
- An offer received by a node on a protocol version below 2 never happens: the sender checks the peer's `Hello` version before it offers, and shows "<name> needs a newer Rocket to receive this." if it is lower.

## 7. Limits

All constants live in `rocket-network/src/limits.rs`.

| Constant | Value |
|---|---|
| `MAX_TRANSFER_BYTES` | 10 MiB |
| `CHUNK_BYTES` | 32 KiB |
| `TRANSFER_OFFER_TTL` | 120 s (same as `PAIRING_TTL`) |
| `MAX_PENDING_INCOMING` | 3 pending offers across all peers |
| `MAX_ACTIVE_PER_PEER` | 1 transfer per peer at a time, in each direction |
| `OFFERS_PER_PEER_PER_MINUTE` | 5 (same rate as pairing requests) |
| `MAX_UNZIPPED_BYTES` | 50 MiB |
| `MAX_ZIP_ENTRIES` | 5000 |

An offer over a limit is refused with `TransferAbort { busy }` and the sender sees "<name> is busy. Try again in a minute."

## 8. Security

- The four new variants are added to the `PeerMessage` allow-list. The peer server still calls only `NetworkService` and `TransferService`. It does not touch collections directly. The import goes through the `CollectionImportPort` after the receiver pressed Accept.
- The receiver never writes into the workspace before Accept, and never overwrites an existing collection.
- ZIP safety on import: reject absolute paths, `..` segments, symlinks, more than `MAX_ZIP_ENTRIES` entries, or more than `MAX_UNZIPPED_BYTES` unzipped. Entries are written only inside the temp folder. The check runs before any file is written, using the declared sizes, and again with a counting writer while extracting.
- The temp import folder and the staged chunk file are deleted on success, abort, expiry and at app start.
- Imported scripts are untrusted until the receiver trusts the collection (existing collection trust gate).
- Audit: two new `AuditEventKind` values, `network_collection_sent` and `network_collection_received`, with the short device id and the collection name. The full device id and the payload are never logged.
- Nothing about the payload, the name or the sender's collection path is logged at info level.

## 9. Components

**`rocket-network`** (pure, no I/O): the four `PeerMessage` variants, the transfer state machine (offered, accepted, receiving, done, aborted, expired), and the constants in section 7.

**`rocket-app`**: `TransferService`, trait-first.
- Sender: ask `CollectionExportService` for the ZIP, hash it, offer, wait for the answer, stream chunks.
- Receiver: hold pending offers with their expiry, stage chunks, verify size and hash, import through a new `CollectionImportPort`.
- Publishes `TransferEvent` (`offerIncoming`, `progress`, `finished`) fire-and-forget, with the usual event rules (camelCase tag, snake_case fields, contract test).

**`rocket-infra`**: routes the four messages from the peer handler to `TransferService`, stages chunks in a temp file under `~/.rocket-api/`, and implements `CollectionImportPort` on top of `rocket-import` with the ZIP safety checks.

**`src-tauri`**: commands `network_send_collection`, `network_respond_transfer`, `network_cancel_transfer`, all added to `src-tauri/app-commands.txt`. Commands stay thin. A new event channel `network-transfer`.

**Frontend**:
- A `transfers` slice in `network-store` and a `network-transfer` handler in `useNetworkEventBridge`, using the same listener scope.
- `SendCollectionDialog` (online paired nodes, "Include environments" checkbox) built with `FormField` and `EmptyState` as in the design system map, and a "Send to..." item in the collection context menu.
- An incoming toast and `IncomingTransferDialog` (sender, short id, name, size, Accept, Decline, progress, expired view).
- No new Settings section in this version.

## 10. Errors

| Case | Sender sees | Receiver sees |
|---|---|---|
| Peer offline | The node is not offered in the dialog | n/a |
| Peer on protocol version 1 | "<name> needs a newer Rocket to receive this." | n/a |
| Receiver declines | "<name> declined." | n/a |
| Offer unanswered for 2 minutes | "The request expired." | "The request expired." (an open dialog switches to it) |
| Over 10 MiB | "This collection is too big to send (X MiB, limit 10)." | n/a |
| Too many pending offers | "<name> is busy. Try again in a minute." | n/a |
| Connection drops mid-transfer | "Sending failed. Try again." | "Receiving failed." (staged file deleted) |
| Hash or size mismatch | "Sending failed. Try again." | "The collection arrived damaged and was not imported." |
| Unsafe ZIP | "Sending failed. Try again." | "The collection was rejected because it is not safe to import." |
| Import fails | "<name> could not import it." | The failure text. Nothing is created |

The import happens in a temp folder and moves into place only when it succeeds, so a failure never leaves a half-written collection.

## 11. Testing

- Domain: the state machine for expiry, abort, duplicate offer, unknown id, out-of-order and surplus chunks. Exact boundary tests for 10 MiB and 10 MiB + 1 byte, and for the entry and unzipped-size limits.
- Service: `TransferService` with fakes for export, import and peer ports. Cases: accept, decline, expiry, hash mismatch, cancel by either side, a second offer from the same peer while one is active, three pending offers and a fourth.
- ZIP safety: absolute path, `..`, symlink entry, entry-count bomb, unzipped-size bomb, a ZIP whose declared sizes lie.
- End to end over the real transport (same style as `crates/rocket-app/src/network/e2e_tests.rs`): a push and import, decline, drop mid-transfer, a tampered chunk.
- Secret check: a collection with a literal token and a secret variable is pushed. The received files contain neither value.
- Wire contract tests pin the `TransferEvent` keys and the JSON shape of the four `PeerMessage` variants.
- Frontend: the store slice, the bridge handler, the send dialog (offline nodes hidden, environments off by default), the incoming dialog and its expiry while open, a duplicate `offerIncoming` (one row, one toast), a late response after the transfer ended.
- Manual (new section in `docs/manual-tests/team-network.md`): Linux to Mac push and the reverse, decline, 2-minute expiry, a name clash, a push of a collection with a secret and a script.

## 12. Open items for the plan

- The exact `CollectionImportPort` signature and whether it reuses the importer behind the existing Bruno import command.
- Whether the context-menu entry lives in the collection tree menu only, or also in the paired node menu of the Network panel.
- How the progress line is shown for a transfer that finishes in under a second (likely hidden below a 300 ms threshold).

## 13. Related

- MVP spec: `docs/superpowers/specs/2026-11-05-team-network-discovery-pairing-design.md` (section 8 route allow-list, section 11 later ideas).
- Export: `docs/features/collection-export.md`.
- Feature doc: `docs/features/team-network.md`.
