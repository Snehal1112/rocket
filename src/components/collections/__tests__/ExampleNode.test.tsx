import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ExampleNode } from '@/components/collections/ExampleNode';
import { Tree } from '@/components/ui/tree';
import { collectAllTabs } from '@/lib/pane-utils';
import { deleteRequestExample, renameRequestExample } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { isExampleTab } from '@/types/pane-types';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    getCollection: vi.fn(),
    endAgentSession: vi.fn(),
    renameRequestExample: vi.fn(),
    deleteRequestExample: vi.fn(),
  };
});
vi.mock('sonner', () => ({ toast: { error: vi.fn() } }));

function renderNode(name = '401') {
  render(
    <Tree aria-label='tree'>
      <ExampleNode collectionName='col' requestPath='users.yml' name={name} />
    </Tree>,
  );
}

function exampleTabNames(): string[] {
  return collectAllTabs(usePaneStore.getState().root)
    .filter(isExampleTab)
    .map((t) => t.exampleName);
}

describe('ExampleNode', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.mocked(renameRequestExample).mockReset();
    vi.mocked(deleteRequestExample).mockReset();
    vi.mocked(toast.error).mockReset();
  });

  it('clicking_an_example_opens_its_tab', async () => {
    const openExampleTab = vi.spyOn(usePaneStore.getState(), 'openExampleTab');
    usePaneStore.setState({ openExampleTab });
    renderNode('401');
    await userEvent.click(screen.getByText('401'));
    expect(openExampleTab).toHaveBeenCalledWith('col', 'users.yml', '401');
    expect(exampleTabNames()).toEqual(['401']);
  });

  it('rename_calls_the_command_and_retitles', async () => {
    vi.mocked(renameRequestExample).mockResolvedValue(undefined);
    usePaneStore.getState().openExampleTab('col', 'users.yml', '401');
    renderNode('401');
    await userEvent.click(screen.getByRole('button', { name: 'Actions for 401' }));
    await userEvent.click(await screen.findByText('Rename'));
    const input = await screen.findByDisplayValue('401');
    await userEvent.clear(input);
    await userEvent.type(input, 'Unauthorized{Enter}');
    await waitFor(() =>
      expect(renameRequestExample).toHaveBeenCalledWith(
        'col',
        'users.yml',
        '401',
        'Unauthorized',
      ),
    );
    await waitFor(() => expect(exampleTabNames()).toEqual(['Unauthorized']));
  });

  it('a_rejected_rename_shows_a_toast_and_keeps_the_old_name', async () => {
    vi.mocked(renameRequestExample).mockRejectedValue(new Error('name already used'));
    usePaneStore.getState().openExampleTab('col', 'users.yml', '401');
    renderNode('401');
    await userEvent.click(screen.getByRole('button', { name: 'Actions for 401' }));
    await userEvent.click(await screen.findByText('Rename'));
    const input = await screen.findByDisplayValue('401');
    await userEvent.clear(input);
    await userEvent.type(input, '200 OK{Enter}');
    await waitFor(() => expect(toast.error).toHaveBeenCalled());
    expect(vi.mocked(toast.error).mock.calls[0][0]).toContain('name already used');
    expect(exampleTabNames()).toEqual(['401']);
    expect(await screen.findByText('401')).toBeInTheDocument();
  });

  it('delete_asks_for_confirmation_then_closes_tabs', async () => {
    vi.mocked(deleteRequestExample).mockResolvedValue(undefined);
    usePaneStore.getState().openExampleTab('col', 'users.yml', '401');
    renderNode('401');
    await userEvent.click(screen.getByRole('button', { name: 'Actions for 401' }));
    await userEvent.click(await screen.findByText('Delete'));
    // Nothing is deleted before the user confirms.
    expect(deleteRequestExample).not.toHaveBeenCalled();
    expect(exampleTabNames()).toEqual(['401']);
    await userEvent.click(await screen.findByRole('button', { name: 'Delete' }));
    await waitFor(() =>
      expect(deleteRequestExample).toHaveBeenCalledWith('col', 'users.yml', '401'),
    );
    await waitFor(() => expect(exampleTabNames()).toEqual([]));
  });

  it('cancelling_the_delete_keeps_the_example_and_its_tab', async () => {
    usePaneStore.getState().openExampleTab('col', 'users.yml', '401');
    renderNode('401');
    await userEvent.click(screen.getByRole('button', { name: 'Actions for 401' }));
    await userEvent.click(await screen.findByText('Delete'));
    await userEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
    expect(deleteRequestExample).not.toHaveBeenCalled();
    expect(exampleTabNames()).toEqual(['401']);
  });
});
