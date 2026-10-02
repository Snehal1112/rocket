import type { FlowNodeKind } from '@/lib/tauri-api';

// These strings must match rocket_flow::handle in the backend.
export const RESULT_HANDLE = 'result';
export const TRUE_HANDLE = 'true';
export const FALSE_HANDLE = 'false';
export const DEFAULT_HANDLE = 'default';
export const INPUT_HANDLE = 'input';
export const TRIGGER_HANDLE = 'trigger';
export const AUTH_HANDLE = 'auth';
const CASE_PREFIX = 'case:';

export function caseHandle(caseId: string): string {
  return `${CASE_PREFIX}${caseId}`;
}

/** Returns the case id of a `case:<id>` handle, or null for any other handle. */
export function caseIdFromHandle(handle: string): string | null {
  if (!handle.startsWith(CASE_PREFIX)) return null;
  const id = handle.slice(CASE_PREFIX.length);
  return id ? id : null;
}

export type RoutingKind = Extract<FlowNodeKind, { kind: 'If' | 'Switch' }>;

export function isRoutingKind(kind: FlowNodeKind): kind is RoutingKind {
  return kind.kind === 'If' || kind.kind === 'Switch';
}

export type SingleInputKind = Extract<FlowNodeKind, { kind: 'If' | 'Switch' | 'Transform' }>;

/** True for the kinds that evaluate one upstream value through an `input` handle. */
export function takesSingleInput(kind: FlowNodeKind): kind is SingleInputKind {
  return kind.kind === 'If' || kind.kind === 'Switch' || kind.kind === 'Transform';
}
