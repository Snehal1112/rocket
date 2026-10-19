import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { flowKeys } from '@/lib/queries/flow-queries';
import type { FlowNode, FlowRequestPathsRewrittenEvent } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { collectAllTabs } from '@/lib/pane-utils';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { useFlowRequestPathEvents } from '../useFlowRequestPathEvents';

const mocks = vi.hoisted(() => ({
  handler: null as null | ((e: FlowRequestPathsRewrittenEvent) => void),
  unlisten: vi.fn(),
  warning: vi.fn(),
}));

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    endAgentSession: vi.fn(),
    onFlowRequestPathsRewritten: vi.fn((h: (e: FlowRequestPathsRewrittenEvent) => void) => {
      mocks.handler = h;
      return Promise.resolve(mocks.unlisten);
    }),
  };
});
vi.mock('sonner', () => ({ toast: { warning: mocks.warning } }));

const requestNode = (id: string, requestPath: string): FlowNode =>
  ({
    id,
    kind: { kind: 'Request', source: { type: 'Saved', requestPath } },
    position: { x: 0, y: 0 },
  }) as unknown as FlowNode;

const flowTab = (nodes: FlowNode[]): FlowTab => ({
  id: 't1',
  title: 'Flow: F',
  isDirty: false,
  tabType: 'flow',
  collectionName: 'c',
  flowName: 'F',
  nodes,
  edges: [],
  callbackHost: null,
  nodeStatus: {},
  runState: 'idle',
});

const tabPath = (): string | null => {
  const tab = collectAllTabs(usePaneStore.getState().root)
    .filter(isFlowTab)
    .find((t) => t.id === 't1');
  const kind = tab?.nodes[0]?.kind;
  return kind && kind.kind === 'Request' && kind.source.type === 'Saved'
    ? kind.source.requestPath
    : null;
};

function setup() {
  const client = new QueryClient();
  const invalidate = vi.spyOn(client, 'invalidateQueries');
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  const view = renderHook(() => useFlowRequestPathEvents(), { wrapper });
  return { invalidate, ...view };
}

const event = (
  patch: Partial<FlowRequestPathsRewrittenEvent>,
): FlowRequestPathsRewrittenEvent => ({
  collection: 'c',
  from: 'auth',
  to: 'identity',
  flows: [],
  failed: [],
  ...patch,
});

describe('useFlowRequestPathEvents', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.handler = null;
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(flowTab([requestNode('a', 'auth/login.yml')]));
  });

  it('an event with flows patches open tabs and refreshes the flow list', () => {
    const { invalidate } = setup();
    mocks.handler?.(event({ flows: ['A'] }));
    expect(tabPath()).toBe('identity/login.yml');
    expect(invalidate).toHaveBeenCalledWith({ queryKey: flowKeys.collection('c') });
    expect(mocks.warning).not.toHaveBeenCalled();
  });

  it('an event with only failures shows the toast and patches nothing', () => {
    const { invalidate } = setup();
    mocks.handler?.(
      event({
        failed: [
          { flow: 'A', message: 'boom' },
          { flow: 'B', message: 'moved out of the collection' },
        ],
      }),
    );
    expect(mocks.warning).toHaveBeenCalledTimes(1);
    expect(mocks.warning).toHaveBeenCalledWith(
      'Flows not updated: A (boom); B (moved out of the collection)',
    );
    expect(invalidate).not.toHaveBeenCalled();
    expect(tabPath()).toBe('auth/login.yml');
  });

  it('an event with flows and failures does both', () => {
    const { invalidate } = setup();
    mocks.handler?.(event({ flows: ['A'], failed: [{ flow: 'B', message: 'boom' }] }));
    expect(tabPath()).toBe('identity/login.yml');
    expect(invalidate).toHaveBeenCalledTimes(1);
    expect(mocks.warning).toHaveBeenCalledTimes(1);
  });

  it('the hook unsubscribes on unmount', async () => {
    const { unmount } = setup();
    unmount();
    await vi.waitFor(() => expect(mocks.unlisten).toHaveBeenCalledTimes(1));
  });
});
