import { cva, type VariantProps } from 'class-variance-authority';
import type * as React from 'react';
import { cn } from '@/lib/utils';

const alertVariants = cva('relative w-full rounded-md border px-3 py-2 text-xs flex items-start gap-2', {
  variants: {
    variant: {
      default: 'bg-card text-card-foreground border-border',
      destructive: 'border-destructive/30 bg-destructive/10 text-destructive',
    },
  },
  defaultVariants: { variant: 'default' },
});

function Alert({
  className,
  variant,
  ...props
}: React.ComponentProps<'div'> & VariantProps<typeof alertVariants>) {
  return (
    <div
      data-slot='alert'
      role='alert'
      className={cn(alertVariants({ variant, className }))}
      {...props}
    />
  );
}

export { Alert, alertVariants };
