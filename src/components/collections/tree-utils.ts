import type { PaneNode } from '@/types/pane-types';

// Returns Tailwind text color class for an HTTP method.
export function methodColor(method: string): string {
  switch (method.toUpperCase()) {
    case 'GET':     return 'text-emerald-500';
    case 'POST':    return 'text-amber-500';
    case 'PUT':     return 'text-blue-500';
    case 'PATCH':   return 'text-violet-500';
    case 'DELETE':  return 'text-red-500';
    case 'OPTIONS': return 'text-cyan-500';
    case 'HEAD':    return 'text-pink-500';
    default:        return 'text-muted-foreground';
  }
}

// Returns true if any active tab in the pane tree matches the given tabId.
export function isActiveRequest(node: PaneNode, tabId: string): boolean {
  if (node.type === 'leaf') return node.activeTabId === tabId;
  return isActiveRequest(node.children[0], tabId) || isActiveRequest(node.children[1], tabId);
}

// Describes the item targeted for deletion in the shared confirmation dialog.
export type DeleteTarget = {
  type: 'collection' | 'folder' | 'request';
  collection: string;
  path?: string;
  name: string;
};
