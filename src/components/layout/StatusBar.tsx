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
    getVersion().then(setVersion);
  }, []);

  return (
    <div className='h-7 border-t border-statusbar-border bg-statusbar-bg px-2 flex items-center gap-1.5 shrink-0'>
      <Button
        variant='ghost'
        size='icon'
        onClick={toggleTheme}
        className='h-5 w-5 hover:bg-statusbar-item-hover'
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
          'h-5 px-1.5 text-xs gap-1 hover:bg-statusbar-item-hover',
          isConsoleOpen && 'bg-statusbar-item-active',
        )}
        onClick={onConsoleToggle}
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
          'h-5 px-1.5 text-xs gap-1 ml-auto hover:bg-statusbar-item-hover',
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
      {version && <span className='text-2xs text-muted-foreground'>{`v${version}`}</span>}
    </div>
  );
}
