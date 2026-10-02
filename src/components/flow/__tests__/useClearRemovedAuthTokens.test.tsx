import { renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { flowAuthKey } from '@/lib/flow-auth';
import type { FlowNode } from '@/lib/tauri-api';
import { useFlowAuthStore } from '@/stores/flow-auth-store';
import type { AuthState } from '@/types/pane-types';
import { useClearRemovedAuthTokens } from '../useClearRemovedAuthTokens';

const authNode = (id: string): FlowNode =>
  ({
    id,
    position: { x: 0, y: 0 },
    kind: {
      kind: 'Auth',
      label: id,
      auth: { authType: 'bearer', token: '' },
      applyToInherit: false,
    },
  }) as unknown as FlowNode;

const keyA = flowAuthKey('c', 'f', 'a1', 'dev', 'g');
const keyB = flowAuthKey('c', 'f', 'a2', 'dev', 'g');

describe('useClearRemovedAuthTokens', () => {
  beforeEach(() => {
    const auth = { authType: 'bearer' } as AuthState;
    useFlowAuthStore.setState({ auths: { [keyA]: { auth }, [keyB]: { auth } } });
  });

  it('clears only the Auth node that left the list', () => {
    const both = [authNode('a1'), authNode('a2')];
    const { rerender } = renderHook(({ nodes }) => useClearRemovedAuthTokens('c', 'f', nodes), {
      initialProps: { nodes: both },
    });
    expect(Object.keys(useFlowAuthStore.getState().auths)).toHaveLength(2);
    rerender({ nodes: [authNode('a2')] });
    expect(Object.keys(useFlowAuthStore.getState().auths)).toEqual([keyB]);
  });

  const entries = () => Object.keys(useFlowAuthStore.getState().auths);
  const other = (id: string): FlowNode =>
    ({ id, position: { x: 0, y: 0 }, kind: { kind: 'Input', label: id } }) as unknown as FlowNode;

  it('keeps the token when a node is replaced by one with the same id', () => {
    const { rerender } = renderHook(({ nodes }) => useClearRemovedAuthTokens('c', 'f', nodes), {
      initialProps: { nodes: [authNode('a1'), authNode('a2')] },
    });
    rerender({ nodes: [{ ...authNode('a1') }, authNode('a2')] });
    expect(entries()).toHaveLength(2);
  });

  it('clears nothing when a non-Auth node leaves', () => {
    const { rerender } = renderHook(({ nodes }) => useClearRemovedAuthTokens('c', 'f', nodes), {
      initialProps: { nodes: [authNode('a1'), authNode('a2'), other('i1')] },
    });
    rerender({ nodes: [authNode('a1'), authNode('a2')] });
    expect(entries()).toHaveLength(2);
  });

  it('clears the token when an Auth node becomes another kind with the same id', () => {
    const { rerender } = renderHook(({ nodes }) => useClearRemovedAuthTokens('c', 'f', nodes), {
      initialProps: { nodes: [authNode('a1'), authNode('a2')] },
    });
    rerender({ nodes: [other('a1'), authNode('a2')] });
    expect(entries()).toEqual([keyB]);
  });

  it('does not clear when the flow or collection changes', () => {
    const { rerender } = renderHook(({ c, f, nodes }) => useClearRemovedAuthTokens(c, f, nodes), {
      initialProps: { c: 'c', f: 'f', nodes: [authNode('a1'), authNode('a2')] },
    });
    rerender({ c: 'c', f: 'g', nodes: [] });
    rerender({ c: 'x', f: 'g', nodes: [] });
    expect(entries()).toHaveLength(2);
  });

  it('is safe without a collection or flow', () => {
    const { rerender } = renderHook(
      ({ c, nodes }: { c: string | null | undefined; nodes: FlowNode[] }) =>
        useClearRemovedAuthTokens(c, 'f', nodes),
      { initialProps: { c: null as string | null | undefined, nodes: [authNode('a1')] } },
    );
    rerender({ c: undefined, nodes: [] });
    expect(entries()).toHaveLength(2);
  });
});
