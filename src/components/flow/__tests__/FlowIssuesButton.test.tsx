import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { FlowIssue } from '@/lib/flow-issues';
import type { FlowNode } from '@/lib/tauri-api';
import { FlowIssuesButton } from '../FlowIssuesButton';

// Radix popovers call pointer-capture and scrollIntoView APIs that jsdom lacks.
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.releasePointerCapture ??= () => undefined;
Element.prototype.scrollIntoView ??= () => undefined;

const nodes: FlowNode[] = [
  { id: 'n1', position: { x: 0, y: 0 }, kind: { kind: 'If', label: 'Check', condition: '' } },
  { id: 'n2', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Result' } },
];

const issues: FlowIssue[] = [
  { code: 'expr-blank', severity: 'error', nodeId: 'n1', message: 'The If node condition is empty.' },
  { code: 'output-no-value', severity: 'warning', nodeId: 'n2', message: 'No value is wired.' },
  { code: 'save', severity: 'error', edgeId: 'e1', message: 'Bad wire.' },
];

describe('FlowIssuesButton', () => {
  it('renders nothing without issues', () => {
    const { container } = render(<FlowIssuesButton issues={[]} nodes={nodes} onSelectNode={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('names the count by severity', () => {
    render(<FlowIssuesButton issues={issues} nodes={nodes} onSelectNode={vi.fn()} />);
    expect(screen.getByRole('button', { name: '2 errors, 1 warning' })).toHaveTextContent('3');
  });

  it('lists every issue, with the node label, in the popover', async () => {
    render(<FlowIssuesButton issues={issues} nodes={nodes} onSelectNode={vi.fn()} />);
    await userEvent.click(screen.getByRole('button', { name: '2 errors, 1 warning' }));
    const list = await screen.findByRole('list', { name: 'Flow issues' });
    expect(within(list).getAllByRole('listitem')).toHaveLength(issues.length);
    expect(list).toHaveTextContent('Check');
    expect(list).toHaveTextContent('The If node condition is empty.');
  });

  it('selects the node when its item is clicked, and closes the list', async () => {
    const onSelectNode = vi.fn();
    render(<FlowIssuesButton issues={issues} nodes={nodes} onSelectNode={onSelectNode} />);
    await userEvent.click(screen.getByRole('button', { name: '2 errors, 1 warning' }));
    const list = await screen.findByRole('list', { name: 'Flow issues' });
    await userEvent.click(within(list).getAllByRole('button')[0]);
    expect(onSelectNode).toHaveBeenCalledWith('n1');
    expect(screen.queryByRole('list', { name: 'Flow issues' })).not.toBeInTheDocument();
  });

  it('selects the node only after the list closed, and leaves focus off the trigger', async () => {
    const trigger = () => screen.getByRole('button', { name: '2 errors, 1 warning' });
    let listOpenAtSelect: boolean | null = null;
    const onSelectNode = vi.fn(() => {
      listOpenAtSelect = screen.queryByRole('list', { name: 'Flow issues' }) !== null;
    });
    render(<FlowIssuesButton issues={issues} nodes={nodes} onSelectNode={onSelectNode} />);
    await userEvent.click(trigger());
    const list = await screen.findByRole('list', { name: 'Flow issues' });
    await userEvent.click(within(list).getAllByRole('button')[0]);
    await waitFor(() => expect(onSelectNode).toHaveBeenCalledWith('n1'));
    expect(listOpenAtSelect).toBe(false);
    expect(document.activeElement).not.toBe(trigger());
  });

  it('does not make an issue without a node clickable', async () => {
    render(<FlowIssuesButton issues={issues} nodes={nodes} onSelectNode={vi.fn()} />);
    await userEvent.click(screen.getByRole('button', { name: '2 errors, 1 warning' }));
    const list = await screen.findByRole('list', { name: 'Flow issues' });
    // Two issues name a node, so two buttons. The edge issue is plain text.
    expect(within(list).getAllByRole('button')).toHaveLength(2);
    expect(list).toHaveTextContent('Bad wire.');
  });
});
