import { render, waitFor } from '@testing-library/react';
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
