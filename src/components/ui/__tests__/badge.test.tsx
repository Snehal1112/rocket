import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { Badge, badgeVariants } from '@/components/ui/badge';

describe('Badge', () => {
  it('has a success variant', () => {
    render(<Badge variant='success'>Online</Badge>);
    expect(screen.getByText('Online')).toHaveClass(
      'bg-[hsl(var(--success-soft))]',
      'text-[hsl(var(--toast-success-fg))]',
    );
  });

  it('keeps the other variants', () => {
    expect(badgeVariants()).toContain('bg-badge-bg');
    expect(badgeVariants({ variant: 'warning' })).toContain('bg-warning');
    expect(badgeVariants({ variant: 'destructive' })).toContain('bg-destructive-solid');
    expect(badgeVariants({ variant: 'outline' })).toContain('text-foreground');
  });
});
