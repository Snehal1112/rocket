import { act, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FlowNode } from '@/lib/tauri-api';

const getRequest = vi.fn();
vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  getRequest: (...args: unknown[]) => getRequest(...args),
  onCollectionChanged: vi.fn(() => Promise.resolve(() => undefined)),
}));

import {
  clearSavedRequestPreviewCache,
  peekSavedRequestPreview,
} from '@/lib/saved-request-preview';
import { FlowCanvas } from '../FlowCanvas';

const saved = (id: string): FlowNode => ({
  id,
  kind: {
    kind: 'Request',
    label: `Login ${id}`,
    source: { type: 'Saved', requestPath: 'auth/login.yml' },
  },
  position: { x: 0, y: Number(id.slice(1)) * 200 },
});

function renderCanvas(nodes: FlowNode[]) {
  render(
    <FlowCanvas
      nodes={nodes}
      edges={[]}
      nodeStatus={{}}
      flowCollectionName='demo'
      onNodesChange={vi.fn()}
      onEdgesChange={vi.fn()}
      onConnect={vi.fn()}
    />,
  );
}

describe('saved Request cards', () => {
  beforeEach(() => {
    clearSavedRequestPreviewCache();
    getRequest.mockReset();
  });

  it('show the saved method, header count and body preview', async () => {
    getRequest.mockResolvedValue({
      uid: 'u',
      name: 'Login',
      method: 'POST',
      url: 'https://x.test/login',
      headers: [
        { key: 'A', value: '1', enabled: true },
        { key: 'B', value: '2', enabled: false },
      ],
      body: { mode: 'json', content: '{"u":1}' },
      auth: { authType: 'none' },
    });
    renderCanvas([saved('n1')]);
    expect(await screen.findByText('POST')).toBeInTheDocument();
    expect(screen.getByTestId('request-node-headers-row')).toHaveTextContent('1 set');
    expect(screen.getByTestId('request-node-card')).toHaveTextContent('{"u":1}');
  });

  it('loads each saved request once for many cards', async () => {
    getRequest.mockResolvedValue({
      uid: 'u',
      name: 'Login',
      method: 'GET',
      url: 'https://x.test',
      headers: [],
      auth: { authType: 'none' },
    });
    renderCanvas([saved('n1'), saved('n2'), saved('n3')]);
    expect(await screen.findAllByText('GET')).toHaveLength(3);
    expect(getRequest).toHaveBeenCalledTimes(1);
  });

  it('keeps the SAVED fallback when the request cannot be loaded', async () => {
    getRequest.mockRejectedValue('file not found');
    renderCanvas([saved('n1')]);
    await vi.waitFor(() =>
      expect(peekSavedRequestPreview('demo', 'auth/login.yml')?.status).toBe('error'),
    );
    expect(screen.getByText('SAVED')).toBeInTheDocument();
    expect(screen.getByTestId('request-node-headers-row')).toHaveTextContent('0 set');
    expect(screen.getByTestId('request-node-card')).toHaveTextContent('—');
  });

  it('reloads the card data after the collection cache is cleared', async () => {
    getRequest.mockResolvedValue({
      uid: 'u',
      name: 'Login',
      method: 'PATCH',
      url: 'https://x.test',
      headers: [{ key: 'A', value: '1', enabled: true }],
      auth: { authType: 'none' },
    });
    renderCanvas([saved('n1')]);
    expect(await screen.findByText('PATCH')).toBeInTheDocument();
    act(() => clearSavedRequestPreviewCache('demo'));
    await vi.waitFor(() => expect(getRequest).toHaveBeenCalledTimes(2));
    expect(await screen.findByText('PATCH')).toBeInTheDocument();
    expect(screen.getByTestId('request-node-headers-row')).toHaveTextContent('1 set');
  });
});
