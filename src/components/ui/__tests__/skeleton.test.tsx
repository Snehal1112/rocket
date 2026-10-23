import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Skeleton, SkeletonRegion } from '@/components/ui/skeleton';

describe('Skeleton', () => {
  it('is hidden from assistive tech and pulses only when motion is allowed', () => {
    render(<Skeleton data-testid='bar' className='h-3 w-10' />);
    const bar = screen.getByTestId('bar');
    expect(bar).toHaveAttribute('aria-hidden', 'true');
    expect(bar).toHaveClass('motion-safe:animate-pulse', 'bg-muted', 'h-3', 'w-10');
    expect(bar).not.toHaveClass('animate-pulse');
  });
});

describe('SkeletonRegion', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('announces what is loading right away', () => {
    render(
      <SkeletonRegion label='Loading collections'>
        <Skeleton data-testid='bar' />
      </SkeletonRegion>,
    );
    const region = screen.getByRole('status');
    expect(region).toHaveTextContent('Loading collections');
    expect(region).toHaveAttribute('aria-busy', 'true');
  });

  it('shows the shapes only after the delay, so a fast load never flashes', () => {
    render(
      <SkeletonRegion label='Loading' delayMs={150}>
        <Skeleton data-testid='bar' />
      </SkeletonRegion>,
    );
    expect(screen.queryByTestId('bar')).toBeNull();
    act(() => {
      vi.advanceTimersByTime(149);
    });
    expect(screen.queryByTestId('bar')).toBeNull();
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(screen.getByTestId('bar')).toBeInTheDocument();
  });

  it('does not show the shapes when it unmounts before the delay', () => {
    const { unmount } = render(
      <SkeletonRegion label='Loading'>
        <Skeleton data-testid='bar' />
      </SkeletonRegion>,
    );
    unmount();
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(screen.queryByTestId('bar')).toBeNull();
  });
});
