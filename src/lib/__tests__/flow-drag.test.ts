import { describe, expect, it } from 'vitest';
import {
  decodeFlowRequestDragPayload,
  encodeFlowRequestDragPayload,
  FLOW_REQUEST_DRAG_MIME,
  FLOW_REQUEST_DRAG_TEXT_PREFIX,
} from '../flow-drag';

function fakeDataTransfer(data: Record<string, string>): DataTransfer {
  return {
    getData: (type: string) => data[type] ?? '',
    // No-op: this fake never needs to report back what was set.
    setData: () => undefined,
  } as unknown as DataTransfer;
}

describe('flow-drag payload codec', () => {
  it('round-trips a payload through encode/decode', () => {
    const payload = {
      collection: 'my-collection',
      path: 'auth/login.yml',
      name: 'Login',
      method: 'POST',
    };
    const encoded = encodeFlowRequestDragPayload(payload);
    const dt = fakeDataTransfer({ [FLOW_REQUEST_DRAG_MIME]: encoded });
    expect(decodeFlowRequestDragPayload(dt)).toEqual(payload);
  });

  it('returns null when the drag payload MIME type is absent', () => {
    const dt = fakeDataTransfer({ 'text/plain': 'not a flow drag' });
    expect(decodeFlowRequestDragPayload(dt)).toBeNull();
  });

  it('decodes the self-identifying text fallback when the custom MIME type is absent', () => {
    const payload = {
      collection: 'my-collection',
      path: 'auth/login.yml',
      name: 'Login',
      method: 'POST',
    };
    const dt = fakeDataTransfer({
      'text/plain': `${FLOW_REQUEST_DRAG_TEXT_PREFIX}${encodeFlowRequestDragPayload(payload)}`,
    });
    expect(decodeFlowRequestDragPayload(dt)).toEqual(payload);
  });

  it('returns null when the payload is present but not valid JSON', () => {
    const dt = fakeDataTransfer({ [FLOW_REQUEST_DRAG_MIME]: '{not json' });
    expect(decodeFlowRequestDragPayload(dt)).toBeNull();
  });
});
