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
- Sending flows, gRPC or WebSocket requests, client certificates or proxy settings. The existing export drops these and reports them. The send command returns that report as a list of notes, and the sender sees them after pressing Send.
- Sending anything other than a collection (environments alone, history, secrets).
- Binary frames or HTTPS download. Revisit if bundles outgrow the cap (MVP spec, section 11, item 4).

## 5. Flow

1. The sender picks **Send to...** in the collection context menu. The item is shown only when at least one paired node is online. A dialog lists online paired nodes and has the checkbox "Include environments".
2. The sender builds the ZIP with `export_bruno` and computes its SHA-256. If the ZIP is over 10 MiB the send stops with "This collection is too big to send (X MiB, limit 10)." and no offer is made.
3. The sender sends `CollectionOffer`: transfer id, collection name, size in bytes, chunk count, SHA-256.
4. The receiver shows a toast and a dialog with sender name, short device id, collection name and size, with Accept and Decline.
5. On Accept the receiver sends `OfferAnswer { accept: true }`. The sender streams `Chunk` frames. The receiver stages them in a temp file.
6. After the last chunk the receiver checks size and SHA-256, validates the ZIP with the safety checks in section 8, and imports it with the Bruno importer as a new collection. It then sends `TransferResult` with the true outcome. The sender waits for that frame (up to `SENDER_RESULT_TIMEOUT`, 60 s) and only then shows "Sent" and writes the audit entry. Both sides get a finished event.

A transfer id is a random 128-bit value created by the sender. It is not secret. It only ties frames together, and every frame is already inside the authenticated, paired TLS connection.

## 6. Protocol

New `PeerMessage` variants. `PROTOCOL_VERSION` goes from 1 to 2. Today a socket whose Hello version differs from the local one is closed and the node shows as `NeedsUpdate` (`version_matches` in `connections.rs`, and `pair_request` refuses a different version). That rule stays, so a version 1 node and a version 2 node do not connect at all until both are updated. The team network is not released yet, so this costs nothing in the field:

```
CollectionOffer { transfer_id, name, size_bytes, chunk_count, sha256 }
OfferAnswer     { transfer_id, accept }
Chunk           { transfer_id, seq, data }        // data is base64
TransferAbort   { transfer_id, reason }           // reason: cancelled | failed | too_big | busy | expired
TransferResult  { transfer_id, outcome }          // receiver to sender after the last chunk; outcome: completed | damaged | rejected | failed
```

- Each frame stays under `MAX_FRAME_BYTES` (64 KiB). `CHUNK_BYTES` is 32 KiB of raw data, about 43 KiB after base64.
- A frame for an unknown, finished or expired transfer id is ignored.
- A `Chunk` before `OfferAnswer { accept: true }`, a `seq` out of order or past `chunk_count`, or data beyond `size_bytes` aborts the transfer with `failed`.
- The pinned test `PROTOCOL_VERSION == 1` in `limits.rs` changes to 2. The variant name `send_collection` is not used, because an existing test uses it as an example of an unknown type.

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

- The five new variants are added to the `PeerMessage` allow-list. The peer server still calls only `NetworkService`. `NetworkService` hands the five variants to a `TransferSink` (a trait in `rocket-network`) and never touches collections. The import goes through the `CollectionImportPort` after the receiver pressed Accept.
- The receiver never writes into the workspace before Accept, and never overwrites an existing collection.
- ZIP safety on import: a new strict validator (`rocket_import::transfer_zip::validate`) runs on the staged file before the importer sees it. It rejects absolute paths, `..` segments (`enclosed_name()` is `None`), symlink entries, more than `MAX_ZIP_ENTRIES` entries, and more than `MAX_UNZIPPED_BYTES` unzipped. The unzipped size is counted by actually decompressing every entry into a counter, so a lying declared size cannot pass. The existing extractor skips unsafe entries silently and has limits of 20000 entries and 512 MiB, so it stays as a second line of defence and is not the check.
- The temp import folder and the staged chunk file are deleted on success, abort, expiry and at app start.
- Imported scripts are untrusted until the receiver trusts the collection (existing collection trust gate).
- Audit: two new `AuditEventKind` values, `network_collection_sent` and `network_collection_received`, with the short device id and the collection name. The full device id and the payload are never logged.
- Nothing about the payload, the name or the sender's collection path is logged at info level.

## 9. Components

**`rocket-network`** (pure, no I/O): the five `PeerMessage` variants, the transfer state machine (offered, accepted, receiving, done, aborted, expired), the constants in section 7, and the `TransferSink` trait (`on_frame(&self, from: DeviceId, message: PeerMessage)` and `on_peer_gone(&self, device: DeviceId)`). `NetworkService` calls `on_peer_gone` when a peer socket ends and when a pairing is removed, so the receiver cleans up at once instead of waiting for an idle timer. A 120 s idle limit between frames stays as a fallback.

**`rocket-app`**: `NetworkService` gains `send_to(&self, device: &DeviceId, message: PeerMessage) -> DomainResult<()>` (a bounded send on the connection's outbound channel, which does not exist as a public call today) and `set_transfer_sink(&self, sink: Arc<dyn TransferSink>)`. `run_socket` passes the four new variants to the sink. `TransferService`, trait-first:
- Sender: ask `CollectionExportService::export_bruno(collection, include_environments)` for the ZIP bytes (it returns them in memory), hash it, offer, wait for the answer, stream chunks.
- Receiver: hold pending offers with their expiry, stage chunks, verify size and hash, import through a new `CollectionImportPort` (`import_zip(&self, zip_path: &Path, display_name: &str) -> DomainResult<ImportedCollection>`). The receiver picks the display name: the offered name, or `<name> (from <sender>)` if a collection with that name exists. The importer's own `-1`, `-2` suffix still applies if that also exists.
- Publishes `TransferEvent` (`offerIncoming`, `progress`, `finished`) fire-and-forget, with the usual event rules (camelCase tag, snake_case fields, contract test).

**`rocket-import`**: `transfer_zip::validate(path) -> ImportResult<()>`, the strict check in section 8.

**`rocket-infra`**: stages chunks in a temp file under `~/.rocket-api/` (`TransferStaging`), deleted on success, abort, expiry and at app start.

**`src-tauri`**: the `CollectionImportPort` adapter lives here, on top of `ImportServiceFactory` and `import_auto_from_zip`, because that factory is in `src-tauri/src/import_wiring.rs` and `rocket-infra` cannot depend on it. The importer takes a ZIP path, not bytes, which is why the chunks are staged to a file first. It names the collection from the ZIP file stem, so the adapter stages the file as `<display name>.zip`; Plan 2 has a probe test that pins this.

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
| Receiver declines | "<name> declined." | n/a |
| Offer unanswered for 2 minutes | "The request expired." | "The request expired." (an open dialog switches to it) |
| Over 10 MiB | "This collection is too big to send (X MiB, limit 10)." | n/a |
| Too many pending offers | "<name> is busy. Try again in a minute." | n/a |
| Connection drops mid-transfer | "Sending failed. Try again." | "Receiving failed." (staged file deleted) |
| Hash or size mismatch | "Sending failed. Try again." | "The collection arrived damaged and was not imported." |
| Unsafe ZIP | "Sending failed. Try again." | "The collection was rejected because it is not safe to import." |
| Import fails | "Sending failed. Try again." (the result frame carries only `failed`) | The failure text from the importer, carried in the finished event's `message`. Nothing is created |
| Sender cancels a pending offer | n/a | "<name> cancelled the transfer." The pending offer and its dialog are closed |
| Too big (event on the receiving or sending side) | "This collection is too big to send (X MiB, limit 10)." as an inline error in the send dialog. A later `too_big` event carries no size, so its toast says "(limit 10 MiB)". | n/a |

A failed import is cleaned up: the importer's own all-or-nothing behaviour plus a removal of any collection folder it created, so no half-written collection stays. The import runs inside the workspace, so the new folder can be briefly visible while it runs. Temp ZIP copies and staged chunk files are deleted on every exit path and at app start.

## 11. Testing

- Domain: the state machine for expiry, abort, duplicate offer, unknown id, out-of-order and surplus chunks. Exact boundary tests for 10 MiB and 10 MiB + 1 byte, and for the entry and unzipped-size limits.
- Service: `TransferService` with fakes for export, import and peer ports. Cases: accept, decline, expiry, hash mismatch, cancel by either side, a second offer from the same peer while one is active, three pending offers and a fourth.
- ZIP safety: absolute path, `..`, symlink entry, entry-count bomb, unzipped-size bomb, a ZIP whose declared sizes lie.
- End to end over the real transport (same style as `crates/rocket-app/src/network/e2e_tests.rs`): a push and import, decline, drop mid-transfer, a tampered chunk.
- Secret check: a collection with a literal token and a secret variable is pushed. The received files contain neither value.
- Wire contract tests pin the `TransferEvent` keys and the JSON shape of the five `PeerMessage` variants.
- Frontend: the store slice, the bridge handler, the send dialog (offline nodes hidden, environments off by default), the incoming dialog and its expiry while open, a duplicate `offerIncoming` (one row, one toast), a late response after the transfer ended.
- Manual (new section in `docs/manual-tests/team-network.md`): Linux to Mac push and the reverse, decline, 2-minute expiry, a name clash, a push of a collection with a secret and a script.

## 12. Open items for the plan

- Whether the importer really names a modern-format collection from the ZIP stem. The plan pins it with a test and falls back to renaming after import if it does not.
- Whether the context-menu entry lives in the collection tree menu only, or also in the paired node menu of the Network panel.
- How the progress line is shown for a transfer that finishes in under a second (likely hidden below a 300 ms threshold).

## 13. Related

- MVP spec: `docs/superpowers/specs/2026-11-05-team-network-discovery-pairing-design.md` (section 8 route allow-list, section 11 later ideas).
- Export: `docs/features/collection-export.md`.
- Feature doc: `docs/features/team-network.md`.
