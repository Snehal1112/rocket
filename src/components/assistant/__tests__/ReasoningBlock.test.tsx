import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';
import { ReasoningBlock } from '@/components/assistant/ReasoningBlock';
import { useAssistantStore } from '@/stores/assistant-store';

function startTurn() {
  const store = useAssistantStore.getState();
  const token = store.beginSession('agent-1', 'edit');
  store.activateSession(token, 's1', []);
  store.appendUserMessage('why does login fail?');
}

describe('ReasoningBlock', () => {
  beforeEach(() => {
    useAssistantStore.getState().reset();
  });

  it('renders nothing when there is no thinking text', () => {
    startTurn();
    const { container } = render(<ReasoningBlock />);
    expect(container).toBeEmptyDOMElement();
  });

  it('is collapsed by default and shows the thinking when opened', async () => {
    startTurn();
    useAssistantStore.setState({ reasoning: 'The token variable is empty.' });
    render(<ReasoningBlock />);
    expect(screen.queryByText('The token variable is empty.')).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: /agent's thinking/i }));
    expect(screen.getByText('The token variable is empty.')).toBeInTheDocument();
  });

  it('is hidden once the turn is done', () => {
    startTurn();
    useAssistantStore.setState({ reasoning: 'The token variable is empty.' });
    useAssistantStore.getState().completeMessage('s1');
    const { container } = render(<ReasoningBlock />);
    expect(container).toBeEmptyDOMElement();
  });
});
