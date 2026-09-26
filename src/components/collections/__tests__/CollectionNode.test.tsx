import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
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
    getCollectionSummaries: vi.fn(),
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
  settings: { headers: [], variables: [], sandboxMode: 'safe' },
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
    vi.mocked(tauriApi.getCollectionSummaries).mockResolvedValue(emptyCollection);
    // Force this node open via the pane store's "active collection" effect —
    // simpler and more reliable in a test than driving the tree's own
    // expand/collapse click handling.
    usePaneStore.setState({ activeCollection: summary.name });
  });

  it('refreshes its tree when a workspace-scoped branch switch fires git-changed, even though the repo path never matches this collection by name', async () => {
    renderNode();

    await waitFor(() => {
      expect(tauriApi.getCollectionSummaries).toHaveBeenCalledWith(summary.name);
    });
    const callsBeforeGitChanged = vi.mocked(tauriApi.getCollectionSummaries).mock.calls.length;

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
      expect(vi.mocked(tauriApi.getCollectionSummaries).mock.calls.length).toBeGreaterThan(
        callsBeforeGitChanged,
      );
    });
  });
});

describe('CollectionNode summary item rendering', () => {
  // The real get_collection_summaries backend excludes type: 'opaque' items (non-HTTP
  // protocols like GraphQL are skipped entirely). This fixture exercises the frontend's
  // defensive render guard as a forward-looking safety net, not current end-to-end behavior.
  const collectionWithSummaryAndOpaqueItems: tauriApi.Collection = {
    name: 'my-collection',
    root: {
      uid: 'root',
      name: 'my-collection',
      items: [
        {
          type: 'summary',
          uid: 'req-1',
          name: 'List Orders',
          method: 'GET',
          url: 'https://api.example.com/orders',
          fileName: 'list-orders.yml',
        },
        {
          type: 'summary',
          uid: 'req-2',
          name: 'Create Invoice',
          method: 'GET',
          url: 'https://api.example.com/invoices',
          fileName: 'create-invoice.yml',
        },
        {
          type: 'opaque',
          protocol: 'graphql',
          name: 'GraphQL Query',
          raw: {},
        },
      ],
    },
    settings: { headers: [], variables: [], sandboxMode: 'safe' },
  };

  beforeEach(() => {
    vi.mocked(tauriApi.getCollectionSummaries).mockResolvedValue(
      collectionWithSummaryAndOpaqueItems,
    );
    usePaneStore.setState({ activeCollection: summary.name });
  });

  it('renders a summary item as a request row and skips the opaque item', async () => {
    renderNode();

    await waitFor(() => {
      expect(screen.getByTestId('request-item-GET-List Orders')).toBeInTheDocument();
    });
    expect(screen.queryByText('GraphQL Query')).not.toBeInTheDocument();
  });

  it('filters out a non-matching summary item by name, like it does for request items', async () => {
    render(
      <QueryClientProvider
        client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
      >
        <CollectionNode
          summary={summary}
          filter='orders'
          summaries={[summary]}
          onNewFolder={vi.fn()}
          onMove={vi.fn()}
          onDelete={vi.fn()}
          onDuplicate={vi.fn()}
        />
      </QueryClientProvider>,
    );

    // Positive control: prove rendering actually happened and settled before
    // asserting on absence — otherwise the absence check below could pass
    // trivially because nothing has rendered yet.
    await waitFor(() => {
      expect(screen.getByTestId('request-item-GET-List Orders')).toBeInTheDocument();
    });
    expect(screen.queryByTestId('request-item-GET-Create Invoice')).not.toBeInTheDocument();
  });
});

describe('CollectionNode filter hides folders that only contain opaque items', () => {
  // A folder containing only an opaque item (no folders, no other requests) must be
  // treated as empty by the filter — opaque items never render (see the
  // `item.type === 'opaque'` guard in both CollectionNode's and FolderNode's render
  // loops), so keeping them in `filteredItems` would incorrectly keep an otherwise-empty
  // folder visible under a non-matching filter.
  const collectionWithFolderContainingOnlyOpaqueItem: tauriApi.Collection = {
    name: 'my-collection',
    root: {
      uid: 'root',
      name: 'my-collection',
      items: [
        {
          type: 'folder',
          uid: 'folder-1',
          name: 'GraphQL Stuff',
          items: [
            {
              type: 'opaque',
              protocol: 'graphql',
              name: 'GraphQL Query',
              raw: {},
            },
          ],
        },
        {
          type: 'summary',
          uid: 'req-1',
          name: 'List Orders',
          method: 'GET',
          url: 'https://api.example.com/orders',
          fileName: 'list-orders.yml',
        },
      ],
    },
    settings: { headers: [], variables: [], sandboxMode: 'safe' },
  };

  beforeEach(() => {
    vi.mocked(tauriApi.getCollectionSummaries).mockResolvedValue(
      collectionWithFolderContainingOnlyOpaqueItem,
    );
    usePaneStore.setState({ activeCollection: summary.name });
  });

  it('hides a folder whose only content is a non-matching opaque item under an active filter', async () => {
    render(
      <QueryClientProvider
        client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
      >
        <CollectionNode
          summary={summary}
          filter='orders'
          summaries={[summary]}
          onNewFolder={vi.fn()}
          onMove={vi.fn()}
          onDelete={vi.fn()}
          onDuplicate={vi.fn()}
        />
      </QueryClientProvider>,
    );

    // Positive control: prove rendering actually happened and settled before
    // asserting on absence — otherwise the absence check below could pass
    // trivially because nothing has rendered yet.
    await waitFor(() => {
      expect(screen.getByTestId('request-item-GET-List Orders')).toBeInTheDocument();
    });
    expect(screen.queryByText('GraphQL Stuff')).not.toBeInTheDocument();
  });
});
