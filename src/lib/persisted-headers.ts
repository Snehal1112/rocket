// Canonical conversion from UI header rows to the persisted Header shape.
//
// Do NOT reuse execute-request.ts's header filtering for this — that filters
// BY `enabled` (only enabled headers should go out on the wire), which is
// the wrong rule for what gets written to disk: a disabled header is a real,
// intentional piece of the saved request and must round-trip, not vanish.
import type { Header } from '@/lib/tauri-api';

interface HeaderRowLike {
  key: string;
  value: string;
  enabled: boolean;
}

/**
 * Drops blank-key rows (unfinished/draft rows a user hasn't named yet), and
 * always preserves `enabled` verbatim — including `false` — for every
 * remaining row.
 */
export function toPersistedHeaders(headers: HeaderRowLike[]): Header[] {
  return headers
    .filter((h) => h.key)
    .map((h) => ({ key: h.key, value: h.value, enabled: h.enabled }));
}
