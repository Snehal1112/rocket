import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { FolderSettingsTab } from '@/components/collections/FolderSettingsTab';
import { collectAllTabs } from '@/lib/pane-utils';
import { usePaneStore } from '@/stores/pane-store';
import type { FolderTab } from '@/types/pane-types';
import { isFolderTab } from '@/types/pane-types';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn(), endAgentSession: vi.fn() };
});

function storedTab(): FolderTab {
  const tab = collectAllTabs(usePaneStore.getState().root).find(isFolderTab);
  if (!tab) throw new Error('No folder tab in the store');
  return tab;
}

// The tab reads its section from the store, so the test re-renders with the stored tab.
function renderStoredTab() {
  const view = render(<FolderSettingsTab tab={storedTab()} />);
  const unsubscribe = usePaneStore.subscribe(() => {
    view.rerender(<FolderSettingsTab tab={storedTab()} />);
  });
  return { ...view, unsubscribe };
}

describe('FolderSettingsTab', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    usePaneStore.getState().openFolderTab('my-col', 'auth/oauth');
  });

  it('shows the folder name and the collection breadcrumb', () => {
    const { unsubscribe } = renderStoredTab();
    expect(screen.getByRole('heading', { name: 'oauth' })).toBeInTheDocument();
    expect(screen.getByText('my-col / auth / oauth')).toBeInTheDocument();
    unsubscribe();
  });

  it('shows the six sections in order', () => {
    const { unsubscribe } = renderStoredTab();
    const labels = screen.getAllByRole('tab').map((t) => t.textContent);
    expect(labels).toEqual(['Headers', 'Script', 'Test', 'Vars', 'Auth', 'Docs']);
    expect(screen.getByRole('tab', { name: 'Headers' })).toHaveAttribute('aria-selected', 'true');
    unsubscribe();
  });

  it('shows the placeholder of the active section', () => {
    usePaneStore.getState().openFolderTab('my-col', 'auth/oauth', 'docs');
    const { unsubscribe } = renderStoredTab();
    expect(screen.getByRole('tab', { name: 'Docs' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByText('Docs for this folder will be editable here.')).toBeInTheDocument();
    unsubscribe();
  });

  it('switching a section updates the tab in the store', async () => {
    const { unsubscribe } = renderStoredTab();
    await userEvent.click(screen.getByRole('tab', { name: 'Auth' }));
    expect(storedTab().activeSection).toBe('auth');
    expect(screen.getByRole('tab', { name: 'Auth' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByText('Auth for this folder will be editable here.')).toBeInTheDocument();
    unsubscribe();
  });
});
