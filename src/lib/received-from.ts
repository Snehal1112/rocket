import { peerName } from '@/lib/network-utils';
import type { ReceivedCollection } from '@/lib/tauri-api';

/** "Received from Ana". */
export function receivedFromText(origin: ReceivedCollection): string {
  return `Received from ${peerName(origin.senderName)}`;
}

/** "11 Nov 2026", in the viewer's locale. An unreadable date gives an empty text. */
export function receivedOnText(origin: ReceivedCollection): string {
  const date = new Date(origin.receivedAt);
  if (Number.isNaN(date.getTime())) return '';
  return date.toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' });
}
