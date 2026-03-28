import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { cn } from '@/lib/utils';
import { useSandboxStore } from '@/stores/sandbox-store';
import { ShieldCheck, Code } from 'lucide-react';

export function SandboxPopover() {
  const mode = useSandboxStore((s) => s.mode);
  const setMode = useSandboxStore((s) => s.setMode);

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7"
          title="JavaScript Sandbox"
        >
          <ShieldCheck className={cn('h-4 w-4', mode === 'safe' ? 'text-green-500' : 'text-amber-500')} />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-80 p-3" align="end">
        <p className="text-sm font-semibold mb-3">JavaScript Sandbox</p>

        {/* Safe Mode option */}
        <button
          type="button"
          onClick={() => setMode('safe')}
          className={cn(
            'w-full rounded-lg border p-3 text-left transition-colors mb-2',
            mode === 'safe'
              ? 'border-green-500 bg-green-500/5'
              : 'border-border hover:border-green-500/50',
          )}
        >
          <div className="flex items-center gap-2 mb-1">
            <div className={cn(
              'w-4 h-4 rounded-full border-2 flex items-center justify-center',
              mode === 'safe' ? 'border-green-500' : 'border-muted-foreground/40',
            )}>
              {mode === 'safe' && <div className="w-2 h-2 rounded-full bg-green-500" />}
            </div>
            <ShieldCheck className="h-4 w-4 text-green-500" />
            <span className="text-sm font-medium">Safe Mode</span>
            <span className="text-[10px] font-medium text-green-600 bg-green-500/10 px-1.5 py-0.5 rounded">
              Recommended
            </span>
          </div>
          <p className="text-xs text-muted-foreground pl-6">
            JavaScript code is executed in a secure sandbox and cannot access your filesystem or execute system commands.
          </p>
        </button>

        {/* Developer Mode option */}
        <button
          type="button"
          onClick={() => setMode('developer')}
          className={cn(
            'w-full rounded-lg border p-3 text-left transition-colors',
            mode === 'developer'
              ? 'border-amber-500 bg-amber-500/5'
              : 'border-border hover:border-amber-500/50',
          )}
        >
          <div className="flex items-center gap-2 mb-1">
            <div className={cn(
              'w-4 h-4 rounded-full border-2 flex items-center justify-center',
              mode === 'developer' ? 'border-amber-500' : 'border-muted-foreground/40',
            )}>
              {mode === 'developer' && <div className="w-2 h-2 rounded-full bg-amber-500" />}
            </div>
            <Code className="h-4 w-4 text-amber-500" />
            <span className="text-sm font-medium">Developer Mode</span>
          </div>
          <p className="text-[10px] text-amber-600 bg-amber-500/10 px-1.5 py-0.5 rounded inline-block mb-1 ml-6">
            Use only if you trust the authors of the collection
          </p>
          <p className="text-xs text-muted-foreground pl-6">
            JavaScript code has access to the filesystem, can execute system commands and access sensitive information.
          </p>
        </button>
      </PopoverContent>
    </Popover>
  );
}
