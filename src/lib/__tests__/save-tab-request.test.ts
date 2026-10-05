import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@/lib/tauri-api', () => ({
  saveRequest: vi.fn().mockResolvedValue({ fileName: 'x.yml' }),
  saveGraphQlRequest: vi.fn().mockResolvedValue({ fileName: 'x.yml' }),
}));

import { saveGraphQlRequest, saveRequest } from '@/lib/tauri-api';
import type { RequestTab } from '@/types/pane-types';
import { createDefaultRequestFor } from '../pane-utils';
import { buildGraphQlSavePayload } from '../request-save-mapper';
import { saveTabRequest } from '../save-tab-request';

function gqlTab(): RequestTab {
  const request = createDefaultRequestFor('graphql');
  request.url = 'https://api.example.com/graphql';
  request.graphql = {
    query: '{ users { id } }',
    variables: '',
    bodyVariants: [
      { title: 'A', selected: true, body: { query: '{ users { id } }' } },
      { title: 'B', selected: false, body: { query: '{ b }' } },
    ],
  };
  return {
    id: 'tab-1',
    title: 'Users',
    tabType: 'request',
    request,
    response: null,
    isDirty: true,
  };
}

describe('saveTabRequest', () => {
  beforeEach(() => vi.clearAllMocks());

  it('routes a graphql tab to saveGraphQlRequest', async () => {
    await saveTabRequest('api', 'users.yml', gqlTab());
    expect(saveGraphQlRequest).toHaveBeenCalledTimes(1);
    expect(saveRequest).not.toHaveBeenCalled();
  });

  it('routes an http tab to saveRequest', async () => {
    const tab = gqlTab();
    tab.request = { ...tab.request, requestType: 'http', graphql: undefined };
    await saveTabRequest('api', 'users.yml', tab);
    expect(saveRequest).toHaveBeenCalledTimes(1);
    expect(saveGraphQlRequest).not.toHaveBeenCalled();
  });
});

describe('buildGraphQlSavePayload', () => {
  it('sends the query, keeps every variant and omits blank variables', () => {
    const payload = buildGraphQlSavePayload(gqlTab());
    expect(payload.uid).toBe('tab-1');
    expect(payload.method).toBe('POST');
    expect(payload.body.query).toBe('{ users { id } }');
    expect(payload.body.variables).toBeUndefined();
    expect(payload.bodyVariants).toHaveLength(2);
  });

  it('applies the save-to-collection overrides', () => {
    const payload = buildGraphQlSavePayload(gqlTab(), { name: 'Chosen', fileName: 'chosen.yml' });
    expect(payload.name).toBe('Chosen');
    expect(payload.fileName).toBe('chosen.yml');
  });
});
