# Team network: keep a sent collection in sync

Project 4 of the team network (CA-5, roadmap R-83). Builds on `docs/superpowers/specs/2026-11-06-team-network-send-collection-design.md` (send a collection, built) and `docs/superpowers/specs/2026-11-05-team-network-discovery-pairing-design.md` (discovery, pairing, presence). Status: design, not built.

## 1. Goal

Two paired nodes hold copies of one collection. Either user can change their copy. Either user can press Sync, review what would change on the other side and on their own side, and apply it. Changes on one side apply on their own. A request changed on both sides is resolved by the user with a diff. Nothing is written on a node until its user confirms.

## 2. Decisions

| # | Decision | Why |
|---|---|---|
| 1 | Two-way: both copies may change after the first send. | Teams edit the same collection from both ends. A one-way push would lose work. |
| 2 | Manual and reviewed. Either side presses Sync. The other node is asked each time. No automatic sync, no per-peer auto-accept. | Same trust model as the first send: pairing proves who the peer is, not that the user wants their data. |
| 3 | One link per pair of nodes. A third copy links to either node separately, with its own base. A hub is just a node with many links. | Simple to reason about and test. No shared history across more than two nodes. |
| 4 | Three-way compare per file against a stored base, using content hashes. No merging of text inside one file. | A request file is the unit. A half-merged YAML file is worse than a conflict the user can see. |
| 5 | A conflict is resolved per request: keep mine, keep theirs, keep both (the second copy gets a name suffix). A delete against an edit is also a conflict. | The user keeps the choice. Nothing is lost silently. |
| 6 | Content is the first send payload: requests, folders and their settings (headers, variables, scripts, auth, docs), in the Bruno export format. Credentials and secret-named values are blanked by the existing export rules. Environments are out. | No new scrubbing code. Plain environment values do not travel. |
| 7 | A link is created at the first send, only when the sender ticks "Keep in sync". Collections sent earlier, and collections that already exist on both sides, cannot be linked. | A link needs a base. Only the send knows the base for sure. |
| 8 | The base lives in app data, never in the collection folder. | A git push of the collection never carries it, and deleting a link never touches the collection. |
| 9 | Trust never moves. A synced file cannot enable Developer mode, extra script folders, agent runs, host env or vault access, and a sync never creates a trust record. | Same rule as the first send. Changed scripts keep running in the Safe sandbox. |
| 10 | Transport: new `PeerMessage` variants on the existing `/peer` WebSocket, with the chunk and ack window of the first send. No new HTTP route. `PROTOCOL_VERSION` goes from 2 to 3. | Reuses the paired-keys-only rule and the frame allow-list. A version 2 peer shows `NeedsUpdate`, as the existing rule says. |
| 11 | The wire files are the scrubbed export files, but a node never overwrites its file with them. It merges the received file into its local file: take the peer's value for every key, except keep a local filled value over a received blank only where the export blanks that slot (secrets), so a peer's deliberate clear of a plain value lands, and keep every key the export removes (`uid`, `info.uid`, `extensions.rocketapi`, `examples`) plus the root `opencollection.yml`'s `info.name`, which the sync leaves out of the hashed and sent bytes. The name is local-only: renaming a collection does not sync, and a receiver that imported it under a suffixed name (`Orders (from Ana)`) keeps that name. Request and folder `info.name` still sync. Trust grants live in `~/.rocket-api/trust.yml`, outside the collection. A file that is new to the node is written as received, but the owned keys (`uid`, `info.uid`, `extensions.rocketapi`, `examples`, and the root `info.name`) are stripped from it too, then it gets a derived uid on first load. Renames and moves are matched by content hash, not by uid. | The export blanks secrets and strips local-only keys. Writing it over a local file would wipe the receiver's secrets and settings. Hashing the scrubbed bytes is still right for deciding what changed. |

## 3. Success criteria

- A and B hold linked copies. A edits request R1, B edits request R2, A presses Sync, B accepts. Both copies then hold both edits and both bases are updated.
- A and B edit the same request R1 differently. The sync shows R1 as a conflict with a side-by-side diff. Each of the three choices gives the expected files on both sides.
- A renames or moves a request without editing it. After a sync it is a rename on B, not a delete plus an add.
- B holds a token in a request that A blanked in the export. After A edits that request's URL and syncs, B has the new URL and still has its own token. B's `uid`, `extensions.rocketapi` settings and saved examples are also unchanged.
- A deletes a request that B did not touch: B's copy loses it after B accepts. A deletes a request that B edited: conflict.
- Declining, ignoring for 2 minutes, a dropped connection, a damaged payload, an unsafe file and a base mismatch each end with a clear message and no file changed on either side.
- A crash during apply leaves the collection as it was before the sync or fully synced, never half merged.
- A secret value in either copy is not in the other copy after a sync.
- A synced script or auth change is marked in the review dialog before Accept, and a synced collection gets no trust record.
- The size limit is exact and counts ZIP bytes, as in the first send: a ZIP of 10 MiB passes in each direction, 10 MiB + 1 byte is refused before any offer.

## 4. Non-goals

- Automatic or background sync, per-peer permissions, auto-accept.
- Three or more copies sharing one history, a hub as the single source of truth.
- Linking copies that already exist without a base, and collections sent before this feature.
- Merging text inside one request file.
- Syncing environments, flows, gRPC or WebSocket requests, client certificates or proxy settings. The export drops these and reports them, as the first send does.
- Syncing anything other than a collection.

## 5. The link

A link joins one local collection to one collection on one paired node.

| Field | Meaning |
|---|---|
| `link_id` | Random 128-bit value made by the sender at the first send. Both sides store it. Not secret. |
| `peer_device_id` | The paired device on the other side. |
| `collection_root` | Canonical path of the local collection folder. |
| `collection_uid` | The `uid` in the local `opencollection.yml`. A link whose root no longer holds this uid is broken. |
| `base_manifest` | Map of relative path to SHA-256 of the file as last agreed. |
| `base_hash` | SHA-256 over the sorted lines `<path> NUL <hash> LF` of `base_manifest`. Both sides compare it at the start of a sync. |
| `state` | `in_sync`, `changes_waiting` (a local change since the base), `out_of_date` (a sync failed after the peer applied) or `broken`. |

- The base files are kept next to the record, as a copy of each file at its agreed version, in `~/.rocket-api/network/links/<link_id>/base/`. The record is `link.yml` in that folder (snake_case persistence struct, `atomic_write`, a store mutex). A damaged `link.yml` is renamed to `link.yml.corrupt-<secs>` once and the link is treated as broken, as the other network repositories do.
- Hashes cover the bytes of each request and folder file as the Bruno export writes them (secrets blanked, `uid` and Rocket-only keys removed), so a hash never depends on a secret or on a local-only key. The root `opencollection.yml` is hashed without its `info.name` (`rocket_export::sync_bytes`), on both the export side and the base read from the first-send ZIP. After the first linked import the receiver writes the sent root file over the imported one through the checked apply (the merge keeps its own `uid` and name), because the importer reorders root keys. Each node builds its manifest by running the export over its own collection in memory.
- A file is identified by its path. A rename or move is detected by content: a path that disappeared and a new path with the same hash, on the same side, is one rename. A renamed file that is also edited looks like a delete plus an add. This is a known limit.
- A blanked value cannot be told from an empty one, so a secret changed on one side is never a change by itself. It does not travel and does not make the file a conflict.
- Removing the pairing or choosing Unlink deletes the link and its base on that side. The other side learns of it at its next sync, which then ends with "link missing on the other side" and marks that link broken.

## 6. Flow

1. The user picks **Sync with Ana** in the menu of a linked collection. The item is shown only when the peer is paired and online.
2. The node builds its current manifest (path and hash of each exported file) and sends `SyncOffer { link_id, base_hash, manifest }`. No file content is sent yet.
3. The peer checks that it has the link, that the link is not broken and that `base_hash` equals its own. If not, it answers `SyncAbort { reason: base_mismatch | link_missing }`. The user sees "The two copies disagree about the last agreed version. Unlink and send the collection again."
4. The peer plans the sync. With its own manifest, the offered manifest and the base, for each path:

   | Base | Mine | Theirs | Result |
   |---|---|---|---|
   | h | h | h | nothing |
   | h | h | h2 | take theirs |
   | h | h2 | h | keep mine, send it |
   | h | h2 | h2 | nothing (same change) |
   | h | h2 | h3 | conflict |
   | h | deleted | h | take the delete |
   | h | deleted | h2 | conflict |
   | none | h | none | keep mine, send it |
   | none | none | h | take theirs |
   | none | h | h | nothing |
   | none | h | h2 | conflict (two different new files) |

   Rows with the roles swapped follow the same rules. Before the table, a path that was removed and a new path with the same hash on the same side are paired as a rename, and the peer is told to move its file when it is unchanged. A path that one side renamed and the other edited is a conflict.
5. The peer shows an incoming sync dialog: who is syncing, the collection name, counts of files added, changed and deleted on each side, which changed files contain scripts or auth, and the list of conflicts. It asks for Accept or Decline. The dialog warns that changed scripts run in the Safe sandbox.
6. Conflicts are decided once, by the user of the node that received the offer (the peer), because it sees both versions. Each conflict opens a side-by-side Monaco diff with keep mine, keep theirs and keep both. On Accept the peer sends `SyncPlan { link_id, transfer_id, wanted_paths, decisions, size_bytes, chunk_count, sha256 }`. `wanted_paths` are the files it needs from the starter, and `decisions` holds the outcome of each conflict. The starter never answers a conflict itself.
7. The starter answers with chunks of one ZIP of the wanted files, with the ack window of the first send. The peer then sends the starter a ZIP of the files the starter needs (the peer's changes plus the outcome of each conflict), the same way.
8. The starter's user sees a short confirm dialog before anything is written ("Apply N changes from Ana", with the same script and auth marks, no conflict choices). Each node then validates the received ZIP (section 7), applies its changes in one file transaction, stores the new base and updates the link. It sends `SyncResult { outcome }`. A node shows "Synced" only after the peer's result.
9. If one side fails after the other applied, the failing side keeps its old base and the link of both sides goes `out_of_date`. The next sync starts again from the stored bases and the planner finds the files already equal.

A transfer id is a random 128-bit value made by the sender of each ZIP. It ties frames together and is not secret.

## 7. Protocol and safety

New `PeerMessage` variants: `SyncOffer`, `SyncAbort`, `SyncPlan`, `SyncDecision`, `SyncResult`. File content goes in the existing chunk frames of the first send, with a transfer id that belongs to a sync. Limits are the first send's: 10 MiB per ZIP, 32 KiB chunks, at most 32 unacked, offers expire after 120 s, one active sync per link, at most the existing pending-incoming cap, rate limits per peer.

Safety rules, all enforced on the receiving node:

- Every received ZIP goes through the strict transfer ZIP validator (unsafe paths, symlinks, duplicate or conflicting entries, entry count, unzipped size, central directory checks).
- Every path in a plan must be a path the collection can hold: inside the collection root, no `..`, no `.rocket` folder, no device names. A path the peer names that is not in the offered manifest or the plan is refused.
- The SHA-256 and the size of the ZIP must match the plan before anything is applied.
- A sync cannot write `trust.yml` or any trust record, and a synced `opencollection.yml` cannot turn on Developer mode, extra script folders, agent runs, host env or RocketVault access. Those are separate switches the user sets.
- Both nodes must still be paired and online. Removing the pairing during a sync ends it with no change.
- The collection is locked (`collection_lock`) for the plan and apply, so a local save cannot interleave. The file transaction is the same journaled one the OpenAPI sync apply uses.
- The base is never read from the peer. Only `base_hash` crosses the wire.

## 8. Errors

Shown as plain sentences, each leaving both collections unchanged unless it says otherwise:

- Declined, expired, the peer left, the connection dropped, the collection is too big (X MiB, limit 10), a damaged payload, an unsafe file, the two bases differ, the link is missing on the other side, the collection folder or uid changed, the peer needs an update.
- The user cancels the conflict dialog: nothing is applied.
- Apply fails on one side: that side rolls back and keeps its base, the link goes `out_of_date`.

Errors never quote file contents or secret values, and the peer's message text is never shown as is.

## 9. Interface

- Send dialog: a "Keep in sync" checkbox, off by default.
- Collection menu: **Sync with <name>** and **Unlink** for linked collections, and a status line (in sync, changes waiting, out of date, broken).
- Incoming sync dialog and toast for the peer, a short confirm dialog for the starter, both in the style of the incoming transfer dialog. Cancel is disabled once all bytes are in.
- Conflict list and a conflict diff, using the Monaco diff viewer already used for git and contract diffs (`src/components/git/DiffViewer.tsx`). The diff is read-only. Only the three choices change anything.
- Network panel and Settings section: a list of links with Unlink.
- All UI uses shadcn/ui primitives and lucide-react icons, no raw buttons or inputs.

## 10. Audit

New audit kinds, each with an `outcome` like the send kinds: `network_sync_sent`, `network_sync_received`, `network_link_removed`. Fields are device ids and counts only, never file names or contents. The audit reader must accept a kind it does not know without failing, which `AuditLogRepository::head_hash` provides at startup. A sync that ended after the first chunk left the node is audited with its outcome.

## 11. Placement in the code

| Crate | Adds |
|---|---|
| `rocket-network` (pure) | Link types, manifest and `base_hash`, the three-way planner, conflict types, the `Sync*` messages and a sync state machine with the same limits and timers as the transfer machines. |
| `rocket-app` | `SyncService` in `src/network/` next to `TransferService`. Holds the ports as `Arc<dyn Trait>`, never imports `rocket-infra`. Uses `CollectionService` for reads and for the apply through the file transaction port. |
| `rocket-infra` | `FsLinkRepo` for `link.yml` and base files, the manifest builder (runs the export over the collection in memory and hashes each file), the YAML key-path merge of decision 11, and reuse of `FsFileTxFactory`. |
| `src-tauri` | Commands to start a sync, respond, decide a conflict, list and remove links, and the event channel. New commands go into `app-commands.txt`. IPC DTOs use camelCase. Persistence structs stay snake_case. |
| Frontend | Store slice and event bridge, the dialogs, the conflict diff, the menu items and the link list. |

The sync service and the persistence follow the existing rules: no `unwrap` in production paths, locks through `unwrap_or_else(|e| e.into_inner())`, and wire contract tests (`wire_contract_*`) pin every new event and message key.

## 12. Testing

- `rocket-network`: planner table tests for every row of the table in section 6 and the role swap, rename by hash, rename against edit, manifest and `base_hash` stability, state machine limits and timeouts.
- The file merge (decision 11): a local secret survives a received blank, a peer's clear of a plain value (docs, a header, the URL) lands, `export(merge(local, export(peer))) == export(peer)`, a received top level that is not a mapping is refused, a local-only key survives, the peer's value wins elsewhere, a new file is written as received, and a merge that would make the file invalid YAML is refused.
- `rocket-infra`: link repo round trip, damaged `link.yml`, base copy fidelity, manifest over a real folder.
- `rocket-app`: service tests with fakes for every outcome in section 8, a crash mid-apply (fault hooks of the file transaction), and trust never written.
- End-to-end in `rocket-app` over the real TLS transport on `127.0.0.1`, as the first send has: a clean two-way sync, a conflict with each of the three choices, a delete against an edit, a rename, a tampered payload, a base mismatch, an unlink on one side and a pairing removed during a sync.
- Frontend: store, bridge and dialog tests with Vitest, including a late event for another sync.
- Manual checks on two real machines are added as section 12 of `docs/manual-tests/team-network.md`.

## 13. Open questions

None that block the plan. To revisit after real use: linking existing copies by agreeing on a base (for example, a first sync with an empty base where every difference is a conflict), per-peer permissions for sync (MVP spec, section 11, item 5), and a larger size cap if bundles outgrow 10 MiB.
