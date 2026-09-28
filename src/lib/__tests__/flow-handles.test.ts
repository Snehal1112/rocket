import { describe, expect, it } from 'vitest';
import type { FlowNodeKind } from '@/lib/tauri-api';
import {
  caseHandle,
  caseIdFromHandle,
  DEFAULT_HANDLE,
  FALSE_HANDLE,
  INPUT_HANDLE,
  isRoutingKind,
  RESULT_HANDLE,
  TRIGGER_HANDLE,
  TRUE_HANDLE,
} from '../flow-handles';

describe('flow handle vocabulary', () => {
  it('uses the exact strings the backend expects', () => {
    expect([
      RESULT_HANDLE,
      TRUE_HANDLE,
      FALSE_HANDLE,
      DEFAULT_HANDLE,
      INPUT_HANDLE,
      TRIGGER_HANDLE,
    ]).toEqual(['result', 'true', 'false', 'default', 'input', 'trigger']);
  });

  it('builds and reads a case handle', () => {
    expect(caseHandle('01J9CASE')).toBe('case:01J9CASE');
    expect(caseIdFromHandle('case:01J9CASE')).toBe('01J9CASE');
  });

  it('returns null for a non-case handle or a case handle with no id', () => {
    expect(caseIdFromHandle('default')).toBeNull();
    expect(caseIdFromHandle('result')).toBeNull();
    expect(caseIdFromHandle('case:')).toBeNull();
  });

  it('recognises only If and Switch as routing kinds', () => {
    const kinds: FlowNodeKind[] = [
      { kind: 'If', label: 'i', condition: 'true' },
      { kind: 'Switch', label: 's', value: 'x', cases: [] },
      { kind: 'Output', label: 'o' },
      { kind: 'Input', label: 'in', value: 'v' },
      { kind: 'Request', label: 'r', source: { type: 'Saved', requestPath: 'a.yml' } },
    ];
    expect(kinds.map(isRoutingKind)).toEqual([true, true, false, false, false]);
  });
});
