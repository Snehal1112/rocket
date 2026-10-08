import { describe, expect, it } from 'vitest';
import type { FlowTab } from '@/types/pane-types';
import { flowPayloadFromTab } from '../flow-save';

const base: FlowTab = {
  id: 't1',
  title: 'Flow: my-flow',
  isDirty: true,
  tabType: 'flow',
  collectionName: 'demo',
  flowName: 'my-flow',
  nodes: [],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
};

describe('flowPayloadFromTab', () => {
  it('builds the save arguments from the tab', () => {
    expect(flowPayloadFromTab(base)).toEqual({
      collection: 'demo',
      flow: { name: 'my-flow', nodes: [], edges: [] },
    });
  });

  it('includes the callback host only when set', () => {
    const withHost = flowPayloadFromTab({ ...base, callbackHost: '10.0.0.5' });
    expect(withHost?.flow.callbackHost).toBe('10.0.0.5');
    const withNull = flowPayloadFromTab({ ...base, callbackHost: null });
    expect(withNull?.flow).not.toHaveProperty('callbackHost');
  });

  it('returns null for a picker tab', () => {
    expect(flowPayloadFromTab({ ...base, flowName: null })).toBeNull();
    expect(flowPayloadFromTab({ ...base, collectionName: null })).toBeNull();
  });
});
