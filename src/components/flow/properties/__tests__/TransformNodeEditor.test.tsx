import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { FlowNodeKind } from '@/lib/tauri-api';
import { TransformNodeEditor } from '../TransformNodeEditor';

// Monaco cannot run in jsdom. A textarea with the same value/onChange contract stands in.
vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: (props: {
    value: string;
    onChange?: (value: string) => void;
    language?: string;
  }) => (
    <textarea
      aria-label='Script editor'
      data-language={props.language}
      value={props.value}
      onChange={(e) => props.onChange?.(e.target.value)}
    />
  ),
}));

type TransformKind = Extract<FlowNodeKind, { kind: 'Transform' }>;
const kind: TransformKind = { kind: 'Transform', label: 'Pick', script: 'return 1;' };

describe('TransformNodeEditor', () => {
  it('shows the script in a JavaScript editor', () => {
    render(<TransformNodeEditor kind={kind} onChange={vi.fn()} />);
    const editor = screen.getByLabelText('Script editor');
    expect(editor).toHaveValue('return 1;');
    expect(editor).toHaveAttribute('data-language', 'javascript');
  });

  it('names the editor area Script for assistive tech', () => {
    render(<TransformNodeEditor kind={kind} onChange={vi.fn()} />);
    const group = screen.getByRole('group', { name: 'Script' });
    expect(group).toContainElement(screen.getByLabelText('Script editor'));
  });

  it('reports the whole node when the script changes', async () => {
    const onChange = vi.fn();
    render(<TransformNodeEditor kind={kind} onChange={onChange} />);
    await userEvent.type(screen.getByLabelText('Script editor'), '2');
    expect(onChange).toHaveBeenLastCalledWith({ ...kind, script: 'return 1;2' });
  });

  it('keeps the label when the script changes', async () => {
    const onChange = vi.fn();
    render(<TransformNodeEditor kind={kind} onChange={onChange} />);
    await userEvent.type(screen.getByLabelText('Script editor'), 'x');
    expect(onChange.mock.lastCall?.[0]).toMatchObject({ label: 'Pick' });
  });

  it('reports the whole node when the label changes', async () => {
    const onChange = vi.fn();
    render(<TransformNodeEditor kind={kind} onChange={onChange} />);
    await userEvent.type(screen.getByLabelText('Label'), 'x');
    expect(onChange).toHaveBeenLastCalledWith({ ...kind, label: 'Pickx' });
  });

  it('warns about an empty script', () => {
    render(<TransformNodeEditor kind={{ ...kind, script: ' \n ' }} onChange={vi.fn()} />);
    expect(screen.getByRole('alert')).toHaveTextContent(/script is empty/i);
  });

  it('shows no warning for a script with content', () => {
    render(<TransformNodeEditor kind={kind} onChange={vi.fn()} />);
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });
});
