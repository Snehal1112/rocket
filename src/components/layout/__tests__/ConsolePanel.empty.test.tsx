import { render, screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { useConsoleStore } from '@/stores/console-store';
import { ConsolePanel } from '../ConsolePanel';

const renderPanel = () =>
  render(<ConsolePanel isOpen height={200} onHeightChange={() => undefined} />);

describe('ConsolePanel empty state', () => {
  beforeEach(() => useConsoleStore.getState().clearEntries());

  it('names what the console collects', () => {
    renderPanel();
    const sources = screen.getByRole('list', { name: 'What the Console shows' });
    const labels = within(sources)
      .getAllByRole('listitem')
      .map((item) => item.textContent);
    expect(labels).toEqual(['Requests', 'Script output', 'Test results', 'Backend warnings']);
  });

  it('disables Clear while there is nothing to clear', () => {
    renderPanel();
    expect(screen.getByRole('button', { name: 'Clear console' })).toBeDisabled();
  });

  it('enables Clear when only hidden INFO logs exist', () => {
    useConsoleStore.getState().addLogEntry({
      timestamp: '2026-10-02T10:00:00Z',
      level: 'INFO',
      target: 'rocket_http::client',
      message: 'request started',
      fields: {},
      spanFields: {},
    });
    renderPanel();
    expect(screen.getByRole('button', { name: 'Clear console' })).toBeEnabled();
  });
});
