import { describe, expect, it } from 'vitest';
import type { RequestTab } from '@/types/pane-types';
import { createDefaultRequest } from '../pane-utils';
import { buildRequestSavePayload } from '../request-save-mapper';

function makeTab(overrides: Partial<RequestTab['request']> = {}): RequestTab {
  return {
    id: 'tab-1',
    title: 'My Request',
    tabType: 'request',
    request: { ...createDefaultRequest(), ...overrides },
    response: null,
    isDirty: false,
  };
}

describe('buildRequestSavePayload', () => {
  it('uses tab.title and omits fileName when no overrides are given (normal save)', () => {
    const payload = buildRequestSavePayload(makeTab());
    expect(payload.name).toBe('My Request');
    expect('fileName' in payload).toBe(false);
  });

  it('applies name and fileName overrides (save-to-collection)', () => {
    const payload = buildRequestSavePayload(makeTab(), { name: 'Chosen Name', fileName: 'x.yml' });
    expect(payload.name).toBe('Chosen Name');
    expect(payload.fileName).toBe('x.yml');
  });

  // Regression test: SaveToCollectionDialog used to hand-build a partial
  // payload that hardcoded auth to 'none' and omitted tags/settings/scripts/
  // assertions entirely — a user who configured any of these before their
  // first save would silently lose them. The shared mapper must carry all
  // of it through regardless of which caller (with or without overrides)
  // is saving.
  it('includes auth, tags, settings, scripts, and assertions — not just url/method/headers/body', () => {
    const tab = makeTab({
      auth: {
        authType: 'aws-sig-v4',
        awsSigV4: {
          accessKey: 'AKIA...',
          secretKey: 'secret',
          region: 'us-east-1',
          service: 'execute-api',
          sessionToken: '',
        },
      },
      tags: ['important'],
      preRequestScript: 'console.log("pre")',
      postResponseScript: 'console.log("post")',
      testsScript: 'expect(res.status).toBe(200)',
      assertions: [{ id: 'a1', type: 'status', operator: 'equals', expected: '200' } as never],
    });

    const payload = buildRequestSavePayload(tab, { name: 'New Request', fileName: 'new.yml' });

    expect(payload.auth).toMatchObject({ authType: 'aws-sig-v4', accessKey: 'AKIA...' });
    expect(payload.tags).toEqual(['important']);
    expect(payload.preRequestScript).toBe('console.log("pre")');
    expect(payload.postResponseScript).toBe('console.log("post")');
    expect(payload.tests).toBe('expect(res.status).toBe(200)');
    expect(payload.assertions).toHaveLength(1);
  });

  it('falls back to a generated uid when tab.id is empty', () => {
    const tab = makeTab();
    tab.id = '';
    const payload = buildRequestSavePayload(tab);
    expect(payload.uid).not.toBe('');
  });
});
