import type { ReactNode } from 'react';

/** A one-line note inside an open collection or folder that has nothing to list. */
export function TreeHint({ children }: { children: ReactNode }) {
  return (
    <p role='status' className='px-2 py-1 text-xs text-muted-foreground'>
      {children}
    </p>
  );
}
