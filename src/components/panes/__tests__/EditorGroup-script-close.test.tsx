import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
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
  beforeEach(() => vi.clearAllMocks());

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
    const { root } = await setupDirty();
    renderGroup(root);
    request('someone-elses-tab');
    expect(screen.queryByText(/This script has unsaved changes/)).not.toBeInTheDocument();
  });

  it('stops listening after unmount', async () => {
    const { tab, root } = await setupDirty();
    const view = renderGroup(root);
    view.unmount();
    request(tab.id);
    expect(screen.queryByText(/This script has unsaved changes/)).not.toBeInTheDocument();
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
