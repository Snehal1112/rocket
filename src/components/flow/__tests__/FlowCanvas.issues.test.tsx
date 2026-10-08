import { render } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { FlowIssue } from '@/lib/flow-issues';
import type { FlowNode } from '@/lib/tauri-api';
import { FlowCanvas } from '../FlowCanvas';

// The real CodeMirror editor needs react-query and Tauri mocks.
vi.mock('@/components/editor', () => ({
  SingleLineEditor: (props: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
  }) => (
    <input
      aria-label={props['aria-label']}
      value={props.value}
      onChange={(e) => props.onChange(e.target.value)}
    />
  ),
}));

const at = { x: 0, y: 0 };
const nodes: FlowNode[] = [
  {
    id: 'auth',
    position: at,
    kind: {
      kind: 'Auth',
      label: 'Sign in',
      auth: { authType: 'bearer', token: 't' },
      applyToInherit: false,
    },
  },
  {
    id: 'req',
    position: at,
    kind: {
      kind: 'Request',
      label: 'Fetch',
      source: { type: 'Inline', request: { method: 'GET', url: 'https://x.test', headers: [] } },
    },
  },
  { id: 'in', position: at, kind: { kind: 'Input', label: 'Key', value: 'k' } },
  { id: 'out', position: at, kind: { kind: 'Output', label: 'Shown' } },
  { id: 'if', position: at, kind: { kind: 'If', label: 'Check', condition: 'true' } },
  {
    id: 'sw',
    position: at,
    kind: {
      kind: 'Switch',
      label: 'Route',
      value: 'x',
      cases: [{ id: 'c1', label: 'One', matches: '1' }],
    },
  },
  { id: 'tf', position: at, kind: { kind: 'Transform', label: 'Pick', script: 'return 1;' } },
  {
    id: 'wait',
    position: at,
    kind: { kind: 'WaitForCallback', label: 'Hook', name: 'cb', timeoutMs: 60000 },
  },
];

const cardIds = [
  'auth-node-card',
  'request-node-card',
  'input-node-card',
  'output-node-card',
  'if-node-card',
  'switch-node-card',
  'transform-node-card',
  'wait-node-card',
];

const issueFor = (nodeId: string, severity: FlowIssue['severity']): FlowIssue => ({
  code: 'test',
  severity,
  nodeId,
  message: severity === 'error' ? 'Boom.' : 'Careful.',
});

function renderCanvas(issues?: FlowIssue[]) {
  return render(
    <FlowCanvas
      nodes={nodes}
      edges={[]}
      nodeStatus={{}}
      onNodesChange={vi.fn()}
      onEdgesChange={vi.fn()}
      onConnect={vi.fn()}
      issues={issues}
    />,
  );
}

const card = (id: string) => document.querySelector<HTMLElement>(`[data-testid="${id}"]`);

describe('FlowCanvas issue badges', () => {
  it('draws no ring and no badge without issues', () => {
    renderCanvas();
    for (const id of cardIds) {
      const el = card(id);
      expect(el, id).not.toBeNull();
      expect(el?.className).not.toContain('ring-');
      expect(el?.querySelector('[data-testid="node-issue-badge"]')).toBeNull();
    }
  });

  it('gives all eight node kinds a red ring and an error badge', () => {
    renderCanvas(nodes.map((n) => issueFor(n.id, 'error')));
    for (const id of cardIds) {
      const el = card(id);
      expect(el?.className, id).toContain('ring-red-500');
      const badge = el?.querySelector('[data-testid="node-issue-badge"]');
      expect(badge, id).toHaveAttribute('data-severity', 'error');
      expect(badge, id).toHaveAttribute('aria-label', 'Error: Boom.');
    }
  });

  it('gives all eight node kinds an amber ring and a warning badge, never red', () => {
    renderCanvas(nodes.map((n) => issueFor(n.id, 'warning')));
    for (const id of cardIds) {
      const el = card(id);
      expect(el?.className, id).toContain('ring-amber-500');
      expect(el?.className, id).not.toContain('ring-red-500');
      expect(el?.querySelector('[data-testid="node-issue-badge"]'), id).toHaveAttribute(
        'data-severity',
        'warning',
      );
    }
  });

  it('marks only the nodes that have issues', () => {
    renderCanvas([issueFor('out', 'error')]);
    expect(card('output-node-card')?.className).toContain('ring-red-500');
    expect(card('input-node-card')?.className).not.toContain('ring-');
  });
});
