import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { type KeyValueRow, KeyValueRows } from '@/components/ui/key-value-rows';

function Pair(props: {
  initial: KeyValueRow[];
  rowErrors?: (string | null)[];
  onChange?: (rows: KeyValueRow[]) => void;
}) {
  const [rows, setRows] = useState(props.initial);
  return (
    <KeyValueRows
      rows={rows}
      onChange={(next) => {
        setRows(next);
        props.onChange?.(next);
      }}
      columns='pair'
      nameLabel='Name'
      valueLabel='Value'
      addLabel='Add variable'
      rowErrors={props.rowErrors}
    />
  );
}

function Single(props: { initial: KeyValueRow[] }) {
  const [rows, setRows] = useState(props.initial);
  return (
    <KeyValueRows
      rows={rows}
      onChange={setRows}
      columns='single'
      valueLabel='Argument'
      addLabel='Add argument'
    />
  );
}

const TWO: KeyValueRow[] = [
  { key: 'k1', name: 'A', value: '1' },
  { key: 'k2', name: 'B', value: '2' },
];

describe('KeyValueRows', () => {
  it('renders a name and a value input per row in pair mode', () => {
    render(<Pair initial={TWO} />);
    expect(screen.getByLabelText('Name 1')).toHaveValue('A');
    expect(screen.getByLabelText('Value 1')).toHaveValue('1');
    expect(screen.getByLabelText('Name 2')).toHaveValue('B');
    expect(screen.getByLabelText('Value 2')).toHaveValue('2');
  });

  it('uses a two-column plus remove grid for pair and one-column plus remove for single', () => {
    const { container, rerender } = render(<Pair initial={TWO} />);
    expect(container.querySelector('[data-slot="kv-row"]')).toHaveClass(
      'grid-cols-[1fr_1fr_32px]',
    );
    rerender(<Single initial={[{ key: 'a', name: '', value: 'run' }]} />);
    expect(container.querySelector('[data-slot="kv-row"]')).toHaveClass('grid-cols-[1fr_32px]');
  });

  it('edits only the value in single mode', async () => {
    render(<Single initial={[{ key: 'a', name: '', value: 'run' }]} />);
    expect(screen.queryByLabelText('Name 1')).toBeNull();
    const input = screen.getByLabelText('Argument 1');
    await userEvent.type(input, 's');
    expect(input).toHaveValue('runs');
  });

  it('reports edits with the stable key untouched', async () => {
    const onChange = vi.fn();
    render(<Pair initial={TWO} onChange={onChange} />);
    await userEvent.type(screen.getByLabelText('Name 1'), 'X');
    expect(onChange).toHaveBeenLastCalledWith([
      { key: 'k1', name: 'AX', value: '1' },
      { key: 'k2', name: 'B', value: '2' },
    ]);
  });

  it('removes a row with an accessible ghost button', async () => {
    const onChange = vi.fn();
    render(<Pair initial={TWO} onChange={onChange} />);
    await userEvent.click(screen.getByRole('button', { name: 'Remove name 1' }));
    expect(onChange).toHaveBeenLastCalledWith([{ key: 'k2', name: 'B', value: '2' }]);
    expect(screen.queryByLabelText('Name 2')).toBeNull();
  });

  it('names the remove button after the value label in single mode', async () => {
    render(<Single initial={[{ key: 'a', name: '', value: 'run' }]} />);
    await userEvent.click(screen.getByRole('button', { name: 'Remove argument 1' }));
    expect(screen.queryByLabelText('Argument 1')).toBeNull();
    expect(screen.getByRole('button', { name: 'Add argument' })).toBeInTheDocument();
  });

  it('adds an empty row with a fresh unique key', async () => {
    const onChange = vi.fn();
    render(<Pair initial={TWO} onChange={onChange} />);
    await userEvent.click(screen.getByRole('button', { name: 'Add variable' }));
    const next = onChange.mock.lastCall?.[0] as KeyValueRow[];
    expect(next).toHaveLength(3);
    expect(next[2]).toMatchObject({ name: '', value: '' });
    expect(new Set(next.map((r) => r.key)).size).toBe(3);
  });

  it('renders only the Add button for empty rows', async () => {
    const onChange = vi.fn();
    const { container } = render(<Pair initial={[]} onChange={onChange} />);
    expect(container.querySelector('[data-slot="kv-row"]')).toBeNull();
    expect(container.querySelector('[data-slot="kv-header"]')).toBeNull();
    expect(screen.queryByRole('textbox')).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Add variable' }));
    expect(onChange).toHaveBeenCalledTimes(1);
    expect((onChange.mock.calls[0]?.[0] as KeyValueRow[]).length).toBe(1);
  });

  it('shows no error for a blank row unless one is given', () => {
    const blank: KeyValueRow[] = [{ key: 'b1', name: '', value: '' }];
    render(<Pair initial={blank} />);
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.getByLabelText('Name 1')).toHaveAttribute('aria-invalid', 'false');
    expect(screen.getByLabelText('Value 1')).toHaveAttribute('aria-invalid', 'false');
  });

  it('shows a row error and marks the inputs invalid', () => {
    render(<Pair initial={TWO} rowErrors={[null, 'Duplicate name.']} />);
    expect(screen.getByRole('alert')).toHaveTextContent('Duplicate name.');
    expect(screen.getByLabelText('Name 2')).toHaveAttribute('aria-invalid', 'true');
    expect(screen.getByLabelText('Value 2')).toHaveAttribute('aria-invalid', 'true');
    expect(screen.getByLabelText('Name 1')).toHaveAttribute('aria-invalid', 'false');
  });

  it('gives every row control a unique id', () => {
    const { container } = render(<Pair initial={TWO} rowErrors={['Bad.', 'Bad.']} />);
    const ids = Array.from(container.querySelectorAll('[id]')).map((node) => node.id);
    expect(ids.length).toBeGreaterThanOrEqual(6);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('keeps an add button that is left aligned', () => {
    render(<Pair initial={TWO} />);
    const add = screen.getByRole('button', { name: 'Add variable' });
    expect(add.parentElement).toHaveClass('flex', 'justify-start');
  });
});
