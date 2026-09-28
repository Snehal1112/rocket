import { caseIdFromHandle, DEFAULT_HANDLE, FALSE_HANDLE, TRUE_HANDLE } from '@/lib/flow-handles';
import type { FlowNodeKind } from '@/lib/tauri-api';

// Display label of a routing node's exit. A case is looked up by id, so
// renaming a case relabels its edges and badge without rewiring anything.
export function exitLabel(kind: FlowNodeKind, handle: string): string | undefined {
  if (kind.kind === 'If') {
    if (handle === TRUE_HANDLE) return 'true';
    if (handle === FALSE_HANDLE) return 'false';
    return undefined;
  }
  if (kind.kind === 'Switch') {
    if (handle === DEFAULT_HANDLE) return 'default';
    const caseId = caseIdFromHandle(handle);
    if (!caseId) return undefined;
    return kind.cases.find((c) => c.id === caseId)?.label;
  }
  return undefined;
}
