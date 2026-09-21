import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { usePaneStore } from '@/stores/pane-store';
import type { RunnerTab } from '@/types/pane-types';
import { RunnerPane } from './RunnerPane';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, listCollections: vi.fn(), getCollection: vi.fn() };
});

beforeAll(() => {
  HTMLElement.prototype.hasPointerCapture = vi.fn(() => false);
  HTMLElement.prototype.setPointerCapture = vi.fn();
  HTMLElement.prototype.releasePointerCapture = vi.fn();
  HTMLElement.prototype.scrollIntoView = vi.fn();
});

function pickerTab(): RunnerTab {
  return {
    id: 'blank-1',
    title: 'Runner',
    isDirty: false,
    tabType: 'runner',
    collectionName: null,
    runState: 'idle',
    requests: [],
  };
}

function scopedTab(runState: RunnerTab['runState'] = 'idle'): RunnerTab {
  return {
    id: 'scoped-1',
    title: 'Run: demo',
    isDirty: false,
    tabType: 'runner',
    collectionName: 'demo',
    runState,
    requests: [],
  };
}

describe('RunnerPane', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
  });

  it('shows a collection picker when collectionName is null', async () => {
    const { listCollections } = await import('@/lib/tauri-api');
    vi.mocked(listCollections).mockResolvedValue([
      {
        uid: 'c1',
        repositoryId: 'collection:default:c1',
        name: 'demo',
        path: '/tmp/demo',
        requestCount: 1,
      },
    ]);

    render(<RunnerPane tab={pickerTab()} groupId='g1' />);
    await waitFor(() => expect(listCollections).toHaveBeenCalled());
    expect(screen.getByText(/choose a collection/i)).toBeInTheDocument();
  });

  it('loads nested folders and opens a runner scoped to the selected folder', async () => {
    const user = userEvent.setup();
    const { getCollection, listCollections } = await import('@/lib/tauri-api');
    vi.mocked(listCollections).mockResolvedValue([
      {
        uid: 'c1',
        repositoryId: 'collection:default:c1',
        name: 'demo',
        path: '/tmp/demo',
        requestCount: 1,
      },
    ]);
    vi.mocked(getCollection).mockResolvedValue({
      name: 'demo',
      settings: { headers: [], variables: [], sandboxMode: 'safe' },
      root: {
        uid: 'root',
        name: 'demo',
        items: [
          {
            type: 'folder',
            uid: 'f1',
            name: 'Auth',
            dirName: 'auth',
            items: [
              {
                type: 'folder',
                uid: 'f2',
                name: 'Nested',
                dirName: 'nested',
                items: [],
              },
            ],
          },
        ],
      },
    });
    const openRunnerTab = vi.fn().mockResolvedValue(undefined);
    const closeTab = vi.fn();
    usePaneStore.setState({ openRunnerTab, closeTab });

    render(<RunnerPane tab={pickerTab()} groupId='g1' />);
    await waitFor(() => expect(listCollections).toHaveBeenCalled());

    await user.click(screen.getByRole('combobox', { name: 'Collection' }));
    await user.click(screen.getByRole('option', { name: 'demo' }));
    await waitFor(() => expect(getCollection).toHaveBeenCalledWith('demo'));

    await user.click(screen.getByRole('combobox', { name: 'Folder' }));
    await user.click(screen.getByRole('option', { name: 'Auth / Nested' }));
    await user.click(screen.getByRole('button', { name: 'Load' }));

    expect(openRunnerTab).toHaveBeenCalledWith('demo', 'auth/nested');
    expect(closeTab).toHaveBeenCalledWith('blank-1', 'g1');
  });

  it('shows RunnerSummaryHeader and RunnerRequestList when idle with a collection set', () => {
    const tab = scopedTab('idle');
    usePaneStore.setState((s) => ({
      root: { ...s.root, type: 'leaf', tabs: [tab], activeTabId: tab.id } as typeof s.root,
    }));
    render(<RunnerPane tab={tab} groupId='g1' />);
    expect(screen.getByText('No requests found in this collection/folder.')).toBeInTheDocument(); // RunnerRequestList empty state
  });

  it('shows RunnerResultsList once the run is done', () => {
    const tab = scopedTab('done');
    usePaneStore.setState((s) => ({
      root: { ...s.root, type: 'leaf', tabs: [tab], activeTabId: tab.id } as typeof s.root,
    }));
    render(<RunnerPane tab={tab} groupId='g1' />);
    expect(screen.getByText('No requests found in this collection/folder.')).toBeInTheDocument(); // RunnerResultsList empty state
  });
});
