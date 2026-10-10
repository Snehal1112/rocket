import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// A fake backend: each save records which workspace was active when it arrived.
const backend = {
  active: 'A',
  savedIn: [] as string[],
  failSave: false,
  switching: false,
  overlapped: false,
};

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  saveRequest: vi.fn(async () => {
    if (backend.failSave) throw new Error('disk full');
    backend.savedIn.push(backend.active);
  }),
  switchWorkspace: vi.fn(async (id: string) => {
    if (backend.switching) backend.overlapped = true;
    backend.switching = true;
    // A few ticks, so an overlapping second switch would be seen.
    for (let i = 0; i < 5; i++) await Promise.resolve();
    backend.active = id;
    backend.switching = false;
    return { id };
  }),
  // Closing or deleting the active workspace activates "default", as the backend does.
  closeWorkspace: vi.fn(async (id: string) => {
    if (backend.active === id) backend.active = 'default';
  }),
  deleteWorkspace: vi.fn(async (id: string) => {
    if (backend.active === id) backend.active = 'default';
  }),
  getActiveWorkspace: vi.fn(async () => ({ id: backend.active })),
}));

import { scheduleAutoSave } from '@/lib/auto-save';
import { createDefaultRequest } from '@/lib/pane-utils';
import {
  closeWorkspaceSafely,
  deleteWorkspaceSafely,
  switchWorkspaceSafely,
} from '@/lib/workspace-switch';
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
    backend.switching = false;
    backend.overlapped = false;
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
    await expect(switchWorkspaceSafely('B')).rejects.toThrow(/not changed/);
    expect(backend.active).toBe('A');
  });

  it('closing the active workspace saves open edits into it first', async () => {
    withDirtyTabs([dirtyTab]);
    await closeWorkspaceSafely('A');
    expect(backend.savedIn).toEqual(['A']);
    expect(useWorkspaceStore.getState().activeWorkspaceId).toBe('default');
  });

  it('deleting the active workspace drops a pending autosave instead of moving it', async () => {
    vi.useFakeTimers();
    scheduleAutoSave('tab1', 'api', 'get-user.yml', 'Get user', createDefaultRequest());
    const deleting = deleteWorkspaceSafely('A');
    await vi.advanceTimersByTimeAsync(1000);
    await deleting;
    expect(backend.active).toBe('default');
    expect(useWorkspaceStore.getState().activeWorkspaceId).toBe('default');
    expect(backend.savedIn).toEqual([]);
  });

  it('closing another workspace changes nothing about autosaves', async () => {
    withDirtyTabs([dirtyTab]);
    await closeWorkspaceSafely('X');
    expect(backend.savedIn).toEqual([]);
    expect(useWorkspaceStore.getState().activeWorkspaceId).toBe('A');
  });

  it('runs two quick switches one after the other and re-enables autosaves after both', async () => {
    await Promise.all([switchWorkspaceSafely('B'), switchWorkspaceSafely('C')]);
    expect(backend.overlapped).toBe(false);
    expect(backend.active).toBe('C');
    expect(useWorkspaceStore.getState().activeWorkspaceId).toBe('C');

    vi.useFakeTimers();
    scheduleAutoSave('tab1', 'api', 'get-user.yml', 'Get user', createDefaultRequest());
    await vi.advanceTimersByTimeAsync(1000);
    expect(backend.savedIn).toEqual(['C']);
  });
});
