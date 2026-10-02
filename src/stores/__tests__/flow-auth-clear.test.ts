import { beforeEach, describe, expect, it } from 'vitest';
import { flowAuthKey, flowAuthKeyMatches } from '@/lib/flow-auth';
import { useFlowAuthStore } from '@/stores/flow-auth-store';
import type { AuthState } from '@/types/pane-types';

const auth = { authType: 'bearer' } as AuthState;
const seed = (...keys: string[]) =>
  useFlowAuthStore.setState({ auths: Object.fromEntries(keys.map((k) => [k, { auth }])) });
const keys = () => Object.keys(useFlowAuthStore.getState().auths).sort();

describe('flowAuthKeyMatches', () => {
  it('matches the flow, and the node as an exact last segment', () => {
    const k = flowAuthKey('c', 'f', 'a', 'dev', 'g');
    expect(flowAuthKeyMatches(k, 'c', 'f')).toBe(true);
    expect(flowAuthKeyMatches(k, 'c', 'f', 'a')).toBe(true);
    expect(flowAuthKeyMatches(k, 'c', 'f', 'ba')).toBe(false);
    expect(flowAuthKeyMatches(flowAuthKey('c', 'f', 'ba', null, null), 'c', 'f', 'a')).toBe(false);
    expect(flowAuthKeyMatches(k, 'c', 'other')).toBe(false);
    expect(flowAuthKeyMatches(k, 'x', 'f')).toBe(false);
  });
});

describe('flow auth store clearing', () => {
  beforeEach(() => useFlowAuthStore.setState({ auths: {} }));

  it('clearNode removes the node in every environment variant and nothing else', () => {
    const keep = [
      flowAuthKey('c', 'f', 'ba', 'dev', 'g'),
      flowAuthKey('c', 'f', 'b', 'dev', 'g'),
      flowAuthKey('c', 'other', 'a', 'dev', 'g'),
      flowAuthKey('c2', 'f', 'a', 'dev', 'g'),
    ];
    seed(
      flowAuthKey('c', 'f', 'a', null, null),
      flowAuthKey('c', 'f', 'a', 'dev', null),
      flowAuthKey('c', 'f', 'a', 'dev', 'g'),
      flowAuthKey('c', 'f', 'a', null, 'g'),
      ...keep,
    );
    useFlowAuthStore.getState().clearNode('c', 'f', 'a');
    expect(keys()).toEqual([...keep].sort());
  });

  it('clearFlow removes every entry of the flow only', () => {
    const keep = [
      flowAuthKey('c', 'other', 'a', null, null),
      flowAuthKey('c2', 'f', 'a', null, null),
    ];
    seed(flowAuthKey('c', 'f', 'a', 'dev', null), flowAuthKey('c', 'f', 'b', null, null), ...keep);
    useFlowAuthStore.getState().clearFlow('c', 'f');
    expect(keys()).toEqual([...keep].sort());
  });
});
