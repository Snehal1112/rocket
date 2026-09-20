import { describe, expect, it, vi } from 'vitest';
import { openGitPanel } from '@/components/layout/GitToolbarButton';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { createDeferred } from '@/test/deferred';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, listCollections: vi.fn() };
});

vi.mock('sonner', () => ({ toast: { error: vi.fn() } }));

describe('openGitPanel active-collection race', () => {
  it('does not open a tab for a collection the user has since navigated away from', async () => {
    usePaneStore.getState().reset();
    usePaneStore.setState({ activeCollection: 'collection-a' });

    const deferred = createDeferred<tauriApi.CollectionSummary[]>();
    vi.mocked(tauriApi.listCollections).mockReturnValue(deferred.promise);

    const openPromise = openGitPanel();

    // User switches to a different collection while listCollections() is pending.
    usePaneStore.setState({ activeCollection: 'collection-b' });

    deferred.resolve([
      {
        uid: 'uid-a',
        name: 'collection-a',
        repositoryId: '/repos/a',
        path: '/path/a',
        requestCount: 0,
      } as tauriApi.CollectionSummary,
    ]);
    await openPromise;

    const { root } = usePaneStore.getState();
    const hasGitTabForA =
      root.type === 'leaf' && root.tabs.some((t) => t.id === 'git:collection-a');
    expect(hasGitTabForA).toBe(false);
  });
});
