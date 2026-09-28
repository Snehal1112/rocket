import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { type Collection, getCollection } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

// The Request editor pulls in Monaco, which jsdom cannot load.
vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn().mockResolvedValue([]),
    listFlows: vi.fn().mockResolvedValue([]),
    getCollection: vi.fn(),
  };
});

vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn() } }));

vi.mock('@/components/editor', () => ({
  SingleLineEditor: (props: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
  }) => (
    <input
      aria-label={props['aria-label']}
      value={props.value}
      onChange={(e) => props.onChange(e.target.value)}
    />
  ),
}));

// jsdom has no DOMMatrixReadOnly, which React Flow reads when it re-measures nodes.
// Radix Select calls pointer-capture and scrollIntoView APIs that jsdom lacks.
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.releasePointerCapture ??= () => undefined;
Element.prototype.scrollIntoView ??= () => undefined;

vi.stubGlobal(
  'DOMMatrixReadOnly',
  class {
    m22 = 1;
  },
);

// jsdom reports every rect as 0,0,0,0 and userEvent clicks at 0,0, so the resize
// handle would count as hit by every click and steal focus from the fields.
// Park the handle away from the pointer.
const realGetRect = Element.prototype.getBoundingClientRect;
Element.prototype.getBoundingClientRect = function getRect() {
  if (this instanceof HTMLElement && this.dataset.slot === 'resizable-handle') {
    return new DOMRect(5000, 5000, 1, 100);
  }
  return realGetRect.call(this);
};

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
    ],
  },
};

const baseTab: FlowTab = {
  id: 'flow-focus-1',
  tabType: 'flow',
  title: 'Flow: focus',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'focus',
  nodes: [
    {
      id: 'empty',
      position: { x: 0, y: 0 },
      kind: {
        kind: 'Request',
        label: 'Empty',
        source: { type: 'Inline', request: { method: 'GET', url: '', headers: [] } },
      },
    },
    {
      id: 'filled',
      position: { x: 300, y: 0 },
      kind: {
        kind: 'Request',
        label: 'Filled',
        source: {
          type: 'Inline',
          request: {
            method: 'POST',
            url: 'https://x.test/a',
            headers: [
              { name: 'A', value: '1' },
              { name: 'B', value: '2' },
            ],
          },
        },
      },
    },
  ],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
};

function getFlowTab(): FlowTab {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected root to be a leaf');
  const tab = root.tabs.find((t) => t.id === baseTab.id);
  if (!tab || !isFlowTab(tab)) throw new Error('Expected the seeded flow tab');
  return tab;
}

// FlowPane receives the tab as a prop. Re-render it from the store after each
// store change, the way PaneRenderer does in the app.
function Harness() {
  const tab = usePaneStore((s) => {
    const root = s.root;
    if (root.type !== 'leaf') return null;
    const t = root.tabs.find((x) => x.id === baseTab.id);
    return t && isFlowTab(t) ? t : null;
  });
  if (!tab) return null;
  return <FlowPane tab={tab} groupId={usePaneStore.getState().activeGroupId} />;
}

function nodeIds() {
  return getFlowTab().nodes.map((n) => n.id);
}

// Waits until focus sits inside the properties panel.
async function expectFocusInPanel() {
  await waitFor(() =>
    expect(screen.getByTestId('node-properties-panel').contains(document.activeElement)).toBe(true),
  );
}

describe('FlowPane Request node panel keeps focus', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
    vi.mocked(getCollection).mockReset();
    vi.mocked(getCollection).mockResolvedValue(collection);
  });

  it('keeps the node on Backspace after cancelling a use-saved confirmation', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByLabelText('Edit Filled'));
    await user.click(screen.getByRole('button', { name: 'Use a saved request…' }));
    await user.click(await screen.findByRole('button', { name: /Login/ }));
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    await expectFocusInPanel();
    await user.keyboard('{Backspace}');
    expect(nodeIds()).toContain('filled');
  });

  it('keeps the node on Backspace after removing the last header', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByLabelText('Edit Filled'));
    await user.click(screen.getByRole('button', { name: 'Remove header 2' }));
    await expectFocusInPanel();
    await user.keyboard('{Backspace}');
    expect(nodeIds()).toContain('filled');
  });

  it('keeps the node on Backspace after an immediate saved pick', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByLabelText('Edit Empty'));
    await user.click(screen.getByRole('button', { name: 'Use a saved request…' }));
    await user.click(await screen.findByRole('button', { name: /Login/ }));
    expect(await screen.findByTestId('saved-request-path')).toHaveTextContent('login.yml');
    await expectFocusInPanel();
    await user.keyboard('{Backspace}');
    expect(nodeIds()).toContain('empty');
  });

  it('Backspace in the URL field edits text and keeps the node', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByLabelText('Edit Filled'));
    const url = screen.getByLabelText('URL');
    await user.click(url);
    await user.keyboard('{Backspace}');
    expect(url).toHaveValue('https://x.test/');
    expect(nodeIds()).toContain('filled');
  });

  it('marks the request picker popover as nokey', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByLabelText('Edit Empty'));
    await user.click(screen.getByRole('button', { name: 'Use a saved request…' }));
    const content = document.body.querySelector('[data-slot="popover-content"]');
    expect(content).toHaveClass('nokey');
  });

  it('marks the Method select content as nokey', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByLabelText('Edit Empty'));
    await user.click(screen.getByRole('combobox', { name: 'Method' }));
    await screen.findByRole('option', { name: 'PUT' });
    const content = document.body.querySelector('[data-slot="select-content"]');
    expect(content).toHaveClass('nokey');
  });
});
