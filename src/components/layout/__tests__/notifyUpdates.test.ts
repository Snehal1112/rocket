import { describe, expect, it } from 'vitest';
import { shouldNotify } from '../notifyUpdates';

const KEY = 'rocket.openapi-sync.notified';

function memoryStore(initial?: string) {
  const map = new Map<string, string>();
  if (initial !== undefined) map.set(KEY, initial);
  return {
    map,
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
  };
}

describe('shouldNotify', () => {
  it('notifies_once_per_pending_id', () => {
    const store = memoryStore();
    expect(shouldNotify(store, 'Orders', 'aaa')).toBe(true);
    expect(shouldNotify(store, 'Orders', 'aaa')).toBe(false);
  });

  it('a_new_pending_id_notifies_again', () => {
    const store = memoryStore();
    shouldNotify(store, 'Orders', 'aaa');
    expect(shouldNotify(store, 'Orders', 'bbb')).toBe(true);
  });

  it('different_collections_do_not_share_ids', () => {
    const store = memoryStore();
    shouldNotify(store, 'Orders', 'aaa');
    expect(shouldNotify(store, 'Billing', 'aaa')).toBe(true);
  });

  it('survives_a_throwing_storage', () => {
    const boom = {
      getItem: () => {
        throw new Error('blocked');
      },
      setItem: () => {
        throw new Error('blocked');
      },
    };
    expect(shouldNotify(boom, 'Orders', 'aaa')).toBe(false);
  });

  it('survives_corrupt_stored_data', () => {
    const store = memoryStore('{not json');
    expect(shouldNotify(store, 'Orders', 'aaa')).toBe(true);
  });

  it('caps_the_stored_ids', () => {
    const store = memoryStore();
    for (let i = 0; i < 201; i++) shouldNotify(store, 'Orders', `id${i}`);
    const stored = JSON.parse(store.map.get(KEY) ?? '[]') as string[];
    expect(stored).toHaveLength(200);
    expect(stored).not.toContain('Orders:id0');
    expect(stored).toContain('Orders:id200');
    expect(shouldNotify(store, 'Orders', 'id0')).toBe(true);
  });
});
