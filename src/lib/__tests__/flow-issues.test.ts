import { describe, expect, it } from 'vitest';
import type { FlowEdge, FlowNode, FlowNodeKind, RepeatUntil } from '@/lib/tauri-api';
import {
  cleanSaveMessage,
  computeFlowIssues,
  type FlowIssue,
  groupIssuesByNode,
  issueCountLabel,
  summarizeIssues,
  worstSeverity,
} from '../flow-issues';

const node = (id: string, kind: FlowNodeKind): FlowNode => ({
  id,
  kind,
  position: { x: 0, y: 0 },
});

const wire = (
  id: string,
  source: string,
  target: string,
  field: string,
  sourceHandle?: string,
): FlowEdge => ({
  id,
  sourceNodeId: source,
  targetNodeId: target,
  targetField: field,
  expression: '',
  ...(sourceHandle ? { sourceHandle } : {}),
});

const input = (id: string) => node(id, { kind: 'Input', label: id, value: 'x' });
const output = (id: string) => node(id, { kind: 'Output', label: id });
const inlineRequest = (id: string, url: string, repeatUntil?: RepeatUntil) =>
  node(id, {
    kind: 'Request',
    label: id,
    source: { type: 'Inline', request: { method: 'GET', url, headers: [] } },
    ...(repeatUntil ? { repeatUntil } : {}),
  });

const only = (issues: FlowIssue[], code: string) => issues.filter((i) => i.code === code);

describe('computeFlowIssues: a healthy flow', () => {
  it('reports nothing for wired nodes with all fields filled in', () => {
    const nodes = [
      input('in1'),
      node('tf1', { kind: 'Transform', label: 'tf1', script: 'return 1;' }),
      output('out1'),
      inlineRequest('req1', 'https://x.test'),
      output('out2'),
    ];
    const edges = [
      wire('e1', 'in1', 'tf1', 'input'),
      wire('e2', 'tf1', 'out1', 'value'),
      wire('e3', 'req1', 'out2', 'value'),
    ];
    expect(computeFlowIssues(nodes, edges)).toEqual([]);
  });

  it('reports nothing for an empty flow', () => {
    expect(computeFlowIssues([], [])).toEqual([]);
  });
});

describe('computeFlowIssues: expressions and inputs', () => {
  it.each<[string, FlowNodeKind]>([
    ['If', { kind: 'If', label: 'c', condition: '  ' }],
    ['Switch', { kind: 'Switch', label: 'c', value: '', cases: [] }],
    ['Transform', { kind: 'Transform', label: 'c', script: '\n' }],
  ])('flags a blank %s expression as an error', (_name, kind) => {
    const issues = computeFlowIssues([input('in1'), node('n1', kind)], [wire('e1', 'in1', 'n1', 'input')]);
    const blank = only(issues, 'expr-blank');
    expect(blank).toHaveLength(1);
    expect(blank[0]).toMatchObject({ severity: 'error', nodeId: 'n1' });
  });

  it('flags an If, Switch or Transform with no input wire, and only then', () => {
    const nodes = [node('n1', { kind: 'If', label: 'c', condition: 'true' }), input('in1')];
    expect(only(computeFlowIssues(nodes, []), 'input-missing').map((i) => i.nodeId)).toEqual(['n1']);
    expect(only(computeFlowIssues(nodes, [wire('e1', 'in1', 'n1', 'input')]), 'input-missing')).toEqual([]);
  });

  it('warns for an Output without a value wire, even with a Run when wire', () => {
    const nodes = [input('in1'), output('out1')];
    const trigger = [wire('e1', 'in1', 'out1', 'trigger')];
    const warned = only(computeFlowIssues(nodes, trigger), 'output-no-value');
    expect(warned).toHaveLength(1);
    expect(warned[0]).toMatchObject({ severity: 'warning', nodeId: 'out1' });
    expect(only(computeFlowIssues(nodes, [wire('e2', 'in1', 'out1', 'value')]), 'output-no-value')).toEqual([]);
  });
});

describe('computeFlowIssues: Request nodes', () => {
  it('flags an empty saved-request path', () => {
    const saved = node('r1', { kind: 'Request', label: 'r1', source: { type: 'Saved', requestPath: ' ' } });
    expect(only(computeFlowIssues([saved], []), 'request-path-empty')[0]).toMatchObject({
      severity: 'error',
      nodeId: 'r1',
    });
  });

  it('flags an empty inline URL unless a wire feeds the url field', () => {
    const req = inlineRequest('r1', '');
    expect(only(computeFlowIssues([req], []), 'request-url-empty')).toHaveLength(1);
    const wired = computeFlowIssues([input('in1'), req], [wire('e1', 'in1', 'r1', 'url')]);
    expect(only(wired, 'request-url-empty')).toEqual([]);
  });

  it.each<[string, Partial<RepeatUntil>, string]>([
    ['blank condition', { condition: ' ' }, 'condition is empty'],
    ['short interval', { intervalMs: 50 }, 'at least 100 ms'],
    ['zero attempts', { maxAttempts: 0 }, 'between 1 and 1000'],
    ['too many attempts', { maxAttempts: 1001 }, 'between 1 and 1000'],
    ['huge timeout', { timeoutMs: 3_600_001 }, 'at most 3600000 ms'],
    ['timeout under interval', { intervalMs: 5000, timeoutMs: 4000 }, 'shorter than the interval'],
  ])('flags repeat-until %s', (_name, patch, fragment) => {
    const base: RepeatUntil = { condition: 'response.status === 200', intervalMs: 1000, maxAttempts: 10, timeoutMs: 60000 };
    const issues = computeFlowIssues([inlineRequest('r1', 'https://x.test', { ...base, ...patch })], []);
    const found = only(issues, 'repeat-limits');
    expect(found).toHaveLength(1);
    expect(found[0].message).toContain(fragment);
  });

  it('accepts the default repeat-until settings', () => {
    const base: RepeatUntil = { condition: 'response.status === 200', intervalMs: 2000, maxAttempts: 30, timeoutMs: 60000 };
    expect(only(computeFlowIssues([inlineRequest('r1', 'https://x.test', base)], []), 'repeat-limits')).toEqual([]);
  });
});

describe('computeFlowIssues: Switch and Wait nodes', () => {
  it('flags duplicate case matches', () => {
    const sw = node('s1', {
      kind: 'Switch',
      label: 's1',
      value: 'x',
      cases: [
        { id: 'c1', label: 'A', matches: 'a' },
        { id: 'c2', label: 'B', matches: 'a' },
      ],
    });
    const issues = computeFlowIssues([input('in1'), sw], [wire('e1', 'in1', 's1', 'input')]);
    expect(only(issues, 'switch-duplicate-match')).toHaveLength(1);
  });

  const wait = (id: string, name: string, over: Partial<Extract<FlowNodeKind, { kind: 'WaitForCallback' }>> = {}) =>
    node(id, { kind: 'WaitForCallback', label: id, name, timeoutMs: 60000, ...over });

  it('flags an empty or invalid Wait name', () => {
    expect(only(computeFlowIssues([wait('w1', '')], []), 'wait-name-invalid')).toHaveLength(1);
    expect(only(computeFlowIssues([wait('w1', 'my hook')], []), 'wait-name-invalid')).toHaveLength(1);
    expect(only(computeFlowIssues([wait('w1', 'my_hook')], []), 'wait-name-invalid')).toEqual([]);
  });

  it('flags every Wait node that shares a name', () => {
    const issues = computeFlowIssues([wait('w1', 'cb'), wait('w2', 'cb'), wait('w3', 'other')], []);
    expect(only(issues, 'wait-name-duplicate').map((i) => i.nodeId)).toEqual(['w1', 'w2']);
  });

  it('flags a Wait timeout outside 1 s to 1 h and a blank accept_when', () => {
    expect(only(computeFlowIssues([wait('w1', 'cb', { timeoutMs: 999 })], []), 'wait-timeout-range')).toHaveLength(1);
    expect(only(computeFlowIssues([wait('w1', 'cb', { timeoutMs: 3_600_001 })], []), 'wait-timeout-range')).toHaveLength(1);
    expect(only(computeFlowIssues([wait('w1', 'cb', { acceptWhen: ' ' })], []), 'wait-accept-empty')).toHaveLength(1);
    expect(only(computeFlowIssues([wait('w1', 'cb', { acceptWhen: null })], []), 'wait-accept-empty')).toEqual([]);
  });
});

describe('computeFlowIssues: warnings about the shape of the flow', () => {
  const ifNode = node('if1', { kind: 'If', label: 'if1', condition: 'true' });
  const base = [input('in1'), ifNode, output('out1'), output('out2')];

  it('warns about unwired If exits, once per node', () => {
    const edges = [
      wire('e1', 'in1', 'if1', 'input'),
      wire('e2', 'if1', 'out1', 'value', 'true'),
      wire('e3', 'in1', 'out2', 'value'),
    ];
    const unwired = only(computeFlowIssues(base, edges), 'exit-unwired');
    expect(unwired).toHaveLength(1);
    expect(unwired[0]).toMatchObject({ severity: 'warning', nodeId: 'if1' });
    expect(unwired[0].message).toContain('false');
    expect(unwired[0].message).not.toContain('true');
  });

  it('is silent when both If exits are wired', () => {
    const edges = [
      wire('e1', 'in1', 'if1', 'input'),
      wire('e2', 'if1', 'out1', 'value', 'true'),
      wire('e3', 'if1', 'out2', 'value', 'false'),
    ];
    expect(only(computeFlowIssues(base, edges), 'exit-unwired')).toEqual([]);
  });

  it('warns about an unwired Switch default and unwired cases', () => {
    const sw = node('s1', {
      kind: 'Switch',
      label: 's1',
      value: 'x',
      cases: [{ id: 'c1', label: '', matches: 'a' }],
    });
    const edges = [wire('e1', 'in1', 's1', 'input')];
    const unwired = only(computeFlowIssues([input('in1'), sw, output('out1')], edges), 'exit-unwired');
    expect(unwired).toHaveLength(1);
    expect(unwired[0].message).toContain('Case 1');
    expect(unwired[0].message).toContain('default');
  });

  it('warns about a node that leads to no Output, but only when the flow has an Output', () => {
    const lonely = [input('in1'), output('out1'), input('in2')];
    const edges = [wire('e1', 'in1', 'out1', 'value')];
    const warned = only(computeFlowIssues(lonely, edges), 'no-path-to-output');
    expect(warned.map((i) => i.nodeId)).toEqual(['in2']);
    expect(only(computeFlowIssues([input('in1'), input('in2')], []), 'no-path-to-output')).toEqual([]);
  });

  it('counts a path through an Auth node and a Request as reaching the Output', () => {
    const auth = node('a1', {
      kind: 'Auth',
      label: 'a1',
      auth: { authType: 'bearer', token: 't' },
      applyToInherit: false,
    });
    const nodes = [auth, inlineRequest('r1', 'https://x.test'), output('out1')];
    const edges = [wire('e1', 'a1', 'r1', 'auth'), wire('e2', 'r1', 'out1', 'value')];
    expect(only(computeFlowIssues(nodes, edges), 'no-path-to-output')).toEqual([]);
  });
});

describe('computeFlowIssues: save errors', () => {
  const nodes = [output('a'), output('b')];

  it('folds the nodes and wires of a rejected save in as errors', () => {
    const issues = computeFlowIssues(nodes, [], {
      save: { nodeIds: ['b'], edgeIds: [], message: 'Invalid input: flow contains a cycle through node(s): b; edge(s): ' },
    });
    const saved = only(issues, 'save');
    expect(saved).toEqual([
      expect.objectContaining({ severity: 'error', nodeId: 'b', message: 'Flow contains a cycle.' }),
    ]);
  });

  it('hints at running again, not saving, for a refused run', () => {
    const issues = computeFlowIssues(nodes, [], {
      save: { nodeIds: ['b'], edgeIds: [], message: 'Invalid input: changed — node(s): b; edge(s): ', kind: 'run' },
    });
    expect(only(issues, 'save')[0].hint).toBe('Run the full flow, or Run from the named node.');
  });

  it('drops ids of nodes and wires that no longer exist', () => {
    const edges = [wire('e1', 'a', 'b', 'value')];
    const issues = computeFlowIssues(nodes, edges, {
      save: { nodeIds: ['gone', 'a'], edgeIds: ['e1', 'e-gone'], message: null },
    });
    const saved = only(issues, 'save');
    expect(saved.map((i) => i.nodeId ?? i.edgeId)).toEqual(['a', 'e1']);
    expect(saved.every((i) => i.severity === 'error')).toBe(true);
  });

  it('lists errors before warnings', () => {
    const issues = computeFlowIssues(nodes, [], { save: { nodeIds: ['b'], edgeIds: [], message: null } });
    const severities = issues.map((i) => i.severity);
    expect(severities).toEqual([...severities].sort((x, y) => (x === y ? 0 : x === 'error' ? -1 : 1)));
    expect(severities[0]).toBe('error');
  });
});

describe('cleanSaveMessage', () => {
  it('strips the prefix and the id list and ends with a full stop', () => {
    expect(
      cleanSaveMessage('Invalid input: flow contains a cycle through node(s): a, b; edge(s): e1, e2'),
    ).toBe('Flow contains a cycle.');
    expect(
      cleanSaveMessage(
        'Invalid input: flow is invalid: the If node needs exactly one input wire, found 0 — node(s): b; edge(s): ',
      ),
    ).toBe('The If node needs exactly one input wire, found 0.');
  });

  it('leaves a message without ids readable', () => {
    expect(cleanSaveMessage('disk full')).toBe('Disk full.');
  });
});

describe('issue helpers', () => {
  const error: FlowIssue = { code: 'expr-blank', severity: 'error', nodeId: 'n1', message: 'Boom.' };
  const warning: FlowIssue = { code: 'exit-unwired', severity: 'warning', nodeId: 'n1', message: 'Careful.' };
  const other: FlowIssue = { code: 'save', severity: 'error', edgeId: 'e1', message: 'Bad wire.' };

  it('groups by node and skips edge-only issues', () => {
    const grouped = groupIssuesByNode([error, warning, other]);
    expect(grouped.get('n1')).toEqual([error, warning]);
    expect(grouped.size).toBe(1);
  });

  it('reports the worst severity', () => {
    expect(worstSeverity([])).toBeNull();
    expect(worstSeverity([warning])).toBe('warning');
    expect(worstSeverity([warning, error])).toBe('error');
  });

  it('summarises one issue or several', () => {
    expect(summarizeIssues([error])).toBe('Error: Boom.');
    expect(summarizeIssues([error, warning])).toBe('2 issues: Error: Boom. Warning: Careful.');
  });

  it('counts errors and warnings in words', () => {
    expect(issueCountLabel([])).toBe('No issues');
    expect(issueCountLabel([error])).toBe('1 error');
    expect(issueCountLabel([error, other, warning])).toBe('2 errors, 1 warning');
    expect(issueCountLabel([warning, warning])).toBe('2 warnings');
  });
});
