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
  async function press(key: string, assertion: () => void) {
    await userEvent.keyboard(`{${key}>}`);
    await waitFor(assertion);
    await userEvent.keyboard(`{/${key}}`);
  }

  it('moves selection with arrow keys and wraps', async () => {
    const onValue = vi.fn();
    render(<Controlled onValue={onValue} />);
    screen.getByRole('radio', { name: 'Secret manager' }).focus();

    await press('ArrowRight', () => () => expect(onValue).toHaveBeenLastCalledWith('env'));
    await press('ArrowRight', () => () => expect(onValue).toHaveBeenLastCalledWith('keychain'));
    await press('ArrowRight', () => () => expect(onValue).toHaveBeenLastCalledWith('vault'));
    await press('ArrowLeft', () => () => expect(onValue).toHaveBeenLastCalledWith('keychain'));
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
