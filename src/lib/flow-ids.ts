let counter = 0;

// A readable node id such as `input-1760000000000-3`. The counter keeps ids unique within a millisecond.
export function newNodeId(prefix: string): string {
  counter += 1;
  return `${prefix}-${Date.now()}-${counter}`;
}

// A random id for things nobody reads: edges, Switch cases and pasted nodes.
export function newEntityId(): string {
  return crypto.randomUUID();
}
