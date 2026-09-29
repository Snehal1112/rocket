import { describe, expect, it } from 'vitest';
import type { FlowNode } from '@/lib/tauri-api';
import { callbackVariable, isValidCallbackName, nextCallbackName } from '../flow-callback';

const waitNode = (id: string, name: string): FlowNode => ({
  id,
  kind: { kind: 'WaitForCallback', label: id, name, timeoutMs: 60000 },
  position: { x: 0, y: 0 },
});

describe('flow-callback helpers', () => {
  it('accepts letters, digits and underscores only', () => {
    expect(isValidCallbackName('payment_2')).toBe(true);
    for (const bad of ['', 'pay ment', 'pay-ment', 'a.b', 'päy']) {
      expect(isValidCallbackName(bad)).toBe(false);
    }
  });

  it('builds the run-scoped variable', () => {
    expect(callbackVariable('payment')).toBe('{{callback.payment}}');
  });

  it('nextCallbackName picks the first free name', () => {
    expect(nextCallbackName([])).toBe('callback');
    expect(nextCallbackName([waitNode('a', 'callback')])).toBe('callback_2');
    expect(nextCallbackName([waitNode('a', 'callback'), waitNode('b', 'callback_2')])).toBe(
      'callback_3',
    );
    expect(nextCallbackName([waitNode('a', 'callback_2')])).toBe('callback');
  });
});
