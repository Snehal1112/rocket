import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { GitFileList } from '@/components/git/GitFileList';
import type { RepoStatus } from '@/lib/tauri-api';
import { createGitStore, type GitState } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';
import { createDeferred } from '@/test/deferred';

const baseStatus: RepoStatus = {
  branch: 'main',
  isClean: false,
  ahead: 0,
  behind: 0,
  files: [{ path: 'notes.txt', status: 'modified', staged: false }],
};

function renderWithStore(patch: Partial<GitState> = {}) {
  const store = createGitStore();
  const discardFiles = vi.fn();
  store.setState({ status: baseStatus, discardFiles, ...patch });
  render(
    <GitStoreProvider store={store}>
      <GitFileList onFileClick={vi.fn()} onConflictClick={vi.fn()} />
    </GitStoreProvider>,
  );
  return { discardFiles: store.getState().discardFiles };
}

describe('GitFileList keyboard behavior', () => {
  it('prevents the default Space-scroll behavior when activating an unstaged file row', () => {
    const store = createGitStore();
    store.setState({ status: baseStatus });
    const onFileClick = vi.fn();
    const onConflictClick = vi.fn();
    render(
      <GitStoreProvider store={store}>
        <GitFileList onFileClick={onFileClick} onConflictClick={onConflictClick} />
      </GitStoreProvider>,
    );
    const row = screen.getByRole('button', { name: /notes.txt/ });
    const event = new KeyboardEvent('keydown', { key: ' ', bubbles: true, cancelable: true });
    row.dispatchEvent(event);

    expect(onFileClick).toHaveBeenCalledWith(expect.objectContaining({ path: 'notes.txt' }));
    expect(event.defaultPrevented).toBe(true);
  });

  it('prevents the default Space-scroll behavior when activating a staged file row', () => {
    const store = createGitStore();
    const stagedStatus: RepoStatus = {
      branch: 'main',
      isClean: false,
      ahead: 0,
      behind: 0,
      files: [{ path: 'staged-file.txt', status: 'modified', staged: true }],
    };
    store.setState({ status: stagedStatus });
    const onFileClick = vi.fn();
    const onConflictClick = vi.fn();
    render(
      <GitStoreProvider store={store}>
        <GitFileList onFileClick={onFileClick} onConflictClick={onConflictClick} />
      </GitStoreProvider>,
    );
    const row = screen.getByRole('button', { name: /staged-file.txt/ });
    const event = new KeyboardEvent('keydown', { key: ' ', bubbles: true, cancelable: true });
    row.dispatchEvent(event);

    expect(onFileClick).toHaveBeenCalledWith(expect.objectContaining({ path: 'staged-file.txt' }));
    expect(event.defaultPrevented).toBe(true);
  });
});

describe('GitFileList individual discard confirmation', () => {
  it('does not call discardFiles until the confirmation dialog is confirmed', async () => {
    const { discardFiles } = renderWithStore();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: 'Discard' }));
    expect(discardFiles).not.toHaveBeenCalled();
    expect(screen.getByText('Discard Changes?')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Discard' }));
    expect(discardFiles).toHaveBeenCalledWith(['notes.txt']);
  });

  it('cancel performs no discard', async () => {
    const { discardFiles } = renderWithStore();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: 'Discard' }));
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(discardFiles).not.toHaveBeenCalled();
  });
});

describe('GitFileList duplicate-action guards', () => {
  it('disables Stage all while a bulk stage is in flight and only calls stageAll once', async () => {
    const deferred = createDeferred<void>();
    const stageAll = vi.fn(() => deferred.promise);
    renderWithStore({
      status: {
        branch: 'main',
        files: [{ path: 'a.txt', staged: false, status: 'modified' }],
        ahead: 0,
        behind: 0,
        isClean: false,
      },
      stageAll,
    });
    const user = userEvent.setup();

    const stageAllButton = screen.getByRole('button', { name: /stage all/i });
    await user.click(stageAllButton);
    expect(stageAllButton).toBeDisabled();
    await user.click(stageAllButton); // no-op — disabled

    deferred.resolve();
    await vi.waitFor(() => expect(stageAllButton).not.toBeDisabled());
    expect(stageAll).toHaveBeenCalledTimes(1);
  });

  it('disables a row while its own stage action is in flight', async () => {
    const deferred = createDeferred<void>();
    const stageFiles = vi.fn(() => deferred.promise);
    renderWithStore({
      status: {
        branch: 'main',
        files: [{ path: 'a.txt', staged: false, status: 'modified' }],
        ahead: 0,
        behind: 0,
        isClean: false,
      },
      stageFiles,
    });
    const user = userEvent.setup();

    const stageButton = screen.getByRole('button', { name: 'Stage' });
    await user.click(stageButton);
    expect(stageButton).toBeDisabled();

    deferred.resolve();
    await vi.waitFor(() => expect(stageFiles).toHaveBeenCalledTimes(1));
  });
});

describe('GitFileList accessible names', () => {
  it('gives the bulk stage/unstage/discard controls accessible names', () => {
    renderWithStore({
      status: {
        branch: 'main',
        files: [
          { path: 'staged.txt', staged: true, status: 'modified' },
          { path: 'unstaged.txt', staged: false, status: 'modified' },
        ],
        ahead: 0,
        behind: 0,
        isClean: false,
      },
    });
    expect(screen.getByRole('button', { name: 'Unstage all' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Stage all' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Discard all unstaged' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Unstage' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Stage' })).toBeInTheDocument();
  });
});
