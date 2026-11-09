# Team network: contract check between nodes (design)

Status: draft for review, 2026-11-09. Roadmap: R-83 (CA-5), follow-up to the send and keep-in-sync specs.

Related:

- `docs/superpowers/specs/2026-11-05-team-network-discovery-pairing-design.md` (nodes, pairing)
- `docs/superpowers/specs/2026-11-06-team-network-send-collection-design.md`
- `docs/superpowers/specs/2026-11-08-team-network-sync-collection-design.md` (frames, router, audit pattern)
- `docs/roadmaps/contracts-ci-ai-roadmap.md` (CA-5 decision of 2026-11-06)
- `docs/features/contract-collaboration-enhancements.md`

## 1. Goal

A provider who changes the request shape a contract covers can ask paired teammates, before publishing, whether the change breaks them. Each consumer node checks the change locally against its own requests and replies with only pass, break or not used per change. The consumer's collection never leaves their node.

The check is advisory. It never blocks publishing and never changes a contract state or any file.

## 2. Decisions

| # | Decision | Source |
|---|---|---|
| 1 | The provider starts the check and asks consumers. | User, 2026-11-09 |
| 2 | A consumer checks against its own collection requests. No consumer pin file. | User |
| 3 | First version covers request shape only (header, query, path, form, body, auth). Response fields are out of scope. | User |
| 4 | A consumer decides which requests apply by method plus URL path template, in all collections of the active workspace. | User |
| 5 | The consumer confirms before replying. Nothing leaves a node without a click. | User |
| 6 | The change is shown as a side-by-side diff in the existing Monaco diff viewer, with per-teammate results next to it. Only pass, break or not used results travel. | Roadmap CA-5 |
| 7 | Two new small frames, change list inline. No transfer machine. | Approach A |
| 8 | Extend the current unreleased protocol version 3. No bump. | Approach A |

Out of scope: response-schema diffing, a static scan of fields that requests, assertions or scripts read (R-80, R-81), consumer pin files, a trusted-node flag, blocking publish, contract state changes, a CLI check (R-79).

## 3. What exists and is reused

Read from the code on 2026-11-09. Verify before building.

- The contract model describes what a request sends. `crates/rocket-collection/src/contract/snapshot.rs` has `RequestSignatureSnapshot` (method, URL pattern, headers, query, body fields, form fields, auth). `diff.rs` has the pure function `diff_signature`, which returns `ChangelogEntry` values (`request_path`, `field`, `change_type`, `old_value`, `new_value`, `is_breaking`, method, http path). `BreakingChangePolicy` is Strict, Lenient or AdditiveOk.
- Field names in the change list follow the existing convention, for example `header:<name>`, `query:<name>`, `path:<name>`, `form:<key>`, `body`, `body:$.a.b`, `auth`.
- Peer frames: `PeerMessage` in `crates/rocket-network/src/messages.rs` (serde tag `type`, `deny_unknown_fields`, `MAX_FRAME_BYTES` 64 KiB). New frames follow the pattern of commit 2356ac4b: add variants and round-trip tests, add them to the allow-list in `connections.rs`, claim them in the router, handle them in a service, add commands.
- `is_sensitive_name` in `rocket-export` decides which names and values are secret.
- Audit: `crates/rocket-audit/src/event.rs`, `tag()`, and the control mapping in `control.rs`.
- `src/components/git/DiffViewer.tsx` already accepts `hideStageToggle`.

Does not exist: a contract-check frame, a "proposed change" type, a per-peer consumer mapping, a breaking-change classification for spec changes.

## 4. Flow

1. The provider opens a contract that has changes since its signed snapshot and picks "Check with the team". The change list is `diff_signature(snapshot, current requests)`. The provider chooses online paired nodes and sees a preview (diff on the left, empty results on the right).
2. For each chosen node the provider sends `ContractCheckOffer`. Each offer expires after 10 minutes.
3. The consumer node shows a toast, "`<name>` wants to check a change to `<contract>` (N changes)". The review dialog lists, per change, the local requests that match, the verdict, and a before and after diff. The user chooses Send result or Decline.
4. The consumer replies with `ContractCheckResult`. The provider shows per-teammate rows next to the diff. Until then a row reads "No answer yet".
5. Nothing else happens. The check changes no contract state and no file on either node.

## 5. Wire

Both frames use `deny_unknown_fields` and the existing size limit. They go in `PeerMessage` and in the router allow-list.

`ContractCheckOffer`:

| Field | Rule |
|---|---|
| `check_id` | A new id type, parsed like `SyncId`. Unique per offer. |
| `contract` | Name, at most 120 characters, no control characters. |
| `version` | At most 32 characters. |
| `policy` | `strict`, `lenient` or `additive_ok`. |
| `changes` | 1 to 200 entries. |

Each change entry: `method`, `path_template` (at most 512 characters), `field` (at most 128), `change_type` (`changed`, `added` or `removed`), `old_value` and `new_value` (optional, at most 256 characters each, cut if longer), `breaking` (bool). The offer must encode within 64 KiB. A provider that exceeds the cap sees "Check fewer changes at a time." and nothing is sent.

Secrets never leave the provider. Before sending, `old_value` and `new_value` become `[hidden]` for every field whose name or auth type `is_sensitive_name` flags (auth fields, tokens, sensitive headers). The change itself is still sent, only the value is hidden.

`ContractCheckResult`:

| Field | Rule |
|---|---|
| `check_id` | The id from the offer. |
| `outcome` | `answered`, `declined` or `busy`. |
| `rows` | For `answered` only. One row per change, in offer order: `{ index, verdict }`, verdict `pass`, `break` or `not_used`. |

The result never echoes a string from the offer. It carries no request name, body, collection name or path. The provider already knows the field names, so each row points back by index.

Protocol: both frames extend version 3. Version 3 has not shipped (v2 is the released protocol), so the hello check stays as it is. Both ends must run the same build, as for keep in sync. A build without the frames refuses the unknown type, and the provider then shows that node as "No answer yet" until the offer expires.

## 6. Hostile peers and safety

- A frame from a node that is not paired, or is blocked, is dropped.
- The consumer parses `check_id` and validates every length, the change count and control characters. A bad offer gets no reply and an audit-free drop with a debug log.
- A node holds at most one pending check per peer and three in total. Past that limit it replies `busy`.
- The provider accepts a result once, only from the node the offer went to, with the matching `check_id`, `rows.len()` equal to the change count and every `index` in range and unique. Anything else is dropped.
- A consumer never sends a result without the user's click. Declining sends `declined`.
- Errors and toasts use fixed sentences and never quote peer strings.
- The consumer reads its collections only through the existing collection port, never writes, and never reads another workspace.
- Audit kinds `network_contract_check_sent` and `network_contract_check_received`, each with `{ device_id, contract, outcome }`, are added to `event.rs`, `tag()` and the control mapping in `control.rs`, with the mapping test updated.

## 7. Consumer matching and verdict

Matching:

1. Normalize each change's method and `path_template` and each local request's method and URL: drop scheme, host and query; turn `{{var}}`, `:id` and `{id}` segments into a wildcard segment; compare segment by segment, case-sensitive.
2. Candidates are requests in all collections of the active workspace. If no request matches, the change is `not_used`.

Verdict per change, for each matching request, using the offer's policy and the existing breaking rules:

| Provider change | Consumer's request | Verdict |
|---|---|---|
| A field is added and the change is breaking | The request does not send it | `break` |
| A field is removed | The request still sends it | `break` |
| A value changes and the policy calls it breaking | The request's value differs from the new value | `break` |
| Any of the above but not breaking by policy | any | `pass` |
| Otherwise | The request already satisfies the new shape | `pass` |

When several requests match one change the verdict is the worst of them (`break` over `pass`). The review dialog lists the local matches. The reply carries only the verdict.

Exact comparison reuses `diff_signature` over the consumer's request signature and the provider's new signature, restricted to the changed fields. The implementation plan fixes the details and tests them with tables.

Limits: at most 200 changes per offer. The match scan is bounded to the active workspace. A check that finds no match for any change replies `answered` with every row `not_used`.

## 8. UI

Provider (contract view): a "Check with the team" action, a node picker with online paired nodes, a preview of the change list in the Monaco diff (`DiffViewer` with `hideStageToggle` and no repository id), and per-teammate result rows beside it: Pass, Break with the field, Not used, No answer yet, Declined, Busy.

Consumer (Network panel): an incoming toast and a review dialog built from shadcn primitives only, lucide icons only. For each change: the local matches (names stay on this node), the verdict, and a before and after request diff. Buttons: Send result, Decline. The dialog expires with the offer.

## 9. Backend shape

- `rocket-network`: `ContractCheckId`, the two frames and their helper types, caps in `limits.rs`, round-trip and hostile-frame tests.
- `rocket-app`: a `ContractCheckService` (provider side: build offer, track pending, accept result; consumer side: match, evaluate, hold for review, reply), a router branch, wiring in `src-tauri/src/lib.rs`, and the audit publisher calls. The service reads collections through the existing ports.
- `src-tauri`: thin commands `network_contract_check_start`, `_review`, `_reply`, `_decline`, an event on the existing `network-*` channel family, all registered in `src-tauri/app-commands.txt`.
- `rocket-audit`: the two kinds.
- Frontend: types, store slice, event bridge, the two dialogs, the contract-view action.

## 10. Testing

- Wire: round trips for both frames, every cap, hostile frames (over caps, duplicate or out-of-range rows, wrong peer, replay, unknown id).
- Redaction: sensitive values never appear in an offer.
- Matching and verdict tables, including variables in paths, wildcard segments and several matching requests.
- End to end: two real nodes over the real transport, offer, review, result, both Declined and Busy.
- Frontend: toast, both dialogs, result rows, fixed texts.
- Manual: a new section 13 in `docs/manual-tests/team-network.md`.

## 11. Limits and follow-ups

- Request shape only. Response fields need R-80 and R-81.
- Matching is by method and path, so two unrelated APIs with the same path look the same. The verdict says only that a request with that shape exists, and the reply reveals nothing more than pass, break or not used.
- Advisory only: nothing blocks publish. Model B enforcement is R-22.
- A consumer who is offline gets no check. There is no queue.
- A reply tells the provider a little about the consumer's usage. This is why the consumer confirms first.

## 12. Open decisions

None blocking. Candidate follow-ups: trusted-node auto-reply, an offline queue, a CLI check (R-79), response-field checks.
