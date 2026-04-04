import { getCurrentWindow } from '@tauri-apps/api/window';
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
        className='h-11 w-12 rounded-none text-muted-foreground hover:bg-accent hover:text-accent-foreground'
        onClick={() => win.minimize()}
        aria-label='Minimize'
      >
        <span className='text-xs' aria-hidden='true'>
          ─
        </span>
      </Button>
      <Button
        variant='ghost'
        size='icon'
        className='h-11 w-12 rounded-none text-muted-foreground hover:bg-accent hover:text-accent-foreground'
        onClick={() => win.toggleMaximize()}
        aria-label={isMaximized ? 'Restore' : 'Maximize'}
      >
        <span className='text-xs' aria-hidden='true'>
          {isMaximized ? '❐' : '▢'}
        </span>
      </Button>
      <Button
        variant='ghost'
        size='icon'
        className='h-11 w-12 rounded-none text-muted-foreground hover:bg-destructive hover:text-destructive-foreground'
        onClick={() => win.close()}
        aria-label='Close'
      >
        <span className='text-xs' aria-hidden='true'>
          ✕
        </span>
      </Button>
    </div>
  );
}
