import { Check } from 'lucide-react';
import { useEffect, useState } from 'react';

/**
 * Appears for 3 s then hides itself. Re-mount with a new key to restart the timer.
 * Usage: {savedAt && <SavedPill key={savedAt} />}
 */
export function SavedPill() {
  const [visible, setVisible] = useState(true);

  useEffect(() => {
    const id = setTimeout(() => setVisible(false), 3000);
    return () => clearTimeout(id);
  }, []);

  if (!visible) return null;

  return (
    <span className='flex items-center gap-1 text-xs text-muted-foreground'>
      <Check className='h-3 w-3' />
      All changes saved
    </span>
  );
}
