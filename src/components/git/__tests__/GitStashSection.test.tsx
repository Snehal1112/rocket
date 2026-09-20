import { Profiler, type ProfilerOnRenderCallback } from 'react';
import { act, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { GitStashSection } from '@/components/git/GitStashSection';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';

describe('GitStashSection store subscription', () => {
  it('does not rerender when an unrelated store field changes', () => {
    const store = createGitStore();
    store.setState({
      stashes: [
        {
          index: 0,
          message: 'wip',
          timestamp: new Date().toISOString(),
          filesChanged: 1,
          insertions: 1,
          deletions: 0,
          changedFiles: ['a.txt'],
          branch: 'main',
        },
      ],
    });
    const onRender = vi.fn<ProfilerOnRenderCallback>();

    render(
      <GitStoreProvider store={store}>
        <Profiler id='stash-section' onRender={onRender}>
          <GitStashSection />
        </Profiler>
      </GitStoreProvider>,
    );
    expect(screen.getByText('wip')).toBeInTheDocument();
    const rendersAfterMount = onRender.mock.calls.length;

    act(() => {
      store.setState({ showCredentialsDialog: true });
    });

    expect(onRender.mock.calls.length).toBe(rendersAfterMount);
  });
});
