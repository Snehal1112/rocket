import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { listCollections, listFlows } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { FlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn(),
    listFlows: vi.fn(),
    saveFlow: vi.fn(),
    runFlow: vi.fn(),
    onFlowRunStarted: vi.fn(),
    onFlowStepStarted: vi.fn(),
    onFlowStepCompleted: vi.fn(),
    onFlowStepProgress: vi.fn(),
  };
});
vi.mock('@/lib/execute-request', () => ({ getActiveGlobalEnvName: vi.fn() }));
vi.mock('@/lib/flow-auth-preflight', () => ({ collectFlowAuthTokens: vi.fn(async () => ({})) }));
vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() } }));

// Radix menus call pointer-capture and scrollIntoView APIs that jsdom lacks.
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.releasePointerCapture ??= () => undefined;
Element.prototype.scrollIntoView ??= () => undefined;

const flowTab: FlowTab = {
  id: 'flow-export-1',
  title: 'Flow: my-flow',
  isDirty: false,
  tabType: 'flow',
  collectionName: 'demo',
  flowName: 'my-flow',
  nodes: [{ id: 'a', kind: { kind: 'Output', label: 'Out a' }, position: { x: 0, y: 0 } }],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
};

describe('FlowPane export menu', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
    vi.mocked(listCollections).mockResolvedValue([]);
    vi.mocked(listFlows).mockResolvedValue([]);
  });

  it('shows an Export menu beside Save, with the report items off before a run', async () => {
    usePaneStore.getState().openTab(flowTab);
    render(<FlowPane tab={flowTab} groupId={usePaneStore.getState().activeGroupId} />);

    expect(screen.getByRole('button', { name: 'Save' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Export' }));
    expect(await screen.findByRole('menuitem', { name: 'Copy flow JSON' })).toBeInTheDocument();
    expect(screen.getByRole('menuitem', { name: 'Export run report (JSON)' })).toHaveAttribute(
      'aria-disabled',
      'true',
    );
  });
});
