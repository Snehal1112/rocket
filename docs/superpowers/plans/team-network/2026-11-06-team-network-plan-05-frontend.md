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

### Task 3: Pairing dialog, incoming notice, intro card, Settings section and docs

**Files:**
- Create: `src/components/network/PairingDialog.tsx`, `src/components/network/IncomingPairingToast.tsx`, `src/hooks/useIncomingPairingNotice.tsx`, `src/components/network/NetworkIntroCard.tsx`, `src/components/settings/sections/NetworkSection.tsx`, `docs/features/team-network.md`, `docs/manual-tests/team-network.md`
- Modify: `src/components/network/network-helpers.ts`, `src/components/network/__tests__/network-helpers.test.ts`, `src/stores/settings-store.ts`, `src/components/settings/settings-sections.ts`, `src/components/settings/__tests__/SettingsPage.test.tsx`, `src/App.tsx`, `docs/features/settings-page.md`, `CLAUDE.md`, `docs/roadmaps/roadmap.md`, `docs/roadmaps/contracts-ci-ai-roadmap.md`
- Test: `src/components/network/__tests__/PairingDialog.test.tsx`, `src/components/network/__tests__/IncomingPairingToast.test.tsx`, `src/hooks/__tests__/useIncomingPairingNotice.test.tsx`, `src/components/network/__tests__/NetworkIntroCard.test.tsx`, `src/components/settings/sections/__tests__/NetworkSection.test.tsx`

**Interfaces:**
- Consumes: Task 1 store, bridge, wrappers and fixtures; Task 2 `network-helpers.ts` (this task adds to it).
- Produces:

```ts
// network-helpers.ts (added)
export const DISPLAY_NAME_MAX_CHARS = 40;
export type NameCheck = { ok: true; name: string } | { ok: false; error: string };
export function validateDisplayName(input: string): NameCheck;   // trimmed, 1..40 characters (code points), no control characters

// PairingDialog.tsx
export function PairingDialog(): JSX.Element | null;             // Mounted once in App. Open while dialogPairingId is set.

// IncomingPairingToast.tsx
export const incomingToastId: (pairingId: string) => string;     // `network-pairing-<id>`
export function IncomingPairingToast(props: { pairingId: string; displayName: string; shortId: string }): JSX.Element;

// useIncomingPairingNotice.tsx
export function useIncomingPairingNotice(): void;                // One toast per open request, fixed id, dismissed when the request goes.

// NetworkIntroCard.tsx
export function NetworkIntroCard(): JSX.Element | null;

// NetworkSection.tsx
export function NetworkSection(): JSX.Element;
```

- [ ] **Step 1: Write the failing helper tests**

In `src/components/network/__tests__/network-helpers.test.ts`, change the import block at the top to also import `validateDisplayName`:

```ts
import {
  filterNodes,
  groupNodes,
  sortNodes,
  validateAddress,
  validateDisplayName,
} from '@/components/network/network-helpers';
```

and append at the end of the file:

```ts

describe('validateDisplayName', () => {
  it('trims the name', () => {
    expect(validateDisplayName('  Ana Lima ')).toEqual({ ok: true, name: 'Ana Lima' });
  });

  it('refuses an empty or blank name', () => {
    expect(validateDisplayName('')).toEqual({ ok: false, error: 'Enter a name.' });
    expect(validateDisplayName('   ')).toEqual({ ok: false, error: 'Enter a name.' });
  });

  it('accepts 40 characters and refuses 41', () => {
    expect(validateDisplayName('a'.repeat(40))).toMatchObject({ ok: true });
    expect(validateDisplayName('a'.repeat(41))).toEqual({
      ok: false,
      error: 'Use 40 characters or fewer.',
    });
  });

  it('counts an emoji as one character, like the backend', () => {
    expect(validateDisplayName('😀'.repeat(40))).toMatchObject({ ok: true });
    expect(validateDisplayName('😀'.repeat(41))).toMatchObject({ ok: false });
  });

  it('refuses control characters', () => {
    expect(validateDisplayName('Ana\tLima')).toEqual({
      ok: false,
      error: 'The name cannot contain control characters.',
    });
  });
});
```

- [ ] **Step 2: Write the failing pairing dialog tests**

Create `src/components/network/__tests__/PairingDialog.test.tsx`:

```tsx
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PairingDialog } from '@/components/network/PairingDialog';
import {
  type ActivePairing,
  type PairingIncoming,
  useNetworkStore,
} from '@/stores/network-store';
import { createDeferred } from '@/test/deferred';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
  api: {
    respondNetworkPairing: vi.fn(),
    cancelNetworkPairing: vi.fn(),
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
    requestNetworkPairing: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const store = () => useNetworkStore.getState();

const started = (patch: Partial<ActivePairing> = {}): ActivePairing => ({
  pairingId: 'p1',
  deviceId: 'DEV1',
  shortId: 'AAAA-1111',
  displayName: 'Ben Ortiz',
  code: '482913',
  phase: 'waiting',
  ...patch,
});

const request = (patch: Partial<PairingIncoming> = {}): PairingIncoming => ({
  pairingId: 'p1',
  deviceId: 'DEV2',
  shortId: 'DDDD-4444',
  displayName: 'Dana Roy',
  code: '048213',
  accepted: false,
  ...patch,
});

function openStarted(patch: Partial<ActivePairing> = {}) {
  useNetworkStore.setState({ activePairing: started(patch), dialogPairingId: 'p1' });
  render(<PairingDialog />);
}

function openRequest(patch: Partial<PairingIncoming> = {}) {
  store().addIncoming(request(patch));
  useNetworkStore.setState({ dialogPairingId: 'p1' });
  render(<PairingDialog />);
}

describe('PairingDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
    mocks.api.respondNetworkPairing.mockResolvedValue(undefined);
    mocks.api.cancelNetworkPairing.mockResolvedValue(undefined);
  });

  it('renders nothing while no dialog is open', () => {
    render(<PairingDialog />);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  describe('started by this node', () => {
    it('shows the name, the code with a space, the short id and the buttons', async () => {
      openStarted();
      expect(await screen.findByRole('dialog', { name: 'Pair with Ben Ortiz' })).toBeInTheDocument();
      expect(screen.getByText('482 913')).toBeInTheDocument();
      expect(screen.getByText('Device ID AAAA-1111')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Accept' })).toBeEnabled();
      expect(screen.getByRole('button', { name: 'Cancel' })).toBeEnabled();
    });

    it('Accept sends the decision and waits for the peer', async () => {
      openStarted();
      await userEvent.click(await screen.findByRole('button', { name: 'Accept' }));
      await waitFor(() =>
        expect(mocks.api.respondNetworkPairing).toHaveBeenCalledWith('p1', 'accept'),
      );
      expect(await screen.findByText('Waiting for Ben Ortiz...')).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Accept' })).not.toBeInTheDocument();
      expect(store().activePairing?.phase).toBe('done');
    });

    it('Cancel cancels the pairing and closes the dialog', async () => {
      openStarted();
      await userEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
      await waitFor(() => expect(mocks.api.cancelNetworkPairing).toHaveBeenCalledWith('p1'));
      await waitFor(() => expect(store().dialogPairingId).toBeNull());
      expect(store().activePairing).toBeNull();
    });

    it('closing a dialog that still waits for the user cancels the pairing', async () => {
      openStarted();
      await screen.findByRole('dialog');
      await userEvent.keyboard('{Escape}');
      await waitFor(() => expect(mocks.api.cancelNetworkPairing).toHaveBeenCalledWith('p1'));
    });

    it('closing after Accept only hides the dialog', async () => {
      openStarted({ phase: 'done' });
      await screen.findByRole('dialog');
      await userEvent.keyboard('{Escape}');
      await waitFor(() => expect(store().dialogPairingId).toBeNull());
      expect(mocks.api.cancelNetworkPairing).not.toHaveBeenCalled();
      expect(store().activePairing?.phase).toBe('done');
    });

    it('does not bring a pairing back when it finished during the call', async () => {
      const pending = createDeferred<void>();
      mocks.api.respondNetworkPairing.mockReturnValue(pending.promise);
      openStarted();
      await userEvent.click(await screen.findByRole('button', { name: 'Accept' }));
      act(() => {
        store().setActivePairing(null);
        store().closePairingDialog();
      });
      pending.resolve();
      await waitFor(() => expect(mocks.api.respondNetworkPairing).toHaveBeenCalled());
      expect(store().activePairing).toBeNull();
    });

    it('shows the error and keeps the buttons when the decision fails', async () => {
      mocks.api.respondNetworkPairing.mockRejectedValue('Internal error: link down');
      openStarted();
      await userEvent.click(await screen.findByRole('button', { name: 'Accept' }));
      expect(await screen.findByRole('alert')).toHaveTextContent('link down');
      expect(screen.getByRole('button', { name: 'Accept' })).toBeEnabled();
      expect(store().activePairing?.phase).toBe('waiting');
    });
  });

  describe('request from another node', () => {
    it('shows the name, the code with its leading zero and three buttons', async () => {
      openRequest();
      expect(await screen.findByRole('dialog', { name: 'Dana Roy wants to pair' })).toBeInTheDocument();
      expect(screen.getByText('048 213')).toBeInTheDocument();
      expect(screen.getByText('Device ID DDDD-4444')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Accept' })).toBeEnabled();
      expect(screen.getByRole('button', { name: 'Decline' })).toBeEnabled();
      expect(screen.getByRole('button', { name: 'Block' })).toBeEnabled();
    });

    it('Accept waits for the peer and keeps the request', async () => {
      openRequest();
      await userEvent.click(await screen.findByRole('button', { name: 'Accept' }));
      await waitFor(() =>
        expect(mocks.api.respondNetworkPairing).toHaveBeenCalledWith('p1', 'accept'),
      );
      expect(await screen.findByText('Waiting for Dana Roy...')).toBeInTheDocument();
      expect(store().incoming[0]?.accepted).toBe(true);
    });

    it('Decline answers, removes the request and closes the dialog', async () => {
      openRequest();
      await userEvent.click(await screen.findByRole('button', { name: 'Decline' }));
      await waitFor(() =>
        expect(mocks.api.respondNetworkPairing).toHaveBeenCalledWith('p1', 'decline'),
      );
      await waitFor(() => expect(store().dialogPairingId).toBeNull());
      expect(store().incoming).toEqual([]);
    });

    it('Block answers with block and says so', async () => {
      openRequest();
      await userEvent.click(await screen.findByRole('button', { name: 'Block' }));
      await waitFor(() =>
        expect(mocks.api.respondNetworkPairing).toHaveBeenCalledWith('p1', 'block'),
      );
      expect(mocks.toast.info).toHaveBeenCalledWith('Dana Roy is blocked.');
      expect(store().incoming).toEqual([]);
    });

    it('switches to "The request expired." when the request goes while open', async () => {
      openRequest();
      await screen.findByRole('dialog', { name: 'Dana Roy wants to pair' });
      act(() => store().removeIncoming('p1'));
      expect(await screen.findByText('The request expired.')).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Accept' })).not.toBeInTheDocument();
      expect(screen.queryByText('048 213')).not.toBeInTheDocument();
      await userEvent.click(screen.getByRole('button', { name: 'Done' }));
      expect(store().dialogPairingId).toBeNull();
    });
  });
});
```

- [ ] **Step 3: Write the failing notice tests**

Create `src/components/network/__tests__/IncomingPairingToast.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { IncomingPairingToast } from '@/components/network/IncomingPairingToast';
import { useNetworkStore } from '@/stores/network-store';

const mocks = vi.hoisted(() => ({
  toast: { dismiss: vi.fn(), custom: vi.fn() },
  api: { getNetworkStatus: vi.fn(), listNetworkNodes: vi.fn(), requestNetworkPairing: vi.fn() },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

function setup() {
  render(<IncomingPairingToast pairingId='p1' displayName='Dana Roy' shortId='DDDD-4444' />);
}

describe('IncomingPairingToast', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
  });

  it('names the sender and its short id, but not the code', () => {
    setup();
    expect(screen.getByText('Dana Roy wants to pair')).toBeInTheDocument();
    expect(screen.getByText('DDDD-4444')).toBeInTheDocument();
  });

  it('Review opens the pairing dialog and dismisses the toast', async () => {
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'Review' }));
    expect(useNetworkStore.getState().dialogPairingId).toBe('p1');
    expect(mocks.toast.dismiss).toHaveBeenCalledWith('network-pairing-p1');
  });

  it('Later only dismisses the toast', async () => {
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'Later' }));
    expect(useNetworkStore.getState().dialogPairingId).toBeNull();
    expect(mocks.toast.dismiss).toHaveBeenCalledWith('network-pairing-p1');
  });
});
```

Create `src/hooks/__tests__/useIncomingPairingNotice.test.tsx`:

```tsx
import { renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useIncomingPairingNotice } from '@/hooks/useIncomingPairingNotice';
import {
  INCOMING_TTL_MS,
  type PairingIncoming,
  useNetworkStore,
} from '@/stores/network-store';

const mocks = vi.hoisted(() => ({
  toast: { custom: vi.fn(), dismiss: vi.fn() },
  api: { getNetworkStatus: vi.fn(), listNetworkNodes: vi.fn(), requestNetworkPairing: vi.fn() },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const request = (pairingId: string): PairingIncoming => ({
  pairingId,
  deviceId: `DEV-${pairingId}`,
  shortId: 'DDDD-4444',
  displayName: 'Dana Roy',
  code: '482913',
  accepted: false,
});

describe('useIncomingPairingNotice', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
  });

  it('shows one toast with a fixed id and the request lifetime', () => {
    renderHook(() => useIncomingPairingNotice());
    useNetworkStore.getState().addIncoming(request('p1'));
    expect(mocks.toast.custom).toHaveBeenCalledTimes(1);
    expect(mocks.toast.custom).toHaveBeenCalledWith(expect.any(Function), {
      id: 'network-pairing-p1',
      duration: INCOMING_TTL_MS,
    });
  });

  it('shows no second toast for a duplicate event', () => {
    renderHook(() => useIncomingPairingNotice());
    useNetworkStore.getState().addIncoming(request('p1'));
    useNetworkStore.getState().addIncoming(request('p1'));
    expect(mocks.toast.custom).toHaveBeenCalledTimes(1);
  });

  it('shows one toast per request, with different ids', () => {
    renderHook(() => useIncomingPairingNotice());
    useNetworkStore.getState().addIncoming(request('p1'));
    useNetworkStore.getState().addIncoming(request('p2'));
    const ids = mocks.toast.custom.mock.calls.map((call) => (call[1] as { id: string }).id);
    expect(ids).toEqual(['network-pairing-p1', 'network-pairing-p2']);
  });

  it('dismisses the toast when the request goes away', () => {
    renderHook(() => useIncomingPairingNotice());
    useNetworkStore.getState().addIncoming(request('p1'));
    useNetworkStore.getState().removeIncoming('p1');
    expect(mocks.toast.dismiss).toHaveBeenCalledWith('network-pairing-p1');
  });

  it('does not react to changes that leave the requests alone', () => {
    renderHook(() => useIncomingPairingNotice());
    useNetworkStore.setState({ panelOpen: true });
    expect(mocks.toast.custom).not.toHaveBeenCalled();
    expect(mocks.toast.dismiss).not.toHaveBeenCalled();
  });

  it('shows a toast for a request that exists at mount', () => {
    useNetworkStore.getState().addIncoming(request('p1'));
    renderHook(() => useIncomingPairingNotice());
    expect(mocks.toast.custom).toHaveBeenCalledTimes(1);
  });

  it('stops listening after unmount', () => {
    const view = renderHook(() => useIncomingPairingNotice());
    view.unmount();
    useNetworkStore.getState().addIncoming(request('p1'));
    expect(mocks.toast.custom).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 4: Write the failing intro card tests**

Create `src/components/network/__tests__/NetworkIntroCard.test.tsx`:

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { NetworkIntroCard } from '@/components/network/NetworkIntroCard';
import { useNetworkStore } from '@/stores/network-store';
import { useSettingsStore } from '@/stores/settings-store';
import { makeStatus } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
  api: {
    markNetworkIntroSeen: vi.fn(),
    setNetworkVisibility: vi.fn(),
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
    requestNetworkPairing: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const INTRO = "You're visible on this network as 'Ana Lima'.";

function setup(patch = {}) {
  useNetworkStore.setState({ status: makeStatus({ introSeen: false, ...patch }) });
  render(<NetworkIntroCard />);
}

describe('NetworkIntroCard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
    useSettingsStore.getState().reset();
    mocks.api.markNetworkIntroSeen.mockResolvedValue(undefined);
    mocks.api.setNetworkVisibility.mockResolvedValue(undefined);
    mocks.api.getNetworkStatus.mockResolvedValue(makeStatus({ introSeen: true }));
  });

  it('says who the user is visible as', () => {
    setup();
    expect(screen.getByText(INTRO)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Edit name' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Hide my node' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'OK' })).toBeInTheDocument();
  });

  it.each([
    ['the intro was seen', { introSeen: true }],
    ['the node is hidden', { hidden: true }],
    ['networking is off', { state: 'off' as const, offReason: 'No keychain.' }],
  ])('stays away when %s', (_label, patch) => {
    setup(patch);
    expect(screen.queryByText(INTRO)).not.toBeInTheDocument();
  });

  it('stays away until the status is loaded', () => {
    render(<NetworkIntroCard />);
    expect(screen.queryByText(INTRO)).not.toBeInTheDocument();
  });

  it('OK marks the intro as seen once and hides the card at once', async () => {
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'OK' }));
    expect(screen.queryByText(INTRO)).not.toBeInTheDocument();
    await waitFor(() => expect(mocks.api.markNetworkIntroSeen).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(mocks.api.getNetworkStatus).toHaveBeenCalled());
    expect(useNetworkStore.getState().status?.introSeen).toBe(true);
  });

  it('Edit name opens the Network section of Settings and marks the intro as seen', async () => {
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'Edit name' }));
    expect(useSettingsStore.getState().open).toBe(true);
    expect(useSettingsStore.getState().section).toBe('network');
    await waitFor(() => expect(mocks.api.markNetworkIntroSeen).toHaveBeenCalledTimes(1));
  });

  it('Hide my node hides the node and marks the intro as seen', async () => {
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'Hide my node' }));
    await waitFor(() => expect(mocks.api.setNetworkVisibility).toHaveBeenCalledWith(true));
    await waitFor(() => expect(mocks.api.markNetworkIntroSeen).toHaveBeenCalledTimes(1));
    expect(screen.queryByText(INTRO)).not.toBeInTheDocument();
  });

  it('keeps the card and shows the error when hiding fails', async () => {
    mocks.api.setNetworkVisibility.mockRejectedValue('Internal error: settings locked');
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'Hide my node' }));
    await waitFor(() => expect(mocks.toast.error).toHaveBeenCalledWith('settings locked'));
    expect(mocks.api.markNetworkIntroSeen).not.toHaveBeenCalled();
    expect(screen.getByText(INTRO)).toBeInTheDocument();
  });

  it('hides the card for this session even when saving the flag fails', async () => {
    mocks.api.markNetworkIntroSeen.mockRejectedValue('Internal error: disk full');
    const log = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'OK' }));
    await waitFor(() => expect(log).toHaveBeenCalled());
    expect(screen.queryByText(INTRO)).not.toBeInTheDocument();
    log.mockRestore();
  });
});
```

- [ ] **Step 5: Write the failing Settings section tests**

Create `src/components/settings/sections/__tests__/NetworkSection.test.tsx`:

```tsx
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { NetworkSection } from '@/components/settings/sections/NetworkSection';
import type { NetworkStatus } from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';
import { makeStatus } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
  api: {
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
    requestNetworkPairing: vi.fn(),
    setNetworkVisibility: vi.fn(),
    setNetworkProfile: vi.fn(),
    listNetworkBlocked: vi.fn(),
    listNetworkAddresses: vi.fn(),
    unblockNetworkDevice: vi.fn(),
    removeNetworkAddress: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const mallory = {
  deviceId: 'B1',
  shortId: 'BBBB-0001',
  name: 'Mallory',
  blockedAt: '2026-11-06T10:00:00Z',
};
const saved = { host: '10.0.0.5', port: 47821, addedAt: '2026-11-06T10:00:00Z' };

function setup(status: NetworkStatus = makeStatus()) {
  mocks.api.getNetworkStatus.mockResolvedValue(status);
  useNetworkStore.setState({ status });
  render(<NetworkSection />);
}

describe('NetworkSection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
    mocks.api.listNetworkBlocked.mockResolvedValue([mallory]);
    mocks.api.listNetworkAddresses.mockResolvedValue([saved]);
    mocks.api.listNetworkNodes.mockResolvedValue([]);
    mocks.api.setNetworkVisibility.mockResolvedValue(undefined);
    mocks.api.setNetworkProfile.mockResolvedValue(undefined);
    mocks.api.unblockNetworkDevice.mockResolvedValue(undefined);
    mocks.api.removeNetworkAddress.mockResolvedValue(undefined);
  });

  it('shows the port in use and the short device id as read-only text', async () => {
    setup();
    expect(screen.getByRole('heading', { name: 'Team network' })).toBeInTheDocument();
    expect(screen.getByText('47821')).toBeInTheDocument();
    expect(screen.getByText('K7QF-2M9X')).toBeInTheDocument();
  });

  it('says there is no port when the node is not listening', () => {
    setup(makeStatus({ port: null, deviceShortId: null }));
    expect(screen.getByText('Not listening')).toBeInTheDocument();
  });

  it('shows why networking is off', () => {
    setup(makeStatus({ state: 'off', offReason: 'The keychain is not available.' }));
    expect(screen.getByRole('alert')).toHaveTextContent('The keychain is not available.');
  });

  it('lists the warnings', () => {
    setup(makeStatus({ warnings: ['blocked.yml was damaged and was reset.'] }));
    expect(screen.getByText('blocked.yml was damaged and was reset.')).toBeInTheDocument();
  });

  it('hides the node with the switch and reloads the status', async () => {
    mocks.api.getNetworkStatus
      .mockResolvedValueOnce(makeStatus())
      .mockResolvedValueOnce(makeStatus({ hidden: true }));
    useNetworkStore.setState({ status: makeStatus() });
    render(<NetworkSection />);
    const toggle = screen.getByRole('switch', { name: 'Hide my node' });
    expect(toggle).not.toBeChecked();
    await userEvent.click(toggle);
    await waitFor(() => expect(mocks.api.setNetworkVisibility).toHaveBeenCalledWith(true));
    await waitFor(() => expect(screen.getByRole('switch', { name: 'Hide my node' })).toBeChecked());
  });

  it('shows the error and keeps the switch when hiding fails', async () => {
    mocks.api.setNetworkVisibility.mockRejectedValue('Internal error: settings locked');
    setup();
    await userEvent.click(screen.getByRole('switch', { name: 'Hide my node' }));
    await waitFor(() => expect(mocks.toast.error).toHaveBeenCalledWith('settings locked'));
    expect(screen.getByRole('switch', { name: 'Hide my node' })).not.toBeChecked();
  });

  it('saves a new display name', async () => {
    setup();
    const field = screen.getByLabelText('Display name');
    await userEvent.clear(field);
    await userEvent.type(field, '  Ana L.  ');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(mocks.api.setNetworkProfile).toHaveBeenCalledWith('Ana L.'));
    await waitFor(() => expect(mocks.toast.success).toHaveBeenCalledWith('Name saved.'));
  });

  it('keeps Save off while the name is unchanged', () => {
    setup();
    expect(screen.getByLabelText('Display name')).toHaveValue('Ana Lima');
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
  });

  it('refuses an empty name and a name over 40 characters', async () => {
    setup();
    const field = screen.getByLabelText('Display name');
    await userEvent.clear(field);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(screen.getByText('Enter a name.')).toBeInTheDocument();
    await userEvent.type(field, 'a'.repeat(41));
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(screen.getByText('Use 40 characters or fewer.')).toBeInTheDocument();
    expect(mocks.api.setNetworkProfile).not.toHaveBeenCalled();
  });

  it('lists blocked devices and unblocks one', async () => {
    setup();
    const row = (await screen.findByText('Mallory')).closest('li');
    if (!row) throw new Error('no blocked row');
    expect(within(row).getByText('BBBB-0001')).toBeInTheDocument();
    await userEvent.click(within(row).getByRole('button', { name: 'Unblock Mallory' }));
    await waitFor(() => expect(mocks.api.unblockNetworkDevice).toHaveBeenCalledWith('B1'));
    await waitFor(() => expect(screen.queryByText('Mallory')).not.toBeInTheDocument());
    expect(screen.getByText('No blocked devices.')).toBeInTheDocument();
  });

  it('lists saved addresses and removes one', async () => {
    setup();
    expect(await screen.findByText('10.0.0.5:47821')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Remove 10.0.0.5:47821' }));
    await waitFor(() =>
      expect(mocks.api.removeNetworkAddress).toHaveBeenCalledWith('10.0.0.5', 47821),
    );
    await waitFor(() => expect(screen.queryByText('10.0.0.5:47821')).not.toBeInTheDocument());
    expect(screen.getByText('No saved addresses.')).toBeInTheDocument();
    expect(mocks.api.listNetworkNodes).toHaveBeenCalled();
  });

  it('shows the error when the lists cannot load', async () => {
    mocks.api.listNetworkBlocked.mockRejectedValue('Internal error: repo down');
    setup();
    expect(await screen.findByText('repo down')).toBeInTheDocument();
  });
});
```

In `src/components/settings/__tests__/SettingsPage.test.tsx`, replace the quoted line

```tsx
    expect(labels).toEqual(['Proxy', 'Secret Managers', 'AI Agents', 'Support', 'About']);
```

with:

```tsx
    expect(labels).toEqual([
      'Proxy',
      'Secret Managers',
      'AI Agents',
      'Team network',
      'Support',
      'About',
    ]);
```

- [ ] **Step 6: Run the tests to verify they fail**

Run: `bunx vitest run src/components/network src/hooks/__tests__/useIncomingPairingNotice.test.tsx src/components/settings`
Expected: FAIL. `network-helpers.test.ts` reports `validateDisplayName is not a function`; the other new files report `Failed to resolve import` for `PairingDialog`, `IncomingPairingToast`, `useIncomingPairingNotice`, `NetworkIntroCard` and `NetworkSection`; `SettingsPage.test.tsx` reports a list mismatch (`'Team network'` missing).

- [ ] **Step 7: Add `validateDisplayName` to `src/components/network/network-helpers.ts`**

Append at the end of the file:

```ts

export const DISPLAY_NAME_MAX_CHARS = 40;

export type NameCheck = { ok: true; name: string } | { ok: false; error: string };

// Counts code points, like the backend counts chars, so an emoji is one character.
const hasControlChar = (text: string): boolean =>
  [...text].some((char) => {
    const code = char.codePointAt(0) ?? 0;
    return code < 32 || code === 127;
  });

/** Checks the display name before it goes to the backend. */
export function validateDisplayName(input: string): NameCheck {
  const name = input.trim();
  const length = [...name].length;
  if (length === 0) return { ok: false, error: 'Enter a name.' };
  if (length > DISPLAY_NAME_MAX_CHARS) {
    return { ok: false, error: `Use ${DISPLAY_NAME_MAX_CHARS} characters or fewer.` };
  }
  if (hasControlChar(name)) {
    return { ok: false, error: 'The name cannot contain control characters.' };
  }
  return { ok: true, name };
}
```

- [ ] **Step 8: Add `src/components/network/PairingDialog.tsx`**

```tsx
import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { errorText, formatPairingCode } from '@/lib/network-utils';
import { cancelNetworkPairing, respondNetworkPairing } from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';

const store = () => useNetworkStore.getState();

/**
 * The pairing dialog, for both roles. The node that started the pairing sees Accept and
 * Cancel. A node that received a request sees Accept, Decline and Block. Both compare the
 * same 6-digit code. Mount it once, outside the Network popover.
 */
export function PairingDialog() {
  const id = useNetworkStore((s) => s.dialogPairingId);
  const active = useNetworkStore((s) => s.activePairing);
  const request = useNetworkStore((s) => s.incoming.find((i) => i.pairingId === s.dialogPairingId));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Another pairing starts with a clean error.
  useEffect(() => {
    setError(null);
    setBusy(false);
  }, [id]);

  if (!id) return null;

  const started = active && active.pairingId === id ? active : null;
  const view = started ?? request ?? null;
  const done = () => store().closePairingDialog();

  async function run(action: () => Promise<void>) {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  }

  const acceptStarted = () =>
    run(async () => {
      await respondNetworkPairing(id, 'accept');
      // The pairing may have finished while the call was in flight. Then it stays gone.
      const current = store().activePairing;
      if (current?.pairingId === id) store().setActivePairing({ ...current, phase: 'done' });
    });

  const cancelStarted = () =>
    run(async () => {
      await cancelNetworkPairing(id);
      store().setActivePairing(null);
      store().closePairingDialog();
    });

  const acceptRequest = () =>
    run(async () => {
      await respondNetworkPairing(id, 'accept');
      store().markIncomingAccepted(id);
    });

  const answerRequest = (decision: 'decline' | 'block', name: string) =>
    run(async () => {
      await respondNetworkPairing(id, decision);
      store().removeIncoming(id);
      store().closePairingDialog();
      if (decision === 'block') toast.info(`${name} is blocked.`);
    });

  // Closing a dialog that still waits for the user's choice ends the pairing it started.
  function handleOpenChange(next: boolean) {
    if (next || busy) return;
    if (started && started.phase === 'waiting') {
      void cancelStarted();
      return;
    }
    done();
  }

  // The request is gone: it expired, or another event finished it.
  if (!view) {
    return (
      <Dialog open onOpenChange={handleOpenChange}>
        <DialogContent className='sm:max-w-sm'>
          <DialogHeader>
            <DialogTitle>Pairing request</DialogTitle>
            <DialogDescription>The request expired.</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button onClick={done}>Done</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }

  const name = view.displayName;
  const waitingForPeer = started ? started.phase === 'done' : (request?.accepted ?? false);

  return (
    <Dialog open onOpenChange={handleOpenChange}>
      <DialogContent className='sm:max-w-sm'>
        <DialogHeader>
          <DialogTitle className='break-words'>
            {started ? `Pair with ${name}` : `${name} wants to pair`}
          </DialogTitle>
          <DialogDescription>
            {`Check that ${name} sees the same code, then accept.`}
          </DialogDescription>
        </DialogHeader>
        <div role='group' aria-label='Pairing code' className='flex flex-col items-center gap-1 py-2'>
          <p className='font-mono text-4xl font-semibold tracking-[0.2em]'>
            {formatPairingCode(view.code)}
          </p>
          <p className='font-mono text-xs text-muted-foreground'>{`Device ID ${view.shortId}`}</p>
        </div>
        {waitingForPeer && (
          <p role='status' className='text-center text-sm text-muted-foreground'>
            {`Waiting for ${name}...`}
          </p>
        )}
        {error && (
          <p role='alert' className='text-sm text-destructive'>
            {error}
          </p>
        )}
        <DialogFooter>
          {waitingForPeer ? (
            <Button variant='outline' onClick={done}>
              Done
            </Button>
          ) : started ? (
            <>
              <Button variant='outline' disabled={busy} onClick={() => void cancelStarted()}>
                Cancel
              </Button>
              <Button disabled={busy} onClick={() => void acceptStarted()}>
                Accept
              </Button>
            </>
          ) : (
            <>
              <Button
                variant='ghost'
                className='text-destructive hover:text-destructive'
                disabled={busy}
                onClick={() => void answerRequest('block', name)}
              >
                Block
              </Button>
              <Button
                variant='outline'
                disabled={busy}
                onClick={() => void answerRequest('decline', name)}
              >
                Decline
              </Button>
              <Button disabled={busy} onClick={() => void acceptRequest()}>
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

- [ ] **Step 9: Add the incoming notice**

Create `src/components/network/IncomingPairingToast.tsx`:

```tsx
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { useNetworkStore } from '@/stores/network-store';

/** One fixed toast id per pairing, so a repeated event updates the toast in place. */
export const incomingToastId = (pairingId: string): string => `network-pairing-${pairingId}`;

interface IncomingPairingToastProps {
  pairingId: string;
  displayName: string;
  shortId: string;
}

// The code is not in the toast. It shows in the dialog the user opens.
export function IncomingPairingToast({
  pairingId,
  displayName,
  shortId,
}: IncomingPairingToastProps) {
  const dismiss = () => toast.dismiss(incomingToastId(pairingId));

  return (
    <div className='flex w-[356px] max-w-full flex-col gap-2 rounded-md border bg-popover p-4 text-popover-foreground shadow-lg'>
      <p className='break-words text-sm font-medium'>{`${displayName} wants to pair`}</p>
      <p className='font-mono text-xs text-muted-foreground'>{shortId}</p>
      <div className='flex flex-wrap items-center gap-2'>
        <Button
          size='sm'
          onClick={() => {
            useNetworkStore.getState().openPairingDialog(pairingId);
            dismiss();
          }}
        >
          Review
        </Button>
        <Button size='sm' variant='ghost' onClick={dismiss}>
          Later
        </Button>
      </div>
    </div>
  );
}
```

Create `src/hooks/useIncomingPairingNotice.tsx`:

```tsx
import { useEffect } from 'react';
import { toast } from 'sonner';
import { IncomingPairingToast, incomingToastId } from '@/components/network/IncomingPairingToast';
import { INCOMING_TTL_MS, type PairingIncoming, useNetworkStore } from '@/stores/network-store';

function showToast(request: PairingIncoming): void {
  toast.custom(
    () => (
      <IncomingPairingToast
        pairingId={request.pairingId}
        displayName={request.displayName}
        shortId={request.shortId}
      />
    ),
    { id: incomingToastId(request.pairingId), duration: INCOMING_TTL_MS },
  );
}

// Shows one toast for each open pairing request and removes it when the request goes.
// The store drops a repeated event, so a duplicate never reaches this hook. Mount once.
export function useIncomingPairingNotice(): void {
  useEffect(() => {
    // A hot reload keeps the store, so requests that already exist get their toast too.
    for (const request of useNetworkStore.getState().incoming) showToast(request);

    return useNetworkStore.subscribe((state, prev) => {
      if (state.incoming === prev.incoming) return;
      for (const request of state.incoming) {
        if (!prev.incoming.some((p) => p.pairingId === request.pairingId)) showToast(request);
      }
      for (const request of prev.incoming) {
        if (!state.incoming.some((i) => i.pairingId === request.pairingId)) {
          toast.dismiss(incomingToastId(request.pairingId));
        }
      }
    });
  }, []);
}
```

- [ ] **Step 10: Add `src/components/network/NetworkIntroCard.tsx`**

```tsx
import { useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardFooter, CardHeader, CardTitle } from '@/components/ui/card';
import { errorText } from '@/lib/network-utils';
import { markNetworkIntroSeen, setNetworkVisibility } from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';
import { useSettingsStore } from '@/stores/settings-store';

/**
 * A one-time card after the first launch: "You're visible on this network as ...".
 * Every button marks the intro as seen. It does not show for a hidden node or when
 * networking is off.
 */
export function NetworkIntroCard() {
  const status = useNetworkStore((s) => s.status);
  const [dismissed, setDismissed] = useState(false);

  if (!status || status.state !== 'running' || status.introSeen || status.hidden || dismissed) {
    return null;
  }

  // The card goes at once. A failed save only means it may return after the next launch.
  async function markSeen() {
    setDismissed(true);
    try {
      await markNetworkIntroSeen();
      await useNetworkStore.getState().refresh();
    } catch (err) {
      console.error('[NetworkIntroCard] could not save the intro flag', err);
    }
  }

  function editName() {
    useSettingsStore.getState().openSettings('network');
    void markSeen();
  }

  async function hideNode() {
    try {
      await setNetworkVisibility(true);
    } catch (err) {
      toast.error(errorText(err));
      return;
    }
    await markSeen();
  }

  return (
    <Card
      role='region'
      aria-label='Team network'
      className='fixed bottom-9 left-3 z-40 w-80 max-w-[calc(100vw-1.5rem)]'
    >
      <CardHeader className='p-4 pb-2'>
        <CardTitle className='text-sm'>Team network</CardTitle>
      </CardHeader>
      <CardContent className='px-4 pb-2 text-sm text-muted-foreground'>
        {`You're visible on this network as '${status.displayName}'.`}
      </CardContent>
      <CardFooter className='flex flex-wrap gap-2 p-4 pt-2'>
        <Button size='sm' variant='outline' onClick={editName}>
          Edit name
        </Button>
        <Button size='sm' variant='outline' onClick={() => void hideNode()}>
          Hide my node
        </Button>
        <Button size='sm' onClick={() => void markSeen()}>
          OK
        </Button>
      </CardFooter>
    </Card>
  );
}
```

- [ ] **Step 11: Add `src/components/settings/sections/NetworkSection.tsx`**

```tsx
import { type ReactNode, useEffect, useState } from 'react';
import { toast } from 'sonner';
import { validateDisplayName } from '@/components/network/network-helpers';
import { SettingsSectionHeader } from '@/components/settings/SettingsSectionHeader';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { errorText } from '@/lib/network-utils';
import {
  listNetworkAddresses,
  listNetworkBlocked,
  type NetworkAddress,
  type NetworkBlockedDevice,
  removeNetworkAddress,
  setNetworkProfile,
  setNetworkVisibility,
  unblockNetworkDevice,
} from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';

function Block({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className='space-y-2 border-b border-border py-4 last:border-b-0'>
      <h3 className='text-sm font-medium'>{title}</h3>
      {children}
    </section>
  );
}

// Settings for the team network: visibility, name, port, blocked devices, saved addresses.
export function NetworkSection() {
  const status = useNetworkStore((s) => s.status);
  const displayName = status?.displayName ?? '';
  const [name, setName] = useState(displayName);
  const [nameError, setNameError] = useState<string | null>(null);
  const [savingName, setSavingName] = useState(false);
  const [blocked, setBlocked] = useState<NetworkBlockedDevice[]>([]);
  const [addresses, setAddresses] = useState<NetworkAddress[]>([]);
  const [listError, setListError] = useState<string | null>(null);

  useEffect(() => {
    void useNetworkStore.getState().refresh();
  }, []);

  // The field follows the stored name after a save or a reload.
  useEffect(() => {
    setName(displayName);
  }, [displayName]);

  useEffect(() => {
    let disposed = false;
    Promise.all([listNetworkBlocked(), listNetworkAddresses()]).then(
      ([blockedList, addressList]) => {
        if (disposed) return;
        setBlocked(blockedList);
        setAddresses(addressList);
      },
      (err) => {
        if (!disposed) setListError(errorText(err));
      },
    );
    return () => {
      disposed = true;
    };
  }, []);

  async function toggleHidden(hidden: boolean) {
    try {
      await setNetworkVisibility(hidden);
    } catch (err) {
      toast.error(errorText(err));
    }
    // The switch shows the stored value, so a failure puts it back.
    await useNetworkStore.getState().refresh();
  }

  async function saveName() {
    const check = validateDisplayName(name);
    if (!check.ok) {
      setNameError(check.error);
      return;
    }
    setNameError(null);
    setSavingName(true);
    try {
      await setNetworkProfile(check.name);
      await useNetworkStore.getState().refresh();
      toast.success('Name saved.');
    } catch (err) {
      setNameError(errorText(err));
    } finally {
      setSavingName(false);
    }
  }

  async function unblock(device: NetworkBlockedDevice) {
    try {
      await unblockNetworkDevice(device.deviceId);
      setBlocked((list) => list.filter((d) => d.deviceId !== device.deviceId));
    } catch (err) {
      toast.error(errorText(err));
    }
  }

  async function removeAddress(address: NetworkAddress) {
    try {
      await removeNetworkAddress(address.host, address.port);
      setAddresses((list) =>
        list.filter((a) => !(a.host === address.host && a.port === address.port)),
      );
      void useNetworkStore.getState().refreshNodes();
    } catch (err) {
      toast.error(errorText(err));
    }
  }

  return (
    <div>
      <SettingsSectionHeader
        title='Team network'
        description='Find and pair with other Rocket apps on your network.'
      />
      {!status ? (
        <p className='text-sm text-muted-foreground'>Loading...</p>
      ) : (
        <>
          {status.state === 'off' && (
            <div role='alert' className='mb-2 rounded-md border border-border p-3 text-sm'>
              {status.offReason ?? 'Team network is off.'}
            </div>
          )}
          {status.warnings.length > 0 && (
            <ul className='mb-2 list-disc space-y-1 pl-5 text-sm text-muted-foreground'>
              {status.warnings.map((warning) => (
                <li key={warning}>{warning}</li>
              ))}
            </ul>
          )}

          <Block title='Visibility'>
            <div className='flex items-center justify-between gap-4'>
              <div className='space-y-1'>
                <Label htmlFor='network-hidden'>Hide my node</Label>
                <p className='text-xs text-muted-foreground'>
                  Others on the network do not see you and cannot send pairing requests. Paired
                  nodes still reach you.
                </p>
              </div>
              <Switch
                id='network-hidden'
                checked={status.hidden}
                onCheckedChange={(next) => void toggleHidden(next)}
              />
            </div>
          </Block>

          <Block title='Name'>
            <Label htmlFor='network-display-name'>Display name</Label>
            <div className='flex items-start gap-2'>
              <div className='flex-1 space-y-1'>
                <Input
                  id='network-display-name'
                  value={name}
                  onChange={(e) => {
                    setName(e.target.value);
                    setNameError(null);
                  }}
                />
                {nameError && <p className='text-xs text-destructive'>{nameError}</p>}
              </div>
              <Button
                disabled={savingName || name.trim() === displayName}
                onClick={() => void saveName()}
              >
                Save
              </Button>
            </div>
          </Block>

          <Block title='This device'>
            <dl className='grid grid-cols-[8rem_1fr] gap-x-4 gap-y-2 text-sm'>
              <dt className='text-muted-foreground'>Port in use</dt>
              <dd className='select-text font-mono'>{status.port ?? 'Not listening'}</dd>
              <dt className='text-muted-foreground'>Device ID</dt>
              <dd className='select-text font-mono'>{status.deviceShortId ?? '-'}</dd>
            </dl>
          </Block>

          <Block title='Blocked devices'>
            {listError && <p className='text-sm text-destructive'>{listError}</p>}
            {!listError && blocked.length === 0 && (
              <p className='text-sm text-muted-foreground'>No blocked devices.</p>
            )}
            <ul className='space-y-1'>
              {blocked.map((device) => (
                <li key={device.deviceId} className='flex items-center gap-2 text-sm'>
                  <span className='min-w-0 flex-1 truncate'>{device.name}</span>
                  <span className='font-mono text-xs text-muted-foreground'>{device.shortId}</span>
                  <Button
                    size='sm'
                    variant='outline'
                    aria-label={`Unblock ${device.name}`}
                    onClick={() => void unblock(device)}
                  >
                    Unblock
                  </Button>
                </li>
              ))}
            </ul>
          </Block>

          <Block title='Saved addresses'>
            {!listError && addresses.length === 0 && (
              <p className='text-sm text-muted-foreground'>No saved addresses.</p>
            )}
            <ul className='space-y-1'>
              {addresses.map((address) => {
                const text = `${address.host}:${address.port}`;
                return (
                  <li key={text} className='flex items-center gap-2 text-sm'>
                    <span className='min-w-0 flex-1 truncate font-mono'>{text}</span>
                    <Button
                      size='sm'
                      variant='outline'
                      aria-label={`Remove ${text}`}
                      onClick={() => void removeAddress(address)}
                    >
                      Remove
                    </Button>
                  </li>
                );
              })}
            </ul>
          </Block>
        </>
      )}
    </div>
  );
}
```

- [ ] **Step 12: Register the section**

Edit `src/stores/settings-store.ts`. Replace the quoted line

```ts
export type SettingsSection = 'proxy' | 'secret-managers' | 'ai' | 'support' | 'about';
```

with:

```ts
export type SettingsSection = 'proxy' | 'secret-managers' | 'ai' | 'network' | 'support' | 'about';
```

Edit `src/components/settings/settings-sections.ts`.

Edit 1: replace the quoted line `import { Bot, Globe, Info, KeyRound, LifeBuoy, type LucideIcon } from 'lucide-react';` with:

```ts
import {
  Bot,
  Globe,
  Info,
  KeyRound,
  LifeBuoy,
  type LucideIcon,
  Radar,
} from 'lucide-react';
```

Edit 2: after the quoted line `import { AiAgentsSection } from '@/components/settings/sections/AiAgentsSection';` add:

```ts
import { NetworkSection } from '@/components/settings/sections/NetworkSection';
```

Edit 3: after the quoted line `  { id: 'ai', label: 'AI Agents', icon: Bot, Component: AiAgentsSection, wide: true },` add:

```ts
  {
    id: 'network',
    label: 'Team network',
    icon: Radar,
    Component: NetworkSection,
    wide: true,
  },
```

- [ ] **Step 13: Mount the new pieces in `src/App.tsx`**

Edit 1: after the quoted line `import { WorkspaceToolbar } from '@/components/layout/WorkspaceToolbar';` add:

```tsx
import { NetworkIntroCard } from '@/components/network/NetworkIntroCard';
import { PairingDialog } from '@/components/network/PairingDialog';
```

Edit 2: after the quoted line `import { useFlowRequestPathEvents } from '@/hooks/useFlowRequestPathEvents';` add:

```tsx
import { useIncomingPairingNotice } from '@/hooks/useIncomingPairingNotice';
```

Edit 3: after the quoted line `  useNetworkEventBridge();` add:

```tsx
  useIncomingPairingNotice();
```

Edit 4: after the quoted line `      <TrustMigrationNotice />` add:

```tsx
      <PairingDialog />
      <NetworkIntroCard />
```

- [ ] **Step 14: Run the tests to verify they pass**

Run: `bunx vitest run src/components/network src/components/status-bar src/hooks/__tests__/useIncomingPairingNotice.test.tsx src/components/settings src/components/layout/__tests__/StatusBar.test.tsx src/stores/__tests__/network-store.test.ts src/lib/__tests__/network-event-bridge.test.ts`
Expected: PASS.

- [ ] **Step 15: Run the checks**

Run: `bun run tsc --noEmit && bun run check`
Expected: no errors. If `check` reports format or import order, run `bun run lint`, then `bun run check` again.

- [ ] **Step 16: Write `docs/features/team-network.md`**

````markdown
# Team network

Rocket nodes on the same network find each other, pair once with a matching 6-digit code and stay paired until one side removes the pairing. This is the base for sending collections and checking contracts between nodes (later projects). Spec: `docs/superpowers/specs/2026-11-05-team-network-discovery-pairing-design.md`. Plans: `docs/superpowers/plans/team-network/`. Roadmap: R-83, project CA-5.

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
| `NetworkService` (state, pairing, presence, audit) | `crates/rocket-app/src/network_service.rs` |
| Tauri commands and DTOs | `src-tauri/src/commands/network.rs` (commands are in `src-tauri/app-commands.txt`) |
| Wrappers, types and listeners | `src/lib/tauri-api.ts`, section "Team network" |
| State | `src/stores/network-store.ts` |
| Events | `src/lib/network-event-bridge.ts`, `src/hooks/useIncomingPairingNotice.tsx` |
| UI | `src/components/network/`, `src/components/status-bar/NetworkStatusItem.tsx`, `src/components/settings/sections/NetworkSection.tsx` |
| Data | `~/.rocket-api/network/` (`paired.yml`, `blocked.yml`, `addresses.yml`, `settings.yml`). Machine-local. The device key is in the OS keychain, service `com.rocketapi.device-identity`. |

## Frontend rules

- The backend owns the state. The store mirrors it. `nodesChanged` and `statusChanged` events trigger a refetch of `network_list_nodes` and `network_status`. A sequence counter drops a stale response.
- `pairingIncoming` and `pairingFinished` feed `incoming` and `activePairing` directly. Event fields are snake_case.
- `incoming` is deduped by pairing id and each entry expires 120 seconds after it arrived. A repeated event neither adds a row nor extends the expiry.
- Only the node that started a pairing gets the "<name> declined." toast.
- The Add by address, Remove and Pairing dialogs are mounted outside the Popover. Closing the panel never closes them.
- Closing the pairing dialog of a pairing you started, before you pressed Accept, cancels it.
- The UI shows the short device id only. Nonces, keys, certificates and the full device id are never shown or logged.

## Decisions

- Visible to everyone on the network by default, with a Hide my node switch (spec section 2). Hiding stops the announcement and refuses new requests. Paired nodes still connect.
- Pairing lasts until removed. A new key is a new device.
- The deviations from the spec are listed in `docs/superpowers/plans/team-network/00-plan-index.md`: quarantined files instead of `.bak`, coarser events (`nodesChanged`), a settings file, the dial grace period, hyper's low-level client, the pairing request first calls `/hello`.
- Error and empty-state copy comes from spec section 9 and is pinned by tests.

## Limits

- 200 paired devices, 50 saved addresses, 256 connections (32 unpaired), 5 pairing requests a minute.
- A pairing request expires after 2 minutes. A declined device waits 1 minute.
- Same-network discovery only, plus Add by address. No relay, no internet pairing.
- No sending of collections or contracts yet.

## How to test

```bash
bunx vitest run src/lib/__tests__/tauri-api.network.test.ts src/stores/__tests__/network-store.test.ts \
  src/lib/__tests__/network-event-bridge.test.ts src/components/network src/components/status-bar \
  src/hooks/__tests__/useIncomingPairingNotice.test.tsx src/components/settings
bun run tsc --noEmit
bun run check
```

Backend tests: `cargo test -j4 -p rocket-network`, `cargo test -j4 -p rocket-infra network`, `cargo test -j4 -p rocket-app network`. Real network checks: `docs/manual-tests/team-network.md`.
````

- [ ] **Step 17: Write `docs/manual-tests/team-network.md`**

````markdown
# Manual test: team network

Checks of discovery, pairing and presence on real machines. Written 2026-11-06 from the plan. None of these steps were run when this file was written, so the first pass is also the check of the steps.

Contents

- Setup
- 1: two machines on one Wi-Fi
- 2: pairing
- 3: restart and reconnect
- 4: sleep and wake
- 5: network change
- 6: Add by address over VPN or Tailscale
- 7: declined firewall prompt
- 8: Wi-Fi with client isolation
- 9: hub with 30 teammates
- 10: hide, block, remove

## Setup

- Two machines, A and B, with the same Rocket build, on the same Wi-Fi. A third machine C helps in checks 9 and 10.
- Start each with `bun run tauri dev` or an installed build.
- To start clean on a machine: quit Rocket, delete `~/.rocket-api/network/` and the keychain entry with service `com.rocketapi.device-identity`. The machine then gets a new device id.
- Each machine shows its own short device id in Settings, Team network.

## 1: two machines on one Wi-Fi

1. Start A, then B. Do not touch either.
2. Wait at most 5 seconds. Open the radar icon in the status bar on A.
3. Expect B under Nearby with its name, initials and short device id, and a Pair button. B shows A the same way.
4. Type the first letters of B's name, then B's initials, then B's short id (with and without the dash). Each finds B. Type `zzz`: the panel says no nodes match.
5. First launch only: a card says "You're visible on this network as '<name>'." Press OK. Restart: the card does not return.

## 2: pairing

1. On A press Pair next to B. A shows a dialog with a 6-digit code (like `482 913`), B's name and B's short id.
2. B shows a toast "<A> wants to pair" and, in the panel, a request at the top. Press Review.
3. Expect the same code on both screens.
4. Press Accept on A, then on B. Both show "Paired with <name>." and the other node moves to Paired with the chip Online.
5. Repeat the pairing and press Decline on B. A shows "<B> declined." and nothing is saved.
6. Repeat and leave the request unanswered for 2 minutes. Expect "The request expired." and the dialog on B switches to the same text if it was open.
7. Press Pair on A again right after a decline. Expect the request to be refused for about 1 minute (busy).

## 3: restart and reconnect

1. With A and B paired and online, quit Rocket on B and start it again.
2. Expect B to show A as Paired and Online within about 30 seconds, with no action. A shows B going Offline, then Online again.
3. Quit both and start both. Expect the same result.

## 4: sleep and wake

1. Put B to sleep for 2 minutes. A shows B Offline after about 40 seconds (two missed pings).
2. Wake B. Expect B Online on A again within about 2 minutes, without any click.

## 5: network change

1. Move B from the Wi-Fi to another network (or switch Wi-Fi off and on). Move it back.
2. Expect B Offline while away and Online again after it returns. The pairing is still there.
3. Join both to a different Wi-Fi. Expect them to find each other again with the same pairing.

## 6: Add by address over VPN or Tailscale

1. Put A and B on different networks joined by a VPN or Tailscale.
2. On A open the panel and press Add by address. Enter B's VPN address or Tailscale name and the port from B's Settings (default 47821).
3. Expect B under Nearby on A within about a minute. Try port `0` and `70000`: the dialog refuses them. Try a closed port: the dialog or the list shows no node, with no crash.
4. Pair as in check 2. Restart both: the pair reconnects through the saved address.
5. In Settings, Team network, remove the saved address. Expect the pair to stay paired.

## 7: declined firewall prompt

1. On a machine with a firewall (macOS or Windows), start Rocket for the first time and deny the incoming connection prompt.
2. Expect the banner "Others can't connect to you. Allow Rocket in your firewall settings." in the panel.
3. Outgoing still works: this machine can pair with a node that can reach it? No. Pairing needs only this machine to reach the other one, so from this machine pressing Pair on a visible node works.
4. Allow Rocket in the firewall settings, restart, and expect the banner to be gone.

## 8: Wi-Fi with client isolation

1. Join A and B to a Wi-Fi with client isolation (guest Wi-Fi).
2. Expect "No Rocket nodes found on this network." and the hint "VPNs and some Wi-Fi networks block discovery." Add by address still shows its dialog.
3. If mDNS is blocked on the machine, expect "Automatic discovery isn't available here." and no hint.

## 9: hub with 30 teammates

Needs 30 nodes. Use real teammates or 30 machines or VMs.

1. Pick one node as the hub H. Each teammate presses Pair on H, one at a time or a few at once.
2. On H review each request from the panel (several open requests list at the top, each with its own toast). Accept each one. After 5 requests in a minute extra requests see "busy"; the teammate tries again a minute later.
3. When all 30 are paired, expect 30 rows under Paired, all Online, and the status bar number 30. Searching by one name finds one row. The panel stays smooth to scroll.
4. Leave it for 10 minutes. Expect all 30 to stay Online (the connection limit is 256).
5. The in-process version of this check is in the Rocket infra tests (one node paired with 30 nodes).

## 10: hide, block, remove

1. On B turn on Hide my node in Settings, Team network. A stops seeing B under Nearby within about 2 minutes, but B stays Online under Paired on A.
2. On C, try to pair with B: C does not see B. Turn Hide off: C sees B again.
3. On B press Block in a request from C. C sees "<B> declined." B lists C under Blocked devices. Press Unblock: C can request again.
4. On A remove B from the Paired menu and confirm. B sees "<A> removed the pairing." and A moves to Nearby (or disappears). Repeat with B offline: B sees the message when it reconnects, and the pairing is gone on both.
5. Change the display name on A (Settings). B shows the new name after the next connection. A name of 41 characters is refused.
6. Open Settings, Team network on a machine where the keychain is locked or unavailable. Expect networking off with the reason, in the panel and in Settings.
````

- [ ] **Step 18: Update `docs/features/settings-page.md`**

Replace the quoted line (inside "Sections"):

`Order follows Bruno: Proxy, Secret Managers, AI Agents, Support, About. The list is`

with:

`Order follows Bruno, with Team network (not in Bruno) after AI Agents: Proxy, Secret Managers, AI Agents, Team network, Support, About. The list is`

and replace the quoted sentence `Secret Managers and AI Agents use the wide panel.` with `Secret Managers, AI Agents and Team network use the wide panel. Team network is described in \`docs/features/team-network.md\`.`

- [ ] **Step 19: Add the pointer to `CLAUDE.md`**

After the quoted line (the update notification pointer):

```
See `docs/features/update-notification.md` for the update notice (schedule, install kinds, settings, manual checks).
```

add:

```
See `docs/features/team-network.md` for the team network (discovery, pairing, presence, Network panel, Settings section, limits, manual checks).
```

- [ ] **Step 20: Update the roadmaps**

In `docs/roadmaps/roadmap.md`, in the R-83 row replace the quoted cell `| spec |` (the status cell right after the project text, before the spec path) with `| built, manual checks open |`, and replace the last cell text `User reviews the spec, then plans. Send and receive, and contract checks between nodes, get their own specs.` with `Manual checks in \`docs/manual-tests/team-network.md\`. Feature doc: \`docs/features/team-network.md\`. Send and receive, and contract checks between nodes, get their own specs.`

In `docs/roadmaps/contracts-ci-ai-roadmap.md`:

- In the table row `| CA-5 | Team network: AirDrop-style discovery and pairing, one node as hub | none | spec |` replace the last cell `spec` with `built, manual checks open`.
- In the CA-5 section replace `- MVP (spec written): every Rocket is a node.` with `- MVP (built, manual checks open in \`docs/manual-tests/team-network.md\`): every Rocket is a node.`

Do not touch other rows. These two files already have uncommitted edits from the user; edit by quoted text only.

- [ ] **Step 21: Run the whole frontend verification**

Run: `bun run tsc --noEmit && bun run check && bunx vitest run src/lib src/stores src/components/network src/components/status-bar src/components/layout src/components/settings src/hooks`
Expected: all PASS. No Rust file changed in this plan, so no cargo command is needed.

- [ ] **Step 22: No commit**

No commit; reviewed at the plan end.

---

## End of Plan 5. Verify:

```bash
bun run tsc --noEmit
bun run check
bunx vitest run src/lib src/stores src/components/network src/components/status-bar src/components/layout src/components/settings src/hooks
git status --short   # only the files named in the File Structure table, plus the user's existing roadmap edits
```

Then stop. Show the user the whole Plan 5 change. After the user's OK, commit through `dev-workflow-skills:1-git-commit`, staging explicit paths only, in this split:

1. `feat(network): add network store, event bridge and wrappers` (Task 1 files).
2. `feat(network): add status bar item, Network panel and address dialogs` (Task 2 files).
3. `feat(network): add pairing dialog, intro card and Settings section` (Task 3 source and tests).
4. `docs(network): add feature doc, manual checks and roadmap status` (docs, `CLAUDE.md`, roadmaps).

After all five plans are merged, run the Opus security review of the protocol and the peer server from spec section 8.7, then the manual checks in `docs/manual-tests/team-network.md` on two real machines.

### Manual check

Run the real app on two machines (`bun run tauri dev`), both on one Wi-Fi.

1. The status bar shows the radar icon. The panel lists the other machine under Nearby within 5 seconds, and the search box finds it by name, initials and short id.
2. Pair: both screens show the same code in the form `482 913`. Accept on both. The panel shows Paired and Online, and the status bar number is 1.
3. Decline once and let one request expire. Check the toasts and the dialog text: "Dana declined." only on the node that started the pairing, "The request expired." for the expiry.
4. Restart one machine. The pair comes back Online with no action.
5. Remove the pairing from the row menu. The confirmation names the node. The other machine shows "<name> removed the pairing."
6. Add by address with a wrong port (0), then with a valid address. The dialog shows the validation and the backend error text.
7. Settings, Team network: Hide my node, rename (try 41 characters), port and device id, blocked list with Unblock, saved addresses with Remove.
8. First launch (delete `~/.rocket-api/network/settings.yml`): the intro card shows once. Edit name opens Settings on Team network.
9. Switch between light and dark themes: the panel, the chips and the pairing dialog stay readable. Check in WebKitGTK on Linux that the popover opens upward above the status bar.

## Self-Review

**Spec mapping**

| Spec | Where |
|---|---|
| 5.5 panel, dialogs, notice, first-launch card | Tasks 2 and 3 |
| 5.5 Settings: visibility, name, port, blocked devices | Task 3 `NetworkSection` |
| 5.5 status bar icon with online count | Task 2 `NetworkStatusItem` |
| 5.5 `network-store` fed by listeners | Task 1 |
| 3a many peers (several requests, 200 limit message) | Task 2 requests list, backend error surfaced by `toast.error`; manual check 9 |
| 7.5 panel (search, groups, chips, Add by address, requests on top) | Task 2 |
| 8.4 first launch card | Task 3 `NetworkIntroCard` |
| 9 error copy (firewall, discovery, failed, expired, declined, needs update, keychain, limit 200) | Task 1 bridge, Task 2 panel, Task 3 dialog and section; strings pinned in tests |
| 10 Vitest: store reacts to each event; search, groups, chips; dialog commands; card once | Tasks 1 to 3 tests |
| 10 manual checks | `docs/manual-tests/team-network.md` |
| Index command table (14 wrappers) and event channels | Task 1 |

**Names used.** Wrappers, channels and DTO fields match the plan index. TS type names (`NetworkStatus`, `NetworkNode`, `NetworkAddress`, `NetworkBlockedDevice`, `NetworkPairingStarted`) drop the `Dto` suffix used on the Rust side, as the repo does for other IPC types. `Presence` values are the node statuses (`online`, `offline`, `pairing`, `needs_update`, `available`).

**Deviations from the plan index and the task brief**

1. `ActivePairing` and `PairingIncoming` carry extra fields (`deviceId`, `shortId`; `accepted` on incoming) because the dialog shows the short id and a request that was accepted must show "Waiting for ...". `phase` means: `waiting` = the user has not decided, `done` = the user accepted and waits for the peer.
2. The store also owns two UI flags, `panelOpen` and `dialogPairingId`, so a toast, the intro card and the panel can open each other's UI.
3. The incoming toast is created by a store subscriber hook (`useIncomingPairingNotice`), not by the bridge, so Task 1 does not depend on a Task 3 component and a duplicate event cannot make a second toast.
4. The two event channels have two payload unions (`NetworkChangedEvent`, `NetworkPairingEvent`), one per channel, instead of one union.
5. Dialog buttons that only hide the dialog are named "Done" because `DialogContent` already has an X button named "Close".
6. The "declined" toast is limited to the node that started the pairing, and `cancelled` is silent. The spec does not say who sees "<name> declined." and a user should not get a toast about their own click.

**Not covered / assumptions that need the backend to confirm**

- `respondNetworkPairing(id, 'accept')` is assumed to work for the node that started the pairing (the initiator's Accept), and `cancelNetworkPairing` to work for either role. Plan 4 defines `respond_pairing(id, decision)` without a role split; if the initiator needs another command, Task 3's `acceptStarted` changes only.
- It is assumed that `PairingFinished` is also sent to the node that pressed Decline or Block, and that the Rust side emits `nodesChanged` after a pairing is saved. The frontend works if it does not, because `accepted` also reloads the nodes and the panel reloads on open.
- The exact wording of backend error messages (for example the 200-device limit) comes from the backend. The UI shows them as received after dropping the type prefix.
- Window focus and tray behaviour for the incoming toast (no OS notification when Rocket is in the background) are not built. Manual check 2 shows what happens today.
- A visual Network tab, a diagnostics view, "paired only" visibility, team invite and any sending of data are post-MVP items from spec section 11.
