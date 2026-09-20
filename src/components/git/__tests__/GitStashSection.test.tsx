import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Profiler, type ProfilerOnRenderCallback } from 'react';
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

describe('GitStashSection stash selection', () => {
  it('selects a stash via an accessible checkbox', async () => {
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
    render(
      <GitStoreProvider store={store}>
        <GitStashSection />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();

    // The checkbox is hidden until hover/selection; hover the row first.
    await user.hover(screen.getByText('wip'));
    const checkbox = await screen.findByRole('checkbox');
    await user.click(checkbox);

    expect(checkbox).toBeChecked();
    expect(await screen.findByText('1 selected')).toBeInTheDocument();
  });
});
