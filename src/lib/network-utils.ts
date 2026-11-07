// Shows six digits as two groups of three, for example "482 913".
export function formatPairingCode(code: string): string {
  return /^\d{6}$/.test(code) ? `${code.slice(0, 3)} ${code.slice(3)}` : code;
}

// Cuts a full device id to the short form the UI shows, for example "K7QF-2M9X".
export function shortDeviceId(full: string): string {
  return full.length > 9 ? `${full.slice(0, 4)}-${full.slice(4, 8)}` : full;
}

// One toast slot for pairing failures, so the event and the rejected command show it once.
export const PAIRING_FAILED_TOAST_ID = 'network-pairing-failed';

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
