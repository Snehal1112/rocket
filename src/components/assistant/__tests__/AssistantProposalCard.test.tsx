import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createDefaultRequest } from '@/lib/pane-utils';
import * as api from '@/lib/tauri-api';
import type { AgentProposal } from '@/lib/tauri-api';
import { useAssistantStore } from '@/stores/assistant-store';
import { usePaneStore } from '@/stores/pane-store';
import { makeProposal, makeRequest } from '@/test/assistant-fixtures';
import { createDeferred } from '@/test/deferred';
import { AssistantProposalCard } from '../AssistantProposalCard';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  acceptAgentProposal: vi.fn(),
  rejectAgentProposal: vi.fn(),
  getRequest: vi.fn(),
}));

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn(), cancelAutoSave: vi.fn() }));

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

  it('does not fetch or mount the diff until expanded, and drops it on collapse', async () => {
    showProposal(makeProposal());
    expect(screen.getByRole('button', { name: 'Show changes' })).toHaveAttribute(
      'aria-expanded',
      'false',
    );
    expect(api.getRequest).not.toHaveBeenCalled();
    expect(screen.queryByTestId('proposal-diff')).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Show changes' }));
    expect(await screen.findByTestId('proposal-diff')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Hide changes' }));
    expect(screen.queryByTestId('proposal-diff')).not.toBeInTheDocument();
  });

  it('says the diff is gone for a resolved proposal instead of a skeleton', async () => {
    showProposal(makeProposal({ status: 'stale' }));
    await userEvent.click(screen.getByRole('button', { name: 'Show changes' }));
    expect(screen.getByText('Diff no longer available.')).toBeInTheDocument();
    expect(api.getRequest).not.toHaveBeenCalled();
  });

  it('offers a retry when the diff fails to load', async () => {
    vi.mocked(api.getRequest).mockRejectedValueOnce('boom internal');
    showProposal(makeProposal());
    await userEvent.click(screen.getByRole('button', { name: 'Show changes' }));
    expect(await screen.findByText('Could not load the current version.')).toBeInTheDocument();
    expect(screen.queryByText(/boom internal/)).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(await screen.findByTestId('proposal-diff')).toBeInTheDocument();
  });

  it('shows a Monaco diff of the script edit', async () => {
    showProposal(makeProposal());
    await userEvent.click(screen.getByRole('button', { name: 'Show changes' }));
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

  it('stops loading when the proposal is resolved mid-load', async () => {
    const pendingLoad = createDeferred<ReturnType<typeof makeRequest>>();
    vi.mocked(api.getRequest).mockReturnValue(pendingLoad.promise);
    const proposal = makeProposal();
    showProposal(proposal);
    await userEvent.click(screen.getByRole('button', { name: 'Show changes' }));
    act(() => {
      useAssistantStore.getState().upsertProposal({ ...proposal, status: 'rejected' });
    });
    expect(await screen.findByText('Diff no longer available.')).toBeInTheDocument();
  });

  it('names the collection when the unsaved edits are in a parked tab', () => {
    usePaneStore.setState({
      collectionTabState: {
        billing: {
          tabs: [
            {
              id: 'tab-1',
              title: 'get.yml',
              tabType: 'request',
              request: createDefaultRequest(),
              response: null,
              isDirty: true,
              source: { collection: 'orders', path: 'get.yml' },
            },
          ],
          activeTabId: 'tab-1',
        },
      },
    });
    showProposal(makeProposal());
    expect(screen.getByText(/parked tab of collection billing/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Accept' })).toBeDisabled();
  });

  it('keeps a triple backtick in a new request from breaking the preview', () => {
    showProposal(
      makeProposal({
        change: {
          op: 'createRequest',
          collection: 'orders',
          folderPath: '',
          request: {
            name: 'a ``` b',
            method: 'GET',
            url: 'https://api.test',
            headers: [],
            queryParams: [],
          },
        },
      }),
    );
    expect(screen.getByText(/a ``` b/).tagName).toBe('PRE');
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
    expect(await screen.findByText('The action failed. Try again.')).toBeInTheDocument();
    expect(screen.queryByText('session ended')).not.toBeInTheDocument();
  });
});
