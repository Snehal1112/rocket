import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { type CollectionSettings, getCollectionSettings } from '@/lib/tauri-api';
import { FolderScriptHint } from '../FolderScriptHint';

vi.mock('@/lib/tauri-api', () => ({ getCollectionSettings: vi.fn() }));

const baseSettings = {
  variables: [],
  headers: [],
  sandboxMode: 'safe',
} as unknown as CollectionSettings;

describe('FolderScriptHint', () => {
  beforeEach(() => {
    vi.mocked(getCollectionSettings).mockReset();
  });

  it('renders the sandwich script wording by default with an icon', async () => {
    vi.mocked(getCollectionSettings).mockResolvedValue({ ...baseSettings });
    const { container } = render(<FolderScriptHint kind='script' collectionName='c' />);
    const hint = screen.getByTestId('folder-script-hint');
    expect(hint).toHaveAttribute('role', 'note');
    expect(hint).toHaveTextContent('rok API');
    expect(hint).toHaveTextContent('pre-request scripts run outermost folder first');
    expect(hint).toHaveTextContent('innermost folder first');
    expect(container.querySelector('svg')).not.toBeNull();
  });

  it('renders the sandwich test wording', async () => {
    vi.mocked(getCollectionSettings).mockResolvedValue({ ...baseSettings });
    render(<FolderScriptHint kind='test' collectionName='c' />);
    expect(screen.getByTestId('folder-script-hint')).toHaveTextContent(
      'tests run after the request',
    );
  });

  it('switches to sequential wording when the collection is sequential', async () => {
    vi.mocked(getCollectionSettings).mockResolvedValue({
      ...baseSettings,
      scriptFlow: 'sequential',
    });
    render(<FolderScriptHint kind='script' collectionName='c' />);
    await waitFor(() =>
      expect(screen.getByTestId('folder-script-hint')).toHaveTextContent('sequentially'),
    );
    expect(screen.getByTestId('folder-script-hint')).toHaveTextContent('outermost folder first');
    expect(screen.getByTestId('folder-script-hint')).not.toHaveTextContent('innermost');
  });

  it('keeps the sandwich wording when the settings fetch fails', async () => {
    vi.mocked(getCollectionSettings).mockRejectedValue(new Error('boom'));
    render(<FolderScriptHint kind='test' collectionName='c' />);
    await Promise.resolve();
    expect(screen.getByTestId('folder-script-hint')).toHaveTextContent('innermost folder first');
  });
});
