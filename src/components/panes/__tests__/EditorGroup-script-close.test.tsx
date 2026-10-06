import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { findScriptTab } from '@/lib/pane-utils';
import { saveScriptFile } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { EditorGroup } from '../EditorGroup';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    getCollection: vi.fn(),
    readScriptFile: vi.fn().mockResolvedValue('one'),
    saveScriptFile: vi.fn(),
    endAgentSession: vi.fn(),
  };
});
vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn() } }));
vi.mock('@/components/scripts/ScriptFilePane', () => ({ ScriptFilePane: () => <div /> }));

async function setup() {
  usePaneStore.getState().reset();
  vi.mocked(saveScriptFile).mockReset();
  await usePaneStore.getState().openScriptTab('col', 'a.js');
  const tab = findScriptTab(usePaneStore.getState().root, 'col', 'a.js')?.tab;
  if (!tab) throw new Error('missing tab');
  usePaneStore.getState().updateScriptContent(tab.id, 'two');
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected leaf root');
  render(
    <QueryClientProvider client={new QueryClient()}>
      <EditorGroup node={root} />
    </QueryClientProvider>,
  );
  fireEvent.click(screen.getByLabelText(/close/i));
}

describe('EditorGroup unsaved script close dialog', () => {
  beforeEach(() => vi.clearAllMocks());

  it('uses script wording and saves before closing', async () => {
    await setup();
    expect(screen.getByText(/This script has unsaved changes/)).toBeInTheDocument();
    vi.mocked(saveScriptFile).mockResolvedValue(undefined);
    fireEvent.click(screen.getByRole('button', { name: 'Save and close' }));
    await waitFor(() => expect(saveScriptFile).toHaveBeenCalledWith('col', 'a.js', 'two'));
    await waitFor(() =>
      expect(findScriptTab(usePaneStore.getState().root, 'col', 'a.js')).toBeNull(),
    );
  });

  it('keeps the tab open when saving fails', async () => {
    await setup();
    vi.mocked(saveScriptFile).mockRejectedValue('disk full');
    fireEvent.click(screen.getByRole('button', { name: 'Save and close' }));
    await waitFor(() => expect(saveScriptFile).toHaveBeenCalled());
    expect(findScriptTab(usePaneStore.getState().root, 'col', 'a.js')).not.toBeNull();
  });
});

describe('EditorGroup rocket:request-close-tab event', () => {
  const realCloseTab = usePaneStore.getState().closeTab;
  beforeEach(() => vi.clearAllMocks());
  afterEach(() => usePaneStore.setState({ closeTab: realCloseTab }));

  async function setupDirty() {
    usePaneStore.getState().reset();
    vi.mocked(saveScriptFile).mockReset();
    await usePaneStore.getState().openScriptTab('col', 'a.js');
    const tab = findScriptTab(usePaneStore.getState().root, 'col', 'a.js')?.tab;
    if (!tab) throw new Error('missing tab');
    usePaneStore.getState().updateScriptContent(tab.id, 'two');
    const { root } = usePaneStore.getState();
    if (root.type !== 'leaf') throw new Error('Expected leaf root');
    return { tab, root };
  }

  function request(tabId: string) {
    act(() => {
      window.dispatchEvent(new CustomEvent('rocket:request-close-tab', { detail: { tabId } }));
    });
  }

  function renderGroup(root: Parameters<typeof EditorGroup>[0]['node']) {
    return render(
      <QueryClientProvider client={new QueryClient()}>
        <EditorGroup node={root} />
      </QueryClientProvider>,
    );
  }

  it('opens the script dialog in the owning group', async () => {
    const { tab, root } = await setupDirty();
    renderGroup(root);
    request(tab.id);
    expect(screen.getByText(/This script has unsaved changes/)).toBeInTheDocument();
  });

  it('ignores the event in a group that does not own the tab', async () => {
    const { tab, root } = await setupDirty();
    const closeTab = vi.fn();
    usePaneStore.setState({ closeTab } as never);
    const otherGroup = { ...root, id: 'leaf-other', groupId: 'group-other', tabs: [] };
    renderGroup(otherGroup);
    request(tab.id);
    expect(closeTab).not.toHaveBeenCalled();
    expect(saveScriptFile).not.toHaveBeenCalled();
    expect(screen.queryByText(/This script has unsaved changes/)).not.toBeInTheDocument();
  });

  it('removes its listener on unmount', async () => {
    const { root } = await setupDirty();
    const add = vi.spyOn(window, 'addEventListener');
    const remove = vi.spyOn(window, 'removeEventListener');
    const view = renderGroup(root);
    const added = add.mock.calls.find(([type]) => type === 'rocket:request-close-tab');
    expect(added).toBeDefined();
    view.unmount();
    const removed = remove.mock.calls.find(([type]) => type === 'rocket:request-close-tab');
    expect(removed?.[1]).toBe(added?.[1]);
    add.mockRestore();
    remove.mockRestore();
  });

  it('shows exactly one dialog when two groups are mounted', async () => {
    const { tab, root } = await setupDirty();
    const groupA = { ...root, id: 'leaf-a', groupId: 'group-a', tabs: [] };
    render(
      <QueryClientProvider client={new QueryClient()}>
        <EditorGroup node={groupA} />
        <EditorGroup node={root} />
      </QueryClientProvider>,
    );
    request(tab.id);
    expect(screen.getAllByText(/This script has unsaved changes/)).toHaveLength(1);
    // The dialog is portalled, so the close below proves group B owns it.
    expect(screen.getAllByRole('alertdialog')).toHaveLength(1);
    fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    expect(findScriptTab(usePaneStore.getState().root, 'col', 'a.js')).toBeNull();
  });

  it('keeps the tab on Cancel', async () => {
    const { tab, root } = await setupDirty();
    renderGroup(root);
    request(tab.id);
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(findScriptTab(usePaneStore.getState().root, 'col', 'a.js')).not.toBeNull();
  });

  it('closes the tab on Close', async () => {
    const { tab, root } = await setupDirty();
    renderGroup(root);
    request(tab.id);
    fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    expect(findScriptTab(usePaneStore.getState().root, 'col', 'a.js')).toBeNull();
    expect(saveScriptFile).not.toHaveBeenCalled();
  });

  it('saves then closes on Save and close', async () => {
    const { tab, root } = await setupDirty();
    vi.mocked(saveScriptFile).mockResolvedValue(undefined);
    renderGroup(root);
    request(tab.id);
    fireEvent.click(screen.getByRole('button', { name: 'Save and close' }));
    await waitFor(() => expect(saveScriptFile).toHaveBeenCalledWith('col', 'a.js', 'two'));
    await waitFor(() =>
      expect(findScriptTab(usePaneStore.getState().root, 'col', 'a.js')).toBeNull(),
    );
  });
});
