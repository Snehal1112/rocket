# Team network

Rocket nodes on the same network find each other, pair once with a matching 6-digit code and stay paired until one side removes the pairing. Paired nodes can send each other a collection (see Send a collection). Checking contracts between nodes is a later project. Spec: `docs/superpowers/specs/2026-11-05-team-network-discovery-pairing-design.md`. Send a collection: `docs/superpowers/specs/2026-11-06-team-network-send-collection-design.md`, plans in `docs/superpowers/plans/team-network-send/`. Plans: `docs/superpowers/plans/team-network/`. Roadmap: R-83, project CA-5.

## What the user sees

- A radar icon in the status bar with the number of online paired nodes. It opens the Network panel.
- The Network panel (a popover): search box (name, initials or short id), open requests at the top, Paired (online first, offline greyed, menu with Remove) and Nearby (Pair button), and Add by address at the bottom.
- The pairing dialog: both screens show the same code, for example `482 913`. The node that started the pairing sees Accept and Cancel. The other node sees Accept, Decline and Block and also gets a toast.
- A one-time card after the first launch: "You're visible on this network as '<name>'." with Edit name, Hide my node and OK.
- Settings, Team network: Hide my node, display name, port in use, device id (short), blocked devices with Unblock, saved addresses with Remove, warnings.

## Where the code lives

| Layer | Place |
|---|---|
| Domain (ids, pairing state machine, presence, dialer, wire types, events, ports) | `crates/rocket-network` |
| Identity, YAML repos, TLS, peer server and client, mDNS | `crates/rocket-infra/src/network/` |
| `NetworkService` (state, pairing, presence, audit) | `crates/rocket-app/src/network/` (a folder) |
| Tauri commands and DTOs | `src-tauri/src/commands/network.rs` (commands are in `src-tauri/app-commands.txt`) |
| Wrappers, types and listeners | `src/lib/tauri-api.ts`, section "Team network" |
| State | `src/stores/network-store.ts` |
| Events | `src/lib/network-event-bridge.ts`, `src/hooks/useIncomingPairingNotice.tsx` |
| UI | `src/components/network/`, `src/components/status-bar/NetworkStatusItem.tsx`, `src/components/settings/sections/NetworkSection.tsx` |
| Shared pieces | `src/lib/error-text.ts`, `src/lib/listen-scope.ts`, `src/hooks/useAsyncAction.ts`, `src/components/ui/search-input.tsx`, `src/components/ui/initials-avatar.tsx`, `src/components/status-bar/StatusBarItem.tsx`, `StatusBarCount.tsx` |
| Send a collection | Protocol and state machines `crates/rocket-network` (`transfer.rs`, `limits.rs`), `TransferService` in `crates/rocket-app/src/network/`, strict ZIP check `crates/rocket-import/src/transfer_zip.rs`, staging in `crates/rocket-infra`, import adapter and commands in `src-tauri`. Frontend: `src/components/network/SendCollectionDialog.tsx`, `IncomingTransferDialog.tsx`, `IncomingTransferToast.tsx`, `src/hooks/useIncomingTransferNotice.tsx`, the `transfers` slice in `src/stores/network-store.ts`, `handleNetworkTransfer` in `src/lib/network-event-bridge.ts`. |
| Data | `~/.rocket-api/network/` (`paired.yml`, `blocked.yml`, `addresses.yml`, `settings.yml`). Machine-local. The device key is in the OS keychain, service `com.rocketapi.device-identity`. |

## Frontend rules

- The backend owns the state. The store mirrors it. `nodesChanged` and `statusChanged` events trigger a refetch of `network_list_nodes` and `network_status`. A sequence counter drops a stale response.
- `pairingIncoming` and `pairingFinished` feed `incoming` and `activePairing` directly. Event fields are snake_case.
- `incoming` is deduped by pairing id and each entry expires 120 seconds after it arrived. A repeated event neither adds a row nor extends the expiry.
- Only the node that started a pairing gets the "<name> declined." toast.
- The Add by address, Remove and Pairing dialogs are mounted outside the Popover. Closing the panel never closes them.
- Closing the pairing dialog of a pairing you started, before you pressed Accept, cancels it.
- The UI shows the short device id only. Nonces, keys, certificates and the full device id are never shown or logged.
- Mutations are store actions (`removePaired`, `addAddress`, `removeAddress`, `unblock`, `setHidden`, `saveName`, `markIntroSeen`). They call the IPC wrapper, reload the nodes or the status, and reject with the backend error. Components do not call `refresh()` after a call.
- `useAsyncAction` wraps one call with a busy flag and a toast (default) or inline error text. `errorText` and `toastError` clean the backend error text. `createListenerScope` handles the Tauri listener lifecycle.

## Send a collection

A one-time push of one collection to a paired node that is online. There is no ongoing link, no merge and no request-a-collection.

Flow:

1. On the sender, **Send to...** in the collection menu (the `...` menu and the right-click menu). The item shows only while at least one paired node is online. The dialog lists online paired nodes, has **Include environments** (off by default, so secrets do not leave unless the user ticks it) and a Send button.
2. The sender exports the collection as the Bruno ZIP (the same export, same blanking). A ZIP over 10 MiB stops the send with "This collection is too big to send (X MiB, limit 10)." and no offer is made.
3. The receiver gets a toast and a dialog with the sender, the short device id, the collection name and the size. Nothing is written before Accept. An unanswered offer expires after 2 minutes.
4. After Accept the sender streams the ZIP in 32 KiB chunks over the existing peer connection. The receiver acknowledges every 16th chunk and the last one, and the sender has at most 32 unacknowledged chunks out. The receiver checks size and SHA-256, validates the ZIP, imports it as a new collection in the active workspace and sends a result frame (`TransferResult`). The sender shows "Sent" only after that frame arrives (up to 60 seconds), so a damaged or rejected collection is never reported as sent. If the receiver goes away after the last chunk was acknowledged, or no result comes within 60 seconds, the sender shows "Sent, but the other node did not confirm." Both sides get a result toast.
5. After Send the sender sees "Sent to <name>. Waiting for them to accept." with a **Cancel** action. Once bytes move it reads "Sending to <name>..."; after the last chunk the Cancel action goes (a cancel then would be refused with "Already sent. Waiting for the result."). The receiver can cancel too, with **Cancel** in the receiving dialog. If the export dropped or blanked something (skipped flows, blanked tokens), the send dialog stays open and lists those notes until Done.

Limits:

- 10 MiB per transfer, 5000 entries and 50 MiB unzipped on import.
- An offer lives 120 seconds.
- 3 pending incoming offers across all peers, 1 active transfer per peer in each direction, 5 offers per peer per minute. An offer over a limit is refused with "<name> is busy. Try again in a minute."
- A transfer with no frame for 120 seconds fails. The result must arrive within 60 seconds of the last acknowledgement.

What leaves the machine: the export blanks credentials, auth fields and secret-named values. Request bodies, scripts and descriptions are sent as written and are not scanned. Environments are sent only when the user ticks **Include environments**. Flows, gRPC and WebSocket requests, client certificates and proxy settings are not sent (the export reports them).

Name clash: the receiver never overwrites. The offered name is used if it is free. Otherwise the collection is named `<name> (from <sender>)`, and the importer adds `-1`, `-2` if that is taken too.

Scripts and trust: a received collection has no trust record, so the collection trust gate (`docs/features/collection-trust.md`) gives it no Developer mode, no extra script folders, no agent runs, no host environment and no RocketVault access until the receiver allows them, even if the ZIP asks for Developer mode. Its pre-request and test scripts DO run, in the Safe sandbox, from the first Send. There is no separate script gate. The incoming dialog warns about this before Accept (no script count: the offer does not carry one).

Version: sending needs protocol version 2. A node still on version 1 shows as "needs update", and a pairing request from or to it is refused, until both nodes run a build with send.

Reload: a full reload of the window while an offer is pending drops it from view. It is not shown again; it expires after 2 minutes and the sender sees "The request expired." (a hot reload keeps it).

Wire outcomes: `completed`, `unconfirmed`, `declined`, `expired`, `cancelled`, `failed`, `too_big`, `busy`, `damaged`, `rejected`. Unconfirmed means the last chunk was acknowledged but no result arrived.

Errors the user sees (pinned by `network-event-bridge.test.ts`):

| Case | Sender | Receiver |
|---|---|---|
| Declined | "<name> declined." | nothing (own choice) |
| Not answered in 2 minutes | "The request expired." | "The request expired." (an open dialog switches to it) |
| Too many pending offers | "<name> is busy. Try again in a minute." | nothing |
| Connection dropped, or no frame for 2 minutes | "Sending failed. Try again." | "Receiving failed." |
| Connection dropped, or no result in 60 s, after the receiver acknowledged the last chunk | "Sent, but the other node did not confirm." (audited as unconfirmed) | its own result |
| Connection dropped or stalled after the last chunk was sent but before the receiver acknowledged it | "Sending failed. Try again." (audited as failed) | "Receiving failed." (after up to 2 minutes if a chunk was lost) |
| Sender went offline while the offer is pending | nothing | "The other node went offline." (an open dialog switches to it) |
| Hash or size mismatch | "Sending failed. Try again." | "The collection arrived damaged and was not imported." |
| Unsafe ZIP | "Sending failed. Try again." | "The collection was rejected because it is not safe to import." |
| Import fails | "Sending failed. Try again." | "The collection could not be imported." (fixed text; the importer's own error is only logged on the receiver and never goes over the wire) |
| Sender cancelled | nothing | "<name> cancelled the transfer." |
| Receiver cancelled while receiving | "<name> cancelled the transfer." | nothing |
| Collection name breaks the name rule | "This collection name cannot be sent. Rename it and try again." (inline in the send dialog) | nothing |

Frontend rules:

- `network-transfer` events feed the `transfers` slice: `offerIncoming` adds to `incomingTransfers`, `progress` updates `activeTransfers`, `finished` removes the transfer and shows the result toast. Fields are snake_case as sent.
- A `finished` event can arrive before the send command returns its transfer id. The store keeps it, and `trackOutgoing` returns false when the transfer already ended, so no waiting toast is shown.
- An offer is deduped by transfer id and expires 120 seconds after it arrived. A repeated event neither adds a row nor extends the expiry. An accepted offer has no timer.
- The store remembers the last 100 finished transfer ids, so a late progress or offer event and a repeated finish do not leave a ghost row or a second toast. Each result toast has one fixed id per transfer.
- Only the sender gets the "declined" toast.
- The `finished` event has `message` (string or null). Only the receiver's `failed` toast shows it, and a missing message becomes "Receiving failed." A pending offer that fails with a message keeps its open dialog and shows the message there.
- A `cancelled` finish shows "<name> cancelled the transfer." unless the local user pressed Cancel on that transfer (the store remembers it until the finish; a refused cancel is not remembered).
- An `unconfirmed` send shows an info toast, not an error.
- The "Waiting for them to accept." toast has its own id (`network-transfer-waiting-<id>`) and stays until the transfer finishes. The bridge turns it into "Sending to <name>..." on the first send progress, removes its Cancel when the last chunk is out, and dismisses it on the first `finished` event of that transfer.
- The progress line in the incoming dialog appears 300 ms after the first bytes, so a fast transfer never flashes it.
- Closing the incoming dialog without an answer leaves the offer pending and brings its toast back. There is no list of pending offers in the Network panel.
- The incoming toast has only a Review action. Inline errors in the dialogs use `StateBlock` (`tone='error'`).

Audit (wire fields `device_id`, `collection`, `outcome`; the full device id and the payload are never logged):

- The sender writes one `network_collection_sent` row for a send that ended after at least one chunk was sent, with the wire outcome, whatever it is (`completed`, `unconfirmed`, `failed`, `cancelled`, `damaged`, `rejected`). A send that ended before any chunk (declined, expired, busy, cancelled while the offer is pending, offline) writes no sender row.
- The receiver writes `network_collection_received` rows only for `completed`, `damaged` and `rejected`.
- The row shows the collection name, the short device id and, when not completed, the outcome in brackets.

Known open items:

- The staging purge at startup is not lock-aware. A second Rocket instance that starts during a receive could delete the first instance's staged file (manual check 21).
- The sonner toast action closes the waiting toast even when a cancel is refused. The Cancel then looks done while the transfer goes on (manual check 15).

## Decisions

- Visible to everyone on the network by default, with a Hide my node switch (spec section 2). Hiding stops the announcement and refuses new requests. Paired nodes still connect.
- Pairing lasts until removed. A new key is a new device.
- The deviations from the spec are listed in `docs/superpowers/plans/team-network/00-plan-index.md`: quarantined files instead of `.bak`, coarser events (`nodesChanged`), a settings file, the dial grace period, hyper's low-level client, the pairing request first calls `/hello`.
- Error and empty-state copy comes from spec section 9 and is pinned by tests.

## Limits

- 200 paired devices, 50 saved addresses, 256 connections (32 unpaired), 5 pairing requests a minute.
- A pairing request expires after 2 minutes. A declined device waits 1 minute.
- Same-network discovery only, plus Add by address. No relay, no internet pairing.
- No sending of contracts yet. A collection can be sent as described in Send a collection.

## How to test

```bash
bunx vitest run src/lib/__tests__/tauri-api.network.test.ts src/stores/__tests__/network-store.test.ts \
  src/lib/__tests__/network-event-bridge.test.ts src/lib/__tests__/error-text.test.ts \
  src/lib/__tests__/listen-scope.test.ts src/components/network src/components/status-bar \
  src/components/ui src/hooks/__tests__/useIncomingPairingNotice.test.tsx \
  src/hooks/__tests__/useAsyncAction.test.tsx src/components/settings \
  src/lib/__tests__/network-utils.test.ts src/components/collections/__tests__ src/components/audit \
  src/hooks/__tests__/useIncomingTransferNotice.test.tsx
bun run tsc --noEmit
bun run check
```

Backend tests: `cargo test -j4 -p rocket-network`, `cargo test -j4 -p rocket-infra network`, `cargo test -j4 -p rocket-app network`. Real network checks: `docs/manual-tests/team-network.md`. Send backend tests: `cargo test -j4 -p rocket-network transfer`, `cargo test -j4 -p rocket-import transfer_zip`, `cargo test -j4 -p rocket-app network`. Real transfers: section 11 of `docs/manual-tests/team-network.md`.
