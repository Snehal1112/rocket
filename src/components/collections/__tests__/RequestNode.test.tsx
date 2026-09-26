import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
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
  };
});

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
    const leaf = createDefaultLeaf();
    usePaneStore.setState({ root: leaf, activeGroupId: leaf.groupId });
    vi.mocked(tauriApi.getRequest).mockReset();
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
    expect(findTabInTree(usePaneStore.getState().root, 'req-2')).toBeNull();
    consoleError.mockRestore();
  });

  it('does not duplicate a tab on a rapid double-click of the same summary item', async () => {
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
    await user.click(row);

    await waitFor(() => {
      expect(tauriApi.getRequest).toHaveBeenCalledTimes(2);
    });
    const leaf = usePaneStore.getState().root;
    expect(leaf.type).toBe('leaf');
    expect(leaf.type === 'leaf' ? leaf.tabs.length : -1).toBe(1);
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
    // The pane tree must still be a single leaf — no split was created for a tab
    // that never successfully loaded.
    expect(usePaneStore.getState().root.type).toBe('leaf');
    expect(findTabInTree(usePaneStore.getState().root, 'req-2')).toBeNull();
    consoleError.mockRestore();
  });
});
