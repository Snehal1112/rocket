import type { ReactNode } from 'react';

/** The small round number next to a status bar label. */
export function StatusBarCount({ children }: { children: ReactNode }) {
  return (
    <span className='text-2xs px-1 rounded-full bg-muted text-muted-foreground'>{children}</span>
  );
}
