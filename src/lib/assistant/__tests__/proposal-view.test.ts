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
  getFolderSettings: vi.fn(),
}));

describe('proposal-view', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows a docs change apart from the JSON diff of the other fields', async () => {
    vi.mocked(api.getRequest).mockResolvedValue(
      makeRequest({ url: 'https://api.test/orders/1', docs: '# Old\nline' }),
    );
    const proposal = makeProposal({
      change: {
        op: 'updateRequest',
        collection: 'orders',
        requestPath: 'get.yml',
        patch: { url: 'https://api.test/orders/2', docs: '# New\nline' },
      },
    });
    const diff = await loadProposalDiff(proposal);
    expect(diff?.docs).toEqual({ before: '# Old\nline', after: '# New\nline' });
    expect(JSON.parse(diff?.before ?? '{}')).toEqual({ url: 'https://api.test/orders/1' });
    expect(JSON.parse(diff?.after ?? '{}')).toEqual({ url: 'https://api.test/orders/2' });
  });

  it('gives a docs-only update an empty JSON part and the docs diff', async () => {
    vi.mocked(api.getRequest).mockResolvedValue(makeRequest({ docs: null }));
    const proposal = makeProposal({
      change: {
        op: 'updateRequest',
        collection: 'orders',
        requestPath: 'get.yml',
        patch: { docs: '# Orders\nLists orders.' },
      },
    });
    const diff = await loadProposalDiff(proposal);
    expect(diff?.docs).toEqual({ before: '', after: '# Orders\nLists orders.' });
    expect(diff?.before).toBe(diff?.after);
    expect(JSON.parse(diff?.before ?? 'x')).toEqual({});
  });

  it('shows folder docs apart from the other folder settings', async () => {
    vi.mocked(api.getFolderSettings).mockResolvedValue({
      headers: [],
      variables: [],
      docs: 'old folder docs',
    } as unknown as Awaited<ReturnType<typeof api.getFolderSettings>>);
    const proposal = makeProposal({
      change: {
        op: 'updateFolderSettings',
        collection: 'orders',
        folderPath: 'v2',
        patch: { docs: 'new folder docs' },
      },
    });
    const diff = await loadProposalDiff(proposal);
    expect(diff?.docs).toEqual({ before: 'old folder docs', after: 'new folder docs' });
  });

  it('leaves a diff without a docs change unchanged', async () => {
    vi.mocked(api.getRequest).mockResolvedValue(makeRequest({ url: 'https://old.test' }));
    const proposal = makeProposal({
      change: {
        op: 'updateRequest',
        collection: 'orders',
        requestPath: 'get.yml',
        patch: { url: 'https://new.test' },
      },
    });
    const diff = await loadProposalDiff(proposal);
    expect(diff?.docs).toBeUndefined();
  });

  it('previews the docs of a new request as Markdown and keeps them out of the JSON', () => {
    const proposal = makeProposal({
      change: {
        op: 'createRequest',
        collection: 'orders',
        folderPath: '',
        request: {
          name: 'List orders',
          method: 'GET',
          url: 'https://api.test/orders',
          headers: [],
          queryParams: [],
          docs: '# Orders\nLists orders.',
        },
      },
    });
    const preview = proposalPreview(proposal.change);
    expect(preview.kind).toBe('definition');
    if (preview.kind !== 'definition') return;
    expect(preview.docs).toBe('# Orders\nLists orders.');
    expect(preview.text).not.toContain('Lists orders');
    expect(JSON.parse(preview.text)).toMatchObject({ name: 'List orders' });
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

  describe('folder settings update', () => {
    const change = {
      op: 'updateFolderSettings' as const,
      collection: 'orders',
      folderPath: 'auth/v2',
      patch: {
        preRequestScript: 'new();',
        variables: [{ key: 'HOST', value: 'api.test', enabled: true }],
      },
    };

    it('targets the folder and previews a diff', () => {
      expect(proposalTarget(change)).toEqual({ collection: 'orders', path: 'auth/v2' });
      expect(proposalPreview(change)).toEqual({ kind: 'diff' });
    });

    it('diffs only the changed sections against the stored folder', async () => {
      vi.mocked(api.getFolderSettings).mockResolvedValue({
        headers: [{ key: 'X-Old', value: 'old', enabled: true }],
        variables: [
          { key: 'HOST', value: 'old.test', initialValue: '', enabled: true, secret: false },
        ],
        preRequestScript: 'old();',
        docs: 'untouched',
      });
      const diff = await loadProposalDiff(makeProposal({ change }));
      expect(api.getFolderSettings).toHaveBeenCalledWith('orders', 'auth/v2');
      expect(diff?.language).toBe('json');
      const before = JSON.parse(diff?.before ?? '{}');
      const after = JSON.parse(diff?.after ?? '{}');
      expect(Object.keys(after).sort()).toEqual(['preRequestScript', 'variables']);
      expect(before.preRequestScript).toBe('old();');
      expect(before.variables).toEqual([{ key: 'HOST', value: 'old.test', enabled: true }]);
      expect(after.variables).toEqual([{ key: 'HOST', value: 'api.test', enabled: true }]);
      expect(diff?.before).not.toContain('untouched');
    });
  });
});
