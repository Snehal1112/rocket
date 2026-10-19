import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { FolderNode } from '@/components/collections/FolderNode';
import { Tree } from '@/components/ui/tree';
import { collectAllTabs } from '@/lib/pane-utils';
import type { CollectionItem, CollectionSummary } from '@/lib/tauri-api';
import { createRequest, saveRequest, moveItem } from '@/lib/tauri-api';
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
    saveRequest: vi.fn(),
    createRequest: vi.fn(),
  };
});
vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn() } }));

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

describe('FolderNode duplicate forwarding', () => {
  it('FolderNode forwards onDuplicate to its child requests', async () => {
    usePaneStore.getState().reset();
    const onDuplicate = vi.fn().mockResolvedValue(undefined);
    const request: CollectionItem = {
      type: 'summary',
      uid: 'r-1',
      name: 'Child Req',
      fileName: 'child.yml',
      method: 'GET',
      url: 'https://api.example.com/x',
    };
    render(
      <Tree aria-label='tree'>
        <FolderNode
          name='folder'
          items={[request]}
          collectionName='col'
          collectionRoot='/ws/col'
          basePath='folder'
          depth={0}
          filter=''
          summaries={[]}
          onNewFolder={vi.fn()}
          onMove={vi.fn()}
          onDelete={vi.fn()}
          onDuplicate={onDuplicate}
        />
      </Tree>,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Expand folder' }));
    const user = userEvent.setup();
    await user.click(screen.getByLabelText('Actions for Child Req'));
    await user.click(await screen.findByText('Duplicate'));
    expect(onDuplicate).toHaveBeenCalledWith('col', 'folder/child.yml');
  });
});

describe('FolderNode new request name collision', () => {
  const existing: CollectionItem = {
    type: 'summary',
    uid: 'r-existing',
    name: 'Login',
    fileName: 'login.yml',
    method: 'POST',
    url: 'https://api.example.com/login',
  };

  afterEach(() => {
    vi.restoreAllMocks();
  });

  async function createRequestNamed(name: string) {
    usePaneStore.getState().reset();
    vi.mocked(saveRequest).mockReset();
    vi.mocked(createRequest).mockReset();
    vi.mocked(toast.error).mockReset();
    vi.mocked(createRequest).mockRejectedValue(new Error('Already exists: col/auth/login.yml'));
    render(
      <Tree aria-label='tree'>
        <FolderNode
          name='auth'
          items={[existing]}
          collectionName='col'
          collectionRoot='/ws/col'
          basePath='auth'
          depth={0}
          filter=''
          summaries={[]}
          onNewFolder={vi.fn()}
          onMove={vi.fn()}
          onDelete={vi.fn()}
          onDuplicate={vi.fn()}
        />
      </Tree>,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Expand auth' }));
    // Focus is stubbed: when the menu closes it moves focus, which blurs the new
    // request input and hides it before the test can type.
    vi.spyOn(HTMLElement.prototype, 'focus').mockImplementation(() => undefined);
    await userEvent.click(screen.getByRole('button', { name: 'Actions for auth' }));
    await userEvent.click(await screen.findByText('New Request'));
    const input = await screen.findByPlaceholderText('Request name');
    fireEvent.change(input, { target: { value: name } });
    fireEvent.keyDown(input, { key: 'Enter' });
  }

  it('does not overwrite an existing request with the same file name', async () => {
    await createRequestNamed('login');
    expect(saveRequest).not.toHaveBeenCalled();
    expect(createRequest).toHaveBeenCalledWith('col', expect.stringMatching(/^auth\/login$/i), expect.anything());
    expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('already exists'));
  });

  it('treats a case variant of an existing file name as a collision', async () => {
    await createRequestNamed('LOGIN');
    expect(saveRequest).not.toHaveBeenCalled();
    expect(createRequest).toHaveBeenCalledWith('col', expect.stringMatching(/^auth\/login$/i), expect.anything());
    expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('already exists'));
  });
});
