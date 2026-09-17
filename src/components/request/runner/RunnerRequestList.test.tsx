import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { usePaneStore } from '@/stores/pane-store';
import type { RunnerRequestEntry, RunnerTab } from '@/types/pane-types';
import { RunnerRequestList } from './RunnerRequestList';

function entry(overrides: Partial<RunnerRequestEntry>): RunnerRequestEntry {
  return {
    requestPath: 'a.yml',
    request: {
      uid: 'a',
      name: 'A',
      method: 'GET',
      url: 'https://example.com/a',
      headers: [],
      auth: { authType: 'none' },
    },
    included: true,
    status: 'pending',
    ...overrides,
  };
}

function tab(requests: RunnerRequestEntry[], runState: RunnerTab['runState'] = 'idle'): RunnerTab {
  return {
    id: 't1',
    title: 'Runner',
    isDirty: false,
    tabType: 'runner',
    collectionName: 'demo',
    runState,
    requests,
  };
}

describe('RunnerRequestList', () => {
  beforeEach(() => usePaneStore.getState().reset());

  it('shows an empty state when there are no requests', () => {
    render(<RunnerRequestList tab={tab([])} />);
    expect(screen.getByText(/no requests/i)).toBeInTheDocument();
  });

  it('renders one row per request with method and name', () => {
    render(
      <RunnerRequestList
        tab={tab([
          entry({ requestPath: 'a.yml' }),
          entry({
            requestPath: 'b.yml',
            request: {
              uid: 'b',
              name: 'B',
              method: 'POST',
              url: '',
              headers: [],
              auth: { authType: 'none' },
            },
          }),
        ])}
      />,
    );
    expect(screen.getByText('A')).toBeInTheDocument();
    expect(screen.getByText('B')).toBeInTheDocument();
    expect(screen.getByText('GET')).toBeInTheDocument();
    expect(screen.getByText('POST')).toBeInTheDocument();
  });

  it('toggling a checkbox calls toggleRunnerEntry with the tab id and request path', () => {
    const runnerTab = tab([entry({ requestPath: 'a.yml' })]);
    // Seed the store so toggleRunnerEntry has a real tab to patch.
    usePaneStore.setState((s) => ({
      root: {
        ...s.root,
        type: 'leaf',
        tabs: [runnerTab],
        activeTabId: runnerTab.id,
      } as typeof s.root,
    }));

    render(<RunnerRequestList tab={runnerTab} />);
    fireEvent.click(screen.getByRole('checkbox'));

    const updated = usePaneStore.getState().root;
    if (updated.type !== 'leaf') throw new Error('Expected leaf root');
    const updatedTab = updated.tabs[0];
    if (updatedTab.tabType !== 'runner') throw new Error('Expected runner tab');
    expect(updatedTab.requests[0].included).toBe(false);
  });

  it('disables checkboxes while a run is in progress', () => {
    render(<RunnerRequestList tab={tab([entry({})], 'running')} />);
    expect(screen.getByRole('checkbox')).toBeDisabled();
  });
});
