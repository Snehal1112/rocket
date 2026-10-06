import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ScriptNode } from '@/components/collections/ScriptNode';
import { Tree } from '@/components/ui/tree';
import { findScriptTab } from '@/lib/pane-utils';
import { readScriptFile, renameScriptFile } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    getCollection: vi.fn(),
    readScriptFile: vi.fn(),
    renameScriptFile: vi.fn(),
    endAgentSession: vi.fn(),
  };
});
vi.mock('sonner', () => ({ toast: { error: vi.fn() } }));

function renderNode(onDelete = vi.fn()) {
  render(
    <Tree aria-label='tree'>
      <ScriptNode name='utils.js' collectionName='col' path='lib/utils.js' onDelete={onDelete} />
    </Tree>,
  );
  return onDelete;
}

describe('ScriptNode', () => {
  beforeEach(() => {
    usePaneStore.getState().closeAll();
    vi.mocked(readScriptFile).mockResolvedValue('module.exports = 1;');
    vi.mocked(renameScriptFile).mockReset();
  });

  it('opens a script tab on click', async () => {
    renderNode();
    fireEvent.click(screen.getByText('utils.js'));
    await waitFor(() =>
      expect(findScriptTab(usePaneStore.getState().root, 'col', 'lib/utils.js')).not.toBeNull(),
    );
  });

  it('asks the sidebar to delete with a script target', async () => {
    const onDelete = renderNode();
    await userEvent.click(screen.getByRole('button', { name: 'Actions for utils.js' }));
    await userEvent.click(await screen.findByText('Delete'));
    expect(onDelete).toHaveBeenCalledWith({
      type: 'script',
      collection: 'col',
      path: 'lib/utils.js',
      name: 'utils.js',
    });
  });

  it('renames the file and retargets the open tab', async () => {
    vi.mocked(renameScriptFile).mockResolvedValue('lib/helpers.js');
    await usePaneStore.getState().openScriptTab('col', 'lib/utils.js');
    renderNode();

    await userEvent.click(screen.getByRole('button', { name: 'Actions for utils.js' }));
    await userEvent.click(await screen.findByText('Rename'));
    const input = await screen.findByDisplayValue('utils.js');
    fireEvent.change(input, { target: { value: 'helpers' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    await waitFor(() =>
      expect(renameScriptFile).toHaveBeenCalledWith('col', 'lib/utils.js', 'helpers'),
    );
    await waitFor(() =>
      expect(findScriptTab(usePaneStore.getState().root, 'col', 'lib/helpers.js')).not.toBeNull(),
    );
  });
});
