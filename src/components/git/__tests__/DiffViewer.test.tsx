import { render, screen } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { DiffViewer } from '@/components/git/DiffViewer';
import type { DiffState } from '@/types/pane-types';

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
