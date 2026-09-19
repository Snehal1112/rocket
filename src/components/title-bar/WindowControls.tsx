import { getCurrentWindow } from '@tauri-apps/api/window';
import { Copy, Minus, Square, X } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { Button } from '@/components/ui/button';

export function WindowControls() {
  const win = useMemo(() => getCurrentWindow(), []);
  const [isMaximized, setIsMaximized] = useState(false);

  useEffect(() => {
    let cancelled = false;

    win.isMaximized().then((m) => {
      if (!cancelled) setIsMaximized(m);
    });

    const unlisten = win.onResized(() => {
      win.isMaximized().then((m) => {
        if (!cancelled) setIsMaximized(m);
      });
    });

    return () => {
      cancelled = true;
      unlisten.then((fn) => fn());
    };
  }, [win]);

  return (
    <div className='flex items-center'>
      <Button
        variant='ghost'
        size='icon'
        className='h-10 w-12 rounded-none text-muted-foreground hover:bg-accent hover:text-accent-foreground'
        onClick={() => win.minimize()}
        aria-label='Minimize'
      >
        <Minus className='h-3.5 w-3.5' aria-hidden='true' />
      </Button>
      <Button
        variant='ghost'
        size='icon'
        className='h-10 w-12 rounded-none text-muted-foreground hover:bg-accent hover:text-accent-foreground'
        onClick={() => win.toggleMaximize()}
        aria-label={isMaximized ? 'Restore' : 'Maximize'}
      >
        {isMaximized ? (
          <Copy className='h-3.5 w-3.5' aria-hidden='true' />
        ) : (
          <Square className='h-3.5 w-3.5' aria-hidden='true' />
        )}
      </Button>
      <Button
        variant='ghost'
        size='icon'
        className='h-10 w-12 rounded-none text-muted-foreground hover:bg-destructive hover:text-destructive-foreground'
        onClick={() => win.close()}
        aria-label='Close'
      >
        <X className='h-3.5 w-3.5' aria-hidden='true' />
      </Button>
    </div>
  );
}
