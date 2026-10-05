import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { RequestNode } from '@/components/collections/RequestNode';
import { createDefaultLeaf, findTabInTree } from '@/lib/pane-utils';
import type { CollectionItem } from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getRequest: vi.fn(),
    getGraphQlRequest: vi.fn(),
  };
});

vi.mock('sonner', () => ({ toast: { error: vi.fn() } }));

const fullItem: Extract<CollectionItem, { type: 'request' } | { type: 'summary' }> = {
  type: 'request',
  uid: 'req-1',
  name: 'Get Users',
  method: 'GET',
  url: 'https://api.example.com/users',
  headers: [{ key: 'X-Test', value: '1', enabled: true }],
  auth: { authType: 'none' },
};

const summaryItem: Extract<CollectionItem, { type: 'request' } | { type: 'summary' }> = {
  type: 'summary',
  uid: 'req-2',
  name: 'List Orders',
  method: 'GET',
  url: 'https://api.example.com/orders',
};

function renderNode(
  itemData: Extract<CollectionItem, { type: 'request' } | { type: 'summary' }>,
  path: string,
) {
  return render(
    <RequestNode
      uid={itemData.uid}
      name={itemData.name}
      method={itemData.method}
      collectionName='my-api'
      collectionRoot='/workspace/collections/my-api'
      path={path}
      itemData={itemData}
      summaries={[]}
      onMove={vi.fn()}
      onDelete={vi.fn()}
      onDuplicate={vi.fn()}
    />,
  );
}

describe('RequestNode click-to-open', () => {
  beforeEach(() => {
    // This file does multiple click + waitFor round-trips per test, which is
    // more sensitive than most to scheduler load (e.g. a concurrent cargo
    // build). Give it more headroom than the default 5000ms.
    vi.setConfig({ testTimeout: 10000 });
    const leaf = createDefaultLeaf();
    usePaneStore.setState({ root: leaf, activeGroupId: leaf.groupId });
    vi.mocked(tauriApi.getRequest).mockReset();
    vi.mocked(toast.error).mockReset();
  });

  it('opens a tab directly from a full request item without calling getRequest', async () => {
    renderNode(fullItem, 'users.yml');
    const user = userEvent.setup();
    await user.click(screen.getByLabelText('Open GET Get Users'));

    await waitFor(() => {
      expect(findTabInTree(usePaneStore.getState().root, 'req-1')).not.toBeNull();
    });
    expect(tauriApi.getRequest).not.toHaveBeenCalled();
  });

  it('fetches the full request on demand when opening a summary item', async () => {
    vi.mocked(tauriApi.getRequest).mockResolvedValue({
      uid: 'req-2',
      name: 'List Orders',
      method: 'GET',
      url: 'https://api.example.com/orders',
      headers: [{ key: 'X-Order', value: 'abc', enabled: true }],
      auth: { authType: 'none' },
    });
    renderNode(summaryItem, 'orders.yml');
    const user = userEvent.setup();
    await user.click(screen.getByLabelText('Open GET List Orders'));

    await waitFor(() => {
      expect(tauriApi.getRequest).toHaveBeenCalledWith('my-api', 'orders.yml');
    });
    await waitFor(() => {
      const found = findTabInTree(usePaneStore.getState().root, 'req-2');
      expect(found?.tab.tabType).toBe('request');
      const headers = found?.tab.tabType === 'request' ? found.tab.request.headers : [];
      expect(headers.some((h) => h.key === 'X-Order' && h.value === 'abc')).toBe(true);
    });
  });

  it('does not open a tab when the on-demand fetch fails', async () => {
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    vi.mocked(tauriApi.getRequest).mockRejectedValue(new Error('not found'));
    renderNode(summaryItem, 'orders.yml');
    const user = userEvent.setup();
    await user.click(screen.getByLabelText('Open GET List Orders'));

    await waitFor(() => {
      expect(consoleError).toHaveBeenCalled();
    });
    expect(toast.error).toHaveBeenCalledWith('Could not open "List Orders": not found');
    expect(findTabInTree(usePaneStore.getState().root, 'req-2')).toBeNull();
    consoleError.mockRestore();
  });

  it('shows the raw backend message when the fetch rejects with a string', async () => {
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    // Tauri invoke rejects with the serialized DomainError string, not an Error.
    vi.mocked(tauriApi.getRequest).mockRejectedValue('Not found: my-api/orders.yml');
    renderNode(summaryItem, 'orders.yml');
    const user = userEvent.setup();
    await user.click(screen.getByLabelText('Open GET List Orders'));

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith(
        'Could not open "List Orders": Not found: my-api/orders.yml',
      );
    });
    consoleError.mockRestore();
  });

  it('does not duplicate a tab on a rapid double-click of the same summary item', async () => {
    // Hold each fetch open so both clicks are in flight before either resolves.
    // Collect every resolver rather than overwriting a single variable, so both
    // in-flight promises can actually be settled below.
    const resolveFetches: Array<(value: Awaited<ReturnType<typeof tauriApi.getRequest>>) => void> =
      [];
    vi.mocked(tauriApi.getRequest).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveFetches.push(resolve);
        }),
    );
    renderNode(summaryItem, 'orders.yml');
    const user = userEvent.setup();
    const row = screen.getByLabelText('Open GET List Orders');
    await user.click(row);
    await user.click(row);

    await waitFor(() => {
      expect(tauriApi.getRequest).toHaveBeenCalledTimes(2);
    });
    expect(resolveFetches).toHaveLength(2);
    // Resolve both in-flight fetches with the same request payload, so the
    // test exercises the real dedup-by-id path in openTab/findTabInTree.
    for (const resolveFetch of resolveFetches) {
      resolveFetch({
        uid: 'req-2',
        name: 'List Orders',
        method: 'GET',
        url: 'https://api.example.com/orders',
        headers: [],
        auth: { authType: 'none' },
      });
    }
    await waitFor(() => {
      expect(findTabInTree(usePaneStore.getState().root, 'req-2')).not.toBeNull();
    });
    const leaf = usePaneStore.getState().root;
    expect(leaf.type).toBe('leaf');
    expect(leaf.type === 'leaf' ? leaf.tabs.length : -1).toBe(1);
  }, 10000);

  it('focuses an already-open tab without fetching again', async () => {
    vi.mocked(tauriApi.getRequest).mockResolvedValue({
      uid: 'req-2',
      name: 'List Orders',
      method: 'GET',
      url: 'https://api.example.com/orders',
      headers: [],
      auth: { authType: 'none' },
    });
    renderNode(summaryItem, 'orders.yml');
    const user = userEvent.setup();
    const row = screen.getByLabelText('Open GET List Orders');
    await user.click(row);
    await waitFor(() => {
      expect(findTabInTree(usePaneStore.getState().root, 'req-2')).not.toBeNull();
    });
    expect(tauriApi.getRequest).toHaveBeenCalledTimes(1);

    await user.click(row);

    expect(tauriApi.getRequest).toHaveBeenCalledTimes(1);
    const leaf = usePaneStore.getState().root;
    expect(leaf.type === 'leaf' ? leaf.tabs.length : -1).toBe(1);
    expect(leaf.type === 'leaf' ? leaf.activeTabId : null).toBe('req-2');
  });

  it('does not let a stale fetch from an earlier click on another row steal focus back', async () => {
    // Two different rows, each keyed by its own request path, so each fetch can be
    // resolved independently and out of click order.
    const deferred: Record<
      string,
      (value: Awaited<ReturnType<typeof tauriApi.getRequest>>) => void
    > = {};
    vi.mocked(tauriApi.getRequest).mockImplementation(
      (_collection, path) =>
        new Promise((resolve) => {
          deferred[path] = resolve;
        }),
    );

    const itemA: Extract<CollectionItem, { type: 'request' } | { type: 'summary' }> = {
      type: 'summary',
      uid: 'req-a',
      name: 'Request A',
      method: 'GET',
      url: 'https://api.example.com/a',
    };
    const itemB: Extract<CollectionItem, { type: 'request' } | { type: 'summary' }> = {
      type: 'summary',
      uid: 'req-b',
      name: 'Request B',
      method: 'GET',
      url: 'https://api.example.com/b',
    };

    render(
      <>
        <RequestNode
          uid={itemA.uid}
          name={itemA.name}
          method={itemA.method}
          collectionName='my-api'
          collectionRoot='/workspace/collections/my-api'
          path='a.yml'
          itemData={itemA}
          summaries={[]}
          onMove={vi.fn()}
          onDelete={vi.fn()}
          onDuplicate={vi.fn()}
        />
        <RequestNode
          uid={itemB.uid}
          name={itemB.name}
          method={itemB.method}
          collectionName='my-api'
          collectionRoot='/workspace/collections/my-api'
          path='b.yml'
          itemData={itemB}
          summaries={[]}
          onMove={vi.fn()}
          onDelete={vi.fn()}
          onDuplicate={vi.fn()}
        />
      </>,
    );

    const user = userEvent.setup();
    // Click row A first (its fetch will be the slow one), then row B before A resolves.
    await user.click(screen.getByLabelText('Open GET Request A'));
    await user.click(screen.getByLabelText('Open GET Request B'));

    await waitFor(() => {
      expect(tauriApi.getRequest).toHaveBeenCalledTimes(2);
    });

    // B's fetch (the newer click) resolves first — it should open and take focus.
    deferred['b.yml']({
      uid: 'req-b',
      name: 'Request B',
      method: 'GET',
      url: 'https://api.example.com/b',
      headers: [],
      auth: { authType: 'none' },
    });
    await waitFor(() => {
      expect(findTabInTree(usePaneStore.getState().root, 'req-b')).not.toBeNull();
    });
    const leafAfterB = usePaneStore.getState().root;
    expect(leafAfterB.type === 'leaf' ? leafAfterB.activeTabId : null).toBe('req-b');

    // A's stale fetch (from the earlier click) resolves last — it must not steal focus.
    deferred['a.yml']({
      uid: 'req-a',
      name: 'Request A',
      method: 'GET',
      url: 'https://api.example.com/a',
      headers: [],
      auth: { authType: 'none' },
    });
    // Let A's now-resolved awaits run to completion before asserting nothing changed.
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(findTabInTree(usePaneStore.getState().root, 'req-a')).toBeNull();
    const leafFinal = usePaneStore.getState().root;
    expect(leafFinal.type === 'leaf' ? leafFinal.activeTabId : null).toBe('req-b');
  });

  it('does not create a split pane when the on-demand fetch fails for "Open to right"', async () => {
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    vi.mocked(tauriApi.getRequest).mockRejectedValue(new Error('not found'));
    renderNode(summaryItem, 'orders.yml');
    const user = userEvent.setup();

    await user.click(screen.getByLabelText('Actions for List Orders'));
    await user.click(await screen.findByText('Open to right'));

    await waitFor(() => {
      expect(consoleError).toHaveBeenCalled();
    });
    expect(toast.error).toHaveBeenCalledWith('Could not open "List Orders": not found');
    // The pane tree must still be a single leaf — no split was created for a tab
    // that never successfully loaded.
    expect(usePaneStore.getState().root.type).toBe('leaf');
    expect(findTabInTree(usePaneStore.getState().root, 'req-2')).toBeNull();
    consoleError.mockRestore();
  });

  it('does not refetch or open an extra split pane when "Open to right" targets an already-open request', async () => {
    vi.mocked(tauriApi.getRequest).mockResolvedValue({
      uid: 'req-2',
      name: 'List Orders',
      method: 'GET',
      url: 'https://api.example.com/orders',
      headers: [],
      auth: { authType: 'none' },
    });
    renderNode(summaryItem, 'orders.yml');
    const user = userEvent.setup();
    const row = screen.getByLabelText('Open GET List Orders');
    await user.click(row);
    await waitFor(() => {
      expect(findTabInTree(usePaneStore.getState().root, 'req-2')).not.toBeNull();
    });
    expect(tauriApi.getRequest).toHaveBeenCalledTimes(1);

    await user.click(screen.getByLabelText('Actions for List Orders'));
    await user.click(await screen.findByText('Open to right'));

    // Give an (incorrect) fetch/split from the menu action a chance to run before
    // asserting it did not happen.
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(tauriApi.getRequest).toHaveBeenCalledTimes(1);
    // Still a single leaf — no empty split pane was created for the already-open tab.
    expect(usePaneStore.getState().root.type).toBe('leaf');
  });
});

describe('RequestNode graphql items', () => {
  const gqlSummary: Extract<CollectionItem, { type: 'request' } | { type: 'summary' }> = {
    type: 'summary',
    uid: 'g-1',
    name: 'List Users',
    method: 'POST',
    url: 'https://api.example.com/graphql',
    kind: 'graphql',
  };

  beforeEach(() => {
    const leaf = createDefaultLeaf();
    usePaneStore.setState({ root: leaf, activeGroupId: leaf.groupId });
    vi.mocked(tauriApi.getRequest).mockReset();
    vi.mocked(tauriApi.getGraphQlRequest).mockReset();
  });

  it('shows a GQL badge instead of the method', () => {
    renderNode(gqlSummary, 'list-users.yml');
    expect(screen.getByText('GQL')).toBeTruthy();
    expect(screen.queryByText('POST')).toBeNull();
  });

  it('opens through getGraphQlRequest and yields a graphql tab', async () => {
    vi.mocked(tauriApi.getGraphQlRequest).mockResolvedValue({
      uid: 'g-1',
      name: 'List Users',
      method: 'POST',
      url: 'https://api.example.com/graphql',
      headers: [],
      auth: { authType: 'none' },
      body: { query: '{ users { id } }' },
    });
    renderNode(gqlSummary, 'list-users.yml');
    await userEvent.click(screen.getByLabelText('Open GQL List Users'));
    await waitFor(() => {
      const found = findTabInTree(usePaneStore.getState().root, 'g-1');
      expect(found).not.toBeNull();
    });
    const tab = findTabInTree(usePaneStore.getState().root, 'g-1')?.tab;
    expect(tab && 'request' in tab && tab.request.requestType).toBe('graphql');
    expect(tauriApi.getRequest).not.toHaveBeenCalled();
  });

  it('is not draggable into a Flow, because Flow requests are HTTP only', () => {
    renderNode(gqlSummary, 'list-users.yml');
    expect(screen.getByTestId('request-item-GQL-List Users').getAttribute('draggable')).toBe(
      'false',
    );
  });
});
