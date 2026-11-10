import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ContractCheckResults } from '@/components/contracts/ContractCheckResults';
import type { ContractChange } from '@/lib/tauri-api';
import { contractCheckRunKey, useContractCheckRunsStore } from '@/stores/contract-check-runs-store';
import { useNetworkStore } from '@/stores/network-store';

vi.mock('@/components/git/DiffViewer', () => ({
  DiffViewer: (props: { diffState: { filePath: string; oldContent: string; newContent: string } }) => (
    <div data-testid='diff' data-path={props.diffState.filePath}>
      <pre data-testid='diff-new'>{props.diffState.newContent}</pre>
    </div>
  ),
}));

const change = (field: string, over: Partial<ContractChange> = {}): ContractChange => ({
  method: 'GET',
  pathTemplate: '/orders',
  field,
  changeType: 'changed',
  oldValue: 'a',
  newValue: 'b',
  breaking: false,
  ...over,
});

const NAMES = ['Ada', 'Bo', 'Cy', 'Di', 'Ed', 'Fay', 'Gus'];

function seed() {
  useContractCheckRunsStore.getState().setRun(contractCheckRunKey('/ws', 'k1'), {
    changes: [change('response.total'), change('header.x-key', { newValue: '[hidden]' })],
    targets: NAMES.map((name, i) => ({ checkId: `c${i}`, deviceId: `d${i}`, name })),
  });
  const set = useNetworkStore.getState().setContractCheckResult;
  set('c0', 'd0', { outcome: 'answered', rows: [{ index: 0, verdict: 'pass' }] });
  set('c1', 'd1', { outcome: 'answered', rows: [{ index: 1, verdict: 'break' }] });
  set('c2', 'd2', { outcome: 'answered', rows: [{ index: 0, verdict: 'not_used' }] });
  // d3 has not answered.
  set('c4', 'd4', { outcome: 'declined', rows: [] });
  set('c5', 'd5', { outcome: 'busy', rows: [] });
  set('c6', 'd6', { outcome: 'expired', rows: [] });
}

describe('ContractCheckResults', () => {
  beforeEach(() => {
    useNetworkStore.getState().reset();
    useContractCheckRunsStore.getState().reset();
  });

  it('renders nothing without a run', () => {
    const { container } = render(<ContractCheckResults collectionId='/ws' contractId='k1' />);
    expect(container).toBeEmptyDOMElement();
  });

  it('shows every state text and the breaking field', () => {
    seed();
    render(<ContractCheckResults collectionId='/ws' contractId='k1' />);
    for (const text of [
      'Pass',
      'Break',
      'Not used',
      'No answer yet',
      'Declined',
      'Busy',
      'No answer, the check expired',
    ]) {
      expect(screen.getByText(text)).toBeInTheDocument();
    }
    expect(screen.getByText('header.x-key')).toBeInTheDocument();
    expect(screen.getByTestId('diff')).toHaveAttribute('data-path', 'contract-check.txt');
    expect(screen.getByTestId('diff-new')).toHaveTextContent('[hidden]');
  });

  it('keeps No answer yet after closing and reopening the view', () => {
    seed();
    const first = render(<ContractCheckResults collectionId='/ws' contractId='k1' />);
    first.unmount();
    render(<ContractCheckResults collectionId='/ws' contractId='k1' />);
    expect(screen.getByText('No answer yet')).toBeInTheDocument();
    expect(screen.getByText('Di')).toBeInTheDocument();
  });

  it('renders a teammate name with markup as plain text', () => {
    useContractCheckRunsStore.getState().setRun(contractCheckRunKey('/ws', 'k1'), {
      changes: [change('f')],
      targets: [{ checkId: 'c1', deviceId: 'd1', name: '<img src=x onerror=alert(1)>' }],
    });
    const { baseElement } = render(<ContractCheckResults collectionId='/ws' contractId='k1' />);
    expect(screen.getByText('<img src=x onerror=alert(1)>')).toBeInTheDocument();
    expect(baseElement.querySelector('img')).toBeNull();
  });
});
