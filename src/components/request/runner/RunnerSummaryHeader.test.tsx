import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { usePaneStore } from '@/stores/pane-store';
import type { RunnerRequestEntry, RunnerTab } from '@/types/pane-types';
import { RunnerSummaryHeader } from './RunnerSummaryHeader';

function entry(overrides: Partial<RunnerRequestEntry>): RunnerRequestEntry {
  return {
    requestPath: 'a.yml',
    request: {
      uid: 'a',
      name: 'A',
      method: 'GET',
      url: '',
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

function seedStore(runnerTab: RunnerTab) {
  usePaneStore.setState((s) => ({
    root: {
      ...s.root,
      type: 'leaf',
      tabs: [runnerTab],
      activeTabId: runnerTab.id,
    } as typeof s.root,
  }));
}

describe('RunnerSummaryHeader', () => {
  beforeEach(() => usePaneStore.getState().reset());

  it('shows pass/fail counts derived from the tab', () => {
    render(
      <RunnerSummaryHeader
        tab={tab(
          [entry({ status: 'passed' }), entry({ requestPath: 'b.yml', status: 'failed' })],
          'done',
        )}
      />,
    );
    expect(screen.getByText(/1.*passed/i)).toBeInTheDocument();
    expect(screen.getByText(/1.*failed/i)).toBeInTheDocument();
  });

  it('shows a Start button when idle, disabled if nothing is included', () => {
    render(<RunnerSummaryHeader tab={tab([entry({ included: false })])} />);
    expect(screen.getByRole('button', { name: /start/i })).toBeDisabled();
  });

  it('clicking Start calls startRun with the tab id', () => {
    const runnerTab = tab([entry({})]);
    seedStore(runnerTab);
    render(<RunnerSummaryHeader tab={runnerTab} />);
    fireEvent.click(screen.getByRole('button', { name: /start/i }));
    const updated = usePaneStore.getState().root;
    if (updated.type !== 'leaf') throw new Error('Expected leaf root');
    const updatedTab = updated.tabs[0];
    if (updatedTab.tabType !== 'runner') throw new Error('Expected runner tab');
    expect(updatedTab.runState).not.toBe('idle');
  });

  it('shows a Stop button while running, and clicking it calls stopRun', () => {
    const runnerTab = tab([entry({ status: 'running' })], 'running');
    seedStore(runnerTab);
    render(<RunnerSummaryHeader tab={runnerTab} />);
    fireEvent.click(screen.getByRole('button', { name: /stop/i }));
    const updated = usePaneStore.getState().root;
    if (updated.type !== 'leaf') throw new Error('Expected leaf root');
    const updatedTab = updated.tabs[0];
    if (updatedTab.tabType !== 'runner') throw new Error('Expected runner tab');
    expect(updatedTab.runState).toBe('stopped');
  });

  it('shows a Re-run button once done', () => {
    render(<RunnerSummaryHeader tab={tab([entry({ status: 'passed' })], 'done')} />);
    expect(screen.getByRole('button', { name: /re-run/i })).toBeInTheDocument();
  });
});
