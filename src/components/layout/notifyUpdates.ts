const STORAGE_KEY = 'rocket.openapi-sync.notified';
const MAX_ENTRIES = 200;

type NotifyStore = Pick<Storage, 'getItem' | 'setItem'>;

function readIds(store: NotifyStore): string[] {
  const raw = store.getItem(STORAGE_KEY);
  if (!raw) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter((v): v is string => typeof v === 'string') : [];
  } catch {
    // Corrupt data reads as an empty list.
    return [];
  }
}

/**
 * True once per `collection:pendingId`, and records it. A storage error returns false, so a
 * broken store never throws and never repeats a toast. Only the newest 200 ids are kept.
 */
export function shouldNotify(store: NotifyStore, collection: string, pendingId: string): boolean {
  try {
    const id = `${collection}:${pendingId}`;
    const ids = readIds(store);
    if (ids.includes(id)) return false;
    ids.push(id);
    store.setItem(STORAGE_KEY, JSON.stringify(ids.slice(-MAX_ENTRIES)));
    return true;
  } catch {
    return false;
  }
}

// The session copy keeps dedupe working when localStorage is blocked or throws.
const sessionIds = new Map<string, string>();

/** A store that prefers localStorage and falls back to memory for the session. */
export const notifyStore: NotifyStore = {
  getItem(key) {
    try {
      const value = localStorage.getItem(key);
      if (value !== null) return value;
    } catch {
      // Fall through to the session copy.
    }
    return sessionIds.get(key) ?? null;
  },
  setItem(key, value) {
    sessionIds.set(key, value);
    try {
      localStorage.setItem(key, value);
    } catch {
      // The session copy is enough.
    }
  },
};

/** Forgets the session copy. Tests call it between cases. */
export function clearSessionNotified(): void {
  sessionIds.clear();
}
