import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createDefaultRequest } from '@/lib/pane-utils';
import * as api from '@/lib/tauri-api';
import type { AgentProposal } from '@/lib/tauri-api';
import { useAssistantStore } from '@/stores/assistant-store';
import { usePaneStore } from '@/stores/pane-store';
import { makeProposal, makeRequest } from '@/test/assistant-fixtures';
import { AssistantProposalCard } from '../AssistantProposalCard';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  acceptAgentProposal: vi.fn(),
  rejectAgentProposal: vi.fn(),
  getRequest: vi.fn(),
}));

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));

vi.mock('../ProposalDiffEditor', () => ({
  ProposalDiffEditor: ({
    original,
    modified,
    language,
  }: {
    original: string;
    modified: string;
    language: string;
  }) => (
    <div
      data-testid='proposal-diff'
      data-original={original}
      data-modified={modified}
      data-language={language}
    />
  ),
}));

vi.mock('@/components/collections/MarkdownRenderer', () => ({
  MarkdownRenderer: ({ children }: { children: string }) => <pre>{children}</pre>,
}));

// Renders the card from the store, the way the chat view does.
function StoreCard() {
  const proposal = useAssistantStore((s) => s.proposals[0]);
  return proposal ? <AssistantProposalCard proposal={proposal} /> : null;
}

function showProposal(proposal: AgentProposal): void {
  useAssistantStore.getState().upsertProposal(proposal);
  render(<StoreCard />);
}

describe('AssistantProposalCard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    useAssistantStore.getState().reset();
    const token = useAssistantStore.getState().beginSession('agent-1', 'edit');
    useAssistantStore.getState().activateSession(token, 's1', []);
    vi.mocked(api.getRequest).mockResolvedValue(makeRequest({ tests: 'old();' }));
  });

  it('shows a Monaco diff of the script edit', async () => {
    showProposal(makeProposal());
    const diff = await screen.findByTestId('proposal-diff');
    expect(diff.dataset.original).toBe('old();');
    expect(diff.dataset.modified).toBe("rok.test('status', () => {});");
    expect(diff.dataset.language).toBe('javascript');
    expect(screen.getByText('orders / get.yml')).toBeInTheDocument();
  });

  it('accepts and shows the accepted state', async () => {
    const proposal = makeProposal();
    vi.mocked(api.acceptAgentProposal).mockResolvedValue({ ...proposal, status: 'accepted' });
    showProposal(proposal);
    await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
    expect(api.acceptAgentProposal).toHaveBeenCalledWith('s1', 'p1');
    expect(await screen.findByText('Accepted')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Accept' })).not.toBeInTheDocument();
  });

  it('rejects', async () => {
    const proposal = makeProposal();
    vi.mocked(api.rejectAgentProposal).mockResolvedValue({ ...proposal, status: 'rejected' });
    showProposal(proposal);
    await userEvent.click(screen.getByRole('button', { name: 'Reject' }));
    expect(api.rejectAgentProposal).toHaveBeenCalledWith('s1', 'p1');
    expect(await screen.findByText('Rejected')).toBeInTheDocument();
  });

  it('explains a stale proposal', () => {
    showProposal(makeProposal({ status: 'stale' }));
    expect(screen.getByText(/changed after the proposal was made/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Accept' })).not.toBeInTheDocument();
  });

  it('explains a failed proposal with its message', () => {
    showProposal(makeProposal({ status: 'failed', statusMessage: 'name taken' }));
    expect(screen.getByText('Could not apply this change: name taken')).toBeInTheDocument();
  });

  it('shows the definition of a new request', () => {
    showProposal(
      makeProposal({
        change: {
          op: 'createRequest',
          collection: 'orders',
          folderPath: 'admin',
          request: {
            name: 'List orders',
            method: 'GET',
            url: 'https://api.test/orders',
            headers: [],
            queryParams: [],
          },
        },
      }),
    );
    expect(screen.getByText(/"name": "List orders"/)).toBeInTheDocument();
    expect(screen.queryByTestId('proposal-diff')).not.toBeInTheDocument();
  });

  it('disables Accept while the request has unsaved edits in an open tab', () => {
    usePaneStore.getState().openTab({
      id: 'tab-1',
      title: 'get.yml',
      tabType: 'request',
      request: createDefaultRequest(),
      response: null,
      isDirty: true,
      source: { collection: 'orders', path: 'get.yml' },
    });
    showProposal(makeProposal());
    expect(screen.getByRole('button', { name: 'Accept' })).toBeDisabled();
    expect(screen.getByText(/unsaved edits in an open tab/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Reject' })).toBeEnabled();
  });

  it('shows the error when Accept fails', async () => {
    vi.mocked(api.acceptAgentProposal).mockRejectedValue('session ended');
    showProposal(makeProposal());
    await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
    expect(await screen.findByText('session ended')).toBeInTheDocument();
  });
});
