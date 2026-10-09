import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  acceptAgentProposal,
  type AgentProposal,
  listAgentProposals,
  onAgentProposalCreated,
  onAgentProposalResolved,
  rejectAgentProposal,
} from '../tauri-api';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

const proposal: AgentProposal = {
  id: 'p1',
  sessionId: 's1',
  change: {
    op: 'editScript',
    collection: 'demo',
    requestPath: 'get-users.yml',
    phase: 'tests',
    body: "rok.test('ok', () => {});",
  },
  summary: "Edit the tests script of 'get-users.yml' in demo",
  status: 'pending',
  createdAtMs: 1,
};

describe('agent proposal commands', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(listen).mockReset();
  });

  it('lists proposals by session id', async () => {
    vi.mocked(invoke).mockResolvedValue([proposal]);
    await expect(listAgentProposals('s1')).resolves.toEqual([proposal]);
    expect(invoke).toHaveBeenCalledWith('list_agent_proposals', { sessionId: 's1' });
  });

  it('accepts and rejects by session id and proposal id', async () => {
    vi.mocked(invoke).mockResolvedValue({ ...proposal, status: 'accepted' });
    await acceptAgentProposal('s1', 'p1');
    expect(invoke).toHaveBeenCalledWith('accept_agent_proposal', {
      sessionId: 's1',
      proposalId: 'p1',
    });
    await rejectAgentProposal('s1', 'p1');
    expect(invoke).toHaveBeenCalledWith('reject_agent_proposal', {
      sessionId: 's1',
      proposalId: 'p1',
    });
  });

  it('listens on the proposal channels and passes the payload through', async () => {
    vi.mocked(listen).mockResolvedValue(vi.fn());
    const created = vi.fn();
    await onAgentProposalCreated(created);
    expect(listen).toHaveBeenCalledWith('agent-proposal-created', expect.any(Function));
    const payload = {
      type: 'acpProposalCreated' as const,
      session_id: 's1',
      proposal_id: 'p1',
      summary: 'Create folder',
    };
    const handler = vi.mocked(listen).mock.calls[0][1];
    handler({ event: 'agent-proposal-created', id: 1, payload });
    expect(created).toHaveBeenCalledWith(payload);

    await onAgentProposalResolved(vi.fn());
    expect(listen).toHaveBeenCalledWith('agent-proposal-resolved', expect.any(Function));
  });
});
