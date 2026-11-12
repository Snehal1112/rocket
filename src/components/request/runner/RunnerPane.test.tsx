import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { usePaneStore } from '@/stores/pane-store';
import type { RunnerTab } from '@/types/pane-types';
import { RunnerPane } from './RunnerPane';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, listCollections: vi.fn(), getCollection: vi.fn(), loadIterationData: vi.fn() };
});

const mockOpen = vi.fn();
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: (...args: unknown[]) => mockOpen(...args),
}));

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
  beforeEach(async () => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
    const { loadIterationData } = await import('@/lib/tauri-api');
    vi.mocked(loadIterationData).mockResolvedValue(null);
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
    expect(screen.getByText('No requests to run')).toBeInTheDocument(); // RunnerRequestList empty state
    expect(screen.getByText(/The collection "demo" has no requests/)).toBeInTheDocument();
  });

  it('shows RunnerResultsList once the run is done', () => {
    const tab = scopedTab('done');
    usePaneStore.setState((s) => ({
      root: { ...s.root, type: 'leaf', tabs: [tab], activeTabId: tab.id } as typeof s.root,
    }));
    render(<RunnerPane tab={tab} groupId='g1' />);
    expect(screen.getByText('Nothing was run')).toBeInTheDocument(); // RunnerResultsList empty state
  });

  describe('data file row', () => {
    function seed(tab: RunnerTab) {
      usePaneStore.setState((s) => ({
        root: { ...s.root, type: 'leaf', tabs: [tab], activeTabId: tab.id } as typeof s.root,
      }));
    }

    it('shows a Data file row with a picker button', async () => {
      const tab = scopedTab('idle');
      seed(tab);
      render(<RunnerPane tab={tab} groupId='g1' />);
      expect(screen.getByText('Data file')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Choose data file' })).toBeInTheDocument();
      expect(await screen.findByText(/goes once/i)).toBeInTheDocument();
    });

    it('shows the saved default as the file in use', async () => {
      const { loadIterationData } = await import('@/lib/tauri-api');
      vi.mocked(loadIterationData).mockResolvedValue({
        columns: ['id'],
        rows: [{ id: '1' }],
        source: 'savedDefault',
        fileName: 'users.csv',
      });
      const tab = scopedTab('idle');
      seed(tab);
      render(<RunnerPane tab={tab} groupId='g1' />);
      expect(await screen.findByText(/using saved default/i)).toBeInTheDocument();
      expect(screen.getByText(/users\.csv/)).toBeInTheDocument();
    });

    it('shows the row count and column names after a successful load', async () => {
      const { loadIterationData } = await import('@/lib/tauri-api');
      vi.mocked(loadIterationData).mockResolvedValue({
        columns: ['id', 'email'],
        rows: [{ id: '1', email: 'a' }, { id: '2', email: 'b' }, { id: '3', email: 'c' }],
        source: 'perRun',
        fileName: 'rows.csv',
      });
      const tab = { ...scopedTab('idle'), dataFilePath: '/tmp/rows.csv' };
      seed(tab);
      render(<RunnerPane tab={tab} groupId='g1' />);
      expect(await screen.findByText(/3 rows/)).toBeInTheDocument();
      expect(screen.getByText(/id, email/)).toBeInTheDocument();
      expect(loadIterationData).toHaveBeenCalledWith('demo', '/tmp/rows.csv');
    });

    it('shows a warning when the saved default is invalid', async () => {
      const { loadIterationData } = await import('@/lib/tauri-api');
      vi.mocked(loadIterationData).mockRejectedValue('Row 3 has more cells than the header');
      const tab = scopedTab('idle');
      seed(tab);
      render(<RunnerPane tab={tab} groupId='g1' />);
      expect(await screen.findByText(/Row 3 has more cells than the header/)).toBeInTheDocument();
      expect(screen.getByRole('alert')).toBeInTheDocument();
    });

    it('shows an error when the file dialog fails', async () => {
      const user = userEvent.setup();
      mockOpen.mockRejectedValue(new Error('no dialog'));
      const tab = scopedTab('idle');
      seed(tab);
      render(<RunnerPane tab={tab} groupId='g1' />);
      await user.click(screen.getByRole('button', { name: 'Choose data file' }));
      expect(await screen.findByText(/Could not open the file dialog: no dialog/)).toBeInTheDocument();
    });

    it('stores the picked file on the tab', async () => {
      const user = userEvent.setup();
      mockOpen.mockResolvedValue('/tmp/picked.json');
      const tab = scopedTab('idle');
      seed(tab);
      render(<RunnerPane tab={tab} groupId='g1' />);
      await user.click(screen.getByRole('button', { name: 'Choose data file' }));
      await waitFor(() => {
        const stored = usePaneStore.getState().root;
        const tabs = stored.type === 'leaf' ? stored.tabs : [];
        expect(tabs[0] && 'dataFilePath' in tabs[0] ? tabs[0].dataFilePath : '').toBe(
          '/tmp/picked.json',
        );
      });
      expect(mockOpen).toHaveBeenCalledWith(
        expect.objectContaining({
          multiple: false,
          filters: [{ name: 'Data files', extensions: ['csv', 'json'] }],
        }),
      );
    });
  });
});
