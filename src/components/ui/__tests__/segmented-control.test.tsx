import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { SegmentedControl } from '@/components/ui/segmented-control';

type Source = 'vault' | 'env' | 'keychain';

const OPTIONS: { value: Source; label: string }[] = [
  { value: 'vault', label: 'Secret manager' },
  { value: 'env', label: 'Environment' },
  { value: 'keychain', label: 'Keychain' },
];

function Controlled({ onValue }: { onValue?: (value: Source) => void }) {
  const [value, setValue] = useState<Source>('vault');
  return (
    <SegmentedControl<Source>
      aria-label='Credential source'
      value={value}
      options={OPTIONS}
      onValueChange={(next) => {
        setValue(next);
        onValue?.(next);
      }}
    />
  );
}

describe('SegmentedControl', () => {
  it('renders a labelled radio group with the selected option checked', () => {
    render(<Controlled />);
    expect(screen.getByRole('radiogroup', { name: 'Credential source' })).toBeInTheDocument();
    expect(screen.getAllByRole('radio')).toHaveLength(3);
    expect(screen.getByRole('radio', { name: 'Secret manager' })).toBeChecked();
    expect(screen.getByRole('radio', { name: 'Keychain' })).not.toBeChecked();
  });

  it('selects an option on click', async () => {
    const onValue = vi.fn();
    render(<Controlled onValue={onValue} />);
    await userEvent.click(screen.getByRole('radio', { name: 'Environment' }));
    expect(onValue).toHaveBeenLastCalledWith('env');
    expect(screen.getByRole('radio', { name: 'Environment' })).toBeChecked();
  });

  // Radix moves focus on a timer and selects only while the arrow key is held, so hold it.
  async function press(
    onValue: (value: Source) => void,
    key: string,
    expectedValue: Source,
    expectedLabel: string,
  ) {
    await userEvent.keyboard(`{${key}>}`);
    await waitFor(() => {
      expect(onValue).toHaveBeenLastCalledWith(expectedValue);
      expect(screen.getByRole('radio', { name: expectedLabel })).toBeChecked();
      expect(screen.getByRole('radio', { name: expectedLabel })).toHaveFocus();
    });
    await userEvent.keyboard(`{/${key}}`);
  }

  it('moves selection with arrow keys and wraps', async () => {
    const onValue = vi.fn();
    render(<Controlled onValue={onValue} />);
    screen.getByRole('radio', { name: 'Secret manager' }).focus();

    await press(onValue, 'ArrowRight', 'env', 'Environment');
    await press(onValue, 'ArrowRight', 'keychain', 'Keychain');
    await press(onValue, 'ArrowRight', 'vault', 'Secret manager');
    await press(onValue, 'ArrowLeft', 'keychain', 'Keychain');
  });

  it('styles the track and the selected segment with tokens', () => {
    render(<Controlled />);
    expect(screen.getByRole('radiogroup')).toHaveClass('bg-muted', 'rounded-md');
    expect(screen.getByRole('radio', { name: 'Secret manager' })).toHaveClass(
      'data-[state=checked]:bg-background',
      'data-[state=checked]:text-foreground',
    );
  });

  it('disables the colour transition for reduced motion', () => {
    render(<Controlled />);
    expect(screen.getByRole('radio', { name: 'Environment' })).toHaveClass(
      'transition-colors',
      'motion-reduce:transition-none',
    );
  });
});
