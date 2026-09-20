import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { DiffViewer } from '@/components/git/DiffViewer';
import * as tauriApi from '@/lib/tauri-api';
import { createDeferred } from '@/test/deferred';
import type { DiffState } from '@/types/pane-types';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, gitDiff: vi.fn(), gitDiffStaged: vi.fn() };
});

// jsdom does not implement window.matchMedia. useMonacoTheme calls it to track OS dark-mode changes.
beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: vi.fn().mockImplementation((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  });
});

// Monaco cannot render in jsdom (hits browser-only APIs at import time).
vi.mock('@/components/editor/monaco-setup', () => ({}));
vi.mock('@monaco-editor/react', () => ({
  DiffEditor: () => <div data-testid='diff-editor' />,
  loader: {
    init: vi.fn().mockResolvedValue({
      editor: {
        defineTheme: vi.fn(),
        setTheme: vi.fn(),
      },
    }),
  },
}));

const diffState: DiffState = {
  filePath: 'collection.yml',
  repositoryId: 'repo-1',
  repositoryLabel: 'Repo',
  oldContent: 'old',
  newContent: 'new',
  status: 'modified',
  isStaged: false,
};

describe('DiffViewer persisted mode validation', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('falls back to text mode when the stored value is invalid', () => {
    localStorage.setItem('git-diff-mode', 'not-a-real-mode');
    render(<DiffViewer diffState={diffState} />);
    expect(screen.getByRole('tab', { name: 'Text' })).toHaveAttribute('data-state', 'active');
  });

  it('honors a valid stored value', () => {
    localStorage.setItem('git-diff-mode', 'visual');
    render(<DiffViewer diffState={diffState} />);
    expect(screen.getByRole('tab', { name: 'Visual' })).toHaveAttribute('data-state', 'active');
  });
});

describe('DiffViewer toggle resilience', () => {
  it('keeps the newer response when an older toggle resolves out of order', async () => {
    const deferredStaged = createDeferred<tauriApi.FileDiff>();
    const deferredWorking = createDeferred<tauriApi.FileDiff>();
    vi.mocked(tauriApi.gitDiffStaged).mockReturnValue(deferredStaged.promise);
    vi.mocked(tauriApi.gitDiff).mockReturnValue(deferredWorking.promise);

    render(<DiffViewer diffState={{ ...diffState, filePath: 'plain.ts' }} />);
    const user = userEvent.setup();

    await user.click(screen.getByRole('tab', { name: 'Staged' }));
    await user.click(screen.getByRole('tab', { name: 'Working' }));

    // The newer (working) request resolves first, the stale (staged) request
    // resolves after — the stale one must be discarded.
    deferredWorking.resolve({
      path: 'plain.ts',
      oldContent: 'old-working',
      newContent: 'new-working',
      hunks: [],
    });
    await vi.waitFor(() =>
      expect(screen.getByRole('tab', { name: 'Working' })).toHaveAttribute('data-state', 'active'),
    );
    deferredStaged.resolve({
      path: 'plain.ts',
      oldContent: 'old-staged',
      newContent: 'new-staged',
      hunks: [],
    });
    await Promise.resolve();

    expect(screen.getByRole('tab', { name: 'Working' })).toHaveAttribute('data-state', 'active');
  });

  it('disables the toggle while a request is in flight and shows an error on failure', async () => {
    const deferred = createDeferred<tauriApi.FileDiff>();
    vi.mocked(tauriApi.gitDiffStaged).mockReturnValue(deferred.promise);

    render(<DiffViewer diffState={{ ...diffState, filePath: 'plain.ts' }} />);
    const user = userEvent.setup();

    await user.click(screen.getByRole('tab', { name: 'Staged' }));
    expect(screen.getByRole('tab', { name: 'Working' })).toBeDisabled();

    deferred.reject(new Error('diff unavailable'));
    expect(await screen.findByText(/diff unavailable/)).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Working' })).not.toBeDisabled();
  });
});
