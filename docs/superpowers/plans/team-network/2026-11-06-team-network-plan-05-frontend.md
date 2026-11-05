# Team Network — Plan 5: Frontend, docs and manual checks

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 5 of 5.** Previous plan: `docs/superpowers/plans/team-network/2026-11-06-team-network-plan-04-service-tauri.md` (must be merged and green).
**Next plan to run: none. This is the last plan of the series. Stop and show the user the whole change.**
**Recommended model: Sonnet.**

**Goal:** A Rocket user sees how many paired teammates are online in the status bar, opens a Network panel with a search box, pairs with a nearby node by comparing a 6-digit code, removes a pairing, adds a node by address, and manages visibility, name, blocked devices and saved addresses in Settings. A first-launch card says who the user is visible as. The feature has a developer doc and a manual test file.

**Architecture:** The backend (Plans 1 to 4) owns all network state. The frontend keeps a thin mirror in `network-store` (status, node list, open incoming requests, the pairing the user started, and two UI flags). The mirror is refilled by calling the IPC commands, never patched from event fields: `nodesChanged` and `statusChanged` trigger a refetch, and a sequence counter drops a stale response. Events about pairings (`pairingIncoming`, `pairingFinished`) feed `incoming` and `activePairing` directly because they carry data the commands do not return. One bridge hook (`useNetworkEventBridge`) subscribes once, like the assistant bridge. Components read the store with narrow selectors. Dialogs that must outlive the panel (Add by address, Remove, Pairing) are mounted outside the Popover, so closing the Popover never destroys them.

**Tech Stack:** React 18, TypeScript, Zustand, shadcn/ui, lucide-react, sonner, Vitest + Testing Library, Bun.

## Spec

`docs/superpowers/specs/2026-11-05-team-network-discovery-pairing-design.md`, sections 3a (many peers), 5.5 (frontend), 7.5 (panel), 8.4 (first launch), 9 (error handling copy), 10 (tests, manual checks). Binding names and DTO fields: `docs/superpowers/plans/team-network/00-plan-index.md` ("Tauri commands and TS wrappers", "Event channels").

## Global Constraints

- shadcn/ui primitives only. No raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`. The initials circle is a styled `<span>` because `src/components/ui/` has no avatar. Enter-to-submit is an `onKeyDown` on the `Input`, not a `<form>`.
- Icons from `lucide-react` only. No inline SVG.
- Single-line variable-aware fields use `SingleLineEditor`; these inputs are not variable-aware, so plain `Input` is correct. No Monaco or CodeMirror is added.
- Zustand: narrow selectors only. Never destructure the whole store at the top of a component. Selectors return primitives or stable references (`find` on an array returns an element that is already in the store).
- Bun only: `bun run tsc --noEmit`, `bun run check`, `bun run lint` (auto-fix), `bunx vitest run <path>`. No Yarn or npm.
- Event payload fields are snake_case as sent (`pairing_id`, `by_peer`). IPC DTO fields are camelCase. Invoke arguments are camelCase.
- Code comments are short full sentences that end with a period.
- No commit during the plan. The user reviews the whole plan at the end. Then commit through `dev-workflow-skills:1-git-commit`, staging explicit paths only, conventional subjects (`feat(network): ...`, `test(network): ...`, `docs(network): ...`).
- Never log or show the full device id, a nonce or a key. The UI shows the short id (`K7QF-2M9X`) and the 6-digit code only.
- Verification per task: `bun run tsc --noEmit`, `bun run check` (if it reports format or import-order findings, run `bun run lint`, then `bun run check` again), and the task's `bunx vitest run <path>`.

## Review Focus

1. **Duplicate `pairingIncoming` event** (Task 1 store and bridge tests, Task 3 notice test): one row, one toast, and the 120 s expiry is not extended by the second event.
2. **Search with no matches** (Task 2 panel test): shows `No nodes match "<query>".`, keeps the open incoming requests visible, and never shows the "No Rocket nodes found" empty state, which is only for an empty list.
3. **Two nodes with the same display name** (Task 2 panel test): each row shows its own short id and has its own Pair button named with the short id. Search by short id (with or without the dash) finds exactly one.
4. **Incoming request expires while its dialog is open** (Task 1 store test for the timer, Task 3 dialog test): the dialog switches to "The request expired." with a Close button and no Accept button, instead of staying open on a dead code.
5. **Listener event after unmount** (Task 1 bridge test): an event delivered after unmount changes nothing, the unlisten functions are called, and a listener that registers after unmount is unlistened at once.
6. **Out-of-order refetch** (Task 1 store test): two `listNetworkNodes` calls that resolve in reverse order leave the newer result in the store.
7. **Decision response races a finish event** (Task 3 dialog test): if `PairingFinished` clears the pairing while `respondNetworkPairing` is still in flight, the late response must not bring the pairing back.
8. **Who gets the "declined" toast** (Task 1 bridge test): only the node that started the pairing. The user who pressed Decline or Block gets no "<name> declined." toast about their own choice.
9. **Dialogs outside the Popover** (Task 2 status item test): choosing Remove or Add by address closes the panel, and the dialog still opens and works.
10. **Port and name boundaries** (Task 2 and Task 3 helper tests): port 0, 65536, `80a` and an empty port are refused; 1 and 65535 pass. A 40-character name passes, 41 fails, and an emoji counts as one character (the backend counts `chars()`).

---

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `src/lib/tauri-api.ts` | modify | "Team network" section: types, 14 wrappers, 2 listener wrappers |
| `src/lib/network-utils.ts` | create | `errorText`, `formatPairingCode` |
| `src/stores/network-store.ts` | create | Mirror of backend state, incoming expiry, `startNetworkPairing`, `selectOnlinePairedCount` |
| `src/lib/network-event-bridge.ts` | create | `useNetworkEventBridge`, `handleNetworkChanged`, `handleNetworkPairing` |
| `src/test/network-fixtures.ts` | create | `makeStatus`, `makeNode` |
| `src/App.tsx` | modify | Mount the bridge, notice hook, `PairingDialog`, `NetworkIntroCard` |
| `src/components/network/network-helpers.ts` | create | `filterNodes`, `sortNodes`, `groupNodes`, `validateAddress`, `validateDisplayName` |
| `src/components/network/NodeRow.tsx` | create | One node row: initials, name, short id, chip, Pair or menu |
| `src/components/network/NetworkPanel.tsx` | create | Panel body: search, requests, groups, empty and error states |
| `src/components/network/AddAddressDialog.tsx` | create | Host and port form |
| `src/components/network/RemovePairedDialog.tsx` | create | Remove confirmation |
| `src/components/status-bar/NetworkStatusItem.tsx` | create | Status bar button, Popover, hosts the dialogs |
| `src/components/layout/StatusBar.tsx` | modify | Render `NetworkStatusItem` |
| `src/components/network/PairingDialog.tsx` | create | Code, Accept, Cancel, Decline, Block, waiting and expired views |
| `src/components/network/IncomingPairingToast.tsx` | create | Body of the incoming toast |
| `src/hooks/useIncomingPairingNotice.tsx` | create | One toast per open request, fixed id |
| `src/components/network/NetworkIntroCard.tsx` | create | One-time visibility card |
| `src/components/settings/sections/NetworkSection.tsx` | create | Settings section |
| `src/stores/settings-store.ts` | modify | `'network'` in `SettingsSection` |
| `src/components/settings/settings-sections.ts` | modify | Row for `network` |
| `src/components/settings/__tests__/SettingsPage.test.tsx` | modify | Section list gains "Team network" |
| `docs/features/team-network.md` | create | Developer doc |
| `docs/manual-tests/team-network.md` | create | Manual checks |
| `docs/features/settings-page.md` | modify | Section list |
| `CLAUDE.md` | modify | One-line pointer |
| `docs/roadmaps/roadmap.md`, `docs/roadmaps/contracts-ci-ai-roadmap.md` | modify | R-83 and CA-5 status |
| Tests | create | `src/lib/__tests__/tauri-api.network.test.ts`, `src/lib/__tests__/network-utils.test.ts`, `src/stores/__tests__/network-store.test.ts`, `src/lib/__tests__/network-event-bridge.test.ts`, and the component, hook and section tests named in each task |

---

### Task 1: Types, wrappers, store and event bridge

**Files:**
- Modify: `src/lib/tauri-api.ts`, `src/App.tsx`
- Create: `src/lib/network-utils.ts`, `src/stores/network-store.ts`, `src/lib/network-event-bridge.ts`, `src/test/network-fixtures.ts`
- Test: `src/lib/__tests__/tauri-api.network.test.ts`, `src/lib/__tests__/network-utils.test.ts`, `src/stores/__tests__/network-store.test.ts`, `src/lib/__tests__/network-event-bridge.test.ts`

**Interfaces:**
- Consumes: Tauri commands and event channels from the plan index (Plan 4 delivered them).
- Produces (used by Tasks 2 and 3):

```ts
// src/lib/tauri-api.ts
export interface NetworkStatus {
  state: 'running' | 'off'; offReason: string | null; port: number | null;
  listenerAvailable: boolean; discoveryAvailable: boolean; hidden: boolean;
  displayName: string; initials: string; deviceShortId: string | null;
  introSeen: boolean; warnings: string[];
}
export type NetworkNodeGroup = 'paired' | 'nearby';
export type NetworkNodeStatus = 'online' | 'offline' | 'pairing' | 'needs_update' | 'available';
export interface NetworkNode {
  deviceId: string; shortId: string; displayName: string; initials: string;
  group: NetworkNodeGroup; status: NetworkNodeStatus; source: 'mdns' | 'manual' | null;
}
export interface NetworkAddress { host: string; port: number; addedAt: string }
export interface NetworkBlockedDevice { deviceId: string; shortId: string; name: string; blockedAt: string }
export interface NetworkPairingStarted { pairingId: string; code: string }
export type NetworkPairingDecision = 'accept' | 'decline' | 'block';
export type NetworkChangedEvent =
  | { type: 'nodesChanged' } | { type: 'statusChanged' }
  | { type: 'pairedRemoved'; device_id: string; display_name: string; by_peer: boolean };
export type NetworkPairingEvent =
  | { type: 'pairingIncoming'; pairing_id: string; device_id: string; short_id: string; display_name: string; code: string }
  | { type: 'pairingFinished'; pairing_id: string; outcome: string; display_name: string };
export const getNetworkStatus: () => Promise<NetworkStatus>;
export const listNetworkNodes: () => Promise<NetworkNode[]>;
export const setNetworkVisibility: (hidden: boolean) => Promise<void>;
export const setNetworkProfile: (displayName: string) => Promise<void>;
export const markNetworkIntroSeen: () => Promise<void>;
export const listNetworkAddresses: () => Promise<NetworkAddress[]>;
export const addNetworkAddress: (host: string, port: number) => Promise<void>;
export const removeNetworkAddress: (host: string, port: number) => Promise<void>;
export const requestNetworkPairing: (deviceId: string) => Promise<NetworkPairingStarted>;
export const respondNetworkPairing: (pairingId: string, decision: NetworkPairingDecision) => Promise<void>;
export const cancelNetworkPairing: (pairingId: string) => Promise<void>;
export const removeNetworkPaired: (deviceId: string) => Promise<void>;
export const listNetworkBlocked: () => Promise<NetworkBlockedDevice[]>;
export const unblockNetworkDevice: (deviceId: string) => Promise<void>;
export const onNetworkChanged: (h: (e: NetworkChangedEvent) => void) => Promise<UnlistenFn>;
export const onNetworkPairing: (h: (e: NetworkPairingEvent) => void) => Promise<UnlistenFn>;

// src/lib/network-utils.ts
export function errorText(error: unknown): string;          // strips "Invalid input: " and "Internal error: "
export function formatPairingCode(code: string): string;    // "482913" -> "482 913"

// src/stores/network-store.ts
export const INCOMING_TTL_MS = 120_000;
export interface PairingIncoming {
  pairingId: string; deviceId: string; shortId: string; displayName: string; code: string;
  accepted: boolean;                       // The local user pressed Accept and waits for the peer.
}
export interface ActivePairing {
  pairingId: string; deviceId: string; shortId: string; displayName: string; code: string;
  phase: 'waiting' | 'done';               // 'waiting': no local decision yet. 'done': Accept was sent.
}
export interface NetworkState {
  status: NetworkStatus | null; nodes: NetworkNode[]; incoming: PairingIncoming[];
  activePairing: ActivePairing | null; panelOpen: boolean; dialogPairingId: string | null;
  error: string | null;
  refresh(): Promise<void>; refreshNodes(): Promise<void>;
  addIncoming(i: PairingIncoming): void; removeIncoming(pairingId: string): void;
  markIncomingAccepted(pairingId: string): void;
  setActivePairing(p: ActivePairing | null): void;
  setPanelOpen(open: boolean): void; openPairingDialog(pairingId: string): void; closePairingDialog(): void;
  reset(): void;
}
export const useNetworkStore: UseBoundStore<StoreApi<NetworkState>>;
export const selectOnlinePairedCount: (s: NetworkState) => number;
export function startNetworkPairing(node: NetworkNode): Promise<void>;   // Rejects with the backend error.

// src/lib/network-event-bridge.ts
export function handleNetworkChanged(event: NetworkChangedEvent): void;
export function handleNetworkPairing(event: NetworkPairingEvent): void;
export function useNetworkEventBridge(): void;

// src/test/network-fixtures.ts
export function makeStatus(patch?: Partial<NetworkStatus>): NetworkStatus;
export function makeNode(patch?: Partial<NetworkNode>): NetworkNode;
```

- [ ] **Step 1: Write the failing wrapper tests**

Create `src/lib/__tests__/tauri-api.network.test.ts`:

```ts
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  addNetworkAddress,
  cancelNetworkPairing,
  getNetworkStatus,
  listNetworkAddresses,
  listNetworkBlocked,
  listNetworkNodes,
  markNetworkIntroSeen,
  onNetworkChanged,
  onNetworkPairing,
  removeNetworkAddress,
  removeNetworkPaired,
  requestNetworkPairing,
  respondNetworkPairing,
  setNetworkProfile,
  setNetworkVisibility,
  unblockNetworkDevice,
} from '../tauri-api';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

describe('team network commands', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(listen).mockReset();
  });

  it('calls the commands that take no arguments', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await getNetworkStatus();
    expect(invoke).toHaveBeenLastCalledWith('network_status');
    await listNetworkNodes();
    expect(invoke).toHaveBeenLastCalledWith('network_list_nodes');
    await markNetworkIntroSeen();
    expect(invoke).toHaveBeenLastCalledWith('network_mark_intro_seen');
    await listNetworkAddresses();
    expect(invoke).toHaveBeenLastCalledWith('network_list_addresses');
    await listNetworkBlocked();
    expect(invoke).toHaveBeenLastCalledWith('network_list_blocked');
  });

  it('sends camelCase arguments', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await setNetworkVisibility(true);
    expect(invoke).toHaveBeenLastCalledWith('network_set_visibility', { hidden: true });
    await setNetworkProfile('Ana Lima');
    expect(invoke).toHaveBeenLastCalledWith('network_set_profile', { displayName: 'Ana Lima' });
    await addNetworkAddress('10.0.0.5', 47821);
    expect(invoke).toHaveBeenLastCalledWith('network_add_address', {
      host: '10.0.0.5',
      port: 47821,
    });
    await removeNetworkAddress('10.0.0.5', 47821);
    expect(invoke).toHaveBeenLastCalledWith('network_remove_address', {
      host: '10.0.0.5',
      port: 47821,
    });
    await requestNetworkPairing('DEV1');
    expect(invoke).toHaveBeenLastCalledWith('network_request_pairing', { deviceId: 'DEV1' });
    await respondNetworkPairing('p1', 'block');
    expect(invoke).toHaveBeenLastCalledWith('network_respond_pairing', {
      pairingId: 'p1',
      decision: 'block',
    });
    await cancelNetworkPairing('p1');
    expect(invoke).toHaveBeenLastCalledWith('network_cancel_pairing', { pairingId: 'p1' });
    await removeNetworkPaired('DEV1');
    expect(invoke).toHaveBeenLastCalledWith('network_remove_paired', { deviceId: 'DEV1' });
    await unblockNetworkDevice('DEV1');
    expect(invoke).toHaveBeenLastCalledWith('network_unblock', { deviceId: 'DEV1' });
  });

  it('returns what the command returns', async () => {
    vi.mocked(invoke).mockResolvedValue({ pairingId: 'p1', code: '482913' });
    await expect(requestNetworkPairing('DEV1')).resolves.toEqual({
      pairingId: 'p1',
      code: '482913',
    });
  });

  it('passes a rejection through unchanged', async () => {
    vi.mocked(invoke).mockRejectedValue('Invalid input: Port is taken');
    await expect(addNetworkAddress('h', 1)).rejects.toBe('Invalid input: Port is taken');
  });

  it('listens on the two channels and hands over the payload', async () => {
    vi.mocked(listen).mockResolvedValue(() => undefined);
    const onChanged = vi.fn();
    const onPairing = vi.fn();
    await onNetworkChanged(onChanged);
    await onNetworkPairing(onPairing);
    expect(listen).toHaveBeenNthCalledWith(1, 'network-changed', expect.any(Function));
    expect(listen).toHaveBeenNthCalledWith(2, 'network-pairing', expect.any(Function));

    const [, changedCallback] = vi.mocked(listen).mock.calls[0] ?? [];
    const [, pairingCallback] = vi.mocked(listen).mock.calls[1] ?? [];
    if (!changedCallback || !pairingCallback) throw new Error('listeners were not registered');
    const deliver = (cb: unknown, payload: unknown) =>
      (cb as (event: { payload: unknown }) => void)({ payload });
    deliver(changedCallback, { type: 'nodesChanged' });
    deliver(pairingCallback, { type: 'pairingFinished', pairing_id: 'p1' });
    expect(onChanged).toHaveBeenCalledWith({ type: 'nodesChanged' });
    expect(onPairing).toHaveBeenCalledWith({ type: 'pairingFinished', pairing_id: 'p1' });
  });
});
```

- [ ] **Step 2: Write the failing util and fixture tests**

Create `src/lib/__tests__/network-utils.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { errorText, formatPairingCode } from '@/lib/network-utils';

describe('errorText', () => {
  it('drops the backend error type prefix', () => {
    expect(errorText(new Error('Invalid input: Port is taken'))).toBe('Port is taken');
    expect(errorText('Internal error: boom')).toBe('boom');
  });

  it('keeps other messages as they are', () => {
    expect(errorText('Could not reach 10.0.0.5')).toBe('Could not reach 10.0.0.5');
  });

  it('falls back to a generic line when there is no text', () => {
    expect(errorText(42)).toBe('Something went wrong. Try again.');
    expect(errorText('')).toBe('Something went wrong. Try again.');
  });
});

describe('formatPairingCode', () => {
  it('splits six digits into two groups of three', () => {
    expect(formatPairingCode('482913')).toBe('482 913');
  });

  it('keeps leading zeros', () => {
    expect(formatPairingCode('048213')).toBe('048 213');
  });

  it('returns anything that is not six digits unchanged', () => {
    expect(formatPairingCode('48291')).toBe('48291');
    expect(formatPairingCode('abc')).toBe('abc');
  });
});
```

- [ ] **Step 3: Write the failing store tests**

Create `src/stores/__tests__/network-store.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { NetworkNode } from '@/lib/tauri-api';
import {
  INCOMING_TTL_MS,
  type PairingIncoming,
  selectOnlinePairedCount,
  startNetworkPairing,
  useNetworkStore,
} from '@/stores/network-store';
import { createDeferred } from '@/test/deferred';
import { makeNode, makeStatus } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  api: {
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
    requestNetworkPairing: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);

const store = () => useNetworkStore.getState();

const incoming = (pairingId = 'p1'): PairingIncoming => ({
  pairingId,
  deviceId: 'DEV9',
  shortId: 'ZZZZ-9999',
  displayName: 'Cleo Park',
  code: '482913',
  accepted: false,
});

describe('network-store', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('refresh stores the status and clears an old error', async () => {
    useNetworkStore.setState({ error: 'old' });
    mocks.api.getNetworkStatus.mockResolvedValue(makeStatus({ displayName: 'Ana' }));
    await store().refresh();
    expect(store().status?.displayName).toBe('Ana');
    expect(store().error).toBeNull();
  });

  it('a failed refresh keeps the last status and records the error text', async () => {
    useNetworkStore.setState({ status: makeStatus({ displayName: 'Kept' }) });
    mocks.api.getNetworkStatus.mockRejectedValue('Internal error: backend down');
    await store().refresh();
    expect(store().status?.displayName).toBe('Kept');
    expect(store().error).toBe('backend down');
  });

  it('refreshNodes stores the node list', async () => {
    mocks.api.listNetworkNodes.mockResolvedValue([makeNode({ deviceId: 'A' })]);
    await store().refreshNodes();
    expect(store().nodes.map((n) => n.deviceId)).toEqual(['A']);
  });

  it('keeps the newest node list when responses arrive out of order', async () => {
    const first = createDeferred<NetworkNode[]>();
    const second = createDeferred<NetworkNode[]>();
    mocks.api.listNetworkNodes
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const a = store().refreshNodes();
    const b = store().refreshNodes();
    second.resolve([makeNode({ deviceId: 'NEW' })]);
    await b;
    first.resolve([makeNode({ deviceId: 'OLD' })]);
    await a;
    expect(store().nodes.map((n) => n.deviceId)).toEqual(['NEW']);
  });

  it('addIncoming dedupes by pairing id', () => {
    store().addIncoming(incoming('p1'));
    store().addIncoming(incoming('p1'));
    store().addIncoming(incoming('p2'));
    expect(store().incoming.map((i) => i.pairingId)).toEqual(['p1', 'p2']);
  });

  it('removes an incoming request after the expiry time', () => {
    vi.useFakeTimers();
    store().addIncoming(incoming());
    vi.advanceTimersByTime(INCOMING_TTL_MS - 1);
    expect(store().incoming).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(store().incoming).toHaveLength(0);
  });

  it('a duplicate event does not extend the expiry', () => {
    vi.useFakeTimers();
    store().addIncoming(incoming());
    vi.advanceTimersByTime(60_000);
    store().addIncoming(incoming());
    expect(store().incoming).toHaveLength(1);
    vi.advanceTimersByTime(INCOMING_TTL_MS - 60_000);
    expect(store().incoming).toHaveLength(0);
  });

  it('removeIncoming cancels the expiry timer', () => {
    vi.useFakeTimers();
    store().addIncoming(incoming());
    expect(vi.getTimerCount()).toBe(1);
    store().removeIncoming('p1');
    expect(vi.getTimerCount()).toBe(0);
    expect(store().incoming).toHaveLength(0);
  });

  it('markIncomingAccepted flags one request and ignores an unknown id', () => {
    store().addIncoming(incoming('p1'));
    store().addIncoming(incoming('p2'));
    store().markIncomingAccepted('p2');
    store().markIncomingAccepted('missing');
    expect(store().incoming.map((i) => i.accepted)).toEqual([false, true]);
  });

  it('reset clears everything, including pending timers', () => {
    vi.useFakeTimers();
    store().addIncoming(incoming());
    useNetworkStore.setState({
      status: makeStatus(),
      nodes: [makeNode()],
      panelOpen: true,
      dialogPairingId: 'p1',
    });
    store().reset();
    expect(store().status).toBeNull();
    expect(store().nodes).toEqual([]);
    expect(store().incoming).toEqual([]);
    expect(store().activePairing).toBeNull();
    expect(store().panelOpen).toBe(false);
    expect(store().dialogPairingId).toBeNull();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('selectOnlinePairedCount counts only online paired nodes', () => {
    useNetworkStore.setState({
      nodes: [
        makeNode({ deviceId: '1', group: 'paired', status: 'online' }),
        makeNode({ deviceId: '2', group: 'paired', status: 'offline' }),
        makeNode({ deviceId: '3', group: 'nearby', status: 'available' }),
        makeNode({ deviceId: '4', group: 'paired', status: 'online' }),
      ],
    });
    expect(selectOnlinePairedCount(store())).toBe(2);
  });

  describe('startNetworkPairing', () => {
    it('keeps the code, opens the dialog and closes the panel', async () => {
      useNetworkStore.setState({ panelOpen: true });
      mocks.api.requestNetworkPairing.mockResolvedValue({ pairingId: 'p1', code: '482913' });
      await startNetworkPairing(makeNode({ deviceId: 'DEV1', shortId: 'AAAA-1111' }));
      expect(mocks.api.requestNetworkPairing).toHaveBeenCalledWith('DEV1');
      expect(store().activePairing).toEqual({
        pairingId: 'p1',
        deviceId: 'DEV1',
        shortId: 'AAAA-1111',
        displayName: 'Ben Ortiz',
        code: '482913',
        phase: 'waiting',
      });
      expect(store().dialogPairingId).toBe('p1');
      expect(store().panelOpen).toBe(false);
    });

    it('leaves the state alone and rethrows when the request fails', async () => {
      useNetworkStore.setState({ panelOpen: true });
      mocks.api.requestNetworkPairing.mockRejectedValue('Invalid input: busy');
      await expect(startNetworkPairing(makeNode())).rejects.toBe('Invalid input: busy');
      expect(store().activePairing).toBeNull();
      expect(store().dialogPairingId).toBeNull();
      expect(store().panelOpen).toBe(true);
    });
  });
});
```

- [ ] **Step 4: Write the failing bridge tests**

Create `src/lib/__tests__/network-event-bridge.test.ts`:

```ts
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useNetworkEventBridge } from '@/lib/network-event-bridge';
import { useNetworkStore } from '@/stores/network-store';
import { createDeferred } from '@/test/deferred';
import { makeNode, makeStatus } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => {
  const handlers: Record<string, (payload: unknown) => void> = {};
  const unlisten = vi.fn();
  const listener = (name: string) =>
    vi.fn((handler: (payload: unknown) => void) => {
      handlers[name] = handler;
      return Promise.resolve<() => void>(unlisten);
    });
  return {
    handlers,
    unlisten,
    toast: { info: vi.fn(), success: vi.fn(), error: vi.fn() },
    api: {
      onNetworkChanged: listener('changed'),
      onNetworkPairing: listener('pairing'),
      getNetworkStatus: vi.fn(),
      listNetworkNodes: vi.fn(),
      requestNetworkPairing: vi.fn(),
    },
  };
});

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const store = () => useNetworkStore.getState();

async function mountBridge() {
  const view = renderHook(() => useNetworkEventBridge());
  await waitFor(() => expect(store().status).not.toBeNull());
  return view;
}

function emit(name: 'changed' | 'pairing', payload: unknown): void {
  const handler = mocks.handlers[name];
  if (!handler) throw new Error(`no ${name} listener`);
  handler(payload);
}

const incomingEvent = (id = 'p1') => ({
  type: 'pairingIncoming',
  pairing_id: id,
  device_id: 'DEV9',
  short_id: 'ZZZZ-9999',
  display_name: 'Dana Roy',
  code: '482913',
});

const finished = (outcome: string, id = 'p1') => ({
  type: 'pairingFinished',
  pairing_id: id,
  outcome,
  display_name: 'Dana Roy',
});

describe('useNetworkEventBridge', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    for (const key of Object.keys(mocks.handlers)) delete mocks.handlers[key];
    store().reset();
    mocks.api.getNetworkStatus.mockResolvedValue(makeStatus());
    mocks.api.listNetworkNodes.mockResolvedValue([makeNode()]);
  });

  it('loads the status and the nodes once on mount', async () => {
    await mountBridge();
    expect(mocks.api.getNetworkStatus).toHaveBeenCalledTimes(1);
    expect(mocks.api.listNetworkNodes).toHaveBeenCalledTimes(1);
    expect(mocks.api.onNetworkChanged).toHaveBeenCalledTimes(1);
    expect(mocks.api.onNetworkPairing).toHaveBeenCalledTimes(1);
  });

  it('nodesChanged reloads the nodes', async () => {
    await mountBridge();
    emit('changed', { type: 'nodesChanged' });
    await waitFor(() => expect(mocks.api.listNetworkNodes).toHaveBeenCalledTimes(2));
    expect(mocks.api.getNetworkStatus).toHaveBeenCalledTimes(1);
  });

  it('statusChanged reloads the status', async () => {
    await mountBridge();
    emit('changed', { type: 'statusChanged' });
    await waitFor(() => expect(mocks.api.getNetworkStatus).toHaveBeenCalledTimes(2));
  });

  it('pairedRemoved reloads the nodes and tells the user when the peer removed it', async () => {
    await mountBridge();
    emit('changed', {
      type: 'pairedRemoved',
      device_id: 'DEV9',
      display_name: 'Dana Roy',
      by_peer: true,
    });
    await waitFor(() => expect(mocks.api.listNetworkNodes).toHaveBeenCalledTimes(2));
    expect(mocks.toast.info).toHaveBeenCalledWith('Dana Roy removed the pairing.');
  });

  it('pairedRemoved from this node reloads quietly', async () => {
    await mountBridge();
    emit('changed', {
      type: 'pairedRemoved',
      device_id: 'DEV9',
      display_name: 'Dana Roy',
      by_peer: false,
    });
    await waitFor(() => expect(mocks.api.listNetworkNodes).toHaveBeenCalledTimes(2));
    expect(mocks.toast.info).not.toHaveBeenCalled();
  });

  it('pairingIncoming adds one request, even when the event repeats', async () => {
    await mountBridge();
    emit('pairing', incomingEvent());
    emit('pairing', incomingEvent());
    expect(store().incoming).toEqual([
      {
        pairingId: 'p1',
        deviceId: 'DEV9',
        shortId: 'ZZZZ-9999',
        displayName: 'Dana Roy',
        code: '482913',
        accepted: false,
      },
    ]);
  });

  it.each([
    ['accepted', 'success', 'Paired with Dana Roy.'],
    ['expired', 'info', 'The request expired.'],
    ['failed', 'error', 'Pairing failed. Try again.'],
  ] as const)('announces the %s outcome', async (outcome, method, message) => {
    await mountBridge();
    emit('pairing', finished(outcome));
    expect(mocks.toast[method]).toHaveBeenCalledWith(message);
  });

  it('accepted also reloads the nodes', async () => {
    await mountBridge();
    emit('pairing', finished('accepted'));
    await waitFor(() => expect(mocks.api.listNetworkNodes).toHaveBeenCalledTimes(2));
  });

  it('says nothing for a cancelled or unknown outcome', async () => {
    await mountBridge();
    emit('pairing', finished('cancelled'));
    emit('pairing', finished('something-new'));
    expect(mocks.toast.info).not.toHaveBeenCalled();
    expect(mocks.toast.success).not.toHaveBeenCalled();
    expect(mocks.toast.error).not.toHaveBeenCalled();
  });

  it('tells the starter of a pairing that the peer declined', async () => {
    await mountBridge();
    useNetworkStore.setState({
      activePairing: {
        pairingId: 'p1',
        deviceId: 'DEV9',
        shortId: 'ZZZZ-9999',
        displayName: 'Dana Roy',
        code: '482913',
        phase: 'done',
      },
    });
    emit('pairing', finished('declined'));
    expect(mocks.toast.info).toHaveBeenCalledWith('Dana Roy declined.');
    expect(store().activePairing).toBeNull();
  });

  it('does not tell a user about their own decline', async () => {
    await mountBridge();
    emit('pairing', incomingEvent());
    emit('pairing', finished('declined'));
    expect(mocks.toast.info).not.toHaveBeenCalled();
    expect(store().incoming).toEqual([]);
  });

  it('a finished pairing closes its dialog but not another one', async () => {
    await mountBridge();
    useNetworkStore.setState({ dialogPairingId: 'p1' });
    emit('pairing', finished('expired', 'other'));
    expect(store().dialogPairingId).toBe('p1');
    emit('pairing', finished('expired', 'p1'));
    expect(store().dialogPairingId).toBeNull();
  });

  it('ignores events after unmount and stops listening', async () => {
    const view = await mountBridge();
    view.unmount();
    await waitFor(() => expect(mocks.unlisten).toHaveBeenCalledTimes(2));
    emit('changed', { type: 'nodesChanged' });
    emit('pairing', incomingEvent());
    expect(mocks.api.listNetworkNodes).toHaveBeenCalledTimes(1);
    expect(store().incoming).toEqual([]);
  });

  it('stops a listener that finishes registering after unmount', async () => {
    const late = createDeferred<() => void>();
    const lateUnlisten = vi.fn();
    mocks.api.onNetworkChanged.mockImplementationOnce(() => late.promise);
    const view = renderHook(() => useNetworkEventBridge());
    view.unmount();
    late.resolve(lateUnlisten);
    await waitFor(() => expect(lateUnlisten).toHaveBeenCalledTimes(1));
  });
});
```

- [ ] **Step 5: Run the tests to verify they fail**

Run: `bunx vitest run src/lib/__tests__/tauri-api.network.test.ts src/lib/__tests__/network-utils.test.ts src/stores/__tests__/network-store.test.ts src/lib/__tests__/network-event-bridge.test.ts`
Expected: FAIL. The wrapper test reports `getNetworkStatus is not a function`; the other three report `Failed to resolve import "@/lib/network-utils"`, `"@/stores/network-store"` and `"@/lib/network-event-bridge"` (and `"@/test/network-fixtures"`).

- [ ] **Step 6: Add the fixtures**

Create `src/test/network-fixtures.ts`:

```ts
import type { NetworkNode, NetworkStatus } from '@/lib/tauri-api';

/** A running status with discovery and the listener available. */
export function makeStatus(patch: Partial<NetworkStatus> = {}): NetworkStatus {
  return {
    state: 'running',
    offReason: null,
    port: 47821,
    listenerAvailable: true,
    discoveryAvailable: true,
    hidden: false,
    displayName: 'Ana Lima',
    initials: 'AL',
    deviceShortId: 'K7QF-2M9X',
    introSeen: true,
    warnings: [],
    ...patch,
  };
}

/** A nearby node that can be paired. */
export function makeNode(patch: Partial<NetworkNode> = {}): NetworkNode {
  return {
    deviceId: 'DEV1',
    shortId: 'AAAA-1111',
    displayName: 'Ben Ortiz',
    initials: 'BO',
    group: 'nearby',
    status: 'available',
    source: 'mdns',
    ...patch,
  };
}
```

- [ ] **Step 7: Add the types, wrappers and listeners to `src/lib/tauri-api.ts`**

Append at the end of the file, after the last export (`onUpdateProgress`):

```ts

// ============================================================
// Team network
// ============================================================

export interface NetworkStatus {
  state: 'running' | 'off';
  /** Why networking is off, for example an unavailable keychain. */
  offReason: string | null;
  port: number | null;
  /** False when the listener could not bind or a firewall blocks it. */
  listenerAvailable: boolean;
  /** False when mDNS is unavailable. Manual addresses still work. */
  discoveryAvailable: boolean;
  hidden: boolean;
  displayName: string;
  initials: string;
  deviceShortId: string | null;
  introSeen: boolean;
  warnings: string[];
}

export type NetworkNodeGroup = 'paired' | 'nearby';
export type NetworkNodeStatus = 'online' | 'offline' | 'pairing' | 'needs_update' | 'available';

export interface NetworkNode {
  deviceId: string;
  /** For example `K7QF-2M9X`. The only id the UI shows. */
  shortId: string;
  displayName: string;
  initials: string;
  group: NetworkNodeGroup;
  status: NetworkNodeStatus;
  source: 'mdns' | 'manual' | null;
}

export interface NetworkAddress {
  host: string;
  port: number;
  addedAt: string;
}

export interface NetworkBlockedDevice {
  deviceId: string;
  shortId: string;
  name: string;
  blockedAt: string;
}

/** `code` is the six digits without a space. */
export interface NetworkPairingStarted {
  pairingId: string;
  code: string;
}

export type NetworkPairingDecision = 'accept' | 'decline' | 'block';

/** Channel `network-changed`. Fields are snake_case as sent. */
export type NetworkChangedEvent =
  | { type: 'nodesChanged' }
  | { type: 'statusChanged' }
  | { type: 'pairedRemoved'; device_id: string; display_name: string; by_peer: boolean };

/** Channel `network-pairing`. Fields are snake_case as sent. */
export type NetworkPairingEvent =
  | {
      type: 'pairingIncoming';
      pairing_id: string;
      device_id: string;
      short_id: string;
      display_name: string;
      code: string;
    }
  | {
      type: 'pairingFinished';
      pairing_id: string;
      /** `accepted`, `declined`, `expired`, `cancelled` or `failed`. */
      outcome: string;
      display_name: string;
    };

export const getNetworkStatus = (): Promise<NetworkStatus> =>
  invoke<NetworkStatus>('network_status');

export const listNetworkNodes = (): Promise<NetworkNode[]> =>
  invoke<NetworkNode[]>('network_list_nodes');

export const setNetworkVisibility = (hidden: boolean): Promise<void> =>
  invoke<void>('network_set_visibility', { hidden });

export const setNetworkProfile = (displayName: string): Promise<void> =>
  invoke<void>('network_set_profile', { displayName });

export const markNetworkIntroSeen = (): Promise<void> =>
  invoke<void>('network_mark_intro_seen');

export const listNetworkAddresses = (): Promise<NetworkAddress[]> =>
  invoke<NetworkAddress[]>('network_list_addresses');

export const addNetworkAddress = (host: string, port: number): Promise<void> =>
  invoke<void>('network_add_address', { host, port });

export const removeNetworkAddress = (host: string, port: number): Promise<void> =>
  invoke<void>('network_remove_address', { host, port });

export const requestNetworkPairing = (deviceId: string): Promise<NetworkPairingStarted> =>
  invoke<NetworkPairingStarted>('network_request_pairing', { deviceId });

export const respondNetworkPairing = (
  pairingId: string,
  decision: NetworkPairingDecision,
): Promise<void> => invoke<void>('network_respond_pairing', { pairingId, decision });

export const cancelNetworkPairing = (pairingId: string): Promise<void> =>
  invoke<void>('network_cancel_pairing', { pairingId });

export const removeNetworkPaired = (deviceId: string): Promise<void> =>
  invoke<void>('network_remove_paired', { deviceId });

export const listNetworkBlocked = (): Promise<NetworkBlockedDevice[]> =>
  invoke<NetworkBlockedDevice[]>('network_list_blocked');

export const unblockNetworkDevice = (deviceId: string): Promise<void> =>
  invoke<void>('network_unblock', { deviceId });

export const onNetworkChanged = (
  handler: (event: NetworkChangedEvent) => void,
): Promise<UnlistenFn> =>
  listen<NetworkChangedEvent>('network-changed', (e) => handler(e.payload));

export const onNetworkPairing = (
  handler: (event: NetworkPairingEvent) => void,
): Promise<UnlistenFn> =>
  listen<NetworkPairingEvent>('network-pairing', (e) => handler(e.payload));
```

- [ ] **Step 8: Add `src/lib/network-utils.ts`**

```ts
// The backend sends errors as their Display text, so the type prefix is dropped.
export function errorText(error: unknown): string {
  const raw = error instanceof Error ? error.message : typeof error === 'string' ? error : '';
  const text = raw.replace(/^(Invalid input|Internal error): /, '').trim();
  return text || 'Something went wrong. Try again.';
}

// Shows six digits as two groups of three, for example "482 913".
export function formatPairingCode(code: string): string {
  return /^\d{6}$/.test(code) ? `${code.slice(0, 3)} ${code.slice(3)}` : code;
}
```

- [ ] **Step 9: Add `src/stores/network-store.ts`**

```ts
import { create } from 'zustand';
import { errorText } from '@/lib/network-utils';
import {
  getNetworkStatus,
  listNetworkNodes,
  type NetworkNode,
  type NetworkStatus,
  requestNetworkPairing,
} from '@/lib/tauri-api';

/** An incoming request lives this long. It matches the backend pairing lifetime. */
export const INCOMING_TTL_MS = 120_000;

/** A request another node sent to this node. */
export interface PairingIncoming {
  pairingId: string;
  deviceId: string;
  shortId: string;
  displayName: string;
  /** The six digits without a space. */
  code: string;
  /** The local user pressed Accept and now waits for the peer. */
  accepted: boolean;
}

/** The pairing this node started. */
export interface ActivePairing {
  pairingId: string;
  deviceId: string;
  shortId: string;
  displayName: string;
  code: string;
  /** `waiting`: no local decision yet. `done`: Accept was sent, the peer decides next. */
  phase: 'waiting' | 'done';
}

export interface NetworkState {
  status: NetworkStatus | null;
  nodes: NetworkNode[];
  incoming: PairingIncoming[];
  activePairing: ActivePairing | null;
  panelOpen: boolean;
  /** The pairing whose dialog is open, for either role. */
  dialogPairingId: string | null;
  error: string | null;
  refresh: () => Promise<void>;
  refreshNodes: () => Promise<void>;
  addIncoming: (incoming: PairingIncoming) => void;
  removeIncoming: (pairingId: string) => void;
  markIncomingAccepted: (pairingId: string) => void;
  setActivePairing: (pairing: ActivePairing | null) => void;
  setPanelOpen: (open: boolean) => void;
  openPairingDialog: (pairingId: string) => void;
  closePairingDialog: () => void;
  reset: () => void;
}

const initialState = {
  status: null as NetworkStatus | null,
  nodes: [] as NetworkNode[],
  incoming: [] as PairingIncoming[],
  activePairing: null as ActivePairing | null,
  panelOpen: false,
  dialogPairingId: null as string | null,
  error: null as string | null,
};

// Expiry timers by pairing id, and the request counters that drop stale responses.
const expiryTimers = new Map<string, ReturnType<typeof setTimeout>>();
let statusSeq = 0;
let nodesSeq = 0;

function clearExpiryTimer(pairingId: string): void {
  const timer = expiryTimers.get(pairingId);
  if (timer === undefined) return;
  clearTimeout(timer);
  expiryTimers.delete(pairingId);
}

// A mirror of the backend network state. It is not saved.
export const useNetworkStore = create<NetworkState>()((set, get) => ({
  ...initialState,

  async refresh() {
    const seq = ++statusSeq;
    try {
      const status = await getNetworkStatus();
      if (seq === statusSeq) set({ status, error: null });
    } catch (err) {
      if (seq === statusSeq) set({ error: errorText(err) });
    }
  },

  async refreshNodes() {
    const seq = ++nodesSeq;
    try {
      const nodes = await listNetworkNodes();
      if (seq === nodesSeq) set({ nodes, error: null });
    } catch (err) {
      if (seq === nodesSeq) set({ error: errorText(err) });
    }
  },

  addIncoming(incoming) {
    // A repeated event neither adds a row nor extends the expiry.
    if (get().incoming.some((i) => i.pairingId === incoming.pairingId)) return;
    set({ incoming: [...get().incoming, incoming] });
    expiryTimers.set(
      incoming.pairingId,
      setTimeout(() => get().removeIncoming(incoming.pairingId), INCOMING_TTL_MS),
    );
  },

  removeIncoming(pairingId) {
    clearExpiryTimer(pairingId);
    if (!get().incoming.some((i) => i.pairingId === pairingId)) return;
    set({ incoming: get().incoming.filter((i) => i.pairingId !== pairingId) });
  },

  markIncomingAccepted(pairingId) {
    if (!get().incoming.some((i) => i.pairingId === pairingId)) return;
    set({
      incoming: get().incoming.map((i) => (i.pairingId === pairingId ? { ...i, accepted: true } : i)),
    });
  },

  setActivePairing: (activePairing) => set({ activePairing }),
  setPanelOpen: (panelOpen) => set({ panelOpen }),
  openPairingDialog: (dialogPairingId) => set({ dialogPairingId }),
  closePairingDialog: () => set({ dialogPairingId: null }),

  reset() {
    for (const timer of expiryTimers.values()) clearTimeout(timer);
    expiryTimers.clear();
    // Bumping the counters makes any request still in flight a no-op.
    statusSeq++;
    nodesSeq++;
    set({ ...initialState });
  },
}));

/** Paired nodes that are online. It feeds the number in the status bar. */
export const selectOnlinePairedCount = (s: NetworkState): number =>
  s.nodes.filter((n) => n.group === 'paired' && n.status === 'online').length;

/**
 * Asks a nearby node to pair, keeps the code and opens the pairing dialog.
 * It rejects with the backend error and leaves the state alone.
 */
export async function startNetworkPairing(node: NetworkNode): Promise<void> {
  const started = await requestNetworkPairing(node.deviceId);
  const store = useNetworkStore.getState();
  store.setActivePairing({
    pairingId: started.pairingId,
    deviceId: node.deviceId,
    shortId: node.shortId,
    displayName: node.displayName,
    code: started.code,
    phase: 'waiting',
  });
  store.openPairingDialog(started.pairingId);
  store.setPanelOpen(false);
}
```

- [ ] **Step 10: Add `src/lib/network-event-bridge.ts`**

```ts
import { useEffect } from 'react';
import { toast } from 'sonner';
import {
  type NetworkChangedEvent,
  type NetworkPairingEvent,
  onNetworkChanged,
  onNetworkPairing,
} from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';

// Changes carry no data worth keeping, so the store refetches from the commands.
export function handleNetworkChanged(event: NetworkChangedEvent): void {
  const store = useNetworkStore.getState();
  switch (event.type) {
    case 'nodesChanged':
      void store.refreshNodes();
      break;
    case 'statusChanged':
      void store.refresh();
      break;
    case 'pairedRemoved':
      void store.refreshNodes();
      if (event.by_peer) toast.info(`${event.display_name} removed the pairing.`);
      break;
  }
}

export function handleNetworkPairing(event: NetworkPairingEvent): void {
  const store = useNetworkStore.getState();
  if (event.type === 'pairingIncoming') {
    store.addIncoming({
      pairingId: event.pairing_id,
      deviceId: event.device_id,
      shortId: event.short_id,
      displayName: event.display_name,
      code: event.code,
      accepted: false,
    });
    return;
  }

  // Read the state before it changes below.
  const started = store.activePairing?.pairingId === event.pairing_id;
  store.removeIncoming(event.pairing_id);
  if (started) store.setActivePairing(null);
  if (store.dialogPairingId === event.pairing_id) store.closePairingDialog();

  const name = event.display_name;
  switch (event.outcome) {
    case 'accepted':
      toast.success(`Paired with ${name}.`);
      void store.refreshNodes();
      break;
    case 'declined':
      // The user who pressed Decline or Block already knows.
      if (started) toast.info(`${name} declined.`);
      break;
    case 'expired':
      toast.info('The request expired.');
      break;
    case 'failed':
      toast.error('Pairing failed. Try again.');
      break;
    default:
      // A cancelled pairing and any outcome added later stay quiet.
      break;
  }
}

/**
 * Subscribes once, for the app's lifetime, to the two network channels and loads the first
 * status and node list. Mount it once, next to the other event bridges.
 */
export function useNetworkEventBridge(): void {
  useEffect(() => {
    let disposed = false;
    const unlisteners: (() => void)[] = [];
    // A listener that resolves after disposal is unlistened at once.
    const keep = (registration: Promise<() => void>) =>
      registration
        .then((unlisten) => {
          if (disposed) unlisten();
          else unlisteners.push(unlisten);
        })
        .catch((err) => console.error('[network] failed to register a listener', err));

    keep(
      onNetworkChanged((event) => {
        if (!disposed) handleNetworkChanged(event);
      }),
    );
    keep(
      onNetworkPairing((event) => {
        if (!disposed) handleNetworkPairing(event);
      }),
    );

    void useNetworkStore.getState().refresh();
    void useNetworkStore.getState().refreshNodes();

    return () => {
      disposed = true;
      for (const unlisten of unlisteners) unlisten();
      unlisteners.length = 0;
    };
  }, []);
}
```

- [ ] **Step 11: Mount the bridge in `src/App.tsx`**

Edit 1: after the quoted line `import { useGraphQlSubscriptionEventBridge } from '@/lib/graphql-subscription-event-bridge';` add:

```ts
import { useNetworkEventBridge } from '@/lib/network-event-bridge';
```

Edit 2: after the quoted line `  useFlowRequestPathEvents();` (inside `App`) add:

```ts
  useNetworkEventBridge();
```

- [ ] **Step 12: Run the tests to verify they pass**

Run: `bunx vitest run src/lib/__tests__/tauri-api.network.test.ts src/lib/__tests__/network-utils.test.ts src/stores/__tests__/network-store.test.ts src/lib/__tests__/network-event-bridge.test.ts`
Expected: PASS (all four files).

- [ ] **Step 13: Run the checks**

Run: `bun run tsc --noEmit && bun run check`
Expected: no errors. If `check` reports format or import order, run `bun run lint`, then `bun run check` again.

- [ ] **Step 14: No commit**

No commit; reviewed at the plan end.

---

### Task 2: Status bar item, Network panel and the Add and Remove dialogs

**Files:**
- Create: `src/components/network/network-helpers.ts`, `src/components/network/NodeRow.tsx`, `src/components/network/NetworkPanel.tsx`, `src/components/network/AddAddressDialog.tsx`, `src/components/network/RemovePairedDialog.tsx`, `src/components/status-bar/NetworkStatusItem.tsx`
- Modify: `src/components/layout/StatusBar.tsx`, `src/components/layout/__tests__/StatusBar.test.tsx`
- Test: `src/components/network/__tests__/network-helpers.test.ts`, `src/components/network/__tests__/NetworkPanel.test.tsx`, `src/components/network/__tests__/AddAddressDialog.test.tsx`, `src/components/network/__tests__/RemovePairedDialog.test.tsx`, `src/components/status-bar/__tests__/NetworkStatusItem.test.tsx`

**Interfaces:**
- Consumes: everything Task 1 produces.
- Produces (used by Task 3):

```ts
// network-helpers.ts
export const DEFAULT_NETWORK_PORT = '47821';
export function filterNodes(nodes: NetworkNode[], query: string): NetworkNode[];  // name, initials or short id; case, spaces and dashes ignored
export function sortNodes(nodes: NetworkNode[]): NetworkNode[];                   // online, pairing, needs update, available, offline; then name; no mutation
export function groupNodes(nodes: NetworkNode[]): { paired: NetworkNode[]; nearby: NetworkNode[] };
export type AddressCheck = { ok: true; host: string; port: number } | { ok: false; error: string };
export function validateAddress(host: string, port: string): AddressCheck;

// NodeRow.tsx
export function NodeRow(props: { node: NetworkNode; busy: boolean; onPair: (n: NetworkNode) => void; onRemove: (n: NetworkNode) => void }): JSX.Element;

// NetworkPanel.tsx
export function NetworkPanel(props: { onAddAddress: () => void; onRemove: (n: NetworkNode) => void }): JSX.Element;

// AddAddressDialog.tsx
export function AddAddressDialog(props: { open: boolean; onOpenChange: (open: boolean) => void }): JSX.Element;

// RemovePairedDialog.tsx
export function RemovePairedDialog(props: { node: NetworkNode | null; onClose: () => void }): JSX.Element;

// NetworkStatusItem.tsx
export function NetworkStatusItem(): JSX.Element;
```

Until Task 3 mounts `PairingDialog`, pressing Pair stores the pairing and opens nothing visible. That is expected.

- [ ] **Step 1: Write the failing helper tests**

Create `src/components/network/__tests__/network-helpers.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import {
  filterNodes,
  groupNodes,
  sortNodes,
  validateAddress,
} from '@/components/network/network-helpers';
import { makeNode } from '@/test/network-fixtures';

const ana = makeNode({
  deviceId: '1',
  displayName: 'Ana Lima',
  initials: 'AL',
  shortId: 'K7QF-2M9X',
});
const ben = makeNode({
  deviceId: '2',
  displayName: 'Ben Ortiz',
  initials: 'BO',
  shortId: 'AAAA-1111',
});
const ids = (list: { deviceId: string }[]) => list.map((n) => n.deviceId);

describe('filterNodes', () => {
  it('returns the same list for an empty or blank query', () => {
    const nodes = [ana, ben];
    expect(filterNodes(nodes, '')).toBe(nodes);
    expect(filterNodes(nodes, '   ')).toBe(nodes);
  });

  it('matches the name without caring about case', () => {
    expect(ids(filterNodes([ana, ben], 'ANA li'))).toEqual(['1']);
  });

  it('matches the initials', () => {
    expect(ids(filterNodes([ana, ben], 'bo'))).toEqual(['2']);
  });

  it('matches the short id with or without the dash', () => {
    expect(ids(filterNodes([ana, ben], 'k7qf-2m'))).toEqual(['1']);
    expect(ids(filterNodes([ana, ben], 'k7qf2m'))).toEqual(['1']);
  });

  it('returns an empty list when nothing matches', () => {
    expect(filterNodes([ana, ben], 'zzz')).toEqual([]);
  });

  it('finds one of two nodes that share a name by its short id', () => {
    const twin = makeNode({ deviceId: '3', displayName: 'Ben Ortiz', shortId: 'BBBB-2222' });
    expect(ids(filterNodes([ben, twin], 'bbbb2222'))).toEqual(['3']);
  });
});

describe('sortNodes', () => {
  it('puts online first and offline last, then sorts by name', () => {
    const list = [
      makeNode({ deviceId: 'off', displayName: 'A', status: 'offline' }),
      makeNode({ deviceId: 'on-z', displayName: 'Z', status: 'online' }),
      makeNode({ deviceId: 'on-b', displayName: 'B', status: 'online' }),
      makeNode({ deviceId: 'pair', displayName: 'C', status: 'pairing' }),
      makeNode({ deviceId: 'upd', displayName: 'D', status: 'needs_update' }),
      makeNode({ deviceId: 'av', displayName: 'E', status: 'available' }),
    ];
    expect(ids(sortNodes(list))).toEqual(['on-b', 'on-z', 'pair', 'upd', 'av', 'off']);
  });

  it('does not change the input', () => {
    const list = [
      makeNode({ deviceId: 'off', status: 'offline' }),
      makeNode({ deviceId: 'on', status: 'online' }),
    ];
    sortNodes(list);
    expect(ids(list)).toEqual(['off', 'on']);
  });
});

describe('groupNodes', () => {
  it('splits paired from nearby', () => {
    const groups = groupNodes([
      makeNode({ deviceId: 'a', group: 'paired' }),
      makeNode({ deviceId: 'b', group: 'nearby' }),
    ]);
    expect(ids(groups.paired)).toEqual(['a']);
    expect(ids(groups.nearby)).toEqual(['b']);
  });
});

describe('validateAddress', () => {
  it('trims the host and parses the port', () => {
    expect(validateAddress('  10.0.0.5 ', '47821')).toEqual({
      ok: true,
      host: '10.0.0.5',
      port: 47821,
    });
    expect(validateAddress('mac.tail1234.ts.net', '1')).toMatchObject({ ok: true, port: 1 });
    expect(validateAddress('h', '65535')).toMatchObject({ ok: true, port: 65535 });
  });

  it('asks for a host', () => {
    expect(validateAddress('', '47821')).toEqual({
      ok: false,
      error: 'Enter a host name or IP address.',
    });
    expect(validateAddress('   ', '47821')).toMatchObject({ ok: false });
  });

  it('refuses spaces and paths in the host', () => {
    const error = 'Enter only the host, without spaces or a path.';
    expect(validateAddress('a b', '47821')).toEqual({ ok: false, error });
    expect(validateAddress('http://x', '47821')).toEqual({ ok: false, error });
    expect(validateAddress('x/y', '47821')).toEqual({ ok: false, error });
  });

  it.each(['0', '65536', '80a', '', '-1', '1.5', '123456'])('refuses the port "%s"', (port) => {
    expect(validateAddress('host', port)).toEqual({
      ok: false,
      error: 'Port must be a number from 1 to 65535.',
    });
  });
});
```

- [ ] **Step 2: Write the failing panel tests**

Create `src/components/network/__tests__/NetworkPanel.test.tsx`:

```tsx
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { NetworkPanel } from '@/components/network/NetworkPanel';
import { useNetworkStore } from '@/stores/network-store';
import { makeNode, makeStatus } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
  api: {
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
    requestNetworkPairing: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const cleo = makeNode({
  deviceId: 'P1',
  displayName: 'Cleo Park',
  initials: 'CP',
  shortId: 'CCCC-3333',
  group: 'paired',
  status: 'offline',
});
const dana = makeNode({
  deviceId: 'P2',
  displayName: 'Dana Roy',
  initials: 'DR',
  shortId: 'DDDD-4444',
  group: 'paired',
  status: 'online',
});
const ben = makeNode({ deviceId: 'N1', displayName: 'Ben Ortiz', shortId: 'AAAA-1111' });

function setup(nodes = [cleo, dana, ben], status = makeStatus()) {
  useNetworkStore.setState({ status, nodes });
  const onAddAddress = vi.fn();
  const onRemove = vi.fn();
  render(<NetworkPanel onAddAddress={onAddAddress} onRemove={onRemove} />);
  return { onAddAddress, onRemove };
}

const search = () => screen.getByRole('searchbox', { name: 'Search nodes' });

describe('NetworkPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
  });

  it('shows a loading line until the status arrives', () => {
    render(<NetworkPanel onAddAddress={vi.fn()} onRemove={vi.fn()} />);
    expect(screen.getByText('Loading...')).toBeInTheDocument();
  });

  it('lists paired nodes with online first and offline greyed, then nearby nodes', () => {
    setup();
    const pairedItems = within(screen.getByRole('list', { name: 'Paired' })).getAllByRole(
      'listitem',
    );
    expect(pairedItems[0]).toHaveTextContent('Dana Roy');
    expect(pairedItems[0]).toHaveTextContent('Online');
    expect(pairedItems[1]).toHaveTextContent('Cleo Park');
    expect(pairedItems[1]).toHaveTextContent('Offline');
    expect(pairedItems[1]).toHaveClass('opacity-60');
    expect(pairedItems[0]).not.toHaveClass('opacity-60');
    const nearby = within(screen.getByRole('list', { name: 'Nearby' }));
    expect(nearby.getByText('Ben Ortiz')).toBeInTheDocument();
    expect(nearby.getByText('AAAA-1111')).toBeInTheDocument();
    expect(nearby.getByRole('button', { name: 'Pair with Ben Ortiz (AAAA-1111)' })).toBeEnabled();
  });

  it('shows the initials circle and the short id of each row', () => {
    setup();
    expect(screen.getByText('DR')).toBeInTheDocument();
    expect(screen.getByText('DDDD-4444')).toBeInTheDocument();
  });

  it('filters by name, initials and short id', async () => {
    setup();
    await userEvent.type(search(), 'dana');
    expect(screen.getByText('Dana Roy')).toBeInTheDocument();
    expect(screen.queryByText('Cleo Park')).not.toBeInTheDocument();
    expect(screen.queryByText('Ben Ortiz')).not.toBeInTheDocument();

    await userEvent.clear(search());
    await userEvent.type(search(), 'cccc3333');
    expect(screen.getByText('Cleo Park')).toBeInTheDocument();
    expect(screen.queryByText('Dana Roy')).not.toBeInTheDocument();

    await userEvent.clear(search());
    await userEvent.type(search(), 'ao');
    expect(screen.queryByText('Cleo Park')).not.toBeInTheDocument();
  });

  it('says nothing matches, keeps the requests and hides the empty state', async () => {
    setup();
    useNetworkStore.getState().addIncoming({
      pairingId: 'p1',
      deviceId: 'DEV9',
      shortId: 'ZZZZ-9999',
      displayName: 'Eli Stone',
      code: '482913',
      accepted: false,
    });
    await userEvent.type(search(), 'zzzqq');
    expect(screen.getByText('No nodes match "zzzqq".')).toBeInTheDocument();
    expect(screen.queryByText('No Rocket nodes found on this network.')).not.toBeInTheDocument();
    expect(screen.getByText('Eli Stone')).toBeInTheDocument();
  });

  it('tells two nodes with the same name apart by short id', async () => {
    const twin = makeNode({ deviceId: 'N2', displayName: 'Ben Ortiz', shortId: 'BBBB-2222' });
    setup([ben, twin]);
    expect(screen.getByText('AAAA-1111')).toBeInTheDocument();
    expect(screen.getByText('BBBB-2222')).toBeInTheDocument();
    expect(screen.getAllByRole('button', { name: /^Pair with Ben Ortiz/ })).toHaveLength(2);
    await userEvent.type(search(), 'bbbb2222');
    expect(screen.getAllByRole('button', { name: /^Pair with Ben Ortiz/ })).toHaveLength(1);
    expect(screen.getByRole('button', { name: 'Pair with Ben Ortiz (BBBB-2222)' })).toBeEnabled();
  });

  it('disables Pair and shows a chip for a node that needs an update', () => {
    setup([makeNode({ displayName: 'Old Mac', shortId: 'OLDM-0001', status: 'needs_update' })]);
    expect(screen.getByText('Needs update')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Pair with Old Mac (OLDM-0001)' })).toBeDisabled();
  });

  it('shows the empty state with the VPN hint and one Add by address button', async () => {
    const { onAddAddress } = setup([]);
    expect(screen.getByText('No Rocket nodes found on this network.')).toBeInTheDocument();
    expect(screen.getByText('VPNs and some Wi-Fi networks block discovery.')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Add by address' }));
    expect(onAddAddress).toHaveBeenCalledTimes(1);
  });

  it('says discovery is unavailable instead of the VPN hint', () => {
    setup([], makeStatus({ discoveryAvailable: false }));
    expect(screen.getByText("Automatic discovery isn't available here.")).toBeInTheDocument();
    expect(
      screen.queryByText('VPNs and some Wi-Fi networks block discovery.'),
    ).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Add by address' })).toBeInTheDocument();
  });

  it('warns when others cannot connect', () => {
    setup([dana], makeStatus({ listenerAvailable: false }));
    expect(
      screen.getByText("Others can't connect to you. Allow Rocket in your firewall settings."),
    ).toBeInTheDocument();
    expect(screen.getByText('Dana Roy')).toBeInTheDocument();
  });

  it('shows why networking is off and nothing else', () => {
    setup([dana], makeStatus({ state: 'off', offReason: 'The keychain is not available.' }));
    expect(screen.getByText('The keychain is not available.')).toBeInTheDocument();
    expect(screen.queryByRole('searchbox')).not.toBeInTheDocument();
    expect(screen.queryByText('Dana Roy')).not.toBeInTheDocument();
  });

  it('lists open requests at the top and reviews one', async () => {
    setup([dana]);
    useNetworkStore.setState({ panelOpen: true });
    useNetworkStore.getState().addIncoming({
      pairingId: 'p1',
      deviceId: 'DEV9',
      shortId: 'ZZZZ-9999',
      displayName: 'Eli Stone',
      code: '482913',
      accepted: false,
    });
    const requests = within(await screen.findByRole('list', { name: 'Requests' }));
    expect(requests.getByText('Eli Stone')).toBeInTheDocument();
    expect(requests.getByText('ZZZZ-9999 · 482 913')).toBeInTheDocument();
    await userEvent.click(
      requests.getByRole('button', { name: 'Review request from Eli Stone (ZZZZ-9999)' }),
    );
    expect(useNetworkStore.getState().dialogPairingId).toBe('p1');
    expect(useNetworkStore.getState().panelOpen).toBe(false);
  });

  it('starts a pairing from the Pair button', async () => {
    setup([ben]);
    mocks.api.requestNetworkPairing.mockResolvedValue({ pairingId: 'p7', code: '482913' });
    await userEvent.click(screen.getByRole('button', { name: 'Pair with Ben Ortiz (AAAA-1111)' }));
    await waitFor(() => expect(useNetworkStore.getState().dialogPairingId).toBe('p7'));
    expect(mocks.api.requestNetworkPairing).toHaveBeenCalledWith('N1');
    expect(useNetworkStore.getState().activePairing?.code).toBe('482913');
  });

  it('shows the backend error when a pairing cannot start', async () => {
    setup([ben]);
    mocks.api.requestNetworkPairing.mockRejectedValue(
      "Invalid input: You've reached the limit of 200 paired devices. Remove one to pair.",
    );
    await userEvent.click(screen.getByRole('button', { name: 'Pair with Ben Ortiz (AAAA-1111)' }));
    await waitFor(() =>
      expect(mocks.toast.error).toHaveBeenCalledWith(
        "You've reached the limit of 200 paired devices. Remove one to pair.",
      ),
    );
    expect(useNetworkStore.getState().dialogPairingId).toBeNull();
    expect(screen.getByRole('button', { name: 'Pair with Ben Ortiz (AAAA-1111)' })).toBeEnabled();
  });

  it('asks the host to remove a paired node from its menu', async () => {
    const { onRemove } = setup([dana]);
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Dana Roy (DDDD-4444)' }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Remove' }));
    expect(onRemove).toHaveBeenCalledWith(expect.objectContaining({ deviceId: 'P2' }));
  });
});
```

- [ ] **Step 3: Write the failing dialog tests**

Create `src/components/network/__tests__/AddAddressDialog.test.tsx`:

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { AddAddressDialog } from '@/components/network/AddAddressDialog';
import { useNetworkStore } from '@/stores/network-store';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
  api: {
    addNetworkAddress: vi.fn(),
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

function setup() {
  const onOpenChange = vi.fn();
  render(<AddAddressDialog open onOpenChange={onOpenChange} />);
  return { onOpenChange };
}

const host = () => screen.getByLabelText('Host');
const port = () => screen.getByLabelText('Port');

describe('AddAddressDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
    mocks.api.addNetworkAddress.mockResolvedValue(undefined);
    mocks.api.listNetworkNodes.mockResolvedValue([]);
  });

  it('starts with the default port and an empty host', () => {
    setup();
    expect(host()).toHaveValue('');
    expect(port()).toHaveValue('47821');
  });

  it('adds the address, reloads the nodes and closes', async () => {
    const { onOpenChange } = setup();
    await userEvent.type(host(), ' 10.0.0.5 ');
    await userEvent.click(screen.getByRole('button', { name: 'Add' }));
    await waitFor(() => expect(mocks.api.addNetworkAddress).toHaveBeenCalledWith('10.0.0.5', 47821));
    await waitFor(() => expect(mocks.api.listNetworkNodes).toHaveBeenCalled());
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it('submits with Enter in the port field', async () => {
    setup();
    await userEvent.type(host(), 'mac.tail1234.ts.net');
    await userEvent.clear(port());
    await userEvent.type(port(), '47822{Enter}');
    await waitFor(() =>
      expect(mocks.api.addNetworkAddress).toHaveBeenCalledWith('mac.tail1234.ts.net', 47822),
    );
  });

  it('refuses an empty host without calling the backend', async () => {
    const { onOpenChange } = setup();
    await userEvent.click(screen.getByRole('button', { name: 'Add' }));
    expect(screen.getByRole('alert')).toHaveTextContent('Enter a host name or IP address.');
    expect(mocks.api.addNetworkAddress).not.toHaveBeenCalled();
    expect(onOpenChange).not.toHaveBeenCalled();
  });

  it.each(['0', '65536', '80a'])('refuses the port %s', async (value) => {
    setup();
    await userEvent.type(host(), 'host');
    await userEvent.clear(port());
    await userEvent.type(port(), value);
    await userEvent.click(screen.getByRole('button', { name: 'Add' }));
    expect(screen.getByRole('alert')).toHaveTextContent('Port must be a number from 1 to 65535.');
    expect(mocks.api.addNetworkAddress).not.toHaveBeenCalled();
  });

  it('shows the backend error and stays open', async () => {
    mocks.api.addNetworkAddress.mockRejectedValue('Invalid input: You already saved 50 addresses.');
    const { onOpenChange } = setup();
    await userEvent.type(host(), 'host');
    await userEvent.click(screen.getByRole('button', { name: 'Add' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('You already saved 50 addresses.');
    expect(onOpenChange).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'Add' })).toBeEnabled();
  });
});
```

Create `src/components/network/__tests__/RemovePairedDialog.test.tsx`:

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { RemovePairedDialog } from '@/components/network/RemovePairedDialog';
import { useNetworkStore } from '@/stores/network-store';
import { makeNode } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
  api: {
    removeNetworkPaired: vi.fn(),
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const dana = makeNode({ deviceId: 'P2', displayName: 'Dana Roy', shortId: 'DDDD-4444' });

describe('RemovePairedDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
    mocks.api.removeNetworkPaired.mockResolvedValue(undefined);
    mocks.api.listNetworkNodes.mockResolvedValue([]);
  });

  it('renders nothing while there is no node', () => {
    render(<RemovePairedDialog node={null} onClose={vi.fn()} />);
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();
  });

  it('names the node and removes it on confirm', async () => {
    const onClose = vi.fn();
    render(<RemovePairedDialog node={dana} onClose={onClose} />);
    const dialog = await screen.findByRole('alertdialog', { name: 'Remove Dana Roy?' });
    expect(dialog).toHaveTextContent('DDDD-4444');
    await userEvent.click(screen.getByRole('button', { name: 'Remove' }));
    await waitFor(() => expect(mocks.api.removeNetworkPaired).toHaveBeenCalledWith('P2'));
    await waitFor(() => expect(mocks.api.listNetworkNodes).toHaveBeenCalled());
    expect(onClose).toHaveBeenCalled();
  });

  it('does not remove on Cancel', async () => {
    const onClose = vi.fn();
    render(<RemovePairedDialog node={dana} onClose={onClose} />);
    await userEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
    expect(mocks.api.removeNetworkPaired).not.toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it('shows the backend error in a toast', async () => {
    mocks.api.removeNetworkPaired.mockRejectedValue('Internal error: disk full');
    render(<RemovePairedDialog node={dana} onClose={vi.fn()} />);
    await userEvent.click(await screen.findByRole('button', { name: 'Remove' }));
    await waitFor(() => expect(mocks.toast.error).toHaveBeenCalledWith('disk full'));
  });
});
```

- [ ] **Step 4: Write the failing status item tests and extend the StatusBar test**

Create `src/components/status-bar/__tests__/NetworkStatusItem.test.tsx`:

```tsx
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { NetworkStatusItem } from '@/components/status-bar/NetworkStatusItem';
import { useNetworkStore } from '@/stores/network-store';
import { makeNode, makeStatus } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
  api: {
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
    requestNetworkPairing: vi.fn(),
    removeNetworkPaired: vi.fn(),
    addNetworkAddress: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const dana = makeNode({
  deviceId: 'P2',
  displayName: 'Dana Roy',
  shortId: 'DDDD-4444',
  group: 'paired',
  status: 'online',
});
const cleo = makeNode({
  deviceId: 'P1',
  displayName: 'Cleo Park',
  shortId: 'CCCC-3333',
  group: 'paired',
  status: 'online',
});

describe('NetworkStatusItem', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
    useNetworkStore.setState({ status: makeStatus(), nodes: [dana, cleo] });
    mocks.api.getNetworkStatus.mockResolvedValue(makeStatus());
    mocks.api.listNetworkNodes.mockResolvedValue([dana, cleo]);
    mocks.api.removeNetworkPaired.mockResolvedValue(undefined);
  });

  it('shows the number of online paired nodes', () => {
    render(<NetworkStatusItem />);
    expect(screen.getByRole('button', { name: 'Team network, 2 online' })).toBeInTheDocument();
    expect(screen.getByText('2')).toBeInTheDocument();
  });

  it('shows no number when nobody is online', () => {
    useNetworkStore.setState({ nodes: [{ ...dana, status: 'offline' }] });
    render(<NetworkStatusItem />);
    expect(screen.getByRole('button', { name: 'Team network, 0 online' })).toBeInTheDocument();
    expect(screen.queryByText('0')).not.toBeInTheDocument();
  });

  it('says so when networking is off', () => {
    useNetworkStore.setState({ status: makeStatus({ state: 'off', offReason: 'No keychain.' }) });
    render(<NetworkStatusItem />);
    expect(screen.getByRole('button', { name: 'Team network, off' })).toBeInTheDocument();
  });

  it('opens the panel and reloads the status and the nodes', async () => {
    render(<NetworkStatusItem />);
    await userEvent.click(screen.getByRole('button', { name: 'Team network, 2 online' }));
    expect(await screen.findByRole('searchbox', { name: 'Search nodes' })).toBeInTheDocument();
    expect(useNetworkStore.getState().panelOpen).toBe(true);
    await waitFor(() => expect(mocks.api.getNetworkStatus).toHaveBeenCalledTimes(1));
    expect(mocks.api.listNetworkNodes).toHaveBeenCalledTimes(1);
  });

  it('opens when the store asks for it', async () => {
    useNetworkStore.setState({ panelOpen: true });
    render(<NetworkStatusItem />);
    expect(await screen.findByRole('searchbox', { name: 'Search nodes' })).toBeInTheDocument();
  });

  it('opens Add by address after the panel closes', async () => {
    render(<NetworkStatusItem />);
    await userEvent.click(screen.getByRole('button', { name: 'Team network, 2 online' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Add by address' }));
    expect(await screen.findByRole('dialog', { name: 'Add by address' })).toBeInTheDocument();
    expect(useNetworkStore.getState().panelOpen).toBe(false);
  });

  it('removes a paired node through the menu and the confirmation', async () => {
    render(<NetworkStatusItem />);
    await userEvent.click(screen.getByRole('button', { name: 'Team network, 2 online' }));
    await userEvent.click(
      await screen.findByRole('button', { name: 'Actions for Dana Roy (DDDD-4444)' }),
    );
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Remove' }));
    const dialog = await screen.findByRole('alertdialog', { name: 'Remove Dana Roy?' });
    expect(useNetworkStore.getState().panelOpen).toBe(false);
    await userEvent.click(within(dialog).getByRole('button', { name: 'Remove' }));
    await waitFor(() => expect(mocks.api.removeNetworkPaired).toHaveBeenCalledWith('P2'));
  });
});
```

Edit `src/components/layout/__tests__/StatusBar.test.tsx`. After the quoted lines

```tsx
vi.mock('@/components/status-bar/ContractsStatusItem', () => ({
  ContractsStatusItem: () => null,
}));
```

add:

```tsx
vi.mock('@/components/status-bar/NetworkStatusItem', () => ({
  NetworkStatusItem: () => <div data-testid='network-status-item' />,
}));
```

and add this test inside the `describe('StatusBar', ...)` block, after the quoted test title `it('displays the app version in the bottom-right corner', ...)` block ends:

```tsx
  it('renders the team network item', () => {
    render(<StatusBar />);
    expect(screen.getByTestId('network-status-item')).toBeInTheDocument();
  });
```

- [ ] **Step 5: Run the tests to verify they fail**

Run: `bunx vitest run src/components/network src/components/status-bar/__tests__/NetworkStatusItem.test.tsx src/components/layout/__tests__/StatusBar.test.tsx`
Expected: FAIL. Each new file reports `Failed to resolve import` for `@/components/network/network-helpers`, `NetworkPanel`, `AddAddressDialog`, `RemovePairedDialog` or `@/components/status-bar/NetworkStatusItem`. The StatusBar test reports `Unable to find an element by: [data-testid="network-status-item"]`.

- [ ] **Step 6: Add `src/components/network/network-helpers.ts`**

```ts
import type { NetworkNode, NetworkNodeStatus } from '@/lib/tauri-api';

export const DEFAULT_NETWORK_PORT = '47821';

// Case, spaces and dashes do not matter, so "k7qf 2m" finds K7QF-2M9X.
const normalize = (text: string): string => text.toLowerCase().replace(/[\s-]/g, '');

/** Keeps the nodes whose name, initials or short id contain the query. */
export function filterNodes(nodes: NetworkNode[], query: string): NetworkNode[] {
  const needle = normalize(query);
  if (!needle) return nodes;
  return nodes.filter((node) =>
    [node.displayName, node.initials, node.shortId].some((field) =>
      normalize(field).includes(needle),
    ),
  );
}

const RANK: Record<NetworkNodeStatus, number> = {
  online: 0,
  pairing: 1,
  needs_update: 2,
  available: 3,
  offline: 4,
};

/** Online first, offline last, then by name and short id. The input is not changed. */
export function sortNodes(nodes: NetworkNode[]): NetworkNode[] {
  return [...nodes].sort(
    (a, b) =>
      RANK[a.status] - RANK[b.status] ||
      a.displayName.localeCompare(b.displayName) ||
      a.shortId.localeCompare(b.shortId),
  );
}

export function groupNodes(nodes: NetworkNode[]): { paired: NetworkNode[]; nearby: NetworkNode[] } {
  return {
    paired: nodes.filter((n) => n.group === 'paired'),
    nearby: nodes.filter((n) => n.group === 'nearby'),
  };
}

export type AddressCheck = { ok: true; host: string; port: number } | { ok: false; error: string };

const PORT_ERROR = 'Port must be a number from 1 to 65535.';

/** Checks the Add by address form. The backend still validates the host. */
export function validateAddress(hostInput: string, portInput: string): AddressCheck {
  const host = hostInput.trim();
  if (!host) return { ok: false, error: 'Enter a host name or IP address.' };
  if (/\s|\//.test(host)) {
    return { ok: false, error: 'Enter only the host, without spaces or a path.' };
  }
  const portText = portInput.trim();
  if (!/^\d{1,5}$/.test(portText)) return { ok: false, error: PORT_ERROR };
  const port = Number(portText);
  if (port < 1 || port > 65535) return { ok: false, error: PORT_ERROR };
  return { ok: true, host, port };
}
```

- [ ] **Step 7: Add `src/components/network/NodeRow.tsx`**

```tsx
import { MoreHorizontal, Trash2 } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import type { NetworkNode, NetworkNodeStatus } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';

interface Chip {
  label: string;
  variant: 'secondary' | 'outline' | 'warning';
  className?: string;
}

// A nearby node that can be paired has no chip. Its Pair button says enough.
const CHIPS: Partial<Record<NetworkNodeStatus, Chip>> = {
  online: { label: 'Online', variant: 'secondary', className: 'text-green-600 dark:text-green-400' },
  offline: { label: 'Offline', variant: 'outline' },
  pairing: { label: 'Pairing…', variant: 'secondary' },
  needs_update: { label: 'Needs update', variant: 'warning' },
};

interface NodeRowProps {
  node: NetworkNode;
  /** A pairing request for this node is in flight. */
  busy: boolean;
  onPair: (node: NetworkNode) => void;
  onRemove: (node: NetworkNode) => void;
}

// One node: initials, name, short id, status chip, and Pair or a menu.
export function NodeRow({ node, busy, onPair, onRemove }: NodeRowProps) {
  const chip = CHIPS[node.status];
  const label = `${node.displayName} (${node.shortId})`;
  const canPair = node.status === 'available';

  return (
    <li
      className={cn(
        'flex items-center gap-2 px-3 py-1.5',
        node.status === 'offline' && 'opacity-60',
      )}
    >
      <span
        aria-hidden='true'
        className='flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-muted text-[10px] font-semibold text-muted-foreground'
      >
        {node.initials}
      </span>
      <div className='min-w-0 flex-1'>
        <p className='truncate text-xs font-medium'>{node.displayName}</p>
        <p className='font-mono text-2xs text-muted-foreground'>{node.shortId}</p>
      </div>
      {chip && (
        <Badge variant={chip.variant} className={cn('px-1.5 py-0 text-2xs', chip.className)}>
          {chip.label}
        </Badge>
      )}
      {node.group === 'nearby' ? (
        <Button
          size='sm'
          variant='outline'
          className='h-6 px-2 text-xs'
          disabled={!canPair || busy}
          aria-label={`Pair with ${label}`}
          onClick={() => onPair(node)}
        >
          Pair
        </Button>
      ) : (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              size='icon'
              variant='ghost'
              className='h-6 w-6'
              aria-label={`Actions for ${label}`}
            >
              <MoreHorizontal className='h-3.5 w-3.5' aria-hidden='true' />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align='end'>
            <DropdownMenuItem onSelect={() => onRemove(node)}>
              <Trash2 className='h-3.5 w-3.5' aria-hidden='true' />
              Remove
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </li>
  );
}
```

- [ ] **Step 8: Add `src/components/network/NetworkPanel.tsx`**

```tsx
import { Plus, Radar, Search } from 'lucide-react';
import { type ReactNode, useMemo, useState } from 'react';
import { toast } from 'sonner';
import {
  filterNodes,
  groupNodes,
  sortNodes,
} from '@/components/network/network-helpers';
import { NodeRow } from '@/components/network/NodeRow';
import { Button } from '@/components/ui/button';
import { EmptyState } from '@/components/ui/empty-state';
import { Input } from '@/components/ui/input';
import { errorText, formatPairingCode } from '@/lib/network-utils';
import type { NetworkNode } from '@/lib/tauri-api';
import { startNetworkPairing, useNetworkStore } from '@/stores/network-store';

const LISTENER_BANNER = "Others can't connect to you. Allow Rocket in your firewall settings.";
const NO_DISCOVERY = "Automatic discovery isn't available here.";
const EMPTY_TITLE = 'No Rocket nodes found on this network.';
const EMPTY_HINT = 'VPNs and some Wi-Fi networks block discovery.';

interface NetworkPanelProps {
  onAddAddress: () => void;
  onRemove: (node: NetworkNode) => void;
}

function Shell({ children }: { children: ReactNode }) {
  return (
    <div className='flex max-h-[28rem] flex-col text-sm'>
      <div className='flex items-center gap-2 border-b border-border px-3 py-2'>
        <Radar className='h-3.5 w-3.5 text-muted-foreground' aria-hidden='true' />
        <p className='text-[11px] font-semibold uppercase tracking-wider text-muted-foreground'>
          Team network
        </p>
      </div>
      {children}
    </div>
  );
}

interface NodeGroupProps {
  title: string;
  nodes: NetworkNode[];
  pairingDeviceId: string | null;
  onPair: (node: NetworkNode) => void;
  onRemove: (node: NetworkNode) => void;
}

function NodeGroup({ title, nodes, pairingDeviceId, onPair, onRemove }: NodeGroupProps) {
  if (nodes.length === 0) return null;
  return (
    <section className='py-1'>
      <h3 className='px-3 py-1 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground'>
        {`${title} (${nodes.length})`}
      </h3>
      <ul aria-label={title}>
        {nodes.map((node) => (
          <NodeRow
            key={node.deviceId}
            node={node}
            busy={pairingDeviceId === node.deviceId}
            onPair={onPair}
            onRemove={onRemove}
          />
        ))}
      </ul>
    </section>
  );
}

// The body of the Network popover. The host mounts the Add and Remove dialogs.
export function NetworkPanel({ onAddAddress, onRemove }: NetworkPanelProps) {
  const status = useNetworkStore((s) => s.status);
  const nodes = useNetworkStore((s) => s.nodes);
  const incoming = useNetworkStore((s) => s.incoming);
  const [query, setQuery] = useState('');
  const [pairingDeviceId, setPairingDeviceId] = useState<string | null>(null);
  const groups = useMemo(() => groupNodes(sortNodes(filterNodes(nodes, query))), [nodes, query]);

  async function pair(node: NetworkNode) {
    setPairingDeviceId(node.deviceId);
    try {
      await startNetworkPairing(node);
    } catch (err) {
      toast.error(errorText(err));
    } finally {
      setPairingDeviceId(null);
    }
  }

  function review(pairingId: string) {
    const store = useNetworkStore.getState();
    store.openPairingDialog(pairingId);
    store.setPanelOpen(false);
  }

  if (!status) {
    return (
      <Shell>
        <p className='px-3 py-4 text-xs text-muted-foreground'>Loading...</p>
      </Shell>
    );
  }

  if (status.state === 'off') {
    return (
      <Shell>
        <p role='status' className='px-3 py-4 text-xs text-muted-foreground'>
          {status.offReason ?? 'Team network is off.'}
        </p>
      </Shell>
    );
  }

  const shown = groups.paired.length + groups.nearby.length;

  return (
    <Shell>
      {!status.listenerAvailable && (
        <div role='alert' className='border-b border-border bg-warning/10 px-3 py-2 text-xs'>
          {LISTENER_BANNER}
        </div>
      )}
      {!status.discoveryAvailable && (
        <p className='border-b border-border px-3 py-2 text-xs text-muted-foreground'>
          {NO_DISCOVERY}
        </p>
      )}
      <div className='relative border-b border-border p-2'>
        <Search
          className='pointer-events-none absolute left-4 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground'
          aria-hidden='true'
        />
        <Input
          type='search'
          aria-label='Search nodes'
          placeholder='Search by name or ID'
          className='h-8 pl-8 text-xs'
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </div>
      <div className='min-h-0 flex-1 overflow-y-auto'>
        {incoming.length > 0 && (
          <section className='border-b border-border py-1'>
            <h3 className='px-3 py-1 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground'>
              {`Requests (${incoming.length})`}
            </h3>
            <ul aria-label='Requests'>
              {incoming.map((request) => (
                <li key={request.pairingId} className='flex items-center gap-2 px-3 py-1.5'>
                  <div className='min-w-0 flex-1'>
                    <p className='truncate text-xs font-medium'>{request.displayName}</p>
                    <p className='font-mono text-2xs text-muted-foreground'>
                      {`${request.shortId} · ${formatPairingCode(request.code)}`}
                    </p>
                  </div>
                  <Button
                    size='sm'
                    variant='outline'
                    className='h-6 px-2 text-xs'
                    aria-label={`Review request from ${request.displayName} (${request.shortId})`}
                    onClick={() => review(request.pairingId)}
                  >
                    Review
                  </Button>
                </li>
              ))}
            </ul>
          </section>
        )}
        {nodes.length === 0 ? (
          <EmptyState
            cause='none-yet'
            size='compact'
            illustration={false}
            title={EMPTY_TITLE}
            reason={status.discoveryAvailable ? EMPTY_HINT : undefined}
          />
        ) : shown === 0 ? (
          <p className='px-3 py-4 text-xs text-muted-foreground'>
            {`No nodes match "${query.trim()}".`}
          </p>
        ) : (
          <>
            <NodeGroup
              title='Paired'
              nodes={groups.paired}
              pairingDeviceId={pairingDeviceId}
              onPair={(n) => void pair(n)}
              onRemove={onRemove}
            />
            <NodeGroup
              title='Nearby'
              nodes={groups.nearby}
              pairingDeviceId={pairingDeviceId}
              onPair={(n) => void pair(n)}
              onRemove={onRemove}
            />
          </>
        )}
      </div>
      <div className='border-t border-border p-1'>
        <Button
          variant='ghost'
          size='sm'
          className='w-full justify-start gap-2 text-xs'
          onClick={onAddAddress}
        >
          <Plus className='h-3.5 w-3.5' aria-hidden='true' />
          Add by address
        </Button>
      </div>
    </Shell>
  );
}
```

- [ ] **Step 9: Add `src/components/network/AddAddressDialog.tsx`**

```tsx
import { type KeyboardEvent, useEffect, useState } from 'react';
import { toast } from 'sonner';
import { DEFAULT_NETWORK_PORT, validateAddress } from '@/components/network/network-helpers';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { errorText } from '@/lib/network-utils';
import { addNetworkAddress } from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';

interface AddAddressDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

// Adds a node by host and port, for a VPN or a Tailscale name. Pairing still needs the code.
export function AddAddressDialog({ open, onOpenChange }: AddAddressDialogProps) {
  const [host, setHost] = useState('');
  const [port, setPort] = useState(DEFAULT_NETWORK_PORT);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Every opening starts with a clean form.
  useEffect(() => {
    if (!open) return;
    setHost('');
    setPort(DEFAULT_NETWORK_PORT);
    setError(null);
    setBusy(false);
  }, [open]);

  async function submit() {
    if (busy) return;
    const check = validateAddress(host, port);
    if (!check.ok) {
      setError(check.error);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await addNetworkAddress(check.host, check.port);
      void useNetworkStore.getState().refreshNodes();
      toast.success(`Added ${check.host}:${check.port}.`);
      onOpenChange(false);
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  }

  const submitOnEnter = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key !== 'Enter') return;
    e.preventDefault();
    void submit();
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <DialogTitle>Add by address</DialogTitle>
          <DialogDescription>
            Enter the address of a Rocket node on a VPN or another network. You still pair with a
            code.
          </DialogDescription>
        </DialogHeader>
        <div className='grid grid-cols-[1fr_6rem] gap-3'>
          <div className='space-y-1.5'>
            <Label htmlFor='network-address-host'>Host</Label>
            <Input
              id='network-address-host'
              value={host}
              placeholder='10.0.0.5 or mac.tailnet.ts.net'
              onChange={(e) => setHost(e.target.value)}
              onKeyDown={submitOnEnter}
            />
          </div>
          <div className='space-y-1.5'>
            <Label htmlFor='network-address-port'>Port</Label>
            <Input
              id='network-address-port'
              inputMode='numeric'
              value={port}
              onChange={(e) => setPort(e.target.value)}
              onKeyDown={submitOnEnter}
            />
          </div>
        </div>
        {error && (
          <p role='alert' className='text-sm text-destructive'>
            {error}
          </p>
        )}
        <DialogFooter>
          <Button variant='outline' onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button disabled={busy} onClick={() => void submit()}>
            Add
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 10: Add `src/components/network/RemovePairedDialog.tsx`**

```tsx
import { toast } from 'sonner';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { errorText } from '@/lib/network-utils';
import { type NetworkNode, removeNetworkPaired } from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';

interface RemovePairedDialogProps {
  /** The node to remove. Null keeps the dialog closed. */
  node: NetworkNode | null;
  onClose: () => void;
}

async function remove(node: NetworkNode) {
  try {
    await removeNetworkPaired(node.deviceId);
  } catch (err) {
    toast.error(errorText(err));
  }
  // The list is the truth, also after a failure.
  void useNetworkStore.getState().refreshNodes();
}

// Asks before an unpair. The peer is told when it is online.
export function RemovePairedDialog({ node, onClose }: RemovePairedDialogProps) {
  return (
    <AlertDialog
      open={node !== null}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{node ? `Remove ${node.displayName}?` : 'Remove'}</AlertDialogTitle>
          <AlertDialogDescription>
            {node
              ? `${node.displayName} (${node.shortId}) will no longer be paired with you. You can pair again later with a new code.`
              : ''}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction
            onClick={() => {
              if (node) void remove(node);
            }}
          >
            Remove
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
```

- [ ] **Step 11: Add `src/components/status-bar/NetworkStatusItem.tsx`**

```tsx
import { Radar } from 'lucide-react';
import { useState } from 'react';
import { AddAddressDialog } from '@/components/network/AddAddressDialog';
import { NetworkPanel } from '@/components/network/NetworkPanel';
import { RemovePairedDialog } from '@/components/network/RemovePairedDialog';
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import type { NetworkNode } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { selectOnlinePairedCount, useNetworkStore } from '@/stores/network-store';

/**
 * Status bar button with the number of online paired nodes. It opens the Network panel.
 * The Add and Remove dialogs live here, outside the Popover, so closing the panel to open
 * one of them does not unmount it.
 */
export function NetworkStatusItem() {
  const online = useNetworkStore(selectOnlinePairedCount);
  const off = useNetworkStore((s) => s.status?.state === 'off');
  const open = useNetworkStore((s) => s.panelOpen);
  const setOpen = useNetworkStore((s) => s.setPanelOpen);
  const [addOpen, setAddOpen] = useState(false);
  const [removeTarget, setRemoveTarget] = useState<NetworkNode | null>(null);
  const label = off ? 'Team network, off' : `Team network, ${online} online`;

  function handleOpenChange(next: boolean) {
    setOpen(next);
    // The panel always shows fresh data, also when an event was missed.
    if (next) {
      void useNetworkStore.getState().refresh();
      void useNetworkStore.getState().refreshNodes();
    }
  }

  return (
    <div className='flex h-5 items-center border-x border-statusbar-border px-1'>
      <Popover open={open} onOpenChange={handleOpenChange}>
        <PopoverTrigger asChild>
          <Button
            variant='ghost'
            size='sm'
            className='h-5 gap-1.25 rounded-sm px-1 text-[11px] text-muted-foreground transition-colors hover:bg-statusbar-item-hover hover:text-foreground'
            title='Team network'
            aria-label={label}
          >
            <Radar className={cn('w-2.75 h-2.75', off && 'opacity-50')} aria-hidden='true' />
            {online > 0 && (
              <span className='text-2xs px-1 rounded-full bg-muted text-muted-foreground'>
                {online}
              </span>
            )}
          </Button>
        </PopoverTrigger>
        <PopoverContent side='top' align='start' className='w-80 overflow-hidden p-0'>
          <NetworkPanel
            onAddAddress={() => {
              setOpen(false);
              setAddOpen(true);
            }}
            onRemove={(node) => {
              setOpen(false);
              setRemoveTarget(node);
            }}
          />
        </PopoverContent>
      </Popover>
      <AddAddressDialog open={addOpen} onOpenChange={setAddOpen} />
      <RemovePairedDialog node={removeTarget} onClose={() => setRemoveTarget(null)} />
    </div>
  );
}
```

- [ ] **Step 12: Render the item in `src/components/layout/StatusBar.tsx`**

Edit 1: after the quoted line `import { ContractsStatusItem } from '@/components/status-bar/ContractsStatusItem';` add:

```tsx
import { NetworkStatusItem } from '@/components/status-bar/NetworkStatusItem';
```

Edit 2: after the quoted line `      <ContractsStatusItem />` add:

```tsx
      <NetworkStatusItem />
```

- [ ] **Step 13: Run the tests to verify they pass**

Run: `bunx vitest run src/components/network src/components/status-bar/__tests__/NetworkStatusItem.test.tsx src/components/layout/__tests__/StatusBar.test.tsx`
Expected: PASS.

- [ ] **Step 14: Run the checks**

Run: `bun run tsc --noEmit && bun run check`
Expected: no errors. If `check` reports format or import order, run `bun run lint`, then `bun run check` again.

- [ ] **Step 15: No commit**

No commit; reviewed at the plan end.

---

