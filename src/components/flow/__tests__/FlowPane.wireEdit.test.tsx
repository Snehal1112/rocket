import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { FlowEdge } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

// Monaco cannot load in jsdom, so a textarea stands in for the editor.
vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: ({ value, onChange }: { value: string; onChange?: (v: string) => void }) => (
    <textarea aria-label='Wire script' value={value} onChange={(e) => onChange?.(e.target.value)} />
  ),
}));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn().mockResolvedValue([]),
    listFlows: vi.fn().mockResolvedValue([]),
    saveFlow: vi.fn().mockResolvedValue(undefined),
  };
});

vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn() } }));

const realGetRect = Element.prototype.getBoundingClientRect;

function makeTab(edge: FlowEdge): FlowTab {
  return {
    id: 'flow-wire-1',
    tabType: 'flow',
    title: 'Flow: wire',
    isDirty: false,
    collectionName: 'demo',
    flowName: 'wire',
    nodes: [
      {
        id: 'in1',
        position: { x: 0, y: 0 },
        kind: { kind: 'Input', label: 'User', value: 'alice' },
      },
      {
        id: 'req1',
        position: { x: 300, y: 0 },
        kind: {
          kind: 'Request',
          label: 'Login',
          source: {
            type: 'Inline',
            request: { method: 'GET', url: '', headers: [], body: undefined },
          },
        },
      },
    ],
    edges: [edge],
    nodeStatus: {},
    runState: 'idle',
  };
}

function edgeOf(targetField: string, expression = 'response.body'): FlowEdge {
  return { id: 'e1', sourceNodeId: 'in1', targetNodeId: 'req1', targetField, expression };
}

function getEdges(): FlowEdge[] {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected root to be a leaf');
  const tab = root.tabs.find((t) => t.id === 'flow-wire-1');
  if (!tab || !isFlowTab(tab)) throw new Error('Expected the seeded flow tab');
  return tab.edges;
}

function Harness() {
  const tab = usePaneStore((s) => {
    if (s.root.type !== 'leaf') return null;
    const t = s.root.tabs.find((x) => x.id === 'flow-wire-1');
    return t && isFlowTab(t) ? t : null;
  });
  if (!tab) return null;
  return <FlowPane tab={tab} groupId={usePaneStore.getState().activeGroupId} />;
}

async function doubleClickWire() {
  await waitFor(() => expect(document.querySelector('.react-flow__edge')).not.toBeNull());
  fireEvent.doubleClick(document.querySelector('.react-flow__edge') as Element);
}

describe('FlowPane wire editing', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(200);
    vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(80);
    vi.stubGlobal(
      'DOMMatrixReadOnly',
      class {
        m22 = 1;
      },
    );
    vi.stubGlobal(
      'ResizeObserver',
      class {
        constructor(private cb: ResizeObserverCallback) {}
        observe(target: Element) {
          // Only node elements matter here; other observers are left idle.
          if (!target.classList.contains('react-flow__node')) return;
          this.cb([{ target } as ResizeObserverEntry], this as unknown as ResizeObserver);
        }
        unobserve() {
          // Not needed by these tests.
        }
        disconnect() {
          // Not needed by these tests.
        }
      },
    );
    // Park the resize handle away from the pointer, as the other pane tests do.
    Element.prototype.getBoundingClientRect = function getRect() {
      if (this instanceof HTMLElement && this.dataset.slot === 'resizable-handle') {
        return new DOMRect(5000, 5000, 1, 100);
      }
      return realGetRect.call(this);
    };
  });

  afterEach(() => {
    Element.prototype.getBoundingClientRect = realGetRect;
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  const seed = (edge: FlowEdge) => usePaneStore.getState().openTab(makeTab(edge));

  it('reopens a data wire and saves an edited script under the same id', async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    seed(edgeOf('url'));
    render(<Harness />);
    await doubleClickWire();
    const editor = await screen.findByLabelText('Wire script');
    expect(editor).toHaveValue('response.body');
    await user.clear(editor);
    await user.type(editor, 'response.body.url');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(getEdges()[0].expression).toBe('response.body.url'));
    expect(getEdges()[0].id).toBe('e1');
    expect(getEdges()).toHaveLength(1);
    await waitFor(() => expect(document.activeElement).not.toBe(document.body));
  });

  it('does not delete the edited wire on a Backspace after the dialog closes', async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    seed(edgeOf('url'));
    render(<Harness />);
    await waitFor(() => expect(document.querySelector('.react-flow__edge')).not.toBeNull());
    const edgeEl = document.querySelector('.react-flow__edge') as Element;
    fireEvent.click(edgeEl);
    fireEvent.doubleClick(edgeEl);
    await screen.findByLabelText('Wire script');
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByLabelText('Wire script')).not.toBeInTheDocument());
    await user.keyboard('{Backspace}');
    expect(getEdges()).toHaveLength(1);
  });

  it('opens no dialog for a Run when wire', async () => {
    seed(edgeOf('trigger', ''));
    render(<Harness />);
    await doubleClickWire();
    expect(screen.queryByLabelText('Wire script')).not.toBeInTheDocument();
  });

  it('keeps an existing headers wire when the dialog is cancelled', async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    seed(edgeOf('headers[X-Token].value', 'response.body.token'));
    render(<Harness />);
    await doubleClickWire();
    await screen.findByLabelText('Wire script');
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByLabelText('Wire script')).not.toBeInTheDocument());
    expect(getEdges()).toEqual([edgeOf('headers[X-Token].value', 'response.body.token')]);
    await waitFor(() => expect(document.activeElement).not.toBe(document.body));
  });
});
