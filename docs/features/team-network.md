# Team network

Rocket nodes on the same network find each other, pair once with a matching 6-digit code and stay paired until one side removes the pairing. Paired nodes can send each other a collection (see Send a collection). A sent collection can be kept in sync (see Keep a collection in sync). A provider can ask teammates whether a contract change breaks their requests (see Contract check). Spec: `docs/superpowers/specs/2026-11-05-team-network-discovery-pairing-design.md`. Send a collection: `docs/superpowers/specs/2026-11-06-team-network-send-collection-design.md`, plans in `docs/superpowers/plans/team-network-send/`. Keep in sync: `docs/superpowers/specs/2026-11-08-team-network-sync-collection-design.md`, plans in `docs/superpowers/plans/team-network-sync/`. Contract check: `docs/superpowers/specs/2026-11-09-team-network-contract-check-design.md`. Plans: `docs/superpowers/plans/team-network/`. Roadmap: R-83, project CA-5.

## What the user sees

- A radar icon in the status bar with the number of online paired nodes. It opens the Network panel.
- The Network panel (a popover): search box (name, initials or short id), open requests at the top, Paired (online first, offline greyed, menu with Remove) and Nearby (Pair button), and Add by address at the bottom.
- The pairing dialog: both screens show the same code, for example `482 913`. The node that started the pairing sees Accept and Cancel. The other node sees Accept, Decline and Block and also gets a toast.
- A one-time card after the first launch: "You're visible on this network as '<name>'." with Edit name, Hide my node and OK.
- Settings, Team network: Hide my node, display name, port in use, device id (short), linked collections with Unlink, blocked devices with Unblock, saved addresses with Remove, warnings.
- Linked collections also show in the Network panel under Paired and Nearby, with an Unlink button.

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
| Keep in sync | Protocol and planner in `crates/rocket-network` (`link.rs`, `planner.rs`, `events.rs` `SyncEvent`), `SyncService` and the frame router in `crates/rocket-app/src/network/`, the file merge in `crates/rocket-export/src/sync_merge.rs`, `FsLinkRepo` in `crates/rocket-infra`. Commands in `src-tauri/src/commands/network_sync.rs` (the `keep_in_sync` argument of the send is in `src-tauri/src/commands/network.rs`). Frontend: `src/components/network/SyncReviewDialog.tsx`, `SyncConflictDiff.tsx`, `IncomingSyncToast.tsx`, `UnlinkDialog.tsx`, `LinkList.tsx`, `src/hooks/useIncomingSyncNotice.tsx`, `src/hooks/useCollectionSyncLinks.ts`, `src/components/collections/CollectionSyncMenuItems.tsx`, `src/lib/network-sync-actions.ts`, `src/lib/network-sync-utils.ts`, the `syncReviews` slice and `links` in `src/stores/network-store.ts`, `handleNetworkSync` in `src/lib/network-event-bridge.ts`. |
| Data | `~/.rocket-api/network/` (`paired.yml`, `blocked.yml`, `addresses.yml`, `settings.yml`, `received.yml`) and `~/.rocket-api/network/links/<link_id>/` (`link.yml` and `base/`). Machine-local. The device key is in the OS keychain, service `com.rocketapi.device-identity`. |

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

Name: the receiver never overwrites. Every received collection is named `<name> (from <sender>)`, whether the offered name is free or not, so the source shows in the sidebar. The importer adds `-1`, `-2` if that name is taken. `receiver_display_name` builds the name and no longer looks at the existing names.

Source: after a successful import the receiver saves who sent the collection (sender name, short device id, date) in `~/.rocket-api/network/received.yml`, keyed by the uid of the collection file, so a rename of the folder keeps it. The file is machine-local and never part of an export, git or sync. `CollectionImportPort::record_origin` writes it (default no-op, real adapter in `src-tauri/src/import_wiring.rs`); a failed write is only logged and never changes the receive. The command `network_list_received` returns the entries. The sidebar shows them as a tooltip on the collection name, and the collection overview shows a "Received from <sender> on <date>" line. Collections received before this existed have no entry and show nothing.

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
- The importer unpacks a received ZIP into the private system temp folder (0700, random name). The startup purge covers the staging and ZIP-copy folders only, so after a crash the unpacked tree stays in the system temp folder until the OS clears it.
- The sonner toast action closes the waiting toast even when a cancel is refused. The Cancel then looks done while the transfer goes on (manual check 15).

## Keep a collection in sync

A collection that was sent can be linked, so the two copies can be merged on request. Sync is manual and reviewed. Nothing happens in the background.

**Make a link.** The Send dialog has a **Keep in sync** box, off by default. Only a collection sent now can be linked. Both sides store the link when the transfer completes.

**Sync.** The collection menu (the `...` menu and the right-click menu) shows a status line, **Sync with <name>** while the peer is paired and online, and **Unlink**. The flow follows section 6 of the spec:

1. The starter sends a manifest. The other node plans the merge and shows the review dialog with counts, script and auth marks, and the conflicts. A conflict has a read-only Monaco diff and the choices keep mine, keep theirs and keep both.
2. After the planner accepts, the starter sees a confirm dialog with the same counts and marks.
3. Each node applies the changes in one file transaction. "Synced" shows only after the peer's result.

**Frontend rules.**

- The backend owns the state. `syncReviews` is deduped by sync id and an unanswered entry expires after 120 seconds. A late event for another sync is ignored.
- The planner decides what is a conflict. There is no default choice. Keep both is hidden when a side has no file, and for `opencollection.yml` and `folder.yml` (the backend refuses it there).
- There is no Cancel once the user has answered (the backend refuses it with "The sync is already being applied."). Cancel stays on the starter's waiting toast.
- The diff uses `DiffViewer` through `SyncConflictDiff` and loads both versions with `network_sync_conflict_files`. If that fails, the choices still work.
- `refreshLinks` never throws. It runs at startup, on `linkChanged` and `finished`, when a collection menu opens and when the active workspace changes.
- Links are matched to a collection by name in the active workspace.
- A decline or cancel by the local user (Decline, the waiting toast Cancel, Unlink of a pending sync) shows no toast about itself.
- The review note says that changed scripts run with the current script access of the collection. A sync never changes trust settings.

**Commands.** `network_sync_start`, `network_sync_review`, `network_sync_confirm`, `network_sync_cancel`, `network_sync_conflict_files`, `network_list_links`, `network_unlink`, and `network_send_collection` with `keepInSync`. All are in `src-tauri/app-commands.txt`.

**Errors.** The texts are in `describeSyncOutcome` (`src/lib/network-event-bridge.ts`) and pinned by `network-event-bridge.sync.test.ts`. For `failed`, `base_mismatch`, `link_missing` and `unconfirmed` the toast shows the message the backend sent, with a fixed fallback. This covers the case clash ("The two copies have file names that differ only by letter case."), a case-only rename, an apply conflict, and "The link is missing on the other side. Unlink and send the collection again." An unconfirmed finish keeps the link `out_of_date`. If the next sync then ends in a base mismatch, the recovery is to unlink and send the collection again.

**Known open items (sync).** The waiting toast of the starter keeps its Cancel action and the text "Waiting for <name> to review the sync." after the planner accepted. A Cancel pressed then is refused by the backend ("The sync is already being applied."), shows that text as an error, and the sync goes on.

**Audit** (wire fields `device_id`, `collection`, `outcome`): `network_sync_sent` and `network_sync_received`, and `network_link_removed` (`device_id`, `collection`). Never file names or contents. The outcome shows in brackets unless it is `applied`.

## Contract check

A provider asks paired teammates, "does this contract change break your requests?" before accepting it. The check is advisory: it changes no contract state and writes no file on either node. Nothing blocks publish.

**Flow.**

1. The provider opens a contract view (the diff pane) that has changes since its signed snapshot and presses **Check with the team**. The button is disabled, with a tooltip, when the contract has no changes or the team network is off.
2. The picker lists only paired nodes that are online, with a checkbox each. With none online it shows an empty state and Send stays off.
3. Send builds one offer per node. Each offer expires after 10 minutes. The view shows a preview of the change list in the read-only Monaco diff and one row per teammate beside it.
4. The teammate gets a toast and a review dialog. For each change the dialog shows their matching local requests (names stay on that node), the verdict and a before and after diff. They press **Send result** or **Decline**.
5. A row reads Pass, Break with the field names, Not used, No answer yet, Declined, Busy, or "No answer, the check expired". The rows live in the stores, so they survive closing and reopening the view.

Results on the provider side are kept in memory and are lost when the app restarts. Changes to the method, the URL or a removed request are not covered by a check. A contract with only such changes is refused with "No changes can be checked."

**What leaves the node.**

- Provider to teammate: contract name and version, policy and the change list. Each change has the method, the path of the request (not the scheme, user info, host, query or fragment, and not a leading `{{base}}` variable), the field name, the change type, the old and new value cut to 256 characters, and the breaking flag. Values of fields with a sensitive name (tokens, keys, passwords, sensitive headers), auth credentials (`auth_detail`) and all body values are sent as `[hidden]`. Auth type names (bearer, basic, apikey and so on) are sent as they are, so a teammate can see an auth type change. The offer must fit 64 KiB, or the provider sees "Check fewer changes at a time." and nothing is sent.
- Teammate to provider: only a verdict (pass, break or not used) per change index, or declined or busy. No request name, path or collection. The provider maps the index to its own change list to show the field.

**Limits.**

- Request shape only. Response fields need R-80 and R-81.
- Removed or re-routed requests are not covered, because matching is by method and path.
- Matching is by method and path, so two unrelated APIs with the same path look alike.
- A teammate holds at most 1 open check per sending node and 3 in all. Any further offer gets Busy.
- An offline teammate gets no check. There is no queue. A node on an older build never answers and shows No answer yet.
- Only the active workspace of the teammate is read.

**Code.**

| Layer | Place |
|---|---|
| Frames, ids, caps | `crates/rocket-network` (`messages.rs`, `limits.rs`) |
| Service and router branch | `crates/rocket-app/src/network/` |
| Commands | `src-tauri/src/commands/network_contract_check.rs`: `network_contract_check_start`, `_review`, `_reply`, `_decline` (all in `src-tauri/app-commands.txt`) |
| API, store, event bridge | `src/lib/tauri-api.ts`, `src/stores/network-store.ts` (contract check slice), `src/lib/network-contract-check-utils.ts` |
| Consumer UI | `src/components/network/ContractCheckReviewDialog.tsx`, `ContractCheckChangeRow.tsx`, `ContractCheckChangeDiff.tsx` |
| Provider UI | `src/components/contracts/ContractCheckAction.tsx`, `ContractCheckResults.tsx`, mounted in `ContractDiffPane.tsx`. Rows are kept in `src/stores/contract-check-runs-store.ts`. |

**Frontend rules.** Peer text (teammate and contract names) renders as plain text. Backend error text is never echoed: the provider sees the fixed "Check fewer changes at a time." or "Could not start the check.", the teammate sees "Could not send. Try again." The diff uses a fixed file name, `contract-check.txt`.

**Audit** (wire kinds): `network_contract_check_sent` on the provider and `network_contract_check_received` on the teammate. Never request names or values.

## Decisions

- Visible to everyone on the network by default, with a Hide my node switch (spec section 2). Hiding stops the announcement and refuses new requests. Paired nodes still connect.
- Pairing lasts until removed. A new key is a new device.
- The deviations from the spec are listed in `docs/superpowers/plans/team-network/00-plan-index.md`: quarantined files instead of `.bak`, coarser events (`nodesChanged`), a settings file, the dial grace period, hyper's low-level client, the pairing request first calls `/hello`.
- Error and empty-state copy comes from spec section 9 and is pinned by tests.

## Limits

- 200 paired devices, 50 saved addresses, 256 connections (32 unpaired), 5 pairing requests a minute.
- A pairing request expires after 2 minutes. A declined device waits 1 minute.
- Same-network discovery only, plus Add by address. No relay, no internet pairing.
- Contracts are not sent, only checked (see Contract check). A collection can be sent as described in Send a collection. A sent collection can be kept in sync as described in Keep a collection in sync.
- A link joins two copies. A third copy links to either node separately.
- Sync is manual and reviewed: no automatic or background sync.
- Collections sent before this feature, and collections that exist on both sides, cannot be linked.
- Environments, flows, gRPC and WebSocket requests, client certificates and proxy settings are not synced.
- Renaming a collection does not sync. The link follows the name it had.
- A file rename that changes only letter case, and two files whose names differ only by letter case, cannot be synced. The sync ends with the backend text.
- If a sync ends unconfirmed and the next one ends in a base mismatch, unlink and send the collection again.
- Links are global, but their state is read in the active workspace. A link whose collection is not in the active workspace shows "Not in this workspace" in the link lists, with no unlink advice. Links are matched to collections by name, so a same-named collection in another workspace shows the menu entries of the link.

## How to test

```bash
bunx vitest run src/lib/__tests__/tauri-api.network.test.ts src/stores/__tests__/network-store.test.ts \
  src/lib/__tests__/network-event-bridge.test.ts src/lib/__tests__/error-text.test.ts \
  src/lib/__tests__/listen-scope.test.ts src/components/network src/components/status-bar \
  src/components/ui src/hooks/__tests__/useIncomingPairingNotice.test.tsx \
  src/hooks/__tests__/useAsyncAction.test.tsx src/components/settings \
  src/lib/__tests__/network-utils.test.ts src/components/collections/__tests__ src/components/audit \
  src/hooks/__tests__/useIncomingTransferNotice.test.tsx \
  src/stores/__tests__/network-store.sync.test.ts src/lib/__tests__/network-event-bridge.sync.test.ts \
  src/lib/__tests__/network-sync-utils.test.ts src/lib/__tests__/network-sync-actions.test.ts \
  src/hooks/__tests__/useIncomingSyncNotice.test.tsx
bun run tsc --noEmit
bun run check
```

Backend tests: `cargo test -j4 -p rocket-network`, `cargo test -j4 -p rocket-infra network`, `cargo test -j4 -p rocket-app network`. Real network checks: `docs/manual-tests/team-network.md`. Send backend tests: `cargo test -j4 -p rocket-network transfer`, `cargo test -j4 -p rocket-import transfer_zip`, `cargo test -j4 -p rocket-app network`. Real transfers: section 11 of `docs/manual-tests/team-network.md`. Sync backend tests: `cargo test -j4 -p rocket-network`, `cargo test -j4 -p rocket-app network`, `cargo test -j4 -p rocket-export sync_merge`. Real syncs: section 12 of the same file.
