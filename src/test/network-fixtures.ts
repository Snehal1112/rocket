import type { NetworkLink, NetworkNode, NetworkStatus, SyncSummary } from '@/lib/tauri-api';
import type { IncomingTransfer, PairingIncoming, SyncReviewEntry } from '@/stores/network-store';

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

/** An open request from a node that wants to pair. The user has not answered yet. */
export function makeIncoming(patch: Partial<PairingIncoming> = {}): PairingIncoming {
  return {
    pairingId: 'p1',
    deviceId: 'DEV9',
    shortId: 'ZZZZ-9999',
    displayName: 'Dana Roy',
    code: '482913',
    accepted: false,
    ...patch,
  };
}

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

/** A summary with a few counts and no marks. */
export function makeSyncSummary(patch: Partial<SyncSummary> = {}): SyncSummary {
  return {
    addedHere: 1,
    changedHere: 2,
    deletedHere: 0,
    sentAdded: 0,
    sentChanged: 1,
    sentDeleted: 0,
    scriptsChanged: [],
    authChanged: [],
    ...patch,
  };
}

/** A review the planner has not answered yet, with no conflicts. */
export function makeSyncReview(patch: Partial<SyncReviewEntry> = {}): SyncReviewEntry {
  return {
    syncId: 's1',
    linkId: 'L1',
    deviceId: 'DEV1',
    shortId: 'AAAA-1111',
    displayName: 'Ben Ortiz',
    collectionName: 'Orders',
    summary: makeSyncSummary(),
    conflicts: [],
    kind: 'review',
    answered: false,
    ...patch,
  };
}

/** A link in sync with an online node. */
export function makeLink(patch: Partial<NetworkLink> = {}): NetworkLink {
  return {
    linkId: 'L1',
    deviceId: 'DEV1',
    displayName: 'Ben Ortiz',
    shortId: 'AAAA-1111',
    collectionName: 'Orders',
    state: 'in_sync',
    updatedAt: '2026-11-08T10:00:00Z',
    ...patch,
  };
}
