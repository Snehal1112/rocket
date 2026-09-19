import { getVersion } from '@tauri-apps/api/app';
import { Moon, PanelBottom, PanelRight, Sun, Terminal } from 'lucide-react';
import { useEffect, useState } from 'react';
import { ContractsStatusItem } from '@/components/status-bar/ContractsStatusItem';
import { Button } from '@/components/ui/button';
import { useTheme } from '@/hooks/useTheme';
import { cn } from '@/lib/utils';
import { useConsoleStore } from '@/stores/console-store';
import { useLayoutStore } from '@/stores/layout-store';

interface StatusBarProps {
  isConsoleOpen?: boolean;
  onConsoleToggle?: () => void;
}

export function StatusBar({ isConsoleOpen, onConsoleToggle }: StatusBarProps) {
  const entryCount = useConsoleStore((s) => s.entries.length);
  const { isDark, toggleTheme } = useTheme();
  const [version, setVersion] = useState<string | null>(null);
  const requestLayout = useLayoutStore((s) => s.requestLayout);
  const setRequestLayout = useLayoutStore((s) => s.setRequestLayout);

  useEffect(() => {
    let disposed = false;

    void getVersion()
      .then((appVersion) => {
        if (!disposed) setVersion(appVersion);
      })
      .catch(() => {
        // Version is supplementary status info; rendering must not depend on it.
      });

    return () => {
      disposed = true;
    };
  }, []);

  return (
    <div className='flex h-6 shrink-0 items-center gap-0.5 border-t border-statusbar-border bg-statusbar-bg px-1 text-[11px]'>
      <Button
        variant='ghost'
        size='icon'
        onClick={toggleTheme}
        className='h-5 w-5 rounded-sm hover:bg-statusbar-item-hover'
        title={isDark ? 'Switch to light mode' : 'Switch to dark mode'}
        aria-label={isDark ? 'Switch to light mode' : 'Switch to dark mode'}
      >
        {isDark ? (
          <Sun className='h-3.5 w-3.5 text-muted-foreground' />
        ) : (
          <Moon className='h-3.5 w-3.5 text-muted-foreground' />
        )}
      </Button>
      <Button
        variant='ghost'
        size='sm'
        className={cn(
          'h-5 gap-1 rounded-sm px-1.5 text-[11px] hover:bg-statusbar-item-hover',
          isConsoleOpen && 'bg-statusbar-item-active',
        )}
        onClick={onConsoleToggle}
        disabled={!onConsoleToggle}
        aria-label='Toggle Console'
      >
        <Terminal className='h-3.5 w-3.5 text-muted-foreground' />
        Console
        {entryCount > 0 && (
          <span className='text-2xs px-1 rounded-full bg-muted text-muted-foreground'>
            {entryCount}
          </span>
        )}
      </Button>
      <ContractsStatusItem />
      <Button
        variant='ghost'
        size='sm'
        className={cn(
          'ml-auto h-5 gap-1 rounded-sm px-1.5 text-[11px] hover:bg-statusbar-item-hover',
          requestLayout === 'side-by-side' && 'bg-statusbar-item-active',
        )}
        onClick={() => setRequestLayout(requestLayout === 'stacked' ? 'side-by-side' : 'stacked')}
        title={requestLayout === 'stacked' ? 'Switch to side by side' : 'Switch to stacked'}
        aria-label={requestLayout === 'stacked' ? 'Side by side' : 'Stack'}
      >
        {requestLayout === 'stacked' ? (
          <PanelBottom className='h-3.5 w-3.5 text-muted-foreground' />
        ) : (
          <PanelRight className='h-3.5 w-3.5 text-muted-foreground' />
        )}
        {requestLayout === 'stacked' ? 'Side by side' : 'Stack'}
      </Button>
      {version && <span className='px-1 text-muted-foreground'>{`v${version}`}</span>}
    </div>
  );
}
