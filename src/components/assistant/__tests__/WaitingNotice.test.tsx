import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { WaitingNotice } from '@/components/assistant/WaitingNotice';
import { useAssistantStore } from '@/stores/assistant-store';

function startTurn() {
  const store = useAssistantStore.getState();
  const token = store.beginSession('agent-1', 'edit');
  store.activateSession(token, 's1', []);
  store.appendUserMessage('why does login fail?');
}

describe('WaitingNotice', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    useAssistantStore.getState().reset();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('stays hidden while the agent is active', () => {
    startTurn();
    render(<WaitingNotice />);
    act(() => {
      vi.advanceTimersByTime(10_000);
    });
    expect(screen.queryByText(/still working/i)).not.toBeInTheDocument();
  });

  it('appears after 20 seconds without any update and says how long', () => {
    startTurn();
    render(<WaitingNotice />);
    act(() => {
      vi.advanceTimersByTime(25_000);
    });
    expect(screen.getByText(/still working/i)).toHaveTextContent('25');
  });

  it('is hidden when no turn is running', () => {
    render(<WaitingNotice />);
    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(screen.queryByText(/still working/i)).not.toBeInTheDocument();
  });
});
