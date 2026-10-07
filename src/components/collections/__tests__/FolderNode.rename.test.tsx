import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
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

// Plain menus: Radix's focus trap would blur the rename input in jsdom.
vi.mock('@/components/ui/context-menu', () => ({
  ContextMenu: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  ContextMenuTrigger: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  ContextMenuContent: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  ContextMenuSeparator: () => null,
  ContextMenuItem: ({ children, onClick }: { children: ReactNode; onClick?: () => void }) => (
    <div role='menuitem' tabIndex={0} onClick={onClick} onKeyDown={() => undefined}>
      {children}
    </div>
  ),
}));

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

describe('FolderNode rename', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.mocked(moveItem).mockClear();
  });

  it('retargets the open folder tab when the folder is renamed', async () => {
    usePaneStore.getState().openFolderTab('col', 'parent');
    const id = folderTabs()[0].id;
    renderNode();

    fireEvent.click(screen.getByText('Rename'));
    const input = await screen.findByDisplayValue('parent');
    fireEvent.change(input, { target: { value: 'renamed' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    await waitFor(() => expect(folderTabs()[0].folderPath).toBe('renamed'));
    expect(moveItem).toHaveBeenCalledWith('col', 'parent', 'col', 'renamed');
    expect(folderTabs()[0].id).toBe(id);
    expect(folderTabs()[0].title).toBe('renamed');
  });
});
