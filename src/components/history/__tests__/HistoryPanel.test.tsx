import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
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

function entry(id: string, runSource?: tauriApi.RunSource): tauriApi.HistoryEntry {
  return {
    id,
    method: 'GET',
    url: `https://api.example.com/${id}`,
    status: 200,
    durationMs: 5,
    responseSize: 0,
    timestamp: '2026-10-11T10:00:00Z',
    runSource,
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
