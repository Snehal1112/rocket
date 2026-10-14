import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { useConsoleStore } from '@/stores/console-store';
import { ConsolePanel } from '../ConsolePanel';

const M = '••••••';
const empty = {
  requestHeaders: [],
  requestBody: '',
  responseHeaders: [],
  responseBody: '',
};

function seed(extra: Record<string, unknown> = {}) {
  useConsoleStore.getState().addHttpEntry({
    method: 'GET',
    url: 'https://x.test/a?k=abcdef123',
    status: 200,
    statusText: 'OK',
    durationMs: 3,
    sizeBytes: 1,
    ...empty,
    requestBody: 'token=abcdef123',
    masked: {
      url: `https://x.test/a?k=${M}`,
      ...empty,
      requestBody: `token=${M}`,
    },
    ...extra,
  });
}

function open() {
  render(<ConsolePanel isOpen height={200} onHeightChange={() => undefined} />);
}

function expandRow() {
  fireEvent.click(screen.getByText(`https://x.test/a?k=${M}`));
}

describe('ConsolePanel masking', () => {
  beforeEach(() => useConsoleStore.getState().clearEntries());

  it('shows the masked url in the row line', () => {
    seed();
    open();
    expect(screen.getByText(`https://x.test/a?k=${M}`)).toBeInTheDocument();
    expect(screen.queryByText('https://x.test/a?k=abcdef123')).toBeNull();
  });

  it('shows masked text in the detail by default', () => {
    seed();
    open();
    expandRow();
    expect(screen.getByText(`token=${M}`)).toBeInTheDocument();
    expect(screen.queryByText('token=abcdef123')).toBeNull();
  });

  it('reveals the raw text with the toggle and masks it again', () => {
    seed();
    open();
    expandRow();
    fireEvent.click(screen.getByRole('button', { name: 'Show secrets' }));
    expect(screen.getByText('token=abcdef123')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Hide secrets' }));
    expect(screen.getByText(`token=${M}`)).toBeInTheDocument();
  });

  it('masks the row again after it is collapsed and reopened', () => {
    seed();
    open();
    expandRow();
    fireEvent.click(screen.getByRole('button', { name: 'Show secrets' }));
    fireEvent.click(screen.getByText('https://x.test/a?k=abcdef123'));
    fireEvent.click(screen.getByText(`https://x.test/a?k=${M}`));
    expect(screen.getByText(`token=${M}`)).toBeInTheDocument();
  });

  it('does not find a masked row by its secret', () => {
    seed();
    open();
    fireEvent.change(screen.getByPlaceholderText('Filter by URL or message'), {
      target: { value: 'abcdef123' },
    });
    expect(screen.queryByText(`https://x.test/a?k=${M}`)).toBeNull();
  });

  it('finds the revealed row by its raw url', () => {
    seed();
    open();
    expandRow();
    fireEvent.click(screen.getByRole('button', { name: 'Show secrets' }));
    fireEvent.change(screen.getByPlaceholderText('Filter by URL or message'), {
      target: { value: 'abcdef123' },
    });
    expect(screen.getByText('https://x.test/a?k=abcdef123')).toBeInTheDocument();
  });

  it('renders a row without a masked copy as before', () => {
    useConsoleStore.getState().addHttpEntry({
      method: 'GET',
      url: 'https://flow.test/raw',
      status: 200,
      statusText: 'OK',
      durationMs: 1,
      sizeBytes: 1,
      ...empty,
      requestBody: 'plain-body',
    });
    open();
    fireEvent.click(screen.getByText('https://flow.test/raw'));
    expect(screen.getByText('plain-body')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Show secrets' })).toBeNull();
  });

  it('shows no toggle when nothing is hidden', () => {
    seed({
      url: 'https://x.test/plain',
      requestBody: 'plain',
      masked: { url: 'https://x.test/plain', ...empty, requestBody: 'plain' },
    });
    open();
    fireEvent.click(screen.getByText('https://x.test/plain'));
    expect(screen.queryByRole('button', { name: 'Show secrets' })).toBeNull();
  });

  it('shows the note and no toggle for an uncertain row', () => {
    seed({ uncertain: true });
    open();
    expandRow();
    expect(screen.getByText('May contain secrets')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Show secrets' })).toBeNull();
    expect(screen.getByText(`token=${M}`)).toBeInTheDocument();
  });
});
