import { describe, expect, it } from 'vitest';
import type { ContractScope } from '@/types/contracts';
import { computeSnapshot } from './snapshot';

const mockRequests = [
  {
    id: 'r1',
    method: 'GET',
    path: '/payments',
    params: [{ key: 'currency', required: true }],
    headers: [{ key: 'Authorization', required: true }],
    body: null,
    folderId: 'folder-a',
  },
  {
    id: 'r2',
    method: 'POST',
    path: '/payments',
    params: [],
    headers: [{ key: 'Content-Type', required: true }],
    body: { schema: '{"type":"object"}' },
    folderId: 'folder-a',
  },
  {
    id: 'r3',
    method: 'GET',
    path: '/users',
    params: [{ key: 'page', required: false }],
    headers: [],
    body: null,
    folderId: 'folder-b',
  },
];

describe('computeSnapshot', () => {
  it('collection scope returns all requests', () => {
    const scope: ContractScope = { type: 'collection' };
    const snap = computeSnapshot(mockRequests, scope);
    expect(Object.keys(snap)).toHaveLength(3);
    expect(snap['r1'].method).toBe('GET');
    expect(snap['r1'].path).toBe('/payments');
  });

  it('collection scope captures params and headers correctly', () => {
    const snap = computeSnapshot(mockRequests, { type: 'collection' });
    expect(snap['r1'].params).toEqual([{ key: 'currency', required: true }]);
    expect(snap['r1'].headers).toEqual([{ key: 'Authorization', required: true }]);
  });

  it('folder scope returns only requests in that folder (rel_path matches folderId)', () => {
    const scope: ContractScope = { type: 'folder', rel_path: 'folder-a' };
    const snap = computeSnapshot(mockRequests, scope);
    expect(Object.keys(snap)).toHaveLength(2);
    expect(snap['r1']).toBeDefined();
    expect(snap['r2']).toBeDefined();
    expect(snap['r3']).toBeUndefined();
  });

  it('request scope returns only the single request matching rel_path', () => {
    const scope: ContractScope = { type: 'request', rel_path: 'r1' };
    const snap = computeSnapshot(mockRequests, scope);
    expect(Object.keys(snap)).toHaveLength(1);
    expect(snap['r1']).toBeDefined();
    expect(snap['r2']).toBeUndefined();
  });

  it('captures bodySchema when body has schema', () => {
    const snap = computeSnapshot(mockRequests, { type: 'collection' });
    expect(snap['r2'].bodySchema).toBe('{"type":"object"}');
    expect(snap['r1'].bodySchema).toBeUndefined();
  });

  it('empty requests returns empty map', () => {
    const snap = computeSnapshot([], { type: 'collection' });
    expect(Object.keys(snap)).toHaveLength(0);
  });
});
