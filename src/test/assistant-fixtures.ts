import type { AgentProposal, Request } from '@/lib/tauri-api';

// Shared fixtures for the AI Assistant tests.
export function makeProposal(overrides: Partial<AgentProposal> = {}): AgentProposal {
  return {
    id: 'p1',
    sessionId: 's1',
    summary: 'Add a status test',
    status: 'pending',
    createdAtMs: 1,
    change: {
      op: 'editScript',
      collection: 'orders',
      requestPath: 'get.yml',
      phase: 'tests',
      body: "rok.test('status', () => {});",
    },
    ...overrides,
  };
}

export function makeRequest(overrides: Partial<Request> = {}): Request {
  return {
    uid: 'u1',
    name: 'Get order',
    method: 'GET',
    url: 'https://api.test/orders/1',
    headers: [],
    auth: { authType: 'none' },
    ...overrides,
  };
}
