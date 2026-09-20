import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CollectionNode } from '@/components/collections/CollectionNode';
import type { CollectionSummary } from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => undefined),
}));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getCollection: vi.fn(),
    // biome-ignore lint/suspicious/noEmptyBlockStatements: unlisten stub.
    onCollectionChanged: vi.fn().mockResolvedValue(() => {}),
  };
});

const summary: CollectionSummary = {
  uid: 'col-1',
  repositoryId: 'repo-1',
  name: 'my-collection',
  path: '/workspace/collections/my-collection',
  requestCount: 0,
};

const emptyCollection: tauriApi.Collection = {
  name: 'my-collection',
  root: { uid: 'root', name: 'my-collection', items: [] },
  settings: { headers: [], variables: [] },
};

function renderNode() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <CollectionNode
        summary={summary}
        filter=''
        summaries={[summary]}
        onNewFolder={vi.fn()}
        onMove={vi.fn()}
        onDelete={vi.fn()}
        onDuplicate={vi.fn()}
      />
    </QueryClientProvider>,
  );
}

describe('CollectionNode git-changed refresh', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.getCollection).mockResolvedValue(emptyCollection);
    // Force this node open via the pane store's "active collection" effect —
    // simpler and more reliable in a test than driving the tree's own
    // expand/collapse click handling.
    usePaneStore.setState({ activeCollection: summary.name });
  });

  it('refreshes its tree when a workspace-scoped branch switch fires git-changed, even though the repo path never matches this collection by name', async () => {
    renderNode();

    await waitFor(() => {
      expect(tauriApi.getCollection).toHaveBeenCalledWith(summary.name);
    });
    const callsBeforeGitChanged = vi.mocked(tauriApi.getCollection).mock.calls.length;

    const { listen } = await import('@tauri-apps/api/event');
    const gitChangedHandler = vi
      .mocked(listen)
      .mock.calls.find(([eventName]) => eventName === 'git-changed')?.[1];
    expect(gitChangedHandler).toBeDefined();

    // Simulate a BranchSwitched event whose `collection` field is the
    // *workspace root path* (the common case — see
    // fs_repository_path_resolver.rs), which never matches this collection's
    // name by the existing collection-changed listener's path-segment logic.
    gitChangedHandler?.({
      event: 'git-changed',
      id: 1,
      payload: { type: 'branchSwitched', collection: '/workspace', branch: 'feature-x' },
    });

    await waitFor(() => {
      expect(vi.mocked(tauriApi.getCollection).mock.calls.length).toBeGreaterThan(
        callsBeforeGitChanged,
      );
    });
  });
});
