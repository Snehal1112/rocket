import { describe, it, expect } from 'vitest';
import { computeSnapshot } from './snapshot';
import type { ContractScope } from '@/types/contracts';

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

  it('folder scope returns only matching folder requests', () => {
    const scope: ContractScope = { type: 'folder', folderId: 'folder-a', path: 'payments/' };
    const snap = computeSnapshot(mockRequests, scope);
    expect(Object.keys(snap)).toHaveLength(2);
    expect(snap['r1']).toBeDefined();
    expect(snap['r2']).toBeDefined();
    expect(snap['r3']).toBeUndefined();
  });

  it('requests scope returns only specified requestIds', () => {
    const scope: ContractScope = { type: 'requests', requestIds: ['r1', 'r3'] };
    const snap = computeSnapshot(mockRequests, scope);
    expect(Object.keys(snap)).toHaveLength(2);
    expect(snap['r1']).toBeDefined();
    expect(snap['r3']).toBeDefined();
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
