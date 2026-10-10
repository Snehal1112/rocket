import { ShieldCheck } from 'lucide-react';
import { AgentAutonomyToggle } from '@/components/request/AgentAutonomyToggle';
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { useCollections } from '@/lib/queries/collection-queries';

/** Lists the workspace's collections, each with its run switch. */
export function AssistantPermissionsPopover() {
  const { data: collections = [] } = useCollections();

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7'
          aria-label='Agent permissions'
          title='Agent permissions'
        >
          <ShieldCheck className='h-4 w-4' aria-hidden='true' />
        </Button>
      </PopoverTrigger>
      <PopoverContent align='end' className='w-80 p-3'>
        <div className='flex flex-col gap-3'>
          <div>
            <p className='text-sm font-medium'>Agent permissions</p>
            <p className='text-xs text-muted-foreground'>
              The assistant can read every collection in this workspace and propose changes. Running
              requests needs the switch below, confirmed on this computer.
            </p>
          </div>
          {collections.length === 0 ? (
            <p className='text-xs text-muted-foreground'>No collections in this workspace.</p>
          ) : (
            <div className='flex max-h-72 flex-col gap-2 overflow-y-auto pr-0.5'>
              {collections.map((c) => (
                <section
                  key={c.name}
                  aria-label={c.name}
                  className='flex flex-col gap-1.5 rounded-md border p-2'
                >
                  <span className='text-xs font-medium'>{c.name}</span>
                  <AgentAutonomyToggle collectionName={c.name} showHint={false} />
                </section>
              ))}
            </div>
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}
