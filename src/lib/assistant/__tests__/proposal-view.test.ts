import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as api from '@/lib/tauri-api';
import { makeProposal, makeRequest } from '@/test/assistant-fixtures';
import {
  loadProposalDiff,
  proposalFailure,
  proposalPreview,
  proposalTarget,
} from '../proposal-view';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  getRequest: vi.fn(),
}));

describe('proposal-view', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('diffs a script edit against the stored script of that phase', async () => {
    vi.mocked(api.getRequest).mockResolvedValue(
      makeRequest({ postResponseScript: 'old();', tests: 'other();' }),
    );
    const proposal = makeProposal({
      change: {
        op: 'editScript',
        collection: 'orders',
        requestPath: 'get.yml',
        phase: 'postResponse',
        body: 'new();',
      },
    });
    expect(await loadProposalDiff(proposal)).toEqual({
      before: 'old();',
      after: 'new();',
      language: 'javascript',
    });
    expect(api.getRequest).toHaveBeenCalledWith('orders', 'get.yml');
  });

  it('diffs only the patched fields of a request update', async () => {
    vi.mocked(api.getRequest).mockResolvedValue(
      makeRequest({ url: 'https://old.test', name: 'Same' }),
    );
    const proposal = makeProposal({
      change: {
        op: 'updateRequest',
        collection: 'orders',
        requestPath: 'get.yml',
        patch: { url: 'https://new.test' },
      },
    });
    const diff = await loadProposalDiff(proposal);
    expect(diff?.language).toBe('json');
    expect(JSON.parse(diff?.before ?? '{}')).toEqual({ url: 'https://old.test' });
    expect(JSON.parse(diff?.after ?? '{}')).toEqual({ url: 'https://new.test' });
  });

  it('has no diff for a create', async () => {
    const proposal = makeProposal({
      change: { op: 'createFolder', collection: 'orders', parentPath: '', name: 'admin' },
    });
    expect(await loadProposalDiff(proposal)).toBeNull();
    expect(api.getRequest).not.toHaveBeenCalled();
  });

  it('describes creates with the new definition', () => {
    const preview = proposalPreview({
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
    });
    expect(preview.kind).toBe('definition');
    expect(preview.kind === 'definition' && preview.text).toContain('"name": "List orders"');
  });

  it('describes moves, renames, folders and environment variables in one line', () => {
    expect(
      proposalPreview({
        op: 'moveItem',
        collection: 'orders',
        fromPath: 'get.yml',
        toFolder: 'admin',
      }),
    ).toEqual({ kind: 'line', text: 'Move get.yml to admin' });
    expect(
      proposalPreview({
        op: 'renameItem',
        collection: 'orders',
        path: 'get.yml',
        newName: 'Get one',
      }),
    ).toEqual({ kind: 'line', text: 'Rename get.yml to Get one' });
    expect(
      proposalPreview({ op: 'createFolder', collection: 'orders', parentPath: 'a', name: 'b' }),
    ).toEqual({ kind: 'line', text: 'New folder a/b' });
    expect(
      proposalPreview({
        op: 'setEnvVar',
        collection: 'orders',
        environment: 'dev',
        key: 'BASE_URL',
        value: 'https://dev.test',
      }),
    ).toEqual({ kind: 'line', text: 'Set BASE_URL = https://dev.test in environment dev' });
  });

  it('names the target of each change', () => {
    expect(proposalTarget(makeProposal().change)).toEqual({
      collection: 'orders',
      path: 'get.yml',
    });
    expect(
      proposalTarget({
        op: 'setEnvVar',
        collection: 'orders',
        environment: 'dev',
        key: 'K',
        value: 'v',
      }),
    ).toEqual({ collection: 'orders' });
  });

  it('reads the failure message', () => {
    expect(
      proposalFailure(makeProposal({ status: 'failed', statusMessage: 'name taken' })),
    ).toBe('name taken');
    expect(proposalFailure(makeProposal())).toBeUndefined();
  });
});
