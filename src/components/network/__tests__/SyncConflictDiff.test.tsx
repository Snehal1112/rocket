import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { SyncConflictDiff } from '@/components/network/SyncConflictDiff';
import { createDeferred } from '@/test/deferred';

const mocks = vi.hoisted(() => ({
  api: { getNetworkSyncConflictFiles: vi.fn() },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('@/components/git/DiffViewer', () => ({
  DiffViewer: (props: {
    diffState: { filePath: string; oldContent: string; newContent: string; status: string };
    hideStageToggle?: boolean;
  }) => (
    <div
      data-testid='diff'
      data-path={props.diffState.filePath}
      data-status={props.diffState.status}
      data-hide-stage={String(props.hideStageToggle)}
    >
      <pre data-testid='diff-old'>{props.diffState.oldContent}</pre>
      <pre data-testid='diff-new'>{props.diffState.newContent}</pre>
    </div>
  ),
}));

const conflict = { path: 'a.yml', kind: 'both_changed' as const, mineHash: 'h1', theirsHash: 'h2' };

describe('SyncConflictDiff', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows mine on the left and theirs on the right through the git diff viewer', async () => {
    mocks.api.getNetworkSyncConflictFiles.mockResolvedValue({ mine: 'url: a', theirs: 'url: b' });
    render(<SyncConflictDiff syncId='s1' conflict={conflict} peerName='Ben Ortiz' />);
    expect(await screen.findByTestId('diff')).toHaveAttribute('data-path', 'a.yml');
    expect(screen.getByTestId('diff-old')).toHaveTextContent('url: a');
    expect(screen.getByTestId('diff-new')).toHaveTextContent('url: b');
    expect(mocks.api.getNetworkSyncConflictFiles).toHaveBeenCalledWith('s1', 'a.yml');
    expect(screen.getByText('Left: your copy. Right: the copy of Ben Ortiz.')).toBeInTheDocument();
  });

  it('hides the stage toggle', async () => {
    mocks.api.getNetworkSyncConflictFiles.mockResolvedValue({ mine: 'a', theirs: 'b' });
    render(<SyncConflictDiff syncId='s1' conflict={conflict} peerName='Ben Ortiz' />);
    expect(await screen.findByTestId('diff')).toHaveAttribute('data-hide-stage', 'true');
  });

  it('shows an added file against an empty left side', async () => {
    mocks.api.getNetworkSyncConflictFiles.mockResolvedValue({ mine: null, theirs: 'new' });
    render(
      <SyncConflictDiff
        syncId='s1'
        conflict={{ ...conflict, kind: 'delete_vs_edit', mineHash: null }}
        peerName='Ben Ortiz'
      />,
    );
    expect(await screen.findByTestId('diff-old')).toHaveTextContent('');
    expect(screen.getByTestId('diff-new')).toHaveTextContent('new');
  });

  it('shows a loading state first', async () => {
    const files = createDeferred<{ mine: string; theirs: string }>();
    mocks.api.getNetworkSyncConflictFiles.mockReturnValue(files.promise);
    render(<SyncConflictDiff syncId='s1' conflict={conflict} peerName='Ben Ortiz' />);
    expect(screen.getByText('Loading the files...')).toBeInTheDocument();
    files.resolve({ mine: 'a', theirs: 'b' });
    expect(await screen.findByTestId('diff')).toBeInTheDocument();
  });

  it('shows the error in a state block when the files cannot be loaded', async () => {
    mocks.api.getNetworkSyncConflictFiles.mockRejectedValue('unavailable');
    render(<SyncConflictDiff syncId='s1' conflict={conflict} peerName='Ben Ortiz' />);
    const block = await screen.findByText('The files could not be loaded. You can still choose.');
    expect(block.closest('[data-slot="state-block"]')).toHaveAttribute('data-tone', 'error');
    expect(screen.queryByTestId('diff')).toBeNull();
  });

  it('does not show the result of a request for another conflict', async () => {
    const first = createDeferred<{ mine: string; theirs: string }>();
    mocks.api.getNetworkSyncConflictFiles
      .mockReturnValueOnce(first.promise)
      .mockResolvedValueOnce({ mine: 'second', theirs: 'second' });
    const view = render(<SyncConflictDiff syncId='s1' conflict={conflict} peerName='Ben' />);
    view.rerender(
      <SyncConflictDiff syncId='s1' conflict={{ ...conflict, path: 'b.yml' }} peerName='Ben' />,
    );
    expect(await screen.findByTestId('diff-new')).toHaveTextContent('second');
    first.resolve({ mine: 'first', theirs: 'first' });
    await Promise.resolve();
    expect(screen.getByTestId('diff-new')).toHaveTextContent('second');
  });
});
