import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ScriptFilePane } from '@/components/scripts/ScriptFilePane';
import { findScriptTab } from '@/lib/pane-utils';
import { readScriptFile, saveScriptFile } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { ScriptTab } from '@/types/pane-types';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    getCollection: vi.fn(),
    readScriptFile: vi.fn(),
    saveScriptFile: vi.fn(),
    endAgentSession: vi.fn(),
  };
});
vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn() } }));
// A plain stub replaces Monaco, which needs a browser layout engine.
vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: ({ value, onChange }: { value: string; onChange?: (v: string) => void }) => (
    <textarea aria-label='editor' value={value} onChange={(e) => onChange?.(e.target.value)} />
  ),
}));

async function openTab(path = 'lib/utils.js'): Promise<ScriptTab> {
  vi.mocked(readScriptFile).mockResolvedValue('module.exports = 1;');
  await usePaneStore.getState().openScriptTab('col', path);
  const found = findScriptTab(usePaneStore.getState().root, 'col', path);
  if (!found) throw new Error('tab not opened');
  return found.tab;
}

describe('ScriptFilePane', () => {
  beforeEach(() => {
    usePaneStore.getState().closeAll();
    vi.mocked(saveScriptFile).mockReset();
  });

  it('shows the require hint relative to the collection root', async () => {
    const tab = await openTab();
    render(<ScriptFilePane tab={tab} />);
    expect(screen.getByText("require('./lib/utils.js')")).toBeInTheDocument();
  });

  it('marks the tab dirty on edit and saves through the command', async () => {
    vi.mocked(saveScriptFile).mockResolvedValue(undefined);
    const tab = await openTab();
    const { rerender } = render(<ScriptFilePane tab={tab} />);

    fireEvent.change(await screen.findByLabelText('editor'), { target: { value: 'edited' } });
    const edited = findScriptTab(usePaneStore.getState().root, 'col', 'lib/utils.js')?.tab;
    expect(edited?.isDirty).toBe(true);
    if (edited) rerender(<ScriptFilePane tab={edited} />);

    fireEvent.click(screen.getByRole('button', { name: /save/i }));
    await waitFor(() =>
      expect(saveScriptFile).toHaveBeenCalledWith('col', 'lib/utils.js', 'edited'),
    );
    await waitFor(() =>
      expect(findScriptTab(usePaneStore.getState().root, 'col', 'lib/utils.js')?.tab.isDirty).toBe(
        false,
      ),
    );
  });

  it('keeps the dirty marker when saving fails', async () => {
    vi.mocked(saveScriptFile).mockRejectedValue('disk full');
    const tab = await openTab();
    usePaneStore.getState().updateScriptContent(tab.id, 'edited');
    const edited = findScriptTab(usePaneStore.getState().root, 'col', 'lib/utils.js')?.tab;
    if (!edited) throw new Error('missing tab');
    render(<ScriptFilePane tab={edited} />);

    fireEvent.click(screen.getByRole('button', { name: /save/i }));
    await waitFor(() => expect(saveScriptFile).toHaveBeenCalled());
    await waitFor(() => expect(toast.error).toHaveBeenCalled());
    expect(findScriptTab(usePaneStore.getState().root, 'col', 'lib/utils.js')?.tab.isDirty).toBe(
      true,
    );
  });

  it('disables Save when there are no changes', async () => {
    const tab = await openTab();
    render(<ScriptFilePane tab={tab} />);
    expect(screen.getByRole('button', { name: /save/i })).toBeDisabled();
  });

  async function dirtyTab(): Promise<ScriptTab> {
    const tab = await openTab();
    usePaneStore.getState().updateScriptContent(tab.id, 'edited');
    const edited = findScriptTab(usePaneStore.getState().root, 'col', 'lib/utils.js')?.tab;
    if (!edited) throw new Error('missing tab');
    return edited;
  }

  it('saves when the rocket:save-draft event targets its tab', async () => {
    vi.mocked(saveScriptFile).mockResolvedValue(undefined);
    const edited = await dirtyTab();
    render(<ScriptFilePane tab={edited} />);

    act(() => {
      window.dispatchEvent(new CustomEvent('rocket:save-draft', { detail: { tabId: 'other' } }));
    });
    expect(saveScriptFile).not.toHaveBeenCalled();
    act(() => {
      window.dispatchEvent(new CustomEvent('rocket:save-draft', { detail: { tabId: edited.id } }));
    });
    await waitFor(() => expect(saveScriptFile).toHaveBeenCalledTimes(1));
  });

  it('ignores a second save while one is in flight', async () => {
    let resolveSave: () => void = () => undefined;
    vi.mocked(saveScriptFile).mockReturnValue(new Promise<void>((r) => (resolveSave = r)));
    const edited = await dirtyTab();
    render(<ScriptFilePane tab={edited} />);

    act(() => {
      for (let i = 0; i < 2; i++) {
        window.dispatchEvent(
          new CustomEvent('rocket:save-draft', { detail: { tabId: edited.id } }),
        );
      }
    });
    expect(saveScriptFile).toHaveBeenCalledTimes(1);
    await act(async () => resolveSave());
  });

  it('does not let Ctrl+S inside the pane reach the global handler', async () => {
    vi.mocked(saveScriptFile).mockResolvedValue(undefined);
    const edited = await dirtyTab();
    const windowKeydown = vi.fn();
    window.addEventListener('keydown', windowKeydown);
    render(<ScriptFilePane tab={edited} />);

    fireEvent.keyDown(screen.getByLabelText('editor'), { key: 's', ctrlKey: true });
    window.removeEventListener('keydown', windowKeydown);
    await waitFor(() => expect(saveScriptFile).toHaveBeenCalledTimes(1));
    expect(windowKeydown).not.toHaveBeenCalled();
  });
});
