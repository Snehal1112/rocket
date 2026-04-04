import { Moon, Sun, Terminal } from 'lucide-react';
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

  return (
    <div className='h-7 border-t border-border/70 bg-card/50 backdrop-blur-sm px-2 flex items-center gap-1.5 shrink-0'>
      <Button
        variant='ghost'
        size='icon'
        onClick={toggleTheme}
        className='h-5 w-5'
        title={isDark ? 'Switch to light mode' : 'Switch to dark mode'}
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
    </div>
  );
}
