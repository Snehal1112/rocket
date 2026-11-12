import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

const colorCalls = vi.hoisted(() => ({ count: 0 }));
vi.mock('@/lib/colors', async () => {
  const actual = await vi.importActual<typeof import('@/lib/colors')>('@/lib/colors');
  return {
    ...actual,
    // Called once per ResultRow render that has a result.
    statusTextColor: (status: number) => {
      colorCalls.count += 1;
      return actual.statusTextColor(status);
    },
  };
});

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

  it('groups results under Pass headings when the run has a data file', () => {
    const base = tab([entry({ status: 'passed' })]);
    render(
      <RunnerResultsList
        tab={{
          ...base,
          iterationData: { columns: ['id'], rows: [{}, {}, {}], source: 'perRun', fileName: 'r.csv' },
          completedPasses: [
            { iteration: 0, requests: [entry({ status: 'passed' })] },
            { iteration: 1, requests: [entry({ status: 'failed' })] },
            { iteration: 2, requests: [entry({ status: 'skipped' })] },
          ],
        }}
      />,
    );
    expect(screen.getByText('Pass 1 of 3')).toBeInTheDocument();
    expect(screen.getByText('Pass 2 of 3')).toBeInTheDocument();
    expect(screen.getByText('Pass 3 of 3')).toBeInTheDocument();
  });

  it('shows a plain list without Pass headings when there is no data file', () => {
    render(<RunnerResultsList tab={tab([entry({ status: 'passed' })])} />);
    expect(screen.queryByText(/^Pass \d/)).not.toBeInTheDocument();
  });

  it('re-renders only the open pass when one entry changes', () => {
    const result = {
      status: 200,
      statusText: 'OK',
      headers: [],
      body: '',
      durationMs: 1,
      ttfbMs: 1,
      sizeBytes: 0,
      testResults: [],
      consoleEntries: [],
      scriptError: null,
    };
    const done = (n: number) => ({
      iteration: n,
      requests: [
        entry({ requestPath: 'a.yml', status: 'passed', result }),
        entry({ requestPath: 'b.yml', status: 'passed', result }),
      ],
    });
    const closed = [done(0), done(1), done(2), done(3), done(4)];
    const open = [
      entry({ requestPath: 'a.yml', status: 'passed', result }),
      entry({ requestPath: 'b.yml', status: 'running' }),
    ];
    const data = {
      columns: ['id'],
      rows: [{}, {}, {}, {}, {}, {}],
      source: 'perRun' as const,
      fileName: 'r.csv',
    };
    const base = { ...tab(open), runState: 'running' as const, iterationData: data };
    colorCalls.count = 0;
    const { rerender } = render(
      <RunnerResultsList tab={{ ...base, iterationIndex: 5, completedPasses: closed }} />,
    );
    // Five closed passes of two rows, plus one open row with a result.
    expect(colorCalls.count).toBe(11);

    colorCalls.count = 0;
    const open2 = [open[0], entry({ requestPath: 'b.yml', status: 'passed', result })];
    rerender(
      <RunnerResultsList
        tab={{ ...base, requests: open2, iterationIndex: 5, completedPasses: closed }}
      />,
    );
    // Only the changed row renders again.
    expect(colorCalls.count).toBe(1);
  });
});
