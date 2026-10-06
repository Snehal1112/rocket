import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { pickOperationKind, useSelectedOperationKind } from '../useSelectedOperationKind';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, listGraphQlOperations: vi.fn() };
});

describe('pickOperationKind', () => {
  const ops = [
    { name: 'A', kind: 'query' as const },
    { name: 'S', kind: 'subscription' as const },
  ];

  it('uses the named operation', () => {
    expect(pickOperationKind(ops, 'S')).toBe('subscription');
    expect(pickOperationKind(ops, 'A')).toBe('query');
    expect(pickOperationKind(ops, 'Nope')).toBeNull();
  });

  it('uses the only operation when none is named, and nothing when it is ambiguous', () => {
    expect(pickOperationKind([ops[1]], undefined)).toBe('subscription');
    expect(pickOperationKind(ops, undefined)).toBeNull();
    expect(pickOperationKind([], undefined)).toBeNull();
  });
});

describe('useSelectedOperationKind', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.listGraphQlOperations).mockReset();
  });

  it('asks the backend scanner and reports the kind', async () => {
    vi.mocked(tauriApi.listGraphQlOperations).mockResolvedValue([
      { name: null, kind: 'subscription' },
    ]);
    const { result } = renderHook(() => useSelectedOperationKind('subscription { a }', undefined));
    await waitFor(() => expect(result.current).toBe('subscription'));
    expect(tauriApi.listGraphQlOperations).toHaveBeenCalledWith('subscription { a }');
  });

  it('reports null for a blank document without calling the backend', async () => {
    const { result } = renderHook(() => useSelectedOperationKind('   ', undefined));
    expect(result.current).toBeNull();
    expect(tauriApi.listGraphQlOperations).not.toHaveBeenCalled();
  });

  it('reports null when the scan fails', async () => {
    vi.mocked(tauriApi.listGraphQlOperations).mockRejectedValue(new Error('boom'));
    const { result } = renderHook(() => useSelectedOperationKind('query { a }', undefined));
    await waitFor(() => expect(tauriApi.listGraphQlOperations).toHaveBeenCalled());
    expect(result.current).toBeNull();
  });
});
