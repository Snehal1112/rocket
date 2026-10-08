import type { FlowNode } from '@/lib/tauri-api';

// The text a user can search in one node: label, kind, request target and Switch value.
// An Input value is left out on purpose, since it can be a secret.
function searchableText(node: FlowNode): string[] {
  const kind = node.kind;
  const fields = [kind.label, kind.kind];
  if (kind.kind === 'Request') {
    fields.push(kind.source.type === 'Saved' ? kind.source.requestPath : kind.source.request.url);
  } else if (kind.kind === 'Switch') {
    fields.push(kind.value);
  }
  return fields;
}

/** Ids of the nodes that match the query, in node order. A blank query matches nothing. */
export function searchFlowNodes(nodes: FlowNode[], query: string): string[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return [];
  return nodes
    .filter((node) => searchableText(node).some((text) => text.toLowerCase().includes(needle)))
    .map((node) => node.id);
}
