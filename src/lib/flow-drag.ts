export const FLOW_REQUEST_DRAG_MIME = 'application/x-rocket-flow-request';
export const FLOW_REQUEST_DRAG_TEXT_PREFIX = 'rocket-flow-request:';

export interface FlowRequestDragPayload {
  collection: string;
  path: string;
  name: string;
  method: string;
}

export function encodeFlowRequestDragPayload(payload: FlowRequestDragPayload): string {
  return JSON.stringify(payload);
}

export function decodeFlowRequestDragPayload(
  dataTransfer: DataTransfer,
): FlowRequestDragPayload | null {
  const customPayload = dataTransfer.getData(FLOW_REQUEST_DRAG_MIME);
  const textPayload = dataTransfer.getData('text/plain');
  const raw =
    customPayload ||
    (textPayload.startsWith(FLOW_REQUEST_DRAG_TEXT_PREFIX)
      ? textPayload.slice(FLOW_REQUEST_DRAG_TEXT_PREFIX.length)
      : '');
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw) as Partial<FlowRequestDragPayload>;
    if (
      typeof parsed.collection === 'string' &&
      typeof parsed.path === 'string' &&
      typeof parsed.name === 'string' &&
      typeof parsed.method === 'string'
    ) {
      return parsed as FlowRequestDragPayload;
    }
    return null;
  } catch {
    return null;
  }
}
