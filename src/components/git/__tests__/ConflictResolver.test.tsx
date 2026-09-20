import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import { ConflictResolver } from '@/components/git/ConflictResolver';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';
import type { ConflictState } from '@/types/pane-types';

// jsdom does not implement window.matchMedia. useMonacoTheme (used by
// ConflictResolver's Editor instances) calls it to track OS dark-mode
// changes, so it needs a minimal stub for this component to render at all.
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

// The real Monaco editor cannot render in jsdom (it hits browser-only APIs
// like document.queryCommandSupported at import time), and monaco-setup.ts
// eagerly imports the real 'monaco-editor' package as a side effect. This
// mirrors the existing convention in BodyEditor.test.tsx, which stubs out
// the Monaco-backed component under test rather than the real editor.
vi.mock('@/components/editor/monaco-setup', () => ({}));
vi.mock('@monaco-editor/react', () => ({
  default: ({
    value,
    onChange,
    options,
  }: {
    value?: string;
    onChange?: (value: string | undefined) => void;
    options?: { readOnly?: boolean };
  }) =>
    options?.readOnly ? (
      // Read-only "Ours"/"Theirs" panes are rendered as plain, non-editable
      // text so they don't collide with role "textbox" queries aimed at the
      // single editable manual-resolution editor.
      <pre>{value ?? ''}</pre>
    ) : (
      <textarea value={value ?? ''} onChange={(e) => onChange?.(e.target.value)} />
    ),
  loader: {
    config: vi.fn(),
    init: vi.fn().mockResolvedValue({ editor: { setTheme: vi.fn(), defineTheme: vi.fn() } }),
  },
}));

function baseConflict(overrides: Partial<ConflictState> = {}): ConflictState {
  return {
    filePath: 'a.txt',
    repositoryId: 'repo-1',
    repositoryLabel: 'Repo',
    ours: 'ours content',
    theirs: 'theirs content',
    ancestor: null,
    ...overrides,
  };
}

describe('ConflictResolver', () => {
  it('resets manual mode and manual content when the conflict file changes', async () => {
    const user = userEvent.setup();
    const store = createGitStore();
    store.setState({
      resolveConflict: vi.fn().mockResolvedValue(undefined),
      abortMerge: vi.fn().mockResolvedValue(undefined),
      error: null,
      clearError: vi.fn(),
    });

    const view = render(
      <GitStoreProvider store={store}>
        <ConflictResolver conflictState={baseConflict({ ours: 'file A ours' })} />
      </GitStoreProvider>,
    );

    await user.click(screen.getByRole('button', { name: 'Edit Manually' }));
    const editorTextbox = screen.getByRole('textbox');
    expect(editorTextbox).toHaveValue('file A ours');
    await user.clear(editorTextbox);
    await user.type(editorTextbox, 'edited by user');

    view.rerender(
      <GitStoreProvider store={store}>
        <ConflictResolver
          conflictState={baseConflict({ filePath: 'b.txt', ours: 'file B ours' })}
        />
      </GitStoreProvider>,
    );

    // Manual mode must reset when the target file changes — otherwise the
    // edited content for a.txt could be saved into b.txt.
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Edit Manually' }));
    expect(screen.getByRole('textbox')).toHaveValue('file B ours');
  });

  it('disables Accept Ours/Theirs while a resolve is in flight and does not allow a second submit', async () => {
    let resolveResolve!: () => void;
    const resolveConflict = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveResolve = resolve;
        }),
    );
    const { getByRole } = render(
      <GitStoreProvider
        store={(() => {
          const store = createGitStore();
          store.setState({
            resolveConflict,
            abortMerge: vi.fn(),
            error: null,
            clearError: vi.fn(),
          });
          return store;
        })()}
      >
        <ConflictResolver conflictState={baseConflict()} />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();

    const oursButton = getByRole('button', { name: 'Accept Ours' });
    await user.click(oursButton);
    expect(oursButton).toBeDisabled();
    await user.click(oursButton);
    expect(resolveConflict).toHaveBeenCalledTimes(1);

    resolveResolve();
  });

  it('requires confirmation before Abort Merge calls the store action', async () => {
    const abortMerge = vi.fn().mockResolvedValue(undefined);
    const store = createGitStore();
    store.setState({ resolveConflict: vi.fn(), abortMerge, error: null, clearError: vi.fn() });
    render(
      <GitStoreProvider store={store}>
        <ConflictResolver conflictState={baseConflict()} />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: 'Abort Merge' }));
    expect(abortMerge).not.toHaveBeenCalled();
    expect(screen.getByText('Abort Merge?')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Confirm Abort' }));
    expect(abortMerge).toHaveBeenCalledTimes(1);
  });
});

describe('ConflictResolver onResolved for abort', () => {
  it('calls onResolved after a successful abort, not just a successful resolve', async () => {
    const onResolved = vi.fn();
    const store = createGitStore();
    store.setState({
      abortMerge: async () => {
        store.setState({ error: null });
      },
    });
    render(
      <GitStoreProvider store={store}>
        <ConflictResolver
          conflictState={{
            filePath: 'a.txt',
            repositoryId: 'repo-1',
            repositoryLabel: 'Repo',
            ours: 'ours',
            theirs: 'theirs',
            ancestor: null,
          }}
          onResolved={onResolved}
        />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /abort merge/i }));
    await user.click(screen.getByRole('button', { name: /confirm abort/i }));

    expect(onResolved).toHaveBeenCalledTimes(1);
  });
});
