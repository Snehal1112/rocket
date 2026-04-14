import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@/lib/tauri-api', () => ({
  listAuditEvents: vi.fn(),
  getComplianceProfile: vi.fn(),
  setComplianceProfile: vi.fn(),
  exportAuditEvidence: vi.fn(),
}));

import type { Framework } from '@/lib/tauri-api';
import * as api from '@/lib/tauri-api';
import { useAuditStore } from '@/stores/audit-store';

const mockedApi = vi.mocked(api);

describe('audit-store', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useAuditStore.setState({
      events: [],
      profile: null,
      loading: false,
      error: null,
    });
  });

  it('loadEvents populates events on success', async () => {
    mockedApi.listAuditEvents.mockResolvedValue([
      {
        id: '01',
        occurredAt: '2026-04-15T00:00:00Z',
        actor: 'me',
        workspaceId: null,
        event: { kind: 'collection_deleted', collection: 'x' },
        controls: [],
        prevHash: '',
        hash: 'h1',
      },
    ]);
    await useAuditStore.getState().loadEvents();
    expect(useAuditStore.getState().events).toHaveLength(1);
    expect(useAuditStore.getState().error).toBeNull();
  });

  it('loadEvents sets error on failure', async () => {
    mockedApi.listAuditEvents.mockRejectedValue(new Error('boom'));
    await useAuditStore.getState().loadEvents();
    expect(useAuditStore.getState().error).toBe('boom');
  });

  it('saveProfile persists and updates state', async () => {
    const profile = {
      activeFrameworks: ['soc2'] as Framework[],
      enforcement: 'record' as const,
      mutedKinds: [],
    };
    mockedApi.setComplianceProfile.mockResolvedValue(undefined);
    await useAuditStore.getState().saveProfile(profile);
    expect(mockedApi.setComplianceProfile).toHaveBeenCalledWith(profile);
    expect(useAuditStore.getState().profile).toEqual(profile);
  });
});
