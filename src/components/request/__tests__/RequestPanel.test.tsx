import { describe, expect, it, vi } from 'vitest';
import { BODY_MODES } from '../RequestPanel';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@/stores/paneStore', () => ({
  usePaneStore: vi.fn(() => ({ activeTabId: 'tab1', tabs: [] })),
}));

describe('RequestPanel body mode selector', () => {
  it('includes formurlencoded in BODY_MODES', () => {
    const modes = BODY_MODES.map((m) => m.value);
    expect(modes).toContain('formurlencoded');
  });

  it('labels formurlencoded as Form URL Encoded', () => {
    const entry = BODY_MODES.find((m) => m.value === 'formurlencoded');
    expect(entry?.label).toBe('Form URL Encoded');
  });
});
