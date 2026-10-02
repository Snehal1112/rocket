import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { useConsoleStore } from '@/stores/console-store';
import { ConsolePanel } from '../ConsolePanel';

const addLog = (level: string, message: string) =>
  useConsoleStore.getState().addLogEntry({
    timestamp: '2026-10-02T10:00:00Z',
    level,
    target: 'rocket_http::client',
    message,
    fields: { status: '500' },
    spanFields: { method: 'GET' },
  });

const renderPanel = () =>
  render(<ConsolePanel isOpen height={200} onHeightChange={() => undefined} />);

describe('ConsolePanel backend logs', () => {
  beforeEach(() => useConsoleStore.getState().clearEntries());

  it('shows WARN and ERROR by default and hides INFO', () => {
    addLog('WARN', 'slow upstream');
    addLog('ERROR', 'request failed');
    addLog('INFO', 'request started');
    renderPanel();
    expect(screen.getByText('slow upstream')).toBeInTheDocument();
    expect(screen.getByText('request failed')).toBeInTheDocument();
    expect(screen.queryByText('request started')).toBeNull();
  });

  it('includes INFO when the toggle is on', () => {
    addLog('INFO', 'request started');
    renderPanel();
    fireEvent.click(screen.getByRole('switch', { name: 'Show INFO backend logs' }));
    expect(screen.getByText('request started')).toBeInTheDocument();
  });

  it('renders level badge, target and fields', () => {
    addLog('ERROR', 'request failed');
    renderPanel();
    expect(screen.getByTestId('console-log-level')).toHaveTextContent('ERROR');
    expect(screen.getByText('rocket_http::client')).toBeInTheDocument();
    expect(screen.getByText('method=GET status=500')).toBeInTheDocument();
  });

  it('shows the empty state when only hidden INFO logs exist', () => {
    addLog('INFO', 'request started');
    renderPanel();
    expect(screen.getByText('No console activity yet')).toBeInTheDocument();
  });

  it('clear removes log entries', () => {
    addLog('WARN', 'slow upstream');
    renderPanel();
    fireEvent.click(screen.getByRole('button', { name: 'Clear console' }));
    expect(screen.queryByText('slow upstream')).toBeNull();
  });
});
