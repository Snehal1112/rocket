import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { FlowNode, FlowNodeKind } from '@/lib/tauri-api';
import { NodePropertiesPanel } from '../NodePropertiesPanel';

// The real CodeMirror editor needs Tauri and react-query. A plain input with
// the same value/onChange contract is enough here.
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

// The Request editor pulls in Monaco, which jsdom cannot load.
vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));

const node = (id: string, kind: FlowNodeKind): FlowNode => ({ id, kind, position: { x: 0, y: 0 } });

function renderPanel(n: FlowNode) {
  const onChange = vi.fn();
  const onClose = vi.fn();
  const onDelete = vi.fn();
  render(
    <NodePropertiesPanel
      node={n}
      edges={[]}
      collection='demo'
      onChange={onChange}
      onClose={onClose}
      onDelete={onDelete}
    />,
  );
  return { onChange, onClose, onDelete };
}

describe('NodePropertiesPanel', () => {
  it('calls onDelete from the Delete node button', async () => {
    const { onDelete } = renderPanel(node('o1', { kind: 'Output', label: 'Out' }));
    await userEvent.click(screen.getByRole('button', { name: 'Delete node' }));
    expect(onDelete).toHaveBeenCalledTimes(1);
  });

  it('edits the label of any node kind with one change per keystroke', async () => {
    const { onChange } = renderPanel(node('o1', { kind: 'Output', label: 'Out' }));
    await userEvent.type(screen.getByLabelText('Label'), 'x');
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenLastCalledWith({ kind: 'Output', label: 'Outx' });
  });

  it('edits an Input value through the variable-aware editor', async () => {
    const { onChange } = renderPanel(node('i1', { kind: 'Input', label: 'User', value: 'al' }));
    await userEvent.type(screen.getByLabelText('Input value'), 'i');
    expect(onChange).toHaveBeenLastCalledWith({ kind: 'Input', label: 'User', value: 'ali' });
  });

  it('shows a structured value read-only', () => {
    renderPanel(node('i1', { kind: 'Input', label: 'User', value: { secret: true } }));
    expect(screen.queryByLabelText('Input value')).not.toBeInTheDocument();
    expect(screen.getByTestId('input-value-readonly')).toHaveTextContent('{"secret":true}');
  });

  it('tells the user where If and Switch details are edited', () => {
    renderPanel(node('if1', { kind: 'If', label: 'Ok?', condition: 'true' }));
    expect(screen.getByText('The condition is edited on the node itself.')).toBeInTheDocument();
  });

  it('shows the kind and label in its header and closes on ✕', async () => {
    const { onClose } = renderPanel(node('o1', { kind: 'Output', label: 'Result' }));
    expect(screen.getByTestId('node-properties-panel')).toHaveTextContent('Output · Result');
    await userEvent.click(screen.getByRole('button', { name: 'Close properties' }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
