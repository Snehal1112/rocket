import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { usePaneStore } from '@/stores/pane-store';
import type { RunnerTab } from '@/types/pane-types';
import { RunnerPane } from './RunnerPane';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, listCollections: vi.fn(), getCollection: vi.fn() };
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
      { uid: 'c1', name: 'demo', path: '/tmp/demo', requestCount: 1 },
    ]);

    render(<RunnerPane tab={pickerTab()} groupId='g1' />);
    await waitFor(() => expect(listCollections).toHaveBeenCalled());
    expect(screen.getByText(/choose a collection/i)).toBeInTheDocument();
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
