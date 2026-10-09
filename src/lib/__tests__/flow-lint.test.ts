import { describe, expect, it } from 'vitest';
import type { FlowIssue } from '@/lib/flow-issues';
import type { FlowLint } from '@/lib/tauri-api';
import { BACKEND_LINT_CODES, mergeFlowIssues, toFlowIssue } from '../flow-lint';
import fixture from './fixtures/flow-lint.json';

const issue = (over: Partial<FlowIssue>): FlowIssue => ({
  code: 'x',
  severity: 'warning',
  message: 'm',
  ...over,
});

describe('toFlowIssue', () => {
  it('maps the golden lint_flow payload without loss', () => {
    const issues = (fixture as FlowLint[]).map(toFlowIssue);
    expect(issues).toEqual([
      {
        code: 'exit_without_edge',
        severity: 'warning',
        nodeId: 'check',
        message: "The 'false' exit of 'Check status' has no wire.",
        hint: 'Wire it to a node, or remove the branch.',
      },
      {
        code: 'invalid_graph',
        severity: 'error',
        edgeId: 'e7',
        message: "an 'auth' wire must go from an Auth node into a Request node",
      },
    ]);
  });
});

describe('mergeFlowIssues', () => {
  it('shows a rule reported by both sides once, with the backend text', () => {
    const client = [issue({ code: 'exit_without_edge', nodeId: 'n1', message: 'client' })];
    const backend = [issue({ code: 'exit_without_edge', nodeId: 'n1', message: 'backend' })];
    expect(mergeFlowIssues(client, backend)).toEqual(backend);
  });

  it('treats a rejected save and an invalid_graph lint on one node as one issue', () => {
    const client = [issue({ code: 'save', severity: 'error', nodeId: 'n1' })];
    const backend = [issue({ code: 'invalid_graph', severity: 'error', nodeId: 'n1' })];
    expect(mergeFlowIssues(client, backend)).toEqual(backend);
  });

  it('keeps rules only the client knows and issues on other nodes', () => {
    const blank = issue({ code: 'blank_condition', severity: 'error', nodeId: 'n1' });
    const other = issue({ code: 'exit_without_edge', nodeId: 'n2' });
    const backend = [issue({ code: 'exit_without_edge', nodeId: 'n1' })];
    expect(mergeFlowIssues([blank, other], backend)).toEqual([blank, other, ...backend]);
  });

  it('tells edge issues apart from node issues', () => {
    const onEdge = issue({ code: 'invalid_graph', severity: 'error', edgeId: 'e1' });
    const onNode = issue({ code: 'invalid_graph', severity: 'error', nodeId: 'n1' });
    expect(mergeFlowIssues([onEdge], [onNode])).toEqual([onEdge, onNode]);
  });

  it('drops a backend invalid_graph on a node the client already flags as an error', () => {
    const client = [issue({ code: 'input-missing', severity: 'error', nodeId: 'n1' })];
    const backend = [issue({ code: 'invalid_graph', severity: 'error', nodeId: 'n1' })];
    expect(mergeFlowIssues(client, backend)).toEqual(client);
  });

  it('drops a backend invalid_graph on an edge the client already flags as an error', () => {
    const client = [issue({ code: 'expr-blank', severity: 'error', edgeId: 'e1' })];
    const backend = [issue({ code: 'invalid_graph', severity: 'error', edgeId: 'e1' })];
    expect(mergeFlowIssues(client, backend)).toEqual(client);
  });

  it('keeps an invalid_graph on a node without a client error, such as a cycle', () => {
    const warning = issue({ code: 'exit_without_edge', nodeId: 'n1' });
    const backend = [issue({ code: 'invalid_graph', severity: 'error', nodeId: 'n1' })];
    expect(mergeFlowIssues([warning], backend)).toEqual([...backend, warning]);
  });

  it('lists errors first, keeping the order inside a severity', () => {
    const cw = issue({ code: 'cw', nodeId: 'a' });
    const ce = issue({ code: 'ce', severity: 'error', nodeId: 'b' });
    const bw = issue({ code: 'exit_without_edge', nodeId: 'c' });
    const be = issue({ code: 'unknown_variable', severity: 'error', nodeId: 'd' });
    expect(mergeFlowIssues([cw, ce], [bw, be]).map((i) => i.code)).toEqual([
      'ce',
      'unknown_variable',
      'cw',
      'exit_without_edge',
    ]);
  });

  it('drops backend issues on nodes and edges that are gone', () => {
    const live = issue({ code: 'exit_without_edge', nodeId: 'n1' });
    const ghostNode = issue({ code: 'exit_without_edge', nodeId: 'gone' });
    const ghostEdge = issue({ code: 'invalid_graph', severity: 'error', edgeId: 'gone' });
    const present = { nodeIds: new Set(['n1']), edgeIds: new Set(['e1']) };
    expect(mergeFlowIssues([], [live, ghostNode, ghostEdge], present)).toEqual([live]);
  });

  it('lists the codes the backend owns', () => {
    expect(BACKEND_LINT_CODES).toEqual(
      expect.arrayContaining(['exit_without_edge', 'switch_without_default', 'no_path_to_output']),
    );
  });
});
