import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { FolderSettingsTab } from '@/components/collections/FolderSettingsTab';
import type * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { FolderTab } from '@/types/pane-types';

const { mockGet, mockSave, mockMarkDirty, mockMarkClean } = vi.hoisted(() => ({
  mockGet: vi.fn(),
  mockSave: vi.fn(),
  mockMarkDirty: vi.fn(),
  mockMarkClean: vi.fn(),
}));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, getFolderSettings: mockGet, saveFolderSettings: mockSave };
});

// A stub section that edits docs, so the shell wiring is tested on its own.
vi.mock('../folder-settings/DocsSection', () => ({
  DocsSection: ({ onChange }: { onChange: (patch: { docs: string }) => void }) => (
    <button type='button' onClick={() => onChange({ docs: 'edited' })}>
      edit-docs
    </button>
  ),
}));

const base: tauriApi.FolderSettings = { headers: [], variables: [], docs: 'base' };

const tab: FolderTab = {
  id: 'folder-tab-1',
  title: 'b',
  isDirty: false,
  tabType: 'folder',
  collectionName: 'col',
  folderPath: 'a/b',
  activeSection: 'docs',
};

function pressSave(tabId: string) {
  window.dispatchEvent(new CustomEvent('rocket:save-draft', { detail: { tabId } }));
}

beforeEach(() => {
  mockGet.mockReset().mockResolvedValue(base);
  mockSave.mockReset().mockResolvedValue(undefined);
  mockMarkDirty.mockReset();
  mockMarkClean.mockReset();
  usePaneStore.setState({ markDirty: mockMarkDirty, markClean: mockMarkClean });
});

describe('FolderSettingsTab save wiring', () => {
  it('has a disabled Save button until something is edited', async () => {
    render(<FolderSettingsTab tab={tab} />);
    await screen.findByText('edit-docs');
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    expect(screen.queryByText('Unsaved changes')).not.toBeInTheDocument();
  });

  it('shows the dirty indicator and saves the whole object from the header button', async () => {
    render(<FolderSettingsTab tab={tab} />);
    await userEvent.click(await screen.findByText('edit-docs'));
    expect(screen.getByText('Unsaved changes')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() =>
      expect(mockSave).toHaveBeenCalledWith('col', 'a/b', { ...base, docs: 'edited' }),
    );
    await waitFor(() => expect(screen.queryByText('Unsaved changes')).not.toBeInTheDocument());
  });

  it('saves on Cmd/Ctrl+S for its own tab only', async () => {
    render(<FolderSettingsTab tab={tab} />);
    await userEvent.click(await screen.findByText('edit-docs'));
    act(() => pressSave('some-other-tab'));
    expect(mockSave).not.toHaveBeenCalled();
    act(() => pressSave(tab.id));
    await waitFor(() => expect(mockSave).toHaveBeenCalledTimes(1));
  });

  it('keeps the pane store dirty marker in sync with the hook', async () => {
    render(<FolderSettingsTab tab={tab} />);
    await userEvent.click(await screen.findByText('edit-docs'));
    await waitFor(() => expect(mockMarkDirty).toHaveBeenCalledWith(tab.id));
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(mockMarkClean).toHaveBeenLastCalledWith(tab.id));
  });

  it('shows an error instead of the sections when the load fails', async () => {
    const errSpy = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    mockGet.mockRejectedValue(new Error('missing'));
    render(<FolderSettingsTab tab={tab} />);
    expect(await screen.findByText('Failed to load folder settings.')).toBeInTheDocument();
    expect(screen.queryByText('edit-docs')).not.toBeInTheDocument();
    errSpy.mockRestore();
  });
});
