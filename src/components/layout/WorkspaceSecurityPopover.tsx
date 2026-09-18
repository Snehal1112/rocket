import { ShieldAlert } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Switch } from '@/components/ui/switch';
import { useUpdateRequestGuardPolicy, useWorkspaceConfig } from '@/lib/queries/workspace-queries';
import { cn } from '@/lib/utils';
import { useWorkspaceStore } from '@/stores/workspace-store';

export function WorkspaceSecurityPopover() {
  const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const { data: config } = useWorkspaceConfig(activeWorkspaceId);
  const updatePolicy = useUpdateRequestGuardPolicy(activeWorkspaceId);

  const blockInternal = config?.requestGuardPolicy?.blockScriptRedirectsToInternalHosts ?? false;
  const blockPrivate = config?.requestGuardPolicy?.alsoBlockPrivateRanges ?? false;

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7 hover:bg-toolbar-hover'
          title='Request Guard'
          aria-label='Request Guard'
        >
          <ShieldAlert
            className={cn(
              'h-4 w-4 transition-colors duration-200',
              blockInternal ? 'text-green-500 dark:text-green-400' : 'text-muted-foreground',
            )}
          />
        </Button>
      </PopoverTrigger>
      <PopoverContent className='w-72 p-0 overflow-hidden' align='end'>
        {/* Header */}
        <div className='flex items-center gap-2 px-4 py-2.5 border-b border-border/60'>
          <ShieldAlert className='h-3 w-3 shrink-0 text-muted-foreground' />
          <p className='text-[11px] font-semibold tracking-wider uppercase text-muted-foreground'>
            Request Guard
          </p>
        </div>

        {/* Toggles */}
        <div className='p-3 space-y-3'>
          <p className='text-xs text-muted-foreground'>
            Block a BeforeRequest script's <code>req.setUrl()</code> redirect from reaching internal
            hosts. Your own manually-typed URLs are never affected.
          </p>

          <div className='flex items-start justify-between gap-3'>
            <div className='flex-1 min-w-0'>
              <p className='text-xs font-medium'>Block internal redirects</p>
              <p className='text-[11px] text-muted-foreground'>
                Loopback, link-local, and the cloud metadata endpoint.
              </p>
            </div>
            <Switch
              checked={blockInternal}
              disabled={!activeWorkspaceId || updatePolicy.isPending}
              onCheckedChange={(checked) => {
                updatePolicy.mutate({
                  blockScriptRedirectsToInternalHosts: checked,
                  alsoBlockPrivateRanges: checked ? blockPrivate : false,
                });
              }}
              aria-label='Block script redirects to internal hosts'
            />
          </div>

          <div className='flex items-start justify-between gap-3'>
            <div className='flex-1 min-w-0'>
              <p className='text-xs font-medium'>Also block private ranges</p>
              <p className='text-[11px] text-muted-foreground'>
                10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16.
              </p>
            </div>
            <Switch
              checked={blockPrivate}
              disabled={!activeWorkspaceId || !blockInternal || updatePolicy.isPending}
              onCheckedChange={(checked) => {
                updatePolicy.mutate({
                  blockScriptRedirectsToInternalHosts: blockInternal,
                  alsoBlockPrivateRanges: checked,
                });
              }}
              aria-label='Also block private IP ranges'
            />
          </div>
        </div>
      </PopoverContent>
    </Popover>
  );
}
