import { fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { NumberInput } from '@/components/ui/number-input';

function Controlled({ onValue }: { onValue: (value: string) => void }) {
  const [value, setValue] = useState('5');
  return (
    <NumberInput
      aria-label='count'
      min={0}
      max={6}
      value={value}
      onChange={(e) => {
        setValue(e.target.value);
        onValue(e.target.value);
      }}
    />
  );
}

describe('NumberInput', () => {
  it('steps through the caller onChange and respects max and min', () => {
    const onValue = vi.fn();
    render(<Controlled onValue={onValue} />);
    const input = screen.getByLabelText('count') as HTMLInputElement;

    fireEvent.click(screen.getByLabelText('Increment'));
    expect(input.value).toBe('6');
    expect(onValue).toHaveBeenLastCalledWith('6');

    // Stepping past max leaves the value unchanged.
    fireEvent.click(screen.getByLabelText('Increment'));
    expect(input.value).toBe('6');

    fireEvent.click(screen.getByLabelText('Decrement'));
    expect(input.value).toBe('5');
    expect(onValue).toHaveBeenLastCalledWith('5');
  });

  it('disables the stepper with the input', () => {
    render(<NumberInput aria-label='count' disabled defaultValue='1' />);
    expect(screen.getByLabelText('Increment')).toBeDisabled();
    expect(screen.getByLabelText('Decrement')).toBeDisabled();
  });
});
