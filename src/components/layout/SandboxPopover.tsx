import { Lock, ShieldCheck, Unlock } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { cn } from '@/lib/utils';
import { useSandboxStore } from '@/stores/sandbox-store';

export function SandboxPopover() {
  const mode = useSandboxStore((s) => s.mode);
  const setMode = useSandboxStore((s) => s.setMode);

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7'
          title='JavaScript Sandbox'
          aria-label='JavaScript Sandbox'
        >
          <ShieldCheck
            fill='currentColor'
            className={cn(
              'h-4 w-4 transition-colors duration-200',
              mode === 'safe'
                ? 'text-green-500 dark:text-green-400'
                : 'text-amber-500 dark:text-amber-400',
            )}
          />
        </Button>
      </PopoverTrigger>
      <PopoverContent className='w-68 p-0 overflow-hidden' align='end'>
        {/* Header */}
        <div className='flex items-center gap-2 px-4 py-2.5 border-b border-border/60'>
          <ShieldCheck
            className={cn(
              'h-3 w-3 shrink-0',
              mode === 'safe'
                ? 'text-green-500 dark:text-green-400'
                : 'text-amber-500 dark:text-amber-400',
            )}
          />
          <p className='text-[11px] font-semibold tracking-wider uppercase text-muted-foreground'>
            JavaScript Sandbox
          </p>
        </div>

        {/* Mode options */}
        <div className='p-1.5 space-y-0.5'>
          {/* Safe Mode */}
          <button
            type='button'
            onClick={() => setMode('safe')}
            className={cn(
              'w-full rounded-md p-2.5 text-left transition-all duration-150 group border',
              mode === 'safe'
                ? 'border-green-500/30 dark:border-green-400/20 bg-green-500/5 dark:bg-green-400/5'
                : 'border-transparent hover:border-border hover:bg-accent/50',
            )}
          >
            <div className='flex items-start gap-2.5'>
              <div
                className={cn(
                  'mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded transition-colors',
                  mode === 'safe'
                    ? 'bg-green-500/15 dark:bg-green-400/10'
                    : 'bg-muted group-hover:bg-muted/70',
                )}
              >
                <Lock
                  className={cn(
                    'h-2.5 w-2.5',
                    mode === 'safe'
                      ? 'text-green-500 dark:text-green-400'
                      : 'text-muted-foreground',
                  )}
                />
              </div>
              <div className='flex-1 min-w-0'>
                <div className='flex items-center gap-1.5 mb-0.5'>
                  <span className='text-[13px] font-medium text-foreground'>Safe Mode</span>
                  <span className='text-[9px] font-semibold tracking-wide uppercase text-green-600 dark:text-green-400 bg-green-500/10 dark:bg-green-500/15 px-1.5 py-px rounded-sm'>
                    Default
                  </span>
                </div>
                <p className='text-[11px] leading-relaxed text-muted-foreground'>
                  Sandboxed. No filesystem or system access.
                </p>
              </div>
              {mode === 'safe' && (
                <div className='mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-green-500 dark:bg-green-400' />
              )}
            </div>
          </button>

          {/* Developer Mode */}
          <button
            type='button'
            onClick={() => setMode('developer')}
            className={cn(
              'w-full rounded-md p-2.5 text-left transition-all duration-150 group border',
              mode === 'developer'
                ? 'border-amber-500/30 dark:border-amber-400/20 bg-amber-500/5 dark:bg-amber-400/5'
                : 'border-transparent hover:border-border hover:bg-accent/50',
            )}
          >
            <div className='flex items-start gap-2.5'>
              <div
                className={cn(
                  'mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded transition-colors',
                  mode === 'developer'
                    ? 'bg-amber-500/15 dark:bg-amber-400/10'
                    : 'bg-muted group-hover:bg-muted/70',
                )}
              >
                <Unlock
                  className={cn(
                    'h-2.5 w-2.5',
                    mode === 'developer'
                      ? 'text-amber-500 dark:text-amber-400'
                      : 'text-muted-foreground',
                  )}
                />
              </div>
              <div className='flex-1 min-w-0'>
                <div className='flex items-center gap-1.5 mb-0.5'>
                  <span className='text-[13px] font-medium text-foreground'>Developer Mode</span>
                </div>
                <p className='text-[11px] leading-relaxed text-muted-foreground'>
                  Full filesystem and system command access.
                </p>
              </div>
              {mode === 'developer' && (
                <div className='mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-amber-500 dark:bg-amber-400' />
              )}
            </div>
          </button>
        </div>

        {/* Warning footer — only visible in developer mode */}
        {mode === 'developer' && (
          <div className='mx-1.5 mb-1.5 rounded border border-amber-500/20 dark:border-amber-400/15 bg-amber-500/8 dark:bg-amber-400/8 px-3 py-2'>
            <p className='text-[10px] leading-relaxed text-amber-600 dark:text-amber-400'>
              Only enable for collections from trusted authors.
            </p>
          </div>
        )}
      </PopoverContent>
    </Popover>
  );
}
