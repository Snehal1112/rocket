import type { ComponentProps } from 'react';
import { cn } from '@/lib/utils';

/** A list item that lays its children out in a row. Pass padding and state classes. */
export function ListRow({ className, ...props }: ComponentProps<'li'>) {
  return <li className={cn('flex items-center gap-2', className)} {...props} />;
}
