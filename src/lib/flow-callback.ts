import type { FlowNode } from '@/lib/tauri-api';

// These values must match rocket_flow's CALLBACK_* constants.
export const DEFAULT_CALLBACK_TIMEOUT_MS = 60_000;
export const MIN_CALLBACK_TIMEOUT_MS = 1_000;
export const MAX_CALLBACK_TIMEOUT_MS = 3_600_000;

const NAME_PATTERN = /^[A-Za-z0-9_]+$/;

export function isValidCallbackName(name: string): boolean {
  return NAME_PATTERN.test(name);
}

/** The run-scoped variable a request uses to send this node's URL. */
export function callbackVariable(name: string): string {
  return `{{callback.${name}}}`;
}

/** `callback`, then `callback_2`, `callback_3`, … whichever is free first. */
export function nextCallbackName(nodes: FlowNode[]): string {
  const taken = new Set(
    nodes.flatMap((n) => (n.kind.kind === 'WaitForCallback' ? [n.kind.name] : [])),
  );
  if (!taken.has('callback')) return 'callback';
  let i = 2;
  while (taken.has(`callback_${i}`)) i += 1;
  return `callback_${i}`;
}
