import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// A fake backend: each save records which workspace was active when it arrived.
const backend = { active: 'A', savedIn: [] as string[], failSave: false };

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  saveRequest: vi.fn(async () => {
    if (backend.failSave) throw new Error('disk full');
    backend.savedIn.push(backend.active);
  }),
  switchWorkspace: vi.fn(async (id: string) => {
    backend.active = id;
    return { id };
  }),
}));

import { scheduleAutoSave } from '@/lib/auto-save';
import { createDefaultRequest } from '@/lib/pane-utils';
import { switchWorkspaceSafely } from '@/lib/workspace-switch';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import type { RequestTab } from '@/types/pane-types';

const dirtyTab: RequestTab = {
  id: 'tab1',
  title: 'Get user',
  tabType: 'request',
  request: createDefaultRequest(),
  response: null,
  isDirty: true,
  source: { collection: 'api', path: 'get-user.yml' },
};

function withDirtyTabs(tabs: RequestTab[]) {
  usePaneStore.setState({ dirtyRequestTabs: () => tabs });
}

describe('workspace switch and autosave', () => {
  beforeEach(() => {
    backend.active = 'A';
    backend.savedIn = [];
    backend.failSave = false;
    useWorkspaceStore.setState({ activeWorkspaceId: 'A' });
    withDirtyTabs([]);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('saves a dirty tab into the old workspace before switching', async () => {
    withDirtyTabs([dirtyTab]);
    await switchWorkspaceSafely('B');
    expect(backend.savedIn).toEqual(['A']);
    expect(backend.active).toBe('B');
    expect(useWorkspaceStore.getState().activeWorkspaceId).toBe('B');
  });

  it('runs a pending autosave into the old workspace and never fires it later', async () => {
    vi.useFakeTimers();
    scheduleAutoSave('tab1', 'api', 'get-user.yml', 'Get user', createDefaultRequest());
    await switchWorkspaceSafely('B');
    await vi.advanceTimersByTimeAsync(1000);
    expect(backend.savedIn).toEqual(['A']);
  });

  it('drops a pending autosave when the workspace changed without a flush', async () => {
    vi.useFakeTimers();
    scheduleAutoSave('tab1', 'api', 'get-user.yml', 'Get user', createDefaultRequest());
    // The backend switched by another path; the listener updates the store.
    backend.active = 'B';
    useWorkspaceStore.setState({ activeWorkspaceId: 'B' });
    await vi.advanceTimersByTimeAsync(1000);
    expect(backend.savedIn).toEqual([]);
  });

  it('does not switch when a save fails, so the edit is not lost', async () => {
    backend.failSave = true;
    withDirtyTabs([dirtyTab]);
    await expect(switchWorkspaceSafely('B')).rejects.toThrow(/not switched/);
    expect(backend.active).toBe('A');
  });
});
