import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@/lib/tauri-api', () => ({
  updateContract: vi.fn(),
}));

vi.mock('@/lib/telemetry', () => ({
  track: vi.fn(),
}));

import type { Contract as IpcContract } from '@/lib/tauri-api';
import * as api from '@/lib/tauri-api';
import type { Contract, CreateContractFormValues } from '@/types/contracts';
import { useContractsStore } from './contractsSlice';

const existingContract: Contract = {
  id: 'c1',
  collectionId: 'col1',
  name: 'Payments API',
  version: '1.0.0',
  status: 'active',
  provider: { id: 'billing', name: 'Billing Team', kind: 'team' },
  consumers: [{ id: 'platform', name: 'Platform', kind: 'team' }],
  scope: { type: 'collection' },
  policy: { breakingChangePolicy: 'lenient', noticeDays: 30, uptimeSla: null },
  effectiveAt: '2026-01-01',
  expiresAt: null,
  signedSnapshot: null,
  driftCount: 0,
  breachCount: 0,
  endpointCount: 1,
  changelog: [],
  createdBy: 'user1',
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-01T00:00:00Z',
  // The attachment this whole test exists to protect.
  documentPaths: ['attachments/c1/spec.pdf'],
};

const ipcUpdateResponse: IpcContract = {
  id: 'c1',
  title: 'Payments API',
  provider: { id: 'billing', name: 'Billing Team', kind: 'team' },
  consumers: [{ id: 'platform', name: 'Platform', kind: 'team' }],
  project: '',
  version: '1.0.1',
  status: 'active',
  effectiveDate: '2026-01-01',
  expiryDate: null,
  documentPaths: ['attachments/c1/spec.pdf'],
  enforcementMode: 'informational',
  scope: { type: 'collection' },
  policy: { breakingChangePolicy: 'lenient', noticeDays: 30, uptimeSla: undefined },
  driftCount: 0,
  breachCount: 0,
  endpointCount: 1,
  createdBy: 'user1',
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-02T00:00:00Z',
};

const formValues: CreateContractFormValues = {
  name: 'Payments API',
  version: '1.0.1',
  provider: { id: 'billing', name: 'Billing Team', kind: 'team' },
  consumers: [{ id: 'platform', name: 'Platform', kind: 'team' }],
  scope: { type: 'collection' },
  policy: { breakingChangePolicy: 'lenient', noticeDays: 30, uptimeSla: null },
  effectiveAt: '2026-01-01',
  expiresAt: null,
  publishImmediately: false,
};

describe('updateContract', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useContractsStore.setState({
      byId: { c1: existingContract },
      byCollection: { col1: ['c1'] },
      hoveredId: null,
      loading: false,
      error: null,
    });
  });

  // Regression test: the backend deletes every attachment file not listed in
  // keptDocumentPaths on update (contract_service.rs). Before this fix,
  // updateContract always sent keptDocumentPaths: [], silently destroying
  // every existing attachment on every edit.
  it('carries the existing documentPaths forward as keptDocumentPaths', async () => {
    vi.mocked(api.updateContract).mockResolvedValue(ipcUpdateResponse);

    await useContractsStore.getState().updateContract('col1', 'c1', formValues);

    expect(api.updateContract).toHaveBeenCalledWith(
      'col1',
      expect.objectContaining({
        keptDocumentPaths: ['attachments/c1/spec.pdf'],
        newDocumentPaths: [],
      }),
    );
  });

  it('falls back to an empty list only if the contract is not already loaded', async () => {
    useContractsStore.setState({ byId: {}, byCollection: {} });
    vi.mocked(api.updateContract).mockResolvedValue(ipcUpdateResponse);

    await useContractsStore.getState().updateContract('col1', 'unknown-id', formValues);

    expect(api.updateContract).toHaveBeenCalledWith(
      'col1',
      expect.objectContaining({ keptDocumentPaths: [] }),
    );
  });
});
