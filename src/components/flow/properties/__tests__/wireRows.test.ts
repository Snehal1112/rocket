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

  it('names the auth field', () => {
    expect(fieldLabel('auth')).toBe('Auth');
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

  it('skips blank lines to find the first non-blank line', () => {
    expect(scriptPreview('\n\nconst x = 1;')).toBe('const x = 1;');
    expect(scriptPreview('  \n  return t;')).toBe('return t;');
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
        handle: null,
        exit: null,
        rows: [expect.objectContaining({ edgeId: 'e1', otherLabel: 'List Users', field: 'URL' })],
      },
    ]);
  });

  it('separates Switch cases with the same label into different groups', () => {
    const switchNode = n('sw', {
      kind: 'Switch',
      label: 'Route',
      value: 'response.status',
      cases: [
        { id: 'c1', label: 'OK', matches: '200' },
        { id: 'c2', label: 'OK', matches: '201' },
      ],
    });
    const edges = [
      edge({
        id: 'e1',
        sourceNodeId: 'sw',
        targetNodeId: 'out',
        targetField: 'value',
        sourceHandle: 'case:c1',
        expression: 'body',
      }),
      edge({
        id: 'e2',
        sourceNodeId: 'sw',
        targetNodeId: 'out',
        targetField: 'input',
        sourceHandle: 'case:c2',
        expression: 'body',
      }),
    ];
    const groups = outgoingGroups(switchNode, [switchNode, out], edges);
    expect(groups).toHaveLength(2);
    expect(groups[0].handle).toBe('case:c1');
    expect(groups[1].handle).toBe('case:c2');
    expect(groups[0].exit).toBe('OK');
    expect(groups[1].exit).toBe('OK');
  });

  it('does not merge a case labelled default with the real default exit', () => {
    const switchNode = n('sw', {
      kind: 'Switch',
      label: 'Route',
      value: 'response.status',
      cases: [{ id: 'c1', label: 'default', matches: '404' }],
    });
    const edges = [
      edge({
        id: 'e1',
        sourceNodeId: 'sw',
        targetNodeId: 'out',
        targetField: 'value',
        sourceHandle: 'case:c1',
      }),
      edge({
        id: 'e2',
        sourceNodeId: 'sw',
        targetNodeId: 'out',
        targetField: 'input',
        sourceHandle: 'default',
      }),
    ];
    const groups = outgoingGroups(switchNode, [switchNode, out], edges);
    expect(groups).toHaveLength(2);
    expect(groups[0].handle).toBe('case:c1');
    expect(groups[1].handle).toBe('default');
  });

  it('shows deleted case and orders it before default', () => {
    const switchNode = n('sw', {
      kind: 'Switch',
      label: 'Route',
      value: 'response.status',
      cases: [{ id: 'c1', label: 'OK', matches: '200' }],
    });
    const edges = [
      edge({
        id: 'e1',
        sourceNodeId: 'sw',
        targetNodeId: 'out',
        targetField: 'value',
        sourceHandle: 'case:deleted-id',
      }),
      edge({
        id: 'e2',
        sourceNodeId: 'sw',
        targetNodeId: 'out',
        targetField: 'input',
        sourceHandle: 'default',
      }),
    ];
    const groups = outgoingGroups(switchNode, [switchNode, out], edges);
    expect(groups).toHaveLength(2);
    expect(groups[0].exit).toBe('(deleted case)');
    expect(groups[1].exit).toBe('default');
  });

  it('marks a missing target node in outgoing rows', () => {
    const edges = [
      edge({
        id: 'e1',
        sourceNodeId: 'check',
        targetNodeId: 'missing',
        targetField: 'url',
        sourceHandle: 'true',
      }),
    ];
    const groups = outgoingGroups(check, [check], edges);
    expect(groups[0].rows[0]).toEqual(
      expect.objectContaining({
        otherLabel: null,
        editable: false,
      }),
    );
  });

  it('marks not-taken wires in outgoing groups', () => {
    const edges = [
      edge({
        id: 'e1',
        sourceNodeId: 'check',
        targetNodeId: 'users',
        targetField: 'trigger',
        sourceHandle: 'true',
      }),
    ];
    const groups = outgoingGroups(
      check,
      [check, users],
      edges,
      { check: 'success' },
      { check: { branch: 'false' } },
    );
    expect(groups[0].rows[0].notTaken).toBe(true);
  });
});

describe('auth wires', () => {
  const signIn = n('signin', {
    kind: 'Auth',
    label: 'Sign in',
    auth: { authType: 'bearer', token: 't' },
    applyToInherit: true,
  });
  const authEdge = edge({
    id: 'ea',
    sourceNodeId: 'signin',
    targetNodeId: 'users',
    targetField: 'auth',
    expression: 'response.body',
  });

  it('shows an incoming auth wire as Auth, with no preview, and not editable', () => {
    const rows = incomingRows(users, [signIn, users], [authEdge]);
    expect(rows).toEqual([
      expect.objectContaining({
        edgeId: 'ea',
        field: 'Auth',
        otherLabel: 'Sign in',
        preview: null,
        editable: false,
      }),
    ]);
  });

  it('shows an outgoing auth wire as not editable', () => {
    const groups = outgoingGroups(signIn, [signIn, users], [authEdge]);
    expect(groups[0].rows[0]).toEqual(
      expect.objectContaining({ field: 'Auth', preview: null, editable: false }),
    );
  });
});

describe('last run values', () => {
  const into = edge({
    id: 'e1',
    sourceNodeId: 'login',
    targetNodeId: 'users',
    targetField: 'headers[Authorization].value',
    expression: 'response.body.token',
  });

  it('reads the value from the target node trace for incoming and outgoing rows', () => {
    const detail = {
      users: { trace: { wires: [{ edgeId: 'e1', sourceNodeId: 'login', targetField: 'x', value: '••••••' }] } },
    };
    const [incoming] = incomingRows(users, [login, users], [into], {}, detail);
    expect(incoming.resolved).toEqual({ value: '••••••', truncated: false, credential: false });
    const [group] = outgoingGroups(login, [login, users], [into], {}, detail);
    expect(group.rows[0].resolved?.value).toBe('••••••');
  });

  it('has no value before a run', () => {
    const [row] = incomingRows(users, [login, users], [into]);
    expect(row.resolved).toBeUndefined();
    expect(row.failed).toBe(false);
  });

  it('drops a value from a credential wire', () => {
    const detail = {
      users: {
        trace: {
          wires: [
            { edgeId: 'e1', sourceNodeId: 'login', targetField: 'auth', credential: true, value: 'tok-123456' },
          ],
        },
      },
    };
    const [row] = incomingRows(users, [login, users], [into], {}, detail);
    expect(row.resolved).toEqual({ value: undefined, truncated: false, credential: true });
  });

  it('marks the wire that failed', () => {
    const second = edge({ id: 'e2', sourceNodeId: 'login', targetNodeId: 'users', targetField: 'url' });
    const detail = {
      users: {
        trace: {
          wires: [{ edgeId: 'e2', sourceNodeId: 'login', targetField: 'url', error: 'boom' }],
          failedEdgeId: 'e2',
        },
      },
    };
    const rows = incomingRows(users, [login, users], [into, second], {}, detail);
    expect(rows.map((r) => r.failed)).toEqual([false, true]);
    expect(rows[1].resolved?.error).toBe('boom');
  });
});
