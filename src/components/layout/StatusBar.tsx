import { getVersion } from '@tauri-apps/api/app';
import { Moon, Sun, Terminal } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { useTheme } from '@/hooks/useTheme';
import { cn } from '@/lib/utils';
import { useConsoleStore } from '@/stores/console-store';

interface StatusBarProps {
  isConsoleOpen?: boolean;
  onConsoleToggle?: () => void;
}

export function StatusBar({ isConsoleOpen, onConsoleToggle }: StatusBarProps) {
  const entryCount = useConsoleStore((s) => s.entries.length);
  const { isDark, toggleTheme } = useTheme();
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    getVersion().then(setVersion);
  }, []);

  return (
    <div className='h-7 border-t border-border bg-card px-2 flex items-center gap-1.5 shrink-0'>
      <Button
        variant='ghost'
        size='icon'
        onClick={toggleTheme}
        className='h-5 w-5'
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
        className={cn('h-5 px-1.5 text-xs gap-1', isConsoleOpen && 'bg-accent')}
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
      {version && <span className='ml-auto text-2xs text-muted-foreground'>{`v${version}`}</span>}
    </div>
  );
}
