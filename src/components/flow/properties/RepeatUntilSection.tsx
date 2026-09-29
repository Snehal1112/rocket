import { SingleLineEditor } from '@/components/editor';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { DEFAULT_REPEAT_UNTIL } from '@/lib/flow-repeat';
import type { RepeatUntil } from '@/lib/tauri-api';

// Returns the number typed into a field, or null while it is empty or not a number.
function parsed(text: string): number | null {
  if (text.trim() === '') return null;
  const n = Number(text);
  return Number.isFinite(n) ? n : null;
}

export function RepeatUntilSection({
  value,
  onChange,
}: {
  value: RepeatUntil | null;
  onChange: (value: RepeatUntil | null) => void;
}) {
  const setSeconds = (field: 'intervalMs' | 'timeoutMs', text: string) => {
    const seconds = parsed(text);
    if (seconds === null || value === null) return;
    onChange({ ...value, [field]: Math.round(seconds * 1000) });
  };

  return (
    <div className='space-y-2'>
      <div className='flex items-center justify-between gap-2'>
        <Label htmlFor='request-repeat-until'>Repeat until</Label>
        <Switch
          id='request-repeat-until'
          checked={value !== null}
          onCheckedChange={(checked) => onChange(checked ? { ...DEFAULT_REPEAT_UNTIL } : null)}
        />
      </div>
      <p className='text-xs text-muted-foreground'>
        Sends the request again until the condition is true. The node fails if the condition is
        still false after the last attempt or the timeout.
      </p>
      {value !== null && (
        <div className='space-y-2'>
          <div className='space-y-1'>
            <span className='text-xs font-medium'>Condition</span>
            <SingleLineEditor
              aria-label='Repeat condition'
              value={value.condition}
              onChange={(condition) => onChange({ ...value, condition })}
              placeholder='response.status === 200'
              className='text-xs'
            />
          </div>
          <div className='grid grid-cols-3 gap-2'>
            <div className='space-y-1'>
              <Label htmlFor='repeat-interval' className='text-xs'>
                Interval (s)
              </Label>
              <Input
                id='repeat-interval'
                type='number'
                min={0.1}
                step={0.1}
                value={value.intervalMs / 1000}
                onChange={(e) => setSeconds('intervalMs', e.target.value)}
                className='h-8 text-xs'
              />
            </div>
            <div className='space-y-1'>
              <Label htmlFor='repeat-max-attempts' className='text-xs'>
                Max attempts
              </Label>
              <Input
                id='repeat-max-attempts'
                type='number'
                min={1}
                max={1000}
                step={1}
                value={value.maxAttempts}
                onChange={(e) => {
                  const n = parsed(e.target.value);
                  if (n !== null) onChange({ ...value, maxAttempts: Math.round(n) });
                }}
                className='h-8 text-xs'
              />
            </div>
            <div className='space-y-1'>
              <Label htmlFor='repeat-timeout' className='text-xs'>
                Timeout (s)
              </Label>
              <Input
                id='repeat-timeout'
                type='number'
                min={1}
                max={3600}
                step={1}
                value={value.timeoutMs / 1000}
                onChange={(e) => setSeconds('timeoutMs', e.target.value)}
                className='h-8 text-xs'
              />
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
