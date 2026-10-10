import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { toast } from 'sonner';
import { findScriptTab, findTabInTree } from '@/lib/pane-utils';
import { saveFlow, saveScriptFile } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { FlowTab, FolderTab } from '@/types/pane-types';
import { TabBar } from '../TabBar';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    readScriptFile: vi.fn().mockResolvedValue('x'),
    saveScriptFile: vi.fn().mockResolvedValue(undefined),
    saveFlow: vi.fn().mockResolvedValue(undefined),
    getCollection: vi.fn().mockResolvedValue({
      name: 'demo',
      settings: { headers: [], variables: [] },
      root: { uid: 'r', name: 'demo', items: [] },
    }),
  };
});

vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() } }));

describe('TabBar new-tab menu', () => {
  beforeEach(() => usePaneStore.getState().reset());

  it('opens a blank runner tab from the new-tab context menu', () => {
    const { root } = usePaneStore.getState();
    if (root.type !== 'leaf') throw new Error('Expected leaf root');
    render(<TabBar node={root} />);

    fireEvent.contextMenu(screen.getByLabelText('New request'));
    fireEvent.click(screen.getByText('Runner'));

    const updated = usePaneStore.getState().root;
    if (updated.type !== 'leaf') throw new Error('Expected leaf root');
    expect(updated.tabs.some((t) => t.tabType === 'runner')).toBe(true);
  });
});

describe('TabBar script tabs', () => {
  beforeEach(() => usePaneStore.getState().reset());

  async function renderWithScript() {
    await usePaneStore.getState().openScriptTab('col', 'lib/a.js');
    const { root } = usePaneStore.getState();
    if (root.type !== 'leaf') throw new Error('Expected leaf root');
    render(<TabBar node={root} />);
  }

  it('does not start an inline rename on double-click', async () => {
    await renderWithScript();
    fireEvent.doubleClick(screen.getByText('a.js'));
    expect(screen.queryByRole('textbox')).toBeNull();
    expect(findScriptTab(usePaneStore.getState().root, 'col', 'lib/a.js')).not.toBeNull();
  });

  it('disables Rename in the context menu', async () => {
    await renderWithScript();
    fireEvent.contextMenu(screen.getByText('a.js'));
    const item = screen.getByText('Rename').closest('[role="menuitem"]');
    expect(item).toHaveAttribute('data-disabled');
  });
});

const dirtyFlow: FlowTab = {
  id: 'flow-tb-1',
  title: 'Flow: my-flow',
  isDirty: true,
  tabType: 'flow',
  collectionName: 'demo',
  flowName: 'my-flow',
  nodes: [],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
};

describe('TabBar Save menu item', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
  });

  function leaf() {
    const { root } = usePaneStore.getState();
    if (root.type !== 'leaf') throw new Error('Expected leaf root');
    return root;
  }

  it('shows a disabled Save, not "Save to collection...", on a clean script tab', async () => {
    await usePaneStore.getState().openScriptTab('col', 'lib/a.js');
    render(<TabBar node={leaf()} />);
    fireEvent.contextMenu(screen.getByText('a.js'));
    expect(screen.queryByText('Save to collection...')).toBeNull();
    expect(screen.getByText('Save').closest('[role="menuitem"]')).toHaveAttribute('data-disabled');
  });

  it('saves a dirty script tab that is not the active one', async () => {
    await usePaneStore.getState().openScriptTab('col', 'lib/a.js');
    const first = findScriptTab(usePaneStore.getState().root, 'col', 'lib/a.js')?.tab;
    if (!first) throw new Error('missing tab');
    usePaneStore.getState().updateScriptContent(first.id, 'two');
    await usePaneStore.getState().openScriptTab('col', 'lib/b.js');
    render(<TabBar node={leaf()} />);

    fireEvent.contextMenu(screen.getByText('a.js'));
    fireEvent.click(screen.getByText('Save'));

    await waitFor(() => expect(saveScriptFile).toHaveBeenCalledWith('col', 'lib/a.js', 'two'));
    const second = findScriptTab(usePaneStore.getState().root, 'col', 'lib/b.js')?.tab;
    expect(leaf().activeTabId).toBe(second?.id);
    await waitFor(() =>
      expect(findScriptTab(usePaneStore.getState().root, 'col', 'lib/a.js')?.tab.isDirty).toBe(
        false,
      ),
    );
  });

  it('saves a dirty flow tab that is not the active one', async () => {
    // Opening a script tab in another collection parks flow tabs, so open the flow second.
    await usePaneStore.getState().openScriptTab('col', 'lib/b.js');
    const second = findScriptTab(usePaneStore.getState().root, 'col', 'lib/b.js')?.tab;
    if (!second) throw new Error('missing tab');
    usePaneStore.getState().openTab(dirtyFlow);
    usePaneStore.getState().setActiveTab(second.id, leaf().groupId);
    render(<TabBar node={leaf()} />);

    fireEvent.contextMenu(screen.getByText('Flow: my-flow'));
    fireEvent.click(screen.getByText('Save'));

    await waitFor(() =>
      expect(saveFlow).toHaveBeenCalledWith('demo', { name: 'my-flow', nodes: [], edges: [] }),
    );
    await waitFor(() =>
      expect(findTabInTree(usePaneStore.getState().root, 'flow-tb-1')?.tab.isDirty).toBe(false),
    );
    expect(leaf().activeTabId).toBe(second.id);
  });

  it('masks a case value in the toast when an inactive flow save fails', async () => {
    vi.mocked(toast.error).mockClear();
    await usePaneStore.getState().openScriptTab('col', 'lib/b.js');
    const second = findScriptTab(usePaneStore.getState().root, 'col', 'lib/b.js')?.tab;
    if (!second) throw new Error('missing tab');
    usePaneStore.getState().openTab(dirtyFlow);
    usePaneStore.getState().setActiveTab(second.id, leaf().groupId);
    vi.mocked(saveFlow).mockRejectedValueOnce(
      new Error(
        "Invalid input: flow is invalid: more than one case matches 'secret-token' — node(s): sw; edge(s): ",
      ),
    );
    render(<TabBar node={leaf()} />);

    fireEvent.contextMenu(screen.getByText('Flow: my-flow'));
    fireEvent.click(screen.getByText('Save'));

    await waitFor(() => expect(toast.error).toHaveBeenCalled());
    const message = String(vi.mocked(toast.error).mock.lastCall?.[0]);
    expect(message).not.toContain('secret-token');
    expect(message).toContain('value hidden');
  });

  it('skips the save of an inactive tab that was cleaned after the menu opened', async () => {
    await usePaneStore.getState().openScriptTab('col', 'lib/a.js');
    const first = findScriptTab(usePaneStore.getState().root, 'col', 'lib/a.js')?.tab;
    if (!first) throw new Error('missing tab');
    usePaneStore.getState().updateScriptContent(first.id, 'two');
    await usePaneStore.getState().openScriptTab('col', 'lib/b.js');
    render(<TabBar node={leaf()} />);

    fireEvent.contextMenu(screen.getByText('a.js'));
    // Another save finishes while the menu is open. The rendered tab object is stale.
    act(() => usePaneStore.getState().markScriptSaved(first.id, 'two'));
    fireEvent.click(screen.getByText('Save'));

    await Promise.resolve();
    expect(saveScriptFile).not.toHaveBeenCalled();
  });

  it('disables Save on a clean flow tab', () => {
    usePaneStore.getState().openTab({ ...dirtyFlow, isDirty: false });
    render(<TabBar node={leaf()} />);
    fireEvent.contextMenu(screen.getByText('Flow: my-flow'));
    expect(screen.queryByText('Save to collection...')).toBeNull();
    expect(screen.getByText('Save').closest('[role="menuitem"]')).toHaveAttribute('data-disabled');
  });

  it('disables Save on a dirty folder tab that is not the active one', async () => {
    const folder: FolderTab = {
      id: 'folder-tb-1',
      title: 'auth',
      isDirty: true,
      tabType: 'folder',
      collectionName: 'col',
      folderPath: 'auth',
      activeSection: 'headers',
    };
    usePaneStore.getState().openTab(folder);
    await usePaneStore.getState().openScriptTab('col', 'lib/b.js');
    render(<TabBar node={leaf()} />);
    fireEvent.contextMenu(screen.getByText('auth'));
    expect(screen.getByText('Save').closest('[role="menuitem"]')).toHaveAttribute('data-disabled');
  });
});
