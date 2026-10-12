import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';
import { ReasoningBlock } from '@/components/assistant/ReasoningBlock';
import { useAssistantStore } from '@/stores/assistant-store';

describe('ReasoningBlock', () => {
  beforeEach(() => {
    useAssistantStore.getState().reset();
  });

  it('renders nothing when there is no reasoning', () => {
    const { container } = render(<ReasoningBlock />);
    expect(container).toBeEmptyDOMElement();
  });

  it('is collapsed by default and shows the reasoning when opened', async () => {
    useAssistantStore.setState({ reasoning: 'The token variable is empty.' });
    render(<ReasoningBlock />);
    expect(screen.queryByText('The token variable is empty.')).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: /reasoning/i }));
    expect(screen.getByText('The token variable is empty.')).toBeInTheDocument();
  });
});
