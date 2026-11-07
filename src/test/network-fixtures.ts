import type { NetworkNode, NetworkStatus } from '@/lib/tauri-api';
import type { IncomingTransfer, PairingIncoming } from '@/stores/network-store';

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
