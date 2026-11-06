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
