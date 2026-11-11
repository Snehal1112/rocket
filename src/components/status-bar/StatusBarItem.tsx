import type { ComponentProps } from 'react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

type StatusBarItemProps = Omit<ComponentProps<typeof Button>, 'variant' | 'size' | 'asChild'> & {
  /** Border sides of the frame. Use `border-r` when a neighbour already draws the left line. */
  frameBorder?: 'border-x' | 'border-r';
};

/** A status bar cell: a bordered frame around a small ghost button. */
export function StatusBarItem({
  className,
  frameBorder = 'border-x',
  ...props
}: StatusBarItemProps) {
  return (
    <div
      className={cn('flex h-5 items-center border-statusbar-border px-1 [&+&]:-ml-px', frameBorder)}
    >
      <Button
        variant='ghost'
        size='sm'
        className={cn(
          'h-5 gap-1.25 rounded-sm px-1 text-[11px] text-muted-foreground transition-colors hover:bg-statusbar-item-hover hover:text-foreground',
          className,
        )}
        {...props}
      />
    </div>
  );
}
