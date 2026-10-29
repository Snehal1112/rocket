import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { findTabInTree } from '@/lib/pane-utils';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestTab } from '@/types/pane-types';
import { HistoryPanel } from '../HistoryPanel';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, listHistory: vi.fn(), searchHistory: vi.fn() };
});

describe('HistoryPanel mount', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.listHistory).mockResolvedValue([]);
  });

  it('calls listHistory exactly once on mount, not twice', async () => {
    render(<HistoryPanel />);

    await waitFor(() => {
      expect(tauriApi.listHistory).toHaveBeenCalledTimes(1);
    });
    expect(tauriApi.listHistory).toHaveBeenCalledWith(200);
  });
});

function entry(
  id: string,
  runSource?: tauriApi.RunSource,
  collection?: string,
): tauriApi.HistoryEntry {
  return {
    id,
    method: 'GET',
    url: `https://api.example.com/${id}`,
    status: 200,
    durationMs: 5,
    responseSize: 0,
    timestamp: '2026-10-11T10:00:00Z',
    runSource,
    collection,
  };
}

describe('HistoryPanel run source', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Radix Select needs these jsdom gaps filled.
    Element.prototype.hasPointerCapture = () => false;
    Element.prototype.scrollIntoView = () => {};
    vi.mocked(tauriApi.listHistory).mockResolvedValue([]);
    vi.mocked(tauriApi.searchHistory).mockResolvedValue([]);
  });

  it('shows a badge for agent, runner, flow and load test but not manual', async () => {
    vi.mocked(tauriApi.listHistory).mockResolvedValue([
      entry('a', 'agent'),
      entry('r', 'runner'),
      entry('f', 'flow'),
      entry('l', 'load_test'),
      entry('m', 'manual'),
      entry('legacy'),
    ]);
    render(<HistoryPanel />);

    expect(await screen.findByText('Agent')).toBeTruthy();
    expect(screen.getByText('Runner')).toBeTruthy();
    expect(screen.getByText('Flow')).toBeTruthy();
    expect(screen.getByText('Load test')).toBeTruthy();
    expect(document.querySelectorAll('[data-slot="badge"]')).toHaveLength(4);
  });

  it('searches with runSource when a source is picked', async () => {
    render(<HistoryPanel />);
    await waitFor(() => expect(tauriApi.listHistory).toHaveBeenCalledTimes(1));

    await userEvent.click(screen.getByLabelText('Filter by run source'));
    await userEvent.click(await screen.findByRole('option', { name: 'Agent' }));

    await waitFor(() =>
      expect(tauriApi.searchHistory).toHaveBeenCalledWith(
        expect.objectContaining({ runSource: 'agent' }),
      ),
    );
  });

  it('keeps the unfiltered listHistory path when nothing is set', async () => {
    render(<HistoryPanel />);
    await waitFor(() => expect(tauriApi.listHistory).toHaveBeenCalledWith(200));
    expect(tauriApi.searchHistory).not.toHaveBeenCalled();
  });
});

describe('HistoryPanel open entry', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    vi.mocked(tauriApi.searchHistory).mockResolvedValue([]);
  });

  async function openAndGetTab(id: string, collection?: string) {
    vi.mocked(tauriApi.listHistory).mockResolvedValue([entry(id, undefined, collection)]);
    render(<HistoryPanel />);
    await userEvent.click(
      await screen.findByRole('button', { name: `Open GET https://api.example.com/${id}` }),
    );
    return findTabInTree(usePaneStore.getState().root, `history-${id}`)?.tab as RequestTab;
  }

  it('sets historyCollection and no source when the entry has a collection', async () => {
    const tab = await openAndGetTab('h1', 'clone');
    expect(tab.historyCollection).toBe('clone');
    expect(tab.source).toBeUndefined();
  });

  it('sets historyCollection to null when the entry has none', async () => {
    const tab = await openAndGetTab('h2');
    expect(tab.historyCollection).toBeNull();
  });
});

describe('HistoryPanel empty states', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriApi.listHistory).mockResolvedValue([]);
    vi.mocked(tauriApi.searchHistory).mockResolvedValue([]);
  });

  it('says there is no history yet when nothing was sent and no filter is set', async () => {
    render(<HistoryPanel />);
    const state = await screen.findByText('No history yet');
    expect(state.closest('[data-slot="empty-state"]')).toHaveAttribute('data-cause', 'none-yet');
  });

  it('blames the filter, not the history, when a filter hides every entry', async () => {
    render(<HistoryPanel />);
    await screen.findByText('No history yet');
    await userEvent.type(screen.getByPlaceholderText(/search/i), 'orders');
    const state = await screen.findByText('No matching history');
    expect(state.closest('[data-slot="empty-state"]')).toHaveAttribute('data-cause', 'filtered');
    expect(screen.queryByText('No history yet')).toBeNull();
  });

  it('clears the filters and reloads the full list', async () => {
    render(<HistoryPanel />);
    await userEvent.type(await screen.findByPlaceholderText(/search/i), 'orders');
    await userEvent.click(await screen.findByRole('button', { name: 'Clear filters' }));
    await waitFor(() => expect(screen.getByPlaceholderText(/search/i)).toHaveValue(''));
    expect(tauriApi.listHistory).toHaveBeenLastCalledWith(200);
  });

  it('reports a failed load instead of claiming there is no history', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.mocked(tauriApi.listHistory).mockRejectedValueOnce(new Error('boom'));
    render(<HistoryPanel />);
    expect(await screen.findByRole('alert')).toHaveTextContent("Couldn't load history");
    expect(screen.queryByText('No history yet')).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByText('No history yet')).toBeInTheDocument();
  });
});
