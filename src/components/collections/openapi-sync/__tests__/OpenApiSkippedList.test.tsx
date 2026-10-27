import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { OpenApiSkippedList } from '../OpenApiSkippedList';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getOpenApiSyncSkipped: vi.fn(), unskipOpenApiSync: vi.fn() };
});

const ROWS = [
  { key: 'listPets', name: 'List pets', method: 'GET', path: '{{baseUrl}}/pets' },
  { key: 'gone', name: 'gone', method: '', path: '' },
];

describe('OpenApiSkippedList', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriApi.getOpenApiSyncSkipped).mockResolvedValue(ROWS);
    vi.mocked(tauriApi.unskipOpenApiSync).mockResolvedValue(1);
  });

  it('shows_each_skipped_item', async () => {
    renderWithQuery(<OpenApiSkippedList collection='c' />);
    expect(await screen.findByText('List pets')).toBeInTheDocument();
    expect(screen.getByText('GET')).toBeInTheDocument();
    expect(screen.getByText('{{baseUrl}}/pets')).toBeInTheDocument();
    expect(screen.getByText('gone')).toBeInTheDocument();
  });

  it('unskip_calls_the_hook_with_the_key', async () => {
    renderWithQuery(<OpenApiSkippedList collection='c' />);
    const row = (await screen.findByText('List pets')).closest('li') as HTMLElement;
    await userEvent.setup().click(within(row).getByRole('button', { name: 'Un-skip' }));
    await waitFor(() =>
      expect(tauriApi.unskipOpenApiSync).toHaveBeenCalledWith('c', ['listPets']),
    );
  });

  it('unskip_all_sends_every_key', async () => {
    renderWithQuery(<OpenApiSkippedList collection='c' />);
    await screen.findByText('List pets');
    await userEvent.setup().click(screen.getByRole('button', { name: 'Un-skip all (2)' }));
    await waitFor(() =>
      expect(tauriApi.unskipOpenApiSync).toHaveBeenCalledWith('c', ['listPets', 'gone']),
    );
  });

  it('the_list_shows_a_fixed_sentence_when_loading_fails', async () => {
    vi.mocked(tauriApi.getOpenApiSyncSkipped).mockRejectedValue('cannot read /home/me/secret');
    renderWithQuery(<OpenApiSkippedList collection='c' />);
    expect(await screen.findByText(/skipped items could not be loaded/i)).toBeInTheDocument();
    expect(screen.queryByText(/secret/)).toBeNull();
  });

  it('an_empty_list_shows_nothing_skipped', async () => {
    vi.mocked(tauriApi.getOpenApiSyncSkipped).mockResolvedValue([]);
    renderWithQuery(<OpenApiSkippedList collection='c' />);
    expect(await screen.findByText('Nothing is skipped.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Un-skip all/ })).toBeNull();
  });

  it('a_failed_unskip_shows_a_fixed_sentence', async () => {
    vi.mocked(tauriApi.unskipOpenApiSync).mockRejectedValue('A sync is running. Try again when it is done.');
    renderWithQuery(<OpenApiSkippedList collection='c' />);
    const row = (await screen.findByText('List pets')).closest('li') as HTMLElement;
    await userEvent.setup().click(within(row).getByRole('button', { name: 'Un-skip' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('A sync is running.');
  });
});
