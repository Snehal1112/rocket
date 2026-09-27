import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen } from '@testing-library/react';
import type { ReactElement } from 'react';
import { describe, expect, it, vi } from 'vitest';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import { WireExpressionPopover } from '../WireExpressionPopover';

// SingleLineEditor pulls in useVariableCommit, which drives react-query
// hooks unconditionally (see src/hooks/useVariableCommit.ts). A
// QueryClientProvider ancestor and these tauri-api mocks keep those
// queries from hitting the real Tauri IPC bridge in tests, mirroring
// src/components/editor/__tests__/SingleLineEditor.test.tsx.
vi.mock('@/stores/env-store', () => ({
  useEnvStore: (selector: (s: unknown) => unknown) =>
    selector({
      activeEnvId: null,
      activeCollection: null,
    }),
}));

vi.mock('@/lib/tauri-api', () => ({
  listEnvironments: vi.fn().mockResolvedValue([]),
  getGlobalEnvironmentName: vi.fn().mockResolvedValue(null),
  getGlobalEnvironment: vi.fn().mockResolvedValue(null),
  listGlobalEnvironments: vi.fn().mockResolvedValue([]),
  getProcessEnvVars: vi.fn().mockResolvedValue({}),
}));

function wrap(ui: ReactElement) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}

const edge: FlowEdge = {
  id: 'edge-1',
  sourceNodeId: 'node-a',
  targetNodeId: 'node-c',
  targetField: 'url',
  expression: 'response.body',
};

const targetNode: FlowNode = {
  id: 'node-c',
  kind: {
    kind: 'Request',
    label: 'Target',
    source: { type: 'Inline', request: { method: 'GET', url: '', headers: [], body: undefined } },
  },
  position: { x: 0, y: 0 },
};

describe('WireExpressionPopover', () => {
  it("pre-fills the expression editor with the edge's current expression and commits an edit", () => {
    const onCommit = vi.fn();
    wrap(
      <WireExpressionPopover
        edge={edge}
        targetNode={targetNode}
        open
        onOpenChange={() => {
          // Not under test here.
        }}
        onCommit={onCommit}
      >
        <button type='button'>anchor</button>
      </WireExpressionPopover>,
    );
    expect(screen.getByText('response.body')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(onCommit).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'edge-1', expression: 'response.body' }),
    );
  });
});
