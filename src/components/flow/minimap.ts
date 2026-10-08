import type { Node } from '@xyflow/react';

// Plain hex colours only. Keep CSS mixing and blur effects out of the minimap.
const STATUS_COLOURS: Record<string, string> = {
  running: '#3b82f6',
  success: '#22c55e',
  failed: '#ef4444',
  skipped: '#9ca3af',
};

const KIND_COLOURS: Record<string, string> = {
  Request: '#6366f1',
  Input: '#14b8a6',
  Output: '#f59e0b',
  If: '#a855f7',
  Switch: '#a855f7',
  Transform: '#0ea5e9',
  WaitForCallback: '#ec4899',
  Auth: '#64748b',
};

const FALLBACK_COLOUR = '#94a3b8';

// A node shows its run status when it has one, and its kind otherwise.
export function minimapNodeColor(node: Node): string {
  const status = (node.data as { status?: string } | undefined)?.status;
  if (status && STATUS_COLOURS[status]) return STATUS_COLOURS[status];
  return KIND_COLOURS[node.type ?? ''] ?? FALLBACK_COLOUR;
}
