import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { type Collection, getCollection, getRequest, type Request } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { SavedSourceEditor } from '../SavedSourceEditor';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn(), getRequest: vi.fn() };
});

const collection: Collection = {
  name: 'demo',
  settings: { headers: [], variables: [], sandboxMode: 'safe' },
  root: {
    uid: 'root',
    name: 'demo',
    items: [
      {
        type: 'summary',
        uid: 's1',
        name: 'Login',
        method: 'POST',
        url: '/l',
        fileName: 'login.yml',
      },
      {
        type: 'folder',
        uid: 'f1',
        name: 'Users',
        dirName: 'users',
        items: [
          {
            type: 'summary',
            uid: 's2',
            name: 'List users',
            method: 'GET',
            url: '/u',
            fileName: 'list.yml',
          },
        ],
      },
    ],
  },
};

const fullRequest: Request = {
  uid: 'req-login',
  name: 'Login',
  method: 'POST',
  url: '{{baseUrl}}/login',
  headers: [],
  auth: { authType: 'inherit' },
};

function renderEditor() {
  const onPick = vi.fn();
  const onConvertToInline = vi.fn();
  render(
    <SavedSourceEditor
      requestPath='login.yml'
      collection='demo'
      onPick={onPick}
      onConvertToInline={onConvertToInline}
      converting={false}
    />,
  );
  return { onPick, onConvertToInline };
}

describe('SavedSourceEditor', () => {
  beforeEach(() => {
    vi.mocked(getCollection).mockReset();
    vi.mocked(getRequest).mockReset();
    usePaneStore.getState().reset();
  });

  it('shows the saved request path', () => {
    renderEditor();
    expect(screen.getByTestId('saved-request-path')).toHaveTextContent('login.yml');
  });

  it('lists and filters the collection requests, and picks one', async () => {
    vi.mocked(getCollection).mockResolvedValue(collection);
    const { onPick } = renderEditor();
    await userEvent.click(screen.getByRole('button', { name: 'Choose request…' }));
    expect(await screen.findByRole('button', { name: /List users/ })).toBeInTheDocument();

    await userEvent.type(screen.getByLabelText('Filter requests'), 'list');
    expect(screen.queryByRole('button', { name: /Login/ })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: /List users/ }));
    expect(onPick).toHaveBeenCalledWith({
      path: 'users/list.yml',
      name: 'List users',
      method: 'GET',
    });
  });

  it('shows a load error with Retry', async () => {
    vi.mocked(getCollection)
      .mockRejectedValueOnce('collection not found')
      .mockResolvedValueOnce(collection);
    renderEditor();
    await userEvent.click(screen.getByRole('button', { name: 'Choose request…' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('collection not found');
    await userEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(await screen.findByRole('button', { name: /Login/ })).toBeInTheDocument();
  });

  it('opens the saved request in a request tab', async () => {
    vi.mocked(getRequest).mockResolvedValue(fullRequest);
    renderEditor();
    await userEvent.click(screen.getByRole('button', { name: 'Open request' }));
    await waitFor(() => {
      const root = usePaneStore.getState().root;
      if (root.type !== 'leaf') throw new Error('Expected a leaf');
      expect(root.tabs.some((t) => t.id === 'req-login' && t.tabType === 'request')).toBe(true);
    });
    expect(getRequest).toHaveBeenCalledWith('demo', 'login.yml');
  });

  it('asks to switch collections instead of hiding the flow tab', async () => {
    vi.mocked(getRequest).mockResolvedValue(fullRequest);
    usePaneStore.getState().switchCollection('other');
    usePaneStore.getState().openTab({
      id: 'flow-1',
      tabType: 'flow',
      title: 'Flow: f',
      isDirty: true,
      collectionName: 'demo',
      flowName: 'f',
      nodes: [],
      edges: [],
      nodeStatus: {},
      runState: 'idle',
    });
    renderEditor();
    await userEvent.click(screen.getByRole('button', { name: 'Open request' }));
    expect(await screen.findByRole('status')).toHaveTextContent(
      'This request is in collection "demo". Switch to that collection to open it.',
    );
    const state = usePaneStore.getState();
    expect(state.activeCollection).toBe('other');
    if (state.root.type !== 'leaf') throw new Error('Expected a leaf');
    expect(state.root.tabs.map((t) => t.id)).toEqual(['flow-1']);
  });

  it('reports a request that cannot be opened', async () => {
    vi.mocked(getRequest).mockRejectedValue('file not found');
    renderEditor();
    await userEvent.click(screen.getByRole('button', { name: 'Open request' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Could not open the request: file not found',
    );
  });

  it('asks the parent to convert to inline', async () => {
    const { onConvertToInline } = renderEditor();
    await userEvent.click(screen.getByRole('button', { name: 'Convert to inline' }));
    expect(onConvertToInline).toHaveBeenCalledTimes(1);
  });
});
