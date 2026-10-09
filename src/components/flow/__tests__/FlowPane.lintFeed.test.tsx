import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { lintFlow } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

vi.mock('@/hooks/useCollectionVariableContext', () => ({
  useCollectionVariableContext: () => ({ variableContext: new Map() }),
}));

// The Request editor pulls in Monaco, which jsdom cannot load.
vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    lintFlow: vi.fn(),
    listCollections: vi.fn().mockResolvedValue([]),
    listFlows: vi.fn().mockResolvedValue([]),
    saveFlow: vi.fn().mockResolvedValue(undefined),
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

// Radix menus call pointer-capture and scrollIntoView APIs that jsdom lacks.
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.releasePointerCapture ??= () => undefined;
Element.prototype.scrollIntoView ??= () => undefined;

// jsdom has no DOMMatrixReadOnly, which React Flow reads when it re-measures nodes.
vi.stubGlobal(
  'DOMMatrixReadOnly',
  class {
    m22 = 1;
  },
);

// Park the resize handle away from the pointer, as the properties test does.
const realGetRect = Element.prototype.getBoundingClientRect;
Element.prototype.getBoundingClientRect = function getRect() {
  if (this instanceof HTMLElement && this.dataset.slot === 'resizable-handle') {
    return new DOMRect(5000, 5000, 1, 100);
  }
  return realGetRect.call(this);
};

const baseTab: FlowTab = {
  id: 'flow-lint-1',
  tabType: 'flow',
  title: 'Flow: lint',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'lint',
  nodes: [
    { id: 'in1', position: { x: 0, y: 0 }, kind: { kind: 'Input', label: 'User', value: 'alice' } },
    { id: 'out1', position: { x: 300, y: 0 }, kind: { kind: 'Output', label: 'Result' } },
    { id: 'out2', position: { x: 300, y: 200 }, kind: { kind: 'Output', label: 'Other' } },
    {
      id: 'req1',
      position: { x: 0, y: 200 },
      kind: {
        kind: 'Request',
        label: 'Login',
        source: { type: 'Saved', requestPath: 'login.yml' },
      },
    },
  ],
  edges: [
    {
      id: 'e1',
      sourceNodeId: 'in1',
      targetNodeId: 'out1',
      targetField: 'value',
      expression: 'response.body',
    },
    {
      id: 'e2',
      sourceNodeId: 'in1',
      targetNodeId: 'out2',
      targetField: 'value',
      expression: 'response.body',
    },
  ],
  nodeStatus: {},
  runState: 'idle',
};

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

describe('FlowPane backend lint feed', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
  });

  it('lints the open flow and shows the backend warning', async () => {
    vi.mocked(lintFlow).mockResolvedValue([
      {
        code: 'no_path_to_output',
        severity: 'warning',
        nodeId: 'req1',
        message: "'Login' does not lead to an Output.",
      },
    ]);
    render(<Harness />);
    await waitFor(() =>
      expect(lintFlow).toHaveBeenCalledWith('demo', expect.objectContaining({ name: 'lint' })),
    );
    expect(await screen.findByRole('button', { name: '1 warning' })).toBeInTheDocument();
  });

  it('keeps the canvas usable when the lint call fails', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    vi.mocked(lintFlow).mockRejectedValue('lint crashed');
    render(<Harness />);
    await waitFor(() => expect(lintFlow).toHaveBeenCalled());
    await waitFor(() => expect(warn).toHaveBeenCalled());
    expect(screen.getByRole('button', { name: 'Run' })).toBeEnabled();
    expect(screen.queryByRole('button', { name: /warning|error/ })).not.toBeInTheDocument();
    warn.mockRestore();
  });

  it('still lints and shows backend issues while a past run is viewed', async () => {
    vi.mocked(lintFlow).mockResolvedValue([
      {
        code: 'no_path_to_output',
        severity: 'warning',
        nodeId: 'req1',
        message: "'Login' does not lead to an Output.",
      },
    ]);
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab({ ...baseTab, viewedRunId: 'run-old' });
    render(<Harness />);
    await waitFor(() => expect(lintFlow).toHaveBeenCalled());
    expect(await screen.findByRole('button', { name: '1 warning' })).toBeInTheDocument();
  });
});
