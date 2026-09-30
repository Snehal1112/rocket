import { describe, expect, it } from 'vitest';
import type { FlowEdge, FlowNode, FlowNodeKind } from '@/lib/tauri-api';
import { fieldLabel, incomingRows, outgoingGroups, scriptPreview } from '../wireRows';

const n = (id: string, kind: FlowNodeKind): FlowNode => ({ id, kind, position: { x: 0, y: 0 } });
const login = n('login', {
  kind: 'Request',
  label: 'Login',
  source: { type: 'Saved', requestPath: 'auth/login.yml' },
});
const check = n('check', { kind: 'If', label: 'Ok?', condition: 'response.status === 200' });
const users = n('users', {
  kind: 'Request',
  label: 'List Users',
  source: { type: 'Saved', requestPath: 'users/list.yml' },
});
const out = n('out', { kind: 'Output', label: 'Result' });
const edge = (
  e: Partial<FlowEdge> & Pick<FlowEdge, 'id' | 'sourceNodeId' | 'targetNodeId' | 'targetField'>,
): FlowEdge => ({
  expression: '',
  ...e,
});

describe('fieldLabel', () => {
  it('names header, URL, body and Run when fields', () => {
    expect(fieldLabel('headers[Authorization].value')).toBe('Authorization');
    expect(fieldLabel('url')).toBe('URL');
    expect(fieldLabel('body')).toBe('Body');
    expect(fieldLabel('trigger')).toBe('Run when');
    expect(fieldLabel('value')).toBe('Value');
    expect(fieldLabel('input')).toBe('Input');
    expect(fieldLabel('headers')).toBe('Headers');
    expect(fieldLabel('custom')).toBe('custom');
  });
});

describe('scriptPreview', () => {
  it('previews only the first line of a script', () => {
    expect(scriptPreview('const t = response.body.token;\nreturn t;')).toBe(
      'const t = response.body.token;',
    );
    expect(scriptPreview(`  ${'a'.repeat(80)}  `)).toBe(`${'a'.repeat(60)}…`);
    expect(scriptPreview('   ')).toBeNull();
    expect(scriptPreview('')).toBeNull();
  });
});

describe('incomingRows', () => {
  it('lists incoming wires with source label and exit', () => {
    const edges = [
      edge({
        id: 'e1',
        sourceNodeId: 'login',
        targetNodeId: 'users',
        targetField: 'headers[Authorization].value',
        expression: "'Bearer ' + response.body.token",
      }),
      edge({
        id: 'e2',
        sourceNodeId: 'check',
        targetNodeId: 'users',
        targetField: 'trigger',
        sourceHandle: 'true',
      }),
    ];
    const rows = incomingRows(users, [login, check, users], edges);
    expect(rows).toEqual([
      expect.objectContaining({
        edgeId: 'e1',
        field: 'Authorization',
        otherLabel: 'Login',
        exit: null,
        preview: "'Bearer ' + response.body.token",
        editable: true,
      }),
      expect.objectContaining({
        edgeId: 'e2',
        field: 'Run when',
        otherLabel: 'Ok?',
        exit: 'true',
        preview: null,
        editable: false,
      }),
    ]);
  });

  it('marks a missing source node', () => {
    const edges = [
      edge({
        id: 'e1',
        sourceNodeId: 'gone',
        targetNodeId: 'users',
        targetField: 'url',
        expression: 'response.body',
      }),
    ];
    const [row] = incomingRows(users, [users], edges);
    expect(row.otherLabel).toBeNull();
    expect(row.editable).toBe(false);
  });

  it('marks a wire the last run did not take', () => {
    const edges = [
      edge({
        id: 'e2',
        sourceNodeId: 'check',
        targetNodeId: 'users',
        targetField: 'trigger',
        sourceHandle: 'true',
      }),
    ];
    const [row] = incomingRows(
      users,
      [check, users],
      edges,
      { check: 'success' },
      { check: { branch: 'false' } },
    );
    expect(row.notTaken).toBe(true);
  });
});

describe('outgoingGroups', () => {
  it('groups outgoing wires by exit in handle order', () => {
    const edges = [
      edge({
        id: 'e3',
        sourceNodeId: 'check',
        targetNodeId: 'out',
        targetField: 'value',
        sourceHandle: 'false',
        expression: 'response.body',
      }),
      edge({
        id: 'e2',
        sourceNodeId: 'check',
        targetNodeId: 'users',
        targetField: 'trigger',
        sourceHandle: 'true',
      }),
    ];
    const groups = outgoingGroups(check, [check, users, out], edges);
    expect(groups.map((g) => g.exit)).toEqual(['true', 'false']);
    expect(groups[0].rows[0]).toEqual(
      expect.objectContaining({ otherLabel: 'List Users', field: 'Run when' }),
    );
    expect(groups[1].rows[0]).toEqual(
      expect.objectContaining({ otherLabel: 'Result', field: 'Value', editable: true }),
    );
  });

  it('puts plain result wires in one group with no exit name', () => {
    const edges = [
      edge({
        id: 'e1',
        sourceNodeId: 'login',
        targetNodeId: 'users',
        targetField: 'url',
        expression: 'response.body.next',
      }),
    ];
    const groups = outgoingGroups(login, [login, users], edges);
    expect(groups).toEqual([
      {
        exit: null,
        rows: [expect.objectContaining({ edgeId: 'e1', otherLabel: 'List Users', field: 'URL' })],
      },
    ]);
  });
});
