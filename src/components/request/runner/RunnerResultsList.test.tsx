import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { RunnerRequestEntry, RunnerTab } from '@/types/pane-types';
import { RunnerResultsList } from './RunnerResultsList';

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

function tab(requests: RunnerRequestEntry[]): RunnerTab {
  return {
    id: 't1',
    title: 'Runner',
    isDirty: false,
    tabType: 'runner',
    collectionName: 'demo',
    runState: 'done',
    requests,
  };
}

describe('RunnerResultsList', () => {
  it('renders a row per request with its status', () => {
    render(
      <RunnerResultsList
        tab={tab([
          entry({ requestPath: 'a.yml', status: 'passed' }),
          entry({
            requestPath: 'b.yml',
            status: 'failed',
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
  });

  it('shows a skipped row distinctly', () => {
    render(<RunnerResultsList tab={tab([entry({ status: 'skipped' })])} />);
    expect(screen.getByText(/skipped/i)).toBeInTheDocument();
  });

  it('shows the error message for a row that threw before executing', () => {
    render(<RunnerResultsList tab={tab([entry({ status: 'failed', error: 'network down' })])} />);
    expect(screen.getByText('network down')).toBeInTheDocument();
  });

  it('expands a row to show its test results via TestsPanel', () => {
    render(
      <RunnerResultsList
        tab={tab([
          entry({
            status: 'passed',
            result: {
              status: 200,
              statusText: 'OK',
              headers: [],
              body: '',
              durationMs: 12,
              ttfbMs: 5,
              sizeBytes: 0,
              testResults: [{ name: 'status is 200', status: 'passed', error: null }],
              consoleEntries: [],
              scriptError: null,
            },
          }),
        ])}
      />,
    );

    // Detail is collapsed until the row is clicked.
    expect(screen.queryByText('status is 200')).not.toBeInTheDocument();
    fireEvent.click(screen.getByText('A'));
    expect(screen.getByText('status is 200')).toBeInTheDocument();
  });
});
