import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { DEFAULT_REPEAT_UNTIL } from '@/lib/flow-repeat';
import type { RepeatUntil } from '@/lib/tauri-api';
import { RepeatUntilSection } from '../RepeatUntilSection';

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

const on: RepeatUntil = {
  condition: 'response.body.done',
  intervalMs: 1500,
  maxAttempts: 10,
  timeoutMs: 30000,
};

describe('RepeatUntilSection', () => {
  it('hides the fields while off and turns on with the defaults', async () => {
    const onChange = vi.fn();
    render(<RepeatUntilSection value={null} onChange={onChange} />);
    expect(screen.queryByLabelText('Repeat condition')).toBeNull();
    await userEvent.click(screen.getByRole('switch', { name: 'Repeat until' }));
    expect(onChange).toHaveBeenCalledWith(DEFAULT_REPEAT_UNTIL);
  });

  it('shows the settings in seconds', () => {
    render(<RepeatUntilSection value={on} onChange={vi.fn()} />);
    expect(screen.getByLabelText('Repeat condition')).toHaveValue('response.body.done');
    expect(screen.getByLabelText('Interval (s)')).toHaveValue(1.5);
    expect(screen.getByLabelText('Max attempts')).toHaveValue(10);
    expect(screen.getByLabelText('Timeout (s)')).toHaveValue(30);
  });

  it('edits the condition', () => {
    const onChange = vi.fn();
    render(<RepeatUntilSection value={on} onChange={onChange} />);
    fireEvent.change(screen.getByLabelText('Repeat condition'), {
      target: { value: 'response.status === 200' },
    });
    expect(onChange).toHaveBeenCalledWith({ ...on, condition: 'response.status === 200' });
  });

  it('stores seconds as milliseconds', () => {
    const onChange = vi.fn();
    render(<RepeatUntilSection value={on} onChange={onChange} />);
    fireEvent.change(screen.getByLabelText('Interval (s)'), { target: { value: '0.5' } });
    expect(onChange).toHaveBeenLastCalledWith({ ...on, intervalMs: 500 });
    fireEvent.change(screen.getByLabelText('Timeout (s)'), { target: { value: '90' } });
    expect(onChange).toHaveBeenLastCalledWith({ ...on, timeoutMs: 90000 });
    fireEvent.change(screen.getByLabelText('Max attempts'), { target: { value: '4' } });
    expect(onChange).toHaveBeenLastCalledWith({ ...on, maxAttempts: 4 });
  });

  it('ignores an empty interval while typing', () => {
    const onChange = vi.fn();
    render(<RepeatUntilSection value={on} onChange={onChange} />);
    fireEvent.change(screen.getByLabelText('Interval (s)'), { target: { value: '' } });
    expect(onChange).not.toHaveBeenCalled();
  });

  it('turns off to null and back on to the defaults', async () => {
    const onChange = vi.fn();
    const { rerender } = render(<RepeatUntilSection value={on} onChange={onChange} />);
    await userEvent.click(screen.getByRole('switch', { name: 'Repeat until' }));
    expect(onChange).toHaveBeenLastCalledWith(null);
    rerender(<RepeatUntilSection value={null} onChange={onChange} />);
    await userEvent.click(screen.getByRole('switch', { name: 'Repeat until' }));
    expect(onChange).toHaveBeenLastCalledWith(DEFAULT_REPEAT_UNTIL);
  });
});
