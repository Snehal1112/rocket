import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { DurationChip, formatDuration } from '../DurationChip';

describe('formatDuration', () => {
  it('uses milliseconds under a second and seconds from a second', () => {
    expect(formatDuration(0)).toBe('0ms');
    expect(formatDuration(999)).toBe('999ms');
    expect(formatDuration(1000)).toBe('1s');
    expect(formatDuration(1500)).toBe('1.5s');
  });
});

describe('DurationChip', () => {
  it('renders nothing without a duration', () => {
    const { container } = render(<DurationChip />);
    expect(container).toBeEmptyDOMElement();
  });

  it('shows the duration', () => {
    render(<DurationChip durationMs={42} />);
    expect(screen.getByTestId('duration-chip')).toHaveTextContent('42ms');
  });
});
