import { Profiler, type ProfilerOnRenderCallback } from 'react';
import { act, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { GitCommitLog } from '@/components/git/GitCommitLog';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';

describe('GitCommitLog store subscription', () => {
  it('does not rerender when an unrelated store field changes', () => {
    const store = createGitStore();
    store.setState({
      commitLog: [
        {
          id: 'abc1234',
          fullId: 'abc1234full',
          message: 'initial commit',
          author: 'Test',
          authorEmail: 'test@test.com',
          timestamp: new Date().toISOString(),
          filesChanged: 1,
        },
      ],
    });
    const onRender = vi.fn<ProfilerOnRenderCallback>();

    render(
      <GitStoreProvider store={store}>
        <Profiler id='commit-log' onRender={onRender}>
          <GitCommitLog onCommitClick={() => {}} />
        </Profiler>
      </GitStoreProvider>,
    );
    expect(screen.getByText('initial commit')).toBeInTheDocument();
    const rendersAfterMount = onRender.mock.calls.length;

    act(() => {
      store.setState({ showCredentialsDialog: true });
    });

    expect(onRender.mock.calls.length).toBe(rendersAfterMount);
  });
});

describe('GitCommitLog keyboard behavior', () => {
  it('prevents the default Space-scroll behavior when activating a row', () => {
    const store = createGitStore();
    store.setState({
      commitLog: [
        {
          id: 'abc1234',
          fullId: 'abc1234full',
          message: 'initial commit',
          author: 'Test',
          authorEmail: 'test@test.com',
          timestamp: new Date().toISOString(),
          filesChanged: 1,
        },
      ],
    });
    const onCommitClick = vi.fn();
    render(
      <GitStoreProvider store={store}>
        <GitCommitLog onCommitClick={onCommitClick} />
      </GitStoreProvider>,
    );
    const row = screen.getByRole('button', { name: /initial commit/ });
    const event = new KeyboardEvent('keydown', { key: ' ', bubbles: true, cancelable: true });
    row.dispatchEvent(event);

    expect(onCommitClick).toHaveBeenCalledWith(expect.objectContaining({ id: 'abc1234' }));
    expect(event.defaultPrevented).toBe(true);
  });
});
