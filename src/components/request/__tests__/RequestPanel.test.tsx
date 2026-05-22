import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@/stores/paneStore', () => ({
  usePaneStore: vi.fn(() => ({ activeTabId: 'tab1', tabs: [] })),
}));

describe('RequestPanel body mode selector', () => {
  it('includes formurlencoded in BODY_MODES', async () => {
    const { BODY_MODES } = await import('../RequestPanel');
    const modes = BODY_MODES.map((m: { label: string; value: string }) => m.value);
    expect(modes).toContain('formurlencoded');
  });

  it('labels formurlencoded as Form URL Encoded', async () => {
    const { BODY_MODES } = await import('../RequestPanel');
    const entry = BODY_MODES.find(
      (m: { label: string; value: string }) => m.value === 'formurlencoded',
    );
    expect(entry?.label).toBe('Form URL Encoded');
  });
});
