import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { useAssistantStore } from '@/stores/assistant-store';
import { AssistantToggleButton } from '../AssistantToggleButton';

const store = () => useAssistantStore.getState();

function activate(): void {
  const token = store().beginSession('agent-1', 'edit');
  store().activateSession(token, 's1', []);
}

beforeEach(() => {
  store().reset();
  useAssistantStore.setState({ panelOpen: false });
});

describe('AssistantToggleButton activity dot', () => {
  it('shows no dot when idle', () => {
    render(<AssistantToggleButton />);
    expect(screen.queryByTestId('assistant-activity-dot')).toBeNull();
    expect(screen.getByRole('button', { name: 'AI Assistant' })).toBeTruthy();
  });

  it('pulses and names the working agent while a turn runs', () => {
    activate();
    useAssistantStore.setState({
      messages: [{ kind: 'agent', id: 'a1', text: '', streaming: true }],
    });
    render(<AssistantToggleButton />);
    const dot = screen.getByTestId('assistant-activity-dot');
    expect(dot.className).toContain('motion-safe:animate-pulse');
    expect(screen.getByRole('button', { name: 'AI Assistant, agent working' })).toBeTruthy();
  });

  it('shows a still dot for an active session with the panel closed', () => {
    activate();
    render(<AssistantToggleButton />);
    const dot = screen.getByTestId('assistant-activity-dot');
    expect(dot.className).not.toContain('animate-pulse');
  });

  it('shows no dot when the panel is open and idle', () => {
    activate();
    useAssistantStore.setState({ panelOpen: true });
    render(<AssistantToggleButton />);
    expect(screen.queryByTestId('assistant-activity-dot')).toBeNull();
  });
});
