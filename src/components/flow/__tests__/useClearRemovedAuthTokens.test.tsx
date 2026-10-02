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

  it('keeps entries when the list changes but the Auth nodes stay', () => {
    const { rerender } = renderHook(({ nodes }) => useClearRemovedAuthTokens('c', 'f', nodes), {
      initialProps: { nodes: [authNode('a1'), authNode('a2')] },
    });
    rerender({ nodes: [authNode('a1'), authNode('a2')] });
    expect(Object.keys(useFlowAuthStore.getState().auths)).toHaveLength(2);
  });
});
