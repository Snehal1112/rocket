import { Check } from 'lucide-react';
import { type ReactNode, useEffect, useState } from 'react';

/**
 * Appears for 3 s then hides itself. Re-mount with a new key to restart the timer.
 * Pass `persistent` to keep it visible, and `children` to change the label.
 * Usage: {savedAt && <SavedPill key={savedAt} />}
 */
export function SavedPill({
  persistent = false,
  children = 'All changes saved',
}: {
  persistent?: boolean;
  children?: ReactNode;
}) {
  const [visible, setVisible] = useState(true);

  useEffect(() => {
    if (persistent) return undefined;
    const id = setTimeout(() => setVisible(false), 3000);
    return () => clearTimeout(id);
  }, [persistent]);

  if (!visible) return null;

  return (
    <span className='flex items-center gap-1 text-xs text-muted-foreground'>
      <Check className='h-3 w-3' />
      {children}
    </span>
  );
}
