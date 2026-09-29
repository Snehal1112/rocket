import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { useConsoleStore } from '@/stores/console-store';
import { ConsolePanel } from '../ConsolePanel';

const seed = (requestName?: string) =>
  useConsoleStore.getState().addHttpEntry({
    requestName,
    method: 'GET',
    url: 'https://x.test/a',
    status: 200,
    statusText: 'OK',
    durationMs: 3,
    sizeBytes: 1,
    requestHeaders: [],
    requestBody: '',
    responseHeaders: [],
    responseBody: '',
  });

describe('ConsolePanel request name', () => {
  beforeEach(() => useConsoleStore.getState().clearEntries());

  it('shows the request name in an HTTP row', () => {
    seed('login-flow › Login');
    render(<ConsolePanel isOpen height={200} onHeightChange={() => undefined} />);
    expect(screen.getByText('login-flow › Login')).toBeInTheDocument();
  });

  it('shows no name element for an HTTP row without one', () => {
    seed();
    render(<ConsolePanel isOpen height={200} onHeightChange={() => undefined} />);
    expect(screen.getByText('https://x.test/a')).toBeInTheDocument();
    expect(screen.queryByTestId('console-request-name')).toBeNull();
  });
});
