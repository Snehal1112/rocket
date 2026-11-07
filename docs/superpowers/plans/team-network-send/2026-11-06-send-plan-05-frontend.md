# Team Network Send — Plan 5: Frontend, docs and manual checks

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 5 of 5.** Previous plan: `docs/superpowers/plans/team-network-send/2026-11-06-send-plan-04-tauri-e2e.md` (must be done and green; Plans 1 to 3 come before it). The three commands `network_send_collection`, `network_respond_transfer`, `network_cancel_transfer` and the event channel `network-transfer` exist.
**Next plan to run: none. Stop and show the user the whole change.**
**Recommended model: Sonnet.**

**Goal:** A Rocket user picks **Send to...** in a collection menu, chooses an online paired node, optionally ticks **Include environments**, and sends. The other user gets a toast and a dialog with the sender, short id, collection name and size, and presses Accept or Decline. Both sides get a clear result toast. Audit rows describe a sent and a received collection. The feature doc, the manual test file and the roadmap say what was built.

**Architecture:** The backend (Plans 1 to 4) owns every transfer. The frontend keeps a thin mirror in `network-store`: a `transfers` slice with `incomingTransfers` (offers waiting for an answer, each with the same 120 s expiry timer as pairing requests), `activeTransfers` (progress with direction) and `dialogTransferId`. The slice is fed only by the `network-transfer` event through `handleNetworkTransfer` in the existing `useNetworkEventBridge`, so it uses the same listener scope. The incoming side copies the pairing pattern: `useIncomingTransferNotice` shows one toast per offer with a fixed id, `IncomingTransferDialog` is mounted once in `App.tsx`, and both guard against late responses with `store().dialogTransferId === id` checks. The sending side is `SendCollectionDialog`, a sibling of `ExportCollectionDialog` inside the collection node, plus a menu item in both parallel menus of `CollectionNode.tsx` shown only while `selectOnlinePairedCount` is above zero.

**Tech Stack:** React 18, TypeScript, Zustand, shadcn/ui, lucide-react, sonner, Vitest + Testing Library, Bun.

## Spec

`docs/superpowers/specs/2026-11-06-team-network-send-collection-design.md`, sections 5 (flow), 9 (frontend components), 10 (error texts), 11 (frontend tests, manual checks). Binding names: `docs/superpowers/plans/team-network-send/00-plan-index.md` ("Plan 4 produces", "Plan 5 consumes"). Design rules: the design system map page (`index.html` in the `dsm` folder under `docs`; read it, this plan does not edit it): Usage guide, Form field, State block, Empty state, Skeleton, Dialog body. Second-review rulings: `00-plan-index.md`, "Rulings after the second review" (R1, R4, R6, R11, R12, R18, R19, R20 touch this plan).

## Global Constraints

- shadcn/ui primitives only. No raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`. There is no progress primitive, so progress is text. Choosing one of several nodes uses `RadioGroup` and `RadioGroupItem`. The checkbox uses `Checkbox` and `Label`.
- Icons from `lucide-react` only (`Send`, `Loader2`, `ShieldAlert`). No inline SVG.
- No CodeMirror and no Monaco is added. No `type='search'` anywhere.
- Zustand: narrow selectors only. A selector returns a primitive or a reference that already lives in the store (`find` on a store array). A selector never builds a new array: filter in the component with `useMemo`.
- Forms follow the design system map: `FormField` for every labelled control (group mode for the radio list and the checkbox row, see Rulings), `EmptyState` for "nothing to pick from", `StateBlock` for the inline form error, text sizes only `text-2xs`, `text-xs`, `text-sm` (dialog title `text-lg` from the primitive).
- Bun only: `bun run tsc --noEmit`, `bun run check`, `bun run lint` (auto-fix), `bunx vitest run <path>`. No Yarn or npm.
- Event payload fields are snake_case as sent (`transfer_id`, `size_bytes`). IPC DTO fields are camelCase. Invoke arguments are camelCase.
- Code comments are short full sentences that end with a period. No emojis.
- No commit during the plan. The user reviews the whole plan at the end. Then commit through `dev-workflow-skills:1-git-commit`, staging explicit paths only, conventional subjects (`feat(network): ...`, `test(network): ...`, `docs(network): ...`).
- Never show the full device id. The UI shows the short id only. The collection name is data: it is always rendered as text and never as HTML, and it is passed to the commands unchanged (no trim, no quoting, no normalisation).
- Verification per task: `bun run tsc --noEmit`, `bun run check` (if it reports format or import-order findings, run `bun run lint`, then `bun run check` again), and the task's `bunx vitest run <paths>`.

## Review Focus

Each line names the test that pins it.

1. **A collection name with a double quote** (`Orders "v2"`): Task 1 bridge test `keeps a double quote in the collection name` (toast text unchanged) and dialog test `shows a collection name with a double quote as given`; Task 2 dialog test `passes a name with spaces, quotes and unicode unchanged` and `puts a double quote in the title unchanged`.
2. **An offer from a sender whose display name is empty or blank**: Task 1 `peerName` test in `network-utils.test.ts`, bridge test `names a sender without a display name`, toast test `falls back when the display name is empty`, dialog test `names a sender without a display name`; Task 2 dialog test `names a node without a display name by its short id`.
3. **0 byte and huge sizes**: Task 1 `formatBytes` table in `network-utils.test.ts` (0, 1, 1023, 1024, 1048575, 10 MiB, 5 GiB, NaN, negative), toast test `shows a zero byte size`, dialog tests `shows 0 bytes` and `shows a huge size in MiB`.
4. **Two offers at once**: Task 1 store test `keeps two offers, each with its own timer`, hook test `shows one toast per offer, with different ids`, dialog test `shows the offer whose id the dialog holds`, bridge test `adds two offers at once as two rows`.
5. **The dialog is open while the peer disconnects**: Task 1 bridge test `closes the dialog of a transfer that failed` (receiver) and dialog test `closes with the transfer that went away`; Task 2 dialog test `disables Send when the chosen node goes offline` and `does not pick another node by itself when the chosen one goes offline`, plus the menu test `keeps the dialog mounted when the last node goes offline`.
6. **Duplicate `offerIncoming`** (spec section 11): Task 1 store test `dedupes by transfer id and does not extend the expiry`, hook test `shows no second toast for a duplicate offer`, bridge test `adds one row for a repeated offer`.
7. **Expiry while the dialog is open**: Task 1 dialog tests `switches to the expired view when the offer expires` (timer) and `keeps the dialog open on an expired finish`; bridge test `keeps an open dialog on expired`.
8. **Listener event after unmount**: Task 1 bridge tests `ignores transfer events after unmount` and `stops a transfer listener that finishes registering after unmount`.
9. **Late response after the transfer finished**: Task 1 dialog tests `a late Accept answer does not bring a finished offer back` and `a late Decline answer does not close the dialog of another offer`; store tests `ignores progress and offers that arrive after the finish`.
10. **Only the sender gets the declined toast**: Task 1 bridge tests `tells the sender that the peer declined` and `says nothing to the receiver who declined`.
11. **A repeated `finished` does not double the toast**: Task 1 bridge test `shows the result once, also when the finish event repeats`.
12. **Receiver failure text** (ruling 8): Task 1 bridge tests `shows the importer message when a receive fails with one`, `falls back when the message is null` and `falls back when the message is blank`, and `ignores the message on a send`.
13. **Cancel on the waiting toast** (ruling 9): Task 2 dialog tests `puts a Cancel action on the waiting toast`, `Cancel on the waiting toast cancels that transfer` and `shows the error when the cancel is refused`; Task 1 bridge tests `dismisses the waiting toast when the transfer finishes` and `dismisses the waiting toast also when the outcome shows nothing`; Task 1 toast test `has only a Review action` (no Later button).
14. **Notes from the send** (ruling 10): Task 2 dialog tests `lists the notes after Send and keeps the dialog open until Done`, `closes at once when there are no notes` and `shows a note with a double quote as given`.
15. **Inline errors use `StateBlock`**: Task 1 dialog test `shows the error in a state block and keeps the buttons when the answer fails`; Task 2 dialog test `shows the error inline, keeps the dialog open and enables Send again` (both check `data-slot='state-block'` and `data-tone='error'`).
16. **Audit row with a short id, wire names and the outcome** (index R6): Task 3 tests `describes a collection that was sent with a short id`, `describes a collection that was received with a short id`, `shows an unconfirmed send`, `shows a rejected and a damaged receive`, next to the full-id tests.
17. **Scripts run in the Safe sandbox** (index R1): Task 1 dialog test `warns that scripts run in the Safe sandbox before Accept` and `drops the script warning once the offer is accepted`.
18. **Unconfirmed send** (index R4): Task 1 bridge table row `announces send unconfirmed` (info toast "Sent, but the other node did not confirm.") and `says nothing for receive unconfirmed`.
19. **Who cancelled** (index R19): Task 1 bridge table row `announces send cancelled` ("Dana Roy cancelled the transfer."), bridge test `says nothing when the local user cancelled a send` / `... a receive`; store test `finishTransfer forgets the send and the local cancel`; Task 1 dialog test `Cancel while receiving cancels the transfer and remembers it was local`; Task 2 dialog test `remembers the send for the bridge and marks a Cancel as local`.
20. **A pending offer whose sender went offline** (index R19): Task 1 bridge test `switches an open pending offer to the offline text when the sender goes away`, store test `shows an end reason in the open dialog until it closes or another opens`, dialog test `shows why an unanswered offer ended`.
21. **The waiting toast follows the send** (index R19): Task 1 bridge tests `switches the waiting toast to sending on the first progress of a send`, `drops the Cancel action once the last chunk is out`, `leaves the toasts alone for a send it did not start or a receive`.
22. **No receive-side `too_big` text** (coverage M9): Task 1 bridge test `says nothing for receive too_big` (the backend never emits it: a refused offer emits nothing).
23. **Bad collection name** (index R15): Task 2 dialog test `shows the name rule error inline` (the backend text "This collection name cannot be sent. Rename it and try again.").

## Rulings and doubts (recorded for the ledger)

- **Ruling 1, radio list.** The node choice is a `RadioGroup` inside `FormField group`. The DSM says group mode is for "a radio set, a row list or a saved secret". `AgentConfigForm.tsx` (`ac-kind`) is the existing example, so the markup copies it. A `Select` would hide the node list, and `SegmentedControl` is for short either-or choices, not for a list of people.
- **Ruling 2, checkbox row.** "Include environments" is a `Checkbox` plus its own `Label` in a row, inside `FormField group label='Environments'` with the help text. A plain `FormField` would put a second label above the box, and two labels would give the box a doubled accessible name. The group has no `<label>`, so the box is named exactly "Include environments".
- **Ruling 3, empty cause.** No online paired node uses `EmptyState cause='needs-setup'`, not `unavailable`. `unavailable` is for a failed load. Here nothing failed: the user has to start the other computer or pair one. The menu item is hidden in that case, so the user reaches this view only when the last node goes offline while the dialog is open.
- **Ruling 4, accepted offers do not expire.** `markTransferAccepted` clears the 120 s timer. An accepted 10 MiB transfer on a slow link can run longer than 120 s, and the row must stay until the `finished` event.
- **Ruling 5, finished memory.** The store remembers the last 100 finished transfer ids and ignores progress or offers for them, and a second `finished`. This keeps a reordered event from leaving a ghost row or a doubled toast. The memory is bounded and cleared by `reset()`.
- **Ruling 6, closing the dialog without an answer.** The incoming toast has one action, Review (no "Later" button: dismissing the toast by swipe or timeout leaves the offer pending until it expires). The offer stays pending when the dialog closes without an answer, and its toast comes back (`useIncomingTransferNotice` watches `dialogTransferId`). There is no pending-offer list in the Network panel in this version (spec section 12, no new UI beyond section 9), so without this the user would lose the only way to answer.
- **Ruling 7, no sender progress line (amended by index R19).** Sender progress is kept in `activeTransfers` but no byte count is drawn. The sender sees the "Waiting for them to accept." toast, which switches to "Sending to <name>..." on the first send-side `progress` event and loses its Cancel action once a `progress` event reports `done_bytes >= total_bytes` (the backend refuses a cancel after the last chunk), and then the result toast. The 300 ms progress line (spec section 12) is receiver-only. The sender result toast ("Sent ...") arrives only after the receiver's `TransferResult` (index ruling 1), so the frontend needs no change for that.
- **Ruling 8, receiver failure text (index ruling 4, R19).** `finished` carries `message: string | null`. The receiver toast for `failed` shows `message` when it is not blank, else "Receiving failed.". A pending offer whose sender went offline arrives as `failed` with `message: "The other node went offline."`: the toast shows that text, and an open dialog for that offer stays open and shows it (store `dialogEndedText`) instead of closing. The sender never shows the message: its text for `failed` stays "Sending failed. Try again." (spec section 10, "Import fails").
- **Ruling 9, Cancel on the waiting toast (index ruling 6).** After a successful send the dialog shows an info toast "Sent to <name>. Waiting for them to accept." with id `network-transfer-waiting-<id>`, no auto close, and a Cancel action that calls `cancelNetworkTransfer`. A refused cancel (for example "Already sent. Waiting for the result.") shows its text as an error toast. The bridge dismisses the waiting toast on every first `finished` of that transfer, so it never outlives the transfer. A separate id from the result toast is used on purpose: sonner merges props when an id is reused, so a result toast with the same id would keep the Cancel action. The same merge is why the bridge passes `action: undefined` explicitly to drop the Cancel after the last chunk.
- **Ruling 10, send notes (index ruling 5).** `sendNetworkCollection` resolves `{ transferId, notes }`. With no notes the dialog closes after the waiting toast, as before. With notes the dialog stays open on a result view that lists them under "Not sent with the collection" and offers Done. The waiting toast shows in both cases.
- **Doubt B, too big on the event path.** The command refuses an oversize collection before any offer and returns the text with the real size, which the send dialog shows inline. The `too_big` outcome event has no size, so its toast says "This collection is too big to send (limit 10 MiB)." A receive-side `too_big` shows nothing: the backend never emits it (a refused offer emits no event, spec section 10), so no text is invented for it (coverage M9).
- **Ruling 11, unconfirmed (index R4).** `TransferOutcome` has `'unconfirmed'`. The sender shows the info toast "Sent, but the other node did not confirm." (info, not error: the receiver acknowledged every chunk and may well have imported; a stream lost before the final ack is `failed`, shown as "Sending failed."). The receiver never gets it.
- **Ruling 12, script warning (index R1).** There is no script gate. The pending-offer view of `IncomingTransferDialog` shows a note (`role='note'`, `ShieldAlert` icon): "This collection's scripts will run in the Safe sandbox when you send its requests. Developer mode, host environment and vault access stay off until you allow them." The offer carries no script count and nothing is unpacked before Accept, so the note has no count. `StateBlock` has no warning tone (`loading | empty | error`), so the note is a plain bordered paragraph.
- **Ruling 13, who cancelled (index R19).** The store keeps two small memories outside the state: `cancelledHere` (ids the local user cancelled, through the waiting toast or the receiving view) and `outgoing` (sends started here, with the node name and the phase `waiting | sending | sent`). A `cancelled` finish shows "<name> cancelled the transfer." in either direction unless the id is in `cancelledHere`. `finishTransfer` forgets both entries; `reset` clears both. A refused cancel leaves the id marked, which is harmless: the transfer then ends with another outcome.
- **Ruling 14, receiving view Cancel (index R19).** An accepted offer shows Cancel next to Done. Cancel marks the id as cancelled here and calls `cancelNetworkTransfer`; an error shows in the `StateBlock`.
- **Ruling 15, webview reload (index R20).** No list command. A full reload empties the store while the backend holds a pending offer; it is not shown again and expires after 2 minutes, and the sender sees "The request expired.". The feature doc says so. A hot reload keeps the store (the hook re-shows toasts).
- **Ruling 16, audit outcome (index R6).** Both audit kinds carry `outcome` (the `Outcome` wire name). The row text appends it in brackets when it is not `completed`: "Sent collection Orders to device K7QF-2M9X (unconfirmed)", "Received collection Orders from device K7QF-2M9X (rejected)".
- **Ruling 17, no design-system page edit (coverage M10).** The spec does not ask for design-system map changes, so this plan does not touch that page.
- **Ruling 18, docs index hunk (reality M-6).** `.claude/docs-index.md` already has an unrelated uncommitted edit (the `.claude/sprint-plan.md` bullet). The commit after this plan stages only this plan's hunk with `git add -p .claude/docs-index.md`; the sprint-plan bullet stays unstaged.

---

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `src/lib/tauri-api.ts` | modify | Team network section: `TransferDirection`, `TransferOutcome`, `NetworkTransferEvent` (with `message`), `SendStarted`, 3 wrappers, `onNetworkTransfer` |
| `src/lib/tauri-api.ts` (`AuditEventKind` union) | modify | Two kinds with wire names (`device_id`, `collection`, `outcome`) |
| `src/lib/__tests__/tauri-api.network.test.ts` | modify | Wrapper and listener tests |
| `src/lib/network-utils.ts` | modify | `formatBytes`, `peerName`, `transferResultToastId`, `transferWaitingToastId` |
| `src/lib/__tests__/network-utils.test.ts` | modify | Tests for the three |
| `src/stores/network-store.ts` | modify | `transfers` slice (with `dialogEndedText`, the `outgoing` and `cancelledHere` memories), `onlinePairedNodes` |
| `src/stores/__tests__/network-store.test.ts` | modify | Slice tests |
| `src/test/network-fixtures.ts` | modify | `makeIncomingTransfer` |
| `src/lib/network-event-bridge.ts` | modify | `handleNetworkTransfer` (receiver `message`, unconfirmed, who cancelled, waiting toast phases and dismiss), third listener |
| `src/lib/__tests__/network-event-bridge.test.ts` | modify | Mock gains `onNetworkTransfer`, new describe |
| `src/components/network/IncomingTransferToast.tsx` | create | Body of the incoming toast (Review only) |
| `src/hooks/useIncomingTransferNotice.tsx` | create | One toast per pending offer |
| `src/components/network/IncomingTransferDialog.tsx` | create | Offer, script warning, Accept, Decline, receiving line with Cancel, ended view (expired or the end reason), inline error in `StateBlock` |
| `src/components/network/__tests__/IncomingTransferToast.test.tsx`, `IncomingTransferDialog.test.tsx` | create | Component tests |
| `src/hooks/__tests__/useIncomingTransferNotice.test.tsx` | create | Hook tests |
| `src/App.tsx` | modify | Mount the hook and the dialog |
| `src/components/network/SendCollectionDialog.tsx` | create | Node choice, Include environments, Send, waiting toast with Cancel, notes view |
| `src/components/network/__tests__/SendCollectionDialog.test.tsx` | create | Dialog tests |
| `src/components/collections/CollectionNode.tsx` | modify | "Send to..." in both menus, dialog sibling |
| `src/components/collections/__tests__/CollectionNode.send.test.tsx` | create | Menu tests |
| `src/components/audit/AuditEventRow.tsx` | modify | Two summary cases with the outcome |
| `src/components/audit/__tests__/AuditEventRow.network.test.tsx` | modify | Seven row tests (full id, short id, odd name, unconfirmed, rejected and damaged) |
| `docs/features/team-network.md` | modify | "Send a collection" section, limits, test list |
| `docs/manual-tests/team-network.md` | modify | Section 11 |
| `docs/roadmaps/roadmap.md`, `docs/roadmaps/contracts-ci-ai-roadmap.md` | modify | R-83 and CA-5 text |
| `.claude/docs-index.md` | modify | Pointer text mentions send (stage only this hunk, ruling 18) |

---

## Task 1: Types, wrappers, store slice, bridge, incoming toast and dialog

**Files:**
- Modify: `src/lib/tauri-api.ts`, `src/lib/__tests__/tauri-api.network.test.ts`, `src/lib/network-utils.ts`, `src/lib/__tests__/network-utils.test.ts`, `src/stores/network-store.ts`, `src/stores/__tests__/network-store.test.ts`, `src/test/network-fixtures.ts`, `src/lib/network-event-bridge.ts`, `src/lib/__tests__/network-event-bridge.test.ts`, `src/App.tsx`
- Create: `src/components/network/IncomingTransferToast.tsx`, `src/components/network/IncomingTransferDialog.tsx`, `src/hooks/useIncomingTransferNotice.tsx`, and the three test files under `__tests__`

**Interfaces:**
- Consumes: commands `network_send_collection(deviceId, collection, includeEnvironments) -> SendStartedDto { transferId, notes }`, `network_respond_transfer(transferId, accept)`, `network_cancel_transfer(transferId)`; channel `network-transfer` (its `finished` event has `message: string | null`); `createListenerScope`, `useAsyncAction`, `errorText`.
- Produces: `TransferDirection`, `TransferOutcome`, `NetworkTransferEvent`, `SendStarted` (`{ transferId: string; notes: string[] }`), `sendNetworkCollection` (returns `Promise<SendStarted>`), `respondNetworkTransfer`, `cancelNetworkTransfer`, `onNetworkTransfer`; `formatBytes`, `peerName`, `transferResultToastId`, `transferWaitingToastId`; store `IncomingTransfer`, `ActiveTransfer`, `incomingTransfers`, `activeTransfers`, `dialogTransferId`, `addIncomingTransfer`, `removeIncomingTransfer`, `markTransferAccepted`, `setTransferProgress`, `finishTransfer`, `openTransferDialog`, `closeTransferDialog`, `dialogEndedText`, `endTransferDialog`, `trackOutgoing`, `advanceOutgoing`, `markCancelledHere`, `wasCancelledHere`, `onlinePairedNodes`; `handleNetworkTransfer`; `IncomingTransferToast`, `incomingTransferToastId`, `useIncomingTransferNotice`, `IncomingTransferDialog`.

### 1a. Types and wrappers

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing wrapper tests**

In `src/lib/__tests__/tauri-api.network.test.ts`, replace the import list with this one (names stay alphabetical):

```ts
import {
  addNetworkAddress,
  cancelNetworkPairing,
  cancelNetworkTransfer,
  getNetworkStatus,
  listNetworkAddresses,
  listNetworkBlocked,
  listNetworkNodes,
  markNetworkIntroSeen,
  onNetworkChanged,
  onNetworkPairing,
  onNetworkTransfer,
  removeNetworkAddress,
  removeNetworkPaired,
  requestNetworkPairing,
  respondNetworkPairing,
  respondNetworkTransfer,
  sendNetworkCollection,
  setNetworkProfile,
  setNetworkVisibility,
  unblockNetworkDevice,
} from '../tauri-api';
```

Append this block at the end of the file, after the last `});`:

```ts
describe('send a collection', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(listen).mockReset();
  });

  it('sends camelCase arguments and returns the transfer id and the notes', async () => {
    const started = {
      transferId: '0123456789abcdef0123456789abcdef',
      notes: ['Flows: skipped', 'Auth token: blanked'],
    };
    vi.mocked(invoke).mockResolvedValue(started);
    await expect(sendNetworkCollection('DEV1', 'Orders "v2" café', true)).resolves.toEqual(started);
    expect(invoke).toHaveBeenLastCalledWith('network_send_collection', {
      deviceId: 'DEV1',
      collection: 'Orders "v2" café',
      includeEnvironments: true,
    });
  });

  it('answers and cancels a transfer with camelCase arguments', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await respondNetworkTransfer('t1', false);
    expect(invoke).toHaveBeenLastCalledWith('network_respond_transfer', {
      transferId: 't1',
      accept: false,
    });
    await cancelNetworkTransfer('t1');
    expect(invoke).toHaveBeenLastCalledWith('network_cancel_transfer', { transferId: 't1' });
  });

  it('passes a rejection through unchanged', async () => {
    vi.mocked(invoke).mockRejectedValue('Invalid input: That node is offline.');
    await expect(sendNetworkCollection('DEV1', 'Orders', false)).rejects.toBe(
      'Invalid input: That node is offline.',
    );
  });

  it('listens on network-transfer and hands over the payload', async () => {
    vi.mocked(listen).mockResolvedValue(() => undefined);
    const handler = vi.fn();
    await onNetworkTransfer(handler);
    expect(listen).toHaveBeenCalledWith('network-transfer', expect.any(Function));
    const callback = vi.mocked(listen).mock.calls[0]?.[1];
    if (!callback) throw new Error('listener was not registered');
    (callback as (event: { payload: unknown }) => void)({
      payload: { type: 'progress', transfer_id: 't1' },
    });
    expect(handler).toHaveBeenCalledWith({ type: 'progress', transfer_id: 't1' });
  });
});
```

- [ ] **Step 3: Run the tests, expect RED**

Run: `bunx vitest run src/lib/__tests__/tauri-api.network.test.ts`
Expected: FAIL, `sendNetworkCollection is not a function` (or similar for the other new names). The older tests in the file still pass.

- [ ] **Step 4: Add the types and wrappers**

In `src/lib/tauri-api.ts`, find the last wrapper of the "Team network" section:

```ts
export const onNetworkPairing = (
  handler: (event: NetworkPairingEvent) => void,
): Promise<UnlistenFn> => listen<NetworkPairingEvent>('network-pairing', (e) => handler(e.payload));
```

and append directly after it:

```ts

export type TransferDirection = 'send' | 'receive';

/** How a transfer ended. The values are the snake_case `Outcome` of `rocket-network`. */
export type TransferOutcome =
  | 'completed'
  | 'unconfirmed'
  | 'declined'
  | 'expired'
  | 'cancelled'
  | 'failed'
  | 'too_big'
  | 'busy'
  | 'damaged'
  | 'rejected';

/** Channel `network-transfer`. Fields are snake_case as sent. */
export type NetworkTransferEvent =
  | {
      type: 'offerIncoming';
      transfer_id: string;
      device_id: string;
      short_id: string;
      display_name: string;
      collection_name: string;
      size_bytes: number;
    }
  | {
      type: 'progress';
      transfer_id: string;
      direction: TransferDirection;
      done_bytes: number;
      total_bytes: number;
    }
  | {
      type: 'finished';
      transfer_id: string;
      direction: TransferDirection;
      outcome: TransferOutcome;
      display_name: string;
      collection_name: string;
      /** The importer's failure text on the receiving side, else null. */
      message: string | null;
    };

/** Result of a started send. `notes` are human lines for what the export dropped or blanked. */
export interface SendStarted {
  transferId: string;
  notes: string[];
}

/** Resolves once the offer is on its way. */
export const sendNetworkCollection = (
  deviceId: string,
  collection: string,
  includeEnvironments: boolean,
): Promise<SendStarted> =>
  invoke<SendStarted>('network_send_collection', { deviceId, collection, includeEnvironments });

export const respondNetworkTransfer = (transferId: string, accept: boolean): Promise<void> =>
  invoke<void>('network_respond_transfer', { transferId, accept });

export const cancelNetworkTransfer = (transferId: string): Promise<void> =>
  invoke<void>('network_cancel_transfer', { transferId });

export const onNetworkTransfer = (
  handler: (event: NetworkTransferEvent) => void,
): Promise<UnlistenFn> =>
  listen<NetworkTransferEvent>('network-transfer', (e) => handler(e.payload));
```

- [ ] **Step 5: Run the tests, expect GREEN**

Run: `bunx vitest run src/lib/__tests__/tauri-api.network.test.ts`
Expected: PASS, all tests in the file.

### 1b. Helpers

- [ ] **Step 6: Write the failing helper tests**

In `src/lib/__tests__/network-utils.test.ts` replace the import line with:

```ts
import {
  formatBytes,
  formatPairingCode,
  peerName,
  shortDeviceId,
  transferResultToastId,
  transferWaitingToastId,
} from '@/lib/network-utils';
```

Append at the end of the file:

```ts
describe('formatBytes', () => {
  it.each([
    [0, '0 bytes'],
    [1, '1 byte'],
    [1023, '1023 bytes'],
    [1024, '1 KiB'],
    [1536, '1.5 KiB'],
    [2048, '2 KiB'],
    // Rounds up to the next unit instead of showing 1024 KiB.
    [1048575, '1 MiB'],
    [1048576, '1 MiB'],
    [10 * 1024 * 1024, '10 MiB'],
    [10 * 1024 * 1024 + 1, '10 MiB'],
    // There is no GiB unit, so a huge size stays in MiB.
    [5 * 1024 * 1024 * 1024, '5120 MiB'],
  ])('formats %d bytes as %s', (bytes, text) => {
    expect(formatBytes(bytes)).toBe(text);
  });

  it.each([Number.NaN, -1, Number.POSITIVE_INFINITY])('does not guess for %s', (bytes) => {
    expect(formatBytes(bytes)).toBe('unknown size');
  });
});

describe('peerName', () => {
  it('returns the display name as given', () => {
    expect(peerName('Dana Roy')).toBe('Dana Roy');
  });

  it('falls back for an empty or blank name', () => {
    expect(peerName('')).toBe('Unnamed node');
    expect(peerName('   ')).toBe('Unnamed node');
  });
});

describe('transferResultToastId', () => {
  it('is one fixed id per transfer and differs from the offer toast', () => {
    expect(transferResultToastId('t1')).toBe('network-transfer-result-t1');
    expect(transferResultToastId('t1')).toBe(transferResultToastId('t1'));
    expect(transferResultToastId('t1')).not.toBe(transferResultToastId('t2'));
  });
});

describe('transferWaitingToastId', () => {
  it('is one fixed id per transfer and differs from the result toast', () => {
    expect(transferWaitingToastId('t1')).toBe('network-transfer-waiting-t1');
    expect(transferWaitingToastId('t1')).not.toBe(transferResultToastId('t1'));
    expect(transferWaitingToastId('t1')).not.toBe(transferWaitingToastId('t2'));
  });
});
```

- [ ] **Step 7: Run, expect RED**

Run: `bunx vitest run src/lib/__tests__/network-utils.test.ts`
Expected: FAIL, `formatBytes is not a function` (and `transferWaitingToastId is not a function`).

- [ ] **Step 8: Implement the helpers**

Append to `src/lib/network-utils.ts`:

```ts

const KIB = 1024;
const MIB = KIB * 1024;

// One decimal, and no ".0" for a round number.
const oneDecimal = (n: number): string => n.toFixed(1).replace(/\.0$/, '');

// Shows a size in bytes, KiB or MiB. There is no larger unit: the transfer limit is 10 MiB.
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return 'unknown size';
  if (bytes < KIB) return bytes === 1 ? '1 byte' : `${Math.round(bytes)} bytes`;
  if (bytes < MIB) {
    const kib = oneDecimal(bytes / KIB);
    // 1048575 bytes would round to 1024 KiB, so it moves up to MiB.
    if (kib !== '1024') return `${kib} KiB`;
  }
  return `${oneDecimal(bytes / MIB)} MiB`;
}

// The name of a peer in a sentence. A node may send an empty display name.
export function peerName(displayName: string): string {
  return displayName.trim() || 'Unnamed node';
}

// One toast slot per transfer result, so a repeated finish event updates the toast in place.
export const transferResultToastId = (transferId: string): string =>
  `network-transfer-result-${transferId}`;

// The "waiting for them to accept" toast has its own id. It carries a Cancel action, and a result
// toast that reused the id would keep that action.
export const transferWaitingToastId = (transferId: string): string =>
  `network-transfer-waiting-${transferId}`;
```

- [ ] **Step 9: Run, expect GREEN**

Run: `bunx vitest run src/lib/__tests__/network-utils.test.ts`
Expected: PASS.

### 1c. Store slice

- [ ] **Step 10: Add the fixture**

In `src/test/network-fixtures.ts` change the type import to `import type { IncomingTransfer, PairingIncoming } from '@/stores/network-store';` and append:

```ts

/** A pending offer from a node that wants to send a collection. */
export function makeIncomingTransfer(patch: Partial<IncomingTransfer> = {}): IncomingTransfer {
  return {
    transferId: 't1',
    deviceId: 'DEV9',
    shortId: 'ZZZZ-9999',
    displayName: 'Dana Roy',
    collectionName: 'Orders',
    sizeBytes: 2048,
    accepted: false,
    ...patch,
  };
}
```

- [ ] **Step 11: Write the failing store tests**

In `src/stores/__tests__/network-store.test.ts` change the fixtures import to `import { makeIncoming, makeIncomingTransfer, makeNode, makeStatus } from '@/test/network-fixtures';` and the store import to include `onlinePairedNodes`:

```ts
import {
  INCOMING_TTL_MS,
  onlinePairedNodes,
  selectOnlinePairedCount,
  startNetworkPairing,
  useNetworkStore,
} from '@/stores/network-store';
```

Append at the end of the file:

```ts
describe('network-store transfers', () => {
  const offer = (transferId = 't1') => makeIncomingTransfer({ transferId });
  const progress = (transferId = 't1', doneBytes = 1024) => ({
    transferId,
    direction: 'receive' as const,
    doneBytes,
    totalBytes: 2048,
  });

  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('dedupes by transfer id and does not extend the expiry', () => {
    vi.useFakeTimers();
    store().addIncomingTransfer(offer('t1'));
    vi.advanceTimersByTime(60_000);
    store().addIncomingTransfer(offer('t1'));
    expect(store().incomingTransfers).toHaveLength(1);
    vi.advanceTimersByTime(INCOMING_TTL_MS - 60_000);
    expect(store().incomingTransfers).toHaveLength(0);
  });

  it('keeps an offer until the expiry time', () => {
    vi.useFakeTimers();
    store().addIncomingTransfer(offer());
    vi.advanceTimersByTime(INCOMING_TTL_MS - 1);
    expect(store().incomingTransfers).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(store().incomingTransfers).toHaveLength(0);
  });

  it('keeps two offers, each with its own timer', () => {
    vi.useFakeTimers();
    store().addIncomingTransfer(offer('a'));
    store().addIncomingTransfer(offer('b'));
    expect(store().incomingTransfers.map((t) => t.transferId)).toEqual(['a', 'b']);
    expect(vi.getTimerCount()).toBe(2);
    store().removeIncomingTransfer('a');
    expect(vi.getTimerCount()).toBe(1);
    expect(store().incomingTransfers.map((t) => t.transferId)).toEqual(['b']);
  });

  it('stores a zero byte offer as given', () => {
    store().addIncomingTransfer(makeIncomingTransfer({ sizeBytes: 0 }));
    expect(store().incomingTransfers[0]?.sizeBytes).toBe(0);
  });

  it('an accepted offer no longer expires', () => {
    vi.useFakeTimers();
    store().addIncomingTransfer(offer());
    store().markTransferAccepted('t1');
    expect(vi.getTimerCount()).toBe(0);
    vi.advanceTimersByTime(INCOMING_TTL_MS * 2);
    expect(store().incomingTransfers[0]?.accepted).toBe(true);
  });

  it('markTransferAccepted does not bring back an offer that is gone', () => {
    store().markTransferAccepted('missing');
    expect(store().incomingTransfers).toEqual([]);
  });

  it('keeps one progress row for each transfer and updates it in place', () => {
    store().setTransferProgress(progress('a', 100));
    store().setTransferProgress(progress('b', 5));
    store().setTransferProgress(progress('a', 900));
    expect(store().activeTransfers).toEqual([progress('a', 900), progress('b', 5)]);
  });

  it('keeps the direction of a progress row', () => {
    store().setTransferProgress({ ...progress('a'), direction: 'send' });
    expect(store().activeTransfers[0]?.direction).toBe('send');
  });

  it('finishTransfer removes the offer, the progress and the timer, and reports the first finish', () => {
    vi.useFakeTimers();
    store().addIncomingTransfer(offer());
    store().setTransferProgress(progress());
    expect(store().finishTransfer('t1')).toBe(true);
    expect(store().incomingTransfers).toEqual([]);
    expect(store().activeTransfers).toEqual([]);
    expect(vi.getTimerCount()).toBe(0);
    expect(store().finishTransfer('t1')).toBe(false);
  });

  it('ignores progress and offers that arrive after the finish', () => {
    store().finishTransfer('t1');
    store().setTransferProgress(progress());
    store().addIncomingTransfer(offer());
    expect(store().activeTransfers).toEqual([]);
    expect(store().incomingTransfers).toEqual([]);
  });

  it('remembers only the last 100 finished transfers', () => {
    for (let i = 0; i < 101; i++) store().finishTransfer(`f${i}`);
    // The oldest id is forgotten, the newest is still remembered.
    expect(store().finishTransfer('f0')).toBe(true);
    expect(store().finishTransfer('f100')).toBe(false);
  });

  it('opens and closes the transfer dialog without touching the pairing dialog', () => {
    store().openPairingDialog('p1');
    store().openTransferDialog('t1');
    expect(store().dialogTransferId).toBe('t1');
    store().closeTransferDialog();
    expect(store().dialogTransferId).toBeNull();
    expect(store().dialogPairingId).toBe('p1');
  });

  it('reset clears the transfers, the timers and the finished memory', () => {
    vi.useFakeTimers();
    store().addIncomingTransfer(offer('a'));
    store().setTransferProgress(progress('b'));
    store().openTransferDialog('a');
    store().endTransferDialog('The other node went offline.');
    store().finishTransfer('c');
    store().trackOutgoing('d', 'Dana Roy');
    store().markCancelledHere('e');
    store().reset();
    expect(store().incomingTransfers).toEqual([]);
    expect(store().activeTransfers).toEqual([]);
    expect(store().dialogTransferId).toBeNull();
    expect(store().dialogEndedText).toBeNull();
    expect(vi.getTimerCount()).toBe(0);
    expect(store().advanceOutgoing('d', 'sending')).toBeNull();
    expect(store().wasCancelledHere('e')).toBe(false);
    // The id may be used again after a reset.
    expect(store().finishTransfer('c')).toBe(true);
  });

  it('tracks a send and moves it forward once per phase', () => {
    store().trackOutgoing('t1', 'Dana Roy');
    expect(store().advanceOutgoing('t1', 'sending')).toBe('Dana Roy');
    expect(store().advanceOutgoing('t1', 'sending')).toBeNull();
    expect(store().advanceOutgoing('t1', 'sent')).toBe('Dana Roy');
    // A phase never goes back.
    expect(store().advanceOutgoing('t1', 'sending')).toBeNull();
    expect(store().advanceOutgoing('other', 'sending')).toBeNull();
  });

  it('can skip the sending phase of a one chunk send', () => {
    store().trackOutgoing('t1', 'Dana Roy');
    expect(store().advanceOutgoing('t1', 'sent')).toBe('Dana Roy');
    expect(store().advanceOutgoing('t1', 'sending')).toBeNull();
  });

  it('finishTransfer forgets the send and the local cancel', () => {
    store().trackOutgoing('t1', 'Dana Roy');
    store().markCancelledHere('t1');
    expect(store().wasCancelledHere('t1')).toBe(true);
    store().finishTransfer('t1');
    expect(store().wasCancelledHere('t1')).toBe(false);
    expect(store().advanceOutgoing('t1', 'sending')).toBeNull();
  });

  it('shows an end reason in the open dialog until it closes or another opens', () => {
    store().openTransferDialog('t1');
    expect(store().dialogEndedText).toBeNull();
    store().endTransferDialog('The other node went offline.');
    expect(store().dialogEndedText).toBe('The other node went offline.');
    expect(store().dialogTransferId).toBe('t1');
    store().openTransferDialog('t2');
    expect(store().dialogEndedText).toBeNull();
    store().endTransferDialog('The other node went offline.');
    store().closeTransferDialog();
    expect(store().dialogEndedText).toBeNull();
  });

  it('a pairing id and a transfer id never share a timer', () => {
    vi.useFakeTimers();
    store().addIncoming(makeIncoming({ pairingId: 'same' }));
    store().addIncomingTransfer(offer('same'));
    expect(vi.getTimerCount()).toBe(2);
    store().removeIncomingTransfer('same');
    expect(vi.getTimerCount()).toBe(1);
    expect(store().incoming).toHaveLength(1);
  });
});

describe('onlinePairedNodes', () => {
  it('keeps only paired nodes that are online', () => {
    const nodes = [
      makeNode({ deviceId: 'A', group: 'paired', status: 'online' }),
      makeNode({ deviceId: 'B', group: 'paired', status: 'offline' }),
      makeNode({ deviceId: 'C', group: 'nearby', status: 'available' }),
      makeNode({ deviceId: 'D', group: 'paired', status: 'needs_update' }),
    ];
    expect(onlinePairedNodes(nodes).map((n) => n.deviceId)).toEqual(['A']);
  });

  it('agrees with the count selector', () => {
    const nodes = [makeNode({ group: 'paired', status: 'online' })];
    useNetworkStore.setState({ nodes });
    expect(selectOnlinePairedCount(useNetworkStore.getState())).toBe(onlinePairedNodes(nodes).length);
  });
});
```

- [ ] **Step 12: Run, expect RED**

Run: `bunx vitest run src/stores/__tests__/network-store.test.ts`
Expected: FAIL, `store().addIncomingTransfer is not a function` (and `trackOutgoing`, `endTransferDialog`, `markCancelledHere` likewise) and `onlinePairedNodes` is not exported. The older store tests still pass.

- [ ] **Step 13: Implement the slice**

Edit `src/stores/network-store.ts`.

(a) In the import from `@/lib/tauri-api`, add `type TransferDirection,` between `setNetworkVisibility,` and `unblockNetworkDevice,`.

(b) After the `ActivePairing` interface (before `export interface NetworkState`), add:

```ts
/** A collection another node offers. It waits for the local user's answer. */
export interface IncomingTransfer {
  transferId: string;
  deviceId: string;
  shortId: string;
  displayName: string;
  collectionName: string;
  sizeBytes: number;
  /** The local user pressed Accept. An accepted offer no longer expires. */
  accepted: boolean;
}

/** A transfer that moves bytes, in either direction. */
export interface ActiveTransfer {
  transferId: string;
  direction: TransferDirection;
  doneBytes: number;
  totalBytes: number;
}
```

(c) In `NetworkState`, replace

```ts
  /** The pairing whose dialog is open, for either role. */
  dialogPairingId: string | null;
  error: string | null;
```

with

```ts
  /** The pairing whose dialog is open, for either role. */
  dialogPairingId: string | null;
  /** Offers from other nodes that wait for an answer or receive bytes. */
  incomingTransfers: IncomingTransfer[];
  /** Progress of transfers in both directions. */
  activeTransfers: ActiveTransfer[];
  /** The offer whose dialog is open. */
  dialogTransferId: string | null;
  /** Why the open offer ended before it was answered, for example "The other node went offline." */
  dialogEndedText: string | null;
  error: string | null;
```

and replace

```ts
  closePairingDialog: () => void;
  reset: () => void;
```

with

```ts
  closePairingDialog: () => void;
  addIncomingTransfer: (transfer: IncomingTransfer) => void;
  removeIncomingTransfer: (transferId: string) => void;
  markTransferAccepted: (transferId: string) => void;
  setTransferProgress: (progress: ActiveTransfer) => void;
  /** Removes the transfer everywhere. It returns false when the transfer had finished before. */
  finishTransfer: (transferId: string) => boolean;
  openTransferDialog: (transferId: string) => void;
  closeTransferDialog: () => void;
  /** Keeps the dialog open on an ended offer and says why. */
  endTransferDialog: (text: string) => void;
  /** A send started here. The bridge updates its waiting toast. */
  trackOutgoing: (transferId: string, displayName: string) => void;
  /** Moves a tracked send forward. It returns the node name when the phase changed, else null. */
  advanceOutgoing: (transferId: string, phase: 'sending' | 'sent') => string | null;
  /** The local user pressed Cancel. The `cancelled` finish then shows no toast. */
  markCancelledHere: (transferId: string) => void;
  wasCancelledHere: (transferId: string) => boolean;
  reset: () => void;
```

(d) In `initialState` replace `dialogPairingId: null as string | null,` with:

```ts
  dialogPairingId: null as string | null,
  incomingTransfers: [] as IncomingTransfer[],
  activeTransfers: [] as ActiveTransfer[],
  dialogTransferId: null as string | null,
  dialogEndedText: null as string | null,
```

(e) After `let nodesSeq = 0;` add:

```ts

// A transfer id shares the timer map with pairing ids, so its key has a prefix.
const transferTimerKey = (transferId: string): string => `transfer:${transferId}`;

// Ids of transfers that finished. A late progress or offer event for one of them is dropped.
const FINISHED_MEMORY = 100;
const finishedTransfers = new Set<string>();

function rememberFinished(transferId: string): void {
  finishedTransfers.add(transferId);
  if (finishedTransfers.size > FINISHED_MEMORY) {
    const oldest = finishedTransfers.values().next().value;
    if (oldest !== undefined) finishedTransfers.delete(oldest);
  }
}

// Sends started here, by transfer id. The phase only moves forward.
type OutgoingPhase = 'waiting' | 'sending' | 'sent';
const PHASE_ORDER: Record<OutgoingPhase, number> = { waiting: 0, sending: 1, sent: 2 };
const outgoing = new Map<string, { displayName: string; phase: OutgoingPhase }>();

// Transfers the local user cancelled. Their finish shows no "cancelled" toast.
const cancelledHere = new Set<string>();
```

(f) Replace `  closePairingDialog: () => set({ dialogPairingId: null }),` with:

```ts
  closePairingDialog: () => set({ dialogPairingId: null }),

  addIncomingTransfer(transfer) {
    if (finishedTransfers.has(transfer.transferId)) return;
    // A repeated event neither adds a row nor extends the expiry.
    if (get().incomingTransfers.some((t) => t.transferId === transfer.transferId)) return;
    set({ incomingTransfers: [...get().incomingTransfers, transfer] });
    expiryTimers.set(
      transferTimerKey(transfer.transferId),
      setTimeout(() => get().removeIncomingTransfer(transfer.transferId), INCOMING_TTL_MS),
    );
  },

  removeIncomingTransfer(transferId) {
    clearExpiryTimer(transferTimerKey(transferId));
    if (!get().incomingTransfers.some((t) => t.transferId === transferId)) return;
    set({ incomingTransfers: get().incomingTransfers.filter((t) => t.transferId !== transferId) });
  },

  markTransferAccepted(transferId) {
    if (!get().incomingTransfers.some((t) => t.transferId === transferId)) return;
    // An accepted transfer can run longer than the offer lifetime.
    clearExpiryTimer(transferTimerKey(transferId));
    set({
      incomingTransfers: get().incomingTransfers.map((t) =>
        t.transferId === transferId ? { ...t, accepted: true } : t,
      ),
    });
  },

  setTransferProgress(progress) {
    if (finishedTransfers.has(progress.transferId)) return;
    const known = get().activeTransfers.some((t) => t.transferId === progress.transferId);
    set({
      activeTransfers: known
        ? get().activeTransfers.map((t) => (t.transferId === progress.transferId ? progress : t))
        : [...get().activeTransfers, progress],
    });
  },

  finishTransfer(transferId) {
    if (finishedTransfers.has(transferId)) return false;
    rememberFinished(transferId);
    clearExpiryTimer(transferTimerKey(transferId));
    outgoing.delete(transferId);
    cancelledHere.delete(transferId);
    set({
      incomingTransfers: get().incomingTransfers.filter((t) => t.transferId !== transferId),
      activeTransfers: get().activeTransfers.filter((t) => t.transferId !== transferId),
    });
    return true;
  },

  openTransferDialog: (dialogTransferId) => set({ dialogTransferId, dialogEndedText: null }),
  closeTransferDialog: () => set({ dialogTransferId: null, dialogEndedText: null }),
  endTransferDialog: (dialogEndedText) => set({ dialogEndedText }),

  trackOutgoing(transferId, displayName) {
    if (finishedTransfers.has(transferId)) return;
    outgoing.set(transferId, { displayName, phase: 'waiting' });
  },

  advanceOutgoing(transferId, phase) {
    const entry = outgoing.get(transferId);
    if (!entry || PHASE_ORDER[phase] <= PHASE_ORDER[entry.phase]) return null;
    entry.phase = phase;
    return entry.displayName;
  },

  markCancelledHere: (transferId) => {
    cancelledHere.add(transferId);
  },
  wasCancelledHere: (transferId) => cancelledHere.has(transferId),
```

(g) In `reset()` replace `    expiryTimers.clear();` with:

```ts
    expiryTimers.clear();
    finishedTransfers.clear();
    outgoing.clear();
    cancelledHere.clear();
```

(h) Replace the `selectOnlinePairedCount` block (comment and function) with:

```ts
/** Paired nodes that are online. The send dialog lists them. */
export const onlinePairedNodes = (nodes: NetworkNode[]): NetworkNode[] =>
  nodes.filter((n) => n.group === 'paired' && n.status === 'online');

/** Paired nodes that are online. It feeds the number in the status bar. */
export const selectOnlinePairedCount = (s: NetworkState): number =>
  onlinePairedNodes(s.nodes).length;
```

- [ ] **Step 14: Run, expect GREEN**

Run: `bunx vitest run src/stores/__tests__/network-store.test.ts`
Expected: PASS, including the older pairing tests.

### 1d. Bridge

- [ ] **Step 15: Update the existing bridge test mock**

In `src/lib/__tests__/network-event-bridge.test.ts`:

(a) In the hoisted `api` object add `onNetworkTransfer: listener('transfer'),` after `onNetworkPairing: listener('pairing'),`.

(b) Change the `emit` signature to `function emit(name: 'changed' | 'pairing' | 'transfer', payload: unknown): void {`.

(c) In `loads the status and the nodes once on mount` add `expect(mocks.api.onNetworkTransfer).toHaveBeenCalledTimes(1);` after the `onNetworkPairing` expectation.

(d) In `ignores events after unmount and stops listening` change `toHaveBeenCalledTimes(2)` to `toHaveBeenCalledTimes(3)`.

(e) In the hoisted `toast` mock change `toast: { info: vi.fn(), success: vi.fn(), error: vi.fn() },` to `toast: { info: vi.fn(), success: vi.fn(), error: vi.fn(), dismiss: vi.fn() },`. Verify the anchor text first; if the file differs, add `dismiss: vi.fn()` to whatever the toast mock is and note it in the ledger.

- [ ] **Step 16: Write the failing transfer bridge tests**

Append to the same file:

```ts
describe('useNetworkEventBridge transfers', () => {
  const offerEvent = (id = 't1', patch: Record<string, unknown> = {}) => ({
    type: 'offerIncoming',
    transfer_id: id,
    device_id: 'DEV9',
    short_id: 'ZZZZ-9999',
    display_name: 'Dana Roy',
    collection_name: 'Orders',
    size_bytes: 2048,
    ...patch,
  });
  const progressEvent = (id = 't1', done = 1024, direction = 'receive') => ({
    type: 'progress',
    transfer_id: id,
    direction,
    done_bytes: done,
    total_bytes: 2048,
  });
  const finishedEvent = (
    outcome: string,
    direction = 'receive',
    patch: Record<string, unknown> = {},
  ) => ({
    type: 'finished',
    transfer_id: 't1',
    direction,
    outcome,
    display_name: 'Dana Roy',
    collection_name: 'Orders',
    message: null,
    ...patch,
  });
  const resultId = { id: 'network-transfer-result-t1' };

  beforeEach(() => {
    vi.clearAllMocks();
    for (const key of Object.keys(mocks.handlers)) delete mocks.handlers[key];
    store().reset();
    mocks.api.getNetworkStatus.mockResolvedValue(makeStatus());
    mocks.api.listNetworkNodes.mockResolvedValue([makeNode()]);
  });

  it('adds one row for a repeated offer', async () => {
    await mountBridge();
    emit('transfer', offerEvent());
    emit('transfer', offerEvent());
    expect(store().incomingTransfers).toEqual([makeIncomingTransfer()]);
  });

  it('adds two offers at once as two rows', async () => {
    await mountBridge();
    emit('transfer', offerEvent('a'));
    emit('transfer', offerEvent('b', { collection_name: 'Billing' }));
    expect(store().incomingTransfers.map((t) => [t.transferId, t.collectionName])).toEqual([
      ['a', 'Orders'],
      ['b', 'Billing'],
    ]);
  });

  it('maps progress into the active transfers', async () => {
    await mountBridge();
    emit('transfer', progressEvent('t1', 1024, 'send'));
    emit('transfer', progressEvent('t1', 2048, 'send'));
    expect(store().activeTransfers).toEqual([
      { transferId: 't1', direction: 'send', doneBytes: 2048, totalBytes: 2048 },
    ]);
  });

  it.each([
    ['receive', 'completed', 'success', 'Received "Orders" from Dana Roy.'],
    ['send', 'completed', 'success', 'Sent "Orders" to Dana Roy.'],
    ['send', 'unconfirmed', 'info', 'Sent, but the other node did not confirm.'],
    ['send', 'declined', 'info', 'Dana Roy declined.'],
    ['receive', 'expired', 'info', 'The request expired.'],
    ['send', 'expired', 'info', 'The request expired.'],
    ['send', 'busy', 'error', 'Dana Roy is busy. Try again in a minute.'],
    ['send', 'too_big', 'error', 'This collection is too big to send (limit 10 MiB).'],
    ['send', 'failed', 'error', 'Sending failed. Try again.'],
    ['receive', 'failed', 'error', 'Receiving failed.'],
    ['send', 'damaged', 'error', 'Sending failed. Try again.'],
    ['receive', 'damaged', 'error', 'The collection arrived damaged and was not imported.'],
    ['send', 'rejected', 'error', 'Sending failed. Try again.'],
    [
      'receive',
      'rejected',
      'error',
      'The collection was rejected because it is not safe to import.',
    ],
    ['receive', 'cancelled', 'info', 'Dana Roy cancelled the transfer.'],
    ['send', 'cancelled', 'info', 'Dana Roy cancelled the transfer.'],
  ] as const)('announces %s %s', async (direction, outcome, method, message) => {
    await mountBridge();
    emit('transfer', finishedEvent(outcome, direction));
    expect(mocks.toast[method]).toHaveBeenCalledWith(message, resultId);
    expect(mocks.toast[method]).toHaveBeenCalledTimes(1);
  });

  it.each([
    ['receive', 'declined'],
    ['receive', 'busy'],
    // The backend never emits these two: a refused offer emits nothing, and only a sender is unconfirmed.
    ['receive', 'too_big'],
    ['receive', 'unconfirmed'],
    ['send', 'something-new'],
  ] as const)('says nothing for %s %s', async (direction, outcome) => {
    await mountBridge();
    emit('transfer', finishedEvent(outcome, direction));
    expect(mocks.toast.info).not.toHaveBeenCalled();
    expect(mocks.toast.success).not.toHaveBeenCalled();
    expect(mocks.toast.error).not.toHaveBeenCalled();
  });

  it('tells the sender that the peer declined', async () => {
    await mountBridge();
    emit('transfer', finishedEvent('declined', 'send'));
    expect(mocks.toast.info).toHaveBeenCalledWith('Dana Roy declined.', resultId);
  });

  it('says nothing to the receiver who declined', async () => {
    await mountBridge();
    emit('transfer', offerEvent());
    emit('transfer', finishedEvent('declined', 'receive'));
    expect(mocks.toast.info).not.toHaveBeenCalled();
    expect(store().incomingTransfers).toEqual([]);
  });

  it('shows the importer message when a receive fails with one', async () => {
    await mountBridge();
    emit('transfer', finishedEvent('failed', 'receive', { message: 'The ZIP has no collection.' }));
    expect(mocks.toast.error).toHaveBeenCalledWith('The ZIP has no collection.', resultId);
  });

  it('falls back when the message is null', async () => {
    await mountBridge();
    emit('transfer', finishedEvent('failed', 'receive', { message: null }));
    expect(mocks.toast.error).toHaveBeenCalledWith('Receiving failed.', resultId);
  });

  it('falls back when the message is blank', async () => {
    await mountBridge();
    emit('transfer', finishedEvent('failed', 'receive', { message: '  ' }));
    expect(mocks.toast.error).toHaveBeenCalledWith('Receiving failed.', resultId);
  });

  it('ignores the message on a send', async () => {
    await mountBridge();
    emit('transfer', finishedEvent('failed', 'send', { message: 'Internal detail' }));
    expect(mocks.toast.error).toHaveBeenCalledWith('Sending failed. Try again.', resultId);
  });

  it('dismisses the waiting toast when the transfer finishes', async () => {
    await mountBridge();
    emit('transfer', finishedEvent('declined', 'send'));
    expect(mocks.toast.dismiss).toHaveBeenCalledWith('network-transfer-waiting-t1');
  });

  it('dismisses the waiting toast also when the outcome shows nothing', async () => {
    await mountBridge();
    emit('transfer', finishedEvent('declined', 'receive'));
    expect(mocks.toast.dismiss).toHaveBeenCalledWith('network-transfer-waiting-t1');
    expect(mocks.toast.info).not.toHaveBeenCalled();
  });

  it.each(['send', 'receive'] as const)(
    'says nothing when the local user cancelled a %s',
    async (direction) => {
      await mountBridge();
      store().markCancelledHere('t1');
      emit('transfer', finishedEvent('cancelled', direction));
      expect(mocks.toast.info).not.toHaveBeenCalled();
      expect(mocks.toast.dismiss).toHaveBeenCalledWith('network-transfer-waiting-t1');
      expect(store().wasCancelledHere('t1')).toBe(false);
    },
  );

  it('switches an open pending offer to the offline text when the sender goes away', async () => {
    await mountBridge();
    emit('transfer', offerEvent());
    store().openTransferDialog('t1');
    emit('transfer', finishedEvent('failed', 'receive', { message: 'The other node went offline.' }));
    expect(store().dialogTransferId).toBe('t1');
    expect(store().dialogEndedText).toBe('The other node went offline.');
    expect(store().incomingTransfers).toEqual([]);
    expect(mocks.toast.error).toHaveBeenCalledWith('The other node went offline.', resultId);
  });

  it('closes the dialog of an accepted transfer that fails with a message', async () => {
    await mountBridge();
    emit('transfer', offerEvent());
    store().markTransferAccepted('t1');
    store().openTransferDialog('t1');
    emit('transfer', finishedEvent('failed', 'receive', { message: 'The ZIP has no collection.' }));
    expect(store().dialogTransferId).toBeNull();
  });

  it('switches the waiting toast to sending on the first progress of a send', async () => {
    await mountBridge();
    store().trackOutgoing('t1', 'Dana Roy');
    emit('transfer', progressEvent('t1', 1024, 'send'));
    emit('transfer', progressEvent('t1', 1536, 'send'));
    expect(mocks.toast.info).toHaveBeenCalledTimes(1);
    expect(mocks.toast.info).toHaveBeenCalledWith('Sending to Dana Roy...', {
      id: 'network-transfer-waiting-t1',
      duration: Number.POSITIVE_INFINITY,
    });
  });

  it('drops the Cancel action once the last chunk is out', async () => {
    await mountBridge();
    store().trackOutgoing('t1', 'Dana Roy');
    emit('transfer', progressEvent('t1', 1024, 'send'));
    emit('transfer', progressEvent('t1', 2048, 'send'));
    expect(mocks.toast.info).toHaveBeenCalledTimes(2);
    // Sonner merges options for a reused id, so the action must be overwritten explicitly.
    expect(mocks.toast.info).toHaveBeenLastCalledWith('Sending to Dana Roy...', {
      id: 'network-transfer-waiting-t1',
      duration: Number.POSITIVE_INFINITY,
      action: undefined,
    });
    const last = mocks.toast.info.mock.calls.at(-1)?.[1] as Record<string, unknown>;
    expect(Object.hasOwn(last, 'action')).toBe(true);
  });

  it('leaves the toasts alone for a send it did not start or a receive', async () => {
    await mountBridge();
    store().trackOutgoing('t1', 'Dana Roy');
    emit('transfer', progressEvent('t9', 1024, 'send'));
    emit('transfer', progressEvent('t1', 1024, 'receive'));
    expect(mocks.toast.info).not.toHaveBeenCalled();
  });

  it('shows the result once, also when the finish event repeats', async () => {
    await mountBridge();
    emit('transfer', finishedEvent('completed', 'send'));
    emit('transfer', finishedEvent('completed', 'send'));
    expect(mocks.toast.success).toHaveBeenCalledTimes(1);
    expect(mocks.toast.dismiss).toHaveBeenCalledTimes(1);
  });

  it('keeps a double quote in the collection name', async () => {
    await mountBridge();
    emit('transfer', finishedEvent('completed', 'receive', { collection_name: 'My "Quoted" API' }));
    expect(mocks.toast.success).toHaveBeenCalledWith(
      'Received "My "Quoted" API" from Dana Roy.',
      resultId,
    );
  });

  it('names a sender without a display name', async () => {
    await mountBridge();
    emit('transfer', finishedEvent('completed', 'receive', { display_name: '  ' }));
    expect(mocks.toast.success).toHaveBeenCalledWith(
      'Received "Orders" from Unnamed node.',
      resultId,
    );
  });

  it('a finish removes the offer and the progress of that transfer only', async () => {
    await mountBridge();
    emit('transfer', offerEvent('t1'));
    emit('transfer', offerEvent('t2'));
    emit('transfer', progressEvent('t1'));
    emit('transfer', finishedEvent('completed'));
    expect(store().incomingTransfers.map((t) => t.transferId)).toEqual(['t2']);
    expect(store().activeTransfers).toEqual([]);
  });

  it('closes the dialog of a transfer that failed', async () => {
    await mountBridge();
    emit('transfer', offerEvent());
    store().openTransferDialog('t1');
    emit('transfer', finishedEvent('failed'));
    expect(store().dialogTransferId).toBeNull();
    expect(mocks.toast.error).toHaveBeenCalledWith('Receiving failed.', resultId);
  });

  it('keeps an open dialog on expired, so the dialog can say so', async () => {
    await mountBridge();
    emit('transfer', offerEvent());
    store().openTransferDialog('t1');
    emit('transfer', finishedEvent('expired'));
    expect(store().dialogTransferId).toBe('t1');
    expect(store().incomingTransfers).toEqual([]);
  });

  it('a finish does not close the dialog of another transfer', async () => {
    await mountBridge();
    store().openTransferDialog('other');
    emit('transfer', finishedEvent('completed'));
    expect(store().dialogTransferId).toBe('other');
  });

  it('ignores progress that arrives after the finish', async () => {
    await mountBridge();
    emit('transfer', finishedEvent('completed'));
    emit('transfer', progressEvent());
    expect(store().activeTransfers).toEqual([]);
  });

  it('ignores transfer events after unmount', async () => {
    const view = await mountBridge();
    view.unmount();
    await waitFor(() => expect(mocks.unlisten).toHaveBeenCalledTimes(3));
    emit('transfer', offerEvent());
    emit('transfer', finishedEvent('completed'));
    expect(store().incomingTransfers).toEqual([]);
    expect(mocks.toast.success).not.toHaveBeenCalled();
  });

  it('stops a transfer listener that finishes registering after unmount', async () => {
    const late = createDeferred<() => void>();
    const lateUnlisten = vi.fn();
    mocks.api.onNetworkTransfer.mockImplementationOnce(() => late.promise);
    const view = renderHook(() => useNetworkEventBridge());
    view.unmount();
    late.resolve(lateUnlisten);
    await waitFor(() => expect(lateUnlisten).toHaveBeenCalledTimes(1));
  });
});
```

Also change the fixtures import at the top of the file to `import { makeIncoming, makeIncomingTransfer, makeNode, makeStatus } from '@/test/network-fixtures';`.

- [ ] **Step 17: Run, expect RED**

Run: `bunx vitest run src/lib/__tests__/network-event-bridge.test.ts`
Expected: FAIL, the new tests fail because the bridge does not call `onNetworkTransfer` and `emit('transfer', ...)` throws `no transfer listener`. The older tests may also fail on the `onNetworkTransfer` count until Step 18. (The store actions used here exist since Step 13.)

- [ ] **Step 18: Implement the handler**

In `src/lib/network-event-bridge.ts`:

(a) Replace the two imports with:

```ts
import {
  PAIRING_FAILED_TOAST_ID,
  peerName,
  transferResultToastId,
  transferWaitingToastId,
} from '@/lib/network-utils';
import {
  type NetworkChangedEvent,
  type NetworkPairingEvent,
  type NetworkTransferEvent,
  onNetworkChanged,
  onNetworkPairing,
  onNetworkTransfer,
} from '@/lib/tauri-api';
```

(keep the `createListenerScope` and store imports as they are).

(b) Insert before the doc comment of `useNetworkEventBridge`:

```ts
type FinishedTransfer = Extract<NetworkTransferEvent, { type: 'finished' }>;
type ProgressTransfer = Extract<NetworkTransferEvent, { type: 'progress' }>;

interface TransferMessage {
  kind: 'success' | 'info' | 'error';
  text: string;
}

// The texts come from spec section 10. A missing row means the user already knows.
// `cancelledHere` is true when the local user pressed Cancel on this transfer.
function describeTransferOutcome(
  event: FinishedTransfer,
  cancelledHere: boolean,
): TransferMessage | null {
  const name = peerName(event.display_name);
  const sending = event.direction === 'send';
  switch (event.outcome) {
    case 'completed':
      return sending
        ? { kind: 'success', text: `Sent "${event.collection_name}" to ${name}.` }
        : { kind: 'success', text: `Received "${event.collection_name}" from ${name}.` };
    case 'unconfirmed':
      // The bytes left, but no result came back. Not an error: the import may have worked.
      return sending ? { kind: 'info', text: 'Sent, but the other node did not confirm.' } : null;
    case 'declined':
      // The user who pressed Decline already knows.
      return sending ? { kind: 'info', text: `${name} declined.` } : null;
    case 'expired':
      return { kind: 'info', text: 'The request expired.' };
    case 'busy':
      return sending
        ? { kind: 'error', text: `${name} is busy. Try again in a minute.` }
        : null;
    case 'too_big':
      // A refused offer emits nothing on the receiver, so only the sender has a text.
      return sending
        ? { kind: 'error', text: 'This collection is too big to send (limit 10 MiB).' }
        : null;
    case 'failed':
      return {
        kind: 'error',
        // The receiver shows the importer's text when there is one. The sender never does.
        text: sending ? 'Sending failed. Try again.' : event.message?.trim() || 'Receiving failed.',
      };
    case 'damaged':
      return {
        kind: 'error',
        text: sending
          ? 'Sending failed. Try again.'
          : 'The collection arrived damaged and was not imported.',
      };
    case 'rejected':
      return {
        kind: 'error',
        text: sending
          ? 'Sending failed. Try again.'
          : 'The collection was rejected because it is not safe to import.',
      };
    case 'cancelled':
      // The user who pressed Cancel already knows. Otherwise the other side cancelled.
      return cancelledHere ? null : { kind: 'info', text: `${name} cancelled the transfer.` };
    default:
      // Any outcome added later stays quiet.
      return null;
  }
}

// The waiting toast follows a send started here: "Sending to <name>..." once bytes move, and no
// Cancel once the last chunk is out, because the backend refuses a cancel then.
function updateWaitingToast(event: ProgressTransfer): void {
  const last = event.done_bytes >= event.total_bytes;
  const name = useNetworkStore.getState().advanceOutgoing(event.transfer_id, last ? 'sent' : 'sending');
  if (name === null) return;
  const options = {
    id: transferWaitingToastId(event.transfer_id),
    duration: Number.POSITIVE_INFINITY,
  };
  // Sonner merges options for a reused id, so an explicit undefined removes the Cancel action.
  toast.info(`Sending to ${peerName(name)}...`, last ? { ...options, action: undefined } : options);
}

export function handleNetworkTransfer(event: NetworkTransferEvent): void {
  const store = useNetworkStore.getState();
  switch (event.type) {
    case 'offerIncoming':
      store.addIncomingTransfer({
        transferId: event.transfer_id,
        deviceId: event.device_id,
        shortId: event.short_id,
        displayName: event.display_name,
        collectionName: event.collection_name,
        sizeBytes: event.size_bytes,
        accepted: false,
      });
      return;
    case 'progress':
      store.setTransferProgress({
        transferId: event.transfer_id,
        direction: event.direction,
        doneBytes: event.done_bytes,
        totalBytes: event.total_bytes,
      });
      if (event.direction === 'send') updateWaitingToast(event);
      return;
    case 'finished': {
      const id = event.transfer_id;
      // Read these before the finish, which forgets both.
      const cancelledHere = store.wasCancelledHere(id);
      const wasPending = store.incomingTransfers.some((t) => t.transferId === id && !t.accepted);
      // A repeated finish event changes nothing and shows nothing.
      if (!store.finishTransfer(id)) return;
      const message = describeTransferOutcome(event, cancelledHere);
      // An offer that ended before it was answered keeps its dialog, so the dialog can say why:
      // it expired, or the sender went offline (a `failed` with a message).
      const keepDialog =
        event.outcome === 'expired' ||
        (wasPending && event.outcome === 'failed' && Boolean(event.message?.trim()));
      if (store.dialogTransferId === id) {
        if (!keepDialog) store.closeTransferDialog();
        else if (event.outcome === 'failed' && message) store.endTransferDialog(message.text);
      }
      // The waiting toast with the Cancel action must not outlive the transfer.
      toast.dismiss(transferWaitingToastId(id));
      if (message) toast[message.kind](message.text, { id: transferResultToastId(id) });
      return;
    }
  }
}

```

(c) Update the doc comment of `useNetworkEventBridge` to say "the three network channels", and after the `onNetworkPairing` block inside the effect add:

```ts
    scope.keep(
      onNetworkTransfer((event) => {
        if (!scope.isDisposed()) handleNetworkTransfer(event);
      }),
    );
```

- [ ] **Step 19: Run, expect GREEN**

Run: `bunx vitest run src/lib/__tests__/network-event-bridge.test.ts`
Expected: PASS, the older tests and the new `useNetworkEventBridge transfers` block.

### 1e. Toast, hook and dialog

- [ ] **Step 20: Write the failing toast test**

Create `src/components/network/__tests__/IncomingTransferToast.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { IncomingTransferToast } from '@/components/network/IncomingTransferToast';
import { useNetworkStore } from '@/stores/network-store';

const mocks = vi.hoisted(() => ({
  toast: { dismiss: vi.fn(), custom: vi.fn() },
  api: { getNetworkStatus: vi.fn(), listNetworkNodes: vi.fn(), requestNetworkPairing: vi.fn() },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

function setup(patch: Partial<React.ComponentProps<typeof IncomingTransferToast>> = {}) {
  render(
    <IncomingTransferToast
      transferId='t1'
      displayName='Dana Roy'
      shortId='DDDD-4444'
      collectionName='Orders'
      sizeBytes={2048}
      {...patch}
    />,
  );
}

describe('IncomingTransferToast', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
  });

  it('names the sender, the short id, the collection and the size', () => {
    setup();
    expect(screen.getByText('Dana Roy wants to send you a collection')).toBeInTheDocument();
    expect(screen.getByText('DDDD-4444')).toBeInTheDocument();
    expect(screen.getByText('Orders (2 KiB)')).toBeInTheDocument();
  });

  it('falls back when the display name is empty', () => {
    setup({ displayName: '' });
    expect(screen.getByText('Unnamed node wants to send you a collection')).toBeInTheDocument();
  });

  it('shows a zero byte size', () => {
    setup({ sizeBytes: 0 });
    expect(screen.getByText('Orders (0 bytes)')).toBeInTheDocument();
  });

  it('shows a collection name with a double quote as given', () => {
    setup({ collectionName: 'Orders "v2"' });
    expect(screen.getByText('Orders "v2" (2 KiB)')).toBeInTheDocument();
  });

  it('Review opens the transfer dialog and dismisses the toast', async () => {
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'Review' }));
    expect(useNetworkStore.getState().dialogTransferId).toBe('t1');
    expect(mocks.toast.dismiss).toHaveBeenCalledWith('network-transfer-offer-t1');
  });

  it('has only a Review action', () => {
    setup();
    expect(screen.getAllByRole('button').map((b) => b.textContent)).toEqual(['Review']);
    expect(screen.queryByRole('button', { name: 'Later' })).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 21: Write the failing hook test**

Create `src/hooks/__tests__/useIncomingTransferNotice.test.tsx`:

```tsx
import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useIncomingTransferNotice } from '@/hooks/useIncomingTransferNotice';
import { INCOMING_TTL_MS, useNetworkStore } from '@/stores/network-store';
import { makeIncomingTransfer } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { custom: vi.fn(), dismiss: vi.fn() },
  api: { getNetworkStatus: vi.fn(), listNetworkNodes: vi.fn(), requestNetworkPairing: vi.fn() },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const store = () => useNetworkStore.getState();
const offer = (transferId: string) => makeIncomingTransfer({ transferId });
const toastIds = () => mocks.toast.custom.mock.calls.map((call) => (call[1] as { id: string }).id);

describe('useIncomingTransferNotice', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
  });

  it('shows one toast with a fixed id and the offer lifetime', () => {
    renderHook(() => useIncomingTransferNotice());
    act(() => store().addIncomingTransfer(offer('t1')));
    expect(mocks.toast.custom).toHaveBeenCalledTimes(1);
    expect(mocks.toast.custom).toHaveBeenCalledWith(expect.any(Function), {
      id: 'network-transfer-offer-t1',
      duration: INCOMING_TTL_MS,
    });
  });

  it('shows no second toast for a duplicate offer', () => {
    renderHook(() => useIncomingTransferNotice());
    act(() => {
      store().addIncomingTransfer(offer('t1'));
      store().addIncomingTransfer(offer('t1'));
    });
    expect(mocks.toast.custom).toHaveBeenCalledTimes(1);
  });

  it('shows one toast per offer, with different ids', () => {
    renderHook(() => useIncomingTransferNotice());
    act(() => {
      store().addIncomingTransfer(offer('t1'));
      store().addIncomingTransfer(offer('t2'));
    });
    expect(toastIds()).toEqual(['network-transfer-offer-t1', 'network-transfer-offer-t2']);
  });

  it('dismisses the toast when the offer goes away', () => {
    renderHook(() => useIncomingTransferNotice());
    act(() => store().addIncomingTransfer(offer('t1')));
    act(() => store().removeIncomingTransfer('t1'));
    expect(mocks.toast.dismiss).toHaveBeenCalledWith('network-transfer-offer-t1');
  });

  it('does not react to changes that leave the offers alone', () => {
    renderHook(() => useIncomingTransferNotice());
    act(() => useNetworkStore.setState({ panelOpen: true }));
    expect(mocks.toast.custom).not.toHaveBeenCalled();
    expect(mocks.toast.dismiss).not.toHaveBeenCalled();
  });

  it('shows a toast for a pending offer that exists at mount, but not for an accepted one', () => {
    store().addIncomingTransfer(offer('t1'));
    store().addIncomingTransfer(offer('t2'));
    store().markTransferAccepted('t2');
    renderHook(() => useIncomingTransferNotice());
    expect(toastIds()).toEqual(['network-transfer-offer-t1']);
  });

  it('brings the toast back when the dialog closes without an answer', () => {
    renderHook(() => useIncomingTransferNotice());
    act(() => store().addIncomingTransfer(offer('t1')));
    act(() => store().openTransferDialog('t1'));
    mocks.toast.custom.mockClear();
    act(() => store().closeTransferDialog());
    expect(toastIds()).toEqual(['network-transfer-offer-t1']);
  });

  it('does not bring the toast back after an answer', () => {
    renderHook(() => useIncomingTransferNotice());
    act(() => store().addIncomingTransfer(offer('t1')));
    act(() => store().openTransferDialog('t1'));
    mocks.toast.custom.mockClear();
    act(() => {
      store().removeIncomingTransfer('t1');
      store().closeTransferDialog();
    });
    expect(mocks.toast.custom).not.toHaveBeenCalled();
  });

  it('does not bring the toast back for an accepted offer', () => {
    renderHook(() => useIncomingTransferNotice());
    act(() => store().addIncomingTransfer(offer('t1')));
    act(() => store().openTransferDialog('t1'));
    mocks.toast.custom.mockClear();
    act(() => {
      store().markTransferAccepted('t1');
      store().closeTransferDialog();
    });
    expect(mocks.toast.custom).not.toHaveBeenCalled();
  });

  it('stops listening after unmount', () => {
    const view = renderHook(() => useIncomingTransferNotice());
    view.unmount();
    act(() => store().addIncomingTransfer(offer('t1')));
    expect(mocks.toast.custom).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 22: Write the failing dialog test**

Create `src/components/network/__tests__/IncomingTransferDialog.test.tsx`:

```tsx
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { IncomingTransferDialog } from '@/components/network/IncomingTransferDialog';
import {
  type IncomingTransfer,
  INCOMING_TTL_MS,
  useNetworkStore,
} from '@/stores/network-store';
import { createDeferred } from '@/test/deferred';
import { makeIncomingTransfer } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
  api: {
    respondNetworkTransfer: vi.fn(),
    cancelNetworkTransfer: vi.fn(),
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
    requestNetworkPairing: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const store = () => useNetworkStore.getState();

function openOffer(patch: Partial<IncomingTransfer> = {}) {
  const offer = makeIncomingTransfer(patch);
  store().addIncomingTransfer(offer);
  useNetworkStore.setState({ dialogTransferId: offer.transferId });
  return render(<IncomingTransferDialog />);
}

describe('IncomingTransferDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
    mocks.api.respondNetworkTransfer.mockResolvedValue(undefined);
    mocks.api.cancelNetworkTransfer.mockResolvedValue(undefined);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  const SCRIPT_WARNING =
    "This collection's scripts will run in the Safe sandbox when you send its requests. Developer mode, host environment and vault access stay off until you allow them.";

  it('renders nothing while no dialog is open', () => {
    render(<IncomingTransferDialog />);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('shows the sender, the short id, the collection, the size and both buttons', async () => {
    openOffer();
    expect(
      await screen.findByRole('dialog', { name: 'Dana Roy wants to send you a collection' }),
    ).toBeInTheDocument();
    expect(screen.getByText('Orders')).toBeInTheDocument();
    expect(screen.getByText('2 KiB')).toBeInTheDocument();
    expect(screen.getByText('Device ID ZZZZ-9999')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Accept' })).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Decline' })).toBeEnabled();
  });

  it('warns that scripts run in the Safe sandbox before Accept', async () => {
    openOffer();
    const note = await screen.findByRole('note');
    expect(note).toHaveTextContent(SCRIPT_WARNING);
    expect(screen.getByText('Nothing is saved until you accept.')).toBeInTheDocument();
  });

  it('drops the script warning once the offer is accepted', async () => {
    openOffer({ accepted: true });
    await screen.findByRole('dialog');
    expect(screen.queryByRole('note')).not.toBeInTheDocument();
  });

  it('shows a collection name with a double quote as given', async () => {
    openOffer({ collectionName: 'My "Quoted" API' });
    expect(await screen.findByText('My "Quoted" API')).toBeInTheDocument();
  });

  it('names a sender without a display name', async () => {
    openOffer({ displayName: '' });
    expect(
      await screen.findByRole('dialog', { name: 'Unnamed node wants to send you a collection' }),
    ).toBeInTheDocument();
  });

  it('shows 0 bytes', async () => {
    openOffer({ sizeBytes: 0 });
    expect(await screen.findByText('0 bytes')).toBeInTheDocument();
  });

  it('shows a huge size in MiB', async () => {
    openOffer({ sizeBytes: 5 * 1024 * 1024 * 1024 });
    expect(await screen.findByText('5120 MiB')).toBeInTheDocument();
  });

  it('shows the offer whose id the dialog holds', async () => {
    store().addIncomingTransfer(makeIncomingTransfer({ transferId: 'a', collectionName: 'First' }));
    store().addIncomingTransfer(makeIncomingTransfer({ transferId: 'b', collectionName: 'Second' }));
    useNetworkStore.setState({ dialogTransferId: 'b' });
    render(<IncomingTransferDialog />);
    expect(await screen.findByText('Second')).toBeInTheDocument();
    expect(screen.queryByText('First')).not.toBeInTheDocument();
  });

  it('Accept sends the answer and waits for the bytes', async () => {
    openOffer();
    await userEvent.click(await screen.findByRole('button', { name: 'Accept' }));
    await waitFor(() => expect(mocks.api.respondNetworkTransfer).toHaveBeenCalledWith('t1', true));
    expect(await screen.findByText('Receiving from Dana Roy...')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Accept' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Done' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeInTheDocument();
    expect(store().incomingTransfers[0]?.accepted).toBe(true);
  });

  it('Cancel while receiving cancels the transfer and remembers it was local', async () => {
    openOffer({ accepted: true });
    await userEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(mocks.api.cancelNetworkTransfer).toHaveBeenCalledWith('t1'));
    expect(store().wasCancelledHere('t1')).toBe(true);
    expect(mocks.api.respondNetworkTransfer).not.toHaveBeenCalled();
  });

  it('shows the error in a state block when the receiving cancel fails', async () => {
    mocks.api.cancelNetworkTransfer.mockRejectedValue('Internal error: link down');
    openOffer({ accepted: true });
    await userEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
    const block = await screen.findByRole('alert');
    expect(block).toHaveTextContent('link down');
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeEnabled();
  });

  it('Decline sends the answer, removes the offer and closes the dialog', async () => {
    openOffer();
    await userEvent.click(await screen.findByRole('button', { name: 'Decline' }));
    await waitFor(() => expect(mocks.api.respondNetworkTransfer).toHaveBeenCalledWith('t1', false));
    await waitFor(() => expect(store().dialogTransferId).toBeNull());
    expect(store().incomingTransfers).toEqual([]);
  });

  it('shows the error in a state block and keeps the buttons when the answer fails', async () => {
    mocks.api.respondNetworkTransfer.mockRejectedValue('Internal error: link down');
    openOffer();
    await userEvent.click(await screen.findByRole('button', { name: 'Accept' }));
    const block = await screen.findByRole('alert');
    expect(block).toHaveTextContent('link down');
    expect(block).toHaveAttribute('data-slot', 'state-block');
    expect(block).toHaveAttribute('data-tone', 'error');
    expect(screen.getByRole('button', { name: 'Accept' })).toBeEnabled();
    expect(store().incomingTransfers[0]?.accepted).toBe(false);
  });

  it('closing the dialog with Escape neither accepts nor declines', async () => {
    openOffer();
    await screen.findByRole('dialog');
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(store().dialogTransferId).toBeNull());
    expect(mocks.api.respondNetworkTransfer).not.toHaveBeenCalled();
    expect(store().incomingTransfers).toHaveLength(1);
  });

  it('a late Accept answer does not bring a finished offer back', async () => {
    const pending = createDeferred<void>();
    mocks.api.respondNetworkTransfer.mockReturnValue(pending.promise);
    openOffer();
    await userEvent.click(await screen.findByRole('button', { name: 'Accept' }));
    act(() => {
      store().finishTransfer('t1');
      store().closeTransferDialog();
    });
    await act(async () => {
      pending.resolve();
    });
    expect(mocks.api.respondNetworkTransfer).toHaveBeenCalledWith('t1', true);
    expect(store().incomingTransfers).toEqual([]);
  });

  it('a late Decline answer does not close the dialog of another offer', async () => {
    const pending = createDeferred<void>();
    mocks.api.respondNetworkTransfer.mockReturnValue(pending.promise);
    store().addIncomingTransfer(makeIncomingTransfer({ transferId: 't2', collectionName: 'Billing' }));
    openOffer();
    await userEvent.click(await screen.findByRole('button', { name: 'Decline' }));
    act(() => store().openTransferDialog('t2'));
    await act(async () => {
      pending.resolve();
    });
    expect(store().dialogTransferId).toBe('t2');
    expect(store().incomingTransfers.map((t) => t.transferId)).toEqual(['t2']);
  });

  it('closes with the transfer that went away', async () => {
    openOffer();
    await screen.findByRole('dialog');
    act(() => {
      store().finishTransfer('t1');
      store().closeTransferDialog();
    });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('switches to the expired view when the offer expires', () => {
    vi.useFakeTimers();
    openOffer();
    expect(screen.getByRole('button', { name: 'Accept' })).toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(INCOMING_TTL_MS);
    });
    expect(screen.getByRole('dialog', { name: 'Collection offer' })).toBeInTheDocument();
    expect(screen.getByText('The request expired.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Accept' })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Done' }));
    expect(store().dialogTransferId).toBeNull();
  });

  it('keeps the dialog open on an expired finish', async () => {
    openOffer();
    await screen.findByRole('dialog');
    act(() => {
      store().finishTransfer('t1');
    });
    expect(await screen.findByText('The request expired.')).toBeInTheDocument();
  });

  it('shows why an unanswered offer ended', async () => {
    openOffer();
    await screen.findByRole('dialog');
    act(() => {
      store().finishTransfer('t1');
      store().endTransferDialog('The other node went offline.');
    });
    expect(await screen.findByText('The other node went offline.')).toBeInTheDocument();
    expect(screen.queryByText('The request expired.')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Accept' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Done' })).toBeInTheDocument();
  });

  it('shows the progress line only after 300 ms', () => {
    vi.useFakeTimers();
    openOffer({ accepted: true });
    act(() => {
      store().setTransferProgress({
        transferId: 't1',
        direction: 'receive',
        doneBytes: 1024,
        totalBytes: 2048,
      });
    });
    expect(screen.getByText('Receiving from Dana Roy...')).toBeInTheDocument();
    expect(screen.queryByText('1 KiB of 2 KiB')).not.toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(299);
    });
    expect(screen.queryByText('1 KiB of 2 KiB')).not.toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(screen.getByText('1 KiB of 2 KiB')).toBeInTheDocument();
  });

  it('never shows the progress line for a transfer that ends within 300 ms', () => {
    vi.useFakeTimers();
    openOffer({ accepted: true });
    act(() => {
      store().setTransferProgress({
        transferId: 't1',
        direction: 'receive',
        doneBytes: 2048,
        totalBytes: 2048,
      });
      vi.advanceTimersByTime(200);
      store().finishTransfer('t1');
      store().closeTransferDialog();
      vi.advanceTimersByTime(500);
    });
    expect(screen.queryByText('2 KiB of 2 KiB')).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 23: Run all three, expect RED**

Run: `bunx vitest run src/components/network/__tests__/IncomingTransferToast.test.tsx src/hooks/__tests__/useIncomingTransferNotice.test.tsx src/components/network/__tests__/IncomingTransferDialog.test.tsx`
Expected: FAIL, the three modules do not exist (`Failed to resolve import`).

- [ ] **Step 24: Implement the toast**

Create `src/components/network/IncomingTransferToast.tsx`:

```tsx
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { formatBytes, peerName } from '@/lib/network-utils';
import { useNetworkStore } from '@/stores/network-store';

/** One fixed toast id per offer, so a repeated event updates the toast in place. */
export const incomingTransferToastId = (transferId: string): string =>
  `network-transfer-offer-${transferId}`;

interface IncomingTransferToastProps {
  transferId: string;
  displayName: string;
  shortId: string;
  collectionName: string;
  sizeBytes: number;
}

// Nothing is saved before the user presses Accept in the dialog this toast opens.
export function IncomingTransferToast({
  transferId,
  displayName,
  shortId,
  collectionName,
  sizeBytes,
}: IncomingTransferToastProps) {
  const dismiss = () => toast.dismiss(incomingTransferToastId(transferId));

  return (
    <div className='flex w-[356px] max-w-full flex-col gap-2 rounded-md border bg-popover p-4 text-popover-foreground shadow-lg'>
      <p className='break-words text-sm font-medium'>
        {`${peerName(displayName)} wants to send you a collection`}
      </p>
      <p className='break-words text-xs text-muted-foreground'>
        {`${collectionName} (${formatBytes(sizeBytes)})`}
      </p>
      <p className='font-mono text-xs text-muted-foreground'>{shortId}</p>
      <div className='flex flex-wrap items-center gap-2'>
        <Button
          size='sm'
          onClick={() => {
            useNetworkStore.getState().openTransferDialog(transferId);
            dismiss();
          }}
        >
          Review
        </Button>
      </div>
    </div>
  );
}
```

- [ ] **Step 25: Implement the hook**

Create `src/hooks/useIncomingTransferNotice.tsx`:

```tsx
import { useEffect } from 'react';
import { toast } from 'sonner';
import {
  IncomingTransferToast,
  incomingTransferToastId,
} from '@/components/network/IncomingTransferToast';
import { INCOMING_TTL_MS, type IncomingTransfer, useNetworkStore } from '@/stores/network-store';

function showToast(offer: IncomingTransfer): void {
  toast.custom(
    () => (
      <IncomingTransferToast
        transferId={offer.transferId}
        displayName={offer.displayName}
        shortId={offer.shortId}
        collectionName={offer.collectionName}
        sizeBytes={offer.sizeBytes}
      />
    ),
    { id: incomingTransferToastId(offer.transferId), duration: INCOMING_TTL_MS },
  );
}

// Shows one toast for each pending offer and removes it when the offer goes. An accepted offer
// needs no toast. The store drops a repeated event, so a duplicate never reaches this hook.
// Mount once.
export function useIncomingTransferNotice(): void {
  useEffect(() => {
    // A hot reload keeps the store, so pending offers that already exist get their toast too.
    for (const offer of useNetworkStore.getState().incomingTransfers) {
      if (!offer.accepted) showToast(offer);
    }

    return useNetworkStore.subscribe((state, prev) => {
      if (state.incomingTransfers !== prev.incomingTransfers) {
        for (const offer of state.incomingTransfers) {
          if (!prev.incomingTransfers.some((p) => p.transferId === offer.transferId)) {
            showToast(offer);
          }
        }
        for (const offer of prev.incomingTransfers) {
          if (!state.incomingTransfers.some((i) => i.transferId === offer.transferId)) {
            toast.dismiss(incomingTransferToastId(offer.transferId));
          }
        }
      }
      // A dialog closed without an answer leaves the offer pending, so its toast comes back.
      if (prev.dialogTransferId !== null && state.dialogTransferId !== prev.dialogTransferId) {
        const left = state.incomingTransfers.find((i) => i.transferId === prev.dialogTransferId);
        if (left && !left.accepted) showToast(left);
      }
    });
  }, []);
}
```

- [ ] **Step 26: Implement the dialog**

Create `src/components/network/IncomingTransferDialog.tsx`:

```tsx
import { ShieldAlert } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { StateBlock } from '@/components/ui/state-block';
import { useAsyncAction } from '@/hooks/useAsyncAction';
import { formatBytes, peerName } from '@/lib/network-utils';
import { cancelNetworkTransfer, respondNetworkTransfer } from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';

// A transfer that ends faster than this never flashes a progress line.
const PROGRESS_DELAY_MS = 300;

// There is no script gate: a received collection has no trust record, so the trust gate keeps
// the dangerous capabilities off, but its scripts run in the Safe sandbox (spec decision 6).
// The offer has no script count, so the warning has none.
const SCRIPT_WARNING =
  "This collection's scripts will run in the Safe sandbox when you send its requests. Developer mode, host environment and vault access stay off until you allow them.";

const store = () => useNetworkStore.getState();

/**
 * The dialog for an offered collection. It shows the sender, the short device id, the
 * collection name, the size and the script warning, with Accept and Decline. After Accept it
 * shows that the collection is arriving, with Cancel. When the offer is gone it shows why
 * (expired, or the end reason the bridge stored). Mount it once.
 */
export function IncomingTransferDialog() {
  const id = useNetworkStore((s) => s.dialogTransferId);
  const offer = useNetworkStore((s) =>
    s.incomingTransfers.find((t) => t.transferId === s.dialogTransferId),
  );
  const active = useNetworkStore((s) =>
    s.activeTransfers.find((t) => t.transferId === s.dialogTransferId),
  );
  const endedText = useNetworkStore((s) => s.dialogEndedText);
  // A failed call shows its text in a StateBlock, above the buttons.
  const { busy, error, run, reset } = useAsyncAction('inline');
  const [showProgress, setShowProgress] = useState(false);
  const receiving = active !== undefined;

  // Another offer starts with a clean error.
  // biome-ignore lint/correctness/useExhaustiveDependencies: id is the trigger, reset is stable.
  useEffect(() => {
    reset();
  }, [id, reset]);

  // The progress line waits 300 ms after the first bytes arrive.
  // biome-ignore lint/correctness/useExhaustiveDependencies: id restarts the delay for another transfer.
  useEffect(() => {
    setShowProgress(false);
    if (!receiving) return undefined;
    const timer = setTimeout(() => setShowProgress(true), PROGRESS_DELAY_MS);
    return () => clearTimeout(timer);
  }, [id, receiving]);

  if (!id) return null;

  const done = () => store().closeTransferDialog();

  const accept = () =>
    run(async () => {
      await respondNetworkTransfer(id, true);
      // The offer may have finished while the call was in flight. Then it stays gone.
      store().markTransferAccepted(id);
    });

  const decline = () =>
    run(async () => {
      await respondNetworkTransfer(id, false);
      store().removeIncomingTransfer(id);
      // The user may have opened another offer meanwhile. Do not close that one.
      if (store().dialogTransferId === id) store().closeTransferDialog();
    });

  const cancel = () =>
    run(async () => {
      // The finish event that follows is the user's own doing, so it shows no toast.
      store().markCancelledHere(id);
      await cancelNetworkTransfer(id);
    });

  const name = offer ? peerName(offer.displayName) : '';
  const waiting = offer?.accepted ?? false;
  const progressLine =
    showProgress && active
      ? `${formatBytes(active.doneBytes)} of ${formatBytes(active.totalBytes)}`
      : null;

  const title = !offer
    ? 'Collection offer'
    : waiting
      ? `Receiving from ${name}`
      : `${name} wants to send you a collection`;
  const description = !offer
    ? (endedText ?? 'The request expired.')
    : waiting
      ? 'You can close this window. The transfer goes on.'
      : 'Nothing is saved until you accept.';

  return (
    <Dialog
      open
      onOpenChange={(next) => {
        if (!next && !busy) done();
      }}
    >
      <DialogContent className='sm:max-w-sm'>
        <DialogHeader>
          <DialogTitle className='break-words'>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        {offer && (
          <div className='flex flex-col gap-1 py-2'>
            <p className='break-words text-sm font-medium'>{offer.collectionName}</p>
            <p className='text-xs text-muted-foreground'>{formatBytes(offer.sizeBytes)}</p>
            <p className='font-mono text-xs text-muted-foreground'>{`Device ID ${offer.shortId}`}</p>
          </div>
        )}
        {offer && !waiting && (
          <p role='note' className='flex items-start gap-2 rounded-md border p-2 text-xs text-muted-foreground'>
            <ShieldAlert className='mt-0.5 h-3.5 w-3.5 shrink-0' aria-hidden />
            <span>{SCRIPT_WARNING}</span>
          </p>
        )}
        {waiting && (
          <p role='status' className='text-sm text-muted-foreground'>
            {`Receiving from ${name}...`}
          </p>
        )}
        {progressLine && (
          <p role='status' className='text-xs text-muted-foreground'>
            {progressLine}
          </p>
        )}
        {error && <StateBlock tone='error'>{error}</StateBlock>}
        <DialogFooter>
          {!offer ? (
            <Button onClick={done}>Done</Button>
          ) : waiting ? (
            <>
              <Button variant='outline' disabled={busy} onClick={() => void cancel()}>
                Cancel
              </Button>
              <Button onClick={done}>Done</Button>
            </>
          ) : (
            <>
              <Button variant='outline' disabled={busy} onClick={() => void decline()}>
                Decline
              </Button>
              <Button disabled={busy} onClick={() => void accept()}>
                Accept
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

Note for the executor: the title for an accepted offer ("Receiving from Dana Roy") and the status line ("Receiving from Dana Roy...") differ by the three dots, and the tests use `getByText('Receiving from Dana Roy...')` (exact match), so the title does not collide.

- [ ] **Step 27: Mount in App.tsx**

In `src/App.tsx` add two imports in sorted position:

```tsx
import { IncomingTransferDialog } from '@/components/network/IncomingTransferDialog';
```

directly after the `NetworkIntroCard` import line is wrong for sorting (`IncomingTransferDialog` sorts before `NetworkIntroCard`), so put it on the line before `import { NetworkIntroCard } from '@/components/network/NetworkIntroCard';`. Add

```tsx
import { useIncomingTransferNotice } from '@/hooks/useIncomingTransferNotice';
```

directly after the `useIncomingPairingNotice` import. Then replace

```tsx
  useIncomingPairingNotice();
```

with

```tsx
  useIncomingPairingNotice();
  useIncomingTransferNotice();
```

and replace

```tsx
      <PairingDialog />
```

with

```tsx
      <PairingDialog />
      <IncomingTransferDialog />
```

- [ ] **Step 28: Run the task checks**

Run: `bunx vitest run src/lib/__tests__/tauri-api.network.test.ts src/lib/__tests__/network-utils.test.ts src/stores/__tests__/network-store.test.ts src/lib/__tests__/network-event-bridge.test.ts src/components/network src/hooks/__tests__/useIncomingTransferNotice.test.tsx src/hooks/__tests__/useIncomingPairingNotice.test.tsx`
Expected: PASS, every file green (the older pairing tests too).

Run: `bun run tsc --noEmit`
Expected: no output, exit 0. If `AuditEventRow` is not mentioned, that is correct: the audit union changes in Task 3.

Run: `bun run check`
Expected: no findings. If it reports format or import-order findings, run `bun run lint`, then `bun run check` again: no findings.

---

## Task 2: SendCollectionDialog and the "Send to..." menu item

**Files:**
- Create: `src/components/network/SendCollectionDialog.tsx`, `src/components/network/__tests__/SendCollectionDialog.test.tsx`, `src/components/collections/__tests__/CollectionNode.send.test.tsx`
- Modify: `src/components/collections/CollectionNode.tsx`

**Interfaces:**
- Consumes: `sendNetworkCollection` (resolves `SendStarted { transferId, notes }`), `cancelNetworkTransfer`, `transferWaitingToastId`, `toastError`, `onlinePairedNodes`, `selectOnlinePairedCount`, `peerName`, `useAsyncAction`, `FormField`, `EmptyState`, `StateBlock`, `RadioGroup`, `Checkbox`.
- Produces: `SendCollectionDialog({ open, collectionName, onClose })` (after Send: a waiting toast with a Cancel action, and a notes view when the export dropped or blanked something); a "Send to..." item in the `...` dropdown and the right-click menu of `CollectionNode`.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing dialog tests**

Create `src/components/network/__tests__/SendCollectionDialog.test.tsx`:

```tsx
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { SendCollectionDialog } from '@/components/network/SendCollectionDialog';
import { useNetworkStore } from '@/stores/network-store';
import { createDeferred } from '@/test/deferred';
import { makeNode } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
  api: {
    sendNetworkCollection: vi.fn(),
    cancelNetworkTransfer: vi.fn(),
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
    requestNetworkPairing: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const store = () => useNetworkStore.getState();

const started = (notes: string[] = []) => ({ transferId: 't1', notes });

// The options of the waiting toast: its id, its duration and its Cancel action.
const waitingOptions = () =>
  mocks.toast.info.mock.calls[0]?.[1] as {
    id: string;
    duration: number;
    action: { label: string; onClick: () => void };
  };

const ben = makeNode({
  deviceId: 'DEV1',
  shortId: 'AAAA-1111',
  displayName: 'Ben Ortiz',
  group: 'paired',
  status: 'online',
});
const cara = makeNode({
  deviceId: 'DEV2',
  shortId: 'BBBB-2222',
  displayName: 'Cara Lund',
  group: 'paired',
  status: 'online',
});

function setup(nodes = [ben], collectionName = 'Orders') {
  useNetworkStore.setState({ nodes });
  const onClose = vi.fn();
  const view = render(<SendCollectionDialog open collectionName={collectionName} onClose={onClose} />);
  return { onClose, view };
}

describe('SendCollectionDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
    mocks.api.sendNetworkCollection.mockResolvedValue(started());
    mocks.api.cancelNetworkTransfer.mockResolvedValue(undefined);
  });

  it('lists online paired nodes only', async () => {
    setup([
      ben,
      makeNode({ deviceId: 'OFF', displayName: 'Offline Olga', group: 'paired', status: 'offline' }),
      makeNode({ deviceId: 'NEAR', displayName: 'Nearby Nils', group: 'nearby', status: 'available' }),
      cara,
    ]);
    expect(await screen.findByRole('radio', { name: /Ben Ortiz/ })).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: /Cara Lund/ })).toBeInTheDocument();
    expect(screen.queryByText(/Offline Olga/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Nearby Nils/)).not.toBeInTheDocument();
  });

  it('shows the short id next to each name', async () => {
    setup([ben, cara]);
    expect(await screen.findByText('AAAA-1111')).toBeInTheDocument();
    expect(screen.getByText('BBBB-2222')).toBeInTheDocument();
  });

  it('chooses the only online node by itself', async () => {
    setup([ben]);
    expect(await screen.findByRole('radio', { name: /Ben Ortiz/ })).toBeChecked();
    expect(screen.getByRole('button', { name: 'Send' })).toBeEnabled();
  });

  it('asks the user to choose when several nodes are online', async () => {
    setup([ben, cara]);
    expect(await screen.findByRole('button', { name: 'Send' })).toBeDisabled();
    await userEvent.click(screen.getByRole('radio', { name: /Cara Lund/ }));
    expect(screen.getByRole('button', { name: 'Send' })).toBeEnabled();
  });

  it('keeps Include environments off by default and sends false', async () => {
    setup();
    const box = await screen.findByRole('checkbox', { name: 'Include environments' });
    expect(box).not.toBeChecked();
    await userEvent.click(screen.getByRole('button', { name: 'Send' }));
    await waitFor(() =>
      expect(mocks.api.sendNetworkCollection).toHaveBeenCalledWith('DEV1', 'Orders', false),
    );
  });

  it('sends true when Include environments is ticked', async () => {
    setup();
    await userEvent.click(await screen.findByRole('checkbox', { name: 'Include environments' }));
    await userEvent.click(screen.getByRole('button', { name: 'Send' }));
    await waitFor(() =>
      expect(mocks.api.sendNetworkCollection).toHaveBeenCalledWith('DEV1', 'Orders', true),
    );
  });

  it('explains the checkbox with always-visible help text', async () => {
    setup();
    const box = await screen.findByRole('checkbox', { name: 'Include environments' });
    expect(box).toHaveAccessibleDescription(/Off by default/);
  });

  it('passes a name with spaces, quotes and unicode unchanged', async () => {
    const name = '  Orders "v2" – café 日本 (copy)  ';
    setup([ben], name);
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    await waitFor(() =>
      expect(mocks.api.sendNetworkCollection).toHaveBeenCalledWith('DEV1', name, false),
    );
  });

  it('puts a double quote in the title unchanged', async () => {
    setup([ben], 'Orders "v2"');
    expect(await screen.findByRole('dialog', { name: 'Send Orders "v2"' })).toBeInTheDocument();
  });

  it('closes at once when there are no notes and tells the user to wait for the answer', async () => {
    const { onClose } = setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
    expect(mocks.toast.info).toHaveBeenCalledWith(
      'Sent to Ben Ortiz. Waiting for them to accept.',
      expect.objectContaining({ id: 'network-transfer-waiting-t1' }),
    );
    expect(screen.queryByText('Not sent with the collection')).not.toBeInTheDocument();
  });

  it('puts a Cancel action on the waiting toast', async () => {
    setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    await waitFor(() => expect(mocks.toast.info).toHaveBeenCalledTimes(1));
    expect(waitingOptions().action.label).toBe('Cancel');
    // The toast stays until the transfer ends: the bridge dismisses it.
    expect(waitingOptions().duration).toBe(Number.POSITIVE_INFINITY);
  });

  it('Cancel on the waiting toast cancels that transfer', async () => {
    setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    await waitFor(() => expect(mocks.toast.info).toHaveBeenCalledTimes(1));
    waitingOptions().action.onClick();
    await waitFor(() => expect(mocks.api.cancelNetworkTransfer).toHaveBeenCalledWith('t1'));
    expect(mocks.toast.error).not.toHaveBeenCalled();
  });

  it('remembers the send for the bridge and marks a Cancel as local', async () => {
    setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    await waitFor(() => expect(mocks.toast.info).toHaveBeenCalledTimes(1));
    // The bridge turns the waiting toast into "Sending to Ben Ortiz..." from this entry.
    expect(store().advanceOutgoing('t1', 'sending')).toBe('Ben Ortiz');
    waitingOptions().action.onClick();
    expect(store().wasCancelledHere('t1')).toBe(true);
  });

  it('shows the name rule error inline', async () => {
    mocks.api.sendNetworkCollection.mockRejectedValue(
      'Invalid input: This collection name cannot be sent. Rename it and try again.',
    );
    setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'This collection name cannot be sent. Rename it and try again.',
    );
  });

  it('shows the error when the cancel is refused', async () => {
    mocks.api.cancelNetworkTransfer.mockRejectedValue(
      'Invalid input: Already sent. Waiting for the result.',
    );
    setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    await waitFor(() => expect(mocks.toast.info).toHaveBeenCalledTimes(1));
    waitingOptions().action.onClick();
    await waitFor(() =>
      expect(mocks.toast.error).toHaveBeenCalledWith('Already sent. Waiting for the result.'),
    );
  });

  it('lists the notes after Send and keeps the dialog open until Done', async () => {
    mocks.api.sendNetworkCollection.mockResolvedValue(
      started(['Flows: skipped', 'Bearer token: blanked']),
    );
    const { onClose } = setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    expect(await screen.findByText('Not sent with the collection')).toBeInTheDocument();
    expect(screen.getByText('Flows: skipped')).toBeInTheDocument();
    expect(screen.getByText('Bearer token: blanked')).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
    // The waiting toast shows in both cases.
    expect(mocks.toast.info).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('button', { name: 'Send' })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Done' }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('shows a note with a double quote as given', async () => {
    mocks.api.sendNetworkCollection.mockResolvedValue(started(['Auth "token" in Orders "v2": blanked']));
    setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    expect(await screen.findByText('Auth "token" in Orders "v2": blanked')).toBeInTheDocument();
  });

  it('names a node without a display name by its short id', async () => {
    const { onClose } = setup([makeNode({ ...ben, displayName: '' })]);
    expect(await screen.findByRole('radio', { name: /AAAA-1111/ })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Send' }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(mocks.toast.info).toHaveBeenCalledWith(
      'Sent to Unnamed node. Waiting for them to accept.',
      expect.objectContaining({ id: 'network-transfer-waiting-t1' }),
    );
  });

  it('shows the error inline, keeps the dialog open and enables Send again', async () => {
    mocks.api.sendNetworkCollection.mockRejectedValue(
      'Invalid input: This collection is too big to send (11 MiB, limit 10).',
    );
    const { onClose } = setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    const block = await screen.findByRole('alert');
    expect(block).toHaveTextContent('This collection is too big to send (11 MiB, limit 10).');
    expect(block).toHaveAttribute('data-slot', 'state-block');
    expect(block).toHaveAttribute('data-tone', 'error');
    expect(onClose).not.toHaveBeenCalled();
    expect(mocks.toast.info).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'Send' })).toBeEnabled();
  });

  it('disables Send and Cancel while the call is in flight', async () => {
    const pending = createDeferred<ReturnType<typeof started>>();
    mocks.api.sendNetworkCollection.mockReturnValue(pending.promise);
    const { onClose } = setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled();
    await userEvent.keyboard('{Escape}');
    expect(onClose).not.toHaveBeenCalled();
    await act(async () => {
      pending.resolve(started());
    });
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
  });

  it('Cancel closes without sending', async () => {
    const { onClose } = setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(mocks.api.sendNetworkCollection).not.toHaveBeenCalled();
  });

  it('shows a needs-setup state and a disabled Send when no node is online', async () => {
    setup([makeNode({ group: 'paired', status: 'offline' })]);
    expect(await screen.findByText('No paired node is online')).toBeInTheDocument();
    expect(document.querySelector('[data-slot="empty-state"]')).toHaveAttribute(
      'data-cause',
      'needs-setup',
    );
    expect(screen.queryByRole('radio')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
  });

  it('disables Send when the chosen node goes offline', async () => {
    setup([ben, cara]);
    await userEvent.click(await screen.findByRole('radio', { name: /Ben Ortiz/ }));
    expect(screen.getByRole('button', { name: 'Send' })).toBeEnabled();
    act(() => useNetworkStore.setState({ nodes: [{ ...ben, status: 'offline' }, cara] }));
    expect(screen.queryByRole('radio', { name: /Ben Ortiz/ })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
    expect(screen.getByRole('alert')).toHaveTextContent('That node went offline. Choose another one.');
  });

  it('does not pick another node by itself when the chosen one goes offline', async () => {
    setup([ben, cara]);
    await userEvent.click(await screen.findByRole('radio', { name: /Ben Ortiz/ }));
    act(() => useNetworkStore.setState({ nodes: [{ ...ben, status: 'offline' }, cara] }));
    // Only Cara is left, and she is still not chosen.
    expect(screen.getByRole('radio', { name: /Cara Lund/ })).not.toBeChecked();
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
    expect(mocks.api.sendNetworkCollection).not.toHaveBeenCalled();
  });

  it('switches to the empty state when the last node goes offline', async () => {
    setup([ben]);
    await screen.findByRole('radio', { name: /Ben Ortiz/ });
    act(() => useNetworkStore.setState({ nodes: [{ ...ben, status: 'offline' }] }));
    expect(screen.getByText('No paired node is online')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
  });

  it('starts clean when it opens again', async () => {
    const { view, onClose } = setup();
    await userEvent.click(await screen.findByRole('checkbox', { name: 'Include environments' }));
    view.rerender(<SendCollectionDialog open={false} collectionName='Orders' onClose={onClose} />);
    view.rerender(<SendCollectionDialog open collectionName='Orders' onClose={onClose} />);
    expect(await screen.findByRole('checkbox', { name: 'Include environments' })).not.toBeChecked();
  });

  it('does not show the notes of an earlier send when it opens again', async () => {
    mocks.api.sendNetworkCollection.mockResolvedValue(started(['Flows: skipped']));
    const { view, onClose } = setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Send' }));
    expect(await screen.findByText('Flows: skipped')).toBeInTheDocument();
    view.rerender(<SendCollectionDialog open={false} collectionName='Orders' onClose={onClose} />);
    view.rerender(<SendCollectionDialog open collectionName='Orders' onClose={onClose} />);
    expect(screen.queryByText('Flows: skipped')).not.toBeInTheDocument();
    expect(await screen.findByRole('button', { name: 'Send' })).toBeInTheDocument();
  });
});
```

- [ ] **Step 3: Run, expect RED**

Run: `bunx vitest run src/components/network/__tests__/SendCollectionDialog.test.tsx`
Expected: FAIL, `Failed to resolve import "@/components/network/SendCollectionDialog"`.

- [ ] **Step 4: Implement the dialog**

Create `src/components/network/SendCollectionDialog.tsx`:

```tsx
import { Loader2, Send } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { EmptyState } from '@/components/ui/empty-state';
import { FormField } from '@/components/ui/form-field';
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { StateBlock } from '@/components/ui/state-block';
import { useAsyncAction } from '@/hooks/useAsyncAction';
import { toastError } from '@/lib/error-text';
import { peerName, transferWaitingToastId } from '@/lib/network-utils';
import { cancelNetworkTransfer, type SendStarted, sendNetworkCollection } from '@/lib/tauri-api';
import { onlinePairedNodes, useNetworkStore } from '@/stores/network-store';

interface SendCollectionDialogProps {
  open: boolean;
  collectionName: string;
  onClose: () => void;
}

// The toast stays until the transfer ends: the event bridge dismisses it on the finish event.
function showWaitingToast(transferId: string, name: string): void {
  toast.info(`Sent to ${name}. Waiting for them to accept.`, {
    id: transferWaitingToastId(transferId),
    duration: Number.POSITIVE_INFINITY,
    action: {
      label: 'Cancel',
      onClick: () => {
        // The finish that follows is the user's own doing, so the bridge shows no toast for it.
        useNetworkStore.getState().markCancelledHere(transferId);
        // A cancel after the last chunk is refused. The user sees why.
        cancelNetworkTransfer(transferId).catch((err) => toastError(err));
      },
    },
  });
}

/**
 * Sends one collection to a paired node that is online. The other user is asked before
 * anything is saved. Environments are off by default. When the export dropped or blanked
 * something, the dialog lists those notes before it closes. Mount it as a sibling of the menu.
 */
export function SendCollectionDialog({ open, collectionName, onClose }: SendCollectionDialogProps) {
  // The list is filtered here, because a selector must not build a new array.
  const nodes = useNetworkStore((s) => s.nodes);
  const online = useMemo(() => onlinePairedNodes(nodes), [nodes]);
  const [picked, setPicked] = useState<string | null>(null);
  const [includeEnvironments, setIncludeEnvironments] = useState(false);
  // The export notes of a started send. Null while the form shows.
  const [notes, setNotes] = useState<string[] | null>(null);
  // A failed send shows its text in the dialog.
  const { busy, error, run, reset } = useAsyncAction('inline');

  // Every open starts clean.
  useEffect(() => {
    if (!open) return;
    setPicked(null);
    setIncludeEnvironments(false);
    setNotes(null);
    reset();
  }, [open, reset]);

  const pickedNode = picked === null ? undefined : online.find((n) => n.deviceId === picked);
  const lost = picked !== null && pickedNode === undefined;
  // One online node is chosen for the user. If the user chose a node and it left, nothing is
  // chosen: another node must never receive the collection by surprise.
  const chosen = pickedNode ?? (picked === null && online.length === 1 ? online[0] : undefined);

  async function send() {
    if (!chosen) return;
    let result = null as SendStarted | null;
    const ok = await run(async () => {
      result = await sendNetworkCollection(chosen.deviceId, collectionName, includeEnvironments);
    });
    if (!ok || !result) return;
    const sent: SendStarted = result;
    // The bridge updates the waiting toast of a send it knows (index R19).
    useNetworkStore.getState().trackOutgoing(sent.transferId, chosen.displayName);
    showWaitingToast(sent.transferId, peerName(chosen.displayName));
    if (sent.notes.length === 0) onClose();
    else setNotes(sent.notes);
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next && !busy) onClose();
      }}
    >
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <DialogTitle className='break-words'>{`Send ${collectionName}`}</DialogTitle>
          <DialogDescription>
            The other user is asked before anything is saved. Credentials and secret values are
            blanked, as in an export.
          </DialogDescription>
        </DialogHeader>
        {notes !== null ? (
          <div className='space-y-2'>
            <p className='text-sm font-medium'>Not sent with the collection</p>
            <ul className='list-disc space-y-1 pl-5 text-xs text-muted-foreground'>
              {notes.map((note, index) => (
                // Notes can repeat, so the index is part of the key.
                <li key={`${index}-${note}`} className='break-words'>
                  {note}
                </li>
              ))}
            </ul>
          </div>
        ) : online.length === 0 ? (
          <EmptyState
            cause='needs-setup'
            size='compact'
            title='No paired node is online'
            reason='Start Rocket on the other computer, or pair it in the Network panel.'
          />
        ) : (
          <div className='space-y-4'>
            <FormField
              id='send-node'
              group
              label='Send to'
              help='Only paired nodes that are online are listed.'
              error={lost ? 'That node went offline. Choose another one.' : null}
            >
              {() => (
                <RadioGroup
                  aria-label='Send to'
                  value={chosen?.deviceId ?? ''}
                  onValueChange={setPicked}
                  disabled={busy}
                  className='gap-2'
                >
                  {online.map((node) => (
                    <div key={node.deviceId} className='flex items-center gap-2'>
                      <RadioGroupItem value={node.deviceId} id={`send-node-${node.deviceId}`} />
                      <Label
                        htmlFor={`send-node-${node.deviceId}`}
                        className='text-sm font-normal'
                      >
                        {node.displayName.trim() ? `${node.displayName} ` : ''}
                        <span className='font-mono text-xs text-muted-foreground'>
                          {node.shortId}
                        </span>
                      </Label>
                    </div>
                  ))}
                </RadioGroup>
              )}
            </FormField>
            <FormField
              id='send-environments'
              group
              label='Environments'
              help='Off by default. Environment variables are sent with credentials and secret values blanked.'
            >
              {(a11y) => (
                <div className='flex items-center gap-2'>
                  <Checkbox
                    id='send-environments-box'
                    aria-describedby={a11y['aria-describedby']}
                    checked={includeEnvironments}
                    disabled={busy}
                    onCheckedChange={(v) => setIncludeEnvironments(v === true)}
                  />
                  <Label htmlFor='send-environments-box' className='text-sm font-normal'>
                    Include environments
                  </Label>
                </div>
              )}
            </FormField>
            {error && <StateBlock tone='error'>{error}</StateBlock>}
          </div>
        )}
        <DialogFooter>
          {notes !== null ? (
            <Button onClick={onClose}>Done</Button>
          ) : (
            <>
              <Button variant='outline' disabled={busy} onClick={onClose}>
                Cancel
              </Button>
              <Button disabled={busy || !chosen} onClick={() => void send()}>
                {busy ? (
                  <Loader2 className='mr-2 h-4 w-4 animate-spin' />
                ) : (
                  <Send className='mr-2 h-4 w-4' />
                )}
                Send
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

Notes for the executor: the notes are the human lines the backend built from the export report. They are data and render as plain text. The anchor-free `key` uses the index because two notes can be identical (the Biome rule `noArrayIndexKey` may flag it: keep the comment and add a `biome-ignore` line only if `bun run check` reports it). A node whose display name is blank shows only its short id in the radio label, so the accessible name still contains `AAAA-1111`. `FormField` in group mode puts `aria-describedby` on the wrapper's children only through the render prop, which is why the checkbox forwards it by hand. If the radio group warns about a controlled value of `''`, that is the Radix way to show "nothing chosen".

- [ ] **Step 5: Run, expect GREEN**

Run: `bunx vitest run src/components/network/__tests__/SendCollectionDialog.test.tsx`
Expected: PASS, all tests. If `has accessible description` fails because the FormField help id is `send-environments-help` and the checkbox gets `aria-describedby` from the render prop, check that the prop is passed (the test pins it).

- [ ] **Step 6: Write the failing menu tests**

Create `src/components/collections/__tests__/CollectionNode.send.test.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CollectionNode } from '@/components/collections/CollectionNode';
import type { CollectionSummary } from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { useNetworkStore } from '@/stores/network-store';
import { makeNode } from '@/test/network-fixtures';

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => undefined),
}));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getCollectionSummaries: vi.fn(),
    getOpenApiSyncStatus: vi.fn(),
    getOpenApiSyncBadges: vi.fn(),
    sendNetworkCollection: vi.fn(),
    // biome-ignore lint/suspicious/noEmptyBlockStatements: unlisten stub.
    onCollectionChanged: vi.fn().mockResolvedValue(() => {}),
  };
});

const makeSummary = (name: string): CollectionSummary => ({
  uid: 'col-1',
  repositoryId: 'repo-1',
  name,
  path: `/workspace/collections/${name}`,
  requestCount: 0,
});

const online = makeNode({
  deviceId: 'DEV1',
  displayName: 'Ben Ortiz',
  group: 'paired',
  status: 'online',
});
const offline = makeNode({ deviceId: 'DEV2', group: 'paired', status: 'offline' });

function renderNode(summary: CollectionSummary) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <CollectionNode
        summary={summary}
        filter=''
        summaries={[summary]}
        onNewFolder={vi.fn()}
        onMove={vi.fn()}
        onDelete={vi.fn()}
        onDuplicate={vi.fn()}
      />
    </QueryClientProvider>,
  );
}

type Menu = 'dropdown' | 'context';

// Opens the `...` menu with the pointer, or the right-click menu on the row.
async function openMenu(menu: Menu, name: string, user: ReturnType<typeof userEvent.setup>) {
  if (menu === 'dropdown') {
    await user.hover(await screen.findByLabelText(`Actions for ${name}`));
    await waitFor(() => expect(tauriApi.getOpenApiSyncStatus).toHaveBeenCalled());
    await act(async () => {
      await Promise.resolve();
    });
    await user.click(screen.getByLabelText(`Actions for ${name}`));
  } else {
    fireEvent.contextMenu(await screen.findByText(name));
  }
  await screen.findByRole('menuitem', { name: /Overview/ });
}

describe('CollectionNode Send to... entry', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    useNetworkStore.getState().reset();
    vi.mocked(tauriApi.getOpenApiSyncBadges).mockResolvedValue([]);
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(null);
    vi.mocked(tauriApi.sendNetworkCollection).mockResolvedValue({ transferId: 't1', notes: [] });
    vi.mocked(tauriApi.getCollectionSummaries).mockResolvedValue({
      name: 'my-collection',
      root: { uid: 'root', name: 'my-collection', items: [] },
      settings: { headers: [], variables: [], sandboxMode: 'safe' },
    });
  });

  describe.each<Menu>(['dropdown', 'context'])('in the %s menu', (menu) => {
    it('is hidden with zero nodes', async () => {
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      expect(screen.queryByRole('menuitem', { name: /Send to/ })).toBeNull();
    });

    it('is hidden when the only paired node is offline', async () => {
      useNetworkStore.setState({ nodes: [offline] });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      expect(screen.queryByRole('menuitem', { name: /Send to/ })).toBeNull();
    });

    it('is visible with one online paired node', async () => {
      useNetworkStore.setState({ nodes: [online, offline] });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      expect(screen.getByRole('menuitem', { name: /Send to/ })).toBeInTheDocument();
    });

    it('opens the dialog and sends the collection', async () => {
      useNetworkStore.setState({ nodes: [online] });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      await user.click(screen.getByRole('menuitem', { name: /Send to/ }));
      expect(await screen.findByRole('dialog', { name: 'Send my-collection' })).toBeInTheDocument();
      await user.click(screen.getByRole('button', { name: 'Send' }));
      await waitFor(() =>
        expect(tauriApi.sendNetworkCollection).toHaveBeenCalledWith('DEV1', 'my-collection', false),
      );
    });

    it('passes a collection name with spaces and unicode unchanged', async () => {
      const name = 'Orders v2 – café "x"';
      useNetworkStore.setState({ nodes: [online] });
      const user = userEvent.setup();
      renderNode(makeSummary(name));
      await openMenu(menu, name, user);
      await user.click(screen.getByRole('menuitem', { name: /Send to/ }));
      await user.click(await screen.findByRole('button', { name: 'Send' }));
      await waitFor(() =>
        expect(tauriApi.sendNetworkCollection).toHaveBeenCalledWith('DEV1', name, false),
      );
    });
  });

  it('keeps the dialog mounted when the last node goes offline', async () => {
    useNetworkStore.setState({ nodes: [online] });
    const user = userEvent.setup();
    renderNode(makeSummary('my-collection'));
    await openMenu('dropdown', 'my-collection', user);
    await user.click(screen.getByRole('menuitem', { name: /Send to/ }));
    await screen.findByRole('dialog', { name: 'Send my-collection' });
    act(() => useNetworkStore.setState({ nodes: [{ ...online, status: 'offline' }] }));
    expect(await screen.findByText('No paired node is online')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
  });
});
```

- [ ] **Step 7: Run, expect RED**

Run: `bunx vitest run src/components/collections/__tests__/CollectionNode.send.test.tsx`
Expected: FAIL, the "is visible" and "opens the dialog" tests cannot find a `Send to` menu item. The "is hidden" tests pass already.

- [ ] **Step 8: Add the menu item and the dialog**

Edit `src/components/collections/CollectionNode.tsx`.

(a) In the `lucide-react` import replace

```tsx
  Plus,
  Trash2,
```

with

```tsx
  Plus,
  Send,
  Trash2,
```

(b) After `import { ExportCollectionDialog } from '@/components/export/ExportCollectionDialog';` add:

```tsx
import { SendCollectionDialog } from '@/components/network/SendCollectionDialog';
```

(c) After `import { useContractsStore } from '@/stores/contracts/contractsSlice';` add:

```tsx
import { selectOnlinePairedCount, useNetworkStore } from '@/stores/network-store';
```

(d) After `const [exportOpen, setExportOpen] = useState(false);` add:

```tsx
  const [sendOpen, setSendOpen] = useState(false);
  // A boolean selector: the row re-renders only when the answer flips.
  const canSend = useNetworkStore((s) => selectOnlinePairedCount(s) > 0);
```

(e) In the `DropdownMenuContent`, replace

```tsx
              <DropdownMenuItem onClick={() => setExportOpen(true)}>
                <Download aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Export...
              </DropdownMenuItem>
```

with

```tsx
              <DropdownMenuItem onClick={() => setExportOpen(true)}>
                <Download aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Export...
              </DropdownMenuItem>
              {canSend && (
                <DropdownMenuItem onClick={() => setSendOpen(true)}>
                  <Send aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Send to...
                </DropdownMenuItem>
              )}
```

(f) In the `ContextMenuContent`, replace

```tsx
        <ContextMenuItem onClick={() => setExportOpen(true)}>
          <Download aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Export...
        </ContextMenuItem>
```

with

```tsx
        <ContextMenuItem onClick={() => setExportOpen(true)}>
          <Download aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Export...
        </ContextMenuItem>
        {canSend && (
          <ContextMenuItem onClick={() => setSendOpen(true)}>
            <Send aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Send to...
          </ContextMenuItem>
        )}
```

(g) After the `ExportCollectionDialog` element, replace

```tsx
        onClose={() => setExportOpen(false)}
      />
```

with

```tsx
        onClose={() => setExportOpen(false)}
      />
      {(canSend || sendOpen) && (
        <SendCollectionDialog
          open={sendOpen}
          collectionName={summary.name}
          onClose={() => setSendOpen(false)}
        />
      )}
```

The dialog stays mounted while it is open even if the last node goes offline, so the user sees the empty state instead of a dialog that vanishes.

- [ ] **Step 9: Run the task checks**

Run: `bunx vitest run src/components/network/__tests__/SendCollectionDialog.test.tsx src/components/collections/__tests__`
Expected: PASS, the new files and the older `CollectionNode*` tests.

Run: `bun run tsc --noEmit`
Expected: no output, exit 0.

Run: `bun run check`
Expected: no findings. If it reports format or import-order findings, run `bun run lint`, then `bun run check` again: no findings.

---

## Task 3: Audit rows, docs, manual checks and roadmap

**Files:**
- Modify: `src/lib/tauri-api.ts`, `src/components/audit/AuditEventRow.tsx`, `src/components/audit/__tests__/AuditEventRow.network.test.tsx`, `docs/features/team-network.md`, `docs/manual-tests/team-network.md`, `docs/roadmaps/roadmap.md`, `docs/roadmaps/contracts-ci-ai-roadmap.md`, `.claude/docs-index.md` (only this plan's hunk, ruling 18)

**Interfaces:**
- Consumes: `shortDeviceId`; the Rust kinds `network_collection_sent` (outcome `completed` or `unconfirmed`) and `network_collection_received` (outcome `completed`, `damaged` or `rejected`) with fields `device_id`, `collection` and `outcome` (snake_case wire names, the Rust enum has no rename; index R6).
- Produces: two `AuditEventKind` union members and their row text; docs.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing audit row tests**

In `src/components/audit/__tests__/AuditEventRow.network.test.tsx` add these tests inside the `describe` block, after `describes a block`. The kinds use the WIRE names: the fields are `device_id`, `collection` and `outcome` (snake_case, the Rust enum has no rename). `FULL_ID` is the 44-character id of the file, and `'K7QF-2M9X'` is a short id (9 characters), which `shortDeviceId` returns unchanged:

```tsx
  it('describes a collection that was sent', () => {
    show({ kind: 'network_collection_sent', device_id: FULL_ID, collection: 'Orders', outcome: 'completed' });
    expect(screen.getByText('Sent collection Orders to device K7QF-2M9X')).toBeInTheDocument();
  });

  it('describes a collection that was received', () => {
    show({ kind: 'network_collection_received', device_id: FULL_ID, collection: 'Orders', outcome: 'completed' });
    expect(screen.getByText('Received collection Orders from device K7QF-2M9X')).toBeInTheDocument();
  });

  it('describes a collection that was sent with a short id', () => {
    show({ kind: 'network_collection_sent', device_id: 'K7QF-2M9X', collection: 'Orders', outcome: 'completed' });
    expect(screen.getByText('Sent collection Orders to device K7QF-2M9X')).toBeInTheDocument();
  });

  it('describes a collection that was received with a short id', () => {
    show({ kind: 'network_collection_received', device_id: 'K7QF-2M9X', collection: 'Orders', outcome: 'completed' });
    expect(screen.getByText('Received collection Orders from device K7QF-2M9X')).toBeInTheDocument();
  });

  it('shows a collection name with a double quote and unicode as given', () => {
    show({
      kind: 'network_collection_sent',
      device_id: FULL_ID,
      collection: 'Orders "v2" café',
      outcome: 'completed',
    });
    expect(
      screen.getByText('Sent collection Orders "v2" café to device K7QF-2M9X'),
    ).toBeInTheDocument();
  });

  it('shows an unconfirmed send', () => {
    show({ kind: 'network_collection_sent', device_id: FULL_ID, collection: 'Orders', outcome: 'unconfirmed' });
    expect(
      screen.getByText('Sent collection Orders to device K7QF-2M9X (unconfirmed)'),
    ).toBeInTheDocument();
  });

  it('shows a rejected and a damaged receive', () => {
    show({ kind: 'network_collection_received', device_id: FULL_ID, collection: 'Orders', outcome: 'rejected' });
    expect(
      screen.getByText('Received collection Orders from device K7QF-2M9X (rejected)'),
    ).toBeInTheDocument();
    cleanup();
    show({ kind: 'network_collection_received', device_id: FULL_ID, collection: 'Orders', outcome: 'damaged' });
    expect(
      screen.getByText('Received collection Orders from device K7QF-2M9X (damaged)'),
    ).toBeInTheDocument();
  });
```

If the file does not import `cleanup` from `@testing-library/react` yet, add it to that import.

- [ ] **Step 3: Run, expect RED**

Run: `bunx vitest run src/components/audit/__tests__/AuditEventRow.network.test.tsx`
Expected: FAIL, seven tests (the two full-id tests, the two short-id tests, the odd-name test, the unconfirmed test and the rejected/damaged test). Vitest does not type-check, so the failure is the missing text (`Unable to find an element with the text`) because `summarize` has no case for the new kinds and returns `undefined`.

- [ ] **Step 4: Add the kinds and the row text**

In `src/lib/tauri-api.ts` replace

```ts
  | { kind: 'network_device_blocked'; device_id: string };
```

with

```ts
  | { kind: 'network_device_blocked'; device_id: string }
  | { kind: 'network_collection_sent'; device_id: string; collection: string; outcome: string }
  | { kind: 'network_collection_received'; device_id: string; collection: string; outcome: string };
```

In `src/components/audit/AuditEventRow.tsx` replace

```tsx
    case 'network_device_blocked':
      return `Blocked device ${shortDeviceId(k.device_id)}`;
```

with

```tsx
    case 'network_device_blocked':
      return `Blocked device ${shortDeviceId(k.device_id)}`;
    case 'network_collection_sent':
      return `Sent collection ${k.collection} to device ${shortDeviceId(k.device_id)}${outcomeSuffix(k.outcome)}`;
    case 'network_collection_received':
      return `Received collection ${k.collection} from device ${shortDeviceId(k.device_id)}${outcomeSuffix(k.outcome)}`;
```

and add this helper above `summarize`:

```tsx
// A completed transfer needs no word. Any other outcome is shown as its wire name.
function outcomeSuffix(outcome: string): string {
  return outcome === 'completed' ? '' : ` (${outcome})`;
}
```

- [ ] **Step 5: Run, expect GREEN**

Run: `bunx vitest run src/components/audit`
Expected: PASS.

Run: `bun run tsc --noEmit`
Expected: no output, exit 0. (The exhaustive `switch` in `summarize` is why Step 4 touches both files.)

- [ ] **Step 6: Feature doc**

In `docs/features/team-network.md`:

(a) Replace the first paragraph sentence "This is the base for sending collections and checking contracts between nodes (later projects)." with "Paired nodes can send each other a collection (see Send a collection). Checking contracts between nodes is a later project." Add the send spec to the spec list: after "Spec: `docs/superpowers/specs/2026-11-05-team-network-discovery-pairing-design.md`." insert " Send a collection: `docs/superpowers/specs/2026-11-06-team-network-send-collection-design.md`, plans in `docs/superpowers/plans/team-network-send/`."

(b) In "Where the code lives" add one table row before the `Data` row:

```
| Send a collection | Protocol and state machines `crates/rocket-network` (`transfer.rs`, `limits.rs`), `TransferService` in `crates/rocket-app/src/network/`, strict ZIP check `crates/rocket-import/src/transfer_zip.rs`, staging in `crates/rocket-infra`, import adapter and commands in `src-tauri`. Frontend: `src/components/network/SendCollectionDialog.tsx`, `IncomingTransferDialog.tsx`, `IncomingTransferToast.tsx`, `src/hooks/useIncomingTransferNotice.tsx`, the `transfers` slice in `src/stores/network-store.ts`, `handleNetworkTransfer` in `src/lib/network-event-bridge.ts`. |
```

(c) In "Limits" replace the line "- No sending of collections or contracts yet." with "- No sending of contracts yet. A collection can be sent as described in Send a collection."

(d) Insert this section before "## Decisions":

```markdown
## Send a collection

A one-time push of one collection to a paired node that is online. There is no ongoing link, no merge and no request-a-collection.

Flow:

1. On the sender, **Send to...** in the collection menu (the `...` menu and the right-click menu). The item shows only while at least one paired node is online. The dialog lists online paired nodes, has **Include environments** (off by default) and a Send button.
2. The sender exports the collection as the Bruno ZIP (the same export, same blanking). A ZIP over 10 MiB stops the send with "This collection is too big to send (X MiB, limit 10)." and no offer is made.
3. The receiver gets a toast and a dialog with the sender, the short device id, the collection name and the size. Nothing is written before Accept. An unanswered offer expires after 2 minutes.
4. After Accept the sender streams the ZIP in 32 KiB chunks over the existing peer connection, at most 32 chunks ahead of the receiver's acks (`ChunkAck` every 16 chunks). The receiver checks size and SHA-256, validates the ZIP, imports it as a new collection in the active workspace and sends a result frame (`TransferResult`). The sender shows "Sent" only after that frame arrives (up to 60 seconds), so a damaged or rejected collection is never reported as sent. If the receiver goes away after the last chunk, or no result comes within 60 seconds, the sender shows "Sent, but the other node did not confirm." Both sides get a result toast.
5. After Send the sender sees "Sent to <name>. Waiting for them to accept." with a **Cancel** action. Once bytes move it reads "Sending to <name>..."; after the last chunk the Cancel action goes (a cancel then would be refused with "Already sent. Waiting for the result."). The receiver can cancel too, with **Cancel** in the receiving dialog. If the export dropped or blanked something (skipped flows, blanked tokens), the send dialog stays open and lists those notes until Done.

Limits: 10 MiB per transfer, 3 pending offers across all peers, 1 active transfer per peer in each direction, 5 offers per peer per minute, 50 MiB unzipped and 5000 entries on import. An offer over a limit is refused with "<name> is busy. Try again in a minute."

What leaves the machine: the export blanks credentials, auth fields and secret-named values. Request bodies, scripts and descriptions are sent as written and are not scanned. Flows, gRPC and WebSocket requests, client certificates and proxy settings are not sent (the export reports them).

Name clash: the receiver never overwrites. The offered name is used if it is free. Otherwise the collection is named `<name> (from <sender>)`, and the importer adds `-1`, `-2` if that is taken too.

Scripts and trust: a received collection has no trust record, so the collection trust gate (`docs/features/collection-trust.md`) gives it no Developer mode, no extra script folders, no agent runs, no host environment and no RocketVault access until the receiver allows them, even if the ZIP asks for Developer mode. Its pre-request and test scripts DO run, in the Safe sandbox, from the first Send. There is no separate script gate. The incoming dialog warns about this before Accept (no script count: the offer does not carry one).

Version: sending needs protocol version 2. A node still on version 1 shows as "needs update", and a pairing request from or to it is refused, until both nodes run a build with send.

Reload: a full reload of the window while an offer is pending drops it from view. It is not shown again; it expires after 2 minutes and the sender sees "The request expired." (a hot reload keeps it).

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
| Import fails | "Sending failed. Try again." | The importer's text from the finished event `message`, else "Receiving failed." |
| Sender cancelled | nothing | "<name> cancelled the transfer." |
| Receiver cancelled while receiving | "<name> cancelled the transfer." | nothing |
| Collection name breaks the name rule | "This collection name cannot be sent. Rename it and try again." (inline in the send dialog) | nothing |

Frontend rules:

- `network-transfer` events feed the `transfers` slice: `offerIncoming` adds to `incomingTransfers`, `progress` updates `activeTransfers`, `finished` removes the transfer and shows the result toast. Fields are snake_case as sent.
- An offer is deduped by transfer id and expires 120 seconds after it arrived. A repeated event neither adds a row nor extends the expiry. An accepted offer has no timer.
- The store remembers the last 100 finished transfer ids, so a late progress or offer event and a repeated finish do not leave a ghost row or a second toast. Each result toast has one fixed id per transfer.
- Only the sender gets the "declined" toast.
- The `finished` event has `message` (string or null). Only the receiver's `failed` toast shows it. A pending offer that fails with a message keeps its open dialog and shows the message there.
- A `cancelled` finish shows "<name> cancelled the transfer." unless the local user pressed Cancel on that transfer (the store remembers it until the finish).
- An `unconfirmed` send shows an info toast, not an error.
- The "Waiting for them to accept." toast has its own id (`network-transfer-waiting-<id>`) and stays until the transfer finishes. The bridge turns it into "Sending to <name>..." on the first send progress, removes its Cancel when the last chunk is out, and dismisses it on the first `finished` event of that transfer.
- The progress line in the incoming dialog appears 300 ms after the first bytes, so a fast transfer never flashes it.
- Closing the incoming dialog without an answer leaves the offer pending and brings its toast back. There is no list of pending offers in the Network panel.
- The incoming toast has only a Review action. Inline errors in the dialogs use `StateBlock` (`tone='error'`).
- Audit: `network_collection_sent` (outcome `completed`, `unconfirmed` or `failed`) and `network_collection_received` (outcome `completed`, `damaged` or `rejected`) rows show the collection name, the short device id and, when not completed, the outcome in brackets (wire fields `device_id`, `collection`, `outcome`). The full device id and the payload are never logged.
```

(e) In "How to test" add these paths to the vitest command: `src/lib/__tests__/network-utils.test.ts src/components/collections/__tests__ src/components/audit src/hooks/__tests__/useIncomingTransferNotice.test.tsx`, and add a sentence at the end: "Send backend tests: `cargo test -j4 -p rocket-network transfer`, `cargo test -j4 -p rocket-import transfer_zip`, `cargo test -j4 -p rocket-app network`. Real transfers: section 11 of `docs/manual-tests/team-network.md`."

- [ ] **Step 7: Manual test file**

In `docs/manual-tests/team-network.md`:

(a) In the "Contents" list add `- 11: send a collection` after `- 10: hide, block, remove`. In the intro sentence "Checks of discovery, pairing and presence on real machines." change to "Checks of discovery, pairing, presence and sending a collection on real machines."

(b) Append this section at the end of the file:

```markdown

## 11: send a collection

Needs two paired machines, A and B, ideally one Linux and one Mac, both Online. Before each check, make sure B has an active workspace you can inspect. Collection names below are examples.

1. Push, Linux to Mac. On A (Linux) create a collection `Orders` with a folder, 3 requests and one collection variable. Right-click it, pick **Send to...**, pick B, leave **Include environments** off, press Send. A toast on A says "Sent to <B>. Waiting for them to accept." B shows a toast "<A> wants to send you a collection" with the name, the size and A's short id. Press Review, check the same details in the dialog, press Accept. B shows "Receiving from <A>" and then the toast `Received "Orders" from <A>.`. Expect a new collection `Orders` in B's active workspace with the same folder, requests and variable. A shows `Sent "Orders" to <B>.`. Both Audit logs have one row ("Sent collection Orders to device ..." on A, "Received collection Orders from device ..." on B, no outcome in brackets).
2. Push, Mac to Linux. Repeat check 1 with the roles swapped.
3. Menu item. Take B offline (quit Rocket on B). Open the collection menu on A, both the `...` menu and the right-click menu. Expect no **Send to...** item. Start B again and wait for Online: the item is back in both menus.
4. Decline. Send `Orders` from A. On B press Decline. Expect "<B> declined." on A, no toast on B about its own choice, and no new collection on B.
5. Expiry. Send from A. On B open the dialog and leave it for 2 minutes. Expect the dialog to switch to "The request expired." with a Done button and no Accept, and "The request expired." on A. Repeat but leave the toast alone (do not open the dialog): the toast goes away and A shows the same message. No collection on B.
6. Close without answering. Send from A, press Review on B, close the dialog with Escape. Expect the toast to come back. Press Review again and Accept. The collection arrives.
7. Name clash. B already has a collection `Orders`. Send `Orders` from A and accept. Expect a new collection `Orders (from <A>)` on B and the old `Orders` untouched. Send it again: expect `Orders (from <A>)-1` (the importer suffix).
8. Secret and script. In `Orders` on A add a bearer token auth with a real-looking value, a variable named `api_token` with a value, and a pre-request script that logs a distinct line with `console.log`. Send with **Include environments** on, with an environment that also holds an `api_token` value. On B, before Accept, expect the dialog's warning that the scripts run in the Safe sandbox and that Developer mode, host environment and vault access stay off. Accept on B. Expect the received files (open the collection folder on B) to contain neither the token nor the `api_token` value. Open a request on B and send it: the script runs in the Safe sandbox on this first Send (the log line shows). Expect Developer mode to be off for the collection and a script that needs it (host env or vault) to be denied until you allow it in the collection's trust settings.
9. Over 10 MiB. In a collection add request bodies until the export is over 10 MiB (a few large JSON bodies). Press Send. Expect the dialog to stay open with an inline error "This collection is too big to send (X MiB, limit 10)." and no toast on B. Remove content until the ZIP is just under 10 MiB and expect the send to work.
10. Wi-Fi drops mid-transfer. Use a collection of about 8 MiB. Accept on B, and as soon as B shows the receiving line switch off Wi-Fi on B (or A). Expect "Sending failed. Try again." on A and "Receiving failed." on B, no new collection, and no leftover file: check that `~/.rocket-api/` has no staged transfer file. Switch Wi-Fi back on and wait for both nodes to be Online, then send again: it works.
11. Two offers at once. Send two different collections from A in a row. Expect two toasts on B, each opening its own dialog with its own name and size. Accept one and decline the other.
12. One send per peer. Send `Orders` from A to B and leave the offer unanswered. Send another collection from A to B. Expect A's send dialog to show the inline error "A transfer to that node is already running." and no second toast on B. (The "<B> is busy. Try again in a minute." case needs four senders; it is covered by the automated test `the_fourth_pending_offer_is_told_busy` in Plan 4.)
13. Environments with a secret variable (the keychain path). On A create an environment for `Orders` with a variable marked secret that holds a value, and a plain variable. Send with **Include environments** on and accept on B. Expect the environment on B with the plain value kept and the secret variable present but blank, and no OS keychain prompt or error on B during the import.
14. Odd names. Send a collection named `My "Quoted" API` and one with a name that has spaces and an accented letter. Expect the names to show unchanged in the toast, the dialog and the result toasts, and the received collection to have a usable name.
15. A sender without a display name. Not reachable from the UI (the name has a minimum length). The automated tests cover it.
16. Cancel. Send from A and, before B answers, press **Cancel** on A's "Waiting for them to accept." toast. Expect the toast to go away, B's toast and dialog to close, and "<A> cancelled the transfer." on B only. Then send a large collection (about 8 MiB), accept on B and watch A's toast change to "Sending to <B>...". Press Cancel on A while chunks flow: it cancels. Send it again and this time press **Cancel** in B's receiving dialog: expect "<B> cancelled the transfer." on A and no toast about it on B. Try once more and wait for the end: once the last chunk is out, A's toast has no Cancel action any more, and a normal result follows.
17. Offline during the offer. Send from A, open the dialog on B, then quit Rocket on A. Expect B's dialog to switch to "The other node went offline." with a Done button (no Accept), and the same text in a toast.
18. Notes. In `Orders` on A add a flow (or a gRPC request) and a bearer token. Send. Expect A's dialog to stay open with a list "Not sent with the collection" that names what was skipped or blanked, and a Done button. The waiting toast shows at the same time.
19. Import fails on B. Make the import fail on B (for example fill the disk quota of the workspace folder, or make the workspace folder read-only). Send from A and accept. Expect B's toast to show the importer's text (not only "Receiving failed."), A to show "Sending failed. Try again.", and no half-written collection on B.
20. Sent means imported. Send a collection and watch A: the "Sent" toast must appear only after B finished the import, not when the last chunk leaves A.
21. Unconfirmed. Send an about 8 MiB collection, accept on B, and quit Rocket on B a few seconds after A's toast lost its Cancel action (last chunk out, so B has long since acknowledged it and is importing). Expect "Sent, but the other node did not confirm." on A as an info toast, and an Audit row on A ending in "(unconfirmed)".
```

(c) Remove item 15 if the display-name minimum is not enforced: check `validateDisplayName` in `src/components/network/network-helpers.ts` before finishing. If an empty name is possible, replace item 15 with the steps to produce one.

- [ ] **Step 8: Roadmap**

In `docs/roadmaps/roadmap.md` (row R-83) replace the final cell text "Send and receive, and contract checks between nodes, get their own specs." with "Send a collection is built (spec `docs/superpowers/specs/2026-11-06-team-network-send-collection-design.md`), manual checks open in section 11 of the manual test file. Contract checks between nodes get their own spec." Keep the status cell `built, manual checks open`.

In `docs/roadmaps/contracts-ci-ai-roadmap.md` (CA-5 "What" list) replace

```
- Later, each with its own spec: sending collections, contracts and pins as
  sanitised snapshots (lands with zero trust), and contract checks between
  nodes. Git history over the hub stays a separate spike.
```

with

```
- Send a collection (built, manual checks open in
  `docs/manual-tests/team-network.md`, section 11): a one-time push of a
  collection to a paired node. The receiver accepts every offer and the
  imported collection has no trust record (its scripts run in the Safe
  sandbox; Developer mode, host env and vault stay off).
- Later, each with its own spec: sending contracts and pins as sanitised
  snapshots (lands with zero trust), and contract checks between nodes. Git
  history over the hub stays a separate spike.
```

Leave the CA-5 table row status (`built, manual checks open`) as it is.

- [ ] **Step 9: Pointer in the docs index**

In `.claude/docs-index.md` replace "(discovery, pairing, presence, Network panel, Settings section, limits, manual checks)" with "(discovery, pairing, presence, Network panel, Settings section, send a collection, limits, manual checks)". Change nothing else in that file: it already has an unrelated uncommitted edit (the `.claude/sprint-plan.md` bullet) that is not part of this plan.

Run: `git diff -- .claude/docs-index.md`
Expected: two hunks or one hunk with two changes: this plan's "send a collection" line and the unrelated sprint-plan bullet. When the user commits after the plan review, stage only this plan's hunk with `git add -p .claude/docs-index.md` (answer `y` for the "send a collection" hunk, `n` for the sprint-plan hunk; if both changes sit in one hunk, split it with `s` or edit it with `e`), then check `git diff --cached -- .claude/docs-index.md` shows only the "send a collection" change. This plan itself stages nothing.

- [ ] **Step 10: Whole verification and stop**

No commit; reviewed at the plan end.

Run the whole frontend verification for this plan:

```bash
bun run tsc --noEmit && bun run check && bunx vitest run \
  src/lib/__tests__/tauri-api.network.test.ts src/lib/__tests__/network-utils.test.ts \
  src/lib/__tests__/network-event-bridge.test.ts src/stores/__tests__/network-store.test.ts \
  src/components/network src/components/collections/__tests__ src/components/audit \
  src/hooks/__tests__/useIncomingTransferNotice.test.tsx \
  src/hooks/__tests__/useIncomingPairingNotice.test.tsx
```

Expected: `tsc` and `check` print no findings (if `check` reports format or import order, run `bun run lint` and `bun run check` again), and every vitest file passes.

**Next plan to run: none. Stop and show the user the whole change.**

---

## Self-review

Spec sections covered:

- **Section 5 (flow):** Send to... in both menus only with an online paired node (Task 2); the dialog with the online list and Include environments (Task 2); the receiver toast and dialog with sender, short id, name, size, Accept and Decline (Task 1); the doc and the manual checks describe all six steps (Task 3).
- **Section 9 (frontend components):** `transfers` slice and the `network-transfer` handler in `useNetworkEventBridge` with the same listener scope (Task 1); `SendCollectionDialog` with `FormField` and `EmptyState` (Task 2); incoming toast and `IncomingTransferDialog` with progress and the expired view (Task 1); no new Settings section.
- **Section 10 (errors):** every row has a toast text in `describeTransferOutcome` and a table test; the "Peer offline" row is the hidden node in the dialog and the hidden menu item; the import-fails row shows the importer's `message` to the receiver (ruling 8) and "Sending failed. Try again." to the sender.
- **Section 11 (frontend tests and manual):** store slice, bridge handler, send dialog (offline hidden, environments off), incoming dialog and its expiry while open, duplicate `offerIncoming` (one row, one toast), late response after the transfer ended, and the manual section 11 with push both ways, decline, expiry, name clash, secret and script, over 10 MiB, dropped Wi-Fi.
- **Section 12 (open items):** the menu entry lives in the collection menu only (no node menu entry); the progress line hides below 300 ms.
- **Section 5 and 6 (result and cancel):** the sender's "Sent" toast comes from the finished event that the backend sends after the receiver's result frame, so the frontend needs no extra state (ruling 7); the waiting toast has a Cancel action and is dismissed by the bridge (ruling 9, Task 2 and Task 1 tests); the send notes show in the dialog (ruling 10).
- **Section 8 (audit):** the two audit kinds with wire names (`device_id`, `collection`, `outcome`) in the TS union and the rows, with full-id, short-id, odd-name, unconfirmed, rejected and damaged tests (Task 3).
- **Second-review rulings (index):** R1 script warning in the dialog, feature doc and manual check 8 (rulings 12); R4 `unconfirmed` toast (ruling 11); R6 audit outcome (ruling 16); R11 manual check 12 rewritten, busy left to Plan 4; R12 manual check 13 (keychain path); R18 docs-index hunk staged with `git add -p` (ruling 18), version 2 note in the feature doc, design-system page step removed (ruling 17), receive-side `too_big` text removed; R19 receiving Cancel, who-cancelled memory, offline text in the dialog, waiting toast phases (rulings 7, 8, 13, 14); R20 reload accepted (ruling 15).
- **Design system map:** `FormField` group mode, `EmptyState`, `StateBlock`, dialog primitives, text sizes, no raw elements. The Badge success, avatar, SearchInput and StatusBarItem entries are added (Task 3 Step 10). A dedicated section for avatar and search input is a follow-up because it needs renumbering.
