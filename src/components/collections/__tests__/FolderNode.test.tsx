import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { FolderNode } from '@/components/collections/FolderNode';
import { Tree } from '@/components/ui/tree';
import { collectAllTabs } from '@/lib/pane-utils';
import type { CollectionItem, CollectionSummary } from '@/lib/tauri-api';
import { moveItem } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { FolderTab } from '@/types/pane-types';
import { isFolderTab } from '@/types/pane-types';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    getCollection: vi.fn(),
    endAgentSession: vi.fn(),
    moveItem: vi.fn().mockResolvedValue(undefined),
  };
});

const childFolder: CollectionItem = { type: 'folder', uid: 'f-child', name: 'child', items: [] };

function folderTabs(): FolderTab[] {
  return collectAllTabs(usePaneStore.getState().root).filter(isFolderTab);
}

function renderNode() {
  const summaries: CollectionSummary[] = [];
  render(
    <Tree aria-label='tree'>
      <FolderNode
        name='parent'
        items={[childFolder]}
        collectionName='col'
        collectionRoot='/ws/col'
        basePath='parent'
        depth={0}
        filter=''
        summaries={summaries}
        onNewFolder={vi.fn()}
        onMove={vi.fn()}
        onDelete={vi.fn()}
        onDuplicate={vi.fn()}
      />
    </Tree>,
  );
}

describe('FolderNode', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.mocked(moveItem).mockClear();
  });

  it('row click opens the tab and expands the folder', () => {
    renderNode();
    expect(screen.queryByText('child')).not.toBeInTheDocument();

    fireEvent.click(screen.getByText('parent'));

    expect(folderTabs()).toHaveLength(1);
    expect(folderTabs()[0]).toMatchObject({
      collectionName: 'col',
      folderPath: 'parent',
      activeSection: 'headers',
    });
    expect(screen.getByText('child')).toBeInTheDocument();
  });

  it('row click never collapses an expanded folder', () => {
    renderNode();
    fireEvent.click(screen.getByText('parent'));
    fireEvent.click(screen.getByText('parent'));

    expect(screen.getByText('child')).toBeInTheDocument();
    expect(folderTabs()).toHaveLength(1);
  });

  it('chevron toggles without opening a tab', () => {
    renderNode();
    fireEvent.click(screen.getByRole('button', { name: 'Expand parent' }));
    expect(screen.getByText('child')).toBeInTheDocument();
    expect(folderTabs()).toHaveLength(0);

    fireEvent.click(screen.getByRole('button', { name: 'Collapse parent' }));
    expect(screen.queryByText('child')).not.toBeInTheDocument();
    expect(folderTabs()).toHaveLength(0);
  });

  it('the Settings menu item opens the tab', async () => {
    renderNode();
    await userEvent.click(screen.getByRole('button', { name: 'Actions for parent' }));
    await userEvent.click(await screen.findByText('Settings'));
    expect(folderTabs().map((t) => t.activeSection)).toEqual(['headers']);
  });

  it('the Variables menu item opens the Vars section', async () => {
    renderNode();
    await userEvent.click(screen.getByRole('button', { name: 'Actions for parent' }));
    await userEvent.click(await screen.findByText('Variables'));
    expect(folderTabs().map((t) => t.activeSection)).toEqual(['vars']);
  });

  it('the context menu has Settings', async () => {
    renderNode();
    fireEvent.contextMenu(screen.getByText('parent'));
    await userEvent.click(await screen.findByText('Settings'));
    expect(folderTabs()).toHaveLength(1);
  });
});
