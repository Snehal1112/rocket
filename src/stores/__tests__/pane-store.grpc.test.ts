import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createDefaultRequestFor } from '@/lib/pane-utils';
import type { RequestTab } from '@/types/pane-types';
import { useGrpcStore } from '../grpc-store';
import { usePaneStore } from '../pane-store';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, grpcCancelSession: vi.fn().mockResolvedValue(undefined) };
});

import { grpcCancelSession } from '@/lib/tauri-api';

function tab(kind: 'grpc' | 'http'): RequestTab {
  return {
    id: crypto.randomUUID(),
    title: 'T',
    tabType: 'request',
    request: createDefaultRequestFor(kind),
    response: null,
    isDirty: false,
  };
}

function leaf() {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('expected a leaf');
  return root;
}

describe('closing a gRPC tab', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    useGrpcStore.setState({ sessions: {}, sessionByTab: {}, unaryByTab: {} });
    vi.mocked(grpcCancelSession).mockClear();
  });

  it('cancels its running stream and forgets its results', async () => {
    const t = tab('grpc');
    usePaneStore.getState().openTab(t);
    useGrpcStore.getState().attachSession(t.id, 'run-1');
    useGrpcStore.getState().setUnary(t.id, { status: 'sending' });

    usePaneStore.getState().closeTab(t.id, leaf().groupId);

    await vi.waitFor(() => expect(grpcCancelSession).toHaveBeenCalledWith('run-1'));
    await vi.waitFor(() => expect(useGrpcStore.getState().sessionByTab[t.id]).toBeUndefined());
    expect(useGrpcStore.getState().unaryByTab[t.id]).toBeUndefined();
  });

  it('leaves the gRPC store alone when an http tab closes', () => {
    const t = tab('http');
    usePaneStore.getState().openTab(t);
    usePaneStore.getState().closeTab(t.id, leaf().groupId);
    expect(grpcCancelSession).not.toHaveBeenCalled();
  });
});
