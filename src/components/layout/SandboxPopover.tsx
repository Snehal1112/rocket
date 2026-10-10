import { Lock, ShieldCheck, Unlock } from 'lucide-react';
import { useState } from 'react';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { useCollectionTrust, useSetCapability } from '@/lib/queries/collection-trust-queries';
import type { SandboxMode } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { usePaneStore } from '@/stores/pane-store';

export function SandboxPopover() {
  const activeCollection = usePaneStore((s) => s.activeCollection);
  const { data: trust, isError: loadFailed } = useCollectionTrust(activeCollection);
  const setCapability = useSetCapability(activeCollection ?? '');
  const [saveError, setSaveError] = useState<string | null>(null);
  const error = loadFailed ? 'Failed to load sandbox mode.' : saveError;
  // The mode that really applies. The collection file only requests one.
  const mode: SandboxMode = trust?.developerMode.effective ? 'developer' : 'safe';
  const requestedNotAllowed =
    !!trust && trust.developerMode.requested && !trust.developerMode.granted;
  // Neutral color when the real mode couldn't be confirmed — never show a confident
  // green "Safe" indicator when the load actually failed.
  const statusColorClass = error
    ? 'text-muted-foreground'
    : mode === 'safe'
      ? 'text-green-500 dark:text-green-400'
      : 'text-amber-500 dark:text-amber-400';

  const [showDevConfirm, setShowDevConfirm] = useState(false);

  // Developer mode is a capability the user allows on this computer. The backend records
  // the grant and updates the collection file, so a plain settings save is not used.
  async function setMode(nextMode: SandboxMode) {
    if (!activeCollection) return;
    try {
      await setCapability.mutateAsync({
        capability: 'developerMode',
        enabled: nextMode === 'developer',
      });
      setSaveError(null);
      if (nextMode === 'developer') setShowDevConfirm(false);
    } catch (err) {
      console.error('[SandboxPopover] save failed', err);
      setSaveError('Failed to save sandbox mode.');
      // Leave the confirm dialog open. The caller must not treat this as a mode switch.
    }
  }

  const selectSafeMode = () => setMode('safe');
  const confirmDeveloperMode = () => setMode('developer');

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7 hover:bg-toolbar-hover'
          title='JavaScript Sandbox'
          aria-label='JavaScript Sandbox'
          disabled={!activeCollection}
        >
          <ShieldCheck
            fill='currentColor'
            className={cn('h-4 w-4 transition-colors duration-200', statusColorClass)}
          />
        </Button>
      </PopoverTrigger>
      <PopoverContent className='w-68 p-0 overflow-hidden' align='end'>
        {!activeCollection ? (
          <div className='px-4 py-3 text-[11px] text-muted-foreground'>
            Open a collection to configure its sandbox mode.
          </div>
        ) : (
          <>
            {/* Header */}
            <div className='flex items-center gap-2 px-4 py-2.5 border-b border-border/60'>
              <ShieldCheck className={cn('h-3 w-3 shrink-0', statusColorClass)} />
              <p className='text-[11px] font-semibold tracking-wider uppercase text-muted-foreground'>
                JavaScript Sandbox
              </p>
            </div>

            {error ? (
              <div className='px-4 py-3 text-[11px] text-destructive'>{error}</div>
            ) : (
              <>
                {/* Mode options */}
                <div className='p-1.5 space-y-0.5'>
                  {/* Safe Mode */}
                  <Button
                    type='button'
                    variant='ghost'
                    onClick={() => void selectSafeMode()}
                    className={cn(
                      'h-auto w-full justify-start whitespace-normal rounded-md p-2.5 text-left font-normal transition-all duration-150 group border',
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
                  </Button>

                  {/* Developer Mode */}
                  <Button
                    type='button'
                    variant='ghost'
                    onClick={() => setShowDevConfirm(true)}
                    className={cn(
                      'h-auto w-full justify-start whitespace-normal rounded-md p-2.5 text-left font-normal transition-all duration-150 group border',
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
                          <span className='text-[13px] font-medium text-foreground'>
                            Developer Mode
                          </span>
                        </div>
                        <p className='text-[11px] leading-relaxed text-muted-foreground'>
                          Full filesystem and system command access.
                        </p>
                      </div>
                      {mode === 'developer' && (
                        <div className='mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-amber-500 dark:bg-amber-400' />
                      )}
                    </div>
                  </Button>
                </div>

                {requestedNotAllowed && (
                  <div className='mx-1.5 mb-1.5 flex flex-col gap-2 rounded border border-amber-500/20 px-3 py-2'>
                    <Badge variant='warning' className='w-fit'>
                      Requested by this collection
                    </Badge>
                    <p className='text-[11px] leading-relaxed text-muted-foreground'>
                      This collection asks for Developer mode. It runs in Safe mode until you allow
                      it on this computer.
                    </p>
                    <Button size='sm' variant='outline' onClick={() => setShowDevConfirm(true)}>
                      Allow on this computer...
                    </Button>
                  </div>
                )}

                {/* Warning footer — only visible in developer mode */}
                {mode === 'developer' && (
                  <div className='mx-1.5 mb-1.5 rounded border border-amber-500/20 dark:border-amber-400/15 bg-amber-500/8 dark:bg-amber-400/8 px-3 py-2'>
                    <p className='text-[10px] leading-relaxed text-amber-600 dark:text-amber-400'>
                      Only enable for collections from trusted authors.
                    </p>
                  </div>
                )}
              </>
            )}
          </>
        )}
      </PopoverContent>
      <AlertDialog open={showDevConfirm} onOpenChange={setShowDevConfirm}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Enable Developer Mode?</AlertDialogTitle>
            <AlertDialogDescription>
              Scripts in this collection will get real filesystem read/write and command execution
              access on your machine — no restrictions on which files or commands. Only enable this
              for a collection you wrote yourself or trust completely; an imported collection can
              carry a script that does anything a normal program on your machine could do.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={(event) => {
                // AlertDialogAction closes the dialog unconditionally by default
                // (Radix). Prevent that here so a failed save leaves the dialog
                // open instead of silently dismissing as if it succeeded —
                // setMode() closes it explicitly on success.
                event.preventDefault();
                void confirmDeveloperMode();
              }}
            >
              Enable
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Popover>
  );
}
