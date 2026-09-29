import { Network } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';

// The host a flow's callback URLs use. Empty means auto-detect the LAN IP.
export function CallbackHostSetting({
  value,
  onChange,
}: {
  value: string | null | undefined;
  onChange: (host: string | null) => void;
}) {
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button size='sm' variant='outline' aria-label='Callback host' title='Callback host'>
          <Network className='h-3.5 w-3.5' aria-hidden='true' />
        </Button>
      </PopoverTrigger>
      <PopoverContent align='end' className='nokey w-72 space-y-2'>
        <Label htmlFor='flow-callback-host' className='text-xs'>
          Callback host
        </Label>
        <Input
          id='flow-callback-host'
          value={value ?? ''}
          placeholder='auto (LAN IP)'
          onChange={(e) => {
            const host = e.target.value.trim();
            onChange(host ? host : null);
          }}
          className='h-8 text-xs'
        />
        <p className='text-xs text-muted-foreground'>
          Used in <code className='font-mono'>{'{{callback.*}}'}</code> URLs. Use{' '}
          <code className='font-mono'>host.docker.internal</code> when the caller runs in Docker.
        </p>
      </PopoverContent>
    </Popover>
  );
}
