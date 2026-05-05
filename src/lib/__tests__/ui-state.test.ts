import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@/lib/tauri-api', () => ({
  saveUiState: vi.fn().mockResolvedValue(undefined),
  loadUiState: vi.fn(),
}));

vi.mock('@/stores/pane-store', () => ({
  usePaneStore: {
    getState: () => ({
      isWorkspaceMode: () => false,
      activeCollection: 'my-collection',
      root: { type: 'leaf', tabs: [], activeTabId: '', groupId: 'g1' },
    }),
  },
}));

vi.mock('@/stores/layout-store', () => ({
  useLayoutStore: {
    getState: () => ({
      requestLayout: 'stacked',
      sidebarWidth: 350,
      isConsoleOpen: true,
      consoleHeight: 400,
    }),
    subscribe: vi.fn(() => () => undefined),
  },
}));

import { saveUiState } from '@/lib/tauri-api';
import { scheduleSaveUiState } from '@/lib/ui-state';

describe('scheduleSaveUiState', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
  });

  it('persists sidebarWidth, isConsoleOpen, consoleHeight from layout-store', async () => {
    scheduleSaveUiState();
    await vi.runAllTimersAsync();

    expect(saveUiState).toHaveBeenCalledWith(
      expect.objectContaining({
        sidebarWidth: 350,
        isConsoleOpen: true,
        consoleHeight: 400,
      }),
    );
  });
});
